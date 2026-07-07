use crate::file_sync::models::FileSyncEntry;
use sqlx::FromRow;
use std::path::PathBuf;

#[derive(Debug, Clone, FromRow)]
pub(super) struct EntryRow {
    pub(super) id: String,
    pub(super) profile_id: String,
    pub(super) origin_device_id: String,
    pub(super) kind: String,
    pub(super) display_name: String,
    pub(super) source_path: Option<String>,
    pub(super) cache_path: Option<String>,
    pub(super) total_size: i64,
    pub(super) file_count: i64,
    pub(super) revision: i64,
    pub(super) status: String,
    pub(super) confirmed: i64,
    pub(super) manifest_hash: Option<String>,
    pub(super) manifest_path: Option<String>,
    pub(super) error: Option<String>,
    pub(super) synced_at: Option<String>,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}

#[derive(Debug, Clone, FromRow)]
pub(super) struct ChunkRow {
    pub(super) entry_id: String,
    pub(super) revision: i64,
    pub(super) file_index: i64,
    pub(super) chunk_index: i64,
    pub(super) size: i64,
    pub(super) plaintext_hash: String,
    pub(super) remote_path: String,
    pub(super) staging_path: Option<String>,
    pub(super) uploaded: i64,
}

pub(super) fn public_entry(row: EntryRow, progress: i64) -> FileSyncEntry {
    FileSyncEntry {
        id: row.id,
        profile_id: row.profile_id,
        origin_device_id: row.origin_device_id,
        kind: row.kind,
        display_name: row.display_name,
        total_size: row.total_size.max(0) as u64,
        file_count: row.file_count.max(0) as u64,
        revision: row.revision,
        status: row.status,
        confirmed: row.confirmed != 0,
        progress_bytes: progress.max(0) as u64,
        error: row.error,
        synced_at: row.synced_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

pub(super) fn source_path(entry: &EntryRow) -> Result<PathBuf, String> {
    entry
        .source_path
        .as_ref()
        .map(PathBuf::from)
        .ok_or_else(|| "Local source path is unavailable".to_string())
}

pub(super) fn may_have_remote_artifacts(entry: &EntryRow, uploaded_chunk_count: i64) -> bool {
    uploaded_chunk_count > 0
        || matches!(entry.status.as_str(), "synced" | "ready" | "remote")
        || entry.synced_at.is_some()
        || (matches!(entry.status.as_str(), "failed" | "cancelled")
            && entry.manifest_hash.is_some())
}
