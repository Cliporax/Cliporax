/// Secrets - encrypted persistent credential storage
use crate::db::database::Db;
use crate::sync::crypto::{decrypt, encrypt};
use crate::sync::error::SyncError;
use crate::sync::models::SecretRef;
use base64::Engine;
use secrecy::SecretVec;
use sha2::{Digest, Sha256};

#[derive(Clone)]
pub struct SecretStore {
    pool: Db,
}

impl SecretStore {
    pub fn new(pool: Db) -> Self {
        Self { pool }
    }

    /// Store a secret encrypted at rest.
    pub async fn set(
        &self,
        profile_id: &str,
        key: &str,
        value: &[u8],
    ) -> Result<SecretRef, SyncError> {
        let ref_id = format!(
            "secret://{}/{}/{}",
            profile_id,
            key,
            uuid::Uuid::new_v4().simple()
        );
        let master_key = self.local_master_key().await?;
        let ciphertext = encrypt(value, &master_key)?;
        let ciphertext_b64 = base64::engine::general_purpose::STANDARD.encode(ciphertext);

        sqlx::query(
            r#"
            INSERT INTO sync_secrets
                (ref_id, profile_id, secret_key, value_ciphertext_b64, updated_at)
            VALUES (?, ?, ?, ?, datetime('now'))
            "#,
        )
        .bind(&ref_id)
        .bind(profile_id)
        .bind(key)
        .bind(ciphertext_b64)
        .execute(&self.pool)
        .await?;

        Ok(SecretRef {
            ref_id,
            profile_id: profile_id.to_string(),
            key: key.to_string(),
        })
    }

    /// Retrieve and decrypt a secret.
    pub async fn get(&self, ref_id: &str) -> Result<Option<Vec<u8>>, SyncError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value_ciphertext_b64 FROM sync_secrets WHERE ref_id = ?")
                .bind(ref_id)
                .fetch_optional(&self.pool)
                .await?;

        let Some((ciphertext_b64,)) = row else {
            return Ok(None);
        };

        let ciphertext = base64::engine::general_purpose::STANDARD
            .decode(ciphertext_b64)
            .map_err(|e| SyncError::SecretStore(format!("Invalid stored secret: {}", e)))?;
        let master_key = self.local_master_key().await?;
        decrypt(&ciphertext, &master_key).map(Some)
    }

    /// Delete a secret.
    pub async fn delete(&self, ref_id: &str) -> Result<(), SyncError> {
        sqlx::query("DELETE FROM sync_secrets WHERE ref_id = ?")
            .bind(ref_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn delete_profile_secrets(&self, profile_id: &str) -> Result<(), SyncError> {
        sqlx::query("DELETE FROM sync_secrets WHERE profile_id = ?")
            .bind(profile_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn local_master_key(&self) -> Result<SecretVec<u8>, SyncError> {
        let device_id = self.get_or_create_device_id().await?;
        let mut hasher = Sha256::new();
        hasher.update(b"Cliporax sync secret store v1");
        hasher.update(device_id.as_bytes());

        for key in ["USER", "USERNAME", "HOME", "USERPROFILE"] {
            if let Ok(value) = std::env::var(key) {
                hasher.update(key.as_bytes());
                hasher.update(value.as_bytes());
            }
        }

        Ok(SecretVec::new(hasher.finalize().to_vec()))
    }

    async fn get_or_create_device_id(&self) -> Result<String, SyncError> {
        let existing: Option<(String,)> =
            sqlx::query_as("SELECT device_id FROM sync_device WHERE id = 1")
                .fetch_optional(&self.pool)
                .await?;

        if let Some((device_id,)) = existing {
            return Ok(device_id);
        }

        let device_id = format!("device_{}", uuid::Uuid::new_v4().simple());
        sqlx::query("INSERT INTO sync_device (id, device_id) VALUES (1, ?)")
            .bind(&device_id)
            .execute(&self.pool)
            .await?;
        Ok(device_id)
    }
}
