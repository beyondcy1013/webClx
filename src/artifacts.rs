use std::{
    collections::{BTreeMap, HashSet},
    ffi::OsStr,
    path::{Component, Path, PathBuf},
};

use axum::{
    Json,
    body::Body,
    extract::{Path as AxumPath, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, Response},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tokio::{fs, io::AsyncWriteExt, sync::Mutex};

use crate::{ApiResult, AppError, AppState};

const ARTIFACTS_DIR: &str = ".webclx-artifacts";
const INDEX_FILE: &str = "index.json";
const MAX_ARTIFACTS_PER_PROJECT: usize = 3;
const ANDROID_MINIMUM_SUPPORTED_VERSION: &str = "1.7.16";
static ARTIFACT_INDEX_LOCK: Mutex<()> = Mutex::const_new(());

#[derive(Debug, Deserialize)]
pub struct PublishArtifactRequest {
    project: String,
    path: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    note: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ArtifactRecord {
    id: String,
    project: String,
    name: String,
    label: String,
    note: String,
    size: u64,
    #[serde(default)]
    sha256: String,
    source_path: String,
    stored_path: String,
    download_url: String,
    published_at: String,
}

#[derive(Debug, Serialize)]
pub struct AndroidUpdateManifest {
    version: String,
    version_code: u64,
    minimum_supported_version: String,
    mandatory: bool,
    release_notes: String,
    published_at: String,
    platform: &'static str,
    arch: &'static str,
    file: String,
    sha256: String,
    size: u64,
    download_url: String,
}

#[derive(Debug, Serialize, Deserialize, Default)]
struct ArtifactIndex {
    artifacts: Vec<ArtifactRecord>,
}

#[derive(Debug, Serialize)]
pub struct ArtifactListResponse {
    projects: Vec<ProjectArtifacts>,
}

#[derive(Debug, Serialize)]
pub struct ProjectArtifacts {
    project: String,
    artifacts: Vec<ArtifactRecord>,
}

pub async fn downloads_page() -> Html<String> {
    Html(DOWNLOADS_HTML.to_string())
}

pub async fn enforce_artifact_retention(app_dir: &Path) -> ApiResult<()> {
    let _index_guard = ARTIFACT_INDEX_LOCK.lock().await;
    let mut index = load_index(app_dir).await?;
    let retired = retire_artifacts_over_limit(&mut index, None);
    if retired.is_empty() {
        return Ok(());
    }
    save_index(app_dir, &index).await?;
    remove_retired_artifacts_from_store(app_dir, &retired).await;
    Ok(())
}

pub async fn list_artifacts(
    State(state): State<AppState>,
) -> ApiResult<Json<ArtifactListResponse>> {
    refresh_artifact_index(&state.app_dir).await?;
    let index = load_index(&state.app_dir).await?;
    let projects = group_artifacts_by_project(index.artifacts);
    Ok(Json(ArtifactListResponse { projects }))
}

pub async fn android_update_manifest(
    State(state): State<AppState>,
    AxumPath(project): AxumPath<String>,
) -> ApiResult<Json<AndroidUpdateManifest>> {
    let project = sanitize_segment(&project, "project")?;
    refresh_artifact_index(&state.app_dir).await?;
    let index = load_index(&state.app_dir).await?;
    let mut candidates = index
        .artifacts
        .into_iter()
        .filter(|record| record.project == project && !record.sha256.is_empty())
        .filter_map(|record| {
            let (version, version_code) = android_version_from_name(&project, &record.name)?;
            Some((record, version, version_code))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|left, right| {
        right
            .2
            .cmp(&left.2)
            .then_with(|| right.0.published_at.cmp(&left.0.published_at))
    });
    let Some((record, version, version_code)) = candidates.into_iter().next() else {
        return Err(AppError::not_found("没有可用的 Android 更新。"));
    };
    Ok(Json(AndroidUpdateManifest {
        minimum_supported_version: ANDROID_MINIMUM_SUPPORTED_VERSION.to_string(),
        version,
        version_code,
        mandatory: false,
        release_notes: record.note,
        published_at: record.published_at,
        platform: "android",
        arch: "universal",
        file: record.name,
        sha256: record.sha256,
        size: record.size,
        download_url: record.download_url,
    }))
}

fn group_artifacts_by_project(artifacts: Vec<ArtifactRecord>) -> Vec<ProjectArtifacts> {
    let mut groups: BTreeMap<String, Vec<ArtifactRecord>> = BTreeMap::new();
    for record in artifacts {
        groups
            .entry(record.project.clone())
            .or_default()
            .push(record);
    }
    let mut projects = groups
        .into_iter()
        .map(|(project, mut artifacts)| {
            sort_artifacts_by_recency(&mut artifacts);
            ProjectArtifacts { project, artifacts }
        })
        .collect::<Vec<_>>();
    projects.sort_by(|left, right| {
        let left_latest = left
            .artifacts
            .first()
            .map(|artifact| artifact.published_at.as_str())
            .unwrap_or_default();
        let right_latest = right
            .artifacts
            .first()
            .map(|artifact| artifact.published_at.as_str())
            .unwrap_or_default();
        right_latest
            .cmp(left_latest)
            .then_with(|| left.project.cmp(&right.project))
    });
    projects
}

pub async fn publish_artifact(
    State(state): State<AppState>,
    Json(payload): Json<PublishArtifactRequest>,
) -> ApiResult<Json<ArtifactRecord>> {
    let source = validate_source_file(&payload.path).await?;
    let metadata = fs::metadata(&source)
        .await
        .map_err(|error| AppError::internal(format!("读取产物信息失败: {error}")))?;
    let project = sanitize_segment(&payload.project, "project")?;
    let name = artifact_name(&payload.name, &source)?;
    let label = non_empty(&payload.label).unwrap_or_else(|| name.clone());
    let note = non_empty(&payload.note).unwrap_or_default();
    let _index_guard = ARTIFACT_INDEX_LOCK.lock().await;
    let id = artifact_id();
    let published_at = now_rfc3339();
    let project_dir = artifacts_root(&state.app_dir).join(&project);
    fs::create_dir_all(&project_dir)
        .await
        .map_err(|error| AppError::internal(format!("创建产物目录失败: {error}")))?;
    let stored_file_name = format!("{id}__{name}");
    let stored = project_dir.join(&stored_file_name);
    fs::copy(&source, &stored)
        .await
        .map_err(|error| AppError::internal(format!("复制产物失败: {error}")))?;
    let sha256 = sha256_file(&stored).await?;

    let download_url =
        format!("/api/artifacts/download/{}/{}", url_component(&id), url_component(&name));
    let record = ArtifactRecord {
        id,
        project,
        name,
        label,
        note,
        size: metadata.len(),
        sha256,
        source_path: source.display().to_string(),
        stored_path: stored.display().to_string(),
        download_url,
        published_at,
    };
    let mut index = load_index(&state.app_dir).await?;
    let mut retired = Vec::new();
    index.artifacts.retain(|item| {
        let replaced = item.project == record.project
            && item.name == record.name
            && item.label == record.label;
        if replaced {
            retired.push(item.clone());
        }
        !replaced
    });
    index.artifacts.push(record.clone());
    retired.extend(retire_artifacts_over_limit(&mut index, Some(&record.project)));
    save_index(&state.app_dir, &index).await?;
    remove_retired_artifact_files(&project_dir, &retired).await;
    Ok(Json(record))
}

fn retire_artifacts_over_limit(
    index: &mut ArtifactIndex,
    project_filter: Option<&str>,
) -> Vec<ArtifactRecord> {
    sort_artifacts_by_recency(&mut index.artifacts);
    let mut project_counts = BTreeMap::<String, usize>::new();
    let mut retired = Vec::new();
    index.artifacts.retain(|item| {
        if project_filter.is_some_and(|project| item.project != project) {
            return true;
        }
        let count = project_counts.entry(item.project.clone()).or_default();
        *count += 1;
        if *count <= MAX_ARTIFACTS_PER_PROJECT {
            true
        } else {
            retired.push(item.clone());
            false
        }
    });
    retired
}

fn sort_artifacts_by_recency(artifacts: &mut [ArtifactRecord]) {
    artifacts.sort_by(|left, right| {
        right
            .published_at
            .cmp(&left.published_at)
            .then_with(|| right.id.cmp(&left.id))
    });
}

async fn remove_retired_artifacts_from_store(app_dir: &Path, records: &[ArtifactRecord]) {
    for record in records {
        let Ok(project) = sanitize_segment(&record.project, "project") else {
            tracing::warn!(
                artifact_id = %record.id,
                project = %record.project,
                "skip deleting retired artifact with an invalid project"
            );
            continue;
        };
        if project != record.project {
            tracing::warn!(
                artifact_id = %record.id,
                project = %record.project,
                "skip deleting retired artifact with a non-canonical project"
            );
            continue;
        }
        let project_dir = artifacts_root(app_dir).join(project);
        remove_retired_artifact_files(&project_dir, std::slice::from_ref(record)).await;
    }
}

async fn refresh_artifact_index(app_dir: &Path) -> ApiResult<()> {
    let _index_guard = ARTIFACT_INDEX_LOCK.lock().await;
    let root = artifacts_root(app_dir);
    fs::create_dir_all(&root)
        .await
        .map_err(|error| AppError::internal(format!("创建产物目录失败: {error}")))?;
    let mut index = load_index(app_dir).await?;
    let mut known_paths = index
        .artifacts
        .iter()
        .map(|record| PathBuf::from(&record.stored_path))
        .collect::<HashSet<_>>();
    let mut projects = fs::read_dir(&root)
        .await
        .map_err(|error| AppError::internal(format!("扫描产物目录失败: {error}")))?;
    let mut changed = false;

    while let Some(project_entry) = projects
        .next_entry()
        .await
        .map_err(|error| AppError::internal(format!("扫描项目目录失败: {error}")))?
    {
        let file_type = project_entry
            .file_type()
            .await
            .map_err(|error| AppError::internal(format!("读取项目目录类型失败: {error}")))?;
        if !file_type.is_dir() {
            continue;
        }
        let project = project_entry.file_name().to_string_lossy().into_owned();
        if sanitize_segment(&project, "project").ok().as_deref() != Some(project.as_str()) {
            continue;
        }
        let project_dir = project_entry.path();
        let mut files = fs::read_dir(&project_dir)
            .await
            .map_err(|error| AppError::internal(format!("扫描项目产物失败: {error}")))?;
        while let Some(file_entry) = files
            .next_entry()
            .await
            .map_err(|error| AppError::internal(format!("扫描产物文件失败: {error}")))?
        {
            let file_type = file_entry
                .file_type()
                .await
                .map_err(|error| AppError::internal(format!("读取产物类型失败: {error}")))?;
            if !file_type.is_file() || known_paths.contains(&file_entry.path()) {
                continue;
            }
            let dropped_name = file_entry.file_name().to_string_lossy().into_owned();
            if dropped_name.starts_with('.')
                || dropped_name.ends_with(".tmp")
                || dropped_name.ends_with(".part")
            {
                continue;
            }
            let (id, name, stored) = if let Some((stored_id, stored_name)) = dropped_name
                .split_once("__")
                .filter(|(stored_id, stored_name)| {
                    !stored_id.is_empty()
                        && sanitize_file_name(stored_name).ok().as_deref() == Some(*stored_name)
                }) {
                (stored_id.to_string(), stored_name.to_string(), file_entry.path())
            } else {
                let name = sanitize_file_name(&dropped_name)?;
                let id = artifact_id();
                let stored = project_dir.join(format!("{id}__{name}"));
                fs::rename(file_entry.path(), &stored)
                    .await
                    .map_err(|error| AppError::internal(format!("接管投放产物失败: {error}")))?;
                (id, name, stored)
            };
            if index.artifacts.iter().any(|record| record.id == id) {
                continue;
            }
            let metadata = fs::metadata(&stored)
                .await
                .map_err(|error| AppError::internal(format!("读取投放产物信息失败: {error}")))?;
            let published_at = metadata
                .modified()
                .ok()
                .and_then(|modified| OffsetDateTime::from(modified).format(&Rfc3339).ok())
                .unwrap_or_else(now_rfc3339);
            let sha256 = sha256_file(&stored).await?;
            let download_url =
                format!("/api/artifacts/download/{}/{}", url_component(&id), url_component(&name));
            index.artifacts.push(ArtifactRecord {
                id,
                project: project.clone(),
                name: name.clone(),
                label: name,
                note: String::new(),
                size: metadata.len(),
                sha256,
                source_path: file_entry.path().display().to_string(),
                stored_path: stored.display().to_string(),
                download_url,
                published_at,
            });
            known_paths.insert(stored);
            changed = true;
        }
    }

    if changed {
        let retired = retire_artifacts_over_limit(&mut index, None);
        save_index(app_dir, &index).await?;
        remove_retired_artifacts_from_store(app_dir, &retired).await;
    }
    Ok(())
}

async fn remove_retired_artifact_files(project_dir: &Path, records: &[ArtifactRecord]) {
    for record in records {
        let stored = Path::new(&record.stored_path);
        if !is_managed_artifact_path(project_dir, record, stored) {
            tracing::warn!(
                artifact_id = %record.id,
                stored_path = %stored.display(),
                "skip deleting retired artifact outside its managed project directory"
            );
            continue;
        }
        match fs::remove_file(stored).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => tracing::warn!(
                artifact_id = %record.id,
                stored_path = %stored.display(),
                %error,
                "failed to delete retired artifact file"
            ),
        }
    }
}

fn is_managed_artifact_path(project_dir: &Path, record: &ArtifactRecord, stored: &Path) -> bool {
    let expected_name = format!("{}__{}", record.id, record.name);
    stored.parent() == Some(project_dir) && stored.file_name() == Some(OsStr::new(&expected_name))
}

pub async fn download_artifact(
    State(state): State<AppState>,
    AxumPath((artifact_id, requested_name)): AxumPath<(String, String)>,
    headers: HeaderMap,
) -> ApiResult<Response> {
    let index = load_index(&state.app_dir).await?;
    let Some(record) = index
        .artifacts
        .into_iter()
        .find(|item| item.id == artifact_id)
    else {
        return Err(AppError::not_found("产物不存在。"));
    };
    let stored = PathBuf::from(&record.stored_path);
    let bytes = fs::read(&stored)
        .await
        .map_err(|error| AppError::not_found(format!("读取产物失败: {error}")))?;
    let file_name = if requested_name.trim().is_empty() {
        record.name
    } else {
        requested_name
    };
    let disposition_type = if is_image_name(&file_name) {
        "inline"
    } else {
        "attachment"
    };
    let content_disposition = format!(
        "{}; filename=\"{}\"; filename*=UTF-8''{}",
        disposition_type,
        ascii_filename(&file_name),
        url_component(&file_name)
    );
    let (status, range_start, content_range) = match requested_open_range(&headers, bytes.len()) {
        Ok(range) => range,
        Err(()) => return range_not_satisfiable_response(bytes.len()),
    };
    let body = bytes[range_start..].to_vec();
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, content_type_for_name(&file_name))
        .header(header::CONTENT_LENGTH, body.len().to_string())
        .header(header::ACCEPT_RANGES, "bytes");
    if let Some(content_range) = content_range {
        builder = builder.header(header::CONTENT_RANGE, content_range);
    }
    let mut response = builder
        .body(Body::from(body))
        .map_err(|error| AppError::internal(format!("构造下载响应失败: {error}")))?;
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&content_disposition)
            .map_err(|error| AppError::internal(format!("构造下载文件名失败: {error}")))?,
    );
    Ok(response)
}

