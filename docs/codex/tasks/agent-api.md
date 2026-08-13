# Agent API 与外部智能体集成

## Agent 页内终端智能体

Agent 页的终端智能体配置固定 API 预设、工作目录、项目路径、Skill、初始任务和终端名称。
Agent 独立页面负责按 profile 展示、恢复和新建智能体会话；终端管理页的主下拉仍列出所有活动
会话，让用户可以直接切换到 `origin=agent` 或 `origin=workflow` 的终端。主下拉有活动会话时
不得插入“终端列表”占位项，否则当前受管会话会显示成空选择。

Agent 页面加载时读取 `/api/terminal/sessions?all=true`，按 profile owner key 匹配会话，并用
`last_opened_at` 恢复全局最近打开的智能体。恢复 URL 只带 `embedded=agent`、`path` 和 `session`，
不得带 `agent_profile`，因此刷新或再次进入 Agent 页面不会创建新会话。旧会话没有 owner key 时，
允许用 `origin=agent`、空 owner key 和 profile `cwd` 做一次兼容匹配。

列表中的“打开”只恢复该 profile 最近会话；“新建”和会话工具栏的“新会话”才加载
`/terminal?embedded=agent&agent_profile=<id>`。终端创建仍由
`static/terminal-work-agent.js` 和共享的指定预设执行逻辑完成。`embedded=agent` 只隐藏重复的
顶级导航，不创建另一套终端协议、WebSocket 或进程。

Agent 页内按 profile 管理智能体会话，工具栏提供当前 profile 的会话选择器和“新会话”；
终端管理页只提供统一的活动会话切换和通用终端操作，不能据此接管或改变 Agent 会话所有权。
内嵌终端仍复用 xterm、WebSocket 和进程协议，但自动继续等页面动作只能作用于当前内嵌会话，
不能遍历同目录的其他智能体或普通终端。`404 Not Found` 属于路由或模型配置错误，默认只标记，
不自动继续。

内嵌终端初始化时可能先恢复浏览器上一次选择的会话。Agent 父页面必须保持启动遮罩，直到
子终端通过同源 `postMessage` 回传本 profile 创建的终端 ID、预设名和模型；不得提前展示
旧终端内容。启动失败也通过同一消息边界回传并停留在错误状态。父页面必须同时校验
`origin`、消息来源 iframe 和 `profileId`，不能接受其他终端页或旧 iframe 的状态消息。

Profile 中的预设是新会话的启动绑定，不是 Agent 页的全局预设锁。启动链可以短暂快照并应用目标预设，但只在 Codex/Claude 读取启动配置的交接窗口内持有租约，随后必须立即恢复用户原来的共享预设；交互式子进程继续使用启动时已经读取的配置直到自行退出。不得把 profile 的 `preset_selector` 写入全局 `api_preset_id`，不得把 MiniMax 或其他 profile 预设设为不可切换，也不得用租约心跳把共享预设锁到会话结束。一次性 `codex exec` 任务仍需在任务全程持有租约，因为它的完整生命周期由任务 API 管理。

## 原生用户配置 Codex 任务

`POST /api/codex/tasks` 是第三方程序、skill、“利器”和终端页“指定”按钮共用的
Codex 执行入口。请求按 ID、精确名称或精确模型选择一个 Codex_API 预设。任务取得全局
预设租约后，先保存真实用户配置和项目本地配置，再应用目标预设并以同一系统用户直接启动
`codex exec`。不生成 `run.sh`，不传 `--model` 或 `--sandbox`，也不设置配置目录环境变量。
启动后从 Codex banner 读取实际模型，不一致时终止任务；Agent 退出后精确恢复原配置。

请求立即返回任务记录，调用方用任务 ID 查询状态和最终结果：

```bash
created=$(curl -fsS -X POST http://127.0.0.1:11111/api/codex/tasks \
  -H 'Content-Type: application/json' \
  -d '{
    "mode": "exec",
    "preset": {"name": "Grok"},
    "cwd": "webClx",
    "task": "检查当前项目并汇报结论",
    "timeout_secs": 1800
  }')
task_id=$(printf '%s' "$created" | jq -r '.id')
curl -fsS "http://127.0.0.1:11111/api/codex/tasks/$task_id" | jq .
```

