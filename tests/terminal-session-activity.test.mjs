import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const require = createRequire(import.meta.url);
const terminalJs = readEntryScriptBundle("terminal.html");
const terminalSessionActivity = require("../static/terminal-session-activity.js");
const terminalSessionActivityJs = readFileSync(
  new URL("../static/terminal-session-activity.js", import.meta.url),
  "utf8",
);
const terminalSettingsJs = readFileSync(new URL("../static/terminal-settings.js", import.meta.url), "utf8");
const terminalCoreJs = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const terminalCommonJs = readFileSync(new URL("../static/terminal-common.js", import.meta.url), "utf8");
const terminalSessionsJs = readFileSync(
  new URL("../static/terminal-sessions.js", import.meta.url),
  "utf8",
);
const terminalSettingsLoaderJs = readFileSync(
  new URL("../static/terminal-settings-loader.js", import.meta.url),
  "utf8",
);
const appJs = readEntryScriptBundle("index.html");
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const settingsCore = readFileSync(new URL("../crates/settings_core/src/lib.rs", import.meta.url), "utf8");
const terminalRoutesRs = readFileSync(new URL("../src/routes/terminal.rs", import.meta.url), "utf8");
const terminalRs = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");
const terminalManagerRs = readFileSync(new URL("../src/terminal/manager.rs", import.meta.url), "utf8");
const terminalManagerErrorDetectionRs = readFileSync(
  new URL("../src/terminal/manager/error_detection.rs", import.meta.url),
  "utf8",
);

assert.match(
  terminalSessionActivityJs,
  /function sessionActivityLabel\(session, now = Date\.now\(\)\)[\s\S]*stateValue === "completed"[\s\S]*return "待查看"[\s\S]*stateValue === "recent_output"[\s\S]*return "输出中"/,
  "shared session activity helper should label completed output as pending review before recent output",
);

assert.match(
  terminalSessionActivityJs,
  /function sessionActivityLabel\(session, now = Date\.now\(\)\)[\s\S]*stateValue === "working"[\s\S]*return "工作中"[\s\S]*stateValue === "recent_output"/,
  "shared session activity helper should prioritize backend working status over generic recent output",
);

assert.match(
  terminalSessionActivityJs,
  /function sessionActivityLabel\(session, now = Date\.now\(\)\)[\s\S]*stateValue === "agent"[\s\S]*return "空闲"/,
  "shared session activity helper should treat idle agent processes as idle instead of pending review",
);

assert.doesNotMatch(
  terminalSessionActivityJs,
  /待命/,
  "shared session activity helper should not render agent labels such as Codex directly",
);

assert.equal(
  terminalSessionActivity.sessionActivityLabel(
    { activity_state: "completed", last_output_at: 1000 },
    1000,
  ),
  "待查看",
  "completed output should mean the agent finished and the user has not viewed the new output",
);

assert.equal(
  terminalSessionActivity.sessionActivityLabel(
    { activity_state: "agent", activity_agent: "Claude", last_output_at: 0 },
    1000,
  ),
  "空闲",
  "idle Claude/Codex process presence should not be shown as pending review",
);

assert.deepEqual(
  terminalSessionActivity.sessionAfterOutputViewed({
    id: "s1",
    activity_state: "completed",
    activity_label: "待查看",
    activity_agent: "Codex",
  }),
  {
    id: "s1",
    activity_state: "agent",
    activity_label: "Codex",
    activity_agent: "Codex",
  },
  "clicking a completed agent session should clear its pending-review label immediately",
);

assert.deepEqual(
  terminalSessionActivity.sessionAfterOutputViewed({
    id: "s2",
    activity_state: "recent_output",
    activity_label: "输出中",
    activity_agent: null,
  }),
  {
    id: "s2",
    activity_state: "idle",
    activity_label: "空闲",
    activity_agent: null,
  },
  "clicking recent output should mark it locally idle while the backend refresh catches up",
);

