use std::{collections::HashSet, time::Duration};

use auth_core::{
    StoredApiPreset, StoredAuthPreset, StoredClaudePreset, UpstreamProxySettings,
    persist_api_presets_async, persist_auth_presets_async, persist_claude_presets_async,
    persist_upstream_proxy_settings,
};
use axum::{
    Json,
    extract::{Path as AxumPath, State},
};
use reqwest::Url;

use crate::login::SESSION_COOKIE_NAME;
use serde::{Deserialize, Serialize};

use crate::{ApiResult, AppError, AppState, proxy::ProxyPreset};

const PRESET_CONFIG_ENDPOINT: &str = "/api/settings/preset-config";
const REMOTE_FETCH_TIMEOUT_SECS: u64 = 15;
const ACCOUNT_PRESET_CLIPBOARD_FORMAT: &str = "webclx-account-presets";
const ACCOUNT_PRESET_CLIPBOARD_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetConfigSnapshot {
    #[serde(default)]
    pub auth_presets: Vec<StoredAuthPreset>,
    #[serde(default)]
    pub api_presets: Vec<StoredApiPreset>,
    #[serde(default)]
    pub claude_presets: Vec<StoredClaudePreset>,
    #[serde(default)]
    pub upstream_proxy: UpstreamProxySettings,
    #[serde(default)]
    pub proxy_presets: Vec<ProxyPreset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_proxy_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PresetConfigSummary {
    pub auth_preset_count: usize,
    pub api_preset_count: usize,
    pub claude_preset_count: usize,
    pub proxy_preset_count: usize,
    pub codex_api_proxy_enabled: bool,
    pub claude_proxy_enabled: bool,
    pub active_proxy_id: Option<String>,
    pub active_api_proxy_preset_id: Option<String>,
    pub active_claude_proxy_preset_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RemotePresetConfigRequest {
    pub remote_url: String,
    #[serde(default)]
    pub sections: Vec<String>,
    #[serde(default)]
    pub confirm_proxy_presets: bool,
}

#[derive(Debug, Serialize)]
pub struct RemotePresetConfigPreviewResponse {
    pub ok: bool,
    pub source_url: String,
    pub summary: PresetConfigSummary,
}

#[derive(Debug, Serialize)]
pub struct RemotePresetConfigImportResponse {
    pub ok: bool,
    pub source_url: String,
    pub summary: PresetConfigSummary,
    pub imported_sections: PresetConfigSections,
}

#[derive(Debug, Clone, Serialize)]
pub struct PresetConfigSections {
    pub auth_presets: bool,
    pub api_presets: bool,
    pub claude_presets: bool,
    pub proxy_presets: bool,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AccountPresetClipboardPayload {
    pub format: String,
    pub version: u32,
    pub section: String,
    pub accounts: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct AccountPresetClipboardImportResponse {
    pub ok: bool,
    pub section: String,
    pub imported_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct AccountPresetClipboardExportRequest {
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AccountPresetSection {
    Auth,
    Api,
    Claude,
}

impl AccountPresetSection {
    fn parse(value: &str) -> ApiResult<Self> {
        match value.trim() {
            "auth_presets" => Ok(Self::Auth),
            "api_presets" => Ok(Self::Api),
            "claude_presets" => Ok(Self::Claude),
            _ => Err(AppError::bad_request("未知的账号列表类别")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Auth => "auth_presets",
            Self::Api => "api_presets",
            Self::Claude => "claude_presets",
        }
    }
}

pub async fn export_preset_config(
    State(state): State<AppState>,
) -> ApiResult<Json<PresetConfigSnapshot>> {
    Ok(Json(snapshot_from_state(&state)))
}

pub async fn export_account_presets_to_clipboard(
    State(state): State<AppState>,
    AxumPath(raw_section): AxumPath<String>,
    Json(request): Json<AccountPresetClipboardExportRequest>,
) -> ApiResult<Json<AccountPresetClipboardPayload>> {
    let section = AccountPresetSection::parse(&raw_section)?;
    let selected_ids = request
        .ids
        .iter()
        .map(|id| id.trim())
        .filter(|id| !id.is_empty())
        .collect::<HashSet<_>>();
    if selected_ids.is_empty() {
        return Err(AppError::bad_request("请至少选择一个要导出的账号"));
    }
    let accounts = match section {
        AccountPresetSection::Auth => serde_json::to_value(
            state
                .auth_manager
                .auth_presets_snapshot()
                .into_iter()
                .filter(|preset| selected_ids.contains(preset.id.as_str()))
                .collect::<Vec<_>>(),
        ),
        AccountPresetSection::Api => serde_json::to_value(
            state
                .auth_manager
                .api_presets_snapshot()
                .into_iter()
                .filter(|preset| selected_ids.contains(preset.id.as_str()))
                .collect::<Vec<_>>(),
        ),
        AccountPresetSection::Claude => serde_json::to_value(
            state
                .auth_manager
                .claude_presets_snapshot()
                .into_iter()
                .filter(|preset| selected_ids.contains(preset.id.as_str()))
                .collect::<Vec<_>>(),
        ),
    }
    .map_err(|error| AppError::internal(format!("序列化账号列表失败: {error}")))?;
    if accounts.as_array().map_or(0, Vec::len) != selected_ids.len() {
        return Err(AppError::bad_request("部分选中账号已不存在，请刷新列表后重试"));
    }

    Ok(Json(AccountPresetClipboardPayload {
        format: ACCOUNT_PRESET_CLIPBOARD_FORMAT.to_string(),
        version: ACCOUNT_PRESET_CLIPBOARD_VERSION,
        section: section.as_str().to_string(),
        accounts,
    }))
}

pub async fn import_account_presets_from_clipboard(
    State(state): State<AppState>,
    AxumPath(raw_section): AxumPath<String>,
    Json(payload): Json<AccountPresetClipboardPayload>,
) -> ApiResult<Json<AccountPresetClipboardImportResponse>> {
    let section = validate_account_preset_clipboard_payload(&raw_section, &payload)?;
    let imported_count = match section {
        AccountPresetSection::Auth => {
            let presets = decode_clipboard_accounts::<StoredAuthPreset>(payload.accounts)?;
            let count = presets.len();
            let mut merged = state.auth_manager.auth_presets_snapshot();
            for preset in presets {
                if let Some(index) = merged.iter().position(|item| item.id == preset.id) {
                    merged[index] = preset;
                } else {
                    merged.push(preset);
                }
            }
            persist_auth_presets_async(&state.auth_manager, &merged)
                .await
                .map_err(|error| AppError::internal(format!("保存 auth 预设失败: {error}")))?;
            count
        }
        AccountPresetSection::Api => {
            let presets = decode_clipboard_accounts::<StoredApiPreset>(payload.accounts)?;
            let count = presets.len();
            let mut merged = state.auth_manager.api_presets_snapshot();
            for preset in presets {
                if let Some(index) = merged.iter().position(|item| item.id == preset.id) {
                    merged[index] = preset;
                } else {
                    merged.push(preset);
                }
            }
            persist_api_presets_async(&state.auth_manager, &merged)
                .await
                .map_err(|error| AppError::internal(format!("保存 API 预设失败: {error}")))?;
            count
        }
        AccountPresetSection::Claude => {
            let presets = decode_clipboard_accounts::<StoredClaudePreset>(payload.accounts)?;
            let count = presets.len();
            let mut merged = state.auth_manager.claude_presets_snapshot();
            for preset in presets {
                if let Some(index) = merged.iter().position(|item| item.id == preset.id) {
                    merged[index] = preset;
                } else {
                    merged.push(preset);
                }
            }
            persist_claude_presets_async(&state.auth_manager, &merged)
                .await
                .map_err(|error| AppError::internal(format!("保存 Claude 预设失败: {error}")))?;
            count
        }
    };

    Ok(Json(AccountPresetClipboardImportResponse {
        ok: true,
        section: section.as_str().to_string(),
        imported_count,
    }))
}

fn validate_account_preset_clipboard_payload(
    raw_section: &str,
    payload: &AccountPresetClipboardPayload,
) -> ApiResult<AccountPresetSection> {
    let route_section = AccountPresetSection::parse(raw_section)?;
    if payload.format != ACCOUNT_PRESET_CLIPBOARD_FORMAT {
        return Err(AppError::bad_request("剪贴板内容不是 webClx 账号列表"));
    }
    if payload.version != ACCOUNT_PRESET_CLIPBOARD_VERSION {
        return Err(AppError::bad_request("不支持该账号列表格式版本"));
    }
    let payload_section = AccountPresetSection::parse(&payload.section)?;
    if route_section != payload_section {
        return Err(AppError::bad_request("剪贴板账号类别与当前页面不一致"));
    }
    if !payload.accounts.is_array() {
        return Err(AppError::bad_request("剪贴板账号列表必须是数组"));
    }
    Ok(route_section)
}

fn decode_clipboard_accounts<T>(value: serde_json::Value) -> ApiResult<Vec<T>>
where
    T: for<'de> Deserialize<'de>,
{
    serde_json::from_value(value)
        .map_err(|error| AppError::bad_request(format!("账号列表内容无效: {error}")))
}

pub async fn preview_remote_preset_config(
    State(state): State<AppState>,
    Json(payload): Json<RemotePresetConfigRequest>,
) -> ApiResult<Json<RemotePresetConfigPreviewResponse>> {
    let (source_url, snapshot) = fetch_remote_preset_config(&payload.remote_url).await?;
    record_preset_sync_remote_url(&state, &source_url)?;
    Ok(Json(RemotePresetConfigPreviewResponse {
        ok: true,
        source_url,
        summary: snapshot_summary(&snapshot),
    }))
}

pub async fn import_remote_preset_config(
    State(state): State<AppState>,
    Json(payload): Json<RemotePresetConfigRequest>,
) -> ApiResult<Json<RemotePresetConfigImportResponse>> {
    let sections = PresetConfigSections::from_request(&payload.sections)?;
    if sections.proxy_presets && !payload.confirm_proxy_presets {
        return Err(AppError::bad_request(
            "同步代理预设/上游代理前必须确认：不同服务器可能无法连接同样代理，会导致网络不可用",
        ));
    }
    let (source_url, snapshot) = fetch_remote_preset_config(&payload.remote_url).await?;
    record_preset_sync_remote_url(&state, &source_url)?;
    persist_snapshot(&state, &snapshot, &sections).await?;
    Ok(Json(RemotePresetConfigImportResponse {
        ok: true,
        source_url,
        summary: snapshot_summary(&snapshot),
        imported_sections: sections,
    }))
}

fn record_preset_sync_remote_url(state: &AppState, source_url: &str) -> ApiResult<()> {
    state
        .workspace_settings
        .record_preset_sync_remote_url(source_url)
        .map(|_| ())
        .map_err(|error| AppError::internal(format!("保存远程 webClx 地址历史失败: {error}")))
}

impl PresetConfigSections {
    fn all() -> Self {
        Self {
            auth_presets: true,
            api_presets: true,
            claude_presets: true,
            proxy_presets: true,
        }
    }

    fn none() -> Self {
        Self {
            auth_presets: false,
            api_presets: false,
            claude_presets: false,
            proxy_presets: false,
        }
    }

    fn from_request(raw_sections: &[String]) -> ApiResult<Self> {
        if raw_sections.is_empty() {
            return Ok(Self::all());
        }

        let mut sections = Self::none();
        for raw_section in raw_sections {
            match raw_section.trim() {
                "auth_presets" | "auth" | "codex_oauth" => sections.auth_presets = true,
                "api_presets" | "api" | "codex_api" => sections.api_presets = true,
                "claude_presets" | "claude" | "claude_api" => sections.claude_presets = true,
                "proxy_presets" | "proxy" | "proxies" => sections.proxy_presets = true,
                "" => {}
                other => {
                    return Err(AppError::bad_request(format!("未知的预设同步类别: {other}")));
                }
            }
        }

        if !sections.auth_presets
            && !sections.api_presets
            && !sections.claude_presets
            && !sections.proxy_presets
        {
            return Err(AppError::bad_request("至少选择一个预设同步类别"));
        }

        Ok(sections)
    }
}

fn snapshot_from_state(state: &AppState) -> PresetConfigSnapshot {
    PresetConfigSnapshot {
        auth_presets: state.auth_manager.auth_presets_snapshot(),
        api_presets: state.auth_manager.api_presets_snapshot(),
        claude_presets: state.auth_manager.claude_presets_snapshot(),
        upstream_proxy: state.auth_manager.upstream_proxy_settings(),
        proxy_presets: state.proxy_manager.list(),
        active_proxy_id: state.proxy_manager.active_id(),
    }
}

fn snapshot_summary(snapshot: &PresetConfigSnapshot) -> PresetConfigSummary {
    PresetConfigSummary {
        auth_preset_count: snapshot.auth_presets.len(),
        api_preset_count: snapshot.api_presets.len(),
        claude_preset_count: snapshot.claude_presets.len(),
        proxy_preset_count: snapshot.proxy_presets.len(),
        codex_api_proxy_enabled: snapshot.upstream_proxy.codex_api_proxy_enabled,
        claude_proxy_enabled: snapshot.upstream_proxy.claude_proxy_enabled,
        active_proxy_id: snapshot.active_proxy_id.clone(),
        active_api_proxy_preset_id: snapshot.upstream_proxy.active_api_proxy_preset_id.clone(),
        active_claude_proxy_preset_id: snapshot
            .upstream_proxy
            .active_claude_proxy_preset_id
            .clone(),
    }
}

async fn persist_snapshot(
    state: &AppState,
    snapshot: &PresetConfigSnapshot,
    sections: &PresetConfigSections,
) -> ApiResult<()> {
    if sections.auth_presets {
        persist_auth_presets_async(&state.auth_manager, &snapshot.auth_presets)
            .await
            .map_err(|error| AppError::internal(format!("保存远程 auth 预设失败: {error}")))?;
    }
    if sections.api_presets {
        persist_api_presets_async(&state.auth_manager, &snapshot.api_presets)
            .await
            .map_err(|error| AppError::internal(format!("保存远程 API 预设失败: {error}")))?;
    }
    if sections.claude_presets {
        persist_claude_presets_async(&state.auth_manager, &snapshot.claude_presets)
            .await
            .map_err(|error| AppError::internal(format!("保存远程 Claude 预设失败: {error}")))?;
    }
    if sections.proxy_presets {
        persist_upstream_proxy_settings(&state.auth_manager, snapshot.upstream_proxy.clone())
            .map_err(|error| AppError::internal(format!("保存远程上游代理设置失败: {error}")))?;
        state
            .proxy_manager
            .replace_all(snapshot.proxy_presets.clone(), snapshot.active_proxy_id.clone())
            .map_err(|error| AppError::internal(format!("保存远程代理预设失败: {error}")))?;
    }
    Ok(())
}

async fn fetch_remote_preset_config(remote_url: &str) -> ApiResult<(String, PresetConfigSnapshot)> {
    let (source_url, login_url, username, password) = analyze_remote_url(remote_url)?;
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(REMOTE_FETCH_TIMEOUT_SECS))
        .build()
        .map_err(|error| AppError::internal(format!("创建HTTP客户端失败: {error}")))?;

    // 远程 webClx 只认 session cookie，不解析 Basic Auth/Bearer。因此当 URL
    // 含 userinfo（http://user:pass@host）时，先调远程 /api/auth/login 拿到
    // session cookie，再带 cookie 请求 preset-config 接口。
    let mut cookie_value: Option<String> = None;
    if let (Some(user), Some(pass)) = (username.as_deref(), password.as_deref()) {
        let login_response = client
            .post(&login_url)
            .header("Content-Type", "application/json")
            .body(
                serde_json::to_string(&serde_json::json!({
                    "username": user,
                    "password": pass,
                }))
                .unwrap_or_default(),
            )
            .send()
            .await
            .map_err(|error| AppError::bad_request(format!("连接远程登录接口失败: {error}")))?;
        if !login_response.status().is_success() {
            return Err(AppError::bad_request(format!(
                "远程登录失败，状态: {}，请检查地址中的账号密码",
                login_response.status()
            )));
        }
        if let Some(set_cookie) = login_response.headers().get(reqwest::header::SET_COOKIE)
            && let Ok(value) = set_cookie.to_str()
        {
            for pair in value.split(';') {
                let pair = pair.trim();
                if let Some((k, v)) = pair.split_once('=')
                    && k.trim() == SESSION_COOKIE_NAME
                {
                    cookie_value = Some(v.trim().to_string());
                    break;
                }
            }
        }
        if cookie_value.is_none() {
            return Err(AppError::bad_request(
                "远程登录成功但未返回会话 cookie，无法继续读取预设配置",
            ));
        }
    }

    let mut request = client.get(&source_url);
    if let Some(ref cookie) = cookie_value {
        request = request.header("Cookie", format!("{SESSION_COOKIE_NAME}={cookie}"));
    }
    let response = request
        .send()
        .await
        .map_err(|error| AppError::bad_request(format!("连接远程预设接口失败: {error}")))?;
    if !response.status().is_success() {
        return Err(AppError::bad_request(format!(
            "远程预设接口返回错误状态: {}",
            response.status()
        )));
    }
    let snapshot = response
        .json::<PresetConfigSnapshot>()
        .await
        .map_err(|error| AppError::bad_request(format!("解析远程预设配置失败: {error}")))?;
    Ok((source_url, snapshot))
}

/// 解析远程地址，返回 (preset-config 接口地址, 登录接口地址, 用户名, 密码)。
/// 登录接口固定为 `${origin}/api/auth/login`，不受 preset-config 路径后缀影响。
fn analyze_remote_url(input: &str) -> ApiResult<(String, String, Option<String>, Option<String>)> {
    let source_url = normalize_preset_config_url(input)?;
    let parsed = Url::parse(&source_url)
        .map_err(|error| AppError::bad_request(format!("远程地址无效: {error}")))?;
    let username = {
        let raw = parsed.username();
        if raw.is_empty() {
            None
        } else {
            Some(
                percent_encoding::percent_decode_str(raw)
                    .decode_utf8_lossy()
                    .into_owned(),
            )
        }
    };
    let password = parsed.password().map(|raw| {
        percent_encoding::percent_decode_str(raw)
            .decode_utf8_lossy()
            .into_owned()
    });

    // 从 origin 重新拼接登录地址，确保 login_url 绝不携带 username/password，
    // 即使原 URL 的凭据无法被 set_username/set_password 清除。
    let login_url = build_login_url(&parsed);
    Ok((source_url, login_url, username, password))
}

/// 基于 `base` 的 scheme/host/port 拼接 `/api/auth/login`，丢弃 userinfo 与 path。
fn build_login_url(base: &Url) -> String {
    format!(
        "{}://{}{}/api/auth/login",
        base.scheme(),
        base.host_str().unwrap_or(""),
        base.port()
            .map(|port| format!(":{port}"))
            .unwrap_or_default(),
    )
}

fn normalize_preset_config_url(input: &str) -> ApiResult<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(AppError::bad_request("远程地址不能为空"));
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("http://{trimmed}")
    };
    let mut url = Url::parse(&candidate)
        .map_err(|error| AppError::bad_request(format!("远程地址无效: {error}")))?;
    url.set_query(None);
    url.set_fragment(None);

    let path = url.path().trim_end_matches('/');
    if path.is_empty() {
        url.set_path(PRESET_CONFIG_ENDPOINT);
    } else if !path.ends_with(PRESET_CONFIG_ENDPOINT) {
        let next_path = format!("{path}{PRESET_CONFIG_ENDPOINT}");
        url.set_path(&next_path);
    }
    Ok(url.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clipboard_payload(section: &str) -> AccountPresetClipboardPayload {
        AccountPresetClipboardPayload {
            format: ACCOUNT_PRESET_CLIPBOARD_FORMAT.to_string(),
            version: ACCOUNT_PRESET_CLIPBOARD_VERSION,
            section: section.to_string(),
            accounts: serde_json::json!([]),
        }
    }

    #[test]
    fn clipboard_payload_requires_matching_account_section() {
        let payload = clipboard_payload("api_presets");
        assert!(validate_account_preset_clipboard_payload("auth_presets", &payload).is_err());
    }

    #[test]
    fn clipboard_payload_rejects_unknown_format_and_version() {
        let mut payload = clipboard_payload("claude_presets");
        payload.format = "other".to_string();
        assert!(validate_account_preset_clipboard_payload("claude_presets", &payload).is_err());

        payload.format = ACCOUNT_PRESET_CLIPBOARD_FORMAT.to_string();
        payload.version += 1;
        assert!(validate_account_preset_clipboard_payload("claude_presets", &payload).is_err());
    }

    #[test]
    fn clipboard_payload_accepts_each_supported_account_section() {
        for section in ["auth_presets", "api_presets", "claude_presets"] {
            let payload = clipboard_payload(section);
            let parsed = validate_account_preset_clipboard_payload(section, &payload).unwrap();
            assert_eq!(parsed.as_str(), section);
        }
    }

    #[test]
    fn clipboard_payload_requires_an_accounts_array() {
        let mut payload = clipboard_payload("auth_presets");
        payload.accounts = serde_json::json!({});
        assert!(validate_account_preset_clipboard_payload("auth_presets", &payload).is_err());
    }
}
