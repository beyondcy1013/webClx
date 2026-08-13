use auth_core::{CurrentClaudeState, StoredApiPreset};
use portable_pty::{Child, ChildKiller, ExitStatus, PtySize};
use settings_core::{SettingsManager, TerminalQuickCommand, build_settings_response};
use std::{
    fs, io,
    path::Path,
    process::Command,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use terminal_core::build_tmux_child_env;

use super::activity::TerminalAgentActivity;
use super::manager::{
    TerminalActivityProbe, arm_output_observations_for_restore_locked,
    auto_continue_backoff_interval_millis, auto_continue_retry_at_millis,
    capture_terminal_input_history, collect_session_infos_from_probes_locked,
    collect_terminal_auto_continue_schedules, is_terminal_continue_line,
    load_terminal_pending_build_requests, next_auto_session_start_index_for_create,
    persist_terminal_pending_build_registry, prepare_restored_output_observation_locked,
    rebaseline_terminal_output_locked, remove_legacy_codex_command_env_launchers,
    restore_system_probe_output_observation_locked, terminal_auto_continue_cron_entry,
    terminal_auto_continue_due_millis, terminal_error_has_continue_after,
    terminal_error_has_queued_input_after, terminal_error_reset_time_from_tail,
    terminal_reset_time_epoch_millis, terminal_tail_error_keyword,
    terminal_tail_error_keyword_with_manual_interrupt_policy, terminal_tail_has_worked_status,
    terminal_tail_has_working_status,
};
use super::session::TerminalOutputBacklog;
use super::tmux::{
    capture_tmux_text_pane_snapshot, normalize_tmux_startup_script, split_tmux_input_submit_keys,
    tmux_client_names, tmux_session_exists,
};
use super::{
    SessionNameState, StoredTerminalPendingBuildRegistry, StoredTerminalRegistry,
    StoredTerminalSession, TERMINAL_PENDING_BUILD_MAX_AGE_MS, TerminalActivitySnapshot,
    TerminalEnvironmentSnapshot, TerminalInputHistoryCapture, TerminalManager,
    TerminalPendingBuildRequest, TerminalResumeRestoreRecord, TerminalState, TitleTracker,
    api_terminal_startup_for_preset, build_terminal_quick_command_input,
    claude_terminal_unset_env_from_current, collect_session_infos_locked,
    default_terminal_user_name, ensure_unique_session_name_locked, is_mouse_only_input,
    parse_terminal_auto_continue_tasks_from_crontab, persist_terminal_shutdown_restore_registry,
    refresh_auto_session_names_for_path_locked, resolve_terminal_quick_command_input,
    should_notify_session_list_sync, sort_session_ids_by_recent_activity, tmux_session_name,
};

fn test_user_profile() -> crate::runtime_paths::UserProfile {
    crate::runtime_paths::resolve_current_user_profile().unwrap_or_else(|| {
        crate::runtime_paths::resolve_user_profile(crate::runtime_paths::DEFAULT_USER_NAME)
            .expect("default user should resolve")
    })
}

fn assert_tmux_command_succeeded(output: std::process::Output, action: &str) {
    assert!(
        output.status.success(),
        "{action} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
}

fn wait_for_tmux_snapshot_matching(session_id: &str, predicate: impl Fn(&str) -> bool) -> String {
    let started = Instant::now();
    let mut latest_snapshot = String::new();
    let mut latest_error = String::new();

    loop {
        match capture_tmux_text_pane_snapshot(session_id) {
            Ok(snapshot) => {
                latest_snapshot = String::from_utf8_lossy(&snapshot).to_string();
                if predicate(&latest_snapshot) {
                    return latest_snapshot;
                }
            }
            Err(error) => latest_error = error.to_string(),
        }

        if started.elapsed() >= Duration::from_secs(2) {
            panic!(
                "tmux snapshot condition timed out for {session_id}; latest error: {latest_error}; latest snapshot: {latest_snapshot}"
            );
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[derive(Debug)]
struct DummyMasterPty;

impl portable_pty::MasterPty for DummyMasterPty {
    fn resize(&self, _size: PtySize) -> anyhow::Result<()> {
        Ok(())
    }

    fn get_size(&self) -> anyhow::Result<PtySize> {
        Ok(PtySize {
            rows: 24,
            cols: 80,
            pixel_width: 0,
            pixel_height: 0,
        })
    }

    fn try_clone_reader(&self) -> anyhow::Result<Box<dyn io::Read + Send>> {
        Ok(Box::new(io::empty()))
    }

    fn take_writer(&self) -> anyhow::Result<Box<dyn io::Write + Send>> {
        Ok(Box::new(io::sink()))
    }

    #[cfg(unix)]
    fn process_group_leader(&self) -> Option<i32> {
        None
    }

    #[cfg(unix)]
    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd> {
        None
    }
}

#[derive(Debug)]
struct DummyChild;

impl ChildKiller for DummyChild {
    fn kill(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
        Box::new(DummyChild)
    }
}

impl Child for DummyChild {
    fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        Ok(None)
    }

    fn wait(&mut self) -> io::Result<ExitStatus> {
        Ok(ExitStatus::with_exit_code(0))
    }

    fn process_id(&self) -> Option<u32> {
        Some(1)
    }
}

fn build_live_session(stored: &StoredTerminalSession) -> Arc<super::TerminalSession> {
    build_live_session_with_redraw_state(
        stored,
        Instant::now()
            .checked_sub(Duration::from_secs(1))
            .unwrap_or_else(Instant::now),
        true,
    )
}

fn build_live_session_with_redraw_state(
    stored: &StoredTerminalSession,
    attached_at: Instant,
    initial_redraw_suppressed: bool,
) -> Arc<super::TerminalSession> {
    let (broadcaster, _) = tokio::sync::broadcast::channel(4);
    Arc::new(super::TerminalSession {
        id: stored.id.clone(),
        path: stored.path.clone(),
        name_state: RwLock::new(SessionNameState::from_stored(
            stored.name.clone(),
            stored.title.clone(),
            stored.manually_renamed,
        )),
        title_tracker: Mutex::new(TitleTracker::default()),
        master: Arc::new(Mutex::new(Box::new(DummyMasterPty))),
        viewports: Arc::new(Mutex::new(super::session::TerminalViewportRegistry::default())),
        writer: Arc::new(Mutex::new(Box::new(io::sink()))),
        _child: Arc::new(Mutex::new(Box::new(DummyChild))),
        broadcaster,
        backlog: Arc::new(Mutex::new(TerminalOutputBacklog::new())),
        next_output_seq: AtomicU64::new(0),
        initial_snapshot: Arc::new(Mutex::new(None)),
        attached_at,
        suppressed_initial_redraw: AtomicBool::new(initial_redraw_suppressed),
        last_output_at: AtomicU64::new(0),
        alive: Arc::new(AtomicBool::new(true)),
    })
}

fn stored_test_terminal_session(id: &str) -> StoredTerminalSession {
    StoredTerminalSession {
        id: id.to_string(),
        path: std::path::PathBuf::from("/tmp/workspace"),
        user_name: default_terminal_user_name(),
        name: "workspace_1".to_string(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
    }
}

#[test]
fn legacy_terminal_sessions_default_to_normal_origin() {
    let stored: StoredTerminalSession = serde_json::from_value(serde_json::json!({
        "id": "s-legacy",
        "path": "/tmp/workspace",
        "name": "legacy-terminal"
    }))
    .expect("legacy terminal session should deserialize");

    assert_eq!(stored.origin, super::TerminalSessionOrigin::Normal);
    assert!(stored.owner_key.is_empty());
}

#[test]
fn classified_terminal_session_persists_origin_and_owner_key() {
    let stored = StoredTerminalSession::new(
        "s-agent".to_string(),
        std::path::PathBuf::from("/tmp/workspace"),
        default_terminal_user_name(),
        "代理设置".to_string(),
        String::new(),
        String::new(),
        super::TerminalSessionOrigin::Agent,
        "proxy_settings_workflow".to_string(),
    );
    let registry = StoredTerminalRegistry {
        next_ordinal: 2,
        sessions: vec![stored],
        input_histories: std::collections::HashMap::new(),
        output_observations: std::collections::HashMap::new(),
    };

    let restored: StoredTerminalRegistry = serde_json::from_slice(
        &serde_json::to_vec(&registry).expect("serialize terminal registry"),
    )
    .expect("deserialize terminal registry");

    assert_eq!(restored.sessions[0].origin, super::TerminalSessionOrigin::Agent);
    assert_eq!(restored.sessions[0].owner_key, "proxy_settings_workflow");
}

#[test]
fn requested_api_preset_controls_created_session_metadata() {
    let presets = vec![StoredApiPreset {
        id: "api-minimax".to_string(),
        name: "MiniMax3".to_string(),
        saved_at: 1,
        provider_name: String::new(),
        base_url: "https://api.minimax.example/v1".to_string(),
        management_url: None,
        wire_api: None,
        responses_proxy: None,
        apply_upstream_proxy_on_switch: false,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        terminal_env: Vec::new(),
        terminal_startup_script: None,
        api_key: String::new(),
        access_token: String::new(),
        account_id: String::new(),
        access_mode: None,
        switch_count: 0,
    }];

    let startup = api_terminal_startup_for_preset(&presets, " api-minimax ")
        .expect("saved preset should resolve");

    assert_eq!(startup.codex_api_preset_name, "MiniMax3");
    assert_eq!(startup.codex_api_base_url, "https://api.minimax.example/v1");
    assert!(api_terminal_startup_for_preset(&presets, "missing").is_none());
}

#[test]
fn manager_persists_classified_session_origin_across_restart() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;
    let base_dir = std::env::temp_dir().join(format!("webclx-terminal-origin-{unique}"));
    let workspace_dir = base_dir.join("workspace");
    fs::create_dir_all(&workspace_dir).expect("create test workspace");
    let state_file = base_dir.join("terminal-sessions.json");
    let registry = StoredTerminalRegistry {
        next_ordinal: unique.max(1),
        sessions: Vec::new(),
        input_histories: std::collections::HashMap::new(),
        output_observations: std::collections::HashMap::new(),
    };
    fs::write(
        &state_file,
        serde_json::to_vec_pretty(&registry).expect("encode terminal registry"),
    )
    .expect("write terminal registry");
    let manager = TerminalManager::new(state_file.clone());

    let created = manager
        .create_session_with_origin(
            &workspace_dir,
            &workspace_dir,
            workspace_dir.clone(),
            test_user_profile(),
            Vec::new(),
            Vec::new(),
            None,
            String::new(),
            String::new(),
            super::TerminalSessionOrigin::Agent,
            "proxy_settings_workflow".to_string(),
        )
        .expect("create classified terminal session");
    assert_eq!(created.origin, super::TerminalSessionOrigin::Agent);
    assert_eq!(created.owner_key, "proxy_settings_workflow");

    let restarted = TerminalManager::new(state_file);
    let sessions = restarted.list_sessions(
        &workspace_dir,
        &workspace_dir,
        &workspace_dir,
        80,
        &[],
        &[],
        60,
        true,
        1.5,
        20,
    );
    let restored = sessions
        .iter()
        .find(|session| session.id == created.id)
        .expect("classified session should survive restart");
    assert_eq!(restored.origin, super::TerminalSessionOrigin::Agent);
    assert_eq!(restored.owner_key, "proxy_settings_workflow");

    let _ = restarted.delete_session(&workspace_dir, &workspace_dir, &created.id);
    let _ = fs::remove_dir_all(base_dir);
}

#[test]
fn terminal_input_history_records_entered_command() {
    let mut history = TerminalInputHistoryCapture::default();

    capture_terminal_input_history(&mut history, "cargo test\r");

    assert_eq!(history.entries.len(), 1);
    assert_eq!(history.entries[0].text, "cargo test");
}

#[test]
fn terminal_input_history_skips_continue_command() {
    let mut history = TerminalInputHistoryCapture::default();

    capture_terminal_input_history(&mut history, "继续\r");
    capture_terminal_input_history(&mut history, "cargo test\r");

    assert_eq!(history.entries.len(), 1);
    assert_eq!(history.entries[0].text, "cargo test");
}

#[test]
fn terminal_input_history_filters_continue_entries_when_read() {
    let entries = vec![
        super::TerminalInputHistoryEntry {
            text: "first prompt".to_string(),
            created_at: 1,
        },
        super::TerminalInputHistoryEntry {
            text: "  继续  ".to_string(),
            created_at: 2,
        },
        super::TerminalInputHistoryEntry {
            text: "继续处理这个问题".to_string(),
            created_at: 3,
        },
    ];

    let filtered = super::manager::filter_terminal_input_history_entries(entries);
    let texts = filtered
        .iter()
        .map(|entry| entry.text.as_str())
        .collect::<Vec<_>>();

    assert_eq!(texts, vec!["first prompt", "继续处理这个问题"]);
}

#[test]
fn terminal_input_history_handles_backspace_and_escape_sequences() {
    let mut history = TerminalInputHistoryCapture::default();

    capture_terminal_input_history(&mut history, "abc\u{7f}\u{1b}[A\r");

    assert_eq!(history.entries.len(), 1);
    assert_eq!(history.entries[0].text, "ab");
}

#[test]
fn terminal_input_history_splits_bracketed_paste_lines() {
    let mut history = TerminalInputHistoryCapture::default();

    capture_terminal_input_history(&mut history, "\u{1b}[200~line1\nline2\u{1b}[201~\r");

    let texts: Vec<&str> = history
        .entries
        .iter()
        .map(|entry| entry.text.as_str())
        .collect();
    assert_eq!(texts, vec!["line1", "line2"]);
}

#[test]
fn codex_conversation_metadata_extracts_user_title_without_startup_context() {
    let temp_dir = std::env::temp_dir().join(format!(
        "webclx-codex-conversation-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let path =
        temp_dir.join("rollout-2026-06-12T09-33-50-019eb976-cc02-7623-9deb-ccbc8cc873cd.jsonl");
    let lines = [
        serde_json::json!({
            "type": "session_meta",
            "payload": { "cwd": "/home/codes/stockJiepan" }
        }),
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "# AGENTS.md instructions for /home/codes/stockJiepan" },
                    { "type": "input_text", "text": "<environment_context>\n  <cwd>/home/codes/stockJiepan</cwd>" }
                ]
            }
        }),
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "看一下claude优化建议ui.MD，核实一下建议是否有道理" }
                ]
            }
        }),
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "继续" }
                ]
            }
        }),
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "第二个任务\n\n更多细节" }
                ]
            }
        }),
    ]
    .into_iter()
    .map(|value| serde_json::to_string(&value).expect("encode json line"))
    .collect::<Vec<_>>()
    .join("\n");
    fs::write(&path, format!("{lines}\n")).expect("write jsonl");

    let metadata = super::codex_conversation_metadata(&path);

    assert_eq!(metadata.cwd.as_deref(), Some("/home/codes/stockJiepan"));
    assert_eq!(
        metadata.title,
        "看一下claude优化建议ui.MD，核实一下建议是否有道理\n第二个任务\n更多细节"
    );

    fs::remove_dir_all(temp_dir).expect("remove temp dir");
}

#[test]
fn codex_conversation_metadata_skips_automation_status_messages() {
    let temp_dir = std::env::temp_dir().join(format!(
        "webclx-codex-conversation-automation-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let path =
        temp_dir.join("rollout-2026-06-23T22-52-45-019ef4f8-2b78-7ae1-8ed6-793cc59ca4cb.jsonl");
    let lines = [
        serde_json::json!({
            "type": "session_meta",
            "payload": { "cwd": "/home/codes/webClx" }
        }),
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "历史工作区TAB中的对话历史列，不正确" }
                ]
            }
        }),
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "[from webClx-compile-api] kidsAI 编译失败(status=1)；请求 214525；webClx 集中日志：/home/codes/webClx/.webclx-compile-queue/runs/example/build-1.log。请先查看日志定位编译失败原因。" }
                ]
            }
        }),
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "<turn_aborted>\nThe user interrupted the previous turn on purpose.\n</turn_aborted>" }
                ]
            }
        }),
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "Skill descriptions were shortened to fit the 2% skills context budget. Codex can still see every skill." }
                ]
            }
        }),
    ]
    .into_iter()
    .map(|value| serde_json::to_string(&value).expect("encode json line"))
    .collect::<Vec<_>>()
    .join("\n");
    fs::write(&path, format!("{lines}\n")).expect("write jsonl");

    let metadata = super::codex_conversation_metadata(&path);

    assert_eq!(metadata.cwd.as_deref(), Some("/home/codes/webClx"));
    assert_eq!(metadata.title, "历史工作区TAB中的对话历史列，不正确");

    fs::remove_dir_all(temp_dir).expect("remove temp dir");
}

