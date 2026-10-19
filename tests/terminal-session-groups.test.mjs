import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { test } from 'node:test';
const require = createRequire(import.meta.url);
const activity = require('../static/terminal-session-activity.js');
const { sortSessionsByRecentActivity: sort, sortTerminalSessions } = activity;
const ids = (rows) => rows.map(row => row.id);
const fixture = () => [
  { id: 's1', last_opened_at: 20 },
  { id: 's2', last_opened_at: 10, group_token: 'a', group_order: 1 },
  { id: 's3', last_opened_at: 5, group_token: 'a', group_order: 2 },
  { id: 's4', last_opened_at: 30, group_token: 'b', group_order: 1 },
  { id: 's5', last_opened_at: 1, group_token: 'b', group_order: 2 },
];
test('activation moves either partner with stable internal order and preserves unrelated ranking', () => {
  const rows = fixture();
  assert.deepEqual(ids(sort(rows)), ['s4', 's5', 's1', 's2', 's3']);
  rows[1].last_opened_at = 100;
  assert.deepEqual(ids(sort(rows)), ['s2', 's3', 's4', 's5', 's1']);
  rows[2].last_opened_at = 200;
  assert.deepEqual(ids(sort(rows.reverse())), ['s2', 's3', 's4', 's5', 's1']);
  rows.find(s => s.id === 's1').last_opened_at = 300;
  assert.deepEqual(ids(sort(rows)), ['s1', 's2', 's3', 's4', 's5']);
});
test('ties are deterministic; filtering never resurrects absent group members', () => {
  const rows = fixture().map(row => ({ ...row, last_opened_at: 1, created_at: 1 }));
  assert.deepEqual(ids(sort(rows)), ids(sort([...rows].reverse())));
  assert.deepEqual(ids(sort(rows.filter(row => row.id !== 's2'))), ['s4', 's5', 's3', 's1']);
  assert.deepEqual(sort([]), []);
});
test('group colors are stable and membership count cannot reassign them', () => {
  assert.equal(activity.terminalGroupColor({ id: 's9' }), '');
  assert.equal(activity.terminalGroupColor({ id: 's1', group_token: 'alpha' }, []), '');
  assert.equal(activity.normalizeTerminalGroupColor('#ABCDEF'), '#abcdef');
  assert.equal(activity.normalizeTerminalGroupColor('red'), '');
  const single = [
    { id: 's1', group_token: 'alpha' },
    { id: 's2', group_token: 'alpha' },
  ];
  const solo = activity.terminalGroupColor(single[0], single);
  assert.equal(solo, activity.terminalGroupColor(single[1], single));
  assert.match(solo, /^hsl\(\d+ 76% 52%\)$/);
  const paired = [
    { id: 's1', group_token: 'alpha' },
    { id: 's2', group_token: 'alpha' },
    { id: 's3', group_token: 'beta' },
    { id: 's4', group_token: 'beta' },
  ];
  const alpha = activity.terminalGroupColor(paired[0], paired);
  const beta = activity.terminalGroupColor(paired[2], paired);
  assert.equal(alpha, activity.terminalGroupColor(paired[1], paired));
  assert.equal(beta, activity.terminalGroupColor(paired[3], paired));
  assert.notEqual(alpha, beta);
  const stablePaired = [
    { id: 's1', group_token: 'alpha', group_color: '#ff5500' },
    { id: 's2', group_token: 'alpha', group_color: '#ff5500' },
    { id: 's3', group_token: 'beta' },
    { id: 's4', group_token: 'beta' },
  ];
  assert.equal(
    activity.terminalGroupColor(stablePaired[0], stablePaired),
    activity.terminalGroupColor(stablePaired[0], [stablePaired[0], stablePaired[1], stablePaired[2], stablePaired[3]]),
  );
  assert.equal(activity.terminalGroupColor(stablePaired[0], stablePaired), '#ff5500');
  const triple = paired.concat([
    { id: 's5', group_token: 'gamma' },
    { id: 's6', group_token: 'gamma' },
  ]);
  const hues = ['alpha', 'beta', 'gamma'].map((token) =>
    Number(activity.terminalGroupColor({ id: 'x', group_token: token }, triple).match(/hsl\((\d+)/)[1]));
  assert.equal(new Set(hues).size, 3);
});

test('manual group_order controls internal order and activation only moves the whole block', () => {
  const rows = [
    { id: 's1', last_opened_at: 30, group_token: 'g', group_order: 2 },
    { id: 's2', last_opened_at: 10, group_token: 'g', group_order: 3 },
    { id: 's3', last_opened_at: 5, group_token: 'g', group_order: 1 },
    { id: 's4', last_opened_at: 20 },
  ];
  assert.deepEqual(ids(sort(rows)), ['s3', 's1', 's2', 's4']);
  rows[0].last_opened_at = 100;
  assert.deepEqual(ids(sort(rows)), ['s3', 's1', 's2', 's4']);
});

test('explicit sort modes keep groups adjacent without discarding their sorting semantics', () => {
  const rows = fixture().map((row, index) => ({ ...row, activity_agent: index < 3 ? 'Z' : 'A' }));
  for (const mode of ['workspace', 'agent', 'status']) {
    const order = ids(sortTerminalSessions(rows, mode));
    assert.equal(order.indexOf('s3'), order.indexOf('s2') + 1);
    assert.equal(order.indexOf('s5'), order.indexOf('s4') + 1);
  }
  assert.deepEqual(ids(sortTerminalSessions(rows, 'agent')).slice(0, 2), ['s4', 's5']);
});

test('group changes refresh other connected terminal lists', () => {
  const storage = require('../static/terminal-session-storage.js');
  assert.equal(storage.shouldRefreshForSessionMutation('group_updated'), true);
});

const message = require('../static/terminal-dialogs.js');
const groupFixture = () => [
  { id: 's1', name: '协作 A', group_token: 'a', group_name: '提审组', group_color: '#ff5500' },
  { id: 's2', name: '协作 B', group_token: 'a', group_name: '提审组', group_color: '#ff5500' },
  { id: 's3', name: '独狼', group_token: '' },
  { id: 's4', name: '值班 A', group_token: 'b' },
  { id: 's5', name: '值班 B', group_token: 'b' },
];

test('named groups are derived from session membership and keep their own name', () => {
  const groups = message.terminalMessageGroups(groupFixture());
  assert.deepEqual(groups.map((group) => group.token), ['a', 'b']);
  assert.equal(message.terminalMessageGroupLabel(groups[0]), '提审组');
  // 未命名分组退回成员名，不会凭 token 编造名字。
  assert.equal(message.terminalMessageGroupLabel(groups[1]), '值班 A、值班 B');
  assert.deepEqual(message.terminalMessageGroupMemberIds(groups[0]), ['s1', 's2']);
});

test('a single remaining member is not a group and an ungrouped terminal carries no badge', () => {
  const rows = [{ id: 's1', name: '协作 A', group_token: 'a', group_name: '提审组' }];
  assert.deepEqual(message.terminalMessageGroups(rows), []);
  const targets = message.terminalMessageTargets(rows);
  assert.equal(targets[0].groupLabel, '');
});

test('targets expose the terminal name and the group they belong to', () => {
  const targets = message.terminalMessageTargets(groupFixture());
  const row = targets.find((item) => item.id === 's2');
  assert.equal(row.name, '协作 B');
  assert.equal(row.groupLabel, '提审组');
  assert.equal(row.groupColor, '#ff5500');
  assert.equal(targets.find((item) => item.id === 's3').groupLabel, '');
});

test('selection keeps group order, rejects empty selection and caps batch size', () => {
  const targets = message.terminalMessageTargets(groupFixture());
  const selected = message.terminalMessageRecipients(targets, ['s2', 's1', 's1']);
  assert.deepEqual(selected.recipients.map((item) => item.id), ['s1', 's2']);
  assert.equal(selected.error, '');
  assert.equal(message.terminalMessageRecipients(targets, []).error, '请至少选择一个目标终端。');
  const many = Array.from({ length: 40 }, (_, index) => `s${index}`);
  assert.equal(message.terminalMessageRecipients(many.map((id) => ({ id, name: id })), many).recipients.length,
    message.TERMINAL_MESSAGE_MAX_TARGETS);
});

test('verified sends require delivery ids and keeping literally what the user typed', () => {
  const verified = message.terminalMessageBody({
    target: 's2',
    message: '请评审 3128',
    deliveryId: 'batch-1:s2',
  });
  assert.deepEqual(verified, {
    target: 's2',
    data: '请评审 3128',
    submit: true,
    submit_enters: 1,
    bracketed_paste: true,
    verify_submission: true,
    delivery_id: 'batch-1:s2',
  });
  const insertOnly = message.terminalMessageBody({
    target: 's2',
    message: '稍等我确认',
    submit: false,
  });
  assert.equal(insertOnly.verify_submission, false);
  assert.equal(insertOnly.submit_enters, 0);
  assert.equal(insertOnly.delivery_id, undefined);
  assert.equal(message.terminalMessageSenderTag('webClx#1'), '[from webClx#1] ');
  assert.equal(message.terminalMessageSenderTag(''), '');
});

test('group save separates membership success from slow notification confirmation', () => {
  const outcomes = [
    { target: 'a', submitted: true },
    { target: 'b', submitted: false, timed_out: true },
    { target: 'c', submitted: false, error: 'network' },
  ];
  const notified = outcomes.filter((item) => item.submitted).length;
  const pending = outcomes.filter((item) => item.timed_out).length;
  const failed = outcomes.filter((item) => !item.submitted && !item.timed_out).length;
  assert.equal(notified, 1);
  assert.equal(pending, 1);
  assert.equal(failed, 1);
});

test('group roles honor the designated manager and map persisted labels to option keys', () => {
  assert.equal(message.terminalMessageGroupRoleKey('管理者'), 'manager');
  assert.equal(message.terminalMessageGroupRoleKey('manager'), 'manager');
  assert.equal(message.terminalMessageGroupRoleKey('复核者'), 'reviewer');
  assert.equal(message.terminalMessageGroupRoleKey('worker'), 'executor');
  assert.equal(message.terminalMessageGroupRoleKey(''), '');
  assert.equal(message.terminalMessageGroupRoleKey('路人'), '');

  // The designated manager wins over member order.
  const designated = message.terminalMessageGroups([
    { id: 's1', name: '执行 A', group_token: 'a', group_role: '执行者' },
    { id: 's2', name: '管理 B', group_token: 'a', group_role: '管理者' },
  ]);
  assert.equal(designated.length, 1);
  assert.equal(designated[0].roles.get('s2'), 'manager');
  assert.equal(designated[0].roles.get('s1'), 'executor');

  // Without any designation the first member keeps acting as manager.
  const fallback = message.terminalMessageGroups([
    { id: 's1', name: '协作 A', group_token: 'a' },
    { id: 's2', name: '协作 B', group_token: 'a' },
  ]);
  assert.equal(fallback[0].roles.get('s1'), 'manager');
  assert.equal(fallback[0].roles.get('s2'), 'executor');

  // Target rows surface the persisted role labels, including reviewer.
  const targets = message.terminalMessageTargets([
    { id: 's1', name: '复核 A', group_token: 'a', group_role: '复核者' },
    { id: 's2', name: '管理 B', group_token: 'a', group_role: '管理者' },
  ]);
  assert.equal(targets.find((row) => row.id === 's1').groupRoleName, '复核者');
  assert.equal(targets.find((row) => row.id === 's2').groupRoleName, '管理者');
});

test('session selector crown honors the designated manager and never follows activation rotation', () => {
  // Terminal-message dropdown rows already honor persisted roles; the session
  // selector must resolve the manager the same way, from the same persisted
  // group_role, instead of treating whichever member holds group_order 1 as
  // the manager (activation rotates that to the current terminal).
  const render = require('../static/terminal-session-render.js');
  assert.equal(render.terminalSelectorGroupRoleKey('管理者'), 'manager');
  assert.equal(render.terminalSelectorGroupRoleKey('执行者'), '');
  assert.equal(render.terminalSelectorGroupRoleKey(''), '');

  // Designated manager wins even when activation rotated the current partner
  // to the head of the group (group_order 1).
  const rotated = [
    { id: 's2', name: '执行 B', group_order: 1, group_role: '执行者' },
    { id: 's1', name: '管理 A', group_order: 2, group_role: '管理者' },
  ];
  assert.equal(render.terminalSelectorGroupManagerId(rotated), 's1');

  // Without any designation the fallback is a stable first member by creation
  // time, so the crown stays put across activations.
  const undesignated = [
    { id: 's2', name: '协作 B', group_order: 1, created_at: 20 },
    { id: 's1', name: '协作 A', group_order: 2, created_at: 10 },
  ];
  assert.equal(render.terminalSelectorGroupManagerId(undesignated), 's1');
  assert.equal(render.terminalSelectorGroupManagerId([]), '');
});

test('delivery reports count only confirmed submissions and name the failures', () => {
  const summary = message.terminalMessageSummary([
    { id: 's1', name: '协作 A', submitted: true },
    { id: 's2', name: '协作 B', submitted: false, error: '提交未获终端确认' },
    { id: 's4', name: '值班 A', error: '终端消息发送失败：agent exited' },
  ]);
  assert.equal(summary.confirmed, 1);
  assert.equal(summary.failed, 2);
  assert.equal(summary.total, 3);
  assert.match(summary.text, /失败：协作 B（提交未获终端确认）/);
  assert.match(summary.text, /值班 A（终端消息发送失败：agent exited）/);
  assert.equal(message.terminalMessageDeliveryState({ submitted: true }), 'confirmed');
  assert.equal(message.terminalMessageDeliveryState({ submitted: false }), 'unconfirmed');
});

test('terminal group name is formatted with 【】 placed before terminal name instead of suffix', () => {
  const fs = require('node:fs');
  const vm = require('node:vm');
  const commonJs = fs.readFileSync(new URL('../static/terminal-common.js', import.meta.url), 'utf8');

  const ctx = {
    state: { showSessionDetails: false, showSessionAgent: false },
    sessionActivityAgentPrefix: () => '',
    sessionActivityPrefix: () => '',
    sessionActivityAgentSuffix: () => '',
    sessionActivityLabel: () => '空闲',
  };
  vm.runInNewContext(commonJs, ctx);

  const sessionWithGroup = { id: 's1', name: '协作终端', group_name: '提审组' };
  assert.equal(ctx.sessionOptionLabel(sessionWithGroup), '【提审组】协作终端');

  const sessionWithoutGroup = { id: 's2', name: '独狼终端' };
  assert.equal(ctx.sessionOptionLabel(sessionWithoutGroup), '独狼终端');
});

