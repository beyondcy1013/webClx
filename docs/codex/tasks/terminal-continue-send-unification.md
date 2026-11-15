# Terminal Continue Send Unification

Date: 2026-07-05

## Implemented

2026-07-05: `继续` submission now has a canonical backend path.

Stable conclusions:

- Backend canonical function: `TerminalManager::send_session_continue`.
- Public endpoint: `POST /api/terminal/sessions/{session_id}/continue`.
- Submission semantics intentionally match the slash soft-keyboard `继续`: write `继续`, wait briefly, then send a standalone Enter.
- Manual `继续`, `send_session_message` with `submit_enters=1`, and server-side scheduled paste/input tasks whose text is exactly `继续` call the canonical unthrottled backend sender.
- Automatic browser retries, backend immediate retries, reset-time due tasks, and cron scripts use the cooldown-aware automatic sender. The public automatic endpoint is `POST /api/terminal/sessions/{session_id}/auto-continue`; it validates the current error and applies the configured per-terminal cooldown before calling the canonical command writer.
- Reset-time cron scripts call the local `/auto-continue` endpoint instead of duplicating `tmux send-keys` semantics. The generated script uses `WEBCLX_BASE_URL` when set, otherwise `http://127.0.0.1:${WEBCLX_PORT:-11111}`.
- Persisted reset-time tasks refresh missing or old cron scripts when the scheduler sees the same task again, so tasks created before this change are upgraded to the `/continue` endpoint template.
- Reinstalling a reset-time cron entry removes both old command marker lines and old `webclx-auto-continue-due` comments for the same session.
- The auto-continue runner also checks already-loaded persisted tasks for stale cron scripts/comments, because an existing task may not flow through new schedule collection again.
- Frontend auto-continue calls `/auto-continue`; manual soft-keyboard `继续` calls `/continue`, and generic `/input` remains available for raw terminal input.
- `GET /api/terminal/sessions/{session_id}/input-history` filters standalone `继续` entries after selecting the rollout or keystroke-history source. This keeps automatic and manual continuation turns out of “对话史” without removing normal prompts such as `继续处理这个问题`.
- The keystroke-reconstruction fallback (`capture_terminal_input_history`) is bracketed-paste aware: a paste block (one paste / one scheduled multi-line delivery) stays a single “对话史” entry with its internal newlines preserved, instead of being split into per-line segments at each `\r`/`\n`. Plain Enters outside paste still commit line by line. The rollout source already records one entry per real user message, so multi-line prompts remain one entry in both sources.
- Terminal-page scheduled-task chips must aggregate both `/api/terminal/scheduled-inputs` and `/api/terminal/auto-continue-tasks`. The chip text is `定时 当前终端数/全部任务数`; newly detected auto-continue tasks should also emit a toast so server-side detection is visible even when the paste dialog did not create the task.
- The terminal `定时` action opens the existing settings task manager in the named `webclx-scheduled-tasks` popup with `compact=tasks`; it must not replace the terminal page or fork a second task-management implementation. URL synchronization must preserve compact mode so refreshing or reopening the named window stays focused on the shared task form and list.
- Regression coverage lives in `src/terminal/tests.rs`, `tests/terminal-error-status.test.mjs`, and `tests/terminal-session-details.test.mjs`.

## Working-status guard (2026-10-18)

Automatic retries must recheck native working status when matching the current
pane snapshot, including direct `/auto-continue` calls and reset-time tasks.
Previously only the activity-list probe applied this guard; an automatic caller
could still match old error text or a keyword in normal tool output during an
active turn. The shared snapshot matcher now uses the same last-10-readable-line
working detector as the list before looking for error keywords. Manual continue
remains available. Regression: `terminal_error_snapshot_does_not_retry_while_agent_is_working`.

