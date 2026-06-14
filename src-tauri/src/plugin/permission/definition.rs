//! Permission definitions for the plugin system

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Permission definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Permission {
    /// Permission identifier (format: namespace:action)
    pub id: String,

    /// Display name
    pub name: String,

    /// Description
    pub description: String,

    /// Risk level
    pub risk_level: RiskLevel,

    /// Permission category
    pub category: PermissionCategory,

    /// Related permissions that are implied by this permission
    #[serde(default)]
    pub implies: Vec<String>,
}

/// Risk level for permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RiskLevel {
    /// Low risk - automatically approved
    Low,

    /// Medium risk - prompt on first use
    Medium,

    /// High risk - require user confirmation
    High,

    /// Dangerous - require explicit authorization
    Dangerous,
}

impl RiskLevel {
    /// Check if this risk level requires user confirmation
    pub fn requires_confirmation(&self) -> bool {
        matches!(self, RiskLevel::High | RiskLevel::Dangerous)
    }

    /// Check if this risk level requires explicit authorization
    pub fn requires_explicit_auth(&self) -> bool {
        matches!(self, RiskLevel::Dangerous)
    }

    /// Get display label
    pub fn label(&self) -> &'static str {
        match self {
            RiskLevel::Low => "Low",
            RiskLevel::Medium => "Medium",
            RiskLevel::High => "High",
            RiskLevel::Dangerous => "Dangerous",
        }
    }

    /// Get color for UI display
    pub fn color(&self) -> &'static str {
        match self {
            RiskLevel::Low => "green",
            RiskLevel::Medium => "yellow",
            RiskLevel::High => "orange",
            RiskLevel::Dangerous => "red",
        }
    }
}

/// Permission category
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PermissionCategory {
    /// UI permissions
    Ui,

    /// Data permissions
    Data,

    /// System permissions
    System,

    /// Network permissions
    Network,
}

impl PermissionCategory {
    /// Get display label
    pub fn label(&self) -> &'static str {
        match self {
            PermissionCategory::Ui => "UI",
            PermissionCategory::Data => "Data",
            PermissionCategory::System => "System",
            PermissionCategory::Network => "Network",
        }
    }
}

