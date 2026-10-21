# Codex 终端闪屏和输入法跳动

日期：2026-05-12

## 现象

- Codex 工作时网页终端整屏闪烁。
- 使用系统输入法输入时，输入法窗口闪烁。
- Codex 输入行光标仍会前后跳动。

## 根因

`static/terminal-cursor-guard.js` 的 Codex 光标纠偏识别过宽，把 `working`、`thinking` 等忙碌状态也当成交互输入状态。

纠偏启停会通过 `term.options.theme` 隐藏/恢复 xterm 原生光标；在 Codex 忙碌状态频繁重绘时，这会触发整块 xterm 画布反复重绘。系统输入法又跟随 xterm helper textarea 的原生光标位置，因此表现为输入法闪烁和光标跳动。

## 修复

- `static/terminal-cursor-guard.js`：只在真正的交互提示状态启用纠偏，并排除 `working/thinking/loading/running/executing/generating/streaming` 等忙碌状态。
- `static/terminal.js`：`onRender`/`onScroll` 的纠偏同步改为 `requestAnimationFrame` 合并，减少渲染回调内连续改 DOM/主题。
- `static/terminal.html`：更新 `terminal.js` 和 `terminal-cursor-guard.js` 缓存版本号。

## 2026-05-31 补充

现象：当 Codex 输入行的真实编辑光标在文本中间时，输入行末尾还会出现另一个闪烁光标。

根因：旧纠偏逻辑只根据“xterm 光标停在底部状态行”补画输入行末尾光标；但 Codex 有时已经在输入行用反显单元格画出了真实应用光标。此时末尾补画光标就是第二个假光标。

修复：`static/terminal.js` 从输入行 xterm 单元格属性中检测反显应用光标，传给 `static/terminal-cursor-guard.js`；守卫发现输入行已有应用光标时不再启用末尾纠偏。

## 2026-06-06 补充

现象：从活动终端下拉框切换到另一个终端后，页面滚动已基本稳定，但当前光标仍会乱闪。

根因：会话切换会执行 `term.reset()`，随后 websocket 重放历史输出。重放中间帧不是稳定的 Codex 输入状态，但旧逻辑仍在每个 `term.write()` 回调里同步光标纠偏，导致 xterm 原生光标隐藏/恢复和补画光标状态在重放过程中被中间帧反复驱动。

修复：`static/terminal.js` 在 `beginTerminalBacklogReplay()` 进入重放时先清掉补画光标；重放期间跳过 `syncTerminalCursorCorrection()`，只在 `endTerminalBacklogReplay()` 的最终稳定帧恢复一次纠偏。

## 2026-06-09 补充

现象：Codex 运行结束回到空输入状态后，光标有时在 `Explain this codebase` 这类 placeholder 前后跳。

根因：Codex 0.137.0 会把空输入 placeholder 画在 `›` 后面，真实编辑光标仍在输入起点。旧纠偏只按输入行可见长度补画光标，把 faint/dim placeholder 当成真实输入内容，因此补画光标会落到 placeholder 末尾。

修复：`static/terminal.js` 从 xterm 单元格属性中识别输入行 faint/dim placeholder 范围并传给 `static/terminal-cursor-guard.js`；守卫计算补画列时忽略 placeholder，只按真实输入内容长度定位。当前 xterm vendor 中 faint/dim 位实测在 `cell.bg` 的 `0x8000000`，反显应用光标仍在 `cell.fg` 的 `0x4000000`。

## 2026-06-21 补充

现象：在 Codex 交互输入中按方向上键召回上一条历史，输入实际有效，但有时显示为黑色或近似不可见。

根因：xterm 默认 ANSI black/brightBlack 颜色接近 webClx 的深色终端背景。Codex TUI 在历史输入、placeholder 或弱化文本中会使用暗色 ANSI，回放到网页终端后就变成“内容存在但看起来发黑”。

修复：`static/styles-base.css` 增加 `--terminal-ansi-black` / `--terminal-ansi-bright-black`，`static/terminal.js` 把它们传入 xterm theme，并启用 `minimumContrastRatio: 4.5`。不要只改普通 `--terminal-fg`，因为 ANSI 30/90 色不会走默认前景色。

