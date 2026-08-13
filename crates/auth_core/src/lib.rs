use std::{collections::BTreeMap, sync::OnceLock};

use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use serde_json::{Map, Value};

mod config;
mod current;
mod models;
mod oauth;
mod selection;
mod storage;

pub(crate) use config::{
    clear_inactive_managed_config_entries_in_content, normalize_expected_config_key,
    normalize_expected_config_value, read_current_config_provider_from_content,
};
pub use config::{
    clear_provider_and_set_config_entry_in_config_content, clear_provider_in_config_content,
    merge_codex_snapshot_projects_in_config_content, parse_claude_settings_document,
    set_api_provider_and_config_entry_in_config_content, set_claude_settings_in_value,
    set_claude_settings_in_value_with_endpoint, set_local_proxy_auth_header_in_config_content,
};
#[cfg(test)]
pub(crate) use config::{
    clear_provider_and_set_model_in_config_content, set_api_provider_and_model_in_config_content,
    set_api_provider_in_config_content,
};
pub use current::{
    read_current_auth_state, read_current_claude_state, read_current_config_provider,
};
pub use models::{
    ApiAccessMode, ApiAuthFile, ApiPresetListResponse, ApiPresetSummary, ApiProviderOptions,
    ApiResponsesProxyMode, ApplyApiPresetResponse, ApplyAuthPresetResponse,
    ApplyClaudePresetResponse, ApplyCurrentAuthRequest, ApplyCurrentAuthResponse, AuthFile,
    AuthPresetDetails, AuthPresetListResponse, AuthPresetManager, AuthPresetSummary, AuthTokens,
    ClaudeAccessMode, ClaudePresetListResponse, ClaudePresetSummary, CodexAuthHttpClientProvider,
    CodexOAuthManager, CodexOAuthSession, CodexOAuthSessionResponse, CodexOAuthSessionStatus,
    ConfigProviderState, CurrentApiState, CurrentApiSummary, CurrentAuthMode, CurrentAuthState,
    CurrentAuthSummary, CurrentClaudeState, CurrentClaudeSummary, DeleteApiPresetResponse,
    DeleteAuthPresetResponse, DeleteClaudePresetResponse, ImportApiAccountsRequest,
    ImportApiAccountsResponse, PendingCodexDeviceLogin, PresetConfigOverride, PresetTerminalEnvVar,
    RefreshAllAuthPresetsResponse, RefreshAuthPresetFailure, ResolvedConfigTarget,
    SaveApiPresetRequest, SaveApiPresetResponse, SaveAuthPresetRequest, SaveAuthPresetResponse,
    SaveClaudePresetRequest, SaveClaudePresetResponse, StoredApiPreset, StoredAuthPreset,
    StoredClaudePreset, SwitchCounted, UpdateUpstreamProxySettingsRequest, UpstreamProxySettings,
    UpstreamProxySettingsResponse,
};
pub(crate) use models::{
    AuthFileContent, CodexAuthorizationCodeTokenResponse, CodexDeviceTokenRequest,
    CodexDeviceTokenResponse, CodexDeviceUserCodeRequest, CodexDeviceUserCodeResponse,
    CodexRefreshTokenResponse, CodexRemoteError, CodexUsageResponse,
};
#[cfg(test)]
pub(crate) use models::{CodexRateLimit, CodexUsageWindow};
pub use oauth::{
    build_codex_device_authorize_url, codex_oauth_session_response, complete_codex_device_login,
    extract_account_id_from_auth, parse_codex_device_poll_interval,
    refresh_stored_auth_preset_quota, request_codex_device_user_code, summarize_remote_body,
    touch_auth_last_refresh,
};
pub use selection::{
    ApiPresetLookup, ApiPresetSelectionEntry, ApiPresetSelectionError, api_preset_model,
    model_from_config_overrides, select_api_preset_index,
};
#[cfg(test)]
pub(crate) use storage::upsert_model_catalog_entry_in_value;
pub use storage::{
    bump_switch_count, clear_config_provider, current_timestamp_secs, generate_preset_id,
    normalize_api_preset, normalize_auth_preset, normalize_claude_preset,
    persist_api_presets_async, persist_auth_presets_async, persist_claude_presets_async,
    persist_upstream_proxy_settings, resolve_api_preset_name, resolve_auth_preset_name,
    resolve_claude_preset_name, sync_api_model_catalog, sync_api_preset_config,
    sync_auth_preset_config, validate_api_auth_file_sync, validate_api_key_sync,
    validate_auth_file_sync, write_api_auth_file, write_claude_settings_file,
    write_login_auth_file, write_opencode_config_file,
};
pub(crate) use storage::{suggest_api_label, validate_auth_file};

pub const AUTH_FILE_RELATIVE_PATH: &str = ".codex/auth.json";
pub const CONFIG_FILE_RELATIVE_PATH: &str = ".codex/config.toml";
pub const CLAUDE_SETTINGS_FILE_RELATIVE_PATH: &str = ".claude/settings.json";
pub const CLAUDE_ONBOARDING_BYPASS_FILE: &str = ".claude.json";
const PRESETS_FILE_NAME: &str = "webclx-auth-presets.json";
const API_PRESETS_FILE_NAME: &str = "webclx-api-presets.json";
const CLAUDE_PRESETS_FILE_NAME: &str = "webclx-claude-presets.json";
const API_PROVIDER_KEY: &str = "webclx_api";
pub const WEBCLX_API_WIRE_API: &str = "responses";
pub const WEBCLX_LOCAL_API_TOKEN_ENV: &str = "WEBCLX_LOCAL_API_TOKEN";
pub const WEBCLX_LOCAL_API_TOKEN_HEADER: &str = "X-WebClx-Local-Token";
const UPSTREAM_PROXY_SETTINGS_FILE_NAME: &str = "webclx-upstream-proxy.json";
pub const LOCAL_PROXY_API_KEY: &str = "webclx-local-api-proxy";
pub const LOCAL_PROXY_CLAUDE_TOKEN: &str = "webclx-local-claude-proxy";
pub const LOCAL_PROXY_API_KEY_PREFIX: &str = "webclx-local-api-proxy:";
pub const LOCAL_PROXY_CLAUDE_TOKEN_PREFIX: &str = "webclx-local-claude-proxy:";
pub const OPENAI_UPSTREAM_PROXY_BASE_PATH: &str = "/api/upstream/openai/v1";
pub const ANTHROPIC_UPSTREAM_PROXY_BASE_PATH: &str = "/api/upstream/anthropic";
static LOCAL_WEBCLX_ORIGIN: OnceLock<String> = OnceLock::new();
const MINIMAX_RESPONSES_PROXY_PATH: &str = "/api/codex-proxy/minimax/v1";
const DEEPSEEK_RESPONSES_PROXY_PATH: &str = "/api/codex-proxy/deepseek/v1";
pub const ANTHROPIC_RESPONSES_PROXY_PATH: &str = "/api/codex-proxy/anthropic/v1";
const CLAUDE_AUTH_TOKEN_KEY: &str = "ANTHROPIC_API_KEY";
const CLAUDE_LEGACY_AUTH_TOKEN_KEY: &str = "ANTHROPIC_AUTH_TOKEN";
const CLAUDE_BASE_URL_KEY: &str = "ANTHROPIC_BASE_URL";
const CLAUDE_DEFAULT_HAIKU_MODEL_KEY: &str = "ANTHROPIC_DEFAULT_HAIKU_MODEL";
const CLAUDE_DEFAULT_SONNET_MODEL_KEY: &str = "ANTHROPIC_DEFAULT_SONNET_MODEL";
const CLAUDE_DEFAULT_OPUS_MODEL_KEY: &str = "ANTHROPIC_DEFAULT_OPUS_MODEL";
const CLAUDE_MODEL_KEY: &str = "ANTHROPIC_MODEL";
const CLAUDE_LEGACY_SMALL_FAST_MODEL_KEY: &str = "ANTHROPIC_SMALL_FAST_MODEL";
const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const CODEX_OAUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const CODEX_USAGE_USER_AGENT: &str = "CodexBar";
const CODEX_DEVICE_USER_CODE_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/usercode";
const CODEX_DEVICE_TOKEN_URL: &str = "https://auth.openai.com/api/accounts/deviceauth/token";
pub const CODEX_DEVICE_VERIFICATION_URL: &str = "https://auth.openai.com/codex/device";
const CODEX_DEVICE_TOKEN_EXCHANGE_REDIRECT_URI: &str =
    "https://auth.openai.com/deviceauth/callback";
