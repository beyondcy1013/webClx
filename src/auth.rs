use std::{
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result};
use auth_core::*;
use axum::{
    Json,
    extract::{Multipart, Path as AxumPath, State},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use terminal_core::current_timestamp_millis;
use tokio::process::Command;
use tracing::warn;

use crate::{ApiResult, AppError, AppState, runtime_paths};

pub use auth_core::{AuthPresetManager, CodexOAuthManager};

mod account_import;
mod apply;
mod preset_run_lease;

#[cfg(test)]
use account_import::API_ACCOUNT_IMPORT_MAX_ARCHIVE_DEPTH;
pub(crate) use account_import::API_ACCOUNT_IMPORT_MAX_UPLOAD_BYTES;
#[cfg(test)]
use account_import::collect_accounts_from_upload;
use account_import::collect_accounts_from_uploads;

#[cfg(test)]
use apply::verify_api_preset_targets;
use apply::{
    activate_dynamic_claude_relay_if_needed, claude_preset_with_global_defaults,
    write_claude_preset_to_targets,
};
pub use apply::{
    apply_api_preset, apply_auth_preset, apply_claude_preset, apply_claude_preset_to_opencode,
    apply_current_auth, verify_api_preset,
};
pub(crate) use preset_run_lease::{
    AcquiredPresetRunLease, PresetRunKind, PresetRunLeaseManager, begin_preset_run_lease,
    heartbeat_preset_run_lease_internal, recover_stale_preset_run_lease,
    release_preset_run_lease_internal,
};
pub use preset_run_lease::{
    acquire_preset_run_lease, heartbeat_preset_run_lease, release_preset_run_lease,
};

const PRESET_TEST_TIMEOUT_SECS: u64 = 45;
const PRESET_CHAT_PROBE_DELAY: Duration = Duration::from_secs(2);
const CODEX_BUNDLED_MODELS_TIMEOUT: Duration = Duration::from_secs(15);
const ANTHROPIC_API_VERSION: &str = "2023-06-01";
const UPSTREAM_PRESET_ID_HEADER: &str = "x-webclx-upstream-preset-id";
#[cfg(not(windows))]
const DEFAULT_CODEX_COMMAND_PATH: &str =
    "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
#[cfg(windows)]
const DEFAULT_CODEX_COMMAND_PATH: &str =
    "C:\\Windows\\System32;C:\\Windows;C:\\Windows\\System32\\WindowsPowerShell\\v1.0";

#[derive(Debug, Serialize)]
pub struct PresetTestResponse {
    pub ok: bool,
    pub result: PresetTestResult,
}

#[derive(Debug, Serialize)]
pub struct PresetBatchTestResponse {
    pub ok: bool,
    pub total: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub results: Vec<PresetTestResult>,
}

#[derive(Debug, Deserialize)]
pub struct ReorderPresetsRequest {
    ids: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ReorderPresetsResponse {
    pub ok: bool,
    pub ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetTestResult {
    pub preset_id: String,
    pub name: String,
    pub ok: bool,
    pub endpoint: String,
    pub status: Option<u16>,
    pub latency_ms: u128,
    pub message: String,
}

impl CodexAuthHttpClientProvider for crate::proxy::ProxyManager {
    fn build_auth_client(&self, timeout_secs: u64) -> Result<reqwest::Client> {
        // OAuth operations (device login, token refresh, usage fetch) must
        // always use a proxy: application proxy preset when active, otherwise
        // inherit shell proxy environment variables. Do not fall back to a
        // direct (no-proxy) connection.
        self.build_oauth_client(timeout_secs)
    }

    fn active_proxy_server(&self) -> Option<String> {
        self.get_active().map(|proxy| proxy.server)
    }
}

fn reorder_presets_by_ids<T, F>(
    presets: Vec<T>,
    ids: &[String],
    id_for: F,
    label: &str,
) -> ApiResult<Vec<T>>
where
    F: Fn(&T) -> &str,
{
    if ids.len() != presets.len() {
        return Err(AppError::bad_request(format!("{label} 预设排序列表必须包含当前全部预设。")));
    }

    let mut seen = HashSet::with_capacity(ids.len());
    for id in ids {
        if id.trim().is_empty() || !seen.insert(id.as_str()) {
            return Err(AppError::bad_request(format!(
                "{label} 预设排序列表包含空 id 或重复 id。"
            )));
        }
    }

    let mut by_id = HashMap::with_capacity(presets.len());
    for preset in presets {
        let id = id_for(&preset).to_string();
        if by_id.insert(id, preset).is_some() {
            return Err(AppError::internal(format!("{label} 预设存储中存在重复 id。")));
        }
    }

    let mut reordered = Vec::with_capacity(ids.len());
    for id in ids {
        let Some(preset) = by_id.remove(id) else {
            return Err(AppError::bad_request(format!("{label} 预设排序列表包含未知 id。")));
        };
        reordered.push(preset);
    }

    if !by_id.is_empty() {
        return Err(AppError::bad_request(format!("{label} 预设排序列表必须包含当前全部预设。")));
    }

    Ok(reordered)
}

#[derive(Debug, Clone)]
struct TerminalAuthFiles {
    user_name: String,
    auth_file: PathBuf,
    config_file: PathBuf,
    claude_settings_file: PathBuf,
}

fn terminal_auth_files(state: &AppState) -> ApiResult<TerminalAuthFiles> {
    terminal_auth_files_for_user(&state.workspace_settings.terminal_user())
}

fn terminal_auth_files_for_user(user: &str) -> ApiResult<TerminalAuthFiles> {
    let normalized_user = user.trim().to_string();
    if normalized_user.is_empty() {
        return Err(AppError::bad_request("用户身份无效: 用户身份不能为空。"));
    }
    let resolve = |relative_path: &str| -> Result<PathBuf, AppError> {
        runtime_paths::resolve_user_file(&normalized_user, relative_path)
            .map_err(|error| AppError::bad_request(format!("用户身份无效: {error}")))
    };
    Ok(TerminalAuthFiles {
        user_name: normalized_user.clone(),
        auth_file: resolve(AUTH_FILE_RELATIVE_PATH)?,
        config_file: resolve(CONFIG_FILE_RELATIVE_PATH)?,
        claude_settings_file: resolve(CLAUDE_SETTINGS_FILE_RELATIVE_PATH)?,
    })
}

/// Read auth.json + config.toml and derive the common fields needed by both
/// Codex_OAuth and Codex_API preset list handlers.  The returned struct borrows
/// the two owned values passed in (`current_auth_value`, `current_config_value`)
/// so callers must keep them alive while using the result.
#[allow(clippy::too_many_arguments)]
async fn read_codex_current_state(
    auth_file: &PathBuf,
    config_file: &PathBuf,
    api_presets: &[StoredApiPreset],
) -> ApiResult<(
    Option<CurrentAuthState>,
    Option<String>,
    Option<ConfigProviderState>,
    Option<String>,
    CurrentAuthMode,
    Option<CurrentApiState>,
)> {
    let current_auth = read_current_auth_state(auth_file).await;
    let current_config = read_current_config_provider(config_file).await;
    let (current_auth_value, current_auth_error) = match current_auth {
        Ok(value) => (value, None),
        Err(error) => (None, Some(error)),
    };
    let (current_config_value, current_config_error) = match current_config {
        Ok(value) => (value, None),
        Err(error) => (None, Some(error)),
    };
    let current_mode =
        derive_current_mode(current_auth_value.as_ref(), current_config_value.as_ref());
    let current_api = derive_current_api_state(
        current_config_value.as_ref(),
        current_auth_value.as_ref(),
        api_presets,
    );
    Ok((
        current_auth_value,
        current_auth_error,
        current_config_value,
        current_config_error,
        current_mode,
        current_api,
    ))
}

/// Unified config-overrides validation wrapper: takes the same five legacy fields
/// that all three save request types carry, resolves them, and maps the error
/// into a 400 with the given preset label.
fn resolve_config_overrides(
    config_overrides: Vec<PresetConfigOverride>,
    legacy_config_key: Option<String>,
    legacy_config_value: Option<String>,
    legacy_secondary_config_key: Option<String>,
    legacy_secondary_config_value: Option<String>,
    kind: ConfigOverrideKind,
    label: &str,
) -> ApiResult<Vec<PresetConfigOverride>> {
    let result = match kind {
        ConfigOverrideKind::Codex => effective_preset_config_overrides(
            config_overrides,
            legacy_config_key,
            legacy_config_value,
            legacy_secondary_config_key,
            legacy_secondary_config_value,
        ),
        ConfigOverrideKind::Claude => effective_claude_config_overrides(
            config_overrides,
            legacy_config_key,
            legacy_config_value,
            legacy_secondary_config_key,
            legacy_secondary_config_value,
        ),
    };
    result.map_err(|error| AppError::bad_request(format!("{label} 预设无效: {error}")))
}

enum ConfigOverrideKind {
    Codex,
    Claude,
}

fn terminal_auth_write_targets(state: &AppState) -> ApiResult<Vec<TerminalAuthFiles>> {
    let configured_user = state.workspace_settings.terminal_user();
    let target_users = collect_terminal_auth_target_users(&configured_user);
    if target_users.is_empty() {
        return Err(AppError::bad_request("无可用终端用户。"));
    }

    let mut targets = Vec::new();
    let mut first_error: Option<AppError> = None;
    for user in &target_users {
        let normalized_user = user.trim();
        if normalized_user.is_empty()
            || targets
                .iter()
                .any(|target: &TerminalAuthFiles| target.user_name == normalized_user)
        {
            continue;
        }
        match terminal_auth_files_for_user(normalized_user) {
            Ok(target) => targets.push(target),
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
                warn!("skip mirrored auth target {normalized_user}: profile resolution failed");
            }
        }
    }

    if targets.is_empty() {
        Err(first_error.unwrap_or_else(|| AppError::bad_request("所有终端用户均无法解析。")))
    } else {
        Ok(targets)
    }
}

fn collect_terminal_auth_target_users(configured_user: &str) -> Vec<String> {
    let mut users = Vec::new();
    push_unique_auth_target_user_name(&mut users, configured_user);
    users
}

fn push_unique_auth_target_user_name(users: &mut Vec<String>, user: &str) {
    let normalized_user = user.trim();
    if normalized_user.is_empty() || users.iter().any(|existing| existing == normalized_user) {
        return;
    }
    users.push(normalized_user.to_string());
}

fn map_bad_request(prefix: &str, error: anyhow::Error) -> AppError {
    AppError::bad_request(format!("{prefix}: {error}"))
}

fn map_internal(prefix: &str, error: anyhow::Error) -> AppError {
    AppError::internal(format!("{prefix}: {error}"))
}

fn validate_auth_file(auth: &AuthFile) -> ApiResult<()> {
    auth_core::validate_auth_file_sync(auth)
        .map_err(|error| map_bad_request("auth 数据无效", error))
}

fn validate_api_auth_file(auth: &ApiAuthFile) -> ApiResult<()> {
    auth_core::validate_api_auth_file_sync(auth)
        .map_err(|error| map_bad_request("API auth 数据无效", error))
}

async fn persist_auth_presets_async(
    manager: &AuthPresetManager,
    presets: &[StoredAuthPreset],
) -> ApiResult<()> {
    auth_core::persist_auth_presets_async(manager, presets)
        .await
        .map_err(|error| map_internal("保存 auth 预设失败", error))
}

async fn persist_api_presets_async(
    manager: &AuthPresetManager,
    presets: &[StoredApiPreset],
) -> ApiResult<()> {
    auth_core::persist_api_presets_async(manager, presets)
        .await
        .map_err(|error| map_internal("保存 API 预设失败", error))
}

async fn persist_claude_presets_async(
    manager: &AuthPresetManager,
    presets: &[StoredClaudePreset],
) -> ApiResult<()> {
    auth_core::persist_claude_presets_async(manager, presets)
        .await
        .map_err(|error| map_internal("保存 Claude 预设失败", error))
}

async fn write_login_auth_file(path: &std::path::Path, auth: &AuthFile) -> ApiResult<()> {
    validate_auth_file(auth)?;
    auth_core::write_login_auth_file(path, auth)
        .await
        .map_err(|error| map_internal("写入 auth.json 失败", error))
}

async fn write_login_auth_files(targets: &[TerminalAuthFiles], auth: &AuthFile) -> ApiResult<()> {
    for target in targets {
        write_login_auth_file(&target.auth_file, auth).await?;
    }
    Ok(())
}

async fn write_api_auth_file(path: &std::path::Path, auth: &ApiAuthFile) -> ApiResult<()> {
    validate_api_auth_file(auth)?;
    auth_core::write_api_auth_file(path, auth)
        .await
        .map_err(|error| map_internal("写入 API auth.json 失败", error))
}

async fn write_api_auth_files(targets: &[TerminalAuthFiles], auth: &ApiAuthFile) -> ApiResult<()> {
    for target in targets {
        write_api_auth_file(&target.auth_file, auth).await?;
    }
    Ok(())
}

async fn write_claude_settings_file(
    path: &std::path::Path,
    preset: &StoredClaudePreset,
) -> ApiResult<()> {
    validate_claude_code_endpoint_compatibility(&preset.base_url)
        .map_err(|error| map_bad_request("Claude 预设无效", error))?;
    auth_core::write_claude_settings_file(path, preset)
        .await
        .map_err(|error| map_internal("写入 Claude settings 失败", error))
}

async fn write_claude_settings_files(
    targets: &[TerminalAuthFiles],
    preset: &StoredClaudePreset,
) -> ApiResult<()> {
    for target in targets {
        write_claude_settings_file(&target.claude_settings_file, preset).await?;
    }
    Ok(())
}

async fn write_opencode_config_file(
    path: &std::path::Path,
    preset: &StoredClaudePreset,
) -> ApiResult<()> {
    auth_core::write_opencode_config_file(path, preset)
        .await
        .map_err(|error| map_internal("写入 opencode.json 失败", error))
}

async fn clear_config_provider(path: &std::path::Path) -> ApiResult<()> {
    auth_core::clear_config_provider(path)
        .await
        .map_err(|error| map_internal("更新 config.toml 失败", error))
}

async fn clear_config_providers(targets: &[TerminalAuthFiles]) -> ApiResult<()> {
    for target in targets {
        clear_config_provider(&target.config_file).await?;
    }
    Ok(())
}

async fn sync_auth_preset_config(
    path: &std::path::Path,
    targets: &[ResolvedConfigTarget],
) -> ApiResult<()> {
    auth_core::sync_auth_preset_config(path, targets)
        .await
        .map_err(|error| map_internal("更新 auth 预设 config 失败", error))
}

async fn sync_auth_preset_configs(
    auth_targets: &[TerminalAuthFiles],
    config_targets: &[ResolvedConfigTarget],
) -> ApiResult<()> {
    for target in auth_targets {
        sync_auth_preset_config(&target.config_file, config_targets).await?;
    }
    Ok(())
}

async fn sync_api_preset_config(
    path: &std::path::Path,
    provider_name: &str,
    base_url: &str,
    provider_options: &ApiProviderOptions,
    targets: &[ResolvedConfigTarget],
    managed_keys: &[String],
) -> ApiResult<()> {
    auth_core::sync_api_preset_config(
        path,
        provider_name,
        base_url,
        provider_options,
        targets,
        managed_keys,
    )
    .await
    .map_err(|error| map_internal("更新 API 预设 config 失败", error))
}

async fn sync_api_preset_configs(
    auth_targets: &[TerminalAuthFiles],
    provider_name: &str,
    base_url: &str,
    provider_options: &ApiProviderOptions,
    config_targets: &[ResolvedConfigTarget],
    managed_keys: &[String],
) -> ApiResult<()> {
    for target in auth_targets {
        sync_api_preset_config(
            &target.config_file,
            provider_name,
            base_url,
            provider_options,
            config_targets,
            managed_keys,
        )
        .await?;
    }
    Ok(())
}

fn api_managed_config_keys(defaults: &[(&str, &str)], presets: &[StoredApiPreset]) -> Vec<String> {
    let mut keys = Vec::new();
    for (key, _) in defaults {
        push_managed_config_key(&mut keys, key);
    }
    for preset in presets {
        for config_override in &preset.config_overrides {
            if let Some(key) = config_override.key.as_deref() {
                push_managed_config_key(&mut keys, key);
            }
        }
    }
    keys
}

fn push_managed_config_key(keys: &mut Vec<String>, key: &str) {
    let key = key.trim();
    if key.is_empty()
        || keys
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(key))
    {
        return;
    }
    keys.push(key.to_string());
}

async fn sync_api_model_catalog(
    path: &std::path::Path,
    targets: &[ResolvedConfigTarget],
    bundled_catalog: Option<&Value>,
) -> ApiResult<()> {
    auth_core::sync_api_model_catalog(path, targets, bundled_catalog)
        .await
        .map_err(|error| map_internal("更新 Codex 模型 metadata 失败", error))
}

async fn sync_api_model_catalogs(
    auth_targets: &[TerminalAuthFiles],
    config_targets: &[ResolvedConfigTarget],
) -> ApiResult<()> {
    if !config_targets
        .iter()
        .any(|target| target.key.eq_ignore_ascii_case("model") && !target.value.trim().is_empty())
    {
        return Ok(());
    }

    for target in auth_targets {
        let (bundled_catalog, bundled_error) =
            match read_codex_bundled_model_catalog(&target.user_name).await {
                Ok(catalog) => (Some(catalog), None),
                Err(error) => {
                    warn!("读取用户 `{}` 的 Codex bundled 模型目录失败: {error}", target.user_name);
                    (None, Some(error))
                }
            };
        if let Err(sync_error) =
            sync_api_model_catalog(&target.config_file, config_targets, bundled_catalog.as_ref())
                .await
        {
            if let Some(bundled_error) = bundled_error {
                return Err(AppError::internal(format!(
                    "{}；读取用户 `{}` 的 Codex bundled 模型目录失败: {bundled_error}",
                    sync_error, target.user_name
                )));
            }
            return Err(sync_error);
        }
    }
    Ok(())
}

async fn read_codex_bundled_model_catalog(user_name: &str) -> Result<Value> {
    let user = runtime_paths::resolve_user_profile(user_name)
        .with_context(|| format!("无法解析终端用户 `{user_name}`"))?;
    let command_path = codex_command_path_for_user(&user);
    let codex = resolve_codex_executable(&command_path);
    let current_user = runtime_paths::resolve_current_user_profile();
    let mut command = if current_user
        .as_ref()
        .is_some_and(|current| current.uid == user.uid)
    {
        Command::new(&codex)
    } else {
        #[cfg(unix)]
        {
            let mut command = Command::new("runuser");
            command
                .arg("-u")
                .arg(&user.name)
                .arg("--preserve-environment")
                .arg("--")
                .arg(&codex);
            command
        }
        #[cfg(not(unix))]
        {
            anyhow::bail!("当前平台不支持读取其他用户的 Codex bundled 模型目录");
        }
    };
    command
        .args(["debug", "models", "--bundled"])
        .env_clear()
        .env("HOME", &user.home)
        .env("USER", &user.name)
        .env("LOGNAME", &user.name)
        .env("SHELL", &user.shell)
        .env("PATH", &command_path)
        .env("TERM", "dumb")
        .kill_on_drop(true);

    let output = tokio::time::timeout(CODEX_BUNDLED_MODELS_TIMEOUT, command.output())
        .await
        .with_context(|| {
            format!("用户 `{}` 的 `codex debug models --bundled` 执行超时", user.name)
        })?
        .with_context(|| {
            format!("无法执行用户 `{}` 的 `codex debug models --bundled`", user.name)
        })?;
    if !output.status.success() {
        let stderr = command_error_summary(&output.stderr);
        anyhow::bail!("用户 `{}` 的 `codex debug models --bundled` 失败: {stderr}", user.name);
    }
    parse_codex_bundled_model_catalog(&output.stdout)
}

pub(crate) fn codex_command_path_for_user(user: &runtime_paths::UserProfile) -> OsString {
    let inherited_path = crate::shell_env::read_user_shell_env(user)
        .ok()
        .and_then(|snapshot| {
            snapshot
                .entries
                .into_iter()
                .find(|(key, _)| key.eq_ignore_ascii_case("PATH"))
                .map(|(_, value)| OsString::from(value))
        })
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_else(|| OsString::from(DEFAULT_CODEX_COMMAND_PATH));
    let mut paths = vec![user.home.join(".local/bin"), user.home.join("bin")];
    paths.extend(std::env::split_paths(&inherited_path));
    std::env::join_paths(paths).unwrap_or(inherited_path)
}

pub(crate) fn resolve_codex_executable(command_path: &OsStr) -> PathBuf {
    for directory in std::env::split_paths(command_path) {
        for file_name in codex_executable_file_names() {
            let candidate = directory.join(file_name);
            if candidate.is_file() {
                return candidate;
            }
        }
    }
    PathBuf::from("codex")
}

#[cfg(not(windows))]
fn codex_executable_file_names() -> &'static [&'static str] {
    &["codex"]
}