#[test]
fn codex_conversation_scan_uses_indexes_with_rollout_fallback() {
    let temp_dir = std::env::temp_dir().join(format!(
        "webclx-codex-conversation-index-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    let codex_home = temp_dir.join(".codex");
    let sessions_dir = codex_home.join("sessions/2026/07/19");
    fs::create_dir_all(&sessions_dir).expect("create Codex sessions fixture");

    let history_session_id = "019f7777-1111-7222-8333-444444444444";
    let index_session_id = "019f7777-5555-7666-8777-888888888888";
    let fallback_session_id = "019f7777-9999-7aaa-8bbb-cccccccccccc";
    let history_rollout =
        sessions_dir.join(format!("rollout-2026-07-19T09-00-00-{history_session_id}.jsonl"));
    let index_rollout =
        sessions_dir.join(format!("rollout-2026-07-19T09-01-00-{index_session_id}.jsonl"));
    let fallback_rollout =
        sessions_dir.join(format!("rollout-2026-07-19T09-02-00-{fallback_session_id}.jsonl"));
    fs::write(
        &history_rollout,
        "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/rollout/history\"}}\n",
    )
    .expect("write history-backed rollout");
    fs::write(
        &index_rollout,
        "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/rollout/index\"}}\n",
    )
    .expect("write index-backed rollout");
    fs::write(
        &fallback_rollout,
        concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/rollout/fallback\"}}\n",
            "{\"type\":\"response_item\",\"payload\":{\"type\":\"message\",\"role\":\"user\",",
            "\"content\":[{\"type\":\"input_text\",\"text\":\"旧会话回退标题\"}]}}\n"
        ),
    )
    .expect("write fallback rollout");
    fs::write(
        codex_home.join("history.jsonl"),
        format!(
            concat!(
                "{{\"session_id\":\"{0}\",\"ts\":1,\"text\":\"索引中的第一个任务\"}}\n",
                "{{\"session_id\":\"{0}\",\"ts\":2,\"text\":\"继续\"}}\n",
                "{{\"session_id\":\"{0}\",\"ts\":3,\"text\":\"索引中的第二个任务\"}}\n"
            ),
            history_session_id
        ),
    )
    .expect("write Codex input history");
    fs::write(
        codex_home.join("session_index.jsonl"),
        format!(
            concat!(
                "{{\"id\":\"{0}\",\"cwd\":\"/indexed/history\",\"thread_name\":\"历史索引标题\"}}\n",
                "{{\"id\":\"{1}\",\"cwd\":\"/indexed/only\",\"thread_name\":\"仅会话索引标题\"}}\n"
            ),
            history_session_id, index_session_id
        ),
    )
    .expect("write Codex session index");

    let conversations = super::scan_codex_conversations(&codex_home).expect("scan conversations");
    let by_id = conversations
        .iter()
        .map(|conversation| (conversation.session_id.as_str(), conversation))
        .collect::<std::collections::HashMap<_, _>>();

    let history = by_id
        .get(history_session_id)
        .expect("history-backed conversation");
    assert_eq!(history.cwd, "/indexed/history");
    assert_eq!(history.title, "索引中的第一个任务\n索引中的第二个任务");

    let indexed = by_id
        .get(index_session_id)
        .expect("session-index-backed conversation");
    assert_eq!(indexed.cwd, "/indexed/only");
    assert_eq!(indexed.title, "仅会话索引标题");

    let fallback = by_id
        .get(fallback_session_id)
        .expect("rollout fallback conversation");
    assert_eq!(fallback.cwd, "/rollout/fallback");
    assert_eq!(fallback.title, "旧会话回退标题");

    fs::remove_dir_all(temp_dir).expect("remove Codex index fixture");
}

#[test]
fn codex_conversation_scan_filters_by_workspace_cwd() {
    let temp_dir = std::env::temp_dir().join(format!(
        "webclx-codex-conversation-cwd-filter-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    let codex_home = temp_dir.join(".codex");
    let sessions_dir = codex_home.join("sessions/2026/07/19");
    fs::create_dir_all(&sessions_dir).expect("create Codex sessions fixture");

    let matching_session_id = "019f8888-1111-7222-8333-444444444444";
    let other_session_id = "019f8888-5555-7666-8777-888888888888";
    fs::write(
        sessions_dir.join(format!("rollout-2026-07-19T10-00-00-{matching_session_id}.jsonl")),
        "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/home/codes/webClx\"}}\n",
    )
    .expect("write matching rollout");
    fs::write(
        sessions_dir.join(format!("rollout-2026-07-19T10-01-00-{other_session_id}.jsonl")),
        "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/home/codes/other\"}}\n",
    )
    .expect("write non-matching rollout");

    let conversations = super::scan_codex_conversations_for_cwd(
        &codex_home,
        Some(std::path::Path::new("/home/codes/webClx")),
    )
    .expect("scan filtered conversations");

    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].session_id, matching_session_id);
    assert_eq!(conversations[0].cwd, "/home/codes/webClx");

    fs::remove_dir_all(temp_dir).expect("remove Codex cwd filter fixture");
}

#[test]
fn tmux_input_submit_keys_accept_cr_and_lf_suffixes() {
    assert_eq!(split_tmux_input_submit_keys("hello"), ("hello", 0));
    assert_eq!(split_tmux_input_submit_keys("hello\r"), ("hello", 1));
    assert_eq!(split_tmux_input_submit_keys("hello\n"), ("hello", 1));
    assert_eq!(split_tmux_input_submit_keys("hello\r\n"), ("hello", 2));
    assert_eq!(split_tmux_input_submit_keys("hello\r\r"), ("hello", 2));
    assert_eq!(split_tmux_input_submit_keys("hello\nworld\r"), ("hello\nworld", 1));
}

#[test]
fn initial_tmux_redraw_suppresses_multiple_chunks_inside_window() {
    let stored = stored_test_terminal_session("redraw-window");
    let session = build_live_session_with_redraw_state(&stored, Instant::now(), false);

    assert!(session.should_suppress_initial_tmux_redraw());
    assert!(session.should_suppress_initial_tmux_redraw());
}

#[test]
fn initial_tmux_redraw_suppression_expires_after_window() {
    let stored = stored_test_terminal_session("redraw-expired");
    let attached_at = Instant::now()
        .checked_sub(Duration::from_millis(super::INITIAL_TMUX_REDRAW_SUPPRESS_MS + 1))
        .unwrap_or_else(Instant::now);
    let session = build_live_session_with_redraw_state(&stored, attached_at, false);

    assert!(!session.should_suppress_initial_tmux_redraw());
    assert!(!session.should_suppress_initial_tmux_redraw());
}

#[test]
fn tmux_child_env_only_includes_proxy_overrides() {
    let env = build_tmux_child_env(
        &[
            ("HOME".to_string(), "/home/root".to_string()),
            ("PATH".to_string(), "/usr/local/bin:/usr/bin".to_string()),
            ("PWD".to_string(), "/tmp".to_string()),
            ("TERM".to_string(), "xterm-256color".to_string()),
            ("HTTP_PROXY".to_string(), "http://127.0.0.1:1111".to_string()),
        ],
        &[("ANTHROPIC_BASE_URL".to_string(), "https://example.invalid".to_string())],
        &[("HTTP_PROXY".to_string(), "http://127.0.0.1:7890".to_string())],
    );

    assert!(env.iter().all(|(key, _)| key != "PWD"));
    assert!(env.iter().all(|(key, _)| key != "TERM"));
    assert!(
        env.iter()
            .any(|(key, value)| key == "HOME" && value == "/home/root")
    );
    let path = env
        .iter()
        .find(|(key, _)| key == "PATH")
        .map(|(_, value)| value.as_str())
        .unwrap_or_default();
    assert_eq!(path, "/usr/local/bin:/usr/bin");
    assert!(
        env.iter()
            .any(|(key, value)| key == "HTTP_PROXY" && value == "http://127.0.0.1:7890")
    );
    assert!(
        env.iter()
            .any(|(key, value)| key == "ANTHROPIC_BASE_URL" && value == "https://example.invalid")
    );
}

#[test]
fn tmux_client_names_ignores_empty_list_rows() {
    assert_eq!(
        tmux_client_names(b"/dev/pts/41\n\n /dev/pts/42 \n"),
        vec!["/dev/pts/41".to_string(), "/dev/pts/42".to_string()]
    );
}

#[test]
fn terminal_quick_command_submits_original_command_without_env_injection() {
    assert_eq!(
        build_terminal_quick_command_input("claude --resume abc"),
        "claude --resume abc\n"
    );
    assert_eq!(build_terminal_quick_command_input("  codex  "), "codex\n");
}

#[test]
fn legacy_codex_launcher_cleanup_removes_only_webclx_managed_files() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let home = std::env::temp_dir().join(format!("webclx-codex-launcher-cleanup-{unique}"));
    let bin_dir = home.join(".local/bin");
    fs::create_dir_all(&bin_dir).expect("create temporary bin directory");
    let managed = bin_dir.join("codex");
    let custom = bin_dir.join("webclx-codex");
    fs::write(&managed, "#!/bin/sh\n# WEBCLX_CODEX_COMMAND_ENV_WRAPPER\necho managed\n")
        .expect("write managed launcher");
    fs::write(&custom, "#!/bin/sh\necho custom\n").expect("write custom launcher");

    let current = test_user_profile();
    let profile = crate::runtime_paths::UserProfile {
        home: home.clone(),
        ..current
    };
    let removed = remove_legacy_codex_command_env_launchers(&profile)
        .expect("remove managed legacy launcher");

    assert_eq!(removed, vec![managed.clone()]);
    assert!(!managed.exists());
    assert_eq!(fs::read_to_string(&custom).unwrap(), "#!/bin/sh\necho custom\n");
    fs::remove_dir_all(home).expect("remove temporary home");
}

#[test]
fn terminal_manager_removes_legacy_command_env_directory() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos();
    let base_dir = std::env::temp_dir().join(format!("webclx-command-env-cleanup-{unique}"));
    let command_env_dir = base_dir.join(".terminal-command-env");
    fs::create_dir_all(&command_env_dir).expect("create legacy command env directory");
    fs::write(command_env_dir.join("s1.sh"), "export STALE=1\n")
        .expect("write legacy command env file");

    let _manager = TerminalManager::new(base_dir.join("terminal-sessions.json"));

    assert!(!command_env_dir.exists());
    fs::remove_dir_all(base_dir).expect("remove temporary manager directory");
}

#[cfg(any())]
mod removed_codex_command_env_tests {
    use super::*;

    #[test]
    fn codex_quick_command_detection_ignores_other_shell_commands() {
        assert!(command_launches_codex("codex"));
        assert!(command_launches_codex("/usr/local/bin/codex resume session-id"));
        assert!(!command_launches_codex("claude"));
        assert!(!command_launches_codex("echo codex"));
    }

    #[test]
    fn config_only_codex_startup_does_not_require_command_env() {
        let mut startup = CurrentApiTerminalStartup {
            codex_api_preset_name: "MiniMax".to_string(),
            codex_api_base_url: "https://example.invalid/v1".to_string(),
            codex_model: "MiniMax-M3".to_string(),
            terminal_env: Vec::new(),
            terminal_startup_script: None,
        };
        assert!(!codex_startup_requires_command_env(&startup, &[], &[]));

        startup.terminal_env = vec![("NO_PROXY".to_string(), "example.invalid".to_string())];
        assert!(codex_startup_requires_command_env(&startup, &[], &[]));
        startup.terminal_env.clear();
        startup.terminal_startup_script = Some("export CUSTOM=1".to_string());
        assert!(codex_startup_requires_command_env(&startup, &[], &[]));
        startup.terminal_startup_script = None;
        assert!(codex_startup_requires_command_env(
            &startup,
            &[("CUSTOM".to_string(), "1".to_string())],
            &[]
        ));
        assert!(codex_startup_requires_command_env(
            &startup,
            &[],
            &[("HTTP_PROXY".to_string(), "http://proxy.invalid".to_string())]
        ));
    }

    #[test]
    fn codex_quick_command_refreshes_only_when_startup_fingerprint_changes() {
        let startup = CurrentApiTerminalStartup {
            codex_api_preset_name: "MiniMax".to_string(),
            codex_api_base_url: "https://example.invalid/v1".to_string(),
            codex_model: "MiniMax-M3".to_string(),
            terminal_env: vec![("NO_PROXY".to_string(), "example.invalid".to_string())],
            terminal_startup_script: None,
        };
        let terminal_env = vec![
            ("CUSTOM".to_string(), "1".to_string()),
            ("NO_PROXY".to_string(), "example.invalid".to_string()),
        ];
        let proxy_env = vec![("HTTPS_PROXY".to_string(), "http://proxy.example:17890".to_string())];
        let fingerprint = codex_startup_fingerprint(&startup, &terminal_env, &proxy_env);
        let reordered_terminal_env = vec![
            ("NO_PROXY".to_string(), "example.invalid".to_string()),
            ("CUSTOM".to_string(), "1".to_string()),
        ];
        let reordered = codex_startup_fingerprint(&startup, &reordered_terminal_env, &proxy_env);
        let changed_proxy = codex_startup_fingerprint(
            &startup,
            &terminal_env,
            &[("HTTPS_PROXY".to_string(), "http://other-proxy.example:17890".to_string())],
        );

        assert_eq!(fingerprint, reordered);
        assert!(fingerprint.starts_with(super::CODEX_TRANSPARENT_LAUNCHER_FINGERPRINT_PREFIX));
        let legacy_fingerprint = codex_startup_fingerprint_for_session(&fingerprint, false);
        assert!(
            !legacy_fingerprint.starts_with(super::CODEX_TRANSPARENT_LAUNCHER_FINGERPRINT_PREFIX)
        );
        assert_eq!(codex_startup_fingerprint_for_session(&legacy_fingerprint, true), fingerprint);
        assert!(!should_refresh_codex_command_env(&fingerprint, &fingerprint));
        assert!(should_refresh_codex_command_env("", &fingerprint));
        assert!(should_refresh_codex_command_env(&fingerprint, &changed_proxy));
    }

    #[test]
    fn oauth_mode_does_not_apply_stale_api_terminal_preset() {
        assert!(!current_mode_uses_api_terminal_startup(auth_core::CurrentAuthMode::Auth));
        assert!(!current_mode_uses_api_terminal_startup(auth_core::CurrentAuthMode::None));
        assert!(current_mode_uses_api_terminal_startup(auth_core::CurrentAuthMode::Api));
    }

    #[test]
    fn codex_command_env_script_replaces_stale_proxy_and_quotes_values() {
        let script = render_terminal_command_env_script(
            &[
                ("HTTPS_PROXY".to_string(), "http://proxy.example:17890".to_string()),
                ("NO_PROXY".to_string(), "127.0.0.1,api.minimaxi.com".to_string()),
                ("CUSTOM_VALUE".to_string(), "a'b".to_string()),
            ],
            vec![
                "HTTPS_PROXY".to_string(),
                "HTTP_PROXY".to_string(),
                "NO_PROXY".to_string(),
                "OLD_PRESET_VALUE".to_string(),
            ],
            Some("export CODEX_RESPONSE_STYLE=compact"),
        );

        assert!(script.contains("unset HTTP_PROXY\n"), "{script}");
        assert!(script.contains("unset OLD_PRESET_VALUE\n"), "{script}");
        assert!(script.contains("export NO_PROXY='127.0.0.1,api.minimaxi.com'\n"), "{script}");
        assert!(script.contains("export CUSTOM_VALUE='a'\\''b'\n"), "{script}");
        assert!(script.ends_with("export CODEX_RESPONSE_STYLE=compact\n"), "{script}");
    }

