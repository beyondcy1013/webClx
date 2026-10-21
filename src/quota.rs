//! 套餐用量查询代理（GLM Coding Plan + MiniMax TokenPlan）。
//!
//! 把智谱（bigmodel）与 MiniMax 的用量/配额查询接口代理到本机，前端可
//! 通过同一组 `/api/quota/*` 接口调用，按 base_url 自动分派上游实现。
//! 避免 CORS 限制并集中管理 API key。key 持久化在
//! `app_dir/.webclx-quota.json` 中，仅保存当前「默认配置」；临时覆盖
//! 通过 query 参数传入，用于在前端下拉里快速切换预设。

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::{
    Json,
    extract::{Query, State},
};
use futures_util::future::join_all;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::{Duration as TimeDuration, OffsetDateTime, UtcOffset, macros::format_description};
use tokio::sync::RwLock;
use tracing::warn;

use crate::{ApiResult, AppError, AppState};

const DEFAULT_ZHIPU_BASE_URL: &str = "https://open.bigmodel.cn";
const DEFAULT_ZHIPU_API_KEY: &str = "741bb8018fcf4de490c03f91d35e55e5.VeGe0U8hsHcS6hWC";
/// MiniMax 国内版（minimaxi.com）；国际版切换为 minimax.io。
const DEFAULT_MINIMAX_BASE_URL: &str = "https://www.minimaxi.com";
const QUOTA_CONFIG_FILE: &str = ".webclx-quota.json";
const UPSTREAM_TIMEOUT_SECS: u64 = 20;

/// 当前 query 支持的套餐平台；按 base_url 自动派发。
/// 仅作为协议派发使用，前端拿到 `platform` 后再做平台差异渲染。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QuotaPlatform {
    Zhipu,
    MiniMax,
    Grok,
}

impl QuotaPlatform {
    fn as_str(self) -> &'static str {
        match self {
            QuotaPlatform::Zhipu => "ZHIPU",
            QuotaPlatform::MiniMax => "MINIMAX",
            QuotaPlatform::Grok => "GROK",
        }
    }
}

/// 按 base_url 推断平台。把不同厂商的 host 都映射到对应平台，便于
/// 用户在 Codex_API 预设里随手填的 `https://www.minimaxi.com/v1`、
/// `https://api.minimax.io/v1` 等都自动走 MiniMax 分支。
fn detect_platform(base_url: &str) -> QuotaPlatform {
    let lower = base_url.trim().to_ascii_lowercase();
    if lower.contains("minimaxi.com")
        || lower.contains("minimax.io")
        || lower.contains("/api/codex-proxy/minimax/")
        || lower.contains("minimax")
    {
        QuotaPlatform::MiniMax
    } else if lower.contains("grok.com") || lower.contains("x.ai") {
        QuotaPlatform::Grok
    } else {
        QuotaPlatform::Zhipu
    }
}

/// 用户在「设置」面板里指定的默认打开平台；决定打开套餐按钮时下拉框
/// 自动选中哪一项。仅做协议派发使用，前端拿到 `platform` 后再做平台
/// 差异渲染。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DefaultQuotaProvider {
    /// 沿用 `Saved` 配置里的 api_key/base_url（与旧版行为一致）。
    Saved,
    Zhipu,
    MiniMax,
    /// 默认选中下拉里的「指定账号」条目（`custom_preset_id` 指向的预设）。
    Custom,
    /// 默认选中第一个 OpenRouter 预设；与前端「默认显示」下拉的选项对齐。
    OpenRouter,
}

impl DefaultQuotaProvider {
    fn as_str(self) -> &'static str {
        match self {
            DefaultQuotaProvider::Saved => "SAVED",
            DefaultQuotaProvider::Zhipu => "ZHIPU",
            DefaultQuotaProvider::MiniMax => "MINIMAX",
            DefaultQuotaProvider::Custom => "CUSTOM",
            DefaultQuotaProvider::OpenRouter => "OPENROUTER",
        }
    }
}

impl Default for DefaultQuotaProvider {
    fn default() -> Self {
        Self::Saved
    }
}

/// 持久化的用量查询配置（API key、可选 base_url、可选默认平台）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaConfig {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,
    /// 设置面板里的「默认显示」选项；缺省时序列化层用 `Saved`。
    #[serde(default)]
    pub default_provider: DefaultQuotaProvider,
    /// 「指定账号」条目：指向 Codex_API 预设列表里某个预设的 id。该预设
    /// 不受平台域名过滤限制，前端在套餐下拉里作为单独一项展示；预设被
    /// 删除后该 id 解析不到即自然失效。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_preset_id: Option<String>,
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            api_key: DEFAULT_ZHIPU_API_KEY.to_string(),
            base_url: DEFAULT_ZHIPU_BASE_URL.to_string(),
            default_provider: DefaultQuotaProvider::Saved,
            custom_preset_id: None,
        }
    }
}

