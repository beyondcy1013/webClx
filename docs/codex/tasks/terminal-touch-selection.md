# Mobile Terminal Touch Selection

Date: 2026-05-16

## Root Cause

The terminal touch-selection path treated a slow drag as text selection. Once a touch moved more than the drag threshold after 280ms, `static/terminal.js` synthesized xterm mouse selection even if the user had not long-pressed.

## Stable Rule

Touch text selection is allowed by default so long-press copy is available immediately.

Touch text selection must require a long press first. Movement before the long-press threshold should cancel the selection candidate so normal touch dragging and scrolling remain usable.

When the long press threshold is reached, create a visible one-cell terminal selection immediately. This is what makes the copy button and draggable selection handles appear even before the user moves their finger.

After that initial long press, ordinary movement of the original touch must not expand the selection. Keep the one-cell selection stable so the page does not start scrolling or selecting unpredictably. Selection expansion belongs to dragging the two visible selection handles.

For copying dynamic terminal output without disturbing drag behavior, use the mobile `全能` menu action `新窗口复制`. It opens the current visible terminal text in a plain browser textarea in a new window so the user can manually select/copy there instead of relying on xterm selection.

If long-pressing terminal text shows four handles or flickers, check whether browser-native text selection is active at the same time as the custom xterm selection. The fix is to suppress native `selectstart`/`contextmenu` while a terminal touch-selection candidate or selection is active, clear DOM selection ranges before selecting in xterm, and keep only the two custom `.terminal-selection-handle` controls visible.

## Implementation

- `static/terminal-touch-selection-policy.js` owns the touch-selection timing policy.
- `static/terminal.js` uses that policy to create the initial long-press selection. The initial touchmove after long press is swallowed without changing the selected range; the separate handle-drag path expands or shrinks the range.
- `static/terminal.js` also guards touch-selection startup against browser-native selection so mobile browsers do not show native copy handles on top of the xterm handles.
- `static/terminal.html` loads the policy before `terminal.js`.
- `src/main.rs` embeds the policy asset for static fallback serving.

## Verification

```bash
node tests/terminal-touch-selection-policy.test.mjs
node --check static/terminal.js
node --check static/terminal-touch-selection-policy.js
cargo test embedded_assets_include_page_dependencies -- --nocapture
```

After frontend-only fixes, verify `/home/bin/webclx/static/terminal.js` is synced with `static/terminal.js`; otherwise the running terminal page will still use the old touch-selection behavior.

## 2026-10-28 长按选择保持输入法状态

长按、松手及拖动选择手柄不属于输入请求；全过程必须保持原有系统键盘状态、helper 焦点和 IME 属性，包括 Android Back 收起键盘但 helper 仍聚焦的状态。禁止通过先 blur 再 focus、重写输入属性或强制隐藏键盘来恢复表面状态。

根因在事件分发层：浏览器 pointerdown 默认行为及兼容 mousedown 会提前改变焦点；冒泡阶段的 contextmenu 拦截晚于 xterm 自带右键处理，后者会 focus 并移动 helper，清除选区。xterm 还可能吞掉 touchstart，使冒泡监听无法登记长按候选。

修复入口为 `static/terminal-focus-selection.js` 与 `static/terminal.js`：在 capture 阶段登记 touchstart、阻止 touch pointerdown 默认聚焦，并在 xterm 前处理 contextmenu；长按只调用选区 API，删除会触发 focus 的模拟 mousedown 兜底；消费选择结束后的 click。普通短按在 touchend 确认后进入原有直接输入路径，滑动与取消不聚焦。选择手柄跳过普通动作按钮的陈旧焦点清理，保持输入连接原样。

回归 `tests/terminal-ime-policy.browser.py` 使用真实 CDP 持续触摸与手柄拖动，覆盖 canvas / Android UA DOM、键盘展开 / 收起 / 收起但残留焦点 / 禁用四态，检查选区与手柄可见、零 focus 调用、零 focus/blur、零 IME 属性写入、零终端输入，并检查下一次普通短按仍可输入。无头浏览器的 contextmenu 使用触摸 PointerEvent 补充派发，原生 IME 显隐由 bridge 模拟；这不等同于安卓真机验收。