fn requested_open_range(
    headers: &HeaderMap,
    size: usize,
) -> Result<(StatusCode, usize, Option<String>), ()> {
    let Some(value) = headers.get(header::RANGE) else {
        return Ok((StatusCode::OK, 0, None));
    };
    let raw = value.to_str().map_err(|_| ())?;
    let Some(offset) = raw
        .strip_prefix("bytes=")
        .and_then(|value| value.strip_suffix('-'))
    else {
        return Err(());
    };
    if offset.is_empty() || offset.contains(',') {
        return Err(());
    }
    let start = offset.parse::<usize>().map_err(|_| ())?;
    if start >= size {
        return Err(());
    }
    Ok((
        StatusCode::PARTIAL_CONTENT,
        start,
        Some(format!("bytes {start}-{}/{size}", size - 1)),
    ))
}

fn range_not_satisfiable_response(size: usize) -> ApiResult<Response> {
    Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{size}"))
        .header(header::ACCEPT_RANGES, "bytes")
        .body(Body::empty())
        .map_err(|error| AppError::internal(format!("构造 Range 错误响应失败: {error}")))
}

async fn sha256_file(path: &Path) -> ApiResult<String> {
    let bytes = fs::read(path)
        .await
        .map_err(|error| AppError::internal(format!("读取产物摘要失败: {error}")))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn android_version_from_name(project: &str, name: &str) -> Option<(String, u64)> {
    let version = name
        .strip_prefix(&format!("{project}-"))?
        .strip_suffix(".apk")?;
    let parts = version
        .split('.')
        .map(str::parse::<u64>)
        .collect::<Result<Vec<_>, _>>()
        .ok()?;
    if parts.len() != 3 || parts[1] >= 1000 || parts[2] >= 1000 {
        return None;
    }
    let version_code = parts[0]
        .checked_mul(1_000_000)?
        .checked_add(parts[1].checked_mul(1000)?)?
        .checked_add(parts[2])?;
    Some((version.to_string(), version_code))
}

async fn validate_source_file(path: &str) -> ApiResult<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err(AppError::bad_request("产物路径不能为空。"));
    }
    let candidate = PathBuf::from(trimmed);
    if !candidate.is_absolute() {
        return Err(AppError::bad_request("产物路径必须是绝对路径。"));
    }
    if has_parent_component(&candidate) {
        return Err(AppError::bad_request("产物路径不能包含 ..。"));
    }
    let canonical = candidate
        .canonicalize()
        .map_err(|error| AppError::not_found(format!("产物不存在: {error}")))?;
    let metadata = fs::metadata(&canonical)
        .await
        .map_err(|error| AppError::not_found(format!("产物不存在: {error}")))?;
    if !metadata.is_file() {
        return Err(AppError::bad_request("产物路径不是普通文件。"));
    }
    Ok(canonical)
}