`mode=exec` 直接运行一次 ephemeral `codex exec`；`mode=terminal` 创建请求专属的临时
webClx 终端，在其中执行任务，并在成功、失败、取消或超时后只关闭该请求创建的终端。
状态依次为 `queued`、`applying_preset`、`starting`、`running`、`collecting`，最终状态为
`succeeded`、`failed`、`timed_out` 或 `cancelled`。最终记录的 `result` 是最后回复，
`actual_model` 是 Codex 实际启动模型，`terminal_closed` 表示临时终端是否完成清理。
取消尚未结束的任务使用 `DELETE /api/codex/tasks/{task_id}`；列出最近任务使用
`GET /api/codex/tasks`。

### 任务退出状态权威边界

`mode=exec` 与 `mode=terminal` 使用不同的退出状态来源，不得互相兜底或等待对方的产物：

- `mode=exec` 由 webClx 直接持有原生 `codex exec` 子进程，子进程的 `ExitStatus` 是唯一权威。
  退出码 `0` 表示成功，非零退出码原样进入失败处理；Unix 信号终止没有退出码，必须作为
  runner failure 并保留信号与进程状态证据。
- `mode=terminal` 没有可供任务监控器等待的原生子进程句柄，仍以终端包装脚本原子写入的
  `exit-status` 文件为权威。该文件只属于终端模式协议。

2026-08-05 曾出现 `mode=exec` 子进程正常退出 `0`，却被报告为“runner 提前退出且未写入状态”。
根因是监控器在原生子进程已经终止后仍要求存在仅由终端包装脚本写入的 `exit-status` 文件。
修复后监控器按是否持有原生子进程句柄选择权威来源，并用纯单元测试覆盖退出 `0`、退出 `7`
和 Unix 信号终止；`mode=terminal` 的状态文件写入与读取协议保持不变。相关实现与测试位于
`src/codex_task.rs`。

skill 调用时应把 POST 返回的 ID 保存在本次调用上下文中，轮询对应 ID，只有最终状态
才向调用者汇报；需要取消时也只能删除这个 ID。不要通过 shell 拼接 API key，不要设置
配置目录重定向变量，也不要把页面当前选中的终端当作临时终端清理目标。远程调用仍遵循下文的
登录 cookie 认证规则。

前端的预设指定入口统一调用 `static/specified-preset-actions.js` 中的
`executeSpecifiedPreset(options)`。`action=apply` 只应用预设，`action=launch` 通过短暂启动租约启动
常驻终端，`action=task` 提交上述原生 Codex 任务 API。可选参数包括 `agent`、
`presetId`/`presetName`/`presetModel`、`cwd`、`projectPath`、`command`、`terminalName`、
`sourceTerminalName`、`sessionAction`、`sessionId`、`quickStart`、`mode`、`task`、
`timeoutSecs`、`outputSchema`、`onCreated`、`onProgress` 和 `waitForResult`。`agent=codex` 启动
`codex`，`agent=claude` 启动 `claude`；`sessionAction` 支持 `new`、`resume`、`fork`，后两者
必须带规范 UUID。没有显式 `terminalName` 时，恢复终端命名为
`<sourceTerminalName>_resume`，fork 终端命名为 `<sourceTerminalName>_fork`。终端软键盘入口
默认选择常驻固定终端；Claude 的单次/临时任务尚未接入 Codex 专用任务 API，只能走固定
终端并立即恢复原共享预设。历史工作区 fork、工作区“指定”、终端软键盘“指定”和利器 Codex 动作都必须复用
该入口，不再各自拼接 apply 或 `/api/codex/tasks` 请求。

## 租约预设执行

`POST /api/agent/exec-with-preset` 用指定 Codex_API 预设在指定工作目录执行一次自然语言任务。它复用 `/api/codex/tasks`：保存原配置、应用目标预设、持有全局租约并运行 `codex exec --ephemeral`。请求结束、超时、取消或启动失败后都恢复原配置。

请求中的 `preset` 必须且只能包含 `id`、精确 `name`、精确 `model`、`index` 或 `current: true` 之一。名称重复或无法精确匹配时必须失败；`model` 按保存顺序选择第一条精确匹配预设。可选 `output_schema` 会传给 Codex `--output-schema`；响应返回原始 `output` 和可解析时的 `structured_output`。调用方仍需校验业务必需字段。

