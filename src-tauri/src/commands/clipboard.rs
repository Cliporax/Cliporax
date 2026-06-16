//! Clipboard item management commands

use crate::clipboard::ClipboardMonitor;
use crate::db::{ClipboardItem, ClipboardItemInput, ClipboardRepository, Db};
use std::sync::Arc;
use tauri::Emitter;

const MAX_PAGE_LIMIT: i64 = 500;
const MAX_BATCH_IDS: usize = 1000;
const MAX_SEARCH_QUERY_LEN: usize = 512;
const MAX_TAG_COUNT: usize = 32;
const MAX_TAG_LEN: usize = 64;
const MAX_TEXT_CONTENT_LEN: usize = 1_048_576;
const MAX_IMAGE_CONTENT_LEN: usize = 52_428_800;

fn validate_positive_id(name: &str, id: i64) -> Result<(), String> {
    if id <= 0 {
        return Err(format!("{} must be positive", name));
    }
    Ok(())
}

fn normalize_page(limit: Option<i64>, offset: Option<i64>) -> Result<(i64, i64), String> {
    let limit = limit.unwrap_or(50);
    let offset = offset.unwrap_or(0);

    if limit <= 0 {
        return Err("limit must be positive".to_string());
    }
    if offset < 0 {
        return Err("offset cannot be negative".to_string());
    }

    Ok((limit.min(MAX_PAGE_LIMIT), offset))
}

fn validate_ids(ids: &[i64]) -> Result<(), String> {
    if ids.is_empty() {
        return Err("ids cannot be empty".to_string());
    }
    if ids.len() > MAX_BATCH_IDS {
        return Err(format!("ids cannot exceed {} items", MAX_BATCH_IDS));
    }
    if ids.iter().any(|id| *id <= 0) {
        return Err("ids must all be positive".to_string());
    }
    Ok(())
}

fn validate_item_type(item_type: &str) -> Result<(), String> {
    match item_type {
        "text" | "image" | "file" => Ok(()),
        _ => Err(format!("Invalid item type: {}", item_type)),
    }
}

fn validate_content(item_type: &str, content: &str) -> Result<(), String> {
    if content.is_empty() {
        return Err("content cannot be empty".to_string());
    }

    let max_len = if item_type == "image" {
        MAX_IMAGE_CONTENT_LEN
    } else {
        MAX_TEXT_CONTENT_LEN
    };
    if content.len() > max_len {
        return Err(format!("content cannot exceed {} bytes", max_len));
    }
    Ok(())
}

fn validate_tags(tags: &[String]) -> Result<(), String> {
    if tags.len() > MAX_TAG_COUNT {
        return Err(format!("tags cannot exceed {} items", MAX_TAG_COUNT));
    }
    for tag in tags {
        let trimmed = tag.trim();
        if trimmed.is_empty() {
            return Err("tags cannot contain empty values".to_string());
        }
        if trimmed.chars().count() > MAX_TAG_LEN {
            return Err(format!("tag cannot exceed {} characters", MAX_TAG_LEN));
        }
    }
    Ok(())
}

