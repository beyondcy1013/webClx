use std::time::Duration;

use axum::{
    Json,
    extract::{Path as AxumPath, State},
};
use tokio::{net::TcpStream, time::timeout};

use super::*;
use crate::{ApiResult, AppError, AppState};

pub async fn get_frpc_status(State(state): State<AppState>) -> ApiResult<Json<FrpcStatusResponse>> {
    Ok(Json(state.frpc_manager.status()))
}

pub async fn save_frpc_config(
    State(state): State<AppState>,
    Json(payload): Json<SaveFrpcConfigRequest>,
) -> ApiResult<Json<FrpcStatusResponse>> {
    let config = normalize_frpc_config(payload.config);
    validate_frpc_config(&config).map_err(|error| AppError::bad_request(error.to_string()))?;
    state
        .frpc_manager
        .persist_config(&config)
        .map_err(|error| AppError::internal(format!("保存 frpc 配置失败: {error}")))?;
    Ok(Json(state.frpc_manager.status()))
}

pub async fn start_frpc(State(state): State<AppState>) -> ApiResult<Json<FrpcStatusResponse>> {
    state
        .frpc_manager
        .start()
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(state.frpc_manager.status()))
}

pub async fn stop_frpc(State(state): State<AppState>) -> ApiResult<Json<FrpcStatusResponse>> {
    state
        .frpc_manager
        .stop()
        .await
        .map_err(|error| AppError::internal(format!("停止 frpc 失败: {error}")))?;
    Ok(Json(state.frpc_manager.status()))
}

pub async fn restart_frpc(State(state): State<AppState>) -> ApiResult<Json<FrpcStatusResponse>> {
    state
        .frpc_manager
        .restart()
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(state.frpc_manager.status()))
}

pub async fn download_frpc_binary(
    State(state): State<AppState>,
) -> ApiResult<Json<FrpDownloadResponse>> {
    state
        .frpc_manager
        .download_binary()
        .await
        .map(Json)
        .map_err(|error| AppError::bad_request(format!("下载 frpc 失败: {error}")))
}

pub async fn get_frps_status(State(state): State<AppState>) -> ApiResult<Json<FrpsStatusResponse>> {
    Ok(Json(state.frps_manager.status()))
}

pub async fn save_frps_config(
    State(state): State<AppState>,
    Json(payload): Json<SaveFrpsConfigRequest>,
) -> ApiResult<Json<FrpsStatusResponse>> {
    let config = normalize_frps_config(payload.config);
    validate_frps_config(&config).map_err(|error| AppError::bad_request(error.to_string()))?;
    state
        .frps_manager
        .persist_config(&config)
        .map_err(|error| AppError::internal(format!("保存 frps 配置失败: {error}")))?;
    Ok(Json(state.frps_manager.status()))
}

pub async fn start_frps(State(state): State<AppState>) -> ApiResult<Json<FrpsStatusResponse>> {
    state
        .frps_manager
        .start()
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(state.frps_manager.status()))
}

pub async fn stop_frps(State(state): State<AppState>) -> ApiResult<Json<FrpsStatusResponse>> {
    state
        .frps_manager
        .stop()
        .await
        .map_err(|error| AppError::internal(format!("停止 frps 失败: {error}")))?;
    Ok(Json(state.frps_manager.status()))
}

pub async fn restart_frps(State(state): State<AppState>) -> ApiResult<Json<FrpsStatusResponse>> {
    state
        .frps_manager
        .restart()
        .await
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(Json(state.frps_manager.status()))
}

pub async fn download_frps_binary(
    State(state): State<AppState>,
) -> ApiResult<Json<FrpDownloadResponse>> {
    state
        .frps_manager
        .download_binary()
        .await
        .map(Json)
        .map_err(|error| AppError::bad_request(format!("下载 frps 失败: {error}")))
}

