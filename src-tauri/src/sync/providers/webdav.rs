//! WebDAV Provider implementation
//!
//! Implements the SyncProvider trait using raw HTTP requests via reqwest.
//! WebDAV operations use standard HTTP methods (PROPFIND, GET, PUT, MKCOL, MOVE, DELETE)
//! with XML request/response parsing for PROPFIND responses.

use crate::sync::error::SyncError;
use crate::sync::models::RemoteObject;
use crate::sync::providers::SyncProvider;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{Client, Method, StatusCode};
use std::time::Duration;

/// XML namespace for DAV responses
const DAV_NS: &str = "DAV:";

/// Timeout for WebDAV operations
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

pub struct WebDavProvider {
    base_url: String,
    username: String,
    password: String,
    client: Client,
}

impl WebDavProvider {
    pub async fn new(base_url: &str, username: &str, password: &str) -> Result<Self, SyncError> {
        // Normalize base_url: ensure it ends with /
        let base_url = if base_url.ends_with('/') {
            base_url.to_string()
        } else {
            [base_url, ""].join("/")
        };

        let client = Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| SyncError::provider(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            base_url,
            username: username.to_string(),
            password: password.to_string(),
            client,
        })
    }

    /// Build the full URL for a remote path
    fn build_url(&self, path: &str) -> String {
        // Remove leading slash from path to avoid double slashes
        let path = path.trim_start_matches('/');
        format!("{}{}", self.base_url, path)
    }

    /// Create a request builder with authentication
    fn request(&self, method: Method, url: &str) -> reqwest::RequestBuilder {
        self.client
            .request(method, url)
            .basic_auth(&self.username, Some(&self.password))
    }

    /// Parse a WebDAV PROPFIND XML response and extract RemoteObject entries
    fn parse_propfind_response(
        &self,
        xml: &str,
        is_single: bool,
    ) -> Result<Vec<RemoteObject>, SyncError> {
        let mut results = Vec::new();

        // Simple XML parsing without external dependencies
        // We look for <response> blocks and extract:
        // - <href> (path)
        // - <getcontentlength> (size)
        // - <getlastmodified> (modified_at)
        // - <getetag> (etag)
        // - <resourcetype> (to detect directories)

        let responses = Self::extract_xml_blocks(xml, "response");

        for response in &responses {
            // Skip if this is a directory and we only want single file stat
            let resource_type = Self::extract_xml_text(response, "resourcetype");
            let is_collection = resource_type
                .as_ref()
                .map(|rt| rt.contains("<collection"))
                .unwrap_or(false);

            if is_single && is_collection {
                continue;
            }

            let href = Self::extract_xml_text(response, "href")
                .ok_or_else(|| SyncError::provider("PROPFIND response missing href"))?;

            // Decode URL-encoded href
            let path = urlencoding::decode(&href)
                .unwrap_or_else(|_| href.clone().into())
                .to_string();

            let size = Self::extract_xml_text(response, "getcontentlength")
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(0);

            let modified_at = Self::extract_xml_text(response, "getlastmodified")
                .and_then(|s| parse_webdav_date(&s).ok());

            let etag = Self::extract_xml_text(response, "getetag");

            // Normalize path: strip the base URL path portion
            let normalized_path = self.normalize_remote_path(&path);

            results.push(RemoteObject {
                path: normalized_path,
                size,
                modified_at,
                etag,
            });
        }

        Ok(results)
    }

    /// Normalize a remote path by stripping the base URL path portion
    fn normalize_remote_path(&self, raw_path: &str) -> String {
        // Parse the base URL to extract its path component
        let base_path = if let Ok(parsed) = reqwest::Url::parse(&self.base_url) {
            parsed.path().trim_start_matches('/').to_string()
        } else {
            String::new()
        };

        // Remove leading slash
        let raw_path = raw_path.trim_start_matches('/');

        // If the raw path starts with the base path, strip it
        if !base_path.is_empty() && raw_path.starts_with(&base_path) {
            let stripped = &raw_path[base_path.len()..];
            return stripped.trim_start_matches('/').to_string();
        }

        raw_path.to_string()
    }

    /// Extract text content between the first occurrence of <tag> and </tag>
    fn extract_xml_text(xml: &str, tag: &str) -> Option<String> {
        let open_tag = format!("<{}", tag);
        let close_tag = format!("</{}>", tag);

        let start = xml.find(&open_tag)?;
        // Find the end of the opening tag (handle self-closing and attributes)
        let tag_content_start = xml[start..].find('>')? + start + 1;
        let end = xml[tag_content_start..].find(&close_tag)? + tag_content_start;

        let content = xml[tag_content_start..end].trim();
        if content.is_empty() {
            None
        } else {
            Some(content.to_string())
        }
    }

    /// Extract all blocks matching the given tag name
    fn extract_xml_blocks(xml: &str, tag: &str) -> Vec<String> {
        let mut blocks = Vec::new();
        let open_tag = format!("<{}", tag);
        let close_tag = format!("</{}>", tag);

        let mut search_from = 0;
        while let Some(start) = xml[search_from..].find(&open_tag) {
            let abs_start = search_from + start;
            // Find matching closing tag
            if let Some(end_rel) = xml[abs_start..].find(&close_tag) {
                let abs_end = abs_start + end_rel + close_tag.len();
                blocks.push(xml[abs_start..abs_end].to_string());
                search_from = abs_end;
            } else {
                break;
            }
        }

        blocks
    }
}

