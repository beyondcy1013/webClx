use axum::{Json, extract::State};

use crate::{ApiResult, AppError, AppState, host};

pub use settings_core::{
    MergeAllRequest, MergeAllResponse, MergeFieldRequest, MergeFieldResponse, MergeTabRequest,
    MergeTabResponse, SaveSettingsRequest, SaveSettingsResponse, SettingsError, SettingsErrorKind,
    SettingsManager, SettingsResponse,
};

pub async fn get_settings(State(state): State<AppState>) -> ApiResult<Json<SettingsResponse>> {
    settings_core::build_settings_response(
        &state.workspace_settings,
        host::current_host_name(),
        state.listen_addr.to_string(),
        state.version,
    )
    .map(Json)
    .map_err(settings_error_to_app_error)
}

pub async fn save_settings(
    State(state): State<AppState>,
    Json(payload): Json<SaveSettingsRequest>,
) -> ApiResult<Json<SaveSettingsResponse>> {
    settings_core::save_settings(&state.workspace_settings, payload)
        .await
        .map(Json)
        .map_err(settings_error_to_app_error)
}

fn settings_error_to_app_error(error: SettingsError) -> AppError {
    match error.kind() {
        SettingsErrorKind::BadRequest => AppError::bad_request(error.to_string()),
        SettingsErrorKind::Internal => AppError::internal(error.to_string()),
    }
}

pub async fn merge_field(
    State(state): State<AppState>,
    Json(payload): Json<MergeFieldRequest>,
) -> ApiResult<Json<MergeFieldResponse>> {
    let remote = fetch_remote_settings(&payload.remote_url)
        .await
        .map_err(|e| AppError::bad_request(format!("获取远程配置失败: {e}")))?;

    settings_core::merge_field(
        &state.workspace_settings,
        &payload.remote_url,
        &payload.field,
        remote,
    )
    .await
    .map(Json)
    .map_err(settings_error_to_app_error)
}

pub async fn merge_tab(
    State(state): State<AppState>,
    Json(payload): Json<MergeTabRequest>,
) -> ApiResult<Json<MergeTabResponse>> {
    let remote = fetch_remote_settings(&payload.remote_url)
        .await
        .map_err(|e| AppError::bad_request(format!("获取远程配置失败: {e}")))?;

    settings_core::merge_tab(&state.workspace_settings, &payload.remote_url, &payload.tab, remote)
        .await
        .map(Json)
        .map_err(settings_error_to_app_error)
}

pub async fn merge_all(
    State(state): State<AppState>,
    Json(payload): Json<MergeAllRequest>,
) -> ApiResult<Json<MergeAllResponse>> {
    let remote = fetch_remote_settings(&payload.remote_url)
        .await
        .map_err(|e| AppError::bad_request(format!("获取远程配置失败: {e}")))?;

    settings_core::merge_all(&state.workspace_settings, &payload.remote_url, remote)
        .await
        .map(Json)
        .map_err(settings_error_to_app_error)
}

async fn fetch_remote_settings(remote_url: &str) -> Result<SettingsResponse, String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| format!("创建HTTP客户端失败: {e}"))?;

    let response = client
        .get(remote_url)
        .send()
        .await
        .map_err(|e| format!("连接远程服务器失败: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("远程服务器返回错误状态: {}", response.status()));
    }

    response
        .json::<SettingsResponse>()
        .await
        .map_err(|e| format!("解析远程配置失败: {e}"))
}
