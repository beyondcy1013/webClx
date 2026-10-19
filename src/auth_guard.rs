use std::{
    collections::HashMap,
    fs, io,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

use axum::{
    extract::{ConnectInfo, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Redirect, Response},
};
use subtle::ConstantTimeEq;

use crate::{AppState, login};

pub const LOCAL_API_TOKEN_FILE_NAME: &str = ".webclx-local-api-token";
pub const LOCAL_API_TOKEN_HEADER: &str = auth_core::WEBCLX_LOCAL_API_TOKEN_HEADER;
const LOCAL_API_TOKEN_BYTES: usize = 32;

/// 未认证尝试频控与熔断配置
const UNAUTH_WINDOW: Duration = Duration::from_secs(300); // 统计时间窗口 5 分钟 (300 秒)
const UNAUTH_MAX_ATTEMPTS: usize = 10; // 5 分钟内最多允许 10 次未认证请求
const UNAUTH_BLOCK_DURATION: Duration = Duration::from_secs(900); // 触发熔断后封禁 15 分钟 (900 秒)
const MAX_TRACKED_IPS: usize = 5000; // 最多追踪 5000 个 IP，防止内存膨胀

#[derive(Clone, Debug)]
struct IpFailures {
    timestamps: Vec<Instant>,
    blocked_until: Option<Instant>,
}

static UNAUTH_TRACKER: std::sync::LazyLock<Mutex<HashMap<IpAddr, IpFailures>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));

/// 检查 IP 是否处于未认证熔断封禁状态
fn check_unauth_rate_limit(ip: IpAddr) -> Option<Duration> {
    let now = Instant::now();
    let mut tracker = UNAUTH_TRACKER.lock().ok()?;
    if let Some(record) = tracker.get_mut(&ip) {
        if let Some(blocked_until) = record.blocked_until {
            if now < blocked_until {
                return Some(blocked_until.duration_since(now));
            } else {
                record.blocked_until = None;
            }
        }
    }
    None
}

/// 记录一次未认证失败，如果超出阈值则触发熔断封禁，返回封禁时长
fn record_unauth_failure(ip: IpAddr) -> Option<Duration> {
    let now = Instant::now();
    let mut tracker = UNAUTH_TRACKER.lock().ok()?;

    if tracker.len() > MAX_TRACKED_IPS {
        tracker.retain(|_, record| {
            if let Some(until) = record.blocked_until {
                now < until
            } else {
                record
                    .timestamps
                    .iter()
                    .any(|&ts| now.duration_since(ts) < UNAUTH_WINDOW)
            }
        });
    }

    let record = tracker.entry(ip).or_insert_with(|| IpFailures {
        timestamps: Vec::new(),
        blocked_until: None,
    });

    if let Some(blocked_until) = record.blocked_until {
        if now < blocked_until {
            return Some(blocked_until.duration_since(now));
        } else {
            record.blocked_until = None;
        }
    }

    record
        .timestamps
        .retain(|&ts| now.duration_since(ts) < UNAUTH_WINDOW);
    record.timestamps.push(now);

    if record.timestamps.len() >= UNAUTH_MAX_ATTEMPTS {
        let block_until = now + UNAUTH_BLOCK_DURATION;
        record.blocked_until = Some(block_until);
        record.timestamps.clear();
        tracing::warn!(
            %ip,
            attempts = UNAUTH_MAX_ATTEMPTS,
            block_secs = UNAUTH_BLOCK_DURATION.as_secs(),
            "auth_guard: rate limit triggered, IP blocked due to excessive unauthorized requests"
        );
        return Some(UNAUTH_BLOCK_DURATION);
    }

    None
}

/// 认证成功时清除该 IP 的失败记录
pub fn clear_unauth_failures(ip: IpAddr) {
    if let Ok(mut tracker) = UNAUTH_TRACKER.lock() {
        tracker.remove(&ip);
    }
}

#[cfg(test)]
pub fn reset_unauth_tracker_for_tests() {
    if let Ok(mut tracker) = UNAUTH_TRACKER.lock() {
        tracker.clear();
    }
}

/// 进程级本地令牌，供内部 HTTP 调用附加 `X-WebClx-Local-Token`。
static SERVER_LOCAL_API_TOKEN: std::sync::OnceLock<std::sync::Arc<str>> =
    std::sync::OnceLock::new();

pub fn set_server_local_api_token(token: std::sync::Arc<str>) {
    let _ = SERVER_LOCAL_API_TOKEN.set(token);
}

pub fn server_local_api_token() -> Option<&'static str> {
    SERVER_LOCAL_API_TOKEN.get().map(|token| token.as_ref())
}

