import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';
import { test } from 'node:test';

const context = vm.createContext({ document: { addEventListener() {} } });
vm.runInContext(readFileSync(new URL('../static/terminal-universal-shortcut.js', import.meta.url), 'utf8'), context);
const encode = context.terminalShortcutSequence;
test('终端组合键与控制字符', () => {
  assert.equal(encode('c', { ctrl: true }), '\x03');
  assert.equal(encode('m', { alt: true }), '\x1bm');
  assert.equal(encode('x', { ctrl: true, alt: true }), '\x1b\x18');
  assert.equal(encode('a', { shift: true }), 'A');
  assert.equal(encode('1', { shift: true }), '!');
  assert.equal(encode('Space', { ctrl: true }), '\x00');
  assert.equal(encode('[', { ctrl: true }), '\x1b');
  assert.equal(encode('Tab', { shift: true }), '\x1b[Z');
  assert.equal(encode('Enter'), '\r');
  assert.equal(encode('Backspace'), '\x7f');
  assert.equal(encode('Escape'), '\x1b');
});
test('方向键、编辑键和功能键遵循终端序列', () => {
  assert.equal(encode('ArrowUp'), '\x1b[A');
  assert.equal(encode('ArrowUp', {}, true), '\x1bOA');
  assert.equal(encode('ArrowLeft', { ctrl: true, shift: true }), '\x1b[1;6D');
  assert.equal(encode('Delete', { ctrl: true, alt: true }), '\x1b[3;7~');
  assert.equal(encode('F1'), '\x1bOP');
  assert.equal(encode('F12', { shift: true }), '\x1b[24;2~');
  assert.equal(encode('Enter', { shift: true }), '\x1b[27;2;13~');
  assert.equal(encode('a', { meta: true }), '\x1b[27;9;97~');
  assert.throws(() => encode('invalid'), /请选择/);
});
