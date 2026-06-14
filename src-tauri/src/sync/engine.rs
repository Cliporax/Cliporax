use crate::db::models::ClipboardItem;
/// Sync Engine - orchestrates the sync process
use crate::sync::change_log::{
    build_change_path, build_tombstone_path, list_remote_changes_after, list_remote_devices,
    next_local_remote_seq,
};
use crate::sync::codec::{
    decode_clipboard_item_with_key, decode_tombstone, encode_change,
    encode_clipboard_item_from_remote_with_key, encode_tombstone,
};
use crate::sync::crypto::derive_key;
use crate::sync::error::SyncError;
use crate::sync::lock::{acquire_remote_lock, release_remote_lock};
use crate::sync::manifest::get_or_create_manifest;
use crate::sync::models::*;
use crate::sync::providers::{join_remote_path, SyncProvider};
use crate::sync::repository::SyncRepository;
use crate::sync::secrets::SecretStore;
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct SyncEngine {
    repository: Arc<SyncRepository>,
    #[allow(dead_code)]
    secret_store: Arc<SecretStore>, // Reserved for future credential store integration
    current_runs: Arc<RwLock<Vec<String>>>, // profile_ids currently running
    cancelled_runs: Arc<RwLock<Vec<String>>>, // profile_ids requested for cancellation
    last_reports: Arc<RwLock<HashMap<String, SyncRunReport>>>,
    current_status: Arc<RwLock<HashMap<String, SyncStatus>>>,
    unlocked_keys: Arc<RwLock<HashMap<String, UnlockedSyncKey>>>, // profile_id -> unlocked key
}

struct PreparedSyncRun {
    profile: SyncProfile,
    device_id: String,
}

#[derive(Default)]
struct SyncPhaseReport {
    items_uploaded: i64,
    items_downloaded: i64,
    items_deleted: i64,
    conflicts_found: i64,
    errors: Vec<String>,
}

