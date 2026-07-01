use crate::db::database::Db;
use crate::db::models::{ClipboardItem, ClipboardItemInput, Tab};
use sqlx::{Error, Executor, SqliteConnection};

const SYNC_ENTITY_CLIPBOARD_ITEM: &str = "clipboard_item";
const SYNC_ENTITY_TAB: &str = "tab";
const SYNC_SOURCE_LOCAL: &str = "local";
const SYNC_OP_CREATE: &str = "create";
const SYNC_OP_DELETE: &str = "delete";
const SYNC_OP_UPDATE: &str = "update";
const SYNC_OP_TOGGLE_PIN: &str = "toggle_pin";
const SYNC_OP_METADATA_UPDATE: &str = "metadata_update";
const SYNC_OP_TAB_CHANGE: &str = "tab_change";
const SYNC_OP_REORDER: &str = "reorder";
const SYNC_OP_TAB_CREATE: &str = "tab_create";
const SYNC_OP_TAB_DELETE: &str = "tab_delete";
const SYNC_OP_TAB_RENAME: &str = "tab_rename";
const SEARCH_RESULT_LIMIT: i64 = 200;

async fn record_sync_change_tx(
    tx: &mut SqliteConnection,
    entity_type: &str,
    entity_id: &str,
    operation: &str,
    tab_id: Option<i64>,
    item_key: Option<&str>,
    source: &str,
) -> Result<(), Error> {
    sqlx::query(
        r#"
        INSERT INTO sync_changes (entity_type, entity_id, operation, tab_id, item_key, source, changed_at)
        VALUES (?, ?, ?, ?, ?, ?, datetime('now'))
        "#,
    )
    .bind(entity_type)
    .bind(entity_id)
    .bind(operation)
    .bind(tab_id)
    .bind(item_key)
    .bind(source)
    .execute(&mut *tx)
    .await?;

    Ok(())
}

async fn get_sync_item_key_tx(
    tx: &mut SqliteConnection,
    local_id: i64,
) -> Result<Option<String>, Error> {
    sqlx::query_scalar("SELECT item_key FROM sync_item_map WHERE local_id = ?")
        .bind(local_id)
        .fetch_optional(&mut *tx)
        .await
}

pub struct TabRepository;

impl TabRepository {
    pub async fn get_all(pool: &Db) -> Result<Vec<Tab>, Error> {
        log::debug!("[TabRepository] get_all called");
        let result = sqlx::query_as::<_, Tab>("SELECT * FROM tabs ORDER BY created_at ASC")
            .fetch_all(pool)
            .await;
        match &result {
            Ok(tabs) => log::debug!("[TabRepository] get_all returned {} tabs", tabs.len()),
            Err(e) => log::error!("[TabRepository] get_all failed: {}", e),
        }
        result
    }

