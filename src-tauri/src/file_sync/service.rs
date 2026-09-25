use crate::clipboard::{parse_file_list, ClipboardMonitor};
use crate::db::Db;
use crate::file_sync::models::*;
use crate::file_sync::rows::{public_entry, source_path, ChunkRow, EntryRow};
use crate::file_sync::snapshot::{
    ensure_staging_space, prepare_snapshot, scan_source, validate_top_level_source,
};
#[cfg(test)]
use crate::file_sync::snapshot::{file_identity, same_file_metadata};
use crate::file_sync::validation::*;
use crate::sync::crypto::{decrypt, encrypt};
use crate::sync::engine::SyncEngine;
use crate::sync::models::{SyncProfile, SyncProviderKind};
use crate::sync::providers::factory::ProviderFactory;
use crate::sync::providers::SyncProvider;
use crate::sync::repository::SyncRepository;
use crate::sync::secrets::SecretStore;
use secrecy::{ExposeSecret, SecretVec};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::Mutex;

const CACHE_LIMIT_BYTES: u64 = 10 * 1024 * 1024 * 1024;

pub struct FileSyncService {
    db: Db,
    sync_repository: Arc<SyncRepository>,
    sync_engine: Arc<SyncEngine>,
    provider_factory: ProviderFactory,
    clipboard: Arc<ClipboardMonitor>,
    app_handle: tauri::AppHandle,
    data_root: PathBuf,
    active_entries: Mutex<HashSet<String>>,
    cancelled_entries: Mutex<HashSet<String>>,
    active_refreshes: Mutex<HashSet<String>>,
    refresh_finished: tokio::sync::Notify,
}

impl FileSyncService {
    pub fn new(
        db: Db,
        sync_repository: Arc<SyncRepository>,
        sync_engine: Arc<SyncEngine>,
        secret_store: Arc<SecretStore>,
        clipboard: Arc<ClipboardMonitor>,
        app_handle: tauri::AppHandle,
    ) -> Result<Self, String> {
        let data_root = crate::portable::app_data_dir(&app_handle)?.join("file-sync");
        std::fs::create_dir_all(data_root.join("staging"))
            .map_err(|e| format!("Failed to create file sync staging directory: {}", e))?;
        std::fs::create_dir_all(data_root.join("cache"))
            .map_err(|e| format!("Failed to create file sync cache directory: {}", e))?;
        Ok(Self {
            db,
            sync_repository,
            sync_engine,
            provider_factory: ProviderFactory::new(secret_store),
            clipboard,
            app_handle,
            data_root,
            active_entries: Mutex::new(HashSet::new()),
            cancelled_entries: Mutex::new(HashSet::new()),
            active_refreshes: Mutex::new(HashSet::new()),
            refresh_finished: tokio::sync::Notify::new(),
        })
    }

    pub async fn config(&self) -> Result<FileSyncConfig, String> {
        let profile_id = sqlx::query_scalar::<_, Option<String>>(
            "SELECT default_profile_id FROM file_sync_settings WHERE id = 1",
        )
        .fetch_optional(&self.db)
        .await
        .map_err(db_error)?
        .flatten();
        Ok(FileSyncConfig {
            default_profile_id: profile_id,
            confirmation_threshold_bytes: CONFIRM_THRESHOLD_BYTES,
            chunk_size: CHUNK_SIZE,
        })
    }

    pub async fn set_default_profile(&self, profile_id: &str) -> Result<(), String> {
        validate_identifier(profile_id, "profile ID")?;
        self.sync_repository
            .get_profile(profile_id)
            .await
            .map_err(safe_sync_error)?;
        sqlx::query(
            r#"
            INSERT INTO file_sync_settings (id, default_profile_id, updated_at)
            VALUES (1, ?, datetime('now'))
            ON CONFLICT(id) DO UPDATE SET
                default_profile_id = excluded.default_profile_id,
                updated_at = datetime('now')
            "#,
        )
        .bind(profile_id)
        .execute(&self.db)
        .await
        .map_err(db_error)?;
        Ok(())
    }

    pub async fn profile_options(&self) -> Result<Vec<FileSyncProfileOption>, String> {
        let profiles = self
            .sync_repository
            .list_profiles()
            .await
            .map_err(safe_sync_error)?;
        Ok(profiles
            .into_iter()
            .map(|profile| FileSyncProfileOption {
                id: profile.id,
                name: profile.name,
                provider: provider_name(&profile.provider).to_string(),
                encryption_enabled: profile.encryption_enabled,
            })
            .collect())
    }

