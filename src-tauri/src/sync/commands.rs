/// Sync IPC Commands
use crate::db::database::Db;
use crate::sync::engine::SyncEngine;
use crate::sync::models::*;
use crate::sync::repository_tabs::{local_id_for_remote_tab, trash_tab_id};
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
    sync_service: State<'_, Arc<SyncService>>,
    profile_id: String,
    conflict_id: i64,
    resolution: ConflictResolutionInput,
) -> Result<ConflictResolutionResult, String> {
    sync_service
        .resolve_conflict(&profile_id, conflict_id, resolution)
        .await
        .map_err(|error| error.to_string())
}

pub(crate) async fn resolve_conflict(
    db: &Db,
    conflict_id: i64,
    resolution: ConflictResolutionInput,
) -> Result<(), String> {
    if conflict_id <= 0 {
        return Err("Invalid conflict ID".into());
    }
    let conflict: SyncConflict = sqlx::query_as(
        r#"
        SELECT id, entity_type, entity_key, local_payload, remote_payload,
               reason, status, resolution, created_at, resolved_at
        FROM sync_conflicts
        WHERE id = ? AND status = 'pending'
        "#,
    )
    .bind(conflict_id)
    .fetch_optional(db)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| format!("Pending conflict {} not found", conflict_id))?;

    match conflict.entity_type.as_str() {
        "clipboard_item" => {
            apply_clipboard_conflict_resolution(db, &conflict, &resolution).await?;
            let remote: RemoteClipboardItem =
                serde_json::from_str(&conflict.remote_payload).map_err(|e| e.to_string())?;
            // Choosing a version is a new edit. Publish a timestamp newer than
            // both sides, even if another device's clock is ahead of this one.
            sqlx::query("UPDATE clipboard_items SET updated_at = strftime('%Y-%m-%d %H:%M:%f', MAX(COALESCE(julianday(updated_at), 0), COALESCE(julianday(json_extract(?, '$.updated_at')), 0), julianday(?), julianday('now')) + 1.0 / 86400000.0) WHERE id = (SELECT local_id FROM sync_item_map WHERE item_key = ?)")
                .bind(&conflict.local_payload).bind(&remote.updated_at).bind(&conflict.entity_key)
                .execute(db).await.map_err(|e| e.to_string())?;
            sqlx::query("UPDATE sync_item_map SET last_remote_updated_at = ? WHERE item_key = ?")
                .bind(&remote.updated_at)
                .bind(&conflict.entity_key)
                .execute(db)
                .await
                .map_err(|e| e.to_string())?;
        }
        "plugin_data" => apply_plugin_conflict_resolution(db, &conflict, &resolution).await?,
        _ => {
            return Err(format!(
                "Unsupported sync conflict entity type: {}",
                conflict.entity_type
            ))
        }
    }

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
    .execute(db)
    .await
    .map_err(|e| e.to_string())?;

    Ok(())
}

async fn apply_plugin_conflict_resolution(
    db: &Db,
    conflict: &SyncConflict,
    resolution: &ConflictResolutionInput,
) -> Result<(), String> {
    let remote: RemotePluginData =
        serde_json::from_str(&conflict.remote_payload).map_err(|e| e.to_string())?;
    match resolution {
        ConflictResolutionInput::UseRemote | ConflictResolutionInput::MergeWithRemotePrimary => {
            sqlx::query("INSERT INTO plugin_sync_data (plugin_id, storage_key, value_json, updated_at) VALUES (?, ?, ?, strftime('%Y-%m-%d %H:%M:%f', 'now')) ON CONFLICT(plugin_id, storage_key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at")
                .bind(&remote.plugin_id).bind(&remote.storage_key).bind(serde_json::to_string(&remote.value).map_err(|e| e.to_string())?).execute(db).await.map_err(|e| e.to_string())?;
        }
        ConflictResolutionInput::UseLocal | ConflictResolutionInput::MergeWithLocalPrimary => {
            sqlx::query("UPDATE plugin_sync_data SET updated_at = strftime('%Y-%m-%d %H:%M:%f', 'now') WHERE plugin_id = ? AND storage_key = ?")
                .bind(&remote.plugin_id).bind(&remote.storage_key).execute(db).await.map_err(|e| e.to_string())?;
        }
        ConflictResolutionInput::KeepBoth => {
            return Err("Choose local or remote for plugin data".into())
        }
    }
    sqlx::query("UPDATE plugin_sync_data SET updated_at = strftime('%Y-%m-%d %H:%M:%f', MAX(COALESCE(julianday(updated_at), 0), COALESCE(julianday(json_extract(?, '$.updated_at')), 0), julianday(?), julianday('now')) + 1.0 / 86400000.0) WHERE plugin_id = ? AND storage_key = ?")
        .bind(&conflict.local_payload).bind(&remote.updated_at).bind(&remote.plugin_id).bind(&remote.storage_key)
        .execute(db).await.map_err(|e| e.to_string())?;
    crate::sync::repository::SyncRepository::new(db.clone())
        .remember_plugin_remote(&remote)
        .await
        .map_err(|e| e.to_string())?;
    sqlx::query("INSERT INTO sync_changes (entity_type, entity_id, operation, plugin_id, source) VALUES ('plugin_data', ?, 'upsert', ?, 'sync_resolution')")
        .bind(&remote.storage_key).bind(&remote.plugin_id).execute(db).await.map_err(|e| e.to_string())?;
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
                VALUES ('clipboard_item', ?, 'update', ?, 'sync_resolution', datetime('now'), NULL)
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

async fn remote_item_location(
    db: &Db,
    remote: &RemoteClipboardItem,
) -> Result<(Option<i64>, Option<i64>), String> {
    let tab_id = if remote.is_trashed {
        Some(trash_tab_id(db).await.map_err(|e| e.to_string())?)
    } else {
        local_id_for_remote_tab(remote.tab_key.as_deref(), remote.tab_name.as_deref(), db)
            .await
            .map_err(|e| e.to_string())?
    };
    let deleted_from = if remote.is_trashed {
        local_id_for_remote_tab(
            remote.deleted_from_tab_key.as_deref(),
            remote.deleted_from_tab_name.as_deref(),
            db,
        )
        .await
        .map_err(|e| e.to_string())?
    } else {
        None
    };
    Ok((tab_id, deleted_from))
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

    let (tab_id, deleted_from_tab_id) = remote_item_location(db, remote).await?;
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
            tab_id = ?,
            deleted_at = ?,
            deleted_from_tab_id = ?,
            created_at = ?,
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
    .bind(tab_id)
    .bind(&remote.deleted_at)
    .bind(deleted_from_tab_id)
    .bind(&remote.created_at)
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
    let (tab_id, deleted_from_tab_id) = remote_item_location(db, remote).await?;
    let tags = tags_to_db(&remote.tags);
    let metadata = serde_json::to_string(&remote.metadata).map_err(|e| e.to_string())?;
    let result = sqlx::query(
        r#"
        INSERT INTO clipboard_items
            (type, content, content_hash, metadata, tags, tab_id, is_sensitive, is_pinned, deleted_at, deleted_from_tab_id, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
        "#,
    )
    .bind(&remote.item_type)
    .bind(remote.content.clone().unwrap_or_default())
    .bind(&remote.content_hash)
    .bind(metadata)
    .bind(tags)
    .bind(tab_id)
    .bind(if remote.is_sensitive { 1 } else { 0 })
    .bind(if remote.is_pinned { 1 } else { 0 })
    .bind(&remote.deleted_at)
    .bind(deleted_from_tab_id)
    .bind(&remote.created_at)
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
