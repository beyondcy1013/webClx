# 终端拖选闪烁 + 选区高亮不可见

记录 2026-06-19 对“桌面 Chrome 鼠标拖动选区后，整个终端显示消失、只剩选区手柄和滚动条在闪烁”的排查，以及顺带定位的“选区有效但看不到蓝色反选高亮”问题。

## 现象

- 环境：Windows Server 2025 + Chrome 桌面浏览器，鼠标（非触摸）拖动选区。
- 终端 codes_1 中 Claude 退出后留下文本（`Press Ctrl-C again to exit` / `Resume this session with:` / `claude --resume <uuid>`），回到 shell prompt。
- 用户用鼠标拖动选中文字准备复制。有一定概率（有时正常），拖动释放后，整个终端显示区域变成空白，页面上**只剩下选区的 2 个手柄（.terminal-selection-handle）和页面滚动条（.terminal-page-scroll-rail）在闪烁**。
- 另一个相伴现象：鼠标拖选后选区数据有效、能正常复制，但**看不到蓝色反选高亮样式**。

## 一、拖选“整片消失只剩手柄滚动条闪烁”的链路

### 让整片 xterm 不可见的唯一前端机制

通过完整审计 `styles-terminal.css` / `styles.css` / `vendor/xterm.css`，让整个 `.xterm` 子树内容不可见的前端 CSS 唯一是：

```css
/* styles-terminal.css:665 */
.terminal-host.terminal-host-replaying .xterm { opacity: 0; }
```

选区手柄（`styles-terminal.css:578`）和页面滚动条（`:486`）在 HTML 结构上是 `.terminal-scroll-shell` 的子节点（`terminal.html:122-169`），**不在 `.xterm` 子树内**，所以 `opacity:0` 不影响它们——这精确解释了“整片空白 + 手柄/滚动条仍可见”。

### 该 class 只能由后端控制帧触发

`terminal-host-replaying` 只在 `beginTerminalBacklogReplay()`（`terminal.js:2509`）添加，而 `beginTerminalBacklogReplay()` 在整个文件里只被 `handleTerminalBacklogReplayControl`（`:2738`）调用，后者只在收到服务端 `terminal_backlog_replay {action:"start"}` 控制帧时执行（`:2743`）。**前端无法脱离后端控制帧自行进入 replaying 状态**（已穷尽验证：无 loadSessions 回调、page resume、重试、storage 事件路径会调用 `beginTerminalBacklogReplay`）。

后端 `handle_socket`（`src/terminal.rs:1776`）在**每次建立新 websocket 连接时**发一对 start（`:1814`）/ end（`:1829`）。因此前端要收到 replay start，必然经历过一次断连重连。

### 被证伪的假设：鼠标 SGR 报告写失败导致断连

最初假设“Claude 退出后 xterm 残留 SGR 鼠标模式 → 拖选高频发鼠标报告 → 后端写 PTY 失败 → 断连”。为此在后端加了 `is_mouse_only_input`（`src/terminal.rs`）对纯鼠标报告写入失败降级。**该降级已部署生效，但服务日志零次触发**，证伪了这条链路——前端大概率根本没向后端发鼠标报告（Claude 退出后鼠标模式已复位）。后端降级逻辑作为无害的健壮性改进保留。

### 真实证据：服务端确实有大量断连

排查时一个关键陷阱：**后端默认不打连接日志**，最初用 `grep mouse|reconnect` 看不到断连记录，误判为“无断连”。实际上应看 `WARN terminal websocket receive error`：

```
WARN webclx::terminal: terminal websocket receive error: WebSocket protocol error: Connection reset without closing handshake
WARN webclx::terminal: terminal websocket receive error: IO error: Connection reset by peer (os error 104)
```

这些 `Connection reset` 是真实断连。完整链路：**拖动（或连接抖动）→ websocket 断 → 前端 socket close（`terminal.js:~7505`）→ scheduleReconnect → connectTerminal → 后端重发 backlog replay start/end → `terminal-host-replaying` 反复 add/remove → `.xterm` opacity:0 闪烁**。

后端侧对应：`input_task`（`terminal.rs:1784`）收到 `Err`（`:1795`）→ `break`（`:1797`）→ 主循环退出。`Connection reset without closing handshake` 意味着 TCP 连接被异常重置（RST），通常是网络瞬断 / NAT/防火墙对空闲长连接 RST / 浏览器在后台或内存压力下回收 websocket / 远程会话焦点抖动，**而非 webClx 应用逻辑主动关闭**。

### 该问题的性质

这是**间歇性、环境敏感**的连接稳定性问题，不是稳定可复现的代码缺陷（排查末期已无法复现）。`Connection reset without closing handshake` 的诱因多在网络/浏览器侧。

### 一个独立的放大器：window focus → 7x fit 风暴

