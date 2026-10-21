import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const accountManagerSource = readFileSync(
  new URL("../static/app-codex-account-manager.js", import.meta.url),
  "utf8",
);

class MockElement {
  constructor(tag) {
    this.tagName = tag.toUpperCase();
    this.children = [];
    this.classList = new Set();
    this.className = "";
    this.style = {};
    this.attributes = {};
    this.textContent = "";
    this.title = "";
    this.dataset = {};
  }
  appendChild(child) {
    this.children.push(child);
    return child;
  }
  setAttribute(k, v) {
    this.attributes[k] = v;
  }
  addEventListener() {}
}

const context = {
  window: {
    location: { pathname: "/codebuddy", hash: "" },
    addEventListener: () => {},
  },
  document: {
    createElement: (tag) => new MockElement(tag),
    getElementById: () => null,
    querySelectorAll: () => [],
    querySelector: () => null,
  },
  localStorage: { getItem: () => null, setItem: () => {} },
  CSS: { escape: (v) => String(v) },
  navigator: {},
  fetch: () => Promise.resolve({ ok: true, json: () => Promise.resolve([]) }),
  createPresetActionMenu: () => new MockElement("button"),
  createCurrentIndicatorCell: () => new MockElement("td"),
  setTimeout: () => {},
  clearTimeout: () => {},
};

vm.runInNewContext(accountManagerSource, context);
const {
  BENCHMARK_CREDITS,
  creditsProgressBar,
  creditsTotalCell,
  simplifyCreditPackageName,
  creditsBreakdownCell,
} = context.window.WebClxCodeBuddyCredits;

assert.equal(BENCHMARK_CREDITS, 3000, "基准满分必须为 3000 分");

// 1. 验证进度条按 3000 比例计算
{
  const barFull = creditsProgressBar(1.0);
  assert.equal(barFull.children[0].style.width, "100%");

  const barHalf = creditsProgressBar(0.5);
  assert.equal(barHalf.children[0].style.width, "50%");

  const barQuarter = creditsProgressBar(0.25);
  assert.equal(barQuarter.children[0].style.width, "25%");

  const barZero = creditsProgressBar(0);
  assert.equal(barZero.children[0].style.width, "0%");

  const barOverflow = creditsProgressBar(1.5);
  assert.equal(barOverflow.children[0].style.width, "100%", "超出基准时应截断在 100%");

  const barTiny = creditsProgressBar(0.005);
  assert.equal(barTiny.children[0].style.width, "3%", "极小正数应保留 3% 最小可见度");
}

// 2. 验证多账号横向对比场景：不同历史总量账号以 3000 分为同一基准绘图
{
  // 账号 A：历史总量仅 1500，剩余 1500
  // 旧逻辑：1500/1500 => 100% 满格，误导用户以为比 3000 分账号还多
  // 新逻辑：1500/3000 => 50%
  const accountA = {
    kind: "codebuddy",
    credits: {
      ok: true,
      totalRemaining: 1500,
      totalCapacity: 1500,
    },
  };
  const cellA = creditsTotalCell(accountA);
  const barA = cellA.children[0];
  const textA = cellA.children[1];
  assert.equal(barA.children[0].style.width, "50%");
  assert.equal(textA.textContent, "1500", "总积分列文字仅展示剩余积分数字，不再显示斜杠与总量");
  assert.ok(cellA.title.includes("3000分基准占比 50%"));
  assert.ok(cellA.title.includes("总量 1500"), "悬停 title 保留完整总量");

  // 账号 B：历史总量 5000，剩余 3000
  // 旧逻辑：3000/5000 => 60%
  // 新逻辑：3000/3000 => 100% 满格
  const accountB = {
    kind: "codebuddy",
    credits: {
      ok: true,
      totalRemaining: 3000,
      totalCapacity: 5000,
    },
  };
  const cellB = creditsTotalCell(accountB);
  const barB = cellB.children[0];
  const textB = cellB.children[1];
  assert.equal(barB.children[0].style.width, "100%");
  assert.equal(textB.textContent, "3000", "总积分列文字仅展示剩余积分数字");
  assert.ok(cellB.title.includes("3000分基准占比 100%"));

  // 账号 C：带小数且总量较大，如 202.69 / 5128
  // 必须只显示 "202.69"，不含 "/ 5128"
  const accountC = {
    kind: "codebuddy",
    credits: {
      ok: true,
      totalRemaining: 202.69,
      totalCapacity: 5128,
    },
  };
  const cellC = creditsTotalCell(accountC);
  const textC = cellC.children[1];
  assert.equal(textC.textContent, "202.69", "带小数总积分仅展示精确小数剩余数值，如 202.69");
  assert.ok(!textC.textContent.includes("/"), "不应包含斜杠与总量");
  assert.ok(cellC.title.includes("总量 5128"), "title 依然记录总量");
}

// 3. 验证分类积分包名精简映射
{
  assert.equal(simplifyCreditPackageName("平台奖励积分"), "奖励");
  assert.equal(simplifyCreditPackageName("版本基础用量"), "基础");
  assert.equal(simplifyCreditPackageName("购买积分"), "购买");
  assert.equal(simplifyCreditPackageName("版本赠送用量"), "赠送");
  assert.equal(simplifyCreditPackageName("CodeBuddy 个人体验版"), "体验");
  assert.equal(simplifyCreditPackageName("CodeBuddy 个人标准版"), "标准");
  assert.equal(simplifyCreditPackageName(""), "其他");
}

// 4. 验证分类积分单元格展示格式：例如 "奖励1300 基础309.50"
{
  const account = {
    kind: "codebuddy",
    credits: {
      ok: true,
      resources: [
        { packageName: "平台奖励积分", remaining: 1300, total: 1300, expireAt: 1780000000000 },
        { packageName: "版本基础用量", remaining: 309.5, total: 500, expireAt: 1780000000000 },
      ],
    },
  };
  const cell = creditsBreakdownCell(account);
  assert.equal(
    cell.textContent,
    "奖励1300 基础309.50",
    "分类积分列文案必须精确为紧凑标签格式，各项之间以空格分隔",
  );
  assert.ok(cell.title.includes("平台奖励积分：剩余 1300"), "悬停 title 必须保留完整包名");
  assert.ok(cell.title.includes("版本基础用量：剩余 309.50"), "悬停 title 必须保留完整包名");
}

console.log("CodeBuddy credits benchmark and breakdown format tests passed successfully!");
