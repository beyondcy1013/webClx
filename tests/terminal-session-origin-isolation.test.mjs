import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalJs = readFileSync(new URL("../static/terminal.js", import.meta.url), "utf8");
const sessionRenderer = readFileSync(
  new URL("../static/terminal-session-render.js", import.meta.url),
  "utf8",
);
const sessionActions = readFileSync(
  new URL("../static/terminal-sessions.js", import.meta.url),
  "utf8",
);
const workflowActions = readFileSync(
  new URL("../static/terminal-tool-actions.js", import.meta.url),
  "utf8",
);
const presetActions = readFileSync(
  new URL("../static/specified-preset-actions.js", import.meta.url),
  "utf8",
);
const terminalBackend = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");
const terminalRoutes = readFileSync(new URL("../src/routes/terminal.rs", import.meta.url), "utf8");

test("terminal management lists all active sessions without merging their ownership", () => {
  assert.match(terminalHtml, /id="session-switcher"/);
  assert.doesNotMatch(
    terminalHtml,
    /id="agent-session-switcher"|for="agent-session-switcher"/,
    "agent sessions belong on the dedicated Agent page",
  );
  assert.doesNotMatch(
    sessionRenderer,
    /createElement\("optgroup"\)/,
    "session ownership should not add presentation-only groups to the terminal picker",
  );
  assert.match(
    sessionRenderer,
    /renderTerminalSessionSelector\(sessionSelectEl, sessions, selectedSessionId, "终端列表", \{[\s\S]*includePlaceholder: false/,
    "the primary terminal picker should list every active session without a cover option",
  );
});

test("session origin and owner key are persisted by the terminal service", () => {
  assert.match(
    terminalBackend,
    /enum TerminalSessionOrigin[\s\S]*Normal[\s\S]*Workflow[\s\S]*Agent/,
  );
  assert.match(
    terminalBackend,
    /struct StoredTerminalSession \{[\s\S]*#\[serde\(default\)\][\s\S]*origin: TerminalSessionOrigin,[\s\S]*#\[serde\(default\)\][\s\S]*owner_key: String,/,
  );
  assert.match(
    terminalBackend,
    /pub struct CreateSessionRequest \{[\s\S]*origin: TerminalSessionOrigin,[\s\S]*owner_key: String,/,
  );
  assert.match(
    terminalBackend,
    /pub struct TerminalSessionInfo \{[\s\S]*origin: TerminalSessionOrigin,[\s\S]*owner_key: String,/,
  );
});

test("workflow launch creates only classified sessions and reuses its own agent terminal", () => {
  assert.match(
    sessionActions,
    /async function createSession\(\{[\s\S]*origin = "normal",[\s\S]*ownerKey = "",[\s\S]*codexApiPresetId = "",[\s\S]*JSON\.stringify\(\{[\s\S]*path,[\s\S]*origin,[\s\S]*owner_key: ownerKey,[\s\S]*codex_api_preset_id: codexApiPresetId/,
  );
  assert.match(
    workflowActions,
    /case "create_terminal"[\s\S]*createSession\(\{[\s\S]*origin: "workflow",[\s\S]*ownerKey: terminalWorkflowOwnerKey\(executionContext\)/,
  );
  assert.match(
    workflowActions,
    /terminalOwnedAgentSession[\s\S]*session\?\.origin === "agent"[\s\S]*session\?\.owner_key === normalizedOwnerKey/,
    "a workflow must never take over a normal terminal merely because the display name matches",
  );
  assert.match(
    workflowActions,
    /findReusableAgentSession[\s\S]*\/api\/terminal\/sessions\?all=true[\s\S]*insertOrReplaceSession\(session\)/,
    "agent reuse must search all workspaces even when the normal terminal list is path-filtered",
  );
  assert.match(
    presetActions,
    /origin: options\.origin,[\s\S]*ownerKey: options\.ownerKey/,
    "fixed agent launches must forward durable ownership metadata to the terminal creator",
  );
});

test("agent reuse discovers an owned session outside the currently loaded workspace", async () => {
  const insertedSessions = [];
  const remoteAgent = {
    id: "session-agent",
    name: "代理设置",
    origin: "agent",
    owner_key: "proxy_settings_workflow",
    idle: false,
  };
  const sandbox = {
    state: {
      sessions: [{
        id: "session-normal",
        name: "代理设置",
        origin: "normal",
        owner_key: "proxy_settings_workflow",
        idle: false,
      }],
    },
    async requestJson(url) {
      assert.equal(url, "/api/terminal/sessions?all=true");
      return { sessions: [remoteAgent] };
    },
    insertOrReplaceSession(session) {
      insertedSessions.push(session);
    },
  };
  vm.runInNewContext(workflowActions, sandbox, { filename: "terminal-tool-actions.js" });

  const found = await sandbox.findReusableAgentSession("proxy_settings_workflow");

  assert.equal(found.id, remoteAgent.id);
  assert.equal(insertedSessions.at(-1).id, remoteAgent.id);
});

test("agent standby prompt invokes the requested skill without executing its task", () => {
  assert.match(workflowActions, /function terminalWorkflowStandbyPrompt\(rawTask\)/);
  const sandbox = {};
  vm.runInNewContext(workflowActions, sandbox, { filename: "terminal-tool-actions.js" });
  const prompt = sandbox.terminalWorkflowStandbyPrompt(
    "$mihomo-proxy-ops 请检查当前代理配置，并根据当前环境完成代理设置。",
  );
  assert.match(prompt, /^\$mihomo-proxy-ops\b/);
  assert.match(prompt, /仅加载上述技能及必要上下文[\s\S]*待命/);
  assert.doesNotMatch(prompt, /检查当前代理配置|完成代理设置/);
  assert.doesNotMatch(workflowActions, /rawTask\.replace\(\/\^\\\$\[\\w-\]\+\\s\*\//);
});

test("legacy browser-only workflow markers are migrated once into durable session metadata", () => {
  assert.match(
    terminalRoutes,
    /sessions\/\{session_id\}[\s\S]*patch\(terminal::update_session_origin\)/,
  );
  assert.match(
    terminalBackend,
    /pub async fn update_session_origin[\s\S]*UpdateSessionOriginRequest/,
  );
  assert.match(
    workflowActions,
    /LEGACY_WORKFLOW_SESSION_STORAGE_KEY[\s\S]*async function migrateLegacyWorkflowSessionOrigins\(\)[\s\S]*origin: metadata\.origin,[\s\S]*owner_key: metadata\.ownerKey/,
  );
  assert.match(
    sessionActions,
    /await migrateLegacyWorkflowSessionOrigins\(\)/,
    "legacy migration should run before the refreshed list is rendered",
  );
});