任务执行继承用户 shell 网络变量、终端默认环境和当前程序代理。HOME、PATH、用户身份、工作目录及配置目录环境变量均受保护，预设不得覆盖；`terminal_startup_script` 不执行。

该接口是跨模型核验的执行原语，不负责决定共识。不同预设任务受同一全局门禁约束并串行执行；调用方应让两个预设分别返回结构化结论，再让双方复议。

Date: 2026-07-11

## 概述

webClx 提供两种让外部智能体（其他项目中的 Codex/Claude/自定义 agent）调用本项目能力并获取汇报的途径：

1. **内置 Agent API** — webClx 自带一个 agentic loop，内置 skill 执行工具，外部调用者只需发消息即可让 webClx 端完成 skill 任务并返回结果。
2. **终端消息 API** — 向一个正在运行 Codex/Claude 的 webClx 终端发送文本消息，驱动该终端用 skill 完成任务，再通过反向消息返回汇报。

两条路径的代码入口：`src/agent.rs`（Agent API）和 `src/terminal.rs`（终端消息）。

## 认证

认证中间件在 `src/auth_guard.rs` 的 `require_auth`。

1. **Loopback 豁免**：来源 IP 是 `127.0.0.1` / `::1` 的请求不需要认证。本机其他进程直接调用即可。
2. **远程调用**：需要先 `POST /api/auth/login`（body `{ user, password }`）拿 session cookie，后续请求带该 cookie。无 Bearer token / API key 机制。

远程调用场景必须先登录：

```bash
# 登录拿 cookie，保存到本地
curl -sS -c /tmp/webclx-cookie.txt -X POST http://<host>:11111/api/auth/login \
  -H 'Content-Type: application/json' \
  -d '{"user":"root","password":"<password>"}'

# 后续请求带 cookie
curl -sS -b /tmp/webclx-cookie.txt http://<host>:11111/api/agent/skills
```

## 内置 Agent API

### 工作流程

三步：建会话 → 发消息（SSE 流）→ 读会话状态。

```bash
# 1. 创建 agent 会话。api_preset_id/model 留空时快照当前 Agent 预设与模型
curl -sS -X POST http://127.0.0.1:11111/api/agent/sessions \
  -H 'Content-Type: application/json' \
  -d '{"title":"外部任务","api_preset_id":"","model":""}'
# 返回中包含实际绑定的 api_preset_id 和 model

# 2. 发消息，agent 自动跑 skill/tool 循环，SSE 流式返回
curl -N -X POST http://127.0.0.1:11111/api/agent/sessions/<session_id>/chat \
  -H 'Content-Type: application/json' \
  -d '{"message":"用 webclx-compile-and-deploy skill 编译部署当前项目"}'

# 3. 拿完整会话状态（含 assistant 回复和 tool 调用历史）
curl -sS http://127.0.0.1:11111/api/agent/sessions/<session_id>
```

### API 端点

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| GET | `/api/agent/sessions` | 列出所有 agent 会话 |
| POST | `/api/agent/sessions` | 创建会话，body: `{ title, api_preset_id, model, system_prompt }`；预设与模型留空时快照当前值 |
| GET | `/api/agent/sessions/{id}` | 获取会话详情（含全部消息） |
| PUT | `/api/agent/sessions/{id}` | 更新会话，body: `{ title, api_preset_id?, model? }` |
| DELETE | `/api/agent/sessions/{id}` | 删除会话 |
| POST | `/api/agent/sessions/{id}/clear` | 清空会话消息 |
| POST | `/api/agent/sessions/{id}/chat` | 发消息，返回 SSE 流，body: `{ message }` |
| GET | `/api/agent/tools` | 列出 agent 可用的 tool 定义 |
| GET | `/api/agent/models` | 列出可用模型和当前默认 |
| GET | `/api/agent/config` | 获取 agent 配置 |
| PUT | `/api/agent/config` | 保存配置（default_model / disabled_skills / extra_skill_dirs / system_prompt_override） |
| GET | `/api/agent/skills` | 列出所有已发现 skill（含 disabled 状态） |
| POST | `/api/agent/skills/toggle` | 开关 skill，body: `{ skill_name, disabled }` |
| POST/DELETE | `/api/agent/skill-dirs` | 添加/删除额外 skill 搜索目录 |

### 内置工具（tool）

