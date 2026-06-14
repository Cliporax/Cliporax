//! OneDrive provider backed by Microsoft Graph app folder storage.

use crate::sync::error::SyncError;
use crate::sync::models::RemoteObject;
use crate::sync::providers::{join_remote_path, SyncProvider};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use std::time::Duration;

const GRAPH_ROOT: &str = "https://graph.microsoft.com/v1.0/me/drive/special/approot";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct OneDriveProvider {
    root_path: String,
    access_token: String,
    client: Client,
}

#[derive(Deserialize)]
struct DriveItem {
    name: String,
    size: Option<u64>,
    #[serde(rename = "lastModifiedDateTime")]
    last_modified: Option<String>,
    e_tag: Option<String>,
    file: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct DriveChildren {
    value: Vec<DriveItem>,
}

impl OneDriveProvider {
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

    fn item_url(&self, path: &str) -> String {
        let full = self.logical_path(path);
        if full.is_empty() {
            GRAPH_ROOT.to_string()
        } else {
            format!("{}:/{}", GRAPH_ROOT, encode_path(&full))
        }
    }

    fn content_url(&self, path: &str) -> String {
        format!("{}:/content", self.item_url(path))
    }

    async fn create_child_folder(&self, parent: &str, name: &str) -> Result<(), SyncError> {
        let base = self.item_url(parent);
        let url = [base.as_str(), "children"].join("/");
        let body = serde_json::json!({
            "name": name,
            "folder": {},
            "@microsoft.graph.conflictBehavior": "replace"
        });
        let resp = self
            .request(self.client.post(url))
            .json(&body)
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("Create folder failed: {}", e)))?;
        let status = resp.status();
        if status.is_success() || status == StatusCode::CONFLICT {
            Ok(())
        } else {
            Err(provider_status("Create folder", status, resp).await)
        }
    }
}

#[async_trait]
impl SyncProvider for OneDriveProvider {
    async fn test_connection(&self) -> Result<(), SyncError> {
        self.mkdir_all("").await?;
        let resp = self
            .request(self.client.get(self.item_url("")))
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("Failed to connect: {}", e)))?;
        if resp.status().is_success() {
            Ok(())
        } else {
            Err(provider_status("Connection test", resp.status(), resp).await)
        }
    }

    async fn list(&self, prefix: &str) -> Result<Vec<RemoteObject>, SyncError> {
        let base = self.item_url(prefix);
        let url = [base.as_str(), "children"].join("/");
        let resp = self
            .request(self.client.get(url))
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("List failed: {}", e)))?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }
        if !resp.status().is_success() {
            return Err(provider_status("List", resp.status(), resp).await);
        }
        let children: DriveChildren = resp
            .json()
            .await
            .map_err(|e| SyncError::provider(format!("Failed to parse list response: {}", e)))?;
        Ok(children
            .value
            .into_iter()
            .filter(|item| item.file.is_some())
            .map(|item| RemoteObject {
                path: join_remote_path(prefix, &item.name),
                size: item.size.unwrap_or(0),
                modified_at: item
                    .last_modified
                    .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
                    .map(|dt| dt.with_timezone(&Utc)),
                etag: item.e_tag,
            })
            .collect())
    }

    async fn stat(&self, path: &str) -> Result<Option<RemoteObject>, SyncError> {
        let resp = self
            .request(self.client.get(self.item_url(path)))
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("Stat failed: {}", e)))?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(provider_status("Stat", resp.status(), resp).await);
        }
        let item: DriveItem = resp
            .json()
            .await
            .map_err(|e| SyncError::provider(format!("Failed to parse stat response: {}", e)))?;
        Ok(Some(RemoteObject {
            path: path.to_string(),
            size: item.size.unwrap_or(0),
            modified_at: item
                .last_modified
                .and_then(|value| DateTime::parse_from_rfc3339(&value).ok())
                .map(|dt| dt.with_timezone(&Utc)),
            etag: item.e_tag,
        }))
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>, SyncError> {
        let resp = self
            .request(self.client.get(self.content_url(path)))
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
        if let Some((parent, _)) = path.rsplit_once('/') {
            self.mkdir_all(parent).await?;
        }
        let resp = self
            .request(self.client.put(self.content_url(path)))
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

    async fn mkdir_all(&self, path: &str) -> Result<(), SyncError> {
        let full = self.logical_path(path);
        let mut parent = String::new();
        for part in full.split('/').filter(|part| !part.is_empty()) {
            let current = join_remote_path(&parent, part);
            if self.stat_path_in_app_root(&current).await?.is_none() {
                self.create_child_folder(&parent, part).await?;
            }
            parent = current;
        }
        Ok(())
    }

    async fn move_object(&self, from: &str, to: &str) -> Result<(), SyncError> {
        if let Some((parent, _)) = to.rsplit_once('/') {
            self.mkdir_all(parent).await?;
        }
        let to_full = self.logical_path(to);
        let (parent_path, name) = to_full
            .rsplit_once('/')
            .map(|(parent, name)| (parent.to_string(), name.to_string()))
            .unwrap_or_else(|| (String::new(), to_full));
        let parent_id = self
            .stat_path_in_app_root(&parent_path)
            .await?
            .ok_or_else(|| SyncError::provider("OneDrive target parent folder not found"))?;
        let body = serde_json::json!({
            "parentReference": { "id": parent_id },
            "name": name,
        });
        let resp = self
            .request(self.client.patch(self.item_url(from)))
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
        let resp = self
            .request(self.client.delete(self.item_url(path)))
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

impl OneDriveProvider {
    async fn stat_path_in_app_root(&self, path: &str) -> Result<Option<String>, SyncError> {
        let url = if path.is_empty() {
            GRAPH_ROOT.to_string()
        } else {
            format!("{}:/{}", GRAPH_ROOT, encode_path(path))
        };
        let resp = self
            .request(self.client.get(url))
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("Stat failed: {}", e)))?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if !resp.status().is_success() {
            return Err(provider_status("Stat", resp.status(), resp).await);
        }
        let value: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| SyncError::provider(format!("Failed to parse stat response: {}", e)))?;
        Ok(value
            .get("id")
            .and_then(|id| id.as_str())
            .map(|id| id.to_string()))
    }
}

fn normalize_root(root: &str) -> String {
    root.trim().trim_matches('/').to_string()
}

fn encode_path(path: &str) -> String {
    path.split('/')
        .filter(|part| !part.is_empty())
        .map(urlencoding::encode)
        .collect::<Vec<_>>()
        .join("/")
}

async fn provider_status(prefix: &str, status: StatusCode, resp: reqwest::Response) -> SyncError {
    let body = resp.text().await.unwrap_or_default();
    SyncError::provider(format!(
        "{} failed with HTTP status {}: {}",
        prefix, status, body
    ))
}
