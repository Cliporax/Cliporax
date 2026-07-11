use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};

/// Sync provider type
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyncProviderKind {
    #[serde(rename = "webdav", alias = "web_dav")]
    WebDav,
    Sftp,
    GoogleDrive,
    OneDrive,
}

/// Tab sync selection mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum TabSyncMode {
    #[default]
    All,
    Selected,
}

/// Tab sync configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TabSyncSelection {
    pub mode: TabSyncMode,
    #[serde(default)]
    pub selected_tab_ids: Vec<i64>,
}

/// Plugin sync selection mode
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PluginSyncMode {
    #[default]
    Selected,
}

/// Plugin sync configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PluginSyncSelection {
    pub mode: PluginSyncMode,
    #[serde(default)]
    pub selected_plugin_ids: Vec<String>,
    #[serde(default)]
    pub include_plugin_bundles: bool,
    #[serde(default)]
    pub include_granted_permissions: bool,
}

/// Encryption configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EncryptionConfig {
    pub enabled: bool,
    #[serde(default = "default_algorithm")]
    pub algorithm: String,
    #[serde(default = "default_kdf")]
    pub kdf: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub salt_b64: Option<String>,
    #[serde(default = "default_kdf_memory_kb")]
    pub memory_kb: u32,
    #[serde(default = "default_kdf_iterations")]
    pub iterations: u32,
    #[serde(default = "default_kdf_parallelism")]
    pub parallelism: u32,
}

fn default_algorithm() -> String {
    "xchacha20poly1305".to_string()
}

fn default_kdf() -> String {
    "argon2id".to_string()
}

fn default_kdf_memory_kb() -> u32 {
    65536
}

fn default_kdf_iterations() -> u32 {
    3
}

fn default_kdf_parallelism() -> u32 {
    4
}

/// Credential references for a sync profile
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CredentialRefs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub private_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
}

/// Sync profile configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncProfile {
    pub id: String,
    pub name: String,
    pub provider: SyncProviderKind,
    pub remote_root: String,
    #[serde(default)]
    pub sync_tabs: TabSyncSelection,
    #[serde(default)]
    pub sync_plugins: PluginSyncSelection,
    #[serde(default)]
    pub encryption: EncryptionConfig,
    #[serde(default)]
    pub credential_refs: CredentialRefs,
    #[serde(default)]
    pub schedule: SyncScheduleConfig,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// Sync schedule configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncScheduleConfig {
    #[serde(default = "default_true")]
    pub manual: bool,
    #[serde(default = "default_true")]
    pub sync_on_startup: bool,
    #[serde(default = "default_startup_delay")]
    pub startup_delay_seconds: u64,
    #[serde(default = "default_true")]
    pub sync_on_local_change: bool,
    #[serde(default = "default_local_change_debounce")]
    pub local_change_debounce_seconds: u64,
    #[serde(default = "default_interval")]
    pub interval_minutes: u64,
    #[serde(default = "default_retry_backoff")]
    pub retry_backoff_seconds: Vec<u64>,
    #[serde(default)]
    pub pause_on_metered_network: bool,
    #[serde(default)]
    pub paused: bool,
}

impl Default for SyncScheduleConfig {
    fn default() -> Self {
        Self {
            manual: true,
            sync_on_startup: true,
            startup_delay_seconds: 15,
            sync_on_local_change: true,
            local_change_debounce_seconds: 30,
            interval_minutes: 15,
            retry_backoff_seconds: vec![30, 120, 300, 900],
            pause_on_metered_network: false,
            paused: false,
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_startup_delay() -> u64 {
    15
}

fn default_local_change_debounce() -> u64 {
    30
}

fn default_interval() -> u64 {
    15
}

fn default_retry_backoff() -> Vec<u64> {
    vec![30, 120, 300, 900]
}

/// Sync profile summary (for list views)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncProfileSummary {
    pub id: String,
    pub name: String,
    pub provider: SyncProviderKind,
    pub remote_root: String,
    pub encryption_enabled: bool,
    pub last_sync_at: Option<String>,
    pub status: String,
}

/// Sync run state
#[derive(Debug, Clone)]
pub struct SyncRun {
    pub profile_id: String,
    pub device_id: String,
    pub run_id: String,
    pub started_at: chrono::DateTime<chrono::Utc>,
}

/// Remote lock state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteLock {
    pub schema_version: u32,
    pub owner_device_id: String,
    pub owner_run_id: String,
    pub created_at: String,
    pub expires_at: String,
}

/// Remote lock wait configuration
#[derive(Debug, Clone)]
pub struct LockWaitConfig {
    pub max_wait_seconds: u64,
    pub retry_interval_ms: u64,
}

impl Default for LockWaitConfig {
    fn default() -> Self {
        Self {
            max_wait_seconds: 60,
            retry_interval_ms: 1000,
        }
    }
}

/// Change operation types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeOperation {
    Upsert,
    Delete,
}

