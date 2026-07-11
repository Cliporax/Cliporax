/// Codec - encode/decode clipboard items and changes for remote storage
use crate::db::models::ClipboardItem;
use crate::sync::crypto::{decrypt, encrypt};
use crate::sync::error::SyncError;
use crate::sync::models::*;
use serde::{Deserialize, Serialize};

/// Current schema version for remote items
const CURRENT_SCHEMA_VERSION: u32 = 1;

/// Encode a clipboard item for remote storage
///
/// Converts a local `ClipboardItem` into a `RemoteClipboardItem` JSON format,
/// optionally encrypting the payload if the sync profile has encryption enabled.
/// Returns an `EncodedRemoteItem` containing the JSON data and optional blob data.
pub fn encode_clipboard_item(
    local: ClipboardItem,
    item_key: String,
    profile: &SyncProfile,
    device_id: String,
    password: Option<&str>,
) -> Result<EncodedRemoteItem, SyncError> {
    // Build the remote item representation
    let remote_item = RemoteClipboardItem {
        schema_version: CURRENT_SCHEMA_VERSION,
        item_key: item_key.clone(),
        stable_seq: 0,
        device_id: device_id.clone(),
        local_id: local.id,
        item_type: local.item_type.clone(),
        content: Some(local.content.clone()),
        blob_path: None,
        blob_mime: None,
        content_hash: local.content_hash.clone(),
        created_at: local
            .created_at
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        updated_at: local
            .updated_at
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
        tab_key: local.tab_id.map(|id| format!("tab:{}", id)),
        tab_name: None,
        tags: local
            .tags
            .as_ref()
            .map(|t| parse_tags(t))
            .unwrap_or_default(),
        is_pinned: local.is_pinned.unwrap_or(0) != 0,
        is_sensitive: local.is_sensitive.unwrap_or(0) != 0,
        metadata: local
            .metadata
            .as_ref()
            .and_then(|m| serde_json::from_str(m).ok())
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new())),
        revision: 0,
        last_modified_by: device_id,
        deleted: false,
        is_trashed: false,
        deleted_at: None,
        deleted_from_tab_key: None,
        deleted_from_tab_name: None,
    };

    // Serialize to JSON
    let json_bytes = serde_json::to_vec(&remote_item).map_err(SyncError::Serialization)?;

    // Apply encryption if enabled
    let (final_json_data, blob_data) = if profile.encryption.enabled {
        // Encrypt the JSON payload
        let password = password.ok_or_else(|| {
            SyncError::Encryption("Encryption enabled but no password provided".to_string())
        })?;

        let key = derive_key_from_password(password)?;
        let encrypted = encrypt(&json_bytes, &key)?;

        // Wrap encrypted data in an envelope with metadata
        let envelope = EncryptionEnvelope {
            schema_version: CURRENT_SCHEMA_VERSION,
            encrypted_data: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &encrypted,
            ),
            algorithm: profile.encryption.algorithm.clone(),
        };
        let envelope_bytes = serde_json::to_vec(&envelope).map_err(SyncError::Serialization)?;

        (envelope_bytes, None)
    } else {
        (json_bytes, None)
    };

    // Build paths
    let item_path = format!("items/{}.json", item_key);

    Ok(EncodedRemoteItem {
        json_data: final_json_data,
        blob_data,
        item_path,
        blob_path: None,
    })
}

/// Decode a clipboard item from remote storage
///
/// Validates the schema version, decrypts if needed, and returns a
/// `RemoteClipboardItem`. Rejects malformed or version-mismatched payloads.
pub fn decode_clipboard_item(
    bytes: &[u8],
    profile: &SyncProfile,
    password: Option<&str>,
) -> Result<RemoteClipboardItem, SyncError> {
    let json_bytes = if profile.encryption.enabled {
        // Parse encryption envelope
        let envelope: EncryptionEnvelope = serde_json::from_slice(bytes)
            .map_err(|e| SyncError::Validation(format!("Invalid encryption envelope: {}", e)))?;

        // Validate envelope schema version
        if envelope.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(SyncError::SchemaVersionMismatch {
                local: CURRENT_SCHEMA_VERSION,
                remote: envelope.schema_version,
            });
        }

        // Decrypt the payload
        let encrypted_data = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &envelope.encrypted_data,
        )
        .map_err(|e| SyncError::Encryption(format!("Base64 decode failed: {}", e)))?;

        let password = password.ok_or_else(|| {
            SyncError::Encryption("Encryption enabled but no password provided".to_string())
        })?;

        let key = derive_key_from_password(password)?;
        decrypt(&encrypted_data, &key)?
    } else {
        bytes.to_vec()
    };

    // Deserialize the remote item
    let remote_item: RemoteClipboardItem = serde_json::from_slice(&json_bytes)
        .map_err(|e| SyncError::Validation(format!("Invalid RemoteClipboardItem JSON: {}", e)))?;

    // Validate schema version
    if remote_item.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(SyncError::SchemaVersionMismatch {
            local: CURRENT_SCHEMA_VERSION,
            remote: remote_item.schema_version,
        });
    }

    Ok(remote_item)
}