Agent 的 agentic loop 自动发现并调用以下 11 个 tool。tool 定义在 `src/agent.rs` 的 `tool_definitions` 函数。

1. **`list_skills`** — 列出 `~/.codex/skills/` 下所有 skill，返回名称、描述和路径。
2. **`read_skill`** — 读取指定 skill 的 SKILL.md 全文。参数: `skill_name`。
3. **`run_skill_script`** — 执行 skill 目录下的脚本。参数: `skill_name`、`script_path`（相对 skill 目录）、`args`（可选）、`cwd`（可选）。自动选择解释器：`.py` 用 `python3`，`.sh` 用 `bash`。
4. **`run_command`** — 在指定工作目录执行 shell 命令。参数: `command`（通过 `bash -c` 执行）、`cwd`（可选）。
5. **`list_files`** — 有界列出工作区文件。
6. **`search_files`** — 使用结构化参数在工作区执行文本搜索。
7. **`read_file`** — 按行分页读取工作区 UTF-8 文件。
8. **`apply_patch`** — 先检查再原子应用 unified diff，文件必须位于工作区内。
9. **`git_diff`** — 查看工作区 Git 差异。
10. **`create_checkpoint`** — 创建受控 Git 检查点。
11. **`run_verification`** — 运行有超时和输出上限的验证命令，返回退出码与证据。

工具执行输出上限 64 KB（`MAX_SKILL_OUTPUT_BYTES`），超出时截断保留首尾各 32 KB。工具循环最多 15 轮（`MAX_TOOL_ITERATIONS`）。
Skill 默认从终端用户的 `~/.codex/skills` 发现；项目或团队 Skill 目录通过 Agent 设置的
`extra_skill_dirs` 或 `/api/agent/skill-dirs` 加入。名称冲突时先发现的用户级 Skill 优先。
`read_skill` 和 `run_skill_script` 必须使用同一搜索顺序，并拒绝禁用 Skill、跨根目录的 Skill
名称以及逃出 Skill `scripts/` 目录的脚本路径。会话刷新后，页面按 `tool_call_id` 把已保存的
`tool` 消息重新关联到原工具卡；已完成结果不得退回“运行中”，也不得再重复显示成独立原始消息。

### `$Skill` 显式调用

原生 Agent 聊天输入框支持与 Codex 一致的 `$skill-name` 调用方式。输入 `$` 或点击输入框旁的
`$` 按钮会打开全部已启用 Skill；继续输入时按名称精确匹配、前缀、包含、描述关键词和名称
子序列排序。匹配会忽略空格、标点、连接符和下划线，仅保留中英文与数字进行比较；例如
`web clx_codex` 可以匹配 `webclx-codex-api-terminal-ops`。方向键切换，`Enter` / `Tab` 插入，
`Esc` 关闭，鼠标或触摸也可直接选择。

发送含 `$skill-name` 的消息时，后端先按 `/api/agent/skills` 的实际结果校验名称和启用状态，
然后在首次 LLM 请求前自动执行并持久化对应 `read_skill` 工具调用。工具卡会显示加载结果，模型
直接接收已加载的 `SKILL.md` 内容；不存在或已禁用的 Skill 返回明确的 400 错误。重复引用同一
Skill 只加载一次。外部 API 调用者可以直接发送：

```json
{"message":"$mihomo-proxy-ops 检查当前代理配置"}
```

### SSE 事件格式

`POST /api/agent/sessions/{id}/chat` 返回 `text/event-stream`。每条 SSE `data` 是一个 JSON 对象，`type` 字段标识事件类型：

| type | 含义 | 关键字段 |
| --- | --- | --- |
| `assistant_message` | LLM 生成的文本回复 | `content` |
| `tool_call_start` | LLM 决定调用工具 | `id`, `name`, `arguments` |
| `tool_result` | 工具执行完成 | `id`, `name`, `result`, `is_error` |
| `done` | 对话轮次结束（无更多 tool 调用） | — |
| `error` | 执行出错或达到迭代上限 | `message` |

外部智能体从 `assistant_message` 事件提取汇报内容，从 `tool_result` 提取执行细节。`done` 事件表示该轮循环结束，流关闭。

### LLM 凭据解析

