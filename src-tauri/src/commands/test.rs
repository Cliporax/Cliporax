//! Test data commands for development

use crate::db::{ClipboardRepository, Db, TabRepository};

/// Debug: List all tabs with their item counts
#[tauri::command]
pub async fn test_debug_tabs(db: tauri::State<'_, Db>) -> Result<String, String> {
    log::info!("[Command] test_debug_tabs called");

    let tabs = TabRepository::get_all(&db)
        .await
        .map_err(|e| e.to_string())?;

    let mut result = String::from("Tabs:\n");
    for tab in &tabs {
        let id = tab.id.unwrap_or(-1);
        let item_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM clipboard_items WHERE tab_id = ?")
                .bind(id)
                .fetch_one(db.inner())
                .await
                .map_err(|e| e.to_string())?;

        result.push_str(&format!(
            "  ID: {}, Name: '{}', is_default: {}, auto_capture: {}, items: {}\n",
            id,
            tab.name,
            tab.is_default.unwrap_or(0),
            tab.auto_capture.unwrap_or(0),
            item_count
        ));
    }

    log::info!("[Command] test_debug_tabs result:\n{}", result);
    Ok(result)
}

/// Insert batch test data
#[tauri::command]
pub async fn test_insert_batch(db: tauri::State<'_, Db>, count: i64) -> Result<i64, String> {
    log::info!("[Command] test_insert_batch called - count: {}", count);

    // Get default tab
    let default_tab = TabRepository::get_default_tab(&db).await.map_err(|e| {
        log::error!(
            "[Command] test_insert_batch failed to get default tab: {}",
            e
        );
        e.to_string()
    })?;
    let tab_id = default_tab.id.unwrap();

    match ClipboardRepository::batch_insert_test_data(&db, count, tab_id).await {
        Ok(inserted) => {
            log::info!(
                "[Command] test_insert_batch success - inserted {} items",
                inserted
            );
            Ok(inserted)
        }
        Err(e) => {
            log::error!("[Command] test_insert_batch failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Clear all clipboard data
#[tauri::command]
pub async fn test_clear_all(db: tauri::State<'_, Db>) -> Result<(), String> {
    log::info!("[Command] test_clear_all called");
    match ClipboardRepository::clear_all(&db).await {
        Ok(_) => {
            log::info!("[Command] test_clear_all success");
            Ok(())
        }
        Err(e) => {
            log::error!("[Command] test_clear_all failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Delete empty tabs (tabs with no items)
#[tauri::command]
pub async fn test_delete_empty_tabs(db: tauri::State<'_, Db>) -> Result<String, String> {
    log::info!("[Command] test_delete_empty_tabs called");

    let tabs = TabRepository::get_all(&db)
        .await
        .map_err(|e| e.to_string())?;
    let mut deleted = Vec::new();

    for tab in &tabs {
        let id = tab.id.unwrap_or(-1);

        // Skip default tabs
        if tab.is_default.unwrap_or(0) == 1 {
            continue;
        }

        let item_count =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM clipboard_items WHERE tab_id = ?")
                .bind(id)
                .fetch_one(db.inner())
                .await
                .map_err(|e| e.to_string())?;

        if item_count == 0 {
            // Delete this empty tab
            sqlx::query("DELETE FROM tabs WHERE id = ?")
                .bind(id)
                .execute(db.inner())
                .await
                .map_err(|e| e.to_string())?;

            deleted.push(format!("  Deleted tab: ID={}, Name='{}'", id, tab.name));
            log::info!(
                "[Command] Deleted empty tab: ID={}, Name='{}'",
                id,
                tab.name
            );
        }
    }

    if deleted.is_empty() {
        Ok("No empty tabs found".to_string())
    } else {
        let result = format!(
            "Deleted {} empty tabs:\n{}",
            deleted.len(),
            deleted.join("\n")
        );
        log::info!("[Command] test_delete_empty_tabs result:\n{}", result);
        Ok(result)
    }
}
