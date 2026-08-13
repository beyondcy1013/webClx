import assert from "node:assert/strict";
import fs from "node:fs";

const terminalSource = fs.readFileSync(
  new URL("../src/terminal.rs", import.meta.url),
  "utf8",
);
const managerSource = fs.readFileSync(
  new URL("../src/terminal/manager.rs", import.meta.url),
  "utf8",
);

assert.match(
  terminalSource,
  /derive\(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq\)[\s\S]*struct TerminalOutputObservation \{[\s\S]*fingerprint: Option<u64>,[\s\S]*serde\(skip\)[\s\S]*last_fingerprint_probe_sequence: u64,[\s\S]*last_output_at: u64,[\s\S]*last_viewed_output_at: u64/,
  "terminal output observations must be serializable while probe ordering stays process-local",
);
assert.match(
  terminalSource,
  /struct StoredTerminalRegistry \{[\s\S]*output_observations: HashMap<String, TerminalOutputObservation>/,
  "the terminal registry must persist viewed-output observations",
);
assert.match(
  managerSource,
  /state\.output_observations = registry\.output_observations;[\s\S]*output_observations[\s\S]*retain\(\|session_id,[\s\S]*state\.sessions_by_id\.contains_key\(session_id\)/,
  "restart loading must restore observations only for surviving sessions",
);
assert.match(
  managerSource,
  /fn persist_state_locked\([\s\S]*output_observations: collect_stored_output_observations\(state\)/,
  "terminal registry persistence must include output observations",
);
assert.match(
  managerSource,
  /fn restore_live_sessions\([\s\S]{0,5000}prepare_restored_output_observation_locked/,
  "normal session restore must rebase the pane fingerprint before the first activity scan",
);
assert.match(
  managerSource,
  /arm_output_observations_for_restore_locked\(&mut state\)/,
  "persisted output observations must be armed before deferred restore can race a browser reconnect",
);
assert.match(
  managerSource,
  /fn restore_shutdown_sessions\([\s\S]{0,6000}prepare_restored_output_observation_locked/,
  "explicit shutdown/save restore must also rebase the pane fingerprint before the first activity scan",
);
assert.match(
  managerSource,
  /fn prepare_restored_output_observation_locked\([\s\S]*rebaseline_terminal_output_locked\([\s\S]*rebaseline_after_restore = true/,
  "restore rebaselining must preserve the old output timestamp and consume the flag on the first scan",
);
const restoreRebaselineFn = managerSource.slice(
  managerSource.indexOf("fn prepare_restored_output_observation_locked"),
  managerSource.indexOf("fn mark_session_output_viewed_locked"),
);
assert.doesNotMatch(
  restoreRebaselineFn,
  /observe_terminal_output_locked/,
  "restore rebaselining must not advance last_output_at for a restart-only redraw",
);
assert.match(
  managerSource,
  /pub fn mark_session_output_viewed_in_memory\([\s\S]*mark_session_output_viewed_with_persistence\(session_id, false\)[\s\S]*pub fn mark_session_output_viewed\([\s\S]*mark_session_output_viewed_with_persistence\(session_id, true\)/,
  "high-frequency viewed acknowledgements must stay in memory while stable boundaries persist",
);
assert.match(
  terminalSource,
  /output_viewed_mark_interval\.tick\(\)[\s\S]{0,260}manager\.mark_session_output_viewed_in_memory\(&session\.id\)/,
  "the periodic live-output acknowledgement must not write the registry every second",
);
assert.match(
  terminalSource,
  /ClientMessage::Visibility \{ visible \} => \{[\s\S]{0,400}output_visible\.swap\(visible,[\s\S]{0,400}else if was_visible \{[\s\S]{0,260}manager\.mark_session_output_viewed\(&session\.id\)/,
  "switching a viewed terminal into the background must persist its latest observation",
);
assert.match(
  terminalSource,
  /'socket_loop: loop \{[\s\S]*\n\s*\}[\s\S]{0,180}if output_visible\.load\([\s\S]{0,160}manager\.mark_session_output_viewed\(&session\.id\)/,
  "disconnecting a visible terminal must persist even after its periodic in-memory acknowledgement cleared the pending flag",
);

console.log("terminal output observation persistence tests passed");
