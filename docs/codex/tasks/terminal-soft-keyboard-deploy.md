# Terminal Project Commands

Date: 2026-07-10

## 工作环境与任务流程归一化（2026-09-26，第一阶段）

入口：设置 → 工作流 → 新建工作流 → 从使用场景开始。

- **进入工作环境**生成现有 `codex_launch` 动作：填写预设、目录、终端名及
  初始技能/环境说明，沿用待命提示与 owner_key 复用，不立即执行环境说明中的任务。
- **执行任务**生成 `switch_api_preset → codex_exec`：在发起时冻结的源项目
  目录中执行，复用现有任务 API 和结果展示。
- **复用已有流程**模板与表格“创建引用”生成 `run_workflow`，不复制动作；
  多个入口使用被引用流程的身份。“复制”仍用于独立修改，“高级自定义”保留动作编辑。
- 模板仅改草稿，替换前确认；应用到列表后仍需保存设置。旧 schema、动作类型、
  条目 ID 和导入格式不变，不自动改写已保存的配置。
- 表格根据动作显示工作环境/任务流程/组合流程。共享 `inspectTerminalWorkflow`
  用于编辑、导入与执行前检查：拒绝缺失引用、循环、无效参数、超过 20 层嵌套或
  展开后超过 200 个动作。显式报错、不截断；检查失败不执行前序动作。
- 删除被引用条目或目录时提示引用入口，先修改引用再删除。
- 状态区分“环境已打开”“任务执行完成”“步骤已提交”。发送命令或启动 Agent
  不等于任务完成，就绪/完成仍以原生输出或任务 API 终态为准。
- 结构化参数复用移动端参数网格区域，避免引用选择器和环境表单挤入编号列。

本阶段没有新增执行引擎或修改预设、认证、终端启动协议，也没有批量删除旧菜单入口。

## 工作流快速检索与运行参数（2026-09-26，第二、三阶段）

- 利器菜单提供跨目录搜索：只匹配工作流名称与目录路径，不读取动作正文，避免暴露
  命令中的敏感值；搜索结果继续复用原有点击执行入口。
- 根目录显示有界 MRU“最近使用”（本地保存 6 个），被删除条目自动不渲染；只有完整
  执行成功才写入最近记录。
- 动作 `value` 支持 `{{参数名}}`。执行前先运行共享引用图检查，再从展开后的完整动作
  集合收集参数；同名参数只询问一次，超过 20 个显式失败。
- 参数对话框在第一次副作用前打开；空值、取消按钮、Esc 或关闭对话框都取消执行，
  不执行前序动作，也不回写流程定义。长文本动作使用多行输入。
- 顶层和嵌套动作共享同一份运行参数并逐个物化；占位符只作用于 `value`，不改变动作
  类型、预设 ID、目录或终端名。
- 引用图展开后按叶子动作串行执行并标记来源工作流。本地执行记录最近 20 条、菜单显示
  5 条，只保存状态/进度/时间/定义摘要，不保存命令、任务正文或参数值。
- 当前页面会话内定义未变化时，失败记录可从失败叶子重试并沿用冻结上下文；定义变化或
  跨刷新后只允许重新开始整条流程。

后续尚未实现：项目菜单/快捷键引用完整映射、更多 Agent 环境模板。

验收与静态补丁：[工作流归一化验收](../../archive/audits/20260926-workflow-unification/README.md)；[工作流搜索验收](../../archive/audits/20260926-workflow-search/README.md)；[运行参数验收](../../archive/audits/20260926-workflow-runtime/README.md)；[执行记录验收](../../archive/audits/20260926-workflow-history/README.md)。

## 永久与临时切换预设（2026-07-27）

终端软键盘“项目指令”倒数七项使用统一的“持久性 / 终端位置 / 会话动作”命名：
`永久`、`临时`、`临时 / 新终端 / fork`、`临时 / 新终端 / resume`、
`临时 / 原地 / resume`、`临时 / 原地 / 新对话`、
`临时 / 新终端 / 新对话`。其中只有 `永久` 会调用共享 apply 接口，修改活动预设及用户
`config.toml` / `settings.json`。菜单从下往上数的六个预设动作
默认都是临时切换：
统一指定预设对话框的“临时切换预设”多选框默认勾选，通过 `webclx run` 应用目标真实配置，并在 Agent 启动确认后立即恢复原预设；
只有 `永久` 是显式永久入口，普通 apply 后不恢复。不存在运行期租约、延迟永久切换或 pending switch，也不使用私有配置目录或配置环境变量。

