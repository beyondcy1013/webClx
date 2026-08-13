use std::{
    collections::HashSet,
    path::{Component, Path},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const RESUME_ARCHIVE_FILE_NAME: &str = ".webclx-codex-resume-archives.json";
const MAX_RESUME_ID_LEN: usize = 160;
const MAX_RESUME_ARCHIVE_NOTE_LEN: usize = 160;
const MAX_RESUME_ARCHIVE_CWD_LEN: usize = 1000;
const MAX_RESUME_ARCHIVE_TERMINAL_NAME_LEN: usize = 160;
const CODEX_SESSION_ID_LEN: usize = 36;
const RESUME_UUID_SCAN_TOKEN_LIMIT: usize = 64;
const TMUX_IMPORTED_ENV_KEYS_TO_SKIP: [&str; 18] = [
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "WEBCLX_USER_HOME",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
    "PWD",
    "OLDPWD",
    "SHLVL",
    "_",
    "TMUX",
    "TMUX_PANE",
    "TERM",
];
const TMUX_TERMINAL_DEFAULT_ENV_KEYS_TO_SKIP: [&str; 4] = [
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "HOME",
    "WEBCLX_USER_HOME",
];

#[derive(Debug, Deserialize)]
pub struct SaveCodexResumeArchiveRequest {
    pub resume_id: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub terminal_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexResumeArchive {
    pub id: String,
    pub resume_id: String,
    pub command: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub terminal_name: String,
    pub note: String,
    pub source: String,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub last_used_at: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumeCommandInfo {
    pub program: String,
    pub resume_id: String,
    pub command: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CodexResumeArchiveRegistry {
    #[serde(default)]
    pub archives: Vec<CodexResumeArchive>,
}

pub fn load_resume_archive_registry(path: &Path) -> Result<CodexResumeArchiveRegistry> {
    if !path.exists() {
        return Ok(CodexResumeArchiveRegistry::default());
    }

    let content = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut registry: CodexResumeArchiveRegistry = serde_json::from_slice(&content)
        .with_context(|| format!("cannot decode {}", path.display()))?;
    normalize_resume_archives(&mut registry.archives);
    sort_resume_archives(&mut registry.archives);
    Ok(registry)
}

pub fn persist_resume_archive_registry(
    path: &Path,
    registry: &CodexResumeArchiveRegistry,
) -> Result<()> {
    let content =
        serde_json::to_vec_pretty(registry).context("cannot encode Codex resume archive")?;
    std::fs::write(path, content).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn normalize_resume_archives(archives: &mut Vec<CodexResumeArchive>) {
    let mut seen = HashSet::new();
    archives.retain_mut(|archive| {
        let Ok(resume_id) = normalize_resume_id(&archive.resume_id) else {
            return false;
        };
        if !seen.insert(resume_id.clone()) {
            return false;
        }

        archive.id = resume_id.clone();
        archive.resume_id = resume_id.clone();
        archive.command = normalize_resume_command(Some(&archive.command), &resume_id);
        archive.cwd = normalize_resume_archive_cwd(Some(&archive.cwd));
        archive.terminal_name =
            normalize_resume_archive_terminal_name(Some(&archive.terminal_name));
        archive.note = normalize_resume_archive_note(Some(&archive.note), &resume_id);
        archive.source = normalize_resume_archive_source(Some(&archive.source));
        archive.updated_at = archive.updated_at.max(archive.created_at);
        true
    });
}

pub fn sort_resume_archives(archives: &mut [CodexResumeArchive]) {
    archives.sort_by(|left, right| {
        right
            .last_used_at
            .cmp(&left.last_used_at)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.note.cmp(&right.note))
            .then_with(|| left.resume_id.cmp(&right.resume_id))
    });
}

pub fn normalize_resume_id(raw: &str) -> Result<String> {
    let candidate = extract_resume_id_from_text(raw).unwrap_or_else(|| cleanup_resume_token(raw));
    if candidate.is_empty() {
        anyhow::bail!("Codex resume id 不能为空。");
    }
    if candidate.len() > MAX_RESUME_ID_LEN {
        anyhow::bail!("Codex resume id 过长。");
    }
    if !candidate
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        anyhow::bail!("Codex resume id 只能包含字母、数字、横线、下划线或点。");
    }

    Ok(candidate)
}

fn extract_resume_id_from_text(raw: &str) -> Option<String> {
    extract_resume_info_from_text(raw).map(|(_, resume_id)| resume_id)
}

pub fn resume_command_info_from_text(raw: &str) -> Option<ResumeCommandInfo> {
    let (program, resume_id) = extract_resume_info_from_text(raw)?;
    Some(ResumeCommandInfo {
        program: program.to_string(),
        command: resume_command_for_program(program, &resume_id),
        resume_id,
    })
}

fn extract_resume_info_from_text(raw: &str) -> Option<(&'static str, String)> {
    let tokens = raw.split_whitespace().collect::<Vec<_>>();
    for index in 0..tokens.len().saturating_sub(1) {
        let Some(program) = resume_program_at(&tokens, index) else {
            continue;
        };

        let argument_index = index + 2;
        let bounded_scan_end = tokens
            .len()
            .min(argument_index + RESUME_UUID_SCAN_TOKEN_LIMIT);
        let scan_end = (argument_index..bounded_scan_end)
            .find(|candidate| resume_program_at(&tokens, *candidate).is_some())
            .unwrap_or(bounded_scan_end);
        let uuid_scan_text = tokens[argument_index..scan_end].concat();
        if let Some(resume_id) = find_uuid_like_text(&uuid_scan_text) {
            return Some((program, resume_id.to_ascii_lowercase()));
        }

        if let Some(argument) = tokens.get(argument_index) {
            let token = cleanup_resume_token(argument);
            if is_valid_resume_token(&token) {
                return Some((program, token));
            }
        }
    }

    None
}

fn resume_program_at(tokens: &[&str], index: usize) -> Option<&'static str> {
    let program = tokens.get(index)?;
    let keyword = cleanup_resume_keyword(tokens.get(index + 1)?);
    if program.eq_ignore_ascii_case("codex") && keyword.eq_ignore_ascii_case("resume") {
        Some("codex")
    } else if program.eq_ignore_ascii_case("claude") && keyword.eq_ignore_ascii_case("--resume") {
        Some("claude")
    } else {
        None
    }
}

fn cleanup_resume_keyword(raw: &str) -> &str {
    raw.trim_matches(|character: char| {
        matches!(character, ',' | '.' | '!' | '?' | ':' | ';')
            || matches!(character, '，' | '。' | '！' | '？' | '：' | '；')
    })
}

fn cleanup_resume_token(raw: &str) -> String {
    raw.trim()
        .trim_matches(|character: char| {
            matches!(character, '`' | '\'' | '"' | ',' | '.' | '!' | '?' | ')' | ']' | '}')
                || matches!(character, '，' | '。' | '；' | '：')
        })
        .to_string()
}

fn is_valid_resume_token(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= MAX_RESUME_ID_LEN
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

pub fn normalize_resume_archive_note(note: Option<&str>, resume_id: &str) -> String {
    let normalized = note
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_RESUME_ARCHIVE_NOTE_LEN)
        .collect::<String>();

    if normalized.is_empty() {
        format!("Codex {}", short_resume_id(resume_id))
    } else {
        normalized
    }
}

pub fn normalize_resume_archive_source(source: Option<&str>) -> String {
    match source.unwrap_or_default().trim() {
        "process_fd" | "terminal_buffer" | "claude_status" | "manual" => {
            source.unwrap_or_default().trim().to_string()
        }
        _ => "manual".to_string(),
    }
}

pub fn normalize_resume_archive_cwd(cwd: Option<&str>) -> String {
    let trimmed = cwd
        .unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_RESUME_ARCHIVE_CWD_LEN)
        .collect::<String>()
        .trim()
        .replace('\\', "/");

    if trimmed.is_empty() || trimmed == "/" {
        return String::new();
    }

    let mut normalized = Vec::new();
    for component in Path::new(&trimmed).components() {
        match component {
            Component::Normal(part) => normalized.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                if normalized.last().is_some_and(|part| part.as_str() != "..") {
                    normalized.pop();
                } else {
                    normalized.push("..".to_string());
                }
            }
            Component::RootDir | Component::Prefix(_) => return String::new(),
        }
    }

    normalized.join("/")
}

