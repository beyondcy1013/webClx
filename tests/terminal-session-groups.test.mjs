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

test('multiple groups in the same project use different background colors even if stored color collides', () => {
  const sameProjectSessions = [
    { id: 's1', path: '/home/codes/lyyStock/stockScreener', group_token: 'group-3', group_color: '#4f8cff' },
    { id: 's2', path: '/home/codes/lyyStock/stockScreener', group_token: 'group-3', group_color: '#4f8cff' },
    { id: 's3', path: '/home/codes/lyyStock/stockScreener', group_token: 'group-4', group_color: '#4f8cff' },
    { id: 's4', path: '/home/codes/lyyStock/stockScreener', group_token: 'group-4', group_color: '#4f8cff' },
  ];
  const colorGroup3 = activity.terminalGroupColor(sameProjectSessions[0], sameProjectSessions);
  const colorGroup4 = activity.terminalGroupColor(sameProjectSessions[2], sameProjectSessions);
  assert.equal(colorGroup3, '#4f8cff');
  assert.notEqual(colorGroup4, colorGroup3);
  assert.match(colorGroup4, /^#[0-9a-fA-F]{6}$/);

  const nextColor = activity.defaultUnusedTerminalGroupColor(
    { path: '/home/codes/lyyStock/stockScreener' },
    sameProjectSessions,
  );
  assert.notEqual(nextColor, '#4f8cff');
  assert.match(nextColor, /^#[0-9a-fA-F]{6}$/);
});

test('defaultUnusedTerminalGroupColor persists obtained colors and avoids collision on subsequent calls', () => {
  activity.clearAllocatedGroupColors();

  // 第一次调用获取默认主色
  const first = activity.defaultUnusedTerminalGroupColor({ id: 's10' }, []);
  assert.equal(first, '#4f8cff');

  // 第二次调用，无需传入任何 sessions，通过已保存的已获取颜色池也能自动避开第一次的颜色
  const second = activity.defaultUnusedTerminalGroupColor({ id: 's11' }, []);
  assert.notEqual(second, first);
  assert.match(second, /^#[0-9a-fA-F]{6}$/);

  // 连续调用 10 次，所有获取到的颜色必须互不相同（全局唯一，绝不重复碰撞）
  const picked = new Set([first, second]);
  for (let i = 0; i < 10; i++) {
    const next = activity.defaultUnusedTerminalGroupColor({ id: `s${20 + i}` }, []);
    assert.ok(!picked.has(next), `Color ${next} was picked more than once!`);
    picked.add(next);
  }
  assert.equal(picked.size, 12);

  // 跨不同项目的会话已知颜色同样被计入避让
  const crossProjectSessions = [
    { id: 's30', path: '/path/projectA', group_token: 'gA', group_color: '#abcdef' },
  ];
  activity.recordAllocatedGroupColor('#fedcba');
  const forProjectB = activity.defaultUnusedTerminalGroupColor({ id: 's31', path: '/path/projectB' }, crossProjectSessions);
  assert.notEqual(forProjectB, '#abcdef');
  assert.notEqual(forProjectB, '#fedcba');
  assert.ok(!picked.has(forProjectB));

  activity.clearAllocatedGroupColors();
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
  assert.equal(
    message.deduplicateConsecutiveSenderTags('[from s33] [from s33] FOLLOWUP'),
    '[from s33] FOLLOWUP',
  );
  assert.equal(
    message.formatTerminalMessageWithSender('s33', '[from s33] FOLLOWUP'),
    '[from s33] FOLLOWUP',
  );
  assert.equal(
    message.formatTerminalMessageWithSender('s33', '[from s33] [from s33] FOLLOWUP'),
    '[from s33] FOLLOWUP',
  );
  assert.equal(
    message.formatTerminalMessageWithSender('s33', 'FOLLOWUP'),
    '[from s33] FOLLOWUP',
  );
  assert.equal(
    message.formatTerminalMessageWithSender('s33', 'FOLLOWUP', false),
    'FOLLOWUP',
  );
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
  assert.equal(message.terminalMessageGroupRoleKey('普通管理员'), 'manager');
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
  assert.equal(targets.find((row) => row.id === 's2').groupRoleName, '普通管理员');
});

test('session selector manager badge honors the designated manager and never follows activation rotation', () => {
  // Terminal-message dropdown rows already honor persisted roles; the session
  // selector must resolve the manager the same way, from the same persisted
  // group_role, instead of treating whichever member holds group_order 1 as
  // the manager (activation rotates that to the current terminal).
  const render = require('../static/terminal-session-render.js');
  assert.equal(render.terminalSelectorGroupRoleKey('管理者'), 'manager');
  assert.equal(render.terminalSelectorGroupRoleKey('普通管理员'), 'manager');
  assert.equal(render.terminalSelectorGroupRoleKey('执行者'), 'executor');
  assert.equal(render.terminalSelectorGroupRoleKey('复核者'), 'reviewer');
  assert.equal(render.terminalSelectorGroupRoleKey('reviewer'), 'reviewer');
  assert.equal(render.terminalSelectorGroupRoleKey(''), '');
  assert.equal(render.terminalSelectorGroupRoleKey('路人'), '');

  // A reviewer keeps its own persisted role instead of collapsing into the
  // executor bucket, which is what made both roles share one dropdown icon.
  assert.equal(render.terminalSelectorGroupMemberRole({ id: 's1', group_role: '复核者' }, 's2'), 'reviewer');
  assert.equal(render.terminalSelectorGroupMemberRole({ id: 's2', group_role: '管理者' }, 's2'), 'manager');
  assert.equal(render.terminalSelectorGroupMemberRole({ id: 's3', group_role: '执行者' }, 's2'), 'executor');
  // Members without a persisted role still fall back to manager/executor.
  assert.equal(render.terminalSelectorGroupMemberRole({ id: 's2' }, 's2'), 'manager');
  assert.equal(render.terminalSelectorGroupMemberRole({ id: 's3' }, 's2'), 'executor');
  assert.equal(render.terminalSelectorGroupRoleLabel('manager'), '普通管理员');
  assert.equal(render.terminalSelectorGroupRoleLabel('reviewer'), '复核者');

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

test('terminal group dialog resolves roles from name suffixes and prevents active terminal role scramble', () => {
  const sessions = require('../static/terminal-sessions.js');
  const render = require('../static/terminal-session-render.js');

  const members = [
    { id: 's1', name: 'stockScreener_26_新复核', group_token: 'group-4', group_order: 1, group_role: '' },
    { id: 's2', name: 'stockScreener_25_管理者', group_token: 'group-4', group_order: 2, group_role: '' },
    { id: 's3', name: 'stockScreener_28_新执行者2', group_token: 'group-4', group_order: 3, group_role: '' },
  ];

  // Even if s1 (新复核) is the active session opening the dialog, it must not become manager;
  // s2 (管理者) must resolve to manager and s1 to reviewer.
  const activeS1 = members[0];
  assert.equal(sessions.resolveTerminalGroupMemberInitialRole(members[0], activeS1, members, 'group-4'), 'reviewer');
  assert.equal(sessions.resolveTerminalGroupMemberInitialRole(members[1], activeS1, members, 'group-4'), 'manager');
  assert.equal(sessions.resolveTerminalGroupMemberInitialRole(members[2], activeS1, members, 'group-4'), 'executor');

  // Selector badge also finds the manager by name when group_role is empty
  assert.equal(render.terminalSelectorGroupManagerId(members), 's2');
  assert.equal(render.terminalSelectorGroupMemberRole(members[0], 's2'), 'reviewer');
  assert.equal(render.terminalSelectorGroupMemberRole(members[1], 's2'), 'manager');
  assert.equal(render.terminalSelectorGroupMemberRole(members[2], 's2'), 'executor');
});

test('terminal super admin round trips across edit, selector and message roles', () => {
  const sessions = require('../static/terminal-sessions.js');
  const render = require('../static/terminal-session-render.js');
  for (const role of ['super_admin', '超级管理员']) {
    assert.equal(sessions.terminalGroupRoleValue(role), 'super_admin');
    assert.equal(sessions.terminalGroupRoleLabel(role), '超级管理员');
    assert.equal(render.terminalSelectorGroupRoleKey(role), 'super_admin');
    assert.equal(message.terminalMessageGroupRoleKey(role), 'super_admin');
  }
  const members = [
    { id: 's1', group_token: 'a', group_role: '普通管理员' },
    { id: 's2', group_token: 'a', group_role: '超级管理员' },
    { id: 's3', group_token: 'a', group_role: '执行者' },
  ];
  assert.equal(sessions.resolveTerminalGroupMemberInitialRole(members[1], members[0], members, 'a'), 'super_admin');
  assert.equal(sessions.terminalCollaborationRenameRole('项目_超级管理员'), '超级管理员');
  assert.equal(render.terminalRoleFromSessionName('项目_超级管理员'), 'super_admin');
  assert.equal(render.terminalSelectorGroupManagerId(members), 's2');
  assert.equal(render.terminalSelectorGroupMemberRole(members[1], 's2'), 'super_admin');
  assert.equal(render.terminalSelectorGroupRoleLabel('super_admin'), '超级管理员');
  assert.equal(message.terminalMessageTargets(members)[1].groupRoleName, '超级管理员');
});

test('super admin leads without forcing an executor into ordinary administration', () => {
  const sessions = require('../static/terminal-sessions.js');
  const members = [{ id: 'a', role: 'super_admin' }, { id: 'b', role: 'executor' }];
  sessions.normalizeTerminalGroupLeadership(members, 'b');
  assert.deepEqual(members.map(m => m.role), ['super_admin', 'executor']);
  members[1].role = 'manager';
  sessions.normalizeTerminalGroupLeadership(members, 'b');
  assert.deepEqual(members.map(m => m.role), ['super_admin', 'manager']);
  members[1].role = 'super_admin';
  sessions.normalizeTerminalGroupLeadership(members, 'b', 'b');
  assert.deepEqual(members.map(m => m.role), ['manager', 'super_admin']);
  members[0].role = 'reviewer';
  members[1].role = 'executor';
  sessions.normalizeTerminalGroupLeadership(members, 'b');
  assert.deepEqual(members.map(m => m.role), ['reviewer', 'manager']);
});

test('team brief preserves ordinary manager and reviewer roles under super admin', () => {
  const brief = message.terminalCollaborationBrief({
    manager: { name: '统筹', role: 'super_admin' },
    executors: [{ name: '日常管理', role: 'manager' }, { name: '检查', role: 'reviewer' }],
    task: '等待明确授权',
  });
  assert.match(brief, /「统筹」（超级管理员）/);
  assert.match(brief, /「日常管理」（普通管理员）/);
  assert.match(brief, /「检查」（复核者）/);
  assert.match(brief, /超级管理员原则上只与管理员联系而不越级接收执行者汇报（执行者向普通管理员汇报），除非超级管理员主动联系执行者。/);
  assert.doesNotMatch(brief, /你当前角色是执行者/);
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

test('terminal group name is formatted with [] placed before terminal name instead of suffix', () => {
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
  assert.equal(ctx.sessionOptionLabel(sessionWithGroup), '[提审组] 协作终端');

  const sessionWithoutGroup = { id: 's2', name: '独狼终端' };
  assert.equal(ctx.sessionOptionLabel(sessionWithoutGroup), '独狼终端');

  ctx.sessionActivityPrefix = () => '[工作中] ';
  assert.equal(ctx.sessionOptionLabel(sessionWithGroup), '[提审组] [工作中] 协作终端');
  assert.equal(ctx.sessionOptionLabel(sessionWithoutGroup), '[工作中] 独狼终端');
  ctx.sessionActivityAgentPrefix = () => '[Codex] ';
  assert.equal(ctx.sessionOptionLabel(sessionWithGroup), '[提审组] [Codex] [工作中] 协作终端');
});

test('terminal-group-dialog title is 终端群组管理 without duplicate section-label', () => {
  const fs = require('node:fs');
  const html = fs.readFileSync(new URL('../static/terminal.html', import.meta.url), 'utf8');
  assert.match(
    html,
    /<dialog id="terminal-group-dialog"[\s\S]*?<h2 id="terminal-group-dialog-title">终端群组管理<\/h2>/,
  );
  assert.doesNotMatch(
    html,
    /<dialog id="terminal-group-dialog"[\s\S]*?<p class="section-label">终端管理<\/p>[\s\S]*?<h2 id="terminal-group-dialog-title">/,
  );
});

test('terminal group session list includes agent type column', () => {
  const fs = require('node:fs');
  const sessionsJs = fs.readFileSync(new URL('../static/terminal-sessions.js', import.meta.url), 'utf8');
  assert.match(
    sessionsJs,
    /function resolveTerminalSessionAgentType\(session\)/,
  );
  assert.match(
    sessionsJs,
    /agentSpan\.className = "terminal-group-session-agent"/,
  );
  assert.match(
    sessionsJs,
    /const agentLabel = resolveTerminalSessionAgentType\(item\)/,
  );
});

test('terminal group dialog provides UI to join existing groups for standalone terminals', () => {
  const fs = require('node:fs');
  const html = fs.readFileSync(new URL('../static/terminal.html', import.meta.url), 'utf8');
  assert.match(html, /id="terminal-group-join-container"/);
  assert.match(html, /id="terminal-group-join-select"/);
  assert.match(html, /id="terminal-group-join-button"/);
  assert.match(html, /id="terminal-group-rename-button"/);
  assert.match(html, /id="terminal-group-join-hint"/);

  const sessionsJs = fs.readFileSync(new URL('../static/terminal-sessions.js', import.meta.url), 'utf8');
  assert.match(sessionsJs, /function getAvailableTerminalGroups/);
  assert.match(sessionsJs, /function renderTerminalGroupJoinControls/);
  assert.match(sessionsJs, /function applyTerminalGroupJoinSelection/);
  assert.match(sessionsJs, /function handleTerminalGroupRenamePrompt/);
  assert.match(sessionsJs, /terminal-group-session-marker is-other-group/);

  const sessions = require('../static/terminal-sessions.js');
  assert.equal(typeof sessions.handleTerminalGroupRenamePrompt, 'function');
  globalThis.state = {
    sessions: [
      { id: 's1', name: '独立终端', group_token: '' },
      { id: 's2', name: '组员 A', group_token: 'g-alpha', group_name: 'alpha组', group_color: '#112233' },
      { id: 's3', name: '组员 B', group_token: 'g-alpha', group_name: 'alpha组', group_color: '#112233' },
      { id: 's4', name: '孤单组员', group_token: 'g-solo', group_name: '单人组', group_color: '#445566' },
    ],
  };
  const groups = sessions.getAvailableTerminalGroups('s1');
  // Only g-alpha has >= 2 members; g-solo is a singleton and should not be offered as an existing group
  assert.equal(groups.length, 1);
  assert.equal(groups[0].token, 'g-alpha');
  assert.equal(groups[0].name, 'alpha组');
  assert.equal(groups[0].color, '#112233');
  assert.equal(groups[0].members.length, 2);
  assert.equal(groups[0].isCurrentGroup, false);
});

test('terminal group rename generates concise update notice instead of collaboration brief', () => {
  const sessions = require('../static/terminal-sessions.js');
  const members = [
    { id: 's91', name: 'stockScreener_91_管理员', role: 'manager' },
    { id: 's90', name: 'stockScreener_90', role: 'executor' },
    { id: 's89', name: 'stockScreener_89', role: 'executor' },
  ];
  const names = new Map(members.map((m) => [m.id, m.name]));

  // 1. 已有群组从旧名称改名为新名称（构仿真）
  const renameNotice = sessions.terminalGroupChangeBrief(
    members,
    members,
    names,
    names,
    '构仿真',
    '旧攻坚组',
  );
  assert.match(renameNotice, /\[系统消息\]\[群组更新:构仿真\]/);
  assert.match(renameNotice, /群组名称已更新「旧攻坚组」→「构仿真」/);
  assert.doesNotMatch(renameNotice, /已建立协作分组/);
  assert.doesNotMatch(renameNotice, /这是群组建队提示/);

  // 2. 原群组无名称，首次命名为构仿真
  const initialNameNotice = sessions.terminalGroupChangeBrief(
    members,
    members,
    names,
    names,
    '构仿真',
    '',
  );
  assert.match(initialNameNotice, /\[系统消息\]\[群组更新:构仿真\]/);
  assert.match(initialNameNotice, /群组名称已更新为「构仿真」/);

  // 3. 成员和群组名均未变更，不产生任何通知
  const noChangeNotice = sessions.terminalGroupChangeBrief(
    members,
    members,
    names,
    names,
    '构仿真',
    '构仿真',
  );
  assert.equal(noChangeNotice, '');
});

test('collaboration contact and group update recipients only notify relevant managers and hide noise from peers and super admin', () => {
  const sessions = require('../static/terminal-sessions.js');

  const s99 = { id: 's99', name: 'stockScreener_99_超级管理', role: 'super_admin' };
  const s85 = { id: 's85', name: 'stockScreener_85__测性能', role: 'manager' };
  const s97 = { id: 's97', name: 'stockScreener_97', role: 'executor' };
  const s98 = { id: 's98', name: 'stockScreener_98', role: 'executor' };
  const s100 = { id: 's100', name: 'stockScreener_100_执行者', role: 'executor' };
  const allMembers = [s99, s85, s97, s98, s100];

  // 联系判定：
  // 1. 执行者之间无直接联系（执行者1与执行者2互不通知）
  assert.equal(sessions.terminalSessionsHaveCollaborationContact(s97, s98, allMembers), false);
  assert.equal(sessions.terminalSessionsHaveCollaborationContact(s97, s100, allMembers), false);

  // 2. 超级管理员原则上只与管理员联系，不越级联系执行者
  assert.equal(sessions.terminalSessionsHaveCollaborationContact(s99, s85, allMembers), true);
  assert.equal(sessions.terminalSessionsHaveCollaborationContact(s99, s97, allMembers), false);
  assert.equal(sessions.terminalSessionsHaveCollaborationContact(s99, s100, allMembers), false);

  // 3. 普通管理员联系超级管理员和所有执行者
  assert.equal(sessions.terminalSessionsHaveCollaborationContact(s85, s99, allMembers), true);
  assert.equal(sessions.terminalSessionsHaveCollaborationContact(s85, s97, allMembers), true);
  assert.equal(sessions.terminalSessionsHaveCollaborationContact(s85, s100, allMembers), true);

  // 群组变更通知投递目标：
  // 场景 A：向既有群组加入新执行者 s100，原有成员为 s99, s85, s97, s98
  const previousMembers = [s99, s85, s97, s98];
  const nextMembers = [s99, s85, s97, s98, s100];
  const recipients = sessions.resolveTerminalGroupUpdateRecipients({
    groupedMembers: nextMembers,
    previousMembers,
    previousGroupName: 'terminal-group',
    groupName: 'terminal-group',
  });
  // 严格只通知普通管理员 s85，绝不通知其它执行者 s97/s98，亦不通知超级管理员 s99
  assert.deepEqual(recipients.map((m) => m.id), ['s85']);

  // 场景 B：执行者 s97 更名
  const s97Renamed = { id: 's97', name: 'stockScreener_97_微调后', role: 'executor' };
  const renamedNextMembers = [s99, s85, s97Renamed, s98];
  const renameRecipients = sessions.resolveTerminalGroupUpdateRecipients({
    groupedMembers: renamedNextMembers,
    previousMembers,
    previousGroupName: 'terminal-group',
    groupName: 'terminal-group',
  });
  // 只通知自己和普通管理员 s85，绝不广播给 s98 或 s99
  assert.deepEqual(renameRecipients.map((m) => m.id).sort(), ['s85', 's97']);
});


