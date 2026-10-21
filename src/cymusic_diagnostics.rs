use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::{Query, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::{ApiResult, AppError, AppState};

const MAX_LOG_BYTES: usize = 256 * 1024;
const MAX_FILES_PER_DEVICE: usize = 20;

#[derive(Deserialize, Serialize)]
pub(crate) struct DiagnosticUpload {
    device_id: String,
    session_id: String,
    app_version: String,
    log: String,
}

#[derive(Deserialize)]
pub(crate) struct DiagnosticQuery {
    device_id: Option<String>,
}

#[derive(Serialize)]
pub(crate) struct DiagnosticReceipt {
    ok: bool,
    device_id: String,
    session_id: String,
    stored_at: u128,
}

#[derive(Serialize)]
pub(crate) struct DiagnosticFile {
    name: String,
    content: String,
}

pub(crate) async fn upload(
    State(state): State<AppState>,
    Json(payload): Json<DiagnosticUpload>,
) -> ApiResult<(StatusCode, Json<DiagnosticReceipt>)> {
    validate_id(&payload.device_id, "device_id")?;
    validate_id(&payload.session_id, "session_id")?;
    if payload.app_version.len() > 32 || payload.log.is_empty() || payload.log.len() > MAX_LOG_BYTES
    {
        return Err(AppError::bad_request("invalid diagnostic payload"));
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| AppError::internal("system clock is invalid"))?
        .as_millis();
    let dir = diagnostic_root(&state).join(&payload.device_id);
    fs::create_dir_all(&dir).await.map_err(|error| {
        AppError::internal(format!("cannot create diagnostic directory: {error}"))
    })?;
    let name = format!("{}-{}.json", now, payload.session_id);
    let bytes = serde_json::to_vec_pretty(&payload)
        .map_err(|error| AppError::internal(format!("cannot encode diagnostic: {error}")))?;
    fs::write(dir.join(&name), bytes)
        .await
        .map_err(|error| AppError::internal(format!("cannot store diagnostic: {error}")))?;
    enforce_retention(&dir).await?;
    Ok((
        StatusCode::CREATED,
        Json(DiagnosticReceipt {
            ok: true,
            device_id: payload.device_id,
            session_id: payload.session_id,
            stored_at: now,
        }),
    ))
}

pub(crate) async fn list(
    State(state): State<AppState>,
    Query(query): Query<DiagnosticQuery>,
) -> ApiResult<Json<Vec<DiagnosticFile>>> {
    let root = diagnostic_root(&state);
    let mut files = Vec::new();
    if let Some(device) = query.device_id {
        validate_id(&device, "device_id")?;
        collect_files(root.join(device), &mut files).await?;
    } else if let Ok(mut devices) = fs::read_dir(&root).await {
        while let Some(entry) = devices
            .next_entry()
            .await
            .map_err(|error| AppError::internal(format!("cannot read diagnostics: {error}")))?
        {
            if entry
                .file_type()
                .await
                .map(|kind| kind.is_dir())
                .unwrap_or(false)
            {
                collect_files(entry.path(), &mut files).await?;
            }
        }
    }
    files.sort_by(|left, right| right.name.cmp(&left.name));
    files.truncate(50);
    Ok(Json(files))
}

fn diagnostic_root(state: &AppState) -> PathBuf {
    state.app_dir.join("cymusic-diagnostics")
}

fn validate_id(value: &str, field: &str) -> ApiResult<()> {
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(AppError::bad_request(format!("invalid {field}")));
    }
    Ok(())
}

async fn collect_files(dir: PathBuf, output: &mut Vec<DiagnosticFile>) -> ApiResult<()> {
    let Ok(mut entries) = fs::read_dir(dir).await else {
        return Ok(());
    };
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| AppError::internal(format!("cannot read diagnostics: {error}")))?
    {
        if !entry
            .file_type()
            .await
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            continue;
        }
        let content = fs::read_to_string(entry.path())
            .await
            .map_err(|error| AppError::internal(format!("cannot read diagnostic: {error}")))?;
        output.push(DiagnosticFile {
            name: entry.path().display().to_string(),
            content,
        });
    }
    Ok(())
}

async fn enforce_retention(dir: &PathBuf) -> ApiResult<()> {
    let mut paths = Vec::new();
    let mut entries = fs::read_dir(dir).await.map_err(|error| {
        AppError::internal(format!("cannot read diagnostic directory: {error}"))
    })?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| AppError::internal(format!("cannot read diagnostic directory: {error}")))?
    {
        if entry
            .file_type()
            .await
            .map(|kind| kind.is_file())
            .unwrap_or(false)
        {
            paths.push(entry.path());
        }
    }
    paths.sort();
    let remove_count = paths.len().saturating_sub(MAX_FILES_PER_DEVICE);
    for path in paths.into_iter().take(remove_count) {
        let _ = fs::remove_file(path).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_ids_are_path_safe_and_bounded() {
        assert!(validate_id("device_01-A", "device_id").is_ok());
        assert!(validate_id("", "device_id").is_err());
        assert!(validate_id("../device", "device_id").is_err());
        assert!(validate_id(&"a".repeat(65), "device_id").is_err());
    }

    #[test]
    fn upload_payload_serializes_for_persistence() {
        let payload = DiagnosticUpload {
            device_id: "device_01".to_owned(),
            session_id: "session_01".to_owned(),
            app_version: "0.1.25".to_owned(),
            log: "stage=probe detail=ok".to_owned(),
        };

        let encoded = serde_json::to_string(&payload).expect("diagnostic payload should serialize");
        assert!(encoded.contains("\"device_id\":\"device_01\""));
    }
}
