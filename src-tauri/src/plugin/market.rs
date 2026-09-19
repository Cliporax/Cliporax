//! Plugin marketplace client and installer.

mod models;

use self::models::GithubRelease;
pub use self::models::{
    InstallPluginRequest, InstallPluginResult, InstalledPluginVersion, MarketCompatibility,
    MarketIndex, MarketInstallStatus, MarketPlugin, MarketPluginAsset, MarketPluginIcon,
    MarketPublisher, MarketRefreshResult, PluginMarketSource,
};
use crate::plugin::get_plugin_dir;
use crate::plugin::lifecycle::registry::PluginRegistry;
use crate::plugin::manifest::PluginManifest;
use chrono::Utc;
use semver::Version;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::sync::RwLock;

const DEFAULT_MARKET_SOURCE_ID: &str = "official";
const DEFAULT_MARKET_SOURCE_NAME: &str = "Cliporax Official Plugins";
const DEFAULT_MARKET_RELEASE_API_URL: &str =
    "https://github.com/Cliporax/cliporax-plugin-market/releases/latest/download/index.json";
const MARKET_DIR_NAME: &str = "plugin-market";
const MARKET_INDEX_FILE: &str = "index.json";
const MAX_PLUGIN_PACKAGE_SIZE: u64 = 64 * 1024 * 1024;

pub fn default_market_sources() -> Vec<PluginMarketSource> {
    vec![PluginMarketSource {
        id: DEFAULT_MARKET_SOURCE_ID.to_string(),
        name: DEFAULT_MARKET_SOURCE_NAME.to_string(),
        release_api_url: DEFAULT_MARKET_RELEASE_API_URL.to_string(),
        readonly: true,
    }]
}

pub async fn get_sources() -> Result<Vec<PluginMarketSource>, String> {
    Ok(default_market_sources())
}

pub async fn refresh_market(
    app_handle: tauri::AppHandle,
    registry: Arc<RwLock<PluginRegistry>>,
) -> Result<MarketRefreshResult, String> {
    let source = default_market_sources()
        .into_iter()
        .next()
        .ok_or_else(|| "No plugin market source configured".to_string())?;

    let index_bytes = fetch_market_index(&source.release_api_url).await?;

    let mut index = parse_market_index(&index_bytes)?;
    write_cached_index(&app_handle, &index_bytes).await?;
    apply_install_status(&mut index.plugins, &registry).await;

    Ok(MarketRefreshResult {
        source_id: source.id,
        stale: false,
        plugins: index.plugins,
    })
}

