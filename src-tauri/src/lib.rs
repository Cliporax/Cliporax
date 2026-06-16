pub mod async_log_writer;
pub mod clipboard;
pub mod commands;
pub mod db;
pub mod dev_log;
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

pub use clipboard::ClipboardMonitor;
pub use db::{init_database, Db};
pub use settings::{init_settings, AppSettings, SettingsManager, SettingsState};
pub use state::WindowState;
pub use window_utils::{
    force_focus_window, hide_main_window, record_focused_window, restore_and_paste,
    restore_focused_window, show_main_window, show_or_hide_main_window, simulate_paste,
};

// Global variables have moved to WindowState; exports are kept for legacy compatibility
// TODO: Remove these exports after the migration is complete
pub use state::window_state::CONTEXT_MENU_OPEN;
pub use state::window_state::PASTE_IN_PROGRESS;
pub use state::window_state::SHORTCUT_IN_PROGRESS;
pub use state::window_state::WINDOW_DRAGGING;
pub use state::window_state::WINDOW_PINNED;
pub use state::window_state::WINDOW_RESIZING;
