//! Settings management commands

use crate::settings::{AppSettings, SettingsManager};
use tauri::Emitter;

/// Get all settings
#[tauri::command]
pub async fn settings_get_all(
    settings: tauri::State<'_, std::sync::Mutex<SettingsManager>>,
) -> Result<AppSettings, String> {
    log::info!("[Command] settings_get_all called");
    let settings_clone = settings
        .lock()
        .map_err(|e| {
            log::error!("[Command] settings_get_all failed to lock settings: {}", e);
            "Failed to lock settings".to_string()
        })?
        .get()
        .clone();
    log::info!("[Command] settings_get_all returned settings");
    Ok(settings_clone)
}

/// Update settings
/// Use spawn_blocking to keep file I/O from blocking the Tokio async runtime
/// Use tokio::sync::Mutex instead of std::sync::Mutex to avoid holding locks across .await
#[tauri::command]
pub async fn settings_update(
    settings: tauri::State<'_, std::sync::Mutex<SettingsManager>>,
    app_handle: tauri::AppHandle,
    new_settings: AppSettings,
) -> Result<(), String> {
    log::info!(
        "[Command] settings_update called, line_height: {}",
        new_settings.line_height
    );

    // Update settings and trigger the file write while holding the lock for as little time as possible
    let (theme_changed, updated_settings) = {
        let mut manager = settings.lock().map_err(|e| {
            log::error!("[Command] settings_update failed to lock settings: {}", e);
            "Failed to lock settings".to_string()
        })?;

        let old_theme = manager.get().theme.clone();
        let theme_changed = old_theme != new_settings.theme;

        manager
            .update(|s| {
                s.theme = new_settings.theme.clone();
                s.max_items = new_settings.max_items;
                s.max_images = new_settings.max_images;
                s.line_height = new_settings.line_height.clone();
                s.auto_start = new_settings.auto_start;
                s.auto_hide = new_settings.auto_hide;
                s.shortcut_toggle_window = new_settings.shortcut_toggle_window.clone();
            })
            .map_err(|e| {
                log::error!("[Command] settings_update failed to save: {}", e);
                e.to_string()
            })?;

        log::info!("[Command] settings_update success");
        (theme_changed, manager.get().clone())
    }; // The lock is released here

    // Broadcast the event without holding the lock
    let _ = app_handle.emit("settings:changed", &updated_settings);
    log::info!("[Command] Emitted settings:changed event");

    if theme_changed {
        log::info!(
            "[Command] Emitted theme:changed event: {}",
            &updated_settings.theme
        );
        let _ = app_handle.emit("theme:changed", &updated_settings.theme);
    }

    Ok(())
}

/// Update toggle window shortcut
#[tauri::command]
pub async fn settings_update_toggle_window_shortcut(
    settings: tauri::State<'_, std::sync::Mutex<SettingsManager>>,
    app_handle: tauri::AppHandle,
    shortcut: String,
) -> Result<(), String> {
    log::info!(
        "[Command] settings_update_toggle_window_shortcut called: {}",
        shortcut
    );

    let updated_settings = {
        let mut manager = settings.lock().map_err(|e| {
            log::error!(
                "[Command] settings_update_toggle_window_shortcut failed to lock settings: {}",
                e
            );
            "Failed to lock settings".to_string()
        })?;

        manager
            .update(|s| {
                s.shortcut_toggle_window = shortcut;
            })
            .map_err(|e| {
                log::error!(
                    "[Command] settings_update_toggle_window_shortcut failed to save: {}",
                    e
                );
                e.to_string()
            })?;

        log::info!("[Command] settings_update_toggle_window_shortcut success");
        manager.get().clone()
    }; // The lock is released here

    let _ = app_handle.emit("settings:changed", &updated_settings);
    Ok(())
}
