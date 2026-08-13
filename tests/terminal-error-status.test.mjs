import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const appJs = readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const appSettingsLoadSaveJs = readFileSync(
  new URL("../static/app-settings-load-save.js", import.meta.url),
  "utf8",
);
const appSettingsEventBindingsJs = readFileSync(
  new URL("../static/app-settings-event-bindings.js", import.meta.url),
  "utf8",
);
const appAutoContinueTasksJs = readFileSync(
  new URL("../static/app-auto-continue-tasks.js", import.meta.url),
  "utf8",
);
const appHomeSessionRenderJs = readFileSync(
  new URL("../static/app-home-session-render.js", import.meta.url),
  "utf8",
);
const terminalSettingsJs = readFileSync(
  new URL("../static/terminal-settings.js", import.meta.url),
  "utf8",
);
const terminalSettingsLoaderJs = readFileSync(
  new URL("../static/terminal-settings-loader.js", import.meta.url),
  "utf8",
);
const terminalJs = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const terminalPasteJs = readFileSync(new URL("../static/terminal-paste.js", import.meta.url), "utf8");
const terminalAutoContinueJs = readFileSync(
  new URL("../static/terminal-auto-continue.js", import.meta.url),
  "utf8",
);
const terminalSessionActivityJs = readFileSync(
  new URL("../static/terminal-session-activity.js", import.meta.url),
  "utf8",
);
const settingsCore = readFileSync(new URL("../crates/settings_core/src/lib.rs", import.meta.url), "utf8");
const mainRs = readFileSync(new URL("../src/main.rs", import.meta.url), "utf8");
const terminalRoutesRs = readFileSync(new URL("../src/routes/terminal.rs", import.meta.url), "utf8");
const terminalRs = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");
const terminalManagerRs = readFileSync(new URL("../src/terminal/manager.rs", import.meta.url), "utf8");
const terminalManagerCrontabRs = readFileSync(
  new URL("../src/terminal/manager/crontab.rs", import.meta.url),
  "utf8",
);
const quotaResetCacheRs = readFileSync(new URL("../src/quota_reset_cache.rs", import.meta.url), "utf8");
const terminalManagerErrorDetectionRs = readFileSync(
  new URL("../src/terminal/manager/error_detection.rs", import.meta.url),
  "utf8",
);
const scheduleAutoContinueStart = terminalAutoContinueJs.indexOf(
  "function scheduleAutoContinueAtResetTime",
);
const sendContinueStart = terminalAutoContinueJs.indexOf(
  "async function sendContinueToSession",
  scheduleAutoContinueStart,
);
const scheduleAutoContinueFunction = terminalAutoContinueJs.slice(
  scheduleAutoContinueStart,
  sendContinueStart,
);
const appSettingsSource = `${appJs}\n${appSettingsLoadSaveJs}\n${appSettingsEventBindingsJs}`;

assert.match(
  indexHtml,
  /id="terminal-error-line-limit-input"[\s\S]*placeholder="12"[\s\S]*id="terminal-error-keyword-rules-body"/,
  "settings page should expose terminal error status configuration",
);

assert.match(
  indexHtml,
  /id="server-port-auto-increment-input"/,
  "settings page should expose server port auto-increment configuration",
);

assert.match(
  appSettingsSource,
  /terminal_error_match_line_limit: nextTerminalErrorMatchLineLimit,[\s\S]*terminal_auto_continue_respect_manual_interrupt:[\s\S]*nextTerminalAutoContinueRespectManualInterrupt,[\s\S]*terminal_error_keywords: nextTerminalErrorKeywords/,
  "settings save should persist terminal error status configuration",
);

assert.match(
  appSettingsSource,
  /terminal_auto_continue_backoff_factor: nextTerminalAutoContinueBackoffFactor,[\s\S]*terminal_auto_continue_backoff_max_minutes: nextTerminalAutoContinueBackoffMaxMinutes/,
  "settings save should persist the configurable auto-continue backoff cap",
);