`终端内临时切换预设` 开始时冻结当前 webClx 终端 ID、
工作目录和 Agent Session，并通过统一高级 Session 提取链先取得完整 Session ID；提取失败时
不得退出 Agent 或切换预设。用户选定预设后，执行顺序固定为：向冻结终端提交 `/exit`、等待
xterm 显示 shell 提示符、向同一终端发送包装原生 `codex resume <id>` 或
`claude --resume <id>` 的启动命令，并在 Agent 启动确认后立即通过普通 apply 恢复原配置。命令不得附加
`-m` / `--model`；模型只来自启动期间应用的真实配置。等待期间即使用户切换了当前终端，退出和恢复
仍必须按冻结的终端 ID 路由。

若退出后的 resume 发送失败，对话框保留失败阶段供重试；已确认退出后不能再次发送
`/exit`。只有完整恢复命令成功发送
后才关闭对话框。
若等待 shell 提示符超时，对话框显示“忽略错误继续”按钮。该按钮是显式旁路：保留冻结的
终端、Session 和预设，不再次发送 `/exit`，直接进入原有启动或恢复阶段；其他错误不显示该按钮。

## 项目指令指定预设启动（2026-07-24）

终端软键盘“项目指令”提供 `指定预设+fork（临时）`、`指定预设+resume（临时）` 和
`指定预设临时终端`。三项
复用同一个指定预设对话框；对话框固定显示 `Session` 行。普通指定预设临时终端没有 Session，
保持空值；fork 与 resume 流程则显示执行前提取到的原 Session ID，提交后在当前项目目录
新建终端并恢复该原 Session（fork 走 `/fork` 分支，
resume 走 `codex resume` / `claude --resume`）。
fork/resume 入口打开后的对话框标题必须明确包含“临时”，普通入口使用“指定预设临时终端”，由
`openTerminalDesignatePresetDialog` 按命名动作派生。

## 统一高级 Session 提取与 /status 回退（2026-07-25）

`指定预设+resume` 在动作开始时冻结原终端 ID 与项目路径，检测当前 Session 后打开指定
预设对话框，命名动作为 `resume`，最终终端名预览与提交值均为 `<原终端名>_resume`；它不
执行 `/fork`，原终端保持不变。新终端以 `codex resume <id>` 或 `claude --resume <id>`
恢复原会话。

软键盘“快捷”中的“高级复制”、恢复当前会话以及指定预设的 resume/fork 基线
统一走 `detectAgentResumeIdComplete`。优先级固定为：当前冻结终端的屏幕提取、向该终端
发送 `/status` 并等待新输出、最后调用
`/api/terminal/sessions/<id>/agent-session/complete`。`/status` 对 Codex 和 Claude 都生效：
Codex 打印 `Session: <uuid>`，Claude 打印 `Session ID: <uuid>`，现有 banner 提取器覆盖
两种格式。普通“屏幕提取id”仍保持纯屏幕操作，同时作为高级链的第一步。

后端完整探测会重新检查 tmux 屏幕快照，再检查 agent 进程打开的 Codex/Claude 会话文件。
多个进程文件候选不再只按修改时间决定：先用当前屏幕中的真实用户消息与 rollout 对话匹配，
匹配分数相同才按修改时间排序。进程与屏幕都落空时，最后读取 Codex `history.jsonl`；该回退
必须至少匹配到当前屏幕文本，并优先限制为当前工作目录，不能仅凭最近修改时间猜测 Session。

