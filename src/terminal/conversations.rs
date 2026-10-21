use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime};

use anyhow::{Context, Result};
use rusqlite::{
    Connection, OpenFlags, OptionalExtension, Transaction, TransactionBehavior, params,
};
use serde_json::Value;
use terminal_core::{codex_session_id_from_path, normalize_resume_id, system_time_to_millis};
use tracing::warn;

use super::{CodexConversationInfo, DeleteCodexConversationResponse};

const MAX_CODEX_CONVERSATION_SCAN_LINES: usize = 512;
const MAX_CODEX_CONVERSATION_TITLE_MESSAGES: usize = 8;
const MAX_CODEX_CONVERSATION_TITLE_CHARS: usize = 2048;

static CODEX_METADATA_CACHE: std::sync::OnceLock<Mutex<Option<CodexMetadataCache>>> =
    std::sync::OnceLock::new();

#[cfg(test)]
pub(super) fn scan_codex_conversations(codex_home: &Path) -> Result<Vec<CodexConversationInfo>> {
    scan_codex_conversations_for_cwd(codex_home, None)
}

pub(super) fn scan_codex_conversations_for_cwd(
    codex_home: &Path,
    cwd_filter: Option<&Path>,
) -> Result<Vec<CodexConversationInfo>> {
    scan_codex_conversations_for_cwd_since(codex_home, cwd_filter, None)
}

pub(super) fn scan_codex_conversations_for_cwd_since(
    codex_home: &Path,
    cwd_filter: Option<&Path>,
    since_ms: Option<u64>,
) -> Result<Vec<CodexConversationInfo>> {
    let started_at = Instant::now();
    if let Some(conversations) =
        codex_conversations_from_state_database(codex_home, cwd_filter, since_ms)?
    {
        return Ok(conversations);
    }
    let sessions_dir = codex_home.join("sessions");
    let mut conversations = Vec::new();
    let metadata_index = codex_conversation_metadata_index(codex_home);
    let metadata_loaded_at = Instant::now();
    collect_codex_conversations(
        &sessions_dir,
        &metadata_index,
        cwd_filter,
        since_ms,
        &mut conversations,
    )?;
    let rollouts_scanned_at = Instant::now();
    conversations.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.size_bytes.cmp(&left.size_bytes))
            .then_with(|| left.cwd.cmp(&right.cwd))
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
    let completed_at = Instant::now();
    if completed_at.duration_since(started_at) >= std::time::Duration::from_millis(250) {
        warn!(
            codex_home = %codex_home.display(),
            cwd_filter = cwd_filter.map(|path| path.display().to_string()).unwrap_or_default(),
            metadata_ms = metadata_loaded_at.duration_since(started_at).as_millis(),
            rollouts_ms = rollouts_scanned_at.duration_since(metadata_loaded_at).as_millis(),
            sort_ms = completed_at.duration_since(rollouts_scanned_at).as_millis(),
            total_ms = completed_at.duration_since(started_at).as_millis(),
            conversation_count = conversations.len(),
            "slow Codex conversation scan"
        );
    }
    Ok(conversations)
}

pub(super) fn delete_codex_conversation_files(
    codex_home: &Path,
    raw_session_id: &str,
) -> Result<DeleteCodexConversationResponse> {
    let session_id = validated_codex_session_id(raw_session_id)?;

    let rollout_paths = codex_rollout_paths_for_session(&codex_home.join("sessions"), &session_id)?;
    let session_index_entries_deleted =
        rewrite_jsonl_without_session(&codex_home.join("session_index.jsonl"), "id", &session_id)?;
    let history_entries_deleted = rewrite_jsonl_without_session(
        &codex_home.join("history.jsonl"),
        "session_id",
        &session_id,
    )?;
    let database_threads_deleted =
        delete_codex_state_thread(&codex_home.join("state_5.sqlite"), &session_id)?;

    let mut rollout_files_deleted = 0;
    for path in rollout_paths {
        match fs::remove_file(&path) {
            Ok(()) => rollout_files_deleted += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("删除 Codex rollout 失败: {}", path.display()));
            }
        }
    }

    Ok(DeleteCodexConversationResponse {
        session_id,
        rollout_files_deleted,
        session_index_entries_deleted,
        history_entries_deleted,
        database_threads_deleted,
        resume_archive_deleted: false,
    })
}