Investigation of `feishuFwd_1` found an active Codex `Working (... esc to interrupt)`
status and broad configured keywords (`sending request for url` and a bare request
URL). No retained trigger snapshot establishes which keyword caused the reported
send; spinner animation alone is not evidence of an error. This guard closes the
confirmed path discrepancy, but does not classify every prose/tool-output keyword
match while an agent is idle.

Verified locally: two `terminal_error_snapshot` regressions passed; deployment
`222705-18df8229b141d59a` succeeded after correcting the stale static-link gate.
Installed and running SHA-256 both equal
`d37ba3e563569e039d32d45da7dffdf4b2452617b178614d2ae7c1fab006cfe0`;
auth-session and session-list APIs returned HTTP 200, with `feishuFwd_1` working
and no matched error. Source commits: `563c296`, `807fca6`.

## Starting Points

- Frontend slash command path: `static/terminal.js` (`sendContinueCommand`, `sendTextCommand`, slash-command dispatch)
- Session input API: `POST /api/terminal/sessions/{session_id}/input`
- Backend send path: `TerminalManager::send_session_input` and `write_session_input_inner`
- Auto-continue paths: `TerminalManager::run_due_auto_continue_task`, `maybe_send_error_auto_continue`, and generated auto-continue cron script

## 2026-10-26 CodeBuddy 额度耗尽自动换号与恢复就绪

现场 `stockScreener_19_手动复核`（s11696）退出画面为 CodeBuddy
`您的使用量已超出频率限制`，原会话 `01a15e5f-93d6-70c9-acba-b299d4566e53`。
这与积分余额不足不同：账号仍可能有积分。原快捷动作能提取 CodeBuddy ID，
但 `terminal-resume-ready.js` 没有 CodeBuddy kind，等待立即返回 false，因而刚启动
就报告不能发送继续。现识别上下横线围住的原生空输入框；shell 基准可识别替换
旧 UI 后数量未增加的新输入框。命令队列执行时才取基准，并固定发送到点击时的
session ID，避免切换终端后发错。

后端 `manager/account_rotation.rs` 由现有自动继续扫描器驱动，不依赖网页在线。
沿用“继续”开关与无人值守时间窗，只接管 CodeBuddy 明确的余额/积分不足或上述
频率额度错误，普通 429/拥塞不换号。按 LLM_API 同版本账号的 `lastSwitchAt`
升序（未切换过优先）查询实时积分，只选择存在未过期正余额资源的账号；查询失败
或无合适账号时不终止原 Agent。积分接口不能证明模型频率额度可用，当前耗尽账号
在服务内存中排除一小时，每终端失败重试间隔五分钟；服务重启后这些临时标记清空。
最近使用顺序采用已有切换记录，不宣称追踪所有手工原生启动。

在同一用户 HOME 下重新核对原报错、空输入框和账号后，两次独立 Ctrl+C 间隔
250ms；确认进程退出且 shell 返回，才调用原生账号切换、原恢复命令和语义 Enter。
启动后确认 CodeBuddy 实进程、稳定空输入框且无工作状态，才调用统一继续入口。
恢复期间普通自动重试不竞争输入；无 ID、用户草稿、账号被其他操作更改、退出或
就绪超时都明确停止，不发“继续”。不引入配置目录/环境变量旁路或 Agent 生命周期锁。
`POST /api/terminal/sessions/{id}/recover-balance` 是显式恢复接口，额外支持用户已退出
到 shell 的同一报错现场；其他 Agent 暂不走该账号恢复器。

验证入口：`node tests/terminal-resume-ready.test.mjs`（真实 CodeBuddy 失败输入、
就绪延迟、切换终端固定目标）、`node tests/terminal-resume-extract.test.mjs`、
`node tests/terminal-ime-policy.test.mjs`；Rust `cargo test --bin webclx account_rotation`
验证真实换行报错、工作/排队/普通 429 反例、候选顺序和过期/零/失败积分。
队列 `044045-18e1b0b2613884d5` 的 check、3 项 Rust 回归、Clippy 通过（已有警告保留）。
部署 `044502-18e1b0b2613884d7` 已完成，运行/安装哈希一致，API 与静态资源核验通过。
指定业务终端已由其他操作恢复并工作，未为验证再次打断。隔离原生终端 s12384
实测两次 Ctrl+C 后恢复 `01a174d5-f562-7e60-be68-5a8a175ddbf9`，生产快捷模块
只调用一次恢复输入和一次 `/continue`，原生 JSONL 中同一会话仅新增一次“继续”；
测试终端已清理。证据 `/tmp/webclx-native-resume-result.json`。