assert.match(
  settingsCore,
  /DEFAULT_TERMINAL_ERROR_MATCH_LINE_LIMIT: u32 = 12[\s\S]*DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS: u32 = 60[\s\S]*DEFAULT_TERMINAL_AUTO_CONTINUE_RESPECT_MANUAL_INTERRUPT: bool = true[\s\S]*DEFAULT_SERVER_PORT_AUTO_INCREMENT: bool = true[\s\S]*terminal_auto_continue_interval_seconds[\s\S]*terminal_auto_continue_respect_manual_interrupt[\s\S]*terminal_error_keywords/,
  "settings core should carry terminal error defaults, auto-continue interval, concurrency-limit, selected-model-capacity, quota 429, port auto-increment, and default HTTP/status keywords",
);

assert.match(
  settingsCore,
  /fn merge_builtin_terminal_error_keywords\(keywords: &\[String\]\) -> Vec<String>[\s\S]*default_terminal_error_keywords\(\)[\s\S]*merged\.push\(keyword\)/,
  "settings core should merge newly added built-in terminal error keywords into saved legacy settings",
);

assert.match(
  terminalSettingsJs,
  /const DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS = 60[\s\S]*const DEFAULT_TERMINAL_AUTO_CONTINUE_RESPECT_MANUAL_INTERRUPT = true/,
  "terminal settings fallback defaults should include the auto-continue interval",
);

assert.match(
  terminalSettingsJs,
  /const DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_MAX_MINUTES = 20[\s\S]*function normalizeTerminalAutoContinueBackoffMaxMinutes\(value\)[\s\S]*Math\.min\(1440, Math\.max\(1, parsed\)\)/,
  "terminal settings should default the configurable backoff cap to 20 minutes",
);

