import assert from 'node:assert/strict';
import test from 'node:test';
import vm from 'node:vm';
import { readFileSync } from 'node:fs';
const source = readFileSync(new URL('../static/terminal-tool-actions.js', import.meta.url), 'utf8');
const entries = [
  { id: 'folder', kind: 'folder', label: '开发环境', root_key: 'tools' },
  { id: 'a', kind: 'action', label: 'API 调试', parent_id: 'folder', root_key: 'tools', sort_order: 0, actions: [{ value: 'PRIVATE_TOKEN' }] },
  { id: 'b', kind: 'action', label: 'API 发布', root_key: 'other', sort_order: 0, actions: [] },
];
function setup() {
  const context = vm.createContext({ state: { terminalToolEntries: structuredClone(entries) }, TERMINAL_TOOL_ROOTS: [{ key: 'tools', label: '工作流' }, { key: 'other', label: '其他' }] });
  vm.runInContext(source, context);
  return context;
}
test('search spans roots, matches all words against names and folder paths without exposing action values', () => {
  const c = setup();
  assert.equal(c.terminalToolSearchResults('api').length, 2);
  assert.equal(c.terminalToolSearchResults('开发 API')[0].entry.id, 'a');
  assert.equal(c.terminalToolSearchResults('其他 发布')[0].entry.id, 'b');
  assert.equal(c.terminalToolSearchResults('PRIVATE_TOKEN').length, 0);
  assert.equal(c.terminalToolSearchResults(' ').length, 0);
  assert.equal(c.terminalToolSearchResults('不存在').length, 0);
});
test('search tolerates folder cycles and retains current entry identity after edits', () => {
  const c = setup();
  c.state.terminalToolEntries[0].parent_id = 'folder';
  assert.equal(c.terminalToolSearchResults('开发').length, 1);
  c.state.terminalToolEntries[1].label = '更新';
  assert.equal(c.terminalToolSearchResults('调试').length, 0);
  assert.equal(c.terminalToolSearchResults('更新')[0].entry, c.state.terminalToolEntries[1]);
});
test('search only renders, opening resets query, existing click dispatcher owns execution', () => {
  const c = setup(); let renders = 0;
  c.renderTerminalToolMenu = () => renders++;
  c.handleTerminalToolSearch({ target: { value: 'API' } });
  assert.equal(renders, 1);
  assert.equal(vm.runInContext('terminalToolSearchQuery', c), 'API');
  c.terminalToolMenuEl = {}; c.terminalToolsButtonEl = null;
  const input = { value: 'API' }; c.document = { getElementById: () => input };
  c.setTerminalToolMenuStatus = () => {}; c.setTerminalToolMenuExpanded = () => {};
  c.openTerminalToolMenu('tools', null);
  assert.equal(input.value, '');
  assert.equal(vm.runInContext('terminalToolSearchQuery', c), '');
});

test('recent workflow ids persist in bounded MRU order and ignore removed entries', () => {
  const store = new Map();
  const c = setup();
  c.window = { localStorage: {
    getItem: (key) => store.get(key) || null,
    setItem: (key, value) => store.set(key, value),
  } };
  c.rememberTerminalToolRecent('a');
  c.rememberTerminalToolRecent('b');
  c.rememberTerminalToolRecent('a');
  assert.deepEqual(Array.from(c.terminalToolRecentEntries(), (entry) => entry.id), ['a', 'b']);
  c.state.terminalToolEntries = c.state.terminalToolEntries.filter((entry) => entry.id !== 'a');
  assert.deepEqual(Array.from(c.terminalToolRecentEntries(), (entry) => entry.id), ['b']);
  assert.deepEqual(JSON.parse(store.get('webclx:workflow-recent')), ['a', 'b']);
});
