// 标准终端按键编码；不执行浏览器或操作系统快捷键。
function terminalShortcutSequence(key, { ctrl = false, alt = false, shift = false, meta = false } = {}, applicationCursor = false) {
  const modifier = 1 + Number(shift) + 2 * Number(alt) + 4 * Number(ctrl) + 8 * Number(meta);
  const finals = { ArrowUp: 'A', ArrowDown: 'B', ArrowRight: 'C', ArrowLeft: 'D', Home: 'H', End: 'F', F1: 'P', F2: 'Q', F3: 'R', F4: 'S' };
  if (finals[key]) {
    if (modifier > 1) return `\x1b[1;${modifier}${finals[key]}`;
    return `\x1b${applicationCursor || /^F/.test(key) ? 'O' : '['}${finals[key]}`;
  }
  const tilde = { Insert: 2, Delete: 3, PageUp: 5, PageDown: 6, F5: 15, F6: 17, F7: 18, F8: 19, F9: 20, F10: 21, F11: 23, F12: 24 };
  if (tilde[key]) return `\x1b[${tilde[key]}${modifier > 1 ? `;${modifier}` : ''}~`;
  const special = { Enter: '\r', Tab: '\t', Escape: '\x1b', Backspace: '\x7f', Space: ' ' };
  let value = special[key] ?? key;
  if ([...value].length !== 1) throw new Error('请选择一个按键。');
  if (key === 'Tab' && shift && !ctrl && !meta) return `${alt ? '\x1b' : ''}\x1b[Z`;
  const shifted = { '`': '~', '1': '!', '2': '@', '3': '#', '4': '$', '5': '%', '6': '^', '7': '&', '8': '*', '9': '(', '0': ')', '-': '_', '=': '+', '[': '{', ']': '}', '\\': '|', ';': ':', "'": '"', ',': '<', '.': '>', '/': '?' };
  if (shift && !special[key]) value = shifted[value] || value.toUpperCase();
  if (meta) return `\x1b[27;${modifier};${value.codePointAt(0)}~`;
  if (ctrl) {
    const code = value.toUpperCase().charCodeAt(0);
    if (code >= 64 && code <= 95) value = String.fromCharCode(code - 64);
    else if (value === ' ' || value === '2') value = '\x00';
    else if (value === '?' || value === '8') value = '\x7f';
    else if (value >= '3' && value <= '7') value = String.fromCharCode(Number(value) + 24);
    else return `\x1b[27;${modifier};${value.codePointAt(0)}~`;
  }
  if (shift && special[key] && key !== 'Space') {
    return `\x1b[27;${modifier};${(special[key] ?? value).codePointAt(0)}~`;
  }
  return `${alt ? '\x1b' : ''}${value}`;
}

