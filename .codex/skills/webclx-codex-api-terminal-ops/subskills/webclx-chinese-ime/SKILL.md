---
name: webclx-chinese-ime
description: 排查 webClx 中文输入法消失、Windows 弹窗返回终端不能输入中文、AGY 新开或恢复后失焦，以及移动软键盘 IME 焦点隔离问题。作为 webclx-ops 的中文输入法子技能按需读取。
---

# 中文输入法问题

关键词：中文打不出来、输入法消失、候选窗、Windows IME、文档管理、新开 AGY、agy -c、恢复会话、焦点门禁、软键盘。

## 先定位设备与故障边界

- 区分 Windows 电脑浏览器、Android 浏览器/WebView，以及“弹窗内不能输入”和“关闭弹窗返回终端后不能输入”。桌面显示 webClx 软键盘不代表主触摸设备。
- 从实际点击链路追踪 `guardTerminalActionEditableFocus` → 弹窗/异步创建终端 → `close` → IME 重置 → xterm helper。只断言 `document.activeElement` 或直接填入中文，不能证明组合输入可用。
- 权威专题：`/home/codes/tools/webClx/docs/codex/tasks/terminal-codex-cursor-flicker.md` 的 2026-10-14 / 2026-10-15 Windows IME 与其他弹窗审计章节；原生客户端待验见项目 `todo.md` R5。

## 已验证的桌面故障与修复入口

- 动作按钮会激活 `terminalActionKeyboardPolicyActive` / `terminalActionEditableFocusBlocked`。恢复前必须先解除门禁，否则程序化 focus 被拦截或 focusin 后立即 blur。修复位于 `static/terminal-focus-selection.js`。
- Windows 关闭弹窗需经中转 textarea 重建 IME 上下文，再恢复 xterm helper 的 `inputmode=text` 和焦点。原逻辑按 `terminalSoftKeyboardVisible()` 分流，导致桌面默认显示软键盘时跳过恢复；现恢复按 `hasPrimaryTouchInput()` 区分设备。
- AGY 新开/恢复共用 `terminal-specified-task-dialog`；创建终端时弹窗仍打开，提前 focus 不能替代关闭后恢复。`static/terminal-specified-task.js` 的原生 close 处理覆盖新建 `agy`、恢复 `agy -c`、取消与 Esc；不要借排障修改账号、启动命令或注入配置。
- `installTerminalDialogCloseRecovery()` 在 `static/terminal.js` 安装，捕获所有原生 dialog close，微任务中补充未执行的桌面恢复；revision 避免已有 close 处理重复重置。父弹窗或替代弹窗仍打开时不得抢回终端焦点。新增弹窗优先复用该入口。
- `static/terminal-session-render.js` 改名关闭后的延迟按钮回焦会再次抢走已恢复的输入焦点，桌面已取消该回焦。恢复重试必须尊重组合输入、用户新焦点与会话切换。
- 文档加载、刷新、保存、切换时 disabled 会丢焦：解除 busy 后恢复编辑器焦点，且文档编辑控件应豁免动作门禁；不能只修弹窗关闭。
- 修复追溯：`055e21e` 文档、`cf54c8e` AGY、新增统一恢复与弹窗审计 `9409fc4`。缓存键只作历史证据，后续修改应使用新键。

## Mobile Soft Keyboard, IME Isolation & Touch Invariants

- **Zero-Refocus Architectural Rule**: Soft-keyboard interaction must **never** proactively call `helper.focus()`. Ordinary soft-keyboard buttons, detached menus, and quick actions may only preserve the existing focus state (`snapshot.focused ? syncTerminalImePolicy() : blurTerminalHelperTextarea()`), and must never refocus the xterm helper textarea. Programmatic focus on `helper` causes the Android IME to re-pop, flicker, or shift layout when it was intended to stay collapsed.
- **Android Chromium Viewport Detection**: On modern Android Chromium, system keyboard popup resizes both `window.innerHeight` and `visualViewport.height` together (`interactive-widget=resizes-content`). Never rely purely on `layoutHeight - viewportHeight >= 80` to determine keyboard visibility. When `terminalHelperTextareaFocused() && terminalSystemImeEnabled` or screen height delta indicates keyboard presence, treat the keyboard as visible. Otherwise `guardTerminalActionEditableFocus` mistakenly treats the keyboard as closed and forces `blurTerminalHelperTextarea()`, causing an already-open keyboard to collapse upon soft key tap.
- **Touch-Action vs Pointer Capture Hazard on Android**: Soft-key rows and buttons use `touch-action: pan-x`. Calling `button.setPointerCapture` on touch events causes Chromium to interpret touch micro-jitters as pan-scroll gestures and dispatch `pointercancel`, dropping key input. Skip `setPointerCapture` for `event.pointerType === "touch"`, and in `handleMobileKeyPointerEnd` allow `pointercancel` with distance `< MOBILE_KEY_DRAG_THRESHOLD` (12px) to trigger as a valid touch release (`isTouchRelease`).
- **Control Mousedown / Pointerdown Default Prevention**: Non-input control targets (such as checkboxes and their parent labels in dropdown menus) must prevent default on both `pointerdown` and `mousedown` in capture phase (`guardTerminalActionEditableFocus` and `preventPointerFocus`). This prevents browser native form activation from stealing focus away from `terminalHelperTextarea()`.
- **Validation Commands**:
  - Unit tests: `node tests/terminal-ime-policy.test.mjs && node tests/terminal-session-activity.test.mjs`
  - Browser Playwright tests: `python3 tests/terminal-ime-policy.browser.py`
  - Required mobile touch scenarios:
    1. Start collapsed -> tap soft key button -> sends input sequence, IME remains collapsed (`helperFocused: false`).
    2. Start popped up -> tap soft key button -> sends input sequence, IME remains popped up (`helperFocused: true`).
    3. Micro-jitter touch release (`pointercancel`) -> input dispatched reliably without loss.

## 验证与生效

在 `/home/codes/tools/webClx` 执行：

```bash
python3 tests/terminal-document-ime.browser.py
python3 tests/terminal-ime-policy.browser.py
python3 tests/terminal-session-switch-focus.browser.py
node tests/terminal-ime-policy.test.mjs
node tests/terminal-session-details.test.mjs
node tests/terminal-rename-presets.test.mjs
node tests/terminal-session-activity.test.mjs
```

- `terminal-document-ime.browser.py` 用 CDP `Input.imeSetComposition` / `Input.insertText` 检查组合开始/结束、延迟恢复不打断组合、中文经 WebSocket 完整且仅发送一次、文档文本不泄漏到终端。已覆盖 21 个静态弹窗的 close/Esc、13 个实际关闭按钮、桌面软键盘显示/隐藏、嵌套选择框、AGY 新建/恢复及触摸取消策略。扩展实际故障入口，不用 synthetic focus 结果冒充用户路径。
- Linux Chromium + Windows UA 不是 Windows 原生输入法验收；修复后请客户端按原入口验证候选窗与提交中文。不得把未验证状态写成已通过。
- 静态修改前确认 `/home/bin/webclx/static` 解析到 `/home/codes/tools/webClx/static`；修改模块的 HTML loader 缓存键，核对服务 HTTP 字节/哈希。无需复制静态目录、重建或重启二进制。
