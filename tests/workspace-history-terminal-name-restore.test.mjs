import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const appJs = readEntryScriptBundle("index.html");
const appHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const stylesBase = readFileSync(new URL("../static/styles-base.css", import.meta.url), "utf8");

function functionSource(source, name) {
  const functionStart = source.indexOf(`function ${name}(`);
  assert.notEqual(functionStart, -1, `missing function ${name}`);
  const asyncStart = source.lastIndexOf("async ", functionStart);
  const start = asyncStart >= 0 && source.slice(asyncStart, functionStart) === "async "
    ? asyncStart
    : functionStart;
  const bodyStart = source.indexOf("{", start);
  let depth = 0;
  for (let index = bodyStart; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(start, index + 1);
  }
  assert.fail(`unterminated function ${name}`);
}

assert.match(
  appJs,
  /let workspaceHistoryTerminalArchivePersistQueue = Promise\.resolve\(\);[\s\S]*function queueWorkspaceHistoryTerminalArchiveWrite\(operation\)[\s\S]*workspaceHistoryTerminalArchivePersistQueue\.then\(operation\)[\s\S]*workspaceHistoryTerminalArchivePersistQueue = persistence\.catch[\s\S]*function persistWorkspaceHistoryTerminalArchive\(session, detected\)[\s\S]*queueWorkspaceHistoryTerminalArchiveWrite/,
  "workspace history should serialize resume archive writes because the registry is file-backed",
);

