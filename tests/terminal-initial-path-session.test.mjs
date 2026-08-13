import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalJs = readEntryScriptBundle("terminal.html");
const terminalRs = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");
const terminalManagerRs = readFileSync(
  new URL("../src/terminal/manager.rs", import.meta.url),
  "utf8",
);

function functionSource(source, name) {
  const start = source.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, `missing function ${name}`);
  const bodyStart = source.indexOf("{", start);
  let depth = 0;
  for (let index = bodyStart; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(start, index + 1);
  }
  assert.fail(`unterminated function ${name}`);
}

function shouldCreateInitialTerminalSession(initialLocation) {
  const context = {
    initialLocation,
    normalizeTerminalPath: (value) => String(value || "").replace(/^\/+|\/+$/g, ""),
  };
  vm.createContext(context);
  vm.runInContext(
    functionSource(terminalJs, "shouldCreateInitialTerminalSession"),
    context,
  );
  return vm.runInContext("shouldCreateInitialTerminalSession()", context);
}

assert.equal(
  shouldCreateInitialTerminalSession({
    path: "/home/codes",
    sessionId: "",
    fresh: false,
    runCommand: "",
  }),
  false,
  "a path-only terminal-management URL must not create a terminal",
);
assert.equal(
  shouldCreateInitialTerminalSession({
    path: "webClx",
    sessionId: "",
    fresh: true,
    runCommand: "",
  }),
  true,
  "an explicit fresh URL should still create a terminal",
);
assert.equal(
  shouldCreateInitialTerminalSession({
    path: "webClx",
    sessionId: "",
    fresh: false,
    runCommand: "codex",
  }),
  true,
  "an explicit run URL should still create a terminal",
);
assert.equal(
  shouldCreateInitialTerminalSession({
    path: "webClx",
    sessionId: "existing-session",
    fresh: true,
    runCommand: "",
  }),
  false,
  "an explicit existing session should remain authoritative",
);

assert.match(
  terminalJs,
  /if \(shouldCreateInitialTerminalSession\(\)\) \{[\s\S]*?await createSession\(\{/,
  "initial terminal load should keep an explicit create-session branch",
);

assert.match(
  terminalJs,
  /initialTerminalIntentPending:\s*Boolean\(initialLocation\.fresh \|\| initialLocation\.quickStart \|\| initialLocation\.runCommand\),/,
  "terminal page should remember one-shot fresh/run/quick-start intent before normalizing history",
);

assert.match(
  terminalJs,
  /function syncHistory\(\{ push = false \} = \{\}\) \{[\s\S]*if \(state\.initialTerminalIntentPending\) \{[\s\S]*syncTopNavigation\(\);[\s\S]*updateNavigationButtons\(\);[\s\S]*return;[\s\S]*\}/,
  "initial history normalization should not drop fresh/run/quick-start parameters before they are consumed",
);

assert.match(
  terminalJs,
  /if \(shouldCreateInitialTerminalSession\(\)\) \{[\s\S]*await createSession\(\{[\s\S]*enableQuickStart: !initialLocation\.runCommand,[\s\S]*\}\);[\s\S]*state\.initialTerminalIntentPending = false;[\s\S]*syncHistory\(\);[\s\S]*\} else \{[\s\S]*state\.initialTerminalIntentPending = false;[\s\S]*await loadSessions/,
  "terminal page should release initial intent only after the create/load branch consumes it",
);

assert.match(
  terminalManagerRs,
  /pub\(in crate::terminal\) fn session_path\(&self, session_id: &str\) -> Option<PathBuf>[\s\S]*sessions_by_id[\s\S]*\.get\(session_id\)[\s\S]*\.map\(\|session\| session\.path\.clone\(\)\)/,
  "terminal manager should expose the registered path for an explicit session id",
);

assert.match(
  terminalRs,
  /let requested_session_id[\s\S]*query[\s\S]*\.session_id[\s\S]*filter\(\|value\| !value\.is_empty\(\)\)[\s\S]*let directory = match requested_session_id\.as_deref\(\)[\s\S]*Some\(session_id\) => match manager\.session_path\(session_id\)[\s\S]*Some\(path\) => path[\s\S]*None =>[\s\S]*filesystem::resolve_terminal_directory_path/,
  "terminal websocket should prefer the stored session path before validating the URL path",
);
