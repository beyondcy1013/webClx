use std::{
    cmp::Ordering as CmpOrdering,
    collections::{HashMap, HashSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use settings_core::TerminalErrorKeywordAction;
use sha2::{Digest, Sha256};
use terminal_core::*;
use time::{Date, Month, OffsetDateTime, Time};
use tokio::sync::{Notify, broadcast};
use tracing::{info, warn};

use crate::{
    atomic_file, codex_launch::prepare_codex_history_command_for_user, filesystem, runtime_paths,
    settings::SettingsManager,
};

use super::{
    MAX_TERMINAL_INPUT_HISTORY_ENTRIES, MAX_TERMINAL_INPUT_HISTORY_LINE_BYTES,
    SESSION_EVENT_CHANNEL_CAPACITY, ScheduledInputUpdate,
    StoredTerminalAutoContinueScheduleRegistry, StoredTerminalPendingBuildRegistry,
    StoredTerminalRegistry, StoredTerminalScheduledInputRegistry, StoredTerminalSession,
    TERMINAL_MESSAGE_DELIVERY_LEDGER_FILE_NAME, TERMINAL_PENDING_BUILD_MAX_AGE_MS,
    TERMINAL_RECENT_OUTPUT_ACTIVE_MS, TERMINAL_WORKSPACE_RESTORE_FILE_NAME,
    TerminalActivitySnapshot, TerminalAutoContinueSchedule, TerminalAutoContinueSendOutcome,
    TerminalAutoContinueTask, TerminalEnvironmentSnapshot, TerminalErrorAutoContinueRecord,
    TerminalInputHistoryCapture, TerminalInputHistoryEntry, TerminalManager, TerminalManagerEvent,
    TerminalOutputObservation, TerminalPendingBuildRequest, TerminalPresetExtractionResponse,
    TerminalResumeRestoreRecord, TerminalScheduledInputTask, TerminalScheduledInputTaskInfo,
    TerminalSessionInfo, TerminalSessionOrigin, TerminalSessionSearchMatch,
    TerminalShutdownRestoreRegistry, TerminalState, TerminalWorkspaceRestoreFailure,
    TerminalWorkspaceRestoreInfo, TerminalWorkspaceRestoreRecord, TerminalWorkspaceRestoreRegistry,
    TerminalWorkspaceRestoreResult, TerminalWorkspaceRestoreSkip,
    activity::{
        TerminalAgentActivity, TerminalAgentDetector, terminal_compact_command_blocked_agent,
    },
    agent_session::{
        DetectedResumeSession, ResumeSessionDetector, current_resume_agent_process_ids,
        detect_current_resume_session, detect_current_resume_session_complete,
        detect_current_session_rollout_path, parse_rollout_user_messages, tmux_pane_current_path,
    },
    default_terminal_user_name, load_terminal_shutdown_restore_registry,
    persist_terminal_shutdown_restore_registry,
    session::TerminalSession,
    sort_terminal_shutdown_restore_records,
    tmux::{
        TmuxSessionStatus, capture_tmux_activity_pane_snapshot, capture_tmux_recent_pane_snapshot,
        capture_tmux_text_pane_snapshot, capture_tmux_visible_text_pane_snapshot,
        create_fresh_tmux_session, destroy_tmux_session, detach_tmux_clients_for_sessions,
        ensure_tmux_session, send_tmux_enter_key, send_tmux_input,
        terminate_tmux_pane_process_tree, tmux_session_status, tmux_socket_name,
    },
};

pub(super) mod activity_probe;
mod cleanup;
use activity_probe::{
    PROBE_SNAPSHOT_MAX_AGE, ProbeSnapshotCache, ProbeSnapshotCacheEntry, map_activity_probes,
};
mod crontab;
mod error_detection;
mod output_fingerprint;
use output_fingerprint::terminal_output_fingerprint;
pub(crate) mod message_ledger;
use message_ledger::{DeliveryRecord, DeliveryState, TerminalMessageLedger};
pub(crate) mod scheduled_input;
#[cfg(test)]
mod scheduled_input_repeat_tests;
pub(in crate::terminal) use crontab::parse_auto_continue_due_epochs;

use crontab::{
    current_crontab, install_crontab, rewrite_crontab_without_markers,
    sanitize_cron_file_component, set_executable_mode, shell_quote_cron,
};

use scheduled_input::{
    claim_and_persist_due_scheduled_input_tasks, load_terminal_scheduled_input_tasks,
    normalize_scheduled_input_text, persist_terminal_scheduled_input_registry,
    prune_completed_scheduled_input_tasks, scheduled_input_task_info, scheduled_input_task_infos,
};

pub(in crate::terminal) type TerminalScheduledInputBatchSender =
    Arc<dyn Fn(&TerminalManager, Vec<TerminalScheduledInputTask>) + Send + Sync>;

pub(in crate::terminal) type TerminalScheduledInputOneSender =
    Arc<dyn Fn(&TerminalManager, &TerminalScheduledInputTask) -> Result<()> + Send + Sync>;

#[derive(Default)]
struct RestoreShutdownResult {
    succeeded: HashSet<String>,
    skipped: Vec<(String, String)>,
    failed: Vec<(String, String)>,
}

use error_detection::{
    TerminalErrorKeywordMatch, compact_terminal_search_line, count_non_overlapping_matches,
    terminal_cline_idle_prompt_from_snapshot, terminal_error_keyword_match,
    terminal_error_keyword_match_from_snapshot,
};

pub(super) use error_detection::{
    terminal_worked_status_match_from_snapshot, terminal_working_status_match_from_snapshot,
};

pub(super) use error_detection::is_terminal_continue_line;

#[cfg(test)]
pub(super) use error_detection::{
    terminal_error_has_continue_after, terminal_error_has_queued_input_after,
    terminal_error_reset_time_from_tail, terminal_tail_error_keyword,
    terminal_tail_error_keyword_with_manual_interrupt_policy, terminal_tail_has_worked_status,
    terminal_tail_has_working_status,
};

const TERMINAL_AUTO_CONTINUE_RESET_GRACE_MS: u64 = 60_000;

const TERMINAL_AUTO_CONTINUE_CRON_FALLBACK_DELAY_MS: u64 = 5_000;
const TERMINAL_ERROR_AUTO_CONTINUE_SCAN_MS: u64 = 5_000;
const TERMINAL_AUTO_CONTINUE_CRON_MARKER_PREFIX: &str = "webclx-auto-continue";
const TERMINAL_AUTO_CONTINUE_HISTORY_FILE_NAME: &str = "terminal-auto-continue-history.json";
const TERMINAL_CONTINUE_COMMAND: &str = "继续";
const TERMINAL_COMMAND_ENTER_DELAY_MS: u64 = 120;
const TERMINAL_PRESET_STATUS_TIMEOUT: Duration = Duration::from_secs(7);
const TERMINAL_PRESET_STATUS_POLL: Duration = Duration::from_millis(120);
const TERMINAL_FORCE_RESUME_EXIT_TIMEOUT: Duration = Duration::from_secs(4);
const TERMINAL_FORCE_RESUME_EXIT_POLL: Duration = Duration::from_millis(100);
const TERMINAL_FORCE_RESUME_SHELL_SETTLE: Duration = Duration::from_millis(250);
const TERMINAL_ACTIVITY_PROBE_CACHE_TTL: Duration = Duration::from_secs(1);
const TERMINAL_CHILD_REAPER_INTERVAL: Duration = Duration::from_secs(1);
/// Slash command that compacts the conversation to free context-window room.
const TERMINAL_COMPACT_COMMAND: &str = "/compact";
/// Delay between typing a slash command and the first Enter, mirroring the
/// frontend MOBILE_SLASH_COMMAND_ENTER_DELAY_MS so Codex registers the command.
const TERMINAL_SLASH_COMMAND_ENTER_DELAY_MS: u64 = 500;
/// Delay between the first and second Enter for a slash command, mirroring the
/// frontend MOBILE_SLASH_COMMAND_CONFIRM_DELAY_MS so Codex confirms the dialog.
const TERMINAL_SLASH_COMMAND_CONFIRM_DELAY_MS: u64 = 120;
static TERMINAL_ACTIVITY_PROBE_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const CODEX_COMMAND_ENV_WRAPPER_HEADER: &str = "#!/bin/sh\n# WEBCLX_CODEX_COMMAND_ENV_WRAPPER\n";
// An auto-continue cron entry is a one-shot moment (M H DoM Mo *). If its next
// calendar occurrence is more than a day in the future it means that one-shot
// moment already passed this year without firing and the entry is an orphan
// that will not re-arm within the same year. Treat it as expired so it is
// pruned and archived instead of lingering in the task list forever.
// Cap how many expired entries we keep in the history file.
const TERMINAL_AUTO_CONTINUE_HISTORY_MAX: usize = 200;

/// Effective cooldown for the Nth consecutive auto-continue attempt.
///
/// `base_interval_millis` is the configured flat interval and `backoff_factor`
/// is the per-attempt multiplier (e.g. 1.5). The first attempt uses the base;
/// each subsequent attempt multiplies by `backoff_factor^(attempts-1)`,
/// saturating at `backoff_max_millis`. The factor is clamped to >= 1.0 and the
/// configured cap cannot shrink the wait below the base interval.
pub(in crate::terminal) fn auto_continue_backoff_interval_millis(
    base_interval_millis: u64,
    consecutive_attempts: u32,
    backoff_factor: f64,
    backoff_max_millis: u64,
) -> u64 {
    let base = base_interval_millis.max(1);
    if consecutive_attempts <= 1 {
        return base;
    }
    // Guard against NaN / sub-unit factors that would shrink the wait.
    let factor = if backoff_factor.is_nan() || backoff_factor < 1.0 {
        1.0
    } else {
        backoff_factor
    };
    let exponent = (consecutive_attempts - 1).min(20) as i32;
    let multiplier = factor.powi(exponent);
    let grown = (base as f64 * multiplier).round();
    let effective_max = backoff_max_millis.max(base);
    grown.clamp(base as f64, effective_max as f64) as u64
}

pub(in crate::terminal) fn auto_continue_retry_at_millis(
    last_sent_at_millis: Option<u64>,
    interval_millis: u64,
    now_millis: u64,
) -> Option<u64> {
    let retry_at_millis = last_sent_at_millis?.saturating_add(interval_millis.max(1));
    (now_millis < retry_at_millis).then_some(retry_at_millis)
}

impl TerminalManager {
    #[allow(dead_code)] // 仅被测试构造路径使用（login/upstream_proxy 的 #[cfg(test)]）。
    pub fn new(state_file: PathBuf) -> Self {
        cleanup_legacy_command_env_dir(&state_file);
        let archive_file = state_file.with_file_name(RESUME_ARCHIVE_FILE_NAME);
        let scheduled_input_file =
            state_file.with_file_name(super::TERMINAL_SCHEDULED_INPUT_FILE_NAME);
        let auto_continue_file =
            state_file.with_file_name(super::TERMINAL_AUTO_CONTINUE_SCHEDULE_FILE_NAME);
        let pending_build_file = state_file.with_file_name(super::TERMINAL_PENDING_BUILD_FILE_NAME);
        let (mut state, next_ordinal, dirty) = load_terminal_state(&state_file, &HashSet::new());
        arm_output_observations_for_restore_locked(&mut state);
        let scheduled_input_tasks = load_terminal_scheduled_input_tasks(&scheduled_input_file);
        let auto_continue_schedules = load_terminal_auto_continue_tasks(&auto_continue_file);
        let pending_build_requests = load_terminal_pending_build_requests(&pending_build_file);
        let (event_sender, _) = broadcast::channel(SESSION_EVENT_CHANNEL_CAPACITY);
        let manager = Self {
            state: Arc::new(std::sync::RwLock::new(state)),
            state_file: Arc::new(state_file),
            registry_write_lock: Arc::new(Mutex::new(())),
            workspace_restore_write_lock: Arc::new(Mutex::new(())),
            input_persist_work: Arc::new(Mutex::new((false, false))),
            archive_file: Arc::new(archive_file),
            scheduled_input_file: Arc::new(scheduled_input_file),
            auto_continue_file: Arc::new(auto_continue_file),
            pending_build_file: Arc::new(pending_build_file),
            shutdown_restore_file: Arc::new(PathBuf::new()),
            workspace_restore_file: Arc::new(PathBuf::new()),
            env_snapshot: Arc::new(TerminalEnvironmentSnapshot {
                workspace_root: PathBuf::new(),
                display_root: PathBuf::new(),
                user_profile: runtime_paths::resolve_current_user_profile().unwrap_or_else(|| {
                    runtime_paths::resolve_user_profile(&default_terminal_user_name())
                        .expect("default terminal user should resolve")
                }),
                proxy_env: Vec::new(),
            }),
            next_id: Arc::new(AtomicU64::new(next_ordinal.max(1))),
            event_sender,
            auto_continue_schedules: Arc::new(std::sync::Mutex::new(auto_continue_schedules)),
            auto_continue_notify: Arc::new(Notify::new()),
            per_session_runtime: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_build_requests: Arc::new(std::sync::Mutex::new(pending_build_requests)),
            auto_continue_interval_seconds: Arc::new(AtomicU64::new(u64::from(
                settings_core::DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS,
            ))),
            auto_continue_backoff_factor: Arc::new(AtomicU64::new(
                (settings_core::DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_FACTOR * 1000.0) as u64,
            )),
            auto_continue_backoff_max_millis: Arc::new(AtomicU64::new(
                u64::from(settings_core::DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_MAX_MINUTES)
                    * 60
                    * 1000,
            )),
            auto_continue_respect_manual_interrupt: Arc::new(std::sync::atomic::AtomicBool::new(
                settings_core::DEFAULT_TERMINAL_AUTO_CONTINUE_RESPECT_MANUAL_INTERRUPT,
            )),
            quota_reset_cache: crate::quota_reset_cache::QuotaResetCache::new(),
            api_preset_snapshot: Arc::new(std::sync::RwLock::new(Vec::new())),
            scheduled_input_tasks: Arc::new(std::sync::Mutex::new(scheduled_input_tasks)),
            scheduled_input_notify: Arc::new(Notify::new()),
            activity_probe_cache: Arc::new(std::sync::Mutex::new(
                TerminalActivityProbeCache::default(),
            )),
            activity_probe_scan_lock: Arc::new(std::sync::Mutex::new(())),
            probe_snapshot_cache: Arc::new(std::sync::Mutex::new(ProbeSnapshotCache::new())),
            service_restart_pending: Arc::new(AtomicBool::new(false)),
            // 测试构造路径不持久化投递账本。
            terminal_message_ledger: Arc::new(Mutex::new(TerminalMessageLedger::in_memory())),
        };

        if dirty {
            manager.persist_state();
        }
        manager.restore_live_sessions();
        manager.spawn_auto_continue_runner();
        manager.spawn_scheduled_input_runner();

        manager
    }

    #[allow(dead_code)] // 保留同步恢复构造路径，供恢复语义测试使用。
    pub fn new_with_environment(
        state_file: PathBuf,
        env_snapshot: TerminalEnvironmentSnapshot,
        quota_reset_cache: crate::quota_reset_cache::QuotaResetCache,
    ) -> Self {
        Self::new_with_environment_restore_mode(state_file, env_snapshot, quota_reset_cache, false)
    }

    pub fn new_with_environment_deferred_restore(
        state_file: PathBuf,
        env_snapshot: TerminalEnvironmentSnapshot,
        quota_reset_cache: crate::quota_reset_cache::QuotaResetCache,
    ) -> Self {
        Self::new_with_environment_restore_mode(state_file, env_snapshot, quota_reset_cache, true)
    }

    fn new_with_environment_restore_mode(
        state_file: PathBuf,
        env_snapshot: TerminalEnvironmentSnapshot,
        quota_reset_cache: crate::quota_reset_cache::QuotaResetCache,
        defer_restore: bool,
    ) -> Self {
        cleanup_legacy_command_env_dir(&state_file);
        let archive_file = state_file.with_file_name(RESUME_ARCHIVE_FILE_NAME);
        let scheduled_input_file =
            state_file.with_file_name(super::TERMINAL_SCHEDULED_INPUT_FILE_NAME);
        let auto_continue_file =
            state_file.with_file_name(super::TERMINAL_AUTO_CONTINUE_SCHEDULE_FILE_NAME);
        let pending_build_file = state_file.with_file_name(super::TERMINAL_PENDING_BUILD_FILE_NAME);
        let shutdown_restore_file =
            state_file.with_file_name(super::TERMINAL_SHUTDOWN_RESTORE_FILE_NAME);
        let workspace_restore_file =
            state_file.with_file_name(TERMINAL_WORKSPACE_RESTORE_FILE_NAME);
        let message_ledger_file =
            state_file.with_file_name(TERMINAL_MESSAGE_DELIVERY_LEDGER_FILE_NAME);
        let shutdown_registry =
            load_terminal_shutdown_restore_registry(&shutdown_restore_file).unwrap_or_default();
        let restore_records = shutdown_registry.records;
        let restore_ids = restore_records
            .iter()
            .map(|record| record.session_id.clone())
            .collect::<HashSet<_>>();
        let (mut state, next_ordinal, dirty) = load_terminal_state(&state_file, &restore_ids);
        arm_output_observations_for_restore_locked(&mut state);
        let scheduled_input_tasks = load_terminal_scheduled_input_tasks(&scheduled_input_file);
        let auto_continue_schedules = load_terminal_auto_continue_tasks(&auto_continue_file);
        let pending_build_requests = load_terminal_pending_build_requests(&pending_build_file);
        let (event_sender, _) = broadcast::channel(SESSION_EVENT_CHANNEL_CAPACITY);
        let manager = Self {
            state: Arc::new(std::sync::RwLock::new(state)),
            state_file: Arc::new(state_file),
            registry_write_lock: Arc::new(Mutex::new(())),
            workspace_restore_write_lock: Arc::new(Mutex::new(())),
            input_persist_work: Arc::new(Mutex::new((false, false))),
            archive_file: Arc::new(archive_file),
            scheduled_input_file: Arc::new(scheduled_input_file),
            auto_continue_file: Arc::new(auto_continue_file),
            pending_build_file: Arc::new(pending_build_file),
            shutdown_restore_file: Arc::new(shutdown_restore_file),
            workspace_restore_file: Arc::new(workspace_restore_file),
            env_snapshot: Arc::new(env_snapshot),
            next_id: Arc::new(AtomicU64::new(next_ordinal.max(1))),
            event_sender,
            auto_continue_schedules: Arc::new(std::sync::Mutex::new(auto_continue_schedules)),
            auto_continue_notify: Arc::new(Notify::new()),
            per_session_runtime: Arc::new(std::sync::Mutex::new(HashMap::new())),
            pending_build_requests: Arc::new(std::sync::Mutex::new(pending_build_requests)),
            auto_continue_interval_seconds: Arc::new(AtomicU64::new(u64::from(
                settings_core::DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS,
            ))),
            auto_continue_backoff_factor: Arc::new(AtomicU64::new(
                (settings_core::DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_FACTOR * 1000.0) as u64,
            )),
            auto_continue_backoff_max_millis: Arc::new(AtomicU64::new(
                u64::from(settings_core::DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_MAX_MINUTES)
                    * 60
                    * 1000,
            )),
            auto_continue_respect_manual_interrupt: Arc::new(std::sync::atomic::AtomicBool::new(
                settings_core::DEFAULT_TERMINAL_AUTO_CONTINUE_RESPECT_MANUAL_INTERRUPT,
            )),
            quota_reset_cache,
            api_preset_snapshot: Arc::new(std::sync::RwLock::new(Vec::new())),
            scheduled_input_tasks: Arc::new(std::sync::Mutex::new(scheduled_input_tasks)),
            scheduled_input_notify: Arc::new(Notify::new()),
            activity_probe_cache: Arc::new(std::sync::Mutex::new(
                TerminalActivityProbeCache::default(),
            )),
            activity_probe_scan_lock: Arc::new(std::sync::Mutex::new(())),
            probe_snapshot_cache: Arc::new(std::sync::Mutex::new(ProbeSnapshotCache::new())),
            service_restart_pending: Arc::new(AtomicBool::new(false)),
            terminal_message_ledger: Arc::new(Mutex::new(TerminalMessageLedger::load(Some(
                message_ledger_file,
            )))),
        };

        if dirty {
            manager.persist_state();
        }
        if let Err(error) = manager.persist_all_workspace_restores() {
            warn!(
                path = %manager.workspace_restore_file.display(),
                "persist initial terminal workspace restores failed: {error}"
            );
        }
        if defer_restore {
            manager.spawn_deferred_initial_restore(restore_records);
        } else {
            manager.restore_initial_sessions(&restore_records);
            manager.spawn_runtime_runners();
        }

        manager
    }

    fn restore_initial_sessions(&self, restore_records: &[TerminalResumeRestoreRecord]) {
        if !restore_records.is_empty() {
            self.restore_shutdown_sessions(restore_records);
        }
        self.restore_live_sessions();
    }

    fn spawn_runtime_runners(&self) {
        self.spawn_auto_continue_runner();
        self.spawn_scheduled_input_runner();
        self.spawn_child_reaper_runner();
    }

    fn spawn_child_reaper_runner(&self) {
        // Synchronous test constructors do not have a Tokio runtime. The
        // production manager always starts this runner with the service.
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let manager = self.clone();
        tokio::spawn(async move {
            manager.run_child_reaper_loop().await;
        });
    }

    async fn run_child_reaper_loop(self) {
        loop {
            reap_direct_tmux_client_zombies();
            let sessions = {
                let state = crate::lock_or_recover!(self.state.read());
                state.live_sessions.values().cloned().collect::<Vec<_>>()
            };
            for session in sessions {
                session.reap_child_if_exited();
            }
            tokio::time::sleep(TERMINAL_CHILD_REAPER_INTERVAL).await;
        }
    }

    fn spawn_deferred_initial_restore(&self, restore_records: Vec<TerminalResumeRestoreRecord>) {
        let manager = self.clone();
        let persisted_session_count = {
            let state = crate::lock_or_recover!(self.state.read());
            state.sessions_by_id.len()
        };
        info!(
            persisted_session_count,
            shutdown_restore_count = restore_records.len(),
            "terminal initial restore scheduled"
        );

        tokio::spawn(async move {
            let restore_manager = manager.clone();
            let started_at = Instant::now();
            let result = tokio::task::spawn_blocking(move || {
                restore_manager.restore_initial_sessions(&restore_records);
            })
            .await;
            match result {
                Ok(()) => info!(
                    persisted_session_count,
                    elapsed_ms = started_at.elapsed().as_millis(),
                    "terminal initial restore completed"
                ),
                Err(error) => warn!(
                    persisted_session_count,
                    elapsed_ms = started_at.elapsed().as_millis(),
                    "terminal initial restore task failed: {error}"
                ),
            }
            manager.spawn_runtime_runners();
        });
    }

    #[cfg(test)]
    pub(in crate::terminal) fn insert_test_live_session(
        &self,
        stored: StoredTerminalSession,
        live: std::sync::Arc<TerminalSession>,
    ) {
        {
            let mut state = crate::lock_or_recover!(self.state.write());
            state.sessions_by_id.insert(stored.id.clone(), stored);
            state.live_sessions.insert(live.id.clone(), live.clone());
            let session_id = live.id.clone();
            let path = state.sessions_by_id[session_id.as_str()].path.clone();
            state
                .sessions_by_path
                .entry(path)
                .or_default()
                .push(session_id);
            state.previous_terminal_names = state
                .sessions_by_id
                .values()
                .flat_map(|session| {
                    session
                        .previous_names
                        .iter()
                        .map(|previous_name| (previous_name.clone(), session.id.clone()))
                })
                .collect();
        }
    }

    pub(in crate::terminal) fn session_path(&self, session_id: &str) -> Option<PathBuf> {
        let state = crate::lock_or_recover!(self.state.read());
        state
            .sessions_by_id
            .get(session_id)
            .map(|session| session.path.clone())
    }

    /// 取该会话的启动串行锁。只在 map 锁内克隆出 Arc，map 锁随本函数返回
    /// 立即释放；调用方之后再锁内层 Mutex。绝不把外层 map 锁压过
    /// 一次会话启动等待，否则所有会话的启动都会全局串行。
    fn session_start_lock(&self, session_id: &str) -> Arc<Mutex<()>> {
        crate::lock_or_recover!(self.per_session_runtime.lock())
            .entry(session_id.to_string())
            .or_default()
            .start_lock
            .clone()
    }

    /// 取该会话的发送串行锁，语义同 `session_start_lock`：continue/compact
    /// 的冷却检查、tmux 写入与记账必须在同一把锁内串行，但 map 锁只在
    /// 克隆 Arc 的瞬间持有——发送路径会取 `state` 读锁，压过发送会与
    /// `state → per_session_runtime` 的持锁顺序构成 ABBA 死锁。
    fn session_send_lock(&self, session_id: &str) -> Arc<Mutex<()>> {
        crate::lock_or_recover!(self.per_session_runtime.lock())
            .entry(session_id.to_string())
            .or_default()
            .send_lock
            .clone()
    }

    /// 认领已预留会话的启动门，同时不违反锁序。
    ///
    /// 调用方此刻持有 `state` 写锁：直接把 `start_lock.lock()` 压进持有区
    /// 会形成 `state → start_lock` 顺序，与先持有 start_lock 再取 state
    /// 的路径（发送/删除等）构成潜在 ABBA。无竞争时先取得启动门再释放
    /// state（预约发布顺序不变）；有竞争时先放 state 再阻塞等锁，拿到锁
    /// 后以读锁复核预约未被取消或替换（id+created_at+path 一致），再把
    /// 启动门交还调用方——慢速后端启动绝不压着全局写锁等待。
    fn lock_reserved_session_start<'a>(
        &self,
        start_lock: &'a Mutex<()>,
        state: std::sync::RwLockWriteGuard<'_, TerminalState>,
        reserved: &StoredTerminalSession,
    ) -> Result<std::sync::MutexGuard<'a, ()>> {
        match start_lock.try_lock() {
            Ok(guard) => {
                drop(state);
                Ok(guard)
            }
            Err(std::sync::TryLockError::Poisoned(error)) => {
                drop(state);
                Ok(error.into_inner())
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                drop(state);
                let guard = crate::lock_or_recover!(start_lock.lock());
                let state = crate::lock_or_recover!(self.state.read());
                anyhow::ensure!(
                    state
                        .sessions_by_id
                        .get(&reserved.id)
                        .is_some_and(|current| {
                            current.created_at == reserved.created_at
                                && current.path == reserved.path
                        }),
                    "终端会话创建已取消或替换。"
                );
                Ok(guard)
            }
        }
    }

    pub(in crate::terminal) fn current_working_directory(
        &self,
        session_id: &str,
    ) -> Result<PathBuf> {
        let (stored_id, stored_path) = {
            let state = crate::lock_or_recover!(self.state.read());
            let session = state
                .sessions_by_id
                .get(session_id)
                .with_context(|| format!("会话 `{session_id}` 不存在"))?;
            (session.id.clone(), session.path.clone())
        };

        Ok(tmux_pane_current_path(&stored_id).unwrap_or(stored_path))
    }

    pub(crate) fn has_session(&self, session_id: &str) -> bool {
        crate::lock_or_recover!(self.state.read())
            .sessions_by_id
            .contains_key(session_id)
    }

    pub(in crate::terminal) fn session_names_by_id(&self) -> HashMap<String, String> {
        let state = crate::lock_or_recover!(self.state.read());
        state
            .sessions_by_id
            .iter()
            .map(|(id, session)| (id.clone(), session.name.clone()))
            .collect()
    }

    pub(crate) fn register_pending_build_request(&self, request_id: &str, session_id: &str) {
        let request_id = request_id.trim();
        let session_id = session_id.trim();
        if request_id.is_empty() || session_id.is_empty() {
            return;
        }

        let now = current_timestamp_millis();
        let mut requests = crate::lock_or_recover!(self.pending_build_requests.lock());
        requests.retain(|_, request| terminal_pending_build_request_is_current(request, now));
        requests.insert(
            request_id.to_string(),
            TerminalPendingBuildRequest {
                request_id: request_id.to_string(),
                session_id: session_id.to_string(),
                queued_at_millis: now,
            },
        );
        self.persist_pending_build_requests_locked(&requests);
        drop(requests);
        self.notify_session_list_changed("build_queued", session_id);
    }

    pub(crate) fn complete_pending_build_request(&self, request_id: &str) -> bool {
        let request_id = request_id.trim();
        if request_id.is_empty() {
            return false;
        }

        let mut requests = crate::lock_or_recover!(self.pending_build_requests.lock());
        let Some(request) = requests.remove(request_id) else {
            return false;
        };
        self.persist_pending_build_requests_locked(&requests);
        drop(requests);
        self.notify_session_list_changed("build_completed", request.session_id);
        true
    }

    fn pending_build_session_ids(&self) -> HashSet<String> {
        let now = current_timestamp_millis();
        let mut requests = crate::lock_or_recover!(self.pending_build_requests.lock());
        let previous_len = requests.len();
        requests.retain(|_, request| terminal_pending_build_request_is_current(request, now));
        if requests.len() != previous_len {
            self.persist_pending_build_requests_locked(&requests);
        }
        requests
            .values()
            .map(|request| request.session_id.clone())
            .collect()
    }

    pub fn list_sessions(
        &self,
        base_dir: &Path,
        display_root: &Path,
        path: &PathBuf,
        error_line_limit: u32,
        error_keywords: &[String],
        auto_continue_time_patterns: &[String],
        auto_continue_interval_seconds: u32,
        auto_continue_respect_manual_interrupt: bool,
        auto_continue_backoff_factor: f64,
        auto_continue_backoff_max_minutes: u32,
    ) -> Vec<TerminalSessionInfo> {
        self.update_auto_continue_runtime_policy(
            auto_continue_interval_seconds,
            auto_continue_respect_manual_interrupt,
            auto_continue_backoff_factor,
            auto_continue_backoff_max_minutes,
        );
        let cleanup_dirty = self.cleanup_sessions(Some(path));
        let (session_ids, cleanup_dirty) = {
            let state = crate::lock_or_recover!(self.state.read());
            let mut session_ids = state
                .sessions_by_path
                .get(path)
                .cloned()
                .unwrap_or_default();
            sort_session_ids_by_recent_activity(&state, &mut session_ids);
            (session_ids, cleanup_dirty)
        };
        let (mut sessions, updated) = self.collect_session_infos_without_manager_lock(
            base_dir,
            display_root,
            session_ids,
            error_line_limit,
            error_keywords,
            auto_continue_time_patterns,
            auto_continue_respect_manual_interrupt,
        );
        self.backfill_zhipu_quota_reset_times_for_sessions(&mut sessions);
        let auto_continue_schedules =
            collect_terminal_auto_continue_schedules(&sessions, current_timestamp_millis());
        let dirty = cleanup_dirty || updated;
        let should_notify = dirty && should_notify_session_list_sync(cleanup_dirty, updated);
        if dirty {
            self.persist_state();
        }

        if should_notify {
            self.notify_session_list_changed("synced", "");
        }
        self.schedule_auto_continue_tasks(
            auto_continue_schedules,
            error_line_limit,
            error_keywords,
            auto_continue_time_patterns,
            auto_continue_interval_seconds,
            auto_continue_respect_manual_interrupt,
        );

        sessions
    }

    /// Return the stored session metadata needed by the workspace-history view.
    ///
    /// History only needs stable IDs, names, and paths to associate a
    /// conversation with a terminal. Avoid tmux probes, process-table scans,
    /// and cleanup here: those checks are useful for the live terminal view but
    /// can take longer than the HTTP task deadline when many sessions exist.
    pub fn list_sessions_lightweight(
        &self,
        base_dir: &Path,
        display_root: &Path,
        path: &PathBuf,
    ) -> Vec<TerminalSessionInfo> {
        let state = crate::lock_or_recover!(self.state.read());
        let mut session_ids = state
            .sessions_by_path
            .get(path)
            .cloned()
            .unwrap_or_default();
        sort_session_ids_by_recent_activity(&state, &mut session_ids);
        session_ids
            .into_iter()
            .filter_map(|session_id| {
                let stored = state.sessions_by_id.get(&session_id)?;
                let connected = state
                    .live_sessions
                    .get(&session_id)
                    .is_some_and(|session| session.is_alive());
                Some(stored.info(
                    base_dir,
                    display_root,
                    TerminalActivitySnapshot::idle(0),
                    connected,
                ))
            })
            .collect()
    }

    pub fn resolve_group_session_ids(&self, group: &str) -> Result<Vec<String>> {
        let group = group.trim();
        if group.is_empty() {
            anyhow::bail!("terminal group is empty");
        }
        let state = crate::lock_or_recover!(self.state.read());
        let mut ids: Vec<String> = state
            .sessions_by_id
            .values()
            .filter(|session| {
                session.group_token == group || session.group_name.eq_ignore_ascii_case(group)
            })
            .map(|session| session.id.clone())
            .collect();
        ids.sort();
        if ids.is_empty() {
            anyhow::bail!("未找到终端分组: {group}");
        }
        Ok(ids)
    }

    pub fn list_all_sessions(
        &self,
        base_dir: &Path,
        display_root: &Path,
        error_line_limit: u32,
        error_keywords: &[String],
        auto_continue_time_patterns: &[String],
        auto_continue_interval_seconds: u32,
        auto_continue_respect_manual_interrupt: bool,
        auto_continue_backoff_factor: f64,
        auto_continue_backoff_max_minutes: u32,
    ) -> Vec<TerminalSessionInfo> {
        self.update_auto_continue_runtime_policy(
            auto_continue_interval_seconds,
            auto_continue_respect_manual_interrupt,
            auto_continue_backoff_factor,
            auto_continue_backoff_max_minutes,
        );
        let cleanup_dirty = self.cleanup_sessions(None);
        let (session_ids, cleanup_dirty) = {
            let state = crate::lock_or_recover!(self.state.read());
            let mut session_ids: Vec<String> = state.sessions_by_id.keys().cloned().collect();
            sort_session_ids_by_recent_activity(&state, &mut session_ids);
            (session_ids, cleanup_dirty)
        };
        let (mut sessions, updated) = self.collect_session_infos_without_manager_lock(
            base_dir,
            display_root,
            session_ids,
            error_line_limit,
            error_keywords,
            auto_continue_time_patterns,
            auto_continue_respect_manual_interrupt,
        );
        self.backfill_zhipu_quota_reset_times_for_sessions(&mut sessions);
        let auto_continue_schedules =
            collect_terminal_auto_continue_schedules(&sessions, current_timestamp_millis());
        let dirty = cleanup_dirty || updated;
        let should_notify = dirty && should_notify_session_list_sync(cleanup_dirty, updated);
        if dirty {
            self.persist_state();
        }

        if should_notify {
            self.notify_session_list_changed("synced", "");
        }
        self.schedule_auto_continue_tasks(
            auto_continue_schedules,
            error_line_limit,
            error_keywords,
            auto_continue_time_patterns,
            auto_continue_interval_seconds,
            auto_continue_respect_manual_interrupt,
        );

        sessions
    }

    pub fn list_all_sessions_lightweight(
        &self,
        base_dir: &Path,
        display_root: &Path,
    ) -> Vec<TerminalSessionInfo> {
        let state = crate::lock_or_recover!(self.state.read());
        let mut session_ids: Vec<String> = state.sessions_by_id.keys().cloned().collect();
        sort_session_ids_by_recent_activity(&state, &mut session_ids);
        session_ids
            .into_iter()
            .filter_map(|session_id| {
                let stored = state.sessions_by_id.get(&session_id)?;
                let connected = state
                    .live_sessions
                    .get(&session_id)
                    .is_some_and(|session| session.is_alive());
                Some(stored.info(
                    base_dir,
                    display_root,
                    TerminalActivitySnapshot::idle(0),
                    connected,
                ))
            })
            .collect()
    }

    fn collect_session_infos_without_manager_lock(
        &self,
        base_dir: &Path,
        display_root: &Path,
        session_ids: Vec<String>,
        error_line_limit: u32,
        error_keywords: &[String],
        auto_continue_time_patterns: &[String],
        auto_continue_respect_manual_interrupt: bool,
    ) -> (Vec<TerminalSessionInfo>, bool) {
        let live_sessions = {
            let state = crate::lock_or_recover!(self.state.read());
            session_ids
                .iter()
                .map(|session_id| {
                    let live_session = state
                        .live_sessions
                        .get(session_id)
                        .filter(|session| session.is_alive())
                        .cloned();
                    (session_id.clone(), live_session)
                })
                .collect()
        };
        let mut probes = self.collect_session_activity_probes_cached(
            live_sessions,
            error_line_limit,
            error_keywords,
            auto_continue_time_patterns,
            auto_continue_respect_manual_interrupt,
        );
        let pending_build_session_ids = self.pending_build_session_ids();
        for probe in &mut probes {
            probe.pending_build = pending_build_session_ids.contains(&probe.session_id);
        }
        let mut state = crate::lock_or_recover!(self.state.write());
        collect_session_infos_from_probes_locked(&mut state, base_dir, display_root, probes)
    }

    fn collect_session_activity_probes_cached(
        &self,
        live_sessions: Vec<(String, Option<Arc<TerminalSession>>)>,
        error_line_limit: u32,
        error_keywords: &[String],
        auto_continue_time_patterns: &[String],
        auto_continue_respect_manual_interrupt: bool,
    ) -> Vec<TerminalActivityProbe> {
        let key = TerminalActivityProbeCacheKey {
            session_ids: live_sessions
                .iter()
                .map(|(session_id, _)| session_id.clone())
                .collect(),
            error_line_limit,
            error_keywords: error_keywords.to_vec(),
            auto_continue_time_patterns: auto_continue_time_patterns.to_vec(),
            auto_continue_respect_manual_interrupt,
        };
        if let Some(probes) = self.cached_activity_probes(&key) {
            return probes;
        }

        let _scan_guard = crate::lock_or_recover!(self.activity_probe_scan_lock.lock());
        if let Some(probes) = self.cached_activity_probes(&key) {
            return probes;
        }

        let probes = collect_session_activity_probes(
            live_sessions,
            error_line_limit,
            error_keywords,
            auto_continue_time_patterns,
            auto_continue_respect_manual_interrupt,
            self.probe_snapshot_cache.as_ref(),
        );
        let mut cache = crate::lock_or_recover!(self.activity_probe_cache.lock());
        cache.key = Some(key);
        cache.probes.clone_from(&probes);
        cache.completed_at = Some(Instant::now());
        probes
    }

    fn cached_activity_probes(
        &self,
        key: &TerminalActivityProbeCacheKey,
    ) -> Option<Vec<TerminalActivityProbe>> {
        let cache = crate::lock_or_recover!(self.activity_probe_cache.lock());
        (cache.key.as_ref() == Some(key)
            && cache.completed_at.is_some_and(|completed_at| {
                completed_at.elapsed() <= TERMINAL_ACTIVITY_PROBE_CACHE_TTL
            }))
        .then(|| cache.probes.clone())
    }

    pub fn search_active_session_output(
        &self,
        base_dir: &Path,
        display_root: &Path,
        needle: &str,
    ) -> Result<Vec<TerminalSessionSearchMatch>> {
        let needle = needle.trim();
        if needle.is_empty() {
            return Ok(Vec::new());
        }

        let candidates = {
            let state = crate::lock_or_recover!(self.state.read());
            let mut session_ids: Vec<String> = state
                .sessions_by_id
                .values()
                .filter(|session| !session.idle)
                .map(|session| session.id.clone())
                .collect();
            sort_session_ids_by_recent_activity(&state, &mut session_ids);

            session_ids
                .into_iter()
                .filter_map(|session_id| {
                    let stored = state.sessions_by_id.get(&session_id)?.clone();
                    let live = state.live_sessions.get(&session_id).cloned();
                    Some((stored, live))
                })
                .collect::<Vec<_>>()
        };

        let mut matches = Vec::new();
        for (stored, live_session) in candidates {
            let output = capture_tmux_text_pane_snapshot(&stored.id)
                .ok()
                .filter(|snapshot| !snapshot.is_empty())
                .or_else(|| {
                    live_session
                        .as_ref()
                        .map(|session| session.backlog_snapshot())
                        .filter(|snapshot| !snapshot.is_empty())
                });
            let Some(output) = output else {
                continue;
            };
            let text = String::from_utf8_lossy(&output);
            let Some((line_number, line, match_count)) = find_terminal_output_match(&text, needle)
            else {
                continue;
            };
            let relative = filesystem::relative_path(base_dir, &stored.path).unwrap_or_default();

            matches.push(TerminalSessionSearchMatch {
                session_id: stored.id.clone(),
                session_name: stored.name.clone(),
                title: stored.title(),
                path: relative_to_string(&relative),
                display_path: filesystem::display_path(base_dir, display_root, &stored.path),
                line_number,
                line,
                match_count,
            });
        }

        Ok(matches)
    }

    pub fn create_session(
        &self,
        base_dir: &Path,
        display_root: &Path,
        path: PathBuf,
        user_profile: runtime_paths::UserProfile,
        proxy_env: Vec<(String, String)>,
        terminal_startup_script: Option<String>,
        codex_api_preset_name: String,
        codex_api_base_url: String,
    ) -> Result<TerminalSessionInfo> {
        self.create_session_with_origin(
            base_dir,
            display_root,
            path,
            user_profile,
            proxy_env,
            terminal_startup_script,
            codex_api_preset_name,
            codex_api_base_url,
            TerminalSessionOrigin::Normal,
            String::new(),
        )
    }

    pub fn create_session_with_origin(
        &self,
        base_dir: &Path,
        display_root: &Path,
        path: PathBuf,
        user_profile: runtime_paths::UserProfile,
        proxy_env: Vec<(String, String)>,
        terminal_startup_script: Option<String>,
        codex_api_preset_name: String,
        codex_api_base_url: String,
        origin: TerminalSessionOrigin,
        owner_key: String,
    ) -> Result<TerminalSessionInfo> {
        cleanup_legacy_codex_launchers(&user_profile);
        let session = self.create_session_inner(
            path,
            user_profile,
            proxy_env,
            terminal_startup_script,
            codex_api_preset_name,
            codex_api_base_url,
            origin,
            owner_key,
        )?;
        self.persist_workspace_restores(&[session.id.clone()]);
        self.notify_session_list_changed("created", session.id.clone());
        Ok(session.info(base_dir, display_root, TerminalActivitySnapshot::idle(0), false))
    }

    pub fn rename_session(
        &self,
        base_dir: &Path,
        display_root: &Path,
        session_id: &str,
        next_name: String,
    ) -> Result<TerminalSessionInfo> {
        let name = normalize_session_name(&next_name)?;

        let session = {
            let mut state = crate::lock_or_recover!(self.state.write());

            let stored = state
                .sessions_by_id
                .get(session_id)
                .cloned()
                .with_context(|| format!("会话 `{session_id}` 不存在"))?;

            ensure_unique_session_name_locked(&state, &name, Some(session_id))?;

            let mut previous_names = stored.previous_names.clone();
            if stored.name != name && !previous_names.iter().any(|old| old == &stored.name) {
                previous_names.push(stored.name.clone());
            }
            let updated = StoredTerminalSession {
                name: name.clone(),
                previous_names,
                manually_renamed: true,
                ..stored
            };

            state
                .sessions_by_id
                .insert(session_id.to_string(), updated.clone());
            state
                .previous_terminal_names
                .insert(stored.name.clone(), session_id.to_string());

            if let Some(session) = state.live_sessions.get(session_id) {
                session.rename(name.clone());
            }

            drop(state);
            self.persist_state();
            updated
        };

        self.persist_workspace_restores(&[session.id.clone()]);
        self.notify_session_list_changed("renamed", session.id.clone());
        Ok(session.info(base_dir, display_root, TerminalActivitySnapshot::idle(0), false))
    }

    pub fn update_session_group(
        &self,
        session_id: &str,
        member_ids: &[String],
        group_roles: &[super::TerminalGroupRoleAssignment],
        fixed_order: bool,
        group_color: Option<String>,
        group_name: Option<String>,
    ) -> Result<()> {
        {
            let mut state = crate::lock_or_recover!(self.state.write());
            update_session_group_locked(
                &mut state,
                session_id,
                member_ids,
                group_roles,
                fixed_order,
                group_color,
                group_name,
            )?;
        }
        self.persist_state();
        {
            let state = crate::lock_or_recover!(self.state.read());
            let affected = terminal_group_change_affected_ids(&state, session_id, member_ids);
            drop(state);
            self.persist_workspace_restores(&affected);
        }
        self.notify_session_list_changed("group_updated", session_id.to_string());
        Ok(())
    }

    pub fn update_session_origin(
        &self,
        base_dir: &Path,
        display_root: &Path,
        session_id: &str,
        origin: TerminalSessionOrigin,
        owner_key: String,
    ) -> Result<TerminalSessionInfo> {
        self.cleanup_sessions(None);
        let session = {
            let mut state = crate::lock_or_recover!(self.state.write());

            let stored = state
                .sessions_by_id
                .get(session_id)
                .cloned()
                .with_context(|| format!("会话 `{session_id}` 不存在"))?;
            let owner_key = if origin == TerminalSessionOrigin::Normal {
                String::new()
            } else {
                owner_key
            };
            let updated = StoredTerminalSession {
                origin,
                owner_key,
                ..stored
            };
            state
                .sessions_by_id
                .insert(session_id.to_string(), updated.clone());
            drop(state);
            self.persist_state();
            updated
        };

        self.notify_session_list_changed("origin_updated", session.id.clone());
        Ok(session.info(base_dir, display_root, TerminalActivitySnapshot::idle(0), false))
    }

    pub fn set_session_idle(
        &self,
        base_dir: &Path,
        display_root: &Path,
        session_id: &str,
        idle: bool,
    ) -> Result<TerminalSessionInfo> {
        let (session, group_member_ids) = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let group_token = state
                .sessions_by_id
                .get_mut(session_id)
                .with_context(|| format!("会话 `{session_id}` 不存在"))?
                .group_token
                .clone();
            let group_member_ids = if idle && !group_token.is_empty() {
                state
                    .sessions_by_id
                    .values()
                    .filter(|session| !session.idle && session.group_token == group_token)
                    .map(|session| session.id.clone())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            if idle && !group_member_ids.is_empty() {
                for member_id in &group_member_ids {
                    if let Some(member) = state.sessions_by_id.get_mut(member_id) {
                        member.idle = true;
                        member.hidden = false;
                    }
                }
            } else if let Some(stored) = state.sessions_by_id.get_mut(session_id) {
                stored.idle = idle;
                stored.hidden = false;
            }
            let session = state
                .sessions_by_id
                .get(session_id)
                .cloned()
                .with_context(|| format!("会话 `{session_id}` 不存在"))?;
            drop(state);
            self.persist_state();
            (session, group_member_ids)
        };

        if group_member_ids.is_empty() {
            self.notify_session_list_changed(
                if idle { "idle" } else { "restored" },
                session.id.clone(),
            );
        } else {
            self.notify_session_list_changed("idle", session.id.clone());
        }
        Ok(session.info(base_dir, display_root, TerminalActivitySnapshot::idle(0), false))
    }

    /// Hide one terminal into the hidden zone without grouped idle semantics.
    /// Ending a window isolates it from active and idle terminal selectors.
    pub fn set_session_hidden(
        &self,
        base_dir: &Path,
        display_root: &Path,
        session_id: &str,
        hidden: bool,
    ) -> Result<TerminalSessionInfo> {
        let session = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let stored = state
                .sessions_by_id
                .get_mut(session_id)
                .with_context(|| format!("会话 `{session_id}` 不存在"))?;
            stored.hidden = hidden;
            if hidden {
                stored.idle = false;
            }
            let session = stored.clone();
            drop(state);
            self.persist_state();
            session
        };

        self.notify_session_list_changed(
            if hidden { "hidden" } else { "restored" },
            session.id.clone(),
        );
        Ok(session.info(base_dir, display_root, TerminalActivitySnapshot::idle(0), false))
    }

    pub fn delete_session(
        &self,
        base_dir: &Path,
        display_root: &Path,
        session_id: &str,
    ) -> Result<TerminalSessionInfo> {
        self.cleanup_sessions(None);
        // Serialize only this session's lifecycle. Process-tree capture, tmux
        // commands and exit waits must never run under the global state lock:
        // one pane blocked in kernel IO would otherwise stall every terminal.
        let observed = {
            let state = crate::lock_or_recover!(self.state.read());
            state
                .sessions_by_id
                .get(session_id)
                .cloned()
                .with_context(|| format!("会话 `{session_id}` 不存在"))?
        };
        let session_start_lock = self.session_start_lock(session_id);
        let _session_start_guard = crate::lock_or_recover!(session_start_lock.lock());
        let stored = {
            let state = crate::lock_or_recover!(self.state.read());
            let stored = state
                .sessions_by_id
                .get(session_id)
                .cloned()
                .with_context(|| format!("会话 `{session_id}` 不存在"))?;
            anyhow::ensure!(
                stored.created_at == observed.created_at,
                "会话 `{session_id}` 已被替换，请刷新后重试"
            );
            stored
        };
        self.archive_deleted_agent_session_name(base_dir, &stored);

        let fallback_user_profile = self.fallback_user_profile();
        let session_user_profile = stored_session_user_profile(&stored, &fallback_user_profile);
        terminate_tmux_pane_process_tree(&stored.id, &session_user_profile)?;

        let (session, live_session) = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let session = state
                .sessions_by_id
                .get(session_id)
                .cloned()
                .with_context(|| format!("会话 `{session_id}` 不存在"))?;
            let live_session = state.live_sessions.remove(session_id);
            if let Some(session) = &live_session {
                session.mark_closed(b"\r\n[webclx] session terminated.\r\n");
            }

            state.sessions_by_id.remove(session_id);
            state.input_histories.remove(session_id);
            remove_session_from_path_locked(&mut state, &stored.path, session_id);
            drop(state);
            self.persist_state();
            (session, live_session)
        };

        if let Some(live_session) = live_session {
            live_session.terminate_child();
        }

        {
            let _write_guard = crate::lock_or_recover!(self.workspace_restore_write_lock.lock());
            let result = self
                .load_workspace_restore_registry()
                .and_then(|mut registry| {
                    let previous_len = registry.records.len();
                    registry
                        .records
                        .retain(|record| record.session_id != session.id);
                    if registry.records.len() != previous_len {
                        self.persist_workspace_restore_registry(&registry)?;
                    }
                    Ok(())
                });
            if let Err(error) = result {
                warn!(
                    path = %self.workspace_restore_file.display(),
                    "remove deleted terminal workspace restore failed: {error}"
                );
            }
        }

        drop(_session_start_guard);
        self.purge_session_runtime_state(session_id);
        self.purge_session_persisted_tasks(session_id);
        self.release_session_start_lock_if_idle(session_id, &session_start_lock);
        self.notify_session_list_changed("deleted", session.id.clone());
        Ok(session.info(base_dir, display_root, TerminalActivitySnapshot::idle(0), false))
    }

    /// 会话被永久移除后，释放其在各内存运行时缓存中的残留项。
    /// 持久化任务注册表（auto-continue、scheduled input、pending build）
    /// 由 `purge_session_persisted_tasks` 另行清理。
    pub(in crate::terminal) fn purge_session_runtime_state(&self, session_id: &str) {
        if session_id.is_empty() {
            return;
        }
        {
            let mut runtime = crate::lock_or_recover!(self.per_session_runtime.lock());
            if let Some(entry) = runtime.get_mut(session_id) {
                entry.error_auto_continue = None;
                entry.auto_compact_signature = None;
                entry.auto_continue_last_sent_at = None;
                entry.canceled_signatures.clear();
                // 启动/发送锁仍被外部持有时保留条目（语义同旧实现：锁条目
                // 由 release_session_start_lock_if_idle 按引用计数移除），
                // 避免持有旧锁克隆的等待者被换到另一把锁上；无外部持有则
                // 连同条目一起移除。
                if entry.prunable() {
                    runtime.remove(session_id);
                }
            }
        }
        {
            let mut cache = crate::lock_or_recover!(self.activity_probe_cache.lock());
            if cache
                .key
                .as_ref()
                .is_some_and(|key| key.session_ids.iter().any(|id| id == session_id))
            {
                *cache = TerminalActivityProbeCache::default();
            }
        }
        crate::lock_or_recover!(self.probe_snapshot_cache.lock()).remove(session_id);
        crate::lock_or_recover!(self.terminal_message_ledger.lock()).remove_session(session_id);
    }

    /// 会话被永久移除后，取消其持久化任务注册表中的残留项：
    /// auto-continue 计划（连同该会话的 crontab 条目行与 due 注释）、
    /// scheduled input 任务、pending build 请求。任何一步失败只告警，
    /// 不阻断删除流程。
    fn purge_session_persisted_tasks(&self, session_id: &str) {
        self.purge_session_persisted_tasks_with(session_id, rewrite_crontab_without_markers);
    }

    /// `rewrite_crontab` 可注入替身，测试据此核对将清除的 marker
    /// 而不触碰真实 crontab。
    fn purge_session_persisted_tasks_with(
        &self,
        session_id: &str,
        rewrite_crontab: impl Fn(&[String]) -> Result<()>,
    ) {
        if session_id.is_empty() {
            return;
        }
        // 各持久化注册表的告警统一带上被删会话 id。
        let _span = tracing::info_span!("purge_session_persisted_tasks", %session_id).entered();

        let removed_auto_continue = {
            let mut tasks = crate::lock_or_recover!(self.auto_continue_schedules.lock());
            let removed = tasks.remove(session_id).is_some();
            if removed {
                self.persist_auto_continue_tasks_locked(&tasks);
            }
            removed
        };
        if removed_auto_continue {
            // 两个前缀分别覆盖该会话的全部 cron 条目行与 due 注释行；
            // rewrite_crontab_without_markers 按 contains/starts_with 过滤。
            let markers = [
                terminal_auto_continue_cron_marker_prefix(session_id),
                terminal_auto_continue_due_marker_prefix(session_id),
            ];
            if let Err(error) = rewrite_crontab(&markers) {
                warn!(
                    "remove deleted terminal auto-continue crontab entries for {session_id} failed: {error}"
                );
            }
            self.auto_continue_notify.notify_one();
        }

        let removed_scheduled_input = {
            let mut tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());
            let previous_len = tasks.len();
            tasks.retain(|_, task| task.session_id != session_id);
            let removed = tasks.len() != previous_len;
            if removed {
                self.persist_scheduled_input_tasks_locked(&tasks);
            }
            removed
        };
        if removed_scheduled_input {
            self.scheduled_input_notify.notify_one();
        }

        {
            let mut requests = crate::lock_or_recover!(self.pending_build_requests.lock());
            let previous_len = requests.len();
            requests.retain(|_, request| request.session_id != session_id);
            if requests.len() != previous_len {
                self.persist_pending_build_requests_locked(&requests);
            }
        }
    }

    /// 会话启动锁仅当无人再持有克隆时才移除条目：map 与调用方各持一份
    /// 时 strong_count == 2；并发 start/delete 已取走克隆则保留该条目，
    /// 避免持有旧锁的等待者被换到另一把锁上。整个判断在 map 锁内完成。
    /// 条目其余字段非空或发送锁仍有外部持有时同样保留——释放只针对
    /// 已完全空闲的条目。
    fn release_session_start_lock_if_idle(&self, session_id: &str, start_lock: &Arc<Mutex<()>>) {
        let mut runtime = crate::lock_or_recover!(self.per_session_runtime.lock());
        let Some(entry) = runtime.get(session_id) else {
            return;
        };
        let idle = Arc::ptr_eq(&entry.start_lock, start_lock)
            && Arc::strong_count(start_lock) <= 2
            && entry.runtime_fields_cleared()
            && Arc::strong_count(&entry.send_lock) == 1;
        if idle {
            runtime.remove(session_id);
        }
    }

    pub fn list_resume_archives(&self) -> Result<Vec<CodexResumeArchive>> {
        let mut registry = load_resume_archive_registry(&self.archive_file)?;
        sort_resume_archives(&mut registry.archives);
        Ok(registry.archives)
    }

    pub fn save_resume_archive(
        &self,
        payload: SaveCodexResumeArchiveRequest,
    ) -> Result<CodexResumeArchive> {
        let resume_id = normalize_resume_id(&payload.resume_id)?;
        let note = normalize_resume_archive_note(payload.note.as_deref(), &resume_id);
        let source = normalize_resume_archive_source(payload.source.as_deref());
        let cwd = normalize_resume_archive_cwd(payload.cwd.as_deref());
        let terminal_name =
            terminal_core::normalize_resume_archive_terminal_name(payload.terminal_name.as_deref());
        let now = current_timestamp_millis();
        let mut registry = load_resume_archive_registry(&self.archive_file)?;
        let command = normalize_resume_command(
            payload.command.as_deref().or(Some(&payload.resume_id)),
            &resume_id,
        );

        let archive = if let Some(existing) = registry
            .archives
            .iter_mut()
            .find(|archive| archive.resume_id == resume_id)
        {
            existing.note = note;
            existing.source = source;
            existing.command = command;
            existing.cwd = cwd;
            if !terminal_name.is_empty() {
                existing.terminal_name = terminal_name;
            }
            existing.updated_at = now;
            existing.clone()
        } else {
            let archive = CodexResumeArchive {
                id: resume_id.clone(),
                resume_id,
                command,
                cwd,
                terminal_name,
                note,
                source,
                created_at: now,
                updated_at: now,
                last_used_at: 0,
            };
            registry.archives.push(archive.clone());
            archive
        };

        sort_resume_archives(&mut registry.archives);
        persist_resume_archive_registry(&self.archive_file, &registry)?;
        Ok(archive)
    }

    fn archive_deleted_agent_session_name(&self, base_dir: &Path, stored: &StoredTerminalSession) {
        if cfg!(windows) {
            return;
        }

        let Ok(Some(detected)) = detect_current_resume_session(&stored.id) else {
            return;
        };
        let Ok(resume_id) = normalize_resume_id(&detected.info.resume_id) else {
            return;
        };

        let terminal_name = normalize_resume_archive_terminal_name(Some(&stored.name));
        if terminal_name.is_empty() {
            return;
        }

        let cwd = filesystem::relative_path(base_dir, &stored.path)
            .map(|relative| relative_to_string(&relative))
            .unwrap_or_default();
        let now = current_timestamp_millis();
        let command = detected.info.command;
        let mut registry = match load_resume_archive_registry(&self.archive_file) {
            Ok(registry) => registry,
            Err(error) => {
                warn!(
                    "failed to load Codex resume archive before deleting terminal {}: {error}",
                    stored.id
                );
                return;
            }
        };

        if let Some(existing) = registry
            .archives
            .iter_mut()
            .find(|archive| archive.resume_id == resume_id)
        {
            existing.terminal_name = terminal_name;
            if existing.cwd.trim().is_empty() {
                existing.cwd = normalize_resume_archive_cwd(Some(&cwd));
            }
            existing.source = detected.source.to_string();
            existing.command = command;
            existing.updated_at = now.max(existing.updated_at);
        } else {
            registry.archives.push(CodexResumeArchive {
                id: resume_id.clone(),
                resume_id: resume_id.clone(),
                command,
                cwd: normalize_resume_archive_cwd(Some(&cwd)),
                terminal_name,
                note: normalize_resume_archive_note(None, &resume_id),
                source: detected.source.to_string(),
                created_at: now,
                updated_at: now,
                last_used_at: 0,
            });
        }

        sort_resume_archives(&mut registry.archives);
        if let Err(error) = persist_resume_archive_registry(&self.archive_file, &registry) {
            warn!(
                "failed to persist Codex resume archive before deleting terminal {}: {error}",
                stored.id
            );
        }
    }

    pub fn touch_resume_archive(&self, archive_id: &str) -> Result<CodexResumeArchive> {
        let normalized_id = normalize_resume_id(archive_id)?;
        let now = current_timestamp_millis();
        let mut registry = load_resume_archive_registry(&self.archive_file)?;

        let archive = registry
            .archives
            .iter_mut()
            .find(|archive| archive.id == normalized_id || archive.resume_id == normalized_id)
            .with_context(|| format!("Codex 归档 `{normalized_id}` 不存在"))?;
        archive.last_used_at = now;
        archive.updated_at = archive.updated_at.max(archive.created_at);
        let archive = archive.clone();

        sort_resume_archives(&mut registry.archives);
        persist_resume_archive_registry(&self.archive_file, &registry)?;
        Ok(archive)
    }

    pub fn delete_resume_archive(&self, archive_id: &str) -> Result<CodexResumeArchive> {
        let normalized_id = normalize_resume_id(archive_id)?;
        let mut registry = load_resume_archive_registry(&self.archive_file)?;

        let Some(index) = registry
            .archives
            .iter()
            .position(|archive| archive.id == normalized_id || archive.resume_id == normalized_id)
        else {
            anyhow::bail!("Codex 归档 `{normalized_id}` 不存在");
        };

        let removed = registry.archives.remove(index);
        persist_resume_archive_registry(&self.archive_file, &registry)?;
        Ok(removed)
    }

    pub(in crate::terminal) fn current_resume_session(
        &self,
        session_id: &str,
    ) -> Result<Option<DetectedResumeSession>> {
        self.current_resume_session_with(session_id, detect_current_resume_session)
    }

    pub(in crate::terminal) fn current_resume_session_complete(
        &self,
        session_id: &str,
    ) -> Result<Option<DetectedResumeSession>> {
        self.current_resume_session_with(session_id, detect_current_resume_session_complete)
    }

    pub(in crate::terminal) fn force_interrupt_and_resume(
        &self,
        session_id: &str,
    ) -> Result<super::TerminalInterruptResumeResponse> {
        if cfg!(windows) {
            anyhow::bail!("Windows 终端暂不支持强制中断并恢复");
        }

        let detected = self
            .current_resume_session_complete(session_id)?
            .with_context(|| "无法识别当前 Codex/Claude 会话，未执行中断")?;
        let resume_command = prepare_codex_history_command_for_user(
            &self.session_user_name(session_id)?,
            &detected.info.command,
        )?;
        let process_ids = current_resume_agent_process_ids(session_id, &detected.info.resume_id)?;
        if process_ids.is_empty() {
            anyhow::bail!("未找到当前会话对应的智能体进程，未执行中断");
        }

        #[cfg(not(windows))]
        for process_id in &process_ids {
            // SAFETY: libc::kill does not dereference pointers. PIDs come only from
            // descendants of this session's tmux pane and must hold this rollout.
            let result = unsafe { libc::kill(*process_id as libc::pid_t, libc::SIGINT) };
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error).context("中断智能体进程失败");
                }
            }
        }

        let wait_started = Instant::now();
        while process_ids
            .iter()
            .any(|process_id| Path::new(&format!("/proc/{process_id}")).exists())
        {
            if wait_started.elapsed() >= TERMINAL_FORCE_RESUME_EXIT_TIMEOUT {
                anyhow::bail!("智能体进程未在 4 秒内退出，已停止恢复以避免重复会话");
            }
            thread::sleep(TERMINAL_FORCE_RESUME_EXIT_POLL);
        }

        thread::sleep(TERMINAL_FORCE_RESUME_SHELL_SETTLE);
        self.send_session_input_silent(session_id, resume_command.clone())?;
        thread::sleep(Duration::from_millis(TERMINAL_COMMAND_ENTER_DELAY_MS));
        self.send_session_input_silent(session_id, "\r".to_string())?;

        Ok(super::TerminalInterruptResumeResponse {
            ok: true,
            outcome: "resumed".to_string(),
            resume_id: detected.info.resume_id,
            program: detected.info.program,
            command: resume_command,
            interrupted_processes: process_ids.len(),
        })
    }

    fn current_resume_session_with(
        &self,
        session_id: &str,
        detector: fn(&str) -> Result<Option<DetectedResumeSession>>,
    ) -> Result<Option<DetectedResumeSession>> {
        if cfg!(windows) {
            let state = crate::lock_or_recover!(self.state.read());
            state
                .sessions_by_id
                .contains_key(session_id)
                .then_some(())
                .with_context(|| format!("会话 `{session_id}` 不存在"))?;
            return Ok(None);
        }

        let stored_id = {
            let state = crate::lock_or_recover!(self.state.read());
            state
                .sessions_by_id
                .get(session_id)
                .map(|session| session.id.clone())
                .with_context(|| format!("会话 `{session_id}` 不存在"))?
        };

        detector(&stored_id)
    }

    pub(in crate::terminal) fn resume_session_is_active(&self, resume_id: &str) -> Result<bool> {
        if cfg!(windows) {
            return Ok(false);
        }

        let session_ids = {
            let state = crate::lock_or_recover!(self.state.read());
            state.sessions_by_id.keys().cloned().collect::<Vec<_>>()
        };
        for session_id in session_ids {
            if detect_current_resume_session(&session_id)?
                .is_some_and(|detected| detected.info.resume_id == resume_id)
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub(super) fn get_for_connection(
        &self,
        path: PathBuf,
        session_id: Option<&str>,
        user_profile: runtime_paths::UserProfile,
        proxy_env: Vec<(String, String)>,
        terminal_startup_script: Option<String>,
        codex_api_preset_name: String,
        codex_api_base_url: String,
        output_visible: bool,
    ) -> Result<Arc<TerminalSession>> {
        cleanup_legacy_codex_launchers(&user_profile);
        let session = match session_id {
            Some(session_id) if !session_id.trim().is_empty() => self.get_session(
                &path,
                session_id,
                user_profile,
                proxy_env,
                terminal_startup_script,
                output_visible,
            ),
            _ => self.get_or_create_latest(
                path,
                user_profile,
                proxy_env,
                terminal_startup_script,
                codex_api_preset_name,
                codex_api_base_url,
                output_visible,
            ),
        }?;
        if output_visible {
            self.mark_session_output_viewed(&session.id);
        }
        Ok(session)
    }

    fn get_session(
        &self,
        path: &PathBuf,
        session_id: &str,
        user_profile: runtime_paths::UserProfile,
        proxy_env: Vec<(String, String)>,
        terminal_startup_script: Option<String>,
        output_visible: bool,
    ) -> Result<Arc<TerminalSession>> {
        let mut changed = false;
        let mut change_action = "opened";
        let changed_session_id = session_id.to_string();
        let session_start_lock = self.session_start_lock(session_id);
        let _session_start_guard = crate::lock_or_recover!(session_start_lock.lock());
        let stored = {
            let state = crate::lock_or_recover!(self.state.read());
            let stored = state
                .sessions_by_id
                .get(session_id)
                .cloned()
                .with_context(|| format!("终端会话 `{session_id}` 不存在"))?;
            if stored.path != *path {
                anyhow::bail!("会话不属于当前目录。");
            }
            stored
        };

        let existing_session = crate::lock_or_recover!(self.state.read())
            .live_sessions
            .get(session_id)
            .filter(|session| session.is_alive())
            .cloned();
        let session = if let Some(session) = existing_session {
            session
        } else {
            let session_user_profile = stored_session_user_profile(&stored, &user_profile);
            ensure_backend_session(&stored.id, &stored.path, &session_user_profile)?;
            let initial_snapshot = TerminalSession::capture_initial_snapshot(&stored.id);
            let candidate = TerminalSession::attach_with_initial_snapshot(
                &stored,
                proxy_env,
                terminal_startup_script,
                initial_snapshot,
            )?;
            let mut state = crate::lock_or_recover!(self.state.write());
            if !state.sessions_by_id.contains_key(session_id) {
                anyhow::bail!("终端会话 `{session_id}` 不存在");
            }
            state
                .live_sessions
                .insert(session_id.to_string(), candidate.clone());
            candidate
        };

        {
            let mut state = crate::lock_or_recover!(self.state.write());
            let mut dirty = false;
            let opened = output_visible && mark_session_opened_locked(&mut state, &stored.id);
            dirty |= opened;
            if change_action == "opened" && !opened {
                change_action = "synced";
            }
            if dirty {
                drop(state);
                self.persist_state();
                changed = true;
            }
        }
        if changed {
            self.notify_session_list_changed(change_action, changed_session_id);
        }

        Ok(session)
    }

    fn get_or_create_latest(
        &self,
        path: PathBuf,
        user_profile: runtime_paths::UserProfile,
        proxy_env: Vec<(String, String)>,
        terminal_startup_script: Option<String>,
        codex_api_preset_name: String,
        codex_api_base_url: String,
        output_visible: bool,
    ) -> Result<Arc<TerminalSession>> {
        let mut changed = false;
        let mut change_action = "opened";
        let mut changed_session_id = String::new();
        let cleanup_dirty = self.cleanup_sessions(Some(&path));
        let session = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let mut dirty = cleanup_dirty;
            let mut session_ids = state
                .sessions_by_path
                .get(&path)
                .cloned()
                .unwrap_or_default();
            sort_session_ids_by_recent_activity(&state, &mut session_ids);

            let (stored, created) = match session_ids
                .iter()
                .filter_map(|session_id| state.sessions_by_id.get(session_id))
                .find(|session| session.user_name == user_profile.name)
                .cloned()
            {
                Some(stored) => (stored, false),
                None => {
                    let created = create_session_locked(
                        &mut state,
                        &self.next_id,
                        path,
                        user_profile.clone(),
                        terminal_startup_script.clone(),
                        codex_api_preset_name.clone(),
                        codex_api_base_url.clone(),
                        TerminalSessionOrigin::Normal,
                        String::new(),
                    )?;
                    dirty = true;
                    (created, true)
                }
            };

            let session_user_profile = stored_session_user_profile(&stored, &user_profile);
            let session = ensure_live_session_locked(
                &mut state,
                &stored,
                session_user_profile,
                proxy_env,
                terminal_startup_script,
            )?;
            let opened = output_visible && mark_session_opened_locked(&mut state, &stored.id);
            dirty |= opened;
            if dirty {
                drop(state);
                self.persist_state();
                changed = true;
                change_action = if created {
                    "created"
                } else if opened {
                    "opened"
                } else {
                    "synced"
                };
                changed_session_id = stored.id.clone();
            }
            session
        };
        if changed {
            self.notify_session_list_changed(change_action, changed_session_id);
        }

        Ok(session)
    }

    fn create_session_inner(
        &self,
        path: PathBuf,
        user_profile: runtime_paths::UserProfile,
        proxy_env: Vec<(String, String)>,
        terminal_startup_script: Option<String>,
        codex_api_preset_name: String,
        codex_api_base_url: String,
        origin: TerminalSessionOrigin,
        owner_key: String,
    ) -> Result<StoredTerminalSession> {
        // Reserve the id/name while holding the registry lock, then perform
        // tmux and shell startup outside it. A slow backend launch must not
        // block rename/delete/list operations for every other session.
        self.cleanup_sessions(Some(&path));
        let start_lock;
        let _start_guard;
        let session = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let session = reserve_session_locked(
                &mut state,
                &self.next_id,
                path.clone(),
                &user_profile,
                codex_api_preset_name,
                codex_api_base_url,
                origin,
                owner_key,
            )?;
            // Publish the reservation only after protecting its backend startup.
            // 启动门被占时先释放 state 写锁再阻塞等待，避免 `state → start_lock`
            // 与 `start_lock → state` 的 ABBA 死锁；拿到门后复核预约仍有效。
            start_lock = self.session_start_lock(&session.id);
            _start_guard = self.lock_reserved_session_start(&start_lock, state, &session)?;
            self.persist_state();
            session
        };

        let startup_result = (|| {
            create_fresh_backend_session(&session.id, &session.path, &user_profile)?;
            if let Some(script) = terminal_startup_script
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            {
                send_backend_startup_script(&session.id, script)?;
            }
            TerminalSession::attach(&session, proxy_env, None)
        })();
        let live_session = match startup_result {
            Ok(live_session) => live_session,
            Err(error) => {
                let removed = {
                    let mut state = crate::lock_or_recover!(self.state.write());
                    let removed = if state
                        .sessions_by_id
                        .get(&session.id)
                        .is_some_and(|stored| stored.created_at == session.created_at)
                    {
                        state.sessions_by_id.remove(&session.id);
                        remove_session_from_path_locked(&mut state, &session.path, &session.id);
                        true
                    } else {
                        // 记录已被替换：条目归属新会话，不能清理其运行时状态。
                        false
                    };
                    drop(state);
                    self.persist_state();
                    removed
                };
                if removed {
                    drop(_start_guard);
                    self.purge_session_runtime_state(&session.id);
                    self.purge_session_persisted_tasks(&session.id);
                    self.release_session_start_lock_if_idle(&session.id, &start_lock);
                }
                if let Err(cleanup) = kill_backend_session(&session.id) {
                    return Err(error.context(format!("终端启动失败且回收失败: {cleanup:#}")));
                }
                return Err(error);
            }
        };
        let mut state = crate::lock_or_recover!(self.state.write());
        if state
            .sessions_by_id
            .get(&session.id)
            .is_none_or(|stored| stored.created_at != session.created_at)
        {
            drop(state);
            live_session.mark_closed(b"\r\n[webclx] session creation cancelled.\r\n");
            let cleanup = kill_backend_session(&session.id);
            live_session.terminate_child();
            cleanup.context("终端会话创建已取消，但回收失败")?;
            anyhow::bail!("终端会话创建已取消。");
        }
        state.live_sessions.insert(session.id.clone(), live_session);
        drop(state);
        Ok(session)
    }

    fn persist_state(&self) {
        // Serialize writers before taking the state snapshot. Never hold the
        // manager lock while waiting for disk or another writer; always snapshot
        // after acquiring the writer lock so older writes cannot replace newer ones.
        let _writer = crate::lock_or_recover!(self.registry_write_lock.lock());
        let registry = {
            let state = crate::lock_or_recover!(self.state.read());
            StoredTerminalRegistry {
                next_ordinal: self.next_id.load(Ordering::SeqCst).max(1),
                sessions: collect_stored_sessions(&state),
                input_histories: collect_stored_input_histories(&state),
                output_observations: collect_stored_output_observations(&state),
            }
        };
        if let Err(error) = persist_terminal_registry(&self.state_file, &registry) {
            warn!(path = %self.state_file.display(), "persist terminal session registry failed: {error}");
        }
    }

    fn persist_pending_build_requests_locked(
        &self,
        requests: &HashMap<String, TerminalPendingBuildRequest>,
    ) {
        let mut requests = requests.values().cloned().collect::<Vec<_>>();
        requests.sort_by(|left, right| left.request_id.cmp(&right.request_id));
        let registry = StoredTerminalPendingBuildRegistry { requests };
        if let Err(error) =
            persist_terminal_pending_build_registry(&self.pending_build_file, &registry)
        {
            warn!(
                "persist terminal pending build registry failed {}: {error}",
                self.pending_build_file.display()
            );
        }
    }

    fn restore_live_sessions(&self) {
        let default_user_name = default_terminal_user_name();
        let fallback_user_profile =
            runtime_paths::resolve_current_user_profile().unwrap_or_else(|| {
                runtime_paths::resolve_user_profile(&default_user_name)
                    .expect("default terminal user should resolve")
            });
        let stored_sessions = {
            let state = crate::lock_or_recover!(self.state.read());
            state.sessions_by_id.values().cloned().collect::<Vec<_>>()
        };

        let mut output_observation_dirty = false;
        for stored in stored_sessions {
            let started_at = Instant::now();
            let session_start_lock = self.session_start_lock(&stored.id);
            let _session_start_guard = crate::lock_or_recover!(session_start_lock.lock());
            let user_profile = stored_session_user_profile(&stored, &fallback_user_profile);
            cleanup_legacy_codex_launchers(&user_profile);
            if crate::lock_or_recover!(self.state.read())
                .live_sessions
                .get(&stored.id)
                .is_some_and(|session| session.is_alive())
            {
                continue;
            }

            if let Err(error) = ensure_backend_session(&stored.id, &stored.path, &user_profile) {
                warn!("restore terminal session {} failed: {error}", stored.id);
                continue;
            }
            let backend_ready_at = Instant::now();
            let initial_snapshot = TerminalSession::capture_initial_snapshot(&stored.id);
            let initial_snapshot_ready_at = Instant::now();

            let candidate = match TerminalSession::attach_with_initial_snapshot(
                &stored,
                Vec::new(),
                None,
                initial_snapshot,
            ) {
                Ok(session) => session,
                Err(error) => {
                    warn!("attach restored terminal session {} failed: {error}", stored.id);
                    continue;
                }
            };
            let attached_at = Instant::now();
            {
                let mut state = crate::lock_or_recover!(self.state.write());
                if !state.sessions_by_id.contains_key(&stored.id) {
                    drop(state);
                    candidate.terminate_child();
                    continue;
                }
                state.live_sessions.insert(stored.id.clone(), candidate);
            }

            let snapshot_probe_sequence =
                TERMINAL_ACTIVITY_PROBE_SEQUENCE.fetch_add(1, Ordering::SeqCst);
            let fingerprint = capture_tmux_recent_pane_snapshot(&stored.id)
                .ok()
                .map(|snapshot| terminal_output_fingerprint(&snapshot));
            let snapshot_at = Instant::now();
            {
                let mut state = crate::lock_or_recover!(self.state.write());
                output_observation_dirty |= prepare_restored_output_observation_locked(
                    &mut state,
                    &stored.id,
                    fingerprint,
                    snapshot_probe_sequence,
                );
            }
            let elapsed = started_at.elapsed();
            if elapsed >= Duration::from_millis(250) {
                warn!(
                    session_id = stored.id,
                    backend_ms = backend_ready_at.duration_since(started_at).as_millis(),
                    initial_snapshot_ms = initial_snapshot_ready_at
                        .duration_since(backend_ready_at)
                        .as_millis(),
                    attach_ms = attached_at
                        .duration_since(initial_snapshot_ready_at)
                        .as_millis(),
                    snapshot_ms = snapshot_at.duration_since(attached_at).as_millis(),
                    total_ms = elapsed.as_millis(),
                    outcome = "ok",
                    "slow terminal initial session restore"
                );
            }
        }

        if output_observation_dirty {
            self.persist_state();
        }
    }

    fn restore_shutdown_sessions(
        &self,
        records: &[TerminalResumeRestoreRecord],
    ) -> RestoreShutdownResult {
        let mut result = RestoreShutdownResult::default();
        if records.is_empty() {
            return result;
        }

        let snapshot = self.env_snapshot.as_ref().clone();
        let mut remaining_records = records.to_vec();
        let mut dirty = false;
        let mut state = crate::lock_or_recover!(self.state.write());

        for record in records {
            if state
                .live_sessions
                .get(&record.session_id)
                .is_some_and(|session| session.is_alive())
            {
                result
                    .skipped
                    .push((record.session_id.clone(), "session_already_running".to_string()));
                continue;
            }

            let stored = StoredTerminalSession {
                id: record.session_id.clone(),
                path: record.path.clone(),
                user_name: record.user_name.clone(),
                name: record.name.clone(),
                previous_names: Vec::new(),
                title: record.title.clone(),
                codex_api_preset_name: record.codex_api_preset_name.clone(),
                codex_api_base_url: record.codex_api_base_url.clone(),
                origin: record.origin,
                owner_key: record.owner_key.clone(),
                manually_renamed: record.manually_renamed,
                idle: record.idle,
                hidden: false,
                created_at: record.created_at,
                last_opened_at: record.last_opened_at,
                group_token: record.group_token.clone(),
                group_color: record.group_color.clone(),
                group_name: record.group_name.clone(),
                group_role: record.group_role.clone(),
                group_order: record.group_order,
                group_fixed_order: record.group_fixed_order,
            };

            let session_user_profile = runtime_paths::resolve_user_profile(&stored.user_name)
                .unwrap_or_else(|_| {
                    warn!(
                        "resolve shutdown restore user {} for {} failed; fallback to {}",
                        stored.user_name, stored.id, snapshot.user_profile.name
                    );
                    snapshot.user_profile.clone()
                });
            if let Err(error) = ensure_live_session_locked(
                &mut state,
                &stored,
                session_user_profile,
                snapshot.proxy_env.clone(),
                None,
            ) {
                warn!("restore shutdown session {} failed: {error}", stored.id);
                result.failed.push((stored.id, error.to_string()));
                continue;
            }

            let snapshot_probe_sequence =
                TERMINAL_ACTIVITY_PROBE_SEQUENCE.fetch_add(1, Ordering::SeqCst);
            let fingerprint = capture_tmux_recent_pane_snapshot(&stored.id)
                .ok()
                .map(|snapshot| terminal_output_fingerprint(&snapshot));
            dirty |= prepare_restored_output_observation_locked(
                &mut state,
                &stored.id,
                fingerprint,
                snapshot_probe_sequence,
            );

            // 恢复重建只把 tmux 后端挂进 live_sessions，但终端列表/持久化依赖
            // sessions_by_id 与 sessions_by_path。若该会话不在已加载的注册表里（例如重启前
            // 刚创建、或被旧进程丢弃），这里必须补登记，否则重启后 /sessions 看不到它、
            // 下次再重启也不会被保护。用 upsert 语义避免覆盖已存在的同名会话登记。
            if !state.sessions_by_id.contains_key(&stored.id) {
                state
                    .sessions_by_path
                    .entry(stored.path.clone())
                    .or_default()
                    .push(stored.id.clone());
                state
                    .sessions_by_id
                    .insert(stored.id.clone(), stored.clone());
                dirty = true;
            }

            if !record.input_history.is_empty() {
                state.input_histories.insert(
                    stored.id.clone(),
                    TerminalInputHistoryCapture {
                        buffer: String::new(),
                        entries: clamp_input_history_entries(record.input_history.clone()),
                        paste_active: false,
                    },
                );
                dirty = true;
            }

            let restore_command =
                match prepare_codex_history_command_for_user(&stored.user_name, &record.command) {
                    Ok(command) => command,
                    Err(error) => {
                        warn!("prepare shutdown restore command for {} failed: {error}", stored.id);
                        result.failed.push((stored.id, error.to_string()));
                        continue;
                    }
                };
            if let Err(error) = send_backend_startup_script(&stored.id, &restore_command) {
                warn!("send shutdown restore command for {} failed: {error}", stored.id);
                result.failed.push((stored.id, error.to_string()));
                continue;
            }

            remaining_records.retain(|item| item.session_id != record.session_id);
            result.succeeded.insert(record.session_id.clone());
        }

        drop(state);
        if dirty {
            self.persist_state();
        }

        if remaining_records.len() != records.len() {
            let registry = TerminalShutdownRestoreRegistry {
                records: remaining_records,
            };
            if let Err(error) =
                persist_terminal_shutdown_restore_registry(&self.shutdown_restore_file, &registry)
            {
                warn!(
                    "clear shutdown restore registry {} failed: {error}",
                    self.shutdown_restore_file.display()
                );
            }
        }
        result
    }

    pub fn finalize_output_observations_for_shutdown(&self) {
        let started_at = Instant::now();
        let (session_ids, live_sessions) = {
            let state = crate::lock_or_recover!(self.state.read());
            (
                state.live_sessions.keys().cloned().collect::<Vec<_>>(),
                state.live_sessions.values().cloned().collect::<Vec<_>>(),
            )
        };
        // systemd signals the service cgroup together. Stop accepting PTY output before tmux
        // clients react to SIGTERM, otherwise their transient "[terminated]" status leaks into
        // the browser backlog and looks like the user's shell or agent exited.
        for session in live_sessions {
            session.prepare_for_shutdown();
        }
        if self.service_restart_pending.load(Ordering::SeqCst) {
            info!("preserving tmux clients during service restart");
        } else if let Err(error) = detach_tmux_clients_for_sessions(&session_ids) {
            warn!("detach tmux clients during shutdown failed: {error}");
        }
        thread::sleep(Duration::from_millis(250));

        let snapshots = session_ids
            .into_iter()
            .filter_map(|session_id| {
                let fingerprint = capture_tmux_recent_pane_snapshot(&session_id)
                    .ok()
                    .map(|snapshot| terminal_output_fingerprint(&snapshot))?;
                let sequence = TERMINAL_ACTIVITY_PROBE_SEQUENCE.fetch_add(1, Ordering::SeqCst);
                Some((session_id, fingerprint, sequence))
            })
            .collect::<Vec<_>>();
        let mut state = crate::lock_or_recover!(self.state.write());
        for (session_id, fingerprint, sequence) in snapshots {
            rebaseline_terminal_output_locked(&mut state, &session_id, Some(fingerprint), sequence);
        }
        drop(state);
        self.persist_state();
        info!(
            elapsed_ms = started_at.elapsed().as_millis(),
            "terminal shutdown output snapshot completed"
        );
    }

    pub fn prepare_service_restart(&self) {
        self.service_restart_pending.store(true, Ordering::SeqCst);
    }

    /// 收集并持久化当前活动 agent 会话的关机恢复记录，返回保存的记录数。
    ///
    /// 它用于"保存会话并关机/重启服务"这类由用户显式触发、需要先确认会话已落盘
    /// 再执行关机或重启的场景；允许对探测不到 resume id 的活跃 agent 发送 Ctrl-C
    /// 促使其退出并落盘。关机保存会话的正确性取决于：在 tmux server 和 codex/claude
    /// 子进程还活着时完成检测与写盘，所以这个动作必须在 systemd 关机流程之前由
    /// 用户主动触发。信号到达时的自动保存走 `save_shutdown_restore_registry_for_signal`。
    pub fn save_shutdown_restore_registry(&self) -> Result<usize> {
        let registry = self.collect_shutdown_restore_registry(true);
        let session_ids = registry
            .records
            .iter()
            .map(|record| record.session_id.clone())
            .collect::<Vec<_>>();
        self.try_persist_workspace_restores(&session_ids)?;
        let saved = registry.records.len();
        persist_terminal_shutdown_restore_registry(&self.shutdown_restore_file, &registry)
            .with_context(|| {
                format!(
                    "cannot persist terminal shutdown restores to {}",
                    self.shutdown_restore_file.display()
                )
            })?;
        Ok(saved)
    }

    /// 收到关机/重启信号（SIGTERM/SIGINT）时的自动保存：写入与菜单命令同一份
    /// 关机恢复注册表，用户不再需要先点「保存会话并关机/重启服务」。
    ///
    /// 与显式保存的区别：
    /// - 绝不向工作中的 agent 发送 Ctrl-C。systemd 服务重启依赖 tmux scope 存活
    ///   继续既有终端，自动保存不能打断正在运行的会话；探测不到 resume id 的会话
    ///   本次不记录，行为与信号到来前一致。
    /// - 关机竞态下 tmux scope 可能先于本进程停止，导致探测不到任何会话。此时若用
    ///   空结果覆盖恢复文件，会清掉上一次成功保存的记录；因此只有当仍有登记会话且
    ///   所有 tmux 后端都不可达时才保留现有文件，其余情况照常写盘。
    pub fn save_shutdown_restore_registry_for_signal(&self) -> Result<usize> {
        let registry = self.collect_shutdown_restore_registry(false);
        if registry.records.is_empty() && self.registered_sessions_have_unreachable_backends() {
            info!(
                "signal shutdown save skipped: all tmux backends unreachable, keep existing restore records"
            );
            return Ok(0);
        }
        let session_ids = registry
            .records
            .iter()
            .map(|record| record.session_id.clone())
            .collect::<Vec<_>>();
        self.try_persist_workspace_restores(&session_ids)?;
        let saved = registry.records.len();
        persist_terminal_shutdown_restore_registry(&self.shutdown_restore_file, &registry)
            .with_context(|| {
                format!(
                    "cannot persist terminal shutdown restores to {}",
                    self.shutdown_restore_file.display()
                )
            })?;
        Ok(saved)
    }

    /// 判断是否处于"登记了会话但所有 tmux 后端都不可达"的状态。仅在信号保存收集
    /// 结果为空时调用；每个会话一次 tmux 探测，不重复扫描进程表。
    fn registered_sessions_have_unreachable_backends(&self) -> bool {
        let session_ids = {
            let state = crate::lock_or_recover!(self.state.read());
            state.sessions_by_id.keys().cloned().collect::<Vec<_>>()
        };
        if session_ids.is_empty() {
            return false;
        }
        !session_ids
            .iter()
            .any(|session_id| capture_tmux_recent_pane_snapshot(session_id).is_ok())
    }

    /// 停掉 webClx 自己创建的 tmux scope（`webclx-tmux-<id>.scope`），但不杀全局 tmux server。
    ///
    /// 用于「保存会话并重启服务」：在 `save_shutdown_restore_registry` 已把 resume 记录落盘后，
    /// 显式杀掉这些 scope，让服务重启后走「从恢复记录重建终端」的完整路径——而不是靠
    /// systemd scope 隔离让 tmux 续命。这样重启服务也能验证保存/恢复链路，与「保存会话并关机」
    /// 行为一致（关机时 systemd 会杀掉整个 cgroup 含 tmux；服务重启不会，所以需要主动杀）。
    ///
    /// 仅在 Linux 下生效；非 Linux 平台为空操作。失败只记日志、不阻断后续重启流程，
    /// 因为重启本身才是主要目标，scope 没杀干净只是少了「从记录恢复」这一路径。
    pub fn stop_tmux_servers(&self) {
        if cfg!(not(target_os = "linux")) {
            return;
        }

        let mut command = Command::new("systemctl");
        command.arg("stop").arg("webclx-tmux-*.scope");
        let output =
            crate::process_exec::bounded_output(command, Duration::from_secs(10), 64 * 1024);

        match output {
            Ok(result) if result.status.success() => {
                info!("stopped webclx-tmux scopes before service restart");
            }
            Ok(result) => {
                let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
                warn!("stop webclx-tmux scopes returned non-zero: {stderr}");
            }
            Err(error) => {
                warn!("stop webclx-tmux scopes failed: {error}");
            }
        }

        // 同步把 live 会话标记为不可用，避免重启前的请求还尝试往已死的 tmux 写。
        let live_sessions = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let live_sessions = state.live_sessions.values().cloned().collect::<Vec<_>>();
            state.live_sessions.clear();
            live_sessions
        };
        for session in live_sessions {
            session.terminate_child();
        }
    }

    fn collect_shutdown_restore_registry(
        &self,
        allow_agent_interrupt: bool,
    ) -> TerminalShutdownRestoreRegistry {
        let agent_detector = TerminalAgentDetector::new();
        let resume_detector = match ResumeSessionDetector::new() {
            Ok(detector) => Some(detector),
            Err(error) => {
                warn!("initialize shutdown resume detector failed: {error}");
                None
            }
        };
        let stored_sessions = {
            let state = crate::lock_or_recover!(self.state.read());
            state.sessions_by_id.values().cloned().collect::<Vec<_>>()
        };

        let mut records = Vec::new();
        for stored in stored_sessions {
            let agent_activity = agent_detector.detect(&stored.id);
            let Some(live_session) = crate::lock_or_recover!(self.state.read())
                .live_sessions
                .get(&stored.id)
                .cloned()
                .filter(|session| session.is_alive())
            else {
                // 没有 live tmux 后端，detect/buffer 扫描都无从谈起。
                continue;
            };
            drop(live_session);

            // 不再用 agent_activity.is_active() 作为进入检测的闸门：进程名匹配可能漏掉
            // `comm=node` 的 claude/codex 启动器或其它变体，但 process_fd / buffer 兜底
            // 仍可能抓到 resume id。只有显式保存（菜单命令）才允许对仍活跃但未探测到
            // resume id 的 agent 发送 Ctrl-C；信号触发的自动保存绝不中断工作中的会话——
            // 服务重启依赖 tmux scope 存活继续既有终端。
            let mut detected = resume_detector
                .as_ref()
                .and_then(|detector| detector.detect(&stored.id));
            if detected.is_none() && allow_agent_interrupt && agent_activity.is_active() {
                self.send_shutdown_ctrl_c_fallback(&stored.id, &agent_activity);
                thread::sleep(Duration::from_millis(super::SHUTDOWN_CTRL_C_DELAY_MS));
                detected = resume_detector
                    .as_ref()
                    .and_then(|detector| detector.detect(&stored.id));
            }

            let Some(detected) = detected else {
                continue;
            };

            let input_history = {
                let state = crate::lock_or_recover!(self.state.read());
                state
                    .input_histories
                    .get(&stored.id)
                    .map(|history| history.entries.clone())
                    .unwrap_or_default()
            };
            records.push(TerminalResumeRestoreRecord {
                session_id: stored.id.clone(),
                path: stored.path.clone(),
                user_name: stored.user_name.clone(),
                name: stored.name.clone(),
                title: stored.title.clone(),
                codex_api_preset_name: stored.codex_api_preset_name.clone(),
                codex_api_base_url: stored.codex_api_base_url.clone(),
                origin: stored.origin,
                owner_key: stored.owner_key.clone(),
                manually_renamed: stored.manually_renamed,
                idle: stored.idle,
                created_at: stored.created_at,
                last_opened_at: stored.last_opened_at,
                group_token: stored.group_token.clone(),
                group_color: stored.group_color.clone(),
                group_name: stored.group_name.clone(),
                group_role: stored.group_role.clone(),
                group_order: stored.group_order,
                group_fixed_order: stored.group_fixed_order,
                input_history,
                resume_id: detected.info.resume_id,
                command: detected.info.command,
                program: detected.info.program,
                source: detected.source.to_string(),
                updated_at: current_timestamp_millis(),
            });
        }

        sort_terminal_shutdown_restore_records(&mut records);
        TerminalShutdownRestoreRegistry { records }
    }

    fn send_shutdown_ctrl_c_fallback(
        &self,
        session_id: &str,
        agent_activity: &TerminalAgentActivity,
    ) {
        let ctrl_c_count = if agent_activity
            .agents
            .iter()
            .any(|agent| agent.eq_ignore_ascii_case("Claude"))
        {
            super::SHUTDOWN_CTRL_C_CLAUDE_TOTAL_COUNT
        } else {
            1
        };

        for _ in 0..ctrl_c_count {
            let _ = self.send_session_input_direct_or_backend(session_id, "\u{3}".to_string());
            thread::sleep(Duration::from_millis(super::SHUTDOWN_CTRL_C_DELAY_MS));
        }
    }

    pub(super) fn subscribe_events(&self) -> broadcast::Receiver<TerminalManagerEvent> {
        self.event_sender.subscribe()
    }

    pub fn list_scheduled_inputs(&self) -> Vec<TerminalScheduledInputTaskInfo> {
        self.reconcile_unconfirmed_scheduled_messages();
        let tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());
        scheduled_input_task_infos(&tasks)
    }

    /// 迟到的原生用户记录可以把 pending/unknown 收成 confirmed。
    /// 只读会话文件，不补发正文，也不把缺少记录当成仍占用输入框。
    fn reconcile_unconfirmed_scheduled_messages(&self) {
        let pending = {
            let tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());
            tasks
                .values()
                .filter(|task| {
                    task.task_type == "agent_message"
                        && matches!(task.status.as_str(), "pending_confirmation" | "unknown")
                })
                .map(|task| {
                    (task.session_id.clone(), task.id.clone(), format!("[scheduled:{}]", task.id))
                })
                .collect::<Vec<_>>()
        };
        if pending.is_empty() {
            return;
        }
        let mut by_session: HashMap<String, Vec<(String, String)>> = HashMap::new();
        for (session_id, task_id, marker) in pending {
            by_session
                .entry(session_id)
                .or_default()
                .push((task_id, marker));
        }
        let mut confirmed_ids = Vec::new();
        for (session_id, items) in by_session {
            let Ok(Some(entries)) = self.session_agent_rollout_history(&session_id) else {
                continue;
            };
            for (task_id, marker) in items {
                if terminal_message_delivery_count(&entries, &marker) > 0 {
                    confirmed_ids.push(task_id);
                }
            }
        }
        if confirmed_ids.is_empty() {
            return;
        }
        let mut tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());
        let mut changed = false;
        for task_id in confirmed_ids {
            if let Some(task) = tasks.get_mut(&task_id) {
                if matches!(task.status.as_str(), "pending_confirmation" | "unknown") {
                    task.status = "confirmed".into();
                    task.last_error.clear();
                    changed = true;
                }
            }
        }
        if changed {
            prune_completed_scheduled_input_tasks(&mut tasks);
            self.persist_scheduled_input_tasks_locked(&tasks);
        }
    }

    pub fn scheduled_input_status(&self, task_id: &str) -> Result<TerminalScheduledInputTaskInfo> {
        let snapshot = crate::lock_or_recover!(self.scheduled_input_tasks.lock())
            .get(task_id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("scheduled input task not found"))?;
        // Read-only reconciliation: late rollout persistence is not a reason
        // to inject another body or Enter.
        if snapshot.task_type == "agent_message"
            && matches!(snapshot.status.as_str(), "pending_confirmation" | "unknown")
        {
            let marker = format!("[scheduled:{}]", snapshot.id);
            if self
                .session_agent_rollout_history(&snapshot.session_id)?
                .is_some_and(|entries| terminal_message_delivery_count(&entries, &marker) > 0)
            {
                let mut tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());
                if let Some(task) = tasks.get_mut(task_id) {
                    task.status = "confirmed".into();
                    task.last_error.clear();
                }
                self.persist_scheduled_input_tasks_locked(&tasks);
            }
        }
        let tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());
        tasks
            .get(task_id)
            .map(scheduled_input_task_info)
            .ok_or_else(|| anyhow::anyhow!("scheduled input task not found"))
    }

    pub fn create_scheduled_input(
        &self,
        session_id: &str,
        text: String,
        due_at_millis: u64,
        label: String,
        send_enter: bool,
        task_type: String,
        working_dir: String,
        repeat_interval_millis: u64,
    ) -> Result<TerminalScheduledInputTaskInfo> {
        scheduled_input::validate_scheduled_input_repeat_interval(repeat_interval_millis)?;
        let task_type = if task_type.trim().is_empty() && send_enter {
            // Legacy API callers often omit task_type. A submitting scheduled
            // message needs the agent delivery marker and bounded Enter retry
            // path; otherwise a busy TUI can treat the lone Enter as a newline.
            "agent_message".to_string()
        } else {
            task_type.trim().to_string()
        };
        if task_type == "agent_message" && !send_enter {
            anyhow::bail!("agent_message requires send_enter");
        }
        let normalized_text = normalize_scheduled_input_text(&text);
        if normalized_text.trim().is_empty() {
            anyhow::bail!("scheduled input text is empty");
        }
        if normalized_text.len() > super::MAX_TERMINAL_SCHEDULED_INPUT_BYTES {
            anyhow::bail!("scheduled input text is too large");
        }
        let now = current_timestamp_millis();
        if due_at_millis <= now {
            anyhow::bail!("scheduled input time must be in the future");
        }
        let terminal_name = {
            let state = crate::lock_or_recover!(self.state.read());
            state
                .sessions_by_id
                .get(session_id)
                .map(|session| session.name.clone())
                .ok_or_else(|| anyhow::anyhow!("terminal session not found"))?
        };
        let task = TerminalScheduledInputTask {
            status: String::new(),
            last_error: String::new(),
            id: format!("paste-{now}-{}", self.next_id.fetch_add(1, Ordering::SeqCst)),
            session_id: session_id.to_string(),
            terminal_name,
            due_at_millis,
            created_at_millis: now,
            label: label.trim().to_string(),
            text: normalized_text,
            send_enter,
            task_type,
            working_dir: working_dir.trim().to_string(),
            repeat_interval_millis,
            parent_task_id: String::new(),
        };
        let info = scheduled_input_task_info(&task);
        {
            let mut tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());
            let mut pending = tasks.values().cloned().collect::<Vec<_>>();
            pending.push(task.clone());
            persist_terminal_scheduled_input_registry(
                &self.scheduled_input_file,
                &StoredTerminalScheduledInputRegistry { tasks: pending },
            )?;
            tasks.insert(task.id.clone(), task);
        }
        self.scheduled_input_notify.notify_one();
        Ok(info)
    }

    pub fn cancel_scheduled_input(&self, task_id: &str) -> Result<bool> {
        let removed = {
            let mut tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());
            if tasks
                .get(task_id)
                .is_some_and(|task| task.status == "dispatching")
            {
                anyhow::bail!(
                    "scheduled input is dispatching; cancellation cannot retract written bytes"
                );
            }
            if !tasks.contains_key(task_id) {
                false
            } else {
                let mut next_tasks = tasks.clone();
                next_tasks.remove(task_id);
                persist_terminal_scheduled_input_registry(
                    &self.scheduled_input_file,
                    &StoredTerminalScheduledInputRegistry {
                        tasks: next_tasks.values().cloned().collect(),
                    },
                )?;
                *tasks = next_tasks;
                true
            }
        };
        if removed {
            self.scheduled_input_notify.notify_one();
        }
        Ok(removed)
    }

    pub fn update_scheduled_input(
        &self,
        task_id: &str,
        update: ScheduledInputUpdate,
    ) -> Result<TerminalScheduledInputTaskInfo> {
        // Validate and normalize incoming fields before touching storage so a
        // bad value never leaves the stored task half-updated.
        if let Some(due_at_millis) = update.due_at {
            let now = current_timestamp_millis();
            if due_at_millis <= now {
                anyhow::bail!("scheduled input time must be in the future");
            }
        }
        if let Some(repeat_interval_millis) = update.repeat_interval_millis {
            scheduled_input::validate_scheduled_input_repeat_interval(repeat_interval_millis)?;
        }
        let normalized_text = match update.text {
            Some(raw) => {
                let normalized = normalize_scheduled_input_text(&raw);
                if normalized.trim().is_empty() {
                    anyhow::bail!("scheduled input text is empty");
                }
                if normalized.len() > super::MAX_TERMINAL_SCHEDULED_INPUT_BYTES {
                    anyhow::bail!("scheduled input text is too large");
                }
                Some(normalized)
            }
            None => None,
        };
        // If the caller wants to retarget the task, resolve the terminal name
        // for the new session so the stored task stays consistent.
        let resolved_name = match update.session_id.as_deref() {
            Some(new_session_id) => {
                let trimmed = new_session_id.trim();
                if trimmed.is_empty() {
                    anyhow::bail!("目标终端不能为空");
                }
                let state = crate::lock_or_recover!(self.state.read());
                let name = state
                    .sessions_by_id
                    .get(trimmed)
                    .map(|session| session.name.clone())
                    .ok_or_else(|| anyhow::anyhow!("terminal session not found"))?;
                Some((trimmed.to_string(), name))
            }
            None => None,
        };
        let info = {
            let mut tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());
            if !tasks.contains_key(task_id) {
                anyhow::bail!("scheduled input task not found");
            }
            if !tasks[task_id].status.is_empty() {
                anyhow::bail!(
                    "only queued scheduled input can be edited; do not resend delivered text"
                );
            }
            let mut next_tasks = tasks.clone();
            let task = next_tasks
                .get_mut(task_id)
                .ok_or_else(|| anyhow::anyhow!("scheduled input task not found"))?;
            if let Some(due_at_millis) = update.due_at {
                task.due_at_millis = due_at_millis;
            }
            if let Some(text) = normalized_text {
                task.text = text;
            }
            if let Some(send_enter) = update.send_enter {
                task.send_enter = send_enter;
            }
            if let Some(task_type) = update.task_type {
                let trimmed = task_type.trim().to_string();
                if !trimmed.is_empty() {
                    task.task_type = trimmed;
                }
            }
            if let Some(label) = update.label {
                task.label = label.trim().to_string();
            }
            if let Some((session_id, terminal_name)) = resolved_name {
                task.session_id = session_id;
                task.terminal_name = terminal_name;
            }
            if let Some(repeat_interval_millis) = update.repeat_interval_millis {
                task.repeat_interval_millis = repeat_interval_millis;
            }
            let info = scheduled_input_task_info(task);
            persist_terminal_scheduled_input_registry(
                &self.scheduled_input_file,
                &StoredTerminalScheduledInputRegistry {
                    tasks: next_tasks.values().cloned().collect(),
                },
            )?;
            *tasks = next_tasks;
            info
        };
        self.scheduled_input_notify.notify_one();
        Ok(info)
    }

    fn scheduled_input_avoid_window(&self) -> String {
        let Some(app_dir) = self.state_file.parent() else {
            return String::new();
        };
        match SettingsManager::load(app_dir) {
            Ok(settings) => settings.terminal_scheduled_input_avoid_window(),
            Err(error) => {
                warn!(
                    "load settings for scheduled input avoid window failed {}: {error}",
                    app_dir.display()
                );
                String::new()
            }
        }
    }

    fn spawn_scheduled_input_runner(&self) {
        // 生产环境（运行于 tokio 运行时内）正常 spawn；同步测试构造 manager 时没有运行时，
        // 此处静默跳过，避免 tokio::spawn 触发 panic（后台循环在没有事件循环时也无法运行）。
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let manager = self.clone();
        tokio::spawn(async move {
            manager.run_scheduled_input_loop().await;
        });
    }

    async fn run_scheduled_input_loop(self) {
        self.run_scheduled_input_loop_with_batch_sender(Arc::new(|manager, tasks| {
            manager.fire_scheduled_input_task_batch(tasks, &Self::scheduled_input_one_sender());
        }))
        .await;
    }

    fn scheduled_input_one_sender() -> TerminalScheduledInputOneSender {
        Arc::new(|manager: &TerminalManager, task: &TerminalScheduledInputTask| {
            manager.fire_scheduled_input_task(task)
        })
    }

    pub(in crate::terminal) async fn run_scheduled_input_loop_with_batch_sender(
        self,
        send_batch: TerminalScheduledInputBatchSender,
    ) {
        loop {
            // Collection takes the task mutex and may persist rescheduling or
            // removal. Isolate the whole synchronous turn, not only delivery.
            let manager = self.clone();
            let send_batch = send_batch.clone();
            let next_due_at = match tokio::task::spawn_blocking(move || {
                let now = current_timestamp_millis();

                // Avoid-window gate: when a high-price avoidance time range is
                // configured, any task that becomes due while the current local
                // time falls inside that range is deferred to the end of the
                // range instead of being fired immediately. Tasks already past
                // their original due time are rescheduled; the loop will wake up
                // at the window boundary to fire them.
                let avoid_window = manager.scheduled_input_avoid_window();
                let defer_until =
                    if !avoid_window.is_empty() && now_within_active_window(&avoid_window) {
                        avoid_window_end_epoch_millis(&avoid_window)
                    } else {
                        None
                    };

                let (due_tasks, next_due_at) = {
                    let mut tasks = crate::lock_or_recover!(manager.scheduled_input_tasks.lock());

                    // If we are inside the avoid window, reschedule tasks subject to avoid window
                    // that are due now (or overdue) to fire at the window end.
                    // Inter-terminal agent messages (task_type == "agent_message") are internal
                    // peer-to-peer coordination between active agents, NOT automated API continuations,
                    // and must never be deferred by the peak-price avoid window.
                    if let Some(end_millis) = defer_until {
                        let mut rescheduled = 0usize;
                        for task in tasks.values_mut() {
                            if task.status.is_empty()
                                && task.task_type != "agent_message"
                                && task.due_at_millis <= now
                            {
                                task.due_at_millis = end_millis;
                                rescheduled += 1;
                            }
                        }
                        if rescheduled > 0 {
                            manager.persist_scheduled_input_tasks_locked(&tasks);
                        }
                    }

                    let due_tasks = match claim_and_persist_due_scheduled_input_tasks(
                        &mut tasks,
                        now,
                        defer_until,
                        |snapshot| {
                            persist_terminal_scheduled_input_registry(
                                &manager.scheduled_input_file,
                                &StoredTerminalScheduledInputRegistry {
                                    tasks: snapshot.values().cloned().collect(),
                                },
                            )
                        },
                    ) {
                        Ok(due_tasks) => due_tasks,
                        Err(error) => {
                            warn!("cannot persist scheduled delivery claim: {error}");
                            return Some(now.saturating_add(1000));
                        }
                    };
                    let next_due_at = tasks
                        .values()
                        .filter(|task| task.status.is_empty())
                        .map(|task| task.due_at_millis)
                        .min();
                    (due_tasks, next_due_at)
                };

                if !due_tasks.is_empty() {
                    send_batch(&manager, due_tasks);
                }
                next_due_at
            })
            .await
            {
                Ok(next_due_at) => next_due_at,
                Err(error) => {
                    if error.is_panic() {
                        warn!("scheduled terminal input batch task panicked: {error}");
                    } else {
                        warn!("scheduled terminal input batch task cancelled: {error}");
                    }
                    Some(current_timestamp_millis().saturating_add(250))
                }
            };

            let sleep_ms = next_due_at
                .map(|due_at| {
                    due_at
                        .saturating_sub(current_timestamp_millis())
                        .clamp(250, 60_000)
                })
                .unwrap_or(60_000);
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(sleep_ms)) => {}
                _ = self.scheduled_input_notify.notified() => {}
            }
        }
    }

    /// Run every task in a due batch through the same injectable one-task
    /// sender. A failure is logged and does not prevent later tasks from being
    /// attempted; this mirrors the scheduler's "continue the batch" contract.
    pub(in crate::terminal) fn fire_scheduled_input_task_batch(
        &self,
        tasks: Vec<TerminalScheduledInputTask>,
        send_one: &TerminalScheduledInputOneSender,
    ) {
        for task in &tasks {
            // Receipt evidence describes this task, not the current composer.
            // A missing rollout marker must not lease the terminal indefinitely.
            // The sender finishes its bounded submit attempt before the next task;
            // old pending/unknown bodies are never retried by this batch.
            if let Err(error) = send_one(self, task) {
                warn!(
                    "send scheduled terminal input {} to {} failed: {error}",
                    task.id, task.session_id
                );
            }
        }
    }

    fn fire_scheduled_input_task(&self, task: &TerminalScheduledInputTask) -> Result<()> {
        // Dedicated agent messages carry a unique task marker so an older
        // identical prompt cannot be mistaken for this submission.
        let agent_message = task.task_type == "agent_message";
        let text = if agent_message {
            format!(
                "[scheduled:{}] {}",
                task.id,
                task.text.split_whitespace().collect::<Vec<_>>().join(" ")
            )
        } else {
            task.text.clone()
        };
        let result = (|| -> Result<&str> {
            if agent_message {
                self.ensure_session_agent_active(&task.session_id)?;
            }
            let history = if task.send_enter && agent_message {
                self.session_agent_rollout_history(&task.session_id)?
            } else {
                None
            };
            let evidence = if agent_message {
                format!("[scheduled:{}]", task.id)
            } else {
                text.clone()
            };
            let baseline = history
                .as_deref()
                .map(|entries| terminal_message_delivery_count(entries, &evidence))
                .unwrap_or(0);
            let verify = task.send_enter && agent_message;
            let body = if agent_message {
                prepare_terminal_message_body(&text, true)
            } else {
                scheduled_input_body(task)
            };
            self.send_session_input(&task.session_id, body)?;
            if !task.send_enter {
                return Ok("written");
            }
            thread::sleep(terminal_message_paste_settle_delay(&text, true));
            if !verify {
                self.send_session_enter_key(&task.session_id)?;
                return Ok("unverified");
            }
            submit_scheduled_message(
                || self.session_message_input_queued(&task.session_id),
                || {
                    self.ensure_session_agent_active(&task.session_id)?;
                    self.send_session_enter_key(&task.session_id)
                },
                || {
                    let entries = self.session_agent_rollout_history(&task.session_id)?;
                    Ok(terminal_message_delivery_confirmed(entries.as_deref(), &evidence, baseline))
                },
                || thread::sleep(Duration::from_millis(100)),
            )
        })();
        let (status, error) = match &result {
            Ok(status) => ((*status).to_string(), String::new()),
            // A write can partially succeed: do not automatically resend on error.
            Err(error) => ("unknown".into(), error.to_string()),
        };
        {
            let mut tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());
            if let Some(stored) = tasks.get_mut(&task.id) {
                stored.status = status;
                stored.last_error = error;
            }
            prune_completed_scheduled_input_tasks(&mut tasks);
            let registry = StoredTerminalScheduledInputRegistry {
                tasks: tasks.values().cloned().collect(),
            };
            persist_terminal_scheduled_input_registry(&self.scheduled_input_file, &registry)?;
        }
        result.map(|_| ())
    }

    fn persist_scheduled_input_tasks_locked(
        &self,
        tasks: &HashMap<String, TerminalScheduledInputTask>,
    ) {
        let registry = StoredTerminalScheduledInputRegistry {
            tasks: tasks.values().cloned().collect(),
        };
        if let Err(error) =
            persist_terminal_scheduled_input_registry(&self.scheduled_input_file, &registry)
        {
            warn!(
                "persist terminal scheduled input registry failed {}: {error}",
                self.scheduled_input_file.display()
            );
        }
    }

    pub fn send_session_input(&self, session_id: &str, data: String) -> Result<()> {
        self.write_session_input_inner(session_id, data, true)
    }

    /// A visible Agent queue already owns submitted input. Missing rollout
    /// evidence is not permission to press Enter into the user's next draft.
    pub fn session_message_input_queued(&self, session_id: &str) -> Result<bool> {
        let snapshot = capture_tmux_visible_text_pane_snapshot(session_id)?;
        Ok(String::from_utf8_lossy(&snapshot)
            .lines()
            .any(error_detection::is_terminal_queued_input_line))
    }

    pub fn retry_session_message_enter(&self, session_id: &str) -> Result<bool> {
        let snapshot = capture_tmux_visible_text_pane_snapshot(session_id)?;
        submit_message_enter_unless_queued(&String::from_utf8_lossy(&snapshot), || {
            self.send_session_enter_key(session_id)
        })
    }

    /// Send a semantic Enter key, matching the terminal soft keyboard's
    /// shortcut. This is used as a recovery submission when a raw PTY CR was
    /// absorbed by an agent TUI as a newline.
    pub fn send_session_enter_key(&self, session_id: &str) -> Result<()> {
        let state = crate::lock_or_recover!(self.state.read());
        if !state.sessions_by_id.contains_key(session_id) {
            anyhow::bail!("terminal session not found");
        }
        drop(state);

        #[cfg(not(windows))]
        {
            send_tmux_enter_key(session_id)
        }
        #[cfg(windows)]
        {
            self.write_session_input_inner(session_id, "\r".to_string(), true)
        }
    }

    pub fn send_session_continue(&self, session_id: &str) -> Result<()> {
        // 冷却记账与 tmux 写入在同一会话 send_lock 内串行：同会话的并发
        // continue/compact 不会字节交错，也不会并发穿透冷却。map 锁只在
        // 克隆锁与记账的瞬间持有，绝不压过发送。
        let send_lock = self.session_send_lock(session_id);
        let _send_guard = crate::lock_or_recover!(send_lock.lock());
        self.send_terminal_command_with_enter(session_id, TERMINAL_CONTINUE_COMMAND)?;
        crate::lock_or_recover!(self.per_session_runtime.lock())
            .entry(session_id.to_string())
            .or_default()
            .auto_continue_last_sent_at = Some(current_timestamp_millis());
        Ok(())
    }

    pub(super) fn send_session_auto_continue(
        &self,
        session_id: &str,
        effective_interval_millis: u64,
    ) -> Result<TerminalAutoContinueSendOutcome> {
        let interval_millis = effective_interval_millis.max(1);
        let now = current_timestamp_millis();
        let send_lock = self.session_send_lock(session_id);
        let _send_guard = crate::lock_or_recover!(send_lock.lock());
        let last_sent_at_millis = crate::lock_or_recover!(self.per_session_runtime.lock())
            .get(session_id)
            .and_then(|entry| entry.auto_continue_last_sent_at);
        if let Some(retry_at_millis) =
            auto_continue_retry_at_millis(last_sent_at_millis, interval_millis, now)
        {
            return Ok(TerminalAutoContinueSendOutcome::Cooldown {
                last_sent_at_millis: last_sent_at_millis.unwrap_or(now),
                retry_at_millis,
            });
        }
        self.send_terminal_command_with_enter(session_id, TERMINAL_CONTINUE_COMMAND)?;
        crate::lock_or_recover!(self.per_session_runtime.lock())
            .entry(session_id.to_string())
            .or_default()
            .auto_continue_last_sent_at = Some(current_timestamp_millis());
        Ok(TerminalAutoContinueSendOutcome::Sent)
    }

    pub(super) fn send_session_auto_continue_if_error(
        &self,
        session_id: &str,
        error_line_limit: u32,
        error_keywords: &[String],
        keyword_actions: &[TerminalErrorKeywordAction],
        auto_continue_time_patterns: &[String],
        respect_manual_interrupt: bool,
        interval_seconds: u32,
    ) -> Result<TerminalAutoContinueSendOutcome> {
        let Some(error_match) = terminal_error_keyword_match(
            session_id,
            error_line_limit,
            error_keywords,
            auto_continue_time_patterns,
            respect_manual_interrupt,
        ) else {
            return Ok(TerminalAutoContinueSendOutcome::NotEligible);
        };
        if error_match.input_queued {
            return Ok(TerminalAutoContinueSendOutcome::NotEligible);
        }
        // Look up the configured action for the matched keyword. Keywords
        // without an explicit rule default to "continue".
        let action = if terminal_error_keyword_is_payload_too_large(&error_match.keyword) {
            settings_core::TERMINAL_ERROR_KEYWORD_ACTION_COMPACT_THEN_CONTINUE.to_string()
        } else {
            resolve_terminal_error_keyword_action(&error_match.keyword, keyword_actions)
        };
        match action.as_str() {
            "compact_then_continue" => {
                // A previous retry for this error may already have submitted
                // "继续". The terminal remains busy while that turn runs;
                // sending /compact again at this point is rejected by Codex
                // ("compact is disabled while a task is in progress"). Wait
                // for the next distinct error signature instead of issuing
                // two competing recovery commands.
                if error_match.continue_sent {
                    return Ok(TerminalAutoContinueSendOutcome::NotEligible);
                }
                let effective_interval_millis = u64::from(interval_seconds.max(1)) * 1000;
                self.send_session_auto_compact(
                    session_id,
                    &error_match.signature,
                    effective_interval_millis,
                )
            }
            "mark_only" => Ok(TerminalAutoContinueSendOutcome::NotEligible),
            _ => {
                let effective_interval_millis = u64::from(interval_seconds.max(1)) * 1000;
                self.send_session_auto_continue(session_id, effective_interval_millis)
            }
        }
    }

    pub(super) fn glm_quota_auto_continue_requires_preflight(
        &self,
        session_id: &str,
        error_line_limit: u32,
        error_keywords: &[String],
        auto_continue_time_patterns: &[String],
        respect_manual_interrupt: bool,
    ) -> bool {
        let Some(error_match) = terminal_error_keyword_match(
            session_id,
            error_line_limit,
            error_keywords,
            auto_continue_time_patterns,
            respect_manual_interrupt,
        ) else {
            return false;
        };
        if error_match.input_queued {
            return false;
        }
        let persisted_base_url = {
            let state = crate::lock_or_recover!(self.state.read());
            state
                .sessions_by_id
                .get(session_id)
                .map(|session| session.codex_api_base_url.clone())
                .unwrap_or_default()
        };
        let live_preset_id = TerminalAgentDetector::new().api_preset_id(session_id);
        let live_api_base_url = live_preset_id.and_then(|preset_id| {
            self.api_preset_snapshot.read().ok().and_then(|presets| {
                presets
                    .iter()
                    .find(|preset| preset.id == preset_id)
                    .map(|preset| preset.base_url.clone())
            })
        });
        let Ok(output) = capture_tmux_recent_pane_snapshot(session_id) else {
            return false;
        };
        let output = String::from_utf8_lossy(&output);
        let quota_error = terminal_error_keyword_is_quota_like(&error_match.keyword)
            || terminal_recent_output_has_quota_error(&output, error_line_limit);
        let glm_session = terminal_quota_preflight_uses_zhipu_provider(
            &persisted_base_url,
            live_api_base_url.as_deref(),
            &output,
        );
        quota_error && glm_session
    }

    pub fn send_session_message(
        &self,
        target: &str,
        path: Option<&Path>,
        data: String,
        submit_enters: u8,
        bracketed_paste: bool,
        require_active_agent: bool,
    ) -> Result<(String, String, bool, String)> {
        let target = target.trim();
        if target.is_empty() {
            anyhow::bail!("terminal target is empty");
        }

        let (session_id, session_name) = self.resolve_session_target(target, path)?;
        let renamed_target = target != session_name && target != session_id;
        if require_active_agent {
            self.ensure_session_agent_active(&session_id)?;
        }

        if submit_enters == 1 && terminal_continue_command_text(&data) {
            self.send_session_continue(&session_id)?;
            return Ok((session_id, session_name.clone(), renamed_target, session_name));
        }

        if !data.is_empty() {
            let body = prepare_terminal_message_body(&data, bracketed_paste);
            self.send_session_input_direct_or_backend(&session_id, body)?;
        }
        if submit_enters > 0 {
            thread::sleep(terminal_message_paste_settle_delay(&data, bracketed_paste));
        }
        for index in 0..submit_enters {
            if require_active_agent {
                self.ensure_session_agent_active(&session_id)?;
            }
            // Submit through the same semantic key path as the soft keyboard.
            // A raw PTY CR can be absorbed by raw-mode TUIs as a composer newline.
            self.send_session_enter_key(&session_id)?;
            if index + 1 < submit_enters {
                thread::sleep(Duration::from_millis(120));
            }
        }
        self.persist_workspace_restores(&[session_id.clone()]);
        Ok((session_id, session_name.clone(), renamed_target, session_name))
    }

    fn terminal_message_delivery_digest(data: &str) -> String {
        hex::encode(Sha256::digest(data.as_bytes()))
    }

    pub fn terminal_message_delivery_status(
        &self,
        session_id: &str,
        delivery_id: &str,
        data: &str,
    ) -> Result<&'static str> {
        let delivery_id = delivery_id.trim();
        if delivery_id.is_empty() {
            return Ok("unknown");
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0);
        {
            let ledger = crate::lock_or_recover!(self.terminal_message_ledger.lock());
            // 账本损坏或上次落盘失败时不能假装 unknown——那会让调用方重发
            // 正文。健康检查先于一切记录判断，data 为空也一样。
            ledger.ensure_healthy()?;
            if !data.is_empty() {
                ledger.ensure_no_conflict(
                    session_id,
                    delivery_id,
                    &Self::terminal_message_delivery_digest(data),
                )?;
            }
            if ledger.is_active(session_id, delivery_id, now) {
                return Ok("in_flight");
            }
        }
        let confirmed = self
            .session_agent_rollout_history(session_id)
            .ok()
            .flatten()
            .is_some_and(|entries| terminal_message_delivery_count(&entries, delivery_id) > 0);
        if confirmed {
            crate::lock_or_recover!(self.terminal_message_ledger.lock())
                .mark_confirmed(session_id, delivery_id);
            return Ok("confirmed");
        }
        // 先取出记录状态再释放账本锁，避免持锁再进 rollout 读取路径。
        let record_state = crate::lock_or_recover!(self.terminal_message_ledger.lock())
            .record(session_id, delivery_id)
            .map(|record| record.state);
        match record_state {
            Some(DeliveryState::Confirmed) => Ok("confirmed"),
            // 终端 agent 没有 rollout 证据面时投递永远无法被 rollout 确认；
            // 正文已注入的记录上报 submitted_unverifiable，避免调用方无限重试。
            Some(_) if !self.session_agent_rollout_verifiable(session_id) => {
                Ok("submitted_unverifiable")
            }
            Some(_) => Ok("pending"),
            None => Ok("unknown"),
        }
    }

    pub fn claim_terminal_message_delivery(
        &self,
        session_id: &str,
        delivery_id: &str,
        data: &str,
    ) -> Result<bool> {
        let delivery_id = delivery_id.trim();
        if delivery_id.is_empty() {
            return Ok(true);
        }

        let expected = Self::terminal_message_delivery_digest(data);

        {
            let mut ledger = crate::lock_or_recover!(self.terminal_message_ledger.lock());
            // 准入健康门（含一次落盘重试）先于冲突检查：persist_error 若只走
            // ensure_healthy 会在到达 try_claim 的重试点之前永久挡死认领。
            ledger.ensure_healthy_for_admission()?;
            ledger.ensure_no_conflict(session_id, delivery_id, &expected)?;
        }

        if self
            .session_agent_rollout_history(session_id)
            .ok()
            .flatten()
            .is_some_and(|entries| terminal_message_delivery_count(&entries, delivery_id) > 0)
        {
            crate::lock_or_recover!(self.terminal_message_ledger.lock()).upsert_confirmed(
                session_id,
                delivery_id,
                expected,
            )?;
            return Ok(false);
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|value| value.as_secs())
            .unwrap_or(0);
        crate::lock_or_recover!(self.terminal_message_ledger.lock()).try_claim(
            session_id,
            delivery_id,
            expected,
            now,
        )
    }

    /// 投递失败时不再移除记录：调用点均为发送失败路径，正文是否到达终端
    /// 不确定，删除记录会让同一 delivery_id 被自动重发。改为标记 Failed。
    pub fn release_terminal_message_delivery(&self, session_id: &str, delivery_id: &str) {
        let delivery_id = delivery_id.trim();
        if delivery_id.is_empty() {
            return;
        }
        crate::lock_or_recover!(self.terminal_message_ledger.lock()).mark_failed(
            session_id.trim(),
            delivery_id,
            "message send failed; delivery outcome is uncertain",
        );
    }

    /// 投递账本的只读快照，供投递状态查询接口返回可观察状态。
    pub fn terminal_message_delivery_record(
        &self,
        session_id: &str,
        delivery_id: &str,
    ) -> Option<DeliveryRecord> {
        let delivery_id = delivery_id.trim();
        if delivery_id.is_empty() {
            return None;
        }
        crate::lock_or_recover!(self.terminal_message_ledger.lock()).record(session_id, delivery_id)
    }

    /// 正文已写入终端。delivery_id 为空或账本中无记录时空操作。
    pub fn mark_terminal_message_injected(&self, session_id: &str, delivery_id: &str) {
        let delivery_id = delivery_id.trim();
        if delivery_id.is_empty() {
            return;
        }
        crate::lock_or_recover!(self.terminal_message_ledger.lock())
            .mark_injected(session_id, delivery_id);
    }

    /// 记录 Enter 提交尝试总数并把状态推进到 Submitted。
    pub fn record_terminal_message_submit_attempt(
        &self,
        session_id: &str,
        delivery_id: &str,
        attempts: u32,
    ) {
        let delivery_id = delivery_id.trim();
        if delivery_id.is_empty() {
            return;
        }
        crate::lock_or_recover!(self.terminal_message_ledger.lock()).record_submit_attempt(
            session_id,
            delivery_id,
            attempts,
        );
    }

    pub fn mark_terminal_message_confirmed(&self, session_id: &str, delivery_id: &str) {
        let delivery_id = delivery_id.trim();
        if delivery_id.is_empty() {
            return;
        }
        crate::lock_or_recover!(self.terminal_message_ledger.lock())
            .mark_confirmed(session_id, delivery_id);
    }

    pub fn mark_terminal_message_unconfirmed(&self, session_id: &str, delivery_id: &str) {
        let delivery_id = delivery_id.trim();
        if delivery_id.is_empty() {
            return;
        }
        crate::lock_or_recover!(self.terminal_message_ledger.lock())
            .mark_unconfirmed(session_id, delivery_id);
    }

    pub fn mark_terminal_message_failed(&self, session_id: &str, delivery_id: &str, error: &str) {
        let delivery_id = delivery_id.trim();
        if delivery_id.is_empty() {
            return;
        }
        crate::lock_or_recover!(self.terminal_message_ledger.lock()).mark_failed(
            session_id,
            delivery_id,
            error,
        );
    }

    pub fn ensure_session_agent_active(&self, session_id: &str) -> Result<()> {
        let state = crate::lock_or_recover!(self.state.read());
        let terminal_is_live = state
            .live_sessions
            .get(session_id)
            .is_some_and(|session| session.is_alive());
        if !terminal_is_live {
            anyhow::bail!("terminal session is not active");
        }
        drop(state);

        if !TerminalAgentDetector::new().detect(session_id).is_active() {
            anyhow::bail!("agent process is not active");
        }
        Ok(())
    }

    pub fn send_session_toast(
        &self,
        target: &str,
        path: Option<&Path>,
        message: String,
        tone: &str,
    ) -> Result<(String, String)> {
        let target = target.trim();
        if target.is_empty() {
            anyhow::bail!("terminal target is empty");
        }

        let (session_id, session_name) = self.resolve_session_target(target, path)?;
        let normalized_tone = match tone {
            "ok" | "warn" | "muted" => tone,
            _ => "info",
        };
        let _ = self.event_sender.send(TerminalManagerEvent::Toast {
            session_id: session_id.clone(),
            message,
            tone: normalized_tone.to_string(),
        });
        Ok((session_id, session_name))
    }

    /// Broadcast a toast to every connected webClx UI by emitting it with an
    /// empty `session_id`. The frontend shows such toasts regardless of the
    /// currently active session (`!message.session_id`), so this is the global
    /// notification path used when a backend event (e.g. an upstream-proxy
    /// request) has no associated terminal session to target.
    pub fn broadcast_toast(&self, message: impl Into<String>, tone: &str) {
        let normalized_tone = match tone {
            "ok" | "warn" | "muted" => tone,
            _ => "info",
        };
        let _ = self.event_sender.send(TerminalManagerEvent::Toast {
            session_id: String::new(),
            message: message.into(),
            tone: normalized_tone.to_string(),
        });
    }

    pub fn resolve_session_target(
        &self,
        target: &str,
        path: Option<&Path>,
    ) -> Result<(String, String)> {
        let state = crate::lock_or_recover!(self.state.read());
        let direct = state
            .sessions_by_id
            .get(target)
            .filter(|session| path.is_none_or(|path| session.path == path))
            .map(|session| (session.id.clone(), session.name.clone()));
        if let Some(match_by_id) = direct {
            return Ok(match_by_id);
        }

        let matches = state
            .sessions_by_id
            .values()
            .filter(|session| session.name == target)
            .filter(|session| path.is_none_or(|path| session.path == path))
            .map(|session| (session.id.clone(), session.name.clone()))
            .collect::<Vec<_>>();

        if matches.is_empty() {
            if let Some(session_id) = state.previous_terminal_names.get(target).cloned() {
                if let Some(session) = state.sessions_by_id.get(&session_id) {
                    if path.is_none_or(|path| session.path == path) {
                        return Ok((session.id.clone(), session.name.clone()));
                    }
                }
            }
        }

        match matches.as_slice() {
            [single] => Ok(single.clone()),
            [] => anyhow::bail!("terminal session not found"),
            _ => anyhow::bail!("terminal name `{target}` is ambiguous; pass session_id or path"),
        }
    }

    pub fn session_user_name(&self, session_id: &str) -> Result<String> {
        let state = crate::lock_or_recover!(self.state.read());
        state
            .sessions_by_id
            .get(session_id)
            .map(|session| session.user_name.clone())
            .with_context(|| format!("终端会话 `{session_id}` 不存在"))
    }

    fn fallback_user_profile(&self) -> runtime_paths::UserProfile {
        runtime_paths::resolve_current_user_profile().unwrap_or_else(|| {
            runtime_paths::resolve_user_profile(&default_terminal_user_name())
                .expect("default terminal user should resolve")
        })
    }

    fn send_session_input_direct_or_backend(&self, session_id: &str, data: String) -> Result<()> {
        self.write_session_input_inner(session_id, data, true)
    }

    fn send_terminal_command_with_enter(&self, session_id: &str, command: &str) -> Result<()> {
        self.send_session_input_direct_or_backend(session_id, command.to_string())?;
        thread::sleep(Duration::from_millis(TERMINAL_COMMAND_ENTER_DELAY_MS));
        self.send_session_input_direct_or_backend(session_id, "\r".to_string())
    }

    /// Sends a slash command (e.g. `/compact`) to a terminal, mirroring the
    /// frontend `sendSlashCommand` soft-keyboard path: type the command, wait
    /// for Codex to register it, press Enter, wait for the confirmation dialog,
    /// then press Enter again to confirm. A plain `command + Enter` would leave
    /// Codex's slash-menu dialog open without executing.
    fn send_terminal_slash_command(&self, session_id: &str, command: &str) -> Result<()> {
        self.ensure_session_compact_allowed(session_id, command)?;
        self.send_session_input_direct_or_backend(session_id, command.to_string())?;
        thread::sleep(Duration::from_millis(TERMINAL_SLASH_COMMAND_ENTER_DELAY_MS));
        self.send_session_input_direct_or_backend(session_id, "\r".to_string())?;
        thread::sleep(Duration::from_millis(TERMINAL_SLASH_COMMAND_CONFIRM_DELAY_MS));
        self.send_session_input_direct_or_backend(session_id, "\r".to_string())
    }

    /// Compact is itself the recovery action for context exhaustion. It uses
    /// the same per-session cooldown as automatic continue so the scanner and
    /// browser cannot repeatedly inject `/compact` while compaction is running.
    fn send_session_auto_compact(
        &self,
        session_id: &str,
        error_signature: &str,
        effective_interval_millis: u64,
    ) -> Result<TerminalAutoContinueSendOutcome> {
        let activity = TerminalAgentDetector::new().detect(session_id);
        if terminal_compact_command_blocked_agent(TERMINAL_COMPACT_COMMAND, &activity).is_some() {
            return Ok(TerminalAutoContinueSendOutcome::NotEligible);
        }

        if self.error_matches_previous_compact(session_id, error_signature) {
            return Ok(TerminalAutoContinueSendOutcome::NotEligible);
        }
        let interval_millis = effective_interval_millis.max(1);
        let now = current_timestamp_millis();
        let send_lock = self.session_send_lock(session_id);
        let _send_guard = crate::lock_or_recover!(send_lock.lock());
        let last_sent_at_millis = crate::lock_or_recover!(self.per_session_runtime.lock())
            .get(session_id)
            .and_then(|entry| entry.auto_continue_last_sent_at);
        if let Some(retry_at_millis) =
            auto_continue_retry_at_millis(last_sent_at_millis, interval_millis, now)
        {
            return Ok(TerminalAutoContinueSendOutcome::Cooldown {
                last_sent_at_millis: last_sent_at_millis.unwrap_or(now),
                retry_at_millis,
            });
        }
        self.send_terminal_slash_command(session_id, TERMINAL_COMPACT_COMMAND)?;
        // 发送成功后冷却时间戳与 compact 标记在同一临界区记账。
        let mut runtime = crate::lock_or_recover!(self.per_session_runtime.lock());
        let entry = runtime.entry(session_id.to_string()).or_default();
        entry.auto_continue_last_sent_at = Some(current_timestamp_millis());
        entry.auto_compact_signature = Some(error_signature.to_string());
        Ok(TerminalAutoContinueSendOutcome::CompactSent)
    }

    fn error_matches_previous_compact(&self, session_id: &str, error_signature: &str) -> bool {
        crate::lock_or_recover!(self.per_session_runtime.lock())
            .get(session_id)
            .is_some_and(|entry| entry.auto_compact_signature.as_deref() == Some(error_signature))
    }

    /// Sends input to a terminal without recording it to the per-session
    /// input history (the "本终端对话历史" panel).
    ///
    /// Used for webClx-auto-typed commands — initial session launch,
    /// quick-start, `reload_claude`, resume-command injection — which
    /// must type the line into the pane (so the agent actually starts)
    /// but should not clutter the user's history view.
    pub fn send_session_input_silent(&self, session_id: &str, data: String) -> Result<()> {
        self.ensure_session_compact_allowed(session_id, &data)?;
        self.write_session_input_inner(session_id, data, false)
    }

    fn ensure_session_compact_allowed(&self, session_id: &str, command: &str) -> Result<()> {
        if command.split_whitespace().next() != Some(TERMINAL_COMPACT_COMMAND) {
            return Ok(());
        }
        let activity = TerminalAgentDetector::new().detect(session_id);
        if let Some(agent) = terminal_compact_command_blocked_agent(command, &activity) {
            anyhow::bail!("{agent} 终端禁止由 webClx 发送 /compact。");
        }
        Ok(())
    }

    /// Persist the API preset selected by a webClx-generated launch.
    pub fn update_session_api_preset(
        &self,
        session_id: &str,
        preset_name: String,
        base_url: String,
    ) -> Result<()> {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            anyhow::bail!("终端会话不能为空");
        }

        let mut state = crate::lock_or_recover!(self.state.write());
        let session = state
            .sessions_by_id
            .get_mut(session_id)
            .with_context(|| "终端会话不存在")?;
        let changed =
            session.codex_api_preset_name != preset_name || session.codex_api_base_url != base_url;
        if !changed {
            return Ok(());
        }
        session.codex_api_preset_name = preset_name;
        session.codex_api_base_url = base_url;
        drop(state);
        self.persist_state();
        self.notify_session_list_changed("updated", session_id.to_string());
        Ok(())
    }

    /// Refresh the session's displayed provider from Codex itself instead of
    /// trusting the API preset that was active when the terminal was created.
    pub fn extract_codex_preset_from_status(
        &self,
        session_id: &str,
    ) -> Result<TerminalPresetExtractionResponse> {
        let session_id = session_id.trim();
        if session_id.is_empty() {
            anyhow::bail!("终端会话不能为空");
        }

        {
            let state = crate::lock_or_recover!(self.state.read());
            if !state.sessions_by_id.contains_key(session_id) {
                anyhow::bail!("终端会话不存在");
            }
        }

        let before = capture_tmux_recent_pane_snapshot(session_id).unwrap_or_default();
        let before_provider_count = codex_model_provider_line_count(&before);
        let before_fingerprint = (!before.is_empty()).then(|| terminal_output_fingerprint(&before));
        let before_observation = {
            let state = crate::lock_or_recover!(self.state.read());
            state.output_observations.get(session_id).cloned()
        };

        let extraction_result = (|| {
            // Keep this system probe out of the terminal's user input history.
            self.send_session_input_silent(session_id, "/status".to_string())?;
            thread::sleep(Duration::from_millis(TERMINAL_COMMAND_ENTER_DELAY_MS));
            self.send_session_input_silent(session_id, "\r".to_string())?;

            let started = Instant::now();
            while started.elapsed() < TERMINAL_PRESET_STATUS_TIMEOUT {
                if let Ok(snapshot) = capture_tmux_recent_pane_snapshot(session_id) {
                    let output_changed = snapshot != before;
                    let provider_count = codex_model_provider_line_count(&snapshot);
                    if (output_changed || provider_count > before_provider_count)
                        && let Some(provider) = parse_codex_model_provider(&snapshot)
                    {
                        return self.persist_codex_preset_from_provider(session_id, provider);
                    }
                }
                thread::sleep(TERMINAL_PRESET_STATUS_POLL);
            }

            anyhow::bail!("未在 /status 输出中读取到 Model provider");
        })();

        let after_fingerprint = capture_tmux_recent_pane_snapshot(session_id)
            .ok()
            .filter(|snapshot| !snapshot.is_empty())
            .map(|snapshot| terminal_output_fingerprint(&snapshot))
            .or(before_fingerprint);
        self.restore_activity_after_system_probe(
            session_id,
            before_observation.as_ref(),
            after_fingerprint,
        );
        extraction_result
    }

    /// System probes such as `/status` intentionally do not change the user's
    /// activity state. Rebase the pane fingerprint to the post-probe output
    /// while restoring the timestamps that determine viewed/completed labels.
    fn restore_activity_after_system_probe(
        &self,
        session_id: &str,
        before: Option<&TerminalOutputObservation>,
        after_fingerprint: Option<u64>,
    ) {
        let mut state = crate::lock_or_recover!(self.state.write());
        restore_system_probe_output_observation_locked(
            &mut state,
            session_id,
            before,
            after_fingerprint,
            TERMINAL_ACTIVITY_PROBE_SEQUENCE.fetch_add(1, Ordering::SeqCst),
        );
        drop(state);
        self.persist_state();
    }

    fn persist_codex_preset_from_provider(
        &self,
        session_id: &str,
        provider: String,
    ) -> Result<TerminalPresetExtractionResponse> {
        let (preset_name, base_url) = split_codex_model_provider(&provider);
        if preset_name.is_empty() {
            anyhow::bail!("Model provider 为空");
        }

        let mut state = crate::lock_or_recover!(self.state.write());
        let session = state
            .sessions_by_id
            .get_mut(session_id)
            .with_context(|| "终端会话不存在")?;
        let changed =
            session.codex_api_preset_name != preset_name || session.codex_api_base_url != base_url;
        session.codex_api_preset_name = preset_name.clone();
        session.codex_api_base_url = base_url.clone();
        if changed {
            drop(state);
            self.persist_state();
        }

        Ok(TerminalPresetExtractionResponse {
            session_id: session_id.to_string(),
            preset_name,
            base_url,
            provider,
        })
    }

    fn write_session_input_inner(
        &self,
        session_id: &str,
        data: String,
        record_history: bool,
    ) -> Result<()> {
        let state = crate::lock_or_recover!(self.state.read());
        if !state.sessions_by_id.contains_key(session_id) {
            anyhow::bail!("terminal session not found");
        }

        if let Some(session) = state.live_sessions.get(session_id) {
            let writer = session.writer.clone();
            drop(state);
            {
                use std::io::Write;
                let mut writer = crate::lock_or_recover!(writer.lock());
                writer.write_all(data.as_bytes())?;
                writer.flush()?;
            }
            if record_history {
                self.record_session_input(session_id, &data);
            }
            return Ok(());
        }

        send_backend_input(&state, session_id, &data)?;
        drop(state);
        if record_history {
            self.record_session_input(session_id, &data);
        }
        Ok(())
    }

    pub fn session_input_history(
        &self,
        session_id: &str,
    ) -> Result<Vec<TerminalInputHistoryEntry>> {
        let state = crate::lock_or_recover!(self.state.read());
        if !state.sessions_by_id.contains_key(session_id) {
            anyhow::bail!("terminal session not found");
        }
        drop(state);

        // 对 Codex/Claude agent 会话，优先读取真实 rollout 文件作为对话历史来源。
        // 按键重建法看不到 Tab 补全/历史回调/agent TUI 输入，会丢命令；rollout 文件
        // 里是用户真实输入的完整文本，是权威来源。
        if let Some(entries) = self.session_agent_rollout_history(session_id)? {
            return Ok(filter_terminal_input_history_entries(entries));
        }

        // 普通 shell 终端没有 rollout 文件，回退到按键重建历史。
        let state = crate::lock_or_recover!(self.state.read());
        let entries = state
            .input_histories
            .get(session_id)
            .map(|history| history.entries.clone())
            .unwrap_or_default();
        Ok(filter_terminal_input_history_entries(entries))
    }

    pub fn session_agent_rollout_history(
        &self,
        session_id: &str,
    ) -> Result<Option<Vec<TerminalInputHistoryEntry>>> {
        let state = crate::lock_or_recover!(self.state.read());
        if !state.sessions_by_id.contains_key(session_id) {
            anyhow::bail!("terminal session not found");
        }
        drop(state);
        Ok(rollout_history_entries(session_id))
    }

    /// 该会话是否存在可用于投递确认的 rollout 证据面（Codex/Claude）。
    /// 返回 false 表示该终端的 agent 不产生 rollout，投递无法被 rollout 确认。
    pub fn session_agent_rollout_verifiable(&self, session_id: &str) -> bool {
        self.session_agent_rollout_history(session_id)
            .ok()
            .flatten()
            .is_some()
    }

    fn load_workspace_restore_registry(&self) -> Result<TerminalWorkspaceRestoreRegistry> {
        load_terminal_workspace_restore_registry(&self.workspace_restore_file)
    }

    fn persist_workspace_restore_registry(
        &self,
        registry: &TerminalWorkspaceRestoreRegistry,
    ) -> Result<()> {
        persist_terminal_workspace_restore_registry(&self.workspace_restore_file, registry)
    }

    fn workspace_restore_record_for_session(
        &self,
        session_id: &str,
        previous: Option<&TerminalWorkspaceRestoreRecord>,
    ) -> Result<TerminalWorkspaceRestoreRecord> {
        let (stored, input_history) = {
            let state = crate::lock_or_recover!(self.state.read());
            let stored = state
                .sessions_by_id
                .get(session_id)
                .cloned()
                .with_context(|| format!("会话 `{session_id}` 不存在"))?;
            let input_history = state
                .input_histories
                .get(session_id)
                .map(|history| history.entries.clone())
                .unwrap_or_default();
            (stored, input_history)
        };

        let detected = detect_current_resume_session(&stored.id).ok().flatten();
        let detected_info = detected.as_ref().map(|item| &item.info);
        let (resume_id, command, program, source) = match detected_info {
            Some(info) => (
                info.resume_id.clone(),
                info.command.clone(),
                info.program.clone(),
                detected
                    .as_ref()
                    .map(|item| item.source.to_string())
                    .unwrap_or_default(),
            ),
            None => match previous {
                Some(record) => (
                    record.resume_id.clone(),
                    record.command.clone(),
                    record.program.clone(),
                    record.source.clone(),
                ),
                None => (String::new(), String::new(), String::new(), String::new()),
            },
        };
        Ok(TerminalWorkspaceRestoreRecord {
            session_id: stored.id.clone(),
            path: stored.path.clone(),
            user_name: stored.user_name.clone(),
            name: stored.name.clone(),
            title: stored.title.clone(),
            codex_api_preset_name: stored.codex_api_preset_name.clone(),
            codex_api_base_url: stored.codex_api_base_url.clone(),
            origin: stored.origin,
            owner_key: stored.owner_key.clone(),
            manually_renamed: stored.manually_renamed,
            idle: stored.idle,
            created_at: stored.created_at,
            last_opened_at: stored.last_opened_at,
            group_token: stored.group_token.clone(),
            group_color: stored.group_color.clone(),
            group_name: stored.group_name.clone(),
            group_role: stored.group_role.clone(),
            group_order: stored.group_order,
            group_fixed_order: stored.group_fixed_order,
            input_history: clamp_input_history_entries(input_history),
            resume_id,
            command,
            program,
            source,
            updated_at: current_timestamp_millis(),
            restore_count: previous.map(|record| record.restore_count).unwrap_or(0),
        })
    }

    pub fn persist_workspace_restores(&self, session_ids: &[String]) -> usize {
        match self.try_persist_workspace_restores(session_ids) {
            Ok(updated) => updated,
            Err(error) => {
                warn!(
                    path = %self.workspace_restore_file.display(),
                    "persist terminal workspace restore registry failed: {error}"
                );
                0
            }
        }
    }

    fn try_persist_workspace_restores(&self, session_ids: &[String]) -> Result<usize> {
        let unique_ids = session_ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .collect::<HashSet<_>>();
        if unique_ids.is_empty() {
            return Ok(0);
        }

        let _write_guard = crate::lock_or_recover!(self.workspace_restore_write_lock.lock());
        let mut registry = self.load_workspace_restore_registry()?;
        let mut updated = 0;
        for session_id in unique_ids {
            let previous = registry
                .records
                .iter()
                .find(|record| record.session_id == session_id)
                .cloned();
            match self.workspace_restore_record_for_session(session_id, previous.as_ref()) {
                Ok(record) => {
                    registry
                        .records
                        .retain(|item| item.session_id != record.session_id);
                    registry.records.push(record);
                    updated += 1;
                }
                Err(error) => {
                    warn!("capture terminal workspace restore {session_id} failed: {error}");
                }
            }
        }
        sort_terminal_workspace_restore_records(&mut registry.records);
        if updated > 0 {
            self.persist_workspace_restore_registry(&registry)?;
        }
        Ok(updated)
    }

    pub fn persist_all_workspace_restores(&self) -> Result<usize> {
        let session_ids = {
            let state = crate::lock_or_recover!(self.state.read());
            state.sessions_by_id.keys().cloned().collect::<Vec<_>>()
        };
        self.try_persist_workspace_restores(&session_ids)
    }

    /// 受控清理入口：删除指定会话的工作现场恢复记录。
    /// 会话仍存在时必须走正常删除会话流程（由其生命周期清理记录），
    /// 否则记录会在下一次现场保存时被重新写回。
    pub fn delete_workspace_restore_record(&self, session_id: &str) -> Result<bool> {
        {
            let state = crate::lock_or_recover!(self.state.read());
            if state.sessions_by_id.contains_key(session_id) {
                anyhow::bail!("会话 `{session_id}` 仍存在，请通过删除会话清理其工作现场记录");
            }
        }
        let _write_guard = crate::lock_or_recover!(self.workspace_restore_write_lock.lock());
        let mut registry = self.load_workspace_restore_registry()?;
        let previous_len = registry.records.len();
        registry
            .records
            .retain(|record| record.session_id != session_id);
        let removed = registry.records.len() != previous_len;
        if removed {
            self.persist_workspace_restore_registry(&registry)?;
        }
        Ok(removed)
    }

    pub fn list_workspace_restores(
        &self,
        base_dir: &Path,
        display_root: &Path,
    ) -> Result<Vec<TerminalWorkspaceRestoreInfo>> {
        let registry = self.load_workspace_restore_registry()?;
        Ok(registry
            .records
            .into_iter()
            .map(|record| {
                let recoverable = !record.name.is_empty() && record.path.is_dir();
                TerminalWorkspaceRestoreInfo {
                    session_id: record.session_id,
                    name: record.name,
                    user_name: record.user_name,
                    path: filesystem::relative_path(base_dir, &record.path)
                        .map(|relative| relative_to_string(&relative))
                        .unwrap_or_else(|_| record.path.to_string_lossy().into_owned()),
                    display_path: filesystem::display_path(base_dir, display_root, &record.path),
                    group_name: record.group_name,
                    group_role: record.group_role,
                    group_token: record.group_token,
                    group_color: record.group_color,
                    group_order: record.group_order,
                    group_fixed_order: record.group_fixed_order,
                    program: if record.program.is_empty() {
                        "shell".to_string()
                    } else {
                        record.program
                    },
                    resume_id: record.resume_id,
                    command: record.command,
                    updated_at: record.updated_at,
                    created_at: record.created_at,
                    restore_count: record.restore_count,
                    recoverable,
                }
            })
            .collect())
    }

    pub fn restore_terminal_workspaces(
        &self,
        requested_ids: &[String],
    ) -> Result<TerminalWorkspaceRestoreResult> {
        let registry = self.load_workspace_restore_registry()?;
        let requested: HashSet<_> = requested_ids
            .iter()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
            .collect();
        let records: Vec<TerminalWorkspaceRestoreRecord> = registry
            .records
            .into_iter()
            .filter(|record| requested.is_empty() || requested.contains(&record.session_id))
            .collect();

        let shutdown_records = records
            .iter()
            .map(|record| TerminalResumeRestoreRecord {
                session_id: record.session_id.clone(),
                path: record.path.clone(),
                user_name: record.user_name.clone(),
                name: record.name.clone(),
                title: record.title.clone(),
                codex_api_preset_name: record.codex_api_preset_name.clone(),
                codex_api_base_url: record.codex_api_base_url.clone(),
                origin: record.origin,
                owner_key: record.owner_key.clone(),
                manually_renamed: record.manually_renamed,
                idle: record.idle,
                created_at: record.created_at,
                last_opened_at: record.last_opened_at,
                group_token: record.group_token.clone(),
                group_color: record.group_color.clone(),
                group_name: record.group_name.clone(),
                group_role: record.group_role.clone(),
                group_order: record.group_order,
                group_fixed_order: record.group_fixed_order,
                input_history: record.input_history.clone(),
                resume_id: record.resume_id.clone(),
                command: record.command.clone(),
                program: record.program.clone(),
                source: record.source.clone(),
                updated_at: record.updated_at,
            })
            .collect::<Vec<_>>();
        let restore_result = self.restore_shutdown_sessions(&shutdown_records);

        let _write_guard = crate::lock_or_recover!(self.workspace_restore_write_lock.lock());
        let mut next_registry = self.load_workspace_restore_registry()?;
        let now = current_timestamp_millis();
        increment_workspace_restore_counts(
            &mut next_registry.records,
            &restore_result.succeeded,
            now,
        );
        sort_terminal_workspace_restore_records(&mut next_registry.records);
        self.persist_workspace_restore_registry(&next_registry)?;
        self.notify_session_list_changed("workspace_restored", "");
        let mut succeeded = restore_result.succeeded.into_iter().collect::<Vec<_>>();
        succeeded.sort();
        let mut skipped = restore_result
            .skipped
            .into_iter()
            .map(|(session_id, reason)| TerminalWorkspaceRestoreSkip { session_id, reason })
            .collect::<Vec<_>>();
        skipped.sort_by(|left, right| left.session_id.cmp(&right.session_id));
        let mut failed = restore_result
            .failed
            .into_iter()
            .map(|(session_id, reason)| TerminalWorkspaceRestoreFailure { session_id, reason })
            .collect::<Vec<_>>();
        failed.sort_by(|left, right| left.session_id.cmp(&right.session_id));
        Ok(TerminalWorkspaceRestoreResult {
            succeeded,
            skipped,
            failed,
        })
    }

    pub fn record_session_input(&self, session_id: &str, data: &str) {
        if self.capture_session_input(session_id, data) {
            self.persist_state();
        }
    }

    fn capture_session_input(&self, session_id: &str, data: &str) -> bool {
        if data.is_empty() {
            return false;
        }
        let mut state = crate::lock_or_recover!(self.state.write());
        if !state.sessions_by_id.contains_key(session_id) {
            return false;
        }
        record_session_input_locked(&mut state, session_id, data)
    }

    pub(super) fn record_session_input_deferred(&self, session_id: &str, data: &str) {
        if !self.capture_session_input(session_id, data) {
            return;
        }
        // One worker per manager, with one dirty flag, regardless of how many
        // Enter/paste events arrive while disk is slow. The shared state remains
        // authoritative and normal shutdown persistence also flushes this history.
        let mut work = crate::lock_or_recover!(self.input_persist_work.lock());
        work.1 = true;
        if work.0 {
            return;
        }
        work.0 = true;
        drop(work);
        let manager = self.clone();
        tokio::task::spawn_blocking(move || {
            loop {
                {
                    let mut work = crate::lock_or_recover!(manager.input_persist_work.lock());
                    if !work.1 {
                        work.0 = false;
                        break;
                    }
                    work.1 = false;
                }
                manager.persist_state();
            }
        });
    }

    pub fn mark_session_output_viewed_in_memory(&self, session_id: &str) {
        self.mark_session_output_viewed_with_persistence(session_id, false);
    }

    pub fn mark_session_output_viewed(&self, session_id: &str) {
        self.mark_session_output_viewed_with_persistence(session_id, true);
    }

    fn mark_session_output_viewed_with_persistence(&self, session_id: &str, persist: bool) {
        let snapshot_probe_sequence =
            TERMINAL_ACTIVITY_PROBE_SEQUENCE.fetch_add(1, Ordering::SeqCst);
        let snapshot_fingerprint = capture_tmux_recent_pane_snapshot(session_id)
            .ok()
            .map(|snapshot| terminal_output_fingerprint(&snapshot));
        let mut state = crate::lock_or_recover!(self.state.write());
        if !state.sessions_by_id.contains_key(session_id) {
            return;
        }
        mark_session_output_viewed_locked(
            &mut state,
            session_id,
            snapshot_fingerprint,
            snapshot_probe_sequence,
        );
        if persist {
            drop(state);
            self.persist_state();
        }
    }

    pub fn mark_session_opened(&self, session_id: &str) {
        let changed = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let changed = mark_session_opened_locked(&mut state, session_id);
            if changed {
                drop(state);
                self.persist_state();
            }
            changed
        };
        if changed {
            self.notify_session_list_changed("opened", session_id);
        }
    }

    fn schedule_auto_continue_tasks(
        &self,
        schedules: Vec<(String, TerminalAutoContinueSchedule)>,
        error_line_limit: u32,
        error_keywords: &[String],
        auto_continue_time_patterns: &[String],
        auto_continue_interval_seconds: u32,
        auto_continue_respect_manual_interrupt: bool,
    ) {
        for (session_id, schedule) in schedules {
            self.schedule_auto_continue_task(
                session_id,
                schedule,
                error_line_limit,
                auto_continue_interval_seconds,
                auto_continue_respect_manual_interrupt,
                error_keywords.to_vec(),
                auto_continue_time_patterns.to_vec(),
            );
        }
    }

    fn schedule_auto_continue_task(
        &self,
        session_id: String,
        schedule: TerminalAutoContinueSchedule,
        error_line_limit: u32,
        auto_continue_interval_seconds: u32,
        auto_continue_respect_manual_interrupt: bool,
        error_keywords: Vec<String>,
        auto_continue_time_patterns: Vec<String>,
    ) {
        {
            let tasks = crate::lock_or_recover!(self.auto_continue_schedules.lock());
            if auto_continue_task_matches(
                tasks.get(&session_id),
                &schedule,
                error_line_limit,
                auto_continue_interval_seconds,
                auto_continue_respect_manual_interrupt,
                &error_keywords,
                &auto_continue_time_patterns,
            ) {
                if self.auto_continue_cron_needs_refresh(&session_id, &schedule)
                    && let Err(error) = self.install_auto_continue_cron(&session_id, &schedule)
                {
                    warn!("refresh terminal auto-continue cron failed for {session_id}: {error}");
                }
                return;
            }
        }

        let cron_installed = match self.install_auto_continue_cron(&session_id, &schedule) {
            Ok(()) => true,
            Err(error) => {
                warn!("install terminal auto-continue cron failed for {session_id}: {error}");
                false
            }
        };
        let backend_due_at_millis = schedule.due_at_millis.saturating_add(if cron_installed {
            TERMINAL_AUTO_CONTINUE_CRON_FALLBACK_DELAY_MS
        } else {
            0
        });
        let terminal_name = {
            let state = crate::lock_or_recover!(self.state.read());
            state
                .sessions_by_id
                .get(&session_id)
                .map(|session| session.name.clone())
                .unwrap_or_default()
        };
        let task = TerminalAutoContinueTask {
            session_id: session_id.clone(),
            terminal_name,
            schedule,
            backend_due_at_millis,
            created_at_millis: current_timestamp_millis(),
            error_line_limit,
            auto_continue_interval_seconds,
            auto_continue_respect_manual_interrupt,
            error_keywords,
            auto_continue_time_patterns,
        };

        {
            let mut tasks = crate::lock_or_recover!(self.auto_continue_schedules.lock());
            tasks.insert(session_id, task);
            self.persist_auto_continue_tasks_locked(&tasks);
        }
        self.auto_continue_notify.notify_one();
    }

    fn install_auto_continue_cron(
        &self,
        session_id: &str,
        schedule: &TerminalAutoContinueSchedule,
    ) -> Result<()> {
        let cron = terminal_auto_continue_cron_entry(schedule.due_at_millis)?;
        let script_dir = self.auto_continue_cron_script_dir();
        fs::create_dir_all(&script_dir).with_context(|| {
            format!("create terminal auto-continue cron dir {}", script_dir.display())
        })?;
        let script_path = self.auto_continue_cron_script_path(session_id, &schedule.signature);
        let marker = terminal_auto_continue_cron_marker(session_id, &schedule.signature);
        let due_epoch_secs = schedule.due_at_millis / 1000;
        let script = terminal_auto_continue_cron_script(
            &marker,
            session_id,
            &schedule.keyword,
            &schedule.reset_at,
            cron.sleep_seconds,
        );
        let mut file = fs::File::create(&script_path).with_context(|| {
            format!("create auto-continue cron script {}", script_path.display())
        })?;
        file.write_all(script.as_bytes()).with_context(|| {
            format!("write auto-continue cron script {}", script_path.display())
        })?;
        set_executable_mode(&script_path);

        let current = current_crontab()?;
        let marker_prefix = terminal_auto_continue_cron_marker_prefix(session_id);
        let due_marker_prefix = terminal_auto_continue_due_marker_prefix(session_id);
        let mut next_lines: Vec<String> = current
            .lines()
            .filter(|line| !line.contains(&marker_prefix) && !line.contains(&due_marker_prefix))
            .map(ToString::to_string)
            .collect();
        next_lines.push(format!("# {marker}"));
        // Record the intended due epoch so the listing path can decide expiry
        // deterministically instead of guessing from the 5 cron fields.
        next_lines.push(terminal_auto_continue_due_marker(
            session_id,
            &schedule.signature,
            due_epoch_secs,
        ));
        next_lines.push(format!(
            "{} {} # {}",
            cron.fields,
            shell_quote_cron(script_path.to_string_lossy().as_ref()),
            marker
        ));
        let next = format!("{}\n", next_lines.join("\n"));
        install_crontab(&next)
    }

    fn auto_continue_cron_script_dir(&self) -> PathBuf {
        self.state_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("terminal-auto-continue-cron")
    }

    fn auto_continue_cron_script_path(&self, session_id: &str, signature: &str) -> PathBuf {
        self.auto_continue_cron_script_dir().join(format!(
            "{}-{}.sh",
            sanitize_cron_file_component(session_id),
            sanitize_cron_file_component(signature)
        ))
    }

    fn auto_continue_cron_needs_refresh(
        &self,
        session_id: &str,
        schedule: &TerminalAutoContinueSchedule,
    ) -> bool {
        if auto_continue_crontab_has_stale_due_markers(session_id, schedule) {
            return true;
        }
        let script_path = self.auto_continue_cron_script_path(session_id, &schedule.signature);
        let Ok(script) = fs::read_to_string(&script_path) else {
            return true;
        };
        !script.contains("/api/terminal/sessions/${SESSION_ID_ENCODED}/auto-continue")
            || !script.contains("TMUX_SOCKET=")
            || script.contains("tmux send-keys -t \"$TMUX_TARGET\" -l \"继续\"")
    }

    pub fn cancel_auto_continue_task(&self, marker: &str) -> Result<bool> {
        let Some((session_id, signature)) = terminal_auto_continue_marker_parts(marker) else {
            anyhow::bail!("invalid auto-continue task marker");
        };
        let removed = {
            let mut tasks = crate::lock_or_recover!(self.auto_continue_schedules.lock());
            let removed = tasks
                .get(&session_id)
                .is_some_and(|task| task.schedule.signature == signature);
            if removed {
                tasks.remove(&session_id);
                self.persist_auto_continue_tasks_locked(&tasks);
            }
            removed
        };
        if removed {
            self.remember_canceled_auto_continue_signature(&session_id, &signature);
            rewrite_crontab_without_markers(&[marker.to_string()])?;
            self.auto_continue_notify.notify_one();
        }
        Ok(removed)
    }

    pub fn update_auto_continue_task_due_at(&self, marker: &str, due_at_millis: u64) -> Result<()> {
        let Some((session_id, signature)) = terminal_auto_continue_marker_parts(marker) else {
            anyhow::bail!("invalid auto-continue task marker");
        };
        let now = current_timestamp_millis();
        if due_at_millis <= now {
            anyhow::bail!("auto-continue time must be in the future");
        }
        let task = {
            let mut tasks = crate::lock_or_recover!(self.auto_continue_schedules.lock());
            let task = tasks
                .get_mut(&session_id)
                .filter(|task| task.schedule.signature == signature)
                .ok_or_else(|| anyhow::anyhow!("auto-continue task not found"))?;
            task.schedule.due_at_millis = due_at_millis;
            task.backend_due_at_millis =
                due_at_millis.saturating_add(TERMINAL_AUTO_CONTINUE_CRON_FALLBACK_DELAY_MS);
            let task = task.clone();
            self.persist_auto_continue_tasks_locked(&tasks);
            task
        };
        self.forget_canceled_auto_continue_signature(&session_id, &signature);
        self.install_auto_continue_cron(&session_id, &task.schedule)?;
        self.auto_continue_notify.notify_one();
        Ok(())
    }

    /// 取消标记按会话隔离存放：集合内只存裸签名，会话 id 由 map 键承担，
    /// 不再拼接 `"{session}:{sig}"` 键。
    fn remember_canceled_auto_continue_signature(&self, session_id: &str, signature: &str) {
        crate::lock_or_recover!(self.per_session_runtime.lock())
            .entry(session_id.to_string())
            .or_default()
            .canceled_signatures
            .insert(signature.to_string());
    }

    fn forget_canceled_auto_continue_signature(&self, session_id: &str, signature: &str) {
        let mut runtime = crate::lock_or_recover!(self.per_session_runtime.lock());
        if let Some(entry) = runtime.get_mut(session_id) {
            entry.canceled_signatures.remove(signature);
            if entry.prunable() {
                runtime.remove(session_id);
            }
        }
    }

    /// 每个会话当前可见的错误签名（取消标记仅在该错误仍可见时保留）。
    fn active_session_error_signatures(
        &self,
        sessions: &[TerminalSessionInfo],
    ) -> HashMap<String, String> {
        sessions
            .iter()
            .filter_map(|session| {
                let signature = session.activity_error_signature.as_deref()?.trim();
                if signature.is_empty() {
                    return None;
                }
                Some((session.id.clone(), signature.to_string()))
            })
            .collect()
    }

    fn spawn_auto_continue_runner(&self) {
        // 同 spawn_scheduled_input_runner：无 tokio 运行时（同步测试）时跳过，避免 panic。
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let manager = self.clone();
        tokio::spawn(async move {
            manager.run_auto_continue_loop().await;
        });
    }

    async fn run_auto_continue_loop(self) {
        loop {
            self.refresh_auto_continue_crons();
            let now = current_timestamp_millis();
            let (due_tasks, next_due_at) = {
                let mut tasks = crate::lock_or_recover!(self.auto_continue_schedules.lock());
                let due_session_ids = tasks
                    .values()
                    .filter(|task| task.backend_due_at_millis <= now)
                    .map(|task| task.session_id.clone())
                    .collect::<Vec<_>>();
                let due_tasks = due_session_ids
                    .iter()
                    .filter_map(|session_id| tasks.remove(session_id))
                    .collect::<Vec<_>>();
                if !due_tasks.is_empty() {
                    self.persist_auto_continue_tasks_locked(&tasks);
                }
                let next_due_at = tasks.values().map(|task| task.backend_due_at_millis).min();
                (due_tasks, next_due_at)
            };

            for task in due_tasks {
                self.run_due_auto_continue_task(&task);
            }

            let sleep_ms = next_due_at
                .map(|due_at| {
                    due_at
                        .saturating_sub(current_timestamp_millis())
                        .clamp(250, 60_000)
                })
                .unwrap_or(60_000);
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(sleep_ms)) => {}
                _ = self.auto_continue_notify.notified() => {}
            }
        }
    }

    fn refresh_auto_continue_crons(&self) {
        let tasks = {
            let tasks = crate::lock_or_recover!(self.auto_continue_schedules.lock());
            tasks
                .values()
                .map(|task| (task.session_id.clone(), task.schedule.clone()))
                .collect::<Vec<_>>()
        };
        for (session_id, schedule) in tasks {
            if self.auto_continue_cron_needs_refresh(&session_id, &schedule)
                && let Err(error) = self.install_auto_continue_cron(&session_id, &schedule)
            {
                warn!("refresh terminal auto-continue cron failed for {session_id}: {error}");
            }
        }
    }

    fn run_due_auto_continue_task(&self, task: &TerminalAutoContinueTask) {
        let session_id = &task.session_id;
        let schedule = &task.schedule;
        let respect_manual_interrupt = self
            .auto_continue_respect_manual_interrupt
            .load(Ordering::Relaxed);
        let interval_seconds = self
            .auto_continue_interval_seconds
            .load(Ordering::Relaxed)
            .clamp(1, 86400) as u32;

        let Some(error_match) = terminal_error_keyword_match(
            session_id,
            task.error_line_limit,
            &task.error_keywords,
            &task.auto_continue_time_patterns,
            respect_manual_interrupt,
        ) else {
            return;
        };
        if error_match.signature != schedule.signature || error_match.input_queued {
            return;
        }

        let base_interval_millis = u64::from(interval_seconds.max(1)) * 1000;
        let consecutive_attempts = crate::lock_or_recover!(self.per_session_runtime.lock())
            .get(session_id.as_str())
            .and_then(|entry| entry.error_auto_continue.as_ref())
            .map(|record| record.consecutive_attempts)
            .unwrap_or(0);
        let backoff_factor =
            self.auto_continue_backoff_factor.load(Ordering::Relaxed) as f64 / 1000.0;
        let backoff_max_millis = self
            .auto_continue_backoff_max_millis
            .load(Ordering::Relaxed);
        let effective_interval_millis = auto_continue_backoff_interval_millis(
            base_interval_millis,
            consecutive_attempts.max(1),
            backoff_factor,
            backoff_max_millis,
        );
        match self.send_session_auto_continue(session_id, effective_interval_millis) {
            Ok(TerminalAutoContinueSendOutcome::CompactSent) => {
                self.notify_session_list_changed("auto_continue", session_id);
            }
            Ok(TerminalAutoContinueSendOutcome::Sent) => {
                let _ = self.event_sender.send(TerminalManagerEvent::Toast {
                    session_id: session_id.to_string(),
                    message: format!(
                        "限额已过重置时间 {} 1 分钟，已发送“继续”。",
                        schedule.reset_at
                    ),
                    tone: "ok".to_string(),
                });
                self.notify_session_list_changed("auto_continue", session_id);
            }
            Ok(TerminalAutoContinueSendOutcome::Cooldown {
                last_sent_at_millis,
                retry_at_millis,
            }) => {
                if last_sent_at_millis < schedule.due_at_millis {
                    self.reschedule_auto_continue_task_after_cooldown(task, retry_at_millis);
                }
            }
            Ok(TerminalAutoContinueSendOutcome::NotEligible) => {}
            Err(error) => {
                warn!("scheduled terminal auto-continue failed for {session_id}: {error}");
            }
        }
    }

    fn reschedule_auto_continue_task_after_cooldown(
        &self,
        task: &TerminalAutoContinueTask,
        retry_at_millis: u64,
    ) {
        let mut task = task.clone();
        task.schedule.due_at_millis = retry_at_millis;
        task.backend_due_at_millis =
            retry_at_millis.saturating_add(TERMINAL_AUTO_CONTINUE_CRON_FALLBACK_DELAY_MS);
        if let Err(error) = self.install_auto_continue_cron(&task.session_id, &task.schedule) {
            warn!(
                "reschedule terminal auto-continue after cooldown failed for {}: {error}",
                task.session_id
            );
            task.backend_due_at_millis = retry_at_millis;
        }
        let mut tasks = crate::lock_or_recover!(self.auto_continue_schedules.lock());
        tasks.insert(task.session_id.clone(), task);
        self.persist_auto_continue_tasks_locked(&tasks);
        self.auto_continue_notify.notify_one();
    }

    pub fn spawn_error_auto_continue_runner(
        &self,
        settings: SettingsManager,
        auth_manager: auth_core::AuthPresetManager,
    ) {
        let manager = self.clone();
        tokio::spawn(async move {
            manager
                .run_error_auto_continue_loop(settings, auth_manager)
                .await;
        });
    }

    async fn run_error_auto_continue_loop(
        self,
        settings: SettingsManager,
        auth_manager: auth_core::AuthPresetManager,
    ) {
        loop {
            let api_presets = auth_manager.api_presets_snapshot();
            self.update_api_preset_snapshot(api_presets.clone());
            self.scan_error_auto_continue_sessions(&settings, &api_presets)
                .await;
            tokio::time::sleep(Duration::from_millis(TERMINAL_ERROR_AUTO_CONTINUE_SCAN_MS)).await;
        }
    }

    async fn scan_error_auto_continue_sessions(
        &self,
        settings: &SettingsManager,
        api_presets: &[auth_core::StoredApiPreset],
    ) {
        // Unattended-window gate: when an active window is configured, only run
        // immediate retries while the current local time falls inside that
        // window. Reset-time tasks are still collected below so a quota reset
        // captured outside the window does not fall back to blind retries.
        let active_window = settings.terminal_auto_continue_active_window();
        let active_window_allows_immediate =
            active_window.is_empty() || now_within_active_window(&active_window);
        let immediate_auto_continue_enabled = settings.terminal_auto_continue_on_error();

        let error_line_limit = settings.terminal_error_match_line_limit();
        let error_keywords = settings.terminal_error_keywords();
        let keyword_actions = settings.terminal_error_keyword_actions();
        let auto_continue_time_patterns = settings.terminal_auto_continue_time_patterns();
        let interval_seconds = settings.terminal_auto_continue_interval_seconds();
        let auto_continue_respect_manual_interrupt =
            settings.terminal_auto_continue_respect_manual_interrupt();
        let auto_continue_backoff_factor = settings.terminal_auto_continue_backoff_factor();
        let auto_continue_backoff_max_minutes =
            settings.terminal_auto_continue_backoff_max_minutes();
        self.update_auto_continue_runtime_policy(
            interval_seconds,
            auto_continue_respect_manual_interrupt,
            auto_continue_backoff_factor,
            auto_continue_backoff_max_minutes,
        );
        let now = current_timestamp_millis();
        let cleanup_dirty = self.cleanup_sessions(None);
        let (session_ids, cleanup_dirty) = {
            let state = crate::lock_or_recover!(self.state.read());
            let mut session_ids: Vec<String> = state.sessions_by_id.keys().cloned().collect();
            sort_session_ids_by_recent_activity(&state, &mut session_ids);
            (session_ids, cleanup_dirty)
        };
        let (mut sessions, updated) = self.collect_session_infos_without_manager_lock(
            &self.env_snapshot.workspace_root,
            &self.env_snapshot.display_root,
            session_ids,
            error_line_limit,
            &error_keywords,
            &auto_continue_time_patterns,
            auto_continue_respect_manual_interrupt,
        );
        self.backfill_zhipu_quota_reset_times_for_sessions(&mut sessions);
        let quota_preflight_blocked = self
            .backfill_glm_model_quota_reset_times(&mut sessions, api_presets)
            .await;
        let auto_continue_schedules = collect_terminal_auto_continue_schedules(&sessions, now);
        let active_error_signatures = self.active_session_error_signatures(&sessions);
        // 取消标记只在该会话的同一错误签名仍可见时保留；顺手清掉业务字段
        // 已空且锁无外部持有的条目。
        crate::lock_or_recover!(self.per_session_runtime.lock()).retain(|session_id, entry| {
            let active = active_error_signatures.get(session_id).map(String::as_str);
            entry
                .canceled_signatures
                .retain(|signature| Some(signature.as_str()) == active);
            !entry.prunable()
        });
        let dirty = cleanup_dirty || updated;
        let should_notify = dirty && should_notify_session_list_sync(cleanup_dirty, updated);
        if dirty {
            self.persist_state();
        }

        if should_notify {
            self.notify_session_list_changed("synced", "");
        }
        let canceled_signatures: HashMap<String, HashSet<String>> =
            crate::lock_or_recover!(self.per_session_runtime.lock())
                .iter()
                .filter(|(_, entry)| !entry.canceled_signatures.is_empty())
                .map(|(session_id, entry)| (session_id.clone(), entry.canceled_signatures.clone()))
                .collect();
        let auto_continue_schedules = auto_continue_schedules
            .into_iter()
            .filter(|(session_id, schedule)| {
                !canceled_signatures
                    .get(session_id)
                    .is_some_and(|signatures| signatures.contains(&schedule.signature))
            })
            .collect();
        self.schedule_auto_continue_tasks(
            auto_continue_schedules,
            error_line_limit,
            &error_keywords,
            &auto_continue_time_patterns,
            interval_seconds,
            auto_continue_respect_manual_interrupt,
        );
        self.prune_error_auto_continue_records(&sessions);
        self.prune_auto_continue_last_sent_at(&sessions);

        if !immediate_auto_continue_enabled || !active_window_allows_immediate {
            let mut runtime = crate::lock_or_recover!(self.per_session_runtime.lock());
            for entry in runtime.values_mut() {
                entry.error_auto_continue = None;
            }
            runtime.retain(|_, entry| !entry.prunable());
            return;
        }

        for session in sessions {
            if quota_preflight_blocked.contains(&session.id) {
                continue;
            }
            self.maybe_send_error_auto_continue(&session, interval_seconds, now, &keyword_actions);
        }
    }

    async fn backfill_glm_model_quota_reset_times(
        &self,
        sessions: &mut [TerminalSessionInfo],
        api_presets: &[auth_core::StoredApiPreset],
    ) -> HashSet<String> {
        let agent_detector = TerminalAgentDetector::new();
        let glm_session_ids = sessions
            .iter()
            .filter(|session| terminal_quota_retry_needs_glm_plan(session))
            .filter_map(|session| {
                if session.model_family.eq_ignore_ascii_case("glm") {
                    return Some(session.id.clone());
                }
                let output = capture_tmux_recent_pane_snapshot(&session.id).ok()?;
                let live_api_base_url = agent_detector.api_preset_id(&session.id).and_then(|id| {
                    api_presets
                        .iter()
                        .find(|preset| preset.id == id)
                        .map(|preset| preset.base_url.as_str())
                });
                terminal_quota_preflight_uses_zhipu_provider(
                    &session.codex_api_base_url,
                    live_api_base_url,
                    &String::from_utf8_lossy(&output),
                )
                .then(|| session.id.clone())
            })
            .collect::<HashSet<_>>();
        if glm_session_ids.is_empty() {
            return HashSet::new();
        }

        let now_millis = current_timestamp_millis();
        let cached_fresh = self
            .quota_reset_cache
            .get_latest_zhipu_reset()
            .as_deref()
            .and_then(|reset| terminal_auto_continue_due_millis(reset, now_millis))
            .is_some();

        let preflight_status = if cached_fresh {
            None
        } else {
            let app_dir = self
                .state_file
                .parent()
                .unwrap_or(std::path::Path::new("."));
            let quota_manager = crate::quota::QuotaConfigManager::load(app_dir);
            let quota_config = quota_manager.snapshot().await;
            Some(
                crate::quota::refresh_zhipu_quota_reset_cache_with_quota_config(
                    api_presets,
                    &self.quota_reset_cache,
                    Some(&quota_config),
                )
                .await,
            )
        };
        if let Some(crate::quota::ZhipuQuotaPreflightStatus::Unavailable) = preflight_status {
            return HashSet::new();
        }

        let reset_at = self.quota_reset_cache.get_latest_zhipu_reset();
        for session in sessions.iter_mut() {
            if glm_session_ids.contains(&session.id) {
                let session_reset = self
                    .zhipu_quota_reset_for_session(
                        &session.codex_api_preset_name,
                        &session.codex_api_base_url,
                    )
                    .or_else(|| reset_at.clone());
                if let Some(reset) = session_reset {
                    session.activity_error_auto_continue_at = Some(reset);
                }
            }
        }
        if reset_at.is_some()
            || matches!(preflight_status, Some(crate::quota::ZhipuQuotaPreflightStatus::Available))
        {
            HashSet::new()
        } else {
            glm_session_ids
        }
    }

    fn prune_error_auto_continue_records(&self, sessions: &[TerminalSessionInfo]) {
        let active_error_keys = sessions
            .iter()
            .filter(|session| terminal_session_is_error_state(session))
            .filter_map(|session| {
                let signature = session.activity_error_signature.as_deref()?.trim();
                (!signature.is_empty()).then(|| (session.id.clone(), signature.to_string()))
            })
            .collect::<HashMap<_, _>>();
        // Keep a compact marker while the same recovery is still in flight or
        // the original error remains visible. Otherwise a compact-induced
        // "working" snapshot would erase the marker before the error reappears.
        let compact_in_progress_ids = sessions
            .iter()
            .filter(|session| {
                session.busy
                    || matches!(
                        session.activity_state.as_str(),
                        "working" | "recent_output" | "building" | "agent" | "orphaned_agent"
                    )
            })
            .map(|session| session.id.clone())
            .collect::<HashSet<_>>();
        let active_error_ids = sessions
            .iter()
            .filter(|session| terminal_session_is_error_state(session))
            .map(|session| session.id.clone())
            .collect::<HashSet<_>>();
        let active_session_ids = sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<HashSet<_>>();

        // 合并后两张表的 retain 落在同一 map 锁内逐条清理，空条目一并移除。
        crate::lock_or_recover!(self.per_session_runtime.lock()).retain(|session_id, entry| {
            entry.auto_compact_signature =
                entry.auto_compact_signature.take().filter(|signature| {
                    match active_error_keys.get(session_id) {
                        Some(current) => current == signature,
                        None => compact_in_progress_ids.contains(session_id),
                    }
                });
            if !(active_session_ids.contains(session_id) && active_error_ids.contains(session_id)) {
                entry.error_auto_continue = None;
            }
            !entry.prunable()
        });
    }

    fn prune_auto_continue_last_sent_at(&self, sessions: &[TerminalSessionInfo]) {
        let active_session_ids = sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<HashSet<_>>();
        let now = current_timestamp_millis();
        let max_retention_millis = self
            .auto_continue_backoff_max_millis
            .load(Ordering::Relaxed)
            .saturating_add(
                self.auto_continue_interval_seconds
                    .load(Ordering::Relaxed)
                    .saturating_mul(1000),
            );
        crate::lock_or_recover!(self.per_session_runtime.lock()).retain(|session_id, entry| {
            if entry.auto_continue_last_sent_at.is_some_and(|sent_at| {
                !active_session_ids.contains(session_id)
                    || now.saturating_sub(sent_at) > max_retention_millis
            }) {
                entry.auto_continue_last_sent_at = None;
            }
            !entry.prunable()
        });
    }

    fn update_auto_continue_runtime_policy(
        &self,
        interval_seconds: u32,
        respect_manual_interrupt: bool,
        backoff_factor: f64,
        backoff_max_minutes: u32,
    ) {
        self.auto_continue_interval_seconds
            .store(u64::from(interval_seconds.max(1)), Ordering::Relaxed);
        self.auto_continue_backoff_factor
            .store((backoff_factor.clamp(1.0, 10.0) * 1000.0) as u64, Ordering::Relaxed);
        self.auto_continue_backoff_max_millis
            .store(u64::from(backoff_max_minutes.clamp(1, 1440)) * 60 * 1000, Ordering::Relaxed);
        self.auto_continue_respect_manual_interrupt
            .store(respect_manual_interrupt, Ordering::Relaxed);
    }

    fn maybe_send_error_auto_continue(
        &self,
        session: &TerminalSessionInfo,
        interval_seconds: u32,
        now: u64,
        keyword_actions: &[TerminalErrorKeywordAction],
    ) {
        if !terminal_session_is_error_state(session) || session.activity_error_input_queued {
            return;
        }
        let Some(error_key) = terminal_session_error_continue_key(session) else {
            return;
        };
        let reset_at = session
            .activity_error_auto_continue_at
            .as_deref()
            .unwrap_or("")
            .trim()
            .to_string();
        if !reset_at.is_empty() {
            self.remember_error_auto_continue_record(&session.id, error_key, now, reset_at);
            return;
        }

        let existing = crate::lock_or_recover!(self.per_session_runtime.lock())
            .get(&session.id)
            .and_then(|entry| entry.error_auto_continue.clone());

        if session.activity_error_continue_sent
            && existing
                .as_ref()
                .is_none_or(|record| record.key != error_key)
        {
            // A continue was already sent for a previous error key; the error
            // has now changed to a new one. Reset the backoff counter to 1.
            self.remember_error_auto_continue_record(
                &session.id,
                error_key.clone(),
                now,
                String::new(),
            );
            crate::lock_or_recover!(self.per_session_runtime.lock())
                .entry(session.id.clone())
                .or_default()
                .auto_continue_last_sent_at
                .get_or_insert(now);
            return;
        }

        // Backoff: increment the consecutive attempt counter for the same
        // error key and grow the effective interval so repeated identical
        // failures retry progressively more slowly.
        let same_key = existing
            .as_ref()
            .is_some_and(|record| record.key == error_key);
        let consecutive_attempts = if same_key {
            existing
                .as_ref()
                .map(|record| record.consecutive_attempts.saturating_add(1))
                .unwrap_or(1)
        } else {
            1
        };
        self.remember_error_auto_continue_record_with_attempts(
            &session.id,
            error_key.clone(),
            now,
            String::new(),
            consecutive_attempts,
        );
        let base_interval_millis = u64::from(interval_seconds.max(1)) * 1000;
        let backoff_factor =
            self.auto_continue_backoff_factor.load(Ordering::Relaxed) as f64 / 1000.0;
        let backoff_max_millis = self
            .auto_continue_backoff_max_millis
            .load(Ordering::Relaxed);
        let effective_interval_millis = auto_continue_backoff_interval_millis(
            base_interval_millis,
            consecutive_attempts,
            backoff_factor,
            backoff_max_millis,
        );
        // Look up the configured action for the matched keyword.
        let matched_keyword = session
            .activity_error_keyword
            .as_deref()
            .unwrap_or("")
            .trim();
        let action = if terminal_error_keyword_is_payload_too_large(matched_keyword) {
            settings_core::TERMINAL_ERROR_KEYWORD_ACTION_COMPACT_THEN_CONTINUE.to_string()
        } else {
            resolve_terminal_error_keyword_action(matched_keyword, keyword_actions)
        };
        let outcome = match action.as_str() {
            "compact_then_continue" if session.activity_error_continue_sent => {
                // The same error is still visible after a retry. Do not race
                // the in-flight turn with another /compact command.
                Ok(TerminalAutoContinueSendOutcome::NotEligible)
            }
            "compact_then_continue" => {
                let signature = session
                    .activity_error_signature
                    .as_deref()
                    .unwrap_or_default()
                    .trim();
                if signature.is_empty() {
                    return;
                }
                self.send_session_auto_compact(&session.id, signature, effective_interval_millis)
            }
            "mark_only" => Ok(TerminalAutoContinueSendOutcome::NotEligible),
            _ => self.send_session_auto_continue(&session.id, effective_interval_millis),
        };
        match outcome {
            Ok(TerminalAutoContinueSendOutcome::CompactSent) => {
                let _ = self.event_sender.send(TerminalManagerEvent::Toast {
                    session_id: session.id.clone(),
                    message: format!(
                        "检测到终端“{}”上下文窗口已满，已发送 /compact。",
                        session.name
                    ),
                    tone: "ok".to_string(),
                });
                self.notify_session_list_changed("auto_continue", &session.id);
            }
            Ok(TerminalAutoContinueSendOutcome::Sent) => {
                let keyword = session
                    .activity_error_keyword
                    .as_deref()
                    .unwrap_or("错误")
                    .trim();
                let message = if keyword.is_empty() {
                    format!("检测到终端“{}”错误，已发送“继续”。", session.name)
                } else {
                    format!("检测到终端“{}”错误“{}”，已发送“继续”。", session.name, keyword)
                };
                let _ = self.event_sender.send(TerminalManagerEvent::Toast {
                    session_id: session.id.clone(),
                    message,
                    tone: "ok".to_string(),
                });
                self.notify_session_list_changed("auto_continue", &session.id);
            }
            Ok(TerminalAutoContinueSendOutcome::Cooldown { .. })
            | Ok(TerminalAutoContinueSendOutcome::NotEligible) => {}
            Err(error) => {
                self.forget_error_auto_continue_record_if_matches(&session.id, &error_key);
                warn!("terminal error auto-continue failed for {}: {error}", session.id);
            }
        }
    }

    fn remember_error_auto_continue_record(
        &self,
        session_id: &str,
        key: String,
        sent_at_millis: u64,
        reset_at: String,
    ) {
        // Preserve any existing backoff counter when only refreshing the
        // reset_at timestamp; a quota reset time does not clear the error.
        // 读取与写回在同一临界区完成（合并前是两把锁的 check-then-act）。
        let mut runtime = crate::lock_or_recover!(self.per_session_runtime.lock());
        let entry = runtime.entry(session_id.to_string()).or_default();
        let consecutive_attempts = entry
            .error_auto_continue
            .as_ref()
            .filter(|record| record.key == key)
            .map(|record| record.consecutive_attempts)
            .unwrap_or(0);
        entry.error_auto_continue = Some(TerminalErrorAutoContinueRecord {
            key,
            sent_at_millis,
            reset_at,
            consecutive_attempts,
        });
    }

    fn remember_error_auto_continue_record_with_attempts(
        &self,
        session_id: &str,
        key: String,
        sent_at_millis: u64,
        reset_at: String,
        consecutive_attempts: u32,
    ) {
        crate::lock_or_recover!(self.per_session_runtime.lock())
            .entry(session_id.to_string())
            .or_default()
            .error_auto_continue = Some(TerminalErrorAutoContinueRecord {
            key,
            sent_at_millis,
            reset_at,
            consecutive_attempts,
        });
    }

    fn forget_error_auto_continue_record_if_matches(&self, session_id: &str, key: &str) {
        let mut runtime = crate::lock_or_recover!(self.per_session_runtime.lock());
        if let Some(entry) = runtime.get_mut(session_id)
            && entry
                .error_auto_continue
                .as_ref()
                .is_some_and(|record| record.key == key)
        {
            entry.error_auto_continue = None;
            if entry.prunable() {
                runtime.remove(session_id);
            }
        }
    }

    fn persist_auto_continue_tasks_locked(
        &self,
        tasks: &HashMap<String, TerminalAutoContinueTask>,
    ) {
        let registry = StoredTerminalAutoContinueScheduleRegistry {
            tasks: tasks.values().cloned().collect(),
        };
        if let Err(error) =
            persist_terminal_auto_continue_registry(&self.auto_continue_file, &registry)
        {
            warn!(
                "persist terminal auto-continue registry failed {}: {error}",
                self.auto_continue_file.display()
            );
        }
    }

    fn notify_session_list_changed(
        &self,
        action: impl Into<String>,
        session_id: impl Into<String>,
    ) {
        let _ = self
            .event_sender
            .send(TerminalManagerEvent::SessionListChanged {
                action: action.into(),
                session_id: session_id.into(),
            });
    }
}

