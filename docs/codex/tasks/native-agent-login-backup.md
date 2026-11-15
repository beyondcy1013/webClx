# LLM_API 本机智能体登录备份

## CodeArts 最高权限开关（2026-11-11）

LLM_API → CodeArts → 账号工具与设置 → 权限与自动化设置提供「最高权限」开关，默认开启。
首次进入页签时初始化；显式关闭会保留关闭状态。受保护的 `GET/PUT /api/auth/codearts-options`
使用终端用户的真实 HOME，PUT 参数为 `{"allow_all_permissions":true}`，GET 只读。

依据本机 CodeArts 26.11.1 原生权限服务，开启联动：

- `.codeartsdoer/cli-data/storage/permission/config.json` 的 `bash_mode=always_allow`。
- 同目录 `global.json` 已有规则 action 改为 allow，并补充通用 allow 规则。
- `.codeartsdoer/codearts_cli-extension.jsonc` 的 `bash.mode=always_allow`、`security.read.enabled=false`。
- `.codeartsdoer/cli-data/settings.json` 的 `security.folderTrust.enabled=false`。

开启前把受管字段保存到私有 `webclx-full-access.json`，关闭恢复这些字段；保留其他设置和新增规则。
重复开启不丢原始快照，不回放旧的整份规则数组。JSONC 支持注释、尾逗号和字符串 URL，保存为
合法 JSONC 的 JSON 格式；坏配置报错、不覆盖。全部源文件先校验再写，多文件写入中断可凭
pending 快照修复，不把部分成功显示为已完整应用。文件私有原子写入，不触碰登录凭据。

保存后重新启动原生 CodeArts 会话可完整生效；原生监视器只响应 change，原子替换不能保证
当前进程热更新。webClx 不结束运行中的任务。开关控制原生全局权限、敏感文件及目录信任，
不授予额外 OS/云端权限，不绕过账号额度、模型权益错误或自定义 Agent 独立规则。

编译请求 `011819-18e6bea0c1272392` 的 10 项 Rust 配置测试、Clippy 和 release 构建通过；
Node 开关行为 7/7、既有登录备份回归通过。320/390/768/1366 px 浏览器检查通过，权限区仅
CodeArts 可见、默认勾选，无全页横向溢出或 JS 错误。布局检查使用只读 options 夹具，真实
接口和原生配置写入须部署后单独验收。

部署 `012156-18e6bea0c1272393`（2026-11-11 01:23:58）后，运行/安装/审计 SHA-256
一致为 `6f75f0d6460e6f4e5edf99d45ed834e648064b9265d3968427da9575233987b6`。
真实 PUT 开启并重新读取返回 initialized/applied/allowAllPermissions 均 true，bashMode
为 always_allow；磁盘四个受管字段及 10 条全局 allow 规则核对通过，auth.json 摘要未变。
匿名 options 请求 401，两段修改 JS 的 HTTP 字节与源码一致。未结束或重启用户 CodeArts 会话，
不以配置保存宣称旧进程已热更新。

## 读取与下载

LLM_API 现有九个独立 Tab（Codex_OAuth、Codex_API、Claude_API、CodeBuddy、Devin、AGY、OpenCode、CodeArts、Qoder）各自提供“读取本机登录”和“备份当前登录（下载）”。账号库为空时也可使用，不依赖表格勾选。原有预设列表、账号保存和导入导出继续使用原接口。

`GET /api/auth/native-login/{kind}` 返回凭据是否存在、文件相对路径和说明；CodeArts 额外返回白名单账号摘要（provider、account_id、auth_type）用于账号表格，不返回 token 或完整凭据对象。`POST` 重新读取当时的原生文件，返回 `webclx-native-login` version 1 JSON 下载，文件内容以 Base64 无损保存。两接口均经过现有登录鉴权，响应 `Cache-Control: no-store`。备份不应用预设、不修改客户端文件、不改变当前登录、不保存到服务端账号库。下载包含完整凭据，Base64 不是加密；该格式暂不支持网页恢复或原账号导入器导入。

读取根目录来自设置页终端用户的 passwd profile HOME，不从工作区、历史会话或已保存的账号列表推断。只读取列举文件，每个文件最多 2 MiB；格式错误、读取失败、非普通文件均明确报错。

