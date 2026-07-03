#[cfg(test)]
mod tests {
    use crate::db::tests::setup_test_db;
    use crate::db::{ClipboardItemInput, ClipboardRepository, TabRepository};

    #[tokio::test]
    async fn test_tab_repository_crud() {
        let pool = setup_test_db().await;

        // Test create tab
        let tab_id = TabRepository::create(&pool, "Test Tab").await.unwrap();
        assert!(tab_id > 0);

        // Test get all tabs
        let tabs = TabRepository::get_all(&pool).await.unwrap();
        assert_eq!(tabs.len(), 2); // Default + Test Tab
        assert_eq!(tabs[1].name, "Test Tab");

        // Test get by id
        let tab = TabRepository::get_by_id(&pool, tab_id).await.unwrap();
        assert!(tab.is_some());
        assert_eq!(tab.unwrap().name, "Test Tab");

        // Test get default tab
        let default_tab = TabRepository::get_default_tab(&pool).await.unwrap();
        assert_eq!(default_tab.name, "Clipboard");

        // Test delete tab
        TabRepository::delete(&pool, tab_id).await.unwrap();
        let tabs_after_delete = TabRepository::get_all(&pool).await.unwrap();
        assert_eq!(tabs_after_delete.len(), 1); // Only default tab remains
    }

