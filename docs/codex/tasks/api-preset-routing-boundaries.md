# API 预设路由边界

本文定义 Codex_API / Claude_API 在账号切换、本地中转、协议转换、测试探测、fallback、兼容迁移中的路由边界。

## 最高原则

**配置边界优先于修复目标。**

用户把一个预设配置成什么访问模式，系统就必须按这个访问模式执行。不能为了达到“切换生效”“测试通过”“兼容旧会话”“自动修复”“模型匹配”等目的，偷偷扩大这个预设的路由权限。

换句话说：修 Bug 时可以修配置写入、终端环境、代理身份、状态显示、测试探测，但不能把用户没有选择中转的预设强行改成中转。

## 硬边界

- `direct` 就是直连。Claude_API 预设如果配置为 `direct`，应用时必须写入真实上游 `base_url` 和真实 token，不得写成 `/api/upstream/anthropic`。
- 本地中转和协议转换必须是每个预设显式选择的访问模式。只有保存为 `anthropic_relay`、`openai_chat`、`openai_responses` 或兼容旧本地代理模式的预设，才允许写 webClx 本地 Anthropic 入口。
- Codex_API 的自动代理匹配规则只属于 Codex_API。不要因为 Claude_API 的 provider 看起来像国内模型、chat 模型、或和 Codex_API 的自动代理 provider 相似，就把 Claude_API 也强行走本地中转。
- Codex_API 的自动代理匹配只负责编辑器默认建议。用户手动取消“本机入口”并保存后，`apply_upstream_proxy_on_switch=false` 是权威值；后端不得再按 provider、Base URL 或 model 把它改回 `true`。
- Codex_API 的协议转换同样以预设显式选择为准。保存为 `responses_proxy=direct` 表示上游原生支持 Responses，后端不得再按 provider、Base URL 或 model 强制推断为 Chat → Responses；缺少该字段的旧预设才允许兼容推断。
- 编辑已有 Codex_API 预设时，前端必须把已保存的“本机入口”值视为用户选择；自动匹配可以提示风险，但不得在打开编辑器或再次保存时把直连 `false` 改回 `true`。自动勾选只适用于尚未形成已保存选择的新建表单。
- Codex 和 Claude 只读取终端用户真实 HOME 下的共享配置。不得创建配置副本、安装配置 wrapper，或设置配置目录重定向环境变量。
- `webclx use` 与 `webclx run` 只操作终端用户真实 HOME 下的共享配置，并把当前 cwd 传给 apply 以同步实际生效的项目级配置。二者都是持久 apply；`webclx run` 不得在固定延时后恢复旧预设。真正的一次性隔离任务可由原生任务后端保存和恢复配置，但必须有可验证的任务生命周期，不能把交互式终端启动当成定时恢复任务。
- 本地代理模式必须保留路由身份。新写入的占位凭据应带预设 id，例如 `webclx-local-api-proxy:<preset_id>`、`webclx-local-claude-proxy:<preset_id>`，让已经运行的进程固定到启动时的预设。
- 旧通用 token 只是兼容路径。允许按请求体 `model` 推断预设，再 fallback 到 active preset；但新配置写入必须优先使用 preset-scoped token。

## 禁止的捷径修复

不要用下面这些方式“解决”表象问题：

- 不要因为已打开的空终端没有读到新配置，就把 direct Claude_API 预设改成中转。
- 不要因为想让已运行 Claude 跟着切换，就让通用中转 token 永远指向当前 active preset。
- 不要因为模型不匹配报错，就把请求路由到另一个 active 预设来掩盖错误。
- 不要让测试接口应用预设、改写配置文件、切换 active proxy state，或改变预设是否使用本地代理。
- 不要让 fallback、迁移、同步、导入、自动修复路径拥有比主路径更大的权限。主路径不能改访问模式，fallback 也不能改。

## 正确修复方式

当一个问题看起来需要跨越路由边界才能解决时，应该修边界的责任方：