前端日志（临时 `[clxdiag]` 埋点）直接观察到：拖动期间 `window focus` 事件反复触发，每次都走 `refreshTerminalInputVisibilityAfterPageResume`（`terminal.js:~2466`）排了 **7 个 setTimeout（0/80/180/360/720/1200/1800ms）**，每个都 `refreshTerminalViewportLayout` → `fitTerminal`（`:7241`）→ `fitAddon.fit()` + resize，在连接不稳定时叠加放大单次断连的视觉闪烁。这是独立的次级路径，不是整片空白的根因（fit 风暴只让 xterm 反复重绘，不会让 host 高度塌缩——`syncTerminalHostHeight` 有 `Math.max(...,0)` 兜底，CSS 有 `min-height:500px`）。

## 二、诊断方法（可复用）

1. **判断是否断连**：看服务端 `journalctl -u webclx.service --since "<时间>" | grep -i "websocket receive error"`，关键词是 `Connection reset`，而不是 `reconnect`/`mouse`。
2. **判断断连是否导致 replay 闪烁**：前端在 `handleTerminalBacklogReplayControl` / `beginTerminalBacklogReplay` / `endTerminalBacklogReplay` / socket `close`/`open`/`error` / `connectTerminal` 加临时 `console.warn("[clxdiag] ...")` 埋点，复现后看 Console。定位后必须移除埋点。
3. **CSS 结构性判断**：若终端整片不可见但 `.terminal-scroll-shell` 里的手柄/滚动条还在，根因只能是 `terminal-host-replaying`（`opacity:0`），因此必然伴随后端 replay 控制帧 = 断连重连。
4. **注意 `term` 是模块作用域常量**：`terminal.js` 是 `<script type="module">`，外部 Console 无法直接访问 `term` 对象做 theme 拦截；诊断需用纯 DOM 观察或改代码加埋点。

## 三、可选的健壮性改进（未实施，待决定）

1. **连接稳定性**：websocket 加 ping/pong 心跳保活，防止中间网络设备对空闲连接发 RST。
2. **replay 不再整片隐藏 xterm**：让 backlog 重放改为“原位更新”而非“先 `opacity:0` 隐藏再显示”，这样即便断连重连也不会整片空白闪（改动可控，是消除闪烁观感最直接的一处）。
3. **减少 fit 风暴**：桌面（`hasPrimaryTouchInput=false`）环境下，`window focus` 触发的 7 次重绘对已连接终端收益低，可跳过或合并为单次。

## 四、选区高亮不可见（已定位根因）

现象：鼠标拖选后选区数据有效、能复制，但看不到蓝色反选高亮。

### 根因：xterm 用的是 DOM 渲染器，selection 颜色被 blend 到深色背景后过暗

- 这个项目 xterm 用 **DomRenderer**（`static/vendor/xterm.js` 里 `DomRenderer` 出现 6 次），不是 canvas 渲染器。DOM 渲染器的 selection 高亮靠注入的 CSS `.xterm-selection div { background-color: <selectionOpaque.css> }`。
- `selectionOpaque = color.blend(background, selectionTransparent)`（xterm 源码）。
- `--terminal-selection-bg: rgba(99, 179, 237, 0.55)`（`styles-base.css:18`）→ `theme.selection`（`terminal.js:633`）。
- 经 blend：`background=#0b1110`（接近黑）+ `rgba(99,179,237,0.55)` → `#3b6a8a`，对比度仅 **3.28:1**（接近“可见”下限），用户感觉“看不到蓝色”。
- 提高到 `alpha=0.85` → 对比度 **6.31:1**（明显）；`alpha=1.0` → **8.35:1**（明亮）。

这与 `docs/codex/index.md` 既有经验一致：“Keep `--terminal-selection-bg` mapped to `selection` and opaque enough that mouse-selected text remains visible after `mouseup`; the copy button can still work even when a too-transparent or ignored selection theme makes the selected range look gone.”（index.md 第 53 行）

### 修复方向

把 `--terminal-selection-bg` 的 alpha 从 0.55 提高到 0.85 左右（两个主题都改：`styles-base.css:18` 的 0.55 和 `:91` 的 0.58），让 blend 后的 `#3b6a8a`（对比度 3.28:1）提升到明显可见的水平。`terminalThemeFromCss` 读取链路无需改动。

## 五、2026-06-27 补充：选择期间每帧翻转 theme 的 cursor correction 路径

### 现象

仍是同一类场景：退出 Claude TUI 回到 shell 后，用鼠标拖选**包含最后两行命令提示符**的文本时，整个终端内容不显示且闪烁，“复制选中”按钮弹出、点击复制功能本身正常。同一操作有时不出现，**与所选内容相关**。

### 根因（与第四章的连接稳定性并列的另一条链路）

