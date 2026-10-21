import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
const unified = readFileSync(new URL('../static/app-unified-tasks.js', import.meta.url), 'utf8');
const paste = readFileSync(new URL('../static/terminal-paste.js', import.meta.url), 'utf8');
function setup() {
  const table = { innerHTML: '' };
  const context = vm.createContext({
    URLSearchParams,
    document: {
      readyState: 'loading',
      body: { classList: { contains: () => false } },
      addEventListener() {},
      querySelectorAll: () => [],
      getElementById: id => id === 'unified-task-list' ? table : id === 'terminal-scheduled-tasks-dialog' ? {} : null,
    },
    window: { location: { search: '' }, setInterval: () => 1, clearInterval() {} },
  });
  vm.runInContext(unified, context);
  return { context, table };
}
test('dialog queue hides delivery records until requested', () => {
  const { context, table } = setup();
  context.input = { id: 'history', due_at: 1, status: 'pending_confirmation' };
  vm.runInContext('unifiedTaskState.pasteTasks = [normalizeUnifiedPasteTask(input)]; unifiedTaskState.showDeliveryRecords = false; renderUnifiedTaskTable()', context);
  assert.match(table.innerHTML, /待发送队列是空的/);
  assert.doesNotMatch(table.innerHTML, /待确认接收|unified-task-run-now/);
});
test('delivery records retain honest states and have no countdown or resend actions', () => {
  const { context, table } = setup();
  for (const status of ['confirmed', 'pending_confirmation', 'unknown', 'dispatching', 'written', 'unverified']) {
    context.input = { id: 'history', due_at: 1, status };
    vm.runInContext('unifiedTaskState.pasteTasks = [normalizeUnifiedPasteTask(input)]; unifiedTaskState.showDeliveryRecords = true; renderUnifiedTaskTable()', context);
    assert.doesNotMatch(table.innerHTML, /即将发送|unified-task-run-now|unified-task-edit|unified-task-cancel|unified-task-remaining/);
    assert.match(table.innerHTML, /投递记录/);
    assert.equal(vm.runInContext('unifiedTaskIsPending(unifiedTaskState.pasteTasks[0])', context), false);
    if (status === 'pending_confirmation') assert.match(table.innerHTML, /待确认接收/);
    if (status === 'unknown') assert.match(table.innerHTML, /结果未知/);
  }
});
test('dialog shows only the active terminal directory until the view switches to all', () => {
  const { context, table } = setup();
  context.state = {
    activeSessionId: 's1',
    currentPath: '/work/app',
    sessions: [
      { id: 's1', path: '/work/app', display_path: '/work/app' },
      { id: 's2', path: '/work/other', display_path: '/work/other' },
    ],
  };
  vm.runInContext(`
    unifiedTaskState.terminals = [
      { id: 's1', name: 'here', path: '/work/app', canonicalPath: '/work/app' },
      { id: 's2', name: 'there', path: '/work/other', canonicalPath: '/work/other' },
    ];
    unifiedTaskState.currentSessionId = 's1';
    unifiedTaskState.currentWorkdir = '/work/app/';
    unifiedTaskState.currentWorkdirDisplay = '/work/app/';
    unifiedTaskState.taskView = 'current';
    unifiedTaskState.showDeliveryRecords = false;
    unifiedTaskState.pasteTasks = [
      normalizeUnifiedPasteTask({ id: 'here', due_at: 1, status: 'queued', session_id: 's1', preview: 'same-dir' }),
      normalizeUnifiedPasteTask({ id: 'there', due_at: 2, status: 'queued', session_id: 's2', preview: 'other-dir' }),
    ];
    renderUnifiedTaskTable();
  `, context);
  assert.match(table.innerHTML, /same-dir/);
  assert.doesNotMatch(table.innerHTML, /other-dir/);
  vm.runInContext(`setUnifiedTaskView('all');`, context);
  assert.match(table.innerHTML, /same-dir/);
  assert.match(table.innerHTML, /other-dir/);
});
test('queued and legacy tasks keep countdown and controls', () => {
  const { context, table } = setup();
  for (const status of ['queued', undefined]) {
    context.input = { id: 'queued', due_at: 1, status };
    vm.runInContext('unifiedTaskState.pasteTasks = [normalizeUnifiedPasteTask(input)]; renderUnifiedTaskTable()', context);
    assert.match(table.innerHTML, /unified-task-run-now/);
    assert.match(table.innerHTML, /unified-task-remaining/);
  }
});
test('terminal timer cache excludes delivery history without dropping queued entries', () => {
  const start = paste.indexOf('function normalizeTerminalPasteScheduledTask(');
  const end = paste.indexOf('function normalizeTerminalAutoContinueScheduledTask(', start);
  const context = vm.createContext({});
  vm.runInContext(paste.slice(start, end), context);
  for (const status of ['confirmed', 'pending_confirmation', 'unknown', 'dispatching', 'written', 'unverified', 'queued', undefined]) {
    context.input = { id: 'task', due_at: 1, status };
    const result = vm.runInContext('normalizeTerminalPasteScheduledTask(input)', context);
    assert.equal(Boolean(result), status === 'queued' || status === undefined);
  }
});