pub fn local_api_token_path(app_dir: &Path) -> PathBuf {
    app_dir.join(LOCAL_API_TOKEN_FILE_NAME)
}

pub fn load_or_create_local_api_token(app_dir: &Path) -> io::Result<String> {
    let path = local_api_token_path(app_dir);
    if crate::private_file::validate_optional_existing(&path)?
        && let Ok(token) = read_existing_local_api_token(&path)
    {
        crate::private_file::tighten_permissions(&path)?;
        return Ok(token);
    }

    use rand::RngCore;
    let mut bytes = [0_u8; LOCAL_API_TOKEN_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    write_local_api_token(&path, &token)?;
    Ok(token)
}

pub fn read_existing_local_api_token(path: &Path) -> io::Result<String> {
    crate::private_file::validate_existing(path)?;
    let token = fs::read_to_string(path)?;
    let token = token.trim();
    if token.len() != LOCAL_API_TOKEN_BYTES * 2
        || !token.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "local API token must be 64 hexadecimal characters",
        ));
    }
    Ok(token.to_string())
}

fn write_local_api_token(path: &Path, token: &str) -> io::Result<()> {
    crate::private_file::write_atomic(path, format!("{token}\n").as_bytes())
}

fn is_api_request(path: &str) -> bool {
    path.starts_with("/api/")
}

fn has_valid_local_api_token(headers: &HeaderMap, addr: SocketAddr, expected: &str) -> bool {
    if !addr.ip().is_loopback() || expected.is_empty() {
        return false;
    }
    let Some(provided) = headers
        .get(LOCAL_API_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    provided.as_bytes().ct_eq(expected.as_bytes()).into()
}

/// 网关认证由其 handler 负责；内部管理令牌不得转发给上游。
pub async fn strip_local_api_token(mut request: Request, next: Next) -> Response {
    request.headers_mut().remove(LOCAL_API_TOKEN_HEADER);
    next.run(request).await
}

/// 管理 API 需要有效会话，或来自 loopback 的本机令牌。页面需要有效会话。
/// 网关不注册在这层路由中，由自身来源和上游凭据策略校验。
///
/// 对未认证页面请求返回 302 重定向到登录页。
/// 使用 from_fn_with_state 注入 AppState。
pub async fn require_auth(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();
    let local_api = is_api_request(&path)
        && has_valid_local_api_token(request.headers(), addr, &state.local_api_token);
    if local_api || login::verify_session_from_headers(request.headers(), &state).is_some() {
        request.headers_mut().remove(LOCAL_API_TOKEN_HEADER);
        clear_unauth_failures(addr.ip());
        return next.run(request).await;
    }

    if is_api_request(&path) {
        if let Some(retry_after) = check_unauth_rate_limit(addr.ip()) {
            let retry_secs = retry_after.as_secs().max(1);
            let mut headers = HeaderMap::new();
            if let Ok(value) = retry_secs.to_string().parse() {
                headers.insert(axum::http::header::RETRY_AFTER, value);
            }
            return (
                StatusCode::TOO_MANY_REQUESTS,
                headers,
                Json(serde_json::json!({
                    "error": format!("未授权请求过多已被临时限制调用，请等待 {retry_secs} 秒后重试"),
                    "retry_after_seconds": retry_secs,
                })),
            )
                .into_response();
        }

        if let Some(block_duration) = record_unauth_failure(addr.ip()) {
            let retry_secs = block_duration.as_secs().max(1);
            let mut headers = HeaderMap::new();
            if let Ok(value) = retry_secs.to_string().parse() {
                headers.insert(axum::http::header::RETRY_AFTER, value);
            }
            return (
                StatusCode::TOO_MANY_REQUESTS,
                headers,
                Json(serde_json::json!({
                    "error": format!("未授权请求过于频繁已触发安全熔断，请等待 {retry_secs} 秒后重试"),
                    "retry_after_seconds": retry_secs,
                })),
            )
                .into_response();
        }

        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({
                "error": "请先登录，或从本机携带有效的 X-WebClx-Local-Token 调用",
            })),
        )
            .into_response();
    }

    tracing::debug!(path = %path, addr = %addr, "auth_guard: redirect to login");
    Redirect::to("/login").into_response()
}

#[cfg(test)]
#[path = "auth_guard/http_tests.rs"]
mod http_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_api_token_is_persistent_and_permission_restricted() {
        let directory = std::env::temp_dir().join(format!(
            "webclx-local-api-token-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();

        let first = load_or_create_local_api_token(&directory).unwrap();
        let second = load_or_create_local_api_token(&directory).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), LOCAL_API_TOKEN_BYTES * 2);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(local_api_token_path(&directory))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }

        fs::remove_dir_all(directory).unwrap();
    }
}
