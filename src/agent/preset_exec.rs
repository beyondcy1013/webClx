use auth_core::{
    ApiPresetLookup, StoredApiPreset, derive_current_api_state, read_current_auth_state,
    read_current_config_provider, select_api_preset_index,
};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    ApiResult, AppError, AppState,
    codex_task::{
        CodexTaskMode, CodexTaskPresetSelector, CodexTaskRecord, CodexTaskStatus,
        CreateCodexTaskRequest,
    },
    runtime_paths,
};

const DEFAULT_TIMEOUT_SECS: u64 = 600;
const MAX_TIMEOUT_SECS: u64 = 7_200;
const MAX_TASK_BYTES: usize = 128 * 1024;
const MAX_SCHEMA_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct ApiPresetSelector {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub index: Option<isize>,
    #[serde(default)]
    pub current: bool,
}

#[derive(Debug, Deserialize)]
pub struct ExecWithPresetRequest {
    pub preset: ApiPresetSelector,
    pub cwd: String,
    pub task: String,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default)]
    pub output_schema: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct ExecPresetSummary {
    pub id: String,
    pub name: String,
    pub index: usize,
    pub model: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExecWithPresetResponse {
    pub ok: bool,
    pub preset: ExecPresetSummary,
    pub cwd: String,
    pub sandbox: &'static str,
    pub ephemeral: bool,
    pub elapsed_ms: u64,
    pub exit_code: i32,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub structured_output: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub async fn exec_with_preset(
    State(state): State<AppState>,
    Json(payload): Json<ExecWithPresetRequest>,
) -> ApiResult<Json<ExecWithPresetResponse>> {
    validate_exec_request(&payload)?;
    if payload
        .reasoning_effort
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return Err(AppError::bad_request(
            "单次 reasoning_effort 覆盖已禁用；请在用户级 ~/.codex/config.toml 或 API 预设中配置。",
        ));
    }

    let user = runtime_paths::resolve_user_profile(&state.workspace_settings.terminal_user())
        .map_err(|error| AppError::bad_request(format!("终端用户无效: {error}")))?;
    let presets = state.auth_manager.api_presets_snapshot();
    let current_id = if payload.preset.current {
        Some(resolve_current_preset_id(&user, &presets).await?)
    } else {
        None
    };
    let (preset_index, preset) = resolve_preset(&presets, &payload.preset, current_id.as_deref())?;
    let request = CreateCodexTaskRequest::new(
        CodexTaskMode::Exec,
        CodexTaskPresetSelector::by_id(&preset.id),
        payload.cwd,
        payload.task,
        payload.timeout_secs,
        payload.output_schema,
    );
    let record = crate::codex_task::submit_task_and_wait(&state, request).await?;

    Ok(Json(legacy_exec_response(record, preset_index)))
}

fn legacy_exec_response(record: CodexTaskRecord, preset_index: usize) -> ExecWithPresetResponse {
    ExecWithPresetResponse {
        ok: record.status == CodexTaskStatus::Succeeded,
        preset: ExecPresetSummary {
            id: record.preset.id,
            name: record.preset.name,
            index: preset_index,
            model: record.preset.model,
        },
        cwd: record.cwd,
        sandbox: "config.toml",
        ephemeral: true,
        elapsed_ms: record
            .finished_at
            .unwrap_or(record.updated_at)
            .saturating_sub(record.started_at.unwrap_or(record.created_at)),
        exit_code: record.exit_code.unwrap_or(-1),
        output: record.result,
        structured_output: record.structured_output,
        error: record.error,
    }
}

fn validate_exec_request(payload: &ExecWithPresetRequest) -> ApiResult<()> {
    if payload.task.trim().is_empty() {
        return Err(AppError::bad_request("task 不能为空。"));
    }
    if payload.task.len() > MAX_TASK_BYTES {
        return Err(AppError::bad_request(format!("task 过大，最多允许 {MAX_TASK_BYTES} 字节。")));
    }
    let timeout_secs = payload.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    if timeout_secs == 0 || timeout_secs > MAX_TIMEOUT_SECS {
        return Err(AppError::bad_request(format!(
            "timeout_secs 必须在 1..={MAX_TIMEOUT_SECS} 之间。"
        )));
    }
    if let Some(schema) = payload.output_schema.as_ref() {
        if !schema.is_object() {
            return Err(AppError::bad_request("output_schema 必须是 JSON 对象。"));
        }
        let encoded = serde_json::to_vec(schema)
            .map_err(|error| AppError::bad_request(format!("output_schema 无效: {error}")))?;
        if encoded.len() > MAX_SCHEMA_BYTES {
            return Err(AppError::bad_request(format!(
                "output_schema 过大，最多允许 {MAX_SCHEMA_BYTES} 字节。"
            )));
        }
    }
    Ok(())
}

pub(super) async fn resolve_current_preset_id(
    user: &runtime_paths::UserProfile,
    presets: &[StoredApiPreset],
) -> ApiResult<String> {
    let auth_file = user.home.join(auth_core::AUTH_FILE_RELATIVE_PATH);
    let config_file = user.home.join(auth_core::CONFIG_FILE_RELATIVE_PATH);
    let current_auth = read_current_auth_state(&auth_file)
        .await
        .map_err(|error| AppError::internal(format!("读取当前 Codex auth 失败: {error}")))?;
    let current_config = read_current_config_provider(&config_file)
        .await
        .map_err(|error| AppError::internal(format!("读取当前 Codex config 失败: {error}")))?;
    derive_current_api_state(current_config.as_ref(), current_auth.as_ref(), presets)
        .and_then(|api| api.preset_id)
        .ok_or_else(|| {
            AppError::bad_request("当前 Codex 配置无法匹配已保存的 API 预设，请显式指定 preset。")
        })
}

pub(super) fn resolve_preset(
    presets: &[StoredApiPreset],
    selector: &ApiPresetSelector,
    current_id: Option<&str>,
) -> ApiResult<(usize, StoredApiPreset)> {
    let supplied = usize::from(selector.id.as_deref().is_some_and(nonempty))
        + usize::from(selector.name.as_deref().is_some_and(nonempty))
        + usize::from(selector.model.as_deref().is_some_and(nonempty))
        + usize::from(selector.index.is_some())
        + usize::from(selector.current);
    if supplied != 1 {
        return Err(AppError::bad_request(
            "preset 必须且只能提供 id、name、model、index、current 其中一个选择器。",
        ));
    }

    if selector.current {
        let id = current_id.ok_or_else(|| AppError::bad_request("无法解析当前 API 预设。"))?;
        let index = select_api_preset_index(presets, ApiPresetLookup::Id(id))
            .map_err(|error| AppError::bad_request(error.to_string()))?;
        return Ok((index, presets[index].clone()));
    }
    if let Some(index) = selector.index {
        let len = presets.len() as isize;
        let resolved = if index < 0 { len + index } else { index };
        if resolved < 0 || resolved >= len {
            return Err(AppError::bad_request(format!(
                "API 预设索引越界: index={index}，当前共 {len} 个预设。"
            )));
        }
        return Ok((resolved as usize, presets[resolved as usize].clone()));
    }
    let lookup = if let Some(id) = selector.id.as_deref() {
        ApiPresetLookup::Id(id)
    } else if let Some(name) = selector.name.as_deref() {
        ApiPresetLookup::Name(name)
    } else {
        ApiPresetLookup::Model(selector.model.as_deref().unwrap_or_default())
    };
    let index = select_api_preset_index(presets, lookup)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok((index, presets[index].clone()))
}

fn nonempty(value: &str) -> bool {
    !value.trim().is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(id: &str, name: &str) -> StoredApiPreset {
        StoredApiPreset {
            id: id.to_string(),
            name: name.to_string(),
            saved_at: 0,
            provider_name: name.to_string(),
            base_url: "https://example.test/v1".to_string(),
            management_url: None,
            wire_api: None,
            responses_proxy: None,
            apply_upstream_proxy_on_switch: false,
            config_overrides: Vec::new(),
            legacy_config_key: None,
            legacy_config_value: None,
            legacy_secondary_config_key: None,
            legacy_secondary_config_value: None,
            terminal_env: Vec::new(),
            terminal_startup_script: None,
            api_key: "test-key".to_string(),
            access_token: String::new(),
            account_id: String::new(),
            access_mode: None,
            switch_count: 0,
        }
    }

    #[test]
    fn selector_requires_exactly_one_kind() {
        let presets = vec![preset("api-1", "one")];
        let empty = ApiPresetSelector {
            id: None,
            name: None,
            model: None,
            index: None,
            current: false,
        };
        assert!(resolve_preset(&presets, &empty, None).is_err());
    }

    #[test]
    fn selector_supports_name_and_negative_index() {
        let presets = vec![preset("api-1", "one"), preset("api-2", "two")];
        let by_name = ApiPresetSelector {
            id: None,
            name: Some("TWO".to_string()),
            model: None,
            index: None,
            current: false,
        };
        assert_eq!(resolve_preset(&presets, &by_name, None).unwrap().0, 1);
        let by_index = ApiPresetSelector {
            id: None,
            name: None,
            model: None,
            index: Some(-1),
            current: false,
        };
        assert_eq!(resolve_preset(&presets, &by_index, None).unwrap().0, 1);
    }
}
