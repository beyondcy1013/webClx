import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const unified = readFileSync(new URL('../static/app-unified-tasks.js', import.meta.url), 'utf8');
const indexHtml = readFileSync(new URL('../static/index.html', import.meta.url), 'utf8');
const terminalHtml = readFileSync(new URL('../static/terminal.html', import.meta.url), 'utf8');

function makeElement() {
  return {
    value: '',
    checked: false,
    textContent: '',
    innerHTML: '',
    hidden: false,
    dataset: {},
    classList: {
      toggle() {},
      add() {},
      remove() {},
      contains: () => false,
    },
    setAttribute() {},
    removeAttribute() {},
    focus() {},
    scrollIntoView() {},
  };
}

function setup() {
  const elements = new Map();
  const table = makeElement();
  elements.set('unified-task-list', table);
  const requests = [];
  const context = vm.createContext({
    URLSearchParams,
    CSS: { escape: (v) => String(v) },
    requestJson: async (url, options) => {
      requests.push({ url, options });
      return { tasks: [], sessions: [] };
    },
    document: {
      readyState: 'loading',
      body: { classList: { contains: () => false } },
      addEventListener() {},
      querySelector: () => null,
      querySelectorAll: () => [],
      getElementById: (id) => {
        if (!elements.has(id)) return null;
        return elements.get(id);
      },
    },
    window: {
      location: { search: '' },
      setInterval: () => 1,
      clearInterval() {},
      setTimeout: () => 1,
      confirm: () => true,
    },
  });
  vm.runInContext(unified, context);
  const el = (id) => {
    if (!elements.has(id)) elements.set(id, makeElement());
    return elements.get(id);
  };
  for (const id of [
    'unified-task-schedule-mode',
    'unified-task-delay-value',
    'unified-task-delay-unit',
    'unified-task-datetime',
    'unified-task-text',
    'unified-task-send-enter',
    'unified-task-type',
    'unified-task-target-mode',
    'unified-task-terminal',
    'unified-task-create-btn',
    'unified-task-create-status',
    'unified-task-list-status',
    'unified-task-terminal-field',
    'unified-task-workdir-field',
    'unified-task-form-card',
  ]) {
    el(id);
  }
  return { context, table, elements, el, requests };
}

function setScheduleForm(el, { mode, value, unit, datetime }) {
  el('unified-task-schedule-mode').value = mode;
  if (value !== undefined) el('unified-task-delay-value').value = value;
  if (unit !== undefined) el('unified-task-delay-unit').value = unit;
  if (datetime !== undefined) el('unified-task-datetime').value = datetime;
}

test('interval mode converts seconds minutes and hours to repeat milliseconds', () => {
  const { context, el } = setup();
  for (const [value, unit, expectedMs] of [
    ['5', 'seconds', 5_000],
    ['30', 'minutes', 1_800_000],
    ['2', 'hours', 7_200_000],
  ]) {
    setScheduleForm(el, { mode: 'interval', value, unit });
    const resolved = vm.runInContext('resolveUnifiedTaskDueAt()', context);
    assert.equal(resolved.error, undefined);
    assert.equal(resolved.repeatIntervalMs, expectedMs);
    assert.ok(resolved.dueAtMs > Date.now(), 'first occurrence fires after now+interval');
    assert.ok(resolved.dueAtMs <= Date.now() + expectedMs);
    assert.match(resolved.label, /每隔/);
  }
});

test('delay and datetime modes produce a one-shot task with zero repeat', () => {
  const { context, el } = setup();
  setScheduleForm(el, { mode: 'delay', value: '5', unit: 'minutes' });
  const delayed = vm.runInContext('resolveUnifiedTaskDueAt()', context);
  assert.equal(delayed.error, undefined);
  assert.equal(delayed.repeatIntervalMs, 0);

  const futureLocalInput = vm.runInContext(
    'formatUnifiedTaskLocalInputValue(Date.now() + 3600000)',
    context,
  );
  setScheduleForm(el, { mode: 'datetime', datetime: futureLocalInput });
  const scheduled = vm.runInContext('resolveUnifiedTaskDueAt()', context);
  assert.equal(scheduled.error, undefined);
  assert.equal(scheduled.repeatIntervalMs, 0);
  assert.ok(
    scheduled.dueAtMs > Date.now(),
    'datetime-local input must round-trip to a local future timestamp',
  );
});

