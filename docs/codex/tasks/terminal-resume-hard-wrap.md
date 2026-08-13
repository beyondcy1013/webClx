# 软键盘恢复命令硬换行解析

日期：2026-05-15

## 现象

移动软键盘的 `extract_resume` 功能命令会从终端 buffer 中提取最近的 `codex resume <id>` 并发送。有时 resume 命令换行后能恢复，有时会发送半截 ID。

注意：底部固定“恢复”键现在用于恢复系统键盘，不再触发 Codex resume 提取。

## 根因

- xterm 软换行会设置 `line.isWrapped`，`readTerminalBufferTailText()` 会把这类行拼回上一行。
- 如果输出本身包含硬换行，`line.isWrapped` 不会生效。
- 旧正则在硬换行前就截断 token，并且旧 ID 校验允许 1 到 160 个字符，所以 `019d...a391-` 这样的半截 UUID 也会被当成合法 ID。

## 修复

- 新增 `static/terminal-resume-extract.js`，把 resume ID 提取逻辑抽成可测试 helper。
- 提取时优先识别 Codex 当前使用的 36 位 UUID，并允许 UUID 片段之间存在少量空白/换行。
- 保留非 UUID 单行 token 兼容，但不再让硬换行 UUID 被半截截断。
- 如果终端下方还有用户手输的最新半截 `codex resume` / `claude --resume` UUID，提取器必须跳过这个不完整 UUID 片段，继续向前找最近的完整 resume 命令；否则半截输入会覆盖上方 `Resume this session with:` 给出的完整命令。
- 纯十六进制 UUID 短前缀也要视为不完整片段，例如用户误执行 `codex resume 019ee6` 后，快捷键恢复应跳过这个失败命令，继续回退到上方完整的 36 位 Codex UUID。
- Codex 也可能输出 `run codex resume, then select <说明文字> (<UUID>)`。前端 buffer 提取和后端 tmux 快照解析都应允许命令与 UUID 之间存在说明文字，并重组 UUID 内部的硬换行。扫描必须限制在有限范围内，不能跨越下一条 `codex resume` / `claude --resume` 命令借用其 UUID；找到规范 UUID 后，后续失败的 `codex resume then` 也不得用普通 token 覆盖它。

## 验证

```bash
node tests/terminal-resume-extract.test.mjs
for test_file in tests/*.test.mjs; do node "$test_file" || exit 1; done
```