| Tab / kind | 原生文件（相对于真实 HOME） |
| --- | --- |
| Codex_OAuth / oauth | `.codex/auth.json`、`.codex/config.toml`；要求 OAuth access token |
| Codex_API / api | 同上；要求 `OPENAI_API_KEY` |
| Claude_API / claude | `.claude/.credentials.json`、`.claude/settings.json`；同时识别原生 Claude OAuth 和 API token |
| CodeBuddy / codebuddy | `.codebuddy/settings.json`；识别原生 `CODEBUDDY_AUTH_TOKEN` |
| Devin / devin | `.local/share/devin/credentials.toml`、`.config/devin/config.json`；要求 `windsurf_api_key`，只处理 CLI |
| AGY / agy | `.gemini/antigravity-cli/antigravity-oauth-token`、同目录 `settings.json` |
| OpenCode / opencode | `.local/share/opencode/auth.json`、`.config/opencode/opencode.json`；识别 OAuth 或 API 凭据 |
| CodeArts / codearts | `.codeartsdoer/cli-data/auth.json`、同目录 `settings.json`、`.codeartsdoer/codearts_cli.json` 与 `codearts_cli.jsonc`（存在时）；OAuth 要求非空 `access` 与 `refresh`，API 要求非空 `key` |
| Qoder / qoder | `.qoder/.auth/user` 与 `.qoder/.auth/machine_id`（国际版），或 `.qoder-cn/.auth/user` 与 `.qoder-cn/.auth/machine_id`（国内版）；`user` 与 `machine_id` 必须成对且非空，同时备份对应 `settings.json` |

本实现针对本机 Linux 文件登录态，不枚举系统其他用户、不读取系统密钥环或仅存在于进程环境中的凭据；XDG 自定义路径和 Windows/macOS 路径适配尚未实现。凭据存在只证明可备份，不证明 token 未过期或服务端请求成功；不伪称在线登录核验通过。Devin Desktop 的加密快照继续遵循独立专题，不把 CLI 备份用于 Desktop。

## 2026-11-10 CodeArts CLI

CodeArts 子页签地址为 `/codex_api/codearts`，使用独立的 `kind=codearts` 检测和备份接口，不从 OpenCode 的凭据目录借用账号。路径和 OAuth 字段以本机安装的原生 `codearts` 启动器及登录文件结构核对；只收录列举的登录与配置文件，不备份对话数据库、日志、安装器或工作区文件。备份下载沿用 version 1 格式，目前不提供账号库切换、网页恢复或在线额度查询。

终端进程识别支持原生 CodeArts 二进制和直接 bash/sh/node/bun 启动器，普通 `rg codearts`、`echo codearts` 不会被识别成智能体。默认智能体命令加入 `codearts` 与 `codearts --session {session_id}`；恢复命令提取识别 `--session`、`-s` 及等号参数。自定义智能体列表保持用户配置，可通过设置页追加 CodeArts 或恢复默认命令。

### 2026-11-11 CodeArts CLI 认证入口纠正

初版提交 `b47033ce` 将 `codearts auth login` 标作 OAuth 登录，依据仅为通用 `--help` 中的 provider/method 参数；这不足以证明当前发行版能够启动 OAuth。相同原生命令实际执行先返回“认证失败，没有设置环境变量 CODEARTS_CLI_AK/CODEARTS_CLI_SK”，并指向官方 CLI 认证设置，未进入 OAuth 提供商选择。这是入口能力判断错误，不能以已有 OAuth 格式文件或帮助参数冒充可用授权流程。

CodeArts 页签现改为“CLI 认证设置（AK/SK）”，直接在新页打开 `https://codearts.huaweicloud.com/portal/settings/cli-auth`，不再运行失败的命令。页面明确当前入口要求 AK/SK，尚未验证可用的 OAuth 登录流程；由用户遵循官方指引申请和自行配置环境变量，webClx 不注入凭据、环境或配置。现有读取与下载仍只备份列举的原生文件，不包含仅存在于环境变量中的 AK/SK，不把浏览器登录官网当作 CLI 已认证。

本次为静态资源修正，运行目录链接即时生效，无需重建或重启；模块缓存键更新为 `20261111-codearts-aksk`。Node 回归校验官方设置地址、入口按页签隔离、不启动终端、AK/SK 说明及备份限制；四宽度浏览器回归校验入口文案与新页导航（目标使用夹具），沿用真实本机读取与备份检查，不声称完成官方授权。

验证入口：`cargo test --locked --bin webclx codearts` 覆盖独立凭据、缺失字段、损坏 JSON、重新读取、大小限制和进程识别；`node tests/native-login-backup.test.mjs` 覆盖 CodeArts 下载与切页响应隔离；URL、智能体命令和恢复提取回归均扩展 CodeArts；`python3 tests/llm-account-mobile.browser.py` 覆盖八页签的四种宽度。