- 空终端重新启动 Claude 没读到新账号：修终端环境清理或启动命令准备，让新进程读取当前配置文件。
- 指定 Agent 启动时有人同时切换预设：串行完成 apply，并让当前 Agent 到达可验证的启动就绪状态后，下一次启动才可改写共享配置；目标预设保持生效，不通过私有配置目录绕开，也不靠固定延时猜测配置已经读完。
- 当前状态显示不对：修状态推导或 summary 逻辑，不要为了让显示匹配而改预设配置。
- API 账号、Base URL、provider 与 model 出现跨预设混配：把当前凭据和配置文件写入视为同一个串行切换操作；命令行切换、手动切换和编辑活动预设不得并发交错写入。
- 应用 API 预设必须在同一串行写锁内回读所有目标用户的 `auth.json` 和 `config.toml`，核对凭据身份、Base URL、wire API 和全部有效 config overrides；任一目标不匹配时 apply 必须失败，不能先报告成功再等待后续列表刷新发现混配。
- API 预设的 config overrides 是一组受管理配置：切换或更新活动预设时，必须写入当前有效项，并删除其他 API 预设曾管理但当前未声明的键；不得让 `model_context_window` 等上一个预设独有值泄漏到新模型，也不得删除未被预设管理的用户自定义配置。存在 `model_catalog_json` 时，`model_context_window` 还必须同步更新对应模型条目的 `context_window` 与 `max_context_window`，不能保留按模型名猜测的旧窗口。
- API 预设的模型目录必须跟随目标终端用户当前安装的 Codex：应用预设或更新活动预设时，运行该用户的 `codex debug models --bundled`，以当前二进制的 bundled catalog 为基底，合并既有自定义条目，按 slug 大小写不敏感去重，再补入预设当前模型。这样 Codex 升级后，下一次预设切换会自动获得新官方模型，不需要在 webClx 中硬编码模型版本。
- `config.toml` 尚未配置 `model_catalog_json` 时，默认初始化同目录下的 `model_catalog.json`，并且必须先成功写入目录文件再写配置引用；已有自定义目录路径不得改名或迁移。bundled catalog 暂时读取失败时，已配置目录仍可更新当前模型；首次初始化必须明确失败，不能生成缺少官方基底的冻结目录。
- `GET /api/auth/api-presets/{preset_id}/verify` 只用于只读核对当前磁盘状态，不应用预设、不修改 active 或代理状态。响应只能返回脱敏 current API、当前 model 和不匹配字段描述，不得回传 API key 或任意配置原值。
- API 预设的 `terminal_env` 不得覆盖 HOME 或任何配置目录变量，`terminal_startup_script` 禁用，避免形成配置文件之外的第二配置源。普通终端网络环境来自终端全局设置与当前应用代理，需要改变时新建终端。
- 已创建的终端保留创建时的网络环境；切换预设不会在终端内生成受保护脚本或热刷新环境。需要新预设 `terminal_env` 或应用代理时必须新建终端。模型、provider、base URL 和凭据仍以进程启动时读取的当前用户 `auth.json` / `config.toml` 为准。
- 应用 API 预设时必须把 `upstream_proxy.active_api_proxy_preset_id` 同步为当前预设 id；该字段是旧裸 token 的 fallback 身份，不能因为 scoped token 已覆盖新会话就长期停留在上一个本地代理预设。直连预设应用不得关闭 `codex_api_proxy_enabled`，避免中断仍依赖本地代理的旧会话。
- 普通手输的裸 `codex` 读取当前用户共享 `auth.json` / `config.toml`。webClx 自动生成直接的 `codex resume` / `codex fork` 命令时，必须在发送命令前读取目标终端用户当前 `config.toml` 的顶级 `model`，并通过 Codex 官方 `--model` 参数传入；`webclx run` 则必须携带 cwd、等目标预设完整应用后，使用 apply 返回的实际配置文件读取 model 并注入其包裹的 resume/fork。不得在预设应用前用旧配置提前注入，也不生成临时 `run.sh`。
- 快捷启动环境刷新与预设测试构造网络环境时，必须保留 webClx 服务进程实际传给新终端的 `NO_PROXY` / `no_proxy` 基线，再按用户 shell、终端默认值、预设和当前应用代理依次覆盖。不能因为净化后的 shell 探针只读取 `.bashrc` 而把服务级直连白名单当成“未配置”，否则 LAN 上游会在测试或实际 Codex 启动时误走程序代理。
- 启动时清理旧 `.terminal-command-env/` 目录；普通终端路径不得再次创建该目录或写入代理凭据。
- Codex CLI 不支持把 `https://` 代理 URL 直接用于 Responses WebSocket 或 HTTPS transport。HTTPS 程序代理用于终端 Codex 时，必须改写为绑定 loopback 的本地 HTTP 代理入口，由 webClx 透明地以 TLS、原始域名和 SNI 连接对应 HTTPS 代理，并在桥接层替换真实 Basic Auth；桥接身份必须固定到代理预设 id，不能动态跟随后续 active 代理。
- Codex 0.144.1 恢复会话时可能从 rollout 的历史 `turn_context` 回填请求模型。webClx 对自动生成的 `codex resume <uuid>` 会在发送前先把该 rollout 的 `payload.model` 与 `collaboration_mode.settings.model` 改写为当前配置模型，再附加 `--model`，同时保留原 UUID 和对话上下文。手工输入、未经过 webClx 命令准备接口的裸 `codex resume` 仍遵循 Codex 原生优先级。
- API 预设表格的单次、批量和定时测试必须按新终端的环境合并顺序构造独立 HTTP 客户端：终端用户 shell 环境 → 终端默认环境 → 预设 `terminal_env` → 当前应用代理。测试客户端必须显式应用最终的 `HTTP_PROXY` / `HTTPS_PROXY` / `ALL_PROXY` 与 `NO_PROXY`，并继续按预设本身的 direct / 本地中继选择测试 URL；不能复用 webClx 进程客户端而造成“测试成功、终端失败”或相反结果。
- 测试 API 预设时，模型列表成功但预设模型不在列表内，必须直接返回“预设模型不在上游模型列表”的明确失败，不再发起对话探针；对话探针收到上游模型不存在类错误时，同样要转成“没有该模型”的提示，不能只显示 `Service temporarily unavailable`。
- Responses 对话探针不能只凭 HTTP 2xx 判成功。必须完整读取响应体，并确认 SSE 收到 `response.completed`（或非流式 JSON 明确标记 `status=completed`）；HTTP 200 后断流、缺少终止事件或响应体读取失败都必须显示为测试失败。
- Responses 可用性探针的输出预算必须容纳推理模型的内部 reasoning token；预算过低会让实际可用的模型以 `response.incomplete` 正常结束并被误判失败。当前短探针使用 128 tokens，同时仍严格要求 `response.completed`。
- 测试某个预设必须经过本地代理：给这次测试请求带显式 preset id、header 或 scoped token，不要改全局 active state。
- 跨模型核验必须串行取得同一全局租约。每个请求保存原配置、应用目标预设并在结束后恢复；请求失败、取消或超时同样必须恢复，不能并发运行不同预设。
- access-token-only Codex_OAuth 凭据可以缺少 `id_token` 与 `refresh_token`；内部兼容空值，但写入 `auth.json` 或规范化 JSON 时必须省略空字段。保留 `"id_token": ""` 会让 Codex 把空串当 JWT 解析并报 `invalid ID token format`。
- Codex_OAuth 多账号文本必须先复用单账号规范化函数完成全量校验，再逐项调用既有单账号保存 API。该流程不是服务端事务；中途请求失败时页面必须报告已保存数量并保留原输入，不能显示整体成功。
- Codex_API 的账号文件导入支持标准 `auth.json`、CPA 多文件扁平账号、sub2api `accounts[]`、JSON 数组/流，以及 `.zip`、`.tar`、`.tar.gz`、`.tgz`、`.gz` 递归嵌套归档。多个文件必须在同一个 multipart 请求中共同解析并一次性持久化可用账号；单个坏 JSON 可以在响应 `errors` 中报告，但归档层级、条目数、展开大小或路径安全检查失败时必须拒绝整批导入。当前边界为整批上传 32 MiB、展开 128 MiB、2048 个条目、12 层嵌套，并且不得把归档内容写入文件系统。
- Codex_API 账号导入继续复用 `chatgpt_oauth` 预设语义，只创建预设，不应用预设、不切换 active 状态，也不改写当前用户的 `auth.json` / `config.toml`。批量生成的预设 id 必须在同一批次及已有仓库内保持唯一。
- Codex_OAuth、Codex_API、Claude_API 的账号列表剪贴板导出只包含当前表格勾选的预设，并使用带 `format`、`version`、`section`、`accounts` 的 `webclx-account-presets` 格式。导入必须校验页面类别与 payload 类别一致，按预设 id 更新或追加，保留未导入账号；导入和导出都不得应用预设、切换 active 状态或改写当前客户端配置。
- 远端 HTTP 页面不属于浏览器安全上下文，可能完全没有 `navigator.clipboard`。账号列表导出必须回退到用户手势下的隐藏 textarea 复制，自动复制仍失败时展示可选中的导出内容；导入必须打开手动粘贴窗口，并继续复用相同的格式、类别和服务端校验，不能把 Clipboard API 缺失当作终止条件。
- 老配置需要兼容：兼容逻辑只能识别和映射旧语义，不能借兼容名义改变用户没有选择的访问模式。

