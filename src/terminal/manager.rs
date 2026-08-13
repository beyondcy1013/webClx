use std::{
    cmp::Ordering as CmpOrdering,
    collections::{HashMap, HashSet, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use settings_core::TerminalErrorKeywordAction;
use terminal_core::*;
use time::{Date, Month, OffsetDateTime, Time};
use tokio::sync::{Notify, broadcast};
use tracing::{info, warn};

use crate::{
    codex_launch::prepare_codex_history_command_for_user, filesystem, runtime_paths,
    settings::SettingsManager,
};

use super::{
    MAX_TERMINAL_INPUT_HISTORY_ENTRIES, MAX_TERMINAL_INPUT_HISTORY_LINE_BYTES,
    SESSION_EVENT_CHANNEL_CAPACITY, ScheduledInputUpdate,
    StoredTerminalAutoContinueScheduleRegistry, StoredTerminalPendingBuildRegistry,
    StoredTerminalRegistry, StoredTerminalScheduledInputRegistry, StoredTerminalSession,
    TERMINAL_PENDING_BUILD_MAX_AGE_MS, TERMINAL_RECENT_OUTPUT_ACTIVE_MS, TerminalActivitySnapshot,
    TerminalAutoContinueSchedule, TerminalAutoContinueSendOutcome, TerminalAutoContinueTask,
    TerminalEnvironmentSnapshot, TerminalErrorAutoContinueRecord, TerminalInputHistoryCapture,
    TerminalInputHistoryEntry, TerminalManager, TerminalManagerEvent, TerminalOutputObservation,
    TerminalPendingBuildRequest, TerminalPresetExtractionResponse, TerminalResumeRestoreRecord,
    TerminalScheduledInputTask, TerminalScheduledInputTaskInfo, TerminalSessionInfo,
    TerminalSessionOrigin, TerminalSessionSearchMatch, TerminalShutdownRestoreRegistry,
    TerminalState,
    activity::{TerminalAgentActivity, TerminalAgentDetector},
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
        capture_tmux_text_pane_snapshot, create_fresh_tmux_session,
        detach_tmux_clients_for_sessions, ensure_tmux_session, kill_tmux_session, send_tmux_input,
        tmux_session_status,
    },
};

mod crontab;
mod error_detection;
mod scheduled_input;
pub(in crate::terminal) use crontab::parse_auto_continue_due_epochs;

use crontab::{
    current_crontab, install_crontab, rewrite_crontab_without_markers,
    sanitize_cron_file_component, set_executable_mode, shell_quote_cron,
};

use scheduled_input::{
    load_terminal_scheduled_input_tasks, normalize_scheduled_input_text,
    persist_terminal_scheduled_input_registry, scheduled_input_task_info,
    scheduled_input_task_infos,
};

use error_detection::{
    TerminalErrorKeywordMatch, compact_terminal_search_line, count_non_overlapping_matches,
    terminal_error_keyword_match, terminal_error_keyword_match_from_snapshot,
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
/// Slash command that compacts the conversation to free context-window room.
const TERMINAL_COMPACT_COMMAND: &str = "/compact";
/// Delay between typing a slash command and the first Enter, mirroring the
/// frontend MOBILE_SLASH_COMMAND_ENTER_DELAY_MS so Codex registers the command.
const TERMINAL_SLASH_COMMAND_ENTER_DELAY_MS: u64 = 500;
/// Delay between the first and second Enter for a slash command, mirroring the
/// frontend MOBILE_SLASH_COMMAND_CONFIRM_DELAY_MS so Codex confirms the dialog.
const TERMINAL_SLASH_COMMAND_CONFIRM_DELAY_MS: u64 = 120;
/// Delay between sending /compact and the follow-up "继续": gives Codex time to
/// finish compacting the conversation before the next turn is submitted.
const TERMINAL_COMPACT_SETTLE_DELAY_MS: u64 = 3000;
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
            archive_file: Arc::new(archive_file),
            scheduled_input_file: Arc::new(scheduled_input_file),
            auto_continue_file: Arc::new(auto_continue_file),
            pending_build_file: Arc::new(pending_build_file),
            shutdown_restore_file: Arc::new(PathBuf::new()),
            env_snapshot: Arc::new(TerminalEnvironmentSnapshot {
                workspace_root: PathBuf::new(),
                display_root: PathBuf::new(),
                user_profile: runtime_paths::resolve_current_user_profile().unwrap_or_else(|| {
                    runtime_paths::resolve_user_profile(&default_terminal_user_name())
                        .expect("default terminal user should resolve")
                }),
                terminal_default_env: Vec::new(),
                proxy_env: Vec::new(),
            }),
            next_id: Arc::new(AtomicU64::new(next_ordinal.max(1))),
            event_sender,
            auto_continue_schedules: Arc::new(std::sync::Mutex::new(auto_continue_schedules)),
            auto_continue_notify: Arc::new(Notify::new()),
            canceled_auto_continue_signatures: Arc::new(std::sync::Mutex::new(HashSet::new())),
            error_auto_continue_records: Arc::new(std::sync::Mutex::new(HashMap::new())),
            auto_continue_last_sent_at: Arc::new(std::sync::Mutex::new(HashMap::new())),
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
            archive_file: Arc::new(archive_file),
            scheduled_input_file: Arc::new(scheduled_input_file),
            auto_continue_file: Arc::new(auto_continue_file),
            pending_build_file: Arc::new(pending_build_file),
            shutdown_restore_file: Arc::new(shutdown_restore_file),
            env_snapshot: Arc::new(env_snapshot),
            next_id: Arc::new(AtomicU64::new(next_ordinal.max(1))),
            event_sender,
            auto_continue_schedules: Arc::new(std::sync::Mutex::new(auto_continue_schedules)),
            auto_continue_notify: Arc::new(Notify::new()),
            canceled_auto_continue_signatures: Arc::new(std::sync::Mutex::new(HashSet::new())),
            error_auto_continue_records: Arc::new(std::sync::Mutex::new(HashMap::new())),
            auto_continue_last_sent_at: Arc::new(std::sync::Mutex::new(HashMap::new())),
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
        };

        if dirty {
            manager.persist_state();
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
        let mut state = crate::lock_or_recover!(self.state.write());
        state.sessions_by_id.insert(stored.id.clone(), stored);
        state.live_sessions.insert(live.id.clone(), live);
    }

    pub(in crate::terminal) fn session_path(&self, session_id: &str) -> Option<PathBuf> {
        let state = crate::lock_or_recover!(self.state.read());
        state
            .sessions_by_id
            .get(session_id)
            .map(|session| session.path.clone())
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
        let (session_ids, cleanup_dirty) = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let cleanup_dirty = cleanup_path_locked(&mut state, path);
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
        let (session_ids, cleanup_dirty) = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let cleanup_dirty = cleanup_all_locked(&mut state);
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
        terminal_default_env: Vec<(String, String)>,
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
            terminal_default_env,
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
        terminal_default_env: Vec<(String, String)>,
        proxy_env: Vec<(String, String)>,
        terminal_startup_script: Option<String>,
        codex_api_preset_name: String,
        codex_api_base_url: String,
        origin: TerminalSessionOrigin,
        owner_key: String,
    ) -> Result<TerminalSessionInfo> {
        cleanup_legacy_codex_launchers(&user_profile);
        let terminal_default_env = self.with_local_api_token_file(terminal_default_env);
        let session = self.create_session_inner(
            path,
            user_profile,
            terminal_default_env,
            proxy_env,
            terminal_startup_script,
            codex_api_preset_name,
            codex_api_base_url,
            origin,
            owner_key,
        )?;
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
            cleanup_all_locked(&mut state);

            let stored = state
                .sessions_by_id
                .get(session_id)
                .cloned()
                .with_context(|| format!("会话 `{session_id}` 不存在"))?;

            ensure_unique_session_name_locked(&state, &name, Some(session_id))?;

            let updated = StoredTerminalSession {
                name: name.clone(),
                manually_renamed: true,
                ..stored
            };

            state
                .sessions_by_id
                .insert(session_id.to_string(), updated.clone());

            if let Some(session) = state.live_sessions.get(session_id) {
                session.rename(name.clone());
            }

            self.persist_state_locked(&state);
            updated
        };

        self.notify_session_list_changed("renamed", session.id.clone());
        Ok(session.info(base_dir, display_root, TerminalActivitySnapshot::idle(0), false))
    }

    pub fn update_session_origin(
        &self,
        base_dir: &Path,
        display_root: &Path,
        session_id: &str,
        origin: TerminalSessionOrigin,
        owner_key: String,
    ) -> Result<TerminalSessionInfo> {
        let session = {
            let mut state = crate::lock_or_recover!(self.state.write());
            cleanup_all_locked(&mut state);

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
            self.persist_state_locked(&state);
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
        let session = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let stored = state
                .sessions_by_id
                .get_mut(session_id)
                .with_context(|| format!("会话 `{session_id}` 不存在"))?;
            stored.idle = idle;
            let session = stored.clone();
            self.persist_state_locked(&state);
            session
        };

        self.notify_session_list_changed(
            if idle { "idle" } else { "restored" },
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
        let session = {
            let mut state = crate::lock_or_recover!(self.state.write());
            cleanup_all_locked(&mut state);

            let stored = state
                .sessions_by_id
                .get(session_id)
                .cloned()
                .with_context(|| format!("会话 `{session_id}` 不存在"))?;
            self.archive_deleted_agent_session_name(base_dir, &stored);

            kill_backend_session(&stored.id)?;

            if let Some(session) = state.live_sessions.remove(session_id) {
                session.mark_closed(b"\r\n[webclx] session terminated.\r\n");
            }

            state.sessions_by_id.remove(session_id);
            state.input_histories.remove(session_id);
            remove_session_from_path_locked(&mut state, &stored.path, session_id);
            self.persist_state_locked(&state);
            stored
        };

        self.notify_session_list_changed("deleted", session.id.clone());
        Ok(session.info(base_dir, display_root, TerminalActivitySnapshot::idle(0), false))
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
        terminal_default_env: Vec<(String, String)>,
        proxy_env: Vec<(String, String)>,
        terminal_startup_script: Option<String>,
        codex_api_preset_name: String,
        codex_api_base_url: String,
        output_visible: bool,
    ) -> Result<Arc<TerminalSession>> {
        cleanup_legacy_codex_launchers(&user_profile);
        let terminal_default_env = self.with_local_api_token_file(terminal_default_env);
        let session = match session_id {
            Some(session_id) if !session_id.trim().is_empty() => self.get_session(
                &path,
                session_id,
                user_profile,
                terminal_default_env,
                proxy_env,
                terminal_startup_script,
            ),
            _ => self.get_or_create_latest(
                path,
                user_profile,
                terminal_default_env,
                proxy_env,
                terminal_startup_script,
                codex_api_preset_name,
                codex_api_base_url,
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
        terminal_default_env: Vec<(String, String)>,
        proxy_env: Vec<(String, String)>,
        terminal_startup_script: Option<String>,
    ) -> Result<Arc<TerminalSession>> {
        let mut changed = false;
        let mut change_action = "opened";
        let changed_session_id = session_id.to_string();
        let session = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let mut dirty = false;
            let stored = state
                .sessions_by_id
                .get(session_id)
                .cloned()
                .with_context(|| format!("终端会话 `{session_id}` 不存在"))?;

            if stored.path != *path {
                anyhow::bail!("会话不属于当前目录。");
            }

            let session_user_profile = stored_session_user_profile(&stored, &user_profile);
            let session = ensure_live_session_locked(
                &mut state,
                &stored,
                session_user_profile,
                terminal_default_env,
                proxy_env,
                terminal_startup_script,
            )?;
            let opened = mark_session_opened_locked(&mut state, &stored.id);
            dirty |= opened;
            if change_action == "opened" && !opened {
                change_action = "synced";
            }
            if dirty {
                self.persist_state_locked(&state);
                changed = true;
            }
            session
        };
        if changed {
            self.notify_session_list_changed(change_action, changed_session_id);
        }

        Ok(session)
    }

    fn get_or_create_latest(
        &self,
        path: PathBuf,
        user_profile: runtime_paths::UserProfile,
        terminal_default_env: Vec<(String, String)>,
        proxy_env: Vec<(String, String)>,
        terminal_startup_script: Option<String>,
        codex_api_preset_name: String,
        codex_api_base_url: String,
    ) -> Result<Arc<TerminalSession>> {
        let mut changed = false;
        let mut change_action = "opened";
        let mut changed_session_id = String::new();
        let session = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let mut dirty = cleanup_path_locked(&mut state, &path);
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
                        terminal_default_env.clone(),
                        proxy_env.clone(),
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
                terminal_default_env,
                proxy_env,
                terminal_startup_script,
            )?;
            let opened = mark_session_opened_locked(&mut state, &stored.id);
            dirty |= opened;
            if dirty {
                self.persist_state_locked(&state);
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
        terminal_default_env: Vec<(String, String)>,
        proxy_env: Vec<(String, String)>,
        terminal_startup_script: Option<String>,
        codex_api_preset_name: String,
        codex_api_base_url: String,
        origin: TerminalSessionOrigin,
        owner_key: String,
    ) -> Result<StoredTerminalSession> {
        let mut state = crate::lock_or_recover!(self.state.write());
        cleanup_path_locked(&mut state, &path);
        let session = create_session_locked(
            &mut state,
            &self.next_id,
            path,
            user_profile,
            terminal_default_env,
            proxy_env,
            terminal_startup_script,
            codex_api_preset_name,
            codex_api_base_url,
            origin,
            owner_key,
        )?;
        self.persist_state_locked(&state);
        Ok(session)
    }

    fn with_local_api_token_file(
        &self,
        terminal_default_env: Vec<(String, String)>,
    ) -> Vec<(String, String)> {
        let token_file = self
            .state_file
            .with_file_name(crate::auth_guard::LOCAL_API_TOKEN_FILE_NAME);
        let token = match crate::auth_guard::read_existing_local_api_token(&token_file) {
            Ok(token) => token,
            Err(error) => {
                warn!(
                    path = %token_file.display(),
                    "cannot load local API token for managed terminal: {error}"
                );
                let environment = with_authoritative_terminal_env(
                    terminal_default_env,
                    "WEBCLX_LOCAL_TOKEN_FILE",
                    token_file.to_string_lossy().into_owned(),
                );
                return without_terminal_env(environment, auth_core::WEBCLX_LOCAL_API_TOKEN_ENV);
            }
        };
        let environment = with_authoritative_terminal_env(
            terminal_default_env,
            "WEBCLX_LOCAL_TOKEN_FILE",
            token_file.to_string_lossy().into_owned(),
        );
        with_authoritative_terminal_env(environment, auth_core::WEBCLX_LOCAL_API_TOKEN_ENV, token)
    }

    fn persist_state(&self) {
        let state = crate::lock_or_recover!(self.state.read());
        self.persist_state_locked(&state);
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
        let terminal_default_env = self.with_local_api_token_file(Vec::new());

        let mut output_observation_dirty = false;
        for stored in stored_sessions {
            let user_profile = stored_session_user_profile(&stored, &fallback_user_profile);
            cleanup_legacy_codex_launchers(&user_profile);
            let mut state = crate::lock_or_recover!(self.state.write());
            if state
                .live_sessions
                .get(&stored.id)
                .is_some_and(|session| session.is_alive())
            {
                continue;
            }

            if let Err(error) = ensure_live_session_locked(
                &mut state,
                &stored,
                user_profile,
                terminal_default_env.clone(),
                Vec::new(),
                None,
            ) {
                warn!("restore terminal session {} failed: {error}", stored.id);
            } else {
                let snapshot_probe_sequence =
                    TERMINAL_ACTIVITY_PROBE_SEQUENCE.fetch_add(1, Ordering::SeqCst);
                let fingerprint = capture_tmux_recent_pane_snapshot(&stored.id)
                    .ok()
                    .map(|snapshot| terminal_output_fingerprint(&snapshot));
                output_observation_dirty |= prepare_restored_output_observation_locked(
                    &mut state,
                    &stored.id,
                    fingerprint,
                    snapshot_probe_sequence,
                );
            }
        }

        if output_observation_dirty {
            self.persist_state();
        }
    }

    fn restore_shutdown_sessions(&self, records: &[TerminalResumeRestoreRecord]) {
        if records.is_empty() {
            return;
        }

        let snapshot = self.env_snapshot.as_ref().clone();
        let terminal_default_env =
            self.with_local_api_token_file(snapshot.terminal_default_env.clone());
        let mut remaining_records = records.to_vec();
        let mut dirty = false;
        let mut state = crate::lock_or_recover!(self.state.write());

        for record in records {
            if state
                .live_sessions
                .get(&record.session_id)
                .is_some_and(|session| session.is_alive())
            {
                continue;
            }

            let stored = StoredTerminalSession {
                id: record.session_id.clone(),
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
                terminal_default_env.clone(),
                snapshot.proxy_env.clone(),
                None,
            ) {
                warn!("restore shutdown session {} failed: {error}", stored.id);
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
                    },
                );
                dirty = true;
            }

            let restore_command =
                match prepare_codex_history_command_for_user(&stored.user_name, &record.command) {
                    Ok(command) => command,
                    Err(error) => {
                        warn!("prepare shutdown restore command for {} failed: {error}", stored.id);
                        continue;
                    }
                };
            if let Err(error) = send_backend_startup_script(&stored.id, &restore_command) {
                warn!("send shutdown restore command for {} failed: {error}", stored.id);
                continue;
            }

            remaining_records.retain(|item| item.session_id != record.session_id);
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
    }

    pub fn finalize_output_observations_for_shutdown(&self) {
        let started_at = Instant::now();
        let session_ids = {
            let state = crate::lock_or_recover!(self.state.read());
            state.live_sessions.keys().cloned().collect::<Vec<_>>()
        };
        if let Err(error) = detach_tmux_clients_for_sessions(&session_ids) {
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
        self.persist_state_locked(&state);
        info!(
            elapsed_ms = started_at.elapsed().as_millis(),
            "terminal shutdown output snapshot completed"
        );
    }

    /// 收集并持久化当前活动 agent 会话的关机恢复记录，返回保存的记录数。
    ///
    /// 普通 SIGTERM 不执行这条完整保存路径；它用于"保存会话并关机"这类需要
    /// 先确认会话已落盘再触发关机的场景。
    /// 关机保存会话的正确性取决于：在 tmux server 和 codex/claude 子进程还活着时
    /// 完成检测与写盘，所以这个动作必须在 systemd 关机流程之前由用户主动触发。
    pub fn save_shutdown_restore_registry(&self) -> Result<usize> {
        let registry = self.collect_shutdown_restore_registry();
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

        let output = Command::new("systemctl")
            .arg("stop")
            .arg("webclx-tmux-*.scope")
            .output();

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
        let mut state = crate::lock_or_recover!(self.state.write());
        state.live_sessions.clear();
    }

    fn collect_shutdown_restore_registry(&self) -> TerminalShutdownRestoreRegistry {
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
            // 仍可能抓到 resume id。只有用户明确要求完整保存时，才允许对仍活跃但未探测
            // 到 resume id 的 agent 发送 Ctrl-C。普通 SIGTERM 不调用这条完整保存路径；
            // 它依赖 tmux scope 跨服务重启存活，不能中断工作中的会话。
            let mut detected = resume_detector
                .as_ref()
                .and_then(|detector| detector.detect(&stored.id));
            if detected.is_none() && agent_activity.is_active() {
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
        let tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());
        scheduled_input_task_infos(&tasks)
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
    ) -> Result<TerminalScheduledInputTaskInfo> {
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
            id: format!("paste-{now}-{}", self.next_id.fetch_add(1, Ordering::SeqCst)),
            session_id: session_id.to_string(),
            terminal_name,
            due_at_millis,
            created_at_millis: now,
            label: label.trim().to_string(),
            text: normalized_text,
            send_enter,
            task_type: task_type.trim().to_string(),
            working_dir: working_dir.trim().to_string(),
        };
        let info = scheduled_input_task_info(&task);
        {
            let mut tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());
            tasks.insert(task.id.clone(), task);
            self.persist_scheduled_input_tasks_locked(&tasks);
        }
        self.scheduled_input_notify.notify_one();
        Ok(info)
    }

    pub fn cancel_scheduled_input(&self, task_id: &str) -> Result<bool> {
        let removed = {
            let mut tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());
            let removed = tasks.remove(task_id).is_some();
            if removed {
                self.persist_scheduled_input_tasks_locked(&tasks);
            }
            removed
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
            let task = tasks
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
            if let Some((session_id, terminal_name)) = resolved_name {
                task.session_id = session_id;
                task.terminal_name = terminal_name;
            }
            let info = scheduled_input_task_info(task);
            self.persist_scheduled_input_tasks_locked(&tasks);
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
        loop {
            let now = current_timestamp_millis();

            // Avoid-window gate: when a high-price avoidance time range is
            // configured, any task that becomes due while the current local
            // time falls inside that range is deferred to the end of the
            // range instead of being fired immediately. Tasks already past
            // their original due time are rescheduled; the loop will wake up
            // at the window boundary to fire them.
            let avoid_window = self.scheduled_input_avoid_window();
            let defer_until = if !avoid_window.is_empty() && now_within_active_window(&avoid_window)
            {
                avoid_window_end_epoch_millis(&avoid_window)
            } else {
                None
            };

            let (due_tasks, next_due_at) = {
                let mut tasks = crate::lock_or_recover!(self.scheduled_input_tasks.lock());

                // If we are inside the avoid window, reschedule every task
                // that is due now (or overdue) to fire at the window end.
                if let Some(end_millis) = defer_until {
                    let mut rescheduled = 0usize;
                    for task in tasks.values_mut() {
                        if task.due_at_millis <= now {
                            task.due_at_millis = end_millis;
                            rescheduled += 1;
                        }
                    }
                    if rescheduled > 0 {
                        self.persist_scheduled_input_tasks_locked(&tasks);
                    }
                }

                // Only collect tasks that are actually due AND not blocked by
                // the avoid window (defer_until is None means no window active).
                let due_tasks = if defer_until.is_none() {
                    let due_ids = tasks
                        .values()
                        .filter(|task| task.due_at_millis <= now)
                        .map(|task| task.id.clone())
                        .collect::<Vec<_>>();
                    due_ids
                        .iter()
                        .filter_map(|task_id| tasks.remove(task_id))
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                if !due_tasks.is_empty() {
                    self.persist_scheduled_input_tasks_locked(&tasks);
                }
                let next_due_at = tasks.values().map(|task| task.due_at_millis).min();
                (due_tasks, next_due_at)
            };

            for task in due_tasks {
                if let Err(error) = self.fire_scheduled_input_task(&task) {
                    warn!(
                        "send scheduled terminal input {} to {} failed: {error}",
                        task.id, task.session_id
                    );
                }
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
                _ = self.scheduled_input_notify.notified() => {}
            }
        }
    }

    fn fire_scheduled_input_task(&self, task: &TerminalScheduledInputTask) -> Result<()> {
        if task.send_enter && terminal_continue_command_text(&task.text) {
            return self.send_session_continue(&task.session_id);
        }
        // Write the text body and the submit CR as two separate PTY writes with
        // a short delay between them, mirroring the frontend
        // submitTerminalPasteDialogAndSend path which calls sendTerminalInput
        // twice (once for the paste body, once for MOBILE_KEY_SEQUENCES.enter).
        // Codex/Claude are raw-mode TUIs: when the body and the CR arrive in a
        // single write the TUI folds the CR into the same input frame and treats
        // it as text, so the command only fills the input box but is never
        // submitted. Splitting the writes makes the CR a separate key event.
        let body = scheduled_input_body(task);
        self.send_session_input(&task.session_id, body)?;
        if task.send_enter {
            thread::sleep(Duration::from_millis(TERMINAL_COMMAND_ENTER_DELAY_MS));
            self.send_session_input(&task.session_id, "\r".to_string())?;
        }
        Ok(())
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

    pub fn send_session_continue(&self, session_id: &str) -> Result<()> {
        let mut last_sent = crate::lock_or_recover!(self.auto_continue_last_sent_at.lock());
        self.send_terminal_command_with_enter(session_id, TERMINAL_CONTINUE_COMMAND)?;
        last_sent.insert(session_id.to_string(), current_timestamp_millis());
        Ok(())
    }

    pub(super) fn send_session_auto_continue(
        &self,
        session_id: &str,
        effective_interval_millis: u64,
    ) -> Result<TerminalAutoContinueSendOutcome> {
        let interval_millis = effective_interval_millis.max(1);
        let now = current_timestamp_millis();
        let mut last_sent = crate::lock_or_recover!(self.auto_continue_last_sent_at.lock());
        if let Some(retry_at_millis) =
            auto_continue_retry_at_millis(last_sent.get(session_id).copied(), interval_millis, now)
        {
            return Ok(TerminalAutoContinueSendOutcome::Cooldown {
                last_sent_at_millis: last_sent.get(session_id).copied().unwrap_or(now),
                retry_at_millis,
            });
        }
        self.send_terminal_command_with_enter(session_id, TERMINAL_CONTINUE_COMMAND)?;
        last_sent.insert(session_id.to_string(), current_timestamp_millis());
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
        let action = resolve_terminal_error_keyword_action(&error_match.keyword, keyword_actions);
        match action.as_str() {
            "compact_then_continue" => {
                self.send_session_compact_then_continue(session_id)?;
                Ok(TerminalAutoContinueSendOutcome::CompactSent)
            }
            "mark_only" => Ok(TerminalAutoContinueSendOutcome::NotEligible),
            _ => {
                let effective_interval_millis = u64::from(interval_seconds.max(1)) * 1000;
                self.send_session_auto_continue(session_id, effective_interval_millis)
            }
        }
    }

    pub fn send_session_message(
        &self,
        target: &str,
        path: Option<&Path>,
        data: String,
        submit_enters: u8,
        bracketed_paste: bool,
    ) -> Result<(String, String)> {
        let target = target.trim();
        if target.is_empty() {
            anyhow::bail!("terminal target is empty");
        }

        let (session_id, session_name) = self.resolve_session_target(target, path)?;

        if submit_enters == 1 && terminal_continue_command_text(&data) {
            self.send_session_continue(&session_id)?;
            return Ok((session_id, session_name));
        }

        if !data.is_empty() {
            let body = prepare_terminal_message_body(&data, bracketed_paste);
            self.send_session_input_direct_or_backend(&session_id, body)?;
        }
        if submit_enters > 0 {
            thread::sleep(terminal_message_paste_settle_delay(&data, bracketed_paste));
        }
        for index in 0..submit_enters {
            self.send_session_input_direct_or_backend(&session_id, "\r".to_string())?;
            if index + 1 < submit_enters {
                thread::sleep(Duration::from_millis(120));
            }
        }
        Ok((session_id, session_name))
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
        self.send_session_input_direct_or_backend(session_id, command.to_string())?;
        thread::sleep(Duration::from_millis(TERMINAL_SLASH_COMMAND_ENTER_DELAY_MS));
        self.send_session_input_direct_or_backend(session_id, "\r".to_string())?;
        thread::sleep(Duration::from_millis(TERMINAL_SLASH_COMMAND_CONFIRM_DELAY_MS));
        self.send_session_input_direct_or_backend(session_id, "\r".to_string())
    }

    /// Compacts the conversation then sends "继续". Used when the context window
    /// is exhausted: sending "继续" alone would immediately re-trigger the same
    /// error, so `/compact` must reclaim room first. The settle delay gives
    /// Codex time to finish the compact operation before the next turn begins.
    fn send_session_compact_then_continue(&self, session_id: &str) -> Result<()> {
        self.send_terminal_slash_command(session_id, TERMINAL_COMPACT_COMMAND)?;
        thread::sleep(Duration::from_millis(TERMINAL_COMPACT_SETTLE_DELAY_MS));
        self.send_terminal_command_with_enter(session_id, TERMINAL_CONTINUE_COMMAND)?;
        let mut last_sent = crate::lock_or_recover!(self.auto_continue_last_sent_at.lock());
        last_sent.insert(session_id.to_string(), current_timestamp_millis());
        Ok(())
    }

    /// Sends input to a terminal without recording it to the per-session
    /// input history (the "本终端对话历史" panel).
    ///
    /// Used for webClx-auto-typed commands — initial session launch,
    /// quick-start, `reload_claude`, resume-command injection — which
    /// must type the line into the pane (so the agent actually starts)
    /// but should not clutter the user's history view.
    pub fn send_session_input_silent(&self, session_id: &str, data: String) -> Result<()> {
        self.write_session_input_inner(session_id, data, false)
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
        self.persist_state_locked(&state);
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
            self.persist_state_locked(&state);
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

    pub fn record_session_input(&self, session_id: &str, data: &str) {
        if data.is_empty() {
            return;
        }

        let mut state = crate::lock_or_recover!(self.state.write());
        if !state.sessions_by_id.contains_key(session_id) {
            return;
        }

        let changed = record_session_input_locked(&mut state, session_id, data);
        if changed {
            self.persist_state_locked(&state);
        }
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
            self.persist_state_locked(&state);
        }
    }

    pub fn mark_session_opened(&self, session_id: &str) {
        let changed = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let changed = mark_session_opened_locked(&mut state, session_id);
            if changed {
                self.persist_state_locked(&state);
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

    fn remember_canceled_auto_continue_signature(&self, session_id: &str, signature: &str) {
        crate::lock_or_recover!(self.canceled_auto_continue_signatures.lock())
            .insert(format!("{session_id}:{signature}"));
    }

    fn forget_canceled_auto_continue_signature(&self, session_id: &str, signature: &str) {
        crate::lock_or_recover!(self.canceled_auto_continue_signatures.lock())
            .remove(&format!("{session_id}:{signature}"));
    }

    fn active_canceled_auto_continue_keys(
        &self,
        sessions: &[TerminalSessionInfo],
    ) -> HashSet<String> {
        sessions
            .iter()
            .filter_map(|session| {
                let signature = session.activity_error_signature.as_deref()?.trim();
                if signature.is_empty() {
                    return None;
                }
                Some(format!("{}:{signature}", session.id))
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
        let consecutive_attempts = crate::lock_or_recover!(self.error_auto_continue_records.lock())
            .get(session_id.as_str())
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
            self.update_api_preset_snapshot(auth_manager.api_presets_snapshot());
            self.scan_error_auto_continue_sessions(&settings);
            tokio::time::sleep(Duration::from_millis(TERMINAL_ERROR_AUTO_CONTINUE_SCAN_MS)).await;
        }
    }

    fn scan_error_auto_continue_sessions(&self, settings: &SettingsManager) {
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
        let (session_ids, cleanup_dirty) = {
            let mut state = crate::lock_or_recover!(self.state.write());
            let cleanup_dirty = cleanup_all_locked(&mut state);
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
        let auto_continue_schedules = collect_terminal_auto_continue_schedules(&sessions, now);
        let active_canceled_keys = self.active_canceled_auto_continue_keys(&sessions);
        crate::lock_or_recover!(self.canceled_auto_continue_signatures.lock())
            .retain(|key| active_canceled_keys.contains(key));
        let dirty = cleanup_dirty || updated;
        let should_notify = dirty && should_notify_session_list_sync(cleanup_dirty, updated);
        if dirty {
            self.persist_state();
        }

        if should_notify {
            self.notify_session_list_changed("synced", "");
        }
        let canceled_keys =
            crate::lock_or_recover!(self.canceled_auto_continue_signatures.lock()).clone();
        let auto_continue_schedules = auto_continue_schedules
            .into_iter()
            .filter(|(session_id, schedule)| {
                !canceled_keys.contains(&format!("{session_id}:{}", schedule.signature))
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
            crate::lock_or_recover!(self.error_auto_continue_records.lock()).clear();
            return;
        }

        for session in sessions {
            self.maybe_send_error_auto_continue(&session, interval_seconds, now, &keyword_actions);
        }
    }

    fn prune_error_auto_continue_records(&self, sessions: &[TerminalSessionInfo]) {
        let active_error_ids = sessions
            .iter()
            .filter(|session| terminal_session_is_error_state(session))
            .map(|session| session.id.clone())
            .collect::<HashSet<_>>();
        let active_session_ids = sessions
            .iter()
            .map(|session| session.id.clone())
            .collect::<HashSet<_>>();

        crate::lock_or_recover!(self.error_auto_continue_records.lock()).retain(|session_id, _| {
            active_session_ids.contains(session_id) && active_error_ids.contains(session_id)
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
        crate::lock_or_recover!(self.auto_continue_last_sent_at.lock()).retain(
            |session_id, sent_at| {
                active_session_ids.contains(session_id)
                    && now.saturating_sub(*sent_at) <= max_retention_millis
            },
        );
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

        let existing = crate::lock_or_recover!(self.error_auto_continue_records.lock())
            .get(&session.id)
            .cloned();

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
            crate::lock_or_recover!(self.auto_continue_last_sent_at.lock())
                .entry(session.id.clone())
                .or_insert(now);
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
        let action = resolve_terminal_error_keyword_action(matched_keyword, keyword_actions);
        let outcome = match action.as_str() {
            "compact_then_continue" => self
                .send_session_compact_then_continue(&session.id)
                .map(|_| TerminalAutoContinueSendOutcome::CompactSent),
            "mark_only" => Ok(TerminalAutoContinueSendOutcome::NotEligible),
            _ => self.send_session_auto_continue(&session.id, effective_interval_millis),
        };
        match outcome {
            Ok(TerminalAutoContinueSendOutcome::CompactSent) => {
                let _ = self.event_sender.send(TerminalManagerEvent::Toast {
                    session_id: session.id.clone(),
                    message: format!(
                        "检测到终端“{}”上下文窗口已满，已发送 /compact 并继续。",
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
        let consecutive_attempts = crate::lock_or_recover!(self.error_auto_continue_records.lock())
            .get(session_id)
            .filter(|record| record.key == key)
            .map(|record| record.consecutive_attempts)
            .unwrap_or(0);
        self.remember_error_auto_continue_record_with_attempts(
            session_id,
            key,
            sent_at_millis,
            reset_at,
            consecutive_attempts,
        );
    }

    fn remember_error_auto_continue_record_with_attempts(
        &self,
        session_id: &str,
        key: String,
        sent_at_millis: u64,
        reset_at: String,
        consecutive_attempts: u32,
    ) {
        crate::lock_or_recover!(self.error_auto_continue_records.lock()).insert(
            session_id.to_string(),
            TerminalErrorAutoContinueRecord {
                key,
                sent_at_millis,
                reset_at,
                consecutive_attempts,
            },
        );
    }

    fn forget_error_auto_continue_record_if_matches(&self, session_id: &str, key: &str) {
        let mut records = crate::lock_or_recover!(self.error_auto_continue_records.lock());
        if records
            .get(session_id)
            .is_some_and(|record| record.key == key)
        {
            records.remove(session_id);
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

    fn persist_state_locked(&self, state: &TerminalState) {
        let registry = StoredTerminalRegistry {
            next_ordinal: self.next_id.load(Ordering::SeqCst).max(1),
            sessions: collect_stored_sessions(state),
            input_histories: collect_stored_input_histories(state),
            output_observations: collect_stored_output_observations(state),
        };

        if let Err(error) = persist_terminal_registry(&self.state_file, &registry) {
            warn!(
                "persist terminal session registry failed {}: {error}",
                self.state_file.display()
            );
        }
    }
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
    let before_len = history.entries.len();
    capture_terminal_input_history(history, data);
    history.entries.len() != before_len
}

pub(super) fn capture_terminal_input_history(
    history: &mut TerminalInputHistoryCapture,
    data: &str,
) {
    let bytes = strip_bracketed_paste_markers(data).into_bytes();
    let mut index = 0;

    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b'\r' | b'\n' => {
                commit_terminal_input_history_line(history);
                index += 1;
                if byte == b'\r' && index < bytes.len() && bytes[index] == b'\n' {
                    index += 1;
                }
            }
            0x08 | 0x7f => {
                history.buffer.pop();
                index += 1;
            }
            0x1b => {
                index = skip_terminal_escape_sequence(&bytes, index);
            }
            byte if byte < 0x20 => {
                index += 1;
            }
            _ => {
                let rest = std::str::from_utf8(&bytes[index..]).unwrap_or_default();
                if let Some(ch) = rest.chars().next() {
                    history.buffer.push(ch);
                    trim_terminal_input_buffer(history);
                    index += ch.len_utf8();
                } else {
                    break;
                }
            }
        }
    }
}

fn strip_bracketed_paste_markers(data: &str) -> String {
    data.replace("\u{1b}[200~", "").replace("\u{1b}[201~", "")
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

fn commit_terminal_input_history_line(history: &mut TerminalInputHistoryCapture) {
    let text = history.buffer.trim();
    if !text.is_empty() && !is_terminal_continue_line(text) {
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
}

fn trim_terminal_input_buffer(history: &mut TerminalInputHistoryCapture) {
    while history.buffer.len() > MAX_TERMINAL_INPUT_HISTORY_LINE_BYTES {
        history.buffer.remove(0);
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
            let (next_name, used_index) =
                next_available_auto_session_name(path, start_index, &name_claims);
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
    let probes = collect_session_activity_probes(
        live_sessions,
        error_line_limit,
        error_keywords,
        auto_continue_time_patterns,
        auto_continue_respect_manual_interrupt,
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
    pub(super) error_match: Option<TerminalErrorKeywordMatch>,
    pub(super) worked_status: bool,
    pub(super) pending_build: bool,
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

fn collect_session_activity_probes(
    live_sessions: Vec<(String, Option<Arc<TerminalSession>>)>,
    error_line_limit: u32,
    error_keywords: &[String],
    auto_continue_time_patterns: &[String],
    auto_continue_respect_manual_interrupt: bool,
) -> Vec<TerminalActivityProbe> {
    let agent_detector = TerminalAgentDetector::new();
    live_sessions
        .into_iter()
        .map(|(session_id, live_session)| {
            let live_last_output_at = live_session
                .as_ref()
                .map(|session| session.last_output_at())
                .unwrap_or(0);
            let snapshot_probe_sequence =
                TERMINAL_ACTIVITY_PROBE_SEQUENCE.fetch_add(1, Ordering::SeqCst);
            let snapshot = capture_tmux_activity_pane_snapshot(&session_id, error_line_limit).ok();
            let snapshot_fingerprint = snapshot.as_deref().map(terminal_output_fingerprint);
            let agent_activity = agent_detector.detect(&session_id);
            let working_status = snapshot
                .as_deref()
                .is_some_and(|snapshot| terminal_working_status_match_from_snapshot(snapshot, 10));
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
                && snapshot.as_deref().is_some_and(|snapshot| {
                    terminal_worked_status_match_from_snapshot(snapshot, 10)
                });

            TerminalActivityProbe {
                session_id,
                live_last_output_at,
                snapshot_fingerprint,
                snapshot_probe_sequence,
                agent_activity,
                working_status,
                error_match,
                worked_status,
                pending_build: false,
            }
        })
        .collect()
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

    if let Some(error_match) = probe.error_match.as_ref() {
        if error_match.continue_sent {
            return TerminalActivitySnapshot::retrying(
                error_match.keyword.clone(),
                error_match.signature.clone(),
                error_match.auto_continue_at.clone(),
                last_output_at,
            )
            .with_agent(activity_agent);
        }
        return TerminalActivitySnapshot::error(
            error_match.keyword.clone(),
            error_match.signature.clone(),
            error_match.continue_sent,
            error_match.input_queued,
            error_match.auto_continue_at.clone(),
            last_output_at,
        )
        .with_agent(activity_agent);
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

    if probe.agent_activity.is_active() {
        return TerminalActivitySnapshot::agent(
            probe.agent_activity.label(),
            activity_agent,
            last_output_at,
        );
    }

    TerminalActivitySnapshot::idle(last_output_at)
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

fn terminal_session_is_error_state(session: &TerminalSessionInfo) -> bool {
    matches!(session.activity_state.as_str(), "error" | "retrying")
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
fn now_within_active_window(window: &str) -> bool {
    let Some((start, end)) = window.split_once('-') else {
        return false;
    };
    let Some(start_min) = parse_hhmm_to_minutes(start.trim()) else {
        return false;
    };
    let Some(end_min) = parse_hhmm_to_minutes(end.trim()) else {
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

fn parse_hhmm_to_minutes(value: &str) -> Option<u32> {
    let (h, m) = value.split_once(':')?;
    let h: u32 = h.parse().ok()?;
    let m: u32 = m.parse().ok()?;
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Compute the epoch-millis timestamp of the end of the avoid window
/// `HH:MM-HH:MM` (24h). The end time is relative to today; if the end
/// minute has already passed today the window crosses midnight, so the
/// end falls tomorrow. Returns None on malformed input.
pub(in crate::terminal) fn avoid_window_end_epoch_millis(window: &str) -> Option<u64> {
    let (start, end) = window.split_once('-')?;
    let start_min = parse_hhmm_to_minutes(start.trim())?;
    let end_min = parse_hhmm_to_minutes(end.trim())?;
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
    let output = std::process::Command::new("date")
        .arg("-d")
        .arg(reset_at)
        .arg("+%s")
        .output()
        .ok()?;
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
    let output = Command::new("date")
        .arg("-d")
        .arg(format!("@{minute_epoch}"))
        .arg("+%M %H %d %m *")
        .output()
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

fn terminal_auto_continue_cron_script(
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
    format!(
        r#"#!/usr/bin/env bash
set -u
MARKER={marker_json}
SESSION_ID={session_id_json}
KEYWORD={keyword_json}
RESET_AT={reset_at_json}
TMUX_TARGET={tmux_target_json}
WEBCLX_BASE_URL="${{WEBCLX_BASE_URL:-http://127.0.0.1:${{WEBCLX_PORT:-11111}}}}"
sleep {sleep_seconds}
cleanup() {{
  current="$(crontab -l 2>/dev/null || true)"
  printf '%s\n' "$current" | grep -F -v "$MARKER" | crontab - 2>/dev/null || true
}}
trap cleanup EXIT
snapshot="$(tmux capture-pane -p -t "$TMUX_TARGET" -S -80 2>/dev/null || true)"
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
    let output = Command::new("date")
        .arg("-d")
        .arg(&date_string)
        .arg("+%s")
        .output()
        .ok()?;
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
            if let Some(reset_at) = self.zhipu_quota_reset_for_session(
                &session.codex_api_preset_name,
                &session.codex_api_base_url,
            ) {
                session.activity_error_auto_continue_at = Some(reset_at);
            }
        }
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

fn terminal_output_fingerprint(bytes: &[u8]) -> u64 {
    let mut hasher = DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
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

pub(super) fn sort_session_ids_by_recent_activity(
    state: &TerminalState,
    session_ids: &mut [String],
) {
    session_ids.sort_by(|left, right| compare_session_ids_by_recent_activity(state, left, right));
}

fn compare_session_ids_by_recent_activity(
    state: &TerminalState,
    left: &str,
    right: &str,
) -> CmpOrdering {
    session_last_opened_at(state, right)
        .cmp(&session_last_opened_at(state, left))
        .then_with(|| session_created_at(state, right).cmp(&session_created_at(state, left)))
        .then_with(|| session_sort_ordinal(right).cmp(&session_sort_ordinal(left)))
        .then_with(|| right.cmp(left))
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
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("terminal session registry filename is invalid")?;
    let nonce = TERMINAL_ACTIVITY_PROBE_SEQUENCE.fetch_add(1, Ordering::SeqCst);
    let temp_path = path.with_file_name(format!(".{file_name}.tmp-{}-{nonce}", std::process::id()));
    std::fs::write(&temp_path, content)
        .with_context(|| format!("cannot write {}", temp_path.display()))?;
    std::fs::rename(&temp_path, path)
        .with_context(|| format!("cannot replace {}", path.display()))?;
    Ok(())
}

pub(super) fn mark_session_opened_locked(state: &mut TerminalState, session_id: &str) -> bool {
    let Some(session) = state.sessions_by_id.get_mut(session_id) else {
        return false;
    };

    let next_timestamp = current_timestamp_millis().max(session.created_at);
    if session.last_opened_at == next_timestamp {
        return false;
    }

    session.last_opened_at = next_timestamp;
    true
}

fn create_session_locked(
    state: &mut TerminalState,
    next_id: &AtomicU64,
    path: PathBuf,
    user_profile: runtime_paths::UserProfile,
    terminal_default_env: Vec<(String, String)>,
    proxy_env: Vec<(String, String)>,
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
    let (session_name, _) = next_available_auto_session_name(&path, initial_index, &name_claims);
    let terminal_metadata_env = terminal_metadata_env(&session_id, &session_name);
    let terminal_default_env =
        prepare_terminal_session_env(terminal_default_env, terminal_metadata_env, &user_profile);
    create_fresh_backend_session(
        &session_id,
        &path,
        &user_profile,
        terminal_default_env,
        proxy_env,
    )?;
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

fn terminal_metadata_env(session_id: &str, session_name: &str) -> Vec<(String, String)> {
    vec![
        ("WEBCLX_TERMINAL_ID".to_string(), session_id.to_string()),
        ("WEBCLX_TERMINAL_NAME".to_string(), session_name.to_string()),
    ]
}

fn with_terminal_metadata_env(
    mut terminal_default_env: Vec<(String, String)>,
    metadata_env: Vec<(String, String)>,
) -> Vec<(String, String)> {
    terminal_default_env
        .retain(|(key, _)| !matches!(key.as_str(), "WEBCLX_TERMINAL_ID" | "WEBCLX_TERMINAL_NAME"));
    terminal_default_env.extend(metadata_env);
    terminal_default_env
}

fn with_authoritative_terminal_env(
    mut terminal_default_env: Vec<(String, String)>,
    key: &str,
    value: String,
) -> Vec<(String, String)> {
    terminal_default_env.retain(|(name, _)| name != key);
    terminal_default_env.push((key.to_string(), value));
    terminal_default_env
}

fn without_terminal_env(
    mut terminal_default_env: Vec<(String, String)>,
    key: &str,
) -> Vec<(String, String)> {
    terminal_default_env.retain(|(name, _)| name != key);
    terminal_default_env
}

fn with_authoritative_local_api_environment(
    terminal_default_env: Vec<(String, String)>,
    token_file: &Path,
) -> std::io::Result<Vec<(String, String)>> {
    let token = crate::auth_guard::read_existing_local_api_token(token_file)?;
    let environment = with_authoritative_terminal_env(
        terminal_default_env,
        "WEBCLX_LOCAL_TOKEN_FILE",
        token_file.to_string_lossy().into_owned(),
    );
    Ok(with_authoritative_terminal_env(
        environment,
        auth_core::WEBCLX_LOCAL_API_TOKEN_ENV,
        token,
    ))
}

fn prepare_terminal_session_env(
    terminal_default_env: Vec<(String, String)>,
    metadata_env: Vec<(String, String)>,
    _user_profile: &runtime_paths::UserProfile,
) -> Vec<(String, String)> {
    with_terminal_metadata_env(terminal_default_env, metadata_env)
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
    terminal_default_env: Vec<(String, String)>,
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

    let terminal_default_env = prepare_terminal_session_env(
        terminal_default_env,
        terminal_metadata_env(&stored.id, &stored.name),
        &user_profile,
    );
    ensure_backend_session(
        &stored.id,
        &stored.path,
        &user_profile,
        terminal_default_env,
        proxy_env.clone(),
    )?;
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
fn backend_session_status(state: &TerminalState, session_id: &str) -> TmuxSessionStatus {
    match state.live_sessions.get(session_id) {
        Some(session) if !session.is_alive() => TmuxSessionStatus::Missing,
        _ => TmuxSessionStatus::Unknown,
    }
}

#[cfg(not(windows))]
fn backend_session_status(_state: &TerminalState, session_id: &str) -> TmuxSessionStatus {
    tmux_session_status(session_id)
}

#[cfg(windows)]
fn ensure_backend_session(
    _session_id: &str,
    _path: &Path,
    _user_profile: &runtime_paths::UserProfile,
    _terminal_default_env: Vec<(String, String)>,
    _proxy_env: Vec<(String, String)>,
) -> Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn ensure_backend_session(
    session_id: &str,
    path: &Path,
    user_profile: &runtime_paths::UserProfile,
    terminal_default_env: Vec<(String, String)>,
    proxy_env: Vec<(String, String)>,
) -> Result<()> {
    ensure_tmux_session(session_id, path, user_profile, terminal_default_env, proxy_env)
}

#[cfg(windows)]
fn create_fresh_backend_session(
    _session_id: &str,
    _path: &Path,
    _user_profile: &runtime_paths::UserProfile,
    _terminal_default_env: Vec<(String, String)>,
    _proxy_env: Vec<(String, String)>,
) -> Result<()> {
    Ok(())
}

#[cfg(not(windows))]
fn create_fresh_backend_session(
    session_id: &str,
    path: &Path,
    user_profile: &runtime_paths::UserProfile,
    terminal_default_env: Vec<(String, String)>,
    proxy_env: Vec<(String, String)>,
) -> Result<()> {
    create_fresh_tmux_session(session_id, path, user_profile, terminal_default_env, proxy_env)
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
    kill_tmux_session(session_id)
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

fn cleanup_path_locked(state: &mut TerminalState, path: &PathBuf) -> bool {
    let Some(existing_ids) = state.sessions_by_path.get(path).cloned() else {
        return false;
    };

    let mut retained_ids = Vec::with_capacity(existing_ids.len());
    let mut dirty = false;
    for session_id in existing_ids {
        match state.sessions_by_id.get(&session_id) {
            Some(session) => match backend_session_status(state, &session.id) {
                TmuxSessionStatus::Exists | TmuxSessionStatus::Unknown => {
                    if state
                        .live_sessions
                        .get(&session_id)
                        .is_some_and(|session| !session.is_alive())
                    {
                        state.live_sessions.remove(&session_id);
                    }
                    retained_ids.push(session_id);
                }
                TmuxSessionStatus::Missing => {
                    state.sessions_by_id.remove(&session_id);
                    state.live_sessions.remove(&session_id);
                    state.input_histories.remove(&session_id);
                    dirty = true;
                }
            },
            None => {
                state.sessions_by_id.remove(&session_id);
                state.live_sessions.remove(&session_id);
                state.input_histories.remove(&session_id);
                dirty = true;
            }
        }
    }

    if retained_ids.is_empty() {
        if state.sessions_by_path.remove(path).is_some() {
            dirty = true;
        }
    } else {
        let unchanged = state
            .sessions_by_path
            .get(path)
            .is_some_and(|existing| *existing == retained_ids);
        if !unchanged {
            state.sessions_by_path.insert(path.clone(), retained_ids);
            dirty = true;
        }

        dirty |= refresh_auto_session_names_for_path_locked(state, path);
    }

    dirty
}

fn cleanup_all_locked(state: &mut TerminalState) -> bool {
    let paths: Vec<PathBuf> = state.sessions_by_path.keys().cloned().collect();
    let mut dirty = false;

    for path in paths {
        dirty |= cleanup_path_locked(state, &path);
    }

    dirty
}

#[cfg(test)]
mod explicit_session_tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn authoritative_terminal_environment_replaces_user_supplied_value() {
        let environment = with_authoritative_terminal_env(
            vec![
                ("KEEP".to_string(), "yes".to_string()),
                ("WEBCLX_LOCAL_TOKEN_FILE".to_string(), "/tmp/forged".to_string()),
            ],
            "WEBCLX_LOCAL_TOKEN_FILE",
            "/runtime/.webclx-local-api-token".to_string(),
        );

        assert_eq!(
            environment,
            vec![
                ("KEEP".to_string(), "yes".to_string()),
                (
                    "WEBCLX_LOCAL_TOKEN_FILE".to_string(),
                    "/runtime/.webclx-local-api-token".to_string()
                ),
            ]
        );
    }

    #[test]
    fn local_api_token_environment_is_loaded_at_runtime_and_replaces_forged_value() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before unix epoch")
            .as_nanos();
        let base_dir = std::env::temp_dir().join(format!("webclx-local-token-env-{unique}"));
        fs::create_dir_all(&base_dir).expect("create token test directory");
        let token_file = base_dir.join(crate::auth_guard::LOCAL_API_TOKEN_FILE_NAME);
        let token = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
        fs::write(&token_file, format!("{token}\n")).expect("write token fixture");

        let environment = with_authoritative_local_api_environment(
            vec![("WEBCLX_LOCAL_API_TOKEN".to_string(), "forged-token".to_string())],
            &token_file,
        )
        .expect("load authoritative local token environment");

        assert_eq!(
            environment,
            vec![
                ("WEBCLX_LOCAL_TOKEN_FILE".to_string(), token_file.to_string_lossy().into_owned(),),
                ("WEBCLX_LOCAL_API_TOKEN".to_string(), token.to_string(),),
            ]
        );
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
            Vec::new(),
            None,
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
