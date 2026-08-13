use std::{
    cmp::Ordering,
    io::ErrorKind,
    path::{Component, Path, PathBuf},
};

use axum::{
    Json,
    body::Body,
    extract::{Query, State},
    http::{StatusCode, header},
    response::Response,
};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tracing::{info, warn};

use crate::{ApiResult, AppError, AppState, PathQuery};

const MAX_EDITABLE_FILE_SIZE: u64 = 1024 * 1024;
const MAX_WORKSPACE_ICON_SIZE: u64 = 2 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct WorkspaceIconQuery {
    #[serde(default)]
    path: String,
    icon_path: String,
    #[serde(default)]
    search: String,
}

#[derive(Debug, Serialize)]
pub struct DirectoryResponse {
    path: String,
    display_path: String,
    parent_path: Option<String>,
    entries: Vec<DirectoryEntry>,
}

#[derive(Debug, Serialize)]
pub struct DirectoryEntry {
    name: String,
    path: String,
    kind: &'static str,
    size: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct FileResponse {
    path: String,
    display_path: String,
    editable: bool,
    size: u64,
    content: String,
    message: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SaveFileRequest {
    path: String,
    content: String,
}

#[derive(Debug, Serialize)]
pub struct SaveFileResponse {
    ok: bool,
    saved_bytes: usize,
}

#[derive(Debug, Deserialize)]
pub struct RenamePathRequest {
    path: String,
    name: String,
}

#[derive(Debug, Serialize)]
pub struct RenamePathResponse {
    ok: bool,
    old_path: String,
    path: String,
    display_path: String,
    kind: &'static str,
}

pub async fn list_entries(
    State(state): State<AppState>,
    Query(query): Query<PathQuery>,
) -> ApiResult<Json<DirectoryResponse>> {
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let show_dot_entries = state.show_dot_entries();
    let directory = resolve_directory_path(&base_dir, &query.path)?;
    let relative = relative_path(&base_dir, &directory)?;
    let display_path = display_path(&base_dir, &display_root, &directory);
    let parent_path = parent_relative_path(&relative);

    let mut reader = fs::read_dir(&directory)
        .await
        .map_err(|error| AppError::internal(format!("读取目录失败: {error}")))?;

    let mut entries = Vec::new();
    while let Some(entry) = reader
        .next_entry()
        .await
        .map_err(|error| AppError::internal(format!("读取目录项失败: {error}")))?
    {
        if !show_dot_entries && is_dot_prefixed(&entry.file_name()) {
            continue;
        }

        let file_type = match entry.file_type().await {
            Ok(file_type) => file_type,
            Err(error) => {
                warn!("skip unreadable dir entry type {}: {error}", entry.path().display());
                continue;
            }
        };
        let entry_path = entry.path();
        let relative_entry_path = relative_path(&base_dir, &entry_path)?;
        let kind = if file_type.is_dir() {
            "dir"
        } else if file_type.is_file() {
            "file"
        } else if file_type.is_symlink() {
            "symlink"
        } else {
            "other"
        };

        entries.push(DirectoryEntry {
            name: entry.file_name().to_string_lossy().to_string(),
            path: path_to_string(&relative_entry_path),
            kind,
            size: if file_type.is_file() {
                match entry.metadata().await {
                    Ok(metadata) => Some(metadata.len()),
                    Err(error) => {
                        warn!("skip unreadable file metadata {}: {error}", entry_path.display());
                        None
                    }
                }
            } else {
                None
            },
        });
    }

    entries.sort_by(compare_entries);

    Ok(Json(DirectoryResponse {
        path: path_to_string(&relative),
        display_path,
        parent_path,
        entries,
    }))
}

pub async fn read_workspace_icon(
    State(state): State<AppState>,
    Query(query): Query<WorkspaceIconQuery>,
) -> ApiResult<Response> {
    let icon_path = resolve_workspace_icon_path(
        &state.workspace_root(),
        &query.path,
        &query.icon_path,
        query.search == "nearest",
    )?;
    let metadata = fs::metadata(&icon_path)
        .await
        .map_err(|error| AppError::not_found(format!("项目图标不存在: {error}")))?;
    if metadata.len() > MAX_WORKSPACE_ICON_SIZE {
        return Err(AppError::bad_request("项目图标不能超过 2 MB。"));
    }
    let bytes = fs::read(&icon_path)
        .await
        .map_err(|error| AppError::internal(format!("读取项目图标失败: {error}")))?;
    let content_type = workspace_icon_content_type(&icon_path).ok_or_else(|| {
        AppError::bad_request("项目图标仅支持 ICO、SVG、PNG、WebP、GIF 或 JPEG。")
    })?;

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "private, max-age=60")
        .body(Body::from(bytes))
        .map_err(|error| AppError::internal(format!("创建项目图标响应失败: {error}")))
}

pub async fn read_file(
    State(state): State<AppState>,
    Query(query): Query<PathQuery>,
) -> ApiResult<Json<FileResponse>> {
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let file_path = resolve_file_path(&base_dir, &query.path)?;
    let metadata = fs::metadata(&file_path)
        .await
        .map_err(|error| AppError::internal(format!("读取文件信息失败: {error}")))?;
    let relative = relative_path(&base_dir, &file_path)?;
    let size = metadata.len();

    if size > MAX_EDITABLE_FILE_SIZE {
        return Ok(Json(FileResponse {
            path: path_to_string(&relative),
            display_path: display_path(&base_dir, &display_root, &file_path),
            editable: false,
            size,
            content: String::new(),
            message: Some(format!(
                "文件超过 {} KB，当前页面只支持在线编辑较小的 UTF-8 文本文件。",
                MAX_EDITABLE_FILE_SIZE / 1024
            )),
        }));
    }

    let bytes = fs::read(&file_path)
        .await
        .map_err(|error| AppError::internal(format!("读取文件失败: {error}")))?;
    let read_size = bytes.len() as u64;

    let content = String::from_utf8(bytes).map_err(|_| {
        AppError::bad_request("当前只支持 UTF-8 文本文件，二进制文件不能在网页里直接编辑。")
    })?;

    if content.as_bytes().contains(&0) {
        return Err(AppError::bad_request("文件包含二进制内容，当前页面只支持 UTF-8 文本文件。"));
    }

    Ok(Json(FileResponse {
        path: path_to_string(&relative),
        display_path: display_path(&base_dir, &display_root, &file_path),
        editable: true,
        size: read_size,
        content,
        message: None,
    }))
}

pub async fn save_file(
    State(state): State<AppState>,
    Json(payload): Json<SaveFileRequest>,
) -> ApiResult<Json<SaveFileResponse>> {
    let file_path = resolve_file_path(&state.workspace_root(), &payload.path)?;
    let content = if should_merge_existing_sections(&file_path) {
        let existing = fs::read_to_string(&file_path)
            .await
            .map_err(|error| AppError::internal(format!("读取原始 config.toml 失败: {error}")))?;
        merge_toml_front_matter(&payload.content, &existing)
    } else {
        payload.content
    };

    fs::write(&file_path, content.as_bytes())
        .await
        .map_err(|error| AppError::internal(format!("保存文件失败: {error}")))?;
    info!(
        path = %file_path.display(),
        saved_bytes = content.len(),
        "saved workspace file"
    );

    Ok(Json(SaveFileResponse {
        ok: true,
        saved_bytes: content.len(),
    }))
}

pub async fn rename_path(
    State(state): State<AppState>,
    Json(payload): Json<RenamePathRequest>,
) -> ApiResult<Json<RenamePathResponse>> {
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let source_path = resolve_rename_path(&base_dir, &payload.path)?;
    let file_type = fs::symlink_metadata(&source_path)
        .await
        .map_err(|error| AppError::not_found(format!("路径不存在: {error}")))?
        .file_type();
    if !file_type.is_file() && !file_type.is_dir() {
        return Err(AppError::bad_request("只能重命名普通文件或文件夹。"));
    }

    let new_name = validate_entry_name(&payload.name)?;
    let parent = source_path
        .parent()
        .ok_or_else(|| AppError::bad_request("当前路径不能重命名。"))?;
    let target_path = parent.join(new_name);

    if target_path == source_path {
        let relative = relative_path(&base_dir, &source_path)?;
        return Ok(Json(RenamePathResponse {
            ok: true,
            old_path: path_to_string(&relative),
            path: path_to_string(&relative),
            display_path: display_path(&base_dir, &display_root, &source_path),
            kind: if file_type.is_dir() { "dir" } else { "file" },
        }));
    }

    match fs::symlink_metadata(&target_path).await {
        Ok(_) => return Err(AppError::bad_request("目标名称已存在。")),
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(AppError::internal(format!("检查目标路径失败: {error}"))),
    }

    fs::rename(&source_path, &target_path)
        .await
        .map_err(|error| AppError::internal(format!("重命名失败: {error}")))?;
    info!(
        old_path = %source_path.display(),
        path = %target_path.display(),
        "renamed workspace path"
    );

    let old_relative = relative_path(&base_dir, &source_path)?;
    let new_relative = relative_path(&base_dir, &target_path)?;
    Ok(Json(RenamePathResponse {
        ok: true,
        old_path: path_to_string(&old_relative),
        path: path_to_string(&new_relative),
        display_path: display_path(&base_dir, &display_root, &target_path),
        kind: if file_type.is_dir() { "dir" } else { "file" },
    }))
}

pub fn resolve_directory_path(base_dir: &Path, requested: &str) -> ApiResult<PathBuf> {
    let candidate = resolve_path(base_dir, requested)?;
    let metadata = std::fs::metadata(&candidate)
        .map_err(|error| AppError::not_found(format!("目录不存在: {error}")))?;
    if !metadata.is_dir() {
        return Err(AppError::bad_request("目标不是目录。"));
    }
    Ok(candidate)
}

pub fn resolve_terminal_directory_path(base_dir: &Path, requested: &str) -> ApiResult<PathBuf> {
    let candidate = resolve_terminal_path(base_dir, requested)?;
    let metadata = std::fs::metadata(&candidate)
        .map_err(|error| AppError::not_found(format!("目录不存在: {error}")))?;
    if !metadata.is_dir() {
        return Err(AppError::bad_request("目标不是目录。"));
    }
    Ok(candidate)
}

pub fn resolve_file_path(base_dir: &Path, requested: &str) -> ApiResult<PathBuf> {
    let candidate = resolve_path(base_dir, requested)?;
    let metadata = std::fs::metadata(&candidate)
        .map_err(|error| AppError::not_found(format!("文件不存在: {error}")))?;
    if !metadata.is_file() {
        return Err(AppError::bad_request("目标不是普通文件。"));
    }
    Ok(candidate)
}

fn resolve_workspace_icon_path(
    base_dir: &Path,
    requested_dir: &str,
    icon_path: &str,
    nearest: bool,
) -> ApiResult<PathBuf> {
    let relative_icon = normalize_workspace_icon_relative_path(icon_path)?;
    let mut directory = resolve_directory_path(base_dir, requested_dir)?;
    let scope_root = access_root(base_dir);

    loop {
        let candidate = directory.join(&relative_icon);
        match candidate.canonicalize() {
            Ok(canonical) if canonical.is_file() && canonical.starts_with(&directory) => {
                return Ok(canonical);
            }
            Ok(_) => {}
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => {
                return Err(AppError::not_found(format!("项目图标不可访问: {error}")));
            }
        }

        if !nearest || directory == scope_root {
            break;
        }
        let Some(parent) = directory.parent() else {
            break;
        };
        if !parent.starts_with(scope_root) {
            break;
        }
        directory = parent.to_path_buf();
    }

    Err(AppError::not_found("项目图标不存在。"))
}

fn normalize_workspace_icon_relative_path(raw: &str) -> ApiResult<PathBuf> {
    let normalized = raw.trim().replace('\\', "/");
    if normalized.is_empty() || normalized.len() > 240 || Path::new(&normalized).is_absolute() {
        return Err(AppError::bad_request("项目图标必须使用项目内的相对路径。"));
    }

    let mut result = PathBuf::new();
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(part) => result.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::bad_request("项目图标路径不能离开项目目录。"));
            }
        }
    }
    if workspace_icon_content_type(&result).is_none() {
        return Err(AppError::bad_request("项目图标仅支持 ICO、SVG、PNG、WebP、GIF 或 JPEG。"));
    }
    Ok(result)
}

