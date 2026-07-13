#[cfg(target_os = "linux")]
use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use tauri::Manager;

use crate::state::WindowState;

lazy_static::lazy_static! {
    /// Stores the previously focused window info before showing Cliporax
    pub static ref PREVIOUS_WINDOW: Mutex<Option<WindowInfo>> = Mutex::new(None);
}

#[derive(Clone, Debug)]
pub struct WindowInfo {
    #[cfg(target_os = "linux")]
    pub window_id: String,
    #[cfg(target_os = "macos")]
    pub process_id: u64,
    #[cfg(target_os = "macos")]
    pub app_name: String,
    #[cfg(target_os = "windows")]
    pub hwnd: isize,
}

pub fn show_main_window(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;

    record_focused_window();

    if let Some(state) = app
        .try_state::<Arc<WindowState>>()
        .map(|state| Arc::clone(state.inner()))
    {
        state.set_shortcut_in_progress(true);
    }

    if let Err(e) = window.unminimize() {
        log::warn!("[WindowUtils] Failed to unminimize main window: {}", e);
    }
    if let Err(e) = window.show() {
        return Err(format!("Failed to show main window: {}", e));
    }
    if let Err(e) = window.set_always_on_top(true) {
        log::warn!("[WindowUtils] Failed to set always_on_top: {}", e);
    }

    let window_clone = window.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        if let Err(e) = window_clone.set_focus() {
            log::warn!("[WindowUtils] Failed to focus main window: {}", e);
        }
        #[cfg(target_os = "linux")]
        {
            if let Err(e) = force_focus_window() {
                log::warn!("[WindowUtils] force_focus_window failed: {}", e);
            }
        }
    });

    Ok(())
}

pub fn hide_main_window(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;
    let is_pinned = app
        .try_state::<Arc<WindowState>>()
        .map(|state| state.pinned.load(Ordering::SeqCst))
        .unwrap_or(false);

    if let Err(e) = window.hide() {
        return Err(format!("Failed to hide main window: {}", e));
    }
    if !is_pinned {
        if let Err(e) = window.set_always_on_top(false) {
            log::warn!("[WindowUtils] Failed to clear always_on_top: {}", e);
        }
    }

    Ok(())
}

pub fn show_or_hide_main_window(app: &tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "Main window not found".to_string())?;

    match window.is_visible() {
        Ok(true) => hide_main_window(app),
        Ok(false) => show_main_window(app),
        Err(e) => {
            log::warn!(
                "[WindowUtils] Failed to determine main window visibility: {}",
                e
            );
            show_main_window(app)
        }
    }
}

/// Force focus to the Cliporax window on Linux/X11 using xdotool.
/// This is more reliable than Tauri's set_focus() on KDE and other X11 window managers.
#[cfg(target_os = "linux")]
pub fn force_focus_window() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    log::info!("[WindowUtils] Force focusing Cliporax window on Linux");

    let current_pid = std::process::id().to_string();
    let class_candidates = ["cliporax", "Cliporax", "com.cliporax.app"];

    for window_class in class_candidates {
        let search_output = Command::new("xdotool")
            .args(["search", "--onlyvisible", "--class", window_class])
            .output()?;

        if !search_output.status.success() {
            log::debug!(
                "[WindowUtils] No visible Linux window found for class: {}",
                window_class
            );
            continue;
        }

        let window_ids: Vec<String> = String::from_utf8_lossy(&search_output.stdout)
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        for window_id in window_ids {
            let pid_output = Command::new("xdotool")
                .args(["getwindowpid", &window_id])
                .output()?;

            if !pid_output.status.success() {
                log::debug!(
                    "[WindowUtils] Failed to read pid for Linux window: {}",
                    window_id
                );
                continue;
            }

            let window_pid = String::from_utf8_lossy(&pid_output.stdout)
                .trim()
                .to_string();
            if window_pid != current_pid {
                log::debug!(
                    "[WindowUtils] Skipping Linux window {} owned by pid {}",
                    window_id,
                    window_pid
                );
                continue;
            }

            let activate_output = Command::new("xdotool")
                .args(["windowactivate", "--sync", &window_id])
                .output()?;

            if activate_output.status.success() {
                log::info!(
                    "[WindowUtils] xdotool activated Cliporax window: {}",
                    window_id
                );
                return Ok(());
            }

            let stderr = String::from_utf8_lossy(&activate_output.stderr);
            log::debug!(
                "[WindowUtils] Failed to activate Linux window {}: {}",
                window_id,
                stderr
            );
        }
    }

    Err("Failed to focus Cliporax window on Linux".into())
}

