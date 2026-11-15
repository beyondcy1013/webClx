# Devin Desktop 登录备份与恢复

## 当前范围

CLI 使用小写 `~/.config/devin` 和 `~/.local/share/devin/credentials.toml`；Desktop 使用 `~/.config/Devin/User/globalStorage/state.vscdb`。两者必须作为不同客户端类型管理，同邮箱不能合并为同一条账号。

2026-10-20 静态检查当前安装的 Desktop 扩展确认：会话由 `context.secrets` 中的 `windsurf_auth.sessions` 保存，服务地址另有 `windsurf_auth.apiServerUrl`。工作台还保存 `windsurfAuthStatus`。不能把 CLI token 写入 Desktop 数据库，亦不能整库覆盖。

## 已实现工具

```bash
python3 scripts/devin/desktop-auth-backup.py backup /安全目录/desktop-login.gpg
python3 scripts/devin/desktop-auth-backup.py restore /安全目录/desktop-login.gpg
```

依赖 Python 3、SQLite 和 GnuPG；密码由 GPG pinentry 获取，不放入命令参数或环境变量。文件名必须不存在，输出权限为 0600。目录由调用者选择；备份含登录凭据，应妥善保管密码和文件。

工具仅处理明确列举的正式环境登录键，及 `codeium.windsurf` 中的登录邮箱和服务地址。恢复保留同一个 JSON 中的其他设置、聊天记录及其他扩展的 secret。缺失的登录键也被记录，恢复时删除后来新增的对应键。

恢复限于原 UID、原配置目录和相同 `Local State` 哈希，要求先正常退出当前用户的 Desktop；工具不关闭应用。写入前创建另一份加密登录快照，事务内检查登录字段未并发变化，再提交登录字段。需要回退时，对生成的 `.before-restore-*.gpg` 使用同一 restore 命令。

这是保守的原环境恢复限制，并不证明 Local State 哈希足以识别系统密钥环状态。系统密钥环已改变、服务端吊销 token、版本迁移均可能导致恢复后仍需重新登录。实际恢复后须启动 Desktop，核对账号、重启持久性并测试原生请求。

## 验证与后续接入

已用合成 SQLite 数据验证登录字段往返、缺失键恢复、保留聊天、其他 secret 和更新后的非登录设置；非法字段拒绝。尚未在真实 Desktop 上做 GPG 往返、替换登录态和原生请求验收，尚未接入网页菜单。

网页接入前需完成上述真实验收。账号 schema 已增加 `client_type: cli | desktop`，旧记录默认 CLI；现有新增、导入和切换接口仅允许 CLI，Desktop 和未知类型会明确报错。CLI 当前状态与去重已检查类型，避免 Desktop token 被当成 CLI 凭据。Desktop 类型目前仅用于协议边界，尚不提供 Desktop 记录创建和网页恢复。后续需实现 Desktop 独立快照存储、唯一 ID 和应用菜单。Desktop 菜单应分别提供备份、恢复、登录核验，不能把数据库写入成功显示成登录成功。

当前工具备份的是 Electron 原始加密 secret；跨机器迁移需要在源用户原生加密环境解密、目标环境重新加密，不能直接放开路径或 Local State 校验。暂不支持 staging 登录字段。

## 2026-10-20 类型边界验证

通过 webClx 编译队列执行 `cargo test --locked devin::tests::`，请求 `091356-18dff904dc371e7f`，3 项通过。覆盖旧记录默认 CLI、导出类型字段、混合批次包含 Desktop/未知类型时整体拒绝，以及 Desktop 新增/切换在写入 CLI 文件之前拒绝。Python 快照测试 2 项通过。该次仅验证源码，未部署后端；真实 GPG、Desktop 恢复和原生请求验收仍未完成。

## 2026-10-20 本机部署验收

部署请求 `092222-18dff904dc371e81` 已完成，执行 `cargo build --release --locked` 和项目部署脚本。运行进程与安装文件 SHA-256 均为 `9fa8d1778ad57fa6e04f1bc13c5d28fd2fcb85ad9b51ed099841c6908b37791a`，systemd 状态为 active/running。认证检查与 Devin 账号列表接口均返回 HTTP 200，现有 3 条账号均明确返回 `client_type: cli`。静态目录链接指向源码权威目录。集中审计位于 `/home/bin/webclx/compile/runs/20261020T092227-3975449-8249/install-report-1.json`。本次按当前工作区构建，包含当时已有的其他未提交改动；Desktop 网页备份恢复与真实登录验收仍未完成。

## 2026-10-20 快照类型与加密验证

