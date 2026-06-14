//! Core type definitions for the plugin system

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Clipboard data packet - unified format for plugin communication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipPacket {
    /// Unique identifier (UUID)
    pub id: String,

    /// Data type
    #[serde(rename = "type")]
    pub packet_type: ClipPacketType,

    /// Actual data content (base64 for binary, raw for text)
    pub data: String,

    /// MIME type
    #[serde(rename = "mimeType")]
    pub mime_type: String,

    /// Metadata
    pub metadata: PacketMetadata,

    /// Processing pipeline trace
    pub pipeline: PipelineTrace,

    /// Extension fields (plugin custom data)
    #[serde(default)]
    pub extensions: serde_json::Map<String, serde_json::Value>,
}

impl ClipPacket {
    /// Create a new ClipPacket with default values
    pub fn new(id: String, packet_type: ClipPacketType, data: String, mime_type: String) -> Self {
        let now = Utc::now();
        Self {
            id,
            packet_type,
            data,
            mime_type,
            metadata: PacketMetadata {
                source_app: None,
                window_title: None,
                created_at: now,
                updated_at: now,
                is_sensitive: false,
                tags: Vec::new(),
                content_hash: None,
            },
            pipeline: PipelineTrace {
                plugins: Vec::new(),
                timestamps: Vec::new(),
                statuses: Vec::new(),
            },
            extensions: serde_json::Map::new(),
        }
    }

    /// Add a plugin processing record to the pipeline
    pub fn add_pipeline_record(&mut self, plugin_id: &str, status: ProcessStatus) {
        self.pipeline.plugins.push(plugin_id.to_string());
        self.pipeline.timestamps.push(Utc::now());
        self.pipeline.statuses.push(status);
    }

    /// Set extension data for a specific plugin
    pub fn set_extension(&mut self, plugin_id: &str, data: serde_json::Value) {
        self.extensions.insert(plugin_id.to_string(), data);
    }

    /// Get extension data for a specific plugin
    pub fn get_extension(&self, plugin_id: &str) -> Option<&serde_json::Value> {
        self.extensions.get(plugin_id)
    }
}

/// Data type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ClipPacketType {
    /// Plain text
    Text,
    /// Image data (base64 encoded)
    Image,
    /// File reference
    File,
    /// Rich text (HTML, RTF, etc.)
    RichText,
    /// Custom type with identifier
    Custom(String),
}

impl ClipPacketType {
    /// Check if this is a text type
    pub fn is_text(&self) -> bool {
        matches!(self, ClipPacketType::Text | ClipPacketType::RichText)
    }

    /// Check if this is an image type
    pub fn is_image(&self) -> bool {
        matches!(self, ClipPacketType::Image)
    }

    /// Get the type name as string
    pub fn as_str(&self) -> &str {
        match self {
            ClipPacketType::Text => "text",
            ClipPacketType::Image => "image",
            ClipPacketType::File => "file",
            ClipPacketType::RichText => "rich-text",
            ClipPacketType::Custom(name) => name.as_str(),
        }
    }
}

impl std::fmt::Display for ClipPacketType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipPacketType::Custom(name) => write!(f, "custom:{}", name),
            _ => write!(f, "{}", self.as_str()),
        }
    }
}

/// Packet metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketMetadata {
    /// Source application
    #[serde(rename = "sourceApp", skip_serializing_if = "Option::is_none")]
    pub source_app: Option<String>,

    /// Window title
    #[serde(rename = "windowTitle", skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,

    /// Creation timestamp
    #[serde(rename = "createdAt")]
    pub created_at: DateTime<Utc>,

    /// Last update timestamp
    #[serde(rename = "updatedAt")]
    pub updated_at: DateTime<Utc>,

    /// Whether content is sensitive (password, key, etc.)
    #[serde(rename = "isSensitive")]
    pub is_sensitive: bool,

    /// Tags
    #[serde(default)]
    pub tags: Vec<String>,

    /// Content hash for deduplication
    #[serde(rename = "contentHash", skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

