//! SFTP Provider implementation using ssh2 crate
//!
//! Since ssh2 uses blocking I/O, all operations are executed via
//! `tokio::task::spawn_blocking` to avoid blocking the async runtime.

use crate::sync::error::SyncError;
use crate::sync::models::{RemoteObject, SftpHostKeyTrustResult};
use crate::sync::providers::{join_remote_path, SyncProvider};
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use chrono::Utc;
use sha2::{Digest, Sha256};
use ssh2::{CheckResult, FileType, KnownHostFileKind, KnownHosts, RenameFlags, Session};
#[cfg(windows)]
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
#[cfg(windows)]
use std::time::{SystemTime, UNIX_EPOCH};

const SSH_SESSION_TIMEOUT_MS: u32 = 30_000;

/// Configuration for SFTP authentication
#[derive(Clone)]
pub enum SftpAuth {
    /// Password authentication
    Password(String),
    /// Key-based authentication using a private key file path.
    KeyFile {
        key_path: String,
        passphrase: Option<String>,
    },
    /// Key-based authentication using PEM private key content.
    KeyPem {
        private_key: String,
        passphrase: Option<String>,
    },
}

/// SFTP provider that connects on-demand for each operation.
/// ssh2::Session is not `Send`, so we cannot store a live connection.
/// Instead, we store connection parameters and establish a fresh connection
/// per operation (or batch of operations) via `spawn_blocking`.
pub struct SftpProvider {
    host: String,
    port: u16,
    username: String,
    remote_root: String,
    auth: SftpAuth,
}

impl SftpProvider {
    pub fn new(host: &str, port: u16, username: &str, remote_root: &str, auth: SftpAuth) -> Self {
        Self {
            host: host.to_string(),
            port,
            username: username.to_string(),
            remote_root: remote_root.to_string(),
            auth,
        }
    }

    pub fn trust_host_key(host: &str, port: u16) -> Result<SftpHostKeyTrustResult, SyncError> {
        let sess = Self::connect_ssh(host, port)?;
        let known_hosts_path = default_known_hosts_path().ok_or_else(|| {
            SyncError::provider("Cannot locate user home directory for known_hosts".to_string())
        })?;

        if let Some(parent) = known_hosts_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut known_hosts = sess
            .known_hosts()
            .map_err(|e| SyncError::provider(format!("Failed to initialize known_hosts: {}", e)))?;
        if known_hosts_path.exists() {
            known_hosts
                .read_file(&known_hosts_path, KnownHostFileKind::OpenSSH)
                .map_err(|e| {
                    SyncError::provider(format!(
                        "Failed to read known_hosts '{}': {}",
                        known_hosts_path.display(),
                        e
                    ))
                })?;
        }

        let (host_key, key_type) = sess
            .host_key()
            .ok_or_else(|| SyncError::provider("SSH server did not provide a host key"))?;

        match check_known_host(&known_hosts, host, port, host_key) {
            CheckResult::Match => {}
            CheckResult::Mismatch => {
                return Err(SyncError::provider(format!(
                    "SFTP host key mismatch for {}:{}",
                    host, port
                )));
            }
            CheckResult::NotFound => {
                let host_pattern = known_host_pattern(host, port);
                known_hosts
                    .add(&host_pattern, host_key, host, key_type.into())
                    .map_err(|e| {
                        SyncError::provider(format!("Failed to add SFTP host key: {}", e))
                    })?;
                known_hosts
                    .write_file(&known_hosts_path, KnownHostFileKind::OpenSSH)
                    .map_err(|e| {
                        SyncError::provider(format!(
                            "Failed to write known_hosts '{}': {}",
                            known_hosts_path.display(),
                            e
                        ))
                    })?;
            }
            CheckResult::Failure => {
                return Err(SyncError::provider(format!(
                    "Failed to verify SFTP host key for {}:{}",
                    host, port
                )));
            }
        }

        Ok(SftpHostKeyTrustResult {
            host: host.to_string(),
            port,
            fingerprint_sha256: ssh_key_fingerprint_sha256(host_key),
            known_hosts_path: known_hosts_path.display().to_string(),
        })
    }