修复 SQLite BLOB 登录值恢复时被强制转换为文本的问题。新快照通过可选 `blob_keys` 保留二进制类型，旧快照仍按文本格式兼容；非法类型、重复或未知 BLOB 键、无效 Base64 和无效 UTF-8 文本均在写入前拒绝。恢复前先解析当前设置对象，避免异常格式触发部分字段改写。

Python 测试现为 4 项通过，覆盖文本与二进制往返、旧快照兼容、损坏输入无写入及无关数据保留。另使用临时 GPG home 和内存中随机密码完成真实 GPG 合成数据加解密，验证输出 0600 和同名备份拒绝覆盖；临时 GPG agent 已清理。该测试经管道传递测试密码，不验证交互式 pinentry，也不替代真实 Desktop 登录恢复验收。本机只读检查发现当前 4 个登录字段均为 SQLite text，未输出字段内容。
## 2026-10-20 网页类型菜单上线验收

提交 `4b0ab67`（Desktop 只读探测与类型菜单）、`6badf34`（元数据序列化补全）、`9a57a26`（前端缓存版本）随干净候选 `62f9f8f` 的部署请求 `222400-18e042e1659cfed5` 上线，安装后二进制 SHA-256 为 `64a40e7014e13e2154ff4cae2e96b7698e0d79dcfe8704bd821f2a90f97f7141`。

运行态核对：

- `POST /api/auth/devin-accounts/detect-desktop` 返回 `ok:true`、`client_type: desktop`、`logged_in:true`，profile 为本机 `.config/Devin`；只返回账号元数据，不含 secret。
- `GET /api/auth/devin-accounts` 三条记录均为 `client_type: cli`，未出现 Desktop 凭据。
- 页面缓存版本为 `20261020-devin-desktop`、`20261020-desktop`，静态目录仍指向源码权威目录。
- `node tests/devin-account-manager.test.mjs` 与两个前端脚本的 `node --check` 通过。

网页菜单对 Desktop 只做只读核验和类型标注；备份/恢复仍只提示终端命令，不在网页执行磁盘写入。

仍未完成：真实 GPG 往返、真实 Desktop 登录态替换与原生请求验收。Desktop 当前正在运行，恢复必须等用户正常退出后进行，因此该项需用户确认时间窗口。

另发现账号库存在同一 `id` 重复记录两条（`devin-buechesteeg435-outlook-com`，一条 active、一条 inactive），当前按 id 删除会同时命中两条，属待确认的数据去重问题。

## 2026-10-20 重复账号记录收敛

现象：`/home/bin/webclx/webclx-devin-accounts.json` 中同一 `id` 存在两条 CLI 记录（同一邮箱、不同 session token，一条 active、一条 inactive），按 id 删除会同时命中两条。

根因：账号 id 只由邮箱派生；启动时若当前 `~/.local/share/devin/credentials.toml` 的 token 不在账号库中，`DevinManager::load` 直接 push 新记录，于是同一 id 出现第二条。

修复（`d74d4bc`）：

- 加载时先 `dedupe_accounts` 按 `(client_type, id)` 收敛，优先保留与当前登录一致的记录，其次保留 `saved_at` 较新的记录。
- 当前登录凭据登记改为 `upsert_current_login`：同一 id 或同一 token 已存在时原位更新，不再追加。
- 新增 4 项测试（去重两种取舍、原位更新、加载后持久化收敛），编译队列请求 `223041-18e04336cf3158de` 中 8 项 devin 测试全部通过。

存量数据处理：手工收敛 3 条为 2 条，删除的是同邮箱下已被替换的旧 token 记录；原文件备份为 `/home/bin/webclx/backups/devin-accounts-before-dedupe-20261020-223206.json`（0600）。重启服务后接口与磁盘均为 2 条唯一 id，切换后仍为 2 条，`devin auth status` 仍为已登录。

运行中的二进制尚未包含该修复（工作区有其它会话未提交改动，未部署）；部署后新产生的同 id 重复会在加载时自动收敛。

## 2026-10-20 导入新旧判定

导入项携带 `saved_at`（兼容 `savedAt`、`updated_at`）时以来源时间为准，缺失才用本次导入时间。此前 `parse_devin_account_value` 把 `saved_at` 硬编码为 `now_ms()`，所有导入项都会被当成最新，现已改为读取来源值。

命中同 id 或同 token 时：

- 导入时间 ≥ 现有记录：直接覆盖，计入 `updated`，不提示；
- 导入时间 < 现有记录：保留现有记录，不计入 `updated`，返回 `skipped` 明细（`id`、`email`、`incoming_saved_at`、`existing_saved_at`）；
- 两者都无记录：追加，计入 `imported`；
- 无时间戳的导入项按最新处理，等同实时导入的新凭据。