fn workspace_icon_content_type(path: &Path) -> Option<&'static str> {
    match path
        .extension()?
        .to_string_lossy()
        .to_ascii_lowercase()
        .as_str()
    {
        "ico" => Some("image/x-icon"),
        "svg" => Some("image/svg+xml"),
        "png" => Some("image/png"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        _ => None,
    }
}

pub fn canonical_directory_in_access_scope(
    base_dir: &Path,
    candidate: &Path,
) -> ApiResult<PathBuf> {
    let canonical = canonicalize_in_access_scope(base_dir, candidate)?;
    if !canonical.is_dir() {
        return Err(AppError::bad_request("目标不是目录。"));
    }
    Ok(canonical)
}

fn resolve_path(base_dir: &Path, requested: &str) -> ApiResult<PathBuf> {
    let relative = normalize_relative_path(requested)?;
    let candidate = base_dir.join(relative);
    canonicalize_in_access_scope(base_dir, &candidate)
}

fn canonicalize_in_access_scope(base_dir: &Path, candidate: &Path) -> ApiResult<PathBuf> {
    let canonical = candidate
        .canonicalize()
        .map_err(|error| AppError::not_found(format!("路径不存在: {error}")))?;

    if !is_within_access_scope(base_dir, &canonical) {
        return Err(AppError::bad_request("只允许访问当前工作目录及其上一层目录。"));
    }

    Ok(canonical)
}