/// 进程内的配置缓存，启动时读取、变更时回写。
#[derive(Clone)]
pub struct QuotaConfigManager {
    inner: Arc<RwLock<QuotaConfig>>,
    config_path: PathBuf,
}

impl QuotaConfigManager {
    pub fn load(app_dir: &std::path::Path) -> Self {
        let config_path = app_dir.join(QUOTA_CONFIG_FILE);
        let config = match std::fs::read_to_string(&config_path) {
            Ok(content) => {
                let mut parsed: QuotaConfig = serde_json::from_str(&content).unwrap_or_default();
                // 文件里若留空，回退到内置默认值，避免发空 key。
                if parsed.api_key.trim().is_empty() {
                    parsed.api_key = DEFAULT_ZHIPU_API_KEY.to_string();
                }
                if parsed.base_url.trim().is_empty() {
                    parsed.base_url = DEFAULT_ZHIPU_BASE_URL.to_string();
                }
                parsed
            }
            Err(error) => {
                if error.kind() != std::io::ErrorKind::NotFound {
                    warn!(?error, "failed to read quota config, using defaults");
                }
                QuotaConfig::default()
            }
        };
        Self {
            inner: Arc::new(RwLock::new(config)),
            config_path,
        }
    }

    pub async fn snapshot(&self) -> QuotaConfig {
        self.inner.read().await.clone()
    }

