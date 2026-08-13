import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const appJs = readEntryScriptBundle("index.html");
const terminalSettingsJs = readFileSync(new URL("../static/terminal-settings.js", import.meta.url), "utf8");
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalStyles = readFileSync(new URL("../static/styles-terminal.css", import.meta.url), "utf8");
const terminalJs = readEntryScriptBundle("terminal.html");
const terminalFunctionDefaults =
  terminalSettingsJs.match(/const DEFAULT_TERMINAL_FUNCTION_COMMANDS = Object\.freeze\(\[[\s\S]*?\]\);/)?.[0] || "";
const terminalSlashDefaults =
  terminalSettingsJs.match(/const DEFAULT_TERMINAL_SLASH_COMMANDS = Object\.freeze\(\[[\s\S]*?\]\);/)?.[0] || "";
const terminalToolsMenuStart = terminalHtml.indexOf('id="terminal-tools-menu"');
const terminalToolsMenuEnd = terminalHtml.indexOf("</div>", terminalToolsMenuStart);
const terminalToolsMenu = terminalHtml.slice(terminalToolsMenuStart, terminalToolsMenuEnd);
const terminalSoftKeyboardStart = terminalHtml.indexOf('id="terminal-mobile-keys"');
const terminalSoftKeyboardEnd = terminalHtml.indexOf('id="terminal-fab"', terminalSoftKeyboardStart);
const terminalSoftKeyboard = terminalHtml.slice(terminalSoftKeyboardStart, terminalSoftKeyboardEnd);
const resumeCurrentAgentSessionBody =
  terminalJs.match(/async function resumeCurrentAgentSession\(\) \{([\s\S]*?)\n\}\n/)?.[1] || "";

