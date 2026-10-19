import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import vm from "node:vm";

const managerSource = readFileSync(
  new URL("../static/app-agy-manager.js", import.meta.url),
  "utf8",
);

const context = {
  window: {},
  document: { getElementById: () => null },
  localStorage: { getItem: () => null, setItem: () => {} },
  CSS: { escape: (value) => String(value) },
  navigator: {},
  fetch: () => Promise.resolve({ ok: true, json: () => Promise.resolve([]) }),
  Blob: class {},
  URL: { createObjectURL: () => "", revokeObjectURL: () => {} },
};
vm.runInNewContext(managerSource, context);
const { parseImportText } = context.window.agyManager;
assert.equal(typeof parseImportText, "function", "parseImportText 应暴露在 window.agyManager 上供测试");

const deliveryText = `buyer.one@gmail.com----pw-real-1----xxxx xxxx xxxx xxxx xxxx xxxx xxxx xxxx 订阅成功 2026-09-21 21:13:20 账号凭证获取成功 {"client_id": "000000000000-testclientid.apps.googleusercontent.com", "client_secret": "GOCSPX-test", "token": "ya29.test-access", "refresh_token": "1//06flat-refresh", "scopes": ["https://www.googleapis.com/auth/cloud-platform"], "token_uri": "https://oauth2.googleapis.com/token", "project_id": "test-project", "expiry": "2026-09-21T14:13:56.509535+00:00"} 反重力验证成功 12345|https://sms.example.invalid/record?key=test-key`;

const mixed = parseImportText(deliveryText);
assert.equal(mixed.length, 1);
assert.equal(mixed[0].email, "buyer.one@gmail.com");
assert.equal(mixed[0].refresh_token, "1//06flat-refresh");
assert.equal(mixed[0].token, "ya29.test-access");
assert.equal(mixed[0].project_id, "test-project");

// 无 email 的凭证 JSON 单独出现时按原样透传，由后端按条报"缺少有效 email"
const noEmail = parseImportText('{"refresh_token": "1//no-email-refresh"}');
assert.equal(noEmail.length, 1);
assert.equal(noEmail[0].refresh_token, "1//no-email-refresh");
assert.equal(noEmail[0].email, undefined);

// 多账号投递：email 行与后续 JSON 按出现顺序配对
const multiText = [
  "a.one@gmail.com----pw1----code1 账号凭证获取成功 {\"refresh_token\": \"1//refresh-a\"}",
  "b.two@gmail.com----pw2----code2 账号凭证获取成功 {\"refresh_token\": \"1//refresh-b\"}",
].join("\n");
const multi = parseImportText(multiText);
assert.equal(multi.length, 2);
assert.equal(multi[0].email, "a.one@gmail.com");
assert.equal(multi[0].refresh_token, "1//refresh-a");
assert.equal(multi[1].email, "b.two@gmail.com");
assert.equal(multi[1].refresh_token, "1//refresh-b");

// 纯 JSON 数组的老用法保持不变
const legacy = parseImportText('[{"email":"legacy@gmail.com","refresh_token":"1//legacy"}]');
assert.equal(legacy.length, 1);
assert.equal(legacy[0].refresh_token, "1//legacy");