回归：`node tests/native-login-backup.test.mjs` 覆盖跨 Tab 响应隔离、备份时重新读取、失败不生成文件和空列表按钮；Rust `routes::native_login::tests::` 覆盖八类凭据正反例、API/OAuth 区分、损坏输入和非原生路径拒绝。部署与本机 HTTP 验收须使用本轮队列结果，不能用上述检测结果冒充原生请求验证。

### 2026-11-11 读取本机登录同步账号表格

修正首次实现仅显示“已检测到凭据”却不填充表格的数据链路：后端从同一次原生读取中提取有效 provider 的账号标识和登录类型，前端在读取完成后同步当前 CodeArts 行，显示“本机当前”“未在线核验”、provider、账号标识和读取时间。原生 API 凭据未提供账号标识时明确显示缺失，不截取密钥冒充账号。行内提供下载登录备份；本机行不加入已保存账号库，不提供虚假的切换、删除和通用导出动作。注销后重新读取或读取失败会清空旧行，过期的跨页签响应不得覆盖表格。

回归入口新增 `node tests/codearts-native-account-table.test.mjs`；原生备份回归验证摘要传递、过期响应隔离和失败清空；Rust 原生登录测试验证摘要严格白名单且无密钥；四宽度浏览器用真实读取响应核对账号表格行数、标识和行内备份按钮。

本轮部署请求 `001513-18e6ba9d9e3a0d2c` 验收通过：5 项原生登录 Rust 测试、release 构建、9 项终端滚动门禁及图标门禁通过。集中审计为 `/home/bin/webclx/compile/runs/20261111T001518-1467631-14259/install-report-1.json`；运行与安装文件 SHA-256 均为 `2a835c3874425073900f5b73ed9eeb4d67e8bd8d7eec97967555dcdcdb846b77`。真实 GET 返回 1 个本机账号摘要，标识与原生文件相符，字段严格限定为白名单；POST 字节校验及文件不变校验通过，未认证 GET 为 401。两处 JS 的 HTTP 字节与源码一致，缓存键更新为 `20261111-codearts-table`。

部署后 Node 回归及 320 / 390 / 768 / 1366 px 浏览器验收通过：点击读取后表格出现对应账号，列数与标识正确；点击行内备份发起真实 POST 并生成 CodeArts 下载文件名，测试拦截最终下载动作以免凭据落入测试产物。仍沿用源码 HTML 与运行服务资源/API 的测试边界，不声称完成网页 Cookie 登录流程。

### 2026-11-11 CodeArts 首次上线验收记录

部署请求 `235924-18e6909d4eb42aed` 完成：4 项原生登录回归、1 项 CodeArts 进程识别回归、release 构建、终端滚动门禁（5 项 Node 与 4 类浏览器）和工作区图标门禁通过。首次请求因测试引用未引入的 `tempfile` 而编译失败，已改为标准库临时目录及 RAII 清理，未新增依赖。安装审计位于 `/home/bin/webclx/compile/runs/20261110T235929-1222421-9009/install-report-1.json`；运行进程与安装文件 SHA-256 均为 `7cedd57f6ce97b0ce686b5ca731eddfff973ed9d733630322416b3bef99d8c1f`。

真实 CodeArts GET/POST 均 HTTP 200 且 `no-store`；检测响应仅含元数据，备份格式、版本、类别和 Base64 解码校验通过，与当前终端用户 HOME 下 3 个原生文件逐字节一致，操作前后文件摘要未变。未认证 GET 返回 401，OpenCode 独立检测仍 HTTP 200。未将备份正文写入测试产物或日志，也未把凭据存在解释为上游登录有效。

四个修改的 JS 经 `/assets/` 返回的字节与源码摘要一致；运行静态目录仍为指向源码的绝对链接，HTML 缓存键为 `20261110-codearts`。网页路由对仅持本机 API 令牌的请求仍显示登录页，浏览器检查继续使用源码 HTML 与真实运行资源/API，不声称完成 Cookie 登录流程验收。当前设置中的既有自定义智能体命令保持原值，仅追加 CodeArts；部署后重新读取确认该命令已持久化。

部署后重跑原生备份、八页签 URL、默认/自定义智能体命令、恢复提取和分 Tab 刷新 Node 回归均通过；320 / 390 / 768 / 1366 px 浏览器检查通过，CodeArts 读取与下载控件可用，八页签无全页横向溢出、无 JS 错误。

