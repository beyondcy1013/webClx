import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

const proxyManager = fs.readFileSync(
  new URL("../static/app-proxy-manager.js", import.meta.url),
  "utf8",
);
const app = fs.readFileSync(new URL("../static/app.js", import.meta.url), "utf8");
const index = fs.readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const authTests = fs.readFileSync(
  new URL("../src/auth/preset_tests.rs", import.meta.url),
  "utf8",
);

test("proxy preset form supports username and write-only password", () => {
  assert.match(index, /id="proxy-username-input"/);
  assert.match(index, /id="proxy-password-input"[^>]*type="password"/);
  assert.match(app, /getElementById\("proxy-username-input"\)/);
  assert.match(app, /getElementById\("proxy-password-input"\)/);
  assert.match(proxyManager, /username:\s*proxyUsernameInputEl\.value\.trim\(\)/);
  assert.match(proxyManager, /password:\s*proxyPasswordInputEl\.value/);
});

test("OAuth preset tests require and report the active application proxy", () => {
  assert.match(authTests, /require_active_oauth_test_proxy/);
  assert.match(authTests, /测试网络：应用代理/);
  assert.match(authTests, /build_auth_client/);
});