/// Change source for sync outbox
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSource {
    Local,
    RemoteApply,
    InternalDedup,
    SyncResolution,
}

impl std::fmt::Display for ChangeSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeSource::Local => write!(f, "local"),
            ChangeSource::RemoteApply => write!(f, "remote_apply"),
            ChangeSource::InternalDedup => write!(f, "internal_dedup"),
            ChangeSource::SyncResolution => write!(f, "sync_resolution"),
        }
    }
}

/// Local sync change (outbox entry)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalSyncChange {
    pub id: i64,
    pub entity_type: String,
    pub entity_id: String,
    pub operation: String,
    pub item_key: Option<String>,
    pub tab_id: Option<i64>,
    pub plugin_id: Option<String>,
    pub source: String,
    pub changed_at: String,
    pub synced_at: Option<String>,
}

/// Remote change from change log
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteChange {
    pub schema_version: u32,
    pub seq: i64,
    pub device_id: String,
    pub operation: ChangeOperation,
    pub entity_type: String,
    pub item_key: String,
    pub item_path: Option<String>,
    pub blob_path: Option<String>,
    pub tombstone_path: Option<String>,
    pub changed_at: String,
}

/// Remote clipboard item
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteClipboardItem {
    pub schema_version: u32,
    pub item_key: String,
    #[serde(default)]
    pub stable_seq: i64,
    pub device_id: String,
    pub local_id: Option<i64>,
    #[serde(rename = "type")]
    pub item_type: String,
    pub content: Option<String>,
    pub blob_path: Option<String>,
    pub blob_mime: Option<String>,
    pub content_hash: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    #[serde(default)]
    pub tab_key: Option<String>,
    pub tab_name: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub is_pinned: bool,
    #[serde(default)]
    pub is_sensitive: bool,
    #[serde(default)]
    pub metadata: serde_json::Value,
    #[serde(default)]
    pub revision: i64,
    #[serde(default)]
    pub last_modified_by: String,
    #[serde(default)]
    pub deleted: bool,
    /// Recoverable deletion is a normal item update, distinct from a tombstone.
    #[serde(default)]
    pub is_trashed: bool,
    #[serde(default)]
    pub deleted_at: Option<String>,
    #[serde(default)]
    pub deleted_from_tab_key: Option<String>,
    #[serde(default)]
    pub deleted_from_tab_name: Option<String>,
}

/// Remote manifest for snapshot-based sync.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SnapshotManifest {
    pub schema_version: u32,
    pub app: String,
    #[serde(default)]
    pub generation: i64,
    pub device_id: String,
    pub updated_at: String,
    pub item_shard_size: usize,
    #[serde(default)]
    pub item_shards: Vec<SnapshotShardRef>,
    pub order: Option<SnapshotFileRef>,
    #[serde(default)]
    pub plugin_data: Option<SnapshotFileRef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemotePluginData {
    pub plugin_id: String,
    pub storage_key: String,
    pub value: serde_json::Value,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotShardRef {
    pub path: String,
    pub start: i64,
    pub end: i64,
    pub count: usize,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotFileRef {
    pub path: String,
    pub hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotItemShard {
    pub schema_version: u32,
    pub start: i64,
    pub end: i64,
    pub items: Vec<RemoteClipboardItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotOrder {
    pub schema_version: u32,
    pub updated_at: String,
    #[serde(default)]
    pub tabs: Vec<SnapshotTabOrder>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotTabOrder {
    pub tab_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tab_name: Option<String>,
    #[serde(default)]
    pub pinned: Vec<String>,
    #[serde(default)]
    pub normal: Vec<String>,
}

/// Remote tombstone for deleted items
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteTombstone {
    pub schema_version: u32,
    pub item_key: String,
    pub deleted_at: String,
    pub deleted_by: String,
}

/// Remote object metadata from provider
pub struct RemoteObject {
    pub path: String,
    pub size: u64,
    pub modified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub etag: Option<String>,
}

/// Sync run status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SyncRunStatus {
    Idle,
    WaitingForLock,
    Pulling,
    ApplyingRemote,
    Uploading,
    Committing,
    Completed,
    PartialSuccess,
    Failed,
}

/// Sync run report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRunReport {
    pub profile_id: String,
    pub run_id: String,
    pub status: SyncRunStatus,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub items_uploaded: i64,
    pub items_downloaded: i64,
    pub items_deleted: i64,
    pub conflicts_found: i64,
    pub errors: Vec<String>,
}

/// Apply item result
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplyItemResult {
    Created { local_id: i64 },
    Updated { local_id: i64 },
    Merged { local_id: i64 },
    Conflict { conflict_id: i64 },
    Skipped,
}

/// Apply report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyReport {
    pub items_applied: i64,
    pub conflicts_created: i64,
    pub errors: Vec<String>,
}

/// Commit report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitReport {
    pub items_committed: i64,
    pub changes_committed: i64,
    pub tombstones_committed: i64,
}

/// Staged commit
#[derive(Debug, Clone)]
pub struct StagedCommit {
    pub run_id: String,
    pub item_paths: Vec<String>,
    pub change_paths: Vec<String>,
    pub tombstone_paths: Vec<String>,
    pub warnings: Vec<String>,
}

/// Sync status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStatus {
    pub profile_id: String,
    pub status: SyncRunStatus,
    pub phase: Option<String>,
    pub progress: Option<f32>,
    pub last_sync_at: Option<String>,
    pub next_sync_at: Option<String>,
    pub is_paused: bool,
    pub is_locked: bool,
    pub backoff_reason: Option<String>,
}