async fn fetch_market_index(source_url: &str) -> Result<Vec<u8>, String> {
    let client = http_client()?;
    if is_direct_market_index_url(source_url) {
        return client
            .get(source_url)
            .send()
            .await
            .map_err(|e| format!("Failed to download plugin market index: {}", e))?
            .error_for_status()
            .map_err(|e| format!("Plugin market index request failed: {}", e))?
            .bytes()
            .await
            .map(|bytes| bytes.to_vec())
            .map_err(|e| format!("Failed to read plugin market index: {}", e));
    }

    let release = client
        .get(source_url)
        .send()
        .await
        .map_err(|e| format!("Failed to fetch plugin market release: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Plugin market release request failed: {}", e))?
        .json::<GithubRelease>()
        .await
        .map_err(|e| format!("Failed to parse plugin market release: {}", e))?;

    let index_asset = release
        .assets
        .iter()
        .find(|asset| asset.name == MARKET_INDEX_FILE)
        .ok_or_else(|| "Plugin market release does not contain index.json".to_string())?;

    let index_bytes = client
        .get(&index_asset.browser_download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download plugin market index: {}", e))?
        .error_for_status()
        .map_err(|e| format!("Plugin market index request failed: {}", e))?
        .bytes()
        .await
        .map_err(|e| format!("Failed to read plugin market index: {}", e))?;

    if index_asset.size > 0 && index_asset.size != index_bytes.len() as u64 {
        return Err("Plugin market index size does not match release metadata".to_string());
    }

    Ok(index_bytes.to_vec())
}

fn is_direct_market_index_url(source_url: &str) -> bool {
    source_url
        .split('?')
        .next()
        .map(|path| path.ends_with(&format!("/{}", MARKET_INDEX_FILE)))
        .unwrap_or(false)
}

pub async fn get_market_plugins(
    app_handle: tauri::AppHandle,
    registry: Arc<RwLock<PluginRegistry>>,
) -> Result<MarketRefreshResult, String> {
    let source = default_market_sources()
        .into_iter()
        .next()
        .ok_or_else(|| "No plugin market source configured".to_string())?;
    let cached = read_cached_index(&app_handle).await?;
    let mut index = parse_market_index(&cached)?;
    apply_install_status(&mut index.plugins, &registry).await;

    Ok(MarketRefreshResult {
        source_id: source.id,
        stale: true,
        plugins: index.plugins,
    })
}

pub async fn install_market_plugin(
    app_handle: tauri::AppHandle,
    registry: Arc<RwLock<PluginRegistry>>,
    request: InstallPluginRequest,
) -> Result<InstallPluginResult, String> {
    validate_plugin_id(&request.plugin_id)?;
    let mut index = load_index_or_refresh(app_handle.clone(), registry.clone()).await?;
    let plugin = index
        .plugins
        .drain(..)
        .find(|plugin| plugin.id == request.plugin_id)
        .ok_or_else(|| format!("Plugin not found in market: {}", request.plugin_id))?;

    {
        let reg = registry.read().await;
        if reg
            .get_manifest(&plugin.id)
            .map(|manifest| manifest.is_builtin)
            .unwrap_or(false)
        {
            return Err(format!(
                "Plugin {} is built into Cliporax and cannot be replaced from the market",
                plugin.id
            ));
        }
    }

    if !is_market_plugin_compatible(&plugin) {
        return Err(format!(
            "Plugin {} is incompatible with this Cliporax version or platform",
            plugin.id
        ));
    }

    validate_asset(&plugin)?;
    let package_path = download_package(&app_handle, &plugin).await?;
    let staging_dir = staging_dir(&app_handle, &plugin.id)?;
    remove_dir_if_exists(&staging_dir).await?;
    tokio::fs::create_dir_all(&staging_dir)
        .await
        .map_err(|e| format!("Failed to create plugin staging directory: {}", e))?;

    let manifest =
        match unpack_and_validate_package(package_path.clone(), staging_dir.clone(), &plugin).await
        {
            Ok(manifest) => manifest,
            Err(error) => {
                let package_path = package_path.clone();
                let staging_dir = staging_dir.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = tokio::fs::remove_file(package_path).await;
                    let _ = tokio::fs::remove_dir_all(staging_dir).await;
                });
                return Err(error);
            }
        };

    let plugin_dir = get_plugin_dir(&app_handle)?;
    tokio::fs::create_dir_all(&plugin_dir)
        .await
        .map_err(|e| format!("Failed to create plugin directory: {}", e))?;
    let target_dir = plugin_dir.join(&plugin.id);
    ensure_child_path(&plugin_dir, &target_dir)?;

    {
        let mut reg = registry.write().await;
        if reg.exists(&plugin.id) {
            let _ = reg.unload(&plugin.id).await;
        }
    }

    let backup_dir = plugin_dir.join(format!(".{}.backup", plugin.id));
    remove_dir_if_exists(&backup_dir).await?;
    if target_dir.exists() {
        tokio::fs::rename(&target_dir, &backup_dir)
            .await
            .map_err(|e| format!("Failed to prepare existing plugin for replacement: {}", e))?;
    }

    if let Err(error) = tokio::fs::rename(&staging_dir, &target_dir).await {
        if backup_dir.exists() {
            let _ = tokio::fs::rename(&backup_dir, &target_dir).await;
        }
        return Err(format!("Failed to install plugin package: {}", error));
    }

    remove_dir_if_exists(&backup_dir).await?;
    let _ = tokio::fs::remove_file(package_path).await;

    {
        let mut reg = registry.write().await;
        reg.discover()
            .await
            .map_err(|e| format!("Failed to refresh plugins after install: {}", e))?;
    }

    Ok(InstallPluginResult {
        plugin_id: manifest.id,
        version: manifest.version,
        installed: true,
    })
}