## 2026-06-21 尺寸同步补充

现象：Codex 运行中动态状态区显示不全、`working` 行和数字/中文残留混在一起。

根因：浏览器端窄视口已经软换行，但 tmux pane 仍保持旧的大宽度（例如 185 列），Codex TUI 按旧列宽清理/重绘状态行，导致浏览器端换行后的残留行没有被清掉。

修复：`static/terminal.js` 在 WebSocket 打开、字体/设置变化、软键盘切换、窗口/visualViewport 变化和页面恢复时安排短暂的多帧 `fitTerminal({ force: true })` settle，让最终稳定后的列/行数再次发到后端 PTY/tmux。

## 2026-06-22 软键盘光标闪烁补充

现象：移动软键盘输入时，为避免系统键盘弹出，终端不会 refocus xterm helper textarea；xterm 原生光标因此可能停在非闪烁状态，用户不容易判断输入位置。

修复：`static/terminal.js` 增加独立的软键盘视觉光标覆盖层，只在软键盘打开且系统 IME 关闭时用 xterm buffer 的 `cursorX/cursorY` 和 cell 尺寸定位。该覆盖层不调用 `term.focus()`，不改 helper textarea 焦点，也不改变系统键盘状态。`static/styles-terminal.css` 的软键盘光标动画在灭的半周期使用 `--terminal-bg` 覆盖底下可能存在的实心原生光标。

注意：Codex 输入行纠偏光标优先级更高；纠偏光标存在时应隐藏软键盘视觉光标，避免双光标。

## 2026-07-26 软键盘不干预系统键盘状态

目标：软键盘不得主动弹出系统键盘；如果系统键盘已经弹出，软键盘也不得关闭它。软键盘交互应完全保留当前系统键盘状态。

根因：`focusTerminalAfterSoftKeyboardInput()` 主动调用 `setTerminalSystemImeEnabled(false)`，因此系统键盘已经打开时，任意软键盘 pointer、touch、keyboard 或 click 交互都会 blur xterm helper textarea 并关闭系统键盘。

修复：软键盘容器继续捕获并抑制会触发原生输入的事件，但共享入口不再调用系统 IME 的开启、关闭、focus 或 blur 路径，只根据现有状态同步 `inputmode` 与软键盘视觉光标。系统键盘已打开时保持打开，已关闭时保持关闭；只有显式系统键盘控制可以改变该状态。

### 2026-07-27 回归门禁

回归原因不是共享入口被改回，而是新增“全能”按钮和悬浮菜单位于旧的 `data-action` 按钮选择器及软键盘容器之外；Escape 关闭菜单时还显式 focus 触发按钮。系统键盘已打开时，这些新控件会夺走 xterm helper textarea 焦点并关闭输入法。

所有软键盘命令面必须复用同一焦点门禁，包括脱离 `#terminal-mobile-keys` 的悬浮菜单和以后动态生成的按钮：交互开始前记录 helper 是否已聚焦；除“系统键盘”显式开关外，阻止按钮和 checkbox 的原生聚焦；若浏览器仍产生 `focusin`，原先已聚焦则立即恢复 helper，原先未聚焦则 blur 新控件。菜单 Escape 关闭不得 focus 触发按钮。新增软键盘控件时，`tests/terminal-ime-policy.test.mjs` 必须先覆盖该控件，不能只检查 `focusTerminalAfterSoftKeyboardInput()`。

## 验证

```bash
node tests/terminal-readable-ansi-colors.test.mjs
node tests/terminal-layout-scroll-preserve.test.mjs
node tests/terminal-cursor-guard.test.mjs
node tests/terminal-ime-policy.test.mjs
node tests/terminal-backlog-replay.test.mjs
```

## 2026-08-03 输入已发送但画面不回显

现象：Codex 欢迎界面已显示，随后键入的文字不显示，但直接按 Enter 仍会把内容有效发送；画面可能数分钟后才自行更新。

