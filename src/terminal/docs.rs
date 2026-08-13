use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use axum::{
    Json,
    extract::{Path as AxumPath, Query, State},
};
use tracing::warn;

use super::{
    TerminalAgentsDocItem, TerminalAgentsDocListResponse, TerminalAgentsDocPathQuery,
    TerminalAgentsDocResponse, TerminalAgentsDocSaveRequest,
};
use crate::{ApiResult, AppError, AppState, filesystem};

#[derive(Debug, Default, serde::Deserialize)]
pub(crate) struct TerminalAgentsDocListQuery {
    #[serde(default)]
    show_hidden: bool,
    #[serde(default)]
    recursive_dirs: String,
}

pub async fn read_session_agents_doc(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Query(query): Query<TerminalAgentsDocPathQuery>,
) -> ApiResult<Json<TerminalAgentsDocResponse>> {
    let session_dir = session_doc_directory(&state, &session_id)?;
    let documents = list_terminal_doc_candidates(
        &state,
        &session_dir,
        query.show_hidden,
        &query.recursive_dirs,
    )?;
    let doc_path = session_agents_doc_path(&state, &session_dir, &query.path)?;
    let exists = match tokio::fs::metadata(&doc_path).await {
        Ok(metadata) => {
            if !metadata.is_file() {
                return Err(AppError::bad_request("文档不是普通文件。"));
            }
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(AppError::internal(format!("读取文档信息失败: {error}"))),
    };

    let content = if exists {
        tokio::fs::read_to_string(&doc_path)
            .await
            .map_err(|error| AppError::internal(format!("读取文档失败: {error}")))?
    } else {
        String::new()
    };

    Ok(Json(terminal_agents_doc_response(
        &state,
        &session_dir,
        &doc_path,
        exists,
        content,
        documents,
    )?))
}

pub async fn list_session_agents_docs(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Query(query): Query<TerminalAgentsDocListQuery>,
) -> ApiResult<Json<TerminalAgentsDocListResponse>> {
    let session_dir = session_doc_directory(&state, &session_id)?;
    Ok(Json(TerminalAgentsDocListResponse {
        documents: list_terminal_doc_candidates(
            &state,
            &session_dir,
            query.show_hidden,
            &query.recursive_dirs,
        )?,
    }))
}

pub async fn save_session_agents_doc(
    State(state): State<AppState>,
    AxumPath(session_id): AxumPath<String>,
    Json(payload): Json<TerminalAgentsDocSaveRequest>,
) -> ApiResult<Json<TerminalAgentsDocResponse>> {
    let session_dir = session_doc_directory(&state, &session_id)?;
    let doc_path = session_agents_doc_path(&state, &session_dir, &payload.path)?;
    if let Some(parent) = doc_path.parent() {
        let metadata = tokio::fs::metadata(parent)
            .await
            .map_err(|error| AppError::not_found(format!("终端工作目录不存在: {error}")))?;
        if !metadata.is_dir() {
            return Err(AppError::bad_request("终端工作目录不是目录。"));
        }
    }
    if let Ok(metadata) = tokio::fs::metadata(&doc_path).await
        && !metadata.is_file()
    {
        return Err(AppError::bad_request("文档不是普通文件。"));
    }

    tokio::fs::write(&doc_path, payload.content.as_bytes())
        .await
        .map_err(|error| AppError::internal(format!("保存文档失败: {error}")))?;
    let documents = list_terminal_doc_candidates(
        &state,
        &session_dir,
        payload.show_hidden,
        &payload.recursive_dirs,
    )?;

    Ok(Json(terminal_agents_doc_response(
        &state,
        &session_dir,
        &doc_path,
        true,
        payload.content,
        documents,
    )?))
}

fn session_doc_directory(state: &AppState, session_id: &str) -> ApiResult<PathBuf> {
    let base_dir = state.workspace_root();
    let session_path = state
        .terminal_manager
        .session_path(session_id)
        .ok_or_else(|| AppError::not_found("终端会话不存在。"))?;
    filesystem::canonical_directory_in_access_scope(&base_dir, &session_path)
}

fn session_agents_doc_path(
    state: &AppState,
    session_dir: &Path,
    requested_path: &str,
) -> ApiResult<PathBuf> {
    let relative = normalize_terminal_doc_path(requested_path)?;
    let doc_path = session_dir.join(relative);
    let parent = doc_path
        .parent()
        .ok_or_else(|| AppError::bad_request("文档路径无效。"))?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|error| AppError::not_found(format!("文档目录不存在: {error}")))?;
    if !canonical_parent.starts_with(session_dir) {
        return Err(AppError::bad_request("文档路径必须位于当前目录内。"));
    }
    if !filesystem::is_within_access_scope(&state.workspace_root(), &canonical_parent) {
        return Err(AppError::bad_request("只允许访问当前工作目录及其上一层目录。"));
    }
    if let Ok(metadata) = std::fs::metadata(&doc_path) {
        if !metadata.is_file() {
            return Err(AppError::bad_request("文档不是普通文件。"));
        }
        let canonical_doc_path = doc_path
            .canonicalize()
            .map_err(|error| AppError::not_found(format!("文档不存在: {error}")))?;
        if !is_allowed_terminal_doc_path(session_dir, &canonical_doc_path) {
            return Err(AppError::bad_request("只允许编辑当前目录内的文档。"));
        }
        if !is_terminal_doc_file(&doc_path) {
            return Err(AppError::bad_request("只能编辑文本类文档。"));
        }
    } else if !is_terminal_doc_file(&doc_path) {
        // 路径尚未落盘：仍按扩展名白名单校验，避免被用于创建任意扩展名的文件。
        // 真正的路径越权由 normalize_terminal_doc_path 与上方父目录 canonicalize
        // 共同把关。AGENTS.MD（大小写不敏感）由 is_terminal_doc_file 允许。
        return Err(AppError::bad_request("只能编辑文本类文档。"));
    }
    Ok(doc_path)
}

