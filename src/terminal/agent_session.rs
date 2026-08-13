use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};
use terminal_core::{
    ResumeCommandInfo, claude_session_id_from_path, codex_session_id_from_path,
    resume_command_for_program, resume_command_info_from_text, system_time_to_millis,
    tmux_session_name,
};

use serde_json::Value;

use super::{sanitize_child_command, tmux::capture_tmux_activity_pane_snapshot};

const COMPLETE_DETECTION_SCREEN_LINE_LIMIT: u32 = 240;

#[derive(Debug, Clone)]
struct AgentSessionCandidate {
    info: ResumeCommandInfo,
    modified_at: u64,
    path: PathBuf,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
struct SessionScreenMatch {
    matched_chars: usize,
    matched_messages: usize,
}

#[derive(Debug, Default)]
struct CodexHistoryMatch {
    screen_match: SessionScreenMatch,
    latest_at: u64,
    same_cwd: bool,
    matched_texts: HashSet<String>,
}

#[derive(Debug, Clone)]
pub(super) struct DetectedResumeSession {
    pub info: ResumeCommandInfo,
    pub source: &'static str,
}

pub(super) struct ResumeSessionDetector {
    pane_pids: HashMap<String, u32>,
    children_by_parent: HashMap<u32, Vec<u32>>,
}

impl ResumeSessionDetector {
    pub(super) fn new() -> Result<Self> {
        Ok(Self {
            pane_pids: read_tmux_pane_pids()?,
            children_by_parent: read_proc_children_map(),
        })
    }

    pub(super) fn detect(&self, session_id: &str) -> Option<DetectedResumeSession> {
        if let Some(detected) = self.detect_from_processes(session_id, None) {
            return Some(detected);
        }

        if let Some(detected) = resume_session_from_claude_projects_for_terminal(session_id) {
            return Some(detected);
        }

        resume_session_from_tmux_snapshot(session_id)
    }

    pub(super) fn detect_complete(&self, session_id: &str) -> Option<DetectedResumeSession> {
        let snapshot =
            capture_tmux_activity_pane_snapshot(session_id, COMPLETE_DETECTION_SCREEN_LINE_LIMIT)
                .ok();
        let snapshot_text = snapshot
            .as_deref()
            .map(String::from_utf8_lossy)
            .map(|text| text.into_owned())
            .unwrap_or_default();

        // The browser has already tried its visible screen and /status output.
        // Use only the current tmux window to score active rollout candidates;
        // full scrollback can contain stale resume commands from prior work.
        if let Some(detected) = self.detect_from_processes(session_id, Some(&snapshot_text)) {
            return Some(detected);
        }

        if let Some(detected) = resume_session_from_tmux_snapshot_text(&snapshot_text) {
            return Some(detected);
        }

        if let Some(detected) =
            resume_session_from_claude_projects_for_terminal_with_screen(session_id, &snapshot_text)
        {
            return Some(detected);
        }

        resume_session_from_codex_history_for_terminal(session_id, &snapshot_text)
    }

    fn detect_from_processes(
        &self,
        session_id: &str,
        screen_text: Option<&str>,
    ) -> Option<DetectedResumeSession> {
        if let Some(pane_pid) = self.pane_pids.get(&tmux_session_name(session_id)).copied() {
            let process_ids =
                descendant_process_ids_from_children(pane_pid, &self.children_by_parent);
            let mut candidates = Vec::new();
            for process_id in process_ids {
                candidates.extend(agent_session_candidates_from_process(process_id));
            }

            let mut scored_candidates = candidates
                .drain(..)
                .map(|candidate| {
                    let screen_match = screen_text
                        .filter(|text| !text.trim().is_empty())
                        .map(|text| agent_session_screen_match_score(&candidate.path, text))
                        .unwrap_or_default();
                    (candidate, screen_match)
                })
                .collect::<Vec<_>>();
            scored_candidates.sort_by(|(left, left_match), (right, right_match)| {
                right_match
                    .cmp(left_match)
                    .then_with(|| right.modified_at.cmp(&left.modified_at))
                    .then_with(|| right.info.resume_id.cmp(&left.info.resume_id))
            });
            scored_candidates
                .dedup_by(|(left, _), (right, _)| left.info.resume_id == right.info.resume_id);

            if let Some((candidate, screen_match)) = scored_candidates.first() {
                return Some(DetectedResumeSession {
                    info: candidate.info.clone(),
                    source: if screen_match.matched_messages > 0 {
                        "process_fd_screen_match"
                    } else {
                        "process_fd"
                    },
                });
            }
        }

        None
    }
}

pub(super) fn detect_current_resume_session(
    session_id: &str,
) -> Result<Option<DetectedResumeSession>> {
    Ok(ResumeSessionDetector::new()?.detect(session_id))
}

pub(super) fn detect_current_resume_session_complete(
    session_id: &str,
) -> Result<Option<DetectedResumeSession>> {
    Ok(ResumeSessionDetector::new()?.detect_complete(session_id))
}

pub(super) fn current_resume_agent_process_ids(
    session_id: &str,
    resume_id: &str,
) -> Result<Vec<u32>> {
    #[cfg(windows)]
    {
        let _ = (session_id, resume_id);
        Ok(Vec::new())
    }

    #[cfg(not(windows))]
    {
        let Some(pane_pid) = tmux_pane_pid(session_id)? else {
            return Ok(Vec::new());
        };
        Ok(process_ids_matching_resume(
            descendant_process_ids(pane_pid),
            resume_id,
            agent_session_candidates_from_process,
        ))
    }
}

fn process_ids_matching_resume<F>(
    process_ids: Vec<u32>,
    resume_id: &str,
    mut candidates_for_process: F,
) -> Vec<u32>
where
    F: FnMut(u32) -> Vec<AgentSessionCandidate>,
{
    let mut matching = process_ids
        .into_iter()
        .filter(|process_id| {
            candidates_for_process(*process_id)
                .iter()
                .any(|candidate| candidate.info.resume_id == resume_id)
        })
        .collect::<Vec<_>>();
    matching.sort_unstable();
    matching.dedup();
    matching
}

fn resume_session_from_tmux_snapshot(session_id: &str) -> Option<DetectedResumeSession> {
    let snapshot =
        capture_tmux_activity_pane_snapshot(session_id, COMPLETE_DETECTION_SCREEN_LINE_LIMIT)
            .ok()?;
    let text = String::from_utf8_lossy(&snapshot);
    resume_session_from_tmux_snapshot_text(&text)
}

fn resume_session_from_tmux_snapshot_text(text: &str) -> Option<DetectedResumeSession> {
    resume_command_info_from_text(&text).map(|info| DetectedResumeSession {
        info,
        source: "terminal_buffer",
    })
}

fn tmux_pane_pid(session_id: &str) -> Result<Option<u32>> {
    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    let output = command
        .arg("display-message")
        .arg("-p")
        .arg("-t")
        .arg(tmux_session_name(session_id))
        .arg("#{pane_pid}")
        .output()
        .context("无法读取 tmux pane pid")?;

    if !output.status.success() {
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.trim().parse::<u32>().ok())
}

fn read_tmux_pane_pids() -> Result<HashMap<String, u32>> {
    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    let output = command
        .arg("list-panes")
        .arg("-a")
        .arg("-F")
        .arg("#{session_name}\t#{pane_pid}")
        .output()
        .context("无法批量读取 tmux pane pid")?;
    if !output.status.success() {
        return Ok(HashMap::new());
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (session_name, pane_pid) = line.split_once('\t')?;
            Some((session_name.to_string(), pane_pid.parse().ok()?))
        })
        .collect())
}

