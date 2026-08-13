import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const specifiedTaskSource = readFileSync(
  new URL("../static/terminal-specified-task.js", import.meta.url),
  "utf8",
);
const sharedSpecifiedPresetSource = readFileSync(
  new URL("../static/specified-preset-actions.js", import.meta.url),
  "utf8",
);
const toolActionsSource = readFileSync(
  new URL("../static/terminal-tool-actions.js", import.meta.url),
  "utf8",
);
const taskBackendSource = readFileSync(new URL("../src/codex_task.rs", import.meta.url), "utf8");
const taskRoutesSource = readFileSync(
  new URL("../src/routes/codex_task.rs", import.meta.url),
  "utf8",
);

test("terminal page exposes the unified temporary specified preset dialog with fixed mode by default", () => {
  assert.match(
    terminalHtml,
    /id="terminal-project-command-select"[\s\S]*?<option value="open_specified_task">指定任务\/预设<\/option>/,
  );
  assert.match(terminalHtml, /id="terminal-specified-task-dialog"/);
  assert.match(terminalHtml, /name="terminal-specified-task-agent" value="codex" checked/);
  assert.match(terminalHtml, /name="terminal-specified-task-agent" value="claude"/);
  assert.match(terminalHtml, /name="terminal-specified-task-mode" value="fixed" checked/);
  assert.match(terminalHtml, /name="terminal-specified-task-mode" value="terminal"/);
  assert.match(terminalHtml, /name="terminal-specified-task-mode" value="exec"/);
  assert.match(terminalHtml, /name="terminal-specified-task-session-action" value="resume"/);
  assert.match(terminalHtml, /name="terminal-specified-task-session-action" value="fork"/);
  assert.match(terminalHtml, /id="terminal-specified-task-session-id"/);
  assert.match(terminalHtml, /id="terminal-specified-task-terminal-name"/);
  assert.match(terminalHtml, /id="terminal-specified-task-terminal-name-preview"/);
  assert.match(terminalHtml, /id="terminal-specified-task-preset"/);
  assert.match(terminalHtml, /id="terminal-specified-task-text"/);
  assert.match(terminalHtml, /id="terminal-specified-task-result"/);
  assert.match(terminalHtml, /terminal-specified-task\.js\?v=/);
  assert.match(terminalHtml, /specified-preset-actions\.js\?v=/);
});

