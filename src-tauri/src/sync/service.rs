use crate::file_sync::FileSyncService;
use crate::plugin::lifecycle::registry::PluginRegistry;
use crate::sync::engine::SyncEngine;
use crate::sync::error::SyncError;
use crate::sync::models::*;
use crate::sync::providers::factory::ProviderFactory;
use crate::sync::repository::SyncRepository;
use crate::sync::secrets::SecretStore;
use base64::Engine;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::RwLock;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SyncCompletedEventPayload {
    profile_id: String,
    report: SyncRunReport,
}

pub struct SyncService {
    repository: Arc<SyncRepository>,
    secret_store: Arc<SecretStore>,
    engine: Arc<SyncEngine>,
    plugin_registry: Arc<RwLock<PluginRegistry>>,
    file_sync_service: Arc<FileSyncService>,
    provider_factory: ProviderFactory,
    app_handle: tauri::AppHandle,
}

impl SyncService {
    pub fn new(
        repository: Arc<SyncRepository>,
        secret_store: Arc<SecretStore>,
        engine: Arc<SyncEngine>,
        plugin_registry: Arc<RwLock<PluginRegistry>>,
        file_sync_service: Arc<FileSyncService>,
        app_handle: tauri::AppHandle,
    ) -> Self {
        let provider_factory = ProviderFactory::new(secret_store.clone());
        Self {
            repository,
            secret_store,
            engine,
            plugin_registry,
            file_sync_service,
            provider_factory,
            app_handle,
        }
    }

    pub async fn list_profiles(&self) -> Result<Vec<SyncProfileSummary>, SyncError> {
        self.repository.list_profiles().await
    }

    pub async fn get_profile(&self, profile_id: &str) -> Result<SyncProfile, SyncError> {
        self.repository.get_profile(profile_id).await
    }

    pub async fn upsert_profile(&self, input: SyncProfileInput) -> Result<(), SyncError> {
        let existing = self.repository.get_profile(&input.id).await.ok();
        let provider = SyncProviderKind::try_from(input.provider.as_str())?;
        let mut encryption = input
            .encryption
            .or_else(|| existing.as_ref().map(|p| p.encryption.clone()))
            .unwrap_or_default();
        if encryption.enabled && encryption.salt_b64.is_none() {
            encryption.salt_b64 = existing
                .as_ref()
                .and_then(|profile| profile.encryption.salt_b64.clone())
                .or_else(|| {
                    let salt: [u8; 16] = rand::random();
                    Some(base64::engine::general_purpose::STANDARD.encode(salt))
                });
        }
        if !encryption.enabled {
            encryption.salt_b64 = None;
        }

        let schedule = input
            .schedule
            .or_else(|| existing.as_ref().map(|p| p.schedule.clone()))
            .unwrap_or_default();

        let sync_profile = SyncProfile {
            id: input.id.clone(),
            name: input.name,
            provider,
            remote_root: input.remote_root.trim().to_string(),
            sync_tabs: input.sync_tabs.unwrap_or_default(),
            sync_plugins: input.sync_plugins.unwrap_or_default(),
            encryption,
            credential_refs: input
                .credential_refs
                .or_else(|| existing.as_ref().map(|p| p.credential_refs.clone()))
                .unwrap_or_default(),
            schedule,
            created_at: None,
            updated_at: None,
        };

        let publishes_to_new_root = existing.as_ref().is_none_or(|profile| {
            profile.provider != sync_profile.provider || profile.remote_root != sync_profile.remote_root
        });
        self.repository.upsert_profile(sync_profile.clone()).await?;
        if publishes_to_new_root {
            self.repository
                .queue_full_snapshot_upload(&sync_profile.id)
                .await?;
        }
        Ok(())
    }

    pub async fn delete_profile(&self, profile_id: &str) -> Result<(), SyncError> {
        self.secret_store.delete_profile_secrets(profile_id).await?;
        self.repository.delete_profile(profile_id).await
    }

