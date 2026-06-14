//! Tauri command handlers
//!
//! This module organizes all IPC commands by domain:
//! - `tabs` - Tab management commands
//! - `clipboard` - Clipboard item commands
//! - `window` - Window control commands
//! - `settings` - Settings commands
//! - `test` - Test utilities
//! - `preview` - Preview window commands
//! - `dev_log` - Development log file commands

pub mod clipboard;
pub mod dev_log;
pub mod preview;
pub mod settings;
pub mod system;
pub mod tabs;
pub mod test;
pub mod window;

// Re-export all commands for convenient access
pub use clipboard::*;
pub use dev_log::*;
pub use preview::*;
pub use settings::*;
pub use system::*;
pub use tabs::*;
pub use test::*;
pub use window::*;