`指定预设+fork` 必须冻结动作开始时的 webClx 终端 ID 和项目路径，通过快捷命令表中的
`fork` 调用 `/fork`。统一完整检测链只读取执行前的基线 Session；提交后必须监听冻结终端
xterm 的 `onRender`，并从相对执行前已经变化的最近 20 行
缓冲区中提取 `/fork` 输出的恢复命令，成功后才能打开指定预设对话框。Codex 的 `/fork`
会让原终端进入新分支，同时打印原 Session 的恢复命令，因此终端显示的 ID 通常与执行前
基线相同；不能再以“必须不同于基线”作为成功条件。对话框传入实际显示并提取到的 Session，
使新建的 `<原终端名>_fork` 终端恢复该会话。不能用进程 fd 提前变化代替终端显示完成，
也不能在等待期间读取 `state.activeSessionId` 或当前 xterm，否则会提前开窗或在用户切换
终端后提取错误的 Session。

指定预设对话框的终端名输入默认取动作开始时的原终端名，并允许修改。最终终端名必须在
输入框下实时预览：new/resume/fork 分别显示 `_new`/`_resume`/`_fork` 后缀，避免新终端
与仍然存在的原终端重名。
后端会把名称内所有 `_<数字>`/`#<数字>` 片段视为同项目自动编号占用；最终名计算必须先
移除这些编号再加动作后缀。例如 `webClx_18_整合预设` 预览为
`webClx_整合预设_new`。若该完整名称已存在，则使用不会被解析为自动编号的 `-2`、`-3`
递增后缀，并让预览值与实际提交值保持一致。
`指定预设+fork` 虽然恢复的是原 Session，但其命名动作固定为 fork；因此编辑基础名称后，
预览和实际创建名称都必须同步为 `<编辑值>_fork`，不能保留打开弹窗时计算的隐藏固定名称。

普通 `指定预设临时终端` 不显式传空名称，而由统一对话框从当前活动终端提取基础名，默认预览
并使用 `<原终端名>_new`。预设启动继续复用 `executeSpecifiedPreset` 的 launch
流程；终端页的 launch 回调等待新终端连接和首屏回放完成，之后再通过 auto-typed API 向
新终端稳定 ID 发送 Agent/resume 命令。

“新会话”的快速后缀只能在终端创建成功后拼接到创建响应返回的真实自动名上。提交前预览使用的
会话列表可能尚未加载或已经过期，不能把预览编号作为最终改名依据，否则会把新终端改回已被旧
终端占用的编号并触发自动编号冲突。预览打开或切换工作目录时必须用工作区相对路径读取该目录的
实时会话列表；加载期间显示计算状态，请求失败则显示创建时自动分配，不能伪造 `_1`。

固定终端启动完整成功后，统一指定预设对话框自动关闭；创建、改名、连接等待或
Agent 命令发送任一步失败时保留对话框，并显示错误供用户修改后重试。
创建终端成功后若改名、连接等待或命令发送失败，启动回调必须使用创建响应的稳定终端 ID
自动删除该新终端，并用确认头重复该 ID。清理不能退回使用当前活动终端，因为等待期间用户
可能已经切换终端；前端随后清除该 ID 的待确认状态、终端上下文和会话偏好，并优先恢复创建
前的原终端。原启动错误必须保留，只有自动清理也失败时才追加清理错误。

## 利器与“指定”Codex 任务（2026-07-24）

终端软键盘在“利器”旁提供独立“指定”按钮。对话框选择 Codex_API 预设、单次执行或
临时终端、任务文本和超时时间，统一提交到 `/api/codex/tasks`，轮询任务 ID 后显示预设
模型、Codex 实际模型、最终回复和临时终端关闭结果。关闭对话框不会取消后台任务；显式
“取消任务”才调用该任务 ID 的 DELETE 接口。

利器配置支持 `指定预设 → Codex 单次任务` 和 `指定预设 → Codex 终端任务`。动作链含
Codex 任务时，`指定预设` 只写入本次执行上下文，由任务 API 记住原预设、应用目标预设并在
启动确认后立即恢复；不能先调用旧的 apply 接口再提交任务。工作目录在动作开始时从原终端冻结，等待期间用户切换
终端不会改变任务目录。任务结果复用“指定”对话框展示；临时终端由后端按创建响应中的
唯一 session ID 自动关闭，不能清理当前活动终端或名称相同的既有终端。

## 利器 fork 会话（2026-07-21）

