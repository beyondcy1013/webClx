import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const appJs = readEntryScriptBundle("index.html");
const terminalJs = readEntryScriptBundle("terminal.html");
const terminalSessionsJs = readFileSync(new URL("../static/terminal-sessions.js", import.meta.url), "utf8");
const terminalSessionRenderJs = readFileSync(new URL("../static/terminal-session-render.js", import.meta.url), "utf8");
const workspaceHistoryTooltipJs = readFileSync(
  new URL("../static/app-workspace-history-tooltip.js", import.meta.url),
  "utf8",
);
const stylesCss = [
  "../static/styles.css",
  "../static/styles-base.css",
  "../static/styles-settings.css",
  "../static/styles-auth.css",
  "../static/styles-terminal.css",
  "../static/styles-responsive.css",
]
  .map((path) => readFileSync(new URL(path, import.meta.url), "utf8"))
  .join("\n");

assert.match(
  indexHtml,
  /data-tab="terminal-archives"[\s\S]*?>\s*归档列表\s*<\/button>/,
  "main page should expose a terminal archive tab beside active sessions",
);

assert.match(
  indexHtml,
  /id="terminal-archives-view"[\s\S]*id="terminal-archives-list"/,
  "main page should render Codex resume archives in a table panel",
);

for (const statusId of ["workspace-history-status", "terminal-archives-status"]) {
  assert.match(
    indexHtml,
    new RegExp(`class="table-card-title-row"(?:(?!class="toolbar)[\\s\\S])*id="${statusId}"`),
    `${statusId} should render beside its table title instead of in the controls row`,
  );
}

