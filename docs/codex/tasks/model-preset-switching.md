# 模型预设切换与协议转换

本文梳理三类预设（`Codex_OAuth`、`Codex_API`、`Claude_API`）在切换时的处理方法，按提供商区分直连、本机中继、协议转换三种访问路径，并说明 LLM 代理转换的实现方式。

边界与禁止项见 [API 预设路由边界](api-preset-routing-boundaries.md)；本文只描述“实际怎么走”。

## 三类预设与对应入口

| 预设类型 | 前端表格 | 应用入口 | 写入文件 |
| --- | --- | --- | --- |
| Codex_OAuth | auth 表 | `apply_auth_preset` | `.codex/auth.json` + `.codex/config.toml` |
| Codex_API | api 表 | `apply_api_preset` | `.codex/auth.json`(`openai_api_key`) + `.codex/config.toml` |
| Claude_API | claude 表 | `apply_claude_preset` | `.claude/settings.json` (`env`) |

应用流程统一在 `src/auth/apply.rs`，核心配置写入函数在 `crates/auth_core/src/config.rs`，访问模式判定在 `crates/auth_core/src/lib.rs`。

每次切换都持 `state.auth_manager.lock_active_config_write()` 串行写锁，并对每个终端目标用户回读 `auth.json`/`config.toml`/`settings.json` 校验；任一目标不匹配即 apply 失败。

## Codex_OAuth（登录 Auth 模式）

`apply_auth_preset`（[src/auth/apply.rs:54](/home/codes/webClx/src/auth/apply.rs:54)）：

1. `write_login_auth_files` 把整份 `auth.json`（含 OAuth tokens）写到每个终端用户目录。
2. `sync_auth_preset_configs` 把预设的 `config_overrides`（经 `resolve_effective_preset_config_targets` 与全局默认条目合并）写入 `config.toml`，并清除 `model_provider`/`provider`——Auth 模式不写 provider 块。
3. 不涉及本地中转和协议转换；Codex 进程直接走官方 OpenAI ChatGPT 后端。

切换回 Auth 模式也可通过 `apply_current_auth`（[src/auth/apply.rs:384](/home/codes/webClx/src/auth/apply.rs:384)），它写 `auth.json` 后 `clear_config_providers` 清掉 provider 配置。

## Codex_API

`apply_api_preset`（[src/auth/apply.rs:90](/home/codes/webClx/src/auth/apply.rs:90)）。关键分叉由两个布尔决定：

- `api_preset_enables_local_upstream_proxy_on_apply(preset)` 读取 `preset.apply_upstream_proxy_on_switch`。
- `effective_api_responses_proxy(preset)` 返回 `responses_proxy` 字段，若为空则按 `infer_api_responses_proxy` 从 base_url / model / provider_name 推断。

### 三种访问路径

1. **直连（direct）**：`apply_upstream_proxy_on_switch=false` 且无 `responses_proxy`。
   - 写入真实 `base_url` 和真实 `api_key` 到 `config.toml` 的 `model_providers.webclx_api` 块。
   - wire_api 固定为 `responses`。
   - 适用：原生支持 OpenAI Responses 协议的上游（newapi、aicode、自建网关等）。

2. **本机 OpenAI 中继**：`apply_upstream_proxy_on_switch=true`。
   - base_url 改写为 `http://127.0.0.1:<port>/api/upstream/openai/v1`（`OPENAI_UPSTREAM_PROXY_BASE_PATH`）。
   - api_key 改写为占位 `webclx-local-api-proxy:<preset_id>`（`local_proxy_api_key_for_preset_id`）。
   - 真实 key 保存在预设里，由 `/api/upstream/openai/v1/*` 路由按 preset_id 取回后转发。
   - 适用：上游需要经过程序代理出口（如局域网/LAN 网关、需统一出口 IP）。

