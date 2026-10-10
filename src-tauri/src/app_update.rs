//! Downloads official release bundles; installation remains an explicit user action.
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
    time::Duration,
};
use tauri::Manager;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const RELEASE_API: &str = "https://api.github.com/repos/Cliporax/Cliporax/releases/latest";
const MAX_PACKAGE_SIZE: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Clone, Copy, Default, Serialize, PartialEq, Debug)]
#[serde(rename_all = "camelCase")]
pub enum Phase {
    #[default]
    Idle,
    Checking,
    Current,
    Available,
    Downloading,
    Ready,
    Installing,
}

impl Phase {
    fn busy(self) -> bool {
        matches!(self, Self::Checking | Self::Downloading | Self::Installing)
    }
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    phase: Phase,
    version: Option<String>,
    file_name: Option<String>,
    downloaded: u64,
    total: u64,
    last_checked: Option<String>,
    error: Option<String>,
    manual_install: bool,
    installer_opened: bool,
    package_path: Option<String>,
}

#[derive(Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    size: u64,
    digest: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<Asset>,
}

#[derive(Default)]
struct Inner {
    status: UpdateStatus,
    asset: Option<Asset>,
    path: Option<PathBuf>,
}

#[derive(Default)]
pub struct AppUpdateState(Mutex<Inner>);

impl AppUpdateState {
    fn lock(&self) -> Result<MutexGuard<'_, Inner>, String> {
        self.0
            .lock()
            .map_err(|_| "Update state is unavailable".into())
    }
}

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(concat!("Cliporax/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(30))
        .timeout(timeout)
        .build()
        .map_err(|e| e.to_string())
}

fn newer_version(release: &Release, current: &str) -> Result<Option<Version>, String> {
    let version = Version::parse(release.tag_name.trim_start_matches('v'))
        .map_err(|_| "Release has an invalid version")?;
    let current = Version::parse(current).map_err(|_| "App has an invalid version")?;
    Ok(
        (!release.draft && !release.prerelease && version.pre.is_empty() && version > current)
            .then_some(version),
    )
}

fn sha256_digest(asset: &Asset) -> Result<&str, String> {
    asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|d| d.len() == 64 && d.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| {
            "Release package has no valid SHA-256 checksum. Try checking again later.".into()
        })
}

fn validate_asset(asset: &Asset, tag: &str) -> Result<(), String> {
    if asset.name.is_empty()
        || !asset
            .name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err("Invalid release package name".into());
    }
    let expected = format!(
        "https://github.com/Cliporax/Cliporax/releases/download/{tag}/{}",
        asset.name
    );
    if asset.browser_download_url != expected || asset.size == 0 || asset.size > MAX_PACKAGE_SIZE {
        return Err("Invalid release package URL or size".into());
    }
    sha256_digest(asset)?;
    Ok(())
}

fn select_asset<'a>(
    assets: &'a [Asset],
    os: &str,
    arch: &str,
    linux_format: &str,
    portable: bool,
) -> Result<(&'a Asset, bool), String> {
    let aliases: &[&str] = match arch {
        "x86_64" => &["x64", "x86_64", "amd64"],
        "aarch64" => &["aarch64", "arm64"],
        _ => return Err(format!("No release package supports architecture {arch}")),
    };
    let (extension, manual) = match os {
        "windows" if portable => ("_portable.zip", true),
        "windows" => ("-setup.exe", false),
        "macos" => (".dmg", false),
        "linux" if linux_format == "deb" => (".deb", false),
        "linux" if linux_format == "rpm" => (".rpm", false),
        "linux" => (".appimage", true),
        _ => return Err(format!("Updates are unavailable for {os}")),
    };
    assets
        .iter()
        .find(|asset| {
            let name = asset.name.to_ascii_lowercase();
            (name.starts_with("cliporax_") || name.starts_with("cliporax-"))
                && name.ends_with(extension)
                && aliases.iter().any(|alias| {
                    ['_', '.', '-'].iter().any(|before| {
                        ['_', '.', '-']
                            .iter()
                            .any(|after| name.contains(&format!("{before}{alias}{after}")))
                    })
                })
        })
        .map(|a| (a, manual))
        .ok_or_else(|| {
            format!(
                "No matching {os}/{arch} installation package is published yet. Try again later."
            )
        })
}

fn linux_format() -> &'static str {
    if std::env::var_os("APPIMAGE").is_some() {
        "appimage"
    } else if Path::new("/etc/debian_version").exists() {
        "deb"
    } else if Path::new("/etc/redhat-release").exists()
        || Path::new("/etc/SuSE-release").exists()
        || Path::new("/usr/bin/rpm").exists()
    {
        "rpm"
    } else {
        "appimage"
    }
}