pub(super) fn validated_codex_session_id(raw_session_id: &str) -> Result<String> {
    let session_id = normalize_resume_id(raw_session_id)
        .map_err(|error| anyhow::anyhow!("Codex session ID 无效: {error}"))?;
    if !looks_like_codex_uuid(&session_id) {
        anyhow::bail!("Codex session ID 无效。");
    }
    Ok(session_id)
}

fn looks_like_codex_uuid(session_id: &str) -> bool {
    let bytes = session_id.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit())
}

fn codex_rollout_paths_for_session(directory: &Path, session_id: &str) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    collect_codex_rollout_paths_for_session(directory, session_id, &mut paths)?;
    Ok(paths)
}

fn collect_codex_rollout_paths_for_session(
    directory: &Path,
    session_id: &str,
    paths: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(error)
                .with_context(|| format!("读取 Codex sessions 目录失败: {}", directory.display()));
        }
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_codex_rollout_paths_for_session(&path, session_id, paths)?;
        } else if file_type.is_file()
            && codex_session_id_from_path(&path).as_deref() == Some(session_id)
        {
            paths.push(path);
        }
    }
    Ok(())
}

fn rewrite_jsonl_without_session(path: &Path, id_field: &str, session_id: &str) -> Result<usize> {
    for _ in 0..3 {
        let content = match fs::read(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("读取 Codex 元数据失败: {}", path.display()));
            }
        };
        let (replacement, removed) = filtered_jsonl_content(&content, id_field, session_id);
        if removed == 0 {
            return Ok(0);
        }
        if fs::read(path)? != content {
            continue;
        }
        replace_file_contents(path, &replacement)?;
        return Ok(removed);
    }
    anyhow::bail!("Codex 正在写入 {}，请稍后重试。", path.display())
}

fn filtered_jsonl_content(content: &[u8], id_field: &str, session_id: &str) -> (Vec<u8>, usize) {
    let mut replacement = Vec::with_capacity(content.len());
    let mut removed = 0;
    for line in content.split(|byte| *byte == b'\n') {
        if line.is_empty() {
            continue;
        }
        let should_remove = serde_json::from_slice::<Value>(line)
            .ok()
            .and_then(|value| {
                value
                    .get(id_field)
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            })
            .is_some_and(|id| id == session_id);
        if should_remove {
            removed += 1;
        } else {
            replacement.extend_from_slice(line);
            replacement.push(b'\n');
        }
    }
    (replacement, removed)
}

fn replace_file_contents(path: &Path, content: &[u8]) -> Result<()> {
    crate::atomic_file::write_atomic(path, content)
        .with_context(|| format!("替换 Codex 元数据失败: {}", path.display()))
}

fn delete_codex_state_thread(database_path: &Path, session_id: &str) -> Result<usize> {
    if !database_path.exists() {
        return Ok(0);
    }
    let mut connection = Connection::open(database_path)
        .with_context(|| format!("打开 Codex 状态数据库失败: {}", database_path.display()))?;
    connection.busy_timeout(std::time::Duration::from_secs(3))?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    delete_from_table_if_present(
        &transaction,
        "thread_dynamic_tools",
        "DELETE FROM thread_dynamic_tools WHERE thread_id = ?1",
        session_id,
    )?;
    delete_from_table_if_present(
        &transaction,
        "thread_spawn_edges",
        "DELETE FROM thread_spawn_edges WHERE parent_thread_id = ?1 OR child_thread_id = ?1",
        session_id,
    )?;
    if sqlite_table_exists(&transaction, "agent_job_items")? {
        transaction.execute(
            "UPDATE agent_job_items SET assigned_thread_id = NULL WHERE assigned_thread_id = ?1",
            params![session_id],
        )?;
    }
    let deleted = delete_from_table_if_present(
        &transaction,
        "threads",
        "DELETE FROM threads WHERE id = ?1",
        session_id,
    )?;
    transaction.commit()?;
    Ok(deleted)
}