#[cfg(windows)]
fn codex_executable_file_names() -> &'static [&'static str] {
    &["codex.exe", "codex.cmd", "codex.bat", "codex"]
}

fn parse_codex_bundled_model_catalog(output: &[u8]) -> Result<Value> {
    let catalog: Value =
        serde_json::from_slice(output).context("Codex bundled 模型目录不是有效 JSON")?;
    if !catalog
        .get("models")
        .is_some_and(|models| models.is_array())
    {
        anyhow::bail!("Codex bundled 模型目录顶层缺少 models 数组");
    }
    Ok(catalog)
}

fn command_error_summary(stderr: &[u8]) -> String {
    let summary = String::from_utf8_lossy(stderr).trim().to_string();
    if summary.is_empty() {
        return "无 stderr 输出".to_string();
    }
    let mut chars = summary.chars();
    let shortened = chars.by_ref().take(500).collect::<String>();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}

async fn refresh_stored_auth_preset_quota(
    proxy_manager: &crate::proxy::ProxyManager,
    preset: &mut StoredAuthPreset,
) -> ApiResult<()> {
    auth_core::refresh_stored_auth_preset_quota(proxy_manager, preset)
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))
}

pub async fn list_auth_presets(
    State(state): State<AppState>,
) -> ApiResult<Json<AuthPresetListResponse>> {
    let auth_files = terminal_auth_files(&state)?;
    let presets = state.auth_manager.auth_presets_snapshot();
    let api_presets = state.auth_manager.api_presets_snapshot();
    let (
        current_auth_value,
        current_auth_error,
        _current_config_value,
        current_config_error,
        current_mode,
        current_api,
    ) = read_codex_current_state(&auth_files.auth_file, &auth_files.config_file, &api_presets)
        .await?;
    let current_login_auth = current_auth_value
        .as_ref()
        .and_then(CurrentAuthState::as_login);

    Ok(Json(AuthPresetListResponse {
        auth_file: auth_files.auth_file.display().to_string(),
        config_file: auth_files.config_file.display().to_string(),
        preset_file: state.auth_manager.preset_file().display().to_string(),
        current_mode,
        current_auth: current_login_auth.map(auth_summary),
        current_api: current_api.as_ref().map(current_api_summary),
        current_auth_error,
        current_config_error,
        upstream_proxy: state.auth_manager.upstream_proxy_settings(),
        presets: presets
            .iter()
            .map(|preset| preset_summary(preset, current_login_auth))
            .collect(),
    }))
}

