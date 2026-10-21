use auth_core::{CurrentClaudeState, StoredApiPreset};
use portable_pty::{Child, ChildKiller, ExitStatus, PtySize};
use rusqlite::{Connection, params};
use settings_core::{SettingsManager, TerminalQuickCommand, build_settings_response};
use std::{
    fs, io,
    path::Path,
    sync::{
        Arc, Condvar, Mutex, OnceLock, RwLock,
        atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use terminal_core::build_tmux_child_env;

use super::activity::TerminalAgentActivity;
use super::manager::activity_probe::{
    PROBE_SNAPSHOT_MAX_AGE, ProbeSnapshotCache, ProbeSnapshotCacheEntry,
};
use super::manager::{
    TerminalActivityProbe, arm_output_observations_for_restore_locked,
    auto_continue_backoff_interval_millis, auto_continue_retry_at_millis,
    capture_terminal_input_history, collect_session_activity_probes,
    collect_session_infos_from_probes_locked, collect_terminal_auto_continue_schedules,
    is_terminal_continue_line, load_terminal_pending_build_requests,
    next_auto_session_start_index_for_create, now_within_active_window,
    persist_terminal_pending_build_registry, prepare_restored_output_observation_locked,
    probe_activity_pane_snapshot_with, rebaseline_terminal_output_locked,
    remove_legacy_codex_command_env_launchers, restore_system_probe_output_observation_locked,
    terminal_auto_continue_cron_entry, terminal_auto_continue_cron_script,
    terminal_auto_continue_due_millis, terminal_compacting_status_from_snapshot,
    terminal_error_has_continue_after, terminal_error_has_queued_input_after,
    terminal_error_keyword_is_quota_like, terminal_error_reset_time_from_tail,
    terminal_model_family_from_snapshot, terminal_quota_preflight_uses_zhipu_provider,
    terminal_recent_output_has_quota_error, terminal_recent_output_uses_glm_model,
    terminal_reset_time_epoch_millis, terminal_tail_error_keyword,
    terminal_tail_error_keyword_with_manual_interrupt_policy, terminal_tail_has_compacting_status,
    terminal_tail_has_worked_status, terminal_tail_has_working_status,
    terminal_worked_status_match_from_snapshot, terminal_working_status_match_from_snapshot,
};
use super::session::TerminalOutputBacklog;
use super::tmux::{
    capture_tmux_text_pane_snapshot, isolated_tmux_command, normalize_tmux_startup_script,
    split_tmux_input_submit_keys, tmux_client_names, tmux_session_exists,
};
use super::{
    CHILD_PROCESS_ENV_KEYS_TO_CLEAR, SessionNameState, SpecifiedPresetRunCommand,
    SpecifiedPresetRunKind, StoredTerminalPendingBuildRegistry, StoredTerminalRegistry,
    StoredTerminalSession, TERMINAL_PENDING_BUILD_MAX_AGE_MS, TerminalActivitySnapshot,
    TerminalEnvironmentSnapshot, TerminalInputHistoryCapture, TerminalManager,
    TerminalPendingBuildRequest, TerminalResumeRestoreRecord, TerminalState, TitleTracker,
    api_terminal_startup_for_preset, build_terminal_quick_command_input,
    claude_terminal_unset_env_from_current, collect_session_infos_locked,
    default_terminal_user_name, ensure_unique_session_name_locked, is_mouse_only_input,
    parse_terminal_auto_continue_tasks_from_crontab, persist_terminal_shutdown_restore_registry,
    refresh_auto_session_names_for_path_locked, resolve_terminal_quick_command_input,
    should_notify_session_list_sync, sort_session_ids_by_recent_activity,
    specified_preset_run_command, tmux_session_name,
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
        child_reaped: AtomicBool::new(false),
        broadcaster,
        backlog: Arc::new(Mutex::new(TerminalOutputBacklog::new())),
        next_output_seq: AtomicU64::new(0),
        output_instance_id: 42,
        attached_at,
        suppressed_initial_redraw: AtomicBool::new(initial_redraw_suppressed),
        last_output_at: AtomicU64::new(0),
        alive: Arc::new(AtomicBool::new(true)),
        expected_shutdown: AtomicBool::new(false),
    })
}

