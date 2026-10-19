use std::{collections::BTreeMap, path::Path};

use auth_core::*;
use axum::{
    Json,
    extract::Query,
    extract::{Path as AxumPath, State},
};
use serde::Serialize;
use serde_json::Value;
use tokio::fs;
use tokio::process::Command;
use toml_edit::DocumentMut;

use crate::{ApiResult, AppError, AppState};

use super::{
    TerminalAuthFiles, api_managed_config_keys, clear_config_providers, sync_api_model_catalogs,
    sync_api_preset_configs, sync_auth_preset_configs, terminal_auth_files,
    terminal_auth_write_targets, validate_auth_file, write_api_auth_files,
    write_claude_settings_files, write_login_auth_files, write_opencode_config_file,
};

#[derive(Debug, serde::Deserialize, Default)]
pub struct ApplyApiPresetQuery {
    #[serde(default = "default_true")]
    pub respect_saved_proxy_preference: bool,
    /// Optional project working directory whose ancestor ``.codex/config.toml``
    /// may override the global config and silently defeat preset switching.
    /// When provided, the detected local config is synced to match the preset.
    #[serde(default)]
    pub project_path: String,
    /// Skip restarting the running Codex app-server daemon after the write.
    /// Temporary `webclx run` switches set this: the launched agent reads the
    /// files directly, and restarting the daemon between apply/restore would
    /// kill the just-launched session.
    #[serde(default)]
    pub skip_daemon_restart: bool,
}

#[derive(Debug, serde::Deserialize, Default)]
pub struct ApplyAuthPresetQuery {
    /// See [`ApplyApiPresetQuery::project_path`].
    #[serde(default)]
    pub project_path: String,
    /// See [`ApplyApiPresetQuery::skip_daemon_restart`].
    #[serde(default)]
    pub skip_daemon_restart: bool,
}

fn default_true() -> bool {
    true
}

/// Codex rotates OAuth refresh tokens inside the live `auth.json`; the stored
/// preset still holds the now-revoked old token family. Before any path that
/// overwrites or removes `auth.json` (or refreshes stored OAuth tokens), fold
/// the rotated tokens back into the matching preset so the next switch does
/// not reuse a stale refresh token and trigger `refresh_token_reused`.
pub(super) async fn capture_live_login_auth_into_presets(state: &AppState) -> ApiResult<()> {
    let auth_targets = terminal_auth_write_targets(state)?;
    capture_live_login_auth_into_presets_for_targets(state, &auth_targets).await
}

pub(super) async fn capture_live_login_auth_into_presets_for_targets(
    state: &AppState,
    auth_targets: &[TerminalAuthFiles],
) -> ApiResult<()> {
    let mut presets = state.auth_manager.auth_presets_snapshot();
    if presets.is_empty() {
        return Ok(());
    }
    let mut captured = Vec::new();
    for target in auth_targets {
        let Ok(Some(current)) = read_current_auth_state(&target.auth_file).await else {
            continue;
        };
        let Some(live) = current.as_login() else {
            continue;
        };
        if let Some(preset_id) = capture_live_login_auth_into_snapshot(&mut presets, live)
            && !captured.contains(&preset_id)
        {
            captured.push(preset_id);
        }
    }
    if captured.is_empty() {
        return Ok(());
    }
    persist_auth_presets_async(&state.auth_manager, &presets)
        .await
        .map_err(|error| AppError::internal(format!("回写 auth 预设失败: {error}")))?;
    for preset_id in captured {
        tracing::info!(preset_id = %preset_id, "auth 预设已回写实时 auth.json 中轮换后的 OAuth token");
    }
    Ok(())
}

/// Returns the preset id when `live` was folded into the stored snapshot.
/// Team workspaces share one `account_id` across users, so a preset only
/// matches when the id_token emails match too (both missing is acceptable).
fn capture_live_login_auth_into_snapshot(
    presets: &mut [StoredAuthPreset],
    live: &AuthFile,
) -> Option<String> {
    let index = login_auth_preset_match_index(presets, live)?;
    let preset = &mut presets[index];
    if preset.auth == *live {
        return None;
    }
    if let (Ok(preset_refresh), Ok(live_refresh)) = (
        time::OffsetDateTime::parse(
            preset.auth.last_refresh.trim(),
            &time::format_description::well_known::Rfc3339,
        ),
        time::OffsetDateTime::parse(
            live.last_refresh.trim(),
            &time::format_description::well_known::Rfc3339,
        ),
    ) && preset_refresh > live_refresh
    {
        return None;
    }
    preset.auth = live.clone();
    preset.saved_at = current_timestamp_secs();
    Some(preset.id.clone())
}