    pub async fn list_entries(
        &self,
        profile_id: Option<&str>,
    ) -> Result<Vec<FileSyncEntry>, String> {
        let rows = if let Some(profile_id) = profile_id {
            validate_identifier(profile_id, "profile ID")?;
            sqlx::query_as::<_, EntryRow>(
                r#"
                SELECT id, profile_id, origin_device_id, kind, display_name, source_path,
                       cache_path, total_size, file_count, revision, status, confirmed,
                       manifest_hash, manifest_path, error, synced_at,
                       CAST(created_at AS TEXT) AS created_at,
                       CAST(updated_at AS TEXT) AS updated_at
                FROM file_sync_entries
                WHERE profile_id = ? AND deleted_at IS NULL
                ORDER BY updated_at DESC
                LIMIT 1000
                "#,
            )
            .bind(profile_id)
            .fetch_all(&self.db)
            .await
        } else {
            sqlx::query_as::<_, EntryRow>(
                r#"
                SELECT id, profile_id, origin_device_id, kind, display_name, source_path,
                       cache_path, total_size, file_count, revision, status, confirmed,
                       manifest_hash, manifest_path, error, synced_at,
                       CAST(created_at AS TEXT) AS created_at,
                       CAST(updated_at AS TEXT) AS updated_at
                FROM file_sync_entries
                WHERE deleted_at IS NULL
                ORDER BY updated_at DESC
                LIMIT 1000
                "#,
            )
            .fetch_all(&self.db)
            .await
        }
        .map_err(db_error)?;

        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            let progress = sqlx::query_scalar::<_, i64>(
                r#"
                SELECT COALESCE(SUM(size), 0)
                FROM file_sync_chunks
                WHERE entry_id = ? AND revision = ?
                  AND CASE WHEN ? = 'downloading' THEN downloaded = 1 ELSE uploaded = 1 END
                "#,
            )
            .bind(&row.id)
            .bind(row.revision)
            .bind(&row.status)
            .fetch_one(&self.db)
            .await
            .map_err(db_error)?;
            entries.push(public_entry(row, progress));
        }
        Ok(entries)
    }

    pub async fn enqueue_clipboard_item(
        self: &Arc<Self>,
        item_id: i64,
    ) -> Result<FileSyncEnqueueResult, String> {
        if item_id <= 0 {
            return Err("Clipboard item ID must be positive".to_string());
        }
        let profile_id = self
            .config()
            .await?
            .default_profile_id
            .ok_or_else(|| "Choose a sync profile in the File Sync tab first".to_string())?;
        self.sync_repository
            .get_profile(&profile_id)
            .await
            .map_err(safe_sync_error)?;

        let row: Option<(String, String)> =
            sqlx::query_as("SELECT type, content FROM clipboard_items WHERE id = ?")
                .bind(item_id)
                .fetch_optional(&self.db)
                .await
                .map_err(db_error)?;
        let (item_type, content) =
            row.ok_or_else(|| "Clipboard item no longer exists".to_string())?;
        if !is_clipboard_file_reference(&item_type, &content) {
            return Err("Only clipboard file items can be synchronized".to_string());
        }

        let paths = parse_file_list(&content);
        if paths.is_empty() {
            return Err("Clipboard item contains no existing local files or folders".to_string());
        }
        if paths.len() > 256 {
            return Err("A clipboard item may contain at most 256 top-level paths".to_string());
        }
        let device_id = self
            .sync_repository
            .get_or_create_device_id()
            .await
            .map_err(safe_sync_error)?;

        let mut entry_ids = Vec::with_capacity(paths.len());
        for path in paths {
            let (display_name, kind) = validate_top_level_source(&path)?;
            if self
                .active_source_entry(&profile_id, &path)
                .await?
                .is_some()
            {
                return Err(format!("{} is already in File Sync", display_name));
            }
            let entry_id = uuid::Uuid::new_v4().simple().to_string();
            sqlx::query(
                r#"
                INSERT INTO file_sync_entries
                    (id, profile_id, origin_device_id, kind, display_name, source_path, status)
                VALUES (?, ?, ?, ?, ?, ?, 'queued')
                "#,
            )
            .bind(&entry_id)
            .bind(&profile_id)
            .bind(&device_id)
            .bind(kind)
            .bind(&display_name)
            .bind(path.to_string_lossy().to_string())
            .execute(&self.db)
            .await
            .map_err(db_error)?;
            entry_ids.push(entry_id.clone());
            self.emit_changed(vec![entry_id.clone()], "queued");
            self.spawn_entry(entry_id);
        }

        Ok(FileSyncEnqueueResult { entry_ids })
    }

    pub async fn clipboard_item_status(
        &self,
        item_id: i64,
    ) -> Result<FileSyncClipboardItemStatus, String> {
        if item_id <= 0 {
            return Err("Clipboard item ID must be positive".to_string());
        }
        let row: Option<(String, String)> =
            sqlx::query_as("SELECT type, content FROM clipboard_items WHERE id = ?")
                .bind(item_id)
                .fetch_optional(&self.db)
                .await
                .map_err(db_error)?;
        let Some((item_type, content)) = row else {
            return Ok(FileSyncClipboardItemStatus {
                visible: false,
                can_enqueue: false,
                reason: None,
            });
        };
        if !is_clipboard_file_reference(&item_type, &content) {
            return Ok(FileSyncClipboardItemStatus {
                visible: false,
                can_enqueue: false,
                reason: None,
            });
        }
        let paths = parse_file_list(&content);
        if paths.is_empty() || paths.len() > 256 {
            return Ok(FileSyncClipboardItemStatus {
                visible: false,
                can_enqueue: false,
                reason: None,
            });
        }
        for path in &paths {
            if validate_top_level_source(path).is_err() {
                return Ok(FileSyncClipboardItemStatus {
                    visible: false,
                    can_enqueue: false,
                    reason: None,
                });
            }
        }
        let Some(profile_id) = self.config().await?.default_profile_id else {
            return Ok(FileSyncClipboardItemStatus {
                visible: true,
                can_enqueue: false,
                reason: Some("Choose a sync profile in the File Sync tab first".to_string()),
            });
        };
        let profile = self
            .sync_repository
            .get_profile(&profile_id)
            .await
            .map_err(safe_sync_error)?;
        if profile.encryption.enabled
            && self
                .sync_engine
                .get_crypto_key(&profile_id)
                .await
                .map_err(safe_sync_error)?
                .is_none()
        {
            return Ok(FileSyncClipboardItemStatus {
                visible: true,
                can_enqueue: false,
                reason: Some("Unlock the selected encrypted sync profile first".to_string()),
            });
        }
        for path in &paths {
            if let Some(status) = self.active_source_entry(&profile_id, path).await? {
                return Ok(FileSyncClipboardItemStatus {
                    visible: true,
                    can_enqueue: false,
                    reason: Some(format!("Already in File Sync ({})", status)),
                });
            }
        }
        Ok(FileSyncClipboardItemStatus {
            visible: true,
            can_enqueue: true,
            reason: None,
        })
    }

    async fn active_source_entry(
        &self,
        profile_id: &str,
        path: &Path,
    ) -> Result<Option<String>, String> {
        sqlx::query_scalar(
            r#"
            SELECT status
            FROM file_sync_entries
            WHERE profile_id = ? AND source_path = ? AND deleted_at IS NULL
              AND status NOT IN ('failed', 'cancelled', 'deleted')
            ORDER BY updated_at DESC
            LIMIT 1
            "#,
        )
        .bind(profile_id)
        .bind(path.to_string_lossy().to_string())
        .fetch_optional(&self.db)
        .await
        .map_err(db_error)
    }

    pub async fn confirm(self: &Arc<Self>, entry_id: &str) -> Result<(), String> {
        validate_identifier(entry_id, "entry ID")?;
        let result = sqlx::query(
            r#"
            UPDATE file_sync_entries
            SET confirmed = 1, status = 'preparing', error = NULL, updated_at = datetime('now')
            WHERE id = ? AND status = 'awaiting_confirmation' AND deleted_at IS NULL
            "#,
        )
        .bind(entry_id)
        .execute(&self.db)
        .await
        .map_err(db_error)?;
        if result.rows_affected() != 1 {
            return Err("Entry is not waiting for confirmation".to_string());
        }
        self.emit_changed(vec![entry_id.to_string()], "confirmed");
        self.spawn_entry(entry_id.to_string());
        Ok(())
    }

    pub async fn retry(self: &Arc<Self>, entry_id: &str) -> Result<(), String> {
        validate_identifier(entry_id, "entry ID")?;
        let has_chunks = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM file_sync_chunks WHERE entry_id = ?",
        )
        .bind(entry_id)
        .fetch_one(&self.db)
        .await
        .map_err(db_error)?
            > 0;
        let next_status = if has_chunks { "uploading" } else { "queued" };
        let result = sqlx::query(
            "UPDATE file_sync_entries SET status = ?, error = NULL, updated_at = datetime('now') WHERE id = ? AND status IN ('failed', 'cancelled') AND deleted_at IS NULL",
        )
        .bind(next_status)
        .bind(entry_id)
        .execute(&self.db)
        .await
        .map_err(db_error)?;
        if result.rows_affected() != 1 {
            return Err("Only failed or cancelled entries can be retried".to_string());
        }
        self.cancelled_entries.lock().await.remove(entry_id);
        self.spawn_entry(entry_id.to_string());
        Ok(())
    }

    pub async fn cancel(&self, entry_id: &str) -> Result<(), String> {
        validate_identifier(entry_id, "entry ID")?;
        let result = sqlx::query(
            r#"
            UPDATE file_sync_entries
            SET status = CASE
                    WHEN status = 'downloading' AND source_path IS NULL THEN 'remote'
                    WHEN status = 'downloading' THEN 'synced'
                    ELSE 'cancelled'
                END,
                error = NULL,
                updated_at = datetime('now')
            WHERE id = ? AND status IN ('queued', 'scanning', 'preparing', 'uploading', 'downloading')
              AND deleted_at IS NULL
            "#,
        )
        .bind(entry_id)
        .execute(&self.db)
        .await
        .map_err(db_error)?;
        if result.rows_affected() != 1 {
            return Err("Only an active upload task can be cancelled".to_string());
        }
        self.cancelled_entries
            .lock()
            .await
            .insert(entry_id.to_string());
        self.emit_changed(vec![entry_id.to_string()], "cancelled");
        Ok(())
    }

    pub async fn refresh(&self, profile_id: &str) -> Result<(), String> {
        validate_identifier(profile_id, "profile ID")?;
        loop {
            let notified = self.refresh_finished.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self
                .active_refreshes
                .lock()
                .await
                .insert(profile_id.to_string())
            {
                break;
            }
            notified.await;
        }
        let result = self.refresh_inner(profile_id).await;
        self.active_refreshes.lock().await.remove(profile_id);
        self.refresh_finished.notify_waiters();
        result
    }

    async fn refresh_inner(&self, profile_id: &str) -> Result<(), String> {
        let profile = self
            .sync_repository
            .get_profile(profile_id)
            .await
            .map_err(safe_sync_error)?;
        let provider = self
            .provider_factory
            .build(&profile)
            .await
            .map_err(safe_sync_error)?;
        let key = self.crypto_key(&profile).await?;
        // Delete work survives app restarts and remains queued until remote cleanup succeeds.
        // Opening File Sync may inspect remote files while clipboard sync is
        // paused. Do not publish queued deletions until the profile resumes.
        let delete_result = if profile.schedule.paused {
            Ok(())
        } else {
            flush_pending_deletes(&self.db, profile_id, provider.as_ref(), key.as_ref()).await
        };
        if !profile.schedule.paused {
            provider
                .mkdir_all(&format!("{}/changes", REMOTE_ROOT))
                .await
                .map_err(safe_sync_error)?;
        }
        let pull_result =
            pull_file_events(&self.db, profile_id, provider.as_ref(), key.as_ref()).await;
        // A partial pull may already have applied some events. Always refresh the view.
        self.emit_changed(Vec::new(), "remote-refresh");
        match (delete_result, pull_result) {
            (Err(a), Err(b)) => Err(format!("{}; {}", a, b)),
            (Err(error), _) | (_, Err(error)) => Err(error),
            _ => Ok(()),
        }
    }

    pub async fn run_background_loop(self: Arc<Self>) {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(30));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            let profiles = sqlx::query_scalar::<_, String>(
                "SELECT default_profile_id FROM file_sync_settings WHERE default_profile_id IS NOT NULL UNION SELECT profile_id FROM file_sync_entries WHERE deleted_at IS NULL UNION SELECT profile_id FROM file_sync_pending_deletes"
            ).fetch_all(&self.db).await;
            match profiles {
                Ok(profiles) => {
                    for id in profiles {
                        if let Ok(profile) = self.sync_repository.get_profile(&id).await {
                            if profile.schedule.paused
                                || (profile.encryption.enabled
                                    && !self.sync_engine.is_profile_unlocked(&id).await)
                            {
                                continue;
                            }
                        }
                        if let Err(error) = self.refresh(&id).await {
                            log::warn!("[FileSync] Background refresh failed: {}", error);
                        }
                    }
                }
                Err(error) => log::warn!("[FileSync] Failed to load refresh profiles: {}", error),
            }
        }
    }

    pub async fn copy_entries(&self, entry_ids: Vec<String>) -> Result<(), String> {
        if entry_ids.is_empty() || entry_ids.len() > MAX_COPY_ENTRIES {
            return Err(format!(
                "Select between 1 and {} file sync entries",
                MAX_COPY_ENTRIES
            ));
        }
        let mut unique = HashSet::new();
        let mut paths = Vec::with_capacity(entry_ids.len());
        for entry_id in entry_ids {
            validate_identifier(&entry_id, "entry ID")?;
            if !unique.insert(entry_id.clone()) {
                continue;
            }
            match self.materialize_entry(&entry_id).await {
                Ok(path) => paths.push(path),
                Err(error) => {
                    if error != CANCELLED_ERROR {
                        let _ = sqlx::query(
                            r#"
                            UPDATE file_sync_entries
                            SET status = CASE WHEN source_path IS NULL THEN 'remote' ELSE 'synced' END,
                                error = ?,
                                updated_at = datetime('now')
                            WHERE id = ?
                            "#,
                        )
                        .bind(&error)
                        .bind(&entry_id)
                        .execute(&self.db)
                        .await;
                        self.emit_changed(vec![entry_id], "download-failed");
                    }
                    return Err(error);
                }
            }
        }
        let clipboard_value = paths
            .iter()
            .map(|path| path.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n");
        self.clipboard
            .write_files(&clipboard_value)
            .await
            .map_err(|_| "Failed to write downloaded files to the system clipboard".to_string())
    }

    pub async fn delete_entry(&self, entry_id: &str) -> Result<(), String> {
        validate_identifier(entry_id, "entry ID")?;
        // Claim the entry without holding a mutex during I/O. An upload finishing
        // after deletion could otherwise recreate chunks that were just removed.
        if !self
            .active_entries
            .lock()
            .await
            .insert(entry_id.to_string())
        {
            return Err("Cancel or finish the active transfer before deleting it".to_string());
        }
        let result = async {
            let entry = self.load_entry(entry_id).await?;
            let device_id = self
                .sync_repository
                .get_or_create_device_id()
                .await
                .map_err(safe_sync_error)?;
            let seq = self.next_sequence(&entry.profile_id, &device_id).await?;
            let event = FileSyncRemoteEvent {
                schema_version: FILE_SYNC_SCHEMA_VERSION,
                device_id,
                seq,
                operation: "delete".into(),
                entry_id: entry.id.clone(),
                revision: entry.revision,
                changed_at: chrono::Utc::now().to_rfc3339(),
                entry: None,
            };
            queue_entry_deletion(&self.db, &entry.profile_id, &event).await?;
            self.cleanup_local_entry_files(entry_id).await;
            self.emit_changed(vec![entry_id.to_string()], "deleted");
            if let Err(error) = self.refresh(&entry.profile_id).await {
                log::warn!("[FileSync] Refresh after deletion failed: {}", error);
                let pending: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM file_sync_pending_deletes WHERE profile_id = ? AND entry_id = ?)")
                    .bind(&entry.profile_id).bind(entry_id).fetch_one(&self.db).await.map_err(db_error)?;
                if pending {
                    return Err("Item removed locally; remote deletion is queued and will retry automatically when the profile is online and unlocked".to_string());
                }
            }
            Ok(())
        }
        .await;
        self.active_entries.lock().await.remove(entry_id);
        result
    }

    async fn cleanup_local_entry_files(&self, entry_id: &str) {
        for directory in [
            self.data_root.join("staging").join(entry_id),
            self.data_root.join("cache").join(entry_id),
        ] {
            if directory.exists() {
                let _ = tokio::fs::remove_dir_all(directory).await;
            }
        }
    }

    pub async fn resume_pending(self: &Arc<Self>) {
        if let Err(error) = self.cleanup_cache().await {
            log::warn!("[FileSync] Cache cleanup failed: {}", error);
        }
        if let Err(error) = sqlx::query(
            r#"
            UPDATE file_sync_entries
            SET status = CASE WHEN source_path IS NULL THEN 'remote' ELSE 'synced' END,
                updated_at = datetime('now')
            WHERE status = 'downloading' AND deleted_at IS NULL
            "#,
        )
        .execute(&self.db)
        .await
        {
            log::warn!(
                "[FileSync] Failed to reset interrupted downloads: {}",
                error
            );
        }
        let ids = match sqlx::query_scalar::<_, String>(
            "SELECT id FROM file_sync_entries WHERE status IN ('queued', 'scanning', 'preparing', 'uploading') AND deleted_at IS NULL",
        )
        .fetch_all(&self.db)
        .await
        {
            Ok(ids) => ids,
            Err(error) => {
                log::warn!("[FileSync] Failed to load resumable tasks: {}", error);
                return;
            }
        };
        for id in ids {
            self.spawn_entry(id);
        }
    }

    fn spawn_entry(self: &Arc<Self>, entry_id: String) {
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            {
                let mut active = service.active_entries.lock().await;
                if !active.insert(entry_id.clone()) {
                    return;
                }
            }
            let result = service.process_entry(&entry_id).await;
            {
                let mut active = service.active_entries.lock().await;
                active.remove(&entry_id);
            }
            service.cancelled_entries.lock().await.remove(&entry_id);
            if let Err(error) = result {
                if error == CANCELLED_ERROR {
                    service.emit_changed(vec![entry_id], "cancelled");
                } else {
                    log::warn!("[FileSync] Entry {} failed: {}", entry_id, error);
                    let _ = sqlx::query(
                        "UPDATE file_sync_entries SET status = 'failed', error = ?, updated_at = datetime('now') WHERE id = ? AND status != 'cancelled' AND deleted_at IS NULL",
                    )
                    .bind(&error)
                    .bind(&entry_id)
                    .execute(&service.db)
                    .await;
                    service.emit_changed(vec![entry_id], "failed");
                }
            }
        });
    }

    async fn process_entry(&self, entry_id: &str) -> Result<(), String> {
        self.ensure_not_cancelled(entry_id).await?;
        let mut entry = self.load_entry(entry_id).await?;
        if matches!(entry.status.as_str(), "queued" | "scanning") {
            self.set_status(entry_id, "scanning", None).await?;
            let source = source_path(&entry)?;
            let scan = tokio::task::spawn_blocking(move || scan_source(&source))
                .await
                .map_err(|_| "File scan task failed".to_string())??;
            self.ensure_not_cancelled(entry_id).await?;
            sqlx::query(
                "UPDATE file_sync_entries SET kind = ?, display_name = ?, total_size = ?, file_count = ?, status = ?, error = NULL, updated_at = datetime('now') WHERE id = ? AND status != 'cancelled'",
            )
            .bind(&scan.kind)
            .bind(&scan.display_name)
            .bind(to_i64(scan.total_size, "entry size")?)
            .bind(to_i64(scan.file_count, "file count")?)
            .bind(if scan.total_size > CONFIRM_THRESHOLD_BYTES && entry.confirmed == 0 {
                "awaiting_confirmation"
            } else {
                "preparing"
            })
            .bind(entry_id)
            .execute(&self.db)
            .await
            .map_err(db_error)?;
            self.ensure_not_cancelled(entry_id).await?;
            self.emit_changed(vec![entry_id.to_string()], "scanned");
            if scan.total_size > CONFIRM_THRESHOLD_BYTES && entry.confirmed == 0 {
                return Ok(());
            }
            entry = self.load_entry(entry_id).await?;
        }

        if entry.status == "awaiting_confirmation" {
            return Ok(());
        }

        let chunk_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM file_sync_chunks WHERE entry_id = ? AND revision = ?",
        )
        .bind(entry_id)
        .bind(entry.revision)
        .fetch_one(&self.db)
        .await
        .map_err(db_error)?;

        if chunk_count == 0 || entry.manifest_path.is_none() {
            self.set_status(entry_id, "preparing", None).await?;
            ensure_staging_space(&self.data_root, entry.total_size.max(0) as u64)?;
            let source = source_path(&entry)?;
            let staging_root = self
                .data_root
                .join("staging")
                .join(entry_id)
                .join(entry.revision.to_string());
            let profile = self
                .sync_repository
                .get_profile(&entry.profile_id)
                .await
                .map_err(safe_sync_error)?;
            let chunk_size = if profile.provider == SyncProviderKind::GoogleDrive {
                GOOGLE_DRIVE_CHUNK_SIZE
            } else {
                CHUNK_SIZE
            };
            let entry_id_owned = entry.id.clone();
            let revision = entry.revision;
            let prepared = tokio::task::spawn_blocking(move || {
                prepare_snapshot(
                    &entry_id_owned,
                    revision,
                    &source,
                    &staging_root,
                    chunk_size,
                )
            })
            .await
            .map_err(|_| "Snapshot preparation task failed".to_string())??;
            self.ensure_not_cancelled(entry_id).await?;
            self.save_prepared(entry_id, entry.revision, &prepared)
                .await?;
            self.emit_changed(vec![entry_id.to_string()], "prepared");
            entry = self.load_entry(entry_id).await?;
        }

        self.upload_entry(&entry).await
    }

    async fn save_prepared(
        &self,
        entry_id: &str,
        revision: i64,
        prepared: &PreparedSnapshot,
    ) -> Result<(), String> {
        let mut transaction = self.db.begin().await.map_err(db_error)?;
        sqlx::query("DELETE FROM file_sync_chunks WHERE entry_id = ? AND revision = ?")
            .bind(entry_id)
            .bind(revision)
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
        for chunk in &prepared.chunks {
            sqlx::query(
                r#"
                INSERT INTO file_sync_chunks
                    (entry_id, revision, file_index, chunk_index, relative_path, size,
                     plaintext_hash, remote_path, staging_path)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(entry_id)
            .bind(revision)
            .bind(chunk.file_index)
            .bind(chunk.chunk_index)
            .bind(&chunk.relative_path)
            .bind(to_i64(chunk.size, "chunk size")?)
            .bind(&chunk.plaintext_hash)
            .bind(&chunk.remote_path)
            .bind(chunk.staging_path.to_string_lossy().to_string())
            .execute(&mut *transaction)
            .await
            .map_err(db_error)?;
        }
        sqlx::query(
            "UPDATE file_sync_entries SET status = 'uploading', manifest_hash = ?, manifest_path = ?, error = NULL, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(&prepared.manifest_hash)
        .bind(prepared.manifest_path.to_string_lossy().to_string())
        .bind(entry_id)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
        transaction.commit().await.map_err(db_error)
    }

    async fn upload_entry(&self, entry: &EntryRow) -> Result<(), String> {
        self.ensure_not_cancelled(&entry.id).await?;
        self.set_status(&entry.id, "uploading", None).await?;
        let profile = self
            .sync_repository
            .get_profile(&entry.profile_id)
            .await
            .map_err(safe_sync_error)?;
        let provider = self
            .provider_factory
            .build(&profile)
            .await
            .map_err(safe_sync_error)?;
        let key = self.crypto_key(&profile).await?;
        let chunks = sqlx::query_as::<_, ChunkRow>(
            r#"
            SELECT entry_id, revision, file_index, chunk_index, size,
                   plaintext_hash, remote_path, staging_path, uploaded
            FROM file_sync_chunks
            WHERE entry_id = ? AND revision = ?
            ORDER BY file_index, chunk_index
            "#,
        )
        .bind(&entry.id)
        .bind(entry.revision)
        .fetch_all(&self.db)
        .await
        .map_err(db_error)?;
        let mut completed = chunks
            .iter()
            .filter(|chunk| chunk.uploaded != 0)
            .map(|chunk| chunk.size.max(0) as u64)
            .sum();

        for chunk in chunks {
            self.ensure_not_cancelled(&entry.id).await?;
            if chunk.uploaded != 0 {
                continue;
            }
            let staging_path = chunk
                .staging_path
                .as_ref()
                .map(PathBuf::from)
                .ok_or_else(|| "A staged chunk is missing".to_string())?;
            let bytes = tokio::fs::read(&staging_path)
                .await
                .map_err(|_| "A staged chunk could not be read".to_string())?;
            if sha256_hex(&bytes) != chunk.plaintext_hash {
                return Err("A staged chunk failed its integrity check".to_string());
            }
            let encoded = encode_remote_bytes(bytes, key.as_ref(), &chunk.remote_path)?;
            if let Some(parent) = remote_parent(&chunk.remote_path) {
                provider.mkdir_all(parent).await.map_err(safe_sync_error)?;
            }
            provider
                .put(&chunk.remote_path, encoded)
                .await
                .map_err(safe_sync_error)?;
            sqlx::query(
                "UPDATE file_sync_chunks SET uploaded = 1, updated_at = datetime('now') WHERE entry_id = ? AND revision = ? AND file_index = ? AND chunk_index = ?",
            )
            .bind(&chunk.entry_id)
            .bind(chunk.revision)
            .bind(chunk.file_index)
            .bind(chunk.chunk_index)
            .execute(&self.db)
            .await
            .map_err(db_error)?;
            completed += chunk.size.max(0) as u64;
            self.emit_progress(
                &entry.id,
                "uploading",
                completed,
                entry.total_size.max(0) as u64,
            );
        }

        let manifest_local = entry
            .manifest_path
            .as_ref()
            .map(PathBuf::from)
            .ok_or_else(|| "Prepared manifest is missing".to_string())?;
        let manifest_bytes = tokio::fs::read(&manifest_local)
            .await
            .map_err(|_| "Prepared manifest could not be read".to_string())?;
        let manifest_hash = sha256_hex(&manifest_bytes);
        if entry.manifest_hash.as_deref() != Some(manifest_hash.as_str()) {
            return Err("Prepared manifest failed its integrity check".to_string());
        }
        let remote_manifest_path = format!(
            "{}/entries/{}/{}/manifest.json{}",
            REMOTE_ROOT,
            entry.id,
            entry.revision,
            if key.is_some() { ".enc" } else { "" }
        );
        provider
            .mkdir_all(&format!(
                "{}/entries/{}/{}",
                REMOTE_ROOT, entry.id, entry.revision
            ))
            .await
            .map_err(safe_sync_error)?;
        provider
            .put(
                &remote_manifest_path,
                encode_remote_bytes(manifest_bytes, key.as_ref(), &remote_manifest_path)?,
            )
            .await
            .map_err(safe_sync_error)?;

        let seq = self
            .next_sequence(&entry.profile_id, &entry.origin_device_id)
            .await?;
        let synced_at = chrono::Utc::now().to_rfc3339();
        let event = FileSyncRemoteEvent {
            schema_version: FILE_SYNC_SCHEMA_VERSION,
            device_id: entry.origin_device_id.clone(),
            seq,
            operation: "upsert".to_string(),
            entry_id: entry.id.clone(),
            revision: entry.revision,
            changed_at: synced_at.clone(),
            entry: Some(RemoteEntrySummary {
                id: entry.id.clone(),
                origin_device_id: entry.origin_device_id.clone(),
                kind: entry.kind.clone(),
                display_name: entry.display_name.clone(),
                total_size: entry.total_size.max(0) as u64,
                file_count: entry.file_count.max(0) as u64,
                revision: entry.revision,
                manifest_path: remote_manifest_path,
                manifest_hash,
                synced_at: synced_at.clone(),
            }),
        };
        let event_path = self
            .publish_event(provider.as_ref(), &event, key.as_ref())
            .await?;
        self.store_cursor(&entry.profile_id, &entry.origin_device_id, seq)
            .await?;
        sqlx::query(
            "UPDATE file_sync_entries SET status = 'synced', remote_event_path = ?, synced_at = ?, error = NULL, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(event_path)
        .bind(&synced_at)
        .bind(&entry.id)
        .execute(&self.db)
        .await
        .map_err(db_error)?;
        self.emit_progress(
            &entry.id,
            "synced",
            entry.total_size.max(0) as u64,
            entry.total_size.max(0) as u64,
        );
        self.emit_changed(vec![entry.id.clone()], "synced");
        if let Some(manifest_path) = &entry.manifest_path {
            if let Some(staging_root) = Path::new(manifest_path).parent() {
                if let Err(error) = tokio::fs::remove_dir_all(staging_root).await {
                    log::warn!(
                        "[FileSync] Failed to reclaim staging data for {}: {}",
                        entry.id,
                        error
                    );
                } else {
                    let _ = sqlx::query(
                        "UPDATE file_sync_chunks SET staging_path = NULL WHERE entry_id = ? AND revision = ?",
                    )
                    .bind(&entry.id)
                    .bind(entry.revision)
                    .execute(&self.db)
                    .await;
                }
            }
        }
        Ok(())
    }

    async fn materialize_entry(&self, entry_id: &str) -> Result<PathBuf, String> {
        {
            let mut active = self.active_entries.lock().await;
            if !active.insert(entry_id.to_string()) {
                return Err("A transfer is already running for this entry".to_string());
            }
        }
        let result = self.materialize_entry_inner(entry_id).await;
        self.active_entries.lock().await.remove(entry_id);
        self.cancelled_entries.lock().await.remove(entry_id);
        result
    }

    async fn materialize_entry_inner(&self, entry_id: &str) -> Result<PathBuf, String> {
        self.ensure_not_cancelled(entry_id).await?;
        let entry = self.load_entry(entry_id).await?;
        if let Some(cache_path) = &entry.cache_path {
            let path = PathBuf::from(cache_path);
            if path.exists() {
                sqlx::query(
                    "UPDATE file_sync_cache SET last_accessed = datetime('now'), protected_until = datetime('now', '+1 day') WHERE entry_id = ? AND revision = ?",
                )
                .bind(entry_id)
                .bind(entry.revision)
                .execute(&self.db)
                .await
                .map_err(db_error)?;
                return Ok(path);
            }
        }
        if entry.status != "synced" && entry.status != "remote" && entry.status != "ready" {
            return Err("Entry is not available for download yet".to_string());
        }
        self.set_status(entry_id, "downloading", None).await?;
        self.emit_changed(vec![entry_id.to_string()], "downloading");
        self.ensure_not_cancelled(entry_id).await?;

        let profile = self
            .sync_repository
            .get_profile(&entry.profile_id)
            .await
            .map_err(safe_sync_error)?;
        let provider = self
            .provider_factory
            .build(&profile)
            .await
            .map_err(safe_sync_error)?;
        let key = self.crypto_key(&profile).await?;
        let remote_manifest_path = format!(
            "{}/entries/{}/{}/manifest.json{}",
            REMOTE_ROOT,
            entry.id,
            entry.revision,
            if key.is_some() { ".enc" } else { "" }
        );
        let encoded = provider
            .get(&remote_manifest_path)
            .await
            .map_err(safe_sync_error)?;
        self.ensure_not_cancelled(entry_id).await?;
        let manifest_bytes = decode_remote_bytes(encoded, key.as_ref(), &remote_manifest_path)?;
        if entry.manifest_hash.as_deref() != Some(sha256_hex(&manifest_bytes).as_str()) {
            return Err("Remote manifest failed its integrity check".to_string());
        }
        let manifest: FileSyncManifest = serde_json::from_slice(&manifest_bytes)
            .map_err(|_| "Remote manifest is invalid".to_string())?;
        validate_manifest(&manifest, &entry)?;
        sqlx::query(
            "UPDATE file_sync_chunks SET downloaded = 0 WHERE entry_id = ? AND revision = ?",
        )
        .bind(entry_id)
        .bind(entry.revision)
        .execute(&self.db)
        .await
        .map_err(db_error)?;

        let cache_parent = self
            .data_root
            .join("cache")
            .join(&entry.id)
            .join(entry.revision.to_string());
        let temp_parent = self
            .data_root
            .join("cache")
            .join(&entry.id)
            .join(format!(".{}.tmp", entry.revision));
        tokio::fs::create_dir_all(&temp_parent)
            .await
            .map_err(|_| "Failed to create download cache".to_string())?;
        let root = safe_join(&temp_parent, &manifest.display_name)?;
        if manifest.kind == "folder" {
            tokio::fs::create_dir_all(&root)
                .await
                .map_err(|_| "Failed to create cached folder".to_string())?;
        }

        let mut completed = 0u64;
        let mut file_index = 0i64;
        for node in &manifest.nodes {
            let destination = if manifest.kind == "file" || node.path.is_empty() {
                root.clone()
            } else {
                safe_join(&root, &node.path)?
            };
            if node.kind == "directory" {
                tokio::fs::create_dir_all(&destination)
                    .await
                    .map_err(|_| "Failed to create cached directory".to_string())?;
                continue;
            }
            if let Some(parent) = destination.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|_| "Failed to create cached file parent".to_string())?;
            }
            for chunk in &node.chunks {
                self.ensure_not_cancelled(entry_id).await?;
                let chunk_path = temp_parent
                    .join(".chunks")
                    .join(file_index.to_string())
                    .join(format!("{}.bin", chunk.index));
                let bytes = match tokio::fs::read(&chunk_path).await {
                    Ok(existing)
                        if existing.len() as u64 == chunk.size
                            && sha256_hex(&existing) == chunk.sha256 =>
                    {
                        existing
                    }
                    _ => {
                        let encoded = provider
                            .get(&chunk.remote_path)
                            .await
                            .map_err(safe_sync_error)?;
                        self.ensure_not_cancelled(entry_id).await?;
                        let downloaded =
                            decode_remote_bytes(encoded, key.as_ref(), &chunk.remote_path)?;
                        if downloaded.len() as u64 != chunk.size
                            || sha256_hex(&downloaded) != chunk.sha256
                        {
                            return Err("Downloaded chunk failed its integrity check".to_string());
                        }
                        if let Some(parent) = chunk_path.parent() {
                            tokio::fs::create_dir_all(parent).await.map_err(|_| {
                                "Failed to create resumable download directory".to_string()
                            })?;
                        }
                        let part_path = chunk_path.with_extension("part");
                        tokio::fs::write(&part_path, &downloaded)
                            .await
                            .map_err(|_| "Failed to persist downloaded chunk".to_string())?;
                        tokio::fs::rename(&part_path, &chunk_path)
                            .await
                            .map_err(|_| "Failed to finalize downloaded chunk".to_string())?;
                        downloaded
                    }
                };
                sqlx::query(
                    r#"
                    INSERT INTO file_sync_chunks
                        (entry_id, revision, file_index, chunk_index, relative_path, size,
                         plaintext_hash, remote_path, staging_path, downloaded)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 1)
                    ON CONFLICT(entry_id, revision, file_index, chunk_index) DO UPDATE SET
                        relative_path = excluded.relative_path,
                        size = excluded.size,
                        plaintext_hash = excluded.plaintext_hash,
                        remote_path = excluded.remote_path,
                        staging_path = excluded.staging_path,
                        downloaded = 1,
                        updated_at = datetime('now')
                    "#,
                )
                .bind(entry_id)
                .bind(entry.revision)
                .bind(file_index)
                .bind(i64::from(chunk.index))
                .bind(&node.path)
                .bind(to_i64(chunk.size, "downloaded chunk size")?)
                .bind(&chunk.sha256)
                .bind(&chunk.remote_path)
                .bind(chunk_path.to_string_lossy().to_string())
                .execute(&self.db)
                .await
                .map_err(db_error)?;
                completed += bytes.len() as u64;
                self.emit_progress(entry_id, "downloading", completed, manifest.total_size);
            }
            let mut output = tokio::fs::File::create(&destination)
                .await
                .map_err(|_| "Failed to create cached file".to_string())?;
            for chunk in &node.chunks {
                self.ensure_not_cancelled(entry_id).await?;
                let chunk_path = temp_parent
                    .join(".chunks")
                    .join(file_index.to_string())
                    .join(format!("{}.bin", chunk.index));
                let bytes = tokio::fs::read(&chunk_path)
                    .await
                    .map_err(|_| "A downloaded chunk disappeared before assembly".to_string())?;
                if bytes.len() as u64 != chunk.size || sha256_hex(&bytes) != chunk.sha256 {
                    return Err("A cached download chunk failed revalidation".to_string());
                }
                tokio::io::AsyncWriteExt::write_all(&mut output, &bytes)
                    .await
                    .map_err(|_| "Failed to write cached file".to_string())?;
            }
            file_index += 1;
        }
        let chunk_cache = temp_parent.join(".chunks");
        if chunk_cache.exists() {
            tokio::fs::remove_dir_all(&chunk_cache)
                .await
                .map_err(|_| "Failed to clean completed download chunks".to_string())?;
        }

        if cache_parent.exists() {
            tokio::fs::remove_dir_all(&cache_parent)
                .await
                .map_err(|_| "Failed to replace old download cache".to_string())?;
        }
        self.ensure_not_cancelled(entry_id).await?;
        tokio::fs::rename(&temp_parent, &cache_parent)
            .await
            .map_err(|_| "Failed to finalize download cache".to_string())?;
        let final_path = cache_parent.join(&manifest.display_name);
        sqlx::query(
            "UPDATE file_sync_entries SET cache_path = ?, status = 'ready', error = NULL, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(final_path.to_string_lossy().to_string())
        .bind(entry_id)
        .execute(&self.db)
        .await
        .map_err(db_error)?;
        sqlx::query(
            r#"
            INSERT INTO file_sync_cache
                (entry_id, revision, path, size, last_accessed, protected_until)
            VALUES (?, ?, ?, ?, datetime('now'), datetime('now', '+1 day'))
            ON CONFLICT(entry_id, revision) DO UPDATE SET
                path = excluded.path,
                size = excluded.size,
                last_accessed = datetime('now'),
                protected_until = datetime('now', '+1 day')
            "#,
        )
        .bind(entry_id)
        .bind(entry.revision)
        .bind(final_path.to_string_lossy().to_string())
        .bind(to_i64(manifest.total_size, "cache size")?)
        .execute(&self.db)
        .await
        .map_err(db_error)?;
        if let Err(error) = self.cleanup_cache().await {
            log::warn!("[FileSync] Cache cleanup after download failed: {}", error);
        }
        self.emit_changed(vec![entry_id.to_string()], "ready");
        Ok(final_path)
    }

    async fn publish_event(
        &self,
        provider: &dyn SyncProvider,
        event: &FileSyncRemoteEvent,
        key: Option<&SecretVec<u8>>,
    ) -> Result<String, String> {
        publish_file_event(provider, event, key).await
    }

    async fn next_sequence(&self, profile_id: &str, device_id: &str) -> Result<i64, String> {
        let mut transaction = self.db.begin().await.map_err(db_error)?;
        sqlx::query(
            r#"
            INSERT INTO file_sync_profile_seq (profile_id, device_id, last_seq, updated_at)
            VALUES (?, ?, 0, datetime('now'))
            ON CONFLICT(profile_id, device_id) DO NOTHING
            "#,
        )
        .bind(profile_id)
        .bind(device_id)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
        sqlx::query(
            "UPDATE file_sync_profile_seq SET last_seq = last_seq + 1, updated_at = datetime('now') WHERE profile_id = ? AND device_id = ?",
        )
        .bind(profile_id)
        .bind(device_id)
        .execute(&mut *transaction)
        .await
        .map_err(db_error)?;
        let seq = sqlx::query_scalar::<_, i64>(
            "SELECT last_seq FROM file_sync_profile_seq WHERE profile_id = ? AND device_id = ?",
        )
        .bind(profile_id)
        .bind(device_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(db_error)?;
        transaction.commit().await.map_err(db_error)?;
        Ok(seq)
    }

    async fn store_cursor(
        &self,
        profile_id: &str,
        device_id: &str,
        seq: i64,
    ) -> Result<(), String> {
        sqlx::query(
            r#"
            INSERT INTO file_sync_remote_cursors
                (profile_id, remote_device_id, last_seq, updated_at)
            VALUES (?, ?, ?, datetime('now'))
            ON CONFLICT(profile_id, remote_device_id) DO UPDATE SET
                last_seq = MAX(file_sync_remote_cursors.last_seq, excluded.last_seq),
                updated_at = datetime('now')
            "#,
        )
        .bind(profile_id)
        .bind(device_id)
        .bind(seq)
        .execute(&self.db)
        .await
        .map_err(db_error)?;
        Ok(())
    }

    async fn crypto_key(&self, profile: &SyncProfile) -> Result<Option<SecretVec<u8>>, String> {
        if !profile.encryption.enabled {
            return Ok(None);
        }
        self.sync_engine
            .get_crypto_key(&profile.id)
            .await
            .map_err(safe_sync_error)?
            .ok_or_else(|| "The selected encrypted sync profile is locked".to_string())
            .map(Some)
    }

    async fn load_entry(&self, entry_id: &str) -> Result<EntryRow, String> {
        sqlx::query_as::<_, EntryRow>(
            r#"
            SELECT id, profile_id, origin_device_id, kind, display_name, source_path,
                   cache_path, total_size, file_count, revision, status, confirmed,
                   manifest_hash, manifest_path, error, synced_at,
                   CAST(created_at AS TEXT) AS created_at,
                   CAST(updated_at AS TEXT) AS updated_at
            FROM file_sync_entries
            WHERE id = ? AND deleted_at IS NULL
            "#,
        )
        .bind(entry_id)
        .fetch_optional(&self.db)
        .await
        .map_err(db_error)?
        .ok_or_else(|| "File sync entry was not found".to_string())
    }

    async fn set_status(
        &self,
        entry_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<(), String> {
        sqlx::query(
            "UPDATE file_sync_entries SET status = ?, error = ?, updated_at = datetime('now') WHERE id = ? AND status != 'cancelled' AND deleted_at IS NULL",
        )
        .bind(status)
        .bind(error)
        .bind(entry_id)
        .execute(&self.db)
        .await
        .map_err(db_error)?;
        self.emit_changed(vec![entry_id.to_string()], status);
        Ok(())
    }

    async fn cleanup_cache(&self) -> Result<(), String> {
        let rows: Vec<(String, i64, String, i64, i64, i64)> = sqlx::query_as(
            r#"
            SELECT entry_id, revision, path, size,
                   CASE WHEN last_accessed < datetime('now', '-7 days') THEN 1 ELSE 0 END,
                   CASE WHEN protected_until > datetime('now') THEN 1 ELSE 0 END
            FROM file_sync_cache
            ORDER BY last_accessed ASC
            "#,
        )
        .fetch_all(&self.db)
        .await
        .map_err(db_error)?;
        let mut total: u64 = rows.iter().map(|row| row.3.max(0) as u64).sum();
        for (entry_id, revision, path, size, expired, protected) in rows {
            let missing = !Path::new(&path).exists();
            let over_limit = total > CACHE_LIMIT_BYTES;
            if !missing && protected != 0 {
                continue;
            }
            if !missing && expired == 0 && !over_limit {
                continue;
            }
            let cache_path = PathBuf::from(&path);
            let cache_root = cache_path
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or(cache_path);
            if cache_root.exists() {
                tokio::fs::remove_dir_all(&cache_root)
                    .await
                    .map_err(|_| "Failed to remove expired file sync cache".to_string())?;
            }
            sqlx::query("DELETE FROM file_sync_cache WHERE entry_id = ? AND revision = ?")
                .bind(&entry_id)
                .bind(revision)
                .execute(&self.db)
                .await
                .map_err(db_error)?;
            sqlx::query(
                r#"
                UPDATE file_sync_entries
                SET cache_path = NULL,
                    status = CASE WHEN source_path IS NULL THEN 'remote' ELSE 'synced' END,
                    updated_at = datetime('now')
                WHERE id = ? AND revision = ?
                "#,
            )
            .bind(&entry_id)
            .bind(revision)
            .execute(&self.db)
            .await
            .map_err(db_error)?;
            sqlx::query(
                "UPDATE file_sync_chunks SET downloaded = 0, staging_path = NULL WHERE entry_id = ? AND revision = ?",
            )
            .bind(&entry_id)
            .bind(revision)
            .execute(&self.db)
            .await
            .map_err(db_error)?;
            total = total.saturating_sub(size.max(0) as u64);
        }
        Ok(())
    }

    async fn ensure_not_cancelled(&self, entry_id: &str) -> Result<(), String> {
        if self.cancelled_entries.lock().await.contains(entry_id) {
            return Err(CANCELLED_ERROR.to_string());
        }
        let deleted: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM file_sync_entries WHERE id = ? AND deleted_at IS NOT NULL)")
            .bind(entry_id).fetch_one(&self.db).await.map_err(db_error)?;
        if deleted {
            Err(CANCELLED_ERROR.to_string())
        } else {
            Ok(())
        }
    }

    fn emit_changed(&self, entry_ids: Vec<String>, reason: &str) {
        let _ = self.app_handle.emit(
            "file-sync:changed",
            FileSyncChangedEvent {
                entry_ids,
                reason: reason.to_string(),
            },
        );
    }

    fn emit_progress(&self, entry_id: &str, status: &str, completed: u64, total: u64) {
        let _ = self.app_handle.emit(
            "file-sync:progress",
            FileSyncProgressEvent {
                entry_id: entry_id.to_string(),
                status: status.to_string(),
                completed_bytes: completed,
                total_bytes: total,
            },
        );
    }
}

fn is_clipboard_file_reference(item_type: &str, content: &str) -> bool {
    if item_type == "file" {
        return true;
    }
    if item_type != "text" {
        return false;
    }

    let mut lines = content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty());
    let Some(first) = lines.next() else {
        return false;
    };
    first.starts_with("file://") && lines.all(|line| line.starts_with("file://"))
}