fn terminal_agents_doc_response(
    state: &AppState,
    session_dir: &Path,
    doc_path: &Path,
    exists: bool,
    content: String,
    documents: Vec<TerminalAgentsDocItem>,
) -> ApiResult<TerminalAgentsDocResponse> {
    let display_root = state.workspace_display_root();
    let base_dir = state.workspace_root();
    Ok(TerminalAgentsDocResponse {
        path: normalize_doc_path_text(doc_path, session_dir),
        display_path: filesystem::display_path(&base_dir, &display_root, doc_path),
        exists,
        content,
        documents,
    })
}

/// AGENTS.MD 和 README.MD（大小写忽略）排在最前。
fn is_pinned_doc_name(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    name == "agents.md" || name == "readme.md"
}

/// 固定文档内部排序：AGENTS.MD 在 README.MD 之前。
fn pin_rank(path: &str) -> u8 {
    let name = path.rsplit('/').next().unwrap_or(path).to_ascii_lowercase();
    if name == "agents.md" { 0 } else { 1 }
}

fn list_terminal_doc_candidates(
    state: &AppState,
    session_dir: &Path,
    show_hidden: bool,
    recursive_dirs: &str,
) -> ApiResult<Vec<TerminalAgentsDocItem>> {
    let mut documents = Vec::new();
    let recursive_directories = parse_recursive_doc_directories(recursive_dirs);
    push_terminal_doc_item(state, session_dir, &session_dir.join("AGENTS.MD"), &mut documents)?;
    collect_terminal_doc_entries(
        state,
        session_dir,
        session_dir,
        false,
        show_hidden,
        &recursive_directories,
        &mut documents,
    )?;
    // AGENTS.MD 和 README.MD（大小写忽略）固定在最前，其余按修改时间倒序。
    documents.sort_by(|left, right| {
        let left_pinned = is_pinned_doc_name(&left.path);
        let right_pinned = is_pinned_doc_name(&right.path);
        match (left_pinned, right_pinned) {
            (true, true) => pin_rank(&left.path).cmp(&pin_rank(&right.path)),
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (false, false) => right
                .modified
                .cmp(&left.modified)
                .then_with(|| left.path.cmp(&right.path)),
        }
    });
    documents.dedup_by(|left, right| left.path == right.path);
    Ok(documents)
}

fn parse_recursive_doc_directories(raw: &str) -> HashSet<String> {
    let mut directories = raw
        .split(|character: char| matches!(character, ',' | ';' | '\n' | '\r'))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(|name| name.to_lowercase())
        .collect::<HashSet<_>>();
    if directories.is_empty() {
        directories.insert("docs".to_string());
    }
    directories
}

// 遍历时跳过的非内容目录，避免把依赖树/构建产物/工具链里的 .json/.toml 全列出来。
const IGNORED_DOC_DIRECTORIES: &[&str] = &[
    ".android-toolchain",
    ".git",
    ".idea",
    ".next",
    ".venv",
    ".webclx-paste",
    "__pycache__",
    "build",
    "dist",
    "node_modules",
    "target",
    "venv",
];