fn has_parent_component(path: &Path) -> bool {
    path.components()
        .any(|component| matches!(component, Component::ParentDir))
}

async fn load_index(app_dir: &Path) -> ApiResult<ArtifactIndex> {
    let path = index_path(app_dir);
    match fs::read(&path).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| AppError::internal(format!("读取产物索引失败: {error}"))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(ArtifactIndex::default()),
        Err(error) => Err(AppError::internal(format!("读取产物索引失败: {error}"))),
    }
}

async fn save_index(app_dir: &Path, index: &ArtifactIndex) -> ApiResult<()> {
    let root = artifacts_root(app_dir);
    fs::create_dir_all(&root)
        .await
        .map_err(|error| AppError::internal(format!("创建产物索引目录失败: {error}")))?;
    let data = serde_json::to_vec_pretty(index)
        .map_err(|error| AppError::internal(format!("序列化产物索引失败: {error}")))?;
    let path = index_path(app_dir);
    let tmp = path.with_extension("json.tmp");
    let mut file = fs::File::create(&tmp)
        .await
        .map_err(|error| AppError::internal(format!("写入产物索引失败: {error}")))?;
    file.write_all(&data)
        .await
        .map_err(|error| AppError::internal(format!("写入产物索引失败: {error}")))?;
    file.flush()
        .await
        .map_err(|error| AppError::internal(format!("写入产物索引失败: {error}")))?;
    fs::rename(&tmp, &path)
        .await
        .map_err(|error| AppError::internal(format!("替换产物索引失败: {error}")))?;
    Ok(())
}

fn artifacts_root(app_dir: &Path) -> PathBuf {
    app_dir.join(ARTIFACTS_DIR)
}

fn index_path(app_dir: &Path) -> PathBuf {
    artifacts_root(app_dir).join(INDEX_FILE)
}

fn artifact_name(raw: &str, source: &Path) -> ApiResult<String> {
    let fallback = source
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| AppError::bad_request("无法确定产物文件名。"))?;
    let candidate = non_empty(raw).unwrap_or_else(|| fallback.to_string());
    sanitize_file_name(&candidate)
}

fn sanitize_segment(raw: &str, fallback: &str) -> ApiResult<String> {
    let value = non_empty(raw).unwrap_or_else(|| fallback.to_string());
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches(['-', '.', '_'])
        .to_string();
    if sanitized.is_empty() {
        return Err(AppError::bad_request("项目名无效。"));
    }
    Ok(sanitized)
}

fn sanitize_file_name(raw: &str) -> ApiResult<String> {
    let value = raw.trim();
    if value.is_empty() || value == "." || value == ".." {
        return Err(AppError::bad_request("文件名无效。"));
    }
    if value.contains('/') || value.contains('\\') || value.contains('\0') {
        return Err(AppError::bad_request("文件名不能包含路径分隔符。"));
    }
    Ok(value.to_string())
}

fn non_empty(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn artifact_id() -> String {
    let now = OffsetDateTime::now_utc();
    format!("{}{:09}", now.unix_timestamp(), now.nanosecond())
}

fn now_rfc3339() -> String {
    OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_else(|_| OffsetDateTime::now_utc().unix_timestamp().to_string())
}

fn url_component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match *byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(*byte as char)
            }
            byte => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn ascii_filename(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_graphic() && ch != '"' && ch != '\\' {
                ch
            } else {
                '_'
            }
        })
        .collect::<String>();
    if sanitized.trim().is_empty() {
        "artifact.bin".to_string()
    } else {
        sanitized
    }
}