pub async fn uninstall_market_plugin(
    app_handle: tauri::AppHandle,
    registry: Arc<RwLock<PluginRegistry>>,
    plugin_id: String,
) -> Result<(), String> {
    validate_plugin_id(&plugin_id)?;
    let plugin_dir = get_plugin_dir(&app_handle)?;
    let target_dir = plugin_dir.join(&plugin_id);
    ensure_child_path(&plugin_dir, &target_dir)?;

    {
        let reg = registry.read().await;
        if reg
            .get_manifest(&plugin_id)
            .map(|manifest| manifest.is_builtin)
            .unwrap_or(false)
        {
            return Err(format!(
                "Plugin {} is built into Cliporax and cannot be uninstalled from the market",
                plugin_id
            ));
        }
    }

    {
        let mut reg = registry.write().await;
        if reg.exists(&plugin_id) {
            let _ = reg.unload(&plugin_id).await;
        }
    }

    remove_dir_if_exists(&target_dir).await?;

    {
        let mut reg = registry.write().await;
        reg.discover()
            .await
            .map_err(|e| format!("Failed to refresh plugins after uninstall: {}", e))?;
    }

    Ok(())
}

pub async fn get_install_status(
    app_handle: tauri::AppHandle,
    registry: Arc<RwLock<PluginRegistry>>,
    plugin_id: String,
) -> Result<MarketInstallStatus, String> {
    validate_plugin_id(&plugin_id)?;
    let result = get_market_plugins(app_handle, registry).await?;
    result
        .plugins
        .into_iter()
        .find(|plugin| plugin.id == plugin_id)
        .map(|plugin| plugin.status)
        .ok_or_else(|| format!("Plugin not found in cached market index: {}", plugin_id))
}

fn http_client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .user_agent(format!("Cliporax/{}", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| format!("Failed to create plugin market HTTP client: {}", e))
}

async fn load_index_or_refresh(
    app_handle: tauri::AppHandle,
    registry: Arc<RwLock<PluginRegistry>>,
) -> Result<MarketIndex, String> {
    // Resolve the current version, filename and checksum together before installing.
    // An existing cache may reference a release that has since been replaced.
    match refresh_market(app_handle.clone(), registry).await {
        Ok(refreshed) => Ok(MarketIndex {
            schema_version: 1,
            generated_at: Utc::now().to_rfc3339(),
            market_version: "refreshed".to_string(),
            plugins: refreshed.plugins,
        }),
        Err(refresh_error) => read_cached_index(&app_handle)
            .await
            .and_then(|bytes| parse_market_index(&bytes))
            .map_err(|cache_error| format!("{}; {}", refresh_error, cache_error)),
    }
}

fn market_dir(app_handle: &tauri::AppHandle) -> Result<PathBuf, String> {
    let data_dir = crate::portable::app_data_dir(app_handle)?;
    Ok(data_dir.join(MARKET_DIR_NAME))
}

async fn read_cached_index(app_handle: &tauri::AppHandle) -> Result<Vec<u8>, String> {
    let path = market_dir(app_handle)?.join(MARKET_INDEX_FILE);
    tokio::fs::read(&path)
        .await
        .map_err(|e| format!("No cached plugin market index at {:?}: {}", path, e))
}

async fn write_cached_index(app_handle: &tauri::AppHandle, bytes: &[u8]) -> Result<(), String> {
    let dir = market_dir(app_handle)?;
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Failed to create plugin market cache directory: {}", e))?;
    tokio::fs::write(dir.join(MARKET_INDEX_FILE), bytes)
        .await
        .map_err(|e| format!("Failed to cache plugin market index: {}", e))
}

fn parse_market_index(bytes: &[u8]) -> Result<MarketIndex, String> {
    let index: MarketIndex = serde_json::from_slice(bytes)
        .map_err(|e| format!("Failed to parse plugin market index: {}", e))?;
    validate_market_index(&index)?;
    Ok(index)
}

fn validate_market_index(index: &MarketIndex) -> Result<(), String> {
    if index.schema_version != 1 {
        return Err(format!(
            "Unsupported plugin market schema version: {}",
            index.schema_version
        ));
    }
    let mut seen = HashSet::new();
    for plugin in &index.plugins {
        validate_plugin_id(&plugin.id)?;
        if !seen.insert(plugin.id.clone()) {
            return Err(format!(
                "Duplicate plugin id in market index: {}",
                plugin.id
            ));
        }
        if Version::parse(&plugin.version).is_err() {
            return Err(format!("Invalid market plugin version: {}", plugin.version));
        }
        validate_asset(plugin)?;
    }
    Ok(())
}

