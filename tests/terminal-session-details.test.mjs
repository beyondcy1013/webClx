import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalJs = readEntryScriptBundle("terminal.html");
const terminalSessionActivityJs = readFileSync(
  new URL("../static/terminal-session-activity.js", import.meta.url),
  "utf8",
);
const terminalStylesCss = readFileSync(new URL("../static/styles-terminal.css", import.meta.url), "utf8");
const terminalRs = readFileSync(new URL("../src/terminal.rs", import.meta.url), "utf8");
const terminalDocsRs = readFileSync(new URL("../src/terminal/docs.rs", import.meta.url), "utf8");
const terminalManagerRs = readFileSync(new URL("../src/terminal/manager.rs", import.meta.url), "utf8");

assert.doesNotMatch(
  terminalHtml,
  /data-action="paste_clipboard"/,
  "terminal soft keyboard should not duplicate the function-menu Ctrl+V paste action",
);

assert.match(
  terminalHtml,
  /id="terminal-project-command-select"[\s\S]*<option value="open_agents_doc">文档<\/option>[\s\S]*data-project-action="open_agents_doc">文档<\/button>/,
  "terminal project commands should expose the document manager without consuming top-toolbar width",
);

assert.match(
  terminalHtml,
  /id="terminal-agents-doc-dialog"[\s\S]*<h2>AGENTS\.MD<\/h2>[\s\S]*id="terminal-agents-doc-path"[\s\S]*<\/div>[\s\S]*id="terminal-agents-doc-select"[\s\S]*id="terminal-agents-doc-status"[\s\S]*id="terminal-agents-doc-editor"[\s\S]*id="terminal-agents-doc-save"[\s\S]*保存[\s\S]*id="terminal-agents-doc-close"/,
  "terminal page should place the document selector on the right side of the dialog header",
);

assert.match(
  terminalJs,
  /action === "open_agents_doc"[\s\S]*openTerminalAgentsDocEditor\(\)/,
  "the project-command document entry should open the AGENTS.MD document manager",
);

assert.match(
  terminalJs,
  /const terminalAgentsDocSelectEl = document\.getElementById\("terminal-agents-doc-select"\);/,
  "terminal page should bind the document selector",
);

assert.match(
  terminalStylesCss,
  /\.terminal-agents-doc-select \{[\s\S]*?width:\s*min\(364px,\s*70%\);[\s\S]*?margin-left:\s*auto;/,
  "terminal document selector should be right-aligned and about 30% narrower than the previous 520px width",
);

assert.match(
  terminalJs,
  /async function openTerminalAgentsDocEditor\(\)[\s\S]*fetchTerminalAgentsDocList\(session\.id\)[\s\S]*renderTerminalAgentsDocOptions\(payload\.documents \|\| \[\], selectedPath\)[\s\S]*loadTerminalAgentsDoc\(session\.id, selectedPath\)/,
  "document button should load selectable current-directory and docs documents before opening the editor",
);

assert.match(
  terminalJs,
  /function openTerminalAgentsDocDialog\(\)[\s\S]*resetTerminalImeFocusContext\(\);[\s\S]*terminalAgentsDocDialogEl\.showModal\(\)/,
  "document button should reset the old terminal IME focus context before opening the modal editor",
);

assert.match(
  terminalJs,
  /function terminalImeResetInput\(\)[\s\S]*document\.createElement\("textarea"\)[\s\S]*input\.setAttribute\("data-terminal-ime-reset", "true"\)[\s\S]*input\.setAttribute\("inputmode", "text"\)[\s\S]*document\.body\.appendChild\(input\)[\s\S]*\}/,
  "desktop document focus recovery should use a neutral textarea to force Chrome to rebuild the IME input context",
);

assert.match(
  terminalJs,
  /function resetTerminalImeFocusContext\(\)[\s\S]*terminalHelperTextarea\(\)[\s\S]*helper\.blur\(\);[\s\S]*focusTerminalImeResetTarget\(\);[\s\S]*syncTerminalImePolicy\(\);[\s\S]*\}/,
  "document editor focus recovery should first break the old xterm helper textarea IME context",
);