## 2026-10-26 AGY 原生会话与额度恢复扩展

用户提供 `webClx_16`（s12381），现场为 `⚠ Individual quota reached`，当前模型
`Claude Opus 4.6 (Thinking)`。从实际 AGY 进程的原生 HOME、打开的
`presence/<uuid>.lock` 及匹配的 `conversations/<uuid>.db` 确认原会话
`1014b787-9874-4e1c-8b58-12b8535f086f`，不以最新文件猜 ID，不将 SQLite
交给 JSON rollout 解析器。缺失/歧义不生成恢复 ID。公共命令构造补齐
`agy --conversation=<id>` 和 CodeBuddy，避免落入 Codex 默认命令。

复用有界换号流程和原生 AGY 账号 apply；按 `last_switch_at` 排序，保持真实
settings/模型不变。候选须实时查询成功，原生 footer 对应的模型族（Claude/GPT
或 Gemini）的周及 5h 窗口均有正剩余、未禁用；陈旧缓存、未知/缺失/歧义窗口
不作可用证据。恢复前重查原错误、账号、模型、草稿及原会话 ID；恢复后重查同一
原生会话 ID，才发送一次“继续”。错误换号另要求原生警告/错误行，排除普通
正文中讨论额度短语。AGY 快捷就绪依据上下边框的空 `>` 输入框。

测试额外发现共享工作状态识别漏掉带模型的 `esc to cancel` footer，以及带取消提示
的盲文旋转状态行；在共享错误/活动判断层修复，保留正文和用户输入反例。
队列 `051035-18e1e0ed5d03a6f5` 的 check、account_rotation、agy_presence、
agy_composer、terminal_working_status_detection、terminal_core 命令回归及 Clippy
通过。Node resume-ready/extract/IME 通过，滚动矩阵 9/9（报告
`/tmp/webclx-repo-check/20261025T210146Z-80f1caac08ad-dirty/report.json`）。
AGY 后端部署 `051317-18e1e0ed5d03a6f7` 成功，运行/安装 SHA-256
`4e4582cc17ade30a875e27a521ad54d4ca3a8723fa6e1ea429195a346237ebf9`，
auth/session 200，线上 `/agent-session/complete` 正确返回上述 AGY ID 与
`source=native_presence`。用户已将原终端模型改为 Gemini 且正常完成一轮，
不将这一外部恢复当作自动换号验收。原有无关修改和部署期间出现的并发前端修改均已恢复。


## 2026-10-26 CodeBuddy 现场未自动换号的提取器断路

用户再次指出 s11696 未自动换号。真实 API 已是 CodeBuddy 额度错误、无排队输入，
自动继续开关与时间窗开启；journal 每五分钟报“未找到原会话 ID”。实际进程为
`node /usr/local/bin/codebuddy --resume=01a15e5f-93d6-70c9-acba-b299d4566e53`。
根因不是安卓缓存：既有 `codebuddy_rollout_path_for_process` 仅用于对话史，
`ResumeSessionDetector` 的候选列表未调用它；后端通用命令解析也缺少 CodeBuddy
和 AGY，连退出时的恢复命令都不能识别。此前隔离快捷测试只覆盖 JS 识别/恢复/继续，
不能证明自动换号提取器可用。