这是和 cursor correction 强相关的链路，独立于第四章的 websocket 断连：

1. `term.onRender`（`terminal.js`）每次重绘都跑 `scheduleTerminalCursorCorrection`。
2. 鼠标拖选时 xterm 每帧 render 选区高亮 → 每帧都重新跑 `detectBottomStatusCursorCorrection`。
3. 退出 TUI 回到 shell 后，若视口底部三行恰好满足误判（最后一行是含交互词的 shell prompt，倒数第二行空，倒数第三行是 TUI 残留的 `›` 输入行，且光标停在最后一行 `cursorRow===rows-1`），守卫会返回一个错误 target。
4. `setTerminalCursorHiddenForCorrection` 通过 `term.options.theme = terminalThemeFromCss()` 翻转光标显隐；`terminalThemeFromCss()` 每次返回**全新对象**，xterm 检测到 theme 引用变化就触发整屏重绘。
5. 每帧翻转 → 整屏反复重绘 = 闪烁/内容消失。

这解释了“选最后两行才出错”“有时正常”——取决于底部三行布局是否误命中。

### 修复

两层修复，互相独立、互为纵深：

- **选择期间冻结纠偏**：`static/terminal.js` 新增 `terminalSelectionBlockingCursorCorrection()`（存在选区或选区手柄拖拽时为真）；`syncTerminalCursorCorrection()` 开头命中则直接 return，不再改 theme；`term.onSelectionChange` 在选区清除后补一次 `scheduleTerminalCursorCorrection()`，让纠偏在选择结束后恢复。这样拖选期间无论 buffer 内容如何误判，都不会再每帧翻转 theme。
- **收紧守卫误判**：`static/terminal-cursor-guard.js` 新增 `isLikelyShellPrompt()`，识别 `[root@host dir]#`、`user@host:path$`、`# `/`$ `/`> ` 等 shell 提示符；`detectBottomStatusCursorCorrection` 在判定 status line 后增加 `isLikelyShellPrompt(statusLine)` 排除，避免把 shell prompt 当成 Codex 状态行。

### 验证

- `node tests/terminal-cursor-guard.test.mjs`：新增 stale `›` 输入行 + shell prompt 底行的回归用例，以及选择抑制逻辑的结构断言。
- 手动：退出 Claude/Codex TUI 回到 shell，鼠标拖选包含最后两行 prompt 的文本，应能正常显示选区、正常复制、无整片闪烁消失；选区清除后 Codex TUI 输入纠偏仍正常。

## 六、2026-07-21 补充：Codex 状态表覆盖层拖选时被重绘

`terminal-codex-status-compact.js` 把 Codex 状态块重排为可选中的 DOM 覆盖层。鼠标按下后到形成浏览器选区前存在短暂窗口；若此时 xterm `onRender` 或状态请求触发扫描，`replaceChildren()` / `removeOverlay()` 会删掉正在拖选的节点，表现为表格看得到但无法复制。

覆盖层必须在主键 `pointerdown` 后立即冻结整个扫描/渲染路径，不仅是跳过 `replaceChildren()`；`pointerup` / `pointercancel` 后，若仍有落在覆盖层内的浏览器选区则继续冻结，选区清除后才恢复刷新。

## 七、2026-07-26：停用 Codex 状态覆盖层

终端页不再加载 `terminal-codex-status-compact.js`，终端实例也不再注册 DOM status compactor。`terminal-codex-status-output.js` 在 WebSocket 字节写入 xterm 前识别 Codex 原生 `/status` 边框块，只用块内已有字段生成更少行数的紧凑文本；不再混入 `agent-session` 的另一份结构化状态，所以页面只会出现一份结果。

转换器必须跨 WebSocket 分块保留 UTF-8、ANSI 和原始 CRLF。backlog replay 结束时，任何未完成或无法确认的候选块都要原样 flush，禁止吞输出。最终字符直接进入 xterm buffer，因此复制、搜索和滚动历史与画布一致，不创建任何额外页面层。该转换只影响 webClx 浏览器端显示；tmux 原始历史和其它终端客户端仍保留 Codex 原文。`agent-session` 接口继续供高级 Session 提取等非展示流程使用。

紧凑输出按最长实际内容收缩边框，但不超过 Codex 原生状态宽度。字段使用 `Model: value` 形式，不为标签对齐填充空格或增加纵向分隔列。`Collaboration mode` 缩写为独立的 `Mode` 行，避免与 `Access` 拼成长行。`Directory` 和 `Agents.md` 出现多个逗号分隔路径时只拆成两行：首行保留标签和结尾逗号，第二行直接续写其余路径。原生状态块左侧只有 ANSI 控制码与空白时，去掉空白缩进但保留 ANSI，让紧凑边框从终端第 1 列开始。