根因：输出写入完成后的主动刷新仍依赖 xterm 的写回调和单次动画帧。浏览器漏掉画布失效通知时，输入后的滚动与光标同步只更新位置，不会强制重画终端内容，因此 buffer 中已有字符但画布保持旧帧。

修复：`refreshTerminalInputVisibilityAfterUserInput()` 的即时、下一动画帧和短延迟刷新都调用统一的 `scheduleTerminalRenderRefresh()`，让任意普通键入都能合并触发一次全视口重画。该刷新只重画当前活动 xterm，不重复发送输入，也不改 PTY 状态。

### Android WebView 补充

Android APK 中的失败并非普通漏刷：Codex 切换全屏 TUI 后 logo 消失，但 TUI 清掉的 Linux 欢迎文字仍留在画面，输入也不可见。这表明 xterm buffer 和 ANSI 状态已推进，而 Android WebView 的硬件加速 canvas 仍在合成旧图层；继续调用 `term.refresh()` 只会重复走同一个失效的 canvas 路径。

APK 会在 UA 末尾附加 `webClxAndroid/<version>`。`createTerminalInstance()` 对该明确标记使用 xterm DOM renderer，其他浏览器继续使用 canvas renderer。Android UA 浏览器回归必须确认 `rendererType === "dom"`、终端中没有 canvas，并且清屏/会话切换后的目标文字存在于 `.xterm-rows`；桌面回归仍确认 canvas 有非背景像素。

## 2026-06-27 交叉引用：同一根因的"拖选"变体

本文件的根因（`term.options.theme =` 每次赋新对象 → xterm 整屏重绘 → 闪烁）还有一条**鼠标拖选变体**，记录在 [终端拖选闪烁 + 选区高亮不可见](terminal-drag-selection-flicker.md) 第五章。

要点（避免重复排查）：

- 鼠标拖选时 xterm 每帧重绘选区高亮，会**放大**任何在 `term.onRender` 里跑的高频逻辑。
- 若该高频逻辑误判命中（退出 TUI 回到 shell 后，底部三行满足 `detectBottomStatusCursorCorrection` 误判），`setTerminalCursorHiddenForCorrection` 就会每帧翻转 theme → 整屏闪烁/内容消失。
- 通用对策：**任何依赖 `onRender` 且会改 theme/触发重绘的逻辑，都要在活动选区期间冻结**；选区清除后用 `term.onSelectionChange` 补一次恢复。判断"活动选区"用 `term.hasSelection()` 或选区手柄拖拽状态。
- 排障任何"整屏闪烁"时，先确认是否有 `term.options.theme =` 的赋值，并查 `onRender`/`onScroll` 里挂了什么每帧逻辑。

## 2026-10-14 Windows 文档关闭后无法输入中文

现象：Windows 浏览器打开项目管理中的“文档”，关闭后返回终端仍无法输入中文。旧浏览器用例在关闭后再次点击终端，仅检查 helper textarea 有焦点，没有验证中文组合输入和 WebSocket 发送。

根因：关闭按钮先激活普通动作的焦点拦截；旧恢复函数先执行输入法重置和聚焦，再清除拦截，导致恢复步骤被拒绝。重置与恢复还依据 `terminalSoftKeyboardVisible()` 分流，而桌面默认显示 webClx 软键盘，因而跳过 Windows 所需的中转 textarea 和终端直接输入恢复。

修复入口：`static/terminal-dialogs.js` 的文档打开和 `static/terminal.js` 的文档 close 事件显式请求 `desktopInput` 恢复。`static/terminal-focus-selection.js` 在非触摸设备上先清除动作拦截，再实际聚焦中转 textarea，随后恢复 xterm helper 的 `inputmode=text` 和焦点；后续重试发现 helper 已聚焦、活动会话已切换、用户已操作其他控件或其他弹窗已打开时立即停止。触摸设备和普通软键盘动作仍走现有键盘状态保持路径。