fn descendant_process_ids(root_pid: u32) -> Vec<u32> {
    let children_by_parent = read_proc_children_map();
    descendant_process_ids_from_children(root_pid, &children_by_parent)
}

fn read_proc_children_map() -> HashMap<u32, Vec<u32>> {
    let parent_by_pid = read_proc_parent_map();
    let mut children_by_parent: HashMap<u32, Vec<u32>> = HashMap::new();
    for (pid, parent_pid) in parent_by_pid {
        children_by_parent.entry(parent_pid).or_default().push(pid);
    }
    children_by_parent
}

fn descendant_process_ids_from_children(
    root_pid: u32,
    children_by_parent: &HashMap<u32, Vec<u32>>,
) -> Vec<u32> {
    let mut process_ids = Vec::new();
    let mut stack = vec![root_pid];
    let mut seen = HashSet::new();
    while let Some(process_id) = stack.pop() {
        if !seen.insert(process_id) {
            continue;
        }
        process_ids.push(process_id);
        if let Some(children) = children_by_parent.get(&process_id) {
            stack.extend(children.iter().copied());
        }
    }

    process_ids
}

fn read_proc_parent_map() -> HashMap<u32, u32> {
    let mut parent_by_pid = HashMap::new();
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return parent_by_pid;
    };

    for entry in entries.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };

        let stat_path = entry.path().join("stat");
        let Ok(stat) = std::fs::read_to_string(stat_path) else {
            continue;
        };
        let Some(parent_pid) = parse_proc_stat_parent_pid(&stat) else {
            continue;
        };
        parent_by_pid.insert(pid, parent_pid);
    }

    parent_by_pid
}

fn parse_proc_stat_parent_pid(stat: &str) -> Option<u32> {
    let close_index = stat.rfind(") ")?;
    let mut fields = stat[close_index + 2..].split_whitespace();
    let _state = fields.next()?;
    fields.next()?.parse::<u32>().ok()
}

fn agent_session_candidates_from_process(process_id: u32) -> Vec<AgentSessionCandidate> {
    let fd_path = PathBuf::from(format!("/proc/{process_id}/fd"));
    let Ok(entries) = std::fs::read_dir(fd_path) else {
        return Vec::new();
    };

    let mut candidates = Vec::new();
    for entry in entries.flatten() {
        let Ok(target) = std::fs::read_link(entry.path()) else {
            continue;
        };
        let modified_at = std::fs::metadata(&target)
            .and_then(|metadata| metadata.modified())
            .map(system_time_to_millis)
            .unwrap_or_default();

        // 同一个 fd target 可能同时被下面的探测器命中；先尝试 Codex（rollout-*.jsonl），
        // 再尝试 Claude（projects/<hash>/<uuid>.jsonl）。两个检测器互不重叠，所以同一文件
        // 最多只会被其中一个识别成会话。
        if let Some(resume_id) = codex_session_id_from_path(&target) {
            candidates.push(AgentSessionCandidate {
                info: ResumeCommandInfo {
                    command: resume_command_for_program("codex", &resume_id),
                    program: "codex".to_string(),
                    resume_id,
                },
                modified_at,
                path: target.clone(),
            });
            continue;
        }

        if let Some(resume_id) = claude_session_id_from_path(&target) {
            candidates.push(AgentSessionCandidate {
                info: ResumeCommandInfo {
                    command: resume_command_for_program("claude", &resume_id),
                    program: "claude".to_string(),
                    resume_id,
                },
                modified_at,
                path: target.clone(),
            });
        }
    }

    candidates
}

fn normalize_screen_match_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn message_screen_match_len(screen_text: &str, message: &str) -> usize {
    const MIN_MATCH_CHARS: usize = 8;

    if !is_real_user_message(message) {
        return 0;
    }
    let normalized_message = normalize_screen_match_text(message);
    let message_chars = normalized_message.chars().count();
    if message_chars < MIN_MATCH_CHARS {
        return 0;
    }
    if screen_text.contains(&normalized_message) {
        return message_chars;
    }

    message
        .lines()
        .map(normalize_screen_match_text)
        .filter(|line| line.chars().count() >= MIN_MATCH_CHARS)
        .filter(|line| screen_text.contains(line))
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
}