    #[test]
    fn codex_command_env_wrapper_loads_only_the_current_terminal_file() {
        let script = codex_command_env_wrapper_script(std::path::Path::new(
            "/home/bin/webclx/.terminal-command-env",
        ));

        assert!(script.contains("WEBCLX_CODEX_COMMAND_ENV_WRAPPER"), "{script}");
        assert!(
            script.contains(
                r#"env_file='/home/bin/webclx/.terminal-command-env'/"${WEBCLX_TERMINAL_ID}.sh""#
            ),
            "{script}"
        );
        assert!(script.contains("*[!A-Za-z0-9_-]*"), "{script}");
        assert!(script.contains(r#". "$env_file""#), "{script}");
        assert!(script.contains(r#""$real_codex" "$@""#), "{script}");
        assert!(!script.contains("prepare-codex-resume"), "{script}");
        assert!(!script.contains("prepare-codex-launch"), "{script}");
        assert!(!script.contains("finalize-codex-launch"), "{script}");
        assert!(!script.contains("CODEX_HOME"), "{script}");
    }

    #[cfg(unix)]
    fn output_retrying_executable_file_busy(
        command: &mut Command,
    ) -> io::Result<std::process::Output> {
        const MAX_ATTEMPTS: usize = 10;

        for attempt in 1..=MAX_ATTEMPTS {
            match command.output() {
                Err(error)
                    if error.kind() == io::ErrorKind::ExecutableFileBusy
                        && attempt < MAX_ATTEMPTS =>
                {
                    // The build host can briefly reject a freshly installed launcher with ETXTBSY.
                    std::thread::sleep(Duration::from_millis(10));
                }
                result => return result,
            }
        }

        unreachable!("the final command attempt always returns")
    }

    #[cfg(unix)]
    #[test]
    fn codex_command_env_wrapper_sources_session_env_and_preserves_custom_wrapper() {
        use std::os::unix::fs::PermissionsExt;

        let unique = format!(
            "webclx-codex-wrapper-install-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir = std::env::temp_dir().join(unique);
        let home = temp_dir.join("home");
        let command_env_dir = temp_dir.join("command-env");
        let real_dir = temp_dir.join("real");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&command_env_dir).unwrap();
        fs::create_dir_all(&real_dir).unwrap();
        let current = test_user_profile();
        let profile = crate::runtime_paths::UserProfile {
            name: current.name,
            uid: current.uid,
            gid: current.gid,
            home: home.clone(),
            shell: current.shell,
        };
        let real_codex = real_dir.join("codex");
        fs::write(
        &real_codex,
        "#!/bin/sh\nprintf '%s|%s|%s\\n' \"${WEBCLX_TEST_PRESET:-}\" \"${WEBCLX_TEST_SOURCES:-0}\" \"$*\"\n",
    )
    .unwrap();
        fs::set_permissions(&real_codex, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
        command_env_dir.join("s-test.sh"),
        "export WEBCLX_TEST_PRESET='MiniMax3'\nWEBCLX_TEST_SOURCES=$((WEBCLX_TEST_SOURCES + 1))\nexport WEBCLX_TEST_SOURCES\n",
    )
    .unwrap();

        let launchers = ensure_codex_command_env_wrapper(&profile, &command_env_dir).unwrap();
        let wrapper = launchers.wrapper_path.expect("managed wrapper");
        assert!(launchers.transparent);
        let transparent_codex = home.join(".local/bin/codex");
        assert!(transparent_codex.exists(), "transparent Codex launcher");
        let mut transparent_command = Command::new(&transparent_codex);
        transparent_command
            .arg("--search")
            .env(
                "PATH",
                format!("{}:{}:/usr/bin", wrapper.parent().unwrap().display(), real_dir.display()),
            )
            .env("WEBCLX_TERMINAL_ID", "s-test")
            .env_remove("CODEX_HOME");
        let output = output_retrying_executable_file_busy(&mut transparent_command).unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "MiniMax3|1|--search");
        let mut dedicated_command = Command::new(&wrapper);
        dedicated_command
            .arg("--search")
            .env(
                "PATH",
                format!("{}:{}:/usr/bin", wrapper.parent().unwrap().display(), real_dir.display()),
            )
            .env("WEBCLX_TERMINAL_ID", "s-test")
            .env_remove("CODEX_HOME");
        let dedicated_output =
            output_retrying_executable_file_busy(&mut dedicated_command).unwrap();
        assert!(dedicated_output.status.success(), "{dedicated_output:?}");
        assert_eq!(String::from_utf8_lossy(&dedicated_output.stdout).trim(), "MiniMax3|1|--search");

        let custom_home = temp_dir.join("custom-home");
        let custom_bin = custom_home.join(".local/bin");
        fs::create_dir_all(&custom_bin).unwrap();
        let custom_codex = custom_bin.join("codex");
        fs::write(&custom_codex, "#!/bin/sh\necho WEBCLX_CODEX_COMMAND_ENV_WRAPPER\necho custom\n")
            .unwrap();
        fs::set_permissions(&custom_codex, fs::Permissions::from_mode(0o755)).unwrap();
        let custom_profile = crate::runtime_paths::UserProfile {
            home: custom_home.clone(),
            ..profile
        };
        let custom_launchers =
            ensure_codex_command_env_wrapper(&custom_profile, &command_env_dir).unwrap();
        let custom_launcher = custom_launchers
            .wrapper_path
            .expect("dedicated launcher should not replace custom codex");
        assert!(!custom_launchers.transparent);
        assert_eq!(custom_launcher, custom_bin.join("webclx-codex"));
        assert_eq!(
            fs::read_to_string(&custom_codex).unwrap(),
            "#!/bin/sh\necho WEBCLX_CODEX_COMMAND_ENV_WRAPPER\necho custom\n"
        );

        let occupied_launcher = custom_bin.join("webclx-codex");
        fs::write(&occupied_launcher, "#!/bin/sh\necho occupied\n").unwrap();
        assert!(
            ensure_codex_command_env_wrapper(&custom_profile, &command_env_dir)
                .unwrap()
                .wrapper_path
                .is_none()
        );
        assert_eq!(fs::read_to_string(&occupied_launcher).unwrap(), "#!/bin/sh\necho occupied\n");

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn codex_command_env_wrapper_passes_resume_through_and_reloads_current_env() {
        use std::os::unix::fs::PermissionsExt;

        let unique = format!(
            "webclx-codex-resume-wrapper-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir = std::env::temp_dir().join(unique);
        let home = temp_dir.join("home");
        let command_env_dir = temp_dir.join("command-env");
        let real_dir = temp_dir.join("real");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&command_env_dir).unwrap();
        fs::create_dir_all(&real_dir).unwrap();

        let resume_id = "019f5976-27dd-71a2-81e1-cba3279c01bc";
        let env_file = command_env_dir.join("s-resume.sh");
        fs::write(&env_file, "export WEBCLX_TEST_PRESET='new'\n").unwrap();

        let real_codex = real_dir.join("codex");
        fs::write(
        &real_codex,
        "#!/bin/sh\nprintf '%s|%s|%s\\n' \"${WEBCLX_TEST_PRESET:-none}\" \"${CODEX_HOME:-none}\" \"$*\"\n",
    )
    .unwrap();
        fs::set_permissions(&real_codex, fs::Permissions::from_mode(0o755)).unwrap();

        let current = test_user_profile();
        let profile = crate::runtime_paths::UserProfile {
            name: current.name,
            uid: current.uid,
            gid: current.gid,
            home: home.clone(),
            shell: current.shell,
        };
        let launchers = ensure_codex_command_env_wrapper(&profile, &command_env_dir).unwrap();
        let transparent_codex = launchers.transparent_path;
        let path = format!(
            "{}:{}:/usr/bin",
            transparent_codex.parent().unwrap().display(),
            real_dir.display()
        );
        let run = |args: &[&str]| {
            let mut command = Command::new(&transparent_codex);
            command
                .args(args)
                .env("PATH", &path)
                .env("WEBCLX_TERMINAL_ID", "s-resume")
                .env_remove("CODEX_HOME");
            output_retrying_executable_file_busy(&mut command).unwrap()
        };

        let first = run(&["resume", resume_id]);
        assert!(first.status.success(), "{first:?}");
        assert_eq!(
            String::from_utf8_lossy(&first.stdout).trim(),
            format!("new|none|resume {resume_id}")
        );

        fs::write(&env_file, "export WEBCLX_TEST_PRESET='latest'\n").unwrap();
        let second = run(&["resume", resume_id, "--search"]);
        assert!(second.status.success(), "{second:?}");
        assert_eq!(
            String::from_utf8_lossy(&second.stdout).trim(),
            format!("latest|none|resume {resume_id} --search")
        );

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn codex_command_env_wrapper_uses_default_codex_home_for_bare_launches() {
        use std::os::unix::fs::PermissionsExt;

        let unique = format!(
            "webclx-codex-launch-wrapper-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir = std::env::temp_dir().join(unique);
        let home = temp_dir.join("home");
        let command_env_dir = temp_dir.join("command-env");
        let real_dir = temp_dir.join("real");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&command_env_dir).unwrap();
        fs::create_dir_all(&real_dir).unwrap();

        let real_codex = real_dir.join("codex");
        fs::write(&real_codex, "#!/bin/sh\nprintf '%s|%s\\n' \"${CODEX_HOME:-none}\" \"$*\"\n")
            .unwrap();
        fs::set_permissions(&real_codex, fs::Permissions::from_mode(0o755)).unwrap();

        let current = test_user_profile();
        let profile = crate::runtime_paths::UserProfile {
            name: current.name,
            uid: current.uid,
            gid: current.gid,
            home: home.clone(),
            shell: current.shell,
        };
        let launchers = ensure_codex_command_env_wrapper(&profile, &command_env_dir).unwrap();
        let transparent_codex = launchers.transparent_path;
        let path = format!(
            "{}:{}:/usr/bin",
            transparent_codex.parent().unwrap().display(),
            real_dir.display()
        );
        let mut command = Command::new(&transparent_codex);
        command
            .arg("--version")
            .env("PATH", &path)
            .env("WEBCLX_TERMINAL_ID", "s-launch")
            .env_remove("CODEX_HOME");
        let output = output_retrying_executable_file_busy(&mut command).unwrap();

        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "none|--version");
        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn codex_command_env_wrapper_sources_current_env_with_custom_codex_home() {
        use std::os::unix::fs::PermissionsExt;

        let unique = format!(
            "webclx-codex-custom-home-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let temp_dir = std::env::temp_dir().join(unique);
        let home = temp_dir.join("home");
        let command_env_dir = temp_dir.join("command-env");
        let real_dir = temp_dir.join("real");
        let custom_codex_home = temp_dir.join("custom-codex-home");
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&command_env_dir).unwrap();
        fs::create_dir_all(&real_dir).unwrap();
        fs::create_dir_all(&custom_codex_home).unwrap();
        fs::write(command_env_dir.join("s-existing.sh"), "export WEBCLX_TEST_PRESET='latest'\n")
            .unwrap();

        let resume_id = "019f5976-27dd-71a2-81e1-cba3279c01bc";
        let real_codex = real_dir.join("codex");
        fs::write(
        &real_codex,
        "#!/bin/sh\nprintf '%s|%s|%s\\n' \"${WEBCLX_TEST_PRESET:-none}\" \"${CODEX_HOME:-none}\" \"$*\"\n",
    )
    .unwrap();
        fs::set_permissions(&real_codex, fs::Permissions::from_mode(0o755)).unwrap();

        let current = test_user_profile();
        let profile = crate::runtime_paths::UserProfile {
            name: current.name,
            uid: current.uid,
            gid: current.gid,
            home: home.clone(),
            shell: current.shell,
        };
        let launchers = ensure_codex_command_env_wrapper(&profile, &command_env_dir).unwrap();
        let transparent_codex = launchers.transparent_path;
        let path = format!(
            "{}:{}:/usr/bin",
            transparent_codex.parent().unwrap().display(),
            real_dir.display()
        );
        let mut command = Command::new(&transparent_codex);
        command
            .args(["resume", resume_id])
            .env("PATH", &path)
            .env("WEBCLX_TERMINAL_ID", "s-existing")
            .env("CODEX_HOME", &custom_codex_home);
        let output = output_retrying_executable_file_busy(&mut command).unwrap();

        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stdout).trim(),
            format!("latest|{}|resume {resume_id}", custom_codex_home.display())
        );

        fs::remove_dir_all(temp_dir).unwrap();
    }

    #[test]
    fn codex_quick_command_keeps_bare_program_with_managed_wrapper() {
        let wrapper = std::path::Path::new("/home/root/.local/bin/webclx-codex");
        assert_eq!(
            codex_command_with_env_launcher("codex --search", wrapper, true),
            Some("codex --search".to_string())
        );
        assert_eq!(
            codex_command_with_env_launcher("/usr/local/bin/codex --search", wrapper, true,),
            None
        );
        assert_eq!(
            codex_command_with_env_launcher("codex --search", wrapper, false),
            Some("'/home/root/.local/bin/webclx-codex' --search".to_string())
        );
    }

    #[test]
    fn codex_command_env_keeps_shell_no_proxy_when_application_proxy_is_active() {
        let shell_env = vec![
            ("HTTP_PROXY".to_string(), "http://old-proxy.example:7890".to_string()),
            ("NO_PROXY".to_string(), "127.0.0.1,localhost,192.168.3.2,::1".to_string()),
            ("PATH".to_string(), "/usr/local/bin:/usr/bin".to_string()),
        ];
        let effective_env = current_api_command_env(
            &shell_env,
            Vec::new(),
            Vec::new(),
            vec![("HTTP_PROXY".to_string(), "http://active-proxy.example:17890".to_string())],
        );
        let script = render_terminal_command_env_script(
            &effective_env,
            CODEX_NETWORK_ENV_KEYS
                .iter()
                .map(|key| (*key).to_string())
                .collect(),
            None,
        );

        assert!(
            script.contains("export NO_PROXY='127.0.0.1,localhost,192.168.3.2,::1'\n"),
            "{script}"
        );
        assert!(
            script.contains("export HTTP_PROXY='http://active-proxy.example:17890'\n"),
            "{script}"
        );
        assert!(!script.contains("old-proxy.example"), "{script}");
        assert!(!script.contains("export PATH="), "{script}");
    }

    #[test]
    fn codex_command_env_script_preserves_https_proxy_domain() {
        let proxy_url = "https://test-user:test-password@proxy.example.test:17891";
        let script = render_terminal_command_env_script(
            &[
                ("HTTP_PROXY".to_string(), proxy_url.to_string()),
                ("HTTPS_PROXY".to_string(), proxy_url.to_string()),
                ("ALL_PROXY".to_string(), proxy_url.to_string()),
            ],
            vec![
                "HTTP_PROXY".to_string(),
                "HTTPS_PROXY".to_string(),
                "ALL_PROXY".to_string(),
            ],
            None,
        );

        assert!(script.contains(&format!("export HTTPS_PROXY='{proxy_url}'\n")), "{script}");
        assert!(script.contains("us.fpsq.xyz:17891"), "{script}");
        assert!(!script.contains("http://proxy-user"), "{script}");
    }
}

#[test]
fn terminal_quick_command_does_not_wrap_claude_commands() {
    assert_eq!(
        build_terminal_quick_command_input(
            "export IS_SANDBOX=1; claude --dangerously-skip-permissions"
        ),
        "export IS_SANDBOX=1; claude --dangerously-skip-permissions\n"
    );
    assert_eq!(
        build_terminal_quick_command_input("unset CLAUDE_CONFIG_DIR; claude"),
        "unset CLAUDE_CONFIG_DIR; claude\n"
    );
    assert_eq!(
        build_terminal_quick_command_input(
            "hash -r 2>/dev/null || true; unset CLAUDE_CONFIG_DIR; claude"
        ),
        "hash -r 2>/dev/null || true; unset CLAUDE_CONFIG_DIR; claude\n"
    );
    assert_eq!(build_terminal_quick_command_input("hash -r; claude"), "hash -r; claude\n");
    assert_eq!(build_terminal_quick_command_input("echo claude"), "echo claude\n");
}

#[test]
fn terminal_quick_command_resolves_configured_shortcut_aliases() {
    let commands = vec![TerminalQuickCommand::new(
        "2",
        "claude",
        "export IS_SANDBOX=1;claude --dangerously-skip-permissions",
    )];

    assert_eq!(
        resolve_terminal_quick_command_input("2", &commands),
        Some("export IS_SANDBOX=1;claude --dangerously-skip-permissions")
    );
    assert_eq!(
        resolve_terminal_quick_command_input("claude", &commands),
        Some("export IS_SANDBOX=1;claude --dangerously-skip-permissions")
    );
    assert_eq!(
        resolve_terminal_quick_command_input(
            "export IS_SANDBOX=1;claude --dangerously-skip-permissions",
            &commands,
        ),
        Some("export IS_SANDBOX=1;claude --dangerously-skip-permissions")
    );
    assert_eq!(resolve_terminal_quick_command_input("codex", &commands), None);
}

#[test]
fn default_terminal_function_commands_include_reload_claude() {
    let temp_dir = std::env::temp_dir().join("webclx-terminal-function-command-defaults-test");
    std::fs::create_dir_all(&temp_dir).expect("temp dir");
    let manager = SettingsManager::load(&temp_dir).expect("settings manager");
    let response = build_settings_response(
        &manager,
        "host".to_string(),
        "listen".to_string(),
        "v".to_string(),
    )
    .expect("settings response");
    let json = serde_json::to_value(response).expect("settings json");
    let commands = json["default_terminal_function_commands"]
        .as_array()
        .expect("default function commands array");
    let reload = commands
        .iter()
        .find(|command| command["key"] == "reload_claude")
        .expect("reload claude command");
    assert_eq!(reload["label"], "重读 Claude");
    assert_eq!(reload["action"], "reload_claude");
    assert_eq!(reload["command"], "claude");
}

#[test]
fn terminal_child_env_does_not_pin_claude_config_dir() {
    let env = build_tmux_child_env(
        &[
            ("HOME".to_string(), "/home/root".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
            ("CLAUDE_CONFIG_DIR".to_string(), "/tmp/stale-claude-config".to_string()),
        ],
        &[("WEBCLX_TERMINAL_ID".to_string(), "s1".to_string())],
        &[],
    );

    assert!(env.iter().all(|(key, _)| key != "CLAUDE_CONFIG_DIR"));
    assert!(
        env.iter()
            .any(|(key, value)| key == "PATH" && value == "/usr/bin")
    );
}

#[test]
fn claude_terminal_unset_env_clears_stale_model_family_values() {
    let mut current_config_values = std::collections::BTreeMap::new();
    current_config_values.insert("ANTHROPIC_CUSTOM_HEADER".to_string(), "tenant-b".to_string());
    let current = CurrentClaudeState {
        provider_name: Some("GLM".to_string()),
        base_url: Some("http://127.0.0.1:11111/api/upstream/anthropic".to_string()),
        management_url: None,
        auth_token: Some("webclx-local-claude-proxy:claude-b".to_string()),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: Some("GLM-5.2".to_string()),
        third_party_model: None,
        config_values: current_config_values,
        preset_name: Some("GLM".to_string()),
    };
    let presets = vec![auth_core::StoredClaudePreset {
        id: "claude-a".to_string(),
        name: "MiniMax".to_string(),
        saved_at: 0,
        provider_name: "MiniMax".to_string(),
        base_url: "https://api.minimaxi.com/anthropic".to_string(),
        management_url: None,
        config_overrides: vec![auth_core::PresetConfigOverride {
            key: Some("ANTHROPIC_EXTRA_BETA".to_string()),
            value: Some("enabled".to_string()),
        }],
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth_token: "secret".to_string(),
        default_haiku_model: None,
        default_sonnet_model: None,
        default_opus_model: None,
        third_party_model: Some("MiniMax-M2.7".to_string()),
        use_local_proxy: false,
        access_mode: None,
        switch_count: 0,
    }];

    let unset_env = claude_terminal_unset_env_from_current(&current, &presets);

    assert!(unset_env.iter().any(|key| key == "ANTHROPIC_MODEL"));
    assert!(
        unset_env
            .iter()
            .any(|key| key == "ANTHROPIC_DEFAULT_OPUS_MODEL")
    );
    assert!(unset_env.iter().any(|key| key == "ANTHROPIC_EXTRA_BETA"));
    assert!(unset_env.iter().any(|key| key == "ANTHROPIC_CUSTOM_HEADER"));
}

#[test]
fn tmux_startup_script_send_path_adds_separator_as_final_guard() {
    assert_eq!(
        normalize_tmux_startup_script("export CODEX_RESPONSE_STYLE='caveman'"),
        "export CODEX_RESPONSE_STYLE='caveman';"
    );
    assert_eq!(
        normalize_tmux_startup_script("export CODEX_RESPONSE_STYLE='caveman';"),
        "export CODEX_RESPONSE_STYLE='caveman';"
    );
    assert_eq!(
        normalize_tmux_startup_script("export CODEX_RESPONSE_STYLE='caveman'\necho ready"),
        "export CODEX_RESPONSE_STYLE='caveman';\necho ready;"
    );
}

#[test]
fn terminal_error_continue_detection_only_counts_continue_after_error() {
    assert!(is_terminal_continue_line("继续"));
    assert!(is_terminal_continue_line("  › 继续"));
    assert!(is_terminal_continue_line("  ↳ 继续"));
    assert!(is_terminal_continue_line("[root@openeuler codes]# 继续"));
    assert!(is_terminal_continue_line("[root@openeuler codes]# 继续继续"));
    assert!(is_terminal_continue_line("user@host:/tmp$ 继续"));
    assert!(!is_terminal_continue_line("继续处理"));
    assert!(!is_terminal_continue_line("[root@openeuler codes]# 继续处理"));

    let already_continued_tail =
        "■ exceeded retry limit, last status: 429 Too Many Requests\n\n› 继续\n\n◦ Working";
    assert!(terminal_error_has_continue_after(already_continued_tail, 0));

    let continue_before_error_tail =
        "› 继续\n\n■ exceeded retry limit, last status: 429 Too Many Requests";
    assert!(!terminal_error_has_continue_after(continue_before_error_tail, 2));

    let new_error_after_old_continue_tail = "■ exceeded retry limit, last status: 429 Too Many Requests\n› 继续\n■ exceeded retry limit, last status: 429 Too Many Requests";
    assert!(!terminal_error_has_continue_after(new_error_after_old_continue_tail, 2));

    let queued_continue_tail =
        "■ exceeded retry limit, last status: 429 Too Many Requests\n• Messages\n  ↳ 继续";
    assert!(terminal_error_has_continue_after(queued_continue_tail, 0));
}

#[test]
fn terminal_error_queued_input_detection_only_counts_queue_after_error() {
    let queued_message_tail = "■ exceeded retry limit, last status: 429 Too Many Requests\n• Messages to be submitted at end of turn";
    assert!(terminal_error_has_queued_input_after(queued_message_tail, 0));

    let queue_before_error_tail = "• Messages to be submitted at end of turn\n■ exceeded retry limit, last status: 429 Too Many Requests";
    assert!(!terminal_error_has_queued_input_after(queue_before_error_tail, 1));
}

#[test]
fn terminal_error_matching_handles_mid_word_terminal_wraps() {
    let keyword =
        "unexpected status 502 Bad Gateway: Upstream service temporarily unavailable, url:";
    let tail = "unexpected status 502 Bad Gateway: Upstream servi\nce temporarily unavailable, url: https://example.invalid";

    assert_eq!(
        terminal_tail_error_keyword(tail, &[keyword.to_string()]).as_deref(),
        Some(keyword)
    );
}

#[test]
fn terminal_error_matching_detects_context_window_exhaustion() {
    let keyword = "ran out of room in the model's context window";
    let tail = "\u{2588} Codex ran out of room in the model's context window. Start a new thread or clear earlier history before retrying.";

    assert_eq!(
        terminal_tail_error_keyword(tail, &[keyword.to_string()]).as_deref(),
        Some(keyword)
    );
}

#[test]
fn terminal_error_matching_detects_openai_cybersecurity_block() {
    // Verbatim OpenAI cybersecurity safety-block page, including the title
    // line with the curly apostrophe and the second explanatory line.
    let tail = "This content can\'t be shown\nWe take extra caution with cybersecurity requests. If you\'re a security professional, you may be able to apply for Trusted Access.\nTrusted Access: https://openai.com/form/enterprise-trusted-access-for-cyber/\nLearn more: https://help.openai.com/en/articles/20001326";

    let matched = terminal_tail_error_keyword(
        tail,
        &[
            "This content can\'t be shown".to_string(),
            "extra caution with cybersecurity requests".to_string(),
        ],
    );
    // Either signature firing is enough to drive the auto-"继续" flow; the
    // matcher picks the latest-occurring keyword, so accept either one.
    assert!(
        matches!(
            matched.as_deref(),
            Some("This content can\'t be shown")
                | Some("extra caution with cybersecurity requests")
        ),
        "expected a cybersecurity-block signature to match, got {matched:?}"
    );
}

#[test]
fn terminal_error_matching_handles_wrapped_upstream_request_failure() {
    let keyword = "stream disconnected before completion:";
    let tail = "\u{25a0} stream disconnected before completion: Upstream\nrequest failed";

    assert_eq!(
        terminal_tail_error_keyword(tail, &[keyword.to_string()]).as_deref(),
        Some(keyword)
    );
}

#[test]
fn terminal_error_matching_ignores_nonfatal_mcp_startup_failure() {
    let tail = "⚠ MCP client for `openchatcut` failed to start: MCP startup failed: handshaking with MCP server failed: Send message error Transport\n\
[rmcp::transport::worker::WorkerTransport<rmcp::transport::streamable_http_client::StreamableHttpClientWorker<codex_rmcp_client::http_client_adapter::StreamableHttpClientAdapter>>] error: Client error: HTTP\n\
request failed: http/request failed: error sending request for url (http://localhost:5199/api/external-mcp/mcp), when send initialize request\n\
\n\
⚠ MCP startup incomplete (failed: openchatcut)\n\
\n\
›";

    assert_eq!(
        terminal_tail_error_keyword(
            tail,
            &[
                "sending request for url".to_string(),
                "MCP startup incomplete".to_string(),
            ],
        ),
        None
    );
}

#[test]
fn terminal_error_matching_ignores_stale_error_after_completion() {
    let keyword = "last status: 429";
    let tail = "- node tests/terminal-session-details.test.mjs\n\
- cargo check 通过，但保留已有 remote_url unused warnings。\n\
\n\
已按项目规则提交 webClx 编译/部署队列：queued: true，请求 ID 1782006184974553954-2179935。\n\
\n\
■ exceeded retry limit, last status: 429 Too Many Requests\n\
\n\
─ Worked for 7m 28s ───────────────────────────────────────────────────────────\n\
\n\
\n\
› 继续";

    assert_eq!(terminal_tail_error_keyword(tail, &[keyword.to_string()]).as_deref(), None);
}

#[test]
fn terminal_error_matching_ignores_stale_error_after_manual_interruption() {
    let keyword = "last status: 429";
    let interrupted_tail = "■ exceeded retry limit, last status: 429 Too Many Requests\n\
\n\
Conversation interrupted - tell the model what\n\
to do differently. Something went wrong? Hit\n\
`/feedback` to report the issue.";

    assert_eq!(
        terminal_tail_error_keyword(interrupted_tail, &[keyword.to_string()]).as_deref(),
        None
    );
    assert_eq!(
        terminal_tail_error_keyword_with_manual_interrupt_policy(
            interrupted_tail,
            &[keyword.to_string()],
            false,
        )
        .as_deref(),
        Some(keyword)
    );

    let later_error_tail = format!(
        "{interrupted_tail}\n\
\n\
› retry this request\n\
\n\
■ exceeded retry limit, last status: 429 Too Many Requests"
    );
    assert_eq!(
        terminal_tail_error_keyword(&later_error_tail, &[keyword.to_string()]).as_deref(),
        Some(keyword)
    );
}

#[test]
fn terminal_auto_continue_cooldown_blocks_until_interval_expires() {
    assert_eq!(auto_continue_retry_at_millis(None, 60_000, 30_000), None);
    assert_eq!(auto_continue_retry_at_millis(Some(1_000), 60_000, 30_000), Some(61_000));
    assert_eq!(auto_continue_retry_at_millis(Some(1_000), 60_000, 61_000), None);
}

#[test]
fn terminal_auto_continue_backoff_grows_by_factor() {
    let base = 60_000u64; // 60s
    let f = 1.5;
    let max = 1_200_000;
    // attempt 1 -> base
    assert_eq!(auto_continue_backoff_interval_millis(base, 1, f, max), 60_000);
    // attempt 2 -> 90s (60 * 1.5)
    assert_eq!(auto_continue_backoff_interval_millis(base, 2, f, max), 90_000);
    // attempt 3 -> 135s (60 * 1.5^2)
    assert_eq!(auto_continue_backoff_interval_millis(base, 3, f, max), 135_000);
    // attempt 4 -> 202.5s rounded
    assert_eq!(auto_continue_backoff_interval_millis(base, 4, f, max), 202_500);
}

#[test]
fn terminal_auto_continue_backoff_caps_at_configured_max() {
    let base = 60_000u64;
    let f = 1.5;
    let max = 120_000u64;
    assert_eq!(auto_continue_backoff_interval_millis(base, 3, f, max), max);
    // Never exceeds cap even at huge attempt counts.
    assert_eq!(auto_continue_backoff_interval_millis(base, 50, f, max), max);
}

#[test]
fn terminal_auto_continue_backoff_first_attempt_is_base() {
    assert_eq!(auto_continue_backoff_interval_millis(5_000, 0, 1.5, 120_000), 5_000);
    assert_eq!(auto_continue_backoff_interval_millis(5_000, 1, 1.5, 120_000), 5_000);
}

#[test]
fn terminal_auto_continue_backoff_factor_two() {
    let base = 60_000u64;
    // factor 2.0 behaves like classic exponential: 60,120,240
    assert_eq!(auto_continue_backoff_interval_millis(base, 1, 2.0, 1_200_000), 60_000);
    assert_eq!(auto_continue_backoff_interval_millis(base, 2, 2.0, 1_200_000), 120_000);
    assert_eq!(auto_continue_backoff_interval_millis(base, 3, 2.0, 1_200_000), 240_000);
}

#[test]
fn terminal_auto_continue_backoff_factor_below_one_is_treated_as_one() {
    // A sub-unit factor must not shrink below the base interval.
    assert_eq!(auto_continue_backoff_interval_millis(60_000, 5, 0.5, 120_000), 60_000);
    assert_eq!(auto_continue_backoff_interval_millis(60_000, 5, 1.0, 120_000), 60_000);
}

#[test]
fn terminal_auto_continue_backoff_cap_never_shortens_base_interval() {
    assert_eq!(auto_continue_backoff_interval_millis(300_000, 5, 1.5, 120_000), 300_000);
}

#[test]
fn terminal_error_matching_ignores_stale_quota_when_no_429_near_current_prompt() {
    let keyword = "last status: 429";
    let tail = "■ exceeded retry limit, last status: 429 Too Many Requests\n\
\n\
─ Worked for 7m 28s ───────────────────────────────────────────────────────────\n\
\n\
❯ 继续\n\
  ⎿  Not logged in · Please run /login\n\
\n\
✻ Baked for 0s\n\
❯ 继续\n\
  ⎿  Not logged in · Please run /login";

    assert_eq!(terminal_tail_error_keyword(tail, &[keyword.to_string()]).as_deref(), None);
}

#[test]
fn terminal_error_reset_time_extracts_configured_time_placeholder() {
    let tail = "API Error: Request rejected (429) · [1308][已达到 5 小时的使用上限。您的限额将在 2026-06-18 04:32:41 重置。][20260618011147c8a2aacdaa584bb7]";
    let patterns = vec!["限额将在 {time} 重置".to_string()];

    assert_eq!(
        terminal_error_reset_time_from_tail(tail, &patterns).as_deref(),
        Some("2026-06-18 04:32:41")
    );
}

#[test]
fn terminal_error_reset_time_extracts_wrapped_quota_time() {
    let tail = "API Error: Request rejected (429) ·\n  [1308][已达到 5\n  小时的使用上限。您的限额将在 2026-06-21\n  10:27:11\n  重置。][20260621055804918203fdfa0741e1]";
    let patterns = vec!["限额将在 {time} 重置".to_string()];

    assert_eq!(
        terminal_error_reset_time_from_tail(tail, &patterns).as_deref(),
        Some("2026-06-21 10:27:11")
    );
}

#[test]
fn terminal_auto_continue_due_is_one_minute_after_reset_time() {
    let reset_at = "2099-06-21 10:27:11";
    let reset_epoch = terminal_reset_time_epoch_millis(reset_at).expect("reset time should parse");

    assert_eq!(
        terminal_auto_continue_due_millis(reset_at, reset_epoch.saturating_sub(1000)),
        Some(reset_epoch + 60_000)
    );
    assert_eq!(terminal_auto_continue_due_millis(reset_at, reset_epoch + 60_001), None);
}

#[test]
fn terminal_auto_continue_schedules_retrying_quota_error_with_prior_continue() {
    let reset_at = "2099-06-21 10:27:11";
    let sessions = vec![super::TerminalSessionInfo {
        id: "s429".to_string(),
        name: "glm_retry".to_string(),
        title: None,
        user_name: default_terminal_user_name(),
        codex_api_preset_name: "GLM".to_string(),
        codex_api_base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        path: "/tmp/workspace".to_string(),
        display_path: "/tmp/workspace".to_string(),
        alive: true,
        connected: true,
        busy: true,
        activity_state: "retrying".to_string(),
        activity_label: "重试中".to_string(),
        activity_agent: Some("Codex".to_string()),
        activity_error_keyword: Some("last status: 429".to_string()),
        activity_error_signature: Some("quota-signature".to_string()),
        activity_error_continue_sent: true,
        activity_error_input_queued: false,
        activity_error_auto_continue_at: Some(reset_at.to_string()),
        last_output_at: 1,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
    }];

    let schedules = collect_terminal_auto_continue_schedules(&sessions, 0);

    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].0, "s429");
    assert_eq!(schedules[0].1.reset_at, reset_at);
    assert_eq!(schedules[0].1.signature, "quota-signature");
}

#[test]
fn zhipu_quota_reset_backfill_uses_base_url_for_session_list_schedules() {
    let (manager, base_dir) = build_test_terminal_manager();
    let reset_at = "2099-06-21 10:27:11";
    manager.quota_reset_cache.record_for_preset(
        "api-primary",
        "https://open.bigmodel.cn/api/coding/paas/v4",
        reset_at.to_string(),
    );
    manager.update_api_preset_snapshot(vec![
        StoredApiPreset {
            id: "api-primary".to_string(),
            name: "智谱5.2 ZCODE API".to_string(),
            saved_at: 1,
            provider_name: String::new(),
            base_url: "https://open.bigmodel.cn/api/coding/paas/v4".to_string(),
            management_url: None,
            wire_api: None,
            responses_proxy: None,
            apply_upstream_proxy_on_switch: false,
            config_overrides: Vec::new(),
            legacy_config_key: None,
            legacy_config_value: None,
            legacy_secondary_config_key: None,
            legacy_secondary_config_value: None,
            terminal_env: Vec::new(),
            terminal_startup_script: None,
            api_key: String::new(),
            access_token: String::new(),
            account_id: String::new(),
            access_mode: None,
            switch_count: 0,
        },
        StoredApiPreset {
            id: "api-1m".to_string(),
            name: "智谱5.2 ZCODE API 1M".to_string(),
            saved_at: 2,
            provider_name: String::new(),
            base_url: "https://open.bigmodel.cn/api/coding/paas/v4".to_string(),
            management_url: None,
            wire_api: None,
            responses_proxy: None,
            apply_upstream_proxy_on_switch: false,
            config_overrides: Vec::new(),
            legacy_config_key: None,
            legacy_config_value: None,
            legacy_secondary_config_key: None,
            legacy_secondary_config_value: None,
            terminal_env: Vec::new(),
            terminal_startup_script: None,
            api_key: String::new(),
            access_token: String::new(),
            account_id: String::new(),
            access_mode: None,
            switch_count: 0,
        },
    ]);
    let mut sessions = vec![super::TerminalSessionInfo {
        id: "s2018".to_string(),
        name: "ZCode_1".to_string(),
        title: None,
        user_name: default_terminal_user_name(),
        codex_api_preset_name: "智谱5.2 ZCODE API 1M".to_string(),
        codex_api_base_url: "https://open.bigmodel.cn/api/coding/paas/v4".to_string(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        path: "/home/third_party/ZCode".to_string(),
        display_path: "/home/third_party/ZCode".to_string(),
        alive: true,
        connected: true,
        busy: true,
        activity_state: "error".to_string(),
        activity_label: "错误".to_string(),
        activity_agent: Some("Codex".to_string()),
        activity_error_keyword: Some("429 Too Many Requests".to_string()),
        activity_error_signature: Some("quota-signature".to_string()),
        activity_error_continue_sent: false,
        activity_error_input_queued: false,
        activity_error_auto_continue_at: None,
        last_output_at: 1,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
    }];

    manager.backfill_zhipu_quota_reset_times_for_sessions(&mut sessions);
    let schedules = collect_terminal_auto_continue_schedules(&sessions, 0);

    assert_eq!(sessions[0].activity_error_auto_continue_at.as_deref(), Some(reset_at));
    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].0, "s2018");
    assert_eq!(schedules[0].1.reset_at, reset_at);

    let _ = fs::remove_dir_all(base_dir);
}

#[test]
fn terminal_auto_continue_cron_entry_preserves_seconds_with_sleep() {
    let reset_at = "2099-06-21 10:27:11";
    let due_at = terminal_auto_continue_due_millis(reset_at, 0).expect("due time should parse");
    let entry = terminal_auto_continue_cron_entry(due_at).expect("cron entry should format");

    assert_eq!(entry.fields.split_whitespace().count(), 5);
    assert_eq!(entry.sleep_seconds, 11);
}

#[test]
fn terminal_auto_continue_task_parser_lists_crontab_entries() {
    let crontab = "\
# webclx-auto-continue:s1518:acb84824e04ff4d6
28 10 21 06 * '/home/bin/webclx/terminal-auto-continue-cron/s1518-acb84824e04ff4d6.sh' # webclx-auto-continue:s1518:acb84824e04ff4d6
# unrelated-comment
* * * * * /tmp/unrelated.sh
";

    let tasks = parse_terminal_auto_continue_tasks_from_crontab(crontab);

    assert_eq!(tasks.len(), 1);
    assert!(tasks[0].marker.starts_with("webclx-auto-continue:s1518:"));
    assert_eq!(tasks[0].session_id, "s1518");
    assert_eq!(tasks[0].webclx_terminal_name, None);
    assert_eq!(tasks[0].tmux_session_name, "webclx_s1518");
    assert_eq!(tasks[0].signature, "acb84824e04ff4d6");
    assert_eq!(tasks[0].schedule, "28 10 21 06 *");
    assert_eq!(
        tasks[0].script_path.as_deref(),
        Some("/home/bin/webclx/terminal-auto-continue-cron/s1518-acb84824e04ff4d6.sh")
    );
}

#[test]
fn terminal_task_parser_lists_delayed_paste_crontab_entries() {
    // The parser currently only handles webclx-auto-continue markers.
    // Test with that marker prefix.
    let crontab = "\
# webclx-auto-continue:s2048:paste9ab
05 22 21 06 * '/home/bin/webclx/terminal-auto-continue-cron/s2048-paste9ab.sh' # webclx-auto-continue:s2048:paste9ab
";

    let tasks = parse_terminal_auto_continue_tasks_from_crontab(crontab);

    assert_eq!(tasks.len(), 1);
    assert!(tasks[0].marker.starts_with("webclx-auto-continue:s2048:"));
    assert_eq!(tasks[0].session_id, "s2048");
    assert_eq!(tasks[0].tmux_session_name, "webclx_s2048");
    assert_eq!(tasks[0].signature, "paste9ab");
    assert_eq!(tasks[0].schedule, "05 22 21 06 *");
    assert_eq!(
        tasks[0].script_path.as_deref(),
        Some("/home/bin/webclx/terminal-auto-continue-cron/s2048-paste9ab.sh")
    );
}

#[test]
fn terminal_error_matching_handles_selected_model_capacity_wraps() {
    let keyword = "Selected model is at capacity. Please try a different model.";
    let tail = "⚠ Selected model is at capacity. Please try a\n  different model.";

    assert_eq!(
        terminal_tail_error_keyword(tail, &[keyword.to_string()]).as_deref(),
        Some(keyword)
    );
}

#[test]
fn terminal_working_status_detection_matches_codex_status_line() {
    assert!(terminal_tail_has_working_status(
        "› Read file\n\n• Working (5m 31s • esc to interrupt)"
    ));
    assert!(terminal_tail_has_working_status("• working (12s • esc to interrupt)"));
    assert!(!terminal_tail_has_working_status("Working directory: /tmp/project"));
    assert!(!terminal_tail_has_working_status("working... 82% context left"));
}

#[test]
fn terminal_working_status_detection_matches_claude_spinner() {
    // Claude Code active spinner: gerund + U+2026 ellipsis.
    assert!(terminal_tail_has_working_status("✻ Thinking…"));
    assert!(terminal_tail_has_working_status("✻ Pondering…"));
    assert!(terminal_tail_has_working_status("✻ Cogitating…"));
    assert!(terminal_tail_has_working_status("✻ Tallying tokens…"));
    assert!(terminal_tail_has_working_status("✻ Searching the web…"));
    // Trailing token-context parenthetical must not mask the gerund.
    assert!(terminal_tail_has_working_status(
        "✻ Acquiring optimized context… (8.2k tokens · 5% context)"
    ));
    assert!(terminal_tail_has_working_status(
        "· 执行 ui.rs 拆分… (41m 35s · ↑ 116.5k tokens)"
    ));
    assert!(terminal_tail_has_working_status("● 读取文件… (12s · ↓ 3.1k tokens)"));
    // ASCII ellipsis form when a gerund directly precedes it.
    assert!(terminal_tail_has_working_status("✻ Thinking..."));

    // Completed / idle Claude lines must not be treated as working.
    assert!(!terminal_tail_has_working_status("✻ Churned for 53s"));
    assert!(!terminal_tail_has_working_status("❯"));
    assert!(!terminal_tail_has_working_status("working... 82% context left"));
}

#[test]
fn terminal_worked_status_detection_matches_codex_completion_line() {
    assert!(terminal_tail_has_worked_status("› Update code\n\n✓ Worked for 7m 10s"));
    assert!(terminal_tail_has_worked_status("Worked for 42s"));
    assert!(terminal_tail_has_worked_status("• Worked for 1h 2m 3s"));
    // Codex pads the line with a trailing box-drawing rule.
    assert!(terminal_tail_has_worked_status(
        "─ Worked for 3m 14s ─────────────────────────────"
    ));
    assert!(!terminal_tail_has_worked_status("I worked for 7m 10s yesterday"));
    assert!(!terminal_tail_has_worked_status("Working (5m 31s • esc to interrupt)"));
}

#[test]
fn terminal_worked_status_detection_matches_claude_completion_line() {
    // Claude Code prints `✻ <verb> for <duration>` with a varying verb.
    assert!(terminal_tail_has_worked_status("✻ Churned for 53s"));
    assert!(terminal_tail_has_worked_status("✻ Cogitated for 1m 2s"));
    assert!(terminal_tail_has_worked_status("  Worked for 7m 10s  "));
    // Trailing prose must disqualify the match.
    assert!(!terminal_tail_has_worked_status("✻ Churned for 53s yesterday"));
    assert!(!terminal_tail_has_worked_status("3m 14s"));
    assert!(!terminal_tail_has_worked_status("❯"));
}

#[test]
fn duplicate_manual_name_is_rejected_for_other_sessions() {
    let workspace_dir = std::path::PathBuf::from("/tmp/workspace");
    let mut state = TerminalState::default();
    state.sessions_by_id.insert(
        "s1".to_string(),
        StoredTerminalSession {
            id: "s1".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: workspace_dir,
            user_name: default_terminal_user_name(),
            name: "custom".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: true,
            idle: false,
            created_at: 2,
            last_opened_at: 2,
        },
    );

    let error = ensure_unique_session_name_locked(&state, "workspace_1", Some("s2"))
        .expect_err("duplicate name should be rejected");
    assert!(error.to_string().contains("已存在"), "unexpected error: {error}");
    ensure_unique_session_name_locked(&state, "workspace_1", Some("s1"))
        .expect("same session should keep its current name");
}

#[test]
fn duplicate_manual_auto_index_is_rejected_for_other_sessions() {
    let workspace_dir = std::path::PathBuf::from("/tmp/workspace");
    let mut state = TerminalState::default();
    state.sessions_by_id.insert(
        "s1".to_string(),
        StoredTerminalSession {
            id: "s1".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1_新想法".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: true,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: workspace_dir,
            user_name: default_terminal_user_name(),
            name: "custom".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: true,
            idle: false,
            created_at: 2,
            last_opened_at: 2,
        },
    );

    let error = ensure_unique_session_name_locked(&state, "workspace_1", Some("s2"))
        .expect_err("duplicate auto index should be rejected");
    assert!(error.to_string().contains("编号"), "unexpected error: {error}");
}

#[test]
fn recent_opened_sessions_sort_first() {
    let path = std::path::PathBuf::from("/tmp/workspace");
    let mut state = TerminalState::default();
    state.sessions_by_id.insert(
        "s1".to_string(),
        StoredTerminalSession {
            id: "s1".to_string(),
            path: path.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 10,
            last_opened_at: 20,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: path.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_2".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 11,
            last_opened_at: 40,
        },
    );
    state.sessions_by_id.insert(
        "s3".to_string(),
        StoredTerminalSession {
            id: "s3".to_string(),
            path,
            user_name: default_terminal_user_name(),
            name: "workspace_3".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 30,
            last_opened_at: 40,
        },
    );

    let mut session_ids = vec!["s1".to_string(), "s2".to_string(), "s3".to_string()];
    sort_session_ids_by_recent_activity(&state, &mut session_ids);
    assert_eq!(session_ids, vec!["s3", "s2", "s1"]);
}

#[test]
fn opening_an_idle_session_does_not_restore_it() {
    let mut state = TerminalState::default();
    state.sessions_by_id.insert(
        "s1".to_string(),
        StoredTerminalSession {
            id: "s1".to_string(),
            path: std::path::PathBuf::from("/tmp/workspace"),
            user_name: default_terminal_user_name(),
            name: "workspace_1".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: true,
            created_at: 1,
            last_opened_at: 1,
        },
    );

    assert!(super::manager::mark_session_opened_locked(&mut state, "s1"));

    let session = state
        .sessions_by_id
        .get("s1")
        .expect("session should remain");
    assert!(session.idle, "opening must not bypass explicit idle restore");
    assert!(session.last_opened_at > 1);
}

#[test]
fn activity_snapshot_can_carry_agent_without_agent_state() {
    let activity =
        TerminalActivitySnapshot::recent_output(42).with_agent(Some("Claude".to_string()));

    assert_eq!(activity.state, "recent_output");
    assert_eq!(activity.label, "输出中");
    assert_eq!(activity.agent.as_deref(), Some("Claude"));
}

#[test]
fn collect_session_infos_updates_title_without_structural_notify() {
    let workspace_dir = std::path::PathBuf::from("/tmp/webclx-terminal-title-test");
    let stored = StoredTerminalSession {
        id: "s1".to_string(),
        path: workspace_dir.clone(),
        user_name: default_terminal_user_name(),
        name: "workspace_1".to_string(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
    };
    let live_session = build_live_session(&stored);
    live_session
        .name_state
        .write()
        .expect("terminal session name poisoned")
        .update_title("cargo test".to_string());

    let mut state = TerminalState::default();
    state
        .sessions_by_id
        .insert(stored.id.clone(), stored.clone());
    state
        .sessions_by_path
        .insert(workspace_dir.clone(), vec![stored.id.clone()]);
    state.live_sessions.insert(stored.id.clone(), live_session);

    let (sessions, updated) = collect_session_infos_locked(
        &mut state,
        &workspace_dir,
        &workspace_dir,
        vec![stored.id.clone()],
        80,
        &[],
        &[],
        true,
    );

    assert_eq!(sessions.len(), 1);
    assert!(updated);
    assert_eq!(sessions[0].title.as_deref(), Some("cargo test"));
    assert_eq!(
        state
            .sessions_by_id
            .get(&stored.id)
            .map(|item| item.title.as_str()),
        Some("cargo test")
    );
    assert!(!should_notify_session_list_sync(false, updated));
    assert!(should_notify_session_list_sync(true, updated));
}

#[test]
fn collect_session_infos_applies_precomputed_activity_probe() {
    let workspace_dir = std::path::PathBuf::from("/tmp/webclx-terminal-probe-test");
    let stored = StoredTerminalSession {
        id: "s1".to_string(),
        path: workspace_dir.clone(),
        user_name: default_terminal_user_name(),
        name: "workspace_1".to_string(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
    };
    let live_session = build_live_session(&stored);
    live_session
        .name_state
        .write()
        .expect("terminal session name poisoned")
        .update_title("cargo test".to_string());

    let mut state = TerminalState::default();
    state
        .sessions_by_id
        .insert(stored.id.clone(), stored.clone());
    state
        .sessions_by_path
        .insert(workspace_dir.clone(), vec![stored.id.clone()]);
    state
        .live_sessions
        .insert(stored.id.clone(), live_session.clone());

    let probes = vec![TerminalActivityProbe {
        session_id: stored.id.clone(),
        live_last_output_at: 42,
        snapshot_fingerprint: None,
        snapshot_probe_sequence: 1,
        agent_activity: TerminalAgentActivity::default(),
        working_status: false,
        error_match: None,
        worked_status: false,
        pending_build: false,
    }];
    let (sessions, updated) = collect_session_infos_from_probes_locked(
        &mut state,
        &workspace_dir,
        &workspace_dir,
        probes,
    );

    assert!(updated);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].title.as_deref(), Some("cargo test"));
    assert_eq!(sessions[0].activity_state, "completed");
    assert_eq!(sessions[0].last_output_at, 42);
}

#[test]
fn pending_build_overrides_unviewed_agent_completion() {
    let workspace_dir = std::path::PathBuf::from("/tmp/webclx-terminal-pending-build-test");
    let stored = StoredTerminalSession {
        id: "s1".to_string(),
        path: workspace_dir.clone(),
        user_name: default_terminal_user_name(),
        name: "workspace_1".to_string(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
    };
    let mut state = TerminalState::default();
    state
        .sessions_by_id
        .insert(stored.id.clone(), stored.clone());
    state
        .sessions_by_path
        .insert(workspace_dir.clone(), vec![stored.id.clone()]);

    let probes = vec![TerminalActivityProbe {
        session_id: stored.id.clone(),
        live_last_output_at: 42,
        snapshot_fingerprint: None,
        snapshot_probe_sequence: 1,
        agent_activity: TerminalAgentActivity::default(),
        working_status: false,
        error_match: None,
        worked_status: true,
        pending_build: true,
    }];
    let (sessions, _) = collect_session_infos_from_probes_locked(
        &mut state,
        &workspace_dir,
        &workspace_dir,
        probes,
    );

    assert_eq!(sessions[0].activity_state, "building");
    assert_eq!(sessions[0].activity_label, "编译中");
    assert!(sessions[0].busy);
}

#[test]
fn pending_build_registry_survives_restart_and_discards_stale_requests() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let registry_file =
        std::env::temp_dir().join(format!("webclx-terminal-pending-build-registry-{unique}.json"));
    let now_millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_millis() as u64;
    let registry = StoredTerminalPendingBuildRegistry {
        requests: vec![
            TerminalPendingBuildRequest {
                request_id: "fresh-request".to_string(),
                session_id: "s1".to_string(),
                queued_at_millis: now_millis,
            },
            TerminalPendingBuildRequest {
                request_id: "stale-request".to_string(),
                session_id: "s2".to_string(),
                queued_at_millis: now_millis.saturating_sub(TERMINAL_PENDING_BUILD_MAX_AGE_MS + 1),
            },
        ],
    };

    persist_terminal_pending_build_registry(&registry_file, &registry)
        .expect("pending build registry should persist");
    let loaded = load_terminal_pending_build_requests(&registry_file);

    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded["fresh-request"].session_id, "s1");
    assert!(!loaded.contains_key("stale-request"));
    fs::remove_file(&registry_file).expect("pending build test registry should be removable");
}

