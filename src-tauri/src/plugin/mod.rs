//! Plugin System Module
//!
//! This module provides a comprehensive plugin system for Cliporax, supporting:
//! - Plugin lifecycle management (discover, load, activate, deactivate, unload)
//! - Permission-based security model
//! - Data pipeline for clipboard content transformation
//! - Extension points for UI customization

pub mod commands;
pub mod lifecycle;
pub mod manifest;
pub mod market;
pub mod permission;
pub mod types;

// Re-export main types for convenience
pub use lifecycle::registry::PluginRegistry;
pub use lifecycle::state::PluginState;
pub use manifest::{PermissionRequest, PluginManifest, PluginType};
pub use permission::checker::PermissionChecker;
pub use types::{ClipPacket, ClipPacketType, PacketMetadata, PipelineTrace};

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use tauri::Manager;

/// Default plugin directory name
pub const PLUGIN_DIR_NAME: &str = "plugins";
pub const BUILTIN_PLUGIN_RESOURCE_DIR: &str = "builtin-plugins";
const BUILTIN_PLUGIN_IDS: &[&str] = &["com.cliporax.cloud-sync"];

/// Get the plugin directory path
pub fn get_plugin_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = crate::portable::app_data_dir(app_handle)?;
    Ok(data_dir.join(PLUGIN_DIR_NAME))
}

async fn copy_if_changed(source: &Path, target: PathBuf) -> Result<bool, String> {
    let source_meta = tokio::fs::metadata(source)
        .await
        .map_err(|e| format!("Failed to stat bundled plugin file: {}", e))?;

    if let Ok(target_meta) = tokio::fs::metadata(&target).await {
        if target_meta.len() == source_meta.len() {
            return Ok(false);
        }
    }

    tokio::fs::copy(source, target)
        .await
        .map_err(|e| format!("Failed to seed bundled plugin file: {}", e))?;
    Ok(true)
}

async fn clear_stale_builtin_flags(
    plugin_dir: &PathBuf,
    bundled_builtin_ids: &HashSet<String>,
) -> Result<usize, String> {
    let mut entries = tokio::fs::read_dir(plugin_dir)
        .await
        .map_err(|e| format!("Failed to read runtime plugin directory: {}", e))?;
    let mut updated = 0usize;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("Failed to read runtime plugin entry: {}", e))?
    {
        let plugin_path = entry.path();
        if !plugin_path.is_dir() {
            continue;
        }

        let manifest_path = plugin_path.join("manifest.json");
        let Ok(content) = tokio::fs::read_to_string(&manifest_path).await else {
            continue;
        };
        let Ok(mut manifest) = serde_json::from_str::<serde_json::Value>(&content) else {
            continue;
        };

        let Some(plugin_id) = manifest
            .get("id")
            .and_then(|value| value.as_str())
            .map(str::to_string)
        else {
            continue;
        };

        if bundled_builtin_ids.contains(&plugin_id) {
            continue;
        }

        let was_builtin = manifest
            .get("isBuiltin")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
            || manifest
                .get("is_builtin")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);

        if !was_builtin {
            continue;
        }

        if let Some(object) = manifest.as_object_mut() {
            object.remove("isBuiltin");
            object.remove("is_builtin");
        }

        let next = serde_json::to_string_pretty(&manifest)
            .map_err(|e| format!("Failed to serialize runtime plugin manifest: {}", e))?;
        tokio::fs::write(&manifest_path, format!("{}\n", next))
            .await
            .map_err(|e| format!("Failed to update runtime plugin manifest: {}", e))?;
        updated += 1;
    }

    Ok(updated)
}

/// Copy bundled plugins into the runtime plugin directory.
///
/// Packaged apps can only read bundled resources, while the plugin registry
/// expects a writable app-data plugin directory for state and user plugins.
pub async fn seed_builtin_plugins(
    app_handle: &tauri::AppHandle,
    plugin_dir: &PathBuf,
) -> Result<(), String> {
    let resource_dir = app_handle
        .path()
        .resolve(
            BUILTIN_PLUGIN_RESOURCE_DIR,
            tauri::path::BaseDirectory::Resource,
        )
        .map_err(|e| format!("Failed to resolve bundled plugin directory: {}", e))?;

    if !resource_dir.exists() {
        log::info!(
            "[Plugin] No bundled plugin resource directory found at {:?}",
            resource_dir
        );
        return Ok(());
    }

    tokio::fs::create_dir_all(plugin_dir)
        .await
        .map_err(|e| format!("Failed to create plugin directory: {}", e))?;

    let mut entries = tokio::fs::read_dir(&resource_dir)
        .await
        .map_err(|e| format!("Failed to read bundled plugin directory: {}", e))?;
    let mut seeded = 0usize;
    let mut bundled_builtin_ids = HashSet::new();

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| format!("Failed to read bundled plugin entry: {}", e))?
    {
        let source_dir = entry.path();
        if !source_dir.is_dir() {
            continue;
        }

        let manifest_path = source_dir.join("manifest.json");
        let main_path = source_dir.join("main.js");
        if !manifest_path.exists() || !main_path.exists() {
            log::warn!(
                "[Plugin] Skipping incomplete bundled plugin at {:?}",
                source_dir
            );
            continue;
        }

        let manifest_json = tokio::fs::read_to_string(&manifest_path)
            .await
            .map_err(|e| format!("Failed to read bundled plugin manifest: {}", e))?;
        let manifest = serde_json::from_str::<serde_json::Value>(&manifest_json)
            .map_err(|e| format!("Failed to parse bundled plugin manifest: {}", e))?;
        let plugin_id = manifest
            .get("id")
            .and_then(|value| value.as_str())
            .ok_or_else(|| "Bundled plugin manifest is missing id".to_string())?;
        let is_builtin = manifest
            .get("isBuiltin")
            .and_then(|value| value.as_bool())
            .unwrap_or(false)
            || manifest
                .get("is_builtin")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
        if is_builtin && BUILTIN_PLUGIN_IDS.contains(&plugin_id) {
            bundled_builtin_ids.insert(plugin_id.to_string());
        } else {
            log::info!(
                "[Plugin] Skipping bundled non-builtin plugin resource: {}",
                plugin_id
            );
            continue;
        }

        let Some(plugin_name) = source_dir.file_name() else {
            continue;
        };
        let target_dir = plugin_dir.join(plugin_name);
        tokio::fs::create_dir_all(&target_dir)
            .await
            .map_err(|e| format!("Failed to create seeded plugin directory: {}", e))?;
        let manifest_copied = copy_if_changed(&manifest_path, target_dir.join("manifest.json"))
            .await
            .map_err(|e| format!("Failed to seed bundled plugin manifest: {}", e))?;
        let main_copied = copy_if_changed(&main_path, target_dir.join("main.js"))
            .await
            .map_err(|e| format!("Failed to seed bundled plugin script: {}", e))?;
        if manifest_copied || main_copied {
            seeded += 1;
        }
    }

    let stale_cleared = clear_stale_builtin_flags(plugin_dir, &bundled_builtin_ids).await?;
    if stale_cleared > 0 {
        log::info!(
            "[Plugin] Cleared stale builtin flags from {} runtime plugins",
            stale_cleared
        );
    }

    log::info!("[Plugin] Seeded {} bundled plugins", seeded);
    Ok(())
}