3. **协议转换（responses_proxy）**：`responses_proxy` 非空。
   - base_url 改写为对应转换路由（见下表）。
   - 请求体由 webClx 把 OpenAI Responses 格式转成上游原生 Chat Completions / Anthropic Messages 格式，再把响应转回 Responses。
   - 适用：上游只支持 Chat Completions 或 Anthropic Messages 协议。

### 模型（model）跟随 provider 原子切换

每个 Codex_API 预设必须在表单里填写**模型**字段，它以 `{key: "model", value: <模型名>}` 形式存进预设的 `config_overrides`。切换预设时，这个 model 与 provider（base_url/key）在同一次写入里一起落到 `config.toml`，杜绝「provider 切了但 model 还是上一个」的错配。

两层保障：

1. **预设自带 model（主路径）**：前端「模型」输入框是必填项（OAuth 代理预设除外），保存时合并进 `config_overrides`。编辑预设时，model 从 `config_overrides` 提取到独立输入框，其余 override 仍走通用编辑器，避免重复。`app-api-manager.js` 的 `mergeModelIntoOverrides` / `extractModelFromOverrides` / `overridesWithoutModel` 负责拆分合并。
2. **managed key 清理（兜底）**：即使某个预设没声明 model（比如规则确立前的旧预设），`model` 始终是受管理键（它在全局 `codex_default_config_entries` 里）。`sync_api_preset_config` 写入前会 `clear_inactive_managed_config_entries_in_content`：当前预设的 active config targets 不含 `model` 时，残留的 `model` 会被删除，让 Codex 回退内置默认模型，而不是用上一个预设的驴唇不对马嘴的 model。见回归测试 `applying_preset_without_model_clears_stale_model`（[crates/auth_core/src/tests.rs](/home/codes/webClx/crates/auth_core/src/tests.rs)）。

旧预设（无 model）编辑时会因必填校验提示补 model；补完保存后即自携 model，切换即生效。

预设编辑保存成功后，前端必须立即用 PUT 响应中的 `preset` 更新当前列表状态，不能只依赖随后
的 GET 刷新。列表 GET 与保存并发时，保存前发出的旧响应必须按修订号丢弃，并在当前请求结束
后补一次刷新；否则后端已经写入的新 `config_overrides` 会在重新编辑时短暂显示成旧值。

### 全局默认 config

Codex_API Tab 直接维护 `codex_default_config_entries`，作为 Codex_OAuth 与 Codex_API
预设共用的 `config.toml` 默认层。预设的同名 `config_overrides` 优先；未覆盖的默认项继续
生效。默认层的“保存默认值”只通过 `PUT /api/settings` 提交
`codex_default_config_entries`，不能连带覆盖设置页其它尚未保存的字段。

预设应用、活动预设更新、预设测试和隔离 Agent 执行都必须调用
`resolve_effective_preset_config_targets` 合并同一默认层，不得各自维护另一份默认值。

Codex_API Tab 的公共配置区同时区分两种所有者：

- “当前 config.toml”通过 `GET/PUT /api/settings/codex-common-config` 直接读取和修改终端用户
  HOME 下的 `.codex/config.toml`。固定开关只管理 Codex 支持的两个顶级特殊值：
  `approval_policy = "never"` 与 `sandbox_mode = "danger-full-access"`；关闭开关只移除相同的
  特殊值，不覆盖用户已经显式设置的其它模式。
- “预设默认值”仍只维护 webClx 的 `codex_default_config_entries` 默认层。界面把无点号键标为
  “顶级键”、点号路径标为“配置表”；`model_provider`、`model_providers.*`、`wire_api` 等标为
  “预设 Provider”，由预设的 Provider 写入器管理，不能当成普通公共键。

当前部署使用的 Codex CLI 严格配置校验不接受 `[tools.shell] confirm_commands = false`
（报 `unknown configuration field tools.shell`），因此 webClx 不写入也不提供这个无效开关。
命令审批由顶级 `approval_policy` 控制；文件系统和网络边界由顶级 `sandbox_mode` 控制。