#[cfg(test)]
pub(super) fn rename_session_for_test(
    state: &mut TerminalState,
    session_id: &str,
    next_name: &str,
) -> Result<StoredTerminalSession> {
    let name = normalize_session_name(next_name)?;
    let stored = state
        .sessions_by_id
        .get(session_id)
        .cloned()
        .with_context(|| format!("会话 `{session_id}` 不存在"))?;
    ensure_unique_session_name_locked(state, &name, Some(session_id))?;
    let mut previous_names = stored.previous_names.clone();
    if stored.name != name && !previous_names.iter().any(|old| old == &stored.name) {
        previous_names.push(stored.name.clone());
    }
    let updated = StoredTerminalSession {
        name,
        previous_names,
        manually_renamed: true,
        ..stored
    };
    state
        .sessions_by_id
        .insert(session_id.to_string(), updated.clone());
    state.previous_terminal_names = state
        .sessions_by_id
        .values()
        .flat_map(|session| {
            session
                .previous_names
                .iter()
                .map(|previous_name| (previous_name.clone(), session.id.clone()))
        })
        .collect();
    Ok(updated)
}

#[cfg(test)]
pub(super) fn resolve_session_target_for_test(
    state: &mut TerminalState,
    target: &str,
    path: Option<&Path>,
) -> Result<(String, String)> {
    let direct = state
        .sessions_by_id
        .get(target)
        .filter(|session| path.is_none_or(|path| session.path == path))
        .map(|session| (session.id.clone(), session.name.clone()));
    if let Some(match_by_id) = direct {
        return Ok(match_by_id);
    }

    let matches = state
        .sessions_by_id
        .values()
        .filter(|session| session.name == target)
        .filter(|session| path.is_none_or(|path| session.path == path))
        .map(|session| (session.id.clone(), session.name.clone()))
        .collect::<Vec<_>>();
    if matches.is_empty() {
        if let Some(session_id) = state.previous_terminal_names.get(target).cloned() {
            if let Some(session) = state.sessions_by_id.get(&session_id) {
                if path.is_none_or(|path| session.path == path) {
                    return Ok((session.id.clone(), session.name.clone()));
                }
            }
        }
    }
    match matches.as_slice() {
        [single] => Ok(single.clone()),
        [] => anyhow::bail!("terminal session not found"),
        _ => anyhow::bail!("terminal name `{target}` is ambiguous; pass session_id or path"),
    }
}