    pub async fn update(
        &self,
        api_key: &str,
        base_url: &str,
        default_provider: DefaultQuotaProvider,
        custom_preset_id: Option<&str>,
    ) -> Result<(), AppError> {
        // 空串与纯空白一律归一为「未指定」，避免存进无效 id。
        let custom_preset_id = custom_preset_id
            .map(str::trim)
            .filter(|id| !id.is_empty())
            .map(str::to_string);
        let mut sanitized = QuotaConfig {
            api_key: api_key.trim().to_string(),
            base_url: base_url.trim().to_string(),
            default_provider,
            custom_preset_id,
        };
        if sanitized.api_key.is_empty() {
            sanitized.api_key = DEFAULT_ZHIPU_API_KEY.to_string();
        }
        if sanitized.base_url.is_empty() {
            sanitized.base_url = DEFAULT_ZHIPU_BASE_URL.to_string();
        }
        let content = serde_json::to_vec_pretty(&sanitized)
            .map_err(|e| AppError::internal(format!("序列化配置失败: {e}")))?;
        std::fs::write(&self.config_path, content)
            .map_err(|e| AppError::internal(format!("写入配置失败: {e}")))?;
        *self.inner.write().await = sanitized;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct SaveQuotaConfigRequest {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,
    /// 缺省时落到 `Saved`，兼容旧版保存请求。
    #[serde(default)]
    pub default_provider: DefaultQuotaProvider,
    /// 「指定账号」条目指向的 Codex_API 预设 id；null/缺省表示未指定。
    #[serde(default)]
    pub custom_preset_id: Option<String>,
}

/// 返回当前配置（key 做脱敏处理，避免明文回传到前端）。
/// 同时返回两套默认值（智谱 / MiniMax），方便前端在用户切换平台时填充
/// 占位/默认值。
pub async fn get_quota_config(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let config = state.quota_manager.snapshot().await;
    let masked = mask_key(&config.api_key);
    Ok(Json(json!({
        "api_key_masked": masked,
        "api_key": config.api_key,
        "base_url": config.base_url,
        "default_provider": config.default_provider.as_str(),
        "custom_preset_id": config.custom_preset_id,
        "default_api_key": DEFAULT_ZHIPU_API_KEY,
        "default_base_url": DEFAULT_ZHIPU_BASE_URL,
        "default_minimax_base_url": DEFAULT_MINIMAX_BASE_URL,
    })))
}

pub async fn save_quota_config(
    State(state): State<AppState>,
    Json(payload): Json<SaveQuotaConfigRequest>,
) -> ApiResult<Json<Value>> {
    state
        .quota_manager
        .update(
            &payload.api_key,
            &payload.base_url,
            payload.default_provider,
            payload.custom_preset_id.as_deref(),
        )
        .await?;
    let config = state.quota_manager.snapshot().await;
    Ok(Json(json!({
        "ok": true,
        "api_key_masked": mask_key(&config.api_key),
        "base_url": config.base_url,
        "default_provider": config.default_provider.as_str(),
        "custom_preset_id": config.custom_preset_id,
    })))
}

/// 代理查询：按 base_url 自动分派到对应平台实现，并组装成前端可直接渲染的结构。
///
/// 支持：
/// - 智谱（bigmodel.cn / zcode.z.ai）：并行抓取 model-usage / tool-usage / quota/limit。
/// - MiniMax（minimaxi.com / minimax.io）：抓取 `/v1/token_plan/remains`。
/// - Grok（cli-chat-proxy.grok.com）：先用 OIDC refresh_token 换 access token，
///   再查询 `/billing?format=credits` 与 `/user?include=subscription`。
pub async fn query_quota(
    State(state): State<AppState>,
    Query(params): Query<QuotaQueryParams>,
) -> ApiResult<Json<Value>> {
    let config = state.quota_manager.snapshot().await;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(UPSTREAM_TIMEOUT_SECS))
        .build()
        .map_err(|e| AppError::internal(format!("创建HTTP客户端失败: {e}")))?;

    // 前端可附带 api_key/base_url 临时覆盖已保存配置，用于在多个 key 之间快速切换查询，
    // 而无需把每个 key 都先保存进持久化配置。
    let (base_url, api_key) = {
        let b = if !params.base_url.trim().is_empty() {
            params.base_url.trim().to_string()
        } else {
            config.base_url.clone()
        };
        let k = if !params.api_key.trim().is_empty() {
            params.api_key.trim().to_string()
        } else {
            config.api_key.clone()
        };
        (b, k)
    };

    let platform = detect_platform(&base_url);
    match platform {
        QuotaPlatform::MiniMax => {
            let payload = query_minimax(&client, &base_url, &api_key).await?;
            Ok(Json(json!({
                "platform": platform.as_str(),
                "base_url": base_url,
                "remains": payload,
            })))
        }
        QuotaPlatform::Grok => {
            let payload = query_grok(&client, &api_key).await?;
            Ok(Json(json!({
                "platform": platform.as_str(),
                "base_url": base_url,
                "billing": payload.billing,
                "subscription": payload.subscription,
            })))
        }
        QuotaPlatform::Zhipu => {
            // 智谱用量/配额监测接口固定在平台根 (scheme://host[:port]) 下：
            //   {root}/api/monitor/usage/{model-usage|tool-usage}、{root}/api/monitor/usage/quota/limit
            // 而 Codex_API 预设里的 base_url 可能是 coding 端点（如
            // `https://open.bigmodel.cn/api/coding/paas/v4`）。若直接在其后拼接
            // `/api/monitor/usage/...`，会得到 `.../v4/api/monitor/usage/...` → 404。
            // 因此先剥离掉 base_url 的路径，只保留 scheme://host[:port]。
            let base = zhipu_monitor_base_url(&base_url);
            let auth = &api_key;

            let model_usage_url = format!("{base}/api/monitor/usage/model-usage");
            let tool_usage_url = format!("{base}/api/monitor/usage/tool-usage");
            let quota_limit_url = format!("{base}/api/monitor/usage/quota/limit");

            // 时间窗口：昨天当前小时 ~ 今天当前小时末。
            let (start_str, end_str) = format_query_window();

            let model_fut =
                fetch_json(&client, &model_usage_url, auth, Some((&start_str, &end_str)));
            let tool_fut = fetch_json(&client, &tool_usage_url, auth, Some((&start_str, &end_str)));
            let quota_fut = fetch_json(&client, &quota_limit_url, auth, None);

            let (model_res, tool_res, quota_res) = tokio::join!(model_fut, tool_fut, quota_fut);

            let model_usage = model_res
                .map_err(|e| AppError::bad_request(format!("Model usage 查询失败: {e}")))?;
            let tool_usage =
                tool_res.map_err(|e| AppError::bad_request(format!("Tool usage 查询失败: {e}")))?;
            let quota_limit = quota_res
                .map_err(|e| AppError::bad_request(format!("Quota limit 查询失败: {e}")))?;

            Ok(Json(json!({
                "platform": platform.as_str(),
                "base_url": base_url,
                "window": { "start": start_str, "end": end_str },
                "model_usage": model_usage,
                "tool_usage": tool_usage,
                "quota_limit": quota_limit,
            })))
        }
    }
}

#[derive(Debug, Deserialize)]
struct GrokAuthEntry {
    refresh_token: String,
    user_id: String,
    oidc_issuer: String,
    oidc_client_id: String,
    #[serde(default)]
    principal_type: Option<String>,
    #[serde(default)]
    principal_id: Option<String>,
}

#[derive(Debug)]
struct GrokQuotaPayload {
    billing: Value,
    subscription: Value,
}

async fn refresh_grok_access_token(
    client: &reqwest::Client,
    issuer: &str,
    client_id: &str,
    refresh_token: &str,
    principal_type: Option<&str>,
    principal_id: Option<&str>,
) -> Result<(String, Option<String>), AppError> {
    let discovery_url =
        format!("{}/.well-known/openid-configuration", issuer.trim_end_matches('/'));
    let discovery_response = client
        .get(&discovery_url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| AppError::bad_request(format!("Grok OIDC discovery 失败: {e}")))?
        .error_for_status()
        .map_err(|e| AppError::bad_request(format!("Grok OIDC discovery HTTP {e}")))?;
    let discovery: Value = discovery_response
        .json()
        .await
        .map_err(|e| AppError::bad_request(format!("解析 Grok OIDC discovery 失败: {e}")))?;
    let token_endpoint = discovery
        .get("token_endpoint")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("Grok OIDC discovery 缺少 token_endpoint"))?;