#[cfg(not(target_os = "linux"))]
pub fn force_focus_window() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Ok(())
}

/// Record the currently focused window before showing Cliporax
pub fn record_focused_window() {
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = Command::new("xdotool").arg("getactivewindow").output() {
            if output.status.success() {
                let window_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !window_id.is_empty() {
                    log::info!(
                        "[WindowUtils] Recorded focused window (Linux): {}",
                        window_id
                    );
                    if let Ok(mut guard) = PREVIOUS_WINDOW.lock() {
                        *guard = Some(WindowInfo { window_id });
                    }
                }
            } else {
                log::warn!("[WindowUtils] Failed to get active window on Linux");
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        match active_win_pos_rs::get_active_window() {
            Ok(active_window) if active_window.process_id != u64::from(std::process::id()) => {
                log::info!(
                    "[WindowUtils] Recorded focused application (macOS): {} (pid {})",
                    active_window.app_name,
                    active_window.process_id
                );
                if let Ok(mut guard) = PREVIOUS_WINDOW.lock() {
                    *guard = Some(WindowInfo {
                        process_id: active_window.process_id,
                        app_name: active_window.app_name,
                    });
                }
            }
            Ok(_) => {
                log::debug!(
                    "[WindowUtils] Skipping Cliporax while recording focused application (macOS)"
                );
            }
            Err(_) => {
                log::warn!("[WindowUtils] Failed to get active application on macOS");
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        unsafe {
            let hwnd = GetForegroundWindow();
            if !hwnd.0.is_null() {
                log::info!(
                    "[WindowUtils] Recorded focused window (Windows): {:?}",
                    hwnd.0
                );
                if let Ok(mut guard) = PREVIOUS_WINDOW.lock() {
                    *guard = Some(WindowInfo {
                        hwnd: hwnd.0 as isize,
                    });
                }
            }
        }
    }
}

/// Restore focus to the previously recorded window
pub fn restore_focused_window() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let window_info = {
        let guard = PREVIOUS_WINDOW
            .lock()
            .map_err(|e| format!("Failed to lock PREVIOUS_WINDOW: {}", e))?;
        guard.clone()
    };

    match window_info {
        Some(info) => {
            #[cfg(target_os = "linux")]
            {
                log::info!(
                    "[WindowUtils] Restoring focus to window (Linux): {}",
                    info.window_id
                );
                let output = Command::new("xdotool")
                    .args(["windowactivate", "--sync", &info.window_id])
                    .output()?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    log::error!("[WindowUtils] Failed to restore window focus: {}", stderr);
                    return Err(format!("xdotool windowactivate failed: {}", stderr).into());
                }
                log::info!("[WindowUtils] Focus restored successfully");
            }

            #[cfg(target_os = "macos")]
            {
                use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};

                log::info!(
                    "[WindowUtils] Restoring focus to application (macOS): {} (pid {})",
                    info.app_name,
                    info.process_id
                );

                let process_id = i32::try_from(info.process_id)
                    .map_err(|_| format!("Invalid macOS process id: {}", info.process_id))?;
                let missing_application_error = format!(
                    "Previously focused macOS application is no longer running: {} (pid {})",
                    info.app_name, info.process_id
                );
                let application =
                    NSRunningApplication::runningApplicationWithProcessIdentifier(process_id)
                        .ok_or(missing_application_error)?;

                if application.isHidden() && !application.unhide() {
                    return Err(format!(
                        "Failed to unhide macOS application: {} (pid {})",
                        info.app_name, info.process_id
                    )
                    .into());
                }

                // Target the exact process that owned the insertion point. `open -a` only
                // addresses an application by name and returns before activation completes,
                // so Cmd+V can otherwise be delivered while Cliporax still owns focus.
                #[allow(deprecated)]
                let activation_options = NSApplicationActivationOptions::ActivateIgnoringOtherApps;
                if !application.activateWithOptions(activation_options) {
                    return Err(format!(
                        "Failed to request macOS application activation: {} (pid {})",
                        info.app_name, info.process_id
                    )
                    .into());
                }

                let activation_deadline =
                    std::time::Instant::now() + std::time::Duration::from_millis(750);
                while !application.isActive() && std::time::Instant::now() < activation_deadline {
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }

                if !application.isActive() {
                    return Err(format!(
                        "Timed out waiting for macOS application focus: {} (pid {})",
                        info.app_name, info.process_id
                    )
                    .into());
                }
                log::info!("[WindowUtils] Focus restored successfully (macOS)");
            }

            #[cfg(target_os = "windows")]
            {
                use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
                unsafe {
                    let hwnd = windows::Win32::Foundation::HWND(info.hwnd as *mut std::ffi::c_void);
                    log::info!(
                        "[WindowUtils] Restoring focus to window (Windows): {:?}",
                        info.hwnd
                    );
                    SetForegroundWindow(hwnd);
                    log::info!("[WindowUtils] Focus restored successfully");
                }
            }

            Ok(())
        }
        None => {
            log::warn!("[WindowUtils] No previous window recorded to restore");
            Err("No previous window recorded".into())
        }
    }
}

