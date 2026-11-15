# CodeBuddy 账号类型（LLM_API）

## 范围

LLM_API 页的"统一账号"列表新增 `CodeBuddy` 账号类型：添加账号（OAuth 扫码 / 手动 token）、
切换 CodeBuddy CLI 账号、自动签到、自动旅行（派猫猫）、积分查询。接口与状态机对齐
[changexbc/workbuddy-switch](https://github.com/changexbc/workbuddy-switch)（MIT）的
`wb-switch-core` 实现，不引入该仓库依赖，仅按其协议语义重新实现。

## 模块与文件

- `src/codebuddy.rs`：账号库、OAuth、token 刷新/保活、签到、旅行、积分、CLI 切换、自动循环。
- `src/routes/codebuddy.rs`：REST 路由。
- `src/main.rs`：`AppState.codebuddy_manager` + `spawn_auto_runner()`。
- 前端：`static/app-codebuddy-manager.js`（新增）；`static/app-codex-account-manager.js`
  （ACCOUNT_KINDS 增加 `codebuddy`、行操作 签到/旅行/积分、积分三列与自动任务栏联动）；
  `static/index.html`（`codex-account-add-codebuddy` 按钮、`codebuddy-add-dialog`、
  `codebuddy-auto-bar`、统一账号表的 总积分/分类积分/到期 三列）。

## 数据与凭据边界

- 账号库：`<app_dir>/webclx-codebuddy-accounts.json`；状态：`<app_dir>/webclx-codebuddy-state.json`
  （active_account_id、auto 开关、签到日志、旅行缓存、积分快照）。
- 切换 CLI 只写真实共享配置，遵守"无终端注入"规则：Unix 与 Windows 统一写
  `~/.codebuddy/settings.json` 的 `env.CODEBUDDY_AUTH_TOKEN`。
  - 发现**其他** apiKeyHelper（路径不含 `.codebuddy-rotate`）时拒绝并提示；
    本家历史 helper 或残留 `apiKeyHelper` 键在切换时移除。
  - 活动账号 token 保活刷新成功后同步 settings.json：仅当 `env.CODEBUDDY_AUTH_TOKEN`
    仍等于刷新前的旧 token 时才改写为新 token，并顺带移除本家 `apiKeyHelper`；
    第三方 helper 不动，IO/JSON 错误只记 warn。
  - 历史方案（已弃用）：Unix 曾用 `~/.codebuddy-rotate/webclx-helper.cjs` 作为
    `apiKeyHelper`，helper 必须读状态文件的 `active_account_id`（snake_case，
    serde 默认命名），再按账号 `active === true` 兜底；早期版本误读 `activeAccountId`
    导致恒回落 `accounts[0]`，切换实际不生效。遗留 `~/.codebuddy-rotate` 目录不再写入，
    由部署后的下一次切换自然清理 settings.json 里的 helper 引用。
- HOME 来自设置页终端用户 profile（`terminal_user_profile().home`），不写死 `/root`。

## 官方接口（www.codebuddy.cn）

- OAuth：`POST /v2/plugin/auth/state?platform=workbuddy` → 轮询 `GET /v2/plugin/auth/token?state=` →
  `GET /v2/plugin/login/account`（Bearer + X-Domain）取资料。
- 刷新：`POST /v2/plugin/auth/token/refresh`，带 `X-Refresh-Token` 头；刷新失败标记 `needs_relogin`。
- 签到：`POST /v2/billing/meter/checkin-activity-status`（回退 `checkin-status`）、
  `POST /v2/billing/meter/daily-checkin`；"已签到"文案按成功处理。
- 旅行：`/activity/growth/buddy/travel/{config,status,depart,claim}`，额外头
  `x-client-platform: web`、origin、referer。状态机 `idle→traveling→arrived→claim`；
  `daily_limit_reached` 即当日完成；depart 文案 `already traveling` / `daily limit` /
  `no active buddy` / `location not available` 需分类，`no-buddy` 属可恢复状态，
  不得标记当日完成（否则后续轮次不再重试）。claim 带 status 返回的 `record_id`。
- 积分：套餐页三路接口
  `POST /billing/meter/get-user-resource-{summary,paid-packages,free-packages}`
  （`x-client-platform: web` + origin/referer `/profile/plans-usage`），
  三路都不可用时回退旧接口 `POST /v2/billing/meter/get-user-resource`
  （body 含 `ProductCode=p_tcaca`、`Status:[0,3]`、时间范围）。
  - 明细数组名随接口不同：`Packages`（summary）/ `Accounts`（paid、free、旧接口）。
  - 展示名按商品码映射官方中文名（码表见 `src/codebuddy.rs` 的 `CREDIT_PACKAGE_NAMES`），
    未登记码回落 `PackageName`，再回落商品码。
  - 分类 `category`：`paid` / `free` / `other`（码表 `PAID_/FREE_PACKAGE_CODES`）。
  - 到期：优先 `DeductionEndTime`/`ExpiredTime`；比 `CycleEndTime` 晚超过 365 天时视为
    长期占位并改用 `CycleEndTime`；距现在超过 730 天视为长期有效（不下发 `expireAt`）。
  - 摘要字段：`totalRemaining` / `totalCapacity` / `expiringSoonRemaining` /
    `expiredRemaining` / `soonestExpireAt` / `expiringSoon` / `expired` /
    `groups`（按 paid/free/other 汇总）/ `resources`（逐包明细）。
- 401/403 或文案含 登录/失效/过期/token 时刷新一次并重试；三路并发只刷新一次，
  仅重试未授权的那一路，避免三次刷新互相覆盖刚落盘的 token。

## 统一账号表列（LLM_API / CodeBuddy 页签）

- 总积分：`剩余 / 总量`，临期染黄、已过期染红。
- 分类积分：按资源包名（官方中文名）聚合剩余积分，最多展示 3 类，完整明细放 `title`。
- 到期：最早到期日期 + 剩余天数；无到期时间显示"长期有效"。
- 非 CodeBuddy 类型的三列显示 `-`；表头与空态行 `colSpan` 同步为 14。

## 自动循环

`spawn_auto_runner` 每 5 分钟一轮：
- 自动签到（默认开，间隔 ≥30 分钟）：逐账号查状态 → 未签到才提交 → 写日志（保留 200 条）。
- 自动旅行（默认开，间隔 ≥15 分钟）：arrived 领奖 / traveling 等待 / idle 派发 / daily_limit 完成。
- 积分刷新（间隔 ≥30 分钟）：逐账号刷新快照，避免列表里的总量/分类/到期长期陈旧。
- 每日保活：无条件刷新所有带 refresh_token 的账号（闲置 refresh 会话会被服务端清理）。

开关与批量执行在 CodeBuddy 页签的 `#codebuddy-auto-bar`（仅在选中 CodeBuddy 类型时显示）：
`GET/PUT /api/auth/codebuddy-auto`（`checkin` / `travel` / `blockInternationalAutoSwitch`，响应含各任务上次运行时间与账号数）；
"刷新全部积分 / 立即全部签到 / 立即全部旅行"由前端逐个账号调用单账号接口，最后汇总成功数。

### 自动换号的国际版开关（2026-10-29）

CodeBuddy Tab →「权限与自动化设置」新增「禁止自动切换到国际版账号」，默认勾选。
字段 `auto.blockInternationalAutoSwitch` 存在 CodeBuddy 状态文件中；老状态文件缺少
该字段时默认 `true`。保存开关立即作用于服务端额度恢复候选，勾选时只选国内账号，
无可用国内候选则保留原终端并明确提示，不回退国际版。取消勾选后恢复国内/国际统一
按最近切换时间排序。该开关不影响手动指定国际账号、账号列表、签到/旅行或 AGY。

`PUT /api/auth/codebuddy-auto` 接收可选字段；省略的字段保留原值，兼容旧客户端只保存
签到/旅行的请求。候选查询结束、发送 Ctrl+C 前及应用账号前均检查最新开关，防止
查询期间重新启用禁用策略后仍切到已选国际账号。回归入口：Rust `account_rotation`、
`auto_config_blocks_international`，Node `tests/codebuddy-auto-switch.test.mjs`。

## 新开对话框账号排序（全部 Agent）

- "新开"对话框（`terminal-specified-task-dialog`）对 Codex、Claude、CodeBuddy、Devin、AGY
  统一处理：当前使用中的账号（Codex/Claude/Devin 的 `active`、AGY 的 `current`）始终排在
  最前并默认选中；其余账号按设置项 `terminal_account_sort` 排序：默认 `credits`
  （积分/额度高→低，无数据回落到最近切换时间），可选 `recent_switch`、`switch_count`、
  `recent_use`（本浏览器最近使用）、`name`。设置页入口为 `#terminal-account-sort-input`，
  旧字段名 `terminal_codebuddy_account_sort` 仅在读取旧配置时作为回退。
- 每个选项显示账号名、模型（有则显示）、`积分 <totalRemaining>`（AGY 为 `额度 N%`）、
  `上次切换 HH:MM（x 小时前）`；没有切换记录的老账号显示"切换时间未记录"。下拉框下方
  注记显示当前时间、快照更新时间与当前排序方式。AGY 的额度取 `quota_cache` 中各 bucket
  的最小 `remainingFraction`（百分比）。
- 切换时间写入账号/预设档案的 `last_switch_at`（毫秒）：CodeBuddy 走
  `switch_account`，Codex/Claude 预设走 `auth_core::bump_switch_count`（apply 时），
  Devin 走 `switch_account`，AGY 走 `apply`。旧档案没有该字段时为 `None`。
- Codex/Claude 的收藏（`favorite`）只作为同序时的稳定兜底，不再覆盖用户选择的排序方式。

## API 一览

```
GET    /api/auth/codebuddy-accounts                       # 列表（meta + credits 快照）
POST   /api/auth/codebuddy-accounts                       # 手动添加 {access_token, refresh_token, name}
DELETE /api/auth/codebuddy-accounts/{id}
PUT    /api/auth/codebuddy-accounts/{id}/switch           # 切换 CLI 账号
POST   /api/auth/codebuddy-accounts/{id}/checkin
POST   /api/auth/codebuddy-accounts/{id}/travel
GET    /api/auth/codebuddy-accounts/{id}/credits          # 现查 + 落快照
POST   /api/auth/codebuddy-accounts/oauth/start|poll      # {login_id}
GET/PUT /api/auth/codebuddy-auto                          # {checkin, travel} 开关
```

## 边界

- 未来任务（webclx run）不支持 CodeBuddy；对话框选 CodeBuddy 时定时模式提交会提示仅支持 Codex/Claude。
- 切换不触碰正在运行的会话；env.CODEBUDDY_AUTH_TOKEN 只影响后续启动的 CLI 进程。
- OAuth state 只存内存，重启后需重新发起登录。

## 设备指纹隔离与 401 鉴权拦截陷阱

### 1. 设备识别与隔离原理
- **硬件指纹来源**：CodeBuddy CLI（`@tencent-ai/codebuddy-code`）通过 `linuxUuid()` 读取 `/var/lib/dbus/machine-id` 作为物理机硬件标识，并在 `chat_request_send` telemetry 及风控上报中携带 `machineId`。
- **虚拟指纹覆盖**：源码内置环境变量优先检查：`process.env.CLIENT_INFO_MACHINE_ID`。只要在 `~/.codebuddy/settings.json` 的 `env` 节点或终端 shell 中导出合法的 RFC 4122 UUID，客户端直接使用该虚拟 ID 覆盖底层硬件读取。
- **账号级绑定**：webClx 采用确定性哈希为每个账号生成专属稳定虚拟 UUID，切号时自动写入 `~/.codebuddy/settings.json` 及 `~/.bashrc`；设置项在前端「LLM_API -> CodeBuddy (WorkBuddy) -> 权限与自动化设置」中的「设备指纹隔离」，默认开启。代码入口：`src/codebuddy.rs`（`account_machine_id`、`install_cli_credentials`、`sync_fingerprint_bashrc`）、`static/app-codebuddy-manager.js`。

### 2. 401 Unauthorized 与 token-length:0 陷阱（严重教训）
- **故障现象**：在 CLI 中执行 prompt 时直接报 `401 Authentication required ... (auth-type:custom-token,token-type:Bearer,token-length:0)`。
- **源码根因**：**严禁设置环境变量 `CODEBUDDY_SKIP_INTERNAL_HEADERS="1"`！** 客户端在 `ModelProvider` 检测到该变量后，会在发往模型的请求头上强行打上内部标记 `X-Skip-Auth-Interceptor: 1`。全局 Axios 鉴权拦截器遇到该标记时直接跳过注入 `Authorization: Bearer <token>` 请求头，导致模型请求裸奔发给腾讯云端网关被拒，客户端错误格式化时测得 token 长度为 0。
- **修复规范**：设备指纹隔离**仅需 `CLIENT_INFO_MACHINE_ID`**。切号与选项保存时必须主动清理遗留的 `CODEBUDDY_SKIP_INTERNAL_HEADERS`。

### 3. 429 频次超限误区
- 换号后若在同一历史终端或同一会话中继续提问仍报 429，绝大多数并非设备指纹问题，而是原会话历史锁定了特定限流模型（如 `--model kimi-k3.1-pro`）。恢复会话时指定切换到正常可用模型（如 `deepseek-v4.1-flash`），或等待频次窗口刷新即可恢复。

## 国际版（codebuddy.ai）切换与认证陷阱（2026-10-31）

### 1. ProductEnviroment 枚举值陷阱（"external" 而非 "public"）
- **现象**：切换到国际版后，CLI 启动告警 `Failed to load product config for environment="public" from "product.public.json"`，请求路由错乱。
- **根因**：CodeBuddy CLI 官方源码的 `ProductEnviroment` 枚举值仅有：`internal`、`iOA`、`cloudhosted`、`external`、`selfhosted`，不存在 `public`。切换国际版必须将 `CODEBUDDY_INTERNET_ENVIRONMENT` 设置为 `external`。

### 2. 原生认证凭据同步与 403 request illegal
- **现象**：设置了国际版 Token 后，CLI 运行时报错 `403 request illegal (01a1.../01a1...)`。
- **根因**：CLI 启动时通过 `resolveEnvFromSessionFileSync()` 读取权威原生凭据文件 `~/.local/share/CodeBuddyExtension/Data/Public/auth/Tencent-Cloud.coding-copilot.info`，并在发送模型请求时使用其中的 `account.uid` 组装 `X-User-Id` 请求头。如果切号时只更新 `~/.codebuddy/settings.json` 而未同步更新该文件，会导致发出的请求头上附带了旧账号（如国内版）的 `X-User-Id`，与当前 JWT `sub` 不符，直接被腾讯云 API 拦截判为 `403 request illegal`。
- **域名约束**：该原生文件的 `auth.domain` 必须为 `"www.codebuddy.ai"`（官方 `product.json` 的 `externalDomain` 仅登记了 `www.codebuddy.ai`，不含 `www.workbuddy.ai`；若写 `workbuddy.ai` 会被误判为 `Selfhosted` 私有化环境）。

### 3. 版本化 Product-Cache 残留导致 401 Unauthorized
- **现象**：从国内版切到国际版后，CLI 依然请求 `https://copilot.tencent.com` 并报 401。
- **根因**：CLI `ProductManager.getCacheKey()` 在新版本中带有版本后缀（如 `CodeBuddy-Product-Cache:2.179.0`，MD5: `3aedeedf0d092a51274ce4ea969ce8ec`），原代码仅删除了旧版无版本号的 `entry_604f48c944053e01d9546675443286c1.info`。切号时必须全量扫描清理 `~/.codebuddy/local_storage/` 下的所有带版本号产品缓存及 gzipped 内容。

### 4. 严禁对官方国际版注入 CODEBUDDY_BASE_URL
- **根因**：在 `settings.json` 中配置 `CODEBUDDY_BASE_URL="https://www.codebuddy.ai/v2"` 会导致 CLI 内部将 `tg` 标记为真，从而关闭官方协议封装与 failoverOrigin pin，引发兼容错误。官方 CLI 在 `external` 环境下通过 `product.json` 自动解析出正确的 `https://www.codebuddy.ai/v2`，无需显式设置 `CODEBUDDY_BASE_URL`。

## 积分快照与单模型频率限制（2026-11-02）

### 1. 列表积分与查询时机
- **快照缓存**：账号列表与卡片显示的是服务端定时轮询的快照缓存（`src/codebuddy.rs` 每 30 分钟轮询落盘，终端换号时对候选账号实时查询 `credits_account(id)` 并落盘）。打开或切换到账号管理页面时只读取本地缓存，不全量并发请求腾讯接口，以避免页面阻塞与防风控（401 / 429）。
- **手动刷新**：支持点击顶部「刷新全部积分」或单个账号「查积分」执行现场查询。

### 2. 通用积分与单模型频率限制（Rate Limit / 429）
- **通用积分（Tokens）**：腾讯计费接口（`/v2/billing/meter/get-user-resource`）只统计通用算力包与赠送积分扣减，官方不提供单模型当前调用频次或剩余配额的前置查询 API。
- **单模型频率超限**：热门模型（如 `kimi-k3.1-pro`）拥有独立的小时/日频次上限。即便账号积分充足，高频调用时服务端在推理时仍会拦截并返回 `429 您的使用量已超出频率限制，将在 ... 重置`。
- **自动轮换治理**：`src/terminal/manager/account_rotation.rs` 捕获 429 并解析重置时间，自动轮换国内候选账号；受限于 `auto.blockInternationalAutoSwitch`（默认不轮换到国际版）。若所有国内账号均耗尽，终端需等待限频窗口刷新或手动切换至低倍率/宽松模型（如 `deepseek-v4.1-flash`）。