/// Reap legacy tmux attach clients that outlived their `TerminalSession`
/// handle. The filter is intentionally narrow: only direct children of this
/// webClx process, in zombie state, whose command name is exactly `tmux: client`
/// are eligible. Other child processes remain owned by their normal wait path.
#[cfg(target_os = "linux")]
fn reap_direct_tmux_client_zombies() {
    let parent_pid = std::process::id();
    let children_path = format!("/proc/{parent_pid}/task/{parent_pid}/children");
    let Ok(children) = fs::read_to_string(children_path) else {
        return;
    };

    for pid in children
        .split_whitespace()
        .filter_map(|value| value.parse::<u32>().ok())
    {
        let stat_path = format!("/proc/{pid}/stat");
        let Ok(stat) = fs::read_to_string(stat_path) else {
            continue;
        };
        let Some((comm, state, ppid)) = parse_proc_stat_identity(&stat) else {
            continue;
        };
        if ppid != parent_pid || state != 'Z' || comm != "tmux: client" {
            continue;
        }

        let mut status = 0;
        let result = unsafe { libc::waitpid(pid as libc::pid_t, &mut status, libc::WNOHANG) };
        if result == pid as libc::pid_t {
            info!(pid, "reaped legacy tmux client zombie");
        } else if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ECHILD) {
                warn!(pid, "failed to reap legacy tmux client zombie: {error}");
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn reap_direct_tmux_client_zombies() {}

#[cfg(target_os = "linux")]
fn parse_proc_stat_identity(stat: &str) -> Option<(String, char, u32)> {
    let open = stat.find('(')?;
    let close = stat.rfind(") ")?;
    if close <= open {
        return None;
    }
    let comm = stat[open + 1..close].to_string();
    let mut fields = stat[close + 2..].split_whitespace();
    let state = fields.next()?.chars().next()?;
    let ppid = fields.next()?.parse().ok()?;
    Some((comm, state, ppid))
}

pub(super) fn parse_codex_model_provider(snapshot: &[u8]) -> Option<String> {
    String::from_utf8_lossy(snapshot)
        .lines()
        .filter_map(|line| {
            let line = line.trim().trim_matches('│').trim();
            let value = line.strip_prefix("Model provider:")?.trim();
            let value =
                value.trim_matches(|character: char| character == '│' || character.is_whitespace());
            (!value.is_empty()).then(|| value.to_string())
        })
        .last()
}

fn codex_model_provider_line_count(snapshot: &[u8]) -> usize {
    String::from_utf8_lossy(snapshot)
        .lines()
        .filter(|line| line.contains("Model provider:"))
        .count()
}

pub(super) fn split_codex_model_provider(provider: &str) -> (String, String) {
    let provider = provider.trim();
    for separator in [" - https://", " - http://"] {
        if let Some(index) = provider.find(separator) {
            let preset_name = provider[..index].trim().to_string();
            let base_url = provider[index + 3..].trim().to_string();
            return (preset_name, base_url);
        }
    }
    (provider.to_string(), String::new())
}

pub(super) fn prepare_terminal_message_body(data: &str, bracketed_paste: bool) -> String {
    if !bracketed_paste {
        return data.to_string();
    }
    let prepared = data.replace("\r\n", "\r").replace('\n', "\r");
    format!("\u{1b}[200~{prepared}\u{1b}[201~")
}

pub(super) fn terminal_message_paste_settle_delay(data: &str, bracketed_paste: bool) -> Duration {
    if !bracketed_paste || data.is_empty() {
        return Duration::from_millis(120);
    }
    let length_delay_ms = (data.len() as u64 / 4).min(1_400);
    Duration::from_millis(600 + length_delay_ms)
}

pub(super) fn terminal_message_delivery_count(
    entries: &[TerminalInputHistoryEntry],
    delivery_id: &str,
) -> usize {
    if delivery_id.is_empty() {
        return 0;
    }
    entries
        .iter()
        .filter(|entry| entry.text.contains(delivery_id))
        .count()
}

pub(super) fn terminal_message_delivery_confirmed(
    entries: Option<&[TerminalInputHistoryEntry]>,
    delivery_id: &str,
    delivery_baseline: usize,
) -> bool {
    entries.is_some_and(|entries| {
        terminal_message_delivery_count(entries, delivery_id) > delivery_baseline
    })
}

fn auto_continue_crontab_has_stale_due_markers(
    session_id: &str,
    schedule: &TerminalAutoContinueSchedule,
) -> bool {
    let Ok(current) = current_crontab() else {
        return true;
    };
    let due_marker_prefix = terminal_auto_continue_due_marker_prefix(session_id);
    let expected_due_marker = terminal_auto_continue_due_marker(
        session_id,
        &schedule.signature,
        schedule.due_at_millis / 1000,
    );
    let due_lines = current
        .lines()
        .filter(|line| line.contains(&due_marker_prefix))
        .collect::<Vec<_>>();
    due_lines.len() != 1
        || due_lines
            .iter()
            .any(|line| line.trim() != expected_due_marker)
}

fn load_terminal_auto_continue_tasks(
    auto_continue_file: &Path,
) -> HashMap<String, TerminalAutoContinueTask> {
    let registry = match load_terminal_auto_continue_registry(auto_continue_file) {
        Ok(registry) => registry,
        Err(error) => {
            warn!(
                "load terminal auto-continue registry failed {}, fallback to empty state: {error}",
                auto_continue_file.display()
            );
            return HashMap::new();
        }
    };
    registry
        .tasks
        .into_iter()
        .filter(|task| !task.session_id.trim().is_empty())
        .filter(|task| !task.schedule.signature.trim().is_empty())
        .map(|mut task| {
            if task.backend_due_at_millis == 0 {
                task.backend_due_at_millis = task.schedule.due_at_millis;
            }
            (task.session_id.clone(), task)
        })
        .collect()
}

pub(in crate::terminal) fn terminal_pending_build_request_is_current(
    request: &TerminalPendingBuildRequest,
    now_millis: u64,
) -> bool {
    !request.request_id.trim().is_empty()
        && !request.session_id.trim().is_empty()
        && (now_millis < request.queued_at_millis
            || now_millis.saturating_sub(request.queued_at_millis)
                <= TERMINAL_PENDING_BUILD_MAX_AGE_MS)
}

pub(in crate::terminal) fn load_terminal_pending_build_requests(
    pending_build_file: &Path,
) -> HashMap<String, TerminalPendingBuildRequest> {
    if !pending_build_file.exists() {
        return HashMap::new();
    }
    let registry = match fs::read(pending_build_file)
        .with_context(|| format!("cannot read {}", pending_build_file.display()))
        .and_then(|content| {
            serde_json::from_slice::<StoredTerminalPendingBuildRegistry>(&content)
                .with_context(|| format!("cannot parse {}", pending_build_file.display()))
        }) {
        Ok(registry) => registry,
        Err(error) => {
            warn!(
                "load terminal pending build registry failed {}, fallback to empty state: {error}",
                pending_build_file.display()
            );
            return HashMap::new();
        }
    };
    let now = current_timestamp_millis();
    registry
        .requests
        .into_iter()
        .filter(|request| terminal_pending_build_request_is_current(request, now))
        .map(|request| (request.request_id.clone(), request))
        .collect()
}

pub(in crate::terminal) fn persist_terminal_pending_build_registry(
    pending_build_file: &Path,
    registry: &StoredTerminalPendingBuildRegistry,
) -> Result<()> {
    if let Some(parent) = pending_build_file.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_vec_pretty(registry)
        .context("cannot encode terminal pending build registry")?;
    fs::write(pending_build_file, content)
        .with_context(|| format!("cannot write {}", pending_build_file.display()))?;
    Ok(())
}

fn load_terminal_auto_continue_registry(
    auto_continue_file: &Path,
) -> Result<StoredTerminalAutoContinueScheduleRegistry> {
    if !auto_continue_file.exists() {
        return Ok(StoredTerminalAutoContinueScheduleRegistry { tasks: Vec::new() });
    }
    let content = fs::read(auto_continue_file).with_context(|| {
        format!("cannot read terminal auto-continue registry {}", auto_continue_file.display())
    })?;
    let registry = serde_json::from_slice(&content).with_context(|| {
        format!("cannot parse terminal auto-continue registry {}", auto_continue_file.display())
    })?;
    Ok(registry)
}

fn persist_terminal_auto_continue_registry(
    auto_continue_file: &Path,
    registry: &StoredTerminalAutoContinueScheduleRegistry,
) -> Result<()> {
    if let Some(parent) = auto_continue_file.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_vec_pretty(registry)
        .context("cannot encode terminal auto-continue registry")?;
    fs::write(auto_continue_file, content).with_context(|| {
        format!("cannot write terminal auto-continue registry {}", auto_continue_file.display())
    })
}

fn auto_continue_task_matches(
    current: Option<&TerminalAutoContinueTask>,
    schedule: &TerminalAutoContinueSchedule,
    error_line_limit: u32,
    auto_continue_interval_seconds: u32,
    auto_continue_respect_manual_interrupt: bool,
    error_keywords: &[String],
    auto_continue_time_patterns: &[String],
) -> bool {
    current.is_some_and(|current| {
        current.schedule.signature == schedule.signature
            && current.schedule.keyword == schedule.keyword
            && current.schedule.reset_at == schedule.reset_at
            && current.error_line_limit == error_line_limit
            && current.auto_continue_interval_seconds == auto_continue_interval_seconds
            && current.auto_continue_respect_manual_interrupt
                == auto_continue_respect_manual_interrupt
            && current.error_keywords == error_keywords
            && current.auto_continue_time_patterns == auto_continue_time_patterns
    })
}

fn terminal_auto_continue_marker_parts(marker: &str) -> Option<(String, String)> {
    let body = marker
        .trim()
        .strip_prefix(TERMINAL_AUTO_CONTINUE_CRON_MARKER_PREFIX)?
        .strip_prefix(':')?;
    let (session_id, signature) = body.split_once(':')?;
    if session_id.trim().is_empty() || signature.trim().is_empty() {
        return None;
    }
    Some((session_id.to_string(), signature.to_string()))
}

/// Resolve the configured action for a matched error keyword. Matches case-
/// insensitively after whitespace-normalization; keywords without an explicit
/// rule default to "continue".
fn resolve_terminal_error_keyword_action(
    keyword: &str,
    actions: &[TerminalErrorKeywordAction],
) -> String {
    let normalized = keyword
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
    actions
        .iter()
        .find(|action| {
            action
                .keyword
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase()
                == normalized
        })
        .map(|action| action.action.clone())
        .unwrap_or_else(settings_core::default_terminal_error_keyword_action)
}

fn terminal_continue_command_text(text: &str) -> bool {
    normalize_scheduled_input_text(text).trim() == TERMINAL_CONTINUE_COMMAND
}
/// Return false when a live queue banner makes a retry unsafe.
pub(super) fn submit_message_enter_unless_queued(
    snapshot: &str,
    mut enter: impl FnMut() -> Result<()>,
) -> Result<bool> {
    if snapshot
        .lines()
        .any(error_detection::is_terminal_queued_input_line)
    {
        return Ok(false);
    }
    enter()?;
    Ok(true)
}

/// Build the byte sequence written to the PTY for a scheduled paste.
///
/// Must stay byte-for-byte aligned with the frontend `prepareTerminalPasteText`
/// + `wrapBracketedTerminalPaste` path used by an immediate paste-and-send.
/// Codex/Claude are raw-mode TUIs: inside a bracketed paste region they treat
/// CR as the line separator, while a lone LF is left in the input buffer and
/// never advances the cursor / submits. The previous implementation normalized
/// internal line breaks to LF, so a scheduled multi-line paste produced only a
/// newline and was never submitted (only the trailing CR submit key worked for
/// single-line input). Internal breaks are now collapsed to CR, matching the
/// frontend, and the submit CR is emitted after the bracket end marker.
/// Submission is separate from body delivery: retries can never paste twice.
/// Native queue acceptance or rollout evidence confirms delivery, never a draft.
pub(super) fn submit_scheduled_message(
    mut queued: impl FnMut() -> Result<bool>,
    mut enter: impl FnMut() -> Result<()>,
    mut confirmed: impl FnMut() -> Result<bool>,
    mut wait: impl FnMut(),
) -> Result<&'static str> {
    for attempt in 0..3 {
        // The body was just pasted. An older queued message cannot prove this
        // new body was submitted; only retries may be suppressed by its banner.
        if confirmed()? || (attempt > 0 && queued()?) {
            return Ok("confirmed");
        }
        enter()?;
        for _ in 0..20 {
            wait();
            if queued()? || confirmed()? {
                return Ok("confirmed");
            }
        }
    }
    Ok("pending_confirmation")
}

pub(super) fn scheduled_input_body(task: &TerminalScheduledInputTask) -> String {
    let is_multiline = task.text.contains('\n') || task.text.contains('\r');
    // Mirror frontend prepareTerminalPasteText: CRLF, lone CR and lone LF
    // all collapse to a single CR so the bracketed region uses CR line breaks.
    let prepared = task.text.replace("\r\n", "\r").replace('\n', "\r");
    if is_multiline {
        format!("\u{1b}[200~{prepared}\u{1b}[201~")
    } else {
        prepared
    }
}

fn load_terminal_state(
    state_file: &Path,
    protected_session_ids: &HashSet<String>,
) -> (TerminalState, u64, bool) {
    let registry = match load_terminal_registry(state_file) {
        Ok(registry) => registry,
        Err(error) => {
            warn!(
                "load terminal session registry failed {}, fallback to empty state: {error}",
                state_file.display()
            );
            return (TerminalState::default(), default_next_ordinal(), false);
        }
    };

    let mut state = TerminalState::default();
    state.input_histories = registry
        .input_histories
        .into_iter()
        .map(|(session_id, entries)| {
            (
                session_id,
                TerminalInputHistoryCapture {
                    buffer: String::new(),
                    entries: clamp_input_history_entries(entries),
                    paste_active: false,
                },
            )
        })
        .collect();
    state.output_observations = registry.output_observations;
    let mut next_ordinal = registry.next_ordinal.max(default_next_ordinal());
    let mut dirty = false;

    for mut session in registry.sessions {
        let normalized_user = runtime_paths::normalize_user_name(&session.user_name)
            .unwrap_or_else(|_| default_terminal_user_name());
        if session.user_name != normalized_user {
            session.user_name = normalized_user;
            dirty = true;
        }

        if session.name.trim().is_empty() {
            session.manually_renamed = false;
            dirty = true;
        }

        if session.created_at == 0 {
            session.created_at = session_sort_ordinal(&session.id);
            dirty = true;
        }

        if session.last_opened_at == 0 || session.last_opened_at < session.created_at {
            session.last_opened_at = session.created_at;
            dirty = true;
        }

        let normalized_title = normalize_session_title(&session.title).unwrap_or_default();
        if session.title != normalized_title {
            session.title = normalized_title;
            dirty = true;
        }

        if normalize_stored_terminal_group_color(&mut session) {
            dirty = true;
        }

        if !stored_session_survives_process_restart(&session.id)
            && !protected_session_ids.contains(&session.id)
        {
            dirty = true;
            continue;
        }

        next_ordinal = next_ordinal.max(next_ordinal_for_session(&session.id));
        state
            .sessions_by_path
            .entry(session.path.clone())
            .or_default()
            .push(session.id.clone());
        state.sessions_by_id.insert(session.id.clone(), session);
    }

    // Early builds derived all default colors from the same narrow RGB bands,
    // which made distinct groups look identical after a restart. Replace only
    // colors that still exactly match that legacy generated default; explicit
    // user colors are never rewritten.
    let mut legacy_default_tokens = std::collections::HashSet::new();
    let mut group_members: std::collections::HashMap<&str, Vec<&str>> =
        std::collections::HashMap::new();
    for session in state.sessions_by_id.values() {
        if session.group_token.is_empty() {
            continue;
        }
        group_members
            .entry(session.group_token.as_str())
            .or_default()
            .push(session.group_color.as_str());
    }
    for (token, colors) in group_members {
        let legacy = legacy_default_terminal_group_color(token);
        if colors.len() > 1 && colors.iter().all(|color| *color == legacy) {
            legacy_default_tokens.insert(token.to_string());
        }
    }
    if !legacy_default_tokens.is_empty() {
        for session in state.sessions_by_id.values_mut() {
            if legacy_default_tokens.contains(session.group_token.as_str()) {
                session.group_color = default_terminal_group_color(&session.group_token);
            }
        }
        dirty = true;
    }

    let before_history_count = state.input_histories.len();
    state
        .input_histories
        .retain(|session_id, _| state.sessions_by_id.contains_key(session_id));
    dirty |= state.input_histories.len() != before_history_count;
    let before_output_observation_count = state.output_observations.len();
    state
        .output_observations
        .retain(|session_id, _| state.sessions_by_id.contains_key(session_id));
    dirty |= state.output_observations.len() != before_output_observation_count;
    state.previous_terminal_names = state
        .sessions_by_id
        .values()
        .flat_map(|session| {
            session
                .previous_names
                .iter()
                .map(|previous_name| (previous_name.clone(), session.id.clone()))
        })
        .collect();

    dirty |= refresh_auto_session_names_locked(&mut state);

    (state, next_ordinal, dirty)
}

fn collect_stored_sessions(state: &TerminalState) -> Vec<StoredTerminalSession> {
    let mut sessions = Vec::new();

    for session_id in sorted_session_ids(state) {
        if let Some(session) = state.sessions_by_id.get(&session_id) {
            sessions.push(session.clone());
        }
    }

    sessions
}

fn collect_stored_input_histories(
    state: &TerminalState,
) -> HashMap<String, Vec<TerminalInputHistoryEntry>> {
    let mut histories = HashMap::new();

    for session_id in sorted_session_ids(state) {
        let Some(history) = state.input_histories.get(&session_id) else {
            continue;
        };
        if history.entries.is_empty() {
            continue;
        }
        histories.insert(session_id, history.entries.clone());
    }

    histories
}

fn collect_stored_output_observations(
    state: &TerminalState,
) -> HashMap<String, TerminalOutputObservation> {
    state
        .output_observations
        .iter()
        .filter(|(session_id, _)| state.sessions_by_id.contains_key(*session_id))
        .map(|(session_id, observation)| (session_id.clone(), observation.clone()))
        .collect()
}

pub(super) fn clamp_input_history_entries(
    mut entries: Vec<TerminalInputHistoryEntry>,
) -> Vec<TerminalInputHistoryEntry> {
    if entries.len() > MAX_TERMINAL_INPUT_HISTORY_ENTRIES {
        entries.drain(0..entries.len() - MAX_TERMINAL_INPUT_HISTORY_ENTRIES);
    }
    entries
}

pub(super) fn filter_terminal_input_history_entries(
    mut entries: Vec<TerminalInputHistoryEntry>,
) -> Vec<TerminalInputHistoryEntry> {
    entries.retain(|entry| !is_terminal_continue_line(&entry.text));
    entries
}

/// 从 agent 真实 rollout 文件构建对话历史条目。
///
/// 返回 None 表示该终端不是 Codex/Claude agent 会话（或检测失败），
/// 调用方应回退到按键重建历史。
fn rollout_history_entries(session_id: &str) -> Option<Vec<TerminalInputHistoryEntry>> {
    let path = detect_current_session_rollout_path(session_id)?;
    let messages = parse_rollout_user_messages(&path);
    if messages.is_empty() {
        return None;
    }

    let entries = messages
        .into_iter()
        .map(|(text, created_at)| TerminalInputHistoryEntry { text, created_at })
        .collect::<Vec<_>>();
    Some(clamp_input_history_entries(entries))
}

fn record_session_input_locked(state: &mut TerminalState, session_id: &str, data: &str) -> bool {
    let history = state
        .input_histories
        .entry(session_id.to_string())
        .or_default();
    capture_terminal_input_history(history, data)
}

/// 记录终端输入到对话历史（rollout 不可用时的按键重建回退来源）。
///
/// 普通 Enter 仍按行提交；bracketed paste 块（一次粘贴 / 一次定时投递的
/// 多行消息）整体保持为一条历史，块内 `\r`/`\n` 归一为 `\n` 保留在文本中，
/// 避免一句话包含多个换行时被“对话史”识别成多段话。
pub(super) fn capture_terminal_input_history(
    history: &mut TerminalInputHistoryCapture,
    data: &str,
) -> bool {
    const PASTE_START: &[u8] = b"\x1b[200~";
    const PASTE_END: &[u8] = b"\x1b[201~";
    let bytes = data.as_bytes();
    let mut index = 0;
    let mut committed = false;

    while index < bytes.len() {
        if bytes[index] == 0x1b {
            if bytes[index..].starts_with(PASTE_START) {
                history.paste_active = true;
                index += PASTE_START.len();
                continue;
            }
            if bytes[index..].starts_with(PASTE_END) {
                history.paste_active = false;
                index += PASTE_END.len();
                continue;
            }
        }
        let byte = bytes[index];
        match byte {
            b'\r' | b'\n' => {
                if history.paste_active {
                    history.buffer.push('\n');
                    index += 1;
                    if byte == b'\r' && index < bytes.len() && bytes[index] == b'\n' {
                        index += 1;
                    }
                } else {
                    committed |= commit_terminal_input_history_line(history);
                    index += 1;
                    if byte == b'\r' && index < bytes.len() && bytes[index] == b'\n' {
                        index += 1;
                    }
                }
            }
            0x08 | 0x7f => {
                history.buffer.pop();
                index += 1;
            }
            0x1b => {
                index = skip_terminal_escape_sequence(bytes, index);
            }
            byte if byte < 0x20 => {
                index += 1;
            }
            _ => {
                let start = index;
                index += 1;
                while index < bytes.len() {
                    let b = bytes[index];
                    if b < 0x20 || b == 0x7f {
                        break;
                    }
                    index += 1;
                }
                if let Ok(chunk) = std::str::from_utf8(&bytes[start..index]) {
                    history.buffer.push_str(chunk);
                    trim_terminal_input_buffer(history);
                }
            }
        }
    }
    committed
}

fn skip_terminal_escape_sequence(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    if index >= bytes.len() {
        return index;
    }

    match bytes[index] {
        b'[' => {
            index += 1;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if (0x40..=0x7e).contains(&byte) {
                    break;
                }
            }
            index
        }
        b']' => {
            index += 1;
            while index < bytes.len() {
                let byte = bytes[index];
                index += 1;
                if byte == 0x07 {
                    break;
                }
                if byte == 0x1b && index < bytes.len() && bytes[index] == b'\\' {
                    index += 1;
                    break;
                }
            }
            index
        }
        _ => (index + 1).min(bytes.len()),
    }
}

