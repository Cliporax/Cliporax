/// Lock - remote sync lock management
use crate::sync::error::SyncError;
use crate::sync::models::*;
use crate::sync::providers::SyncProvider;

/// Acquire a remote lock with wait and retry
pub async fn acquire_remote_lock(
    provider: &dyn SyncProvider,
    lock_path: &str,
    owner: RemoteLock,
    wait_config: LockWaitConfig,
) -> Result<RemoteLockGuard, SyncError> {
    let start = std::time::Instant::now();

    loop {
        if start.elapsed().as_secs() > wait_config.max_wait_seconds {
            return Err(SyncError::Lock(format!(
                "Failed to acquire lock after {} seconds",
                wait_config.max_wait_seconds
            )));
        }

        // Try to read existing lock
        let existing_lock = read_remote_lock(provider, lock_path).await?;

        if let Some(lock) = existing_lock {
            // Check if lock has expired
            let expires_at = chrono::DateTime::parse_from_rfc3339(&lock.expires_at)
                .map_err(|e| SyncError::Lock(format!("Invalid expires_at: {}", e)))?;

            if expires_at
                > chrono::Utc::now()
                    .to_rfc3339()
                    .parse::<chrono::DateTime<chrono::FixedOffset>>()
                    .unwrap()
            {
                // Lock is still valid, wait and retry
                tokio::time::sleep(tokio::time::Duration::from_millis(
                    wait_config.retry_interval_ms,
                ))
                .await;
                continue;
            }

            // Lock has expired, overwrite it
        }

        // Create new lock
        let lock_json = serde_json::to_string_pretty(&owner).map_err(SyncError::Serialization)?;

        provider.put(lock_path, lock_json.into_bytes()).await?;

        // Verify lock
        let verified = read_remote_lock(provider, lock_path).await?;
        if let Some(verified_lock) = verified {
            if verified_lock.owner_run_id == owner.owner_run_id {
                return Ok(RemoteLockGuard {
                    provider_path: lock_path.to_string(),
                    owner_run_id: owner.owner_run_id,
                });
            }
        }

        // Verification failed, wait and retry
        tokio::time::sleep(tokio::time::Duration::from_millis(
            wait_config.retry_interval_ms,
        ))
        .await;
    }
}

/// Read remote lock
pub async fn read_remote_lock(
    provider: &dyn SyncProvider,
    lock_path: &str,
) -> Result<Option<RemoteLock>, SyncError> {
    match provider.stat(lock_path).await? {
        Some(_) => {
            let data = provider.get(lock_path).await?;
            let lock: RemoteLock = serde_json::from_slice(&data)
                .map_err(|e| SyncError::Lock(format!("Failed to parse lock: {}", e)))?;
            Ok(Some(lock))
        }
        None => Ok(None),
    }
}

/// Release remote lock (only if we own it)
pub async fn release_remote_lock(
    provider: &dyn SyncProvider,
    lock_path: &str,
    owner_run_id: &str,
) -> Result<(), SyncError> {
    let lock = read_remote_lock(provider, lock_path).await?;
    if let Some(lock) = lock {
        if lock.owner_run_id == owner_run_id {
            provider.delete(lock_path).await?;
        }
    }
    Ok(())
}

/// Guard that releases lock on drop
pub struct RemoteLockGuard {
    provider_path: String,
    owner_run_id: String,
}

impl RemoteLockGuard {
    pub fn path(&self) -> &str {
        &self.provider_path
    }
}

impl Drop for RemoteLockGuard {
    fn drop(&mut self) {
        // Note: Can't use async in Drop, so lock cleanup happens on next run
        log::debug!(
            "[Sync::Lock] Lock guard dropped for run: {}",
            self.owner_run_id
        );
    }
}