### 命令行按模型选择

`webclx use api <selector>` 与 `webclx run api <selector> -- <agent>` 统一按
`ID → 唯一名称 → 模型首条` 解析。模型比较不区分 ASCII 大小写，命中多个预设时选择后端
保存顺序中的第一条。Codex_API 表格可在 `Base URL` 与`大模型`分组之间切换；模型分组内
上移/下移会持久化完整预设顺序，因此可直接控制模型参数默认命中的预设。

模型提取和选择规则由 `auth_core::api_preset_model`、`model_from_config_overrides` 与
`select_api_preset_index` 统一维护。`webclx use` 的永久切换、`webclx run` 和
`/api/agent/exec-with-preset` 的隔离执行只复用选择规则，不共享执行副作用。

`webclx use` 和 `webclx run` 都把当前 cwd 作为 `project_path` 传给 apply，使用户级
`config.toml` 与实际生效的项目级 `.codex/config.toml` 在同一次切换中同步。`webclx run`
在 apply 完成并回读配置后才启动 Agent，且不会用固定延时恢复旧预设；选中的共享预设会继续
保持为当前值。已运行的 Codex/Claude 进程不可靠地热加载 provider、Base URL 或凭据，切换
已有对话必须停止旧进程，再用原 thread id 启动新的 `resume` 进程。

项目目录中仅用于 `AGENTS.MD`、feature、MCP 等项目设置的 `.codex/config.toml` 不属于
预设路由配置。apply 不能因为文件存在就向其中注入 `model`、`model_provider`、上下文窗口或
模型目录；只有该文件原本已经声明顶层 `model`、`model_provider`、`provider` 或
`model_providers`，确实会覆盖用户级预设时，才同步项目级路由。否则用户级 `config.toml`
继续作为模型和 provider 的唯一所有者。

### responses_proxy 取值与路由

`ApiResponsesProxyMode`（[crates/auth_core/src/models.rs:330](/home/codes/webClx/crates/auth_core/src/models.rs:330)），对应路由常量在 `crates/auth_core/src/lib.rs:88-93`：

| 模式 | base_url 写入 | 转换路由 | 上游协议 |
| --- | --- | --- | --- |
| `direct` | 原始 base_url | 不转换 | OpenAI Responses |
| `openai_chat`（智谱 GLM） | 原始 base_url | 直连上游 | OpenAI Chat Completions |
| `minimax_chat` | `/api/codex-proxy/minimax/v1` | minimax 转换 | MiniMax Chat |
| `deepseek_chat` | `/api/codex-proxy/deepseek/v1` | deepseek 转换 | DeepSeek Chat |
| `anthropic_chat` | `/api/codex-proxy/anthropic/v1` | anthropic 转换 | Anthropic Messages |

注意：`openai_chat` 模式 base_url 保持原值、不走 `/api/codex-proxy/*`，但 wire_api 仍为 responses，靠客户端逻辑直接打上游 `/chat/completions`。

### 自动推断（infer_api_responses_proxy）

`infer_api_responses_proxy`（[crates/auth_core/src/lib.rs:1056](/home/codes/webClx/crates/auth_core/src/lib.rs:1056)）的判定顺序（命中即返回）：

1. base_url 含 `/anthropic` 或 `anthropic.com`，或 provider/model 含 `anthropic`/`claude` → `AnthropicChat`。
2. minimax 域名 + `codex-MiniMax-` 模型前缀 → `MinimaxChat`。
3. deepseek 域名或模型名（排除 `/anthropic` 路径与原生支持 Responses 的 `deepseek-v4-flash`） → `DeepseekChat`。
4. bigmodel.cn / GLM / 智谱（排除 `/api/codex-proxy/zhipu/`） → `OpenaiChat`。
5. 否则 `None`。