/// Shared login-preset matcher: non-empty equal `account_id` AND
/// case-insensitive equal id_token email (both missing counts as a match).
/// Returns the unique match index; warns and returns `None` on multiple
/// matches because Team workspaces share one `account_id` across users.
pub(super) fn login_auth_preset_match_index(
    presets: &[StoredAuthPreset],
    live: &AuthFile,
) -> Option<usize> {
    let live_account_id = live.tokens.account_id.trim();
    if live_account_id.is_empty() {
        return None;
    }
    let live_email = auth_summary(live).email;
    let matches: Vec<usize> = presets
        .iter()
        .enumerate()
        .filter_map(|(index, preset)| {
            if preset.auth.tokens.account_id.trim() != live_account_id {
                return None;
            }
            let preset_email = auth_summary(&preset.auth).email;
            let email_matches = match (&live_email, &preset_email) {
                (Some(live), Some(preset)) => live.eq_ignore_ascii_case(preset),
                (None, None) => true,
                _ => false,
            };
            email_matches.then_some(index)
        })
        .collect();
    match matches.as_slice() {
        [index] => Some(*index),
        [] => None,
        _ => {
            tracing::warn!(
                account_id = %live_account_id,
                "实时 auth.json 匹配到多个 auth 预设，跳过账号匹配"
            );
            None
        }
    }
}

/// Codex 0.157+ hosts TUI sessions inside a persistent `app-server
/// --managed-daemon` process that keeps auth/config state in memory and only
/// reloads credentials for the already-active account. Without a restart, new
/// Codex sessions keep using the account the daemon loaded at startup and an
/// applied preset appears to take no effect. Restart each target user's daemon
/// (only when it is running) so subsequent sessions read the just-written
/// files. Returns true when at least one daemon was restarted.
pub(super) async fn restart_running_codex_daemons(auth_targets: &[TerminalAuthFiles]) -> bool {
    let mut restarted_any = false;
    for target in auth_targets {
        match restart_codex_daemon_for_target(target).await {
            Ok(true) => restarted_any = true,
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(
                    user = %target.user_name,
                    %error,
                    "restart Codex app-server daemon failed after preset apply"
                );
            }
        }
    }
    restarted_any
}