修复把原生进程参数与 HOME 下验证存在的 CodeBuddy JSONL 接入公共候选提取器；
补齐 `codebuddy --resume=<id>`、空格/换行形式与 `agy --conversation=<id>`。
按用户提示，运行中暂无法提取 ID 时不再提前拒绝：先核实可用候选账号及空输入框，
两次 Ctrl+C 后确认 shell，解析本次原生 `To resume this session:`；若已有运行中 ID
须完全一致，没有运行中 ID 则要求新的原生退出证据。ID 未确认仍不切换账号或发继续。
回归验证实际启动参数、JSONL 不长期打开、退出命令换行、未知/陈旧/错配 ID。

队列 `052302-18e1e2823cbedf18` 的 check、原生 CodeBuddy 检测 1 项、恢复 6 项、
公共原生退出命令 2 项以及 Clippy 通过。补充陈旧退出提示后续文本变化反例，发布前
随部署队列再次运行，真实自动换号结果仍待安装后记录。


部署 `052453-18e1e2823cbedf1c` 于 05:29:05 成功：恢复 6 项、原生 CodeBuddy
候选 1 项、公共原生命令 2 项及部署门禁通过。运行/安装 SHA-256 均为
`0226de717787b04e113a704059f8d247961c20060629ba78a34f4526132a52c1`，auth/session
200，线上 s11696 `/agent-session/complete` 已正确返回原 CodeBuddy ID 与命令。
无关改动已恢复，tracked diff 与暂存前逐字节一致。业务终端此时正常工作；原生记录
新增的两条“继续”时间为 05:27:25、05:27:38，均早于新版 05:29 安装，属于外部恢复，
不作为本版自动换号成功证据。当前所有 CodeBuddy/AGY 终端均无额度错误，待自然额度
耗尽场景再验收完整自动轮转。安卓无需重启；后台自动恢复由服务端执行。


## 2026-10-26 自然场景：换号成功后再次限频不应停五分钟

自然现场后续出现两次真实轮转证据：05:37:26 活跃账号变为 cb-83371778cc8f913d，
原生记录 05:37:30 新增“继续”；05:42:27 账号变为 cb-43f43d7f592e9ff4，原会话新
进程同秒启动，05:42:32 新增“继续”。ID 始终为原会话。之后仍收到
`您的使用量已超出频率限制`，新的重置时间 10:13:08；该账号实时积分仍有 4236。
故重复报错是模型频率额度耗尽，正积分不证明此额度可用，不能把恢复后的继续当成
普通 429 分支误发；但用户指出再次失败应马上换下一个账号是正确的。

根因：`attempted` 五分钟标记在恢复前设置，成功后从未删除，成功恢复也被当成
失败恢复退避，下一账号耗尽时被阻塞。现成功发送继续后仅清除本终端失败退避，
保留耗尽账号隔离；新真实额度错误可立即触发下一轮，同轮仍由运行 guard 独占。
无可用候选、网络失败或真实恢复失败才保留退避，候选不重复试已隔离账号。
积分查询只能筛余额，频率可用性以新账号的原生执行结果判断；不伪装查询可证明频率额度。

新增阶段/成功日志涵盖候选查询、双 Ctrl+C、退出 ID 核验、实际账号切换、等待原生
输入框以及发送继续；手动入口区分正在执行的阶段与失败退避剩余秒数。
队列 `054547-18e1e35c2c7f07ce` 的 check、恢复回归（含成功后立即再次认领、在途
互斥与失败退避）、Clippy 通过，待本轮部署观察下一自然轮转。


## 2026-10-26 连续轮转上线与设备/模型限制诊断