fn resolve_terminal_path(base_dir: &Path, requested: &str) -> ApiResult<PathBuf> {
    let trimmed = requested.trim();
    let candidate = if trimmed.is_empty() || trimmed == "/" {
        base_dir.to_path_buf()
    } else if Path::new(trimmed).is_absolute() {
        PathBuf::from(trimmed)
    } else {
        base_dir.join(normalize_relative_path(trimmed)?)
    };
    let lexical_candidate = normalize_path_lexically(&candidate);

    if !is_within_access_scope(base_dir, &lexical_candidate) {
        return Err(AppError::bad_request("只允许访问当前工作目录及其上一层目录。"));
    }

    Ok(lexical_candidate)
}

fn resolve_rename_path(base_dir: &Path, requested: &str) -> ApiResult<PathBuf> {
    let requested = requested.trim();
    if requested.is_empty() || requested == "/" {
        return Err(AppError::bad_request("当前路径不能重命名。"));
    }

    let relative = normalize_relative_path(requested)?;
    if relative.components().next().is_none() || relative == Path::new("..") {
        return Err(AppError::bad_request("当前路径不能重命名。"));
    }

    let candidate = base_dir.join(relative);
    let parent = candidate
        .parent()
        .ok_or_else(|| AppError::bad_request("当前路径不能重命名。"))?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|error| AppError::not_found(format!("父目录不存在: {error}")))?;

    if !is_within_access_scope(base_dir, &canonical_parent) {
        return Err(AppError::bad_request("只允许访问当前工作目录及其上一层目录。"));
    }

    let file_name = candidate
        .file_name()
        .ok_or_else(|| AppError::bad_request("当前路径不能重命名。"))?;
    Ok(canonical_parent.join(file_name))
}