/// Pipeline processing trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineTrace {
    /// Plugin IDs that processed this packet
    pub plugins: Vec<String>,

    /// Processing timestamps
    pub timestamps: Vec<DateTime<Utc>>,

    /// Processing statuses
    pub statuses: Vec<ProcessStatus>,
}

/// Processing status for each plugin in the pipeline
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProcessStatus {
    /// Successfully processed without modification
    Success,
    /// Successfully processed with modification
    Modified,
    /// Packet was filtered out
    Filtered,
    /// Processing failed
    Failed(String),
}

impl ProcessStatus {
    /// Check if processing was successful
    pub fn is_success(&self) -> bool {
        matches!(self, ProcessStatus::Success | ProcessStatus::Modified)
    }

    /// Check if packet was modified
    pub fn is_modified(&self) -> bool {
        matches!(self, ProcessStatus::Modified)
    }

    /// Check if processing failed
    pub fn is_failed(&self) -> bool {
        matches!(self, ProcessStatus::Failed(_))
    }
}

/// Plugin instance information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInstance {
    /// Plugin ID
    pub id: String,

    /// Plugin manifest
    pub manifest: crate::plugin::manifest::PluginManifest,

    /// Current state
    pub state: crate::plugin::lifecycle::state::PluginState,

    /// Granted permissions
    pub granted_permissions: Vec<String>,

    /// Plugin configuration
    pub config: serde_json::Value,

    /// Statistics
    pub statistics: PluginStatistics,
}

/// Plugin runtime statistics
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PluginStatistics {
    /// Number of times activated
    #[serde(rename = "activatedCount")]
    pub activated_count: u64,

    /// Total runtime in milliseconds
    #[serde(rename = "totalRuntimeMs")]
    pub total_runtime_ms: u64,

    /// Last activation time
    #[serde(rename = "lastActivated")]
    pub last_activated: Option<DateTime<Utc>>,

    /// Error count
    #[serde(rename = "errorCount")]
    pub error_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clip_packet_creation() {
        let packet = ClipPacket::new(
            "test-id".to_string(),
            ClipPacketType::Text,
            "Hello, World!".to_string(),
            "text/plain".to_string(),
        );

        assert_eq!(packet.id, "test-id");
        assert_eq!(packet.packet_type, ClipPacketType::Text);
        assert_eq!(packet.data, "Hello, World!");
        assert_eq!(packet.mime_type, "text/plain");
        assert!(!packet.metadata.is_sensitive);
    }

    #[test]
    fn test_clip_packet_extensions() {
        let mut packet = ClipPacket::new(
            "test-id".to_string(),
            ClipPacketType::Text,
            "test".to_string(),
            "text/plain".to_string(),
        );

        packet.set_extension(
            "com.example.ocr",
            serde_json::json!({ "text": "recognized text" }),
        );

        assert!(packet.get_extension("com.example.ocr").is_some());
        assert!(packet.get_extension("unknown").is_none());
    }

    #[test]
    fn test_pipeline_record() {
        let mut packet = ClipPacket::new(
            "test-id".to_string(),
            ClipPacketType::Text,
            "test".to_string(),
            "text/plain".to_string(),
        );

        packet.add_pipeline_record("com.example.plugin", ProcessStatus::Success);

        assert_eq!(packet.pipeline.plugins.len(), 1);
        assert_eq!(packet.pipeline.plugins[0], "com.example.plugin");
        assert!(packet.pipeline.statuses[0].is_success());
    }

    #[test]
    fn test_packet_type_serialization() {
        let text_type = ClipPacketType::Text;
        let json = serde_json::to_string(&text_type).unwrap();
        assert_eq!(json, "\"text\"");

        let custom_type = ClipPacketType::Custom("my-type".to_string());
        let json = serde_json::to_string(&custom_type).unwrap();
        assert_eq!(json, "{\"custom\":\"my-type\"}");
    }
}
