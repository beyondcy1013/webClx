use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{
        Arc, OnceLock, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};

// ===== Antigravity OAuth（对齐 CLIProxyAPI internal/auth/antigravity 的实现）=====
const AGY_CLIENT_ID: &str =
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
const AGY_CLIENT_SECRET: &str = "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";
const AGY_AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const AGY_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const AGY_USERINFO_ENDPOINT: &str = "https://www.googleapis.com/oauth2/v2/userinfo?alt=json";
/// 与 CLIProxyAPI 一致的本机回跳地址；浏览器端没有监听时页面打不开，
/// 但地址栏仍带 code，由用户粘贴回来兑换。
const AGY_REDIRECT_URI: &str = "http://localhost:51121/oauth-callback";
const AGY_SCOPES: &str = "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs";
/// 与 CLIProxyAPI misc.AntigravityRequestUserAgent 一致的短 UA（使用其 fallback 版本号）。
const AGY_USER_AGENT: &str = "antigravity/hub/2.9.1 darwin/arm64";
const AGY_OAUTH_SESSION_TTL_SECS: i64 = 600;

#[derive(Clone)]
struct AgyOAuthSession {
    state: String,
    target_id: Option<String>,
    expires_at: i64,
}

fn agy_oauth_sessions() -> &'static RwLock<HashMap<String, AgyOAuthSession>> {
    static SESSIONS: OnceLock<RwLock<HashMap<String, AgyOAuthSession>>> = OnceLock::new();
    SESSIONS.get_or_init(|| RwLock::new(HashMap::new()))
}

fn random_hex_string() -> String {
    use rand::RngCore;
    let mut bytes = [0_u8; 16];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn agy_oauth_auth_url(state: &str) -> Result<String, String> {
    let url = reqwest::Url::parse_with_params(
        AGY_AUTH_ENDPOINT,
        &[
            ("access_type", "offline"),
            ("client_id", AGY_CLIENT_ID),
            ("prompt", "consent"),
            ("redirect_uri", AGY_REDIRECT_URI),
            ("response_type", "code"),
            ("scope", AGY_SCOPES),
            ("state", state),
        ],
    )
    .map_err(|error| format!("构造 Antigravity 授权链接失败: {error}"))?;
    Ok(url.to_string())
}

fn agy_oauth_insert_session(target_id: Option<String>) -> Result<(String, String), String> {
    let now = now();
    let mut sessions = agy_oauth_sessions().write().unwrap();
    sessions.retain(|_, session| session.expires_at > now);
    let state = random_hex_string();
    let session_id = loop {
        let candidate = random_hex_string();
        if !sessions.contains_key(&candidate) {
            break candidate;
        }
    };
    sessions.insert(
        session_id.clone(),
        AgyOAuthSession {
            state: state.clone(),
            target_id,
            expires_at: now + AGY_OAUTH_SESSION_TTL_SECS,
        },
    );
    Ok((session_id, state))
}

fn agy_oauth_take_session(session_id: &str) -> Result<AgyOAuthSession, String> {
    let mut sessions = agy_oauth_sessions().write().unwrap();
    match sessions.remove(session_id) {
        Some(session) if session.expires_at > now() => Ok(session),
        Some(_) => Err("AGY 登录会话已过期，请重新发起登录。".into()),
        None => Err("找不到 AGY 登录会话，请重新发起登录。".into()),
    }
}

/// 解析 Google 登录回调文本：完整 URL 时取 query，否则视为裸 code。
fn agy_oauth_parse_callback(raw: &str) -> (String, String) {
    let text = raw.trim();
    if text.is_empty() {
        return (String::new(), String::new());
    }
    if let Ok(url) = reqwest::Url::parse(text) {
        let mut code = String::new();
        let mut state = String::new();
        for (key, value) in url.query_pairs() {
            match key.to_string().as_str() {
                "code" => code = value.to_string(),
                "state" => state = value.to_string(),
                _ => {}
            }
        }
        if !code.is_empty() || !state.is_empty() {
            return (code, state);
        }
    }
    (text.to_string(), String::new())
}

fn agy_oauth_client() -> Result<Client, String> {
    Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|error| format!("构造 HTTP 客户端失败: {error}"))
}

#[derive(Default)]
struct AgyOAuthTokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    token_type: Option<String>,
    id_token: Option<String>,
}

async fn agy_oauth_exchange_code(client: &Client, code: &str) -> Result<AgyOAuthTokens, String> {
    let response = client
        .post(AGY_TOKEN_ENDPOINT)
        .form(&[
            ("code", code),
            ("client_id", AGY_CLIENT_ID),
            ("client_secret", AGY_CLIENT_SECRET),
            ("redirect_uri", AGY_REDIRECT_URI),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|error| format!("请求 Google token 端点失败: {error}"))?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|error| format!("解析 Google token 响应失败: {error}"))?;
    if !status.is_success() {
        let detail = body
            .get("error_description")
            .or_else(|| body.get("error"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| body.to_string());
        return Err(format!("Google token 兑换失败 (HTTP {status}): {detail}"));
    }
    let access_token = body
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| "Google token 响应缺少 access_token".to_string())?
        .to_string();
    Ok(AgyOAuthTokens {
        access_token,
        refresh_token: body
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string),
        expires_in: body.get("expires_in").and_then(Value::as_i64),
        token_type: body
            .get("token_type")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string),
        id_token: body
            .get("id_token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|token| !token.is_empty())
            .map(str::to_string),
    })
}

async fn agy_oauth_fetch_email(client: &Client, tokens: &AgyOAuthTokens) -> String {
    if let Ok(response) = client
        .get(AGY_USERINFO_ENDPOINT)
        .bearer_auth(&tokens.access_token)
        .header("user-agent", AGY_USER_AGENT)
        .send()
        .await
    {
        if response.status().is_success() {
            if let Ok(body) = response.json::<Value>().await {
                if let Some(email) = body
                    .get("email")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|email| !email.is_empty())
                {
                    return email.to_string();
                }
            }
        }
    }
    // 回退：从 id_token 的 JWT payload 里解 email（与 detect_current 相同做法）。
    if let Some(jwt) = tokens.id_token.as_deref() {
        let parts: Vec<&str> = jwt.split('.').collect();
        if parts.len() >= 2 {
            use base64::Engine;
            if let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(parts[1])
                .or_else(|_| base64::engine::general_purpose::STANDARD.decode(parts[1]))
            {
                if let Ok(payload) = serde_json::from_slice::<Value>(&bytes) {
                    if let Some(email) = payload
                        .get("email")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|email| !email.is_empty())
                    {
                        return email.to_string();
                    }
                }
            }
        }
    }
    String::new()
}

fn agy_oauth_token_expiry(expires_in: Option<i64>) -> Value {
    let Some(expires_in) = expires_in.filter(|seconds| *seconds > 0) else {
        return Value::Null;
    };
    let now = time::OffsetDateTime::now_local()
        .map(|now| now + time::Duration::seconds(expires_in))
        .unwrap_or_else(|_| time::OffsetDateTime::now_utc() + time::Duration::seconds(expires_in));
    now.format(&time::format_description::well_known::Rfc3339)
        .map(Value::String)
        .unwrap_or(Value::Null)
}

/// 纯逻辑：把一次 OAuth 登录结果写入账号库。
/// - target 有值：重新登录，只更新该账号；未返回新 refresh token 时保留旧的。
/// - target 无值：新登录，按 email upsert。
/// 返回 (账号下标, 是否当前生效, 是否保留了旧 refresh token, 是否新建账号)。
fn apply_oauth_login(
    accounts: &mut Vec<Account>,
    target: Option<&str>,
    email: &str,
    tokens: &AgyOAuthTokens,
    now_secs: i64,
) -> Result<(usize, bool, bool, bool), String> {
    let expiry = agy_oauth_token_expiry(tokens.expires_in);
    let write_login = |account: &mut Account| {
        if !account.token.is_object() {
            account.token = json!({"auth_method": "consumer", "token": {}});
        }
        if account
            .token
            .get("auth_method")
            .and_then(Value::as_str)
            .is_none()
        {
            account.token["auth_method"] = json!("consumer");
        }
        if !account
            .token
            .get("token")
            .map(Value::is_object)
            .unwrap_or(false)
        {
            account.token["token"] = json!({});
        }
        let old_refresh = account.token["token"]
            .get("refresh_token")
            .and_then(Value::as_str)
            .map(str::to_string);
        account.token["token"]["access_token"] = json!(tokens.access_token);
        if let Some(refresh) = tokens.refresh_token.as_deref() {
            account.token["token"]["refresh_token"] = json!(refresh);
        } else if old_refresh.is_none() {
            object_remove(&mut account.token["token"], "refresh_token");
        }
        account.token["token"]["token_type"] =
            json!(tokens.token_type.clone().unwrap_or_else(|| "Bearer".into()));
        if let Some(expires_in) = tokens.expires_in {
            account.token["token"]["expires_in"] = json!(expires_in);
        } else {
            object_remove(&mut account.token["token"], "expires_in");
        }
        match expiry {
            Value::Null => object_remove(&mut account.token["token"], "expiry"),
            ref value => account.token["token"]["expiry"] = value.clone(),
        }
        match tokens.id_token.as_deref() {
            Some(id_token) => account.token["id_token"] = json!(id_token),
            None => object_remove(&mut account.token, "id_token"),
        }
        if !email.is_empty() {
            account.email = email.to_string();
        }
        account.saved_at = now_secs;
        account.quota_cache = Value::Null;
        account.quota_cached_at = 0;
    };

    if let Some(target_id) = target {
        let index = accounts
            .iter()
            .position(|account| account.id == target_id)
            .ok_or_else(|| "找不到 AGY 账号".to_string())?;
        let is_current = accounts[index].current;
        let refresh_kept = tokens.refresh_token.is_none()
            && accounts[index].token["token"]
                .get("refresh_token")
                .and_then(Value::as_str)
                .is_some();
        write_login(&mut accounts[index]);
        return Ok((index, is_current, refresh_kept, false));
    }

    if !email.is_empty() {
        if let Some(index) = accounts
            .iter()
            .position(|account| account.email.eq_ignore_ascii_case(email))
        {
            write_login(&mut accounts[index]);
            return Ok((index, accounts[index].current, false, false));
        }
    }
    let name = if email.is_empty() {
        "AGY 账号".to_string()
    } else {
        default_account_name(email)
    };
    let mut account = Account {
        id: fresh_account_id(accounts, email),
        name,
        email: email.to_string(),
        token: Value::Null,
        settings: Value::Null,
        current: false,
        saved_at: now_secs,
        quota_cache: Value::Null,
        quota_cached_at: 0,
    };
    write_login(&mut account);
    accounts.push(account);
    Ok((accounts.len() - 1, false, false, true))
}

