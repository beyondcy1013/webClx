import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalJs = readEntryScriptBundle("terminal.html");

// 1. 终端粘贴弹窗具备上传图片按钮与多图选择控件
assert.match(
  terminalHtml,
  /id="terminal-paste-dialog"[\s\S]*id="terminal-paste-upload-image-button"[\s\S]*>上传图片<\/button>[\s\S]*id="terminal-paste-image-input"[\s\S]*type="file"[\s\S]*accept="image\/\*"[\s\S]*multiple/,
  "terminal paste dialog should have an upload image button and multiple image file input",
);

// 2. 全能菜单与终端工具菜单均支持打开粘贴对话框与上传图片
assert.match(
  terminalHtml,
  /id="terminal-function-command-menu"[\s\S]*data-action="open_terminal_paste_dialog"/,
  "terminal function command menu should have an action to open the paste dialog",
);

assert.match(
  terminalHtml,
  /id="terminal-tools-menu"[\s\S]*id="terminal-tools-paste-dialog-button"[\s\S]*data-action="open_terminal_paste_dialog"/,
  "terminal tools menu should have an action to open the paste dialog",
);

assert.match(
  terminalHtml,
  /id="terminal-tools-menu"[\s\S]*id="terminal-tools-upload-image-button"[\s\S]*data-action="upload_terminal_image"/,
  "terminal tools menu should have an upload image button",
);

// 2b. 独立的粘贴图片对话框与菜单入口
assert.match(
  terminalHtml,
  /id="terminal-image-paste-dialog"[\s\S]*id="terminal-image-paste-select-button"[\s\S]*id="terminal-image-paste-assets"[\s\S]*id="terminal-image-paste-text"[\s\S]*id="terminal-image-paste-input"/,
  "terminal should have a dedicated image paste dialog",
);

assert.match(
  terminalHtml,
  /id="terminal-tools-menu"[\s\S]*id="terminal-tools-paste-image-button"[\s\S]*data-action="open_terminal_image_paste_dialog"/,
  "terminal tools menu should have an action to open the dedicated image paste dialog",
);

assert.match(
  terminalHtml,
  /id="terminal-function-command-menu"[\s\S]*id="terminal-paste-image-button"[\s\S]*data-action="open_terminal_image_paste_dialog"/,
  "terminal function command menu should have an action to open the dedicated image paste dialog",
);

// 3. 点击全能菜单或终端工具菜单中的打开粘贴对话框动作或 Ctrl+V 能够触发打开粘贴弹窗
assert.match(
  terminalJs,
  /button\.dataset\.action === "open_terminal_paste_dialog"[\s\S]*closeTerminalFunctionCommandMenu\(\);[\s\S]*openTerminalPasteDialogFromClipboard\(\)/,
  "clicking open_terminal_paste_dialog in function menu should open paste dialog",
);

assert.match(
  terminalJs,
  /if \(button\.dataset\.action === "open_terminal_paste_dialog"\) \{\s*openTerminalPasteDialogFromClipboard\(\);\s*return;\s*\}/,
  "triggerMobileKey should handle open_terminal_paste_dialog",
);

assert.match(
  terminalJs,
  /if \(button\.dataset\.action === "open_terminal_image_paste_dialog"\) \{\s*openTerminalImagePasteDialog\(\);\s*return;\s*\}/,
  "triggerMobileKey should handle open_terminal_image_paste_dialog",
);

assert.match(
  terminalJs,
  /command\.action === "open_paste_dialog"[\s\S]*openTerminalPasteDialogFromClipboard\(\)/,
  "open_paste_dialog action should call openTerminalPasteDialogFromClipboard",
);

// 4. 全能命令里的 Ctrl+V sequence 也复用了 pasteFromClipboard / 粘贴处理
assert.match(
  terminalJs,
  /if \(command\.action === "send_sequence"\) \{[\s\S]*?if \(command\.command === "ctrl_v"\) \{[\s\S]*?pasteFromClipboard\(\);[\s\S]*?return true;/,
  "send_sequence with ctrl_v should call pasteFromClipboard",
);

// 5. 粘贴弹窗中的上传图片按钮在点击时触发文件选择，且选择图片后调用 applyTerminalPasteParts
assert.match(
  terminalJs,
  /terminalPasteUploadImageButtonEl\.addEventListener\("click"[\s\S]*terminalPasteImageInputEl\.click\(\)/,
  "terminalPasteUploadImageButtonEl click should trigger terminalPasteImageInputEl.click",
);

assert.match(
  terminalJs,
  /terminalPasteImageInputEl\.addEventListener\("change"[\s\S]*applyTerminalPasteParts\(\[\{\s*type:\s*"images",\s*blobs:\s*files\s*\}\],\s*\{\s*openDialog:\s*false\s*\}\)/,
  "terminalPasteImageInputEl change should upload selected images into the paste dialog",
);

// 6. 粘贴弹窗右上角提供关闭按钮，底部取消按钮已移除，且定时发送按钮位于粘贴并发送之后
assert.match(
  terminalHtml,
  /id="terminal-paste-dialog"[\s\S]*class="panel-head wide"[\s\S]*id="terminal-paste-close"[\s\S]*aria-label="关闭"/,
  "terminal paste dialog should have a close button in panel-head",
);

assert.ok(
  !/class="toolbar dialog-actions terminal-paste-actions"[\s\S]*id="terminal-paste-cancel"/.test(terminalHtml),
  "terminal paste actions toolbar should not contain cancel button",
);

assert.match(
  terminalHtml,
  /id="terminal-paste-submit-enter"[\s\S]*>直接发送<\/button>[\s\S]*id="terminal-paste-schedule-toggle"[\s\S]*>定时发送<\/button>/,
  "schedule toggle button should be placed after paste-and-send button",
);

assert.match(
  terminalJs,
  /terminalPasteCloseButton\.addEventListener\("click"[\s\S]*closeTerminalPasteDialog\(\)|\[.*terminalPasteCloseButton.*\]\.forEach/,
  "terminalPasteCloseButton should close terminal paste dialog",
);

console.log("PASS: terminal-paste-ctrl-v-upload contract verified");