部署 `054825-18e1e35c2c7f07cf` 于 05:51:19 完成，运行/安装哈希均为
`f023db29378ed98edb8ca2558b7c0577b39d9802f8fdb382b158ba889646b890`，auth/session
200。原无关修改和部署期间并发前端修改已分别保全并恢复。
真实 s12388（sub2api_2）在 05:53:50、05:54:01、05:54:11 连续完成三轮；journal
逐轮记录双 Ctrl+C、原生退出 ID、账号从 cb-8c7c6fb002444366 到 cb-f28853c5c8d7d929、
再到 cb-ecda75698b023c23、再到 cb-18391876de189ade，恢复 ID 始终
`01a174e6-5c3c-749e-b604-21fc26fabc08`。原生 JSONL 对应三条一次性继续（时间戳
1792965230598、1792965241468、1792965251460）。再次限额不再停五分钟；05:57 又
自然换到 cb-abe3df2d1ed81ff8。s11696 当时由用户退出至 shell，未为测试重新打断。

用户怀疑新版本识别设备，且说明这些账号基本未使用；不能将所有失败归因为账号
已耗光。读取已安装官方 CodeBuddy 2.176.0（未修改二进制、设备标识或官方包）：
`dist-server/7376.codebuddy.js` 的 linuxUuid 读取 `/var/lib/dbus/machine-id`，
ClientInfoProvider 生成 machineId；`8035.codebuddy.js` 将它作为安全审计 deviceId，
主包审计 SDK 存在 X-Device-Id。由此只能证明存在原生设备元数据，不能证明该字段
导致本次限频，亦没有版本对照证明近期才新增。
原生 token 解析优先进程环境，再取合并设置 env.CODEBUDDY_AUTH_TOKEN；目标登录
shell 未携带旧 token/base URL 覆盖，已检查祖先项目设置，无同名认证覆盖或 helper；
7 个国内账号有 7 个不同 UID，原生日志可见不同账号身份。

隔离、无工具、新会话短提示对照（原生正常配置，同一设备，未改共享凭据）：
两次测试前后活跃账号均为 cb-abe3df2d1ed81ff8，账号没有切换。
第一轮实际原生记录模型 deepseek-v4.1-flash，成功返回固定字符串；第二轮使用官方
`--model kimi-k3.1-pro`，短提示仍收到相同频率限制，未收到余额不足。
测试终端 s12397/s12399 均通过资源 guard 删除，临时工作目录清理。证据
`/tmp/webclx-codebuddy-fresh-quota-result.json`、
`/tmp/webclx-codebuddy-fresh-kimi-quota-result.json`；成功会话记录
`01a17513-ccde-746c-92ed-03b9af41ff57`。
这排除账号/设备所有模型一律不可用及“仅长历史导致该错误”的简单解释，支持限制
与 Kimi 模型相关；仍无法区分按账号+模型、设备+模型或服务侧模型规则，不能把
“设备风控”作为已证实根因，也不能保证不可检测。未擅自替换用户业务模型。


## 2026-10-26 按用户要求回退前一官方版本并对照

npm 官方稳定版本列表确认 2.176.0 的前一版为 2.175.0。通过 npm 将 2.175.0 安装到
`/home/bin/codebuddy-versions/2.175.0/`，检查官方 product.json 完整后，原子替换
`/usr/local/bin/codebuddy`、`cbc`、`codebuddy-code` 三个命令链接，均指向该目录下
`node_modules/@tencent-ai/codebuddy-code/bin/codebuddy`，三个 --version 均为 2.175.0。
原 2.176.0 npm 包实体保留于 `/usr/local/lib/node_modules/@tencent-ai/codebuddy-code`，
保护既有进程与回滚；原命令链接记录为 `/tmp/webclx-codebuddy-2.176.0-command-links.json`。
没有覆写用户账号、设备文件或既有会话记录，也没有更新/重启 webClx 服务。

两版原生源码均存在 `/var/lib/dbus/machine-id` 读取与 `X-Device-Id` 审计字段，
不能称为 2.176.0 新增设备识别的证据。同一账号 cb-abe3df2d1ed81ff8、同设备、
新会话固定短提示、各轮前后账号均稳定：

