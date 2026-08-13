use serde::{Deserialize, Serialize};

pub(in crate::frpc) const DEFAULT_BINARY_SOURCE: &str = "auto";
pub(in crate::frpc) const DEFAULT_LOCAL_IP: &str = "127.0.0.1";
pub(in crate::frpc) const DEFAULT_LOCAL_PORT: u16 = 11111;
pub(in crate::frpc) const DEFAULT_REMOTE_PORT: u16 = 11111;
pub(in crate::frpc) const DEFAULT_FRPS_BIND_ADDR: &str = "0.0.0.0";
pub(in crate::frpc) const DEFAULT_FRPS_BIND_PORT: u16 = 7000;
pub(in crate::frpc) const DEFAULT_WEB_SERVER_ADDR: &str = "127.0.0.1";
pub(in crate::frpc) const DEFAULT_WEB_SERVER_PORT: u16 = 17400;
pub(in crate::frpc) const DEFAULT_FRPS_WEB_SERVER_PORT: u16 = 17500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FrpComponent {
    Frpc,
    Frps,
}

impl FrpComponent {
    pub(in crate::frpc) fn executable_name(self) -> &'static str {
        match self {
            Self::Frpc => {
                if cfg!(windows) {
                    "frpc.exe"
                } else {
                    "frpc"
                }
            }
            Self::Frps => {
                if cfg!(windows) {
                    "frps.exe"
                } else {
                    "frps"
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct FrpPlatform {
    pub os: String,
    pub arch: String,
    pub archive_ext: String,
}

#[derive(Debug, Serialize)]
pub struct FrpDownloadResponse {
    pub component: FrpComponent,
    pub version: String,
    pub platform: FrpPlatform,
    pub asset_name: String,
    pub download_url: String,
    pub binary_path: String,
}

#[derive(Debug, Deserialize)]
pub(in crate::frpc) struct GithubRelease {
    pub(in crate::frpc) tag_name: String,
    pub(in crate::frpc) assets: Vec<GithubReleaseAsset>,
}

#[derive(Debug, Deserialize)]
pub(in crate::frpc) struct GithubReleaseAsset {
    pub(in crate::frpc) name: String,
    pub(in crate::frpc) browser_download_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrpcConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_binary_source")]
    pub binary_source: String,
    #[serde(default)]
    pub binary_path: String,
    #[serde(default)]
    pub external_config_path: String,
    #[serde(default)]
    pub server_addr: String,
    #[serde(default = "default_server_port")]
    pub server_port: u16,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub tls_enable: bool,
    #[serde(default = "default_web_server_addr")]
    pub web_server_addr: String,
    #[serde(default = "default_web_server_port")]
    pub web_server_port: u16,
    #[serde(default)]
    pub proxies: Vec<FrpcProxyConfig>,
    #[serde(default)]
    pub extra_toml: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrpcProxyConfig {
    #[serde(default = "default_proxy_name")]
    pub name: String,
    #[serde(default = "default_proxy_type")]
    pub proxy_type: String,
    #[serde(default = "default_local_ip")]
    pub local_ip: String,
    #[serde(default = "default_local_port")]
    pub local_port: u16,
    #[serde(default = "default_remote_port")]
    pub remote_port: u16,
    #[serde(default)]
    pub custom_domains: String,
}

#[derive(Debug, Serialize)]
pub struct FrpcStatusResponse {
    pub configured: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub started_at: Option<u64>,
    pub binary_path: Option<String>,
    pub config_path: String,
    pub generated_config_path: String,
    pub log_path: String,
    pub last_error: Option<String>,
    pub config: FrpcConfig,
    pub log_tail: String,
    pub download_platform: Option<FrpPlatform>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrpsConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_binary_source")]
    pub binary_source: String,
    #[serde(default)]
    pub binary_path: String,
    #[serde(default)]
    pub external_config_path: String,
    #[serde(default = "default_frps_bind_addr")]
    pub bind_addr: String,
    #[serde(default = "default_frps_bind_port")]
    pub bind_port: u16,
    #[serde(default)]
    pub public_addr: String,
    #[serde(default)]
    pub token: String,
    #[serde(default = "default_web_server_addr")]
    pub web_server_addr: String,
    #[serde(default = "default_frps_web_server_port")]
    pub web_server_port: u16,
    #[serde(default)]
    pub dashboard_user: String,
    #[serde(default)]
    pub dashboard_password: String,
    #[serde(default)]
    pub extra_toml: String,
}

#[derive(Debug, Serialize)]
pub struct FrpsStatusResponse {
    pub configured: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub started_at: Option<u64>,
    pub binary_path: Option<String>,
    pub config_path: String,
    pub generated_config_path: String,
    pub log_path: String,
    pub last_error: Option<String>,
    pub config: FrpsConfig,
    pub log_tail: String,
    pub download_platform: Option<FrpPlatform>,
}

#[derive(Debug, Deserialize)]
pub struct SaveFrpcConfigRequest {
    pub config: FrpcConfig,
}

#[derive(Debug, Deserialize)]
pub struct SaveFrpsConfigRequest {
    pub config: FrpsConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrpRole {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub component: FrpComponent,
    #[serde(default)]
    pub frpc: Option<FrpcConfig>,
    #[serde(default)]
    pub frps: Option<FrpsConfig>,
}

#[derive(Debug, Serialize)]
pub struct FrpRoleStatus {
    pub role: FrpRole,
    pub configured: bool,
    pub running: bool,
    pub pid: Option<u32>,
    pub started_at: Option<u64>,
    pub binary_path: Option<String>,
    pub generated_config_path: String,
    pub log_path: String,
    pub last_error: Option<String>,
    pub log_tail: String,
}

#[derive(Debug, Serialize)]
pub struct FrpRolesResponse {
    pub roles: Vec<FrpRoleStatus>,
    pub download_platform: Option<FrpPlatform>,
}

#[derive(Debug, Deserialize)]
pub struct SaveFrpRoleRequest {
    pub role: FrpRole,
}

#[derive(Debug, Clone, Serialize)]
pub struct FrpSystemEntry {
    pub id: String,
    pub component: FrpComponent,
    pub source: String,
    pub pid: Option<u32>,
    pub binary_path: String,
    pub config_path: Option<String>,
    pub command: String,
    pub managed_role_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FrpSystemDiscoveryResponse {
    pub items: Vec<FrpSystemEntry>,
}

#[derive(Debug, Deserialize)]
pub struct AdoptFrpSystemRequest {
    pub component: FrpComponent,
    #[serde(default)]
    pub binary_path: String,
    #[serde(default)]
    pub config_path: String,
    #[serde(default)]
    pub role_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub public_addr: String,
}

#[derive(Debug, Deserialize)]
pub struct FrpPortTestRequest {
    #[serde(default)]
    pub host: String,
    pub port: u16,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct FrpPortTestResponse {
    pub ok: bool,
    pub target: String,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

impl Default for FrpcConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            binary_source: default_binary_source(),
            binary_path: String::new(),
            external_config_path: String::new(),
            server_addr: String::new(),
            server_port: default_server_port(),
            token: String::new(),
            tls_enable: false,
            web_server_addr: default_web_server_addr(),
            web_server_port: default_web_server_port(),
            proxies: vec![FrpcProxyConfig::default()],
            extra_toml: String::new(),
        }
    }
}

impl FrpcConfig {
    pub(in crate::frpc) fn default_for_local_port(local_port: u16) -> Self {
        let mut config = Self::default();
        if let Some(proxy) = config.proxies.first_mut() {
            proxy.local_port = local_port;
            proxy.remote_port = local_port;
        }
        config
    }
}

impl Default for FrpcProxyConfig {
    fn default() -> Self {
        Self {
            name: default_proxy_name(),
            proxy_type: default_proxy_type(),
            local_ip: default_local_ip(),
            local_port: default_local_port(),
            remote_port: default_remote_port(),
            custom_domains: String::new(),
        }
    }
}

impl Default for FrpsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            binary_source: default_binary_source(),
            binary_path: String::new(),
            external_config_path: String::new(),
            bind_addr: default_frps_bind_addr(),
            bind_port: default_frps_bind_port(),
            public_addr: String::new(),
            token: String::new(),
            web_server_addr: default_web_server_addr(),
            web_server_port: default_frps_web_server_port(),
            dashboard_user: String::new(),
            dashboard_password: String::new(),
            extra_toml: String::new(),
        }
    }
}

fn default_enabled() -> bool {
    true
}

fn default_binary_source() -> String {
    DEFAULT_BINARY_SOURCE.to_string()
}

pub(in crate::frpc) fn normalize_binary_source(value: &str) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "bundled" | "system" | "custom" => value.trim().to_ascii_lowercase(),
        _ => DEFAULT_BINARY_SOURCE.to_string(),
    }
}

fn default_server_port() -> u16 {
    7000
}

fn default_frps_bind_addr() -> String {
    DEFAULT_FRPS_BIND_ADDR.to_string()
}

fn default_frps_bind_port() -> u16 {
    DEFAULT_FRPS_BIND_PORT
}

fn default_web_server_addr() -> String {
    DEFAULT_WEB_SERVER_ADDR.to_string()
}

fn default_web_server_port() -> u16 {
    DEFAULT_WEB_SERVER_PORT
}

fn default_frps_web_server_port() -> u16 {
    DEFAULT_FRPS_WEB_SERVER_PORT
}

fn default_proxy_name() -> String {
    "webclx".to_string()
}

fn default_proxy_type() -> String {
    "tcp".to_string()
}

fn default_local_ip() -> String {
    DEFAULT_LOCAL_IP.to_string()
}

fn default_local_port() -> u16 {
    DEFAULT_LOCAL_PORT
}

fn default_remote_port() -> u16 {
    DEFAULT_REMOTE_PORT
}