fn agent_session_screen_match_score(path: &Path, screen_text: &str) -> SessionScreenMatch {
    let normalized_screen = normalize_screen_match_text(screen_text);
    if normalized_screen.is_empty() {
        return SessionScreenMatch::default();
    }

    let mut matched_texts = HashSet::new();
    let mut screen_match = SessionScreenMatch::default();
    for (message, _) in parse_rollout_user_messages(path) {
        let normalized_message = normalize_screen_match_text(&message);
        if !matched_texts.insert(normalized_message) {
            continue;
        }
        let matched_chars = message_screen_match_len(&normalized_screen, &message);
        if matched_chars == 0 {
            continue;
        }
        screen_match.matched_messages += 1;
        screen_match.matched_chars += matched_chars;
    }
    screen_match
}

fn tmux_environment_value(session_id: &str, key: &str) -> Option<String> {
    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    let output = command
        .arg("show-environment")
        .arg("-t")
        .arg(tmux_session_name(session_id))
        .arg(key)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8_lossy(&output.stdout)
        .trim()
        .strip_prefix(&format!("{key}="))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn codex_home_for_tmux_session(session_id: &str) -> Option<PathBuf> {
    tmux_environment_value(session_id, "HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .map(|home| home.join(".codex"))
}

fn claude_config_dir_for_tmux_session(session_id: &str) -> Option<PathBuf> {
    tmux_environment_value(session_id, "HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .map(|home| home.join(".claude"))
}

pub(super) fn tmux_pane_current_path(session_id: &str) -> Option<PathBuf> {
    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    let output = command
        .arg("display-message")
        .arg("-p")
        .arg("-t")
        .arg(tmux_session_name(session_id))
        .arg("#{pane_current_path}")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim());
    path.is_absolute().then_some(path)
}

fn looks_like_codex_session_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 36
        && [8, 13, 18, 23].iter().all(|index| bytes[*index] == b'-')
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| [8, 13, 18, 23].contains(&index) || byte.is_ascii_hexdigit())
}

fn codex_session_cwd_index(codex_home: &Path) -> HashMap<String, PathBuf> {
    let Ok(file) = fs::File::open(codex_home.join("session_index.jsonl")) else {
        return HashMap::new();
    };
    BufReader::new(file)
        .lines()
        .map_while(std::result::Result::ok)
        .filter_map(|line| {
            let record = serde_json::from_str::<Value>(&line).ok()?;
            let session_id = record
                .get("id")
                .and_then(Value::as_str)
                .filter(|value| looks_like_codex_session_id(value))?;
            let cwd = record
                .get("cwd")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            Some((session_id.to_string(), PathBuf::from(cwd)))
        })
        .collect()
}

fn paths_match(left: &Path, right: &Path) -> bool {
    left.components().eq(right.components())
}

fn process_runs_program(process_id: u32, expected: &str) -> bool {
    let Ok(cmdline) = fs::read(format!("/proc/{process_id}/cmdline")) else {
        return false;
    };
    let Some(program) = cmdline.split(|byte| *byte == 0).next() else {
        return false;
    };
    let program = String::from_utf8_lossy(program);
    Path::new(program.as_ref())
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

fn terminal_runs_program(session_id: &str, expected: &str) -> bool {
    tmux_pane_pid(session_id)
        .ok()
        .flatten()
        .map(descendant_process_ids)
        .is_some_and(|process_ids| {
            process_ids
                .into_iter()
                .any(|process_id| process_runs_program(process_id, expected))
        })
}

fn rollout_file_matches_cwd(path: &Path, expected_cwd: &Path) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    BufReader::new(file)
        .lines()
        .map_while(std::result::Result::ok)
        .filter_map(|line| serde_json::from_str::<Value>(&line).ok())
        .filter_map(|record| record.get("cwd").and_then(Value::as_str).map(PathBuf::from))
        .any(|cwd| paths_match(&cwd, expected_cwd))
}

fn claude_rollout_path_from_projects(
    claude_config_dir: &Path,
    current_cwd: &Path,
    screen_text: &str,
) -> Option<PathBuf> {
    if normalize_screen_match_text(screen_text).is_empty() {
        return None;
    }

    let projects_dir = claude_config_dir.join("projects");
    let project_key = current_cwd.to_string_lossy().replace(['/', '\\'], "-");
    let preferred_dir = projects_dir.join(project_key);
    let project_dirs = if preferred_dir.is_dir() {
        vec![preferred_dir]
    } else {
        fs::read_dir(&projects_dir)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .collect::<Vec<_>>()
    };

    let mut candidates = project_dirs
        .into_iter()
        .filter_map(|directory| fs::read_dir(directory).ok())
        .flatten()
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_file() || claude_session_id_from_path(&path).is_none() {
                return None;
            }
            let modified_at = entry
                .metadata()
                .ok()
                .and_then(|metadata| metadata.modified().ok())
                .map(system_time_to_millis)
                .unwrap_or_default();
            Some((modified_at, path))
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|(left_modified, left_path), (right_modified, right_path)| {
        right_modified
            .cmp(left_modified)
            .then_with(|| right_path.cmp(left_path))
    });

    candidates
        .into_iter()
        .filter(|(_, path)| rollout_file_matches_cwd(path, current_cwd))
        .map(|(modified_at, path)| {
            let score = agent_session_screen_match_score(&path, screen_text);
            (score, modified_at, path)
        })
        .filter(|(score, _, _)| score.matched_messages > 0)
        .max_by(
            |(left_score, left_modified, left_path), (right_score, right_modified, right_path)| {
                left_score
                    .cmp(right_score)
                    .then_with(|| left_modified.cmp(right_modified))
                    .then_with(|| left_path.cmp(right_path))
            },
        )
        .map(|(_, _, path)| path)
}

