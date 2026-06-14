//! Plugin registry - manages plugin lifecycle

use crate::plugin::lifecycle::state::PluginState;
use crate::plugin::manifest::{ManifestError, PermissionRequest, PluginManifest, PluginType};
use crate::plugin::permission::checker::PermissionChecker;
use crate::plugin::types::{PluginInstance, PluginStatistics};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Persisted plugin state
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersistedState {
    /// Active plugin IDs
    active_plugins: Vec<String>,
    /// Granted permissions per plugin
    granted_permissions: HashMap<String, Vec<String>>,
}

/// Discovered plugin information
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    /// Plugin manifest
    pub manifest: PluginManifest,

    /// Plugin directory path
    pub path: PathBuf,

    /// Current state
    pub state: PluginState,
}

/// Plugin registry - manages all plugins
pub struct PluginRegistry {
    /// Plugin directory path
    plugin_dir: PathBuf,

    /// State file path
    state_file: PathBuf,

    /// Discovered plugins
    discovered: HashMap<String, DiscoveredPlugin>,

    /// Loaded plugin instances
    instances: HashMap<String, PluginInstance>,

    /// Permission checker
    permission_checker: PermissionChecker,

    /// Plugin configurations
    configs: HashMap<String, serde_json::Value>,

    /// Builtin plugin IDs
    builtin_plugins: HashSet<String>,
}

/// Plugin info for IPC responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    /// Plugin ID
    pub id: String,

    /// Plugin name
    pub name: String,

    /// Plugin version
    pub version: String,

    /// Plugin description
    pub description: String,

    /// Author name
    pub author: String,

    /// Icon path
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,

    /// Current state
    pub state: PluginState,

    /// Requested permissions
    pub permissions: Vec<PermissionRequest>,

    /// Plugin type
    #[serde(rename = "type")]
    pub plugin_type: PluginType,

    /// Whether this is a builtin plugin
    #[serde(rename = "isBuiltin", default)]
    pub is_builtin: bool,
}

/// Plugin detail for IPC responses
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDetail {
    /// Plugin manifest
    pub manifest: PluginManifest,

    /// Current state
    pub state: PluginState,

    /// Granted permissions
    #[serde(rename = "grantedPermissions")]
    pub granted_permissions: Vec<String>,

    /// Plugin configuration
    pub config: serde_json::Value,

    /// Statistics
    pub statistics: PluginStatistics,
}

/// Load result for IPC responses
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LoadResult {
    /// Successfully loaded
    Success { success: bool },
    /// Permission required before loading
    PermissionRequired {
        #[serde(rename = "permissionRequired")]
        permissions: Vec<PermissionRequest>,
    },
}

