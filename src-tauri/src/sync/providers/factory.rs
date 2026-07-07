use crate::sync::error::SyncError;
use crate::sync::models::{ConnectionTestResult, SyncProfile, SyncProviderKind};
use crate::sync::providers::google_drive::GoogleDriveProvider;
use crate::sync::providers::onedrive::OneDriveProvider;
use crate::sync::providers::sftp::{SftpAuth, SftpProvider};
use crate::sync::providers::webdav::WebDavProvider;
use crate::sync::providers::SyncProvider;
use crate::sync::secrets::SecretStore;
use std::sync::Arc;

pub struct ProviderFactory {
    secret_store: Arc<SecretStore>,
}

impl ProviderFactory {
    pub fn new(secret_store: Arc<SecretStore>) -> Self {
        Self { secret_store }
    }

    pub async fn build(&self, profile: &SyncProfile) -> Result<Arc<dyn SyncProvider>, SyncError> {
        match profile.provider {
            SyncProviderKind::WebDav => self.build_webdav(profile).await,
            SyncProviderKind::Sftp => self.build_sftp(profile).await,
            SyncProviderKind::GoogleDrive => self.build_google_drive(profile).await,
            SyncProviderKind::OneDrive => self.build_onedrive(profile).await,
        }
    }

    pub async fn test_connection(
        &self,
        profile: &SyncProfile,
    ) -> Result<ConnectionTestResult, SyncError> {
        let provider = self.build(profile).await?;
        match provider.test_connection().await {
            Ok(()) => Ok(ConnectionTestResult {
                success: true,
                message: "Connection successful".to_string(),
                server_info: Some(format!(
                    "{}: {}",
                    match profile.provider {
                        SyncProviderKind::WebDav => "WebDAV",
                        SyncProviderKind::Sftp => "SFTP",
                        SyncProviderKind::GoogleDrive => "Google Drive",
                        SyncProviderKind::OneDrive => "OneDrive",
                    },
                    profile.remote_root
                )),
            }),
            Err(e) => Ok(ConnectionTestResult {
                success: false,
                message: format!("Connection failed: {}", e),
                server_info: None,
            }),
        }
    }

    pub async fn trust_sftp_host_key(
        &self,
        profile: &SyncProfile,
    ) -> Result<crate::sync::models::SftpHostKeyTrustResult, SyncError> {
        if profile.provider != SyncProviderKind::Sftp {
            return Err(SyncError::Validation(
                "Host key trust is only available for SFTP profiles".to_string(),
            ));
        }

        let (host, port, _) = parse_sftp_remote_root(&profile.remote_root)?;
        tokio::task::spawn_blocking(move || SftpProvider::trust_host_key(&host, port))
            .await
            .map_err(|e| SyncError::provider(format!("SFTP host key task panicked: {}", e)))?
    }

    async fn build_webdav(
        &self,
        profile: &SyncProfile,
    ) -> Result<Arc<dyn SyncProvider>, SyncError> {
        let password_ref =
            profile.credential_refs.password.as_ref().ok_or_else(|| {
                SyncError::Validation("WebDAV password not configured".to_string())
            })?;
        let username_ref = profile
            .credential_refs
            .username
            .as_ref()
            .or(profile.credential_refs.passphrase.as_ref())
            .ok_or_else(|| SyncError::Validation("WebDAV username not configured".to_string()))?;
        let username = self.get_secret_string(username_ref, "Username").await?;
        let password = self.get_secret_string(password_ref, "Password").await?;
        let provider = WebDavProvider::new(&profile.remote_root, &username, &password).await?;
        Ok(Arc::new(provider))
    }