fn commit_terminal_input_history_line(history: &mut TerminalInputHistoryCapture) -> bool {
    let text = history.buffer.trim();
    let committed = !text.is_empty() && !is_terminal_continue_line(text);
    if committed {
        history.entries.push(TerminalInputHistoryEntry {
            text: text.to_string(),
            created_at: current_timestamp_millis(),
        });
        if history.entries.len() > MAX_TERMINAL_INPUT_HISTORY_ENTRIES {
            let excess = history.entries.len() - MAX_TERMINAL_INPUT_HISTORY_ENTRIES;
            history.entries.drain(0..excess);
        }
    }
    history.buffer.clear();
    committed
}

fn trim_terminal_input_buffer(history: &mut TerminalInputHistoryCapture) {
    if history.buffer.len() > MAX_TERMINAL_INPUT_HISTORY_LINE_BYTES {
        let mut drop_bytes = history.buffer.len() - MAX_TERMINAL_INPUT_HISTORY_LINE_BYTES;
        while drop_bytes < history.buffer.len() && !history.buffer.is_char_boundary(drop_bytes) {
            drop_bytes += 1;
        }
        history.buffer.drain(..drop_bytes);
    }
}

fn refresh_auto_session_names_locked(state: &mut TerminalState) -> bool {
    let mut paths: Vec<PathBuf> = state.sessions_by_path.keys().cloned().collect();
    paths.sort();
    let mut dirty = false;

    for path in paths {
        dirty |= refresh_auto_session_names_for_path_locked(state, &path);
    }

    dirty
}

