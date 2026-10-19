import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const settings = require('../static/terminal-settings.js');
const source = readFileSync(new URL('../static/terminal-tool-actions.js', import.meta.url), 'utf8');
const command = { kind: 'send_command', value: 'echo one', seconds: 0 };
const failing = { kind: 'send_command', value: 'echo two', seconds: 0 };
const entry = (id, actions) => ({
  id, label: `流程${id}`, kind: 'action', root_key: 'tools', parent_id: null, sort_order: 0, actions,
});

function setup(entries = []) {
  const store = new Map();
  let statuses = [];
  const c = vm.createContext({
    WebClxTerminalSettings: settings,
    state: { sessions: [], currentPath: '/project', terminalToolEntries: entries },
    closeTerminalToolMenu() {},
    setTerminalToolMenuStatus(...message) { statuses.push(String(message[0] || '')); },
    updateStatus() {},
    terminalToolMenuEl: null,
    terminalToolMenuStatusEl: null,
    terminalToolsMenuEl: null,
    window: {
      localStorage: {
        getItem: (key) => store.get(key) || null,
        setItem: (key, value) => store.set(key, value),
      },
    },
  });
  vm.runInContext(source, c);
  c.setTerminalToolMenuStatus = (...args) => statuses.push(String(args[0] || ""));
  c.TERMINAL_TOOL_ACTION_TYPES = [
    { key: 'send_command', label: '发送命令' },
    { key: 'run_workflow', label: '嵌套工作流' },
  ];
  c.__statuses = statuses;
  c.__resetStatuses = () => { statuses.length = 0; };
  c.__store = store;
  return c;
}

test('preflight plan marks flattened actions with their owning workflow', () => {
  const inner = entry('inner', [command]);
  const outer = entry('outer', [
    { kind: 'run_workflow', value: 'inner', target_entry_id: 'inner', seconds: 0 },
    failing,
  ]);
  const plan = settings.inspectTerminalWorkflow(outer, [inner, outer]);
  assert.deepEqual(plan.steps.map((step) => step.workflow_entry_id), ['inner', 'outer']);
});

test('successful runs persist bounded metadata without command or parameter values', async () => {
  const item = entry('a', [command]);
  const c = setup([item]);
  c.prepareTerminalWorkflowRuntimeParameters = async () => ({});
  c.executeTerminalToolAction = async () => {};
  await c.executeTerminalToolEntry(item);
  const records = JSON.parse(c.__store.get('webclx:workflow-runs'));
  assert.equal(records.length, 1);
  assert.equal(records[0].entryId, 'a');
  assert.equal(records[0].status, 'completed');
  assert.equal(records[0].progress, '1/1');
  assert.equal(JSON.stringify(records[0]).includes('echo one'), false);
  for (let index = 0; index < 25; index += 1) {
    await c.executeTerminalToolEntry(item);
  }
  const bounded = JSON.parse(c.__store.get('webclx:workflow-runs'));
  assert.ok(bounded.length <= 20 && bounded.every((record) => record.id));
});

test('failure stores failed index and same-session retry continues from that leaf', async () => {
  const item = entry('retry', [command, failing]);
  const c = setup([item]);
  const values = [];
  let failFirst = true;
  c.prepareTerminalWorkflowRuntimeParameters = async () => ({});
  c.executeTerminalToolAction = async (action) => {
    if (action.value === 'echo two' && failFirst) {
      failFirst = false;
      throw new Error('temporary failure');
    }
    values.push(action.value);
  };
  await c.executeTerminalToolEntry(item);
  assert.deepEqual(values, ['echo one']);
  assert.match(c.__statuses.at(-1), /temporary failure/);
  const failedRecord = JSON.parse(c.__store.get('webclx:workflow-runs')).find((record) => record.status === 'failed');
  assert.equal(failedRecord.progress, '2/2');
  assert.equal(vm.runInContext('terminalToolFailedExecution.nextIndex', c), 1);

  await c.executeTerminalToolEntry(item, vm.runInContext('terminalToolFailedExecution', c));
  assert.deepEqual(values, ['echo one', 'echo two']);
  const records = JSON.parse(c.__store.get('webclx:workflow-runs'));
  assert.equal(records[0].status, 'completed');
  assert.equal(records.some((record) => record.id === failedRecord.id && record.status === 'completed'), true);
});

test('definition changes reject failed-step resume but permit full restart', async () => {
  const item = entry('changed', [command]);
  const c = setup([item]);
  c.prepareTerminalWorkflowRuntimeParameters = async () => ({});
  c.executeTerminalToolAction = async () => { throw new Error('boom'); };
  await c.executeTerminalToolEntry(item);
  c.__resetStatuses();
  const saved = vm.runInContext('terminalToolFailedExecution', c);
  item.actions.push({ ...failing, value: 'echo changed' });
  c.__resetStatuses();
  await c.executeTerminalToolEntry(item, saved);
  assert.match(c.__statuses.at(-1), /定义已变化/);
  c.executeTerminalToolAction = async () => {};
  await c.executeTerminalToolEntry(item);
  assert.equal(JSON.parse(c.__store.get('webclx:workflow-runs'))[0].status, 'completed');
});

test('cancelling parameter prompt records cancellation without side effects', async () => {
  const item = entry('cancel', [command]);
  const c = setup([item]);
  let executions = 0;
  c.prepareTerminalWorkflowRuntimeParameters = async () => null;
  c.executeTerminalToolAction = async () => { executions += 1; };
  await c.executeTerminalToolEntry(item);
  assert.equal(executions, 0);
  assert.equal(JSON.parse(c.__store.get('webclx:workflow-runs'))[0].status, 'cancelled');
});