新会话会持久化 `api_preset_id`，原生 Agent 页可为每个会话独立切换 API 预设；切换时同步该预设保存的模型。`resolve_llm_credential` 按“会话绑定预设 → Agent 全局预设 → 当前实际应用预设”的顺序获取调用目标。旧会话没有 `api_preset_id` 时继续按后两级回退，保持兼容。如果预设已删除或当前配置无法匹配已保存预设，chat 请求返回 400 错误。

LLM 的预设目标解析、HTTP 传输和响应解析统一放在 `src/llm.rs`。目标解析同时供 API 预设测试和 Agent 使用，并按预设选择 Chat Completions、OpenAI Responses 或 Anthropic Messages；显式启用的本地中继会使用 preset-scoped token 和 preset id，不会改写全局 active state。Anthropic 兼容预设在直连时使用原生 Messages，在启用本地中继时使用 `/api/codex-proxy/anthropic/v1/responses` 完成 Responses→Anthropic 转换，不经过通用 OpenAI 上游入口。

Agent 和 API 预设测试通过 `src/llm/environment.rs` 使用同一套 HTTP 环境：终端用户 shell 环境 → 终端默认环境 → 预设 `terminal_env` → 当前应用代理，并显式应用最终的 `HTTP_PROXY`、`HTTPS_PROXY`、`ALL_PROXY` 与 `NO_PROXY`。Agent 在一次对话工具循环开始前构造一次客户端；预设测试只执行一次最多 16 token 的无工具对话，不创建 Agent 会话，也不会执行 skill 或 shell 命令。

Agent 与中转网关不合并为同一个 handler。`src/upstream_proxy.rs` 和 `src/codex_proxy.rs` 仍负责原始请求转发、客户端凭据透传、响应 header/status 和 SSE 网关语义；Agent 只复用预设路由决策与协议调用层。

### 外部智能体集成示例（Python）

```python
import json, requests

BASE = "http://127.0.0.1:11111"

# 创建会话
r = requests.post(f"{BASE}/api/agent/sessions", json={"title": "编译任务", "model": ""})
session_id = r.json()["id"]

# 发消息并读 SSE 流
resp = requests.post(
    f"{BASE}/api/agent/sessions/{session_id}/chat",
    json={"message": "列出所有 skill，然后用 webclx-compile-and-deploy 编译部署"},
    stream=True,
)
for line in resp.iter_lines():
    if line.startswith(b"data: "):
        event = json.loads(line[6:])
        if event["type"] == "assistant_message":
            print("汇报:", event["content"])
        elif event["type"] == "tool_result":
            print("工具结果:", event["result"])
        elif event["type"] == "done":
            break
        elif event["type"] == "error":
            print("错误:", event["message"])
            break
```

## 终端消息 API

驱动一个正在运行 Codex/Claude 的 webClx 终端用 skill 完成任务，通过反向消息返回汇报。

### API

```text
POST /api/terminal/sessions/message
```

请求体（`src/terminal.rs` 的 `TerminalMessageRequest`，约第 678 行）：

```json
{
  "target": "webClx#3",
  "data": "[from my-agent] 请用 webclx-compile-and-deploy skill 编译部署",
  "submit": true,
  "submit_enters": 1,
  "bracketed_paste": true,
  "verify_submission": true,
  "delivery_id": "[from my-agent] 请用 webclx-compile-and-deploy skill 编译部署"
}
```

目标终端可用 `target` / `session_id` / `terminal_name` / `name` 任一字段指定。消息内容可用 `data` 或 `message`。`submit_enters` 上限 4。

编译回调等必须自动唤醒 Codex/Claude 的长消息使用可确认投递模式：

```json
{
  "target": "s1234",
  "data": "[from webClx-compile-api] ... 请求 054328 ...",
  "submit": true,
  "submit_enters": 1,
  "bracketed_paste": true,
  "verify_submission": true,
  "delivery_id": "054328"
}
```