    pub async fn set_secret(
        &self,
        profile_id: &str,
        key: &str,
        value: String,
    ) -> Result<SecretRef, SyncError> {
        let secret_ref = self
            .secret_store
            .set(profile_id, key, value.as_bytes())
            .await?;
        log::info!("[Sync::Service] Secret stored: {}", secret_ref.ref_id);
        Ok(secret_ref)
    }

    pub async fn delete_secret(&self, secret_ref: &str) -> Result<(), SyncError> {
        self.secret_store.delete(secret_ref).await?;
        log::info!("[Sync::Service] Secret deleted: {}", secret_ref);
        Ok(())
    }

    pub async fn test_connection(
        &self,
        profile_id: &str,
    ) -> Result<ConnectionTestResult, SyncError> {
        let profile = self.repository.get_profile(profile_id).await?;
        self.provider_factory.test_connection(&profile).await
    }

    pub async fn trust_sftp_host_key(
        &self,
        profile_id: &str,
    ) -> Result<SftpHostKeyTrustResult, SyncError> {
        let profile = self.repository.get_profile(profile_id).await?;
        self.provider_factory.trust_sftp_host_key(&profile).await
    }

    pub async fn run_now(&self, profile_id: &str) -> Result<SyncRunReport, SyncError> {
        let profile = self.repository.get_profile(profile_id).await?;
        if profile.schedule.paused {
            return Err(SyncError::Validation(
                "Sync profile is paused. Resume it before running sync.".to_string(),
            ));
        }
        if profile.encryption.enabled && !self.engine.is_profile_unlocked(profile_id).await {
            return Err(SyncError::Encryption(
                "Encrypted profile is locked. Unlock it before syncing.".to_string(),
            ));
        }
        let provider = self.provider_factory.build(&profile).await?;
        let report = self.engine.run_now(profile_id, provider).await?;
        if let Err(error) = self.file_sync_service.refresh(profile_id).await {
            log::warn!(
                "[Sync::Service] File Sync refresh failed after sync for profile {}: {}",
                profile_id,
                error
            );
        }
        self.emit_sync_completed(profile_id, &report);
        Ok(report)
    }

    fn emit_sync_completed(&self, profile_id: &str, report: &SyncRunReport) {
        let payload = SyncCompletedEventPayload {
            profile_id: profile_id.to_string(),
            report: report.clone(),
        };
        if let Err(error) = self.app_handle.emit("sync:completed", payload) {
            log::warn!(
                "[Sync::Service] Failed to emit sync completion event: {}",
                error
            );
        }
    }

    pub async fn run_startup_profiles(&self) {
        let profiles = match self.repository.list_profiles().await {
            Ok(profiles) => profiles,
            Err(e) => {
                log::warn!(
                    "[Sync::Service] Failed to list startup sync profiles: {}",
                    e
                );
                return;
            }
        };

        for summary in profiles {
            let profile = match self.repository.get_profile(&summary.id).await {
                Ok(profile) => profile,
                Err(e) => {
                    log::warn!(
                        "[Sync::Service] Failed to load profile {}: {}",
                        summary.id,
                        e
                    );
                    continue;
                }
            };

            if profile.schedule.paused || !profile.schedule.sync_on_startup {
                continue;
            }
            if profile.encryption.enabled && !self.engine.is_profile_unlocked(&profile.id).await {
                log::info!(
                    "[Sync::Service] Skipping startup sync for locked encrypted profile {}",
                    profile.id
                );
                continue;
            }
            if let Err(e) = self.run_now(&profile.id).await {
                log::warn!(
                    "[Sync::Service] Startup sync failed for profile {}: {}",
                    profile.id,
                    e
                );
            }
        }
    }