fn validate_asset(plugin: &MarketPlugin) -> Result<(), String> {
    if plugin.asset.size == 0 || plugin.asset.size > MAX_PLUGIN_PACKAGE_SIZE {
        return Err(format!("Invalid package size for plugin {}", plugin.id));
    }
    if plugin.asset.content_type != "application/zip" {
        return Err(format!(
            "Invalid package content type for plugin {}",
            plugin.id
        ));
    }
    if !plugin
        .asset
        .name
        .starts_with(&format!("{}-{}", plugin.id, plugin.version))
        || !plugin.asset.name.ends_with(".cliporax-plugin.zip")
    {
        return Err(format!("Package name does not match plugin {}", plugin.id));
    }
    validate_sha256(&plugin.asset.sha256)
}

fn validate_sha256(value: &str) -> Result<(), String> {
    if value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err("Invalid sha256 digest in plugin market index".to_string())
    }
}

fn validate_plugin_id(plugin_id: &str) -> Result<(), String> {
    if plugin_id.len() > 160 || plugin_id.is_empty() || !plugin_id.contains('.') {
        return Err("Invalid plugin id".to_string());
    }
    if plugin_id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_')
    {
        Ok(())
    } else {
        Err("Plugin id contains unsupported characters".to_string())
    }
}

async fn apply_install_status(
    plugins: &mut [MarketPlugin],
    registry: &Arc<RwLock<PluginRegistry>>,
) {
    let installed: HashMap<String, InstalledPluginVersion> = {
        let reg = registry.read().await;
        reg.get_all()
            .into_iter()
            .map(|plugin| {
                (
                    plugin.id,
                    InstalledPluginVersion {
                        version: plugin.version,
                        state: plugin.state,
                        is_builtin: plugin.is_builtin,
                    },
                )
            })
            .collect()
    };

    for plugin in plugins {
        plugin.installed = installed.get(&plugin.id).cloned();
        plugin.status = if !is_market_plugin_compatible(plugin) {
            MarketInstallStatus::Incompatible
        } else if let Some(installed) = &plugin.installed {
            match (
                Version::parse(&installed.version),
                Version::parse(&plugin.version),
            ) {
                (Ok(local), Ok(remote)) if remote > local => MarketInstallStatus::UpdateAvailable,
                _ => MarketInstallStatus::Installed,
            }
        } else {
            MarketInstallStatus::NotInstalled
        };
    }
}

fn is_market_plugin_compatible(plugin: &MarketPlugin) -> bool {
    let Ok(app_version) = Version::parse(env!("CARGO_PKG_VERSION")) else {
        return true;
    };
    if let Ok(min_version) = Version::parse(&plugin.min_app_version) {
        if app_version < min_version {
            return false;
        }
    }
    if plugin.compatibility.platforms.is_empty() {
        return true;
    }
    plugin
        .compatibility
        .platforms
        .iter()
        .any(|platform| platform == current_platform())
}

fn current_platform() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        "windows"
    }
    #[cfg(target_os = "macos")]
    {
        "macos"
    }
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "linux"
    }
}

/// Recover a moved GitHub release asset using its stable repository and the
/// filename from the current index. Never rewrite third-party hosting URLs.
fn latest_github_asset_url(download_url: &str, asset_name: &str) -> Option<String> {
    let mut url = reqwest::Url::parse(download_url).ok()?;
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || url.port().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return None;
    }
    let segments: Vec<_> = url.path_segments()?.collect();
    if segments.len() != 6
        || segments[0].is_empty()
        || segments[1].is_empty()
        || segments[2] != "releases"
        || !(segments[3] == "download" || (segments[3] == "latest" && segments[4] == "download"))
        || segments[4].is_empty()
        || segments[5].is_empty()
        || asset_name.is_empty()
        || asset_name.contains(['/', '\\'])
    {
        return None;
    }
    let owner = segments[0].to_string();
    let repo = segments[1].to_string();
    url.set_path(&format!("/{}/{}/releases/latest/download/", owner, repo));
    url.path_segments_mut()
        .ok()?
        .pop_if_empty()
        .push(asset_name);
    url.set_query(None);
    url.set_fragment(None);
    (url.as_str() != download_url).then(|| url.to_string())
}

