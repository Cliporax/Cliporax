/// Snapshot manifest management.
use crate::sync::error::SyncError;
use crate::sync::models::SnapshotManifest;
use crate::sync::providers::{join_remote_path, SyncProvider};

pub const MANIFEST_PATH: &str = "manifest.json";
pub const SNAPSHOT_SCHEMA_VERSION: u32 = 2;
pub const APP_NAME: &str = "Cliporax";

pub async fn read_manifest(
    provider: &dyn SyncProvider,
    base_path: &str,
) -> Result<Option<SnapshotManifest>, SyncError> {
    let manifest_path = join_remote_path(base_path, MANIFEST_PATH);
    let data = match provider.get(&manifest_path).await {
        Ok(data) => data,
        Err(SyncError::Provider(message)) if message.contains("File not found") => {
            return Ok(None);
        }
        Err(error) => return Err(error),
    };

    let manifest: SnapshotManifest = serde_json::from_slice(&data)
        .map_err(|e| SyncError::Provider(format!("Failed to parse manifest: {}", e)))?;
    if manifest.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(SyncError::SchemaVersionMismatch {
            local: SNAPSHOT_SCHEMA_VERSION,
            remote: manifest.schema_version,
        });
    }
    Ok(Some(manifest))
}

pub async fn write_manifest(
    provider: &dyn SyncProvider,
    base_path: &str,
    manifest: &SnapshotManifest,
) -> Result<(), SyncError> {
    let manifest_path = join_remote_path(base_path, MANIFEST_PATH);
    let data = serde_json::to_vec_pretty(manifest).map_err(SyncError::Serialization)?;
    provider.put(&manifest_path, data).await
}