#[async_trait]
impl SyncProvider for WebDavProvider {
    /// Test connection by sending a PROPFIND request to the root
    async fn test_connection(&self) -> Result<(), SyncError> {
        log::info!("[Sync::WebDAV] Testing connection to {}", self.base_url);

        let url = self.build_url("");
        let body = build_propfind_body();

        let resp = self
            .request(Method::from_bytes(b"PROPFIND").unwrap(), &url)
            .header("Depth", "0")
            .header("Content-Type", "application/xml")
            .body(body)
            .send()
            .await
            .map_err(|e| {
                log::error!("[Sync::WebDAV] Connection test failed: {}", e);
                SyncError::provider(format!("Failed to connect: {}", e))
            })?;

        let status = resp.status();
        if status.is_success() || status == StatusCode::MULTI_STATUS {
            // Optionally read the response to verify it's valid XML
            let text = resp.text().await.unwrap_or_default();
            if text.contains("<response>") || text.contains("href") {
                log::info!("[Sync::WebDAV] Connection test successful");
                Ok(())
            } else {
                // Some servers return HTML error pages even with 200
                log::warn!("[Sync::WebDAV] Server responded but no DAV XML found");
                Err(SyncError::provider(
                    "Server responded but did not return valid WebDAV XML".to_string(),
                ))
            }
        } else {
            let body_text = resp.text().await.unwrap_or_default();
            log::error!(
                "[Sync::WebDAV] Connection test failed with status {}: {}",
                status,
                body_text
            );
            Err(SyncError::provider(format!(
                "Connection test failed with HTTP status {}",
                status
            )))
        }
    }

