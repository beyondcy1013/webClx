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

## AGY 屏幕恢复命令

- `Ctrl+Alt+R` 的 `extract_resume` 动作通过统一提取器识别 `agy --conversation=<UUID>`，发送时保留 AGY 命令，不转换成 Codex。
- 同时接受空格分隔参数、带引号 UUID 和 UUID 硬换行；多个工具命令按最近完整 UUID 选择，跳过末尾半截 AGY UUID。Codex 的说明文字扫描遇到 AGY 命令时停止，避免借用其它工具的会话 ID。
- 入口资源版本：`terminal-resume-extract.js?v=20260916a`。当前 `static` 是运行目录的符号链接，资源修改直接生效；Git 仅跟踪该链接，本次回归测试和说明纳入提交。

## Devin 屏幕恢复命令

- `Ctrl+Alt+R` 提取器识别 `devin -r <slug>` 与 `devin --resume <slug>`，发送时统一改写为 `devin -r <slug>`。slug 只允许 ASCII 字母数字按连字符分段（每段非空、至少一个连字符、≤160 字符）；选项与 slug 之间只接受同行空白，不跨硬换行借参数；引号/反引号按既有清洗规则剥离。
- Devin slug 与 UUID 同级按出现位置取最新：更新的 UUID 仍优先，更新的 Devin slug 也能盖过旧 UUID；裸 `devin -r` 选择器、`devin -r lists recent sessions` 这类说明文字、悬挂连字符、缺失参数和 shell 元字符一律不匹配，也不会覆盖更早的有效命令。
- Codex/Claude 说明文字的 UUID 上下文扫描遇到 `devin -r` / `devin --resume` 即停止，不借用其后的 UUID；后端 `extract_resume_info_from_text` 对 Devin 走直接参数分支（要求 slug 与命令同行），同样不借下游 UUID。
- 未加命令标签的裸 slug 仍按既有逻辑解释为 Codex resume id，不做 Devin 推断。
- 入口资源版本：`terminal-resume-extract.js?v=20261007-devin`。