回归：`python3 tests/terminal-document-ime.browser.py` 使用 Chromium CDP `Input.imeSetComposition` / `Input.insertText` 验证组合输入开始和结束、中文完整且仅发送一次、文档中文字不泄漏到终端，以及关闭后立即开始的组合输入在延迟恢复窗口中持续有效。覆盖桌面软键盘显示/隐藏、关闭按钮/Esc、多次重开，以及触摸端收起键盘的保持行为。`terminal-document-draft.browser.py` 和 `terminal-ime-policy.browser.py` 检查原有草稿和键盘策略。

验证边界：Linux 上的 Chromium 使用 Windows UA 和真实浏览器组合输入事件；这不等于 Windows 原生输入法候选窗实测。在线静态资源经运行目录符号链接即时生效，三个改动模块的 HTML 缓存键均为 `20261014-win-doc-ime2`。客户端应刷新后按“文档 → 关闭 → 终端中文输入”复核原生输入法。

## 2026-10-15 Windows 新开 AGY / 恢复后无法输入中文

新开终端的 `terminal-specified-task-dialog` 共用于 AGY 新对话和恢复（含 `agy -c`）。原关闭逻辑只回焦触发菜单按钮，没有调用桌面 IME 恢复；创建终端时的聚焦发生在模态弹窗仍打开期间，不能代替关闭后的恢复。

`terminal-specified-task.js` 统一在原生 `close` 事件调用 `restoreTerminalFocusAfterDialogClose({ desktopInput: true })`，覆盖关闭按钮、Esc 和成功启动；终端页不再抢回菜单按钮焦点，工作区页继续原行为。复用既有桌面恢复与触摸策略，不更改 AGY 命令或账号。缓存键 `20261015-launch-ime`。

`tests/terminal-document-ime.browser.py` 扩展 AGY 预设、创建/改名与自动输入 API fixture，实际提交新建 `agy` / 恢复 `agy -c`，验证关闭后 CDP 中文组合输入与 WebSocket 传输；覆盖桌面软键盘显示/隐藏、关闭/Esc，以及触摸取消保持收起。Windows 原生候选窗仍需客户端验收。

验证：IME 策略、会话详情/活动、AGY 恢复与预设菜单矩阵 Node 测试通过；文档/启动 IME 浏览器 6 组以及 `terminal-ime-policy.browser.py` 通过。额外执行的 `specified-preset-actions.test.mjs` 为 20/21：既有用例的 `/home/codes/webClx` 与 `/home/codes/tools/webClx` 路径期望不一致（本次未改该测试及其实现）。运行服务返回的 `terminal-specified-task.js` 与源码 SHA-256 一致；未认证 `/terminal` 返回登录页面，页面缓存引用由源码及浏览器 fixture 核验。

## 2026-10-15 其他终端弹窗同类缺口审计

发现粘贴、输入历史、定时任务关闭仍按桌面软键盘可见性跳过 IME 上下文恢复；配额、群组、消息、预设切换等缺少统一关闭恢复。改名还会用 `setTimeout` 回焦按钮，在恢复后再次抢焦点。

`installTerminalDialogCloseRecovery()` 在终端页捕获所有原生 dialog close，微任务中等待本次关闭处理完成；若弹窗自己的恢复函数已执行（revision 变更），不重复重置，否则补桌面恢复。父弹窗/替代弹窗仍打开时不恢复终端；触摸设备不进入新监听器。已有恢复函数统一按主触摸设备判断桌面，保留调用参数兼容性。改名延迟回焦仅保留触摸路径。普通菜单/软键盘动作不新增回焦。缓存键 `20261015-dialog-ime`。

`terminal-document-ime.browser.py` 枚举 21 个弹窗，桌面软键盘显示/隐藏各验证原生 close、Esc 及 13 个真实关闭按钮：每次成功中转聚焦仅一次，中文 CDP 组合输入持续跨越延迟恢复窗口、经 WebSocket 只发送一次；另验证嵌套路径选择框关闭保留父弹窗。加载修复前 HEAD 静态 JS 时相同审计用例可复现焦点门禁阻止输入；修复后全通过。原文档、新开 AGY、新建/恢复及触摸用例同时通过。Windows 原生候选窗验证仍由用户客户端完成。
