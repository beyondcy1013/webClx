use std::{
    collections::HashMap,
    io::Read as _,
    sync::{Arc, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose};
use reqwest::StatusCode as HttpStatusCode;
use serde_json::Value;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::{
    AuthFile, AuthPresetDetails, AuthTokens, CODEX_DEVICE_DEFAULT_POLL_INTERVAL_SECS,
    CODEX_DEVICE_TIMEOUT_SECS, CODEX_DEVICE_TOKEN_EXCHANGE_REDIRECT_URI, CODEX_DEVICE_TOKEN_URL,
    CODEX_DEVICE_USER_CODE_URL, CODEX_DEVICE_VERIFICATION_URL, CODEX_OAUTH_CLIENT_ID,
    CODEX_OAUTH_SESSION_RETENTION_SECS, CODEX_OAUTH_TOKEN_URL, CODEX_USAGE_URL,
    CODEX_USAGE_USER_AGENT, CodexAuthHttpClientProvider, CodexAuthorizationCodeTokenResponse,
    CodexDeviceTokenRequest, CodexDeviceTokenResponse, CodexDeviceUserCodeRequest,
    CodexDeviceUserCodeResponse, CodexOAuthManager, CodexOAuthSession, CodexOAuthSessionResponse,
    CodexOAuthSessionStatus, CodexRefreshTokenResponse, CodexRemoteError, CodexUsageResponse,
    PendingCodexDeviceLogin, StoredAuthPreset, current_timestamp_secs, decode_jwt_payload,
    derive_auth_preset_details, first_json_string, merge_refreshed_auth_preset_details,
    short_account_id, validate_auth_file, validate_auth_file_sync,
};

impl Default for CodexOAuthManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CodexOAuthManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn insert_pending(
        &self,
        verification_url: &str,
        authorize_url: &str,
        user_code: &str,
        poll_interval_seconds: u64,
    ) -> CodexOAuthSession {
        let now = current_timestamp_secs();
        let session = CodexOAuthSession {
            id: generate_codex_oauth_session_id(),
            status: CodexOAuthSessionStatus::Pending,
            verification_url: verification_url.to_string(),
            authorize_url: authorize_url.to_string(),
            user_code: user_code.to_string(),
            poll_interval_seconds: poll_interval_seconds.max(1),
            created_at: now,
            updated_at: now,
            expires_at: now.saturating_add(CODEX_DEVICE_TIMEOUT_SECS),
            error: None,
            auth: None,
            details: None,
            suggested_name: None,
        };

        let mut sessions = self
            .sessions
            .write()
            .expect("codex oauth session manager poisoned");
        cleanup_codex_oauth_sessions_locked(&mut sessions);
        sessions.insert(session.id.clone(), session.clone());
        session
    }

    pub fn get(&self, session_id: &str) -> Option<CodexOAuthSession> {
        let mut sessions = self
            .sessions
            .write()
            .expect("codex oauth session manager poisoned");
        cleanup_codex_oauth_sessions_locked(&mut sessions);
        let now = current_timestamp_secs();
        if let Some(session) = sessions.get_mut(session_id) {
            if session.status == CodexOAuthSessionStatus::Pending && now >= session.expires_at {
                session.status = CodexOAuthSessionStatus::Expired;
                session.updated_at = now;
                session.error = Some("等待官网登录超时，请重新发起登录。".to_string());
            }
            return Some(session.clone());
        }
        None
    }

    pub fn complete(&self, session_id: &str, auth: AuthFile, details: AuthPresetDetails) {
        let mut sessions = self
            .sessions
            .write()
            .expect("codex oauth session manager poisoned");
        cleanup_codex_oauth_sessions_locked(&mut sessions);
        if let Some(session) = sessions.get_mut(session_id) {
            session.status = CodexOAuthSessionStatus::Completed;
            session.updated_at = current_timestamp_secs();
            session.error = None;
            session.suggested_name = Some(suggest_auth_preset_name(&details, &auth));
            session.auth = Some(auth);
            session.details = Some(details);
        }
    }

    pub fn fail(&self, session_id: &str, error: impl Into<String>) {
        let mut sessions = self
            .sessions
            .write()
            .expect("codex oauth session manager poisoned");
        cleanup_codex_oauth_sessions_locked(&mut sessions);
        if let Some(session) = sessions.get_mut(session_id) {
            session.status = CodexOAuthSessionStatus::Error;
            session.updated_at = current_timestamp_secs();
            session.error = Some(error.into());
        }
    }
}