    /// Build the full remote path by joining remote_root with the relative path
    fn full_path(&self, path: &str) -> String {
        let root = self.remote_root.trim_end_matches('/');
        if path.is_empty() {
            root.to_string()
        } else {
            let rel = path.trim_start_matches('/');
            join_remote_path(root, rel)
        }
    }

    /// Establish an SSH connection and authenticate. Returns the session.
    fn connect(&self) -> Result<Session, SyncError> {
        log::info!(
            "[Sync::SFTP] Connecting to {}@{}:{}",
            self.username,
            self.host,
            self.port
        );

        let sess = Self::connect_ssh(&self.host, self.port)?;

        self.verify_host_key(&sess)?;

        // Authenticate
        match &self.auth {
            SftpAuth::Password(password) => {
                sess.userauth_password(&self.username, password)
                    .map_err(|e| {
                        SyncError::provider(format!("Password authentication failed: {}", e))
                    })?;
            }
            SftpAuth::KeyFile {
                key_path,
                passphrase,
            } => {
                let key_path = std::path::Path::new(key_path);
                match passphrase {
                    Some(pp) => {
                        sess.userauth_pubkey_file(&self.username, None, key_path, Some(pp))
                            .map_err(|e| {
                                SyncError::provider(format!("Key authentication failed: {}", e))
                            })?;
                    }
                    None => {
                        sess.userauth_pubkey_file(&self.username, None, key_path, None)
                            .map_err(|e| {
                                SyncError::provider(format!("Key authentication failed: {}", e))
                            })?;
                    }
                }
            }
            SftpAuth::KeyPem {
                private_key,
                passphrase,
            } => {
                self.authenticate_key_pem(&sess, private_key, passphrase.as_deref())?;
            }
        }

        if !sess.authenticated() {
            return Err(SyncError::provider("SSH authentication failed".to_string()));
        }

        log::info!("[Sync::SFTP] Authenticated successfully");
        Ok(sess)
    }

    fn connect_ssh(host: &str, port: u16) -> Result<Session, SyncError> {
        let tcp = TcpStream::connect(format!("{}:{}", host, port))
            .map_err(|e| SyncError::provider(format!("TCP connection failed: {}", e)))?;

        let mut sess = Session::new()
            .map_err(|e| SyncError::provider(format!("Failed to create SSH session: {}", e)))?;

        sess.set_tcp_stream(tcp);
        // ssh2 expects this timeout in milliseconds.
        sess.set_timeout(SSH_SESSION_TIMEOUT_MS);
        sess.handshake()
            .map_err(|e| SyncError::provider(format!("SSH handshake failed: {}", e)))?;

        Ok(sess)
    }

    #[cfg(not(windows))]
    fn authenticate_key_pem(
        &self,
        sess: &Session,
        private_key: &str,
        passphrase: Option<&str>,
    ) -> Result<(), SyncError> {
        sess.userauth_pubkey_memory(&self.username, None, private_key, passphrase)
            .map_err(|e| SyncError::provider(format!("In-memory key authentication failed: {}", e)))
    }

    #[cfg(windows)]
    fn authenticate_key_pem(
        &self,
        sess: &Session,
        private_key: &str,
        passphrase: Option<&str>,
    ) -> Result<(), SyncError> {
        let key_path = write_temp_private_key(private_key)?;
        let auth_result = sess
            .userauth_pubkey_file(&self.username, None, &key_path, passphrase)
            .map_err(|e| SyncError::provider(format!("Key authentication failed: {}", e)));

        if let Err(e) = std::fs::remove_file(&key_path) {
            log::warn!("[Sync::SFTP] Failed to remove temporary key file: {}", e);
        }

        auth_result
    }

