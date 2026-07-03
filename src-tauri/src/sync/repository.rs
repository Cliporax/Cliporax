/// Sync Repository - manages local sync state in SQLite
use crate::db::database::Db;
use crate::db::models::ClipboardItem;
use crate::sync::error::SyncError;
use crate::sync::models::*;
use base64::Engine;

pub struct SyncRepository {
    pool: Db,
}

fn provider_kind_from_db(provider: &str) -> SyncProviderKind {
    match provider {
        "webdav" => SyncProviderKind::WebDav,
        "sftp" => SyncProviderKind::Sftp,
        "google_drive" => SyncProviderKind::GoogleDrive,
        "one_drive" => SyncProviderKind::OneDrive,
        _ => SyncProviderKind::Sftp,
    }
}

fn provider_kind_to_db(provider: &SyncProviderKind) -> &'static str {
    match provider {
        SyncProviderKind::WebDav => "webdav",
        SyncProviderKind::Sftp => "sftp",
        SyncProviderKind::GoogleDrive => "google_drive",
        SyncProviderKind::OneDrive => "one_drive",
    }
}

impl SyncRepository {
    pub fn new(pool: Db) -> Self {
        Self { pool }
    }

    /// Get or create device ID for this installation
    pub async fn get_or_create_device_id(&self) -> Result<String, SyncError> {
        // Check if device exists
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT device_id FROM sync_device WHERE id = 1")
                .fetch_optional(&self.pool)
                .await?;

        if let Some((device_id,)) = existing {
            return Ok(device_id);
        }

        // Create new device ID
        let device_id = format!("device_{}", uuid::Uuid::new_v4().simple());
        sqlx::query("INSERT INTO sync_device (id, device_id) VALUES (1, ?)")
            .bind(&device_id)
            .execute(&self.pool)
            .await?;

        log::info!("[Sync::Repository] Created new device ID: {}", device_id);
        Ok(device_id)
    }

    /// List all sync profiles
    pub async fn list_profiles(&self) -> Result<Vec<SyncProfileSummary>, SyncError> {
        let profiles: Vec<(String, String, String, String, bool, Option<String>)> = sqlx::query_as(
            r#"
            SELECT sp.id, sp.name, sp.provider, sp.remote_root,
                   CASE WHEN sp.config_json LIKE '%"enabled":true%' THEN 1 ELSE 0 END,
                   ss.last_sync_at
            FROM sync_profiles sp
            LEFT JOIN sync_state ss ON sp.id = ss.scope_id AND ss.scope = 'profile'
            ORDER BY sp.updated_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(profiles
            .into_iter()
            .map(
                |(id, name, provider, remote_root, encryption_enabled, last_sync_at)| {
                    SyncProfileSummary {
                        id,
                        name,
                        provider: provider_kind_from_db(&provider),
                        remote_root,
                        encryption_enabled,
                        last_sync_at,
                        status: "idle".to_string(),
                    }
                },
            )
            .collect())
    }

    /// Get a sync profile by ID
    pub async fn get_profile(&self, profile_id: &str) -> Result<SyncProfile, SyncError> {
        let row: Option<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
            r#"
            SELECT id, name, provider, remote_root, config_json, credential_refs_json
            FROM sync_profiles WHERE id = ?
            "#,
        )
        .bind(profile_id)
        .fetch_optional(&self.pool)
        .await?;

        let (id, name, provider, remote_root, config_json, credential_refs_json) =
            row.ok_or_else(|| SyncError::ProfileNotFound(profile_id.to_string()))?;

        let profile: SyncProfile = serde_json::from_str(&config_json).map_err(|e| {
            SyncError::InvalidProfile(format!("Failed to parse profile config: {}", e))
        })?;

        // Merge with basic fields
        Ok(SyncProfile {
            id,
            name,
            provider: provider_kind_from_db(&provider),
            remote_root,
            credential_refs: credential_refs_json
                .and_then(|json| serde_json::from_str(&json).ok())
                .unwrap_or_default(),
            ..profile
        })
    }

    /// Upsert a sync profile
    pub async fn upsert_profile(&self, profile: SyncProfile) -> Result<(), SyncError> {
        let config_json = serde_json::to_string(&profile).map_err(|e| {
            SyncError::InvalidProfile(format!("Failed to serialize profile config: {}", e))
        })?;

        let credential_refs_json = serde_json::to_string(&profile.credential_refs).ok();

        let provider_str = provider_kind_to_db(&profile.provider);

        sqlx::query(
            r#"
            INSERT INTO sync_profiles (id, name, provider, remote_root, config_json, credential_refs_json, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, datetime('now'))
            ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                provider = excluded.provider,
                remote_root = excluded.remote_root,
                config_json = excluded.config_json,
                credential_refs_json = excluded.credential_refs_json,
                updated_at = datetime('now')
            "#,
        )
        .bind(&profile.id)
        .bind(&profile.name)
        .bind(provider_str)
        .bind(&profile.remote_root)
        .bind(&config_json)
        .bind(&credential_refs_json)
        .execute(&self.pool)
        .await?;

        log::info!("[Sync::Repository] Upserted profile: {}", profile.id);
        Ok(())
    }

    /// Update persisted profile pause state without disturbing credentials.
    pub async fn set_profile_paused(
        &self,
        profile_id: &str,
        paused: bool,
    ) -> Result<(), SyncError> {
        let mut profile = self.get_profile(profile_id).await?;
        profile.schedule.paused = paused;
        self.upsert_profile(profile).await?;
        Ok(())
    }