pub fn normalize_resume_archive_terminal_name(name: Option<&str>) -> String {
    name.unwrap_or_default()
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_RESUME_ARCHIVE_TERMINAL_NAME_LEN)
        .collect::<String>()
}

pub fn codex_resume_command(resume_id: &str) -> String {
    format!("codex resume {resume_id}")
}

pub fn resume_command_for_program(program: &str, resume_id: &str) -> String {
    if program.eq_ignore_ascii_case("claude") {
        format!("claude --resume {resume_id}")
    } else {
        codex_resume_command(resume_id)
    }
}

pub fn normalize_resume_command(command: Option<&str>, resume_id: &str) -> String {
    if let Some((program, command_resume_id)) = command.and_then(extract_resume_info_from_text)
        && command_resume_id == resume_id
    {
        return resume_command_for_program(program, resume_id);
    }

    codex_resume_command(resume_id)
}

fn short_resume_id(resume_id: &str) -> String {
    let mut short = resume_id.chars().take(8).collect::<String>();
    if resume_id.chars().count() > 8 {
        short.push_str("...");
    }
    short
}

pub fn default_next_ordinal() -> u64 {
    1
}

pub fn build_tmux_launch_env(
    home: &str,
    shell: &str,
    user_name: &str,
    terminal_default_env: &[(String, String)],
    proxy_env: &[(String, String)],
) -> Vec<(String, String)> {
    let mut launch_env = Vec::new();

    upsert_tmux_env_entry(&mut launch_env, "HOME".to_string(), home.to_string());
    upsert_tmux_env_entry(&mut launch_env, "SHELL".to_string(), shell.to_string());
    upsert_tmux_env_entry(&mut launch_env, "USER".to_string(), user_name.to_string());
    upsert_tmux_env_entry(&mut launch_env, "LOGNAME".to_string(), user_name.to_string());

    for (key, value) in terminal_default_env {
        if should_apply_tmux_terminal_default_env_key(key) {
            upsert_tmux_env_entry(&mut launch_env, key.clone(), value.clone());
        }
    }

    for (key, value) in proxy_env {
        upsert_tmux_env_entry(&mut launch_env, key.clone(), value.clone());
    }

    launch_env
}