fn collect_terminal_doc_entries(
    state: &AppState,
    session_dir: &Path,
    directory: &Path,
    recursive: bool,
    show_hidden: bool,
    recursive_directories: &HashSet<String>,
    documents: &mut Vec<TerminalAgentsDocItem>,
) -> ApiResult<()> {
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            warn!("read terminal document directory {} failed: {error}", directory.display());
            return Ok(());
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                warn!("read terminal document entry {} failed: {error}", path.display());
                continue;
            }
        };
        // 跳过符号链接，避免循环与意外越权。
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };
            if should_skip_terminal_doc_directory(name, show_hidden) {
                continue;
            }
            if !should_recurse_terminal_doc_directory(name, recursive, recursive_directories) {
                continue;
            }
            // 防越权：把子目录限定在 session_dir 内（canonicalize 失败也跳过）。
            match path.canonicalize() {
                Ok(canonical) if canonical.starts_with(session_dir) => {
                    collect_terminal_doc_entries(
                        state,
                        session_dir,
                        &path,
                        true,
                        show_hidden,
                        recursive_directories,
                        documents,
                    )?;
                }
                Ok(_) | Err(_) => continue,
            }
            continue;
        }
        if file_type.is_file() && is_terminal_doc_file(&path) {
            push_terminal_doc_item(state, session_dir, &path, documents)?;
        }
    }
    Ok(())
}

fn push_terminal_doc_item(
    state: &AppState,
    session_dir: &Path,
    path: &Path,
    documents: &mut Vec<TerminalAgentsDocItem>,
) -> ApiResult<()> {
    let relative_to_session = normalize_doc_path_text(path, session_dir);
    if relative_to_session.is_empty() {
        return Ok(());
    }
    let exists = path.is_file();
    let modified = std::fs::metadata(path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs() as i64);
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    documents.push(TerminalAgentsDocItem {
        path: relative_to_session.clone(),
        display_path: filesystem::display_path(&base_dir, &display_root, path),
        label: relative_to_session,
        exists,
        modified,
    });
    Ok(())
}

fn normalize_terminal_doc_path(requested_path: &str) -> ApiResult<PathBuf> {
    let trimmed = requested_path.trim();
    let relative = if trimmed.is_empty() || trimmed == "AGENTS.MD" {
        PathBuf::from("AGENTS.MD")
    } else {
        PathBuf::from(trimmed)
    };
    let mut normalized = PathBuf::new();
    for component in relative.components() {
        match component {
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(AppError::bad_request("文档路径必须位于当前目录内。"));
            }
        }
    }
    if normalized.components().next().is_none() {
        normalized.push("AGENTS.MD");
    }
    Ok(normalized)
}

fn should_skip_terminal_doc_directory(name: &str, show_hidden: bool) -> bool {
    (!show_hidden && name.starts_with('.'))
        || IGNORED_DOC_DIRECTORIES
            .iter()
            .any(|ignored| *ignored == name)
}

fn should_recurse_terminal_doc_directory(
    name: &str,
    recursive: bool,
    recursive_directories: &HashSet<String>,
) -> bool {
    recursive || recursive_directories.contains(&name.to_lowercase())
}

fn is_allowed_terminal_doc_path(session_dir: &Path, path: &Path) -> bool {
    path.parent()
        .is_some_and(|parent| parent.starts_with(session_dir))
}

fn normalize_doc_path_text(path: &Path, session_dir: &Path) -> String {
    path.strip_prefix(session_dir)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_terminal_doc_file(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if file_name == "agents.md" {
        return true;
    }
    matches!(
        path.extension()
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .as_deref(),
        Some("md" | "markdown" | "txt" | "toml" | "json" | "yaml" | "yml")
    )
}

#[cfg(test)]
mod tests {
    use super::{
        parse_recursive_doc_directories, should_recurse_terminal_doc_directory,
        should_skip_terminal_doc_directory,
    };

    #[test]
    fn terminal_doc_recursion_defaults_to_docs_and_ignores_name_case() {
        let defaults = parse_recursive_doc_directories("");
        assert_eq!(defaults.len(), 1);
        assert!(should_recurse_terminal_doc_directory("DOCS", false, &defaults));
        assert!(!should_recurse_terminal_doc_directory("src", false, &defaults));

        let configured = parse_recursive_doc_directories(" Docs, PRD;notes\nNOTES ");
        assert_eq!(configured.len(), 3);
        assert!(should_recurse_terminal_doc_directory("docs", false, &configured));
        assert!(should_recurse_terminal_doc_directory("Prd", false, &configured));
        assert!(should_recurse_terminal_doc_directory("anything", true, &configured));
    }

    #[test]
    fn terminal_doc_directories_only_include_hidden_entries_when_requested() {
        assert!(!should_skip_terminal_doc_directory("docs", false));
        assert!(should_skip_terminal_doc_directory(".codex", false));
        assert!(!should_skip_terminal_doc_directory(".codex", true));
        assert!(should_skip_terminal_doc_directory(".git", true));
    }
}
