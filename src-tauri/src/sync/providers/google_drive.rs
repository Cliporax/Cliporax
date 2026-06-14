//! Google Drive provider backed by the appDataFolder space.

use crate::sync::error::SyncError;
use crate::sync::models::RemoteObject;
use crate::sync::providers::{join_remote_path, SyncProvider};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{multipart, Client, StatusCode};
use serde::Deserialize;
use std::time::Duration;

const DRIVE_FILES: &str = "https://www.googleapis.com/drive/v3/files";
const DRIVE_UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3/files";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct GoogleDriveProvider {
    root_path: String,
    access_token: String,
    client: Client,
}

#[derive(Deserialize)]
struct FileList {
    files: Vec<DriveFile>,
}

#[derive(Deserialize)]
struct DriveFile {
    id: String,
    name: String,
    parents: Option<Vec<String>>,
    size: Option<String>,
    #[serde(rename = "modifiedTime")]
    modified_time: Option<String>,
    #[serde(rename = "md5Checksum")]
    md5_checksum: Option<String>,
}

impl GoogleDriveProvider {
    pub fn new(root_path: &str, access_token: &str) -> Result<Self, SyncError> {
        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| SyncError::provider(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            root_path: normalize_root(root_path),
            access_token: access_token.to_string(),
            client,
        })
    }

    fn request(&self, builder: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        builder.bearer_auth(&self.access_token)
    }

    fn logical_path(&self, path: &str) -> String {
        join_remote_path(&self.root_path, path)
    }

    async fn find_child(
        &self,
        parent_id: &str,
        name: &str,
        folder_only: bool,
    ) -> Result<Option<DriveFile>, SyncError> {
        let mut query = format!(
            "'{}' in parents and name = '{}' and trashed = false",
            escape_query(parent_id),
            escape_query(name)
        );
        if folder_only {
            query.push_str(" and mimeType = 'application/vnd.google-apps.folder'");
        }
        let resp = self
            .request(self.client.get(DRIVE_FILES))
            .query(&[
                ("spaces", "appDataFolder"),
                (
                    "fields",
                    "files(id,name,parents,size,modifiedTime,md5Checksum)",
                ),
                ("pageSize", "1"),
                ("q", query.as_str()),
            ])
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("Find file failed: {}", e)))?;
        if !resp.status().is_success() {
            return Err(provider_status("Find file", resp.status(), resp).await);
        }
        let list: FileList = resp
            .json()
            .await
            .map_err(|e| SyncError::provider(format!("Failed to parse file list: {}", e)))?;
        Ok(list.files.into_iter().next())
    }

    async fn ensure_folder_path(&self, path: &str) -> Result<String, SyncError> {
        let mut parent_id = "appDataFolder".to_string();
        for part in path.split('/').filter(|part| !part.is_empty()) {
            if let Some(folder) = self.find_child(&parent_id, part, true).await? {
                parent_id = folder.id;
                continue;
            }
            parent_id = self.create_folder(&parent_id, part).await?;
        }
        Ok(parent_id)
    }

    async fn folder_id_for_path(&self, path: &str) -> Result<Option<String>, SyncError> {
        let mut parent_id = "appDataFolder".to_string();
        for part in path.split('/').filter(|part| !part.is_empty()) {
            let Some(folder) = self.find_child(&parent_id, part, true).await? else {
                return Ok(None);
            };
            parent_id = folder.id;
        }
        Ok(Some(parent_id))
    }

    async fn file_for_path(&self, path: &str) -> Result<Option<DriveFile>, SyncError> {
        let full = self.logical_path(path);
        let Some((parent, name)) = split_parent_name(&full) else {
            return Ok(None);
        };
        let Some(parent_id) = self.folder_id_for_path(parent).await? else {
            return Ok(None);
        };
        self.find_child(&parent_id, name, false).await
    }

    async fn create_folder(&self, parent_id: &str, name: &str) -> Result<String, SyncError> {
        let body = serde_json::json!({
            "name": name,
            "parents": [parent_id],
            "mimeType": "application/vnd.google-apps.folder",
        });
        let resp = self
            .request(self.client.post(DRIVE_FILES))
            .query(&[("fields", "id")])
            .json(&body)
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("Create folder failed: {}", e)))?;
        if !resp.status().is_success() {
            return Err(provider_status("Create folder", resp.status(), resp).await);
        }
        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SyncError::provider(format!("Failed to parse folder response: {}", e)))?;
        value
            .get("id")
            .and_then(|id| id.as_str())
            .map(|id| id.to_string())
            .ok_or_else(|| SyncError::provider("Google Drive folder response missing id"))
    }

    async fn upload_new(
        &self,
        parent_id: &str,
        name: &str,
        data: Vec<u8>,
    ) -> Result<(), SyncError> {
        let metadata = serde_json::json!({
            "name": name,
            "parents": [parent_id],
        });
        let form = multipart::Form::new()
            .part(
                "metadata",
                multipart::Part::text(metadata.to_string())
                    .mime_str("application/json")
                    .map_err(|e| SyncError::provider(format!("Invalid metadata MIME: {}", e)))?,
            )
            .part(
                "media",
                multipart::Part::bytes(data)
                    .mime_str("application/octet-stream")
                    .map_err(|e| SyncError::provider(format!("Invalid media MIME: {}", e)))?,
            );
        let resp = self
            .request(self.client.post(DRIVE_UPLOAD))
            .query(&[("uploadType", "multipart"), ("fields", "id")])
            .multipart(form)
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("Upload failed: {}", e)))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(provider_status("Upload", resp.status(), resp).await)
        }
    }

    async fn update_existing(&self, file_id: &str, data: Vec<u8>) -> Result<(), SyncError> {
        let resp = self
            .request(self.client.patch([DRIVE_UPLOAD, file_id].join("/")))
            .query(&[("uploadType", "media")])
            .body(data)
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("Upload failed: {}", e)))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(provider_status("Upload", resp.status(), resp).await)
        }
    }
}

