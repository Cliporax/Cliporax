//! Settings management commands

use crate::settings::{
    AppSettings, SettingsManager, MAX_EXCLUDED_TEXT_PATTERNS, MAX_EXCLUDED_TEXT_PATTERN_LEN,
};
use regex::Regex;
use tauri::Emitter;

fn validate_excluded_text_patterns(patterns: &[String]) -> Result<(), String> {
    if patterns.len() > MAX_EXCLUDED_TEXT_PATTERNS {
        return Err(format!(
            "excluded_text_patterns cannot exceed {} patterns",
            MAX_EXCLUDED_TEXT_PATTERNS
        ));
    }
    for pattern in patterns {
        if pattern.trim().is_empty() || pattern.len() > MAX_EXCLUDED_TEXT_PATTERN_LEN {
            return Err(format!(
                "each excluded_text_patterns entry must be 1-{} bytes",
                MAX_EXCLUDED_TEXT_PATTERN_LEN
            ));
        }
        Regex::new(pattern).map_err(|error| format!("invalid excluded text pattern: {}", error))?;
    }
    Ok(())
}

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
    validate_excluded_text_patterns(&new_settings.excluded_text_patterns)?;

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
                s.excluded_text_patterns = new_settings.excluded_text_patterns.clone();
                s.show_item_index = new_settings.show_item_index;
                s.show_line_count = new_settings.show_line_count;
                s.show_source_host = new_settings.show_source_host;
                s.show_action_buttons = new_settings.show_action_buttons;
                s.show_edit_button = new_settings.show_edit_button;
                s.show_pin_button = new_settings.show_pin_button;
                s.show_plugin_action_buttons = new_settings.show_plugin_action_buttons;
                s.plugin_action_visibility = new_settings.plugin_action_visibility.clone();
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

#[cfg(test)]
mod tests {
    use super::validate_excluded_text_patterns;

    #[test]
    fn validates_excluded_text_patterns() {
        assert!(validate_excluded_text_patterns(&[r"(?i)^password:".to_string()]).is_ok());
        assert!(validate_excluded_text_patterns(&["[".to_string()]).is_err());
        assert!(validate_excluded_text_patterns(&[" ".to_string()]).is_err());
    }
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