async fn download_package(
    app_handle: &tauri::AppHandle,
    plugin: &MarketPlugin,
) -> Result<PathBuf, String> {
    let dir = market_dir(app_handle)?.join("downloads");
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("Failed to create plugin download directory: {}", e))?;
    let package_path = dir.join(&plugin.asset.name);
    let client = http_client()?;
    let mut response = client
        .get(&plugin.asset.download_url)
        .send()
        .await
        .map_err(|e| format!("Failed to download plugin package: {}", e))?;
    if matches!(response.status().as_u16(), 404 | 410) {
        if let Some(url) = latest_github_asset_url(&plugin.asset.download_url, &plugin.asset.name) {
            response = client
                .get(url)
                .send()
                .await
                .map_err(|e| format!("Failed to download relocated plugin package: {}", e))?;
        }
    }
    // A moved asset must still satisfy the index size, digest and manifest checks.
    let mut response = response
        .error_for_status()
        .map_err(|e| format!("Plugin package download failed: {}", e))?;

    let mut file = tokio::fs::File::create(&package_path)
        .await
        .map_err(|e| format!("Failed to create plugin package file: {}", e))?;
    let mut hasher = Sha256::new();
    let mut total = 0u64;

    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|e| format!("Failed to read plugin package download: {}", e))?
    {
        total += chunk.len() as u64;
        if total > plugin.asset.size || total > MAX_PLUGIN_PACKAGE_SIZE {
            let _ = tokio::fs::remove_file(&package_path).await;
            return Err("Downloaded plugin package is larger than expected".to_string());
        }
        hasher.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Failed to write plugin package: {}", e))?;
    }

    file.flush()
        .await
        .map_err(|e| format!("Failed to flush plugin package: {}", e))?;

    if total != plugin.asset.size {
        let _ = tokio::fs::remove_file(&package_path).await;
        return Err("Downloaded plugin package size does not match market index".to_string());
    }

    let digest = format!("{:x}", hasher.finalize());
    if digest != plugin.asset.sha256 {
        let _ = tokio::fs::remove_file(&package_path).await;
        return Err("Downloaded plugin package sha256 does not match market index".to_string());
    }

    Ok(package_path)
}

fn staging_dir(app_handle: &tauri::AppHandle, plugin_id: &str) -> Result<PathBuf, String> {
    Ok(market_dir(app_handle)?.join(format!(
        ".staging-{}-{}",
        plugin_id,
        Utc::now().timestamp_millis()
    )))
}

async fn unpack_and_validate_package(
    package_path: PathBuf,
    staging_dir: PathBuf,
    plugin: &MarketPlugin,
) -> Result<PluginManifest, String> {
    let expected_id = plugin.id.clone();
    let expected_version = plugin.version.clone();
    tauri::async_runtime::spawn_blocking(move || {
        unpack_zip(package_path, staging_dir.clone())?;
        let manifest_path = staging_dir.join("manifest.json");
        let manifest_json = std::fs::read_to_string(&manifest_path)
            .map_err(|e| format!("Failed to read plugin manifest from package: {}", e))?;
        let manifest = PluginManifest::from_json(&manifest_json)
            .map_err(|e| format!("Failed to parse plugin manifest from package: {}", e))?;
        manifest
            .validate()
            .map_err(|e| format!("Invalid plugin manifest in package: {}", e))?;

        if manifest.id != expected_id || manifest.version != expected_version {
            return Err("Plugin package manifest does not match market index".to_string());
        }

        let main_path = Path::new(&manifest.main);
        let main_file = staging_dir.join(main_path);
        ensure_child_path(&staging_dir, &main_file)?;
        if !main_file.is_file() {
            return Err("Plugin package does not contain the manifest main file".to_string());
        }

        Ok(manifest)
    })
    .await
    .map_err(|e| format!("Plugin package validation task failed: {}", e))?
}

fn unpack_zip(package_path: PathBuf, staging_dir: PathBuf) -> Result<(), String> {
    let file = std::fs::File::open(&package_path)
        .map_err(|e| format!("Failed to open plugin package: {}", e))?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Failed to read plugin package: {}", e))?;

    for index in 0..archive.len() {
        let mut file = archive
            .by_index(index)
            .map_err(|e| format!("Failed to read plugin package entry: {}", e))?;
        let enclosed = file
            .enclosed_name()
            .ok_or_else(|| "Plugin package contains an unsafe path".to_string())?
            .to_path_buf();
        validate_relative_path(&enclosed)?;
        let output_path = staging_dir.join(&enclosed);
        ensure_child_path(&staging_dir, &output_path)?;

        if file.is_dir() {
            std::fs::create_dir_all(&output_path)
                .map_err(|e| format!("Failed to create plugin package directory: {}", e))?;
            continue;
        }

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create plugin package parent directory: {}", e))?;
        }
        let mut output = std::fs::File::create(&output_path)
            .map_err(|e| format!("Failed to create plugin package file: {}", e))?;
        std::io::copy(&mut file, &mut output)
            .map_err(|e| format!("Failed to extract plugin package file: {}", e))?;
    }

    Ok(())
}

