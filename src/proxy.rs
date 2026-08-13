use std::{
    collections::HashSet,
    net::SocketAddr,
    process::Stdio,
    sync::{Arc, RwLock},
    time::Duration,
};

use anyhow::{Context, Result};
use axum::{
    Json,
    extract::{Path, State},
};
use reqwest::{
    Proxy,
    header::{
        ACCEPT, ACCEPT_LANGUAGE, CACHE_CONTROL, HeaderMap, HeaderValue, PRAGMA,
        UPGRADE_INSECURE_REQUESTS, USER_AGENT,
    },
};
use serde::{Deserialize, Serialize};
use tokio::{net::TcpStream, process::Command, time::timeout};
use tracing::{info, warn};

use crate::{ApiResult, AppError, AppState};

const PROXY_FILE_NAME: &str = "webclx-proxy-presets.json";
const TEST_URL: &str = "https://httpbin.org/ip";
const TEST_TIMEOUT_SECS: u64 = 15;
const CODEX_EXEC_TIMEOUT_SECS: u64 = 60;
const DEFAULT_CODEX_PROMPT: &str = "hi";
const BROWSER_LIKE_USER_AGENT: &str = concat!(
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) ",
    "AppleWebKit/537.36 (KHTML, like Gecko) ",
    "Chrome/135.0.0.0 Safari/537.36"
);
const APP_PROXY_ENV_KEYS: [&str; 6] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
];
const EXTRA_PROXY_ENV_KEYS: [&str; 2] = ["NO_PROXY", "no_proxy"];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ProxyType {
    #[default]
    Http,
    Https,
    Socks5,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProxyPreset {
    pub id: String,
    pub name: String,
    pub proxy_type: ProxyType,
    pub server: String,
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
}

impl ProxyPreset {
    fn new(id: String, name: String, proxy_type: ProxyType, server: String) -> Self {
        Self {
            id,
            name,
            proxy_type,
            server,
            enabled: true,
            username: None,
            password: None,
        }
    }

    pub fn public_view(&self) -> ProxyPresetView {
        ProxyPresetView {
            id: self.id.clone(),
            name: self.name.clone(),
            proxy_type: self.proxy_type.clone(),
            server: self.server.clone(),
            enabled: self.enabled,
            username: self.username.clone(),
            has_password: self.password.is_some(),
        }
    }

    pub fn network_summary(&self) -> String {
        let auth = if self.username.is_some() && self.password.is_some() {
            "已认证"
        } else {
            "无认证"
        };
        format!(
            "{}（{}://{}，{}）",
            self.name,
            proxy_type_label(&self.proxy_type),
            self.server,
            auth
        )
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct ProxyPresetView {
    pub id: String,
    pub name: String,
    pub proxy_type: ProxyType,
    pub server: String,
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    pub has_password: bool,
}

#[derive(Clone)]
pub struct ProxyManager {
    presets: Arc<RwLock<Vec<ProxyPreset>>>,
    active_proxy_id: Arc<RwLock<Option<String>>>,
    config_path: Arc<std::path::PathBuf>,
    https_proxy_bridge_addr: Arc<RwLock<Option<SocketAddr>>>,
}

impl ProxyManager {
    pub fn load(app_dir: &std::path::Path) -> Result<Self> {
        let config_path = if cfg!(windows) {
            app_dir.join("config").join(PROXY_FILE_NAME)
        } else {
            app_dir.join(PROXY_FILE_NAME)
        };
        let (presets, active_proxy_id) = load_proxy_config(&config_path).unwrap_or_else(|error| {
            warn!("load proxy config failed: {error}");
            (Vec::new(), None)
        });

        Ok(Self {
            presets: Arc::new(RwLock::new(presets)),
            active_proxy_id: Arc::new(RwLock::new(active_proxy_id)),
            config_path: Arc::new(config_path),
            https_proxy_bridge_addr: Arc::new(RwLock::new(None)),
        })
    }

    pub fn list(&self) -> Vec<ProxyPreset> {
        crate::lock_or_recover!(self.presets.read()).clone()
    }

    pub fn get(&self, id: &str) -> Option<ProxyPreset> {
        crate::lock_or_recover!(self.presets.read())
            .iter()
            .find(|preset| preset.id == id)
            .cloned()
    }

    pub fn save(&self, preset: ProxyPreset) -> Result<()> {
        validate_proxy_preset(&preset)?;
        {
            let mut presets = crate::lock_or_recover!(self.presets.write());
            if let Some(index) = presets.iter().position(|existing| existing.id == preset.id) {
                presets[index] = preset;
            } else {
                presets.insert(0, preset);
            }
        }
        self.persist()
    }

    pub fn delete(&self, id: &str) -> Result<()> {
        {
            let mut presets = crate::lock_or_recover!(self.presets.write());
            presets.retain(|preset| preset.id != id);
        }
        {
            let mut active = crate::lock_or_recover!(self.active_proxy_id.write());
            if *active == Some(id.to_string()) {
                *active = None;
            }
        }
        self.persist()
    }

    pub fn get_active(&self) -> Option<ProxyPreset> {
        let id = crate::lock_or_recover!(self.active_proxy_id.read()).clone()?;
        self.get(&id)
    }

    pub fn active_id(&self) -> Option<String> {
        crate::lock_or_recover!(self.active_proxy_id.read()).clone()
    }

    pub fn replace_all(&self, presets: Vec<ProxyPreset>, active_id: Option<String>) -> Result<()> {
        let valid_active_id = active_id.filter(|id| presets.iter().any(|preset| preset.id == *id));
        {
            let mut stored = crate::lock_or_recover!(self.presets.write());
            *stored = presets;
        }
        {
            let mut active = crate::lock_or_recover!(self.active_proxy_id.write());
            *active = valid_active_id;
        }
        self.persist()
    }

    pub fn reorder(&self, ids: Vec<String>) -> Result<()> {
        {
            let mut presets = crate::lock_or_recover!(self.presets.write());
            if ids.len() != presets.len() {
                anyhow::bail!("代理预设排序列表必须包含当前全部预设");
            }

            let mut seen = HashSet::with_capacity(ids.len());
            for id in &ids {
                if id.trim().is_empty() || !seen.insert(id.as_str()) {
                    anyhow::bail!("代理预设排序列表包含空 id 或重复 id");
                }
            }

            let mut reordered = Vec::with_capacity(presets.len());
            for id in &ids {
                let Some(index) = presets.iter().position(|preset| preset.id == *id) else {
                    anyhow::bail!("代理预设排序列表包含未知 id");
                };
                reordered.push(presets.remove(index));
            }

            if !presets.is_empty() {
                anyhow::bail!("代理预设排序列表必须包含当前全部预设");
            }

            *presets = reordered;
        }
        self.persist()
    }

    pub fn set_active(&self, id: &str) -> Result<()> {
        if self.get(id).is_none() {
            anyhow::bail!("预设不存在");
        }
        {
            let mut active = crate::lock_or_recover!(self.active_proxy_id.write());
            *active = Some(id.to_string());
        }
        self.persist()
    }

    pub fn clear_active(&self) -> Result<()> {
        {
            let mut active = crate::lock_or_recover!(self.active_proxy_id.write());
            *active = None;
        }
        self.persist()
    }

    pub fn get_proxy_env(&self) -> Vec<(String, String)> {
        let Some(proxy) = self.get_active() else {
            return Vec::new();
        };
        build_proxy_env(
            &proxy.proxy_type,
            &proxy.server,
            proxy.username.as_deref(),
            proxy.password.as_deref(),
        )
    }

    pub fn set_https_proxy_bridge_addr(&self, addr: SocketAddr) {
        *crate::lock_or_recover!(self.https_proxy_bridge_addr.write()) = Some(addr);
    }

    pub fn get_terminal_proxy_env(&self) -> Vec<(String, String)> {
        let Some(proxy) = self.get_active() else {
            return Vec::new();
        };
        let bridge_addr = *crate::lock_or_recover!(self.https_proxy_bridge_addr.read());
        crate::proxy_bridge::build_terminal_proxy_env(&proxy, bridge_addr)
    }

    pub fn inherited_proxy_env(&self) -> Vec<(String, String)> {
        APP_PROXY_ENV_KEYS
            .iter()
            .filter_map(|key| {
                std::env::var(key)
                    .ok()
                    .map(|value| ((*key).to_string(), value))
            })
            .collect()
    }

    pub fn apply_to_client_builder(
        &self,
        builder: reqwest::ClientBuilder,
    ) -> Result<reqwest::ClientBuilder> {
        let builder = builder.no_proxy();
        match self.get_active() {
            Some(proxy) => {
                let reqwest_proxy = build_reqwest_proxy(
                    &proxy.proxy_type,
                    &proxy.server,
                    proxy.username.as_deref(),
                    proxy.password.as_deref(),
                )?;
                Ok(builder.proxy(reqwest_proxy))
            }
            None => Ok(builder),
        }
    }

    pub fn build_app_client(&self, timeout_secs: u64) -> Result<reqwest::Client> {
        self.apply_to_client_builder(
            reqwest::Client::builder().timeout(Duration::from_secs(timeout_secs)),
        )?
        .build()
        .context("创建程序代理 HTTP 客户端失败")
    }

    /// Builds an HTTP client for Codex OAuth operations (device login, token
    /// refresh, usage fetch). These endpoints must always be accessed through
    /// a proxy, regardless of whether the webClx application proxy is active.
    ///
    /// When an application proxy preset is active it is used directly and
    /// shell proxy environment variables are stripped to avoid double-proxying.
    /// When no application proxy is active, the builder does NOT call
    /// `.no_proxy()`, so reqwest inherits any `HTTP_PROXY` / `HTTPS_PROXY` /
    /// `ALL_PROXY` environment variables.
    pub fn build_oauth_client(&self, timeout_secs: u64) -> Result<reqwest::Client> {
        let builder = reqwest::Client::builder().timeout(Duration::from_secs(timeout_secs));
        match self.get_active() {
            Some(proxy) => {
                let reqwest_proxy = build_reqwest_proxy(
                    &proxy.proxy_type,
                    &proxy.server,
                    proxy.username.as_deref(),
                    proxy.password.as_deref(),
                )?;
                builder
                    .no_proxy()
                    .proxy(reqwest_proxy)
                    .build()
                    .context("创建 Codex OAuth 代理客户端失败")
            }
            None => builder.build().context("创建 Codex OAuth 代理客户端失败"),
        }
    }

    fn persist(&self) -> Result<()> {
        let presets = crate::lock_or_recover!(self.presets.read()).clone();
        let active_id = crate::lock_or_recover!(self.active_proxy_id.read()).clone();
        let config = ProxyConfig {
            presets,
            active_proxy_id: active_id,
        };
        let encoded = serde_json::to_vec_pretty(&config)?;
        std::fs::write(self.config_path.as_path(), encoded)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                self.config_path.as_path(),
                std::fs::Permissions::from_mode(0o600),
            )?;
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct ProxyConfig {
    presets: Vec<ProxyPreset>,
    #[serde(skip_serializing_if = "Option::is_none")]
    active_proxy_id: Option<String>,
}

fn load_proxy_config(path: &std::path::Path) -> Result<(Vec<ProxyPreset>, Option<String>)> {
    if !path.exists() {
        return Ok((Vec::new(), None));
    }
    let content = std::fs::read_to_string(path)?;
    let content_trimmed = content.trim();

    // New format: { "presets": [...], "active_proxy_id": "..." }
    if content_trimmed.starts_with('{') {
        let config: ProxyConfig = serde_json::from_str(content_trimmed)?;
        return Ok((dedupe_proxy_presets(config.presets), config.active_proxy_id));
    }

    // Old format: [...]
    let presets: Vec<ProxyPreset> = serde_json::from_str(content_trimmed)?;
    Ok((dedupe_proxy_presets(presets), None))
}

fn dedupe_proxy_presets(presets: Vec<ProxyPreset>) -> Vec<ProxyPreset> {
    let mut seen = HashSet::with_capacity(presets.len());
    presets
        .into_iter()
        .filter(|preset| seen.insert(preset.id.clone()))
        .collect()
}

#[derive(Debug, Serialize)]
pub struct ProxyPresetResponse {
    pub presets: Vec<ProxyPresetView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaveProxyPresetRequest {
    pub name: String,
    pub proxy_type: ProxyType,
    pub server: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReorderProxyPresetsRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ReorderProxyPresetsResponse {
    pub ok: bool,
    pub ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ProxyTestResponse {
    pub test_mode: ProxyTestMode,
    pub ok: bool,
    pub proxy_connect_ok: bool,
    pub proxy_connect_elapsed_ms: Option<u64>,
    pub proxy_connect_error: Option<String>,
    pub target_access_ok: bool,
    pub status: Option<u16>,
    pub status_text: Option<String>,
    pub body: Option<String>,
    pub elapsed_ms: Option<u64>,
    pub error: Option<String>,
    pub proxy_url: String,
    pub test_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_display: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_last_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[derive(Default)]
pub enum ProxyTestMode {
    #[default]
    Http,
    CodexExec,
}

fn build_proxy_url(proxy_type: &ProxyType, server: &str) -> String {
    match proxy_type {
        ProxyType::Http => format!("http://{}", server),
        ProxyType::Https => format!("https://{}", server),
        ProxyType::Socks5 => format!("socks5://{}", server),
    }
}

fn proxy_type_label(proxy_type: &ProxyType) -> &'static str {
    match proxy_type {
        ProxyType::Http => "HTTP",
        ProxyType::Https => "HTTPS",
        ProxyType::Socks5 => "SOCKS5",
    }
}

fn normalize_proxy_credential(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn validate_proxy_credentials(username: Option<&str>, password: Option<&str>) -> Result<()> {
    if username.is_some() != password.is_some() {
        anyhow::bail!("代理用户名和密码必须同时填写");
    }
    Ok(())
}

fn validate_proxy_preset(preset: &ProxyPreset) -> Result<()> {
    parse_proxy_host_port(&preset.proxy_type, &preset.server)?;
    validate_proxy_credentials(preset.username.as_deref(), preset.password.as_deref())
}

fn apply_proxy_credentials(
    mut proxy: Proxy,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<Proxy> {
    validate_proxy_credentials(username, password)?;
    if let (Some(username), Some(password)) = (username, password) {
        proxy = proxy.basic_auth(username, password);
    }
    Ok(proxy)
}

fn build_authenticated_proxy_url(
    proxy_type: &ProxyType,
    server: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> String {
    let proxy_url = build_proxy_url(proxy_type, server);
    let (Some(username), Some(password)) = (username, password) else {
        return proxy_url;
    };
    let Ok(mut parsed) = reqwest::Url::parse(&proxy_url) else {
        return proxy_url;
    };
    if parsed.set_username(username).is_err() || parsed.set_password(Some(password)).is_err() {
        return proxy_url;
    }
    parsed.to_string().trim_end_matches('/').to_string()
}

fn redact_proxy_env_value(value: &str) -> String {
    let Ok(mut parsed) = reqwest::Url::parse(value) else {
        return value.to_string();
    };
    if parsed.password().is_none() {
        return value.to_string();
    }
    if parsed.set_password(Some("REDACTED")).is_err() {
        return value.to_string();
    }
    parsed.to_string().trim_end_matches('/').to_string()
}

fn normalize_test_url(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return TEST_URL.to_string();
    }
    if trimmed.contains("://") {
        return trimmed.to_string();
    }
    format!("https://{trimmed}")
}

fn browser_like_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(BROWSER_LIKE_USER_AGENT));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(
            "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
        ),
    );
    headers.insert(ACCEPT_LANGUAGE, HeaderValue::from_static("zh-CN,zh;q=0.9,en;q=0.8"));
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));
    headers.insert(UPGRADE_INSECURE_REQUESTS, HeaderValue::from_static("1"));
    headers.insert("sec-fetch-dest", HeaderValue::from_static("document"));
    headers.insert("sec-fetch-mode", HeaderValue::from_static("navigate"));
    headers.insert("sec-fetch-site", HeaderValue::from_static("none"));
    headers.insert("sec-fetch-user", HeaderValue::from_static("?1"));
    headers
}

pub async fn list_proxy_presets(
    State(state): State<AppState>,
) -> ApiResult<Json<ProxyPresetResponse>> {
    Ok(Json(ProxyPresetResponse {
        presets: state
            .proxy_manager
            .list()
            .iter()
            .map(ProxyPreset::public_view)
            .collect(),
        active_id: state.proxy_manager.active_id(),
    }))
}

pub async fn create_proxy_preset(
    State(state): State<AppState>,
    Json(payload): Json<SaveProxyPresetRequest>,
) -> ApiResult<Json<ProxyPresetView>> {
    let id = uuid_v4();
    let mut preset = ProxyPreset::new(id, payload.name, payload.proxy_type, payload.server);
    preset.username = normalize_proxy_credential(payload.username);
    preset.password = normalize_proxy_credential(payload.password);
    state
        .proxy_manager
        .save(preset.clone())
        .map_err(|error| AppError::bad_request(format!("保存失败: {error}")))?;
    Ok(Json(preset.public_view()))
}

pub async fn update_proxy_preset(
    State(state): State<AppState>,
    Path(preset_id): Path<String>,
    Json(payload): Json<SaveProxyPresetRequest>,
) -> ApiResult<Json<ProxyPresetView>> {
    let existing = state
        .proxy_manager
        .get(&preset_id)
        .ok_or_else(|| AppError::not_found(format!("预设 `{}` 不存在", preset_id)))?;
    let username = normalize_proxy_credential(payload.username);
    let submitted_password = normalize_proxy_credential(payload.password);
    let password = match username.as_deref() {
        None => None,
        Some(username) if existing.username.as_deref() == Some(username) => {
            submitted_password.or(existing.password)
        }
        Some(_) => submitted_password,
    };
    let preset = ProxyPreset {
        id: preset_id,
        name: payload.name,
        proxy_type: payload.proxy_type,
        server: payload.server,
        enabled: payload.enabled,
        username,
        password,
    };
    state
        .proxy_manager
        .save(preset.clone())
        .map_err(|error| AppError::bad_request(format!("保存失败: {error}")))?;
    Ok(Json(preset.public_view()))
}

pub async fn delete_proxy_preset(
    State(state): State<AppState>,
    Path(preset_id): Path<String>,
) -> ApiResult<Json<serde_json::Value>> {
    if state.proxy_manager.get(&preset_id).is_none() {
        return Err(AppError::not_found(format!("预设 `{}` 不存在", preset_id)));
    }
    state
        .proxy_manager
        .delete(&preset_id)
        .map_err(|error| AppError::internal(format!("删除失败: {error}")))?;
    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn reorder_proxy_presets(
    State(state): State<AppState>,
    Json(payload): Json<ReorderProxyPresetsRequest>,
) -> ApiResult<Json<ReorderProxyPresetsResponse>> {
    state
        .proxy_manager
        .reorder(payload.ids.clone())
        .map_err(|error| AppError::bad_request(format!("代理预设排序失败: {error}")))?;

    Ok(Json(ReorderProxyPresetsResponse {
        ok: true,
        ids: payload.ids,
    }))
}

pub async fn test_proxy(
    State(state): State<AppState>,
    Json(payload): Json<TestProxyRequest>,
) -> ApiResult<Json<ProxyTestResponse>> {
    let proxy = if let Some(preset_id) = payload.preset_id.as_deref() {
        state
            .proxy_manager
            .get(preset_id)
            .ok_or_else(|| AppError::not_found(format!("预设 `{preset_id}` 不存在")))?
    } else {
        let mut proxy = ProxyPreset::new(
            "test".to_string(),
            "test".to_string(),
            payload.proxy_type.clone(),
            payload.server.clone(),
        );
        proxy.username = normalize_proxy_credential(payload.username.clone());
        proxy.password = normalize_proxy_credential(payload.password.clone());
        proxy
    };
    validate_proxy_preset(&proxy)
        .map_err(|error| AppError::bad_request(format!("代理配置无效: {error}")))?;
    let proxy_url = build_proxy_url(&proxy.proxy_type, &proxy.server);
    info!("testing proxy: {}", proxy_url);
    let test_mode = payload.test_mode.clone();
    let test_url = match test_mode {
        ProxyTestMode::Http => normalize_test_url(&payload.test_url),
        ProxyTestMode::CodexExec => normalize_codex_prompt(&payload.codex_prompt),
    };

    let (proxy_host, proxy_port) = parse_proxy_host_port(&proxy.proxy_type, &proxy.server)
        .map_err(|error| AppError::bad_request(format!("代理地址无效: {error}")))?;

    let proxy_connect_start = std::time::Instant::now();
    match timeout(
        Duration::from_secs(TEST_TIMEOUT_SECS),
        TcpStream::connect((proxy_host.as_str(), proxy_port)),
    )
    .await
    {
        Ok(Ok(_stream)) => {}
        Ok(Err(error)) => {
            let elapsed_ms = proxy_connect_start.elapsed().as_millis() as u64;
            return Ok(Json(proxy_connect_failure_response(
                &test_mode,
                proxy_url,
                test_url,
                Some(format!("无法连接到代理服务器: {error}")),
                Some(elapsed_ms),
            )));
        }
        Err(_) => {
            let elapsed_ms = proxy_connect_start.elapsed().as_millis() as u64;
            return Ok(Json(proxy_connect_failure_response(
                &test_mode,
                proxy_url,
                test_url,
                Some(format!("连接代理服务器超时（{}秒）", TEST_TIMEOUT_SECS)),
                Some(elapsed_ms),
            )));
        }
    }

    match test_mode {
        ProxyTestMode::Http => Ok(Json(
            run_http_proxy_test(&proxy, proxy_url, test_url, proxy_connect_start).await,
        )),
        ProxyTestMode::CodexExec => Ok(Json(
            run_codex_exec_proxy_test(&payload, &proxy, proxy_url, test_url, proxy_connect_start)
                .await,
        )),
    }
}

async fn run_http_proxy_test(
    proxy: &ProxyPreset,
    proxy_url: String,
    test_url: String,
    proxy_connect_start: std::time::Instant,
) -> ProxyTestResponse {
    let proxy_client = build_reqwest_proxy(
        &proxy.proxy_type,
        &proxy.server,
        proxy.username.as_deref(),
        proxy.password.as_deref(),
    )
    .and_then(|reqwest_proxy| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(TEST_TIMEOUT_SECS))
            .default_headers(browser_like_headers())
            .no_proxy()
            .proxy(reqwest_proxy)
            .build()
            .context("创建 HTTP 代理测试客户端失败")
    });
    let client = match proxy_client {
        Ok(client) => client,
        Err(error) => {
            return ProxyTestResponse {
                test_mode: ProxyTestMode::Http,
                ok: false,
                proxy_connect_ok: true,
                proxy_connect_elapsed_ms: Some(proxy_connect_start.elapsed().as_millis() as u64),
                proxy_connect_error: None,
                target_access_ok: false,
                status: None,
                status_text: None,
                body: None,
                elapsed_ms: None,
                error: Some(error.to_string()),
                proxy_url,
                test_url,
                command_display: None,
                command_prompt: None,
                command_last_message: None,
                command_output: None,
                exit_code: None,
            };
        }
    };

    let start = std::time::Instant::now();
    match client.get(&test_url).send().await {
        Ok(response) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let status = response.status().as_u16();
            let status_text = response.status().canonical_reason().map(|s| s.to_string());

            let body_preview = response.text().await.ok().map(|text| {
                if text.len() > 500 {
                    format!("{}...(truncated {} bytes)", &text[..500], text.len() - 500)
                } else {
                    text
                }
            });

            ProxyTestResponse {
                test_mode: ProxyTestMode::Http,
                ok: true,
                proxy_connect_ok: true,
                proxy_connect_elapsed_ms: Some(proxy_connect_start.elapsed().as_millis() as u64),
                proxy_connect_error: None,
                target_access_ok: true,
                status: Some(status),
                status_text,
                body: body_preview,
                elapsed_ms: Some(elapsed_ms),
                error: None,
                proxy_url,
                test_url,
                command_display: None,
                command_prompt: None,
                command_last_message: None,
                command_output: None,
                exit_code: None,
            }
        }
        Err(error) => {
            let elapsed_ms = start.elapsed().as_millis() as u64;
            let error_msg = if error.is_timeout() {
                format!("连接超时（{}秒）", TEST_TIMEOUT_SECS)
            } else if error.is_connect() {
                "已连上代理服务器，但代理转发目标地址失败".to_string()
            } else {
                format!("请求失败: {}", error)
            };
            ProxyTestResponse {
                test_mode: ProxyTestMode::Http,
                ok: false,
                proxy_connect_ok: true,
                proxy_connect_elapsed_ms: Some(proxy_connect_start.elapsed().as_millis() as u64),
                proxy_connect_error: None,
                target_access_ok: false,
                status: None,
                status_text: None,
                body: None,
                elapsed_ms: Some(elapsed_ms),
                error: Some(error_msg),
                proxy_url,
                test_url,
                command_display: None,
                command_prompt: None,
                command_last_message: None,
                command_output: None,
                exit_code: None,
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TestProxyRequest {
    #[serde(default)]
    pub preset_id: Option<String>,
    pub proxy_type: ProxyType,
    pub server: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub test_mode: ProxyTestMode,
    #[serde(default)]
    pub test_url: String,
    #[serde(default)]
    pub codex_prompt: String,
}

#[derive(Serialize)]
pub struct ProxyActiveResponse {
    pub active: Option<ProxyPresetView>,
    pub effective_env: Vec<String>,
    pub inherited_proxy_env: Vec<String>,
    pub ignores_system_proxy_env: bool,
}

#[derive(Deserialize)]
pub struct ApplyProxyRequest {
    pub preset_id: String,
}

pub async fn get_active_proxy(
    State(state): State<AppState>,
) -> ApiResult<Json<ProxyActiveResponse>> {
    Ok(Json(proxy_active_response(&state.proxy_manager)))
}

pub async fn apply_proxy(
    State(state): State<AppState>,
    Json(payload): Json<ApplyProxyRequest>,
) -> ApiResult<Json<ProxyActiveResponse>> {
    state
        .proxy_manager
        .set_active(&payload.preset_id)
        .map_err(|error| AppError::internal(format!("应用失败: {error}")))?;
    info!("proxy applied: {}", payload.preset_id);
    Ok(Json(proxy_active_response(&state.proxy_manager)))
}

pub async fn clear_proxy(State(state): State<AppState>) -> ApiResult<Json<ProxyActiveResponse>> {
    state
        .proxy_manager
        .clear_active()
        .map_err(|error| AppError::internal(format!("清除失败: {error}")))?;
    info!("proxy cleared");
    Ok(Json(proxy_active_response(&state.proxy_manager)))
}

fn build_reqwest_proxy(
    proxy_type: &ProxyType,
    server: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> Result<Proxy> {
    let proxy_url = build_proxy_url(proxy_type, server);
    let proxy = Proxy::all(&proxy_url).with_context(|| format!("无效代理地址: {proxy_url}"))?;
    apply_proxy_credentials(proxy, username, password)
}

pub(crate) fn build_proxy_env(
    proxy_type: &ProxyType,
    server: &str,
    username: Option<&str>,
    password: Option<&str>,
) -> Vec<(String, String)> {
    let url = build_authenticated_proxy_url(proxy_type, server, username, password);
    vec![
        ("HTTP_PROXY".to_string(), url.clone()),
        ("HTTPS_PROXY".to_string(), url.clone()),
        ("ALL_PROXY".to_string(), url.clone()),
        ("http_proxy".to_string(), url.clone()),
        ("https_proxy".to_string(), url.clone()),
        ("all_proxy".to_string(), url),
    ]
}

fn normalize_codex_prompt(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        DEFAULT_CODEX_PROMPT.to_string()
    } else {
        trimmed.to_string()
    }
}

fn proxy_connect_failure_response(
    test_mode: &ProxyTestMode,
    proxy_url: String,
    test_url: String,
    proxy_connect_error: Option<String>,
    proxy_connect_elapsed_ms: Option<u64>,
) -> ProxyTestResponse {
    let error = match test_mode {
        ProxyTestMode::Http => "代理服务器连通性检查失败，未继续访问测试地址",
        ProxyTestMode::CodexExec => "代理服务器连通性检查失败，未继续运行 codex exec",
    };
    ProxyTestResponse {
        test_mode: test_mode.clone(),
        ok: false,
        proxy_connect_ok: false,
        proxy_connect_elapsed_ms,
        proxy_connect_error,
        target_access_ok: false,
        status: None,
        status_text: None,
        body: None,
        elapsed_ms: None,
        error: Some(error.to_string()),
        proxy_url,
        test_url,
        command_display: None,
        command_prompt: None,
        command_last_message: None,
        command_output: None,
        exit_code: None,
    }
}

async fn run_codex_exec_proxy_test(
    payload: &TestProxyRequest,
    proxy: &ProxyPreset,
    proxy_url: String,
    test_url: String,
    proxy_connect_start: std::time::Instant,
) -> ProxyTestResponse {
    let prompt = normalize_codex_prompt(&payload.codex_prompt);
    let workdir = std::env::current_dir()
        .ok()
        .unwrap_or_else(|| std::path::PathBuf::from("/"));
    let output_path = std::env::temp_dir().join(format!(
        "webclx-codex-proxy-test-{}-{}.txt",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let command_display = format!("codex exec --skip-git-repo-check {:?}", prompt);
    let mut command = Command::new("codex");
    command
        .kill_on_drop(true)
        .stdin(Stdio::null())
        .current_dir(&workdir)
        .arg("exec")
        .arg("--skip-git-repo-check")
        .arg("--color")
        .arg("never")
        .arg("--output-last-message")
        .arg(&output_path)
        .arg("--cd")
        .arg(&workdir)
        .arg(&prompt);

    for key in APP_PROXY_ENV_KEYS {
        command.env_remove(key);
    }
    for key in EXTRA_PROXY_ENV_KEYS {
        command.env_remove(key);
    }
    for key in auth_core::forbidden_config_home_env_keys() {
        command.env_remove(key);
    }
    for (key, value) in build_proxy_env(
        &proxy.proxy_type,
        &proxy.server,
        proxy.username.as_deref(),
        proxy.password.as_deref(),
    ) {
        command.env(key, value);
    }

    let start = std::time::Instant::now();
    let output = match timeout(Duration::from_secs(CODEX_EXEC_TIMEOUT_SECS), command.output()).await
    {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            let _ = std::fs::remove_file(&output_path);
            return ProxyTestResponse {
                test_mode: ProxyTestMode::CodexExec,
                ok: false,
                proxy_connect_ok: true,
                proxy_connect_elapsed_ms: Some(proxy_connect_start.elapsed().as_millis() as u64),
                proxy_connect_error: None,
                target_access_ok: false,
                status: None,
                status_text: None,
                body: None,
                elapsed_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("启动 codex exec 失败: {error}")),
                proxy_url,
                test_url,
                command_display: Some(command_display),
                command_prompt: Some(prompt),
                command_last_message: None,
                command_output: None,
                exit_code: None,
            };
        }
        Err(_) => {
            let _ = std::fs::remove_file(&output_path);
            return ProxyTestResponse {
                test_mode: ProxyTestMode::CodexExec,
                ok: false,
                proxy_connect_ok: true,
                proxy_connect_elapsed_ms: Some(proxy_connect_start.elapsed().as_millis() as u64),
                proxy_connect_error: None,
                target_access_ok: false,
                status: None,
                status_text: None,
                body: None,
                elapsed_ms: Some(start.elapsed().as_millis() as u64),
                error: Some(format!("运行 codex exec 超时（{}秒）", CODEX_EXEC_TIMEOUT_SECS)),
                proxy_url,
                test_url,
                command_display: Some(command_display),
                command_prompt: Some(prompt),
                command_last_message: None,
                command_output: None,
                exit_code: None,
            };
        }
    };

    let last_message = std::fs::read_to_string(&output_path)
        .ok()
        .map(|content| content.trim().to_string())
        .filter(|content| !content.is_empty());
    let _ = std::fs::remove_file(&output_path);

    let output_preview =
        truncate_for_display(combine_command_output(&output.stdout, &output.stderr), 4000);
    let elapsed_ms = start.elapsed().as_millis() as u64;
    let ok = output.status.success();
    let error = if ok {
        None
    } else if output_preview.is_empty() {
        Some(format!("codex exec 退出码 {}", output.status.code().unwrap_or(-1)))
    } else {
        Some(output_preview.clone())
    };

    ProxyTestResponse {
        test_mode: ProxyTestMode::CodexExec,
        ok,
        proxy_connect_ok: true,
        proxy_connect_elapsed_ms: Some(proxy_connect_start.elapsed().as_millis() as u64),
        proxy_connect_error: None,
        target_access_ok: ok,
        status: None,
        status_text: None,
        body: None,
        elapsed_ms: Some(elapsed_ms),
        error,
        proxy_url,
        test_url,
        command_display: Some(command_display),
        command_prompt: Some(prompt),
        command_last_message: last_message,
        command_output: (!output_preview.is_empty()).then_some(output_preview),
        exit_code: output.status.code(),
    }
}

fn combine_command_output(stdout: &[u8], stderr: &[u8]) -> String {
    let stdout_text = String::from_utf8_lossy(stdout).trim().to_string();
    let stderr_text = String::from_utf8_lossy(stderr).trim().to_string();
    match (stdout_text.is_empty(), stderr_text.is_empty()) {
        (true, true) => String::new(),
        (false, true) => stdout_text,
        (true, false) => stderr_text,
        (false, false) => format!("{stdout_text}\n{stderr_text}"),
    }
}

fn truncate_for_display(text: String, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text;
    }
    let truncated = text.chars().take(max_chars).collect::<String>();
    format!("{truncated}\n...[truncated]")
}

fn parse_proxy_host_port(proxy_type: &ProxyType, server: &str) -> Result<(String, u16)> {
    let proxy_url = build_proxy_url(proxy_type, server);
    let parsed =
        reqwest::Url::parse(&proxy_url).with_context(|| format!("无效代理地址: {proxy_url}"))?;
    let host = parsed
        .host_str()
        .map(str::to_string)
        .with_context(|| format!("代理地址缺少主机名: {proxy_url}"))?;
    let port = parsed
        .port_or_known_default()
        .with_context(|| format!("代理地址缺少端口: {proxy_url}"))?;
    Ok((host, port))
}

fn proxy_active_response(proxy_manager: &ProxyManager) -> ProxyActiveResponse {
    ProxyActiveResponse {
        active: proxy_manager
            .get_active()
            .as_ref()
            .map(ProxyPreset::public_view),
        effective_env: proxy_manager
            .get_proxy_env()
            .into_iter()
            .map(|(key, value)| format!("{key}={}", redact_proxy_env_value(&value)))
            .collect(),
        inherited_proxy_env: proxy_manager
            .inherited_proxy_env()
            .into_iter()
            .map(|(key, value)| format!("{key}={}", redact_proxy_env_value(&value)))
            .collect(),
        ignores_system_proxy_env: true,
    }
}

fn uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let rand: u64 =
        (now.as_nanos() as u64) ^ (std::process::id() as u64).wrapping_mul(0x517cc1b727220a95);
    format!("{:016x}{:016x}", now.as_secs(), rand)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    #[test]
    fn proxy_manager_reorders_and_persists_presets() {
        let app_dir = std::env::temp_dir().join(format!("webclx-proxy-order-{}", uuid_v4()));
        std::fs::create_dir_all(&app_dir).expect("test app dir should be created");
        let manager = ProxyManager::load(&app_dir).expect("manager should load");

        manager
            .save(ProxyPreset::new(
                "proxy-a".to_string(),
                "Proxy A".to_string(),
                ProxyType::Http,
                "127.0.0.1:8001".to_string(),
            ))
            .expect("first preset should save");
        manager
            .save(ProxyPreset::new(
                "proxy-b".to_string(),
                "Proxy B".to_string(),
                ProxyType::Http,
                "127.0.0.1:8002".to_string(),
            ))
            .expect("second preset should save");
        manager
            .save(ProxyPreset::new(
                "proxy-c".to_string(),
                "Proxy C".to_string(),
                ProxyType::Socks5,
                "127.0.0.1:8003".to_string(),
            ))
            .expect("third preset should save");

        manager
            .reorder(vec![
                "proxy-c".to_string(),
                "proxy-a".to_string(),
                "proxy-b".to_string(),
            ])
            .expect("complete id list should reorder");

        assert_eq!(
            manager
                .list()
                .into_iter()
                .map(|preset| preset.id)
                .collect::<Vec<_>>(),
            vec!["proxy-c", "proxy-a", "proxy-b"]
        );

        let reloaded = ProxyManager::load(&app_dir).expect("manager should reload");
        assert_eq!(
            reloaded
                .list()
                .into_iter()
                .map(|preset| preset.id)
                .collect::<Vec<_>>(),
            vec!["proxy-c", "proxy-a", "proxy-b"]
        );

        let _ = std::fs::remove_dir_all(app_dir);
    }

    #[test]
    fn https_proxy_uses_tls_proxy_url() {
        assert_eq!(serde_json::to_string(&ProxyType::Https).unwrap(), r#""https""#);
        assert_eq!(
            build_proxy_url(&ProxyType::Https, "us.fpsq.xyz:17891"),
            "https://us.fpsq.xyz:17891"
        );
    }

    #[test]
    fn authenticated_proxy_persists_credentials_but_public_view_hides_password() {
        let app_dir = std::env::temp_dir().join(format!("webclx-proxy-auth-{}", uuid_v4()));
        std::fs::create_dir_all(&app_dir).expect("test app dir should be created");
        let manager = ProxyManager::load(&app_dir).expect("manager should load");
        let mut preset = ProxyPreset::new(
            "proxy-auth".to_string(),
            "Authenticated proxy".to_string(),
            ProxyType::Https,
            "proxy.example.com:17891".to_string(),
        );
        preset.username = Some("proxy-user".to_string());
        preset.password = Some("proxy-secret".to_string());
        manager
            .save(preset)
            .expect("authenticated proxy should save");
        manager
            .set_active("proxy-auth")
            .expect("authenticated proxy should become active");

        let reloaded = ProxyManager::load(&app_dir).expect("manager should reload");
        let stored = reloaded.get("proxy-auth").expect("proxy should exist");
        assert_eq!(stored.username.as_deref(), Some("proxy-user"));
        assert_eq!(stored.password.as_deref(), Some("proxy-secret"));

        let public_json =
            serde_json::to_value(stored.public_view()).expect("public proxy view should serialize");
        assert_eq!(public_json["username"], "proxy-user");
        assert_eq!(public_json["has_password"], true);
        assert!(public_json.get("password").is_none());
        let env = reloaded.get_proxy_env();
        assert!(
            env.iter()
                .all(|(_, value)| value.contains("proxy-user:proxy-secret@")),
            "{env:?}"
        );
        let active_response = proxy_active_response(&reloaded);
        assert!(
            active_response
                .effective_env
                .iter()
                .all(|value| !value.contains("proxy-secret") && value.contains("REDACTED")),
            "{:?}",
            active_response.effective_env
        );

        let _ = std::fs::remove_dir_all(app_dir);
    }

    #[tokio::test]
    async fn authenticated_proxy_client_sends_basic_auth() {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("proxy listener should bind");
        let addr = listener.local_addr().expect("proxy address should resolve");
        let captured = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let captured_for_task = captured.clone();
        tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.expect("proxy should accept");
            let mut buffer = vec![0_u8; 8 * 1024];
            let read = stream.read(&mut buffer).await.expect("request should read");
            *captured_for_task.lock().unwrap() =
                String::from_utf8_lossy(&buffer[..read]).to_string();
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .expect("response should write");
        });

        let proxy = build_reqwest_proxy(
            &ProxyType::Http,
            &addr.to_string(),
            Some("proxy-user"),
            Some("proxy-secret"),
        )
        .expect("authenticated proxy should build");
        let client = reqwest::Client::builder()
            .no_proxy()
            .proxy(proxy)
            .build()
            .expect("proxy client should build");
        let response = client
            .get("http://example.test/probe")
            .send()
            .await
            .expect("proxy request should succeed");
        assert_eq!(response.status(), reqwest::StatusCode::OK);

        let request = captured.lock().unwrap().to_ascii_lowercase();
        assert!(
            request.contains("proxy-authorization: basic chjvehktdxnlcjpwcm94es1zzwnyzxq="),
            "{request}"
        );
    }
}