fn portable_windows() -> bool {
    cfg!(target_os = "windows")
        && std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .is_some_and(|dir| {
                crate::portable::PORTABLE_MARKERS
                    .iter()
                    .any(|marker| dir.join(marker).exists())
                    || dir.join("data").join("cliporax.db").exists()
                    || dir.join("data").join("settings.json").exists()
            })
}

#[tauri::command]
pub fn get_app_update_status(
    state: tauri::State<'_, AppUpdateState>,
) -> Result<UpdateStatus, String> {
    Ok(state.lock()?.status.clone())
}

#[tauri::command]
pub async fn check_app_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppUpdateState>,
) -> Result<UpdateStatus, String> {
    {
        let mut inner = state.lock()?;
        if inner.status.phase.busy() {
            return Err("An update operation is already running".into());
        }
        // Preserve a verified download until the user has installed it.
        if inner.status.phase == Phase::Ready {
            return Ok(inner.status.clone());
        }
        inner.status.phase = Phase::Checking;
        inner.status.error = None;
    }
    let result: Result<_, String> = async {
        let release = client(Duration::from_secs(30))?
            .get(RELEASE_API)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?
            .json::<Release>()
            .await
            .map_err(|e| e.to_string())?;
        let Some(version) = newer_version(&release, &app.package_info().version.to_string())?
        else {
            return Ok(None);
        };
        let (asset, manual) = select_asset(
            &release.assets,
            std::env::consts::OS,
            std::env::consts::ARCH,
            linux_format(),
            portable_windows(),
        )?;
        validate_asset(asset, &release.tag_name)?;
        Ok(Some((version.to_string(), asset.clone(), manual)))
    }
    .await;
    let mut inner = state.lock()?;
    inner.status.phase = Phase::Idle;
    match result {
        Ok(update) => {
            inner.status = UpdateStatus {
                last_checked: Some(chrono::Utc::now().to_rfc3339()),
                ..Default::default()
            };
            inner.asset = None;
            if let Some((version, asset, manual)) = update {
                inner.status.phase = Phase::Available;
                inner.status.version = Some(version);
                inner.status.file_name = Some(asset.name.clone());
                inner.status.total = asset.size;
                inner.status.manual_install = manual;
                inner.asset = Some(asset);
            } else {
                inner.status.phase = Phase::Current;
            }
        }
        Err(error) => inner.status.error = Some(error),
    }
    Ok(inner.status.clone())
}

async fn download_package(
    client: &reqwest::Client,
    asset: &Asset,
    directory: &Path,
    state: &AppUpdateState,
) -> Result<PathBuf, String> {
    tokio::fs::create_dir_all(directory)
        .await
        .map_err(|e| format!("Cannot create update folder: {e}"))?;
    let partial = directory.join("download.part");
    let target = directory.join(&asset.name);
    let result = async {
        let mut response = client
            .get(&asset.browser_download_url)
            .send()
            .await
            .map_err(|e| e.to_string())?
            .error_for_status()
            .map_err(|e| e.to_string())?;
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)
            .await
            .map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut received = 0u64;
        while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
            received += chunk.len() as u64;
            if received > asset.size {
                return Err("Download exceeds the release package size".into());
            }
            file.write_all(&chunk).await.map_err(|e| e.to_string())?;
            hash.update(&chunk);
            state.lock()?.status.downloaded = received;
        }
        verify_download(asset, received, &format!("{:x}", hash.finalize()))?;
        #[cfg(target_os = "linux")]
        if asset.name.to_ascii_lowercase().ends_with(".appimage") {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(std::fs::Permissions::from_mode(0o755))
                .await
                .map_err(|e| e.to_string())?;
        }
        file.sync_all().await.map_err(|e| e.to_string())?;
        drop(file);
        tokio::fs::rename(&partial, &target)
            .await
            .map_err(|e| e.to_string())?;
        Ok(target)
    }
    .await;
    if result.is_err() {
        let _ = tokio::fs::remove_dir_all(directory).await;
    }
    result
}

fn verify_download(asset: &Asset, size: u64, hash: &str) -> Result<(), String> {
    if size != asset.size || !sha256_digest(asset)?.eq_ignore_ascii_case(hash) {
        return Err("Package verification failed. Please download it again.".into());
    }
    Ok(())
}

