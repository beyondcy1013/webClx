import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const read = name => readFileSync(new URL(`../static/${name}`, import.meta.url), 'utf8');
const accountSource = read('app-codex-account-manager.js');
const loader = accountSource.slice(accountSource.indexOf('  let accountReloadPending'), accountSource.indexOf('  window.loadCodexAccounts ='));
let storedName = 'old';
let release;
let reads = 0;
const rendered = [];
const context = vm.createContext({
  document: { getElementById: () => ({}) }, state: {},
  requestJson: async path => {
    if (path !== '/api/auth/codex-accounts') return { presets: [] };
    reads++;
    const name = storedName;
    if (reads === 1) await new Promise(resolve => { release = resolve; });
    return { accounts: [{ id: 'fixture', kind: 'oauth', name }] };
  },
  orderCodexAccountsByPresetTables: accounts => accounts,
  renderAccounts: accounts => rendered.push(accounts[0].name),
  showStatus: () => {},
});
vm.runInContext(`let allAccounts = []; ${loader}`, context);
const first = context.loadCodexAccounts();
storedName = 'saved';
const second = context.loadCodexAccounts();
assert.equal(second, first, 'overlapping callers await the same fresh reload');
release();
await second;
assert.equal(reads, 2, 'saving during an old request must schedule a fresh request');
assert.equal(rendered.at(-1), 'saved');
await context.loadCodexAccounts();
assert.equal(reads, 3, 'loader remains reusable');

// Execute the real OAuth save helper, not a duplicated save implementation.
const auth = read('app-auth-manager.js');
const saveHelper = auth.slice(auth.indexOf('async function saveAuthPresetFromRawText('), auth.indexOf('function buildAuthApplyPayload('));
const calls = [];
Object.assign(context, {
  window: { loadCodexAccounts: async () => calls.push('unified') },
  normalizeAuthInput: () => ({}), buildAuthPresetName: () => 'fixture',
  extractAuthDetails: () => ({}), updateStatus: () => {},
  requestJson: async () => { calls.push('save'); return { preset: { id: 'fixture', name: 'saved' } }; },
  loadAuthPresets: async () => calls.push('legacy'),
});
vm.runInContext(saveHelper, context);
assert.ok(await context.saveAuthPresetFromRawText('{}', 'saved', {}, [], 'fixture'));
assert.deepEqual(calls, ['save', 'legacy', 'unified']);
calls.length = 0;
context.requestJson = async () => { throw new Error('save failed'); };
assert.equal(await context.saveAuthPresetFromRawText('{}', 'saved', {}, [], 'fixture'), null);
assert.deepEqual(calls, [], 'failed saves must not report a refreshed success');
assert.match(read('app-api-manager.js'), /await loadApiPresets\(\);\s*await window\.loadCodexAccounts\?\.\(\);\s*apiPresetFormSnapshot = null;/);
console.log('Account save refresh and overlapping reload regression checks passed');