pub async fn list_frp_roles(State(state): State<AppState>) -> ApiResult<Json<FrpRolesResponse>> {
    Ok(Json(state.frp_role_manager.status()))
}

pub async fn save_frp_role(
    State(state): State<AppState>,
    Json(payload): Json<SaveFrpRoleRequest>,
) -> ApiResult<Json<FrpRolesResponse>> {
    state
        .frp_role_manager
        .save_role(payload.role)
        .map(Json)
        .map_err(|error| AppError::bad_request(error.to_string()))
}

pub async fn delete_frp_role(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<FrpRolesResponse>> {
    state
        .frp_role_manager
        .delete_role(&id)
        .await
        .map(Json)
        .map_err(|error| AppError::bad_request(error.to_string()))
}

pub async fn unmanage_frp_role(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<FrpRolesResponse>> {
    state
        .frp_role_manager
        .unmanage_role(&id)
        .map(Json)
        .map_err(|error| AppError::bad_request(error.to_string()))
}

pub async fn start_frp_role(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<FrpRolesResponse>> {
    state
        .frp_role_manager
        .start_role(&id)
        .await
        .map(Json)
        .map_err(|error| AppError::bad_request(error.to_string()))
}

pub async fn stop_frp_role(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<FrpRolesResponse>> {
    state
        .frp_role_manager
        .stop_role(&id)
        .await
        .map(Json)
        .map_err(|error| AppError::internal(format!("停止 FRP 角色失败: {error}")))
}

pub async fn restart_frp_role(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<FrpRolesResponse>> {
    state
        .frp_role_manager
        .restart_role(&id)
        .await
        .map(Json)
        .map_err(|error| AppError::bad_request(error.to_string()))
}

pub async fn download_frp_role_binary(
    State(state): State<AppState>,
    AxumPath(id): AxumPath<String>,
) -> ApiResult<Json<FrpDownloadResponse>> {
    state
        .frp_role_manager
        .download_role_binary(&id)
        .await
        .map(Json)
        .map_err(|error| AppError::bad_request(format!("下载 FRP 角色二进制失败: {error}")))
}

pub async fn discover_system_frp(
    State(state): State<AppState>,
) -> ApiResult<Json<FrpSystemDiscoveryResponse>> {
    Ok(Json(state.frp_role_manager.system_discovery()))
}

pub async fn adopt_system_frp(
    State(state): State<AppState>,
    Json(payload): Json<AdoptFrpSystemRequest>,
) -> ApiResult<Json<FrpRolesResponse>> {
    state
        .frp_role_manager
        .adopt_system_entry(payload)
        .map(Json)
        .map_err(|error| AppError::bad_request(format!("接管系统 FRP 失败: {error}")))
}

pub async fn test_frp_port(
    Json(payload): Json<FrpPortTestRequest>,
) -> ApiResult<Json<FrpPortTestResponse>> {
    let host = payload.host.trim();
    if host.is_empty() {
        return Err(AppError::bad_request("请填写要测试的公网地址"));
    }
    if payload.port == 0 {
        return Err(AppError::bad_request("请填写有效端口"));
    }
    let timeout_secs = payload.timeout_secs.unwrap_or(5).clamp(1, 30);
    let target = format!("{host}:{}", payload.port);
    let started = std::time::Instant::now();
    let result =
        timeout(Duration::from_secs(timeout_secs), TcpStream::connect((host, payload.port))).await;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    match result {
        Ok(Ok(_stream)) => Ok(Json(FrpPortTestResponse {
            ok: true,
            target,
            elapsed_ms,
            error: None,
        })),
        Ok(Err(error)) => Ok(Json(FrpPortTestResponse {
            ok: false,
            target,
            elapsed_ms,
            error: Some(error.to_string()),
        })),
        Err(_) => Ok(Json(FrpPortTestResponse {
            ok: false,
            target,
            elapsed_ms,
            error: Some(format!("{timeout_secs}s 超时")),
        })),
    }
}