    let mut form = vec![
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", client_id),
    ];
    if let Some(value) = principal_type {
        form.push(("principal_type", value));
    }
    if let Some(value) = principal_id {
        form.push(("principal_id", value));
    }
    let response = client
        .post(token_endpoint)
        .form(&form)
        .send()
        .await
        .map_err(|e| AppError::bad_request(format!("刷新 Grok token 失败: {e}")))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|e| AppError::bad_request(format!("读取 Grok token 响应失败: {e}")))?;
    if !status.is_success() {
        return Err(AppError::bad_request(format!("刷新 Grok token HTTP {status}: {text}")));
    }
    let payload: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::bad_request(format!("解析 Grok token 响应失败: {e}")))?;
    let access_token = payload
        .get("access_token")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("Grok token 响应缺少 access_token"))?
        .to_string();
    Ok((
        access_token,
        payload
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::to_string),
    ))
}

async fn query_grok(
    client: &reqwest::Client,
    refresh_token: &str,
) -> Result<GrokQuotaPayload, AppError> {
    let auth_path = crate::runtime_paths::resolve_current_user_home_or_env()
        .ok_or_else(|| AppError::bad_request("无法解析当前用户 HOME"))?
        .join(".grok")
        .join("auth.json");
    let raw = std::fs::read_to_string(&auth_path)
        .map_err(|e| AppError::bad_request(format!("读取 Grok auth 失败: {e}")))?;
    let auths: std::collections::BTreeMap<String, GrokAuthEntry> = serde_json::from_str(&raw)
        .map_err(|e| AppError::bad_request(format!("解析 Grok auth 失败: {e}")))?;
    let auth = auths
        .into_values()
        .next()
        .ok_or_else(|| AppError::bad_request("Grok auth 为空，请先运行 grok login"))?;
    let (access_token, _rotated_refresh_token) = refresh_grok_access_token(
        client,
        &auth.oidc_issuer,
        &auth.oidc_client_id,
        if refresh_token.trim().is_empty() {
            auth.refresh_token.as_str()
        } else {
            refresh_token
        },
        auth.principal_type.as_deref(),
        auth.principal_id.as_deref(),
    )
    .await?;

    let headers = |request: reqwest::RequestBuilder| {
        request
            .header("Authorization", format!("Bearer {access_token}"))
            .header("X-XAI-Token-Auth", "xai-grok-cli")
            .header("x-userid", &auth.user_id)
            .header("x-grok-client-version", "1.0.40")
            .header("x-grok-client-mode", "interactive")
            .header("x-grok-client-identifier", "grok-shell")
            .header("x-authenticateresponse", "authenticate-response")
            .header("Accept", "application/json")
    };
    let billing_url = "https://cli-chat-proxy.grok.com/v1/billing?format=credits";
    let subscription_url = "https://cli-chat-proxy.grok.com/v1/user?include=subscription";
    let (billing_res, subscription_res) = tokio::join!(
        async {
            headers(client.get(billing_url))
                .send()
                .await
                .map_err(|e| AppError::bad_request(format!("Grok billing 请求失败: {e}")))?
                .error_for_status()
                .map_err(|e| AppError::bad_request(format!("Grok billing HTTP {e}")))?
                .json::<Value>()
                .await
                .map_err(|e| AppError::bad_request(format!("解析 Grok billing 失败: {e}")))
        },
        async {
            headers(client.get(subscription_url))
                .send()
                .await
                .map_err(|e| AppError::bad_request(format!("Grok subscription 请求失败: {e}")))?
                .error_for_status()
                .map_err(|e| AppError::bad_request(format!("Grok subscription HTTP {e}")))?
                .json::<Value>()
                .await
                .map_err(|e| AppError::bad_request(format!("解析 Grok subscription 失败: {e}")))
        }
    );
    Ok(GrokQuotaPayload {
        billing: billing_res?,
        subscription: subscription_res?,
    })
}

