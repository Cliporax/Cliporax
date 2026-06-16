use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Pool, Sqlite,
};

pub type Db = Pool<Sqlite>;

pub async fn init_database(app_handle: &tauri::AppHandle) -> Result<Db, sqlx::Error> {
    let app_data = crate::portable::app_data_dir(app_handle).map_err(|e| {
        sqlx::Error::Io(std::io::Error::other(format!(
            "Failed to get app data directory: {}",
            e
        )))
    })?;
    let db_path = app_data.join("cliporax.db");

    log::info!("[Database] Initializing database at: {:?}", db_path);

    // Ensure directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            sqlx::Error::Io(std::io::Error::new(
                e.kind(),
                format!("Failed to create database directory {:?}: {}", parent, e),
            ))
        })?;
        log::debug!("[Database] Created directory: {:?}", parent);
    }

    // Create connection pool
    log::debug!("[Database] Creating connection pool...");
    let connect_options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(connect_options)
        .await?;
    log::info!("[Database] Connection pool created");

    // Enable foreign key constraints (required for ON DELETE CASCADE)
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await?;
    log::info!("[Database] Foreign key constraints enabled");

    // Run migrations
    log::info!("[Database] Running migrations...");
    run_migrations(&pool).await?;
    log::info!("[Database] Migrations completed");

    Ok(pool)
}

async fn run_migrations(pool: &Db) -> Result<(), sqlx::Error> {
    log::debug!("[Database] Creating tabs table...");
    // Create tabs table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS tabs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL UNIQUE,
            is_default INTEGER DEFAULT 0,
            auto_capture INTEGER DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;
    log::debug!("[Database] tabs table created");

    log::debug!("[Database] Creating clipboard_items table...");
    // Create clipboard_items table
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS clipboard_items (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            type TEXT NOT NULL,
            content TEXT,
            content_hash TEXT,
            metadata TEXT,
            tags TEXT,
            tab_id INTEGER,
            is_sensitive INTEGER DEFAULT 0,
            is_pinned INTEGER DEFAULT 0,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (tab_id) REFERENCES tabs(id) ON DELETE CASCADE
        )
        "#,
    )
    .execute(pool)
    .await?;
    log::debug!("[Database] clipboard_items table created");

    // Add content_hash column if it doesn't exist (migration for existing databases)
    let add_hash_result = sqlx::query(
        r#"
        ALTER TABLE clipboard_items ADD COLUMN content_hash TEXT
        "#,
    )
    .execute(pool)
    .await;
    match add_hash_result {
        Ok(_) => log::info!("[Database] Added content_hash column"),
        Err(e) => log::debug!(
            "[Database] content_hash column likely already exists: {}",
            e
        ),
    }

    // Add display_order column if it doesn't exist (migration for drag reorder)
    let add_order_result = sqlx::query(
        r#"
        ALTER TABLE clipboard_items ADD COLUMN display_order INTEGER DEFAULT 0
        "#,
    )
    .execute(pool)
    .await;
    match add_order_result {
        Ok(_) => log::info!("[Database] Added display_order column"),
        Err(e) => log::debug!(
            "[Database] display_order column likely already exists: {}",
            e
        ),
    }

    // Add auto_capture column if it doesn't exist (migration for mixed tab model)
    let add_auto_capture_result = sqlx::query(
        r#"
        ALTER TABLE tabs ADD COLUMN auto_capture INTEGER DEFAULT 0
        "#,
    )
    .execute(pool)
    .await;
    match add_auto_capture_result {
        Ok(_) => {
            log::info!("[Database] Added auto_capture column");
            // Set default tab to auto_capture=1
            sqlx::query(
                r#"
                UPDATE tabs SET auto_capture = 1, name = 'Clipboard' WHERE is_default = 1 AND (auto_capture = 0 OR name = 'Default')
                "#,
            )
            .execute(pool)
            .await?;
            log::info!("[Database] Set default tab to auto_capture=1 with name 'Clipboard'");
        }
        Err(e) => log::debug!(
            "[Database] auto_capture column likely already exists: {}",
            e
        ),
    }

    // Cleanup: Ensure ONLY the default tab has auto_capture=1
    // This prevents multiple tabs from capturing clipboard content
    sqlx::query(
        r#"
        UPDATE tabs SET auto_capture = 0 
        WHERE is_default = 0 AND auto_capture = 1
        "#,
    )
    .execute(pool)
    .await?;
    log::info!("[Database] Ensured only default tab has auto_capture=1");

    // Create index on content_hash for faster image dedup
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_clipboard_items_content_hash 
        ON clipboard_items(content_hash)
        "#,
    )
    .execute(pool)
    .await?;

    log::debug!("[Database] Creating indexes...");
    // Create index for better query performance
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_clipboard_items_tab_id 
        ON clipboard_items(tab_id)
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_clipboard_items_updated_at 
        ON clipboard_items(updated_at DESC)
        "#,
    )
    .execute(pool)
    .await?;

    // Create composite index for display_order (for faster reordering queries)
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_clipboard_items_reorder 
        ON clipboard_items(tab_id, is_pinned DESC, display_order ASC)
        "#,
    )
    .execute(pool)
    .await?;

    // Create covering index for pagination query (matches ORDER BY clause exactly)
    // This is critical for fast OFFSET queries
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_clipboard_items_pagination 
        ON clipboard_items(tab_id, is_pinned DESC, display_order ASC, updated_at DESC)
        "#,
    )
    .execute(pool)
    .await?;

    log::debug!("[Database] Indexes created");

    // Create sync tables (Phase 0: Cloud Sync foundations)
    log::info!("[Database] Creating sync tables...");
    create_sync_tables(pool).await?;
    log::info!("[Database] Sync tables created");

    log::debug!("[Database] Inserting default tab if not exists...");
    // Insert default tab if not exists
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO tabs (name, is_default, auto_capture) 
        VALUES ('System Clipboard', 1, 1)
        "#,
    )
    .execute(pool)
    .await?;

    // Clean up: ensure only one default tab exists (fix legacy duplicate default tabs)
    // Keep 'System Clipboard' as the canonical default, reset is_default on others
    sqlx::query(
        r#"
        UPDATE tabs SET is_default = 0 
        WHERE is_default = 1 AND name != 'System Clipboard'
        "#,
    )
    .execute(pool)
    .await?;
    log::info!("[Database] Default tab ensured (duplicates cleaned)");

    // Log current state
    let tabs: Result<Vec<(i64, String, i32)>, _> =
        sqlx::query_as("SELECT id, name, is_default FROM tabs")
            .fetch_all(pool)
            .await;
    match tabs {
        Ok(tabs) => {
            log::info!("[Database] Current tabs: {:?}", tabs);
        }
        Err(e) => {
            log::error!("[Database] Failed to query tabs: {}", e);
        }
    }

    // Count clipboard items
    let count: Result<(i64,), _> = sqlx::query_as("SELECT COUNT(*) FROM clipboard_items")
        .fetch_one(pool)
        .await;
    match count {
        Ok((count,)) => {
            log::info!("[Database] Current clipboard items count: {}", count);
        }
        Err(e) => {
            log::error!("[Database] Failed to count clipboard items: {}", e);
        }
    }

    Ok(())
}