pub(super) fn refresh_auto_session_names_for_path_locked(
    state: &mut TerminalState,
    path: &Path,
) -> bool {
    let session_ids = state
        .sessions_by_path
        .get(path)
        .cloned()
        .unwrap_or_default();
    if session_ids.is_empty() {
        return false;
    }

    let mut name_claims = used_auto_session_name_claims(state, path, Some(path));

    let mut dirty = false;
    let mut live_name_updates = Vec::new();
    let mut next_index = 1;
    for session_id in session_ids {
        if let Some(session) = state.sessions_by_id.get_mut(&session_id) {
            if session.manually_renamed {
                continue;
            }

            let start_index = preferred_auto_session_start_index(session, next_index);
            let Some((next_name, used_index)) =
                next_available_auto_session_name(path, start_index, &name_claims)
            else {
                continue;
            };
            next_index = next_index.max(used_index.saturating_add(1));
            if session.name != next_name {
                session.name = next_name.clone();
                live_name_updates.push((session_id.clone(), next_name.clone()));
                dirty = true;
            }

            name_claims.claim_path_name(next_name);
        }
    }

    for (session_id, next_name) in live_name_updates {
        if let Some(session) = state.live_sessions.get(&session_id) {
            session.rename_auto(next_name);
        }
    }

    dirty
}