fn delete_from_table_if_present(
    transaction: &Transaction<'_>,
    table: &str,
    statement: &str,
    session_id: &str,
) -> Result<usize> {
    if !sqlite_table_exists(transaction, table)? {
        return Ok(0);
    }
    Ok(transaction.execute(statement, params![session_id])?)
}

fn sqlite_table_exists(transaction: &Transaction<'_>, table: &str) -> Result<bool> {
    Ok(transaction
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
            params![table],
            |_| Ok(()),
        )
        .optional()?
        .is_some())
}

fn codex_conversations_from_state_database(
    codex_home: &Path,
    cwd_filter: Option<&Path>,
    since_ms: Option<u64>,
) -> Result<Option<Vec<CodexConversationInfo>>> {
    let database_path = codex_home.join("state_5.sqlite");
    if !database_path.is_file() {
        return Ok(None);
    }
    let connection = Connection::open_with_flags(
        &database_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("打开 Codex 状态索引失败: {}", database_path.display()))?;
    connection.busy_timeout(std::time::Duration::from_secs(2))?;
    let has_threads = connection
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'threads'",
            [],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    if !has_threads {
        return Ok(None);
    }

    let cwd = cwd_filter.map(|path| path.to_string_lossy().into_owned());
    let cutoff_secs = since_ms
        .unwrap_or_default()
        .saturating_div(1000)
        .min(i64::MAX as u64) as i64;
    let mut statement = connection.prepare(
        "SELECT id, cwd, title, rollout_path, \
            updated_at * 1000, created_at * 1000 \
         FROM threads \
         WHERE (?1 IS NULL OR cwd = ?1) \
           AND (?2 = 0 OR updated_at >= ?2) \
         ORDER BY updated_at DESC, id DESC",
    )?;
    let rows = statement.query_map(params![cwd, cutoff_secs], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)?.max(0) as u64,
            row.get::<_, i64>(5)?.max(0) as u64,
        ))
    })?;
    let mut conversations = Vec::new();
    for row in rows {
        let (session_id, cwd, title, file_path, updated_at, created_at) = row?;
        let rollout_path = PathBuf::from(&file_path);
        let Ok(metadata) = fs::metadata(&rollout_path) else {
            continue;
        };
        if !metadata.is_file() {
            continue;
        }
        conversations.push(CodexConversationInfo {
            session_id,
            cwd,
            title,
            file_path,
            size_bytes: metadata.len(),
            updated_at,
            created_at,
        });
    }
    Ok(Some(conversations))
}

fn collect_codex_conversations(
    directory: &Path,
    metadata_index: &HashMap<String, CodexConversationMetadata>,
    cwd_filter: Option<&Path>,
    since_ms: Option<u64>,
    conversations: &mut Vec<CodexConversationInfo>,
) -> Result<()> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(());
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            collect_codex_conversations(
                &path,
                metadata_index,
                cwd_filter,
                since_ms,
                conversations,
            )?;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        let updated_at = metadata
            .modified()
            .map(system_time_to_millis)
            .unwrap_or_default();
        if since_ms.is_some_and(|cutoff| updated_at < cutoff) {
            continue;
        }
        let Some(session_id) = codex_session_id_from_path(&path) else {
            continue;
        };
        // A workspace-scoped query can rely on the session/history indexes.
        // Reading rollout bodies just to discover a cwd turns large archives
        // into an unbounded scan; indexed sessions are the authoritative set
        // for this view. Keep the rollout fallback for installations without
        // an index (and for the unfiltered all-workspaces view).
        if cwd_filter.is_some()
            && !metadata_index.is_empty()
            && !metadata_index.contains_key(&session_id)
        {
            continue;
        }
        let mut conversation_metadata =
            metadata_index.get(&session_id).cloned().unwrap_or_default();
        if let (Some(expected), Some(actual)) = (cwd_filter, conversation_metadata.cwd.as_deref())
            && Path::new(actual).components().ne(expected.components())
        {
            continue;
        }
        if conversation_metadata.cwd.is_none() || conversation_metadata.title.is_empty() {
            let rollout_metadata = codex_conversation_metadata(&path);
            if conversation_metadata.cwd.is_none() {
                conversation_metadata.cwd = rollout_metadata.cwd;
            }
            if conversation_metadata.title.is_empty() {
                conversation_metadata.title = rollout_metadata.title;
            }
        }
        if let Some(expected) = cwd_filter
            && conversation_metadata
                .cwd
                .as_deref()
                .is_none_or(|actual| Path::new(actual).components().ne(expected.components()))
        {
            continue;
        }
        conversations.push(CodexConversationInfo {
            session_id,
            cwd: conversation_metadata.cwd.unwrap_or_default(),
            title: conversation_metadata.title,
            file_path: path.display().to_string(),
            size_bytes: metadata.len(),
            updated_at,
            created_at: metadata
                .created()
                .map(system_time_to_millis)
                .unwrap_or_default(),
        });
    }

    Ok(())
}

