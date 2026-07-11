pub mod database;
pub mod models;
pub mod repositories;

pub use database::{init_database, Db};
pub use models::{ClipboardItem, ClipboardItemInput, Metadata, Tab};
pub use repositories::{ClipboardRepository, TabRepository};

#[cfg(test)]
mod repositories_test;

#[cfg(test)]
mod tests {
    use sqlx::SqlitePool;
    use std::env;

    #[allow(dead_code)]
    pub async fn setup_test_db() -> SqlitePool {
        // Use in-memory database for tests
        let db_url = if cfg!(test) {
            ":memory:".to_string()
        } else {
            env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite::memory:".to_string())
        };

        let pool = SqlitePool::connect(&db_url).await.unwrap();

        // Create tables
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tabs (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                is_default INTEGER DEFAULT 0,
                auto_capture INTEGER DEFAULT 0,
                is_trash INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS clipboard_items (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                type TEXT NOT NULL,
                content TEXT NOT NULL,
                content_hash TEXT,
                metadata TEXT,
                tags TEXT,
                tab_id INTEGER,
                is_sensitive INTEGER DEFAULT 0,
                is_pinned INTEGER DEFAULT 0,
                display_order INTEGER DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                deleted_at DATETIME,
                deleted_from_tab_id INTEGER,
                FOREIGN KEY (tab_id) REFERENCES tabs (id)
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sync_changes (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                entity_type TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                operation TEXT NOT NULL,
                tab_id INTEGER,
                item_key TEXT,
                source TEXT NOT NULL DEFAULT 'local',
                changed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                synced INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS sync_item_map (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                local_id INTEGER NOT NULL,
                item_key TEXT NOT NULL,
                profile_id INTEGER,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(&pool)
        .await
        .unwrap();

        // Insert default tab
        sqlx::query("INSERT INTO tabs (name, is_default, auto_capture) VALUES ('Clipboard', 1, 1)")
            .execute(&pool)
            .await
            .unwrap();

        pool
    }
}
