use std::{
    fs, io,
    net::SocketAddr,
    path::{Path, PathBuf},
};

use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::{
    extract::{ConnectInfo, Request, State},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};

use crate::{AppState, login};

pub const LOCAL_API_TOKEN_FILE_NAME: &str = ".webclx-local-api-token";
pub const LOCAL_API_TOKEN_HEADER: &str = auth_core::WEBCLX_LOCAL_API_TOKEN_HEADER;
const LOCAL_API_TOKEN_BYTES: usize = 32;

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

fn is_local_api_request(addr: SocketAddr, headers: &HeaderMap, expected_token: &str) -> bool {
    if !addr.ip().is_loopback() || expected_token.is_empty() {
        return false;
    }
    let Some(provided) = headers
        .get(LOCAL_API_TOKEN_HEADER)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    provided.len() == expected_token.len()
        && bool::from(subtle::ConstantTimeEq::ct_eq(provided.as_bytes(), expected_token.as_bytes()))
}

/// 认证中间件：校验会话 cookie；本地工具必须同时来自 loopback 并持有本地令牌。
///
/// 对未认证的非页面请求返回 401，对浏览器页面请求返回 302 重定向到登录页。
/// 使用 from_fn_with_state 注入 AppState。
pub async fn require_auth(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    mut request: Request,
    next: Next,
) -> Response {
    if is_local_api_request(addr, request.headers(), &state.local_api_token) {
        request.headers_mut().remove(LOCAL_API_TOKEN_HEADER);
        return next.run(request).await;
    }

    let path = request.uri().path().to_string();

    if login::verify_session_from_headers(request.headers(), &state).is_some() {
        return next.run(request).await;
    }

    // 未认证。对 API 请求（/api/）返回 401，对页面请求重定向到登录页。
    if path.starts_with("/api/") {
        let mut resp = (StatusCode::UNAUTHORIZED, "未登录或会话已过期").into_response();
        resp.headers_mut()
            .insert("X-WebClx-Auth", HeaderValue::from_static("required"));
        tracing::debug!(path = %path, addr = %addr, "auth_guard: 401 for unauthenticated request");
        resp
    } else {
        tracing::debug!(path = %path, addr = %addr, "auth_guard: redirect to login");
        Redirect::to("/login").into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn headers(token: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(token) = token {
            headers.insert("x-webclx-local-token", token.parse().unwrap());
        }
        headers
    }

    #[test]
    fn loopback_requires_the_server_generated_local_token() {
        let addr = "127.0.0.1:45678".parse().unwrap();

        assert!(!is_local_api_request(addr, &headers(None), TOKEN));
        assert!(!is_local_api_request(addr, &headers(Some("wrong")), TOKEN));
        assert!(is_local_api_request(addr, &headers(Some(TOKEN)), TOKEN));
    }

    #[test]
    fn non_loopback_is_not_exempt_even_with_the_local_token() {
        let addr = "192.168.3.2:45678".parse().unwrap();

        assert!(!is_local_api_request(addr, &headers(Some(TOKEN)), TOKEN));
    }

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