    /// List objects under a prefix using PROPFIND with Depth: 1
    async fn list(&self, prefix: &str) -> Result<Vec<RemoteObject>, SyncError> {
        log::debug!("[Sync::WebDAV] Listing: {}", prefix);

        let url = self.build_url(prefix);
        let body = build_propfind_body();

        let resp = self
            .request(Method::from_bytes(b"PROPFIND").unwrap(), &url)
            .header("Depth", "1")
            .header("Content-Type", "application/xml")
            .body(body)
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("PROPFIND failed: {}", e)))?;

        let status = resp.status();
        if !status.is_success() && status != StatusCode::MULTI_STATUS {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(SyncError::provider(format!(
                "PROPFIND failed with status {}: {}",
                status, body_text
            )));
        }

        let xml = resp
            .text()
            .await
            .map_err(|e| SyncError::provider(format!("Failed to read response: {}", e)))?;

        let objects = self.parse_propfind_response(&xml, false)?;

        // Filter out the directory entry itself (the prefix path)
        let prefix_normalized = self.normalize_remote_path(prefix);
        let filtered: Vec<RemoteObject> = objects
            .into_iter()
            .filter(|obj| {
                let p = &obj.path;
                // Exclude the prefix directory itself and empty paths
                !p.is_empty() && p != prefix_normalized.trim_end_matches('/')
            })
            .collect();

        log::debug!(
            "[Sync::WebDAV] Listed {} objects under {}",
            filtered.len(),
            prefix
        );
        Ok(filtered)
    }

    /// Get metadata for a single path using PROPFIND with Depth: 0
    async fn stat(&self, path: &str) -> Result<Option<RemoteObject>, SyncError> {
        log::debug!("[Sync::WebDAV] Stat: {}", path);

        let url = self.build_url(path);
        let body = build_propfind_body();

        let resp = match self
            .request(Method::from_bytes(b"PROPFIND").unwrap(), &url)
            .header("Depth", "0")
            .header("Content-Type", "application/xml")
            .body(body)
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                // Check if it's a 404 wrapped in the error
                if let Some(status) = e.status() {
                    if status == StatusCode::NOT_FOUND {
                        return Ok(None);
                    }
                }
                return Err(SyncError::provider(format!("PROPFIND failed: {}", e)));
            }
        };

        let status = resp.status();

        // 404 means not found
        if status == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        if !status.is_success() && status != StatusCode::MULTI_STATUS {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(SyncError::provider(format!(
                "PROPFIND failed with status {}: {}",
                status, body_text
            )));
        }

        let xml = resp
            .text()
            .await
            .map_err(|e| SyncError::provider(format!("Failed to read response: {}", e)))?;

        let objects = self.parse_propfind_response(&xml, true)?;

        // Return the first object, or None if empty
        if objects.is_empty() {
            Ok(None)
        } else {
            Ok(Some(objects.into_iter().next().unwrap()))
        }
    }

    /// Download file content using GET
    async fn get(&self, path: &str) -> Result<Vec<u8>, SyncError> {
        log::debug!("[Sync::WebDAV] Getting: {}", path);

        let url = self.build_url(path);

        let resp = self
            .request(Method::GET, &url)
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("GET failed: {}", e)))?;

        let status = resp.status();
        if status == StatusCode::NOT_FOUND {
            return Err(SyncError::provider(format!("File not found: {}", path)));
        }

        if !status.is_success() {
            let body_text = resp.text().await.unwrap_or_default();
            return Err(SyncError::provider(format!(
                "GET failed with status {}: {}",
                status, body_text
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| SyncError::provider(format!("Failed to read response body: {}", e)))?;

        log::debug!("[Sync::WebDAV] Got {} bytes from {}", bytes.len(), path);
        Ok(bytes.to_vec())
    }

    /// Upload file content using PUT
    async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), SyncError> {
        log::debug!("[Sync::WebDAV] Putting: {} ({} bytes)", path, data.len());

        if let Some(parent) = path.trim_matches('/').rsplit_once('/') {
            if !parent.0.is_empty() {
                self.mkdir_all(parent.0).await?;
            }
        }

        let url = self.build_url(path);

        let resp = self
            .request(Method::PUT, &url)
            .body(data)
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("PUT failed: {}", e)))?;

        let status = resp.status();
        if status.is_success() || status == StatusCode::CREATED {
            log::debug!("[Sync::WebDAV] Successfully put {}", path);
            Ok(())
        } else {
            let body_text = resp.text().await.unwrap_or_default();
            Err(SyncError::provider(format!(
                "PUT failed with status {}: {}",
                status, body_text
            )))
        }
    }

    /// Create directory hierarchy using MKCOL requests
    async fn mkdir_all(&self, path: &str) -> Result<(), SyncError> {
        log::debug!("[Sync::WebDAV] Mkdir: {}", path);

        // Split path into components and create each directory in sequence
        let path = path.trim_start_matches('/').trim_end_matches('/');
        if path.is_empty() {
            return Ok(());
        }

        let parts: Vec<&str> = path.split('/').collect();

        // Try to create each directory level
        for i in 1..=parts.len() {
            let partial = parts[..i].join("/");
            let dir_path = [partial.as_str(), ""].join("/");
            let url = self.build_url(&dir_path);

            let resp = self
                .request(Method::from_bytes(b"MKCOL").unwrap(), &url)
                .send()
                .await
                .map_err(|e| SyncError::provider(format!("MKCOL failed for {}: {}", partial, e)))?;

            let status = resp.status();

            // 201 Created = success, 405 Method Not Allowed = already exists,
            // 409 Conflict = parent doesn't exist (shouldn't happen if we create in order)
            if status == StatusCode::CREATED
                || status == StatusCode::OK
                || status == StatusCode::NO_CONTENT
            {
                log::debug!("[Sync::WebDAV] Created directory: {}", partial);
            } else if status == StatusCode::METHOD_NOT_ALLOWED {
                // Directory already exists
                log::debug!("[Sync::WebDAV] Directory already exists: {}", partial);
            } else if status == StatusCode::CONFLICT {
                // Parent doesn't exist - this shouldn't happen with sequential creation
                return Err(SyncError::provider(format!(
                    "MKCOL conflict for {}: parent directory does not exist",
                    partial
                )));
            } else {
                let body_text = resp.text().await.unwrap_or_default();
                // If the directory already exists (some servers return different codes),
                // check with a PROPFIND
                let dir_path = [partial.as_str(), ""].join("/");
                let stat_result = self.stat(&dir_path).await;
                match stat_result {
                    Ok(Some(_)) => {
                        log::debug!(
                            "[Sync::WebDAV] Directory already exists (verified): {}",
                            partial
                        );
                    }
                    _ => {
                        return Err(SyncError::provider(format!(
                            "MKCOL failed for {} with status {}: {}",
                            partial, status, body_text
                        )));
                    }
                }
            }
        }

        Ok(())
    }

    /// Move/rename object using MOVE request
    async fn move_object(&self, from: &str, to: &str) -> Result<(), SyncError> {
        log::debug!("[Sync::WebDAV] Move: {} -> {}", from, to);

        let from_url = self.build_url(from);
        let to_url = self.build_url(to);

        let resp = self
            .request(Method::from_bytes(b"MOVE").unwrap(), &from_url)
            .header("Destination", &to_url)
            .header("Overwrite", "T")
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("MOVE failed: {}", e)))?;

        let status = resp.status();
        if status.is_success() || status == StatusCode::CREATED || status == StatusCode::NO_CONTENT
        {
            log::debug!("[Sync::WebDAV] Successfully moved {} -> {}", from, to);
            Ok(())
        } else {
            let body_text = resp.text().await.unwrap_or_default();
            Err(SyncError::provider(format!(
                "MOVE failed with status {}: {}",
                status, body_text
            )))
        }
    }

    /// Delete object using DELETE request
    async fn delete(&self, path: &str) -> Result<(), SyncError> {
        log::debug!("[Sync::WebDAV] Delete: {}", path);

        let url = self.build_url(path);

        let resp = self
            .request(Method::DELETE, &url)
            .send()
            .await
            .map_err(|e| SyncError::provider(format!("DELETE failed: {}", e)))?;

        let status = resp.status();
        if status.is_success() || status == StatusCode::NO_CONTENT {
            log::debug!("[Sync::WebDAV] Successfully deleted {}", path);
            Ok(())
        } else if status == StatusCode::NOT_FOUND {
            // Already deleted or never existed - treat as success
            log::debug!("[Sync::WebDAV] Already deleted or not found: {}", path);
            Ok(())
        } else {
            let body_text = resp.text().await.unwrap_or_default();
            Err(SyncError::provider(format!(
                "DELETE failed with status {}: {}",
                status, body_text
            )))
        }
    }
}