静态 loader 缓存键为 `20261028-selection-ime`。服务直接读取源码静态目录，重载页面生效，无需重建 APK 或重启服务。

本次验证：IME 策略与右键菜单 Node 检查通过；上述浏览器四态在两种渲染器均通过；`bash scripts/repo-check.sh --terminal-scroll` 最终 9/9 通过（报告 `/tmp/webclx-repo-check/20261027T183142Z-fe35cb5235db-dirty/report.json`）。HTTP 两模块和 HTML 与源码逐字节一致。`terminal-touch-selection-policy.test.mjs` 仍在第 197 行已有的 `terminalCanScrollTouchPixels` 源码断言失败；用 HEAD 五份原始文件在临时目录独立运行，同样失败，未放宽断言或修改滚动算法。
文档/弹窗中文组合输入浏览器回归最终通过（桌面软键盘显隐、触摸、AGY 新建/恢复、23 个弹窗及 13 个实际取消按钮）。两个 IME 浏览器脚本补齐 Android 首次语言选择流程，避免欢迎弹窗干扰输入焦点；并发运行曾遇到 quota 弹窗重复恢复的时序断言，独立重跑通过。

### 安卓真机反馈后的 touchstart 补修

用户随后反馈安卓长按马上弹出输入法且无选区，上一版不能视为真机修复完成。遗漏在于只取消 `pointerdown`，`rememberTerminalTouchSelectionCandidate` 仍以 passive touchstart 注册，无法取消 WebView 的原生编辑器激活与长按默认行为。原浏览器检查只核对 focus/blur 和最终选区，且补发 contextmenu，没有检查原生默认是否在按下时被阻断。

改为 capture + `passive: false` 的 touchstart，在成功登记单指长按候选后同步 `preventDefault()`；普通短按仍由 touchend 明确聚焦。取消 touchstart 也会取消原生页面 panning，因此同一手势在终端滚动边界显式滚动外层页面；禁用选择时保持原生滚动路径。未通过 blur/focus 或输入属性修改来隐藏症状。

`terminal-ime-policy.browser.py` 增加按下 100ms 时的 defaultPrevented、候选、零 focus/blur 检查，并在补发 contextmenu **之前**断言定时器已建立选区和手柄。该检查在上一提交上因 `defaultPrevented: false` 失败，补修后两渲染器四态均通过。`terminal-touch-ime-gesture.test.mjs` 调用生产处理函数，验证终端边界页面滚动、回到历史滚动及禁用选择时的默认行为。缓存键改为 `20261028-touchstart-ime`；仍需安卓真机重载后确认原生 IME 与选区，浏览器测试不替代现场验收。
补修验证：`bash scripts/repo-check.sh --terminal-scroll` 9/9 通过，报告 `/tmp/webclx-repo-check/20261028T014818Z-f91bf437db65-dirty/report.json`；IME 策略、右键菜单与新增手势 Node 回归通过。HTTP 两模块及 HTML 与源码一致，静态更新即时生效。

### 2026-10-29 系统键盘展开时长按收起

用户真机反馈系统键盘已激活时，长按会关闭键盘，前两次补修仍不能视为现场验收通过。继续审计发现 `beginTerminalTouchSelection` 及 contextmenu/selectstart 防护均无条件 `document.getSelection().removeAllRanges()`，会操作 WebView 与编辑器共享的原生选区。旧回归只检查 focus/blur 和 IME 属性，不能证明原生编辑连接未受影响。新浏览器检查在旧实现上记录一次长按两次清空原生选区并失败；这证明存在多余操作，尚不能单凭浏览器证明真机键盘收起的唯一根因。

新增 `clearNativeTerminalTextSelection()`：存在活动编辑器时完全不触碰原生选区；没有编辑器时也只清除真正非折叠、两端都在当前终端内的 DOM 文本选区。空选区、外部选区不操作。继续用事件 preventDefault 阻止原生选择，xterm 选区 API 与自定义手柄照常工作，不通过重新 focus 或强制显示键盘掩盖收起。

