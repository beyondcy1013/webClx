use std::{
    collections::{BTreeMap, HashMap},
    path::PathBuf,
    sync::{Arc, RwLock},
};

use anyhow::Result;
use reqwest::StatusCode as HttpStatusCode;
use serde::{Deserialize, Deserializer, Serialize, de};
use serde_json::Value;

pub trait CodexAuthHttpClientProvider {
    fn build_auth_client(&self, timeout_secs: u64) -> Result<reqwest::Client>;

    fn active_proxy_server(&self) -> Option<String> {
        None
    }
}

#[derive(Clone)]
pub struct AuthPresetManager {
    pub auth_presets: Arc<RwLock<Vec<StoredAuthPreset>>>,
    pub api_presets: Arc<RwLock<Vec<StoredApiPreset>>>,
    pub claude_presets: Arc<RwLock<Vec<StoredClaudePreset>>>,
    pub upstream_proxy_settings: Arc<RwLock<UpstreamProxySettings>>,
    pub(crate) active_config_write_lock: Arc<tokio::sync::Mutex<()>>,
    pub preset_file: Arc<PathBuf>,
    pub api_preset_file: Arc<PathBuf>,
    pub claude_preset_file: Arc<PathBuf>,
    pub upstream_proxy_settings_file: Arc<PathBuf>,
}