pub async fn reorder_auth_presets(
    State(state): State<AppState>,
    Json(payload): Json<ReorderPresetsRequest>,
) -> ApiResult<Json<ReorderPresetsResponse>> {
    let presets = state.auth_manager.auth_presets_snapshot();
    let reordered =
        reorder_presets_by_ids(presets, &payload.ids, |preset| preset.id.as_str(), "auth")?;
    persist_auth_presets_async(&state.auth_manager, &reordered).await?;

    Ok(Json(ReorderPresetsResponse {
        ok: true,
        ids: payload.ids,
    }))
}

pub async fn list_api_presets(
    State(state): State<AppState>,
) -> ApiResult<Json<ApiPresetListResponse>> {
    let auth_files = terminal_auth_files(&state)?;
    let presets = state.auth_manager.api_presets_snapshot();
    let (
        current_auth_value,
        current_auth_error,
        _current_config_value,
        current_config_error,
        current_mode,
        current_api,
    ) = read_codex_current_state(&auth_files.auth_file, &auth_files.config_file, &presets).await?;
    let current_login_auth = current_auth_value
        .as_ref()
        .and_then(CurrentAuthState::as_login);
    let upstream_proxy = state.auth_manager.upstream_proxy_settings();

    Ok(Json(ApiPresetListResponse {
        auth_file: auth_files.auth_file.display().to_string(),
        config_file: auth_files.config_file.display().to_string(),
        preset_file: state.auth_manager.api_preset_file().display().to_string(),
        current_mode,
        current_auth: current_login_auth.map(auth_summary),
        current_api: current_api.as_ref().map(current_api_summary),
        current_auth_error,
        current_config_error,
        upstream_proxy: upstream_proxy.clone(),
        presets: presets
            .iter()
            .map(|preset| {
                api_preset_summary_with_proxy_state(
                    preset,
                    current_mode,
                    current_api.as_ref(),
                    &upstream_proxy,
                )
            })
            .collect(),
    }))
}

pub async fn reorder_api_presets(
    State(state): State<AppState>,
    Json(payload): Json<ReorderPresetsRequest>,
) -> ApiResult<Json<ReorderPresetsResponse>> {
    let presets = state.auth_manager.api_presets_snapshot();
    let reordered =
        reorder_presets_by_ids(presets, &payload.ids, |preset| preset.id.as_str(), "API")?;
    persist_api_presets_async(&state.auth_manager, &reordered).await?;

    Ok(Json(ReorderPresetsResponse {
        ok: true,
        ids: payload.ids,
    }))
}