## 验证要求

改动触及 API 预设路由时，至少要覆盖下面的正反路径：

- 应用 direct Claude_API 预设时，写入真实上游配置，不写本地代理 token。
- 应用 relay / conversion Claude_API 预设时，写入 preset-scoped 本地 token。
- 带 preset-scoped token 的请求，在其他预设变成 active 后仍然路由到原预设。
- 旧通用 token 请求优先按请求体 `model` 推断预设，再 fallback 到 active preset。
- 测试接口不切换 active preset，不改写用户配置文件。
- direct API 预设测试使用真实上游 URL，并在最终终端环境含无效代理时失败；为目标主机加入预设级 `NO_PROXY` 后，同一测试应绕过代理成功。
- Codex_OAuth 预设测试直接使用目标预设保存的 `access_token` 与 `ChatGPT-Account-Id` 发起一次 Responses 对话；不得先应用该账号再测试，也不得启动带工具执行能力的 Agent 循环。
- Codex_OAuth 预设测试必须使用当前 active 的程序代理，不回退到服务进程或终端环境代理；测试结果必须标明实际代理名称、协议、地址和认证状态。
- 需要 Basic Auth 的程序代理由代理预设单独保存用户名和密码。列表、active 状态和编辑表单不得回传已保存密码；测试已保存预设时由后端按 `preset_id` 读取凭据并构造代理客户端。