软键盘「利器」内置 `fork` 条目，使用单一的 `fork_session` 类型动作完成一条
有输出依赖的流程：在原终端发送 `/fork`，等待原终端缓冲区出现与执行前不同的
完整 resume 命令，再按顺序新建终端、发送该 resume 命令，并把新终端改名为
`<原终端名>_fork`。

执行开始时必须冻结原终端的 session ID、名称和项目路径。命令发送、输出读取、
新终端等待和改名都按稳定 session ID 定位，不能依赖等待期间可能变化的活动终端。
resume 就绪由 xterm 渲染事件和现有完整 UUID 解析器驱动，不使用固定延时猜测输出；
超时、命令发送失败、新建失败或改名失败均终止后续步骤，并由现有利器状态显示错误。

`/fork` 同时是内置的软键盘 `/斜杠` 命令。利器流程必须按 key 调用这条共享命令，
由 `sendSlashCommand` 先向冻结的原 session ID 写入 `/fork`，等待斜杠命令的默认
500ms 输入延迟，再发送 Enter 和既有的确认 Enter。不能把 `/fork` 直接交给
`auto-typed-input`：该接口用于 shell 命令准备，无法复现 Codex TUI 的斜杠命令
选择与延迟提交时序，实测会只留下换行。resume 命令仍走 auto-typed API。

## 利器新终端命令启动时序（2026-07-19）

利器动作链中的 `create_terminal → send_command` 必须把新建返回的 session ID
保存在本次动作执行上下文中。新终端 WebSocket `open` 不代表首屏已经可显示；发送
后续命令前还要等待该 session context 的初始 tmux 回放、xterm 输出队列和当前
`term.write()` 全部结束。

`send_command` 统一复用 `/api/terminal/auto-typed-input`，显式传递稳定的
`session_id`。不要用 `sendTerminalInput(command) + 定时延迟 + 回车` 模拟键盘：
该方式会中断首屏回放，也可能在用户切换终端后把命令写入错误会话。API 失败应向
利器动作链抛出，由现有状态提示展示失败，不能静默降级为 WebSocket 模拟输入。

## Terminal Keyboard Shortcuts (2026-07-18)

### Reliable External Terminal Messages (2026-08-28)

外部终端消息提交分为两个 API 操作：先通过
`POST /api/terminal/sessions/message` 注入 bracketed-paste 正文且不提交，再通过
`POST /api/terminal/sessions/{session_id}/enter` 调用软键盘“快捷 > Enter”。第二个 API
复用粘贴稳定等待窗口、提交确认和有界 Enter 重试，因此不会重复注入消息正文；
`--no-enter` 仍是仅插入模式。

The terminal page reserves these built-in shortcuts and dispatches them through
the same function-command actions used by the soft-keyboard menus:

- `Ctrl+K`: show or hide the temporary desktop soft keyboard.
- `Ctrl+B`: run the current project's build/deploy action.
- `Ctrl+Alt+S`: extract and copy the current agent Session ID.
- `Ctrl+Alt+T`: copy the current terminal name.
- `Ctrl+Alt+O`: sort terminal sessions by working directory.

`ensureBuiltInTerminalSlashCommands` and
`ensureBuiltInTerminalFunctionCommands` replace stale saved shortcut values for
these built-ins while loading both the terminal and Settings pages. Browser
coverage lives in `tests/terminal-function-shortcuts.browser.py`; its build API
is stubbed so the shortcut test never queues a real deployment.

### Project Deploy Command Ownership (2026-07-24)

`本项目部署脚本` belongs only to the soft keyboard's `项目指令` select. Legacy
saved `deploy_project` entries are filtered from both the `全能` function menu
and the `快捷` menu while settings load. `Ctrl+B` is declared on the project
command option and the shortcut dispatcher resolves project options before the
two configurable command lists, so removing the duplicate menu items does not
remove the keyboard shortcut.

## Project URL Configuration

The terminal soft keyboard exposes a `项目指令` collection with `部署` and
`项目 URL`. Project web entry metadata belongs to the project and is stored in
the project root as `.webclx.json`; webClx does not maintain a central
project-to-port registry.

For a service listening on a local port:

```json
{
  "web": {
    "port": 4173,
    "scheme": "http",
    "path": "/"
  }
}
```