/// Create sync-related tables for Cloud Sync feature
async fn create_sync_tables(pool: &Db) -> Result<(), sqlx::Error> {
    // sync_device table - stores local device ID
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sync_device (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            device_id TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;
    log::info!("[Database] sync_device table created");

    // sync_profiles table - stores sync profile configurations
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sync_profiles (
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
    .execute(pool)
    .await?;
    log::info!("[Database] sync_profiles table created");

    // sync_secrets table - encrypted provider credentials.
    //
    // Values are encrypted before insertion by Sync::SecretStore. Keeping this
    // in SQLite makes saved sync profiles usable after app restart without
    // exposing credentials to plugin JavaScript.
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sync_secrets (
            ref_id TEXT PRIMARY KEY,
            profile_id TEXT NOT NULL,
            secret_key TEXT NOT NULL,
            value_ciphertext_b64 TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;
    log::info!("[Database] sync_secrets table created");

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_sync_secrets_profile
        ON sync_secrets(profile_id)
        "#,
    )
    .execute(pool)
    .await?;

    // sync_item_map table - maps local item IDs to remote item keys
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sync_item_map (
            local_id INTEGER NOT NULL,
            item_key TEXT NOT NULL UNIQUE,
            remote_path TEXT NOT NULL,
            last_remote_updated_at DATETIME,
            last_synced_at DATETIME,
            PRIMARY KEY (local_id)
        )
        "#,
    )
    .execute(pool)
    .await?;
    log::info!("[Database] sync_item_map table created");

    // sync_state table - stores sync cursors per scope
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sync_state (
            scope TEXT NOT NULL,
            scope_id TEXT NOT NULL,
            provider TEXT NOT NULL,
            cursor TEXT,
            last_sync_at DATETIME,
            PRIMARY KEY (scope, scope_id, provider)
        )
        "#,
    )
    .execute(pool)
    .await?;
    log::info!("[Database] sync_state table created");

    // sync_changes table - local outbox for unsynced changes
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sync_changes (
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
    .execute(pool)
    .await?;
    log::info!("[Database] sync_changes table created");

    // sync_remote_cursors table - tracks remote device cursors
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sync_remote_cursors (
            profile_id TEXT NOT NULL,
            remote_device_id TEXT NOT NULL,
            last_seq INTEGER NOT NULL DEFAULT 0,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (profile_id, remote_device_id)
        )
        "#,
    )
    .execute(pool)
    .await?;
    log::info!("[Database] sync_remote_cursors table created");

    // sync_conflicts table - stores sync conflicts
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sync_conflicts (
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
    .execute(pool)
    .await?;
    log::info!("[Database] sync_conflicts table created");

    // sync_logs table - stores user-visible sync status and failure history
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sync_logs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            profile_id TEXT,
            run_id TEXT,
            level TEXT NOT NULL,
            message TEXT NOT NULL,
            created_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;
    log::info!("[Database] sync_logs table created");

    // sync_run_reports table - persists the most recent run report per profile
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS sync_run_reports (
            profile_id TEXT PRIMARY KEY,
            run_id TEXT NOT NULL,
            report_json TEXT NOT NULL,
            status TEXT NOT NULL,
            started_at DATETIME NOT NULL,
            completed_at DATETIME,
            updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
        )
        "#,
    )
    .execute(pool)
    .await?;
    log::info!("[Database] sync_run_reports table created");

    // Create sync-related indexes
    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_sync_changes_unsynced
        ON sync_changes(synced_at, changed_at)
        WHERE synced_at IS NULL
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_sync_changes_item_key
        ON sync_changes(item_key)
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_sync_remote_cursors_profile
        ON sync_remote_cursors(profile_id, remote_device_id)
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_sync_conflicts_pending
        ON sync_conflicts(status, created_at)
        WHERE status = 'pending'
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_sync_logs_profile_created
        ON sync_logs(profile_id, created_at DESC)
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE INDEX IF NOT EXISTS idx_sync_run_reports_updated
        ON sync_run_reports(updated_at DESC)
        "#,
    )
    .execute(pool)
    .await?;

    log::info!("[Database] sync indexes created");
    Ok(())
}
