//! Plugin IPC commands

use crate::db::database::Db;
use crate::plugin::lifecycle::registry::{LoadResult, PluginDetail, PluginInfo, PluginRegistry};
use crate::plugin::permission::definition::builtin_permissions_list;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

const MAX_PLUGIN_ICON_BYTES: u64 = 512 * 1024;
const MAX_PLUGIN_STORAGE_KEY_BYTES: usize = 256;
const MAX_PLUGIN_STORAGE_VALUE_BYTES: usize = 1024 * 1024;

fn validate_plugin_storage_input(
    plugin_id: &str,
    key: &str,
    value: Option<&serde_json::Value>,
) -> Result<(), String> {
    if plugin_id.trim().is_empty()
        || plugin_id.len() > 200
        || key.trim().is_empty()
        || key.len() > MAX_PLUGIN_STORAGE_KEY_BYTES
    {
        return Err("Invalid plugin storage identifier".to_string());
    }
    if let Some(value) = value {
        if serde_json::to_vec(value)
            .map_err(|_| "Invalid plugin storage value".to_string())?
            .len()
            > MAX_PLUGIN_STORAGE_VALUE_BYTES
        {
            return Err("Plugin storage value exceeds 1 MiB".to_string());
        }
    }
    Ok(())
}

async fn ensure_storage_permission(
    registry: &Arc<RwLock<PluginRegistry>>,
    plugin_id: &str,
) -> Result<(), String> {
    let registry = registry.read().await;
    let manifest = registry
        .get_manifest(plugin_id)
        .ok_or_else(|| "Plugin not found".to_string())?;
    if manifest
        .permissions
        .iter()
        .any(|permission| permission.permission == "system:storage")
    {
        Ok(())
    } else {
        Err("Plugin lacks system:storage permission".to_string())
    }
}

#[tauri::command]
pub async fn plugin_storage_get(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    db: tauri::State<'_, Db>,
    plugin_id: String,
    key: String,
) -> Result<Option<serde_json::Value>, String> {
    validate_plugin_storage_input(&plugin_id, &key, None)?;
    ensure_storage_permission(registry.inner(), &plugin_id).await?;
    let value: Option<String> = sqlx::query_scalar(
        "SELECT value_json FROM plugin_sync_data WHERE plugin_id = ? AND storage_key = ?",
    )
    .bind(&plugin_id)
    .bind(&key)
    .fetch_optional(db.inner())
    .await
    .map_err(|e| e.to_string())?;
    value
        .map(|value| {
            serde_json::from_str(&value).map_err(|_| "Stored plugin data is invalid".to_string())
        })
        .transpose()
}

#[tauri::command]
pub async fn plugin_storage_set(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    db: tauri::State<'_, Db>,
    plugin_id: String,
    key: String,
    value: serde_json::Value,
) -> Result<(), String> {
    validate_plugin_storage_input(&plugin_id, &key, Some(&value))?;
    ensure_storage_permission(registry.inner(), &plugin_id).await?;
    let json =
        serde_json::to_string(&value).map_err(|_| "Invalid plugin storage value".to_string())?;
    sqlx::query("INSERT INTO plugin_sync_data (plugin_id, storage_key, value_json, updated_at) VALUES (?, ?, ?, datetime('now')) ON CONFLICT(plugin_id, storage_key) DO UPDATE SET value_json = excluded.value_json, updated_at = datetime('now')").bind(&plugin_id).bind(&key).bind(json).execute(db.inner()).await.map_err(|e| e.to_string())?;
    Ok(())
}

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

/// Read the icon declared by a plugin manifest as an image data URL.
#[tauri::command]
pub async fn plugin_read_icon(
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    plugin_id: String,
) -> Result<String, String> {
    let (plugin_path, icon_path) = {
        let reg = registry.read().await;
        let plugin_path = reg
            .get_plugin_path(&plugin_id)
            .cloned()
            .ok_or_else(|| format!("Plugin not found: {}", plugin_id))?;
        let icon_path = reg
            .get_manifest(&plugin_id)
            .and_then(|manifest| manifest.icon.clone())
            .ok_or_else(|| format!("Plugin {} does not declare an icon", plugin_id))?;
        (plugin_path, icon_path)
    };

    let icon_relative_path = Path::new(&icon_path);
    if icon_relative_path.is_absolute()
        || icon_relative_path
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("Plugin icon path must stay inside the plugin directory".to_string());
    }

    let mime_type = match icon_relative_path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        _ => return Err("Plugin icon must be SVG, PNG, JPEG, or WebP".to_string()),
    };

    let plugin_root = plugin_path
        .canonicalize()
        .map_err(|error| format!("Failed to canonicalize plugin directory: {}", error))?;
    let icon_file_path = plugin_path.join(icon_relative_path);
    let icon_file_path = icon_file_path
        .canonicalize()
        .map_err(|error| format!("Failed to canonicalize plugin icon: {}", error))?;
    if !icon_file_path.starts_with(&plugin_root) {
        return Err("Plugin icon path escapes the plugin directory".to_string());
    }

    let metadata = tokio::fs::metadata(&icon_file_path)
        .await
        .map_err(|error| format!("Failed to read plugin icon metadata: {}", error))?;
    if metadata.len() > MAX_PLUGIN_ICON_BYTES {
        return Err("Plugin icon exceeds the 512 KiB size limit".to_string());
    }

    let contents = tokio::fs::read(&icon_file_path)
        .await
        .map_err(|error| format!("Failed to read plugin icon: {}", error))?;
    Ok(format!(
        "data:{};base64,{}",
        mime_type,
        BASE64.encode(contents)
    ))
}
