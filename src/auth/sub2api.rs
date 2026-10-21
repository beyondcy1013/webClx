use std::collections::HashMap;
use std::time::Duration;

use anyhow::{Context, Result};
use auth_core::{AuthFile, AuthTokens, extract_account_id_from_auth};
use serde::{Deserialize, Serialize};

const SUB2API_TIMEOUT_SECS: u64 = 15;

pub(crate) struct Sub2ApiEnvelope<T> {
    pub code: serde_json::Value,
    pub message: Option<String>,
    pub data: Option<T>,
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Sub2ApiEnvelope<T> {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawEnvelope {
            #[serde(default)]
            code: serde_json::Value,
            #[serde(default)]
            message: Option<String>,
            #[serde(default)]
            data: Option<serde_json::Value>,
        }

        let raw = RawEnvelope::deserialize(deserializer)?;
        let data = match raw.data {
            Some(value) => T::deserialize(value).ok(),
            None => None,
        };
        Ok(Sub2ApiEnvelope {
            code: raw.code,
            message: raw.message,
            data,
        })
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct Sub2ApiLoginResponse {
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub requires_2fa: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Sub2ApiTotpResponse {
    #[serde(default)]
    pub access_token: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Sub2ApiAccountPage {
    #[serde(default)]
    pub items: Vec<Sub2ApiAccount>,
    #[serde(default)]
    pub pages: u64,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Sub2ApiAccount {
    #[serde(default)]
    pub id: Option<i64>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub platform: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub credentials: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Sub2ApiWindowStats {
    #[serde(default)]
    pub requests: Option<i64>,
    #[serde(default)]
    pub tokens: Option<i64>,
    #[serde(default)]
    pub cost: Option<f64>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Sub2ApiUsageProgress {
    #[serde(default)]
    pub utilization: Option<f64>,
    #[serde(default)]
    pub resets_at: Option<String>,
    #[serde(default)]
    pub remaining_seconds: Option<i64>,
    #[serde(default)]
    pub window_stats: Option<Sub2ApiWindowStats>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Sub2ApiUsageInfo {
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub five_hour: Option<Sub2ApiUsageProgress>,
    #[serde(default)]
    pub seven_day: Option<Sub2ApiUsageProgress>,
    #[serde(default)]
    pub seven_day_sonnet: Option<Sub2ApiUsageProgress>,
    #[serde(default)]
    pub seven_day_fable: Option<Sub2ApiUsageProgress>,
    #[serde(default)]
    pub error: Option<String>,
}

/// `POST /admin/accounts/usage/batch` 的响应负载：按账号 id（JSON 对象键
/// 恒为字符串）返回用量或错误。
#[derive(Debug, Deserialize)]
pub(crate) struct Sub2ApiBatchUsagePayload {
    #[serde(default)]
    pub usage: HashMap<String, Sub2ApiUsageInfo>,
    #[serde(default)]
    pub errors: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Sub2ApiDataPayload {
    #[serde(default)]
    pub accounts: Vec<Sub2ApiDataAccount>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Sub2ApiDataAccount {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub platform: String,
    #[serde(rename = "type", default)]
    pub kind: String,
    #[serde(default)]
    pub credentials: HashMap<String, serde_json::Value>,
}

impl Sub2ApiAccount {
    fn credential(&self, key: &str) -> Option<&str> {
        self.credentials
            .as_ref()
            .and_then(|map| map.get(key))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }

    fn is_openai(&self) -> bool {
        self.platform
            .as_deref()
            .map(|p| {
                p.eq_ignore_ascii_case("openai")
                    || p.eq_ignore_ascii_case("chatgpt")
                    || p.eq_ignore_ascii_case("codex")
            })
            .unwrap_or(false)
    }

    fn is_anthropic(&self) -> bool {
        self.platform
            .as_deref()
            .map(|p| p.eq_ignore_ascii_case("anthropic") || p.eq_ignore_ascii_case("claude"))
            .unwrap_or(false)
    }

    fn is_oauth(&self) -> bool {
        self.kind
            .as_deref()
            .map(|k| {
                k.eq_ignore_ascii_case("oauth")
                    || k.eq_ignore_ascii_case("setup-token")
                    || k.eq_ignore_ascii_case("setuptoken")
            })
            .unwrap_or(false)
            || self.access_token().is_some()
    }

    fn is_api_key(&self) -> bool {
        self.kind
            .as_deref()
            .map(|k| {
                k.eq_ignore_ascii_case("apikey")
                    || k.eq_ignore_ascii_case("api_key")
                    || k.eq_ignore_ascii_case("token")
            })
            .unwrap_or(false)
            || (self.access_token().is_none() && self.api_key().is_some())
    }

    fn access_token(&self) -> Option<&str> {
        self.credential("access_token")
            .or_else(|| self.credential("accessToken"))
            .or_else(|| self.credential("token"))
    }

    fn account_id(&self) -> Option<&str> {
        self.credential("account_id")
            .or_else(|| self.credential("chatgpt_account_id"))
            .or_else(|| self.credential("accountId"))
    }

    fn api_key(&self) -> Option<&str> {
        self.credential("api_key")
            .or_else(|| self.credential("apiKey"))
            .or_else(|| self.credential("key"))
    }

    fn id_token(&self) -> Option<&str> {
        self.credential("id_token")
            .or_else(|| self.credential("idToken"))
    }

    fn refresh_token(&self) -> Option<&str> {
        self.credential("refresh_token")
            .or_else(|| self.credential("refreshToken"))
    }

    fn email(&self) -> Option<&str> {
        self.credential("email")
            .or_else(|| self.credential("user_email"))
    }

    fn plan_type(&self) -> Option<&str> {
        self.credential("plan_type")
            .or_else(|| self.credential("planType"))
            .or_else(|| self.credential("plan"))
    }

    pub(crate) fn base_url(&self) -> Option<&str> {
        self.credential("base_url")
            .or_else(|| self.credential("baseUrl"))
    }
}

#[derive(Debug, Clone)]
pub(crate) struct NormalizedOauthPreset {
    pub name: String,
    pub access_token: String,
    pub account_id: String,
    pub id_token: String,
    pub refresh_token: String,
    pub last_refresh: Option<String>,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub config_overrides: Vec<auth_core::PresetConfigOverride>,
}

#[derive(Debug, Clone)]
pub(crate) struct NormalizedApiPreset {
    pub name: String,
    pub base_url: String,
    pub provider_name: String,
    pub api_key: String,
    pub wire_api: Option<String>,
    pub config_overrides: Vec<auth_core::PresetConfigOverride>,
}

#[derive(Debug, Clone)]
pub(crate) struct NormalizedClaudePreset {
    pub name: String,
    pub base_url: String,
    pub provider_name: String,
    pub auth_token: String,
}

#[derive(Debug, Default)]
pub(crate) struct NormalizedAccounts {
    pub oauth: Vec<NormalizedOauthPreset>,
    pub api: Vec<NormalizedApiPreset>,
    pub claude: Vec<NormalizedClaudePreset>,
}

fn extract_auth_fields(
    creds: Option<&HashMap<String, serde_json::Value>>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
) {
    let Some(creds) = creds else {
        return (None, None, None, None, None, None, None, None);
    };

    let mut auth_sub = None;
    for key in &["auth.json", "auth_json", "auth"] {
        if let Some(val) = creds.get(*key) {
            if let Some(s) = val.as_str() {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(s) {
                    auth_sub = Some(parsed);
                    break;
                }
            } else if val.is_object() {
                auth_sub = Some(val.clone());
                break;
            }
        }
    }

    let tokens_sub = creds.get("tokens").and_then(serde_json::Value::as_object);
    let auth_obj = auth_sub.as_ref().and_then(serde_json::Value::as_object);
    let auth_tokens = auth_obj
        .and_then(|a| a.get("tokens"))
        .and_then(serde_json::Value::as_object);

    let get_first_str = |keys: &[&str]| -> Option<String> {
        for key in keys {
            if let Some(val) = creds
                .get(*key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                return Some(val.to_string());
            }
            if let Some(val) = tokens_sub
                .and_then(|t| t.get(*key))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                return Some(val.to_string());
            }
            if let Some(val) = auth_tokens
                .and_then(|t| t.get(*key))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                return Some(val.to_string());
            }
            if let Some(val) = auth_obj
                .and_then(|a| a.get(*key))
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                return Some(val.to_string());
            }
        }
        None
    };

    let access_token = get_first_str(&["access_token", "accessToken", "token"]);
    let account_id = get_first_str(&[
        "account_id",
        "chatgpt_account_id",
        "chatgptAccountId",
        "accountId",
    ]);
    let id_token = get_first_str(&["id_token", "idToken"]);
    let refresh_token = get_first_str(&["refresh_token", "refreshToken"]);
    let api_key = get_first_str(&[
        "api_key",
        "apiKey",
        "key",
        "OPENAI_API_KEY",
        "openai_api_key",
    ]);
    let email = get_first_str(&["email", "user_email", "userEmail"]);
    let plan_type = get_first_str(&["plan_type", "planType", "plan"]);
    let last_refresh =
        get_first_str(&["last_refresh", "lastRefresh", "refresh_time", "refreshTime"]);

    (
        access_token,
        account_id,
        id_token,
        refresh_token,
        api_key,
        email,
        plan_type,
        last_refresh,
    )
}

fn extract_config_fields(
    creds: Option<&HashMap<String, serde_json::Value>>,
) -> (
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Vec<auth_core::PresetConfigOverride>,
) {
    let mut model = None;
    let mut provider_name = None;
    let mut base_url = None;
    let mut wire_api = None;
    let mut overrides = Vec::new();

    let Some(creds) = creds else {
        return (model, provider_name, base_url, wire_api, overrides);
    };

    if let Some(m) = creds
        .get("model")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        model = Some(m.to_string());
    }
    if let Some(p) = creds
        .get("model_provider")
        .or_else(|| creds.get("provider"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        provider_name = Some(p.to_string());
    }
    if let Some(b) = creds
        .get("base_url")
        .or_else(|| creds.get("baseUrl"))
        .or_else(|| creds.get("url"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        base_url = Some(b.to_string());
    }
    if let Some(w) = creds
        .get("wire_api")
        .or_else(|| creds.get("wireApi"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        wire_api = Some(w.to_string());
    }
    if let Some(e) = creds
        .get("model_reasoning_effort")
        .or_else(|| creds.get("reasoning_effort"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        overrides.push(auth_core::PresetConfigOverride {
            key: Some("model_reasoning_effort".to_string()),
            value: Some(e.to_string()),
        });
    }

    let raw_config = creds
        .get("config.toml")
        .or_else(|| creds.get("config_toml"))
        .or_else(|| creds.get("config"));

    if let Some(cfg_val) = raw_config {
        if let Some(s) = cfg_val.as_str() {
            if let Ok(doc) = s.parse::<toml_edit::DocumentMut>() {
                if model.is_none() {
                    if let Some(m) = doc
                        .get("model")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        model = Some(m.to_string());
                    }
                }
                if provider_name.is_none() {
                    if let Some(p) = doc
                        .get("model_provider")
                        .or_else(|| doc.get("provider"))
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        provider_name = Some(p.to_string());
                    }
                }
                if let Some(effort) = doc
                    .get("model_reasoning_effort")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    if !overrides
                        .iter()
                        .any(|o| o.key.as_deref() == Some("model_reasoning_effort"))
                    {
                        overrides.push(auth_core::PresetConfigOverride {
                            key: Some("model_reasoning_effort".to_string()),
                            value: Some(effort.to_string()),
                        });
                    }
                }
                if let Some(providers) = doc.get("model_providers").and_then(|v| v.as_table_like())
                {
                    let target_id = provider_name.as_deref().unwrap_or("webclx_api");
                    let p_table = providers
                        .get(target_id)
                        .or_else(|| providers.iter().next().map(|(_, item)| item))
                        .and_then(|item| item.as_table_like());
                    if let Some(tbl) = p_table {
                        if base_url.is_none() {
                            if let Some(b) = tbl
                                .get("base_url")
                                .and_then(|v| v.as_str())
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                            {
                                base_url = Some(b.to_string());
                            }
                        }
                        if wire_api.is_none() {
                            if let Some(w) = tbl
                                .get("wire_api")
                                .and_then(|v| v.as_str())
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                            {
                                wire_api = Some(w.to_string());
                            }
                        }
                    }
                }
            } else if let Ok(parsed_json) = serde_json::from_str::<serde_json::Value>(s) {
                if let Some(obj) = parsed_json.as_object() {
                    if model.is_none() {
                        if let Some(m) = obj.get("model").and_then(serde_json::Value::as_str) {
                            model = Some(m.to_string());
                        }
                    }
                    if base_url.is_none() {
                        if let Some(b) = obj
                            .get("base_url")
                            .or_else(|| obj.get("baseUrl"))
                            .and_then(serde_json::Value::as_str)
                        {
                            base_url = Some(b.to_string());
                        }
                    }
                }
            }
        } else if let Some(obj) = cfg_val.as_object() {
            if model.is_none() {
                if let Some(m) = obj.get("model").and_then(serde_json::Value::as_str) {
                    model = Some(m.to_string());
                }
            }
            if base_url.is_none() {
                if let Some(b) = obj
                    .get("base_url")
                    .or_else(|| obj.get("baseUrl"))
                    .and_then(serde_json::Value::as_str)
                {
                    base_url = Some(b.to_string());
                }
            }
        }
    }

    if let Some(m) = model.clone() {
        if !overrides.iter().any(|o| o.key.as_deref() == Some("model")) {
            overrides.push(auth_core::PresetConfigOverride {
                key: Some("model".to_string()),
                value: Some(m),
            });
        }
    }

    (model, provider_name, base_url, wire_api, overrides)
}

pub(crate) fn normalize_accounts(accounts: &[Sub2ApiAccount]) -> NormalizedAccounts {
    let mut result = NormalizedAccounts::default();

    for account in accounts {
        let name = account
            .name
            .clone()
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| format!("sub2api-{}", account.id.unwrap_or(0)));

        let (
            access_token,
            account_id,
            id_token,
            refresh_token,
            api_key,
            email,
            plan_type,
            last_refresh,
        ) = extract_auth_fields(account.credentials.as_ref());
        let (
            _model,
            provider_from_config,
            base_url_from_config,
            wire_api_from_config,
            config_overrides,
        ) = extract_config_fields(account.credentials.as_ref());

        if account.is_anthropic() {
            let base_url = base_url_from_config
                .or_else(|| account.base_url().map(str::to_string))
                .unwrap_or_else(|| "https://api.anthropic.com".to_string())
                .trim_end_matches('/')
                .to_string();
            let auth_token = if account.is_oauth() {
                access_token.clone()
            } else {
                api_key.clone().or(access_token.clone())
            };
            if let Some(token) = auth_token {
                result.claude.push(NormalizedClaudePreset {
                    name,
                    base_url,
                    provider_name: provider_from_config.unwrap_or_else(|| "Anthropic".to_string()),
                    auth_token: token,
                });
            }
        } else if account.is_oauth() || access_token.is_some() {
            if let Some(access_token) = access_token {
                let resolved_account_id = account_id
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        let temp_auth = AuthFile {
                            openai_api_key: None,
                            last_refresh: String::new(),
                            tokens: AuthTokens {
                                access_token: access_token.clone(),
                                account_id: String::new(),
                                id_token: String::new(),
                                refresh_token: String::new(),
                            },
                        };
                        extract_account_id_from_auth(&temp_auth)
                    })
                    .unwrap_or_else(|| format!("sub2api-{}", account.id.unwrap_or(0)));

                result.oauth.push(NormalizedOauthPreset {
                    name,
                    access_token,
                    account_id: resolved_account_id,
                    id_token: id_token.unwrap_or_default(),
                    refresh_token: refresh_token.unwrap_or_default(),
                    last_refresh,
                    email,
                    plan_type,
                    config_overrides,
                });
            }
        } else if account.is_api_key() || api_key.is_some() {
            if let Some(api_key) = api_key {
                let base_url = base_url_from_config
                    .or_else(|| account.base_url().map(str::to_string))
                    .unwrap_or_else(|| "https://api.openai.com/v1".to_string())
                    .trim_end_matches('/')
                    .to_string();
                let provider_name = provider_from_config.unwrap_or_else(|| {
                    if base_url.contains("openai.com") {
                        "OpenAI".to_string()
                    } else {
                        account
                            .platform
                            .clone()
                            .unwrap_or_else(|| "OpenAI".to_string())
                    }
                });
                result.api.push(NormalizedApiPreset {
                    name,
                    base_url,
                    provider_name,
                    api_key,
                    wire_api: wire_api_from_config.or_else(|| Some("responses".to_string())),
                    config_overrides,
                });
            }
        }
    }

    result
}

fn build_client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .timeout(Duration::from_secs(SUB2API_TIMEOUT_SECS))
        .build()
        .context("创建 sub2api 客户端失败")
}

pub(crate) async fn login(base_url: &str, email: &str, password: &str) -> Result<String> {
    let base = base_url.trim_end_matches('/');
    let client = build_client()?;

    let payload = serde_json::json!({ "email": email, "password": password });

    let response = client
        .post(format!("{base}/api/v1/auth/login"))
        .json(&payload)
        .send()
        .await
        .context("连接 sub2api 登录接口失败")?;

    let status = response.status();
    let body: Sub2ApiEnvelope<Sub2ApiLoginResponse> =
        response.json().await.context("解析 sub2api 登录响应失败")?;

    if let Some(data) = body.data {
        if data.requires_2fa.unwrap_or(false) {
            anyhow::bail!("sub2api 需要 TOTP 验证，请使用 temp_token + totp_code 流程。");
        }
        if !data.access_token.is_empty() {
            return Ok(data.access_token);
        }
    }

    anyhow::bail!(
        "sub2api 登录失败: HTTP {} {}",
        status.as_u16(),
        body.message.unwrap_or_default()
    );
}

pub(crate) async fn login_totp(
    base_url: &str,
    temp_token: &str,
    totp_code: &str,
) -> Result<String> {
    let base = base_url.trim_end_matches('/');
    let client = build_client()?;

    let payload = serde_json::json!({ "temp_token": temp_token, "totp_code": totp_code });

    let response = client
        .post(format!("{base}/api/v1/auth/login/2fa"))
        .json(&payload)
        .send()
        .await
        .context("连接 sub2api TOTP 接口失败")?;

    let status = response.status();
    let body: Sub2ApiEnvelope<Sub2ApiTotpResponse> = response
        .json()
        .await
        .context("解析 sub2api TOTP 响应失败")?;

    if let Some(data) = body.data {
        if !data.access_token.is_empty() {
            return Ok(data.access_token);
        }
    }

    anyhow::bail!(
        "sub2api TOTP 失败: HTTP {} {}",
        status.as_u16(),
        body.message.unwrap_or_default()
    );
}

pub(crate) async fn fetch_all_accounts(base_url: &str, token: &str) -> Result<Vec<Sub2ApiAccount>> {
    let base = base_url.trim_end_matches('/');
    let client = build_client()?;

    let mut accounts = Vec::new();
    let mut page = 1_u64;
    loop {
        let url = format!("{base}/api/v1/admin/accounts?page={page}&page_size=200");
        let response = client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .with_context(|| format!("读取 sub2api 账号第 {page} 页失败"))?;

        let status = response.status();
        let body: Sub2ApiEnvelope<Sub2ApiAccountPage> = response
            .json()
            .await
            .with_context(|| format!("解析 sub2api 账号第 {page} 页失败"))?;

        if !status.is_success() {
            anyhow::bail!(
                "读取 sub2api 账号失败: HTTP {} {}",
                status.as_u16(),
                body.message.unwrap_or_default()
            );
        }

        let data = body
            .data
            .ok_or_else(|| anyhow::anyhow!("sub2api 响应缺少 data"))?;
        let page_count = data.pages.max(1);
        let page_empty = data.items.is_empty();
        accounts.extend(data.items);
        if page >= page_count || page_empty {
            break;
        }
        page += 1;
    }
    Ok(accounts)
}

pub(crate) async fn get_account_usage(
    base_url: &str,
    token: &str,
    account_id: i64,
    source: Option<&str>,
    force: bool,
) -> Result<Sub2ApiUsageInfo> {
    let base = base_url.trim_end_matches('/');
    let client = build_client()?;

    let mut url = format!("{base}/api/v1/admin/accounts/{account_id}/usage");
    let mut params = Vec::new();
    if let Some(src) = source {
        params.push(format!("source={src}"));
    }
    if force {
        params.push("force=true".to_string());
    }
    if !params.is_empty() {
        url.push('?');
        url.push_str(&params.join("&"));
    }

    let response = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("读取 sub2api 账号额度失败")?;

    let status = response.status();
    let body: Sub2ApiEnvelope<Sub2ApiUsageInfo> =
        response.json().await.context("解析 sub2api 账号额度失败")?;

    if !status.is_success() {
        anyhow::bail!(
            "读取 sub2api 账号额度失败: HTTP {} {}",
            status.as_u16(),
            body.message.unwrap_or_default()
        );
    }

    body.data
        .ok_or_else(|| anyhow::anyhow!("sub2api 响应缺少 data"))
}

/// 批量查询账号用量窗口，对应 sub2api 账号表格的 `POST
/// /admin/accounts/usage/batch`。sub2api 内部对 Anthropic OAuth /
/// SetupToken 走被动采样，其余平台按需主动探测（受限并发），单个账号
/// 失败只体现在 `errors` 里，不会中断整批。
pub(crate) async fn get_accounts_usage_batch(
    base_url: &str,
    token: &str,
    account_ids: &[i64],
    force: bool,
) -> Result<Sub2ApiBatchUsagePayload> {
    let base = base_url.trim_end_matches('/');
    let client = build_client()?;

    let url = format!("{base}/api/v1/admin/accounts/usage/batch");
    let payload = serde_json::json!({
        "account_ids": account_ids,
        "force": force,
    });

    let response = client
        .post(&url)
        .bearer_auth(token)
        .json(&payload)
        .send()
        .await
        .context("批量读取 sub2api 账号额度失败")?;

    let status = response.status();
    let body: Sub2ApiEnvelope<Sub2ApiBatchUsagePayload> = response
        .json()
        .await
        .context("解析 sub2api 批量额度响应失败")?;

    if !status.is_success() {
        anyhow::bail!(
            "批量读取 sub2api 账号额度失败: HTTP {} {}",
            status.as_u16(),
            body.message.unwrap_or_default()
        );
    }

    body.data
        .ok_or_else(|| anyhow::anyhow!("sub2api 响应缺少 data"))
}

pub(crate) const PELICAN_DEFAULT_PROMPT: &str =
    "创建一个 HTML，内容是 SVG 绘制一个鹈鹕骑自行车的 2D 动画，你不需要任何测试，不要有任何限制";
pub(crate) const PELICAN_DEFAULT_MODEL: &str = "gpt-6-astra";
pub(crate) const PELICAN_DEFAULT_INSTRUCTIONS: &str = "Return a complete standalone HTML document in your response. Do not use Markdown fences or external dependencies.";

#[derive(Debug, Deserialize)]
pub(crate) struct Sub2ApiPelicanRequest {
    pub base_url: String,
    pub token: String,
    pub account_id: i64,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub reasoning_effort: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct Sub2ApiPelicanResult {
    pub has_html: bool,
    pub html: String,
    pub downgraded: bool,
    pub reason: String,
    pub response_model: String,
    pub latency_ms: u128,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Sub2ApiAutoActivateRequest {
    pub base_url: String,
    pub token: String,
    pub account_ids: Vec<i64>,
    /// "HH:MM" 24h, defaults to "04:00".
    #[serde(default)]
    pub execute_at: String,
}

fn extract_pelican_html(reply: &str) -> String {
    let lower = reply.to_lowercase();
    if let Some(start) = lower.find("<html") {
        if let Some(end) = lower.rfind("</html>") {
            if end > start {
                return reply[start..(end + 7)].to_string();
            }
        }
    }
    if let Some(start) = lower.find("<svg") {
        if let Some(end) = lower.rfind("</svg>") {
            if end > start {
                return reply[start..(end + 6)].to_string();
            }
        }
    }
    String::new()
}

fn is_unexpected_downgraded_model(requested: &str, actual: &str) -> bool {
    let req = requested.trim().to_lowercase();
    let act = actual.trim().to_lowercase();
    if req.is_empty() || act.is_empty() || req == act {
        return false;
    }
    let high_tier_req = req.contains("5")
        || req.contains("6")
        || req.contains("o1")
        || req.contains("o3")
        || req.contains("codex")
        || req.contains("sonnet")
        || req.contains("opus");
    let low_tier_act = act.contains("mini")
        || act.contains("nano")
        || act.contains("haiku")
        || act.contains("flash-lite")
        || act.contains("3.5");
    high_tier_req && low_tier_act
}

pub(crate) fn evaluate_pelican_result(
    requested_model: &str,
    response_model: &str,
    full_reply: &str,
) -> Sub2ApiPelicanResult {
    let extracted = extract_pelican_html(full_reply);
    let has_html = !extracted.is_empty();
    let model_degraded = is_unexpected_downgraded_model(requested_model, response_model);
    let downgraded = !has_html || model_degraded;
    let reason = if model_degraded {
        format!("模型被上游降级为: {response_model}")
    } else if !has_html {
        "未生成完整的 HTML/SVG 动画（疑似降智或输出截断）".to_string()
    } else {
        "测智通过（未降智，已生成完整动画）".to_string()
    };
    Sub2ApiPelicanResult {
        has_html,
        html: extracted,
        downgraded,
        reason,
        response_model: response_model.to_string(),
        latency_ms: 0,
    }
}

fn pelican_request_body(model: &str, prompt: &str, reasoning_effort: &str) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "instructions": PELICAN_DEFAULT_INSTRUCTIONS,
        "input": [{
            "type": "message",
            "role": "user",
            "content": [{"type": "input_text", "text": prompt}]
        }],
        "reasoning": {
            "effort": if reasoning_effort.is_empty() { "low" } else { reasoning_effort },
            "summary": "auto"
        },
        "stream": true
    })
}

fn pelican_final_text_from_response(response_text: &str) -> String {
    let mut texts = Vec::new();
    for line in response_text.lines() {
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(data.trim()) else {
            continue;
        };
        if value.get("type").and_then(|v| v.as_str()) != Some("response.completed") {
            continue;
        }
        let Some(output) = value.pointer("/response/output").and_then(|v| v.as_array()) else {
            continue;
        };
        for item in output {
            if item.get("type").and_then(|v| v.as_str()) != Some("message") {
                continue;
            }
            let Some(contents) = item.get("content").and_then(|v| v.as_array()) else {
                continue;
            };
            for part in contents {
                let part_type = part.get("type").and_then(|v| v.as_str()).unwrap_or("");
                if part_type == "output_text" {
                    if let Some(text) = part.get("text").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            texts.push(text.to_string());
                        }
                    }
                } else if part_type == "refusal" {
                    if let Some(text) = part.get("refusal").and_then(|v| v.as_str()) {
                        if !text.is_empty() {
                            texts.push(text.to_string());
                        }
                    }
                }
            }
        }
    }
    texts.join("\n")
}