async fn restart_codex_daemon_for_target(target: &TerminalAuthFiles) -> ApiResult<bool> {
    let Some(home) = target
        .auth_file
        .parent()
        .and_then(|codex_dir| codex_dir.parent())
    else {
        return Ok(false);
    };
    let pid_file = home.join(".codex/app-server-daemon/daemon.pid");
    let Ok(content) = fs::read_to_string(&pid_file).await else {
        return Ok(false);
    };
    let pid = serde_json::from_str::<Value>(&content)
        .ok()
        .and_then(|value| value.get("pid").and_then(Value::as_u64));
    let Some(pid) = pid else {
        return Ok(false);
    };
    // Guard against stale pid files: only restart when the recorded pid is a
    // live codex app-server process.
    let cmdline = fs::read(format!("/proc/{pid}/cmdline"))
        .await
        .unwrap_or_default();
    if !String::from_utf8_lossy(&cmdline).contains("app-server") {
        return Ok(false);
    }
    // Run through the target user's login shell so codex resolves HOME/PATH
    // the same way terminal-spawned processes do. kill_on_drop ensures a
    // timed-out restart does not leave a detached `runuser`/`codex` child.
    let mut restart = Command::new("runuser");
    restart
        .arg("-u")
        .arg(&target.user_name)
        .arg("--")
        .arg("/bin/bash")
        .arg("-lc")
        .arg("codex app-server daemon restart");
    match crate::process_exec::bounded_output_tokio(
        restart,
        std::time::Duration::from_secs(20),
        64 * 1024,
    )
    .await
    {
        Ok(output) if output.status.success() => Ok(true),
        Ok(output) => Err(AppError::internal(format!(
            "codex app-server daemon restart 失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))),
        Err(error) if error.to_string().contains("命令执行超时") => {
            Err(AppError::internal("codex app-server daemon restart 超时".to_string()))
        }
        Err(error) => Err(AppError::internal(format!("执行 runuser 失败: {error}"))),
    }
}

pub(super) async fn lock_active_config_for_request(
    state: &AppState,
) -> tokio::sync::OwnedMutexGuard<()> {
    state.auth_manager.lock_active_config_write().await
}

#[derive(Debug)]
pub(super) struct ApiPresetTargetVerification {
    pub matches: bool,
    pub current_mode: CurrentAuthMode,
    pub current_api: Option<CurrentApiState>,
    pub config_values: BTreeMap<String, String>,
    pub mismatches: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct VerifyApiPresetResponse {
    pub ok: bool,
    pub preset_id: String,
    pub name: String,
    pub matches: bool,
    pub current_mode: CurrentAuthMode,
    pub current_api: Option<CurrentApiSummary>,
    pub model: Option<String>,
    pub mismatches: Vec<String>,
}

pub async fn apply_auth_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
    Query(query): Query<ApplyAuthPresetQuery>,
) -> ApiResult<Json<ApplyAuthPresetResponse>> {
    let _active_config_guard = lock_active_config_for_request(&state).await;
    let (response, _) = apply_auth_preset_locked(
        &state,
        &preset_id,
        &query.project_path,
        query.skip_daemon_restart,
    )
    .await?;
    Ok(Json(response))
}

pub(crate) async fn apply_auth_preset_locked(
    state: &AppState,
    preset_id: &str,
    project_path: &str,
    skip_daemon_restart: bool,
) -> ApiResult<(ApplyAuthPresetResponse, StoredAuthPreset)> {
    let auth_files = terminal_auth_files(&state)?;
    let auth_targets = terminal_auth_write_targets(&state)?;
    apply_auth_preset_to_targets(
        state,
        preset_id,
        project_path,
        skip_daemon_restart,
        &auth_files,
        &auth_targets,
    )
    .await
}

pub(super) async fn apply_auth_preset_to_targets(
    state: &AppState,
    preset_id: &str,
    project_path: &str,
    skip_daemon_restart: bool,
    auth_files: &TerminalAuthFiles,
    auth_targets: &[TerminalAuthFiles],
) -> ApiResult<(ApplyAuthPresetResponse, StoredAuthPreset)> {
    capture_live_login_auth_into_presets_for_targets(state, auth_targets).await?;
    let presets = state.auth_manager.auth_presets_snapshot();
    let preset = presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .cloned()
        .ok_or_else(|| AppError::not_found("找不到指定的 auth 预设。"))?;
    let default_config_entries = state.workspace_settings.codex_default_config_entries();
    let default_config_pairs = default_config_entries
        .iter()
        .map(|entry| (entry.key.as_str(), entry.value.as_str()))
        .collect::<Vec<_>>();
    let config_targets =
        resolve_effective_preset_config_targets(&default_config_pairs, &preset.config_overrides)
            .map_err(|error| AppError::internal(format!("auth 预设 config 覆盖无效: {error}")))?;

    write_login_auth_files(auth_targets, &preset.auth).await?;
    sync_auth_preset_configs(auth_targets, &config_targets).await?;

    bump_auth_preset_switch_count(&state, &preset.id).await?;

    // A project-local .codex/config.toml overrides the global config in Codex,
    // so switching the global file alone is ineffective. When a project path is
    // provided, sync the detected local config to the same preset targets.
    let local_config_file =
        sync_local_codex_config(state, project_path, None, None, None, &config_targets).await?;

    let codex_daemon_restarted =
        !skip_daemon_restart && restart_running_codex_daemons(auth_targets).await;

    let response = ApplyAuthPresetResponse {
        ok: true,
        deferred: false,
        preset_id: preset.id.clone(),
        name: preset.name.clone(),
        auth_file: auth_files.auth_file.display().to_string(),
        config_file: auth_files.config_file.display().to_string(),
        local_config_file,
        codex_daemon_restarted,
    };
    Ok((response, preset))
}

pub async fn apply_api_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
    Query(query): Query<ApplyApiPresetQuery>,
) -> ApiResult<Json<ApplyApiPresetResponse>> {
    let _active_config_guard = lock_active_config_for_request(&state).await;
    let _respect_saved_proxy_preference = query.respect_saved_proxy_preference;
    let project_path = query.project_path.clone();
    let (response, _) =
        apply_api_preset_locked(&state, &preset_id, &project_path, query.skip_daemon_restart)
            .await?;
    Ok(Json(response))
}

/// Applies one API preset while the caller holds `lock_active_config_write`.
/// Keeping the lock outside this function lets native Codex task launches
/// retain it until the child confirms which model it loaded.
pub(crate) async fn apply_api_preset_locked(
    state: &AppState,
    preset_id: &str,
    project_path: &str,
    skip_daemon_restart: bool,
) -> ApiResult<(ApplyApiPresetResponse, StoredApiPreset)> {
    let auth_files = terminal_auth_files(&state)?;
    let auth_targets = terminal_auth_write_targets(&state)?;
    let presets = state.auth_manager.api_presets_snapshot();
    let preset = presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .cloned()
        .ok_or_else(|| AppError::not_found("找不到指定的 API 预设。"))?;
    let default_config_entries = state.workspace_settings.codex_default_config_entries();
    let default_config_pairs = default_config_entries
        .iter()
        .map(|entry| (entry.key.as_str(), entry.value.as_str()))
        .collect::<Vec<_>>();
    let config_targets =
        resolve_effective_preset_config_targets(&default_config_pairs, &preset.config_overrides)
            .map_err(|error| AppError::internal(format!("API 预设 config 覆盖无效: {error}")))?;
    let managed_config_keys = api_managed_config_keys(&default_config_pairs, &presets);
    let use_local_proxy = api_preset_enables_local_upstream_proxy_on_apply(&preset);
    let provider_base_url = api_provider_base_url_for_mode(&preset, use_local_proxy);
    let api_key = auth_core::api_provider_api_key_for_mode(&preset, use_local_proxy);
    capture_live_login_auth_into_presets_for_targets(&state, &auth_targets).await?;
    write_api_auth_files(
        &auth_targets,
        &ApiAuthFile {
            openai_api_key: api_key.clone(),
        },
    )
    .await?;
    sync_api_preset_configs(
        &auth_targets,
        &preset.provider_name,
        &provider_base_url,
        &api_provider_options(&preset),
        &config_targets,
        &managed_config_keys,
    )
    .await?;
    sync_api_model_catalogs(state, &auth_targets, &config_targets).await?;

    let verification = verify_api_preset_targets(&auth_targets, &preset, &config_targets).await?;
    if !verification.matches {
        return Err(AppError::internal(format!(
            "API 预设写入后校验失败: {}",
            verification.mismatches.join("；")
        )));
    }

    sync_active_api_proxy_preset(&state, &preset)?;
    bump_api_preset_switch_count(&state, &preset.id).await?;

    // A project-local .codex/config.toml overrides the global config in Codex,
    // so switching the global file alone is ineffective. When a project path is
    // provided, sync the detected local config to the same API preset targets.
    let local_config_file = sync_local_codex_config(
        state,
        project_path,
        Some((
            preset.provider_name.as_str(),
            provider_base_url.as_str(),
            &api_provider_options(&preset),
        )),
        Some(api_key.as_str()),
        Some(managed_config_keys.as_slice()),
        &config_targets,
    )
    .await?;

    let codex_daemon_restarted =
        !skip_daemon_restart && restart_running_codex_daemons(&auth_targets).await;

    let response = ApplyApiPresetResponse {
        ok: true,
        deferred: false,
        preset_id: preset.id.clone(),
        name: preset.name.clone(),
        auth_file: auth_files.auth_file.display().to_string(),
        config_file: auth_files.config_file.display().to_string(),
        local_config_file,
        codex_daemon_restarted,
    };
    Ok((response, preset))
}

pub(super) fn sync_active_api_proxy_preset(
    state: &AppState,
    preset: &StoredApiPreset,
) -> ApiResult<()> {
    let mut settings = state.auth_manager.upstream_proxy_settings();
    if api_preset_enables_local_upstream_proxy_on_apply(preset) {
        settings.codex_api_proxy_enabled = true;
    }
    settings.active_api_proxy_preset_id = Some(preset.id.clone());
    persist_upstream_proxy_settings(&state.auth_manager, settings)
        .map_err(|error| AppError::internal(format!("保存 API 动态中转上游失败: {error}")))
}

pub async fn verify_api_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
) -> ApiResult<Json<VerifyApiPresetResponse>> {
    let _active_config_guard = lock_active_config_for_request(&state).await;
    let auth_targets = terminal_auth_write_targets(&state)?;
    let presets = state.auth_manager.api_presets_snapshot();
    let preset = presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .cloned()
        .ok_or_else(|| AppError::not_found("找不到指定的 API 预设。"))?;
    let default_config_entries = state.workspace_settings.codex_default_config_entries();
    let default_config_pairs = default_config_entries
        .iter()
        .map(|entry| (entry.key.as_str(), entry.value.as_str()))
        .collect::<Vec<_>>();
    let config_targets =
        resolve_effective_preset_config_targets(&default_config_pairs, &preset.config_overrides)
            .map_err(|error| AppError::internal(format!("API 预设 config 覆盖无效: {error}")))?;
    let verification = verify_api_preset_targets(&auth_targets, &preset, &config_targets).await?;
    let model = verification.config_values.get("model").cloned();

    Ok(Json(VerifyApiPresetResponse {
        ok: true,
        preset_id: preset.id,
        name: preset.name,
        matches: verification.matches,
        current_mode: verification.current_mode,
        current_api: verification.current_api.as_ref().map(current_api_summary),
        model,
        mismatches: verification.mismatches,
    }))
}

pub(super) async fn verify_api_preset_targets(
    auth_targets: &[TerminalAuthFiles],
    preset: &StoredApiPreset,
    config_targets: &[ResolvedConfigTarget],
) -> ApiResult<ApiPresetTargetVerification> {
    let mut primary_mode = CurrentAuthMode::None;
    let mut primary_api = None;
    let mut primary_config_values = BTreeMap::new();
    let mut mismatches = Vec::new();

    for (index, target) in auth_targets.iter().enumerate() {
        let current_auth = read_current_auth_state(&target.auth_file)
            .await
            .map_err(|error| {
                AppError::internal(format!(
                    "回读用户 `{}` 的 auth.json 失败: {error}",
                    target.user_name
                ))
            })?;
        let current_config = read_current_config_provider(&target.config_file)
            .await
            .map_err(|error| {
                AppError::internal(format!(
                    "回读用户 `{}` 的 config.toml 失败: {error}",
                    target.user_name
                ))
            })?;
        let current_mode = derive_current_mode(current_auth.as_ref(), current_config.as_ref());
        let current_api = derive_current_api_state(
            current_config.as_ref(),
            current_auth.as_ref(),
            std::slice::from_ref(preset),
        );
        let config_values = current_config
            .as_ref()
            .map(|config| config.config_values.clone())
            .unwrap_or_default();

        if index == 0 {
            primary_mode = current_mode;
            primary_api = current_api.clone();
            primary_config_values = config_values.clone();
        }

        if !api_preset_has_current_applied_api_credentials(
            preset,
            current_mode,
            current_api.as_ref(),
        ) {
            mismatches.push(format!(
                "用户 `{}` 的 API 凭据、Base URL 或 wire API 不匹配",
                target.user_name
            ));
        }

        for config_target in config_targets {
            let expected = config_target_comparable_text(&config_target.value);
            if config_values.get(&config_target.key) != Some(&expected) {
                mismatches.push(format!(
                    "用户 `{}` 的 config 键 `{}` 不匹配",
                    target.user_name, config_target.key
                ));
            }
        }
    }

    Ok(ApiPresetTargetVerification {
        matches: mismatches.is_empty(),
        current_mode: primary_mode,
        current_api: primary_api,
        config_values: primary_config_values,
        mismatches,
    })
}

fn config_target_comparable_text(value: &str) -> String {
    let trimmed = value.trim();
    let probe = format!("value = {trimmed}\n");
    probe
        .parse::<DocumentMut>()
        .ok()
        .and_then(|doc| doc.get("value").and_then(|item| item.as_value()).cloned())
        .map(|value| match value {
            toml_edit::Value::String(value) => value.value().to_string(),
            toml_edit::Value::Integer(value) => value.value().to_string(),
            toml_edit::Value::Float(value) => value.value().to_string(),
            toml_edit::Value::Boolean(value) => value.value().to_string(),
            value => value.to_string(),
        })
        .unwrap_or_else(|| trimmed.to_string())
}

pub async fn apply_claude_preset(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
) -> ApiResult<Json<ApplyClaudePresetResponse>> {
    let _active_config_guard = lock_active_config_for_request(&state).await;
    let (response, _) = apply_claude_preset_locked(&state, &preset_id).await?;
    Ok(Json(response))
}

pub(crate) async fn apply_claude_preset_locked(
    state: &AppState,
    preset_id: &str,
) -> ApiResult<(ApplyClaudePresetResponse, StoredClaudePreset)> {
    let auth_files = terminal_auth_files(&state)?;
    let auth_targets = terminal_auth_write_targets(&state)?;
    let presets = state.auth_manager.claude_presets_snapshot();
    let preset = presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .cloned()
        .ok_or_else(|| AppError::not_found("找不到指定的 Claude 预设。"))?;

    let effective_preset = claude_preset_with_global_defaults(&state.workspace_settings, &preset)?;
    activate_dynamic_claude_relay_if_needed(&state, &preset)?;
    write_claude_preset_to_targets(&auth_targets, &effective_preset).await?;

    bump_claude_preset_switch_count(&state, &preset.id).await?;

    let response = ApplyClaudePresetResponse {
        ok: true,
        deferred: false,
        preset_id: preset.id.clone(),
        name: preset.name.clone(),
        settings_file: auth_files.claude_settings_file.display().to_string(),
    };
    Ok((response, preset))
}

pub(super) fn claude_preset_with_global_defaults(
    settings: &crate::settings::SettingsManager,
    preset: &StoredClaudePreset,
) -> ApiResult<StoredClaudePreset> {
    let defaults = settings.claude_default_config_entries();
    let default_pairs = defaults
        .iter()
        .map(|entry| (entry.key.as_str(), entry.value.as_str()))
        .collect::<Vec<_>>();
    let mut effective = preset.clone();
    effective.config_overrides = resolve_effective_claude_config_overrides(&default_pairs, preset)
        .map_err(|error| {
            AppError::internal(format!("Claude 全局默认选项或预设覆盖无效: {error}"))
        })?;
    Ok(effective)
}

pub(super) async fn write_claude_preset_to_targets(
    auth_targets: &[TerminalAuthFiles],
    preset: &StoredClaudePreset,
) -> ApiResult<()> {
    match effective_claude_access_mode(preset) {
        ClaudeAccessMode::Direct => {
            write_claude_settings_files(auth_targets, preset).await?;
        }
        ClaudeAccessMode::AnthropicProxy
        | ClaudeAccessMode::AnthropicRelay
        | ClaudeAccessMode::OpenaiChat
        | ClaudeAccessMode::OpenaiResponses => {
            let local_proxy_base_url = claude_provider_base_url_for_mode(preset, true);
            let local_proxy_token = local_proxy_claude_token_for_preset_id(&preset.id);
            for target in auth_targets {
                let settings =
                    read_claude_settings_document_for_apply(&target.claude_settings_file).await?;
                let next = set_claude_settings_in_value_with_endpoint(
                    settings,
                    preset,
                    &local_proxy_base_url,
                    &local_proxy_token,
                )
                .map_err(|error| {
                    AppError::internal(format!("更新 Claude settings 失败: {error}"))
                })?;
                write_json_value_for_apply(&target.claude_settings_file, &next, "Claude settings")
                    .await?;
                write_claude_onboarding_bypass_for_settings(&target.claude_settings_file).await?;
            }
        }
    }
    Ok(())
}

pub(super) fn activate_dynamic_claude_relay_if_needed(
    state: &AppState,
    preset: &StoredClaudePreset,
) -> ApiResult<()> {
    if effective_claude_access_mode(preset) == ClaudeAccessMode::Direct {
        return Ok(());
    }
    let mut settings = state.auth_manager.upstream_proxy_settings();
    settings.claude_proxy_enabled = true;
    settings.active_claude_proxy_preset_id = Some(preset.id.clone());
    persist_upstream_proxy_settings(&state.auth_manager, settings)
        .map_err(|error| AppError::internal(format!("保存 Claude 动态中转上游失败: {error}")))
}

pub async fn apply_claude_preset_to_opencode(
    State(state): State<AppState>,
    AxumPath(preset_id): AxumPath<String>,
) -> ApiResult<Json<ApplyClaudePresetResponse>> {
    let presets = state.auth_manager.claude_presets_snapshot();
    let preset = presets
        .iter()
        .find(|preset| preset.id == preset_id)
        .cloned()
        .ok_or_else(|| AppError::not_found("找不到指定的 Claude 预设。"))?;

    let opencode_path = state.workspace_root().join("opencode.json");
    write_opencode_config_file(&opencode_path, &preset).await?;

    Ok(Json(ApplyClaudePresetResponse {
        ok: true,
        deferred: false,
        preset_id: preset.id,
        name: preset.name,
        settings_file: opencode_path.display().to_string(),
    }))
}

pub async fn apply_current_auth(
    State(state): State<AppState>,
    Json(payload): Json<ApplyCurrentAuthRequest>,
) -> ApiResult<Json<ApplyCurrentAuthResponse>> {
    let _active_config_guard = lock_active_config_for_request(&state).await;
    let auth_files = terminal_auth_files(&state)?;
    let auth_targets = terminal_auth_write_targets(&state)?;
    validate_auth_file(&payload.auth)?;
    capture_live_login_auth_into_presets_for_targets(&state, &auth_targets).await?;
    write_login_auth_files(&auth_targets, &payload.auth).await?;
    clear_config_providers(&auth_targets).await?;

    let codex_daemon_restarted = restart_running_codex_daemons(&auth_targets).await;

    Ok(Json(ApplyCurrentAuthResponse {
        ok: true,
        auth_file: auth_files.auth_file.display().to_string(),
        config_file: auth_files.config_file.display().to_string(),
        account_id: payload.auth.tokens.account_id,
        codex_daemon_restarted,
    }))
}

/// Log out of the official Codex CLI account by removing only auth.json.
pub async fn clear_current_auth(
    State(state): State<AppState>,
) -> ApiResult<Json<super::CurrentAuthActionResponse>> {
    let _active_config_guard = lock_active_config_for_request(&state).await;
    let auth_files = terminal_auth_files(&state)?;
    let current = auth_core::read_current_auth_state(&auth_files.auth_file)
        .await
        .map_err(AppError::bad_request)?;
    let Some(auth_core::CurrentAuthState::Login(auth)) = current else {
        return Err(AppError::bad_request("当前没有可注销的 Codex OAuth 登录。"));
    };
    capture_live_login_auth_into_presets(&state).await?;
    match fs::remove_file(&auth_files.auth_file).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(AppError::internal(format!("清除 auth.json 失败: {error}"))),
    }
    let auth_targets = terminal_auth_write_targets(&state)?;
    clear_config_providers(&auth_targets).await?;
    let codex_daemon_restarted = restart_running_codex_daemons(&auth_targets).await;
    Ok(Json(super::CurrentAuthActionResponse {
        ok: true,
        account: Some(auth_core::auth_summary(&auth)),
        preset: None,
        auth_file: auth_files.auth_file.display().to_string(),
        codex_daemon_restarted,
    }))
}

