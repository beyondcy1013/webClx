import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
const settings = require('../static/terminal-settings.js');
const source = readFileSync(new URL('../static/terminal-tool-actions.js', import.meta.url), 'utf8');
const command = { kind: 'send_command', value: 'git checkout {{ branch }}' };
const task = { kind: 'codex_exec', value: '处理 {{branch}}：{{ 任务描述 }}' };
const entry = (id, actions) => ({
  id, label: id, kind: 'action', root_key: 'tools', parent_id: null, sort_order: 0, actions,
});

function loadActions() {
  const context = vm.createContext({
    WebClxTerminalSettings: settings,
    state: { sessions: [], currentPath: '/project', terminalToolEntries: [] },
    closeTerminalToolMenu() {},
    setTerminalToolMenuStatus() {},
    updateStatus() {},
    terminalToolMenuEl: null, terminalToolMenuStatusEl: null, terminalToolsMenuEl: null,
  });
  vm.runInContext(source, context);
  context.TERMINAL_TOOL_ACTION_TYPES = [{ key: 'send_command', label: '发送命令' }, { key: 'codex_exec', label: '任务' }];
  return context;
}

test('runtime placeholders survive schema validation and reference expansion', () => {
  const outer = entry('outer', [{ kind: 'run_workflow', value: 'inner', target_entry_id: 'inner' }, task]);
  const inner = entry('inner', [command]);
  const entries = [outer, inner];
  assert.equal(settings.normalizeTerminalToolEntries(entries).length, entries.length);
  const plan = settings.inspectTerminalWorkflow(outer, entries);
  assert.deepEqual(
    [...new Set(plan.steps.flatMap((step) => [...String(step.value).matchAll(/\{\{\s*([\w\-\u00c0-\uffff]{1,64})\s*\}\}/g)].map((match) => match[1])))],
    ['branch', '任务描述'],
  );
});

test('parameter collection deduplicates shared names and respects the action size limit', () => {
  const c = loadActions();
  const plan = { steps: [command, task] };
  assert.deepEqual(JSON.parse(JSON.stringify(c.collectTerminalWorkflowRuntimeParameters(plan))), [
    { name: 'branch', long: false },
    { name: '任务描述', long: true },
  ]);
  assert.throws(
    () => c.collectTerminalWorkflowRuntimeParameters({
      steps: Array.from({ length: 21 }, (_, index) => ({
        kind: 'send_command', value: `{{param_${index + 1}}}`,
      })),
    }),
    /超过 20/,
  );
});

test('materialization replaces only supplied parameters and returns the original action when unchanged', () => {
  const c = loadActions();
  const materialized = c.materializeTerminalWorkflowAction(task, { branch: 'main', 任务描述: 'review' });
  assert.equal(materialized.value, '处理 main：review');
  assert.equal(c.materializeTerminalWorkflowAction(command, { other: 'x' }), command);
});

test('execution asks once, materializes top and nested actions, and shares runtime context', async () => {
  const c = loadActions();
  const outer = entry('outer', [
    command,
    { kind: 'run_workflow', value: 'inner', target_entry_id: 'inner' },
  ]);
  const inner = entry('inner', [{ kind: 'codex_exec', value: '{{branch}} 的 {{任务描述}}', seconds: 0 }]);
  c.state.terminalToolEntries = [outer, inner];
  const executed = [];
  const statuses = [];
  c.setTerminalToolMenuStatus = (...args) => statuses.push(args[0]);
  c.prepareTerminalWorkflowRuntimeParameters = async () => ({ branch: 'release', 任务描述: '发布检查' });
  c.executeTerminalToolAction = async (action, context) => {
    executed.push({ action, context });
  };
  await c.executeTerminalToolEntry(outer);
  assert.equal(executed.length, 2, statuses.join(' | '));
  assert.equal(executed[0].action.value, 'git checkout release');
  assert.equal(executed[1].action.value, 'release 的 发布检查');
  assert.equal(executed[1].context.workflowEntryId, 'inner');
  assert.deepEqual(executed[0].context.runtimeParameters, { branch: 'release', 任务描述: '发布检查' });
  assert.equal(c.materializeTerminalWorkflowAction(inner.actions[0], executed[0].context.runtimeParameters).value, 'release 的 发布检查');
  assert.equal(vm.runInContext('terminalToolExecutionRunning', c), false);
});

test('canceling the parameter dialog executes no workflow action and restores the running flag', async () => {
  const c = loadActions();
  const item = entry('outer', [command]);
  c.state.terminalToolEntries = [item];
  let executions = 0;
  c.prepareTerminalWorkflowRuntimeParameters = async () => null;
  c.executeTerminalToolAction = async () => { executions += 1; };
  await c.executeTerminalToolEntry(item);
  assert.equal(executions, 0);
  assert.equal(vm.runInContext('terminalToolExecutionRunning', c), false);
});