function initializeTerminalUniversalShortcut() {
  const trigger = document.getElementById('terminal-universal-shortcut');
  const dialog = document.getElementById('terminal-shortcut-dialog');
  const form = document.getElementById('terminal-shortcut-form');
  if (!trigger || !dialog || !form) return;
  const select = form.elements.key;
  const preview = document.getElementById('terminal-shortcut-preview');
  const status = document.getElementById('terminal-shortcut-status');
  const groups = {
    '控制与导航': ['Enter', 'Tab', 'Escape', 'Backspace', 'Space', 'ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Home', 'End', 'Insert', 'Delete', 'PageUp', 'PageDown'],
    '字母': [...'abcdefghijklmnopqrstuvwxyz'],
    '数字': [...'0123456789'],
    '符号': [...'`-=[]\\;\',./~!@#$%^&*()_+{}|:"<>?'],
    '功能键': Array.from({ length: 12 }, (_, index) => `F${index + 1}`),
  };
  for (const [label, keys] of Object.entries(groups)) {
    const group = document.createElement('optgroup');
    group.label = label;
    for (const key of keys) group.append(new Option(key, key));
    select.append(group);
  }
  const keyboard = document.getElementById('terminal-shortcut-keyboard');
  const labels = { Escape: 'Esc', Backspace: '退格', Enter: 'Enter', Space: '空格', ArrowUp: '↑', ArrowDown: '↓', ArrowLeft: '←', ArrowRight: '→', PageUp: 'PgUp', PageDown: 'PgDn', Insert: 'Ins', Delete: 'Del' };
  const rows = [
    ['Escape', ...Array.from({ length: 12 }, (_, index) => `F${index + 1}`)],
    [...'`1234567890-=', 'Backspace'],
    ['Tab', ...'qwertyuiop[]', '\\'],
    [..."asdfghjkl;'", 'Enter'],
    ['shift', ...'zxcvbnm,./'],
    ['ctrl', 'meta', 'alt', 'Space'],
    ['Insert', 'Home', 'PageUp', 'Delete', 'End', 'PageDown'],
    ['ArrowLeft', 'ArrowUp', 'ArrowDown', 'ArrowRight'],
  ];
  const shiftedLabels = Object.fromEntries([...'`1234567890-=[]\\;\',./'].map((key, index) => [key, [...'~!@#$%^&*()_+{}|:"<>?'][index]]));
  const keyButtons = [];
  for (const keys of rows) {
    const row = document.createElement('div');
    row.className = 'terminal-shortcut-keyboard-row';
    for (const key of keys) {
      const button = document.createElement('button');
      button.type = 'button';
      button.dataset.key = key;
      button.className = 'terminal-shortcut-key';
      const isModifier = ['ctrl', 'alt', 'shift', 'meta'].includes(key);
      button.textContent = labels[key] || (isModifier ? { ctrl: 'Ctrl', alt: 'Alt', shift: 'Shift', meta: 'Meta' }[key] : key.toUpperCase());
      button.setAttribute('aria-label', button.textContent);
      button.setAttribute('aria-pressed', 'false');
      if (['Backspace', 'Tab', 'Enter', 'shift'].includes(key)) button.classList.add('terminal-shortcut-key-wide');
      if (key === 'Space') button.classList.add('terminal-shortcut-key-space');
      button.addEventListener('click', () => {
        if (isModifier) form.elements[key].checked = !form.elements[key].checked;
        else select.value = key;
        refresh();
      });
      keyButtons.push(button);
      row.append(button);
    }
    keyboard.append(row);
  }
  let sessionId = '';
  const modifiers = () => Object.fromEntries(['ctrl', 'alt', 'shift', 'meta'].map(name => [name, form.elements[name].checked]));
  const refresh = () => {
    const selected = modifiers();
    preview.textContent = [...Object.keys(selected).filter(name => selected[name]).map(name => ({ ctrl: 'Ctrl', alt: 'Alt', shift: 'Shift', meta: 'Meta' })[name]), select.value].join(' + ');
    for (const button of keyButtons) {
      const key = button.dataset.key;
      button.setAttribute('aria-pressed', String(key in selected ? selected[key] : key === select.value));
      if (key.length === 1) button.textContent = selected.shift ? (shiftedLabels[key] || key.toUpperCase()) : key.toUpperCase();
    }
    status.textContent = '';
  };
  trigger.addEventListener('click', () => {
    closeTerminalFunctionCommandMenu();
    sessionId = state.activeSessionId;
    if (!sessionId) { updateStatus('请先选择终端。', 'error'); return; }
    refresh();
    dialog.showModal();
  });
  dialog.addEventListener('close', () => {
    if (sessionId === state.activeSessionId) focusTerminalAfterSoftKeyboardInput();
  });
  form.addEventListener('change', refresh);
  document.getElementById('terminal-shortcut-cancel').addEventListener('click', () => dialog.close());
  form.addEventListener('submit', event => {
    event.preventDefault();
    try {
      const applicationCursor = sessionId === state.activeSessionId && term?.modes?.applicationCursorKeysMode;
      const data = terminalShortcutSequence(select.value, modifiers(), applicationCursor);
      if (!sendTerminalInputToSession(data, sessionId)) throw new Error('目标终端已断开，请连接后重试。');
      dialog.close();
    } catch (error) { status.textContent = error.message; }
  });
}

document.addEventListener('DOMContentLoaded', initializeTerminalUniversalShortcut);