pub async fn refresh_stored_auth_preset_quota(
    proxy_manager: &impl CodexAuthHttpClientProvider,
    preset: &mut StoredAuthPreset,
) -> Result<()> {
    let usage = refresh_codex_usage_for_auth(proxy_manager, &mut preset.auth).await?;
    preset.details = merge_refreshed_auth_preset_details(&preset.details, &preset.auth, &usage);
    preset.saved_at = current_timestamp_secs();
    Ok(())
}

async fn refresh_codex_usage_for_auth(
    proxy_manager: &impl CodexAuthHttpClientProvider,
    auth: &mut AuthFile,
) -> Result<CodexUsageResponse> {
    match fetch_codex_usage(proxy_manager, &auth.tokens.access_token, &auth.tokens.account_id).await
    {
        Ok(usage) => {
            touch_auth_last_refresh(auth)?;
            Ok(usage)
        }
        Err(error)
            if matches!(
                error.status,
                Some(HttpStatusCode::UNAUTHORIZED | HttpStatusCode::FORBIDDEN)
            ) =>
        {
            refresh_codex_tokens(proxy_manager, auth).await?;
            let usage = fetch_codex_usage(
                proxy_manager,
                &auth.tokens.access_token,
                &auth.tokens.account_id,
            )
            .await
            .map_err(|error| {
                anyhow::anyhow!("刷新额度失败: {}", format_codex_remote_error(&error))
            })?;
            touch_auth_last_refresh(auth)?;
            Ok(usage)
        }
        Err(error) => Err(anyhow::anyhow!("刷新额度失败: {}", format_codex_remote_error(&error))),
    }
}

async fn refresh_codex_tokens(
    proxy_manager: &impl CodexAuthHttpClientProvider,
    auth: &mut AuthFile,
) -> Result<()> {
    if auth.tokens.refresh_token.trim().is_empty() {
        anyhow::bail!("刷新 Codex token 失败: 当前账号没有 refresh_token，请重新导入完整凭据。");
    }

    let client = proxy_manager
        .build_auth_client(15)
        .context("创建 Codex 刷新客户端失败")?;

    let response = client
        .post(CODEX_OAUTH_TOKEN_URL)
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&[
            ("client_id", CODEX_OAUTH_CLIENT_ID),
            ("grant_type", "refresh_token"),
            ("refresh_token", auth.tokens.refresh_token.as_str()),
            ("scope", "openid profile email"),
        ])
        .send()
        .await
        .context("刷新 Codex token 失败")?;

    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!(
            "刷新 Codex token 失败: HTTP {}{}",
            status.as_u16(),
            summarize_remote_body(&body)
        );
    }

    let payload: CodexRefreshTokenResponse = response
        .json()
        .await
        .context("解析 Codex token 刷新响应失败")?;

    let new_access_token = payload.access_token.trim();
    if new_access_token.is_empty() {
        anyhow::bail!("刷新 Codex token 失败: 返回了空 access_token。");
    }

    let previous_account_id = auth.tokens.account_id.clone();
    let previous_id_token = auth.tokens.id_token.clone();
    let previous_refresh_token = auth.tokens.refresh_token.clone();

    auth.tokens.access_token = new_access_token.to_string();
    if !payload.id_token.trim().is_empty() {
        auth.tokens.id_token = payload.id_token.trim().to_string();
    }
    if !payload.refresh_token.trim().is_empty() {
        auth.tokens.refresh_token = payload.refresh_token.trim().to_string();
    }

    if auth.tokens.id_token.trim().is_empty() {
        auth.tokens.id_token = previous_id_token;
    }
    if auth.tokens.refresh_token.trim().is_empty() {
        auth.tokens.refresh_token = previous_refresh_token;
    }

    auth.tokens.account_id = extract_account_id_from_auth(auth).unwrap_or(previous_account_id);
    Ok(())
}

pub fn touch_auth_last_refresh(auth: &mut AuthFile) -> Result<()> {
    auth.last_refresh = current_rfc3339_timestamp();
    validate_auth_file(auth)?;
    Ok(())
}