    /// Delete a sync profile
    pub async fn delete_profile(&self, profile_id: &str) -> Result<(), SyncError> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            "DELETE FROM file_sync_chunks WHERE entry_id IN (SELECT id FROM file_sync_entries WHERE profile_id = ?)",
        )
        .bind(profile_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "DELETE FROM file_sync_cache WHERE entry_id IN (SELECT id FROM file_sync_entries WHERE profile_id = ?)",
        )
        .bind(profile_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM file_sync_entries WHERE profile_id = ?")
            .bind(profile_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM file_sync_remote_cursors WHERE profile_id = ?")
            .bind(profile_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM file_sync_profile_seq WHERE profile_id = ?")
            .bind(profile_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM file_sync_tombstones WHERE profile_id = ?")
            .bind(profile_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            "UPDATE file_sync_settings SET default_profile_id = NULL, updated_at = datetime('now') WHERE default_profile_id = ?",
        )
        .bind(profile_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM sync_profiles WHERE id = ?")
            .bind(profile_id)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;

        log::info!("[Sync::Repository] Deleted profile: {}", profile_id);
        Ok(())
    }

    /// Persist a user-visible sync log entry.
    pub async fn record_log(
        &self,
        profile_id: Option<&str>,
        run_id: Option<&str>,
        level: &str,
        message: &str,
    ) -> Result<(), SyncError> {
        sqlx::query(
            r#"
            INSERT INTO sync_logs (profile_id, run_id, level, message, created_at)
            VALUES (?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(profile_id)
        .bind(run_id)
        .bind(level)
        .bind(message)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Read recent sync log entries for a profile.
    pub async fn list_log_entries(
        &self,
        profile_id: &str,
        limit: i64,
    ) -> Result<Vec<SyncLogEntry>, SyncError> {
        let limit = limit.clamp(1, 500);
        let entries = sqlx::query_as::<_, SyncLogEntry>(
            r#"
            SELECT created_at,
                   level,
                   message,
                   profile_id,
                   run_id
            FROM sync_logs
            WHERE profile_id = ? OR profile_id IS NULL
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(profile_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(entries)
    }

    /// Persist the latest run report so status survives app restart.
    pub async fn save_run_report(&self, report: &SyncRunReport) -> Result<(), SyncError> {
        let report_json = serde_json::to_string(report).map_err(SyncError::Serialization)?;
        sqlx::query(
            r#"
            INSERT INTO sync_run_reports
                (profile_id, run_id, report_json, status, started_at, completed_at, updated_at)
            VALUES (?, ?, ?, ?, ?, ?, datetime('now'))
            ON CONFLICT(profile_id) DO UPDATE SET
                run_id = excluded.run_id,
                report_json = excluded.report_json,
                status = excluded.status,
                started_at = excluded.started_at,
                completed_at = excluded.completed_at,
                updated_at = datetime('now')
            "#,
        )
        .bind(&report.profile_id)
        .bind(&report.run_id)
        .bind(report_json)
        .bind(format!("{:?}", report.status))
        .bind(&report.started_at)
        .bind(&report.completed_at)
        .execute(&self.pool)
        .await?;

        if report.status == SyncRunStatus::Completed {
            sqlx::query(
                r#"
                INSERT INTO sync_state (scope, scope_id, provider, cursor, last_sync_at)
                VALUES ('profile', ?, 'sync', NULL, ?)
                ON CONFLICT(scope, scope_id, provider) DO UPDATE SET
                    last_sync_at = excluded.last_sync_at
                "#,
            )
            .bind(&report.profile_id)
            .bind(&report.completed_at)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    /// Load the most recent persisted run report for a profile.
    pub async fn get_last_run_report(
        &self,
        profile_id: &str,
    ) -> Result<Option<SyncRunReport>, SyncError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT report_json FROM sync_run_reports WHERE profile_id = ?")
                .bind(profile_id)
                .fetch_optional(&self.pool)
                .await?;

        row.map(|(json,)| serde_json::from_str(&json).map_err(SyncError::Serialization))
            .transpose()
    }

    /// Return a stored KDF context for encrypted profiles.
    pub fn crypto_context_from_profile(
        profile: &SyncProfile,
    ) -> Result<Option<SyncCryptoContext>, SyncError> {
        if !profile.encryption.enabled {
            return Ok(None);
        }

        let salt_b64 = profile.encryption.salt_b64.as_ref().ok_or_else(|| {
            SyncError::Encryption("Encrypted profile is missing KDF salt".to_string())
        })?;
        let salt = base64::engine::general_purpose::STANDARD
            .decode(salt_b64)
            .map_err(|e| SyncError::Encryption(format!("Invalid KDF salt: {}", e)))?;

        Ok(Some(SyncCryptoContext {
            algorithm: profile.encryption.algorithm.clone(),
            kdf: profile.encryption.kdf.clone(),
            salt,
            memory_kb: profile.encryption.memory_kb,
            iterations: profile.encryption.iterations,
            parallelism: profile.encryption.parallelism,
        }))
    }

    /// Get or create item key mapping
    pub async fn get_or_create_item_key(
        &self,
        local_id: i64,
        created_at: chrono::DateTime<chrono::Utc>,
        device_id: &str,
    ) -> Result<String, SyncError> {
        // Check if mapping exists
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT item_key FROM sync_item_map WHERE local_id = ?")
                .bind(local_id)
                .fetch_optional(&self.pool)
                .await?;

        if let Some((item_key,)) = existing {
            return Ok(item_key);
        }

        // Create new item key
        let created_at_ms = created_at.timestamp_millis();
        let item_key = format!("{}_{}_{}", device_id, local_id, created_at_ms);

        self.insert_item_mapping(local_id, &item_key, "", None)
            .await?;

        Ok(item_key)
    }

    async fn next_stable_seq(&self) -> Result<i64, SyncError> {
        let next: Option<(Option<i64>,)> =
            sqlx::query_as("SELECT MAX(stable_seq) FROM sync_item_map")
                .fetch_optional(&self.pool)
                .await?;
        Ok(next.and_then(|(value,)| value).unwrap_or(0) + 1)
    }

    async fn insert_item_mapping(
        &self,
        local_id: i64,
        item_key: &str,
        remote_path: &str,
        stable_seq_hint: Option<i64>,
    ) -> Result<i64, SyncError> {
        let stable_seq = match stable_seq_hint {
            Some(value) if value > 0 => value,
            _ => self.next_stable_seq().await?,
        };

        sqlx::query(
            r#"
            INSERT INTO sync_item_map
                (local_id, item_key, stable_seq, remote_path, last_synced_at)
            VALUES (?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(local_id)
        .bind(item_key)
        .bind(stable_seq)
        .bind(remote_path)
        .execute(&self.pool)
        .await?;

        Ok(stable_seq)
    }

    async fn find_local_id_by_item_key(&self, item_key: &str) -> Result<Option<i64>, SyncError> {
        let existing: Option<(i64,)> =
            sqlx::query_as("SELECT local_id FROM sync_item_map WHERE item_key = ?")
                .bind(item_key)
                .fetch_optional(&self.pool)
                .await?;

        Ok(existing.map(|(local_id,)| local_id))
    }

    async fn upsert_item_mapping_with_stable_seq(
        &self,
        local_id: i64,
        item_key: &str,
        stable_seq: i64,
        remote_path: &str,
        last_remote_updated_at: Option<&str>,
    ) -> Result<(), SyncError> {
        let stable_seq = if stable_seq > 0 {
            stable_seq
        } else {
            self.next_stable_seq().await?
        };
        sqlx::query(
            r#"
            INSERT INTO sync_item_map
                (local_id, item_key, stable_seq, remote_path, last_remote_updated_at, last_synced_at)
            VALUES (?, ?, ?, ?, ?, datetime('now'))
            ON CONFLICT(item_key) DO UPDATE SET
                local_id = excluded.local_id,
                stable_seq = excluded.stable_seq,
                remote_path = excluded.remote_path,
                last_remote_updated_at = excluded.last_remote_updated_at,
                last_synced_at = datetime('now')
            "#,
        )
        .bind(local_id)
        .bind(item_key)
        .bind(stable_seq)
        .bind(remote_path)
        .bind(last_remote_updated_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn local_has_pending_change(&self, local_id: i64) -> Result<bool, SyncError> {
        let pending: Option<(i64,)> = sqlx::query_as(
            r#"
            SELECT 1
            FROM sync_changes
            WHERE entity_type = 'clipboard_item'
              AND entity_id = ?
              AND synced_at IS NULL
              AND source IN ('local', 'sync_resolution')
            LIMIT 1
            "#,
        )
        .bind(local_id.to_string())
        .fetch_optional(&self.pool)
        .await?;

        Ok(pending.is_some())
    }

    async fn local_content_matches_remote(
        &self,
        local_id: i64,
        remote_item: &RemoteClipboardItem,
    ) -> Result<bool, SyncError> {
        let Some(remote_content) = remote_item.content.as_deref() else {
            return Ok(false);
        };
        let local: Option<(String, String)> =
            sqlx::query_as("SELECT type, content FROM clipboard_items WHERE id = ?")
                .bind(local_id)
                .fetch_optional(&self.pool)
                .await?;
        Ok(local
            .map(|(item_type, content)| {
                item_type == remote_item.item_type && content == remote_content
            })
            .unwrap_or(false))
    }

    async fn auto_resolve_same_content_conflicts(&self, item_key: &str) -> Result<(), SyncError> {
        sqlx::query(
            r#"
            UPDATE sync_conflicts
            SET status = 'resolved',
                resolution = 'auto_same_content',
                resolved_at = datetime('now')
            WHERE entity_type = 'clipboard_item'
              AND entity_key = ?
              AND status = 'pending'
            "#,
        )
        .bind(item_key)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn create_clipboard_conflict(
        &self,
        item_key: &str,
        local_id: i64,
        remote_item: &RemoteClipboardItem,
        reason: &str,
    ) -> Result<i64, SyncError> {
        let local_payload: Option<(String,)> =
            sqlx::query_as("SELECT json_object('id', id, 'type', type, 'content', content, 'content_hash', content_hash, 'metadata', metadata, 'tags', tags, 'tab_id', tab_id, 'is_sensitive', is_sensitive, 'is_pinned', is_pinned, 'updated_at', updated_at) FROM clipboard_items WHERE id = ?")
                .bind(local_id)
                .fetch_optional(&self.pool)
                .await?;
        let Some((local_payload,)) = local_payload else {
            return Err(SyncError::validation(format!(
                "Local item {} for conflict {} not found",
                local_id, item_key
            )));
        };
        let remote_payload =
            serde_json::to_string(remote_item).map_err(SyncError::Serialization)?;

        let result = sqlx::query(
            r#"
            INSERT INTO sync_conflicts
                (entity_type, entity_key, local_payload, remote_payload, reason, status, created_at)
            VALUES ('clipboard_item', ?, ?, ?, ?, 'pending', datetime('now'))
            "#,
        )
        .bind(item_key)
        .bind(local_payload)
        .bind(remote_payload)
        .bind(reason)
        .execute(&self.pool)
        .await?;

        Ok(result.last_insert_rowid())
    }

    /// Record a clipboard change in the sync outbox
    pub async fn record_clipboard_change(
        &self,
        entity_id: i64,
        operation: &str,
        source: &str,
        item_key: Option<&str>,
    ) -> Result<(), SyncError> {
        sqlx::query(
            r#"
            INSERT INTO sync_changes
                (entity_type, entity_id, operation, item_key, source, changed_at, synced_at)
            VALUES (
                'clipboard_item',
                ?,
                ?,
                ?,
                ?,
                datetime('now'),
                CASE WHEN ? = 'remote_apply' THEN datetime('now') ELSE NULL END
            )
            "#,
        )
        .bind(entity_id)
        .bind(operation)
        .bind(item_key)
        .bind(source)
        .bind(source)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// List unsynced changes for a profile
    pub async fn list_unsynced_changes(
        &self,
        profile: &SyncProfile,
    ) -> Result<Vec<LocalSyncChange>, SyncError> {
        #[allow(clippy::type_complexity)]
        let changes: Vec<(
            i64,
            String,
            String,
            String,
            Option<String>,
            Option<i64>,
            Option<String>,
            String,
            String,
            Option<String>,
        )> = sqlx::query_as(
            r#"
            SELECT sc.id,
                   sc.entity_type,
                   sc.entity_id,
                   sc.operation,
                   sc.item_key,
                   COALESCE(sc.tab_id, ci.tab_id) AS tab_id,
                   sc.plugin_id,
                   sc.source,
                   sc.changed_at,
                   sc.synced_at
            FROM sync_changes sc
            LEFT JOIN clipboard_items ci
              ON sc.entity_type = 'clipboard_item'
             AND sc.entity_id = CAST(ci.id AS TEXT)
            WHERE sc.synced_at IS NULL AND sc.source IN ('local', 'sync_resolution')
            ORDER BY sc.changed_at ASC
            LIMIT 1000
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        let mut changes: Vec<LocalSyncChange> = changes
            .into_iter()
            .map(
                |(
                    id,
                    entity_type,
                    entity_id,
                    operation,
                    item_key,
                    tab_id,
                    plugin_id,
                    source,
                    changed_at,
                    synced_at,
                )| {
                    LocalSyncChange {
                        id,
                        entity_type,
                        entity_id,
                        operation,
                        item_key,
                        tab_id,
                        plugin_id,
                        source,
                        changed_at,
                        synced_at,
                    }
                },
            )
            .collect();

        if profile.sync_tabs.mode == TabSyncMode::Selected {
            let excluded = &profile.sync_tabs.selected_tab_ids;
            changes.retain(|change| {
                change.entity_type != "clipboard_item"
                    || change
                        .tab_id
                        .map(|id| !excluded.contains(&id))
                        .unwrap_or(true)
            });
        }

        Ok(changes)
    }

    pub async fn has_unsynced_changes(&self, profile: &SyncProfile) -> Result<bool, SyncError> {
        Ok(!self.list_unsynced_changes(profile).await?.is_empty())
    }

    /// Return the current local clipboard snapshot with stable remote identity.
    pub async fn list_snapshot_items(
        &self,
        profile: &SyncProfile,
        device_id: &str,
    ) -> Result<Vec<RemoteClipboardItem>, SyncError> {
        let mut sql = String::from(
            r#"
            SELECT ci.id,
                   ci.type,
                   ci.content,
                   ci.content_hash,
                   ci.metadata,
                   ci.tags,
                   ci.tab_id,
                   t.name,
                   COALESCE(t.is_default, 0),
                   ci.is_sensitive,
                   ci.is_pinned,
                   ci.display_order,
                   ci.created_at,
                   ci.updated_at,
                   sim.item_key,
                   sim.stable_seq
            FROM clipboard_items ci
            LEFT JOIN sync_item_map sim ON sim.local_id = ci.id
            LEFT JOIN tabs t ON t.id = ci.tab_id
            "#,
        );

        if profile.sync_tabs.mode == TabSyncMode::Selected
            && !profile.sync_tabs.selected_tab_ids.is_empty()
        {
            let placeholders = profile
                .sync_tabs
                .selected_tab_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");
            sql.push_str(&format!(
                " WHERE (ci.tab_id IS NULL OR ci.tab_id NOT IN ({}))",
                placeholders
            ));
        }

        sql.push_str(" ORDER BY COALESCE(sim.stable_seq, ci.id), ci.id");

        type SnapshotRow = (
            Option<i64>,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i64>,
            Option<String>,
            i32,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<chrono::DateTime<chrono::Utc>>,
            Option<String>,
            Option<i64>,
        );

        let mut query = sqlx::query_as::<_, SnapshotRow>(&sql);
        if profile.sync_tabs.mode == TabSyncMode::Selected {
            for tab_id in &profile.sync_tabs.selected_tab_ids {
                query = query.bind(tab_id);
            }
        }

        let rows = query.fetch_all(&self.pool).await?;
        let mut items = Vec::with_capacity(rows.len());

        for row in rows {
            let (
                local_id,
                item_type,
                content,
                content_hash,
                metadata,
                tags,
                tab_id,
                tab_name,
                tab_is_default,
                is_sensitive,
                is_pinned,
                _display_order,
                created_at,
                updated_at,
                item_key,
                stable_seq,
            ) = row;
            let local_id = local_id.ok_or_else(|| {
                SyncError::validation("Snapshot item is missing local id".to_string())
            })?;
            let created_at = created_at.unwrap_or_else(chrono::Utc::now);
            let (item_key, stable_seq) = match item_key {
                Some(item_key) => (item_key, stable_seq),
                None => {
                    let item_key = self
                        .get_or_create_item_key(local_id, created_at, device_id)
                        .await?;
                    let stored_stable_seq: Option<(Option<i64>,)> =
                        sqlx::query_as("SELECT stable_seq FROM sync_item_map WHERE local_id = ?")
                            .bind(local_id)
                            .fetch_optional(&self.pool)
                            .await?;
                    (item_key, stored_stable_seq.and_then(|(value,)| value))
                }
            };
            let stable_seq = match stable_seq {
                Some(value) if value > 0 => value,
                _ => {
                    let next = self.next_stable_seq().await?;
                    sqlx::query("UPDATE sync_item_map SET stable_seq = ? WHERE local_id = ?")
                        .bind(next)
                        .bind(local_id)
                        .execute(&self.pool)
                        .await?;
                    next
                }
            };

            items.push(RemoteClipboardItem {
                schema_version: 2,
                item_key,
                stable_seq,
                device_id: device_id.to_string(),
                local_id: Some(local_id),
                item_type,
                content: Some(content),
                blob_path: None,
                blob_mime: None,
                content_hash,
                created_at: created_at.to_rfc3339(),
                updated_at: updated_at.unwrap_or_else(chrono::Utc::now).to_rfc3339(),
                tab_key: Some(tab_key_for_snapshot(
                    tab_id,
                    tab_name.as_deref(),
                    tab_is_default != 0,
                )),
                tab_name,
                tags: parse_sync_tags(tags.as_deref()),
                is_pinned: is_pinned.unwrap_or(0) != 0,
                is_sensitive: is_sensitive.unwrap_or(0) != 0,
                metadata: metadata
                    .as_deref()
                    .and_then(|value| serde_json::from_str(value).ok())
                    .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new())),
                revision: 0,
                last_modified_by: device_id.to_string(),
                deleted: false,
            });
        }

        items.sort_by_key(|item| item.stable_seq);
        Ok(items)
    }

    pub async fn build_snapshot_order(
        &self,
        profile: &SyncProfile,
    ) -> Result<SnapshotOrder, SyncError> {
        let mut sql = String::from(
            r#"
            SELECT t.id,
                   t.name,
                   COALESCE(t.is_default, 0),
                   COALESCE(ci.is_pinned, 0),
                   sim.item_key
            FROM tabs t
            LEFT JOIN clipboard_items ci ON ci.tab_id = t.id
            LEFT JOIN sync_item_map sim ON sim.local_id = ci.id
            "#,
        );
        if profile.sync_tabs.mode == TabSyncMode::Selected
            && !profile.sync_tabs.selected_tab_ids.is_empty()
        {
            let placeholders = profile
                .sync_tabs
                .selected_tab_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");
            sql.push_str(&format!(" WHERE t.id NOT IN ({})", placeholders));
        }
        sql.push_str(" ORDER BY t.id, ci.is_pinned DESC, ci.display_order ASC, ci.updated_at DESC");

        let mut query = sqlx::query_as::<_, (i64, String, i32, i32, Option<String>)>(&sql);
        if profile.sync_tabs.mode == TabSyncMode::Selected {
            for tab_id in &profile.sync_tabs.selected_tab_ids {
                query = query.bind(tab_id);
            }
        }

        let rows = query.fetch_all(&self.pool).await?;
        let mut tabs: Vec<SnapshotTabOrder> = Vec::new();
        for (tab_id, tab_name, tab_is_default, is_pinned, item_key) in rows {
            let tab_key = tab_key_for_snapshot(Some(tab_id), Some(&tab_name), tab_is_default != 0);
            let tab_index = match tabs.iter().position(|tab| tab.tab_key == tab_key) {
                Some(index) => index,
                None => {
                    tabs.push(SnapshotTabOrder {
                        tab_key: tab_key.clone(),
                        tab_name: (tab_is_default == 0).then_some(tab_name.clone()),
                        pinned: Vec::new(),
                        normal: Vec::new(),
                    });
                    tabs.len() - 1
                }
            };
            let tab = &mut tabs[tab_index];
            if let Some(item_key) = item_key {
                if is_pinned != 0 {
                    tab.pinned.push(item_key);
                } else {
                    tab.normal.push(item_key);
                }
            }
        }

        Ok(SnapshotOrder {
            schema_version: 1,
            updated_at: chrono::Utc::now().to_rfc3339(),
            tabs,
        })
    }

    /// Enqueue existing clipboard items for a first manual sync.
    ///
    /// The sync outbox only captures mutations that happen after the sync
    /// tables exist. This seeds a profile with current clipboard history so a
    /// newly configured profile can upload useful data on its first run.
    pub async fn enqueue_initial_clipboard_snapshot(
        &self,
        profile: &SyncProfile,
    ) -> Result<u64, SyncError> {
        let mut sql = String::from(
            r#"
            INSERT INTO sync_changes (entity_type, entity_id, operation, tab_id, source, changed_at)
            SELECT 'clipboard_item', CAST(ci.id AS TEXT), 'create', ci.tab_id, 'local', datetime('now')
            FROM clipboard_items ci
            WHERE NOT EXISTS (
                SELECT 1 FROM sync_item_map sim WHERE sim.local_id = ci.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM sync_changes sc
                WHERE sc.entity_type = 'clipboard_item'
                  AND sc.entity_id = CAST(ci.id AS TEXT)
                  AND sc.synced_at IS NULL
            )
            "#,
        );

        if profile.sync_tabs.mode == TabSyncMode::Selected
            && !profile.sync_tabs.selected_tab_ids.is_empty()
        {
            let placeholders = profile
                .sync_tabs
                .selected_tab_ids
                .iter()
                .map(|_| "?")
                .collect::<Vec<_>>()
                .join(", ");
            sql.push_str(&format!(
                " AND (ci.tab_id IS NULL OR ci.tab_id NOT IN ({}))",
                placeholders
            ));
        }

        sql.push_str(" ORDER BY ci.updated_at DESC LIMIT 1000");

        let mut query = sqlx::query(&sql);
        if profile.sync_tabs.mode == TabSyncMode::Selected {
            for tab_id in &profile.sync_tabs.selected_tab_ids {
                query = query.bind(tab_id);
            }
        }

        let result = query.execute(&self.pool).await?;
        let inserted = result.rows_affected();
        if inserted > 0 {
            log::info!(
                "[Sync::Repository] Enqueued {} existing clipboard items for initial sync",
                inserted
            );
        }
        Ok(inserted)
    }

    /// Fetch a local clipboard item for upload encoding.
    pub async fn get_local_clipboard_item(
        &self,
        local_id: i64,
    ) -> Result<Option<ClipboardItem>, SyncError> {
        let item = sqlx::query_as::<_, ClipboardItem>("SELECT * FROM clipboard_items WHERE id = ?")
            .bind(local_id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(item)
    }

    /// Mark changes as synced
    pub async fn mark_changes_synced(&self, change_ids: &[i64]) -> Result<(), SyncError> {
        if change_ids.is_empty() {
            return Ok(());
        }

        let placeholders: Vec<String> = change_ids.iter().map(|_| "?".to_string()).collect();
        let sql = format!(
            "UPDATE sync_changes SET synced_at = datetime('now') WHERE id IN ({})",
            placeholders.join(", ")
        );

        let mut query = sqlx::query(&sql);
        for &id in change_ids {
            query = query.bind(id);
        }
        query.execute(&self.pool).await?;

        Ok(())
    }

    /// Get remote cursor for a device
    pub async fn get_remote_cursor(
        &self,
        profile_id: &str,
        remote_device_id: &str,
    ) -> Result<i64, SyncError> {
        let cursor: Option<(i64,)> = sqlx::query_as(
            "SELECT last_seq FROM sync_remote_cursors WHERE profile_id = ? AND remote_device_id = ?"
        )
        .bind(profile_id)
        .bind(remote_device_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(cursor.map(|(seq,)| seq).unwrap_or(0))
    }

    /// Update remote cursor
    pub async fn update_remote_cursor(
        &self,
        profile_id: &str,
        remote_device_id: &str,
        seq: i64,
    ) -> Result<(), SyncError> {
        sqlx::query(
            r#"
            INSERT INTO sync_remote_cursors (profile_id, remote_device_id, last_seq, updated_at)
            VALUES (?, ?, ?, datetime('now'))
            ON CONFLICT(profile_id, remote_device_id) DO UPDATE SET
                last_seq = excluded.last_seq,
                updated_at = datetime('now')
            "#,
        )
        .bind(profile_id)
        .bind(remote_device_id)
        .bind(seq)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Apply a remote item to local database
    pub async fn apply_remote_item(
        &self,
        _profile: &SyncProfile,
        item: RemoteClipboardItem,
        _blob: Option<Vec<u8>>,
    ) -> Result<ApplyItemResult, SyncError> {
        log::info!("[Sync::Repository] Applying remote item: {}", item.item_key);

        let existing_local_id = self.find_local_id_by_item_key(&item.item_key).await?;

        if let Some(local_id) = existing_local_id {
            if self.local_has_pending_change(local_id).await? {
                if self.local_content_matches_remote(local_id, &item).await? {
                    self.auto_resolve_same_content_conflicts(&item.item_key)
                        .await?;
                    log::info!(
                        "[Sync::Repository] Auto-resolved same-content conflict for {}",
                        item.item_key
                    );
                    return Ok(ApplyItemResult::Merged { local_id });
                }
                let conflict_id = self
                    .create_clipboard_conflict(
                        &item.item_key,
                        local_id,
                        &item,
                        "local item has unsynced changes",
                    )
                    .await?;
                log::warn!(
                    "[Sync::Repository] Created conflict {} for item_key {}",
                    conflict_id,
                    item.item_key
                );
                return Ok(ApplyItemResult::Conflict { conflict_id });
            }

            // Item exists, update it
            let tags_str = if item.tags.is_empty() {
                None
            } else {
                Some(item.tags.join(","))
            };

            let metadata_json =
                serde_json::to_string(&item.metadata).unwrap_or_else(|_| "{}".to_string());
            let tab_id = local_id_for_remote_tab(
                item.tab_key.as_deref(),
                item.tab_name.as_deref(),
                &self.pool,
            )
            .await?;

            sqlx::query(
                r#"
                UPDATE clipboard_items 
                SET content = COALESCE(?, content),
                    type = ?,
                    content_hash = ?,
                    tags = COALESCE(?, tags),
                    metadata = ?,
                    tab_id = ?,
                    is_pinned = ?,
                    is_sensitive = ?,
                    updated_at = datetime('now')
                WHERE id = ?
                "#,
            )
            .bind(&item.content)
            .bind(&item.item_type)
            .bind(&item.content_hash)
            .bind(&tags_str)
            .bind(&metadata_json)
            .bind(tab_id)
            .bind(if item.is_pinned { 1 } else { 0 })
            .bind(if item.is_sensitive { 1 } else { 0 })
            .bind(local_id)
            .execute(&self.pool)
            .await?;

            log::info!("[Sync::Repository] Updated local item {}", local_id);

            self.upsert_item_mapping_with_stable_seq(
                local_id,
                &item.item_key,
                item.stable_seq,
                &format!("items/{}.json", item.item_key),
                Some(&item.updated_at),
            )
            .await?;

            // Record sync_change with source='remote_apply'
            self.record_clipboard_change(
                local_id,
                "update",
                &ChangeSource::RemoteApply.to_string(),
                Some(&item.item_key),
            )
            .await?;

            Ok(ApplyItemResult::Updated { local_id })
        } else {
            // Item doesn't exist locally, create it
            let tags_str = if item.tags.is_empty() {
                None
            } else {
                Some(item.tags.join(","))
            };

            let metadata_json =
                serde_json::to_string(&item.metadata).unwrap_or_else(|_| "{}".to_string());

            let tab_id = local_id_for_remote_tab(
                item.tab_key.as_deref(),
                item.tab_name.as_deref(),
                &self.pool,
            )
            .await?;

            let result = sqlx::query(
                r#"
                INSERT INTO clipboard_items 
                (type, content, content_hash, metadata, tags, tab_id, is_sensitive, is_pinned, updated_at)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, datetime('now'))
                "#,
            )
            .bind(&item.item_type)
            .bind(item.content.unwrap_or_default())
            .bind(&item.content_hash)
            .bind(&metadata_json)
            .bind(&tags_str)
            .bind(tab_id)
            .bind(if item.is_sensitive { 1 } else { 0 })
            .bind(if item.is_pinned { 1 } else { 0 })
            .execute(&self.pool)
            .await?;

            let local_id = result.last_insert_rowid();

            log::info!("[Sync::Repository] Created local item {}", local_id);

            self.upsert_item_mapping_with_stable_seq(
                local_id,
                &item.item_key,
                item.stable_seq,
                &format!("items/{}.json", item.item_key),
                Some(&item.updated_at),
            )
            .await?;

            // Record sync_change with source='remote_apply'
            self.record_clipboard_change(
                local_id,
                "create",
                &ChangeSource::RemoteApply.to_string(),
                Some(&item.item_key),
            )
            .await?;

            Ok(ApplyItemResult::Created { local_id })
        }
    }

    /// Apply a remote tombstone
    pub async fn apply_remote_tombstone(
        &self,
        _profile: &SyncProfile,
        tombstone: RemoteTombstone,
    ) -> Result<(), SyncError> {
        log::info!(
            "[Sync::Repository] Applying tombstone for item: {}",
            tombstone.item_key
        );

        if let Some(local_id) = self.find_local_id_by_item_key(&tombstone.item_key).await? {
            let result = sqlx::query("DELETE FROM clipboard_items WHERE id = ?")
                .bind(local_id)
                .execute(&self.pool)
                .await?;

            if result.rows_affected() > 0 {
                sqlx::query("DELETE FROM sync_item_map WHERE item_key = ?")
                    .bind(&tombstone.item_key)
                    .execute(&self.pool)
                    .await?;
                log::info!("[Sync::Repository] Deleted local item {}", local_id);
                return Ok(());
            }
        }

        log::info!(
            "[Sync::Repository] No local item found for tombstone: {}",
            tombstone.item_key
        );
        Ok(())
    }

    pub async fn apply_snapshot_items(
        &self,
        profile: &SyncProfile,
        items: Vec<RemoteClipboardItem>,
        prune_missing: bool,
    ) -> Result<i64, SyncError> {
        let mut applied = 0;
        let remote_keys: std::collections::HashSet<String> =
            items.iter().map(|item| item.item_key.clone()).collect();
        for item in items {
            if item.deleted {
                continue;
            }
            match self.apply_remote_item(profile, item, None).await? {
                ApplyItemResult::Created { .. }
                | ApplyItemResult::Updated { .. }
                | ApplyItemResult::Merged { .. } => applied += 1,
                ApplyItemResult::Conflict { .. } | ApplyItemResult::Skipped => {}
            }
        }
        let deduplicated = self.deduplicate_same_content_items(profile).await?;
        if deduplicated > 0 {
            log::info!(
                "[Sync::Repository] Removed {} duplicate clipboard items",
                deduplicated
            );
        }
        if prune_missing {
            self.prune_items_missing_from_snapshot(profile, &remote_keys)
                .await?;
        }
        Ok(applied)
    }

    pub async fn deduplicate_same_content_items(
        &self,
        profile: &SyncProfile,
    ) -> Result<i64, SyncError> {
        let duplicate_groups: Vec<(Option<i64>, String, String)> = sqlx::query_as(
            r#"
            SELECT tab_id, type, content
            FROM clipboard_items
            WHERE content IS NOT NULL
            GROUP BY tab_id, type, content
            HAVING COUNT(*) > 1
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        let mut removed = 0;

        for (tab_id, item_type, content) in duplicate_groups {
            if profile.sync_tabs.mode == TabSyncMode::Selected
                && tab_id
                    .map(|id| profile.sync_tabs.selected_tab_ids.contains(&id))
                    .unwrap_or(false)
            {
                continue;
            }

            type DuplicateRow = (
                i64,
                Option<String>,
                Option<i64>,
                Option<String>,
                Option<String>,
                Option<String>,
                i64,
            );
            let rows: Vec<DuplicateRow> = sqlx::query_as(
                r#"
                SELECT ci.id,
                       sim.item_key,
                       sim.stable_seq,
                       sim.remote_path,
                       sim.last_remote_updated_at,
                       sim.last_synced_at,
                       EXISTS (
                           SELECT 1
                           FROM sync_changes sc
                           WHERE sc.entity_type = 'clipboard_item'
                             AND sc.entity_id = CAST(ci.id AS TEXT)
                             AND sc.synced_at IS NULL
                             AND sc.source IN ('local', 'sync_resolution')
                       ) AS has_pending
                FROM clipboard_items ci
                LEFT JOIN sync_item_map sim ON sim.local_id = ci.id
                WHERE ci.tab_id IS ?
                  AND ci.type = ?
                  AND ci.content = ?
                ORDER BY has_pending DESC,
                         sim.item_key IS NULL,
                         sim.item_key ASC,
                         ci.updated_at DESC,
                         ci.id ASC
                "#,
            )
            .bind(tab_id)
            .bind(&item_type)
            .bind(&content)
            .fetch_all(&self.pool)
            .await?;
            if rows.len() < 2 {
                continue;
            }

            let winner_id = rows[0].0;
            let canonical_mapping = rows
                .iter()
                .filter_map(|row| {
                    row.1.as_ref().map(|item_key| {
                        (
                            item_key.clone(),
                            row.2,
                            row.3.clone(),
                            row.4.clone(),
                            row.5.clone(),
                        )
                    })
                })
                .min_by(|left, right| left.0.cmp(&right.0));
            let loser_ids: Vec<i64> = rows.iter().skip(1).map(|row| row.0).collect();
            let duplicate_keys: Vec<String> = rows.iter().filter_map(|row| row.1.clone()).collect();

            let mut tx = self.pool.begin().await?;
            for row in &rows {
                sqlx::query("DELETE FROM sync_item_map WHERE local_id = ?")
                    .bind(row.0)
                    .execute(&mut *tx)
                    .await?;
            }
            if let Some((item_key, stable_seq, remote_path, remote_updated_at, last_synced_at)) =
                canonical_mapping
            {
                sqlx::query(
                    r#"
                    INSERT INTO sync_item_map
                        (local_id, item_key, stable_seq, remote_path, last_remote_updated_at, last_synced_at)
                    VALUES (?, ?, ?, ?, ?, ?)
                    "#,
                )
                .bind(winner_id)
                .bind(item_key)
                .bind(stable_seq)
                .bind(remote_path.unwrap_or_default())
                .bind(remote_updated_at)
                .bind(last_synced_at)
                .execute(&mut *tx)
                .await?;
            }
            for loser_id in &loser_ids {
                sqlx::query(
                    r#"
                    UPDATE sync_changes
                    SET synced_at = datetime('now')
                    WHERE entity_type = 'clipboard_item'
                      AND entity_id = ?
                      AND synced_at IS NULL
                    "#,
                )
                .bind(loser_id.to_string())
                .execute(&mut *tx)
                .await?;
                sqlx::query("DELETE FROM clipboard_items WHERE id = ?")
                    .bind(loser_id)
                    .execute(&mut *tx)
                    .await?;
            }
            for item_key in duplicate_keys {
                sqlx::query(
                    r#"
                    UPDATE sync_conflicts
                    SET status = 'resolved',
                        resolution = 'auto_same_content',
                        resolved_at = datetime('now')
                    WHERE entity_type = 'clipboard_item'
                      AND entity_key = ?
                      AND status = 'pending'
                    "#,
                )
                .bind(item_key)
                .execute(&mut *tx)
                .await?;
            }
            sqlx::query(
                r#"
                INSERT INTO sync_changes
                    (entity_type, entity_id, operation, item_key, source, changed_at)
                VALUES (
                    'clipboard_item',
                    ?,
                    'deduplicate',
                    (SELECT item_key FROM sync_item_map WHERE local_id = ?),
                    'sync_resolution',
                    datetime('now')
                )
                "#,
            )
            .bind(winner_id.to_string())
            .bind(winner_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            removed += loser_ids.len() as i64;
        }

        Ok(removed)
    }

    async fn prune_items_missing_from_snapshot(
        &self,
        profile: &SyncProfile,
        remote_keys: &std::collections::HashSet<String>,
    ) -> Result<(), SyncError> {
        let local_mappings: Vec<(i64, String, Option<i64>)> = sqlx::query_as(
            r#"
            SELECT sim.local_id, sim.item_key, ci.tab_id
            FROM sync_item_map sim
            JOIN clipboard_items ci ON ci.id = sim.local_id
            "#,
        )
        .fetch_all(&self.pool)
        .await?;
        for (local_id, item_key, tab_id) in local_mappings {
            if profile.sync_tabs.mode == TabSyncMode::Selected
                && tab_id
                    .map(|id| profile.sync_tabs.selected_tab_ids.contains(&id))
                    .unwrap_or(false)
            {
                continue;
            }
            if remote_keys.contains(&item_key) {
                continue;
            }
            sqlx::query("DELETE FROM clipboard_items WHERE id = ?")
                .bind(local_id)
                .execute(&self.pool)
                .await?;
            sqlx::query("DELETE FROM sync_item_map WHERE local_id = ?")
                .bind(local_id)
                .execute(&self.pool)
                .await?;
        }
        Ok(())
    }

    pub async fn apply_snapshot_order(
        &self,
        profile: &SyncProfile,
        order: &SnapshotOrder,
        prune_missing_tabs: bool,
    ) -> Result<(), SyncError> {
        for tab_order in &order.tabs {
            let tab_id = local_id_for_order_tab(
                &tab_order.tab_key,
                tab_order.tab_name.as_deref(),
                &self.pool,
            )
            .await?;
            self.apply_order_group(tab_id, true, &tab_order.pinned)
                .await?;
            self.apply_order_group(tab_id, false, &tab_order.normal)
                .await?;
        }
        if prune_missing_tabs && profile.sync_tabs.mode == TabSyncMode::All {
            self.prune_empty_tabs_missing_from_order(order).await?;
        }
        Ok(())
    }

    async fn prune_empty_tabs_missing_from_order(
        &self,
        order: &SnapshotOrder,
    ) -> Result<(), SyncError> {
        let remote_tab_keys: std::collections::HashSet<&str> =
            order.tabs.iter().map(|tab| tab.tab_key.as_str()).collect();
        let local_tabs: Vec<(i64, String, i32)> =
            sqlx::query_as("SELECT id, name, COALESCE(is_default, 0) FROM tabs")
                .fetch_all(&self.pool)
                .await?;

        for (tab_id, tab_name, is_default) in local_tabs {
            if is_default != 0 {
                continue;
            }
            let tab_key = tab_key_for_snapshot(Some(tab_id), Some(&tab_name), false);
            if remote_tab_keys.contains(tab_key.as_str()) {
                continue;
            }
            let item_count: (i64,) =
                sqlx::query_as("SELECT COUNT(*) FROM clipboard_items WHERE tab_id = ?")
                    .bind(tab_id)
                    .fetch_one(&self.pool)
                    .await?;
            if item_count.0 == 0 {
                sqlx::query("DELETE FROM tabs WHERE id = ? AND is_default = 0")
                    .bind(tab_id)
                    .execute(&self.pool)
                    .await?;
            }
        }
        Ok(())
    }

    async fn apply_order_group(
        &self,
        tab_id: Option<i64>,
        pinned: bool,
        item_keys: &[String],
    ) -> Result<(), SyncError> {
        for (index, item_key) in item_keys.iter().enumerate() {
            let display_order = if pinned {
                -1_000_000 + index as i32
            } else {
                index as i32
            };
            let query = sqlx::query(
                r#"
                UPDATE clipboard_items
                SET display_order = ?, is_pinned = ?, tab_id = ?, updated_at = updated_at
                WHERE id = (SELECT local_id FROM sync_item_map WHERE item_key = ?)
                  AND NOT EXISTS (
                      SELECT 1
                      FROM sync_changes sc
                      WHERE sc.entity_type = 'clipboard_item'
                        AND sc.entity_id = CAST(clipboard_items.id AS TEXT)
                        AND sc.synced_at IS NULL
                        AND sc.source IN ('local', 'sync_resolution')
                  )
                "#,
            )
            .bind(display_order)
            .bind(if pinned { 1 } else { 0 })
            .bind(tab_id)
            .bind(item_key);
            query.execute(&self.pool).await?;
        }
        Ok(())
    }
}

fn tab_key_for_local_id(tab_id: Option<i64>) -> String {
    tab_id
        .map(|id| format!("tab:{}", id))
        .unwrap_or_else(|| "default".to_string())
}

fn tab_key_for_snapshot(tab_id: Option<i64>, tab_name: Option<&str>, is_default: bool) -> String {
    if is_default || tab_id.is_none() {
        return "default".to_string();
    }
    tab_name
        .map(|name| format!("tab-name:{}", name.trim().to_lowercase()))
        .filter(|key| key != "tab-name:")
        .unwrap_or_else(|| tab_key_for_local_id(tab_id))
}

async fn local_id_for_remote_tab(
    tab_key: Option<&str>,
    tab_name: Option<&str>,
    pool: &sqlx::SqlitePool,
) -> Result<Option<i64>, SyncError> {
    if let Some(tab_key) = tab_key {
        if tab_key == "default" {
            return default_tab_id(pool).await;
        }
        if let Some(name) = tab_key.strip_prefix("tab-name:") {
            if let Some(id) = find_tab_id_by_name(name, pool).await? {
                return Ok(Some(id));
            }
        }
        if let Some(id) = tab_key
            .strip_prefix("tab:")
            .and_then(|value| value.parse::<i64>().ok())
        {
            let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM tabs WHERE id = ?")
                .bind(id)
                .fetch_optional(pool)
                .await?;
            if let Some((id,)) = existing {
                return Ok(Some(id));
            }
        }
    }

    if let Some(name) = normalize_remote_tab_name(tab_name) {
        if let Some(id) = find_tab_id_by_name(&name, pool).await? {
            return Ok(Some(id));
        }
        let result = sqlx::query("INSERT INTO tabs (name) VALUES (?)")
            .bind(&name)
            .execute(pool)
            .await?;
        return Ok(Some(result.last_insert_rowid()));
    }

    default_tab_id(pool).await
}

async fn local_id_for_tab_key(
    tab_key: &str,
    pool: &sqlx::SqlitePool,
) -> Result<Option<i64>, SyncError> {
    if tab_key == "default" {
        return default_tab_id(pool).await;
    }
    if let Some(name) = tab_key.strip_prefix("tab-name:") {
        return find_tab_id_by_name(name, pool).await;
    }
    if let Some(id) = tab_key
        .strip_prefix("tab:")
        .and_then(|value| value.parse::<i64>().ok())
    {
        let existing: Option<(i64,)> = sqlx::query_as("SELECT id FROM tabs WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
        return Ok(existing.map(|(id,)| id));
    }
    Ok(None)
}

async fn local_id_for_order_tab(
    tab_key: &str,
    tab_name: Option<&str>,
    pool: &sqlx::SqlitePool,
) -> Result<Option<i64>, SyncError> {
    if tab_key == "default" {
        return default_tab_id(pool).await;
    }

    let portable_name = normalize_remote_tab_name(tab_name).or_else(|| {
        tab_key
            .strip_prefix("tab-name:")
            .and_then(|name| normalize_remote_tab_name(Some(name)))
    });
    if let Some(name) = portable_name {
        if let Some(id) = find_tab_id_by_name(&name, pool).await? {
            return Ok(Some(id));
        }
        let result = sqlx::query("INSERT INTO tabs (name) VALUES (?)")
            .bind(name)
            .execute(pool)
            .await?;
        return Ok(Some(result.last_insert_rowid()));
    }

    local_id_for_tab_key(tab_key, pool).await
}

async fn default_tab_id(pool: &sqlx::SqlitePool) -> Result<Option<i64>, SyncError> {
    let default_tab: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM tabs WHERE is_default = 1 LIMIT 1")
            .fetch_optional(pool)
            .await?;
    Ok(default_tab.map(|(id,)| id))
}

async fn find_tab_id_by_name(
    name: &str,
    pool: &sqlx::SqlitePool,
) -> Result<Option<i64>, SyncError> {
    let existing: Option<(i64,)> =
        sqlx::query_as("SELECT id FROM tabs WHERE lower(name) = lower(?) LIMIT 1")
            .bind(name.trim())
            .fetch_optional(pool)
            .await?;
    Ok(existing.map(|(id,)| id))
}

fn normalize_remote_tab_name(name: Option<&str>) -> Option<String> {
    let name = name?.trim();
    if name.is_empty() {
        return None;
    }
    if ["default", "system clipboard"]
        .iter()
        .any(|reserved| reserved.eq_ignore_ascii_case(name))
    {
        return None;
    }
    Some(name.chars().take(64).collect())
}

fn parse_sync_tags(tags: Option<&str>) -> Vec<String> {
    let Some(tags) = tags else {
        return Vec::new();
    };
    if tags.trim().is_empty() {
        return Vec::new();
    }
    if let Ok(values) = serde_json::from_str::<Vec<String>>(tags) {
        return values;
    }
    tags.split(',')
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    async fn setup_sync_test_db() -> SqlitePool {
        let pool = SqlitePool::connect(":memory:").await.unwrap();
        sqlx::query(
            r#"
            CREATE TABLE tabs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                is_default INTEGER DEFAULT 0,
                auto_capture INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE clipboard_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                type TEXT NOT NULL,
                content TEXT,
                content_hash TEXT,
                metadata TEXT,
                tags TEXT,
                tab_id INTEGER,
                is_sensitive INTEGER DEFAULT 0,
                is_pinned INTEGER DEFAULT 0,
                display_order INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE sync_profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider TEXT NOT NULL,
                remote_root TEXT NOT NULL,
                config_json TEXT NOT NULL,
                credential_refs_json TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE sync_item_map (
                local_id INTEGER NOT NULL,
                item_key TEXT NOT NULL UNIQUE,
                stable_seq INTEGER,
                remote_path TEXT NOT NULL,
                last_remote_updated_at DATETIME,
                last_synced_at DATETIME,
                PRIMARY KEY (local_id)
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE sync_changes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                operation TEXT NOT NULL,
                item_key TEXT,
                tab_id INTEGER,
                plugin_id TEXT,
                source TEXT DEFAULT 'local',
                changed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                synced_at DATETIME
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            CREATE TABLE sync_conflicts (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_type TEXT NOT NULL,
                entity_key TEXT NOT NULL,
                local_payload TEXT NOT NULL,
                remote_payload TEXT NOT NULL,
                reason TEXT NOT NULL,
                status TEXT DEFAULT 'pending',
                resolution TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                resolved_at DATETIME
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query("INSERT INTO tabs (name, is_default) VALUES ('Clipboard', 1)")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn test_profile() -> SyncProfile {
        SyncProfile {
            id: "profile".to_string(),
            name: "Profile".to_string(),
            provider: SyncProviderKind::WebDav,
            remote_root: String::new(),
            sync_tabs: TabSyncSelection::default(),
            sync_plugins: PluginSyncSelection::default(),
            encryption: EncryptionConfig::default(),
            credential_refs: CredentialRefs::default(),
            schedule: SyncScheduleConfig::default(),
            created_at: None,
            updated_at: None,
        }
    }

    fn remote_item(item_key: &str, content: &str, content_hash: &str) -> RemoteClipboardItem {
        RemoteClipboardItem {
            schema_version: 1,
            item_key: item_key.to_string(),
            stable_seq: 0,
            device_id: "device_b".to_string(),
            local_id: None,
            item_type: "text".to_string(),
            content: Some(content.to_string()),
            blob_path: None,
            blob_mime: None,
            content_hash: Some(content_hash.to_string()),
            created_at: "2026-05-15T00:00:00Z".to_string(),
            updated_at: "2026-05-15T00:00:00Z".to_string(),
            tab_key: None,
            tab_name: None,
            tags: Vec::new(),
            is_pinned: false,
            is_sensitive: false,
            metadata: serde_json::json!({}),
            revision: 0,
            last_modified_by: "device_b".to_string(),
            deleted: false,
        }
    }

    #[tokio::test]
    async fn get_profile_returns_upserted_profile_by_id() {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool);
        let profile = test_profile();

        repository.upsert_profile(profile.clone()).await.unwrap();

        let stored = repository.get_profile(&profile.id).await.unwrap();
        assert_eq!(stored.id, profile.id);
        assert_eq!(stored.name, profile.name);
        assert_eq!(stored.provider, profile.provider);
    }

    #[tokio::test]
    async fn snapshot_apply_deduplicates_same_content_from_different_devices() {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let profile = test_profile();

        repository
            .apply_remote_item(
                &profile,
                remote_item("device_b_1_1", "same content", "same-hash"),
                None,
            )
            .await
            .unwrap();
        repository
            .apply_remote_item(
                &profile,
                remote_item("device_c_1_1", "same content", "same-hash"),
                None,
            )
            .await
            .unwrap();
        repository
            .apply_snapshot_items(
                &profile,
                vec![
                    remote_item("device_b_1_1", "same content", "same-hash"),
                    remote_item("device_c_1_1", "same content", "same-hash"),
                ],
                false,
            )
            .await
            .unwrap();

        let item_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM clipboard_items")
            .fetch_one(&pool)
            .await
            .unwrap();
        let map_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM sync_item_map")
            .fetch_one(&pool)
            .await
            .unwrap();
        let canonical_key: (String,) = sqlx::query_as("SELECT item_key FROM sync_item_map LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        let pending_resolution: (i64,) = sqlx::query_as(
            r#"
            SELECT COUNT(*)
            FROM sync_changes
            WHERE operation = 'deduplicate'
              AND source = 'sync_resolution'
              AND synced_at IS NULL
            "#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(item_count.0, 1);
        assert_eq!(map_count.0, 1);
        assert_eq!(canonical_key.0, "device_b_1_1");
        assert_eq!(pending_resolution.0, 1);
    }

    #[tokio::test]
    async fn tombstone_deletes_only_mapped_item_key() {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let profile = test_profile();

        let first = match repository
            .apply_remote_item(
                &profile,
                remote_item("device_b_1_1", "same content", "same-hash"),
                None,
            )
            .await
            .unwrap()
        {
            ApplyItemResult::Created { local_id } => local_id,
            other => panic!("unexpected apply result: {:?}", other),
        };
        repository
            .apply_remote_item(
                &profile,
                remote_item("device_c_1_1", "different content", "different-hash"),
                None,
            )
            .await
            .unwrap();

        repository
            .apply_remote_tombstone(
                &profile,
                RemoteTombstone {
                    schema_version: 1,
                    item_key: "device_b_1_1".to_string(),
                    deleted_at: "2026-05-15T00:01:00Z".to_string(),
                    deleted_by: "device_b".to_string(),
                },
            )
            .await
            .unwrap();

        let remaining_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM clipboard_items")
            .fetch_one(&pool)
            .await
            .unwrap();
        let first_exists: Option<(i64,)> =
            sqlx::query_as("SELECT id FROM clipboard_items WHERE id = ?")
                .bind(first)
                .fetch_optional(&pool)
                .await
                .unwrap();

        assert_eq!(remaining_count.0, 1);
        assert!(first_exists.is_none());
    }

    #[tokio::test]
    async fn same_content_pending_conflict_is_auto_resolved() {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let profile = test_profile();
        let item = remote_item("device_b_1_1", "same content", "same-hash");
        let local_id = match repository
            .apply_remote_item(&profile, item.clone(), None)
            .await
            .unwrap()
        {
            ApplyItemResult::Created { local_id } => local_id,
            other => panic!("unexpected apply result: {:?}", other),
        };
        sqlx::query(
            r#"
            INSERT INTO sync_changes
                (entity_type, entity_id, operation, source, changed_at)
            VALUES ('clipboard_item', ?, 'update', 'local', datetime('now'))
            "#,
        )
        .bind(local_id.to_string())
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            r#"
            INSERT INTO sync_conflicts
                (entity_type, entity_key, local_payload, remote_payload, reason, status)
            VALUES ('clipboard_item', ?, '{}', '{}', 'test', 'pending')
            "#,
        )
        .bind(&item.item_key)
        .execute(&pool)
        .await
        .unwrap();

        let result = repository
            .apply_remote_item(&profile, item, None)
            .await
            .unwrap();
        assert!(matches!(
            result,
            ApplyItemResult::Merged {
                local_id: merged_id
            } if merged_id == local_id
        ));

        let conflict: (String, Option<String>) = sqlx::query_as(
            "SELECT status, resolution FROM sync_conflicts WHERE entity_key = 'device_b_1_1'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(conflict.0, "resolved");
        assert_eq!(conflict.1.as_deref(), Some("auto_same_content"));
    }

    #[tokio::test]
    async fn same_content_in_different_tabs_remains_distinct() {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let profile = test_profile();
        let mut first = remote_item("device_b_1_1", "same content", "same-hash");
        first.tab_key = Some("tab-name:first".to_string());
        first.tab_name = Some("First".to_string());
        let mut second = remote_item("device_c_1_1", "same content", "same-hash");
        second.tab_key = Some("tab-name:second".to_string());
        second.tab_name = Some("Second".to_string());

        repository
            .apply_snapshot_items(&profile, vec![first, second], false)
            .await
            .unwrap();

        let item_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM clipboard_items")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(item_count.0, 2);
    }

    #[tokio::test]
    async fn apply_remote_item_updates_existing_content_hash() {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let profile = test_profile();

        let local_id = match repository
            .apply_remote_item(
                &profile,
                remote_item("device_b_1_1", "old content", "old-hash"),
                None,
            )
            .await
            .unwrap()
        {
            ApplyItemResult::Created { local_id } => local_id,
            other => panic!("unexpected apply result: {:?}", other),
        };

        repository
            .apply_remote_item(
                &profile,
                remote_item("device_b_1_1", "new content", "new-hash"),
                None,
            )
            .await
            .unwrap();

        let stored: (String, Option<String>) =
            sqlx::query_as("SELECT content, content_hash FROM clipboard_items WHERE id = ?")
                .bind(local_id)
                .fetch_one(&pool)
                .await
                .unwrap();

        assert_eq!(stored.0, "new content");
        assert_eq!(stored.1.as_deref(), Some("new-hash"));
    }

    #[tokio::test]
    async fn snapshot_items_include_portable_tab_identity() -> Result<(), SyncError> {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let profile = test_profile();

        let tab_id = sqlx::query("INSERT INTO tabs (name) VALUES ('Research')")
            .execute(&pool)
            .await?
            .last_insert_rowid();
        sqlx::query(
            r#"
            INSERT INTO clipboard_items
                (type, content, tab_id, is_sensitive, is_pinned, created_at, updated_at)
            VALUES ('text', 'tabbed item', ?, 0, 0, datetime('now'), datetime('now'))
            "#,
        )
        .bind(tab_id)
        .execute(&pool)
        .await?;

        let items = repository.list_snapshot_items(&profile, "device_a").await?;
        let item = items
            .iter()
            .find(|item| item.content.as_deref() == Some("tabbed item"))
            .ok_or_else(|| SyncError::validation("missing tabbed snapshot item"))?;

        assert_eq!(item.tab_key.as_deref(), Some("tab-name:research"));
        assert_eq!(item.tab_name.as_deref(), Some("Research"));
        Ok(())
    }

    #[tokio::test]
    async fn apply_remote_item_creates_missing_tab_by_name() -> Result<(), SyncError> {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let profile = test_profile();
        let mut item = remote_item("device_b_1_1", "remote tab item", "remote-tab-hash");
        item.tab_key = Some("tab-name:research".to_string());
        item.tab_name = Some("Research".to_string());

        let local_id = match repository.apply_remote_item(&profile, item, None).await? {
            ApplyItemResult::Created { local_id } => local_id,
            other => panic!("unexpected apply result: {:?}", other),
        };

        let stored: (String, String) = sqlx::query_as(
            r#"
            SELECT ci.content, t.name
            FROM clipboard_items ci
            JOIN tabs t ON t.id = ci.tab_id
            WHERE ci.id = ?
            "#,
        )
        .bind(local_id)
        .fetch_one(&pool)
        .await?;

        assert_eq!(stored.0, "remote tab item");
        assert_eq!(stored.1, "Research");
        Ok(())
    }

    #[tokio::test]
    async fn selected_tab_scope_excludes_only_listed_tabs() -> Result<(), SyncError> {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let excluded_tab_id = sqlx::query("INSERT INTO tabs (name) VALUES ('Excluded')")
            .execute(&pool)
            .await?
            .last_insert_rowid();
        let included_tab_id = sqlx::query("INSERT INTO tabs (name) VALUES ('Included Later')")
            .execute(&pool)
            .await?
            .last_insert_rowid();

        for (content, tab_id) in [
            ("default item", 1),
            ("excluded item", excluded_tab_id),
            ("included later item", included_tab_id),
        ] {
            sqlx::query(
                r#"
                INSERT INTO clipboard_items
                    (type, content, tab_id, is_sensitive, is_pinned, display_order, created_at, updated_at)
                VALUES ('text', ?, ?, 0, 0, 0, datetime('now'), datetime('now'))
                "#,
            )
            .bind(content)
            .bind(tab_id)
            .execute(&pool)
            .await?;
        }

        let mut profile = test_profile();
        profile.sync_tabs.mode = TabSyncMode::Selected;
        profile.sync_tabs.selected_tab_ids = vec![excluded_tab_id];

        let items = repository.list_snapshot_items(&profile, "device_a").await?;
        let contents: std::collections::HashSet<&str> = items
            .iter()
            .filter_map(|item| item.content.as_deref())
            .collect();

        assert!(contents.contains("default item"));
        assert!(contents.contains("included later item"));
        assert!(!contents.contains("excluded item"));

        let order = repository.build_snapshot_order(&profile).await?;
        let tab_keys: std::collections::HashSet<&str> =
            order.tabs.iter().map(|tab| tab.tab_key.as_str()).collect();
        assert!(tab_keys.contains("default"));
        assert!(tab_keys.contains("tab-name:included later"));
        assert!(!tab_keys.contains("tab-name:excluded"));
        Ok(())
    }

    #[tokio::test]
    async fn snapshot_round_trip_syncs_named_and_empty_tabs_across_different_local_ids(
    ) -> Result<(), SyncError> {
        let source_pool = setup_sync_test_db().await;
        let source = SyncRepository::new(source_pool.clone());
        let profile = test_profile();

        let research_id = sqlx::query("INSERT INTO tabs (name) VALUES ('Research')")
            .execute(&source_pool)
            .await?
            .last_insert_rowid();
        sqlx::query("INSERT INTO tabs (name) VALUES ('Empty Workspace')")
            .execute(&source_pool)
            .await?;

        for (content, tab_id) in [("default item", 1), ("research item", research_id)] {
            sqlx::query(
                r#"
                INSERT INTO clipboard_items
                    (type, content, tab_id, is_sensitive, is_pinned, display_order, created_at, updated_at)
                VALUES ('text', ?, ?, 0, 0, 0, datetime('now'), datetime('now'))
                "#,
            )
            .bind(content)
            .bind(tab_id)
            .execute(&source_pool)
            .await?;
        }

        let items = source.list_snapshot_items(&profile, "device_a").await?;
        let order = source.build_snapshot_order(&profile).await?;
        assert!(order
            .tabs
            .iter()
            .any(|tab| tab.tab_name.as_deref() == Some("Empty Workspace")));

        let target_pool = setup_sync_test_db().await;
        let target = SyncRepository::new(target_pool.clone());
        let colliding_id = sqlx::query("INSERT INTO tabs (name) VALUES ('Unrelated')")
            .execute(&target_pool)
            .await?
            .last_insert_rowid();
        assert_eq!(colliding_id, research_id);

        target.apply_snapshot_items(&profile, items, false).await?;
        target.apply_snapshot_order(&profile, &order, false).await?;

        let tabs: Vec<(i64, String)> = sqlx::query_as("SELECT id, name FROM tabs ORDER BY id")
            .fetch_all(&target_pool)
            .await?;
        assert!(tabs.iter().any(|(_, name)| name == "Research"));
        assert!(tabs.iter().any(|(_, name)| name == "Empty Workspace"));

        let research_item_tab: (String,) = sqlx::query_as(
            r#"
            SELECT t.name
            FROM clipboard_items ci
            JOIN tabs t ON t.id = ci.tab_id
            WHERE ci.content = 'research item'
            "#,
        )
        .fetch_one(&target_pool)
        .await?;
        assert_eq!(research_item_tab.0, "Research");
        Ok(())
    }

    #[tokio::test]
    async fn legacy_order_without_tab_name_still_creates_empty_tab() -> Result<(), SyncError> {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let profile = test_profile();
        let order: SnapshotOrder = serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "updated_at": "2026-05-15T00:00:00Z",
            "tabs": [{
                "tab_key": "tab-name:legacy workspace",
                "pinned": [],
                "normal": []
            }]
        }))?;

        repository
            .apply_snapshot_order(&profile, &order, false)
            .await?;

        let exists: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM tabs WHERE name = 'legacy workspace'")
                .fetch_one(&pool)
                .await?;
        assert_eq!(exists.0, 1);
        Ok(())
    }

    #[tokio::test]
    async fn selected_scope_pruning_preserves_items_in_excluded_tabs() -> Result<(), SyncError> {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let excluded_tab_id = sqlx::query("INSERT INTO tabs (name) VALUES ('Local Only')")
            .execute(&pool)
            .await?
            .last_insert_rowid();

        let included_id = sqlx::query(
            r#"
            INSERT INTO clipboard_items
                (type, content, tab_id, is_sensitive, is_pinned, created_at, updated_at)
            VALUES ('text', 'stale included', 1, 0, 0, datetime('now'), datetime('now'))
            "#,
        )
        .execute(&pool)
        .await?
        .last_insert_rowid();
        let excluded_id = sqlx::query(
            r#"
            INSERT INTO clipboard_items
                (type, content, tab_id, is_sensitive, is_pinned, created_at, updated_at)
            VALUES ('text', 'local only', ?, 0, 0, datetime('now'), datetime('now'))
            "#,
        )
        .bind(excluded_tab_id)
        .execute(&pool)
        .await?
        .last_insert_rowid();

        for (local_id, item_key) in [(included_id, "included-key"), (excluded_id, "excluded-key")] {
            sqlx::query(
                r#"
                INSERT INTO sync_item_map
                    (local_id, item_key, stable_seq, remote_path, last_synced_at)
                VALUES (?, ?, ?, ?, datetime('now'))
                "#,
            )
            .bind(local_id)
            .bind(item_key)
            .bind(local_id)
            .bind(format!("items/{item_key}.json"))
            .execute(&pool)
            .await?;
        }

        let mut profile = test_profile();
        profile.sync_tabs.mode = TabSyncMode::Selected;
        profile.sync_tabs.selected_tab_ids = vec![excluded_tab_id];

        repository
            .apply_snapshot_items(&profile, Vec::new(), true)
            .await?;

        let remaining: Vec<(String,)> =
            sqlx::query_as("SELECT content FROM clipboard_items ORDER BY id")
                .fetch_all(&pool)
                .await?;
        assert_eq!(remaining, vec![("local only".to_string(),)]);
        Ok(())
    }

    #[tokio::test]
    async fn all_scope_keeps_remote_new_tab_after_order_apply() -> Result<(), SyncError> {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let profile = test_profile();
        let mut item = remote_item("device_b_1_1", "remote tab item", "remote-tab-hash");
        item.tab_key = Some("tab-name:research".to_string());
        item.tab_name = Some("Research".to_string());

        repository
            .apply_snapshot_items(&profile, vec![item], false)
            .await?;
        repository
            .apply_snapshot_order(
                &profile,
                &SnapshotOrder {
                    schema_version: 1,
                    updated_at: "2026-05-15T00:00:00Z".to_string(),
                    tabs: vec![SnapshotTabOrder {
                        tab_key: "tab-name:research".to_string(),
                        tab_name: Some("Research".to_string()),
                        pinned: Vec::new(),
                        normal: vec!["device_b_1_1".to_string()],
                    }],
                },
                false,
            )
            .await?;

        let stored: (String,) = sqlx::query_as(
            r#"
            SELECT t.name
            FROM clipboard_items ci
            JOIN tabs t ON t.id = ci.tab_id
            WHERE ci.content = 'remote tab item'
            "#,
        )
        .fetch_one(&pool)
        .await?;

        assert_eq!(stored.0, "Research");
        Ok(())
    }

    #[tokio::test]
    async fn all_scope_prunes_empty_tabs_missing_from_remote_order() -> Result<(), SyncError> {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let profile = test_profile();
        sqlx::query("INSERT INTO tabs (name) VALUES ('Deleted Remote Tab')")
            .execute(&pool)
            .await?;

        repository
            .apply_snapshot_order(
                &profile,
                &SnapshotOrder {
                    schema_version: 1,
                    updated_at: "2026-05-15T00:00:00Z".to_string(),
                    tabs: vec![SnapshotTabOrder {
                        tab_key: "default".to_string(),
                        tab_name: None,
                        pinned: Vec::new(),
                        normal: Vec::new(),
                    }],
                },
                true,
            )
            .await?;

        let deleted_count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM tabs WHERE name = 'Deleted Remote Tab'")
                .fetch_one(&pool)
                .await?;
        assert_eq!(deleted_count.0, 0);
        Ok(())
    }

    #[tokio::test]
    async fn remote_order_does_not_override_pending_local_tab_move() -> Result<(), SyncError> {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let profile = test_profile();
        let test_tab_id = sqlx::query("INSERT INTO tabs (name) VALUES ('test')")
            .execute(&pool)
            .await?
            .last_insert_rowid();
        let item_id = sqlx::query(
            r#"
            INSERT INTO clipboard_items
                (type, content, tab_id, is_sensitive, is_pinned, display_order, created_at, updated_at)
            VALUES ('text', 'moved local item', ?, 0, 1, -1000000, datetime('now'), datetime('now'))
            "#,
        )
        .bind(test_tab_id)
        .execute(&pool)
        .await?
        .last_insert_rowid();
        sqlx::query(
            r#"
            INSERT INTO sync_item_map
                (local_id, item_key, stable_seq, remote_path, last_synced_at)
            VALUES (?, 'device_a_1_1', 1, 'items/device_a_1_1.json', datetime('now'))
            "#,
        )
        .bind(item_id)
        .execute(&pool)
        .await?;
        sqlx::query(
            r#"
            INSERT INTO sync_changes
                (entity_type, entity_id, operation, tab_id, source, changed_at)
            VALUES ('clipboard_item', ?, 'tab_change', ?, 'local', datetime('now'))
            "#,
        )
        .bind(item_id.to_string())
        .bind(test_tab_id)
        .execute(&pool)
        .await?;

        repository
            .apply_snapshot_order(
                &profile,
                &SnapshotOrder {
                    schema_version: 1,
                    updated_at: "2026-05-15T00:00:00Z".to_string(),
                    tabs: vec![SnapshotTabOrder {
                        tab_key: "default".to_string(),
                        tab_name: None,
                        pinned: Vec::new(),
                        normal: vec!["device_a_1_1".to_string()],
                    }],
                },
                false,
            )
            .await?;

        let stored: (Option<i64>, i32) =
            sqlx::query_as("SELECT tab_id, is_pinned FROM clipboard_items WHERE id = ?")
                .bind(item_id)
                .fetch_one(&pool)
                .await?;
        assert_eq!(stored.0, Some(test_tab_id));
        assert_eq!(stored.1, 1);
        Ok(())
    }

    #[tokio::test]
    async fn snapshot_items_keep_stable_seq_when_display_order_changes() -> Result<(), SyncError> {
        let pool = setup_sync_test_db().await;
        let repository = SyncRepository::new(pool.clone());
        let profile = test_profile();

        for (content, display_order) in [("first", 0), ("second", 1), ("third", 2)] {
            sqlx::query(
                r#"
                INSERT INTO clipboard_items
                    (type, content, tab_id, is_sensitive, is_pinned, display_order, created_at, updated_at)
                VALUES ('text', ?, 1, 0, 0, ?, datetime('now'), datetime('now'))
                "#,
            )
            .bind(content)
            .bind(display_order)
            .execute(&pool)
            .await?;
        }

        let before = repository.list_snapshot_items(&profile, "device_a").await?;
        let before_pairs: Vec<(String, i64)> = before
            .iter()
            .map(|item| (item.item_key.clone(), item.stable_seq))
            .collect();

        sqlx::query("UPDATE clipboard_items SET display_order = CASE content WHEN 'third' THEN 0 WHEN 'first' THEN 1 ELSE 2 END")
            .execute(&pool)
            .await?;

        let after = repository.list_snapshot_items(&profile, "device_a").await?;
        let after_pairs: Vec<(String, i64)> = after
            .iter()
            .map(|item| (item.item_key.clone(), item.stable_seq))
            .collect();
        let order = repository.build_snapshot_order(&profile).await?;

        assert_eq!(before_pairs, after_pairs);
        assert_eq!(order.tabs.len(), 1);
        assert_eq!(order.tabs[0].normal.len(), 3);
        assert_eq!(order.tabs[0].normal[0], before[2].item_key);
        Ok(())
    }
}
