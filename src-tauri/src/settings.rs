use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

const SETTINGS_FILE: &str = "settings.json";

/// All application settings stored in a single JSON file
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    // General settings
    pub theme: String,
    pub max_items: i32,
    pub max_images: i32,
    pub line_height: String,
    pub auto_start: bool,
    pub auto_hide: bool,

    // Shortcut settings
    pub shortcut_toggle_window: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "dark".to_string(),
            max_items: 1000,
            max_images: 500,
            line_height: "medium".to_string(),
            auto_start: false,
            auto_hide: true,
            shortcut_toggle_window: "CmdOrControl+Shift+V".to_string(),
        }
    }
}

/// Settings manager that handles loading and saving to JSON file
pub struct SettingsManager {
    settings_path: PathBuf,
    settings: AppSettings,
}

impl SettingsManager {
    /// Initialize settings manager and load settings from file
    pub fn new(app_handle: &tauri::AppHandle) -> Result<Self, Box<dyn std::error::Error>> {
        // Use config_dir for settings file (XDG Base Directory Specification)
        // Linux: ~/.config/cliporax/settings.json
        // Windows: %APPDATA%/cliporax/settings.json
        // macOS: ~/Library/Application Support/cliporax/settings.json
        let app_config_dir = crate::portable::settings_dir(app_handle)?;
        let settings_path = app_config_dir.join(SETTINGS_FILE);
        log::info!("[SettingsManager] Settings file path: {:?}", settings_path);

        // Load existing settings or create default
        let settings = if settings_path.exists() {
            match Self::load_from_file(&settings_path) {
                Ok(settings) => {
                    log::info!("[SettingsManager] Loaded settings from file");
                    settings
                }
                Err(e) => {
                    log::warn!(
                        "[SettingsManager] Failed to load settings: {}, using default",
                        e
                    );
                    AppSettings::default()
                }
            }
        } else {
            log::info!("[SettingsManager] Settings file not found, using default");
            let default_settings = AppSettings::default();
            // Create the file with default settings
            if let Err(e) = Self::save_to_file(&settings_path, &default_settings) {
                log::error!(
                    "[SettingsManager] Failed to create default settings file: {}",
                    e
                );
            }
            default_settings
        };

        Ok(Self {
            settings_path,
            settings,
        })
    }

    /// Load settings from JSON file
    fn load_from_file(path: &PathBuf) -> Result<AppSettings, Box<dyn std::error::Error>> {
        let content =
            fs::read_to_string(path).map_err(|e| format!("Failed to read settings file: {}", e))?;
        let settings: AppSettings = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse settings JSON: {}", e))?;
        Ok(settings)
    }

    /// Save settings to JSON file
    fn save_to_file(
        path: &PathBuf,
        settings: &AppSettings,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create settings directory: {}", e))?;
        }

        let content = serde_json::to_string_pretty(settings)
            .map_err(|e| format!("Failed to serialize settings: {}", e))?;
        fs::write(path, content).map_err(|e| format!("Failed to write settings file: {}", e))?;

        log::info!("[SettingsManager] Settings saved to file");
        Ok(())
    }

    /// Get current settings
    pub fn get(&self) -> &AppSettings {
        &self.settings
    }

    /// Get mutable reference to settings
    pub fn get_mut(&mut self) -> &mut AppSettings {
        &mut self.settings
    }

    /// Save current settings to file
    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        Self::save_to_file(&self.settings_path, &self.settings)
    }

    /// Update a specific setting and save
    pub fn update<F>(&mut self, updater: F) -> Result<(), Box<dyn std::error::Error>>
    where
        F: FnOnce(&mut AppSettings),
    {
        updater(&mut self.settings);
        self.save()
    }

    /// Get the settings file path
    pub fn settings_path(&self) -> &PathBuf {
        &self.settings_path
    }
}

/// Global settings instance managed by Tauri
pub type SettingsState = std::sync::Mutex<SettingsManager>;

/// Initialize settings manager and add to Tauri state
pub fn init_settings(
    app_handle: &tauri::AppHandle,
) -> Result<SettingsManager, Box<dyn std::error::Error>> {
    SettingsManager::new(app_handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_settings() {
        let settings = AppSettings::default();
        assert_eq!(settings.theme, "dark");
        assert_eq!(settings.max_items, 1000);
        assert_eq!(settings.shortcut_toggle_window, "CmdOrControl+Shift+V");
    }

    #[test]
    fn test_save_and_load_settings() {
        let temp_file = NamedTempFile::new().unwrap();
        let path = temp_file.path().to_path_buf();

        // Create and save settings
        let settings = AppSettings {
            theme: "light".to_string(),
            max_items: 500,
            max_images: 250,
            line_height: "large".to_string(),
            auto_start: true,
            auto_hide: false,
            shortcut_toggle_window: "CmdOrControl+Shift+A".to_string(),
        };

        SettingsManager::save_to_file(&path, &settings).unwrap();

        // Load settings back
        let loaded = SettingsManager::load_from_file(&path).unwrap();
        assert_eq!(loaded.theme, "light");
        assert_eq!(loaded.max_items, 500);
        assert_eq!(loaded.shortcut_toggle_window, "CmdOrControl+Shift+A");
    }
}