/// Get all built-in permissions
pub fn builtin_permissions() -> HashMap<String, Permission> {
    let mut perms = HashMap::new();

    // === UI Permissions ===

    perms.insert(
        "ui:notification".to_string(),
        Permission {
            id: "ui:notification".to_string(),
            name: "Show Notifications".to_string(),
            description: "Display notification messages in the system".to_string(),
            risk_level: RiskLevel::Low,
            category: PermissionCategory::Ui,
            implies: vec![],
        },
    );

    perms.insert(
        "ui:dialog".to_string(),
        Permission {
            id: "ui:dialog".to_string(),
            name: "Show Dialogs".to_string(),
            description: "Display modal dialogs for user interaction".to_string(),
            risk_level: RiskLevel::Medium,
            category: PermissionCategory::Ui,
            implies: vec!["ui:notification".to_string()],
        },
    );

    perms.insert(
        "ui:extension".to_string(),
        Permission {
            id: "ui:extension".to_string(),
            name: "Register UI Extensions".to_string(),
            description: "Add custom components to the application interface".to_string(),
            risk_level: RiskLevel::Medium,
            category: PermissionCategory::Ui,
            implies: vec![],
        },
    );

    perms.insert(
        "ui:context-menu".to_string(),
        Permission {
            id: "ui:context-menu".to_string(),
            name: "Add Context Menu Items".to_string(),
            description: "Add items to right-click context menus".to_string(),
            risk_level: RiskLevel::Low,
            category: PermissionCategory::Ui,
            implies: vec![],
        },
    );

    // === Data Permissions ===

    perms.insert(
        "data:read".to_string(),
        Permission {
            id: "data:read".to_string(),
            name: "Read Clipboard Data".to_string(),
            description: "Read clipboard history records".to_string(),
            risk_level: RiskLevel::Low,
            category: PermissionCategory::Data,
            implies: vec![],
        },
    );

    perms.insert(
        "data:write".to_string(),
        Permission {
            id: "data:write".to_string(),
            name: "Write Clipboard Data".to_string(),
            description: "Modify or add clipboard content".to_string(),
            risk_level: RiskLevel::High,
            category: PermissionCategory::Data,
            implies: vec!["data:read".to_string()],
        },
    );

    perms.insert(
        "data:delete".to_string(),
        Permission {
            id: "data:delete".to_string(),
            name: "Delete Clipboard Data".to_string(),
            description: "Delete clipboard history records".to_string(),
            risk_level: RiskLevel::Dangerous,
            category: PermissionCategory::Data,
            implies: vec!["data:read".to_string()],
        },
    );

    perms.insert(
        "data:sensitive".to_string(),
        Permission {
            id: "data:sensitive".to_string(),
            name: "Access Sensitive Data".to_string(),
            description: "Read clipboard content marked as sensitive (passwords, keys)".to_string(),
            risk_level: RiskLevel::Dangerous,
            category: PermissionCategory::Data,
            implies: vec!["data:read".to_string()],
        },
    );

    perms.insert(
        "data:transform".to_string(),
        Permission {
            id: "data:transform".to_string(),
            name: "Transform Clipboard Data".to_string(),
            description: "Transform clipboard content through the plugin pipeline".to_string(),
            risk_level: RiskLevel::Medium,
            category: PermissionCategory::Data,
            implies: vec!["data:read".to_string()],
        },
    );

    // === System Permissions ===

    perms.insert(
        "system:storage".to_string(),
        Permission {
            id: "system:storage".to_string(),
            name: "Local Storage".to_string(),
            description: "Store plugin data locally".to_string(),
            risk_level: RiskLevel::Low,
            category: PermissionCategory::System,
            implies: vec![],
        },
    );

    perms.insert(
        "system:clipboard-read".to_string(),
        Permission {
            id: "system:clipboard-read".to_string(),
            name: "Read System Clipboard".to_string(),
            description: "Directly read system clipboard content".to_string(),
            risk_level: RiskLevel::Medium,
            category: PermissionCategory::System,
            implies: vec!["data:read".to_string()],
        },
    );

    perms.insert(
        "system:clipboard-write".to_string(),
        Permission {
            id: "system:clipboard-write".to_string(),
            name: "Write System Clipboard".to_string(),
            description: "Directly write to system clipboard".to_string(),
            risk_level: RiskLevel::High,
            category: PermissionCategory::System,
            implies: vec![
                "system:clipboard-read".to_string(),
                "data:write".to_string(),
            ],
        },
    );

    perms.insert(
        "clipboard:read".to_string(),
        Permission {
            id: "clipboard:read".to_string(),
            name: "Read System Clipboard".to_string(),
            description: "Directly read system clipboard content".to_string(),
            risk_level: RiskLevel::Medium,
            category: PermissionCategory::System,
            implies: vec!["data:read".to_string()],
        },
    );

    perms.insert(
        "clipboard:write".to_string(),
        Permission {
            id: "clipboard:write".to_string(),
            name: "Write System Clipboard".to_string(),
            description: "Directly write to system clipboard".to_string(),
            risk_level: RiskLevel::High,
            category: PermissionCategory::System,
            implies: vec!["clipboard:read".to_string(), "data:write".to_string()],
        },
    );

    perms.insert(
        "system:process".to_string(),
        Permission {
            id: "system:process".to_string(),
            name: "Execute External Programs".to_string(),
            description: "Start and manage external processes".to_string(),
            risk_level: RiskLevel::Dangerous,
            category: PermissionCategory::System,
            implies: vec![],
        },
    );

    perms.insert(
        "system:file-read".to_string(),
        Permission {
            id: "system:file-read".to_string(),
            name: "Read Files".to_string(),
            description: "Read files from the file system".to_string(),
            risk_level: RiskLevel::High,
            category: PermissionCategory::System,
            implies: vec![],
        },
    );

    perms.insert(
        "system:file-write".to_string(),
        Permission {
            id: "system:file-write".to_string(),
            name: "Write Files".to_string(),
            description: "Write files to the file system".to_string(),
            risk_level: RiskLevel::Dangerous,
            category: PermissionCategory::System,
            implies: vec!["system:file-read".to_string()],
        },
    );

    // === Network Permissions ===

    perms.insert(
        "network:fetch".to_string(),
        Permission {
            id: "network:fetch".to_string(),
            name: "HTTP Requests".to_string(),
            description: "Make HTTP/HTTPS network requests".to_string(),
            risk_level: RiskLevel::High,
            category: PermissionCategory::Network,
            implies: vec![],
        },
    );

    perms.insert(
        "network:websocket".to_string(),
        Permission {
            id: "network:websocket".to_string(),
            name: "WebSocket Connections".to_string(),
            description: "Establish WebSocket connections".to_string(),
            risk_level: RiskLevel::High,
            category: PermissionCategory::Network,
            implies: vec!["network:fetch".to_string()],
        },
    );

    perms.insert(
        "network:sync".to_string(),
        Permission {
            id: "network:sync".to_string(),
            name: "Cloud Sync".to_string(),
            description: "Sync data with cloud services".to_string(),
            risk_level: RiskLevel::Dangerous,
            category: PermissionCategory::Network,
            implies: vec![
                "network:fetch".to_string(),
                "data:read".to_string(),
                "data:write".to_string(),
            ],
        },
    );

    perms.insert(
        "network:localhost".to_string(),
        Permission {
            id: "network:localhost".to_string(),
            name: "Localhost Access".to_string(),
            description: "Access localhost services".to_string(),
            risk_level: RiskLevel::Medium,
            category: PermissionCategory::Network,
            implies: vec![],
        },
    );

    perms
}

