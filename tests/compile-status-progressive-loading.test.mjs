import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const compileServiceRs = readFileSync(
  new URL("../src/compile_service.rs", import.meta.url),
  "utf8",
);

const managerJs = readFileSync(
  new URL("../static/app-compile-status-manager.js", import.meta.url),
  "utf8",
);
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");

function deferred() {
  let resolve;
  const promise = new Promise((next) => {
    resolve = next;
  });
  return { promise, resolve };
}

function fakeElement(textContent = "") {
  return {
    textContent,
    innerHTML: "",
    dataset: {},
    disabled: false,
    addEventListener() {},
    insertAdjacentHTML(_position, html) {
      this.innerHTML += html;
    },
  };
}

const context = vm.createContext({
  console,
  setTimeout,
  clearTimeout,
  showToast() {},
});
vm.runInContext(managerJs, context);

const historyResponse = deferred();
const requestedUrls = [];
const elements = {
  compilePendingListEl: fakeElement(),
  compileRunningListEl: fakeElement(),
  compileRunListEl: fakeElement(),
  compileStatusMessageEl: fakeElement(),
  compileStatusRefreshButtonEl: fakeElement("刷新状态"),
};
const state = {
  compileStatus: null,
  compileStatusRequestToken: 0,
  workspaceDir: "/home/codes/webClx",
  defaultWorkspaceDir: "/home/codes",
};
const liveResponse = {
  ok: true,
  queue_dir: "/home/codes/webClx/.webclx-compile-queue",
  pending_count: 1,
  run_count: 423,
  latest_log: "/tmp/latest.log",
  pending_requests: [{
    request_id: "pending-1",
    request_kind: "deploy",
    project: "webClx",
    command: ["cargo", "build"],
  }],
  runs: [{
    run_id: "run-live",
    status: "running",
    request_count: 2,
    projects: ["runAny", "stockScreener"],
    source_terminal_names: ["stockScreener_1"],
    started_at: "2026-07-15 09:25:00",
    current_project: "stockScreener",
    current_phase: "compile",
    current_spec_index: 2,
    spec_count: 2,
    packages_completed: 37,
    packages_total: 120,
    current_package: "tokio",
    log_path: "/tmp/live.log",
  }],
  logs: [],
};
const fullResponse = {
  ...liveResponse,
  runs: [{
    run_id: "run-1",
    status: "success",
    request_count: 1,
    projects: ["webClx"],
    source_terminal_names: ["webClx_1"],
    started_at: "2026-07-15 06:00:00",
    finished_at: "2026-07-15 06:01:00",
    log_path: "/tmp/latest.log",
  }],
};

const manager = context.WebClxCompileStatusManager.create({
  state,
  requestJson(url) {
    requestedUrls.push(url);
    if (requestedUrls.length === 1) {
      return Promise.resolve(liveResponse);
    }
    return historyResponse.promise;
  },
  escapeHtml(value) {
    return String(value ?? "")
      .replaceAll("&", "&amp;")
      .replaceAll("<", "&lt;")
      .replaceAll(">", "&gt;")
      .replaceAll('"', "&quot;");
  },
  setTextContent(element, value) {
    element.textContent = String(value);
  },
  setInlineStatus(element, value, tone) {
    element.textContent = value;
    element.dataset.tone = tone;
  },
  setButtonBusy(button, busy, label = "") {
    button.disabled = busy;
    button.textContent = busy && label ? label : "刷新状态";
  },
  formatDateTimeLong(value) {
    return value.toISOString();
  },
  setActiveTab() {},
  loadFile() {},
  showToast() {},
  elements,
});

await manager.loadCompileStatus();

assert.deepEqual(
  requestedUrls,
  [
    "/api/build/compile/status?include_history=false",
    "/api/build/compile/status",
  ],
  "live compile state should load before the independent history request",
);
assert.equal(
  elements.compileStatusMessageEl.textContent,
  "队列：/home/codes/webClx/.webclx-compile-queue · 等待 1 · 进行中 1 · 历史 423 · 历史加载中 · 每 2 秒更新",
);
assert.doesNotMatch(elements.compileStatusMessageEl.textContent, /集中日志|latest\.log/);
assert.match(elements.compilePendingListEl.innerHTML, /pending-1/);
assert.match(elements.compileRunningListEl.innerHTML, /第 2\/2 项/);
assert.match(elements.compileRunningListEl.innerHTML, /37\/120/);
assert.match(elements.compileRunningListEl.innerHTML, /tokio/);
assert.match(
  elements.compileRunListEl.innerHTML,
  /正在后台加载 423 条历史记录/,
  "history should remain a non-blocking loading state while live content is visible",
);

historyResponse.resolve(fullResponse);
await new Promise((resolve) => setTimeout(resolve, 10));

assert.match(elements.compileRunListEl.innerHTML, /run-1/);
assert.equal(
  elements.compileStatusMessageEl.textContent,
  "队列：/home/codes/webClx/.webclx-compile-queue · 等待 1 · 进行中 0 · 历史 423 · 每 2 秒更新",
);
assert.equal(elements.compileStatusRefreshButtonEl.disabled, false);
assert.match(managerJs, /COMPILE_LIVE_REFRESH_MS\s*=\s*2_000/);
assert.equal(typeof manager.startLiveRefresh, "function");
assert.equal(typeof manager.stopLiveRefresh, "function");
assert.match(
  compileServiceRs,
  /tokio::task::spawn_blocking\(move \|\| compile_status_snapshot\(include_history\)\)/,
  "filesystem-heavy compile status scans must not run on an async request worker",
);
assert.match(
  compileServiceRs,
  /filter\(\|run_dir\| !run_dir\.join\("run-finished-at"\)\.is_file\(\)\)/,
  "live polling must skip completed run directories before parsing their history files",
);
assert.match(
  indexHtml,
  /class="panel-head wide compile-overview-head"[\s\S]*class="compile-overview-title card-title-help"[\s\S]*id="compile-status-message"[\s\S]*id="compile-status-refresh"/,
  "compile status and refresh controls should live in the Build API title row",
);
assert.doesNotMatch(
  indexHtml,
  /compile-summary-grid|compile-latest-log|最新集中日志/,
  "the removed compile summary cards and centralized log field should not return",
);

console.log("compile status progressive loading tests passed");