pub async fn list_claude_presets(
    State(state): State<AppState>,
) -> ApiResult<Json<ClaudePresetListResponse>> {
    let auth_files = terminal_auth_files(&state)?;
    let presets = state.auth_manager.claude_presets_snapshot();
    let effective_presets = presets
        .iter()
        .map(|preset| claude_preset_with_global_defaults(&state.workspace_settings, preset))
        .collect::<ApiResult<Vec<_>>>()?;
    let current_claude =
        read_current_claude_state(&auth_files.claude_settings_file, &effective_presets).await;
    let (current_claude_value, current_settings_error) = match current_claude {
        Ok(value) => (value, None),
        Err(error) => (None, Some(error)),
    };
    let upstream_proxy = state.auth_manager.upstream_proxy_settings();

    Ok(Json(ClaudePresetListResponse {
        settings_file: auth_files.claude_settings_file.display().to_string(),
        preset_file: state
            .auth_manager
            .claude_preset_file()
            .display()
            .to_string(),
        current_claude: current_claude_value.as_ref().map(current_claude_summary),
        current_settings_error,
        upstream_proxy: upstream_proxy.clone(),
        presets: presets
            .iter()
            .zip(&effective_presets)
            .map(|(preset, effective_preset)| {
                claude_preset_summary_with_effective_proxy_state(
                    preset,
                    effective_preset,
                    current_claude_value.as_ref(),
                    &upstream_proxy,
                )
            })
            .collect(),
    }))
}

pub async fn reorder_claude_presets(
    State(state): State<AppState>,
    Json(payload): Json<ReorderPresetsRequest>,
) -> ApiResult<Json<ReorderPresetsResponse>> {
    let presets = state.auth_manager.claude_presets_snapshot();
    let reordered =
        reorder_presets_by_ids(presets, &payload.ids, |preset| preset.id.as_str(), "Claude")?;
    persist_claude_presets_async(&state.auth_manager, &reordered).await?;

    Ok(Json(ReorderPresetsResponse {
        ok: true,
        ids: payload.ids,
    }))
}

pub async fn update_upstream_proxy_settings(
    State(state): State<AppState>,
    Json(payload): Json<UpdateUpstreamProxySettingsRequest>,
) -> ApiResult<Json<UpstreamProxySettingsResponse>> {
    let _active_config_guard = apply::lock_active_config_for_request(&state).await;
    let mut settings = state.auth_manager.upstream_proxy_settings();
    if let Some(enabled) = payload.codex_api_proxy_enabled {
        settings.codex_api_proxy_enabled = enabled;
    }
    if let Some(enabled) = payload.claude_proxy_enabled {
        settings.claude_proxy_enabled = enabled;
    }
    persist_upstream_proxy_settings(&state.auth_manager, settings.clone())
        .map_err(|error| map_internal("保存上游代理设置失败", error))?;

    Ok(Json(UpstreamProxySettingsResponse {
        ok: true,
        upstream_proxy: settings,
    }))
}

pub async fn save_auth_preset(
    State(state): State<AppState>,
    Json(payload): Json<SaveAuthPresetRequest>,
) -> ApiResult<Json<SaveAuthPresetResponse>> {
    let auth_files = terminal_auth_files(&state)?;
    validate_auth_file(&payload.auth)?;
    let config_overrides = resolve_config_overrides(
        payload.config_overrides,
        payload.config_key,
        payload.config_value,
        payload.secondary_config_key,
        payload.secondary_config_value,
        ConfigOverrideKind::Codex,
        "auth",
    )?;

    let mut presets = state.auth_manager.auth_presets_snapshot();
    let preset = StoredAuthPreset {
        id: generate_preset_id("auth"),
        name: resolve_auth_preset_name(&payload.name, &payload.auth, &presets, None),
        saved_at: current_timestamp_secs(),
        details: sanitize_auth_preset_details(payload.details),
        config_overrides,
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth: payload.auth,
        switch_count: 0,
    };
    presets.insert(0, preset.clone());

    persist_auth_presets_async(&state.auth_manager, &presets).await?;

    let current_auth = read_current_auth_state(&auth_files.auth_file)
        .await
        .ok()
        .flatten();

    Ok(Json(SaveAuthPresetResponse {
        ok: true,
        preset: preset_summary(&preset, current_auth.as_ref().and_then(CurrentAuthState::as_login)),
    }))
}

pub async fn update_auth_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
    Json(payload): Json<SaveAuthPresetRequest>,
) -> ApiResult<Json<SaveAuthPresetResponse>> {
    let auth_files = terminal_auth_files(&state)?;
    validate_auth_file(&payload.auth)?;
    let config_overrides = resolve_config_overrides(
        payload.config_overrides,
        payload.config_key,
        payload.config_value,
        payload.secondary_config_key,
        payload.secondary_config_value,
        ConfigOverrideKind::Codex,
        "auth",
    )?;

    let mut presets = state.auth_manager.auth_presets_snapshot();
    let Some(index) = presets.iter().position(|preset| preset.id == preset_id) else {
        return Err(AppError::not_found("找不到指定的 auth 预设。"));
    };

    let name = resolve_auth_preset_name(&payload.name, &payload.auth, &presets, Some(&preset_id));
    let preset = StoredAuthPreset {
        id: preset_id,
        name,
        saved_at: current_timestamp_secs(),
        details: sanitize_auth_preset_details(payload.details),
        config_overrides,
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth: payload.auth,
        switch_count: 0,
    };
    presets[index] = preset.clone();

    persist_auth_presets_async(&state.auth_manager, &presets).await?;

    let current_auth = read_current_auth_state(&auth_files.auth_file)
        .await
        .ok()
        .flatten();

    Ok(Json(SaveAuthPresetResponse {
        ok: true,
        preset: preset_summary(&preset, current_auth.as_ref().and_then(CurrentAuthState::as_login)),
    }))
}

pub async fn save_api_preset(
    State(state): State<AppState>,
    Json(payload): Json<SaveApiPresetRequest>,
) -> ApiResult<Json<SaveApiPresetResponse>> {
    let auth_files = terminal_auth_files(&state)?;
    let base_url = sanitize_base_url(payload.base_url)
        .map_err(|error| AppError::bad_request(format!("API 预设无效: {error}")))?;
    let provider_name = sanitize_api_provider_name(payload.provider_name, &base_url);
    let management_url = resolve_api_management_url(
        payload.management_url,
        payload.management_url_same_as_base,
        &base_url,
    )
    .map_err(|error| AppError::bad_request(format!("API 预设无效: {error}")))?;
    let api_key = sanitize_api_key(payload.api_key)
        .map_err(|error| AppError::bad_request(format!("API 预设无效: {error}")))?;
    let wire_api = sanitize_api_wire_api(payload.wire_api)
        .map_err(|error| AppError::bad_request(format!("API 预设无效: {error}")))?;
    let responses_proxy = payload.responses_proxy;
    let apply_upstream_proxy_on_switch = payload.apply_upstream_proxy_on_switch;
    let terminal_env = sanitize_terminal_env_vars(payload.terminal_env);
    let terminal_startup_script = sanitize_terminal_startup_script(payload.terminal_startup_script);
    let config_overrides = resolve_config_overrides(
        payload.config_overrides,
        payload.config_key,
        payload.config_value,
        payload.secondary_config_key,
        payload.secondary_config_value,
        ConfigOverrideKind::Codex,
        "API",
    )?;

    let mut presets = state.auth_manager.api_presets_snapshot();
    let preset = StoredApiPreset {
        id: generate_preset_id("api"),
        name: resolve_api_preset_name(&payload.name, &base_url, &presets, None),
        saved_at: current_timestamp_secs(),
        provider_name,
        base_url,
        management_url,
        wire_api,
        responses_proxy,
        apply_upstream_proxy_on_switch,
        terminal_env,
        terminal_startup_script,
        config_overrides,
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        api_key,
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    };
    presets.insert(0, preset.clone());

    persist_api_presets_async(&state.auth_manager, &presets).await?;

    let current_auth = read_current_auth_state(&auth_files.auth_file)
        .await
        .ok()
        .flatten();
    let current_config = read_current_config_provider(&auth_files.config_file)
        .await
        .ok()
        .flatten();
    let current_mode = derive_current_mode(current_auth.as_ref(), current_config.as_ref());
    let current_api =
        derive_current_api_state(current_config.as_ref(), current_auth.as_ref(), &presets);

    Ok(Json(SaveApiPresetResponse {
        ok: true,
        preset: api_preset_summary(&preset, current_mode, current_api.as_ref()),
    }))
}

