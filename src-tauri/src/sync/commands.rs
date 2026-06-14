/// Sync IPC Commands
use crate::db::database::Db;
use crate::sync::engine::SyncEngine;
use crate::sync::models::*;
use crate::sync::service::{SyncProfileInput, SyncService};
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn sync_profile_list(
    sync_service: State<'_, Arc<SyncService>>,
) -> Result<Vec<SyncProfileSummary>, String> {
    sync_service
        .list_profiles()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_profile_get(
    sync_service: State<'_, Arc<SyncService>>,
    profile_id: String,
) -> Result<SyncProfile, String> {
    sync_service
        .get_profile(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_profile_update(
    sync_service: State<'_, Arc<SyncService>>,
    profile: SyncProfileInput,
) -> Result<(), String> {
    sync_service
        .upsert_profile(profile)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_profile_delete(
    sync_service: State<'_, Arc<SyncService>>,
    profile_id: String,
) -> Result<(), String> {
    sync_service
        .delete_profile(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_profile_pause(
    sync_service: State<'_, Arc<SyncService>>,
    profile_id: String,
) -> Result<(), String> {
    sync_service
        .pause_profile(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_profile_resume(
    sync_service: State<'_, Arc<SyncService>>,
    profile_id: String,
) -> Result<(), String> {
    sync_service
        .resume_profile(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_secret_set(
    sync_service: State<'_, Arc<SyncService>>,
    profile_id: String,
    key: String,
    value: String,
) -> Result<SecretRef, String> {
    sync_service
        .set_secret(&profile_id, &key, value)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_secret_delete(
    sync_service: State<'_, Arc<SyncService>>,
    secret_ref: String,
) -> Result<(), String> {
    sync_service
        .delete_secret(&secret_ref)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_profile_unlock(
    sync_engine: State<'_, Arc<SyncEngine>>,
    profile_id: String,
    password: String,
    remember_with_system_keychain: bool,
) -> Result<(), String> {
    sync_engine
        .unlock_profile(&profile_id, &password, remember_with_system_keychain)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_profile_lock(
    sync_engine: State<'_, Arc<SyncEngine>>,
    profile_id: String,
) -> Result<(), String> {
    sync_engine
        .lock_profile(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_test_connection(
    sync_service: State<'_, Arc<SyncService>>,
    profile_id: String,
) -> Result<ConnectionTestResult, String> {
    sync_service
        .test_connection(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_trust_sftp_host_key(
    sync_service: State<'_, Arc<SyncService>>,
    profile_id: String,
) -> Result<SftpHostKeyTrustResult, String> {
    sync_service
        .trust_sftp_host_key(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_run_now(
    sync_service: State<'_, Arc<SyncService>>,
    profile_id: String,
) -> Result<SyncRunReport, String> {
    sync_service
        .run_now(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_cancel_run(
    sync_service: State<'_, Arc<SyncService>>,
    profile_id: String,
) -> Result<(), String> {
    sync_service
        .cancel_run(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_get_status(
    sync_service: State<'_, Arc<SyncService>>,
    profile_id: String,
) -> Result<SyncStatus, String> {
    sync_service
        .get_status(&profile_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_get_last_report(
    sync_service: State<'_, Arc<SyncService>>,
    profile_id: String,
) -> Result<Option<SyncRunReport>, String> {
    Ok(sync_service.get_last_report(&profile_id).await)
}

#[tauri::command]
pub async fn sync_get_conflicts(
    state: State<'_, Db>,
    _profile_id: String,
) -> Result<Vec<SyncConflict>, String> {
    let conflicts: Vec<SyncConflict> = sqlx::query_as(
        r#"
        SELECT id, entity_type, entity_key, local_payload, remote_payload,
               reason, status, resolution, created_at, resolved_at
        FROM sync_conflicts
        WHERE status = 'pending'
        ORDER BY created_at ASC
        "#,
    )
    .fetch_all(state.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(conflicts)
}

#[tauri::command]
pub async fn sync_resolve_conflict(
    state: State<'_, Db>,
    _profile_id: String,
    conflict_id: i64,
    resolution: ConflictResolutionInput,
) -> Result<(), String> {
    let conflict: SyncConflict = sqlx::query_as(
        r#"
        SELECT id, entity_type, entity_key, local_payload, remote_payload,
               reason, status, resolution, created_at, resolved_at
        FROM sync_conflicts
        WHERE id = ? AND status = 'pending'
        "#,
    )
    .bind(conflict_id)
    .fetch_optional(state.inner())
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("Pending conflict {} not found", conflict_id))?;

    if conflict.entity_type != "clipboard_item" {
        return Err(format!(
            "Unsupported sync conflict entity type: {}",
            conflict.entity_type
        ));
    }

    apply_clipboard_conflict_resolution(state.inner(), &conflict, &resolution).await?;

    sqlx::query(
        r#"
        UPDATE sync_conflicts
        SET status = 'resolved',
            resolution = ?,
            resolved_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(format!("{:?}", resolution))
    .bind(conflict_id)
    .execute(state.inner())
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

async fn apply_clipboard_conflict_resolution(
    db: &Db,
    conflict: &SyncConflict,
    resolution: &ConflictResolutionInput,
) -> Result<(), String> {
    match resolution {
        ConflictResolutionInput::UseLocal | ConflictResolutionInput::MergeWithLocalPrimary => {
            let local_id: Option<(i64,)> =
                sqlx::query_as("SELECT local_id FROM sync_item_map WHERE item_key = ?")
                    .bind(&conflict.entity_key)
                    .fetch_optional(db)
                    .await
                    .map_err(|e| e.to_string())?;
            let Some((local_id,)) = local_id else {
                return Err(format!(
                    "Cannot keep local for unmapped item_key {}",
                    conflict.entity_key
                ));
            };

            sqlx::query(
                r#"
                INSERT INTO sync_changes
                    (entity_type, entity_id, operation, item_key, source, changed_at)
                VALUES ('clipboard_item', ?, 'update', ?, 'sync_resolution', datetime('now'))
                "#,
            )
            .bind(local_id.to_string())
            .bind(&conflict.entity_key)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        }
        ConflictResolutionInput::UseRemote | ConflictResolutionInput::MergeWithRemotePrimary => {
            let remote: RemoteClipboardItem =
                serde_json::from_str(&conflict.remote_payload).map_err(|e| e.to_string())?;
            let local_id =
                apply_remote_payload_to_mapped_item(db, &conflict.entity_key, &remote).await?;
            sqlx::query(
                r#"
                INSERT INTO sync_changes
                    (entity_type, entity_id, operation, item_key, source, changed_at, synced_at)
                VALUES ('clipboard_item', ?, 'update', ?, 'remote_apply', datetime('now'), datetime('now'))
                "#,
            )
            .bind(local_id.to_string())
            .bind(&conflict.entity_key)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        }
        ConflictResolutionInput::KeepBoth => {
            let remote: RemoteClipboardItem =
                serde_json::from_str(&conflict.remote_payload).map_err(|e| e.to_string())?;
            let local_id = insert_remote_payload_as_new_local_item(db, &remote).await?;
            sqlx::query(
                r#"
                INSERT INTO sync_changes
                    (entity_type, entity_id, operation, source, changed_at)
                VALUES ('clipboard_item', ?, 'create', 'sync_resolution', datetime('now'))
                "#,
            )
            .bind(local_id.to_string())
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(())
}

async fn apply_remote_payload_to_mapped_item(
    db: &Db,
    item_key: &str,
    remote: &RemoteClipboardItem,
) -> Result<i64, String> {
    let local_id: Option<(i64,)> =
        sqlx::query_as("SELECT local_id FROM sync_item_map WHERE item_key = ?")
            .bind(item_key)
            .fetch_optional(db)
            .await
            .map_err(|e| e.to_string())?;
    let Some((local_id,)) = local_id else {
        return insert_remote_payload_with_mapping(db, remote).await;
    };

    let tags = tags_to_db(&remote.tags);
    let metadata = serde_json::to_string(&remote.metadata).map_err(|e| e.to_string())?;
    sqlx::query(
        r#"
        UPDATE clipboard_items
        SET type = ?,
            content = ?,
            content_hash = ?,
            metadata = ?,
            tags = ?,
            is_sensitive = ?,
            is_pinned = ?,
            updated_at = datetime('now')
        WHERE id = ?
        "#,
    )
    .bind(&remote.item_type)
    .bind(remote.content.clone().unwrap_or_default())
    .bind(&remote.content_hash)
    .bind(metadata)
    .bind(tags)
    .bind(if remote.is_sensitive { 1 } else { 0 })
    .bind(if remote.is_pinned { 1 } else { 0 })
    .bind(local_id)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(local_id)
}

async fn insert_remote_payload_with_mapping(
    db: &Db,
    remote: &RemoteClipboardItem,
) -> Result<i64, String> {
    let local_id = insert_remote_payload_as_new_local_item(db, remote).await?;
    sqlx::query(
        r#"
        INSERT INTO sync_item_map
            (local_id, item_key, remote_path, last_remote_updated_at, last_synced_at)
        VALUES (?, ?, ?, ?, datetime('now'))
        ON CONFLICT(item_key) DO UPDATE SET
            local_id = excluded.local_id,
            remote_path = excluded.remote_path,
            last_remote_updated_at = excluded.last_remote_updated_at,
            last_synced_at = datetime('now')
        "#,
    )
    .bind(local_id)
    .bind(&remote.item_key)
    .bind(format!("items/{}.json", remote.item_key))
    .bind(&remote.updated_at)
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(local_id)
}

async fn insert_remote_payload_as_new_local_item(
    db: &Db,
    remote: &RemoteClipboardItem,
) -> Result<i64, String> {
    let default_tab: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM tabs WHERE is_default = 1 LIMIT 1")
            .fetch_optional(db)
            .await
            .map_err(|e| e.to_string())?;
    let tags = tags_to_db(&remote.tags);
    let metadata = serde_json::to_string(&remote.metadata).map_err(|e| e.to_string())?;
    let result = sqlx::query(
        r#"
        INSERT INTO clipboard_items
            (type, content, content_hash, metadata, tags, tab_id, is_sensitive, is_pinned, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
        "#,
    )
    .bind(&remote.item_type)
    .bind(remote.content.clone().unwrap_or_default())
    .bind(&remote.content_hash)
    .bind(metadata)
    .bind(tags)
    .bind(default_tab.map(|(id,)| id))
    .bind(if remote.is_sensitive { 1 } else { 0 })
    .bind(if remote.is_pinned { 1 } else { 0 })
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(result.last_insert_rowid())
}

fn tags_to_db(tags: &[String]) -> Option<String> {
    if tags.is_empty() {
        None
    } else {
        Some(tags.join(","))
    }
}

#[tauri::command]
pub async fn sync_get_tab_options(state: State<'_, Db>) -> Result<Vec<SyncTabOption>, String> {
    use crate::db::repositories::TabRepository;
    let tabs = TabRepository::get_all(state.inner())
        .await
        .map_err(|e| e.to_string())?;
    Ok(tabs
        .into_iter()
        .map(|tab| SyncTabOption {
            id: tab.id.unwrap_or(0),
            name: tab.name,
        })
        .collect())
}

#[tauri::command]
pub async fn sync_get_plugin_options(
    sync_service: State<'_, Arc<SyncService>>,
) -> Result<Vec<SyncPluginOption>, String> {
    sync_service
        .list_plugin_options()
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn sync_get_log_entries(
    sync_service: State<'_, Arc<SyncService>>,
    profile_id: String,
    limit: i64,
) -> Result<Vec<SyncLogEntry>, String> {
    sync_service
        .list_log_entries(&profile_id, limit)
        .await
        .map_err(|e| e.to_string())
}
