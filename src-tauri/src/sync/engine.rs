/// Sync Engine - orchestrates the sync process
use crate::sync::crypto::{decrypt, derive_key, encrypt};
use crate::sync::error::SyncError;
use crate::sync::lock::{acquire_remote_lock, release_remote_lock, renew_remote_lock};
use crate::sync::manifest::{read_manifest, write_manifest, APP_NAME, SNAPSHOT_SCHEMA_VERSION};
use crate::sync::models::*;
use crate::sync::providers::{join_remote_path, SyncProvider};
use crate::sync::repository::SyncRepository;
use crate::sync::secrets::SecretStore;
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

pub struct SyncEngine {
    repository: Arc<SyncRepository>,
    #[allow(dead_code)]
    secret_store: Arc<SecretStore>, // Reserved for future credential store integration
    operation_gates: RwLock<HashMap<String, Arc<Mutex<()>>>>,
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
    warnings: Vec<String>,
    // Failed objects remain referenced until a later pull can read/apply them.
    retained_shards: Vec<SnapshotShardRef>,
    retain_order: bool,
    retain_plugins: bool,
    readable_entities: HashSet<(String, String)>,
}

const SNAPSHOT_SHARD_SIZE: usize = 100;
const SNAPSHOT_INLINE_CONTENT_LIMIT: usize = 64 * 1024;

#[derive(Debug, Serialize, Deserialize)]
struct SnapshotEncryptionEnvelope {
    schema_version: u32,
    encrypted_data: String,
    algorithm: String,
}

