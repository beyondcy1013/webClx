import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const terminalNavigationJs = readFileSync(
  new URL("../static/terminal-navigation-layout.js", import.meta.url),
  "utf8",
);
const appNavigationJs = readFileSync(
  new URL("../static/app-navigation-tabs.js", import.meta.url),
  "utf8",
);
const appSessionRenderJs = readFileSync(
  new URL("../static/app-home-session-render.js", import.meta.url),
  "utf8",
);
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");

function functionSource(source, name) {
  const start = source.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, `missing function ${name}`);
  const bodyStart = source.indexOf("{", start);
  let depth = 0;
  for (let index = bodyStart; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(start, index + 1);
  }
  assert.fail(`unterminated function ${name}`);
}

const terminalContext = vm.createContext({
  URLSearchParams,
  state: {
    activeSessionId: "session-A",
    currentPath: "webClx",
  },
  normalizeTerminalPath: (value) => String(value || ""),
});
vm.runInContext(functionSource(terminalNavigationJs, "buildWorkspaceUrl"), terminalContext);
assert.equal(
  vm.runInContext('buildWorkspaceUrl("/workspace")', terminalContext),
  "/workspace?path=webClx&terminal_session=session-A",
  "leaving a terminal for the workspace must carry the exact active session id",
);

const appContext = vm.createContext({
  URLSearchParams,
  state: {
    returnTerminalSessionId: "session-A",
  },
});
vm.runInContext(functionSource(appNavigationJs, "workspacePathQuery"), appContext);
assert.equal(
  vm.runInContext('workspacePathQuery("webClx")', appContext),
  "?path=webClx&terminal_session=session-A",
  "workspace tab navigation must preserve the terminal return session id",
);

assert.match(
  indexHtml,
  /id="top-nav-terminal"[^>]+href="\/terminal"/,
  "the workspace terminal tab must expose a stable element for session-aware href updates",
);
assert.match(
  indexHtml,
  /app-home-session-render\.js\?v=20260801a/,
  "terminal-management navigation changes should use a fresh asset version",
);
assert.match(
  functionSource(appSessionRenderJs, "syncSessionsTerminalLink"),
  /topNavTerminalLink\.href\s*=\s*targetUrl/,
  "the workspace terminal tab must use the same explicit session URL as the terminal action",
);

const sessionTerminalLink = { href: "" };
const topNavTerminalLink = { href: "" };
const terminalLinkContext = vm.createContext({
  state: {
    currentPath: "webClx",
    preferredSessionId: "",
    returnTerminalSessionId: "",
    sessions: [],
  },
  sessionTerminalLink,
  topNavTerminalLink,
  activeGlobalSession() {
    return terminalLinkContext.state.sessions.find(
      (session) => session.id === terminalLinkContext.state.preferredSessionId,
    ) || null;
  },
  buildTerminalUrl(path, sessionId = "") {
    const params = new URLSearchParams();
    if (path) params.set("path", path);
    if (sessionId) params.set("session", sessionId);
    const query = params.toString();
    return query ? `/terminal?${query}` : "/terminal";
  },
  buildFreshTerminalUrl(path) {
    const params = new URLSearchParams({ path, fresh: "1", quick_start: "1" });
    return `/terminal?${params.toString()}`;
  },
});
vm.runInContext(
  functionSource(appSessionRenderJs, "syncSessionsTerminalLink"),
  terminalLinkContext,
);
vm.runInContext("syncSessionsTerminalLink()", terminalLinkContext);
assert.equal(
  topNavTerminalLink.href,
  "/terminal?path=webClx",
  "opening terminal management without a return session must not request a fresh terminal",
);
assert.match(
  sessionTerminalLink.href,
  /[?&]fresh=1(?:&|$)/,
  "the explicit open-terminal action may still request a fresh terminal",
);

console.log("terminal navigation session return tests passed");
