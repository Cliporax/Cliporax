//! Plugin state definitions

use serde::{Deserialize, Serialize};

use crate::plugin::manifest::PermissionRequest;

/// Plugin lifecycle state
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum PluginState {
    /// Plugin discovered in plugin directory
    #[default]
    Discovered,

    /// Plugin manifest validated
    Validated,

    /// Plugin loaded into memory
    Loaded,

    /// Waiting for permission approval
    PendingPermission(Vec<PermissionRequest>),

    /// Plugin active and running
    Active,

    /// Plugin inactive (paused)
    Inactive,

    /// Plugin unloaded from memory
    Unloaded,

    /// Plugin encountered an error
    Error(String),
}

impl PluginState {
    /// Check if plugin is in an error state
    pub fn is_error(&self) -> bool {
        matches!(self, PluginState::Error(_))
    }

    /// Check if plugin is active
    pub fn is_active(&self) -> bool {
        matches!(self, PluginState::Active)
    }

    /// Check if plugin is loaded
    pub fn is_loaded(&self) -> bool {
        matches!(
            self,
            PluginState::Loaded
                | PluginState::Active
                | PluginState::Inactive
                | PluginState::PendingPermission(_)
        )
    }

    /// Check if plugin is pending permission
    pub fn is_pending_permission(&self) -> bool {
        matches!(self, PluginState::PendingPermission(_))
    }

    /// Get error message if in error state
    pub fn error_message(&self) -> Option<&str> {
        match self {
            PluginState::Error(msg) => Some(msg),
            _ => None,
        }
    }

    /// Get pending permissions if in pending state
    pub fn pending_permissions(&self) -> Option<&[PermissionRequest]> {
        match self {
            PluginState::PendingPermission(perms) => Some(perms),
            _ => None,
        }
    }

    /// Get display label
    pub fn label(&self) -> &str {
        match self {
            PluginState::Discovered => "Discovered",
            PluginState::Validated => "Validated",
            PluginState::Loaded => "Loaded",
            PluginState::PendingPermission(_) => "Pending Permission",
            PluginState::Active => "Active",
            PluginState::Inactive => "Inactive",
            PluginState::Unloaded => "Unloaded",
            PluginState::Error(_) => "Error",
        }
    }
}

impl std::fmt::Display for PluginState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PluginState::Error(msg) => write!(f, "Error: {}", msg),
            PluginState::PendingPermission(perms) => {
                write!(f, "Pending Permission ({} items)", perms.len())
            }
            _ => write!(f, "{}", self.label()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_checks() {
        let active = PluginState::Active;
        assert!(active.is_active());
        assert!(active.is_loaded());
        assert!(!active.is_error());
        assert!(!active.is_pending_permission());

        let error = PluginState::Error("Test error".to_string());
        assert!(error.is_error());
        assert!(!error.is_active());
        assert_eq!(error.error_message(), Some("Test error"));

        let pending = PluginState::PendingPermission(vec![PermissionRequest {
            permission: "data:read".to_string(),
            reason: "Need to read data".to_string(),
            required: true,
        }]);
        assert!(pending.is_pending_permission());
        assert!(pending.pending_permissions().is_some());
        assert_eq!(pending.pending_permissions().unwrap().len(), 1);
    }

    #[test]
    fn test_state_display() {
        assert_eq!(format!("{}", PluginState::Active), "Active");
        assert_eq!(format!("{}", PluginState::Inactive), "Inactive");

        let error = PluginState::Error("Something went wrong".to_string());
        assert_eq!(format!("{}", error), "Error: Something went wrong");

        let pending = PluginState::PendingPermission(vec![]);
        assert_eq!(format!("{}", pending), "Pending Permission (0 items)");
    }
}