impl SyncEngine {
    pub fn new(repository: Arc<SyncRepository>, secret_store: Arc<SecretStore>) -> Self {
        Self {
            repository,
            secret_store,
            operation_gates: RwLock::new(HashMap::new()),
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
        let gate = self.operation_gate(profile_id).await;
        let _operation = gate
            .try_lock_owned()
            .map_err(|_| SyncError::Cancelled("Sync already running for this profile".into()))?;
        self.run_now_guarded(profile_id, provider).await
    }

    async fn operation_gate(&self, profile_id: &str) -> Arc<Mutex<()>> {
        self.operation_gates
            .write()
            .await
            .entry(profile_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub async fn resolve_conflict_and_run(
        &self,
        profile_id: &str,
        provider: Result<Arc<dyn SyncProvider>, SyncError>,
        conflict_id: i64,
        resolution: ConflictResolutionInput,
    ) -> Result<ConflictResolutionResult, SyncError> {
        // This per-profile gate spans resolution and publication, so a running
        // pull cannot overwrite a user's choice or acknowledge it prematurely.
        // User actions wait for the active run; scheduled triggers still skip it.
        let gate = self.operation_gate(profile_id).await;
        let _operation = gate.lock_owned().await;
        self.repository
            .resolve_conflict(conflict_id, resolution)
            .await?;
        let outcome = match provider {
            Ok(provider) => self.run_now_guarded(profile_id, provider).await,
            Err(error) => Err(error),
        };
        Ok(match outcome {
            Ok(report) => ConflictResolutionResult {
                report: Some(report),
                sync_error: None,
            },
            Err(error) => ConflictResolutionResult {
                report: None,
                sync_error: Some(error.to_string()),
            },
        })
    }

    async fn run_now_guarded(
        &self,
        profile_id: &str,
        provider: Arc<dyn SyncProvider>,
    ) -> Result<SyncRunReport, SyncError> {
        log::info!("[Sync::Engine] Sync requested for profile: {}", profile_id);

        // Check and mark under one write lock so concurrent triggers cannot
        // start multiple runs for the same profile and fight over remote locks.
        let profile_id_string = profile_id.to_string();
        {
            let mut runs = self.current_runs.write().await;
            if runs.contains(&profile_id_string) {
                return Err(SyncError::Cancelled(
                    "Sync already running for this profile".to_string(),
                ));
            }
            runs.push(profile_id_string.clone());
        }
        {
            let mut cancelled = self.cancelled_runs.write().await;
            cancelled.retain(|id| id != profile_id);
        }

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
                warnings: Vec::new(),
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

        {
            let mut runs = self.current_runs.write().await;
            if let Some(pos) = runs.iter().position(|p| p == &profile_id_string) {
                runs.remove(pos);
            }
        }

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
        let sync_work = async {
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
            self.apply_conflict_policy(&prepared.profile, &mut report)
                .await?;
            self.set_status(
                profile_id,
                SyncRunStatus::Uploading,
                "upload_local_changes",
                0.7,
                None,
            )
            .await;
            match self
                .upload_local_changes(provider.as_ref(), &prepared, &report)
                .await
            {
                Ok(upload_report) => {
                    report.items_uploaded = upload_report.items_uploaded;
                    report.errors.extend(upload_report.errors);
                    report.warnings.extend(upload_report.warnings);
                }
                Err(SyncError::Cancelled(message)) => return Err(SyncError::Cancelled(message)),
                Err(error) => report
                    .errors
                    .push(format!("Upload retained for retry: {}", error)),
            }

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
        };
        let keep_lock_alive = async {
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                renew_remote_lock(provider.as_ref(), lock_guard.path(), &run_id).await?;
            }
            #[allow(unreachable_code)]
            Ok::<(), SyncError>(())
        };
        let sync_result = tokio::select! {
            result = sync_work => result,
            result = keep_lock_alive => result.and_then(|_| Err(SyncError::Lock("Remote lock renewal ended unexpectedly".to_string()))),
        };

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
        let mut profile = self.repository.get_profile(profile_id).await?;
        self.repository
            .prune_missing_tab_scope_ids(&mut profile)
            .await?;
        if profile.encryption.enabled && !self.is_profile_unlocked(profile_id).await {
            return Err(SyncError::Encryption(
                "Encrypted profile is locked. Unlock it before syncing.".to_string(),
            ));
        }
        let device_id = self.repository.get_or_create_device_id().await?;
        log::info!("[Sync::Engine] Step 2: Testing provider availability");
        let _manifest = read_manifest(provider, "")
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

    async fn read_snapshot<T: serde::de::DeserializeOwned>(
        &self,
        provider: &dyn SyncProvider,
        profile: &SyncProfile,
        path: &str,
        expected_hash: &str,
        report: &mut SyncPhaseReport,
    ) -> Result<T, SyncError> {
        let bytes = provider.get(path).await?;
        let crypto_key = self.get_crypto_key(&profile.id).await?;
        let decoded = decode_snapshot_file(&bytes, profile, crypto_key.as_ref())?;
        // The manifest can lag behind an interrupted legacy upload. A hash
        // mismatch is advisory; authenticated decryption and parsing still apply.
        let value = serde_json::from_slice(&decoded)
            .map_err(|_| SyncError::validation(format!("Invalid snapshot JSON: {}", path)))?;
        if sha256_hex(&decoded) != expected_hash {
            let warning = format!("Snapshot hash mismatch recovered: {}", path);
            log::warn!("[Sync::Engine] {}", warning);
            report.warnings.push(warning);
        }
        Ok(value)
    }

    async fn pull_remote_changes(
        &self,
        profile_id: &str,
        provider: &dyn SyncProvider,
        prepared: &PreparedSyncRun,
    ) -> Result<SyncPhaseReport, SyncError> {
        let mut report = SyncPhaseReport::default();
        let Some(manifest) = read_manifest(provider, "").await? else {
            return Ok(report);
        };
        let profile = &prepared.profile;
        let mut remote_order = None;
        if let Some(reference) = &manifest.order {
            match self
                .read_snapshot::<SnapshotOrder>(
                    provider,
                    profile,
                    &reference.path,
                    &reference.hash,
                    &mut report,
                )
                .await
            {
                Ok(order) if matches!(order.schema_version, 1 | SNAPSHOT_SCHEMA_VERSION) => {
                    remote_order = Some(order)
                }
                result => {
                    report.retain_order = true;
                    report.errors.push(format!(
                        "Order snapshot retained for retry: {} ({})",
                        reference.path,
                        result
                            .err()
                            .map(|e| e.to_string())
                            .unwrap_or_else(|| "unsupported schema".into())
                    ));
                }
            }
        }
        if let Some(reference) = &manifest.plugin_data {
            match self
                .read_snapshot::<Vec<RemotePluginData>>(
                    provider,
                    profile,
                    &reference.path,
                    &reference.hash,
                    &mut report,
                )
                .await
            {
                Ok(records) => {
                    for record in records {
                        let selected = profile
                            .sync_plugins
                            .selected_plugin_ids
                            .contains(&record.plugin_id);
                        let entity_key =
                            serde_json::to_string(&(&record.plugin_id, &record.storage_key))?;
                        match self
                            .repository
                            .apply_snapshot_plugin_data(vec![record])
                            .await
                        {
                            Ok(()) => {
                                if selected {
                                    report
                                        .readable_entities
                                        .insert(("plugin_data".into(), entity_key));
                                }
                            }
                            Err(error) => {
                                report.retain_plugins = true;
                                report
                                    .errors
                                    .push(format!("Plugin record retained for retry: {}", error));
                            }
                        }
                    }
                }
                Err(error) => {
                    report.retain_plugins = true;
                    report.errors.push(format!(
                        "Plugin snapshot retained for retry: {} ({})",
                        reference.path, error
                    ));
                }
            }
        }
        let crypto_key = self.get_crypto_key(profile_id).await?;
        for reference in &manifest.item_shards {
            self.ensure_not_cancelled(profile_id).await?;
            let shard = self
                .read_snapshot::<SnapshotItemShard>(
                    provider,
                    profile,
                    &reference.path,
                    &reference.hash,
                    &mut report,
                )
                .await;
            let shard = match shard {
                Ok(shard) if shard.schema_version == SNAPSHOT_SCHEMA_VERSION => shard,
                result => {
                    report.retained_shards.push(reference.clone());
                    report.errors.push(format!(
                        "Item snapshot retained for retry: {} ({})",
                        reference.path,
                        result
                            .err()
                            .map(|e| e.to_string())
                            .unwrap_or_else(|| "unsupported schema".into())
                    ));
                    continue;
                }
            };
            let mut retain_shard = false;
            for mut item in shard.items {
                // An absent record is not a deletion; only mapped tombstones
                // may delete local data. Legacy deleted records stay untouched.
                if item.deleted {
                    continue;
                }
                let item_key = item.item_key.clone();
                let apply = async {
                    if item.item_key.trim().is_empty()
                        || !matches!(item.schema_version, 1 | SNAPSHOT_SCHEMA_VERSION)
                    {
                        return Err(SyncError::validation(
                            "Invalid remote item identity or schema",
                        ));
                    }
                    if item.content.is_none() && item.blob_path.is_none() {
                        return Err(SyncError::validation("Remote item has no content or blob"));
                    }
                    if item.content.is_none() {
                        if let Some(path) = &item.blob_path {
                            let bytes = provider.get(path).await?;
                            let decoded =
                                decode_snapshot_file(&bytes, profile, crypto_key.as_ref())?;
                            item.content = Some(String::from_utf8(decoded).map_err(|_| {
                                SyncError::validation("Remote blob is not valid UTF-8")
                            })?);
                        }
                    }
                    self.repository.apply_remote_item(profile, item, None).await
                }
                .await;
                if matches!(apply, Ok(ApplyItemResult::Conflict { .. })) {
                    report
                        .readable_entities
                        .insert(("clipboard_item".into(), item_key.clone()));
                }
                match apply {
                    Ok(
                        ApplyItemResult::Created { .. }
                        | ApplyItemResult::Updated { .. }
                        | ApplyItemResult::Merged { .. },
                    ) => report.items_downloaded += 1,
                    Ok(ApplyItemResult::Conflict { .. } | ApplyItemResult::Skipped) => {}
                    Err(error) => {
                        retain_shard = true;
                        report
                            .errors
                            .push(format!("Item {} retained for retry: {}", item_key, error));
                    }
                }
            }
            if retain_shard {
                report.retained_shards.push(reference.clone());
            }
        }
        // Never prune after a partial pull, and do not let ordering failure
        // block publication of successfully recovered clipboard items.
        if let Some(order) = remote_order {
            if let Err(error) = self
                .repository
                .apply_snapshot_order(profile, &order, false)
                .await
            {
                report.retain_order = true;
                report
                    .errors
                    .push(format!("Order retained for retry: {}", error));
            }
        }
        report.conflicts_found = self.repository.pending_conflicts().await?.len() as i64;
        Ok(report)
    }

    async fn apply_conflict_policy(
        &self,
        profile: &SyncProfile,
        report: &mut SyncPhaseReport,
    ) -> Result<(), SyncError> {
        let resolution = match profile.conflict_policy {
            ConflictPolicy::Manual => return Ok(()),
            ConflictPolicy::PreferLocal => ConflictResolutionInput::UseLocal,
            ConflictPolicy::PreferRemote => ConflictResolutionInput::UseRemote,
        };
        for conflict in self.repository.pending_conflicts().await? {
            // Only resolve entities successfully read from this remote snapshot.
            // Unreadable shards, excluded entities and absent rows stay pending.
            if !report
                .readable_entities
                .contains(&(conflict.entity_type.clone(), conflict.entity_key.clone()))
                || !self
                    .repository
                    .conflict_is_in_scope(profile, &conflict)
                    .await?
            {
                continue;
            }
            if let Err(error) = self
                .repository
                .resolve_conflict(conflict.id, resolution.clone())
                .await
            {
                report.errors.push(format!(
                    "Automatic conflict resolution retained for retry: {}",
                    error
                ));
            }
        }
        report.conflicts_found = self.repository.pending_conflicts().await?.len() as i64;
        Ok(())
    }

    async fn upload_local_changes(
        &self,
        provider: &dyn SyncProvider,
        prepared: &PreparedSyncRun,
        pull: &SyncPhaseReport,
    ) -> Result<SyncPhaseReport, SyncError> {
        log::info!("[Sync::Engine] Step 7: Reading local snapshot changes");
        let deduplicated = self
            .repository
            .deduplicate_same_content_items(&prepared.profile)
            .await?;
        if deduplicated > 0 {
            log::info!(
                "[Sync::Engine] Removed {} duplicate local items before upload",
                deduplicated
            );
        }
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

        if unsynced_changes.is_empty() && pull.warnings.is_empty() && pull.errors.is_empty() {
            if self
                .remote_order_matches_local(provider, &prepared.profile)
                .await?
            {
                log::info!(
                    "[Sync::Engine] No unsynced changes; checking for unreferenced remote objects"
                );
                return self
                    .cleanup_snapshot_objects_for_local_state(
                        provider,
                        &prepared.profile,
                        &prepared.device_id,
                    )
                    .await;
            }
            log::info!(
                "[Sync::Engine] Snapshot order differs from remote; uploading repaired snapshot"
            );
            return self
                .upload_snapshot_preserving(provider, &prepared.profile, &prepared.device_id, pull)
                .await;
        }

        log::info!(
            "[Sync::Engine] Found {} unsynced changes",
            unsynced_changes.len()
        );
        log::info!("[Sync::Engine] Step 8: Uploading local snapshot");
        let upload_report = self
            .upload_snapshot_preserving(provider, &prepared.profile, &prepared.device_id, pull)
            .await?;
        let conflicts = self.repository.pending_conflicts().await?;
        let change_ids: Vec<i64> = unsynced_changes
            .iter()
            .filter(|change| {
                if pull.retain_plugins && change.entity_type == "plugin_data" {
                    return false;
                }
                !conflicts.iter().any(|conflict| {
                    (conflict.entity_type == "clipboard_item"
                        && change.item_key.as_deref() == Some(&conflict.entity_key))
                        || (conflict.entity_type == "plugin_data"
                            && serde_json::from_str::<RemotePluginData>(&conflict.remote_payload)
                                .map(|record| {
                                    change.plugin_id.as_deref() == Some(record.plugin_id.as_str())
                                        && change.entity_id == record.storage_key
                                })
                                .unwrap_or(false))
                })
            })
            .map(|change| change.id)
            .collect();
        self.repository.mark_changes_synced(&change_ids).await?;

        Ok(SyncPhaseReport {
            items_uploaded: upload_report.items_uploaded,
            errors: upload_report.errors,
            warnings: upload_report.warnings,
            ..SyncPhaseReport::default()
        })
    }

    async fn remote_order_matches_local(
        &self,
        provider: &dyn SyncProvider,
        profile: &SyncProfile,
    ) -> Result<bool, SyncError> {
        let Some(manifest) = read_manifest(provider, "").await? else {
            return Ok(false);
        };
        let Some(order_ref) = manifest.order else {
            return Ok(false);
        };
        let remote_bytes = provider.get(&order_ref.path).await?;
        let crypto_key = self.get_crypto_key(&profile.id).await.ok().flatten();
        let decoded = decode_snapshot_file(&remote_bytes, profile, crypto_key.as_ref())?;
        let remote_order: SnapshotOrder = serde_json::from_slice(&decoded)?;
        let local_order = self.repository.build_snapshot_order(profile).await?;
        if remote_order.tabs != local_order.tabs {
            return Ok(false);
        }
        let plugin_data = self.repository.list_snapshot_plugin_data().await?;
        let plugin_hash = sha256_hex(&serde_json::to_vec(&plugin_data)?);
        Ok(manifest.plugin_data.as_ref().map(|data| data.hash.as_str())
            == Some(plugin_hash.as_str()))
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

        for message in &report.errors {
            self.repository
                .record_log(Some(profile_id), Some(run_id), "WARN", message)
                .await?;
        }
        let run_report = SyncRunReport {
            profile_id: profile_id.to_string(),
            run_id: run_id.to_string(),
            status: if report.errors.is_empty() && report.conflicts_found == 0 {
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
            warnings: report.warnings.clone(),
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

    #[cfg(test)]
    async fn upload_snapshot(
        &self,
        provider: &dyn SyncProvider,
        profile: &SyncProfile,
        device_id: &str,
    ) -> Result<SyncPhaseReport, SyncError> {
        self.upload_snapshot_preserving(provider, profile, device_id, &SyncPhaseReport::default())
            .await
    }

    async fn put_snapshot(
        &self,
        provider: &dyn SyncProvider,
        profile: &SyncProfile,
        path: &str,
        bytes: &[u8],
        crypto_key: Option<&secrecy::SecretVec<u8>>,
    ) -> Result<(), SyncError> {
        provider
            .put(path, encode_snapshot_file(bytes, profile, crypto_key)?)
            .await?;
        let stored = provider.get(path).await?;
        let decoded = decode_snapshot_file(&stored, profile, crypto_key)?;
        if decoded != bytes {
            return Err(SyncError::validation(format!(
                "Snapshot upload verification failed: {}",
                path
            )));
        }
        Ok(())
    }

    async fn upload_snapshot_preserving(
        &self,
        provider: &dyn SyncProvider,
        profile: &SyncProfile,
        device_id: &str,
        pull: &SyncPhaseReport,
    ) -> Result<SyncPhaseReport, SyncError> {
        let previous_manifest = read_manifest(provider, "").await?;
        let crypto_key = self.get_crypto_key(&profile.id).await?;
        let mut snapshot_items = self
            .repository
            .list_snapshot_items(profile, device_id)
            .await?;
        let conflicts = self.repository.pending_conflicts().await?;
        // Publish the remote side of pending conflicts. Local versions remain in
        // SQLite/outbox and are only published after the user's explicit choice.
        for conflict in &conflicts {
            if conflict.entity_type != "clipboard_item" {
                continue;
            }
            let remote: RemoteClipboardItem = serde_json::from_str(&conflict.remote_payload)?;
            if let Some(local) = snapshot_items
                .iter_mut()
                .find(|item| item.item_key == conflict.entity_key)
            {
                *local = remote;
            } else if profile.sync_tabs.mode == TabSyncMode::All {
                snapshot_items.push(remote);
            }
        }
        let mut buckets: BTreeMap<i64, Vec<RemoteClipboardItem>> = BTreeMap::new();
        let mut current_blob_paths = HashSet::new();
        for item in snapshot_items {
            let start = ((item.stable_seq.saturating_sub(1)) / SNAPSHOT_SHARD_SIZE as i64)
                * SNAPSHOT_SHARD_SIZE as i64
                + 1;
            buckets.entry(start).or_default().push(item);
        }
        let mut shard_refs = pull.retained_shards.clone();
        let mut items_uploaded = 0;
        for (start, bucket_items) in buckets {
            let mut items = Vec::with_capacity(bucket_items.len());
            for mut item in bucket_items {
                if let Some(content) = item.content.take() {
                    let bytes = content.into_bytes();
                    if bytes.len() > SNAPSHOT_INLINE_CONTENT_LIMIT {
                        let hash = sha256_hex(&bytes);
                        let path = format!("blobs/{}.bin", hash);
                        if provider.stat(&path).await?.is_none() {
                            self.put_snapshot(
                                provider,
                                profile,
                                &path,
                                &bytes,
                                crypto_key.as_ref(),
                            )
                            .await?;
                        }
                        item.blob_path = Some(path);
                    } else {
                        item.content = Some(String::from_utf8(bytes).map_err(|_| {
                            SyncError::validation("Local content is not valid UTF-8")
                        })?);
                    }
                }
                if let Some(path) = &item.blob_path {
                    current_blob_paths.insert(path.clone());
                }
                items.push(item);
            }
            let shard = SnapshotItemShard {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                start,
                end: start + SNAPSHOT_SHARD_SIZE as i64 - 1,
                items,
            };
            let bytes = serde_json::to_vec(&shard)?;
            let hash = sha256_hex(&bytes);
            let path = format!(
                "items/{:08}-{:08}-{}.json",
                shard.start.max(0),
                shard.end.max(0),
                hash
            );
            let unchanged = previous_manifest
                .as_ref()
                .map(|m| {
                    m.item_shards
                        .iter()
                        .any(|r| r.path == path && r.hash == hash)
                })
                .unwrap_or(false);
            if !unchanged || !pull.warnings.is_empty() {
                self.put_snapshot(provider, profile, &path, &bytes, crypto_key.as_ref())
                    .await?;
                items_uploaded += shard.items.len() as i64;
            }
            if !shard_refs.iter().any(|r| r.path == path) {
                shard_refs.push(SnapshotShardRef {
                    path,
                    start: shard.start,
                    end: shard.end,
                    count: shard.items.len(),
                    hash,
                });
            }
        }
        let order_ref = if pull.retain_order {
            previous_manifest.as_ref().and_then(|m| m.order.clone())
        } else {
            let mut order = self.repository.build_snapshot_order(profile).await?;
            for conflict in &conflicts {
                if conflict.entity_type != "clipboard_item" {
                    continue;
                }
                let remote: RemoteClipboardItem = serde_json::from_str(&conflict.remote_payload)?;
                for tab in &mut order.tabs {
                    tab.pinned.retain(|key| key != &remote.item_key);
                    tab.normal.retain(|key| key != &remote.item_key);
                }
                let tab_key = remote.tab_key.clone().unwrap_or_else(|| "default".into());
                let index = match order.tabs.iter().position(|tab| tab.tab_key == tab_key) {
                    Some(index) => index,
                    None => {
                        order.tabs.push(SnapshotTabOrder {
                            tab_key,
                            tab_name: remote.tab_name.clone(),
                            pinned: Vec::new(),
                            normal: Vec::new(),
                        });
                        order.tabs.len() - 1
                    }
                };
                if remote.is_pinned {
                    order.tabs[index].pinned.push(remote.item_key);
                } else {
                    order.tabs[index].normal.push(remote.item_key);
                }
            }
            let bytes = serde_json::to_vec(&order)?;
            let hash = sha256_hex(&bytes);
            let path = format!("order/{}.json", hash);
            let unchanged = previous_manifest
                .as_ref()
                .and_then(|m| m.order.as_ref())
                .map(|r| r.path == path && r.hash == hash)
                .unwrap_or(false);
            if !unchanged || !pull.warnings.is_empty() {
                self.put_snapshot(provider, profile, &path, &bytes, crypto_key.as_ref())
                    .await?;
            }
            Some(SnapshotFileRef { path, hash })
        };
        let mut published_plugin_records = Vec::new();
        let plugin_ref = if pull.retain_plugins {
            previous_manifest
                .as_ref()
                .and_then(|m| m.plugin_data.clone())
        } else {
            let mut records = self.repository.list_snapshot_plugin_data().await?;
            for conflict in &conflicts {
                if conflict.entity_type != "plugin_data" {
                    continue;
                }
                let remote: RemotePluginData = serde_json::from_str(&conflict.remote_payload)?;
                if let Some(local) = records.iter_mut().find(|r| {
                    r.plugin_id == remote.plugin_id && r.storage_key == remote.storage_key
                }) {
                    *local = remote;
                }
            }
            published_plugin_records = records.clone();
            let bytes = serde_json::to_vec(&records)?;
            let hash = sha256_hex(&bytes);
            let path = format!("plugins/{}.json", hash);
            let unchanged = previous_manifest
                .as_ref()
                .and_then(|m| m.plugin_data.as_ref())
                .map(|r| r.path == path && r.hash == hash)
                .unwrap_or(false);
            if !unchanged || !pull.warnings.is_empty() {
                self.put_snapshot(provider, profile, &path, &bytes, crypto_key.as_ref())
                    .await?;
            }
            Some(SnapshotFileRef { path, hash })
        };
        let manifest = SnapshotManifest {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            app: APP_NAME.to_string(),
            generation: previous_manifest
                .as_ref()
                .map(|m| m.generation + 1)
                .unwrap_or(1),
            device_id: device_id.to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            item_shard_size: SNAPSHOT_SHARD_SIZE,
            item_shards: shard_refs,
            order: order_ref,
            plugin_data: plugin_ref,
        };
        // Every referenced object is complete before publishing the manifest.
        // Failed pulls keep their original refs/blobs for future recovery.
        write_manifest(provider, "", &manifest).await?;
        for record in &published_plugin_records {
            self.repository.remember_plugin_remote(record).await?;
        }
        let errors = if !pull.retained_shards.is_empty() || pull.retain_order || pull.retain_plugins
        {
            Vec::new()
        } else {
            self.cleanup_unreferenced_snapshot_objects(provider, &manifest, &current_blob_paths)
                .await
        };
        Ok(SyncPhaseReport {
            items_uploaded,
            errors,
            ..SyncPhaseReport::default()
        })
    }

    async fn cleanup_snapshot_objects_for_local_state(
        &self,
        provider: &dyn SyncProvider,
        profile: &SyncProfile,
        _device_id: &str,
    ) -> Result<SyncPhaseReport, SyncError> {
        let Some(manifest) = read_manifest(provider, "").await? else {
            return Ok(SyncPhaseReport::default());
        };
        // GC follows the committed remote manifest, never a mutable local view.
        // A local edit arriving during sync must not delete a still-referenced blob.
        let mut report = SyncPhaseReport::default();
        let mut current_blob_paths = HashSet::new();
        for reference in &manifest.item_shards {
            match self
                .read_snapshot::<SnapshotItemShard>(
                    provider,
                    profile,
                    &reference.path,
                    &reference.hash,
                    &mut report,
                )
                .await
            {
                Ok(shard) if shard.schema_version == SNAPSHOT_SCHEMA_VERSION => {
                    current_blob_paths
                        .extend(shard.items.into_iter().filter_map(|item| item.blob_path));
                }
                _ => {
                    report.errors.push(format!(
                        "Snapshot unreadable; cleanup deferred: {}",
                        reference.path
                    ));
                    return Ok(report);
                }
            }
        }
        let errors = self
            .cleanup_unreferenced_snapshot_objects(provider, &manifest, &current_blob_paths)
            .await;
        report.errors.extend(errors);
        Ok(report)
    }

    async fn cleanup_unreferenced_snapshot_objects(
        &self,
        provider: &dyn SyncProvider,
        manifest: &SnapshotManifest,
        current_blob_paths: &HashSet<String>,
    ) -> Vec<String> {
        let mut errors = Vec::new();
        let current_shards: HashSet<&str> = manifest
            .item_shards
            .iter()
            .map(|shard| shard.path.as_str())
            .collect();

        match provider.list("items/").await {
            Ok(objects) => {
                for object in objects {
                    if !object.path.ends_with(".json")
                        || current_shards.contains(object.path.as_str())
                    {
                        continue;
                    }
                    if let Err(error) = provider.delete(&object.path).await {
                        let message = format!(
                            "Failed to delete unreferenced shard {}: {}",
                            object.path, error
                        );
                        log::warn!("[Sync::Engine] {}", message);
                        errors.push(message);
                    }
                }
            }
            Err(error) => {
                let message = format!("Failed to list snapshot shards for cleanup: {}", error);
                log::warn!("[Sync::Engine] {}", message);
                errors.push(message);
            }
        }

        for (prefix, reference) in [
            ("order/", manifest.order.as_ref()),
            ("plugins/", manifest.plugin_data.as_ref()),
        ] {
            match provider.list(prefix).await {
                Ok(objects) => {
                    for object in objects {
                        if !object.path.ends_with(".json")
                            || reference.map(|r| r.path == object.path).unwrap_or(false)
                        {
                            continue;
                        }
                        if let Err(error) = provider.delete(&object.path).await {
                            errors.push(format!(
                                "Failed to delete unreferenced snapshot {}: {}",
                                object.path, error
                            ));
                        }
                    }
                }
                Err(error) => {
                    errors.push(format!("Failed to list {} for cleanup: {}", prefix, error))
                }
            }
        }

        match provider.list("blobs/").await {
            Ok(objects) => {
                for object in objects {
                    if !object.path.ends_with(".bin") || current_blob_paths.contains(&object.path) {
                        continue;
                    }
                    if let Err(error) = provider.delete(&object.path).await {
                        let message = format!(
                            "Failed to delete unreferenced blob {}: {}",
                            object.path, error
                        );
                        log::warn!("[Sync::Engine] {}", message);
                        errors.push(message);
                    }
                }
            }
            Err(error) => {
                let message = format!("Failed to list snapshot blobs for cleanup: {}", error);
                log::warn!("[Sync::Engine] {}", message);
                errors.push(message);
            }
        }
        errors
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

    pub async fn append_post_sync_error(
        &self,
        mut report: SyncRunReport,
        error: String,
    ) -> Result<SyncRunReport, SyncError> {
        report.status = SyncRunStatus::PartialSuccess;
        report.errors.push(error.clone());
        self.repository.save_run_report(&report).await?;
        self.last_reports
            .write()
            .await
            .insert(report.profile_id.clone(), report.clone());
        self.set_status(
            &report.profile_id,
            SyncRunStatus::PartialSuccess,
            "file_sync_refresh_failed",
            1.0,
            Some(error),
        )
        .await;
        Ok(report)
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

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{:02x}", byte)).collect()
}

fn encode_snapshot_file(
    bytes: &[u8],
    profile: &SyncProfile,
    crypto_key: Option<&secrecy::SecretVec<u8>>,
) -> Result<Vec<u8>, SyncError> {
    if !profile.encryption.enabled {
        return Ok(bytes.to_vec());
    }
    let key = crypto_key.ok_or_else(|| {
        SyncError::encryption("Encryption enabled but no crypto key provided".to_string())
    })?;
    let encrypted = encrypt(bytes, key)?;
    let envelope = SnapshotEncryptionEnvelope {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        encrypted_data: base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            encrypted,
        ),
        algorithm: profile.encryption.algorithm.clone(),
    };
    serde_json::to_vec(&envelope).map_err(SyncError::Serialization)
}

fn decode_snapshot_file(
    bytes: &[u8],
    profile: &SyncProfile,
    crypto_key: Option<&secrecy::SecretVec<u8>>,
) -> Result<Vec<u8>, SyncError> {
    if !profile.encryption.enabled {
        return Ok(bytes.to_vec());
    }
    let envelope: SnapshotEncryptionEnvelope = serde_json::from_slice(bytes).map_err(|e| {
        SyncError::validation(format!("Invalid snapshot encryption envelope: {}", e))
    })?;
    if envelope.schema_version != SNAPSHOT_SCHEMA_VERSION {
        return Err(SyncError::SchemaVersionMismatch {
            local: SNAPSHOT_SCHEMA_VERSION,
            remote: envelope.schema_version,
        });
    }
    let encrypted = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        envelope.encrypted_data,
    )
    .map_err(|e| SyncError::encryption(format!("Base64 decode failed: {}", e)))?;
    let key = crypto_key.ok_or_else(|| {
        SyncError::encryption("Encryption enabled but no crypto key provided".to_string())
    })?;
    decrypt(&encrypted, key)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use sqlx::SqlitePool;

    #[derive(Default)]
    struct MemoryProvider {
        objects: RwLock<HashMap<String, Vec<u8>>>,
        delete_failures: RwLock<HashSet<String>>,
        put_failures: RwLock<HashSet<String>>,
        corrupt_puts: RwLock<HashSet<String>>,
    }

    #[async_trait]
    impl SyncProvider for MemoryProvider {
        async fn test_connection(&self) -> Result<(), SyncError> {
            Ok(())
        }

        async fn list(&self, prefix: &str) -> Result<Vec<RemoteObject>, SyncError> {
            Ok(self
                .objects
                .read()
                .await
                .iter()
                .filter(|(path, _)| path.starts_with(prefix))
                .map(|(path, data)| RemoteObject {
                    path: path.clone(),
                    size: data.len() as u64,
                    modified_at: None,
                    etag: None,
                })
                .collect())
        }

        async fn stat(&self, path: &str) -> Result<Option<RemoteObject>, SyncError> {
            let objects = self.objects.read().await;
            Ok(objects.get(path).map(|data| RemoteObject {
                path: path.to_string(),
                size: data.len() as u64,
                modified_at: None,
                etag: None,
            }))
        }

        async fn get(&self, path: &str) -> Result<Vec<u8>, SyncError> {
            self.objects
                .read()
                .await
                .get(path)
                .cloned()
                .ok_or_else(|| SyncError::provider("File not found".to_string()))
        }

        async fn put(&self, path: &str, mut data: Vec<u8>) -> Result<(), SyncError> {
            if self
                .put_failures
                .read()
                .await
                .iter()
                .any(|prefix| path.starts_with(prefix))
            {
                return Err(SyncError::provider("Injected upload failure"));
            }
            if self
                .corrupt_puts
                .read()
                .await
                .iter()
                .any(|prefix| path.starts_with(prefix))
            {
                data.push(b' ');
            }
            self.objects.write().await.insert(path.to_string(), data);
            Ok(())
        }

        async fn mkdir_all(&self, _path: &str) -> Result<(), SyncError> {
            Ok(())
        }

        async fn move_object(&self, from: &str, to: &str) -> Result<(), SyncError> {
            let mut objects = self.objects.write().await;
            let data = objects
                .remove(from)
                .ok_or_else(|| SyncError::provider("File not found".to_string()))?;
            objects.insert(to.to_string(), data);
            Ok(())
        }

        async fn delete(&self, path: &str) -> Result<(), SyncError> {
            if self.delete_failures.read().await.contains(path) {
                return Err(SyncError::provider("Injected delete failure"));
            }
            self.objects.write().await.remove(path);
            Ok(())
        }
    }

    fn test_profile() -> SyncProfile {
        SyncProfile {
            id: "profile".to_string(),
            name: "Profile".to_string(),
            provider: SyncProviderKind::WebDav,
            remote_root: String::new(),
            sync_tabs: TabSyncSelection::default(),
            sync_plugins: PluginSyncSelection::default(),
            encryption: EncryptionConfig::default(),
            credential_refs: CredentialRefs::default(),
            schedule: SyncScheduleConfig::default(),
            conflict_policy: ConflictPolicy::default(),
            created_at: None,
            updated_at: None,
        }
    }

    async fn setup_order_test_pool() -> Result<SqlitePool, SyncError> {
        let pool = SqlitePool::connect(":memory:").await?;
        sqlx::query(
            r#"
            CREATE TABLE tabs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                is_default INTEGER DEFAULT 0,
                is_trash INTEGER DEFAULT 0,
                display_order INTEGER NOT NULL DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE clipboard_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                type TEXT NOT NULL DEFAULT 'text',
                content TEXT,
                content_hash TEXT,
                metadata TEXT,
                tags TEXT,
                tab_id INTEGER,
                is_sensitive INTEGER DEFAULT 0,
                is_pinned INTEGER DEFAULT 0,
                display_order INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
                ,deleted_at DATETIME,
                deleted_from_tab_id INTEGER
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE sync_item_map (
                local_id INTEGER PRIMARY KEY,
                item_key TEXT NOT NULL UNIQUE,
                stable_seq INTEGER,
                remote_path TEXT,
                last_remote_updated_at DATETIME,
                last_synced_at DATETIME
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"
            CREATE TABLE sync_changes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                operation TEXT NOT NULL,
                item_key TEXT,
                tab_id INTEGER,
                plugin_id TEXT,
                source TEXT DEFAULT 'local',
                changed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                synced_at DATETIME
            )
            "#,
        )
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO tabs (name, is_default) VALUES ('Clipboard', 1), ('Workspace', 0)",
        )
        .execute(&pool)
        .await?;
        sqlx::query("CREATE TABLE plugin_sync_data (plugin_id TEXT NOT NULL, storage_key TEXT NOT NULL, value_json TEXT NOT NULL, updated_at TEXT NOT NULL, PRIMARY KEY(plugin_id, storage_key))")
            .execute(&pool).await?;
        sqlx::query("CREATE TABLE sync_conflicts (id INTEGER PRIMARY KEY AUTOINCREMENT, entity_type TEXT NOT NULL, entity_key TEXT NOT NULL, local_payload TEXT NOT NULL, remote_payload TEXT NOT NULL, reason TEXT NOT NULL, status TEXT DEFAULT 'pending', resolution TEXT, created_at TEXT DEFAULT CURRENT_TIMESTAMP, resolved_at TEXT)")
            .execute(&pool).await?;
        for sql in [
            "CREATE TABLE sync_device (id INTEGER PRIMARY KEY, device_id TEXT)",
            "INSERT INTO sync_device VALUES (1, 'local-device')",
            "CREATE TABLE sync_profiles (id TEXT PRIMARY KEY, name TEXT, provider TEXT, remote_root TEXT, config_json TEXT, credential_refs_json TEXT, created_at TEXT DEFAULT CURRENT_TIMESTAMP, updated_at TEXT DEFAULT CURRENT_TIMESTAMP)",
            "CREATE TABLE sync_state (scope TEXT, scope_id TEXT, provider TEXT, cursor TEXT, last_sync_at TEXT, PRIMARY KEY(scope, scope_id, provider))",
            "CREATE TABLE sync_logs (id INTEGER PRIMARY KEY AUTOINCREMENT, profile_id TEXT, run_id TEXT, level TEXT, message TEXT, created_at TEXT DEFAULT CURRENT_TIMESTAMP)",
            "CREATE TABLE sync_run_reports (profile_id TEXT PRIMARY KEY, run_id TEXT, report_json TEXT, status TEXT, started_at TEXT, completed_at TEXT, updated_at TEXT DEFAULT CURRENT_TIMESTAMP)",
        ] { sqlx::query(sql).execute(&pool).await?; }
        SyncRepository::new(pool.clone())
            .upsert_profile(test_profile())
            .await?;
        Ok(pool)
    }

    #[tokio::test]
    async fn plugin_only_edit_requires_snapshot_upload() -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        let engine = SyncEngine::new(repository.clone(), Arc::new(SecretStore::new(pool.clone())));
        let provider = MemoryProvider::default();
        engine
            .upload_snapshot(&provider, &test_profile(), "local-device")
            .await?;
        assert!(
            engine
                .remote_order_matches_local(&provider, &test_profile())
                .await?
        );
        repository
            .save_local_plugin_data("todo", "state", &serde_json::json!({"tasks": []}))
            .await?;
        assert!(repository.has_unsynced_changes(&test_profile()).await?);
        assert!(
            !engine
                .remote_order_matches_local(&provider, &test_profile())
                .await?
        );
        engine
            .upload_snapshot(&provider, &test_profile(), "local-device")
            .await?;
        assert!(
            engine
                .remote_order_matches_local(&provider, &test_profile())
                .await?
        );
        Ok(())
    }

    #[tokio::test]
    async fn plugin_pull_preserves_subsecond_order_and_pending_local_edits() -> Result<(), SyncError>
    {
        let pool = setup_order_test_pool().await?;
        let repository = SyncRepository::new(pool.clone());
        let record = |value: &str, timestamp: &str| RemotePluginData {
            plugin_id: "todo".into(),
            storage_key: "state".into(),
            value: serde_json::json!(value),
            updated_at: timestamp.into(),
        };
        repository
            .apply_snapshot_plugin_data(vec![record("new", "2026-09-19T12:00:00.900Z")])
            .await?;
        repository
            .apply_snapshot_plugin_data(vec![record("old", "2026-09-19T12:00:00.100Z")])
            .await?;
        assert_eq!(
            repository.list_snapshot_plugin_data().await?[0].value,
            "new"
        );
        sqlx::query("INSERT INTO sync_changes (entity_type, entity_id, operation, plugin_id) VALUES ('plugin_data', 'state', 'upsert', 'todo')")
            .execute(&pool).await?;
        assert!(repository.has_unsynced_changes(&test_profile()).await?);
        repository
            .apply_snapshot_plugin_data(vec![record("remote", "2026-09-19T12:00:01.000Z")])
            .await?;
        assert_eq!(
            repository.list_snapshot_plugin_data().await?[0].value,
            "new"
        );
        Ok(())
    }

    #[tokio::test]
    async fn repaired_snapshot_upload_is_required_when_remote_order_omits_custom_tabs(
    ) -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        let engine = SyncEngine::new(repository, Arc::new(SecretStore::new(pool)));
        let provider = MemoryProvider::default();
        let remote_order = SnapshotOrder {
            schema_version: 1,
            updated_at: "2026-05-15T00:00:00Z".to_string(),
            tabs: vec![SnapshotTabOrder {
                tab_key: "default".to_string(),
                tab_name: None,
                pinned: Vec::new(),
                normal: Vec::new(),
            }],
        };
        provider
            .put("order/default.json", serde_json::to_vec(&remote_order)?)
            .await?;
        provider
            .put(
                "manifest.json",
                serde_json::to_vec(&SnapshotManifest {
                    schema_version: SNAPSHOT_SCHEMA_VERSION,
                    app: APP_NAME.to_string(),
                    generation: 1,
                    device_id: "remote-device".to_string(),
                    updated_at: "2026-05-15T00:00:00Z".to_string(),
                    item_shard_size: SNAPSHOT_SHARD_SIZE,
                    item_shards: Vec::new(),
                    order: Some(SnapshotFileRef {
                        path: "order/default.json".to_string(),
                        hash: String::new(),
                    }),
                    plugin_data: None,
                })?,
            )
            .await?;

        assert!(
            !engine
                .remote_order_matches_local(&provider, &test_profile())
                .await?
        );
        Ok(())
    }

    #[tokio::test]
    async fn no_change_cleanup_removes_orphaned_snapshot_blobs() -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let live_content = "x".repeat(SNAPSHOT_INLINE_CONTENT_LIMIT + 1);
        sqlx::query(
            "INSERT INTO clipboard_items (id, type, content, content_hash, tab_id) VALUES (1, 'text', ?, 'live-content', 1)",
        )
        .bind(live_content)
        .execute(&pool)
        .await?;
        sqlx::query(
            "INSERT INTO sync_item_map (local_id, item_key, stable_seq) VALUES (1, 'live-item', 1)",
        )
        .execute(&pool)
        .await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        let engine = SyncEngine::new(repository, Arc::new(SecretStore::new(pool)));
        let provider = MemoryProvider::default();
        let shard_json = serde_json::to_vec(&SnapshotItemShard {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            start: 1,
            end: 1,
            items: vec![RemoteClipboardItem {
                blob_path: Some("blobs/live-content.bin".to_string()),
                ..engine
                    .repository
                    .list_snapshot_items(&test_profile(), "local-device")
                    .await?
                    .remove(0)
            }],
        })?;
        provider.put("items/live.json", shard_json.clone()).await?;
        provider
            .put(
                "manifest.json",
                serde_json::to_vec(&SnapshotManifest {
                    schema_version: SNAPSHOT_SCHEMA_VERSION,
                    app: APP_NAME.to_string(),
                    generation: 1,
                    device_id: "remote-device".to_string(),
                    updated_at: "2026-07-24T00:00:00Z".to_string(),
                    item_shard_size: SNAPSHOT_SHARD_SIZE,
                    item_shards: vec![SnapshotShardRef {
                        path: "items/live.json".to_string(),
                        start: 1,
                        end: 1,
                        count: 1,
                        hash: sha256_hex(&shard_json),
                    }],
                    order: None,
                    plugin_data: None,
                })?,
            )
            .await?;
        provider
            .put("blobs/orphaned-content.bin", vec![1, 2, 3])
            .await?;
        provider
            .put("blobs/live-content.bin", vec![4, 5, 6])
            .await?;

        let report = engine
            .cleanup_snapshot_objects_for_local_state(&provider, &test_profile(), "local-device")
            .await?;

        assert!(report.errors.is_empty());
        assert!(provider.stat("blobs/orphaned-content.bin").await?.is_none());
        assert!(provider.stat("blobs/live-content.bin").await?.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn screenshot_blob_survives_trash_but_is_removed_after_permanent_delete(
    ) -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        sqlx::query("INSERT INTO tabs (id, name, is_trash) VALUES (3, 'Trash', 1)")
            .execute(&pool)
            .await?;
        sqlx::query("INSERT INTO clipboard_items (id, type, content, content_hash, tab_id) VALUES (1, 'image', ?, 'screenshot', 1)")
            .bind("x".repeat(SNAPSHOT_INLINE_CONTENT_LIMIT + 1)).execute(&pool).await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        let engine = SyncEngine::new(repository, Arc::new(SecretStore::new(pool.clone())));
        let provider = MemoryProvider::default();
        let blob_path = format!(
            "blobs/{}.bin",
            sha256_hex("x".repeat(SNAPSHOT_INLINE_CONTENT_LIMIT + 1).as_bytes())
        );
        engine
            .upload_snapshot(&provider, &test_profile(), "device")
            .await?;
        assert!(provider.stat(&blob_path).await?.is_some());
        sqlx::query("UPDATE clipboard_items SET tab_id = 3, deleted_at = datetime('now'), deleted_from_tab_id = 1 WHERE id = 1")
            .execute(&pool).await?;
        engine
            .upload_snapshot(&provider, &test_profile(), "device")
            .await?;
        assert!(provider.stat(&blob_path).await?.is_some());
        sqlx::query("DELETE FROM sync_item_map WHERE local_id = 1")
            .execute(&pool)
            .await?;
        sqlx::query("DELETE FROM clipboard_items WHERE id = 1")
            .execute(&pool)
            .await?;
        // Until a new manifest is published, the remote blob is still live.
        engine
            .cleanup_snapshot_objects_for_local_state(&provider, &test_profile(), "device")
            .await?;
        assert!(provider.stat(&blob_path).await?.is_some());
        engine
            .upload_snapshot(&provider, &test_profile(), "device")
            .await?;
        assert!(provider.stat(&blob_path).await?.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn failed_blob_cleanup_is_reported_and_can_be_retried() -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        let engine = SyncEngine::new(repository, Arc::new(SecretStore::new(pool)));
        let provider = MemoryProvider::default();
        provider
            .put(
                "manifest.json",
                serde_json::to_vec(&SnapshotManifest {
                    schema_version: SNAPSHOT_SCHEMA_VERSION,
                    app: APP_NAME.to_string(),
                    generation: 1,
                    device_id: "remote-device".to_string(),
                    updated_at: "2026-07-24T00:00:00Z".to_string(),
                    item_shard_size: SNAPSHOT_SHARD_SIZE,
                    item_shards: Vec::new(),
                    order: None,
                    plugin_data: None,
                })?,
            )
            .await?;
        let blob_path = "blobs/retry-content.bin";
        provider.put(blob_path, vec![1, 2, 3]).await?;
        provider
            .delete_failures
            .write()
            .await
            .insert(blob_path.to_string());

        let first = engine
            .cleanup_snapshot_objects_for_local_state(&provider, &test_profile(), "local-device")
            .await?;
        assert_eq!(first.errors.len(), 1);
        assert!(provider.stat(blob_path).await?.is_some());

        provider.delete_failures.write().await.remove(blob_path);
        let retry = engine
            .cleanup_snapshot_objects_for_local_state(&provider, &test_profile(), "local-device")
            .await?;
        assert!(retry.errors.is_empty());
        assert!(provider.stat(blob_path).await?.is_none());
        Ok(())
    }

    #[tokio::test]
    async fn same_device_snapshot_applies_missing_empty_tabs() -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        let engine = SyncEngine::new(repository, Arc::new(SecretStore::new(pool.clone())));
        let provider = MemoryProvider::default();
        let remote_order = SnapshotOrder {
            schema_version: 1,
            updated_at: "2026-05-15T00:00:00Z".to_string(),
            tabs: vec![
                SnapshotTabOrder {
                    tab_key: "default".to_string(),
                    tab_name: None,
                    pinned: Vec::new(),
                    normal: Vec::new(),
                },
                SnapshotTabOrder {
                    tab_key: "tab-name:research".to_string(),
                    tab_name: Some("Research".to_string()),
                    pinned: Vec::new(),
                    normal: Vec::new(),
                },
            ],
        };
        let order_json = serde_json::to_vec(&remote_order)?;
        let remote_item = RemoteClipboardItem {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            item_key: "remote_research_item".to_string(),
            stable_seq: 1,
            device_id: "device_a".to_string(),
            local_id: None,
            item_type: "text".to_string(),
            content: Some("research note".to_string()),
            blob_path: None,
            blob_mime: None,
            content_hash: Some("research-hash".to_string()),
            created_at: "2026-05-15T00:00:00Z".to_string(),
            updated_at: "2026-05-15T00:00:00Z".to_string(),
            tab_key: Some("tab-name:research".to_string()),
            tab_name: Some("Research".to_string()),
            tags: Vec::new(),
            is_pinned: false,
            is_sensitive: false,
            metadata: serde_json::Value::Object(serde_json::Map::new()),
            revision: 0,
            last_modified_by: "device_a".to_string(),
            deleted: false,
            is_trashed: false,
            deleted_at: None,
            deleted_from_tab_key: None,
            deleted_from_tab_name: None,
        };
        let shard = SnapshotItemShard {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            start: 1,
            end: 100,
            items: vec![remote_item],
        };
        let shard_json = serde_json::to_vec(&shard)?;
        provider
            .put("order/default.json", order_json.clone())
            .await?;
        provider
            .put("items/00000001-00000100.json", shard_json.clone())
            .await?;
        provider
            .put(
                "manifest.json",
                serde_json::to_vec(&SnapshotManifest {
                    schema_version: SNAPSHOT_SCHEMA_VERSION,
                    app: APP_NAME.to_string(),
                    generation: 1,
                    device_id: "device_a".to_string(),
                    updated_at: "2026-05-15T00:00:00Z".to_string(),
                    item_shard_size: SNAPSHOT_SHARD_SIZE,
                    item_shards: vec![SnapshotShardRef {
                        path: "items/00000001-00000100.json".to_string(),
                        start: 1,
                        end: 100,
                        count: 1,
                        hash: sha256_hex(&shard_json),
                    }],
                    order: Some(SnapshotFileRef {
                        path: "order/default.json".to_string(),
                        hash: sha256_hex(&order_json),
                    }),
                    plugin_data: None,
                })?,
            )
            .await?;

        let prepared = PreparedSyncRun {
            profile: test_profile(),
            device_id: "device_a".to_string(),
        };
        engine
            .pull_remote_changes("profile", &provider, &prepared)
            .await?;

        let exists: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tabs WHERE name = 'Research'")
            .fetch_one(&pool)
            .await?;
        assert_eq!(exists.0, 1);
        let stored: (String,) = sqlx::query_as(
            r#"
            SELECT ci.content
            FROM clipboard_items ci
            JOIN tabs t ON t.id = ci.tab_id
            WHERE t.name = 'Research'
            "#,
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(stored.0, "research note");
        Ok(())
    }

    #[tokio::test]
    async fn remote_snapshot_missing_a_local_item_does_not_delete_it() -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        sqlx::query("INSERT INTO clipboard_items (id, type, content, tab_id) VALUES (1, 'text', 'newer local note', 1)")
            .execute(&pool).await?;
        sqlx::query("INSERT INTO sync_item_map (local_id, item_key, stable_seq) VALUES (1, 'local-note', 1)")
            .execute(&pool).await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        let engine = SyncEngine::new(repository, Arc::new(SecretStore::new(pool.clone())));
        let provider = MemoryProvider::default();
        let order = SnapshotOrder {
            schema_version: 1,
            updated_at: "2026-09-24T00:00:00Z".to_string(),
            tabs: Vec::new(),
        };
        let order_json = serde_json::to_vec(&order)?;
        provider
            .put("order/default.json", order_json.clone())
            .await?;
        provider
            .put(
                "manifest.json",
                serde_json::to_vec(&SnapshotManifest {
                    schema_version: SNAPSHOT_SCHEMA_VERSION,
                    app: APP_NAME.to_string(),
                    generation: 1,
                    device_id: "other-device".to_string(),
                    updated_at: "2026-09-24T00:00:00Z".to_string(),
                    item_shard_size: SNAPSHOT_SHARD_SIZE,
                    item_shards: Vec::new(),
                    order: Some(SnapshotFileRef {
                        path: "order/default.json".to_string(),
                        hash: sha256_hex(&order_json),
                    }),
                    plugin_data: None,
                })?,
            )
            .await?;
        let prepared = PreparedSyncRun {
            profile: test_profile(),
            device_id: "local-device".to_string(),
        };

        engine
            .pull_remote_changes("profile", &provider, &prepared)
            .await?;

        let content: (String,) = sqlx::query_as("SELECT content FROM clipboard_items WHERE id = 1")
            .fetch_one(&pool)
            .await?;
        assert_eq!(content.0, "newer local note");
        Ok(())
    }

    #[tokio::test]
    async fn mismatched_snapshot_shard_is_recovered_without_failing_pull() -> Result<(), SyncError>
    {
        let pool = setup_order_test_pool().await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        let engine = SyncEngine::new(repository, Arc::new(SecretStore::new(pool.clone())));
        let provider = MemoryProvider::default();
        let shard = SnapshotItemShard {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            start: 1,
            end: 100,
            items: Vec::new(),
        };
        provider
            .put("items/00000001-00000100.json", serde_json::to_vec(&shard)?)
            .await?;
        provider
            .put(
                "manifest.json",
                serde_json::to_vec(&SnapshotManifest {
                    schema_version: SNAPSHOT_SCHEMA_VERSION,
                    app: APP_NAME.to_string(),
                    generation: 1,
                    device_id: "other-device".to_string(),
                    updated_at: "2026-09-24T00:00:00Z".to_string(),
                    item_shard_size: SNAPSHOT_SHARD_SIZE,
                    item_shards: vec![SnapshotShardRef {
                        path: "items/00000001-00000100.json".to_string(),
                        start: 1,
                        end: 100,
                        count: 0,
                        hash: "wrong-hash".to_string(),
                    }],
                    order: None,
                    plugin_data: None,
                })?,
            )
            .await?;
        let prepared = PreparedSyncRun {
            profile: test_profile(),
            device_id: "local-device".to_string(),
        };

        let result = engine
            .pull_remote_changes("profile", &provider, &prepared)
            .await?;
        assert_eq!(result.warnings.len(), 1);
        assert!(result.errors.is_empty());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM clipboard_items")
                .fetch_one(&pool)
                .await?,
            0
        );
        Ok(())
    }

    fn remote_note(key: &str, content: &str) -> RemoteClipboardItem {
        RemoteClipboardItem {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            item_key: key.into(),
            stable_seq: 1,
            device_id: "other-device".into(),
            local_id: None,
            item_type: "text".into(),
            content: Some(content.into()),
            blob_path: None,
            blob_mime: None,
            content_hash: Some(sha256_hex(content.as_bytes())),
            created_at: "2026-09-20T00:00:00Z".into(),
            updated_at: "2026-09-21T00:00:00Z".into(),
            tab_key: Some("default".into()),
            tab_name: None,
            tags: Vec::new(),
            is_pinned: false,
            is_sensitive: false,
            metadata: serde_json::json!({}),
            revision: 0,
            last_modified_by: "other-device".into(),
            deleted: false,
            is_trashed: false,
            deleted_at: None,
            deleted_from_tab_key: None,
            deleted_from_tab_name: None,
        }
    }

    async fn publish_test_shard(
        provider: &MemoryProvider,
        items: Vec<RemoteClipboardItem>,
        wrong_hash: bool,
    ) -> Result<(), SyncError> {
        let bytes = serde_json::to_vec(&SnapshotItemShard {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            start: 1,
            end: 100,
            items,
        })?;
        provider.put("items/legacy.json", bytes.clone()).await?;
        write_manifest(
            provider,
            "",
            &SnapshotManifest {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                app: APP_NAME.into(),
                generation: 1,
                device_id: "other-device".into(),
                updated_at: "2026-09-21T00:00:00Z".into(),
                item_shard_size: 100,
                item_shards: vec![SnapshotShardRef {
                    path: "items/legacy.json".into(),
                    start: 1,
                    end: 100,
                    count: 1,
                    hash: if wrong_hash {
                        "outdated".into()
                    } else {
                        sha256_hex(&bytes)
                    },
                }],
                order: None,
                plugin_data: None,
            },
        )
        .await
    }

    async fn published_notes(
        provider: &MemoryProvider,
    ) -> Result<Vec<RemoteClipboardItem>, SyncError> {
        let manifest = read_manifest(provider, "")
            .await?
            .ok_or_else(|| SyncError::validation("Missing test snapshot reference"))?;
        let mut notes = Vec::new();
        for reference in manifest.item_shards {
            let bytes = provider.get(&reference.path).await?;
            assert_eq!(sha256_hex(&bytes), reference.hash);
            let shard: SnapshotItemShard = serde_json::from_slice(&bytes)?;
            notes.extend(shard.items);
        }
        Ok(notes)
    }

    #[tokio::test]
    async fn hash_mismatch_recovers_items_and_publishes_a_consistent_snapshot(
    ) -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        let engine = SyncEngine::new(repository, Arc::new(SecretStore::new(pool.clone())));
        let provider = Arc::new(MemoryProvider::default());
        publish_test_shard(
            &provider,
            vec![remote_note("remote-note", "today's remote note")],
            true,
        )
        .await?;
        sqlx::query(
            "INSERT INTO clipboard_items (type, content, tab_id) VALUES ('text', 'today local', 1)",
        )
        .execute(&pool)
        .await?;
        let report = engine.run_now("profile", provider.clone()).await?;
        assert_eq!(report.status, SyncRunStatus::Completed);
        assert_eq!(report.items_downloaded, 1);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.errors.is_empty());
        let notes = published_notes(&provider).await?;
        assert!(notes
            .iter()
            .any(|n| n.content.as_deref() == Some("today local")));
        assert!(notes.iter().any(|n| n.item_key == "remote-note"));
        let persisted = engine
            .repository
            .get_last_run_report("profile")
            .await?
            .ok_or_else(|| SyncError::validation("Missing test snapshot reference"))?;
        assert_eq!(persisted.warnings, report.warnings);
        let retry = engine.run_now("profile", provider).await?;
        assert!(retry.warnings.is_empty());
        assert!(retry.errors.is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn damaged_shard_does_not_block_healthy_downloads_or_local_uploads(
    ) -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        let engine = SyncEngine::new(repository, Arc::new(SecretStore::new(pool.clone())));
        let provider = Arc::new(MemoryProvider::default());
        publish_test_shard(
            &provider,
            vec![remote_note("healthy", "healthy remote")],
            false,
        )
        .await?;
        provider
            .put("items/broken.json", b"truncated {".to_vec())
            .await?;
        provider.put("blobs/unknown.bin", vec![1, 2, 3]).await?;
        let mut manifest = read_manifest(provider.as_ref(), "")
            .await?
            .ok_or_else(|| SyncError::validation("Missing test snapshot reference"))?;
        manifest.item_shards.insert(
            0,
            SnapshotShardRef {
                path: "items/broken.json".into(),
                start: 101,
                end: 200,
                count: 2,
                hash: "missing".into(),
            },
        );
        write_manifest(provider.as_ref(), "", &manifest).await?;
        sqlx::query(
            "INSERT INTO clipboard_items (type, content, tab_id) VALUES ('text', 'local new', 1)",
        )
        .execute(&pool)
        .await?;
        let report = engine.run_now("profile", provider.clone()).await?;
        assert_eq!(report.status, SyncRunStatus::PartialSuccess);
        assert_eq!(report.items_downloaded, 1);
        assert!(report.items_uploaded > 0);
        assert_eq!(report.errors.len(), 1);
        let manifest = read_manifest(provider.as_ref(), "")
            .await?
            .ok_or_else(|| SyncError::validation("Missing test snapshot reference"))?;
        assert!(manifest
            .item_shards
            .iter()
            .any(|r| r.path == "items/broken.json"));
        assert!(provider.stat("blobs/unknown.bin").await?.is_some());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM clipboard_items")
                .fetch_one(&pool)
                .await?,
            2
        );
        provider
            .put(
                "items/broken.json",
                serde_json::to_vec(&SnapshotItemShard {
                    schema_version: SNAPSHOT_SCHEMA_VERSION,
                    start: 101,
                    end: 200,
                    items: vec![RemoteClipboardItem {
                        stable_seq: 101,
                        ..remote_note("recovered", "late remote")
                    }],
                })?,
            )
            .await?;
        let retry = engine.run_now("profile", provider.clone()).await?;
        assert!(retry.errors.is_empty());
        assert!(published_notes(&provider)
            .await?
            .iter()
            .any(|n| n.item_key == "recovered"));
        Ok(())
    }