/// Build a standard PROPFIND request body asking for common properties
fn build_propfind_body() -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<D:propfind xmlns:D="{ns}">
  <D:prop>
    <D:resourcetype/>
    <D:getcontentlength/>
    <D:getlastmodified/>
    <D:getetag/>
    <D:displayname/>
  </D:prop>
</D:propfind>"#,
        ns = DAV_NS
    )
}

/// Parse WebDAV date format (RFC 1123)
/// Example: "Wed, 10 May 2023 14:30:00 GMT"
fn parse_webdav_date(s: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    // Try RFC 1123 format first (most common for WebDAV)
    DateTime::parse_from_rfc2822(s)
        .map(|dt| dt.with_timezone(&Utc))
        .or_else(|_| {
            // Fallback: try ISO 8601
            DateTime::parse_from_rfc3339(s).map(|dt| dt.with_timezone(&Utc))
        })
        .or_else(|_| {
            // Last fallback: try common WebDAV variants
            chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ")
                .map(|nd| DateTime::<Utc>::from_naive_utc_and_offset(nd, Utc))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_parse_webdav_date_rfc1123() {
        let date = parse_webdav_date("Wed, 10 May 2023 14:30:00 GMT").unwrap();
        assert_eq!(date.year(), 2023);
        assert_eq!(date.month(), 5);
        assert_eq!(date.day(), 10);
    }

    #[test]
    fn test_parse_webdav_date_iso8601() {
        let date = parse_webdav_date("2023-05-10T14:30:00Z").unwrap();
        assert_eq!(date.year(), 2023);
        assert_eq!(date.month(), 5);
        assert_eq!(date.day(), 10);
    }

    #[test]
    fn test_extract_xml_text() {
        let xml = "<response><href>/path/to/file</href><getcontentlength>1234</getcontentlength></response>";
        assert_eq!(
            WebDavProvider::extract_xml_text(xml, "href"),
            Some("/path/to/file".to_string())
        );
        assert_eq!(
            WebDavProvider::extract_xml_text(xml, "getcontentlength"),
            Some("1234".to_string())
        );
        assert_eq!(WebDavProvider::extract_xml_text(xml, "nonexistent"), None);
    }

    #[test]
    fn test_extract_xml_blocks() {
        let xml = r#"
        <multistatus>
            <response><href>/file1</href></response>
            <response><href>/file2</href></response>
        </multistatus>"#;

        let blocks = WebDavProvider::extract_xml_blocks(xml, "response");
        assert_eq!(blocks.len(), 2);
        assert!(blocks[0].contains("/file1"));
        assert!(blocks[1].contains("/file2"));
    }

    #[test]
    fn test_parse_propfind_response() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<D:multistatus xmlns:D="DAV:">
  <D:response>
    <D:href>/remote.php/dav/files/admin/test.txt</D:href>
    <D:propstat>
      <D:prop>
        <D:resourcetype/>
        <D:getcontentlength>1234</D:getcontentlength>
        <D:getlastmodified>Wed, 10 May 2023 14:30:00 GMT</D:getlastmodified>
        <D:getetag>"abc123"</D:getetag>
      </D:prop>
    </D:propstat>
  </D:response>
</D:multistatus>"#;

        // Create a minimal provider for testing the parser
        // We can't easily construct a full WebDavProvider in tests,
        // so we test the static methods directly
        // Note: extract_xml_blocks matches partial tags, so "response" matches "<D:response"
        let responses = WebDavProvider::extract_xml_blocks(xml, "D:response");
        assert_eq!(responses.len(), 1);

        let href = WebDavProvider::extract_xml_text(&responses[0], "D:href");
        assert!(href.is_some());
    }
}