#[tauri::command]
pub async fn download_app_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppUpdateState>,
) -> Result<UpdateStatus, String> {
    let client = client(Duration::from_secs(30 * 60))?;
    let directory = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("updates")
        .join(uuid::Uuid::new_v4().to_string());
    let asset = {
        let mut inner = state.lock()?;
        if inner.status.phase != Phase::Available {
            return Err("Check for an update before downloading".into());
        }
        let asset = inner.asset.clone().ok_or("No release package selected")?;
        inner.status.phase = Phase::Downloading;
        inner.status.downloaded = 0;
        inner.status.error = None;
        inner.status.installer_opened = false;
        asset
    };
    let result = download_package(&client, &asset, &directory, &state).await;
    let mut inner = state.lock()?;
    match result {
        Ok(path) => {
            inner.status.phase = Phase::Ready;
            inner.status.package_path = Some(path.to_string_lossy().into_owned());
            inner.path = Some(path);
        }
        Err(error) => {
            inner.status.phase = Phase::Available;
            inner.status.downloaded = 0;
            inner.status.error = Some(error);
        }
    }
    Ok(inner.status.clone())
}

async fn verify_package_file(asset: &Asset, path: &Path) -> Result<(), String> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| format!("Cannot read downloaded package: {e}"))?;
    if file.metadata().await.map_err(|e| e.to_string())?.len() != asset.size {
        return Err("Downloaded package size has changed".into());
    }
    let mut hash = Sha256::new();
    let mut size = 0;
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).await.map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        size += read as u64;
        if size > asset.size {
            return Err("Downloaded package size has changed".into());
        }
        hash.update(&buffer[..read]);
    }
    verify_download(asset, size, &format!("{:x}", hash.finalize()))
}

