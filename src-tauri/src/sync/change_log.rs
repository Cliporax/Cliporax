/// Change log - manage remote change logs for incremental sync
use crate::sync::codec::decode_change;
use crate::sync::error::SyncError;
use crate::sync::models::*;
use crate::sync::providers::{join_remote_path, SyncProvider};
use chrono::Datelike;

/// List remote devices from change logs
pub async fn list_remote_devices(
    provider: &dyn SyncProvider,
    _base_path: &str,
) -> Result<Vec<String>, SyncError> {
    let objects = match provider.list("changes/").await {
        Ok(objects) => objects,
        Err(e) => {
            log::debug!(
                "[Sync::ChangeLog] No remote changes directory yet, treating as empty: {}",
                e
            );
            return Ok(Vec::new());
        }
    };

    // Extract device IDs from directory names (changes/<device_id>/)
    let devices: Vec<String> = objects
        .iter()
        .filter_map(|obj| {
            // Path format: changes/<device_id>/ or changes/<device_id>
            let parts: Vec<&str> = obj.path.split('/').collect();
            if parts.len() >= 2 && parts[0] == "changes" && !parts[1].is_empty() {
                Some(parts[1].to_string())
            } else {
                None
            }
        })
        .collect();

    // Deduplicate
    let mut unique_devices = devices.clone();
    unique_devices.sort();
    unique_devices.dedup();

    log::debug!(
        "[Sync::ChangeLog] Found remote devices: {:?}",
        unique_devices
    );
    Ok(unique_devices)
}

/// List remote changes after a given sequence number
pub async fn list_remote_changes_after(
    provider: &dyn SyncProvider,
    _base_path: &str,
    remote_device_id: &str,
    last_seq: i64,
) -> Result<Vec<RemoteChange>, SyncError> {
    let prefix = join_remote_path("changes", remote_device_id) + "/";
    let objects = match provider.list(&prefix).await {
        Ok(objects) => objects,
        Err(e) => {
            log::debug!(
                "[Sync::ChangeLog] No remote changes for device {} yet: {}",
                remote_device_id,
                e
            );
            return Ok(Vec::new());
        }
    };

    let mut changes = Vec::new();

    for obj in objects {
        // Extract sequence number from filename (e.g., "00000005.json" -> 5)
        let filename = std::path::Path::new(&obj.path)
            .file_stem()
            .and_then(|s| s.to_str());

        if let Some(seq_str) = filename {
            if let Ok(seq) = seq_str.parse::<i64>() {
                if seq > last_seq {
                    // Download and parse the change file
                    match provider.get(&obj.path).await {
                        Ok(data) => match decode_change(&data) {
                            Ok(change) => changes.push(change),
                            Err(e) => {
                                log::warn!(
                                    "[Sync::ChangeLog] Failed to decode change {}: {}",
                                    obj.path,
                                    e
                                );
                            }
                        },
                        Err(e) => {
                            log::warn!("[Sync::ChangeLog] Failed to download {}: {}", obj.path, e);
                        }
                    }
                }
            }
        }
    }

    // Sort by sequence number
    changes.sort_by_key(|c| c.seq);

    log::debug!(
        "[Sync::ChangeLog] Found {} changes for {} after seq {}",
        changes.len(),
        remote_device_id,
        last_seq
    );
    Ok(changes)
}

/// Get next sequence number for local device
pub async fn next_local_remote_seq(
    provider: &dyn SyncProvider,
    _base_path: &str,
    local_device_id: &str,
) -> Result<i64, SyncError> {
    let prefix = join_remote_path("changes", local_device_id) + "/";
    let objects = match provider.list(&prefix).await {
        Ok(objects) => objects,
        Err(e) => {
            log::debug!(
                "[Sync::ChangeLog] No local remote change log yet, starting at seq 1: {}",
                e
            );
            return Ok(1);
        }
    };
    let max_seq = objects
        .iter()
        .filter_map(|obj| {
            let filename = std::path::Path::new(&obj.path).file_stem()?.to_str()?;
            filename.parse::<i64>().ok()
        })
        .max()
        .unwrap_or(0);

    Ok(max_seq + 1)
}

/// Build change file path
pub fn build_change_path(device_id: &str, seq: i64) -> String {
    join_remote_path(
        &join_remote_path("changes", device_id),
        &format!("{:08}.json", seq),
    )
}

/// Build item file path
pub fn build_item_path(item: &RemoteClipboardItem) -> String {
    let created_at = chrono::DateTime::parse_from_rfc3339(&item.created_at)
        .unwrap_or_else(|_| chrono::DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap());
    format!(
        "items/{:04}/{:02}/{}.json",
        created_at.year(),
        created_at.month(),
        item.item_key
    )
}

/// Build tombstone file path
pub fn build_tombstone_path(item_key: &str) -> String {
    format!("tombstones/{}.json", item_key)
}
