// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use cliporax_lib::{
    commands::*,
    db::init_database,
    dev_log::{install_logger, DevLogFileBackend},
    file_sync::{commands::*, FileSyncService},
    init_settings,
    plugin::{
        commands::*, get_plugin_dir, lifecycle::registry::PluginRegistry, market::*,
        seed_builtin_plugins,
    },
    show_main_window, show_or_hide_main_window,
    state::WindowState,
    sync::commands::*,
    sync::engine::SyncEngine,
    sync::repository::SyncRepository,
    sync::secrets::SecretStore,
    sync::service::SyncService,
    ClipboardMonitor,
};
pub mod types;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// Track the last time window lost focus for debouncing
static LAST_FOCUS_LOST_TIME: AtomicU64 = AtomicU64::new(0);
static WINDOW_STATE_SAVE_SEQ: AtomicU64 = AtomicU64::new(0);
const MAIN_TRAY_ID: &str = "main";
const TRAY_ICON_DEFAULT: &str = "icons/tray-32.png";
const TRAY_ICON_ACTIVE: &str = "icons/tray-active-32.png";
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder},
    Manager, WindowEvent,
};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

fn schedule_window_state_save(app_handle: tauri::AppHandle) {
    let save_seq = WINDOW_STATE_SAVE_SEQ.fetch_add(1, Ordering::SeqCst) + 1;

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(700)).await;
        if WINDOW_STATE_SAVE_SEQ.load(Ordering::SeqCst) != save_seq {
            return;
        }

        if let Err(e) = app_handle.save_window_state(StateFlags::SIZE | StateFlags::POSITION) {
            log::warn!("[Main] WARN: Failed to save debounced window state: {}", e);
        }
    });
}

fn load_tray_icon(
    app: &tauri::AppHandle,
    active: bool,
) -> Result<tauri::image::Image<'static>, Box<dyn std::error::Error>> {
    let icon_path = app.path().resolve(
        if active {
            TRAY_ICON_ACTIVE
        } else {
            TRAY_ICON_DEFAULT
        },
        tauri::path::BaseDirectory::Resource,
    )?;

    Ok(tauri::image::Image::from_path(icon_path)?)
}

fn set_tray_icon(app: &tauri::AppHandle, active: bool) {
    let Some(tray) = app.tray_by_id(MAIN_TRAY_ID) else {
        log::warn!("[Main] WARN: Failed to find tray icon for state update");
        return;
    };

    match load_tray_icon(app, active) {
        Ok(icon) => {
            if let Err(e) = tray.set_icon(Some(icon)) {
                log::warn!("[Main] WARN: Failed to update tray icon: {}", e);
            }
        }
        Err(e) => {
            log::warn!("[Main] WARN: Failed to load tray icon: {}", e);
        }
    }
}

