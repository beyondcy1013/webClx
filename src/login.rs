use std::{
    fs, io,
    path::Path,
    sync::{OnceLock, RwLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    Json,
    extract::State,
    http::{HeaderMap, HeaderValue, StatusCode, header::COOKIE, header::SET_COOKIE},
    response::IntoResponse,
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use hmac::{Hmac, KeyInit, Mac};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use tokio::sync::{Mutex, Semaphore};

use tracing::warn;

use crate::{ApiResult, AppError, AppState};
use reqwest::Url;

pub const SESSION_COOKIE_NAME: &str = "webclx_session";
/// 从设置读取登录会话保持天数，默认 30 天。
fn session_ttl(settings: &crate::settings::SettingsManager) -> Duration {
    Duration::from_secs(settings.session_ttl_days() as u64 * 24 * 60 * 60)
}
const SECRET_FILE_NAME: &str = ".webclx-session-secret";
const CREDENTIALS_FILE_NAME: &str = ".webclx-login-credentials.json";
const INITIAL_PASSWORD_FILE_NAME: &str = ".webclx-initial-password";
const CREDENTIALS_VERSION: u8 = 2;
const INITIAL_PASSWORD_RANDOM_BYTES: usize = 24;
const TRUST_SECRET_FILE_NAME: &str = ".webclx-cross-instance-trust";
const TICKET_TTL_SECS: u64 = 30 * 24 * 60 * 60;
const DEFAULT_USERNAME: &str = "webclx";
const MAX_CONCURRENT_PASSWORD_VERIFICATIONS: usize = 2;
const MAX_ARGON2_MEMORY_KIB: u32 = 64 * 1024;
const MAX_ARGON2_ITERATIONS: u32 = 10;
const MAX_ARGON2_PARALLELISM: u32 = 4;
const LEGACY_SHA256_SALT_BYTES: usize = 32;

type HmacSha256 = Hmac<Sha256>;

static SESSION_SECRET: OnceLock<RwLock<Vec<u8>>> = OnceLock::new();
static CROSS_INSTANCE_TRUST_SECRET: OnceLock<Option<Vec<u8>>> = OnceLock::new();
static CREDENTIALS: OnceLock<RwLock<LoginCredentials>> = OnceLock::new();
static LOGIN_VERIFICATION_SLOTS: Semaphore =
    Semaphore::const_new(MAX_CONCURRENT_PASSWORD_VERIFICATIONS);
static PASSWORD_UPDATE_LOCK: Mutex<()> = Mutex::const_new(());

fn session_secret(state: &AppState) -> io::Result<Vec<u8>> {
    if let Some(secret) = SESSION_SECRET.get() {
        return secret
            .read()
            .map(|secret| secret.clone())
            .map_err(|_| io::Error::other("会话密钥锁已损坏"));
    }
    let secret = load_or_create_secret(&state.app_dir)?;
    let _ = SESSION_SECRET.set(RwLock::new(secret));
    SESSION_SECRET
        .get()
        .ok_or_else(|| io::Error::other("无法初始化会话密钥"))?
        .read()
        .map(|secret| secret.clone())
        .map_err(|_| io::Error::other("会话密钥锁已损坏"))
}

fn load_or_create_secret(app_dir: &Path) -> io::Result<Vec<u8>> {
    let path = app_dir.join(SECRET_FILE_NAME);
    if crate::private_file::validate_optional_existing(&path)? {
        let bytes = std::fs::read(&path)?;
        if bytes.len() < 32 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "existing session secret is too short",
            ));
        }
        crate::private_file::tighten_permissions(&path)?;
        return Ok(bytes);
    }
    let secret = generate_session_secret();
    crate::private_file::write_atomic(&path, &secret)?;
    Ok(secret)
}

fn generate_session_secret() -> Vec<u8> {
    let mut secret = vec![0u8; 64];
    rand::thread_rng().fill_bytes(&mut secret);
    secret
}

pub fn initialize_session_secret(app_dir: &Path) -> io::Result<()> {
    let secret = load_or_create_secret(app_dir)?;
    let _ = SESSION_SECRET.set(RwLock::new(secret));
    Ok(())
}

/// version=2 使用 Argon2id PHC；salt 字段仅为读取旧 SHA-256 格式保留。
#[derive(Serialize, Deserialize, Clone)]
struct StoredCredentials {
    #[serde(default)]
    version: u8,
    username: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    salt: String,
    password_hash: String,
}

#[derive(Clone)]
struct LoginCredentials {
    username: String,
    password_hash: String,
}

#[derive(Serialize, Deserialize)]
struct InitialCredentials {
    username: String,
    password: String,
}

fn credentials(state: &AppState) -> Result<LoginCredentials, AppError> {
    if let Some(credentials) = CREDENTIALS.get() {
        return credentials
            .read()
            .map(|credentials| credentials.clone())
            .map_err(|_| AppError::internal("登录凭据锁已损坏"));
    }
    let loaded = load_or_create_credentials(&state.app_dir)
        .map_err(|error| AppError::internal(format!("加载登录凭据失败: {error}")))?;
    let _ = CREDENTIALS.set(RwLock::new(loaded));
    CREDENTIALS
        .get()
        .ok_or_else(|| AppError::internal("无法初始化登录凭据"))?
        .read()
        .map(|credentials| credentials.clone())
        .map_err(|_| AppError::internal("登录凭据锁已损坏"))
}