    fn verify_host_key(&self, sess: &Session) -> Result<(), SyncError> {
        let known_hosts_path = default_known_hosts_path().ok_or_else(|| {
            SyncError::provider("Cannot locate user home directory for known_hosts".to_string())
        })?;

        let mut known_hosts = sess
            .known_hosts()
            .map_err(|e| SyncError::provider(format!("Failed to initialize known_hosts: {}", e)))?;
        if known_hosts_path.exists() {
            known_hosts
                .read_file(&known_hosts_path, KnownHostFileKind::OpenSSH)
                .map_err(|e| {
                    SyncError::provider(format!(
                        "Failed to read known_hosts '{}': {}",
                        known_hosts_path.display(),
                        e
                    ))
                })?;
        }

        let (host_key, _) = sess
            .host_key()
            .ok_or_else(|| SyncError::provider("SSH server did not provide a host key"))?;

        match check_known_host(&known_hosts, &self.host, self.port, host_key) {
            CheckResult::Match => Ok(()),
            CheckResult::Mismatch => Err(SyncError::provider(format!(
                "SFTP host key mismatch for {}:{}",
                self.host, self.port
            ))),
            CheckResult::NotFound => Err(SyncError::provider(format!(
                "SFTP host key for {}:{} is not trusted; add it to known_hosts first",
                self.host, self.port
            ))),
            CheckResult::Failure => Err(SyncError::provider(format!(
                "Failed to verify SFTP host key for {}:{}",
                self.host, self.port
            ))),
        }
    }

    /// Open an SFTP session from an established SSH session
    fn open_sftp(sess: &Session) -> Result<ssh2::Sftp, SyncError> {
        sess.sftp()
            .map_err(|e| SyncError::provider(format!("SFTP subsystem failed: {}", e)))
    }

    /// Clone connection params for use in spawn_blocking
    fn clone_params(&self) -> Self {
        Self {
            host: self.host.clone(),
            port: self.port,
            username: self.username.clone(),
            remote_root: self.remote_root.clone(),
            auth: self.auth.clone(),
        }
    }

    /// Run a blocking SFTP operation on a separate thread.
    /// Connects, runs the operation, and disconnects.
    async fn run_sftp<F, T>(&self, operation: F) -> Result<T, SyncError>
    where
        F: FnOnce(&ssh2::Sftp) -> Result<T, SyncError> + Send + 'static,
        T: Send + 'static,
    {
        let params = self.clone_params();

        tokio::task::spawn_blocking(move || {
            let sess = params.connect()?;
            let sftp = Self::open_sftp(&sess)?;
            let result = operation(&sftp);
            // Drop sftp before session to ensure clean teardown
            drop(sftp);
            drop(sess);
            result
        })
        .await
        .map_err(|e| SyncError::provider(format!("SFTP task panicked: {}", e)))?
    }
}

fn default_known_hosts_path() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
        .map(|home| home.join(".ssh").join("known_hosts"))
}

fn known_host_pattern(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{}]:{}", host, port)
    }
}

fn check_known_host(
    known_hosts: &KnownHosts,
    host: &str,
    port: u16,
    host_key: &[u8],
) -> CheckResult {
    if port == 22 {
        known_hosts.check(host, host_key)
    } else {
        known_hosts.check_port(host, port, host_key)
    }
}

fn ssh_key_fingerprint_sha256(host_key: &[u8]) -> String {
    let digest = Sha256::digest(host_key);
    format!("SHA256:{}", STANDARD_NO_PAD.encode(digest))
}

#[cfg(windows)]
fn write_temp_private_key(private_key: &str) -> Result<PathBuf, SyncError> {
    let temp_dir = std::env::temp_dir();
    let pid = std::process::id();

    for attempt in 0..16 {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        let key_path = temp_dir.join(format!(
            "cliporax-sftp-key-{}-{}-{}.pem",
            pid, nonce, attempt
        ));

        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&key_path)
        {
            Ok(mut file) => {
                file.write_all(private_key.as_bytes()).map_err(|e| {
                    let _ = std::fs::remove_file(&key_path);
                    SyncError::provider(format!("Failed to write temporary key file: {}", e))
                })?;
                return Ok(key_path);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                return Err(SyncError::provider(format!(
                    "Failed to create temporary key file: {}",
                    e
                )));
            }
        }
    }

    Err(SyncError::provider(
        "Failed to create unique temporary key file".to_string(),
    ))
}