fn main() {
    // Use env_logger in production mode
    #[cfg(not(debug_assertions))]
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(StateFlags::SIZE | StateFlags::POSITION)
                .build(),
        )
        .setup(|app| {
            // Initialize development log files in development mode only
            #[cfg(debug_assertions)]
            {
                let backend = match DevLogFileBackend::init(app.handle()) {
                    Ok(b) => std::sync::Arc::new(b),
                    Err(e) => {
                        eprintln!("[Main] ERROR: Failed to init dev log: {}", e);
                        panic!("Failed to init dev log: {}", e);
                    }
                };
                // Install the custom logger to write backend logs to files
                if let Err(e) = install_logger(backend.clone()) {
                    eprintln!("[Main] ERROR: Failed to install dev logger: {}", e);
                }
                // Register it in Tauri state for IPC commands
                app.manage(backend);
            }

            log::info!("App is ready, initializing...");

            // Initialize settings manager (blocking, needed for shortcut registration)
            log::info!("Initializing settings...");
            let settings_manager = init_settings(app.handle()).map_err(|e| {
                log::error!("[Main] ERROR: Failed to initialize settings: {}", e);
                Box::<dyn std::error::Error>::from(e.to_string())
            })?;
            let shortcut_toggle_window = settings_manager.get().shortcut_toggle_window.clone();
            app.manage(std::sync::Mutex::new(settings_manager));
            log::info!("Settings initialized, shortcut: {}", shortcut_toggle_window);

            let app_handle = app.handle().clone();
            tauri::async_runtime::block_on(async {
                // Initialize database
                log::info!("Initializing database...");
                let db = init_database(&app_handle).await.map_err(|e| {
                    log::error!("[Main] ERROR: Failed to initialize database: {}", e);
                    Box::<dyn std::error::Error>::from(e.to_string())
                })?;
                log::info!("Database initialized successfully");

                // Initialize clipboard monitor (without outer Mutex since internals use Arc<Mutex<>>)
                log::info!("Initializing clipboard monitor...");
                let clipboard_monitor = ClipboardMonitor::new().map_err(|e| {
                    log::error!("[Main] ERROR: Failed to create clipboard monitor: {}", e);
                    Box::<dyn std::error::Error>::from(e.to_string())
                })?;

                // Start clipboard monitoring in background
                let db_clone = db.clone();
                let monitor_handle = app_handle.clone();
                let monitor_clone = clipboard_monitor.clone(); // ClipboardMonitor is already cloneable since it uses Arc internally
                tokio::spawn(async move {
                    if let Err(e) = monitor_clone
                        .start_monitoring(db_clone, monitor_handle)
                        .await
                    {
                        log::error!("Clipboard monitoring failed: {}", e);
                    }
                });

                // Store state (wrapped in Arc for management)
                app_handle.manage(db.clone());
                let clipboard_monitor = Arc::new(clipboard_monitor);
                app_handle.manage(clipboard_monitor.clone());

                // Initialize and register secret store for sync credentials
                let secret_store = Arc::new(SecretStore::new(db.clone()));
                let sync_repository = Arc::new(SyncRepository::new(db.clone()));
                let sync_engine = Arc::new(SyncEngine::new(
                    sync_repository.clone(),
                    secret_store.clone(),
                ));
                app_handle.manage((*secret_store).clone());
                app_handle.manage(sync_engine.clone());
                log::info!("[Main] SecretStore registered for sync credentials");

                // Initialize WindowState for centralized window state management
                let window_state = Arc::new(WindowState::new());
                app_handle.manage(window_state.clone());
                log::info!("WindowState initialized");

                // Initialize plugin registry
                log::info!("Initializing plugin system...");
                let plugin_dir = get_plugin_dir(&app_handle).map_err(|e| {
                    log::error!("[Main] ERROR: Failed to get plugin directory: {}", e);
                    Box::<dyn std::error::Error>::from(e.to_string())
                })?;
                seed_builtin_plugins(&app_handle, &plugin_dir).await.map_err(|e| {
                    log::error!("[Main] ERROR: Failed to seed bundled plugins: {}", e);
                    Box::<dyn std::error::Error>::from(e.to_string())
                })?;
                let plugin_registry = Arc::new(RwLock::new(PluginRegistry::new(plugin_dir)));

                // Discover plugins
                {
                    let mut reg = plugin_registry.write().await;
                    if let Err(e) = reg.discover().await {
                        log::error!("Plugin discovery failed: {}", e);
                    }
                    if let Err(e) = reg.auto_activate_builtin().await {
                        log::error!("Builtin plugin activation failed: {}", e);
                    }
                }
                let sync_service = Arc::new(SyncService::new(
                    sync_repository.clone(),
                    secret_store.clone(),
                    sync_engine.clone(),
                    plugin_registry.clone(),
                    app_handle.clone(),
                ));
                app_handle.manage(plugin_registry);
                app_handle.manage(sync_service.clone());
                tokio::spawn(sync_service.run_scheduler_loop());

                let file_sync_service = Arc::new(
                    FileSyncService::new(
                        db.clone(),
                        sync_repository,
                        sync_engine,
                        secret_store,
                        clipboard_monitor,
                        app_handle.clone(),
                    )
                    .map_err(|error| {
                        log::error!("[Main] Failed to initialize File Sync: {}", error);
                        Box::<dyn std::error::Error>::from(error)
                    })?,
                );
                app_handle.manage(file_sync_service.clone());
                tokio::spawn(async move {
                    file_sync_service.resume_pending().await;
                });
                log::info!("Plugin system initialized");

                Ok::<(), Box<dyn std::error::Error>>(())
            })?;

            // Create system tray
            create_tray(app)?;

            // Register global shortcut using settings from JSON file
            log::info!("Registering global shortcut...");
            if let Err(e) = register_shortcut(app.handle(), &shortcut_toggle_window) {
                log::error!("Failed to register shortcut: {}", e);
            }

            log::info!("Setup complete");

            // Explicitly set focus and always_on_top on startup for Linux reliability
            // On Linux, set_focus() may not work if called too early in setup.
            // We delay the focus call slightly to ensure the window is fully visible.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_always_on_top(true);
                let window_clone = window.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    if let Err(e) = window_clone.set_focus() {
                        log::warn!("Failed to set focus on startup: {}", e);
                    } else {
                        log::info!("Window focus set on startup (delayed)");
                    }
                });
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    let label = window.label().to_string();

                    if label == "main" {
                        // Close all preview and settings windows first
                        let windows = window.app_handle().webview_windows();
                        for (win_label, win) in windows.iter() {
                            if win_label.starts_with("preview-") || win_label == "settings" {
                                let _ = win.close();
                                log::info!("Closed window: {}", win_label);
                            }
                        }
                        // Save window state before hiding
                        let _ = window.app_handle().save_window_state(StateFlags::all());
                        // Hide window instead of closing
                        if let Err(e) = window.hide() {
                            log::warn!("[Main] WARN: Failed to hide main window on close: {}", e);
                        }
                        api.prevent_close();
                        log::info!("Window hidden (not closed)");
                    } else if label == "settings" {
                        // Settings window: allow it to close naturally
                        log::info!("Settings window close requested, allowing close");
                    } else if label.starts_with("preview-") {
                        // Preview windows: allow them to close naturally
                        log::info!("Preview window close requested, allowing close");
                    } else {
                        // Other windows: hide instead of close
                        let _ = window.app_handle().save_window_state(StateFlags::all());
                        if let Err(e) = window.hide() {
                            log::warn!("[Main] WARN: Failed to hide window on close: {}", e);
                        }
                        api.prevent_close();
                        log::info!("Window hidden (not closed)");
                    }
                }
                WindowEvent::Resized(_) => {
                    // Mark window as resizing to prevent auto-hide
                    if let Some(state) = window.app_handle().try_state::<Arc<WindowState>>() {
                        state.set_resizing(true);
                    }
                    schedule_window_state_save(window.app_handle().clone());
                    // Reset the resizing flag after a delay
                    let state = window.app_handle().try_state::<Arc<WindowState>>().map(|s| Arc::clone(s.inner()));
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        if let Some(s) = state {
                            s.set_resizing(false);
                        }
                    });
                }
                WindowEvent::Moved(_) => {
                    // Mark window as dragging to prevent auto-hide
                    if let Some(state) = window.app_handle().try_state::<Arc<WindowState>>() {
                        state.set_dragging(true);
                    }
                    schedule_window_state_save(window.app_handle().clone());
                    // Reset the dragging flag after a delay
                    let state = window.app_handle().try_state::<Arc<WindowState>>().map(|s| Arc::clone(s.inner()));
                    tauri::async_runtime::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                        if let Some(s) = state {
                            s.set_dragging(false);
                        }
                    });
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    // Ignore scale factor changes
                }
                WindowEvent::Focused(focused) => {
                    // Skip auto-hide logic for preview windows and settings window
                    if window.label().starts_with("preview-") || window.label() == "settings" {
                        log::debug!("{} window focus event, skipping auto-hide", window.label());
                        return;
                    }

                    set_tray_icon(window.app_handle(), *focused);

                    // Get WindowState for checking flags
                    let window_state = window.app_handle().try_state::<Arc<WindowState>>();

                    if !focused {
                        // Record the time when focus was lost
                        let now = Instant::now().elapsed().as_millis() as u64;
                        LAST_FOCUS_LOST_TIME.store(now, Ordering::SeqCst);

                        // Reset shortcut_in_progress on focus loss
                        // This handles the case where window was shown but never gained focus
                        // (common on Linux with some window managers)
                        if let Some(state) = &window_state {
                            state.set_shortcut_in_progress(false);
                        }

                        // Check if window is pinned by user - this completely disables auto-hide
                        if let Some(state) = &window_state {
                            if state.pinned.load(Ordering::SeqCst) {
                                log::info!("Window is pinned by user, ignoring blur event completely");
                                return;
                            }
                        }

                        // Only auto-hide if not during paste operation
                        if let Some(state) = &window_state {
                            if state.paste_in_progress.load(Ordering::SeqCst) {
                                log::debug!(
                                    "[Main] DEBUG: Ignoring blur event during paste operation"
                                );
                                return;
                            }
                        }

                        // Only auto-hide if context menu is not open
                        if let Some(state) = &window_state {
                            if state.context_menu_open.load(Ordering::SeqCst) {
                                log::debug!(
                                    "[Main] DEBUG: Ignoring blur event during context menu open"
                                );
                                return;
                            }
                        }

                        // macOS special handling: add a delay to avoid false triggers from Mission Control, Space switching, and similar actions
                        #[cfg(target_os = "macos")]
                        let focus_lost_delay = Duration::from_millis(300);
                        #[cfg(not(target_os = "macos"))]
                        let focus_lost_delay = Duration::from_millis(100);

                        // Delay the hide operation to allow drag/resize events to set their flags
                        let window_clone = window.clone();
                        let app_handle = window.app_handle().clone();
                        let state_clone = window_state.map(|s| Arc::clone(s.inner()));
                        tauri::async_runtime::spawn(async move {
                            tokio::time::sleep(focus_lost_delay).await;

                            // Check if we're still not focused after the delay
                            if let Ok(true) = window_clone.is_focused() {
                                log::debug!("Window regained focus, not hiding");
                                return;
                            }

                            // Check if any app window (preview or settings) has focus
                            let windows = app_handle.webview_windows();
                            for (label, win) in windows.iter() {
                                if label.starts_with("preview-") || label == "settings" {
                                    if let Ok(true) = win.is_focused() {
                                        log::debug!(
                                            "[Main] DEBUG: Window '{}' has focus, not hiding main window",
                                            label
                                        );
                                        return;
                                    }
                                }
                            }

                            // Check if window is being dragged
                            if let Some(state) = &state_clone {
                                if state.dragging.load(Ordering::SeqCst) {
                                    log::debug!(
                                        "[Main] DEBUG: Ignoring blur event during window dragging"
                                    );
                                    return;
                                }
                            }

                            // Check if window is being resized
                            if let Some(state) = &state_clone {
                                if state.resizing.load(Ordering::SeqCst) {
                                    log::debug!(
                                        "[Main] DEBUG: Ignoring blur event during window resizing"
                                    );
                                    return;
                                }
                            }

                            log::info!("Window lost focus (no preview window has focus), hiding window");
                            let _ = window_clone.hide();
                            let _ = window_clone.set_always_on_top(false);
                        });
                    } else {
                        // Window gained focus - reset flags
                        if let Some(state) = window_state {
                            state.set_resizing(false);
                            state.set_dragging(false);
                            state.set_context_menu_open(false);
                            // Reset shortcut_in_progress on focus gain:
                            // If window gained focus, the shortcut show operation is complete.
                            // This is a safety mechanism in case ShortcutState::Released
                            // event doesn't fire reliably on some Linux environments.
                            state.set_shortcut_in_progress(false);
                        }
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            // Dev log commands (only in debug mode)
            #[cfg(debug_assertions)]
            dev_log_write,
            app_ready,
            // Tab commands
            tabs_get_all,
            tabs_create,
            tabs_delete,
            tabs_rename,
            // Clipboard commands
            clipboard_get_by_tab,
            clipboard_get_latest,
            clipboard_create,
            clipboard_delete,
            clipboard_toggle_pin,
            clipboard_move_to_top,
            clipboard_search,
            clipboard_update_tags,
            clipboard_update_content,
            clipboard_clear_sensitive,
            clipboard_copy,
            clipboard_write_text_and_create,
            clipboard_get_total_count,
            clipboard_get_item_at_index,
            clipboard_delete_by_index_range,
            clipboard_delete_by_ids,
            clipboard_get_all_types,
            clipboard_move_item_to_position,
            clipboard_move_to_tab,
            clipboard_copy_to_tab,
            clipboard_move_to_tab_batch,
            clipboard_copy_to_tab_batch,
            // Window commands
            window_command,
            window_is_maximized_v2,
            window_open_settings,
            window_minimize,
            window_maximize,
            window_close,
            window_show,
            window_hide,
            window_toggle,
            window_is_maximized,
            window_set_always_on_top,
            window_restore_and_paste,
            window_restore_focus,
            window_simulate_paste,
            window_hide_and_paste,
            window_paste_to_previous,
            window_set_context_menu_open,
            window_start_dragging,
            window_end_dragging,
            // Shortcut commands
            shortcut_update,
            shortcut_pause,
            shortcut_resume,
            plugin_shortcut_update,
            plugin_shortcut_unregister,
            // Settings commands
            settings_get_all,
            settings_update,
            settings_update_toggle_window_shortcut,
            // System utility commands
            qrscanner_capture_region,
            // Test commands
            test_insert_batch,
            test_clear_all,
            test_debug_tabs,
            test_delete_empty_tabs,
            // Plugin commands
            plugin_get_all,
            plugin_get_detail,
            plugin_load,
            plugin_activate,
            plugin_deactivate,
            plugin_unload,
            plugin_grant_permission,
            plugin_get_config,
            plugin_update_config,
            plugin_get_permission_definitions,
            plugin_discover,
            plugin_get_state,
            plugin_read_script,
            plugin_market_get_sources,
            plugin_market_refresh,
            plugin_market_get_plugins,
            plugin_market_install,
            plugin_market_uninstall,
            plugin_market_get_install_status,
            // Preview window commands
            preview_create_window,
            preview_get_data,
            preview_close_window,
            preview_close_all,
            // Sync commands (Cloud Sync plugin)
            sync_profile_list,
            sync_profile_get,
            sync_profile_update,
            sync_profile_delete,
            sync_profile_pause,
            sync_profile_resume,
            sync_secret_set,
            sync_secret_delete,
            sync_profile_unlock,
            sync_profile_lock,
            sync_test_connection,
            sync_trust_sftp_host_key,
            sync_run_now,
            sync_cancel_run,
            sync_get_status,
            sync_get_last_report,
            sync_get_conflicts,
            sync_resolve_conflict,
            sync_get_tab_options,
            sync_get_plugin_options,
            sync_get_log_entries,
            // File Sync market plugin commands
            file_sync_get_config,
            file_sync_set_profile,
            file_sync_profile_options,
            file_sync_list,
            file_sync_enqueue_clipboard_item,
            file_sync_clipboard_item_status,
            file_sync_confirm,
            file_sync_retry,
            file_sync_cancel,
            file_sync_refresh,
            file_sync_copy,
            file_sync_delete,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn create_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    log::info!("Creating system tray");

    let show_i = MenuItem::with_id(app, "show", "Show Cliporax", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &quit_i])?;

    let icon = load_tray_icon(app.handle(), false)?;

    let _tray = TrayIconBuilder::with_id(MAIN_TRAY_ID)
        .icon(icon)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Err(e) = show_main_window(app) {
                    log::warn!("[Main] WARN: Failed to show main window from tray: {}", e);
                }
            }
            "quit" => {
                let _ = app.save_window_state(StateFlags::all());
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            let app = tray.app_handle();
            match event {
                // Double click to show window
                tauri::tray::TrayIconEvent::DoubleClick { .. } => {
                    log::info!("Tray icon double-clicked, showing window");
                    if let Err(e) = show_main_window(app) {
                        log::warn!("[Main] WARN: Failed to show main window from tray: {}", e);
                    }
                }
                // Single click to toggle window visibility
                // Note: On KDE and some Linux DEs, double-click may be interpreted as two single clicks
                tauri::tray::TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } => {
                    log::info!("Tray icon clicked, toggling window");
                    if let Err(e) = show_or_hide_main_window(app) {
                        log::warn!("[Main] WARN: Failed to toggle main window: {}", e);
                    }
                }
                // Handle Enter/Return key on tray icon (some Linux DEs use this)
                tauri::tray::TrayIconEvent::Enter { .. } => {
                    log::info!("Tray icon enter event received");
                }
                // Handle any other tray events for debugging
                _ => {
                    log::debug!("[Main] DEBUG: Unhandled tray icon event: {:?}", event);
                }
            }
        })
        .build(app)?;

    log::info!("[Main] INFO: System tray created");
    Ok(())
}

