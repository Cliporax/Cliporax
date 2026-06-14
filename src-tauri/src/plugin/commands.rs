//! Plugin IPC commands

use crate::plugin::lifecycle::registry::{LoadResult, PluginDetail, PluginInfo, PluginRegistry};
use crate::plugin::permission::definition::builtin_permissions_list;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Get all discovered plugins
#[tauri::command]
pub async fn plugin_get_all(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<Vec<PluginInfo>, String> {
    log::info!("[Command] plugin_get_all called");

    let reg = registry.read().await;
    let plugins = reg.get_all();

    log::info!(
        "[Command] plugin_get_all returned {} plugins",
        plugins.len()
    );
    Ok(plugins)
}

/// Get plugin detail
#[tauri::command]
pub async fn plugin_get_detail(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    plugin_id: String,
) -> Result<PluginDetail, String> {
    log::info!("[Command] plugin_get_detail called: {}", plugin_id);

    let reg = registry.read().await;
    reg.get_detail(&plugin_id)
        .ok_or_else(|| format!("Plugin not found: {}", plugin_id))
}

/// Load a plugin
#[tauri::command]
pub async fn plugin_load(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    plugin_id: String,
) -> Result<LoadResult, String> {
    log::info!("[Command] plugin_load called: {}", plugin_id);

    let mut reg = registry.write().await;
    reg.load(&plugin_id).await.map_err(|e| {
        log::error!("[Command] plugin_load failed: {}", e);
        e.to_string()
    })
}

/// Activate a plugin
#[tauri::command]
pub async fn plugin_activate(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    plugin_id: String,
) -> Result<(), String> {
    log::info!("[Command] plugin_activate called: {}", plugin_id);

    let mut reg = registry.write().await;
    reg.activate(&plugin_id).await.map_err(|e| {
        log::error!("[Command] plugin_activate failed: {}", e);
        e.to_string()
    })
}

/// Deactivate a plugin
#[tauri::command]
pub async fn plugin_deactivate(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    plugin_id: String,
) -> Result<(), String> {
    log::info!("[Command] plugin_deactivate called: {}", plugin_id);

    let mut reg = registry.write().await;
    reg.deactivate(&plugin_id).await.map_err(|e| {
        log::error!("[Command] plugin_deactivate failed: {}", e);
        e.to_string()
    })
}

/// Unload a plugin
#[tauri::command]
pub async fn plugin_unload(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    plugin_id: String,
) -> Result<(), String> {
    log::info!("[Command] plugin_unload called: {}", plugin_id);

    let mut reg = registry.write().await;
    reg.unload(&plugin_id).await.map_err(|e| {
        log::error!("[Command] plugin_unload failed: {}", e);
        e.to_string()
    })
}

/// Grant permission to a plugin
#[tauri::command]
pub async fn plugin_grant_permission(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    plugin_id: String,
    permission: String,
) -> Result<(), String> {
    log::info!(
        "[Command] plugin_grant_permission called: {} -> {}",
        plugin_id,
        permission
    );

    let mut reg = registry.write().await;
    reg.grant_permission(&plugin_id, &permission).map_err(|e| {
        log::error!("[Command] plugin_grant_permission failed: {}", e);
        e.to_string()
    })
}

/// Get plugin configuration
#[tauri::command]
pub async fn plugin_get_config(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    plugin_id: String,
) -> Result<serde_json::Value, String> {
    log::info!("[Command] plugin_get_config called: {}", plugin_id);

    let reg = registry.read().await;
    Ok(reg
        .get_config(&plugin_id)
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

/// Update plugin configuration
#[tauri::command]
pub async fn plugin_update_config(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    plugin_id: String,
    config: serde_json::Value,
) -> Result<(), String> {
    log::info!("[Command] plugin_update_config called: {}", plugin_id);

    let mut reg = registry.write().await;
    reg.update_config(&plugin_id, config).map_err(|e| {
        log::error!("[Command] plugin_update_config failed: {}", e);
        e.to_string()
    })
}

/// Get all permission definitions
#[tauri::command]
pub async fn plugin_get_permission_definitions(
) -> Result<Vec<crate::plugin::permission::definition::Permission>, String> {
    log::info!("[Command] plugin_get_permission_definitions called");
    Ok(builtin_permissions_list())
}

/// Discover plugins
#[tauri::command]
pub async fn plugin_discover(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<Vec<String>, String> {
    log::info!("[Command] plugin_discover called");

    let mut reg = registry.write().await;
    reg.discover().await.map_err(|e| {
        log::error!("[Command] plugin_discover failed: {}", e);
        e.to_string()
    })
}

/// Get plugin state
#[tauri::command]
pub async fn plugin_get_state(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    plugin_id: String,
) -> Result<crate::plugin::lifecycle::state::PluginState, String> {
    log::info!("[Command] plugin_get_state called: {}", plugin_id);

    let reg = registry.read().await;
    reg.get_state(&plugin_id)
        .cloned()
        .ok_or_else(|| format!("Plugin not found: {}", plugin_id))
}

/// Read plugin script content
#[tauri::command]
pub async fn plugin_read_script(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    plugin_id: String,
) -> Result<String, String> {
    log::info!("[Command] plugin_read_script called: {}", plugin_id);

    let reg = registry.read().await;
    let plugin_path = reg
        .get_plugin_path(&plugin_id)
        .ok_or_else(|| format!("Plugin not found: {}", plugin_id))?;

    let main_file = reg
        .get_manifest(&plugin_id)
        .map(|m| m.main.clone())
        .unwrap_or_else(|| "main.js".to_string());

    let main_path = Path::new(&main_file);
    if main_path.is_absolute()
        || main_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("Plugin main path must stay inside the plugin directory".to_string());
    }

    let plugin_root = plugin_path
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize plugin directory: {}", e))?;
    let script_path = plugin_path.join(main_path);
    let script_path = script_path
        .canonicalize()
        .map_err(|e| format!("Failed to canonicalize plugin script: {}", e))?;
    if !script_path.starts_with(&plugin_root) {
        return Err("Plugin script path escapes the plugin directory".to_string());
    }
    log::info!("[Command] Reading script from: {:?}", script_path);

    tokio::fs::read_to_string(&script_path)
        .await
        .map_err(|e| format!("Failed to read plugin script: {}", e))
}