pub fn initialize_credentials(app_dir: &Path) -> io::Result<()> {
    let loaded = load_or_create_credentials(app_dir)?;
    let _ = CREDENTIALS.set(RwLock::new(loaded));
    Ok(())
}

fn load_cross_instance_trust_secret(app_dir: &Path) -> io::Result<Option<Vec<u8>>> {
    let path = app_dir.join(TRUST_SECRET_FILE_NAME);
    if !crate::private_file::validate_optional_existing(&path)? {
        return Ok(None);
    }
    let secret = fs::read(&path)?;
    if secret.len() < 32 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "cross-instance trust secret is too short",
        ));
    }
    crate::private_file::tighten_permissions(&path)?;
    Ok(Some(secret))
}

pub fn initialize_cross_instance_trust_secret(app_dir: &Path) -> io::Result<()> {
    let _ = CROSS_INSTANCE_TRUST_SECRET.set(load_cross_instance_trust_secret(app_dir)?);
    Ok(())
}

fn cross_instance_trust_secret() -> Option<&'static [u8]> {
    CROSS_INSTANCE_TRUST_SECRET.get().and_then(Option::as_deref)
}

#[derive(Serialize, Deserialize)]
struct CrossInstanceTicketPayload {
    aud: String,
    exp: u64,
    user: String,
}

pub fn issue_cross_instance_ticket(
    target_origin: &str,
    headers: &HeaderMap,
    state: &AppState,
) -> Result<String, AppError> {
    let Some(secret) = cross_instance_trust_secret() else {
        return Err(AppError::not_found("跨实例免登录未启用"));
    };

    let parsed_origin =
        Url::parse(target_origin).map_err(|_| AppError::bad_request("目标服务器地址无效"))?;
    if !parsed_origin.path().is_empty() && parsed_origin.path() != "/" {
        return Err(AppError::bad_request("目标服务器地址无效"));
    }
    let aud = format!(
        "{}://{}{}",
        parsed_origin.scheme(),
        parsed_origin
            .host_str()
            .ok_or_else(|| AppError::bad_request("目标服务器地址无效"))?,
        parsed_origin
            .port_or_known_default()
            .map(|port| format!(":{port}"))
            .unwrap_or_default(),
    );
    let user = verify_session_from_headers(headers, state).ok_or_else(|| AppError {
        status: StatusCode::UNAUTHORIZED,
        message: "宿主会话无效".to_string(),
    })?;

    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs() + TICKET_TTL_SECS)
        .unwrap_or(0);
    let payload = CrossInstanceTicketPayload {
        aud,
        exp,
        user: user.trim().to_lowercase(),
    };
    let payload_b64 = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&payload).unwrap_or_default());
    let mut mac = HmacSha256::new_from_slice(&secret)
        .map_err(|error| AppError::internal(format!("构造跨实例签名失败: {error}")))?;
    mac.update(payload_b64.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    Ok(format!("{payload_b64}.{sig}"))
}

#[derive(Deserialize)]
pub struct CrossInstanceLoginRequest {
    pub ticket: String,
}

pub async fn cross_instance_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> ApiResult<Json<CrossInstanceTicketResponse>> {
    let target_origin = headers
        .get("X-WebClx-Target-Origin")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !target_origin.starts_with("http://") && !target_origin.starts_with("https://") {
        return Err(AppError::bad_request("目标服务器地址无效"));
    }
    let ticket = issue_cross_instance_ticket(target_origin, &headers, &state)?;
    Ok(Json(CrossInstanceTicketResponse { ticket }))
}

#[derive(Serialize)]
pub struct CrossInstanceTicketResponse {
    pub ticket: String,
}

pub async fn cross_instance_login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<CrossInstanceLoginRequest>,
) -> ApiResult<impl IntoResponse> {
    let Some(secret) = cross_instance_trust_secret() else {
        return Err(AppError::not_found("跨实例免登录未启用"));
    };

    let (payload_b64, sig_hex) = payload.ticket.split_once('.').ok_or_else(|| AppError {
        status: StatusCode::UNAUTHORIZED,
        message: "跨实例凭证无效".to_string(),
    })?;
    let mut mac = HmacSha256::new_from_slice(&secret)
        .map_err(|error| AppError::internal(format!("验证跨实例签名失败: {error}")))?;
    mac.update(payload_b64.as_bytes());
    let expected = mac.finalize().into_bytes();
    let provided = hex::decode(sig_hex).map_err(|_| AppError {
        status: StatusCode::UNAUTHORIZED,
        message: "跨实例凭证无效".to_string(),
    })?;
    if !bool::from(subtle::ConstantTimeEq::ct_eq(&provided[..], &expected[..])) {
        return Err(AppError {
            status: StatusCode::UNAUTHORIZED,
            message: "跨实例凭证无效".to_string(),
        });
    }

    let payload_bytes = URL_SAFE_NO_PAD.decode(payload_b64).map_err(|_| AppError {
        status: StatusCode::UNAUTHORIZED,
        message: "跨实例凭证无效".to_string(),
    })?;
    let ticket: CrossInstanceTicketPayload =
        serde_json::from_slice(&payload_bytes).map_err(|_| AppError {
            status: StatusCode::UNAUTHORIZED,
            message: "跨实例凭证无效".to_string(),
        })?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0);
    let host = headers
        .get(axum::http::header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let forwarded_proto = headers
        .get("X-Forwarded-Proto")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("http");
    let request_origin = format!("{forwarded_proto}://{host}");
    let listen_origin = format!("http://{}", state.listen_addr);
    if ticket.exp <= now
        || ticket.user.is_empty()
        || (ticket.aud != request_origin && ticket.aud != listen_origin)
    {
        return Err(AppError {
            status: StatusCode::UNAUTHORIZED,
            message: "跨实例凭证已过期或不适用于本服务器".to_string(),
        });
    }

    let cookie_value = issue_session_cookie(&ticket.user, &state)?;
    let ttl = session_ttl(&state.workspace_settings);
    let mut headers = axum::http::HeaderMap::new();
    append_session_cookie(&mut headers, &cookie_value, ttl.as_secs())?;
    Ok((
        headers,
        Json(LoginResponse {
            ok: true,
            user: ticket.user,
        }),
    ))
}

