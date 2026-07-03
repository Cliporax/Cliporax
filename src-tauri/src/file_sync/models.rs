use serde::{Deserialize, Serialize};

pub const FILE_SYNC_SCHEMA_VERSION: u32 = 1;
pub const CONFIRM_THRESHOLD_BYTES: u64 = 100 * 1024 * 1024;
pub const CHUNK_SIZE: usize = 8 * 1024 * 1024;
pub const GOOGLE_DRIVE_CHUNK_SIZE: usize = 4 * 1024 * 1024;
pub const MAX_SINGLE_FILE_BYTES: u64 = 20 * 1024 * 1024 * 1024;
pub const MAX_ENTRY_BYTES: u64 = 100 * 1024 * 1024 * 1024;
pub const MAX_ENTRY_FILES: usize = 100_000;
pub const MAX_COPY_ENTRIES: usize = 32;
pub const CANCELLED_ERROR: &str = "File sync task was cancelled";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSyncEntry {
    pub id: String,
    pub profile_id: String,
    pub origin_device_id: String,
    pub kind: String,
    pub display_name: String,
    pub total_size: u64,
    pub file_count: u64,
    pub revision: i64,
    pub status: String,
    pub confirmed: bool,
    pub progress_bytes: u64,
    pub error: Option<String>,
    pub synced_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSyncConfig {
    pub default_profile_id: Option<String>,
    pub confirmation_threshold_bytes: u64,
    pub chunk_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSyncProfileOption {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub encryption_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSyncEnqueueResult {
    pub entry_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSyncClipboardItemStatus {
    pub visible: bool,
    pub can_enqueue: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestChunk {
    pub index: u32,
    pub size: u64,
    pub sha256: String,
    pub remote_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestNode {
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub modified_unix_ms: Option<i64>,
    #[serde(default)]
    pub chunks: Vec<ManifestChunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSyncManifest {
    pub schema_version: u32,
    pub entry_id: String,
    pub revision: i64,
    pub kind: String,
    pub display_name: String,
    pub total_size: u64,
    pub file_count: u64,
    pub created_at: String,
    pub nodes: Vec<ManifestNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteEntrySummary {
    pub id: String,
    pub origin_device_id: String,
    pub kind: String,
    pub display_name: String,
    pub total_size: u64,
    pub file_count: u64,
    pub revision: i64,
    pub manifest_path: String,
    pub manifest_hash: String,
    pub synced_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSyncRemoteEvent {
    pub schema_version: u32,
    pub device_id: String,
    pub seq: i64,
    pub operation: String,
    pub entry_id: String,
    pub revision: i64,
    pub changed_at: String,
    pub entry: Option<RemoteEntrySummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSyncChangedEvent {
    pub entry_ids: Vec<String>,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSyncProgressEvent {
    pub entry_id: String,
    pub status: String,
    pub completed_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct ScannedNode {
    pub absolute_path: std::path::PathBuf,
    pub relative_path: String,
    pub kind: String,
    pub size: u64,
    pub modified_unix_ms: Option<i64>,
    pub file_identity: Option<(u64, u64)>,
}

#[derive(Debug, Clone)]
pub struct ScanResult {
    pub kind: String,
    pub display_name: String,
    pub total_size: u64,
    pub file_count: u64,
    pub nodes: Vec<ScannedNode>,
}

#[derive(Debug, Clone)]
pub struct PreparedChunk {
    pub file_index: i64,
    pub chunk_index: i64,
    pub relative_path: String,
    pub size: u64,
    pub plaintext_hash: String,
    pub remote_path: String,
    pub staging_path: std::path::PathBuf,
}

#[derive(Debug, Clone)]
pub struct PreparedSnapshot {
    pub manifest: FileSyncManifest,
    pub manifest_path: std::path::PathBuf,
    pub manifest_hash: String,
    pub chunks: Vec<PreparedChunk>,
}
