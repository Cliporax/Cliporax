/// Sync Engine - orchestrates the sync process
use crate::sync::crypto::{decrypt, derive_key, encrypt};
use crate::sync::error::SyncError;
use crate::sync::lock::{acquire_remote_lock, release_remote_lock};
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
                .upload_local_changes(provider.as_ref(), &prepared)
                .await?;
            report.items_uploaded = upload_report.items_uploaded;
            report.errors.extend(upload_report.errors);

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

    async fn pull_remote_changes(
        &self,
        profile_id: &str,
        provider: &dyn SyncProvider,
        prepared: &PreparedSyncRun,
    ) -> Result<SyncPhaseReport, SyncError> {
        log::info!("[Sync::Engine] Step 5: Pulling remote snapshot");
        let mut report = SyncPhaseReport::default();

        let Some(manifest) = read_manifest(provider, "").await? else {
            log::info!("[Sync::Engine] Remote snapshot is empty");
            return Ok(report);
        };

        if manifest.device_id == prepared.device_id {
            return Ok(report);
        }

        let crypto_key = self.get_crypto_key(profile_id).await.ok().flatten();
        let mut remote_items = Vec::new();
        for shard_ref in &manifest.item_shards {
            let shard_bytes = provider.get(&shard_ref.path).await.map_err(|e| {
                SyncError::provider(format!(
                    "Failed to download shard {}: {}",
                    shard_ref.path, e
                ))
            })?;
            let decoded =
                decode_snapshot_file(&shard_bytes, &prepared.profile, crypto_key.as_ref())?;
            let actual_hash = sha256_hex(&decoded);
            if actual_hash != shard_ref.hash {
                log::warn!(
                    "[Sync::Engine] Shard hash mismatch for {}; expected {}, got {}. Attempting to parse shard payload.",
                    shard_ref.path,
                    shard_ref.hash,
                    actual_hash
                );
            }
            let mut shard: SnapshotItemShard = serde_json::from_slice(&decoded)?;
            for item in &mut shard.items {
                if item.content.is_none() {
                    if let Some(blob_path) = &item.blob_path {
                        let blob_bytes = provider.get(blob_path).await.map_err(|e| {
                            SyncError::provider(format!(
                                "Failed to download blob {}: {}",
                                blob_path, e
                            ))
                        })?;
                        let decoded_blob = decode_snapshot_file(
                            &blob_bytes,
                            &prepared.profile,
                            crypto_key.as_ref(),
                        )?;
                        item.content = Some(String::from_utf8(decoded_blob).map_err(|e| {
                            SyncError::validation(format!("Remote blob is not valid UTF-8: {}", e))
                        })?);
                    }
                }
            }
            remote_items.extend(shard.items);
        }

        let prune_missing = !self
            .repository
            .has_unsynced_changes(&prepared.profile)
            .await?;
        report.items_downloaded = self
            .repository
            .apply_snapshot_items(&prepared.profile, remote_items, prune_missing)
            .await?;

        if let Some(order_ref) = &manifest.order {
            let order_bytes = provider.get(&order_ref.path).await.map_err(|e| {
                SyncError::provider(format!(
                    "Failed to download order {}: {}",
                    order_ref.path, e
                ))
            })?;
            let decoded =
                decode_snapshot_file(&order_bytes, &prepared.profile, crypto_key.as_ref())?;
            let actual_hash = sha256_hex(&decoded);
            if actual_hash != order_ref.hash {
                log::warn!(
                    "[Sync::Engine] Order hash mismatch for {}; expected {}, got {}. Attempting to parse order payload.",
                    order_ref.path,
                    order_ref.hash,
                    actual_hash
                );
            }
            let order: SnapshotOrder = serde_json::from_slice(&decoded)?;
            self.repository
                .apply_snapshot_order(&prepared.profile, &order, prune_missing)
                .await?;
        }

        Ok(report)
    }

    async fn upload_local_changes(
        &self,
        provider: &dyn SyncProvider,
        prepared: &PreparedSyncRun,
    ) -> Result<SyncPhaseReport, SyncError> {
        log::info!("[Sync::Engine] Step 7: Reading local snapshot changes");
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
        log::info!("[Sync::Engine] Step 8: Uploading local snapshot");
        let upload_report = self
            .upload_snapshot(provider, &prepared.profile, &prepared.device_id)
            .await?;
        let change_ids: Vec<i64> = unsynced_changes.iter().map(|change| change.id).collect();
        self.repository.mark_changes_synced(&change_ids).await?;

        Ok(SyncPhaseReport {
            items_uploaded: upload_report.items_uploaded,
            errors: upload_report.errors,
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

    async fn upload_snapshot(
        &self,
        provider: &dyn SyncProvider,
        profile: &SyncProfile,
        device_id: &str,
    ) -> Result<SyncPhaseReport, SyncError> {
        let previous_manifest = read_manifest(provider, "").await?;
        let previous_shards = previous_manifest
            .as_ref()
            .map(|manifest| {
                manifest
                    .item_shards
                    .iter()
                    .map(|shard| (shard.path.clone(), shard.hash.clone()))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        let previous_order_hash = previous_manifest
            .as_ref()
            .and_then(|manifest| manifest.order.as_ref())
            .map(|order| order.hash.clone());

        let crypto_key = self.get_crypto_key(&profile.id).await.ok().flatten();
        let snapshot_items = self
            .repository
            .list_snapshot_items(profile, device_id)
            .await?;
        let mut buckets: BTreeMap<i64, Vec<RemoteClipboardItem>> = BTreeMap::new();
        let mut current_blob_paths: HashSet<String> = HashSet::new();
        for item in snapshot_items {
            let bucket_start = ((item.stable_seq.saturating_sub(1)) / SNAPSHOT_SHARD_SIZE as i64)
                * SNAPSHOT_SHARD_SIZE as i64
                + 1;
            buckets.entry(bucket_start).or_default().push(item);
        }
        let mut shard_refs = Vec::new();
        let mut items_uploaded = 0;

        for (bucket_start, bucket_items) in buckets {
            let bucket_end = bucket_start + SNAPSHOT_SHARD_SIZE as i64 - 1;
            let mut shard_items = Vec::with_capacity(bucket_items.len());
            for item in bucket_items {
                let mut item = item.clone();
                if let Some(content) = item.content.take() {
                    let content_bytes = content.into_bytes();
                    if content_bytes.len() > SNAPSHOT_INLINE_CONTENT_LIMIT {
                        let content_hash = item
                            .content_hash
                            .clone()
                            .unwrap_or_else(|| sha256_hex(&content_bytes));
                        let blob_path = format!("blobs/{}.bin", content_hash);
                        let blob_data =
                            encode_snapshot_file(&content_bytes, profile, crypto_key.as_ref())?;
                        if provider.stat(&blob_path).await?.is_none() {
                            provider.put(&blob_path, blob_data).await?;
                        }
                        current_blob_paths.insert(blob_path.clone());
                        item.content_hash = Some(content_hash);
                        item.blob_path = Some(blob_path);
                    } else {
                        item.content = Some(String::from_utf8(content_bytes).map_err(|e| {
                            SyncError::validation(format!(
                                "Local content is not valid UTF-8: {}",
                                e
                            ))
                        })?);
                    }
                }
                shard_items.push(item);
            }

            let shard = SnapshotItemShard {
                schema_version: SNAPSHOT_SCHEMA_VERSION,
                start: bucket_start,
                end: bucket_end,
                items: shard_items,
            };
            let shard_json = serde_json::to_vec(&shard)?;
            let shard_hash = sha256_hex(&shard_json);
            let shard_path = format!(
                "items/{:08}-{:08}.json",
                shard.start.max(0),
                shard.end.max(0)
            );
            if previous_shards.get(&shard_path) != Some(&shard_hash) {
                let tmp_path = join_remote_path(".tmp", &format!("{}.tmp", shard_path));
                let data = encode_snapshot_file(&shard_json, profile, crypto_key.as_ref())?;
                provider.put(&tmp_path, data.clone()).await?;
                provider.put(&shard_path, data).await?;
                if let Err(e) = provider.delete(&tmp_path).await {
                    log::debug!(
                        "[Sync::Engine] Failed to cleanup snapshot temp file {}: {}",
                        tmp_path,
                        e
                    );
                }
                items_uploaded += shard.items.len() as i64;
            }
            shard_refs.push(SnapshotShardRef {
                path: shard_path,
                start: shard.start,
                end: shard.end,
                count: shard.items.len(),
                hash: shard_hash,
            });
        }

        let order = self.repository.build_snapshot_order(profile).await?;
        let order_json = serde_json::to_vec(&order)?;
        let order_hash = sha256_hex(&order_json);
        let order_path = "order/default.json".to_string();
        if previous_order_hash.as_deref() != Some(order_hash.as_str()) {
            let data = encode_snapshot_file(&order_json, profile, crypto_key.as_ref())?;
            let tmp_path = join_remote_path(".tmp", "order/default.json.tmp");
            provider.put(&tmp_path, data.clone()).await?;
            provider.put(&order_path, data).await?;
            if let Err(e) = provider.delete(&tmp_path).await {
                log::debug!(
                    "[Sync::Engine] Failed to cleanup order temp file {}: {}",
                    tmp_path,
                    e
                );
            }
        }

        let manifest = SnapshotManifest {
            schema_version: SNAPSHOT_SCHEMA_VERSION,
            app: APP_NAME.to_string(),
            generation: previous_manifest
                .as_ref()
                .map(|manifest| manifest.generation + 1)
                .unwrap_or(1),
            device_id: device_id.to_string(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            item_shard_size: SNAPSHOT_SHARD_SIZE,
            item_shards: shard_refs,
            order: Some(SnapshotFileRef {
                path: order_path,
                hash: order_hash,
            }),
        };
        write_manifest(provider, "", &manifest).await?;
        self.cleanup_unreferenced_snapshot_objects(provider, &manifest, &current_blob_paths)
            .await;

        Ok(SyncPhaseReport {
            items_uploaded,
            ..SyncPhaseReport::default()
        })
    }

    async fn cleanup_unreferenced_snapshot_objects(
        &self,
        provider: &dyn SyncProvider,
        manifest: &SnapshotManifest,
        current_blob_paths: &HashSet<String>,
    ) {
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
                        log::warn!(
                            "[Sync::Engine] Failed to delete unreferenced shard {}: {}",
                            object.path,
                            error
                        );
                    }
                }
            }
            Err(error) => log::warn!(
                "[Sync::Engine] Failed to list snapshot shards for cleanup: {}",
                error
            ),
        }

        match provider.list("blobs/").await {
            Ok(objects) => {
                for object in objects {
                    if !object.path.ends_with(".bin") || current_blob_paths.contains(&object.path) {
                        continue;
                    }
                    if let Err(error) = provider.delete(&object.path).await {
                        log::warn!(
                            "[Sync::Engine] Failed to delete unreferenced blob {}: {}",
                            object.path,
                            error
                        );
                    }
                }
            }
            Err(error) => log::warn!(
                "[Sync::Engine] Failed to list snapshot blobs for cleanup: {}",
                error
            ),
        }
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