impl SyncEngine {
    pub fn new(repository: Arc<SyncRepository>, secret_store: Arc<SecretStore>) -> Self {
        Self {
            repository,
            secret_store,
            current_runs: Arc::new(RwLock::new(vec![])),
            cancelled_runs: Arc::new(RwLock::new(vec![])),
            last_reports: Arc::new(RwLock::new(HashMap::new())),
            current_status: Arc::new(RwLock::new(HashMap::new())),
            unlocked_keys: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    async fn set_status(
        &self,
        profile_id: &str,
        status: SyncRunStatus,
        phase: &str,
        progress: f32,
        backoff_reason: Option<String>,
    ) {
        let is_locked = match self.repository.get_profile(profile_id).await {
            Ok(profile) => {
                profile.encryption.enabled && !self.is_profile_unlocked(profile_id).await
            }
            Err(_) => false,
        };
        let mut statuses = self.current_status.write().await;
        statuses.insert(
            profile_id.to_string(),
            SyncStatus {
                profile_id: profile_id.to_string(),
                status,
                phase: Some(phase.to_string()),
                progress: Some(progress.clamp(0.0, 1.0)),
                last_sync_at: self
                    .get_last_report(profile_id)
                    .await
                    .and_then(|report| report.completed_at),
                next_sync_at: None,
                is_paused: false,
                is_locked,
                backoff_reason,
            },
        );
    }

    /// Unlock an encrypted profile by deriving the encryption key
    pub async fn unlock_profile(
        &self,
        profile_id: &str,
        password: &str,
        _remember: bool,
    ) -> Result<(), SyncError> {
        // Get profile to check encryption config
        let profile = self.repository.get_profile(profile_id).await?;

        if !profile.encryption.enabled {
            return Err(SyncError::Validation(
                "Profile encryption is not enabled".to_string(),
            ));
        }

        let context = SyncRepository::crypto_context_from_profile(&profile)?.ok_or_else(|| {
            SyncError::Encryption("Encrypted profile is missing KDF parameters".to_string())
        })?;

        // Derive the encryption key from password
        let key = derive_key(password, &context)
            .map_err(|e| SyncError::Encryption(format!("Key derivation failed: {}", e)))?;

        // Cache the key in memory
        let unlocked = UnlockedSyncKey {
            profile_id: profile_id.to_string(),
            key,
            unlocked_at: chrono::Utc::now(),
        };

        let mut keys = self.unlocked_keys.write().await;
        keys.insert(profile_id.to_string(), unlocked);

        log::info!("[Sync::Engine] Profile {} unlocked", profile_id);
        Ok(())
    }

    /// Lock a profile by clearing its cached key
    pub async fn lock_profile(&self, profile_id: &str) -> Result<(), SyncError> {
        let mut keys = self.unlocked_keys.write().await;
        if keys.remove(profile_id).is_some() {
            log::info!("[Sync::Engine] Profile {} locked", profile_id);
        } else {
            log::debug!("[Sync::Engine] Profile {} was not unlocked", profile_id);
        }
        Ok(())
    }

    /// Get the cached encryption key for a profile (if unlocked)
    pub async fn get_crypto_key(
        &self,
        profile_id: &str,
    ) -> Result<Option<secrecy::SecretVec<u8>>, SyncError> {
        let keys = self.unlocked_keys.read().await;
        Ok(keys
            .get(profile_id)
            .map(|k| secrecy::SecretVec::new(k.key.expose_secret().clone())))
    }

    /// Check if a profile is unlocked
    pub async fn is_profile_unlocked(&self, profile_id: &str) -> bool {
        let keys = self.unlocked_keys.read().await;
        keys.contains_key(profile_id)
    }

    /// Run sync now - implements the full sync loop as defined in the plan
    pub async fn run_now(
        &self,
        profile_id: &str,
        provider: Arc<dyn SyncProvider>,
    ) -> Result<SyncRunReport, SyncError> {
        log::info!("[Sync::Engine] Sync requested for profile: {}", profile_id);

        // Check if already running
        {
            let runs = self.current_runs.read().await;
            if runs.contains(&profile_id.to_string()) {
                return Err(SyncError::Cancelled(
                    "Sync already running for this profile".to_string(),
                ));
            }
        }

        // Mark as running
        {
            let mut runs = self.current_runs.write().await;
            runs.push(profile_id.to_string());
        }
        {
            let mut cancelled = self.cancelled_runs.write().await;
            cancelled.retain(|id| id != profile_id);
        }

        // Execute sync with cleanup guard
        let profile_id_clone = profile_id.to_string();
        let runs_clone = self.current_runs.clone();
        let cleanup_guard = scopeguard::guard((), move |_| {
            tokio::spawn(async move {
                let mut r = runs_clone.write().await;
                if let Some(pos) = r.iter().position(|p| p == &profile_id_clone) {
                    r.remove(pos);
                }
            });
        });

        // Run the actual sync
        let result = self.execute_sync_run(profile_id, provider).await;
        if let Err(error) = &result {
            let message = format!("[Sync::Engine] ERROR: Sync failed: {}", error);
            log::error!("{}", message);
            if let Err(log_error) = self
                .repository
                .record_log(Some(profile_id), None, "ERROR", &message)
                .await
            {
                log::warn!(
                    "[Sync::Engine] Failed to persist sync failure log: {}",
                    log_error
                );
            }
            let report = SyncRunReport {
                profile_id: profile_id.to_string(),
                run_id: format!("failed_{}", uuid::Uuid::new_v4().simple()),
                status: SyncRunStatus::Failed,
                started_at: chrono::Utc::now().to_rfc3339(),
                completed_at: Some(chrono::Utc::now().to_rfc3339()),
                items_uploaded: 0,
                items_downloaded: 0,
                items_deleted: 0,
                conflicts_found: 0,
                errors: vec![error.to_string()],
            };
            if let Err(save_error) = self.repository.save_run_report(&report).await {
                log::warn!(
                    "[Sync::Engine] Failed to persist failed sync report: {}",
                    save_error
                );
            }
            let mut reports = self.last_reports.write().await;
            reports.insert(profile_id.to_string(), report);
        }

        // Drop the cleanup guard (will remove from current_runs)
        drop(cleanup_guard);

        result
    }

    async fn ensure_not_cancelled(&self, profile_id: &str) -> Result<(), SyncError> {
        let cancelled = self.cancelled_runs.read().await;
        if cancelled.contains(&profile_id.to_string()) {
            return Err(SyncError::Cancelled("Sync cancelled by user".to_string()));
        }
        Ok(())
    }

    /// Internal implementation of the sync run
    async fn execute_sync_run(
        &self,
        profile_id: &str,
        provider: Arc<dyn SyncProvider>,
    ) -> Result<SyncRunReport, SyncError> {
        let run_id = format!("run_{}", uuid::Uuid::new_v4().simple());
        let started_at = chrono::Utc::now();
        self.set_status(profile_id, SyncRunStatus::Pulling, "starting", 0.02, None)
            .await;
        self.repository
            .record_log(
                Some(profile_id),
                Some(&run_id),
                "INFO",
                "[Sync::Engine] INFO: Sync run started",
            )
            .await?;

        let prepared = self.prepare_run(profile_id, provider.as_ref()).await?;
        self.ensure_not_cancelled(profile_id).await?;
        self.set_status(
            profile_id,
            SyncRunStatus::WaitingForLock,
            "acquiring_remote_lock",
            0.15,
            None,
        )
        .await;
        let lock_guard = self
            .acquire_run_lock(provider.as_ref(), "", &prepared.device_id, &run_id)
            .await?;
        let sync_result = async {
            log::info!("[Sync::Engine] Step 4: Cleaning up stale .tmp runs");
            self.set_status(
                profile_id,
                SyncRunStatus::Pulling,
                "cleanup_stale_tmp",
                0.25,
                None,
            )
            .await;
            self.cleanup_stale_tmp_runs(provider.as_ref(), "", &run_id)
                .await?;
            self.ensure_not_cancelled(profile_id).await?;

            self.set_status(
                profile_id,
                SyncRunStatus::Pulling,
                "pull_remote_changes",
                0.4,
                None,
            )
            .await;
            let mut report = self
                .pull_remote_changes(profile_id, provider.as_ref(), &prepared)
                .await?;
            self.ensure_not_cancelled(profile_id).await?;
            self.set_status(
                profile_id,
                SyncRunStatus::Uploading,
                "upload_local_changes",
                0.7,
                None,
            )
            .await;
            let upload_report = self
                .upload_local_changes(provider.as_ref(), &prepared, &run_id)
                .await?;
            report.items_uploaded = upload_report.items_uploaded;

            self.set_status(
                profile_id,
                SyncRunStatus::Committing,
                "finalize",
                0.95,
                None,
            )
            .await;
            self.finalize_run(profile_id, &run_id, started_at, report)
                .await
        }
        .await;

        if let Err(e) = release_remote_lock(provider.as_ref(), lock_guard.path(), &run_id).await {
            log::warn!("[Sync::Engine] Failed to release remote lock: {}", e);
        }

        sync_result
    }

    async fn prepare_run(
        &self,
        profile_id: &str,
        provider: &dyn SyncProvider,
    ) -> Result<PreparedSyncRun, SyncError> {
        log::info!("[Sync::Engine] Step 1: Loading profile {}", profile_id);
        let profile = self.repository.get_profile(profile_id).await?;
        if profile.encryption.enabled && !self.is_profile_unlocked(profile_id).await {
            return Err(SyncError::Encryption(
                "Encrypted profile is locked. Unlock it before syncing.".to_string(),
            ));
        }
        let device_id = self.repository.get_or_create_device_id().await?;
        log::info!("[Sync::Engine] Step 2: Testing provider availability");
        let _manifest = get_or_create_manifest(provider, "")
            .await
            .map_err(|e| SyncError::Provider(format!("Failed to access remote storage: {}", e)))?;

        Ok(PreparedSyncRun { profile, device_id })
    }

    async fn acquire_run_lock(
        &self,
        provider: &dyn SyncProvider,
        remote_root: &str,
        device_id: &str,
        run_id: &str,
    ) -> Result<crate::sync::lock::RemoteLockGuard, SyncError> {
        log::info!("[Sync::Engine] Step 3: Acquiring remote lock");
        let lock_owner = RemoteLock {
            schema_version: 1,
            owner_device_id: device_id.to_string(),
            owner_run_id: run_id.to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            expires_at: (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
        };
        let lock_path = join_remote_path(remote_root, "sync.lock");
        acquire_remote_lock(provider, &lock_path, lock_owner, LockWaitConfig::default()).await
    }

    async fn pull_remote_changes(
        &self,
        profile_id: &str,
        provider: &dyn SyncProvider,
        prepared: &PreparedSyncRun,
    ) -> Result<SyncPhaseReport, SyncError> {
        log::info!("[Sync::Engine] Step 5: Pulling remote changes");
        let mut report = SyncPhaseReport::default();
        let remote_devices = list_remote_devices(provider, "").await?;

        for remote_device_id in remote_devices {
            if remote_device_id == prepared.device_id {
                continue;
            }

            let cursor = self
                .repository
                .get_remote_cursor(profile_id, &remote_device_id)
                .await?;
            let remote_changes =
                list_remote_changes_after(provider, "", &remote_device_id, cursor).await?;

            for change in remote_changes {
                if let Some(change_report) = self
                    .apply_remote_change(profile_id, provider, &prepared.profile, &change)
                    .await?
                {
                    let had_errors = !change_report.errors.is_empty();
                    report.items_downloaded += change_report.items_downloaded;
                    report.items_deleted += change_report.items_deleted;
                    report.conflicts_found += change_report.conflicts_found;
                    report.errors.extend(change_report.errors);
                    if !had_errors {
                        self.repository
                            .update_remote_cursor(profile_id, &remote_device_id, change.seq)
                            .await?;
                    }
                }
            }
        }

        Ok(report)
    }

    async fn apply_remote_change(
        &self,
        profile_id: &str,
        provider: &dyn SyncProvider,
        profile: &SyncProfile,
        change: &RemoteChange,
    ) -> Result<Option<SyncPhaseReport>, SyncError> {
        let mut report = SyncPhaseReport::default();
        match change.operation {
            ChangeOperation::Upsert => {
                let Some(item_path) = &change.item_path else {
                    return Ok(Some(report));
                };
                let item_data = match provider.get(item_path).await {
                    Ok(data) => data,
                    Err(e) => {
                        let message = format!(
                            "[Sync::Engine] Failed to download item {}: {}",
                            item_path, e
                        );
                        log::warn!("{}", message);
                        report.errors.push(message);
                        return Ok(Some(report));
                    }
                };
                let crypto_key = self.get_crypto_key(profile_id).await.ok().flatten();
                let remote_item = match decode_clipboard_item_with_key(
                    &item_data,
                    profile,
                    crypto_key.as_ref(),
                ) {
                    Ok(item) => item,
                    Err(e) => {
                        let message = format!("[Sync::Engine] Failed to decode remote item: {}", e);
                        log::warn!("{}", message);
                        report.errors.push(message);
                        return Ok(Some(report));
                    }
                };

                match self
                    .repository
                    .apply_remote_item(profile, remote_item, None)
                    .await
                {
                    Ok(ApplyItemResult::Conflict { .. }) => report.conflicts_found += 1,
                    Ok(_) => report.items_downloaded += 1,
                    Err(e) => {
                        let message = format!("[Sync::Engine] Failed to apply remote item: {}", e);
                        log::warn!("{}", message);
                        report.errors.push(message);
                        return Ok(Some(report));
                    }
                }
            }
            ChangeOperation::Delete => {
                let Some(tombstone_path) = &change.tombstone_path else {
                    return Ok(Some(report));
                };
                let tombstone_data = match provider.get(tombstone_path).await {
                    Ok(data) => data,
                    Err(e) => {
                        let message = format!(
                            "[Sync::Engine] Failed to download tombstone {}: {}",
                            tombstone_path, e
                        );
                        log::warn!("{}", message);
                        report.errors.push(message);
                        return Ok(Some(report));
                    }
                };
                let tombstone = match decode_tombstone(&tombstone_data) {
                    Ok(tombstone) => tombstone,
                    Err(e) => {
                        let message = format!("[Sync::Engine] Failed to decode tombstone: {}", e);
                        log::warn!("{}", message);
                        report.errors.push(message);
                        return Ok(Some(report));
                    }
                };
                if let Err(e) = self
                    .repository
                    .apply_remote_tombstone(profile, tombstone)
                    .await
                {
                    let message = format!("[Sync::Engine] Failed to apply tombstone: {}", e);
                    log::warn!("{}", message);
                    report.errors.push(message);
                    return Ok(Some(report));
                }
                report.items_deleted += 1;
            }
        }

        Ok(Some(report))
    }

    async fn upload_local_changes(
        &self,
        provider: &dyn SyncProvider,
        prepared: &PreparedSyncRun,
        run_id: &str,
    ) -> Result<SyncPhaseReport, SyncError> {
        log::info!("[Sync::Engine] Step 7: Reading unsynced local changes");
        let mut unsynced_changes = self
            .repository
            .list_unsynced_changes(&prepared.profile)
            .await?;
        if unsynced_changes.is_empty() {
            let seeded = self
                .repository
                .enqueue_initial_clipboard_snapshot(&prepared.profile)
                .await?;
            if seeded > 0 {
                unsynced_changes = self
                    .repository
                    .list_unsynced_changes(&prepared.profile)
                    .await?;
            }
        }

        if unsynced_changes.is_empty() {
            log::info!("[Sync::Engine] No unsynced changes");
            return Ok(SyncPhaseReport::default());
        }

        log::info!(
            "[Sync::Engine] Found {} unsynced changes",
            unsynced_changes.len()
        );
        log::info!("[Sync::Engine] Step 8: Staging local files");
        let staged = self
            .stage_local_changes(
                provider,
                "",
                run_id,
                &prepared.device_id,
                &prepared.profile,
                &unsynced_changes,
            )
            .await?;

        log::info!("[Sync::Engine] Step 9: Committing staged files");
        let items_uploaded = self
            .commit_staged_changes(provider, "", run_id, &staged)
            .await?;

        log::info!("[Sync::Engine] Step 10: Marking changes as synced");
        let change_ids: Vec<i64> = unsynced_changes.iter().map(|c| c.id).collect();
        self.repository.mark_changes_synced(&change_ids).await?;

        Ok(SyncPhaseReport {
            items_uploaded,
            errors: staged.warnings,
            ..SyncPhaseReport::default()
        })
    }

    async fn finalize_run(
        &self,
        profile_id: &str,
        run_id: &str,
        started_at: chrono::DateTime<chrono::Utc>,
        report: SyncPhaseReport,
    ) -> Result<SyncRunReport, SyncError> {
        log::info!("[Sync::Engine] Step 11: Updating cursors");
        log::info!("[Sync::Engine] Step 12: Sync complete, releasing lock");
        self.repository
            .record_log(
                Some(profile_id),
                Some(run_id),
                "INFO",
                &format!(
                    "[Sync::Engine] INFO: Sync completed: {} uploaded, {} downloaded, {} deleted, {} conflicts",
                    report.items_uploaded,
                    report.items_downloaded,
                    report.items_deleted,
                    report.conflicts_found
                ),
            )
            .await?;

        let run_report = SyncRunReport {
            profile_id: profile_id.to_string(),
            run_id: run_id.to_string(),
            status: if report.errors.is_empty() {
                SyncRunStatus::Completed
            } else {
                SyncRunStatus::PartialSuccess
            },
            started_at: started_at.to_rfc3339(),
            completed_at: Some(chrono::Utc::now().to_rfc3339()),
            items_uploaded: report.items_uploaded,
            items_downloaded: report.items_downloaded,
            items_deleted: report.items_deleted,
            conflicts_found: report.conflicts_found,
            errors: report.errors.clone(),
        };

        let mut reports = self.last_reports.write().await;
        reports.insert(profile_id.to_string(), run_report.clone());
        drop(reports);
        self.repository.save_run_report(&run_report).await?;
        self.set_status(
            profile_id,
            run_report.status.clone(),
            if run_report.errors.is_empty() {
                "completed"
            } else {
                "partial_success"
            },
            1.0,
            run_report.errors.first().cloned(),
        )
        .await;

        Ok(run_report)
    }

    /// Cleanup stale .tmp runs from previous failed sync attempts
    async fn cleanup_stale_tmp_runs(
        &self,
        provider: &dyn SyncProvider,
        remote_root: &str,
        current_run_id: &str,
    ) -> Result<(), SyncError> {
        let tmp_prefix = join_remote_path(remote_root, ".tmp/");

        match provider.list(&tmp_prefix).await {
            Ok(objects) => {
                for obj in objects {
                    // Extract run_id from path .tmp/<run_id>/
                    let parts: Vec<&str> = obj.path.split('/').collect();
                    if parts.len() >= 3 && parts[parts.len() - 3] == ".tmp" {
                        let run_id_from_path = parts[parts.len() - 2];
                        if run_id_from_path != current_run_id {
                            log::info!(
                                "[Sync::Engine] Cleaning up stale .tmp run: {}",
                                run_id_from_path
                            );
                            // Delete all files in this stale run
                            let stale_prefix = join_remote_path(
                                remote_root,
                                &join_remote_path(".tmp", run_id_from_path),
                            ) + "/";
                            if let Err(e) = provider.delete(&stale_prefix).await {
                                log::warn!(
                                    "[Sync::Engine] Failed to delete stale run {}: {}",
                                    stale_prefix,
                                    e
                                );
                            }
                        }
                    }
                }
            }
            Err(e) => {
                log::debug!("[Sync::Engine] No .tmp runs to cleanup: {}", e);
            }
        }

        Ok(())
    }

    /// Stage local changes under .tmp/<run_id>/
    async fn stage_local_changes(
        &self,
        provider: &dyn SyncProvider,
        remote_root: &str,
        run_id: &str,
        device_id: &str,
        profile: &SyncProfile,
        changes: &[LocalSyncChange],
    ) -> Result<StagedCommit, SyncError> {
        let mut item_paths = Vec::new();
        let mut change_paths = Vec::new();
        let mut tombstone_paths = Vec::new();
        let mut warnings = Vec::new();
        let mut next_seq = next_local_remote_seq(provider, remote_root, device_id).await?;

        for change in changes {
            match change.operation.as_str() {
                "create" | "update" | "upsert" | "toggle_pin" | "metadata_update"
                | "tab_change" | "reorder" => {
                    let local_id = change.entity_id.parse::<i64>().map_err(|_| {
                        SyncError::Validation(format!(
                            "Invalid clipboard change entity_id: {}",
                            change.entity_id
                        ))
                    })?;
                    let local_item = match self
                        .repository
                        .get_local_clipboard_item(local_id)
                        .await?
                    {
                        Some(local_item) => local_item,
                        None => {
                            let message = format!(
                                "[Sync::Engine] Skipping stale local change {}: clipboard item {} no longer exists",
                                change.id, local_id
                            );
                            log::warn!("{}", message);
                            warnings.push(message);

                            if let Some(item_key) = change.item_key.clone() {
                                let tombstone = RemoteTombstone {
                                    schema_version: 1,
                                    item_key: item_key.clone(),
                                    deleted_at: chrono::Utc::now().to_rfc3339(),
                                    deleted_by: device_id.to_string(),
                                };
                                let tombstone_bytes = encode_tombstone(tombstone)?;
                                let tombstone_path = build_tombstone_path(&item_key);
                                let tmp_run_path = join_remote_path(".tmp", run_id);
                                let staged_tombstone_path = join_remote_path(
                                    remote_root,
                                    &join_remote_path(&tmp_run_path, &tombstone_path),
                                );
                                provider
                                    .put(&staged_tombstone_path, tombstone_bytes)
                                    .await?;
                                tombstone_paths.push(staged_tombstone_path);

                                let change_entry = RemoteChange {
                                    schema_version: 1,
                                    seq: next_seq,
                                    device_id: device_id.to_string(),
                                    operation: ChangeOperation::Delete,
                                    entity_type: change.entity_type.clone(),
                                    item_key,
                                    item_path: None,
                                    blob_path: None,
                                    tombstone_path: Some(tombstone_path),
                                    changed_at: chrono::Utc::now().to_rfc3339(),
                                };
                                let change_bytes = encode_change(change_entry)?;
                                let change_path = build_change_path(device_id, next_seq);
                                let staged_change_path = join_remote_path(
                                    remote_root,
                                    &join_remote_path(&tmp_run_path, &change_path),
                                );
                                provider.put(&staged_change_path, change_bytes).await?;
                                change_paths.push(staged_change_path);
                                next_seq += 1;
                            }

                            continue;
                        }
                    };
                    let created_at = local_item.created_at.unwrap_or_else(chrono::Utc::now);
                    let item_key = match &change.item_key {
                        Some(item_key) => item_key.clone(),
                        None => {
                            self.repository
                                .get_or_create_item_key(local_id, created_at, device_id)
                                .await?
                        }
                    };
                    let remote_item = local_item_to_remote(local_item, item_key.clone(), device_id);

                    // Get crypto key if profile is unlocked
                    let crypto_key = self.get_crypto_key(&profile.id).await.ok().flatten();
                    let encoded = encode_clipboard_item_from_remote_with_key(
                        remote_item.clone(),
                        profile,
                        crypto_key.as_ref(),
                    )?;

                    // Stage item
                    let tmp_run_path = join_remote_path(".tmp", run_id);
                    let staged_item_path = join_remote_path(
                        remote_root,
                        &join_remote_path(&tmp_run_path, &encoded.item_path),
                    );
                    provider.put(&staged_item_path, encoded.json_data).await?;
                    item_paths.push(staged_item_path.clone());

                    // Build and stage change log entry
                    let change_entry = RemoteChange {
                        schema_version: 1,
                        seq: next_seq,
                        device_id: device_id.to_string(),
                        operation: ChangeOperation::Upsert,
                        entity_type: change.entity_type.clone(),
                        item_key: item_key.clone(),
                        item_path: Some(encoded.item_path.clone()),
                        blob_path: None,
                        tombstone_path: None,
                        changed_at: chrono::Utc::now().to_rfc3339(),
                    };
                    let change_bytes = encode_change(change_entry)?;
                    let change_path = build_change_path(device_id, next_seq);
                    let staged_change_path = join_remote_path(
                        remote_root,
                        &join_remote_path(&tmp_run_path, &change_path),
                    );
                    provider.put(&staged_change_path, change_bytes).await?;
                    change_paths.push(staged_change_path);
                    next_seq += 1;
                }
                "delete" => {
                    let item_key = change
                        .item_key
                        .clone()
                        .unwrap_or_else(|| format!("{}_{}_unknown", device_id, change.entity_id));

                    // Stage tombstone
                    let tombstone = RemoteTombstone {
                        schema_version: 1,
                        item_key: item_key.clone(),
                        deleted_at: chrono::Utc::now().to_rfc3339(),
                        deleted_by: device_id.to_string(),
                    };
                    let tombstone_bytes = encode_tombstone(tombstone)?;
                    let tombstone_path = build_tombstone_path(&item_key);
                    let tmp_run_path = join_remote_path(".tmp", run_id);
                    let staged_tombstone_path = join_remote_path(
                        remote_root,
                        &join_remote_path(&tmp_run_path, &tombstone_path),
                    );
                    provider
                        .put(&staged_tombstone_path, tombstone_bytes)
                        .await?;
                    tombstone_paths.push(staged_tombstone_path);

                    // Build and stage delete change log entry
                    let change_entry = RemoteChange {
                        schema_version: 1,
                        seq: next_seq,
                        device_id: device_id.to_string(),
                        operation: ChangeOperation::Delete,
                        entity_type: change.entity_type.clone(),
                        item_key,
                        item_path: None,
                        blob_path: None,
                        tombstone_path: Some(tombstone_path.clone()),
                        changed_at: chrono::Utc::now().to_rfc3339(),
                    };
                    let change_bytes = encode_change(change_entry)?;
                    let change_path = build_change_path(device_id, next_seq);
                    let staged_change_path = join_remote_path(
                        remote_root,
                        &join_remote_path(&tmp_run_path, &change_path),
                    );
                    provider.put(&staged_change_path, change_bytes).await?;
                    change_paths.push(staged_change_path);
                    next_seq += 1;
                }
                _ => {
                    log::warn!(
                        "[Sync::Engine] Unknown operation in change {}: {}",
                        change.id,
                        change.operation
                    );
                }
            }
        }

        Ok(StagedCommit {
            run_id: run_id.to_string(),
            item_paths,
            change_paths,
            tombstone_paths,
            warnings,
        })
    }

    /// Commit staged changes by moving from .tmp/ to final paths
    async fn commit_staged_changes(
        &self,
        provider: &dyn SyncProvider,
        remote_root: &str,
        run_id: &str,
        staged: &StagedCommit,
    ) -> Result<i64, SyncError> {
        let mut items_uploaded: i64 = 0;

        // Commit items
        for staged_path in &staged.item_paths {
            // Extract final path from .tmp/<run_id>/<final_path>
            let prefix = join_remote_path(remote_root, &join_remote_path(".tmp", run_id)) + "/";
            if let Some(final_path) = staged_path.strip_prefix(&prefix) {
                let final_full_path = join_remote_path(remote_root, final_path);
                // Copy by reading and re-uploading
                let data = provider.get(staged_path).await?;
                provider.put(&final_full_path, data).await?;
                items_uploaded += 1;
            }
        }

        // Commit changes
        for staged_path in &staged.change_paths {
            let prefix = join_remote_path(remote_root, &join_remote_path(".tmp", run_id)) + "/";
            if let Some(final_path) = staged_path.strip_prefix(&prefix) {
                let final_full_path = join_remote_path(remote_root, final_path);
                // Copy by reading and re-uploading
                let data = provider.get(staged_path).await?;
                provider.put(&final_full_path, data).await?;
            }
        }

        // Commit tombstones
        for staged_path in &staged.tombstone_paths {
            let prefix = join_remote_path(remote_root, &join_remote_path(".tmp", run_id)) + "/";
            if let Some(final_path) = staged_path.strip_prefix(&prefix) {
                let final_full_path = join_remote_path(remote_root, final_path);
                // Copy by reading and re-uploading
                let data = provider.get(staged_path).await?;
                provider.put(&final_full_path, data).await?;
            }
        }

        // Clean up .tmp directory after successful commit
        let tmp_prefix = join_remote_path(remote_root, &join_remote_path(".tmp", run_id)) + "/";
        if let Err(e) = provider.delete(&tmp_prefix).await {
            log::warn!("[Sync::Engine] Failed to cleanup .tmp directory: {}", e);
        }

        Ok(items_uploaded)
    }

    /// Cancel running sync
    pub async fn cancel_run(&self, profile_id: &str) -> Result<(), SyncError> {
        log::info!("[Sync::Engine] Cancelling sync for profile: {}", profile_id);
        let mut cancelled = self.cancelled_runs.write().await;
        if !cancelled.contains(&profile_id.to_string()) {
            cancelled.push(profile_id.to_string());
        }
        Ok(())
    }

    pub async fn get_last_report(&self, profile_id: &str) -> Option<SyncRunReport> {
        let reports = self.last_reports.read().await;
        if let Some(report) = reports.get(profile_id).cloned() {
            return Some(report);
        }
        drop(reports);
        self.repository
            .get_last_run_report(profile_id)
            .await
            .ok()
            .flatten()
    }

    /// Get sync status
    pub async fn get_status(&self, profile_id: &str) -> Result<SyncStatus, SyncError> {
        let runs = self.current_runs.read().await;
        let is_running = runs.contains(&profile_id.to_string());
        drop(runs);
        let profile = self.repository.get_profile(profile_id).await?;
        let is_locked = profile.encryption.enabled && !self.is_profile_unlocked(profile_id).await;
        if is_running {
            let statuses = self.current_status.read().await;
            if let Some(status) = statuses.get(profile_id) {
                let mut status = status.clone();
                status.is_paused = profile.schedule.paused;
                status.is_locked = is_locked;
                return Ok(status);
            }
        }
        let last_report = self.get_last_report(profile_id).await;

        Ok(SyncStatus {
            profile_id: profile_id.to_string(),
            status: if is_running {
                SyncRunStatus::Uploading
            } else {
                SyncRunStatus::Idle
            },
            phase: if is_running {
                Some("running".to_string())
            } else {
                Some("idle".to_string())
            },
            progress: if is_running { Some(0.5) } else { None },
            last_sync_at: last_report.and_then(|report| report.completed_at),
            next_sync_at: None,
            is_paused: profile.schedule.paused,
            is_locked,
            backoff_reason: if profile.schedule.paused {
                Some("Profile is paused".to_string())
            } else if is_locked {
                Some("Encrypted profile is locked".to_string())
            } else {
                None
            },
        })
    }
}

fn local_item_to_remote(
    local: ClipboardItem,
    item_key: String,
    device_id: &str,
) -> RemoteClipboardItem {
    RemoteClipboardItem {
        schema_version: 1,
        item_key,
        device_id: device_id.to_string(),
        local_id: local.id,
        item_type: local.item_type,
        content: Some(local.content),
        blob_path: None,
        blob_mime: None,
        content_hash: local.content_hash,
        created_at: local
            .created_at
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        updated_at: local
            .updated_at
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        tab_name: None,
        tags: parse_local_tags(local.tags.as_deref()),
        is_pinned: local.is_pinned.unwrap_or(0) != 0,
        is_sensitive: local.is_sensitive.unwrap_or(0) != 0,
        metadata: local
            .metadata
            .as_deref()
            .and_then(|metadata| serde_json::from_str(metadata).ok())
            .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new())),
        revision: 0,
        last_modified_by: device_id.to_string(),
        deleted: false,
    }
}

fn parse_local_tags(tags: Option<&str>) -> Vec<String> {
    let Some(tags) = tags else {
        return Vec::new();
    };
    if tags.trim().is_empty() {
        return Vec::new();
    }
    if let Ok(values) = serde_json::from_str::<Vec<String>>(tags) {
        return values;
    }
    tags.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

// Need to add scopeguard to Cargo.toml
mod scopeguard {
    pub fn guard<T, F: FnOnce(T)>(value: T, drop_fn: F) -> impl Drop {
        Guard {
            value: Some(value),
            drop_fn: Some(drop_fn),
        }
    }

    pub struct Guard<T, F: FnOnce(T)> {
        value: Option<T>,
        drop_fn: Option<F>,
    }

    impl<T, F: FnOnce(T)> Drop for Guard<T, F> {
        fn drop(&mut self) {
            if let (Some(value), Some(drop_fn)) = (self.value.take(), self.drop_fn.take()) {
                drop_fn(value);
            }
        }
    }
}