用户显式保存的 `responses_proxy` 优先于推断，其中 `direct` 明确表示不转换。只有缺少该字段的旧预设才执行自动推断；例如 DeepSeek v4 Flash 已原生支持 Responses，保存为 `direct` 后不得再按 DeepSeek 模型名强制改成 `deepseek_chat`。

### 自动本机入口建议（前端）

前端在 `static/app-api-manager.js` 的 `currentApiApplyProxyRecommendation`：当 `responses_proxy` 非空，或 base_url 命中设置页“自动匹配 provider”列表（GLM/智谱/deepseek/minimax 等），保存时自动勾选本机入口。用户手动取消后保存会二次确认（[api-preset-routing-boundaries.md](api-preset-routing-boundaries.md) 硬边界：后端不得按 provider 把 `false` 改回 `true`）。

### ChatGPT OAuth 访问模式（access_mode=chatgpt_oauth）

`StoredApiPreset.access_mode = ChatgptOauth` 且 `access_token` 非空时（[src/upstream_proxy.rs:99](/home/codes/webClx/src/upstream_proxy.rs:99)）：

- 凭据用 `access_token` 作 Bearer，不用 `api_key`（api_key 仅作本地占位身份）。
- base_url 固定为 `https://chatgpt.com/backend-api/codex`，请求拼后缀打到 ChatGPT 后端 Responses。
- 自动注入 `ChatGPT-Account-Id: <account_id>`。
- `apply_upstream_proxy_on_switch` 强制 true，始终走 `/api/upstream/openai/v1` 本机中继。

## Claude_API

`apply_claude_preset`（[src/auth/apply.rs:286](/home/codes/webClx/src/auth/apply.rs:286)）。

### ClaudeAccessMode 五种取值

`ClaudeAccessMode`（[crates/auth_core/src/models.rs:339](/home/codes/webClx/crates/auth_core/src/models.rs:339)）：

| 模式 | 含义 | base_url / token 写入 |
| --- | --- | --- |
| `direct` | 直连真实 Anthropic 兼容上游 | 真实 base_url + 真实 auth_token |
| `anthropic_proxy` | 旧字段，归一化为 `anthropic_relay` | — |
| `anthropic_relay` | 本机 Anthropic 中继 | `/api/upstream/anthropic` + 占位 `webclx-local-claude-proxy:<id>` |
| `openai_chat` | Anthropic Messages → OpenAI Chat 转换 | 本机入口 + 转换 |
| `openai_responses` | Anthropic Messages → OpenAI Responses 转换 | 本机入口 + 转换 |

`effective_claude_access_mode`（[crates/auth_core/src/lib.rs:1202](/home/codes/webClx/crates/auth_core/src/lib.rs:1202)）把 `anthropic_proxy` 归一为 `anthropic_relay`；旧字段 `use_local_proxy=true` 回退为 `anthropic_relay`，否则 `direct`。

### 写入逻辑（write_claude_preset_to_targets）

[crates/auth_core/src/lib.rs](/home/codes/webClx/crates/auth_core/src/lib.rs) `apply.rs:write_claude_preset_to_targets`：

- `direct`：`write_claude_settings_files` 写真实 base_url 和真实 token 到 `.claude/settings.json` 的 `env`。
- 其它四种：走 `set_claude_settings_in_value_with_endpoint`，base_url 换成 `claude_provider_base_url_for_mode(preset, true)`（`/api/upstream/anthropic`），token 换成占位 `webclx-local-claude-proxy:<id>`。

env 写入键（`crates/auth_core/src/config.rs:set_claude_settings_in_value_with_endpoint`）：

- `ANTHROPIC_API_KEY`（新）/ 删除旧的 `ANTHROPIC_AUTH_TOKEN`。
- `ANTHROPIC_BASE_URL`。
- `ANTHROPIC_DEFAULT_HAIKU_MODEL` / `ANTHROPIC_DEFAULT_SONNET_MODEL` / `ANTHROPIC_DEFAULT_OPUS_MODEL` / `ANTHROPIC_MODEL`（按预设模型字段）。
- 删除旧 `ANTHROPIC_SMALL_FAST_MODEL`。

