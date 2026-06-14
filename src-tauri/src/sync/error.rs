use thiserror::Error;

#[derive(Error, Debug)]
pub enum SyncError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Provider error: {0}")]
    Provider(String),

    #[error("Lock error: {0}")]
    Lock(String),

    #[error("Encryption error: {0}")]
    Encryption(String),

    #[error("Secret store error: {0}")]
    SecretStore(String),

    #[error("Profile not found: {0}")]
    ProfileNotFound(String),

    #[error("Invalid profile: {0}")]
    InvalidProfile(String),

    #[error("Conflict error: {0}")]
    Conflict(String),

    #[error("Sync cancelled: {0}")]
    Cancelled(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Connection test failed: {0}")]
    ConnectionTestFailed(String),

    #[error("Schema version mismatch: local={local}, remote={remote}")]
    SchemaVersionMismatch { local: u32, remote: u32 },

    #[error("Tombstone retention error: {0}")]
    TombstoneError(String),
}

impl SyncError {
    pub fn provider(msg: impl Into<String>) -> Self {
        SyncError::Provider(msg.into())
    }

    pub fn lock(msg: impl Into<String>) -> Self {
        SyncError::Lock(msg.into())
    }

    pub fn encryption(msg: impl Into<String>) -> Self {
        SyncError::Encryption(msg.into())
    }

    pub fn secret_store(msg: impl Into<String>) -> Self {
        SyncError::SecretStore(msg.into())
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        SyncError::Validation(msg.into())
    }

    pub fn conflict(msg: impl Into<String>) -> Self {
        SyncError::Conflict(msg.into())
    }
}
