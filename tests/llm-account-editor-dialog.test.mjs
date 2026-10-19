import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const editorDialogJs = readFileSync(
  new URL("../static/app-llm-account-editor-dialog.js", import.meta.url),
  "utf8",
);
const accountManager = readFileSync(
  new URL("../static/app-codex-account-manager.js", import.meta.url),
  "utf8",
);
const settingsBindings = readFileSync(
  new URL("../static/app-settings-event-bindings.js", import.meta.url),
  "utf8",
);
const authStyles = readFileSync(new URL("../static/styles-auth.css", import.meta.url), "utf8");

// 弹窗骨架与脚本加载。
assert.match(indexHtml, /<dialog id="llm-account-editor-dialog"[^>]*aria-labelledby="llm-account-editor-title"/);
assert.match(indexHtml, /id="llm-account-editor-body"/);
assert.match(indexHtml, /id="llm-account-editor-close"/);
assert.match(indexHtml, /app-llm-account-editor-dialog\.js\?v=/);
assert.match(authStyles, /\.llm-account-editor-dialog\s*\{/);
assert.match(authStyles, /\.llm-account-editor-dialog::backdrop/);

// 四类编辑面板都必须在弹窗脚本中注册。
assert.match(editorDialogJs, /api-preset-editor-panel/, "Codex_API panel must be registered");
assert.match(editorDialogJs, /claude-preset-editor-panel/, "Claude_API panel must be registered");
assert.match(editorDialogJs, /auth-layout-merged/, "Codex_OAuth panel must be registered");
assert.match(editorDialogJs, /agy-editor-panel/, "AGY panel must be registered");
assert.match(editorDialogJs, /attributeFilter:\s*\["hidden"\]/);
assert.match(editorDialogJs, /showModal\(\)/);
assert.match(editorDialogJs, /cancelApiPresetEdit/);
assert.match(editorDialogJs, /cancelClaudePresetEdit/);

// agy-view 必须在导入卡片后立即闭合，claude/api/oauth 编辑面板不得嵌套其中。
assert.match(indexHtml, /<section id="agy-editor-panel" class="auth-card" hidden>/);
{
  const agyStart = indexHtml.indexOf('id="agy-view"');
  const agyPanel = indexHtml.indexOf('id="agy-editor-panel"');
  assert.ok(agyStart >= 0 && agyPanel > agyStart, "agy editor card must live inside agy-view");
  // 从 agy-editor-panel 结束标签到 claude-preset-editor-panel 之间必须出现 agy-view 的闭合。
  const afterAgyPanel = indexHtml.indexOf("</section>", agyPanel);
  const agyViewClose = indexHtml.indexOf("</section>", afterAgyPanel + "</section>".length);
  const claudePanel = indexHtml.indexOf('id="claude-preset-editor-panel"');
  assert.ok(agyViewClose > 0 && agyViewClose < claudePanel, "agy-view must close before claude editor panel");
}

// 工具栏按钮按子页签过滤。
for (const [id, kinds] of [
  ["codex-account-add-oauth", "oauth"],
  ["codex-account-save-current", "oauth"],
  ["codex-account-logout-current", "oauth"],
  ["codex-account-add-api", "api"],
  ["codex-account-add-claude", "claude"],
  ["codex-account-add-codebuddy", "codebuddy"],
  ["codex-account-add-agy", "agy"],
]) {
  assert.match(
    indexHtml,
    new RegExp(`id="${id}"[^>]*data-account-kinds="${kinds}"`),
    `${id} must declare data-account-kinds="${kinds}"`,
  );
}
assert.match(indexHtml, /id="codex-account-export"[^>]*data-account-kinds="[^"]*oauth api claude devin[^"]*"/);
assert.match(accountManager, /function syncToolbarForKind\(\)/);
assert.match(accountManager, /data-account-kinds\]/);

// 全局选项条。
assert.match(indexHtml, /id="llm-global-options-bar"/);
assert.match(indexHtml, /class="llm-global-codex-options"[^>]*data-account-kinds="oauth api"/);
assert.match(indexHtml, /id="llm-codex-approval-never"/);
assert.match(indexHtml, /id="llm-codex-sandbox-full"/);
assert.match(indexHtml, /id="llm-global-options-note"/);
assert.match(indexHtml, /id="llm-global-options-note"[\s\S]*id="codex-global-settings-open"/);
assert.match(indexHtml, /id="codex-global-settings-open"[^>]*class="[^"]*llm-global-settings-open[^"]*"[^>]*>全局设置<\/button>/);
assert.match(
  authStyles,
  /\.llm-global-options-bar\s+\.llm-global-settings-open[\s\S]*margin-left:\s*auto/,
  "global settings button must align to the right in the options bar",
);
assert.match(accountManager, /dataset\.globalTab\s*=\s*activeKind === "claude" \? "claude" : "codex"/);
assert.match(accountManager, /loadCodexCommonConfig/);
assert.match(accountManager, /saveCodexCommonConfig/);

// 全局设置对话框：同一个按钮可通过 data-global-tab 请求 Claude 分区。
assert.match(
  settingsBindings,
  /globalSettingsOpenerEl\?\.dataset\.globalTab === "claude"/,
  "openGlobalSettingsDialog must honor data-global-tab",
);

console.log("llm-account-editor-dialog assertions passed");