test("fixed terminal flow creates, renames, waits, and starts the selected native agent", () => {
  assert.match(specifiedTaskSource, /async function launchTerminalSpecifiedPreset/);
  assert.match(
    specifiedTaskSource,
    /createSession\(\{[\s\S]*?path: cwd,[\s\S]*?codexApiPresetId: options\.codexApiPresetId[\s\S]*?renameTerminalForTool\(created\.id, options\.terminalName, cwd\)[\s\S]*?waitForTerminalToolSessionReady\(created\.id\)[\s\S]*?sendTerminalAutoTypedInput\(options\.runCommand/,
  );
  assert.match(specifiedTaskSource, /action: "launch"[\s\S]*?agent,[\s\S]*?sessionAction,[\s\S]*?sessionId:/);
  assert.match(specifiedTaskSource, /sourceTerminalName:/);
  assert.match(specifiedTaskSource, /terminalName: terminalSpecifiedTaskFinalTerminalName\(\)/);
  assert.match(specifiedTaskSource, /namingAction === "new"[\s\S]*?`\$\{namingBase\}_new`/);
  assert.match(
    specifiedTaskSource,
    /terminalName\?\.addEventListener\("input", renderTerminalSpecifiedTaskTerminalNamePreview\)/,
  );
  assert.match(
    specifiedTaskSource,
    /setTerminalSpecifiedTaskStatus\([\s\S]*?已启动固定终端[\s\S]*?updateStatus\([\s\S]*?已按指定预设启动[\s\S]*?该预设已设为当前共享配置[\s\S]*?closeTerminalSpecifiedTaskDialog\(\)[\s\S]*?return/,
  );
  assert.match(specifiedTaskSource, /agent === "claude" && mode !== "fixed"/);
});

test("failed fixed terminal launches delete only the newly created session", async (t) => {
  for (const failureStage of ["rename", "ready", "send"]) {
    await t.test(failureStage, async () => {
      const calls = [];
      const created = { id: "created-session", name: "webClx_19", path: "webClx" };
      const source = { id: "source-session", name: "webClx_18", path: "webClx" };
      const other = { id: "other-session", name: "webClx_20", path: "webClx" };
      const sandbox = {
        document: {
          getElementById() { return null; },
          querySelector() { return null; },
        },
        state: {
          activeSessionId: source.id,
          currentPath: "webClx",
          sessions: [source, other],
          pendingCreatedSessionIds: new Set(),
        },
        async createSession() {
          sandbox.state.sessions.push(created);
          sandbox.state.pendingCreatedSessionIds.add(created.id);
          sandbox.state.activeSessionId = created.id;
          return created;
        },
        async renameTerminalForTool() {
          if (failureStage === "rename") {
            throw new Error("rename failed");
          }
          return { ...created, name: "webClx_new" };
        },
        async waitForTerminalToolSessionReady() {
          if (failureStage === "ready") {
            sandbox.state.activeSessionId = other.id;
            throw new Error("ready failed");
          }
        },
        async sendTerminalAutoTypedInput() {
          return failureStage !== "send";
        },
        async requestJson(path, options) {
          calls.push(["delete", path, options]);
          return created;
        },
        announceSessionMutation(type, session) {
          calls.push(["announce", type, session.id]);
        },
        closeSocket(options) {
          calls.push(["closeSocket", options]);
        },
        clearActiveSession() {
          calls.push(["clearActiveSession"]);
          sandbox.state.activeSessionId = "";
        },
        updateStatus(message, tone) {
          calls.push(["status", message, tone]);
        },
        disposeTerminalSessionContext(sessionId) {
          calls.push(["dispose", sessionId]);
        },
        forgetSessionPreference(path, sessionId) {
          calls.push(["forget", path, sessionId]);
        },
        async loadSessions(options) {
          calls.push(["loadSessions", options]);
        },
      };
      vm.createContext(sandbox);
      vm.runInContext(specifiedTaskSource, sandbox);

      const launch = sandbox.launchTerminalSpecifiedPreset("webClx", {
        terminalName: "webClx_new",
        runCommand: "codex",
      });
      if (failureStage === "rename") {
        const launched = await launch;
        assert.equal(launched.id, created.id);
        assert.equal(calls.some(([type]) => type === "delete"), false);
        return;
      }
      await assert.rejects(
        launch,
        failureStage === "send" ? /Agent 启动命令为空/ : new RegExp(`${failureStage} failed`),
      );

      const deleteCall = calls.find(([type]) => type === "delete");
      deleteCall[2] = JSON.parse(JSON.stringify(deleteCall[2]));
      assert.deepEqual(
        deleteCall,
        [
          "delete",
          "/api/terminal/sessions/created-session",
          {
            method: "DELETE",
            headers: {
              "X-WebClx-Confirm-Session": created.id,
              "X-WebClx-Delete-Source": "specified-preset-launch",
            },
          },
        ],
      );
      assert.equal(sandbox.state.pendingCreatedSessionIds.has(created.id), false);
      assert.deepEqual(calls.find(([type]) => type === "dispose"), ["dispose", created.id]);
      assert.deepEqual(calls.find(([type]) => type === "forget"), ["forget", "webClx", created.id]);
      const loadSessionsCall = calls.find(([type]) => type === "loadSessions");
      loadSessionsCall[1] = JSON.parse(JSON.stringify(loadSessionsCall[1]));
      assert.deepEqual(
        loadSessionsCall,
        ["loadSessions", { preferredSessionId: source.id, forcePreferredSession: true }],
      );
      const switchedAway = failureStage === "ready";
      assert.equal(calls.some(([type]) => type === "closeSocket"), !switchedAway);
      assert.equal(calls.some(([type]) => type === "clearActiveSession"), !switchedAway);
      assert.equal(
        calls.some(([, path]) => path === "/api/terminal/sessions/other-session"),
        false,
      );
    });
  }
});

test("final terminal names remove occupied auto indices and remain unique", () => {
  const terminalName = { value: "webClx_18_整合预设" };
  const sandbox = {
    document: {
      getElementById(id) {
        return id === "terminal-specified-task-terminal-name" ? terminalName : null;
      },
      querySelector(selector) {
        if (selector.includes("terminal-specified-task-session-action")) {
          return { value: "new" };
        }
        return null;
      },
    },
    state: {
      activeSessionId: "source",
      currentPath: "webClx",
      sessions: [
        { id: "source", name: "webClx_18_整合预设", path: "webClx" },
        { id: "existing", name: "webClx_整合预设_new", path: "webClx" },
      ],
    },
    sessionPath: (session) => session.path,
    specifiedPresetSessionAction: (action) => action || "new",
    specifiedPresetTerminalName({ sourceTerminalName, sessionAction }) {
      return sessionAction === "new"
        ? sourceTerminalName
        : `${sourceTerminalName}_${sessionAction}`;
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(specifiedTaskSource, sandbox);

  assert.equal(
    sandbox.terminalSpecifiedTaskFinalTerminalName(),
    "webClx_整合预设_new-2",
  );
});

test("specified dialog creates, polls, and can cancel persistent Codex tasks", () => {
  assert.match(specifiedTaskSource, /executeSpecifiedPreset\(\{[\s\S]*?action: "task"/);
  assert.match(sharedSpecifiedPresetSource, /requestJson\("\/api\/codex\/tasks",\s*\{/);
  assert.match(
    sharedSpecifiedPresetSource,
    /requestJson\(`\/api\/codex\/tasks\/\$\{encodeURIComponent\(id\)\}`\)/,
  );
  assert.match(
    specifiedTaskSource,
    /`\/api\/codex\/tasks\/\$\{encodeURIComponent\(terminalSpecifiedTaskId\)\}`[\s\S]*?method: "DELETE"/,
  );
  assert.match(specifiedTaskSource, /terminal_closed \? "已关闭" : "关闭失败"/);
});

test("weapon Codex actions use the same task API helper and frozen source path", () => {
  assert.match(toolActionsSource, /case "codex_exec":[\s\S]*?case "codex_terminal"/);
  assert.match(toolActionsSource, /executeSpecifiedPreset\(\{[\s\S]*?action: "task"[\s\S]*?presetId,[\s\S]*?cwd: executionContext\.sourcePath/);
  assert.match(toolActionsSource, /showTerminalCodexTaskResult\(record, \{ source: "tool" \}\)/);
});

test("native task backend holds and restores the global preset lease without a runner script", () => {
  assert.match(taskRoutesSource, /"\/api\/codex\/tasks"/);
  assert.match(taskBackendSource, /auth::begin_preset_run_lease/);
  assert.match(taskBackendSource, /auth::release_preset_run_lease_internal/);
  assert.match(taskBackendSource, /heartbeat_preset_run_lease_internal/);
  assert.doesNotMatch(taskBackendSource, /runner_file|render_runner_script/);
  assert.match(taskBackendSource, /assert!\(!command\.contains\("run\.sh"\)\)/);
  assert.match(taskBackendSource, /spawn_direct_codex/);
  assert.match(taskBackendSource, /api_preset_model\(&preset\)/);
  assert.match(taskBackendSource, /actual_model/);
  assert.match(taskBackendSource, /close_owned_terminal\(&state, terminal_id\.as_deref\(\)\)/);
});
