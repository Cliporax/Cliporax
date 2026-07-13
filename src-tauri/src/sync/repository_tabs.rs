use crate::sync::error::SyncError;

async fn insert_synced_tab(name: &str, pool: &sqlx::SqlitePool) -> Result<i64, SyncError> {
    let mut tx = pool.begin().await?;
    let max_order: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(display_order), -1) FROM tabs")
        .fetch_one(&mut *tx)
        .await?;
    let trash_order: Option<i64> =
        sqlx::query_scalar("SELECT display_order FROM tabs WHERE is_trash = 1 LIMIT 1")
            .fetch_optional(&mut *tx)
            .await?;
    let insert_order = if trash_order == Some(max_order) {
        sqlx::query("UPDATE tabs SET display_order = ? WHERE is_trash = 1")
            .bind(max_order + 1)
            .execute(&mut *tx)
            .await?;
        max_order
    } else {
        max_order + 1
    };
    let id = sqlx::query("INSERT INTO tabs (name, display_order) VALUES (?, ?)")
        .bind(name)
        .bind(insert_order)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
    tx.commit().await?;
    Ok(id)
}

pub(super) fn tab_key_for_local_id(tab_id: Option<i64>) -> String {
    tab_id
        .map(|id| format!("tab:{}", id))
        .unwrap_or_else(|| "default".to_string())
}

pub(super) fn tab_key_for_snapshot(
    tab_id: Option<i64>,
    tab_name: Option<&str>,
    is_default: bool,
) -> String {
    if is_default || tab_id.is_none() {
        return "default".to_string();
    }
    tab_name
        .map(|name| format!("tab-name:{}", name.trim().to_lowercase()))
        .filter(|key| key != "tab-name:")
        .unwrap_or_else(|| tab_key_for_local_id(tab_id))
}

pub(super) async fn local_id_for_remote_tab(
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
        return Ok(Some(insert_synced_tab(&name, pool).await?));
    }

    default_tab_id(pool).await
}

pub(super) async fn trash_tab_id(pool: &sqlx::SqlitePool) -> Result<i64, SyncError> {
    if let Some((id,)) =
        sqlx::query_as::<_, (i64,)>("SELECT id FROM tabs WHERE is_trash = 1 LIMIT 1")
            .fetch_optional(pool)
            .await?
    {
        return Ok(id);
    }
    Ok(sqlx::query(
        "INSERT INTO tabs (name, is_default, auto_capture, is_trash, display_order) \
         VALUES ('Trash', 0, 0, 1, (SELECT COALESCE(MAX(display_order), -1) + 1 FROM tabs))",
    )
    .execute(pool)
    .await?
    .last_insert_rowid())
}

pub(super) async fn local_id_for_tab_key(
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

pub(super) async fn local_id_for_order_tab(
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
        return Ok(Some(insert_synced_tab(&name, pool).await?));
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

pub(super) fn normalize_remote_tab_name(name: Option<&str>) -> Option<String> {
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

pub(super) fn parse_sync_tags(tags: Option<&str>) -> Vec<String> {
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