### 全局默认 settings.json env

Claude_API Tab 直接维护 `claude_default_config_entries`，作为所有 Claude 预设共享的
`.claude/settings.json` `env` 默认层。“保存默认值”只通过 `PUT /api/settings` 提交该字段，
不能覆盖 Codex 默认项、设置页其它字段或任何 Claude 预设。

合并顺序固定为：全局 env 默认值 < 预设的 Haiku / Sonnet / Opus / 第三方模型字段 <
预设的 `config_overrides`。因此预设已选择模型时，同名全局模型变量不会压过它；高级选项中
显式填写同名变量仍可作为最高优先级覆盖。认证 token、Base URL 和旧模型变量不接受全局层
覆盖，始终由预设访问模式与凭据写入路径管理。

预设应用、活动预设更新和当前状态识别都使用
`resolve_effective_claude_config_overrides` 生成同一份有效配置。列表响应仍返回原始预设，继承的
全局值不会被误显示或保存成单个预设的专属配置。
- `config_overrides` 额外键值。

对 `anthropic_relay`/`openai_chat`/`openai_responses`，切换前还会 `activate_dynamic_claude_relay_if_needed`：把 `upstream_proxy_settings.claude_proxy_enabled=true` 并记 `active_claude_proxy_preset_id`。

### OpenCode 目标

`apply_claude_preset_to_opencode`（[src/auth/apply.rs:362](/home/codes/webClx/src/auth/apply.rs:362)）写 `<workspace>/opencode.json`，与终端 Claude 配置独立。

## LLM 代理转换实现

转换分两层路由：

### 1. `/api/codex-proxy/{provider}/v1/responses`

固定上游，客户端 `Authorization` 始终透传。见 `src/codex_proxy.rs`：

- `minimax_responses` / `zhipu_responses` / `deepseek_responses` → `proxy_responses`：Responses→Chat Completions。上游 URL 在常量里写死（`api.minimaxi.com`、`open.bigmodel.cn`、`api.deepseek.com`）。
- `anthropic_responses` → `proxy_anthropic_responses`：Responses→Anthropic Messages。上游 base_url 来自 `resolve_anthropic_preset` 匹配到的 API 预设，端点拼 `/v1/messages`。

转换函数在 `codex_proxy_core` crate：`responses_request_to_chat_request`、`chat_response_to_responses_payload`、`chat_request_to_anthropic_messages`、`anthropic_messages_response_to_chat_response`。MiniMax/DeepSeek 有专用 sanitizer（`sanitize_chat_request_for_minimax`、`sanitize_chat_request_for_deepseek`）。

### 2. `/api/upstream/openai/v1/*` 与 `/api/upstream/anthropic/*`

通用中继，按请求 path/method + 预设访问模式决定转换还是透传。见 `src/upstream_proxy.rs`：

- OpenAI：POST `/responses` 且预设有 `responses_proxy` → `proxy_openai_responses_conversion`；否则 chatgpt_oauth 走 Bearer+account_id，直连走预设 api_key 透传。
- Anthropic：POST `/messages` 且 `effective_claude_access_mode` 为 `openai_chat` → `proxy_anthropic_messages_to_openai_chat`；为 `openai_responses` → `proxy_anthropic_messages_to_openai_responses`；否则透传到 `preset.base_url`。

Anthropic↔OpenAI 双向转换在 `src/upstream_proxy/transform.rs`：`anthropic_messages_request_to_openai_chat`、`anthropic_messages_request_to_openai_responses` 及反向 response 转换。

## 普通 Codex 直接读取用户配置