#[derive(Debug, Clone, Default)]
pub(super) struct CodexConversationMetadata {
    pub(super) cwd: Option<String>,
    pub(super) title: String,
    title_count: usize,
}

#[derive(Clone, Default)]
struct CodexMetadataCache {
    codex_home: PathBuf,
    history_modified: Option<SystemTime>,
    index_modified: Option<SystemTime>,
    metadata: Arc<HashMap<String, CodexConversationMetadata>>,
}

fn codex_conversation_metadata_index(
    codex_home: &Path,
) -> Arc<HashMap<String, CodexConversationMetadata>> {
    let history_path = codex_home.join("history.jsonl");
    let index_path = codex_home.join("session_index.jsonl");
    let history_modified = fs::metadata(&history_path)
        .and_then(|value| value.modified())
        .ok();
    let index_modified = fs::metadata(&index_path)
        .and_then(|value| value.modified())
        .ok();
    let cache = CODEX_METADATA_CACHE.get_or_init(|| Mutex::new(None));
    if let Some(cached) = crate::lock_or_recover!(cache.lock()).as_ref()
        && cached.codex_home == codex_home
        && cached.history_modified == history_modified
        && cached.index_modified == index_modified
    {
        return Arc::clone(&cached.metadata);
    }
    let mut metadata_by_session = HashMap::new();

    if let Ok(file) = fs::File::open(&history_path) {
        for line in BufReader::new(file)
            .lines()
            .map_while(std::result::Result::ok)
        {
            let Ok(record) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(session_id) = record
                .get("session_id")
                .and_then(Value::as_str)
                .filter(|session_id| looks_like_codex_uuid(session_id))
            else {
                continue;
            };
            let Some(text) = record
                .get("text")
                .and_then(Value::as_str)
                .and_then(normalize_codex_conversation_user_text)
            else {
                continue;
            };
            let metadata = metadata_by_session
                .entry(session_id.to_string())
                .or_insert_with(CodexConversationMetadata::default);
            append_codex_conversation_title(&mut metadata.title, &text, &mut metadata.title_count);
        }
    }

    if let Ok(file) = fs::File::open(&index_path) {
        for line in BufReader::new(file)
            .lines()
            .map_while(std::result::Result::ok)
        {
            let Ok(record) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            let Some(session_id) = record
                .get("id")
                .and_then(Value::as_str)
                .filter(|session_id| looks_like_codex_uuid(session_id))
            else {
                continue;
            };
            let metadata = metadata_by_session
                .entry(session_id.to_string())
                .or_insert_with(CodexConversationMetadata::default);
            if let Some(cwd) = record
                .get("cwd")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|cwd| !cwd.is_empty())
            {
                metadata.cwd = Some(cwd.to_string());
            }
            if metadata.title.is_empty()
                && let Some(title) = record
                    .get("thread_name")
                    .and_then(Value::as_str)
                    .and_then(normalize_codex_conversation_user_text)
            {
                append_codex_conversation_title(
                    &mut metadata.title,
                    &title,
                    &mut metadata.title_count,
                );
            }
        }
    }

    let mut cached = crate::lock_or_recover!(cache.lock());
    let metadata = Arc::new(metadata_by_session);
    *cached = Some(CodexMetadataCache {
        codex_home: codex_home.to_path_buf(),
        history_modified,
        index_modified,
        metadata: Arc::clone(&metadata),
    });
    metadata
}