## 对外网关与客户端凭据透传

webClx 默认只允许 loopback 访问 `/api/upstream/*` 与 `/api/codex-proxy/*` 代理路由。当局域网内的外部项目（另一台电脑上的 Codex/Claude 客户端）需要通过 webClx 当中转代理时，需要在设置页开启对外网关开关 `gateway_listen_non_loopback`（默认 `false`）。这条边界记录该开关与客户端凭据透传的规则。

### 网络访问边界

- `gateway_listen_non_loopback=false`（默认）时，所有代理路由只接受 loopback 来源；非 loopback 请求一律 `403`。本机终端行为完全不变。
- `gateway_listen_non_loopback=true` 时，代理路由额外接受非 loopback 来源。webClx 不做客户端鉴权——服务是否提供，最终取决于路由转发到上游后上游的鉴权结果。这是局域网部署假设；公网暴露场景必须自行在 webClx 前面加反代/鉴权。
- 进程本身始终监听 `0.0.0.0:11111`，`is_loopback()` 是唯一对外防线，开关只控制这条防线，不改监听地址。

### 客户端凭据透传规则（凭据来源优先级）

`/api/upstream/openai/v1/*` 与 `/api/upstream/anthropic/*` 在选择发往上游的凭据时，按以下优先级：

1. **客户端带真实非占位凭据** → 透传客户端凭据给上游。客户端在 `Authorization: Bearer <key>` 或 `x-api-key: <key>` 里携带上游真实 key，webClx 只做协议转换 + 按预设 base_url 转发，凭据用客户端的。
2. **客户端带占位 token** → 回到预设凭据。`webclx-local-api-proxy:<id>`、`webclx-local-claude-proxy:<id>`、旧通用 `webclx-local-api-proxy`、`webclx-local-claude-proxy` 都是 webClx 预设身份标识，**永远不是客户端凭据**，必须走预设解析路径，用预设里保存的真实上游 key。
3. **客户端无凭据** → 预设兜底。旧的本机直连客户端行为不变。