/// Encode a remote change to bytes
pub fn encode_change(change: RemoteChange) -> Result<Vec<u8>, SyncError> {
    serde_json::to_vec(&change).map_err(SyncError::Serialization)
}

/// Decode a remote change from bytes
pub fn decode_change(bytes: &[u8]) -> Result<RemoteChange, SyncError> {
    serde_json::from_slice(bytes).map_err(SyncError::Serialization)
}

/// Encode a tombstone to bytes
///
/// Serializes a `RemoteTombstone` to JSON bytes for storage or transmission.
pub fn encode_tombstone(tombstone: RemoteTombstone) -> Result<Vec<u8>, SyncError> {
    // Validate schema version before encoding
    if tombstone.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(SyncError::Validation(format!(
            "Invalid tombstone schema version: expected {}, got {}",
            CURRENT_SCHEMA_VERSION, tombstone.schema_version
        )));
    }

    serde_json::to_vec(&tombstone).map_err(SyncError::Serialization)
}

/// Decode a tombstone from bytes
///
/// Deserializes JSON bytes into a `RemoteTombstone`, validating the schema version.
pub fn decode_tombstone(bytes: &[u8]) -> Result<RemoteTombstone, SyncError> {
    let tombstone: RemoteTombstone = serde_json::from_slice(bytes)
        .map_err(|e| SyncError::Validation(format!("Invalid tombstone JSON: {}", e)))?;

    // Validate schema version
    if tombstone.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(SyncError::SchemaVersionMismatch {
            local: CURRENT_SCHEMA_VERSION,
            remote: tombstone.schema_version,
        });
    }

    Ok(tombstone)
}

// ─── Helper Types ───────────────────────────────────────────────────────────

/// Encryption envelope for encrypted sync payloads
#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncryptionEnvelope {
    schema_version: u32,
    encrypted_data: String,
    algorithm: String,
}

// ─── Helper Functions ───────────────────────────────────────────────────────

/// Parse a comma-separated tag string into a Vec<String>
fn parse_tags(tags: &str) -> Vec<String> {
    if tags.is_empty() {
        return Vec::new();
    }
    tags.split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

/// Derive a symmetric key from a password string for encryption/decryption.
///
/// Uses the profile's stored crypto context defaults when not explicitly
/// configured.
fn derive_key_from_password(password: &str) -> Result<secrecy::SecretVec<u8>, SyncError> {
    // Use default crypto context parameters
    let context = crate::sync::crypto::default_crypto_context();
    crate::sync::crypto::derive_key(password, &context)
}

/// Encode a RemoteClipboardItem for remote storage (used during staging)
///
/// Similar to `encode_clipboard_item` but takes a `RemoteClipboardItem` directly
/// instead of a local `ClipboardItem`. This is used when staging local changes
/// that have already been converted to the remote format.
pub fn encode_clipboard_item_from_remote(
    remote_item: RemoteClipboardItem,
    profile: &SyncProfile,
    password: Option<&str>,
) -> Result<EncodedRemoteItem, SyncError> {
    // Serialize to JSON
    let json_bytes = serde_json::to_vec(&remote_item).map_err(SyncError::Serialization)?;

    // Apply encryption if enabled
    let (final_json_data, blob_data) = if profile.encryption.enabled {
        // Encrypt the JSON payload
        let password = password.ok_or_else(|| {
            SyncError::Encryption("Encryption enabled but no password provided".to_string())
        })?;

        let key = derive_key_from_password(password)?;
        let encrypted = encrypt(&json_bytes, &key)?;

        // Wrap encrypted data in an envelope with metadata
        let envelope = EncryptionEnvelope {
            schema_version: CURRENT_SCHEMA_VERSION,
            encrypted_data: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &encrypted,
            ),
            algorithm: profile.encryption.algorithm.clone(),
        };
        let envelope_bytes = serde_json::to_vec(&envelope).map_err(SyncError::Serialization)?;

        (envelope_bytes, None)
    } else {
        (json_bytes, None)
    };

    // Build paths
    let item_path = format!("items/{}.json", remote_item.item_key);

    Ok(EncodedRemoteItem {
        json_data: final_json_data,
        blob_data,
        item_path,
        blob_path: None,
    })
}

