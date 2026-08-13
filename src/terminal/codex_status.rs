use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
};

use serde::Serialize;
use serde_json::Value;
use terminal_core::{codex_session_id_from_path, tmux_session_name};

use super::{
    agent_session::detect_current_session_rollout_path, sanitize_child_command,
    tmux::capture_tmux_text_pane_snapshot,
};

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub(super) struct CodexCompactTokenUsage {
    input_tokens: u64,
    output_tokens: u64,
    total_tokens: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub(super) struct CodexCompactContextWindow {
    used_tokens: u64,
    total_tokens: u64,
    percent_left: u64,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub(super) struct CodexCompactStatus {
    version: String,
    model: String,
    reasoning_effort: String,
    summary_mode: String,
    cwd: String,
    permission: String,
    collaboration_mode: String,
    session_id: String,
    forked_from: String,
    thread_name: String,
    agents_md: Vec<String>,
    token_usage: CodexCompactTokenUsage,
    context_window: CodexCompactContextWindow,
}

pub(super) fn detect_current_codex_status(session_id: &str) -> Option<CodexCompactStatus> {
    let rollout_path = detect_current_session_rollout_path(session_id)
        .or_else(|| rollout_path_from_status_snapshot(session_id))?;
    if !rollout_path.to_string_lossy().contains("/.codex/sessions/") {
        return None;
    }
    let file = fs::File::open(&rollout_path).ok()?;
    let mut status = parse_codex_status_records(BufReader::new(file));
    if status.session_id.is_empty() {
        return None;
    }

    let codex_root = codex_root_from_rollout_path(&rollout_path);
    if let Some(root) = codex_root.as_deref() {
        status.thread_name =
            read_thread_name(&root.join("session_index.jsonl"), &status.session_id)
                .unwrap_or_default();
        status.agents_md = agents_documents(root, Path::new(&status.cwd));
    }
    Some(status)
}

fn rollout_path_from_status_snapshot(terminal_session_id: &str) -> Option<PathBuf> {
    let snapshot = capture_tmux_text_pane_snapshot(terminal_session_id).ok()?;
    let session_prefix = codex_session_prefix_from_snapshot(&String::from_utf8_lossy(&snapshot))?;
    let codex_root = codex_root_for_tmux_session(terminal_session_id)?;
    find_rollout_path_by_session_prefix(&codex_root, &session_prefix)
}

fn codex_session_prefix_from_snapshot(snapshot: &str) -> Option<String> {
    for raw_line in snapshot.lines().rev() {
        let line = raw_line.trim().trim_matches('│').trim();
        let Some(value) = line.strip_prefix("Session:") else {
            continue;
        };
        let prefix: String = value
            .trim_start()
            .chars()
            .take_while(|character| character.is_ascii_hexdigit() || *character == '-')
            .collect();
        if looks_like_codex_session_prefix(&prefix) {
            return Some(prefix);
        }
    }
    None
}

fn looks_like_codex_session_prefix(value: &str) -> bool {
    (12..=36).contains(&value.len())
        && value.bytes().enumerate().all(|(index, byte)| {
            if [8, 13, 18, 23].contains(&index) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}

fn codex_root_for_tmux_session(terminal_session_id: &str) -> Option<PathBuf> {
    tmux_environment_value(terminal_session_id, "HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .map(|home| home.join(".codex"))
}

fn tmux_environment_value(terminal_session_id: &str, key: &str) -> Option<String> {
    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    let output = command
        .arg("show-environment")
        .arg("-t")
        .arg(tmux_session_name(terminal_session_id))
        .arg(key)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .trim()
        .strip_prefix(&format!("{key}="))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn find_rollout_path_by_session_prefix(codex_root: &Path, prefix: &str) -> Option<PathBuf> {
    if !looks_like_codex_session_prefix(prefix) {
        return None;
    }
    let mut directories = vec![codex_root.join("sessions")];
    let mut matched_session_id: Option<String> = None;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    while let Some(directory) = directories.pop() {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                directories.push(path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let Some(session_id) = codex_session_id_from_path(&path) else {
                continue;
            };
            if !session_id.starts_with(prefix) {
                continue;
            }
            if matched_session_id
                .as_deref()
                .is_some_and(|matched| matched != session_id)
            {
                return None;
            }
            matched_session_id = Some(session_id);
            let modified = entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            if newest
                .as_ref()
                .is_none_or(|(current, _)| modified > *current)
            {
                newest = Some((modified, path));
            }
        }
    }
    newest.map(|(_, path)| path)
}

fn parse_codex_status_records(reader: impl BufRead) -> CodexCompactStatus {
    let mut status = CodexCompactStatus::default();
    let mut last_context_usage = 0_u64;
    let mut context_window = 0_u64;

    for line in reader.lines().map_while(Result::ok) {
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let record_type = record
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let Some(payload) = record.get("payload") else {
            continue;
        };

        match record_type {
            "session_meta" => {
                assign_string(&mut status.version, payload.get("cli_version"));
                assign_string(&mut status.cwd, payload.get("cwd"));
                assign_string(&mut status.session_id, payload.get("id"));
                if status.session_id.is_empty() {
                    assign_string(&mut status.session_id, payload.get("session_id"));
                }
                status.forked_from = forked_from_value(payload).unwrap_or_default();
            }
            "turn_context" => {
                assign_string(&mut status.model, payload.get("model"));
                assign_string(&mut status.reasoning_effort, payload.get("effort"));
                assign_string(&mut status.summary_mode, payload.get("summary"));
                assign_string(&mut status.cwd, payload.get("cwd"));
                status.permission = permission_label(payload);
                status.collaboration_mode = payload
                    .pointer("/collaboration_mode/mode")
                    .and_then(Value::as_str)
                    .map(title_case)
                    .unwrap_or_default();
            }
            "event_msg" if payload.get("type").and_then(Value::as_str) == Some("token_count") => {
                let Some(info) = payload.get("info") else {
                    continue;
                };
                let total = info.get("total_token_usage").unwrap_or(&Value::Null);
                status.token_usage = CodexCompactTokenUsage {
                    input_tokens: json_u64(total.get("input_tokens")),
                    output_tokens: json_u64(total.get("output_tokens")),
                    total_tokens: json_u64(total.get("total_tokens")),
                };
                let last = info.get("last_token_usage").unwrap_or(&Value::Null);
                last_context_usage = json_u64(last.get("total_tokens"));
                context_window = json_u64(info.get("model_context_window"));
            }
            _ => {}
        }
    }

    let percent_left = if context_window == 0 {
        0
    } else {
        context_window
            .saturating_sub(last_context_usage)
            .saturating_mul(100)
            .saturating_add(context_window / 2)
            / context_window
    };
    status.context_window = CodexCompactContextWindow {
        used_tokens: last_context_usage,
        total_tokens: context_window,
        percent_left: percent_left.min(100),
    };
    status
}

fn assign_string(target: &mut String, value: Option<&Value>) {
    if let Some(value) = value.and_then(Value::as_str).map(str::trim)
        && !value.is_empty()
    {
        *target = value.to_string();
    }
}

fn json_u64(value: Option<&Value>) -> u64 {
    value.and_then(Value::as_u64).unwrap_or_default()
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return String::new();
    };
    first.to_uppercase().chain(chars).collect()
}

fn permission_label(payload: &Value) -> String {
    let sandbox_type = payload
        .pointer("/sandbox_policy/type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    match sandbox_type {
        "danger-full-access" => "Full Access".to_string(),
        "workspace-write" => "Workspace Write".to_string(),
        "read-only" => "Read Only".to_string(),
        _ => payload
            .get("approval_policy")
            .and_then(Value::as_str)
            .map(title_case)
            .unwrap_or_default(),
    }
}

fn forked_from_value(payload: &Value) -> Option<String> {
    for pointer in [
        "/forked_from",
        "/parent_thread_id",
        "/thread_source/forked_from",
        "/thread_source/parent_thread_id",
        "/thread_source/thread_id",
    ] {
        if let Some(value) = payload.pointer(pointer).and_then(Value::as_str)
            && !value.trim().is_empty()
        {
            return Some(value.trim().to_string());
        }
    }
    None
}

fn codex_root_from_rollout_path(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|ancestor| ancestor.file_name().and_then(|name| name.to_str()) == Some("sessions"))
        .and_then(Path::parent)
        .map(Path::to_path_buf)
}

fn read_thread_name(path: &Path, session_id: &str) -> Option<String> {
    let file = fs::File::open(path).ok()?;
    let mut found = None;
    for line in BufReader::new(file).lines().map_while(Result::ok) {
        let Ok(record) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if record.get("id").and_then(Value::as_str) != Some(session_id) {
            continue;
        }
        found = record
            .get("thread_name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
    }
    found
}

fn agents_documents(codex_root: &Path, cwd: &Path) -> Vec<String> {
    let mut documents = Vec::new();
    let global = codex_root.join("AGENTS.md");
    if global.is_file() {
        documents.push(global.display().to_string());
    }
    for name in ["AGENTS.md", "AGENTS.MD"] {
        let project = cwd.join(name);
        if project.is_file()
            && !documents
                .iter()
                .any(|item| item == &project.display().to_string())
        {
            documents.push(project.display().to_string());
        }
    }
    documents
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    #[test]
    fn parses_complete_codex_status_from_rollout_records() {
        let input = concat!(
            r#"{"type":"session_meta","payload":{"id":"019f741e-6bb4-7a03-ac49-d28a60ef3765","cli_version":"0.144.5","cwd":"/home/codes/stockScreener","thread_source":{"forked_from":"019f73d6-ece8-72d0-addc-e74da1b25a1a"}}}"#,
            "\n",
            r#"{"type":"turn_context","payload":{"cwd":"/home/codes/stockScreener","model":"gpt-5.6-sol","effort":"xhigh","summary":"auto","sandbox_policy":{"type":"danger-full-access"},"approval_policy":"never","collaboration_mode":{"mode":"default"}}}"#,
            "\n",
            r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":1610000,"output_tokens":45700,"total_tokens":1730000},"last_token_usage":{"total_tokens":15800},"model_context_window":1000000}}}"#,
        );

        let status = parse_codex_status_records(Cursor::new(input));
        assert_eq!(status.version, "0.144.5");
        assert_eq!(status.model, "gpt-5.6-sol");
        assert_eq!(status.reasoning_effort, "xhigh");
        assert_eq!(status.summary_mode, "auto");
        assert_eq!(status.permission, "Full Access");
        assert_eq!(status.collaboration_mode, "Default");
        assert_eq!(status.token_usage.total_tokens, 1_730_000);
        assert_eq!(status.context_window.used_tokens, 15_800);
        assert_eq!(status.context_window.total_tokens, 1_000_000);
        assert_eq!(status.context_window.percent_left, 98);
        assert_eq!(status.forked_from, "019f73d6-ece8-72d0-addc-e74da1b25a1a");
    }

    #[test]
    fn ignores_malformed_records_and_keeps_latest_turn_and_usage() {
        let input = concat!(
            "not-json\n",
            r#"{"type":"session_meta","payload":{"id":"session-1","cli_version":"0.1"}}"#,
            "\n",
            r#"{"type":"turn_context","payload":{"model":"old","sandbox_policy":{"type":"read-only"}}}"#,
            "\n",
            r#"{"type":"turn_context","payload":{"model":"new","sandbox_policy":{"type":"workspace-write"}}}"#,
        );
        let status = parse_codex_status_records(Cursor::new(input));
        assert_eq!(status.model, "new");
        assert_eq!(status.permission, "Workspace Write");
        assert_eq!(status.context_window.percent_left, 0);
    }

    #[test]
    fn extracts_truncated_session_prefix_from_status_snapshot() {
        let snapshot = concat!(
            "╭───────────────────────────────────────────────╮\n",
            "│  >_ OpenAI Codex (v0.144.5)                   │\n",
            "│  Session:              019f74a6-e7f7-7153-8e2 │\n",
            "╰───────────────────────────────────────────────╯\n",
        );
        assert_eq!(
            codex_session_prefix_from_snapshot(snapshot).as_deref(),
            Some("019f74a6-e7f7-7153-8e2")
        );
    }

    #[test]
    fn finds_only_a_unique_rollout_session_prefix() {
        let root =
            std::env::temp_dir().join(format!("webclx-codex-status-prefix-{}", std::process::id()));
        let codex_root = root.join(".codex");
        let sessions = codex_root.join("sessions/2026/07/18");
        fs::create_dir_all(&sessions).expect("create sessions");
        let target =
            sessions.join("rollout-2026-07-18T18-00-00-019f74a6-e7f7-7153-8e27-9b16a35b8da6.jsonl");
        fs::write(&target, "{}\n").expect("write target");
        assert_eq!(
            find_rollout_path_by_session_prefix(&codex_root, "019f74a6-e7f7-7153"),
            Some(target.clone())
        );

        let other =
            sessions.join("rollout-2026-07-18T18-01-00-019f74a6-e7f7-7999-8e27-9b16a35b8da6.jsonl");
        fs::write(other, "{}\n").expect("write other");
        assert_eq!(find_rollout_path_by_session_prefix(&codex_root, "019f74a6-e7f7"), None);
        fs::remove_dir_all(root).expect("remove fixture");
    }
}
