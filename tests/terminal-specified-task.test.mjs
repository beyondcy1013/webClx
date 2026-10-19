import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const terminalStyles = readFileSync(
  new URL("../static/styles-terminal.css", import.meta.url),
  "utf8",
);
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
    /id="terminal-project-command-select"[\s\S]*?<option value="open_specified_task">临时<\/option>/,
  );
  assert.match(terminalHtml, /id="terminal-specified-task-dialog"/);
  assert.match(terminalHtml, /id="terminal-specified-task-agent-list"/);
  assert.match(terminalHtml, /name="terminal-specified-task-mode" value="fixed" checked/);
  assert.match(terminalHtml, /name="terminal-specified-task-mode" value="terminal"/);
  assert.match(terminalHtml, /name="terminal-specified-task-mode" value="exec"/);
  assert.match(
    terminalHtml,
    /name="terminal-specified-task-preset-scope"[\s\S]*?value="terminal"[\s\S]*?checked/,
  );
  assert.match(terminalHtml, /name="terminal-specified-task-preset-scope"[\s\S]*?value="global"/);
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

test("new terminal dialog nests the enabled import-title option under the prompt label", () => {
  for (const html of [terminalHtml, indexHtml]) {
    assert.match(
      html,
      /class="terminal-specified-task-field terminal-specified-task-prompt-field">\s*<div class="terminal-specified-task-prompt-labels">\s*<label id="terminal-specified-task-text-label" for="terminal-specified-task-text">初始提示词<\/label>\s*<label class="terminal-specified-task-import-title-option">[\s\S]*?id="terminal-specified-task-import-title"[\s\S]*?<span>导入标题<\/span>[\s\S]*?<\/label>\s*<\/div>\s*<textarea[\s\S]*?id="terminal-specified-task-text"/,
    );
    assert.match(
      html,
      /id="terminal-specified-task-import-title"[^>]*\bchecked\b/,
    );
    const suffixField = html.match(
      /<div[^>]*class="terminal-specified-task-suffix-field"[\s\S]*?<\/div>/,
    )?.[0] ?? "";
    assert.doesNotMatch(suffixField, /terminal-specified-task-import-title/);
  }
  assert.match(
    specifiedTaskSource,
    /dom\.importTitle\.checked = true/,
  );
  assert.match(
    terminalStyles,
    /\.terminal-specified-task-prompt-labels \{[^}]*align-self: start;[^}]*gap: 2px;/,
  );
});

test("suffix controls keep label, input, and extract action on one row", () => {
  for (const html of [terminalHtml, indexHtml]) {
    assert.match(
      html,
      /class="terminal-specified-task-suffix-label" for="terminal-specified-task-terminal-name-suffix">快速后缀<\/label>[\s\S]*?<input[^>]*id="terminal-specified-task-terminal-name-suffix"[^>]*placeholder="输入后自动添加到编号后"[^>]*>[\s\S]*?id="terminal-specified-task-use-original-suffix"[\s\S]*?>提取原后缀<\/button>/,
    );
  }
  assert.match(
    terminalStyles,
    /\.terminal-specified-task-suffix-field \{[^}]*grid-template-columns: auto minmax\(0, 1fr\) auto;[^}]*align-items: start;/,
  );
  assert.match(
    terminalStyles,
    /\.terminal-specified-task-suffix-field > \.text-input \{[^}]*border-radius: 0;[^}]*background: transparent;/,
  );
  assert.match(
    terminalStyles,
    /@media \(max-width: 640px\)[\s\S]*?\.terminal-specified-task-suffix-field \{[^}]*grid-column: 1 \/ -1;/,
  );
});

test("terminal name row shows the complete original terminal name without ellipsis", () => {
  for (const html of [terminalHtml, indexHtml]) {
    assert.match(
      html,
      /id="terminal-specified-task-terminal-name-row"[\s\S]*?terminal-specified-task-row-title">原终端名<\/span>[\s\S]*?<output[\s\S]*?id="terminal-specified-task-terminal-name"[\s\S]*?class="terminal-specified-task-original-name"[\s\S]*?aria-label="原终端名"/,
    );
    assert.doesNotMatch(html, /<input[^>]*id="terminal-specified-task-terminal-name"/);
    assert.doesNotMatch(
      html,
      /terminal-specified-task-compact-field"[^>]*>[\s\S]*?<span>原终端名<\/span>[\s\S]*?id="terminal-specified-task-terminal-name"/,
    );
  }
  assert.match(
    terminalStyles,
    /\.terminal-specified-task-original-name \{[^}]*overflow-x: auto;[^}]*overflow-y: hidden;[^}]*white-space: nowrap;[^}]*\}/,
  );
  assert.doesNotMatch(
    terminalStyles,
    /\.terminal-specified-task-original-name \{[^}]*text-overflow: ellipsis;/,
  );
  assert.match(
    specifiedTaskSource,
    /dom\.terminalName\.title = terminalSpecifiedTaskSourceName/,
  );
});