/// 调用 MiniMax TokenPlan `/v1/token_plan/remains`：
/// `GET {root}/v1/token_plan/remains`，Authorization 头携带 API key。
/// 响应体直接透传给前端，由前端按平台自定义渲染。
async fn query_minimax(
    client: &reqwest::Client,
    base_url: &str,
    api_key: &str,
) -> Result<Value, AppError> {
    let base = root_base_url(base_url);
    let url = format!("{base}/v1/token_plan/remains");
    let resp = client
        .get(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| AppError::bad_request(format!("MiniMax remains 请求失败: {e}")))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| AppError::bad_request(format!("读取 MiniMax 响应失败: {e}")))?;
    if !status.is_success() {
        return Err(AppError::bad_request(format!("MiniMax remains HTTP {status}: {text}")));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| AppError::bad_request(format!("解析 MiniMax 响应失败: {e}")))?;
    // 兼容直接数组返回或 { data: ... } 嵌套：向上规整到 data 节点，保持前端
    // 渲染逻辑简单。
    Ok(value.get("data").cloned().unwrap_or(value))
}

/// 前端可附带 api_key/base_url 临时覆盖已保存配置，用于在多个 key 之间快速切换查询。
#[derive(Debug, Default, Deserialize)]
pub struct QuotaQueryParams {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub base_url: String,
}

async fn fetch_json(
    client: &reqwest::Client,
    url: &str,
    auth: &str,
    window: Option<(&str, &str)>,
) -> Result<Value, String> {
    let mut req = client
        .get(url)
        .header("Authorization", auth)
        .header("Accept-Language", "en-US,en")
        .header("Content-Type", "application/json");
    if let Some((start, end)) = window {
        req = req.query(&[("startTime", start), ("endTime", end)]);
    }
    let resp = req.send().await.map_err(|e| format!("请求失败: {e}"))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {e}"))?;
    if !status.is_success() {
        return Err(format!("HTTP {status}: {text}"));
    }
    serde_json::from_str::<Value>(&text)
        .map(|v| v.get("data").cloned().unwrap_or(v))
        .map_err(|e| format!("解析响应失败: {e}"))
}

/// 把 API key 脱敏为 `741b…6hWC` 形式。
fn mask_key(key: &str) -> String {
    let len = key.len();
    if len <= 8 {
        "****".to_string()
    } else {
        format!("{}…{}", &key[..4], &key[len - 4..])
    }
}

/// 把任意上游 base_url 规范化为 `scheme://host[:port]`，剥除路径与尾斜杠。
/// 智谱 / MiniMax 监测接口都挂在平台根下，因此需要先去掉 path 后再拼接
/// 具体 endpoint；详见 `query_quota` 内的注释。仅做字符串切分，避免引入
/// `url` crate；无法识别协议时返回去掉尾斜杠的原值。
fn root_base_url(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if let Some(rest) = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
    {
        // host[:port] 是第一个 '/' 之前的部分。
        let host_port = rest.split('/').next().unwrap_or(rest);
        let scheme = if trimmed.starts_with("https://") {
            "https://"
        } else {
            "http://"
        };
        format!("{scheme}{host_port}")
    } else {
        trimmed.to_string()
    }
}