async fn fetch_codex_usage(
    proxy_manager: &impl CodexAuthHttpClientProvider,
    access_token: &str,
    account_id: &str,
) -> std::result::Result<CodexUsageResponse, CodexRemoteError> {
    let client = proxy_manager
        .build_auth_client(15)
        .map_err(|error| CodexRemoteError {
            status: None,
            message: format!("创建 Codex 配额客户端失败: {error}"),
        })?;

    let mut request = client
        .get(CODEX_USAGE_URL)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", access_token.trim()))
        .header(reqwest::header::USER_AGENT, CODEX_USAGE_USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/json");
    if !account_id.trim().is_empty() {
        request = request.header("ChatGPT-Account-Id", account_id.trim());
    }

    let response = request.send().await.map_err(|error| {
        let mut message = format!("请求 Codex 配额失败: {error}");
        if proxy_manager.active_proxy_server().is_none() && (error.is_connect() || error.is_timeout()) {
            message.push_str(
                "。当前未启用 webclx 应用内代理，且 webclx 不继承 shell 里的代理环境；请在“代理”页启用可用代理后重试。",
            );
        }
        CodexRemoteError {
            status: None,
            message,
        }
    })?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(CodexRemoteError {
            status: Some(status),
            message: format!("HTTP {}{}", status.as_u16(), summarize_remote_body(&body)),
        });
    }

    response.json().await.map_err(|error| CodexRemoteError {
        status: Some(status),
        message: format!("解析 Codex 配额响应失败: {error}"),
    })
}

pub fn extract_account_id_from_auth(auth: &AuthFile) -> Option<String> {
    let id_payload = decode_jwt_payload(&auth.tokens.id_token);
    let access_payload = decode_jwt_payload(&auth.tokens.access_token);
    first_json_string(&[
        id_payload
            .as_ref()
            .and_then(|payload| payload.get("https://api.openai.com/auth"))
            .and_then(|claim| claim.get("chatgpt_account_id"))
            .and_then(Value::as_str),
        access_payload
            .as_ref()
            .and_then(|payload| payload.get("https://api.openai.com/auth"))
            .and_then(|claim| claim.get("chatgpt_account_id"))
            .and_then(Value::as_str),
        Some(auth.tokens.account_id.as_str()),
    ])
}

fn current_rfc3339_timestamp() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_string())
}

pub fn summarize_remote_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    if let Some(message) = extract_remote_error_message(trimmed) {
        return format!(": {message}");
    }

    let summary = truncate_chars(trimmed, 240);
    format!(": {summary}")
}

fn format_codex_remote_error(error: &CodexRemoteError) -> String {
    if error.message.trim().is_empty() {
        "远端没有返回可读错误信息。".to_string()
    } else {
        error.message.clone()
    }
}

fn extract_remote_error_message(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    first_json_string(&[
        value
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(Value::as_str),
        value.get("message").and_then(Value::as_str),
        value.get("detail").and_then(Value::as_str),
    ])
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let mut truncated = String::new();
    for _ in 0..max_chars {
        let Some(ch) = chars.next() else {
            return value.to_string();
        };
        truncated.push(ch);
    }
    if chars.next().is_some() {
        truncated.push_str("...");
    }
    truncated
}

pub fn codex_oauth_session_response(session: &CodexOAuthSession) -> CodexOAuthSessionResponse {
    CodexOAuthSessionResponse {
        ok: true,
        session_id: session.id.clone(),
        status: session.status,
        verification_url: session.verification_url.clone(),
        authorize_url: session.authorize_url.clone(),
        user_code: session.user_code.clone(),
        poll_interval_seconds: session.poll_interval_seconds,
        created_at: session.created_at,
        updated_at: session.updated_at,
        expires_at: session.expires_at,
        error: session.error.clone(),
        auth: session.auth.clone(),
        details: session.details.clone(),
        suggested_name: session.suggested_name.clone(),
    }
}

fn cleanup_codex_oauth_sessions_locked(sessions: &mut HashMap<String, CodexOAuthSession>) {
    let now = current_timestamp_secs();
    sessions.retain(|_, session| {
        if session.status == CodexOAuthSessionStatus::Pending && now >= session.expires_at {
            session.status = CodexOAuthSessionStatus::Expired;
            session.updated_at = now;
            session.error = Some("等待官网登录超时，请重新发起登录。".to_string());
        }
        now.saturating_sub(session.updated_at) <= CODEX_OAUTH_SESSION_RETENTION_SECS
    });
}