`/api/codex-proxy/{minimax,zhipu,deepseek}/v1/responses` 这三条路由 base_url 由路由常量绑定，客户端 `Authorization` 始终透传（实现上不读预设 key）。

### 占位 token 识别（关键约束）

`is_local_proxy_placeholder_token` 必须同时识别两类占位 token：

- preset-scoped：`webclx-local-api-proxy:<id>` / `webclx-local-claude-proxy:<id>`（带 id 后缀，由 `local_proxy_*_preset_id_from_*` 解析）。
- 旧通用兼容：裸 `webclx-local-api-proxy` / `webclx-local-claude-proxy`（无 id 后缀，是边界文档定义的动态中继兼容路径）。

误把旧通用 token 当作客户端凭据，会让本机终端走错上游 key。`client_provided_credential` 必须把这两类都排除掉。

### 已运行终端的边界

- 本机终端通过 `apply_*_preset` 写入占位 token，永远走预设凭据路径，不受 `gateway_listen_non_loopback` 影响。
- 对外网关开关和客户端凭据透传只影响"客户端在请求里自带真实凭据"的路径，不改变预设应用、active state、终端配置写入的任何行为。

### ChatGPT OAuth 访问模式（Codex_API 新增）

当 `StoredApiPreset` 的 `access_mode` 为 `chatgpt_oauth` 且 `access_token` 非空时，代理行为如下：

- 凭据来源：使用 `access_token` 作为 `Bearer` token，**不**使用 `api_key`（`api_key` 仅作为本地代理占位 token 身份标识）。
- URL：使用 `preset.base_url`（导入时自动设为 `https://chatgpt.com/backend-api/codex`）拼接请求后缀，映射到 ChatGPT 后端 Responses 端点。
- Header 注入：自动注入 `ChatGPT-Account-Id: <account_id>`，其中 `account_id` 从导入文本的 `chatgpt_account_id` 或 access_token JWT payload 的 `https://api.openai.com/auth.chatgpt_account_id` claim 恢复。
- `apply_upstream_proxy_on_switch` 强制为 `true`，确保请求始终走 webClx 本机入口。
- 这里有两层独立代理：LLM 代理固定为 `/api/upstream/openai/v1` 本地中继；网络代理复用 Codex_OAuth 当前启用的程序代理，由 `ProxyManager::build_oauth_client` 构造。测试结果必须分别标明两层，不能把终端 `HTTP_PROXY` 摘要当成 OAuth 网络代理。
- `chatgpt_oauth` 预设测试必须跳过普通 API 的 `/models` 探针，直接复用 Codex_OAuth Responses 探针；ChatGPT backend 请求体不得携带其不支持的 `max_output_tokens`。

`direct` 预设（`access_mode = None` 或 `Direct`）的行为完全不变。
