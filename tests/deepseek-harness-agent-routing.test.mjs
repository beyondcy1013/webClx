import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const routes = readFileSync(new URL("../src/routes/agent.rs", import.meta.url), "utf8");
const agent = readFileSync(new URL("../src/agent.rs", import.meta.url), "utf8");
const appHtml = readFileSync(new URL("../static/index.html", import.meta.url), "utf8");
const agentHtml = readFileSync(new URL("../static/agent.html", import.meta.url), "utf8");
const pageRoutes = readFileSync(new URL("../src/routes/pages.rs", import.meta.url), "utf8");
const login = readFileSync(new URL("../src/login.rs", import.meta.url), "utf8");

assert.match(
  routes,
  /\.route\("\/agent", get\(agent::agent_page\)\)/,
  "/agent should retain a webClx shell around DeepSeek Harness",
);
assert.match(
  routes,
  /\.route\("\/agent\/runtime", get\(agent::deepseek_harness_redirect\)\)/,
  "the embedded runtime should resolve through the configured Harness redirect",
);
assert.match(
  routes,
  /\.route\("\/agent\/profiles", get\(agent::agent_profiles_page\)\)/,
  "terminal Agent profiles should remain available as a webClx control-plane page",
);

for (const retiredRoute of [
  "/api/agent/sessions",
  "/api/agent/tools",
  "/api/agent/models",
  "/api/agent/config",
  "/api/agent/skills/toggle",
  "/api/agent/skill-dirs",
]) {
  assert.doesNotMatch(
    routes,
    new RegExp(`\\.route\\(\\s*"${retiredRoute.replaceAll("/", "\\/")}`),
    `${retiredRoute} should not remain registered after Harness takes ownership`,
  );
}

for (const retainedRoute of [
  "/api/agent/terminal-profiles",
  "/api/agent/terminal-profiles/{profile_id}",
  "/api/agent/api-presets",
  "/api/agent/exec-with-preset",
  "/api/agent/skills",
]) {
  assert.match(
    routes,
    new RegExp(retainedRoute.replaceAll("/", "\\/")),
    `${retainedRoute} should remain available to the webClx control plane`,
  );
}

assert.match(agent, /WEBCLX_DEEPSEEK_HARNESS_URL/);
assert.match(agent, /HeaderMap/);
assert.match(agent, /TEMPORARY_REDIRECT/);
assert.match(agent, /LOCATION/);
assert.match(agent, /pub async fn agent_page[\s\S]*agent\.html/);
assert.match(
  agent,
  /pub async fn agent_profiles_page[\s\S]*agent-profiles\.html/,
  "the retained profile page should use a dedicated static document",
);
assert.match(agentHtml, /<iframe[^>]+src="\/agent\/runtime"/);
assert.match(agentHtml, /href="\/agent\/profiles"/);
assert.match(agentHtml, /href="\/agent\/runtime"[^>]+target="_blank"/);
assert.doesNotMatch(
  agentHtml,
  /\.agent-runtime-nav\s*\{\s*display:\s*none/,
  "mobile Agent pages must retain webClx tab navigation",
);
assert.match(
  agentHtml,
  /\.agent-runtime-page\s*\{[^}]*height:\s*100vh;[^}]*height:\s*100dvh;/,
  "desktop browsers without dynamic viewport units need a 100vh height fallback",
);
assert.match(pageRoutes, /"\/api\/auth\/proxy-check", get\(login::proxy_auth_check\)/);
assert.match(
  login,
  /pub async fn proxy_auth_check[\s\S]*verify_session_from_headers[\s\S]*StatusCode::NO_CONTENT[\s\S]*StatusCode::UNAUTHORIZED/,
  "the Harness proxy check must explicitly validate the signed webClx cookie",
);

assert.doesNotMatch(
  appHtml,
  /app-agent-settings\.js|agent-default-model-input|agent-system-prompt-input|agent-skill-dirs-list|agent-skills-list/,
  "webClx settings should not retain controls for the retired native Agent runtime",
);
assert.match(
  appHtml,
  /settings-panel-agent[\s\S]*href="\/agent"[\s\S]*DeepSeek Harness[\s\S]*href="\/agent\/profiles"[\s\S]*终端智能体配置/,
  "AI settings should link to Harness and retained terminal profile management",
);

console.log("DeepSeek Harness Agent routing contract passed");