test('invalid intervals are rejected before any request', () => {
  const { context, el } = setup();
  for (const [value, unit] of [
    ['0', 'minutes'],
    ['abc', 'minutes'],
    ['0.5', 'seconds'],
    ['0.0002', 'hours'],
    ['1e20', 'hours'],
  ]) {
    setScheduleForm(el, { mode: 'interval', value, unit });
    const resolved = vm.runInContext('resolveUnifiedTaskDueAt()', context);
    assert.ok(resolved.error, `interval ${value} ${unit} must be rejected`);
  }
});

test('creation payload carries repeat_interval_millis and defaults to zero', async () => {
  const { context, el, requests } = setup();
  el('unified-task-type').value = 'paste';
  el('unified-task-target-mode').value = 'existing';
  el('unified-task-text').value = 'loop body';
  el('unified-task-send-enter').checked = false;
  el('unified-task-terminal').value = 's1';
  setScheduleForm(el, { mode: 'interval', value: '10', unit: 'minutes' });

  await vm.runInContext('createUnifiedTask()', context);
  const create = requests.find(
    (req) => req.url === '/api/terminal/scheduled-inputs' && req.options?.method === 'POST',
  );
  assert.ok(create, 'scheduled-inputs POST expected');
  const body = JSON.parse(create.options.body);
  assert.equal(body.repeat_interval_millis, 600_000);
  assert.equal(body.session_id, 's1');

  requests.length = 0;
  el('unified-task-text').value = 'one shot body';
  el('unified-task-terminal').value = 's1';
  setScheduleForm(el, { mode: 'delay', value: '5', unit: 'minutes' });
  await vm.runInContext('createUnifiedTask()', context);
  const oneShot = requests.find(
    (req) => req.url === '/api/terminal/scheduled-inputs' && req.options?.method === 'POST',
  );
  assert.ok(oneShot, 'one-shot scheduled-inputs POST expected');
  assert.equal(JSON.parse(oneShot.options.body).repeat_interval_millis, 0);
});

test('setUnifiedTaskButtonBusy toggles busy state and restores the original label', () => {
  const { context, el } = setup();
  const button = el('unified-task-create-btn');
  button.textContent = '创建任务';

  vm.runInContext(
    'setUnifiedTaskButtonBusy(document.getElementById("unified-task-create-btn"), true, "创建中...")',
    context,
  );
  assert.equal(button.disabled, true);
  assert.equal(button.textContent, '创建中...');
  assert.equal(button.dataset.defaultLabel, '创建任务');

  vm.runInContext(
    'setUnifiedTaskButtonBusy(document.getElementById("unified-task-create-btn"), false)',
    context,
  );
  assert.equal(button.disabled, false);
  assert.equal(button.textContent, '创建任务');

  assert.equal(
    vm.runInContext('setUnifiedTaskButtonBusy(null, true)', context),
    undefined,
    'null button must be a safe no-op',
  );
});

test('editing a recurring parent preserves its interval through the PATCH body', async () => {
  const { context, el, requests } = setup();
  const task = vm.runInContext(
    `normalizeUnifiedPasteTask({
      id: 'parent-1', due_at: Date.now() + 600000, status: 'queued',
      session_id: 's1', text: 'loop', repeat_interval_millis: 300000, parent_task_id: '',
    })`,
    context,
  );
  assert.equal(task.repeatIntervalMs, 300_000);

  el('unified-task-type').value = 'paste';
  el('unified-task-target-mode').value = 'existing';
  el('unified-task-terminal').value = 's1';
  el('unified-task-text').value = 'loop';
  el('unified-task-send-enter').checked = true;
  context.task = task;
  vm.runInContext('fillUnifiedFormFromTask(task)', context);
  assert.equal(el('unified-task-schedule-mode').value, 'interval');
  assert.equal(el('unified-task-delay-value').value, '5');
  assert.equal(el('unified-task-delay-unit').value, 'minutes');

  vm.runInContext(
    `unifiedTaskState.editTaskId = 'parent-1'; unifiedTaskState.editTaskSource = 'server';`,
    context,
  );
  await vm.runInContext('updateUnifiedTask()', context);
  const patch = requests.find((req) => req.url === '/api/terminal/scheduled-inputs/parent-1');
  assert.ok(patch, 'PATCH expected');
  assert.equal(patch.options.method, 'PATCH');
  assert.equal(JSON.parse(patch.options.body).repeat_interval_millis, 300_000);
});

