import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";
import vm from "node:vm";

const staticRoot = new URL("../static/", import.meta.url);
const runtimePath = new URL("i18n.js", staticRoot);

function read(relativePath) {
  return fs.readFileSync(new URL(relativePath, staticRoot), "utf8");
}

test("all browser entry points load the shared localization runtime first", () => {
  for (const page of ["index.html", "terminal.html", "agent-profiles.html", "login.html"]) {
    const html = read(page);
    assert.match(html, /<script defer src="\/assets\/i18n\.js\?v=[^"]+"><\/script>/);
    assert.ok(
      html.indexOf("/assets/i18n.js") < html.indexOf("/assets/login.js") || !html.includes("/assets/login.js"),
      `${page} should load i18n before its page scripts`,
    );
  }
});

test("localization runtime follows the system language until the user selects a locale", () => {
  const source = fs.readFileSync(runtimePath, "utf8");
  const stored = new Map();
  const context = vm.createContext({
    window: {},
    document: {
      addEventListener() {},
      documentElement: { dataset: {}, lang: "" },
    },
    localStorage: {
      getItem(key) { return stored.get(key) ?? null; },
      setItem(key, value) { stored.set(key, value); },
    },
    navigator: { language: "en-US", languages: ["zh-CN"] },
    MutationObserver: class { observe() {} },
    CustomEvent: class { constructor(type, init) { this.type = type; this.detail = init?.detail; } },
  });
  context.window = context;
  context.window.dispatchEvent = () => {};
  vm.runInContext(source, context);

  assert.deepEqual(Array.from(context.webclxI18n.supportedLocales), ["zh-CN", "en"]);
  assert.equal(context.webclxI18n.getLocale(), "en");
  assert.equal(context.webclxI18n.translate("设置"), "Settings");
  context.webclxI18n.setLocale("zh-CN");
  assert.equal(stored.get("webclx:locale"), "zh-CN");
  assert.equal(context.document.documentElement.lang, "zh-CN");
});

test("stored locale overrides the system language", () => {
  const source = fs.readFileSync(runtimePath, "utf8");
  const context = vm.createContext({
    window: {},
    document: {
      addEventListener() {},
      documentElement: { dataset: {}, lang: "" },
    },
    localStorage: {
      getItem() { return "zh-CN"; },
      setItem() {},
    },
    navigator: { language: "en-US", languages: ["en-US"] },
    MutationObserver: class { observe() {} },
    CustomEvent: class { constructor(type, init) { this.type = type; this.detail = init?.detail; } },
  });
  context.window = context;
  context.window.dispatchEvent = () => {};
  vm.runInContext(source, context);

  assert.equal(context.webclxI18n.getLocale(), "zh-CN");
});

test("runtime translates navigation, terminal messaging, Agent, settings, and login copy", () => {
  const source = fs.readFileSync(runtimePath, "utf8");
  for (const [zh, en] of [
    ["工作区", "Workspace"],
    ["终端管理", "Terminals"],
    ["发送消息", "Send message"],
    ["新建会话", "New session"],
    ["设置", "Settings"],
    ["账号", "Username"],
    ["密码", "Password"],
    ["登录", "Sign in"],
    ["登录中…", "Signing in..."],
    ["登录请求过多，请稍后重试", "Too many sign-in requests. Try again shortly."],
    ["网络错误，请稍后重试", "Network error. Try again shortly."],
    ["新 Agent 会话", "New Agent session"],
  ]) {
    assert.match(source, new RegExp(`${JSON.stringify(zh)}\\s*:\\s*${JSON.stringify(en)}`));
  }
  assert.match(source, /MutationObserver/);
  assert.match(source, /aria-label/);
  assert.match(source, /placeholder/);
  assert.match(source, /webclx-language-select/);
  assert.match(source, /browser-topbar-spacer/);
  assert.match(source, /replaceWith\(label\)/);
  assert.match(read("login.js"), /webclxI18n\?\.translate/);
});