async fn apply_remote_event_to_db(
    db: &Db,
    profile_id: &str,
    event: &FileSyncRemoteEvent,
) -> Result<(), String> {
    if event.operation == "delete" {
        sqlx::query(
            r#"
            INSERT INTO file_sync_tombstones (profile_id, entry_id, revision, deleted_at)
            VALUES (?, ?, ?, datetime('now'))
            ON CONFLICT(profile_id, entry_id) DO UPDATE SET
                revision = MAX(file_sync_tombstones.revision, excluded.revision),
                deleted_at = datetime('now')
            "#,
        )
        .bind(profile_id)
        .bind(&event.entry_id)
        .bind(event.revision)
        .execute(db)
        .await
        .map_err(db_error)?;
        sqlx::query(
            "UPDATE file_sync_entries SET status = 'deleted', deleted_at = datetime('now'), updated_at = datetime('now') WHERE id = ? AND profile_id = ? AND revision <= ?",
        )
        .bind(&event.entry_id)
        .bind(profile_id)
        .bind(event.revision)
        .execute(db)
        .await
        .map_err(db_error)?;
        return Ok(());
    }
    let summary = event
        .entry
        .as_ref()
        .ok_or_else(|| "Remote upsert event has no entry".to_string())?;
    validate_remote_summary(summary)?;
    let tombstone_revision = sqlx::query_scalar::<_, i64>(
        "SELECT revision FROM file_sync_tombstones WHERE profile_id = ? AND entry_id = ?",
    )
    .bind(profile_id)
    .bind(&summary.id)
    .fetch_optional(db)
    .await
    .map_err(db_error)?;
    if tombstone_revision.is_some_and(|revision| revision >= summary.revision) {
        return Ok(());
    }
    sqlx::query(
        r#"
        INSERT INTO file_sync_entries
            (id, profile_id, origin_device_id, kind, display_name, total_size,
             file_count, revision, status, confirmed, manifest_hash, manifest_path,
             synced_at, updated_at)
        SELECT ?, ?, ?, ?, ?, ?, ?, ?, 'remote', 1, ?, ?, ?, datetime('now')
        WHERE NOT EXISTS (
            SELECT 1 FROM file_sync_tombstones
            WHERE profile_id = ? AND entry_id = ? AND revision >= ?
        )
        ON CONFLICT(id) DO UPDATE SET
            profile_id = excluded.profile_id,
            origin_device_id = excluded.origin_device_id,
            kind = excluded.kind,
            display_name = excluded.display_name,
            total_size = excluded.total_size,
            file_count = excluded.file_count,
            revision = excluded.revision,
            status = CASE
                WHEN file_sync_entries.source_path IS NOT NULL THEN 'synced'
                WHEN file_sync_entries.cache_path IS NOT NULL THEN 'ready'
                ELSE 'remote'
            END,
            confirmed = 1,
            manifest_hash = excluded.manifest_hash,
            manifest_path = excluded.manifest_path,
            synced_at = excluded.synced_at,
            deleted_at = NULL,
            error = NULL,
            updated_at = datetime('now')
        WHERE excluded.revision > file_sync_entries.revision
           OR (excluded.revision = file_sync_entries.revision
               AND file_sync_entries.status NOT IN ('queued', 'scanning', 'preparing', 'uploading', 'downloading'))
        "#,
    )
    .bind(&summary.id)
    .bind(profile_id)
    .bind(&summary.origin_device_id)
    .bind(&summary.kind)
    .bind(&summary.display_name)
    .bind(to_i64(summary.total_size, "remote entry size")?)
    .bind(to_i64(summary.file_count, "remote file count")?)
    .bind(summary.revision)
    .bind(&summary.manifest_hash)
    .bind(&summary.manifest_path)
    .bind(&summary.synced_at)
    .bind(profile_id)
    .bind(&summary.id)
    .bind(summary.revision)
    .execute(db)
    .await
    .map_err(db_error)?;
    Ok(())
}