/// Simulate Ctrl+V (or Cmd+V on macOS) to paste clipboard content
pub fn simulate_paste() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    #[cfg(target_os = "linux")]
    {
        log::info!("[WindowUtils] Simulating paste (Ctrl+V) on Linux");
        // Small delay to ensure window is focused
        std::thread::sleep(std::time::Duration::from_millis(50));

        let output = Command::new("xdotool")
            .args(["key", "--clearmodifiers", "ctrl+v"])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::error!("[WindowUtils] Failed to simulate paste: {}", stderr);
            return Err(format!("xdotool key failed: {}", stderr).into());
        }
        log::info!("[WindowUtils] Paste simulated successfully");
    }

    #[cfg(target_os = "macos")]
    {
        use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
        use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

        log::info!("[WindowUtils] Simulating paste (Cmd+V) on macOS");
        std::thread::sleep(std::time::Duration::from_millis(100));

        let source = CGEventSource::new(CGEventSourceStateID::CombinedSessionState)
            .map_err(|_| "Failed to create macOS keyboard event source")?;
        // 0x09 is the layout-independent macOS virtual key code for the V key.
        let key_down = CGEvent::new_keyboard_event(source.clone(), 0x09, true)
            .map_err(|_| "Failed to create macOS paste key-down event")?;
        let key_up = CGEvent::new_keyboard_event(source, 0x09, false)
            .map_err(|_| "Failed to create macOS paste key-up event")?;
        key_down.set_flags(CGEventFlags::CGEventFlagCommand);
        key_up.set_flags(CGEventFlags::CGEventFlagCommand);
        key_down.post(CGEventTapLocation::Session);
        key_up.post(CGEventTapLocation::Session);

        log::info!("[WindowUtils] Paste simulated successfully (macOS)");
    }

    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VK_CONTROL, VK_V,
        };

        log::info!("[WindowUtils] Simulating paste (Ctrl+V) on Windows");
        // Small delay to ensure window is focused
        std::thread::sleep(std::time::Duration::from_millis(50));

        unsafe {
            let mut inputs: [INPUT; 4] = std::mem::zeroed();

            // Press Ctrl
            inputs[0].r#type = INPUT_KEYBOARD;
            inputs[0].Anonymous.ki = KEYBDINPUT {
                wVk: VK_CONTROL,
                ..Default::default()
            };

            // Press V
            inputs[1].r#type = INPUT_KEYBOARD;
            inputs[1].Anonymous.ki = KEYBDINPUT {
                wVk: VK_V,
                ..Default::default()
            };

            // Release V
            inputs[2].r#type = INPUT_KEYBOARD;
            inputs[2].Anonymous.ki = KEYBDINPUT {
                wVk: VK_V,
                dwFlags: KEYEVENTF_KEYUP,
                ..Default::default()
            };

            // Release Ctrl
            inputs[3].r#type = INPUT_KEYBOARD;
            inputs[3].Anonymous.ki = KEYBDINPUT {
                wVk: VK_CONTROL,
                dwFlags: KEYEVENTF_KEYUP,
                ..Default::default()
            };

            let sent = SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            if sent != 4 {
                log::error!("[WindowUtils] SendInput returned {}, expected 4", sent);
                return Err("Failed to send all input events".into());
            }
            log::info!("[WindowUtils] Paste simulated successfully");
        }
    }

    Ok(())
}

/// Restore focus and paste in one operation
pub fn restore_and_paste() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    restore_focused_window()?;
    simulate_paste()?;
    Ok(())
}
