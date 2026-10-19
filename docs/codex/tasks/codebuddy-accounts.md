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
`GET/PUT /api/auth/codebuddy-auto`（`checkin` / `travel`，响应含各任务上次运行时间与账号数）；
"刷新全部积分 / 立即全部签到 / 立即全部旅行"由前端逐个账号调用单账号接口，最后汇总成功数。

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