#[async_trait]
impl SyncProvider for SftpProvider {
    async fn test_connection(&self) -> Result<(), SyncError> {
        log::info!(
            "[Sync::SFTP] Testing connection to {}:{}",
            self.host,
            self.port
        );

        let remote_root = self.remote_root.clone();
        let username = self.username.clone();
        let host = self.host.clone();
        let port = self.port;

        self.run_sftp(move |sftp| {
            // Try to read the remote root directory to verify access
            let _ = sftp.readdir(Path::new(&remote_root)).map_err(|e| {
                SyncError::provider(format!(
                    "Cannot access remote root '{}': {}",
                    remote_root, e
                ))
            })?;

            log::info!(
                "[Sync::SFTP] Connection test successful: {}@{}:{}",
                username,
                host,
                port
            );
            Ok(())
        })
        .await
    }

    async fn list(&self, prefix: &str) -> Result<Vec<RemoteObject>, SyncError> {
        log::debug!("[Sync::SFTP] Listing: {}", prefix);

        let full = self.full_path(prefix);
        let prefix_owned = prefix.to_string();

        self.run_sftp(move |sftp| {
            let entries = sftp.readdir(Path::new(&full)).map_err(|e| {
                SyncError::provider(format!("Failed to read directory '{}': {}", full, e))
            })?;

            let mut objects = Vec::new();
            for (name, stat) in entries {
                let path = if prefix_owned.is_empty() {
                    name.to_string_lossy().to_string()
                } else {
                    join_remote_path(&prefix_owned, &name.to_string_lossy())
                };

                let size = stat.size.unwrap_or(0);
                let modified_at = stat
                    .mtime
                    .and_then(|m| chrono::DateTime::<Utc>::from_timestamp(m as i64, 0));

                objects.push(RemoteObject {
                    path,
                    size,
                    modified_at,
                    etag: None,
                });
            }

            log::debug!(
                "[Sync::SFTP] Listed {} entries in {}",
                objects.len(),
                prefix_owned
            );
            Ok(objects)
        })
        .await
    }

    async fn stat(&self, path: &str) -> Result<Option<RemoteObject>, SyncError> {
        log::debug!("[Sync::SFTP] Stat: {}", path);

        let full = self.full_path(path);
        let path_owned = path.to_string();

        self.run_sftp(move |sftp| {
            match sftp.stat(Path::new(&full)) {
                Ok(stat) => {
                    let size = stat.size.unwrap_or(0);
                    let modified_at = stat
                        .mtime
                        .and_then(|m| chrono::DateTime::<Utc>::from_timestamp(m as i64, 0));

                    Ok(Some(RemoteObject {
                        path: path_owned,
                        size,
                        modified_at,
                        etag: None,
                    }))
                }
                Err(e) => {
                    // Check if it's a "not found" error
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("not found")
                        || err_str.contains("no such file")
                        || err_str.contains("no such")
                        || err_str.contains("does not exist")
                    {
                        log::debug!("[Sync::SFTP] Path not found: {}", path_owned);
                        Ok(None)
                    } else {
                        Err(SyncError::provider(format!(
                            "Failed to stat '{}': {}",
                            full, e
                        )))
                    }
                }
            }
        })
        .await
    }

    async fn get(&self, path: &str) -> Result<Vec<u8>, SyncError> {
        log::debug!("[Sync::SFTP] Getting: {}", path);

        let full = self.full_path(path);
        let path_owned = path.to_string();

        self.run_sftp(move |sftp| {
            let mut file = sftp
                .open(Path::new(&full))
                .map_err(|e| SyncError::provider(format!("Failed to open '{}': {}", full, e)))?;

            let mut data = Vec::new();
            file.read_to_end(&mut data)
                .map_err(|e| SyncError::provider(format!("Failed to read '{}': {}", full, e)))?;

            log::debug!("[Sync::SFTP] Got {} bytes from {}", data.len(), path_owned);
            Ok(data)
        })
        .await
    }

