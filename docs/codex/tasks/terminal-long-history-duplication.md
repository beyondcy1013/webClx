# 终端长历史重复和顶部历史丢失

日期：2026-05-15

## 现象

- Codex 在网页终端里输出很长历史后，向上翻页会看到一大片重复内容。
- 程序持续输出时，浏览器端上方较早的行会被逐步顶掉。

## 根因

网页终端连接已有 tmux 会话时，会先用 `tmux capture-pane -S -` 抓一份历史快照并回放给 xterm，然后再通过 `tmux attach-session` 接收实时输出。

`tmux attach-session` 启动时还会发送当前屏幕的初始重绘。旧逻辑只丢弃第一段重绘 chunk；当重绘分成多段，或回放长历史期间积压后续 chunk 时，当前屏幕会再次进入 xterm scrollback，表现为大块重复。

持续输出挤掉顶部历史则来自历史容量上限：前端 xterm 和 tmux 的历史容量都会决定可回看的范围。

## 修复

- `src/terminal/session.rs`：只有存在 tmux 快照时才启用初始重绘抑制，并在短窗口内丢弃整批初始重绘 chunk，而不是只丢第一段。
- `static/terminal-shell-settings.js`：xterm `scrollback` 读取设置项 `terminal_scrollback_lines`；默认 `5000` 行，可在 `100..100000` 之间调整，保存后会应用到已打开和新建的浏览器终端。
- `src/terminal/tmux.rs`：每次创建或复用 tmux 会话时设置 `history-limit` 为 100000。
- `src/terminal.rs`：后端 byte backlog 提高到 32 MiB，作为非 tmux 快照或回退路径的缓冲。

## 验证

```bash
node tests/terminal-backlog-replay.test.mjs
for test_file in tests/*.test.mjs; do node "$test_file" || exit 1; done
cargo test terminal -- --nocapture
cargo test
```

## Codex 备用屏幕不积累 scrollback（2026-09-26）

现象：部分 Codex 终端在 webClx 独立页只能看到当前一屏，向上翻几乎没有历史。

根因：Codex TUI 默认进入 tmux alternate screen（`alternate_on=1`、
`history_size≈0`），输出从不写入 scrollback；`capture-pane -S -N` 首屏回放只能
拿到当前屏幕。devin/agy/codebuddy 等非全屏程序不受影响。已丢失的历史无法找回。

修复：在终端用户真实 `~/.codex/config.toml` 的 `[tui]` 段加
`alternate_screen = "never"`（字符串枚举，布尔 `false` 会被拒：
`invalid type: unit variant, expected string only`）。等价于每次启动加
`--no-alt-screen`，resume/fork 同样受 config 覆盖。验证：新开 tmux 里
`codex --no-daemon` 后 `alternate_on=0`，transcript 滚动即积累
`history_size`；`--strict-config` 只查字段名，不校验该枚举取值，须以实际启动为准。

该开关已并入 Codex 公共配置：`src/config_files.rs` 的
`codex-common-config` API 增加 `alt_screen_disabled`（Option，旧前端不携带时
保持现状），设置页"Codex 公共配置"对话框与账号工具栏均有"内联模式"勾选。
取消勾选仅当当前值恰为 `"never"` 时移除该键，保留用户手写的其他取值。