`scheme` and `path` are optional. Without `scheme`, webClx uses the current
page protocol; the host always comes from the current page so the action also
works when webClx is accessed from another machine.

For a reverse proxy, independent domain, or a project whose entry follows the
current webClx origin, configure `url` instead. It takes precedence over
`port` and may be absolute or origin-relative:

```json
{
  "web": {
    "url": "https://app.example.com/"
  }
}
```

Only HTTP and HTTPS URLs are accepted. The terminal reads the file through
the existing workspace-scoped `/api/file` endpoint and opens it in a new tab.
`项目管理` is the sole owner of this project URL action; stale `WebUI` copies
in the quick menu are removed while loading.

`复制id并提问` uses the complete current-session detection chain and copies
`调用codex db维护skill读取session id为 <id>并接受指令：` through the same
clipboard helper as `复制终端名`. It does not write or submit text to the terminal.

The quick menu is normalized for both defaults and saved configurations:
ordinary actions, Session ID extraction/copy actions, `复制终端名`, then all `/`
commands at the bottom. This makes `复制终端名` fifth from the bottom, directly
above `/resume`. `套餐` is function-menu-only and stale saved quick-menu copies
are removed while loading.

## Deploy Callback Identity

Before submitting `/api/build/deploy`, the terminal page refreshes
`/api/terminal/sessions?all=true` and matches the current stable session ID.
The payload uses the latest terminal name together with `source_terminal_id`
and `source_tmux_session = webclx_<session-id>`. This keeps compile/deploy
completion callbacks routed to the initiating webClx terminal after a rename
or reconnect instead of relying on the page's cached session name.

## Terminal Message Rename Aliases

Terminal message delivery resolves targets in this order: stable session ID,
persisted previous names, then the current unique terminal name. Renaming a
session appends its former name to `previous_names` in `terminal-state.json`;
state loading rebuilds the alias map, so aliases survive restarts and are
scoped to the same workspace path. A message sent through an old name is still
delivered to the same session and returns `renamed_target: true` together with
`current_terminal_name`. The `terminal-notifications` script prints the new
name and its JSON response remains available to callers, allowing the sender
to switch targets on the next message while old in-flight senders continue to
work.

## Terminal Tools And Codex Full Access (2026-07-17)

The `终端工具` button sits immediately after `粘贴`. Its compact non-modal
dropdown starts with `Codex 最高权限`, followed by the configured quick
commands (normally `codex` and `claude`), `复制全部`, then the `api`,
`智能体`, and `继续` checkboxes. These controls no longer consume space in
the soft keyboard row. The dropdown matches the interaction style of
`项目指令`, prefers the space
above the trigger, stays inside the viewport, closes on outside click or
Escape, and restores focus to the trigger after Escape. The checkboxes keep
their original element IDs, so their existing persistence and session-state
bindings continue to work.

`Codex 最高权限` is a switch. Opening the dropdown reads its current state with
`GET /api/terminal/codex-full-access`. Switching it on calls `PUT`, saves only
the previous `approval_policy` and `sandbox_mode`, writes `"never"` and
`"danger-full-access"`, then sends the bare `codex` command through the
existing auto-typed-input API to the terminal session that was active when the
operation started. Switching it off calls `DELETE`, restores the two previous
values (or removes keys that were previously absent), and does not send a
terminal command. Models, providers, comments, and other settings remain
untouched in both directions.

The backend resolves the terminal user's HOME from the system user profile,
serializes the write with preset application, rejects config paths outside
that HOME (including dangling or escaping symlinks), creates a missing
`.codex` directory with mode `0700`, and writes the config and the two-value
backup with mode `0600` and the target user's UID/GID. Frontend changes include
`terminal-tools.js`, so deployment must sync the complete `static/` directory
instead of replacing only the binary.

### Terminal Directory Sorting (2026-07-19)

`终端工具` includes `终端按目录排序`. The menu action routes through
`runTerminalFunctionCommand`, sorts the terminal page's `state.sessions` by
`sessionDisplayPath`, and calls `renderSessions`; the active session remains
selected by ID. Do not call the workspace-page `sortAndRenderDirectorySessions`
from this button because only `index.html` loads `app-session-views.js`.

## Symptom