pub fn build_tmux_child_env(
    shell_env_entries: &[(String, String)],
    terminal_default_env: &[(String, String)],
    proxy_env: &[(String, String)],
) -> Vec<(String, String)> {
    let mut child_env = Vec::new();

    for (key, value) in shell_env_entries {
        if should_import_tmux_shell_env_key(key) {
            upsert_tmux_env_entry(&mut child_env, key.clone(), value.clone());
        }
    }

    for (key, value) in terminal_default_env {
        if should_apply_tmux_terminal_default_env_key(key) {
            upsert_tmux_env_entry(&mut child_env, key.clone(), value.clone());
        }
    }

    for (key, value) in proxy_env {
        upsert_tmux_env_entry(&mut child_env, key.clone(), value.clone());
    }

    child_env
}

fn should_import_tmux_shell_env_key(key: &str) -> bool {
    !key.trim().is_empty() && !TMUX_IMPORTED_ENV_KEYS_TO_SKIP.contains(&key)
}

fn should_apply_tmux_terminal_default_env_key(key: &str) -> bool {
    !key.trim().is_empty() && !TMUX_TERMINAL_DEFAULT_ENV_KEYS_TO_SKIP.contains(&key)
}

fn upsert_tmux_env_entry(child_env: &mut Vec<(String, String)>, key: String, value: String) {
    if let Some(existing) = child_env
        .iter_mut()
        .find(|(existing_key, _)| existing_key == &key)
    {
        existing.1 = value;
        return;
    }

    child_env.push((key, value));
}

pub fn current_timestamp_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