assert.match(
  appJs,
  /const DEFAULT_TERMINAL_ERROR_KEYWORDS = Object\.freeze\(\[[\s\S]*Concurrency limit exceeded for user, please retry later[\s\S]*Selected model is at capacity\. Please try a different model\.[\s\S]*API Error: Request rejected \(429\)[\s\S]*已达到 5 小时的使用上限[\s\S]*last status: 429[\s\S]*last status: 503[\s\S]*last status: 404[\s\S]*unexpected status 502 Bad Gateway/,
  "terminal settings fallback defaults should include auto-continue interval, concurrency-limit, selected-model-capacity, quota 429, and 502 upstream unavailable error keywords",
);

assert.match(
  appJs,
  /DEFAULT_TERMINAL_ERROR_KEYWORD_ACTIONS[\s\S]*last status: 404[\s\S]*MARK_ONLY[\s\S]*404 Not Found[\s\S]*MARK_ONLY/,
  "404 routing and model errors must be marked without automatic retries",
);

assert.match(
  settingsCore,
  /default_terminal_error_keyword_actions[\s\S]*last status: 404[\s\S]*TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY[\s\S]*404 Not Found[\s\S]*TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY[\s\S]*merge_builtin_terminal_error_keyword_actions[\s\S]*existing\.action = builtin\.action/,
  "saved legacy 404 continue rules should migrate to the built-in mark-only guard",
);

assert.match(
  terminalAutoContinueJs,
  /embeddedAgent[\s\S]*\[activeSession\(\)\]\.filter\(Boolean\)[\s\S]*session\?\.origin === "normal"/,
  "automatic terminal actions must stay inside the active Agent conversation or normal-terminal scope",
);

assert.match(
  appJs,
  /"stream disconnected before completion:",\s*"Concurrency limit exceeded for user, please retry later"/,
  "terminal settings fallback defaults should match every stream-disconnect reason without treating MCP startup warnings as errors",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /NONFATAL_MCP_STARTUP_SUMMARY[\s\S]*summary_order >= best_match\.order[\s\S]*return None/,
  "terminal error detection should suppress nonfatal MCP startup failures instead of marking the session as error",
);

assert.match(
  indexHtml,
  /id="terminal-auto-continue-interval-input"[\s\S]*min="1"[\s\S]*placeholder="60"[\s\S]*id="terminal-auto-continue-respect-manual-interrupt-input"[\s\S]*id="terminal-error-keyword-rules-body"/,
  "settings page should expose a one-minute auto-continue cooldown and manual-interrupt toggle",
);

assert.match(
  indexHtml,
  /id="terminal-auto-continue-backoff-factor-input"[\s\S]*id="terminal-auto-continue-backoff-max-minutes-input"[\s\S]*min="1"[\s\S]*max="1440"[\s\S]*placeholder="20"/,
  "settings page should expose the auto-continue backoff cap in minutes",
);

assert.match(
  settingsCore,
  /DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_MAX_MINUTES: u32 = 20[\s\S]*terminal_auto_continue_backoff_max_minutes/,
  "settings core should persist a 20-minute default auto-continue backoff cap",
);

assert.match(
  terminalManagerRs,
  /fn auto_continue_backoff_interval_millis\([\s\S]*backoff_max_millis: u64[\s\S]*let effective_max = backoff_max_millis\.max\(base\)/,
  "backend backoff should use the live configured cap without shortening the base interval",
);

assert.match(
  indexHtml,
  /id="terminal-auto-continue-time-patterns-input"[\s\S]*\{time\}[\s\S]*id="terminal-error-keyword-rules-body"/,
  "settings page should expose configurable auto-continue reset-time formats using a {time} placeholder",
);

assert.match(
  indexHtml,
  /id="settings-tab-auto-continue-tasks"[\s\S]*data-settings-category="tasks"[\s\S]*id="settings-panel-auto-continue-tasks"[\s\S]*id="unified-task-refresh"[\s\S]*任务类型[\s\S]*id="unified-task-list"[\s\S]*session ID[\s\S]*tmux 会话/,
  "settings page should expose a dedicated task category with task type and readable terminal identifiers",
);

assert.match(
  terminalRoutesRs,
  /\/api\/terminal\/auto-continue-tasks[\s\S]*get\(terminal::list_auto_continue_tasks\)/,
  "backend should expose a read-only terminal auto-continue task API",
);

assert.match(
  terminalRs,
  /TerminalAutoContinueTaskInfo[\s\S]*task_kind: String[\s\S]*task_label: String[\s\S]*session_id: String[\s\S]*webclx_terminal_name: Option<String>[\s\S]*tmux_session_name: String[\s\S]*pub async fn list_auto_continue_tasks\([\s\S]*TerminalAutoContinueTasksResponse[\s\S]*auto_continue_tasks/,
  "terminal API should list parsed terminal crontab entries with task type, webClx names, and tmux names",
);

assert.match(
  terminalSettingsJs,
  /function normalizeTerminalAutoContinueIntervalSeconds\(value\)[\s\S]*parsed <= 0[\s\S]*DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS[\s\S]*Math\.min\(86400, Math\.max\(1, parsed\)\)/,
  "terminal settings helper should migrate the legacy zero interval to the one-minute default",
);

assert.match(
  appSettingsSource,
  /terminal_auto_continue_interval_seconds: nextTerminalAutoContinueIntervalSeconds[\s\S]*terminal_auto_continue_respect_manual_interrupt:[\s\S]*nextTerminalAutoContinueRespectManualInterrupt[\s\S]*terminal_auto_continue_time_patterns: nextTerminalAutoContinueTimePatterns/,
  "settings save should persist the cooldown, manual-interrupt policy, and reset-time patterns",
);

assert.match(
  `${terminalJs}\n${terminalSettingsLoaderJs}`,
  /terminalAutoContinueIntervalSeconds: DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS[\s\S]*state\.terminalAutoContinueIntervalSeconds = normalizeTerminalAutoContinueIntervalSeconds\([\s\S]*settings\.terminal_auto_continue_interval_seconds[\s\S]*\)/,
  "terminal page should load the configured timed auto-continue interval",
);

assert.match(
  `${terminalJs}\n${terminalAutoContinueJs}`,
  /TERMINAL_AUTO_CONTINUE_SCHEDULE_ACK_STORAGE_KEY[\s\S]*function hasAutoContinueScheduleAck\(sessionId, resetAt[\s\S]*function scheduleAutoContinueAtResetTime\(session, errorKey, resetAt\)[\s\S]*existing\?\.resetAt === resetAt \|\| hasAutoContinueScheduleAck\(session\.id, resetAt, now\)[\s\S]*return true;[\s\S]*rememberAutoContinueScheduleAck\(session\.id, resetAt, now\)[\s\S]*已添加定时，将在重置后 1 分钟发送/,
  "terminal page should acknowledge each parsed reset-time auto-continue schedule once per terminal and reset time",
);

assert.doesNotMatch(
  scheduleAutoContinueFunction,
  /autoContinueHandledKey\(existing\) === errorKey && existing\?\.resetAt === resetAt/,
  "scheduled auto-continue prompt suppression should not depend on a stable error signature",
);

assert.doesNotMatch(
  scheduleAutoContinueFunction,
  /window\.setTimeout/,
  "terminal page should not create its own browser timer for parsed reset times",
);

assert.match(
  terminalManagerRs,
  /terminal_auto_continue_cron_marker_prefix\(session_id\)[\s\S]*terminal_auto_continue_due_marker_prefix\(session_id\)[\s\S]*filter\(\|line\| !line\.contains\(&marker_prefix\) && !line\.contains\(&due_marker_prefix\)\)[\s\S]*next_lines\.push\(format!\("# \{marker\}"\)\)/,
  "backend cron scheduling should replace any existing auto-continue cron entry and due comment for the same terminal",
);

assert.match(
  terminalManagerRs,
  /self\.schedule_auto_continue_tasks\([\s\S]*self\.prune_error_auto_continue_records\([\s\S]*if !immediate_auto_continue_enabled \|\| !active_window_allows_immediate \{[\s\S]*return;[\s\S]*self\.maybe_send_error_auto_continue/,
  "backend should persist reset-time tasks before the gates and only run immediate retries when the auto-continue setting and active window both allow them",
);

assert.match(
  quotaResetCacheRs,
  /pub fn record_for_preset\(&self, preset_id: &str, base_url: &str, reset_at: String\)[\s\S]*base_url_cache_key\(base_url\)[\s\S]*pub fn get_for_base_url\(&self, base_url: &str\) -> Option<String>[\s\S]*fn cache_records_base_url_fallback\(\)[\s\S]*cache\.get_for_base_url\("https:\/\/OPEN\.bigmodel\.cn\/api\/coding\/paas\/v4"\)/,
  "quota reset cache should record preset reset times by both preset id and normalized base URL",
);

assert.match(
  terminalManagerRs,
  /zhipu_quota_reset_for_session\([\s\S]*get_for_preset\(&preset\.id\)[\s\S]*get_for_base_url\(&preset\.base_url\)[\s\S]*get_for_base_url\(fallback_base\)/,
  "terminal scanner should fall back to session base URL when preset-id reset lookup is unavailable",
);

assert.match(
  terminalManagerRs,
  /pub fn send_session_continue\(&self, session_id: &str\) -> Result<\(\)>[\s\S]*send_terminal_command_with_enter\(session_id, TERMINAL_CONTINUE_COMMAND\)/,
  "terminal manager should expose one canonical continue sender",
);

assert.match(
  terminalManagerRs,
  /fn send_terminal_command_with_enter\([\s\S]*self\.send_session_input_direct_or_backend\(session_id, command\.to_string\(\)\)[\s\S]*thread::sleep\(Duration::from_millis\(TERMINAL_COMMAND_ENTER_DELAY_MS\)\)[\s\S]*self\.send_session_input_direct_or_backend\(session_id, "\\r"\.to_string\(\)\)/,
  "canonical command submission should send text and Enter as separate writes like the slash soft keyboard",
);

assert.match(
  terminalManagerRs,
  /fn run_due_auto_continue_task\([\s\S]*send_session_auto_continue\(session_id, effective_interval_millis\)/,
  "reset-time auto-continue should use the cooldown-aware sender",
);

assert.match(
  terminalManagerRs,
  /fn maybe_send_error_auto_continue\([\s\S]*self\.send_session_auto_continue\(&session\.id, effective_interval_millis\)/,
  "immediate error auto-continue should use the cooldown-aware sender",
);

assert.match(
  terminalManagerRs,
  /fn terminal_auto_continue_cron_script\([\s\S]*curl[\s\S]*\/api\/terminal\/sessions\/\$\{\{SESSION_ID_ENCODED\}\}\/auto-continue/,
  "cron fallback should call the cooldown-aware backend API instead of the manual continue API",
);

assert.match(
  terminalAutoContinueJs,
  /\/auto-continue[\s\S]*existing && !autoContinueRetryDue\(existing\)[\s\S]*result\?\.sent === false/,
  "terminal-page automatic retries should use the cooldown-aware endpoint and handle cooldown responses",
);

assert.match(
  terminalManagerRs,
  /if auto_continue_task_matches\([\s\S]*self\.auto_continue_cron_needs_refresh\(&session_id, &schedule\)[\s\S]*self\.install_auto_continue_cron\(&session_id, &schedule\)[\s\S]*return;/,
  "matching persisted auto-continue tasks should refresh stale cron scripts before returning",
);

assert.match(
  terminalManagerRs,
  /fn auto_continue_cron_needs_refresh\([\s\S]*auto_continue_crontab_has_stale_due_markers\(session_id, schedule\)/,
  "matching persisted auto-continue tasks should check crontab due comments before returning",
);

assert.match(
  `${terminalManagerRs}\n${terminalManagerCrontabRs}`,
  /fn auto_continue_crontab_has_stale_due_markers\([\s\S]*let expected_due_marker = terminal_auto_continue_due_marker\([\s\S]*schedule\.due_at_millis \/ 1000[\s\S]*due_lines\.len\(\) != 1[\s\S]*line\.trim\(\) != expected_due_marker/,
  "matching persisted auto-continue tasks should refresh when crontab contains stale due comments",
);

assert.match(
  terminalManagerRs,
  /async fn run_auto_continue_loop\([\s\S]*self\.refresh_auto_continue_crons\(\);[\s\S]*fn refresh_auto_continue_crons\(&self\)[\s\S]*auto_continue_cron_needs_refresh\(&session_id, &schedule\)[\s\S]*install_auto_continue_cron\(&session_id, &schedule\)/,
  "loaded persisted auto-continue tasks should refresh stale cron scripts and comments even when no new schedule is collected",
);

assert.match(
  mainRs,
  /spawn_error_auto_continue_runner\([\s\S]*state\.workspace_settings\.clone\(\)[\s\S]*state\.auth_manager\.clone\(\)/,
  "terminal error auto-continue runner should receive AuthPresetManager so it can refresh preset snapshots server-side",
);

assert.match(
  terminalManagerRs,
  /run_error_auto_continue_loop\([\s\S]*auth_manager: auth_core::AuthPresetManager[\s\S]*update_api_preset_snapshot\(auth_manager\.api_presets_snapshot\(\)\)[\s\S]*scan_error_auto_continue_sessions/,
  "terminal error auto-continue loop should refresh API preset snapshots without relying on browser session-list requests",
);

assert.match(
  appAutoContinueTasksJs,
  /function renderAutoContinueTasks\(payload\)[\s\S]*task\.task_label[\s\S]*function loadAutoContinueTasks\([\s\S]*\/api\/terminal\/auto-continue-tasks[\s\S]*renderAutoContinueTasks/,
  "settings scheduled-task tab should load and show task types from the backend API",
);

assert.match(
  terminalPasteJs,
  /requestJson\("\/api\/terminal\/auto-continue-tasks"\)[\s\S]*applyTerminalAutoContinueScheduledTaskList\(autoContinueResult\.value\?\.auto_continue_tasks \|\| \[\],[\s\S]*tickTerminalPasteScheduledCountdown\(\);[\s\S]*notifyNewTerminalAutoContinueScheduledTasks/,
  "terminal page scheduled chip should poll server-side auto-continue tasks and refresh its count",
);

assert.match(
  terminalPasteJs,
  /function notifyNewTerminalAutoContinueScheduledTasks\(tasks\)[\s\S]*检测到当前终端 \$\{currentTasks\.length\} 个自动继续定时任务[\s\S]*已更新定时 \$\{counts\.current\}\/\$\{counts\.total\}/,
  "terminal page should toast when it detects new auto-continue scheduled tasks",
);

assert.match(
  terminalJs,
  /function refreshTerminalSettingsFromBroadcast\(\)[\s\S]*previousShowAllWorkspaceSessions[\s\S]*loadTerminalSettings\(\)[\s\S]*loadSessions\(\{[\s\S]*preferredSessionId: state\.activeSessionId[\s\S]*refreshTerminalPasteScheduledTasks\(\)[\s\S]*SETTINGS_EVENT_STORAGE_KEY[\s\S]*refreshTerminalSettingsFromBroadcast\(\)/,
  "terminal page should refresh sessions and scheduled tasks after settings broadcasts change the visible session scope",
);

assert.match(
  mainRs,
  /force_unspecified_listen_host[\s\S]*0\.0\.0\.0[\s\S]*bind_server_listener[\s\S]*AddrInUse[\s\S]*trying next port/,
  "server startup should force 0.0.0.0 listening and auto-increment on occupied ports",
);

assert.match(
  terminalRs,
  /activity_error_keyword: Option<String>[\s\S]*activity_error_signature: Option<String>[\s\S]*activity_error_continue_sent: bool[\s\S]*activity_error_input_queued: bool[\s\S]*activity_error_auto_continue_at: Option<String>[\s\S]*terminal_error_match_line_limit\(\)[\s\S]*terminal_error_keywords\(\)[\s\S]*terminal_auto_continue_time_patterns\(\)/,
  "terminal session API should include configured error activity metadata, a stable error signature, whether continue already follows the error, queued input state, and parsed reset time",
);

assert.match(
  terminalManagerRs,
  /terminal_error_keyword_match\([\s\S]*TerminalActivitySnapshot::error\([\s\S]*error_match\.keyword[\s\S]*error_match\.signature[\s\S]*error_match\.continue_sent/,
  "terminal manager should turn matching tail output into an error activity state with a signature and continue-after-error flag",
);

assert.match(
  terminalManagerRs,
  /if error_match\.continue_sent \{[\s\S]*TerminalActivitySnapshot::retrying\([\s\S]*error_match\.keyword[\s\S]*error_match\.signature[\s\S]*last_output_at[\s\S]*TerminalActivitySnapshot::error\(/,
  "terminal manager should report retrying once a continue command follows the matched error",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /fn terminal_error_keyword_match\([\s\S]*compact_tail[\s\S]*normalize_terminal_error_text/,
  "terminal manager should match wrapped error text by comparing whitespace-collapsed tail output",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /indexed_squashed_terminal_error_text\(tail\)[\s\S]*squashed_needle[\s\S]*squashed_tail_lower\.rfind\(&squashed_needle\)/,
  "terminal manager should match long error keywords even when terminal wrapping splits words",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /let tail = terminal_tail_lines\(&text, line_limit\);[\s\S]*TerminalErrorSignatureCandidate[\s\S]*count_non_overlapping_matches/,
  "terminal error state should be recomputed from the current tail snapshot on each session list refresh",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /best_match\.keyword\.hash\(&mut hasher\);[\s\S]*best_match\.order\.hash\(&mut hasher\);[\s\S]*best_match\.match_count\.hash\(&mut hasher\);[\s\S]*best_match\.context\.hash\(&mut hasher\)/,
  "terminal error signatures should change only when the matched error occurrence changes",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /fn newer_terminal_error_candidate\([\s\S]*existing\.order > candidate\.order[\s\S]*Some\(existing\)[\s\S]*Some\(candidate\)/,
  "terminal error matching should keep the latest matching error occurrence in tail order",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /compact_tail_lower\.rfind\(&needle\)[\s\S]*compact_tail[\s\S]*line_indexes[\s\S]*compact_terminal_error_context/,
  "terminal error matching should assign cross-line compact matches an ordered context signature",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /fn indexed_compact_terminal_error_text\(text: &str\) -> IndexedCompactText[\s\S]*for \(line_index, line\) in text\.lines\(\)\.enumerate\(\)[\s\S]*line_indexes\.push\(line_index\)/,
  "cross-line compact error matching should preserve original tail line order",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /continue_sent: terminal_error_has_continue_after\(tail, best_match\.order\)/,
  "terminal error matching should record when a continue command appears after the matched error occurrence",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /input_queued: terminal_error_has_queued_input_after\(tail, best_match\.order\)/,
  "terminal error matching should record when Codex already has queued user input after the matched error occurrence",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /fn terminal_error_has_continue_after\([\s\S]*tail: &str,[\s\S]*error_line_index: usize,[\s\S]*\) -> bool[\s\S]*skip\(error_line_index\.saturating_add\(1\)\)[\s\S]*is_terminal_continue_line/,
  "terminal error matching should inspect only lines after the latest matched error for an existing continue",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /fn terminal_error_has_queued_input_after\([\s\S]*tail: &str,[\s\S]*error_line_index: usize,[\s\S]*\) -> bool[\s\S]*skip\(error_line_index\.saturating_add\(1\)\)[\s\S]*Messages to be submitted at end of turn/,
  "terminal error matching should inspect only lines after the latest matched error for queued Codex input",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /fn is_terminal_continue_line\(line: &str\) -> bool[\s\S]*trimmed == "继续"[\s\S]*trimmed\.starts_with\("› 继续"\)[\s\S]*trimmed\.starts_with\("↳ 继续"\)/,
  "terminal error matching should recognize submitted and queued continue markers",
);

assert.match(
  terminalManagerRs,
  /pub fn send_session_input\(&self, session_id: &str, data: String\) -> Result<\(\)>[\s\S]*state\.sessions_by_id\.contains_key\(session_id\)[\s\S]*send_backend_input\(&state, session_id, &data\)/,
  "terminal manager should be able to send input directly to a background tmux session",
);

assert.match(
  terminalManagerErrorDetectionRs,
  /fn terminal_tail_lines\(text: &str, line_limit: u32\)[\s\S]*let start = lines\.len\(\)\.saturating_sub\(limit\);[\s\S]*lines\[start\.\.\]\.join\("\\n"\)/,
  "terminal error state should automatically clear once matching lines scroll out of the configured tail range",
);

assert.match(
  terminalSessionActivityJs,
  /function sessionActivityLabel\(session, now = Date\.now\(\)\)[\s\S]*stateValue === "error"[\s\S]*return "错误"[\s\S]*stateValue === "retrying"[\s\S]*return "重试中"/,
  "shared activity labels should support the error and retrying states",
);

assert.match(
  terminalJs,
  /function sessionActivityLabel\(session\) \{[\s\S]*return sharedSessionActivityLabel\(session\);/,
  "terminal page session switcher should use the shared activity labels",
);

assert.match(
  appHomeSessionRenderJs,
  /session-activity-badge[\s\S]*session-activity-error/,
  "active terminal list should render the error state as a status badge",
);