fn response_model_from_response(response_text: &str) -> String {
    for line in response_text.lines() {
        let Some(data) = line.trim().strip_prefix("data:") else {
            continue;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(data.trim()) else {
            continue;
        };
        if value.get("type").and_then(|v| v.as_str()) == Some("response.created") {
            if let Some(model) = value.pointer("/response/model").and_then(|v| v.as_str()) {
                return model.to_string();
            }
        }
    }
    String::new()
}

pub(crate) async fn run_pelican_test(
    base_url: &str,
    token: &str,
    account_id: i64,
    model: &str,
    prompt: &str,
    reasoning_effort: &str,
) -> Result<Sub2ApiPelicanResult> {
    let base = base_url.trim_end_matches('/');
    let client = build_client()?;
    let model = if model.trim().is_empty() {
        PELICAN_DEFAULT_MODEL
    } else {
        model.trim()
    };
    let prompt = if prompt.trim().is_empty() {
        PELICAN_DEFAULT_PROMPT
    } else {
        prompt.trim()
    };

    let url = format!("{base}/api/v1/admin/accounts/{account_id}/test");
    let body = pelican_request_body(model, prompt, reasoning_effort);

    let started = std::time::Instant::now();
    let response = client
        .post(&url)
        .bearer_auth(token)
        .json(&body)
        .send()
        .await
        .context("发送鹈鹕测智请求失败")?;

    let status = response.status();
    let response_text = response.text().await.context("读取鹈鹕测智响应失败")?;
    let latency_ms = started.elapsed().as_millis();

    if !status.is_success() {
        anyhow::bail!(
            "鹈鹕测智失败: HTTP {} {}",
            status.as_u16(),
            &response_text[..response_text.len().min(200)]
        );
    }

    let response_model = response_model_from_response(&response_text);
    let final_text = pelican_final_text_from_response(&response_text);
    let mut result = evaluate_pelican_result(model, &response_model, &final_text);
    result.latency_ms = latency_ms;
    Ok(result)
}

pub(crate) async fn recycle_account(base_url: &str, token: &str, account_id: i64) -> Result<()> {
    let base = base_url.trim_end_matches('/');
    let client = build_client()?;
    let url = format!("{base}/api/v1/admin/accounts/{account_id}/recycle");
    let response = client
        .post(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("归档账号失败")?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("归档失败: HTTP {} {}", status.as_u16(), &body[..body.len().min(150)]);
    }
    Ok(())
}

pub(crate) async fn restore_account(base_url: &str, token: &str, account_id: i64) -> Result<()> {
    let base = base_url.trim_end_matches('/');
    let client = build_client()?;
    let url = format!("{base}/api/v1/admin/accounts/{account_id}/restore");
    let response = client
        .post(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("恢复账号失败")?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        anyhow::bail!("恢复失败: HTTP {} {}", status.as_u16(), &body[..body.len().min(150)]);
    }
    Ok(())
}

pub(crate) async fn fetch_accounts_by_recycled(
    base_url: &str,
    token: &str,
    recycled: bool,
) -> Result<Vec<Sub2ApiAccount>> {
    let base = base_url.trim_end_matches('/');
    let client = build_client()?;

    let mut accounts = Vec::new();
    let mut page = 1_u64;
    loop {
        let recycled_param = if recycled { "&recycled=1" } else { "" };
        let url = format!("{base}/api/v1/admin/accounts?page={page}&page_size=200{recycled_param}");
        let response = client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .with_context(|| format!("读取 sub2api 账号第 {page} 页失败"))?;

        let status = response.status();
        let body: Sub2ApiEnvelope<Sub2ApiAccountPage> = response
            .json()
            .await
            .with_context(|| format!("解析 sub2api 账号第 {page} 页失败"))?;

        if !status.is_success() {
            anyhow::bail!(
                "读取 sub2api 账号失败: HTTP {} {}",
                status.as_u16(),
                body.message.unwrap_or_default()
            );
        }

        let data = body
            .data
            .ok_or_else(|| anyhow::anyhow!("sub2api 响应缺少 data"))?;
        let page_count = data.pages.max(1);
        let page_empty = data.items.is_empty();
        accounts.extend(data.items);
        if page >= page_count || page_empty {
            break;
        }
        page += 1;
    }
    Ok(accounts)
}

pub(crate) async fn fetch_accounts_data(
    base_url: &str,
    token: &str,
    ids: Option<&[i64]>,
) -> Result<Vec<Sub2ApiAccount>> {
    let base = base_url.trim_end_matches('/');
    let client = build_client()?;

    let query_ids = match ids {
        Some(list) if !list.is_empty() => {
            let ids_str: Vec<String> = list.iter().map(|id| id.to_string()).collect();
            format!("?ids={}&include_proxies=false", ids_str.join(","))
        }
        _ => "?include_proxies=false".to_string(),
    };

    let url = format!("{base}/api/v1/admin/accounts/data{query_ids}");
    let response = client
        .get(&url)
        .bearer_auth(token)
        .send()
        .await
        .context("连接 sub2api 备份导出接口失败")?;

    let status = response.status();
    let body: Sub2ApiEnvelope<Sub2ApiDataPayload> =
        response.json().await.context("解析 sub2api 导出数据失败")?;

    if !status.is_success() {
        anyhow::bail!(
            "读取 sub2api 导出数据失败: HTTP {} {}",
            status.as_u16(),
            body.message.unwrap_or_default()
        );
    }

    let data = body
        .data
        .ok_or_else(|| anyhow::anyhow!("sub2api 导出响应缺少 data"))?;

    let accounts = data
        .accounts
        .into_iter()
        .map(|da| Sub2ApiAccount {
            id: None,
            name: Some(da.name),
            platform: Some(da.platform),
            kind: Some(da.kind),
            credentials: Some(da.credentials),
        })
        .collect();

    Ok(accounts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sub2api_normalize_accounts() {
        let mut chatgpt_creds = HashMap::new();
        chatgpt_creds.insert("access_token".to_string(), serde_json::json!("tok_chatgpt_123"));
        chatgpt_creds.insert("account_id".to_string(), serde_json::json!("acc_openai_456"));

        let mut claude_creds = HashMap::new();
        claude_creds.insert("api_key".to_string(), serde_json::json!("sk-ant-123"));

        let mut api_creds = HashMap::new();
        api_creds.insert("api_key".to_string(), serde_json::json!("sk-proj-789"));

        let accounts = vec![
            Sub2ApiAccount {
                id: Some(1),
                name: Some("GPT Account".to_string()),
                platform: Some("chatgpt".to_string()),
                kind: Some("oauth".to_string()),
                credentials: Some(chatgpt_creds),
            },
            Sub2ApiAccount {
                id: Some(2),
                name: Some("Claude Account".to_string()),
                platform: Some("claude".to_string()),
                kind: Some("api_key".to_string()),
                credentials: Some(claude_creds),
            },
            Sub2ApiAccount {
                id: Some(3),
                name: Some("OpenAI API Account".to_string()),
                platform: Some("openai".to_string()),
                kind: Some("api_key".to_string()),
                credentials: Some(api_creds),
            },
        ];

        let normalized = normalize_accounts(&accounts);
        assert_eq!(normalized.oauth.len(), 1);
        assert_eq!(normalized.oauth[0].name, "GPT Account");
        assert_eq!(normalized.oauth[0].access_token, "tok_chatgpt_123");
        assert_eq!(normalized.oauth[0].account_id, "acc_openai_456");

        assert_eq!(normalized.claude.len(), 1);
        assert_eq!(normalized.claude[0].name, "Claude Account");
        assert_eq!(normalized.claude[0].auth_token, "sk-ant-123");

        assert_eq!(normalized.api.len(), 1);
        assert_eq!(normalized.api[0].name, "OpenAI API Account");
        assert_eq!(normalized.api[0].api_key, "sk-proj-789");
    }

    #[test]
    fn test_sub2api_native_config_and_auth_conversion() {
        // Native auth.json format with nested tokens
        let mut oauth_native = HashMap::new();
        oauth_native.insert(
            "auth.json".to_string(),
            serde_json::json!({
                "tokens": {
                    "access_token": "tok_native_oauth",
                    "account_id": "acc_native_123",
                    "id_token": "id_tok_native",
                    "refresh_token": "ref_tok_native"
                },
                "last_refresh": "2026-09-21T12:00:00Z"
            }),
        );
        oauth_native.insert("model".to_string(), serde_json::json!("gpt-5.4"));

        // Native config.toml format with model_providers
        let mut api_native = HashMap::new();
        api_native.insert("api_key".to_string(), serde_json::json!("sk-custom-api-key"));
        api_native.insert(
            "config.toml".to_string(),
            serde_json::json!(
                "model = \"gpt-5.6\"\nmodel_provider = \"webclx_api\"\n\n[model_providers.webclx_api]\nbase_url = \"https://api.example.com/v1\"\nwire_api = \"responses\"\n"
            ),
        );

        let accounts = vec![
            Sub2ApiAccount {
                id: Some(10),
                name: Some("Native OAuth".to_string()),
                platform: Some("openai".to_string()),
                kind: Some("oauth".to_string()),
                credentials: Some(oauth_native),
            },
            Sub2ApiAccount {
                id: Some(11),
                name: Some("Native API".to_string()),
                platform: Some("openai".to_string()),
                kind: Some("api_key".to_string()),
                credentials: Some(api_native),
            },
        ];

        let normalized = normalize_accounts(&accounts);
        assert_eq!(normalized.oauth.len(), 1);
        assert_eq!(normalized.oauth[0].name, "Native OAuth");
        assert_eq!(normalized.oauth[0].access_token, "tok_native_oauth");
        assert_eq!(normalized.oauth[0].account_id, "acc_native_123");
        assert_eq!(normalized.oauth[0].id_token, "id_tok_native");
        assert_eq!(normalized.oauth[0].refresh_token, "ref_tok_native");
        assert_eq!(normalized.oauth[0].last_refresh.as_deref(), Some("2026-09-21T12:00:00Z"));
        assert!(
            normalized.oauth[0]
                .config_overrides
                .iter()
                .any(|o| o.key.as_deref() == Some("model") && o.value.as_deref() == Some("gpt-5.4"))
        );

        assert_eq!(normalized.api.len(), 1);
        assert_eq!(normalized.api[0].name, "Native API");
        assert_eq!(normalized.api[0].api_key, "sk-custom-api-key");
        assert_eq!(normalized.api[0].base_url, "https://api.example.com/v1");
        assert_eq!(normalized.api[0].wire_api.as_deref(), Some("responses"));
        assert!(
            normalized.api[0]
                .config_overrides
                .iter()
                .any(|o| o.key.as_deref() == Some("model") && o.value.as_deref() == Some("gpt-5.6"))
        );
    }
}
