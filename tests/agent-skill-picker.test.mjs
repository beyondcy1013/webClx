import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const agentHtml = readFileSync(new URL("../static/agent.html", import.meta.url), "utf8");

function extractFunction(source, name) {
  const start = source.indexOf(`function ${name}(`);
  assert.notEqual(start, -1, `missing ${name}`);
  const bodyStart = source.indexOf("{", start);
  let depth = 0;
  for (let index = bodyStart; index < source.length; index += 1) {
    if (source[index] === "{") depth += 1;
    if (source[index] === "}") depth -= 1;
    if (depth === 0) return source.slice(start, index + 1);
  }
  throw new Error(`unterminated ${name}`);
}

assert.match(agentHtml, /id="agent-skill-button"[^>]*aria-label="选择 Skill"/);
assert.match(agentHtml, /id="agent-skill-picker"[^>]*role="listbox"/);
assert.match(agentHtml, /apiJson\("\/api\/agent\/skills"\)/);

const functionNames = [
  "findSkillTrigger",
  "skillMatchScore",
  "rankSkills",
  "insertSkillReference",
  "isCompletedSkillReference",
];
const functionSource = functionNames.map((name) => extractFunction(agentHtml, name)).join("\n");
const sandbox = {};
vm.runInNewContext(
  `${functionSource}; ${functionNames.map((name) => `this.${name} = ${name};`).join("\n")}`,
  sandbox,
);

const skills = [
  { name: "terminal-message", description: "Send terminal notifications", disabled: false },
  { name: "mihomo-proxy-ops", description: "Operate Clash Meta proxies and VPN routes", disabled: false },
  { name: "webclx-codex-api-terminal-ops", description: "Work on Agent presets and terminal UI", disabled: false },
  { name: "disabled-skill", description: "Unavailable", disabled: true },
];

assert.deepEqual(
  Array.from(sandbox.rankSkills(skills, "")).map((skill) => skill.name),
  ["mihomo-proxy-ops", "terminal-message", "webclx-codex-api-terminal-ops"],
);
assert.equal(sandbox.rankSkills(skills, "proxy")[0].name, "mihomo-proxy-ops");
assert.equal(sandbox.rankSkills(skills, "agent preset")[0].name, "webclx-codex-api-terminal-ops");
assert.equal(sandbox.rankSkills(skills, "mhproxy")[0].name, "mihomo-proxy-ops");
assert.equal(sandbox.rankSkills(skills, "mihomo / proxy_ops")[0].name, "mihomo-proxy-ops");
assert.equal(sandbox.rankSkills(skills, "web clx_codex")[0].name, "webclx-codex-api-terminal-ops");

const source = "请用 $term 检查终端";
const cursor = source.indexOf(" 检查");
const trigger = sandbox.findSkillTrigger(source, cursor);
assert.deepEqual(JSON.parse(JSON.stringify(trigger)), { start: 3, end: 8, query: "term" });
assert.deepEqual(
  JSON.parse(JSON.stringify(sandbox.insertSkillReference(source, trigger, "terminal-message"))),
  { value: "请用 $terminal-message 检查终端", cursor: 21 },
);
assert.deepEqual(
  JSON.parse(JSON.stringify(sandbox.findSkillTrigger("请用 $web clx_codex", 17))),
  { start: 3, end: 17, query: "web clx_codex" },
);
assert.equal(sandbox.findSkillTrigger("price$term", 10), null);
assert.equal(sandbox.isCompletedSkillReference("terminal-message 请汇报", skills), true);
assert.equal(sandbox.isCompletedSkillReference("terminal message", skills), false);

console.log("native Agent Skill picker checks passed");