fn normalize_relative_path(requested: &str) -> ApiResult<PathBuf> {
    let trimmed = requested.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return Ok(PathBuf::new());
    }

    let mut normalized = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            Component::ParentDir => normalized.push(".."),
            Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::bad_request("路径必须使用相对路径。"));
            }
        }
    }

    Ok(normalized)
}

fn normalize_path_lexically(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(std::path::MAIN_SEPARATOR_STR),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(part) => normalized.push(part),
        }
    }
    normalized
}

fn validate_entry_name(name: &str) -> ApiResult<&str> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::bad_request("新名称不能为空。"));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(AppError::bad_request("新名称不能是 . 或 ..。"));
    }
    if trimmed.contains('\0') {
        return Err(AppError::bad_request("新名称包含非法字符。"));
    }
    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(AppError::bad_request("新名称不能包含路径分隔符。"));
    }

    let mut components = Path::new(trimmed).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(trimmed),
        _ => Err(AppError::bad_request("新名称不能包含路径分隔符。")),
    }
}

pub fn relative_path(base_dir: &Path, path: &Path) -> ApiResult<PathBuf> {
    diff_paths(base_dir, path).ok_or_else(|| AppError::internal("无法计算相对路径。"))
}

fn parent_relative_path(path: &Path) -> Option<String> {
    let current = path_to_string(path);
    if current.is_empty() {
        return Some("..".to_string());
    }
    if current == ".." {
        return None;
    }

    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    Some(path_to_string(parent))
}

