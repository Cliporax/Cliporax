pub mod async_log_writer;
pub mod clipboard;
pub mod commands;
pub mod db;
pub mod dev_log;
pub mod file_sync;
pub mod plugin;
pub mod portable;
pub mod settings;
pub mod state;
pub mod sync;
pub mod trace_context;
pub mod traced_mutex;
pub mod types;
pub mod window_utils;

// Re-export sync types for use in main.rs
pub use sync::commands::*;
pub use sync::secrets::SecretStore;

pub use dev_log::{install_logger, DevLogFileBackend};
pub use file_sync::FileSyncService;

pub use clipboard::ClipboardMonitor;
pub use db::{init_database, Db};
pub use settings::{init_settings, AppSettings, SettingsManager, SettingsState};
pub use state::WindowState;
pub use window_utils::{
    force_focus_window, hide_main_window, record_focused_window, restore_and_paste,
    restore_focused_window, show_main_window, show_or_hide_main_window, simulate_paste,
};
