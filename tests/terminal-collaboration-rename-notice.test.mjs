import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
const source = readFileSync(new URL('../static/terminal-sessions.js', import.meta.url), 'utf8');
const start = source.indexOf('async function scheduleTerminalCollaborationRenameNotice(session) {');
const end = source.indexOf('function dispatchTerminalGroupNotifications', start);
assert.ok(start >= 0 && end > start);
function harness() {
  const requests = [];
  const context = vm.createContext({
    terminalCollaborationRenameRole: () => '复核者',
    requestJson: async (url, options) => { requests.push({ url, body: JSON.parse(options.body) }); },
  });
  vm.runInContext(source.slice(start, end), context);
  return { requests, notify: context.scheduleTerminalCollaborationRenameNotice };
}
test('普通终端名称带角色后缀也不发送协作消息', async () => {
  const { requests, notify } = harness();
  for (const group_token of [undefined, null, '', '   ']) {
    await notify({ id: 'ordinary', name: 'lyyStock_3_Astra340-复核', group_token });
  }
  await notify(null);
  assert.equal(requests.length, 0);
});
test('群组成员改名保留协作通知', async () => {
  const { requests, notify } = harness();
  await notify({ id: 'grouped', name: 'lyyStock_3_Astra340-复核', group_token: 'team-1' });
  assert.equal(requests.length, 1);
  assert.equal(requests[0].body.target, 'grouped');
  assert.match(requests[0].body.data, /你已改名为/);
});