    async fn build_sftp(&self, profile: &SyncProfile) -> Result<Arc<dyn SyncProvider>, SyncError> {
        let (host, port, remote_path) = parse_sftp_remote_root(&profile.remote_root)?;
        let username_ref = profile
            .credential_refs
            .username
            .as_ref()
            .ok_or_else(|| SyncError::Validation("SFTP username not configured".to_string()))?;
        let username = self.get_secret_string(username_ref, "Username").await?;

        let auth = if let Some(password_ref) = &profile.credential_refs.password {
            SftpAuth::Password(self.get_secret_string(password_ref, "Password").await?)
        } else if let Some(key_ref) = &profile.credential_refs.private_key {
            let private_key = self.get_secret_string(key_ref, "Private key").await?;
            let passphrase = if let Some(passphrase_ref) = &profile.credential_refs.passphrase {
                Some(self.get_secret_string(passphrase_ref, "Passphrase").await?)
            } else {
                None
            };
            if looks_like_private_key_pem(&private_key) {
                SftpAuth::KeyPem {
                    private_key,
                    passphrase,
                }
            } else {
                SftpAuth::KeyFile {
                    key_path: private_key,
                    passphrase,
                }
            }
        } else {
            return Err(SyncError::Validation(
                "No SFTP credentials configured".to_string(),
            ));
        };

        Ok(Arc::new(SftpProvider::new(
            &host,
            port,
            &username,
            &remote_path,
            auth,
        )))
    }

    async fn build_google_drive(
        &self,
        profile: &SyncProfile,
    ) -> Result<Arc<dyn SyncProvider>, SyncError> {
        let token = self.get_access_token(profile, "Google Drive").await?;
        Ok(Arc::new(GoogleDriveProvider::new(
            &profile.remote_root,
            &token,
        )?))
    }

    async fn build_onedrive(
        &self,
        profile: &SyncProfile,
    ) -> Result<Arc<dyn SyncProvider>, SyncError> {
        let token = self.get_access_token(profile, "OneDrive").await?;
        Ok(Arc::new(OneDriveProvider::new(
            &profile.remote_root,
            &token,
        )?))
    }

    async fn get_access_token(
        &self,
        profile: &SyncProfile,
        provider_label: &str,
    ) -> Result<String, SyncError> {
        let token_ref = profile.credential_refs.password.as_ref().ok_or_else(|| {
            SyncError::Validation(format!("{} access token not configured", provider_label))
        })?;
        self.get_secret_string(token_ref, "Access token").await
    }

    async fn get_secret_string(&self, secret_ref: &str, label: &str) -> Result<String, SyncError> {
        let bytes = self.secret_store.get(secret_ref).await?.ok_or_else(|| {
            SyncError::SecretStore(format!("{} not found in secret store", label))
        })?;

        String::from_utf8(bytes)
            .map_err(|_| SyncError::SecretStore(format!("{} is not valid UTF-8", label)))
    }
}

fn looks_like_private_key_pem(value: &str) -> bool {
    let trimmed = value.trim();
    trimmed.contains("-----BEGIN ") && trimmed.contains("PRIVATE KEY-----")
        || trimmed.lines().count() > 1
}

/// Parse SFTP host and port from remote_root.
/// Supported formats: sftp://host:port/path, host:port, or host.
pub fn parse_sftp_host(remote_root: &str) -> Result<(String, u16), SyncError> {
    let (host, port, _) = parse_sftp_remote_root(remote_root)?;
    Ok((host, port))
}

/// Parse SFTP host, port, and remote path from remote_root.
/// Supported formats: sftp://host:port/path, host:port/path, host:port, or host.
pub fn parse_sftp_remote_root(remote_root: &str) -> Result<(String, u16, String), SyncError> {
    let root = remote_root.trim_start_matches("sftp://");
    let (host_port, path) = if let Some(slash_pos) = root.find('/') {
        (&root[..slash_pos], &root[slash_pos..])
    } else {
        (root, "/cliporax/v1")
    };

    if host_port.trim().is_empty() {
        return Err(SyncError::Validation("SFTP host is required".to_string()));
    }

    if let Some(colon_pos) = host_port.rfind(':') {
        let host = &host_port[..colon_pos];
        let port_str = &host_port[colon_pos + 1..];
        let port = port_str.parse::<u16>().map_err(|_| {
            SyncError::Validation(format!("Invalid SFTP port number: {}", port_str))
        })?;
        Ok((host.to_string(), port, path.to_string()))
    } else {
        Ok((host_port.to_string(), 22, path.to_string()))
    }
}