| 客户端版本 | 实际/指定模型 | 结果 |
| --- | --- | --- |
| 2.176.0 | deepseek-v4.1-flash | 固定短回复成功 |
| 2.176.0 | kimi-k3.1-pro | 频率限制 |
| 2.175.0 | kimi-k3.1-pro | 频率限制 |
| 2.175.0 | deepseek-v4.1-flash | 固定短回复成功 |

上一版测试终端 s12403/s12404 均由资源 guard 清理，结果分别在
`/tmp/webclx-codebuddy-2.175-fresh-kimi-quota-result.json` 与
`/tmp/webclx-codebuddy-2.175-fresh-deepseek-quota-result.json`。
降级未消除 Kimi 问题；保持用户要求的 2.175.0，不将这一结果夸大为已排除所有
版本/服务端变化，也不宣称已确定或绕过设备限制。


## 2026-10-26 用户明确要求的临时设备标识对照

按用户本轮明确的“测试下更改设备环境指纹”授权，仅在独立原生测试进程的 shell
命令中使用官方 `CLIENT_INFO_MACHINE_ID` 覆盖；不作为终端启动常规注入规则，
不修改系统 machine-id 或用户配置。原生 ENV_CLIENT_INFO_KEYS 映射证明该变量
进入 ClientInfoProvider.machineId。

即时 A/B 两轮均为 CodeBuddy 2.175.0、kimi-k3.1-pro、同一固定新会话短提示，
账号 cb-abe3df2d1ed81ff8 在各轮前后保持一致：
原设备标识 s12405 报频率限制；随机新 UUID 标识 s12406 仍报同样频率限制。
变更组原生日志
`.webclx-fresh-quota-probe-1792966578392__4edb28ccc24624cb3f722cae6366fc26.log`
的 machineId 字段确实包含本次测试 UUID，排除变量未生效的简单解释。
`/var/lib/dbus/machine-id`、`/etc/machine-id` 前后逐字节一致；两个测试终端由
资源 guard 删除。证据 `/tmp/webclx-codebuddy-device-baseline-result.json` 与
`/tmp/webclx-codebuddy-device-override-result.json`。业务命令继续保持 2.175.0。

结论只限于：更改原生客户端 machineId 单项未解除 Kimi 限频。未验证或排除出口 IP、
账号+模型规则、其他设备维度或服务端策略，也不保证更改后不可被识别。


## 2026-10-29 默认禁止自动切入国际账号

用户在国际账号仍返回 403/11140 后要求增加可选限制。CodeBuddy 页签的
「权限与自动化设置」新增默认勾选的「禁止自动切换到国际版账号」，旧配置也默认开启。
服务端候选筛选遵守持久化 `auto.blockInternationalAutoSwitch`，查询后、打断前和
账号应用前重查最新策略；没有国内可用候选时明确停止，不自动放宽到国际版。
取消勾选仍可使用下述跨区域轮转；手动指定账号不受该自动策略限制。
该功能不修复上游 403/11140，也不将其归类为余额耗尽。

验证请求 `181219-18e253049754ab34` 通过 check、10 项换号回归、1 项旧配置/持久化
回归及 Clippy（保留存量告警）。Node 开关交互 4 项及账号页刷新检查通过；账号页
320/390/768/1366 四档浏览器检查通过，滚动矩阵 9/9（报告
`/tmp/webclx-repo-check/20261029T101113Z-6fded87f2c21-dirty/report.json`）。
提交 `637d39f6` 经部署 `181539-18e253049754ab35` 于 18:18:38 上线；release、
工作区图标门禁和滚动矩阵 9/9 通过（部署报告
`/tmp/webclx-repo-check/20261029T101642Z-637d39f69b9e/report.json`）。运行与安装
二进制 SHA-256 均为 `e36e46545d7eb75db0a393aca588c34382a627d32db023490ad56f6cb791e432`，
服务 active、auth/session 200，HTTP HTML/JS 与源码一致，新缓存键有效。