async fn read_claude_settings_document_for_apply(path: &Path) -> ApiResult<Value> {
    let content = match fs::read_to_string(path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(AppError::internal(format!("读取 Claude settings 失败: {error}")));
        }
    };
    parse_claude_settings_document(&content)
        .map_err(|error| AppError::bad_request(format!("Claude settings 无效: {error}")))
}

async fn write_json_value_for_apply(path: &Path, value: &Value, label: &str) -> ApiResult<()> {
    let content = serde_json::to_vec_pretty(value)
        .map_err(|error| AppError::internal(format!("序列化 {label} 失败: {error}")))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| AppError::internal(format!("创建 {label} 目录失败: {error}")))?;
    }
    fs::write(path, content)
        .await
        .map_err(|error| AppError::internal(format!("写入 {label} 失败: {error}")))
}

pub(super) async fn write_claude_onboarding_bypass_for_settings(
    settings_path: &Path,
) -> ApiResult<()> {
    let Some(claude_dir) = settings_path.parent() else {
        return Ok(());
    };
    let bypass_path = claude_dir
        .parent()
        .unwrap_or(claude_dir)
        .join(CLAUDE_ONBOARDING_BYPASS_FILE);
    let content = match fs::read_to_string(&bypass_path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(AppError::internal(format!("读取 Claude onboarding 状态失败: {error}")));
        }
    };
    let next = set_claude_onboarding_bypass_in_content(&content)?;
    write_json_value_for_apply(&bypass_path, &next, "Claude onboarding").await
}

