import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const source = readFileSync(
  new URL("../static/terminal-session-render.js", import.meta.url),
  "utf8",
);

function createSelect() {
  return {
    children: [],
    disabled: false,
    value: "",
    set textContent(value) {
      if (value === "") {
        this.children = [];
        this.value = "";
      }
    },
    appendChild(option) {
      this.children.push(option);
      if (option.selected || (this.children.length === 1 && !this.value)) {
        this.value = option.value;
      }
    },
  };
}

function createOption() {
  return {
    dataset: {},
    selected: false,
    textContent: "",
    title: "",
    value: "",
  };
}

const normalSession = { id: "normal", name: "普通终端", origin: "normal", path: "webClx" };
const agentSession = {
  id: "s3259",
  name: "quoteNetzipRs_全推速度_诊断",
  origin: "agent",
  path: "quoteNetzipRs",
};
const mainSelect = createSelect();
const agentSelect = createSelect();
const state = {
  activeSessionId: agentSession.id,
  renamingSessionId: "",
  sessionSortMode: "",
  sessions: [normalSession, agentSession],
  terminalRenamePresets: [],
};
const sandbox = {
  console,
  document: {
    createElement(tagName) {
      assert.equal(tagName, "option");
      return createOption();
    },
  },
  state,
  sessionSelectEl: mainSelect,
  agentSessionSelectEl: agentSelect,
  idleSessionSelectEl: null,
  terminalSessionSortButtonEl: null,
  renameSessionButton: null,
  deleteSessionButton: null,
  idleSessionButton: null,
  sessionRenameDialogEl: null,
  visibleSessions: () => state.sessions,
  idleSessions: () => [],
  isIdleSession: () => false,
  sessionPath: (session) => session.path,
  sessionOptionLabel: (session) => session.name,
  sessionOptionTitle: (session) => session.name,
  sharedNormalizeTerminalSessionSortMode: (mode) => mode,
  syncTerminalInputHistoryButton() {},
};

vm.createContext(sandbox);
vm.runInContext(source, sandbox);
sandbox.renderSessions();

assert.deepEqual(
  mainSelect.children.map((option) => option.value),
  [normalSession.id, agentSession.id],
  "the primary terminal picker should contain every active terminal without a cover option",
);
assert.equal(
  mainSelect.value,
  agentSession.id,
  "the primary terminal picker should display the active managed terminal",
);
assert.equal(
  mainSelect.children.some((option) => option.textContent === "终端列表"),
  false,
  "the populated primary picker should not prepend the terminal-list cover",
);

console.log("terminal session picker regression tests passed");