fn claude_rollout_path_for_terminal(session_id: &str, screen_text: &str) -> Option<PathBuf> {
    if !terminal_runs_program(session_id, "claude") {
        return None;
    }
    let claude_config_dir = claude_config_dir_for_tmux_session(session_id)?;
    let current_cwd = tmux_pane_current_path(session_id)?;
    claude_rollout_path_from_projects(&claude_config_dir, &current_cwd, screen_text)
}

fn resume_session_from_claude_projects_for_terminal(
    session_id: &str,
) -> Option<DetectedResumeSession> {
    let snapshot =
        capture_tmux_activity_pane_snapshot(session_id, COMPLETE_DETECTION_SCREEN_LINE_LIMIT)
            .ok()?;
    resume_session_from_claude_projects_for_terminal_with_screen(
        session_id,
        &String::from_utf8_lossy(&snapshot),
    )
}

fn resume_session_from_claude_projects_for_terminal_with_screen(
    session_id: &str,
    screen_text: &str,
) -> Option<DetectedResumeSession> {
    let path = claude_rollout_path_for_terminal(session_id, screen_text)?;
    let resume_id = claude_session_id_from_path(&path)?;
    Some(DetectedResumeSession {
        info: ResumeCommandInfo {
            command: resume_command_for_program("claude", &resume_id),
            program: "claude".to_string(),
            resume_id,
        },
        source: "claude_projects_screen_match",
    })
}

fn resume_session_from_codex_history_for_terminal(
    terminal_session_id: &str,
    screen_text: &str,
) -> Option<DetectedResumeSession> {
    let codex_home = codex_home_for_tmux_session(terminal_session_id)?;
    let current_cwd = tmux_pane_current_path(terminal_session_id);
    resume_session_from_codex_history(&codex_home, current_cwd.as_deref(), screen_text)
}

fn resume_session_from_codex_history(
    codex_home: &Path,
    current_cwd: Option<&Path>,
    screen_text: &str,
) -> Option<DetectedResumeSession> {
    let normalized_screen = normalize_screen_match_text(screen_text);
    if normalized_screen.is_empty() {
        return None;
    }
    let cwd_by_session = codex_session_cwd_index(codex_home);
    let file = fs::File::open(codex_home.join("history.jsonl")).ok()?;
    let mut matches: HashMap<String, CodexHistoryMatch> = HashMap::new();

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
            .filter(|value| looks_like_codex_session_id(value))
        else {
            continue;
        };
        let candidate_cwd = cwd_by_session.get(session_id);
        if let (Some(expected), Some(actual)) = (current_cwd, candidate_cwd)
            && !paths_match(expected, actual)
        {
            continue;
        }
        let Some(message) = record.get("text").and_then(Value::as_str) else {
            continue;
        };
        let matched_chars = message_screen_match_len(&normalized_screen, message);
        if matched_chars == 0 {
            continue;
        }

        let normalized_message = normalize_screen_match_text(message);
        let candidate = matches.entry(session_id.to_string()).or_default();
        candidate.latest_at = candidate
            .latest_at
            .max(record.get("ts").and_then(Value::as_u64).unwrap_or_default());
        candidate.same_cwd = current_cwd
            .zip(candidate_cwd.map(PathBuf::as_path))
            .is_some_and(|(expected, actual)| paths_match(expected, actual));
        if candidate.matched_texts.insert(normalized_message) {
            candidate.screen_match.matched_messages += 1;
            candidate.screen_match.matched_chars += matched_chars;
        }
    }

    let (resume_id, _) = matches
        .into_iter()
        .max_by(|(left_id, left), (right_id, right)| {
            left.screen_match
                .cmp(&right.screen_match)
                .then_with(|| left.same_cwd.cmp(&right.same_cwd))
                .then_with(|| left.latest_at.cmp(&right.latest_at))
                .then_with(|| left_id.cmp(right_id))
        })?;
    Some(DetectedResumeSession {
        info: ResumeCommandInfo {
            command: resume_command_for_program("codex", &resume_id),
            program: "codex".to_string(),
            resume_id,
        },
        source: "codex_history_screen_match",
    })
}

/// 解析 ISO-8601 时间戳（Codex rollout 与 Claude 会话均用 UTC Z 后缀），返回毫秒。
fn parse_rollout_timestamp(value: &str) -> Option<u64> {
    // 期望形如 "2026-07-01T17:01:57.057Z"
    let value = value.trim().trim_end_matches('Z');
    let (date_part, time_part) = value.split_once('T')?;
    let mut date_iter = date_part.split('-');
    let year: i64 = date_iter.next()?.parse().ok()?;
    let month: u32 = date_iter.next()?.parse().ok()?;
    let day: u32 = date_iter.next()?.parse().ok()?;

    let (main_time, frac) = match time_part.split_once('.') {
        Some((m, f)) => (m, f),
        None => (time_part, "0"),
    };
    let mut time_iter = main_time.split(':');
    let hour: u32 = time_iter.next()?.parse().ok()?;
    let minute: u32 = time_iter.next()?.parse().ok()?;
    let second: u32 = time_iter.next()?.parse().ok()?;
    let millis: u64 = {
        let frac_digits = frac.split('+').next().unwrap_or(frac);
        let padded = format!("{:0<3}", &frac_digits[..frac_digits.len().min(3)]);
        padded.parse().unwrap_or(0)
    };

    Some(
        days_from_civil(year, month, day)? as u64 * 86_400_000
            + hour as u64 * 3_600_000
            + minute as u64 * 60_000
            + second as u64 * 1000
            + millis,
    )
}