/// GLM 5.3 的模型请求入口迁移到了 `zcode.z.ai`，但套餐监控接口仍由
/// `open.bigmodel.cn` 提供。旧版 bigmodel 端点继续按其自身 origin 查询。
fn zhipu_monitor_base_url(raw: &str) -> String {
    let root = root_base_url(raw);
    if matches!(root.to_ascii_lowercase().as_str(), "https://zcode.z.ai" | "http://zcode.z.ai") {
        DEFAULT_ZHIPU_BASE_URL.to_string()
    } else {
        root
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZhipuQuotaPreflightStatus {
    Available,
    Exhausted { reset_millis: u64 },
    Unavailable,
}

pub(crate) async fn refresh_zhipu_quota_reset_cache(
    presets: &[auth_core::StoredApiPreset],
    cache: &crate::quota_reset_cache::QuotaResetCache,
) -> ZhipuQuotaPreflightStatus {
    refresh_zhipu_quota_reset_cache_with_quota_config(presets, cache, None).await
}

pub(crate) async fn refresh_zhipu_quota_reset_cache_with_quota_config(
    presets: &[auth_core::StoredApiPreset],
    cache: &crate::quota_reset_cache::QuotaResetCache,
    fallback_quota: Option<&QuotaConfig>,
) -> ZhipuQuotaPreflightStatus {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(UPSTREAM_TIMEOUT_SECS))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            warn!("create GLM quota preflight client failed: {error}");
            return ZhipuQuotaPreflightStatus::Unavailable;
        }
    };

    struct QuotaTarget {
        id: String,
        name: String,
        base_url: String,
        api_key: String,
    }

    let mut targets = Vec::new();
    for preset in presets {
        if crate::quota_reset_cache::base_url_is_zhipu_upstream(&preset.base_url)
            && !preset.api_key.trim().is_empty()
        {
            targets.push(QuotaTarget {
                id: preset.id.clone(),
                name: preset.name.clone(),
                base_url: preset.base_url.clone(),
                api_key: preset.api_key.trim().to_string(),
            });
        }
    }

    if let Some(quota) = fallback_quota {
        let key = quota.api_key.trim();
        if !key.is_empty() {
            let base_url = if quota.base_url.trim().is_empty() {
                DEFAULT_ZHIPU_BASE_URL.to_string()
            } else {
                quota.base_url.trim().to_string()
            };
            if crate::quota_reset_cache::base_url_is_zhipu_upstream(&base_url)
                && !targets
                    .iter()
                    .any(|t| t.api_key == key && t.base_url == base_url)
            {
                targets.push(QuotaTarget {
                    id: "quota_config".to_string(),
                    name: "default_quota_config".to_string(),
                    base_url,
                    api_key: key.to_string(),
                });
            }
        }
    }

    if targets.is_empty() {
        return ZhipuQuotaPreflightStatus::Unavailable;
    }

    let requests = targets.into_iter().map(|target| {
        let client = client.clone();
        async move {
            let url = format!(
                "{}/api/monitor/usage/quota/limit",
                zhipu_monitor_base_url(&target.base_url)
            );
            let result = async {
                let response = client
                    .get(url)
                    .header("Authorization", format!("Bearer {}", target.api_key))
                    .header("Accept", "application/json")
                    .send()
                    .await
                    .map_err(|error| error.to_string())?;
                if !response.status().is_success() {
                    return Err(format!("HTTP {}", response.status()));
                }
                response
                    .json::<Value>()
                    .await
                    .map_err(|error| error.to_string())
            }
            .await;
            (target, result)
        }
    });

    let now_millis = current_timestamp_millis();
    let mut has_available_plan = false;
    let mut saw_exhausted_plan = false;
    let mut latest_reset = None::<(u64, String)>;
    for (target, result) in join_all(requests).await {
        let payload = match result {
            Ok(payload) => payload,
            Err(error) => {
                warn!(target_name = %target.name, "GLM quota preflight failed: {error}");
                continue;
            }
        };
        let status = classify_zhipu_token_quota(&payload, now_millis);
        let ZhipuQuotaPreflightStatus::Exhausted { reset_millis } = status else {
            has_available_plan |= matches!(status, ZhipuQuotaPreflightStatus::Available);
            continue;
        };
        saw_exhausted_plan = true;
        let Some(reset_at) = format_reset_time_millis(reset_millis) else {
            continue;
        };
        cache.record_for_preset(&target.id, &target.base_url, reset_at.clone());
        if latest_reset
            .as_ref()
            .is_none_or(|(latest_millis, _)| reset_millis > *latest_millis)
        {
            latest_reset = Some((reset_millis, reset_at));
        }
    }
    if has_available_plan {
        return ZhipuQuotaPreflightStatus::Available;
    }
    if let Some((reset_millis, reset_at)) = latest_reset.filter(|_| saw_exhausted_plan) {
        cache.record_latest_zhipu_reset(reset_at);
        return ZhipuQuotaPreflightStatus::Exhausted { reset_millis };
    }
    ZhipuQuotaPreflightStatus::Unavailable
}

fn parse_zhipu_token_quota_reset_millis(payload: &Value) -> Option<u64> {
    match classify_zhipu_token_quota(payload, 0) {
        ZhipuQuotaPreflightStatus::Exhausted { reset_millis } => Some(reset_millis),
        _ => None,
    }
}

fn classify_zhipu_token_quota(payload: &Value, now_millis: u64) -> ZhipuQuotaPreflightStatus {
    let data = payload.get("data").unwrap_or(payload);
    let Some(limits) = data.get("limits").and_then(Value::as_array) else {
        return ZhipuQuotaPreflightStatus::Unavailable;
    };
    let Some(token_limit) = limits
        .iter()
        .find(|limit| limit.get("type").and_then(Value::as_str) == Some("TOKENS_LIMIT"))
    else {
        return ZhipuQuotaPreflightStatus::Unavailable;
    };
    let Some(percentage) = token_limit.get("percentage").and_then(Value::as_f64) else {
        return ZhipuQuotaPreflightStatus::Unavailable;
    };
    if percentage < 100.0 {
        return ZhipuQuotaPreflightStatus::Available;
    }
    let Some(reset_millis) = token_limit.get("nextResetTime").and_then(Value::as_u64) else {
        return ZhipuQuotaPreflightStatus::Unavailable;
    };
    if reset_millis <= now_millis {
        return ZhipuQuotaPreflightStatus::Unavailable;
    }
    ZhipuQuotaPreflightStatus::Exhausted { reset_millis }
}

