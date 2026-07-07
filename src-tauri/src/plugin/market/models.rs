use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMarketSource {
    pub id: String,
    pub name: String,
    pub release_api_url: String,
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketIndex {
    pub schema_version: u32,
    pub generated_at: String,
    pub market_version: String,
    pub plugins: Vec<MarketPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketPlugin {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
    pub author: String,
    pub license: String,
    pub homepage: String,
    pub repository: String,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(rename = "type")]
    pub plugin_type: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub min_app_version: String,
    #[serde(default)]
    pub compatibility: MarketCompatibility,
    pub icon: MarketPluginIcon,
    pub publisher: MarketPublisher,
    pub asset: MarketPluginAsset,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed: Option<InstalledPluginVersion>,
    #[serde(default)]
    pub status: MarketInstallStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketCompatibility {
    #[serde(default)]
    pub platforms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketPluginIcon {
    pub path: String,
    pub content_type: String,
    pub size: u64,
    pub sha256: String,
    pub data_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketPublisher {
    pub name: String,
    pub url: String,
    pub official: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketPluginAsset {
    pub name: String,
    pub download_url: String,
    pub api_url: String,
    pub size: u64,
    pub sha256: String,
    pub github_asset_id: Option<u64>,
    pub content_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPluginVersion {
    pub version: String,
    pub state: crate::plugin::lifecycle::state::PluginState,
    pub is_builtin: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum MarketInstallStatus {
    #[default]
    NotInstalled,
    Installed,
    UpdateAvailable,
    Incompatible,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPluginRequest {
    pub plugin_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPluginResult {
    pub plugin_id: String,
    pub version: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketRefreshResult {
    pub source_id: String,
    pub stale: bool,
    pub plugins: Vec<MarketPlugin>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct GithubRelease {
    pub assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct GithubReleaseAsset {
    pub name: String,
    pub url: String,
    pub browser_download_url: String,
    pub size: u64,
}