    #[tokio::test]
    async fn test_tab_repository_delete_records_sync_deletes() -> Result<(), sqlx::Error> {
        let pool = setup_test_db().await;
        let tab_id = TabRepository::create(&pool, "Delete Me").await?;
        let item_id = ClipboardRepository::create(
            &pool,
            ClipboardItemInput {
                item_type: "image".to_string(),
                content: "data:image/png;base64,test".to_string(),
                content_hash: Some("image-hash".to_string()),
                metadata: None,
                tags: None,
                tab_id: Some(tab_id),
                is_sensitive: Some(0),
                is_pinned: Some(0),
            },
        )
        .await?;
        sqlx::query("INSERT INTO sync_item_map (local_id, item_key) VALUES (?, ?)")
            .bind(item_id)
            .bind("remote-image-key")
            .execute(&pool)
            .await?;

        TabRepository::delete(&pool, tab_id).await?;

        let item_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM clipboard_items WHERE id = ?")
                .bind(item_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(item_count.0, 0);

        let item_delete: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sync_changes WHERE entity_type = 'clipboard_item' AND operation = 'delete' AND item_key = 'remote-image-key'",
        )
        .fetch_one(&pool)
        .await?;
        assert_eq!(item_delete.0, 1);

        let tab_delete: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM sync_changes WHERE entity_type = 'tab' AND entity_id = ? AND operation = 'tab_delete'",
        )
        .bind(tab_id.to_string())
        .fetch_one(&pool)
        .await?;
        assert_eq!(tab_delete.0, 1);
        Ok(())
    }

    #[tokio::test]
    async fn test_clipboard_repository_create_and_retrieve() {
        let pool = setup_test_db().await;

        // Create test item
        let item_input = ClipboardItemInput {
            item_type: "text".to_string(),
            content: "Test content".to_string(),
            content_hash: None,
            metadata: Some(r#"{"source": "test"}"#.to_string()),
            tags: Some(r#"["tag1", "tag2"]"#.to_string()),
            tab_id: None,
            is_sensitive: Some(0),
            is_pinned: Some(0),
        };

        let item_id = ClipboardRepository::create(&pool, item_input)
            .await
            .unwrap();
        assert!(item_id > 0);

        // Get default tab id for retrieval
        let default_tab = TabRepository::get_default_tab(&pool).await.unwrap();
        let tab_id = default_tab.id.unwrap();

        // Test get by tab
        let items = ClipboardRepository::get_by_tab(&pool, tab_id, 10, 0)
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].content, "Test content");
        assert_eq!(items[0].item_type, "text");
    }

    #[tokio::test]
    async fn test_clipboard_repository_create_assigns_sparse_top_order() {
        let pool = setup_test_db().await;

        for content in ["First", "Second", "Third"] {
            let item_input = ClipboardItemInput {
                item_type: "text".to_string(),
                content: content.to_string(),
                content_hash: None,
                metadata: None,
                tags: None,
                tab_id: None,
                is_sensitive: Some(0),
                is_pinned: Some(0),
            };

            ClipboardRepository::create(&pool, item_input)
                .await
                .unwrap();
        }

        let default_tab = TabRepository::get_default_tab(&pool).await.unwrap();
        let tab_id = default_tab.id.unwrap();
        let items = ClipboardRepository::get_by_tab(&pool, tab_id, 10, 0)
            .await
            .unwrap();

        assert_eq!(
            items
                .iter()
                .map(|item| item.content.as_str())
                .collect::<Vec<_>>(),
            vec!["Third", "Second", "First"]
        );
        assert!(items[0].display_order.unwrap() < items[1].display_order.unwrap());
        assert!(items[1].display_order.unwrap() < items[2].display_order.unwrap());
    }

    #[tokio::test]
    async fn test_clipboard_repository_latest_ignores_pinned_list_order(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let pool = setup_test_db().await;
        let default_tab = TabRepository::get_default_tab(&pool).await?;
        let tab_id = default_tab.id.ok_or("default tab has no ID")?;

        let pinned_id = ClipboardRepository::create(
            &pool,
            ClipboardItemInput {
                item_type: "text".to_string(),
                content: "Pinned item".to_string(),
                content_hash: None,
                metadata: None,
                tags: None,
                tab_id: Some(tab_id),
                is_sensitive: Some(0),
                is_pinned: Some(1),
            },
        )
        .await?;
        let latest_id = ClipboardRepository::create(
            &pool,
            ClipboardItemInput {
                item_type: "text".to_string(),
                content: "Latest unpinned item".to_string(),
                content_hash: None,
                metadata: None,
                tags: None,
                tab_id: Some(tab_id),
                is_sensitive: Some(0),
                is_pinned: Some(0),
            },
        )
        .await?;

        let visible_items = ClipboardRepository::get_by_tab(&pool, tab_id, 10, 0).await?;
        assert_eq!(visible_items[0].id, Some(pinned_id));

        let latest = ClipboardRepository::get_latest_by_tab(&pool, tab_id)
            .await?
            .ok_or("latest item was not found")?;
        assert_eq!(latest.id, Some(latest_id));
        assert_eq!(latest.content, "Latest unpinned item");
        Ok(())
    }

    #[tokio::test]
    async fn test_clipboard_repository_move_to_tab_places_item_at_target_top(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let pool = setup_test_db().await;
        let target_tab_id = TabRepository::create(&pool, "Target").await?;

        for content in ["Target One", "Target Two"] {
            let item_input = ClipboardItemInput {
                item_type: "text".to_string(),
                content: content.to_string(),
                content_hash: None,
                metadata: None,
                tags: None,
                tab_id: Some(target_tab_id),
                is_sensitive: Some(0),
                is_pinned: Some(0),
            };
            ClipboardRepository::create(&pool, item_input).await?;
        }

        let source_item = ClipboardItemInput {
            item_type: "text".to_string(),
            content: "Moved Item".to_string(),
            content_hash: None,
            metadata: None,
            tags: None,
            tab_id: None,
            is_sensitive: Some(0),
            is_pinned: Some(0),
        };
        let moved_id = ClipboardRepository::create(&pool, source_item).await?;

        let moved = ClipboardRepository::move_to_tab(&pool, moved_id, target_tab_id).await?;
        assert!(moved);

        let target_items = ClipboardRepository::get_by_tab(&pool, target_tab_id, 10, 0).await?;
        assert_eq!(target_items[0].id, Some(moved_id));
        assert_eq!(target_items[0].content, "Moved Item");
        assert!(target_items[0].display_order < target_items[1].display_order);
        Ok(())
    }

    #[tokio::test]
    async fn test_clipboard_repository_move_to_tab_batch_places_items_at_target_top(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let pool = setup_test_db().await;
        let target_tab_id = TabRepository::create(&pool, "Target").await?;

        for content in ["Target One", "Target Two"] {
            let item_input = ClipboardItemInput {
                item_type: "text".to_string(),
                content: content.to_string(),
                content_hash: None,
                metadata: None,
                tags: None,
                tab_id: Some(target_tab_id),
                is_sensitive: Some(0),
                is_pinned: Some(0),
            };
            ClipboardRepository::create(&pool, item_input).await?;
        }

        let mut source_ids = Vec::new();
        for content in ["Source One", "Source Two", "Source Three"] {
            let item_input = ClipboardItemInput {
                item_type: "text".to_string(),
                content: content.to_string(),
                content_hash: None,
                metadata: None,
                tags: None,
                tab_id: None,
                is_sensitive: Some(0),
                is_pinned: Some(0),
            };
            source_ids.push(ClipboardRepository::create(&pool, item_input).await?);
        }

        let ids_in_visible_order = vec![source_ids[2], source_ids[1], source_ids[0]];
        let moved =
            ClipboardRepository::move_to_tab_batch(&pool, &ids_in_visible_order, target_tab_id)
                .await?;
        assert_eq!(moved, 3);

        let target_items = ClipboardRepository::get_by_tab(&pool, target_tab_id, 10, 0).await?;
        assert_eq!(
            target_items
                .iter()
                .take(5)
                .map(|item| item.content.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Source Three",
                "Source Two",
                "Source One",
                "Target Two",
                "Target One"
            ]
        );
        assert!(target_items
            .windows(2)
            .all(|pair| pair[0].display_order <= pair[1].display_order));
        Ok(())
    }

    #[tokio::test]
    async fn test_clipboard_repository_reorder_normalizes_legacy_orders_once() {
        let pool = setup_test_db().await;
        let default_tab = TabRepository::get_default_tab(&pool).await.unwrap();
        let tab_id = default_tab.id.unwrap();

        for (idx, content) in ["First", "Second", "Third", "Fourth"].iter().enumerate() {
            sqlx::query(
                r#"
                INSERT INTO clipboard_items
                (type, content, tab_id, is_sensitive, is_pinned, display_order, created_at, updated_at)
                VALUES ('text', ?, ?, 0, 0, 0, datetime('now'), datetime('now', '-' || ? || ' seconds'))
                "#,
            )
            .bind(content)
            .bind(tab_id)
            .bind(3 - idx as i32)
            .execute(&pool)
            .await
            .unwrap();
        }

        let before = ClipboardRepository::get_by_tab(&pool, tab_id, 10, 0)
            .await
            .unwrap();
        assert_eq!(
            before
                .iter()
                .map(|item| item.content.as_str())
                .collect::<Vec<_>>(),
            vec!["Fourth", "Third", "Second", "First"]
        );

        let first_id = before[3].id.unwrap();
        let moved = ClipboardRepository::move_item_to_position(&pool, tab_id, first_id, 3, 1)
            .await
            .unwrap();
        assert!(moved);

        let after = ClipboardRepository::get_by_tab(&pool, tab_id, 10, 0)
            .await
            .unwrap();
        assert_eq!(
            after
                .iter()
                .map(|item| item.content.as_str())
                .collect::<Vec<_>>(),
            vec!["Fourth", "First", "Third", "Second"]
        );
        assert!(after
            .windows(2)
            .all(|pair| pair[0].display_order.unwrap() < pair[1].display_order.unwrap()));
    }

    #[tokio::test]
    async fn test_clipboard_repository_deduplication() {
        let pool = setup_test_db().await;

        let short_text = "Short text".to_string();
        let item1 = ClipboardItemInput {
            item_type: "text".to_string(),
            content: short_text.clone(),
            content_hash: None,
            metadata: None,
            tags: None,
            tab_id: None,
            is_sensitive: Some(0),
            is_pinned: Some(0),
        };

        let item2 = ClipboardItemInput {
            item_type: "text".to_string(),
            content: short_text.clone(), // Same content
            content_hash: None,
            metadata: None,
            tags: None,
            tab_id: None,
            is_sensitive: Some(0),
            is_pinned: Some(0),
        };

        // Create first item
        let id1 = ClipboardRepository::create(&pool, item1).await.unwrap();
        let _ = id1;

        // Create second item with same content (should trigger dedup)
        let id2 = ClipboardRepository::create(&pool, item2).await.unwrap();

        // Get default tab id
        let default_tab = TabRepository::get_default_tab(&pool).await.unwrap();
        let tab_id = default_tab.id.unwrap();

        // Should only have one item due to deduplication
        let items = ClipboardRepository::get_by_tab(&pool, tab_id, 10, 0)
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, Some(id2)); // Latest item should remain
    }

    #[tokio::test]
    async fn test_clipboard_repository_pin_operations() {
        let pool = setup_test_db().await;

        // Create test item
        let item_input = ClipboardItemInput {
            item_type: "text".to_string(),
            content: "Test content".to_string(),
            content_hash: None,
            metadata: None,
            tags: None,
            tab_id: None,
            is_sensitive: Some(0),
            is_pinned: Some(0),
        };

        let item_id = ClipboardRepository::create(&pool, item_input)
            .await
            .unwrap();

        // Test pin item
        ClipboardRepository::toggle_pin(&pool, item_id, 1)
            .await
            .unwrap();

        // Get default tab id
        let default_tab = TabRepository::get_default_tab(&pool).await.unwrap();
        let tab_id = default_tab.id.unwrap();

        // Check that pinned item comes first
        let items = ClipboardRepository::get_by_tab(&pool, tab_id, 10, 0)
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].is_pinned, Some(1));

        // Test unpin item
        ClipboardRepository::toggle_pin(&pool, item_id, 0)
            .await
            .unwrap();
        let items = ClipboardRepository::get_by_tab(&pool, tab_id, 10, 0)
            .await
            .unwrap();
        assert_eq!(items[0].is_pinned, Some(0));
    }

    #[tokio::test]
    async fn test_clipboard_repository_search() {
        let pool = setup_test_db().await;

        // Create test items
        let item1 = ClipboardItemInput {
            item_type: "text".to_string(),
            content: "Hello world test".to_string(),
            content_hash: None,
            metadata: None,
            tags: None,
            tab_id: None,
            is_sensitive: Some(0),
            is_pinned: Some(0),
        };

        let item2 = ClipboardItemInput {
            item_type: "text".to_string(),
            content: "Another test content".to_string(),
            content_hash: None,
            metadata: None,
            tags: None,
            tab_id: None,
            is_sensitive: Some(0),
            is_pinned: Some(0),
        };

        ClipboardRepository::create(&pool, item1).await.unwrap();
        ClipboardRepository::create(&pool, item2).await.unwrap();

        // Test search
        let results = ClipboardRepository::search(&pool, "test", None)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);

        let results_partial = ClipboardRepository::search(&pool, "world", None)
            .await
            .unwrap();
        assert_eq!(results_partial.len(), 1);
        assert!(results_partial[0].content.contains("world"));
    }

    #[tokio::test]
    async fn test_clipboard_repository_delete() {
        let pool = setup_test_db().await;

        // Create test item
        let item_input = ClipboardItemInput {
            item_type: "text".to_string(),
            content: "Test content".to_string(),
            content_hash: None,
            metadata: None,
            tags: None,
            tab_id: None,
            is_sensitive: Some(0),
            is_pinned: Some(0),
        };

        let item_id = ClipboardRepository::create(&pool, item_input)
            .await
            .unwrap();

        // Verify item exists
        let default_tab = TabRepository::get_default_tab(&pool).await.unwrap();
        let tab_id = default_tab.id.unwrap();
        let items_before = ClipboardRepository::get_by_tab(&pool, tab_id, 10, 0)
            .await
            .unwrap();
        assert_eq!(items_before.len(), 1);

        // Delete item
        ClipboardRepository::delete(&pool, item_id).await.unwrap();

        // Verify item is deleted
        let items_after = ClipboardRepository::get_by_tab(&pool, tab_id, 10, 0)
            .await
            .unwrap();
        assert_eq!(items_after.len(), 0);
    }

    #[tokio::test]
    async fn test_clipboard_repository_update_tags() {
        let pool = setup_test_db().await;

        // Create test item
        let item_input = ClipboardItemInput {
            item_type: "text".to_string(),
            content: "Test content".to_string(),
            content_hash: None,
            metadata: None,
            tags: None,
            tab_id: None,
            is_sensitive: Some(0),
            is_pinned: Some(0),
        };

        let item_id = ClipboardRepository::create(&pool, item_input)
            .await
            .unwrap();

        // Update tags
        let new_tags = r#"["updated", "tags"]"#;
        ClipboardRepository::update_tags(&pool, item_id, new_tags)
            .await
            .unwrap();

        // Verify tags were updated
        let default_tab = TabRepository::get_default_tab(&pool).await.unwrap();
        let tab_id = default_tab.id.unwrap();
        let items = ClipboardRepository::get_by_tab(&pool, tab_id, 10, 0)
            .await
            .unwrap();
        assert_eq!(items[0].tags.as_deref().unwrap(), new_tags);
    }

    #[tokio::test]
    async fn test_clipboard_repository_clear_sensitive() {
        let pool = setup_test_db().await;

        // Create normal item
        let normal_item = ClipboardItemInput {
            item_type: "text".to_string(),
            content: "Normal content".to_string(),
            content_hash: None,
            metadata: None,
            tags: None,
            tab_id: None,
            is_sensitive: Some(0),
            is_pinned: Some(0),
        };

        // Create sensitive item
        let sensitive_item = ClipboardItemInput {
            item_type: "text".to_string(),
            content: "password123".to_string(),
            content_hash: None,
            metadata: None,
            tags: None,
            tab_id: None,
            is_sensitive: Some(1),
            is_pinned: Some(0),
        };

        ClipboardRepository::create(&pool, normal_item)
            .await
            .unwrap();
        ClipboardRepository::create(&pool, sensitive_item)
            .await
            .unwrap();

        // Verify both items exist
        let default_tab = TabRepository::get_default_tab(&pool).await.unwrap();
        let tab_id = default_tab.id.unwrap();
        let items_before = ClipboardRepository::get_by_tab(&pool, tab_id, 10, 0)
            .await
            .unwrap();
        assert_eq!(items_before.len(), 2);

        // Clear sensitive items
        ClipboardRepository::clear_sensitive(&pool).await.unwrap();

        // Verify only normal item remains
        let items_after = ClipboardRepository::get_by_tab(&pool, tab_id, 10, 0)
            .await
            .unwrap();
        assert_eq!(items_after.len(), 1);
        assert_eq!(items_after[0].content, "Normal content");
        assert_eq!(items_after[0].is_sensitive, Some(0));
    }
}