fn format_reset_time_millis(timestamp_millis: u64) -> Option<String> {
    let timestamp_seconds = i64::try_from(timestamp_millis / 1000).ok()?;
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    OffsetDateTime::from_unix_timestamp(timestamp_seconds)
        .ok()?
        .to_offset(offset)
        .format(format_description!("[year]-[month]-[day] [hour]:[minute]:[second]"))
        .ok()
}

fn current_timestamp_millis() -> u64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    u64::try_from(now).unwrap_or(u64::MAX)
}

/// 计算查询窗口：昨天当前整点 ~ 今天当前小时末，使用 `time` crate 本地时区。
fn format_query_window() -> (String, String) {
    let fmt = format_description!("[year]-[month]-[day] [hour]:[minute]:[second]");
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let start = now - TimeDuration::hours(24);
    let start_floor = start
        .replace_minute(0)
        .unwrap_or(start)
        .replace_second(0)
        .unwrap_or(start);
    let end_floor = now
        .replace_minute(59)
        .unwrap_or(now)
        .replace_second(59)
        .unwrap_or(now);
    let start_str = start_floor.format(fmt).unwrap_or_default();
    let end_str = end_floor.format(fmt).unwrap_or_default();
    (start_str, end_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_platform_zhipu_default() {
        assert_eq!(
            detect_platform("https://open.bigmodel.cn/api/coding/paas/v4"),
            QuotaPlatform::Zhipu
        );
        // 任何与智谱无关的 upstream 都保持 Zhipu 作为安全默认值，
        // 避免把完全无关的 base_url 误派到 MiniMax。
        assert_eq!(detect_platform("https://api.openai.com/v1"), QuotaPlatform::Zhipu);
        assert_eq!(detect_platform(""), QuotaPlatform::Zhipu);
    }

    #[test]
    fn parses_glm_token_plan_reset_time() {
        let payload = json!({
            "code": 200,
            "success": true,
            "data": {
                "limits": [
                    { "type": "TIME_LIMIT", "nextResetTime": 1788449868998_u64 },
                    { "type": "TOKENS_LIMIT", "percentage": 100, "nextResetTime": 1786781764932_u64 }
                ]
            }
        });

        assert_eq!(parse_zhipu_token_quota_reset_millis(&payload), Some(1786781764932));
    }

    #[test]
    fn classifies_glm_token_plan_with_remaining_quota_as_available() {
        let payload = json!({
            "code": 200,
            "success": true,
            "data": {
                "limits": [
                    { "type": "TOKENS_LIMIT", "percentage": 42, "nextResetTime": 1786781764932_u64 }
                ]
            }
        });

        assert_eq!(
            classify_zhipu_token_quota(&payload, 1_700_000_000_000),
            ZhipuQuotaPreflightStatus::Available,
        );
    }

    #[test]
    fn classifies_exhausted_glm_token_plan_for_scheduled_retry() {
        let reset_millis = 1_786_781_764_932_u64;
        let payload = json!({
            "code": 200,
            "success": true,
            "data": {
                "limits": [
                    { "type": "TOKENS_LIMIT", "percentage": 100, "nextResetTime": reset_millis }
                ]
            }
        });

        assert_eq!(
            classify_zhipu_token_quota(&payload, reset_millis - 1),
            ZhipuQuotaPreflightStatus::Exhausted { reset_millis },
        );
    }

    #[test]
    fn unavailable_official_quota_probe_does_not_create_latest_reset_schedule() {
        let cache = crate::quota_reset_cache::QuotaResetCache::new();

        // This is the exact production defect: the provider selector sees a
        // GLM model, so the official account is probed, but every official
        // quota request fails. A network/auth failure is not proof that any
        // plan is exhausted, so the shared reset schedule must stay empty and
        // the scanner must fall back to the normal 429 retry policy.
        let presets = [auth_core::StoredApiPreset {
            interchange_source: None,
            id: "api-official-unavailable".to_string(),
            name: "official unavailable".to_string(),
            saved_at: 1,
            provider_name: String::new(),
            base_url: "https://open.bigmodel.cn/api/coding/paas/v4".to_string(),
            management_url: None,
            wire_api: None,
            responses_proxy: None,
            apply_upstream_proxy_on_switch: false,
            config_overrides: Vec::new(),
            legacy_config_key: None,
            legacy_config_value: None,
            legacy_secondary_config_key: None,
            legacy_secondary_config_value: None,
            terminal_env: Vec::new(),
            terminal_startup_script: None,
            api_key: "test-key".to_string(),
            access_token: String::new(),
            account_id: String::new(),
            access_mode: None,
            favorite: false,
            switch_count: 0,
        }];

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        let status = runtime.block_on(refresh_zhipu_quota_reset_cache(&presets, &cache));

        assert_eq!(status, ZhipuQuotaPreflightStatus::Unavailable);
        assert_eq!(cache.get_latest_zhipu_reset(), None);
    }

    #[test]
    fn fallback_quota_config_handles_empty_or_non_zhipu_gracefully() {
        let cache = crate::quota_reset_cache::QuotaResetCache::new();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");

        let empty_quota = QuotaConfig {
            api_key: String::new(),
            base_url: DEFAULT_ZHIPU_BASE_URL.to_string(),
            default_provider: DefaultQuotaProvider::Saved,
            custom_preset_id: None,
        };
        let status = runtime.block_on(refresh_zhipu_quota_reset_cache_with_quota_config(
            &[],
            &cache,
            Some(&empty_quota),
        ));
        assert_eq!(status, ZhipuQuotaPreflightStatus::Unavailable);
        assert_eq!(cache.get_latest_zhipu_reset(), None);
    }

    #[test]
    fn detect_platform_minimax_hosts() {
        // 国内版：minimaxi.com
        assert_eq!(detect_platform("https://www.minimaxi.com/v1"), QuotaPlatform::MiniMax);
        assert_eq!(
            detect_platform("https://api.minimaxi.com/v1/token_plan/remains"),
            QuotaPlatform::MiniMax
        );
        // 国际版：minimax.io
        assert_eq!(detect_platform("https://api.minimax.io/v1"), QuotaPlatform::MiniMax);
        // 经过本地 codex-proxy 转发
        assert_eq!(
            detect_platform("http://127.0.0.1:11111/api/codex-proxy/minimax/v1"),
            QuotaPlatform::MiniMax
        );
    }

    #[test]
    fn root_base_url_strips_path_and_trailing_slash() {
        assert_eq!(
            root_base_url("https://open.bigmodel.cn/api/coding/paas/v4"),
            "https://open.bigmodel.cn"
        );
        assert_eq!(root_base_url("https://www.minimaxi.com/v1/"), "https://www.minimaxi.com");
        assert_eq!(root_base_url("http://api.minimax.io"), "http://api.minimax.io");
    }

    #[test]
    fn zhipu_monitor_base_url_maps_zcode_model_endpoint_to_bigmodel_monitoring() {
        assert_eq!(
            zhipu_monitor_base_url("https://zcode.z.ai/api/v1/ultra/anthropic"),
            "https://open.bigmodel.cn"
        );
        assert_eq!(
            zhipu_monitor_base_url("https://open.bigmodel.cn/api/coding/paas/v4"),
            "https://open.bigmodel.cn"
        );
    }

    #[test]
    fn default_provider_parses_custom_and_openrouter_strings() {
        // 前端「默认显示」下拉会提交 CUSTOM / OPENROUTER 字符串；反序列化
        // 必须接受，且 roundtrip 后字符串稳定。
        assert_eq!(
            serde_json::from_str::<DefaultQuotaProvider>("\"CUSTOM\"").unwrap(),
            DefaultQuotaProvider::Custom
        );
        assert_eq!(
            serde_json::from_str::<DefaultQuotaProvider>("\"OPENROUTER\"").unwrap(),
            DefaultQuotaProvider::OpenRouter
        );
        assert_eq!(serde_json::to_string(&DefaultQuotaProvider::Custom).unwrap(), "\"CUSTOM\"");
        assert_eq!(
            serde_json::to_string(&DefaultQuotaProvider::OpenRouter).unwrap(),
            "\"OPENROUTER\""
        );
    }

    #[test]
    fn quota_config_update_persists_and_normalizes_custom_preset_id() {
        let dir = std::env::temp_dir().join(format!(
            "webclx-quota-cfg-test-{}-{}",
            std::process::id(),
            current_timestamp_millis()
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let manager = QuotaConfigManager::load(&dir);
        assert_eq!(manager.inner.blocking_read().custom_preset_id, None);

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        runtime
            .block_on(manager.update(
                "key-1",
                "https://open.bigmodel.cn",
                DefaultQuotaProvider::Custom,
                Some("  preset-42  "),
            ))
            .expect("update config");
        // 空白 id 归一为未指定，而不是存进无效引用。
        runtime
            .block_on(manager.update(
                "key-1",
                "https://open.bigmodel.cn",
                DefaultQuotaProvider::Saved,
                Some("   "),
            ))
            .expect("update config with blank id");
        assert_eq!(manager.inner.blocking_read().custom_preset_id, None);

        runtime
            .block_on(manager.update(
                "key-1",
                "https://open.bigmodel.cn",
                DefaultQuotaProvider::Custom,
                Some("preset-42"),
            ))
            .expect("update config with preset id");
        // 从磁盘重新加载，确认持久化且 trim 生效。
        let reloaded = QuotaConfigManager::load(&dir);
        let snapshot = runtime.block_on(reloaded.snapshot());
        assert_eq!(snapshot.custom_preset_id.as_deref(), Some("preset-42"));
        assert_eq!(snapshot.default_provider, DefaultQuotaProvider::Custom);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
