use std::path::PathBuf;
use tauri::Manager;

pub const PORTABLE_MARKERS: &[&str] = &["portable", "cliporax.portable"];
pub const PORTABLE_DATA_DIR: &str = "data";
const PRODUCTION_IDENTIFIER: &str = "com.cliporax.app";
const LEGACY_SETTINGS_DIR: &str = "cliporax";

fn executable_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
}

fn portable_data_dir(identifier: &str) -> Option<PathBuf> {
    let exe_dir = executable_dir()?;
    let data_dir = exe_dir.join(portable_data_dir_name(identifier));

    let has_marker = PORTABLE_MARKERS
        .iter()
        .any(|marker| exe_dir.join(marker).exists());
    let has_existing_data =
        data_dir.join("cliporax.db").exists() || data_dir.join("settings.json").exists();

    (has_marker || has_existing_data).then_some(data_dir)
}

fn portable_data_dir_name(identifier: &str) -> String {
    if identifier == PRODUCTION_IDENTIFIER {
        PORTABLE_DATA_DIR.to_string()
    } else {
        let variant = identifier
            .rsplit('.')
            .next()
            .filter(|value| !value.is_empty())
            .unwrap_or("variant");
        format!("{}-{}", PORTABLE_DATA_DIR, variant)
    }
}

fn writable_portable_data_dir(identifier: &str) -> Option<PathBuf> {
    let data_dir = portable_data_dir(identifier)?;

    if let Err(e) = std::fs::create_dir_all(&data_dir) {
        log::warn!(
            "[Portable] Portable data directory is not writable, falling back to system app data: {:?}: {}",
            data_dir,
            e
        );
        return None;
    }

    let probe_path = data_dir.join(".cliporax-write-test");
    match std::fs::write(&probe_path, b"ok") {
        Ok(_) => {
            let _ = std::fs::remove_file(&probe_path);
            Some(data_dir)
        }
        Err(e) => {
            log::warn!(
                "[Portable] Portable write probe failed, falling back to system app data: {:?}: {}",
                probe_path,
                e
            );
            None
        }
    }
}

pub fn app_data_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Some(dir) = writable_portable_data_dir(&app_handle.config().identifier) {
        log::info!("[Portable] Using portable app data directory: {:?}", dir);
        return Ok(dir);
    }

    app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))
}

pub fn settings_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Some(dir) = writable_portable_data_dir(&app_handle.config().identifier) {
        log::info!("[Portable] Using portable settings directory: {:?}", dir);
        return Ok(dir);
    }

    let config_dir = app_handle
        .path()
        .config_dir()
        .map_err(|e| format!("Failed to get config dir: {}", e))?;
    Ok(system_settings_dir(
        config_dir,
        &app_handle.config().identifier,
    ))
}

fn system_settings_dir(config_dir: PathBuf, identifier: &str) -> PathBuf {
    // Keep the production settings in their historical location so existing users
    // do not lose preferences. Other build variants use their unique identifier.
    if identifier == PRODUCTION_IDENTIFIER {
        config_dir.join(LEGACY_SETTINGS_DIR)
    } else {
        config_dir.join(identifier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn production_settings_keep_the_legacy_location() {
        let config_dir = PathBuf::from("config-root");

        assert_eq!(
            system_settings_dir(config_dir.clone(), PRODUCTION_IDENTIFIER),
            config_dir.join(LEGACY_SETTINGS_DIR)
        );
    }

    #[test]
    fn development_settings_use_the_development_identifier() {
        let config_dir = PathBuf::from("config-root");

        assert_eq!(
            system_settings_dir(config_dir.clone(), "com.cliporax.app.dev"),
            config_dir.join("com.cliporax.app.dev")
        );
    }

    #[test]
    fn portable_data_directories_are_isolated_by_build_variant() {
        assert_eq!(portable_data_dir_name(PRODUCTION_IDENTIFIER), "data");
        assert_eq!(portable_data_dir_name("com.cliporax.app.dev"), "data-dev");
    }
}