fn validate_relative_path(path: &Path) -> Result<(), String> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
        return Err("Plugin package entry escapes the plugin directory".to_string());
    }
    Ok(())
}

fn ensure_child_path(parent: &Path, child: &Path) -> Result<(), String> {
    let parent = normalize_path(parent);
    let child = normalize_path(child);
    if child.starts_with(&parent) {
        Ok(())
    } else {
        Err("Plugin filesystem path escapes the plugin directory".to_string())
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

async fn remove_dir_if_exists(path: &Path) -> Result<(), String> {
    match tokio::fs::remove_dir_all(path).await {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("Failed to remove directory {:?}: {}", path, error)),
    }
}

#[tauri::command]
pub async fn plugin_market_get_sources() -> Result<Vec<PluginMarketSource>, String> {
    get_sources().await
}

#[tauri::command]
pub async fn plugin_market_refresh(
    app_handle: tauri::AppHandle,
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<MarketRefreshResult, String> {
    refresh_market(app_handle, registry.inner().clone()).await
}

#[tauri::command]
pub async fn plugin_market_get_plugins(
    app_handle: tauri::AppHandle,
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
) -> Result<MarketRefreshResult, String> {
    get_market_plugins(app_handle, registry.inner().clone()).await
}

#[tauri::command]
pub async fn plugin_market_install(
    app_handle: tauri::AppHandle,
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    request: InstallPluginRequest,
) -> Result<InstallPluginResult, String> {
    install_market_plugin(app_handle, registry.inner().clone(), request).await
}

#[tauri::command]
pub async fn plugin_market_uninstall(
    app_handle: tauri::AppHandle,
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    plugin_id: String,
) -> Result<(), String> {
    uninstall_market_plugin(app_handle, registry.inner().clone(), plugin_id).await
}

#[tauri::command]
pub async fn plugin_market_get_install_status(
    app_handle: tauri::AppHandle,
    registry: tauri::State<'_, Arc<RwLock<PluginRegistry>>>,
    plugin_id: String,
) -> Result<MarketInstallStatus, String> {
    get_install_status(app_handle, registry.inner().clone(), plugin_id).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_moved_release_using_current_asset_name() {
        assert_eq!(
            latest_github_asset_url(
                "https://github.com/Cliporax/cliporax-plugin-market/releases/download/v0.1.0/old.zip",
                "com.example.plugin-2.0.0.cliporax-plugin.zip",
            ),
            Some("https://github.com/Cliporax/cliporax-plugin-market/releases/latest/download/com.example.plugin-2.0.0.cliporax-plugin.zip".to_string())
        );
    }

    #[test]
    fn resolves_renamed_latest_asset_and_encodes_filename() {
        assert_eq!(
            latest_github_asset_url(
                "https://github.com/owner/repo/releases/latest/download/old.zip?raw=true#asset",
                "new package.zip",
            ),
            Some(
                "https://github.com/owner/repo/releases/latest/download/new%20package.zip"
                    .to_string()
            )
        );
        assert_eq!(
            latest_github_asset_url(
                "https://github.com/owner/repo/releases/download/plugins%2Fv1.0.0/old.zip",
                "new.zip",
            ),
            Some("https://github.com/owner/repo/releases/latest/download/new.zip".to_string())
        );
    }

    #[test]
    fn does_not_rewrite_other_hosts_or_non_release_urls() {
        for url in [
            "https://example.com/owner/repo/releases/download/v1/old.zip",
            "https://github.com.example.com/owner/repo/releases/download/v1/old.zip",
            "http://github.com/owner/repo/releases/download/v1/old.zip",
            "https://github.com/owner/repo/archive/refs/tags/v1.zip",
            "https://github.com/owner/repo/releases/download/v1/",
            "https://github.com/owner/repo/releases/download//old.zip",
            "https://github.com/owner/repo/releases/latest/download/new.zip",
            "not a URL",
        ] {
            assert_eq!(latest_github_asset_url(url, "new.zip"), None, "{url}");
        }
    }

    #[test]
    fn does_not_resolve_asset_names_containing_paths() {
        for name in ["", "../plugin.zip", "dir/plugin.zip", "dir\\plugin.zip"] {
            assert_eq!(
                latest_github_asset_url(
                    "https://github.com/owner/repo/releases/download/v1/old.zip",
                    name,
                ),
                None,
            );
        }
    }

    #[test]
    fn rejects_unsafe_relative_paths() {
        assert!(validate_relative_path(Path::new("manifest.json")).is_ok());
        assert!(validate_relative_path(Path::new("assets/icon.svg")).is_ok());
        assert!(validate_relative_path(Path::new("../manifest.json")).is_err());
        assert!(validate_relative_path(Path::new("/tmp/manifest.json")).is_err());
    }

    #[test]
    fn computes_market_status_from_versions() {
        let mut plugin = MarketPlugin {
            id: "com.example.plugin".to_string(),
            name: "Example".to_string(),
            description: String::new(),
            version: "1.1.0".to_string(),
            author: "Example".to_string(),
            license: "MIT".to_string(),
            homepage: "https://example.com".to_string(),
            repository: "https://example.com".to_string(),
            keywords: vec![],
            plugin_type: "utility".to_string(),
            permissions: vec![],
            min_app_version: "0.1.0".to_string(),
            compatibility: MarketCompatibility {
                platforms: vec![current_platform().to_string()],
            },
            icon: MarketPluginIcon {
                path: "assets/icon.svg".to_string(),
                content_type: "image/svg+xml".to_string(),
                size: 1,
                sha256: "a".repeat(64),
                data_url: "data:image/svg+xml;base64,AA==".to_string(),
            },
            publisher: MarketPublisher {
                name: "Example".to_string(),
                url: "https://example.com".to_string(),
                official: false,
            },
            asset: MarketPluginAsset {
                name: "com.example.plugin-1.1.0.cliporax-plugin.zip".to_string(),
                download_url: "https://example.com/plugin.zip".to_string(),
                api_url: "https://example.com/plugin.zip".to_string(),
                size: 1,
                sha256: "a".repeat(64),
                github_asset_id: None,
                content_type: "application/zip".to_string(),
            },
            installed: Some(InstalledPluginVersion {
                version: "1.0.0".to_string(),
                state: crate::plugin::lifecycle::state::PluginState::Discovered,
                is_builtin: false,
            }),
            status: MarketInstallStatus::NotInstalled,
        };

        plugin.status = if let Some(installed) = &plugin.installed {
            match (
                Version::parse(&installed.version),
                Version::parse(&plugin.version),
            ) {
                (Ok(local), Ok(remote)) if remote > local => MarketInstallStatus::UpdateAvailable,
                _ => MarketInstallStatus::Installed,
            }
        } else {
            MarketInstallStatus::NotInstalled
        };

        assert_eq!(plugin.status, MarketInstallStatus::UpdateAvailable);
    }

    #[test]
    fn parses_market_index_without_runtime_status() -> Result<(), String> {
        let json = r#"{
          "schemaVersion": 1,
          "generatedAt": "2026-07-01T00:00:00Z",
          "marketVersion": "test",
          "plugins": [{
            "id": "com.example.plugin",
            "name": "Example",
            "description": "Example plugin",
            "version": "1.0.0",
            "author": "Example",
            "license": "MIT",
            "homepage": "https://example.com",
            "repository": "https://example.com",
            "keywords": [],
            "type": "utility",
            "permissions": [],
            "minAppVersion": "0.1.0",
            "compatibility": { "platforms": ["linux", "macos", "windows"] },
            "icon": {
              "path": "assets/icon.svg",
              "contentType": "image/svg+xml",
              "size": 1,
              "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
              "dataUrl": "data:image/svg+xml;base64,AA=="
            },
            "publisher": {
              "name": "Example",
              "url": "https://example.com",
              "official": false
            },
            "asset": {
              "name": "com.example.plugin-1.0.0.cliporax-plugin.zip",
              "downloadUrl": "https://example.com/plugin.zip",
              "apiUrl": "https://example.com/plugin.zip",
              "size": 1,
              "sha256": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
              "githubAssetId": null,
              "contentType": "application/zip"
            }
          }]
        }"#;

        let index = parse_market_index(json.as_bytes())?;

        assert_eq!(index.plugins.len(), 1);
        assert_eq!(index.plugins[0].status, MarketInstallStatus::NotInstalled);
        Ok(())
    }
}