fn content_type_for_name(name: &str) -> &'static str {
    match Path::new(name).extension().and_then(OsStr::to_str) {
        Some("apk") => "application/vnd.android.package-archive",
        Some("zip") => "application/zip",
        Some("gz") | Some("tgz") => "application/gzip",
        Some("json") => "application/json; charset=utf-8",
        Some("txt") | Some("log") => "text/plain; charset=utf-8",
        Some("html") => "text/html; charset=utf-8",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        Some("ico") => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// Whether the extension corresponds to an image that can be previewed inline.
fn is_image_name(name: &str) -> bool {
    matches!(
        Path::new(name)
            .extension()
            .and_then(OsStr::to_str)
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg" | "bmp")
    )
}

const DOWNLOADS_HTML: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <meta name="color-scheme" content="light dark">
  <title>编译产物下载</title>
  <link rel="icon" href="/assets/favicon.svg" type="image/svg+xml">
  <script>
    (() => {
      try {
        const mode = window.localStorage.getItem("webclx:theme-mode");
        if (mode === "light" || mode === "dark") {
          document.documentElement.dataset.theme = mode;
          document.documentElement.style.colorScheme = mode;
        }
      } catch {}
    })();
  </script>
  <link rel="stylesheet" href="/assets/styles-base.css?v=20260727b">
  <link rel="stylesheet" href="/assets/styles-responsive.css?v=20260727b">
  <style>
    .downloads-page { min-height: 100dvh; }
    .downloads-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; max-width: 1180px; margin: 0 auto; padding: 14px 16px 4px; }
    .downloads-heading-copy { min-width: 0; }
    .downloads-heading h1 { margin: 0; font-size: 20px; }
    .downloads-heading-actions { display: flex; align-items: center; gap: 8px; flex: 0 0 auto; }
    #refresh-artifacts { min-width: 88px; }
    #manage-downloads[hidden] { display: none; }
    .downloads-content { max-width: 1180px; margin: 0 auto; padding: 10px; }
    .downloads-content section { background: var(--panel); border: 1px solid var(--line); border-radius: 8px; padding: 10px; margin-bottom: 10px; }
    table { width: 100%; border-collapse: collapse; table-layout: fixed; }
    th, td { padding: 7px 6px; border-top: 1px solid var(--line); text-align: left; vertical-align: top; font-size: 13px; }
    th { color: var(--muted); font-weight: 650; }
    .sort-button { display: inline-flex; align-items: center; gap: 4px; min-height: 28px; margin: -4px; padding: 4px; border: 0; border-radius: 4px; background: transparent; color: inherit; font: inherit; font-weight: inherit; cursor: pointer; }
    .sort-button:hover { color: var(--text); background: color-mix(in srgb, var(--panel) 80%, var(--line)); }
    .sort-button:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
    .sort-indicator { display: inline-block; width: 10px; color: var(--accent); }
    a { color: var(--accent); text-decoration: none; font-weight: 700; }
    code { display: block; color: var(--muted); font-size: 11px; word-break: break-all; margin-top: 2px; }
    .muted { color: var(--muted); font-size: 13px; }
    .project { white-space: nowrap; font-weight: 650; overflow-wrap: anywhere; }
    .mobile-project { display: none; }
    .file { min-width: 0; overflow-wrap: anywhere; }
    .note { max-width: 360px; overflow-wrap: anywhere; word-break: break-word; }
    .time { white-space: nowrap; }
    .image-preview-overlay { position: fixed; inset: 0; z-index: 9999; background: rgba(0,0,0,0.82); display: flex; align-items: center; justify-content: center; touch-action: none; }
    .image-preview-wrap { position: relative; max-width: 96vw; max-height: 96vh; display: flex; align-items: center; justify-content: center; overflow: hidden; }
    .image-preview-img { max-width: 96vw; max-height: 92vh; user-select: none; -webkit-user-drag: none; cursor: grab; transition: none; will-change: transform; transform-origin: center center; }
    .image-preview-img.zoomed { cursor: grab; }
    .image-preview-img.dragging { cursor: grabbing; }
    .image-preview-close { position: absolute; top: 12px; right: 14px; width: 38px; height: 38px; border: 0; border-radius: 50%; background: rgba(255,255,255,0.16); color: #fff; font-size: 22px; line-height: 1; cursor: pointer; display: flex; align-items: center; justify-content: center; z-index: 2; transition: background 0.15s; }
    .image-preview-close:hover { background: rgba(255,255,255,0.3); }
    .image-preview-toolbar { position: absolute; bottom: 16px; left: 50%; transform: translateX(-50%); display: flex; gap: 8px; align-items: center; background: rgba(0,0,0,0.55); border-radius: 22px; padding: 6px 10px; z-index: 2; }
    .image-preview-toolbar button { width: 34px; height: 34px; border: 0; border-radius: 50%; background: rgba(255,255,255,0.14); color: #fff; font-size: 17px; line-height: 1; cursor: pointer; display: flex; align-items: center; justify-content: center; transition: background 0.15s; }
    .image-preview-toolbar button:hover { background: rgba(255,255,255,0.28); }
    .image-preview-zoom-label { color: #fff; font-size: 12px; min-width: 44px; text-align: center; user-select: none; }
    .image-preview-download { position: absolute; top: 12px; left: 14px; display: inline-flex; align-items: center; gap: 5px; height: 38px; padding: 0 16px; border: 0; border-radius: 19px; background: rgba(255,255,255,0.16); color: #fff; font-size: 13px; font-weight: 650; cursor: pointer; text-decoration: none; transition: background 0.15s; z-index: 2; }
    .image-preview-download:hover { background: rgba(255,255,255,0.3); }
    .image-preview-hint { position: absolute; top: 12px; left: 50%; transform: translateX(-50%); color: rgba(255,255,255,0.6); font-size: 11px; user-select: none; z-index: 2; pointer-events: none; }
    @media (max-width: 720px) { .downloads-heading { align-items: flex-end; } table, thead, tbody, tr, th, td { display: block; } thead { display: none; } td { width: auto !important; border-top: 0; padding: 3px 0; } tr { border-top: 1px solid var(--line); padding: 8px 0; } code { margin-top: 1px; } td.project { display: none; } .mobile-project { display: block; color: var(--muted); font-size: 12px; font-weight: 650; margin-bottom: 2px; overflow-wrap: anywhere; } .time { white-space: normal; } }
  </style>
</head>
<body class="app-shell">
  <div class="page downloads-page">
    <header class="topbar slim browser-topbar">
      <nav class="tabs page-tabs" aria-label="webClx 顶级导航">
        <a class="tab-button topbar-link-button" href="/terminal">终端管理</a>
        <a class="tab-button topbar-link-button" href="/workspace">工作区</a>
        <a class="tab-button topbar-link-button" href="/workspace_history">历史工作区</a>
        <a class="tab-button topbar-link-button" href="/codex_api">Codex_API</a>
        <a class="tab-button topbar-link-button" href="/claude_api">Claude_API</a>
        <a class="tab-button topbar-link-button" href="/settings">设置</a>
        <a class="tab-button topbar-link-button" href="/desktop">远程桌面</a>
        <a class="tab-button topbar-link-button" href="/agent">Agent</a>
        <a class="tab-button topbar-link-button" href="/codex_oauth">Codex_OAuth</a>
        <a class="tab-button topbar-link-button" href="/archives">归档列表</a>
        <a class="tab-button topbar-link-button active" href="/downloads" aria-current="page">编译产物</a>
      </nav>
    </header>
    <div class="downloads-heading">
      <div class="downloads-heading-copy">
        <h1>编译产物下载</h1>
        <div id="status" class="muted">加载中...</div>
      </div>
      <div class="downloads-heading-actions">
        <button id="manage-downloads" class="button secondary" type="button" hidden>下载管理</button>
        <button id="refresh-artifacts" class="button secondary" type="button">立即刷新</button>
      </div>
    </div>
    <main id="content" class="downloads-content"></main>
  </div>
  <div id="image-preview-overlay" class="image-preview-overlay" hidden></div>
  <script>
    const content = document.getElementById("content");
    const status = document.getElementById("status");
    const refresh = document.getElementById("refresh-artifacts");
    const manageDownloads = document.getElementById("manage-downloads");
    let artifactCatalog = [];
    let artifactProjectCount = 0;
    let artifactSort = { key: "published_at", direction: "desc" };
    const isAndroidClient = /(?:^|\s)webClxAndroid\//.test(navigator.userAgent);
    if (isAndroidClient) manageDownloads.hidden = false;
    const fmtSize = (bytes) => {
      if (!Number.isFinite(bytes)) return "-";
      if (bytes >= 1024 * 1024) return (bytes / 1024 / 1024).toFixed(1) + " MB";
      if (bytes >= 1024) return (bytes / 1024).toFixed(1) + " KB";
      return bytes + " B";
    };
    const esc = (value) => String(value ?? "").replace(/[&<>"']/g, ch => ({ "&":"&amp;", "<":"&lt;", ">":"&gt;", '"':"&quot;", "'":"&#39;" }[ch]));
    const pad = (value) => String(value).padStart(2, "0");
    const formatPublishedAt = (value) => {
      const date = new Date(value);
      if (Number.isNaN(date.getTime())) return value || "-";
      return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`;
    };
    const normalizedText = (value) => String(value ?? "").trim();
    const artifactFileMeta = (item) => {
      const label = normalizedText(item.label || item.name);
      const name = normalizedText(item.name);
      return name && name !== label ? name : "";
    };
    const artifactNote = (item) => {
      const note = normalizedText(item.note);
      const label = normalizedText(item.label || item.name);
      const name = normalizedText(item.name);
      return note && note !== label && note !== name ? note : "";
    };
    const IMAGE_EXT = ["png", "jpg", "jpeg", "gif", "webp", "svg", "bmp"];
    const isImageUrl = (item) => {
      const name = normalizedText(item.name).toLowerCase();
      const idx = name.lastIndexOf(".");
      if (idx < 0) return false;
      return IMAGE_EXT.includes(name.slice(idx + 1));
    };
    const artifactSortValue = (item, key) => {
      if (key === "size") return Number(item.size) || 0;
      if (key === "published_at") return Date.parse(item.published_at) || 0;
      if (key === "name") return normalizedText(item.label || item.name);
      if (key === "note") return artifactNote(item);
      return normalizedText(item[key]);
    };
    const compareSortValues = (left, right) => {
      if (typeof left === "number" && typeof right === "number") return left - right;
      return String(left).localeCompare(String(right), "zh-CN", { numeric: true, sensitivity: "base" });
    };
    const sortArtifacts = (items) => [...items].sort((left, right) => {
      const primary = compareSortValues(
        artifactSortValue(left, artifactSort.key),
        artifactSortValue(right, artifactSort.key)
      );
      if (primary) return artifactSort.direction === "asc" ? primary : -primary;
      const byTime = artifactSortValue(right, "published_at") - artifactSortValue(left, "published_at");
      if (byTime) return byTime;
      return compareSortValues(artifactSortValue(left, "name"), artifactSortValue(right, "name"));
    });
    const updateSortHeaders = () => {
      content.querySelectorAll("[data-sort-header]").forEach(header => {
        const active = header.dataset.sortHeader === artifactSort.key;
        const ariaSort = active ? (artifactSort.direction === "asc" ? "ascending" : "descending") : "none";
        header.setAttribute("aria-sort", ariaSort);
        const indicator = header.querySelector(".sort-indicator");
        if (indicator) indicator.textContent = active ? (artifactSort.direction === "asc" ? "▲" : "▼") : "";
      });
    };
    const renderArtifacts = () => {
      const artifacts = sortArtifacts(artifactCatalog);
      status.textContent = artifacts.length ? `共 ${artifactProjectCount} 个项目，${artifacts.length} 个产物` : "暂无产物";
      content.innerHTML = artifacts.length ? `
        <section>
          <table>
            <thead><tr>
              <th data-sort-header="project" aria-sort="none"><button class="sort-button" type="button" data-sort="project">项目<span class="sort-indicator" aria-hidden="true"></span></button></th>
              <th data-sort-header="name" aria-sort="none"><button class="sort-button" type="button" data-sort="name">文件<span class="sort-indicator" aria-hidden="true"></span></button></th>
              <th data-sort-header="size" aria-sort="none"><button class="sort-button" type="button" data-sort="size">大小<span class="sort-indicator" aria-hidden="true"></span></button></th>
              <th data-sort-header="published_at" aria-sort="none"><button class="sort-button" type="button" data-sort="published_at">发布时间<span class="sort-indicator" aria-hidden="true"></span></button></th>
              <th data-sort-header="note" aria-sort="none"><button class="sort-button" type="button" data-sort="note">说明<span class="sort-indicator" aria-hidden="true"></span></button></th>
            </tr></thead>
            <tbody>${artifacts.map(item => {
              const fileMeta = artifactFileMeta(item);
              const note = artifactNote(item);
              return `
              <tr>
                <td class="project">${esc(item.project)}</td>
                <td class="file"><span class="mobile-project">${esc(item.project)}</span><a href="${esc(item.download_url)}">${esc(item.label || item.name)}</a>${isImageUrl(item) ? ` <button class="image-preview-btn" type="button" data-preview-url="${esc(item.download_url)}" data-preview-name="${esc(item.name)}" aria-label="预览图片">🔍 预览</button>` : ""}${fileMeta ? `<code>${esc(fileMeta)}</code>` : ""}</td>
                <td>${fmtSize(Number(item.size))}</td>
                <td class="time">${esc(formatPublishedAt(item.published_at))}</td>
                <td class="note">${esc(note)}</td>
              </tr>
            `;
            }).join("")}</tbody>
          </table>
        </section>
      ` : '<section class="muted">暂无编译产物。发布后会显示在这里。</section>';
      updateSortHeaders();
    };
    async function load() {
      refresh.disabled = true;
      refresh.textContent = "刷新中...";
      try {
        const response = await fetch("/api/artifacts", { cache: "no-store" });
        if (!response.ok) throw new Error(await response.text());
        const data = await response.json();
        const projects = Array.isArray(data.projects) ? data.projects : [];
        const flatArtifacts = projects.flatMap(project =>
          (project.artifacts || []).map(item => ({ ...item, project: project.project || item.project || "" }))
        );
        artifactCatalog = flatArtifacts;
        artifactProjectCount = projects.length;
        renderArtifacts();
      } catch (error) {
        status.textContent = "加载失败: " + error.message;
      } finally {
        refresh.disabled = false;
        refresh.textContent = "立即刷新";
      }
    }
    manageDownloads.addEventListener("click", () => {
      window.location.href = "webclx://downloads";
    });
    content.addEventListener("click", event => {
      const button = event.target.closest("[data-sort]");
      if (!button) return;
      const key = button.dataset.sort;
      artifactSort = artifactSort.key === key
        ? { key, direction: artifactSort.direction === "asc" ? "desc" : "asc" }
        : { key, direction: "asc" };
      renderArtifacts();
    });
    // ===== Image preview popup with zoom / pan / drag =====
    const previewOverlay = document.getElementById("image-preview-overlay");
    let previewScale = 1;
    let previewX = 0;
    let previewY = 0;
    let isDragging = false;
    let dragStartX = 0;
    let dragStartY = 0;
    let dragStartTX = 0;
    let dragStartTY = 0;
    const MIN_SCALE = 0.2;
    const MAX_SCALE = 8;

    const clampScale = (s) => Math.max(MIN_SCALE, Math.min(MAX_SCALE, s));
    const applyTransform = (img) => {
      img.style.transform = `translate(${previewX}px, ${previewY}px) scale(${previewScale})`;
    };
    const updateZoomLabel = (label) => { if (label) label.textContent = Math.round(previewScale * 100) + "%"; };

    function openImagePreview(url, name) {
      previewScale = 1;
      previewX = 0;
      previewY = 0;
      previewOverlay.hidden = false;
      previewOverlay.innerHTML = "";
      const wrap = document.createElement("div");
      wrap.className = "image-preview-wrap";
      const downloadLink = document.createElement("a");
      downloadLink.className = "image-preview-download";
      downloadLink.href = url + "?download=1";
      downloadLink.download = name || "";
      downloadLink.textContent = "⬇ 下载";
      const hint = document.createElement("div");
      hint.className = "image-preview-hint";
      hint.textContent = "滚轮/双击缩放 · 拖动平移";
      const closeBtn = document.createElement("button");
      closeBtn.className = "image-preview-close";
      closeBtn.type = "button";
      closeBtn.setAttribute("aria-label", "关闭预览");
      closeBtn.textContent = "\u2715";
      const toolbar = document.createElement("div");
      toolbar.className = "image-preview-toolbar";
      const zoomOut = document.createElement("button");
      zoomOut.type = "button";
      zoomOut.setAttribute("aria-label", "缩小");
      zoomOut.textContent = "\u2212";
      const zoomLabel = document.createElement("span");
      zoomLabel.className = "image-preview-zoom-label";
      const zoomIn = document.createElement("button");
      zoomIn.type = "button";
      zoomIn.setAttribute("aria-label", "放大");
      zoomIn.textContent = "+";
      const resetBtn = document.createElement("button");
      resetBtn.type = "button";
      resetBtn.setAttribute("aria-label", "重置");
      resetBtn.textContent = "\u21BA";
      toolbar.append(zoomOut, zoomLabel, zoomIn, resetBtn);
      const img = document.createElement("img");
      img.className = "image-preview-img";
      img.alt = name || "预览图片";
      img.draggable = false;
      img.src = url;
      const zoomBy = (delta) => {
        previewScale = clampScale(previewScale + delta);
        if (previewScale <= 1) { previewX = 0; previewY = 0; }
        applyTransform(img);
        updateZoomLabel(zoomLabel);
        img.classList.toggle("zoomed", previewScale > 1);
      };
      zoomOut.addEventListener("click", (e) => { e.stopPropagation(); zoomBy(-0.25); });
      zoomIn.addEventListener("click", (e) => { e.stopPropagation(); zoomBy(0.25); });
      resetBtn.addEventListener("click", (e) => { e.stopPropagation(); previewScale = 1; previewX = 0; previewY = 0; applyTransform(img); updateZoomLabel(zoomLabel); img.classList.remove("zoomed"); });
      img.addEventListener("wheel", (e) => {
        e.preventDefault();
        zoomBy(e.deltaY < 0 ? 0.15 : -0.15);
      }, { passive: false });
      img.addEventListener("dblclick", (e) => {
        e.preventDefault();
        if (previewScale > 1) { previewScale = 1; previewX = 0; previewY = 0; img.classList.remove("zoomed"); }
        else { previewScale = 2.5; img.classList.add("zoomed"); }
        applyTransform(img);
        updateZoomLabel(zoomLabel);
      });
      img.addEventListener("pointerdown", (e) => {
        if (previewScale <= 1) return;
        isDragging = true;
        img.classList.add("dragging");
        img.setPointerCapture(e.pointerId);
        dragStartX = e.clientX;
        dragStartY = e.clientY;
        dragStartTX = previewX;
        dragStartTY = previewY;
      });
      img.addEventListener("pointermove", (e) => {
        if (!isDragging) return;
        previewX = dragStartTX + (e.clientX - dragStartX);
        previewY = dragStartTY + (e.clientY - dragStartY);
        applyTransform(img);
      });
      const endDrag = (e) => {
        if (!isDragging) return;
        isDragging = false;
        img.classList.remove("dragging");
        try { img.releasePointerCapture(e.pointerId); } catch {}
      };
      img.addEventListener("pointerup", endDrag);
      img.addEventListener("pointercancel", endDrag);
      const closePreview = () => {
        previewOverlay.hidden = true;
        previewOverlay.innerHTML = "";
        document.removeEventListener("keydown", onKey);
      };
      const onKey = (e) => { if (e.key === "Escape") closePreview(); };
      closeBtn.addEventListener("click", closePreview);
      previewOverlay.addEventListener("click", (e) => { if (e.target === previewOverlay) closePreview(); });
      document.addEventListener("keydown", onKey);
      wrap.append(img);
      previewOverlay.append(downloadLink, hint, closeBtn, wrap, toolbar);
      img.addEventListener("load", () => {
        updateZoomLabel(zoomLabel);
        applyTransform(img);
      });
    }

    content.addEventListener("click", event => {
      const btn = event.target.closest(".image-preview-btn");
      if (btn) {
        event.preventDefault();
        openImagePreview(btn.dataset.previewUrl, btn.dataset.previewName);
        return;
      }
    });

    refresh.addEventListener("click", load);
    load();
  </script>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::{
        ANDROID_MINIMUM_SUPPORTED_VERSION, ARTIFACTS_DIR, ArtifactIndex, ArtifactRecord,
        DOWNLOADS_HTML, PublishArtifactRequest, android_update_manifest, android_version_from_name,
        enforce_artifact_retention, group_artifacts_by_project, list_artifacts, publish_artifact,
        requested_open_range, save_index,
    };
    use crate::{AppState, agent, auth, codex_proxy, frpc, proxy, quota, settings, terminal};
    use axum::{
        Json,
        extract::{Path as AxumPath, State},
        http::{HeaderMap, HeaderValue, StatusCode, header},
    };
    use std::{
        net::SocketAddr,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    #[tokio::test]
    async fn publishing_four_versions_retains_only_the_latest_three() {
        let app_dir = TestAppDir::new("webclx-artifact-retention-test");
        let state = test_state(app_dir.path());

        for version in 1..=4 {
            let name = format!("demo-1.0.{version}.apk");
            let source = app_dir.path().join(format!("source-{version}.apk"));
            tokio::fs::write(&source, format!("version-{version}"))
                .await
                .unwrap();
            let Json(_) = publish_artifact(
                State(state.clone()),
                Json(PublishArtifactRequest {
                    project: "demo".to_string(),
                    path: source.display().to_string(),
                    name,
                    label: format!("Demo 1.0.{version}"),
                    note: String::new(),
                }),
            )
            .await
            .unwrap();
        }

        let Json(list) = list_artifacts(State(state)).await.unwrap();
        assert_eq!(list.projects.len(), 1);
        let names = list.projects[0]
            .artifacts
            .iter()
            .map(|artifact| artifact.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["demo-1.0.4.apk", "demo-1.0.3.apk", "demo-1.0.2.apk"]);

        let mut stored_files = tokio::fs::read_dir(app_dir.path().join(ARTIFACTS_DIR).join("demo"))
            .await
            .unwrap();
        let mut stored_names = Vec::new();
        while let Some(entry) = stored_files.next_entry().await.unwrap() {
            stored_names.push(entry.file_name().to_string_lossy().into_owned());
        }
        assert_eq!(stored_names.len(), 3);
        assert!(
            stored_names
                .iter()
                .all(|name| !name.ends_with("__demo-1.0.1.apk"))
        );
    }

    #[tokio::test]
    async fn listing_discovers_files_dropped_into_project_directories() {
        let app_dir = TestAppDir::new("webclx-artifact-drop-discovery-test");
        let state = test_state(app_dir.path());
        let project_dir = app_dir.path().join(ARTIFACTS_DIR).join("demo");
        tokio::fs::create_dir_all(&project_dir).await.unwrap();
        let dropped = project_dir.join("demo-2.0.0.zip");
        tokio::fs::write(&dropped, b"dropped-artifact")
            .await
            .unwrap();

        let Json(list) = list_artifacts(State(state)).await.unwrap();

        assert_eq!(list.projects.len(), 1);
        assert_eq!(list.projects[0].project, "demo");
        assert_eq!(list.projects[0].artifacts.len(), 1);
        let artifact = &list.projects[0].artifacts[0];
        assert_eq!(artifact.name, "demo-2.0.0.zip");
        assert_eq!(artifact.size, b"dropped-artifact".len() as u64);
        assert_eq!(artifact.sha256.len(), 64);
        assert!(!dropped.exists(), "discovered files should move into managed storage");
        assert!(Path::new(&artifact.stored_path).exists());
    }

    #[tokio::test]
    async fn published_android_artifact_exposes_a_verified_update_manifest() {
        let app_dir = TestAppDir::new("webclx-android-update-manifest-test");
        let state = test_state(app_dir.path());
        let source = app_dir.path().join("webClx-1.8.2.apk");
        tokio::fs::write(&source, b"signed-apk-fixture")
            .await
            .unwrap();
        let Json(record) = publish_artifact(
            State(state.clone()),
            Json(PublishArtifactRequest {
                project: "webClx".to_string(),
                path: source.display().to_string(),
                name: "webClx-1.8.2.apk".to_string(),
                label: "webClx Android 1.8.2".to_string(),
                note: "release notes".to_string(),
            }),
        )
        .await
        .unwrap();

        assert_eq!(record.sha256.len(), 64);
        let Json(manifest) = android_update_manifest(State(state), AxumPath("webClx".to_string()))
            .await
            .unwrap();
        assert_eq!(manifest.version, "1.8.2");
        assert_eq!(manifest.version_code, 1_008_002);
        assert_eq!(manifest.minimum_supported_version, ANDROID_MINIMUM_SUPPORTED_VERSION);
        assert!(!manifest.mandatory);
        assert_eq!(manifest.sha256, record.sha256);
        assert_eq!(manifest.size, b"signed-apk-fixture".len() as u64);
        assert_eq!(manifest.download_url, record.download_url);
    }

    #[test]
    fn android_versions_and_open_ranges_are_strictly_validated() {
        assert_eq!(
            android_version_from_name("webClx", "webClx-1.7.17.apk"),
            Some(("1.7.17".to_string(), 1_007_017))
        );
        assert_eq!(android_version_from_name("webClx", "other-1.7.17.apk"), None);
        assert_eq!(android_version_from_name("webClx", "webClx-latest.apk"), None);

        let mut headers = HeaderMap::new();
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=4-"));
        assert_eq!(
            requested_open_range(&headers, 10),
            Ok((StatusCode::PARTIAL_CONTENT, 4, Some("bytes 4-9/10".to_string())))
        );
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=10-"));
        assert!(requested_open_range(&headers, 10).is_err());
        headers.insert(header::RANGE, HeaderValue::from_static("bytes=1-2"));
        assert!(requested_open_range(&headers, 10).is_err());
    }

    #[tokio::test]
    async fn startup_retention_prunes_existing_artifacts_to_three_per_project() {
        let app_dir = TestAppDir::new("webclx-artifact-startup-retention-test");
        let state = test_state(app_dir.path());
        let project_dir = app_dir.path().join(ARTIFACTS_DIR).join("demo");
        tokio::fs::create_dir_all(&project_dir).await.unwrap();
        let mut artifacts = Vec::new();
        for version in 1..=4 {
            let name = format!("demo-1.0.{version}.apk");
            let id = format!("artifact-{version}");
            let stored = project_dir.join(format!("{id}__{name}"));
            tokio::fs::write(&stored, format!("version-{version}"))
                .await
                .unwrap();
            let mut artifact =
                test_artifact(&name, "demo", &format!("2026-07-28T00:00:0{version}Z"));
            artifact.id = id;
            artifact.stored_path = stored.display().to_string();
            artifacts.push(artifact);
        }
        save_index(app_dir.path(), &ArtifactIndex { artifacts })
            .await
            .unwrap();

        enforce_artifact_retention(app_dir.path()).await.unwrap();

        let Json(list) = list_artifacts(State(state)).await.unwrap();
        let names = list.projects[0]
            .artifacts
            .iter()
            .map(|artifact| artifact.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["demo-1.0.4.apk", "demo-1.0.3.apk", "demo-1.0.2.apk"]);
        assert!(!project_dir.join("artifact-1__demo-1.0.1.apk").exists());
    }

    #[test]
    fn downloads_page_formats_published_at_before_rendering() {
        assert!(
            DOWNLOADS_HTML.contains("formatPublishedAt(item.published_at)"),
            "downloads page should render formatted artifact publish time"
        );
        assert!(
            !DOWNLOADS_HTML.contains("<td>${esc(item.published_at)}</td>"),
            "downloads page should not expose raw RFC3339 publish time"
        );
    }

    #[test]
    fn downloads_page_can_refresh_discovered_artifacts_immediately() {
        assert!(
            DOWNLOADS_HTML.contains(r#"id="refresh-artifacts""#),
            "downloads page should expose an immediate refresh button"
        );
        assert!(DOWNLOADS_HTML.contains("立即刷新"));
        assert!(
            DOWNLOADS_HTML.contains("refresh.addEventListener(\"click\", load)"),
            "refresh button should rescan and reload the artifact catalog"
        );
    }

    #[test]
    fn downloads_page_exposes_native_download_management_to_android_clients() {
        assert!(DOWNLOADS_HTML.contains(r#"id="manage-downloads""#));
        assert!(DOWNLOADS_HTML.contains("webClxAndroid\\/"));
        assert!(DOWNLOADS_HTML.contains(r#"window.location.href = "webclx://downloads""#));
    }

    #[test]
    fn downloads_page_keeps_each_artifact_independently_sortable() {
        assert!(
            DOWNLOADS_HTML.contains(r#"<td class="project">${esc(item.project)}</td>"#),
            "each sortable artifact row should retain its project value"
        );
        assert!(
            DOWNLOADS_HTML.contains("artifacts.map(item =>"),
            "each artifact should render as an independent table row"
        );
        assert!(
            DOWNLOADS_HTML.contains("mobile-project"),
            "mobile version rows should retain their software name"
        );
    }

    #[test]
    fn downloads_page_sorts_by_time_by_default_and_allows_column_sorting() {
        assert!(
            DOWNLOADS_HTML
                .contains(r#"let artifactSort = { key: "published_at", direction: "desc" };"#),
            "downloads should default to newest artifacts first"
        );
        assert!(
            DOWNLOADS_HTML.contains("sortArtifacts(artifactCatalog)"),
            "downloads should sort the global artifact list instead of preserving project groups"
        );
        for (key, label) in [
            ("project", "项目"),
            ("name", "文件"),
            ("size", "大小"),
            ("published_at", "发布时间"),
            ("note", "说明"),
        ] {
            assert!(
                DOWNLOADS_HTML.contains(&format!(
                    r#"<button class="sort-button" type="button" data-sort="{key}">{label}"#
                )),
                "downloads should expose a sortable {label} column"
            );
        }
        assert!(
            DOWNLOADS_HTML.contains("header.setAttribute(\"aria-sort\", ariaSort)"),
            "the active table header should expose its sort direction"
        );
    }

    #[test]
    fn groups_projects_by_latest_artifact_publish_time() {
        let projects = group_artifacts_by_project(vec![
            test_artifact("aaa-old", "aaa", "2026-06-20T09:00:00Z"),
            test_artifact("zzz-new", "zzz", "2026-06-21T09:00:00Z"),
            test_artifact("aaa-newest", "aaa", "2026-06-22T09:00:00Z"),
        ]);

        let project_names = projects
            .iter()
            .map(|project| project.project.as_str())
            .collect::<Vec<_>>();
        assert_eq!(project_names, vec!["aaa", "zzz"]);
        assert_eq!(projects[0].artifacts[0].name, "aaa-newest");
        assert_eq!(projects[0].artifacts[1].name, "aaa-old");
    }

    #[test]
    fn downloads_page_uses_compact_table_and_deduplicates_repeated_text() {
        assert!(
            DOWNLOADS_HTML.contains("flatArtifacts ="),
            "downloads page should render one compact artifact table"
        );
        assert!(
            DOWNLOADS_HTML.contains("artifactFileMeta(item)"),
            "downloads page should hide duplicate label/name text"
        );
        assert!(
            DOWNLOADS_HTML.contains("artifactNote(item)"),
            "downloads page should hide duplicate note text"
        );
    }

    #[test]
    fn downloads_page_follows_the_system_color_scheme() {
        assert!(
            DOWNLOADS_HTML.contains(r#"<meta name="color-scheme" content="light dark">"#),
            "downloads page should advertise native light and dark controls"
        );
        assert!(
            DOWNLOADS_HTML.contains("webclx:theme-mode"),
            "downloads page should follow the main application's saved theme"
        );
        assert!(
            DOWNLOADS_HTML.contains("/assets/styles-base.css"),
            "downloads page should reuse the main application styles"
        );
        assert!(
            DOWNLOADS_HTML.contains("border-top: 1px solid var(--line)"),
            "table and mobile row separators should use theme variables"
        );
    }

    #[test]
    fn downloads_page_uses_the_shared_top_level_navigation() {
        for (path, label) in [
            ("/terminal", "终端管理"),
            ("/workspace", "工作区"),
            ("/workspace_history", "历史工作区"),
            ("/codex_api", "Codex_API"),
            ("/claude_api", "Claude_API"),
            ("/settings", "设置"),
            ("/desktop", "远程桌面"),
            ("/agent", "Agent"),
            ("/codex_oauth", "Codex_OAuth"),
            ("/archives", "归档列表"),
        ] {
            assert!(
                DOWNLOADS_HTML.contains(&format!(r#"href="{path}">{label}</a>"#)),
                "downloads page should link to {label}"
            );
        }
        assert!(
            DOWNLOADS_HTML.contains(
                r#"class="tab-button topbar-link-button active" href="/downloads" aria-current="page""#
            ),
            "downloads should be marked as the current child page"
        );
    }

    #[test]
    fn downloads_page_contains_long_text_without_horizontal_overflow() {
        assert!(
            DOWNLOADS_HTML.contains("table-layout: fixed"),
            "artifact table should keep its layout inside the viewport"
        );
        assert!(
            DOWNLOADS_HTML.contains("overflow-wrap: anywhere"),
            "long artifact notes should wrap at arbitrary boundaries"
        );
        assert!(
            DOWNLOADS_HTML.contains(".file { min-width: 0;"),
            "file column should not impose a minimum width on the table"
        );
    }

    fn test_artifact(name: &str, project: &str, published_at: &str) -> ArtifactRecord {
        ArtifactRecord {
            id: name.to_string(),
            project: project.to_string(),
            name: name.to_string(),
            label: name.to_string(),
            note: String::new(),
            size: 1,
            sha256: String::new(),
            source_path: format!("/tmp/{name}"),
            stored_path: format!("/tmp/{name}"),
            download_url: format!("/api/artifacts/download/{name}/{name}"),
            published_at: published_at.to_string(),
        }
    }

    fn test_state(app_dir: &Path) -> AppState {
        std::fs::create_dir_all(app_dir).unwrap();
        AppState {
            static_dir: app_dir.join("static"),
            listen_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            version: "test".to_string(),
            app_dir: app_dir.to_path_buf(),
            local_api_token: std::sync::Arc::from("test-local-api-token"),
            workspace_settings: settings::SettingsManager::load(app_dir).unwrap(),
            auth_manager: auth::AuthPresetManager::load(app_dir).unwrap(),
            codex_oauth_manager: auth::CodexOAuthManager::new(),
            codex_proxy_history: codex_proxy::CodexProxyHistory::new(),
            proxy_manager: proxy::ProxyManager::load(app_dir).unwrap(),
            quota_reset_cache: crate::quota_reset_cache::QuotaResetCache::new(),
            quota_manager: quota::QuotaConfigManager::load(app_dir),
            frpc_manager: frpc::FrpcManager::load(app_dir, 0).unwrap(),
            frps_manager: frpc::FrpsManager::load(app_dir).unwrap(),
            frp_role_manager: frpc::FrpRoleManager::load(app_dir, 0).unwrap(),
            terminal_manager: terminal::TerminalManager::new(
                app_dir.join(".webclx-terminal-sessions.json"),
            ),
            preset_test_scheduler: auth::PresetTestScheduler::new(
                &app_dir.join(".webclx-terminal-sessions.json"),
            ),
            preset_run_lease_manager: auth::PresetRunLeaseManager::new(
                app_dir.join(".webclx-preset-run-lease.json"),
            ),
            agent_manager: agent::AgentManager::new(app_dir),
            agent_config: agent::AgentConfigManager::new(app_dir),
        }
    }

    struct TestAppDir(PathBuf);

    impl TestAppDir {
        fn new(label: &str) -> Self {
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            Self(std::env::temp_dir().join(format!("{label}-{nanos}")))
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestAppDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}