    async fn put(&self, path: &str, data: Vec<u8>) -> Result<(), SyncError> {
        log::debug!("[Sync::SFTP] Putting: {} ({} bytes)", path, data.len());

        let full = self.full_path(path);
        let path_owned = path.to_string();
        let data_len = data.len();

        // Ensure parent directory exists first
        let params = self.clone_params();
        if let Some(parent) = Path::new(&full).parent() {
            let parent_str = parent.to_string_lossy().to_string();
            // We need to run mkdir_all before the put, but mkdir_all is also async.
            // To avoid recursive run_sftp calls, we do it inline here.
            mkdir_all_blocking(&params, &parent_str)?;
        }

        self.run_sftp(move |sftp| {
            let mut file = sftp
                .create(Path::new(&full))
                .map_err(|e| SyncError::provider(format!("Failed to create '{}': {}", full, e)))?;

            file.write_all(&data)
                .map_err(|e| SyncError::provider(format!("Failed to write '{}': {}", full, e)))?;

            log::debug!("[Sync::SFTP] Put {} bytes to {}", data_len, path_owned);
            Ok(())
        })
        .await
    }

    async fn mkdir_all(&self, path: &str) -> Result<(), SyncError> {
        log::debug!("[Sync::SFTP] Mkdir: {}", path);

        let params = self.clone_params();
        let full = self.full_path(path);

        tokio::task::spawn_blocking(move || mkdir_all_blocking(&params, &full))
            .await
            .map_err(|e| SyncError::provider(format!("SFTP task panicked: {}", e)))?
    }

    async fn move_object(&self, from: &str, to: &str) -> Result<(), SyncError> {
        log::debug!("[Sync::SFTP] Move: {} -> {}", from, to);

        let from_full = self.full_path(from);
        let to_full = self.full_path(to);
        let from_owned = from.to_string();
        let to_owned = to.to_string();

        // Ensure parent directory of target exists
        let params = self.clone_params();
        if let Some(parent) = Path::new(&to_full).parent() {
            let parent_str = parent.to_string_lossy().to_string();
            mkdir_all_blocking(&params, &parent_str)?;
        }

        self.run_sftp(move |sftp| {
            sftp.rename(
                Path::new(&from_full),
                Path::new(&to_full),
                Some(RenameFlags::OVERWRITE),
            )
            .map_err(|e| {
                SyncError::provider(format!(
                    "Failed to rename '{}' -> '{}': {}",
                    from_full, to_full, e
                ))
            })?;

            log::debug!("[Sync::SFTP] Moved: {} -> {}", from_owned, to_owned);
            Ok(())
        })
        .await
    }

    async fn delete(&self, path: &str) -> Result<(), SyncError> {
        log::debug!("[Sync::SFTP] Delete: {}", path);

        let full = self.full_path(path);
        let path_owned = path.to_string();

        self.run_sftp(move |sftp| {
            // Check if it's a directory or file
            match sftp.stat(Path::new(&full)) {
                Ok(stat) => {
                    match stat.file_type() {
                        FileType::Directory => {
                            // Recursive delete for directories
                            recursive_delete_sftp(sftp, Path::new(&full)).map_err(|e| {
                                SyncError::provider(format!(
                                    "Failed to delete directory '{}': {}",
                                    full, e
                                ))
                            })?;
                        }
                        _ => {
                            // It's a file
                            sftp.unlink(Path::new(&full)).map_err(|e| {
                                SyncError::provider(format!(
                                    "Failed to delete file '{}': {}",
                                    full, e
                                ))
                            })?;
                        }
                    }
                }
                Err(e) => {
                    // If file doesn't exist, that's fine for delete
                    let err_str = e.to_string().to_lowercase();
                    if err_str.contains("not found")
                        || err_str.contains("no such file")
                        || err_str.contains("no such")
                        || err_str.contains("does not exist")
                    {
                        log::debug!("[Sync::SFTP] Path already deleted: {}", path_owned);
                        return Ok(());
                    }
                    return Err(SyncError::provider(format!(
                        "Failed to stat '{}': {}",
                        full, e
                    )));
                }
            }

            log::debug!("[Sync::SFTP] Deleted: {}", path_owned);
            Ok(())
        })
        .await
    }
}