pub(super) fn codex_conversation_metadata(path: &Path) -> CodexConversationMetadata {
    let mut metadata = CodexConversationMetadata::default();
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return metadata,
    };
    let reader = BufReader::new(file);

    for line in reader
        .lines()
        .map_while(std::result::Result::ok)
        .take(MAX_CODEX_CONVERSATION_SCAN_LINES)
    {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };

        if metadata.cwd.is_none() {
            metadata.cwd = codex_conversation_meta_cwd(&value);
        }

        for text in codex_conversation_user_texts(&value) {
            append_codex_conversation_title(&mut metadata.title, &text, &mut metadata.title_count);
            if metadata.title_count >= MAX_CODEX_CONVERSATION_TITLE_MESSAGES {
                break;
            }
        }

        if metadata.cwd.is_some() && metadata.title_count >= MAX_CODEX_CONVERSATION_TITLE_MESSAGES {
            break;
        }
    }

    metadata
}

fn codex_conversation_meta_cwd(value: &Value) -> Option<String> {
    if value.get("type").and_then(Value::as_str) != Some("session_meta") {
        return None;
    }
    let cwd = value
        .get("payload")
        .and_then(|payload| payload.get("cwd"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    if cwd.is_empty() {
        return None;
    }
    Some(cwd.to_string())
}

fn codex_conversation_user_texts(value: &Value) -> Vec<String> {
    let payload = if value.get("type").and_then(Value::as_str) == Some("response_item") {
        value.get("payload").unwrap_or(value)
    } else {
        value
    };
    if payload.get("type").and_then(Value::as_str) != Some("message") {
        return Vec::new();
    }
    if payload.get("role").and_then(Value::as_str) != Some("user") {
        return Vec::new();
    }

    let Some(content) = payload.get("content") else {
        return Vec::new();
    };
    if let Some(text) = content.as_str() {
        return normalize_codex_conversation_user_text(text)
            .into_iter()
            .collect();
    }

    content
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|part| {
            let kind = part.get("type").and_then(Value::as_str).unwrap_or_default();
            if !matches!(kind, "input_text" | "text") {
                return None;
            }
            let text = part.get("text").and_then(Value::as_str)?;
            normalize_codex_conversation_user_text(text)
        })
        .collect()
}

fn normalize_codex_conversation_user_text(text: &str) -> Option<String> {
    let normalized = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n");
    if normalized.is_empty() || should_skip_codex_conversation_user_text(&normalized) {
        return None;
    }
    Some(normalized)
}

fn should_skip_codex_conversation_user_text(text: &str) -> bool {
    text == "继续"
        || text.eq_ignore_ascii_case("continue")
        || text.starts_with("[from webClx-compile-api]")
        || text.starts_with("<turn_aborted>")
        || text.starts_with("Skill descriptions were shortened to fit the 2% skills context budget")
        || text.starts_with("service temporarily unavailable (source:")
        || text.starts_with("# AGENTS.md instructions")
        || text.starts_with("<environment_context>")
        || text.starts_with("<permissions instructions>")
        || text.starts_with("<collaboration_mode>")
        || text.starts_with("<skills_instructions>")
        || text.starts_with("<model_config>")
}

fn append_codex_conversation_title(title: &mut String, text: &str, title_count: &mut usize) {
    if *title_count >= MAX_CODEX_CONVERSATION_TITLE_MESSAGES {
        return;
    }
    let separator_chars = usize::from(!title.is_empty());
    let used_chars = title.chars().count();
    let remaining = MAX_CODEX_CONVERSATION_TITLE_CHARS.saturating_sub(used_chars + separator_chars);
    if remaining == 0 {
        return;
    }

    if !title.is_empty() {
        title.push('\n');
    }
    title.push_str(&clamp_text_chars(text, remaining));
    *title_count += 1;
}

fn clamp_text_chars(text: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for (index, ch) in text.chars().enumerate() {
        if index >= max_chars {
            output.push('…');
            break;
        }
        output.push(ch);
    }
    output
}