fn register_shortcut(
    app: &tauri::AppHandle,
    shortcut_str: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let shortcut: Shortcut = shortcut_str.parse()?;
    log::info!("[Main] Registering global shortcut: {}", shortcut_str);

    app.global_shortcut()
        .on_shortcut(shortcut, |app, _shortcut, event| {
            // Get WindowState from app handle
            let window_state = app
                .try_state::<Arc<WindowState>>()
                .map(|s| Arc::clone(s.inner()));

            match event.state {
                ShortcutState::Pressed => {
                    // Disable auto-hide when shortcut is pressed
                    if let Some(state) = &window_state {
                        state.set_shortcut_in_progress(true);
                    }
                    log::debug!("[Main] DEBUG: Shortcut pressed, auto-hide disabled");

                    if let Err(e) = show_or_hide_main_window(app) {
                        log::warn!("[Main] WARN: Failed to toggle main window: {}", e);
                    }
                }
                ShortcutState::Released => {
                    // Re-enable auto-hide when shortcut is released
                    if let Some(state) = &window_state {
                        state.set_shortcut_in_progress(false);
                    }
                    log::debug!("[Main] DEBUG: Shortcut released, auto-hide re-enabled");
                }
            }
        })?;

    log::info!("[Main] INFO: Global shortcut registered: {}", shortcut_str);
    Ok(())
}
