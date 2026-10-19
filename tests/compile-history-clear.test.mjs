import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const managerJs = readFileSync(
  new URL("../static/app-compile-status-manager.js", import.meta.url),
  "utf8",
);

function fakeElement(textContent = "") {
  const listeners = new Map();
  return {
    textContent,
    innerHTML: "",
    dataset: {},
    disabled: false,
    addEventListener(type, listener) { listeners.set(type, listener); },
    dispatch(type) { return listeners.get(type)?.({ target: this }); },
    insertAdjacentHTML(_position, html) { this.innerHTML += html; },
  };
}

const requests = [];
const toasts = [];
let confirmed = false;
let resolveStatusRefresh;
const statusRefresh = new Promise((resolve) => { resolveStatusRefresh = resolve; });
const context = vm.createContext({
  console,
  setTimeout,
  clearTimeout,
  window: { confirm: () => confirmed },
});
vm.runInContext(managerJs, context);

const elements = {
  compilePendingListEl: fakeElement(),
  compileRunningListEl: fakeElement(),
  compileRunListEl: fakeElement(),
  compileStatusMessageEl: fakeElement(),
  compileStatusRefreshButtonEl: fakeElement("刷新状态"),
  compileHistoryClearButtonEl: fakeElement("清空历史"),
};
const manager = context.WebClxCompileStatusManager.create({
  state: {
    compileStatusRequestToken: 0,
    workspaceDir: "/home/codes/tools/webClx",
    defaultWorkspaceDir: "/home/codes",
  },
  async requestJson(url, options = {}) {
    requests.push([url, options.method || "GET"]);
    if (url === "/api/build/compile/history") return { ok: true, deleted_count: 3 };
    return statusRefresh;
  },
  escapeHtml: String,
  setTextContent(element, value) { element.textContent = String(value); },
  setInlineStatus(element, value, tone) { element.textContent = value; element.dataset.tone = tone; },
  setButtonBusy(button, busy, label = "") {
    button.disabled = busy;
    button.textContent = busy && label ? label : button.dataset.defaultLabel || "清空历史";
  },
  formatDateTimeLong: String,
  setActiveTab() {},
  loadFile() {},
  showToast(message, tone) { toasts.push([message, tone]); },
  elements,
});

await elements.compileHistoryClearButtonEl.dispatch("click");
assert.deepEqual(requests, [], "cancelled confirmation must not clear history");

confirmed = true;
const clearPromise = elements.compileHistoryClearButtonEl.dispatch("click");
await new Promise((resolve) => setTimeout(resolve, 0));
assert.deepEqual(
  toasts,
  [["已清空 3 条构建运行历史。", "ok"]],
  "success feedback must not wait for the follow-up status refresh",
);
assert.equal(
  elements.compileHistoryClearButtonEl.disabled,
  false,
  "the clear button must recover as soon as DELETE succeeds",
);
assert.match(
  elements.compileRunListEl.innerHTML,
  /暂无已结束的构建运行历史/,
  "the history table should clear immediately before background refresh finishes",
);
assert.deepEqual(requests, [
  ["/api/build/compile/history", "DELETE"],
  ["/api/build/compile/status?include_history=false", "GET"],
]);
resolveStatusRefresh({ ok: true, queue_dir: "/tmp/compile", pending_count: 0, run_count: 0, pending_requests: [], runs: [] });
await clearPromise;
assert.equal(typeof manager.loadCompileStatus, "function");

console.log("compile history clear tests passed");
