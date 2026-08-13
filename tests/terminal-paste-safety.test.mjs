import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalJs = readEntryScriptBundle("terminal.html");

assert.match(
  terminalJs,
  /const bracketMultilinePaste = multiline && options\.forceBracketedPaste !== false;[\s\S]*?if \(bracketMultilinePaste\) \{[\s\S]*?sendTerminalInput\(wrapBracketedTerminalPaste\(prepared\)\);[\s\S]*?\} else if \(typeof term\.paste === "function"\)/,
  "all multiline terminal text paste should use bracketed paste before falling back to xterm's ordinary paste path",
);

assert.match(
  terminalJs,
  /function sendPastedText\(text[\s\S]*?updateStatus\(bracketMultilinePaste \? "已安全粘贴多行内容到终端输入区。" : "已粘贴到终端输入区。", "ok"\)/,
  "multiline paste should report the safe bracketed-paste path",
);

assert.match(
  terminalHtml,
  /id="terminal-paste-schedule"[\s\S]*id="terminal-paste-schedule-confirm"[\s\S]*确认定时[\s\S]*id="terminal-paste-schedule-toggle"[\s\S]*定时发送[\s\S]*id="terminal-paste-submit-enter"/,
  "terminal paste dialog should expose server-side scheduled send controls without replacing paste-and-send",
);

assert.match(
  terminalHtml,
  /id="terminal-paste-submit"[\s\S]*type="button"[\s\S]*>粘贴到终端<\/button>[\s\S]*id="terminal-paste-submit-enter"[\s\S]*type="submit"[\s\S]*>粘贴并发送<\/button>/,
  "terminal paste dialog should make Enter default to paste-and-send instead of paste-only or cancel",
);

assert.match(
  terminalJs,
  /terminalPasteSubmitButton\.addEventListener\("click"[\s\S]*?submitTerminalPasteDialog\(\)/,
  "clicking the paste-only button should still paste without submitting Enter",
);

assert.match(
  terminalJs,
  /terminalPasteFormEl\.addEventListener\("submit"[\s\S]*?const schedulePanelOpen = terminalPasteScheduleEl && !terminalPasteScheduleEl\.hidden;[\s\S]*?if \(schedulePanelOpen\) \{[\s\S]*?confirmTerminalPasteSchedule\(\)[\s\S]*?submitTerminalPasteDialogAndSend\(\)/,
  "terminal paste dialog form submit should send by default, while the open schedule panel should confirm the timed send",
);

assert.match(
  terminalJs,
  /async function confirmTerminalPasteSchedule\(\)[\s\S]*requestJson\("\/api\/terminal\/scheduled-inputs",[\s\S]*session_id: state\.activeSessionId,[\s\S]*due_at: resolved\.dueAtMs,[\s\S]*send_enter: true,[\s\S]*applyTerminalPasteScheduledTaskList\(payload\?\.tasks \|\| \[\]\)/,
  "terminal paste scheduling should create a persisted server-side scheduled input for the active session",
);

assert.match(
  terminalHtml,
  /terminal\.js\?v=20260810a/,
  "terminal page should bump the terminal.js cache key after terminal input visibility changes",
);

assert.match(
  terminalHtml,
  /terminal-paste\.js\?v=20260810a/,
  "terminal page should bump the terminal paste helper cache key after scheduled-task chip changes",
);

assert.match(
  terminalHtml,
  /terminal-layout-connection\.js\?v=20260805a/,
  "terminal page should bump the layout/connection helper cache key after session-switch chip refresh changes",
);

assert.match(
  terminalJs,
  /function terminalScheduledTaskActiveSessionIds\(\)[\s\S]*state\.activeSessionId[\s\S]*sessionSelectEl\?\.value[\s\S]*activeSession\(\)\?\.id[\s\S]*function terminalScheduledTaskMatchesActiveSession\(task\)[\s\S]*terminalScheduledTaskActiveSessionIds\(\)\.has\(sessionId\)/,
  "terminal scheduled task chip should match the active terminal from state and the session picker",
);

assert.match(
  terminalJs,
  /TERMINAL_AUTO_CONTINUE_TASK_NOTIFY_STORAGE_KEY[\s\S]*function terminalAutoContinueTaskNotifyKey\(task\)[\s\S]*sessionId[\s\S]*taskKind[\s\S]*stableTime[\s\S]*function filterUnnotifiedTerminalAutoContinueTasks\(tasks/,
  "terminal auto-continue task toasts should use a stable session/type/due-time notification key",
);

assert.match(
  terminalJs,
  /const rawNewlyDetectedAutoContinueTasks = applyTerminalAutoContinueScheduledTaskList[\s\S]*newlyDetectedAutoContinueTasks = filterUnnotifiedTerminalAutoContinueTasks\([\s\S]*rawNewlyDetectedAutoContinueTasks/,
  "terminal auto-continue refresh should suppress duplicate task-detected toasts across signature changes",
);

assert.match(
  terminalJs,
  /function terminalScheduledTaskCounts\(\)[\s\S]*terminalPasteScheduledTasks\.values\(\)[\s\S]*terminalAutoContinueScheduledTasks\.values\(\)[\s\S]*terminalScheduledTaskMatchesActiveSession[\s\S]*current[\s\S]*total/,
  "terminal scheduled task chip should aggregate paste schedules and auto-continue schedules for the active terminal",
);

assert.match(
  terminalJs,
  /function selectSession\(sessionId,[\s\S]*state\.activeSessionId = session\.id;[\s\S]*renderSessions\(\);[\s\S]*tickTerminalPasteScheduledCountdown\(\);[\s\S]*restoreSessionPageScrollIfActive/,
  "terminal scheduled task chip should refresh immediately after switching sessions",
);

assert.match(
  terminalJs,
  /function terminalScheduledTaskChipText\(\)[\s\S]*return `定时 \$\{counts\.current\}\/\$\{counts\.total\}`;/,
  "terminal scheduled task chip should show current-terminal count and global total",
);

assert.doesNotMatch(
  terminalJs,
  /terminalPasteScheduleChipCancelEl/,
  "the merged schedule item should open task controls instead of restoring the removed blanket-cancel button",
);

assert.match(
  terminalJs,
  /function refreshTerminalInputVisibilityAfterPaste\(\)[\s\S]*?const followUpDelays = \[80, 180, 360, 720, 1200, 1800\];[\s\S]*?scrollTerminalToBottom\(\);[\s\S]*?saveTerminalScrollPositionForSession\(sessionId\);[\s\S]*?syncTerminalCursorCorrection\(\);[\s\S]*?window\.requestAnimationFrame\(refresh\);[\s\S]*?followUpDelays\.forEach/,
  "terminal paste should keep refreshing bottom visibility while delayed Codex echo arrives",
);

assert.match(
  terminalJs,
  /function refreshTerminalInputVisibilityAfterUserInput\(\)[\s\S]*?const followUpDelays = \[60, 140, 280, 560\];[\s\S]*?scheduleTerminalRenderRefresh\(\);[\s\S]*?scrollTerminalToBottom\(\);[\s\S]*?saveTerminalScrollPositionForSession\(sessionId\);[\s\S]*?syncTerminalCursorCorrection\(\);[\s\S]*?window\.requestAnimationFrame\(refresh\);[\s\S]*?followUpDelays\.forEach/,
  "ordinary terminal typing should keep the input and cursor visible while delayed mobile viewport changes settle",
);

assert.match(
  terminalJs,
  /sendTerminalInput\(wrapBracketedTerminalPaste\(prepared\)\);[\s\S]*?refreshTerminalInputVisibilityAfterPaste\(\);[\s\S]*?updateStatus\(bracketMultilinePaste/,
  "terminal paste should refresh visibility before reporting success",
);

assert.match(
  terminalJs,
  /function refreshTerminalInputVisibilityAfterPageResume\(\)[\s\S]*?!sessionId \|\| terminalSessionInitializing\(\) \|\| shouldDeferSessionListRender\(\)[\s\S]*?captureTerminalScrollSnapshotForSession\(sessionId\)[\s\S]*?shouldDeferSessionListRender\(\)[\s\S]*?return;[\s\S]*?preservePageScrollDuringLayout\([\s\S]*?refreshTerminalViewportLayout\(\{ requireConnected: true \}\)[\s\S]*?restoreTerminalScrollSnapshot\(scrollSnapshot\);[\s\S]*?syncTerminalCursorCorrection\(\);/,
  "returning to the terminal page should preserve scroll and avoid auto-focus while a native select or mobile IME may own focus",
);
assert.doesNotMatch(
  terminalJs.slice(
    terminalJs.indexOf("function refreshTerminalInputVisibilityAfterPageResume()"),
    terminalJs.indexOf("function terminalShouldStickToBottomForOutput", terminalJs.indexOf("function refreshTerminalInputVisibilityAfterPageResume()")),
  ),
  /focusTerminalForUserInput\(/,
  "page resume must not re-focus the terminal and reopen the system keyboard",
);

assert.match(
  terminalJs,
  /function refreshSessionsAfterPageResume\(\) \{[\s\S]*?refreshTerminalInputVisibilityAfterPageResume\(\);[\s\S]*?loadSessions\(/,
  "page resume should refresh terminal input visibility before doing the ordinary session-list refresh",
);

assert.match(
  terminalJs,
  /updateStatus\(imageCount \? `已上传 \$\{imageCount\} 张图片，可粘贴到终端。` : "已读取剪贴板文本。", "ok"\)/,
  "manual rich paste should explicitly confirm image upload success",
);

assert.match(
  terminalJs,
  /updateStatus\(`已上传 \$\{prepared\.imageCount\} 张图片，并粘贴到终端输入区。`, "ok"\)/,
  "direct rich paste should explicitly confirm image upload success",
);

assert.match(
  terminalJs,
  /navigator\.clipboard\?\.read[\s\S]*?\} catch \(_error\) \{[\s\S]*?Continue with plain text\.[\s\S]*?\} finally \{[\s\S]*?setClipboardPasteBusy\(false\);[\s\S]*?\}[\s\S]*?navigator\.clipboard\?\.readText/,
  "rich clipboard read failures should fall through to the plain text clipboard path",
);

assert.match(
  terminalJs,
  /\} catch \(_error\) \{\s*openTerminalPasteDialog\(\);\s*updateStatus\("浏览器无法直接读取剪贴板，请在弹窗里粘贴内容。", "warn"\);/,
  "plain text clipboard failures should open the manual paste dialog instead of surfacing host clipboard errors",
);
