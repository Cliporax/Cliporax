use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

mod bool_int {
    use super::*;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum BoolOrInt {
        Bool(bool),
        Int(i32),
    }

    pub fn serialize<S>(value: &Option<i32>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bool(value.unwrap_or(0) != 0)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<i32>, D::Error>
    where
        D: Deserializer<'de>,
    {
        Option::<BoolOrInt>::deserialize(deserializer).map(|value| {
            value.map(|inner| match inner {
                BoolOrInt::Bool(flag) => i32::from(flag),
                BoolOrInt::Int(number) => i32::from(number != 0),
            })
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ClipboardItem {
    pub id: Option<i64>,
    #[serde(rename = "type")]
    #[sqlx(rename = "type")]
    pub item_type: String,
    pub content: String,
    pub content_hash: Option<String>,
    pub metadata: Option<String>,
    pub tags: Option<String>,
    pub tab_id: Option<i64>,
    #[serde(serialize_with = "bool_int::serialize")]
    pub is_sensitive: Option<i32>,
    #[serde(serialize_with = "bool_int::serialize")]
    pub is_pinned: Option<i32>,
    pub display_order: Option<i32>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Tab {
    pub id: Option<i64>,
    pub name: String,
    #[serde(serialize_with = "bool_int::serialize")]
    pub is_default: Option<i32>,
    #[serde(serialize_with = "bool_int::serialize")]
    pub auto_capture: Option<i32>,
    pub created_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipboardItemInput {
    #[serde(rename = "type")]
    pub item_type: String,
    pub content: String,
    pub content_hash: Option<String>,
    pub metadata: Option<String>,
    pub tags: Option<String>,
    pub tab_id: Option<i64>,
    #[serde(default, deserialize_with = "bool_int::deserialize")]
    pub is_sensitive: Option<i32>,
    #[serde(default, deserialize_with = "bool_int::deserialize")]
    pub is_pinned: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub source: String,
    pub source_app: String,
    pub window_title: String,
    pub source_host: String,
    pub timestamp: String,
}