fn set_claude_onboarding_bypass_in_content(content: &str) -> ApiResult<Value> {
    let mut root = if content.trim().is_empty() {
        serde_json::Map::new()
    } else {
        let value: Value = serde_json::from_str(content).map_err(|error| {
            AppError::internal(format!(
                "读取 Claude onboarding 状态失败: .claude.json 格式无效: {error}"
            ))
        })?;
        value.as_object().cloned().ok_or_else(|| {
            AppError::internal("读取 Claude onboarding 状态失败: .claude.json 顶层必须是对象。")
        })?
    };
    root.insert("hasCompletedOnboarding".to_string(), Value::Bool(true));
    Ok(Value::Object(root))
}

async fn bump_auth_preset_switch_count(state: &AppState, preset_id: &str) -> ApiResult<()> {
    let mut presets = state.auth_manager.auth_presets_snapshot();
    if !bump_switch_count(&mut presets, preset_id) {
        return Ok(());
    }
    persist_auth_presets_async(&state.auth_manager, &presets)
        .await
        .map_err(|error| AppError::internal(format!("更新 auth 预设使用计数失败: {error}")))
}

async fn bump_api_preset_switch_count(state: &AppState, preset_id: &str) -> ApiResult<()> {
    let mut presets = state.auth_manager.api_presets_snapshot();
    if !bump_switch_count(&mut presets, preset_id) {
        return Ok(());
    }
    persist_api_presets_async(&state.auth_manager, &presets)
        .await
        .map_err(|error| AppError::internal(format!("更新 API 预设使用计数失败: {error}")))
}