const CODEX_DEVICE_TIMEOUT_SECS: u64 = 15 * 60;
const CODEX_DEVICE_DEFAULT_POLL_INTERVAL_SECS: u64 = 5;
const CODEX_OAUTH_SESSION_RETENTION_SECS: u64 = 60 * 60;

pub fn short_account_id(account_id: &str) -> String {
    account_id
        .chars()
        .rev()
        .take(6)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

pub fn auth_summary(auth: &AuthFile) -> CurrentAuthSummary {
    let details = derive_auth_preset_details(auth);
    CurrentAuthSummary {
        account_id: auth.tokens.account_id.clone(),
        short_id: short_account_id(&auth.tokens.account_id),
        email: details.email,
        plan_type: details.plan_type,
        last_refresh: auth.last_refresh.clone(),
    }
}

pub fn current_api_summary(current_api: &CurrentApiState) -> CurrentApiSummary {
    CurrentApiSummary {
        provider_id: current_api.provider_id.clone(),
        provider_name: current_api.provider_name.clone(),
        base_url: current_api.base_url.clone(),
        wire_api: current_api.wire_api.clone(),
        masked_api_key: current_api.api_key.as_deref().map(mask_api_key),
        preset_name: current_api.preset_name.clone(),
        management_url: current_api.management_url.clone(),
    }
}

pub fn current_claude_summary(current_claude: &CurrentClaudeState) -> CurrentClaudeSummary {
    CurrentClaudeSummary {
        provider_name: current_claude.provider_name.clone(),
        base_url: current_claude.base_url.clone(),
        management_url: current_claude.management_url.clone(),
        default_haiku_model: current_claude.default_haiku_model.clone(),
        default_sonnet_model: current_claude.default_sonnet_model.clone(),
        default_opus_model: current_claude.default_opus_model.clone(),
        third_party_model: current_claude.third_party_model.clone(),
        masked_auth_token: current_claude.auth_token.as_deref().map(mask_api_key),
        preset_name: current_claude.preset_name.clone(),
    }
}

pub fn preset_summary(
    preset: &StoredAuthPreset,
    current_auth: Option<&AuthFile>,
) -> AuthPresetSummary {
    let (config_key, config_value, secondary_config_key, secondary_config_value) =
        legacy_preset_config_override_fields(&preset.config_overrides);
    AuthPresetSummary {
        id: preset.id.clone(),
        name: preset.name.clone(),
        account_id: preset.auth.tokens.account_id.clone(),
        last_refresh: preset.auth.last_refresh.clone(),
        saved_at: preset.saved_at,
        active: current_auth == Some(&preset.auth),
        details: merge_auth_preset_details(&preset.details, &preset.auth),
        config_overrides: preset.config_overrides.clone(),
        config_key,
        config_value,
        secondary_config_key,
        secondary_config_value,
        auth: preset.auth.clone(),
        switch_count: preset.switch_count,
    }
}

pub fn api_preset_summary(
    preset: &StoredApiPreset,
    current_mode: CurrentAuthMode,
    current_api: Option<&CurrentApiState>,
) -> ApiPresetSummary {
    api_preset_summary_with_active(preset, current_mode, current_api, None)
}

pub fn api_preset_summary_with_proxy_state(
    preset: &StoredApiPreset,
    current_mode: CurrentAuthMode,
    current_api: Option<&CurrentApiState>,
    upstream_proxy: &UpstreamProxySettings,
) -> ApiPresetSummary {
    if current_mode == CurrentAuthMode::Api && current_api.is_some() {
        return api_preset_summary_with_active(
            preset,
            current_mode,
            current_api,
            Some(api_preset_matches_current_api(preset, current_mode, current_api)),
        );
    }

    let active_preset_id = upstream_proxy.active_api_proxy_preset_id.as_deref();
    // 两个前置分支产生相同的 Some(...) 值，但都是第三分支（额外校验
    // api_preset_has_current_api_credentials）的必要 gate，不可合并；
    // clippy identical_blocks 为误报，显式放行以保留 gate 语义。
    #[allow(clippy::if_same_then_else)]
    let active_override = if api_preset_enables_local_upstream_proxy_on_apply(preset) {
        Some(active_preset_id == Some(preset.id.as_str()))
    } else if upstream_proxy.codex_api_proxy_enabled {
        Some(active_preset_id == Some(preset.id.as_str()))
    } else {
        active_preset_id.map(|id| {
            id == preset.id.as_str()
                && api_preset_has_current_api_credentials(preset, current_mode, current_api)
        })
    };

    api_preset_summary_with_active(preset, current_mode, current_api, active_override)
}

fn api_preset_summary_with_active(
    preset: &StoredApiPreset,
    current_mode: CurrentAuthMode,
    current_api: Option<&CurrentApiState>,
    active_override: Option<bool>,
) -> ApiPresetSummary {
    let provider_base_url = api_provider_base_url(preset);
    let access_mode = preset.access_mode.unwrap_or(ApiAccessMode::Direct);
    let is_chatgpt_oauth = access_mode == ApiAccessMode::ChatgptOauth;
    let active = active_override
        .unwrap_or_else(|| api_preset_matches_current_api(preset, current_mode, current_api));
    let (config_key, config_value, secondary_config_key, secondary_config_value) =
        legacy_preset_config_override_fields(&preset.config_overrides);

    ApiPresetSummary {
        id: preset.id.clone(),
        name: preset.name.clone(),
        provider_name: preset.provider_name.clone(),
        base_url: preset.base_url.clone(),
        management_url: preset.management_url.clone(),
        wire_api: Some(api_provider_options(preset).wire_api),
        responses_proxy: preset
            .responses_proxy
            .clone()
            .or_else(|| infer_api_responses_proxy(preset)),
        apply_upstream_proxy_on_switch: api_preset_enables_local_upstream_proxy_on_apply(preset),
        provider_base_url: if provider_base_url == preset.base_url {
            None
        } else {
            Some(provider_base_url)
        },
        terminal_env: preset.terminal_env.clone(),
        terminal_startup_script: preset.terminal_startup_script.clone(),
        config_overrides: preset.config_overrides.clone(),
        config_key,
        config_value,
        secondary_config_key,
        secondary_config_value,
        api_key: preset.api_key.clone(),
        masked_api_key: if is_chatgpt_oauth {
            "OAuth Token".to_string()
        } else {
            mask_api_key(&preset.api_key)
        },
        access_mode,
        masked_access_token: is_chatgpt_oauth.then(|| mask_api_key(&preset.access_token)),
        account_id: is_chatgpt_oauth.then(|| preset.account_id.clone()),
        saved_at: preset.saved_at,
        active,
        switch_count: preset.switch_count,
    }
}

pub fn api_preset_matches_current_api(
    preset: &StoredApiPreset,
    current_mode: CurrentAuthMode,
    current_api: Option<&CurrentApiState>,
) -> bool {
    api_preset_has_current_applied_api_credentials(preset, current_mode, current_api)
        && current_api.is_some_and(|current| {
            current
                .preset_name
                .as_deref()
                .is_none_or(|name| name == preset.name)
        })
}

pub fn api_preset_has_current_api_credentials(
    preset: &StoredApiPreset,
    current_mode: CurrentAuthMode,
    current_api: Option<&CurrentApiState>,
) -> bool {
    api_preset_has_current_applied_api_credentials(preset, current_mode, current_api)
}

pub fn api_preset_has_current_applied_api_credentials(
    preset: &StoredApiPreset,
    current_mode: CurrentAuthMode,
    current_api: Option<&CurrentApiState>,
) -> bool {
    let use_local_proxy = api_preset_enables_local_upstream_proxy_on_apply(preset);
    let provider_base_url = api_provider_base_url_for_mode(preset, use_local_proxy);
    let expected_api_key = if use_local_proxy {
        local_proxy_api_key_for_preset_id(&preset.id)
    } else {
        preset.api_key.clone()
    };
    let expected_wire_api = api_provider_options(preset).wire_api;
    current_mode == CurrentAuthMode::Api
        && current_api.is_some_and(|current| {
            current.base_url.as_deref() == Some(provider_base_url.as_str())
                && current.api_key.as_deref() == Some(expected_api_key.as_str())
                && current.wire_api.as_deref().unwrap_or(WEBCLX_API_WIRE_API)
                    == expected_wire_api.as_str()
        })
}

pub fn local_proxy_api_key_for_preset_id(preset_id: &str) -> String {
    format!("{LOCAL_PROXY_API_KEY_PREFIX}{preset_id}")
}

pub fn local_proxy_claude_token_for_preset_id(preset_id: &str) -> String {
    format!("{LOCAL_PROXY_CLAUDE_TOKEN_PREFIX}{preset_id}")
}

pub fn local_proxy_api_preset_id_from_api_key(value: &str) -> Option<&str> {
    value
        .trim()
        .strip_prefix(LOCAL_PROXY_API_KEY_PREFIX)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn local_proxy_claude_preset_id_from_token(value: &str) -> Option<&str> {
    value
        .trim()
        .strip_prefix(LOCAL_PROXY_CLAUDE_TOKEN_PREFIX)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub fn claude_preset_summary(
    preset: &StoredClaudePreset,
    current_claude: Option<&CurrentClaudeState>,
) -> ClaudePresetSummary {
    claude_preset_summary_with_active(preset, current_claude, None)
}

pub fn claude_preset_summary_with_proxy_state(
    preset: &StoredClaudePreset,
    current_claude: Option<&CurrentClaudeState>,
    upstream_proxy: &UpstreamProxySettings,
) -> ClaudePresetSummary {
    if upstream_proxy.claude_proxy_enabled {
        let active_from_current = current_claude
            .is_some_and(|current| claude_preset_matches_current_claude_state(preset, current));
        let active_from_legacy_proxy_state = current_claude
            .is_some_and(|current| current.auth_token.as_deref() == Some(LOCAL_PROXY_CLAUDE_TOKEN))
            && upstream_proxy.active_claude_proxy_preset_id.as_deref() == Some(preset.id.as_str());
        claude_preset_summary_with_active(
            preset,
            current_claude,
            Some(active_from_current || active_from_legacy_proxy_state),
        )
    } else {
        claude_preset_summary(preset, current_claude)
    }
}

pub fn claude_preset_summary_with_effective_proxy_state(
    preset: &StoredClaudePreset,
    effective_preset: &StoredClaudePreset,
    current_claude: Option<&CurrentClaudeState>,
    upstream_proxy: &UpstreamProxySettings,
) -> ClaudePresetSummary {
    let active_from_current = current_claude.is_some_and(|current| {
        claude_preset_matches_current_claude_state(effective_preset, current)
    });
    let active_from_legacy_proxy_state = upstream_proxy.claude_proxy_enabled
        && current_claude
            .is_some_and(|current| current.auth_token.as_deref() == Some(LOCAL_PROXY_CLAUDE_TOKEN))
        && upstream_proxy.active_claude_proxy_preset_id.as_deref() == Some(preset.id.as_str());
    claude_preset_summary_with_active(
        preset,
        current_claude,
        Some(active_from_current || active_from_legacy_proxy_state),
    )
}

fn claude_preset_summary_with_active(
    preset: &StoredClaudePreset,
    current_claude: Option<&CurrentClaudeState>,
    active_override: Option<bool>,
) -> ClaudePresetSummary {
    let active = active_override.unwrap_or_else(|| {
        current_claude
            .is_some_and(|current| claude_preset_matches_current_claude_state(preset, current))
    });
    let (config_key, config_value, secondary_config_key, secondary_config_value) =
        legacy_preset_config_override_fields(&preset.config_overrides);

    ClaudePresetSummary {
        id: preset.id.clone(),
        name: preset.name.clone(),
        provider_name: preset.provider_name.clone(),
        base_url: preset.base_url.clone(),
        management_url: preset.management_url.clone(),
        config_overrides: preset.config_overrides.clone(),
        config_key,
        config_value,
        secondary_config_key,
        secondary_config_value,
        auth_token: preset.auth_token.clone(),
        masked_auth_token: mask_api_key(&preset.auth_token),
        default_haiku_model: preset.default_haiku_model.clone(),
        default_sonnet_model: preset.default_sonnet_model.clone(),
        default_opus_model: preset.default_opus_model.clone(),
        third_party_model: preset.third_party_model.clone(),
        use_local_proxy: preset.use_local_proxy,
        access_mode: effective_claude_access_mode(preset),
        saved_at: preset.saved_at,
        active,
        switch_count: preset.switch_count,
    }
}

fn claude_preset_matches_current_claude_state(
    preset: &StoredClaudePreset,
    current: &CurrentClaudeState,
) -> bool {
    let direct_matches = current.base_url.as_deref() == Some(preset.base_url.as_str())
        && current.auth_token.as_deref() == Some(preset.auth_token.as_str());
    let local_proxy_token = local_proxy_claude_token_for_preset_id(&preset.id);
    let local_proxy_base_url = claude_provider_base_url_for_mode(preset, true);
    let local_proxy_matches = current.auth_token.as_deref() == Some(local_proxy_token.as_str())
        && current.base_url.as_deref() == Some(local_proxy_base_url.as_str());

    (direct_matches || local_proxy_matches)
        && current.default_haiku_model
            == effective_claude_model_value(
                preset,
                CLAUDE_DEFAULT_HAIKU_MODEL_KEY,
                preset.default_haiku_model.as_ref(),
            )
        && current.default_sonnet_model
            == effective_claude_model_value(
                preset,
                CLAUDE_DEFAULT_SONNET_MODEL_KEY,
                preset.default_sonnet_model.as_ref(),
            )
        && current.default_opus_model
            == effective_claude_model_value(
                preset,
                CLAUDE_DEFAULT_OPUS_MODEL_KEY,
                preset.default_opus_model.as_ref(),
            )
        && current.third_party_model
            == effective_claude_model_value(
                preset,
                CLAUDE_MODEL_KEY,
                preset.third_party_model.as_ref(),
            )
        && claude_config_overrides_match_current(preset, current)
}

fn effective_claude_model_value(
    preset: &StoredClaudePreset,
    key: &str,
    dedicated: Option<&String>,
) -> Option<String> {
    preset
        .config_overrides
        .iter()
        .rev()
        .find(|item| {
            item.key
                .as_deref()
                .is_some_and(|candidate| candidate.eq_ignore_ascii_case(key))
        })
        .and_then(|item| item.value.as_deref())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| dedicated.cloned())
}

pub fn derive_current_mode(
    current_auth: Option<&CurrentAuthState>,
    current_config: Option<&ConfigProviderState>,
) -> CurrentAuthMode {
    if current_config.is_some() || matches!(current_auth, Some(CurrentAuthState::Api(_))) {
        CurrentAuthMode::Api
    } else if matches!(current_auth, Some(CurrentAuthState::Login(_))) {
        CurrentAuthMode::Auth
    } else {
        CurrentAuthMode::None
    }
}

pub fn derive_current_api_state(
    current_config: Option<&ConfigProviderState>,
    current_auth: Option<&CurrentAuthState>,
    presets: &[StoredApiPreset],
) -> Option<CurrentApiState> {
    let api_key = current_auth
        .and_then(CurrentAuthState::api_key)
        .and_then(|value| sanitize_optional_text(Some(value.to_string())));
    if current_config.is_none() && api_key.is_none() {
        return None;
    }

    let matched_preset = current_config.and_then(|config| {
        // 同一组 base_url+api_key+wire_api 可能被多个预设共享
        // (例如同一上游、不同 model_context_window 的预设)。仅凭凭据无法区分，
        // 因此先按凭据匹配出候选集合，再优先选择 config_overrides 也与当前
        // config.toml 实际取值一致的预设。由于一个预设的 overrides 可能是另一个
        // 预设 overrides 的真子集 (例如 "sub2api" 是 "sub2api gpt-5.5 1M" 的子集)，
        // 在两者 override 都能匹配当前取值时，必须选择 override 更多、更具体的那个，
        // 否则会错误地选中较泛的预设。
        let candidates: Vec<&StoredApiPreset> = presets
            .iter()
            .filter(|preset| api_preset_credentials_match(preset, config, current_auth))
            .collect();
        let config_match_candidates = candidates
            .iter()
            .copied()
            .filter(|preset| {
                !preset.config_overrides.is_empty()
                    && api_preset_config_overrides_match_values(preset, &config.config_values)
            })
            .collect::<Vec<_>>();
        if !config_match_candidates.is_empty() {
            // override 数量最多 (最具体) 的候选胜出；数量相同则保留原有顺序优先。
            // Iterator::max_by 在相等时返回最后一个元素，这里需要保留首个，因此用
            // 较少 override 优先的 min_by 再取反，使首个最大元素胜出。
            config_match_candidates
                .into_iter()
                .enumerate()
                .max_by(|(idx_a, a), (idx_b, b)| {
                    a.config_overrides
                        .len()
                        .cmp(&b.config_overrides.len())
                        .then(idx_b.cmp(idx_a))
                })
                .map(|(_, preset)| preset)
        } else {
            candidates.first().copied()
        }
    });

    Some(CurrentApiState {
        preset_id: matched_preset.map(|preset| preset.id.clone()),
        provider_id: current_config.map(|config| config.provider_id.clone()),
        provider_name: matched_preset
            .map(|preset| preset.provider_name.clone())
            .or_else(|| current_config.and_then(|config| config.provider_name.clone()))
            .or_else(|| current_config.map(|config| config.provider_id.clone())),
        base_url: current_config.and_then(|config| config.base_url.clone()),
        wire_api: current_config.and_then(|config| config.wire_api.clone()),
        api_key,
        preset_name: matched_preset.map(|preset| preset.name.clone()),
        management_url: matched_preset.and_then(|preset| preset.management_url.clone()),
        config_values: current_config
            .map(|config| config.config_values.clone())
            .unwrap_or_default(),
    })
}

/// 判断预设的 base_url+api_key+wire_api 是否与当前 config/auth 文件一致。
fn api_preset_credentials_match(
    preset: &StoredApiPreset,
    config: &ConfigProviderState,
    current_auth: Option<&CurrentAuthState>,
) -> bool {
    let expected_wire_api = api_provider_options(preset).wire_api;
    let current_key = current_auth.and_then(CurrentAuthState::api_key);
    let wire_api_matches =
        config.wire_api.as_deref().unwrap_or(WEBCLX_API_WIRE_API) == expected_wire_api;
    let direct_matches = current_key == Some(preset.api_key.as_str())
        && config.base_url.as_deref() == Some(api_provider_base_url(preset).as_str());
    let local_proxy_base_url = api_provider_base_url_for_mode(preset, true);
    let local_proxy_api_key = local_proxy_api_key_for_preset_id(&preset.id);
    let local_proxy_matches = api_preset_enables_local_upstream_proxy_on_apply(preset)
        && current_key == Some(local_proxy_api_key.as_str())
        && config.base_url.as_deref() == Some(local_proxy_base_url.as_str());
    wire_api_matches && (direct_matches || local_proxy_matches)
}

/// 判断预设的 config_overrides 是否全部与当前 config.toml 实际取值一致。
/// 用于在凭据相同的多个预设中区分当前生效的那个。
fn api_preset_config_overrides_match_values(
    preset: &StoredApiPreset,
    values: &BTreeMap<String, String>,
) -> bool {
    preset.config_overrides.iter().all(|item| {
        let Some(key) = item
            .key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return false;
        };
        let Some(expected) = item
            .value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return false;
        };
        values.get(key).is_some_and(|value| value == expected)
    })
}