`terminal-ime-policy.browser.py` 在原有两渲染器四态下增加零原生选区清空断言；`terminal-touch-ime-gesture.test.mjs` 检查活动编辑器、空选区、终端 DOM 选区及外部选区的所有权。缓存键 `20261029-selection-editor`，静态修复重载页面生效，安卓真机仍待重载验证键盘持续展开、选区可用。
验证：两渲染器四态 IME 浏览器检查通过（包含零原生选区清空），手势/IME/右键菜单 Node 检查通过；滚动矩阵 9/9 通过，报告 `/tmp/webclx-repo-check/20261029T100645Z-562685d4b43d-dirty/report.json`。HTTP 模块及 HTML 与源码一致。既有触摸策略脚本的第 197 行滚动源码断言仍按上一轮已记录状态，未放宽。

### 2026-10-31 移动端双击智能选词与输入法零干扰（含松手防抖与长按松开零干扰修复）

针对长按单字符选区在小屏上手柄微调繁琐的问题，增加双击（Double-Tap）智能选词支持；并针对真机测试反馈的“松开的一瞬间误触发系统输入法”完成手势状态机根治：

1. **松开瞬间误触发输入法根因排查与修复**：
   - **长按松开瞬间**：长按 2 秒建立选区后手指松开，原代码因缺乏对已触发长按标记的识别，误落入 candidate 短按分支并派发 `focusTerminalFromTerminalTap()` 唤起输入法。修复：在定时器触发长按及拖拽选区命中时确立 `candidate.longPressFired = true`，松手时只要命中 `longPressFired` 即直接释放手势（`endTerminalTouchSelection`）并阻断全部 focus 派发，选区与手柄保持，输入法绝对零干扰。
   - **双击前序点击松开瞬间**：双击由两段快速短按构成，若第一击松开时同步调起聚焦，系统软键盘瞬间弹起导致晃屏打断第二击。修复：键盘未打开时为单次轻点增加 250ms 防抖消歧窗口（`terminalSingleTapTimer`）；当第二击按下时立即取消该防抖，全程不进入 focus；若 250ms 内无第二击，才判定为单次点击意图唤起输入法。
   - **收起保护与定时器清理**：在 `blurTerminalHelperTextarea()` 中同步清除悬挂的单次点击定时器，彻底防止失焦后延时误唤起。
2. **策略与分词**：`static/terminal-touch-selection-policy.js` 增加 `terminalTouchSelectionIsDoubleTap`（300ms 且位移 <= 24px）与 `terminalTouchSelectionWordRange`。分词保留文件路径（如 `/path/to/file`）、参数标识（如 `--flag`）、commit 哈希及 CJK 连续短语整体连贯，单字符分隔符或空白独立选区。
3. **手势与输入法状态保护**：
   - 双击命中：调用 `beginTerminalTouchWordSelection()`，通过 `term.select(startCol, row, wordLen)` 激活选区并呼出手柄与快捷复制按钮；键盘关闭时双击选词保持关闭，键盘已激活时双击选词保持展开且保护原生编辑选区。
   - 滚动与取消：触摸滑动超出位移或长按触发后均重置双击状态，点击空白处清除当前选区。
4. **验证**：
   - `tests/terminal-touch-double-tap.test.mjs` 覆盖单次点击防抖不弹输入法、双击智能选词全程零输入法干扰、激活键盘下双击选词零干扰、轻点清除选区及长按松开选区保持 5 项完整手势状态机断言全部通过；
   - `tests/terminal-touch-ime-gesture.test.mjs` 4 项编辑器原生选区防护断言全部通过；
   - `tests/terminal-ime-policy.browser.py` 真实 Chromium 触屏双击选词、手柄可见性、轻点清除及全量 IME 策略验证，两渲染器四态全过；
   - `bash scripts/repo-check.sh --terminal-scroll` 9/9 矩阵全部通过；
   - `cargo test embedded_assets_include_page_dependencies` 通过；
   - 静态缓存键更新为 `20261101-tap-release-zero-ime`。

## 2026-11-02 选区菜单增加“打开输入法”

长按单字符和双击选词建立选区后，原菜单只有“复制选中”。用户仍需要从同一手势显式回到输入状态，因此增加 `打开输入法` 按钮，两个按钮组成选区操作菜单。

按钮定位仍在选区下方，优先按可用宽度水平排列，空间不足时整体回退到宿主左上角，不覆盖终端。`pointerdown` 阻止焦点转移；点击“打开输入法”是显式授权动作，清除系统键盘抑制、启用系统输入法、进入终端直接输入路径并同步键盘复选框，不清除已有选区。

