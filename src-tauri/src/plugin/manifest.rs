//! Plugin manifest parsing and validation

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Component, Path};

use crate::plugin::permission::definition::is_permission_defined;

/// Plugin manifest - defines plugin metadata and configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin unique identifier (reverse domain format: com.example.plugin-name)
    pub id: String,

    /// Plugin display name
    pub name: String,

    /// Version number (semantic versioning)
    pub version: String,

    /// Plugin description
    pub description: String,

    /// Author information
    pub author: AuthorInfo,

    /// Main entry file (default: "main.js")
    #[serde(default = "default_main")]
    pub main: String,

    /// Plugin type
    #[serde(rename = "type")]
    pub plugin_type: PluginType,

    /// Requested permissions
    pub permissions: Vec<PermissionRequest>,

    /// Extension point declarations
    #[serde(default)]
    pub extensions: Vec<ExtensionDeclaration>,

    /// Configuration schema
    #[serde(rename = "configSchema", skip_serializing_if = "Option::is_none")]
    pub config_schema: Option<ConfigSchema>,

    /// Compatibility requirements
    #[serde(default)]
    pub compatibility: CompatibilityInfo,

    /// Icon path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// Keywords for search
    #[serde(default)]
    pub keywords: Vec<String>,

    /// Homepage URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,

    /// Repository URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,

    /// License
    #[serde(skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,

    /// Minimum application version
    #[serde(rename = "minAppVersion", skip_serializing_if = "Option::is_none")]
    pub min_app_version: Option<String>,

    /// Whether this is a builtin plugin
    #[serde(rename = "isBuiltin", default)]
    pub is_builtin: bool,
}

fn default_main() -> String {
    "main.js".to_string()
}

impl PluginManifest {
    /// Parse manifest from JSON string
    pub fn from_json(json: &str) -> Result<Self, ManifestError> {
        serde_json::from_str(json).map_err(ManifestError::ParseError)
    }

    /// Parse manifest from JSON value
    pub fn from_value(value: serde_json::Value) -> Result<Self, ManifestError> {
        serde_json::from_value(value).map_err(ManifestError::ParseError)
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String, ManifestError> {
        serde_json::to_string_pretty(self).map_err(|e| ManifestError::SerializeError(e.to_string()))
    }

    /// Validate the manifest
    pub fn validate(&self) -> Result<(), ManifestError> {
        // Validate ID format (reverse domain)
        if !self.id.contains('.') {
            return Err(ManifestError::InvalidId(format!(
                "Plugin ID must be in reverse domain format (e.g., com.example.plugin), got: {}",
                self.id
            )));
        }

        // Validate ID characters
        if !self
            .id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
        {
            return Err(ManifestError::InvalidId(format!(
                "Plugin ID contains invalid characters: {}",
                self.id
            )));
        }

        // Validate version format (semantic versioning)
        if let Err(e) = semver::Version::parse(&self.version) {
            return Err(ManifestError::InvalidVersion(format!(
                "Invalid version '{}': {}",
                self.version, e
            )));
        }

        // Validate main entry file
        if self.main.is_empty() {
            return Err(ManifestError::InvalidMain(
                "Main entry file cannot be empty".to_string(),
            ));
        }
        let main_path = Path::new(&self.main);
        if main_path.is_absolute()
            || main_path
                .components()
                .any(|component| matches!(component, Component::ParentDir))
        {
            return Err(ManifestError::InvalidMain(
                "Main entry file must stay inside the plugin directory".to_string(),
            ));
        }

        // Validate permissions
        for perm in &self.permissions {
            if perm.permission.is_empty() {
                return Err(ManifestError::InvalidPermission(
                    "Permission cannot be empty".to_string(),
                ));
            }
            if !is_permission_defined(&perm.permission) {
                return Err(ManifestError::InvalidPermission(format!(
                    "Unknown permission: {}",
                    perm.permission
                )));
            }
        }

        for extension in &self.extensions {
            if !matches!(
                extension.point,
                ExtensionPoint::SettingsPanel
                    | ExtensionPoint::Card
                    | ExtensionPoint::Sidebar
                    | ExtensionPoint::Preview
            ) {
                return Err(ManifestError::ValidationFailed(
                    "Custom extension points are not allowed".to_string(),
                ));
            }
            if extension.component.trim().is_empty()
                || !extension
                    .component
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                return Err(ManifestError::ValidationFailed(format!(
                    "Invalid extension component: {}",
                    extension.component
                )));
            }
        }

        Ok(())
    }