pub fn derive_current_claude_state(
    settings: &Value,
    presets: &[StoredClaudePreset],
) -> Option<CurrentClaudeState> {
    let env = settings.get("env").and_then(Value::as_object);
    let auth_token = env_value_as_text(env, CLAUDE_AUTH_TOKEN_KEY)
        .or_else(|| env_value_as_text(env, CLAUDE_LEGACY_AUTH_TOKEN_KEY));
    let base_url = env_value_as_text(env, CLAUDE_BASE_URL_KEY);
    let default_haiku_model = env_value_as_text(env, CLAUDE_DEFAULT_HAIKU_MODEL_KEY)
        .or_else(|| env_value_as_text(env, CLAUDE_LEGACY_SMALL_FAST_MODEL_KEY));
    let default_sonnet_model = env_value_as_text(env, CLAUDE_DEFAULT_SONNET_MODEL_KEY);
    let default_opus_model = env_value_as_text(env, CLAUDE_DEFAULT_OPUS_MODEL_KEY);
    let third_party_model = env_value_as_text(env, CLAUDE_MODEL_KEY);
    let config_values = current_claude_config_values(env, presets);

    if auth_token.is_none()
        && base_url.is_none()
        && default_haiku_model.is_none()
        && default_sonnet_model.is_none()
        && default_opus_model.is_none()
        && third_party_model.is_none()
    {
        return None;
    }

    let matched_preset = presets.iter().find(|preset| {
        let direct_matches = auth_token.as_deref() == Some(preset.auth_token.as_str())
            && base_url.as_deref() == Some(preset.base_url.as_str());
        let local_proxy_token = local_proxy_claude_token_for_preset_id(&preset.id);
        let local_proxy_base_url = claude_provider_base_url_for_mode(preset, true);
        let local_proxy_matches = auth_token.as_deref() == Some(local_proxy_token.as_str())
            && base_url.as_deref() == Some(local_proxy_base_url.as_str());
        (direct_matches || local_proxy_matches)
            && default_haiku_model
                == effective_claude_model_value(
                    preset,
                    CLAUDE_DEFAULT_HAIKU_MODEL_KEY,
                    preset.default_haiku_model.as_ref(),
                )
            && default_sonnet_model
                == effective_claude_model_value(
                    preset,
                    CLAUDE_DEFAULT_SONNET_MODEL_KEY,
                    preset.default_sonnet_model.as_ref(),
                )
            && default_opus_model
                == effective_claude_model_value(
                    preset,
                    CLAUDE_DEFAULT_OPUS_MODEL_KEY,
                    preset.default_opus_model.as_ref(),
                )
            && third_party_model
                == effective_claude_model_value(
                    preset,
                    CLAUDE_MODEL_KEY,
                    preset.third_party_model.as_ref(),
                )
            && claude_config_overrides_match_values(preset, &config_values)
    });

    Some(CurrentClaudeState {
        provider_name: matched_preset
            .map(|preset| preset.provider_name.clone())
            .or_else(|| base_url.as_deref().map(suggest_api_label)),
        base_url,
        management_url: matched_preset.and_then(|preset| preset.management_url.clone()),
        auth_token,
        default_haiku_model,
        default_sonnet_model,
        default_opus_model,
        third_party_model,
        config_values,
        preset_name: matched_preset.map(|preset| preset.name.clone()),
    })
}