`tests/terminal-ime-policy.browser.py` 在原有两渲染器四态长按检查后增加菜单可见性与点击结果断言：系统键盘启用、终端 helper 聚焦、选区保留。Node 手势回归同步提供菜单按钮桩。缓存键 `20261102-selection-ime-menu`；纯前端和静态资源修改，重载页面生效，无需重建 APK。Android WebView 中 `term.focus()` 的原生键盘弹出仍属真机验收；若旧 WebView 不弹出，需要后续增加专用原生 bridge 再发版。

### 2026-11-03 选区菜单“打开输入法”走原生 bridge

纯前端版本依赖 `term.focus()` 唤起键盘；旧 Android WebView 不会因脚本聚焦弹起 IME，因此补一层显式原生入口，不改动长按/双击本身不得干扰输入法的既有约束。

- 原生：`MainActivity.AndroidWebViewBridge` 增加 `@JavascriptInterface showSystemKeyboard()`。该方法只由网页显式调用，活动不可用或 WebView 为空时返回 `false` 让前端保留原聚焦回退；否则在 `mainHandler` 上 `webView.requestFocus()` 后用 `InputMethodManager.showSoftInput(webView, 0)` 请求键盘。IME 可见性仍由 `WindowInsetsLayout` 独占，未引入第二套可见性来源。
- 前端：`terminal-focus-selection.js` 新增 `showTerminalSystemKeyboardNatively()`，仅在 `window.WebClxAndroid.showSystemKeyboard` 存在时调用并以 `try/catch` 兜底返回 `false`；`terminal.js` 的选区菜单点击顺序为“启用系统输入法 → 聚焦终端 → 请求原生键盘”，浏览器与旧 APK 因无 bridge 自动退回原行为，选区与菜单不受影响。
- 回归：`tests/android-client-contract.test.mjs` 校验 bridge 方法与 `InputMethodManager` 调用形态；`tests/terminal-ime-policy.test.mjs` 校验前端 helper 的兜底与调用顺序；`tests/terminal-ime-policy.browser.py` 在两渲染器四态点击断言 `bridgeCalls == 1` 且聚焦、选区保持。`bash scripts/repo-check.sh --terminal-scroll` 9/9 通过。
- 生效方式：前端为静态资源修改（缓存键 `20261103-selection-ime-bridge`），重载页面即生效；原生方法必须随新版 APK 下发，未发版前旧 APK 继续走聚焦回退。

### 2026-11-03 选区菜单随选区一起隐藏

用户反馈：点击其它地方后，选区与“复制选中”消失，但“打开输入法”按钮仍留在原位。

根因在 `syncTerminalSelectionCopyButton()`：无选区分支只 `terminalSelectionCopyButton.hidden = true` 就 return，从未处理 2026-11-02 新增的 `terminalSelectionOpenImeButton`，而全文件也没有任何位置把它置为 `hidden = true`，于是按钮停留在上一次定位的坐标上。这不是样式或定位问题，是菜单状态缺少统一的隐藏入口。

修复为新增 `hideTerminalSelectionMenu()`，在无选区分支同时隐藏两个按钮并清除各自的 left/top/bottom/visibility 内联定位，避免下次建区复用陈旧坐标；有选区的定位分支不变。复制选中后 `term.clearSelection()` 走同一分支，附带修掉复制后菜单残留。

回归：`tests/terminal-touch-double-tap.test.mjs` 在双击建区后断言两个按钮均可见，轻点清除后断言两者均 `hidden`——新增断言在修复前于“tapping away must hide the open-IME button together with the copy button”失败；`tests/terminal-ime-policy.browser.py` 在真实触屏双击后断言整个菜单可见，轻点他处后断言 `!term.hasSelection()` 且两个按钮均隐藏。`bash scripts/repo-check.sh --terminal-scroll` 9/9 通过。缓存键 `20261103-selection-menu-hide`，静态资源重载页面即生效。

### 2026-11-09 选区菜单增加“复制全部”并统一三按钮布局

长按（以及双击选词）后的菜单原来只有“复制选中 / 打开输入法”，用户需要一个直接复制终端全部缓冲内容的入口。新增 `terminal-selection-copy-all` 按钮，排在“复制选中”与“打开输入法”之间，复用 `.terminal-selection-copy` 样式，无需新增 CSS 规则。

