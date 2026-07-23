//! Window control commands

use crate::settings::SettingsManager;
use crate::state::WindowState;
use crate::window_utils;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PluginShortcutPayload {
    plugin_id: String,
    shortcut: String,
}

/// Window action enum for unified command API
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WindowAction {
    Minimize,
    Maximize,
    Restore,
    Close,
    Show,
    Hide,
    Toggle,
    SetAlwaysOnTop { always_on_top: bool },
    StartDragging,
    EndDragging,
    SetContextMenuOpen { open: bool },
    HideAndPaste,
    PasteToPrevious,
    RestoreFocus,
    SimulatePaste,
}

/// Unified window command - consolidates multiple window operations
#[tauri::command]
pub async fn window_command(
    window: tauri::Window,
    action: WindowAction,
    state: tauri::State<'_, Arc<WindowState>>,
) -> Result<(), String> {
    log::info!("[Command] window_command: {:?}", action);

    match action {
        WindowAction::Minimize => {
            if window.label() == "main" {
                window_utils::hide_main_window(window.app_handle())
            } else {
                window.minimize().map_err(|e| e.to_string())
            }
        }

        WindowAction::Maximize => if window.is_maximized().unwrap_or(false) {
            window.unmaximize()
        } else {
            window.maximize()
        }
        .map_err(|e| e.to_string()),

        WindowAction::Restore => window.unmaximize().map_err(|e| e.to_string()),

        WindowAction::Close => {
            let label = window.label().to_string();
            if label == "main" {
                window.hide().map_err(|e| e.to_string())
            } else {
                window.close().map_err(|e| e.to_string())
            }
        }

        WindowAction::Show => {
            window_utils::ensure_main_window_min_size(&window)?;
            window.show().map_err(|e| e.to_string())?;
            window.set_focus().map_err(|e| e.to_string())
        }

        WindowAction::Hide => {
            let is_pinned = state.pinned.load(Ordering::SeqCst);
            window.hide().map_err(|e| e.to_string())?;
            if !is_pinned {
                window.set_always_on_top(false).map_err(|e| e.to_string())?;
            }
            Ok(())
        }

        WindowAction::Toggle => {
            if window.is_visible().unwrap_or(false) {
                state.set_shortcut_in_progress(true);
                let is_pinned = state.pinned.load(Ordering::SeqCst);
                window.hide().map_err(|e| e.to_string())?;
                if !is_pinned {
                    window.set_always_on_top(false).map_err(|e| e.to_string())?;
                }
                state.set_shortcut_in_progress(false);
            } else {
                crate::window_utils::record_focused_window();
                state.set_shortcut_in_progress(true);
                window.unminimize().map_err(|e| e.to_string())?;
                window_utils::ensure_main_window_min_size(&window)?;
                window.show().map_err(|e| e.to_string())?;
                window.set_always_on_top(true).map_err(|e| e.to_string())?;
                window.set_focus().map_err(|e| e.to_string())?;
                // shortcut_in_progress will be reset by Focused(true) event handler
            }
            Ok(())
        }

        WindowAction::SetAlwaysOnTop { always_on_top } => {
            state.set_pinned(always_on_top);
            window
                .set_always_on_top(always_on_top)
                .map_err(|e| e.to_string())
        }

        WindowAction::StartDragging => {
            state.set_dragging(true);
            window.start_dragging().map_err(|e| e.to_string())
        }

        WindowAction::EndDragging => {
            state.set_dragging(false);
            Ok(())
        }

        WindowAction::SetContextMenuOpen { open } => {
            state.set_context_menu_open(open);
            Ok(())
        }

        WindowAction::HideAndPaste => {
            let is_pinned = state.pinned.load(Ordering::SeqCst);
            window.hide().map_err(|e| e.to_string())?;
            if !is_pinned {
                window.set_always_on_top(false).map_err(|e| e.to_string())?;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            window_utils::restore_and_paste().map_err(|e| e.to_string())
        }

        WindowAction::PasteToPrevious => {
            state.set_paste_in_progress(true);
            if let Err(e) = window_utils::restore_focused_window() {
                state.set_paste_in_progress(false);
                return Err(e.to_string());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
            if let Err(e) = window_utils::simulate_paste() {
                state.set_paste_in_progress(false);
                return Err(e.to_string());
            }
            state.set_paste_in_progress(false);
            Ok(())
        }

        WindowAction::RestoreFocus => {
            window_utils::restore_focused_window().map_err(|e| e.to_string())
        }

        WindowAction::SimulatePaste => window_utils::simulate_paste().map_err(|e| e.to_string()),
    }
}

/// Check if window is maximized (v2 API)
#[tauri::command]
pub async fn window_is_maximized_v2(window: tauri::Window) -> Result<bool, String> {
    window.is_maximized().map_err(|e| e.to_string())
}

// Window control commands
#[tauri::command]
pub async fn window_minimize(window: tauri::Window) -> Result<(), String> {
    log::info!("[Command] window_minimize called");
    let result = if window.label() == "main" {
        window_utils::hide_main_window(window.app_handle())
    } else {
        window.minimize().map_err(|e| e.to_string())
    };

    result.map_err(|e| {
        log::error!("[Command] window_minimize failed: {}", e);
        e
    })
}

#[tauri::command]
pub async fn window_maximize(window: tauri::Window) -> Result<(), String> {
    log::info!("[Command] window_maximize called");
    if window.is_maximized().map_err(|e| e.to_string())? {
        window.unmaximize().map_err(|e| e.to_string())
    } else {
        window.maximize().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn window_close(window: tauri::Window) -> Result<(), String> {
    log::info!("[Command] window_close called");
    let label = window.label().to_string();
    if label == "main" {
        // For the main window, hide instead of close
        window.hide().map_err(|e| e.to_string())
    } else {
        // For other windows (settings, etc.), actually close them
        window.close().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn window_show(window: tauri::Window) -> Result<(), String> {
    log::info!("[Command] window_show called");
    window_utils::ensure_main_window_min_size(&window)?;
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn window_hide(window: tauri::Window) -> Result<(), String> {
    log::info!("[Command] window_hide called");
    window.hide().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn window_toggle(window: tauri::Window) -> Result<(), String> {
    log::info!("[Command] window_toggle called");
    if window.is_visible().map_err(|e| e.to_string())? {
        window.hide().map_err(|e| e.to_string())
    } else {
        window_utils::ensure_main_window_min_size(&window)?;
        window.show().map_err(|e| e.to_string())?;
        window.set_focus().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn window_is_maximized(window: tauri::Window) -> Result<bool, String> {
    log::debug!("[Command] window_is_maximized called");
    window.is_maximized().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn window_set_always_on_top(
    window: tauri::Window,
    #[allow(non_snake_case)] alwaysOnTop: bool,
    state: tauri::State<'_, Arc<WindowState>>,
) -> Result<(), String> {
    log::info!(
        "[Command] window_set_always_on_top called with: {}",
        alwaysOnTop
    );

    // Set the global flag that controls auto-hide behavior
    state.set_pinned(alwaysOnTop);
    log::info!("[Command] WINDOW_PINNED flag set to: {}", alwaysOnTop);

    window.set_always_on_top(alwaysOnTop).map_err(|e| {
        log::error!("[Command] window_set_always_on_top failed: {}", e);
        e.to_string()
    })
}

/// Notify backend that a context menu is open/closed to prevent auto-hide
#[tauri::command]
pub async fn window_set_context_menu_open(
    open: bool,
    state: tauri::State<'_, Arc<WindowState>>,
) -> Result<(), String> {
    log::debug!(
        "[Command] window_set_context_menu_open called with: {}",
        open
    );
    state.set_context_menu_open(open);
    Ok(())
}

/// Paste content to previous window while keeping Cliporax visible (pinned mode)
#[tauri::command]
pub async fn window_paste_to_previous(
    state: tauri::State<'_, Arc<WindowState>>,
) -> Result<(), String> {
    log::info!("[Command] window_paste_to_previous called");

    // Set flag to prevent auto-hide during paste operation
    state.set_paste_in_progress(true);

    // Step 1: Restore focus to previous window
    log::debug!("[Command] Restoring focus to previous window...");
    if let Err(e) = window_utils::restore_focused_window() {
        log::warn!("[Command] Failed to restore focus: {}", e);
        // Continue anyway - paste might still work
    }

    // Small delay to ensure focus is restored
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Step 2: Simulate paste
    log::debug!("[Command] Simulating paste...");
    if let Err(e) = window_utils::simulate_paste() {
        log::error!("[Command] Failed to simulate paste: {}", e);
        state.set_paste_in_progress(false);
        return Err(format!("Paste failed: {}", e));
    }

    // Small delay to ensure paste completes before clearing flag
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Clear flag
    state.set_paste_in_progress(false);

    log::info!("[Command] window_paste_to_previous completed successfully");
    Ok(())
}

// Shortcut commands
fn register_toggle_shortcut(app: &tauri::AppHandle, shortcut: Shortcut) -> Result<(), String> {
    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            let window_state = app
                .try_state::<Arc<WindowState>>()
                .map(|s| Arc::clone(s.inner()));

            match event.state {
                ShortcutState::Pressed => {
                    if let Some(state) = &window_state {
                        state.set_shortcut_in_progress(true);
                    }
                    log::debug!("[Command] Shortcut pressed, auto-hide disabled");
                    if let Err(e) = window_utils::show_or_hide_main_window(app) {
                        log::warn!("[Command] Failed to toggle main window: {}", e);
                    }
                }
                ShortcutState::Released => {
                    if let Some(state) = &window_state {
                        state.set_shortcut_in_progress(false);
                    }
                    log::debug!("[Command] Shortcut released, auto-hide re-enabled");
                }
            }
        })
        .map_err(|e| e.to_string())
}

fn parse_shortcut(shortcut: &str) -> Result<Shortcut, String> {
    shortcut
        .parse::<Shortcut>()
        .map_err(|e| format!("Failed to parse shortcut '{}': {:?}", shortcut, e))
}

fn current_toggle_shortcut(
    settings: &tauri::State<'_, std::sync::Mutex<SettingsManager>>,
) -> Option<String> {
    settings
        .lock()
        .map(|manager| manager.get().shortcut_toggle_window.clone())
        .map_err(|e| {
            log::warn!(
                "[Command] Failed to lock settings for shortcut lookup: {}",
                e
            );
        })
        .ok()
}

fn unregister_shortcut_if_registered(app: &tauri::AppHandle, shortcut_str: &str) {
    match parse_shortcut(shortcut_str) {
        Ok(shortcut) => {
            if app.global_shortcut().is_registered(shortcut) {
                if let Err(e) = app.global_shortcut().unregister(shortcut) {
                    log::warn!(
                        "[Command] Failed to unregister shortcut {}: {}",
                        shortcut_str,
                        e
                    );
                }
            }
        }
        Err(e) => {
            log::warn!(
                "[Command] Skipping invalid shortcut '{}': {}",
                shortcut_str,
                e
            );
        }
    }
}

#[tauri::command]
pub async fn shortcut_update(
    app: tauri::AppHandle,
    settings: tauri::State<'_, std::sync::Mutex<SettingsManager>>,
    old_shortcut: String,
    new_shortcut: String,
) -> Result<bool, String> {
    log::info!(
        "[Command] shortcut_update called - old: {}, new: {}",
        old_shortcut,
        new_shortcut
    );

    let new = parse_shortcut(&new_shortcut)?;
    let current_shortcut = current_toggle_shortcut(&settings);

    unregister_shortcut_if_registered(&app, &old_shortcut);
    if let Some(current_shortcut) = current_shortcut.as_deref() {
        if current_shortcut != old_shortcut {
            unregister_shortcut_if_registered(&app, current_shortcut);
        }
    }

    if let Err(e) = register_toggle_shortcut(&app, new) {
        log::error!("[Command] Failed to register new shortcut: {}", e);
        if let Some(current_shortcut) = current_shortcut.as_deref() {
            if let Ok(current) = parse_shortcut(current_shortcut) {
                if let Err(restore_error) = register_toggle_shortcut(&app, current) {
                    log::error!(
                        "[Command] Failed to restore current shortcut after update failure: {}",
                        restore_error
                    );
                }
            }
        } else if let Ok(old) = parse_shortcut(&old_shortcut) {
            if let Err(restore_error) = register_toggle_shortcut(&app, old) {
                log::error!(
                    "[Command] Failed to restore old shortcut after update failure: {}",
                    restore_error
                );
            }
        }
        return Err(format!("Failed to register new shortcut: {}", e));
    }

    log::info!("[Command] shortcut_update success: {}", new_shortcut);
    Ok(true)
}

/// Temporarily unregister the global toggle shortcut while recording a new shortcut
#[tauri::command]
pub async fn shortcut_pause(
    app: tauri::AppHandle,
    settings: tauri::State<'_, std::sync::Mutex<SettingsManager>>,
    shortcut_str: String,
) -> Result<bool, String> {
    log::info!(
        "[Command] shortcut_pause called for shortcut: {}",
        shortcut_str
    );

    unregister_shortcut_if_registered(&app, &shortcut_str);
    if let Some(current_shortcut) = current_toggle_shortcut(&settings) {
        if current_shortcut != shortcut_str {
            unregister_shortcut_if_registered(&app, &current_shortcut);
        }
    }

    log::info!("[Command] shortcut_pause success");
    Ok(true)
}

/// Resume (re-register) the global toggle shortcut after recording is complete
#[tauri::command]
pub async fn shortcut_resume(
    app: tauri::AppHandle,
    settings: tauri::State<'_, std::sync::Mutex<SettingsManager>>,
    shortcut_str: String,
) -> Result<bool, String> {
    log::info!(
        "[Command] shortcut_resume called for shortcut: {}",
        shortcut_str
    );

    let shortcut_to_resume = current_toggle_shortcut(&settings).unwrap_or(shortcut_str);
    let shortcut = parse_shortcut(&shortcut_to_resume)?;
    register_toggle_shortcut(&app, shortcut).map_err(|e| {
        log::error!("[Command] shortcut_resume failed to register: {}", e);
        e
    })?;

    log::info!(
        "[Command] shortcut_resume success: re-registered {}",
        shortcut_to_resume
    );
    Ok(true)
}

/// Register or update a plugin-owned global shortcut.
#[tauri::command]
pub async fn plugin_shortcut_update(
    app: tauri::AppHandle,
    plugin_id: String,
    old_shortcut: Option<String>,
    new_shortcut: String,
) -> Result<bool, String> {
    log::info!(
        "[Command] plugin_shortcut_update called - plugin: {}, old: {:?}, new: {}",
        plugin_id,
        old_shortcut,
        new_shortcut
    );

    if let Some(old_shortcut) = old_shortcut.filter(|s| !s.trim().is_empty()) {
        if let Ok(old) = parse_shortcut(&old_shortcut) {
            if let Err(e) = app.global_shortcut().unregister(old) {
                log::warn!(
                    "[Command] Failed to unregister old plugin shortcut {}: {}",
                    old_shortcut,
                    e
                );
            }
        }
    }

    let shortcut = parse_shortcut(&new_shortcut)?;
    let shortcut_for_event = new_shortcut.clone();
    let plugin_for_event = plugin_id.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                let payload = PluginShortcutPayload {
                    plugin_id: plugin_for_event.clone(),
                    shortcut: shortcut_for_event.clone(),
                };
                if let Err(e) = app.emit("plugin:shortcut", payload) {
                    log::warn!("[Command] Failed to emit plugin shortcut event: {}", e);
                }
            }
        })
        .map_err(|e| format!("Failed to register plugin shortcut: {}", e))?;

    log::info!(
        "[Command] plugin_shortcut_update success - plugin: {}, shortcut: {}",
        plugin_id,
        new_shortcut
    );
    Ok(true)
}

/// Unregister a plugin-owned global shortcut.
#[tauri::command]
pub async fn plugin_shortcut_unregister(
    app: tauri::AppHandle,
    plugin_id: String,
    shortcut: String,
) -> Result<bool, String> {
    log::info!(
        "[Command] plugin_shortcut_unregister called - plugin: {}, shortcut: {}",
        plugin_id,
        shortcut
    );

    let shortcut = parse_shortcut(&shortcut)?;
    app.global_shortcut()
        .unregister(shortcut)
        .map_err(|e| format!("Failed to unregister plugin shortcut: {}", e))?;

    Ok(true)
}

// Window focus and paste commands
#[tauri::command]
pub async fn window_restore_and_paste() -> Result<(), String> {
    log::info!("[Command] window_restore_and_paste called");
    window_utils::restore_and_paste().map_err(|e| {
        log::error!("[Command] window_restore_and_paste failed: {}", e);
        e.to_string()
    })
}

#[tauri::command]
pub async fn window_restore_focus() -> Result<(), String> {
    log::info!("[Command] window_restore_focus called");
    window_utils::restore_focused_window().map_err(|e| {
        log::error!("[Command] window_restore_focus failed: {}", e);
        e.to_string()
    })
}

#[tauri::command]
pub async fn window_simulate_paste() -> Result<(), String> {
    log::info!("[Command] window_simulate_paste called");
    window_utils::simulate_paste().map_err(|e| {
        log::error!("[Command] window_simulate_paste failed: {}", e);
        e.to_string()
    })
}

/// Combined command: hide window, restore focus to previous window, and simulate paste
#[tauri::command]
pub async fn window_hide_and_paste(window: tauri::Window) -> Result<(), String> {
    log::info!("[Command] window_hide_and_paste called");

    // 1. Hide the Cliporax window
    // Check if window is pinned before resetting always_on_top
    let is_pinned = window.is_always_on_top().unwrap_or(false);
    window.hide().map_err(|e| {
        log::error!("[Command] Failed to hide window: {}", e);
        e.to_string()
    })?;
    if !is_pinned {
        window.set_always_on_top(false).map_err(|e| {
            log::error!("[Command] Failed to set always_on_top: {}", e);
            e.to_string()
        })?;
    } else {
        log::info!("[Command] Window is pinned, preserving always_on_top state");
    }

    // 2. Small delay to ensure window is fully hidden
    std::thread::sleep(std::time::Duration::from_millis(50));

    // 3. Restore focus and paste
    window_utils::restore_and_paste().map_err(|e| {
        log::error!("[Command] window_hide_and_paste failed: {}", e);
        e.to_string()
    })?;

    log::info!("[Command] window_hide_and_paste completed successfully");
    Ok(())
}

// Window dragging commands for Windows compatibility
#[tauri::command]
pub async fn window_start_dragging(
    window: tauri::Window,
    state: tauri::State<'_, Arc<WindowState>>,
) -> Result<(), String> {
    log::debug!("[Command] window_start_dragging called");
    state.set_dragging(true);
    window.start_dragging().map_err(|e| {
        log::error!("[Command] window_start_dragging failed: {}", e);
        e.to_string()
    })?;
    // Note: The flag will be reset by the WindowEvent::Moved handler
    // or by window_end_dragging when mouse is released
    Ok(())
}

#[tauri::command]
pub async fn window_end_dragging(state: tauri::State<'_, Arc<WindowState>>) -> Result<(), String> {
    log::debug!("[Command] window_end_dragging called");
    state.set_dragging(false);
    Ok(())
}

/// Open settings in a new window
#[tauri::command]
pub async fn window_open_settings(app: tauri::AppHandle) -> Result<(), String> {
    log::info!("[Command] window_open_settings called");

    // Check if settings window already exists
    if let Some(existing_window) = app.get_webview_window("settings") {
        log::info!("[Command] Settings window already exists, focusing it");
        let _ = existing_window.set_always_on_top(true);
        existing_window.show().map_err(|e| e.to_string())?;
        existing_window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }

    // Create new settings window
    let builder = tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("/settings".into()),
    )
    .title("Cliporax Settings")
    .inner_size(1040.0, 760.0)
    // Minimum size must not be smaller than PluginDetailModal minimum (500x400)
    .min_inner_size(760.0, 560.0)
    .resizable(true)
    .decorations(false)
    .transparent(false)
    .center();

    #[cfg(target_os = "macos")]
    let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay);

    let settings_window = builder.build().map_err(|e| {
        log::error!("[Command] Failed to create settings window: {}", e);
        e.to_string()
    })?;

    // Ensure settings window is on top of the main window and focused
    settings_window.set_always_on_top(true).map_err(|e| {
        log::warn!(
            "[Command] Failed to set settings window always on top: {}",
            e
        );
        e.to_string()
    })?;
    settings_window.set_focus().map_err(|e| {
        log::warn!("[Command] Failed to focus settings window: {}", e);
        e.to_string()
    })?;

    log::info!("[Command] Settings window created successfully");
    Ok(())
}