fn current_claude_config_values(
    env: Option<&Map<String, Value>>,
    presets: &[StoredClaudePreset],
) -> BTreeMap<String, String> {
    let mut values = BTreeMap::new();
    let Some(env) = env else {
        return values;
    };

    for preset in presets {
        for item in &preset.config_overrides {
            let Some(key) = item
                .key
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            if values.contains_key(key) {
                continue;
            }
            if let Some(value) = env_value_as_text(Some(env), key) {
                values.insert(key.to_string(), value);
            }
        }
    }
    values
}

fn claude_config_overrides_match_current(
    preset: &StoredClaudePreset,
    current: &CurrentClaudeState,
) -> bool {
    claude_config_overrides_match_values(preset, &current.config_values)
}

fn claude_config_overrides_match_values(
    preset: &StoredClaudePreset,
    values: &BTreeMap<String, String>,
) -> bool {
    preset.config_overrides.iter().all(|item| {
        let Some(key) = item
            .key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return false;
        };
        let Some(expected) = item
            .value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return false;
        };
        values.get(key).is_some_and(|value| value == expected)
    })
}

fn mask_api_key(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return "未配置".to_string();
    }

    if trimmed.len() <= 12 {
        return format!("{}***", &trimmed[..trimmed.len().min(4)]);
    }

    format!(
        "{}***{}",
        &trimmed[..trimmed.len().min(6)],
        &trimmed[trimmed.len().saturating_sub(4)..]
    )
}