fn generate_codex_oauth_session_id() -> String {
    let mut bytes = [0u8; 12];
    if std::fs::File::open("/dev/urandom")
        .and_then(|mut file| file.read_exact(&mut bytes))
        .is_ok()
    {
        return format!("codex_{}", general_purpose::URL_SAFE_NO_PAD.encode(bytes));
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("codex_{now:x}_{}", std::process::id())
}

pub fn build_codex_device_authorize_url(user_code: &str) -> String {
    let trimmed = user_code.trim();
    if trimmed.is_empty() {
        return CODEX_DEVICE_VERIFICATION_URL.to_string();
    }

    reqwest::Url::parse_with_params(CODEX_DEVICE_VERIFICATION_URL, [("code", trimmed)])
        .map(|url| url.to_string())
        .unwrap_or_else(|_| CODEX_DEVICE_VERIFICATION_URL.to_string())
}

pub fn parse_codex_device_poll_interval(value: &Value) -> u64 {
    value
        .as_str()
        .map(str::trim)
        .and_then(|raw| raw.parse::<u64>().ok())
        .or_else(|| value.as_u64())
        .filter(|value| *value > 0)
        .unwrap_or(CODEX_DEVICE_DEFAULT_POLL_INTERVAL_SECS)
}

fn should_continue_codex_device_poll(status: HttpStatusCode, body: &str) -> bool {
    if matches!(status, HttpStatusCode::FORBIDDEN | HttpStatusCode::NOT_FOUND) {
        return true;
    }

    if status == HttpStatusCode::BAD_REQUEST {
        let lowered = body.to_ascii_lowercase();
        return lowered.contains("pending")
            || lowered.contains("authorization")
            || lowered.contains("not found")
            || lowered.trim().is_empty();
    }

    status == HttpStatusCode::TOO_MANY_REQUESTS
}

fn summarize_codex_device_poll_error(status: HttpStatusCode, body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return format!("HTTP {}", status.as_u16());
    }
    format!("HTTP {}: {}", status.as_u16(), summarize_text(trimmed, 180))
}

fn summarize_text(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        return text.to_string();
    }
    text.chars().take(max_chars).collect::<String>()
}

pub async fn request_codex_device_user_code(
    proxy_manager: &impl CodexAuthHttpClientProvider,
) -> Result<PendingCodexDeviceLogin> {
    let client = proxy_manager.build_auth_client(15)?;
    let proxy_hint = proxy_manager
        .active_proxy_server()
        .map(|server| format!("（当前程序代理 {server}）"))
        .unwrap_or_default();
    let response = client
        .post(CODEX_DEVICE_USER_CODE_URL)
        .header(reqwest::header::ACCEPT, "application/json")
        .json(&CodexDeviceUserCodeRequest {
            client_id: CODEX_OAUTH_CLIENT_ID.to_string(),
        })
        .send()
        .await
        .with_context(|| format!("请求 Codex 设备验证码失败{proxy_hint}"))?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!(
            "请求 Codex 设备验证码失败: HTTP {}{}",
            status.as_u16(),
            summarize_remote_body(&body)
        );
    }

    let payload: CodexDeviceUserCodeResponse =
        serde_json::from_str(&body).context("解析 Codex 设备验证码响应失败")?;
    let user_code = payload.user_code.trim();
    let user_code_alt = payload.user_code_alt.trim();
    let resolved_user_code = if !user_code.is_empty() {
        user_code
    } else {
        user_code_alt
    };

    if payload.device_auth_id.trim().is_empty() || resolved_user_code.is_empty() {
        anyhow::bail!("Codex 设备验证码响应缺少 device_auth_id 或 user_code。");
    }

    Ok(PendingCodexDeviceLogin {
        device_auth_id: payload.device_auth_id.trim().to_string(),
        user_code: resolved_user_code.to_string(),
        poll_interval_seconds: parse_codex_device_poll_interval(&payload.interval),
    })
}

pub async fn complete_codex_device_login(
    proxy_manager: &impl CodexAuthHttpClientProvider,
    pending: PendingCodexDeviceLogin,
) -> std::result::Result<(AuthFile, AuthPresetDetails), String> {
    let exchange = poll_codex_device_authorization_code(proxy_manager, &pending)
        .await
        .map_err(|error| format!("等待 Codex 官网授权完成失败: {error:#}"))?;
    let auth = exchange_codex_authorization_code_for_auth(
        proxy_manager,
        &exchange.authorization_code,
        &exchange.code_verifier,
    )
    .await
    .map_err(|error| format!("换取 Codex token 失败: {error:#}"))?;

    let mut details = derive_auth_preset_details(&auth);
    if details.login_method.is_none() {
        details.login_method = Some("OAuth".to_string());
    }

    Ok((auth, details))
}

