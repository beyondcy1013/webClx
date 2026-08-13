import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import vm from "node:vm";

const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const serverSwitchSource = readFileSync(
  new URL("../static/terminal-server-switch.js", import.meta.url),
  "utf8",
);
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
  for (const target of [
    "fpsq.xyz",
    "frp6.ccszxc.site:25401",
    "fpsq.xyz:11112",
    "fpsq.xyz:14151",
    "us.fpsq.xyz",
    "jd.fpsq.xyz",
    "192.168.3.38",
  ]) {
    assert.match(terminalHtml, new RegExp(`value="${target.replaceAll(".", "\\.")}"`));
  }
  assert.doesNotMatch(terminalHtml, /fpsq\.xyz:10002/);
  assert.match(
    terminalHtml,
    /value="fpsq\.xyz">fpsq\.xyz:11111<\/option>\s*<option value="frp6\.ccszxc\.site:25401">frp6\.ccszxc\.site:25401<\/option>/,
  );
  assert.match(terminalHtml, /terminal-server-switch\.js\?v=/);
  assert.match(
    terminalMobileKeysSource,
    /if \(action === "switch_server"\) \{\s*openTerminalServerSwitchDialog\(\);\s*\} else if \(action === "deploy_project"\)/,
  );
});

test("project management and terminal tools use compact custom menus", () => {
  assert.match(terminalHtml, /id="terminal-project-command-select"[\s\S]*?hidden/);
  assert.match(terminalStyles, /\.terminal-project-command-menu \{[^}]*width: min\(176px,/);
  assert.match(terminalStyles, /\.terminal-project-command-menu > button \{[^}]*min-height: 28px;[^}]*font: 600 12px\/1/);
  assert.match(terminalStyles, /\.terminal-tools-menu-with-tools \{[^}]*width: min\(176px,/);
  assert.match(
    terminalStyles,
    /\.terminal-tools-menu-with-tools \.terminal-tools-quick-commands \{[^}]*grid-template-columns: minmax\(0, 1fr\);/,
  );
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

  assert.equal(sandbox.normalizeTerminalServerTarget("fpsq.xyz"), "http://fpsq.xyz:11111");
  assert.equal(
    sandbox.normalizeTerminalServerTarget("fpsq.xyz:11112"),
    "http://fpsq.xyz:11112",
  );
  assert.equal(
    sandbox.buildTerminalServerSwitchUrl("192.168.3.38", sandbox.window.location),
    "http://192.168.3.38:11111/terminal?path=webClx#session",
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

  const expected = "http://fpsq.xyz:14151/terminal?path=webClx#session";
  assert.equal(
    sandbox.navigateToTerminalServer("fpsq.xyz:14151", location, {
      openInWebView(url) { openedInWebView.push(url); },
    }),
    expected,
  );
  assert.deepEqual(openedInWebView, [expected]);
  assert.deepEqual(assigned, []);

  sandbox.navigateToTerminalServer("fpsq.xyz:14151", location, null);
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
      location: { origin: "http://fpsq.xyz:11112" },
      WebClxAndroid: {
        probeHost(url) {
          probedUrls.push(url);
          return JSON.stringify({ ok: true, latency: 12 });
        },
      },
    },
    URL,
    console,
    performance,
    AbortController,
    fetch,
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
  assert.deepEqual(probedUrls, ["http://fpsq.xyz:11112/favicon.svg"]);
});
