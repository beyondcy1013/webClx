import assert from "node:assert/strict";
import fs from "node:fs";

const tmuxSource = fs.readFileSync(new URL("../src/terminal/tmux.rs", import.meta.url), "utf8");
const managerSource = fs.readFileSync(
  new URL("../src/terminal/manager.rs", import.meta.url),
  "utf8",
);
const terminalSource = fs.readFileSync(
  new URL("../src/terminal.rs", import.meta.url),
  "utf8",
);
const activitySource = fs.readFileSync(
  new URL("../src/terminal/activity.rs", import.meta.url),
  "utf8",
);

assert.match(
  tmuxSource,
  /const INITIAL_TMUX_SNAPSHOT_LINE_LIMIT:\s*u32\s*=\s*100;/,
  "initial terminal switch replay must stay bounded to 100 tmux lines",
);
assert.match(
  tmuxSource,
  /pub\(super\) fn capture_tmux_text_pane_snapshot\(session_id: &str\)[\s\S]*capture_tmux_pane_snapshot_from\(session_id, "-", false\)/,
  "full text history capture must remain available for search and diagnostics",
);
assert.match(
  managerSource,
  /fn collect_session_infos_without_manager_lock\([\s\S]*let live_sessions = \{[\s\S]*self\.state\.read\(\)[\s\S]*let mut probes = self\.collect_session_activity_probes_cached\([\s\S]*let mut state = crate::lock_or_recover!\(self\.state\.write\(\)\);[\s\S]*collect_session_infos_from_probes_locked/,
  "tmux and process activity probes must run between the short read and write lock sections",
);
const unlockedInfoCollector = managerSource.slice(
  managerSource.indexOf("fn collect_session_infos_without_manager_lock("),
  managerSource.indexOf("fn collect_session_activity_probes_cached("),
);
assert.doesNotMatch(
  unlockedInfoCollector,
  /let mut state = crate::lock_or_recover!\(self\.state\.write\(\)\);[\s\S]*collect_session_activity_probes\(/,
  "activity probes must never execute while the terminal manager write lock is held",
);

const activityProbeCollector = managerSource.slice(
  managerSource.indexOf("fn collect_session_activity_probes("),
  managerSource.indexOf("pub(super) fn collect_session_infos_from_probes_locked("),
);
assert.match(
  activityProbeCollector,
  /capture_tmux_activity_pane_snapshot\(&session_id, error_line_limit\)/,
  "each session activity probe must capture readable tmux text only once",
);
assert.doesNotMatch(
  activityProbeCollector,
  /capture_tmux_recent_pane_snapshot|terminal_working_status_match_default|terminal_error_keyword_match\(\s*&session_id|terminal_worked_status_match_default/,
  "session activity probing must not recapture the same tmux pane for each status check",
);
assert.equal(
  (activityProbeCollector.match(/capture_tmux_activity_pane_snapshot\(/g) || []).length,
  1,
  "all activity fields must share a single pane capture per probe",
);
const lockedActivityMerge = managerSource.slice(
  managerSource.indexOf("fn terminal_activity_snapshot_from_probe_locked("),
  managerSource.indexOf("pub(super) fn collect_terminal_auto_continue_schedules("),
);
assert.ok(lockedActivityMerge.length > 0, "locked activity merge must be found");
assert.doesNotMatch(
  lockedActivityMerge,
  /capture_tmux_|Command::|\.output\(|\.spawn\(/,
  "merging precomputed activity under the state write lock must not launch a subprocess",
);
assert.match(
  activitySource,
  /fn read_tmux_pane_pid\(session_id: &str\)[\s\S]*isolated_tmux_command\(session_id\)[\s\S]*display-message/,
  "agent detection must use each session's isolated tmux socket for its pane PID",
);
assert.doesNotMatch(
  activitySource,
  /Command::new\("tmux"\)[\s\S]{0,200}list-panes/,
  "agent detection must not use a global tmux server listing that cannot address isolated sockets",
);
assert.match(
  terminalSource,
  /activity_probe_cache:\s*Arc<Mutex<manager::TerminalActivityProbeCache>>[\s\S]{0,200}activity_probe_scan_lock:\s*Arc<Mutex<\(\)>>/,
  "terminal manager must share activity probe cache and single-flight scan state across clones",
);
assert.match(
  managerSource,
  /fn collect_session_activity_probes_cached\([\s\S]*cached_activity_probes\(&key\)[\s\S]*activity_probe_scan_lock[\s\S]*cached_activity_probes\(&key\)[\s\S]*collect_session_activity_probes\([\s\S]*activity_probe_cache/,
  "concurrent session-list requests must reuse one recent activity scan",
);
assert.match(
  managerSource,
  /let (?:mut )?probes = self\.collect_session_activity_probes_cached\(/,
  "session listing must use the cached single-flight activity probe path",
);

console.log("terminal switch performance contract tests passed");

const activityCapture = tmuxSource.slice(
  tmuxSource.indexOf("pub(super) fn capture_tmux_activity_pane_snapshot("),
  tmuxSource.indexOf("pub(super) fn capture_tmux_text_pane_snapshot("),
);
assert.match(activityCapture, /bounded_command_output/, "Unix activity capture must use the bounded runner");
assert.match(activityCapture, /decode_tmux_snapshot\(output\)/, "bounded activity output retains existing decoding");
const panePidLookup = activitySource.slice(
  activitySource.indexOf("fn read_tmux_pane_pid("),
  activitySource.indexOf("fn parse_tmux_pane_pid("),
);
assert.match(panePidLookup, /bounded_command_output/, "Unix pane PID lookup must use the same bounded runner");