/// Get clipboard items by tab
#[tauri::command]
pub async fn clipboard_get_by_tab(
    db: tauri::State<'_, Db>,
    tab_id: i64,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<ClipboardItem>, String> {
    validate_positive_id("tab_id", tab_id)?;
    let (limit, offset) = normalize_page(limit, offset)?;
    log::info!(
        "[Command] clipboard_get_by_tab called - tab_id: {}, limit: {}, offset: {}",
        tab_id,
        limit,
        offset
    );

    match ClipboardRepository::get_by_tab(&db, tab_id, limit, offset).await {
        Ok(items) => {
            log::info!(
                "[Command] clipboard_get_by_tab returned {} items",
                items.len()
            );
            for item in &items {
                log::debug!(
                    "[Command] Item id: {}, type: {}, content_len: {}",
                    item.id.unwrap_or(-1),
                    item.item_type,
                    item.content.len()
                );
            }
            Ok(items)
        }
        Err(e) => {
            log::error!("[Command] clipboard_get_by_tab failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Get the latest clipboard item for incremental updates
#[tauri::command]
pub async fn clipboard_get_latest(
    db: tauri::State<'_, Db>,
    tab_id: i64,
) -> Result<Option<ClipboardItem>, String> {
    log::info!("[Command] clipboard_get_latest called - tab_id: {}", tab_id);
    validate_positive_id("tab_id", tab_id)?;

    match ClipboardRepository::get_by_tab(&db, tab_id, 1, 0).await {
        Ok(items) => {
            let item = items.into_iter().next();
            if let Some(ref i) = item {
                log::info!(
                    "[Command] clipboard_get_latest returned item id: {}, type: {}",
                    i.id.unwrap_or(-1),
                    i.item_type
                );
            } else {
                log::info!("[Command] clipboard_get_latest returned no items");
            }
            Ok(item)
        }
        Err(e) => {
            log::error!("[Command] clipboard_get_latest failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Create a new clipboard item
#[tauri::command]
pub async fn clipboard_create(
    db: tauri::State<'_, Db>,
    app_handle: tauri::AppHandle,
    item: ClipboardItemInput,
) -> Result<i64, String> {
    validate_item_type(&item.item_type)?;
    validate_content(&item.item_type, &item.content)?;
    if let Some(tab_id) = item.tab_id {
        validate_positive_id("tab_id", tab_id)?;
    }
    log::info!(
        "[Command] clipboard_create called - type: {}, content_len: {}",
        item.item_type,
        item.content.len()
    );
    match ClipboardRepository::create(&db, item).await {
        Ok(id) => {
            log::info!("[Command] clipboard_create success, id: {}", id);
            let _ = app_handle.emit("clipboard:changed", ());
            Ok(id)
        }
        Err(e) => {
            log::error!("[Command] clipboard_create failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Delete a clipboard item
#[tauri::command]
pub async fn clipboard_delete(db: tauri::State<'_, Db>, id: i64) -> Result<(), String> {
    log::info!("[Command] clipboard_delete called with id: {}", id);
    validate_positive_id("id", id)?;
    match ClipboardRepository::delete(&db, id).await {
        Ok(_) => {
            log::info!("[Command] clipboard_delete success");
            Ok(())
        }
        Err(e) => {
            log::error!("[Command] clipboard_delete failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Toggle pin status of a clipboard item
#[tauri::command]
pub async fn clipboard_toggle_pin(
    db: tauri::State<'_, Db>,
    id: i64,
    is_pinned: i32,
) -> Result<(), String> {
    validate_positive_id("id", id)?;
    if is_pinned != 0 && is_pinned != 1 {
        return Err("is_pinned must be 0 or 1".to_string());
    }
    log::info!(
        "[Command] clipboard_toggle_pin called - id: {}, is_pinned: {}",
        id,
        is_pinned
    );
    match ClipboardRepository::toggle_pin(&db, id, is_pinned).await {
        Ok(_) => {
            log::info!("[Command] clipboard_toggle_pin success");
            Ok(())
        }
        Err(e) => {
            log::error!("[Command] clipboard_toggle_pin failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Move a clipboard item to the top
#[tauri::command]
pub async fn clipboard_move_to_top(
    db: tauri::State<'_, Db>,
    app_handle: tauri::AppHandle,
    id: i64,
) -> Result<(), String> {
    log::info!("[Command] clipboard_move_to_top called with id: {}", id);
    validate_positive_id("id", id)?;
    match ClipboardRepository::move_to_top(&db, id).await {
        Ok(_) => {
            log::info!("[Command] clipboard_move_to_top success");
            // Notify frontend to refresh the list
            let _ = app_handle.emit("clipboard:changed", ());
            Ok(())
        }
        Err(e) => {
            log::error!("[Command] clipboard_move_to_top failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Search clipboard items
#[tauri::command]
pub async fn clipboard_search(
    db: tauri::State<'_, Db>,
    query: String,
    tab_id: Option<i64>,
) -> Result<Vec<ClipboardItem>, String> {
    let trimmed_query = query.trim();
    if trimmed_query.is_empty() {
        return Err("query cannot be empty".to_string());
    }
    if trimmed_query.chars().count() > MAX_SEARCH_QUERY_LEN {
        return Err(format!(
            "query cannot exceed {} characters",
            MAX_SEARCH_QUERY_LEN
        ));
    }
    if let Some(tab_id) = tab_id {
        validate_positive_id("tab_id", tab_id)?;
    }
    log::info!(
        "[Command] clipboard_search called - query: {}, tab_id: {:?}",
        query,
        tab_id
    );
    match ClipboardRepository::search(&db, trimmed_query, tab_id).await {
        Ok(items) => {
            log::info!("[Command] clipboard_search returned {} items", items.len());
            Ok(items)
        }
        Err(e) => {
            log::error!("[Command] clipboard_search failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Update tags of a clipboard item
#[tauri::command]
pub async fn clipboard_update_tags(
    db: tauri::State<'_, Db>,
    id: i64,
    tags: Vec<String>,
) -> Result<(), String> {
    validate_positive_id("id", id)?;
    validate_tags(&tags)?;
    log::info!(
        "[Command] clipboard_update_tags called - id: {}, tags: {:?}",
        id,
        tags
    );
    match ClipboardRepository::update_tags(
        &db,
        id,
        &serde_json::to_string(&tags).unwrap_or_default(),
    )
    .await
    {
        Ok(_) => {
            log::info!("[Command] clipboard_update_tags success");
            Ok(())
        }
        Err(e) => {
            log::error!("[Command] clipboard_update_tags failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Update content of a clipboard item
#[tauri::command]
pub async fn clipboard_update_content(
    db: tauri::State<'_, Db>,
    id: i64,
    content: String,
) -> Result<(), String> {
    validate_positive_id("id", id)?;
    validate_content("text", &content)?;
    log::info!(
        "[Command] clipboard_update_content called - id: {}, content_len: {}",
        id,
        content.len()
    );
    match ClipboardRepository::update_content(&db, id, &content).await {
        Ok(_) => {
            log::info!("[Command] clipboard_update_content success");
            Ok(())
        }
        Err(e) => {
            log::error!("[Command] clipboard_update_content failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Clear all sensitive clipboard items
#[tauri::command]
pub async fn clipboard_clear_sensitive(db: tauri::State<'_, Db>) -> Result<(), String> {
    log::info!("[Command] clipboard_clear_sensitive called");
    match ClipboardRepository::clear_sensitive(&db).await {
        Ok(_) => {
            log::info!("[Command] clipboard_clear_sensitive success");
            Ok(())
        }
        Err(e) => {
            log::error!("[Command] clipboard_clear_sensitive failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Get total count of clipboard items in a tab
#[tauri::command]
pub async fn clipboard_get_total_count(
    db: tauri::State<'_, Db>,
    tab_id: i64,
) -> Result<i64, String> {
    validate_positive_id("tab_id", tab_id)?;
    log::info!(
        "[Command] clipboard_get_total_count called - tab_id: {}",
        tab_id
    );
    match ClipboardRepository::get_total_count(&db, tab_id).await {
        Ok(count) => {
            log::info!("[Command] clipboard_get_total_count returned: {}", count);
            Ok(count)
        }
        Err(e) => {
            log::error!("[Command] clipboard_get_total_count failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Get clipboard item at specific index
#[tauri::command]
pub async fn clipboard_get_item_at_index(
    db: tauri::State<'_, Db>,
    tab_id: i64,
    index: i64,
) -> Result<Option<ClipboardItem>, String> {
    validate_positive_id("tab_id", tab_id)?;
    if index < 0 {
        return Err("index cannot be negative".to_string());
    }
    log::info!(
        "[Command] clipboard_get_item_at_index called - tab_id: {}, index: {}",
        tab_id,
        index
    );
    match ClipboardRepository::get_item_at_index(&db, tab_id, index).await {
        Ok(item) => {
            if let Some(ref i) = item {
                log::info!(
                    "[Command] clipboard_get_item_at_index returned item id: {:?}",
                    i.id
                );
            } else {
                log::info!("[Command] clipboard_get_item_at_index returned no item");
            }
            Ok(item)
        }
        Err(e) => {
            log::error!("[Command] clipboard_get_item_at_index failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Delete clipboard items by index range
#[tauri::command]
pub async fn clipboard_delete_by_index_range(
    db: tauri::State<'_, Db>,
    tab_id: i64,
    start_index: i64,
    end_index: i64,
) -> Result<i64, String> {
    validate_positive_id("tab_id", tab_id)?;
    if start_index < 0 || end_index < 0 {
        return Err("index range cannot be negative".to_string());
    }
    if start_index > end_index {
        return Err("start_index cannot be greater than end_index".to_string());
    }
    if end_index - start_index + 1 > MAX_BATCH_IDS as i64 {
        return Err(format!("index range cannot exceed {} items", MAX_BATCH_IDS));
    }
    log::info!(
        "[Command] clipboard_delete_by_index_range called - tab_id: {}, start: {}, end: {}",
        tab_id,
        start_index,
        end_index
    );
    match ClipboardRepository::delete_by_index_range(&db, tab_id, start_index, end_index).await {
        Ok(deleted_count) => {
            log::info!(
                "[Command] clipboard_delete_by_index_range success - deleted {} items",
                deleted_count
            );
            Ok(deleted_count)
        }
        Err(e) => {
            log::error!("[Command] clipboard_delete_by_index_range failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Delete clipboard items by IDs
#[tauri::command]
pub async fn clipboard_delete_by_ids(
    db: tauri::State<'_, Db>,
    ids: Vec<i64>,
) -> Result<i64, String> {
    validate_ids(&ids)?;
    log::info!(
        "[Command] clipboard_delete_by_ids called - count: {}",
        ids.len()
    );
    match ClipboardRepository::delete_by_ids(&db, &ids).await {
        Ok(deleted_count) => {
            log::info!(
                "[Command] clipboard_delete_by_ids success - deleted {} items",
                deleted_count
            );
            Ok(deleted_count)
        }
        Err(e) => {
            log::error!("[Command] clipboard_delete_by_ids failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Get all item types for a tab (for virtual scrolling height calculation)
#[tauri::command]
pub async fn clipboard_get_all_types(
    db: tauri::State<'_, Db>,
    tab_id: i64,
) -> Result<Vec<(i64, String)>, String> {
    validate_positive_id("tab_id", tab_id)?;
    log::info!(
        "[Command] clipboard_get_all_types called - tab_id: {}",
        tab_id
    );
    match ClipboardRepository::get_all_types(&db, tab_id).await {
        Ok(types) => {
            log::info!(
                "[Command] clipboard_get_all_types returned {} items",
                types.len()
            );
            Ok(types)
        }
        Err(e) => {
            log::error!("[Command] clipboard_get_all_types failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Move an item to a new position within the same pin group
#[tauri::command]
pub async fn clipboard_move_item_to_position(
    db: tauri::State<'_, Db>,
    tab_id: i64,
    item_id: i64,
    from_index: i64,
    to_index: i64,
) -> Result<bool, String> {
    validate_positive_id("tab_id", tab_id)?;
    validate_positive_id("item_id", item_id)?;
    if from_index < 0 || to_index < 0 {
        return Err("indexes cannot be negative".to_string());
    }
    log::info!(
        "[Command] clipboard_move_item_to_position called - tab_id: {}, item_id: {}, from: {}, to: {}",
        tab_id,
        item_id,
        from_index,
        to_index
    );
    match ClipboardRepository::move_item_to_position(&db, tab_id, item_id, from_index, to_index)
        .await
    {
        Ok(success) => {
            log::info!(
                "[Command] clipboard_move_item_to_position success: {}",
                success
            );
            Ok(success)
        }
        Err(e) => {
            log::error!("[Command] clipboard_move_item_to_position failed: {}", e);
            Err(e.to_string())
        }
    }
}

/// Copy content to system clipboard
#[tauri::command]
pub async fn clipboard_copy(
    monitor: tauri::State<'_, Arc<ClipboardMonitor>>,
    content: String,
    #[allow(non_snake_case)] itemType: String,
) -> Result<(), String> {
    validate_item_type(&itemType)?;
    validate_content(&itemType, &content)?;
    log::info!(
        "[Command] clipboard_copy called - type: {}, content_len: {}",
        itemType,
        content.len()
    );
    let monitor = monitor.as_ref().clone();
    match itemType.as_str() {
        "text" => {
            log::debug!("[Command] Attempting to copy text to clipboard");
            match monitor.write_text(&content).await {
                Ok(_) => {
                    log::info!("[Command] clipboard_copy text success");
                    Ok(())
                }
                Err(e) => {
                    let error_msg = format!("Failed to write text: {}", e);
                    log::error!("[Command] clipboard_copy text failed: {}", error_msg);
                    Err(error_msg)
                }
            }
        }
        "image" => {
            log::debug!("[Command] Attempting to copy image to clipboard");
            match monitor.write_image(&content).await {
                Ok(_) => {
                    log::info!("[Command] clipboard_copy image success");
                    Ok(())
                }
                Err(e) => {
                    let error_msg = format!("Failed to write image: {}", e);
                    log::error!("[Command] clipboard_copy image failed: {}", error_msg);
                    Err(error_msg)
                }
            }
        }
        "file" => {
            log::debug!("[Command] Attempting to copy file list to clipboard");
            match monitor.write_files(&content).await {
                Ok(_) => {
                    log::info!("[Command] clipboard_copy file success");
                    Ok(())
                }
                Err(e) => {
                    let error_msg = format!("Failed to write file list: {}", e);
                    log::error!("[Command] clipboard_copy file failed: {}", error_msg);
                    Err(error_msg)
                }
            }
        }
        _ => {
            let error_msg = format!("Invalid type: {}", itemType);
            log::error!("[Command] clipboard_copy invalid type: {}", error_msg);
            Err(error_msg)
        }
    }
}

/// Write text to the system clipboard and explicitly save it to Cliporax.
#[tauri::command]
pub async fn clipboard_write_text_and_create(
    db: tauri::State<'_, Db>,
    monitor: tauri::State<'_, Arc<ClipboardMonitor>>,
    app_handle: tauri::AppHandle,
    content: String,
    metadata: Option<String>,
    tags: Option<String>,
    #[allow(non_snake_case)] isSensitive: Option<i32>,
) -> Result<(), String> {
    validate_content("text", &content)?;
    log::info!(
        "[Command] clipboard_write_text_and_create called - content_len: {}",
        content.len()
    );

    let monitor = monitor.as_ref().clone();
    if let Err(e) = monitor.write_text(&content).await {
        let error_msg = format!("Failed to write text: {}", e);
        log::error!(
            "[Command] clipboard_write_text_and_create clipboard write failed: {}",
            error_msg
        );
        return Err(error_msg);
    }

    let item = ClipboardItemInput {
        item_type: "text".to_string(),
        content,
        content_hash: None,
        metadata,
        tags,
        tab_id: None,
        is_sensitive: isSensitive,
        is_pinned: Some(0),
    };

    match ClipboardRepository::create_for_auto_capture_tabs(&db, item).await {
        Ok(ids) => {
            log::info!(
                "[Command] clipboard_write_text_and_create saved {} item(s)",
                ids.len()
            );
            let _ = app_handle.emit("clipboard:changed", ());
            Ok(())
        }
        Err(e) => {
            log::error!(
                "[Command] clipboard_write_text_and_create create failed: {}",
                e
            );
            Err(e.to_string())
        }
    }
}

/// Move a clipboard item to another tab (changes tab_id, deletes from original tab)
#[tauri::command]
pub async fn clipboard_move_to_tab(
    db: tauri::State<'_, Db>,
    item_id: i64,
    target_tab_id: i64,
) -> Result<(), String> {
    validate_positive_id("item_id", item_id)?;
    validate_positive_id("target_tab_id", target_tab_id)?;
    log::info!(
        "[Command] clipboard_move_to_tab called - item_id: {}, target_tab_id: {}",
        item_id,
        target_tab_id
    );

    // Check if target tab exists
    match crate::db::TabRepository::get_by_id(&db, target_tab_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            let error_msg = format!("Target tab not found: {}", target_tab_id);
            log::error!("[Command] clipboard_move_to_tab failed: {}", error_msg);
            return Err(error_msg);
        }
        Err(e) => {
            let error_msg = format!("Failed to query target tab: {}", e);
            log::error!("[Command] clipboard_move_to_tab failed: {}", error_msg);
            return Err(error_msg);
        }
    }

    // Update the item's tab_id
    match sqlx::query(
        "UPDATE clipboard_items SET tab_id = ?, updated_at = datetime('now') WHERE id = ?",
    )
    .bind(target_tab_id)
    .bind(item_id)
    .execute(db.inner())
    .await
    {
        Ok(result) => {
            if result.rows_affected() == 0 {
                let error_msg = format!("Item not found: {}", item_id);
                log::error!("[Command] clipboard_move_to_tab failed: {}", error_msg);
                Err(error_msg)
            } else {
                log::info!(
                    "[Command] clipboard_move_to_tab success - item {} moved to tab {}",
                    item_id,
                    target_tab_id
                );
                Ok(())
            }
        }
        Err(e) => {
            let error_msg = format!("Failed to move item: {}", e);
            log::error!("[Command] clipboard_move_to_tab failed: {}", error_msg);
            Err(error_msg)
        }
    }
}

/// Copy a clipboard item to another tab (creates new item, keeps original)
#[tauri::command]
pub async fn clipboard_copy_to_tab(
    db: tauri::State<'_, Db>,
    item_id: i64,
    target_tab_id: i64,
) -> Result<i64, String> {
    validate_positive_id("item_id", item_id)?;
    validate_positive_id("target_tab_id", target_tab_id)?;
    log::info!(
        "[Command] clipboard_copy_to_tab called - item_id: {}, target_tab_id: {}",
        item_id,
        target_tab_id
    );

    // Check if target tab exists
    match crate::db::TabRepository::get_by_id(&db, target_tab_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            let error_msg = format!("Target tab not found: {}", target_tab_id);
            log::error!("[Command] clipboard_copy_to_tab failed: {}", error_msg);
            return Err(error_msg);
        }
        Err(e) => {
            let error_msg = format!("Failed to query target tab: {}", e);
            log::error!("[Command] clipboard_copy_to_tab failed: {}", error_msg);
            return Err(error_msg);
        }
    }

    // Get the source item
    let source_item = match sqlx::query_as::<_, crate::db::ClipboardItem>(
        "SELECT * FROM clipboard_items WHERE id = ?",
    )
    .bind(item_id)
    .fetch_optional(db.inner())
    .await
    {
        Ok(Some(item)) => item,
        Ok(None) => {
            let error_msg = format!("Source item not found: {}", item_id);
            log::error!("[Command] clipboard_copy_to_tab failed: {}", error_msg);
            return Err(error_msg);
        }
        Err(e) => {
            let error_msg = format!("Failed to query source item: {}", e);
            log::error!("[Command] clipboard_copy_to_tab failed: {}", error_msg);
            return Err(error_msg);
        }
    };

    // Create a new item with the same content but different tab_id
    match sqlx::query(
        r#"
        INSERT INTO clipboard_items 
        (type, content, content_hash, metadata, tags, tab_id, is_sensitive, is_pinned, display_order, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
        "#
    )
    .bind(&source_item.item_type)
    .bind(&source_item.content)
    .bind(&source_item.content_hash)
    .bind(source_item.metadata.as_deref().unwrap_or("{}"))
    .bind(source_item.tags.as_deref().unwrap_or("[]"))
    .bind(target_tab_id)
    .bind(source_item.is_sensitive.unwrap_or(0))
    .bind(source_item.is_pinned.unwrap_or(0))
    .bind(source_item.display_order.unwrap_or(0))
    .execute(db.inner())
    .await
    {
        Ok(result) => {
            let new_id = result.last_insert_rowid();
            log::info!(
                "[Command] clipboard_copy_to_tab success - item {} copied to tab {} as new item {}",
                item_id,
                target_tab_id,
                new_id
            );
            Ok(new_id)
        }
        Err(e) => {
            let error_msg = format!("Failed to copy item: {}", e);
            log::error!("[Command] clipboard_copy_to_tab failed: {}", error_msg);
            Err(error_msg)
        }
    }
}

/// Move multiple clipboard items to another tab (batch)
#[tauri::command]
pub async fn clipboard_move_to_tab_batch(
    db: tauri::State<'_, Db>,
    ids: Vec<i64>,
    target_tab_id: i64,
) -> Result<i64, String> {
    validate_ids(&ids)?;
    validate_positive_id("target_tab_id", target_tab_id)?;
    log::info!(
        "[Command] clipboard_move_to_tab_batch called - count: {}, target_tab_id: {}",
        ids.len(),
        target_tab_id
    );

    // Check if target tab exists
    match crate::db::TabRepository::get_by_id(&db, target_tab_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            let error_msg = format!("Target tab not found: {}", target_tab_id);
            log::error!(
                "[Command] clipboard_move_to_tab_batch failed: {}",
                error_msg
            );
            return Err(error_msg);
        }
        Err(e) => {
            let error_msg = format!("Failed to query target tab: {}", e);
            log::error!(
                "[Command] clipboard_move_to_tab_batch failed: {}",
                error_msg
            );
            return Err(error_msg);
        }
    }

    match ClipboardRepository::move_to_tab_batch(&db, &ids, target_tab_id).await {
        Ok(count) => {
            log::info!(
                "[Command] clipboard_move_to_tab_batch success - moved {} items to tab {}",
                count,
                target_tab_id
            );
            Ok(count)
        }
        Err(e) => {
            let error_msg = format!("Failed to move items: {}", e);
            log::error!(
                "[Command] clipboard_move_to_tab_batch failed: {}",
                error_msg
            );
            Err(error_msg)
        }
    }
}

/// Copy multiple clipboard items to another tab (batch)
#[tauri::command]
pub async fn clipboard_copy_to_tab_batch(
    db: tauri::State<'_, Db>,
    ids: Vec<i64>,
    target_tab_id: i64,
) -> Result<i64, String> {
    validate_ids(&ids)?;
    validate_positive_id("target_tab_id", target_tab_id)?;
    log::info!(
        "[Command] clipboard_copy_to_tab_batch called - count: {}, target_tab_id: {}",
        ids.len(),
        target_tab_id
    );

    // Check if target tab exists
    match crate::db::TabRepository::get_by_id(&db, target_tab_id).await {
        Ok(Some(_)) => {}
        Ok(None) => {
            let error_msg = format!("Target tab not found: {}", target_tab_id);
            log::error!(
                "[Command] clipboard_copy_to_tab_batch failed: {}",
                error_msg
            );
            return Err(error_msg);
        }
        Err(e) => {
            let error_msg = format!("Failed to query target tab: {}", e);
            log::error!(
                "[Command] clipboard_copy_to_tab_batch failed: {}",
                error_msg
            );
            return Err(error_msg);
        }
    }

    match ClipboardRepository::copy_to_tab_batch(&db, &ids, target_tab_id).await {
        Ok(count) => {
            log::info!(
                "[Command] clipboard_copy_to_tab_batch success - copied {} items to tab {}",
                count,
                target_tab_id
            );
            Ok(count)
        }
        Err(e) => {
            let error_msg = format!("Failed to copy items: {}", e);
            log::error!(
                "[Command] clipboard_copy_to_tab_batch failed: {}",
                error_msg
            );
            Err(error_msg)
        }
    }
}