// Receipts replace the old contiguous cursor: publication can fail after a
// sequence is reserved, and remote listings can reveal a smaller sequence later.
async fn pull_file_events(
    db: &Db,
    profile_id: &str,
    provider: &dyn SyncProvider,
    key: Option<&SecretVec<u8>>,
) -> Result<(), String> {
    let directory = format!("{}/changes", REMOTE_ROOT);
    let mut changes: Vec<_> = provider
        .list(&directory)
        .await
        .map_err(safe_sync_error)?
        .into_iter()
        .filter_map(|object| parse_change_object(&object.path))
        .collect();
    changes.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
    let mut blocked = HashSet::new();
    let mut errors = Vec::new();
    for (device, seq, path) in changes {
        if blocked.contains(&device) {
            continue;
        }
        let received: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM file_sync_received_events WHERE profile_id = ? AND device_id = ? AND seq = ?)")
            .bind(profile_id).bind(&device).bind(seq).fetch_one(db).await.map_err(db_error)?;
        if received {
            continue;
        }
        let mut manifest_failed = false;
        let result = async {
            let bytes = provider.get(&path).await.map_err(safe_sync_error)?;
            let decoded = decode_remote_bytes(bytes, key, &path)?;
            let event: FileSyncRemoteEvent = serde_json::from_slice(&decoded)
                .map_err(|_| "Remote file sync event is invalid".to_string())?;
            validate_remote_event(&event, &device, seq)?;
            if let Some(summary) = &event.entry {
                let tombstone_revision = sqlx::query_scalar::<_, i64>(
                    "SELECT revision FROM file_sync_tombstones WHERE profile_id = ? AND entry_id = ?",
                )
                .bind(profile_id)
                .bind(&event.entry_id)
                .fetch_optional(db)
                .await
                .map_err(db_error)?;
                if !tombstone_revision.is_some_and(|revision| revision >= event.revision) {
                    if let Err(error) = verify_remote_manifest(provider, key, summary).await {
                        manifest_failed = true;
                        return Err(error);
                    }
                }
            }
            apply_remote_event_to_db(db, profile_id, &event).await?;
            // If the process stops between apply and receipt, replay is idempotent.
            sqlx::query("INSERT OR IGNORE INTO file_sync_received_events (profile_id, device_id, seq) VALUES (?, ?, ?)")
                .bind(profile_id).bind(&device).bind(seq).execute(db).await.map_err(db_error)?;
            Ok::<(), String>(())
        }.await;
        if let Err(error) = result {
            errors.push(format!(
                "File Sync event {} from {} failed: {}",
                seq, device, error
            ));
            // A later event from the same device may delete this entry.
            // Receipts track each event independently, so it can still apply.
            if !manifest_failed {
                blocked.insert(device);
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

async fn verify_remote_manifest(
    provider: &dyn SyncProvider,
    key: Option<&SecretVec<u8>>,
    summary: &RemoteEntrySummary,
) -> Result<(), String> {
    validate_remote_summary(summary)?;
    let expected_path = format!(
        "{}/entries/{}/{}/manifest.json{}",
        REMOTE_ROOT,
        summary.id,
        summary.revision,
        if key.is_some() { ".enc" } else { "" }
    );
    if summary.manifest_path != expected_path {
        return Err("Remote file manifest path is invalid".to_string());
    }
    let encoded = provider.get(&expected_path).await.map_err(|_| {
        format!(
            "Remote file data for {} is unavailable",
            summary.display_name
        )
    })?;
    let decoded = decode_remote_bytes(encoded, key, &expected_path)?;
    if sha256_hex(&decoded) != summary.manifest_hash {
        return Err("Remote file manifest failed its integrity check".to_string());
    }
    let manifest: FileSyncManifest = serde_json::from_slice(&decoded)
        .map_err(|_| "Remote file manifest is invalid".to_string())?;
    if manifest.schema_version != FILE_SYNC_SCHEMA_VERSION
        || manifest.entry_id != summary.id
        || manifest.revision != summary.revision
        || manifest.kind != summary.kind
        || manifest.display_name != summary.display_name
        || manifest.total_size != summary.total_size
        || manifest.file_count != summary.file_count
    {
        return Err("Remote file manifest does not match its event".to_string());
    }
    Ok(())
}

async fn queue_entry_deletion(
    db: &Db,
    profile_id: &str,
    event: &FileSyncRemoteEvent,
) -> Result<(), String> {
    let json = serde_json::to_string(event).map_err(|_| "Failed to encode deletion")?;
    let mut tx = db.begin().await.map_err(db_error)?;
    sqlx::query("INSERT INTO file_sync_pending_deletes (profile_id, entry_id, event_json) VALUES (?, ?, ?) ON CONFLICT(profile_id, entry_id) DO NOTHING")
        .bind(profile_id).bind(&event.entry_id).bind(json).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("INSERT INTO file_sync_tombstones (profile_id, entry_id, revision) VALUES (?, ?, ?) ON CONFLICT(profile_id, entry_id) DO UPDATE SET revision = MAX(file_sync_tombstones.revision, excluded.revision)")
        .bind(profile_id).bind(&event.entry_id).bind(event.revision).execute(&mut *tx).await.map_err(db_error)?;
    sqlx::query("UPDATE file_sync_entries SET status = 'deleted', deleted_at = datetime('now'), updated_at = datetime('now') WHERE profile_id = ? AND id = ?")
        .bind(profile_id).bind(&event.entry_id).execute(&mut *tx).await.map_err(db_error)?;
    tx.commit().await.map_err(db_error)
}

async fn publish_file_event(
    provider: &dyn SyncProvider,
    event: &FileSyncRemoteEvent,
    key: Option<&SecretVec<u8>>,
) -> Result<String, String> {
    let directory = format!("{}/changes", REMOTE_ROOT);
    provider
        .mkdir_all(&directory)
        .await
        .map_err(safe_sync_error)?;
    let path = format!(
        "{}/{}_{:020}.json{}",
        directory,
        event.device_id,
        event.seq,
        if key.is_some() { ".enc" } else { "" }
    );
    let bytes = serde_json::to_vec(event).map_err(|_| "Failed to encode file sync event")?;
    provider
        .put(&path, encode_remote_bytes(bytes, key, &path)?)
        .await
        .map_err(safe_sync_error)?;
    Ok(path)
}

async fn flush_pending_deletes(
    db: &Db,
    profile_id: &str,
    provider: &dyn SyncProvider,
    key: Option<&SecretVec<u8>>,
) -> Result<(), String> {
    let pending: Vec<(String, String)> = sqlx::query_as(
        "SELECT entry_id, event_json FROM file_sync_pending_deletes WHERE profile_id = ?",
    )
    .bind(profile_id)
    .fetch_all(db)
    .await
    .map_err(db_error)?;
    let mut errors = Vec::new();
    for (id, json) in pending {
        let result = async {
            let event: FileSyncRemoteEvent =
                serde_json::from_str(&json).map_err(|_| "Invalid pending deletion")?;
            validate_identifier(&id, "entry ID")?;
            validate_remote_event(&event, &event.device_id, event.seq)?;
            if event.entry_id != id || event.operation != "delete" {
                return Err("Invalid pending deletion identity".to_string());
            }
            // Publish the tombstone first. Retrying uses the same event and sequence.
            publish_file_event(provider, &event, key).await?;
            let root = remote_entry_root(&id);
            provider.delete(&root).await.map_err(safe_sync_error)?;
            if provider
                .stat(&root)
                .await
                .map_err(safe_sync_error)?
                .is_some()
            {
                return Err("Remote file data still exists after deletion".to_string());
            }
            sqlx::query(
                "DELETE FROM file_sync_pending_deletes WHERE profile_id = ? AND entry_id = ?",
            )
            .bind(profile_id)
            .bind(&id)
            .execute(db)
            .await
            .map_err(db_error)?;
            Ok::<(), String>(())
        }
        .await;
        if let Err(error) = result {
            sqlx::query("UPDATE file_sync_pending_deletes SET error = ? WHERE profile_id = ? AND entry_id = ?")
                .bind(&error).bind(profile_id).bind(&id).execute(db).await.map_err(db_error)?;
            errors.push(error);
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!("Remote deletion pending: {}", errors.join("; ")))
    }
}

fn parse_change_object(path: &str) -> Option<(String, i64, String)> {
    let file_name = Path::new(path).file_name()?.to_str()?;
    let stem = file_name
        .strip_suffix(".json.enc")
        .or_else(|| file_name.strip_suffix(".json"))?;
    let (device_id, seq) = stem.rsplit_once('_')?;
    if device_id.is_empty() {
        return None;
    }
    let seq = seq.parse::<i64>().ok()?;
    Some((device_id.to_string(), seq, path.to_string()))
}

fn safe_join(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let relative_path = Path::new(relative);
    validate_relative_path(relative_path)?;
    let joined = root.join(relative_path);
    if !joined.starts_with(root) {
        return Err("Remote path escaped the download cache".to_string());
    }
    Ok(joined)
}

fn encode_remote_bytes(
    bytes: Vec<u8>,
    key: Option<&SecretVec<u8>>,
    object_path: &str,
) -> Result<Vec<u8>, String> {
    match key {
        Some(key) => {
            let object_key = derive_object_key(key, object_path);
            encrypt(&bytes, &object_key).map_err(safe_sync_error)
        }
        None => Ok(bytes),
    }
}

fn decode_remote_bytes(
    bytes: Vec<u8>,
    key: Option<&SecretVec<u8>>,
    object_path: &str,
) -> Result<Vec<u8>, String> {
    match key {
        Some(key) => {
            let object_key = derive_object_key(key, object_path);
            decrypt(&bytes, &object_key).map_err(safe_sync_error)
        }
        None => Ok(bytes),
    }
}

fn derive_object_key(key: &SecretVec<u8>, object_path: &str) -> SecretVec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(b"cliporax-file-sync-object-v1\0");
    hasher.update(key.expose_secret());
    hasher.update(b"\0");
    hasher.update(object_path.as_bytes());
    SecretVec::new(hasher.finalize().to_vec())
}

fn sha256_hex(data: &[u8]) -> String {
    format!("{:x}", Sha256::digest(data))
}

fn remote_parent(path: &str) -> Option<&str> {
    path.rsplit_once('/').map(|(parent, _)| parent)
}

fn provider_name(provider: &SyncProviderKind) -> &'static str {
    match provider {
        SyncProviderKind::WebDav => "webdav",
        SyncProviderKind::Sftp => "sftp",
        SyncProviderKind::GoogleDrive => "google_drive",
        SyncProviderKind::OneDrive => "one_drive",
    }
}

fn remote_entry_root(entry_id: &str) -> String {
    format!("{}/entries/{}", REMOTE_ROOT, entry_id)
}

fn db_error(error: sqlx::Error) -> String {
    log::warn!("[FileSync] Database operation failed: {}", error);
    "File Sync database operation failed".to_string()
}

fn safe_sync_error(error: impl std::fmt::Display) -> String {
    let message = error.to_string();
    if message.contains("locked") {
        "The selected encrypted sync profile is locked".to_string()
    } else if message.contains("not found") {
        "The selected sync profile or remote object was not found".to_string()
    } else {
        "File Sync remote operation failed".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct TestProvider {
        objects: Mutex<std::collections::BTreeMap<String, Vec<u8>>>,
        fail_get: Mutex<HashSet<String>>,
        fail_put: std::sync::atomic::AtomicBool,
        fail_delete: std::sync::atomic::AtomicBool,
    }

    #[async_trait::async_trait]
    impl SyncProvider for TestProvider {
        async fn test_connection(&self) -> Result<(), crate::sync::error::SyncError> {
            Ok(())
        }
        async fn list(
            &self,
            prefix: &str,
        ) -> Result<Vec<crate::sync::models::RemoteObject>, crate::sync::error::SyncError> {
            Ok(self
                .objects
                .lock()
                .await
                .iter()
                .filter(|(path, _)| path.starts_with(prefix))
                .map(|(path, bytes)| crate::sync::models::RemoteObject {
                    path: path.clone(),
                    size: bytes.len() as u64,
                    modified_at: None,
                    etag: None,
                })
                .collect())
        }
        async fn stat(
            &self,
            path: &str,
        ) -> Result<Option<crate::sync::models::RemoteObject>, crate::sync::error::SyncError>
        {
            Ok(self.list(path).await?.into_iter().next())
        }
        async fn get(&self, path: &str) -> Result<Vec<u8>, crate::sync::error::SyncError> {
            if self.fail_get.lock().await.contains(path) {
                return Err(crate::sync::error::SyncError::provider(
                    "Injected read failure",
                ));
            }
            self.objects
                .lock()
                .await
                .get(path)
                .cloned()
                .ok_or_else(|| crate::sync::error::SyncError::provider("Not found"))
        }
        async fn put(
            &self,
            path: &str,
            bytes: Vec<u8>,
        ) -> Result<(), crate::sync::error::SyncError> {
            if self.fail_put.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(crate::sync::error::SyncError::provider(
                    "Injected publish failure",
                ));
            }
            self.objects.lock().await.insert(path.into(), bytes);
            Ok(())
        }
        async fn mkdir_all(&self, _: &str) -> Result<(), crate::sync::error::SyncError> {
            Ok(())
        }
        async fn move_object(&self, _: &str, _: &str) -> Result<(), crate::sync::error::SyncError> {
            Err(crate::sync::error::SyncError::provider("Unused"))
        }
        async fn delete(&self, path: &str) -> Result<(), crate::sync::error::SyncError> {
            if self.fail_delete.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(crate::sync::error::SyncError::provider(
                    "Injected delete failure",
                ));
            }
            self.objects
                .lock()
                .await
                .retain(|key, _| key != path && !key.starts_with(&format!("{}/", path)));
            Ok(())
        }
    }

    async fn replication_db() -> Result<Db, sqlx::Error> {
        let db = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect(":memory:")
            .await?;
        sqlx::query("PRAGMA foreign_keys = ON").execute(&db).await?;
        sqlx::query("CREATE TABLE sync_profiles (id TEXT PRIMARY KEY)")
            .execute(&db)
            .await?;
        sqlx::query("INSERT INTO sync_profiles VALUES ('profile123')")
            .execute(&db)
            .await?;
        crate::db::database::create_file_sync_tables(&db).await?;
        Ok(db)
    }

    fn upsert_event(seq: i64, id: &str) -> FileSyncRemoteEvent {
        FileSyncRemoteEvent {
            schema_version: FILE_SYNC_SCHEMA_VERSION,
            device_id: "device123".into(),
            seq,
            operation: "upsert".into(),
            entry_id: id.into(),
            revision: 1,
            changed_at: "2026-09-19T00:00:00Z".into(),
            entry: Some(RemoteEntrySummary {
                id: id.into(),
                origin_device_id: "device123".into(),
                kind: "file".into(),
                display_name: "file.txt".into(),
                total_size: 4,
                file_count: 1,
                revision: 1,
                manifest_path: format!("file-sync/v1/entries/{}/1/manifest.json", id),
                manifest_hash: "a".repeat(64),
                synced_at: "2026-09-19T00:00:00Z".into(),
            }),
        }
    }

    async fn publish_test_upsert(
        provider: &TestProvider,
        mut event: FileSyncRemoteEvent,
    ) -> Result<String, String> {
        let summary = event.entry.as_mut().ok_or("Missing test entry")?;
        let manifest = FileSyncManifest {
            schema_version: FILE_SYNC_SCHEMA_VERSION,
            entry_id: summary.id.clone(),
            revision: summary.revision,
            kind: summary.kind.clone(),
            display_name: summary.display_name.clone(),
            total_size: summary.total_size,
            file_count: summary.file_count,
            created_at: summary.synced_at.clone(),
            nodes: vec![ManifestNode {
                path: summary.display_name.clone(),
                kind: "file".to_string(),
                size: 4,
                modified_unix_ms: None,
                chunks: vec![ManifestChunk {
                    index: 0,
                    size: 4,
                    sha256: "0".repeat(64),
                    remote_path: format!(
                        "{}/entries/{}/1/objects/0/0.bin",
                        REMOTE_ROOT, summary.id
                    ),
                }],
            }],
        };
        let bytes = serde_json::to_vec(&manifest).map_err(|error| error.to_string())?;
        summary.manifest_hash = sha256_hex(&bytes);
        provider
            .put(&summary.manifest_path, bytes)
            .await
            .map_err(safe_sync_error)?;
        publish_file_event(provider, &event, None).await
    }

    #[tokio::test]
    async fn gaps_and_late_events_do_not_stall_or_lose_entries(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = replication_db().await?;
        let provider = TestProvider::default();
        // Old high-water cursors may have skipped events. Receipts safely replay them.
        sqlx::query("INSERT INTO file_sync_remote_cursors (profile_id, remote_device_id, last_seq) VALUES ('profile123', 'device123', 99)").execute(&db).await?;
        publish_test_upsert(&provider, upsert_event(2, "second")).await?;
        pull_file_events(&db, "profile123", &provider, None).await?;
        publish_test_upsert(&provider, upsert_event(1, "first")).await?;
        pull_file_events(&db, "profile123", &provider, None).await?;
        pull_file_events(&db, "profile123", &provider, None).await?;
        let ids: Vec<String> = sqlx::query_scalar("SELECT id FROM file_sync_entries ORDER BY id")
            .fetch_all(&db)
            .await?;
        assert_eq!(ids, vec!["first", "second"]);
        let receipts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_sync_received_events")
            .fetch_one(&db)
            .await?;
        assert_eq!(receipts, 2);
        Ok(())
    }

    #[tokio::test]
    async fn failed_download_is_not_acknowledged_and_retries(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = replication_db().await?;
        let provider = TestProvider::default();
        let first = publish_test_upsert(&provider, upsert_event(1, "first")).await?;
        publish_test_upsert(&provider, upsert_event(2, "second")).await?;
        provider.fail_get.lock().await.insert(first.clone());
        assert!(pull_file_events(&db, "profile123", &provider, None)
            .await
            .is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_sync_received_events")
            .fetch_one(&db)
            .await?;
        assert_eq!(count, 0);
        provider.fail_get.lock().await.remove(&first);
        provider.put(&first, b"invalid event".to_vec()).await?;
        assert!(pull_file_events(&db, "profile123", &provider, None)
            .await
            .is_err());
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_sync_received_events")
            .fetch_one(&db)
            .await?;
        assert_eq!(count, 0);
        publish_test_upsert(&provider, upsert_event(1, "first")).await?;
        pull_file_events(&db, "profile123", &provider, None).await?;
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_sync_received_events")
            .fetch_one(&db)
            .await?;
        assert_eq!(count, 2);
        Ok(())
    }

    #[tokio::test]
    async fn orphaned_upsert_does_not_create_an_unusable_file_entry(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = replication_db().await?;
        let provider = TestProvider::default();
        publish_file_event(&provider, &upsert_event(1, "missing"), None).await?;

        let error = pull_file_events(&db, "profile123", &provider, None)
            .await
            .expect_err("missing manifest must reject the event");
        assert!(error.contains("Remote file data for file.txt is unavailable"));
        let entries: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_sync_entries")
            .fetch_one(&db)
            .await?;
        let receipts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_sync_received_events")
            .fetch_one(&db)
            .await?;
        assert_eq!((entries, receipts), (0, 0));

        publish_test_upsert(&provider, upsert_event(1, "missing")).await?;
        pull_file_events(&db, "profile123", &provider, None).await?;
        let entries: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_sync_entries")
            .fetch_one(&db)
            .await?;
        assert_eq!(entries, 1);
        Ok(())
    }

    #[tokio::test]
    async fn missing_manifest_does_not_block_a_later_delete_from_the_same_device(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let db = replication_db().await?;
        let provider = TestProvider::default();
        publish_file_event(&provider, &upsert_event(1, "missing"), None).await?;
        let mut deletion = upsert_event(2, "missing");
        deletion.operation = "delete".to_string();
        deletion.entry = None;
        publish_file_event(&provider, &deletion, None).await?;

        assert!(pull_file_events(&db, "profile123", &provider, None)
            .await
            .is_err());
        let tombstone: i64 = sqlx::query_scalar(
            "SELECT revision FROM file_sync_tombstones WHERE profile_id = 'profile123' AND entry_id = 'missing'",
        )
        .fetch_one(&db)
        .await?;
        assert_eq!(tombstone, 1);
        pull_file_events(&db, "profile123", &provider, None).await?;
        let receipts: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_sync_received_events")
            .fetch_one(&db)
            .await?;
        assert_eq!(receipts, 2);
        Ok(())
    }

    #[tokio::test]
    async fn deletion_retries_publication_and_remote_cleanup_without_resurrection(
    ) -> Result<(), Box<dyn std::error::Error>> {
        use std::sync::atomic::Ordering;
        let db = replication_db().await?;
        let other = replication_db().await?;
        let provider = TestProvider::default();
        let upsert = upsert_event(1, "entry123");
        publish_test_upsert(&provider, upsert.clone()).await?;
        let object = "file-sync/v1/entries/entry123/1/objects/0/0.bin";
        provider.put(object, vec![1, 2, 3, 4]).await?;
        pull_file_events(&db, "profile123", &provider, None).await?;
        pull_file_events(&other, "profile123", &provider, None).await?;
        let mut deletion = upsert.clone();
        deletion.device_id = "otherdevice".into();
        deletion.operation = "delete".into();
        deletion.entry = None;
        queue_entry_deletion(&db, "profile123", &deletion).await?;
        let visible: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM file_sync_entries WHERE deleted_at IS NULL")
                .fetch_one(&db)
                .await?;
        assert_eq!(visible, 0);
        provider.fail_put.store(true, Ordering::SeqCst);
        assert!(flush_pending_deletes(&db, "profile123", &provider, None)
            .await
            .is_err());
        assert!(provider.stat(object).await?.is_some());
        provider.fail_put.store(false, Ordering::SeqCst);
        provider.fail_delete.store(true, Ordering::SeqCst);
        assert!(flush_pending_deletes(&db, "profile123", &provider, None)
            .await
            .is_err());
        pull_file_events(&other, "profile123", &provider, None).await?;
        let visible: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM file_sync_entries WHERE deleted_at IS NULL")
                .fetch_one(&other)
                .await?;
        assert_eq!(visible, 0);
        provider.fail_delete.store(false, Ordering::SeqCst);
        flush_pending_deletes(&db, "profile123", &provider, None).await?;
        assert!(provider.stat(object).await?.is_none());
        let pending: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM file_sync_pending_deletes")
            .fetch_one(&db)
            .await?;
        assert_eq!(pending, 0);
        // A previously unseen, delayed upsert cannot undo a tombstone.
        publish_file_event(&provider, &upsert_event(3, "entry123"), None).await?;
        pull_file_events(&other, "profile123", &provider, None).await?;
        let visible: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM file_sync_entries WHERE deleted_at IS NULL")
                .fetch_one(&other)
                .await?;
        assert_eq!(visible, 0);
        Ok(())
    }

    fn test_entry() -> EntryRow {
        EntryRow {
            id: "entry123".to_string(),
            profile_id: "profile123".to_string(),
            origin_device_id: "device123".to_string(),
            kind: "folder".to_string(),
            display_name: "folder".to_string(),
            source_path: None,
            cache_path: None,
            total_size: 4,
            file_count: 1,
            revision: 1,
            status: "remote".to_string(),
            confirmed: 1,
            manifest_hash: None,
            manifest_path: None,
            error: None,
            synced_at: None,
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    fn test_manifest() -> FileSyncManifest {
        FileSyncManifest {
            schema_version: FILE_SYNC_SCHEMA_VERSION,
            entry_id: "entry123".to_string(),
            revision: 1,
            kind: "folder".to_string(),
            display_name: "folder".to_string(),
            total_size: 4,
            file_count: 1,
            created_at: chrono::Utc::now().to_rfc3339(),
            nodes: vec![
                ManifestNode {
                    path: String::new(),
                    kind: "directory".to_string(),
                    size: 0,
                    modified_unix_ms: None,
                    chunks: Vec::new(),
                },
                ManifestNode {
                    path: "file.txt".to_string(),
                    kind: "file".to_string(),
                    size: 4,
                    modified_unix_ms: None,
                    chunks: vec![ManifestChunk {
                        index: 0,
                        size: 4,
                        sha256: "0".repeat(64),
                        remote_path: "file-sync/v1/entries/entry123/1/objects/0/0.bin".to_string(),
                    }],
                },
            ],
        }
    }

    #[test]
    fn rejects_unsafe_portable_names() {
        assert!(validate_portable_name("../secret").is_err());
        assert!(validate_portable_name("CON.txt").is_err());
        assert!(validate_portable_name("bad?.txt").is_err());
        assert!(validate_portable_name("safe-file.txt").is_ok());
    }

    #[test]
    fn parses_flat_change_log_names() {
        let parsed =
            parse_change_object("file-sync/v1/changes/device_abc_00000000000000000042.json.enc")
                .expect("change path should parse");
        assert_eq!(parsed.0, "device_abc");
        assert_eq!(parsed.1, 42);
    }

    #[test]
    fn safe_join_rejects_parent_components() {
        let root = Path::new("/tmp/cache");
        assert!(safe_join(root, "../escape").is_err());
        assert_eq!(
            safe_join(root, "folder/file.txt").unwrap(),
            root.join("folder").join("file.txt")
        );
    }

    #[test]
    fn snapshot_preparation_splits_files_into_resumable_chunks() {
        let temp = tempfile::tempdir().expect("temp directory");
        let source = temp.path().join("large.bin");
        let mut data = vec![0x5au8; CHUNK_SIZE];
        data.extend_from_slice(b"tail");
        std::fs::write(&source, &data).expect("source file");
        let staging = temp.path().join("staging");

        let prepared = prepare_snapshot("entry123", 1, &source, &staging, CHUNK_SIZE)
            .expect("prepared snapshot");

        assert_eq!(prepared.manifest.file_count, 1);
        assert_eq!(prepared.manifest.total_size, data.len() as u64);
        assert_eq!(prepared.chunks.len(), 2);
        assert!(prepared
            .chunks
            .iter()
            .all(|chunk| chunk.staging_path.is_file()));
        assert_eq!(
            prepared
                .manifest
                .nodes
                .iter()
                .find(|node| node.kind == "file")
                .expect("file node")
                .chunks
                .len(),
            2
        );
    }

    #[test]
    fn encrypted_objects_are_bound_to_their_remote_path() {
        let key = SecretVec::new(vec![0x5a; 32]);
        let ciphertext = encode_remote_bytes(
            b"chunk".to_vec(),
            Some(&key),
            "file-sync/v1/entries/a/1/objects/0/0.bin",
        )
        .expect("encrypted object");

        assert_eq!(
            decode_remote_bytes(
                ciphertext.clone(),
                Some(&key),
                "file-sync/v1/entries/a/1/objects/0/0.bin",
            )
            .expect("matching object path"),
            b"chunk",
        );
        assert!(decode_remote_bytes(
            ciphertext,
            Some(&key),
            "file-sync/v1/entries/a/1/objects/0/1.bin",
        )
        .is_err());
    }

    #[tokio::test]
    async fn remote_upsert_populates_file_sync_item_list() -> Result<(), Box<dyn std::error::Error>>
    {
        let db = sqlx::SqlitePool::connect(":memory:").await?;
        sqlx::query("PRAGMA foreign_keys = ON").execute(&db).await?;
        sqlx::query("CREATE TABLE sync_profiles (id TEXT PRIMARY KEY)")
            .execute(&db)
            .await?;
        sqlx::query(
            r#"
            CREATE TABLE file_sync_entries (
                id TEXT PRIMARY KEY,
                profile_id TEXT NOT NULL,
                origin_device_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                display_name TEXT NOT NULL,
                source_path TEXT,
                cache_path TEXT,
                total_size INTEGER NOT NULL DEFAULT 0,
                file_count INTEGER NOT NULL DEFAULT 0,
                revision INTEGER NOT NULL DEFAULT 1,
                status TEXT NOT NULL,
                confirmed INTEGER NOT NULL DEFAULT 0,
                manifest_hash TEXT,
                manifest_path TEXT,
                remote_event_path TEXT,
                error TEXT,
                synced_at DATETIME,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                deleted_at DATETIME,
                FOREIGN KEY (profile_id) REFERENCES sync_profiles(id)
            )
            "#,
        )
        .execute(&db)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE file_sync_tombstones (
                profile_id TEXT NOT NULL,
                entry_id TEXT NOT NULL,
                revision INTEGER NOT NULL,
                deleted_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                PRIMARY KEY (profile_id, entry_id),
                FOREIGN KEY (profile_id) REFERENCES sync_profiles(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(&db)
        .await?;
        sqlx::query("INSERT INTO sync_profiles (id) VALUES ('profile123')")
            .execute(&db)
            .await?;

        let synced_at = chrono::Utc::now().to_rfc3339();
        let event = FileSyncRemoteEvent {
            schema_version: FILE_SYNC_SCHEMA_VERSION,
            device_id: "device123".to_string(),
            seq: 1,
            operation: "upsert".to_string(),
            entry_id: "entry123".to_string(),
            revision: 1,
            changed_at: synced_at.clone(),
            entry: Some(RemoteEntrySummary {
                id: "entry123".to_string(),
                origin_device_id: "device123".to_string(),
                kind: "file".to_string(),
                display_name: "report.txt".to_string(),
                total_size: 12,
                file_count: 1,
                revision: 1,
                manifest_path: "file-sync/v1/entries/entry123/1/manifest.json".to_string(),
                manifest_hash: "a".repeat(64),
                synced_at,
            }),
        };

        apply_remote_event_to_db(&db, "profile123", &event).await?;

        let rows: Vec<(String, String, String)> = sqlx::query_as(
            r#"
            SELECT id, display_name, status
            FROM file_sync_entries
            WHERE profile_id = ? AND deleted_at IS NULL
            ORDER BY updated_at DESC
            "#,
        )
        .bind("profile123")
        .fetch_all(&db)
        .await?;
        assert_eq!(
            rows,
            vec![(
                "entry123".to_string(),
                "report.txt".to_string(),
                "remote".to_string()
            )]
        );

        Ok(())
    }

    #[test]
    fn accepts_only_pure_file_uri_text_items() {
        assert!(is_clipboard_file_reference(
            "text",
            "file:///tmp/first\nfile:///tmp/second"
        ));
        assert!(!is_clipboard_file_reference(
            "text",
            "open file:///tmp/first"
        ));
        assert!(!is_clipboard_file_reference("text", ""));
    }

    #[test]
    fn parses_existing_file_uri_text() -> Result<(), Box<dyn std::error::Error>> {
        let temp = tempfile::tempdir()?;
        let file = temp.path().join("file sync.txt");
        std::fs::write(&file, b"content")?;
        let uri = format!(
            "file://{}",
            urlencoding::encode(file.to_string_lossy().as_ref())
        );

        assert_eq!(parse_file_list(&uri), vec![file]);
        Ok(())
    }

    #[test]
    fn validates_remote_manifest_identity_and_paths() {
        let entry = test_entry();
        let manifest = test_manifest();
        assert!(validate_manifest(&manifest, &entry).is_ok());

        let mut escaped = manifest.clone();
        escaped.nodes[1].path = "../escape.txt".to_string();
        assert!(validate_manifest(&escaped, &entry).is_err());

        let mut wrong_identity = manifest;
        wrong_identity.entry_id = "other-entry".to_string();
        assert!(validate_manifest(&wrong_identity, &entry).is_err());
    }

    #[test]
    fn rejects_duplicate_remote_chunk_references() {
        let mut entry = test_entry();
        entry.total_size = 8;
        entry.file_count = 2;
        let mut manifest = test_manifest();
        manifest.total_size = 8;
        manifest.file_count = 2;
        let mut duplicate = manifest.nodes[1].clone();
        duplicate.path = "second.txt".to_string();
        manifest.nodes.push(duplicate);

        assert!(validate_manifest(&manifest, &entry).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn scan_rejects_symbolic_links() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().expect("temp directory");
        let target = temp.path().join("target.txt");
        let link = temp.path().join("link.txt");
        std::fs::write(&target, b"target").expect("target file");
        symlink(&target, &link).expect("symbolic link");

        assert!(scan_source(&link).is_err());
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn metadata_comparison_detects_replaced_files() {
        let temp = tempfile::tempdir().expect("temp directory");
        let first = temp.path().join("first.bin");
        let second = temp.path().join("second.bin");
        std::fs::write(&first, b"same").expect("first file");
        std::fs::write(&second, b"same").expect("second file");

        let first_metadata = std::fs::metadata(&first).expect("first metadata");
        let second_metadata = std::fs::metadata(&second).expect("second metadata");
        assert_ne!(
            file_identity(&first_metadata),
            file_identity(&second_metadata)
        );
        assert!(!same_file_metadata(&first_metadata, &second_metadata));
    }
}
