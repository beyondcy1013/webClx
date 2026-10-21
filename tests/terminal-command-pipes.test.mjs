import assert from 'node:assert/strict';
import {readFileSync} from 'node:fs';
import vm from 'node:vm';
import {test} from 'node:test';
const ctx = vm.createContext({});
vm.runInContext(readFileSync('static/terminal-settings.js', 'utf8'), ctx);
vm.runInContext('const {normalizeTerminalFunctionCommands} = WebClxTerminalSettings;', ctx);
vm.runInContext(readFileSync('static/app-terminal-commands.js', 'utf8'), ctx);
const command = `! if command -v sensors >/dev/null 2>&1; then out="$(sensors)"; cpu="$(printf '%s\\n' "$out" | awk '/^Tctl:/ {print $2; exit}')"; printf '%s\\n' "$cpu"; fi`;
test('设置往返保留 shell 管道、引号、转义及快捷键', () => {
  for (const shortcut of ['', 'Ctrl+Alt+T']) {
    const input = [{key:'temperature', label:'系统温度', action:'send_text', command, shortcut}];
    const parsed = ctx.parseTerminalFunctionCommandsInput(ctx.formatTerminalFunctionCommands(input));
    assert.equal(parsed[0].command, command);
    assert.equal(parsed[0].shortcut, shortcut);
  }
});
test('兼容未引用的旧命令和多个管道', () => {
  for (const ending of ['', ' | ', ' | Ctrl+T']) {
    const result = ctx.parseTerminalFunctionCommandsInput(`temperature | 系统温度 | send_text | ${command}${ending}`)[0];
    assert.equal(result.command, command);
    assert.equal(result.shortcut, ending.includes('Ctrl') ? 'Ctrl+T' : '');
  }
});