fn preferred_auto_session_start_index(
    session: &StoredTerminalSession,
    fallback_index: usize,
) -> usize {
    session_name_auto_indices(&session.name)
        .into_iter()
        .next()
        .unwrap_or(fallback_index)
        .max(1)
}

pub(super) fn ensure_unique_session_name_locked(
    state: &TerminalState,
    name: &str,
    excluded_session_id: Option<&str>,
) -> Result<()> {
    let candidate_auto_indices = session_name_auto_indices(name);
    let candidate_path = excluded_session_id
        .and_then(|session_id| state.sessions_by_id.get(session_id))
        .map(|session| session.path.as_path());

    for (session_id, session) in &state.sessions_by_id {
        if Some(session_id.as_str()) == excluded_session_id {
            continue;
        }

        if session.name == name {
            anyhow::bail!("名称 `{name}` 已存在，请使用其他名称。");
        }

        if candidate_path == Some(session.path.as_path())
            && !candidate_auto_indices.is_empty()
            && session_name_auto_indices(&session.name)
                .iter()
                .any(|index| candidate_auto_indices.contains(index))
        {
            anyhow::bail!("名称 `{name}` 的自动编号已被 `{}` 占用，请使用其他编号。", session.name);
        }
    }

    Ok(())
}

fn used_auto_session_name_claims(
    state: &TerminalState,
    path: &Path,
    excluded_auto_path: Option<&Path>,
) -> AutoSessionNameClaims {
    let mut claims = AutoSessionNameClaims::default();

    for session in state.sessions_by_id.values() {
        if excluded_auto_path
            .is_some_and(|excluded_path| session.path == excluded_path && !session.manually_renamed)
        {
            continue;
        }

        if session.path == path {
            claims.claim_path_name(session.name.clone());
        } else {
            claims.claim_name(session.name.clone());
        }
    }

    claims
}

#[cfg(test)]
pub(super) fn collect_session_infos_locked(
    state: &mut TerminalState,
    base_dir: &Path,
    display_root: &Path,
    session_ids: Vec<String>,
    error_line_limit: u32,
    error_keywords: &[String],
    auto_continue_time_patterns: &[String],
    auto_continue_respect_manual_interrupt: bool,
) -> (Vec<TerminalSessionInfo>, bool) {
    let live_sessions = session_ids
        .into_iter()
        .map(|session_id| {
            let live_session = state
                .live_sessions
                .get(&session_id)
                .filter(|session| session.is_alive())
                .cloned();
            (session_id, live_session)
        })
        .collect();
    // This test helper has no manager to borrow the shared cache from; a
    // per-call cache still exercises the same probe path.
    let probe_snapshot_cache = Mutex::new(ProbeSnapshotCache::new());
    let probes = collect_session_activity_probes(
        live_sessions,
        error_line_limit,
        error_keywords,
        auto_continue_time_patterns,
        auto_continue_respect_manual_interrupt,
        &probe_snapshot_cache,
    );
    collect_session_infos_from_probes_locked(state, base_dir, display_root, probes)
}

#[derive(Clone)]
pub(super) struct TerminalActivityProbe {
    pub(super) session_id: String,
    pub(super) live_last_output_at: u64,
    pub(super) snapshot_fingerprint: Option<u64>,
    pub(super) snapshot_probe_sequence: u64,
    pub(super) agent_activity: TerminalAgentActivity,
    pub(super) working_status: bool,
    pub(super) cline_idle_prompt: bool,
    pub(super) error_match: Option<TerminalErrorKeywordMatch>,
    pub(super) worked_status: bool,
    pub(super) pending_build: bool,
    pub(super) model_family: String,
}

#[derive(Clone, PartialEq, Eq)]
struct TerminalActivityProbeCacheKey {
    session_ids: Vec<String>,
    error_line_limit: u32,
    error_keywords: Vec<String>,
    auto_continue_time_patterns: Vec<String>,
    auto_continue_respect_manual_interrupt: bool,
}

#[derive(Default)]
pub(super) struct TerminalActivityProbeCache {
    key: Option<TerminalActivityProbeCacheKey>,
    probes: Vec<TerminalActivityProbe>,
    completed_at: Option<Instant>,
}

pub(super) fn collect_session_activity_probes(
    live_sessions: Vec<(String, Option<Arc<TerminalSession>>)>,
    error_line_limit: u32,
    error_keywords: &[String],
    auto_continue_time_patterns: &[String],
    auto_continue_respect_manual_interrupt: bool,
    probe_snapshot_cache: &Mutex<ProbeSnapshotCache>,
) -> Vec<TerminalActivityProbe> {
    if live_sessions.is_empty() {
        return Vec::new();
    }
    let agent_detector = TerminalAgentDetector::new();
    // Reserve observation order before dispatch so thread scheduling cannot
    // change the sequence assigned to each input session.
    let live_sessions: Vec<_> = live_sessions
        .into_iter()
        .map(|(session_id, live_session)| {
            let sequence = TERMINAL_ACTIVITY_PROBE_SEQUENCE.fetch_add(1, Ordering::SeqCst);
            (session_id, live_session, sequence)
        })
        .collect();
    map_activity_probes(&live_sessions, |(session_id, live_session, sequence)| {
        let live_last_output_at = live_session
            .as_ref()
            .map(|session| session.last_output_at())
            .unwrap_or(0);
        let (snapshot, snapshot_fingerprint) = probe_activity_pane_snapshot(
            probe_snapshot_cache,
            session_id,
            live_session.as_ref(),
            error_line_limit,
        );
        let model_family = terminal_model_family_from_snapshot(snapshot.as_deref());
        // Agent liveness must still be probed every cycle even when the pane
        // snapshot is reused: a process exiting without producing output is a
        // real transition the PTY output stamp cannot observe.
        let agent_activity = agent_detector.detect(&session_id);
        let working_status = snapshot
            .as_deref()
            .is_some_and(|snapshot| terminal_working_status_match_from_snapshot(snapshot, 10));
        let cline_idle_prompt = (!working_status)
            && snapshot
                .as_deref()
                .is_some_and(|snapshot| terminal_cline_idle_prompt_from_snapshot(snapshot, 10));
        let error_match = (!working_status)
            .then(|| {
                terminal_error_keyword_match_from_snapshot(
                    snapshot.as_deref()?,
                    error_line_limit,
                    error_keywords,
                    auto_continue_time_patterns,
                    auto_continue_respect_manual_interrupt,
                )
            })
            .flatten();
        let worked_status = !working_status
            && error_match.is_none()
            && snapshot
                .as_deref()
                .is_some_and(|snapshot| terminal_worked_status_match_from_snapshot(snapshot, 10));

        TerminalActivityProbe {
            session_id: session_id.clone(),
            live_last_output_at,
            snapshot_fingerprint,
            // Keep assigning the fresh per-scan sequence even for a reused
            // snapshot: observe_terminal_output_locked orders fingerprint
            // observations by this counter, and an identical fingerprint is a
            // no-op there anyway.
            snapshot_probe_sequence: *sequence,
            agent_activity,
            working_status,
            cline_idle_prompt,
            error_match,
            worked_status,
            pending_build: false,
            model_family,
        }
    })
}

/// Resolve the activity-pane snapshot for one probe. tmux pane 更新与
/// attach PTY reader 的观测不同步；输出可能延迟或被初始重绘抑制，因此
/// seq 未变仅是提示，不能作为 pane 未变的证明 — reuse is bounded by
/// `PROBE_SNAPSHOT_MAX_AGE` and pinned to the same attachment instance.
/// Dead/detached sessions have no running reader and always capture; any
/// stale entry is dropped so a later re-attach cannot alias it.
fn probe_activity_pane_snapshot(
    cache: &Mutex<ProbeSnapshotCache>,
    session_id: &str,
    live_session: Option<&Arc<TerminalSession>>,
    error_line_limit: u32,
) -> (Option<Vec<u8>>, Option<u64>) {
    probe_activity_pane_snapshot_with(cache, session_id, live_session, error_line_limit, || {
        capture_tmux_activity_pane_snapshot(session_id, error_line_limit).ok()
    })
}

/// Production seam: the caller supplies the actual pane capture so tests can
/// exercise the real cache decision without a live tmux session.
pub(super) fn probe_activity_pane_snapshot_with(
    cache: &Mutex<ProbeSnapshotCache>,
    session_id: &str,
    live_session: Option<&Arc<TerminalSession>>,
    error_line_limit: u32,
    capture: impl FnOnce() -> Option<Vec<u8>>,
) -> (Option<Vec<u8>>, Option<u64>) {
    let live_alive = live_session.is_some_and(|session| session.is_alive());
    let attachment_started_at = live_session.map(|session| session.attached_at);
    let seq_before = live_session
        .map(|session| session.current_output_seq())
        .unwrap_or(0);
    let reused = {
        let mut cache = crate::lock_or_recover!(cache.lock());
        let reused = match cache.get(session_id) {
            Some(entry)
                if live_alive
                    && seq_before != 0
                    && attachment_started_at == Some(entry.attachment_started_at)
                    && entry.live_output_seq == seq_before
                    && entry.error_line_limit == error_line_limit
                    && entry.captured_at.elapsed() < PROBE_SNAPSHOT_MAX_AGE =>
            {
                Some((entry.snapshot.clone(), entry.snapshot_fingerprint))
            }
            _ => None,
        };
        if reused.is_none() && live_session.is_none() {
            cache.remove(session_id);
        }
        reused
    };
    if let Some(reused) = reused {
        return reused;
    }

    let snapshot = capture();
    let snapshot_fingerprint = snapshot.as_deref().map(terminal_output_fingerprint);
    let seq_after = live_session
        .map(|session| session.current_output_seq())
        .unwrap_or(0);
    let still_alive = live_session.is_some_and(|session| session.is_alive());
    let mut cache = crate::lock_or_recover!(cache.lock());
    if snapshot.is_some()
        && still_alive
        && seq_before != 0
        && seq_before == seq_after
        && let Some(attachment_started_at) = attachment_started_at
    {
        // 只有抓取前后 seq 一致且非 0 的存活 attachment 才写缓存：抓取
        // 期间的输出推进意味着快照不再反映当前 pane。
        cache.insert(
            session_id.to_string(),
            ProbeSnapshotCacheEntry {
                live_output_seq: seq_after,
                attachment_started_at,
                captured_at: Instant::now(),
                error_line_limit,
                snapshot: snapshot.clone(),
                snapshot_fingerprint,
            },
        );
    } else if snapshot.is_none() {
        // 抓取失败清除旧条目：下一轮必须重试，不得复用更旧的快照。
        cache.remove(session_id);
    }
    drop(cache);
    (snapshot, snapshot_fingerprint)
}

pub(super) fn collect_session_infos_from_probes_locked(
    state: &mut TerminalState,
    base_dir: &Path,
    display_root: &Path,
    probes: Vec<TerminalActivityProbe>,
) -> (Vec<TerminalSessionInfo>, bool) {
    let mut sessions = Vec::with_capacity(probes.len());
    let mut dirty = false;

    for probe in probes {
        let activity = terminal_activity_snapshot_from_probe_locked(state, &probe);
        let session_id = &probe.session_id;
        let live_session = state
            .live_sessions
            .get(session_id)
            .filter(|session| session.is_alive())
            .cloned();

        if let Some(stored) = state.sessions_by_id.get_mut(session_id) {
            if let Some(session) = live_session.as_ref() {
                let live_name = session.name();
                if !stored.manually_renamed && stored.name != live_name {
                    stored.name = live_name;
                    dirty = true;
                }

                let live_title = session.title().unwrap_or_default();
                if stored.title != live_title {
                    stored.title = live_title;
                    dirty = true;
                }
            }

            let connected = live_session.is_some();
            sessions.push(stored.info(base_dir, display_root, activity, connected));
        }
    }

    (sessions, dirty)
}

