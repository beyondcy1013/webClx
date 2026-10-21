use axum::{Json, extract::State, routing::get};
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::fs;

use crate::{ApiResult, AppError, AppState};

#[derive(Debug, Deserialize)]
struct UpdateOpenCodeOptionsRequest {
    allow_all_permissions: bool,
}

pub(super) fn routes() -> axum::Router<AppState> {
    axum::Router::new().route("/api/auth/opencode-options", get(get_options).put(update_options))
}

async fn get_options(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let path = config_path(&state)?;
    let root = read_config(&path).await?;
    Ok(Json(serde_json::json!({
        "allowAllPermissions": root.get("permission") == Some(&Value::String("allow".to_string())),
    })))
}

async fn update_options(
    State(state): State<AppState>,
    Json(payload): Json<UpdateOpenCodeOptionsRequest>,
) -> ApiResult<Json<Value>> {
    let path = config_path(&state)?;
    write_options(&path, payload.allow_all_permissions).await?;
    Ok(Json(serde_json::json!({
        "ok": true,
        "allowAllPermissions": payload.allow_all_permissions,
    })))
}

async fn write_options(path: &std::path::Path, allow_all_permissions: bool) -> ApiResult<()> {
    let mut root = read_config(&path).await?;
    if allow_all_permissions {
        root.insert("permission".to_string(), Value::String("allow".to_string()));
    } else {
        root.insert("permission".to_string(), Value::String("ask".to_string()));
    }
    root.entry("$schema")
        .or_insert_with(|| Value::String("https://opencode.ai/config.json".to_string()));
    let content = serde_json::to_vec_pretty(&Value::Object(root))
        .map_err(|error| AppError::internal(format!("序列化 opencode.json 失败: {error}")))?;
    fs::create_dir_all(path.parent().expect("config path has parent"))
        .await
        .map_err(|error| AppError::internal(format!("创建 OpenCode 配置目录失败: {error}")))?;
    crate::atomic_file::write_atomic(&path, &content)
        .map_err(|error| AppError::internal(format!("写入 opencode.json 失败: {error}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|error| AppError::internal(format!("更新 opencode.json 权限失败: {error}")))?;
    }
    Ok(())
}

fn config_path(state: &AppState) -> ApiResult<std::path::PathBuf> {
    Ok(state.workspace_root().join("opencode.json"))
}

async fn read_config(path: &std::path::Path) -> ApiResult<Map<String, Value>> {
    let content = match fs::read_to_string(path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "{}".to_string(),
        Err(error) => return Err(AppError::internal(format!("读取 opencode.json 失败: {error}"))),
    };
    let value: Value = serde_json::from_str(&content)
        .map_err(|error| AppError::bad_request(format!("opencode.json 格式无效: {error}")))?;
    value
        .as_object()
        .cloned()
        .ok_or_else(|| AppError::bad_request("opencode.json 顶层必须是对象。"))
}