- `bracketed_paste` 把正文按终端 bracketed-paste 协议发送，明确区分快速粘贴正文和后续 Enter。后端按正文长度等待 600–2000ms，让 Codex/Claude 完成 paste-burst 聚合后才发送首次 Enter，避免提交键被吸收成编辑区换行。
- `verify_submission` 使用 Codex/Claude rollout 对话历史确认 `delivery_id` 已成为真实 user message。响应里的 `submitted: true` 才代表已经提交；HTTP 200 或 `ok: true` 只表示 API 调用成功。
- 每次 Enter 后后端最多等待 2 秒确认 rollout；未确认时按 1/2/4 秒退避补发独立 Enter，不重复正文，避免同一回调被投递两次。`submit_attempts` 是本次写入 Enter 的总次数。可靠调用方应允许至少 30 秒 HTTP 时间并同时检查 `.ok == true && .submitted == true`。
- 编译 worker 先发送浏览器 toast，再等待目标终端 `connected && !busy` 后投递 prompt。toast 是即时状态通知，rollout 确认的 prompt 是继续原任务的可靠入口。
- Codex/Claude 终端消息默认应设置这些字段；普通 shell 可保留原始输入兼容行为。

### 回复机制

被叫终端收到消息后，需要用同一 API 或 `terminal-message` skill 向原终端反向发送结果：

```bash
# 推荐使用打包脚本，自动处理 JSON 转义、sender tag、wait-ready
python3 /home/root/.codex/skills/terminal-message/scripts/send_terminal_message.py \
  --target 'webClx#1' \
  --from 'webClx#3' \
  --reply-base-url 'http://sender-webclx:11111' \
  --message '编译部署完成，request_id: 115417，审计 modified=1' \
  --request-reply
```

`--request-reply` 会自动追加回复指令，告诉对方用 terminal-message skill 回到发送方终端。跨 webClx 服务发送时还必须通过 `--reply-base-url` 或 `WEBCLX_REPLY_URL` 提供发送方可访问的 HTTP 地址；该地址会与发送方终端名一起写入消息，接收方无需预先知道发送方 IP。发送前脚本会查询回复端点的会话 API，要求发送方终端唯一可解析；远程目标不能使用 loopback 回复地址。自动提交正文会折叠为安全单行，`--no-enter` 插入模式仍保留多行。

### skill 脚本

`terminal-message` skill（`/home/root/.codex/skills/terminal-message/`）封装了终端消息 API，提供 `--wait-ready`（等待目标终端空闲后再发送）、`--no-enter`（只插入不提交）、`--submit-enters`（控制首次回车次数）、`--no-verify`（普通 shell 跳过 rollout 确认）等选项。Codex/Claude 终端默认只发一次初始 Enter；若 rollout 未确认，后端再按退避补发 Enter。浏览器标签是否在前台不改变服务端 PTY 写入路径。

终端名未知时，脚本可用 `--agent codex|claude` 配合 `--path` 查询 `/api/terminal/sessions?all=true`，按 `activity_agent` 解析唯一目标；候选不唯一时会报出候选名称，不会猜测。指定 `--start-if-needed` 后，若唯一目标只是 shell，脚本先调用 `/api/terminal/auto-typed-input` 启动 Agent，轮询会话列表直到 `activity_agent` 匹配，再发送正式消息；目标已运行另一种 Agent 时拒绝覆盖。

## 两种方式的选择

| 场景 | 推荐方式 |
| --- | --- |
| 外部智能体无需本地运行 Codex/Claude，只想要结果 | 内置 Agent API |
| 需要利用已有 Codex/Claude 终端的完整上下文 | 终端消息 API |
| 结果必须从同一 HTTP 响应提取 | 内置 Agent API（SSE 流） |
| 跨主机驱动远程终端 | 终端消息 API（`--base-url`） |
| 外部智能体没有 LLM 调用能力 | 内置 Agent API（webClx 端自带 LLM） |

## 代码入口

- Agent 会话管理和 agentic loop：`src/agent.rs`
- Agent 路由注册：`src/main.rs`（约第 576-604 行）
- 工具定义和执行：`src/agent.rs` 的 `tool_definitions`（353）、`execute_tool`（747）
- 终端消息发送：`src/terminal.rs` 的 `send_session_message`（1729）
- 认证中间件：`src/auth_guard.rs` 的 `require_auth`（21）
- skill 搜索目录：`~/.codex/skills/`（主目录）+ agent 配置的 `extra_skill_dirs`

## 验证

```bash
# 列出 agent 工具
curl -sS http://127.0.0.1:11111/api/agent/tools | jq .

# 列出可用 skill
curl -sS http://127.0.0.1:11111/api/agent/skills | jq '.skills | length'

# 列出可用模型
curl -sS http://127.0.0.1:11111/api/agent/models | jq .
```