/// Import account JSON (sub2api bundle, CPA array, or single object) and save each
/// as a Codex_API preset with OAuth access_token + account_id for ChatGPT backend proxy.
pub async fn import_api_accounts(
    State(state): State<AppState>,
    Json(payload): Json<ImportApiAccountsRequest>,
) -> ApiResult<Json<ImportApiAccountsResponse>> {
    let parsed = parse_imported_accounts(&payload.raw_text)
        .map_err(|error| AppError::bad_request(error.to_string()))?;

    Ok(Json(save_imported_api_accounts(&state, parsed, Vec::new()).await?))
}

pub async fn import_api_accounts_file(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> ApiResult<Json<ImportApiAccountsResponse>> {
    let mut uploads = Vec::new();
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::bad_request(format!("读取导入文件失败: {error}")))?
    {
        if field.name() != Some("file") {
            continue;
        }
        let fallback_name = format!("accounts-{}.json", uploads.len() + 1);
        let file_name = field.file_name().unwrap_or(&fallback_name).to_string();
        let bytes = field
            .bytes()
            .await
            .map_err(|error| AppError::bad_request(format!("读取导入文件失败: {error}")))?;
        uploads.push((file_name, bytes));
    }

    let imported = collect_accounts_from_uploads(
        uploads
            .iter()
            .map(|(file_name, bytes)| (file_name.as_str(), bytes.as_ref())),
    )
    .map_err(|error| AppError::bad_request(error.to_string()))?;

    Ok(Json(
        save_imported_api_accounts(&state, imported.accounts, imported.errors).await?,
    ))
}

async fn save_imported_api_accounts(
    state: &AppState,
    parsed: Vec<ImportedAccount>,
    errors: Vec<String>,
) -> ApiResult<ImportApiAccountsResponse> {
    let mut presets = state.auth_manager.api_presets_snapshot();
    let base_url = "https://chatgpt.com/backend-api/codex".to_string();
    let provider_name = "ChatGPT".to_string();

    let mut saved_names = Vec::new();

    for account in &parsed {
        let preset_name = if account.name.is_empty() {
            account.email.clone()
        } else {
            account.name.clone()
        };

        // Use a local proxy placeholder as api_key identity; the real credential
        // is the access_token which the proxy injects from the preset.
        let preset_id = unique_import_preset_id(
            &generate_preset_id("api"),
            presets.iter().map(|preset| preset.id.as_str()),
        );
        let placeholder_api_key = local_proxy_api_key_for_preset_id(&preset_id);

        let resolved_name = resolve_api_preset_name(&preset_name, &base_url, &presets, None);
        let preset = StoredApiPreset {
            id: preset_id,
            name: resolved_name.clone(),
            saved_at: current_timestamp_secs(),
            provider_name: provider_name.clone(),
            base_url: base_url.clone(),
            management_url: None,
            wire_api: Some("responses".to_string()),
            responses_proxy: None,
            apply_upstream_proxy_on_switch: true,
            terminal_env: Vec::new(),
            terminal_startup_script: None,
            config_overrides: Vec::new(),
            legacy_config_key: None,
            legacy_config_value: None,
            legacy_secondary_config_key: None,
            legacy_secondary_config_value: None,
            api_key: placeholder_api_key,
            access_token: account.access_token.clone(),
            account_id: account.account_id.clone(),
            access_mode: Some(ApiAccessMode::ChatgptOauth),
            switch_count: 0,
        };
        presets.insert(0, preset);
        saved_names.push(resolved_name);
    }

    let saved_count = saved_names.len();
    persist_api_presets_async(&state.auth_manager, &presets).await?;

    Ok(ImportApiAccountsResponse {
        ok: saved_count > 0,
        saved_count,
        saved_names,
        errors,
    })
}

fn unique_import_preset_id<'a>(
    candidate: &str,
    existing_ids: impl Iterator<Item = &'a str>,
) -> String {
    let existing = existing_ids.collect::<HashSet<_>>();
    if !existing.contains(candidate) {
        return candidate.to_string();
    }

    for suffix in 2_u64.. {
        let suffixed = format!("{candidate}-{suffix}");
        if !existing.contains(suffixed.as_str()) {
            return suffixed;
        }
    }
    unreachable!("u64 suffix space is not exhaustible")
}

pub async fn save_claude_preset(
    State(state): State<AppState>,
    Json(payload): Json<SaveClaudePresetRequest>,
) -> ApiResult<Json<SaveClaudePresetResponse>> {
    let auth_files = terminal_auth_files(&state)?;
    let base_url = sanitize_base_url(payload.base_url)
        .map_err(|error| AppError::bad_request(format!("Claude 预设无效: {error}")))?;
    let provider_name = sanitize_api_provider_name(payload.provider_name, &base_url);
    let management_url = sanitize_management_url(payload.management_url)
        .map_err(|error| AppError::bad_request(format!("Claude 预设无效: {error}")))?;
    let auth_token = sanitize_auth_token(payload.auth_token)
        .map_err(|error| AppError::bad_request(format!("Claude 预设无效: {error}")))?;
    let default_haiku_model = sanitize_claude_model(payload.default_haiku_model);
    let default_sonnet_model = sanitize_claude_model(payload.default_sonnet_model);
    let default_opus_model = sanitize_claude_model(payload.default_opus_model);
    let third_party_model = sanitize_claude_model(payload.third_party_model);
    let config_overrides = resolve_config_overrides(
        payload.config_overrides,
        payload.config_key,
        payload.config_value,
        payload.secondary_config_key,
        payload.secondary_config_value,
        ConfigOverrideKind::Claude,
        "Claude",
    )?;
    validate_claude_model_selection(
        default_haiku_model.as_deref(),
        default_sonnet_model.as_deref(),
        default_opus_model.as_deref(),
        third_party_model.as_deref(),
    )
    .map_err(|error| AppError::bad_request(format!("Claude 预设无效: {error}")))?;
    let access_mode = payload.access_mode.unwrap_or({
        if payload.use_local_proxy {
            ClaudeAccessMode::AnthropicProxy
        } else {
            ClaudeAccessMode::Direct
        }
    });
    let use_local_proxy = matches!(
        access_mode,
        ClaudeAccessMode::AnthropicProxy
            | ClaudeAccessMode::AnthropicRelay
            | ClaudeAccessMode::OpenaiChat
            | ClaudeAccessMode::OpenaiResponses
    );

    let mut presets = state.auth_manager.claude_presets_snapshot();
    let preset = StoredClaudePreset {
        id: generate_preset_id("claude"),
        name: resolve_claude_preset_name(&payload.name, &base_url, &presets, None),
        saved_at: current_timestamp_secs(),
        provider_name,
        base_url,
        management_url,
        config_overrides,
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token,
        default_haiku_model,
        default_sonnet_model,
        default_opus_model,
        third_party_model,
        use_local_proxy,
        access_mode: Some(access_mode),
        switch_count: 0,
    };
    presets.insert(0, preset.clone());

    persist_claude_presets_async(&state.auth_manager, &presets).await?;

    let current_claude = read_current_claude_state(&auth_files.claude_settings_file, &presets)
        .await
        .ok()
        .flatten();

    Ok(Json(SaveClaudePresetResponse {
        ok: true,
        preset: claude_preset_summary(&preset, current_claude.as_ref()),
    }))
}