剪贴板导入接口把 `skipped` 转成 `warnings` 文本（“导入凭据早于现有记录，已保留现有账号”），直连 `/api/auth/devin-accounts/import` 的调用方直接读 `skipped`。

测试新增 3 项（新覆盖旧、旧跳过并报告、无时间戳按最新），devin 测试共 11 项全部通过（编译队列 `223740-18e043617f527f44`）。提交 `b4a7677`，尚未部署。

## 2026-10-20 去重与导入策略上线

部署请求 `224536-18e043617f527f49` 以干净候选 `ce97f92` 发布（工作区其他会话改动先整体保留、发布后原样恢复，指纹一致）。运行二进制 SHA-256 `d192c258e0272ebf3b33a460d107985eba9fab5131d7af92d20da36b4808a221`，服务 active，`/api/auth/session` 返回 200。

运行态验收：

- `GET /api/auth/devin-accounts` 返回 2 条，id 唯一，active 仍为 `devin-buechesteeg435-outlook-com`。
- `POST /api/auth/devin-accounts/import` 提交同邮箱、`saved_at: 1` 的探测凭据：返回 `updated: 0`，`skipped` 含 `incoming_saved_at: 1` 与 `existing_saved_at: 1790266340603`，导入后账号数仍为 2，现有 token 未被覆盖。
- 新凭据覆盖分支由单元测试覆盖，未在线上改写真实凭据。

遗留：真实 Desktop GPG 备份/恢复与原生登录验收仍需退出 Desktop 后进行；另一个会话的批量导入弹窗（未跟踪文件）仍按 `imported_count == 账号数` 判定成功，遇到跳过项会提示“接收数量不一致”。

## 2026-10-25 套餐内外额度

LLM_API 的 Devin 列表增加「套餐内剩余」「套餐外剩余」两列，加载时查询，单列可手动刷新。新增只读接口 `GET /api/auth/devin-accounts/{account_id}/quota`：按账号库中该账号的 token 与 org_id 请求 `https://app.devin.ai/api/{orgId}/billing/quota/usage`，使用 Bearer 和 `x-cog-org-id`，不切换当前登录、不把 token 返回浏览器。参考实现为 `/home/codes/plugins/ppdevinRefactory/out/switch/quota.js`。

真实响应包含 `daily_percentage`、`weekly_percentage`、重置时间和 `overage_balance`。官方网页在已用百分比达到 100 时判为额度耗尽，因此套餐内显示 `max(0, 100 - percentage)` 的日/周剩余比例；`hide_daily_quota` 为 true 时不显示日窗口。非额度套餐、未分配额度、字段缺失分别明确提示，不把缺失字段当作零。套餐外显示 `overage_balance` 原始单位、保留负值，两位小数；不把套餐内百分比与套餐外余额相加。

浏览器按账号缓存成功响应 60 秒，并合并同账号并发请求；查询失败明确提示，不显示旧余额冒充新结果。Desktop 仅含登录摘要，显示不支持查询。测试涵盖零余额、负余额、日周剩余计算、隐藏日窗口、缓存、手动刷新、失败和 Desktop 不发送请求。后端验证与部署结果以本次编译队列回调为准。

本次验证与上线：提交 `058fcd5`；后端队列 `023202-18e105f98e96e25a` 的 12 项 Devin 测试通过，Node 行为测试与 Chromium 隔离双列渲染通过。部署 `023250-18e105f98e96e25b` 成功，运行/安装二进制 SHA-256 同为 `d60818c0e783931101732476179c38f101a188a07ed2997efd878f4fbbe4afd0`；认证接口 HTTP 200，HTTP 静态 `index.html` 含两个表头，额度模块缓存版本 `20261025-quota` 已生效。真实账号查询一条 HTTP 200（日已用 0%、周已用 83%、套餐外余额 -0.045780214），对应显示日剩余 100%、周剩余 17%、套餐外 -0.05；另一条上游返回 HTTP 404，按查询失败展示，不伪造余额。根路径未带浏览器登录 Cookie 会返回登录页，不能用它替代已登录列表的页面验收。持久验收提醒已取消。安装审计：`/home/bin/webclx/compile/runs/20261025T023255-5558-19523/install-report-1.json`。

### CLI 团队标识与网页额度组织（2026-10-25）