Clicking the 部署 button in the terminal soft keyboard fails with:

```
无法定位项目工作目录 /webClx: No such file or directory (os error 2)
```

`/api/build/deploy` rejected the request because `project_dir` resolved to a
root-level path (`/webClx`) instead of the configured workspace root +
relative path (`/home/codes/tools/webClx`).

## Root Cause

`triggerProjectDeploy` in [static/terminal-mobile-keys.js](../../static/terminal-mobile-keys.js)
joined `state.workspaceDir` with `state.currentPath` to build `project_dir`.
The terminal page's `state` object (defined in [static/terminal.js](../../static/terminal.js))
never declared a `workspaceDir` field and `loadTerminalSettings` never
populated one, so the runtime value was always `undefined` and the join
produced `/<relative path>`.

The workspace browser page (`/`) loads `state.workspaceDir` from
`/api/settings` in [static/app-settings-load-save.js](../../static/app-settings-load-save.js).
The terminal page reuses the same backend field but had not been updated to
fetch it, so the soft keyboard deploy button shipped with an uninitialized
base path.

## Fix Pattern

- Declare `workspaceDir: ""` in the terminal page's `state` object so
  `triggerProjectDeploy` always reads a string.
- In `loadTerminalSettings`, copy `settings.workspace_dir` into
  `state.workspaceDir` after the existing `hostName` initialization.
