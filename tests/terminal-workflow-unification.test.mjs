import assert from 'node:assert/strict';
import test from 'node:test';
import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
const require = createRequire(import.meta.url);
const settings = require('../static/terminal-settings.js');
const entry = (id, actions) => ({ id, label: id, kind: 'action', root_key: 'tools', parent_id: null, sort_order: 0, actions });
const ref = (id) => ({ kind: 'run_workflow', value: id });
const command = { kind: 'send_command', value: 'echo example' };

test('templates reuse persisted action types and return independent drafts', () => {
  const a = settings.createTerminalWorkflowTemplate('environment', '/project');
  Object.assign(a[0], { preset_selector: 'Example', terminal_name: '开发' });
  assert.equal(settings.normalizeTerminalToolEntries([entry('env', a)]).length, 1);
  assert.equal(settings.inspectTerminalWorkflow(entry('env', a), []).label, '工作环境');
  assert.equal(settings.createTerminalWorkflowTemplate('environment')[0].preset_selector, '');
  assert.deepEqual(settings.createTerminalWorkflowTemplate('task').map(a => a.kind), ['switch_api_preset', 'codex_exec']);
  assert.equal(settings.createTerminalWorkflowTemplate('reference')[0].kind, 'run_workflow');
  assert.throws(() => settings.createTerminalWorkflowTemplate('unknown'));
});

test('references follow shared definition; sequential reuse is not a cycle', () => {
  const base = entry('base', [command]);
  const link = entry('link', [ref('base'), ref('base')]);
  assert.equal(settings.inspectTerminalWorkflow(link, [base, link]).steps.length, 2);
  base.actions[0] = { ...command, value: 'echo changed' };
  assert.equal(settings.inspectTerminalWorkflow(link, [base, link]).steps[0].value, 'echo changed');
});

test('preflight rejects missing references, cycles, invalid parameters and expansion overflow', () => {
  const a = entry('a', [ref('b')]);
  const b = entry('b', [ref('a')]);
  assert.throws(() => settings.inspectTerminalWorkflow(a, [a]), /找不到/);
  assert.throws(() => settings.inspectTerminalWorkflow(a, [a, b]), /循环/);
  assert.throws(() => settings.inspectTerminalWorkflow(entry('bad', [{ kind: 'send_command' }]), []), /无效/);
  const wide = entry('wide', Array.from({ length: 20 }, () => ref('base')));
  const huge = entry('huge', Array.from({ length: 20 }, () => ref('wide')));
  assert.throws(() => settings.inspectTerminalWorkflow(huge, [entry('base', [command]), wide, huge]), /200/);
});

test('completion never equates command submission with task success', () => {
  const task = entry('task', [{ kind: 'switch_api_preset', value: 'preset' }, { kind: 'codex_exec', value: 'review' }]);
  assert.match(settings.inspectTerminalWorkflow(task, []).completion, /任务执行完成/);
  assert.match(settings.inspectTerminalWorkflow(entry('cmd', [command]), []).completion, /终端输出为准/);
});

test('invalid nested reference fails before executing any preceding action', async () => {
  const statuses = [];
  let executions = 0;
  const broken = entry('broken', [command, ref('missing')]);
  const context = vm.createContext({
    WebClxTerminalSettings: settings,
    state: { sessions: [], currentPath: '/project', terminalToolEntries: [broken] },
    closeTerminalToolMenu() {},
    setTerminalToolMenuStatus: (...args) => statuses.push(args),
    updateStatus() {}, terminalToolMenuEl: null,
  });
  vm.runInContext(readFileSync(new URL('../static/terminal-tool-actions.js', import.meta.url), 'utf8'), context);
  context.closeTerminalToolMenu = () => {};
  context.setTerminalToolMenuStatus = (...args) => statuses.push(args);
  context.executeTerminalToolAction = async () => { executions += 1; };
  await context.executeTerminalToolEntry(broken);
  assert.equal(executions, 0);
  assert.match(statuses.at(-1)[0], /找不到/);
  assert.equal(vm.runInContext('terminalToolExecutionRunning', context), false);
});
