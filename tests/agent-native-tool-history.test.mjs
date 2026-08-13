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

const functionSource = extractFunction(agentHtml, "renderStoredMessages");
const sandbox = {};
vm.runInNewContext(`${functionSource}; this.renderStoredMessages = renderStoredMessages;`, sandbox);

const rendered = [];
const html = sandbox.renderStoredMessages(
  [
    { role: "user", content: "run it" },
    {
      role: "assistant",
      tool_calls: [
        { id: "call-ok", function: { name: "read_skill", arguments: "{}" } },
        { id: "call-error", function: { name: "run_command", arguments: "{}" } },
      ],
    },
    { role: "tool", tool_call_id: "call-ok", content: { path: "/tmp/skill" } },
    { role: "tool", tool_call_id: "call-error", content: { error: "failed" } },
    { role: "assistant", content: "done" },
  ],
  (message, toolResults) => {
    rendered.push({
      role: message.role,
      ok: toolResults.get("call-ok"),
      error: toolResults.get("call-error"),
    });
    return `[${message.role}]`;
  },
);

assert.equal(html, "[user][assistant][assistant]");
assert.deepEqual(rendered.map((item) => item.role), ["user", "assistant", "assistant"]);
assert.equal(rendered[1].ok.is_error, false);
assert.equal(rendered[1].ok.result.path, "/tmp/skill");
assert.equal(rendered[1].error.is_error, true);
assert.equal(rendered[1].error.result.error, "failed");

console.log("native Agent stored tool history checks passed");
