import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalJs = readEntryScriptBundle("terminal.html");
const cliRust = readFileSync(new URL("../src/cli.rs", import.meta.url), "utf8");
const terminalRoutesRust = readFileSync(
  new URL("../src/routes/terminal.rs", import.meta.url),
  "utf8",
);
const terminalManagerRust = readFileSync(
  new URL("../src/terminal/manager.rs", import.meta.url),
  "utf8",
);
const terminalRust = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");
const codexLaunchRust = readFileSync(new URL("../src/codex_launch.rs", import.meta.url), "utf8");
const codexConversationModelRust = readFileSync(
  new URL("../src/codex_conversation_model.rs", import.meta.url),
  "utf8",
);

function sourceBetween(startMarker, endMarker) {
  const start = terminalJs.indexOf(startMarker);
  const end = terminalJs.indexOf(endMarker, start);
  assert.ok(start >= 0 && end > start, `missing source block: ${startMarker}`);
  return terminalJs.slice(start, end);
}

const resumeCurrentAgentSession = sourceBetween(
  "async function resumeCurrentAgentSession()",
  "function terminalShortcutKeyName",
);
assert.match(
  resumeCurrentAgentSession,
  /extractCurrentAgentSessionId\(\)[\s\S]*await sendTerminalAutoTypedInput\(command\)/,
  "current-session resume should use backend preparation so the latest terminal environment is loaded",
);
assert.doesNotMatch(
  resumeCurrentAgentSession,
  /sendTerminalInput\(command\)|MOBILE_KEY_SEQUENCES\.enter/,
  "current-session resume must not bypass the current terminal environment preparation",
);

const runPendingTerminalCommand = sourceBetween(
  "function runPendingTerminalCommand()",
  "function terminalInitialReplaySettled",
);
assert.match(
  runPendingTerminalCommand,
  /await sendTerminalAutoTypedInput\(command\)/,
  "run= startup commands should use backend preparation so the latest terminal environment is loaded",
);
assert.doesNotMatch(
  runPendingTerminalCommand,
  /sendTerminalInput\(command\)|MOBILE_KEY_SEQUENCES\.enter/,
  "run= startup commands must not bypass the current terminal environment preparation",
);

const runAgentStart = cliRust.indexOf("async fn run_agent(");
const runAgentEnd = cliRust.indexOf("\n#[cfg(test)]", runAgentStart);
const runAgent = cliRust.slice(runAgentStart, runAgentEnd);
assert.match(
  runAgent,
  /client\.apply\(kind, &target, Some\(&cwd\)\)\.await[\s\S]*Command::new\(agent\)/,
  "webclx run should persist the selected preset before launching the agent",
);
assert.match(
  runAgent,
  /applied\.codex_model\(\)[\s\S]*codex_history_args_with_model\(agent, args, codex_model\.as_deref\(\)\)[\s\S]*Command::new\(agent\)/,
  "webclx run should pass the model read back from the applied preset to Codex resume and fork",
);
assert.match(
  runAgent,
  /while applied\.deferred[\s\S]*client\.apply\(kind, &target, Some\(&cwd\)\)\.await\?;[\s\S]*Command::new\(agent\)/,
  "webclx run should wait for a deferred apply before starting the agent",
);
assert.doesNotMatch(
  runAgent,
  /STARTUP_PRESET_HANDOFF|acquire_preset_run_lease|release_preset_run_lease/,
  "webclx run must not restore the previous provider after a timing-based startup handoff",
);
assert.doesNotMatch(
  runAgent,
  /heartbeat_preset_run_lease|global preset gate while agent `[{}\w-]*` was running/,
  "webclx run must not lock the shared preset for the lifetime of the agent session",
);
assert.doesNotMatch(
  runAgent,
  /finalize_codex|RESTORE_DELAY|runner_path|TemporaryPresetRuntime/,
  "webclx run must not create a temporary configuration runtime",
);

for (const removedRoute of [
  "prepare-codex-launch",
  "finalize-codex-launch",
  "prepare-codex-resume",
]) {
  assert.doesNotMatch(
    terminalRoutesRust,
    new RegExp(removedRoute),
    `${removedRoute} should not be registered`,
  );
  assert.doesNotMatch(
    terminalManagerRust,
    new RegExp(removedRoute),
    `${removedRoute} should not be called by terminal startup`,
  );
}
assert.doesNotMatch(
  terminalManagerRust,
  /\.webclx-launches|inherited_codex_home|CODEX_HOME/,
  "terminal startup must not create or depend on Codex configuration homes",
);
assert.doesNotMatch(
  terminalManagerRust,
  /ensure_codex_command_env_wrapper|write_session_command_env_script|codex_command_env_wrapper_script/,
  "ordinary terminals must not install launchers or write protected command environments",
);
assert.match(
  terminalManagerRust,
  /remove_legacy_codex_command_env_launchers/,
  "startup should remove only legacy webClx-managed launchers",
);
assert.doesNotMatch(
  terminalRust,
  /codex_startup_fingerprint|write_session_command_env_script|codex_command_with_env_launcher/,
  "terminal command routing must not cache model state or inject a launcher",
);
assert.match(
  terminalRust,
  /prepare_terminal_quick_command_for_session[\s\S]*prepare_codex_history_model_for_user[\s\S]*prepare_codex_history_command_for_user/,
  "webClx-generated resume commands should rewrite the rollout to the current model before adding the CLI override",
);
assert.match(
  terminalManagerRust,
  /restore_initial_sessions[\s\S]*prepare_codex_history_command_for_user[\s\S]*send_backend_startup_script/,
  "shutdown restore should pass the restored terminal user's current config model",
);
assert.match(
  terminalManagerRust,
  /force_interrupt_and_resume[\s\S]*prepare_codex_history_command_for_user[\s\S]*send_session_input_silent/,
  "interrupt-and-resume should pass the target terminal user's current config model",
);
assert.match(
  codexLaunchRust,
  /config\.toml[\s\S]*get\("model"\)/,
  "the shared command preparer should parse the current config model",
);
assert.match(
  codexLaunchRust,
  /codex_history_command_with_model[\s\S]*--model/,
  "the shared command preparer should add the current model as a Codex CLI override",
);
assert.equal(
  existsSync(new URL("../src/terminal/codex_model_migration.rs", import.meta.url)),
  false,
  "model migration should remain in the shared Codex conversation module",
);
assert.doesNotMatch(
  terminalRust,
  /rewrite_codex_rollout_model/,
  "terminal.rs must not install automatic rollout rewrites into resume routing",
);
assert.match(
  terminalRoutesRust,
  /\/api\/terminal\/codex-conversations\/model[\s\S]*put\(codex_conversation_model::update_codex_conversation_model\)/,
  "the explicit conversation model API should be registered under terminal routes",
);
assert.match(
  codexConversationModelRust,
  /"model"[\s\S]*"collaboration_mode"[\s\S]*"settings"/,
  "the explicit API should rewrite payload.model and collaboration settings model",
);