assert.match(
  appJs,
  /async function persistWorkspaceHistoryTerminalArchiveNow\(session, detected\)[\s\S]*const cwd = workspaceHistoryArchiveCwd\(sessionPath, existing\)[\s\S]*requestJson\("\/api\/terminal\/resume-archives", \{[\s\S]*method: "POST"[\s\S]*resume_id: resumeId[\s\S]*cwd,[\s\S]*terminal_name: terminalName/,
  "workspace history should persist the active terminal name with a relative archive cwd",
);

assert.match(
  appJs,
  /async function hydrateWorkspaceHistoryTerminalSessionIds[\s\S]*session\.agent_session_id = response\.resume_id \|\| ""[\s\S]*persistWorkspaceHistoryTerminalArchive\(session, response\)/,
  "active terminal session detection should persist the discovered terminal name mapping",
);

assert.match(
  appJs,
  /state\.terminalArchives\.forEach\(\(archive\) => \{[\s\S]*activeTerminalNameBySessionId\.has\(resumeId\)[\s\S]*return;[\s\S]*type: "archive"/,
  "an automatically recorded archive should not duplicate its still-active terminal row",
);

assert.match(
  appJs,
  /async function openFreshTerminalSession\([\s\S]*terminalName = ""[\s\S]*requestJson\("\/api\/terminal\/sessions", \{[\s\S]*method: "POST"[\s\S]*renameFreshTerminalForRestore\(session, requestedPath, terminalName\)[\s\S]*window\.location\.assign/,
  "restoring a conversation should rename the concrete terminal through the API before navigation",
);

assert.match(
  appJs,
  /function openFreshTerminalRunLink\([\s\S]*terminalName = ""[\s\S]*openFreshTerminalSession\(path, \{[\s\S]*terminalName/,
  "resume links should forward the recorded terminal name to fresh-session creation",
);

assert.match(
  appJs,
  /item\.type === "archive"[\s\S]*openFreshTerminalRunLink\(event, workingPath, command, \{[\s\S]*terminalName: item\.activeTerminalName/,
  "archive restore should request the archived terminal name",
);

assert.match(
  appJs,
  /item\.sessionId[\s\S]*openFreshTerminalRunLink\(event, workingPath, command, \{[\s\S]*terminalName: item\.activeTerminalName/,
  "conversation restore should request the recorded terminal name",
);

{
  const requestedNames = [];
  const context = vm.createContext({
    announceSessionMutation() {},
    encodeURIComponent,
    requestJson: async (_url, options) => {
      const { name } = JSON.parse(options.body);
      requestedNames.push(name);
      if (requestedNames.length < 3) {
        throw new Error(`修改会话名称失败: 名称 \`${name}\` 已存在，请使用其他名称。`);
      }
      return { id: "s-new", name, path: "/home/codes/webClx" };
    },
  });
  for (const name of [
    "restoredTerminalNameForAttempt",
    "isRestoredTerminalNameConflict",
    "renameFreshTerminalForRestore",
  ]) {
    vm.runInContext(functionSource(appJs, name), context);
  }

  const renamed = await vm.runInContext(
    'renameFreshTerminalForRestore({ id: "s-new", name: "webClx_4", path: "/home/codes/webClx" }, "/home/codes/webClx", "原终端")',
    context,
  );
  assert.deepEqual(
    requestedNames,
    ["原终端", "原终端2", "原终端3"],
    "restore rename should append an increasing numeric suffix when the archived name is occupied",
  );
  assert.equal(renamed.name, "原终端3");
}

{
  let attempts = 0;
  const context = vm.createContext({
    announceSessionMutation() {},
    encodeURIComponent,
    requestJson: async () => {
      attempts += 1;
      throw new Error("修改会话名称失败: 会话不存在");
    },
  });
  for (const name of [
    "restoredTerminalNameForAttempt",
    "isRestoredTerminalNameConflict",
    "renameFreshTerminalForRestore",
  ]) {
    vm.runInContext(functionSource(appJs, name), context);
  }

  await assert.rejects(
    vm.runInContext(
      'renameFreshTerminalForRestore({ id: "s-new", name: "webClx_4", path: "/home/codes/webClx" }, "/home/codes/webClx", "原终端")',
      context,
    ),
    /会话不存在/,
  );
  assert.equal(attempts, 1, "restore rename should not retry unrelated API failures");
}

{
  const requestedNames = [];
  const context = vm.createContext({
    announceSessionMutation() {},
    encodeURIComponent,
    requestJson: async (_url, options) => {
      const { name } = JSON.parse(options.body);
      requestedNames.push(name);
      if (requestedNames.length > 4) {
        throw new Error("restore rename retry loop did not converge");
      }
      if (name.includes("webClx_7_")) {
        throw new Error(`修改会话名称失败: 名称 \`${name}\` 的自动编号已被占用，请使用其他编号。`);
      }
      return { id: "s-new", name, path: "webClx" };
    },
  });
  for (const name of [
    "restoredTerminalNameForAttempt",
    "isRestoredTerminalNameConflict",
    "renameFreshTerminalForRestore",
  ]) {
    vm.runInContext(functionSource(appJs, name), context);
  }

  const renamed = await vm.runInContext(
    'renameFreshTerminalForRestore({ id: "s-new", name: "webClx_15", path: "webClx" }, "webClx", "webClx_7_待查看状态不准")',
    context,
  );
  assert.deepEqual(
    requestedNames,
    ["webClx_7_待查看状态不准", "webClx_15_待查看状态不准"],
    "restore rename should move the archived label onto the fresh terminal auto index",
  );
  assert.equal(renamed.name, "webClx_15_待查看状态不准");
}

{
  const requestedNames = [];
  const freshSession = { id: "s-new", name: "webClx_15", path: "webClx" };
  const context = vm.createContext({
    announceSessionMutation() {},
    encodeURIComponent,
    requestJson: async (_url, options) => {
      const { name } = JSON.parse(options.body);
      requestedNames.push(name);
      throw new Error(`修改会话名称失败: 名称 \`${name}\` 的自动编号已被占用，请使用其他编号。`);
    },
  });
  for (const name of [
    "restoredTerminalNameForAttempt",
    "isRestoredTerminalNameConflict",
    "renameFreshTerminalForRestore",
  ]) {
    vm.runInContext(functionSource(appJs, name), context);
  }

  const renamed = await vm.runInContext(
    'renameFreshTerminalForRestore({ id: "s-new", name: "webClx_15", path: "webClx" }, "webClx", "webClx_7_待查看状态不准")',
    context,
  );
  assert.deepEqual(
    requestedNames,
    ["webClx_7_待查看状态不准", "webClx_15_待查看状态不准"],
    "restore rename should stop after the fresh-index fallback also conflicts",
  );
  assert.equal(renamed.id, freshSession.id);
  assert.equal(renamed.name, freshSession.name, "restore should continue with the fresh terminal name");
  assert.equal(renamed.path, freshSession.path);
}

assert.match(
  appHtml,
  /<th>终端名字<\/th>/,
  "workspace history should label archived and active names as terminal names",
);

assert.match(
  appJs,
  /function createWorkspaceHistoryMoreButton\(item\)[\s\S]*label: "改名"[\s\S]*startWorkspaceHistoryTerminalRename\(item, button\)/,
  "workspace history rows should expose terminal rename from the more menu",
);

assert.match(
  appJs,
  /function startWorkspaceHistoryTerminalRename\(item, trigger\)[\s\S]*item\?\.terminal\?\.id[\s\S]*startSessionRename\(item\.terminal, trigger\)[\s\S]*return;/,
  "active workspace history rows should reuse the terminal rename action",
);

assert.match(
  appJs,
  /function startWorkspaceHistoryTerminalRename\(item, trigger\)[\s\S]*openTerminalRenameDialog\(sessionRenameDraftName\(currentName\), trigger\)/,
  "archived history rows should use the same rename draft behavior",
);

assert.match(
  appHtml,
  /<dialog id="terminal-rename-dialog" class="terminal-rename-dialog" aria-labelledby="terminal-rename-dialog-title">[\s\S]*id="session-rename-form" class="terminal-rename-dialog-form"[\s\S]*id="terminal-rename-dialog-title"[\s\S]*id="session-rename-input"[\s\S]*id="terminal-rename-dialog-status"[\s\S]*>保存名称<\/button>[\s\S]*id="session-rename-cancel"[\s\S]*>取消<\/button>[\s\S]*<\/dialog>/,
  "terminal management and workspace history should share one accessible rename dialog",
);

assert.doesNotMatch(
  appHtml,
  /id="session-rename-inline"|id="workspace-history-rename-inline"/,
  "rename editors should no longer occupy inline page space",
);

assert.match(
  appJs,
  /function startWorkspaceHistoryTerminalRename\(item, trigger\)[\s\S]*startSessionRename\(item\.terminal, trigger\)[\s\S]*workspaceHistoryRenamingItem = item[\s\S]*state\.renamingSessionId = ""[\s\S]*openTerminalRenameDialog\(sessionRenameDraftName\(currentName\), trigger\)/,
  "workspace history should reuse terminal rename for active rows and the shared dialog for archives",
);

assert.match(
  appJs,
  /function startSessionRename\(session, trigger\)[\s\S]*state\.renamingSessionId = session\.id[\s\S]*workspaceHistoryRenamingItem = null[\s\S]*openTerminalRenameDialog\(sessionRenameDraftName\(session\.name\), trigger\)/,
  "terminal management rename action should open the same shared dialog",
);

assert.match(
  appJs,
  /async function renameWorkspaceHistoryTerminal\(\)[\s\S]*closeTerminalRenameDialog\(\);[\s\S]*updateTableCardStatus\(workspaceHistoryStatusEl, `正在改名 \$\{currentName\}…`, "info"\);[\s\S]*await persistWorkspaceHistoryArchiveName[\s\S]*`修改终端名称失败：\$\{error\.message\}`/,
  "archived terminal rename should close the dialog before waiting for persistence",
);

{
  const context = vm.createContext({});
  for (const name of ["sessionRenameDraftName", "sessionRenameSavedName"]) {
    vm.runInContext(functionSource(appJs, name), context);
  }

  assert.equal(
    vm.runInContext('sessionRenameDraftName(" webClx_1 ")', context),
    "webClx_1_",
    "terminal management rename should append an underscore before editing",
  );
  assert.equal(
    vm.runInContext('sessionRenameSavedName(" webClx_1_\t")', context),
    "webClx_1",
    "rename submission should remove its trailing underscore",
  );
  assert.equal(
    vm.runInContext('sessionRenameSavedName("web_Clx__")', context),
    "web_Clx",
    "rename submission should preserve internal underscores and remove all trailing underscores",
  );
  assert.equal(
    vm.runInContext('sessionRenameSavedName("___")', context),
    "",
    "a rename made only of trailing underscores should remain invalid",
  );
}

assert.match(
  appJs,
  /async function renameSession\(\)[\s\S]*sessionRenameSavedName\(sessionRenameInputEl\.value\)[\s\S]*name: nextName/,
  "terminal management should submit the normalized dialog name",
);

assert.match(
  appJs,
  /async function renameSession\(\)[\s\S]*state\.activeTab === "workspace-history"[\s\S]*renderWorkspaceHistory\(\)/,
  "the shared terminal rename flow should refresh an open workspace history view",
);

assert.match(
  appJs,
  /async function renameWorkspaceHistoryTerminal\(\)[\s\S]*sessionRenameSavedName\(sessionRenameInputEl\.value\)[\s\S]*persistWorkspaceHistoryArchiveName\(item, nextName\)/,
  "workspace history should apply the same trailing underscore rule in the shared dialog",
);

assert.match(
  appJs,
  /function closeSessionRenameEditor\(\)[\s\S]*if \(workspaceHistoryRenamingItem\)[\s\S]*return;[\s\S]*closeTerminalRenameDialog\(\)/,
  "terminal list refresh should not close a workspace history rename dialog",
);

assert.match(
  appJs,
  /async function renameWorkspaceHistoryTerminal\(\)[\s\S]*workspaceHistoryRenamingItem[\s\S]*sessionRenameSavedName\(sessionRenameInputEl\.value\)[\s\S]*if \(!nextName\)[\s\S]*updateTerminalRenameDialogStatus\([\s\S]*persistWorkspaceHistoryArchiveName\(item, nextName\)[\s\S]*closeTerminalRenameDialog\(\)[\s\S]*refreshWorkspaceHistoryConversations\(\)/,
  "workspace history dialog should reject blanks inside the dialog, persist, close, and refresh",
);

assert.match(
  appJs,
  /sessionRenameDialogEl\.addEventListener\("cancel"[\s\S]*event\.preventDefault\(\)[\s\S]*closeTerminalRenameDialog\(\)[\s\S]*sessionRenameDialogEl\.addEventListener\("click"[\s\S]*event\.target === sessionRenameDialogEl[\s\S]*closeTerminalRenameDialog\(\)[\s\S]*sessionRenameCancelButton\.addEventListener\("click"[\s\S]*closeTerminalRenameDialog\(\)[\s\S]*sessionRenameFormEl\.addEventListener\("submit"[\s\S]*workspaceHistoryRenamingItem[\s\S]*renameWorkspaceHistoryTerminal\(\)[\s\S]*renameSession\(\)/,
  "shared rename dialog should support Escape, backdrop, cancel, and target-aware submit",
);

assert.match(
  appJs,
  /function openTerminalRenameDialog\(name, trigger\)[\s\S]*terminalRenameTriggerEl = trigger[\s\S]*sessionRenameDialogEl\.showModal\(\)[\s\S]*focusTextInputToEnd\(sessionRenameInputEl\)[\s\S]*function closeTerminalRenameDialog\(\)[\s\S]*const trigger = terminalRenameTriggerEl[\s\S]*sessionRenameDialogEl\.close\(\)[\s\S]*document\.querySelectorAll\("\[data-terminal-rename-key\]"\)[\s\S]*focusTarget\?\.focus\(\)/,
  "shared rename dialog should focus the input and restore focus to its trigger",
);

assert.doesNotMatch(
  appJs,
  /window\.prompt\("修改终端名称"/,
  "workspace history rename should not use a browser prompt",
);

assert.match(
  stylesBase,
  /\.terminal-rename-dialog \{[\s\S]*?width:\s*min\(520px, calc\(100vw - 40px\)\);[\s\S]*?padding:\s*0;[\s\S]*?border:\s*0;/,
  "shared rename dialog should use a compact modal shell that stays inside narrow visual viewports",
);

assert.match(
  stylesBase,
  /\.terminal-rename-dialog::backdrop \{[\s\S]*?background:/,
  "shared rename dialog should provide a visible modal backdrop",
);

assert.match(
  appJs,
  /actionCell\.appendChild\(createWorkspaceHistoryMoreButton\(item\)\)/,
  "workspace history should render a more-action menu next to the primary action",
);

assert.match(
  appJs,
  /function removeWorkspaceHistoryConversationLocally\(sessionId\)[\s\S]*workspaceHistoryRenamingItem\?\.sessionId[\s\S]*closeTerminalRenameDialog\(\)/,
  "deleting the history row being renamed should close the shared rename dialog",
);

assert.doesNotMatch(
  appJs,
  /closeWorkspaceHistoryTerminalRenameEditor/,
  "workspace history should not call the removed inline rename closer",
);

assert.match(
  stylesBase,
  /\.workspace-history-table \.session-action-cell \.mini-button \{[\s\S]*?padding-inline:\s*9px;/,
  "workspace history action buttons should be comfortably sized with a clear min-height",
);

assert.match(
  stylesBase,
  /\.workspace-history-table th:nth-child\(1\),[\s\S]*?width:\s*202px;/,
  "workspace history should allocate enough width for restore, fork, model selection, and more actions",
);

assert.match(
  stylesBase,
  /\.workspace-history-table \.session-action-cell \.mini-button \+ \.mini-button \{[\s\S]*?margin-inline-start:\s*8px;/,
  "workspace history actions should remain separated without overflowing the action column",
);

{
  const requests = [];
  const mutations = [];
  const context = vm.createContext({
    announceSessionMutation: (action, session) => mutations.push([action, session]),
    archiveResumeId: (archive) => archive?.resume_id || "",
    archiveWorkspacePath: (archive) => archive?.cwd || "",
    conversationWorkspacePathFromCwd: (cwd) => String(cwd || "").trim(),
    encodeURIComponent,
    requestJson: async (url, options) => {
      const body = JSON.parse(options.body);
      requests.push({ url, method: options.method, body });
      if (url.startsWith("/api/terminal/sessions/")) {
        return { id: "s-active", name: body.name, path: body.path };
      }
      return {
        id: body.resume_id,
        resume_id: body.resume_id,
        command: body.command,
        cwd: body.cwd,
        terminal_name: body.terminal_name,
        note: body.note || "",
        source: body.source,
      };
    },
    resumeCommandFromId: (resumeId) => `codex resume ${resumeId}`,
    sortTerminalArchives: (archives) => archives,
    state: {
      workspaceDir: "/home/codes",
      sessions: [{ id: "s-active", name: "旧名称", path: "/home/codes/webClx" }],
      terminalArchives: [],
    },
  });
  vm.runInContext("let workspaceHistoryTerminalArchivePersistQueue = Promise.resolve();", context);
  for (const name of [
    "splitPathParts",
    "normalizeRelativePath",
    "normalizeAbsolutePath",
    "relativePathBetweenAbsolute",
    "archiveWorkingPath",
    "queueWorkspaceHistoryTerminalArchiveWrite",
    "workspaceHistoryArchiveForItem",
    "workspaceHistoryArchiveCwd",
    "saveWorkspaceHistoryTerminalArchiveName",
    "persistWorkspaceHistoryArchiveName",
  ]) {
    vm.runInContext(functionSource(appJs, name), context);
  }

  await vm.runInContext(
    `persistWorkspaceHistoryArchiveName({
      type: "terminal",
      terminal: { id: "s-active", name: "旧名称", path: "/home/codes/webClx" },
      sessionId: "resume-active",
      cwd: "/home/codes/webClx"
    }, "新名称")`,
    context,
  );

  assert.deepEqual(requests, [], "active terminal rename should use the shared terminal rename flow");
  assert.deepEqual(mutations, []);

  requests.length = 0;
  await vm.runInContext(
    `persistWorkspaceHistoryArchiveName({
      type: "conversation",
      sessionId: "resume-history",
      cwd: "/home/codes/other"
    }, "历史名称")`,
    context,
  );
  assert.deepEqual(
    requests.map(({ url, method }) => [url, method]),
    [["/api/terminal/resume-archives", "POST"]],
    "inactive history rename should update only the resume archive",
  );
  assert.equal(requests[0].body.resume_id, "resume-history");
  assert.equal(requests[0].body.terminal_name, "历史名称");
  assert.equal(requests[0].body.cwd, "other");
}