assert.match(
  terminalJs,
  /function restoreTerminalFocusAfterDialogClose\(\)[\s\S]*resetTerminalImeFocusContext\(\);[\s\S]*window\.requestAnimationFrame\(\(\) => \{[\s\S]*window\.requestAnimationFrame\(\(\) => \{[\s\S]*focusTerminalForUserInput\(\);[\s\S]*window\.setTimeout\(\(\) => \{[\s\S]*focusTerminalForUserInput\(\);[\s\S]*\}, 80\);[\s\S]*window\.setTimeout\(\(\) => \{[\s\S]*focusTerminalForUserInput\(\);[\s\S]*\}, 180\);[\s\S]*\}/,
  "closing the document editor should rebuild the Windows IME context before restoring xterm focus",
);

assert.match(
  terminalJs,
  /terminalAgentsDocDialogEl\.addEventListener\("close", \(\) => \{[\s\S]*updateTerminalAgentsDocStatus\("", "info"\);[\s\S]*terminalAgentsDocSessionId = "";[\s\S]*restoreTerminalFocusAfterDialogClose\(\);[\s\S]*\}\)/,
  "document editor close event should restore terminal focus so Chinese IME can be selected again",
);

assert.match(
  terminalJs,
  /async function loadTerminalAgentsDoc\(sessionId, documentPath\)[\s\S]*const selectedPath = String\(documentPath \|\| "AGENTS\.MD"\)[\s\S]*new URLSearchParams\(\{[\s\S]*path: selectedPath,[\s\S]*show_hidden:[\s\S]*\/api\/terminal\/sessions\/\$\{encodeURIComponent\(sessionId\)\}\/agents-doc\?\$\{query\}[\s\S]*terminalAgentsDocEditorEl\.value = payload\.content \|\| ""/,
  "document selector should load the selected document path into the editor",
);

assert.match(
  terminalJs,
  /async function saveTerminalAgentsDoc\(\)[\s\S]*const documentPath = terminalAgentsDocPathValue\(\)[\s\S]*\/api\/terminal\/sessions\/\$\{encodeURIComponent\(sessionId\)\}\/agents-doc[\s\S]*method: "PUT"[\s\S]*path: documentPath,[\s\S]*content: terminalAgentsDocEditorEl\.value/,
  "document editor should save edited content back through the selected session document endpoint",
);

assert.match(
  terminalRs,
  /struct TerminalAgentsDocItem \{[\s\S]*path: String,[\s\S]*display_path: String,[\s\S]*label: String,[\s\S]*exists: bool,[\s\S]*\}[\s\S]*struct TerminalAgentsDocListResponse \{[\s\S]*documents: Vec<TerminalAgentsDocItem>,[\s\S]*\}/,
  "terminal document list endpoint should return selectable document metadata",
);

assert.match(
  terminalDocsRs,
  /pub async fn list_session_agents_docs\([\s\S]*AxumPath\(session_id\): AxumPath<String>[\s\S]*list_terminal_doc_candidates/,
  "terminal API should expose a session-scoped document list endpoint",
);

assert.match(
  terminalRs,
  /struct TerminalAgentsDocResponse \{[\s\S]*path: String,[\s\S]*display_path: String,[\s\S]*exists: bool,[\s\S]*content: String,[\s\S]*documents: Vec<TerminalAgentsDocItem>,[\s\S]*\}/,
  "terminal document read endpoint should return path metadata, content, and refreshed document options",
);

assert.match(
  terminalDocsRs,
  /pub async fn read_session_agents_doc\([\s\S]*AxumPath\(session_id\): AxumPath<String>[\s\S]*session_agents_doc_path/,
  "terminal API should expose a session-scoped AGENTS.MD read endpoint",
);

assert.match(
  terminalDocsRs,
  /pub async fn save_session_agents_doc\([\s\S]*AxumPath\(session_id\): AxumPath<String>[\s\S]*Json\(payload\): Json<TerminalAgentsDocSaveRequest>[\s\S]*tokio::fs::write/,
  "terminal API should expose a session-scoped AGENTS.MD save endpoint that can create the file",
);

assert.match(
  terminalHtml,
  /data-sequence="end"[\s\S]*<input[\s\S]*id="session-auto-continue-toggle"[\s\S]*type="checkbox"[\s\S]*>\s*继续/,
  "terminal soft keyboard should expose the auto-continue checkbox after End",
);

assert.doesNotMatch(
  terminalHtml.match(/<header class="topbar slim compact terminal-control-bar">[\s\S]*?<\/header>/)?.[0] || "",
  /id="session-detail-toggle|id="session-auto-continue-toggle/,
  "terminal session picker should not spend top-bar width on Details or auto-continue checkboxes",
);

assert.match(
  terminalJs,
  /const sessionDetailToggleEl = document\.getElementById\("session-detail-toggle"\);/,
  "terminal page should bind the Details checkbox",
);

assert.match(
  terminalJs,
  /const sessionAutoContinueToggleEl = document\.getElementById\("session-auto-continue-toggle"\);/,
  "terminal page should bind the auto-continue checkbox",
);

assert.match(
  terminalJs,
  /function sessionOptionLabel\(session\) \{[\s\S]*state\.showSessionDetails[\s\S]*codex_api_preset_name[\s\S]*detailParts\.push\(apiDetail\)[\s\S]*return `\$\{label\} \| \$\{detailParts\.join\(" \| "\)\}`;[\s\S]*\}/,
  "terminal session dropdown labels should append the startup API detail without the Codex_API tab prefix when Details is enabled",
);

assert.doesNotMatch(
  terminalJs.match(/function sessionOptionLabel\(session\) \{[\s\S]*?\n\}/)?.[0] || "",
  /Codex_API:/,
  "terminal session detail labels should not include the Codex_API tab name",
);

assert.match(
  terminalJs,
  /sessionDetailToggleEl\.addEventListener\("change"[\s\S]*state\.showSessionDetails = sessionDetailToggleEl\.checked[\s\S]*renderSessions\(\)/,
  "toggling Details should rerender the session dropdown immediately",
);

assert.match(
  terminalJs,
  /sessionAutoContinueToggleEl\.addEventListener\("change"[\s\S]*setAutoContinueOnError\(sessionAutoContinueToggleEl\.checked\)/,
  "toggling auto-continue should update the runtime setting immediately",
);

assert.match(
  terminalSessionActivityJs,
  /function sessionErrorContinueKey\(session\)[\s\S]*activity_error_signature[\s\S]*activityErrorSignature[\s\S]*`\$\{sessionId\}\\n\$\{keyword\}\\n\$\{signature\}`/,
  "auto-continue should use the backend error signature instead of output timestamps",
);

assert.match(
  terminalJs,
  /function sessionErrorContinueKey\(session\) \{[\s\S]*return sharedSessionErrorContinueKey\(session\);[\s\S]*\}/,
  "terminal page should use the shared backend-signature error key helper",
);

assert.match(
  terminalJs,
  /async function sendContinueToSession\(session\)[\s\S]*`\/api\/terminal\/sessions\/\$\{encodeURIComponent\(session\.id\)\}\/auto-continue`[\s\S]*method: "POST"/,
  "auto-continue should use the session-scoped backend endpoint with cooldown handling",
);

assert.match(
  terminalJs,
  /function autoContinueSessionLabel\(session\)[\s\S]*session\?\.name[\s\S]*`终端“\$\{name\}”`[\s\S]*session\?\.id[\s\S]*`终端 \$\{sessionId\}`/,
  "auto-continue status messages should format the target terminal name",
);

assert.match(
  terminalJs,
  /function maybeAutoContinueErroredSession\([\s\S]*if \(!state\.autoContinueOnError \|\| !session\?\.id\) \{[\s\S]*const existing = state\.autoContinueHandledErrors\.get\(session\.id\);[\s\S]*autoContinueRetryDue\(existing\)[\s\S]*sendContinueToSession\(session\)/,
  "auto-continue should send continue once per error-state lifecycle without requiring the session to be active",
);

assert.match(
  terminalJs,
  /function scheduleAutoContinueCooldownCleanup\(sessionId, sentAt, delayMs\)[\s\S]*state\.autoContinueScheduledTimers\.get\(sessionId\)[\s\S]*window\.clearTimeout[\s\S]*state\.autoContinueHandledErrors\.get\(sessionId\)[\s\S]*state\.autoContinueScheduledTimers\.delete\(sessionId\)/,
  "auto-continue cooldown cleanup should be independently timed and cleared per terminal session",
);

assert.match(
  terminalJs,
  /scheduleAutoContinueCooldownCleanup\(\s*session\.id,[\s\S]*normalizeTerminalAutoContinueIntervalSeconds\(state\.terminalAutoContinueIntervalSeconds\) \* 1000/,
  "each terminal should schedule its own auto-continue cooldown expiry",
);

assert.match(
  terminalJs,
  /const continueSent = Boolean\(session\?\.activity_error_continue_sent \|\| session\?\.activityErrorContinueSent\);[\s\S]*if \(continueSent\) \{[\s\S]*state\.autoContinueHandledErrors\.set\(session\.id, \{ key: errorKey, sentAt: Date\.now\(\) \}\);[\s\S]*return false;/,
  "auto-continue should not send another continue when the backend already sees continue after the matched error",
);

assert.match(
  terminalJs,
  /const inputQueued = Boolean\(session\?\.activity_error_input_queued \|\| session\?\.activityErrorInputQueued\);[\s\S]*if \(inputQueued\) \{[\s\S]*return false;/,
  "auto-continue should not send continue while Codex already has queued user input",
);

assert.match(
  terminalJs,
  /const sessionLabel = autoContinueSessionLabel\(session\);[\s\S]*`检测到\$\{sessionLabel\}错误“\$\{keyword\}”，已发送“继续”。`[\s\S]*`检测到\$\{sessionLabel\}错误，已发送“继续”。`/,
  "auto-continue success messages should include the terminal name",
);

assert.match(
  terminalJs,
  /`检测到\$\{autoContinueSessionLabel\(session\)\}限额重置时间 \$\{resetAt\}，已添加定时，将在重置后 1 分钟发送“继续”。`/,
  "scheduled auto-continue setup message should include the terminal name and timing",
);

assert.match(
  terminalSessionActivityJs,
  /function isSessionErrorState\(session\) \{[\s\S]*stateValue === "error"[\s\S]*stateValue === "retrying"/,
  "auto-continue should keep retrying sessions in the same handled-error lifecycle",
);

assert.match(
  terminalJs,
  /function isSessionErrorState\(session\) \{[\s\S]*return sharedIsSessionErrorState\(session\);[\s\S]*\}/,
  "terminal page should use the shared error-state helper",
);

assert.match(
  terminalJs,
  /state\.autoContinueHandledErrors\.delete\(session\.id\)/,
  "auto-continue should clear handled errors when a session leaves the error state",
);

assert.match(
  terminalJs,
  /function syncAutoContinueHandledErrors\(\)[\s\S]*if \(!isSessionErrorState\(session\)\) \{[\s\S]*state\.autoContinueHandledErrors\.delete\(session\.id\)/,
  "auto-continue should clear its handled marker after refreshed session data no longer reports an error",
);

assert.match(
  terminalJs,
  /function maybeAutoContinueErroredSession\([\s\S]*if \(!isSessionErrorState\(session\)\) \{[\s\S]*state\.autoContinueHandledErrors\.delete\(session\.id\);[\s\S]*return false;/,
  "auto-continue should clear its handled marker immediately when the active session no longer matches an error",
);

assert.doesNotMatch(
  terminalJs.match(/function maybeAutoContinueErroredSession\(session = activeSession\(\)\)[\s\S]*?\n\}/)?.[0] || "",
  /session\.id !== state\.activeSessionId|isTerminalConnected\(\)/,
  "auto-continue should not wait until the errored session is selected and connected",
);

assert.match(
  terminalRs,
  /struct TerminalInputRequest \{[\s\S]*data: String,[\s\S]*\}/,
  "terminal API should accept backend session input payloads",
);

assert.match(
  terminalRs,
  /pub async fn send_session_continue\([\s\S]*AxumPath\(session_id\): AxumPath<String>[\s\S]*send_session_continue\(&session_id\)/,
  "terminal API should expose a session-id continue endpoint for background auto-continue",
);

assert.match(
  terminalManagerRs,
  /pub fn send_session_continue\(&self, session_id: &str\) -> Result<\(\)>[\s\S]*send_terminal_command_with_enter\(session_id, TERMINAL_CONTINUE_COMMAND\)/,
  "backend continue endpoint should use the canonical continue sender",
);

assert.match(
  terminalRs,
  /struct StoredTerminalSession \{[\s\S]*codex_api_preset_name: String,[\s\S]*codex_api_base_url: String,[\s\S]*\}/,
  "stored terminal sessions should persist the launch Codex_API preset name and legacy Base URL",
);

assert.match(
  terminalRs,
  /pub struct TerminalSessionInfo \{[\s\S]*codex_api_preset_name: String,[\s\S]*codex_api_base_url: String,[\s\S]*\}/,
  "terminal session API responses should include the launch Codex_API preset name",
);

assert.match(
  terminalRs,
  /struct CurrentApiTerminalStartup \{[\s\S]*codex_api_preset_name: String,[\s\S]*codex_api_base_url: String,[\s\S]*\}/,
  "current Codex_API startup metadata should carry the active preset name",
);

assert.match(
  terminalRs,
  /codex_api_preset_name: preset\.name\.clone\(\)/,
  "terminal creation should snapshot the active Codex_API preset name",
);

assert.match(
  terminalManagerRs,
  /fn create_session_locked\([\s\S]*codex_api_preset_name: String,[\s\S]*codex_api_base_url: String,[\s\S]*StoredTerminalSession::new\([\s\S]*codex_api_preset_name[\s\S]*codex_api_base_url[\s\S]*\)/,
  "terminal manager should write the startup Codex_API preset name into newly created sessions",
);
