use std::path::PathBuf;
use tauri::Manager;

pub const PORTABLE_MARKERS: &[&str] = &["portable", "cliporax.portable"];
pub const PORTABLE_DATA_DIR: &str = "data";

fn executable_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.to_path_buf()))
}

pub fn portable_data_dir() -> Option<PathBuf> {
    let exe_dir = executable_dir()?;
    let data_dir = exe_dir.join(PORTABLE_DATA_DIR);

    let has_marker = PORTABLE_MARKERS
        .iter()
        .any(|marker| exe_dir.join(marker).exists());
    let has_existing_data =
        data_dir.join("cliporax.db").exists() || data_dir.join("settings.json").exists();

    (has_marker || has_existing_data).then_some(data_dir)
}

fn writable_portable_data_dir() -> Option<PathBuf> {
    let data_dir = portable_data_dir()?;

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
    if let Some(dir) = writable_portable_data_dir() {
        log::info!("[Portable] Using portable app data directory: {:?}", dir);
        return Ok(dir);
    }

    app_handle
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data directory: {}", e))
}

pub fn settings_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Some(dir) = writable_portable_data_dir() {
        log::info!("[Portable] Using portable settings directory: {:?}", dir);
        return Ok(dir);
    }

    let config_dir = app_handle
        .path()
        .config_dir()
        .map_err(|e| format!("Failed to get config dir: {}", e))?;
    Ok(config_dir.join("cliporax"))
}