async fn bump_claude_preset_switch_count(state: &AppState, preset_id: &str) -> ApiResult<()> {
    let mut presets = state.auth_manager.claude_presets_snapshot();
    if !bump_switch_count(&mut presets, preset_id) {
        return Ok(());
    }
    persist_claude_presets_async(&state.auth_manager, &presets)
        .await
        .map_err(|error| AppError::internal(format!("更新 Claude 预设使用计数失败: {error}")))
}

/// Return whether a project-local config already owns model/provider routing.
/// Merely having a `.codex/config.toml` for project documentation or features
/// must not cause a preset apply to inject user-level model settings into it.
pub(super) fn local_codex_config_requires_preset_sync(
    content: &str,
) -> Result<bool, toml_edit::TomlError> {
    let doc = content.parse::<DocumentMut>()?;
    Ok(["model", "model_provider", "provider", "model_providers"]
        .iter()
        .any(|key| doc.get(key).is_some()))
}

/// Resolve a project-local `.codex/config.toml` directly inside `project_path`.
///
/// Codex reads a project-local config at `<project>/.codex/config.toml`; when it
/// exists it overrides the global `~/.codex/config.toml`, so writing only the
/// global file can leave account switching ineffective (e.g. `../stockScreener`).
/// Returns the resolved local path only when the file already exists. The project
/// directory itself is checked — no ancestor walk — to avoid touching unrelated
/// configs that happen to live in parent directories.
pub(super) fn find_local_codex_config(project_path: &str) -> Option<std::path::PathBuf> {
    use auth_core::CONFIG_FILE_RELATIVE_PATH;
    let trimmed = project_path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let start = std::path::Path::new(trimmed);
    let dir = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(start)
    };
    let candidate = dir.join(CONFIG_FILE_RELATIVE_PATH);
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