test("new terminal dialog calls the task field initial prompt without optional text", () => {
  for (const html of [terminalHtml, indexHtml]) {
    assert.match(html, /id="terminal-specified-task-text-label"[^>]*>初始提示词<\/label>/);
    assert.doesNotMatch(html, /初始提示词（可选）/);
  }
  assert.match(
    specifiedTaskSource,
    /dom\.taskLabel\.textContent = terminalSpecifiedTaskFutureMode\s*\?\s*"提示词"\s*:\s*fixed\s*\?\s*"初始提示词"\s*:\s*"任务"/,
  );
});

test("new terminal synchronizes the editable title into the initial prompt without duplicates", () => {
  const sandbox = {
    document: {
      getElementById() { return null; },
      querySelector() { return null; },
    },
    state: { sessions: [] },
  };
  vm.createContext(sandbox);
  vm.runInContext(specifiedTaskSource, sandbox);

  const imported = sandbox.terminalSpecifiedTaskSyncImportedTitleValue(
    "处理登录流程",
    "",
    "_修复登录",
    true,
  );
  assert.deepEqual(
    { task: imported.task, importedTitle: imported.importedTitle },
    { task: "处理登录流程\n修复登录", importedTitle: "修复登录" },
  );
  const updated = sandbox.terminalSpecifiedTaskSyncImportedTitleValue(
    imported.task,
    imported.importedTitle,
    "_修复登录表单",
    true,
  );
  assert.deepEqual(
    { task: updated.task, importedTitle: updated.importedTitle },
    { task: "处理登录流程\n修复登录表单", importedTitle: "修复登录表单" },
  );
  const manuallyEdited = sandbox.terminalSpecifiedTaskSyncImportedTitleValue(
    "处理登录流程\n自定义标题",
    updated.importedTitle,
    "",
    false,
  );
  assert.equal(manuallyEdited.task, "处理登录流程\n自定义标题");
  assert.match(
    specifiedTaskSource,
    /dom\.terminalNameSuffix\?\.addEventListener\("input",[\s\S]*?syncTerminalSpecifiedTaskImportedTitle\(\)/,
  );
  assert.match(
    specifiedTaskSource,
    /dom\.importTitle\?\.addEventListener\("change", syncTerminalSpecifiedTaskImportedTitle\)/,
  );
  assert.match(
    specifiedTaskSource,
    /sourceTerminalName:[\s\S]*?task,[\s\S]*?origin:/,
  );
  assert.doesNotMatch(
    specifiedTaskSource,
    /const launchTask = terminalSpecifiedTaskTaskWithNameSuffix/,
  );
});

test("fixed terminal flow creates, renames, waits, and starts the selected native agent", () => {
  assert.match(specifiedTaskSource, /async function launchTerminalSpecifiedPreset/);
  assert.match(
    specifiedTaskSource,
    /createSession\(\{[\s\S]*?path: cwd,[\s\S]*?codexApiPresetId: options\.codexApiPresetId[\s\S]*?renameTerminalForTool\(created\.id, terminalName, cwd\)[\s\S]*?waitForTerminalToolSessionReady\(created\.id\)[\s\S]*?sendTerminalAutoTypedInput\(runCommand/,
  );
  assert.match(specifiedTaskSource, /action: "launch"[\s\S]*?agent,[\s\S]*?sessionAction,[\s\S]*?sessionId:/);
  assert.match(specifiedTaskSource, /sourceTerminalName:/);
  assert.match(specifiedTaskSource, /terminalName: terminalSpecifiedTaskFinalTerminalName\(\)/);
  assert.match(specifiedTaskSource, /namingAction === "new"[\s\S]*?return ""/);
  assert.doesNotMatch(specifiedTaskSource, /`\$\{namingBase\}_new`/);
  assert.match(
    specifiedTaskSource,
    /terminalNameSuffix\?\.addEventListener\("input",[\s\S]*?renderTerminalSpecifiedTaskTerminalNamePreview\(\)/,
  );
  assert.match(
    specifiedTaskSource,
    /setTerminalSpecifiedTaskStatus\([\s\S]*?已启动交互终端[\s\S]*?reportTerminalSpecifiedTaskPageStatus\([\s\S]*?全局预设未改变[\s\S]*?closeTerminalSpecifiedTaskDialog\(\)[\s\S]*?return/,
  );
  assert.match(specifiedTaskSource, /agent === "claude" && mode !== "fixed"/);
});

test("new terminal dialog supports configurable Devin direct launch and resume", () => {
  assert.match(specifiedTaskSource, /function renderTerminalSpecifiedTaskAgents/);
  assert.match(specifiedTaskSource, /function terminalSpecifiedTaskAgentLaunchCommand/);
  assert.match(
    specifiedTaskSource,
    /template\.split\("\{session_id\}"\)\.join\(shellQuoteSpecifiedPresetArgument\(normalized\)\)/,
  );
  assert.match(
    specifiedTaskSource,
    /恢复 \$\{command_\.label \|\| command_\.key\} 会话需要有效的 session ID/,
  );
});

test("new terminal suffix uses the backend-assigned automatic name", async () => {
  const calls = [];
  const created = { id: "created-session", name: "webClx_2", path: "webClx" };
  const sandbox = {
    document: {
      getElementById() { return null; },
      querySelector() { return null; },
    },
    state: {
      activeSessionId: "source-session",
      currentPath: "webClx",
      sessions: [
        { id: "source-session", name: "webClx_1_工作中终端下拉列表的顺序", path: "webClx" },
      ],
      pendingCreatedSessionIds: new Set(),
    },
    async createSession() {
      return created;
    },
    async renameTerminalForTool(sessionId, name, path) {
      calls.push(["rename", sessionId, name, path]);
      return { ...created, name };
    },
    async waitForTerminalToolSessionReady(sessionId) {
      calls.push(["ready", sessionId]);
    },
    async sendTerminalAutoTypedInput(command, options) {
      calls.push(["send", command, options.sessionId]);
      return true;
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(specifiedTaskSource, sandbox);

  const launched = await sandbox.launchTerminalSpecifiedPreset("webClx", {
    terminalName: "webClx_1_终端如何与deepseek agent通讯",
    terminalNameSuffix: "终端如何与deepseek agent通讯",
    runCommand: "codex",
  });

  assert.equal(launched.name, "webClx_2_终端如何与deepseek agent通讯");
  assert.deepEqual(calls[0], [
    "rename",
    "created-session",
    "webClx_2_终端如何与deepseek agent通讯",
    "webClx",
  ]);
});

test("other terminal accepts an empty command and only creates the terminal", async () => {
  const calls = [];
  const created = { id: "created-session", name: "webClx_2", path: "webClx" };
  const sandbox = {
    document: {
      getElementById() { return null; },
      querySelector() { return null; },
    },
    state: {
      activeSessionId: "source-session",
      currentPath: "webClx",
      sessions: [],
      pendingCreatedSessionIds: new Set(),
    },
    async createSession(options) {
      calls.push(["create", options.path]);
      return created;
    },
    async waitForTerminalToolSessionReady(sessionId) {
      calls.push(["ready", sessionId]);
    },
    async sendTerminalAutoTypedInput() {
      calls.push(["send"]);
      throw new Error("empty terminal launch must not send input");
    },
  };
  vm.createContext(sandbox);
  vm.runInContext(specifiedTaskSource, sandbox);

  const launched = await sandbox.launchTerminalSpecifiedPreset("webClx", {
    runCommand: "",
    allowEmptyCommand: true,
    origin: "agent",
    ownerKey: "manual-other-terminal",
  });

  assert.equal(launched.id, created.id);
  assert.deepEqual(calls, [
    ["create", "webClx"],
    ["ready", created.id],
  ]);
  assert.match(
    specifiedTaskSource,
    /dom\.otherCommand\.required = isOther && sessionAction !== "new"/,
  );
  assert.match(
    specifiedTaskSource,
    /allowEmptyCommand: agent === "other" && sessionAction === "new"/,
  );
});

test("new terminal preview loads the current path numbering from the backend", async () => {
  const preview = { textContent: "" };
  const requestedUrls = [];
  const sandbox = {
    document: {
      getElementById(id) {
        return id === "terminal-specified-task-terminal-name-preview" ? preview : null;
      },
      querySelector(selector) {
        if (selector.includes("terminal-specified-task-session-action")) {
          return { value: "new" };
        }
        return null;
      },
    },
    state: {
      activeSessionId: "",
      currentPath: "webClx",
      workspaceDir: "/home/codes",
      workspaceHistory: [],
      favoritePaths: [],
      sessions: [],
    },
    async requestJson(url) {
      requestedUrls.push(url);
      return {
        sessions: [
          { name: "webClx_1_工作中终端下拉列表的顺序", path: "webClx" },
          { name: "webClx_14", path: "webClx" },
          { name: "webClx_15_终端如何与deepseek agent通讯", path: "webClx" },
        ],
      };
    },
    specifiedPresetSessionAction: (action) => action || "new",
  };
  vm.createContext(sandbox);
  vm.runInContext(specifiedTaskSource, sandbox);

  await sandbox.setTerminalSpecifiedTaskSourcePath("/home/codes/tools/webClx");

  assert.deepEqual(requestedUrls, ["/api/terminal/sessions?path=webClx"]);
  assert.equal(preview.textContent, "webClx_16");
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
      await assert.rejects(
        launch,
        failureStage === "send"
          ? /Agent 启动命令为空/
          : new RegExp(`${failureStage} failed`),
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

test("new terminals keep backend auto naming while derived actions use the source prefix", () => {
  const terminalName = { value: "webClx_18_整合预设" };
  let sessionAction = "new";
  const sandbox = {
    document: {
      getElementById(id) {
        return id === "terminal-specified-task-terminal-name" ? terminalName : null;
      },
      querySelector(selector) {
        if (selector.includes("terminal-specified-task-session-action")) {
          return { value: sessionAction };
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

  assert.equal(sandbox.terminalSpecifiedTaskFinalTerminalName(), "");

  sessionAction = "fork";
  assert.equal(sandbox.terminalSpecifiedTaskFinalTerminalName(), "webClx_整合预设_fork");
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

test("native task backend uses normal preset apply without a runtime gate", () => {
  assert.match(taskRoutesSource, /"\/api\/codex\/tasks"/);
  assert.match(taskBackendSource, /auth::current_api_preset_id/);
  assert.match(taskBackendSource, /auth::apply_api_preset_for_run/);
  assert.doesNotMatch(taskBackendSource, /preset_run_lease|heartbeat|全局预设门禁/);
  assert.match(
    taskBackendSource,
    /await_verified_launch\([\s\S]*?\.await\?;[\s\S]*?restore_original_api_preset\([\s\S]*?monitor_task\(/,
    "temporary task configuration must be restored after verified launch, before task monitoring",
  );
  assert.doesNotMatch(taskBackendSource, /runner_file|render_runner_script/);
  assert.match(taskBackendSource, /assert!\(!command\.contains\("run\.sh"\)\)/);
  assert.match(taskBackendSource, /spawn_direct_codex/);
  assert.match(taskBackendSource, /api_preset_model\(&applied_preset\)/);
  assert.match(taskBackendSource, /actual_model/);
  assert.match(taskBackendSource, /close_owned_terminal\(&state, terminal_id\.as_deref\(\)\)/);
});

test("opening browse falls back from a missing path to the default workspace once", async () => {
  for (const failure of ["路径不存在: deleted", "目录不存在: deleted", "网络错误"]) {
    const calls = [];
    const element = () => ({ dataset: {}, value: "", textContent: "", append() {}, replaceChildren() {}, showModal() {} });
    const dom = { pathBody: element(), pathInput: element(), pathSelected: element(), pathDialog: element() };
    const sandbox = {
      state: { workspaceDir: "/workspace" },
      document: { createElement: element },
      requestJson: async (url) => {
        calls.push(url);
        if (calls.length === 1) throw new Error(failure);
        return { path: "", display_path: "/workspace", parent_path: null, entries: [] };
      },
    };
    vm.createContext(sandbox);
    vm.runInContext(specifiedTaskSource, sandbox);
    sandbox.terminalSpecifiedTaskDom = () => dom;
    vm.runInContext('terminalSpecifiedTaskSourcePath = "/workspace/deleted"', sandbox);
    await sandbox.openTerminalSpecifiedTaskPathDialog();
    if (failure === "网络错误") {
      assert.deepEqual(calls, ["/api/entries?path=deleted"]);
      assert.match(dom.pathBody.textContent, /网络错误/);
      assert.equal(vm.runInContext('terminalSpecifiedTaskBrowseDisplayPath', sandbox), "");
    } else {
      assert.deepEqual(calls, ["/api/entries?path=deleted", "/api/entries"]);
      assert.equal(dom.pathInput.value, "/workspace");
      assert.equal(dom.pathSelected.textContent, "/workspace");
      calls.length = 0;
      sandbox.requestJson = async (url) => { calls.push(url); throw new Error(failure); };
      await sandbox.openTerminalSpecifiedTaskPathDialog();
      assert.equal(calls.length, 2);
      assert.match(dom.pathBody.textContent, /读取目录失败/);
      assert.equal(vm.runInContext('terminalSpecifiedTaskBrowseDisplayPath', sandbox), "");
    }
  }
});

test("new dialog keeps every Codex account while Claude filters sub2api", async () => {
  const options = [];
  const preset = {
    dataset: { agent: "codex" }, value: "imported-oauth", disabled: false,
    replaceChildren() { options.length = 0; this.value = ""; },
    append(option) { options.push(option); },
  };
  const requested = [];
  const sandbox = {
    document: { createElement: () => ({ dataset: {} }) },
    requestJson: async (endpoint) => {
      requested.push(endpoint);
      return {
        accounts: [
          { id: "imported-oauth", kind: "oauth", name: "Imported account", active: true },
          { id: "sub2api-api", kind: "api", name: "sub2api API", model: "model-a", active: false },
          { id: "api-account", kind: "api", name: "Regular account", favorite: true },
        ],
        presets: [
          { id: "api-preset", name: "Regular API", favorite: true },
          { id: "sub2api-name", name: "Sub2API API", favorite: true },
          { id: "sub2api-provider", name: "Renamed", provider_name: "sub2api" },
          { id: "sub2api-url", name: "Renamed", base_url: "https://sub2api.example/v1" },
          { id: "sub2api-admin", name: "Renamed", management_url: "https://example/sub2api" },
        ],
      };
    },
    specifiedPresetListEndpoint: (agent) => `/api/auth/${agent === "claude" ? "claude" : "api"}-presets`,
    specifiedPresetModel: () => "",
  };
  vm.createContext(sandbox);
  vm.runInContext(specifiedTaskSource, sandbox);
  sandbox.terminalSpecifiedTaskDom = () => ({ preset });
  sandbox.terminalSpecifiedTaskAgent = () => "codex";
  await sandbox.loadTerminalSpecifiedTaskPresets();
  assert.deepEqual(requested, ["/api/auth/codex-accounts"]);
  assert.deepEqual(options.map((option) => option.value), [
    "api-account",
    "imported-oauth",
    "sub2api-api",
  ]);
  assert.equal(options[0].dataset.kind, "api");
  assert.equal(options[1].dataset.kind, "oauth");
  assert.equal(preset.value, "api-account");
  sandbox.terminalSpecifiedTaskAgent = () => "claude";
  await sandbox.loadTerminalSpecifiedTaskPresets();
  assert.equal(requested[1], "/api/auth/claude-presets");
  assert.deepEqual(options.map((option) => option.value), ["api-preset"]);
});

test("new terminal dialog includes session-only checkbox beside preset selector", () => {
  for (const html of [terminalHtml, indexHtml]) {
    assert.match(
      html,
      /id="terminal-specified-task-preset-field"[\s\S]*?id="terminal-specified-task-session-only"[\s\S]*?仅对此会话/,
    );
  }
  assert.match(
    terminalStyles,
    /\.terminal-specified-task-preset-field\s*\{[\s\S]*?grid-template-columns:\s*auto minmax\(0, 1fr\) auto/,
  );
  assert.match(terminalStyles, /\.terminal-specified-task-session-only-label/);
  assert.match(
    specifiedTaskSource,
    /sessionOnly:\s*document\.getElementById\("terminal-specified-task-session-only"\)/,
  );
  assert.match(
    specifiedTaskSource,
    /if \(dom\.sessionOnly\) \{\s*return dom\.sessionOnly\.checked === true;\s*\}/,
  );
});

test("new terminal dialog displays matching LLM_API accounts and label for each agent platform", async () => {
  const options = [];
  const requested = [];
  const preset = {
    value: "",
    dataset: {},
    replaceChildren() {
      options.length = 0;
    },
    append(child) {
      options.push(child);
      if (!this.value || child.selected) {
        this.value = child.value;
      }
    },
  };
  const sandbox = {
    document: {
      createElement() {
        return { dataset: {} };
      },
    },
    terminalSpecifiedTaskId: "",
    terminalSpecifiedTaskDom: () => ({ preset }),
    specifiedPresetModel: () => "",
    requestJson: async (url) => {
      requested.push(url);
      if (url === "/api/auth/codebuddy-accounts") {
        return [
          { id: "cb-1", nickname: "Buddy User", active: true },
          { id: "cb-2", email: "work@company.com", active: false },
        ];
      }
      if (url === "/api/auth/devin-accounts") {
        return [
          { id: "dev-1", name: "Devin Personal", active: false },
          { id: "dev-2", name: "Devin Org", active: true },
        ];
      }
      if (url === "/api/agy/presets") {
        return [
          { id: "agy-1", name: "AGY Main", current: true },
          { id: "agy-2", email: "backup@gmail.com", current: false },
        ];
      }
      return [];
    },
  };

  vm.createContext(sandbox);
  vm.runInContext(specifiedTaskSource, sandbox);
  sandbox.terminalSpecifiedTaskDom = () => ({ preset });

  assert.equal(sandbox.terminalSpecifiedTaskAccountLabel("codex"), "Codex 账号");
  assert.equal(sandbox.terminalSpecifiedTaskAccountLabel("claude"), "Claude 账号");
  assert.equal(sandbox.terminalSpecifiedTaskAccountLabel("codebuddy"), "CodeBuddy 账号");
  assert.equal(sandbox.terminalSpecifiedTaskAccountLabel("devin"), "Devin 账号");
  assert.equal(sandbox.terminalSpecifiedTaskAccountLabel("agy"), "AGY 账号");

  assert.equal(sandbox.terminalSpecifiedTaskHasAccounts("codex"), true);
  assert.equal(sandbox.terminalSpecifiedTaskHasAccounts("claude"), true);
  assert.equal(sandbox.terminalSpecifiedTaskHasAccounts("codebuddy"), true);
  assert.equal(sandbox.terminalSpecifiedTaskHasAccounts("devin"), true);
  assert.equal(sandbox.terminalSpecifiedTaskHasAccounts("agy"), true);
  assert.equal(sandbox.terminalSpecifiedTaskHasAccounts("other"), false);

  // Test CodeBuddy accounts
  sandbox.terminalSpecifiedTaskAgent = () => "codebuddy";
  await sandbox.loadTerminalSpecifiedTaskPresets();
  assert.equal(requested.includes("/api/auth/codebuddy-accounts"), true);
  assert.deepEqual(options.map((o) => o.value), ["cb-1", "cb-2"]);
  assert.equal(preset.value, "cb-1");
  assert.equal(preset.dataset.activeId, "cb-1");

  // Test Devin accounts
  sandbox.terminalSpecifiedTaskAgent = () => "devin";
  await sandbox.loadTerminalSpecifiedTaskPresets();
  assert.equal(requested.includes("/api/auth/devin-accounts"), true);
  assert.deepEqual(options.map((o) => o.value), ["dev-1", "dev-2"]);
  assert.equal(preset.value, "dev-2");
  assert.equal(preset.dataset.activeId, "dev-2");

  // Test AGY presets
  sandbox.terminalSpecifiedTaskAgent = () => "agy";
  await sandbox.loadTerminalSpecifiedTaskPresets();
  assert.equal(requested.includes("/api/agy/presets"), true);
  assert.deepEqual(options.map((o) => o.value), ["agy-1", "agy-2"]);
  assert.equal(preset.value, "agy-1");
  assert.equal(preset.dataset.activeId, "agy-1");

  // Test apply endpoints
  const appliedEndpoints = [];
  sandbox.requestJson = async (url, opts) => {
    appliedEndpoints.push({ url, method: opts?.method });
    return { ok: true };
  };
  await sandbox.applyTerminalSpecifiedAgentAccount("codebuddy", "cb-2");
  assert.deepEqual(appliedEndpoints[0], { url: "/api/auth/codebuddy-accounts/cb-2/switch", method: "PUT" });

  await sandbox.applyTerminalSpecifiedAgentAccount("devin", "dev-1");
  assert.deepEqual(appliedEndpoints[1], { url: "/api/auth/devin-accounts/dev-1/switch", method: "PUT" });

  await sandbox.applyTerminalSpecifiedAgentAccount("agy", "agy-2");
  assert.deepEqual(appliedEndpoints[2], { url: "/api/agy/presets/agy-2/apply", method: "PUT" });
});


