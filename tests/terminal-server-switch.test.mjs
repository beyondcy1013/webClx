import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const serverSwitchSource = readFileSync(
  new URL("../static/terminal-server-switch.js", import.meta.url),
  "utf8",
);
const pageRoutesSource = readFileSync(new URL("../src/routes/pages.rs", import.meta.url), "utf8");
const loginSource = readFileSync(new URL("../src/login.rs", import.meta.url), "utf8");
const serverProbeSource = readFileSync(
  new URL("../static/terminal-server-probe.js", import.meta.url),
  "utf8",
);
const terminalMobileKeysSource = readFileSync(
  new URL("../static/terminal-mobile-keys.js", import.meta.url),
  "utf8",
);
const terminalStyles = readFileSync(
  new URL("../static/styles-terminal.css", import.meta.url),
  "utf8",
);

test("project commands expose server switching first and provide the configured targets", () => {
  assert.match(
    terminalHtml,
    /id="terminal-project-command-button"[\s\S]*?>项目管理<\/button>/,
  );
  assert.match(
    terminalHtml,
    /id="terminal-project-command-menu"[\s\S]*?data-project-action="switch_server">切换服务器<\/button>/,
  );
  assert.match(
    terminalHtml,
    /<option value="">项目指令<\/option>\s*<option value="switch_server">切换服务器<\/option>/,
  );
  assert.match(terminalHtml, /id="terminal-server-switch-dialog"/);
  assert.match(terminalHtml, /id="terminal-server-switch-select"/);
  assert.match(
    terminalHtml,
    /id="top-nav-server"[\s\S]*?id="top-nav-terminal-archives"/,
  );
  assert.doesNotMatch(terminalHtml, /id="terminal-view-tabs"/);
  assert.doesNotMatch(terminalHtml, /id="terminal-view-tab-(terminal|server)"/);
  assert.match(terminalHtml, /aria-labelledby="top-nav-server"/);
  assert.match(
    terminalHtml,
    /id="terminal-server-frame-select"[\s\S]*?id="terminal-server-frame-edit"[\s\S]*?>\s*编辑服务器\s*<\/button>/,
  );
  assert.match(terminalHtml, /id="terminal-server-edit-dialog"/);
  assert.match(terminalHtml, /id="terminal-server-edit-select"/);
  assert.match(terminalHtml, /id="terminal-server-edit-input"/);
  assert.match(terminalHtml, /id="terminal-workspace"/);
  assert.match(
    terminalHtml,
    /id="terminal-server-workspace"[\s\S]*?<iframe[^>]+id="terminal-server-frame"[^>]*title="远程 webClx 服务器"/,
  );
  for (const target of [
    "PUBLIC_DOMAIN",
    "PUBLIC_DOMAIN:PORT",
    "PUBLIC_DOMAIN:14151",
    "us.PUBLIC_DOMAIN",
    "jd.PUBLIC_DOMAIN",
  ]) {
    assert.match(terminalHtml, new RegExp(`value="${target.replaceAll(".", "\\.")}"`));
  }
  assert.doesNotMatch(terminalHtml, /fpsq\.xyz/);
  assert.doesNotMatch(terminalHtml, /ccszxc\.site/);
  assert.doesNotMatch(terminalHtml, /192\.168\.\d+\.\d+/);
  assert.match(
    terminalHtml,
    /value="PUBLIC_DOMAIN">PUBLIC_DOMAIN:PORT<\/option>\s*<option value="PUBLIC_DOMAIN:PORT">PUBLIC_DOMAIN:PORT<\/option>/,
  );
  assert.match(terminalHtml, /terminal-server-switch\.js\?v=/);
  assert.match(
    serverSwitchSource,
    /terminalTab\?\.addEventListener\("click", \(event\) => \{\s*event\.preventDefault\(\);\s*setTerminalServerView\(false\);/,
  );
  assert.match(
    terminalStyles,
    /body\[data-terminal-view="server"\] \.terminal-control-bar,\s*body\[data-terminal-view="server"\] #terminal-mobile-keys/,
  );
  assert.match(serverSwitchSource, /function syncTerminalServerFrameHeight\(\)/);
  assert.match(serverSwitchSource, /const TERMINAL_SERVER_PRESETS_STORAGE_KEY = "webclx:terminal-server-presets"/);
  assert.match(
    serverSwitchSource,
    /frameEdit\?\.addEventListener\("click"[\s\S]*editDialog\.showModal\(\)/,
  );
  assert.match(
    serverSwitchSource,
    /editSelect\?\.addEventListener\("change"[\s\S]*editInput\.value = editSelect\.value/,
  );
  assert.match(
    serverSwitchSource,
    /const option = editSelect\?\.selectedOptions\?\.\[0\]/,
  );
  assert.match(
    serverSwitchSource,
    /editForm\?\.addEventListener\("submit"[\s\S]*frameSelect\.options\[optionIndex\]\.value = option\.value[\s\S]*select\.options\[optionIndex\]\.value = option\.value[\s\S]*saveTerminalServerPreset\(optionIndex, option\.value\)/,
  );
  assert.match(
    serverSwitchSource,
    /window\.innerHeight -\s*\n\s*Math\.max\(viewRect\.top, 0\) -/,
  );
  assert.match(
    serverSwitchSource,
    /async function ensureHostSessionForServerFrame\(\)[\s\S]*fetch\("\/api\/auth\/session"[\s\S]*session\?\.authenticated/,
  );
  assert.match(
    serverSwitchSource,
    /frameForm\?\.addEventListener\("submit", async \(event\) => \{[\s\S]*connectTerminalServerFrameWithHostAuth\(frameSelect\.value\)/,
  );
  assert.match(
    serverSwitchSource,
    /async function issueTerminalServerFrameTicket\(targetUrl\)[\s\S]*X-WebClx-Target-Origin/,
  );
  assert.match(
    serverSwitchSource,
    /function terminalServerFrameLoginUrl\(targetUrl, ticket\)[\s\S]*cross_instance_ticket: ticket/,
  );
  assert.match(pageRoutesSource, /"\/api\/auth\/cross-instance-ticket", get\(login::cross_instance_ticket\)/);
  assert.match(pageRoutesSource, /"\/api\/auth\/cross-instance-login", post\(login::cross_instance_login\)/);
  assert.match(loginSource, /fn cross_instance_trust_secret\(\)[\s\S]*CROSS_INSTANCE_TRUST_SECRET/);
  assert.match(loginSource, /TICKET_TTL_SECS: u64 = 30 \* 24 \* 60 \* 60/);
  assert.match(loginSource, /ticket\.aud != request_origin/);
  assert.match(loginSource, /ticket\.aud != listen_origin/);
  assert.match(
    terminalMobileKeysSource,
    /if \(action === "switch_server"\) \{\s*openTerminalServerSwitchDialog\(\);\s*\} else if \(action === "deploy_project"\)/,
  );
});

test("project management and terminal tools use compact custom menus", () => {
  assert.match(terminalHtml, /id="terminal-project-command-select"[\s\S]*?hidden/);
  assert.match(terminalStyles, /\.terminal-project-command-menu \{[^}]*width: min\(144px,/);
  assert.match(terminalStyles, /\.terminal-project-command-menu > button \{[^}]*min-height: 28px;[^}]*font: 600 12px\/1\.25[^}]*white-space: normal/);
  assert.match(terminalStyles, /\.terminal-tools-menu-with-tools \{[^}]*width: min\(144px,/);
  assert.match(
    terminalStyles,
    /\.terminal-tools-menu-with-tools \.terminal-tools-quick-commands \{[^}]*grid-template-columns: minmax\(0, 1fr\);/,
  );
  assert.match(
    terminalStyles,
    /\.terminal-function-command-menu \.terminal-tools-actions \{[^}]*border-bottom: 0;[^}]*border-radius: 0;/,
  );
  assert.match(terminalHtml, /data-shortcut="\^B"/);
  assert.match(
    terminalStyles,
    /\.terminal-tools-menu-with-tools \.terminal-tools-action \{[^}]*min-height: 28px;[^}]*background: transparent;[^}]*font: 600 12px\/1[^}]*text-align: left;/,
  );
  assert.match(
    terminalStyles,
    /\.terminal-tools-menu-with-tools \.terminal-tools-option \{[^}]*min-height: 28px;[^}]*padding: 3px 7px;[^}]*font: 600 12px\/1/,
  );
  assert.match(
    terminalStyles,
    /\.terminal-tools-menu-with-tools \.terminal-tool-menu-item-action \.terminal-tool-menu-item-detail \{[^}]*display: none;/,
  );
  assert.match(
    terminalStyles,
    /\.terminal-tools-menu-with-tools \.terminal-tool-menu-item-label \{[^}]*overflow: hidden;[^}]*text-overflow: ellipsis;[^}]*white-space: nowrap;/,
  );
});