/// Get permission definitions as a vector (for IPC responses)
pub fn builtin_permissions_list() -> Vec<Permission> {
    builtin_permissions().into_values().collect()
}

/// Check if a permission ID is defined
pub fn is_permission_defined(permission_id: &str) -> bool {
    builtin_permissions().contains_key(permission_id)
}

/// Get a specific permission by ID
pub fn get_permission(permission_id: &str) -> Option<Permission> {
    builtin_permissions().get(permission_id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_permissions_count() {
        let perms = builtin_permissions();
        // Should have all defined permissions
        assert!(perms.len() >= 18);
    }

    #[test]
    fn test_permission_implies() {
        let data_write = get_permission("data:write").unwrap();
        assert!(data_write.implies.contains(&"data:read".to_string()));

        let network_sync = get_permission("network:sync").unwrap();
        assert!(network_sync.implies.contains(&"network:fetch".to_string()));
        assert!(network_sync.implies.contains(&"data:read".to_string()));
    }

    #[test]
    fn test_risk_level_requires_confirmation() {
        assert!(!RiskLevel::Low.requires_confirmation());
        assert!(!RiskLevel::Medium.requires_confirmation());
        assert!(RiskLevel::High.requires_confirmation());
        assert!(RiskLevel::Dangerous.requires_confirmation());
    }

    #[test]
    fn test_risk_level_requires_explicit_auth() {
        assert!(!RiskLevel::Low.requires_explicit_auth());
        assert!(!RiskLevel::Medium.requires_explicit_auth());
        assert!(!RiskLevel::High.requires_explicit_auth());
        assert!(RiskLevel::Dangerous.requires_explicit_auth());
    }

    #[test]
    fn test_is_permission_defined() {
        assert!(is_permission_defined("data:read"));
        assert!(is_permission_defined("network:fetch"));
        assert!(!is_permission_defined("unknown:permission"));
    }

    #[test]
    fn test_permission_categories() {
        let perms = builtin_permissions();

        // Check each category has permissions
        let ui_count = perms
            .values()
            .filter(|p| p.category == PermissionCategory::Ui)
            .count();
        let data_count = perms
            .values()
            .filter(|p| p.category == PermissionCategory::Data)
            .count();
        let system_count = perms
            .values()
            .filter(|p| p.category == PermissionCategory::System)
            .count();
        let network_count = perms
            .values()
            .filter(|p| p.category == PermissionCategory::Network)
            .count();

        assert!(ui_count > 0);
        assert!(data_count > 0);
        assert!(system_count > 0);
        assert!(network_count > 0);
    }
}