    pub async fn run_profiles_with_pending_changes(&self) {
        let profiles = match self.repository.list_profiles().await {
            Ok(profiles) => profiles,
            Err(e) => {
                log::warn!(
                    "[Sync::Service] Failed to list local-change sync profiles: {}",
                    e
                );
                return;
            }
        };

        for summary in profiles {
            let profile = match self.repository.get_profile(&summary.id).await {
                Ok(profile) => profile,
                Err(e) => {
                    log::warn!(
                        "[Sync::Service] Failed to load profile {}: {}",
                        summary.id,
                        e
                    );
                    continue;
                }
            };

            if profile.schedule.paused || !profile.schedule.sync_on_local_change {
                continue;
            }
            if profile.encryption.enabled && !self.engine.is_profile_unlocked(&profile.id).await {
                continue;
            }

            match self.repository.has_unsynced_changes(&profile).await {
                Ok(true) => {
                    if let Err(e) = self.run_now(&profile.id).await {
                        log::warn!(
                            "[Sync::Service] Local-change sync failed for profile {}: {}",
                            profile.id,
                            e
                        );
                    }
                }
                Ok(false) => {}
                Err(e) => log::warn!(
                    "[Sync::Service] Failed to inspect pending changes for {}: {}",
                    profile.id,
                    e
                ),
            }
        }
    }

    pub async fn run_scheduler_loop(self: Arc<Self>) {
        let scheduler_started_at = chrono::Utc::now();
        let mut startup_completed: HashSet<String> = HashSet::new();
        let mut pending_since: HashMap<String, chrono::DateTime<chrono::Utc>> = HashMap::new();
        let mut last_interval_run: HashMap<String, chrono::DateTime<chrono::Utc>> = HashMap::new();
        let mut next_retry_at: HashMap<String, chrono::DateTime<chrono::Utc>> = HashMap::new();
        let mut failure_counts: HashMap<String, usize> = HashMap::new();
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(5));