pub fn sanitize_auth_preset_details(details: AuthPresetDetails) -> AuthPresetDetails {
    AuthPresetDetails {
        email: sanitize_optional_text(details.email),
        plan_type: sanitize_optional_text(details.plan_type),
        account_name: sanitize_optional_text(details.account_name),
        login_method: sanitize_optional_text(details.login_method),
        hourly_percentage: details.hourly_percentage,
        hourly_reset_time: details.hourly_reset_time,
        weekly_percentage: details.weekly_percentage,
        weekly_reset_time: details.weekly_reset_time,
    }
}

fn sanitize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    })
}

fn sanitize_optional_config_key(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = sanitize_optional_text(value) else {
        return Ok(None);
    };
    Ok(Some(normalize_expected_config_key(&value)?))
}

fn sanitize_optional_config_value(value: Option<String>) -> Option<String> {
    sanitize_optional_text(value)
}

pub fn sanitize_terminal_env_vars(vars: Vec<PresetTerminalEnvVar>) -> Vec<PresetTerminalEnvVar> {
    let mut resolved = Vec::new();
    for item in vars {
        if resolved.len() >= 64 {
            break;
        }
        let key = item.key.trim().to_string();
        if !is_valid_terminal_env_key(&key)
            || is_forbidden_preset_env_key(&key)
            || resolved
                .iter()
                .any(|entry: &PresetTerminalEnvVar| entry.key == key)
        {
            continue;
        }
        let value = sanitize_terminal_env_value(&item.value);
        resolved.push(PresetTerminalEnvVar { key, value });
    }
    resolved
}

pub fn sanitize_terminal_startup_script(_value: Option<String>) -> Option<String> {
    None
}

pub fn is_forbidden_preset_env_key(key: &str) -> bool {
    key.eq_ignore_ascii_case("HOME") || is_forbidden_config_home_env_key(key)
}

pub fn forbidden_config_home_env_keys() -> &'static [&'static str] {
    &["CODEX_HOME", "CLAUDE_CONFIG_DIR", "WEBCLX_USER_HOME"]
}

pub fn is_forbidden_config_home_env_key(key: &str) -> bool {
    forbidden_config_home_env_keys()
        .iter()
        .any(|forbidden| key.eq_ignore_ascii_case(forbidden))
}

fn is_valid_terminal_env_key(key: &str) -> bool {
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

fn sanitize_terminal_env_value(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control() || matches!(character, '\t'))
        .collect::<String>()
        .trim()
        .chars()
        .take(4096)
        .collect()
}

fn sanitize_preset_config_override(
    config_key: Option<String>,
    config_value: Option<String>,
) -> Result<(Option<String>, Option<String>)> {
    Ok((
        sanitize_optional_config_key(config_key)?,
        sanitize_optional_config_value(config_value),
    ))
}

fn build_preset_config_override(
    config_key: Option<String>,
    config_value: Option<String>,
) -> Result<Option<PresetConfigOverride>> {
    let (key, value) = sanitize_preset_config_override(config_key, config_value)?;
    if key.is_none() && value.is_none() {
        return Ok(None);
    }
    Ok(Some(PresetConfigOverride { key, value }))
}

pub fn effective_preset_config_overrides(
    config_overrides: Vec<PresetConfigOverride>,
    legacy_config_key: Option<String>,
    legacy_config_value: Option<String>,
    legacy_secondary_config_key: Option<String>,
    legacy_secondary_config_value: Option<String>,
) -> Result<Vec<PresetConfigOverride>> {
    if !config_overrides.is_empty() {
        return sanitize_preset_config_overrides(config_overrides);
    }

    let mut resolved = Vec::new();
    if let Some(override_item) =
        build_preset_config_override(legacy_config_key, legacy_config_value)?
    {
        resolved.push(override_item);
    }
    if let Some(override_item) =
        build_preset_config_override(legacy_secondary_config_key, legacy_secondary_config_value)?
    {
        resolved.push(override_item);
    }
    Ok(resolved)
}

pub fn effective_claude_config_overrides(
    config_overrides: Vec<PresetConfigOverride>,
    legacy_config_key: Option<String>,
    legacy_config_value: Option<String>,
    legacy_secondary_config_key: Option<String>,
    legacy_secondary_config_value: Option<String>,
) -> Result<Vec<PresetConfigOverride>> {
    let overrides = effective_preset_config_overrides(
        config_overrides,
        legacy_config_key,
        legacy_config_value,
        legacy_secondary_config_key,
        legacy_secondary_config_value,
    )?;

    for (index, item) in overrides.iter().enumerate() {
        if item
            .key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            anyhow::bail!("第 {} 个 Claude 额外选项需要填写键名。", index + 1);
        }
        if item
            .value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
        {
            anyhow::bail!("第 {} 个 Claude 额外选项需要填写键值。", index + 1);
        }
    }

    Ok(overrides)
}

fn sanitize_preset_config_overrides(
    config_overrides: Vec<PresetConfigOverride>,
) -> Result<Vec<PresetConfigOverride>> {
    let mut resolved = Vec::new();
    for (index, item) in config_overrides.into_iter().enumerate() {
        let Some(override_item) = build_preset_config_override(item.key, item.value)? else {
            continue;
        };
        if index >= 2 && override_item.key.is_none() {
            anyhow::bail!("第 {} 个 config 覆盖需要填写键名。", index + 1);
        }
        if index >= 2 && override_item.value.is_none() {
            anyhow::bail!("第 {} 个 config 覆盖需要填写键值。", index + 1);
        }
        resolved.push(override_item);
    }
    Ok(resolved)
}

fn legacy_preset_config_override_fields(
    config_overrides: &[PresetConfigOverride],
) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    let first = config_overrides.first();
    let second = config_overrides.get(1);
    (
        first.and_then(|item| item.key.clone()),
        first.and_then(|item| item.value.clone()),
        second.and_then(|item| item.key.clone()),
        second.and_then(|item| item.value.clone()),
    )
}

pub fn resolve_effective_preset_config_targets(
    defaults: &[(&str, &str)],
    config_overrides: &[PresetConfigOverride],
) -> Result<Vec<ResolvedConfigTarget>> {
    let mut targets = Vec::new();
    for &(key, value) in defaults {
        if key.trim().is_empty() || value.trim().is_empty() {
            continue;
        }
        upsert_config_target(
            &mut targets,
            ResolvedConfigTarget {
                key: normalize_expected_config_key(key)?,
                value: normalize_expected_config_value(value)?,
            },
        );
    }

    for (index, item) in config_overrides.iter().enumerate() {
        let fallback = defaults.get(index).copied();
        let key = item
            .key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| fallback.map(|(key, _)| key))
            .ok_or_else(|| anyhow::anyhow!("第 {} 个 config 覆盖缺少键名。", index + 1))?;
        let value = item
            .value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                fallback
                    .map(|(_, value)| value)
                    .filter(|value| !value.trim().is_empty())
            })
            .ok_or_else(|| anyhow::anyhow!("第 {} 个 config 覆盖缺少键值。", index + 1))?;
        upsert_config_target(
            &mut targets,
            ResolvedConfigTarget {
                key: normalize_expected_config_key(key)?,
                value: normalize_expected_config_value(value)?,
            },
        );
    }

    ensure_auto_compact_token_limit(&mut targets);
    Ok(targets)
}

