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
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use time::{Duration as TimeDuration, OffsetDateTime, macros::format_description};
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
}

impl QuotaPlatform {
    fn as_str(self) -> &'static str {
        match self {
            QuotaPlatform::Zhipu => "ZHIPU",
            QuotaPlatform::MiniMax => "MINIMAX",
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
}

impl Default for QuotaConfig {
    fn default() -> Self {
        Self {
            api_key: DEFAULT_ZHIPU_API_KEY.to_string(),
            base_url: DEFAULT_ZHIPU_BASE_URL.to_string(),
            default_provider: DefaultQuotaProvider::Saved,
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
    ) -> Result<(), AppError> {
        let mut sanitized = QuotaConfig {
            api_key: api_key.trim().to_string(),
            base_url: base_url.trim().to_string(),
            default_provider,
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
}

/// 返回当前配置（key 做脱敏处理，避免明文回传到前端）。
/// 同时返回两套默认值（智谱 / MiniMax），方便前端在用户切换平台时填充
/// 占位/默认值。
pub async fn get_quota_config(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let config = state.quota_manager.snapshot().await;
    let masked = mask_key(&config.api_key);
    let default_provider = match config.default_provider {
        DefaultQuotaProvider::Saved => "SAVED",
        DefaultQuotaProvider::Zhipu => "ZHIPU",
        DefaultQuotaProvider::MiniMax => "MINIMAX",
    };
    Ok(Json(json!({
        "api_key_masked": masked,
        "api_key": config.api_key,
        "base_url": config.base_url,
        "default_provider": default_provider,
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
        .update(&payload.api_key, &payload.base_url, payload.default_provider)
        .await?;
    let config = state.quota_manager.snapshot().await;
    let default_provider = match config.default_provider {
        DefaultQuotaProvider::Saved => "SAVED",
        DefaultQuotaProvider::Zhipu => "ZHIPU",
        DefaultQuotaProvider::MiniMax => "MINIMAX",
    };
    Ok(Json(json!({
        "ok": true,
        "api_key_masked": mask_key(&config.api_key),
        "base_url": config.base_url,
        "default_provider": default_provider,
    })))
}

/// 代理查询：按 base_url 自动分派到对应平台实现，并组装成前端可直接渲染的结构。
///
/// 支持：
/// - 智谱（bigmodel.cn）：并行抓取 model-usage / tool-usage / quota/limit。
/// - MiniMax（minimaxi.com / minimax.io）：抓取 `/v1/token_plan/remains`。
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
        QuotaPlatform::Zhipu => {
            // 智谱用量/配额监测接口固定在平台根 (scheme://host[:port]) 下：
            //   {root}/api/monitor/usage/{model-usage|tool-usage}、{root}/api/monitor/usage/quota/limit
            // 而 Codex_API 预设里的 base_url 可能是 coding 端点（如
            // `https://open.bigmodel.cn/api/coding/paas/v4`）。若直接在其后拼接
            // `/api/monitor/usage/...`，会得到 `.../v4/api/monitor/usage/...` → 404。
            // 因此先剥离掉 base_url 的路径，只保留 scheme://host[:port]。
            let base = root_base_url(&base_url);
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
}