pub async fn update_api_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
    Json(payload): Json<SaveApiPresetRequest>,
) -> ApiResult<Json<SaveApiPresetResponse>> {
    let _active_config_guard = apply::lock_active_config_for_request(&state).await;
    let auth_files = terminal_auth_files(&state)?;
    let base_url = sanitize_base_url(payload.base_url)
        .map_err(|error| AppError::bad_request(format!("API 预设无效: {error}")))?;
    let provider_name = sanitize_api_provider_name(payload.provider_name, &base_url);
    let management_url = resolve_api_management_url(
        payload.management_url,
        payload.management_url_same_as_base,
        &base_url,
    )
    .map_err(|error| AppError::bad_request(format!("API 预设无效: {error}")))?;
    let api_key = sanitize_api_key(payload.api_key)
        .map_err(|error| AppError::bad_request(format!("API 预设无效: {error}")))?;
    let wire_api = sanitize_api_wire_api(payload.wire_api)
        .map_err(|error| AppError::bad_request(format!("API 预设无效: {error}")))?;
    let responses_proxy = payload.responses_proxy;
    let apply_upstream_proxy_on_switch = payload.apply_upstream_proxy_on_switch;
    let terminal_env = sanitize_terminal_env_vars(payload.terminal_env);
    let terminal_startup_script = sanitize_terminal_startup_script(payload.terminal_startup_script);
    let config_overrides = resolve_config_overrides(
        payload.config_overrides,
        payload.config_key,
        payload.config_value,
        payload.secondary_config_key,
        payload.secondary_config_value,
        ConfigOverrideKind::Codex,
        "API",
    )?;

    let mut presets = state.auth_manager.api_presets_snapshot();
    let Some(index) = presets.iter().position(|preset| preset.id == preset_id) else {
        return Err(AppError::not_found("找不到指定的 API 预设。"));
    };
    let previous_managed_config_keys = api_managed_config_keys(&[], &presets);
    let current_auth = read_current_auth_state(&auth_files.auth_file)
        .await
        .ok()
        .flatten();
    let current_config = read_current_config_provider(&auth_files.config_file)
        .await
        .ok()
        .flatten();
    let current_mode = derive_current_mode(current_auth.as_ref(), current_config.as_ref());
    let current_api =
        derive_current_api_state(current_config.as_ref(), current_auth.as_ref(), &presets);
    let upstream_proxy = state.auth_manager.upstream_proxy_settings();
    let was_active =
        api_preset_matches_current_api(&presets[index], current_mode, current_api.as_ref());

    let previous = presets[index].clone();
    let previous_access_mode = previous.access_mode.unwrap_or(ApiAccessMode::Direct);
    if payload
        .access_mode
        .is_some_and(|mode| mode != previous_access_mode)
    {
        return Err(AppError::bad_request(
            "不能通过普通编辑切换 API 预设访问模式。OAuth 代理账号请使用导入功能。",
        ));
    }
    let is_chatgpt_oauth = previous_access_mode == ApiAccessMode::ChatgptOauth;
    let effective_base_url = if is_chatgpt_oauth {
        previous.base_url.clone()
    } else {
        base_url
    };
    let name =
        resolve_api_preset_name(&payload.name, &effective_base_url, &presets, Some(&preset_id));
    let preset = StoredApiPreset {
        id: preset_id,
        name,
        saved_at: current_timestamp_secs(),
        provider_name: if is_chatgpt_oauth {
            previous.provider_name
        } else {
            provider_name
        },
        base_url: effective_base_url,
        management_url,
        wire_api: if is_chatgpt_oauth {
            previous.wire_api
        } else {
            wire_api
        },
        responses_proxy: if is_chatgpt_oauth {
            previous.responses_proxy
        } else {
            responses_proxy
        },
        apply_upstream_proxy_on_switch: if is_chatgpt_oauth {
            true
        } else {
            apply_upstream_proxy_on_switch
        },
        terminal_env,
        terminal_startup_script,
        config_overrides,
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        api_key: if is_chatgpt_oauth {
            previous.api_key
        } else {
            api_key
        },
        access_token: if is_chatgpt_oauth {
            previous.access_token
        } else {
            String::new()
        },
        account_id: if is_chatgpt_oauth {
            previous.account_id
        } else {
            String::new()
        },
        access_mode: Some(previous_access_mode),
        switch_count: 0,
    };
    presets[index] = preset.clone();

    persist_api_presets_async(&state.auth_manager, &presets).await?;

    let refreshed_current_auth = if was_active {
        let auth_targets = terminal_auth_write_targets(&state)?;
        let use_local_proxy = api_preset_enables_local_upstream_proxy_on_apply(&preset);
        let openai_api_key = if use_local_proxy {
            local_proxy_api_key_for_preset_id(&preset.id)
        } else {
            preset.api_key.clone()
        };

        write_api_auth_files(
            &auth_targets,
            &ApiAuthFile {
                openai_api_key: openai_api_key.clone(),
            },
        )
        .await?;

        let default_config_entries = state.workspace_settings.codex_default_config_entries();
        let default_config_pairs = default_config_entries
            .iter()
            .map(|entry| (entry.key.as_str(), entry.value.as_str()))
            .collect::<Vec<_>>();
        let config_targets = resolve_effective_preset_config_targets(
            &default_config_pairs,
            &preset.config_overrides,
        )
        .map_err(|error| AppError::internal(format!("API 预设 config 覆盖无效: {error}")))?;
        let mut managed_config_keys = api_managed_config_keys(&default_config_pairs, &presets);
        for key in &previous_managed_config_keys {
            push_managed_config_key(&mut managed_config_keys, key);
        }
        sync_api_preset_configs(
            &auth_targets,
            &preset.provider_name,
            &api_provider_base_url_for_mode(&preset, use_local_proxy),
            &api_provider_options(&preset),
            &config_targets,
            &managed_config_keys,
        )
        .await?;
        sync_api_model_catalogs(&auth_targets, &config_targets).await?;

        Some(CurrentAuthState::Api(ApiAuthFile { openai_api_key }))
    } else {
        current_auth
    };

    let refreshed_current_config = if was_active {
        read_current_config_provider(&auth_files.config_file)
            .await
            .ok()
            .flatten()
    } else {
        current_config
    };
    let refreshed_current_mode =
        derive_current_mode(refreshed_current_auth.as_ref(), refreshed_current_config.as_ref());
    let refreshed_current_api = derive_current_api_state(
        refreshed_current_config.as_ref(),
        refreshed_current_auth.as_ref(),
        &presets,
    );

    Ok(Json(SaveApiPresetResponse {
        ok: true,
        preset: api_preset_summary_with_proxy_state(
            &preset,
            refreshed_current_mode,
            refreshed_current_api.as_ref(),
            &upstream_proxy,
        ),
    }))
}

pub async fn update_claude_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
    Json(payload): Json<SaveClaudePresetRequest>,
) -> ApiResult<Json<SaveClaudePresetResponse>> {
    let _active_config_guard = apply::lock_active_config_for_request(&state).await;
    let auth_files = terminal_auth_files(&state)?;
    let auth_targets = terminal_auth_write_targets(&state)?;
    let base_url = sanitize_base_url(payload.base_url)
        .map_err(|error| AppError::bad_request(format!("Claude 预设无效: {error}")))?;
    let provider_name = sanitize_api_provider_name(payload.provider_name, &base_url);
    let management_url = sanitize_management_url(payload.management_url)
        .map_err(|error| AppError::bad_request(format!("Claude 预设无效: {error}")))?;
    let auth_token = sanitize_auth_token(payload.auth_token)
        .map_err(|error| AppError::bad_request(format!("Claude 预设无效: {error}")))?;
    let default_haiku_model = sanitize_claude_model(payload.default_haiku_model);
    let default_sonnet_model = sanitize_claude_model(payload.default_sonnet_model);
    let default_opus_model = sanitize_claude_model(payload.default_opus_model);
    let third_party_model = sanitize_claude_model(payload.third_party_model);
    let config_overrides = resolve_config_overrides(
        payload.config_overrides,
        payload.config_key,
        payload.config_value,
        payload.secondary_config_key,
        payload.secondary_config_value,
        ConfigOverrideKind::Claude,
        "Claude",
    )?;
    validate_claude_model_selection(
        default_haiku_model.as_deref(),
        default_sonnet_model.as_deref(),
        default_opus_model.as_deref(),
        third_party_model.as_deref(),
    )
    .map_err(|error| AppError::bad_request(format!("Claude 预设无效: {error}")))?;
    let access_mode = payload.access_mode.unwrap_or({
        if payload.use_local_proxy {
            ClaudeAccessMode::AnthropicProxy
        } else {
            ClaudeAccessMode::Direct
        }
    });
    let use_local_proxy = matches!(
        access_mode,
        ClaudeAccessMode::AnthropicProxy
            | ClaudeAccessMode::AnthropicRelay
            | ClaudeAccessMode::OpenaiChat
            | ClaudeAccessMode::OpenaiResponses
    );

    let mut presets = state.auth_manager.claude_presets_snapshot();
    let Some(index) = presets.iter().position(|preset| preset.id == preset_id) else {
        return Err(AppError::not_found("找不到指定的 Claude 预设。"));
    };
    let effective_presets_before_update = presets
        .iter()
        .map(|preset| claude_preset_with_global_defaults(&state.workspace_settings, preset))
        .collect::<ApiResult<Vec<_>>>()?;
    let current_claude_before_update = read_current_claude_state(
        &auth_files.claude_settings_file,
        &effective_presets_before_update,
    )
    .await
    .ok()
    .flatten();
    let upstream_proxy_settings_before_update = state.auth_manager.upstream_proxy_settings();
    let was_active = current_claude_before_update
        .as_ref()
        .is_some_and(|current| {
            claude_preset_summary_with_effective_proxy_state(
                &presets[index],
                &effective_presets_before_update[index],
                Some(current),
                &upstream_proxy_settings_before_update,
            )
            .active
        });

    let name = resolve_claude_preset_name(&payload.name, &base_url, &presets, Some(&preset_id));
    let preset = StoredClaudePreset {
        id: preset_id,
        name,
        saved_at: current_timestamp_secs(),
        provider_name,
        base_url,
        management_url,
        config_overrides,
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token,
        default_haiku_model,
        default_sonnet_model,
        default_opus_model,
        third_party_model,
        use_local_proxy,
        access_mode: Some(access_mode),
        switch_count: 0,
    };
    presets[index] = preset.clone();

    persist_claude_presets_async(&state.auth_manager, &presets).await?;

    if was_active {
        let effective_preset =
            claude_preset_with_global_defaults(&state.workspace_settings, &preset)?;
        activate_dynamic_claude_relay_if_needed(&state, &preset)?;
        write_claude_preset_to_targets(&auth_targets, &effective_preset).await?;
    }

    let effective_presets = presets
        .iter()
        .map(|preset| claude_preset_with_global_defaults(&state.workspace_settings, preset))
        .collect::<ApiResult<Vec<_>>>()?;
    let current_claude =
        read_current_claude_state(&auth_files.claude_settings_file, &effective_presets)
            .await
            .ok()
            .flatten();

    Ok(Json(SaveClaudePresetResponse {
        ok: true,
        preset: claude_preset_summary(&preset, current_claude.as_ref()),
    }))
}