test('clearing the form resets schedule mode to one-shot', () => {
  const { context, el } = setup();
  el('unified-task-schedule-mode').value = 'interval';
  el('unified-task-delay-value').value = '3';
  el('unified-task-delay-unit').value = 'hours';
  vm.runInContext('clearUnifiedTaskForm()', context);
  assert.equal(el('unified-task-schedule-mode').value, 'delay');
});

test('run-now on a recurring parent advances the next trigger without deleting the series', async () => {
  const { context, requests } = setup();
  const task = vm.runInContext(
    `normalizeUnifiedPasteTask({
      id: 'parent-9', due_at: Date.now() + 600000, status: 'queued',
      session_id: 's1', text: 'loop', repeat_interval_millis: 60000, parent_task_id: '',
    })`,
    context,
  );
  context.task = task;
  vm.runInContext('unifiedTaskState.pasteTasks = [task]', context);
  const before = Date.now();
  await vm.runInContext('runUnifiedTaskNow("parent-9")', context);
  const patch = requests.find((req) => req.url === '/api/terminal/scheduled-inputs/parent-9');
  assert.ok(patch, 'run-now must PATCH the parent due_at');
  assert.equal(patch.options.method, 'PATCH');
  const patchBody = JSON.parse(patch.options.body);
  assert.ok(patchBody.due_at > before && patchBody.due_at <= Date.now() + 2000);
  assert.equal(
    requests.some((req) => req.options?.method === 'DELETE'),
    false,
    'the recurring parent must not be cancelled',
  );
  assert.equal(
    requests.some((req) => /\/api\/terminal\/sessions\/.+\/input/.test(req.url)),
    false,
    'run-now on a parent must not bypass the scheduler with a direct write',
  );
});

test('recurring parent renders 每隔 schedule with countdown controls while children stay delivery records', () => {
  const { context, table } = setup();
  vm.runInContext(
    `unifiedTaskState.pasteTasks = [
      normalizeUnifiedPasteTask({
        id: 'parent-1', due_at: Date.now() + 300000, status: 'queued',
        session_id: 's1', text: 'loop', repeat_interval_millis: 300000, parent_task_id: '',
      }),
      normalizeUnifiedPasteTask({
        id: 'parent-1-fire-3', due_at: 1, status: 'confirmed',
        session_id: 's1', text: 'loop', repeat_interval_millis: 0, parent_task_id: 'parent-1',
      }),
    ];
    unifiedTaskState.showDeliveryRecords = true;
    renderUnifiedTaskTable()`,
    context,
  );
  assert.match(table.innerHTML, /每隔 5 分钟/);
  assert.match(table.innerHTML, /unified-task-remaining/);
  assert.match(table.innerHTML, /unified-task-run-now/);
  assert.match(table.innerHTML, /unified-task-edit/);
  assert.match(table.innerHTML, /unified-task-cancel/);
  const rows = table.innerHTML.split('<tr data-task-id=');
  const childRow = rows.find((row) => row.includes('parent-1-fire-3'));
  assert.ok(childRow, 'child delivery record row expected');
  assert.match(childRow, /投递记录/);
  assert.match(childRow, /循环任务 parent-1/);
  assert.doesNotMatch(childRow, /unified-task-run-now|unified-task-edit|unified-task-cancel/);
});

test('both pages expose the 循环间隔 schedule option and share the cache version', () => {
  for (const html of [indexHtml, terminalHtml]) {
    assert.match(html, /<option value="interval">循环间隔<\/option>/);
  }
  const version = (html) => html.match(/app-unified-tasks\.js\?v=([a-zA-Z0-9-]+)/)?.[1];
  assert.ok(version(indexHtml), 'index.html must version app-unified-tasks.js');
  assert.equal(version(indexHtml), version(terminalHtml));
});