#[tauri::command]
pub async fn install_app_update(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppUpdateState>,
) -> Result<UpdateStatus, String> {
    if app.config().identifier != "com.cliporax.app" {
        return Err(
            "Install updates from the production app to keep development data isolated.".into(),
        );
    }
    let (asset, path, manual) = {
        let mut inner = state.lock()?;
        if inner.status.phase != Phase::Ready {
            return Err("Download an update before installing".into());
        }
        let asset = inner.asset.clone().ok_or("No release package selected")?;
        let path = inner.path.clone().ok_or("No downloaded package")?;
        inner.status.phase = Phase::Installing;
        inner.status.error = None;
        (asset, path, inner.status.manual_install)
    };
    // Recheck cached bytes before allowing execution, including after a deferred installation.
    if let Err(error) = verify_package_file(&asset, &path).await {
        let _ = tokio::fs::remove_file(&path).await;
        let mut inner = state.lock()?;
        inner.path = None;
        inner.status.package_path = None;
        inner.status.downloaded = 0;
        inner.status.phase = Phase::Available;
        inner.status.error = Some(error);
        return Ok(inner.status.clone());
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        let open_path = if manual {
            path.parent().ok_or("Invalid package folder")?
        } else {
            &path
        };
        // Only backend-selected paths reach the OS launcher. Await its result so
        // missing package handlers are shown as errors rather than false success.
        open::that(open_path).map_err(|e| format!("Cannot open installer: {e}"))
    })
    .await
    .map_err(|e| e.to_string())
    .and_then(|r| r);
    let mut inner = state.lock()?;
    inner.status.phase = Phase::Ready;
    match result {
        Ok(()) => inner.status.installer_opened = true,
        Err(error) => inner.status.error = Some(error),
    }
    Ok(inner.status.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.into(),
            browser_download_url: format!(
                "https://github.com/Cliporax/Cliporax/releases/download/v1.3.0/{name}"
            ),
            size: 3,
            digest: Some(format!("sha256:{:x}", Sha256::digest(b"abc"))),
        }
    }

    #[test]
    fn compares_semver_and_ignores_unstable_releases() {
        let mut release = Release {
            tag_name: "v1.10.0".into(),
            draft: false,
            prerelease: false,
            assets: vec![],
        };
        assert!(newer_version(&release, "1.9.0").unwrap().is_some());
        assert!(newer_version(&release, "1.10.0").unwrap().is_none());
        assert!(newer_version(&release, "2.0.0").unwrap().is_none());
        release.prerelease = true;
        assert!(newer_version(&release, "1.0.0").unwrap().is_none());
        release.prerelease = false;
        release.tag_name = "v2.0.0-beta.1".into();
        assert!(newer_version(&release, "1.0.0").unwrap().is_none());
        release.tag_name = "not-a-version".into();
        assert!(newer_version(&release, "1.0.0").is_err());
    }

    #[test]
    fn selects_published_and_future_package_names_for_each_platform() {
        let assets: Vec<_> = [
            "Cliporax_1.3.0_aarch64.dmg",
            "Cliporax_1.3.0_x64.dmg",
            "Cliporax_1.3.0_x64-setup.exe",
            "Cliporax_1.3.0_amd64.deb",
            "Cliporax-1.3.0-1.x86_64.rpm",
            "Cliporax_1.3.0_amd64.AppImage",
            "Cliporax_v1.3.0_windows_x64_portable.zip",
        ]
        .iter()
        .map(|n| asset(n))
        .collect();
        for (os, arch, format, portable, index, manual) in [
            ("macos", "aarch64", "", false, 0, false),
            ("macos", "x86_64", "", false, 1, false),
            ("windows", "x86_64", "", false, 2, false),
            ("linux", "x86_64", "deb", false, 3, false),
            ("linux", "x86_64", "rpm", false, 4, false),
            ("linux", "x86_64", "appimage", false, 5, true),
            ("windows", "x86_64", "", true, 6, true),
        ] {
            let (selected, is_manual) = select_asset(&assets, os, arch, format, portable).unwrap();
            assert_eq!(selected.name, assets[index].name);
            assert_eq!(is_manual, manual);
        }
        assert!(select_asset(&assets, "windows", "aarch64", "", false).is_err());
        assert!(select_asset(&assets, "linux", "x86", "deb", false).is_err());
        assert!(select_asset(&[], "linux", "x86_64", "deb", false).is_err());
        let future = vec![asset("Cliporax_1.3.0_windows_x64-setup.exe")];
        assert!(select_asset(&future, "windows", "x86_64", "", false).is_ok());
    }

    #[test]
    fn rejects_untrusted_urls_paths_and_missing_digests() {
        let mut package = asset("Cliporax_1.3.0_amd64.deb");
        assert!(validate_asset(&package, "v1.3.0").is_ok());
        package.browser_download_url = "https://example.com/package.deb".into();
        assert!(validate_asset(&package, "v1.3.0").is_err());
        assert!(validate_asset(&asset("../evil.exe"), "v1.3.0").is_err());
        package = asset("Cliporax_1.3.0_amd64.deb");
        package.digest = None;
        assert!(validate_asset(&package, "v1.3.0").is_err());
    }

    #[tokio::test]
    async fn streaming_download_verifies_bytes_and_removes_failed_partial_files() {
        for (body, content_length, success) in [
            ("abc", 3, true),
            ("xyz", 3, false),  // equal length, wrong digest
            ("ab", 2, false),   // short package
            ("abcd", 4, false), // oversized package
            ("ab", 3, false),   // interrupted HTTP response
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let server = tokio::spawn(async move {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = [0; 4096];
                socket.read(&mut request).await.unwrap();
                let response = format!("HTTP/1.1 200 OK\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n{body}");
                socket.write_all(response.as_bytes()).await.unwrap();
            });
            let mut package = asset("Cliporax_1.3.0_amd64.AppImage");
            package.browser_download_url = format!("http://{address}/package");
            let directory = tempfile::tempdir().unwrap();
            let download_dir = directory.path().join("download");
            let state = AppUpdateState::default();
            // HTTP is permitted only in this loopback fixture. Production clients require HTTPS.
            let client = reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(5))
                .build()
                .unwrap();
            let result = download_package(&client, &package, &download_dir, &state).await;
            server.await.unwrap();
            assert_eq!(
                result.is_ok(),
                success,
                "body: {body}, length: {content_length}"
            );
            if success {
                let path = result.unwrap();
                assert_eq!(tokio::fs::read(&path).await.unwrap(), b"abc");
                #[cfg(target_os = "linux")]
                {
                    use std::os::unix::fs::PermissionsExt;
                    assert_eq!(
                        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                        0o755
                    );
                }
                assert_eq!(state.lock().unwrap().status.downloaded, 3);
                assert!(!download_dir.join("download.part").exists());
            } else {
                assert!(
                    !download_dir.exists(),
                    "Failed downloads must not leave partial installers"
                );
            }
        }
    }

    #[tokio::test]
    async fn cached_package_must_match_size_and_checksum_before_installing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("installer.deb");
        let package = asset("Cliporax_1.3.0_amd64.deb");
        tokio::fs::write(&path, b"abc").await.unwrap();
        assert!(verify_package_file(&package, &path).await.is_ok());
        tokio::fs::write(&path, b"xyz").await.unwrap();
        assert!(verify_package_file(&package, &path).await.is_err());
        tokio::fs::write(&path, b"ab").await.unwrap();
        assert!(verify_package_file(&package, &path).await.is_err());
        tokio::fs::remove_file(&path).await.unwrap();
        assert!(verify_package_file(&package, &path).await.is_err());
    }
}