点击处理与“复制选中”同形：`readTerminalAllText()` 取全量文本，`copyTextToClipboard()` 复制成功后 `term.clearSelection()` 并同步菜单（菜单随之隐藏），失败只提示不清选区；`pointerdown` 同样 `preventDefault()`，避免抢走终端焦点、改变输入法状态。

布局层顺手把按钮集合收敛成 `terminalSelectionMenuButtons()`（顺序即左到右顺序），定位时先整体 `visibility:hidden` 测量、按整组宽度居中，再逐个排开；隐藏走 `hideTerminalSelectionMenu()` 统一处理三个按钮；选区滚出视口或无 metrics 时改为 `showTerminalSelectionMenuAtHostCorner()`，在宿主左上角排成一行，不再让多个按钮叠在 8,8。

回归：`tests/terminal-touch-double-tap.test.mjs` 增加 `terminalSelectionCopyAllButton` 桩并断言建区可见、轻点他处隐藏；`tests/terminal-ime-policy.browser.py` 的菜单可见性与轻点清除断言扩到三个按钮。`bash scripts/repo-check.sh --terminal-scroll` 9/9 通过。缓存键 `20261103-selection-copy-all`，静态资源重载页面即生效。

### 2026-11-09 系统输入法只在显式动作后武装（inputmode 门禁）

用户真机仍反馈“长按会弹出系统输入法”，并把要求扩大为：所有软键盘/菜单都不得触发或改变系统输入法弹出状态，只有“系统输入法”相关控件可以。此前三轮都在事件路径上拦截（pointerdown/touchstart 默认行为、原生选区清空、松手焦点），真机仍漏，说明根因不在某条事件链，而在输入连接本身：helper textarea 平时就是 `inputmode="text"`，任何一条路径——原生或脚本——只要把它聚焦就会唤起键盘。

改为在输入连接层强制该规则：

- 新增 `terminalSystemImeArmed`，唯一赋值点是 `focusTerminalForDirectInput()`（普通轻点直接输入、系统输入法复选框、选区菜单“打开输入法”都汇聚到这里），并且在 `term.focus()` 之前武装。
- `syncTerminalImePolicy()` 中 helper 未聚焦时一律 `inputmode="none"`，只有已武装且不在软键盘抑制态才是 `"text"`。这样长按、选区手柄、软键盘、各类菜单即使再发生一次意外聚焦，也无法唤起系统键盘。
- helper 已聚焦时跳过“降级为 none”的写入：改写聚焦 textarea 的 inputmode 会重启输入连接、把键盘收起，等于又一次改变输入法状态。反向的 none→text 仍允许，因为只有显式动作会武装。
- helper 上安装一次性的 blur 监听，失焦即解除武装并重新同步，避免授权残留导致后续意外聚焦重新弹出键盘。

行为对照：键盘收起时长按不再可能弹键盘；键盘展开时长按不改写 inputmode，键盘保持展开；普通轻点仍照常唤起键盘；复制选中/复制全部等菜单后的 `focusTerminalIfAllowed()` 不再附带唤起键盘。桌面端 `inputmode` 不影响物理键盘与中文 IME 组合，`tests/terminal-document-ime.browser.py` 全量通过。

回归：`tests/terminal-ime-policy.browser.py` 新增两条——长前后未聚焦的 helper 必须 `inputmode=none`；在未聚焦状态下额外做一次“意外聚焦”，断言前后 inputmode 均为 none 且 `terminalSystemImeArmed` 为 false。把 `static/terminal-focus-selection.js` 换回 HEAD 旧实现后，该用例在 `(visible=False, focused=False, enabled=True)` 状态按预期失败，恢复后两渲染器四态全过。`tests/terminal-ime-policy.test.mjs` 同步更新 inputmode 契约并新增武装点、聚焦不降级、失焦解除武装三条断言；`tests/terminal-touch-double-tap.test.mjs` 的 `TerminalNode` 补 `addEventListener` 桩。`bash scripts/repo-check.sh --terminal-scroll` 9/9 通过。缓存键 `20261109-ime-armed-gate`，静态资源重载页面即生效；真机结果仍需现场复验。
