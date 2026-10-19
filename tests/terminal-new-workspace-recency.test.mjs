import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import test from 'node:test';
const source = readFileSync(new URL('../static/terminal-specified-task.js', import.meta.url), 'utf8');
const start = source.indexOf('let terminalSpecifiedTaskWorkspaceSessions = null;');
const end = source.indexOf('function terminalSpecifiedTaskWorkspaceLabel', start);
function fixture(state, requestJson) {
  const context = vm.createContext({
    state, requestJson, console: { warn() {} },
    terminalSpecifiedTaskResolveWorkspacePath: (path) => path.startsWith('/') ? path.replace(/\/$/, '') : '/home/codes/' + path,
    terminalSpecifiedTaskSessionPath: (session) => session.path,
    terminalSpecifiedTaskCurrentPath: () => '/home/codes/current',
  });
  vm.runInContext(source.slice(start, end), context);
  return context;
}
const choices = (context) => Array.from(context.terminalSpecifiedTaskWorkspaceChoices('/home/codes/current'));
test('current directory stays first and recent terminal precedes fifteen older workspace visits', () => {
  const history = Array.from({ length: 15 }, (_, i) => ({ path: '/home/codes/old-' + i, last_opened_at: 100 + i }));
  history.push({ path: '/home/codes/system', last_opened_at: 1 });
  const original = JSON.stringify(history);
  const context = fixture({ workspaceHistory: history, sessions: [{ path: 'system', last_opened_at: 500 }], workspaceDir: '/home/codes' });
  const result = choices(context);
  assert.equal(result[0], '/home/codes/current');
  assert.equal(result[1], '/home/codes/system');
  assert.equal(result[2], '/home/codes/old-14');
  assert.equal(result.filter((path) => path === '/home/codes/current').length, 1);
  assert.equal(result.filter((path) => path === '/home/codes/system').length, 1);
  assert.equal(JSON.stringify(history), original);
});
test('deduplicates latest timestamps and tolerates legacy and malformed entries', () => {
  const context = fixture({ workspaceHistory: [null, {}, '/home/codes/legacy', { path: '/home/codes/a', last_opened_at: 30 }, { path: '/home/codes/a/', last_opened_at: 40 }, { path: '/home/codes/b', last_opened_at: 'bad' }], sessions: [{ path: '', created_at: 50 }], workspaceDir: '/home/codes' });
  const result = choices(context);
  assert.equal(result[0], '/home/codes/current');
  assert.equal(result[1], '/home/codes');
  assert.equal(result[2], '/home/codes/a');
  assert.equal(result.filter((path) => path === '/home/codes/a').length, 1);
  assert.ok(result.includes('/home/codes/legacy'));
});
test('refreshes settings and all session history every time the dialog opens', async () => {
  const calls = [];
  let lastOpened = 200;
  const context = fixture({ workspaceHistory: [], sessions: [], workspaceDir: '/home/codes' }, async (url) => {
    calls.push(url);
    return url === '/api/settings'
      ? { workspace_history: [{ path: '/home/codes/other', last_opened_at: 300 }] }
      : { sessions: [{ path: 'system', last_opened_at: lastOpened }] };
  });
  await context.refreshTerminalSpecifiedTaskWorkspaceHistory();
  assert.equal(choices(context)[1], '/home/codes/other');
  lastOpened = 400;
  await context.refreshTerminalSpecifiedTaskWorkspaceHistory();
  assert.equal(choices(context)[1], '/home/codes/system');
  assert.deepEqual(calls, ['/api/settings', '/api/terminal/sessions?all=true&history=true', '/api/settings', '/api/terminal/sessions?all=true&history=true']);
  assert.ok(source.includes('await refreshTerminalSpecifiedTaskWorkspaceHistory();'));
});
test('refresh failure preserves known workspace and local session history', async () => {
  const context = fixture({ workspaceHistory: [{ path: '/home/codes/old', last_opened_at: 1 }], sessions: [{ path: 'system', last_opened_at: 2 }], workspaceDir: '/home/codes' }, async () => { throw new Error('offline'); });
  await context.refreshTerminalSpecifiedTaskWorkspaceHistory();
  assert.equal(choices(context)[1], '/home/codes/system');
  assert.ok(choices(context).includes('/home/codes/old'));
});