#[test]
fn unchanged_pane_snapshot_does_not_restore_viewed_output_to_completed() {
    let workspace_dir = std::path::PathBuf::from("/tmp/webclx-terminal-output-redraw-test");
    let stored = StoredTerminalSession {
        id: "s1".to_string(),
        path: workspace_dir.clone(),
        user_name: default_terminal_user_name(),
        name: "workspace_1".to_string(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
    };

    let mut state = TerminalState::default();
    state
        .sessions_by_id
        .insert(stored.id.clone(), stored.clone());
    state
        .sessions_by_path
        .insert(workspace_dir.clone(), vec![stored.id.clone()]);

    let initial_probe = TerminalActivityProbe {
        session_id: stored.id.clone(),
        live_last_output_at: 100,
        snapshot_fingerprint: Some(7),
        snapshot_probe_sequence: 1,
        agent_activity: TerminalAgentActivity::default(),
        working_status: false,
        error_match: None,
        worked_status: false,
        pending_build: false,
    };
    let _ = collect_session_infos_from_probes_locked(
        &mut state,
        &workspace_dir,
        &workspace_dir,
        vec![initial_probe],
    );
    state
        .output_observations
        .get_mut(&stored.id)
        .expect("output observation should exist")
        .last_viewed_output_at = 100;

    let idle_redraw_probe = TerminalActivityProbe {
        session_id: stored.id.clone(),
        live_last_output_at: 200,
        snapshot_fingerprint: Some(7),
        snapshot_probe_sequence: 2,
        agent_activity: TerminalAgentActivity::default(),
        working_status: false,
        error_match: None,
        worked_status: false,
        pending_build: false,
    };
    let (sessions, _) = collect_session_infos_from_probes_locked(
        &mut state,
        &workspace_dir,
        &workspace_dir,
        vec![idle_redraw_probe],
    );

    assert_eq!(sessions[0].activity_state, "idle");
    assert_eq!(sessions[0].last_output_at, 100);
}

#[test]
fn restored_session_redraw_rebaselines_without_pending_output() {
    let workspace_dir = std::path::PathBuf::from("/tmp/webclx-terminal-restore-redraw-test");
    let stored = StoredTerminalSession {
        id: "s1".to_string(),
        path: workspace_dir.clone(),
        user_name: default_terminal_user_name(),
        name: "workspace_1".to_string(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
    };

    let mut state = TerminalState::default();
    state
        .sessions_by_id
        .insert(stored.id.clone(), stored.clone());
    state
        .sessions_by_path
        .insert(workspace_dir.clone(), vec![stored.id.clone()]);
    state.output_observations.insert(
        stored.id.clone(),
        super::TerminalOutputObservation {
            fingerprint: Some(7),
            last_fingerprint_probe_sequence: 0,
            rebaseline_after_restore: false,
            last_output_at: 100,
            last_viewed_output_at: 100,
        },
    );
    assert!(!prepare_restored_output_observation_locked(&mut state, &stored.id, Some(8), 1,));

    let restored_redraw_probe = TerminalActivityProbe {
        session_id: stored.id.clone(),
        // Attaching the replacement PTY produces redraw bytes and advances the
        // live clock even though the tmux pane has no new user-visible output.
        live_last_output_at: 200,
        snapshot_fingerprint: Some(8),
        snapshot_probe_sequence: 2,
        agent_activity: TerminalAgentActivity::default(),
        working_status: false,
        error_match: None,
        worked_status: false,
        pending_build: false,
    };
    let (sessions, _) = collect_session_infos_from_probes_locked(
        &mut state,
        &workspace_dir,
        &workspace_dir,
        vec![restored_redraw_probe],
    );

    assert_eq!(sessions[0].activity_state, "idle");
    assert_eq!(sessions[0].last_output_at, 100);
    let restored_observation = state
        .output_observations
        .get(&stored.id)
        .expect("restored observation should remain available");
    assert_eq!(restored_observation.fingerprint, Some(8));
    assert_eq!(restored_observation.last_output_at, 100);
    assert_eq!(restored_observation.last_viewed_output_at, 100);

    let real_output_probe = TerminalActivityProbe {
        session_id: stored.id.clone(),
        live_last_output_at: 300,
        snapshot_fingerprint: Some(9),
        snapshot_probe_sequence: 3,
        agent_activity: TerminalAgentActivity::default(),
        working_status: false,
        error_match: None,
        worked_status: false,
        pending_build: false,
    };
    let (sessions, _) = collect_session_infos_from_probes_locked(
        &mut state,
        &workspace_dir,
        &workspace_dir,
        vec![real_output_probe],
    );

    assert_eq!(sessions[0].activity_state, "recent_output");
    assert!(sessions[0].last_output_at > 100);
}

#[test]
fn deferred_restore_arms_observations_before_browser_reconnect() {
    let mut state = TerminalState::default();
    state.output_observations.insert(
        "s1".to_string(),
        super::TerminalOutputObservation {
            fingerprint: Some(7),
            last_fingerprint_probe_sequence: 0,
            rebaseline_after_restore: false,
            last_output_at: 100,
            last_viewed_output_at: 100,
        },
    );

    arm_output_observations_for_restore_locked(&mut state);

    let observation = state.output_observations.get("s1").unwrap();
    assert!(observation.rebaseline_after_restore);
    assert_eq!(observation.last_output_at, 100);
    assert_eq!(observation.last_viewed_output_at, 100);
}

#[test]
fn shutdown_redraw_rebaseline_preserves_viewed_output_timestamps() {
    let mut state = TerminalState::default();
    state.output_observations.insert(
        "s1".to_string(),
        super::TerminalOutputObservation {
            fingerprint: Some(7),
            last_fingerprint_probe_sequence: 3,
            rebaseline_after_restore: false,
            last_output_at: 100,
            last_viewed_output_at: 100,
        },
    );

    rebaseline_terminal_output_locked(&mut state, "s1", Some(8), 4);

    let observation = state
        .output_observations
        .get("s1")
        .expect("shutdown rebaseline should retain the observation");
    assert_eq!(observation.fingerprint, Some(8));
    assert_eq!(observation.last_fingerprint_probe_sequence, 4);
    assert_eq!(observation.last_output_at, 100);
    assert_eq!(observation.last_viewed_output_at, 100);
}

#[test]
fn system_preset_probe_restores_original_activity_timestamps() {
    let mut state = TerminalState::default();
    let before = super::TerminalOutputObservation {
        fingerprint: Some(7),
        last_fingerprint_probe_sequence: 3,
        rebaseline_after_restore: false,
        last_output_at: 100,
        last_viewed_output_at: 40,
    };

    restore_system_probe_output_observation_locked(&mut state, "s1", Some(&before), Some(8), 9);

    let observation = state
        .output_observations
        .get("s1")
        .expect("system probe should retain an output observation");
    assert_eq!(observation.fingerprint, Some(8));
    assert_eq!(observation.last_fingerprint_probe_sequence, 9);
    assert_eq!(observation.last_output_at, 100);
    assert_eq!(observation.last_viewed_output_at, 40);
}

#[test]
fn first_system_preset_probe_stays_viewed_after_rebaseline() {
    let mut state = TerminalState::default();

    restore_system_probe_output_observation_locked(&mut state, "s1", None, Some(8), 9);

    let observation = state
        .output_observations
        .get("s1")
        .expect("system probe should establish an output baseline");
    assert_eq!(observation.fingerprint, Some(8));
    assert_eq!(observation.last_output_at, 0);
    assert_eq!(observation.last_viewed_output_at, 0);
}

#[test]
fn refresh_auto_names_avoids_manual_name_collisions() {
    let workspace_dir = std::path::PathBuf::from("/tmp/workspace");
    let mut state = TerminalState::default();
    state.sessions_by_id.insert(
        "s1".to_string(),
        StoredTerminalSession {
            id: "s1".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: true,
            idle: false,
            created_at: 2,
            last_opened_at: 2,
        },
    );
    state
        .sessions_by_path
        .insert(workspace_dir.clone(), vec!["s1".to_string(), "s2".to_string()]);

    let dirty = refresh_auto_session_names_for_path_locked(&mut state, &workspace_dir);

    assert!(dirty);
    assert_eq!(
        state
            .sessions_by_id
            .get("s1")
            .map(|session| session.name.as_str()),
        Some("workspace_2")
    );
    assert_eq!(
        state
            .sessions_by_id
            .get("s2")
            .map(|session| session.name.as_str()),
        Some("workspace_1")
    );
}

#[test]
fn refresh_auto_names_avoids_manual_auto_index_collisions() {
    let workspace_dir = std::path::PathBuf::from("/tmp/workspace");
    let mut state = TerminalState::default();
    state.sessions_by_id.insert(
        "s1".to_string(),
        StoredTerminalSession {
            id: "s1".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1 codex".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: true,
            idle: false,
            created_at: 2,
            last_opened_at: 2,
        },
    );
    state
        .sessions_by_path
        .insert(workspace_dir.clone(), vec!["s1".to_string(), "s2".to_string()]);

    let dirty = refresh_auto_session_names_for_path_locked(&mut state, &workspace_dir);

    assert!(dirty);
    assert_eq!(
        state
            .sessions_by_id
            .get("s1")
            .map(|session| session.name.as_str()),
        Some("workspace_2")
    );
    assert_eq!(
        state
            .sessions_by_id
            .get("s2")
            .map(|session| session.name.as_str()),
        Some("workspace_1 codex")
    );
}

#[test]
fn refresh_auto_names_preserves_existing_auto_index_gaps() {
    let workspace_dir = std::path::PathBuf::from("/tmp/workspace");
    let mut state = TerminalState::default();
    state.sessions_by_id.insert(
        "s1".to_string(),
        StoredTerminalSession {
            id: "s1".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
        },
    );
    state.sessions_by_id.insert(
        "s3".to_string(),
        StoredTerminalSession {
            id: "s3".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_3".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 3,
            last_opened_at: 3,
        },
    );
    state
        .sessions_by_path
        .insert(workspace_dir.clone(), vec!["s1".to_string(), "s3".to_string()]);

    let dirty = refresh_auto_session_names_for_path_locked(&mut state, &workspace_dir);

    assert!(!dirty);
    assert_eq!(
        state
            .sessions_by_id
            .get("s3")
            .map(|session| session.name.as_str()),
        Some("workspace_3")
    );
}

#[test]
fn refresh_auto_names_updates_live_session_state() {
    let workspace_dir = std::path::PathBuf::from("/tmp/workspace");
    let mut state = TerminalState::default();
    state.sessions_by_id.insert(
        "s1".to_string(),
        StoredTerminalSession {
            id: "s1".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
        },
    );
    let duplicate = StoredTerminalSession {
        id: "s2".to_string(),
        path: workspace_dir.clone(),
        user_name: default_terminal_user_name(),
        name: "workspace_1".to_string(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 2,
        last_opened_at: 2,
    };
    state
        .sessions_by_id
        .insert(duplicate.id.clone(), duplicate.clone());
    state
        .sessions_by_path
        .insert(workspace_dir.clone(), vec!["s1".to_string(), "s2".to_string()]);
    state
        .live_sessions
        .insert(duplicate.id.clone(), build_live_session(&duplicate));

    let dirty = refresh_auto_session_names_for_path_locked(&mut state, &workspace_dir);
    let (sessions, info_dirty) = collect_session_infos_locked(
        &mut state,
        &workspace_dir,
        &workspace_dir,
        vec![duplicate.id.clone()],
        80,
        &[],
        &[],
        true,
    );

    assert!(dirty);
    assert!(!info_dirty);
    assert_eq!(sessions[0].name, "workspace_2");
    assert_eq!(
        state
            .sessions_by_id
            .get("s2")
            .map(|session| session.name.as_str()),
        Some("workspace_2")
    );
}

#[test]
fn refresh_auto_names_avoids_cross_path_name_collisions() {
    let first_workspace_dir = std::path::PathBuf::from("/tmp/alpha/workspace");
    let second_workspace_dir = std::path::PathBuf::from("/tmp/beta/workspace");
    let mut state = TerminalState::default();
    state.sessions_by_id.insert(
        "s1".to_string(),
        StoredTerminalSession {
            id: "s1".to_string(),
            path: first_workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: second_workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 2,
            last_opened_at: 2,
        },
    );
    state
        .sessions_by_path
        .insert(first_workspace_dir.clone(), vec!["s1".to_string()]);
    state
        .sessions_by_path
        .insert(second_workspace_dir.clone(), vec!["s2".to_string()]);

    let dirty = refresh_auto_session_names_for_path_locked(&mut state, &second_workspace_dir);

    assert!(dirty);
    assert_eq!(
        state
            .sessions_by_id
            .get("s1")
            .map(|session| session.name.as_str()),
        Some("workspace_1")
    );
    assert_eq!(
        state
            .sessions_by_id
            .get("s2")
            .map(|session| session.name.as_str()),
        Some("workspace_2")
    );
}

#[test]
fn refresh_auto_names_preserves_cross_path_auto_name() {
    let first_workspace_dir = std::path::PathBuf::from("/tmp/alpha");
    let second_workspace_dir = std::path::PathBuf::from("/tmp/beta");
    let mut state = TerminalState::default();
    state.sessions_by_id.insert(
        "s1".to_string(),
        StoredTerminalSession {
            id: "s1".to_string(),
            path: first_workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "alpha_1".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: second_workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "beta_9".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 2,
            last_opened_at: 2,
        },
    );
    state
        .sessions_by_path
        .insert(first_workspace_dir.clone(), vec!["s1".to_string()]);
    state
        .sessions_by_path
        .insert(second_workspace_dir.clone(), vec!["s2".to_string()]);

    let dirty = refresh_auto_session_names_for_path_locked(&mut state, &second_workspace_dir);

    assert!(!dirty);
    assert_eq!(
        state
            .sessions_by_id
            .get("s1")
            .map(|session| session.name.as_str()),
        Some("alpha_1")
    );
    assert_eq!(
        state
            .sessions_by_id
            .get("s2")
            .map(|session| session.name.as_str()),
        Some("beta_9")
    );
}

#[test]
fn create_auto_name_start_index_uses_max_remaining_auto_index_for_path() {
    let workspace_dir = std::path::PathBuf::from("/tmp/webclx/workspace");
    let other_workspace_dir = std::path::PathBuf::from("/tmp/other/workspace");
    let mut state = TerminalState::default();
    state.sessions_by_id.insert(
        "s5".to_string(),
        StoredTerminalSession {
            id: "s5".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_5".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 5,
            last_opened_at: 5,
        },
    );
    state.sessions_by_id.insert(
        "s12".to_string(),
        StoredTerminalSession {
            id: "s12".to_string(),
            path: other_workspace_dir,
            user_name: default_terminal_user_name(),
            name: "workspace_12".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 12,
            last_opened_at: 12,
        },
    );
    state
        .sessions_by_path
        .insert(workspace_dir.clone(), vec!["s5".to_string()]);

    assert_eq!(next_auto_session_start_index_for_create(&state, &workspace_dir), 6);
}

#[test]
fn create_auto_name_start_index_respects_legacy_hash_names() {
    let workspace_dir = std::path::PathBuf::from("/tmp/webclx/workspace");
    let mut state = TerminalState::default();
    state.sessions_by_id.insert(
        "s5".to_string(),
        StoredTerminalSession {
            id: "s5".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace#5".to_string(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 5,
            last_opened_at: 5,
        },
    );
    state
        .sessions_by_path
        .insert(workspace_dir.clone(), vec!["s5".to_string()]);

    assert_eq!(next_auto_session_start_index_for_create(&state, &workspace_dir), 6);
}

#[test]
fn deleting_session_preserves_remaining_automatic_name() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;
    let base_dir = std::env::temp_dir().join(format!("webclx-terminal-delete-test-{unique}"));
    let workspace_dir = base_dir.join("workspace");
    fs::create_dir_all(&workspace_dir).expect("create test workspace");
    let state_file = base_dir.join("terminal-sessions.json");

    let registry = StoredTerminalRegistry {
        next_ordinal: unique.max(1),
        sessions: Vec::new(),
        input_histories: std::collections::HashMap::new(),
        output_observations: std::collections::HashMap::new(),
    };
    fs::write(&state_file, serde_json::to_vec_pretty(&registry).expect("encode registry"))
        .expect("write registry");

    let manager = TerminalManager::new(state_file);
    let first = manager
        .create_session(
            &workspace_dir,
            &workspace_dir,
            workspace_dir.clone(),
            test_user_profile(),
            Vec::new(),
            Vec::new(),
            None,
            String::new(),
            String::new(),
        )
        .expect("create first tmux-backed session");
    let second = manager
        .create_session(
            &workspace_dir,
            &workspace_dir,
            workspace_dir.clone(),
            test_user_profile(),
            Vec::new(),
            Vec::new(),
            None,
            String::new(),
            String::new(),
        )
        .expect("create second tmux-backed session");

    assert_eq!(first.origin, super::TerminalSessionOrigin::Normal);
    assert_eq!(second.origin, super::TerminalSessionOrigin::Normal);
    assert_eq!(first.name, "workspace_1");
    assert_eq!(second.name, "workspace_2");

    manager
        .delete_session(&workspace_dir, &workspace_dir, &first.id)
        .expect("delete first session");

    let sessions = manager.list_sessions(
        &workspace_dir,
        &workspace_dir,
        &workspace_dir,
        80,
        &[],
        &[],
        60,
        true,
        1.5,
        20,
    );
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, second.id);
    assert_eq!(sessions[0].name, "workspace_2");

    let _ = manager.delete_session(&workspace_dir, &workspace_dir, &second.id);
    let _ = fs::remove_dir_all(base_dir);
}

#[test]
fn tmux_backed_sessions_survive_manager_restart() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;
    let base_dir = std::env::temp_dir().join(format!("webclx-terminal-test-{unique}"));
    let workspace_dir = base_dir.join("workspace");
    fs::create_dir_all(&workspace_dir).expect("create test workspace");
    let state_file = base_dir.join("terminal-sessions.json");

    let registry = StoredTerminalRegistry {
        next_ordinal: unique.max(1),
        sessions: Vec::new(),
        input_histories: std::collections::HashMap::new(),
        output_observations: std::collections::HashMap::new(),
    };
    fs::write(&state_file, serde_json::to_vec_pretty(&registry).expect("encode registry"))
        .expect("write registry");

    let manager = TerminalManager::new(state_file.clone());
    let created = manager
        .create_session(
            &workspace_dir,
            &workspace_dir,
            workspace_dir.clone(),
            test_user_profile(),
            Vec::new(),
            Vec::new(),
            None,
            "Example API".to_string(),
            "https://api.example.test/v1".to_string(),
        )
        .expect("create tmux-backed session");
    assert_eq!(
        manager
            .list_sessions(
                &workspace_dir,
                &workspace_dir,
                &workspace_dir,
                80,
                &[],
                &[],
                60,
                true,
                1.5,
                20,
            )
            .len(),
        1
    );
    assert_eq!(created.name, "workspace_1");
    assert_eq!(created.codex_api_preset_name, "Example API");
    assert_eq!(created.codex_api_base_url, "https://api.example.test/v1");

    let restored = TerminalManager::new(state_file.clone());
    let restored_state = restored
        .state
        .read()
        .expect("terminal session map poisoned");
    assert!(restored_state.live_sessions.contains_key(&created.id));
    assert!(
        restored_state
            .live_sessions
            .get(&created.id)
            .is_some_and(|session| session.is_alive())
    );
    drop(restored_state);
    let sessions = restored.list_sessions(
        &workspace_dir,
        &workspace_dir,
        &workspace_dir,
        80,
        &[],
        &[],
        60,
        true,
        1.5,
        20,
    );
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, created.id);
    assert_eq!(sessions[0].name, "workspace_1");
    assert!(sessions[0].connected);

    kill_tmux_session(&created.id);

    let cleaned = TerminalManager::new(state_file);
    assert!(
        cleaned
            .list_sessions(
                &workspace_dir,
                &workspace_dir,
                &workspace_dir,
                80,
                &[],
                &[],
                60,
                true,
                1.5,
                20,
            )
            .is_empty()
    );

    let _ = fs::remove_dir_all(base_dir);
}