线上 `GET /api/auth/codebuddy-auto` 默认返回 `auto.blockInternationalAutoSwitch=true`；
幂等保存 `true` 后保留原签到/旅行设置，再用旧格式请求保存原签到/旅行值，新开关仍为
`true`，与磁盘状态一致（`/tmp/webclx-international-policy-live-save-result.json`）。
390/1366 两档真实浏览器读取线上 API，展开 CodeBuddy 选项可见且勾选，无横向溢出
或 JS 错误；浏览器检查未修改设置或触碰业务终端。取消勾选后的跨区域候选与再次
启用时的在途重查由前述定向回归验证，未对运行终端制造额度故障。

无关 stash 按 OID 恢复并删除；tracked diff 逐字节一致，86 项未跟踪文件内容、链接和
权限均匹配（`/tmp/webclx-international-policy-restoration-result.json`）。Git stash 未保留
的 18 项普通文件权限按部署前清单复原；没有覆写文件内容。上游 403 仍属 R9 遗留项。

## 2026-10-27 CodeBuddy 自动换号纳入国际账号

用户确认国内账号耗尽时也允许切换国际账号。候选筛选取消 `variant` 同区域限制，
国内、国际账号统一按 `lastSwitchAt` 从未切换/最早切换排序；仍排除当前账号、
需重新登录账号及尚在耗尽隔离期的账号，并通过候选账号所属区域的官方积分接口实时查询。
沿用 `CodeBuddyManager::switch_account` 写入账号对应的原生区域、服务地址与缓存，
再恢复同一原生会话，不替换原模型。积分通过不代表目标区域支持原模型或频率额度可用，
须核对后续原生请求，不能将“已恢复并发送继续”当作业务请求成功。

无候选提示改为“没有未隔离且额度已核实充足的候选账号”，不再误报为同版本限制或余额全空。
回归覆盖国内到国际、国际到国内的排序，以及跨区域仍排除需登录/已隔离账号和未知当前账号。

部署 `132732-18e1f73d9a75f1cc` 于 13:33:25 完成，运行/安装 SHA-256 均为
`87eebb9c0414988ff44933a1730def0d7e5516975716a6458826d1622a5f4408`，auth/session
200。额度恢复回归 8 项、区域端点回归 1 项及部署滚动矩阵 9/9 通过（报告
`/tmp/webclx-repo-check/20261027T053056Z-7725ec5fed9f/report.json`）。无关 tracked diff
逐字节恢复，80 项 untracked 内容/链接逐项核对一致，保全 stash 已恢复并删除。

自然现场：13:33 s12449 从国内账号自动切到国际账号
`1ec509ea-1e37-48ca-8183-c5473d77c189`；s12448 首轮因错误变化取消，13:38:31
自动切入另一个国际账号 `4445eace-43fb-481e-9c11-01ec9be1e90e`，核对原生会话
`01a178e3-7f2d-7e6c-ae4b-cedf98009980`，13:38:37.791 原生记录新增一次“继续”。
原生日志明确向 `https://www.codebuddy.ai/v2/chat/completions` 发出
`kimi-k3.1-pro` 请求；原生自动压缩及后续 chat 均被上游以 `403 request illegal` 拒绝。
因此只验收跨区域选取、实际切号和同会话恢复，不能称为业务恢复成功。

13:40 隔离的新会话短提示对照使用当时原生 CLI 2.177.0、同一上述国际账号；
Kimi 与 DeepSeek 均返回相同 403，未出现余额/频率不足。两轮前后账号保持一致，
测试终端 s12484/s12485 由资源 guard 清理，结果保存在
`/tmp/webclx-intl-rotation-fresh-model-probe-result.json`。同 token 在 workbuddy.ai 与
codebuddy.ai 的官方登录资料接口均返回 HTTP 200 / code 0；仅能证明登录有效，不能证明
模型服务授权充足。国际模型请求的拒绝原因仍未确定，不能归因为长历史、单一 Kimi
模型、余额不足或未切号，不将 403 纳入额度错误而盲目轮换，也未替换业务模型。