#[async_trait]
impl SyncProvider for GoogleDriveProvider {
    async fn test_connection(&self) -> Result<(), SyncError> {
        self.ensure_folder_path(&self.root_path).await.map(|_| ())
    }

    async fn list(&self, prefix: &str) -> Result<Vec<RemoteObject>, SyncError> {
        let full = self.logical_path(prefix);
        let Some(folder_id) = self.folder_id_for_path(&full).await? else {
            return Ok(vec![]);
        };
        let query = format!(
            "'{}' in parents and trashed = false and mimeType != 'application/vnd.google-apps.folder'",
            escape_query(&folder_id)
        );
        let resp = self
            .request(self.client.get(DRIVE_FILES))
            .query(&[
                ("spaces", "appDataFolder"),
                (
                    "fields",
                    "files(id,name,parents,size,modifiedTime,md5Checksum)",
                ),
                ("pageSize", "1000"),
                ("q", query.as_str()),
            ])
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("List failed: {}", e)))?;
        if !resp.status().is_success() {
            return Err(provider_status("List", resp.status(), resp).await);
        }
        let list: FileList = resp
            .json()
            .await
            .map_err(|e| SyncError::provider(format!("Failed to parse list response: {}", e)))?;
        Ok(list
            .files
            .into_iter()
            .map(|file| RemoteObject {
                path: join_remote_path(prefix, &file.name),
                size: file.size.and_then(|size| size.parse().ok()).unwrap_or(0),
                modified_at: file
                    .modified_time
                    .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                etag: file.md5_checksum,
            })
            .collect())
    }

    async fn stat(&self, path: &str) -> Result<Option<RemoteObject>, SyncError> {
        Ok(self.file_for_path(path).await?.map(|file| RemoteObject {
            path: path.to_string(),
            size: file.size.and_then(|size| size.parse().ok()).unwrap_or(0),
            modified_at: file
                .modified_time
                .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            etag: file.md5_checksum,
        }))
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>, SyncError> {
        let file = self
            .file_for_path(path)
            .await?
            .ok_or_else(|| SyncError::provider(format!("File not found: {}", path)))?;
        let resp = self
            .request(self.client.get([DRIVE_FILES, file.id.as_str()].join("/")))
            .query(&[("alt", "media")])
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("Download failed: {}", e)))?;
        if !resp.status().is_success() {
            return Err(provider_status("Download", resp.status(), resp).await);
        }
        Ok(resp
            .bytes()
            .await
            .map_err(|e| SyncError::provider(format!("Failed to read download body: {}", e)))?
            .to_vec())
    }

    async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), SyncError> {
        let full = self.logical_path(path);
        let (parent, name) = split_parent_name(&full)
            .ok_or_else(|| SyncError::provider("Google Drive upload path is empty"))?;
        let parent_id = self.ensure_folder_path(parent).await?;
        if let Some(existing) = self.find_child(&parent_id, name, false).await? {
            self.update_existing(&existing.id, data).await
        } else {
            self.upload_new(&parent_id, name, data).await
        }
    }

    async fn mkdir_all(&self, path: &str) -> Result<(), SyncError> {
        self.ensure_folder_path(&self.logical_path(path))
            .await
            .map(|_| ())
    }

    async fn move_object(&self, from: &str, to: &str) -> Result<(), SyncError> {
        let file = self
            .file_for_path(from)
            .await?
            .ok_or_else(|| SyncError::provider(format!("File not found: {}", from)))?;
        let to_full = self.logical_path(to);
        let (parent, name) = split_parent_name(&to_full)
            .ok_or_else(|| SyncError::provider("Google Drive target path is empty"))?;
        let parent_id = self.ensure_folder_path(parent).await?;
        let body = serde_json::json!({ "name": name });
        let remove_parents = file.parents.as_deref().unwrap_or(&[]).join(",");
        let resp = self
            .request(self.client.patch([DRIVE_FILES, file.id.as_str()].join("/")))
            .query(&[
                ("addParents", parent_id.as_str()),
                ("removeParents", remove_parents.as_str()),
                ("fields", "id"),
            ])
            .json(&body)
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("Move failed: {}", e)))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(provider_status("Move", resp.status(), resp).await)
        }
    }

    async fn delete(&self, path: &str) -> Result<(), SyncError> {
        let Some(file) = self.file_for_path(path).await? else {
            return Ok(());
        };
        let resp = self
            .request(
                self.client
                    .delete([DRIVE_FILES, file.id.as_str()].join("/")),
            )
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("Delete failed: {}", e)))?;
        if resp.status().is_success() || resp.status() == StatusCode::NOT_FOUND {
            Ok(())
        } else {
            Err(provider_status("Delete", resp.status(), resp).await)
        }
    }
}

fn normalize_root(root: &str) -> String {
    root.trim().trim_matches('/').to_string()
}

fn split_parent_name(path: &str) -> Option<(&str, &str)> {
    let path = path.trim_matches('/');
    if path.is_empty() {
        return None;
    }
    Some(path.rsplit_once('/').unwrap_or(("", path)))
}

fn escape_query(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\'', "\\'")
}

async fn provider_status(prefix: &str, status: StatusCode, resp: reqwest::Response) -> SyncError {
    let body = resp.text().await.unwrap_or_default();
    SyncError::provider(format!(
        "{} failed with HTTP status {}: {}",
        prefix, status, body
    ))
}
