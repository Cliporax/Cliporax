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

/// Get the plugin directory path
pub fn get_plugin_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))?;

    Ok(data_dir.join(PLUGIN_DIR_NAME))
}
