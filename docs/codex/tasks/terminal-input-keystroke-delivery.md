# 终端按键投递与退格失效

## 现象

部分终端运行 CodeBuddy 后，输入框里的文字删掉一部分就再也删不动；继续按退格没有任何反应，只有再敲一个普通字符后才可能恢复。

## 根因

基于 ink 的 Agent TUI 把**一次 stdin 读取当成一次按键**。当一次读取包含多于一个字节时，它按“粘贴文本”处理，其中的退格字节（DEL `0x7f` / BS `0x08`）被整段丢弃：

- 单字节读取 `\x7f`：识别为退格，正常删除。
- 双字节读取 `\x7f\x7f` 或 `a\x7f`：删除字符数为 0，`a` 照常插入。

webClx 会把连续按键合并成一次写入（前端 8ms 批量 + 服务端一次 `write_all` / 一次 `tmux send-keys`）。只要两次退格落进同一次写入，Agent 就一个都不删；按住退格或快速连按时合并概率更高，于是表现为“删着删着就删不动了”。

## 实测证据（tmux + codebuddy v2.170.0）

| 投递方式 | 6 次退格后剩余 |
| --- | --- |
| 一次写入 `\x7f` × 6 | 一个都没删 |
| 一次写入 `a\x7f` | 只插入 `a`，不删除 |
| 6 个独立 `tmux send-keys` 进程、无间隔 | 只删 2 个 |
| 6 个独立写入、间隔 1ms | 10 次中只删 5 个 |
| 6 个独立写入、间隔 ≥2ms | 全部删除 |

同样结论在 webClx 实际使用的路径（PTY 写入 tmux attach 客户端）上复现：一次写入 6 个 DEL 删除 0 个，间隔 6ms 逐个写入全部删除。

## 修复

新增 `src/terminal/input_pacing.rs`：把输入按退格字节切成多段，每段单独写一次，段间留 `ERASE_KEY_WRITE_GAP`（5ms，实测 1ms 仍会合并、2ms 起不再合并，取 5ms 留余量）。三条输入写入路径统一改用它：

- `TerminalSession::write_input`：浏览器 WebSocket 输入（PTY 写入 tmux attach 客户端）。
- `TerminalManager::write_session_input_inner` 的 live session 分支（Windows / 直连 PTY）。
- `tmux::send_tmux_input`：HTTP 输入 API、定时消息、移动端按键走的 `tmux send-keys` 路径，逐段发送。

普通文本仍然一次写完，不受影响；只有出现退格键时才会分段并产生间隔。

## 验证

同一段 WebSocket 输入（一次 `abcdef`、一次 6 个退格）打到读取分块探针上，修复前后对比：

| 版本 | 探针输出 |
| --- | --- |
| 修复前 | `CHUNK len=6 data=abcdef` / `CHUNK len=6 data=\x7f\x7f\x7f\x7f\x7f\x7f` |
| 修复后 | `CHUNK len=6 data=abcdef` / 6 条 `CHUNK len=1 data=\x7f` |

真实 CodeBuddy 终端（同一 WS 路径）：输入 `abcdef` 后一次投递 6 个退格，输入框回到空行 `>`；修复前同样操作文字保持不变。

## 边界

- 这是时序缓解，不是上游修复：Agent 若长时间繁忙（例如正在流式输出），仍可能把多次写入合并成一次读取而丢退格。真正的修复应由 Agent 的输入解析按字节/按键拆分，而不是按读取批次。
- 同类风险适用于其它 C0 控制字节（Ctrl-A..Ctrl-Z）：它们混进多字节读取时同样无法被识别为组合键。前端已对 `\r`、`\n`、`\x03`、`\x04` 立即冲刷队列，其余控制键若出现同类症状，可按同一模式扩展分段规则。
- 分段间隔会让连续退格按 5ms/次节流；按住退格（系统重复约 30 次/秒）仍在预算内，长串退格投递会略慢于即时写入。
