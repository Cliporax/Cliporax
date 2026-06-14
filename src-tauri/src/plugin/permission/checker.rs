//! Permission checker implementation

use crate::plugin::manifest::PermissionRequest;
use crate::plugin::permission::definition::{builtin_permissions, Permission, RiskLevel};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Permission checker
#[derive(Debug)]
pub struct PermissionChecker {
    /// Defined permissions
    definitions: HashMap<String, Permission>,

    /// Granted permissions per plugin
    grants: HashMap<String, HashSet<String>>,

    /// Permission usage log
    usage_log: Vec<PermissionUsage>,
}

/// Permission usage record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionUsage {
    /// Plugin ID
    #[serde(rename = "pluginId")]
    pub plugin_id: String,

    /// Permission ID
    pub permission: String,

    /// Usage timestamp
    pub timestamp: DateTime<Utc>,

    /// Whether permission was granted
    pub granted: bool,
}

/// Permission check result
#[derive(Debug, Clone)]
pub enum PermissionResult {
    /// Permission granted
    Granted,

    /// Permission denied
    Denied {
        /// Denial reason
        reason: PermissionDeniedReason,

        /// Risk level of the permission
        risk_level: RiskLevel,
    },
}

/// Reason for permission denial
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PermissionDeniedReason {
    /// Permission not granted to this plugin
    NotGranted,

    /// Permission is not defined
    Undefined,

    /// Permission was revoked
    Revoked,

    /// Permission was denied by user
    UserDenied,
}

/// Permission evaluation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PermissionEvaluation {
    /// Permissions that can be auto-granted (low risk)
    #[serde(rename = "autoGrant")]
    pub auto_grant: Vec<PermissionRequest>,

    /// Permissions that need user confirmation
    #[serde(rename = "needConfirm")]
    pub need_confirm: Vec<PermissionRequest>,

    /// Permissions that were denied (unknown or dangerous not required)
    pub denied: Vec<(PermissionRequest, String)>,
}

impl PermissionChecker {
    /// Create a new permission checker
    pub fn new() -> Self {
        Self {
            definitions: builtin_permissions(),
            grants: HashMap::new(),
            usage_log: Vec::new(),
        }
    }

    /// Check if a permission is defined
    pub fn is_defined(&self, permission_id: &str) -> bool {
        self.definitions.contains_key(permission_id)
    }

    /// Check if a plugin has a specific permission granted
    pub fn is_granted(&self, plugin_id: &str, permission_id: &str) -> bool {
        self.grants
            .get(plugin_id)
            .map(|set| set.contains(permission_id))
            .unwrap_or(false)
    }

    /// Check if a plugin has permission
    pub fn check(&mut self, plugin_id: &str, permission_id: &str) -> PermissionResult {
        // 1. Check if permission is defined
        let perm_def = match self.definitions.get(permission_id) {
            Some(p) => p.clone(),
            None => {
                self.log_usage(plugin_id, permission_id, false);
                return PermissionResult::Denied {
                    reason: PermissionDeniedReason::Undefined,
                    risk_level: RiskLevel::Dangerous,
                };
            }
        };

        // 2. Check if permission is granted
        let granted = self.is_granted(plugin_id, permission_id);

        // 3. Log usage
        self.log_usage(plugin_id, permission_id, granted);

        if granted {
            PermissionResult::Granted
        } else {
            PermissionResult::Denied {
                reason: PermissionDeniedReason::NotGranted,
                risk_level: perm_def.risk_level,
            }
        }
    }

    /// Grant a permission to a plugin
    pub fn grant(&mut self, plugin_id: &str, permission_id: &str) -> Result<(), String> {
        let perm_def = self
            .definitions
            .get(permission_id)
            .ok_or_else(|| format!("Permission '{}' is not defined", permission_id))?
            .clone();

        // Grant the main permission
        self.grants
            .entry(plugin_id.to_string())
            .or_default()
            .insert(permission_id.to_string());

        // Auto-grant implied permissions
        for implied in &perm_def.implies {
            self.grant(plugin_id, implied)?;
        }

        log::info!(
            "[Permission] Granted '{}' to plugin '{}'",
            permission_id,
            plugin_id
        );
        Ok(())
    }