- Guard `triggerProjectDeploy` against an empty `workspaceDir` (e.g. settings
  request failed or hasn't returned yet) and surface a clear status message
  instead of submitting `/webClx` to the deploy API.
- Bump the cache-busting query string on the modified assets in
  [static/terminal.html](../../static/terminal.html) so the new bundle is
  served immediately.
- Add a test ([tests/terminal-soft-keyboard-deploy.test.mjs](../../tests/terminal-soft-keyboard-deploy.test.mjs))
  that pins the join expression, the guard, the bumped versions, and the
  presence of the `deploy_project` button.

## Verification

```bash
node --check static/terminal.js
node --check static/terminal-mobile-keys.js
node --check static/terminal-settings-loader.js
node tests/terminal-soft-keyboard-deploy.test.mjs
```

`/api/build/deploy` now accepts `project_dir = /home/codes/tools/webClx` and
`detect_install_command` resolves the project-level `scripts/deploy.sh`
or `scripts/rebuild-and-deploy.sh` script. Backend path normalization
(`compile_project_dir` in `src/compile_service.rs`) is unchanged.

## Compile-Stage Native Build Breaks Cross-Compile Projects (2026-07-12)

### Symptom

Clicking 部署 on a Windows cross-compile project (rustCommander, loongMsg) fails
in the compile stage:

```
error: failed to run custom build command for `atk-sys v0.18.2`
  The system library `atk` required by crate `atk-sys` was not found.
  The file `atk.pc` needs to be installed ...
```

or, before the worker-env fix, `error[E0463]: can't find crate for core`.

### Root Cause

Two independent issues combined to make every `--target` deploy fail:

1. `compile-worker.sh` is launched via `systemd-run`, which inherits a
   near-empty environment (`PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin`,
   `HOME=`). `cargo` resolves to `/usr/bin/cargo` (apt package 1.90, no
   rustup, no `x86_64-pc-windows-gnu` target) instead of
   `/home/root/.cargo/bin/cargo` (rustup 1.97 with the target installed).
   Missing target -> E0463 on every `--target` build.

2. Even after cargo resolves correctly, `compile_command` (deploy branch)
   short-circuited on `Cargo.toml` and returned `cargo build --release`
   (no `--target`) as the compile stage. For Windows projects whose
   `rebuild-and-deploy.sh` does its own `cargo build --target
   x86_64-pc-windows-gnu`, this compile stage builds a useless host-native
   binary that drags in Linux-only sys crates (atk-sys, gdk-pixbuf-sys, ...)
   and fails on missing system libs.

### Fix

1. `compile-worker.sh` top: restore `HOME` from `getent passwd`, prepend
   `$HOME/.cargo/bin` to `PATH`, and set `RUSTUP_HOME`/`CARGO_HOME` when
   the rustup dirs exist. The script is read by path at build time (not
   compiled into the binary), so the change takes effect without redeploy.

2. `compile_command` deploy branch: if the project has
   `scripts/rebuild-and-deploy.sh`, return the no-op
   `docs/codex/skills/webclx-rebuild/scripts/noop-compile.sh` as the compile
   stage. The install stage (`rebuild-and-deploy.sh`) owns both the
   cross-compile and the deploy. Projects without the script still get the
   native `cargo build --release` compile stage, so services deployed via
   `scripts/deploy.sh` are unaffected.

### Verification

- `cargo test --release -p webclx compile_service` (16 tests, including two
  new deploy-compile-inference tests).
- Direct `systemd-run --pipe` check that the bootstrapped worker resolves
  `cargo` to `/home/root/.cargo/bin/cargo` with the windows target visible.
- loongMsg deploy (request 232435) succeeded end to end: 1m17s cross-compile,
  rustCommander replace-restart, loongMsg.exe PID 416 online on HOST.
- rustCommander deploy via soft button is the remaining repro after the
  webClx self-deploy (request 233614) lands the `noop-compile` path.

### Files

- [compile-worker.sh](../../skills/webclx-rebuild/scripts/compile-worker.sh)
- [compile_service.rs](../../../src/compile_service.rs)
- [noop-compile.sh](../../skills/webclx-rebuild/scripts/noop-compile.sh)

## Configurable Workflows

The terminal `工作流` soft key and the settings `工作流搭建` panel are powered by a fully configurable workflow tree. No source edits or deployments are needed to add, modify, or remove workflows.

**Compatibility:** The persisted Settings field is `terminal_tool_entries` and the root key is `tools`. These internal names are preserved for backward compatibility even though the product-facing label is `工作流`.

### Schema

Each entry is either a `folder` (for multi-level menus) or an `action` (an executable workflow). Actions contain 1 to 20 serially-executed action steps. Supported action kinds:

- `create_terminal` - creates a new terminal in the current directory
- `fork_session` - forks the active Codex/Claude session into a new terminal
- `rename_terminal` - renames the current or last-created terminal
- `switch_api_preset` - selects a Codex API preset by ID
- `codex_exec` - runs a one-shot Codex exec task (requires a preceding preset selection)
- `codex_terminal` - runs a Codex terminal task (requires a preceding preset selection)
- `codex_launch` - launches a Codex terminal with structured configuration
- `wait` - pauses for N seconds
- `send_command` - sends a command to the current terminal

### codex_launch Fields

The `codex_launch` action supports the following structured fields:

- `value` - the initial task text (e.g. `$mihomo-proxy-ops ...`)
- `preset_selector` - preset ID or name to resolve
- `preset_match` - one of `id`, `exact_name`, or `unique_contains`
- `cwd` - absolute working directory path (e.g. `/home/system`)
- `project_path` - absolute project path passed to the preset apply endpoint
- `terminal_name` - display name for the launched terminal
- `session_action` - currently `new` only

### Preset Resolution

`resolveSpecifiedPreset` resolves presets deterministically:

- `id` matches the preset ID exactly
- `exact_name` matches case-insensitively by full name
- `unique_contains` prefers an exact name match, then requires exactly one substring match; ambiguity or no match throws before any terminal or session is created

### Multi-Level Menus

Folders support arbitrary nesting depth (within the 200-entry limit). The tree validates against missing parents, non-folder parents, self-parenting, and cycles.

### Serial Execution

Actions execute sequentially and stop on the first failure with a visible error status.

### Import/Export

Workflows can be exported as JSON (`{ version: 1, terminal_tool_entries: [...] }`) and re-imported. Invalid imports are rejected without modifying current state. Export contains only workflow selectors, never API credentials.

### Built-in Workflows

- `fork_session` - the fork shortcut
- `proxy_settings_workflow` - launches Codex with `miniMax` preset from `/home/system`, invoking `$mihomo-proxy-ops`

Both are merged into saved entries by stable ID and can be edited, moved, or removed by the user.

## 工作流快速搜索（2026-09-26，第二阶段）

终端“利器/工作流”菜单顶部提供搜索框，按工作流名称与完整目录路径跨根目录查找；空格分隔
的关键词必须全部命中。搜索只使用标签和目录元数据，不扫描动作正文或参数，避免把命令、凭据
等内容显示到结果中。结果点击继续调用现有工作流执行分发器；清空菜单后恢复原目录导航。
移动端菜单在搜索时使用受控宽度，目录上下文不进入视口外。验收见
`docs/archive/audits/20260926-workflow-search/`。

## 万能快捷键（2026-10-17）

“全能 → 万能快捷键”提供 Ctrl、Alt、Shift、Meta 多选与控制/导航、字母、数字、符号、F1–F12 按键选择，显示组合预览后点击发送。不附加额外 Enter，不触发浏览器或操作系统快捷操作。目标固定为打开弹窗时的 session ID，断线保留弹窗并提示重试；取消和 Escape 不发送。

实现由 `static/terminal-universal-shortcut.js` 独立管理，复用 `sendTerminalInputToSession`。常见组合使用传统控制字节、Alt 的 ESC 前缀及 xterm 导航/功能键序列；其余组合使用扩展 modifyOtherKeys 编码，需要终端应用支持。传统终端协议不能区分部分组合（例如 Ctrl+I 与 Tab）。

验证：`node --test tests/terminal-universal-shortcut.test.mjs`、`python3 tests/terminal-universal-shortcut.browser.py`，覆盖字节编码、桌面/移动弹窗几何、固定目标、断线、取消和 Escape；线上 HTTP 新脚本与源码哈希一致，静态改动无需重启。附带工作流回归 43/45 通过，剩余两个断言仍使用迁移前 `/home/codes/webClx` / `path=webClx`，相关实现文件与 HEAD 相同。

## 2026-10-18：全能误发温度命令与安卓入口核对

- 根因：设置页 `parseTerminalFunctionCommandsInput` 直接按 `|` 拆字段，Shell
  管道后的内容被错认作快捷键并丢弃。命令格式化改为 JSON 引用命令字段；解析兼容旧格式，
  旧格式只有空尾列或合法组合快捷键才作为快捷键列。
- 经本机设置 API 仅恢复 `system_temperature` 的完整命令并清空污染的快捷键，来源是
  `path-repath-backup-20260829T133147/webclx-settings.json`，先通过 `bash -n`，未向用户终端执行。
- 万能快捷键固定放在全能菜单首项，与底部自定义命令区分。
- HTTP 核对本机、HOST:PORT、PUBLIC_DOMAIN:11112 的 `/assets/terminal.html`
  均含万能入口。安卓返回前台仅派发 pageshow，不会重新加载 HTML；已打开旧页面需真实刷新。
- `tests/terminal-command-pipes.test.mjs` 覆盖命令管道、引号与快捷键往返。
  `tests/terminal-menu-history.browser.py` 使用完整页面与生产脚本，模拟桌面和安卓 touch，
  验证菜单首项打开对话框而不发送输入，对话史按钮可打开并显示模拟 API 数据。
  模拟测试不能替代用户真机的服务器与页面状态核对；用户随后确认“对话史好了”，未修改其业务逻辑。
- `tests/terminal-universal-shortcut.browser.py` 与原有 multiline history 浏览器测试通过。
  旧 `terminal-function-shortcuts.test.mjs` 仍要求已不存在的
  `MOBILE_ESCAPE_COMMAND_SEPARATION_MS = 12`，相关 transport 源码与 HEAD 一致，属既有断言漂移。

## 2026-10-18：万能快捷键内置全键盘

- 对话框下方改为可点击键盘，按常规键盘行排列字母、数字、符号、F1–F12、
  方向和导航编辑键；Ctrl/Alt/Shift/Meta 按钮与上方复选框双向同步。
- 选择键高亮；Shift 同步显示数字与标点的上档符号。键盘点击只更新选中组合，
  唯一的发送入口仍是“发送”，保持打开时冻结目标会话与断线报错。
- 使用独立版本化样式和脚本；375px 窄屏不横向溢出，对话框超高时内部滚动。
- 验证：组合键 Node 测试、桌面/移动键盘浏览器测试（高亮、修饰键同步、Shift 符号、
  点击不发送、目标冻结及取消）、完整桌面/安卓触屏页面与对话史回归。