`ozy9zne9@icloudev.com` 查询返回 `Organization not found`，根因是原生 CLI `Team ID` 为 `devin-team$account-…`，而网页 billing 接口需要 `org-…`。不能把该 CLI 团队标识直接拼入网页额度 URL，也不能改写 CLI 配置迎合网页接口。

查询时对 `devin-team$` 前缀，通过同一 token 的 `GET /api/organizations`，严格按 `account_id` 匹配对应组织，并将唯一匹配的 `org_id` 同时用于额度路径和 `x-cog-org-id`。无匹配或多匹配明确失败，不猜测首个组织、不修改账号库。已有网页 `org_id` 直接查询。真实只读复验已成功取得日已用 17%、周已用 96%、套餐外余额 2.753169283951077；显示应为日剩余 83%、周剩余 4%、套餐外 2.75。新增回归覆盖精确匹配、无匹配、多匹配与非法响应；运行服务修复仍以本轮测试、部署回调为准。

CLI 组织映射修复 `68b8169` 已上线：13 项后端测试通过；部署 `023750-18e18b1011833e81` 成功，运行/安装 SHA-256 同为 `7419c6ba8fb6897b334e957a0cebcd79dda4230cabba0d03adccb195d5aa7da6`，服务 active，认证 HTTP 200。线上指定账号额度接口 HTTP 200，日剩余 83%、周剩余 4%、套餐外 2.75，账号库仍保留原 CLI 团队标识。提醒 `paste-1792867050195-12187` 已清理。

### 重设时间与倒计时

套餐内日、周窗口下方分别显示距重设剩余时间与具体重设时间，按浏览器本地时区展示并标注时区。使用接口 ISO 时间中的时区解析绝对时刻，每 30 秒更新倒计时；已到期提示刷新额度，缺失或非法时间显示未知，不猜测下一周期。不为套餐外余额编造重设时间。成功查询和失败重试时清理旧时间明细；全局单一定时器仅扫描连接到文档的行，表格重绘不保留旧节点。静态模块缓存版本 `20261025-quota-reset`；Node 测试与 Chromium 时钟推进验证覆盖日周时间、时区、到期、隐藏日窗口和倒计时自动变化。

### 套餐到期与续费时间（2026-10-25）

`POST /api/auth/devin-accounts/{account_id}/subscription/refresh` 使用该账号的 token 和精确映射后的 `x-cog-org-id`，只读请求 Devin `GET /api/billing/subscription`。官方网页 billing actions 使用 Unix 秒字段 `current_period_end` 显示续费时间，使用 `cancel_at` 显示套餐结束；`end_of_cycle_slug` 与当前套餐不同时表示到期变更套餐。它们与日周 quota reset 是不同时间，不能互相推算。

仅白名单持久化 `subscription`（slug、status、current_period_end、cancel_at、end_of_cycle_slug、queried_at）；不修改 CLI 登录、组织配置、账号凭据时间 `saved_at` 或切换记录。网络请求不持有账号锁，保存前核对账号 token/org 未变，原子写入成功后更新内存。上游失败或非法日期不覆盖上次结果；成功响应的空日期会清除旧日期。

Devin 列表复用「到期」列自动查询，并支持手动刷新、60 秒成功缓存和同账号请求合并；明确展示续费、套餐结束、套餐变更或「接口未提供套餐到期时间」。日期按浏览器本地时区显示并标注时区与剩余天数；失败时明确标注上次成功结果。Desktop 摘要不发送查询。

实测指定 `ozy9zne9@icloudev.com` 订阅接口 HTTP 200、pro/active，`current_period_end=1793784543`，即北京时间 2026-11-04 17:29:03；`cancel_at=null`，应标为续费时间。另一现有账号 pro/canceled 且日期均为空，不编造到期日。16 项后端 Devin 测试通过（队列 `135210-18e1adb15d2e2c9f`）；Node 行为测试与 Chromium 本地时区/取消后清除日期验证通过。部署 `135413-18e1adb15d2e2ca1` 已成功，运行/安装二进制 SHA-256 同为 `b415652d86136742e708601eea224bed101f4ed7beda6d5cda7f261eb6b5c5b5`；服务 active/running，认证 HTTP 200。线上两账号 subscription/refresh 均 HTTP 200，分别返回上述 active/续费日期与 canceled/空日期；列表接口与磁盘的 subscription 与查询结果一致，token、CLI org、saved_at、last_switch_at、active 均保持原值。HTTP 两个 JS 模块及 index.html 哈希与源码一致，缓存版本 `20261025-devin-subscription` 生效。部署前暂存改动已完整恢复；安装审计 `/home/bin/webclx/compile/runs/20261025T135418-4059978-15425/install-report-1.json`。