#[test]
fn creating_session_replaces_orphan_tmux_with_same_id() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;
    let base_dir = std::env::temp_dir().join(format!("webclx-terminal-orphan-test-{unique}"));
    let workspace_dir = base_dir.join("workspace");
    fs::create_dir_all(&workspace_dir).expect("create test workspace");
    let state_file = base_dir.join("terminal-sessions.json");
    let next_ordinal = unique.max(1);
    let session_id = format!("s{next_ordinal}");
    let orphan_marker = format!("WEBCLX_ORPHAN_MARKER_{next_ordinal}");

    let registry = StoredTerminalRegistry {
        next_ordinal,
        sessions: Vec::new(),
        input_histories: std::collections::HashMap::new(),
        output_observations: std::collections::HashMap::new(),
    };
    fs::write(&state_file, serde_json::to_vec_pretty(&registry).expect("encode registry"))
        .expect("write registry");

    kill_tmux_session(&session_id);
    let output = Command::new("tmux")
        .arg("new-session")
        .arg("-d")
        .arg("-s")
        .arg(tmux_session_name(&session_id))
        .arg("-c")
        .arg(&workspace_dir)
        .output()
        .expect("create orphan tmux session");
    assert_tmux_command_succeeded(output, "create orphan tmux session");
    let output = Command::new("tmux")
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(&session_id))
        .arg("-l")
        .arg(format!("printf '{orphan_marker}'"))
        .output()
        .expect("send orphan marker");
    assert_tmux_command_succeeded(output, "send orphan marker");
    let output = Command::new("tmux")
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(&session_id))
        .arg("C-m")
        .output()
        .expect("submit orphan marker");
    assert_tmux_command_succeeded(output, "submit orphan marker");

    let orphan_snapshot =
        wait_for_tmux_snapshot_matching(&session_id, |snapshot| snapshot.contains(&orphan_marker));
    assert!(orphan_snapshot.contains(&orphan_marker));

    let manager = TerminalManager::new(state_file);
    let created = manager
        .create_session(
            &workspace_dir,
            &workspace_dir,
            workspace_dir.clone(),
            test_user_profile(),
            Vec::new(),
            Vec::new(),
            None,
            String::new(),
            String::new(),
        )
        .expect("create tmux-backed session over orphan");
    assert_eq!(created.id, session_id);

    let fresh_snapshot =
        wait_for_tmux_snapshot_matching(&session_id, |snapshot| !snapshot.contains(&orphan_marker));
    assert!(!fresh_snapshot.contains(&orphan_marker));

    let _ = manager.delete_session(&workspace_dir, &workspace_dir, &created.id);
    let _ = fs::remove_dir_all(base_dir);
}