test("server targets default to port 11111 and preserve the current terminal route", () => {
  const sandbox = {
    document: { getElementById() { return null; } },
    window: {
      location: {
        pathname: "/terminal",
        search: "?path=webClx",
        hash: "#session",
      },
    },
    URL,
  };
  vm.createContext(sandbox);
  vm.runInContext(serverSwitchSource, sandbox);

  assert.equal(sandbox.normalizeTerminalServerTarget("example.invalid"), "http://example.invalid:11111");
  assert.equal(
    sandbox.normalizeTerminalServerTarget("example.invalid:11112"),
    "http://example.invalid:11112",
  );
  assert.equal(
    sandbox.buildTerminalServerSwitchUrl("127.0.0.1", sandbox.window.location),
    "http://127.0.0.1:11111/terminal?path=webClx#session",
  );
  assert.equal(sandbox.editableTerminalServerTarget("example.test"), "example.test:11111");
  assert.equal(
    sandbox.editableTerminalServerTarget("https://example.test"),
    "https://example.test:11111",
  );
});

test("server switching stays inside Android and falls back to browser navigation", () => {
  const assigned = [];
  const openedInWebView = [];
  const location = {
    pathname: "/terminal",
    search: "?path=webClx",
    hash: "#session",
    assign(url) { assigned.push(url); },
  };
  const sandbox = {
    document: { getElementById() { return null; } },
    window: { location },
    URL,
  };
  vm.createContext(sandbox);
  vm.runInContext(serverSwitchSource, sandbox);

  const expected = "http://example.invalid:14151/terminal?path=webClx#session";
  assert.equal(
    sandbox.navigateToTerminalServer("example.invalid:14151", location, {
      openInWebView(url) { openedInWebView.push(url); },
    }),
    expected,
  );
  assert.deepEqual(openedInWebView, [expected]);
  assert.deepEqual(assigned, []);

  sandbox.navigateToTerminalServer("example.invalid:14151", location, null);
  assert.deepEqual(assigned, [expected]);
});

