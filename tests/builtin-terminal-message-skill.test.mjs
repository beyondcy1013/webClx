import assert from "node:assert/strict";
import fs from "node:fs";
import test from "node:test";

function read(relativePath) {
  return fs.readFileSync(new URL(`../${relativePath}`, import.meta.url), "utf8");
}

test("terminal messaging ships as a concise portable built-in Skill", () => {
  const manifest = read("builtin-skills/webclx-terminal-message/SKILL.md");
  const script = read("builtin-skills/webclx-terminal-message/scripts/send_terminal_message.py");
  const metadata = read("builtin-skills/webclx-terminal-message/agents/openai.yaml");

  assert.match(manifest, /^---\nname: webclx-terminal-message\ndescription: .+\n---/);
  assert.match(manifest, /Codex/);
  assert.match(manifest, /Claude/);
  assert.match(manifest, /DeepSeek Harness/);
  assert.match(manifest, /scripts\/send_terminal_message\.py/);
  assert.doesNotMatch(manifest, /\/home\/root|\/home\/codes\/webClx/);
  assert.match(script, /\/api\/terminal\/sessions\/message/);
  assert.match(script, /verify_submission/);
  assert.match(script, /delivery_id/);
  assert.match(script, /WEBCLX_LOCAL_TOKEN_FILE/);
  assert.match(script, /X-WebClx-Local-Token/);
  assert.match(script, /if not is_loopback\(base_url\)/);
  assert.match(metadata, /display_name:/);
});

test("the server embeds, discovers, and installs managed built-in Skills", () => {
  const moduleSource = read("src/builtin_skills.rs");
  const mainSource = read("src/main.rs");
  const agentSource = read("src/agent.rs");
  const activitySource = read("src/terminal/activity.rs");

  assert.match(moduleSource, /include_dir!\("\$CARGO_MANIFEST_DIR\/builtin-skills"\)/);
  assert.match(moduleSource, /install_for_user/);
  assert.match(moduleSource, /\.codex/);
  assert.match(moduleSource, /\.claude/);
  assert.match(moduleSource, /\.dsh/);
  assert.match(moduleSource, /webclx-managed-skill/);
  assert.match(mainSource, /mod builtin_skills;/);
  assert.match(mainSource, /builtin_skills::install_for_user/);
  assert.match(agentSource, /builtin_skills::root_dir/);
  assert.match(agentSource, /"builtin"/);
  assert.match(activitySource, /is_deepseek_process/);
  assert.match(activitySource, /agents\.push\("DeepSeek"\)/);
});