assert.match(
  stylesCss,
  /\.table-card-title-row\s*\{[\s\S]*?flex-wrap:\s*nowrap;/,
  "table-title status rows should remain on one line at every viewport width",
);
assert.match(
  stylesCss,
  /\.table-card-status\s*\{[\s\S]*?text-overflow:\s*ellipsis;[\s\S]*?white-space:\s*nowrap;/,
  "long table-title status messages should truncate instead of creating a second row",
);
assert.match(
  appJs,
  /function updateTableCardStatus\([\s\S]*?const delay = tone === "warn" \? 6000 : 2800;[\s\S]*?element\.hidden = true;/,
  "table-title status messages should auto-dismiss after loading completes",
);

assert.match(
  indexHtml,
  /id="workspace-history-path-select"[\s\S]*<th>操作<\/th>[\s\S]*<th>终端<\/th>[\s\S]*<th>终端名字<\/th>[\s\S]*<th>session ID<\/th>[\s\S]*<th>大小<\/th>[\s\S]*<th>对话历史<\/th>[\s\S]*<th>最近更新<\/th>/,
  "workspace history should switch work directories with a dropdown and render conversations plus active terminal names as a table",
);

assert.doesNotMatch(
  indexHtml,
  /<table class="entry-table workspace-history-table">(?:(?!<\/table>)[\s\S])*<th>工作目录<\/th>/,
  "workspace history should not duplicate the selected work directory as a table column",
);

assert.doesNotMatch(
  appJs,
  /workspace-history-path-cell|workspace-history-path-link|row\.append\(terminalCell, activeTerminalNameCell, sessionCell, sizeCell, pathCell/,
  "workspace history rows should not render a work-directory cell",
);

assert.match(
  appJs,
  /cell\.colSpan = 7/,
  "workspace history empty rows should span the seven visible columns",
);

assert.match(
  appJs,
  /activeTerminalNameBySessionId[\s\S]*?archiveTerminalNameBySessionId[\s\S]*?activeTerminalName: session\.name \|\| session\.id \|\| ""[\s\S]*?activeTerminalName: activeTerminalNameBySessionId\.get\(resumeId\) \|\| archiveTerminalNameBySessionId\.get\(resumeId\) \|\| ""[\s\S]*?workspace-history-active-terminal-cell/,
  "workspace history rows should show archived terminal names after the active session is closed",
);

assert.match(
  appJs,
  /hash === "#terminal-archives"[\s\S]*return "terminal-archives"/,
  "terminal archive tab should be addressable by hash",
);

assert.match(
  appJs,
  /async function loadTerminalArchives\(\)[\s\S]*requestJson\("\/api\/terminal\/resume-archives"\)/,
  "terminal archive table should load from the existing resume archive API",
);

assert.match(
  appJs,
  /function workspaceHistoryCoreRequestUrls\(\)[\s\S]*sessions: "\/api\/terminal\/sessions\?all=true"[\s\S]*sessions: `\/api\/terminal\/sessions\?path=\$\{encodeURIComponent\(relativePath\)\}`[\s\S]*conversations: `\/api\/terminal\/codex-conversations\?cwd=\$\{encodeURIComponent\(path\)\}`/,
  "workspace history should load only the selected directory unless all-workspace search is explicit",
);

assert.match(
  appJs,
  /workspaceHistoryPathSelectEl\.addEventListener\("change",[\s\S]*refreshWorkspaceHistoryConversations\(\)[\s\S]*workspaceHistorySearchAllEl\.addEventListener\("change",[\s\S]*refreshWorkspaceHistoryConversations\(\)/,
  "changing the selected directory or explicit all-workspace scope should reload that scope",
);

assert.match(
  appJs,
  /hydrateWorkspaceHistoryTerminalSessionIds[\s\S]*agent-session/,
  "workspace history should reuse active terminal Codex session detection",
);

assert.match(
  appJs,
  /function workspaceHistoryInputHistoryText\(entries[\s\S]*?map\(\(entry\) => String\(entry\?\.text \|\| ""\)\.trim\(\)\)[\s\S]*?join\("\\n"\)/,
  "workspace history input-history text should render each command on its own line without truncation",
);

assert.match(
  appJs,
  /\/api\/terminal\/sessions\/\$\{encodeURIComponent\(session\.id\)\}\/input-history[\s\S]*?session\.input_history_text = workspaceHistoryInputHistoryText\(payload\.entries \|\| \[\]\)/,
  "workspace history should hydrate active terminal input-history summaries",
);

assert.match(
  appJs,
  /title: session\.input_history_text \|\| session\.title \|\| "活动终端"/,
  "workspace history active terminal conversation column should show input-history text before the terminal title",
);

assert.match(
  appJs,
  /function conversationHistoryTitle\(conversation\)[\s\S]*conversation\?\.title/,
  "workspace history should read conversation summaries returned by the Codex jsonl API",
);

assert.match(
  appJs,
  /function archiveHistoryNote\(archive, resumeId\)[\s\S]*isDefaultArchiveHistoryNote\(note, resumeId\)[\s\S]*return ""/,
  "workspace history should treat default archive notes as empty instead of displaying duplicated session ids",
);

assert.match(
  appJs,
  /title: archiveHistoryNote\(archive, resumeId\) \|\| conversationTitle \|\| "无对话摘要"/,
  "workspace history archive rows should prefer real conversation summaries over generated archive ids",
);

assert.match(
  appJs,
  /title: conversationHistoryTitle\(conversation\) \|\| "无对话摘要"/,
  "workspace history jsonl-only rows should not fall back to rollout filenames",
);

assert.match(
  stylesCss,
  /\.workspace-history-table td\.workspace-history-title-cell \{[\s\S]*?white-space:\s*nowrap;[\s\S]*?text-overflow:\s*ellipsis;/,
  "workspace history conversation history cells should show one line by default",
);

assert.match(
  stylesCss,
  /#workspace-history-view \{[\s\S]*?min-width:\s*0;[\s\S]*?max-width:\s*100%;[\s\S]*?grid-template-columns:\s*minmax\(0,\s*1fr\);[\s\S]*?overflow:\s*hidden;/,
  "workspace history panel should not let its dense table widen the mobile page",
);

assert.match(
  stylesCss,
  /\.workspace-history-toolbar \{[\s\S]*?width:\s*100%;[\s\S]*?min-width:\s*0;/,
  "workspace history toolbar should stay constrained to the panel width",
);

assert.match(
  stylesCss,
  /\.workspace-history-toolbar > \* \{[\s\S]*?flex-shrink:\s*0;/,
  "workspace history mobile controls should scroll without compressing their labels",
);

assert.match(
  stylesCss,
  /\.workspace-history-toolbar \.button \{[\s\S]*?white-space:\s*nowrap;/,
  "workspace history mobile action labels should remain on one line",
);

assert.match(
  stylesCss,
  /\.workspace-history-toolbar > \.directory-session-select \{[\s\S]*?flex:\s*0 0 clamp\(190px,\s*62vw,\s*300px\);/,
  "workspace history mobile directory picker should remain readable",
);

assert.match(
  stylesCss,
  /#workspace-history-view > \.table-wrap \{[\s\S]*?max-width:\s*100%;/,
  "workspace history table should scroll inside its wrapper instead of widening the page",
);

assert.match(
  workspaceHistoryTooltipJs,
  /function attachWorkspaceHistoryTooltip\(target, \{ title, dir \}\)[\s\S]*mouseenter[\s\S]*mouseleave[\s\S]*focus[\s\S]*blur/,
  "workspace history details should open from hover or keyboard focus without expanding table rows",
);

assert.match(
  appJs,
  /titleCell\.tabIndex = 0;[\s\S]*?attachWorkspaceHistoryTooltip\(titleCell, \{[\s\S]*?title: item\.title \|\| ""[\s\S]*?dir:/,
  "workspace history conversation cells should attach their full title and directory to the shared tooltip",
);

assert.match(
  terminalHtml,
  /id="terminal-input-history-button"[\s\S]*aria-label="对话史"[\s\S]*title="对话史"[\s\S]*<span class="terminal-fab-item-label">对话史<\/span>[\s\S]*<\/button>/,
  "terminal input-history floating button should be labeled conversation history",
);

assert.match(
  terminalHtml,
  /id="terminal-input-history-dialog"[\s\S]*<p class="section-label">对话历史<\/p>[\s\S]*<h2>本终端对话历史<\/h2>/,
  "terminal input-history dialog should be titled conversation history",
);

assert.match(
  terminalJs,
  /function openTerminalInputHistoryDialog\(\)[\s\S]*resetTerminalImeFocusContext\(\);[\s\S]*terminalInputHistoryDialogEl\.showModal\(\)/,
  "opening the conversation history dialog should reset the old terminal IME focus context before focus moves inside the modal",
);

assert.match(
  terminalJs,
  /terminalInputHistoryDialogEl\.addEventListener\("close", \(\) => \{[\s\S]*?restoreTerminalFocusAfterDialogClose\(\);[\s\S]*?\}\)/,
  "closing the conversation history dialog should rebuild the terminal IME context before restoring focus",
);

assert.match(
  appJs,
  /buildTerminalUrl\(archiveWorkingPath\(archive\), "", \{ fresh: true, runCommand: command \}\)/,
  "running an archive should open a fresh terminal in the archived working directory with the resume command queued",
);

assert.match(
  appJs,
  /const workingPath = resolveWorkspaceHistoryPath\(archiveWorkingPath\(item\.archive\)\)[\s\S]*?openFreshTerminalRunLink\(event, workingPath, command, \{[\s\S]*?beforeNavigate: \(\) => touchTerminalArchive\(archiveIdentity\(item\.archive\)\)/,
  "workspace history archive restore should create a concrete terminal session before navigating",
);

assert.match(
  appJs,
  /const workingPath = resolveWorkspaceHistoryPath\(selectedPath\);[\s\S]*?const command = resumeCommandFromId\(item\.sessionId\);[\s\S]*?openFreshTerminalRunLink\(event, workingPath, command, \{[\s\S]*?terminalName: item\.activeTerminalName/,
  "workspace history conversation restore should create a concrete terminal session before navigating",
);

assert.match(
  appJs,
  /function archiveWorkingPath\(archive\)[\s\S]*?rawPath\.startsWith\("\/"\)[\s\S]*?relativePathBetweenAbsolute\(state\.workspaceDir \|\| "\/", rawPath\)[\s\S]*?normalizeRelativePath\(rawPath\)/,
  "archive working paths should convert absolute saved cwd values to terminal-relative paths",
);

assert.match(
  indexHtml,
  /<th>工作目录<\/th>/,
  "terminal archive table should show the working directory captured with each archive",
);

assert.match(
  stylesCss,
  /\.terminal-archives-table th,\s*\.terminal-archives-table td \{[\s\S]*?white-space:\s*nowrap;[\s\S]*?max-height:\s*28px;[\s\S]*?text-overflow:\s*ellipsis;/,
  "terminal archive rows should stay compact and single-line",
);

assert.match(
  stylesCss,
  /@media \(max-width: 720px\) \{[\s\S]*?\.terminal-archives-table \{[\s\S]*?width:\s*max-content;[\s\S]*?min-width:\s*980px;[\s\S]*?\.terminal-archives-table th,\s*\.terminal-archives-table td \{[\s\S]*?white-space:\s*nowrap;[\s\S]*?max-height:\s*28px;/,
  "mobile archive table should use horizontal scrolling instead of wrapping cells",
);

assert.match(
  appJs,
  /const resumeCell = createTextCell\(resumeId \|\| "—", "mono-text terminal-archive-resume-cell"\)/,
  "terminal archive Resume ID column should render the full ID instead of a shortened label",
);

assert.match(
  appJs,
  /const commandCell = document\.createElement\("td"\);[\s\S]*?className = "terminal-archive-command-cell"[\s\S]*?const copyCommandButton = createActionButton\("复制",[\s\S]*?copyTerminalArchiveCommand\(command, copyCommandButton\)/,
  "terminal archive command column should be a compact copy button",
);

assert.match(
  appJs,
  /function copyTextWithHiddenTextarea\(text\)[\s\S]*?document\.createElement\("textarea"\)[\s\S]*?left:-9999px[\s\S]*?document\.execCommand\("copy"\)/,
  "terminal archive command copy fallback should use a hidden textarea instead of opening a window",
);

assert.match(
  appJs,
  /navigator\.clipboard\?\.writeText[\s\S]*?copyTextWithHiddenTextarea\(commandText\)[\s\S]*?copyTextWithHiddenTextarea\(commandText\)/,
  "terminal archive command copy should fall back to direct hidden-textarea copy when clipboard copy is unavailable",
);

assert.doesNotMatch(
  appJs,
  /openTerminalArchiveCommandCopyWindow|window\.open\("", "_blank"\)[\s\S]*?复制命令/,
  "terminal archive command copy should not open a new browser window",
);

assert.match(
  stylesCss,
  /\.terminal-archives-table th:nth-child\(4\),\s*\.terminal-archives-table td:nth-child\(4\) \{[\s\S]*?width:\s*300px;/,
  "terminal archive Resume ID column should be wide enough for full UUIDs",
);

assert.match(
  stylesCss,
  /\.terminal-archives-table th:nth-child\(6\),\s*\.terminal-archives-table td:nth-child\(6\) \{[\s\S]*?width:\s*52px;/,
  "terminal archive command column should stay narrow because it only contains a copy button",
);

assert.match(
  terminalJs,
  /body: JSON\.stringify\(\{[\s\S]*cwd: state\.currentPath,[\s\S]*resume_id: resumeId,[\s\S]*command: command \|\| resumeCommandFromId\(resumeId\),[\s\S]*terminal_name: activeSession\(\)\?\.name \|\| "",[\s\S]*\}\)/,
  "saving an archive should include the current terminal working directory, resume command, and terminal name",
);

assert.match(
  appJs,
  /\/api\/terminal\/resume-archives\/\$\{encodeURIComponent\(archiveId\)\}[\s\S]*method: "DELETE"/,
  "archive table should delete rows through the existing archive delete API",
);

assert.match(
  appJs,
  /function createWorkspaceHistoryMoreButton\(item\)[\s\S]*label: "删除"[\s\S]*danger: true[\s\S]*disabled: item\.type === "terminal"/,
  "workspace history should disable delete in the more menu for active Codex conversations",
);

assert.match(
  appJs,
  /async function deleteWorkspaceHistoryConversation\(item, button\)[\s\S]*window\.confirm\([\s\S]*\/api\/terminal\/codex-conversations\/\$\{encodeURIComponent\(item\.sessionId\)\}[\s\S]*method: "DELETE"[\s\S]*removeWorkspaceHistoryConversationLocally\(item\.sessionId\)/,
  "workspace history should confirm permanent deletion, call the API, and remove only that session locally",
);

assert.doesNotMatch(
  appJs,
  /async function deleteWorkspaceHistoryConversation\(item, button\)(?:(?!\n\}).)*refreshWorkspaceHistoryConversations\(\)/s,
  "workspace history deletion should not reload every session and conversation",
);

assert.match(
  appJs,
  /async function deleteWorkspaceHistoryConversation\(item, button\)[\s\S]*removeWorkspaceHistoryConversationLocally\(item\.sessionId\)[\s\S]*showToast\("Codex 会话已删除。", "ok", 2800\)/,
  "workspace history deletion should show a short success toast",
);

assert.match(
  appJs,
  /async function deleteWorkspaceHistoryConversation\(item, button\)[\s\S]*catch \(error\) \{[\s\S]*showToast\(`删除 Codex 会话失败：\$\{error\.message\}`, "warn", 6000\)/,
  "workspace history deletion should show a longer failure toast",
);

assert.match(
  appJs,
  /actionCell\.appendChild\(createWorkspaceHistoryMoreButton\(item\)\)/,
  "workspace history should add Codex actions through the more menu",
);

assert.match(
  terminalHtml,
  /id="idle-session"[\s\S]*>闲置<\/button>[\s\S]*id="archive-resume"[\s\S]*>归档<\/button>/,
  "terminal toolbar should place the idle button before archive",
);

assert.match(
  terminalHtml,
  /id="idle-session-select"[\s\S]*闲置终端/,
  "terminal toolbar should use a native select for idle terminals",
);

assert.doesNotMatch(
  terminalHtml,
  /id="restore-idle-session"/,
  "standalone 恢复 button should be removed (idle select auto-switches)",
);

assert.doesNotMatch(
  terminalHtml,
  /id="resume-archive-switcher"/,
  "terminal toolbar should no longer render the archive dropdown",
);

assert.doesNotMatch(
  terminalJs,
  /localStorage\.setItem\([^)]*idle-terminal-sessions/,
  "idle terminal selection should not be stored in browser-local localStorage",
);

assert.match(
  terminalSessionsJs,
  /async function idleCurrentSession\(\)[\s\S]*\/api\/terminal\/sessions\/\$\{encodeURIComponent\(current\.id\)\}\/idle[\s\S]*method: "PUT"/,
  "idle button should persist idle state through the server",
);

assert.doesNotMatch(
  terminalSessionsJs,
  /if \(isIdleSession\(current\.id\)\) \{[\s\S]*await restoreIdleSession\(current\.id\)/,
  "idle button should not double as a restore toggle",
);

assert.match(
  terminalSessionsJs,
  /async function restoreIdleSession\(sessionId\)[\s\S]*\/api\/terminal\/sessions\/\$\{encodeURIComponent\(sessionId\)\}\/restore[\s\S]*method: "PUT"[\s\S]*selectSession\(session\.id, \{ connect: true, pushHistory: true \}\)/,
  "restoreIdleSession should round-trip the server and connect the session",
);

assert.match(
  terminalJs,
  /idleSessionSelectEl\.addEventListener\("change", \(\) => \{[\s\S]*restoreIdleSession\(sessionId\)/,
  "idle select change handler should auto-restore the chosen idle terminal",
);

assert.doesNotMatch(
  terminalSessionsJs,
  /function previewIdleSession/,
  "previewIdleSession should be removed now that idle select auto-switches",
);

assert.doesNotMatch(
  terminalSessionRenderJs,
  /restoreIdleSessionButton/,
  "renderIdleSessions should no longer reference the removed 恢复 button",
);

assert.match(
  terminalSessionRenderJs,
  /idleSessionButton\.disabled = !current \|\| isIdleSession\(current\.id\)/,
  "闲置 button should disable when current session is idle",
);

assert.match(
  terminalJs,
  /function isIdleSession\(sessionId\)[\s\S]*return Boolean\(session\?\.idle\);/,
  "idle rendering should derive from server session data",
);

assert.doesNotMatch(
  terminalJs,
  /function selectSession\(sessionId,[\s\S]*?\/api\/terminal\/sessions\/\$\{encodeURIComponent\(session\.id\)\}\/restore/,
  "passive session selection, including URL refresh, should not restore idle sessions",
);

assert.match(
  terminalJs,
  /if \(targetSession && isIdleSession\(targetSession\.id\)\) \{[\s\S]*targetSession = visibleSessions\(\)\[0\] \|\| null;[\s\S]*\}/,
  "refreshing a URL that points at an idle session should keep it idle and pick an active session instead",
);
