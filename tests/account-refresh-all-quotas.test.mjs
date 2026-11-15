import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
const source = readFileSync(new URL('../static/app-codex-account-manager.js', import.meta.url), 'utf8');
const fn = source.slice(source.indexOf('  async function refreshAllCurrentQuotas()'), source.indexOf('  function updateRefreshButtonTitle()'));
for (const kind of ['oauth', 'api', 'devin', 'claude']) {
  const calls = [];
  const statuses = [];
  const button = {disabled: false};
  const accounts = Array.from({length: 8}, (_, n) => ({id: String(n), name: `A${n}`, endpoint: n === 7 ? 'unsupported' : 'supported', clientType: n === 7 ? 'desktop' : 'cli'}));
  let running = 0, maxRunning = 0;
  const query = async (agent, id) => {
    calls.push([agent, id]); running++; maxRunning = Math.max(maxRunning, running);
    await new Promise(resolve => setTimeout(resolve, 1)); running--;
    if (id === '1') throw new Error('fixture quota failure');
    return {ok: true};
  };
  const context = vm.createContext({activeKind:kind,document:{getElementById:()=>button},showStatus:(...args)=>statuses.push(args),
    fetchOauthAccounts:async()=>({accounts}),fetchApiAccounts:async()=>({accounts}),fetchDevinAccounts:async()=>({accounts}),
    detectUsagePlatform:endpoint=>endpoint==='supported',
    refreshAuthPresetQuota:(id)=>query('oauth',id),queryApiUsage:(account)=>query('api',account.id),
    WebClxDevinManager:{queryQuota:(account,force)=>{assert.equal(force,true);return query('devin',account.id);}},
    loadAccountsForKind:async agent=>assert.equal(agent,kind)});
  vm.runInContext(fn, context);
  if (kind === 'claude') {
    await context.refreshAllCurrentQuotas(); assert.equal(calls.length,0); continue;
  }
  const pending = context.refreshAllCurrentQuotas();
  await context.refreshAllCurrentQuotas(); // repeated click cannot submit a second batch
  await pending;
  assert.equal(calls.length,kind==='oauth'?8:7);
  assert.ok(calls.every(([agent])=>agent===kind));
  assert.ok(maxRunning<=4);
  assert.equal(statuses.at(-1)[1],'warn');
  assert.match(statuses.at(-1)[0],/失败 1/);
  assert.equal(button.disabled,false);
}
console.log('PASS refresh all quotas: tab scope, capability filtering, bounded concurrency, duplicate-click guard and partial failures');

// Initial unified account loading must work without the batch-only strict parameter.
{
  const statuses = [];
  let rendered = [];
  const context = vm.createContext({
    state: {claudePresets: []}, allAccounts: [],
    requestJson: async () => ({accounts: [], presets: []}),
    WebClxDevinManager: {loadAccounts: async (...args) => {
      assert.equal(args.length, 0);
      return [{id: 'devin-fixture', email: 'fixture@example.test', client_type: 'cli'}];
    }},
    orderCodexAccountsByPresetTables: accounts => accounts,
    renderAccounts: accounts => { rendered = accounts; },
    showStatus: (...args) => statuses.push(args),
  });
  const initialLoad = source.slice(source.indexOf('  async function loadCodexAccountsOnce()'), source.indexOf('  async function fetchOauthAccounts()'));
  vm.runInContext(initialLoad, context);
  await context.loadCodexAccountsOnce();
  assert.equal(rendered.length, 1);
  assert.equal(rendered[0].id, 'devin-fixture');
  assert.equal(rendered[0].kind, 'devin');
  assert.ok(statuses.every(([, tone]) => tone !== 'warn'));
}
console.log('PASS initial account loading with Devin manager present');