        loop {
            tick.tick().await;
            let now = chrono::Utc::now();
            let profiles = match self.repository.list_profiles().await {
                Ok(profiles) => profiles,
                Err(e) => {
                    log::warn!("[Sync::Scheduler] Failed to list profiles: {}", e);
                    continue;
                }
            };

            for summary in profiles {
                let profile = match self.repository.get_profile(&summary.id).await {
                    Ok(profile) => profile,
                    Err(e) => {
                        log::warn!(
                            "[Sync::Scheduler] Failed to load profile {}: {}",
                            summary.id,
                            e
                        );
                        continue;
                    }
                };

                if profile.schedule.paused {
                    pending_since.remove(&profile.id);
                    continue;
                }
                if profile.encryption.enabled && !self.engine.is_profile_unlocked(&profile.id).await
                {
                    continue;
                }
                if profile.schedule.pause_on_metered_network {
                    log::debug!(
                        "[Sync::Scheduler] Metered-network detection is unavailable; profile {} is not paused",
                        profile.id
                    );
                }
                if next_retry_at
                    .get(&profile.id)
                    .map(|retry_at| now < *retry_at)
                    .unwrap_or(false)
                {
                    continue;
                }

                let mut should_run = false;

                if profile.schedule.sync_on_startup && !startup_completed.contains(&profile.id) {
                    let elapsed = now
                        .signed_duration_since(scheduler_started_at)
                        .num_seconds()
                        .max(0) as u64;
                    if elapsed >= profile.schedule.startup_delay_seconds {
                        should_run = true;
                        startup_completed.insert(profile.id.clone());
                    }
                }

                if !should_run && profile.schedule.interval_minutes > 0 {
                    let interval_seconds = (profile.schedule.interval_minutes * 60) as i64;
                    let last_run = last_interval_run
                        .get(&profile.id)
                        .copied()
                        .or_else(|| summary.last_sync_at.as_deref().and_then(parse_sync_time));
                    if last_run
                        .map(|last| {
                            now.signed_duration_since(last).num_seconds() >= interval_seconds
                        })
                        .unwrap_or(true)
                    {
                        should_run = true;
                    }
                }

                if !should_run && profile.schedule.sync_on_local_change {
                    match self.repository.has_unsynced_changes(&profile).await {
                        Ok(true) => {
                            let first_seen = pending_since.entry(profile.id.clone()).or_insert(now);
                            let elapsed =
                                now.signed_duration_since(*first_seen).num_seconds().max(0) as u64;
                            if elapsed >= profile.schedule.local_change_debounce_seconds {
                                should_run = true;
                            }
                        }
                        Ok(false) => {
                            pending_since.remove(&profile.id);
                        }
                        Err(e) => {
                            log::warn!(
                                "[Sync::Scheduler] Failed to inspect pending changes for {}: {}",
                                profile.id,
                                e
                            );
                        }
                    }
                }

                if should_run {
                    match self.run_now(&profile.id).await {
                        Ok(report) => {
                            last_interval_run.insert(profile.id.clone(), now);
                            if report.errors.is_empty() {
                                pending_since.remove(&profile.id);
                                failure_counts.remove(&profile.id);
                                next_retry_at.remove(&profile.id);
                            }
                        }
                        Err(e) => {
                            let failure_count =
                                failure_counts.entry(profile.id.clone()).or_insert(0);
                            let backoff_seconds = profile
                                .schedule
                                .retry_backoff_seconds
                                .get(*failure_count)
                                .copied()
                                .or_else(|| profile.schedule.retry_backoff_seconds.last().copied())
                                .unwrap_or(300);
                            *failure_count += 1;
                            next_retry_at.insert(
                                profile.id.clone(),
                                now + chrono::Duration::seconds(backoff_seconds as i64),
                            );
                            log::warn!(
                                "[Sync::Scheduler] Scheduled retry for profile {} in {}s after failure: {}",
                                profile.id,
                                backoff_seconds,
                                e
                            );
                        }
                    }
                }
            }
        }
    }

    pub async fn cancel_run(&self, profile_id: &str) -> Result<(), SyncError> {
        self.engine.cancel_run(profile_id).await
    }

    pub async fn get_status(&self, profile_id: &str) -> Result<SyncStatus, SyncError> {
        self.engine.get_status(profile_id).await
    }

    pub async fn get_last_report(&self, profile_id: &str) -> Option<SyncRunReport> {
        self.engine.get_last_report(profile_id).await
    }

    pub async fn pause_profile(&self, profile_id: &str) -> Result<(), SyncError> {
        self.repository.set_profile_paused(profile_id, true).await?;
        log::info!("[Sync::Service] Profile paused: {}", profile_id);
        Ok(())
    }

    pub async fn resume_profile(&self, profile_id: &str) -> Result<(), SyncError> {
        self.repository
            .set_profile_paused(profile_id, false)
            .await?;
        log::info!("[Sync::Service] Profile resumed: {}", profile_id);
        Ok(())
    }

    pub async fn list_plugin_options(&self) -> Result<Vec<SyncPluginOption>, SyncError> {
        let registry = self.plugin_registry.read().await;
        Ok(registry
            .get_all()
            .into_iter()
            .map(|plugin| SyncPluginOption {
                id: plugin.id,
                name: plugin.name,
                is_active: plugin.state.is_active(),
            })
            .collect())
    }

    pub async fn list_log_entries(
        &self,
        profile_id: &str,
        limit: i64,
    ) -> Result<Vec<SyncLogEntry>, SyncError> {
        self.repository.list_log_entries(profile_id, limit).await
    }
}

fn parse_sync_time(value: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .ok()
}

#[derive(serde::Deserialize)]
pub struct SyncProfileInput {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub remote_root: String,
    pub sync_tabs: Option<TabSyncSelection>,
    pub sync_plugins: Option<PluginSyncSelection>,
    pub encryption: Option<EncryptionConfig>,
    pub credential_refs: Option<CredentialRefs>,
    pub schedule: Option<SyncScheduleConfig>,
}

impl TryFrom<&str> for SyncProviderKind {
    type Error = SyncError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "webdav" => Ok(SyncProviderKind::WebDav),
            "sftp" => Ok(SyncProviderKind::Sftp),
            "google_drive" => Ok(SyncProviderKind::GoogleDrive),
            "one_drive" => Ok(SyncProviderKind::OneDrive),
            other => Err(SyncError::Validation(format!(
                "Unsupported sync provider: {}",
                other
            ))),
        }
    }
}
