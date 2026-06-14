/// Scheduler - manage sync scheduling and triggers
use crate::sync::error::SyncError;
use crate::sync::models::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct SyncScheduler {
    #[allow(dead_code)]
    profiles: Arc<RwLock<HashMap<String, SyncScheduleConfig>>>,
    pending_runs: Arc<RwLock<HashMap<String, bool>>>,
}

impl SyncScheduler {
    pub fn new() -> Self {
        Self {
            profiles: Arc::new(RwLock::new(HashMap::new())),
            pending_runs: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Schedule manual sync
    pub async fn schedule_manual_sync(&self, profile_id: &str) -> Result<(), SyncError> {
        log::info!(
            "[Sync::Scheduler] Manual sync scheduled for profile: {}",
            profile_id
        );
        let mut pending = self.pending_runs.write().await;
        pending.insert(profile_id.to_string(), true);
        Ok(())
    }

    /// Schedule startup sync
    pub async fn schedule_startup_sync(&self) -> Result<(), SyncError> {
        log::info!("[Sync::Scheduler] Startup sync scheduled");
        // This will be triggered by the main app on startup
        Ok(())
    }

    /// Notify local change for debounced sync
    pub async fn notify_local_change(&self, _change: LocalSyncChange) -> Result<(), SyncError> {
        // TODO: Implement debounced sync trigger
        Ok(())
    }

    /// Pause a profile
    pub async fn pause_profile(&self, profile_id: &str) -> Result<(), SyncError> {
        log::info!("[Sync::Scheduler] Profile paused: {}", profile_id);
        let mut pending = self.pending_runs.write().await;
        pending.remove(profile_id);
        Ok(())
    }

    /// Resume a profile
    pub async fn resume_profile(&self, profile_id: &str) -> Result<(), SyncError> {
        log::info!("[Sync::Scheduler] Profile resumed: {}", profile_id);
        Ok(())
    }

    /// Get pending runs
    pub async fn get_pending_runs(&self) -> Vec<String> {
        let pending = self.pending_runs.read().await;
        pending.keys().cloned().collect()
    }
}

impl Default for SyncScheduler {
    fn default() -> Self {
        Self::new()
    }
}