/// Decode a clipboard item using a pre-derived crypto key
///
/// This variant accepts an already-derived crypto key, avoiding redundant key derivation.
pub fn decode_clipboard_item_with_key(
    bytes: &[u8],
    profile: &SyncProfile,
    crypto_key: Option<&secrecy::SecretVec<u8>>,
) -> Result<RemoteClipboardItem, SyncError> {
    let json_bytes = if profile.encryption.enabled {
        // Parse encryption envelope
        let envelope: EncryptionEnvelope = serde_json::from_slice(bytes)
            .map_err(|e| SyncError::Validation(format!("Invalid encryption envelope: {}", e)))?;

        // Validate envelope schema version
        if envelope.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(SyncError::SchemaVersionMismatch {
                local: CURRENT_SCHEMA_VERSION,
                remote: envelope.schema_version,
            });
        }

        // Decrypt the payload using provided key
        let encrypted_data = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD,
            &envelope.encrypted_data,
        )
        .map_err(|e| SyncError::Encryption(format!("Base64 decode failed: {}", e)))?;

        let key = crypto_key.ok_or_else(|| {
            SyncError::Encryption("Encryption enabled but no crypto key provided".to_string())
        })?;

        decrypt(&encrypted_data, key)?
    } else {
        bytes.to_vec()
    };

    // Deserialize the remote item
    let remote_item: RemoteClipboardItem = serde_json::from_slice(&json_bytes)
        .map_err(|e| SyncError::Validation(format!("Invalid RemoteClipboardItem JSON: {}", e)))?;

    // Validate schema version
    if remote_item.schema_version != CURRENT_SCHEMA_VERSION {
        return Err(SyncError::SchemaVersionMismatch {
            local: CURRENT_SCHEMA_VERSION,
            remote: remote_item.schema_version,
        });
    }

    Ok(remote_item)
}

/// Encode a RemoteClipboardItem using a pre-derived crypto key
///
/// This variant accepts an already-derived crypto key, avoiding redundant key derivation.
pub fn encode_clipboard_item_from_remote_with_key(
    remote_item: RemoteClipboardItem,
    profile: &SyncProfile,
    crypto_key: Option<&secrecy::SecretVec<u8>>,
) -> Result<EncodedRemoteItem, SyncError> {
    // Serialize to JSON
    let json_bytes = serde_json::to_vec(&remote_item).map_err(SyncError::Serialization)?;

    // Apply encryption if enabled
    let (final_json_data, blob_data) = if profile.encryption.enabled {
        let key = crypto_key.ok_or_else(|| {
            SyncError::Encryption("Encryption enabled but no crypto key provided".to_string())
        })?;

        let encrypted = encrypt(&json_bytes, key)?;

        // Wrap encrypted data in an envelope with metadata
        let envelope = EncryptionEnvelope {
            schema_version: CURRENT_SCHEMA_VERSION,
            encrypted_data: base64::Engine::encode(
                &base64::engine::general_purpose::STANDARD,
                &encrypted,
            ),
            algorithm: profile.encryption.algorithm.clone(),
        };
        let envelope_bytes = serde_json::to_vec(&envelope).map_err(SyncError::Serialization)?;

        (envelope_bytes, None)
    } else {
        (json_bytes, None)
    };

    // Build paths
    let item_path = format!("items/{}.json", remote_item.item_key);

    Ok(EncodedRemoteItem {
        json_data: final_json_data,
        blob_data,
        item_path,
        blob_path: None,
    })
}
