import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const codexAccountManagerJs = readFileSync(
  new URL("../static/app-codex-account-manager.js", import.meta.url),
  "utf8",
);
const terminalToolsJs = readFileSync(
  new URL("../static/app-terminal-tools-settings.js", import.meta.url),
  "utf8",
);
const codebuddyManagerJs = readFileSync(
  new URL("../static/app-codebuddy-manager.js", import.meta.url),
  "utf8",
);
const agyManagerJs = readFileSync(
  new URL("../static/app-agy-manager.js", import.meta.url),
  "utf8",
);

// 1. 统一账号工具栏（剪贴板与文件双通道）
// 导入通道：从剪贴板导入 + 导入文件
assert.match(indexHtml, /id="codex-account-import"[^>]*>从剪贴板导入<\/button>/);
assert.match(indexHtml, /id="codex-account-import-file-btn"[^>]*>导入文件<\/button>/);
assert.match(indexHtml, /<input id="codex-account-import-file"[^>]*type="file"/);
// 导出通道：导出选中（剪贴板）+ 导出文件
assert.match(indexHtml, /id="codex-account-export"[^>]*>导出选中<\/button>/);
assert.match(indexHtml, /id="codex-account-export-file"[^>]*>导出文件<\/button>/);

// 统一账号 JS 逻辑验证
assert.match(codexAccountManagerJs, /exportSelectedAccounts/);
assert.match(codexAccountManagerJs, /exportSelectedAccountsToFile/);
assert.match(codexAccountManagerJs, /importAccountsFromClipboard/);
assert.match(codexAccountManagerJs, /triggerImportAccountsFile/);
assert.match(codexAccountManagerJs, /handleImportAccountsFile/);
assert.match(codexAccountManagerJs, /applyImportAccountsPayload/);

// 2. 工作流搭建工具栏（剪贴板与文件双通道）
// 导出通道：导出（文件）+ 导出到剪贴板
assert.match(indexHtml, /id="terminal-tool-export"[^>]*>导出<\/button>/);
assert.match(indexHtml, /id="terminal-tool-export-clipboard"[^>]*>导出到剪贴板<\/button>/);
// 导入通道：导入（文件）+ 从剪贴板导入
assert.match(indexHtml, /id="terminal-tool-import"[^>]*>导入<\/button>/);
assert.match(indexHtml, /id="terminal-tool-import-clipboard"[^>]*>从剪贴板导入<\/button>/);
assert.match(indexHtml, /<input id="terminal-tool-import-file"[^>]*type="file"/);

// 工作流 JS 逻辑验证
assert.match(terminalToolsJs, /exportTerminalToolEntries/);
assert.match(terminalToolsJs, /exportTerminalToolEntriesToClipboard/);
assert.match(terminalToolsJs, /importTerminalToolEntries/);
assert.match(terminalToolsJs, /importTerminalToolEntriesFromClipboard/);
assert.match(terminalToolsJs, /importTerminalToolData/);

// 3. CodeBuddy 子模块（剪贴板与文件双通道）
assert.match(indexHtml, /id="codebuddy-export-file"[^>]*>导出 JSON<\/button>/);
assert.match(indexHtml, /id="codebuddy-export-clipboard"[^>]*>导出到剪贴板<\/button>/);
assert.match(indexHtml, /id="codebuddy-import-toolbar"[^>]*>导入文件<\/button>/);
assert.match(indexHtml, /id="codebuddy-import-clipboard"[^>]*>从剪贴板导入<\/button>/);
assert.match(codebuddyManagerJs, /exportAccountsFile/);
assert.match(codebuddyManagerJs, /exportAccountsClipboard/);
assert.match(codebuddyManagerJs, /importAccountsFromClipboard/);
assert.match(codebuddyManagerJs, /importAccountsData/);

// 4. AGY 子模块（双通道保留验证）
assert.match(indexHtml, /id="agy-export-file"[^>]*>导出 JSON<\/button>/);
assert.match(indexHtml, /id="agy-export-clipboard"[^>]*>导出到剪贴板<\/button>/);
assert.match(indexHtml, /id="agy-import-file-button"[^>]*>选择文件<\/button>/);
assert.match(indexHtml, /id="agy-import-clipboard"[^>]*>从剪贴板读取<\/button>/);
assert.match(agyManagerJs, /exportAccountsFile/);
assert.match(agyManagerJs, /exportAccountsClipboard/);
assert.match(agyManagerJs, /readImportClipboard/);
assert.match(agyManagerJs, /readImportFile/);

console.log("JSON dual channel import/export tests passed successfully!");