webClx 终端中的普通裸 `codex` 不设置 `CODEX_HOME`，不安装 launcher，也不生成或加载
`.terminal-command-env` 会话脚本。快捷启动和恢复都把原始命令直接交给 shell；真实 Codex 按原生规则读取
终端用户 HOME 下的 `~/.codex/auth.json`、`~/.codex/config.toml` 和项目级配置。启动时会删除带
`WEBCLX_CODEX_COMMAND_ENV_WRAPPER` 标记的旧 launcher，但保留用户自己创建的同名文件。
API 预设的 `terminal_env` / `terminal_startup_script` 也不注入普通终端；HOME 与所有配置目录
环境变量不能通过终端默认环境覆盖。

API 预设切换在 `active_config_write` 写锁内用 `toml_edit` 按键更新共享 `config.toml`，把
`model`、provider、base URL、wire API 和该预设的其它受管理键作为同一次切换写入并回读校验。
新启动的 Codex 因此直接读取切换后的共享配置，不经过额外配置副本。已运行的 Codex 不会由 webClx
热重载；切换后必须退出并重新启动进程，才能确定读取新的 provider 和 model。

中转/转换模式仍写入带 preset_id 的占位 token：
`webclx-local-api-proxy:<preset_id>` / `webclx-local-claude-proxy:<preset_id>`。路由层可从
进程实际发送的 token 定位预设，且占位 token 永远不会透传上游。

“永久切换预设”和 `webclx run` 都调用共享 apply 并同步当前项目本地配置。`webclx run`
应用目标预设后直接启动真实 Agent，目标预设保持为共享配置的当前值，不再保存和定时恢复旧
配置。真正的一次性隔离任务仍由原生任务后端按自己的可验证生命周期管理；不能把交互式
resume/fork 伪装成“一次性任务”，也不能重新引入固定等待或临时 `run.sh`。

`is_local_proxy_placeholder_token`（[src/upstream_proxy.rs:814](/home/codes/webClx/src/upstream_proxy.rs:814)）保证 scoped token 永远被当作预设身份标识、绝不会透传给上游。

### 多个普通 Codex 进程的配置边界

所有手工裸启动的普通 Codex 进程共享终端用户的 `~/.codex`。永久切换会更新这份共享配置；之后新启动的
进程读取新预设。已经运行的进程继续使用内存状态还是重新读取配置，由 Codex 自身决定。
目录信任同样由普通 Codex 直接写入共享 `config.toml`，无需 webClx 在退出时合并。

### 新启动与恢复旧会话的区别

- webClx 终端中的裸 `codex` 是新进程启动：shell 直接解析真实 Codex，进程读取当前用户级 `.codex/auth.json` / `.codex/config.toml`。切换后重新执行裸 `codex` 必须同时使用新预设的 provider、base URL、key 和 model，但仍要服从下面的项目级配置覆盖规则。
- `codex resume <uuid>` 除读取当前配置外，还会读取旧 rollout。Codex 0.144.1/0.145.0 的 rollout 会持久化历史 `turn_context.payload.model`，普通恢复可能用它覆盖当前默认 model，因此旧 GLM 会话直接恢复到 sub2api provider 时可能出现「新 provider + 旧 model」。这是 Codex 原生恢复层的第二个模型来源。
- webClx 自动生成直接的 `codex resume` 时，在命令发送前读取目标终端用户真实 `~/.codex/config.toml` 的顶级 `model`，先把该 rollout 的历史 `turn_context` 模型改写为当前模型，再附加官方 `--model` 参数，从而只恢复 rollout 的对话与上下文，不恢复旧模型。关停重启恢复和“中断并恢复”应复用同一准备逻辑。
- 指定预设恢复由 `webclx run` 包装原生 resume/fork。外层终端命令准备不能在预设应用前注入旧配置中的模型；`webclx run` 必须在目标预设应用后，用应用结果返回的 model 构造 Codex 参数。用户在 shell 中手工输入、未经过 webClx 命令准备接口的裸 `codex resume` 仍保持 Codex 原生行为。

### `webclx run` 持久预设启动