pub fn display_path(base_dir: &Path, display_root: &Path, path: &Path) -> String {
    remap_display_path(base_dir, display_root, path)
        .unwrap_or_else(|| path.to_path_buf())
        .display()
        .to_string()
}

fn remap_display_path(base_dir: &Path, display_root: &Path, path: &Path) -> Option<PathBuf> {
    let relative = diff_paths(base_dir, path)?;
    let mut remapped = display_root.to_path_buf();

    for component in relative.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                remapped.pop();
            }
            Component::Normal(part) => remapped.push(part),
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    Some(remapped)
}

fn is_dot_prefixed(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy().starts_with('.')
}

pub fn is_within_access_scope(base_dir: &Path, candidate: &Path) -> bool {
    candidate.starts_with(access_root(base_dir))
}

fn access_root(base_dir: &Path) -> &Path {
    base_dir.parent().unwrap_or(base_dir)
}

fn path_to_string(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy()),
            Component::ParentDir => Some("..".into()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join(std::path::MAIN_SEPARATOR_STR)
}

fn diff_paths(base: &Path, target: &Path) -> Option<PathBuf> {
    let base_components: Vec<_> = base.components().collect();
    let target_components: Vec<_> = target.components().collect();

    let common = base_components
        .iter()
        .zip(target_components.iter())
        .take_while(|(left, right)| left == right)
        .count();

    if common == 0 && (base.is_absolute() || target.is_absolute()) {
        return None;
    }

    let mut result = PathBuf::new();

    for component in &base_components[common..] {
        if matches!(component, Component::Normal(_)) {
            result.push("..");
        }
    }

    for component in &target_components[common..] {
        if let Component::Normal(part) = component {
            result.push(part);
        }
    }

    Some(result)
}

fn compare_entries(left: &DirectoryEntry, right: &DirectoryEntry) -> Ordering {
    match (left.kind == "dir", right.kind == "dir") {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        _ => left
            .name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name)),
    }
}

fn should_merge_existing_sections(path: &Path) -> bool {
    matches!(
        (
            path.file_name().and_then(|name| name.to_str()),
            path.parent()
                .and_then(|parent| parent.file_name())
                .and_then(|name| name.to_str()),
        ),
        (Some("config.toml"), Some(".codex"))
    )
}

fn merge_toml_front_matter(new_content: &str, existing_content: &str) -> String {
    let next_prefix = split_toml_sections(new_content).0;
    let existing_sections = split_toml_sections(existing_content).1;
    if existing_sections.is_empty() {
        return new_content.to_string();
    }
    join_toml_segments(next_prefix, existing_sections)
}