pub async fn refresh_auth_preset_quota(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
) -> ApiResult<Json<SaveAuthPresetResponse>> {
    let _active_config_guard = apply::lock_active_config_for_request(&state).await;
    let auth_files = terminal_auth_files(&state)?;
    let auth_targets = terminal_auth_write_targets(&state)?;
    let mut presets = state.auth_manager.auth_presets_snapshot();
    let Some(index) = presets.iter().position(|preset| preset.id == preset_id) else {
        return Err(AppError::not_found("找不到指定的 auth 预设。"));
    };

    let current_auth = read_current_auth_state(&auth_files.auth_file)
        .await
        .ok()
        .flatten();
    let was_active =
        current_auth.as_ref().and_then(CurrentAuthState::as_login) == Some(&presets[index].auth);

    let original_auth = presets[index].auth.clone();
    if let Err(error) =
        refresh_stored_auth_preset_quota(&state.proxy_manager, &mut presets[index]).await
    {
        if presets[index].auth != original_auth {
            persist_auth_presets_async(&state.auth_manager, &presets).await?;
            if was_active {
                write_login_auth_files(&auth_targets, &presets[index].auth).await?;
            }
        }
        return Err(error);
    }
    let preset = presets[index].clone();

    persist_auth_presets_async(&state.auth_manager, &presets).await?;

    if was_active {
        write_login_auth_files(&auth_targets, &preset.auth).await?;
    }

    let refreshed_current_auth = if was_active {
        Some(CurrentAuthState::Login(preset.auth.clone()))
    } else {
        read_current_auth_state(&auth_files.auth_file)
            .await
            .ok()
            .flatten()
    };

    Ok(Json(SaveAuthPresetResponse {
        ok: true,
        preset: preset_summary(
            &preset,
            refreshed_current_auth
                .as_ref()
                .and_then(CurrentAuthState::as_login),
        ),
    }))
}

pub async fn refresh_all_auth_preset_quotas(
    State(state): State<AppState>,
) -> ApiResult<Json<RefreshAllAuthPresetsResponse>> {
    let _active_config_guard = apply::lock_active_config_for_request(&state).await;
    let auth_files = terminal_auth_files(&state)?;
    let auth_targets = terminal_auth_write_targets(&state)?;
    let mut presets = state.auth_manager.auth_presets_snapshot();
    if presets.is_empty() {
        return Ok(Json(RefreshAllAuthPresetsResponse {
            ok: true,
            total: 0,
            success_count: 0,
            failure_count: 0,
            failures: Vec::new(),
        }));
    }

    let current_auth = read_current_auth_state(&auth_files.auth_file)
        .await
        .ok()
        .flatten();
    let active_auth = current_auth.as_ref().and_then(CurrentAuthState::as_login);
    let active_preset_id = presets
        .iter()
        .find(|preset| active_auth == Some(&preset.auth))
        .map(|preset| preset.id.clone());

    let mut success_count = 0usize;
    let mut failures = Vec::new();
    let mut active_preset_auth = None;
    let mut changed_count = 0usize;

    for preset in &mut presets {
        let original_auth = preset.auth.clone();
        match refresh_stored_auth_preset_quota(&state.proxy_manager, preset).await {
            Ok(()) => {
                success_count += 1;
                if active_preset_id.as_deref() == Some(preset.id.as_str()) {
                    active_preset_auth = Some(preset.auth.clone());
                }
            }
            Err(error) => {
                if preset.auth != original_auth {
                    changed_count += 1;
                    if active_preset_id.as_deref() == Some(preset.id.as_str()) {
                        active_preset_auth = Some(preset.auth.clone());
                    }
                }
                failures.push(RefreshAuthPresetFailure {
                    preset_id: preset.id.clone(),
                    name: preset.name.clone(),
                    error: error.to_string(),
                });
            }
        }
    }

    if success_count > 0 || changed_count > 0 {
        persist_auth_presets_async(&state.auth_manager, &presets).await?;
        if let Some(auth) = active_preset_auth.as_ref() {
            write_login_auth_files(&auth_targets, auth).await?;
        }
    }

    Ok(Json(RefreshAllAuthPresetsResponse {
        ok: true,
        total: presets.len(),
        success_count,
        failure_count: failures.len(),
        failures,
    }))
}

mod preset_test_scheduler;
mod preset_tests;

pub use preset_tests::{
    test_all_api_presets, test_all_auth_presets, test_all_claude_presets, test_api_preset,
    test_auth_preset, test_claude_preset,
};

pub(crate) use preset_tests::{
    test_stored_api_preset_with_delay, test_stored_claude_preset_with_delay,
};

pub use preset_test_scheduler::{
    PresetKind, PresetTestSchedule, PresetTestScheduleInfo, PresetTestScheduleRequest,
    PresetTestScheduleResult, PresetTestScheduleUpdateRequest, PresetTestScheduler, ScheduleParams,
    ScheduleType, parse_preset_kind_public,
};

#[cfg(test)]
pub(in crate::auth) use preset_tests::{
    api_probe_endpoint, apply_claude_preset_test_headers, build_preset_test_client_from_env,
    claude_preset_test_target, readable_chat_summary, readable_models_summary,
    test_stored_auth_preset_with_endpoint,
};

// ---- Preset test schedule handlers ----

#[derive(Debug, Serialize)]
pub struct PresetTestScheduleListResponse {
    pub schedules: Vec<PresetTestScheduleInfo>,
}

#[derive(Debug, Serialize)]
pub struct PresetTestScheduleResponse {
    pub ok: bool,
    pub schedule: Option<PresetTestScheduleInfo>,
    pub schedules: Vec<PresetTestScheduleInfo>,
}

pub async fn list_preset_test_schedules(
    State(state): State<AppState>,
) -> ApiResult<Json<PresetTestScheduleListResponse>> {
    let schedules = enrich_schedule_infos(state.preset_test_scheduler.list(), &state);
    Ok(Json(PresetTestScheduleListResponse { schedules }))
}

pub async fn create_preset_test_schedule(
    State(state): State<AppState>,
    Json(payload): Json<PresetTestScheduleRequest>,
) -> ApiResult<Json<PresetTestScheduleResponse>> {
    // Validate that the referenced preset exists and capture its name.
    let (preset_name, kind) =
        resolve_preset_name(&state, &payload.preset_kind, &payload.preset_id)?;
    let req = payload;
    // Store the resolved preset name so the schedule display is self-contained.
    let schedule = state
        .preset_test_scheduler
        .create(req.clone())
        .map_err(|e| AppError::bad_request(format!("创建定时测试任务失败: {e}")))?;
    state
        .preset_test_scheduler
        .set_preset_name_if_exists(&req.preset_id, &preset_name, &kind);
    let schedules = enrich_schedule_infos(state.preset_test_scheduler.list(), &state);
    let schedule = schedules.into_iter().find(|s| s.id == schedule.id);
    Ok(Json(PresetTestScheduleResponse {
        ok: true,
        schedule,
        schedules: enrich_schedule_infos(state.preset_test_scheduler.list(), &state),
    }))
}