    /// Revoke a permission from a plugin
    pub fn revoke(&mut self, plugin_id: &str, permission_id: &str) {
        if let Some(grants) = self.grants.get_mut(plugin_id) {
            grants.remove(permission_id);
        }
        log::info!(
            "[Permission] Revoked '{}' from plugin '{}'",
            permission_id,
            plugin_id
        );
    }

    /// Revoke all permissions from a plugin
    pub fn revoke_all(&mut self, plugin_id: &str) {
        self.grants.remove(plugin_id);
        log::info!(
            "[Permission] Revoked all permissions from plugin '{}'",
            plugin_id
        );
    }

    /// Get all granted permissions for a plugin
    pub fn get_granted(&self, plugin_id: &str) -> Vec<String> {
        self.grants
            .get(plugin_id)
            .map(|set| set.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Evaluate permission requests and categorize them
    pub fn evaluate_requests(&self, requests: &[PermissionRequest]) -> PermissionEvaluation {
        let mut auto_grant = Vec::new();
        let mut need_confirm = Vec::new();
        let mut denied = Vec::new();

        for req in requests {
            if let Some(perm) = self.definitions.get(&req.permission) {
                match perm.risk_level {
                    RiskLevel::Low => {
                        auto_grant.push(req.clone());
                    }
                    RiskLevel::Medium | RiskLevel::High => {
                        need_confirm.push(req.clone());
                    }
                    RiskLevel::Dangerous => {
                        if req.required {
                            need_confirm.push(req.clone());
                        } else {
                            denied.push((
                                req.clone(),
                                "Dangerous permission not marked as required".to_string(),
                            ));
                        }
                    }
                }
            } else {
                denied.push((req.clone(), "Unknown permission".to_string()));
            }
        }

        PermissionEvaluation {
            auto_grant,
            need_confirm,
            denied,
        }
    }

    /// Process permission requests for a plugin
    /// Returns evaluation result for user confirmation if needed
    pub fn process_requests(
        &mut self,
        plugin_id: &str,
        requests: &[PermissionRequest],
    ) -> Result<(), PermissionEvaluation> {
        let evaluation = self.evaluate_requests(requests);

        // Auto-grant low-risk permissions
        for req in &evaluation.auto_grant {
            if let Err(e) = self.grant(plugin_id, &req.permission) {
                log::error!("[Permission] Failed to auto-grant: {}", e);
            }
        }

        // If there are permissions needing confirmation, return the evaluation
        if !evaluation.need_confirm.is_empty() {
            return Err(evaluation);
        }

        // Log any denied permissions
        for (req, reason) in &evaluation.denied {
            log::warn!(
                "[Permission] Denied '{}' for plugin '{}': {}",
                req.permission,
                plugin_id,
                reason
            );
        }

        Ok(())
    }

    /// Log permission usage
    fn log_usage(&mut self, plugin_id: &str, permission: &str, granted: bool) {
        self.usage_log.push(PermissionUsage {
            plugin_id: plugin_id.to_string(),
            permission: permission.to_string(),
            timestamp: Utc::now(),
            granted,
        });
    }

    /// Get usage log for a plugin
    pub fn get_usage_log(&self, plugin_id: &str) -> Vec<&PermissionUsage> {
        self.usage_log
            .iter()
            .filter(|u| u.plugin_id == plugin_id)
            .collect()
    }

    /// Get all usage logs
    pub fn get_all_usage_logs(&self) -> &[PermissionUsage] {
        &self.usage_log
    }

    /// Clear usage logs
    pub fn clear_usage_logs(&mut self) {
        self.usage_log.clear();
    }

    /// Get permission definition
    pub fn get_definition(&self, permission_id: &str) -> Option<&Permission> {
        self.definitions.get(permission_id)
    }

    /// Get all permission definitions
    pub fn get_all_definitions(&self) -> Vec<&Permission> {
        self.definitions.values().collect()
    }
}

impl Default for PermissionChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grant_permission() {
        let mut checker = PermissionChecker::new();

        // Grant permission
        checker.grant("com.example.plugin", "data:read").unwrap();

        // Check it's granted
        assert!(checker.is_granted("com.example.plugin", "data:read"));

        // Check implied permissions are NOT granted (data:read has no implies)
        assert!(!checker.is_granted("com.example.plugin", "data:write"));
    }

    #[test]
    fn test_grant_permission_with_implies() {
        let mut checker = PermissionChecker::new();

        // Grant data:write which implies data:read
        checker.grant("com.example.plugin", "data:write").unwrap();

        // Both should be granted
        assert!(checker.is_granted("com.example.plugin", "data:write"));
        assert!(checker.is_granted("com.example.plugin", "data:read"));
    }

    #[test]
    fn test_revoke_permission() {
        let mut checker = PermissionChecker::new();

        checker.grant("com.example.plugin", "data:read").unwrap();
        assert!(checker.is_granted("com.example.plugin", "data:read"));

        checker.revoke("com.example.plugin", "data:read");
        assert!(!checker.is_granted("com.example.plugin", "data:read"));
    }

    #[test]
    fn test_revoke_all_permissions() {
        let mut checker = PermissionChecker::new();

        checker.grant("com.example.plugin", "data:read").unwrap();
        checker
            .grant("com.example.plugin", "ui:notification")
            .unwrap();

        checker.revoke_all("com.example.plugin");

        assert!(!checker.is_granted("com.example.plugin", "data:read"));
        assert!(!checker.is_granted("com.example.plugin", "ui:notification"));
    }

    #[test]
    fn test_check_permission() {
        let mut checker = PermissionChecker::new();

        // Check undefined permission
        let result = checker.check("com.example.plugin", "unknown:permission");
        assert!(matches!(
            result,
            PermissionResult::Denied {
                reason: PermissionDeniedReason::Undefined,
                ..
            }
        ));

        // Check not granted permission
        let result = checker.check("com.example.plugin", "data:read");
        assert!(matches!(
            result,
            PermissionResult::Denied {
                reason: PermissionDeniedReason::NotGranted,
                ..
            }
        ));

        // Grant and check again
        checker.grant("com.example.plugin", "data:read").unwrap();
        let result = checker.check("com.example.plugin", "data:read");
        assert!(matches!(result, PermissionResult::Granted));
    }

    #[test]
    fn test_evaluate_requests() {
        let checker = PermissionChecker::new();

        let requests = vec![
            PermissionRequest {
                permission: "ui:notification".to_string(), // Low risk
                reason: "Need to show notifications".to_string(),
                required: true,
            },
            PermissionRequest {
                permission: "data:read".to_string(), // Medium risk
                reason: "Need to read data".to_string(),
                required: true,
            },
            PermissionRequest {
                permission: "data:delete".to_string(), // Dangerous
                reason: "Need to delete data".to_string(),
                required: false, // Not required
            },
            PermissionRequest {
                permission: "unknown:permission".to_string(), // Unknown
                reason: "Unknown permission".to_string(),
                required: true,
            },
        ];

        let evaluation = checker.evaluate_requests(&requests);

        // Low risk should be auto-grant
        assert_eq!(evaluation.auto_grant.len(), 2);
        assert_eq!(evaluation.auto_grant[0].permission, "ui:notification");
        assert_eq!(evaluation.auto_grant[1].permission, "data:read");

        // No medium/high required permissions in this set
        assert_eq!(evaluation.need_confirm.len(), 0);

        // Unknown and non-required dangerous should be denied
        assert_eq!(evaluation.denied.len(), 2);
    }

    #[test]
    fn test_usage_logging() {
        let mut checker = PermissionChecker::new();

        checker.grant("com.example.plugin", "data:read").unwrap();
        checker.check("com.example.plugin", "data:read");
        checker.check("com.example.plugin", "data:write");

        let logs = checker.get_usage_log("com.example.plugin");
        assert_eq!(logs.len(), 2);

        // First check should be granted
        assert!(logs[0].granted);
        assert_eq!(logs[0].permission, "data:read");

        // Second check should be denied
        assert!(!logs[1].granted);
        assert_eq!(logs[1].permission, "data:write");
    }
}