    #[tokio::test]
    async fn individual_invalid_items_and_missing_blobs_are_retried_without_blocking_the_shard(
    ) -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let engine = SyncEngine::new(
            Arc::new(SyncRepository::new(pool.clone())),
            Arc::new(SecretStore::new(pool.clone())),
        );
        let provider = Arc::new(MemoryProvider::default());
        let mut invalid = remote_note("bad-date", "invalid date");
        invalid.created_at = "invalid".into();
        let mut missing = remote_note("missing-blob", "");
        missing.content = None;
        missing.blob_path = Some("blobs/missing.bin".into());
        publish_test_shard(
            &provider,
            vec![invalid, missing, remote_note("good", "valid item")],
            false,
        )
        .await?;
        let report = engine.run_now("profile", provider.clone()).await?;
        assert_eq!(report.items_downloaded, 1);
        assert_eq!(report.errors.len(), 2);
        assert!(report.items_uploaded > 0);
        assert!(read_manifest(provider.as_ref(), "")
            .await?
            .ok_or_else(|| SyncError::validation("Missing test snapshot reference"))?
            .item_shards
            .iter()
            .any(|r| r.path == "items/legacy.json"));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM clipboard_items")
                .fetch_one(&pool)
                .await?,
            1
        );
        Ok(())
    }

    #[tokio::test]
    async fn pending_conflicts_survive_retries_and_all_user_choices_publish(
    ) -> Result<(), SyncError> {
        for resolution in [
            ConflictResolutionInput::UseLocal,
            ConflictResolutionInput::UseRemote,
            ConflictResolutionInput::KeepBoth,
        ] {
            let pool = setup_order_test_pool().await?;
            let repository = Arc::new(SyncRepository::new(pool.clone()));
            repository
                .apply_remote_item(&test_profile(), remote_note("shared", "base"), None)
                .await?;
            sqlx::query(
                "UPDATE clipboard_items SET content = 'local version', is_pinned = 1 WHERE id = 1",
            )
            .execute(&pool)
            .await?;
            // Old outbox entries can have no item_key; they must remain pending too.
            repository
                .record_clipboard_change(1, "update", "local", None)
                .await?;
            sqlx::query("INSERT INTO clipboard_items (type, content, tab_id) VALUES ('text', 'independent local', 1)").execute(&pool).await?;
            repository
                .record_clipboard_change(2, "create", "local", None)
                .await?;
            let engine =
                SyncEngine::new(repository.clone(), Arc::new(SecretStore::new(pool.clone())));
            let provider = Arc::new(MemoryProvider::default());
            let mut remote = remote_note("shared", "remote version");
            remote.updated_at = "2026-09-22T00:00:00Z".into();
            remote.tab_key = Some("tab-name:Remote".into());
            remote.tab_name = Some("Remote".into());
            publish_test_shard(&provider, vec![remote], true).await?;
            for _ in 0..2 {
                let report = engine.run_now("profile", provider.clone()).await?;
                assert_eq!(report.conflicts_found, 1);
                assert!(!repository.has_publishable_changes(&test_profile()).await?);
                assert!(published_notes(&provider)
                    .await?
                    .iter()
                    .any(|n| n.content.as_deref() == Some("independent local")));
                assert!(report.errors.is_empty());
                let notes = published_notes(&provider).await?;
                assert_eq!(notes[0].content.as_deref(), Some("remote version"));
                assert!(!notes[0].is_pinned);
                assert_eq!(
                    sqlx::query_scalar::<_, String>(
                        "SELECT content FROM clipboard_items WHERE id=1"
                    )
                    .fetch_one(&pool)
                    .await?,
                    "local version"
                );
                assert_eq!(sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sync_changes WHERE source='local' AND synced_at IS NULL").fetch_one(&pool).await?, 1);
            }
            let conflicts = repository.pending_conflicts().await?;
            assert_eq!(conflicts.len(), 1);
            let result = engine
                .resolve_conflict_and_run(
                    "profile",
                    Ok(provider.clone()),
                    conflicts[0].id,
                    resolution.clone(),
                )
                .await?;
            assert!(result.sync_error.is_none());
            let report = result
                .report
                .ok_or_else(|| SyncError::validation("Resolution did not immediately sync"))?;
            assert_eq!(report.conflicts_found, 0);
            let notes = published_notes(&provider).await?;
            let shared = notes
                .iter()
                .find(|n| n.item_key == "shared")
                .ok_or_else(|| SyncError::validation("Missing shared test item"))?;
            if matches!(resolution, ConflictResolutionInput::UseRemote) {
                assert_eq!(shared.tab_name.as_deref(), Some("Remote"));
            } else {
                assert_eq!(shared.tab_key.as_deref(), Some("default"));
            }
            match resolution {
                ConflictResolutionInput::UseLocal => {
                    assert_eq!(notes[0].content.as_deref(), Some("local version"))
                }
                ConflictResolutionInput::UseRemote => {
                    assert_eq!(notes[0].content.as_deref(), Some("remote version"))
                }
                ConflictResolutionInput::KeepBoth => {
                    assert_eq!(notes.len(), 3);
                    assert!(notes
                        .iter()
                        .any(|n| n.content.as_deref() == Some("local version")));
                    assert!(notes
                        .iter()
                        .any(|n| n.content.as_deref() == Some("remote version")));
                }
                _ => unreachable!(),
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn failed_upload_leaves_committed_snapshot_intact_and_retains_pull_progress(
    ) -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let engine = SyncEngine::new(
            Arc::new(SyncRepository::new(pool.clone())),
            Arc::new(SecretStore::new(pool.clone())),
        );
        let provider = Arc::new(MemoryProvider::default());
        publish_test_shard(&provider, vec![remote_note("remote", "remote base")], false).await?;
        let old_manifest = provider.get("manifest.json").await?;
        let old_shard = provider.get("items/legacy.json").await?;
        sqlx::query(
            "INSERT INTO clipboard_items (type, content, tab_id) VALUES ('text', 'new local', 1)",
        )
        .execute(&pool)
        .await?;
        provider
            .put_failures
            .write()
            .await
            .insert("manifest.json".into());
        let report = engine.run_now("profile", provider.clone()).await?;
        assert_eq!(report.status, SyncRunStatus::PartialSuccess);
        assert_eq!(report.items_downloaded, 1);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(provider.get("manifest.json").await?, old_manifest);
        assert_eq!(provider.get("items/legacy.json").await?, old_shard);
        assert!(
            engine
                .repository
                .has_unsynced_changes(&test_profile())
                .await?
        );
        provider.put_failures.write().await.clear();
        assert!(engine
            .run_now("profile", provider.clone())
            .await?
            .errors
            .is_empty());
        assert_eq!(published_notes(&provider).await?.len(), 2);
        Ok(())
    }

    #[tokio::test]
    async fn corrupted_upload_is_not_published() -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let engine = SyncEngine::new(
            Arc::new(SyncRepository::new(pool.clone())),
            Arc::new(SecretStore::new(pool.clone())),
        );
        let provider = Arc::new(MemoryProvider::default());
        publish_test_shard(
            &provider,
            vec![remote_note("remote", "old snapshot")],
            false,
        )
        .await?;
        sqlx::query("INSERT INTO clipboard_items (type, content, tab_id) VALUES ('text', 'pending local', 1)").execute(&pool).await?;
        let old_manifest = provider.get("manifest.json").await?;
        provider.corrupt_puts.write().await.insert("items/".into());
        let report = engine.run_now("profile", provider.clone()).await?;
        assert_eq!(report.items_downloaded, 1);
        assert!(!report.errors.is_empty());
        assert_eq!(provider.get("manifest.json").await?, old_manifest);
        assert!(
            engine
                .repository
                .has_unsynced_changes(&test_profile())
                .await?
        );
        Ok(())
    }

    #[tokio::test]
    async fn plugin_conflicts_keep_remote_until_user_choice_without_recreating_resolved_conflicts(
    ) -> Result<(), SyncError> {
        for resolution in [
            ConflictResolutionInput::UseLocal,
            ConflictResolutionInput::UseRemote,
        ] {
            let pool = setup_order_test_pool().await?;
            let repository = Arc::new(SyncRepository::new(pool.clone()));
            repository
                .save_local_plugin_data("todo", "state", &serde_json::json!({"tasks":["local"]}))
                .await?;
            let engine =
                SyncEngine::new(repository.clone(), Arc::new(SecretStore::new(pool.clone())));
            let provider = Arc::new(MemoryProvider::default());
            let record = RemotePluginData {
                plugin_id: "todo".into(),
                storage_key: "state".into(),
                value: serde_json::json!({"tasks":["remote"]}),
                updated_at: "2026-10-10T00:00:00Z".into(),
            };
            let bytes = serde_json::to_vec(&vec![record])?;
            provider.put("plugins/legacy.json", bytes).await?;
            let manifest = SnapshotManifest {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                app: APP_NAME.into(),
                generation: 1,
                device_id: "other-device".into(),
                updated_at: "2026-10-10T00:00:00Z".into(),
                item_shard_size: 100,
                item_shards: Vec::new(),
                order: None,
                plugin_data: Some(SnapshotFileRef {
                    path: "plugins/legacy.json".into(),
                    hash: "stale".into(),
                }),
            };
            write_manifest(provider.as_ref(), "", &manifest).await?;
            for _ in 0..2 {
                let report = engine.run_now("profile", provider.clone()).await?;
                assert_eq!(report.conflicts_found, 1);
                assert!(repository.has_unsynced_changes(&test_profile()).await?);
                let manifest = read_manifest(provider.as_ref(), "")
                    .await?
                    .ok_or_else(|| SyncError::validation("Missing test snapshot reference"))?;
                let remote: Vec<RemotePluginData> = serde_json::from_slice(
                    &provider
                        .get(
                            &manifest
                                .plugin_data
                                .ok_or_else(|| {
                                    SyncError::validation("Missing test snapshot reference")
                                })?
                                .path,
                        )
                        .await?,
                )?;
                assert_eq!(remote[0].value, serde_json::json!({"tasks":["remote"]}));
            }
            let conflicts = repository.pending_conflicts().await?;
            assert_eq!(conflicts.len(), 1);
            let result = engine
                .resolve_conflict_and_run(
                    "profile",
                    Ok(provider.clone()),
                    conflicts[0].id,
                    resolution.clone(),
                )
                .await?;
            assert!(result.sync_error.is_none());
            let report = result
                .report
                .ok_or_else(|| SyncError::validation("Resolution did not immediately sync"))?;
            assert_eq!(report.conflicts_found, 0);
            let manifest = read_manifest(provider.as_ref(), "")
                .await?
                .ok_or_else(|| SyncError::validation("Missing test snapshot reference"))?;
            let remote: Vec<RemotePluginData> = serde_json::from_slice(
                &provider
                    .get(
                        &manifest
                            .plugin_data
                            .ok_or_else(|| {
                                SyncError::validation("Missing test snapshot reference")
                            })?
                            .path,
                    )
                    .await?,
            )?;
            let expected = if matches!(resolution, ConflictResolutionInput::UseLocal) {
                "local"
            } else {
                "remote"
            };
            assert_eq!(remote[0].value, serde_json::json!({"tasks":[expected]}));
        }
        Ok(())
    }

    #[tokio::test]
    async fn automatic_defaults_resolve_clipboard_and_plugins_and_publish_in_the_same_run(
    ) -> Result<(), SyncError> {
        for policy in [ConflictPolicy::PreferLocal, ConflictPolicy::PreferRemote] {
            let pool = setup_order_test_pool().await?;
            let repository = Arc::new(SyncRepository::new(pool.clone()));
            let mut profile = test_profile();
            profile.sync_plugins.selected_plugin_ids = vec!["todo".into()];
            repository.upsert_profile(profile.clone()).await?;
            repository
                .apply_remote_item(&profile, remote_note("shared", "base"), None)
                .await?;
            sqlx::query("UPDATE clipboard_items SET content='local version' WHERE id=1")
                .execute(&pool)
                .await?;
            repository
                .record_clipboard_change(1, "update", "local", None)
                .await?;
            repository
                .save_local_plugin_data("todo", "state", &serde_json::json!({"tasks":["local"]}))
                .await?;
            let engine =
                SyncEngine::new(repository.clone(), Arc::new(SecretStore::new(pool.clone())));
            let provider = Arc::new(MemoryProvider::default());
            let mut remote = remote_note("shared", "remote version");
            remote.updated_at = "2030-01-01T00:00:00Z".into();
            let peer_pool = setup_order_test_pool().await?;
            let peer = SyncRepository::new(peer_pool);
            peer.apply_remote_item(&profile, remote.clone(), None)
                .await?;
            publish_test_shard(&provider, vec![remote], false).await?;
            let records = vec![RemotePluginData {
                plugin_id: "todo".into(),
                storage_key: "state".into(),
                value: serde_json::json!({"tasks":["remote"]}),
                updated_at: "2030-01-01T00:00:00Z".into(),
            }];
            peer.apply_snapshot_plugin_data(records.clone()).await?;
            let bytes = serde_json::to_vec(&records)?;
            let hash = format!("{:x}", Sha256::digest(&bytes));
            provider.put("plugins/legacy.json", bytes).await?;
            let mut manifest = read_manifest(provider.as_ref(), "")
                .await?
                .ok_or_else(|| SyncError::validation("Missing manifest"))?;
            manifest.plugin_data = Some(SnapshotFileRef {
                path: "plugins/legacy.json".into(),
                hash,
            });
            write_manifest(provider.as_ref(), "", &manifest).await?;
            assert_eq!(
                engine
                    .run_now("profile", provider.clone())
                    .await?
                    .conflicts_found,
                2
            );
            assert!(!repository.has_publishable_changes(&profile).await?);
            profile.conflict_policy = policy;
            repository.upsert_profile(profile.clone()).await?;
            assert_eq!(
                repository.get_profile("profile").await?.conflict_policy,
                policy
            );
            assert!(repository.has_publishable_changes(&profile).await?);
            for _ in 0..2 {
                let report = engine.run_now("profile", provider.clone()).await?;
                assert_eq!(report.status, SyncRunStatus::Completed);
                assert_eq!(report.conflicts_found, 0);
                let expected = if policy == ConflictPolicy::PreferLocal {
                    "local"
                } else {
                    "remote"
                };
                assert_eq!(
                    published_notes(&provider).await?[0].content.as_deref(),
                    Some(format!("{} version", expected).as_str())
                );
                let chosen = published_notes(&provider).await?[0].clone();
                assert!(
                    chrono::DateTime::parse_from_rfc3339(&chosen.updated_at)
                        .map_err(|e| SyncError::validation(e.to_string()))?
                        > chrono::DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
                            .map_err(|e| SyncError::validation(e.to_string()))?
                );
                peer.apply_remote_item(&profile, chosen, None).await?;
                assert_eq!(
                    peer.list_snapshot_items(&profile, "peer").await?[0]
                        .content
                        .as_deref(),
                    Some(format!("{} version", expected).as_str())
                );
                let manifest = read_manifest(provider.as_ref(), "")
                    .await?
                    .ok_or_else(|| SyncError::validation("Missing manifest"))?;
                let reference = manifest
                    .plugin_data
                    .ok_or_else(|| SyncError::validation("Missing plugins"))?;
                let records: Vec<RemotePluginData> =
                    serde_json::from_slice(&provider.get(&reference.path).await?)?;
                assert_eq!(records[0].value, serde_json::json!({"tasks":[expected]}));
                peer.apply_snapshot_plugin_data(records).await?;
                assert_eq!(
                    peer.list_snapshot_plugin_data().await?[0].value,
                    serde_json::json!({"tasks":[expected]})
                );
                assert!(!repository.has_unsynced_changes(&profile).await?);
            }
            let resolved: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sync_conflicts WHERE status='resolved' AND resolution IS NOT NULL").fetch_one(&pool).await?;
            assert_eq!(resolved, 2);
        }
        Ok(())
    }

    #[tokio::test]
    async fn automatic_policy_preserves_unreadable_and_excluded_conflicts() -> Result<(), SyncError>
    {
        let pool = setup_order_test_pool().await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        let mut profile = test_profile();
        profile.conflict_policy = ConflictPolicy::PreferRemote;
        profile.sync_tabs = TabSyncSelection {
            mode: TabSyncMode::Selected,
            selected_tab_ids: vec![1],
        };
        repository.upsert_profile(profile.clone()).await?;
        let mut remote_records = Vec::new();
        for (id, key) in [(1, "excluded"), (2, "unreadable")] {
            repository
                .apply_remote_item(&profile, remote_note(key, &format!("base {}", id)), None)
                .await?;
            sqlx::query("UPDATE clipboard_items SET content=? WHERE id=?")
                .bind(format!("local {}", id))
                .bind(id)
                .execute(&pool)
                .await?;
            repository
                .record_clipboard_change(id, "update", "local", None)
                .await?;
            let mut remote = remote_note(key, &format!("remote {}", id));
            remote.updated_at = "2030-01-01T00:00:00Z".into();
            repository
                .apply_remote_item(&profile, remote.clone(), None)
                .await?;
            remote_records.push(remote);
        }
        repository
            .save_local_plugin_data("excluded-plugin", "state", &serde_json::json!("local"))
            .await?;
        let plugin = RemotePluginData {
            plugin_id: "excluded-plugin".into(),
            storage_key: "state".into(),
            value: serde_json::json!("remote"),
            updated_at: "2030-01-01T00:00:00Z".into(),
        };
        let provider = Arc::new(MemoryProvider::default());
        publish_test_shard(&provider, vec![remote_records[0].clone()], false).await?;
        provider
            .put("items/unreadable.json", b"broken".to_vec())
            .await?;
        let bytes = serde_json::to_vec(&vec![plugin])?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        provider.put("plugins/legacy.json", bytes).await?;
        let mut manifest = read_manifest(provider.as_ref(), "")
            .await?
            .ok_or_else(|| SyncError::validation("Missing manifest"))?;
        manifest.item_shards.push(SnapshotShardRef {
            path: "items/unreadable.json".into(),
            start: 101,
            end: 200,
            count: 1,
            hash: "missing".into(),
        });
        manifest.plugin_data = Some(SnapshotFileRef {
            path: "plugins/legacy.json".into(),
            hash,
        });
        write_manifest(provider.as_ref(), "", &manifest).await?;
        let engine = SyncEngine::new(repository.clone(), Arc::new(SecretStore::new(pool.clone())));
        let report = engine.run_now("profile", provider).await?;
        assert_eq!(report.conflicts_found, 3);
        assert_eq!(repository.pending_conflicts().await?.len(), 3);
        assert!(!report.errors.is_empty());
        let content: String = sqlx::query_scalar("SELECT content FROM clipboard_items WHERE id=1")
            .fetch_one(&pool)
            .await?;
        assert_eq!(content, "local 1");
        Ok(())
    }

    #[tokio::test]
    async fn saved_choices_retry_failed_publication_without_recreating_conflicts(
    ) -> Result<(), SyncError> {
        for fail_manifest in [true, false] {
            for resolution in [
                ConflictResolutionInput::UseLocal,
                ConflictResolutionInput::UseRemote,
            ] {
                let pool = setup_order_test_pool().await?;
                let repository = Arc::new(SyncRepository::new(pool.clone()));
                repository
                    .apply_remote_item(&test_profile(), remote_note("shared", "base"), None)
                    .await?;
                sqlx::query("UPDATE clipboard_items SET content='local version' WHERE id=1")
                    .execute(&pool)
                    .await?;
                repository
                    .record_clipboard_change(1, "update", "local", None)
                    .await?;
                let engine =
                    SyncEngine::new(repository.clone(), Arc::new(SecretStore::new(pool.clone())));
                let provider = Arc::new(MemoryProvider::default());
                let mut remote = remote_note("shared", "remote version");
                remote.updated_at = "2026-09-22T00:00:00Z".into();
                publish_test_shard(&provider, vec![remote], false).await?;
                engine.run_now("profile", provider.clone()).await?;
                let conflicts = repository.pending_conflicts().await?;
                if fail_manifest {
                    provider
                        .put_failures
                        .write()
                        .await
                        .insert("manifest.json".into());
                }
                let connection: Result<Arc<dyn SyncProvider>, SyncError> = if fail_manifest {
                    Ok(provider.clone())
                } else {
                    Err(SyncError::validation("Offline"))
                };
                let result = engine
                    .resolve_conflict_and_run(
                        "profile",
                        connection,
                        conflicts[0].id,
                        resolution.clone(),
                    )
                    .await?;
                assert!(
                    result.sync_error.is_some()
                        || result.report.as_ref().is_some_and(|r| !r.errors.is_empty())
                );
                assert!(repository.pending_conflicts().await?.is_empty());
                assert!(repository.has_unsynced_changes(&test_profile()).await?);
                assert_eq!(
                    published_notes(&provider).await?[0].content.as_deref(),
                    Some("remote version")
                );
                provider.put_failures.write().await.clear();
                for _ in 0..2 {
                    let report = engine.run_now("profile", provider.clone()).await?;
                    assert_eq!(report.conflicts_found, 0);
                    assert!(report.errors.is_empty());
                    let expected = if matches!(resolution, ConflictResolutionInput::UseLocal) {
                        "local version"
                    } else {
                        "remote version"
                    };
                    assert_eq!(
                        published_notes(&provider).await?[0].content.as_deref(),
                        Some(expected)
                    );
                }
                assert!(!repository.has_unsynced_changes(&test_profile()).await?);
            }
        }
        Ok(())
    }

    #[tokio::test]
    async fn resolution_waits_for_active_sync_before_changing_data() -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        repository
            .apply_remote_item(&test_profile(), remote_note("shared", "base"), None)
            .await?;
        sqlx::query("UPDATE clipboard_items SET content='local version' WHERE id=1")
            .execute(&pool)
            .await?;
        repository
            .record_clipboard_change(1, "update", "local", None)
            .await?;
        let engine = Arc::new(SyncEngine::new(
            repository.clone(),
            Arc::new(SecretStore::new(pool.clone())),
        ));
        let provider = Arc::new(MemoryProvider::default());
        let mut remote = remote_note("shared", "remote version");
        remote.updated_at = "2026-09-22T00:00:00Z".into();
        publish_test_shard(&provider, vec![remote], false).await?;
        engine.run_now("profile", provider.clone()).await?;
        let conflict_id = repository.pending_conflicts().await?[0].id;
        let gate = engine.operation_gate("profile").await;
        let active_run = gate.lock_owned().await;
        let resolver_engine = engine.clone();
        let resolver_provider = provider.clone();
        let resolver = tokio::spawn(async move {
            resolver_engine
                .resolve_conflict_and_run(
                    "profile",
                    Ok(resolver_provider),
                    conflict_id,
                    ConflictResolutionInput::UseLocal,
                )
                .await
        });
        tokio::task::yield_now().await;
        assert!(!resolver.is_finished());
        assert_eq!(repository.pending_conflicts().await?.len(), 1);
        assert!(matches!(
            engine.run_now("profile", provider.clone()).await,
            Err(SyncError::Cancelled(_))
        ));
        drop(active_run);
        let result = resolver
            .await
            .map_err(|e| SyncError::validation(e.to_string()))??;
        assert!(result.sync_error.is_none());
        assert!(repository.pending_conflicts().await?.is_empty());
        assert_eq!(
            published_notes(&provider).await?[0].content.as_deref(),
            Some("local version")
        );
        Ok(())
    }

    #[test]
    fn older_profiles_default_to_manual_resolution() -> Result<(), SyncError> {
        let mut value = serde_json::to_value(test_profile())?;
        value
            .as_object_mut()
            .ok_or_else(|| SyncError::validation("Expected profile object"))?
            .remove("conflict_policy");
        let profile: SyncProfile = serde_json::from_value(value)?;
        assert_eq!(profile.conflict_policy, ConflictPolicy::Manual);
        Ok(())
    }

    #[tokio::test]
    async fn normal_local_plugin_edits_do_not_conflict_with_the_last_published_version(
    ) -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        let engine = SyncEngine::new(repository.clone(), Arc::new(SecretStore::new(pool)));
        let provider = Arc::new(MemoryProvider::default());
        for value in ["first", "second", "third"] {
            repository
                .save_local_plugin_data("todo", "state", &serde_json::json!({"task":value}))
                .await?;
            let report = engine.run_now("profile", provider.clone()).await?;
            assert!(report.errors.is_empty());
            assert_eq!(report.conflicts_found, 0);
            assert!(!repository.has_unsynced_changes(&test_profile()).await?);
            let manifest = read_manifest(provider.as_ref(), "")
                .await?
                .ok_or_else(|| SyncError::validation("Missing test manifest"))?;
            let reference = manifest
                .plugin_data
                .ok_or_else(|| SyncError::validation("Missing test plugin reference"))?;
            let records: Vec<RemotePluginData> =
                serde_json::from_slice(&provider.get(&reference.path).await?)?;
            assert_eq!(records[0].value, serde_json::json!({"task":value}));
        }
        Ok(())
    }

    #[tokio::test]
    async fn metadata_divergence_requires_a_choice_even_when_content_matches(
    ) -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        repository
            .apply_remote_item(
                &test_profile(),
                remote_note("shared", "unchanged text"),
                None,
            )
            .await?;
        sqlx::query("UPDATE clipboard_items SET is_pinned = 1 WHERE id = 1")
            .execute(&pool)
            .await?;
        repository
            .record_clipboard_change(1, "update", "local", None)
            .await?;
        let engine = SyncEngine::new(repository.clone(), Arc::new(SecretStore::new(pool.clone())));
        let provider = Arc::new(MemoryProvider::default());
        let mut remote = remote_note("shared", "unchanged text");
        remote.updated_at = "2026-09-22T00:00:00Z".into();
        remote.tags = vec!["remote tag".into()];
        publish_test_shard(&provider, vec![remote], false).await?;
        assert_eq!(
            engine
                .run_now("profile", provider.clone())
                .await?
                .conflicts_found,
            1
        );
        assert!(
            sqlx::query_scalar::<_, bool>("SELECT is_pinned FROM clipboard_items WHERE id = 1")
                .fetch_one(&pool)
                .await?
        );
        let notes = published_notes(&provider).await?;
        assert_eq!(notes[0].tags, vec!["remote tag"]);
        assert!(!notes[0].is_pinned);
        let manifest = read_manifest(provider.as_ref(), "")
            .await?
            .ok_or_else(|| SyncError::validation("Missing test manifest"))?;
        let reference = manifest
            .order
            .ok_or_else(|| SyncError::validation("Missing test order"))?;
        let order: SnapshotOrder = serde_json::from_slice(&provider.get(&reference.path).await?)?;
        assert!(order
            .tabs
            .iter()
            .all(|tab| !tab.pinned.contains(&"shared".to_string())));
        Ok(())
    }

    #[tokio::test]
    async fn tampered_encrypted_shard_is_retained_while_valid_encrypted_shard_syncs(
    ) -> Result<(), SyncError> {
        let pool = setup_order_test_pool().await?;
        let repository = Arc::new(SyncRepository::new(pool.clone()));
        let engine = SyncEngine::new(repository.clone(), Arc::new(SecretStore::new(pool.clone())));
        let provider = Arc::new(MemoryProvider::default());
        let mut profile = test_profile();
        profile.encryption.enabled = true;
        repository.upsert_profile(profile.clone()).await?;
        let key = secrecy::SecretVec::new(vec![7; 32]);
        engine.unlocked_keys.write().await.insert(
            profile.id.clone(),
            UnlockedSyncKey {
                profile_id: profile.id.clone(),
                key: secrecy::SecretVec::new(key.expose_secret().clone()),
                unlocked_at: chrono::Utc::now(),
            },
        );
        let plain = serde_json::to_vec(&SnapshotItemShard {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            start: 1,
            end: 100,
            items: vec![remote_note("encrypted-good", "valid encrypted content")],
        })?;
        provider
            .put(
                "items/good.json",
                encode_snapshot_file(&plain, &profile, Some(&key))?,
            )
            .await?;
        let mut bad: SnapshotEncryptionEnvelope =
            serde_json::from_slice(&encode_snapshot_file(&plain, &profile, Some(&key))?)?;
        let mut encrypted = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &bad.encrypted_data,
        )
        .map_err(|e| SyncError::validation(e.to_string()))?;
        encrypted[30] ^= 1;
        bad.encrypted_data =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, encrypted);
        provider
            .put("items/bad.json", serde_json::to_vec(&bad)?)
            .await?;
        write_manifest(
            provider.as_ref(),
            "",
            &SnapshotManifest {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                app: APP_NAME.into(),
                generation: 1,
                device_id: "other-device".into(),
                updated_at: "2026-10-10T00:00:00Z".into(),
                item_shard_size: 100,
                item_shards: vec![
                    SnapshotShardRef {
                        path: "items/bad.json".into(),
                        start: 101,
                        end: 200,
                        count: 1,
                        hash: "wrong".into(),
                    },
                    SnapshotShardRef {
                        path: "items/good.json".into(),
                        start: 1,
                        end: 100,
                        count: 1,
                        hash: sha256_hex(&plain),
                    },
                ],
                order: None,
                plugin_data: None,
            },
        )
        .await?;
        let report = engine.run_now("profile", provider.clone()).await?;
        assert_eq!(report.status, SyncRunStatus::PartialSuccess);
        assert_eq!(report.items_downloaded, 1);
        assert!(report.errors.iter().any(|e| e.contains("Encryption error")));
        assert!(report.warnings.is_empty());
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM clipboard_items")
                .fetch_one(&pool)
                .await?,
            1
        );
        assert!(provider.stat("items/bad.json").await?.is_some());
        Ok(())
    }

    #[tokio::test]
    async fn lock_renewal_rejects_a_different_owner() -> Result<(), SyncError> {
        let provider = MemoryProvider::default();
        provider
            .put(
                "sync.lock",
                serde_json::to_vec(&RemoteLock {
                    schema_version: 1,
                    owner_device_id: "other-device".to_string(),
                    owner_run_id: "other-run".to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    expires_at: (chrono::Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
                })?,
            )
            .await?;
        assert!(matches!(
            renew_remote_lock(&provider, "sync.lock", "our-run").await,
            Err(SyncError::Lock(_))
        ));
        assert_eq!(
            crate::sync::lock::read_remote_lock(&provider, "sync.lock")
                .await?
                .map(|lock| lock.owner_run_id)
                .as_deref(),
            Some("other-run")
        );
        Ok(())
    }

    #[tokio::test]
    async fn lock_renewal_extends_the_current_run() -> Result<(), SyncError> {
        let provider = MemoryProvider::default();
        let old_expiry = chrono::Utc::now() + chrono::Duration::minutes(1);
        provider
            .put(
                "sync.lock",
                serde_json::to_vec(&RemoteLock {
                    schema_version: 1,
                    owner_device_id: "local-device".to_string(),
                    owner_run_id: "our-run".to_string(),
                    created_at: chrono::Utc::now().to_rfc3339(),
                    expires_at: old_expiry.to_rfc3339(),
                })?,
            )
            .await?;

        renew_remote_lock(&provider, "sync.lock", "our-run").await?;

        let renewed = crate::sync::lock::read_remote_lock(&provider, "sync.lock")
            .await?
            .ok_or_else(|| SyncError::validation("Renewed lock is missing"))?;
        let renewed_expiry = chrono::DateTime::parse_from_rfc3339(&renewed.expires_at)
            .map_err(|error| SyncError::validation(error.to_string()))?;
        assert!(renewed_expiry > old_expiry);
        assert_eq!(renewed.owner_run_id, "our-run");
        Ok(())
    }
}