pub(crate) fn stored_test_terminal_session(id: &str) -> StoredTerminalSession {
    StoredTerminalSession {
        id: id.to_string(),
        path: std::path::PathBuf::from("/tmp/workspace"),
        user_name: default_terminal_user_name(),
        name: "workspace_1".to_string(),
        previous_names: Vec::new(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        hidden: false,
    }
}

#[test]
fn rename_session_preserves_previous_name_alias() {
    let (manager, base_dir) = build_test_terminal_manager();
    let stored = stored_test_terminal_session("s1");
    let live = build_live_session(&stored);
    manager.insert_test_live_session(stored, live);

    manager
        .rename_session(&base_dir, &base_dir, "s1", "renamed_target".to_string())
        .expect("rename should succeed");

    assert_eq!(
        manager
            .resolve_session_target("workspace_1", None)
            .expect("old name should resolve"),
        ("s1".to_string(), "renamed_target".to_string())
    );
    assert_eq!(
        manager
            .resolve_session_target("renamed_target", None)
            .expect("new name should resolve"),
        ("s1".to_string(), "renamed_target".to_string())
    );

    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn message_response_identifies_renamed_alias_target() {
    let (manager, base_dir) = build_test_terminal_manager();
    let stored = stored_test_terminal_session("s1");
    let live = build_live_session(&stored);
    manager.insert_test_live_session(stored, live);
    manager
        .rename_session(&base_dir, &base_dir, "s1", "renamed_target".to_string())
        .expect("rename should succeed");

    let (_session_id, terminal_name, renamed_target, current_name) = manager
        .send_session_message("workspace_1", None, "test".to_string(), 0, false, false)
        .expect("message should resolve through the old alias");

    assert!(renamed_target);
    assert_eq!(terminal_name, "renamed_target");
    assert_eq!(current_name, "renamed_target");

    let _ = std::fs::remove_dir_all(base_dir);
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
        interchange_source: None,
        id: "api-minimax".to_string(),
        name: "MiniMax3".to_string(),
        provider_name: String::new(),
        saved_at: 1,
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
        favorite: false,
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

#[tokio::test]
async fn initial_terminal_replay_hands_live_output_off_after_snapshot_boundary() {
    let stored = stored_test_terminal_session("s-replay-boundary");
    let session = build_live_session(&stored);
    session.seed_backlog_prefix(b"snapshot\r\n");
    session.mark_closed(b"live redraw\r\n");

    let replay = super::initial_replay_for_socket(&session).await.unwrap();

    assert_eq!(replay.bytes, b"snapshot\r\nlive redraw\r\n");
    assert_eq!(replay.last_output_seq, 1);
}

#[tokio::test]
async fn expected_shutdown_closes_without_appending_client_exit_output() {
    let stored = stored_test_terminal_session("s-expected-shutdown");
    let session = build_live_session(&stored);

    session.prepare_for_shutdown();
    session.mark_closed(b"");

    let replay = super::initial_replay_for_socket(&session).await.unwrap();
    assert!(replay.bytes.is_empty());
    assert_eq!(replay.last_output_seq, 0);
    assert!(!session.is_alive());
}

fn resume_query(
    seq_framed: bool,
    resume_seq: Option<u64>,
    resume_instance: Option<u64>,
) -> super::TerminalSessionQuery {
    super::TerminalSessionQuery {
        path: String::new(),
        session_id: None,
        visible: true,
        seq_framed,
        resume_seq,
        resume_instance,
    }
}

#[test]
fn terminal_session_query_deserializes_from_lenient_query_string() {
    use axum::extract::Query;
    use axum::http::Uri;

    let uri: Uri =
        "/api/terminal/ws?path=tools%2FwebClx&session_id=s9080&visible=true&seq_framed=1"
            .parse()
            .unwrap();
    let Query(query): Query<super::TerminalSessionQuery> = Query::try_from_uri(&uri).unwrap();
    assert_eq!(query.path, "tools/webClx");
    assert_eq!(query.session_id.as_deref(), Some("s9080"));
    assert!(query.visible);
    assert!(query.seq_framed);

    let uri_bool: Uri =
        "/api/terminal/ws?path=tools%2FwebClx&session_id=s9080&visible=false&seq_framed=true"
            .parse()
            .unwrap();
    let Query(query_bool): Query<super::TerminalSessionQuery> =
        Query::try_from_uri(&uri_bool).unwrap();
    assert!(!query_bool.visible);
    assert!(query_bool.seq_framed);

    let uri_zero: Uri = "/api/terminal/ws?path=tools%2FwebClx&visible=0&seq_framed=0"
        .parse()
        .unwrap();
    let Query(query_zero): Query<super::TerminalSessionQuery> =
        Query::try_from_uri(&uri_zero).unwrap();
    assert!(!query_zero.visible);
    assert!(!query_zero.seq_framed);

    let uri_empty: Uri = "/api/terminal/ws".parse().unwrap();
    let Query(query_empty): Query<super::TerminalSessionQuery> =
        Query::try_from_uri(&uri_empty).unwrap();
    assert!(query_empty.visible);
    assert!(!query_empty.seq_framed);
}

#[test]
fn delta_replay_chunks_returns_contiguous_chunks_after_resume_seq() {
    let stored = stored_test_terminal_session("s-delta-resume");
    let session = build_live_session(&stored);
    session.seed_backlog_prefix(b"snapshot\r\n");
    session.record_output_chunk(b"out-1\r\n");
    session.record_output_chunk(b"out-2\r\n");
    session.record_output_chunk(b"out-3\r\n");

    let query = resume_query(true, Some(1), Some(session.output_instance_id()));
    let chunks = super::websocket::delta_replay_chunks(&session, &query).unwrap();

    assert_eq!(chunks.iter().map(|chunk| chunk.seq).collect::<Vec<_>>(), vec![2, 3]);
    assert_eq!(chunks[0].bytes, b"out-2\r\n");
}

#[test]
fn delta_replay_chunks_resumes_with_empty_tail_when_nothing_missed() {
    let stored = stored_test_terminal_session("s-delta-empty");
    let session = build_live_session(&stored);
    session.record_output_chunk(b"out-1\r\n");

    let query = resume_query(true, Some(1), Some(session.output_instance_id()));
    let chunks = super::websocket::delta_replay_chunks(&session, &query).unwrap();

    assert!(chunks.is_empty());
}

#[test]
fn delta_replay_chunks_rejects_gap_instance_mismatch_and_stale_seq() {
    let stored = stored_test_terminal_session("s-delta-reject");
    let session = build_live_session(&stored);
    session.record_output_chunk(b"out-1\r\n");
    session.record_output_chunk(b"out-2\r\n");
    session.record_output_chunk(b"out-3\r\n");
    let instance = session.output_instance_id();

    // backlog 头部被淘汰后 resume_seq+1 已不在保留范围内：弹掉 seq 1/2，
    // 以 seq 1 续传时最先保留的是 seq 3 → 缺口 → 拒绝续传。
    for _ in 0..2 {
        session
            .backlog
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .pop_front_for_test();
    }
    assert!(
        super::websocket::delta_replay_chunks(
            &session,
            &resume_query(true, Some(1), Some(instance))
        )
        .is_none()
    );
    // 实例 id 不匹配（会话重建）→ 拒绝续传。
    assert!(
        super::websocket::delta_replay_chunks(&session, &resume_query(true, Some(1), Some(999)))
            .is_none()
    );
    // resume_seq 超过当前已分配 seq（会话重建后 seq 重置）→ 拒绝续传。
    assert!(
        super::websocket::delta_replay_chunks(
            &session,
            &resume_query(true, Some(99), Some(instance))
        )
        .is_none()
    );
    // 未协商 seq 帧的客户端无法解析带序号的输出帧 → 拒绝续传。
    assert!(
        super::websocket::delta_replay_chunks(
            &session,
            &resume_query(false, Some(1), Some(instance))
        )
        .is_none()
    );
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
fn terminal_input_history_keeps_bracketed_paste_as_single_entry() {
    let mut history = TerminalInputHistoryCapture::default();

    // 一次粘贴/投递的多行消息必须保持为一条对话历史，
    // 不能因为内部换行被拆成多段。
    capture_terminal_input_history(&mut history, "\u{1b}[200~line1\rline2\nline3\u{1b}[201~\r");

    assert_eq!(history.entries.len(), 1);
    assert_eq!(history.entries[0].text, "line1\nline2\nline3");
}

#[test]
fn terminal_input_history_paste_then_plain_enter_is_separate_entries() {
    let mut history = TerminalInputHistoryCapture::default();

    capture_terminal_input_history(&mut history, "\u{1b}[200~line1\rline2\u{1b}[201~\r");
    capture_terminal_input_history(&mut history, "next command\r");

    let texts: Vec<&str> = history
        .entries
        .iter()
        .map(|entry| entry.text.as_str())
        .collect();
    assert_eq!(texts, vec!["line1\nline2", "next command"]);
}

#[test]
fn terminal_input_history_handles_large_text_paste() {
    let mut history = TerminalInputHistoryCapture::default();

    // 100KB of mixed Chinese and English text with lines and bracketed paste markers
    let line = "这是一段用于测试终端粘贴性能的大段文字，包含多种字符和代码：let mut x = 12345;\n";
    let large_paste = format!("\u{1b}[200~{}\u{1b}[201~\r", line.repeat(1000));
    assert!(large_paste.len() > 100_000);

    let started = std::time::Instant::now();
    let committed = capture_terminal_input_history(&mut history, &large_paste);
    let elapsed = started.elapsed();

    assert!(committed);
    assert!(
        elapsed < std::time::Duration::from_millis(500),
        "large text paste took too long: {:?}",
        elapsed
    );
    assert!(!history.entries.is_empty());
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
            "payload": { "cwd": "/home/codes/lyyStock/stockJiepan" }
        }),
        serde_json::json!({
            "type": "response_item",
            "payload": {
                "type": "message",
                "role": "user",
                "content": [
                    { "type": "input_text", "text": "# AGENTS.md instructions for /home/codes/lyyStock/stockJiepan" },
                    { "type": "input_text", "text": "<environment_context>\n  <cwd>/home/codes/lyyStock/stockJiepan</cwd>" }
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

    assert_eq!(metadata.cwd.as_deref(), Some("/home/codes/lyyStock/stockJiepan"));
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
            "payload": { "cwd": "/home/codes/tools/webClx" }
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
                    { "type": "input_text", "text": "[from webClx-compile-api] kidsAI 编译失败(status=1)；请求 214525；webClx 集中日志：/home/codes/tools/webClx/.webclx-compile-queue/runs/example/build-1.log。请先查看日志定位编译失败原因。" }
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

    assert_eq!(metadata.cwd.as_deref(), Some("/home/codes/tools/webClx"));
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
        "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/home/codes/tools/webClx\"}}\n",
    )
    .expect("write matching rollout");
    fs::write(
        sessions_dir.join(format!("rollout-2026-07-19T10-01-00-{other_session_id}.jsonl")),
        "{\"type\":\"session_meta\",\"payload\":{\"cwd\":\"/home/codes/other\"}}\n",
    )
    .expect("write non-matching rollout");

    let conversations = super::scan_codex_conversations_for_cwd(
        &codex_home,
        Some(std::path::Path::new("/home/codes/tools/webClx")),
    )
    .expect("scan filtered conversations");

    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].session_id, matching_session_id);
    assert_eq!(conversations[0].cwd, "/home/codes/tools/webClx");

    fs::remove_dir_all(temp_dir).expect("remove Codex cwd filter fixture");
}

#[test]
fn codex_conversation_scan_uses_state_index_and_recent_cutoff() {
    let temp_dir = std::env::temp_dir().join(format!(
        "webclx-codex-conversation-state-index-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    let codex_home = temp_dir.join(".codex");
    let sessions_dir = codex_home.join("sessions/2026/09/18");
    fs::create_dir_all(&sessions_dir).expect("create indexed Codex fixture");
    let recent_id = "019f9999-1111-7222-8333-444444444444";
    let old_id = "019f9999-5555-7666-8777-888888888888";
    let recent_path = sessions_dir.join(format!("rollout-recent-{recent_id}.jsonl"));
    let old_path = sessions_dir.join(format!("rollout-old-{old_id}.jsonl"));
    fs::write(&recent_path, "not parsed by indexed scan\n").expect("write recent rollout");
    fs::write(&old_path, "not parsed by indexed scan\n").expect("write old rollout");

    let database =
        Connection::open(codex_home.join("state_5.sqlite")).expect("create Codex state index");
    database
        .execute_batch(
            "CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                rollout_path TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL,
                cwd TEXT NOT NULL,
                title TEXT NOT NULL
            );",
        )
        .expect("create threads table");
    database
        .execute(
            "INSERT INTO threads (id, rollout_path, created_at, updated_at, cwd, title)
             VALUES (?1, ?2, 100, 200, '/workspace/indexed', 'recent indexed title')",
            params![recent_id, recent_path.display().to_string()],
        )
        .expect("insert recent indexed thread");
    database
        .execute(
            "INSERT INTO threads (id, rollout_path, created_at, updated_at, cwd, title)
             VALUES (?1, ?2, 50, 60, '/workspace/indexed', 'old indexed title')",
            params![old_id, old_path.display().to_string()],
        )
        .expect("insert old indexed thread");
    drop(database);

    let conversations = super::scan_codex_conversations_for_cwd_since(
        &codex_home,
        Some(std::path::Path::new("/workspace/indexed")),
        Some(150_000),
    )
    .expect("scan indexed recent conversations");
    assert_eq!(conversations.len(), 1);
    assert_eq!(conversations[0].session_id, recent_id);
    assert_eq!(conversations[0].title, "recent indexed title");

    fs::remove_dir_all(temp_dir).expect("remove indexed Codex fixture");
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
            ("NO_PROXY".to_string(), "127.0.0.1,localhost,127.0.0.1,::1".to_string()),
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
            script.contains("export NO_PROXY='127.0.0.1,localhost,127.0.0.1,::1'\n"),
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
        let proxy_url = "https://test-user:test-password@proxy.example.test:PORT";
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
        assert!(script.contains("proxy.example.com:PORT"), "{script}");
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
        interchange_source: None,
        id: "claude-a".to_string(),
        name: "MiniMax".to_string(),
        provider_name: "MiniMax".to_string(),
        saved_at: 1,
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
fn terminal_error_matching_detects_retry_limit_429_by_short_signature() {
    let tail = "■ exceeded retry limit, last status: 429 Too Many Requests";

    assert_eq!(
        terminal_tail_error_keyword(tail, &["last status: 429".to_string()]).as_deref(),
        Some("last status: 429")
    );
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
fn terminal_error_matching_detects_wrapped_model_busy_529_response() {
    let keyword = "该模型当前访问量过大，请您稍后再试";
    let tail = "■ unexpected status 529 <unknown status code>:\n[1305][该模型当前访问量过大，请您稍\n后再试][20260817113955263946d7b0074410], url:\nhttp://127.0.0.1:11111/api/codex-proxy/anthropic/v1/responses";

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
fn terminal_error_matching_detects_openrouter_web_search_failure() {
    let keyword = "Server tool \\\"openrouter:web_search\\\" failed".to_string();
    let tail = r#"{"error":{"code":"400","message":"Server tool \"openrouter:web_search\" failed: invalid request (400)","type":"invalid_request_error"}}"#;

    assert_eq!(
        terminal_tail_error_keyword(tail, &[keyword]).as_deref(),
        Some("Server tool \\\"openrouter:web_search\\\" failed")
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
fn terminal_auto_continue_full_day_window_is_always_active() {
    assert!(now_within_active_window("00:00-24:00"));
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
        previous_names: Vec::new(),
        title: None,
        user_name: default_terminal_user_name(),
        codex_api_preset_name: "GLM".to_string(),
        codex_api_base_url: "https://open.bigmodel.cn/api/paas/v4".to_string(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        path: "/tmp/workspace".to_string(),
        icon_project_path: "/tmp/workspace".to_string(),
        display_path: "/tmp/workspace".to_string(),
        alive: true,
        connected: true,
        busy: true,
        activity_state: "retrying".to_string(),
        activity_label: "重试中".to_string(),
        activity_agent: Some("Codex".to_string()),
        model_family: String::new(),
        activity_error_keyword: Some("last status: 429".to_string()),
        activity_error_signature: Some("quota-signature".to_string()),
        activity_error_continue_sent: true,
        activity_error_input_queued: false,
        activity_error_auto_continue_at: Some(reset_at.to_string()),
        last_output_at: 1,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        hidden: false,
    }];

    let schedules = collect_terminal_auto_continue_schedules(&sessions, 0);

    assert_eq!(schedules.len(), 1);
    assert_eq!(schedules[0].0, "s429");
    assert_eq!(schedules[0].1.reset_at, reset_at);
    assert_eq!(schedules[0].1.signature, "quota-signature");
}

#[test]
fn quota_retry_preflight_detects_glm_from_recent_terminal_model() {
    let glm_tail = "■ exceeded retry limit, last status: 429 Too Many Requests\n\n›\n\nGLM-5.3 xhigh · Context 0% used";
    assert!(terminal_recent_output_uses_glm_model(glm_tail));

    let stale_history =
        "GLM-5.3 was discussed earlier\n1\n2\n3\n4\n5\n6\n7\ngpt-5.6 xhigh · Context 0% used";
    assert!(!terminal_recent_output_uses_glm_model(stale_history));
}

#[test]
fn terminal_model_family_probe_uses_lossy_utf8_last_eight_lines_and_priority() {
    assert_eq!(terminal_model_family_from_snapshot(None), "");
    assert_eq!(terminal_model_family_from_snapshot(Some(b"")), "");
    assert_eq!(terminal_model_family_from_snapshot(Some(b"ordinary output")), "");
    assert_eq!(
        terminal_model_family_from_snapshot(Some(b"DEEPSEEK-v3\n1\n2\n3\n4\n5\n6\n7\n")),
        "deepseek"
    );
    assert_eq!(terminal_model_family_from_snapshot(Some(b"GLM-5.3")), "glm");
    assert_eq!(terminal_model_family_from_snapshot(Some(b"deepseek-v3\nGLM-5.3")), "deepseek");
    let stale = b"deepseek-old\n1\n2\n3\n4\n5\n6\n7\nGLM-5.3";
    assert_eq!(terminal_model_family_from_snapshot(Some(stale)), "glm");
    assert_eq!(
        terminal_model_family_from_snapshot(Some(&[0xff, b' ', b'G', b'L', b'M', b'-', b'1'])),
        "glm"
    );
}

#[test]
fn quota_retry_preflight_reads_429_from_tail_not_only_matched_keyword() {
    let tail = "■ exceeded retry limit, last status: 429 Too Many Requests\n\n›\n\nGLM-5.3 xhigh · Context 6% used";
    assert!(terminal_recent_output_has_quota_error(tail, 20));
    assert!(!terminal_recent_output_has_quota_error(
        "ordinary upstream disconnect\nGLM-5.3 xhigh · Context 6% used",
        20,
    ));
}

#[test]
fn session_list_uses_shared_glm_reset_when_preset_metadata_is_stale() {
    let (manager, base_dir) = build_test_terminal_manager();
    manager
        .quota_reset_cache
        .record_latest_zhipu_reset("2026-08-15 16:16:04".to_string());

    assert_eq!(
        manager.glm_model_quota_reset_from_output(
            b"exceeded retry limit, last status: 429\nGLM-5.3 xhigh | Context 0% used"
        ),
        Some("2026-08-15 16:16:04".to_string())
    );

    let _ = fs::remove_dir_all(base_dir);
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
            interchange_source: None,
            id: "api-primary".to_string(),
            name: "智谱5.2 ZCODE API".to_string(),
            provider_name: String::new(),
            saved_at: 1,
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
            favorite: false,
            switch_count: 0,
        },
        StoredApiPreset {
            interchange_source: None,
            id: "api-1m".to_string(),
            name: "智谱5.2 ZCODE API 1M".to_string(),
            provider_name: String::new(),
            saved_at: 2,
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
            favorite: false,
            switch_count: 0,
        },
    ]);
    let mut sessions = vec![super::TerminalSessionInfo {
        id: "s2018".to_string(),
        name: "ZCode_1".to_string(),
        previous_names: Vec::new(),
        title: None,
        user_name: default_terminal_user_name(),
        codex_api_preset_name: "智谱5.2 ZCODE API 1M".to_string(),
        codex_api_base_url: "https://open.bigmodel.cn/api/coding/paas/v4".to_string(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        path: "/home/third_party/ZCode".to_string(),
        icon_project_path: "/home/third_party/ZCode".to_string(),
        display_path: "/home/third_party/ZCode".to_string(),
        alive: true,
        connected: true,
        busy: true,
        activity_state: "error".to_string(),
        activity_label: "错误".to_string(),
        activity_agent: Some("Codex".to_string()),
        model_family: String::new(),
        activity_error_keyword: Some("429 Too Many Requests".to_string()),
        activity_error_signature: Some("quota-signature".to_string()),
        activity_error_continue_sent: false,
        activity_error_input_queued: false,
        activity_error_auto_continue_at: None,
        last_output_at: 1,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        hidden: false,
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
fn terminal_auto_continue_cron_uses_the_terminal_socket() {
    let script = terminal_auto_continue_cron_script(
        "webclx-auto-continue:s41:test",
        "s41",
        "quota",
        "2026-08-19 18:00:00",
        0,
    );

    assert!(script.contains("TMUX_SOCKET=\"webclx_s41\""));
    assert!(script.contains("tmux -L \"$TMUX_SOCKET\" capture-pane -p -t \"$TMUX_TARGET\" -S -80"));
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
    assert!(terminal_tail_has_working_status(
        "Let's inspect environment, network docs, rustCommander. Also current terminal id. Use exec safely. \
(38m 21s • esc to interrupt)"
    ));
    assert!(terminal_tail_has_working_status(
        "⚠ Selected model is at capacity. Please retry later. (12s • esc to interrupt)"
    ));
    assert!(terminal_tail_has_working_status("Reconnecting... 3/5 (21s • esc to interrupt)"));
    assert!(!terminal_tail_has_working_status("Working directory: /tmp/project"));
    assert!(!terminal_tail_has_working_status("working... 82% context left"));
    assert!(!terminal_tail_has_working_status("Tell the user to press esc to interrupt"));
    assert!(!terminal_tail_has_working_status(
        "Messages to be submitted after next tool call (press esc to interrupt and send immediately)"
    ));
}

#[test]
fn terminal_compacting_status_requires_live_compaction_output() {
    // Codex renders a dedicated progress line while the context is rewritten.
    assert!(terminal_tail_has_compacting_status(
        "■ Conversation interrupted - use /feedback if something went wrong\n\nCompacting context (9s • esc to interrupt)\n  └ Making room to continue.\n\n› Ask Codex to do anything"
    ));
    // Claude Code reports the same operation through its spinner family.
    assert!(terminal_tail_has_compacting_status(
        "✻ Compacting conversation… (esc to interrupt)"
    ));
    assert!(terminal_tail_has_compacting_status("正在压缩上下文，请稍候"));

    // A finished compaction is no longer in progress.
    assert!(!terminal_tail_has_compacting_status(
        "Compacting context (9s • esc to interrupt)\n• Context compacted\n› Ask Codex to do anything"
    ));
    assert!(!terminal_tail_has_compacting_status(
        "✻ Conversation compacted (ctrl+o for history)"
    ));
    // A newer turn marker means the pane moved past the finished compaction.
    assert!(!terminal_tail_has_compacting_status(
        "Compacting context (9s • esc to interrupt)\n• Working (2s • esc to interrupt)"
    ));
    assert!(!terminal_tail_has_compacting_status(
        "Compacting context (9s • esc to interrupt)\n✻ Worked for 12s"
    ));
    // Merely typing /compact must never look like a running compaction.
    assert!(!terminal_tail_has_compacting_status("/compact"));
    assert!(!terminal_tail_has_compacting_status("› /compact"));
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
fn terminal_working_status_detection_matches_codebuddy_progress() {
    for status in [
        "∴ Thinking...",
        "✷ Thinking… (0s)",
        "✸ 检查终端状态… (31s · waiting for response)",
        "✻ 执行测试… (120s · tool execution · ctrl+t to show todos)",
        "✶ 检查结果... (2s · ↑ 1.2k tokens)",
    ] {
        assert!(terminal_tail_has_working_status(status), "{status}");
    }
    for frame in ['✶', '✸', '✹', '✺', '✹', '✷'] {
        let status = format!("{frame} 检查结果… (0s)");
        assert!(terminal_tail_has_working_status(&status), "{status}");
    }
    for status in [
        "∴ Thought",
        "✔ Worked for 8m 15s",
        "✸ 检查结果… (completed)",
        "> 检查结果… (31s)",
        "The agent is thinking... (31s)",
    ] {
        assert!(!terminal_tail_has_working_status(status), "{status}");
    }
}

#[test]
fn terminal_working_status_detection_matches_agy_progress() {
    for status in [
        "Thinking...",
        "⠋ Thinking... (31s)",
        "Working... (2m 11s, esc to cancel)",
        "Generating… (esc to cancel)",
        "esc to cancel",
        "Press esc to interrupt generation.",
        "● Running (bash scripts/build-unified-release.sh) (ctrl+o to expand)",
        "● Running (go test -tags unit ./internal/service ./internal/handler/admin) (ctrl+o to expand)",
        "● Running (git status -s)",
        "  ● [19:31:02] bash scripts/build-unified-release.sh running",
        "● [19:31:02] bash scripts/build-unified-release.sh running",
        "[19:31:02] bash scripts/build-unified-release.sh running",
    ] {
        assert!(terminal_tail_has_working_status(status), "{status}");
    }
    for status in [
        "○ Thought for 7.0s (The deployment is queued...)",
        "○ Thought for 7.1s (Internal service and admin handler tests passed, as did the frontend account management and scheduling tests. The next step involves reviewing the deployment checklist to ensure all code changes are finalized f...)",
        "▸ Thought for 3s, 261 tokens",
        "? for shortcuts    Gemini 3.8 Flash · high",
        "? for shortcuts",
        "Working directory: /tmp/project",
        "Tell the user to press esc to cancel",
        "> Thinking...",
        "┃ esc to dismiss",
        "● Ran (git status -s)",
        "● Ran (go test -tags unit ./internal/service ./internal/handler/admin) (ctrl+o to expand)",
        "● Ran (bash scripts/build-unified-release.sh) (ctrl+o to expand)",
    ] {
        assert!(!terminal_tail_has_working_status(status), "{status}");
    }
}

#[test]
fn terminal_working_status_detection_matches_agy_tool_execution_and_background_status() {
    let running_snapshot = r#"
● Ran (go test -tags unit ./internal/service ./internal/handler/admin) (ctrl+o to expand)

  I will wait for the backend tests to complete.

○ Thought for 7.1s (Internal service and admin handler tests passed, as did the frontend account management and scheduling tests. The next step involves reviewing the deployment checklist to ensure all code changes are finalized f...)
● Ran (git status -s)
● Running (bash scripts/build-unified-release.sh) (ctrl+o to expand)

  I will wait for the unified release build to complete.

───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
>
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
  ● [19:31:02] bash scripts/build-unified-release.sh running
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
? for shortcuts                       
"#;
    assert!(terminal_working_status_match_from_snapshot(running_snapshot.as_bytes(), 10));

    // When the background command finishes, agy removes the bottom statusline
    // and updates Running to Ran.
    let finished_snapshot = r#"
● Ran (go test -tags unit ./internal/service ./internal/handler/admin) (ctrl+o to expand)

  I will wait for the backend tests to complete.

○ Thought for 7.1s (Internal service and admin handler tests passed, as did the frontend account management and scheduling tests. The next step involves reviewing the deployment checklist to ensure all code changes are finalized f...)
● Ran (git status -s)
● Ran (bash scripts/build-unified-release.sh) (ctrl+o to expand)

  I will wait for the unified release build to complete.

───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
>
───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────
? for shortcuts                       
"#;
    assert!(!terminal_working_status_match_from_snapshot(finished_snapshot.as_bytes(), 10));
}

#[test]
fn terminal_working_status_detection_prefers_the_latest_turn_status() {
    assert!(!terminal_tail_has_working_status("∴ Thinking...\n✔ Worked for 8m 15s\n>"));
    assert!(terminal_tail_has_working_status(
        "✔ Worked for 8m 15s\n> Continue\n✻ 检查结果… (31s)"
    ));
    assert!(terminal_tail_has_working_status(
        "✔ Worked for 8m 15s\n> Inspect the remaining files\nLet's inspect environment. (12s • esc to interrupt)"
    ));
    assert!(!terminal_tail_has_working_status(
        "Let's inspect environment. (12s • esc to interrupt)\n✔ Worked for 8m 15s"
    ));
}

#[test]
fn terminal_working_status_detection_ignores_unused_pane_rows() {
    let padding = "\n   \n".repeat(30);
    let snapshot = format!("∴ Thinking...\n────\n>\n────\npermissions on{padding}");
    assert!(terminal_working_status_match_from_snapshot(snapshot.as_bytes(), 10));

    let snapshot = format!("✔ Worked for 8m 15s\n────\n>\n────\npermissions on{padding}");
    assert!(!terminal_working_status_match_from_snapshot(snapshot.as_bytes(), 10));
    assert!(terminal_worked_status_match_from_snapshot(snapshot.as_bytes(), 10));

    let snapshot = format!("∴ Thinking...\n{}", "newer output\n".repeat(10));
    assert!(!terminal_working_status_match_from_snapshot(snapshot.as_bytes(), 10));
}

#[test]
fn terminal_agent_activity_preserves_working_during_output_pauses() {
    for (agent, status, completion) in [
        ("CodeBuddy", "∴ Thinking...", "✔ Worked for 8m 15s"),
        ("CodeBuddy", "✸ 执行测试… (120s · tool execution)", "✔ Worked for 2m"),
        ("AGY", "Thinking... (31s)", "○ Thought for 31s\n? for shortcuts"),
        ("AGY", "esc to cancel", "? for shortcuts    Gemini 3.8 Flash · high"),
    ] {
        for last_viewed_output_at in [0, 42] {
            let stored = stored_test_terminal_session("agent-paused-output");
            let mut state = TerminalState::default();
            state
                .sessions_by_id
                .insert(stored.id.clone(), stored.clone());
            state.output_observations.insert(
                stored.id.clone(),
                super::TerminalOutputObservation {
                    fingerprint: Some(7),
                    last_fingerprint_probe_sequence: 1,
                    rebaseline_after_restore: false,
                    last_output_at: 42,
                    last_viewed_output_at,
                },
            );
            let snapshot = format!("{status}\n────\n>\n────{}", "\n \n".repeat(30));
            let mut probe = TerminalActivityProbe {
                session_id: stored.id.clone(),
                live_last_output_at: 42,
                snapshot_fingerprint: Some(7),
                snapshot_probe_sequence: 2,
                agent_activity: TerminalAgentActivity {
                    agents: vec![agent.to_string()],
                    orphaned_agents: Vec::new(),
                },
                working_status: terminal_working_status_match_from_snapshot(
                    snapshot.as_bytes(),
                    10,
                ),
                cline_idle_prompt: false,
                error_match: None,
                worked_status: terminal_worked_status_match_from_snapshot(snapshot.as_bytes(), 10),
                pending_build: false,
                compacting_status: false,
                model_family: String::new(),
            };

            // Repeated identical probes are already outside the output window;
            // viewing output must not clear a still-active turn either.
            for sequence in 2..=4 {
                probe.snapshot_probe_sequence = sequence;
                let (sessions, _) = collect_session_infos_from_probes_locked(
                    &mut state,
                    &stored.path,
                    &stored.path,
                    vec![probe.clone()],
                );
                assert_eq!(sessions[0].activity_state, "working", "{agent}: {status}");
                assert_eq!(sessions[0].activity_agent.as_deref(), Some(agent));
                assert!(sessions[0].busy);
                assert_eq!(sessions[0].last_output_at, 42);
            }

            probe.working_status =
                terminal_working_status_match_from_snapshot(completion.as_bytes(), 10);
            probe.worked_status =
                terminal_worked_status_match_from_snapshot(completion.as_bytes(), 10);
            probe.snapshot_probe_sequence = 5;
            let (sessions, _) = collect_session_infos_from_probes_locked(
                &mut state,
                &stored.path,
                &stored.path,
                vec![probe],
            );
            assert_eq!(
                sessions[0].activity_state,
                if last_viewed_output_at == 0 {
                    "completed"
                } else {
                    "agent"
                },
                "{agent}: {completion}",
            );
            assert!(!sessions[0].busy);
        }
    }
}

#[test]
fn terminal_working_status_detection_recognizes_cline_idle_prompt() {
    assert!(!terminal_tail_has_working_status(
        "▶ Thinking: ...处理中...\n\n❯ Plan something..."
    ));
    assert!(!terminal_tail_has_working_status("❯ Plan something..."));
    assert!(!terminal_tail_has_working_status("❯ Ask anything..."));
    assert!(!terminal_tail_has_working_status("❯ Ask anything…"));
    assert!(!terminal_tail_has_working_status(
        "▶ Thinking: 处理中…
❯ Ask anything...
Space Bunny Alpha (free) (xhigh) ██████ (44,733) $0.00  ○ Plan ● Act (Tab)
codes (main) | 410 files +19083 -110717
⏵⏵ Auto-approve all enabled (Shift+Tab)"
    ));
    assert!(!terminal_tail_has_working_status("Thinking about the answer…"));
    // A visible input placeholder must not suppress real ongoing work.
    assert!(terminal_tail_has_working_status(
        "✻ Thinking…
❯ Ask anything..."
    ));
    assert!(terminal_tail_has_working_status(
        "• Working (12s • esc to interrupt)
❯ Plan something..."
    ));
    assert!(terminal_tail_has_working_status("✻ Thinking..."));
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
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: workspace_dir,
            user_name: default_terminal_user_name(),
            name: "custom".to_string(),
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: true,
            idle: false,
            created_at: 2,
            last_opened_at: 2,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );

    let error = ensure_unique_session_name_locked(&state, "workspace_1", Some("s2"))
        .expect_err("duplicate name should be rejected");
    assert!(error.to_string().contains("已存在"), "unexpected error: {error}");
    ensure_unique_session_name_locked(&state, "workspace_1", Some("s1"))
        .expect("same session should keep its current name");
}

#[test]
fn renamed_terminal_preserves_old_name_alias_and_delivery_notice() {
    let mut state = TerminalState::default();
    state.sessions_by_id.insert(
        "s1".to_string(),
        StoredTerminalSession {
            id: "s1".to_string(),
            path: std::path::PathBuf::from("/tmp/workspace"),
            user_name: default_terminal_user_name(),
            name: "old".to_string(),
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );

    let renamed = super::manager::rename_session_for_test(&mut state, "s1", "new").unwrap();
    assert_eq!(renamed.name, "new");
    assert_eq!(renamed.previous_names, vec!["old".to_string()]);
    assert_eq!(state.previous_terminal_names.get("old").map(String::as_str), Some("s1"));

    let resolved =
        super::manager::resolve_session_target_for_test(&mut state, "old", None).unwrap();
    assert_eq!(resolved, ("s1".to_string(), "new".to_string()));
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
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: true,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: workspace_dir,
            user_name: default_terminal_user_name(),
            name: "custom".to_string(),
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: true,
            idle: false,
            created_at: 2,
            last_opened_at: 2,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
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
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 10,
            last_opened_at: 20,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: path.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_2".to_string(),
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 11,
            last_opened_at: 40,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );
    state.sessions_by_id.insert(
        "s3".to_string(),
        StoredTerminalSession {
            id: "s3".to_string(),
            path,
            user_name: default_terminal_user_name(),
            name: "workspace_3".to_string(),
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 30,
            last_opened_at: 40,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );

    let mut session_ids = vec!["s1".to_string(), "s2".to_string(), "s3".to_string()];
    sort_session_ids_by_recent_activity(&state, &mut session_ids);
    assert_eq!(session_ids, vec!["s3", "s2", "s1"]);
}

#[test]
fn grouped_sessions_follow_their_recently_activated_partner() {
    let mut state = TerminalState::default();
    let path = std::path::PathBuf::from("/tmp/workspace");
    let make_session = |state: &mut TerminalState,
                        id: &str,
                        created_at: u64,
                        last_opened_at: u64,
                        group_token: &str| {
        let mut session = stored_test_terminal_session(id);
        session.path = path.clone();
        session.created_at = created_at;
        session.last_opened_at = last_opened_at;
        session.group_token = group_token.to_string();
        state.sessions_by_id.insert(id.to_string(), session);
    };

    make_session(&mut state, "s1", 10, 20, "");
    make_session(&mut state, "s2", 11, 100, "pair-a");
    make_session(&mut state, "s3", 30, 40, "pair-a");

    let mut session_ids = vec!["s1".to_string(), "s2".to_string(), "s3".to_string()];
    sort_session_ids_by_recent_activity(&state, &mut session_ids);
    assert_eq!(
        session_ids,
        vec!["s2", "s3", "s1"],
        "opening s2 should move its grouped follower beside it"
    );
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
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: true,
            created_at: 1,
            last_opened_at: 1,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
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
        previous_names: Vec::new(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        hidden: false,
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
        previous_names: Vec::new(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        hidden: false,
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
        cline_idle_prompt: false,
        error_match: None,
        worked_status: false,
        pending_build: false,
        compacting_status: false,
        model_family: String::new(),
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
fn activity_probe_reuses_cached_snapshot_within_seq_attachment_and_age() {
    // 假会话 id 用非 `s<digits>` 形式，避免与并行测试创建的真实
    // `webclx_s{n}` tmux 会话同名导致 capture 拿到真实 pane 字节。
    let stored = stored_test_terminal_session("probe-fake-a");
    let live = build_live_session(&stored);
    live.next_output_seq.store(5, Ordering::SeqCst);

    let cache = Mutex::new(ProbeSnapshotCache::new());
    cache.lock().expect("probe snapshot cache poisoned").insert(
        stored.id.clone(),
        probe_snapshot_cache_entry(5, live.attached_at, Instant::now(), Some(0x5eed)),
    );

    // seq + attachment + 年龄全部匹配：命中缓存，capture 闭包不得执行。
    let captures = AtomicUsize::new(0);
    let (snapshot, fingerprint) =
        probe_activity_pane_snapshot_with(&cache, &stored.id, Some(&live), 80, || {
            captures.fetch_add(1, Ordering::SeqCst);
            Some(b"fresh pane".to_vec())
        });
    assert_eq!(captures.load(Ordering::SeqCst), 0);
    assert_eq!(snapshot.as_deref(), Some(b"cached pane bytes".as_ref()));
    assert_eq!(fingerprint, Some(0x5eed));

    // 缓存命中仍每轮重跑 agent_detector 与快照派生匹配（working/error/
    // worked/model_family）：把缓存快照换成含 Working 行的内容验证派生。
    cache.lock().unwrap().get_mut(&stored.id).unwrap().snapshot =
        Some("Working (3s • esc to interrupt)\n".as_bytes().to_vec());
    let probes = collect_session_activity_probes(
        vec![(stored.id.clone(), Some(live.clone()))],
        80,
        &[],
        &[],
        true,
        &cache,
    );
    assert_eq!(probes.len(), 1);
    assert_eq!(probes[0].snapshot_fingerprint, Some(0x5eed));
    assert!(probes[0].working_status);
    assert!(probes[0].error_match.is_none());
    assert!(!probes[0].worked_status);
    // agent_detector 无条件执行：假会话无真实 agent 进程，探测结果为空。
    assert!(probes[0].agent_activity.agents.is_empty());
}

fn probe_snapshot_cache_entry(
    live_output_seq: u64,
    attachment_started_at: Instant,
    captured_at: Instant,
    fingerprint: Option<u64>,
) -> ProbeSnapshotCacheEntry {
    ProbeSnapshotCacheEntry {
        live_output_seq,
        attachment_started_at,
        captured_at,
        error_line_limit: 80,
        snapshot: Some(b"cached pane bytes".to_vec()),
        snapshot_fingerprint: fingerprint,
    }
}

#[test]
fn activity_probe_snapshot_reuse_is_bounded_and_per_session() {
    let stored = stored_test_terminal_session("probe-fake-b");
    let live = build_live_session(&stored);
    live.next_output_seq.store(7, Ordering::SeqCst);

    let cache = Mutex::new(ProbeSnapshotCache::new());
    let captures = AtomicUsize::new(0);
    let probe = |live_session: Option<&Arc<super::TerminalSession>>, limit: u32| {
        probe_activity_pane_snapshot_with(&cache, &stored.id, live_session, limit, || {
            captures.fetch_add(1, Ordering::SeqCst);
            Some(b"fresh pane".to_vec())
        })
    };

    // 条目年龄超过上限：重新抓取。
    let expired = Instant::now()
        .checked_sub(PROBE_SNAPSHOT_MAX_AGE + Duration::from_secs(1))
        .unwrap_or_else(Instant::now);
    cache.lock().unwrap().insert(
        stored.id.clone(),
        probe_snapshot_cache_entry(7, live.attached_at, expired, Some(1)),
    );
    let (snapshot, _) = probe(Some(&live), 80);
    assert_eq!(captures.load(Ordering::SeqCst), 1);
    assert_eq!(snapshot.as_deref(), Some(b"fresh pane".as_ref()));

    // seq 推进：重新抓取。
    live.next_output_seq.store(8, Ordering::SeqCst);
    let _ = probe(Some(&live), 80);
    assert_eq!(captures.load(Ordering::SeqCst), 2);

    // attachment 更换（同 id 新实例）：重新抓取。
    let replaced = build_live_session_with_redraw_state(
        &stored,
        live.attached_at + Duration::from_millis(1),
        true,
    );
    replaced.next_output_seq.store(8, Ordering::SeqCst);
    let _ = probe(Some(&replaced), 80);
    assert_eq!(captures.load(Ordering::SeqCst), 3);

    // error_line_limit 变化：重新抓取（它同时决定 capture 深度）。
    let _ = probe(Some(&replaced), 200);
    assert_eq!(captures.load(Ordering::SeqCst), 4);

    // 其它会话的条目按 session_id 隔离：本会话命中自己的新条目（不再
    // capture），probe-fake-c 的同名 attachment/seq 条目不被命中也不被改写。
    cache.lock().unwrap().insert(
        "probe-fake-c".to_string(),
        probe_snapshot_cache_entry(8, replaced.attached_at, Instant::now(), Some(9)),
    );
    let (snapshot, fingerprint) = probe(Some(&replaced), 200);
    assert_eq!(captures.load(Ordering::SeqCst), 4);
    assert_eq!(snapshot.as_deref(), Some(b"fresh pane".as_ref()));
    assert!(fingerprint.is_some());
    assert!(
        cache
            .lock()
            .unwrap()
            .get("probe-fake-c")
            .is_some_and(|entry| entry.snapshot_fingerprint == Some(9))
    );
}

#[test]
fn activity_probe_snapshot_seq_change_during_capture_is_not_cached() {
    let stored = stored_test_terminal_session("probe-fake-d");
    let live = build_live_session(&stored);
    live.next_output_seq.store(5, Ordering::SeqCst);
    let cache = Mutex::new(ProbeSnapshotCache::new());

    let live_for_capture = live.clone();
    let (snapshot, _) =
        probe_activity_pane_snapshot_with(&cache, &stored.id, Some(&live), 80, || {
            // 抓取期间 PTY 又产生输出：seq 推进意味着快照不再是当前 pane。
            live_for_capture.next_output_seq.store(6, Ordering::SeqCst);
            Some(b"raced pane".to_vec())
        });
    assert_eq!(snapshot.as_deref(), Some(b"raced pane".as_ref()));
    assert!(
        cache.lock().unwrap().get(&stored.id).is_none(),
        "seq moved during capture must not populate the cache"
    );

    // seq 稳定后再次抓取正常写入缓存。
    let (snapshot, _) =
        probe_activity_pane_snapshot_with(&cache, &stored.id, Some(&live), 80, || {
            Some(b"stable pane".to_vec())
        });
    assert_eq!(snapshot.as_deref(), Some(b"stable pane".as_ref()));
    assert!(cache.lock().unwrap().get(&stored.id).is_some());
}

#[test]
fn activity_probe_failed_capture_clears_stale_entry_and_retries() {
    let stored = stored_test_terminal_session("probe-fake-e");
    let live = build_live_session(&stored);
    live.next_output_seq.store(3, Ordering::SeqCst);
    let cache = Mutex::new(ProbeSnapshotCache::new());
    // seq 不一致的旧条目：不可命中，但抓取失败时必须清除。
    cache.lock().unwrap().insert(
        stored.id.clone(),
        probe_snapshot_cache_entry(9, live.attached_at, Instant::now(), Some(2)),
    );

    let (snapshot, fingerprint) =
        probe_activity_pane_snapshot_with(&cache, &stored.id, Some(&live), 80, || None);
    assert!(snapshot.is_none());
    assert_eq!(fingerprint, None);
    // 失败清除旧条目：下一轮必须重试，不得复用更旧的快照。
    assert!(cache.lock().unwrap().get(&stored.id).is_none());

    let attempts = AtomicUsize::new(0);
    let (snapshot, _) =
        probe_activity_pane_snapshot_with(&cache, &stored.id, Some(&live), 80, || {
            attempts.fetch_add(1, Ordering::SeqCst);
            Some(b"recovered pane".to_vec())
        });
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
    assert!(snapshot.is_some());
    assert!(cache.lock().unwrap().get(&stored.id).is_some());
}

#[test]
fn activity_probe_snapshot_never_reuses_for_detached_or_zero_seq() {
    let stored = stored_test_terminal_session("probe-fake-f");
    // seq=0 的 live attachment：无有效水位线，永不复用也不写缓存。
    let live = build_live_session(&stored);
    let cache = Mutex::new(ProbeSnapshotCache::new());
    cache.lock().unwrap().insert(
        stored.id.clone(),
        probe_snapshot_cache_entry(0, live.attached_at, Instant::now(), Some(4)),
    );

    let captures = AtomicUsize::new(0);
    let (snapshot, _) =
        probe_activity_pane_snapshot_with(&cache, &stored.id, Some(&live), 80, || {
            captures.fetch_add(1, Ordering::SeqCst);
            Some(b"fresh pane".to_vec())
        });
    assert_eq!(captures.load(Ordering::SeqCst), 1);
    assert_eq!(snapshot.as_deref(), Some(b"fresh pane".as_ref()));
    // seq=0 不写缓存：条目未被命中，抓取结果为新快照。
    assert!(
        cache
            .lock()
            .unwrap()
            .get(&stored.id)
            .is_none_or(|entry| entry.snapshot.as_deref() != Some(b"fresh pane".as_ref()))
    );

    // detached（live=None）不复用并清掉可能过期的条目。
    cache.lock().unwrap().insert(
        stored.id.clone(),
        probe_snapshot_cache_entry(0, Instant::now(), Instant::now(), Some(5)),
    );
    let (snapshot, _) = probe_activity_pane_snapshot_with(&cache, &stored.id, None, 80, || {
        captures.fetch_add(1, Ordering::SeqCst);
        Some(b"detached pane".to_vec())
    });
    assert_eq!(captures.load(Ordering::SeqCst), 2);
    assert_eq!(snapshot.as_deref(), Some(b"detached pane".as_ref()));
    assert!(cache.lock().unwrap().get(&stored.id).is_none());
}

#[test]
fn purge_session_runtime_state_drops_probe_snapshot_cache_entry() {
    let (manager, base_dir) = build_test_terminal_manager();
    manager
        .probe_snapshot_cache
        .lock()
        .expect("probe snapshot cache poisoned")
        .insert(
            "probe-fake-a".to_string(),
            probe_snapshot_cache_entry(0, Instant::now(), Instant::now(), Some(1)),
        );

    manager.purge_session_runtime_state("probe-fake-a");

    assert!(
        manager
            .probe_snapshot_cache
            .lock()
            .expect("probe snapshot cache poisoned")
            .get("probe-fake-a")
            .is_none()
    );
    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn pending_build_overrides_unviewed_agent_completion() {
    let workspace_dir = std::path::PathBuf::from("/tmp/webclx-terminal-pending-build-test");
    let stored = StoredTerminalSession {
        id: "s1".to_string(),
        path: workspace_dir.clone(),
        user_name: default_terminal_user_name(),
        name: "workspace_1".to_string(),
        previous_names: Vec::new(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        hidden: false,
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
        cline_idle_prompt: false,
        error_match: None,
        worked_status: true,
        pending_build: true,
        compacting_status: false,
        model_family: "deepseek".to_string(),
    }];
    let (sessions, _) = collect_session_infos_from_probes_locked(
        &mut state,
        &workspace_dir,
        &workspace_dir,
        probes,
    );

    assert_eq!(sessions[0].activity_state, "building");
    assert_eq!(sessions[0].activity_label, "编译中");
    assert_eq!(sessions[0].model_family, "");
    assert!(sessions[0].busy);
}

#[test]
fn compacting_pane_reports_compacting_state_and_typed_command_does_not() {
    let stored = stored_test_terminal_session("compacting");
    let mut state = TerminalState::default();
    state
        .sessions_by_id
        .insert(stored.id.clone(), stored.clone());

    let compacting_snapshot = "Compacting context (9s • esc to interrupt)\n  └ Making room to continue.\n\n› Ask Codex to do anything";
    let mut probe = TerminalActivityProbe {
        session_id: stored.id.clone(),
        live_last_output_at: 42,
        snapshot_fingerprint: Some(7),
        snapshot_probe_sequence: 1,
        agent_activity: TerminalAgentActivity {
            agents: vec!["Codex".to_string()],
            orphaned_agents: Vec::new(),
        },
        working_status: terminal_working_status_match_from_snapshot(
            compacting_snapshot.as_bytes(),
            10,
        ),
        compacting_status: terminal_compacting_status_from_snapshot(
            compacting_snapshot.as_bytes(),
            15,
        ),
        cline_idle_prompt: false,
        error_match: None,
        worked_status: false,
        pending_build: false,
        model_family: String::new(),
    };
    let (sessions, _) = collect_session_infos_from_probes_locked(
        &mut state,
        &stored.path,
        &stored.path,
        vec![probe.clone()],
    );
    assert_eq!(sessions[0].activity_state, "compacting");
    assert_eq!(sessions[0].activity_label, "压缩中");
    assert!(sessions[0].busy);

    // A typed but unexecuted /compact leaves no compaction progress line, so
    // the session must never be reported as compacting.
    let typed_snapshot = "› /compact\n\n  GPT-6.1-Sol high · Context 45% used · Fast off";
    assert!(!terminal_compacting_status_from_snapshot(typed_snapshot.as_bytes(), 15));
    probe.compacting_status =
        terminal_compacting_status_from_snapshot(typed_snapshot.as_bytes(), 15);
    probe.working_status =
        terminal_working_status_match_from_snapshot(typed_snapshot.as_bytes(), 10);
    probe.snapshot_probe_sequence = 2;
    let (sessions, _) = collect_session_infos_from_probes_locked(
        &mut state,
        &stored.path,
        &stored.path,
        vec![probe],
    );
    assert_ne!(sessions[0].activity_state, "compacting");
}

#[test]
fn orphaned_agent_remains_abnormal_after_output_is_viewed() {
    let workspace_dir = std::path::PathBuf::from("/tmp/webclx-terminal-orphaned-agent-test");
    let stored = StoredTerminalSession {
        id: "s1".to_string(),
        path: workspace_dir.clone(),
        user_name: default_terminal_user_name(),
        name: "workspace_1".to_string(),
        previous_names: Vec::new(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        hidden: false,
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
            last_fingerprint_probe_sequence: 1,
            rebaseline_after_restore: false,
            last_output_at: 42,
            last_viewed_output_at: 42,
        },
    );

    let probes = vec![TerminalActivityProbe {
        session_id: stored.id.clone(),
        live_last_output_at: 42,
        snapshot_fingerprint: Some(7),
        snapshot_probe_sequence: 2,
        agent_activity: TerminalAgentActivity {
            agents: vec!["Codex".to_string()],
            orphaned_agents: vec!["Codex".to_string()],
        },
        working_status: false,
        cline_idle_prompt: false,
        error_match: None,
        worked_status: false,
        pending_build: false,
        compacting_status: false,
        model_family: "glm".to_string(),
    }];
    let (sessions, _) = collect_session_infos_from_probes_locked(
        &mut state,
        &workspace_dir,
        &workspace_dir,
        probes,
    );

    assert_eq!(sessions[0].activity_state, "orphaned_agent");
    assert_eq!(sessions[0].activity_label, "Agent 异常");
    assert_eq!(sessions[0].activity_agent.as_deref(), Some("Codex"));
    assert_eq!(sessions[0].model_family, "");
    assert!(sessions[0].busy);
}

#[test]
fn idle_agent_process_does_not_report_running_or_busy() {
    let workspace_dir = std::path::PathBuf::from("/tmp/webclx-terminal-idle-agent-test");
    let stored = StoredTerminalSession {
        id: "s1".to_string(),
        path: workspace_dir.clone(),
        user_name: default_terminal_user_name(),
        name: "workspace_1".to_string(),
        previous_names: Vec::new(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        hidden: false,
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
            last_fingerprint_probe_sequence: 1,
            rebaseline_after_restore: false,
            last_output_at: 42,
            last_viewed_output_at: 42,
        },
    );

    let probes = vec![TerminalActivityProbe {
        session_id: stored.id.clone(),
        live_last_output_at: 42,
        snapshot_fingerprint: Some(7),
        snapshot_probe_sequence: 2,
        agent_activity: TerminalAgentActivity {
            agents: vec!["Cline".to_string()],
            orphaned_agents: Vec::new(),
        },
        working_status: false,
        cline_idle_prompt: false,
        error_match: None,
        worked_status: false,
        pending_build: false,
        compacting_status: false,
        model_family: String::new(),
    }];
    let (sessions, _) = collect_session_infos_from_probes_locked(
        &mut state,
        &workspace_dir,
        &workspace_dir,
        probes,
    );

    assert_eq!(sessions[0].activity_state, "agent");
    assert_eq!(sessions[0].activity_label, "空闲");
    assert_eq!(sessions[0].activity_agent.as_deref(), Some("Cline"));
    assert!(!sessions[0].busy);
}

#[test]
fn cline_idle_prompt_suppresses_pending_review_after_output_window() {
    let workspace_dir = std::path::PathBuf::from("/tmp/webclx-terminal-cline-idle-test");
    let stored = StoredTerminalSession {
        id: "s1".to_string(),
        path: workspace_dir.clone(),
        user_name: default_terminal_user_name(),
        name: "workspace_1".to_string(),
        previous_names: Vec::new(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        hidden: false,
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
        snapshot_fingerprint: Some(7),
        snapshot_probe_sequence: 1,
        agent_activity: TerminalAgentActivity {
            agents: vec!["Cline".to_string()],
            orphaned_agents: Vec::new(),
        },
        working_status: false,
        cline_idle_prompt: true,
        error_match: None,
        worked_status: false,
        pending_build: false,
        compacting_status: false,
        model_family: String::new(),
    }];
    let (sessions, _) = collect_session_infos_from_probes_locked(
        &mut state,
        &workspace_dir,
        &workspace_dir,
        probes,
    );

    assert_eq!(sessions[0].activity_state, "agent");
    assert_eq!(sessions[0].activity_label, "空闲");
    assert_eq!(sessions[0].activity_agent.as_deref(), Some("Cline"));
    assert_eq!(sessions[0].last_output_at, 42);
    assert!(!sessions[0].busy);
    assert_eq!(
        state
            .output_observations
            .get(&stored.id)
            .expect("output observation should exist")
            .last_viewed_output_at,
        42,
        "the idle Cline prompt is itself evidence that its latest output was observed"
    );
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
        previous_names: Vec::new(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        hidden: false,
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
        cline_idle_prompt: false,
        error_match: None,
        worked_status: false,
        pending_build: false,
        compacting_status: false,
        model_family: String::new(),
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
        cline_idle_prompt: false,
        error_match: None,
        worked_status: false,
        pending_build: false,
        compacting_status: false,
        model_family: "glm".to_string(),
    };
    let (sessions, _) = collect_session_infos_from_probes_locked(
        &mut state,
        &workspace_dir,
        &workspace_dir,
        vec![idle_redraw_probe],
    );

    assert_eq!(sessions[0].activity_state, "idle");
    assert_eq!(sessions[0].model_family, "glm");
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
        previous_names: Vec::new(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        hidden: false,
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
        cline_idle_prompt: false,
        error_match: None,
        worked_status: false,
        pending_build: false,
        compacting_status: false,
        model_family: String::new(),
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
        cline_idle_prompt: false,
        error_match: None,
        worked_status: false,
        pending_build: false,
        compacting_status: false,
        model_family: String::new(),
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
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1".to_string(),
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: true,
            idle: false,
            created_at: 2,
            last_opened_at: 2,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
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
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1 codex".to_string(),
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: true,
            idle: false,
            created_at: 2,
            last_opened_at: 2,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
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
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );
    state.sessions_by_id.insert(
        "s3".to_string(),
        StoredTerminalSession {
            id: "s3".to_string(),
            path: workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_3".to_string(),
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 3,
            last_opened_at: 3,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
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
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );
    let duplicate = StoredTerminalSession {
        id: "s2".to_string(),
        path: workspace_dir.clone(),
        user_name: default_terminal_user_name(),
        name: "workspace_1".to_string(),
        previous_names: Vec::new(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 2,
        last_opened_at: 2,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        hidden: false,
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
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: second_workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "workspace_1".to_string(),
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 2,
            last_opened_at: 2,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
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
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 1,
            last_opened_at: 1,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );
    state.sessions_by_id.insert(
        "s2".to_string(),
        StoredTerminalSession {
            id: "s2".to_string(),
            path: second_workspace_dir.clone(),
            user_name: default_terminal_user_name(),
            name: "beta_9".to_string(),
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 2,
            last_opened_at: 2,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
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
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 5,
            last_opened_at: 5,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
        },
    );
    state.sessions_by_id.insert(
        "s12".to_string(),
        StoredTerminalSession {
            id: "s12".to_string(),
            path: other_workspace_dir,
            user_name: default_terminal_user_name(),
            name: "workspace_12".to_string(),
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 12,
            last_opened_at: 12,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
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
            previous_names: Vec::new(),
            title: String::new(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: super::TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            manually_renamed: false,
            idle: false,
            created_at: 5,
            last_opened_at: 5,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
            hidden: false,
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
    let output = isolated_tmux_command(&session_id)
        .arg("new-session")
        .arg("-d")
        .arg("-s")
        .arg(tmux_session_name(&session_id))
        .arg("-c")
        .arg(&workspace_dir)
        .output()
        .expect("create orphan tmux session");
    assert_tmux_command_succeeded(output, "create orphan tmux session");
    let output = isolated_tmux_command(&session_id)
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(&session_id))
        .arg("-l")
        .arg(format!("printf '{orphan_marker}'"))
        .output()
        .expect("send orphan marker");
    assert_tmux_command_succeeded(output, "send orphan marker");
    let output = isolated_tmux_command(&session_id)
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
        program: "claude".to_string(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: 1,
        last_opened_at: 1,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        input_history: Vec::new(),
        resume_id: "019d1ba6-f772-7452-a391-6553ccbc0a50".to_string(),
        command: "claude --resume 019d1ba6-f772-7452-a391-6553ccbc0a50".to_string(),
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
                group_token: String::new(),
                group_color: String::new(),
                group_name: String::new(),
                group_role: String::new(),
                group_order: 0,
                group_fixed_order: false,
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
        previous_names: Vec::new(),
        title: String::new(),
        codex_api_preset_name: String::new(),
        codex_api_base_url: String::new(),
        origin: super::TerminalSessionOrigin::Normal,
        owner_key: String::new(),
        manually_renamed: false,
        idle: false,
        created_at: unique,
        last_opened_at: unique,
        group_token: String::new(),
        group_color: String::new(),
        group_name: String::new(),
        group_role: String::new(),
        group_order: 0,
        group_fixed_order: false,
        hidden: false,
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
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
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
            None,
            String::new(),
            String::new(),
            true,
        )
        .expect("attach live browser session");
    let resume_id = "ad08e570-051b-4b66-8c7a-7b20e434b168";
    manager.record_session_input(&created.id, "first saved user prompt\n");
    manager.record_session_input(&created.id, "second saved user prompt\n");
    isolated_tmux_command(&created.id)
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(&created.id))
        .arg(format!("echo 'Resume this session with: codex resume {resume_id}'"))
        .output()
        .expect("echo codex resume prompt");
    isolated_tmux_command(&created.id)
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

/// 信号触发的自动保存（SIGTERM/SIGINT 路径）必须与菜单命令写入同一份恢复注册表：
/// 终端里可见 resume 提示时，`save_shutdown_restore_registry_for_signal` 也要落盘记录。
#[test]
fn signal_shutdown_save_persists_resume_records() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;
    let base_dir = std::env::temp_dir().join(format!("webclx-signal-save-resume-{unique}"));
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
            None,
            String::new(),
            String::new(),
            true,
        )
        .expect("attach live browser session");
    let resume_id = "5f0e2b41-8c1d-4f29-9f2f-91e7df2a3b70";
    isolated_tmux_command(&created.id)
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(&created.id))
        .arg(format!("echo 'Resume this session with: codex resume {resume_id}'"))
        .output()
        .expect("echo codex resume prompt");
    isolated_tmux_command(&created.id)
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(&created.id))
        .arg("C-m")
        .output()
        .expect("submit echo");

    let mut detected = None;
    for _ in 0..30 {
        if let Ok(Some(value)) = manager.current_resume_session(&created.id)
            && value.info.resume_id == resume_id
        {
            detected = Some(value);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    detected.expect("resume command should be detected from terminal buffer");
    assert_eq!(
        manager
            .save_shutdown_restore_registry_for_signal()
            .expect("signal-triggered shutdown save"),
        1
    );

    let restore_file = state_file.with_file_name(super::TERMINAL_SHUTDOWN_RESTORE_FILE_NAME);
    let restore_registry =
        super::load_terminal_shutdown_restore_registry(&restore_file).expect("load restore file");
    assert_eq!(restore_registry.records.len(), 1);
    assert_eq!(restore_registry.records[0].resume_id, resume_id);
    assert_eq!(restore_registry.records[0].session_id, created.id);

    let _ = manager.delete_session(&workspace_dir, &workspace_dir, &created.id);
    let _ = fs::remove_dir_all(base_dir);
}

/// tmux scope 是为了服务重启时保住终端；但在整机关机事务里，它必须晚于
/// webclx.service 停止，否则信号保存会先失去 tmux/agent 进程。这里做源码
/// 回归，不额外创建 systemd transient unit。
#[test]
fn tmux_scope_is_ordered_after_webclx_service_for_shutdown_save() {
    let source = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/terminal/tmux.rs"))
        .expect("read tmux source");
    assert!(
        source.contains(".arg(\"--property=After=webclx.service\")"),
        "tmux transient scope must be ordered after webclx.service"
    );
}

/// 关机竞态中 tmux 后端可能先于本进程停止，信号保存探测不到任何会话；此时不能
/// 用空结果覆盖恢复文件，否则会清掉上一次成功保存的记录。
#[test]
fn signal_shutdown_save_preserves_records_when_tmux_backends_unreachable() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;
    let base_dir = std::env::temp_dir().join(format!("webclx-signal-save-dead-{unique}"));
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
            None,
            String::new(),
            String::new(),
            true,
        )
        .expect("attach live browser session");

    let legacy_resume_id = "11111111-2222-3333-4444-555555555555";
    let restore_file = state_file.with_file_name(super::TERMINAL_SHUTDOWN_RESTORE_FILE_NAME);
    let legacy_record = TerminalResumeRestoreRecord {
        session_id: "legacy-shutdown-session".to_string(),
        path: workspace_dir.clone(),
        user_name: crate::runtime_paths::resolve_current_user_profile()
            .map(|profile| profile.name)
            .unwrap_or_else(|| default_terminal_user_name()),
        name: "legacy session".to_string(),
        resume_id: legacy_resume_id.to_string(),
        command: format!("codex resume {legacy_resume_id}"),
        program: "codex".to_string(),
        ..TerminalResumeRestoreRecord::default()
    };
    persist_terminal_shutdown_restore_registry(
        &restore_file,
        &super::TerminalShutdownRestoreRegistry {
            records: vec![legacy_record],
        },
    )
    .expect("pre-write legacy restore records");

    // 模拟关机竞态：tmux server 先于信号保存停止，探测必然失败。
    isolated_tmux_command(&created.id)
        .arg("kill-server")
        .output()
        .expect("kill per-session tmux server");

    assert_eq!(
        manager
            .save_shutdown_restore_registry_for_signal()
            .expect("signal-triggered shutdown save"),
        0
    );
    let restore_registry =
        super::load_terminal_shutdown_restore_registry(&restore_file).expect("load restore file");
    assert_eq!(restore_registry.records.len(), 1, "legacy records must survive");
    assert_eq!(restore_registry.records[0].resume_id, legacy_resume_id);

    let _ = manager.delete_session(&workspace_dir, &workspace_dir, &created.id);
    let _ = fs::remove_dir_all(base_dir);
}

/// 后端仍可达但没有任何可探测的 agent 会话时，空结果是真实的；信号保存应照常
/// 写盘清掉陈旧记录，避免下次启动复活早已关闭的终端。
#[test]
fn signal_shutdown_save_clears_stale_records_when_backends_reachable() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_nanos() as u64;
    let base_dir = std::env::temp_dir().join(format!("webclx-signal-save-clear-{unique}"));
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
            None,
            String::new(),
            String::new(),
            true,
        )
        .expect("attach live browser session");

    let legacy_resume_id = "99999999-8888-7777-6666-555555555555";
    let restore_file = state_file.with_file_name(super::TERMINAL_SHUTDOWN_RESTORE_FILE_NAME);
    let legacy_record = TerminalResumeRestoreRecord {
        session_id: "stale-shutdown-session".to_string(),
        path: workspace_dir.clone(),
        user_name: crate::runtime_paths::resolve_current_user_profile()
            .map(|profile| profile.name)
            .unwrap_or_else(|| default_terminal_user_name()),
        name: "stale session".to_string(),
        resume_id: legacy_resume_id.to_string(),
        command: format!("codex resume {legacy_resume_id}"),
        program: "codex".to_string(),
        ..TerminalResumeRestoreRecord::default()
    };
    persist_terminal_shutdown_restore_registry(
        &restore_file,
        &super::TerminalShutdownRestoreRegistry {
            records: vec![legacy_record],
        },
    )
    .expect("pre-write stale restore records");

    assert_eq!(
        manager
            .save_shutdown_restore_registry_for_signal()
            .expect("signal-triggered shutdown save"),
        0
    );
    let restore_registry =
        super::load_terminal_shutdown_restore_registry(&restore_file).expect("load restore file");
    assert!(
        restore_registry.records.is_empty(),
        "stale records should be cleared when backends are reachable"
    );

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
            None,
            String::new(),
            String::new(),
            true,
        )
        .expect("attach live browser session");

    let resume_id = "ad08e570-051b-4b66-8c7a-7b20e434b168";
    // 模拟 Claude 退出时打印的恢复提示。
    isolated_tmux_command(&created.id)
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(&created.id))
        .arg(format!("echo 'Resume this session with: claude --resume {resume_id}'"))
        .output()
        .expect("echo claude resume prompt");
    isolated_tmux_command(&created.id)
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
    let _ = isolated_tmux_command(session_id)
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
            String::new(),
            String::new(),
            0,
        )
        .expect("scheduled input should be created");
    assert_eq!(created.task_type, "agent_message");

    let reloaded = TerminalManager::new_with_environment(
        base_dir.join("terminal-state.json"),
        TerminalEnvironmentSnapshot {
            workspace_root: base_dir.clone(),
            display_root: base_dir.clone(),
            user_profile: test_user_profile(),
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
fn agent_message_refuses_live_terminal_without_agent_process() {
    let (manager, base_dir) = build_test_terminal_manager();
    let stored = stored_test_terminal_session("s1");
    let live = build_live_session(&stored);
    manager.insert_test_live_session(stored, live);

    let error = manager
        .send_session_message(
            "s1",
            None,
            "[from webClx-compile-api] build completed".to_string(),
            1,
            true,
            true,
        )
        .expect_err("agent callback must not be written into an idle shell");

    assert!(
        error.to_string().contains("agent process is not active"),
        "unexpected error: {error:#}"
    );

    let history = manager
        .session_input_history("s1")
        .expect("session should be present");
    assert!(history.is_empty(), "refused callback must not reach the PTY");

    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn parses_lexically_safe_specified_preset_run_commands() {
    assert_eq!(
        specified_preset_run_command("webclx run api 'api-1' -- codex"),
        Some(SpecifiedPresetRunCommand {
            kind: SpecifiedPresetRunKind::Api,
            preset_id: "api-1".to_string(),
        })
    );
    assert_eq!(
        specified_preset_run_command("/usr/bin/webclx run claude 'claude-1' -- claude"),
        Some(SpecifiedPresetRunCommand {
            kind: SpecifiedPresetRunKind::Claude,
            preset_id: "claude-1".to_string(),
        })
    );
    assert_eq!(
        specified_preset_run_command("webclx run oauth oauth-1 -- codex"),
        Some(SpecifiedPresetRunCommand {
            kind: SpecifiedPresetRunKind::OAuth,
            preset_id: "oauth-1".to_string(),
        })
    );
    assert_eq!(specified_preset_run_command("codex"), None);
    assert_eq!(specified_preset_run_command("webclx run api"), None);
    assert_eq!(
        specified_preset_run_command("webclx run api api-1 codex"),
        None,
        "only the generator's explicit separator form is recognized"
    );
    assert_eq!(
        specified_preset_run_command("webclx run api $PRESET -- codex"),
        None,
        "variable expansion is not inspected"
    );
}

#[test]
fn terminal_output_identifies_glm_model_for_quota_preflight_across_proxies() {
    let glm_output = "GLM-5.3-Flash medium";
    let non_glm_output = "gpt-5.6-sol high";

    assert!(terminal_quota_preflight_uses_zhipu_provider(
        "",
        Some("http://HOST:PORT/v1"),
        glm_output,
    ));
    assert!(!terminal_quota_preflight_uses_zhipu_provider(
        "",
        Some("http://HOST:PORT/v1"),
        non_glm_output,
    ));
    assert!(terminal_quota_preflight_uses_zhipu_provider(
        "",
        Some("https://zcode.z.ai/api/v1/ultra/anthropic"),
        non_glm_output,
    ));
    assert!(terminal_quota_preflight_uses_zhipu_provider("", None, glm_output));
}

#[test]
fn quota_error_matching_recognizes_503_and_glm_status_line() {
    assert!(terminal_error_keyword_is_quota_like("503 Service Unavailable"));
    assert!(terminal_error_keyword_is_quota_like(
        "unexpected status 503 service unavailable"
    ));
    assert!(terminal_error_keyword_is_quota_like("service temporarily unavailable"));
    assert!(terminal_error_keyword_is_quota_like("exceeded retry limit"));
    assert!(terminal_error_keyword_is_quota_like("429 Too Many Requests"));

    let pane_with_blanks = "Reconnecting... 3/5 (21s • esc to interrupt)\n  └ Unexpected status 503 Service Unavailable: 继续Service temporarily unavailable, url: http://HOST:PORT/v1/responses\n\n› Ask Codex to do anything\n\n  GLM-5.3 xhigh · Context 27% used · Fast off\n\n\n\n\n\n\n\n\n\n\n\n";
    assert!(terminal_recent_output_uses_glm_model(pane_with_blanks));
    assert!(terminal_recent_output_has_quota_error(pane_with_blanks, 20));
}

#[test]
fn temporary_api_run_metadata_updates_and_persists() {
    let (manager, base_dir) = build_test_terminal_manager();
    let state_file = base_dir.join("terminal-state.json");
    let mut stored = stored_test_terminal_session("s1");
    stored.path = base_dir.clone();
    stored.codex_api_preset_name = "Default".to_string();
    stored.codex_api_base_url = "https://default.example/v1".to_string();
    let live = build_live_session(&stored);
    manager.insert_test_live_session(stored, live);

    manager
        .update_session_api_preset(
            "s1",
            "Selected".to_string(),
            "https://selected.example/v1".to_string(),
        )
        .expect("update selected API preset metadata");
    manager
        .update_session_api_preset(
            "s1",
            "Selected".to_string(),
            "https://selected.example/v1".to_string(),
        )
        .expect("unchanged metadata remains idempotent");

    // 状态加载（stored_session_survives_process_restart）按隔离 tmux 套接字
    // 探测存续：没有真实 tmux 会话的夹具在装有 tmux 的主机上会被判定为
    // Missing，在 TerminalManager::new 加载阶段即被丢弃。测试语义是元数据
    // 持久化，因此给 s1 提供真实 tmux 后端并在结束时销毁。
    kill_tmux_session("s1");
    let output = isolated_tmux_command("s1")
        .arg("new-session")
        .arg("-d")
        .arg("-s")
        .arg(tmux_session_name("s1"))
        .arg("-c")
        .arg(&base_dir)
        .output()
        .expect("create fixture tmux session");
    assert_tmux_command_succeeded(output, "create fixture tmux session");

    drop(manager);
    let restarted = TerminalManager::new(state_file.clone());
    // 用轻量列表验证元数据持久化：list_sessions 会先用真实隔离 tmux 套接字
    // 探测并清理失效会话，而测试夹具没有真实 tmux 会话，在任何装有 tmux 的
    // 主机上都会被当作 Missing 清掉，与本测试关心的持久化语义无关。
    let sessions = restarted.list_sessions_lightweight(&base_dir, &base_dir, &base_dir);
    let restored = sessions
        .iter()
        .find(|item| item.id == "s1")
        .expect("terminal should survive restart");
    assert_eq!(restored.codex_api_preset_name, "Selected");
    assert_eq!(restored.codex_api_base_url, "https://selected.example/v1");

    drop(restarted);
    kill_tmux_session("s1");
    fs::remove_dir_all(&base_dir).expect("remove test fixture");
}

#[test]
fn terminal_message_delivery_claim_is_idempotent_and_send_failure_stays_deduplicated() {
    let (manager, base_dir) = build_test_terminal_manager();

    assert!(
        manager
            .claim_terminal_message_delivery("s1", "delivery-1", "message-1")
            .unwrap()
    );
    assert!(
        !manager
            .claim_terminal_message_delivery("s1", "delivery-1", "message-1")
            .unwrap()
    );
    assert!(
        manager
            .claim_terminal_message_delivery("s2", "delivery-1", "message-1")
            .unwrap()
    );

    // 发送失败路径调用 release：记录改为 Failed 且保留——正文是否到达
    // 终端不确定，同一 delivery_id 不得被自动重发。
    manager.release_terminal_message_delivery("s1", "delivery-1");
    assert_eq!(
        manager
            .terminal_message_delivery_record("s1", "delivery-1")
            .unwrap()
            .state,
        crate::terminal::manager::message_ledger::DeliveryState::Failed
    );
    assert!(
        !manager
            .claim_terminal_message_delivery("s1", "delivery-1", "message-1")
            .unwrap()
    );

    let _ = std::fs::remove_dir_all(base_dir);
}

#[test]
fn terminal_message_delivery_claim_recovers_after_persist_failure() {
    struct TempDirGuard(std::path::PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    let (mut manager, base_dir) = build_test_terminal_manager();
    let _guard = TempDirGuard(base_dir.clone());

    // 换成文件后端账本：父目录先是合法目录，文件缺失=健康空账本。
    let blocked = base_dir.join("ledger-parent");
    std::fs::create_dir(&blocked).unwrap();
    let ledger_path = blocked.join("terminal-message-deliveries.json");
    manager.terminal_message_ledger = std::sync::Arc::new(std::sync::Mutex::new(
        crate::terminal::manager::message_ledger::TerminalMessageLedger::load(Some(
            ledger_path.clone(),
        )),
    ));

    // 父目录换成普通文件后落盘必然失败：manager 认领路径报错，不放行发送。
    std::fs::remove_dir(&blocked).unwrap();
    std::fs::write(&blocked, b"not a directory").unwrap();
    assert!(
        manager
            .claim_terminal_message_delivery("s1", "delivery-1", "message-1")
            .is_err()
    );

    // 修复父路径后准入重试成功：同一 id 因保守 Claimed 记录去重为 false，
    // 不同 id 正常认领——证明生产路由能到达恢复点。
    std::fs::remove_file(&blocked).unwrap();
    std::fs::create_dir(&blocked).unwrap();
    assert!(
        !manager
            .claim_terminal_message_delivery("s1", "delivery-1", "message-1")
            .unwrap()
    );
    assert!(
        manager
            .claim_terminal_message_delivery("s1", "delivery-2", "message-2")
            .unwrap()
    );
    assert!(
        !fs::read(&ledger_path).unwrap().is_empty(),
        "ledger file must exist after recovery"
    );
}

#[test]
fn scheduled_input_body_single_line() {
    use super::TerminalScheduledInputTask;
    let task = TerminalScheduledInputTask {
        status: String::new(),
        last_error: String::new(),
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
        repeat_interval_millis: 0,
        parent_task_id: String::new(),
    };
    // Body never includes the submit CR; fire_scheduled_input_task emits it as
    // a separate write so the raw-mode TUI treats it as a distinct key event.
    assert_eq!(super::manager::scheduled_input_body(&task), "/goal resume");
}

#[test]
fn scheduled_input_body_plain_text() {
    use super::TerminalScheduledInputTask;
    let task = TerminalScheduledInputTask {
        status: String::new(),
        last_error: String::new(),
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
        repeat_interval_millis: 0,
        parent_task_id: String::new(),
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
        status: String::new(),
        last_error: String::new(),
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
        repeat_interval_millis: 0,
        parent_task_id: String::new(),
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
        status: String::new(),
        last_error: String::new(),
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
        repeat_interval_millis: 0,
        parent_task_id: String::new(),
    };
    let body = super::manager::scheduled_input_body(&task);
    assert_eq!(body, "\u{1b}[200~a\rb\rc\u{1b}[201~");
    assert!(!body.contains('\n'));
}

#[cfg(test)]
mod scheduled_input_batch {
    use super::*;
    use crate::terminal::manager::TerminalScheduledInputOneSender;
    use std::panic::{AssertUnwindSafe, catch_unwind};

    #[test]
    fn scheduled_unconfirmed_history_does_not_block_new_messages() {
        let (manager, base_dir) = build_test_terminal_manager();
        for status in ["pending_confirmation", "unknown"] {
            let mut old = test_scheduled_input_task(status, "session-1", "old body");
            old.status = status.into();
            old.task_type = "agent_message".into();
            old.last_error = "original diagnostic".into();
            manager
                .scheduled_input_tasks
                .lock()
                .unwrap()
                .insert(old.id.clone(), old);
        }
        let tasks = vec![
            test_scheduled_input_task("new-1", "session-1", "first"),
            test_scheduled_input_task("new-2", "session-1", "second"),
        ];
        let attempted = Arc::new(Mutex::new(Vec::new()));
        let attempts = attempted.clone();
        let send_one: TerminalScheduledInputOneSender = Arc::new(move |_, task| {
            attempts.lock().unwrap().push(task.id.clone());
            Ok(())
        });
        manager.fire_scheduled_input_task_batch(tasks, &send_one);
        assert_eq!(*attempted.lock().unwrap(), vec!["new-1", "new-2"]);
        let stored = manager.scheduled_input_tasks.lock().unwrap();
        for status in ["pending_confirmation", "unknown"] {
            assert_eq!(stored[status].status, status);
            assert_eq!(stored[status].last_error, "original diagnostic");
        }
        drop(stored);
        let _ = std::fs::remove_dir_all(base_dir);
    }

    #[test]
    fn batch_sender_preserves_order_continues_after_errors() {
        let (manager, _base_dir) = build_test_terminal_manager();
        let tasks = vec![
            test_scheduled_input_task("task-1", "session-1", "fail"),
            test_scheduled_input_task("task-2", "session-2", "ok"),
            test_scheduled_input_task("task-3", "session-3", "fail"),
            test_scheduled_input_task("task-4", "session-4", "ok"),
        ];
        let attempted = Arc::new(Mutex::new(Vec::new()));
        let attempted_for_sender = attempted.clone();
        let failure_text = Arc::new(Mutex::new("fail".to_string()));
        let failure_text_for_sender = failure_text.clone();
        let send_one: TerminalScheduledInputOneSender = Arc::new(
            move |manager: &TerminalManager, task: &crate::terminal::TerminalScheduledInputTask| {
                attempted_for_sender
                    .lock()
                    .expect("attempt log lock")
                    .push(task.id.clone());
                let _ = manager; // no terminal IO needed for this injectable sender
                let failure_text = failure_text_for_sender
                    .lock()
                    .expect("failure text lock")
                    .clone();
                if task.text == failure_text {
                    Err(anyhow::anyhow!("injected failure"))
                } else {
                    Ok(())
                }
            },
        );

        // Exercise the production batch helper, not a test-local copy of its loop.
        manager.fire_scheduled_input_task_batch(tasks, &send_one);
        assert_eq!(
            attempted.lock().expect("attempt log lock").clone(),
            vec!["task-1", "task-2", "task-3", "task-4"],
            "sender must attempt tasks in batch order and continue after injected failures"
        );

        // Stop failures and run another batch to prove the production helper is reusable.
        *failure_text.lock().expect("failure text lock") = "\u{0}no-failure-marker".to_string();
        attempted.lock().expect("attempt log lock").clear();
        let same_tasks = vec![
            test_scheduled_input_task("task-1", "session-1", "ok"),
            test_scheduled_input_task("task-2", "session-2", "ok"),
            test_scheduled_input_task("task-3", "session-3", "ok"),
            test_scheduled_input_task("task-4", "session-4", "ok"),
        ];
        manager.fire_scheduled_input_task_batch(same_tasks, &send_one);
        assert_eq!(
            attempted.lock().expect("attempt log lock").clone(),
            vec!["task-1", "task-2", "task-3", "task-4"],
            "the production helper must remain reusable and continue after failures"
        );
    }

    #[test]
    fn agent_messages_are_not_deferred_by_scheduled_input_avoid_window() {
        let (manager, _base_dir) = build_test_terminal_manager();
        let now = terminal_core::current_timestamp_millis();
        let mut msg_task = test_scheduled_input_task("agent-1", "session-1", "hello agent");
        msg_task.task_type = "agent_message".into();
        msg_task.due_at_millis = now.saturating_sub(1000);

        let mut normal_task = test_scheduled_input_task("normal-1", "session-1", "normal input");
        normal_task.task_type = "paste".into();
        normal_task.due_at_millis = now.saturating_sub(1000);

        {
            let mut tasks = manager.scheduled_input_tasks.lock().unwrap();
            tasks.insert(msg_task.id.clone(), msg_task);
            tasks.insert(normal_task.id.clone(), normal_task);
        }

        let end_millis = now.saturating_add(3600_000);
        {
            let mut tasks = manager.scheduled_input_tasks.lock().unwrap();
            for task in tasks.values_mut() {
                if task.status.is_empty()
                    && task.task_type != "agent_message"
                    && task.due_at_millis <= now
                {
                    task.due_at_millis = end_millis;
                }
            }
            let due_ids = tasks
                .values()
                .filter(|task| {
                    task.status.is_empty()
                        && task.due_at_millis <= now
                        && task.task_type == "agent_message"
                })
                .map(|t| t.id.clone())
                .collect::<Vec<_>>();
            assert_eq!(due_ids, vec!["agent-1"]);
            assert_eq!(tasks["normal-1"].due_at_millis, end_millis);
            assert!(tasks["agent-1"].due_at_millis <= now);
        }
    }

    #[test]
    fn completed_scheduled_tasks_are_bounded_and_pruned() {
        let mut tasks = std::collections::HashMap::new();
        // Add 40 completed tasks
        for i in 0..40 {
            let mut task = test_scheduled_input_task(&format!("done-{i}"), "session-1", "msg");
            task.status = "confirmed".into();
            task.due_at_millis = 1000 + i;
            tasks.insert(task.id.clone(), task);
        }
        // Add 2 pending tasks
        let mut pending1 = test_scheduled_input_task("pending-1", "session-1", "pending");
        pending1.status = "".into();
        pending1.due_at_millis = 500;
        let mut pending2 = test_scheduled_input_task("pending-2", "session-1", "pending");
        pending2.status = "".into();
        pending2.due_at_millis = 600;
        tasks.insert(pending1.id.clone(), pending1);
        tasks.insert(pending2.id.clone(), pending2);

        super::super::manager::scheduled_input::prune_completed_scheduled_input_tasks(&mut tasks);

        // Should retain all 2 pending tasks
        assert!(tasks.contains_key("pending-1"));
        assert!(tasks.contains_key("pending-2"));
        // Should retain at most 30 completed tasks (total tasks = 32)
        assert_eq!(tasks.len(), 32);
        // The oldest completed tasks (done-0 .. done-9) should have been pruned
        assert!(!tasks.contains_key("done-0"));
        assert!(!tasks.contains_key("done-9"));
        // The newest completed tasks (done-10 .. done-39) should remain
        assert!(tasks.contains_key("done-10"));
        assert!(tasks.contains_key("done-39"));
    }

    #[test]
    fn batch_sender_blocking_does_not_block_current_thread_worker() {
        let (manager, _base_dir) = build_test_terminal_manager();
        let (batch_started_tx, batch_started_rx) = tokio::sync::oneshot::channel();
        let batch_started_tx = Arc::new(Mutex::new(Some(batch_started_tx)));
        let (batch_completed_tx, batch_completed_rx) =
            tokio::sync::oneshot::channel::<&'static str>();
        let batch_completed_tx = Arc::new(Mutex::new(Some(batch_completed_tx)));
        let batch_release_state = Arc::new((Mutex::new(false), Condvar::new()));
        let batch_release_state_for_sender = batch_release_state.clone();

        // The loop only dispatches due tasks, so seed one task that is already due.
        // Mark it agent_message so the peak-hour avoid window (default 14:00-18:00)
        // never defers it: this test asserts blocking-pool scheduling, not the window.
        let mut due_task = test_scheduled_input_task("task-1", "session-1", "block");
        due_task.task_type = "agent_message".into();
        manager
            .scheduled_input_tasks
            .lock()
            .expect("scheduled input tasks lock")
            .insert("task-1".to_string(), due_task);

        let send_batch: crate::terminal::manager::TerminalScheduledInputBatchSender =
            Arc::new(move |_manager, batch| {
                assert_eq!(batch.len(), 1);
                assert_eq!(batch[0].text, "block");
                let sender = batch_started_tx
                    .lock()
                    .expect("batch start sender lock")
                    .take()
                    .expect("batch start sender should be available exactly once");
                sender
                    .send(())
                    .expect("batch start signal receiver should remain alive");

                // The barrier has a hard timeout. Return the causal check to
                // the async side because a panic would become a JoinError in
                // the production loop and would look like a false pass here.
                let (released, condition) = &*batch_release_state_for_sender;
                let (released_guard, wait_result) = condition
                    .wait_timeout_while(
                        released.lock().expect("release state lock"),
                        Duration::from_secs(2),
                        |released| !*released,
                    )
                    .expect("release condition lock should not be poisoned");
                let result = if *released_guard && !wait_result.timed_out() {
                    "ok"
                } else {
                    "blocking batch timed out or was not released"
                };
                drop(released_guard);
                let completed_sender = batch_completed_tx
                    .lock()
                    .expect("batch completion sender lock")
                    .take()
                    .expect("batch completion sender should be available exactly once");
                completed_sender
                    .send(result)
                    .expect("batch completion receiver should remain alive");
            });

        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime should start");

        let collection_wait_started = Instant::now();
        runtime.block_on(async move {
            let notify = manager.scheduled_input_notify.clone();
            let heartbeat = tokio::spawn(async move {
                loop {
                    notify.notify_one();
                    tokio::time::sleep(Duration::from_millis(2)).await;
                }
            });

            // Hold a release guard for every assertion in this test. If an
            // assertion fails or the runtime unwinds, Drop still releases the
            // blocking closure so runtime teardown cannot hang.
            let guard = BlockingBatchReleaseGuard::new(batch_release_state.clone());

            let loop_task =
                tokio::spawn(manager.run_scheduled_input_loop_with_batch_sender(send_batch));
            tokio::time::timeout(Duration::from_secs(2), batch_started_rx)
                .await
                .expect("batch should start on the blocking pool")
                .expect("batch start signal should be sent");

            for _ in 0..10 {
                tokio::time::timeout(
                    Duration::from_millis(100),
                    tokio::time::sleep(Duration::from_millis(1)),
                )
                .await
                .expect("async worker should remain responsive while batch blocks");
            }

            // Proactive normal release; guard Drop remains the failure backstop.
            {
                let (released, condition) = &*batch_release_state;
                let mut released_guard = released
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                *released_guard = true;
                condition.notify_all();
            }

            // The scheduler itself runs forever; assert the batch result, not
            // completion of the loop. Then abort and await the stale handle.
            let result = tokio::time::timeout(Duration::from_secs(2), batch_completed_rx)
                .await
                .expect("batch completion should be signaled")
                .expect("batch completion channel should remain open");
            assert_eq!(result, "ok", "blocking batch must be released before timeout");

            heartbeat.abort();
            let _ = tokio::time::timeout(Duration::from_secs(1), heartbeat).await;
            loop_task.abort();
            let _ = tokio::time::timeout(Duration::from_secs(1), loop_task).await;
            drop(guard);
        });
    }

    #[test]
    fn scheduled_collection_blocking_does_not_block_current_thread_worker() {
        let (manager, _base_dir) = build_test_terminal_manager();
        // Mark it agent_message so the peak-hour avoid window (default 14:00-18:00)
        // never defers it: this test asserts collection-lock scheduling, not the window.
        let mut due_task = test_scheduled_input_task("task-1", "session-1", "queued");
        due_task.task_type = "agent_message".into();
        manager
            .scheduled_input_tasks
            .lock()
            .expect("scheduled input tasks lock")
            .insert("task-1".to_string(), due_task);

        let (lock_ready_tx, lock_ready_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let tasks = manager.scheduled_input_tasks.clone();
        let lock_holder = std::thread::spawn(move || {
            let guard = tasks.lock().expect("scheduled input tasks lock");
            lock_ready_tx
                .send(())
                .expect("lock-ready receiver should remain alive");
            let _ = release_rx.recv_timeout(Duration::from_secs(2));
            drop(guard);
        });
        let release_guard = ScheduledCollectionReleaseGuard(Some(release_tx));
        lock_ready_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("collection lock holder should start");

        let (batch_started_tx, mut batch_started_rx) = tokio::sync::oneshot::channel();
        let batch_started_tx = Arc::new(Mutex::new(Some(batch_started_tx)));
        let send_batch: crate::terminal::manager::TerminalScheduledInputBatchSender =
            Arc::new(move |_manager, batch| {
                assert_eq!(batch.len(), 1);
                batch_started_tx
                    .lock()
                    .expect("batch start sender lock")
                    .take()
                    .expect("batch start sender should be available once")
                    .send(())
                    .expect("batch start receiver should remain alive");
            });

        let heartbeat_count = Arc::new(AtomicUsize::new(0));
        let heartbeat_count_for_task = heartbeat_count.clone();
        let (heartbeat_second_tx, heartbeat_second_rx) = tokio::sync::oneshot::channel();
        let heartbeat_second_tx = Arc::new(Mutex::new(Some(heartbeat_second_tx)));
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("current-thread runtime should start");
        let collection_wait_started = Instant::now();
        runtime.block_on(async move {
            let notify = manager.scheduled_input_notify.clone();
            let heartbeat = tokio::spawn(async move {
                loop {
                    let count = heartbeat_count_for_task.fetch_add(1, Ordering::SeqCst) + 1;
                    if count == 2 {
                        if let Some(sender) = heartbeat_second_tx
                            .lock()
                            .expect("heartbeat sender lock")
                            .take()
                        {
                            let _ = sender.send(());
                        }
                    }
                    notify.notify_one();
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            });
            let loop_task =
                tokio::spawn(manager.run_scheduled_input_loop_with_batch_sender(send_batch));

            tokio::time::timeout(Duration::from_secs(1), heartbeat_second_rx)
                .await
                .expect("current-thread worker should keep heartbeats while collection blocks")
                .expect("heartbeat signal should remain connected");
            assert!(heartbeat_count.load(Ordering::SeqCst) >= 2);
            assert!(
                collection_wait_started.elapsed() < Duration::from_secs(1),
                "heartbeat must run before the lock holder's safety timeout"
            );
            assert!(batch_started_rx.try_recv().is_err(), "batch must wait for collection lock");

            drop(release_guard);
            tokio::time::timeout(Duration::from_secs(2), batch_started_rx)
                .await
                .expect("batch should start after collection lock release")
                .expect("batch start signal should remain connected");

            heartbeat.abort();
            let _ = tokio::time::timeout(Duration::from_secs(1), heartbeat).await;
            loop_task.abort();
            let _ = tokio::time::timeout(Duration::from_secs(1), loop_task).await;
        });
        lock_holder
            .join()
            .expect("lock holder should exit after release");
    }

    struct ScheduledCollectionReleaseGuard(Option<mpsc::SyncSender<()>>);

    impl Drop for ScheduledCollectionReleaseGuard {
        fn drop(&mut self) {
            if let Some(sender) = self.0.take() {
                let _ = sender.send(());
            }
        }
    }

    #[test]
    fn blocking_batch_release_guard_releases_on_unwind() {
        let batch_release_state = Arc::new((Mutex::new(false), Condvar::new()));
        let result = catch_unwind(AssertUnwindSafe(|| {
            let _guard = BlockingBatchReleaseGuard::new(batch_release_state.clone());
            panic!("simulated assertion failure while guard is held");
        }));

        assert!(result.is_err(), "the simulated failure must unwind");
        let (released, condition) = &*batch_release_state;
        let released_guard = released
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert!(*released_guard, "guard Drop must release the blocking barrier after unwind");
        drop(released_guard);
        condition.notify_all();
    }

    struct BlockingBatchReleaseGuard(Arc<(Mutex<bool>, Condvar)>);

    impl BlockingBatchReleaseGuard {
        fn new(state: Arc<(Mutex<bool>, Condvar)>) -> Self {
            Self(state)
        }
    }

    impl Drop for BlockingBatchReleaseGuard {
        fn drop(&mut self) {
            let (released, condition) = &*self.0;
            let mut released_guard = released
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            *released_guard = true;
            condition.notify_all();
        }
    }
}

fn test_scheduled_input_task(
    id: &str,
    session_id: &str,
    text: &str,
) -> super::TerminalScheduledInputTask {
    super::TerminalScheduledInputTask {
        status: String::new(),
        last_error: String::new(),
        id: id.to_string(),
        session_id: session_id.to_string(),
        terminal_name: String::new(),
        due_at_millis: 0,
        created_at_millis: 0,
        label: String::new(),
        text: text.to_string(),
        send_enter: false,
        task_type: String::new(),
        working_dir: String::new(),
        repeat_interval_millis: 0,
        parent_task_id: String::new(),
    }
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
    assert_eq!(verification_window_ms, 6_000);
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
        "│  Model provider:       sub2api_gpt-5.6_1M - http://HOST:PORT/v1  │\n",
    );

    let provider = super::manager::parse_codex_model_provider(snapshot.as_bytes())
        .expect("Model provider should be present");
    assert_eq!(provider, "sub2api_gpt-5.6_1M - http://HOST:PORT/v1");
    assert_eq!(
        super::manager::split_codex_model_provider(&provider),
        ("sub2api_gpt-5.6_1M".to_string(), "http://HOST:PORT/v1".to_string(),)
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

#[test]
fn grouped_sessions_atomic_membership_and_persistence() {
    let mut state = TerminalState::default();
    for id in ["s1", "s2", "s3", "s4", "s5"] {
        state
            .sessions_by_id
            .insert(id.into(), stored_test_terminal_session(id));
    }
    let ids = |values: &[&str]| values.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    super::manager::update_session_group_locked(
        &mut state,
        "s2",
        &ids(&["s3"]),
        &[],
        false,
        None,
        None,
    )
    .unwrap();
    let token = state.sessions_by_id["s2"].group_token.clone();
    assert!(!token.is_empty());
    assert_eq!(state.sessions_by_id["s3"].group_token, token);
    assert!(!state.sessions_by_id["s2"].group_color.is_empty());
    assert_eq!(state.sessions_by_id["s3"].group_color, state.sessions_by_id["s2"].group_color);
    // Invalid selection fails before any member is changed.
    assert!(
        super::manager::update_session_group_locked(
            &mut state,
            "s2",
            &ids(&["missing"]),
            &[],
            false,
            None,
            None,
        )
        .is_err()
    );
    assert_eq!(state.sessions_by_id["s2"].group_token, token);
    super::manager::update_session_group_locked(
        &mut state,
        "s2",
        &ids(&["s3"]),
        &[],
        false,
        Some("#ABCDEF".to_string()),
        None,
    )
    .unwrap();
    assert_eq!(state.sessions_by_id["s2"].group_color, "#abcdef");
    assert_eq!(state.sessions_by_id["s3"].group_color, "#abcdef");

    // A color change is atomic with the same membership payload.
    assert!(
        super::manager::update_session_group_locked(
            &mut state,
            "s2",
            &ids(&["missing"]),
            &[],
            false,
            Some("#111111".to_string()),
            None,
        )
        .is_err()
    );
    assert_eq!(state.sessions_by_id["s2"].group_color, "#abcdef");
    assert_eq!(state.sessions_by_id["s3"].group_color, "#abcdef");

    // If the color is omitted, the existing group color is inherited.
    super::manager::update_session_group_locked(
        &mut state,
        "s2",
        &ids(&["s3"]),
        &[],
        false,
        None,
        None,
    )
    .unwrap();
    assert_eq!(state.sessions_by_id["s2"].group_color, "#abcdef");
    assert_eq!(state.sessions_by_id["s3"].group_color, "#abcdef");

    let encoded = serde_json::to_vec(&state.sessions_by_id["s2"]).unwrap();
    let restored: StoredTerminalSession = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(restored.group_token, token);
    assert_eq!(restored.group_color, "#abcdef");
    assert!(!restored.manually_renamed);
    for active in ["s2", "s3"] {
        state.sessions_by_id.get_mut(active).unwrap().last_opened_at += 100;
        let mut order = ids(&["s5", "s3", "s1", "s2", "s4"]);
        sort_session_ids_by_recent_activity(&state, &mut order);
        assert_eq!(&order[..2], &["s2", "s3"]);
    }
    super::manager::update_session_group_locked(
        &mut state,
        "s4",
        &ids(&["s5"]),
        &[],
        false,
        None,
        None,
    )
    .unwrap();
    state.sessions_by_id.get_mut("s5").unwrap().last_opened_at = 500;
    state.sessions_by_id.get_mut("s1").unwrap().last_opened_at = 400;
    let mut order = ids(&["s1", "s2", "s3", "s4", "s5"]);
    sort_session_ids_by_recent_activity(&state, &mut order);
    assert_eq!(order, ids(&["s4", "s5", "s1", "s2", "s3"]));
    // Moving one partner leaves no stale singleton marker in its former group.
    super::manager::update_session_group_locked(
        &mut state,
        "s2",
        &ids(&["s4"]),
        &[],
        false,
        None,
        None,
    )
    .unwrap();
    assert!(state.sessions_by_id["s3"].group_token.is_empty());
    assert!(state.sessions_by_id["s3"].group_color.is_empty());
    assert!(state.sessions_by_id["s5"].group_token.is_empty());
    assert!(state.sessions_by_id["s5"].group_color.is_empty());
    super::manager::update_session_group_locked(&mut state, "s2", &[], &[], false, None, None)
        .unwrap();
    assert!(
        state
            .sessions_by_id
            .values()
            .all(|s| s.group_token.is_empty())
    );
}

#[test]
fn grouped_sessions_preserve_manual_order() {
    let mut state = TerminalState::default();
    for id in ["s1", "s2", "s3", "s4", "s5"] {
        state
            .sessions_by_id
            .insert(id.into(), stored_test_terminal_session(id));
    }
    let ids = |values: &[&str]| values.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    super::manager::update_session_group_locked(
        &mut state,
        "s1",
        &ids(&["s3", "s1", "s2"]),
        &[],
        false,
        None,
        None,
    )
    .unwrap();
    let _ = state.sessions_by_id["s1"].group_token.clone();
    let mut order = ids(&["s4", "s5", "s1", "s2", "s3"]);
    sort_session_ids_by_recent_activity(&state, &mut order);
    assert_eq!(order, ids(&["s5", "s4", "s3", "s1", "s2"]));

    state.sessions_by_id.get_mut("s2").unwrap().last_opened_at = 100;
    let mut order = ids(&["s1", "s2", "s3", "s4", "s5"]);
    sort_session_ids_by_recent_activity(&state, &mut order);
    assert_eq!(order, ids(&["s3", "s1", "s2", "s5", "s4"]));

    super::manager::update_session_group_locked(
        &mut state,
        "s1",
        &ids(&["s3", "s1"]),
        &[],
        false,
        None,
        None,
    )
    .unwrap();
    assert!(state.sessions_by_id["s2"].group_token.is_empty());
}

#[test]
fn new_group_default_color_skips_colors_already_used_by_other_groups() {
    let mut state = TerminalState::default();
    for id in ["s1", "s2", "s3", "s4"] {
        state
            .sessions_by_id
            .insert(id.into(), stored_test_terminal_session(id));
    }
    let ids = |values: &[&str]| values.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    super::manager::update_session_group_locked(
        &mut state,
        "s1",
        &ids(&["s2"]),
        &[],
        false,
        Some(super::manager::default_terminal_group_color("group-2")),
        None,
    )
    .unwrap();
    super::manager::update_session_group_locked(
        &mut state,
        "s3",
        &ids(&["s4"]),
        &[],
        false,
        None,
        None,
    )
    .unwrap();
    assert_eq!(state.sessions_by_id["s3"].group_token, "group-2");
    assert_ne!(state.sessions_by_id["s3"].group_color, state.sessions_by_id["s1"].group_color);
}

#[test]
fn grouped_session_activation_promotes_member_unless_order_is_fixed() {
    let mut state = TerminalState::default();
    for id in ["s1", "s2", "s3", "s4"] {
        state
            .sessions_by_id
            .insert(id.into(), stored_test_terminal_session(id));
    }
    let ids = |values: &[&str]| values.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    super::manager::update_session_group_locked(
        &mut state,
        "s1",
        &ids(&["s1", "s2", "s3"]),
        &[],
        false,
        None,
        None,
    )
    .unwrap();
    state.sessions_by_id.get_mut("s1").unwrap().group_order = 1;
    state.sessions_by_id.get_mut("s2").unwrap().group_order = 2;
    state.sessions_by_id.get_mut("s3").unwrap().group_order = 3;
    super::manager::mark_session_opened_locked(&mut state, "s3");
    assert_eq!(state.sessions_by_id["s3"].group_order, 1);
    assert_eq!(state.sessions_by_id["s1"].group_order, 2);
    assert_eq!(state.sessions_by_id["s2"].group_order, 3);

    super::manager::update_session_group_locked(
        &mut state,
        "s1",
        &ids(&["s1", "s2", "s3"]),
        &[],
        true,
        None,
        None,
    )
    .unwrap();
    state.sessions_by_id.get_mut("s1").unwrap().group_order = 1;
    state.sessions_by_id.get_mut("s2").unwrap().group_order = 2;
    state.sessions_by_id.get_mut("s3").unwrap().group_order = 3;
    state.sessions_by_id.get_mut("s3").unwrap().last_opened_at = 0;
    super::manager::mark_session_opened_locked(&mut state, "s3");
    assert_eq!(state.sessions_by_id["s1"].group_order, 1);
    assert_eq!(state.sessions_by_id["s2"].group_order, 2);
    assert_eq!(state.sessions_by_id["s3"].group_order, 3);
}

#[test]
fn codex_scan_slot_shares_inflight_scan_between_requests() {
    let key = super::CodexScanKey {
        codex_home: std::env::temp_dir().join(format!(
            "webclx-scan-slot-share-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after epoch")
                .as_nanos()
        )),
        cwd_filter: None,
        since_ms: None,
    };
    let (leader_slot, is_leader) = super::register_codex_scan_slot(&key);
    assert!(is_leader, "first registration should lead the scan");
    let (follower_slot, follower_is_leader) = super::register_codex_scan_slot(&key);
    assert!(!follower_is_leader, "second identical request should follow");
    assert_eq!(leader_slot.id, follower_slot.id);

    let shared: super::CodexScanShared = Arc::new(Ok(Vec::new()));
    super::complete_codex_scan_slot(&key, &leader_slot, shared);

    let (next_slot, next_is_leader) = super::register_codex_scan_slot(&key);
    assert!(next_is_leader, "completed scan must not block a fresh scan");
    assert_ne!(next_slot.id, leader_slot.id);
    assert!(
        follower_slot.result_rx.borrow().is_some(),
        "follower must observe the published shared result"
    );
    super::complete_codex_scan_slot(&key, &next_slot, Arc::new(Ok(Vec::new())));
}

#[test]
fn codex_scan_slot_takes_over_stale_leader() {
    let key = super::CodexScanKey {
        codex_home: std::env::temp_dir().join(format!(
            "webclx-scan-slot-stale-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time should be after epoch")
                .as_nanos()
        )),
        cwd_filter: None,
        since_ms: None,
    };
    let (stale_slot, is_leader) = super::register_codex_scan_slot(&key);
    assert!(is_leader);
    {
        let mut slots = crate::lock_or_recover!(super::codex_scan_slots().lock());
        let stored = slots.get_mut(&key).expect("stored scan slot");
        stored.created_at =
            Instant::now() - Duration::from_secs(super::TERMINAL_TASK_TIMEOUT_SECS + 3);
    }
    let (replacement_slot, takeover_is_leader) = super::register_codex_scan_slot(&key);
    assert!(takeover_is_leader, "stale leader slot must be taken over");
    assert_ne!(replacement_slot.id, stale_slot.id);

    super::complete_codex_scan_slot(&key, &stale_slot, Arc::new(Ok(Vec::new())));
    let (after_stale, still_registered) = super::register_codex_scan_slot(&key);
    assert!(
        !still_registered,
        "replacement slot must stay registered after a stale leader completes"
    );
    assert_eq!(after_stale.id, replacement_slot.id);
    super::complete_codex_scan_slot(&key, &replacement_slot, Arc::new(Ok(Vec::new())));
}

#[tokio::test]
async fn codex_scan_slot_waiter_receives_ok_and_error_results() {
    let base = std::env::temp_dir().join(format!(
        "webclx-scan-slot-wait-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos()
    ));
    let ok_key = super::CodexScanKey {
        codex_home: base.join("ok"),
        cwd_filter: None,
        since_ms: None,
    };
    let (ok_slot, is_leader) = super::register_codex_scan_slot(&ok_key);
    assert!(is_leader);
    let ok_waiter = super::wait_codex_scan_result(ok_slot.result_rx.clone());
    super::complete_codex_scan_slot(&ok_key, &ok_slot, Arc::new(Ok(Vec::new())));
    assert!(ok_waiter.await.is_ok(), "waiter should receive Ok result");

    let error_key = super::CodexScanKey {
        codex_home: base.join("error"),
        cwd_filter: None,
        since_ms: None,
    };
    let (error_slot, error_is_leader) = super::register_codex_scan_slot(&error_key);
    assert!(error_is_leader);
    let error_waiter = super::wait_codex_scan_result(error_slot.result_rx.clone());
    let shared: super::CodexScanShared = Arc::new(Err("扫描失败原因".to_string()));
    super::complete_codex_scan_slot(&error_key, &error_slot, shared);
    let error = error_waiter
        .await
        .expect_err("waiter must surface the scan error instead of faking success");
    assert!(
        error.to_string().contains("扫描失败原因"),
        "error detail must propagate to the follower"
    );
}
#[test]
fn stored_hsl_group_colors_convert_to_hex_for_group_api() {
    assert_eq!(
        super::manager::hsl_terminal_group_color("hsl(120.000 76% 52%)").as_deref(),
        Some("#28e228")
    );
    assert_eq!(super::manager::hsl_terminal_group_color("#28e228"), None);

    let mut session = stored_test_terminal_session("s1");
    session.group_token = "group-1".to_string();
    session.group_color = "hsl(0.000 76% 52%)".to_string();
    assert!(super::manager::normalize_stored_terminal_group_color(&mut session));
    assert_eq!(session.group_color, "#e22828");

    session.group_color = "#123ABC".to_string();
    assert!(!super::manager::normalize_stored_terminal_group_color(&mut session));
    assert_eq!(session.group_color, "#123ABC");
}

#[test]
fn scheduled_submission_retries_only_enter_until_rollout_confirms() {
    use std::cell::Cell;
    let enters = Cell::new(0);
    let waits = Cell::new(0);
    let result = super::manager::submit_scheduled_message(
        || Ok(false),
        || {
            enters.set(enters.get() + 1);
            Ok(())
        },
        || Ok(enters.get() == 2),
        || waits.set(waits.get() + 1),
    )
    .unwrap();
    assert_eq!(result, "confirmed");
    assert_eq!(enters.get(), 2);
    assert_eq!(waits.get(), 21);
}

#[test]
fn scheduled_submission_unconfirmed_is_bounded_and_never_success() {
    use std::cell::Cell;
    let enters = Cell::new(0);
    let waits = Cell::new(0);
    assert_eq!(
        super::manager::submit_scheduled_message(
            || Ok(false),
            || {
                enters.set(enters.get() + 1);
                Ok(())
            },
            || Ok(false),
            || waits.set(waits.get() + 1),
        )
        .unwrap(),
        "pending_confirmation"
    );
    assert_eq!(enters.get(), 3);
    assert_eq!(waits.get(), 60);
}

#[test]
fn scheduled_submission_already_confirmed_never_sends_enter() {
    assert_eq!(
        super::manager::submit_scheduled_message(
            || Ok(false),
            || panic!("confirmed message must not receive another Enter"),
            || Ok(true),
            || panic!("no wait needed"),
        )
        .unwrap(),
        "confirmed"
    );
}

#[test]
fn scheduled_submission_stops_on_unknown_history_or_write_error() {
    let error = super::manager::submit_scheduled_message(
        || Ok(false),
        || panic!("unknown history must stop before Enter"),
        || anyhow::bail!("rollout unavailable"),
        || panic!("no wait needed"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("rollout unavailable"));
    let error = super::manager::submit_scheduled_message(
        || Ok(false),
        || anyhow::bail!("partial write"),
        || Ok(false),
        || panic!("no wait needed"),
    )
    .unwrap_err();
    assert!(error.to_string().contains("partial write"));
}

#[test]
fn scheduled_dispatching_survives_reload_as_unknown_without_resend() {
    let (manager, base_dir) = build_test_terminal_manager();
    let mut task = test_scheduled_input_task("scheduled-restart", "s1", "body");
    task.status = "dispatching".into();
    let registry = super::StoredTerminalScheduledInputRegistry { tasks: vec![task] };
    std::fs::write(manager.scheduled_input_file.as_ref(), serde_json::to_vec(&registry).unwrap())
        .unwrap();
    let reloaded = super::TerminalManager::new(base_dir.join("terminal-state.json"));
    let rows = reloaded.list_scheduled_inputs();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "unknown");
    assert!(rows[0].last_error.contains("restarted"));
    let _ = std::fs::remove_dir_all(base_dir);
}

// ===== 终端注入红线源码扫描 =====
//
// AGENTS.md 最高优先级规则：webClx 不得向终端、shell、Agent 或其子进程注入
// 环境变量、启动参数或配置。下面的回归测试不启动任何进程，只读取本 crate
// 自身的源码（`env!("CARGO_MANIFEST_DIR")` 指向 crate 根目录）做静态扫描，
// 覆盖三道防线：
//
// 1. `CHILD_PROCESS_ENV_KEYS_TO_CLEAR` 仍覆盖全部禁入变量（直接断言常量）；
// 2. src/terminal/ 源码树（含模块根 src/terminal.rs）里每个含
//    `Command::new(` / `CommandBuilder::new(` 的非测试文件，剥离
//    `#[cfg(test)]` 条目后必须仍出现 `sanitize_child_command(&mut` 或
//    `sanitize_pty_command(&mut`——即「新文件裸 spawn 必被拦」。例外的
//    OS 工具 spawn 只能登记进 SOURCE_SCAN_UNWASHED_SPAWN_ALLOWLIST，条目
//    附带当时的 spawn 计数，往豁免文件里再加一个裸 spawn 会因计数失配
//    而失败。真正的进程执行器 process_exec 在 src/process_exec.rs，不在
//    本扫描范围内；
// 3. 剥离测试代码后任何文件不得出现 "--login"，且 `arg("runuser")` 只允许
//    在 src/terminal/tmux.rs 的 append_runuser_login_shell 出现一次。

/// `CHILD_PROCESS_ENV_KEYS_TO_CLEAR` 必须覆盖的禁入变量全集。新增可旁路
/// 终端身份的变量时，先加进 terminal.rs 的常量，再补进这份期望清单。
const SOURCE_SCAN_EXPECTED_CLEARED_ENV_KEYS: &[&str] = &[
    "LD_LIBRARY_PATH",
    "LD_PRELOAD",
    "DYLD_LIBRARY_PATH",
    "DYLD_FALLBACK_LIBRARY_PATH",
    "DYLD_INSERT_LIBRARIES",
    "LIBRARY_PATH",
    "HOME",
    "WEBCLX_USER_HOME",
    "CODEX_HOME",
    "CLAUDE_CONFIG_DIR",
    "WEBCLX_LOCAL_API_TOKEN",
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_BASE_URL",
    "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    "ANTHROPIC_DEFAULT_SONNET_MODEL",
    "ANTHROPIC_DEFAULT_OPUS_MODEL",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_SMALL_FAST_MODEL",
];

/// 允许不经 sanitize_* 就 spawn 子进程的源码文件。每项的 `&str` 原因必须
/// 说明为什么这里起的进程不是用户终端/shell/Agent；`usize` 锁定登记时的
/// `Command::new(` + `CommandBuilder::new(` 总数，新增或删除 spawn 都必须
/// 回来更新本清单，从而强制人工复核。
const SOURCE_SCAN_UNWASHED_SPAWN_ALLOWLIST: &[(&str, usize, &str)] = &[
    (
        "src/terminal.rs",
        1,
        "模块根 read_terminal_auto_continue_crontab 只经 \
         process_exec::bounded_output 起 `crontab -l`，属服务自身 OS 工具",
    ),
    (
        "src/terminal/manager.rs",
        4,
        "systemctl stop webclx-tmux-*.scope 与 date 的 cron 时间换算都经 \
         process_exec::bounded_output，属服务自身 OS 工具",
    ),
    (
        "src/terminal/manager/crontab.rs",
        2,
        "crontab -l / crontab - 读写系统 crontab，经 \
         process_exec::bounded_output，不是用户终端",
    ),
];

/// `#[cfg(test)]` 与 `#[cfg(all(test, ...))]` 标记的是仅测试期存在的条目；
/// `any(...)`、`not(test)`、`cfg_attr` 不保证只在测试编译出现，保持原样。
fn source_scan_attr_is_test_only(line: &str) -> bool {
    let Some(inner) = line.trim_start().strip_prefix("#[cfg(") else {
        return false;
    };
    if inner.trim_end() == "test)]" {
        return true;
    }
    inner.trim_start().starts_with("all(") && inner.contains("test") && !inner.contains("not(test")
}

/// 消费一个被 cfg(test) 属性装饰的条目：先跳过空行与同一条目上的叠加
/// 属性行，再对条目做字符级扫描——顶层 `;` 或行尾顶层 `,`（cfg(test) 的
/// struct 字段）即条目结束；顶层 `{` 进入块模式，花括号归零结束。圆/方
/// 括号内的 `,`/`;` 不算顶层。这是面向本 crate 源码的启发式扫描。
fn source_scan_skip_gated_item<'a>(lines: &mut impl Iterator<Item = &'a str>) {
    let mut current: &str = loop {
        match lines.next() {
            Some(line) if line.trim().is_empty() || line.trim_start().starts_with('#') => {
                continue;
            }
            Some(line) => break line,
            None => return,
        }
    };

    let (mut round, mut square, mut curly) = (0_i32, 0_i32, 0_i32);
    let mut in_body = false;
    loop {
        for ch in current.chars() {
            if in_body {
                match ch {
                    '{' => curly += 1,
                    '}' => curly -= 1,
                    _ => {}
                }
                if curly == 0 {
                    return;
                }
                continue;
            }
            match ch {
                '(' => round += 1,
                ')' => round -= 1,
                '[' => square += 1,
                ']' => square -= 1,
                '{' => {
                    curly += 1;
                    if round == 0 && square == 0 {
                        in_body = true;
                    }
                }
                '}' => curly -= 1,
                ';' if curly == 0 && round == 0 && square == 0 => return,
                _ => {}
            }
        }
        if !in_body && curly == 0 && round == 0 && square == 0 && current.trim_end().ends_with(',')
        {
            return;
        }
        match lines.next() {
            Some(next) => current = next,
            None => return,
        }
    }
}

/// 返回去掉全部 cfg(test)/cfg(all(test, ...)) 条目后的源码，使测试专用
/// helper（如 `Command::new` fixture）不被计成生产 spawn 点。
fn source_scan_strip_test_items(source: &str) -> String {
    let mut kept = String::with_capacity(source.len());
    let mut lines = source.lines();
    while let Some(line) = lines.next() {
        if source_scan_attr_is_test_only(line) {
            source_scan_skip_gated_item(&mut lines);
            continue;
        }
        kept.push_str(line);
        kept.push('\n');
    }
    kept
}

/// 终端模块的生产源码集合：模块根 `src/terminal.rs` 加上 `src/terminal/`
/// 下递归找到的全部 `.rs` 文件（`tests.rs` 即本文件除外）。目录条目逐个
/// 遍历、不写死文件清单，新增文件自动纳入扫描。返回按仓库相对路径排序的
/// `(相对路径, 源码文本)`。
fn source_scan_terminal_sources() -> Vec<(String, String)> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let src_dir = manifest_dir.join("src");
    let mut paths = vec![src_dir.join("terminal.rs")];
    let mut stack = vec![src_dir.join("terminal")];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir)
            .unwrap_or_else(|error| panic!("无法读取源码目录 {}: {error}", dir.display()));
        for entry in entries {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|ext| ext == "rs")
                && path.file_name().is_some_and(|name| name != "tests.rs")
            {
                paths.push(path);
            }
        }
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let source = fs::read_to_string(&path)
                .unwrap_or_else(|error| panic!("无法读取源码文件 {}: {error}", path.display()));
            let rel = path
                .strip_prefix(manifest_dir)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            (rel, source)
        })
        .collect()
}

#[test]
fn terminal_injection_redline_source_scan() {
    // 1) 禁入变量全集仍在清除清单内。直接断言常量本身，不依赖源码文本。
    for key in SOURCE_SCAN_EXPECTED_CLEARED_ENV_KEYS {
        assert!(
            CHILD_PROCESS_ENV_KEYS_TO_CLEAR.contains(key),
            "CHILD_PROCESS_ENV_KEYS_TO_CLEAR 缺少禁入变量 {key}"
        );
    }
    assert_eq!(
        auth_core::WEBCLX_LOCAL_API_TOKEN_ENV,
        "WEBCLX_LOCAL_API_TOKEN",
        "本地 API token 环境变量名变更时同步更新 SOURCE_SCAN_EXPECTED_CLEARED_ENV_KEYS"
    );

    // 2)+3) spawn-site 与 runuser/--login 扫描。
    let mut violations: Vec<String> = Vec::new();
    let mut runuser_sites: Vec<String> = Vec::new();
    for (rel_path, source) in source_scan_terminal_sources() {
        let stripped = source_scan_strip_test_items(&source);
        let spawn_count = stripped.matches("Command::new(").count()
            + stripped.matches("CommandBuilder::new(").count();
        match SOURCE_SCAN_UNWASHED_SPAWN_ALLOWLIST
            .iter()
            .find(|entry| entry.0 == rel_path)
        {
            Some(&(_, expected, _)) if spawn_count != expected => violations.push(format!(
                "{rel_path}: Command::new/CommandBuilder::new 计数 {spawn_count} 与豁免记录 \
                 {expected} 不一致；新增裸 spawn 请改用 sanitize_child_command / \
                 sanitize_pty_command，确属非终端 OS 工具时更新本测试的豁免清单"
            )),
            Some(_) => {}
            None if spawn_count > 0
                && !stripped.contains("sanitize_child_command(&mut")
                && !stripped.contains("sanitize_pty_command(&mut") =>
            {
                violations.push(format!(
                    "{rel_path}: 含 {spawn_count} 处 Command::new/CommandBuilder::new 但没有任何 \
                     sanitize_*(&mut …) 调用；新子进程必须先按 \
                     CHILD_PROCESS_ENV_KEYS_TO_CLEAR 清环境，或把文件登记进豁免清单"
                ));
            }
            None => {}
        }
        if stripped.contains("--login") {
            violations.push(format!(
                "{rel_path}: 出现 \"--login\"；AGENTS.md 要求新终端登录启动只走 \
                 `runuser -u USER -- SHELL -l`，禁止与 -u 并用 --login"
            ));
        }
        let runuser_count = stripped.matches("arg(\"runuser\")").count();
        if runuser_count > 0 {
            runuser_sites.push(format!("{rel_path} x{runuser_count}"));
        }
    }
    if runuser_sites != ["src/terminal/tmux.rs x1".to_string()] {
        violations.push(format!(
            "runuser argv 只允许在 src/terminal/tmux.rs 的 append_runuser_login_shell \
             构造一次，实际命中: {runuser_sites:?}"
        ));
    }
    assert!(violations.is_empty(), "终端注入红线源码扫描失败:\n{}", violations.join("\n"));
}

#[test]
fn terminal_close_rate_limit_throttles_rapid_bulk_destructions() {
    let test_ip = std::net::Ipv4Addr::new(127, 0, 0, 88).into();
    if let Ok(mut tracker) = super::TERMINAL_CLOSE_TRACKER.lock() {
        tracker.remove(&test_ip);
    }
    for i in 1..=5 {
        assert!(
            super::check_terminal_close_rate_limit(test_ip).is_ok(),
            "attempt {i} should pass"
        );
    }
    let err =
        super::check_terminal_close_rate_limit(test_ip).expect_err("6th attempt must be throttled");
    assert_eq!(err.status, axum::http::StatusCode::TOO_MANY_REQUESTS);
}

#[test]
fn message_submission_queue_suppresses_extra_enter() {
    let sample = "■ Conversation interrupted - use /feedback if something went wrong\n\nCompacting context (9s • esc to interrupt)\n  └ Making room to continue.\n\n• Messages to be submitted at end of turn\n  ↳ 我现在使用sub2api正在测上游sub2freeApi，结果日志并没有体现\n\n› Ask Codex to do anything\n\n  GPT-6.1-Sol high · Context 45% used · Fast off";
    let agy = "> 软链接当前啥情况\n\n● Ran 3 commands (ctrl+o to expand)\n\n⣟  Running command...\n▸ 你知道吗 (alt+m to send immediately)\n──────────────────────\n>\n──────────────────────\n  Press up to edit queued messages";
    // CodeBuddy v2.168.2: captured from a real 80-column interactive PTY
    // while Bash sleep 40 was running, after submitting a follow-up with Enter.
    let codebuddy = "● REPL · sleep 40 seconds for terminal display test\n  └ ...running 5s ago → Bash · sleep 40\n\n✺ Locating… (10s · running REPL · ⚒ 9 tokens)\n\n Messages to be submitted after next tool call (press esc to interrupt and send\n immediately)\n ↳ QUEUE_PROBE_ONE 请只回复 QUEUE_ACK\n──────────────────────\n>\n──────────────────────";
    for snapshot in [
        sample,
        "• Queued follow-up inputs\n  ↳ 下一条消息",
        agy,
        codebuddy,
        "Messages to be submitted after next tool call (press esc to interrupt and send immediately)",
        "Messages to be submitted after next tool call\n(press esc to interrupt and send immediately)",
        "▸ 你知道吗 (alt+m to send immediately)",
        "  Press up to edit queued messages",
    ] {
        assert!(
            !super::manager::submit_message_enter_unless_queued(snapshot, || {
                panic!("Agent queue must suppress retry Enter")
            })
            .unwrap()
        );
        let enters = std::cell::Cell::new(0);
        assert_eq!(
            super::manager::submit_scheduled_message(
                || Ok(true),
                || {
                    enters.set(enters.get() + 1);
                    Ok(())
                },
                || Ok(false),
                || {},
            )
            .unwrap(),
            "confirmed"
        );
        assert_eq!(
            enters.get(),
            1,
            "an existing queue must not swallow the new body's first Enter"
        );
    }
    for snapshot in [
        "⣟  Running command...\n> 用户正在输入",
        "> 请解释 Press up to edit queued messages",
        "▸ 普通输出文字",
        "> Messages to be submitted after next tool call (press esc to interrupt and send immediately)",
        "● REPL · sleep 40 seconds for terminal display test\n✺ Locating… (10s · running REPL)\n> QUEUE_PROBE_ONE 草稿",
        "Messages to be submitted after next tool callback",
    ] {
        let mut enters = 0;
        assert!(
            super::manager::submit_message_enter_unless_queued(snapshot, || {
                enters += 1;
                Ok(())
            })
            .unwrap()
        );
        assert_eq!(enters, 1);
    }
    let mut enters = 0;
    assert!(
        super::manager::submit_message_enter_unless_queued(
            "› 未提交草稿\nGPT-6.1-Sol high",
            || {
                enters += 1;
                Ok(())
            }
        )
        .unwrap()
    );
    assert_eq!(enters, 1);
    assert!(
        super::manager::submit_message_enter_unless_queued("› 草稿", || anyhow::bail!(
            "write failed"
        ))
        .is_err()
    );
}

#[test]
fn scheduled_submission_queue_appearing_after_enter_stops_retry() {
    use std::cell::Cell;
    let enters = Cell::new(0);
    assert_eq!(
        super::manager::submit_scheduled_message(
            || Ok(enters.get() > 0),
            || {
                enters.set(enters.get() + 1);
                Ok(())
            },
            || Ok(false),
            || {},
        )
        .unwrap(),
        "confirmed"
    );
    assert_eq!(enters.get(), 1);
}