fn split_toml_sections(content: &str) -> (&str, &str) {
    let mut offset = 0usize;

    for segment in content.split_inclusive('\n') {
        if is_toml_section_header(segment) {
            return content.split_at(offset);
        }
        offset += segment.len();
    }

    if !content.is_empty() && is_toml_section_header(content) {
        return ("", content);
    }

    (content, "")
}

fn is_toml_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') {
        return false;
    }

    let without_comment = trimmed
        .split_once('#')
        .map(|(head, _)| head.trim_end())
        .unwrap_or(trimmed);

    without_comment.ends_with(']')
}

fn join_toml_segments(prefix: &str, sections: &str) -> String {
    let prefix = prefix.trim_end_matches('\n');
    let sections = sections.trim_matches('\n');

    match (prefix.is_empty(), sections.is_empty()) {
        (true, true) => String::new(),
        (false, true) => format!("{prefix}\n"),
        (true, false) => format!("{sections}\n"),
        (false, false) => format!("{prefix}\n\n{sections}\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        canonical_directory_in_access_scope, diff_paths, display_path, is_within_access_scope,
        merge_toml_front_matter, normalize_relative_path, parent_relative_path, path_to_string,
        resolve_file_path, resolve_terminal_directory_path, resolve_workspace_icon_path,
        should_merge_existing_sections, validate_entry_name,
    };
    use std::{
        fs,
        path::{Path, PathBuf},
    };

    #[test]
    fn normalize_root_path() {
        assert_eq!(normalize_relative_path("").unwrap(), Path::new(""));
        assert_eq!(normalize_relative_path("/").unwrap(), Path::new(""));
    }

    #[test]
    fn allow_parent_segments() {
        assert_eq!(normalize_relative_path("../secret").unwrap(), PathBuf::from("../secret"));
        assert_eq!(
            normalize_relative_path("src/../../main.rs").unwrap(),
            PathBuf::from("src/../../main.rs")
        );
    }

    #[test]
    fn reject_absolute_paths() {
        assert!(normalize_relative_path("/etc/passwd").is_err());
    }

    #[test]
    fn terminal_directory_path_accepts_absolute_path_inside_access_scope() {
        let unique = format!(
            "webclx-terminal-path-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_root = std::env::temp_dir().join(unique);
        let workspace_parent = temp_root.join("home");
        let workspace = workspace_parent.join("codes");
        let sibling = workspace_parent.join("third_party").join("ZCode");
        let outside = temp_root.join("outside");

        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        fs::create_dir_all(&outside).unwrap();

        assert_eq!(
            resolve_terminal_directory_path(&workspace, sibling.to_str().unwrap()).unwrap(),
            sibling
        );
        assert!(resolve_terminal_directory_path(&workspace, outside.to_str().unwrap()).is_err());

        fs::remove_dir_all(temp_root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn terminal_directory_path_keeps_symlink_path_inside_access_scope() {
        use std::os::unix::fs::symlink;

        let unique = format!(
            "webclx-terminal-symlink-path-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_root = std::env::temp_dir().join(unique);
        let workspace_parent = temp_root.join("home");
        let workspace = workspace_parent.join("codes");
        let link_parent = workspace_parent.join("third_party");
        let link_path = link_parent.join("ZCode");
        let real_target = temp_root.join("opt").join("ZCode");

        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&link_parent).unwrap();
        fs::create_dir_all(&real_target).unwrap();
        symlink(&real_target, &link_path).unwrap();

        assert_eq!(
            resolve_terminal_directory_path(&workspace, link_path.to_str().unwrap()).unwrap(),
            link_path
        );

        fs::remove_dir_all(temp_root).unwrap();
    }

    #[test]
    fn validate_rename_entry_name_rejects_path_segments() {
        assert_eq!(validate_entry_name("next-name").unwrap(), "next-name");
        assert!(validate_entry_name("").is_err());
        assert!(validate_entry_name(".").is_err());
        assert!(validate_entry_name("..").is_err());
        assert!(validate_entry_name("child/name").is_err());
        assert!(validate_entry_name("../name").is_err());
        assert!(validate_entry_name("name\0suffix").is_err());
    }

    #[test]
    fn display_root_and_nested_paths() {
        assert_eq!(
            display_path(
                Path::new("/tmp/workspace"),
                Path::new("/tmp/workspace"),
                Path::new("/tmp/workspace")
            ),
            "/tmp/workspace"
        );
        assert_eq!(
            display_path(
                Path::new("/tmp/workspace"),
                Path::new("/tmp/workspace"),
                Path::new("/tmp/workspace/demo.txt")
            ),
            "/tmp/workspace/demo.txt"
        );
        assert_eq!(
            display_path(
                Path::new("/home/workspaces-src"),
                Path::new("/home/codes"),
                Path::new("/home/workspaces-src/webClx")
            ),
            "/home/codes/webClx"
        );
        assert_eq!(
            display_path(
                Path::new("/home/workspaces-src"),
                Path::new("/home/codes"),
                Path::new("/home")
            ),
            "/home"
        );
        assert_eq!(path_to_string(Path::new("src/main.rs")), "src/main.rs");
        assert_eq!(path_to_string(Path::new("../main.rs")), "../main.rs");
    }

    #[test]
    fn diff_paths_can_point_to_parent() {
        let base = Path::new("/home/workspaces-src");
        let target = Path::new("/home");
        assert_eq!(diff_paths(base, target).unwrap(), PathBuf::from(".."));
        assert_eq!(
            diff_paths(base, Path::new("/home/Documents")).unwrap(),
            PathBuf::from("../Documents")
        );
    }

    #[test]
    fn allow_only_one_level_up_from_workspace_root() {
        let base = Path::new("/home/workspaces-src");
        assert!(is_within_access_scope(base, Path::new("/home/workspaces-src")));
        assert!(is_within_access_scope(base, Path::new("/home")));
        assert!(is_within_access_scope(base, Path::new("/home/webClx/src")));
        assert!(!is_within_access_scope(base, Path::new("/")));
        assert!(!is_within_access_scope(base, Path::new("/root")));
    }

    #[cfg(unix)]
    #[test]
    fn resolve_file_path_rejects_symlink_escape_outside_access_root() {
        use std::os::unix::fs::symlink;

        let unique = format!(
            "webclx-pathguard-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_root = std::env::temp_dir().join(unique);
        let workspace_parent = temp_root.join("parent");
        let workspace = workspace_parent.join("workspace");
        let outside = temp_root.join("outside");

        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.txt"), "secret").unwrap();
        symlink(outside.join("secret.txt"), workspace.join("link.txt")).unwrap();

        let result = resolve_file_path(&workspace, "link.txt");
        fs::remove_dir_all(&temp_root).unwrap();

        assert!(result.is_err());
    }

    #[test]
    fn workspace_icon_resolution_supports_exact_and_nearest_project_lookup() {
        let unique = format!(
            "webclx-workspace-icon-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_root = std::env::temp_dir().join(unique);
        let workspace = temp_root.join("codes");
        let project = workspace.join("demo");
        let nested = project.join("src").join("feature");
        fs::create_dir_all(project.join("static")).unwrap();
        fs::create_dir_all(&nested).unwrap();
        fs::write(project.join("icon.ico"), b"ico").unwrap();
        fs::write(project.join("static").join("favicon.svg"), b"<svg/>").unwrap();

        assert_eq!(
            resolve_workspace_icon_path(&workspace, "demo", "icon.ico", false).unwrap(),
            project.join("icon.ico")
        );
        assert_eq!(
            resolve_workspace_icon_path(
                &workspace,
                "demo/src/feature",
                "static/favicon.svg",
                true,
            )
            .unwrap(),
            project.join("static").join("favicon.svg")
        );
        assert!(resolve_workspace_icon_path(&workspace, "demo", "../secret.ico", false).is_err());
        assert!(resolve_workspace_icon_path(&workspace, "demo", "README.md", false).is_err());

        fs::remove_dir_all(temp_root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn workspace_icon_resolution_rejects_symlink_escape_from_project() {
        use std::os::unix::fs::symlink;

        let unique = format!(
            "webclx-workspace-icon-symlink-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_root = std::env::temp_dir().join(unique);
        let workspace = temp_root.join("codes");
        let project = workspace.join("demo");
        let outside = temp_root.join("outside");
        fs::create_dir_all(&project).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("secret.ico"), b"secret").unwrap();
        symlink(outside.join("secret.ico"), project.join("icon.ico")).unwrap();

        assert!(resolve_workspace_icon_path(&workspace, "demo", "icon.ico", false).is_err());

        fs::remove_dir_all(temp_root).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn canonical_directory_rejects_symlink_escape_outside_access_root() {
        use std::os::unix::fs::symlink;

        let unique = format!(
            "webclx-directory-pathguard-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_root = std::env::temp_dir().join(unique);
        let workspace_parent = temp_root.join("parent");
        let workspace = workspace_parent.join("workspace");
        let outside = temp_root.join("outside");
        let link = workspace.join("outside-link");

        fs::create_dir_all(&workspace).unwrap();
        fs::create_dir_all(&outside).unwrap();
        symlink(&outside, &link).unwrap();

        let result = canonical_directory_in_access_scope(&workspace, &link);
        fs::remove_dir_all(&temp_root).unwrap();

        assert!(result.is_err());
    }

    #[test]
    fn parent_relative_path_matches_one_level_up_rule() {
        assert_eq!(parent_relative_path(Path::new("")), Some("..".to_string()));
        assert_eq!(parent_relative_path(Path::new("src")), Some(String::new()));
        assert_eq!(parent_relative_path(Path::new("src/bin")), Some("src".to_string()));
        assert_eq!(parent_relative_path(Path::new("..")), None);
        assert_eq!(parent_relative_path(Path::new("../documents")), Some("..".to_string()));
    }

    #[test]
    fn codex_config_toml_uses_merge_save_mode() {
        assert!(should_merge_existing_sections(Path::new("/home/example/.codex/config.toml")));
        assert!(!should_merge_existing_sections(Path::new("/home/example/project/config.toml")));
    }

    #[test]
    fn merge_toml_keeps_existing_sections() {
        let existing = r#"model = "gpt-5.4"
approval_policy = "never"

[notice]
hide_full_access_warning = true

[projects."\\?\D:\UserData\Documents\codes"]
trust_level = "trusted"
"#;
        let updated = r#"model = "gpt-5.4-mini"
approval_policy = "never"
sandbox_mode = "danger-full-access"
"#;

        assert_eq!(
            merge_toml_front_matter(updated, existing),
            r#"model = "gpt-5.4-mini"
approval_policy = "never"
sandbox_mode = "danger-full-access"

[notice]
hide_full_access_warning = true

[projects."\\?\D:\UserData\Documents\codes"]
trust_level = "trusted"
"#
        );
    }

    #[test]
    fn merge_toml_ignores_sections_from_new_content() {
        let existing = r#"model = "gpt-5.4"

[notice]
hide_full_access_warning = true
"#;
        let updated = r#"model = "gpt-5.4-mini"

[notice]
hide_full_access_warning = false
"#;

        assert_eq!(
            merge_toml_front_matter(updated, existing),
            r#"model = "gpt-5.4-mini"

[notice]
hide_full_access_warning = true
"#
        );
    }

    #[test]
    fn merge_toml_falls_back_to_normal_save_without_existing_sections() {
        let existing = r#"model = "gpt-5.4"
approval_policy = "never"
"#;
        let updated = r#"model = "gpt-5.4-mini"

[notice]
hide_full_access_warning = false
"#;

        assert_eq!(merge_toml_front_matter(updated, existing), updated);
    }
}
