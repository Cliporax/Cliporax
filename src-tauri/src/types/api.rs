/// Shared type definitions - generate TypeScript types with ts-rs
use serde::{Deserialize, Serialize};

#[cfg(feature = "export-types")]
use ts_rs::TS;

/// Item type enum - generates a TypeScript union type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "export-types", derive(TS))]
#[cfg_attr(
    feature = "export-types",
    ts(export, export_to = "../src/types/generated/")
)]
#[serde(rename_all = "lowercase")]
pub enum ItemType {
    Text,
    Image,
    File,
}

/// Clipboard item
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "export-types", derive(TS))]
#[cfg_attr(
    feature = "export-types",
    ts(export, export_to = "../src/types/generated/")
)]
pub struct ClipboardItem {
    pub id: Option<i64>,

    /// Item type
    #[serde(rename = "type")]
    pub item_type: ItemType,

    /// Content, either text or a base64-encoded image
    pub content: String,

    /// Content hash used for deduplication
    pub content_hash: Option<String>,

    /// Metadata JSON
    #[cfg_attr(feature = "export-types", ts(optional))]
    pub metadata: Option<String>,

    /// Tags JSON array
    #[cfg_attr(feature = "export-types", ts(optional))]
    pub tags: Option<String>,

    /// Owning tab ID
    #[cfg_attr(feature = "export-types", ts(optional))]
    pub tab_id: Option<i64>,

    /// Whether the content is sensitive
    #[cfg_attr(feature = "export-types", ts(type = "boolean"))]
    pub is_sensitive: i32,

    /// Whether the item is pinned
    #[cfg_attr(feature = "export-types", ts(type = "boolean"))]
    pub is_pinned: i32,

    /// Display order
    #[cfg_attr(feature = "export-types", ts(optional))]
    pub display_order: Option<i32>,

    /// Creation time in ISO 8601 format
    #[cfg_attr(feature = "export-types", ts(type = "string"))]
    pub created_at: Option<String>,

    /// Update time in ISO 8601 format
    #[cfg_attr(feature = "export-types", ts(type = "string"))]
    pub updated_at: Option<String>,
}

/// Clipboard item input used during creation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "export-types", derive(TS))]
#[cfg_attr(
    feature = "export-types",
    ts(export, export_to = "../src/types/generated/")
)]
pub struct ClipboardItemInput {
    #[serde(rename = "type")]
    pub item_type: ItemType,
    pub content: String,
    pub content_hash: Option<String>,
    #[cfg_attr(feature = "export-types", ts(optional))]
    pub metadata: Option<String>,
    #[cfg_attr(feature = "export-types", ts(optional))]
    pub tags: Option<String>,
    #[cfg_attr(feature = "export-types", ts(optional))]
    pub tab_id: Option<i64>,
    #[cfg_attr(feature = "export-types", ts(type = "boolean"))]
    pub is_sensitive: i32,
    #[cfg_attr(feature = "export-types", ts(type = "boolean"))]
    pub is_pinned: i32,
}

/// Tab
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "export-types", derive(TS))]
#[cfg_attr(
    feature = "export-types",
    ts(export, export_to = "../src/types/generated/")
)]
pub struct Tab {
    pub id: Option<i64>,
    pub name: String,
    #[cfg_attr(feature = "export-types", ts(type = "boolean"))]
    pub is_default: i32,
    /// Creation time in ISO 8601 format
    #[cfg_attr(feature = "export-types", ts(type = "string"))]
    pub created_at: Option<String>,
}

/// Unified API response wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "export-types", derive(TS))]
#[cfg_attr(
    feature = "export-types",
    ts(export, export_to = "../src/types/generated/")
)]
#[serde(tag = "status", content = "data")]
pub enum ApiResult<T> {
    Ok(T),
    Err(ApiError),
}

/// API error type
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "export-types", derive(TS))]
#[cfg_attr(
    feature = "export-types",
    ts(export, export_to = "../src/types/generated/")
)]
#[serde(tag = "type", content = "data")]
pub enum ApiError {
    /// Database error
    Database { message: String },
    /// Resource not found
    NotFound { resource: String, id: i64 },
    /// Insufficient permissions
    Permission { required: String, reason: String },
    /// Invalid parameters
    Validation { field: String, message: String },
    /// Internal error
    Internal { message: String },
}

impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        match err {
            sqlx::Error::RowNotFound => ApiError::NotFound {
                resource: "unknown".to_string(),
                id: 0,
            },
            _ => ApiError::Database {
                message: err.to_string(),
            },
        }
    }
}

impl From<std::io::Error> for ApiError {
    fn from(err: std::io::Error) -> Self {
        ApiError::Internal {
            message: err.to_string(),
        }
    }
}