- `webclx run <type> <preset> -- <agent>` 先用当前 cwd 调用共享 apply，确认 `deferred=false`，再从 apply 返回的实际配置文件读取模型并启动真实 Agent。
- CLI 不再在 `750ms` 或其它固定时间后恢复旧 provider；目标预设保持生效。若 apply 因另一个序列化操作返回 `deferred=true`，`webclx run` 会持续等待该序列化操作结束，期间不启动 Agent；配置写入完成后才读取实际配置文件并启动，因此不会用陈旧 provider/model 启动。
- 前端指定预设启动（包括 fork/resume）统一生成 `webclx run ... -- <agent>` 命令；deferred 时新终端先创建并等待，不会直接执行裸 `codex` 或 `claude`。
- 多个 dispatch worker 可以并行工作，但启动交接必须逐个进行：前一个 Codex 已出现真实 TUI banner、证明完成启动配置读取后，才能让下一次 `webclx run` 改写共享配置。并行的是后续工作，不是共享配置交接。
- `webclx use <type> <preset>` 同样传递 cwd，用于只切换共享配置而不启动 Agent。

### cwd 中的项目级配置覆盖

- Codex 还会读取当前工作目录中的 `.codex/config.toml`。`webclx use` 和 `webclx run` 都会同步当前项目文件；不同预设的启动配置交接不能并发写入，已完成启动的进程可以继续并行运行。
- webClx 的 API 预设 apply 只管理设置页 `terminal_user` 对应 HOME 下的用户配置。以 `root`、HOME=`/home/root` 为例，apply 响应中的权威路径是 `/home/root/.codex/config.toml`，不会覆盖项目目录 `/home/.codex/config.toml`。
- 排障时同时核对 `GET /api/settings` 的 `terminal_user_home`、apply 响应的 `config_file`、终端进程 `/proc/<pid>/environ` 中的 `HOME`，以及 cwd 祖先是否存在项目 `.codex/config.toml`。只检查路径相似的旧目录容易误判。
- 真实项目有意设置的 model override 应保留，不能在切换全局预设时批量删除。若祖先 `.codex` 是旧 HOME 遗留目录，应先确认无进程使用并备份旧配置，再移除该项目级 `config.toml`，让 Codex 回退用户配置。
- 不要只把旧目录的 `config.toml` 链接到新 HOME。配置中的 `model_catalog_json` 等相对路径会按链接所在目录解析，可能导致 Codex 启动时报文件不存在；要么迁移整个 `.codex`，要么仅备份并移除误生效的项目配置。

### 自定义 API 模型目录

Codex_API 预设应用会同步项目级 `model_catalog.json`，让 Codex 认识 GLM、MiniMax 等非官方
模型。自定义条目可以复用官方模板的能力字段，但必须覆盖模型身份字段，并显式设置
`upgrade: null`；否则模板中的官方升级目标会让 Codex 把自定义模型误判为已退役的官方模型。

刷新已有条目时，只清理由 WebClx 创建的自定义模型描述所对应的陈旧 `upgrade`，官方模型的
升级元数据必须保留。若启动停在 `Choose how you'd like Codex to proceed`，先核对当前配置中的
model/provider/Base URL 以及项目级模型条目的 `upgrade`，不要自动选择迁移目标或把 GLM 切成
GPT-5.6 Terra。

### 旧通用 token 兼容路径（仅读侧 fallback）

裸 `webclx-local-api-proxy` / `webclx-local-claude-proxy`（无 `:id` 后缀）是历史遗留兼容标识。**当前所有写入路径只产出 scoped token，裸 token 不再被写入配置**；它只存在于识别层，用于兼容本规则确立前创建的旧会话。

裸 token 请求无法解析出 preset_id，会落到第 3 级 fallback（全局 `active_*_proxy_preset_id`）。这意味着旧会话在切换 active 预设后**可能**被重路由——这是兼容代价，不是新行为。禁止的修复见边界文档：不得为了让旧会话跟着切换，就让通用 token 永久指向当前 active。新会话一律走 scoped token，彻底规避此问题。

