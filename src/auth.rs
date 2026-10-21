use std::{
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
    fs,
    path::PathBuf,
    time::Duration,
};

use anyhow::{Context, Result};
use auth_core::*;
use axum::{
    Json,
    extract::{Multipart, Path as AxumPath, Query, State},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use terminal_core::current_timestamp_millis;
use tokio::process::Command;
use tracing::warn;

use crate::{ApiResult, AppError, AppState, runtime_paths};

pub use auth_core::{AuthPresetManager, CodexOAuthManager};

mod account_import;
mod apply;
mod model_catalog_policy;
mod sub2api;
pub use model_catalog_policy::{get_model_catalog_policy, save_model_catalog_policy};

#[cfg(test)]
use account_import::API_ACCOUNT_IMPORT_MAX_ARCHIVE_DEPTH;
pub(crate) use account_import::API_ACCOUNT_IMPORT_MAX_UPLOAD_BYTES;
#[cfg(test)]
use account_import::collect_accounts_from_upload;
use account_import::collect_accounts_from_uploads;

#[cfg(test)]
use apply::verify_api_preset_targets;
use apply::{
    activate_dynamic_claude_relay_if_needed, apply_api_preset_locked, apply_auth_preset_locked,
    claude_preset_with_global_defaults, lock_active_config_for_request,
    write_claude_preset_to_targets,
};
pub use apply::{
    apply_api_preset, apply_auth_preset, apply_claude_preset, apply_claude_preset_to_opencode,
    apply_current_auth, clear_current_auth, verify_api_preset,
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

#[derive(Debug, Deserialize, Default)]
pub struct SaveCurrentAuthPresetRequest {
    #[serde(default)]
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct CurrentAuthActionResponse {
    pub ok: bool,
    pub account: Option<CurrentAuthSummary>,
    pub preset: Option<AuthPresetSummary>,
    pub auth_file: String,
    /// See `CodexAccountApplyResponse::codex_daemon_restarted`.
    #[serde(default)]
    pub codex_daemon_restarted: bool,
}

#[derive(Debug, Deserialize, Default)]
pub struct ZcodeMigrationRequest {
    #[serde(default)]
    pub tools: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ZcodeMigrationItem {
    pub tool: String,
    pub imported: usize,
    pub skipped: usize,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ZcodeMigrationResponse {
    pub ok: bool,
    pub source_root: String,
    pub items: Vec<ZcodeMigrationItem>,
}

#[derive(Debug, Serialize)]
pub struct ToggleApiPresetFavoriteResponse {
    pub ok: bool,
    pub preset: ApiPresetSummary,
}

#[derive(Debug, Deserialize, Default)]
pub struct CodexAccountApplyQuery {
    #[serde(default)]
    project_path: String,
    /// See `ApplyApiPresetQuery::skip_daemon_restart`.
    #[serde(default)]
    skip_daemon_restart: bool,
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
    suppress_comp_hash_changes: bool,
) -> ApiResult<()> {
    auth_core::sync_api_model_catalog(path, targets, bundled_catalog, suppress_comp_hash_changes)
        .await
        .map_err(|error| map_internal("更新 Codex 模型 metadata 失败", error))
}

async fn sync_api_model_catalogs(
    state: &AppState,
    auth_targets: &[TerminalAuthFiles],
    config_targets: &[ResolvedConfigTarget],
) -> ApiResult<()> {
    if !config_targets
        .iter()
        .any(|target| target.key.eq_ignore_ascii_case("model") && !target.value.trim().is_empty())
    {
        return Ok(());
    }

    let suppress = model_catalog_policy::load(state)
        .await?
        .suppress_comp_hash_changes;
    for target in auth_targets {
        let (bundled_catalog, bundled_error) =
            match read_codex_bundled_model_catalog(&target.user_name).await {
                Ok(catalog) => (Some(catalog), None),
                Err(error) => {
                    warn!("读取用户 `{}` 的 Codex bundled 模型目录失败: {error}", target.user_name);
                    (None, Some(error))
                }
            };
        if let Err(sync_error) = sync_api_model_catalog(
            &target.config_file,
            config_targets,
            bundled_catalog.as_ref(),
            suppress,
        )
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
    let current_login_auth = (current_mode == CurrentAuthMode::Auth)
        .then(|| {
            current_auth_value
                .as_ref()
                .and_then(CurrentAuthState::as_login)
        })
        .flatten();

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

pub async fn list_codex_accounts(
    State(state): State<AppState>,
) -> ApiResult<Json<CodexAccountListResponse>> {
    let auth_files = terminal_auth_files(&state)?;
    let auth_presets = state.auth_manager.auth_presets_snapshot();
    let api_presets = state.auth_manager.api_presets_snapshot();
    let (current_auth, _, _, _, current_mode, current_api) =
        read_codex_current_state(&auth_files.auth_file, &auth_files.config_file, &api_presets)
            .await?;
    let current_login = current_auth.as_ref().and_then(CurrentAuthState::as_login);
    let upstream_proxy = state.auth_manager.upstream_proxy_settings();

    let accounts = auth_presets
        .iter()
        .map(|preset| {
            let summary = preset_summary(preset, current_login);
            CodexAccountSummary {
                kind: CodexAccountKind::Oauth,
                id: preset.id.clone(),
                name: preset.name.clone(),
                active: current_mode == CurrentAuthMode::Auth && summary.active,
                saved_at: preset.saved_at,
                switch_count: preset.switch_count,
                account: summary
                    .details
                    .email
                    .clone()
                    .or_else(|| Some(summary.account_id)),
                model: None,
                endpoint: Some("ChatGPT OAuth".to_string()),
                favorite: false,
            }
        })
        .chain(api_presets.iter().map(|preset| {
            let summary = api_preset_summary_with_proxy_state(
                preset,
                current_mode,
                current_api.as_ref(),
                &upstream_proxy,
            );
            CodexAccountSummary {
                kind: CodexAccountKind::Api,
                id: preset.id.clone(),
                name: preset.name.clone(),
                active: summary.active,
                saved_at: preset.saved_at,
                switch_count: preset.switch_count,
                account: Some(preset.provider_name.clone()),
                model: api_preset_model(preset).map(str::to_string),
                endpoint: Some(preset.base_url.clone()),
                favorite: summary.favorite,
            }
        }))
        .collect::<Vec<_>>();

    Ok(Json(CodexAccountListResponse {
        current_mode,
        accounts,
    }))
}

pub async fn apply_codex_account(
    State(state): State<AppState>,
    AxumPath((kind, preset_id)): AxumPath<(CodexAccountKind, String)>,
    Query(query): Query<CodexAccountApplyQuery>,
) -> ApiResult<Json<CodexAccountApplyResponse>> {
    let _guard = lock_active_config_for_request(&state).await;
    let (name, local_config_file, codex_daemon_restarted) = match kind {
        CodexAccountKind::Oauth => {
            let (response, _) = apply_auth_preset_locked(
                &state,
                &preset_id,
                &query.project_path,
                query.skip_daemon_restart,
            )
            .await?;
            (response.name, response.local_config_file, response.codex_daemon_restarted)
        }
        CodexAccountKind::Api => {
            let (response, _) = apply_api_preset_locked(
                &state,
                &preset_id,
                &query.project_path,
                query.skip_daemon_restart,
            )
            .await?;
            (response.name, response.local_config_file, response.codex_daemon_restarted)
        }
    };

    Ok(Json(CodexAccountApplyResponse {
        ok: true,
        kind,
        id: preset_id,
        name,
        local_config_file,
        codex_daemon_restarted,
    }))
}

pub(crate) async fn current_api_preset_id(state: &AppState) -> ApiResult<Option<String>> {
    let auth_files = terminal_auth_files(state)?;
    let presets = state.auth_manager.api_presets_snapshot();
    let (_, _, _, _, current_mode, current_api) =
        read_codex_current_state(&auth_files.auth_file, &auth_files.config_file, &presets).await?;
    Ok(presets
        .iter()
        .find(|preset| api_preset_matches_current_api(preset, current_mode, current_api.as_ref()))
        .map(|preset| preset.id.clone()))
}

pub(crate) async fn apply_api_preset_for_run(
    state: &AppState,
    preset_id: &str,
    project_path: &str,
) -> ApiResult<StoredApiPreset> {
    let _config_guard = lock_active_config_for_request(state).await;
    let (_, preset) = apply_api_preset_locked(state, preset_id, project_path, true).await?;
    Ok(preset)
}

pub async fn acknowledge_removed_preset_run_lease(
    AxumPath(lease_id): AxumPath<String>,
) -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "ok": true,
        "lease_id": lease_id,
        "compatibility_only": true,
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

pub async fn detect_current_codex_auth(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let _ = state.auth_manager.reload_auth_presets();
    let auth_files = terminal_auth_files(&state)?;
    let api_presets = state.auth_manager.api_presets_snapshot();
    let (current_auth, _, _, _, _, _) =
        read_codex_current_state(&auth_files.auth_file, &auth_files.config_file, &api_presets)
            .await?;
    let presets = state.auth_manager.auth_presets_snapshot();
    let current_login = current_auth.as_ref().and_then(CurrentAuthState::as_login);

    if let Some(current) = current_login {
        let matched = presets.iter().find(|preset| {
            if !current.tokens.account_id.is_empty() && !preset.auth.tokens.account_id.is_empty() {
                current.tokens.account_id == preset.auth.tokens.account_id
            } else {
                current.tokens.access_token == preset.auth.tokens.access_token
            }
        });
        let summary = auth_summary(current);
        Ok(Json(json!({
            "ok": true,
            "matched": matched.is_some(),
            "preset_id": matched.map(|p| p.id.as_str()),
            "name": matched.map(|p| p.name.as_str()),
            "email": summary.email,
            "account_id": summary.account_id,
            "short_id": summary.short_id,
        })))
    } else {
        Ok(Json(json!({
            "ok": true,
            "matched": false,
            "logged_in": false,
        })))
    }
}

pub async fn detect_current_codex_api(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let _ = state.auth_manager.reload_api_presets();
    let auth_files = terminal_auth_files(&state)?;
    let api_presets = state.auth_manager.api_presets_snapshot();
    let (_, _, _, _, current_mode, current_api) =
        read_codex_current_state(&auth_files.auth_file, &auth_files.config_file, &api_presets)
            .await?;
    let upstream_proxy = state.auth_manager.upstream_proxy_settings();

    let matched = api_presets.iter().find(|preset| {
        api_preset_summary_with_proxy_state(
            preset,
            current_mode,
            current_api.as_ref(),
            &upstream_proxy,
        )
        .active
    });

    let matched_model = matched.and_then(|p| {
        p.config_overrides
            .iter()
            .find(|o| o.key.as_deref() == Some("model"))
            .and_then(|o| o.value.as_deref())
            .or(p.legacy_config_value.as_deref())
    });

    Ok(Json(json!({
        "ok": true,
        "matched": matched.is_some(),
        "preset_id": matched.map(|p| p.id.as_str()),
        "name": matched.map(|p| p.name.as_str()),
        "model": matched_model,
        "provider": current_api.as_ref().and_then(|a| a.provider_name.as_deref()).or_else(|| current_api.as_ref().and_then(|a| a.provider_id.as_deref())),
        "current_mode": current_mode,
    })))
}

pub async fn detect_current_claude(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let _ = state.auth_manager.reload_claude_presets();
    let auth_files = terminal_auth_files(&state)?;
    let presets = state.auth_manager.claude_presets_snapshot();
    let effective_presets = presets
        .iter()
        .map(|preset| claude_preset_with_global_defaults(&state.workspace_settings, preset))
        .collect::<ApiResult<Vec<_>>>()?;
    let current_claude =
        read_current_claude_state(&auth_files.claude_settings_file, &effective_presets).await;
    let (current_claude_value, _) = match current_claude {
        Ok(value) => (value, None),
        Err(error) => (None, Some(error)),
    };
    let upstream_proxy = state.auth_manager.upstream_proxy_settings();

    let matched = presets
        .iter()
        .zip(&effective_presets)
        .find(|(preset, effective_preset)| {
            claude_preset_summary_with_effective_proxy_state(
                preset,
                effective_preset,
                current_claude_value.as_ref(),
                &upstream_proxy,
            )
            .active
        })
        .map(|(preset, _)| preset);

    Ok(Json(json!({
        "ok": true,
        "matched": matched.is_some(),
        "preset_id": matched.map(|p| p.id.as_str()),
        "name": matched.map(|p| p.name.as_str()),
        "model": current_claude_value.as_ref().and_then(|c| c.default_sonnet_model.as_deref()),
        "base_url": current_claude_value.as_ref().and_then(|c| c.base_url.as_deref()),
    })))
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

async fn ensure_auth_id_token_if_possible(
    proxy_manager: &impl CodexAuthHttpClientProvider,
    auth: &mut AuthFile,
) {
    if auth.tokens.id_token.trim().is_empty() && !auth.tokens.refresh_token.trim().is_empty() {
        if let Err(error) = refresh_codex_tokens(proxy_manager, auth).await {
            tracing::warn!(%error, "尝试通过 refresh_token 补全 id_token 失败，将使用原始凭据保存");
        } else {
            tracing::info!(account_id = %auth.tokens.account_id, "导入/保存 Auth 预设时已自动通过 refresh_token 换取官方 id_token");
        }
    }
}

pub async fn save_auth_preset(
    State(state): State<AppState>,
    Json(payload): Json<SaveAuthPresetRequest>,
) -> ApiResult<Json<SaveAuthPresetResponse>> {
    let auth_files = terminal_auth_files(&state)?;
    let mut auth = payload.auth;
    ensure_auth_id_token_if_possible(&state.proxy_manager, &mut auth).await;
    validate_auth_file(&auth)?;
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
    let name = resolve_auth_preset_name(&payload.name, &auth, &presets, None);
    let mut details = sanitize_auth_preset_details(payload.details);
    if details.email.is_none() {
        let summary = auth_summary(&auth);
        details.email = summary.email;
        if details.plan_type.is_none() {
            details.plan_type = summary.plan_type;
        }
    }
    let preset = StoredAuthPreset {
        interchange_source: None,
        id: generate_preset_id("auth"),
        name,
        saved_at: current_timestamp_secs(),
        details,
        config_overrides,
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth,
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

/// Save the current official Codex OAuth login as a local CLI preset.
pub async fn save_current_auth_preset(
    State(state): State<AppState>,
    Json(payload): Json<SaveCurrentAuthPresetRequest>,
) -> ApiResult<Json<CurrentAuthActionResponse>> {
    let auth_files = terminal_auth_files(&state)?;
    let current = read_current_auth_state(&auth_files.auth_file)
        .await
        .map_err(AppError::bad_request)?;
    let Some(CurrentAuthState::Login(auth)) = current else {
        return Err(AppError::bad_request("当前没有可保存的 Codex OAuth 登录。"));
    };
    Ok(Json(
        save_current_auth_preset_for_login(
            &state,
            &payload.name,
            auth_files.auth_file.display().to_string(),
            auth,
        )
        .await?,
    ))
}

/// Re-login/save-current for an existing account updates only the auth tokens
/// in place — never the preset name, account details, config overrides, or
/// switch count. The match reuses the shared account_id + id_token email rule
/// (Team workspaces share an account_id); no match or an ambiguous match falls
/// back to creating a new preset.
async fn save_current_auth_preset_for_login(
    state: &AppState,
    name: &str,
    auth_file_display: String,
    auth: AuthFile,
) -> ApiResult<CurrentAuthActionResponse> {
    let mut presets = state.auth_manager.auth_presets_snapshot();
    let details = auth_summary(&auth);
    let preset = match apply::login_auth_preset_match_index(&presets, &auth) {
        Some(index) => {
            let preset = &mut presets[index];
            preset.auth = auth;
            preset.saved_at = current_timestamp_secs();
            if details.email.is_some() {
                preset.details.email = details.email.clone();
            }
            if details.plan_type.is_some() {
                preset.details.plan_type = details.plan_type.clone();
            }
            preset.clone()
        }
        None => {
            let preset = StoredAuthPreset {
                interchange_source: None,
                id: generate_preset_id("auth"),
                name: resolve_auth_preset_name(name, &auth, &presets, None),
                saved_at: current_timestamp_secs(),
                details: sanitize_auth_preset_details(AuthPresetDetails {
                    email: details.email.clone(),
                    plan_type: details.plan_type.clone(),
                    ..Default::default()
                }),
                config_overrides: Vec::new(),
                legacy_config_key: None,
                legacy_config_value: None,
                legacy_secondary_config_key: None,
                legacy_secondary_config_value: None,
                auth,
                switch_count: 0,
            };
            presets.insert(0, preset.clone());
            preset
        }
    };
    persist_auth_presets_async(&state.auth_manager, &presets).await?;
    Ok(CurrentAuthActionResponse {
        ok: true,
        account: Some(details),
        preset: Some(preset_summary(&preset, Some(&preset.auth))),
        auth_file: auth_file_display,
        codex_daemon_restarted: false,
    })
}

/// Import the CLI account snapshots produced by zcode-account-manager.  This
/// deliberately targets CLI stores only: no imported OAuth token is used by
/// webClx's LLM proxy.
pub async fn migrate_zcode_accounts(
    State(state): State<AppState>,
    Json(payload): Json<ZcodeMigrationRequest>,
) -> ApiResult<Json<ZcodeMigrationResponse>> {
    let auth_files = terminal_auth_files(&state)?;
    let source_root = auth_files
        .auth_file
        .parent()
        .and_then(|path| path.parent())
        .map(|path| path.to_path_buf())
        .ok_or_else(|| AppError::internal("无法解析终端用户目录。"))?;
    let requested: HashSet<String> = payload
        .tools
        .into_iter()
        .map(|tool| tool.trim().to_ascii_lowercase())
        .filter(|tool| !tool.is_empty())
        .collect();
    let selected = |tool: &str| requested.is_empty() || requested.contains(tool);
    let mut items = Vec::new();

    if selected("codex") {
        let root = source_root.join(".codex").join("account_backups");
        let mut presets = state.auth_manager.auth_presets_snapshot();
        let mut imported = 0;
        let mut skipped = 0;
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                let dir = entry.path();
                if !dir.is_dir() || entry.file_name().to_string_lossy().starts_with('.') {
                    continue;
                }
                let manifest = fs::read_to_string(dir.join("manifest.json"))
                    .ok()
                    .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
                let name = manifest
                    .as_ref()
                    .and_then(|value| value.get("alias").or_else(|| value.get("name")))
                    .and_then(serde_json::Value::as_str)
                    .filter(|value| !value.trim().is_empty())
                    .unwrap_or("zcode Codex 账号")
                    .to_string();
                let bytes = match fs::read(dir.join("data").join("auth")) {
                    Ok(bytes) => bytes,
                    Err(_) => {
                        skipped += 1;
                        continue;
                    }
                };
                let auth = match serde_json::from_slice::<AuthFile>(&bytes) {
                    Ok(auth) => auth,
                    Err(_) => {
                        skipped += 1;
                        continue;
                    }
                };
                if let Some(existing) = presets
                    .iter_mut()
                    .find(|preset| preset.auth.tokens.account_id == auth.tokens.account_id)
                {
                    existing.auth = auth;
                    existing.name = name;
                    existing.saved_at = current_timestamp_secs();
                } else {
                    let details = auth_summary(&auth);
                    presets.push(StoredAuthPreset {
                        interchange_source: None,
                        id: generate_preset_id("auth"),
                        name,
                        saved_at: current_timestamp_secs(),
                        details: AuthPresetDetails {
                            email: details.email,
                            plan_type: details.plan_type,
                            ..Default::default()
                        },
                        config_overrides: Vec::new(),
                        legacy_config_key: None,
                        legacy_config_value: None,
                        legacy_secondary_config_key: None,
                        legacy_secondary_config_value: None,
                        auth,
                        switch_count: 0,
                    });
                }
                imported += 1;
            }
        }
        if imported > 0 {
            persist_auth_presets_async(&state.auth_manager, &presets).await?;
        }
        items.push(ZcodeMigrationItem {
            tool: "codex".into(),
            imported,
            skipped,
            message: "已导入 Codex CLI 登录快照；仅供官方 Codex CLI 切换使用。".into(),
        });
    }

    if selected("codebuddy") {
        let root = source_root.join(".codebuddy").join("account_backups");
        let mut imported = 0;
        let mut skipped = 0;
        if let Ok(entries) = fs::read_dir(&root) {
            for entry in entries.flatten() {
                let dir = entry.path();
                let settings = fs::read(dir.join("data").join("settings.json"))
                    .or_else(|_| fs::read(dir.join("data").join("settings")))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
                let token = settings
                    .as_ref()
                    .and_then(|value| {
                        value
                            .pointer("/env/CODEBUDDY_AUTH_TOKEN")
                            .and_then(Value::as_str)
                            .or_else(|| value.get("CODEBUDDY_AUTH_TOKEN").and_then(Value::as_str))
                    })
                    .map(str::trim)
                    .filter(|value| !value.is_empty());
                let Some(token) = token else {
                    skipped += 1;
                    continue;
                };
                let name = fs::read_to_string(dir.join("manifest.json"))
                    .ok()
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                    .and_then(|value| {
                        value
                            .get("alias")
                            .or_else(|| value.get("name"))
                            .and_then(Value::as_str)
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| "zcode CodeBuddy 账号".into());
                if state
                    .codebuddy_manager
                    .add_manual_account(token.to_string(), String::new(), name, None)
                    .await
                    .is_ok()
                {
                    imported += 1;
                } else {
                    skipped += 1;
                }
            }
        }
        items.push(ZcodeMigrationItem {
            tool: "codebuddy".into(),
            imported,
            skipped,
            message: "已导入 CodeBuddy CLI 账号；不会写入 Codex_API 预设。".into(),
        });
    }

    if selected("gemini") || selected("agy") {
        let tool = if selected("agy") { "agy" } else { "gemini" };
        let root = source_root.join(".gemini").join("account_backups");
        let count = fs::read_dir(root)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.path().is_dir())
                    .count()
            })
            .unwrap_or(0);
        let (imported, skipped) = state.agy_manager.import_snapshots();
        items.push(ZcodeMigrationItem {
            tool: tool.into(),
            imported,
            skipped: skipped.max(count.saturating_sub(imported)),
            message: "已导入独立 AGY CLI 账号库；不会写入 LLM_API。".into(),
        });
    }

    Ok(Json(ZcodeMigrationResponse {
        ok: true,
        source_root: source_root.display().to_string(),
        items,
    }))
}

pub async fn update_auth_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
    Json(payload): Json<SaveAuthPresetRequest>,
) -> ApiResult<Json<SaveAuthPresetResponse>> {
    let auth_files = terminal_auth_files(&state)?;
    let mut auth = payload.auth;
    ensure_auth_id_token_if_possible(&state.proxy_manager, &mut auth).await;
    validate_auth_file(&auth)?;
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

    let name = resolve_auth_preset_name(&payload.name, &auth, &presets, Some(&preset_id));
    let mut details = sanitize_auth_preset_details(payload.details);
    if details.email.is_none() {
        let summary = auth_summary(&auth);
        details.email = summary.email;
        if details.plan_type.is_none() {
            details.plan_type = summary.plan_type;
        }
    }
    let preset = StoredAuthPreset {
        interchange_source: presets[index].interchange_source.clone(),
        id: preset_id,
        name,
        saved_at: current_timestamp_secs(),
        details,
        config_overrides,
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth,
        switch_count: presets[index].switch_count,
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
        interchange_source: None,
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
        favorite: false,
        switch_count: 0,
    };
    presets.insert(0, preset.clone());

    persist_api_presets_async(&state.auth_manager, &presets).await?;

    let auth_files = terminal_auth_files(&state)?;
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

    Ok(Json(save_imported_accounts(&state, parsed, Vec::new()).await?))
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

    Ok(Json(save_imported_accounts(&state, imported.accounts, imported.errors).await?))
}

async fn save_imported_accounts(
    state: &AppState,
    parsed: Vec<ImportedAccount>,
    errors: Vec<String>,
) -> ApiResult<ImportApiAccountsResponse> {
    let (oauth_accounts, api_accounts): (Vec<_>, Vec<_>) = parsed
        .into_iter()
        .partition(|account| account.is_sub2api_data);

    let mut api_response = save_imported_api_accounts(state, api_accounts, Vec::new()).await?;
    let mut auth_response = save_imported_auth_accounts(state, oauth_accounts).await?;

    let mut saved_names = auth_response.saved_names;
    saved_names.append(&mut api_response.saved_names);
    let mut response_errors = errors;
    response_errors.append(&mut auth_response.errors);
    response_errors.append(&mut api_response.errors);

    Ok(ImportApiAccountsResponse {
        ok: !saved_names.is_empty(),
        saved_count: saved_names.len(),
        saved_names,
        errors: response_errors,
    })
}

async fn save_imported_auth_accounts(
    state: &AppState,
    parsed: Vec<ImportedAccount>,
) -> ApiResult<ImportApiAccountsResponse> {
    if parsed.is_empty() {
        return Ok(ImportApiAccountsResponse {
            ok: false,
            saved_count: 0,
            saved_names: Vec::new(),
            errors: Vec::new(),
        });
    }

    let manager = &state.auth_manager;
    let mut presets = manager.auth_presets_snapshot();
    let mut saved_names = Vec::new();
    let mut errors = Vec::new();

    for account in &parsed {
        if account.account_id.is_empty() {
            errors.push(format!("账号 {} 缺少 account_id。", account.name));
            continue;
        }

        let mut auth = AuthFile {
            openai_api_key: None,
            last_refresh: if account.last_refresh.is_empty() {
                current_rfc3339_timestamp()
            } else {
                account.last_refresh.clone()
            },
            tokens: AuthTokens {
                access_token: account.access_token.clone(),
                account_id: account.account_id.clone(),
                id_token: account.id_token.clone(),
                refresh_token: account.refresh_token.clone(),
            },
        };
        ensure_auth_id_token_if_possible(&state.proxy_manager, &mut auth).await;
        let derived_details = auth_summary(&auth);
        let details = sanitize_auth_preset_details(AuthPresetDetails {
            email: (!account.email.is_empty())
                .then(|| account.email.clone())
                .or(derived_details.email),
            plan_type: (!account.plan_type.is_empty())
                .then(|| account.plan_type.to_uppercase())
                .or(derived_details.plan_type),
            account_name: (!account.name.is_empty()).then(|| account.name.clone()),
            login_method: Some("OAuth".to_string()),
            ..AuthPresetDetails::default()
        });

        let preset_id = unique_import_preset_id(
            &generate_preset_id("auth"),
            presets.iter().map(|preset| preset.id.as_str()),
        );
        let resolved_name = resolve_auth_preset_name(&account.name, &auth, &presets, None);
        let preset = StoredAuthPreset {
            interchange_source: None,
            id: preset_id,
            name: resolved_name.clone(),
            saved_at: current_timestamp_secs(),
            details,
            config_overrides: Vec::new(),
            legacy_config_key: None,
            legacy_config_value: None,
            legacy_secondary_config_key: None,
            legacy_secondary_config_value: None,
            auth,
            switch_count: 0,
        };
        presets.insert(0, preset);
        saved_names.push(resolved_name);
    }

    if !saved_names.is_empty() {
        persist_auth_presets_async(manager, &presets).await?;
    }

    Ok(ImportApiAccountsResponse {
        ok: !saved_names.is_empty(),
        saved_count: saved_names.len(),
        saved_names,
        errors,
    })
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
            interchange_source: None,
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
            favorite: false,
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

#[derive(Debug, Deserialize)]
pub struct Sub2ApiSyncRequest {
    pub base_url: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub totp_code: String,
    #[serde(default)]
    pub temp_token: String,
    #[serde(default)]
    pub recycled: bool,
    #[serde(default)]
    pub list_only: bool,
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default)]
    pub import_selected: Vec<i64>,
}

#[derive(Debug, Serialize)]
pub struct Sub2ApiSyncResponse {
    pub ok: bool,
    pub codex_saved: usize,
    pub claude_saved: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

pub async fn sync_sub2api_accounts(
    State(state): State<AppState>,
    Json(payload): Json<Sub2ApiSyncRequest>,
) -> ApiResult<Json<serde_json::Value>> {
    let token = if !payload.token.trim().is_empty() {
        payload.token.trim().to_string()
    } else if !payload.temp_token.trim().is_empty() && !payload.totp_code.trim().is_empty() {
        sub2api::login_totp(&payload.base_url, &payload.temp_token, &payload.totp_code)
            .await
            .map_err(|error| AppError::bad_request(error.to_string()))?
    } else {
        sub2api::login(&payload.base_url, &payload.email, &payload.password)
            .await
            .map_err(|error| AppError::bad_request(error.to_string()))?
    };

    if payload.list_only || payload.dry_run {
        let accounts =
            sub2api::fetch_accounts_by_recycled(&payload.base_url, &token, payload.recycled)
                .await
                .map_err(|error| AppError::bad_request(error.to_string()))?;

        let list: Vec<serde_json::Value> = accounts
            .iter()
            .map(|a| {
                serde_json::json!({
                    "id": a.id.unwrap_or(0),
                    "name": a.name.clone().unwrap_or_default(),
                    "platform": a.platform.clone().unwrap_or_default(),
                    "kind": a.kind.clone().unwrap_or_default(),
                    "base_url": a.base_url().unwrap_or(""),
                })
            })
            .collect();

        let response_payload = serde_json::json!({
            "ok": true,
            "token": token,
            "accounts": list,
            "codex_saved": 0,
            "claude_saved": 0,
            "skipped": 0,
            "errors": [],
        });
        return Ok(Json(response_payload));
    }

    let ids_filter = if payload.import_selected.is_empty() {
        None
    } else {
        Some(payload.import_selected.as_slice())
    };

    let accounts = match sub2api::fetch_accounts_data(&payload.base_url, &token, ids_filter).await {
        Ok(data_accounts) if !data_accounts.is_empty() => data_accounts,
        _ => {
            let fallback_accounts =
                sub2api::fetch_accounts_by_recycled(&payload.base_url, &token, payload.recycled)
                    .await
                    .map_err(|error| AppError::bad_request(error.to_string()))?;
            if let Some(ids) = ids_filter {
                let id_set: HashSet<i64> = ids.iter().copied().collect();
                fallback_accounts
                    .into_iter()
                    .filter(|a| a.id.map(|id| id_set.contains(&id)).unwrap_or(false))
                    .collect()
            } else {
                fallback_accounts
            }
        }
    };

    let normalized = sub2api::normalize_accounts(&accounts);

    let mut saved_oauth_names = Vec::new();
    {
        let mut presets = state.auth_manager.auth_presets_snapshot();
        for norm in normalized.oauth.iter() {
            let preset_id = unique_import_preset_id(
                &generate_preset_id("auth"),
                presets.iter().map(|preset| preset.id.as_str()),
            );
            let auth = AuthFile {
                openai_api_key: None,
                last_refresh: norm
                    .last_refresh
                    .clone()
                    .unwrap_or_else(current_rfc3339_timestamp),
                tokens: AuthTokens {
                    access_token: norm.access_token.clone(),
                    account_id: norm.account_id.clone(),
                    id_token: norm.id_token.clone(),
                    refresh_token: norm.refresh_token.clone(),
                },
            };
            let summary = auth_summary(&auth);
            let details = sanitize_auth_preset_details(AuthPresetDetails {
                email: norm.email.clone().or(summary.email),
                plan_type: norm.plan_type.clone().or(summary.plan_type),
                ..Default::default()
            });
            let resolved_name = resolve_auth_preset_name(&norm.name, &auth, &presets, None);
            let preset = StoredAuthPreset {
                interchange_source: None,
                id: preset_id,
                name: resolved_name.clone(),
                saved_at: current_timestamp_secs(),
                details,
                config_overrides: norm.config_overrides.clone(),
                legacy_config_key: None,
                legacy_config_value: None,
                legacy_secondary_config_key: None,
                legacy_secondary_config_value: None,
                auth,
                switch_count: 0,
            };
            presets.insert(0, preset);
            saved_oauth_names.push(resolved_name);
        }
        if !saved_oauth_names.is_empty() {
            persist_auth_presets_async(&state.auth_manager, &presets).await?;
        }
    }

    let mut saved_api_names = Vec::new();
    {
        let mut presets = state.auth_manager.api_presets_snapshot();
        for norm in normalized.api.iter() {
            let preset_id = unique_import_preset_id(
                &generate_preset_id("api"),
                presets.iter().map(|preset| preset.id.as_str()),
            );
            let resolved_name = resolve_api_preset_name(&norm.name, &norm.base_url, &presets, None);
            let preset = StoredApiPreset {
                interchange_source: None,
                id: preset_id,
                name: resolved_name.clone(),
                saved_at: current_timestamp_secs(),
                provider_name: norm.provider_name.clone(),
                base_url: norm.base_url.clone(),
                management_url: None,
                wire_api: norm.wire_api.clone(),
                responses_proxy: None,
                apply_upstream_proxy_on_switch: false,
                terminal_env: Vec::new(),
                terminal_startup_script: None,
                config_overrides: norm.config_overrides.clone(),
                legacy_config_key: None,
                legacy_config_value: None,
                legacy_secondary_config_key: None,
                legacy_secondary_config_value: None,
                api_key: norm.api_key.clone(),
                access_token: String::new(),
                account_id: String::new(),
                access_mode: Some(auth_core::ApiAccessMode::Direct),
                favorite: false,
                switch_count: 0,
            };
            presets.insert(0, preset);
            saved_api_names.push(resolved_name);
        }
        if !saved_api_names.is_empty() {
            persist_api_presets_async(&state.auth_manager, &presets).await?;
        }
    }

    let mut saved_claude_names = Vec::new();
    {
        let mut presets = state.auth_manager.claude_presets_snapshot();
        for norm in normalized.claude.iter() {
            let preset_id = generate_preset_id("claude");
            let resolved_name = {
                let existing_names: HashSet<&str> =
                    presets.iter().map(|preset| preset.name.as_str()).collect();
                let mut candidate = norm.name.clone();
                let mut suffix = 2_u32;
                while existing_names.contains(candidate.as_str()) {
                    candidate = format!("{} ({})", norm.name, suffix);
                    suffix += 1;
                }
                candidate
            };
            let preset = StoredClaudePreset {
                interchange_source: None,
                id: preset_id,
                name: resolved_name.clone(),
                saved_at: current_timestamp_secs(),
                provider_name: norm.provider_name.clone(),
                base_url: norm.base_url.clone(),
                management_url: None,
                config_overrides: Vec::new(),
                legacy_config_key: None,
                legacy_config_value: None,
                legacy_secondary_config_key: None,
                legacy_secondary_config_value: None,
                auth_token: norm.auth_token.clone(),
                default_haiku_model: None,
                default_sonnet_model: None,
                default_opus_model: None,
                third_party_model: None,
                use_local_proxy: false,
                access_mode: None,
                switch_count: 0,
            };
            presets.insert(0, preset);
            saved_claude_names.push(resolved_name);
        }
        if !saved_claude_names.is_empty() {
            persist_claude_presets_async(&state.auth_manager, &presets).await?;
        }
    }

    let response = serde_json::json!({
        "ok": !saved_oauth_names.is_empty() || !saved_api_names.is_empty() || !saved_claude_names.is_empty(),
        "token": token,
        "oauth_saved": saved_oauth_names.len(),
        "api_saved": saved_api_names.len(),
        "codex_saved": saved_oauth_names.len() + saved_api_names.len(),
        "claude_saved": saved_claude_names.len(),
        "skipped": 0,
        "errors": [],
    });
    Ok(Json(response))
}

#[derive(Debug, Deserialize)]
pub struct Sub2ApiUsageQueryRequest {
    pub base_url: String,
    pub token: String,
    pub account_id: i64,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Serialize)]
pub struct Sub2ApiUsageQueryResponse {
    pub ok: bool,
    pub usage: Option<serde_json::Value>,
    pub error: Option<String>,
}

pub async fn query_sub2api_account_usage(
    State(_state): State<AppState>,
    Json(payload): Json<Sub2ApiUsageQueryRequest>,
) -> ApiResult<Json<Sub2ApiUsageQueryResponse>> {
    let source = payload.source.as_deref().filter(|s| !s.is_empty());
    match sub2api::get_account_usage(
        &payload.base_url,
        &payload.token,
        payload.account_id,
        source,
        payload.force,
    )
    .await
    {
        Ok(usage) => {
            let usage_value = serde_json::to_value(&usage)
                .map_err(|error| AppError::internal(format!("序列化额度数据失败: {error}")))?;
            Ok(Json(Sub2ApiUsageQueryResponse {
                ok: true,
                usage: Some(usage_value),
                error: None,
            }))
        }
        Err(error) => Ok(Json(Sub2ApiUsageQueryResponse {
            ok: false,
            usage: None,
            error: Some(error.to_string()),
        })),
    }
}

#[derive(Debug, Deserialize)]
pub struct Sub2ApiBatchUsageQueryRequest {
    pub base_url: String,
    pub token: String,
    #[serde(default)]
    pub account_ids: Vec<i64>,
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Serialize)]
pub struct Sub2ApiBatchUsageQueryResponse {
    pub ok: bool,
    pub usage: Option<serde_json::Value>,
    pub errors: Option<serde_json::Value>,
    pub error: Option<String>,
}

pub async fn query_sub2api_accounts_usage_batch(
    State(_state): State<AppState>,
    Json(payload): Json<Sub2ApiBatchUsageQueryRequest>,
) -> ApiResult<Json<Sub2ApiBatchUsageQueryResponse>> {
    match sub2api::get_accounts_usage_batch(
        &payload.base_url,
        &payload.token,
        &payload.account_ids,
        payload.force,
    )
    .await
    {
        Ok(batch) => {
            let usage = serde_json::to_value(&batch.usage)
                .map_err(|error| AppError::internal(format!("序列化批量额度数据失败: {error}")))?;
            let errors = serde_json::to_value(&batch.errors)
                .map_err(|error| AppError::internal(format!("序列化批量额度错误失败: {error}")))?;
            Ok(Json(Sub2ApiBatchUsageQueryResponse {
                ok: true,
                usage: Some(usage),
                errors: Some(errors),
                error: None,
            }))
        }
        Err(error) => Ok(Json(Sub2ApiBatchUsageQueryResponse {
            ok: false,
            usage: None,
            errors: None,
            error: Some(error.to_string()),
        })),
    }
}

#[derive(Debug, Deserialize)]
pub struct Sub2ApiPelicanQueryRequest {
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
pub struct Sub2ApiPelicanQueryResponse {
    pub ok: bool,
    pub result: Option<crate::auth::sub2api::Sub2ApiPelicanResult>,
    pub error: Option<String>,
}

pub async fn run_sub2api_pelican_test(
    State(_state): State<AppState>,
    Json(payload): Json<Sub2ApiPelicanQueryRequest>,
) -> ApiResult<Json<Sub2ApiPelicanQueryResponse>> {
    match sub2api::run_pelican_test(
        &payload.base_url,
        &payload.token,
        payload.account_id,
        &payload.model,
        &payload.prompt,
        &payload.reasoning_effort,
    )
    .await
    {
        Ok(result) => Ok(Json(Sub2ApiPelicanQueryResponse {
            ok: true,
            result: Some(result),
            error: None,
        })),
        Err(error) => Ok(Json(Sub2ApiPelicanQueryResponse {
            ok: false,
            result: None,
            error: Some(error.to_string()),
        })),
    }
}

#[derive(Debug, Deserialize)]
pub struct Sub2ApiAutoActivateRequest {
    pub base_url: String,
    pub token: String,
    pub account_ids: Vec<i64>,
    #[serde(default)]
    pub execute_at: String,
}

#[derive(Debug, Serialize)]
pub struct Sub2ApiAutoActivateResponse {
    pub ok: bool,
    pub scheduled_count: usize,
    pub errors: Vec<String>,
}

pub async fn schedule_sub2api_auto_activate(
    State(state): State<AppState>,
    Json(payload): Json<Sub2ApiAutoActivateRequest>,
) -> ApiResult<Json<Sub2ApiAutoActivateResponse>> {
    let base = payload.base_url.trim_end_matches('/');
    let execute_at = if payload.execute_at.trim().is_empty() {
        "04:00"
    } else {
        payload.execute_at.trim()
    };
    let mut parts = execute_at.split(':');
    let hour: u32 = parts
        .next()
        .unwrap_or("4")
        .parse()
        .map_err(|_| AppError::bad_request("execute_at 小时格式无效"))?;
    let minute: u32 = parts
        .next()
        .unwrap_or("0")
        .parse()
        .map_err(|_| AppError::bad_request("execute_at 分钟格式无效"))?;
    if hour > 23 || minute > 59 {
        return Err(AppError::bad_request("execute_at 时间超出范围"));
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|error| AppError::internal(error.to_string()))?;

    let mut scheduled_count = 0usize;
    let mut errors = Vec::new();

    for account_id in &payload.account_ids {
        let url = format!("{base}/api/v1/admin/accounts/{account_id}/scheduled-action");
        let body = serde_json::json!({
            "action": "enable_and_recover",
            "execute_at": chrono_format_next_run(hour, minute),
        });
        let response = client
            .put(&url)
            .bearer_auth(&payload.token)
            .json(&body)
            .send()
            .await;
        match response {
            Ok(resp) if resp.status().is_success() => {
                scheduled_count += 1;
            }
            Ok(resp) => {
                let status = resp.status();
                let body_text = resp.text().await.unwrap_or_default();
                errors.push(format!(
                    "账号 {account_id}: HTTP {} {}",
                    status.as_u16(),
                    &body_text[..body_text.len().min(120)]
                ));
            }
            Err(error) => {
                errors.push(format!("账号 {account_id}: {error}"));
            }
        }
    }

    let _ = &state;

    Ok(Json(Sub2ApiAutoActivateResponse {
        ok: !errors.is_empty() || scheduled_count > 0,
        scheduled_count,
        errors,
    }))
}

fn chrono_format_next_run(hour: u32, minute: u32) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let now_secs = now.as_secs();
    let today_start = now_secs - (now_secs % 86400);
    let run_today = today_start + (hour as u64) * 3600 + (minute as u64) * 60;
    let run_at = if run_today > now_secs {
        run_today
    } else {
        run_today + 86400
    };
    let days = run_at / 86400;
    let _ = days;
    format!("{}", run_at)
}

#[derive(Debug, Deserialize)]
pub struct Sub2ApiArchiveActionRequest {
    pub base_url: String,
    pub token: String,
    pub account_id: i64,
}

#[derive(Debug, Serialize)]
pub struct Sub2ApiArchiveActionResponse {
    pub ok: bool,
    pub error: Option<String>,
}

pub async fn sub2api_recycle_account(
    State(_state): State<AppState>,
    Json(payload): Json<Sub2ApiArchiveActionRequest>,
) -> ApiResult<Json<Sub2ApiArchiveActionResponse>> {
    match sub2api::recycle_account(&payload.base_url, &payload.token, payload.account_id).await {
        Ok(()) => Ok(Json(Sub2ApiArchiveActionResponse {
            ok: true,
            error: None,
        })),
        Err(error) => Ok(Json(Sub2ApiArchiveActionResponse {
            ok: false,
            error: Some(error.to_string()),
        })),
    }
}

pub async fn sub2api_restore_account(
    State(_state): State<AppState>,
    Json(payload): Json<Sub2ApiArchiveActionRequest>,
) -> ApiResult<Json<Sub2ApiArchiveActionResponse>> {
    match sub2api::restore_account(&payload.base_url, &payload.token, payload.account_id).await {
        Ok(()) => Ok(Json(Sub2ApiArchiveActionResponse {
            ok: true,
            error: None,
        })),
        Err(error) => Ok(Json(Sub2ApiArchiveActionResponse {
            ok: false,
            error: Some(error.to_string()),
        })),
    }
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
        interchange_source: None,
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
        interchange_source: presets[index].interchange_source.clone(),
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
        favorite: previous.favorite,
        switch_count: previous.switch_count,
    };
    presets[index] = preset.clone();

    persist_api_presets_async(&state.auth_manager, &presets).await?;

    let refreshed_current_auth = if was_active {
        let auth_targets = terminal_auth_write_targets(&state)?;
        let use_local_proxy = api_preset_enables_local_upstream_proxy_on_apply(&preset);
        let provider_base_url = api_provider_base_url_for_mode(&preset, use_local_proxy);
        let openai_api_key = auth_core::api_provider_api_key_for_mode(&preset, use_local_proxy);

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
            &provider_base_url,
            &api_provider_options(&preset),
            &config_targets,
            &managed_config_keys,
        )
        .await?;
        sync_api_model_catalogs(&state, &auth_targets, &config_targets).await?;

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
        interchange_source: presets[index].interchange_source.clone(),
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
    apply::capture_live_login_auth_into_presets_for_targets(&state, &auth_targets).await?;
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
    apply::capture_live_login_auth_into_presets_for_targets(&state, &auth_targets).await?;
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

// 与 refresh_auth_preset_quota 相同：上游调用可能刷新 token 或更新用量详情，
// 成功后落盘预设；若该预设正是当前登录，同步重写 auth.json 目标文件。
async fn persist_auth_preset_after_token_op(
    state: &AppState,
    auth_targets: &[TerminalAuthFiles],
    presets: &[StoredAuthPreset],
    index: usize,
    was_active: bool,
) -> ApiResult<()> {
    persist_auth_presets_async(&state.auth_manager, presets).await?;
    if was_active {
        write_login_auth_files(auth_targets, &presets[index].auth).await?;
    }
    Ok(())
}

/// 兑换一个 Codex 额度重置点数（对齐 sub2api 账号表格的「重置」）。
pub async fn consume_auth_preset_reset_credit(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let _active_config_guard = apply::lock_active_config_for_request(&state).await;
    let auth_files = terminal_auth_files(&state)?;
    let auth_targets = terminal_auth_write_targets(&state)?;
    apply::capture_live_login_auth_into_presets_for_targets(&state, &auth_targets).await?;
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

    let result =
        match auth_core::consume_codex_reset_credit(&state.proxy_manager, &mut presets[index].auth)
            .await
        {
            Ok(result) => result,
            Err(error) => {
                if presets[index].auth != original_auth {
                    persist_auth_presets_async(&state.auth_manager, &presets).await?;
                    if was_active {
                        write_login_auth_files(&auth_targets, &presets[index].auth).await?;
                    }
                }
                return Err(map_bad_request("兑换额度重置点数失败", error));
            }
        };
    // 重置成功后立即重拉用量窗口，把刷新后的窗口/点数写回预设详情。
    let _ = refresh_stored_auth_preset_quota(&state.proxy_manager, &mut presets[index]).await;
    persist_auth_preset_after_token_op(&state, &auth_targets, &presets, index, was_active).await?;

    Ok(Json(json!({
        "ok": true,
        "code": result.code,
        "windows_reset": result.windows_reset,
        "preset": preset_summary(
            &presets[index],
            read_current_auth_state(&auth_files.auth_file)
                .await
                .ok()
                .flatten()
                .as_ref()
                .and_then(CurrentAuthState::as_login),
        ),
    })))
}

/// 查询 Codex 邀请资格（对齐 sub2api 的「可邀请」入口）。
pub async fn get_auth_preset_codex_referral(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let _active_config_guard = apply::lock_active_config_for_request(&state).await;
    let auth_files = terminal_auth_files(&state)?;
    let auth_targets = terminal_auth_write_targets(&state)?;
    let mut presets = state.auth_manager.auth_presets_snapshot();
    let Some(index) = presets.iter().position(|preset| preset.id == preset_id) else {
        return Err(AppError::not_found("找不到指定的 auth 预设。"));
    };
    let was_active = read_current_auth_state(&auth_files.auth_file)
        .await
        .ok()
        .flatten()
        .as_ref()
        .and_then(CurrentAuthState::as_login)
        == Some(&presets[index].auth);
    let original_auth = presets[index].auth.clone();
    let plan_type = presets[index].details.plan_type.clone();

    let info = match auth_core::fetch_codex_referral_eligibility(
        &state.proxy_manager,
        &mut presets[index].auth,
        plan_type.as_deref(),
    )
    .await
    {
        Ok(info) => info,
        Err(error) => {
            if presets[index].auth != original_auth {
                persist_auth_presets_async(&state.auth_manager, &presets).await?;
                if was_active {
                    write_login_auth_files(&auth_targets, &presets[index].auth).await?;
                }
            }
            return Err(map_bad_request("查询邀请资格失败", error));
        }
    };
    persist_auth_preset_after_token_op(&state, &auth_targets, &presets, index, was_active).await?;
    Ok(Json(json!({ "ok": true, "referral": info })))
}

#[derive(Debug, Deserialize)]
pub struct CodexReferralSendPayload {
    pub email: String,
}

/// 发送 Codex 邀请（对齐 sub2api 的「邀请用户」）。
pub async fn send_auth_preset_codex_referral(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
    Json(payload): Json<CodexReferralSendPayload>,
) -> ApiResult<Json<Value>> {
    let _active_config_guard = apply::lock_active_config_for_request(&state).await;
    let auth_files = terminal_auth_files(&state)?;
    let auth_targets = terminal_auth_write_targets(&state)?;
    let mut presets = state.auth_manager.auth_presets_snapshot();
    let Some(index) = presets.iter().position(|preset| preset.id == preset_id) else {
        return Err(AppError::not_found("找不到指定的 auth 预设。"));
    };
    let was_active = read_current_auth_state(&auth_files.auth_file)
        .await
        .ok()
        .flatten()
        .as_ref()
        .and_then(CurrentAuthState::as_login)
        == Some(&presets[index].auth);
    let original_auth = presets[index].auth.clone();
    let plan_type = presets[index].details.plan_type.clone();

    if let Err(error) = auth_core::send_codex_referral_invite(
        &state.proxy_manager,
        &mut presets[index].auth,
        plan_type.as_deref(),
        &payload.email,
    )
    .await
    {
        if presets[index].auth != original_auth {
            persist_auth_presets_async(&state.auth_manager, &presets).await?;
            if was_active {
                write_login_auth_files(&auth_targets, &presets[index].auth).await?;
            }
        }
        return Err(map_bad_request("发送邀请失败", error));
    }
    persist_auth_preset_after_token_op(&state, &auth_targets, &presets, index, was_active).await?;
    Ok(Json(json!({ "ok": true })))
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

#[derive(Debug, Deserialize)]
pub struct CodexBrowserOAuthExchangeRequest {
    pub session_id: String,
    pub callback: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub state: String,
}

pub async fn start_codex_browser_oauth_session(
    State(state): State<AppState>,
) -> ApiResult<Json<CodexOAuthSessionResponse>> {
    let pending = create_codex_browser_login().map_err(|error| {
        AppError::bad_request(format!("生成 Codex 浏览器登录链接失败: {error:#}"))
    })?;
    let session = state.codex_oauth_manager.insert_browser_pending(
        &pending.authorize_url,
        &pending.code_verifier,
        &pending.state,
    );
    Ok(Json(codex_oauth_session_response(&session)))
}

pub async fn exchange_codex_browser_oauth_session(
    State(state): State<AppState>,
    Json(payload): Json<CodexBrowserOAuthExchangeRequest>,
) -> ApiResult<Json<CodexOAuthSessionResponse>> {
    let session = state
        .codex_oauth_manager
        .get(&payload.session_id)
        .ok_or_else(|| AppError::not_found("找不到指定的 Codex 浏览器登录会话。"))?;

    if session.flow != CodexOAuthFlow::Browser {
        return Err(AppError::bad_request(
            "该登录会话不是浏览器授权码会话，请使用设备码登录等待自动完成。",
        ));
    }
    if session.status != CodexOAuthSessionStatus::Pending {
        return Err(AppError::bad_request("该浏览器登录会话已结束，请重新发起登录。"));
    }

    let (callback_code, callback_state) = if payload.code.trim().is_empty() {
        parse_codex_browser_callback(&payload.callback).map_err(|error| {
            AppError::bad_request(format!("解析 Codex 浏览器回调失败: {error:#}"))
        })?
    } else {
        (payload.code.trim().to_string(), payload.state.trim().to_string())
    };

    let expected_state = session.state.as_deref().unwrap_or_default();
    let supplied_state = if callback_state.is_empty() {
        payload.state.trim().to_string()
    } else {
        callback_state
    };
    if expected_state.is_empty()
        || !constant_time_eq(supplied_state.as_bytes(), expected_state.as_bytes())
    {
        state
            .codex_oauth_manager
            .fail(&payload.session_id, "浏览器登录 state 校验失败。");
        return Err(AppError::bad_request("浏览器登录 state 校验失败，请重新发起登录。"));
    }

    let code_verifier = session
        .code_verifier
        .as_deref()
        .map(str::trim)
        .unwrap_or_default();
    if code_verifier.is_empty() {
        state
            .codex_oauth_manager
            .fail(&payload.session_id, "浏览器登录会话缺少 PKCE verifier。");
        return Err(AppError::bad_request("浏览器登录会话缺少 PKCE verifier，请重新发起登录。"));
    }

    match complete_codex_browser_login(&state.proxy_manager, &callback_code, code_verifier).await {
        Ok(auth) => {
            let mut details = derive_auth_preset_details(&auth);
            if details.login_method.is_none() {
                details.login_method = Some("OAuth".to_string());
            }
            state
                .codex_oauth_manager
                .complete(&payload.session_id, auth, details);
        }
        Err(error) => {
            state
                .codex_oauth_manager
                .fail(&payload.session_id, format!("兑换 Codex 浏览器登录 token 失败: {error:#}"));
        }
    }

    let updated = state
        .codex_oauth_manager
        .get(&payload.session_id)
        .ok_or_else(|| AppError::not_found("Codex 浏览器登录会话已失效。"))?;
    Ok(Json(codex_oauth_session_response(&updated)))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |accumulator, (left, right)| accumulator | (left ^ right))
        == 0
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

pub async fn toggle_api_preset_favorite(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
) -> ApiResult<Json<ToggleApiPresetFavoriteResponse>> {
    let mut presets = state.auth_manager.api_presets_snapshot();
    let Some(preset) = presets.iter_mut().find(|preset| preset.id == preset_id) else {
        return Err(AppError::not_found("找不到指定的 API 预设。"));
    };
    preset.favorite = !preset.favorite;
    let changed = preset.clone();
    persist_api_presets_async(&state.auth_manager, &presets).await?;

    let auth_files = terminal_auth_files(&state)?;
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

    Ok(Json(ToggleApiPresetFavoriteResponse {
        ok: true,
        preset: api_preset_summary(&changed, current_mode, current_api.as_ref()),
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
