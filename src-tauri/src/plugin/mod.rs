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
pub mod permission;
pub mod types;

// Re-export main types for convenience
pub use lifecycle::registry::PluginRegistry;
pub use lifecycle::state::PluginState;
pub use manifest::{PermissionRequest, PluginManifest, PluginType};
pub use permission::checker::PermissionChecker;
pub use types::{ClipPacket, ClipPacketType, PacketMetadata, PipelineTrace};

use std::path::PathBuf;
use tauri::Manager;

/// Default plugin directory name
pub const PLUGIN_DIR_NAME: &str = "plugins";
pub const BUILTIN_PLUGIN_RESOURCE_DIR: &str = "builtin-plugins";

/// Get the plugin directory path
pub fn get_plugin_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    Ok(data_dir.join(PLUGIN_DIR_NAME))
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

        let Some(plugin_name) = source_dir.file_name() else {
            continue;
        };
        let target_dir = plugin_dir.join(plugin_name);
        tokio::fs::create_dir_all(&target_dir)
            .await
            .map_err(|e| format!("Failed to create seeded plugin directory: {}", e))?;
        tokio::fs::copy(&manifest_path, target_dir.join("manifest.json"))
            .await
            .map_err(|e| format!("Failed to seed bundled plugin manifest: {}", e))?;
        tokio::fs::copy(&main_path, target_dir.join("main.js"))
            .await
            .map_err(|e| format!("Failed to seed bundled plugin script: {}", e))?;
        seeded += 1;
    }

    log::info!("[Plugin] Seeded {} bundled plugins", seeded);
    Ok(())
}