pub fn resolve_effective_claude_config_overrides(
    defaults: &[(&str, &str)],
    preset: &StoredClaudePreset,
) -> Result<Vec<PresetConfigOverride>> {
    let mut resolved = Vec::new();
    for &(key, value) in defaults {
        let key = key.trim();
        let value = value.trim();
        if key.is_empty() || value.is_empty() || !is_valid_terminal_env_key(key) {
            continue;
        }
        if matches!(
            key.to_ascii_uppercase().as_str(),
            CLAUDE_AUTH_TOKEN_KEY
                | CLAUDE_LEGACY_AUTH_TOKEN_KEY
                | CLAUDE_BASE_URL_KEY
                | CLAUDE_LEGACY_SMALL_FAST_MODEL_KEY
        ) {
            continue;
        }
        upsert_claude_config_override(
            &mut resolved,
            PresetConfigOverride {
                key: Some(key.to_string()),
                value: Some(value.to_string()),
            },
        );
    }

    let dedicated_models = [
        (CLAUDE_DEFAULT_HAIKU_MODEL_KEY, preset.default_haiku_model.as_deref()),
        (CLAUDE_DEFAULT_SONNET_MODEL_KEY, preset.default_sonnet_model.as_deref()),
        (CLAUDE_DEFAULT_OPUS_MODEL_KEY, preset.default_opus_model.as_deref()),
        (CLAUDE_MODEL_KEY, preset.third_party_model.as_deref()),
    ];
    for (key, value) in dedicated_models {
        if value.is_some_and(|value| !value.trim().is_empty()) {
            resolved.retain(|item| {
                !item
                    .key
                    .as_deref()
                    .is_some_and(|candidate| candidate.eq_ignore_ascii_case(key))
            });
        }
    }

    for item in &preset.config_overrides {
        let key = item
            .key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Claude 额外选项缺少键名。"))?;
        let value = item
            .value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("Claude 额外选项缺少键值。"))?;
        if !is_valid_terminal_env_key(key) {
            anyhow::bail!("Claude 额外选项键名无效: {key}");
        }
        upsert_claude_config_override(
            &mut resolved,
            PresetConfigOverride {
                key: Some(key.to_string()),
                value: Some(value.to_string()),
            },
        );
    }

    Ok(resolved)
}

fn upsert_claude_config_override(
    overrides: &mut Vec<PresetConfigOverride>,
    next: PresetConfigOverride,
) {
    let next_key = next.key.as_deref().unwrap_or_default();
    if let Some(existing) = overrides.iter_mut().find(|item| {
        item.key
            .as_deref()
            .is_some_and(|key| key.eq_ignore_ascii_case(next_key))
    }) {
        *existing = next;
    } else {
        overrides.push(next);
    }
}

fn upsert_config_target(targets: &mut Vec<ResolvedConfigTarget>, next: ResolvedConfigTarget) {
    if let Some(existing) = targets
        .iter_mut()
        .find(|target| target.key.eq_ignore_ascii_case(&next.key))
    {
        *existing = next;
    } else {
        targets.push(next);
    }
}

/// 当 targets 中存在 model_context_window 但没有显式 model_auto_compact_token_limit 时，
/// 自动注入压缩阈值为上下文窗口的 80%。
fn ensure_auto_compact_token_limit(targets: &mut Vec<ResolvedConfigTarget>) {
    let has_auto_compact = targets.iter().any(|target| {
        target
            .key
            .eq_ignore_ascii_case("model_auto_compact_token_limit")
    });
    if has_auto_compact {
        return;
    }
    let Some(context_window) = targets
        .iter()
        .rev()
        .find(|target| target.key.eq_ignore_ascii_case("model_context_window"))
        .and_then(|target| target.value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
    else {
        return;
    };
    let compact_limit = (context_window as f64 * 0.8) as i64;
    if compact_limit <= 0 {
        return;
    }
    upsert_config_target(
        targets,
        ResolvedConfigTarget {
            key: "model_auto_compact_token_limit".to_string(),
            value: compact_limit.to_string(),
        },
    );
}

pub fn sanitize_api_key(value: String) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("api_key 不能为空。");
    }
    Ok(trimmed.to_string())
}

pub fn sanitize_auth_token(value: String) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("auth_token 不能为空。");
    }
    Ok(trimmed.to_string())
}

pub fn sanitize_base_url(value: String) -> Result<String> {
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        anyhow::bail!("base_url 不能为空。");
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        anyhow::bail!("base_url 需要以 http:// 或 https:// 开头。");
    }
    Ok(trimmed.to_string())
}

pub fn sanitize_api_wire_api(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = sanitize_optional_text(value) else {
        return Ok(None);
    };
    let normalized = value.to_ascii_lowercase();
    match normalized.as_str() {
        "responses" | "chat" => Ok(Some(WEBCLX_API_WIRE_API.to_string())),
        _ => anyhow::bail!("wire_api 只支持 responses。"),
    }
}

pub fn api_provider_options(preset: &StoredApiPreset) -> ApiProviderOptions {
    let _ = preset;
    ApiProviderOptions {
        wire_api: WEBCLX_API_WIRE_API.to_string(),
    }
}

#[cfg(test)]
pub fn default_api_provider_options() -> ApiProviderOptions {
    ApiProviderOptions {
        wire_api: WEBCLX_API_WIRE_API.to_string(),
    }
}

pub fn effective_api_responses_proxy(preset: &StoredApiPreset) -> Option<ApiResponsesProxyMode> {
    match preset.responses_proxy.as_ref() {
        Some(ApiResponsesProxyMode::Direct) => None,
        Some(mode) => Some(mode.clone()),
        None => infer_api_responses_proxy(preset),
    }
}

fn infer_api_responses_proxy(preset: &StoredApiPreset) -> Option<ApiResponsesProxyMode> {
    let model = api_preset_model(preset).unwrap_or_default();
    let model_lower = model.to_ascii_lowercase();
    let base_url_lower = preset.base_url.to_ascii_lowercase();
    let provider_name_lower = preset.provider_name.to_ascii_lowercase();

    // Third-party Anthropic-compatible relays must be detected before the
    // generic deepseek/minimax checks fire, because some providers (e.g.
    // `https://api.deepseek.com/anthropic`) share the deepseek host but
    // expose an Anthropic-compatible surface.
    let looks_like_anthropic_url = base_url_lower.contains("/anthropic")
        || base_url_lower.contains("anthropic.com")
        || provider_name_lower.contains("anthropic")
        || provider_name_lower.contains("claude");
    let looks_like_anthropic_model = model_lower.contains("claude")
        || model_lower.contains("anthropic")
        || model_lower.starts_with("claude-");
    if looks_like_anthropic_url && looks_like_anthropic_model {
        return Some(ApiResponsesProxyMode::AnthropicChat);
    }

    let is_minimax = preset.base_url.contains("api.minimaxi.com")
        || preset.base_url.contains("api.minimax.io")
        || model_lower.contains("minimax");
    if is_minimax && model.starts_with("codex-MiniMax-") {
        return Some(ApiResponsesProxyMode::MinimaxChat);
    }

    let is_deepseek = !base_url_lower.contains("/anthropic")
        && (preset.base_url.contains("api.deepseek.com") || model_lower.contains("deepseek"));
    let supports_native_responses = model_lower == "deepseek-v4-flash";
    if is_deepseek && !supports_native_responses {
        return Some(ApiResponsesProxyMode::DeepseekChat);
    }

    let is_zhipu = !base_url_lower.contains("/api/codex-proxy/zhipu/")
        && (base_url_lower.contains("open.bigmodel.cn")
            || base_url_lower.contains("bigmodel.cn")
            || model_lower.contains("glm")
            || provider_name_lower.contains("zhipu")
            || provider_name_lower.contains("智谱")
            || provider_name_lower.contains("bigmodel"));
    if is_zhipu {
        return Some(ApiResponsesProxyMode::OpenaiChat);
    }

    None
}