/// Howard Hinnant 的 civil_from_days 反函数，返回 1970-01-01 起的天数。
fn days_from_civil(year: i64, month: u32, day: u32) -> Option<i64> {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32; // [0, 399]
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    Some(era * 146097 + doe as i64 - 719468)
}

/// 检测终端当前 agent 会话对应的真实 rollout 文件路径。
///
/// 通过 /proc/<pid>/fd 找到 agent 进程正在打开的 Codex/Claude 会话 jsonl。
/// 返回按修改时间最新优先的第一条命中路径。
pub(super) fn detect_current_session_rollout_path(session_id: &str) -> Option<PathBuf> {
    let pane_pid = tmux_pane_pid(session_id).ok().flatten()?;
    let process_ids = descendant_process_ids(pane_pid);

    let mut best: Option<(u64, PathBuf)> = None;
    for process_id in process_ids {
        for candidate in agent_session_candidates_from_process(process_id) {
            match &best {
                Some((current_modified, _)) if candidate.modified_at <= *current_modified => {}
                _ => best = Some((candidate.modified_at, candidate.path)),
            }
        }
    }
    if let Some((_, path)) = best {
        return Some(path);
    }
    let snapshot =
        capture_tmux_activity_pane_snapshot(session_id, COMPLETE_DETECTION_SCREEN_LINE_LIMIT)
            .ok()?;
    claude_rollout_path_for_terminal(session_id, &String::from_utf8_lossy(&snapshot))
}

/// 从 Codex/Claude rollout 文件中提取真实用户消息（对话历史）。
///
/// Codex: payload.type=="message" && role=="user" 的纯文本 content，
///        或 event_msg.payload.type=="user_message" 的 message 字段。
/// Claude: type=="user" 且 message.role=="user"，content 为字符串
///        （列表型 content 是 tool_result，跳过）。
///
/// 自动跳过 base_instructions / AGENTS.md / 系统注入的大段文本，只保留
/// 用户实际输入的对话内容。
pub(super) fn parse_rollout_user_messages(path: &Path) -> Vec<(String, u64)> {
    let Ok(bytes) = std::fs::read(path) else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&bytes);

    let is_codex = path.to_string_lossy().contains("/.codex/sessions/");

    // 先按行顺序收集所有候选 (text, timestamp)，再统一去重。
    let mut candidates: Vec<(String, u64)> = Vec::new();
    for line in text.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };

        let record_type = value.get("type").and_then(|v| v.as_str()).unwrap_or("");

        if is_codex {
            // Codex: response_item.payload.message.user
            if record_type == "response_item"
                && let Some(payload) = value.get("payload")
            {
                let payload_type = payload.get("type").and_then(|v| v.as_str());
                if payload_type == Some("message")
                    && payload.get("role").and_then(|v| v.as_str()) == Some("user")
                    && let Some(text) = extract_codex_user_text(payload)
                {
                    let timestamp = value
                        .get("timestamp")
                        .and_then(|v| v.as_str())
                        .and_then(parse_rollout_timestamp)
                        .unwrap_or(0);
                    if is_real_user_message(&text) {
                        candidates.push((text, timestamp));
                    }
                }
            }
            // Codex: event_msg.payload.user_message
            if record_type == "event_msg"
                && let Some(payload) = value.get("payload")
                && payload.get("type").and_then(|v| v.as_str()) == Some("user_message")
                && let Some(text) = payload
                    .get("message")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                && is_real_user_message(&text)
            {
                let timestamp = value
                    .get("timestamp")
                    .and_then(|v| v.as_str())
                    .and_then(parse_rollout_timestamp)
                    .unwrap_or(0);
                candidates.push((text, timestamp));
            }
        } else {
            // Claude: type=="user", message.role=="user", content 是字符串
            if record_type == "user"
                && let Some(message) = value.get("message")
                && message.get("role").and_then(|v| v.as_str()) == Some("user")
            {
                // 只取字符串 content（真实输入）；列表 content 是 tool_result
                if let Some(text) = message.get("content").and_then(|v| v.as_str()) {
                    // 跳过系统注入的 tool_result 文本和空内容
                    if is_real_user_message(text) {
                        let timestamp = value
                            .get("timestamp")
                            .and_then(|v| v.as_str())
                            .and_then(parse_rollout_timestamp)
                            .unwrap_or(0);
                        candidates.push((text.to_string(), timestamp));
                    }
                }
            }
        }
    }

    // 顺序去重：Codex 会把同一条用户消息同时记成
    // `response_item.payload.message.user` 和 `event_msg.payload.user_message`。
    // 两份记录文本相同，但时间戳经常差 1 毫秒（response_item 先写、event_msg 后写）。
    // 按 (timestamp, text) 精确去重无法覆盖这种毫秒级抖动，会导致每条消息重复一遍。
    // 改为顺序窗口去重：仅当文本与上一条已接受消息相同、且时间戳差在窗口内时跳过。
    // 窗口设为 2 秒——远大于观察到的最大抖动 2ms，但远小于用户连续发送相同消息的
    // 最小间隔（需等待 agent 响应后才能再次输入），不会误杀合法的重复消息。
    const DEDUP_WINDOW_MS: u64 = 2_000;
    let mut messages: Vec<(String, u64)> = Vec::new();
    let mut last_text: Option<String> = None;
    let mut last_timestamp: u64 = 0;
    for (text, timestamp) in candidates {
        let is_dup = last_text.as_deref() == Some(&text)
            && timestamp.abs_diff(last_timestamp) <= DEDUP_WINDOW_MS;
        if !is_dup {
            last_text = Some(text.clone());
            last_timestamp = timestamp;
            messages.push((text, timestamp));
        }
    }

    messages
}