assert.match(
  terminalSlashDefaults,
  /Object\.freeze\(\{\s*key:\s*"extract_resume"[\s\S]*?label:\s*"屏幕提取id并恢复"[\s\S]*?action:\s*"extract_resume"[\s\S]*?shortcut:\s*"Ctrl\+Alt\+R"/,
  "settings slash defaults should expose the restore action with Ctrl+Alt+R",
);

assert.match(
  terminalSlashDefaults,
  /Object\.freeze\(\{\s*key:\s*"fork"[\s\S]*?label:\s*"\/fork"[\s\S]*?action:\s*"send_slash_command"[\s\S]*?command:\s*"\/fork"/,
  "settings slash defaults should expose fork through the delayed slash-command path",
);

assert.match(
  terminalSlashDefaults,
  /Object\.freeze\(\{\s*key:\s*"extract_current_session"[\s\S]*?action:\s*"extract_current_session"[\s\S]*?shortcut:\s*"Ctrl\+Alt\+S"/,
  "extract-session should use Ctrl+Alt+S",
);

assert.doesNotMatch(
  terminalSlashDefaults,
  /key:\s*"webui"|action:\s*"open_project_url"/,
  "快捷 should not duplicate 项目管理's project URL action",
);

assert.match(
  terminalHtml,
  /<option value="open_project_url">项目 URL<\/option>/,
  "项目管理 should retain the project URL action",
);

assert.match(
  terminalSlashDefaults,
  /Object\.freeze\(\{\s*key:\s*"copy_id_and_ask"[\s\S]*?label:\s*"复制id并提问"[\s\S]*?action:\s*"copy_id_and_ask"[\s\S]*?command:\s*""/,
  "the quick menu should expose the copy-ID-and-ask action",
);

assert.doesNotMatch(
  terminalSlashDefaults,
  /key:\s*"quota"|action:\s*"open_quota_dialog"/,
  "套餐 should only remain in 全能, not 快捷",
);

assert.match(
  terminalJs,
  /if \(command\.action === "open_project_url"\) \{[\s\S]*?openProjectUrl\(\);[\s\S]*?return true;[\s\S]*?\}/,
  "the WebUI quick command should reuse the project URL action",
);

assert.match(
  terminalJs,
  /async function copyCurrentSessionIdAndAsk\(\)[\s\S]*?const sourceSessionId = state\.activeSessionId;[\s\S]*?detectAgentResumeIdComplete\(sourceSessionId, sourceContext\)[\s\S]*?const prompt = `调用codex对话数据库skill读取session id为 \$\{detected\.resumeId\}并回答我的问题 `;[\s\S]*?copyTextToClipboard\(prompt\)/,
  "copy-ID-and-ask should freeze the source terminal and copy the complete prompt like copy-terminal-name",
);

const copyIdAndAskBody =
  terminalJs.match(/async function copyCurrentSessionIdAndAsk\(\) \{([\s\S]*?)\n\}\n/)?.[1] || "";
assert.doesNotMatch(
  copyIdAndAskBody,
  /sendTerminalInputToSession|sendTerminalInput|sendTerminalAutoTypedInput/,
  "copy-ID-and-ask should not write the prompt into the terminal",
);

assert.match(
  terminalJs,
  /if \(command\.action === "copy_id_and_ask"\) \{[\s\S]*?copyCurrentSessionIdAndAsk\(\);[\s\S]*?return true;[\s\S]*?\}/,
  "the copy-ID-and-ask quick command should dispatch through the function command runner",
);

assert.match(
  terminalFunctionDefaults,
  /Object\.freeze\(\{\s*key:\s*"toggle_soft_keyboard"[\s\S]*?action:\s*"toggle_soft_keyboard"[\s\S]*?shortcut:\s*"Ctrl\+K"/,
  "soft-keyboard toggle should use Ctrl+K",
);

assert.match(
  terminalSlashDefaults,
  /Object\.freeze\(\{\s*key:\s*"copy_terminal_name"[\s\S]*?action:\s*"copy_terminal_name"[\s\S]*?shortcut:\s*"Ctrl\+Alt\+T"/,
  "copy-terminal-name should live in the quick menu and use Ctrl+Alt+T",
);

assert.doesNotMatch(
  terminalFunctionDefaults,
  /key:\s*"copy_terminal_name"|action:\s*"copy_terminal_name"/,
  "copy-terminal-name should move out of the function menu",
);

assert.match(
  terminalSettingsJs,
  /function isMovedSlashCommand\(command\)[\s\S]*?command\?\.key === "copy_terminal_name"[\s\S]*?command\?\.action === "copy_terminal_name"/,
  "saved function-menu settings should migrate copy-terminal-name into the quick menu",
);

assert.doesNotMatch(
  terminalFunctionDefaults,
  /key:\s*"deploy_project"|action:\s*"deploy_project"/,
  "project deploy should not be owned by the general-purpose command defaults",
);

assert.match(
  terminalHtml,
  /<option value="deploy_project" data-shortcut="Ctrl\+B">本项目部署脚本<\/option>/,
  "project commands should own deploy and its Ctrl+B shortcut",
);

assert.match(
  terminalJs,
  /DEFAULT_TERMINAL_SLASH_COMMANDS/,
  "terminal page should load shared slash command defaults",
);

assert.match(
  appJs,
  /DEFAULT_TERMINAL_SLASH_COMMANDS/,
  "settings page should load shared slash command defaults",
);

assert.match(
  terminalSlashDefaults,
  /Object\.freeze\(\{\s*key:\s*"resume_current_session"[\s\S]*?label:\s*"恢复会话"[\s\S]*?action:\s*"resume_current_agent_session"[\s\S]*?command:\s*""/,
  "settings slash defaults should expose a current-session resume action",
);

assert.match(
  terminalSettingsJs,
  /function ensureBuiltInTerminalSlashCommands\(commands\)[\s\S]*?DEFAULT_TERMINAL_SLASH_COMMANDS[\s\S]*?normalized\.push\(command\)/,
  "terminal page should add built-in slash commands back into existing customized slash menus",
);

assert.match(
  terminalJs,
  /ensureBuiltInTerminalSlashCommands\(DEFAULT_TERMINAL_SLASH_COMMANDS\)/,
  "terminal page should initialize slash commands through the shared built-in merge helper",
);

assert.doesNotMatch(
  terminalFunctionDefaults,
  /Object\.freeze\(\{\s*key:\s*"rust_backup"[\s\S]*?label:\s*"!rust-backup"[\s\S]*?action:\s*"insert_text"[\s\S]*?command:\s*"!rust-backup "/,
  "rust-backup should move out of the function menu and into project commands",
);

assert.doesNotMatch(
  terminalFunctionDefaults,
  /Object\.freeze\(\{\s*key:\s*"deploy_project"[\s\S]*?label:\s*"部署"[\s\S]*?action:\s*"deploy_project"/,
  "deploy_project should not be duplicated in the function menu when project commands exposes it",
);

assert.doesNotMatch(
  terminalFunctionDefaults,
  /Object\.freeze\(\{\s*key:\s*"downloads"[\s\S]*?label:\s*"下载中心"[\s\S]*?action:\s*"open_artifact_downloads"/,
  "downloads should move out of the function menu and into project commands",
);

assert.match(
  terminalFunctionDefaults,
  /Object\.freeze\(\{\s*key:\s*"enter"[\s\S]*?label:\s*"Enter"[\s\S]*?action:\s*"send_sequence"[\s\S]*?command:\s*"enter"/,
  "terminal page should expose an Enter command under the function menu",
);

assert.match(
  terminalFunctionDefaults,
  /Object\.freeze\(\{\s*key:\s*"current_resume_id"[\s\S]*?label:\s*"session ID"[\s\S]*?action:\s*"copy_current_resume_id"[\s\S]*?command:\s*""/,
  "settings defaults should expose a current Codex session ID command under the function menu",
);

assert.match(
  terminalFunctionDefaults,
  /Object\.freeze\(\{\s*key:\s*"copy_window"[\s\S]*?label:\s*"新窗口复制"[\s\S]*?action:\s*"copy_terminal_view_in_new_window"[\s\S]*?command:\s*""/,
  "settings defaults should expose a new-window copy command under the function menu",
);

assert.match(
  terminalJs,
  /let terminalTouchSelectionDisabled = false;/,
  "terminal touch text selection should be allowed by default so long-press copy is available immediately",
);

assert.match(
  terminalJs,
  /function openTerminalVisibleTextCopyWindow\(\)[\s\S]*?readTerminalVisibleText\(\)[\s\S]*?window\.open\("", "_blank"\)[\s\S]*?copyWindow\.opener = null/,
  "new-window copy should read the visible terminal text and open a manual-copy window",
);

assert.match(
  terminalJs,
  /if \(command\.action === "copy_terminal_view_in_new_window"\) \{[\s\S]*?openTerminalVisibleTextCopyWindow\(\);[\s\S]*?return true;[\s\S]*?\}/,
  "the new-window copy command should dispatch through the function command runner",
);

assert.match(
  terminalSlashDefaults,
  /Object\.freeze\(\{\s*key:\s*"continue"[\s\S]*?label:\s*"继续"[\s\S]*?action:\s*"send_text"[\s\S]*?command:\s*"继续"/,
  "terminal page should expose a continue command that sends Enter under the slash menu",
);

assert.doesNotMatch(
  terminalHtml,
  /<button[^>]+data-text="继续"[^>]*>\s*继续\s*<\/button>/,
  "continue should move into the function menu instead of remaining a fixed soft key",
);

assert.doesNotMatch(
  terminalSoftKeyboard,
  /data-action="paste_clipboard"|>\s*粘贴\s*<\/button>/,
  "soft keyboard should use the function-menu Ctrl+V command instead of a duplicate paste button",
);

assert.match(
  terminalJs,
  /if \(command\.action === "send_sequence"\) \{[\s\S]*?if \(command\.command === "ctrl_v"\) \{[\s\S]*?pasteFromClipboard\(\);[\s\S]*?return true;/,
  "the function-menu Ctrl+V command should reuse the shared clipboard paste function",
);

assert.match(
  terminalFunctionDefaults,
  /Object\.freeze\(\{\s*key:\s*"enter"[\s\S]*?label:\s*"Enter"[\s\S]*?action:\s*"send_sequence"[\s\S]*?command:\s*"enter"/,
  "settings defaults should expose an Enter command under the function menu",
);

assert.match(
  terminalSlashDefaults,
  /Object\.freeze\(\{\s*key:\s*"continue"[\s\S]*?label:\s*"继续"[\s\S]*?action:\s*"send_text"[\s\S]*?command:\s*"继续"/,
  "settings slash defaults should expose a continue command that sends Enter under the slash menu",
);

assert.doesNotMatch(
  terminalFunctionDefaults,
  /key:\s*"extract_resume"|key:\s*"continue"/,
  "restore and continue should move out of the function menu",
);

assert.doesNotMatch(
  terminalHtml,
  /<select[^>]+id="terminal-slash-command-select"/,
  "the mobile quick menu should not use Android's native radio-style select dialog",
);

assert.match(
  terminalHtml,
  /<button[^>]+id="terminal-slash-command-button"[\s\S]*?aria-haspopup="menu"[\s\S]*?>\s*快捷\s*<\/button>[\s\S]*?id="terminal-slash-command-menu"/,
  "the mobile quick trigger should open an ordinary button menu",
);

assert.match(
  terminalJs,
  /function renderTerminalSlashCommandMenu\(\)[\s\S]*?document\.createElement\("button"\)[\s\S]*?button\.setAttribute\("role", "menuitem"\)/,
  "quick commands should render as ordinary menu buttons",
);

assert.match(
  terminalStyles,
  /\.terminal-slash-command-menu > button[\s\S]*?min-height:\s*28px[\s\S]*?font:\s*650 var\(--terminal-key-font-size\)\/1/,
  "quick menu rows should be compact and use the soft-key font size",
);

assert.match(
  terminalJs,
  /function scrollTerminalSlashCommandMenuToBottom\(\)[\s\S]*?terminalSlashCommandMenuEl\.scrollTop\s*=\s*terminalSlashCommandMenuEl\.scrollHeight/,
  "opening the quick menu should provide a dedicated bottom-scroll helper",
);

assert.match(
  terminalJs,
  /if \(expanded\) \{[\s\S]*?scrollTerminalSlashCommandMenuToBottom\(\);[\s\S]*?requestAnimationFrame\([\s\S]*?scrollTerminalSlashCommandMenuToBottom\(\)/,
  "the quick menu should default to its last buttons after layout settles on every open",
);

assert.match(
  terminalJs,
  /function positionTerminalFunctionCommandMenu\(\)[\s\S]*?availableHeight\s*=\s*Math\.max\(0, triggerRect\.top - 14\)[\s\S]*?menuRect\.height[\s\S]*?triggerRect\.top - menuRect\.height - 6/,
  "the function menu should stay above the trigger and fit within the available height",
);

assert.match(
  terminalHtml,
  /<div class="terminal-mobile-row">[\s\S]*id="terminal-escape-command-button"[\s\S]*<button[^>]+data-sequence="tab"[^>]*>Tab<\/button>\s*<button[^>]+data-sequence="arrow_up"[^>]*>↑<\/button>/,
  "mobile up arrow should appear after Tab on the first soft-key row",
);

assert.doesNotMatch(
  terminalHtml,
  /<select[^>]+id="terminal-escape-command-select"/,
  "Esc/^C should not use Android's native radio-style select dialog",
);

assert.match(
  terminalHtml,
  /<button[^>]+id="terminal-escape-command-button"[\s\S]*?aria-haspopup="menu"[\s\S]*?>\s*Esc\/\^C\s*<\/button>[\s\S]*?id="terminal-escape-command-menu"/,
  "Esc/^C should open an ordinary compact menu",
);

assert.match(
  terminalToolsMenu,
  /id="terminal-codex-full-access-toggle"[\s\S]*id="terminal-quick-command-buttons"[\s\S]*id="terminal-copy-all"[\s\S]*id="session-detail-toggle"[\s\S]*>\s*api[\s\S]*id="session-agent-toggle"[\s\S]*>\s*智能体[\s\S]*id="session-auto-continue-toggle"/,
  "terminal tools should order full access, commands, copy-all, and session toggles",
);

assert.doesNotMatch(
  terminalSoftKeyboard,
  /id="terminal-quick-command-buttons"|id="terminal-copy-all"|id="session-detail-toggle"|id="session-agent-toggle"|id="session-auto-continue-toggle"/,
  "soft keyboard should keep terminal-tools controls out of its button rows",
);

assert.match(
  terminalJs,
  /state\.terminalQuickCommands = normalizeTerminalQuickCommands\([\s\S]*?rawTerminalQuickCommands,[\s\S]*?Array\.isArray\(settings\.terminal_quick_commands\) \? null : DEFAULT_TERMINAL_QUICK_COMMANDS,[\s\S]*?\{\s*includeCommandLine:\s*true\s*\},[\s\S]*?\);/,
  "terminal page should preserve full quick command lines after loading settings",
);

assert.match(
  terminalJs,
  /function terminalQuickCommandLine\(command\)[\s\S]*?normalizeTerminalQuickText\(command\?\.commandLine, 1000\)[\s\S]*?\|\| normalizeTerminalQuickText\(command\?\.command, 1000\)/,
  "terminal quick command sending should fall back to the stored command field",
);

assert.match(
  terminalHtml,
  /data-text="1"[\s\S]*data-text="2"[\s\S]*data-sequence="arrow_left"[^>]*>←<\/button>\s*<button[^>]+data-sequence="arrow_down"[^>]*>↓<\/button>\s*<button[^>]+data-sequence="arrow_right"[^>]*>→<\/button>[\s\S]*id="terminal-number-button"/,
  "mobile arrow keys should sit together on the number row before the numeric overflow menu",
);

assert.doesNotMatch(
  terminalHtml,
  /<select[^>]+id="terminal-number-select"/,
  "the number control should not use a native picker that dismisses the system keyboard",
);

assert.match(
  terminalHtml,
  /<button[^>]+id="terminal-number-button"[\s\S]*?aria-haspopup="menu"[\s\S]*?>\s*数\s*<\/button>[\s\S]*?id="terminal-number-menu"[\s\S]*?data-digit="3"[\s\S]*?data-digit="4"[\s\S]*?data-digit="5"[\s\S]*?data-digit="6"/,
  "the number control should use an ordinary compact menu",
);

assert.doesNotMatch(
  terminalSoftKeyboard,
  /id="terminal-tool-root-tools"|data-terminal-tool-root="tools"/,
  "the weapon root should no longer consume a standalone soft-key slot",
);

assert.match(
  terminalHtml,
  /id="terminal-tools-menu"[\s\S]*id="terminal-tool-menu"[\s\S]*id="terminal-tool-menu-body"/,
  "weapon entries should render inside the terminal-tools menu",
);

assert.match(
  terminalSoftKeyboard,
  /data-sequence="end"[\s\S]*data-sequence="page_up"/,
  "mobile Page Up should follow End without an inline auto-continue toggle",
);

assert.match(
  terminalHtml,
  /data-sequence="arrow_left"[\s\S]*data-sequence="arrow_right"/,
  "mobile left arrow should appear before right arrow",
);

assert.doesNotMatch(
  terminalHtml,
  /id="terminal-ime-toggle"|>\s*输入法\s*<\/button>/,
  "fixed input-method soft key should be removed",
);

assert.match(
  terminalSettingsJs,
  /function normalizeTerminalFunctionCommandLine\(value, maxLength = 1000\)[\s\S]*?trimStart\(\)[\s\S]*?return withoutLeading\.slice\(0, maxLength\);/,
  "function command normalization should preserve trailing command spaces",
);

assert.match(
  terminalJs,
  /function insertTextCommand\(command\)[\s\S]*?normalizeTerminalFunctionCommandLine\(command, 1000\)[\s\S]*?sendTerminalInput\(commandLine\);[\s\S]*?focusTerminalSoon\(\);[\s\S]*?return true;/,
  "insert_text function commands should insert text without sending Enter",
);

assert.match(
  terminalJs,
  /if \(command\.action === "insert_text"\) \{[\s\S]*?insertTextCommand\(command\.command\);[\s\S]*?return true;[\s\S]*?\}/,
  "insert_text commands should use the insert-only path that does not send Enter",
);

assert.match(
  terminalJs,
  /function sendContinueCommand\(options = \{\}\)[\s\S]*?sendTextCommand\("继续", options\)/,
  "the continue command should send text through the Enter-submitting text command path",
);

assert.match(
  terminalJs,
  /function sendSlashCommand\(command, \{ enterDelayMs = 0, sessionId = "" \} = \{\}\)[\s\S]*?const effectiveEnterDelayMs =[\s\S]*?commandLine\.startsWith\("\/"\)[\s\S]*?MOBILE_SLASH_COMMAND_ENTER_DELAY_MS[\s\S]*?sendInput\(commandLine\)[\s\S]*?await waitForMobileKeyDelay\(effectiveEnterDelayMs\);[\s\S]*?sendInput\(MOBILE_KEY_SEQUENCES\.enter\)/,
  "slash-command sending should target the requested session and wait before the first Enter",
);

assert.match(
  terminalJs,
  /if \(command\.key === "continue"\) \{[\s\S]*?sendContinueCommand\(\{ enterDelayMs: options\.enterDelayMs \}\);[\s\S]*?return true;[\s\S]*?\}/,
  "legacy customized continue commands should still submit with Enter",
);

assert.match(
  terminalJs,
  /async function copyCurrentAgentResumeId\(\)[\s\S]*?detectCurrentAgentResumeId\(\)[\s\S]*?showCopyResumeOverlay\(command \|\| resumeCommandFromId\(resumeId\)\)[\s\S]*?focusTerminalAfterTransientControl\(\);/,
  "current Codex resume ID copying should reuse active-session detection and show a copyable resume command",
);

assert.match(
  terminalJs,
  /if \(command\.action === "copy_current_resume_id"\) \{[\s\S]*?copyCurrentAgentResumeId\(\);[\s\S]*?return true;[\s\S]*?\}/,
  "the current Codex resume ID command should dispatch through the function command runner",
);

assert.match(
  terminalJs,
  /async function detectAgentResumeIdComplete\([\s\S]*?extractLatestResumeInfo\(initialBufferText\)[\s\S]*?probeTerminalStatusResumeId\(context, initialBufferText\)[\s\S]*?detectAgentResumeIdForSession\(targetSessionId, \{ complete: true \}\)/,
  "advanced Session extraction should use screen, /status, then the complete backend fallback",
);

assert.match(
  terminalJs,
  /async function detectCurrentAgentResumeId\(\) \{[\s\S]*?detectAgentResumeIdComplete\(state\.activeSessionId, activeTerminalContext\)/,
  "current-session callers should share the complete Session extraction chain",
);

assert.match(
  terminalJs,
  /runTerminalSlashCommandByKey\("status", \{ sessionId: targetSessionId \}\)/,
  "the /status fallback should target the frozen terminal instead of whichever terminal is active later",
);

assert.match(
  resumeCurrentAgentSessionBody,
  /extractCurrentAgentSessionId\(\)[\s\S]*?const command = detected\.command \|\| resumeCommandFromId\(detected\.resumeId, detected\.program\);[\s\S]*?await sendTerminalAutoTypedInput\(command\)/,
  "current-session resume should run through the prepared auto-typed path so the current terminal environment is applied",
);

assert.doesNotMatch(
  resumeCurrentAgentSessionBody,
  /sendTerminalInput\(command\)|MOBILE_KEY_SEQUENCES\.enter/,
  "current-session resume must not bypass the backend preparation path with direct terminal input",
);

assert.match(
  terminalJs,
  /if \(command\.action === "resume_current_agent_session" \|\| command\.action === "resume_current_codex_session"\) \{[\s\S]*?resumeCurrentAgentSession\(\);[\s\S]*?return true;[\s\S]*?\}/,
  "the current-session resume slash command should dispatch through the function command runner",
);

assert.match(
  appJs,
  /const \[key = "", label = "", action = "", rawCommand = "", shortcut = ""\]/,
  "settings parser should read function command shortcuts from a fifth pipe-delimited column",
);

assert.ok(
  appJs.includes('const command = String(rawCommand)') &&
    appJs.includes(".trimStart()") &&
    appJs.includes('.replace(/[ \\t]$/, "");'),
  "settings parser should preserve intentional trailing command spaces before the pipe separator",
);

assert.match(
  appJs,
  /\[command\.key, command\.label, command\.action, command\.command, command\.shortcut\]\.join\(" \| "\)/,
  "settings formatter should persist function command shortcuts in the fifth column",
);

assert.match(
  terminalJs,
  /function handleTerminalFunctionShortcut\(event\)[\s\S]*?findTerminalFunctionCommandByShortcut\(event\)[\s\S]*?runTerminalFunctionCommand\(command/,
  "terminal page should dispatch configured function shortcuts through the same function command runner",
);

assert.match(
  terminalJs,
  /if \(command\.action === "toggle_soft_keyboard"\) \{[\s\S]*?toggleTerminalSoftKeyboard\(\);[\s\S]*?return true;[\s\S]*?\}/,
  "the Ctrl+K command should dispatch through the existing soft-keyboard toggle",
);

assert.match(
  terminalSettingsJs,
  /BUILTIN_TERMINAL_SHORTCUT_KEYS[\s\S]*?"toggle_soft_keyboard"[\s\S]*?"extract_current_session"[\s\S]*?"copy_terminal_name"[\s\S]*?command\.shortcut = builtin\.shortcut/,
  "requested built-in shortcuts should replace stale saved shortcut values",
);

assert.match(
  terminalJs,
  /function findTerminalFunctionCommandByShortcut\(event\)[\s\S]*?terminalProjectCommandSelectEl\?\.options[\s\S]*?option\.dataset\.shortcut[\s\S]*?action: projectOption\.value/,
  "keyboard shortcuts should dispatch project-owned actions from the project command menu",
);

assert.match(
  appJs,
  /state\.terminalFunctionCommands = ensureBuiltInTerminalFunctionCommands\(/,
  "settings UI should show the migrated built-in function shortcuts",
);

assert.doesNotMatch(
  terminalHtml,
  /<button[^>]+data-action="extract_resume"[^>]*>\s*恢复\s*<\/button>/,
  "bottom restore key should move into the function menu instead of remaining a fixed soft key",
);

assert.match(
  terminalJs,
  /if \(button\.dataset\.action === "show_system_keyboard" \|\| button\.dataset\.action === "disable_system_keyboard"\) \{[\s\S]*?runTerminalKeyboardCommand\(button\.dataset\.action\);[\s\S]*?return;/,
  "mobile keys should dispatch system keyboard restore/disable actions through the IME command path",
);

assert.doesNotMatch(
  terminalJs,
  /event\.key\?\.toLowerCase\(\) !== "r"[\s\S]*?!event\.ctrlKey[\s\S]*?event\.shiftKey/,
  "restore shortcut should no longer be hard-coded as Ctrl+R without Shift",
);