test("server probing identifies the current server by origin including its port", () => {
  assert.match(serverProbeSource, /window\.location\.origin === originFor\(host\)/);
  assert.match(serverProbeSource, /const currentResult = await probeHost\(currentOrigin\)/);
  assert.match(serverProbeSource, /if \(currentResult\.ok\) \{/);
  assert.match(
    serverProbeSource,
    /candidates\.filter\(\(host\) => originFor\(host\) !== currentOrigin\)/,
  );
  assert.doesNotMatch(serverProbeSource, /hostWithoutPort/);
});

test("server probing stops after a healthy current origin", async () => {
  const probedUrls = [];
  const sandbox = {
    document: { getElementById() { return null; } },
    window: {
      location: { origin: "http://example.invalid:11112" },
      WebClxAndroid: {
        probeHost() {
          throw new Error("the synchronous bridge probe must never be called");
        },
      },
    },
    URL,
    console,
    performance,
    AbortController,
    fetch(url) {
      probedUrls.push(String(url));
      return Promise.resolve({});
    },
    setTimeout,
    clearTimeout,
  };
  vm.createContext(sandbox);
  vm.runInContext(serverProbeSource, sandbox);

  const selected = await vm.runInContext(
    'TERMINAL_SERVER_PROBE.findBestServer("test")',
    sandbox,
  );

  assert.equal(selected, null);
  assert.equal(probedUrls.length, 1);
  assert.ok(
    probedUrls[0].startsWith("http://example.invalid:11112/favicon.svg?_probe="),
    "the current origin is probed over fetch, not the removed native bridge",
  );
});