fn terminal_activity_snapshot_from_probe_locked(
    state: &mut TerminalState,
    probe: &TerminalActivityProbe,
) -> TerminalActivitySnapshot {
    let last_output_at = observe_terminal_output_locked(
        state,
        &probe.session_id,
        probe.live_last_output_at,
        probe.snapshot_fingerprint,
        probe.snapshot_probe_sequence,
    );
    let activity_agent = probe
        .agent_activity
        .is_active()
        .then(|| probe.agent_activity.agents.join("/"));

    if probe.working_status {
        return TerminalActivitySnapshot::working(last_output_at).with_agent(activity_agent);
    }

    if probe.cline_idle_prompt {
        mark_session_output_viewed_locked(
            state,
            &probe.session_id,
            probe.snapshot_fingerprint,
            probe.snapshot_probe_sequence,
        );
        return TerminalActivitySnapshot::agent(activity_agent, last_output_at);
    }

    if let Some(error_match) = probe.error_match.as_ref() {
        if error_match.continue_sent {
            return TerminalActivitySnapshot::retrying(
                error_match.keyword.clone(),
                error_match.signature.clone(),
                error_match.auto_continue_at.clone(),
                last_output_at,
            )
            .with_agent(activity_agent)
            .with_model_family(probe.model_family.clone());
        }
        return TerminalActivitySnapshot::error(
            error_match.keyword.clone(),
            error_match.signature.clone(),
            error_match.continue_sent,
            error_match.input_queued,
            error_match.auto_continue_at.clone(),
            last_output_at,
        )
        .with_agent(activity_agent)
        .with_model_family(probe.model_family.clone());
    }

    if probe.pending_build {
        return TerminalActivitySnapshot::building(last_output_at).with_agent(activity_agent);
    }

    let last_viewed_output_at = state
        .output_observations
        .get(&probe.session_id)
        .map(|observation| observation.last_viewed_output_at)
        .unwrap_or(0);
    if probe.worked_status && last_output_at > last_viewed_output_at {
        return TerminalActivitySnapshot::completed(last_output_at).with_agent(activity_agent);
    }

    let now = current_timestamp_millis();
    if last_output_at > last_viewed_output_at
        && now >= last_output_at
        && now.saturating_sub(last_output_at) <= TERMINAL_RECENT_OUTPUT_ACTIVE_MS
    {
        return TerminalActivitySnapshot::recent_output(last_output_at).with_agent(activity_agent);
    }

    if last_output_at > last_viewed_output_at {
        return TerminalActivitySnapshot::completed(last_output_at).with_agent(activity_agent);
    }

    if probe.agent_activity.has_orphaned_agents() {
        return TerminalActivitySnapshot::orphaned_agent(activity_agent, last_output_at);
    }

    if probe.agent_activity.is_active() {
        return TerminalActivitySnapshot::agent(activity_agent, last_output_at);
    }

    TerminalActivitySnapshot::idle(last_output_at).with_model_family(probe.model_family.clone())
}

pub(super) fn collect_terminal_auto_continue_schedules(
    sessions: &[TerminalSessionInfo],
    now_millis: u64,
) -> Vec<(String, TerminalAutoContinueSchedule)> {
    sessions
        .iter()
        .filter_map(|session| {
            if !terminal_session_is_error_state(session) || session.activity_error_input_queued {
                return None;
            }
            let reset_at = session.activity_error_auto_continue_at.as_ref()?.trim();
            let due_at_millis = terminal_auto_continue_due_millis(reset_at, now_millis)?;
            let signature = session.activity_error_signature.as_ref()?.trim();
            if signature.is_empty() {
                return None;
            }
            Some((
                session.id.clone(),
                TerminalAutoContinueSchedule {
                    signature: signature.to_string(),
                    keyword: session
                        .activity_error_keyword
                        .as_deref()
                        .unwrap_or_default()
                        .to_string(),
                    reset_at: reset_at.to_string(),
                    due_at_millis,
                },
            ))
        })
        .collect()
}

fn terminal_error_keyword_is_payload_too_large(keyword: &str) -> bool {
    let keyword = keyword.to_ascii_lowercase();
    keyword.contains("413")
        || keyword.contains("payload too large")
        || keyword.contains("length limit exceeded")
}

fn terminal_session_is_error_state(session: &TerminalSessionInfo) -> bool {
    matches!(session.activity_state.as_str(), "error" | "retrying")
}

fn terminal_quota_retry_needs_glm_plan(session: &TerminalSessionInfo) -> bool {
    if !terminal_session_is_error_state(session)
        || session.activity_error_input_queued
        || session
            .activity_error_auto_continue_at
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        || !session
            .activity_agent
            .as_deref()
            .is_some_and(|agent| agent.eq_ignore_ascii_case("codex"))
    {
        return false;
    }

    terminal_error_keyword_is_quota_like(
        session
            .activity_error_keyword
            .as_deref()
            .unwrap_or_default(),
    )
}

pub(super) fn terminal_error_keyword_is_quota_like(keyword: &str) -> bool {
    let keyword = keyword.to_ascii_lowercase();
    keyword.contains("429")
        || keyword.contains("503")
        || keyword.contains("502")
        || keyword.contains("529")
        || keyword.contains("too many requests")
        || keyword.contains("service temporarily unavailable")
        || keyword.contains("service unavailable")
        || keyword.contains("temporarily unavailable")
        || keyword.contains("exceeded retry limit")
        || keyword.contains("concurrency limit exceeded")
        || keyword.contains("at capacity")
        || keyword.contains("访问量过大")
        || keyword.contains("使用上限")
        || keyword.contains("限额")
}

pub(in crate::terminal) fn terminal_quota_preflight_uses_zhipu_provider(
    persisted_base_url: &str,
    live_api_base_url: Option<&str>,
    output: &str,
) -> bool {
    if terminal_recent_output_uses_glm_model(output) {
        return true;
    }
    if let Some(base_url) = live_api_base_url.filter(|value| !value.trim().is_empty()) {
        return crate::quota_reset_cache::base_url_is_zhipu_upstream(base_url);
    }
    if !persisted_base_url.trim().is_empty() {
        return crate::quota_reset_cache::base_url_is_zhipu_upstream(persisted_base_url);
    }
    false
}

pub(super) fn terminal_recent_output_has_quota_error(output: &str, line_limit: u32) -> bool {
    let limit = usize::try_from(line_limit.max(1)).unwrap_or(usize::MAX);
    let mut lines = output.lines().rev().take(limit).collect::<Vec<_>>();
    lines.reverse();
    let tail = lines.join("\n").to_ascii_lowercase();
    let collapsed = tail.split_whitespace().collect::<Vec<_>>().join(" ");
    let compact = tail
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    collapsed.contains("429")
        || collapsed.contains("503")
        || collapsed.contains("502")
        || collapsed.contains("529")
        || collapsed.contains("too many requests")
        || collapsed.contains("service temporarily unavailable")
        || collapsed.contains("service unavailable")
        || collapsed.contains("temporarily unavailable")
        || collapsed.contains("exceeded retry limit")
        || collapsed.contains("concurrency limit exceeded")
        || collapsed.contains("at capacity")
        || compact.contains("访问量过大")
        || compact.contains("使用上限")
        || compact.contains("限额")
}

pub(super) fn terminal_recent_output_uses_glm_model(output: &str) -> bool {
    terminal_recent_output_uses_model_family(output, "glm-")
        || terminal_recent_output_uses_model_family(output, "glm ")
        || terminal_recent_output_uses_model_family(output, "glm/")
}

pub(super) fn terminal_recent_output_uses_deepseek_model(output: &str) -> bool {
    terminal_recent_output_uses_model_family(output, "deepseek")
}

pub(super) fn terminal_model_family_from_snapshot(snapshot: Option<&[u8]>) -> String {
    let Some(snapshot) = snapshot else {
        return String::new();
    };
    let output = String::from_utf8_lossy(snapshot);
    if terminal_recent_output_uses_deepseek_model(&output) {
        "deepseek".to_string()
    } else if terminal_recent_output_uses_glm_model(&output) {
        "glm".to_string()
    } else {
        String::new()
    }
}

fn terminal_recent_output_uses_model_family(output: &str, family: &str) -> bool {
    output
        .trim_end()
        .lines()
        .rev()
        .take(8)
        .any(|line| line.to_ascii_lowercase().contains(family))
}

pub(super) const MODEL_PEAK_GLM_WINDOW: &str = "14:00-18:00";
pub(super) const MODEL_PEAK_DEEPSEEK_WEEKDAY_WINDOWS: &[&str] = &["09:00-12:00", "14:00-18:00"];

pub(super) fn peak_window_for_model_family(family: &str) -> String {
    let normalized = family.trim().to_ascii_lowercase();
    if normalized.contains("glm") || normalized.contains("zhipu") {
        return MODEL_PEAK_GLM_WINDOW.to_string();
    }
    if normalized.contains("deepseek") {
        return MODEL_PEAK_DEEPSEEK_WEEKDAY_WINDOWS.join(", ");
    }
    String::new()
}

pub(super) fn now_within_model_peak_window(family: &str) -> bool {
    let normalized = family.trim().to_ascii_lowercase();
    if normalized.contains("glm") || normalized.contains("zhipu") {
        return now_within_active_window(MODEL_PEAK_GLM_WINDOW);
    }
    if normalized.contains("deepseek") {
        return now_within_deepseek_peak_window();
    }
    false
}

fn now_within_deepseek_peak_window() -> bool {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let weekday = now.weekday().number_from_monday();
    if weekday > 5 {
        return false;
    }
    MODEL_PEAK_DEEPSEEK_WEEKDAY_WINDOWS
        .iter()
        .any(|window| now_within_active_window(window))
}

fn terminal_session_error_continue_key(session: &TerminalSessionInfo) -> Option<String> {
    let signature = session.activity_error_signature.as_ref()?.trim();
    if signature.is_empty() {
        return None;
    }
    let keyword = session
        .activity_error_keyword
        .as_deref()
        .filter(|keyword| !keyword.trim().is_empty())
        .unwrap_or("error");
    Some(format!("{}\n{}\n{}", session.id, keyword, signature))
}

/// Returns true when the current local time falls inside the configured
/// active window `HH:MM-HH:MM` (24h). Supports windows that cross midnight
/// (e.g. `22:00-08:00`). Empty or malformed windows are treated as "always
/// active" by the caller, so this only needs to handle the well-formed case.
pub(super) fn now_within_active_window(window: &str) -> bool {
    let Some((start, end)) = window.split_once('-') else {
        return false;
    };
    let Some(start_min) = parse_hhmm_to_minutes(start.trim(), false) else {
        return false;
    };
    let Some(end_min) = parse_hhmm_to_minutes(end.trim(), true) else {
        return false;
    };
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let now_min = now.hour() as u32 * 60 + now.minute() as u32;
    if start_min <= end_min {
        now_min >= start_min && now_min < end_min
    } else {
        // Crosses midnight, e.g. 22:00-08:00.
        now_min >= start_min || now_min < end_min
    }
}

fn parse_hhmm_to_minutes(value: &str, allow_next_day_endpoint: bool) -> Option<u32> {
    let (h, m) = value.split_once(':')?;
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    (m < 60 && (h < 24 || (allow_next_day_endpoint && h == 24 && m == 0))).then_some(h * 60 + m)
}

/// Compute the epoch-millis timestamp of the end of the avoid window
/// `HH:MM-HH:MM` (24h). The end time is relative to today; if the end
/// minute has already passed today the window crosses midnight, so the
/// end falls tomorrow. Returns None on malformed input.
pub(in crate::terminal) fn avoid_window_end_epoch_millis(window: &str) -> Option<u64> {
    let (start, end) = window.split_once('-')?;
    let start_min = parse_hhmm_to_minutes(start.trim(), false)?;
    let end_min = parse_hhmm_to_minutes(end.trim(), false)?;
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let now_min = now.hour() as u32 * 60 + now.minute() as u32;

    // If end <= start the window crosses midnight (e.g. 22:00-06:00).
    // In that case, if we are currently before end_min, the end is today;
    // otherwise the end is tomorrow.
    let target_day_offset: i64 = if end_min <= start_min {
        // Crossing midnight: we are inside [start_min..24:00) or [0..end_min).
        // If now_min < end_min, end is today; else end is tomorrow.
        if now_min < end_min { 0 } else { 1 }
    } else {
        // Same-day window: end is today (we already know now is inside).
        0
    };

    let mut end_date = now.date();
    if target_day_offset > 0 {
        end_date = end_date.next_day()?;
    }
    let end_time = Time::from_hms((end_min / 60) as u8, (end_min % 60) as u8, 0).ok()?;
    let end_dt = end_date.with_time(end_time).assume_offset(now.offset());
    Some(end_dt.unix_timestamp() as u64 * 1000)
}

pub(in crate::terminal) fn terminal_auto_continue_due_millis(
    reset_at: &str,
    now_millis: u64,
) -> Option<u64> {
    let reset_millis = terminal_reset_time_epoch_millis(reset_at)?;
    let due_millis = reset_millis.saturating_add(TERMINAL_AUTO_CONTINUE_RESET_GRACE_MS);
    (due_millis > now_millis).then_some(due_millis)
}

pub(in crate::terminal) fn terminal_reset_time_epoch_millis(reset_at: &str) -> Option<u64> {
    let normalized = normalize_terminal_reset_datetime(reset_at)?;
    terminal_reset_time_epoch_millis_via_date_command(&normalized)
}

fn normalize_terminal_reset_datetime(reset_at: &str) -> Option<String> {
    let text = reset_at.trim().replace('T', " ");
    let mut parts = text.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    if parts.next().is_some()
        || !date
            .chars()
            .all(|character| character.is_ascii_digit() || matches!(character, '-' | '/'))
        || !time
            .chars()
            .all(|character| character.is_ascii_digit() || character == ':')
    {
        return None;
    }
    Some(format!("{} {}", date.replace('/', "-"), time))
}

#[cfg(not(target_os = "linux"))]
fn terminal_reset_time_epoch_millis_via_date_command(_reset_at: &str) -> Option<u64> {
    None
}

#[cfg(target_os = "linux")]
fn terminal_reset_time_epoch_millis_via_date_command(reset_at: &str) -> Option<u64> {
    let mut command = std::process::Command::new("date");
    command.arg("-d").arg(reset_at).arg("+%s");
    let output =
        crate::process_exec::bounded_output(command, Duration::from_secs(5), 64 * 1024).ok()?;
    if !output.status.success() {
        return None;
    }
    let seconds = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(seconds.saturating_mul(1000))
}

pub(in crate::terminal) struct TerminalAutoContinueCronEntry {
    pub(in crate::terminal) fields: String,
    pub(in crate::terminal) sleep_seconds: u64,
}

pub(in crate::terminal) fn terminal_auto_continue_cron_entry(
    due_at_millis: u64,
) -> Result<TerminalAutoContinueCronEntry> {
    let due_seconds = due_at_millis / 1000;
    let sleep_seconds = due_seconds % 60;
    let minute_epoch = due_seconds.saturating_sub(sleep_seconds);
    let mut command = Command::new("date");
    command
        .arg("-d")
        .arg(format!("@{minute_epoch}"))
        .arg("+%M %H %d %m *");
    let output = crate::process_exec::bounded_output(command, Duration::from_secs(5), 64 * 1024)
        .context("format terminal auto-continue cron time")?;
    if !output.status.success() {
        anyhow::bail!(
            "date returned non-zero status: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let fields = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if fields.split_whitespace().count() != 5 {
        anyhow::bail!("unexpected cron fields: {fields}");
    }
    Ok(TerminalAutoContinueCronEntry {
        fields,
        sleep_seconds,
    })
}

fn terminal_auto_continue_cron_marker(session_id: &str, signature: &str) -> String {
    format!("{}:{}:{}", TERMINAL_AUTO_CONTINUE_CRON_MARKER_PREFIX, session_id, signature)
}

fn terminal_auto_continue_cron_marker_prefix(session_id: &str) -> String {
    format!("{}:{}:", TERMINAL_AUTO_CONTINUE_CRON_MARKER_PREFIX, session_id)
}

fn terminal_auto_continue_due_marker_prefix(session_id: &str) -> String {
    format!("# webclx-auto-continue-due:{session_id}:")
}

fn terminal_auto_continue_due_marker(
    session_id: &str,
    signature: &str,
    due_epoch_secs: u64,
) -> String {
    format!("# webclx-auto-continue-due:{session_id}:{signature}:{due_epoch_secs}")
}

pub(in crate::terminal) fn terminal_auto_continue_cron_script(
    marker: &str,
    session_id: &str,
    keyword: &str,
    reset_at: &str,
    sleep_seconds: u64,
) -> String {
    let marker_json = serde_json::to_string(marker).unwrap_or_else(|_| "\"\"".to_string());
    let session_id_json = serde_json::to_string(session_id).unwrap_or_else(|_| "\"\"".to_string());
    let keyword_json = serde_json::to_string(keyword).unwrap_or_else(|_| "\"\"".to_string());
    let reset_at_json = serde_json::to_string(reset_at).unwrap_or_else(|_| "\"\"".to_string());
    let tmux_target_json = serde_json::to_string(&tmux_session_name(session_id))
        .unwrap_or_else(|_| "\"\"".to_string());
    let tmux_socket_json =
        serde_json::to_string(&tmux_socket_name(session_id)).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        r#"#!/usr/bin/env bash
set -u
MARKER={marker_json}
SESSION_ID={session_id_json}
KEYWORD={keyword_json}
RESET_AT={reset_at_json}
TMUX_TARGET={tmux_target_json}
TMUX_SOCKET={tmux_socket_json}
WEBCLX_BASE_URL="${{WEBCLX_BASE_URL:-http://127.0.0.1:${{WEBCLX_PORT:-11111}}}}"
sleep {sleep_seconds}
cleanup() {{
  current="$(crontab -l 2>/dev/null || true)"
  printf '%s\n' "$current" | grep -F -v "$MARKER" | crontab - 2>/dev/null || true
}}
trap cleanup EXIT
snapshot="$(tmux -L "$TMUX_SOCKET" capture-pane -p -t "$TMUX_TARGET" -S -80 2>/dev/null || true)"
if [ -z "$snapshot" ]; then
  exit 0
fi
SNAPSHOT="$snapshot" python3 - "$KEYWORD" "$RESET_AT" <<'PY'
import os
import sys
keyword = sys.argv[1]
reset_at = sys.argv[2]
snapshot = os.environ.get("SNAPSHOT", "")
compact_snapshot = " ".join(snapshot.split())
if keyword and keyword not in compact_snapshot:
    sys.exit(1)
if reset_at and reset_at not in compact_snapshot:
    sys.exit(1)
lines = snapshot.splitlines()
last_continue = max((index for index, line in enumerate(lines) if line.strip() in ("继续", "› 继续", "↳ 继续")), default=-1)
last_error = max((index for index, line in enumerate(lines) if (keyword and keyword in " ".join(line.split())) or (reset_at and reset_at in " ".join(line.split()))), default=-1)
if last_error < 0 or last_continue > last_error:
    sys.exit(1)
PY
if [ "$?" -ne 0 ]; then
  exit 0
fi
SESSION_ID_ENCODED="$(python3 - "$SESSION_ID" <<'PY'
import sys
from urllib.parse import quote
print(quote(sys.argv[1], safe=""))
PY
)"
curl -fsS --noproxy '*' -X POST \
  "$WEBCLX_BASE_URL/api/terminal/sessions/${{SESSION_ID_ENCODED}}/auto-continue" \
  -H 'Content-Type: application/json' \
  --data '{{}}' >/dev/null 2>&1 || true
"#
    )
}

/// Parse the 5-field cron schedule ("M H DoM Mo *") that this module itself
/// generates and compute, for the given reference year, the local epoch seconds
/// of its occurrence. Returns None when the fields are not the expected shape.
fn auto_continue_cron_occurrence_for_year(schedule_fields: &str, year: i32) -> Option<i64> {
    // The cron schedule is generated locally by terminal_auto_continue_cron_entry
    // using the local `date` command, so resolve its epoch the same way to stay
    // consistent with how cron itself interprets the fields.
    let parts: Vec<&str> = schedule_fields.split_whitespace().collect();
    if parts.len() != 5 || parts[4] != "*" {
        return None;
    }
    let minute: u8 = parts[0].parse().ok()?;
    let hour: u8 = parts[1].parse().ok()?;
    let day: u8 = parts[2].parse().ok()?;
    let month_num: u8 = parts[3].parse().ok()?;
    // Validate via the time crate so malformed fields (e.g. day 31 in a short
    // month) are rejected before we hand them to the shell.
    let month = Month::try_from(month_num).ok()?;
    let _ = Date::from_calendar_date(year, month, day).ok()?;
    let date_string = format!("{year:04}-{month_num:02}-{day:02} {hour:02}:{minute:02}:00");
    let mut command = Command::new("date");
    command.arg("-d").arg(&date_string).arg("+%s");
    let output =
        crate::process_exec::bounded_output(command, Duration::from_secs(5), 64 * 1024).ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    text.parse::<i64>().ok()
}

/// Earliest local occurrence of the one-shot cron schedule at or after the
/// search start, checking the start year and the next year (a 5-field cron has
/// no year field, so the soonest valid occurrence is within at most one year).
fn auto_continue_cron_next_occurrence_epoch(schedule_fields: &str) -> Option<i64> {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let start_epoch = now.unix_timestamp();
    for year in [now.year(), now.year() + 1] {
        if let Some(epoch) = auto_continue_cron_occurrence_for_year(schedule_fields, year)
            && epoch >= start_epoch
        {
            return Some(epoch);
        }
    }
    None
}

/// A snapshot of an expired auto-continue cron entry kept for the history view.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(in crate::terminal) struct ExpiredAutoContinueTaskRecord {
    pub marker: String,
    pub session_id: String,
    pub session_name: Option<String>,
    pub webclx_terminal_name: Option<String>,
    pub tmux_session_name: String,
    pub signature: String,
    pub schedule: String,
    pub expired_at: i64,
}

impl TerminalManager {
    /// Refresh the cached API preset snapshot used to map terminal sessions to
    /// their upstream provider when consuming proxy-captured quota reset times.
    /// Called from the HTTP layer whenever session lists are built.
    pub(in crate::terminal) fn update_api_preset_snapshot(
        &self,
        presets: Vec<auth_core::StoredApiPreset>,
    ) {
        if let Ok(mut guard) = self.api_preset_snapshot.write() {
            *guard = presets;
        }
    }

    /// Fill missing reset times on errored Zhipu sessions from the proxy
    /// quota-reset cache. This is used by both background scanning and session
    /// list responses so the frontend does not treat quota 429s as ordinary
    /// retryable errors before the scanner has run.
    pub(in crate::terminal) fn backfill_zhipu_quota_reset_times_for_sessions(
        &self,
        sessions: &mut [TerminalSessionInfo],
    ) {
        for session in sessions {
            if !terminal_session_is_error_state(session) {
                continue;
            }
            if session
                .activity_error_auto_continue_at
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty())
            {
                continue;
            }
            let reset_at = self
                .zhipu_quota_reset_for_session(
                    &session.codex_api_preset_name,
                    &session.codex_api_base_url,
                )
                .or_else(|| {
                    let output = capture_tmux_recent_pane_snapshot(&session.id).ok()?;
                    self.glm_model_quota_reset_from_output(&output)
                });
            if let Some(reset_at) = reset_at {
                session.activity_error_auto_continue_at = Some(reset_at);
            }
        }
    }

    pub(super) fn glm_model_quota_reset_from_output(&self, output: &[u8]) -> Option<String> {
        terminal_recent_output_uses_glm_model(&String::from_utf8_lossy(output))
            .then(|| self.quota_reset_cache.get_latest_zhipu_reset())
            .flatten()
    }

    /// Resolve the upstream base_url for a session by matching its preset name
    /// against the cached API preset snapshot, then read a proxy-captured
    /// Zhipu quota reset time keyed by that preset id or base URL.
    ///
    /// Returns the parsed reset time string when the session maps to a Zhipu
    /// preset that recently hit a quota-exceeded 429.
    pub(in crate::terminal) fn zhipu_quota_reset_for_session(
        &self,
        preset_name: &str,
        codex_api_base_url: &str,
    ) -> Option<String> {
        let fallback_base = codex_api_base_url.trim();
        let presets = self.api_preset_snapshot.read().ok();
        if let Some(preset) = presets.as_ref().and_then(|presets| {
            presets.iter().find(|preset| {
                let name = preset.name.trim();
                let base = preset.base_url.trim();
                (!name.is_empty() && name == preset_name.trim())
                    || (!base.is_empty() && base == fallback_base)
            })
        }) {
            // Only read for Zhipu upstreams so non-Zhipu presets are unaffected.
            if !crate::quota_reset_cache::base_url_is_zhipu_upstream(&preset.base_url) {
                return None;
            }
            if let Some(reset_at) = self.quota_reset_cache.get_for_preset(&preset.id) {
                return Some(reset_at);
            }
            return self.quota_reset_cache.get_for_base_url(&preset.base_url);
        }

        if !crate::quota_reset_cache::base_url_is_zhipu_upstream(fallback_base) {
            return None;
        }
        self.quota_reset_cache.get_for_base_url(fallback_base)
    }

    /// Read the current auto-continue crontab, split it into still-valid and
    /// expired entries, purge the expired entries from crontab, and archive
    /// them into the history file. Returns (expired_records, crontab_error).
    ///
    /// `active_tasks` is the parsed view of every current crontab line; the
    /// caller keeps only the non-expired ones for the live list.
    pub(in crate::terminal) fn prune_expired_auto_continue_tasks(
        &self,
        active_tasks: &mut Vec<crate::terminal::TerminalAutoContinueTaskInfo>,
    ) -> (Vec<ExpiredAutoContinueTaskRecord>, Option<String>) {
        let history_path = self
            .state_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(TERMINAL_AUTO_CONTINUE_HISTORY_FILE_NAME);

        let now_epoch = OffsetDateTime::now_local()
            .unwrap_or_else(|_| OffsetDateTime::now_utc())
            .unix_timestamp();

        // Partition live tasks into expired / active. An entry is expired when its
        // intended fire moment has already passed: prefer the deterministic
        // `due_epoch` metadata written at install time; for legacy entries that
        // predate that metadata, fall back to checking whether this year's cron
        // occurrence is in the past (with a short grace for scheduling jitter).
        const LEGACY_GRACE_SECS: i64 = 15 * 60;
        let mut expired: Vec<ExpiredAutoContinueTaskRecord> = Vec::new();
        let mut expired_markers: Vec<String> = Vec::new();
        active_tasks.retain(|task| {
            let is_expired = if let Some(due_epoch) = task.due_epoch {
                // Deterministic path: the one-shot moment is in the past.
                now_epoch - due_epoch > LEGACY_GRACE_SECS
            } else {
                // Legacy path (no due metadata): infer from the 5-field schedule.
                match auto_continue_cron_next_occurrence_epoch(&task.schedule) {
                    // next occurrence resolves to next year => this year's moment
                    // already passed without firing => orphan.
                    Some(epoch) => epoch - now_epoch > 86_400,
                    // Unparseable schedule: keep it visible rather than risk
                    // deleting an entry the user still wants to inspect.
                    None => false,
                }
            };
            if is_expired {
                expired_markers.push(task.marker.clone());
                expired.push(ExpiredAutoContinueTaskRecord {
                    marker: task.marker.clone(),
                    session_id: task.session_id.clone(),
                    session_name: task.session_name.clone(),
                    webclx_terminal_name: task.webclx_terminal_name.clone(),
                    tmux_session_name: task.tmux_session_name.clone(),
                    signature: task.signature.clone(),
                    schedule: task.schedule.clone(),
                    expired_at: now_epoch,
                });
                false
            } else {
                true
            }
        });

        if expired.is_empty() {
            let history = load_auto_continue_history(&history_path).unwrap_or_default();
            return (history, None);
        }

        // Remove the expired lines from the live crontab so they stop reappearing.
        let crontab_error = match rewrite_crontab_without_markers(&expired_markers) {
            Ok(()) => None,
            Err(error) => {
                warn!("prune expired auto-continue crontab lines failed: {error}");
                Some(format!("清理过期定时任务失败: {error}"))
            }
        };

        // Merge into history (most recent first, dedup by marker, capped).
        let mut history = load_auto_continue_history(&history_path).unwrap_or_default();
        for record in &expired {
            history.retain(|item| item.marker != record.marker);
            history.insert(0, record.clone());
        }
        if history.len() > TERMINAL_AUTO_CONTINUE_HISTORY_MAX {
            history.truncate(TERMINAL_AUTO_CONTINUE_HISTORY_MAX);
        }
        if let Err(error) = save_auto_continue_history(&history_path, &history) {
            warn!("persist auto-continue history failed: {error}");
        }

        (history, crontab_error)
    }

    /// Wipe the archived expired-task history file. Returns the number of
    /// records that were removed. Missing file is treated as already empty.
    pub(in crate::terminal) fn clear_auto_continue_history(&self) -> Result<usize> {
        let history_path = self
            .state_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(TERMINAL_AUTO_CONTINUE_HISTORY_FILE_NAME);
        let removed = match load_auto_continue_history(&history_path) {
            Ok(records) => records.len(),
            Err(_) => 0,
        };
        // Persist an empty array so the API reflects the cleared state even
        // before the next prune pass; NotFound is fine (already gone).
        let _ = save_auto_continue_history(&history_path, &[]);
        Ok(removed)
    }
}

fn load_auto_continue_history(path: &Path) -> Result<Vec<ExpiredAutoContinueTaskRecord>> {
    let content = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Vec::new());
        }
        Err(error) => {
            return Err(anyhow::anyhow!(error));
        }
    };
    let history: Vec<ExpiredAutoContinueTaskRecord> =
        serde_json::from_str(&content).unwrap_or_default();
    Ok(history)
}

fn save_auto_continue_history(
    path: &Path,
    history: &[ExpiredAutoContinueTaskRecord],
) -> Result<()> {
    let json = serde_json::to_string_pretty(history)?;
    fs::write(path, json)
        .with_context(|| format!("write auto-continue history {}", path.display()))?;
    Ok(())
}

fn observe_terminal_output_locked(
    state: &mut TerminalState,
    session_id: &str,
    live_last_output_at: u64,
    snapshot_fingerprint: Option<u64>,
    snapshot_probe_sequence: u64,
) -> u64 {
    let observation = state
        .output_observations
        .entry(session_id.to_string())
        .or_default();

    if let Some(fingerprint) = snapshot_fingerprint
        && snapshot_probe_sequence >= observation.last_fingerprint_probe_sequence
    {
        observation.last_fingerprint_probe_sequence = snapshot_probe_sequence;
        if observation.rebaseline_after_restore {
            observation.rebaseline_after_restore = false;
            observation.fingerprint = Some(fingerprint);
            return observation.last_output_at;
        }
        match observation.fingerprint {
            None => {
                observation.fingerprint = Some(fingerprint);
                observation.last_output_at = observation.last_output_at.max(live_last_output_at);
            }
            Some(previous) if previous != fingerprint => {
                observation.fingerprint = Some(fingerprint);
                observation.last_output_at = observation
                    .last_output_at
                    .max(live_last_output_at)
                    .max(current_timestamp_millis());
            }
            Some(_) => {}
        }
    } else if snapshot_fingerprint.is_none() {
        observation.last_output_at = observation.last_output_at.max(live_last_output_at);
    }

    observation.last_output_at
}

pub(super) fn rebaseline_terminal_output_locked(
    state: &mut TerminalState,
    session_id: &str,
    snapshot_fingerprint: Option<u64>,
    snapshot_probe_sequence: u64,
) {
    let Some(fingerprint) = snapshot_fingerprint else {
        return;
    };
    let observation = state
        .output_observations
        .entry(session_id.to_string())
        .or_default();
    if snapshot_probe_sequence < observation.last_fingerprint_probe_sequence {
        return;
    }
    observation.last_fingerprint_probe_sequence = snapshot_probe_sequence;
    observation.fingerprint = Some(fingerprint);
}

pub(super) fn restore_system_probe_output_observation_locked(
    state: &mut TerminalState,
    session_id: &str,
    before: Option<&TerminalOutputObservation>,
    after_fingerprint: Option<u64>,
    snapshot_probe_sequence: u64,
) {
    let observation = state
        .output_observations
        .entry(session_id.to_string())
        .or_default();
    observation.fingerprint =
        after_fingerprint.or_else(|| before.and_then(|item| item.fingerprint));
    observation.last_fingerprint_probe_sequence = snapshot_probe_sequence;
    observation.rebaseline_after_restore = false;
    observation.last_output_at = before.map(|item| item.last_output_at).unwrap_or(0);
    observation.last_viewed_output_at = before.map(|item| item.last_viewed_output_at).unwrap_or(0);
}

pub(super) fn arm_output_observations_for_restore_locked(state: &mut TerminalState) {
    for observation in state.output_observations.values_mut() {
        observation.rebaseline_after_restore = true;
    }
}

/// Rebase a restored session's pane fingerprint before the first activity scan.
///
/// tmux attach/replay after a restart can redraw the same pane. The observation
/// must keep its old `last_output_at` for that restore-time fingerprint and let
/// the first scan consume the flag; otherwise restart alone can turn an already
/// viewed idle terminal back into `待查看`.
pub(super) fn prepare_restored_output_observation_locked(
    state: &mut TerminalState,
    session_id: &str,
    fingerprint: Option<u64>,
    snapshot_probe_sequence: u64,
) -> bool {
    let created = !state.output_observations.contains_key(session_id);
    if let Some(fingerprint) = fingerprint {
        rebaseline_terminal_output_locked(
            state,
            session_id,
            Some(fingerprint),
            snapshot_probe_sequence,
        );
    }
    state
        .output_observations
        .entry(session_id.to_string())
        .or_default()
        .rebaseline_after_restore = true;
    created
}

fn mark_session_output_viewed_locked(
    state: &mut TerminalState,
    session_id: &str,
    snapshot_fingerprint: Option<u64>,
    snapshot_probe_sequence: u64,
) {
    let live_last_output_at = state
        .live_sessions
        .get(session_id)
        .filter(|session| session.is_alive())
        .map(|session| session.last_output_at())
        .unwrap_or(0);
    let last_output_at = observe_terminal_output_locked(
        state,
        session_id,
        live_last_output_at,
        snapshot_fingerprint,
        snapshot_probe_sequence,
    );
    let observation = state
        .output_observations
        .entry(session_id.to_string())
        .or_default();
    observation.last_viewed_output_at = observation.last_viewed_output_at.max(last_output_at);
}

fn find_terminal_output_match(text: &str, needle: &str) -> Option<(usize, String, usize)> {
    let needle_lower = needle.to_lowercase();
    if needle_lower.is_empty() {
        return None;
    }

    let text_lower = text.to_lowercase();
    let match_count = count_non_overlapping_matches(&text_lower, &needle_lower);
    if match_count == 0 {
        return None;
    }

    for (index, line) in text.lines().enumerate() {
        if line.to_lowercase().contains(&needle_lower) {
            return Some((index + 1, compact_terminal_search_line(line), match_count));
        }
    }

    Some((1, String::new(), match_count))
}

pub(super) fn should_notify_session_list_sync(cleanup_dirty: bool, _info_dirty: bool) -> bool {
    cleanup_dirty
}

fn sorted_session_ids(state: &TerminalState) -> Vec<String> {
    let mut session_ids: Vec<String> = state.sessions_by_id.keys().cloned().collect();
    session_ids.sort_by(|left, right| compare_session_ids(left, right));
    session_ids
}

fn compare_session_ids(left: &str, right: &str) -> CmpOrdering {
    session_sort_ordinal(left)
        .cmp(&session_sort_ordinal(right))
        .then_with(|| left.cmp(right))
}

fn normalize_terminal_group_color(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let value = value.trim();
    if value.is_empty() {
        return Ok(Some(String::new()));
    }
    anyhow::ensure!(
        value.len() <= 7
            && value.starts_with('#')
            && value[1..].len() == 6
            && value[1..]
                .chars()
                .all(|character| character.is_ascii_hexdigit()),
        "分组颜色必须使用 #RRGGBB 格式"
    );
    let mut normalized = String::with_capacity(7);
    normalized.push('#');
    normalized.push_str(&value[1..].to_ascii_lowercase());
    Ok(Some(normalized))
}

fn normalize_terminal_group_name(value: Option<&str>) -> Result<Option<String>> {
    const MAX_GROUP_NAME_CHARS: usize = 32;
    let Some(value) = value else {
        return Ok(None);
    };
    let normalized = value.trim();
    if normalized.is_empty() {
        return Ok(Some(String::new()));
    }
    anyhow::ensure!(
        normalized.chars().count() <= MAX_GROUP_NAME_CHARS,
        "分组名称不能超过 {MAX_GROUP_NAME_CHARS} 个字符"
    );
    anyhow::ensure!(!normalized.chars().any(char::is_control), "分组名称不能包含控制字符");
    Ok(Some(normalized.to_string()))
}

fn normalize_terminal_group_role(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let normalized = value.trim();
    if normalized.is_empty() {
        return Ok(Some(String::new()));
    }
    let normalized = match normalized.to_ascii_lowercase().as_str() {
        "manager" | "管理者" | "管理" | "计划者" | "planner" => "管理者",
        "executor" | "执行者" | "执行" | "worker" => "执行者",
        "reviewer" | "复核者" | "复核" => "复核者",
        _ => anyhow::bail!("分组角色只能是管理者、执行者或复核者"),
    };
    Ok(Some(normalized.to_string()))
}

fn terminal_group_color_slot(token: &str) -> f64 {
    let mut hash = 2_166_136_261u32;
    for byte in token.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    (hash % 360) as f64
}

pub(super) fn default_terminal_group_color(token: &str) -> String {
    default_terminal_group_color_avoiding(token, &std::collections::HashSet::new())
}

fn default_terminal_group_color_avoiding(
    token: &str,
    used_colors: &std::collections::HashSet<String>,
) -> String {
    // Use a fixed golden-angle step after a collision. This keeps the result
    // stable across reloads while ensuring a newly created group does not
    // reuse a color already assigned to another group.
    let base_hue = terminal_group_color_slot(token);
    for attempt in 0..360 {
        let hue = (base_hue + f64::from(attempt) * 137.508) % 360.0;
        let hex = hsl_terminal_group_color(&format!("hsl({hue:.3} 76% 52%)"))
            .expect("canonical default group color should convert");
        if !used_colors.contains(&hex) {
            debug_assert_eq!(hex.len(), 7);
            return hex;
        }
    }
    // Even when the hue palette is exhausted, search the full RGB space
    // rather than returning an occupied color or panicking.
    (0..=0xff_ffffu32)
        .map(|rgb| format!("#{rgb:06x}"))
        .find(|color| !used_colors.contains(color))
        .expect("terminal groups cannot exhaust the full RGB color space")
}

