/**
 * 增强导入测试
 * 验证从混合文本（终端日志、聊天记录、Markdown 代码块）中提取 auth JSON 的能力。
 * 对应功能：app-auth-manager.js extractAuthJsonSources / parseAuthSources
 */
import assert from "node:assert/strict";
import vm from "node:vm";
import { readFileSync } from "node:fs";

const authManagerSource = readFileSync(
  new URL("../static/app-auth-manager.js", import.meta.url),
  "utf8",
);
const context = {
  TextDecoder,
  Uint8Array,
  window: {
    atob(value) {
      return Buffer.from(value, "base64").toString("binary");
    },
  },
};
vm.runInNewContext(authManagerSource, context);
const authManager = context.WebClxAuthManager.create({
  firstFiniteNumber: (...values) => values.find(Number.isFinite) ?? null,
  elements: {},
});

// ── 1. extractAuthJsonSources：纯 JSON 文本直接返回自身 ──────────────────────
{
  const plain = JSON.stringify({ access_token: "tok", account_id: "acc" });
  const result = authManager.extractAuthJsonSources(plain);
  assert.equal(result.length, 1, "纯 JSON 应返回 1 个片段");
  assert.equal(result[0], plain.trim());
}

// ── 2. extractAuthJsonSources：从混合文字中提取多段 JSON ─────────────────────
{
  const json1 = JSON.stringify({ access_token: "tok1", account_id: "acc1" });
  const json2 = JSON.stringify({ access_token: "tok2", account_id: "acc2" });
  const mixed = `Some terminal log line\n${json1}\nMore prose here\n\`\`\`json\n${json2}\n\`\`\`\nEnd of log`;
  const result = authManager.extractAuthJsonSources(mixed);
  assert.ok(result.length >= 2, `应从混合文本中提取至少 2 个 JSON 片段，实际: ${result.length}`);
}

// ── 3. parseAuthSources：严格 JSON 路径不受影响 ──────────────────────────────
{
  const strictJson = JSON.stringify({
    access_token: "strict-tok",
    account_id: "strict-acc",
  });
  const sources = authManager.parseAuthSources(strictJson);
  assert.equal(sources.length, 1);
  // mergeAuthSource 会展开，account_id 应存在
  assert.ok(sources[0].account_id === "strict-acc" || sources[0].access_token === "strict-tok",
    "严格路径：应能解析 access_token / account_id");
}

// ── 4. parseAuthSources：从混合终端日志中提取账号 ────────────────────────────
// 模拟用户粘贴的长篇终端协作日志，其中嵌入一个可识别的 auth JSON
const embeddedAuth = {
  access_token: "embedded-access-token-12345",
  account_id: "embedded-account-id-6789",
  email: "embedded@example.com",
};
const terminalLog = `
回调。

  done 1:57 AM

› [scheduled:paste-1790013590834-7181] [from stockScreener_1] 本端 s6226 已接用户授权
  刚核对 stockScreener 工作树由多项 dirty 变为 clean，我们暂仅只读核查。

• Ran python3 /home/user/.codex/skills/terminal-notifications/scripts/send_terminal_message.py
  └ {"accepted": true, "submitted": false, "task_id": "paste-1790013609837-7183"}

已告知日线修复终端。

  done 2:00 AM

导入账号如下：
${JSON.stringify(embeddedAuth)}

› [from webClx-compile-api] stockScreener 部署完成；请求 015658-18d7600fe8b01aa5
`;

{
  const sources = authManager.parseAuthSources(terminalLog);
  assert.ok(sources.length >= 1, `应从终端日志中提取至少 1 个账号，实际: ${sources.length}`);
  const found = sources.find(
    (s) => s.access_token === embeddedAuth.access_token ||
           s.account_id === embeddedAuth.account_id
  );
  assert.ok(found, "应从终端日志中找到嵌入的 access_token / account_id");
}

// ── 5. normalizeAuthInputs：混合文本也能批量标准化 ───────────────────────────
{
  const mixedText = `Some prose\n${JSON.stringify(embeddedAuth)}\nMore text`;
  const results = authManager.normalizeAuthInputs(mixedText);
  assert.ok(results.length >= 1, "混合文本批量标准化应返回至少 1 条结果");
}

// ── 6. 无 JSON 的纯文字应抛出明确错误 ───────────────────────────────────────
{
  assert.throws(
    () => authManager.parseAuthSources("这是纯文字，没有任何 JSON"),
    (err) => {
      assert.ok(
        err.message.includes("JSON") || err.message.includes("片段"),
        `错误消息应提示 JSON 相关问题，实际: ${err.message}`
      );
      return true;
    }
  );
}

console.log("✓ 所有增强导入测试通过");