fn load_or_create_credentials(app_dir: &Path) -> io::Result<LoginCredentials> {
    let path = app_dir.join(CREDENTIALS_FILE_NAME);
    if let Some(stored) = read_stored_credentials(&path)? {
        if stored.version == CREDENTIALS_VERSION && is_valid_argon2id_hash(&stored.password_hash) {
            crate::private_file::tighten_permissions(&path)?;
            return Ok(LoginCredentials {
                username: stored.username,
                password_hash: stored.password_hash,
            });
        }
        if is_valid_legacy_sha256_credentials(&stored) {
            return rotate_credentials(app_dir, &path, normalized_username(&stored.username));
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "existing login credentials have an unsupported or invalid format",
        ));
    }

    rotate_credentials(app_dir, &path, DEFAULT_USERNAME.to_string())
}

fn is_valid_argon2id_hash(encoded: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(encoded) else {
        return false;
    };
    if parsed.algorithm != Algorithm::Argon2id.ident()
        || parsed.version != Some(Version::V0x13.into())
        || parsed.salt.is_none()
        || parsed.hash.is_none()
    {
        return false;
    }
    let (Some(memory), Some(iterations), Some(parallelism)) = (
        parsed.params.get_decimal("m"),
        parsed.params.get_decimal("t"),
        parsed.params.get_decimal("p"),
    ) else {
        return false;
    };
    Params::try_from(&parsed).is_ok()
        && memory <= MAX_ARGON2_MEMORY_KIB
        && iterations <= MAX_ARGON2_ITERATIONS
        && parallelism <= MAX_ARGON2_PARALLELISM
}

fn is_valid_legacy_sha256_credentials(stored: &StoredCredentials) -> bool {
    if stored.version != 0 {
        return false;
    }
    let Ok(salt) = URL_SAFE_NO_PAD.decode(&stored.salt) else {
        return false;
    };
    let Ok(password_hash) = hex::decode(&stored.password_hash) else {
        return false;
    };
    salt.len() == LEGACY_SHA256_SALT_BYTES && password_hash.len() == 32
}

fn read_stored_credentials(path: &Path) -> io::Result<Option<StoredCredentials>> {
    if !crate::private_file::validate_optional_existing(path)? {
        return Ok(None);
    }
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn normalized_username(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        DEFAULT_USERNAME.to_string()
    } else {
        value.to_string()
    }
}

fn rotate_credentials(
    app_dir: &Path,
    credentials_path: &Path,
    username: String,
) -> io::Result<LoginCredentials> {
    let password = generate_initial_password();
    let password_hash = hash_password_argon2(&password)?;
    let stored = StoredCredentials {
        version: CREDENTIALS_VERSION,
        username: username.clone(),
        salt: String::new(),
        password_hash: password_hash.clone(),
    };
    let initial = InitialCredentials {
        username: username.clone(),
        password,
    };
    let initial_path = app_dir.join(INITIAL_PASSWORD_FILE_NAME);
    write_private_json_atomic(&initial_path, &initial)?;
    if let Err(error) = write_private_json_atomic(credentials_path, &stored) {
        let _ = fs::remove_file(initial_path);
        return Err(error);
    }
    Ok(LoginCredentials {
        username,
        password_hash,
    })
}

fn generate_initial_password() -> String {
    let mut bytes = [0_u8; INITIAL_PASSWORD_RANDOM_BYTES];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_password_argon2(password: &str) -> io::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|error| io::Error::other(format!("Argon2id 哈希失败: {error}")))
}

fn write_private_json_atomic(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let json = serde_json::to_vec_pretty(value)
        .map_err(|error| io::Error::other(format!("序列化凭据失败: {error}")))?;
    crate::private_file::write_atomic(path, &json)
}

fn store_credentials(app_dir: &Path, credentials: &LoginCredentials) -> io::Result<()> {
    write_private_json_atomic(
        &app_dir.join(CREDENTIALS_FILE_NAME),
        &StoredCredentials {
            version: CREDENTIALS_VERSION,
            username: credentials.username.clone(),
            salt: String::new(),
            password_hash: credentials.password_hash.clone(),
        },
    )
}

fn password_hash_for_user(user: &str, state: &AppState) -> Result<Option<String>, AppError> {
    let creds = credentials(state)?;
    if !user.eq_ignore_ascii_case(&creds.username) {
        return Ok(None);
    }
    Ok(Some(creds.password_hash.clone()))
}