fn object_remove(value: &mut Value, key: &str) {
    if let Some(object) = value.as_object_mut() {
        object.remove(key);
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
struct Account {
    id: String,
    name: String,
    email: String,
    token: Value,
    settings: Value,
    current: bool,
    saved_at: i64,
    #[serde(default)]
    quota_cache: Value,
    #[serde(default)]
    quota_cached_at: i64,
}
#[derive(Clone)]
pub(crate) struct Manager {
    file: PathBuf,
    home: PathBuf,
    accounts: Arc<RwLock<Vec<Account>>>,
}
impl Manager {
    pub(crate) fn load(app_dir: PathBuf, home: PathBuf) -> Self {
        let file = app_dir.join("webclx-agy-accounts.json");
        let accounts = fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        Self {
            file,
            home,
            accounts: Arc::new(RwLock::new(accounts)),
        }
    }
    fn persist(&self, a: &[Account]) -> Result<(), String> {
        static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
        let t = self.file.with_extension(format!(
            "json.tmp.{:x}.{}",
            std::process::id(),
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&t, serde_json::to_vec_pretty(a).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        fs::rename(t, &self.file).map_err(|e| e.to_string())
    }
    pub(crate) fn list(&self) -> Vec<Value> {
        let token_path = self
            .home
            .join(".gemini/antigravity-cli/antigravity-oauth-token");
        if token_path.exists() {
            self.detect_current();
        }
        self.accounts
            .read()
            .unwrap()
            .iter()
            .map(|a| {
                json!({
                    "id": a.id,
                    "name": a.name,
                    "email": a.email,
                    "current": a.current,
                    "valid": !a.token.is_null(),
                    "saved_at": a.saved_at,
                    "quota_cache": a.quota_cache,
                    "quota_cached_at": a.quota_cached_at,
                })
            })
            .collect()
    }
    pub(crate) fn detect_current(&self) -> Value {
        let token_path = self
            .home
            .join(".gemini/antigravity-cli/antigravity-oauth-token");
        let token_val: Option<Value> = fs::read(&token_path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());

        let mut a = self.accounts.write().unwrap();
        if let Some(disk_accounts) = fs::read(&self.file)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<Account>>(&b).ok())
        {
            *a = disk_accounts;
        }

        let Some(token_json) = token_val else {
            for y in a.iter_mut() {
                y.current = false;
            }
            let _ = self.persist(&a);
            return json!({ "ok": true, "matched": false, "logged_in": false });
        };

        let mut email = token_json
            .get("email")
            .and_then(Value::as_str)
            .or_else(|| {
                token_json
                    .get("token")
                    .and_then(|t| t.get("email"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("")
            .trim()
            .to_string();

        if email.is_empty() {
            if let Some(jwt) = token_json.get("id_token").and_then(Value::as_str) {
                let parts: Vec<&str> = jwt.split('.').collect();
                if parts.len() >= 2 {
                    use base64::Engine;
                    if let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD
                        .decode(parts[1])
                        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(parts[1]))
                    {
                        if let Ok(payload) = serde_json::from_slice::<Value>(&bytes) {
                            if let Some(em) = payload.get("email").and_then(Value::as_str) {
                                email = em.trim().to_string();
                            }
                        }
                    }
                }
            }
        }

        let refresh_token = token_json
            .get("refresh_token")
            .and_then(Value::as_str)
            .or_else(|| {
                token_json
                    .get("token")
                    .and_then(|t| t.get("refresh_token"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("")
            .trim();

        let access_token = token_json
            .get("access_token")
            .and_then(Value::as_str)
            .or_else(|| {
                token_json
                    .get("token")
                    .and_then(|t| t.get("access_token"))
                    .and_then(Value::as_str)
            })
            .unwrap_or("")
            .trim();

        let matched_idx = a.iter().position(|acc| {
            if !email.is_empty() && acc.email.eq_ignore_ascii_case(&email) {
                return true;
            }
            let acc_rt = acc
                .token
                .get("refresh_token")
                .and_then(Value::as_str)
                .or_else(|| {
                    acc.token
                        .get("token")
                        .and_then(|t| t.get("refresh_token"))
                        .and_then(Value::as_str)
                })
                .unwrap_or("")
                .trim();
            if !refresh_token.is_empty() && !acc_rt.is_empty() && acc_rt == refresh_token {
                return true;
            }
            let acc_at = acc
                .token
                .get("access_token")
                .and_then(Value::as_str)
                .or_else(|| {
                    acc.token
                        .get("token")
                        .and_then(|t| t.get("access_token"))
                        .and_then(Value::as_str)
                })
                .unwrap_or("")
                .trim();
            if !access_token.is_empty() && !acc_at.is_empty() && acc_at == access_token {
                return true;
            }
            false
        });

        for (idx, y) in a.iter_mut().enumerate() {
            y.current = Some(idx) == matched_idx;
        }
        let _ = self.persist(&a);

        if let Some(idx) = matched_idx {
            let matched = &a[idx];
            json!({
                "ok": true,
                "matched": true,
                "id": matched.id,
                "name": matched.name,
                "email": matched.email,
            })
        } else {
            json!({
                "ok": true,
                "matched": false,
                "email": email,
                "logged_in": true,
            })
        }
    }
    pub(crate) fn import_snapshots(&self) -> (usize, usize) {
        let root = self.home.join(".gemini/account_backups");
        let mut a = self.accounts.write().unwrap();
        let (mut i, mut s) = (0, 0);
        let Ok(es) = fs::read_dir(root) else {
            return (0, 0);
        };
        for e in es.flatten() {
            let d = e.path();
            if !d.is_dir() || e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let Ok(tb) = fs::read(d.join("data/agy_oauth_token")) else {
                s += 1;
                continue;
            };
            let Ok(t) = serde_json::from_slice::<Value>(&tb) else {
                s += 1;
                continue;
            };
            let set = fs::read(d.join("data/agy_settings"))
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or(Value::Null);
            let m = fs::read(d.join("manifest.json"))
                .ok()
                .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                .unwrap_or(Value::Null);
            let n = m
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("agy 账号")
                .to_string();
            let email = m
                .get("identity")
                .and_then(Value::as_str)
                .unwrap_or("")
                .split(' ')
                .next()
                .unwrap_or("")
                .to_string();
            let id = format!("agy-{}", m.get("fingerprint").and_then(Value::as_str).unwrap_or(&n));
            if a.iter().any(|x| x.id == id) {
                continue;
            }
            a.push(Account {
                id,
                name: n,
                email,
                token: t,
                settings: set,
                current: false,
                saved_at: now(),
                quota_cache: Value::Null,
                quota_cached_at: 0,
            });
            i += 1
        }
        let _ = self.persist(&a);
        (i, s)
    }
    pub(crate) async fn refresh_account_token(&self, id: &str) -> Result<String, String> {
        let (refresh_token, client_id, client_secret) = {
            let a = self.accounts.read().unwrap();
            let Some(account) = a.iter().find(|x| x.id == id) else {
                return Err("找不到 AGY 账号".into());
            };
            let (_, refresh) = oauth_material(&account.token);
            let Some(refresh) = refresh else {
                return Err("账号缺少 OAuth refresh_token".into());
            };
            (refresh, AGY_CLIENT_ID, AGY_CLIENT_SECRET)
        };
        let c = Client::new();
        let resp = c
            .post(AGY_TOKEN_ENDPOINT)
            .form(&[
                ("client_id", client_id),
                ("client_secret", client_secret),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token.as_str()),
            ])
            .send()
            .await
            .map_err(|e| format!("OAuth 刷新请求失败: {e}"))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(format!("OAuth 刷新失败 ({status}): {body}"));
        }

        let body: Value = resp
            .json()
            .await
            .map_err(|e| format!("OAuth 刷新响应无法解析: {e}"))?;
        let access = body
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| "OAuth 刷新响应缺少 access_token".to_string())?;

        let mut accounts = self.accounts.write().unwrap();
        let Some(account) = accounts.iter_mut().find(|x| x.id == id) else {
            return Err("找不到 AGY 账号".into());
        };
        store_access_token(account, access, &body);
        let snapshot = accounts.clone();
        let _ = self.persist(&snapshot);
        Ok(access.to_string())
    }
    pub(crate) async fn apply(&self, id: &str) -> Result<(), String> {
        let needs_refresh = {
            let a = self.accounts.read().unwrap();
            let Some(account) = a.iter().find(|x| x.id == id) else {
                return Err("找不到 AGY 账号".into());
            };
            let (_, refresh) = oauth_material(&account.token);
            is_token_expired_or_expiring(&account.token)
                && refresh.as_deref().map(valid_refresh_token).unwrap_or(false)
        };

        if needs_refresh {
            if let Err(err) = self.refresh_account_token(id).await {
                let a = self.accounts.read().unwrap();
                let has_access = a
                    .iter()
                    .find(|x| x.id == id)
                    .map(|x| oauth_material(&x.token).0.is_some())
                    .unwrap_or(false);
                if !has_access {
                    return Err(format!("AGY 账号 token 已过期且自动刷新失败: {err}"));
                }
            }
        }

        let mut a = self.accounts.write().unwrap();
        let Some(x) = a.iter().find(|x| x.id == id).cloned() else {
            return Err("找不到 AGY 账号".into());
        };
        let d = self.home.join(".gemini/antigravity-cli");
        fs::create_dir_all(&d).map_err(|e| e.to_string())?;
        fs::write(d.join("antigravity-oauth-token"), serde_json::to_vec_pretty(&x.token).unwrap())
            .map_err(|e| e.to_string())?;
        // Settings (including trusted workspaces) belong to the user, not the account.
        for y in a.iter_mut() {
            y.current = y.id == id
        }
        self.persist(&a)
    }
    pub(crate) fn delete(&self, id: &str) -> Result<(), String> {
        let mut a = self.accounts.write().unwrap();
        let len_before = a.len();
        a.retain(|x| x.id != id);
        if a.len() == len_before {
            return Err("找不到 AGY 账号".into());
        }
        self.persist(&a)
    }
    pub(crate) fn test_structure(&self, id: &str) -> Result<Value, String> {
        let a = self.accounts.read().unwrap();
        let Some(acc) = a.iter().find(|x| x.id == id) else {
            return Err("找不到 AGY 账号".into());
        };
        let (access, refresh) = oauth_material(&acc.token);
        if refresh.is_none() && access.is_none() {
            return Err("账号缺少有效 token 或 refresh_token".into());
        }
        Ok(json!({"ok": true, "id": acc.id, "email": acc.email}))
    }
    pub(crate) fn cached_quota(&self, id: &str) -> Value {
        self.accounts
            .read()
            .unwrap()
            .iter()
            .find(|account| account.id == id)
            .map(|account| {
                json!({
                    "ok": true,
                    "cached": true,
                    "updatedAt": account.quota_cached_at,
                    "quotaSummary": account.quota_cache.get("quotaSummary").cloned().unwrap_or(Value::Null),
                })
            })
            .unwrap_or(json!({"ok": false, "error": "找不到 AGY 账号"}))
    }
    pub(crate) fn import_refresh_accounts(&self, body: &Value) -> Result<Value, String> {
        let items = collect_import_items(body)?;
        if items.is_empty() {
            return Err("没有可导入的账号".into());
        }
        let mut accounts = self.accounts.write().unwrap();
        let mut imported = 0usize;
        let mut updated = 0usize;
        let mut skipped = 0usize;
        let mut errors = Vec::new();
        let mut seen = HashSet::new();
        for (index, item) in items.iter().enumerate() {
            let label = format!("第 {} 条", index + 1);
            if !item.is_object() {
                skipped += 1;
                errors.push(format!("{label}不是对象"));
                continue;
            }
            let email = match non_empty_str(item, &["email"])
                .as_deref()
                .map(normalize_email)
            {
                Some(Ok(email)) => email,
                _ => {
                    skipped += 1;
                    errors.push(format!("{label}缺少有效 email"));
                    continue;
                }
            };
            let refresh = match non_empty_str(item, &["refresh_token", "refreshToken"]) {
                Some(token) if valid_refresh_token(&token) => token,
                Some(_) => {
                    skipped += 1;
                    errors.push(format!("{label}的 refresh_token 无效"));
                    continue;
                }
                None => {
                    skipped += 1;
                    errors.push(format!("{label}缺少 refresh_token"));
                    continue;
                }
            };
            let key = email.to_ascii_lowercase();
            if !seen.insert(key) {
                if let Some(found) = find_import_target(&accounts, &email, &refresh) {
                    write_refresh_token(&mut accounts[found], &refresh);
                    accounts[found].email = email;
                    accounts[found].saved_at = now();
                }
                continue;
            }
            if let Some(found) = find_import_target(&accounts, &email, &refresh) {
                write_refresh_token(&mut accounts[found], &refresh);
                accounts[found].email = email.clone();
                if accounts[found].name.trim().is_empty() {
                    accounts[found].name = default_account_name(&email);
                }
                accounts[found].saved_at = now();
                updated += 1;
            } else {
                let mut account = Account {
                    id: fresh_account_id(&accounts, &email),
                    name: default_account_name(&email),
                    email: email.clone(),
                    token: Value::Null,
                    settings: Value::Null,
                    current: false,
                    saved_at: now(),
                    quota_cache: Value::Null,
                    quota_cached_at: 0,
                };
                write_refresh_token(&mut account, &refresh);
                accounts.push(account);
                imported += 1;
            }
        }
        if imported == 0 && updated == 0 {
            return Err(if errors.is_empty() {
                "没有可导入的账号".to_string()
            } else {
                errors.join("；")
            });
        }
        self.persist(&accounts)?;
        Ok(json!({
            "ok": true,
            "imported": imported,
            "updated": updated,
            "skipped": skipped,
            "errors": errors,
        }))
    }
    /// 导出 Antigravity Manager 使用的账号数组，每项只有 email 和 refresh_token。
    /// 响应放在 accounts 里；下载文件和剪贴板只写这个数组，不包含 access token。
    pub(crate) fn export_refresh_accounts(&self, body: &Value) -> Result<Value, String> {
        let ids = export_id_filter(body);
        let accounts = self.accounts.read().unwrap();
        let mut exported = Vec::new();
        let mut skipped = 0usize;
        for account in accounts.iter() {
            if let Some(ids) = &ids {
                if !ids.contains(&account.id) {
                    continue;
                }
            }
            let Some(refresh) = oauth_material(&account.token).1 else {
                skipped += 1;
                continue;
            };
            if !valid_refresh_token(&refresh) || normalize_email(account.email.trim()).is_err() {
                skipped += 1;
                continue;
            }
            exported.push(json!({
                "email": account.email.trim(),
                "refresh_token": refresh,
            }));
        }
        if exported.is_empty() {
            return Err("没有可导出的账号".into());
        }
        Ok(json!({
            "ok": true,
            "accounts": exported,
            "skipped": skipped,
        }))
    }
    /// 写入一次 Antigravity OAuth 登录结果：
    /// - target 有值：重新登录该账号；若它是当前生效账号，同步刷新 live token 文件。
    /// - target 无值：新登录，按 email upsert（同邮箱更新，否则新建）。
    pub(crate) async fn save_oauth_login(
        &self,
        target: Option<&str>,
        email: &str,
        tokens: &AgyOAuthTokens,
    ) -> Result<Value, String> {
        let (summary, live_snapshot) = {
            let mut accounts = self.accounts.write().unwrap();
            let (index, is_current, refresh_kept, created) =
                apply_oauth_login(&mut accounts, target, email, tokens, now())?;
            let account = &accounts[index];
            let summary = json!({
                "ok": true,
                "relogin": target.is_some(),
                "created": created,
                "id": account.id,
                "name": account.name,
                "email": account.email,
                "current": account.current,
                "refresh_kept": refresh_kept,
            });
            let live_snapshot = if is_current {
                Some(account.token.clone())
            } else {
                None
            };
            self.persist(&accounts)?;
            (summary, live_snapshot)
        };
        if let Some(token) = live_snapshot {
            let d = self.home.join(".gemini/antigravity-cli");
            fs::create_dir_all(&d).map_err(|e| e.to_string())?;
            fs::write(
                d.join("antigravity-oauth-token"),
                serde_json::to_vec_pretty(&token).unwrap(),
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(summary)
    }
    pub(crate) fn require_account(&self, id: &str) -> Result<(), String> {
        if self.accounts.read().unwrap().iter().any(|a| a.id == id) {
            Ok(())
        } else {
            Err("找不到 AGY 账号".into())
        }
    }
    /// 读取 AGY CLI 选项：
    /// - dangerous：permissions.allow 是否包含 "*"（危险也不问，真正 full pass）；
    ///   同时检查 toolPermission 是否为 always-proceed，这是 AGY 1.3+ 的实际
    ///   全通执行策略。两者都要满足，否则只配 allow 规则但命令仍走 request-review。
    /// - autoEdits：executionMode 是否为 accept-edits（自动批准文件编辑）。
    /// 注意 AGY 的文件编辑确认由执行模式控制，与 allow 规则是两层。
    pub(crate) fn options(&self) -> Value {
        json!({
            "dangerous": self.read_settings_full_pass(),
            "autoEdits": self.read_execution_mode() == Some("accept-edits"),
        })
    }
    /// 写入 AGY CLI 选项：
    /// - dangerous：只增删 permissions.allow 里的 "*" 这一条（开启时同时把 deny/ask
    ///   里的 "*" 移除以保证 full pass）。用户手工维护的其余 allow 条目一律保留。
    /// - autoEdits：把 executionMode 设为 accept-edits；关闭时仅在该键当前值就是
    ///   accept-edits 时移除（用户手设的 plan 等其他模式不被覆盖）。
    pub(crate) fn set_options(&self, dangerous: bool, auto_edits: bool) -> Result<Value, String> {
        self.write_settings_full_pass(dangerous)?;
        self.write_execution_mode(auto_edits)?;
        Ok(json!({
            "ok": true,
            "dangerous": dangerous,
            "autoEdits": auto_edits,
        }))
    }
    fn settings_file(&self) -> PathBuf {
        self.home
            .join(".gemini/antigravity-cli")
            .join("settings.json")
    }
    fn read_settings_full_pass(&self) -> bool {
        let Ok(content) = fs::read_to_string(self.settings_file()) else {
            return false;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            return false;
        };
        value
            .get("permissions")
            .and_then(|permissions| permissions.get("allow"))
            .and_then(Value::as_array)
            .map(|allow| allow.iter().any(|item| item.as_str() == Some("*")))
            .unwrap_or(false)
            && value.get("toolPermission").and_then(Value::as_str) == Some("always-proceed")
    }

    /// AGY 1.3+ 的 always-proceed 是命令免提示的实际开关；
    /// 即使 allow 里有 "*"，如果 toolPermission 还是 request-review，仍会询问。
    fn ensure_tool_permission_always_proceed(&self, value: &mut Value) -> Result<(), String> {
        let object = value
            .as_object_mut()
            .ok_or("AGY settings.json 顶层不是对象")?;
        object.insert("toolPermission".to_string(), json!("always-proceed"));
        Ok(())
    }
    fn write_settings_full_pass(&self, dangerous: bool) -> Result<(), String> {
        let path = self.settings_file();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut value: Value = fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_else(|| json!({}));
        if !value.is_object() {
            value = json!({});
        }
        let object = value
            .as_object_mut()
            .ok_or("AGY settings.json 顶层不是对象")?;
        if !object
            .get("permissions")
            .map(Value::is_object)
            .unwrap_or(false)
        {
            object.insert("permissions".to_string(), json!({}));
        }
        let permissions = object
            .get_mut("permissions")
            .and_then(Value::as_object_mut)
            .ok_or("AGY settings.json 的 permissions 字段不是对象")?;
        for key in ["allow", "deny", "ask"] {
            if !permissions.get(key).map(Value::is_array).unwrap_or(false) {
                continue;
            }
            let items = permissions
                .get_mut(key)
                .and_then(Value::as_array_mut)
                .unwrap();
            items.retain(|item| item.as_str() != Some("*"));
        }
        if dangerous {
            let allow = permissions.entry("allow").or_insert_with(|| json!([]));
            if let Some(items) = allow.as_array_mut() {
                items.push(json!("*"));
            }
            self.ensure_tool_permission_always_proceed(&mut value)?;
        } else if let Some(object) = value.as_object_mut() {
            object.remove("toolPermission");
        }
        let content = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
        fs::write(&path, content).map_err(|error| error.to_string())
    }
    fn read_execution_mode(&self) -> Option<&'static str> {
        let Ok(content) = fs::read_to_string(self.settings_file()) else {
            return None;
        };
        let Ok(value) = serde_json::from_str::<Value>(&content) else {
            return None;
        };
        match value.get("executionMode").and_then(Value::as_str) {
            Some("accept-edits") => Some("accept-edits"),
            Some("default") => Some("default"),
            Some("plan") => Some("plan"),
            _ => None,
        }
    }
    fn write_execution_mode(&self, auto_edits: bool) -> Result<(), String> {
        let path = self.settings_file();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let mut value: Value = fs::read_to_string(&path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_else(|| json!({}));
        if !value.is_object() {
            value = json!({});
        }
        let object = value
            .as_object_mut()
            .ok_or("AGY settings.json 顶层不是对象")?;
        let current = object.get("executionMode").and_then(Value::as_str);
        if auto_edits {
            object.insert("executionMode".to_string(), json!("accept-edits"));
        } else if current == Some("accept-edits") {
            object.remove("executionMode");
        }
        let content = serde_json::to_string_pretty(&value).map_err(|error| error.to_string())?;
        fs::write(&path, content).map_err(|error| error.to_string())
    }
    pub(crate) async fn quota(&self, id: &str) -> Value {
        let (mut token, refresh_token) = {
            let a = self.accounts.read().unwrap();
            let Some(account) = a.iter().find(|x| x.id == id) else {
                return json!({"ok":false,"error":"找不到 AGY 账号"});
            };
            let (access, refresh) = oauth_material(&account.token);
            if access.is_none() && refresh.is_none() {
                return json!({"ok":false,"error":"账号缺少 OAuth refresh_token"});
            }
            (access.unwrap_or_default(), refresh)
        };
        if let Some(refresh_token) = refresh_token {
            if valid_refresh_token(&refresh_token) {
                match self.refresh_account_token(id).await {
                    Ok(access) => token = access,
                    Err(err) => return json!({"ok":false,"error":err}),
                }
            }
        }
        let c = Client::new();
        if token.is_empty() {
            return json!({"ok":false,"error":"账号缺少 OAuth access_token"});
        }
        let load=c.post("https://daily-cloudcode-pa.googleapis.com/v1internal:loadCodeAssist")
            .bearer_auth(&token)
            .header("user-agent", "antigravity/1.20.5 linux/amd64 google-api-nodejs-client/10.3.0")
            .header("x-goog-api-client", "gl-node/22.21.1")
            .json(&json!({"metadata":{"ideName":"antigravity","ideType":"ANTIGRAVITY","ideVersion":"1.20.5","pluginVersion":"1.8.16","platform":"LINUX_AMD64","updateChannel":"stable","pluginType":"GEMINI"},"mode":"FULL_ELIGIBILITY_CHECK"}))
            .send().await;
        let Ok(resp) = load else {
            return json!({"ok":false,"error":"请求 loadCodeAssist 失败"});
        };
        let load_status = resp.status();
        let Ok(load): Result<Value, _> = resp.json().await else {
            return json!({"ok":false,"error":"解析配额响应失败"});
        };
        if !load_status.is_success() {
            let error = load.get("error").cloned().unwrap_or_else(|| load.clone());
            return json!({"ok":false,"error":error});
        }
        let project = load
            .get("cloudaicompanionProject")
            .or_else(|| load.get("project"))
            .cloned()
            .unwrap_or(Value::Null);
        let project_id = project.as_str().map(str::to_string).or_else(|| {
            project
                .get("id")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        let models = c
            .post("https://daily-cloudcode-pa.googleapis.com/v1internal:fetchAvailableModels")
            .bearer_auth(&token)
            .header("user-agent", "antigravity/1.20.5 linux/amd64")
            .json(&if project.is_null() {
                json!({})
            } else {
                json!({"project":project})
            })
            .send()
            .await;
        let Ok(resp) = models else {
            return json!({"ok":false,"error":"请求 fetchAvailableModels 失败"});
        };
        let models_status = resp.status();
        let body = resp.json::<Value>().await.unwrap_or(Value::Null);
        if !models_status.is_success() || body.get("error").is_some() {
            let error = body.get("error").cloned().unwrap_or_else(|| body.clone());
            return json!({"ok":false,"error":error});
        }
        let payload = match project_id.as_deref() {
            Some(project) => json!({"project": project}),
            None => json!({}),
        };
        let summary_response = c
            .post("https://daily-cloudcode-pa.googleapis.com/v1internal:retrieveUserQuotaSummary")
            .bearer_auth(&token)
            .header("user-agent", "antigravity/1.20.5 linux/amd64")
            .json(&payload)
            .send()
            .await;
        let quota_summary = match summary_response {
            Ok(resp) if resp.status().is_success() => {
                resp.json::<Value>().await.unwrap_or(Value::Null)
            }
            _ => Value::Null,
        };
        let result = json!({
            "ok":true,
            "updatedAt":now(),
            "models":body.get("models").cloned().unwrap_or(body),
            "quotaSummary":quota_summary,
            "plan":load.get("planInfo").cloned()
        });
        {
            let mut accounts = self.accounts.write().unwrap();
            if let Some(account) = accounts.iter_mut().find(|account| account.id == id) {
                account.quota_cache = json!({
                    "ok": true,
                    "updatedAt": result.get("updatedAt").cloned().unwrap_or(Value::Null),
                    "quotaSummary": result.get("quotaSummary").cloned().unwrap_or(Value::Null),
                });
                account.quota_cached_at = result
                    .get("updatedAt")
                    .and_then(Value::as_i64)
                    .unwrap_or_else(now);
                drop(accounts);
                let accounts = self.accounts.read().unwrap();
                let snapshot = accounts.clone();
                drop(accounts);
                if let Err(error) = self.persist(&snapshot) {
                    return json!({
                        "ok": false,
                        "error": format!("保存配额缓存失败: {}", error),
                    });
                }
            }
        }
        result
    }
}
fn non_empty_str(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string)
    })
}

fn oauth_material(token: &Value) -> (Option<String>, Option<String>) {
    let nested = token.get("token").filter(|value| value.is_object());
    let primary = nested.unwrap_or(token);
    let mut access = non_empty_str(primary, &["access_token"]);
    let mut refresh = non_empty_str(primary, &["refresh_token", "refreshToken"]);
    if nested.is_some() {
        if access.is_none() {
            access = non_empty_str(token, &["access_token"]);
        }
        if refresh.is_none() {
            refresh = non_empty_str(token, &["refresh_token", "refreshToken"]);
        }
    }
    if access.is_none() {
        // Google 扁平 oauth_creds 格式把 access token 存在字符串字段 `token` 里
        access = token
            .get("token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string);
    }
    (access, refresh)
}

fn is_token_expired_or_expiring(token: &Value) -> bool {
    let nested = token.get("token").filter(|value| value.is_object());
    let primary = nested.unwrap_or(token);
    let mut access = non_empty_str(primary, &["access_token"]);
    if nested.is_some() && access.is_none() {
        access = non_empty_str(token, &["access_token"]);
    }
    if access.is_none() {
        access = token
            .get("token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(str::to_string);
    }
    if access.is_none() {
        return true;
    }
    let expiry_val = primary.get("expiry").or_else(|| token.get("expiry"));
    let Some(expiry_str) = expiry_val.and_then(Value::as_str) else {
        return true;
    };
    match time::OffsetDateTime::parse(expiry_str, &time::format_description::well_known::Rfc3339) {
        Ok(exp) => {
            let now = time::OffsetDateTime::now_utc();
            exp <= now + time::Duration::seconds(120)
        }
        Err(_) => true,
    }
}

fn normalize_email(raw: &str) -> Result<String, ()> {
    let email = raw.trim();
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if local.is_empty()
        || domain.is_empty()
        || domain.contains('@')
        || parts.next().is_some()
        || email.chars().any(char::is_whitespace)
    {
        return Err(());
    }
    Ok(email.to_string())
}

fn valid_refresh_token(token: &str) -> bool {
    token.len() >= 16 && !token.chars().any(char::is_whitespace)
}

fn default_account_name(email: &str) -> String {
    email
        .split('@')
        .next()
        .filter(|part| !part.is_empty())
        .unwrap_or(email)
        .chars()
        .take(120)
        .collect()
}

fn email_account_id(email: &str) -> String {
    let digest = Sha256::digest(email.to_ascii_lowercase().as_bytes());
    let hex: String = digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    format!("agy-{hex}")
}

fn fresh_account_id(accounts: &[Account], email: &str) -> String {
    let base = email_account_id(email);
    if accounts.iter().all(|account| account.id != base) {
        return base;
    }
    for suffix in 2..1000 {
        let candidate = format!("{base}-{suffix}");
        if accounts.iter().all(|account| account.id != candidate) {
            return candidate;
        }
    }
    format!("{base}-{}", now())
}

fn find_import_target(accounts: &[Account], email: &str, refresh: &str) -> Option<usize> {
    if let Some(index) = accounts
        .iter()
        .position(|account| account.email.eq_ignore_ascii_case(email))
    {
        return Some(index);
    }
    accounts
        .iter()
        .position(|account| oauth_material(&account.token).1.as_deref() == Some(refresh))
}

fn write_refresh_token(account: &mut Account, refresh: &str) {
    let changed = oauth_material(&account.token).1.as_deref() != Some(refresh);
    if !account.token.is_object() {
        account.token = json!({});
    }
    if account
        .token
        .get("auth_method")
        .and_then(Value::as_str)
        .is_none()
    {
        account.token["auth_method"] = Value::String("consumer".into());
    }
    if !account
        .token
        .get("token")
        .map(Value::is_object)
        .unwrap_or(false)
    {
        account.token["token"] = json!({});
    }
    account.token["token"]["refresh_token"] = Value::String(refresh.to_string());
    if account.token["token"]
        .get("token_type")
        .and_then(Value::as_str)
        .is_none()
    {
        account.token["token"]["token_type"] = json!("Bearer");
    }
    if changed {
        if let Some(object) = account.token["token"].as_object_mut() {
            object.remove("access_token");
            object.remove("expires_in");
            object.remove("expiry");
        }
    }
}

fn store_access_token(account: &mut Account, access: &str, body: &Value) {
    if !account.token.is_object() {
        account.token = json!({"auth_method": "consumer"});
    }
    if !account
        .token
        .get("token")
        .map(Value::is_object)
        .unwrap_or(false)
    {
        account.token["token"] = json!({});
    }
    account.token["token"]["access_token"] = Value::String(access.to_string());
    if let Some(expires) = body.get("expires_in") {
        account.token["token"]["expires_in"] = expires.clone();
        let expires_in_secs = expires
            .as_i64()
            .or_else(|| expires.as_str().and_then(|s| s.parse::<i64>().ok()));
        match agy_oauth_token_expiry(expires_in_secs) {
            Value::Null => object_remove(&mut account.token["token"], "expiry"),
            ref value => account.token["token"]["expiry"] = value.clone(),
        }
    }
    if let Some(expiry) = body.get("expiry").and_then(Value::as_str) {
        account.token["token"]["expiry"] = Value::String(expiry.to_string());
    }
    if let Some(token_type) = body.get("token_type").and_then(Value::as_str) {
        account.token["token"]["token_type"] = Value::String(token_type.to_string());
    }
    if let Some(id_token) = body.get("id_token").and_then(Value::as_str) {
        account.token["id_token"] = Value::String(id_token.to_string());
    }
}

fn export_id_filter(body: &Value) -> Option<HashSet<String>> {
    let ids = body.get("ids")?.as_array()?;
    let set = ids
        .iter()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect::<HashSet<_>>();
    if set.is_empty() { None } else { Some(set) }
}

fn collect_import_items(body: &Value) -> Result<Vec<Value>, String> {
    if let Some(text) = body.as_str() {
        let parsed: Value = serde_json::from_str(text.trim().trim_start_matches('\u{feff}'))
            .map_err(|_| "文本不是合法 JSON".to_string())?;
        return collect_import_items(&parsed);
    }
    if let Some(items) = body.as_array() {
        return Ok(items.clone());
    }
    if body.get("email").is_some()
        || body.get("refresh_token").is_some()
        || body.get("refreshToken").is_some()
    {
        return Ok(vec![body.clone()]);
    }
    for key in ["accounts", "items", "data"] {
        if let Some(items) = body.get(key).and_then(Value::as_array) {
            return Ok(items.clone());
        }
    }
    Err("需要包含 email 和 refresh_token 的 JSON 数组".into())
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
pub(crate) fn routes() -> axum::Router<crate::AppState> {
    use axum::{
        Json,
        extract::{Path, State},
        routing::{get, post, put},
    };
    axum::Router::new()
        .route(
            "/api/agy/presets",
            get(|State(s): State<crate::AppState>| async move { Json(s.agy_manager.list()) }),
        )
        .route(
            "/api/agy/presets/detect-current",
            post(|State(s): State<crate::AppState>| async move {
                Json(s.agy_manager.detect_current())
            }),
        )
        .route(
            "/api/agy/presets/import",
            post(|State(s): State<crate::AppState>, Json(body): Json<Value>| async move {
                match s.agy_manager.import_refresh_accounts(&body) {
                    Ok(value) => (axum::http::StatusCode::OK, Json(value)),
                    Err(error) => (
                        axum::http::StatusCode::BAD_REQUEST,
                        Json(json!({"ok": false, "error": error})),
                    ),
                }
            }),
        )
        .route(
            "/api/agy/presets/export",
            post(|State(s): State<crate::AppState>, Json(body): Json<Value>| async move {
                match s.agy_manager.export_refresh_accounts(&body) {
                    Ok(value) => (axum::http::StatusCode::OK, Json(value)),
                    Err(error) => (
                        axum::http::StatusCode::BAD_REQUEST,
                        Json(json!({"ok": false, "error": error})),
                    ),
                }
            }),
        )
        .route(
            "/api/agy/presets/{id}",
            axum::routing::delete(
                |State(s): State<crate::AppState>, Path(id): Path<String>| async move {
                    match s.agy_manager.delete(&id) {
                        Ok(()) => (axum::http::StatusCode::OK, Json(json!({"ok": true}))),
                        Err(e) => (
                            axum::http::StatusCode::BAD_REQUEST,
                            Json(json!({"ok": false, "error": e})),
                        ),
                    }
                },
            ),
        )
        .route(
            "/api/agy/presets/{id}/test",
            post(|State(s): State<crate::AppState>, Path(id): Path<String>| async move {
                match s.agy_manager.test_structure(&id) {
                    Ok(val) => (axum::http::StatusCode::OK, Json(val)),
                    Err(e) => (
                        axum::http::StatusCode::BAD_REQUEST,
                        Json(json!({"ok": false, "error": e})),
                    ),
                }
            }),
        )
        .route(
            "/api/agy/presets/{id}/apply",
            put(|State(s): State<crate::AppState>, Path(id): Path<String>| async move {
                match s.agy_manager.apply(&id).await {
                    Ok(()) => (axum::http::StatusCode::OK, Json(json!({"ok": true}))),
                    Err(e) => (
                        axum::http::StatusCode::BAD_REQUEST,
                        Json(json!({"ok": false, "error": e})),
                    ),
                }
            })
            .post(|State(s): State<crate::AppState>, Path(id): Path<String>| async move {
                match s.agy_manager.apply(&id).await {
                    Ok(()) => (axum::http::StatusCode::OK, Json(json!({"ok": true}))),
                    Err(e) => (
                        axum::http::StatusCode::BAD_REQUEST,
                        Json(json!({"ok": false, "error": e})),
                    ),
                }
            }),
        )
        .route(
            "/api/agy/oauth/start",
            post(
                |State(s): State<crate::AppState>, body: String| async move {
                    let target_id = serde_json::from_str::<Value>(body.trim())
                        .ok()
                        .and_then(|value| {
                            value
                                .get("target_id")
                                .and_then(Value::as_str)
                                .map(str::trim)
                                .filter(|id| !id.is_empty())
                                .map(str::to_string)
                        });
                    if let Some(target) = target_id.as_deref() {
                        if let Err(e) = s.agy_manager.require_account(target) {
                            return (
                                axum::http::StatusCode::BAD_REQUEST,
                                Json(json!({"ok": false, "error": e})),
                            );
                        }
                    }
                    match agy_oauth_insert_session(target_id.clone()) {
                        Ok((session_id, state)) => {
                            match agy_oauth_auth_url(&state) {
                                Ok(auth_url) => (
                                    axum::http::StatusCode::OK,
                                    Json(json!({
                                        "ok": true,
                                        "session_id": session_id,
                                        "auth_url": auth_url,
                                        "state": state,
                                        "target_id": target_id,
                                    })),
                                ),
                                Err(e) => (
                                    axum::http::StatusCode::BAD_REQUEST,
                                    Json(json!({"ok": false, "error": e})),
                                ),
                            }
                        }
                        Err(e) => (
                            axum::http::StatusCode::BAD_REQUEST,
                            Json(json!({"ok": false, "error": e})),
                        ),
                    }
                },
            ),
        )
        .route(
            "/api/agy/oauth/exchange",
            post(
                |State(s): State<crate::AppState>, Json(body): Json<Value>| async move {
                    let bad_request = |message: String| {
                        (
                            axum::http::StatusCode::BAD_REQUEST,
                            Json(json!({"ok": false, "error": message})),
                        )
                    };
                    let session_id = body
                        .get("session_id")
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .unwrap_or_default();
                    if session_id.is_empty() {
                        return bad_request("缺少 session_id，请重新发起登录。".into());
                    }
                    let session = match agy_oauth_take_session(session_id) {
                        Ok(session) => session,
                        Err(e) => return bad_request(e),
                    };
                    let (code, callback_state) = {
                        let raw_code = body
                            .get("code")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .unwrap_or_default()
                            .to_string();
                        let callback = body
                            .get("callback")
                            .and_then(Value::as_str)
                            .map(str::trim)
                            .unwrap_or_default();
                        if !raw_code.is_empty() {
                            (raw_code, String::new())
                        } else if !callback.is_empty() {
                            agy_oauth_parse_callback(callback)
                        } else {
                            (String::new(), String::new())
                        }
                    };
                    if code.is_empty() {
                        return bad_request("缺少授权码，请粘贴登录回调 URL 或其中的 code。".into());
                    }
                    let supplied_state = [
                        body.get("state").and_then(Value::as_str).map(str::trim),
                        (!callback_state.is_empty()).then_some(callback_state.as_str()),
                    ]
                    .into_iter()
                    .flatten()
                    .find(|state| !state.is_empty())
                    .unwrap_or_default();
                    if supplied_state != session.state {
                        return bad_request("state 校验失败，请重新发起登录。".into());
                    }
                    let client = match agy_oauth_client() {
                        Ok(client) => client,
                        Err(e) => return bad_request(e),
                    };
                    let tokens = match agy_oauth_exchange_code(&client, &code).await {
                        Ok(tokens) => tokens,
                        Err(e) => return bad_request(e),
                    };
                    let email = agy_oauth_fetch_email(&client, &tokens).await;
                    if email.is_empty() {
                        return bad_request(
                            "未能获取账号邮箱（userinfo 与 id_token 均无 email）。".into(),
                        );
                    }
                    match s
                        .agy_manager
                        .save_oauth_login(session.target_id.as_deref(), &email, &tokens)
                        .await
                    {
                        Ok(value) => (axum::http::StatusCode::OK, Json(value)),
                        Err(e) => bad_request(e),
                    }
                },
            ),
        )
        .route(
            "/api/agy/presets/{id}/quota",
            get(|State(s): State<crate::AppState>, Path(id): Path<String>| async move {
                Json(s.agy_manager.quota(&id).await)
            }),
        )
        .route(
            "/api/agy/options",
            get(|State(s): State<crate::AppState>| async move { Json(s.agy_manager.options()) })
                .put(
                    |State(s): State<crate::AppState>, Json(body): Json<Value>| async move {
                        let dangerous = body.get("dangerous").and_then(Value::as_bool).unwrap_or(false);
                        let auto_edits = body.get("auto_edits").and_then(Value::as_bool).unwrap_or(false);
                        match s.agy_manager.set_options(dangerous, auto_edits) {
                            Ok(value) => (axum::http::StatusCode::OK, Json(value)),
                            Err(error) => (
                                axum::http::StatusCode::BAD_REQUEST,
                                Json(json!({"ok": false, "error": error})),
                            ),
                        }
                    },
                ),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_app_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "webclx-agy-quota-cache-{label}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[tokio::test]
    async fn account_switch_preserves_live_settings() {
        let dir = temp_app_dir("switch-settings");
        let manager = Manager::load(dir.clone(), dir.join("home"));
        let live = manager.home.join(".gemini/antigravity-cli");
        fs::create_dir_all(&live).unwrap();
        let original = br#"{"trustedWorkspaces":["/work/one"],"model":"live","custom":true}"#;
        fs::write(live.join("settings.json"), original).unwrap();
        for id in ["one", "two"] {
            manager.accounts.write().unwrap().push(Account {
                id: id.into(),
                token: json!({"token": {"refresh_token": id}}),
                settings: json!({"trustedWorkspaces": [], "model": "stale"}),
                ..Account::default()
            });
        }
        for id in ["one", "two", "one"] {
            manager.apply(id).await.unwrap();
            assert_eq!(fs::read(live.join("settings.json")).unwrap(), original);
            let token: Value =
                serde_json::from_slice(&fs::read(live.join("antigravity-oauth-token")).unwrap())
                    .unwrap();
            assert_eq!(token["token"]["refresh_token"], id);
        }
        fs::remove_file(live.join("settings.json")).unwrap();
        manager.apply("two").await.unwrap();
        assert!(!live.join("settings.json").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dangerous_option_writes_always_proceed_and_reads_back() {
        let dir = temp_app_dir("dangerous-always-proceed");
        let manager = Manager::load(dir.clone(), dir.join("home"));
        let live = manager.home.join(".gemini/antigravity-cli");
        fs::create_dir_all(&live).unwrap();

        manager.set_options(true, false).unwrap();
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(live.join("settings.json")).unwrap()).unwrap();
        assert_eq!(
            settings["toolPermission"],
            json!("always-proceed"),
            "AGY 1.3+ 需要 toolPermission 才能免命令提示"
        );
        assert_eq!(manager.options()["dangerous"], json!(true));

        manager.set_options(false, false).unwrap();
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(live.join("settings.json")).unwrap()).unwrap();
        assert_ne!(settings["toolPermission"], json!("always-proceed"));
        assert_eq!(manager.options()["dangerous"], json!(false));

        fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn current_account_relogin_preserves_live_settings() {
        let dir = temp_app_dir("relogin-settings");
        let manager = Manager::load(dir.clone(), dir.join("home"));
        manager.accounts.write().unwrap().push(Account {
            id: "current".into(),
            current: true,
            settings: json!({"trustedWorkspaces": []}),
            ..Account::default()
        });
        let live = manager.home.join(".gemini/antigravity-cli");
        fs::create_dir_all(&live).unwrap();
        let original =
            br#"{"trustedWorkspaces":["/work/one","/work/two"],"permissions":{"custom":true}}"#;
        fs::write(live.join("settings.json"), original).unwrap();
        let tokens = AgyOAuthTokens {
            access_token: "new-access".into(),
            refresh_token: Some("1//new-refresh".into()),
            expires_in: Some(3600),
            token_type: Some("Bearer".into()),
            id_token: None,
        };
        manager
            .save_oauth_login(Some("current"), "user@example.com", &tokens)
            .await
            .unwrap();
        assert_eq!(fs::read(live.join("settings.json")).unwrap(), original);
        let token: Value =
            serde_json::from_slice(&fs::read(live.join("antigravity-oauth-token")).unwrap())
                .unwrap();
        assert_eq!(token["token"]["access_token"], "new-access");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn oauth_login_creates_updates_and_relogs_accounts() {
        let mut accounts: Vec<Account> = vec![Account {
            id: "agy-existing".to_string(),
            name: "已有".to_string(),
            email: "kept@example.com".to_string(),
            token: json!({
                "auth_method": "consumer",
                "token": {"access_token": "old", "refresh_token": "1//old-refresh", "token_type": "Bearer"}
            }),
            settings: json!({"model": "gemini"}),
            current: true,
            saved_at: 1,
            quota_cache: json!({"ok": true}),
            quota_cached_at: 9,
        }];
        let tokens = AgyOAuthTokens {
            access_token: "ya29.new".to_string(),
            refresh_token: Some("1//new-refresh".to_string()),
            expires_in: Some(3600),
            token_type: Some("Bearer".to_string()),
            id_token: Some("header.eyJlbWFpbCI6Im5ld0BleGFtcGxlLmNvbSJ9.sig".to_string()),
        };

        // 新登录：同邮箱更新，不重复建号
        let (index, is_current, refresh_kept, created) =
            apply_oauth_login(&mut accounts, None, "kept@example.com", &tokens, 42).unwrap();
        assert_eq!(index, 0);
        assert_eq!(accounts.len(), 1);
        assert!(is_current);
        assert!(!refresh_kept);
        assert!(!created);
        assert_eq!(accounts[0].token["token"]["refresh_token"], "1//new-refresh");
        assert_eq!(accounts[0].token["token"]["access_token"], "ya29.new");
        assert_eq!(accounts[0].token["id_token"], tokens.id_token.clone().unwrap());
        assert!(accounts[0].token["token"]["expiry"].is_string());
        assert_eq!(accounts[0].saved_at, 42);
        assert_eq!(accounts[0].quota_cached_at, 0);
        assert_eq!(accounts[0].settings["model"], "gemini");

        // 新登录：新邮箱建号
        let (index, _, _, created) =
            apply_oauth_login(&mut accounts, None, "new.user@gmail.com", &tokens, 43).unwrap();
        assert_eq!(index, 1);
        assert!(created);
        assert_eq!(accounts[1].email, "new.user@gmail.com");
        assert_eq!(accounts[1].name, "new.user");
        assert_eq!(accounts[1].token["token"]["refresh_token"], "1//new-refresh");

        // 重新登录：未返回新 refresh token 时保留旧的
        let mut kept_tokens = AgyOAuthTokens {
            access_token: "ya29.refreshed".to_string(),
            refresh_token: None,
            expires_in: None,
            token_type: None,
            id_token: None,
        };
        let (index, is_current, refresh_kept, created) = apply_oauth_login(
            &mut accounts,
            Some("agy-existing"),
            "kept@example.com",
            &kept_tokens,
            44,
        )
        .unwrap();
        assert_eq!(index, 0);
        assert!(is_current);
        assert!(refresh_kept);
        assert!(!created);
        assert_eq!(accounts[0].token["token"]["refresh_token"], "1//new-refresh");
        assert_eq!(accounts[0].token["token"]["access_token"], "ya29.refreshed");
        assert!(accounts[0].token["token"].get("expires_in").is_none());
        assert!(accounts[0].token.get("id_token").is_none());
        assert_eq!(accounts[0].quota_cache, Value::Null);

        // 重新登录：换绑不同邮箱时以新邮箱覆盖
        kept_tokens.access_token = "ya29.other".to_string();
        let (_, _, _, _) = apply_oauth_login(
            &mut accounts,
            Some("agy-existing"),
            "moved@example.com",
            &kept_tokens,
            45,
        )
        .unwrap();
        assert_eq!(accounts[0].email, "moved@example.com");

        // 重新登录：目标不存在报错
        assert!(
            apply_oauth_login(&mut accounts, Some("agy-missing"), "a@b.com", &tokens, 46).is_err()
        );
    }

    #[test]
    fn oauth_callback_parsing_accepts_url_and_bare_code() {
        assert_eq!(
            agy_oauth_parse_callback("http://localhost:51121/oauth-callback?code=4%2F0x&state=abc"),
            ("4/0x".to_string(), "abc".to_string())
        );
        assert_eq!(agy_oauth_parse_callback("4/0bare"), ("4/0bare".to_string(), String::new()));
        assert_eq!(agy_oauth_parse_callback("  "), (String::new(), String::new()));
    }

    #[test]
    fn oauth_sessions_are_single_use_and_bound_to_state() {
        let (session_id, state) = agy_oauth_insert_session(Some("agy-target".into())).unwrap();
        let session = agy_oauth_take_session(&session_id).unwrap();
        assert_eq!(session.state, state);
        assert_eq!(session.target_id.as_deref(), Some("agy-target"));
        // 会话取出后立即失效
        assert!(agy_oauth_take_session(&session_id).is_err());
    }

    #[tokio::test]
    async fn save_oauth_login_relogin_of_current_account_refreshes_live_token_file() {
        let app_dir = temp_app_dir("oauth-live-app");
        let home_dir = temp_app_dir("oauth-live-home");
        let token_dir = home_dir.join(".gemini/antigravity-cli");
        fs::create_dir_all(&token_dir).unwrap();
        let manager = Manager::load(app_dir, home_dir.clone());
        let body = json!([{"email": "live@example.com", "refresh_token": "1//live-refresh-tok"}]);
        manager.import_refresh_accounts(&body).unwrap();
        {
            let mut accounts = manager.accounts.write().unwrap();
            accounts[0].current = true;
        }
        let tokens = AgyOAuthTokens {
            access_token: "ya29.live".to_string(),
            refresh_token: Some("1//live-refresh-2".to_string()),
            expires_in: Some(3600),
            token_type: Some("Bearer".to_string()),
            id_token: None,
        };
        let summary = manager
            .save_oauth_login(
                Some(accounts_first_id(&manager).as_str()),
                "live@example.com",
                &tokens,
            )
            .await
            .unwrap();
        assert_eq!(summary["relogin"], true);
        assert_eq!(summary["refresh_kept"], false);
        let live: Value =
            serde_json::from_slice(&fs::read(token_dir.join("antigravity-oauth-token")).unwrap())
                .unwrap();
        assert_eq!(live["token"]["refresh_token"], "1//live-refresh-2");
        assert_eq!(live["token"]["access_token"], "ya29.live");
        assert_eq!(live["auth_method"], "consumer");
    }

    fn accounts_first_id(manager: &Manager) -> String {
        manager.accounts.read().unwrap()[0].id.clone()
    }

    #[test]
    fn quota_cache_survives_reload_and_old_accounts_remain_compatible() {
        let app_dir = temp_app_dir("reload");
        let account = Account {
            id: "agy-test".to_string(),
            name: "Test".to_string(),
            email: "test@example.com".to_string(),
            token: json!({"access_token": "old"}),
            settings: Value::Null,
            current: false,
            saved_at: 1,
            quota_cache: json!({
                "ok": true,
                "quotaSummary": {"groups": [{"displayName": "Gemini Models"}]}
            }),
            quota_cached_at: 123,
        };
        fs::write(
            app_dir.join("webclx-agy-accounts.json"),
            serde_json::to_vec(&vec![account]).unwrap(),
        )
        .unwrap();

        let manager = Manager::load(app_dir.clone(), std::env::temp_dir());
        let listed = manager.list();
        assert_eq!(listed[0]["quota_cached_at"], 123);
        let cached = manager.cached_quota("agy-test");
        assert_eq!(cached["cached"], true);
        assert_eq!(cached["quotaSummary"]["groups"][0]["displayName"], "Gemini Models");
    }

    #[test]
    fn import_refresh_accounts_creates_updates_and_skips_invalid_rows() {
        let app_dir = temp_app_dir("import");
        let existing = Account {
            id: "agy-existing".to_string(),
            name: "已有账号".to_string(),
            email: "kept@example.com".to_string(),
            token: json!({
                "auth_method": "consumer",
                "token": {
                    "access_token": "stale-access",
                    "refresh_token": "1//old-refresh-token",
                    "token_type": "Bearer"
                }
            }),
            settings: json!({"model": "gemini"}),
            current: true,
            saved_at: 1,
            quota_cache: json!({"ok": true}),
            quota_cached_at: 9,
        };
        fs::write(
            app_dir.join("webclx-agy-accounts.json"),
            serde_json::to_vec(&vec![existing]).unwrap(),
        )
        .unwrap();
        let manager = Manager::load(app_dir.clone(), std::env::temp_dir());
        let body = json!([
            {"email": "Kept@example.com", "refresh_token": "1//new-refresh-token"},
            {"email": "new.user@gmail.com", "refreshToken": "1//brand-new-refresh"},
            {"email": "bad", "refresh_token": "1//another-refresh-ok"},
            "not-an-object"
        ]);
        let result = manager.import_refresh_accounts(&body).unwrap();
        assert_eq!(result["imported"], 1);
        assert_eq!(result["updated"], 1);
        assert_eq!(result["skipped"], 2);

        let reloaded = Manager::load(app_dir, std::env::temp_dir());
        let accounts = reloaded.accounts.read().unwrap();
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].id, "agy-existing");
        assert_eq!(accounts[0].name, "已有账号");
        assert_eq!(accounts[0].email, "Kept@example.com");
        assert_eq!(accounts[0].current, true);
        assert_eq!(accounts[0].quota_cached_at, 9);
        assert_eq!(accounts[0].token["token"]["refresh_token"], "1//new-refresh-token");
        assert!(accounts[0].token["token"].get("access_token").is_none());
        assert_eq!(accounts[0].settings["model"], "gemini");
        assert_eq!(accounts[1].email, "new.user@gmail.com");
        assert_eq!(accounts[1].name, "new.user");
        assert_eq!(accounts[1].token["auth_method"], "consumer");
        assert_eq!(accounts[1].token["token"]["refresh_token"], "1//brand-new-refresh");
        assert_eq!(accounts[1].token["token"]["token_type"], "Bearer");

        let again = json!({"email": "new.user@gmail.com", "refresh_token": "1//brand-new-refresh"});
        drop(accounts);
        let second = reloaded.import_refresh_accounts(&again).unwrap();
        assert_eq!(second["imported"], 0);
        assert_eq!(second["updated"], 1);
        let accounts = reloaded.accounts.read().unwrap();
        assert_eq!(accounts.len(), 2);
        assert!(accounts[1].token["token"].get("access_token").is_none());
    }

    #[test]
    fn export_refresh_accounts_matches_antigravity_file_and_roundtrips() {
        let app_dir = temp_app_dir("export");
        let ready = Account {
            id: "agy-ready".to_string(),
            name: "Ready".to_string(),
            email: "ready.user@gmail.com".to_string(),
            token: json!({
                "auth_method": "consumer",
                "token": {
                    "access_token": "do-not-export",
                    "refresh_token": "1//ready-refresh-token",
                    "token_type": "Bearer"
                }
            }),
            settings: Value::Null,
            current: true,
            saved_at: 1,
            quota_cache: Value::Null,
            quota_cached_at: 0,
        };
        let no_refresh = Account {
            id: "agy-access-only".to_string(),
            name: "Access".to_string(),
            email: "access.only@gmail.com".to_string(),
            token: json!({"access_token": "only-access-token-value"}),
            settings: Value::Null,
            current: false,
            saved_at: 1,
            quota_cache: Value::Null,
            quota_cached_at: 0,
        };
        fs::write(
            app_dir.join("webclx-agy-accounts.json"),
            serde_json::to_vec(&vec![ready, no_refresh]).unwrap(),
        )
        .unwrap();
        let manager = Manager::load(app_dir, std::env::temp_dir());
        let exported = manager.export_refresh_accounts(&json!({})).unwrap();
        assert_eq!(exported["skipped"], 1);
        assert_eq!(exported["accounts"].as_array().unwrap().len(), 1);
        assert_eq!(exported["accounts"][0]["email"], "ready.user@gmail.com");
        assert_eq!(exported["accounts"][0]["refresh_token"], "1//ready-refresh-token");
        assert!(exported["accounts"][0].get("access_token").is_none());

        let filtered = manager
            .export_refresh_accounts(&json!({"ids": ["agy-access-only"]}))
            .unwrap_err();
        assert_eq!(filtered, "没有可导出的账号");
        let one = manager
            .export_refresh_accounts(&json!({"ids": ["agy-ready"]}))
            .unwrap();
        assert_eq!(one["accounts"][0]["email"], "ready.user@gmail.com");
        assert_eq!(one["skipped"], 0);

        let other_dir = temp_app_dir("export-import");
        let other = Manager::load(other_dir, std::env::temp_dir());
        let imported = other
            .import_refresh_accounts(&exported["accounts"])
            .unwrap();
        assert_eq!(imported["imported"], 1);
        let accounts = other.accounts.read().unwrap();
        assert_eq!(accounts[0].email, "ready.user@gmail.com");
        assert_eq!(accounts[0].token["token"]["refresh_token"], "1//ready-refresh-token");
        assert!(accounts[0].token["token"].get("access_token").is_none());
    }

    #[test]
    fn oauth_material_reads_refresh_token_without_access_token() {
        let (access, refresh) = oauth_material(&json!({
            "auth_method": "consumer",
            "token": {"refresh_token": "1//only-refresh-token", "token_type": "Bearer"}
        }));
        assert!(access.is_none());
        assert_eq!(refresh.as_deref(), Some("1//only-refresh-token"));
    }

    #[test]
    fn oauth_material_reads_flat_google_creds_format() {
        let (access, refresh) = oauth_material(&json!({
            "client_id": "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com",
            "client_secret": "GOCSPX-test",
            "token": "ya29.flat-access-token",
            "refresh_token": "1//06flat-refresh-token",
            "scopes": ["https://www.googleapis.com/auth/cloud-platform"],
            "token_uri": "https://oauth2.googleapis.com/token",
            "project_id": "test-project",
            "expiry": "2026-09-21T14:13:56.509535+00:00"
        }));
        assert_eq!(access.as_deref(), Some("ya29.flat-access-token"));
        assert_eq!(refresh.as_deref(), Some("1//06flat-refresh-token"));
    }

    #[test]
    fn import_refresh_accounts_accepts_flat_creds_item() {
        let app_dir = temp_app_dir("flat-import");
        let manager = Manager::load(app_dir, std::env::temp_dir());
        let body = json!([
            {"email": "flat.creds@gmail.com", "refresh_token": "1//flat-creds-refresh", "access_token": "ya29.do-not-keep"}
        ]);
        let result = manager.import_refresh_accounts(&body).unwrap();
        assert_eq!(result["imported"], 1);
        let accounts = manager.accounts.read().unwrap();
        assert_eq!(accounts[0].email, "flat.creds@gmail.com");
        assert_eq!(accounts[0].token["token"]["refresh_token"], "1//flat-creds-refresh");
        assert!(accounts[0].token["token"].get("access_token").is_none());
    }

    #[test]
    fn delete_and_test_structure_work() {
        let app_dir = temp_app_dir("delete-test");
        let manager = Manager::load(app_dir, std::env::temp_dir());
        let body = json!([
            {"email": "user1@example.com", "refresh_token": "1//user1-refresh-token"},
            {"email": "user2@example.com", "refresh_token": "1//user2-refresh-token"}
        ]);
        manager.import_refresh_accounts(&body).unwrap();
        let accounts = manager.accounts.read().unwrap();
        let id1 = accounts[0].id.clone();
        let id2 = accounts[1].id.clone();
        drop(accounts);

        let test_res = manager.test_structure(&id1).unwrap();
        assert_eq!(test_res["ok"], true);

        manager.delete(&id1).unwrap();
        let accounts = manager.accounts.read().unwrap();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, id2);
    }

    #[test]
    fn list_auto_detects_current_when_token_file_exists() {
        let app_dir = temp_app_dir("list-detect-app");
        let home_dir = temp_app_dir("list-detect-home");
        let token_dir = home_dir.join(".gemini/antigravity-cli");
        fs::create_dir_all(&token_dir).unwrap();

        let manager = Manager::load(app_dir, home_dir);
        let body = json!([
            {"email": "first@example.com", "refresh_token": "1//first-refresh-token"},
            {"email": "second@example.com", "refresh_token": "1//second-refresh-token"}
        ]);
        manager.import_refresh_accounts(&body).unwrap();

        // Write token file for second user
        fs::write(
            token_dir.join("antigravity-oauth-token"),
            serde_json::to_vec(&json!({
                "token": {
                    "refresh_token": "1//second-refresh-token",
                    "token_type": "Bearer"
                },
                "auth_method": "consumer"
            }))
            .unwrap(),
        )
        .unwrap();

        let list = manager.list();
        let first = list
            .iter()
            .find(|x| x["email"] == "first@example.com")
            .unwrap();
        let second = list
            .iter()
            .find(|x| x["email"] == "second@example.com")
            .unwrap();
        assert_eq!(first["current"], false);
        assert_eq!(second["current"], true);
    }

    #[test]
    fn persist_allows_concurrent_writers() {
        let app_dir = temp_app_dir("persist-concurrent");
        let manager = Manager::load(app_dir.clone(), std::env::temp_dir());
        let account = Account {
            id: "agy-concurrent".to_string(),
            name: "Concurrent".to_string(),
            email: "concurrent@example.com".to_string(),
            token: json!({"access_token": "tok"}),
            settings: Value::Null,
            current: false,
            saved_at: 1,
            quota_cache: Value::Null,
            quota_cached_at: 0,
        };
        let mut handles = Vec::new();
        for _ in 0..16 {
            let manager = manager.clone();
            let account = account.clone();
            handles.push(std::thread::spawn(move || manager.persist(&[account])));
        }
        for handle in handles {
            assert!(handle.join().unwrap().is_ok());
        }
        assert!(app_dir.join("webclx-agy-accounts.json").exists());
        let leftovers: Vec<_> = fs::read_dir(&app_dir)
            .unwrap()
            .flatten()
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp."))
            .collect();
        assert!(leftovers.is_empty());
    }

    #[test]
    fn store_access_token_computes_rfc3339_expiry() {
        let mut account = Account::default();
        let body = json!({
            "access_token": "fresh-token-123",
            "expires_in": 3600,
            "token_type": "Bearer"
        });
        store_access_token(&mut account, "fresh-token-123", &body);
        assert_eq!(account.token["token"]["access_token"], "fresh-token-123");
        assert_eq!(account.token["token"]["expires_in"], 3600);
        let expiry = account.token["token"]["expiry"].as_str().unwrap();
        assert!(expiry.contains('T'));
        assert!(!is_token_expired_or_expiring(&account.token));
    }

    #[test]
    fn is_token_expired_or_expiring_detects_expired_and_valid() {
        // No access token
        assert!(is_token_expired_or_expiring(&json!({"token": {}})));

        // Missing expiry
        assert!(is_token_expired_or_expiring(&json!({
            "token": {"access_token": "tok"}
        })));

        // Past expiry
        assert!(is_token_expired_or_expiring(&json!({
            "token": {
                "access_token": "tok",
                "expiry": "2020-01-01T00:00:00Z"
            }
        })));

        // Future expiry (1 hour ahead)
        let future = (time::OffsetDateTime::now_utc() + time::Duration::seconds(3600))
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap();
        assert!(!is_token_expired_or_expiring(&json!({
            "token": {
                "access_token": "tok",
                "expiry": future
            }
        })));
    }
}