### 旧通用 token 回退的可观测性

为了在仍有进程使用裸 token 时能被发现，`/api/upstream/openai/v1/*` 与 `/api/upstream/anthropic/*` 在请求未能通过 scoped token 或 header 解析出 preset_id、只能落到全局 `active_*_proxy_preset_id` 时，会同时做两件事（`warn_active_fallback`，[src/upstream_proxy.rs](/home/codes/webClx/src/upstream_proxy.rs)）：

- 记录一条 `warn` 日志，含 channel（OpenAI/Claude）、命中的 preset_id 和脱敏凭据摘要。
- 通过 `TerminalManager::broadcast_toast`（[src/terminal/manager.rs](/home/codes/webClx/src/terminal/manager.rs)）广播一条全局 toast：toast 带**空 `session_id`**，前端对 `!message.session_id` 的 toast 不区分当前会话一律显示（`static/terminal-layout-connection.js` 的 toast 处理），所以每个已打开的 webClx 页面都会看到。

这是纯告警，不改路由结果——fallback 仍按 active 预设转发，只是让运维知道这台机器上还有旧会话在用裸 token，可以考虑重启那些进程让它们重新用 scoped token。新会话不会触发该告警。

### 预设身份解析（凭据来源优先级）

`/api/upstream/*` 路由按以下顺序定位预设（[src/upstream_proxy.rs:65](/home/codes/webClx/src/upstream_proxy.rs:65)）：

1. header `x-webclx-upstream-preset-id`。
2. 占位 token 解析出的 preset_id（`local_proxy_api_preset_id_from_api_key` / `local_proxy_claude_preset_id_from_token`）。
3. `upstream_proxy_settings.active_api_proxy_preset_id` / `active_claude_proxy_preset_id`。
4. Anthropic 侧还可按请求体 `model` 字段匹配预设（`claude_preset_from_request_model`）。

凭据选择（`client_provided_credential`）：客户端带真实非占位凭据 → 透传；占位 token 或无凭据 → 用预设保存的真实 key。详见边界文档的“客户端凭据透传规则”。

## 应用后状态推导

`derive_current_mode`（auth_core）按 `auth.json` 和 `config.toml` 推断当前是 `auth` / `api` / `none`。`current_api_summary` / `current_claude_summary` 产出脱敏状态供前端状态条显示。`verify_api_preset_targets` 在 apply 末尾回读校验 base_url、wire_api、凭据身份和全部 config overrides。

## 关键文件速查

- 访问模式与 base_url 改写：[crates/auth_core/src/lib.rs:1049](/home/codes/webClx/crates/auth_core/src/lib.rs:1049)
- 三类 apply 入口：[src/auth/apply.rs](/home/codes/webClx/src/auth/apply.rs)
- config.toml / settings.json 写入：[crates/auth_core/src/config.rs](/home/codes/webClx/crates/auth_core/src/config.rs)
- 预设数据模型与枚举：[crates/auth_core/src/models.rs:189](/home/codes/webClx/crates/auth_core/src/models.rs:189)
- 固定上游协议转换：[src/codex_proxy.rs](/home/codes/webClx/src/codex_proxy.rs)
- 通用中继与转换分叉：[src/upstream_proxy.rs](/home/codes/webClx/src/upstream_proxy.rs)
- Anthropic↔OpenAI 转换函数：[src/upstream_proxy/transform.rs](/home/codes/webClx/src/upstream_proxy/transform.rs)
- 网关路由注册：[src/routes/gateway.rs](/home/codes/webClx/src/routes/gateway.rs)
- 前端 API 表与本机入口建议：[static/app-api-manager.js](/home/codes/webClx/static/app-api-manager.js)
- 前端 Claude 表与 access_mode：[static/app-claude-manager.js](/home/codes/webClx/static/app-claude-manager.js)