pub async fn update_preset_test_schedule(
    State(state): State<AppState>,
    AxumPath(schedule_id): AxumPath<String>,
    Json(payload): Json<PresetTestScheduleUpdateRequest>,
) -> ApiResult<Json<PresetTestScheduleResponse>> {
    let resolved_preset = match (&payload.preset_kind, &payload.preset_id) {
        (Some(kind), Some(preset_id)) => Some(resolve_preset_name(&state, kind, preset_id)?),
        _ => None,
    };
    let updated_preset_id = payload.preset_id.clone();
    let schedule = state
        .preset_test_scheduler
        .update(&schedule_id, payload)
        .map_err(|e| AppError::bad_request(format!("更新定时测试任务失败: {e}")))?;
    if let (Some((name, kind)), Some(preset_id)) = (resolved_preset, updated_preset_id) {
        state
            .preset_test_scheduler
            .set_preset_name_if_exists(&preset_id, &name, &kind);
    }
    let schedules = enrich_schedule_infos(state.preset_test_scheduler.list(), &state);
    Ok(Json(PresetTestScheduleResponse {
        ok: true,
        schedule: Some(schedule),
        schedules,
    }))
}

pub async fn delete_preset_test_schedule(
    State(state): State<AppState>,
    AxumPath(schedule_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let removed = state.preset_test_scheduler.delete(&schedule_id);
    Ok(Json(serde_json::json!({ "ok": true, "removed": removed })))
}

/// Manually trigger a schedule test immediately (non-blocking result).
pub async fn run_preset_test_schedule_now(
    State(state): State<AppState>,
    AxumPath(schedule_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let schedules = state.preset_test_scheduler.list();
    let schedule = schedules
        .iter()
        .find(|s| s.id == schedule_id)
        .ok_or_else(|| AppError::not_found("定时测试任务不存在"))?;
    let kind_str = schedule.preset_kind.clone();
    let preset_id = schedule.preset_id.clone();
    let (preset_name, _kind) = resolve_preset_name(&state, &kind_str, &preset_id)?;
    state
        .preset_test_scheduler
        .set_preset_name_if_exists(&preset_id, &preset_name, &_kind);

    // Fire the test inline so the user gets immediate feedback.
    let auth_manager = state.auth_manager.clone();
    let proxy_manager = state.proxy_manager.clone();
    let workspace_settings = state.workspace_settings.clone();
    let scheduler = state.preset_test_scheduler.clone();
    let sid = schedule_id.clone();
    tokio::spawn(async move {
        let now = current_timestamp_millis();
        // Build a temporary schedule object to reuse fire_test.
        let temp_schedule = PresetTestSchedule {
            id: sid.clone(),
            name: String::new(),
            preset_kind: match kind_str.as_str() {
                "api" => PresetKind::Api,
                _ => PresetKind::Claude,
            },
            preset_id: preset_id.clone(),
            preset_name: preset_name.clone(),
            schedule_type: ScheduleType::Interval,
            schedule_params: ScheduleParams {
                time: String::new(),
                weekdays: Vec::new(),
                weekday: None,
                interval_minutes: 0,
            },
            enabled: true,
            created_at_millis: now,
            last_fired_at_millis: 0,
            next_fire_at_millis: 0,
        };
        let result = scheduler
            .fire_test_for_manual(
                &temp_schedule,
                &auth_manager,
                &proxy_manager,
                &workspace_settings,
            )
            .await;
        let fired_at = current_timestamp_millis();
        if let Ok(test_result) = result {
            let record = PresetTestScheduleResult {
                schedule_id: sid.clone(),
                fired_at_millis: fired_at,
                ok: test_result.ok,
                result: test_result,
            };
            scheduler.store_manual_result(&sid, record);
        }
    });
    Ok(Json(
        serde_json::json!({ "ok": true, "message": "测试已触发，请稍后刷新查看结果" }),
    ))
}

/// Resolve the display name of a preset by kind+id.
fn resolve_preset_name(
    state: &AppState,
    kind_str: &str,
    preset_id: &str,
) -> ApiResult<(String, PresetKind)> {
    let kind = parse_preset_kind_public(kind_str)
        .ok_or_else(|| AppError::bad_request("预设类型无效，请使用 api 或 claude"))?;
    let name = match kind {
        PresetKind::Api => {
            let presets = state.auth_manager.api_presets_snapshot();
            presets
                .iter()
                .find(|p| p.id == preset_id)
                .map(|p| p.name.clone())
                .ok_or_else(|| AppError::not_found("找不到指定的 API 预设"))?
        }
        PresetKind::Claude => {
            let presets = state.auth_manager.claude_presets_snapshot();
            presets
                .iter()
                .find(|p| p.id == preset_id)
                .map(|p| p.name.clone())
                .ok_or_else(|| AppError::not_found("找不到指定的 Claude 预设"))?
        }
    };
    Ok((name, kind))
}

/// Enrich schedule infos with the current preset names (in case presets were renamed).
fn enrich_schedule_infos(
    mut infos: Vec<PresetTestScheduleInfo>,
    state: &AppState,
) -> Vec<PresetTestScheduleInfo> {
    let api_presets = state.auth_manager.api_presets_snapshot();
    let claude_presets = state.auth_manager.claude_presets_snapshot();
    for info in &mut infos {
        let name = match info.preset_kind.as_str() {
            "api" => api_presets
                .iter()
                .find(|p| p.id == info.preset_id)
                .map(|p| p.name.clone()),
            "claude" => claude_presets
                .iter()
                .find(|p| p.id == info.preset_id)
                .map(|p| p.name.clone()),
            _ => None,
        };
        if let Some(name) = name {
            info.preset_name = name;
        }
    }
    infos
}

pub async fn start_codex_oauth_session(
    State(state): State<AppState>,
) -> ApiResult<Json<CodexOAuthSessionResponse>> {
    let pending = request_codex_device_user_code(&state.proxy_manager)
        .await
        .map_err(|error| AppError::bad_request(format!("启动 Codex 官方登录失败: {error:#}")))?;
    let authorize_url = build_codex_device_authorize_url(&pending.user_code);
    let session = state.codex_oauth_manager.insert_pending(
        CODEX_DEVICE_VERIFICATION_URL,
        &authorize_url,
        &pending.user_code,
        pending.poll_interval_seconds,
    );

    let codex_oauth_manager = state.codex_oauth_manager.clone();
    let proxy_manager = state.proxy_manager.clone();
    let session_id = session.id.clone();

    tokio::spawn(async move {
        match complete_codex_device_login(&proxy_manager, pending).await {
            Ok((auth, details)) => codex_oauth_manager.complete(&session_id, auth, details),
            Err(error) => codex_oauth_manager.fail(&session_id, error),
        }
    });

    Ok(Json(codex_oauth_session_response(&session)))
}

pub async fn get_codex_oauth_session(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
) -> ApiResult<Json<CodexOAuthSessionResponse>> {
    let session = state
        .codex_oauth_manager
        .get(&session_id)
        .ok_or_else(|| AppError::not_found("找不到指定的 Codex 官方登录会话。"))?;
    Ok(Json(codex_oauth_session_response(&session)))
}

pub async fn delete_auth_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
) -> ApiResult<Json<DeleteAuthPresetResponse>> {
    let mut presets = state.auth_manager.auth_presets_snapshot();
    let original_len = presets.len();
    presets.retain(|preset| preset.id != preset_id);

    if presets.len() == original_len {
        return Err(AppError::not_found("找不到指定的 auth 预设。"));
    }

    persist_auth_presets_async(&state.auth_manager, &presets).await?;

    Ok(Json(DeleteAuthPresetResponse {
        ok: true,
        deleted_id: preset_id,
    }))
}

pub async fn delete_api_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
) -> ApiResult<Json<DeleteApiPresetResponse>> {
    let mut presets = state.auth_manager.api_presets_snapshot();
    let original_len = presets.len();
    presets.retain(|preset| preset.id != preset_id);

    if presets.len() == original_len {
        return Err(AppError::not_found("找不到指定的 API 预设。"));
    }

    persist_api_presets_async(&state.auth_manager, &presets).await?;

    Ok(Json(DeleteApiPresetResponse {
        ok: true,
        deleted_id: preset_id,
    }))
}

pub async fn delete_claude_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
) -> ApiResult<Json<DeleteClaudePresetResponse>> {
    let mut presets = state.auth_manager.claude_presets_snapshot();
    let original_len = presets.len();
    presets.retain(|preset| preset.id != preset_id);

    if presets.len() == original_len {
        return Err(AppError::not_found("找不到指定的 Claude 预设。"));
    }

    persist_claude_presets_async(&state.auth_manager, &presets).await?;

    Ok(Json(DeleteClaudePresetResponse {
        ok: true,
        deleted_id: preset_id,
    }))
}

#[cfg(test)]
mod tests;