    /// Check if plugin requires specific permission
    pub fn requires_permission(&self, permission: &str) -> bool {
        self.permissions
            .iter()
            .any(|p| p.permission == permission && p.required)
    }

    /// Get all required permissions
    pub fn get_required_permissions(&self) -> Vec<&PermissionRequest> {
        self.permissions.iter().filter(|p| p.required).collect()
    }

    /// Check if plugin is compatible with given app version
    pub fn is_compatible_with(&self, app_version: &str) -> bool {
        if let Some(min_version) = &self.min_app_version {
            if let (Ok(app_ver), Ok(min_ver)) = (
                semver::Version::parse(app_version),
                semver::Version::parse(min_version),
            ) {
                if app_ver < min_ver {
                    return false;
                }
            }
        }

        if let Some(max_version) = &self.compatibility.max_app_version {
            if let (Ok(app_ver), Ok(max_ver)) = (
                semver::Version::parse(app_version),
                semver::Version::parse(max_version),
            ) {
                if app_ver > max_ver {
                    return false;
                }
            }
        }

        true
    }
}

/// Author information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorInfo {
    /// Author name
    pub name: String,

    /// Author email
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,

    /// Author URL
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Plugin type enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PluginType {
    /// Source plugin - produces clipboard data
    Source,
    /// Transform plugin - processes/modifies data
    Transform,
    /// Sink plugin - consumes data
    Sink,
    /// Router plugin - decides data flow
    Router,
    /// Hybrid type with multiple capabilities
    Hybrid(Vec<PluginType>),
}

impl PluginType {
    /// Check if this is a source plugin
    pub fn is_source(&self) -> bool {
        matches!(self, PluginType::Source)
            || matches!(self, PluginType::Hybrid(types) if types.iter().any(|t| t.is_source()))
    }

    /// Check if this is a transform plugin
    pub fn is_transform(&self) -> bool {
        matches!(self, PluginType::Transform)
            || matches!(self, PluginType::Hybrid(types) if types.iter().any(|t| t.is_transform()))
    }

    /// Check if this is a sink plugin
    pub fn is_sink(&self) -> bool {
        matches!(self, PluginType::Sink)
            || matches!(self, PluginType::Hybrid(types) if types.iter().any(|t| t.is_sink()))
    }

    /// Check if this is a router plugin
    pub fn is_router(&self) -> bool {
        matches!(self, PluginType::Router)
            || matches!(self, PluginType::Hybrid(types) if types.iter().any(|t| t.is_router()))
    }
}

/// Permission request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PermissionRequest {
    /// Permission identifier
    pub permission: String,

    /// Reason shown to user
    pub reason: String,

    /// Whether permission is required
    #[serde(default)]
    pub required: bool,
}

/// Extension point declaration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionDeclaration {
    /// Extension point ID
    pub point: ExtensionPoint,

    /// Entry component
    pub component: String,

    /// Display condition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<String>,

    /// Priority (higher = more priority)
    #[serde(default)]
    pub priority: i32,
}

/// Extension point enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ExtensionPoint {
    /// Settings panel extension
    SettingsPanel,
    /// Card extension
    Card,
    /// Sidebar extension
    Sidebar,
    /// Preview extension
    Preview,
    /// Custom extension point
    Custom(String),
}

/// Configuration schema
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSchema {
    /// JSON Schema for configuration
    #[serde(flatten)]
    pub schema: serde_json::Value,

    /// Default configuration values
    #[serde(default)]
    pub default: HashMap<String, serde_json::Value>,
}

/// Compatibility information
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompatibilityInfo {
    /// Maximum supported app version
    #[serde(rename = "maxAppVersion", skip_serializing_if = "Option::is_none")]
    pub max_app_version: Option<String>,

    /// Supported platforms
    #[serde(default = "default_platforms")]
    pub platforms: Vec<Platform>,
}