#[derive(Serialize, Deserialize)]
struct SessionPayload {
    user: String,
    exp: u64,
}

pub(crate) fn issue_session_cookie(user: &str, state: &AppState) -> Result<String, AppError> {
    let ttl = session_ttl(&state.workspace_settings);
    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() + ttl.as_secs())
        .unwrap_or(0);
    let payload = SessionPayload {
        user: user.to_string(),
        exp,
    };
    let payload_bytes = serde_json::to_vec(&payload).unwrap_or_default();
    let payload_b64 = URL_SAFE_NO_PAD.encode(&payload_bytes);
    let secret = session_secret(state)
        .map_err(|error| AppError::internal(format!("加载会话密钥失败: {error}")))?;
    let mut mac = HmacSha256::new_from_slice(&secret)
        .map_err(|error| AppError::internal(format!("构造会话签名失败: {error}")))?;
    mac.update(payload_b64.as_bytes());
    let sig = hex::encode(mac.finalize().into_bytes());
    Ok(format!("{payload_b64}.{sig}"))
}

/// 解析并验证 cookie 值。返回用户名（有效）或 None（无效/过期）。
pub fn verify_session_cookie(value: Option<&str>, state: &AppState) -> Option<String> {
    let value = value?;
    let (payload_b64, sig_hex) = value.split_once('.')?;
    let secret = session_secret(state).ok()?;
    let mut mac = HmacSha256::new_from_slice(&secret).ok()?;
    mac.update(payload_b64.as_bytes());
    let expected = mac.finalize().into_bytes();
    let provided = hex::decode(sig_hex).ok()?;
    if provided.len() != expected.len() {
        return None;
    }
    if !bool::from(subtle::ConstantTimeEq::ct_eq(&provided[..], &expected[..])) {
        return None;
    }
    let payload_bytes = URL_SAFE_NO_PAD.decode(payload_b64).ok()?;
    let payload: SessionPayload = serde_json::from_slice(&payload_bytes).ok()?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    if payload.exp <= now {
        return None;
    }
    Some(payload.user)
}

pub fn verify_session_from_headers(headers: &HeaderMap, state: &AppState) -> Option<String> {
    session_cookie_values(headers)
        .into_iter()
        .find_map(|value| verify_session_cookie(Some(&value), state))
}

// ===== HTTP handlers =====

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub ok: bool,
    pub user: String,
}

#[derive(Deserialize)]
pub struct UpdatePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

#[derive(Serialize)]
pub struct UpdatePasswordResponse {
    pub ok: bool,
    pub message: String,
}