/// 从 Codex response_item.payload 中提取用户文本。
fn extract_codex_user_text(payload: &Value) -> Option<String> {
    let content = payload.get("content")?;
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = content.as_array() {
        let mut parts = Vec::new();
        for item in arr {
            if item.get("type").and_then(|v| v.as_str()) == Some("input_text")
                && let Some(text) = item.get("text").and_then(|v| v.as_str())
            {
                parts.push(text.to_string());
            }
        }
        if parts.is_empty() {
            return None;
        }
        return Some(parts.join("\n"));
    }
    None
}

/// 判断是否是用户真实输入的对话消息，而非系统注入的上下文。
///
/// Codex/Claude 会把 AGENTS.md、系统提示、环境上下文、turn 中断等系统事件
/// 作为 user message 注入 rollout 文件。这些内容以固定的注入标记开头或被
/// 系统标签包裹，用户不会真正"输入"它们。
fn is_real_user_message(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    // 系统注入的大段上下文：AGENTS.md 指令、环境上下文等
    // 这些前缀标记表示整条消息都是系统注入的。
    let injected_prefix_markers = [
        "# AGENTS.md instructions",
        "# AGENTS.md",
        "<INSTRUCTIONS>",
        "<environment_context>",
        "<user_instructions>",
    ];
    for marker in injected_prefix_markers {
        if trimmed.starts_with(marker) {
            return false;
        }
    }
    // 系统标签：Codex 会把 turn 中断、token 用量等系统事件用 <tag>...</tag>
    // 包裹后作为 user message 注入。如果整条消息只包含一个系统标签（去掉标签
    // 后其余文本很短），视为系统注入。
    // 注意：用户可能在消息中引用系统标签文本（如"对话史里有 <turn_aborted>"），
    // 此时用户文本在标签之外，不能误杀。
    let system_tags = ["turn_aborted", "turn_context", "world_state", "token_count"];
    for tag in system_tags {
        // 纯系统标签：整条消息以 <tag> 开头。
        // 用户真实输入不会以系统标签开头（即使引用标签文本，用户的文字也在前面）。
        if trimmed.starts_with(&format!("<{tag}>")) {
            return false;
        }
    }
    // 过长的内容（>4000 字符）几乎一定是注入的系统上下文，而非用户输入
    if trimmed.chars().count() > 4000 {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_codex_rollout(
        file_name: &str,
        lines: &[&str],
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "webclx-rollout-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dir = root.join(".codex/sessions/2026/07");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(file_name);
        let mut file = std::fs::File::create(&path).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
        (root, path)
    }

    #[test]
    fn interrupt_process_filter_keeps_only_target_rollout_holders() {
        let candidate = |resume_id: &str| AgentSessionCandidate {
            info: ResumeCommandInfo {
                command: resume_command_for_program("codex", resume_id),
                program: "codex".to_string(),
                resume_id: resume_id.to_string(),
            },
            modified_at: 0,
            path: PathBuf::new(),
        };
        let matching =
            process_ids_matching_resume(vec![30, 10, 20, 10], "target", |pid| match pid {
                10 => vec![candidate("target")],
                20 => vec![candidate("other")],
                _ => Vec::new(),
            });

        assert_eq!(matching, vec![10]);
    }

    fn write_temp_codex_history(history_lines: &[&str], index_lines: &[&str]) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "webclx-history-match-test-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let mut history = std::fs::File::create(root.join("history.jsonl")).unwrap();
        for line in history_lines {
            writeln!(history, "{line}").unwrap();
        }
        let mut index = std::fs::File::create(root.join("session_index.jsonl")).unwrap();
        for line in index_lines {
            writeln!(index, "{line}").unwrap();
        }
        root
    }

    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn descendant_process_ids_reuses_complete_parent_snapshot() {
        let children_by_parent =
            HashMap::from([(10, vec![11, 12]), (11, vec![13]), (99, vec![100])]);

        let descendants = descendant_process_ids_from_children(10, &children_by_parent)
            .into_iter()
            .collect::<HashSet<_>>();

        assert_eq!(descendants, HashSet::from([10, 11, 12, 13]));
    }

    #[test]
    fn codex_history_fallback_prefers_screen_text_match_over_recency() {
        let matched_id = "019d1ba6-f772-7452-a391-6553ccbc0a50";
        let newer_id = "019d2091-73ef-7522-a073-e5a4b8195fe7";
        let codex_home = write_temp_codex_history(
            &[
                &format!(
                    r#"{{"session_id":"{matched_id}","ts":10,"text":"统一获取 Session ID 的检测顺序"}}"#
                ),
                &format!(r#"{{"session_id":"{newer_id}","ts":20,"text":"完全无关的新对话内容"}}"#),
            ],
            &[
                &format!(r#"{{"id":"{matched_id}","cwd":"/home/codes/webClx"}}"#),
                &format!(r#"{{"id":"{newer_id}","cwd":"/home/codes/webClx"}}"#),
            ],
        );

        let detected = resume_session_from_codex_history(
            &codex_home,
            Some(Path::new("/home/codes/webClx")),
            "正在处理：统一获取 Session ID 的检测顺序\n继续",
        )
        .unwrap();
        let _ = std::fs::remove_dir_all(&codex_home);

        assert_eq!(detected.info.resume_id, matched_id);
        assert_eq!(detected.source, "codex_history_screen_match");
    }

    #[test]
    fn codex_history_fallback_does_not_choose_by_recency_without_screen_match() {
        let recent_id = "019d2091-73ef-7522-a073-e5a4b8195fe7";
        let codex_home = write_temp_codex_history(
            &[&format!(
                r#"{{"session_id":"{recent_id}","ts":20,"text":"最近修改但与屏幕无关的会话"}}"#
            )],
            &[&format!(
                r#"{{"id":"{recent_id}","cwd":"/home/codes/webClx"}}"#
            )],
        );

        let detected = resume_session_from_codex_history(
            &codex_home,
            Some(Path::new("/home/codes/webClx")),
            "当前屏幕展示另一段完全不同的工作内容",
        );
        let _ = std::fs::remove_dir_all(&codex_home);

        assert!(detected.is_none());
    }

    #[test]
    fn process_candidate_screen_score_matches_rollout_user_text() {
        let lines = [
            r#"{"type":"session_meta","payload":{"session_id":"abc"}}"#,
            r#"{"timestamp":"2026-07-01T17:02:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"用屏幕文本确认当前 Codex 会话"}]}}"#,
        ];
        let (temp_root, rollout_path) = write_temp_codex_rollout("rollout-match.jsonl", &lines);

        let score = agent_session_screen_match_score(
            &rollout_path,
            "终端当前显示：用屏幕文本确认当前 Codex 会话",
        );
        let _ = std::fs::remove_dir_all(temp_root);

        assert_eq!(score.matched_messages, 1);
        assert!(score.matched_chars >= 8);
    }

    #[test]
    fn claude_projects_fallback_selects_matching_cwd_and_screen() {
        let root = std::env::temp_dir().join(format!(
            "webclx-claude-fallback-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let claude_config = root.join(".claude");
        let project_dir = claude_config.join("projects/-home-codes-webClx");
        std::fs::create_dir_all(&project_dir).unwrap();
        let current = project_dir.join("86509791-52b5-46e0-899d-9a96fe9b18af.jsonl");
        let unrelated = project_dir.join("97509791-52b5-46e0-899d-9a96fe9b18af.jsonl");
        let wrong_cwd = project_dir.join("a8509791-52b5-46e0-899d-9a96fe9b18af.jsonl");
        std::fs::write(
            &current,
            r#"{"type":"user","cwd":"/home/codes/webClx","message":{"role":"user","content":"后台终端可靠提交验收"},"timestamp":"2026-07-26T10:28:33.114Z"}
"#,
        )
        .unwrap();
        std::fs::write(
            &unrelated,
            r#"{"type":"user","cwd":"/home/codes/webClx","message":{"role":"user","content":"同目录的另一段工作"},"timestamp":"2026-07-26T10:29:33.114Z"}
"#,
        )
        .unwrap();
        std::fs::write(
            &wrong_cwd,
            r#"{"type":"user","cwd":"/home/codes/other","message":{"role":"user","content":"后台终端可靠提交验收"},"timestamp":"2026-07-26T10:30:33.114Z"}
"#,
        )
        .unwrap();

        let selected = claude_rollout_path_from_projects(
            &claude_config,
            Path::new("/home/codes/webClx"),
            "❯ 后台终端可靠提交验收",
        );
        let no_match = claude_rollout_path_from_projects(
            &claude_config,
            Path::new("/home/codes/webClx"),
            "屏幕上完全无关的内容",
        );
        let _ = std::fs::remove_dir_all(root);

        assert_eq!(selected.as_deref(), Some(current.as_path()));
        assert!(no_match.is_none());
    }

    #[test]
    fn parses_codex_user_messages_and_skips_injected_context() {
        let lines = [
            // session_meta ignored
            r#"{"type":"session_meta","payload":{"session_id":"abc"}}"#,
            // injected AGENTS.md context must be skipped
            r##"{"timestamp":"2026-07-01T17:01:57.057Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"# AGENTS.md instructions long context"}]}"##,
            // real user message via response_item
            r#"{"timestamp":"2026-07-01T17:02:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello world"}]}}"#,
            // assistant message ignored
            r#"{"timestamp":"2026-07-01T17:02:01.000Z","type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"hi"}]}}"#,
            // real user message via event_msg
            r#"{"timestamp":"2026-07-01T17:02:05.000Z","type":"event_msg","payload":{"type":"user_message","message":"second question"}}"#,
        ];

        let (temp_root, codex_path) = write_temp_codex_rollout("rollout-test.jsonl", &lines);

        let messages = parse_rollout_user_messages(&codex_path);
        let _ = std::fs::remove_dir_all(temp_root);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].0, "hello world");
        assert_eq!(messages[0].1, 1782925320000);
        assert_eq!(messages[1].0, "second question");
        assert_eq!(messages[1].1, 1782925325000);
    }

    #[test]
    fn parses_codex_user_messages_dedupes_response_item_and_event_msg() {
        // Codex 会在 rollout 里把同一条用户消息同时记成
        // `response_item.payload.message.user` 和 `event_msg.payload.user_message`。
        // 两条记录文本相同、时间戳通常相同；解析器应只保留一条，否则 input-history
        // 会把同一行重复一遍，导致历史工作区“对话历史”列出现重复内容。
        let lines = [
            r#"{"type":"session_meta","payload":{"session_id":"abc"}}"#,
            r#"{"timestamp":"2026-07-06T00:57:51.722Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"项目瘦身，不要到模块太大，js也一样"}]}}"#,
            r#"{"timestamp":"2026-07-06T00:57:51.722Z","type":"event_msg","payload":{"type":"user_message","message":"项目瘦身，不要到模块太大，js也一样"}}"#,
            r#"{"timestamp":"2026-07-06T00:58:24.012Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"不要单模块太大，要对于ai编程管理代码有帮助有收益"}]}}"#,
            r#"{"timestamp":"2026-07-06T00:58:24.012Z","type":"event_msg","payload":{"type":"user_message","message":"不要单模块太大，要对于ai编程管理代码有帮助有收益"}}"#,
        ];

        let (temp_root, codex_path) = write_temp_codex_rollout("rollout-dedupe.jsonl", &lines);

        let messages = parse_rollout_user_messages(&codex_path);
        let _ = std::fs::remove_dir_all(temp_root);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].0, "项目瘦身，不要到模块太大，js也一样");
        assert_eq!(messages[1].0, "不要单模块太大，要对于ai编程管理代码有帮助有收益");
    }

    #[test]
    fn dedupes_codex_user_message_when_timestamps_differ_by_one_ms() {
        // 真实数据中 response_item 与 event_msg 的时间戳经常差 1 毫秒（如 .575Z 与 .576Z），
        // 因为 Codex 异步写入这两条记录。按 (timestamp, text) 精确去重会失效，导致重复。
        // 顺序窗口去重应覆盖这种毫秒级抖动。
        let lines = [
            r#"{"type":"session_meta","payload":{"session_id":"abc"}}"#,
            r#"{"timestamp":"2026-07-15T16:15:42.575Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"在codex_oauth， codex_api和claude_api中，都支持从剪贴板导入账号列表和导出到剪贴板账号列表"}]}}"#,
            r#"{"timestamp":"2026-07-15T16:15:42.576Z","type":"event_msg","payload":{"type":"user_message","message":"在codex_oauth， codex_api和claude_api中，都支持从剪贴板导入账号列表和导出到剪贴板账号列表"}}"#,
        ];

        let (temp_root, codex_path) = write_temp_codex_rollout("rollout-dedupe-1ms.jsonl", &lines);

        let messages = parse_rollout_user_messages(&codex_path);
        let _ = std::fs::remove_dir_all(temp_root);

        assert_eq!(messages.len(), 1);
        assert_eq!(
            messages[0].0,
            "在codex_oauth， codex_api和claude_api中，都支持从剪贴板导入账号列表和导出到剪贴板账号列表"
        );
    }

    #[test]
    fn dedup_preserves_repeated_user_message_after_response_window() {
        // 用户在不同 turn 发送相同内容（如多次"继续"）是合法的重复，不应被去重。
        // 两次"继续"之间隔着 agent 的响应（时间戳差远超 2 秒窗口），应各自保留。
        let lines = [
            r#"{"type":"session_meta","payload":{"session_id":"abc"}}"#,
            r#"{"timestamp":"2026-07-15T15:30:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"继续"}]}}"#,
            r#"{"timestamp":"2026-07-15T15:30:00.001Z","type":"event_msg","payload":{"type":"user_message","message":"继续"}}"#,
            // agent 响应后，用户再次输入"继续"，间隔 5 分钟
            r#"{"timestamp":"2026-07-15T15:35:00.000Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"继续"}]}}"#,
            r#"{"timestamp":"2026-07-15T15:35:00.001Z","type":"event_msg","payload":{"type":"user_message","message":"继续"}}"#,
        ];

        let (temp_root, codex_path) =
            write_temp_codex_rollout("rollout-dedupe-repeat.jsonl", &lines);

        let messages = parse_rollout_user_messages(&codex_path);
        let _ = std::fs::remove_dir_all(temp_root);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].0, "继续");
        assert_eq!(messages[1].0, "继续");
    }

    #[test]
    fn parses_claude_string_user_messages_and_skips_tool_results() {
        let lines = [
            r#"{"type":"mode","mode":"normal"}"#,
            // real typed user message (string content)
            r#"{"type":"user","message":{"role":"user","content":"操作系统有新版，查下如何升级"},"timestamp":"2026-07-01T13:48:28.658Z"}"#,
            // tool_result (list content) must be skipped
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":""}]}}"#,
            // another real message
            r#"{"type":"user","message":{"role":"user","content":"真升级SP4"},"timestamp":"2026-07-01T13:50:00.000Z"}"#,
            // assistant ignored
            r#"{"type":"assistant","message":{"role":"assistant","content":"ok"}}"#,
        ];

        let claude_dir = std::env::temp_dir().join("webclx-claude-projects-test/abc");
        std::fs::create_dir_all(&claude_dir).unwrap();
        let claude_path = claude_dir.join("test-uuid.jsonl");
        // write lines
        {
            use std::io::Write;
            let mut f = std::fs::File::create(&claude_path).unwrap();
            for line in &lines {
                writeln!(f, "{line}").unwrap();
            }
        }

        let messages = parse_rollout_user_messages(&claude_path);
        let _ = std::fs::remove_dir_all(&claude_dir);

        assert_eq!(messages.len(), 2);
        assert_eq!(messages[0].0, "操作系统有新版，查下如何升级");
        assert_eq!(messages[1].0, "真升级SP4");
    }

    #[test]
    fn parse_rollout_timestamp_handles_milliseconds() {
        assert_eq!(parse_rollout_timestamp("2026-07-01T17:02:05.000Z"), Some(1782925325000));
    }

    #[test]
    fn is_real_user_message_filters_system_context() {
        assert!(!is_real_user_message("# AGENTS.md instructions"));
        assert!(!is_real_user_message(""));
        assert!(is_real_user_message("hello"));
        // very long content treated as injected
        let long = "x".repeat(4001);
        assert!(!is_real_user_message(&long));
    }

    #[test]
    fn is_real_user_message_filters_turn_aborted() {
        // Codex injects <turn_aborted> as a standalone user message
        assert!(!is_real_user_message(
            "<turn_aborted>\nThe user interrupted the previous turn on purpose.\n</turn_aborted>"
        ));
        // without closing tag too
        assert!(!is_real_user_message(
            "<turn_aborted> The user interrupted the previous turn on purpose."
        ));
    }

    #[test]
    fn is_real_user_message_preserves_user_text_quoting_system_tags() {
        // User real input that quotes/references a system tag must NOT be filtered.
        // The user's own text precedes the tag, so starts_with does not match.
        assert!(is_real_user_message(
            "当前对话史中有这些系统提示信息：<turn_aborted> The user interrupted"
        ));
        assert!(is_real_user_message("帮我看看 <turn_aborted> 是什么"));
    }

    #[test]
    fn is_real_user_message_filters_other_system_tags() {
        assert!(!is_real_user_message("<turn_context>some context</turn_context>"));
        assert!(!is_real_user_message("<world_state>state</world_state>"));
        assert!(!is_real_user_message("<token_count>12345</token_count>"));
    }
}
