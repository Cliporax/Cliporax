/// Cloud Sync Module
///
/// Provides synchronization capabilities for clipboard items across devices
/// using WebDAV and SFTP providers with optional end-to-end encryption.
///
/// Architecture:
/// - Sync engine coordinates the sync process
/// - Providers handle remote storage (WebDAV/SFTP)
/// - Repositories manage local sync state in SQLite
/// - Commands expose functionality via Tauri IPC
pub mod change_log;
pub mod codec;
pub mod commands;
pub mod crypto;
pub mod engine;
pub mod error;
pub mod lock;
pub mod manifest;
pub mod models;
pub mod providers;
pub mod repository;
mod repository_profile;
mod repository_tabs;
pub mod scheduler;
pub mod secrets;
pub mod service;

// Re-export commonly used types
pub use error::SyncError;
pub use models::*;