/// Connection test result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionTestResult {
    pub success: bool,
    pub message: String,
    pub server_info: Option<String>,
}

/// Result of trusting an SFTP host key
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SftpHostKeyTrustResult {
    pub host: String,
    pub port: u16,
    pub fingerprint_sha256: String,
    pub known_hosts_path: String,
}

/// Sync conflict
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SyncConflict {
    pub id: i64,
    pub entity_type: String,
    pub entity_key: String,
    pub local_payload: String,
    pub remote_payload: String,
    pub reason: String,
    pub status: String,
    pub resolution: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

/// Conflict resolution input
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolutionInput {
    UseLocal,
    UseRemote,
    KeepBoth,
    MergeWithLocalPrimary,
    MergeWithRemotePrimary,
}

/// Sync tab option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncTabOption {
    pub id: i64,
    pub name: String,
}

/// Sync plugin option
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPluginOption {
    pub id: String,
    pub name: String,
    pub is_active: bool,
}

/// Sync log entry
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct SyncLogEntry {
    #[sqlx(rename = "created_at")]
    #[serde(rename = "timestamp")]
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub profile_id: Option<String>,
    pub run_id: Option<String>,
}

/// Secret reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretRef {
    pub ref_id: String,
    pub profile_id: String,
    pub key: String,
}

/// Encoded remote item
#[derive(Debug, Clone)]
pub struct EncodedRemoteItem {
    pub json_data: Vec<u8>,
    pub blob_data: Option<Vec<u8>>,
    pub item_path: String,
    pub blob_path: Option<String>,
}

/// Sync crypto context
#[derive(Debug, Clone)]
pub struct SyncCryptoContext {
    pub algorithm: String,
    pub kdf: String,
    pub salt: Vec<u8>,
    pub memory_kb: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

/// Unlocked sync key
pub struct UnlockedSyncKey {
    pub profile_id: String,
    pub key: secrecy::SecretVec<u8>,
    pub unlocked_at: chrono::DateTime<chrono::Utc>,
}

// Manual impl because secrecy::SecretVec<u8> doesn't implement Debug or Clone
impl std::fmt::Debug for UnlockedSyncKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnlockedSyncKey")
            .field("profile_id", &self.profile_id)
            .field("key", &"<redacted>")
            .field("unlocked_at", &self.unlocked_at)
            .finish()
    }
}

impl Clone for UnlockedSyncKey {
    fn clone(&self) -> Self {
        Self {
            profile_id: self.profile_id.clone(),
            key: secrecy::SecretVec::new(self.key.expose_secret().clone()),
            unlocked_at: self.unlocked_at,
        }
    }
}

/// Cleanup report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupReport {
    pub tombstones_removed: i64,
    pub tmp_dirs_cleaned: i64,
    pub errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::{SnapshotManifest, SyncProviderKind};

    #[test]
    fn webdav_provider_serializes_to_frontend_value() -> Result<(), serde_json::Error> {
        let value = serde_json::to_string(&SyncProviderKind::WebDav)?;
        assert_eq!(value, "\"webdav\"");
        Ok(())
    }

    #[test]
    fn webdav_provider_reads_legacy_snake_case_value() -> Result<(), serde_json::Error> {
        let provider: SyncProviderKind = serde_json::from_str("\"web_dav\"")?;
        assert_eq!(provider, SyncProviderKind::WebDav);
        Ok(())
    }

    #[test]
    fn snapshot_manifest_reads_legacy_missing_generation() -> Result<(), serde_json::Error> {
        let manifest: SnapshotManifest = serde_json::from_str(
            r#"{
              "schema_version": 2,
              "app": "Cliporax",
              "device_id": "old-device",
              "updated_at": "2026-01-01T00:00:00Z",
              "item_shard_size": 500,
              "item_shards": [],
              "order": null
            }"#,
        )?;

        assert_eq!(manifest.generation, 0);
        Ok(())
    }
}
