//! Tab management commands

use crate::db::{Db, Tab, TabRepository};

const MAX_TAB_NAME_LEN: usize = 64;

fn validate_tab_id(id: i64) -> Result<(), String> {
    if id <= 0 {
        return Err("Tab id must be positive".to_string());
    }
    Ok(())
}

fn validate_tab_name(name: &str) -> Result<&str, String> {
    let trimmed_name = name.trim();
    if trimmed_name.is_empty() {
        return Err("Tab name cannot be empty".to_string());
    }
    if trimmed_name.chars().count() > MAX_TAB_NAME_LEN {
        return Err(format!(
            "Tab name cannot exceed {} characters",
            MAX_TAB_NAME_LEN
        ));
    }
    Ok(trimmed_name)
}

/// Get all tabs
#[tauri::command]
pub async fn tabs_get_all(db: tauri::State<'_, Db>) -> Result<Vec<Tab>, String> {
    log::info!("[Command] tabs_get_all called");
    match TabRepository::get_all(&db).await {
        Ok(tabs) => {
            log::info!("[Command] tabs_get_all returned {} tabs", tabs.len());
            Ok(tabs)
        }
        Err(e) => {
            log::error!("[Command] tabs_get_all failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Create a new tab
#[tauri::command]
pub async fn tabs_create(db: tauri::State<'_, Db>, name: String) -> Result<i64, String> {
    log::info!("[Command] tabs_create called with name: {}", name);
    let trimmed_name = validate_tab_name(&name)?;
    match TabRepository::create(&db, trimmed_name).await {
        Ok(id) => {
            log::info!("[Command] tabs_create success, id: {}", id);
            Ok(id)
        }
        Err(e) => {
            log::error!("[Command] tabs_create failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Delete a tab
#[tauri::command]
pub async fn tabs_delete(db: tauri::State<'_, Db>, id: i64) -> Result<(), String> {
    log::info!("[Command] tabs_delete called with id: {}", id);
    validate_tab_id(id)?;
    match TabRepository::delete(&db, id).await {
        Ok(_) => {
            log::info!("[Command] tabs_delete success");
            Ok(())
        }
        Err(e) => {
            log::error!("[Command] tabs_delete failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Rename a tab
#[tauri::command]
pub async fn tabs_rename(db: tauri::State<'_, Db>, id: i64, name: String) -> Result<(), String> {
    log::info!("[Command] tabs_rename called - id: {}, name: {}", id, name);
    validate_tab_id(id)?;
    let trimmed_name = validate_tab_name(&name)?;

    // Check if this is a default tab
    let tabs = TabRepository::get_all(&db)
        .await
        .map_err(|e| e.to_string())?;
    let tab = tabs.iter().find(|t| t.id == Some(id));

    if let Some(tab) = tab {
        if tab.is_default.unwrap_or(0) == 1 {
            return Err("Cannot rename default tab".to_string());
        }
    } else {
        return Err("Tab not found".to_string());
    }

    // Check for name conflicts with reserved names
    let reserved_names = ["Default", "System Clipboard"];
    if reserved_names
        .iter()
        .any(|n| n.eq_ignore_ascii_case(trimmed_name))
    {
        return Err(format!("Tab name '{}' is reserved", trimmed_name));
    }

    // Check for duplicate names with other tabs
    if tabs
        .iter()
        .any(|t| t.id != Some(id) && t.name.eq_ignore_ascii_case(trimmed_name))
    {
        return Err("Tab name already exists".to_string());
    }

    match TabRepository::rename(&db, id, trimmed_name).await {
        Ok(_) => {
            log::info!("[Command] tabs_rename success");
            Ok(())
        }
        Err(e) => {
            log::error!("[Command] tabs_rename failed: {}", e);
            Err(e.to_string())
        }
    }
}