pub async fn update_password(
    State(state): State<AppState>,
    Json(payload): Json<UpdatePasswordRequest>,
) -> ApiResult<impl IntoResponse> {
    if payload.current_password.is_empty() {
        return Err(AppError::bad_request("请输入当前密码"));
    }
    if payload.new_password.is_empty() {
        return Err(AppError::bad_request("新密码不能为空"));
    }
    if payload.current_password.len() > 4096 || payload.new_password.len() > 4096 {
        return Err(AppError::bad_request("密码长度不能超过 4096 字节"));
    }

    let _update_guard = PASSWORD_UPDATE_LOCK.lock().await;
    let current = credentials(&state)?;
    let _verification_slot = LOGIN_VERIFICATION_SLOTS
        .try_acquire()
        .map_err(|_| AppError {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: "密码校验请求过多，请稍后重试".to_string(),
        })?;
    let current_password = payload.current_password;
    let current_hash = current.password_hash.clone();
    let verified = tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&current_hash)
            .map_err(|error| format!("登录凭据格式无效: {error}"))?;
        Ok::<bool, String>(
            Argon2::default()
                .verify_password(current_password.as_bytes(), &parsed)
                .is_ok(),
        )
    })
    .await
    .map_err(|error| AppError::internal(format!("密码校验任务失败: {error}")))?
    .map_err(AppError::internal)?;
    if !verified {
        return Err(AppError {
            status: StatusCode::UNAUTHORIZED,
            message: "当前密码错误".to_string(),
        });
    }

    let new_password = payload.new_password;
    let password_hash = tokio::task::spawn_blocking(move || hash_password_argon2(&new_password))
        .await
        .map_err(|error| AppError::internal(format!("密码哈希任务失败: {error}")))?
        .map_err(|error| AppError::internal(format!("生成新密码哈希失败: {error}")))?;
    let updated = LoginCredentials {
        username: current.username,
        password_hash,
    };
    store_credentials(&state.app_dir, &updated)
        .map_err(|error| AppError::internal(format!("保存新登录凭据失败: {error}")))?;
    *CREDENTIALS
        .get()
        .ok_or_else(|| AppError::internal("登录凭据尚未初始化"))?
        .write()
        .map_err(|_| AppError::internal("登录凭据锁已损坏"))? = updated;
    remove_initial_password_file(&state.app_dir)
        .map_err(|error| AppError::internal(format!("无法删除初始登录凭据: {error}")))?;

    let secret = generate_session_secret();
    crate::private_file::write_atomic(&state.app_dir.join(SECRET_FILE_NAME), &secret)
        .map_err(|error| AppError::internal(format!("轮换会话密钥失败: {error}")))?;
    *SESSION_SECRET
        .get()
        .ok_or_else(|| AppError::internal("会话密钥尚未初始化"))?
        .write()
        .map_err(|_| AppError::internal("会话密钥锁已损坏"))? = secret;

    let mut headers = HeaderMap::new();
    append_expired_session_cookie(&mut headers, SESSION_COOKIE_NAME)?;
    Ok((
        headers,
        Json(UpdatePasswordResponse {
            ok: true,
            message: "WebUI 登录密码已更新，请重新登录。".to_string(),
        }),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> ApiResult<impl IntoResponse> {
    let user = payload.username.trim().to_string();
    if user.is_empty() {
        return Err(AppError::bad_request("用户名不能为空"));
    }
    let _verification_slot = LOGIN_VERIFICATION_SLOTS
        .try_acquire()
        .map_err(|_| AppError {
            status: StatusCode::TOO_MANY_REQUESTS,
            message: "登录请求过多，请稍后重试".to_string(),
        })?;
    let Some(password_hash) = password_hash_for_user(&user, &state)? else {
        return Err(AppError {
            status: StatusCode::UNAUTHORIZED,
            message: "用户名或密码错误".to_string(),
        });
    };
    let password = payload.password;
    let verified = tokio::task::spawn_blocking(move || {
        let parsed = PasswordHash::new(&password_hash)
            .map_err(|error| format!("登录凭据格式无效: {error}"))?;
        Ok::<bool, String>(
            Argon2::default()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok(),
        )
    })
    .await
    .map_err(|error| AppError::internal(format!("密码校验任务失败: {error}")))?
    .map_err(AppError::internal)?;
    if !verified {
        return Err(AppError {
            status: StatusCode::UNAUTHORIZED,
            message: "用户名或密码错误".to_string(),
        });
    }
    remove_initial_password_file(&state.app_dir)
        .map_err(|error| AppError::internal(format!("无法删除初始登录凭据: {error}")))?;
    let cookie_value = issue_session_cookie(&user, &state)?;
    let ttl = session_ttl(&state.workspace_settings);
    let mut headers = axum::http::HeaderMap::new();
    append_session_cookie(&mut headers, &cookie_value, ttl.as_secs())?;
    Ok((
        headers,
        Json(LoginResponse {
            ok: true,
            user: user.clone(),
        }),
    ))
}

fn remove_initial_password_file(app_dir: &Path) -> io::Result<()> {
    match fs::remove_file(app_dir.join(INITIAL_PASSWORD_FILE_NAME)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

#[derive(Serialize)]
pub struct SessionResponse {
    pub authenticated: bool,
    pub user: Option<String>,
}

pub async fn logout() -> impl IntoResponse {
    let mut headers = axum::http::HeaderMap::new();
    if let Err(err) = append_expired_session_cookie(&mut headers, SESSION_COOKIE_NAME) {
        warn!(error = %err, "无法构造登出失效 cookie header");
    }
    (
        StatusCode::OK,
        headers,
        Json(SessionResponse {
            authenticated: false,
            user: None,
        }),
    )
}

/// GET /api/auth/session：返回当前会话状态（前端用于判断是否需要跳登录）。
pub async fn session_status(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Json<SessionResponse> {
    let user = verify_session_from_headers(&headers, &state);
    Json(SessionResponse {
        authenticated: user.is_some(),
        user,
    })
}

/// Nginx auth_request endpoint for sibling services on the same browser host.
/// Unlike the normal auth middleware, loopback callers receive no exemption.
pub async fn proxy_auth_check(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> StatusCode {
    if verify_session_from_headers(&headers, &state).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::UNAUTHORIZED
    }
}

fn session_cookie_values(headers: &HeaderMap) -> Vec<String> {
    let Some(raw) = headers.get(COOKIE).and_then(|value| value.to_str().ok()) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    for pair in raw.split(';') {
        let pair = pair.trim();
        if let Some((k, v)) = pair.split_once('=')
            && k.trim() == SESSION_COOKIE_NAME
        {
            values.push(v.trim().to_string());
        }
    }
    values
}

fn append_session_cookie(
    headers: &mut HeaderMap,
    value: &str,
    max_age_secs: u64,
) -> Result<(), AppError> {
    let cookie = format!(
        "{SESSION_COOKIE_NAME}={value}; Path=/; HttpOnly; SameSite=Strict; Max-Age={max_age_secs};"
    );
    let parsed = cookie
        .parse()
        .map_err(|err| AppError::internal(format!("构造会话 cookie 失败: {err}")))?;
    headers.append(SET_COOKIE, parsed);
    Ok(())
}

fn append_expired_session_cookie(headers: &mut HeaderMap, name: &str) -> Result<(), AppError> {
    for path in ["/", "/terminal", "/login", "/api"] {
        let cookie = format!("{name}=; Path={path}; HttpOnly; SameSite=Strict; Max-Age=0");
        let parsed = HeaderValue::from_str(&cookie)
            .map_err(|err| AppError::internal(format!("构造登出 cookie 失败: {err}")))?;
        headers.append(SET_COOKIE, parsed);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppState, auth, codex_proxy, frpc, proxy, quota, settings, terminal};
    use sha2::Digest;
    use std::{
        net::SocketAddr,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[tokio::test]
    async fn verifies_later_valid_cookie_when_same_name_cookie_precedes_it() {
        let state = test_state();
        let valid = issue_session_cookie("beyondcy", &state).unwrap();
        let mut headers = HeaderMap::new();
        headers.insert(
            COOKIE,
            format!("{SESSION_COOKIE_NAME}=not-a-valid-cookie; {SESSION_COOKIE_NAME}={valid}")
                .parse()
                .unwrap(),
        );

        assert_eq!(verify_session_from_headers(&headers, &state).as_deref(), Some("beyondcy"));
    }

    #[tokio::test]
    async fn proxy_auth_check_requires_a_valid_signed_cookie() {
        let state = test_state();

        assert_eq!(
            proxy_auth_check(State(state.clone()), HeaderMap::new()).await,
            StatusCode::UNAUTHORIZED
        );

        let mut local_token_headers = HeaderMap::new();
        local_token_headers.insert(
            axum::http::header::AUTHORIZATION,
            "Bearer test-local-api-token".parse().unwrap(),
        );
        assert_eq!(
            proxy_auth_check(State(state.clone()), local_token_headers).await,
            StatusCode::UNAUTHORIZED
        );

        let valid = issue_session_cookie("beyondcy", &state).unwrap();
        let mut cookie_headers = HeaderMap::new();
        cookie_headers.insert(COOKIE, format!("{SESSION_COOKIE_NAME}={valid}").parse().unwrap());
        assert_eq!(proxy_auth_check(State(state), cookie_headers).await, StatusCode::NO_CONTENT);
    }

    #[tokio::test]
    async fn login_rejects_when_argon2_verification_capacity_is_exhausted() {
        let state = test_state();
        let held_slots: Vec<_> = (0..MAX_CONCURRENT_PASSWORD_VERIFICATIONS)
            .map(|_| LOGIN_VERIFICATION_SLOTS.try_acquire().unwrap())
            .collect();

        let result = login(
            State(state),
            Json(LoginRequest {
                username: DEFAULT_USERNAME.to_string(),
                password: "not-the-password".to_string(),
            }),
        )
        .await;

        match result {
            Ok(_) => panic!("login must reject excess password verification work"),
            Err(error) => assert_eq!(error.status, StatusCode::TOO_MANY_REQUESTS),
        }
        drop(held_slots);
    }

    #[test]
    fn new_install_uses_argon2id_and_writes_a_recoverable_random_password() {
        let app_dir = unique_temp_dir("webclx-login-new-credentials-test");
        let second_app_dir = unique_temp_dir("webclx-login-second-credentials-test");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::create_dir_all(&second_app_dir).unwrap();

        let credentials = load_or_create_credentials(&app_dir).unwrap();
        let second_credentials = load_or_create_credentials(&second_app_dir).unwrap();
        let initial = read_initial_credentials(&app_dir);
        let second_initial = read_initial_credentials(&second_app_dir);

        assert!(credentials.password_hash.starts_with("$argon2id$"));
        assert_eq!(initial.username, DEFAULT_USERNAME);
        assert!(verify_password(&initial.password, &credentials.password_hash));
        assert!(verify_password(&second_initial.password, &second_credentials.password_hash));
        assert_ne!(initial.password, second_initial.password);
        assert_eq!(private_file_mode(&app_dir.join(CREDENTIALS_FILE_NAME)), 0o600);
        assert_eq!(private_file_mode(&app_dir.join(INITIAL_PASSWORD_FILE_NAME)), 0o600);
        std::fs::remove_dir_all(app_dir).unwrap();
        std::fs::remove_dir_all(second_app_dir).unwrap();
    }

    #[test]
    fn legacy_sha256_credentials_are_rotated_instead_of_remaining_publicly_guessable() {
        let app_dir = unique_temp_dir("webclx-login-legacy-credentials-test");
        std::fs::create_dir_all(&app_dir).unwrap();
        let salt = [7_u8; LEGACY_SHA256_SALT_BYTES];
        let legacy = StoredCredentials {
            version: 0,
            username: "legacy-user".to_string(),
            salt: URL_SAFE_NO_PAD.encode(&salt),
            password_hash: legacy_sha256_hash(&salt, "legacy-password"),
        };
        std::fs::write(
            app_dir.join(CREDENTIALS_FILE_NAME),
            serde_json::to_vec_pretty(&legacy).unwrap(),
        )
        .unwrap();

        let credentials = load_or_create_credentials(&app_dir).unwrap();
        let initial = read_initial_credentials(&app_dir);
        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(app_dir.join(CREDENTIALS_FILE_NAME)).unwrap())
                .unwrap();

        assert_eq!(stored["username"], "legacy-user");
        assert!(
            stored["password_hash"]
                .as_str()
                .unwrap()
                .starts_with("$argon2id$")
        );
        assert_eq!(stored["version"], CREDENTIALS_VERSION);
        assert_eq!(initial.username, "legacy-user");
        assert!(verify_password(&initial.password, &credentials.password_hash));
        std::fs::remove_dir_all(app_dir).unwrap();
    }

    #[test]
    fn successful_login_removes_the_recoverable_initial_password() {
        let app_dir = unique_temp_dir("webclx-login-initial-password-cleanup-test");
        std::fs::create_dir_all(&app_dir).unwrap();
        let initial_path = app_dir.join(INITIAL_PASSWORD_FILE_NAME);
        std::fs::write(&initial_path, b"temporary recovery credential").unwrap();

        remove_initial_password_file(&app_dir).unwrap();

        assert!(!initial_path.exists());
        remove_initial_password_file(&app_dir).unwrap();
        std::fs::remove_dir_all(app_dir).unwrap();
    }

    #[test]
    fn updated_credentials_are_persisted_as_argon2id_and_replace_the_old_password() {
        let app_dir = unique_temp_dir("webclx-login-update-credentials-test");
        std::fs::create_dir_all(&app_dir).unwrap();
        let old_hash = hash_password_argon2("old-password").unwrap();
        let updated = LoginCredentials {
            username: "existing-user".to_string(),
            password_hash: hash_password_argon2("new-password").unwrap(),
        };

        store_credentials(&app_dir, &updated).unwrap();

        let stored = read_stored_credentials(&app_dir.join(CREDENTIALS_FILE_NAME))
            .unwrap()
            .unwrap();
        assert_eq!(stored.version, CREDENTIALS_VERSION);
        assert_eq!(stored.username, "existing-user");
        assert!(is_valid_argon2id_hash(&stored.password_hash));
        assert!(verify_password("new-password", &stored.password_hash));
        assert!(!verify_password("old-password", &stored.password_hash));
        assert_ne!(stored.password_hash, old_hash);
        assert_eq!(private_file_mode(&app_dir.join(CREDENTIALS_FILE_NAME)), 0o600);
        std::fs::remove_dir_all(app_dir).unwrap();
    }

    #[test]
    fn malformed_existing_credentials_fail_closed_instead_of_rotating() {
        let app_dir = unique_temp_dir("webclx-login-malformed-credentials-test");
        std::fs::create_dir_all(&app_dir).unwrap();
        let credentials_path = app_dir.join(CREDENTIALS_FILE_NAME);
        std::fs::write(&credentials_path, b"not valid credential JSON").unwrap();

        let error = match load_or_create_credentials(&app_dir) {
            Ok(_) => panic!("malformed credentials must not be silently replaced"),
            Err(error) => error,
        };

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(std::fs::read(&credentials_path).unwrap(), b"not valid credential JSON");
        assert!(!app_dir.join(INITIAL_PASSWORD_FILE_NAME).exists());
        std::fs::remove_dir_all(app_dir).unwrap();
    }

    #[test]
    fn malformed_argon2_credentials_fail_closed_instead_of_rotating() {
        let app_dir = unique_temp_dir("webclx-login-malformed-argon2-test");
        std::fs::create_dir_all(&app_dir).unwrap();
        let credentials_path = app_dir.join(CREDENTIALS_FILE_NAME);
        let malformed = StoredCredentials {
            version: CREDENTIALS_VERSION,
            username: "existing-user".to_string(),
            salt: String::new(),
            password_hash: "$argon2id$not-a-valid-phc".to_string(),
        };
        let original = serde_json::to_vec_pretty(&malformed).unwrap();
        std::fs::write(&credentials_path, &original).unwrap();

        let error = match load_or_create_credentials(&app_dir) {
            Ok(_) => panic!("malformed Argon2 credentials must not be silently replaced"),
            Err(error) => error,
        };

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(std::fs::read(&credentials_path).unwrap(), original);
        assert!(!app_dir.join(INITIAL_PASSWORD_FILE_NAME).exists());
        std::fs::remove_dir_all(app_dir).unwrap();
    }

    #[test]
    fn incomplete_or_excessive_argon2_credentials_fail_closed() {
        for (label, password_hash) in [
            (
                "missing-params",
                "$argon2id$v=19$c29tZXNhbHQxMjM0NTY3OA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            ),
            (
                "excessive-memory",
                "$argon2id$v=19$m=1048576,t=2,p=1$c29tZXNhbHQxMjM0NTY3OA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            ),
            (
                "excessive-iterations",
                "$argon2id$v=19$m=19456,t=11,p=1$c29tZXNhbHQxMjM0NTY3OA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            ),
            (
                "excessive-parallelism",
                "$argon2id$v=19$m=19456,t=2,p=5$c29tZXNhbHQxMjM0NTY3OA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
            ),
        ] {
            let app_dir = unique_temp_dir(&format!("webclx-login-{label}"));
            std::fs::create_dir_all(&app_dir).unwrap();
            let credentials_path = app_dir.join(CREDENTIALS_FILE_NAME);
            let malformed = StoredCredentials {
                version: CREDENTIALS_VERSION,
                username: "existing-user".to_string(),
                salt: String::new(),
                password_hash: password_hash.to_string(),
            };
            let original = serde_json::to_vec_pretty(&malformed).unwrap();
            std::fs::write(&credentials_path, &original).unwrap();

            let error = match load_or_create_credentials(&app_dir) {
                Ok(_) => panic!("{label} Argon2 credentials must fail closed"),
                Err(error) => error,
            };

            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert_eq!(std::fs::read(&credentials_path).unwrap(), original);
            assert!(!app_dir.join(INITIAL_PASSWORD_FILE_NAME).exists());
            std::fs::remove_dir_all(app_dir).unwrap();
        }
    }

    #[test]
    fn legacy_credentials_require_the_historical_32_byte_salt() {
        for (label, salt) in [("empty", Vec::new()), ("short", vec![7_u8; 16])] {
            let app_dir = unique_temp_dir(&format!("webclx-login-legacy-{label}-salt"));
            std::fs::create_dir_all(&app_dir).unwrap();
            let credentials_path = app_dir.join(CREDENTIALS_FILE_NAME);
            let legacy = StoredCredentials {
                version: 0,
                username: "legacy-user".to_string(),
                salt: URL_SAFE_NO_PAD.encode(&salt),
                password_hash: legacy_sha256_hash(&salt, "legacy-password"),
            };
            let original = serde_json::to_vec_pretty(&legacy).unwrap();
            std::fs::write(&credentials_path, &original).unwrap();

            let error = match load_or_create_credentials(&app_dir) {
                Ok(_) => panic!("legacy {label} salt must fail closed"),
                Err(error) => error,
            };

            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert_eq!(std::fs::read(&credentials_path).unwrap(), original);
            assert!(!app_dir.join(INITIAL_PASSWORD_FILE_NAME).exists());
            std::fs::remove_dir_all(app_dir).unwrap();
        }
    }

    #[cfg(unix)]
    #[test]
    fn dangling_credentials_symlink_fails_closed_without_recovery_data() {
        use std::os::unix::fs::symlink;

        let app_dir = unique_temp_dir("webclx-login-dangling-credentials-link");
        std::fs::create_dir_all(&app_dir).unwrap();
        let credentials_path = app_dir.join(CREDENTIALS_FILE_NAME);
        symlink(app_dir.join("missing-target"), &credentials_path).unwrap();

        let error = match load_or_create_credentials(&app_dir) {
            Ok(_) => panic!("dangling credentials symlink must fail closed"),
            Err(error) => error,
        };

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(
            std::fs::symlink_metadata(&credentials_path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!app_dir.join(INITIAL_PASSWORD_FILE_NAME).exists());
        std::fs::remove_dir_all(app_dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn session_secret_symlink_fails_closed() {
        use std::os::unix::fs::symlink;

        let app_dir = unique_temp_dir("webclx-login-session-secret-link");
        std::fs::create_dir_all(&app_dir).unwrap();
        let secret_path = app_dir.join(SECRET_FILE_NAME);
        symlink(app_dir.join("missing-target"), &secret_path).unwrap();

        let error =
            load_or_create_secret(&app_dir).expect_err("session secret symlink must fail closed");

        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert!(
            std::fs::symlink_metadata(&secret_path)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        std::fs::remove_dir_all(app_dir).unwrap();
    }

    fn read_initial_credentials(app_dir: &Path) -> InitialCredentials {
        serde_json::from_slice(&std::fs::read(app_dir.join(INITIAL_PASSWORD_FILE_NAME)).unwrap())
            .unwrap()
    }

    fn verify_password(password: &str, password_hash: &str) -> bool {
        let parsed = PasswordHash::new(password_hash).unwrap();
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    }

    fn legacy_sha256_hash(salt: &[u8], password: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(salt);
        hasher.update(password.as_bytes());
        hex::encode(hasher.finalize())
    }

    #[cfg(unix)]
    fn private_file_mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[cfg(not(unix))]
    fn private_file_mode(_path: &Path) -> u32 {
        0o600
    }

    fn test_state() -> AppState {
        let app_dir = unique_temp_dir("webclx-login-cookie-test");
        std::fs::create_dir_all(&app_dir).unwrap();
        AppState {
            static_dir: app_dir.join("static"),
            listen_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            version: "test".to_string(),
            app_dir: app_dir.clone(),
            local_api_token: std::sync::Arc::from("test-local-api-token"),
            workspace_settings: settings::SettingsManager::load(&app_dir).unwrap(),
            auth_manager: auth::AuthPresetManager::load(&app_dir).unwrap(),
            codebuddy_manager: crate::codebuddy::CodeBuddyManager::load(
                app_dir.clone(),
                std::env::temp_dir(),
            )
            .unwrap(),
            agy_manager: crate::agy::Manager::load(app_dir.clone(), std::env::temp_dir()),
            devin_manager: crate::devin::DevinManager::load(app_dir.clone(), std::env::temp_dir()),
            codex_oauth_manager: auth::CodexOAuthManager::new(),
            codex_proxy_history: codex_proxy::CodexProxyHistory::new(),
            proxy_manager: proxy::ProxyManager::load(&app_dir).unwrap(),
            quota_reset_cache: crate::quota_reset_cache::QuotaResetCache::new(),
            quota_manager: quota::QuotaConfigManager::load(&app_dir),
            frpc_manager: frpc::FrpcManager::load(&app_dir, 0).unwrap(),
            frps_manager: frpc::FrpsManager::load(&app_dir).unwrap(),
            frp_role_manager: frpc::FrpRoleManager::load(&app_dir, 0).unwrap(),
            terminal_manager: terminal::TerminalManager::new(
                app_dir.join(".webclx-terminal-sessions.json"),
            ),
            preset_test_scheduler: auth::PresetTestScheduler::new(
                &app_dir.join(".webclx-terminal-sessions.json"),
            ),
            agent_manager: crate::agent::AgentManager::new(&app_dir),
            agent_config: crate::agent::AgentConfigManager::new(&app_dir),
            collaboration_manager: crate::collaboration::CollaborationManager::new(&app_dir),
        }
    }

    fn unique_temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{label}-{nanos}"))
    }
}