#[test]
fn requested_session_recreates_missing_tmux_session() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;
    let base_dir = std::env::temp_dir().join(format!("webclx-terminal-recreate-test-{unique}"));
    let workspace_dir = base_dir.join("workspace");
    fs::create_dir_all(&workspace_dir).expect("create test workspace");
    let state_file = base_dir.join("terminal-sessions.json");
    let registry = StoredTerminalRegistry {
        next_ordinal: unique.max(1),
        sessions: Vec::new(),
        input_histories: std::collections::HashMap::new(),
        output_observations: std::collections::HashMap::new(),
    };
    fs::write(&state_file, serde_json::to_vec_pretty(&registry).expect("encode registry"))
        .expect("write registry");

    let user_profile = test_user_profile();
    let manager = TerminalManager::new(state_file);
    let created = manager
        .create_session(
            &workspace_dir,
            &workspace_dir,
            workspace_dir.clone(),
            user_profile.clone(),
            Vec::new(),
            Vec::new(),
            None,
            String::new(),
            String::new(),
        )
        .expect("create tmux-backed session");

    kill_tmux_session(&created.id);
    for _ in 0..20 {
        if !tmux_session_exists(&created.id) {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    manager
        .state
        .write()
        .expect("terminal session map poisoned")
        .live_sessions
        .clear();

    let restored = manager
        .get_for_connection(
            workspace_dir.clone(),
            Some(&created.id),
            user_profile,
            Vec::new(),
            Vec::new(),
            None,
            String::new(),
            String::new(),
            true,
        )
        .expect("recreate missing tmux session");

    assert_eq!(restored.id, created.id);
    assert!(tmux_session_exists(&created.id));

    let _ = manager.delete_session(&workspace_dir, &workspace_dir, &created.id);
    let _ = fs::remove_dir_all(base_dir);
}

#[test]
fn terminal_resume_restore_record_persists_resume_command_and_source() {
    let record = TerminalResumeRestoreRecord {
        session_id: "s123".to_string(),
        path: std::path::PathBuf::from("/tmp/workspace"),
        user_name: "root".to_string(),
        name: "webClx#1".to_string(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
        input_history: Vec::new(),
        resume_id: "019d1ba6-f772-7452-a391-6553ccbc0a50".to_string(),
        command: "claude --resume 019d1ba6-f772-7452-a391-6553ccbc0a50".to_string(),
        program: "claude".to_string(),
        source: "terminal_buffer".to_string(),
        updated_at: 2,
    };

    assert_eq!(record.command, "claude --resume 019d1ba6-f772-7452-a391-6553ccbc0a50");
    assert_eq!(record.program, "claude");
    assert_eq!(record.source, "terminal_buffer");
}

#[tokio::test(flavor = "current_thread")]
async fn deferred_initial_restore_does_not_attach_sessions_before_returning() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;
    let base_dir = std::env::temp_dir().join(format!("webclx-deferred-restore-{unique}"));
    fs::create_dir_all(&base_dir).expect("create test directory");
    let state_file = base_dir.join("terminal-sessions.json");
    let mut stored = stored_test_terminal_session(&format!("s{unique}"));
    stored.path = base_dir.clone();
    let registry = StoredTerminalRegistry {
        next_ordinal: unique.saturating_add(1),
        sessions: vec![stored.clone()],
        input_histories: std::collections::HashMap::new(),
        output_observations: std::collections::HashMap::new(),
    };
    fs::write(
        &state_file,
        serde_json::to_vec_pretty(&registry).expect("encode terminal registry"),
    )
    .expect("write terminal registry");
    let restore_file = state_file.with_file_name(super::TERMINAL_SHUTDOWN_RESTORE_FILE_NAME);
    persist_terminal_shutdown_restore_registry(
        &restore_file,
        &super::TerminalShutdownRestoreRegistry {
            records: vec![TerminalResumeRestoreRecord {
                session_id: stored.id.clone(),
                path: stored.path.clone(),
                user_name: stored.user_name.clone(),
                name: stored.name.clone(),
                title: stored.title.clone(),
                codex_api_preset_name: String::new(),
                codex_api_base_url: String::new(),
                origin: super::TerminalSessionOrigin::Normal,
                owner_key: String::new(),
                manually_renamed: false,
                idle: false,
                created_at: stored.created_at,
                last_opened_at: stored.last_opened_at,
                input_history: Vec::new(),
                resume_id: format!("resume-{unique}"),
                command: "printf deferred-restore".to_string(),
                program: "codex".to_string(),
                source: "test".to_string(),
                updated_at: unique,
            }],
        },
    )
    .expect("write shutdown restore registry");

    let manager = TerminalManager::new_with_environment_deferred_restore(
        state_file,
        TerminalEnvironmentSnapshot {
            workspace_root: base_dir.clone(),
            display_root: base_dir.clone(),
            user_profile: test_user_profile(),
            terminal_default_env: Vec::new(),
            proxy_env: Vec::new(),
        },
        crate::quota_reset_cache::QuotaResetCache::new(),
    );

    let state = crate::lock_or_recover!(manager.state.read());
    assert!(state.sessions_by_id.contains_key(&stored.id));
    assert!(state.live_sessions.is_empty());
    drop(state);
    fs::remove_dir_all(base_dir).expect("remove test directory");
}

#[test]
fn shutdown_restore_recreates_missing_tmux_session_and_runs_saved_command() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;
    let base_dir = std::env::temp_dir().join(format!("webclx-terminal-shutdown-restore-{unique}"));
    let workspace_dir = base_dir.join("workspace");
    fs::create_dir_all(&workspace_dir).expect("create test workspace");
    let state_file = base_dir.join("terminal-sessions.json");
    let session_id = format!("s{unique}");
    let marker = format!("WEBCLX_RESTORED_{unique}");
    let user_profile = test_user_profile();
    let stored = StoredTerminalSession {
        id: session_id.clone(),
        path: workspace_dir.clone(),
        user_name: user_profile.name.clone(),
        name: "workspace_1".to_string(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: unique,
        last_opened_at: unique,
    };
    let registry = StoredTerminalRegistry {
        next_ordinal: unique.saturating_add(1),
        sessions: vec![stored.clone()],
        input_histories: std::collections::HashMap::new(),
        output_observations: std::collections::HashMap::new(),
    };
    fs::write(
        &state_file,
        serde_json::to_vec_pretty(&registry).expect("encode terminal registry"),
    )
    .expect("write terminal registry");

    let restore_file = state_file.with_file_name(super::TERMINAL_SHUTDOWN_RESTORE_FILE_NAME);
    let restore_registry = super::TerminalShutdownRestoreRegistry {
        records: vec![TerminalResumeRestoreRecord {
            session_id: session_id.clone(),
            path: workspace_dir.clone(),
            user_name: user_profile.name.clone(),
            name: stored.name.clone(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: unique,
            last_opened_at: unique,
            input_history: vec![
                super::TerminalInputHistoryEntry {
                    text: "first saved user prompt".to_string(),
                    created_at: unique.saturating_sub(2),
                },
                super::TerminalInputHistoryEntry {
                    text: "second saved user prompt".to_string(),
                    created_at: unique.saturating_sub(1),
                },
            ],
            resume_id: format!("resume-{unique}"),
            command: format!("printf {marker}"),
            program: "codex".to_string(),
            source: "test".to_string(),
            updated_at: unique,
        }],
    };
    persist_terminal_shutdown_restore_registry(&restore_file, &restore_registry)
        .expect("write shutdown restore registry");
    kill_tmux_session(&session_id);

    let manager = TerminalManager::new_with_environment(
        state_file.clone(),
        TerminalEnvironmentSnapshot {
            workspace_root: base_dir.clone(),
            display_root: base_dir.clone(),
            user_profile: user_profile.clone(),
            terminal_default_env: Vec::new(),
            proxy_env: Vec::new(),
        },
        crate::quota_reset_cache::QuotaResetCache::new(),
    );

    for _ in 0..20 {
        let snapshot = capture_tmux_text_pane_snapshot(&session_id)
            .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
            .unwrap_or_default();
        if snapshot.contains(&marker) {
            let history = manager
                .session_input_history(&session_id)
                .expect("restored input history");
            let texts = history
                .iter()
                .map(|entry| entry.text.as_str())
                .collect::<Vec<_>>();
            assert_eq!(texts, vec!["first saved user prompt", "second saved user prompt"]);
            let _ = manager.delete_session(&workspace_dir, &workspace_dir, &session_id);
            let _ = fs::remove_dir_all(base_dir);
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let snapshot =
        String::from_utf8_lossy(&capture_tmux_text_pane_snapshot(&session_id).unwrap()).to_string();
    let _ = manager.delete_session(&workspace_dir, &workspace_dir, &session_id);
    let _ = fs::remove_dir_all(base_dir);
    panic!("restore command marker not found in tmux snapshot: {snapshot}");
}

#[test]
fn save_shutdown_restore_registry_persists_input_history() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;
    let base_dir = std::env::temp_dir().join(format!("webclx-shutdown-save-history-{unique}"));
    let workspace_dir = base_dir.join("workspace");
    fs::create_dir_all(&workspace_dir).expect("create test workspace");
    let state_file = base_dir.join("terminal-sessions.json");
    let registry = StoredTerminalRegistry {
        next_ordinal: unique.max(1),
        sessions: Vec::new(),
        input_histories: std::collections::HashMap::new(),
        output_observations: std::collections::HashMap::new(),
    };
    fs::write(&state_file, serde_json::to_vec_pretty(&registry).expect("encode registry"))
        .expect("write registry");
    let user_profile = test_user_profile();
    let connection_user_profile = user_profile.clone();
    let manager = TerminalManager::new_with_environment(
        state_file.clone(),
        TerminalEnvironmentSnapshot {
            workspace_root: base_dir.clone(),
            display_root: base_dir.clone(),
            user_profile: user_profile.clone(),
            terminal_default_env: Vec::new(),
            proxy_env: Vec::new(),
        },
        crate::quota_reset_cache::QuotaResetCache::new(),
    );
    let created = manager
        .create_session(
            &workspace_dir,
            &workspace_dir,
            workspace_dir.clone(),
            user_profile,
            Vec::new(),
            Vec::new(),
            None,
            String::new(),
            String::new(),
        )
        .expect("create tmux-backed session");
    let _live_session = manager
        .get_for_connection(
            workspace_dir.clone(),
            Some(&created.id),
            connection_user_profile,
            Vec::new(),
            Vec::new(),
            None,
            String::new(),
            String::new(),
            true,
        )
        .expect("attach live browser session");
    let resume_id = "ad08e570-051b-4b66-8c7a-7b20e434b168";
    manager.record_session_input(&created.id, "first saved user prompt\n");
    manager.record_session_input(&created.id, "second saved user prompt\n");
    Command::new("tmux")
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(&created.id))
        .arg(format!("echo 'Resume this session with: codex resume {resume_id}'"))
        .output()
        .expect("echo codex resume prompt");
    Command::new("tmux")
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(&created.id))
        .arg("C-m")
        .output()
        .expect("submit echo");

    let mut detected = None;
    for _ in 0..30 {
        if let Ok(Some(value)) = manager.current_resume_session(&created.id) {
            if value.info.resume_id == resume_id {
                detected = Some(value);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    detected.expect("resume command should be detected from terminal buffer");
    assert_eq!(
        manager
            .save_shutdown_restore_registry()
            .expect("save shutdown restore registry"),
        1
    );

    let restore_file = state_file.with_file_name(super::TERMINAL_SHUTDOWN_RESTORE_FILE_NAME);
    let restore_registry =
        super::load_terminal_shutdown_restore_registry(&restore_file).expect("load restore file");
    assert_eq!(restore_registry.records.len(), 1);
    let texts = restore_registry.records[0]
        .input_history
        .iter()
        .map(|entry| entry.text.as_str())
        .collect::<Vec<_>>();
    assert_eq!(texts, vec!["first saved user prompt", "second saved user prompt"]);

    let _ = manager.delete_session(&workspace_dir, &workspace_dir, &created.id);
    let _ = fs::remove_dir_all(base_dir);
}

/// "保存会话并关机" 必须能把 Claude 会话也恢复出来，不能只认 Codex。
/// 这里覆盖 buffer 兜底路径：当终端里能看到 `claude --resume <id>` 的退出提示时，
/// `current_resume_session` 应返回 `program="claude"` 与完整命令，而非退化成 codex。
#[test]
fn current_resume_session_detects_claude_from_terminal_buffer() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;
    let base_dir = std::env::temp_dir().join(format!("webclx-claude-resume-detect-{unique}"));
    let workspace_dir = base_dir.join("workspace");
    fs::create_dir_all(&workspace_dir).expect("create test workspace");
    let state_file = base_dir.join("terminal-sessions.json");
    let registry = StoredTerminalRegistry {
        next_ordinal: unique.max(1),
        sessions: Vec::new(),
        input_histories: std::collections::HashMap::new(),
        output_observations: std::collections::HashMap::new(),
    };
    fs::write(&state_file, serde_json::to_vec_pretty(&registry).expect("encode registry"))
        .expect("write registry");

    let user_profile = test_user_profile();
    let connection_user_profile = user_profile.clone();
    let manager = TerminalManager::new_with_environment(
        state_file,
        TerminalEnvironmentSnapshot {
            workspace_root: base_dir.clone(),
            display_root: base_dir.clone(),
            user_profile: user_profile.clone(),
            terminal_default_env: Vec::new(),
            proxy_env: Vec::new(),
        },
        crate::quota_reset_cache::QuotaResetCache::new(),
    );
    let created = manager
        .create_session(
            &workspace_dir,
            &workspace_dir,
            workspace_dir.clone(),
            user_profile.clone(),
            Vec::new(),
            Vec::new(),
            None,
            String::new(),
            String::new(),
        )
        .expect("create tmux-backed session");
    let _live_session = manager
        .get_for_connection(
            workspace_dir.clone(),
            Some(&created.id),
            connection_user_profile,
            Vec::new(),
            Vec::new(),
            None,
            String::new(),
            String::new(),
            true,
        )
        .expect("attach live browser session");

    let resume_id = "ad08e570-051b-4b66-8c7a-7b20e434b168";
    // 模拟 Claude 退出时打印的恢复提示。
    Command::new("tmux")
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(&created.id))
        .arg(format!("echo 'Resume this session with: claude --resume {resume_id}'"))
        .output()
        .expect("echo claude resume prompt");
    Command::new("tmux")
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(&created.id))
        .arg("C-m")
        .output()
        .expect("submit echo");

    // 给 echo 足够时间落盘到 tmux buffer，并让 buffer 扫描读到。
    let mut detected = None;
    for _ in 0..30 {
        if let Ok(Some(value)) = manager.current_resume_session(&created.id) {
            if value.info.resume_id == resume_id {
                detected = Some(value);
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    let info = detected.expect("claude resume command should be detected from buffer");
    assert_eq!(info.info.program, "claude");
    assert_eq!(info.info.resume_id, resume_id);
    assert_eq!(info.info.command, format!("claude --resume {resume_id}"));

    let _ = manager.delete_session(&workspace_dir, &workspace_dir, &created.id);
    let _ = fs::remove_dir_all(base_dir);
}

#[test]
fn mouse_only_input_detects_sgr_reports() {
    // 单条 SGR 鼠标报告（press）：ESC [ < 0 ; 12 ; 5 M
    let sgr_press = "\u{1b}[<0;12;5M";
    assert!(is_mouse_only_input(sgr_press));

    // SGR release 用小写 m
    let sgr_release = "\u{1b}[<0;12;5m";
    assert!(is_mouse_only_input(sgr_release));

    // 多位坐标
    let sgr_multi_digit = "\u{1b}[<35;120;48M";
    assert!(is_mouse_only_input(sgr_multi_digit));
}

#[test]
fn mouse_only_input_detects_x10_reports() {
    // X10 鼠标报告：ESC [ M <b1> <b2> <b3>
    let x10 = "\u{1b}[M !5";
    assert!(is_mouse_only_input(x10));
}

#[test]
fn mouse_only_input_detects_concatenated_reports() {
    // 拖动选区时高频拼接的多条 SGR 报告
    let batch = "\u{1b}[<32;10;3M\u{1b}[<32;11;3M\u{1b}[<32;12;4M\u{1b}[<0;12;4m";
    assert!(is_mouse_only_input(batch));
}

#[test]
fn mouse_only_input_rejects_non_mouse_input() {
    // 空串不算鼠标报告
    assert!(!is_mouse_only_input(""));

    // 普通键盘输入
    assert!(!is_mouse_only_input("ls -la\n"));
    assert!(!is_mouse_only_input("claude --resume abc-123"));

    // 控制字符 / 粘贴
    assert!(!is_mouse_only_input("\u{3}")); // Ctrl-C
    assert!(!is_mouse_only_input("paste text"));

    // 鼠标报告后跟真实输入（不是纯鼠标）
    assert!(!is_mouse_only_input("\u{1b}[<0;1;1Mx"));

    // 真实输入后跟鼠标报告（不是纯鼠标）
    assert!(!is_mouse_only_input("x\u{1b}[<0;1;1M"));

    // 残缺的 SGR 序列（缺终结符）不算
    assert!(!is_mouse_only_input("\u{1b}[<0;1;1"));
    // 残缺字段
    assert!(!is_mouse_only_input("\u{1b}[<0;;1M"));

    // ESC 序列但不是鼠标（如光标键）
    assert!(!is_mouse_only_input("\u{1b}[A")); // 上方向键
}

fn kill_tmux_session(session_id: &str) {
    let _ = Command::new("tmux")
        .arg("kill-session")
        .arg("-t")
        .arg(tmux_session_name(session_id))
        .output();
}

fn build_test_terminal_manager() -> (TerminalManager, std::path::PathBuf) {
    let unique = format!(
        "webclx-auto-typed-input-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    let base_dir = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&base_dir).expect("temp base dir");
    let state_file = base_dir.join("terminal-state.json");
    let user_profile = test_user_profile();
    let manager = TerminalManager::new_with_environment(
        state_file.clone(),
        TerminalEnvironmentSnapshot {
            workspace_root: base_dir.clone(),
            display_root: base_dir.clone(),
            user_profile: user_profile.clone(),
            terminal_default_env: Vec::new(),
            proxy_env: Vec::new(),
        },
        crate::quota_reset_cache::QuotaResetCache::new(),
    );
    (manager, base_dir)
}

#[test]
fn scheduled_input_persists_across_manager_reload() {
    let (manager, base_dir) = build_test_terminal_manager();
    let stored = stored_test_terminal_session("s1");
    let live = build_live_session(&stored);
    manager.insert_test_live_session(stored, live);

    let due_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_millis() as u64
        + 60_000;
    let created = manager
        .create_scheduled_input(
            "s1",
            "echo persisted".to_string(),
            due_at,
            "1 分钟后".to_string(),
            true,
            "paste".to_string(),
            String::new(),
        )
        .expect("scheduled input should be created");

    let reloaded = TerminalManager::new_with_environment(
        base_dir.join("terminal-state.json"),
        TerminalEnvironmentSnapshot {
            workspace_root: base_dir.clone(),
            display_root: base_dir.clone(),
            user_profile: test_user_profile(),
            terminal_default_env: Vec::new(),
            proxy_env: Vec::new(),
        },
        crate::quota_reset_cache::QuotaResetCache::new(),
    );
    let tasks = reloaded.list_scheduled_inputs();

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].task_id, created.task_id);
    assert_eq!(tasks[0].session_id, "s1");
    assert_eq!(tasks[0].preview, "echo persisted");
}

#[test]
fn send_session_input_silent_does_not_record_input_history() {
    let (manager, base_dir) = build_test_terminal_manager();
    let stored = stored_test_terminal_session("s1");
    let live = build_live_session(&stored);
    manager.insert_test_live_session(stored, live);

    manager
        .send_session_input_silent(
            "s1",
            "export IS_SANDBOX=1; claude --dangerously-skip-permissions\n".to_string(),
        )
        .expect("silent send should succeed for live session");

    let history = manager
        .session_input_history("s1")
        .expect("session should be present");
    assert!(
        history.is_empty(),
        "silent send must not append to the per-session input history (got {:?})",
        history
    );

    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn send_session_input_records_input_history_by_default() {
    let (manager, base_dir) = build_test_terminal_manager();
    let stored = stored_test_terminal_session("s1");
    let live = build_live_session(&stored);
    manager.insert_test_live_session(stored, live);

    manager
        .send_session_input("s1", "ls\n".to_string())
        .expect("default send should succeed for live session");

    let history = manager
        .session_input_history("s1")
        .expect("session should be present");
    assert_eq!(history.len(), 1, "default send should record one entry");
    assert_eq!(history[0].text, "ls");

    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn scheduled_input_body_single_line() {
    use super::TerminalScheduledInputTask;
    let task = TerminalScheduledInputTask {
        id: "t1".to_string(),
        session_id: "s1".to_string(),
        terminal_name: String::new(),
        due_at_millis: 0,
        created_at_millis: 0,
        label: String::new(),
        text: "/goal resume".to_string(),
        send_enter: true,
        task_type: String::new(),
        working_dir: String::new(),
    };
    // Body never includes the submit CR; fire_scheduled_input_task emits it as
    // a separate write so the raw-mode TUI treats it as a distinct key event.
    assert_eq!(super::manager::scheduled_input_body(&task), "/goal resume");
}

#[test]
fn scheduled_input_body_plain_text() {
    use super::TerminalScheduledInputTask;
    let task = TerminalScheduledInputTask {
        id: "t2".to_string(),
        session_id: "s1".to_string(),
        terminal_name: String::new(),
        due_at_millis: 0,
        created_at_millis: 0,
        label: String::new(),
        text: "hello".to_string(),
        send_enter: false,
        task_type: String::new(),
        working_dir: String::new(),
    };
    assert_eq!(super::manager::scheduled_input_body(&task), "hello");
}

#[test]
fn scheduled_input_body_multiline_uses_cr_inside_bracketed_paste() {
    use super::TerminalScheduledInputTask;
    // Internal line breaks must collapse to CR (not LF) so a raw-mode TUI
    // (Codex/Claude) treats them as line separators inside the bracketed paste
    // region. The submit CR is NOT part of the body.
    let task = TerminalScheduledInputTask {
        id: "t3".to_string(),
        session_id: "s1".to_string(),
        terminal_name: String::new(),
        due_at_millis: 0,
        created_at_millis: 0,
        label: String::new(),
        text: "line1\nline2".to_string(),
        send_enter: true,
        task_type: String::new(),
        working_dir: String::new(),
    };
    let body = super::manager::scheduled_input_body(&task);
    assert_eq!(
        body, "\u{1b}[200~line1\rline2\u{1b}[201~",
        "multiline body must wrap CR-separated text in bracketed paste without a submit CR"
    );
    assert!(!body.contains('\n'), "body must not contain any LF byte; got {body:?}");
}

#[test]
fn scheduled_input_body_crlf_source_collapses_to_cr() {
    use super::TerminalScheduledInputTask;
    let task = TerminalScheduledInputTask {
        id: "t4".to_string(),
        session_id: "s1".to_string(),
        terminal_name: String::new(),
        due_at_millis: 0,
        created_at_millis: 0,
        label: String::new(),
        text: "a\r\nb\r\nc".to_string(),
        send_enter: true,
        task_type: String::new(),
        working_dir: String::new(),
    };
    let body = super::manager::scheduled_input_body(&task);
    assert_eq!(body, "\u{1b}[200~a\rb\rc\u{1b}[201~");
    assert!(!body.contains('\n'));
}

#[test]
fn terminal_message_body_uses_explicit_bracketed_paste_for_agent_prompts() {
    assert_eq!(
        super::manager::prepare_terminal_message_body("line1\nline2", true),
        "\u{1b}[200~line1\rline2\u{1b}[201~"
    );
}

#[test]
fn terminal_message_body_preserves_raw_mode() {
    assert_eq!(
        super::manager::prepare_terminal_message_body("line1\nline2", false),
        "line1\nline2"
    );
}

#[test]
fn terminal_message_bracketed_paste_waits_for_tui_settle() {
    assert_eq!(
        super::manager::terminal_message_paste_settle_delay("short", true),
        std::time::Duration::from_millis(601),
    );
    assert_eq!(
        super::manager::terminal_message_paste_settle_delay(&"x".repeat(20_000), true),
        std::time::Duration::from_millis(2_000),
    );
    assert_eq!(
        super::manager::terminal_message_paste_settle_delay("short", false),
        std::time::Duration::from_millis(120),
    );
}

#[test]
fn terminal_message_submission_timing_is_bounded() {
    let verification_window_ms =
        super::TERMINAL_MESSAGE_VERIFY_POLLS as u64 * super::TERMINAL_MESSAGE_VERIFY_POLL_MS;
    assert_eq!(verification_window_ms, 2_000);
    assert!(
        (super::TERMINAL_MESSAGE_VERIFY_POLLS.saturating_sub(1) as u64)
            * super::TERMINAL_MESSAGE_VERIFY_POLL_MS
            <= verification_window_ms
    );
    assert_eq!(super::TERMINAL_MESSAGE_SUBMIT_RETRY_DELAYS_MS, [1_000, 2_000, 4_000]);
}

#[test]
fn terminal_message_delivery_ack_requires_new_rollout_evidence() {
    let entries = vec![
        super::TerminalInputHistoryEntry {
            text: "[from webClx#1] repeatable message".to_string(),
            created_at: 1,
        },
        super::TerminalInputHistoryEntry {
            text: "[from webClx#1] repeatable message".to_string(),
            created_at: 2,
        },
    ];

    assert!(!super::manager::terminal_message_delivery_confirmed(
        None,
        "repeatable message",
        0,
    ));
    assert!(!super::manager::terminal_message_delivery_confirmed(
        Some(&entries[..1]),
        "repeatable message",
        1,
    ));
    assert!(super::manager::terminal_message_delivery_confirmed(
        Some(&entries),
        "repeatable message",
        1,
    ));
    assert!(!super::manager::terminal_message_delivery_confirmed(
        Some(&entries),
        "missing message",
        0,
    ));
}

#[test]
fn parses_codex_model_provider_from_status_output() {
    let snapshot = concat!(
        "│  Model:                gpt-5.6-sol (reasoning xhigh, summaries auto)    │\n",
        "│  Model provider:       sub2api_gpt-5.6_1M - http://192.168.3.2:18381/v1  │\n",
    );

    let provider = super::manager::parse_codex_model_provider(snapshot.as_bytes())
        .expect("Model provider should be present");
    assert_eq!(provider, "sub2api_gpt-5.6_1M - http://192.168.3.2:18381/v1");
    assert_eq!(
        super::manager::split_codex_model_provider(&provider),
        ("sub2api_gpt-5.6_1M".to_string(), "http://192.168.3.2:18381/v1".to_string(),)
    );
}

#[test]
fn delete_codex_conversation_removes_only_target_session_metadata() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be after epoch")
        .as_nanos();
    let temp_dir = std::env::temp_dir().join(format!("webclx-codex-delete-{unique}"));
    let codex_home = temp_dir.join(".codex");
    let sessions_dir = codex_home.join("sessions/2026/07/17");
    fs::create_dir_all(&sessions_dir).expect("create Codex sessions directory");

    let target_id = "019f0a18-1111-7222-8333-444455556666";
    let kept_id = "019f0a18-aaaa-7bbb-8ccc-ddddeeeeffff";
    let target_rollout =
        sessions_dir.join(format!("rollout-2026-07-17T10-00-00-{target_id}.jsonl"));
    let kept_rollout = sessions_dir.join(format!("rollout-2026-07-17T10-01-00-{kept_id}.jsonl"));
    fs::write(&target_rollout, "{\"type\":\"session_meta\"}\n").expect("write target rollout");
    fs::write(&kept_rollout, "{\"type\":\"session_meta\"}\n").expect("write kept rollout");
    fs::write(
        codex_home.join("session_index.jsonl"),
        format!(
            "{{\"id\":\"{target_id}\",\"cwd\":\"/target\"}}\n{{\"id\":\"{kept_id}\",\"cwd\":\"/kept\"}}\n"
        ),
    )
    .expect("write session index");
    fs::write(
        codex_home.join("history.jsonl"),
        format!(
            "{{\"session_id\":\"{target_id}\",\"text\":\"remove\"}}\n{{\"session_id\":\"{kept_id}\",\"text\":\"keep\"}}\n"
        ),
    )
    .expect("write input history");

    let database = rusqlite::Connection::open(codex_home.join("state_5.sqlite"))
        .expect("create Codex state database");
    database
        .execute_batch(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, rollout_path TEXT NOT NULL);\
             CREATE TABLE thread_dynamic_tools (thread_id TEXT NOT NULL, name TEXT NOT NULL);\
             CREATE TABLE thread_spawn_edges (parent_thread_id TEXT NOT NULL, child_thread_id TEXT NOT NULL);",
        )
        .expect("create Codex state schema");
    database
        .execute(
            "INSERT INTO threads (id, rollout_path) VALUES (?1, ?2), (?3, ?4)",
            rusqlite::params![
                target_id,
                target_rollout.display().to_string(),
                kept_id,
                kept_rollout.display().to_string(),
            ],
        )
        .expect("seed Codex threads");
    database
        .execute(
            "INSERT INTO thread_dynamic_tools (thread_id, name) VALUES (?1, 'target'), (?2, 'kept')",
            rusqlite::params![target_id, kept_id],
        )
        .expect("seed dynamic tools");
    database
        .execute(
            "INSERT INTO thread_spawn_edges (parent_thread_id, child_thread_id) VALUES (?1, ?2)",
            rusqlite::params![target_id, kept_id],
        )
        .expect("seed spawn edge");
    drop(database);

    let result = super::delete_codex_conversation_files(&codex_home, target_id)
        .expect("delete target Codex conversation");

    assert_eq!(result.session_id, target_id);
    assert_eq!(result.rollout_files_deleted, 1);
    assert!(!target_rollout.exists());
    assert!(kept_rollout.exists());
    let session_index = fs::read_to_string(codex_home.join("session_index.jsonl"))
        .expect("read filtered session index");
    assert!(!session_index.contains(target_id));
    assert!(session_index.contains(kept_id));
    let history =
        fs::read_to_string(codex_home.join("history.jsonl")).expect("read filtered history");
    assert!(!history.contains(target_id));
    assert!(history.contains(kept_id));

    let database = rusqlite::Connection::open(codex_home.join("state_5.sqlite"))
        .expect("open filtered Codex state database");
    let target_threads: i64 = database
        .query_row("SELECT COUNT(*) FROM threads WHERE id = ?1", [target_id], |row| row.get(0))
        .expect("count target threads");
    let kept_threads: i64 = database
        .query_row("SELECT COUNT(*) FROM threads WHERE id = ?1", [kept_id], |row| row.get(0))
        .expect("count kept threads");
    let target_tools: i64 = database
        .query_row(
            "SELECT COUNT(*) FROM thread_dynamic_tools WHERE thread_id = ?1",
            [target_id],
            |row| row.get(0),
        )
        .expect("count target dynamic tools");
    let target_edges: i64 = database
        .query_row(
            "SELECT COUNT(*) FROM thread_spawn_edges WHERE parent_thread_id = ?1 OR child_thread_id = ?1",
            [target_id],
            |row| row.get(0),
        )
        .expect("count target spawn edges");
    assert_eq!(target_threads, 0);
    assert_eq!(kept_threads, 1);
    assert_eq!(target_tools, 0);
    assert_eq!(target_edges, 0);

    drop(database);
    fs::remove_dir_all(temp_dir).expect("remove Codex delete fixture");
}

#[test]
fn delete_codex_conversation_rejects_invalid_session_id() {
    let error =
        super::delete_codex_conversation_files(Path::new("/tmp/.codex"), "../history.jsonl")
            .expect_err("invalid Codex session id should be rejected");
    assert!(error.to_string().contains("session ID"));
}