## 2026-10-25 本机上线验收

实现提交 `6766edf4`，部署请求 `024150-18e18b51e710dd56` 完成。两项 native-login Rust 回归、release 构建及部署门禁通过。运行进程与安装文件 SHA-256 均为 `212c32a366cc113040b972694e602c41cd86b100a1a8d995bd00b5dc488615a5`，前端模块 HTTP 字节与源码哈希一致；运行静态目录保持指向源码的绝对链接。

七类检测接口均 HTTP 200，带 `no-store`；本机当前 Codex API、Claude、CodeBuddy、Devin CLI、AGY、OpenCode 六类存在凭据，备份 POST 均 HTTP 200，下载结构、类别、文件集合及 Base64 解码通过。Codex OAuth 当前不存在凭据，备份按预期 HTTP 400；未知类型 HTTP 400，未认证检测 HTTP 401。连续备份前后九个现存原生文件的摘要一致，未把备份正文写入测试产物或日志。

Chrome 在 1366px / 390px 两种宽度逐个切换七个 Tab，读取状态、按钮可见及可用、零页面脚本错误通过。浏览器测试使用源码 HTML 夹具与真实运行 API，本机令牌仅供 API 鉴权；未绕过网页 Cookie 门禁，也不冒充真实浏览器登录流程验收。系统 Playwright 浏览器缓存缺失，改用已安装 `/opt/google/chrome/chrome`。

既有 `codex-account-unification.test.mjs` 的行菜单正则仍期待旧“切换到此账号”结构，与 HEAD 已存在的 Devin Desktop 分支和 CLI 文案不一致；本轮未改该段，不将此旧断言失败算成本轮通过。定向前端回归和 URL、分 Tab 刷新回归通过。本次只验收原生凭据读取与备份，不证明六类上游登录均未过期。

集中安装审计：`/home/bin/webclx/compile/runs/20261025T024155-131113-5298/install-report-1.json`。本次按共享主工作区构建，包含当时其他执行者的 Devin 额度倒计时改动；本任务提交未纳入该改动。

## 2026-10-25 移动端布局与当前页签批量额度

- 手机端七个 Agent 页签采用紧凑两行网格；账号操作、导入工具、权限设置、本机凭据详情和 OAuth 高级表默认折叠。原生登录读取与下载仍位于当前页签内。OAuth 旧表保留高级入口，不再与统一表同时展开；旧隐藏快捷按钮移除 `data-account-kinds`，避免页签同步重新显示重复按钮。
- OAuth / Codex API / Devin 新增「刷新全部额度」，无需勾选账号；CodeBuddy / AGY 原有批量刷新移到同一常用操作区。API 仅查询已有平台识别支持的预设，Devin 跳过无额度会话凭据的 Desktop 摘要。Claude / OpenCode 未提供统一额度查询，不显示该按钮。
- 新批量查询捕获点击时的页签，最多四路并发，阻止重复点击并汇总失败；切换页签不会扩大查询范围。Devin 严格加载模式向批量操作上抛账号列表读取错误，避免误报「零账号成功」。既有预设访问模式不变。
- 验证：`node tests/account-refresh-all-quotas.test.mjs` 覆盖范围、能力筛选、并发上限、重复点击及部分失败；原生备份、页签 URL、局部刷新和 Devin 额度回归通过。`python3 tests/llm-account-mobile.browser.py` 在 320 / 390 / 768 / 1366 px 检查七页签、额度按钮归属、工具区展开、无全页横向溢出及无 JS 错误。浏览器使用源码 HTML 和运行服务资源/API，未验证浏览器登录流程，也未发起真实批量额度请求。
- 静态文件经 `/home/bin/webclx/static` 绝对链接直接生效，HTML 中 CSS 和修改的 JS 均更新缓存版本；无需编译或重启。

### 首次加载 `strict is not defined` 修复

批量额度严格加载模式最初误把 `strict` 实参传进无同名参数的 `loadCodexAccountsOnce`，导致首次合并账号列表在读取 Devin 时抛出 ReferenceError。首次加载恢复为无参调用，仅专用 `fetchDevinAccounts(strict = false)` 传递严格模式。补充真实首次加载函数的 VM 行为回归，核对 Devin 账号成功进入统一列表且不报告警告；浏览器回归同时检查该错误不再出现。JS 缓存键更新为 `20261025-initial-load-fix`，静态修复即时生效。
