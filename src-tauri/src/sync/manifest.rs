/// Manifest - remote manifest management
use crate::sync::error::SyncError;
use crate::sync::providers::{join_remote_path, SyncProvider};

const MANIFEST_PATH: &str = "manifest.json";
const SCHEMA_VERSION: u32 = 1;
const APP_NAME: &str = "Cliporax";

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct RemoteManifest {
    pub schema_version: u32,
    pub app: String,
    pub created_at: String,
    pub updated_at: String,
    pub encryption: EncryptionInfo,
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Default)]
pub struct EncryptionInfo {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kdf: Option<String>,
}

/// Get or create remote manifest
pub async fn get_or_create_manifest(
    provider: &dyn SyncProvider,
    base_path: &str,
) -> Result<RemoteManifest, SyncError> {
    let manifest_path = join_remote_path(base_path, MANIFEST_PATH);

    match provider.stat(&manifest_path).await? {
        Some(_) => {
            // Read existing manifest
            let data = provider.get(&manifest_path).await?;
            let manifest: RemoteManifest = serde_json::from_slice(&data)
                .map_err(|e| SyncError::Provider(format!("Failed to parse manifest: {}", e)))?;

            // Check schema version
            if manifest.schema_version != SCHEMA_VERSION {
                return Err(SyncError::SchemaVersionMismatch {
                    local: SCHEMA_VERSION,
                    remote: manifest.schema_version,
                });
            }

            Ok(manifest)
        }
        None => {
            // Create new manifest
            let now = chrono::Utc::now().to_rfc3339();
            let manifest = RemoteManifest {
                schema_version: SCHEMA_VERSION,
                app: APP_NAME.to_string(),
                created_at: now.clone(),
                updated_at: now,
                encryption: EncryptionInfo::default(),
            };

            let data = serde_json::to_string_pretty(&manifest).map_err(SyncError::Serialization)?;

            provider.put(&manifest_path, data.into_bytes()).await?;

            Ok(manifest)
        }
    }
}

/// Update manifest
pub async fn update_manifest(
    provider: &dyn SyncProvider,
    base_path: &str,
    manifest: &RemoteManifest,
) -> Result<(), SyncError> {
    let manifest_path = join_remote_path(base_path, MANIFEST_PATH);
    let data = serde_json::to_string_pretty(&manifest).map_err(SyncError::Serialization)?;

    provider.put(&manifest_path, data.into_bytes()).await
}
