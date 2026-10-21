import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const stylesAuth = readFileSync(new URL("../static/styles-auth.css", import.meta.url), "utf8");
const agyManagerSource = readFileSync(new URL("../static/app-agy-manager.js", import.meta.url), "utf8");

// 1. index.html 验证：AGY 表格操作列置于第一列，与 Codex 保持一致
const theadMatch = indexHtml.match(/<table class="[^"]*agy-table[^"]*">[\s\S]*?<thead>[\s\S]*?<tr>([\s\S]*?)<\/tr>/);
assert.ok(theadMatch, "index.html 必须包含 agy-table thead tr");
const theadContent = theadMatch[1];
const firstThMatch = theadContent.match(/<th class="([^"]*)">([^<]*)<\/th>/);
assert.ok(firstThMatch, "thead tr 必须有首个 th");
assert.ok(firstThMatch[1].includes("agy-col-actions"), "AGY 表格第一列应为 agy-col-actions 操作列");
assert.equal(firstThMatch[2].trim(), "操作", "AGY 表格第一列标题应为 '操作'");

// 2. styles-auth.css 验证：agy 操作列使用 44px 紧凑 sticky 样式，并具备当前行高亮
assert.match(
  stylesAuth,
  /\.agy-col-actions[\s\S]*?position:\s*sticky[\s\S]*?left:\s*0/,
  "agy-col-actions 必须为 sticky left: 0 样式",
);
assert.match(
  stylesAuth,
  /\.agy-table\s+\.codex-account-action-cell/,
  "styles-auth.css 应覆盖 .agy-table .codex-account-action-cell 的位置",
);
assert.match(
  stylesAuth,
  /thead\s+th\.agy-col-actions[\s\S]*?position:\s*sticky[\s\S]*?left:\s*0/,
  "thead th.agy-col-actions 必须为 sticky left: 0 样式",
);
assert.match(
  stylesAuth,
  /tr\.agy-row-current\s+td\.agy-col-actions/,
  "当前生效账号行的操作列必须具有匹配背景色以防水平滚动透底",
);

// 3. app-agy-manager.js 验证：renderRow 使用 createPresetActionMenu 生成与 codex 一致的操作菜单
let capturedMenuActions = null;
let capturedMenuOpts = null;

class MockElement {
  constructor(tag) {
    this.tagName = tag.toUpperCase();
    this.children = [];
    this.classList = new Set();
    this.className = "";
    this.attributes = {};
    this.innerHTML = "";
  }
  appendChild(child) {
    this.children.push(child);
    return child;
  }
  insertAdjacentHTML(position, html) {
    if (position === "beforeend") {
      this.innerHTML += html;
    }
  }
  setAttribute(k, v) {
    this.attributes[k] = v;
  }
}

const context = {
  window: {},
  document: {
    createElement: (tag) => new MockElement(tag),
    getElementById: () => null,
  },
  localStorage: { getItem: () => null, setItem: () => {} },
  CSS: { escape: (v) => String(v) },
  navigator: {},
  fetch: () => Promise.resolve({ ok: true, json: () => Promise.resolve([]) }),
  createPresetActionMenu: (actions, opts) => {
    capturedMenuActions = actions;
    capturedMenuOpts = opts;
    const btn = new MockElement("button");
    btn.className = "mini-button preset-action-menu-trigger";
    return btn;
  },
};

vm.runInNewContext(agyManagerSource, context);
const { renderRow } = context.window.agyManager;
assert.equal(typeof renderRow, "function", "renderRow 必须导出在 window.agyManager 上");

// 测试非当前生效账号
const item1 = {
  id: "agy-account-1",
  name: "测试账号1",
  email: "test1@gmail.com",
  current: false,
  valid: true,
  saved_at: 1720000000,
};

const row1 = renderRow(item1);
assert.equal(row1.children.length, 1, "操作列应作为首个子节点 appended 到 row");
const actionCell1 = row1.children[0];
assert.ok(actionCell1.className.includes("agy-col-actions"), "首个 cell 必须包含 agy-col-actions 类名");
assert.ok(actionCell1.className.includes("codex-account-action-cell"), "首个 cell 必须包含 codex-account-action-cell 类名");
assert.equal(actionCell1.children.length, 1, "操作列内应挂载菜单 trigger 按钮");

// 校验传递给 createPresetActionMenu 的 actions
assert.ok(Array.isArray(capturedMenuActions), "必须向 createPresetActionMenu 传递 actions 数组");
assert.equal(capturedMenuOpts.label, "测试账号1 的操作");

const switchAction = capturedMenuActions.find((a) => a.label === "切换到此账号");
assert.ok(switchAction, "操作菜单应包含 '切换到此账号'");
assert.equal(switchAction.disabled, false, "非当前账号应允许切换");

const quotaAction = capturedMenuActions.find((a) => a.label === "查询配额");
assert.ok(quotaAction, "操作菜单应包含 '查询配额'");

const testAction = capturedMenuActions.find((a) => a.label === "测试");
assert.ok(testAction, "操作菜单应包含 '测试'");

const deleteAction = capturedMenuActions.find((a) => a.label === "删除");
assert.ok(deleteAction, "操作菜单应包含 '删除'");
assert.equal(deleteAction.danger, true, "删除操作应标记 danger: true");

// 测试当前生效账号（应禁用切换按钮并标记已是当前账号）
const item2 = {
  id: "agy-account-2",
  name: "当前生效账号",
  email: "current@gmail.com",
  current: true,
  valid: true,
  saved_at: 1720000000,
};

const row2 = renderRow(item2);
assert.ok(row2.className.includes("current-row"), "当前账号行必须带有 current-row 类名");
const switchAction2 = capturedMenuActions.find((a) => a.label === "切换到此账号");
assert.equal(switchAction2.disabled, true, "当前账号的切换选项应为 disabled");
assert.equal(switchAction2.title, "已是当前账号");

console.log("AGY action column style and behavior unit tests passed successfully!");