pub fn api_preset_prefers_local_upstream_proxy(preset: &StoredApiPreset) -> bool {
    let model = api_preset_model(preset)
        .unwrap_or_default()
        .to_ascii_lowercase();
    let base_url = preset.base_url.to_ascii_lowercase();
    let provider_name = preset.provider_name.to_ascii_lowercase();
    let haystacks = [model.as_str(), base_url.as_str(), provider_name.as_str()];

    haystacks.iter().any(|value| {
        value.contains("glm")
            || value.contains("zhipu")
            || value.contains("bigmodel.cn")
            || value.contains("deepseek")
            || value.contains("minimax")
            || value.contains("minimaxi")
    })
}

pub fn api_preset_enables_local_upstream_proxy_on_apply(preset: &StoredApiPreset) -> bool {
    preset.apply_upstream_proxy_on_switch
}

pub fn api_provider_base_url(preset: &StoredApiPreset) -> String {
    match effective_api_responses_proxy(preset) {
        Some(ApiResponsesProxyMode::Direct) => preset.base_url.clone(),
        Some(ApiResponsesProxyMode::OpenaiChat) => preset.base_url.clone(),
        Some(ApiResponsesProxyMode::MinimaxChat) => {
            format!("{}{}", local_webclx_origin(), MINIMAX_RESPONSES_PROXY_PATH)
        }
        Some(ApiResponsesProxyMode::DeepseekChat) => {
            format!("{}{}", local_webclx_origin(), DEEPSEEK_RESPONSES_PROXY_PATH)
        }
        Some(ApiResponsesProxyMode::AnthropicChat) => {
            format!("{}{}", local_webclx_origin(), ANTHROPIC_RESPONSES_PROXY_PATH)
        }
        None => preset.base_url.clone(),
    }
}

pub fn api_provider_base_url_for_mode(preset: &StoredApiPreset, proxy_enabled: bool) -> String {
    if proxy_enabled {
        format!("{}{}", local_webclx_origin(), OPENAI_UPSTREAM_PROXY_BASE_PATH)
    } else {
        api_provider_base_url(preset)
    }
}

pub fn claude_provider_base_url_for_mode(
    preset: &StoredClaudePreset,
    proxy_enabled: bool,
) -> String {
    if proxy_enabled {
        format!("{}{}", local_webclx_origin(), ANTHROPIC_UPSTREAM_PROXY_BASE_PATH)
    } else {
        preset.base_url.clone()
    }
}

pub fn claude_preset_supports_direct_anthropic_endpoint(preset: &StoredClaudePreset) -> bool {
    claude_base_url_supports_direct_anthropic_endpoint(&preset.base_url)
}

pub fn claude_base_url_supports_direct_anthropic_endpoint(base_url: &str) -> bool {
    let normalized = base_url.trim().trim_end_matches('/').to_ascii_lowercase();
    normalized.contains("/anthropic") || normalized.contains("anthropic.com")
}

pub fn effective_claude_use_local_proxy(preset: &StoredClaudePreset) -> bool {
    matches!(
        effective_claude_access_mode(preset),
        ClaudeAccessMode::AnthropicRelay
            | ClaudeAccessMode::OpenaiChat
            | ClaudeAccessMode::OpenaiResponses
    )
}

pub fn effective_claude_access_mode(preset: &StoredClaudePreset) -> ClaudeAccessMode {
    let mode = preset.access_mode.unwrap_or({
        if preset.use_local_proxy {
            ClaudeAccessMode::AnthropicProxy
        } else {
            ClaudeAccessMode::Direct
        }
    });
    match mode {
        ClaudeAccessMode::AnthropicProxy => ClaudeAccessMode::AnthropicRelay,
        other => other,
    }
}

fn local_webclx_origin() -> String {
    if let Some(origin) = LOCAL_WEBCLX_ORIGIN.get() {
        return origin.clone();
    }

    if let Ok(value) = std::env::var("WEBCLX_PUBLIC_URL") {
        let trimmed = value.trim().trim_end_matches('/');
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }

    let bind_addr = std::env::var("WEBCLX_ADDR").unwrap_or_else(|_| "0.0.0.0:11111".to_string());
    let port = bind_addr
        .rsplit_once(':')
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .unwrap_or(11111);
    format!("http://127.0.0.1:{port}")
}

pub fn set_local_webclx_origin(origin: impl Into<String>) {
    let _ = LOCAL_WEBCLX_ORIGIN.set(origin.into());
}

pub fn sanitize_api_provider_name(value: String, base_url: &str) -> String {
    let trimmed = value.trim();
    if !trimmed.is_empty() {
        return trimmed.to_string();
    }
    suggest_api_label(base_url)
}

pub fn sanitize_management_url(value: Option<String>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Ok(None);
    }
    if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
        anyhow::bail!("management_url 需要以 http:// 或 https:// 开头。");
    }
    Ok(Some(trimmed.to_string()))
}

pub fn resolve_api_management_url(
    value: Option<String>,
    same_as_base: bool,
    base_url: &str,
) -> Result<Option<String>> {
    if same_as_base {
        return Ok(Some(base_url.to_string()));
    }
    sanitize_management_url(value)
}

pub fn sanitize_claude_model(value: Option<String>) -> Option<String> {
    sanitize_optional_text(value)
}

pub fn validate_claude_model_selection(
    default_haiku_model: Option<&str>,
    default_sonnet_model: Option<&str>,
    default_opus_model: Option<&str>,
    third_party_model: Option<&str>,
) -> Result<()> {
    let has_official_models = default_haiku_model.is_some()
        || default_sonnet_model.is_some()
        || default_opus_model.is_some();
    if has_official_models && third_party_model.is_some() {
        anyhow::bail!("官方模型设置和第三方模型设置不能同时填写。");
    }
    Ok(())
}

pub fn validate_claude_code_endpoint_compatibility(base_url: &str) -> Result<()> {
    let lower = base_url.trim().trim_end_matches('/').to_ascii_lowercase();
    if lower == "https://api.deepseek.com" || lower == "https://api.deepseek.com/v1" {
        anyhow::bail!(
            "DeepSeek 官方 OpenAI 兼容接口不能直接切换到 Claude Code；Claude Code 需要 Anthropic 兼容接口。DeepSeek 官方 Anthropic 端点请使用 https://api.deepseek.com/anthropic。"
        );
    }
    Ok(())
}

