---
name: webclx-terminal-scroll
description: 作为 webclx-ops 子技能，排查和修复 webClx 终端切换未到底、输入或回答后跳顶部、新输出不跟随，以及回放、共享尺寸和布局引起的滚动位置错误。
---

# 终端滚动修复

关键词：下拉切换、跳顶部、停在中间、跟随输出、CodeBuddy 重绘、共享终端、scrollback、viewportY/baseY、scrollTop、scrollIntentRevision、scroll area clamp。

## 当前产品语义

- 切换到不同终端时始终显示底部，并重新启用 `followOutput`。同一终端内手动向上阅读后保留位置；新输入或跳底按钮恢复跟随。焦点恢复、IME、布局变化及同会话重连不能冒充新的用户输入。
- 后台新增输出也必须滚到底部：write 完成和 replay 结束调用 `followBackgroundTerminalOutput`，更新所属 context 的逻辑位置、`followOutput` 和保存的底部意图；只操作该 context 的 xterm，不影响前台阅读。`activateTerminalContext` 仍在显示目标前定位底部。此为 2026-10-16 用户最新要求，替代“后台不强制滚动”。
- 权威专题：`/home/codes/tools/webClx/docs/codex/tasks/terminal-session-switch-output.md`；移动布局另读 `terminal-mobile-scroll-settle.md`，中文输入法按需读相邻 `../webclx-chinese-ime/SKILL.md`。

## 取证与定位

- 先核对用户客户端实际请求的 HTML 缓存键、HTTP JS 哈希与源码；运行目录 `/home/bin/webclx/static` 必须指向项目实体 `static`。仅修改静态资源无需重启，改动模块需更新 HTML 缓存版本。
- 同时记录页面 `scrollY`、终端 `buffer.active.viewportY/baseY`、`followOutput`/intent revision、DOM `scrollTop/scrollHeight/clientHeight`、渲染 `canvasHeight`、本地 fit 与 canonical 行列。`viewportY === baseY` 不能单独证明最后一行可见。
- 追踪选择入口 `selectSession` → `activateTerminalSessionContext` / `restoreCachedTerminalViewport`，输出队列、回放结束、尺寸通知、fit/IME、焦点恢复的异步回调。所有恢复都必须校验所属 context/session 和新旧用户意图。
- 没有复现证据，不能把“回答后跳顶”直接归因于消息对话框或某个 Agent；需要对应 ANSI 字节或浏览器布局事件。真实终端只读取证，不向用户工作中的 Agent 发送测试输入。

## 已知回归入口

- 写入结束不得恢复过期的 follow 决策；旧 viewport 事件不得写入当前会话；切换修复历史见 `244b9c2`。恢复快照必须核对活动 context 的 session，而非仅信任刚更新的页面 session ID。
- 共享 canonical 行数超过浏览器可见行数时，缓冲区逻辑到底仍可能裁掉输入区。检查 `static/terminal-layout-connection.js` 的尺寸通知与 fit 两处行数限制，并验证最后一行在 viewport 内。
- xterm 异步刷新缩短 scroll area 时，浏览器会夹紧 DOM scrollTop；若随后被当成用户滚动，逻辑位置与 follow 都会丢失。检查 capture 阶段的 `repairTerminalViewportScrollArea` / `syncTerminalViewportDomToBuffer`；保存意图的 `handleTerminalViewportScroll` 留在 bubble 阶段，确保真实滚动条操作先更新 xterm 逻辑位置。

## 验证与交付

- 单元入口：`tests/terminal-scroll-intent.test.mjs`；真实 xterm：`tests/terminal-session-cache.browser.py`。按项目当前检查规则运行，涉及编译时用 webClx 队列，收到匹配 request_id 回调后才读取结果。
- 浏览器矩阵包含 desktop canvas、触屏、Android DOM、重连；本机 Chromium 为 `/opt/google/chrome/chrome`，通过 `WEBCLX_TEST_CHROMIUM` 指定。
- 反查：几何夹紧/共享行数修复 `0d3310d`；显示前滚底 `4a81835`；后台新增输出即时滚底修复 `91d10ed`，四类浏览器通过请求 `205201-18debef697f8f6ad`；旧代码新增 200 行仍停第 9 行/底部 320 行的复现请求 `205100-18debef697f8f6aa`。浏览器测试应在切回前断言后台逻辑到底、保存跟随且前台历史不动，不能仅验切回后到底或后台滚动调用为零；详细证据见专题“后台输出与显示前定位”。
- 必测切换即时与稳定后的位置、活动/后台尺寸变化、输入与未完成 write 交错、手动滚轮/触摸/滚动条、回放、clear-scrollback 重绘、滚动区域临时收缩。既验证逻辑到底，也验证 DOM 和可见最后一行；新增用例先证明旧代码失败。
- 验证记录写入专题，父技能只保留子技能入口；同步整个 `webclx-ops` 技能目录，避免只复制父 SKILL.md 而丢失子技能。
