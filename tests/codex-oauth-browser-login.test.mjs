import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const routes = readFileSync(new URL("../src/routes/auth.rs", import.meta.url), "utf8");
const oauthSource = readFileSync(new URL("../src/auth.rs", import.meta.url), "utf8");
const oauthCore = readFileSync(new URL("../crates/auth_core/src/oauth.rs", import.meta.url), "utf8");
const indexHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const accountManagerSource = readFileSync(
  new URL("../static/app-codex-account-manager.js", import.meta.url),
  "utf8",
);
const baseStyles = readFileSync(new URL("../static/styles-base.css", import.meta.url), "utf8");


const authManagerSource = readFileSync(
  new URL("../static/app-auth-manager.js", import.meta.url),
  "utf8",
);

assert.match(routes, /"\/api\/auth\/oauth\/codex\/browser\/start"/);
assert.match(routes, /"\/api\/auth\/oauth\/codex\/browser\/exchange"/);
assert.match(oauthCore, /create_codex_browser_login\(/);
assert.match(oauthCore, /code_challenge_method", "S256"/);
assert.match(oauthCore, /CODEX_BROWSER_TOKEN_EXCHANGE_REDIRECT_URI/);
assert.match(oauthCore, /parse_codex_browser_callback\(/);
assert.match(oauthSource, /complete_codex_browser_login\(/);
assert.match(oauthSource, /CodexOAuthFlow::Browser/);
assert.match(oauthSource, /state 校验失败/);

assert.match(indexHtml, /id="codex-account-login-oauth"[^>]*data-account-kinds="oauth"[^>]*>新登录</);
assert.match(indexHtml, /id="codex-oauth-subtab-login"[^>]*data-account-kinds="oauth"[^>]*>新登录</);
assert.match(indexHtml, /id="auth-detail-login-btn"[^>]*>新登录</);
assert.match(accountManagerSource, /codex-account-login-oauth[\s\S]*?startAuthBrowserOauthSession/);
assert.match(accountManagerSource, /codex-oauth-subtab-login/);
assert.match(accountManagerSource, /auth-detail-login-btn/);

// Requirement 1: 保存当前登录为账号信息，自动提取，已存在则提示更新
assert.match(indexHtml, /id="codex-oauth-subtab-save-current"[^>]*>保存当前登录</);
assert.match(indexHtml, /id="auth-detail-save-current-btn"[^>]*>保存当前登录</);
assert.match(accountManagerSource, /codex-oauth-subtab-save-current/);
assert.match(accountManagerSource, /auth-detail-save-current-btn/);
assert.match(accountManagerSource, /saveCurrentCodexLogin/);
assert.match(accountManagerSource, /window\.confirm\(`账号“\${label}”已存在，是否更新其登录信息？`\)/);

// Requirement 2: 刷新当前登录，重走登录流程，完成后更新登录信息
assert.match(indexHtml, /id="codex-oauth-subtab-refresh-current"[^>]*>刷新当前登录</);
assert.match(indexHtml, /id="auth-detail-refresh-current-btn"[^>]*>刷新当前登录</);
assert.match(accountManagerSource, /codex-oauth-subtab-refresh-current/);
assert.match(accountManagerSource, /auth-detail-refresh-current-btn/);
assert.match(accountManagerSource, /refreshCurrentCodexLogin/);
assert.match(accountManagerSource, /window\.isRefreshingCurrentLogin = true/);
assert.match(authManagerSource, /if \(window\.isRefreshingCurrentLogin\)/);
assert.match(authManagerSource, /PUT/);

// Requirement 3: 默认不直接跳，显示跳转地址，可选打开或复制，点击直接就能复制
assert.match(indexHtml, /id="auth-oauth-url-display"/);
assert.match(indexHtml, /id="auth-oauth-copy-url"[^>]*>复制链接</);
assert.match(indexHtml, /id="auth-oauth-open-url"[^>]*>打开链接</);
const startBrowserMethod = authManagerSource.slice(
  authManagerSource.indexOf("async function startAuthBrowserOauthSession"),
  authManagerSource.indexOf("function parseAuthOauthCallback"),
);
assert.doesNotMatch(startBrowserMethod, /window\.open/);
assert.match(authManagerSource, /copyAuthOauthAuthorizeUrl/);

assert.doesNotMatch(baseStyles, /\.account-type-tabs\s*\{\s*display:\s*none\s*;?\s*\}/);
assert.match(indexHtml, /styles-base\.css\?v=20260926-llm-subtabs/);
assert.match(indexHtml, /app\.js\?v=\w+/);
assert.match(indexHtml, /app-auth-manager\.js\?v=20260926/);
assert.match(indexHtml, /app-feature-managers-init\.js\?v=20260926-browser-oauth/);
assert.match(indexHtml, /app-preset-form-event-bindings\.js\?v=20260926-browser-oauth/);

// Requirement 4: 各智能体子页签支持“重新读取当前账号”，从本地配置重新识别当前生效账号
const codebuddyRoutes = readFileSync(new URL("../src/routes/codebuddy.rs", import.meta.url), "utf8");
const devinSource = readFileSync(new URL("../src/devin.rs", import.meta.url), "utf8");
const agySource = readFileSync(new URL("../src/agy.rs", import.meta.url), "utf8");
const codebuddyManagerSource = readFileSync(new URL("../static/app-codebuddy-manager.js", import.meta.url), "utf8");
const devinManagerSource = readFileSync(new URL("../static/app-devin-manager.js", import.meta.url), "utf8");
const agyManagerSource = readFileSync(new URL("../static/app-agy-manager.js", import.meta.url), "utf8");

// UI buttons
assert.match(indexHtml, /id="codex-account-detect-current"[^>]*>重新读取当前账号</);
assert.match(indexHtml, /id="codex-oauth-subtab-detect-current"[^>]*>重新读取当前账号</);
assert.match(indexHtml, /id="codex-api-subtab-detect-current"[^>]*>重新读取当前账号</);
assert.match(indexHtml, /id="claude-subtab-detect-current"[^>]*>重新读取当前账号</);
assert.match(indexHtml, /id="codebuddy-detect-current"[^>]*>重新读取当前账号</);
assert.match(indexHtml, /id="devin-detect-current"[^>]*>重新读取当前账号</);
assert.match(indexHtml, /id="agy-detect-current"[^>]*>重新读取当前账号</);
assert.match(indexHtml, /id="auth-detail-detect-current-btn"[^>]*>重新读取当前账号</);

// Backend routes
assert.match(routes, /"\/api\/auth\/codex-accounts\/detect-current"/);
assert.match(routes, /"\/api\/auth\/api-presets\/detect-current"/);
assert.match(routes, /"\/api\/auth\/claude-presets\/detect-current"/);
assert.match(codebuddyRoutes, /"\/api\/auth\/codebuddy-accounts\/detect-current"/);
assert.match(devinSource, /"\/api\/auth\/devin-accounts\/detect-current"/);
assert.match(agySource, /"\/api\/agy\/presets\/detect-current"/);

// Frontend bindings
assert.match(accountManagerSource, /detectCurrentForActiveKind/);
assert.match(accountManagerSource, /detectCurrentCodexOAuth/);
assert.match(accountManagerSource, /detectCurrentCodexApi/);
assert.match(accountManagerSource, /detectCurrentClaude/);
assert.match(codebuddyManagerSource, /detectCurrent/);
assert.match(devinManagerSource, /detectCurrent/);
assert.match(agyManagerSource, /detectCurrent/);

// Requirement 5: 相关按钮默认灰色，多选框选中后才可用；第一个登录按钮改名为"新登录"
assert.match(indexHtml, /id="codex-account-login-oauth"[^>]*>新登录</);
assert.match(indexHtml, /id="codex-account-save-current"[^>]*disabled[^>]*>/);
assert.match(indexHtml, /id="codex-account-refresh-current"[^>]*disabled[^>]*>/);
assert.match(indexHtml, /id="codex-account-logout-current"[^>]*disabled[^>]*>/);
assert.match(indexHtml, /id="codex-account-detect-current"[^>]*disabled[^>]*>/);
assert.match(indexHtml, /id="codebuddy-detect-current"[^>]*disabled[^>]*>/);
assert.match(indexHtml, /id="devin-detect-current"[^>]*disabled[^>]*>/);
assert.match(indexHtml, /id="codex-oauth-subtab-save-current"[^>]*disabled[^>]*>/);
assert.match(indexHtml, /id="codex-oauth-subtab-refresh-current"[^>]*disabled[^>]*>/);
assert.match(indexHtml, /id="codex-oauth-subtab-detect-current"[^>]*disabled[^>]*>/);
assert.match(indexHtml, /id="auth-detail-save-current-btn"[^>]*disabled[^>]*>/);
assert.match(indexHtml, /id="auth-detail-refresh-current-btn"[^>]*disabled[^>]*>/);
assert.match(indexHtml, /id="auth-detail-detect-current-btn"[^>]*disabled[^>]*>/);
assert.match(accountManagerSource, /updateSelectionControls[\s\S]*?codex-account-save-current/);
assert.match(accountManagerSource, /updateSelectionControls[\s\S]*?codex-account-refresh-current/);
assert.match(accountManagerSource, /updateSelectionControls[\s\S]*?codex-account-logout-current/);
assert.match(accountManagerSource, /updateSelectionControls[\s\S]*?codex-account-detect-current/);

console.log("codex oauth browser login tests passed");