fn env_value_as_text(env: Option<&Map<String, Value>>, key: &str) -> Option<String> {
    env.and_then(|env| env.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn merge_auth_preset_details(stored: &AuthPresetDetails, auth: &AuthFile) -> AuthPresetDetails {
    let derived = derive_auth_preset_details(auth);
    AuthPresetDetails {
        email: stored.email.clone().or(derived.email),
        plan_type: stored.plan_type.clone().or(derived.plan_type),
        account_name: stored.account_name.clone().or(derived.account_name),
        login_method: stored.login_method.clone().or(derived.login_method),
        hourly_percentage: stored.hourly_percentage.or(derived.hourly_percentage),
        hourly_reset_time: stored.hourly_reset_time.or(derived.hourly_reset_time),
        weekly_percentage: stored.weekly_percentage.or(derived.weekly_percentage),
        weekly_reset_time: stored.weekly_reset_time.or(derived.weekly_reset_time),
    }
}

fn merge_refreshed_auth_preset_details(
    stored: &AuthPresetDetails,
    auth: &AuthFile,
    usage: &CodexUsageResponse,
) -> AuthPresetDetails {
    let derived = derive_auth_preset_details(auth);
    let primary_window = usage
        .rate_limit
        .as_ref()
        .and_then(|limit| limit.primary_window.as_ref());
    let secondary_window = usage
        .rate_limit
        .as_ref()
        .and_then(|limit| limit.secondary_window.as_ref());

    AuthPresetDetails {
        email: stored.email.clone().or(derived.email),
        plan_type: normalize_plan_type_label(usage.plan_type.as_deref())
            .or(stored.plan_type.clone())
            .or(derived.plan_type),
        account_name: stored.account_name.clone().or(derived.account_name),
        login_method: stored.login_method.clone().or(derived.login_method),
        hourly_percentage: primary_window
            .and_then(|window| round_percentage(window.used_percent))
            .or(stored.hourly_percentage),
        hourly_reset_time: primary_window
            .and_then(|window| window.reset_at)
            .or(stored.hourly_reset_time),
        weekly_percentage: secondary_window
            .and_then(|window| round_percentage(window.used_percent))
            .or(stored.weekly_percentage),
        weekly_reset_time: secondary_window
            .and_then(|window| window.reset_at)
            .or(stored.weekly_reset_time),
    }
}

fn normalize_plan_type_label(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_uppercase)
}

fn round_percentage(value: Option<f64>) -> Option<u64> {
    value
        .filter(|value| value.is_finite())
        .map(|value| value.clamp(0.0, 100.0).round() as u64)
}

fn derive_auth_preset_details(auth: &AuthFile) -> AuthPresetDetails {
    let id_payload = decode_jwt_payload(&auth.tokens.id_token);
    let access_payload = decode_jwt_payload(&auth.tokens.access_token);
    let auth_claim = id_payload
        .as_ref()
        .and_then(|payload| payload.get("https://api.openai.com/auth"))
        .or_else(|| {
            access_payload
                .as_ref()
                .and_then(|payload| payload.get("https://api.openai.com/auth"))
        });
    let profile_claim = access_payload
        .as_ref()
        .and_then(|payload| payload.get("https://api.openai.com/profile"));

    AuthPresetDetails {
        email: first_json_string(&[
            profile_claim
                .and_then(|claim| claim.get("email"))
                .and_then(Value::as_str),
            id_payload
                .as_ref()
                .and_then(|payload| payload.get("email"))
                .and_then(Value::as_str),
        ]),
        plan_type: first_json_string(&[auth_claim
            .and_then(|claim| claim.get("chatgpt_plan_type"))
            .and_then(Value::as_str)])
        .map(|value| value.to_uppercase()),
        account_name: auth_claim
            .and_then(|claim| claim.get("organizations"))
            .and_then(pick_organization_title),
        login_method: first_json_string(&[id_payload
            .as_ref()
            .and_then(|payload| payload.get("auth_provider"))
            .and_then(Value::as_str)])
        .map(normalize_login_method_label),
        ..AuthPresetDetails::default()
    }
}

fn decode_jwt_payload(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let decoded = general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .or_else(|_| general_purpose::URL_SAFE.decode(payload))
        .ok()?;
    serde_json::from_slice(&decoded).ok()
}

fn first_json_string(values: &[Option<&str>]) -> Option<String> {
    values
        .iter()
        .flatten()
        .map(|value| value.trim())
        .find(|value| !value.is_empty())
        .map(str::to_string)
}

fn pick_organization_title(value: &Value) -> Option<String> {
    let organizations = value.as_array()?;
    organizations
        .iter()
        .filter_map(|item| item.get("title").and_then(Value::as_str))
        .map(str::trim)
        .find(|title| !title.is_empty() && *title != "Personal")
        .map(str::to_string)
        .or_else(|| {
            organizations
                .iter()
                .filter_map(|item| item.get("title").and_then(Value::as_str))
                .map(str::trim)
                .find(|title| !title.is_empty())
                .map(str::to_string)
        })
}

fn normalize_login_method_label(value: String) -> String {
    match value.trim().to_ascii_lowercase().as_str() {
        "password" => "Password".to_string(),
        "google-oauth2" | "google" => "Google".to_string(),
        "github" => "GitHub".to_string(),
        "apple" => "Apple".to_string(),
        "microsoft" | "microsoft-account" => "Microsoft".to_string(),
        "oauth" => "OAuth".to_string(),
        _ => {
            let mut chars = value.trim().chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => value,
            }
        }
    }
}

/// Parsed account from import JSON (sub2api bundle, CPA array, or single object).
#[derive(Debug, Clone)]
pub struct ImportedAccount {
    pub access_token: String,
    pub account_id: String,
    pub name: String,
    pub email: String,
}

/// Parse imported account JSON text and extract access_token + account_id for each account.
/// Supports JSON streams, sub2api bundles, flat arrays, and standard auth.json objects.
/// When `chatgpt_account_id` is empty, recovers it from the JWT access_token payload.
pub fn parse_imported_accounts(raw_text: &str) -> Result<Vec<ImportedAccount>> {
    let values = serde_json::Deserializer::from_str(raw_text)
        .into_iter::<Value>()
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("内容不是有效的 JSON: {e}"))?;
    if values.is_empty() {
        anyhow::bail!("内容不是有效的 JSON: 内容为空。");
    }

    let mut accounts = Vec::new();
    for value in &values {
        collect_imported_account_values(value, &mut accounts);
    }

    let mut result = Vec::new();
    for account in &accounts {
        let credentials = account
            .get("credentials")
            .filter(|v| v.is_object())
            .or_else(|| account.get("tokens").filter(|v| v.is_object()))
            .unwrap_or(account);

        let access_token = first_json_string(&[
            credentials.get("access_token").and_then(Value::as_str),
            credentials.get("accessToken").and_then(Value::as_str),
            credentials.get("token").and_then(Value::as_str),
            account.get("access_token").and_then(Value::as_str),
            account.get("accessToken").and_then(Value::as_str),
            account.get("token").and_then(Value::as_str),
        ])
        .unwrap_or_default();
        if access_token.is_empty() {
            continue;
        }

        let explicit_account_id = first_json_string(&[
            credentials
                .get("chatgpt_account_id")
                .and_then(Value::as_str),
            credentials.get("account_id").and_then(Value::as_str),
            credentials.get("chatgptAccountId").and_then(Value::as_str),
            credentials.get("accountId").and_then(Value::as_str),
            account.get("chatgpt_account_id").and_then(Value::as_str),
            account.get("account_id").and_then(Value::as_str),
            account.get("chatgptAccountId").and_then(Value::as_str),
            account.get("accountId").and_then(Value::as_str),
        ]);

        let account_id = explicit_account_id
            .clone()
            .or_else(|| {
                let payload = decode_jwt_payload(&access_token)?;
                payload
                    .get("https://api.openai.com/auth.chatgpt_account_id")
                    .and_then(Value::as_str)
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .or_else(|| {
                        payload
                            .get("https://api.openai.com/auth")?
                            .get("chatgpt_account_id")
                            .and_then(Value::as_str)
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                    })
            })
            .unwrap_or_default();

        let extra = account.get("extra").filter(|v| v.is_object());
        let name = first_json_string(&[
            account.get("name").and_then(Value::as_str),
            account.get("email").and_then(Value::as_str),
        ])
        .unwrap_or_else(|| {
            if !account_id.is_empty() {
                let start = account_id.len().saturating_sub(6);
                format!("账号 {}", &account_id[start..])
            } else {
                "未命名账号".to_string()
            }
        });

        let email = first_json_string(&[
            credentials.get("email").and_then(Value::as_str),
            account.get("email").and_then(Value::as_str),
            extra.and_then(|e| e.get("email")).and_then(Value::as_str),
        ])
        .unwrap_or_default();

        result.push(ImportedAccount {
            access_token,
            account_id,
            name,
            email,
        });
    }

    if result.is_empty() {
        anyhow::bail!("内容缺少可用的账号对象。");
    }

    Ok(result)
}

fn collect_imported_account_values<'a>(value: &'a Value, accounts: &mut Vec<&'a Value>) {
    if let Some(items) = value.as_array() {
        for item in items {
            collect_imported_account_values(item, accounts);
        }
        return;
    }

    if let Some(items) = value.get("accounts").and_then(Value::as_array) {
        for item in items {
            collect_imported_account_values(item, accounts);
        }
        return;
    }

    if let Some(items) = value
        .get("data")
        .and_then(|data| data.get("accounts"))
        .and_then(Value::as_array)
    {
        for item in items {
            collect_imported_account_values(item, accounts);
        }
        return;
    }

    accounts.push(value);
}

#[cfg(test)]
mod tests;