const errorSession = {
  id: "s3",
  activity_state: "error",
  activity_label: "错误",
  activity_agent: "Codex",
};
assert.equal(
  terminalSessionActivity.sessionAfterOutputViewed(errorSession),
  errorSession,
  "viewing a terminal must not optimistically clear error or working states",
);

assert.match(
  terminalCoreJs,
  /function markSessionOpenedLocally\(sessionId\)[\s\S]*sharedSessionAfterOutputViewed\(session\)/,
  "selecting a session should update its viewed activity before rendering the terminal list",
);

assert.match(
  terminalCoreJs,
  /function sortSessionsByRecentActivity\(sessions\) \{[\s\S]*state\.sessionSortMode[\s\S]*sharedSortTerminalSessions\(sessions, state\.sessionSortMode\)[\s\S]*sharedSortSessionsByRecentActivity\(sessions\)/,
  "session metadata refreshes should preserve the terminal-list sort mode selected by the user",
);

assert.match(
  terminalSessionActivityJs,
  /function sessionActivityAgentPrefix\(session, displayMode = "hidden"\)[\s\S]*displayMode === "prefix"[\s\S]*function sessionActivityAgentSuffix\(session, displayMode = "hidden"\)[\s\S]*displayMode === "suffix"/,
  "shared session activity helper should show running program labels only when configured as a prefix or suffix",
);

assert.match(
  terminalCommonJs,
  /function sessionOptionLabel\(session\) \{[\s\S]*`\$\{sessionActivityAgentPrefix\(session\)\}\$\{sessionActivityPrefix\(session\)\}\$\{session\?\.name \|\| session\?\.id \|\| "未命名终端"\}\$\{sessionActivityAgentSuffix\(session\)\}`/,
  "terminal dropdown should place configured running program labels around the session option label",
);

assert.match(
  terminalCommonJs,
  /function sessionOptionLabel\(session\) \{[\s\S]*sessionActivityPrefix\(session\)[\s\S]*sessionOptionTitle\(session\)/,
  "terminal dropdown options should prefix names with the activity label",
);

assert.match(
  terminalJs,
  /const TERMINAL_SESSION_ACTIVITY_REFRESH_MS = 6000/,
  "terminal activity refresh should poll the backend slowly instead of rebuilding the dropdown on every output chunk",
);

assert.match(
  terminalJs,
  /function scheduleSessionActivityRefresh\([\s\S]*isSessionDropdownInteracting\(\)[\s\S]*TERMINAL_SESSION_ACTIVITY_INTERACTION_RETRY_MS[\s\S]*loadSessions\(\{[\s\S]*preserveCurrentList: true,[\s\S]*\}/,
  "terminal activity refresh should defer backend polling and dropdown rebuilding while the user is interacting with selects",
);

assert.match(
  terminalJs,
  /function shouldDeferSessionListRender\(\) \{[\s\S]*return isSessionDropdownInteracting\(\);[\s\S]*\}/,
  "terminal page should identify when native session selects are being used",
);

assert.match(
  terminalSessionsJs,
  /async function loadSessions\([\s\S]*if \(shouldDeferSessionListRender\(\)\) \{[\s\S]*mergePendingSessionRefresh\(\{[\s\S]*preferredSessionId,[\s\S]*pushHistoryOnSelect,[\s\S]*preserveCurrentList,[\s\S]*forcePreferredSession,[\s\S]*\}\);[\s\S]*return;[\s\S]*\}/,
  "terminal session list loading should not rebuild the native session select while its popup is open",
);

assert.match(
  terminalSessionsJs,
  /const fetchedSessions = \(response\.sessions \|\| \[\]\)\.slice\(\);[\s\S]*state\.sessions = sortSessionsByRecentActivity\(fetchedSessions\);[\s\S]*if \(shouldDeferSessionListRender\(\)\) \{[\s\S]*mergePendingSessionRefresh\(\{[\s\S]*preserveCurrentList: true,[\s\S]*\}\);[\s\S]*return;[\s\S]*\}[\s\S]*if \(state\.sessions\.length === 0\)/,
  "terminal session list loading should also defer rendering when a request completes after the select interaction begins",
);

assert.match(
  terminalJs,
  /function bindSessionActivitySafeSelect\(selectEl\)[\s\S]*pointerdown[\s\S]*markSessionDropdownInteracting[\s\S]*focus[\s\S]*markSessionDropdownInteracting[\s\S]*blur[\s\S]*flushSessionActivityRenderAfterInteraction[\s\S]*change[\s\S]*flushSessionActivityRenderAfterInteraction/,
  "session dropdowns should mark interaction windows and flush pending activity after closing",
);

assert.match(
  appJs,
  /function bindSessionSelectInteractionGuard\(selectEl,[\s\S]*const beginInteraction = \(\) => \{[\s\S]*setBlocked\(true\)[\s\S]*restartPendingSessionViewsRefresh\(\);[\s\S]*selectEl\.addEventListener\("pointerdown", beginInteraction\);[\s\S]*selectEl\.addEventListener\("mousedown", beginInteraction\);[\s\S]*selectEl\.addEventListener\("touchstart", beginInteraction, \{ passive: true \}\);/,
  "home activity terminal selects should guard native dropdowns from pointer and touch refresh rebuilds as soon as interaction starts",
);

assert.match(
  appJs,
  /function bindSessionSelectInteractionGuard\(selectEl,[\s\S]*selectEl\.addEventListener\("keydown", \(event\) => \{[\s\S]*\[" ", "Enter", "ArrowDown", "ArrowUp"\]\.includes\(event\.key\)[\s\S]*beginInteraction\(\);[\s\S]*\["Escape", "Enter"\]\.includes\(event\.key\)[\s\S]*window\.setTimeout\(endInteraction, 120\);[\s\S]*selectEl\.addEventListener\("blur", endInteraction\);[\s\S]*selectEl\.addEventListener\("change", endInteraction\);/,
  "home activity terminal selects should keep keyboard-opened native dropdowns stable until close or change",
);

assert.match(
  appJs,
  /bindSessionSelectInteractionGuard\(directorySessionListEl,[\s\S]*state\.directorySessionUiBlocked = blocked[\s\S]*syncDirectorySessionControls\(\)/,
  "workspace terminal dropdown should use the shared interaction guard before rebuilding options",
);

assert.match(
  appJs,
  /bindSessionSelectInteractionGuard\(sessionsSessionListEl,[\s\S]*state\.sessionsSessionUiBlocked = blocked[\s\S]*renderSessionsSessionPicker\(\)/,
  "activity terminal dropdown should use the shared interaction guard before rebuilding options",
);

assert.match(
  terminalJs,
  /const bytes = new Uint8Array\(buffer\);\s*queueTerminalOutput\(bytes, token, context\);/,
  "frontend websocket output should not decide session activity; backend polling owns output detection",
);

assert.match(
  terminalJs,
  /function sessionOptionTitle\(session\) \{[\s\S]*sessionActivityLabel\(session\)[\s\S]*option\.title = sessionOptionTitle\(session\)/,
  "terminal dropdown option titles should include the activity label",
);

assert.match(
  terminalRs,
  /manager\.mark_session_output_viewed\(&session\.id\);/,
  "terminal websocket delivery should mark live output as viewed for the connected terminal",
);

assert.match(
  terminalManagerRs,
  /fn collect_session_activity_probes\([\s\S]*let agent_detector = TerminalAgentDetector::new\(\);[\s\S]*let agent_activity = agent_detector\.detect\(&session_id\);[\s\S]*TerminalActivityProbe \{[\s\S]*agent_activity,[\s\S]*fn terminal_activity_snapshot_from_probe_locked\([\s\S]*let activity_agent = probe[\s\S]*\.then\(\|\| probe\.agent_activity\.agents\.join\("\/"\)\);/,
  "backend activity should detect running program names before state-specific activity decisions",
);

assert.match(
  terminalManagerRs,
  /last_output_at > last_viewed_output_at[\s\S]*TerminalActivitySnapshot::completed\(last_output_at\)\.with_agent\(activity_agent\)/,
  "backend activity should report completed when stopped output has not been viewed while preserving detected agent names",
);

assert.match(
  terminalRs,
  /fn completed\(last_output_at: u64\) -> Self \{[\s\S]*label: "待查看"\.to_string\(\)/,
  "backend completed activity label should match the pending-review UI wording",
);

assert.match(
  terminalManagerRs,
  /fn terminal_activity_snapshot_from_probe_locked\([\s\S]*if probe\.working_status \{[\s\S]*TerminalActivitySnapshot::working\(last_output_at\)\.with_agent\(activity_agent\)[\s\S]*if let Some\(error_match\) = probe\.error_match\.as_ref\(\)/,
  "backend activity should prioritize Codex Working status from the last 10 terminal lines before other states while preserving detected agent names",
);

assert.match(
  `${terminalManagerRs}\n${terminalManagerErrorDetectionRs}`,
  /terminal_working_status_match_from_snapshot\(snapshot, 10\)[\s\S]*fn terminal_working_status_match_from_snapshot\([\s\S]*terminal_tail_lines\(&text, line_limit\)/,
  "backend activity should keep Working as 工作中 instead of falling through to recent 输出中",
);

assert.match(
  terminalManagerRs,
  /fn terminal_activity_snapshot_from_probe_locked\([\s\S]*if probe\.worked_status && last_output_at > last_viewed_output_at \{[\s\S]*return TerminalActivitySnapshot::completed\(last_output_at\)\.with_agent\(activity_agent\);[\s\S]*TERMINAL_RECENT_OUTPUT_ACTIVE_MS/,
  "backend activity should report worked-for completion as pending review only when the latest output has not been viewed while preserving detected agent names",
);

assert.match(
  appJs,
  /sessionActivityLabel: sharedSessionActivityLabel[\s\S]*= globalThis\.WebClxTerminalSessionActivity/,
  "home page should import the shared activity label helper",
);

assert.match(
  appJs,
  /function sessionActivityLabel\(session\) \{[\s\S]*return sharedSessionActivityLabel\(session\);/,
  "home activity labels should use the shared activity helper",
);

assert.match(
  appJs,
  /function sessionActivityAgentPrefix\(session\) \{[\s\S]*return sharedSessionActivityAgentPrefix\(session, state\.terminalActivityAgentDisplay\);[\s\S]*function sessionActivityAgentSuffix\(session\) \{[\s\S]*return sharedSessionActivityAgentSuffix\(session, state\.terminalActivityAgentDisplay\);/,
  "home session labels should pass configured running program display mode to the shared helper",
);

assert.doesNotMatch(
  appJs,
  /return activityLabel \|\| agentLabel \|\| "待命"/,
  "home activity labels should not render agent labels such as Codex directly",
);

assert.match(
  terminalJs,
  /const \{[\s\S]*sessionActivityLabel: sharedSessionActivityLabel[\s\S]*\} = globalThis\.WebClxTerminalSessionActivity;[\s\S]*function sessionActivityLabel\(session\) \{[\s\S]*return sharedSessionActivityLabel\(session\);/,
  "terminal activity labels should use the shared activity helper",
);

assert.match(
  appJs,
  /function directorySessionOptionLabel\(session\) \{[\s\S]*`\$\{sessionActivityAgentPrefix\(session\)\}\$\{sessionActivityPrefix\(session\)\}\$\{session\.name\}\$\{sessionActivityAgentSuffix\(session\)\}`/,
  "workspace terminal dropdown should place configured running program labels around the session option label",
);

assert.match(
  appJs,
  /body: JSON\.stringify\(\{[\s\S]*terminal_activity_agent_display: nextTerminalActivityAgentDisplay,[\s\S]*\}\)/,
  "settings save should persist the terminal running program display setting",
);

assert.match(
  indexHtml,
  /id="terminal-completion-bell-enabled-input"[\s\S]*id="terminal-completion-bell-test"/,
  "settings page should expose a completion bell enable toggle and test button",
);

assert.match(
  terminalSettingsJs,
  /const TERMINAL_COMPLETION_BELL_URL = "\/api\/terminal\/completion-bell\.wav"/,
  "shared terminal settings should define the built-in completion bell URL",
);

assert.match(
  appJs,
  /terminalCompletionBellEnabled[\s\S]*settings\.terminal_completion_bell_enabled !== false[\s\S]*terminal_completion_bell_enabled: nextTerminalCompletionBellEnabled[\s\S]*terminalCompletionBellTestButtonEl\.addEventListener\("click"[\s\S]*playTerminalCompletionBellTest/,
  "settings page should load, save, and test the terminal completion bell setting",
);

assert.match(
  terminalCoreJs,
  /terminalCompletionBellEnabled: true/,
  "terminal page should enable the completion bell by default",
);

assert.match(
  terminalSettingsLoaderJs,
  /state\.terminalCompletionBellEnabled = settings\.terminal_completion_bell_enabled !== false/,
  "terminal page should load the completion bell enabled setting",
);

assert.match(
  terminalJs,
  /function maybePlayTerminalCompletionSound\(sessions\)[\s\S]*!state\.terminalCompletionBellEnabled[\s\S]*return/,
  "terminal page should honor the completion bell enabled setting before auto-playing",
);

assert.match(
  terminalRoutesRs,
  /route\(\s*"\/api\/terminal\/completion-bell\.wav",\s*get\(terminal::completion_bell_sound\),?\s*\)/,
  "terminal routes should expose a built-in completion bell sound API",
);

assert.match(
  terminalRs,
  /pub async fn completion_bell_sound\(\) -> Response[\s\S]*header::CONTENT_TYPE[\s\S]*audio\/wav[\s\S]*completion_bell_wav_bytes\(\)/,
  "completion bell API should return an embedded WAV sound",
);

assert.match(
  terminalJs,
  /const TERMINAL_COMPLETION_BELL_URL = "\/api\/terminal\/completion-bell\.wav"[\s\S]*completionSoundPreviousStates: new Map\(\)/,
  "terminal page should configure a built-in completion bell URL and previous-state map",
);

assert.match(
  terminalJs,
  /function maybePlayTerminalCompletionSound\(sessions\)[\s\S]*previousState === "working" && stateValue === "completed"[\s\S]*state\.completionSoundPreviousStates\.set\(sessionId, stateValue\)[\s\S]*playTerminalCompletionSound\(\)/,
  "terminal page should play the built-in bell only when a session transitions from working to completed",
);

assert.match(
  indexHtml,
  /id="terminal-activity-agent-display-select"[\s\S]*value="hidden"[\s\S]*value="prefix"[\s\S]*value="suffix"/,
  "settings page should expose terminal running program display placement options",
);

assert.match(
  settingsCore,
  /DEFAULT_TERMINAL_ACTIVITY_AGENT_DISPLAY: TerminalActivityAgentDisplay\s*=\s*TerminalActivityAgentDisplay::Hidden[\s\S]*DEFAULT_TERMINAL_COMPLETION_BELL_ENABLED: bool = true[\s\S]*enum TerminalActivityAgentDisplay[\s\S]*Hidden[\s\S]*Prefix[\s\S]*Suffix[\s\S]*terminal_activity_agent_display[\s\S]*terminal_completion_bell_enabled/,
  "settings core should persist terminal running program display and completion bell settings",
);