/// Sync a detected project-local `.codex/config.toml` only when it already owns
/// model/provider routing that can override the user-level preset. A project
/// config used solely for documentation, features, MCP, or other project settings
/// is left untouched. For API presets the matching local `.codex/auth.json` (if
/// present) is also rewritten and the referenced model catalog is refreshed.
///
/// `api_context` is `Some((provider_name, base_url, provider_options))` for API
/// presets and `None` for auth presets. Returns the local config path when synced.
pub(super) async fn sync_local_codex_config(
    state: &AppState,
    project_path: &str,
    api_context: Option<(&str, &str, &auth_core::ApiProviderOptions)>,
    api_key: Option<&str>,
    managed_keys: Option<&[String]>,
    config_targets: &[auth_core::ResolvedConfigTarget],
) -> ApiResult<Option<String>> {
    let Some(local_config) = find_local_codex_config(project_path) else {
        return Ok(None);
    };
    let local_content = fs::read_to_string(&local_config)
        .await
        .map_err(|error| AppError::internal(format!("读取项目本地 config.toml 失败: {error}")))?;
    let requires_sync = local_codex_config_requires_preset_sync(&local_content)
        .map_err(|error| AppError::internal(format!("解析项目本地 config.toml 失败: {error}")))?;
    if !requires_sync {
        return Ok(None);
    }

    let user_name = state.workspace_settings.terminal_user();

    if let Some((provider_name, base_url, provider_options)) = api_context {
        auth_core::sync_api_preset_config(
            &local_config,
            provider_name,
            base_url,
            provider_options,
            config_targets,
            managed_keys.unwrap_or(&[]),
        )
        .await
        .map_err(|error| AppError::internal(format!("同步项目本地 config.toml 失败: {error}")))?;

        // Refresh the model catalog referenced by the local config so the
        // switched model resolves its context window / capabilities correctly.
        if config_targets.iter().any(|target| {
            target.key.eq_ignore_ascii_case("model") && !target.value.trim().is_empty()
        }) {
            let bundled_catalog = super::read_codex_bundled_model_catalog(&user_name)
                .await
                .ok();
            auth_core::sync_api_model_catalog(
                &local_config,
                config_targets,
                bundled_catalog.as_ref(),
                super::model_catalog_policy::load(state)
                    .await?
                    .suppress_comp_hash_changes,
            )
            .await
            .map_err(|error| {
                AppError::internal(format!("同步项目本地 model_catalog 失败: {error}"))
            })?;
        }

        // Also switch the local API key if a project-local auth.json exists.
        if let Some(key) = api_key {
            let local_auth = local_config.with_file_name("auth.json");
            if local_auth.is_file() {
                auth_core::write_api_auth_file(
                    &local_auth,
                    &auth_core::ApiAuthFile {
                        openai_api_key: key.to_string(),
                    },
                )
                .await
                .map_err(|error| {
                    AppError::internal(format!("同步项目本地 auth.json 失败: {error}"))
                })?;
            }
        }
    } else {
        auth_core::sync_auth_preset_config(&local_config, config_targets)
            .await
            .map_err(|error| {
                AppError::internal(format!("同步项目本地 config.toml 失败: {error}"))
            })?;
    }

    tracing::info!(
        "Synced project-local Codex config to match preset switch: {}",
        local_config.display()
    );
    Ok(Some(local_config.display().to_string()))
}