/// Plugin error types
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Plugin already exists: {0}")]
    AlreadyExists(String),

    #[error("Manifest error: {0}")]
    Manifest(#[from] ManifestError),

    #[error("Discovery error: {0}")]
    Discovery(String),

    #[error("Load error: {0}")]
    Load(String),

    #[error("Permission required")]
    PermissionRequired(Vec<PermissionRequest>),

    #[error("Permission error: {0}")]
    Permission(String),

    #[error("Invalid state transition: {0}")]
    InvalidState(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

impl PluginRegistry {
    /// Create a new plugin registry
    pub fn new(plugin_dir: PathBuf) -> Self {
        let state_file = plugin_dir.join(".plugin_state.json");
        Self {
            plugin_dir,
            state_file,
            discovered: HashMap::new(),
            instances: HashMap::new(),
            permission_checker: PermissionChecker::new(),
            configs: HashMap::new(),
            builtin_plugins: HashSet::new(),
        }
    }

    /// Save persisted state to file
    async fn save_state(&self) -> Result<(), PluginError> {
        let state = PersistedState {
            active_plugins: self
                .instances
                .iter()
                .filter(|(_, inst)| inst.state == PluginState::Active)
                .map(|(id, _)| id.clone())
                .collect(),
            granted_permissions: self
                .instances
                .iter()
                .map(|(id, inst)| (id.clone(), inst.granted_permissions.clone()))
                .collect(),
        };

        let json = serde_json::to_string_pretty(&state)?;
        tokio::fs::write(&self.state_file, json).await?;
        log::info!("[Plugin] Saved state to {:?}", self.state_file);
        Ok(())
    }

    /// Load persisted state from file
    async fn load_state(&self) -> PersistedState {
        if !self.state_file.exists() {
            return PersistedState::default();
        }

        match tokio::fs::read_to_string(&self.state_file).await {
            Ok(json) => match serde_json::from_str::<PersistedState>(&json) {
                Ok(state) => {
                    log::info!(
                        "[Plugin] Loaded state: {} active plugins",
                        state.active_plugins.len()
                    );
                    state
                }
                Err(e) => {
                    log::warn!("[Plugin] Failed to parse state file: {}", e);
                    PersistedState::default()
                }
            },
            Err(e) => {
                log::warn!("[Plugin] Failed to read state file: {}", e);
                PersistedState::default()
            }
        }
    }

    /// Discover all plugins in the plugin directory
    pub async fn discover(&mut self) -> Result<Vec<String>, PluginError> {
        log::info!("[Plugin] Discovering plugins in: {:?}", self.plugin_dir);

        // Create plugin directory if it doesn't exist
        if !self.plugin_dir.exists() {
            tokio::fs::create_dir_all(&self.plugin_dir).await?;
            log::info!("[Plugin] Created plugin directory");
            return Ok(Vec::new());
        }

        self.builtin_plugins.clear();

        let mut discovered_ids = Vec::new();
        let mut entries = tokio::fs::read_dir(&self.plugin_dir).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_dir() {
                let manifest_path = path.join("manifest.json");
                if manifest_path.exists() {
                    match self.load_manifest(&manifest_path).await {
                        Ok(manifest) => {
                            let id = manifest.id.clone();

                            // Preserve existing state if plugin is already loaded/active
                            let existing_state = self.discovered.get(&id).map(|d| d.state.clone());
                            let instance_state = self.instances.get(&id).map(|i| i.state.clone());

                            let state = instance_state
                                .or(existing_state)
                                .unwrap_or(PluginState::Discovered);

                            let is_builtin = manifest.is_builtin;
                            if is_builtin {
                                self.builtin_plugins.insert(id.clone());
                            }

                            self.discovered.insert(
                                id.clone(),
                                DiscoveredPlugin {
                                    manifest,
                                    path,
                                    state,
                                },
                            );
                            discovered_ids.push(id);
                        }
                        Err(e) => {
                            log::warn!("[Plugin] Failed to load manifest at {:?}: {}", path, e);
                        }
                    }
                }
            }
        }

        log::info!("[Plugin] Discovered {} plugins", discovered_ids.len());

        // Restore previously active plugins
        let persisted = self.load_state().await;
        for plugin_id in &persisted.active_plugins {
            if self.discovered.contains_key(plugin_id) {
                log::info!(
                    "[Plugin] Auto-loading previously active plugin: {}",
                    plugin_id
                );
                // Restore granted permissions first
                if let Some(perms) = persisted.granted_permissions.get(plugin_id) {
                    for perm in perms {
                        let _ = self.permission_checker.grant(plugin_id, perm);
                    }
                }
                // Try to load and activate
                match self.load(plugin_id).await {
                    Ok(_) => {
                        if let Err(e) = self.activate(plugin_id).await {
                            log::warn!("[Plugin] Failed to auto-activate {}: {}", plugin_id, e);
                        }
                    }
                    Err(e) => {
                        log::warn!("[Plugin] Failed to auto-load {}: {}", plugin_id, e);
                    }
                }
            }
        }

        Ok(discovered_ids)
    }

    /// Load plugin manifest from file
    async fn load_manifest(&self, path: &PathBuf) -> Result<PluginManifest, PluginError> {
        let content = tokio::fs::read_to_string(path).await?;
        let manifest = PluginManifest::from_json(&content)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Get all discovered plugins as PluginInfo
    pub fn get_all(&self) -> Vec<PluginInfo> {
        self.discovered
            .iter()
            .map(|(id, discovered)| {
                // let instance = self.instances.get(id);
                PluginInfo {
                    id: id.clone(),
                    name: discovered.manifest.name.clone(),
                    version: discovered.manifest.version.clone(),
                    description: discovered.manifest.description.clone(),
                    author: discovered.manifest.author.name.clone(),
                    icon: discovered.manifest.icon.clone(),
                    state: discovered.state.clone(),
                    permissions: discovered.manifest.permissions.clone(),
                    plugin_type: discovered.manifest.plugin_type.clone(),
                    is_builtin: discovered.manifest.is_builtin,
                }
            })
            .collect()
    }

    /// Get plugin detail
    pub fn get_detail(&self, plugin_id: &str) -> Option<PluginDetail> {
        let discovered = self.discovered.get(plugin_id)?;
        let instance = self.instances.get(plugin_id);

        Some(PluginDetail {
            manifest: discovered.manifest.clone(),
            state: discovered.state.clone(),
            granted_permissions: self.permission_checker.get_granted(plugin_id),
            config: instance
                .map(|i| i.config.clone())
                .unwrap_or(serde_json::Value::Null),
            statistics: instance.map(|i| i.statistics.clone()).unwrap_or_default(),
        })
    }

    /// Load a plugin
    pub async fn load(&mut self, plugin_id: &str) -> Result<LoadResult, PluginError> {
        let discovered = self
            .discovered
            .get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?
            .clone();

        // Check if already loaded
        if self.instances.contains_key(plugin_id) {
            return Ok(LoadResult::Success { success: true });
        }

        let unresolved_permissions: Vec<PermissionRequest> = discovered
            .manifest
            .permissions
            .iter()
            .filter(|permission| {
                !self
                    .permission_checker
                    .is_granted(plugin_id, &permission.permission)
            })
            .cloned()
            .collect();

        // Evaluate permissions that have not already been granted.
        let mut evaluation = self
            .permission_checker
            .evaluate_requests(&unresolved_permissions);

        // Auto-grant low-risk permissions
        for perm in &evaluation.auto_grant {
            self.permission_checker
                .grant(plugin_id, &perm.permission)
                .map_err(PluginError::Permission)?;
        }

        // For builtin plugins, auto-grant ALL permissions (skip confirmation)
        let is_builtin = self.builtin_plugins.contains(plugin_id);
        if is_builtin && !evaluation.need_confirm.is_empty() {
            for perm in &evaluation.need_confirm {
                self.permission_checker
                    .grant(plugin_id, &perm.permission)
                    .map_err(PluginError::Permission)?;
            }
            evaluation.need_confirm.clear();
        }

        // Check if there are permissions needing confirmation
        if !evaluation.need_confirm.is_empty() {
            // Update state to pending permission
            if let Some(d) = self.discovered.get_mut(plugin_id) {
                d.state = PluginState::PendingPermission(evaluation.need_confirm.clone());
            }
            return Ok(LoadResult::PermissionRequired {
                permissions: evaluation.need_confirm,
            });
        }

        // Create plugin instance
        let instance = self.create_instance(&discovered).await?;

        // Store instance
        self.instances.insert(plugin_id.to_string(), instance);

        // Update state
        if let Some(d) = self.discovered.get_mut(plugin_id) {
            d.state = PluginState::Loaded;
        }

        log::info!("[Plugin] Loaded plugin: {}", plugin_id);
        Ok(LoadResult::Success { success: true })
    }

    /// Create a plugin instance
    async fn create_instance(
        &self,
        discovered: &DiscoveredPlugin,
    ) -> Result<PluginInstance, PluginError> {
        // Get default config
        let config = discovered
            .manifest
            .config_schema
            .as_ref()
            .map(|schema| serde_json::to_value(&schema.default).unwrap_or(serde_json::Value::Null))
            .unwrap_or(serde_json::Value::Null);

        Ok(PluginInstance {
            id: discovered.manifest.id.clone(),
            manifest: discovered.manifest.clone(),
            state: PluginState::Loaded,
            granted_permissions: self.permission_checker.get_granted(&discovered.manifest.id),
            config,
            statistics: PluginStatistics::default(),
        })
    }

    /// Activate a plugin
    pub async fn activate(&mut self, plugin_id: &str) -> Result<(), PluginError> {
        // Check if plugin is discovered (used for validation only)
        let _ = self
            .discovered
            .get(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;

        // Check if loaded
        if !self.instances.contains_key(plugin_id) {
            return Err(PluginError::InvalidState(
                "Plugin must be loaded before activation".to_string(),
            ));
        }

        // Update instance state
        if let Some(instance) = self.instances.get_mut(plugin_id) {
            instance.state = PluginState::Active;
            instance.statistics.activated_count += 1;
            instance.statistics.last_activated = Some(Utc::now());
        }

        // Update discovered state
        if let Some(d) = self.discovered.get_mut(plugin_id) {
            d.state = PluginState::Active;
        }

        // Persist state
        let _ = self.save_state().await;

        log::info!("[Plugin] Activated plugin: {}", plugin_id);
        Ok(())
    }

    /// Auto-activate all builtin plugins
    pub async fn auto_activate_builtin(&mut self) -> Result<(), PluginError> {
        let builtin_ids: Vec<String> = self.builtin_plugins.iter().cloned().collect();

        for plugin_id in builtin_ids {
            log::info!("[Plugin] Auto-loading builtin plugin: {}", plugin_id);

            match self.load(&plugin_id).await {
                Ok(_) => {
                    if let Err(e) = self.activate(&plugin_id).await {
                        log::warn!("[Plugin] Failed to activate builtin {}: {}", plugin_id, e);
                    } else {
                        log::info!("[Plugin] Auto-activated builtin plugin: {}", plugin_id);
                    }
                }
                Err(e) => {
                    log::warn!("[Plugin] Failed to load builtin {}: {}", plugin_id, e);
                }
            }
        }

        Ok(())
    }

    /// Deactivate a plugin
    pub async fn deactivate(&mut self, plugin_id: &str) -> Result<(), PluginError> {
        // Check if active
        let is_active = self
            .discovered
            .get(plugin_id)
            .map(|d| d.state.is_active())
            .unwrap_or(false);

        if !is_active {
            return Err(PluginError::InvalidState(
                "Plugin is not active".to_string(),
            ));
        }

        // Update instance state
        if let Some(instance) = self.instances.get_mut(plugin_id) {
            instance.state = PluginState::Inactive;
        }

        // Update discovered state
        if let Some(d) = self.discovered.get_mut(plugin_id) {
            d.state = PluginState::Inactive;
        }

        // Persist state
        let _ = self.save_state().await;

        log::info!("[Plugin] Deactivated plugin: {}", plugin_id);
        Ok(())
    }

    /// Unload a plugin
    pub async fn unload(&mut self, plugin_id: &str) -> Result<(), PluginError> {
        // Deactivate first if active
        if self
            .discovered
            .get(plugin_id)
            .map(|d| d.state.is_active())
            .unwrap_or(false)
        {
            self.deactivate(plugin_id).await?;
        }

        // Remove instance
        self.instances.remove(plugin_id);

        // Revoke all permissions
        self.permission_checker.revoke_all(plugin_id);

        // Update state
        if let Some(d) = self.discovered.get_mut(plugin_id) {
            d.state = PluginState::Unloaded;
        }

        // Persist state
        let _ = self.save_state().await;

        log::info!("[Plugin] Unloaded plugin: {}", plugin_id);
        Ok(())
    }

    /// Grant permission to a plugin
    pub fn grant_permission(
        &mut self,
        plugin_id: &str,
        permission: &str,
    ) -> Result<(), PluginError> {
        self.permission_checker
            .grant(plugin_id, permission)
            .map_err(PluginError::Permission)?;

        // Update instance granted permissions
        if let Some(instance) = self.instances.get_mut(plugin_id) {
            instance.granted_permissions = self.permission_checker.get_granted(plugin_id);
        }

        log::info!(
            "[Plugin] Granted permission '{}' to plugin '{}'",
            permission,
            plugin_id
        );
        Ok(())
    }

    /// Get plugin configuration
    pub fn get_config(&self, plugin_id: &str) -> Option<&serde_json::Value> {
        self.instances
            .get(plugin_id)
            .map(|i| &i.config)
            .or_else(|| self.configs.get(plugin_id))
    }

    /// Update plugin configuration
    pub fn update_config(
        &mut self,
        plugin_id: &str,
        config: serde_json::Value,
    ) -> Result<(), PluginError> {
        if let Some(instance) = self.instances.get_mut(plugin_id) {
            instance.config = config.clone();
        }
        self.configs.insert(plugin_id.to_string(), config);

        log::info!("[Plugin] Updated config for plugin: {}", plugin_id);
        Ok(())
    }

    /// Get plugin state
    pub fn get_state(&self, plugin_id: &str) -> Option<&PluginState> {
        self.discovered.get(plugin_id).map(|d| &d.state)
    }

    /// Get plugin path
    pub fn get_plugin_path(&self, plugin_id: &str) -> Option<&PathBuf> {
        self.discovered.get(plugin_id).map(|d| &d.path)
    }

    /// Get plugin manifest
    pub fn get_manifest(&self, plugin_id: &str) -> Option<&PluginManifest> {
        self.discovered.get(plugin_id).map(|d| &d.manifest)
    }

    /// Check if plugin exists
    pub fn exists(&self, plugin_id: &str) -> bool {
        self.discovered.contains_key(plugin_id)
    }

    /// Get plugin directory path
    pub fn get_plugin_dir(&self) -> &PathBuf {
        &self.plugin_dir
    }

    /// Get permission checker (for IPC commands)
    pub fn get_permission_checker(&self) -> &PermissionChecker {
        &self.permission_checker
    }

    /// Get mutable permission checker (for IPC commands)
    pub fn get_permission_checker_mut(&mut self) -> &mut PermissionChecker {
        &mut self.permission_checker
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_info_serialization() {
        let info = PluginInfo {
            id: "com.example.test".to_string(),
            name: "Test Plugin".to_string(),
            version: "1.0.0".to_string(),
            description: "A test plugin".to_string(),
            author: "Test Author".to_string(),
            icon: Some("icon.png".to_string()),
            state: PluginState::Active,
            permissions: vec![PermissionRequest {
                permission: "data:read".to_string(),
                reason: "Need to read data".to_string(),
                required: true,
            }],
            plugin_type: PluginType::Transform,
            is_builtin: false,
        };

        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("com.example.test"));
        assert!(json.contains("Test Plugin"));
    }

    #[test]
    fn test_load_result_serialization() {
        let success = LoadResult::Success { success: true };
        let json = serde_json::to_string(&success).unwrap();
        assert_eq!(json, r#"{"success":true}"#);

        let pending = LoadResult::PermissionRequired {
            permissions: vec![PermissionRequest {
                permission: "data:read".to_string(),
                reason: "Need to read data".to_string(),
                required: true,
            }],
        };
        let json = serde_json::to_string(&pending).unwrap();
        assert!(json.contains("permissionRequired"));
    }
}
