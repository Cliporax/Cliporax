use crate::file_sync::models::{
    FileSyncClipboardItemStatus, FileSyncConfig, FileSyncEnqueueResult, FileSyncEntry,
    FileSyncProfileOption,
};
use crate::file_sync::FileSyncService;
use crate::plugin::lifecycle::registry::PluginRegistry;
use crate::plugin::lifecycle::state::PluginState;
use std::sync::Arc;
use tauri::State;
use tokio::sync::RwLock;

const FILE_SYNC_PLUGIN_ID: &str = "com.cliporax.file-sync";

async fn authorize(registry: &Arc<RwLock<PluginRegistry>>, plugin_id: &str) -> Result<(), String> {
    if plugin_id != FILE_SYNC_PLUGIN_ID {
        return Err("File Sync API is only available to the official File Sync plugin".to_string());
    }
    let registry = registry.read().await;
    let detail = registry
        .get_detail(plugin_id)
        .ok_or_else(|| "File Sync plugin is not installed".to_string())?;
    if detail.state != PluginState::Active {
        return Err("File Sync plugin is not active".to_string());
    }
    for permission in [
        "ui:extension",
        "network:sync",
        "system:file-read",
        "system:file-write",
        "clipboard:write",
    ] {
        if !detail
            .granted_permissions
            .iter()
            .any(|granted| granted == permission)
        {
            return Err(format!(
                "File Sync permission is not granted: {}",
                permission
            ));
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn file_sync_get_config(
    plugin_id: String,
    service: State<'_, Arc<FileSyncService>>,
    registry: State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<FileSyncConfig, String> {
    authorize(registry.inner(), &plugin_id).await?;
    service.config().await
}

#[tauri::command]
pub async fn file_sync_set_profile(
    plugin_id: String,
    profile_id: String,
    service: State<'_, Arc<FileSyncService>>,
    registry: State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<(), String> {
    authorize(registry.inner(), &plugin_id).await?;
    service.set_default_profile(&profile_id).await
}

#[tauri::command]
pub async fn file_sync_profile_options(
    plugin_id: String,
    service: State<'_, Arc<FileSyncService>>,
    registry: State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<Vec<FileSyncProfileOption>, String> {
    authorize(registry.inner(), &plugin_id).await?;
    service.profile_options().await
}

#[tauri::command]
pub async fn file_sync_list(
    plugin_id: String,
    profile_id: Option<String>,
    service: State<'_, Arc<FileSyncService>>,
    registry: State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<Vec<FileSyncEntry>, String> {
    authorize(registry.inner(), &plugin_id).await?;
    service.list_entries(profile_id.as_deref()).await
}

#[tauri::command]
pub async fn file_sync_enqueue_clipboard_item(
    plugin_id: String,
    item_id: i64,
    service: State<'_, Arc<FileSyncService>>,
    registry: State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<FileSyncEnqueueResult, String> {
    authorize(registry.inner(), &plugin_id).await?;
    service.inner().enqueue_clipboard_item(item_id).await
}

#[tauri::command]
pub async fn file_sync_clipboard_item_status(
    plugin_id: String,
    item_id: i64,
    service: State<'_, Arc<FileSyncService>>,
    registry: State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<FileSyncClipboardItemStatus, String> {
    authorize(registry.inner(), &plugin_id).await?;
    service.clipboard_item_status(item_id).await
}

#[tauri::command]
pub async fn file_sync_confirm(
    plugin_id: String,
    entry_id: String,
    service: State<'_, Arc<FileSyncService>>,
    registry: State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<(), String> {
    authorize(registry.inner(), &plugin_id).await?;
    service.inner().confirm(&entry_id).await
}

#[tauri::command]
pub async fn file_sync_retry(
    plugin_id: String,
    entry_id: String,
    service: State<'_, Arc<FileSyncService>>,
    registry: State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<(), String> {
    authorize(registry.inner(), &plugin_id).await?;
    service.inner().retry(&entry_id).await
}

#[tauri::command]
pub async fn file_sync_cancel(
    plugin_id: String,
    entry_id: String,
    service: State<'_, Arc<FileSyncService>>,
    registry: State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<(), String> {
    authorize(registry.inner(), &plugin_id).await?;
    service.cancel(&entry_id).await
}

#[tauri::command]
pub async fn file_sync_refresh(
    plugin_id: String,
    profile_id: String,
    service: State<'_, Arc<FileSyncService>>,
    registry: State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<(), String> {
    authorize(registry.inner(), &plugin_id).await?;
    service.refresh(&profile_id).await
}

#[tauri::command]
pub async fn file_sync_copy(
    plugin_id: String,
    entry_ids: Vec<String>,
    service: State<'_, Arc<FileSyncService>>,
    registry: State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<(), String> {
    authorize(registry.inner(), &plugin_id).await?;
    service.copy_entries(entry_ids).await
}

#[tauri::command]
pub async fn file_sync_delete(
    plugin_id: String,
    entry_id: String,
    service: State<'_, Arc<FileSyncService>>,
    registry: State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<(), String> {
    authorize(registry.inner(), &plugin_id).await?;
    service.delete_entry(&entry_id).await
}