fn default_platforms() -> Vec<Platform> {
    vec![Platform::Windows, Platform::Linux, Platform::Macos]
}

/// Platform enumeration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Windows,
    Linux,
    Macos,
}

impl Platform {
    /// Get current platform
    pub fn current() -> Self {
        #[cfg(target_os = "windows")]
        {
            Platform::Windows
        }
        #[cfg(target_os = "linux")]
        {
            Platform::Linux
        }
        #[cfg(target_os = "macos")]
        {
            Platform::Macos
        }
    }

    /// Check if this is the current platform
    pub fn is_current(&self) -> bool {
        self == &Self::current()
    }
}

/// Manifest error types
#[derive(Debug, thiserror::Error)]
pub enum ManifestError {
    #[error("Failed to parse manifest JSON: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("Failed to serialize manifest: {0}")]
    SerializeError(String),

    #[error("Invalid plugin ID: {0}")]
    InvalidId(String),

    #[error("Invalid version: {0}")]
    InvalidVersion(String),

    #[error("Invalid main entry: {0}")]
    InvalidMain(String),

    #[error("Invalid permission: {0}")]
    InvalidPermission(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_from_json() {
        let json = r#"
        {
            "id": "com.example.test-plugin",
            "name": "Test Plugin",
            "version": "1.0.0",
            "description": "A test plugin",
            "author": {
                "name": "Test Author"
            },
            "type": "transform",
            "permissions": [
                {
                    "permission": "data:read",
                    "reason": "Need to read clipboard data",
                    "required": true
                }
            ]
        }
        "#;

        let manifest = PluginManifest::from_json(json).unwrap();
        assert_eq!(manifest.id, "com.example.test-plugin");
        assert_eq!(manifest.name, "Test Plugin");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.plugin_type, PluginType::Transform);
        assert_eq!(manifest.permissions.len(), 1);
        assert_eq!(manifest.main, "main.js"); // default value
    }

    #[test]
    fn test_manifest_validation() {
        let mut manifest = PluginManifest {
            id: "invalid-id".to_string(), // No dot
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            description: "Test".to_string(),
            author: AuthorInfo {
                name: "Test".to_string(),
                email: None,
                url: None,
            },
            main: "main.js".to_string(),
            plugin_type: PluginType::Transform,
            permissions: vec![],
            extensions: vec![],
            config_schema: None,
            compatibility: CompatibilityInfo::default(),
            icon: None,
            keywords: vec![],
            homepage: None,
            repository: None,
            license: None,
            min_app_version: None,
            is_builtin: false,
        };

        // Invalid ID (no dot)
        assert!(manifest.validate().is_err());

        // Fix ID
        manifest.id = "com.example.test".to_string();
        assert!(manifest.validate().is_ok());

        // Invalid version
        manifest.version = "invalid".to_string();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_plugin_type_checks() {
        let source = PluginType::Source;
        assert!(source.is_source());
        assert!(!source.is_transform());

        let hybrid = PluginType::Hybrid(vec![PluginType::Source, PluginType::Transform]);
        assert!(hybrid.is_source());
        assert!(hybrid.is_transform());
        assert!(!hybrid.is_sink());
    }

    #[test]
    fn test_platform_current() {
        let current = Platform::current();
        assert!(current.is_current());
    }

    #[test]
    fn test_compatibility_check() {
        let manifest = PluginManifest {
            id: "com.example.test".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            description: "Test".to_string(),
            author: AuthorInfo {
                name: "Test".to_string(),
                email: None,
                url: None,
            },
            main: "main.js".to_string(),
            plugin_type: PluginType::Transform,
            permissions: vec![],
            extensions: vec![],
            config_schema: None,
            compatibility: CompatibilityInfo {
                max_app_version: Some("2.0.0".to_string()),
                platforms: default_platforms(),
            },
            icon: None,
            keywords: vec![],
            homepage: None,
            repository: None,
            license: None,
            min_app_version: Some("1.0.0".to_string()),
            is_builtin: false,
        };

        // Compatible version
        assert!(manifest.is_compatible_with("1.5.0"));

        // Below minimum
        assert!(!manifest.is_compatible_with("0.9.0"));

        // Above maximum
        assert!(!manifest.is_compatible_with("2.1.0"));
    }
}
