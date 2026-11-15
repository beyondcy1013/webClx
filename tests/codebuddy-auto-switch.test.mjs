import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const source = readFileSync(new URL('../static/app-codebuddy-manager.js', import.meta.url), 'utf8');
const html = readFileSync(new URL('../static/index.html', import.meta.url), 'utf8');

function harness(auto = { checkin: false, travel: true }) {
  const elements = new Map();
  for (const id of ['codebuddy-auto-checkin', 'codebuddy-auto-travel', 'codebuddy-auto-block-international', 'codex-account-status']) {
    elements.set(id, {
      checked: true, dataset: {}, listeners: {},
      addEventListener(event, handler) { this.listeners[event] = handler; },
    });
  }
  const state = { auto: { ...auto }, writes: [], failSave: false };
  const window = {
    addEventListener(event, handler) { if (event === 'DOMContentLoaded') handler(); },
  };
  vm.runInNewContext(source, {
    window,
    document: { getElementById: (id) => elements.get(id) },
    requestJson: async (url, options) => {
      assert.equal(url, '/api/auth/codebuddy-auto');
      if (options?.method === 'PUT') {
        if (state.failSave) throw new Error('save failed');
        const body = JSON.parse(options.body);
        state.writes.push(body);
        Object.assign(state.auto, body);
      }
      return { auto: { ...state.auto }, accountCount: 2 };
    },
  });
  return { state, elements, manager: window.WebClxCodeBuddyManager };
}

test('new policy defaults to checked in HTML and when the server has no saved field', async () => {
  assert.match(html, /id="codebuddy-auto-block-international"[^>]*\bchecked\b/);
  const h = harness();
  h.elements.get('codebuddy-auto-block-international').checked = false;
  await h.manager.refreshAutoBar();
  assert.equal(h.elements.get('codebuddy-auto-block-international').checked, true);
  assert.equal(h.elements.get('codebuddy-auto-checkin').checked, false);
});

test('changing the checkbox saves explicit false and restores the saved selection', async () => {
  const h = harness();
  await h.manager.refreshAutoBar();
  const box = h.elements.get('codebuddy-auto-block-international');
  box.checked = false;
  await box.listeners.change();
  assert.deepEqual(h.state.writes, [{ checkin: false, travel: true, blockInternationalAutoSwitch: false }]);
  assert.equal(box.checked, false);
  box.checked = true;
  await h.manager.refreshAutoBar();
  assert.equal(box.checked, false);
  box.checked = true;
  await box.listeners.change();
  assert.equal(h.state.auto.blockInternationalAutoSwitch, true);
});

test('changing an existing automation switch preserves the international policy', async () => {
  const h = harness({ checkin: true, travel: true, blockInternationalAutoSwitch: false });
  await h.manager.refreshAutoBar();
  h.elements.get('codebuddy-auto-travel').checked = false;
  await h.elements.get('codebuddy-auto-travel').listeners.change();
  assert.deepEqual(h.state.auto, { checkin: true, travel: false, blockInternationalAutoSwitch: false });
});

test('a rejected save reloads the persisted policy', async () => {
  const h = harness({ checkin: false, travel: true, blockInternationalAutoSwitch: true });
  await h.manager.refreshAutoBar();
  h.state.failSave = true;
  const box = h.elements.get('codebuddy-auto-block-international');
  box.checked = false;
  await box.listeners.change();
  assert.equal(box.checked, true);
  assert.equal(h.state.writes.length, 0);
});
