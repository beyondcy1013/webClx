import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { readEntryScriptBundle } from "./static-entry-assets.mjs";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const terminalJs = readEntryScriptBundle("terminal.html");
const androidMain = readFileSync(
  new URL("../android/app/src/main/java/com/webclx/app/MainActivity.java", import.meta.url),
  "utf8",
);

assert.match(
  terminalHtml,
  /id="terminal-function-command-menu"[\s\S]*id="terminal-image-upload-button"[\s\S]*data-action="upload_terminal_image"[\s\S]*>上传图片<\/button>[\s\S]*id="terminal-image-upload-input"[\s\S]*type="file"[\s\S]*accept="image\/png,image\/jpeg,image\/gif,image\/webp,image\/bmp"[\s\S]*multiple/,
  "the all-purpose command menu should expose a multi-image picker",
);

assert.match(
  terminalJs,
  /button\.dataset\.action === "upload_terminal_image"[\s\S]*closeTerminalFunctionCommandMenu\(\);[\s\S]*openTerminalImageUploadPicker\(\)/,
  "the upload command should close the menu and open the shared file picker",
);

assert.match(
  terminalJs,
  /async function handleTerminalImageUploadSelection\(\)[\s\S]*terminalImageUploadInputEl\?\.files[\s\S]*pasteTerminalPartsDirectly\([\s\S]*type: "images", blobs: files[\s\S]*progressMessage: "正在上传所选图片…"/,
  "selected images should reuse the terminal paste upload path",
);

assert.match(
  terminalJs,
  /terminalImageUploadInputEl\.addEventListener\("change", handleTerminalImageUploadSelection\)/,
  "the picker should upload after a client selection",
);

assert.match(
  androidMain,
  /onShowFileChooser\([\s\S]*startActivityForResult\(params\.createIntent\(\), FILE_CHOOSER_REQUEST\)[\s\S]*WebChromeClient\.FileChooserParams\.parseResult/,
  "the Android WebView should route the HTML image picker through its native file chooser",
);