fn legacy_default_terminal_group_color(token: &str) -> String {
    let mut hash = 2_166_136_261u32;
    for byte in token.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    format!(
        "#{:02x}{:02x}{:02x}",
        64 + (hash & 0x7f),
        96 + ((hash >> 8) & 0x7f),
        160 + ((hash >> 16) & 0x5f)
    )
}

pub(super) fn hsl_terminal_group_color(value: &str) -> Option<String> {
    let rest = value.strip_prefix("hsl(")?.strip_suffix(")")?;
    let mut parts = rest.split_whitespace();
    let hue = parts.next()?;
    let saturation = parts.next()?;
    let lightness = parts.next()?;
    if parts.next().is_some() {
        return None;
    }
    let hue: f64 = hue.parse().ok()?;
    let saturation: f64 = saturation.strip_suffix('%')?.parse().ok()?;
    let lightness: f64 = lightness.strip_suffix('%')?.parse().ok()?;
    if !hue.is_finite() || !saturation.is_finite() || !lightness.is_finite() {
        return None;
    }
    if !(0.0..=360.0).contains(&hue)
        || !(0.0..=100.0).contains(&saturation)
        || !(0.0..=100.0).contains(&lightness)
    {
        return None;
    }

    let chroma = (1.0 - (2.0 * lightness / 100.0 - 1.0).abs()) * saturation / 100.0;
    let hue_prime = hue / 60.0;
    let secondary = chroma * (1.0 - (hue_prime.rem_euclid(2.0) - 1.0).abs());
    let base = lightness / 100.0 - chroma / 2.0;
    let (red, green, blue) = if hue_prime < 1.0 {
        (chroma, secondary, 0.0)
    } else if hue_prime < 2.0 {
        (secondary, chroma, 0.0)
    } else if hue_prime < 3.0 {
        (0.0, chroma, secondary)
    } else if hue_prime < 4.0 {
        (0.0, secondary, chroma)
    } else if hue_prime < 5.0 {
        (secondary, 0.0, chroma)
    } else {
        (chroma, 0.0, secondary)
    };
    Some(format!(
        "#{:02x}{:02x}{:02x}",
        ((red + base) * 255.0).round() as u8,
        ((green + base) * 255.0).round() as u8,
        ((blue + base) * 255.0).round() as u8
    ))
}

pub(super) fn normalize_stored_terminal_group_color(session: &mut StoredTerminalSession) -> bool {
    if session.group_token.is_empty() {
        return false;
    }
    if let Some(hex) = hsl_terminal_group_color(&session.group_color) {
        session.group_color = hex;
        true
    } else {
        false
    }
}

pub(super) fn update_session_group_locked(
    state: &mut TerminalState,
    session_id: &str,
    member_ids: &[String],
    group_roles: &[super::TerminalGroupRoleAssignment],
    fixed_order: bool,
    group_color: Option<String>,
    group_name: Option<String>,
) -> Result<()> {
    let normalized_color = normalize_terminal_group_color(group_color.as_deref())?;
    let normalized_name = normalize_terminal_group_name(group_name.as_deref())?;
    let mut normalized_roles = HashMap::new();
    for assignment in group_roles {
        let id = assignment.session_id.trim();
        let role = normalize_terminal_group_role(Some(&assignment.role))?.unwrap_or_default();
        normalized_roles.insert(id.to_string(), role);
    }
    let old_token = state
        .sessions_by_id
        .get(session_id)
        .with_context(|| format!("会话 `{session_id}` 不存在"))?
        .group_token
        .clone();
    let (inherited_color, inherited_name, inherited_role) = if old_token.is_empty() {
        (None, None, None)
    } else {
        let stored = state.sessions_by_id.get(session_id);
        (
            stored
                .map(|session| session.group_color.clone())
                .filter(|color| !color.is_empty()),
            stored
                .map(|session| session.group_name.clone())
                .filter(|name| !name.is_empty()),
            stored
                .map(|session| session.group_role.clone())
                .filter(|role| !role.is_empty()),
        )
    };
    let mut members: Vec<String> = member_ids
        .iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    anyhow::ensure!(
        members
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            == members.len(),
        "分组顺序不能包含重复终端"
    );
    if !members.iter().any(|id| id == session_id) {
        members.insert(0, session_id.to_string());
    }
    for id in &members {
        anyhow::ensure!(state.sessions_by_id.contains_key(id), "会话 `{id}` 不存在");
    }
    if member_ids.is_empty() {
        if !old_token.is_empty() {
            let mut remaining: Vec<String> = state
                .sessions_by_id
                .values()
                .filter(|session| session.group_token == old_token && session.id != session_id)
                .map(|session| session.id.clone())
                .collect();
            remaining.sort_by(|left, right| {
                session_group_order(state, left)
                    .cmp(&session_group_order(state, right))
                    .then_with(|| compare_session_ids(left, right))
            });
            if let Some(session) = state.sessions_by_id.get_mut(session_id) {
                session.group_token.clear();
                session.group_color.clear();
                session.group_name.clear();
                session.group_role.clear();
                session.group_order = 0;
                session.group_fixed_order = false;
            }
            if remaining.len() < 2 {
                for id in remaining {
                    if let Some(session) = state.sessions_by_id.get_mut(&id) {
                        session.group_token.clear();
                        session.group_color.clear();
                        session.group_name.clear();
                        session.group_role.clear();
                        session.group_order = 0;
                        session.group_fixed_order = false;
                    }
                }
            } else {
                for (index, id) in remaining.iter().enumerate() {
                    if let Some(session) = state.sessions_by_id.get_mut(id) {
                        session.group_order = index as u64 + 1;
                    }
                }
            }
        }
        return Ok(());
    }
    let token = if members.len() > 1 {
        (1u64..)
            .map(|n| format!("group-{n}"))
            .find(|token| {
                !state
                    .sessions_by_id
                    .values()
                    .any(|s| s.group_token == *token)
            })
            .unwrap()
    } else {
        String::new()
    };
    let effective_color = if token.is_empty() {
        String::new()
    } else if let Some(color) = normalized_color {
        color
    } else {
        inherited_color.unwrap_or_else(|| {
            let used_colors = state
                .sessions_by_id
                .values()
                .filter(|session| session.group_token != old_token)
                .filter(|session| !session.group_color.is_empty())
                .map(|session| session.group_color.to_lowercase())
                .collect();
            default_terminal_group_color_avoiding(&token, &used_colors)
        })
    };
    let effective_name = if token.is_empty() {
        String::new()
    } else if let Some(name) = normalized_name {
        name
    } else {
        inherited_name.unwrap_or_default()
    };
    let default_role = if normalized_roles.is_empty() {
        inherited_role.unwrap_or_default()
    } else {
        String::new()
    };
    for (index, id) in members.iter().enumerate() {
        let session = state
            .sessions_by_id
            .get_mut(id)
            .expect("validated group member should exist");
        session.group_token = token.clone();
        session.group_color = effective_color.clone();
        session.group_name = effective_name.clone();
        session.group_role = normalized_roles
            .get(id)
            .cloned()
            .filter(|role| !role.is_empty())
            .unwrap_or(default_role.clone());
        session.group_fixed_order = fixed_order && !token.is_empty();
        session.group_order = if token.is_empty() {
            0
        } else {
            index as u64 + 1
        };
    }
    for (id, session) in &mut state.sessions_by_id {
        if !members.contains(id) && !old_token.is_empty() && session.group_token == old_token {
            session.group_token.clear();
            session.group_color.clear();
            session.group_name.clear();
            session.group_role.clear();
            session.group_order = 0;
        }
    }
    // A remaining singleton is no longer a group.
    let mut counts = std::collections::HashMap::new();
    for session in state.sessions_by_id.values() {
        *counts.entry(session.group_token.clone()).or_insert(0usize) += 1;
    }
    for session in state.sessions_by_id.values_mut() {
        if counts.get(&session.group_token) == Some(&1) {
            session.group_token.clear();
            session.group_color.clear();
            session.group_name.clear();
            session.group_role.clear();
            session.group_order = 0;
        }
    }
    Ok(())
}

fn terminal_group_change_affected_ids(
    state: &TerminalState,
    session_id: &str,
    member_ids: &[String],
) -> Vec<String> {
    let mut ids: HashSet<String> = member_ids
        .iter()
        .map(|id| id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    ids.insert(session_id.to_string());
    let previous_token = state
        .sessions_by_id
        .get(session_id)
        .map(|session| session.group_token.clone())
        .unwrap_or_default();
    if !previous_token.is_empty() {
        ids.extend(
            state
                .sessions_by_id
                .values()
                .filter(|session| session.group_token == previous_token)
                .map(|session| session.id.clone()),
        );
    }
    ids.into_iter().collect()
}

pub(super) fn sort_session_ids_by_recent_activity(
    state: &TerminalState,
    session_ids: &mut [String],
) {
    session_ids.sort_by(|left, right| {
        session_last_opened_at(state, right)
            .cmp(&session_last_opened_at(state, left))
            .then_with(|| session_created_at(state, right).cmp(&session_created_at(state, left)))
            .then_with(|| session_sort_ordinal(right).cmp(&session_sort_ordinal(left)))
            .then_with(|| right.cmp(left))
    });
    let mut groups: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for id in session_ids.iter() {
        let token = session_group_token(state, id);
        if !token.is_empty() {
            groups
                .entry(token.to_string())
                .or_default()
                .push(id.clone());
        }
    }
    for members in groups.values_mut() {
        members.sort_by(|left, right| {
            session_group_order(state, left)
                .cmp(&session_group_order(state, right))
                .then_with(|| compare_session_ids(left, right))
        });
    }
    let mut result = Vec::with_capacity(session_ids.len());
    for id in session_ids.iter() {
        let token = session_group_token(state, id);
        if token.is_empty() {
            result.push(id.clone());
        } else if let Some(members) = groups.remove(token) {
            result.extend(members);
        }
    }
    session_ids.clone_from_slice(&result);
}

fn session_group_token<'a>(state: &'a TerminalState, session_id: &str) -> &'a str {
    state
        .sessions_by_id
        .get(session_id)
        .map(|session| session.group_token.as_str())
        .unwrap_or_default()
}

fn session_group_order(state: &TerminalState, session_id: &str) -> u64 {
    state
        .sessions_by_id
        .get(session_id)
        .map(|session| session.group_order)
        .unwrap_or(0)
}

fn session_created_at(state: &TerminalState, session_id: &str) -> u64 {
    state
        .sessions_by_id
        .get(session_id)
        .map(|session| session.created_at)
        .unwrap_or_default()
}

fn session_last_opened_at(state: &TerminalState, session_id: &str) -> u64 {
    state
        .sessions_by_id
        .get(session_id)
        .map(|session| session.last_opened_at)
        .unwrap_or_default()
}

fn load_terminal_registry(path: &Path) -> Result<StoredTerminalRegistry> {
    if !path.exists() {
        return Ok(StoredTerminalRegistry::default());
    }

    let content = std::fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let registry = serde_json::from_slice(&content)
        .with_context(|| format!("cannot decode {}", path.display()))?;
    Ok(registry)
}

fn persist_terminal_registry(path: &Path, registry: &StoredTerminalRegistry) -> Result<()> {
    let content =
        serde_json::to_vec_pretty(registry).context("cannot encode terminal session registry")?;
    atomic_file::write_atomic(path, &content)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn load_terminal_workspace_restore_registry(
    path: &Path,
) -> Result<TerminalWorkspaceRestoreRegistry> {
    if !path.exists() {
        return Ok(TerminalWorkspaceRestoreRegistry::default());
    }
    let content = fs::read(path)
        .with_context(|| format!("cannot read terminal workspace restores {}", path.display()))?;
    let mut registry: TerminalWorkspaceRestoreRegistry = serde_json::from_slice(&content)
        .with_context(|| format!("cannot decode terminal workspace restores {}", path.display()))?;
    registry
        .records
        .sort_by(|left, right| left.session_id.cmp(&right.session_id));
    registry
        .records
        .dedup_by(|left, right| left.session_id == right.session_id);
    Ok(registry)
}

fn persist_terminal_workspace_restore_registry(
    path: &Path,
    registry: &TerminalWorkspaceRestoreRegistry,
) -> Result<()> {
    let content =
        serde_json::to_vec_pretty(registry).context("cannot encode terminal workspace restores")?;
    atomic_file::write_atomic(path, &content)
        .with_context(|| format!("cannot write {}", path.display()))?;
    Ok(())
}

fn sort_terminal_workspace_restore_records(records: &mut [TerminalWorkspaceRestoreRecord]) {
    records.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.session_id.cmp(&right.session_id))
    });
}

fn increment_workspace_restore_counts(
    records: &mut [TerminalWorkspaceRestoreRecord],
    restored_ids: &HashSet<String>,
    updated_at: u64,
) -> usize {
    let mut updated = 0;
    for record in records {
        if restored_ids.contains(&record.session_id) {
            record.restore_count = record.restore_count.saturating_add(1);
            record.updated_at = updated_at;
            updated += 1;
        }
    }
    updated
}

#[cfg(test)]
mod workspace_restore_registry_tests {
    use super::*;

    fn unique_temp_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!("webclx-workspace-restore-{label}-{}-{nonce}.json", std::process::id()))
    }

    fn registry_from_json(json: &str) -> TerminalWorkspaceRestoreRegistry {
        serde_json::from_str(json).expect("workspace restore fixture should decode")
    }

    #[test]
    fn workspace_restore_load_deduplicates_without_rewriting_source() {
        let path = unique_temp_path("deduplicate");
        let original = br#"{
            "records": [
                {"session_id":"s1","path":"/tmp","user_name":"root","name":"first"},
                {"session_id":"s1","path":"/tmp","user_name":"root","name":"second"}
            ]
        }"#;
        fs::write(&path, original).expect("write duplicate fixture");

        let registry = load_terminal_workspace_restore_registry(&path)
            .expect("duplicate registry should load");

        assert_eq!(registry.records.len(), 1);
        assert_eq!(registry.records[0].name, "first");
        assert_eq!(fs::read(&path).expect("read fixture"), original);
        fs::remove_file(path).expect("remove fixture");
    }

    #[test]
    fn workspace_restore_persist_replaces_file_without_fixed_temp_artifact() {
        let path = unique_temp_path("atomic");
        let registry = registry_from_json(
            r#"{"records":[{"session_id":"s2","path":"/tmp","user_name":"root","name":"saved"}]}"#,
        );

        persist_terminal_workspace_restore_registry(&path, &registry)
            .expect("persist workspace restore registry");

        let loaded = load_terminal_workspace_restore_registry(&path).expect("reload registry");
        assert_eq!(loaded.records[0].name, "saved");
        let fixed_temp = path.with_extension("json.webclx-tmp");
        assert!(!fixed_temp.exists(), "legacy fixed temp file must not be used");
        fs::remove_file(path).expect("remove fixture");
    }

    #[test]
    fn workspace_restore_count_only_tracks_successful_ids() {
        let mut registry = registry_from_json(
            r#"{"records":[
                {"session_id":"ok","path":"/tmp","user_name":"root","name":"ok","restore_count":2},
                {"session_id":"failed","path":"/tmp","user_name":"root","name":"failed","restore_count":4}
            ]}"#,
        );
        let restored_ids = HashSet::from(["ok".to_string()]);

        let updated =
            increment_workspace_restore_counts(&mut registry.records, &restored_ids, 123_456);

        assert_eq!(updated, 1);
        let ok = registry
            .records
            .iter()
            .find(|record| record.session_id == "ok")
            .expect("successful record");
        let failed = registry
            .records
            .iter()
            .find(|record| record.session_id == "failed")
            .expect("failed record");
        assert_eq!((ok.restore_count, ok.updated_at), (3, 123_456));
        assert_eq!((failed.restore_count, failed.updated_at), (4, 0));
    }

    #[test]
    fn workspace_restore_corrupt_json_is_reported() {
        let path = unique_temp_path("corrupt");
        fs::write(&path, b"{not-json").expect("write corrupt fixture");

        let error = load_terminal_workspace_restore_registry(&path)
            .expect_err("corrupt registry must be distinguishable from an empty registry");

        assert!(error.to_string().contains("cannot decode"));
        assert_eq!(fs::read(&path).expect("read corrupt fixture"), b"{not-json");
        fs::remove_file(path).expect("remove fixture");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn workspace_restore_delete_only_removes_orphan_target_record() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        let base_dir = std::env::temp_dir()
            .join(format!("webclx-workspace-restore-delete-{}-{unique}", std::process::id()));
        fs::create_dir_all(&base_dir).expect("create test directory");
        let state_file = base_dir.join("terminal-sessions.json");
        let registry_file = base_dir.join(TERMINAL_WORKSPACE_RESTORE_FILE_NAME);
        let registry = registry_from_json(
            r#"{"records":[
                {"session_id":"orphan-a","path":"/tmp","user_name":"root","name":"fixture-a","updated_at":30},
                {"session_id":"orphan-b","path":"/tmp","user_name":"root","name":"fixture-b","updated_at":20},
                {"session_id":"live-session","path":"/tmp","user_name":"root","name":"fixture-live","updated_at":10}
            ]}"#,
        );
        persist_terminal_workspace_restore_registry(&registry_file, &registry)
            .expect("write registry fixture");

        let manager = TerminalManager::new_with_environment_deferred_restore(
            state_file,
            TerminalEnvironmentSnapshot {
                workspace_root: base_dir.clone(),
                display_root: base_dir.clone(),
                user_profile: crate::runtime_paths::resolve_current_user_profile().unwrap_or_else(
                    || {
                        crate::runtime_paths::resolve_user_profile(
                            crate::runtime_paths::DEFAULT_USER_NAME,
                        )
                        .expect("default user should resolve")
                    },
                ),
                proxy_env: Vec::new(),
            },
            crate::quota_reset_cache::QuotaResetCache::new(),
        );

        // 孤儿记录可删除，且只影响目标记录。
        let removed = manager
            .delete_workspace_restore_record("orphan-a")
            .expect("delete orphan record");
        assert!(removed);
        let remaining =
            load_terminal_workspace_restore_registry(&registry_file).expect("reload registry");
        assert_eq!(remaining.records.len(), 2);
        assert!(
            remaining
                .records
                .iter()
                .all(|record| record.session_id != "orphan-a")
        );
        assert!(
            remaining
                .records
                .iter()
                .any(|record| record.session_id == "orphan-b")
        );
        assert!(
            remaining
                .records
                .iter()
                .any(|record| record.session_id == "live-session")
        );

        // 重复删除同一记录：幂等返回未删除。
        let repeated = manager
            .delete_workspace_restore_record("orphan-a")
            .expect("repeat delete should not fail");
        assert!(!repeated);

        // 存活会话的记录必须由删除会话流程清理，直接删除被拒绝。
        {
            let mut state = crate::lock_or_recover!(manager.state.write());
            state.sessions_by_id.insert(
                "live-session".to_string(),
                StoredTerminalSession {
                    id: "live-session".to_string(),
                    path: base_dir.clone(),
                    user_name: default_terminal_user_name(),
                    name: "fixture-live".to_string(),
                    previous_names: Vec::new(),
                    title: String::new(),
                    codex_api_preset_name: String::new(),
                    codex_api_base_url: String::new(),
                    origin: TerminalSessionOrigin::Normal,
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
        }
        let error = manager
            .delete_workspace_restore_record("live-session")
            .expect_err("live session record deletion must be refused");
        assert!(error.to_string().contains("仍存在"), "unexpected error: {error}");
        let after = load_terminal_workspace_restore_registry(&registry_file)
            .expect("reload registry after refusal");
        assert!(
            after
                .records
                .iter()
                .any(|record| record.session_id == "live-session")
        );

        fs::remove_dir_all(base_dir).expect("remove test directory");
    }
}

fn reserve_session_locked(
    state: &mut TerminalState,
    next_id: &AtomicU64,
    path: PathBuf,
    user_profile: &runtime_paths::UserProfile,
    codex_api_preset_name: String,
    codex_api_base_url: String,
    origin: TerminalSessionOrigin,
    owner_key: String,
) -> Result<StoredTerminalSession> {
    let ordinal = next_id.fetch_add(1, Ordering::SeqCst);
    let session_id = format!("s{ordinal}");
    let name_claims = used_auto_session_name_claims(state, &path, None);
    let initial_index = next_auto_session_start_index_for_create(state, &path);
    let (session_name, _) = next_available_auto_session_name(&path, initial_index, &name_claims)
        .ok_or_else(|| anyhow::anyhow!("1-100 范围内没有可用的自动终端编号。"))?;
    let session = StoredTerminalSession::new(
        session_id.clone(),
        path.clone(),
        user_profile.name.clone(),
        session_name,
        codex_api_preset_name,
        codex_api_base_url,
        origin,
        owner_key,
    );
    state
        .sessions_by_path
        .entry(path)
        .or_default()
        .push(session_id.clone());
    state.sessions_by_id.insert(session_id, session.clone());
    Ok(session)
}

pub(super) fn mark_session_opened_locked(state: &mut TerminalState, session_id: &str) -> bool {
    let Some(session) = state.sessions_by_id.get(session_id) else {
        return false;
    };
    let next_timestamp = current_timestamp_millis().max(session.created_at);
    if session.last_opened_at == next_timestamp {
        return false;
    }
    let group_token = session.group_token.clone();
    let group_fixed_order = session.group_fixed_order;
    if let Some(session) = state.sessions_by_id.get_mut(session_id) {
        session.last_opened_at = next_timestamp;
    }
    if !group_token.is_empty() && !group_fixed_order {
        let mut members: Vec<String> = state
            .sessions_by_id
            .values()
            .filter(|candidate| candidate.group_token == group_token)
            .map(|candidate| candidate.id.clone())
            .collect();
        if members.len() > 1 {
            members.sort_by(|left, right| {
                session_group_order(state, left)
                    .cmp(&session_group_order(state, right))
                    .then_with(|| compare_session_ids(left, right))
            });
            if let Some(index) = members.iter().position(|id| id == session_id) {
                let activated = members.remove(index);
                members.insert(0, activated);
                for (index, id) in members.iter().enumerate() {
                    if let Some(member) = state.sessions_by_id.get_mut(id) {
                        member.group_order = index as u64 + 1;
                    }
                }
            }
        }
    }
    true
}

fn create_session_locked(
    state: &mut TerminalState,
    next_id: &AtomicU64,
    path: PathBuf,
    user_profile: runtime_paths::UserProfile,
    terminal_startup_script: Option<String>,
    codex_api_preset_name: String,
    codex_api_base_url: String,
    origin: TerminalSessionOrigin,
    owner_key: String,
) -> Result<StoredTerminalSession> {
    let ordinal = next_id.fetch_add(1, Ordering::SeqCst);
    let session_id = format!("s{ordinal}");
    let name_claims = used_auto_session_name_claims(state, &path, None);
    let initial_index = next_auto_session_start_index_for_create(state, &path);
    let (session_name, _) = next_available_auto_session_name(&path, initial_index, &name_claims)
        .ok_or_else(|| anyhow::anyhow!("1-100 范围内没有可用的自动终端编号。"))?;
    create_fresh_backend_session(&session_id, &path, &user_profile)?;
    if let Some(script) = terminal_startup_script
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        send_backend_startup_script(&session_id, script)?;
    }
    let session = StoredTerminalSession::new(
        session_id.clone(),
        path.clone(),
        user_profile.name.clone(),
        session_name,
        codex_api_preset_name,
        codex_api_base_url,
        origin,
        owner_key,
    );

    state
        .sessions_by_path
        .entry(path)
        .or_default()
        .push(session_id.clone());
    state.sessions_by_id.insert(session_id, session.clone());

    Ok(session)
}

fn cleanup_legacy_command_env_dir(state_file: &Path) {
    let directory = state_file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(".terminal-command-env");
    match fs::remove_dir_all(&directory) {
        Ok(()) => info!(
            path = %directory.display(),
            "removed legacy terminal command environment directory"
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => warn!(
            "remove legacy terminal command environment directory {} failed: {error}",
            directory.display()
        ),
    }
}

pub(in crate::terminal) fn remove_legacy_codex_command_env_launchers(
    user_profile: &runtime_paths::UserProfile,
) -> Result<Vec<PathBuf>> {
    let bin_dir = user_profile.home.join(".local/bin");
    let mut removed = Vec::new();
    for file_name in ["codex", "webclx-codex"] {
        let path = bin_dir.join(file_name);
        let Ok(contents) = fs::read_to_string(&path) else {
            continue;
        };
        if !contents.starts_with(CODEX_COMMAND_ENV_WRAPPER_HEADER) {
            continue;
        }
        fs::remove_file(&path)
            .with_context(|| format!("删除旧 Codex 环境 wrapper 失败: {}", path.display()))?;
        removed.push(path);
    }
    Ok(removed)
}

fn cleanup_legacy_codex_launchers(user_profile: &runtime_paths::UserProfile) {
    match remove_legacy_codex_command_env_launchers(user_profile) {
        Ok(paths) => {
            for path in paths {
                info!(path = %path.display(), "removed legacy Codex command environment wrapper");
            }
        }
        Err(error) => warn!(
            "remove legacy Codex command environment wrapper for user {} failed: {error}",
            user_profile.name
        ),
    }
}

pub(super) fn next_auto_session_start_index_for_create(
    state: &TerminalState,
    path: &Path,
) -> usize {
    state
        .sessions_by_id
        .values()
        .filter(|session| session.path == path)
        .flat_map(|session| session_name_auto_indices(&session.name))
        .max()
        .map(|index| index.saturating_add(1))
        .unwrap_or(1)
}

fn ensure_live_session_locked(
    state: &mut TerminalState,
    stored: &StoredTerminalSession,
    user_profile: runtime_paths::UserProfile,
    proxy_env: Vec<(String, String)>,
    terminal_startup_script: Option<String>,
) -> Result<Arc<TerminalSession>> {
    if let Some(session) = state
        .live_sessions
        .get(&stored.id)
        .filter(|session| session.is_alive())
    {
        return Ok(session.clone());
    }

    ensure_backend_session(&stored.id, &stored.path, &user_profile)?;
    let session = TerminalSession::attach(stored, proxy_env, terminal_startup_script)?;
    state
        .live_sessions
        .insert(stored.id.clone(), session.clone());
    Ok(session)
}

fn stored_session_user_profile(
    stored: &StoredTerminalSession,
    fallback: &runtime_paths::UserProfile,
) -> runtime_paths::UserProfile {
    runtime_paths::resolve_user_profile(&stored.user_name).unwrap_or_else(|error| {
        warn!(
            "resolve stored terminal user {} for {} failed: {error}; fallback to {}",
            stored.user_name, stored.id, fallback.name
        );
        fallback.clone()
    })
}

#[cfg(windows)]
fn stored_session_survives_process_restart(_session_id: &str) -> bool {
    false
}

#[cfg(not(windows))]
fn stored_session_survives_process_restart(session_id: &str) -> bool {
    !matches!(tmux_session_status(session_id), TmuxSessionStatus::Missing)
}

#[cfg(windows)]
fn ensure_backend_session(
    _session_id: &str,
    _path: &Path,
    _user_profile: &runtime_paths::UserProfile,
) -> Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn ensure_backend_session(
    session_id: &str,
    path: &Path,
    user_profile: &runtime_paths::UserProfile,
) -> Result<()> {
    ensure_tmux_session(session_id, path, user_profile)
}

#[cfg(windows)]
fn create_fresh_backend_session(
    _session_id: &str,
    _path: &Path,
    _user_profile: &runtime_paths::UserProfile,
) -> Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn create_fresh_backend_session(
    session_id: &str,
    path: &Path,
    user_profile: &runtime_paths::UserProfile,
) -> Result<()> {
    create_fresh_tmux_session(session_id, path, user_profile)
}

#[cfg(windows)]
fn send_backend_startup_script(_session_id: &str, _script: &str) -> Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn send_backend_startup_script(session_id: &str, script: &str) -> Result<()> {
    super::tmux::send_tmux_startup_script(session_id, script)
}

#[cfg(windows)]
fn send_backend_input(state: &TerminalState, session_id: &str, data: &str) -> Result<()> {
    let Some(session) = state.live_sessions.get(session_id) else {
        anyhow::bail!("terminal session is not connected");
    };
    let writer = session.writer.clone();
    let mut writer = crate::lock_or_recover!(writer.lock());
    use std::io::Write;
    writer.write_all(data.as_bytes())?;
    writer.flush()?;
    Ok(())
}

#[cfg(not(windows))]
fn send_backend_input(_state: &TerminalState, session_id: &str, data: &str) -> Result<()> {
    send_tmux_input(session_id, data)
}

#[cfg(windows)]
fn kill_backend_session(_session_id: &str) -> Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn kill_backend_session(session_id: &str) -> Result<()> {
    destroy_tmux_session(session_id)
}

fn remove_session_from_path_locked(state: &mut TerminalState, path: &Path, session_id: &str) {
    let Some(existing_ids) = state.sessions_by_path.get(path).cloned() else {
        return;
    };

    let retained_ids: Vec<String> = existing_ids
        .into_iter()
        .filter(|existing_id| existing_id != session_id)
        .collect();

    if retained_ids.is_empty() {
        state.sessions_by_path.remove(path);
    } else {
        state
            .sessions_by_path
            .insert(path.to_path_buf(), retained_ids);
        refresh_auto_session_names_for_path_locked(state, path);
    }
}

#[cfg(test)]
mod explicit_session_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[allow(clippy::await_holding_lock)] // Deliberate disk-stall injection, released before assertions.
    #[tokio::test(flavor = "current_thread")]
    async fn terminal_persistence_slow_writer_does_not_block_input_or_state() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base_dir = std::env::temp_dir().join(format!("webclx-persistence-{unique}"));
        fs::create_dir_all(&base_dir).unwrap();
        let manager = TerminalManager::new(base_dir.join("sessions.json"));
        let session_id = {
            let mut state = manager.state.write().unwrap();
            reserve_session_locked(
                &mut state,
                &manager.next_id,
                base_dir.clone(),
                &manager.env_snapshot.user_profile,
                String::new(),
                String::new(),
                TerminalSessionOrigin::Normal,
                String::new(),
            )
            .unwrap()
            .id
        };
        // Simulate a stalled disk writer without starting PTYs or tmux processes.
        let writer_guard = manager.registry_write_lock.lock().unwrap();
        let input_manager = manager.clone();
        let input_id = session_id.clone();
        let input = tokio::task::spawn_blocking(move || {
            for index in 0..32 {
                input_manager
                    .record_session_input_deferred(&input_id, &format!("prompt {index}\r"));
            }
        });
        let completed = tokio::time::timeout(Duration::from_secs(5), input).await;
        let state_available = manager.state.try_write().is_ok();
        let worker_running = manager.input_persist_work.lock().unwrap().0;
        // Release the injected stall before assertions, including failure paths.
        drop(writer_guard);
        assert!(completed.is_ok_and(|result| result.is_ok()), "input waited for persistence");
        assert!(state_available, "disk writer retained the global state lock");
        assert!(worker_running, "committed history must schedule persistence");
        let flush_manager = manager.clone();
        tokio::time::timeout(
            Duration::from_secs(2),
            tokio::task::spawn_blocking(move || {
                flush_manager.persist_state();
            }),
        )
        .await
        .unwrap()
        .unwrap();
        let registry = load_terminal_registry(&manager.state_file).unwrap();
        let entries = &registry.input_histories[&session_id];
        assert_eq!(entries.len(), 32);
        assert_eq!(entries.last().unwrap().text, "prompt 31");
        // Wait for the coalesced worker to retire before removing its directory.
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if !manager.input_persist_work.lock().unwrap().0 {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        fs::remove_dir_all(base_dir).unwrap();
    }

    #[test]
    fn terminal_persistence_detects_commits_at_history_capacity() {
        let mut history = TerminalInputHistoryCapture::default();
        for index in 0..MAX_TERMINAL_INPUT_HISTORY_ENTRIES {
            assert!(capture_terminal_input_history(&mut history, &format!("prompt {index}\r")));
        }
        assert!(capture_terminal_input_history(&mut history, "newest prompt\r"));
        assert_eq!(history.entries.len(), MAX_TERMINAL_INPUT_HISTORY_ENTRIES);
        assert_eq!(history.entries.last().unwrap().text, "newest prompt");
        assert!(!capture_terminal_input_history(&mut history, "unfinished"));
    }

    #[test]
    fn payload_too_large_is_always_a_compact_recovery_error() {
        assert!(terminal_error_keyword_is_payload_too_large(
            "unexpected status 413 Payload Too Large: Failed to buffer the request body: length limit exceeded"
        ));
        assert!(terminal_error_keyword_is_payload_too_large(
            "request body length limit exceeded"
        ));
        assert!(!terminal_error_keyword_is_payload_too_large("429 Too Many Requests"));
    }

    #[test]
    fn compact_signature_guard_suppresses_only_the_sent_error() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let base_dir = std::env::temp_dir().join(format!("webclx-compact-guard-{unique}"));
        fs::create_dir_all(&base_dir).expect("create compact guard test directory");
        let manager = TerminalManager::new(base_dir.join("terminal-sessions.json"));
        let session_id = "s-compact-guard";
        let signature = "payload-error-signature";

        assert!(!manager.error_matches_previous_compact(session_id, signature));
        crate::lock_or_recover!(manager.per_session_runtime.lock())
            .entry(session_id.to_string())
            .or_default()
            .auto_compact_signature = Some(signature.to_string());
        assert!(manager.error_matches_previous_compact(session_id, signature));
        assert!(!manager.error_matches_previous_compact(session_id, "new-error-signature"));

        let _ = fs::remove_dir_all(base_dir);
    }

    #[test]
    fn deleted_explicit_session_does_not_create_replacement() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let base_dir =
            std::env::temp_dir().join(format!("webclx-deleted-session-connect-test-{unique}"));
        let workspace_dir = base_dir.join("workspace");
        fs::create_dir_all(&workspace_dir).expect("create test workspace");
        let manager = TerminalManager::new(base_dir.join("terminal-sessions.json"));

        let result = manager.get_session(
            &workspace_dir,
            "s-deleted",
            runtime_paths::resolve_user_profile(runtime_paths::DEFAULT_USER_NAME)
                .expect("resolve default terminal user"),
            Vec::new(),
            None,
            false,
        );

        assert!(
            result
                .as_ref()
                .is_err_and(|error| error.to_string().contains("s-deleted")
                    && error.to_string().contains("不存在")),
            "deleted explicit session id should be rejected"
        );
        assert!(
            manager
                .state
                .read()
                .expect("terminal session map poisoned")
                .sessions_by_id
                .is_empty(),
            "a rejected stale connection must not register a replacement session"
        );

        let _ = fs::remove_dir_all(base_dir);
    }
}

#[cfg(test)]
mod group_idle_tests {
    use super::*;

    fn make_stored_session(id: &str, group_token: &str, idle: bool) -> StoredTerminalSession {
        let mut session = StoredTerminalSession {
            idle,
            group_token: group_token.to_string(),
            ..crate::terminal::tests::stored_test_terminal_session(id)
        };
        session.name = format!("{id}_name");
        session
    }

    #[test]
    fn idling_one_member_moves_its_active_group_members_to_idle() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base_dir = std::env::temp_dir().join(format!("webclx-group-idle-{unique}"));
        fs::create_dir_all(&base_dir).unwrap();
        let manager = TerminalManager::new(base_dir.join("terminal-sessions.json"));
        {
            let mut state = manager.state.write().unwrap();
            state
                .sessions_by_id
                .insert("s1".to_string(), make_stored_session("s1", "", false));
            state
                .sessions_by_id
                .insert("s2".to_string(), make_stored_session("s2", "pair", false));
            state
                .sessions_by_id
                .insert("s3".to_string(), make_stored_session("s3", "pair", false));
            state
                .sessions_by_id
                .insert("s4".to_string(), make_stored_session("s4", "pair", true));
        }

        manager
            .set_session_idle(&base_dir, &base_dir, "s2", true)
            .unwrap();
        let state = manager.state.read().unwrap();
        assert!(!state.sessions_by_id["s1"].idle);
        assert!(state.sessions_by_id["s2"].idle);
        assert!(state.sessions_by_id["s3"].idle);
        assert!(state.sessions_by_id["s4"].idle);
    }

    #[test]
    fn restoring_one_member_does_not_restore_its_group() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base_dir = std::env::temp_dir().join(format!("webclx-group-restore-{unique}"));
        fs::create_dir_all(&base_dir).unwrap();
        let manager = TerminalManager::new(base_dir.join("terminal-sessions.json"));
        {
            let mut state = manager.state.write().unwrap();
            state
                .sessions_by_id
                .insert("s2".to_string(), make_stored_session("s2", "pair", true));
            state
                .sessions_by_id
                .insert("s3".to_string(), make_stored_session("s3", "pair", true));
        }

        manager
            .set_session_idle(&base_dir, &base_dir, "s2", false)
            .unwrap();
        let state = manager.state.read().unwrap();
        assert!(!state.sessions_by_id["s2"].idle);
        assert!(state.sessions_by_id["s3"].idle);
    }
}

#[cfg(test)]
mod session_hide_tests {
    use super::*;

    fn make_stored_session(id: &str, group_token: &str, idle: bool) -> StoredTerminalSession {
        StoredTerminalSession {
            idle,
            group_token: group_token.to_string(),
            ..crate::terminal::tests::stored_test_terminal_session(id)
        }
    }

    #[test]
    fn hiding_one_group_member_only_hides_that_session() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base_dir = std::env::temp_dir().join(format!("webclx-single-hide-{unique}"));
        fs::create_dir_all(&base_dir).unwrap();
        let manager = TerminalManager::new(base_dir.join("terminal-sessions.json"));
        {
            let mut state = manager.state.write().unwrap();
            state
                .sessions_by_id
                .insert("s1".to_string(), make_stored_session("s1", "pair", false));
            state
                .sessions_by_id
                .insert("s2".to_string(), make_stored_session("s2", "pair", false));
        }

        manager
            .set_session_hidden(&base_dir, &base_dir, "s1", true)
            .unwrap();
        let state = manager.state.read().unwrap();
        assert!(state.sessions_by_id["s1"].hidden);
        assert!(!state.sessions_by_id["s1"].idle);
        assert!(!state.sessions_by_id["s2"].hidden);
        assert!(!state.sessions_by_id["s2"].idle);
    }
}