#[derive(Clone)]
pub struct CodexOAuthManager {
    pub sessions: Arc<RwLock<HashMap<String, CodexOAuthSession>>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct AuthFile {
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,
    #[serde(alias = "refresh_time", alias = "refresh time", alias = "refreshTime")]
    pub last_refresh: String,
    pub tokens: AuthTokens,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthTokens {
    pub access_token: String,
    pub account_id: String,
    pub id_token: String,
    pub refresh_token: String,
}

impl<'de> Deserialize<'de> for AuthFile {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        parse_auth_file_value(&value).map_err(de::Error::custom)
    }
}

fn parse_auth_file_value(value: &Value) -> std::result::Result<AuthFile, String> {
    let source = value
        .as_object()
        .ok_or_else(|| "auth 数据必须是对象。".to_string())?;
    let token_source = source
        .get("tokens")
        .and_then(Value::as_object)
        .unwrap_or(source);

    let access_token = required_json_string(token_source, "access_token")?;
    let id_token = optional_json_string(token_source, "id_token")?;
    let refresh_token = optional_json_string(token_source, "refresh_token")?;
    let account_id = json_string_field(token_source, "account_id")
        .or_else(|| json_string_field(source, "account_id"))
        .ok_or_else(|| "内容缺少 account_id。".to_string())?;
    let last_refresh = json_string_field(source, "last_refresh")
        .or_else(|| json_string_field(source, "refresh_time"))
        .or_else(|| json_string_field(source, "refresh time"))
        .or_else(|| json_string_field(source, "refreshTime"))
        .ok_or_else(|| "内容缺少 last_refresh。".to_string())?;
    let openai_api_key = match source.get("OPENAI_API_KEY") {
        Some(Value::Null) | None => None,
        Some(Value::String(value)) => Some(value.clone()),
        Some(_) => return Err("OPENAI_API_KEY 必须是字符串或 null。".to_string()),
    };

    Ok(AuthFile {
        openai_api_key,
        last_refresh,
        tokens: AuthTokens {
            access_token,
            account_id,
            id_token,
            refresh_token,
        },
    })
}

fn required_json_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> std::result::Result<String, String> {
    json_string_field(object, key).ok_or_else(|| format!("内容缺少 {key}。"))
}

fn optional_json_string(
    object: &serde_json::Map<String, Value>,
    key: &str,
) -> std::result::Result<String, String> {
    match object.get(key) {
        None | Some(Value::Null) => Ok(String::new()),
        Some(Value::String(value)) => Ok(value.trim().to_string()),
        Some(_) => Err(format!("{key} 必须是字符串。")),
    }
}

fn json_string_field(object: &serde_json::Map<String, Value>, key: &str) -> Option<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApiAuthFile {
    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct UpstreamProxySettings {
    pub codex_api_proxy_enabled: bool,
    pub claude_proxy_enabled: bool,
    pub active_api_proxy_preset_id: Option<String>,
    pub active_claude_proxy_preset_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub(crate) enum AuthFileContent {
    Login(AuthFile),
    Api(ApiAuthFile),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PresetConfigOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PresetTerminalEnvVar {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAuthPreset {
    pub id: String,
    pub name: String,
    pub saved_at: u64,
    #[serde(default)]
    pub details: AuthPresetDetails,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_overrides: Vec<PresetConfigOverride>,
    #[serde(default, rename = "config_key", skip_serializing)]
    pub legacy_config_key: Option<String>,
    #[serde(default, rename = "config_value", skip_serializing)]
    pub legacy_config_value: Option<String>,
    #[serde(default, rename = "secondary_config_key", skip_serializing)]
    pub legacy_secondary_config_key: Option<String>,
    #[serde(default, rename = "secondary_config_value", skip_serializing)]
    pub legacy_secondary_config_value: Option<String>,
    pub auth: AuthFile,
    #[serde(default)]
    pub switch_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredApiPreset {
    pub id: String,
    pub name: String,
    pub saved_at: u64,
    #[serde(default)]
    pub provider_name: String,
    pub base_url: String,
    #[serde(default)]
    pub management_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wire_api: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responses_proxy: Option<ApiResponsesProxyMode>,
    #[serde(default)]
    pub apply_upstream_proxy_on_switch: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_overrides: Vec<PresetConfigOverride>,
    #[serde(default, rename = "config_key", skip_serializing)]
    pub legacy_config_key: Option<String>,
    #[serde(default, rename = "config_value", skip_serializing)]
    pub legacy_config_value: Option<String>,
    #[serde(default, rename = "secondary_config_key", skip_serializing)]
    pub legacy_secondary_config_key: Option<String>,
    #[serde(default, rename = "secondary_config_value", skip_serializing)]
    pub legacy_secondary_config_value: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terminal_env: Vec<PresetTerminalEnvVar>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_startup_script: Option<String>,
    pub api_key: String,
    /// OAuth access token for ChatGPT backend proxy mode.
    /// When present, the proxy uses this instead of api_key and routes to
    /// https://chatgpt.com/backend-api/codex/responses.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub access_token: String,
    /// ChatGPT account ID injected as ChatGPT-Account-Id header.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub account_id: String,
    /// Access mode: direct (api_key) or chatgpt_oauth (access_token).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_mode: Option<ApiAccessMode>,
    #[serde(default)]
    pub switch_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredClaudePreset {
    pub id: String,
    pub name: String,
    pub saved_at: u64,
    #[serde(default)]
    pub provider_name: String,
    pub base_url: String,
    #[serde(default)]
    pub management_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_overrides: Vec<PresetConfigOverride>,
    #[serde(default, rename = "config_key", skip_serializing)]
    pub legacy_config_key: Option<String>,
    #[serde(default, rename = "config_value", skip_serializing)]
    pub legacy_config_value: Option<String>,
    #[serde(default, rename = "secondary_config_key", skip_serializing)]
    pub legacy_secondary_config_key: Option<String>,
    #[serde(default, rename = "secondary_config_value", skip_serializing)]
    pub legacy_secondary_config_value: Option<String>,
    pub auth_token: String,
    #[serde(default, alias = "small_fast_model")]
    pub default_haiku_model: Option<String>,
    #[serde(default)]
    pub default_sonnet_model: Option<String>,
    #[serde(default)]
    pub default_opus_model: Option<String>,
    #[serde(default)]
    pub third_party_model: Option<String>,
    #[serde(default)]
    pub use_local_proxy: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub access_mode: Option<ClaudeAccessMode>,
    #[serde(default)]
    pub switch_count: u64,
}

/// Abstraction over preset types that have an `id` and `switch_count`.
/// Enables a single generic `bump_switch_count` implementation for all three
/// preset kinds (auth, api, claude).
pub trait SwitchCounted {
    fn preset_id(&self) -> &str;
    fn switch_count_mut(&mut self) -> &mut u64;
}

impl SwitchCounted for StoredAuthPreset {
    fn preset_id(&self) -> &str {
        &self.id
    }
    fn switch_count_mut(&mut self) -> &mut u64 {
        &mut self.switch_count
    }
}

impl SwitchCounted for StoredApiPreset {
    fn preset_id(&self) -> &str {
        &self.id
    }
    fn switch_count_mut(&mut self) -> &mut u64 {
        &mut self.switch_count
    }
}

impl SwitchCounted for StoredClaudePreset {
    fn preset_id(&self) -> &str {
        &self.id
    }
    fn switch_count_mut(&mut self) -> &mut u64 {
        &mut self.switch_count
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AuthPresetDetails {
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub account_name: Option<String>,
    pub login_method: Option<String>,
    pub hourly_percentage: Option<u64>,
    pub hourly_reset_time: Option<u64>,
    pub weekly_percentage: Option<u64>,
    pub weekly_reset_time: Option<u64>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CurrentAuthMode {
    None,
    Auth,
    Api,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiResponsesProxyMode {
    Direct,
    OpenaiChat,
    MinimaxChat,
    DeepseekChat,
    AnthropicChat,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeAccessMode {
    Direct,
    AnthropicProxy,
    AnthropicRelay,
    OpenaiChat,
    OpenaiResponses,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApiAccessMode {
    /// Standard API key → upstream provider (default).
    Direct,
    /// OAuth access token → ChatGPT backend (codex/responses).
    ChatgptOauth,
}

#[derive(Debug, Serialize)]
pub struct AuthPresetListResponse {
    pub auth_file: String,
    pub config_file: String,
    pub preset_file: String,
    pub current_mode: CurrentAuthMode,
    pub current_auth: Option<CurrentAuthSummary>,
    pub current_api: Option<CurrentApiSummary>,
    pub current_auth_error: Option<String>,
    pub current_config_error: Option<String>,
    pub upstream_proxy: UpstreamProxySettings,
    pub presets: Vec<AuthPresetSummary>,
}

#[derive(Debug, Serialize)]
pub struct ApiPresetListResponse {
    pub auth_file: String,
    pub config_file: String,
    pub preset_file: String,
    pub current_mode: CurrentAuthMode,
    pub current_auth: Option<CurrentAuthSummary>,
    pub current_api: Option<CurrentApiSummary>,
    pub current_auth_error: Option<String>,
    pub current_config_error: Option<String>,
    pub upstream_proxy: UpstreamProxySettings,
    pub presets: Vec<ApiPresetSummary>,
}

#[derive(Debug, Serialize)]
pub struct ClaudePresetListResponse {
    pub settings_file: String,
    pub preset_file: String,
    pub current_claude: Option<CurrentClaudeSummary>,
    pub current_settings_error: Option<String>,
    pub upstream_proxy: UpstreamProxySettings,
    pub presets: Vec<ClaudePresetSummary>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateUpstreamProxySettingsRequest {
    #[serde(default)]
    pub codex_api_proxy_enabled: Option<bool>,
    #[serde(default)]
    pub claude_proxy_enabled: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct UpstreamProxySettingsResponse {
    pub ok: bool,
    pub upstream_proxy: UpstreamProxySettings,
}

#[derive(Debug, Serialize)]
pub struct CurrentAuthSummary {
    pub account_id: String,
    pub short_id: String,
    pub email: Option<String>,
    pub plan_type: Option<String>,
    pub last_refresh: String,
}

#[derive(Debug, Serialize)]
pub struct CurrentApiSummary {
    pub provider_id: Option<String>,
    pub provider_name: Option<String>,
    pub base_url: Option<String>,
    pub wire_api: Option<String>,
    pub masked_api_key: Option<String>,
    pub preset_name: Option<String>,
    pub management_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CurrentClaudeSummary {
    pub provider_name: Option<String>,
    pub base_url: Option<String>,
    pub management_url: Option<String>,
    pub default_haiku_model: Option<String>,
    pub default_sonnet_model: Option<String>,
    pub default_opus_model: Option<String>,
    pub third_party_model: Option<String>,
    pub masked_auth_token: Option<String>,
    pub preset_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AuthPresetSummary {
    pub id: String,
    pub name: String,
    pub account_id: String,
    pub last_refresh: String,
    pub saved_at: u64,
    pub active: bool,
    pub details: AuthPresetDetails,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_overrides: Vec<PresetConfigOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_config_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_config_value: Option<String>,
    pub auth: AuthFile,
    pub switch_count: u64,
}

#[derive(Debug, Serialize)]
pub struct ApiPresetSummary {
    pub id: String,
    pub name: String,
    pub provider_name: String,
    pub base_url: String,
    pub management_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wire_api: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responses_proxy: Option<ApiResponsesProxyMode>,
    pub apply_upstream_proxy_on_switch: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terminal_env: Vec<PresetTerminalEnvVar>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_startup_script: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_overrides: Vec<PresetConfigOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_config_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_config_value: Option<String>,
    pub api_key: String,
    pub masked_api_key: String,
    pub access_mode: ApiAccessMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub masked_access_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
    pub saved_at: u64,
    pub active: bool,
    pub switch_count: u64,
}

#[derive(Debug, Serialize)]
pub struct ClaudePresetSummary {
    pub id: String,
    pub name: String,
    pub provider_name: String,
    pub base_url: String,
    pub management_url: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub config_overrides: Vec<PresetConfigOverride>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_config_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_config_value: Option<String>,
    pub auth_token: String,
    pub masked_auth_token: String,
    pub default_haiku_model: Option<String>,
    pub default_sonnet_model: Option<String>,
    pub default_opus_model: Option<String>,
    pub third_party_model: Option<String>,
    pub use_local_proxy: bool,
    pub access_mode: ClaudeAccessMode,
    pub saved_at: u64,
    pub active: bool,
    pub switch_count: u64,
}

#[derive(Debug, Deserialize)]
pub struct SaveAuthPresetRequest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub details: AuthPresetDetails,
    #[serde(default)]
    pub config_overrides: Vec<PresetConfigOverride>,
    #[serde(default)]
    pub config_key: Option<String>,
    #[serde(default)]
    pub config_value: Option<String>,
    #[serde(default)]
    pub secondary_config_key: Option<String>,
    #[serde(default)]
    pub secondary_config_value: Option<String>,
    pub auth: AuthFile,
}

#[derive(Debug, Deserialize)]
pub struct SaveApiPresetRequest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub provider_name: String,
    pub api_key: String,
    #[serde(default)]
    pub access_mode: Option<ApiAccessMode>,
    pub base_url: String,
    #[serde(default)]
    pub management_url: Option<String>,
    #[serde(default)]
    pub management_url_same_as_base: bool,
    #[serde(default)]
    pub wire_api: Option<String>,
    #[serde(default)]
    pub responses_proxy: Option<ApiResponsesProxyMode>,
    #[serde(default)]
    pub apply_upstream_proxy_on_switch: bool,
    #[serde(default)]
    pub terminal_env: Vec<PresetTerminalEnvVar>,
    #[serde(default)]
    pub terminal_startup_script: Option<String>,
    #[serde(default)]
    pub config_overrides: Vec<PresetConfigOverride>,
    #[serde(default)]
    pub config_key: Option<String>,
    #[serde(default)]
    pub config_value: Option<String>,
    #[serde(default)]
    pub secondary_config_key: Option<String>,
    #[serde(default)]
    pub secondary_config_value: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaveClaudePresetRequest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub provider_name: String,
    pub auth_token: String,
    pub base_url: String,
    #[serde(default)]
    pub management_url: Option<String>,
    #[serde(default)]
    pub config_overrides: Vec<PresetConfigOverride>,
    #[serde(default)]
    pub config_key: Option<String>,
    #[serde(default)]
    pub config_value: Option<String>,
    #[serde(default)]
    pub secondary_config_key: Option<String>,
    #[serde(default)]
    pub secondary_config_value: Option<String>,
    #[serde(default, alias = "small_fast_model")]
    pub default_haiku_model: Option<String>,
    #[serde(default)]
    pub default_sonnet_model: Option<String>,
    #[serde(default)]
    pub default_opus_model: Option<String>,
    #[serde(default)]
    pub third_party_model: Option<String>,
    #[serde(default)]
    pub use_local_proxy: bool,
    #[serde(default)]
    pub access_mode: Option<ClaudeAccessMode>,
}

#[derive(Debug, Serialize)]
pub struct SaveAuthPresetResponse {
    pub ok: bool,
    pub preset: AuthPresetSummary,
}

#[derive(Debug, Serialize)]
pub struct RefreshAllAuthPresetsResponse {
    pub ok: bool,
    pub total: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub failures: Vec<RefreshAuthPresetFailure>,
}

#[derive(Debug, Serialize)]
pub struct RefreshAuthPresetFailure {
    pub preset_id: String,
    pub name: String,
    pub error: String,
}

#[derive(Debug, Serialize)]
pub struct SaveApiPresetResponse {
    pub ok: bool,
    pub preset: ApiPresetSummary,
}

#[derive(Debug, Deserialize)]
pub struct ImportApiAccountsRequest {
    /// Raw JSON text: sub2api bundle, single account object, or flat CPA array.
    pub raw_text: String,
}

#[derive(Debug, Serialize)]
pub struct ImportApiAccountsResponse {
    pub ok: bool,
    pub saved_count: usize,
    pub saved_names: Vec<String>,
    pub errors: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SaveClaudePresetResponse {
    pub ok: bool,
    pub preset: ClaudePresetSummary,
}

#[derive(Debug, Serialize)]
pub struct ApplyAuthPresetResponse {
    pub ok: bool,
    /// The request is recorded behind an active temporary preset lease and
    /// will be applied after that lease restores its snapshot.
    #[serde(default)]
    pub deferred: bool,
    pub preset_id: String,
    pub name: String,
    pub auth_file: String,
    pub config_file: String,
    /// Project-local .codex/config.toml that was synced so the switch takes
    /// effect even when a local config overrides the global one. Empty when no
    /// local config was found for the given project path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_config_file: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApplyApiPresetResponse {
    pub ok: bool,
    /// See [`ApplyAuthPresetResponse::deferred`].
    #[serde(default)]
    pub deferred: bool,
    pub preset_id: String,
    pub name: String,
    pub auth_file: String,
    pub config_file: String,
    /// See [`ApplyAuthPresetResponse::local_config_file`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_config_file: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApplyClaudePresetResponse {
    pub ok: bool,
    /// See [`ApplyAuthPresetResponse::deferred`].
    #[serde(default)]
    pub deferred: bool,
    pub preset_id: String,
    pub name: String,
    pub settings_file: String,
}

#[derive(Debug, Deserialize)]
pub struct ApplyCurrentAuthRequest {
    pub auth: AuthFile,
}

#[derive(Debug, Serialize)]
pub struct ApplyCurrentAuthResponse {
    pub ok: bool,
    pub auth_file: String,
    pub config_file: String,
    pub account_id: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexOAuthSessionStatus {
    Pending,
    Completed,
    Error,
    Expired,
}

#[derive(Debug, Clone)]
pub struct CodexOAuthSession {
    pub id: String,
    pub status: CodexOAuthSessionStatus,
    pub verification_url: String,
    pub authorize_url: String,
    pub user_code: String,
    pub poll_interval_seconds: u64,
    pub created_at: u64,
    pub updated_at: u64,
    pub expires_at: u64,
    pub error: Option<String>,
    pub auth: Option<AuthFile>,
    pub details: Option<AuthPresetDetails>,
    pub suggested_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CodexOAuthSessionResponse {
    pub(crate) ok: bool,
    pub(crate) session_id: String,
    pub(crate) status: CodexOAuthSessionStatus,
    pub(crate) verification_url: String,
    pub(crate) authorize_url: String,
    pub(crate) user_code: String,
    pub(crate) poll_interval_seconds: u64,
    pub(crate) created_at: u64,
    pub(crate) updated_at: u64,
    pub(crate) expires_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) auth: Option<AuthFile>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) details: Option<AuthPresetDetails>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) suggested_name: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexUsageResponse {
    #[serde(default)]
    pub(crate) rate_limit: Option<CodexRateLimit>,
    #[serde(default)]
    pub(crate) plan_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRateLimit {
    #[serde(default)]
    pub(crate) primary_window: Option<CodexUsageWindow>,
    #[serde(default)]
    pub(crate) secondary_window: Option<CodexUsageWindow>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexUsageWindow {
    #[serde(default)]
    pub(crate) used_percent: Option<f64>,
    #[serde(default)]
    pub(crate) reset_at: Option<u64>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexRefreshTokenResponse {
    pub(crate) access_token: String,
    #[serde(default)]
    pub(crate) refresh_token: String,
    #[serde(default)]
    pub(crate) id_token: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct CodexDeviceUserCodeRequest {
    pub(crate) client_id: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexDeviceUserCodeResponse {
    pub(crate) device_auth_id: String,
    #[serde(default)]
    pub(crate) user_code: String,
    #[serde(default, alias = "usercode")]
    pub(crate) user_code_alt: String,
    #[serde(default)]
    pub(crate) interval: Value,
}

#[derive(Debug, Serialize)]
pub(crate) struct CodexDeviceTokenRequest {
    pub(crate) device_auth_id: String,
    pub(crate) user_code: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexDeviceTokenResponse {
    pub(crate) authorization_code: String,
    pub(crate) code_verifier: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CodexAuthorizationCodeTokenResponse {
    pub(crate) access_token: String,
    #[serde(default)]
    pub(crate) refresh_token: String,
    #[serde(default)]
    pub(crate) id_token: String,
}

#[derive(Debug)]
pub(crate) struct CodexRemoteError {
    pub(crate) status: Option<HttpStatusCode>,
    pub(crate) message: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteAuthPresetResponse {
    pub ok: bool,
    pub deleted_id: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteApiPresetResponse {
    pub ok: bool,
    pub deleted_id: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteClaudePresetResponse {
    pub ok: bool,
    pub deleted_id: String,
}

#[derive(Debug, Clone)]
pub enum CurrentAuthState {
    Login(AuthFile),
    Api(ApiAuthFile),
}

impl CurrentAuthState {
    pub fn as_login(&self) -> Option<&AuthFile> {
        match self {
            Self::Login(auth) => Some(auth),
            Self::Api(_) => None,
        }
    }

    pub fn api_key(&self) -> Option<&str> {
        match self {
            Self::Login(auth) => auth.openai_api_key.as_deref(),
            Self::Api(auth) => Some(auth.openai_api_key.as_str()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigProviderState {
    pub provider_id: String,
    pub provider_name: Option<String>,
    pub base_url: Option<String>,
    pub wire_api: Option<String>,
    /// 当前 config.toml 中已应用的非 provider 根键/二级键取值，
    /// 用于在多个预设共享同一组 base_url+api_key+wire_api 时区分当前生效的是哪一个预设。
    pub config_values: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct CurrentApiState {
    pub preset_id: Option<String>,
    pub provider_id: Option<String>,
    pub provider_name: Option<String>,
    pub base_url: Option<String>,
    pub wire_api: Option<String>,
    pub api_key: Option<String>,
    pub preset_name: Option<String>,
    pub management_url: Option<String>,
    /// 当前 config.toml 中实际生效的 config 覆盖取值，供预设匹配区分同名凭据预设。
    pub config_values: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConfigTarget {
    pub key: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiProviderOptions {
    pub wire_api: String,
}

#[derive(Debug, Clone)]
pub struct CurrentClaudeState {
    pub provider_name: Option<String>,
    pub base_url: Option<String>,
    pub management_url: Option<String>,
    pub auth_token: Option<String>,
    pub default_haiku_model: Option<String>,
    pub default_sonnet_model: Option<String>,
    pub default_opus_model: Option<String>,
    pub third_party_model: Option<String>,
    pub config_values: BTreeMap<String, String>,
    pub preset_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PendingCodexDeviceLogin {
    pub device_auth_id: String,
    pub user_code: String,
    pub poll_interval_seconds: u64,
}
