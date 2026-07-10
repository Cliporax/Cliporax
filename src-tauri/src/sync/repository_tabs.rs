use crate::sync::error::SyncError;

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
        let result = sqlx::query("INSERT INTO tabs (name) VALUES (?)")
            .bind(&name)
            .execute(pool)
            .await?;
        return Ok(Some(result.last_insert_rowid()));
    }

    default_tab_id(pool).await
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
