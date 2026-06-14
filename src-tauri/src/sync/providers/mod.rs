/// Provider abstraction for remote storage
use crate::sync::error::SyncError;
use crate::sync::models::RemoteObject;
use async_trait::async_trait;

#[async_trait]
pub trait SyncProvider: Send + Sync {
    /// Test connection to the remote server
    async fn test_connection(&self) -> Result<(), SyncError>;

    /// List objects with a given prefix
    async fn list(&self, prefix: &str) -> Result<Vec<RemoteObject>, SyncError>;

    /// Get object metadata
    async fn stat(&self, path: &str) -> Result<Option<RemoteObject>, SyncError>;

    /// Download object data
    async fn get(&self, path: &str) -> Result<Vec<u8>, SyncError>;

    /// Upload object data
    async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), SyncError>;

    /// Create directory hierarchy
    async fn mkdir_all(&self, path: &str) -> Result<(), SyncError>;

    /// Move object from one path to another
    async fn move_object(&self, from: &str, to: &str) -> Result<(), SyncError>;

    /// Delete object
    async fn delete(&self, path: &str) -> Result<(), SyncError>;
}

/// Join logical remote paths. Remote provider paths are POSIX-style on every
/// platform and must not use local OS separators.
pub fn join_remote_path(base: &str, child: &str) -> String {
    let base = base.trim_end_matches('/');
    let child = child.trim_start_matches('/');

    match (base.is_empty(), child.is_empty()) {
        (true, true) => String::new(),
        (true, false) => child.to_string(),
        (false, true) => base.to_string(),
        (false, false) => [base, child].join("/"),
    }
}

pub mod factory;
pub mod google_drive;
pub mod onedrive;
pub mod sftp;
pub mod webdav;