/// Blocking helper to create a directory hierarchy via SFTP
fn mkdir_all_blocking(params: &SftpProvider, path: &str) -> Result<(), SyncError> {
    let sess = params.connect()?;
    let sftp = SftpProvider::open_sftp(&sess)?;

    let path_str = path.trim_end_matches('/');

    // Build up path components
    let parts: Vec<&str> = path_str.split('/').filter(|s| !s.is_empty()).collect();

    let mut current = String::new();
    if path_str.starts_with('/') {
        current.push('/');
    }

    for part in parts {
        if current.ends_with('/') || current.is_empty() {
            current.push_str(part);
        } else {
            current.push('/');
            current.push_str(part);
        }

        // Try to create the directory; ignore "already exists" errors
        match sftp.mkdir(Path::new(&current), 0o755) {
            Ok(_) => {
                log::debug!("[Sync::SFTP] Created directory: {}", current);
            }
            Err(e) => {
                // Check if directory already exists
                match sftp.stat(Path::new(&current)) {
                    Ok(stat) => {
                        if stat.file_type() == FileType::Directory {
                            log::debug!("[Sync::SFTP] Directory already exists: {}", current);
                        } else {
                            drop(sftp);
                            drop(sess);
                            return Err(SyncError::provider(format!(
                                "Path '{}' exists but is not a directory",
                                current
                            )));
                        }
                    }
                    Err(_) => {
                        drop(sftp);
                        drop(sess);
                        return Err(SyncError::provider(format!(
                            "Failed to create directory '{}': {}",
                            current, e
                        )));
                    }
                }
            }
        }
    }

    log::debug!("[Sync::SFTP] Mkdir all complete: {}", path);
    drop(sftp);
    drop(sess);
    Ok(())
}

/// Recursively delete a directory via SFTP (blocking)
fn recursive_delete_sftp(sftp: &ssh2::Sftp, path: &Path) -> Result<(), std::io::Error> {
    let entries = sftp.readdir(path)?;

    for (name, _stat) in entries {
        let entry_path = path.join(&name);

        // Check if it's a directory
        match sftp.stat(&entry_path) {
            Ok(stat) => {
                if stat.file_type() == FileType::Directory {
                    // Recurse into directory
                    recursive_delete_sftp(sftp, &entry_path)?;
                    // Remove empty directory
                    sftp.rmdir(&entry_path)?;
                } else {
                    // Remove file
                    sftp.unlink(&entry_path)?;
                }
            }
            Err(_) => {
                // If we can't stat it, try to unlink it as a file
                let _ = sftp.unlink(&entry_path);
            }
        }
    }

    // Remove the now-empty directory
    sftp.rmdir(path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_full_path_construction() {
        let provider = SftpProvider::new(
            "example.com",
            22,
            "user",
            "/remote/root",
            SftpAuth::Password("pass".to_string()),
        );

        assert_eq!(provider.full_path(""), "/remote/root");
        assert_eq!(provider.full_path("file.txt"), "/remote/root/file.txt");
        assert_eq!(
            provider.full_path("/subdir/file.txt"),
            "/remote/root/subdir/file.txt"
        );
    }

    #[test]
    fn test_full_path_with_trailing_slash() {
        let provider = SftpProvider::new(
            "example.com",
            22,
            "user",
            "/remote/root/",
            SftpAuth::Password("pass".to_string()),
        );

        assert_eq!(provider.full_path(""), "/remote/root");
        assert_eq!(provider.full_path("file.txt"), "/remote/root/file.txt");
    }
}