async fn poll_codex_device_authorization_code(
    proxy_manager: &impl CodexAuthHttpClientProvider,
    pending: &PendingCodexDeviceLogin,
) -> Result<CodexDeviceTokenResponse> {
    let client = proxy_manager.build_auth_client(15)?;
    let deadline = SystemTime::now()
        .checked_add(Duration::from_secs(CODEX_DEVICE_TIMEOUT_SECS))
        .unwrap_or(SystemTime::now());
    let sleep_duration = Duration::from_secs(pending.poll_interval_seconds.max(1));

    loop {
        if SystemTime::now() >= deadline {
            anyhow::bail!("超过 15 分钟仍未在官网完成授权。");
        }

        let response = client
            .post(CODEX_DEVICE_TOKEN_URL)
            .header(reqwest::header::ACCEPT, "application/json")
            .json(&CodexDeviceTokenRequest {
                device_auth_id: pending.device_auth_id.clone(),
                user_code: pending.user_code.clone(),
            })
            .send()
            .await
            .context("轮询 Codex 设备授权状态失败")?;

        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if status.is_success() {
            let payload: CodexDeviceTokenResponse =
                serde_json::from_str(&body).context("解析 Codex 授权状态响应失败")?;
            if payload.authorization_code.trim().is_empty()
                || payload.code_verifier.trim().is_empty()
            {
                anyhow::bail!("Codex 授权状态响应缺少 authorization_code 或 code_verifier。");
            }
            return Ok(payload);
        }

        if should_continue_codex_device_poll(status, &body) {
            tokio::time::sleep(sleep_duration).await;
            continue;
        }

        anyhow::bail!("{}", summarize_codex_device_poll_error(status, &body));
    }
}

async fn exchange_codex_authorization_code_for_auth(
    proxy_manager: &impl CodexAuthHttpClientProvider,
    authorization_code: &str,
    code_verifier: &str,
) -> Result<AuthFile> {
    let client = proxy_manager.build_auth_client(15)?;
    let response = client
        .post(CODEX_OAUTH_TOKEN_URL)
        .header(reqwest::header::ACCEPT, "application/json")
        .form(&[
            ("grant_type", "authorization_code"),
            ("client_id", CODEX_OAUTH_CLIENT_ID),
            ("code", authorization_code.trim()),
            ("redirect_uri", CODEX_DEVICE_TOKEN_EXCHANGE_REDIRECT_URI),
            ("code_verifier", code_verifier.trim()),
        ])
        .send()
        .await
        .context("请求 Codex token 接口失败")?;

    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        anyhow::bail!("HTTP {}{}", status.as_u16(), summarize_remote_body(&body));
    }

    let payload: CodexAuthorizationCodeTokenResponse =
        serde_json::from_str(&body).context("解析 Codex token 响应失败")?;

    let mut auth = AuthFile {
        openai_api_key: None,
        last_refresh: current_rfc3339_timestamp(),
        tokens: AuthTokens {
            access_token: payload.access_token.trim().to_string(),
            account_id: String::new(),
            id_token: payload.id_token.trim().to_string(),
            refresh_token: payload.refresh_token.trim().to_string(),
        },
    };
    auth.tokens.account_id = extract_account_id_from_auth(&auth)
        .ok_or_else(|| anyhow::anyhow!("未能从 id_token / access_token 中解析 account_id。"))?;
    validate_auth_file_sync(&auth)?;
    Ok(auth)
}

fn suggest_auth_preset_name(details: &AuthPresetDetails, auth: &AuthFile) -> String {
    let mut parts = Vec::new();
    if let Some(email) = details.email.as_deref() {
        parts.push(email.trim());
    }
    if let Some(account_name) = details.account_name.as_deref() {
        let trimmed = account_name.trim();
        if !trimmed.is_empty() && !parts.contains(&trimmed) {
            parts.push(trimmed);
        }
    }
    if !parts.is_empty() {
        return parts.join(" · ");
    }

    format!("账号 {}", short_account_id(&auth.tokens.account_id))
}