13:59 进一步读取原生 JSONL 的 `providerData.error`：s12448 同一原会话的失败为
HTTP 403 / code `11140` / `isRetryable=false`；五个隔离新会话的 Kimi、DeepSeek、
default-model 也均为 403/11140。相同账号/默认模型分别使用 WorkBuddy 国际站的
`/v2` 与 `/console` 基址仍失败，临时配置经 token/键值核对后恢复，测试终端已删除。
证据：`/tmp/webclx-intl-business-rejection-evidence.json`、
`/tmp/webclx-intl-native-rejection-evidence.json`。npm 2.177.0 官方 tarball 的 SHA-512
完整性通过；入口、核心服务包、错误转换包、模型路由包及 product.json 与安装文件
逐字节 SHA-256 一致（`/tmp/webclx-codebuddy-official-integrity-result.json`）。
这能定位到上游不可重试的请求拒绝，尚不能确定 11140 的具体触发条件，不能宣称设备
检测已证实、账号必然封禁或重新登录一定有效。

最小 HTTP 协议探针另返回 400/`11128`，官方 `displayTips` 明示“没通过渠道校验，
请用官方版本重新发送”；该结果与原生 403/11140 不同，不得混为同一根因。国际模型
目录返回 HTTP 200/code 0，包含 default-model，但不包含原业务的 kimi-k3.1-pro；
目录可读和正积分均不证明模型推理可用，也未以替换业务模型或修改设备身份来掩盖失败。

本次补齐 `403 request illegal` 内置错误关键词及 `mark_only` 动作，并将旧配置同名
`continue` 规则迁移为仅标记错误；前端设置默认值同步。此前漏匹配使请求失败的终端
回落成 `completed`，容易误以为恢复成功。回归使用真实错误形态及换行形式，验证
较新 403 覆盖旧额度错误、已发送继续不污染新错误、工作状态仍优先，以及旧规则迁移。
该识别修复不解除上游 11140。验证请求 `140415-18e24d99e75f986f` 的两项定向
Rust 回归及终端错误 Node 检查通过；额外使用的 `-D warnings` 因 settings_core
现有两处 `too_many_arguments` 和一处测试 `needless_borrows_for_generic_args` 失败，
release 尚未执行。这三处不属本次变更；后续按项目既有 Clippy 警告级别保留诊断，
继续构建并由部署脚本执行门禁。

修复提交 `0cf8aeb0` 经部署 `140732-18e24d99e75f9870` 于 14:12:19 上线。
Clippy 保留上述存量告警并成功完成，release、滚动矩阵 9/9、工作区图标门禁通过；
滚动报告 `/tmp/webclx-repo-check/20261027T060948Z-0cf8aeb0e505/report.json`。
运行 PID 4150400，运行/安装二进制 SHA-256 均为
`fa02f4560406980d8aa5364285fd8d8d277ae71b78c7d3b5a41ce8d9624f7b88`，
auth/session HTTP 200；HTTP app.js 与 index.html 静态资源均与源码一致，HTML 资源
包含新的 `20261027-codebuddy-forbidden` 缓存键。

14:19 线上 s12448/s12449 均从 `completed` 改为 `error`，关键词为
`403 request illegal`，重置时间为空；设置 API 的对应规则为 `mark_only`。
逐个调用真实 `/auto-continue` 接口，均为 `sent=false`、`compact_sent=false`、
`retry_after_millis=0`，原生 JSONL 行数及“继续”计数/最后时间在调用前后完全不变。
部署前后“继续”计数分别保持 43/21，没有新增业务请求；s12381 AGY 仍为 `agent`。
证据 `/tmp/webclx-codebuddy-forbidden-{before,after}-deploy.json`。上游 403/11140
仍未解除，业务推理尚未恢复，R9 保持未验收。无关 stash 已按 OID 恢复并删除，
tracked diff 逐字节一致，80 项 untracked 的路径、内容/链接及权限全部核对一致。