    pub async fn get_by_id(pool: &Db, id: i64) -> Result<Option<Tab>, Error> {
        log::debug!("[TabRepository] get_by_id called with id: {}", id);
        let result = sqlx::query_as::<_, Tab>("SELECT * FROM tabs WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await;
        match &result {
            Ok(Some(tab)) => log::debug!("[TabRepository] get_by_id found tab: {:?}", tab.name),
            Ok(None) => log::debug!("[TabRepository] get_by_id no tab found"),
            Err(e) => log::error!("[TabRepository] get_by_id failed: {}", e),
        }
        result
    }

    pub async fn get_default_tab(pool: &Db) -> Result<Tab, Error> {
        log::debug!("[TabRepository] get_default_tab called");
        let result = sqlx::query_as::<_, Tab>("SELECT * FROM tabs WHERE is_default = 1 LIMIT 1")
            .fetch_one(pool)
            .await;
        match &result {
            Ok(tab) => log::info!(
                "[TabRepository] get_default_tab found: id={:?}, name={}",
                tab.id,
                tab.name
            ),
            Err(e) => log::error!("[TabRepository] get_default_tab failed: {}", e),
        }
        result
    }

    pub async fn create(pool: &Db, name: &str) -> Result<i64, Error> {
        log::info!("[TabRepository] create called with name: {}", name);
        let mut tx = pool.begin().await?;
        let result = sqlx::query("INSERT INTO tabs (name) VALUES (?)")
            .bind(name)
            .execute(&mut *tx)
            .await?;
        let id = result.last_insert_rowid();
        record_sync_change_tx(
            &mut tx,
            SYNC_ENTITY_TAB,
            &id.to_string(),
            SYNC_OP_TAB_CREATE,
            None,
            None,
            SYNC_SOURCE_LOCAL,
        )
        .await?;
        tx.commit().await?;
        log::info!("[TabRepository] create success, id: {}", id);
        Ok(id)
    }

    pub async fn delete(pool: &Db, id: i64) -> Result<(), Error> {
        log::info!("[TabRepository] delete called with id: {}", id);

        let mut tx = pool.begin().await?;
        let tab: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM tabs WHERE id = ? AND is_default = 0")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?;
        if tab.is_none() {
            tx.commit().await?;
            log::info!("[TabRepository] delete skipped default or missing tab");
            return Ok(());
        }

        let item_keys: Vec<(i64, Option<String>)> = sqlx::query_as(
            r#"
            SELECT ci.id, sim.item_key
            FROM clipboard_items ci
            LEFT JOIN sync_item_map sim ON sim.local_id = ci.id
            WHERE ci.tab_id = ?
            "#,
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;

        for (item_id, item_key) in &item_keys {
            record_sync_change_tx(
                &mut tx,
                SYNC_ENTITY_CLIPBOARD_ITEM,
                &item_id.to_string(),
                SYNC_OP_DELETE,
                Some(id),
                item_key.as_deref(),
                SYNC_SOURCE_LOCAL,
            )
            .await?;
        }

        for (item_id, _) in &item_keys {
            sqlx::query("DELETE FROM sync_item_map WHERE local_id = ?")
                .bind(item_id)
                .execute(&mut *tx)
                .await?;
        }

        // Delete all clipboard items in this tab first
        let items_result = sqlx::query("DELETE FROM clipboard_items WHERE tab_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await;
        match &items_result {
            Ok(result) => log::info!(
                "[TabRepository] deleted {} clipboard items for tab {}",
                result.rows_affected(),
                id
            ),
            Err(e) => log::error!(
                "[TabRepository] failed to delete clipboard items for tab {}: {}",
                id,
                e
            ),
        }
        items_result?;

        // Then delete the tab itself
        sqlx::query("DELETE FROM tabs WHERE id = ? AND is_default = 0")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        record_sync_change_tx(
            &mut tx,
            SYNC_ENTITY_TAB,
            &id.to_string(),
            SYNC_OP_TAB_DELETE,
            None,
            None,
            SYNC_SOURCE_LOCAL,
        )
        .await?;
        tx.commit().await?;
        log::info!("[TabRepository] delete success");
        Ok(())
    }

    pub async fn rename(pool: &Db, id: i64, new_name: &str) -> Result<(), Error> {
        log::info!(
            "[TabRepository] rename called - id: {}, new_name: {}",
            id,
            new_name
        );

        let mut tx = pool.begin().await?;
        sqlx::query("UPDATE tabs SET name = ? WHERE id = ? AND is_default = 0")
            .bind(new_name.trim())
            .bind(id)
            .execute(&mut *tx)
            .await?;
        record_sync_change_tx(
            &mut tx,
            SYNC_ENTITY_TAB,
            &id.to_string(),
            SYNC_OP_TAB_RENAME,
            None,
            None,
            SYNC_SOURCE_LOCAL,
        )
        .await?;
        tx.commit().await?;
        log::info!("[TabRepository] rename success");
        Ok(())
    }

    /// Get all tabs with auto_capture enabled (only default tab should have this)
    pub async fn get_auto_capture_tabs(pool: &Db) -> Result<Vec<Tab>, Error> {
        log::debug!("[TabRepository] get_auto_capture_tabs called");
        // Only return the default tab for auto-capture
        let result = sqlx::query_as::<_, Tab>(
            "SELECT * FROM tabs WHERE is_default = 1 AND auto_capture = 1 ORDER BY created_at ASC",
        )
        .fetch_all(pool)
        .await;
        match &result {
            Ok(tabs) => log::debug!(
                "[TabRepository] get_auto_capture_tabs returned {} tabs (default tab only)",
                tabs.len()
            ),
            Err(e) => log::error!("[TabRepository] get_auto_capture_tabs failed: {}", e),
        }
        result
    }
}

pub struct ClipboardRepository;

impl ClipboardRepository {
    const ORDER_STEP: i32 = 1000;
    const PINNED_ORDER_BASE: i32 = 0;
    const UNPINNED_ORDER_BASE: i32 = 1_000_000;

    async fn next_display_order_for_new_item<'e, E>(
        executor: E,
        tab_id: i64,
        is_pinned: i32,
    ) -> Result<i32, Error>
    where
        E: Executor<'e, Database = sqlx::Sqlite>,
    {
        let min_order = sqlx::query_scalar::<_, Option<i32>>(
            r#"
            SELECT MIN(COALESCE(display_order, 0))
            FROM clipboard_items
            WHERE tab_id = ? AND is_pinned = ?
            "#,
        )
        .bind(tab_id)
        .bind(is_pinned)
        .fetch_one(executor)
        .await?;

        Ok(min_order
            .map(|order| order.saturating_sub(Self::ORDER_STEP))
            .unwrap_or_else(|| {
                if is_pinned == 1 {
                    Self::PINNED_ORDER_BASE
                } else {
                    Self::UNPINNED_ORDER_BASE
                }
            }))
    }

    async fn renormalize_display_order(
        pool: &Db,
        ordered_ids: &[i64],
        source_is_pinned: bool,
    ) -> Result<(), Error> {
        let base_order = if source_is_pinned {
            Self::PINNED_ORDER_BASE
        } else {
            Self::UNPINNED_ORDER_BASE
        };

        let mut tx = pool.begin().await?;

        for (idx, &id) in ordered_ids.iter().enumerate() {
            let new_order = base_order.saturating_add((idx as i32) * Self::ORDER_STEP);
            sqlx::query("UPDATE clipboard_items SET display_order = ? WHERE id = ?")
                .bind(new_order)
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    fn order_between(
        prev_order: Option<i32>,
        next_order: Option<i32>,
        source_is_pinned: bool,
    ) -> Option<i32> {
        match (prev_order, next_order) {
            (Some(prev), Some(next)) => {
                if (next as i64) - (prev as i64) > 1 {
                    Some(prev + ((next - prev) / 2))
                } else {
                    None
                }
            }
            (None, Some(next)) => {
                let candidate = next.saturating_sub(Self::ORDER_STEP);
                (candidate < next).then_some(candidate)
            }
            (Some(prev), None) => {
                let candidate = prev.saturating_add(Self::ORDER_STEP);
                (candidate > prev).then_some(candidate)
            }
            (None, None) => Some(if source_is_pinned {
                Self::PINNED_ORDER_BASE
            } else {
                Self::UNPINNED_ORDER_BASE
            }),
        }
    }

    pub async fn create(pool: &Db, item: ClipboardItemInput) -> Result<i64, Error> {
        log::info!(
            "[ClipboardRepository] create called - type: {}, content_len: {}",
            item.item_type,
            item.content.len()
        );

        // Use a transaction to ensure both clipboard insert and sync outbox are atomic
        let mut tx = pool.begin().await?;

        // Deduplication logic for text items
        if item.item_type == "text" {
            let len = item.content.len();
            if len < 64 {
                // Full database deduplication for short strings
                log::debug!("[ClipboardRepository] Short text, doing full dedup");
                sqlx::query("DELETE FROM clipboard_items WHERE type = 'text' AND content = ?")
                    .bind(&item.content)
                    .execute(&mut *tx)
                    .await?;
            } else if (64..4096).contains(&len) {
                // Check against most recent 1024 items
                log::debug!(
                    "[ClipboardRepository] Medium text, checking recent 1024 items for dedup"
                );
                sqlx::query(
                    r#"
                    DELETE FROM clipboard_items 
                    WHERE id IN (
                        SELECT id FROM (
                            SELECT id FROM clipboard_items 
                            WHERE type = 'text'
                            ORDER BY updated_at DESC 
                            LIMIT 1024
                        ) WHERE content = ?
                    )
                    "#,
                )
                .bind(&item.content)
                .execute(&mut *tx)
                .await?;
            }
        }

        // Get default tab id by querying directly in the transaction
        let default_tab =
            sqlx::query_as::<_, Tab>("SELECT * FROM tabs WHERE is_default = 1 LIMIT 1")
                .fetch_optional(&mut *tx)
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
        let tab_id = item
            .tab_id
            .unwrap_or(default_tab.id.ok_or(sqlx::Error::RowNotFound)?);
        let is_pinned = item.is_pinned.unwrap_or(0);
        let display_order =
            Self::next_display_order_for_new_item(&mut *tx, tab_id, is_pinned).await?;
        log::debug!("[ClipboardRepository] Using tab_id: {}", tab_id);

        let result = sqlx::query(
            r#"
            INSERT INTO clipboard_items 
            (type, content, content_hash, metadata, tags, tab_id, is_sensitive, is_pinned, display_order, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(&item.item_type)
        .bind(&item.content)
        .bind(&item.content_hash)
        .bind(item.metadata.as_deref().unwrap_or("{}"))
        .bind(item.tags.as_deref().unwrap_or("[]"))
        .bind(tab_id)
        .bind(item.is_sensitive.unwrap_or(0))
        .bind(is_pinned)
        .bind(display_order)
        .execute(&mut *tx)
        .await?;

        let id = result.last_insert_rowid();

        // Record sync change in the same transaction - this ensures the outbox contract is never violated
        record_sync_change_tx(
            &mut tx,
            SYNC_ENTITY_CLIPBOARD_ITEM,
            &id.to_string(),
            SYNC_OP_CREATE,
            None,
            None,
            SYNC_SOURCE_LOCAL,
        )
        .await?;

        tx.commit().await?;

        log::info!("[ClipboardRepository] create success, id: {}", id);
        Ok(id)
    }

    pub async fn get_by_tab(
        pool: &Db,
        tab_id: i64,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<ClipboardItem>, Error> {
        let start_time = std::time::Instant::now();
        log::info!(
            "[ClipboardRepository] get_by_tab called - tab_id: {}, limit: {}, offset: {}",
            tab_id,
            limit,
            offset
        );

        let result = sqlx::query_as::<_, ClipboardItem>(
            r#"
            SELECT * FROM clipboard_items 
            WHERE tab_id = ? 
            ORDER BY is_pinned DESC, display_order ASC, updated_at DESC 
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(tab_id)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await;

        let elapsed = start_time.elapsed();
        match &result {
            Ok(items) => {
                log::info!(
                    "[ClipboardRepository] get_by_tab returned {} items in {}ms (offset={})",
                    items.len(),
                    elapsed.as_millis(),
                    offset
                );
                // Calculate the total size of returned data
                let total_size: usize = items.iter().map(|i| i.content.len()).sum();
                log::debug!(
                    "[ClipboardRepository] Total content size: {} bytes ({:.2} KB)",
                    total_size,
                    total_size as f64 / 1024.0
                );
            }
            Err(e) => log::error!("[ClipboardRepository] get_by_tab failed: {}", e),
        }
        result
    }

    pub async fn toggle_pin(pool: &Db, id: i64, is_pinned: i32) -> Result<(), Error> {
        log::info!(
            "[ClipboardRepository] toggle_pin called - id: {}, is_pinned: {}",
            id,
            is_pinned
        );

        // Use transaction to ensure pin update and sync outbox are atomic
        let mut tx = pool.begin().await?;

        sqlx::query(
            "UPDATE clipboard_items SET is_pinned = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(is_pinned)
        .bind(id)
        .execute(&mut *tx)
        .await?;

        // Record sync change in the same transaction
        record_sync_change_tx(
            &mut tx,
            SYNC_ENTITY_CLIPBOARD_ITEM,
            &id.to_string(),
            SYNC_OP_TOGGLE_PIN,
            None,
            None,
            SYNC_SOURCE_LOCAL,
        )
        .await?;

        tx.commit().await?;

        log::info!("[ClipboardRepository] toggle_pin success");
        Ok(())
    }

    /// Check if a text item with the same content hash exists
    /// Returns the ID of the existing item if found, None otherwise
    pub async fn check_duplicate_text(pool: &Db, content_hash: &str) -> Result<Option<i64>, Error> {
        log::debug!(
            "[ClipboardRepository] check_duplicate_text called - hash: {}",
            &content_hash[..16.min(content_hash.len())]
        );

        let result = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT id FROM clipboard_items 
            WHERE type = 'text' AND content_hash = ?
            ORDER BY updated_at DESC 
            LIMIT 1
            "#,
        )
        .bind(content_hash)
        .fetch_optional(pool)
        .await;

        match &result {
            Ok(Some(id)) => log::info!("[ClipboardRepository] Duplicate text found: {}", id),
            Ok(None) => log::debug!("[ClipboardRepository] No duplicate text found"),
            Err(e) => log::error!("[ClipboardRepository] check_duplicate_text failed: {}", e),
        }
        result
    }

    pub async fn move_to_top(pool: &Db, id: i64) -> Result<(), Error> {
        log::info!("[ClipboardRepository] move_to_top called - id: {}", id);

        let mut tx = pool.begin().await?;
        let item = sqlx::query_as::<_, (i64, i32)>(
            "SELECT tab_id, is_pinned FROM clipboard_items WHERE id = ?",
        )
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        let display_order = Self::next_display_order_for_new_item(&mut *tx, item.0, item.1).await?;

        sqlx::query(
            "UPDATE clipboard_items SET display_order = ?, updated_at = datetime('now') WHERE id = ?",
        )
            .bind(display_order)
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        log::info!("[ClipboardRepository] move_to_top success");
        Ok(())
    }

    /// Check if an image with the same content hash exists in recent N items
    /// Returns the ID of the existing item if found, None otherwise
    pub async fn check_duplicate_image(
        pool: &Db,
        content_hash: &str,
        recent_n: i64,
    ) -> Result<Option<i64>, Error> {
        log::debug!(
            "[ClipboardRepository] check_duplicate_image called - hash: {}, recent_n: {}",
            &content_hash[..16.min(content_hash.len())],
            recent_n
        );

        // Use content_hash for fast lookup instead of comparing full content
        let result = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT id FROM clipboard_items 
            WHERE type = 'image' AND content_hash = ?
            ORDER BY updated_at DESC 
            LIMIT 1
            "#,
        )
        .bind(content_hash)
        .fetch_optional(pool)
        .await;

        match &result {
            Ok(Some(id)) => log::info!("[ClipboardRepository] Duplicate image found: {}", id),
            Ok(None) => log::debug!("[ClipboardRepository] No duplicate image found"),
            Err(e) => log::error!("[ClipboardRepository] check_duplicate_image failed: {}", e),
        }
        result
    }

    pub async fn delete(pool: &Db, id: i64) -> Result<(), Error> {
        log::info!("[ClipboardRepository] delete called - id: {}", id);

        // Use transaction to ensure delete and sync outbox are atomic
        let mut tx = pool.begin().await?;

        // Record sync change before deleting (in same transaction)
        // This ensures the tombstone is created even if delete fails
        let item_key = get_sync_item_key_tx(&mut tx, id).await?;
        record_sync_change_tx(
            &mut tx,
            SYNC_ENTITY_CLIPBOARD_ITEM,
            &id.to_string(),
            SYNC_OP_DELETE,
            None,
            item_key.as_deref(),
            SYNC_SOURCE_LOCAL,
        )
        .await?;

        sqlx::query("DELETE FROM clipboard_items WHERE id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        sqlx::query("DELETE FROM sync_item_map WHERE local_id = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;

        log::info!("[ClipboardRepository] delete success");
        Ok(())
    }

    pub async fn update_tags(pool: &Db, id: i64, tags: &str) -> Result<(), Error> {
        log::debug!(
            "[ClipboardRepository] update_tags called - id: {}, tags: {}",
            id,
            tags
        );
        let mut tx = pool.begin().await?;
        sqlx::query("UPDATE clipboard_items SET tags = ? WHERE id = ?")
            .bind(tags)
            .bind(id)
            .execute(&mut *tx)
            .await?;
        record_sync_change_tx(
            &mut tx,
            SYNC_ENTITY_CLIPBOARD_ITEM,
            &id.to_string(),
            SYNC_OP_METADATA_UPDATE,
            None,
            None,
            SYNC_SOURCE_LOCAL,
        )
        .await?;
        tx.commit().await?;
        log::debug!("[ClipboardRepository] update_tags success");
        Ok(())
    }

    pub async fn update_timestamp(
        pool: &Db,
        id: i64,
        updated_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), Error> {
        log::debug!(
            "[ClipboardRepository] update_timestamp called - id: {}, updated_at: {}",
            id,
            updated_at
        );
        sqlx::query("UPDATE clipboard_items SET updated_at = ? WHERE id = ?")
            .bind(updated_at)
            .bind(id)
            .execute(pool)
            .await?;
        log::debug!("[ClipboardRepository] update_timestamp success");
        Ok(())
    }

    pub async fn search(
        pool: &Db,
        query: &str,
        tab_id: Option<i64>,
    ) -> Result<Vec<ClipboardItem>, Error> {
        log::debug!(
            "[ClipboardRepository] search called - query: {}, tab_id: {:?}",
            query,
            tab_id
        );
        // Search only text items and exclude base64 image content
        let sql = if tab_id.is_some() {
            "SELECT * FROM clipboard_items WHERE type = 'text' AND content LIKE ? AND tab_id = ? ORDER BY created_at DESC LIMIT ?"
        } else {
            "SELECT * FROM clipboard_items WHERE type = 'text' AND content LIKE ? ORDER BY created_at DESC LIMIT ?"
        };

        let mut q = sqlx::query_as::<_, ClipboardItem>(sql).bind(format!("%{}%", query));

        if let Some(tid) = tab_id {
            q = q.bind(tid);
        }
        q = q.bind(SEARCH_RESULT_LIMIT);

        let result = q.fetch_all(pool).await;
        match &result {
            Ok(items) => log::debug!(
                "[ClipboardRepository] search returned {} items",
                items.len()
            ),
            Err(e) => log::error!("[ClipboardRepository] search failed: {}", e),
        }
        result
    }

    pub async fn clear_sensitive(pool: &Db) -> Result<(), Error> {
        log::info!("[ClipboardRepository] clear_sensitive called");
        sqlx::query("DELETE FROM clipboard_items WHERE is_sensitive = 1")
            .execute(pool)
            .await?;
        log::info!("[ClipboardRepository] clear_sensitive success");
        Ok(())
    }

    pub async fn update_content(pool: &Db, id: i64, content: &str) -> Result<(), Error> {
        log::info!(
            "[ClipboardRepository] update_content called - id: {}, content_len: {}",
            id,
            content.len()
        );

        // Use transaction to ensure content update and sync outbox are atomic
        let mut tx = pool.begin().await?;

        sqlx::query(
            "UPDATE clipboard_items SET content = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(content)
        .bind(id)
        .execute(&mut *tx)
        .await?;

        // Record sync change in the same transaction with resolved item_key for idempotent retry
        record_sync_change_tx(
            &mut tx,
            SYNC_ENTITY_CLIPBOARD_ITEM,
            &id.to_string(),
            SYNC_OP_UPDATE,
            None,
            None,
            SYNC_SOURCE_LOCAL,
        )
        .await?;

        tx.commit().await?;

        log::info!("[ClipboardRepository] update_content success");
        Ok(())
    }

    /// Batch insert test data for performance testing
    /// This bypasses deduplication and directly inserts items
    pub async fn batch_insert_test_data(pool: &Db, count: i64, tab_id: i64) -> Result<i64, Error> {
        log::info!(
            "[ClipboardRepository] batch_insert_test_data called - count: {}, tab_id: {}",
            count,
            tab_id
        );

        let start = std::time::Instant::now();
        let mut inserted = 0i64;

        // Use transaction for batch insert
        let mut tx = pool.begin().await?;

        for i in 0..count {
            // Create varied content types
            let (content, item_type) = if i % 10 == 0 {
                // Every 10th item is a longer text
                (format!("Test data item #{} - This is a longer text content for performance testing. It contains multiple sentences and varies in length to simulate real clipboard content. Item index: {}.", i, i), "text")
            } else if i % 100 == 50 {
                // Some items with sensitive keywords
                (format!("Password for service #{}: test123", i), "text")
            } else {
                // Regular short text
                (
                    format!("Test clipboard item #{} - Sample text content", i),
                    "text",
                )
            };

            let is_sensitive = if content.to_lowercase().contains("password")
                || content.to_lowercase().contains("secret")
                || content.to_lowercase().contains("key")
            {
                1
            } else {
                0
            };

            let is_pinned = if i % 50 == 0 { 1 } else { 0 };

            let metadata = format!(
                r#"{{"source":"test","source_app":"test_generator","window_title":"Test Window {}","timestamp":"{}"}}"#,
                i,
                chrono::Utc::now().to_rfc3339()
            );

            let result = sqlx::query(
                r#"
                INSERT INTO clipboard_items 
                (type, content, content_hash, metadata, tags, tab_id, is_sensitive, is_pinned, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now', '-' || ? || ' seconds'), datetime('now'))
                "#,
            )
            .bind(item_type)
            .bind(&content)
            .bind(None::<String>) // content_hash
            .bind(&metadata)
            .bind("[]") // tags
            .bind(tab_id)
            .bind(is_sensitive)
            .bind(is_pinned)
            .bind(i) // Stagger created_at times
            .execute(&mut *tx)
            .await;

            match result {
                Ok(_) => inserted += 1,
                Err(e) => {
                    log::warn!("[ClipboardRepository] Failed to insert item {}: {}", i, e);
                }
            }
        }

        tx.commit().await?;

        let elapsed = start.elapsed();
        log::info!(
            "[ClipboardRepository] batch_insert_test_data completed - inserted: {} items in {:?}",
            inserted,
            elapsed
        );

        Ok(inserted)
    }

    /// Clear all clipboard items (for testing)
    pub async fn clear_all(pool: &Db) -> Result<(), Error> {
        log::info!("[ClipboardRepository] clear_all called");
        sqlx::query("DELETE FROM clipboard_items")
            .execute(pool)
            .await?;
        log::info!("[ClipboardRepository] clear_all success");
        Ok(())
    }

    /// Get total count of clipboard items for a tab
    pub async fn get_total_count(pool: &Db, tab_id: i64) -> Result<i64, Error> {
        log::debug!(
            "[ClipboardRepository] get_total_count called - tab_id: {}",
            tab_id
        );
        let result =
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM clipboard_items WHERE tab_id = ?")
                .bind(tab_id)
                .fetch_one(pool)
                .await;
        match &result {
            Ok(count) => log::debug!("[ClipboardRepository] get_total_count returned {}", count),
            Err(e) => log::error!("[ClipboardRepository] get_total_count failed: {}", e),
        }
        result
    }

    /// Get a single clipboard item at a specific index (ordered by is_pinned DESC, display_order ASC, updated_at DESC)
    /// This is used for scrollbar tooltip to show item info at a given position
    pub async fn get_item_at_index(
        pool: &Db,
        tab_id: i64,
        index: i64,
    ) -> Result<Option<ClipboardItem>, Error> {
        log::debug!(
            "[ClipboardRepository] get_item_at_index called - tab_id: {}, index: {}",
            tab_id,
            index
        );
        let result = sqlx::query_as::<_, ClipboardItem>(
            r#"
            SELECT * FROM clipboard_items 
            WHERE tab_id = ? 
            ORDER BY is_pinned DESC, display_order ASC, updated_at DESC 
            LIMIT 1 OFFSET ?
            "#,
        )
        .bind(tab_id)
        .bind(index)
        .fetch_optional(pool)
        .await;
        match &result {
            Ok(Some(item)) => log::debug!(
                "[ClipboardRepository] get_item_at_index found item id: {:?}",
                item.id
            ),
            Ok(None) => log::debug!("[ClipboardRepository] get_item_at_index no item found"),
            Err(e) => log::error!("[ClipboardRepository] get_item_at_index failed: {}", e),
        }
        result
    }

    /// Delete items by index range (ordered by is_pinned DESC, display_order ASC, updated_at DESC)
    /// This is used for batch deletion in range selection
    pub async fn delete_by_index_range(
        pool: &Db,
        tab_id: i64,
        start_index: i64,
        end_index: i64,
    ) -> Result<i64, Error> {
        log::info!(
            "[ClipboardRepository] delete_by_index_range called - tab_id: {}, start: {}, end: {}",
            tab_id,
            start_index,
            end_index
        );

        let mut tx = pool.begin().await?;
        let ids: Vec<(i64,)> = sqlx::query_as(
            r#"
            SELECT id FROM (
                SELECT id FROM (
                    SELECT id FROM clipboard_items 
                    WHERE tab_id = ? 
                    ORDER BY is_pinned DESC, display_order ASC, updated_at DESC 
                    LIMIT ? OFFSET ?
                )
            )
            "#,
        )
        .bind(tab_id)
        .bind(end_index - start_index + 1) // count
        .bind(start_index) // offset
        .fetch_all(&mut *tx)
        .await?;

        for (id,) in &ids {
            let item_key = get_sync_item_key_tx(&mut tx, *id).await?;
            record_sync_change_tx(
                &mut tx,
                SYNC_ENTITY_CLIPBOARD_ITEM,
                &id.to_string(),
                SYNC_OP_DELETE,
                Some(tab_id),
                item_key.as_deref(),
                SYNC_SOURCE_LOCAL,
            )
            .await?;
        }

        for (id,) in &ids {
            sqlx::query("DELETE FROM clipboard_items WHERE id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("DELETE FROM sync_item_map WHERE local_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }

        tx.commit().await?;

        Ok(ids.len() as i64)
    }

    /// Move multiple items to another tab (batch update tab_id)
    /// Returns the number of items moved
    pub async fn move_to_tab(pool: &Db, id: i64, target_tab_id: i64) -> Result<bool, Error> {
        log::info!(
            "[ClipboardRepository] move_to_tab called - id: {}, target_tab_id: {}",
            id,
            target_tab_id
        );

        let mut tx = pool.begin().await?;
        let Some(is_pinned) =
            sqlx::query_scalar::<_, i32>("SELECT is_pinned FROM clipboard_items WHERE id = ?")
                .bind(id)
                .fetch_optional(&mut *tx)
                .await?
        else {
            tx.commit().await?;
            return Ok(false);
        };
        let display_order =
            Self::next_display_order_for_new_item(&mut *tx, target_tab_id, is_pinned).await?;

        let result = sqlx::query(
            "UPDATE clipboard_items SET tab_id = ?, display_order = ?, updated_at = datetime('now') WHERE id = ?",
        )
        .bind(target_tab_id)
        .bind(display_order)
        .bind(id)
        .execute(&mut *tx)
        .await?;

        record_sync_change_tx(
            &mut tx,
            SYNC_ENTITY_CLIPBOARD_ITEM,
            &id.to_string(),
            SYNC_OP_TAB_CHANGE,
            Some(target_tab_id),
            None,
            SYNC_SOURCE_LOCAL,
        )
        .await?;

        tx.commit().await?;
        Ok(result.rows_affected() > 0)
    }

    /// Move multiple items to another tab. Moved items are placed at the top of
    /// the target tab instead of preserving their old manual order.
    pub async fn move_to_tab_batch(
        pool: &Db,
        ids: &[i64],
        target_tab_id: i64,
    ) -> Result<i64, Error> {
        log::info!(
            "[ClipboardRepository] move_to_tab_batch called - count: {}, target_tab_id: {}",
            ids.len(),
            target_tab_id
        );

        if ids.is_empty() {
            return Ok(0);
        }

        let mut tx = pool.begin().await?;
        let mut movable_items = Vec::new();

        for &id in ids {
            if let Some(is_pinned) =
                sqlx::query_scalar::<_, i32>("SELECT is_pinned FROM clipboard_items WHERE id = ?")
                    .bind(id)
                    .fetch_optional(&mut *tx)
                    .await?
            {
                movable_items.push((id, is_pinned));
            }
        }

        let pinned_count = movable_items
            .iter()
            .filter(|(_, is_pinned)| *is_pinned == 1)
            .count();
        let unpinned_count = movable_items.len().saturating_sub(pinned_count);

        let first_pinned_order = if pinned_count > 0 {
            Some(
                Self::next_display_order_for_new_item(&mut *tx, target_tab_id, 1)
                    .await?
                    .saturating_sub((pinned_count.saturating_sub(1) as i32) * Self::ORDER_STEP),
            )
        } else {
            None
        };
        let first_unpinned_order = if unpinned_count > 0 {
            Some(
                Self::next_display_order_for_new_item(&mut *tx, target_tab_id, 0)
                    .await?
                    .saturating_sub((unpinned_count.saturating_sub(1) as i32) * Self::ORDER_STEP),
            )
        } else {
            None
        };

        let mut moved = 0i64;
        let mut next_pinned_order = first_pinned_order;
        let mut next_unpinned_order = first_unpinned_order;

        for (id, is_pinned) in movable_items {
            let next_order = if is_pinned == 1 {
                &mut next_pinned_order
            } else {
                &mut next_unpinned_order
            };
            let display_order = match next_order {
                Some(order) => {
                    let current = *order;
                    *order = order.saturating_add(Self::ORDER_STEP);
                    current
                }
                None => {
                    Self::next_display_order_for_new_item(&mut *tx, target_tab_id, is_pinned)
                        .await?
                }
            };

            let result = sqlx::query(
                "UPDATE clipboard_items SET tab_id = ?, display_order = ?, updated_at = datetime('now') WHERE id = ?",
            )
            .bind(target_tab_id)
            .bind(display_order)
            .bind(id)
            .execute(&mut *tx)
            .await?;

            moved += result.rows_affected() as i64;
            record_sync_change_tx(
                &mut tx,
                SYNC_ENTITY_CLIPBOARD_ITEM,
                &id.to_string(),
                SYNC_OP_TAB_CHANGE,
                Some(target_tab_id),
                None,
                SYNC_SOURCE_LOCAL,
            )
            .await?;
        }
        tx.commit().await?;

        log::info!(
            "[ClipboardRepository] move_to_tab_batch moved {} rows",
            moved
        );
        Ok(moved)
    }

    /// Copy multiple items to another tab (batch insert with new tab_id)
    /// Returns the number of items copied
    pub async fn copy_to_tab_batch(
        pool: &Db,
        ids: &[i64],
        target_tab_id: i64,
    ) -> Result<i64, Error> {
        log::info!(
            "[ClipboardRepository] copy_to_tab_batch called - count: {}, target_tab_id: {}",
            ids.len(),
            target_tab_id
        );

        if ids.is_empty() {
            return Ok(0);
        }

        let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
        let select_sql = format!(
            "SELECT type, content, content_hash, metadata, tags, is_sensitive, is_pinned, display_order FROM clipboard_items WHERE id IN ({})",
            placeholders.join(", ")
        );

        let mut select_query = sqlx::query_as::<
            _,
            (
                String,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
                Option<i32>,
                Option<i32>,
                Option<i32>,
            ),
        >(&select_sql);
        for &id in ids {
            select_query = select_query.bind(id);
        }

        let source_items = select_query.fetch_all(pool).await?;

        let mut copied = 0i64;
        for (
            item_type,
            content,
            content_hash,
            metadata,
            tags,
            is_sensitive,
            is_pinned,
            display_order,
        ) in &source_items
        {
            let result = sqlx::query(
                r#"
                INSERT INTO clipboard_items 
                (type, content, content_hash, metadata, tags, tab_id, is_sensitive, is_pinned, display_order, created_at, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'), datetime('now'))
                "#
            )
            .bind(item_type)
            .bind(content)
            .bind(content_hash)
            .bind(metadata.as_deref().unwrap_or("{}"))
            .bind(tags.as_deref().unwrap_or("[]"))
            .bind(target_tab_id)
            .bind(is_sensitive.unwrap_or(0))
            .bind(is_pinned.unwrap_or(0))
            .bind(display_order.unwrap_or(0))
            .execute(pool)
            .await?;
            copied += result.rows_affected() as i64;
        }

        log::info!(
            "[ClipboardRepository] copy_to_tab_batch copied {} rows",
            copied
        );
        Ok(copied)
    }

    /// Delete multiple items by their IDs in a single transaction
    /// Returns the number of items deleted
    pub async fn delete_by_ids(pool: &Db, ids: &[i64]) -> Result<i64, Error> {
        log::info!(
            "[ClipboardRepository] delete_by_ids called - count: {}",
            ids.len()
        );

        if ids.is_empty() {
            return Ok(0);
        }

        let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "DELETE FROM clipboard_items WHERE id IN ({})",
            placeholders.join(", ")
        );

        let mut tx = pool.begin().await?;
        for &id in ids {
            let item_key = get_sync_item_key_tx(&mut tx, id).await?;
            record_sync_change_tx(
                &mut tx,
                SYNC_ENTITY_CLIPBOARD_ITEM,
                &id.to_string(),
                SYNC_OP_DELETE,
                None,
                item_key.as_deref(),
                SYNC_SOURCE_LOCAL,
            )
            .await?;
        }

        let mut query = sqlx::query(&sql);
        for &id in ids {
            query = query.bind(id);
        }

        let result = query.execute(&mut *tx).await?;
        for &id in ids {
            sqlx::query("DELETE FROM sync_item_map WHERE local_id = ?")
                .bind(id)
                .execute(&mut *tx)
                .await?;
        }
        tx.commit().await?;

        log::info!(
            "[ClipboardRepository] delete_by_ids deleted {} rows",
            result.rows_affected()
        );
        Ok(result.rows_affected() as i64)
    }

    /// Get all item types for a tab (for virtual scrolling height calculation)
    /// Returns a vector of (id, type) tuples ordered by is_pinned DESC, display_order ASC, updated_at DESC
    pub async fn get_all_types(pool: &Db, tab_id: i64) -> Result<Vec<(i64, String)>, Error> {
        log::debug!(
            "[ClipboardRepository] get_all_types called - tab_id: {}",
            tab_id
        );
        let result = sqlx::query_as::<_, (i64, String)>(
            r#"
            SELECT id, type FROM clipboard_items 
            WHERE tab_id = ? 
            ORDER BY is_pinned DESC, display_order ASC, updated_at DESC
            "#,
        )
        .bind(tab_id)
        .fetch_all(pool)
        .await;
        match &result {
            Ok(items) => log::debug!(
                "[ClipboardRepository] get_all_types returned {} items",
                items.len()
            ),
            Err(e) => log::error!("[ClipboardRepository] get_all_types failed: {}", e),
        }
        result
    }

    /// Move an item to a new position within the same pin group
    /// This updates display_order for all affected items
    ///
    /// # Arguments
    /// * `tab_id` - The tab ID
    /// * `item_id` - The ID of the item to move
    /// * `from_index` - Current index of the item (for validation)
    /// * `to_index` - Target index for the item
    ///
    /// # Returns
    /// true if successful, false if the move would cross pin boundaries
    pub async fn move_item_to_position(
        pool: &Db,
        tab_id: i64,
        item_id: i64,
        from_index: i64,
        to_index: i64,
    ) -> Result<bool, Error> {
        log::info!(
            "[ClipboardRepository] move_item_to_position called - tab_id: {}, item_id: {}, from: {}, to: {}",
            tab_id,
            item_id,
            from_index,
            to_index
        );

        // If same position, nothing to do
        if from_index == to_index {
            log::debug!("[ClipboardRepository] Same position, skipping");
            return Ok(true);
        }

        // Get the item being moved to check pin status
        let source_item =
            sqlx::query_as::<_, ClipboardItem>("SELECT * FROM clipboard_items WHERE id = ?")
                .bind(item_id)
                .fetch_optional(pool)
                .await?;

        let source_item = match source_item {
            Some(item) => item,
            None => {
                log::error!("[ClipboardRepository] Source item not found: {}", item_id);
                return Ok(false);
            }
        };

        // Get the item at the target index to check if we can move there
        let target_item = sqlx::query_as::<_, ClipboardItem>(
            r#"
            SELECT * FROM clipboard_items 
            WHERE tab_id = ? 
            ORDER BY is_pinned DESC, display_order ASC, updated_at DESC 
            LIMIT 1 OFFSET ?
            "#,
        )
        .bind(tab_id)
        .bind(to_index)
        .fetch_optional(pool)
        .await?;

        // Check if moving would cross pin boundaries
        let source_is_pinned = source_item.is_pinned.unwrap_or(0) == 1;

        if let Some(ref target) = target_item {
            let target_is_pinned = target.is_pinned.unwrap_or(0) == 1;

            // Only allow reordering within the same pin group
            if source_is_pinned != target_is_pinned {
                log::warn!(
                    "[ClipboardRepository] Cannot move across pin boundaries: source={}, target={}",
                    source_is_pinned,
                    target_is_pinned
                );
                return Ok(false);
            }
        }

        // Get items in the same pin group. We only update the moved row when
        // there is enough numeric space between its new neighbors.
        let pin_filter = if source_is_pinned {
            "is_pinned = 1"
        } else {
            "is_pinned = 0"
        };

        // Get all items in this pin group with their current positions
        let items_in_group: Vec<(i64, i32)> = sqlx::query_as::<_, (i64, i32)>(&format!(
            r#"
                SELECT id, COALESCE(display_order, 0) as display_order
                FROM clipboard_items 
                WHERE tab_id = ? AND {}
                ORDER BY display_order ASC, updated_at DESC
                "#,
            pin_filter
        ))
        .bind(tab_id)
        .fetch_all(pool)
        .await?;

        // Build a list of (item ID, order) in the new order.
        let mut ordered_items: Vec<(i64, i32)> = items_in_group;

        // Find the position of our item in this list
        let current_pos = ordered_items.iter().position(|(id, _)| *id == item_id);

        // Convert global to_index to pin-group-local index
        // to_index is the global position, but we need the position within the pin group
        let local_target_index = if let Some(ref target) = target_item {
            if let Some(target_id) = target.id {
                // Find the target item's position within the pin group
                ordered_items
                    .iter()
                    .position(|(id, _)| *id == target_id)
                    .unwrap_or(to_index as usize)
            } else {
                to_index as usize
            }
        } else {
            // If no target item (moving to end), use the end of the group
            ordered_items.len().saturating_sub(1)
        };

        if let Some(pos) = current_pos {
            // Remove from current position
            let moved_item = ordered_items.remove(pos);

            // Insert at new position (use local_target_index instead of global to_index)
            let new_pos = local_target_index.min(ordered_items.len());
            ordered_items.insert(new_pos, moved_item);

            let prev_order = new_pos
                .checked_sub(1)
                .and_then(|idx| ordered_items.get(idx).map(|(_, order)| *order));
            let next_order = ordered_items.get(new_pos + 1).map(|(_, order)| *order);

            if let Some(new_order) = Self::order_between(prev_order, next_order, source_is_pinned) {
                let mut tx = pool.begin().await?;
                sqlx::query("UPDATE clipboard_items SET display_order = ? WHERE id = ?")
                    .bind(new_order)
                    .bind(item_id)
                    .execute(&mut *tx)
                    .await?;
                record_sync_change_tx(
                    &mut tx,
                    SYNC_ENTITY_CLIPBOARD_ITEM,
                    &item_id.to_string(),
                    SYNC_OP_REORDER,
                    Some(tab_id),
                    None,
                    SYNC_SOURCE_LOCAL,
                )
                .await?;
                tx.commit().await?;

                log::info!(
                    "[ClipboardRepository] move_item_to_position success - updated one row, item {} from {} to {}",
                    item_id,
                    from_index,
                    to_index
                );
                return Ok(true);
            }

            let ordered_ids: Vec<i64> = ordered_items.iter().map(|(id, _)| *id).collect();
            Self::renormalize_display_order(pool, &ordered_ids, source_is_pinned).await?;
            let mut tx = pool.begin().await?;
            record_sync_change_tx(
                &mut tx,
                SYNC_ENTITY_CLIPBOARD_ITEM,
                &item_id.to_string(),
                SYNC_OP_REORDER,
                Some(tab_id),
                None,
                SYNC_SOURCE_LOCAL,
            )
            .await?;
            tx.commit().await?;

            log::info!(
                "[ClipboardRepository] move_item_to_position success - renormalized {} rows, moved item {} from {} to {}",
                ordered_ids.len(),
                item_id,
                from_index,
                to_index
            );
            Ok(true)
        } else {
            log::warn!("[ClipboardRepository] Item not found in group during reorder");
            Ok(false)
        }
    }

    /// Create clipboard item for all auto_capture tabs (used by clipboard monitoring)
    pub async fn create_for_auto_capture_tabs(
        pool: &Db,
        item: ClipboardItemInput,
    ) -> Result<Vec<i64>, Error> {
        log::info!(
            "[ClipboardRepository] create_for_auto_capture_tabs called - type: {}, content_len: {}",
            item.item_type,
            item.content.len()
        );

        // Get all auto_capture tabs
        let auto_capture_tabs = TabRepository::get_auto_capture_tabs(pool).await?;

        if auto_capture_tabs.is_empty() {
            log::warn!("[ClipboardRepository] No auto_capture tabs found, skipping item creation");
            return Ok(vec![]);
        }

        let mut created_ids = Vec::new();

        // Create item for each auto_capture tab
        for tab in &auto_capture_tabs {
            let tab_id = tab.id.ok_or_else(|| sqlx::Error::RowNotFound)?;

            // Create a copy of item input with explicit tab_id
            let item_for_tab = ClipboardItemInput {
                item_type: item.item_type.clone(),
                content: item.content.clone(),
                content_hash: item.content_hash.clone(),
                metadata: item.metadata.clone(),
                tags: item.tags.clone(),
                tab_id: Some(tab_id),
                is_sensitive: item.is_sensitive,
                is_pinned: item.is_pinned,
            };

            // Use the existing create logic but skip dedup for non-first tabs
            if created_ids.is_empty() {
                // First tab: use full create logic with dedup
                match Self::create(pool, item_for_tab).await {
                    Ok(id) => {
                        log::debug!(
                            "[ClipboardRepository] Created item in tab {}: {}",
                            tab.name,
                            id
                        );
                        created_ids.push(id);
                    }
                    Err(e) => {
                        log::error!(
                            "[ClipboardRepository] Failed to create item in tab {}: {}",
                            tab.name,
                            e
                        );
                    }
                }
            } else {
                // Subsequent tabs: skip dedup, just insert
                let is_pinned = item_for_tab.is_pinned.unwrap_or(0);
                let display_order =
                    Self::next_display_order_for_new_item(pool, tab_id, is_pinned).await?;
                let mut tx = pool.begin().await?;
                let result = sqlx::query(
                    r#"
                    INSERT INTO clipboard_items 
                    (type, content, content_hash, metadata, tags, tab_id, is_sensitive, is_pinned, display_order, updated_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
                    "#,
                )
                .bind(&item_for_tab.item_type)
                .bind(&item_for_tab.content)
                .bind(&item_for_tab.content_hash)
                .bind(item_for_tab.metadata.as_deref().unwrap_or("{}"))
                .bind(item_for_tab.tags.as_deref().unwrap_or("[]"))
                .bind(tab_id)
                .bind(item_for_tab.is_sensitive.unwrap_or(0))
                .bind(is_pinned)
                .bind(display_order)
                .execute(&mut *tx)
                .await;

                match result {
                    Ok(res) => {
                        let id = res.last_insert_rowid();
                        record_sync_change_tx(
                            &mut tx,
                            SYNC_ENTITY_CLIPBOARD_ITEM,
                            &id.to_string(),
                            SYNC_OP_CREATE,
                            Some(tab_id),
                            None,
                            SYNC_SOURCE_LOCAL,
                        )
                        .await?;
                        tx.commit().await?;
                        log::debug!(
                            "[ClipboardRepository] Created item in tab {}: {}",
                            tab.name,
                            id
                        );
                        created_ids.push(id);
                    }
                    Err(e) => {
                        log::error!(
                            "[ClipboardRepository] Failed to create item in tab {}: {}",
                            tab.name,
                            e
                        );
                    }
                }
            }
        }

        log::info!(
            "[ClipboardRepository] create_for_auto_capture_tabs success, created {} items",
            created_ids.len()
        );
        Ok(created_ids)
    }
}