pub fn system_time_to_millis(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

pub fn codex_session_id_from_path(path: &Path) -> Option<String> {
    let path_text = path.to_string_lossy();
    if !path_text.contains("/.codex/sessions/") {
        return None;
    }

    let file_name = path.file_name()?.to_str()?;
    if !file_name.starts_with("rollout-") || !file_name.ends_with(".jsonl") {
        return None;
    }

    find_uuid_like_text(file_name)
}

/// 从 Claude Code 会话文件路径中提取 resume id。
///
/// Claude Code 把每个会话写成 `<uuid>.jsonl`，存放在
///   `<home>/.claude/projects/<cwd-hash>/<uuid>.jsonl`
/// 或较新版本的 `<home>/.claude/sessions/<uuid>.jsonl`。
///
/// 与 Codex 的 `rollout-<timestamp>-<uuid>.jsonl` 不同，Claude 的文件名本身就是
/// 一个完整的 UUID，因此这里要求整个 basename（去掉 `.jsonl`）必须是合法 UUID，
/// 避免把同名目录或 `memory/MEMORY.md` 这类文件误判成会话。
pub fn claude_session_id_from_path(path: &Path) -> Option<String> {
    let path_text = path.to_string_lossy();
    let is_claude_session_dir =
        path_text.contains("/.claude/projects/") || path_text.contains("/.claude/sessions/");
    if !is_claude_session_dir {
        return None;
    }

    let file_name = path.file_name()?.to_str()?;
    let stem = file_name.strip_suffix(".jsonl")?;
    if !is_uuid_like(stem) {
        return None;
    }

    Some(stem.to_ascii_lowercase())
}

fn find_uuid_like_text(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    if bytes.len() < CODEX_SESSION_ID_LEN {
        return None;
    }

    for index in 0..=bytes.len() - CODEX_SESSION_ID_LEN {
        let candidate = &bytes[index..index + CODEX_SESSION_ID_LEN];
        if is_uuid_like_bytes(candidate) {
            return std::str::from_utf8(candidate).ok().map(ToString::to_string);
        }
    }

    None
}

fn is_uuid_like_bytes(bytes: &[u8]) -> bool {
    if bytes.len() != CODEX_SESSION_ID_LEN {
        return false;
    }

    bytes.iter().enumerate().all(|(index, byte)| {
        if matches!(index, 8 | 13 | 18 | 23) {
            *byte == b'-'
        } else {
            byte.is_ascii_hexdigit()
        }
    })
}

fn is_uuid_like(text: &str) -> bool {
    is_uuid_like_bytes(text.as_bytes())
}

pub fn next_ordinal_for_session(session_id: &str) -> u64 {
    extract_session_ordinal(session_id)
        .map(|ordinal| ordinal.saturating_add(1))
        .unwrap_or(default_next_ordinal())
}

pub fn extract_session_ordinal(session_id: &str) -> Option<u64> {
    session_id.strip_prefix('s')?.parse().ok()
}

pub fn session_sort_ordinal(session_id: &str) -> u64 {
    extract_session_ordinal(session_id).unwrap_or_default()
}

#[derive(Debug, Default)]
pub struct AutoSessionNameClaims {
    used_names: HashSet<String>,
    used_path_indices: HashSet<usize>,
}

impl AutoSessionNameClaims {
    pub fn claim_name(&mut self, name: String) {
        self.used_names.insert(name);
    }

    pub fn claim_path_name(&mut self, name: String) {
        for index in session_name_auto_indices(&name) {
            self.used_path_indices.insert(index);
        }
        self.claim_name(name);
    }
}

pub fn auto_session_name_for_path(path: &Path, index: usize) -> String {
    format!("{}_{}", session_directory_label(path), index.max(1))
}

pub fn normalize_session_name(raw: &str) -> Result<String> {
    // 先去掉首尾空白，再去掉结尾的下划线 —— 用户在输入框最后习惯按 Shift+- 顺手补一个下划线，
    // 保存时直接剥掉，避免后端会话名出现 `demo_` 这种尾部下划线。
    let name = raw.trim().trim_end_matches('_');
    if name.is_empty() {
        anyhow::bail!("名称不能为空。");
    }

    Ok(name.to_string())
}

pub fn session_name_auto_indices(name: &str) -> Vec<usize> {
    let bytes = name.as_bytes();
    let mut indices = Vec::new();
    let mut position = 0;

    while position < bytes.len() {
        if !matches!(bytes[position], b'_' | b'#') {
            position += 1;
            continue;
        }

        let digit_start = position + 1;
        let mut digit_end = digit_start;
        while digit_end < bytes.len() && bytes[digit_end].is_ascii_digit() {
            digit_end += 1;
        }

        let digit_is_bounded = digit_end == bytes.len()
            || bytes
                .get(digit_end)
                .is_some_and(|byte| byte.is_ascii_whitespace() || *byte == b'_');
        if digit_end > digit_start && digit_is_bounded {
            if let Ok(index) = name[digit_start..digit_end].parse::<usize>()
                && index > 0
            {
                indices.push(index);
            }
            position = digit_end;
        } else {
            position += 1;
        }
    }

    indices
}

pub fn next_available_auto_session_name(
    path: &Path,
    start_index: usize,
    claims: &AutoSessionNameClaims,
) -> (String, usize) {
    let mut index = start_index.max(1);
    loop {
        let candidate = auto_session_name_for_path(path, index);
        if !claims.used_names.contains(&candidate) && !claims.used_path_indices.contains(&index) {
            return (candidate, index);
        }
        index = index.saturating_add(1);
    }
}

fn session_directory_label(path: &Path) -> String {
    let candidate = path
        .file_name()
        .map(|name| name.to_string_lossy().trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "根目录".to_string());

    let normalized = candidate
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .to_string();

    if normalized.is_empty() {
        "根目录".to_string()
    } else {
        normalized
    }
}

pub fn relative_to_string(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy()),
            Component::ParentDir => Some("..".into()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[derive(Clone)]
pub struct SessionNameState {
    display_name: String,
    title: String,
    manually_renamed: bool,
}

impl SessionNameState {
    pub fn from_stored(display_name: String, title: String, manually_renamed: bool) -> Self {
        Self {
            display_name,
            title: normalize_session_title(&title).unwrap_or_default(),
            manually_renamed,
        }
    }

    pub fn rename_manual(&mut self, next_name: String) {
        self.display_name = next_name;
        self.manually_renamed = true;
    }

    pub fn rename_auto(&mut self, next_name: String) {
        self.display_name = next_name;
        self.manually_renamed = false;
    }

    pub fn update_title(&mut self, next_title: String) {
        self.title = normalize_session_title(&next_title).unwrap_or_default();
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn title(&self) -> Option<String> {
        normalize_session_title(&self.title)
    }
}

#[derive(Default)]
pub struct TitleTracker {
    mode: TitleMode,
    buffer: Vec<u8>,
}

#[derive(Default)]
enum TitleMode {
    #[default]
    Normal,
    EscSeen,
    Osc,
    OscEscSeen,
}

impl TitleTracker {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        let mut titles = Vec::new();

        for &byte in chunk {
            match self.mode {
                TitleMode::Normal => {
                    if byte == 0x1b {
                        self.mode = TitleMode::EscSeen;
                    }
                }
                TitleMode::EscSeen => {
                    if byte == b']' {
                        self.buffer.clear();
                        self.mode = TitleMode::Osc;
                    } else if byte == 0x1b {
                        self.mode = TitleMode::EscSeen;
                    } else {
                        self.mode = TitleMode::Normal;
                    }
                }
                TitleMode::Osc => {
                    if byte == 0x07 {
                        if let Some(title) = parse_title_bytes(&self.buffer) {
                            titles.push(title);
                        }
                        self.buffer.clear();
                        self.mode = TitleMode::Normal;
                    } else if byte == 0x1b {
                        self.mode = TitleMode::OscEscSeen;
                    } else if self.buffer.len() < 4096 {
                        self.buffer.push(byte);
                    }
                }
                TitleMode::OscEscSeen => {
                    if byte == b'\\' {
                        if let Some(title) = parse_title_bytes(&self.buffer) {
                            titles.push(title);
                        }
                        self.buffer.clear();
                        self.mode = TitleMode::Normal;
                    } else {
                        if self.buffer.len() < 4095 {
                            self.buffer.push(0x1b);
                            self.buffer.push(byte);
                        }
                        self.mode = TitleMode::Osc;
                    }
                }
            }
        }

        titles
    }
}

fn parse_title_bytes(raw: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(raw);
    let candidate = match text.split_once(';') {
        // OSC 0/1/2 设置窗口标题；其他前缀同样取分号后的 title 部分。
        Some((_prefix, title)) => title,
        None => text.as_ref(),
    };

    normalize_session_title(candidate)
}

pub fn normalize_session_title(raw: &str) -> Option<String> {
    let normalized = raw
        .chars()
        .filter(|character| !character.is_control())
        .collect::<String>()
        .trim()
        .to_string();

    if normalized.is_empty() || normalized == "?" {
        None
    } else {
        Some(normalized)
    }
}

pub fn tmux_session_name(session_id: &str) -> String {
    format!("webclx_{session_id}")
}

pub fn tmux_scope_unit_name(session_id: &str) -> String {
    format!("webclx-tmux-{session_id}")
}

pub fn should_disable_tmux_scope_isolation(error_message: &str) -> bool {
    let normalized = error_message.to_ascii_lowercase();
    normalized.contains("failed to start transient scope unit")
        || normalized.contains("interactive authentication required")
        || normalized.contains("authentication is required")
        || normalized.contains("access denied")
        || normalized.contains("failed to connect to bus")
        || normalized.contains("failed to create bus connection")
        || normalized.contains("transport endpoint is not connected")
}

pub fn tmux_missing_session_error(stderr: &str) -> bool {
    let normalized = stderr.to_ascii_lowercase();
    normalized.contains("can't find session") || normalized.contains("no server running")
}

pub fn normalize_tmux_snapshot(bytes: &[u8]) -> Vec<u8> {
    let mut lines: Vec<&[u8]> = bytes.split(|byte| *byte == b'\n').collect();

    while lines
        .last()
        .is_some_and(|line| snapshot_line_is_blank(line))
    {
        lines.pop();
    }

    let estimated_len =
        lines.iter().map(|line| line.len()).sum::<usize>() + lines.len().saturating_mul(2);
    let mut normalized = Vec::with_capacity(estimated_len);

    for (index, line) in lines.iter().enumerate() {
        let trimmed_line = line.strip_suffix(b"\r").unwrap_or(line);
        normalized.extend_from_slice(trimmed_line);
        if index + 1 < lines.len() {
            normalized.extend_from_slice(b"\r\n");
        }
    }

    normalized
}

fn snapshot_line_is_blank(line: &[u8]) -> bool {
    let mut index = 0;

    while index < line.len() {
        match line[index] {
            b' ' | b'\t' | b'\r' => {
                index += 1;
            }
            0x1b => {
                index += 1;
                if index >= line.len() {
                    break;
                }

                match line[index] {
                    b'[' => {
                        index += 1;
                        while index < line.len() {
                            let byte = line[index];
                            index += 1;
                            if (0x40..=0x7e).contains(&byte) {
                                break;
                            }
                        }
                    }
                    b']' => {
                        index += 1;
                        while index < line.len() {
                            let byte = line[index];
                            index += 1;
                            if byte == 0x07 {
                                break;
                            }
                            if byte == 0x1b && line.get(index) == Some(&b'\\') {
                                index += 1;
                                break;
                            }
                        }
                    }
                    _ => {
                        index += 1;
                    }
                }
            }
            _ => return false,
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::{
        AutoSessionNameClaims, CodexResumeArchive, SessionNameState, TitleTracker,
        auto_session_name_for_path, build_tmux_child_env, claude_session_id_from_path,
        codex_session_id_from_path, next_available_auto_session_name, normalize_resume_archive_cwd,
        normalize_resume_archive_terminal_name, normalize_resume_archives,
        normalize_resume_command, normalize_resume_id, normalize_session_name,
        normalize_tmux_snapshot, resume_command_info_from_text, session_name_auto_indices,
        should_disable_tmux_scope_isolation, tmux_missing_session_error,
    };

    #[test]
    fn title_tracker_parses_bel_terminated_titles() {
        let mut tracker = TitleTracker::default();
        let titles = tracker.push(b"\x1b]0;first title\x07");
        assert_eq!(titles, vec!["first title".to_string()]);
    }

    #[test]
    fn title_tracker_parses_split_st_terminated_titles() {
        let mut tracker = TitleTracker::default();
        assert!(tracker.push(b"\x1b]2;split").is_empty());
        let titles = tracker.push(b" title\x1b\\");
        assert_eq!(titles, vec!["split title".to_string()]);
    }

    #[test]
    fn tmux_snapshot_normalizes_lf_to_crlf() {
        assert_eq!(normalize_tmux_snapshot(b"1\n2\r\n3"), b"1\r\n2\r\n3".to_vec());
    }

    #[test]
    fn tmux_snapshot_trims_trailing_blank_rows() {
        assert_eq!(normalize_tmux_snapshot(b"alpha\nbeta\n \n\t\n"), b"alpha\r\nbeta".to_vec());
    }

    #[test]
    fn tmux_scope_disable_detects_manager_permission_failures() {
        assert!(should_disable_tmux_scope_isolation(
            "Failed to start transient scope unit: Interactive authentication required."
        ));
        assert!(should_disable_tmux_scope_isolation("Failed to connect to bus: No medium found"));
        assert!(!should_disable_tmux_scope_isolation("无法创建 tmux 终端会话: can't find pane"));
    }

    #[test]
    fn tmux_missing_session_error_is_specific() {
        assert!(tmux_missing_session_error("can't find session: webclx_s123"));
        assert!(tmux_missing_session_error("no server running on /tmp/tmux-0/default"));
    }

    #[test]
    fn manual_name_marks_session_as_manual() {
        let mut state = SessionNameState::from_stored("webClx#1".to_string(), String::new(), false);
        state.rename_manual("manual title".to_string());
        assert_eq!(state.display_name, "manual title");
        assert!(state.manually_renamed);
    }

    #[test]
    fn auto_name_uses_directory_basename_and_index() {
        let path = std::path::Path::new("/tmp/example-project");
        assert_eq!(auto_session_name_for_path(path, 1), "example-project_1");
        assert_eq!(auto_session_name_for_path(path, 3), "example-project_3");
    }

    #[test]
    fn next_available_auto_name_skips_existing_names() {
        let path = std::path::Path::new("/tmp/workspace");
        let mut claims = AutoSessionNameClaims::default();
        claims.claim_path_name("workspace_2".to_string());
        claims.claim_path_name("workspace_3".to_string());

        assert_eq!(
            next_available_auto_session_name(path, 2, &claims),
            ("workspace_4".to_string(), 4)
        );
    }

    #[test]
    fn next_available_auto_name_skips_existing_auto_indices() {
        let path = std::path::Path::new("/tmp/workspace");
        let mut claims = AutoSessionNameClaims::default();
        claims.claim_path_name("workspace_2 codex".to_string());
        claims.claim_path_name("custom #3".to_string());

        assert_eq!(
            next_available_auto_session_name(path, 2, &claims),
            ("workspace_4".to_string(), 4)
        );
    }

    #[test]
    fn next_available_auto_name_skips_index_with_custom_suffix() {
        let path = std::path::Path::new("/tmp/workspace");
        let mut claims = AutoSessionNameClaims::default();
        claims.claim_path_name("workspace_1_新想法".to_string());

        assert_eq!(
            next_available_auto_session_name(path, 1, &claims),
            ("workspace_2".to_string(), 2)
        );
    }

    #[test]
    fn session_name_auto_indices_extracts_numeric_suffixes() {
        assert_eq!(session_name_auto_indices("workspace_2 codex #5"), vec![2, 5]);
        assert_eq!(session_name_auto_indices("workspace_02"), vec![2]);
        assert_eq!(session_name_auto_indices("project_v2_1"), vec![1]);
        assert_eq!(session_name_auto_indices("workspace_1_新想法"), vec![1]);
        assert!(session_name_auto_indices("workspace_abc #0").is_empty());
    }

    #[test]
    fn normalize_session_name_rejects_blank_values() {
        assert!(normalize_session_name("   ").is_err());
        assert_eq!(normalize_session_name("  workspace_1  ").expect("trimmed name"), "workspace_1");
    }

    #[test]
    fn normalize_session_name_strips_trailing_underscores() {
        // 用户在输入末尾习惯性补的下划线（也覆盖先有空白再下划线的情况）。
        assert_eq!(normalize_session_name("demo_").expect("trimmed name"), "demo");
        assert_eq!(normalize_session_name("demo___").expect("trimmed name"), "demo");
        assert_eq!(normalize_session_name("  demo_  ").expect("trimmed name"), "demo");
        // 中段下划线不能动，自动命名依赖 `workspace_1` 这种结构。
        assert_eq!(normalize_session_name("workspace_1_").expect("trimmed name"), "workspace_1");
        // 全是下划线视为空名，触发原有的非空校验。
        assert!(normalize_session_name("___").is_err());
    }

    #[test]
    fn tmux_child_env_filters_reserved_shell_keys_and_overrides() {
        let env = build_tmux_child_env(
            &[
                ("CLAUDE_CONFIG_DIR".to_string(), "/tmp/outer-claude".to_string()),
                ("HOME".to_string(), "/home/root".to_string()),
                ("PATH".to_string(), "/usr/bin".to_string()),
                ("TERM".to_string(), "xterm-256color".to_string()),
                ("HTTP_PROXY".to_string(), "http://old.example".to_string()),
            ],
            &[
                ("CLAUDE_CONFIG_DIR".to_string(), "/tmp/webclx-claude".to_string()),
                ("CODEX_HOME".to_string(), "/tmp/hidden-codex".to_string()),
                ("HOME".to_string(), "/tmp/hidden-home".to_string()),
                ("WEBCLX_USER_HOME".to_string(), "/tmp/hidden-user".to_string()),
                ("CUSTOM".to_string(), "1".to_string()),
            ],
            &[("HTTP_PROXY".to_string(), "http://127.0.0.1:7890".to_string())],
        );

        assert!(env.iter().all(|(key, _)| key != "TERM"));
        assert!(
            env.iter()
                .any(|(key, value)| key == "HOME" && value == "/home/root")
        );
        assert!(env.iter().all(|(key, _)| key != "CODEX_HOME"));
        assert!(env.iter().all(|(key, _)| key != "WEBCLX_USER_HOME"));
        assert!(
            env.iter()
                .any(|(key, value)| key == "CUSTOM" && value == "1")
        );
        assert!(env.iter().all(|(key, _)| key != "CLAUDE_CONFIG_DIR"));
        assert!(
            env.iter()
                .any(|(key, value)| key == "PATH" && value == "/usr/bin")
        );
        assert!(
            env.iter()
                .any(|(key, value)| key == "HTTP_PROXY" && value == "http://127.0.0.1:7890")
        );
    }

    #[test]
    fn tmux_launch_env_keeps_profile_home_as_codex_config_authority() {
        let env = crate::build_tmux_launch_env(
            "/home/root",
            "/bin/bash",
            "root",
            &[
                ("HOME".to_string(), "/tmp/hidden-home".to_string()),
                ("CODEX_HOME".to_string(), "/tmp/hidden-codex".to_string()),
                ("WEBCLX_USER_HOME".to_string(), "/tmp/hidden-user".to_string()),
            ],
            &[],
        );

        assert!(
            env.iter()
                .any(|(key, value)| key == "HOME" && value == "/home/root")
        );
        assert!(env.iter().all(|(key, _)| key != "CODEX_HOME"));
        assert!(env.iter().all(|(key, _)| key != "WEBCLX_USER_HOME"));
    }

    #[test]
    fn codex_session_id_is_parsed_from_rollout_path() {
        let path = std::path::Path::new(
            "/home/root/.codex/sessions/2026/05/08/rollout-2026-05-08T00-00-00-12345678-1234-1234-1234-1234567890ab.jsonl",
        );

        assert_eq!(
            codex_session_id_from_path(path).as_deref(),
            Some("12345678-1234-1234-1234-1234567890ab")
        );
        assert!(
            codex_session_id_from_path(std::path::Path::new("/home/root/.codex/history.jsonl"))
                .is_none()
        );
    }

    #[test]
    fn claude_session_id_is_parsed_from_projects_path() {
        let path = std::path::Path::new(
            "/home/root/.claude/projects/-home-codes-stockScreener/ad08e570-051b-4b66-8c7a-7b20e434b168.jsonl",
        );

        assert_eq!(
            claude_session_id_from_path(path).as_deref(),
            Some("ad08e570-051b-4b66-8c7a-7b20e434b168")
        );
    }

    #[test]
    fn claude_session_id_rejects_non_session_files() {
        // memory 文档不是会话文件
        assert!(
            claude_session_id_from_path(std::path::Path::new(
                "/home/root/.claude/projects/-home-codes/memory/MEMORY.md"
            ))
            .is_none()
        );
        // 非 UUID 文件名
        assert!(
            claude_session_id_from_path(std::path::Path::new(
                "/home/root/.claude/projects/-home-codes/history.jsonl"
            ))
            .is_none()
        );
        // 全局 settings 不是会话文件
        assert!(
            claude_session_id_from_path(std::path::Path::new("/home/root/.claude/settings.json"))
                .is_none()
        );
        // Codex 路径不应被 Claude 检测器命中
        assert!(claude_session_id_from_path(std::path::Path::new(
            "/home/root/.codex/sessions/2026/05/08/rollout-2026-05-08T00-00-00-12345678-1234-1234-1234-1234567890ab.jsonl"
        ))
        .is_none());
    }

    #[test]
    fn resume_archive_cwd_keeps_relative_workspace_path() {
        assert_eq!(
            normalize_resume_archive_cwd(Some(" webClx/../stockScreener/ ")),
            "stockScreener"
        );
        assert_eq!(normalize_resume_archive_cwd(Some("/tmp/project")), "");
        assert_eq!(normalize_resume_archive_cwd(Some("projects\\webClx\\.")), "projects/webClx");
    }

    #[test]
    fn resume_archive_terminal_name_is_trimmed_and_optional() {
        assert_eq!(normalize_resume_archive_terminal_name(Some("  webClx#12  ")), "webClx#12");
        assert_eq!(normalize_resume_archive_terminal_name(Some("   ")), "");
    }

    #[test]
    fn claude_resume_prompt_preserves_resume_command() {
        let resume_id = "019d1ba6-f772-7452-a391-6553ccbc0a50";

        assert_eq!(
            normalize_resume_id(&format!(
                "To continue this session, run claude --resume {resume_id}"
            ))
            .expect("claude resume id"),
            resume_id
        );
        assert_eq!(
            normalize_resume_command(Some(&format!("claude --resume {resume_id}")), resume_id),
            format!("claude --resume {resume_id}")
        );
    }

    #[test]
    fn codex_selection_prompt_allows_explanatory_text_before_resume_id() {
        let resume_id = "019f741e-6bb4-7a03-ac43-80226f0aaced";
        let prompt = format!(
            "Token usage: total=6,594,994 input=6,180,327 (+ 119,623,680 cached) \
             output=414,667 (reasoning 160,001)\n\
             To continue this session, run codex resume, then select \
             扩展字段基础上注册为dsl字段 ({resume_id})"
        );

        let info = resume_command_info_from_text(&prompt).expect("codex resume selection prompt");

        assert_eq!(info.program, "codex");
        assert_eq!(info.resume_id, resume_id);
        assert_eq!(info.command, format!("codex resume {resume_id}"));
    }

    #[test]
    fn interrupted_codex_selection_prompt_ignores_later_plain_token_attempts() {
        let resume_id = "019f8d03-c14d-7712-b5ac-2a63ebd7af36";
        let prompt = "exceeded retry limit, last status: 429 Too Many\n\
Requests\n\
Token usage: total=52,954,470 input=52,452,152 (+\n\
 106,606,875 cached) output=502,318 (reasoning 16\n\
0,154)\n\
To continue this session, run codex resume, then\n\
select glm接着修 (019f8d03-c14d-7712-b5ac-2a63ebd\n\
7af36)\n\
[root@openeuler longzijue]# codex resume then\n\
bash: /home/root/.local/bin/codex: No such file or directory\n\
[root@openeuler longzijue]# codex resume then\n\
bash: /home/root/.local/bin/codex: No such file or directory";

        let info = resume_command_info_from_text(prompt).expect("interrupted selection prompt");

        assert_eq!(info.program, "codex");
        assert_eq!(info.resume_id, resume_id);
        assert_eq!(info.command, format!("codex resume {resume_id}"));
    }

    #[test]
    fn resume_command_parser_rejects_documentation_without_a_valid_id() {
        let screen_text = "指定预设+resume：恢复命令使用 codex resume / claude --resume";

        assert!(resume_command_info_from_text(screen_text).is_none());
    }

    #[test]
    fn resume_uuid_scan_does_not_cross_into_the_next_resume_command() {
        let later_resume_id = "019f741e-6bb4-7a03-ac43-80226f0aaced";
        let prompt = format!(
            "codex resume local-test_id.1\nTo continue another session, run codex resume {later_resume_id}"
        );

        let info = resume_command_info_from_text(&prompt).expect("first codex resume command");

        assert_eq!(info.resume_id, "local-test_id.1");
    }

    #[test]
    fn resume_archive_normalization_does_not_rewrite_claude_command_to_codex() {
        let resume_id = "019d1ba6-f772-7452-a391-6553ccbc0a50";
        let mut archives = vec![CodexResumeArchive {
            id: resume_id.to_string(),
            resume_id: resume_id.to_string(),
            command: format!("claude --resume {resume_id}"),
            cwd: "webClx".to_string(),
            terminal_name: "webClx#1".to_string(),
            note: "Claude session".to_string(),
            source: "terminal_buffer".to_string(),
            created_at: 10,
            updated_at: 10,
            last_used_at: 0,
        }];

        normalize_resume_archives(&mut archives);

        assert_eq!(archives[0].command, format!("claude --resume {resume_id}"));
    }

    #[test]
    fn resume_command_info_preserves_claude_exit_prompt() {
        let resume_id = "40dca26c-7f30-4ebf-be6f-e0f53ae79f2e";
        let prompt = format!(
            "Press Ctrl-C again to exit\n\nResume this session with:\nclaude --resume {resume_id}"
        );

        let info = resume_command_info_from_text(&prompt).expect("claude resume command");

        assert_eq!(info.program, "claude");
        assert_eq!(info.resume_id, resume_id);
        assert_eq!(info.command, format!("claude --resume {resume_id}"));
    }

    #[test]
    fn title_is_stored_separately_from_display_name() {
        let mut state = SessionNameState::from_stored("webClx#1".to_string(), String::new(), false);
        state.update_title("cargo run".to_string());
        assert_eq!(state.display_name, "webClx#1");
        assert_eq!(state.title(), Some("cargo run".to_string()));
    }

    #[test]
    fn placeholder_title_is_hidden() {
        let mut state =
            SessionNameState::from_stored("webClx#1".to_string(), "?".to_string(), false);
        assert_eq!(state.title(), None);
        state.update_title("?".to_string());
        assert_eq!(state.title(), None);
    }
}
