# 项目优化执行计划与进度

更新日期：2026-09-20。管理者负责计划、证据复核和状态更新；GLM-5.3-Flash 执行者负责实施。本文为当前进度入口，历史建议见 [优化评估](../../archive/audits/astra_optimization_recommendations_2026-09-19.md)。

## 状态规则

状态使用：已完成、执行中、待复核、待前置、暂缓。提交代码不等于测量完成，执行者报告通过后仍需管理者复核；失败和环境阻塞保留证据，不计入完成。每次更新记录提交、命令结果、剩余条件，不使用缺乏依据的完成百分比。

## 任务清单

| ID | 工作与价值 | 状态 | 负责人 | 依赖与验收 |
| --- | --- | --- | --- | --- |
| A1 | 统一预设切换文档契约 | 已完成（历史验收） | 执行者 / 管理复核 | `bf70182`；3 份文档，预设矩阵 4 项通过。后续规范变更须另行语义复核，不以关键词消失代表正确 |
| A2 | 修复选择竞态测试夹具 | 已完成（历史验收） | 执行者 / 管理复核 | `b9dbc57`、`7c46ef1`；4 项 Node 契约测试通过，保留原失败记录 |
| A3 | 统一仓库检查入口 | 已实现；全量绿灯未达成 | 既有任务交付 | `43eff20`，已有 `scripts/repo-check.sh`，不重复建设；历史 Node 135/181 通过、browser 3/3 通过，详见检查专题 |
| B0 | 建立性能基线入口 | 已提交；可靠性验收未完成 | 执行者 | `15d8fab`；仅语法、help、dry-run 等入口检查通过，尚无真实性能数据 |
| B1 | 修复基线可信度及环境预检 | 已完成（离线复核） | webClx_9_优化评估_执行者 | `160b8fe`；17 项离线测试通过，真实 Playwright 入口 0 次；资源释放、真实 main 失败路径和三 summary 通过；尚无真实性能数据 |
| B2 | 采集可复现性能数据 | 已完成 | webClx_9_优化评估_执行者 | `de9ce6b`；四组 warm/cold×round1/2 各 90 samples/3 scenarios×30/failure_rate=0；JSONL 在 /tmp/b2-*.jsonl |
| B3 | 确定回归阈值 | 已完成（初步阈值） | 管理者 | 基于四组 B2 数据制定 warm/cold p95、失败率与环境可信度阈值；后续采集滚动复核，不把当前结果作为长期服务性能承诺 |
| C1 | 检查入口失败项分类与修复排序 | 已完成（只读分类） | 执行者 / 管理复核 | 历史 46 失败、当前 dirty 快照 43 失败；恢复 3 项，无新增失败；已按 P1-P4 排序，修复另案派单 |
| P1 | 修复预设切换契约 | 已完成 | 执行者 / 管理复核 | `9c12bba` + `013ed5b`；gate 6/6、in-place 10/10、matrix 4/4；补丁在 tracked `99b73de` 基线 dry-run/apply 均 0，patch 后与运行态逐字节一致 |
| P2 | 修复指定预设动作与迁移测试夹具 | 已完成 | webClx_9_优化评估_执行者 | `d9a787c`；specified-preset-actions 21/21、terminal-designate-preset 7/7、terminal-resume-model-migration-entrypoints 退出码 0；范围仅三个测试文件 |
| P3 | 修复配额格式回归断言 | 已完成 | webClx_9_优化评估_执行者 | `46785d1`；frontend-review-regressions 8/8，退出码 0；范围仅一个测试文件 |
| P4 | 修复Android竖屏契约、compile日志测试、包排除大小写 | 已完成 | webClx_9_优化评估_执行者 | `7850e4f`；android 21/21、compile exit0、source 7/7；patch 基线重放通过 |
| C2 | 发布记录关联部署与 HTTP 资源证据 | 已完成 | 发布执行者 / 管理复核 | 部署请求 `131511-18d6c05b1549dbc3` 成功；运行二进制与 `target/release/webclx` SHA-256 一致；服务端口可达、`/api/auth/session` 返回 200；使用 XFCE Firefox 有效会话请求 `/assets/app.js?v=20260920a` 返回 200，HTTP 响应与 `/home/bin/webclx/static/app.js` SHA-256 均为 `7ac886a5af1d75ba25c0c9d7b1e0cd3924b9df499b6bbf8dd331c1ea173c3e3e`；stash 已恢复 |
| E1 | 修复终端消息接口超时与投递状态不一致 | 已实现、编译并发布 | webClx_9 / 管理复核 | `collab-20260920-e1impl`；实现提交 `2ad4d74`，兼容测试修复 `966e138`；部署候选 `61cf9a8` 后正式发布，保留既有 dirty 现场 |
| D1 | 会话缓存预算 / LRU | 暂缓 | 管理者评估 | 必须先证明资源持续增长及实际压力；有证据才实施，并验证活动会话与恢复行为 |
| D2 | 大模块拆分 | 首轮完成；第二轮执行中 | webClx_13执行 / 管理者计划复核 | `a9d3482`；提取 terminal WebSocket、Agent 上下文与内联测试；队列 `cargo check` 及 27 项 Rust 测试通过；未部署 |

## B1 当前派单与验收

任务号：`collab-20260920-plan`，阶段 B1。代码阅读发现以下问题，修复前不把脚本标记为可用性能基线：

1. `APIResponse.raise_for_status()` 不符合 Playwright Python API，应检查 `ok/status`；API 请求和浏览器需一致的认证上下文，工作区路径可配置并验证。
2. `owner_key` 目前固定，应为每次运行生成唯一归属；删除只能针对本次创建、路径和归属均匹配的会话，清理后回读确认。
3. 资源初始化在 `try` 外，关闭异常可能跳过后续清理。部分初始化、请求失败和清理失败都需保证独立清理尝试及可诊断结果。
4. 样本异常被记录后主程序仍返回 0。运行失败、样本不足、清理失败须产生非零退出码，同时保留样本和失败证据。
5. 输出发送到 second 会话，却切换到 first；不能据此声称目标持续输出首屏性能。建立目标输出证据，区分内部 ready、输出可见和实际渲染时间。
6. 冷热目前只是外部标签。明确测量的是浏览器资源缓存、会话上下文缓存或后端回放状态，并提供可复现准备步骤；不清系统缓存，不把刷新页面视为自动变冷。

执行者文件归属：基线 Python 脚本、对应专题和必要独立测试；本文由管理者维护。至少验证 HTTP 失败、样本失败退出码、部分初始化失败、关闭失败仍继续清理、归属不符不删除。环境预检只读，不泄露 token，不改变共享预设。

阶段检查点：

- B1.1 实现修订：`160b8fe` 已交付；范围限于基线脚本、独立测试和专题。
- B1.2 离线复核：`python3 tests/terminal-performance-baseline.test.py` 17 项通过；用 `async_playwright` guard 覆盖后重跑仍 17 项通过，真实入口尝试 0 次。
- B1.3 生命周期故障注入：二次 launch 失败时旧 browser/context/request 关闭或释放各 1 次，不存在旧资源二次释放；二次 context 失败保留独立新 browser 关闭证据。
- B1.4 真实 main 边界：样本失败保留真实 `run`，stdout 为失败 sample 加精确 3 份 summary；launch 失败为精确 3 份 summary。`py_compile`、`--help`、`--dry-run`、`git diff --check` 均通过。
- B1 结论：离线可信度通过；真实性能数据与回归阈值仍属 B2/B3。

## P1 预设契约裁定与管理决策

- 管理复核通过：`preset-global-switch-gate` 6 项 5 过 1 败，`terminal-in-place-preset-switch` 10 项 6 过 4 败；失败证据与执行者回报一致。基线 `terminal-preset-command-matrix` 管理复跑 4/4 通过。
- 唯一契约确认：临时预设事务必须由 `webclx run` 拥有 apply、启动信号 settle、固定数秒兜底恢复；使用真实共享配置；禁止替代 HOME、运行时租约或直接 apply 后裸启动。原地入口可先等待当前 Agent 退出，但后续仍必须交给 `webclx run`。
- 产品根因确认：当前 `static/specified-preset-actions.js` 临时 launch 改为先直接 apply、固定 3 秒恢复并丢失 `specifiedPresetRunCommand`；当前 `static/terminal-in-place-preset-switch.js` 改为直接 apply 加裸启动命令。旧 `99b73de` 均通过 `specifiedPresetRunCommand` 归属 `webclx run`，与现行 AGENTS 契约一致。
- 修复归属：以产品修复为准，恢复两个静态入口的 `specifiedPresetRunCommand` 事务；不允许删除断言求绿。in-place 测试需补齐 `specifiedPresetRunCommand`、`mobileKeySendQueue` 等夹具，并覆盖 exit→queue→prompt→`webclx run` 顺序、deferred、launch 失败、超时忽略和重试状态。
- 验收：`terminal-preset-command-matrix` 保持 4/4；两个 P1 测试退出码 0；`git diff --check` 通过。修复不得触碰 5 个 src dirty、AGENTS 或部署；静态链接确认后直接修改运行时静态实体目录。

## C1 只读分类结论

- 管理复核通过：执行者未修改文件；历史报告 `b082075470d4` 为 46 失败/135 通过，当前 dirty 快照 `d04d1526a69a` 为 43 失败/138 通过；无新增失败，恢复 `terminal-deleted-session-reconnect`、`terminal-idle-session-icons`、`unify-cargo-targets`。
- 当前 43 项初步分类：运行静态漂移 22、夹具/宿主依赖 3、环境并发 1（复现后恢复）、预设契约 5、其他功能契约 12。该结果是 dirty 工作区快照，不作为发布或全量绿灯基线；正式绿灯须在任务无关 dirty 协调后重跑。
- 修复排序：P1 先语义裁定全局与原地预设切换（`preset-global-switch-gate`、`terminal-in-place-preset-switch`）；P2 处理指定预设动作与迁移（`specified-preset-actions`、`terminal-designate-preset`、`terminal-resume-model-migration-entrypoints`）；P3 处理配额格式回归；P4 处理包排除大小写、Android 恢复竖屏契约和 compile-worker 测试夹具。
- 边界：预设契约禁止批量改断言；静态漂移逐项确认目标缓存键；夹具只改测试/夹具；Rust 相关走编译队列；保留既有 dirty 和 untracked 现场。40 项静默通过仅记录，未复验；历史 static 未入基线提交，只能 HTTP/当前只读验证；真实部署资源身份留给 C2。

## B2 数据与环境要求

- 记录源码提交与 dirty 状态、运行二进制/静态资源身份、浏览器版本、视口、缓存准备、负载条件和测量定义。源码与服务不一致时明确标注。
- 三个场景分别统计样本数、有效数、失败率、p50/p95/p99，逐样本 JSONL 可追溯；失败样本不得静默替换或从失败率分母删除。
- 冷热各两轮，共四组；每组每场景至少 30 个有效样本。保留预热策略与轮次，不混合不同视口或缓存条件。
- 两轮先比较 p95 差异和样本分布；两轮不足以可靠估计跨轮变异系数。环境不稳时扩轮，不立即设置硬阈值。
- finally 清理后检查无本次 fixture 残留或进程泄漏；发生异常先排查，禁止自动无限重试。

## B3 初步回归阈值

- 依据：B2 四组 JSONL 的 12 份场景汇总（warm/cold × round1/2 × 3 场景），每组 30 个有效样本、失败率 0。阈值只约束相同采集口径的 buffer-ready p95，不代表 p99、canvas 渲染或长期容量承诺。
- 单场景通过条件：warm `p95_ms ≤ 1400`、`session_switch ≤ 1050`、`output_switch ≤ 550`；cold `first_connection ≤ 1900`、`session_switch ≤ 1300`、`output_switch ≤ 550`；每场景 `valid_samples ≥ 30` 且 `failure_rate < 0.05`。
- 环境可信度：warm 两轮同场景 p95 变异系数 `≤ 20%` 才比较 cold；cold 初期允许单独轮次抖动，但同一轮内三个 warm 场景至少两项达标，且 cold 阈值不得使用 warm 通过轮的数据放宽。
- B2 实测：warm p95 首连接 963.184–1121.899ms（CV 10.765%）、切换 758.586–854.421ms（CV 8.402%）、输出切换 407.104–429.422ms（CV 3.773%）；cold 首连接 1100.899–1569.054ms（CV 24.797%）、切换 862.943–1066.346ms（CV 14.910%）、输出切换 367.474–423.072ms（CV 9.946%）。首连接和切换阈值取观测最大值并加 20% 边界；输出切换因 cold 与 warm 分布重叠，直接采用统一 550ms。首连接/切换只约束同 cache 的 p95，不把 cold 上限用于 warm。
- 回归判定：正式比较前先满足环境可信度；随后 warm 与 cold 各场景 p95 或失败率任一越界即判回归。数据用于历史趋势比较时，必须核对源码与服务身份、浏览器、视口、负载和采集口径一致；不一致只能另建基线，不得混入本阈值。
- 滚动规则：每完成同一运行身份的两组新测量后，将当前四组加入滚动窗口并保留最新四组；各场景 p95 阈值更新为窗口最大值加 20%，但 warm first_connection/session_switch 不得低于 900/700ms，warm output_switch 不得低于 350ms。若连续 4 组同 cache 的 CV > 20%，停止阈值比较，先扩轮和排查环境。

## 协作现场与调度

- 管理者：`webClx_8_优化评估_管理复核`，会话 `s6233`；负责复核并更新本文。
- 本轮执行者：`webClx_9_优化评估_执行者`，会话 `s6234`；实时 agent-session 返回模型 `GLM-5.3-Flash`，resume ID `01a0b8ad-a842-7293-b69c-ae3bdfece85b`。
- 当前组 token 为 `group-1`：管理者顺序 1、`webClx_11` 顺序 2、原执行者顺序 3；保留全部成员，不重复建终端。用户指定 webClx_11（s6244）负责 E1，与 B1 文件范围分离。
- 发单前已通过 workspace-restores API 保存现场。按用户最新要求，后续派单、跟进和回报统一使用 `scheduled-inputs` 定时消息，端点 `http://127.0.0.1:11111`；发送前实时核对终端名称与 session ID，未来时间、单行正文、`send_enter=true`。先查询待执行任务避免重复，排程成功不等于接收，以对方定时回调或真实终端记录确认；已有确认提交的任务不重发。
- 调度顺序：B1 → 管理复核 → B2 → B3；B1 后可安排 C1 的只读分类。等待回信时不重复发单，不提前启动依赖阶段。
- 现有其他任务未提交改动保留；只提交本任务文件，不建 worktree。B1 不改产品、AGENTS 或部署；E1 可修消息接口产品逻辑，遇到已有 dirty 文件先协调 hunk，部署待管理复核。需要构建或部署时使用既有编译队列。

## 进度记录

- 2026-09-20 10:42：执行者定时 ACK 确认接收 `collab-20260920-e1impl`；计划先隔离既有 dirty hunk、补源码契约测试，仅触碰 `terminal.rs`/`manager.rs` 投递 hunk、delivery 测试和专题文档，完成后实时回报。等待新提交后管理复验，不重复派单或运行旧草稿。
- 2026-09-20 10:45：B3 执行者定时确认通过 `5e79080`，保持待命。管理随后实时派单 `collab-20260920-e1impl`，API rollout 确认进入真实 s6234；范围限定 E1 消息投递状态机、对应测试和专题文档，不改 auth/activity 既有 dirty 或部署。等待实现回报后管理复验超时 pending、claim=false 语义、冲突处理、状态查询和编译检查。
- 2026-09-20 12:49：E1 提交 `2ad4d74` 完成最小投递状态机与查询接口，提交范围仅 `src/routes/terminal.rs`、`src/terminal.rs`、`src/terminal/manager.rs`、`tests/terminal-message-delivery.test.mjs`；未修改 `src/main.rs`、既有 auth/activity dirty、AGENTS 或部署。管理复核确认 handler、in-flight/pending/confirmed 状态、正文 SHA-256 冲突、pending 不完成 build callback 均已进入实现；Node 契约测试在设置 `WEBCLX_LOCAL_TOKEN_FILE=/home/bin/webclx/.webclx-local-api-token` 后退出 0，未设置令牌时 dry-run 子进程因本机 API 401 失败，属于测试环境前提。`git diff-tree --check` 通过；webClx 编译请求 `124749-18d6c05b1549dbbf` 的集中日志 `build-1.log` 显示 `cargo build --release` 退出码 0（260 warnings）。尚未部署，需后续明确部署窗口并按部署技能执行。
- 2026-09-20 13:17：首次正式发布因 `src/terminal/tests.rs` 仍使用旧两参数 claim API 而失败；兼容测试修复提交 `966e138`（4 个调用补正文参数并解包 `Result<bool>`），恢复既有 stash 后重试。部署请求 `131511-18d6c05b1549dbc3` 编译与安装退出码 0，集中日志 `20260920T131516-2913855-22359/build-1.log`，审计 `install-report-1.json`；安装脚本报告服务重启完成，运行 `/home/bin/webclx/webClx` 与 `target/release/webclx` SHA-256 均为 `789400521f195e94ef826ff010f93221e99b8ed12c8a6b40e33b6a3891154a7e`。`/api/auth/session` 返回 200，未认证 `/` 与静态路径返回 303 并跟随至登录页（200），因此本轮无法取得登录后的业务静态资源哈希；物理 `/home/bin/webclx/static/app.js` SHA-256 为 `7ac886a5af1d75ba25c0c9d7b1e0cd3924b9df499b6bbf8dd331c1ea173c3e3e`。既有 dirty/untracked 现场已由 stash `452732d42cc5efee26b4fc4d5b340f1c1d178a58` 恢复。
- 2026-09-20 C2 补证：用户在 XFCE Firefox 完成免密码登录后提供有效浏览器会话。认证请求 `GET /assets/app.js?v=20260920a` 返回 200，响应大小 104541 bytes，SHA-256 `7ac886a5af1d75ba25c0c9d7b1e0cd3924b9df499b6bbf8dd331c1ea173c3e3e`，与 `/home/bin/webclx/static/app.js` 逐字节一致。`/static/app.js` 是 SPA fallback HTML，不作为业务资源路径；C2 认证资源核验完成。
- 2026-09-20 10:35：B3 管理制定完成。从 `/tmp/b2-{warm,cold}-round{1,2}.jsonl` 复核 12 份 summary：每场景 30/30、failure_rate=0。warm p95 CV 为 10.765%/8.402%/3.773%，cold 为 24.797%/14.910%/9.946%；因此 warm 阈值 1400/1050/550ms，cold 单独报警阈值 1900/1300/550ms，并规定 CV > 20% 时停止趋势比较。阈值和滚动规则同步写入性能基线专题；本轮仅管理者文档更新，未改产品或测试。
- 2026-09-20 08:42：管理复核 `013ed5b` 通过。提交仅替换静态补丁；独立目录以 `99b73de` 两静态文件为 base，`patch --dry-run -p1` 和实际 `patch -p1` 均退出 0，patch 后两文件与运行态 SHA-256 一致；`git diff-tree --check` 通过。语义复验用 `--root` 指向补丁后基线目录并加载已批准修订测试，gate 6/6、in-place 10/10、matrix 4/4；不改运行态、Rust、AGENTS 或部署。P1 完成，P2 前先确认测试与静态漂移范围。
- 2026-09-20 08:39：执行者定时确认接收 P1 补丁基线退回，现场已出现替换后 patch 的 staged 草稿；尚无修订提交。等待其完成 tracked `99b73de` 基线验证、仅提交 patch 后复查；不重发正文、不运行旧测试。
- 2026-09-20 08:37：管理复核 `9dc014d`：提交范围与元数据正确，`diff-tree --check` 通过；但独立从提交内补丁解包并在 tracked `99b73de` 静态基线 dry-run 时 4/6 hunk 失败、实际 apply 退出 1。所谓 P1 前运行态基线实际对应未跟踪 `static.checkout-artifact`，含 `futureTask`/direct-apply 漂移；tracked `99b73de` 另一文件可与运行态一致。补丁不能作为当前提交链的可重放静态修复，P1 followup 退回；未运行旧测试，未启动 P2。
- 2026-09-20 08:32：P1 followup复查：无新提交，补丁仍为未跟踪草稿；未运行测试、未重发正文、未启动 P2。草稿已含目标链接、9c12bba 关联、文件/缓存/测试元数据及 in-place 修订，等待执行者完成 dry-run 提交和定时回报。
- 2026-09-20 08:27：管理复核 `9c12bba`：提交只含已批准的两个测试修订；运行时静态链接有效，两个入口已恢复 `specifiedPresetRunCommand` 和 `webclx run` 事务。管理复跑 gate 6/6、in-place 10/10、matrix 4/4，`git diff --check` 通过；5 个 src dirty、AGENTS 和部署未改。因 `static` 是 tracked 绝对 symlink，运行时静态修复无法进入普通 commit，P1 判为有条件通过。已定时派唯一 followup：按既有 `scripts/patches` 格式新增本次修复的可重放补丁，`patch -p1 --dry-run` 通过后仅提交该补丁；不重复测试或启动 P2。
- 2026-09-20 08:28：执行者定时确认接收 P1 followup，暂无新提交；等待其完成后管理复验补丁可重放性和提交范围。
- 2026-09-20 08:12：P1 实现复验无执行者回报；真实 s6234 alive/connected/busy，当前工作区仅出现两个已批准范围内的测试文件修改，两个静态权威入口尚无修改，无新提交。静态符号链接仍指向 `/home/bin/webclx/static`，未见产品静态被半改。按指令不重跑测试、不重发任务正文；等待其完成回报后做统一验收。
- 2026-09-20 07:58：P1 阻塞裁定通过。执行者报告恢复 `specifiedPresetRunCommand` 后 `preset-global-switch-gate` 已 6/6；但 `terminal-preset-command-matrix` 73-76 行仍要求原地切换调用 `specifiedPresetApplyEndpoint`，与 AGENTS 27-38/73-80 裁定的事务归属冲突。管理核对 `adf8c5c`：该断言在 2026-09-15 从旧 `specifiedPresetRunCommand` 改为直接 apply，属于 P1 既有夹具冲突。批准最小扩围修订第三个测试的 73-76 行为原地路径匹配 `specifiedPresetRunCommand(normalizedPresetId)`；最终范围限两个静态入口和三个相关测试，继续禁止改产品 Rust、AGENTS、部署或既有 dirty。验收改为矩阵、gate、in-place 全部退出 0，并保留 `git diff --check`。
- 2026-09-20 07:56：P1 实现定时任务 `paste-1789861629593-6303` 已触发并进入真实 s6234；管理自检确认其开始修改 `tests/terminal-in-place-preset-switch.test.mjs`（当前仅 +2 行），尚无提交或定时回报。按验收规则暂不重跑旧测试、不重发正文；等待执行者完成后的定时回报，再复核提交范围和测试。
- 2026-09-20 07:47：管理复核 P1 只读裁定通过，并确认唯一契约与产品归属。管理复跑两个 P1 测试确认失败数与执行者一致；`terminal-preset-command-matrix` 保持 4/4。已批准按上述边界修复，随后安排实现。
- 2026-09-20 07:37：管理复核 C1 回报通过。执行者确实保持只读，分类和排序有当前 repo-check 报告与逐项日志支撑；当前 43/138 为 dirty 快照，不能当作无脏工作区基线。已将 C1 标记为只读分类完成，P1-P4 修复另案派单；先安排 P1 预设契约只读语义裁定，不改断言、静态或产品代码。
- 2026-09-20 07:25：C1 定时复验：尚无执行者回报，真实 s6234 仍在只读盘点。已看到其解析历史报告失败项、复核静态符号链接和缓存键漂移、对照当前静态资产与相关 Node 测试，并核对运行二进制哈希；未发现其修改本任务文件或产品路径，工作区仍只保留既有无关 dirty。管理未重发任务、未重复测试或派单。保留唯一下一检查，稍后另行排定。
- 2026-09-20 07:13：管理自检确认 `paste-1789859476482-6296` 已进入真实 s6234：终端正读取 repo-check 历史报告、核对静态链接与缓存键漂移，并检查相关测试；该 C1 任务仍在进行且未回信。队列已清空，未重发正文、未重复派单。`s6234` 的 codex_status 继续显示 GLM-5.3-Flash medium。已保留唯一管理复验 `paste-1789859634004-6298`（07:23:53，Asia/Shanghai）。E1 s6244 空闲，仍无 GLM 切换证据。
- 2026-09-20 02:40：管理复核 `160b8fe` 通过。提交只改 B1 三文件（+86/-24），当前工作树与 HEAD 一致；17 项离线行为测试退出码 0，`py_compile`、`--help`、`--dry-run`、`git diff --check` 均为 0。管理 guard 重跑后真实 `async_playwright` 入口尝试 0 次。针对性故障注入确认二次 launch 失败时旧资源 close/dispose 各 1 次、新 browser 未误用；main 样本失败保留真实 `run`，输出失败 sample 加精确 3 份 summary，launch 失败精确输出 3 份 summary。B1 离线验收通过，B2 仍必须等待隔离低负载环境；未运行共享服务压测。已排定 C1 只读盘点 `paste-1789859476482-6296`（02:44:41，Asia/Shanghai），排程不代表接收。E1 真实 TUI 仍为 `gpt-5.6-sol`，GLM 切换证据缺失；状态机仍只读设计。
- 2026-09-20 02:21：已核查单次恢复的真实执行结果：原 429 request id 为 `360f20fc-14fc-4634-9d9b-679acc500bb8`，定时恢复正文进入终端后再次报 `exceeded retry limit, last status: 429 Too Many Requests`，新 request id 为 `8a902dd8-34a8-48ea-baa8-76c331910bff`。终端 alive/connected；API busy=true 不代表请求成功或代码推进。无新提交，仅保留基线脚本未提交草稿。B1 标记上游限流阻塞，停止自动恢复与无变化的周期检查，不重测旧版本、不重复派单、不擅自换模型。现有定时队列为空；等待明确的服务恢复证据或用户调整模型要求后恢复原任务，交付新提交再复验两项。E1 原模型限制尚未解决；B2 不启动。

- 2026-09-20 02:15：两项退回已获定时 ACK。真实 s6234 已修改基线脚本、正在阅读测试时出现 `exceeded retry limit, last status: 429 Too Many Requests`，尚无修订提交。该状态是上游限流中断，不是等待验收；管理未运行草稿/旧测试，未重发任务。已安排 60 秒后仅一次恢复原任务，继续使用 GLM-5.3-Flash；再次限流则记录阻碍，不自动循环重试。唯一管理检查 `paste-1789841761306-6294`（02:21:01，Asia/Shanghai）。B1 未通过，B2 不启动。

- 2026-09-20 02:10：复核 467f122，17 项断言通过且无真实 Playwright 调用。管理追加故障注入证实 second launch 时旧资源关闭 (2,2)，second context 时旧资源 (2,2)、新 browser 关闭 1 次；两路径均有三 summary。根因仍是 release 后 resources 未清空，open 抛错后 finally 再释放旧对象。main 样本失败测试直接替换 run，未验证真实测量失败路径。已定时精确退回两点，保留其余改进、不扩范围；唯一跟进 `paste-1789841418695-6291`（02:15:18，Asia/Shanghai）。未真实压测，B1 未通过、B2 不启动。

- 2026-09-20 B1 三项退回已 ACK：执行者确认仅修改 B1 三文件，处理 launch 受保护生命周期、逐轮资源独立清理及完整 Playwright mock 下真实 main stdout/退出码验证；补第二次 launch/new_context 失败、新对象 factory、关闭计数精确为 1 和关闭后调用拒绝。输出场景按有界输出准确命名并同步文档。接收阻塞解除，尚未交付新提交，不视为验收通过；保留唯一管理检查 `paste-1789841036154-6287`（02:08:56，Asia/Shanghai），B2 不启动。

- 2026-09-20 02:03：收到的完成回报仍对应已退回的 a0e59d2，暂无新提交或 B1 文件改动。真实 s6234 空闲，三项退回正文仍停在输入框且尚无 ACK，不能记为已接收。已短定时核实原消息接收，未重发任务正文、未重复测试；E1 真实模型仍 gpt-5.6-sol。唯一下一检查 `paste-1789841036154-6287`（02:08:56，Asia/Shanghai），待 ACK/修订提交后推进；B1 未通过、B2 不启动。

- 2026-09-20 01:58：管理复核 `a0e59d2`，guard 下 12 项通过且真实 Playwright 调用 0。针对性离线探针确认首次 launch 失败 summary=0；cold reset cleanup 故障后再次对已关闭资源 cleanup，context/browser 各关闭 2 次，现有 >=1 断言掩盖重复关闭。main 仅测缺配置/dry-run，测量分支 stdout 未验收。按原范围定时退回上述三项，保留已通过修复，允许准确收窄输出场景；未真实压测。E1 TUI 仍 gpt-5.6-sol，无新模型证据。唯一管理跟进 `paste-1789840689215-6285`（02:03:09，Asia/Shanghai），B2 待前置。

- 2026-09-20 01:50 检查点：B1 定时 ACK 已收到，真实 s6234 busy 且正在修改基线脚本和离线测试；终端记录草稿测试因 `select failed: s2/s3` 退出 1，执行者正在排查。暂无 ccf2547 后修订提交，管理未执行草稿或旧测试，未重复派单；草稿失败不作为最终验收结果。E1 实时仍 gpt-5.6-sol，尚无原生模型切换证据，实现未开始。前次检查已触发，查重后保留唯一跟进 `paste-1789840257477-6282`（01:55:57，Asia/Shanghai）。B1 未通过，B2 保持待前置。

- 2026-09-20 B1 二轮退回已确认接收：收到 webClx_9 定时 ACK，接收阻塞解除。执行者确认仅改 B1 三文件，先建立 async_playwright 边界全 mock、状态化 FakePage、真实 stdout/唯一三份 summary、指定失败原因和异常清理顺序测试，再修 warm 切换、first evidence、输出场景、cold 重置异常及 cache_scope，并同步专题。ACK 只证明接收与实施计划，不代表修复或验收通过；等待新提交后管理复验，不重复派单、不重跑旧测试，B2 保持待前置。保留已安排的唯一管理检查 `paste-1789839916288-6280`（01:50:16，Asia/Shanghai）。

- 2026-09-20 接收异常纠正：B1 无新提交，真实终端反复显示二轮退回正文停在输入界面，原生 resume 文件未找到 `ccf2547复核未通过` 正文；尚无接收 ACK，不能继续简单记为“执行者待命”。已短延时发送一次仅核实接收的消息，不重发原派单，不运行旧测试；后续依据 ACK/真实对话确认再推进。E1 实时仍 gpt-5.6-sol，模型菜单建议未经实际菜单验证，不能作为已切换证据。B1 未通过、B2 不启动。

- 2026-09-20 当前复核：再次核对 `ccf2547` 后日志，仍无 B1 新提交；s6234 在线但空闲，未产生新的定时 ACK，因此未执行测试、未重复派单。s6244 E1 同样空闲，真实 TUI 仍为 gpt-5.6-sol；状态机与源码证据停留在只读交付，未有实现提交或部署。B1 未通过，B2 继续挂起。

- 2026-09-20 当前定时复核：`ccf2547` 后仍无 B1 新提交，s6234 空闲且未提供新的修订 ACK；因此未运行全 mock 或旧测试，也未重复派单。s6244 E1 仍空闲，TUI 保持 gpt-5.6-sol；最近回调仅完成状态机只读设计，未修改产品代码、共享配置或 dirty hunk。源码证据与既有记录一致，B1 未通过、B2 不启动。

- 2026-09-20 E1 只读交付补充：webClx_11 提供状态迁移与故障测试表，建议 `claimed/in_flight → delivered_pending_confirmation`，确认失败区分 `confirmation_not_written` 与 `partial_or_unknown`；unknown 不释放占位、不自动重发，需 worker liveness/lease fencing 防 TTL 并发重复。状态绑定 session、delivery_id 与正文摘要；同 ID 不同正文返回 conflict；重启后仅 reconcile/unknown，不宣称跨重启 exactly-once；pending 不触发 build callback。具体 hunk 仍在 `src/terminal.rs` send_session_message、wait_for_terminal_message_delivery、claim=false 响应及 `src/terminal/manager.rs` claim/release。原生模型入口仅有真实 TUI 的 Ctrl+P 或 `/model` 选择，当前 API 无 agent_model，s6244 仍 gpt-5.6-sol。B1 检查显示 ccf2547 后无新提交、s6234 待命，因此未重复测试或派单；B1/B2 状态不变。

- 2026-09-20 当前复核：`ccf2547` 之后没有 B1 新提交，真实 `s6234` 仍停留在二轮退回后的待命界面；按规则未重跑测试、未重复派单，B1 仍未通过。E1 `webClx_11` 已定时回报并确认 `src/terminal.rs` 发送阶段 10 秒 watchdog、外置 rollout 确认轮询及 `claim=false` 伪成功风险；状态机建议仍只读设计，未修改 dirty hunk。其真实 TUI 仍为 `gpt-5.6-sol`，GLM-5.3-Flash 原生切换尚无证据，不能按计划模型记账。B2 保持待前置。

- 2026-09-20 E1 状态机检查点：执行者已 ACK 超时范围及 claim=false 伪成功问题，提出 in-flight、delivered_pending_confirmation、confirmed、failed；管理认可设计方向但未验收实现。补充要求区分确定未写入与部分写入/结果未知，未知状态禁止释放占位重发；活动 worker 不得因 TTL 回收形成重复投递；相同 delivery_id 不同正文需冲突处理，明确重启及状态回收边界。pending 不得完成 build callback，confirmed 必须有真实 rollout 证据。通过原任务定时消息要求具体 hunk/迁移/故障测试表及原生模型选择交互依据；仍只读，未修改产品或共享配置，B1/B2 状态不变。

- 2026-09-20 E1 根因复核：webClx_11 正确署名回调，仍为 gpt-5.6-sol；原生 GLM 切换需外部交互的阻碍已记录，未改共享配置。管理核对 src/terminal.rs:2763 的 10 秒 timeout 包围发送阶段 spawn_blocking，超时不取消已运行线程，可能正文已投递却 HTTP500；后续 rollout 确认在此 timeout 之外，60 次 100ms 仅是 sleep 预算，每次 run_terminal_task 另受超时约束，不能声称完整确认最多 6 秒。另发现 src/terminal.rs:2830 将 claim=false 直接作为 submitted:true，而 manager.rs:2971 的 false 同时包含历史已确认与 in-flight 占位，存在未确认伪成功风险，需并发重试状态测试。已定时补充原 E1 复核意见，要求只读整理最小状态机、hunk 和测试设计；未批准绕过模型要求或修改既有 dirty。B1/B2 状态不变，保留唯一管理跟进 6269。

- 2026-09-20 01:21：收到执行者 `ccf2547` 定时回报并核对真实终端（GLM-5.3-Flash），管理二轮复核未通过。guard 离线测试捕获 2 次真实 Playwright 入口尝试；9 项断言通过、2 项失败，不能采信“11 passed 即全 mock”。状态化探针确认 warm 在未 goto 的页面等待，随后也未按目标切换；返回 first 却等待 second_evidence。离线成功路径汇总数为 6，应为 3。cold 轮间 close/context 初始化异常可能再次使用已关闭资源，现有测试重复返回同一 browser/context 不能证明资源隔离。输出突发不等于切换期间持续输出，warm cache_scope 与实现不符。已通过短延时任务 `paste-1789838453575-6267` 退回原 B1 范围，待 ACK 和修订，不重复派单。E1 实时 TUI 仍 gpt-5.6-sol，已通过 `paste-1789838453576-6268` 跟进原生模型选择具体阻碍及源码证据，保持只读、不碰 dirty。恢复现场 PUT 成功；唯一管理跟进 `paste-1789838453683-6269`，01:25:53（Asia/Shanghai），排程不代表接收。未运行真实测量，B2 保持待前置。

- 2026-09-20 01:14：管理再次核对，B1 暂无修订提交，脚本和 offline test 两文件正在修改；真实终端显示执行者运行草稿离线测试仍有 AssertionError、退出码 1，正在排查，不作为最终交付。管理未重复执行旧测试，B1 未通过、B2 不启动。E1 真实终端记录包含先前错误署名阶段回调的排程查询，当前界面仍显示 gpt-5.6-sol；共享配置 gpt-6-astra 不代表 live 模型，GLM 切换未落实，根因线索尚待源码复核。旧管理跟进已触发，查重后仅新增 `paste-1789838054048-6265`，01:19:14（Asia/Shanghai）检查新交付，不重复派单。

- 2026-09-20 01:12：B1 已收到定时分项 ACK，七项修订计划覆盖 stdout/失败 summary、warm evidence 一致性、防命令回显 marker、实际输出切换、逐样本冷资源、逐项异常清理及严格 owner、main 全 mock 行为断言。真实 s6234 终端确认正在重写脚本，工作区该脚本已有修改；尚无新修订提交，因此不运行草稿测试、不将 ACK 视为验收通过，B2 保持待前置。E1 已以 webClx_11 正确署名确认身份，报告模型切换尚未执行；共享配置的 model 不是运行中 Agent 模型证据，模型要求仍待解决。另收到署名 webClx_9 的 E1 根因阶段回调，称 10 秒 spawn_blocking watchdog 超时不取消后台投递；该结论暂记为待核验线索，需核对真实来源及源码后采信，不据此认定 B1 执行者接管 E1。保留唯一管理定时跟进 `paste-1789837688834-6261`（01:13:08，Asia/Shanghai），不重复派单。

- 2026-09-20 01:08：B1 仍无新提交或文件修改，退回意见尚无分项 ACK，因此未重跑旧测试、未进入 B2。已安排短延时接收核实 `paste-1789837688415-6259`，仅跟进原任务、不重发退回正文。E1 真实终端已只读追踪消息路由与 manager，收到接单回调，但其正文错误署名为 webClx_9；结合 webClx_11 终端排程记录确认来源，已安排身份纠正及模型核实 `paste-1789837688416-6260`，当前仍显示 gpt-5.6-sol，未声称已切换。唯一管理兜底跟进为 `paste-1789837688834-6261`，01:13:08（Asia/Shanghai）；新回调到达时优先处理证据。
- 2026-09-20 01:04：收到执行者定时回调，确认其已采用定时通信；该回调仍汇报旧提交 `3e00ceb` 的 12 项测试，不是对管理退回问题的答复，不改变 B1 未通过状态。现场核查暂无修订提交或 B1 文件改动，退回正文仍见于执行者终端输入界面，尚无分项修复 ACK；不重复派单。E1 终端已读取通信规则并查询定时 API，但尚无接单回调或 GLM 模型切换证据。保留唯一管理跟进 `paste-1789837327689-6257`（01:07:07，Asia/Shanghai），届时检查两终端的新证据。
- 2026-09-20 01:01 定时跟进：最新 B1 提交仍为 `3e00ceb`，对应三文件暂无新修改；执行者真实终端显示通信规则已读，复核退回正文已出现在输入/队列界面，但尚未看到对该退回意见的 ACK 或修订动作。本轮不重复派单、不重复运行相同测试；等待定时回调确认后检查新交付。
- 2026-09-20：复核 `3e00ceb` 未通过，按短延时定时消息退回。默认 `output=json.dumps` 不打印 JSONL，失败路径无 summary；warmup 等待未发送的新随机 marker；printf 命令包含完整证据导致回显假阳性；output_switch 未从 second 切走就再次选择 second；cold 多样本复用已暖 context。创建校验失败前未登记 fixture，owner 只检查非空。管理离线注入证实两个 fixture 的首个 DELETE 抛 transport 异常后只尝试一次删除。为避免真实启动浏览器，管理替换 async_playwright 为抛错 guard 后运行原 12 个测试，断言仍全部通过，但捕获两次真实 Playwright 入口调用尝试，说明非零退出断言掩盖了失败原因，不能据此称为全 mock 验收通过。要求补 stdout/summary、marker/目标、资源生命周期与异常清理行为测试；B2 继续待前置。
- 2026-09-20：按用户指示向 webClx_11 派 E1。实时 API 确认 s6244 alive/connected，tmux 显示原任务结束并处于输入界面；当前显示模型为 gpt-5.6-sol，已要求按原要求使用原生模型选择切至 GLM-5.3-Flash 并提供证据，不把计划模型写成已验证模型。本次 workspace-restores PUT 返回 HTTP 200。E1 不触碰 B1 文件或本计划；manager/activity 的既有修改先只读核查、必要时协调。B1 验收门槛保持不变。
- 2026-09-20（定时跟进）：最新提交仍为 `c563863`，执行者正在修改脚本并新增离线测试，未交付最终提交。管理运行 `python3 tests/terminal-performance-baseline.test.py` 退出码 1：初始化故障测试被 `/tmp/unused` storage-state 不存在提前截断，未到预期注入点，随后异常信息断言失败。本次仅反映检查时的草稿状态。
- 同轮代码复核发现：`run` 先关闭 context 后才由调用者清理会话，请求上下文已失效；运行异常跳过会话清理，context 关闭异常还会跳过 browser 关闭。已要求执行者在资源关闭前清理会话，逐项独立清理并回读验证；离线测试必须完全替代浏览器和网络，验证真实异常路径。另退回旧 marker 假阳性、失败样本未落盘、持续输出与冷热标签不符等问题。B1 保持执行中，B2 不启动。
- 2026-09-20：核对最新仓库，修正“自动检查入口尚未建设”的旧判断：`43eff20` 已实现；全量失败项仍待分类。
- 2026-09-20：复读性能脚本后重开可靠性验收，已派 B1；原 help/dry-run 验证不作为真实运行成功依据。
- 2026-09-20：跟进发现执行者重复遇到同一语法错误，补充消息最初未获提交确认；检查真实终端确认消息在队列中，随后中断重复执行并要求分阶段恢复。终端显示重写后的语法检查已通过，但尚无故障测试及最终提交证据，B1 保持执行中。
- 2026-09-20：确认 C1 历史 JSON 报告仍可读取（181 项、46 项失败）。该数据属于旧提交的 dirty 工作区，不能直接认定当前仍有 46 项失败，也不能仅凭专题摘要认定全部源于静态漂移；后续分类须逐项核对日志和现状。

相关专题：[性能基线](terminal-performance-baseline.md)、[仓库检查入口](repo-check-entry.md)。

## P2 指定预设动作与迁移入口语义裁定

- 管理复核 2026-09-20：三个相关测试失败均为测试夹具漂移而非产品回归。
- `specified-preset-actions.test.mjs` 20/21：唯一失败为缓存键 `20260821c` vs HTML 当前 `20260916a`，更新测试缓存键即可。
- `terminal-designate-preset.test.mjs` 2/7：五处失败涵盖 HTML 空格格式差异（`临时 \/ 新终端 \/` vs `临时/新终端/`）、`title` 去掉 `（临时）` 后缀（UI 精简）、`namingAction` 条件赋值重构（非回归）。更新测试匹配当前产品状态。
- `terminal-resume-model-migration-entrypoints.test.mjs` crash：正则要求 `prepare_codex_history_args_model_for_current_user(...)?;` 直调风格，但当前 `cli.rs` 改为 `if let Err(error)` + restore-on-failure 模式，属更好的错误处理且保留 `codex_history_args_with_model` 下游调用。更新正则接受当前模式。
- 派单范围：仅三个测试文件；不改产品代码、AGENTS 或部署。验收：三项测试退出码 0，`git diff --check` 通过。

- 2026-09-20 P2 管理复核通过：`d9a787c` 仅改三个目标测试文件（+10/-7），specified-preset-actions 21/21、terminal-designate-preset 7/7、terminal-resume-model-migration-entrypoints 退出码 0，`git diff --check` 通过。产品代码、AGENTS、部署和 5 个 src dirty 未触碰。P2 完成。

- 2026-09-20 P3 管理复核通过：`46785d1` 仅改 `tests/frontend-review-regressions.test.mjs`（+1/-1），8/8 通过，退出码 0，`git diff --check` 通过。产品代码、AGENTS、部署和 5 个 src dirty 未触碰。P3 完成。

## P4 只读裁定范围

- `android-client-contract` 20/21：唯一失败为 `terminal.html` 缺少 `terminal-orientation-toggle` 元素且不含 `data-action="restore_portrait"`+`hidden`，属产品 HTML 回归。
- `compile-worker-compact-logs` crash：`compile-worker.js` 中无法按测试正则提取 `safe_path_slug`，需确认函数名是否变更。
- `compile-worker-audit-paths`、`compile-worker-cargo-target`、`compile-worker-required-artifacts` 均退出码 0。
- 包排除大小写项在当前测试中未定位到对应失败，需进一步确认是否由 C1 分类映射到其他测试。

## P4 管理裁定

- Android：产品回归。文档 `android-orientation-toggle.md` 2026-08-16 明确单向恢复竖屏；测试 `02c222d` 与文档一致。tracked `49d8fa6^` 静态已漂移为双向 `rotate_screen` 且无契约变更记录。修复归属产品静态，恢复单向 `restore_portrait`，范围 `terminal.html`、`terminal.js`、`terminal-mobile-keys.js`，以 tracked `49d8fa6^` 三文件为基线生成可重放 patch，并同步更新 android 测试中的 `terminal.js` 缓存键断言。
- compile-worker-compact-logs：夹具超前。`ac1dfa1` 同批新增测试引用的 `allocate_project_log_prefix`、`compact_record_label`、`elapsed_seconds_label` 在任何提交中均不存在；通知断言与 `15e464e` 后的权威契约（集中日志/集中审计/原始备注 + 投递标记）冲突。裁定：重写该测试为当前实现的真实契约测试，覆盖 `safe_path_slug` 存在性、通知格式关键短语与 `[webclx-callback:complete:<id>]` 投递标记；不实现投机性日志分配功能。
- source-release-package：测试漂移。`49d8fa6` 将 `AGENTS.MD` 更名为 `AGENTS.md`，AGENTS 规则明确唯一小写规范文件；测试第 51 行改为断言小写。
- 验收：android 21/21、compile compact-logs 退出 0、source-release 7/7；patch 从 `49d8fa6^` 基线 dry-run/apply 退出 0 且与运行态逐字节一致；`git diff --check` 通过。

- 2026-09-20 P4 管理复核通过：`7850e4f` 仅改 patch+3 测试文件（+1037/-116）。android 21/21、compile compact-logs 退出 0、source-release 7/7；patch 从 `49d8fa6^` 基线 dry-run/apply 均 0，三静态文件与运行态 SHA-256 一致；`git diff --check` 通过。产品 Rust、AGENTS、部署和 5 个 src dirty 未触碰。P4 完成。

- 2026-09-20 B2 管理复核通过：`de9ce6b` 仅改基线脚本 owner 判定（+7/-2），`git diff --check` 和 `git diff-tree --check` 均 0。四组数据完整：warm/cold × round1/2，各组 90 samples / 3 scenarios × 30 / failure_rate=0；JSONL 文件 `/tmp/b2-{warm,cold}-round{1,2}.jsonl`。warm p95 首连接 963-1122ms，切换 759-854ms，输出切换 407-429ms；cold p95 首连接 1101-1569ms，切换 863-1066ms，输出切换 367-423ms。无泄漏 fixture，产品 Rust、AGENTS、部署未触碰。B2 完成。

## D2 实施与验证（2026-09-20）

- 边界：`src/terminal.rs` 5310 → 4766 行，新增 `terminal/websocket.rs` 574 行；`src/agent.rs` 5260 → 4217 行，新增 `agent/context.rs` 338 行、`agent/terminal_profile_tests.rs` 721 行。提取具体职责，未改路由、业务行为、认证或部署。
- 原函数体逐段比对：除可见性及 rustfmt 格式外一致。三个 WebSocket 源码契约测试仅更新读取路径，原断言保留，backlog-replay / output-viewed-ack / output-observation-persistence 均退出 0。
- 本轮扫描 22 个引用原入口的 Node 测试：15 项通过，3 项因文件移动更新读取路径后通过；其余 4 项失败保留：initial-path-session 的旧源码模式、session-activity 的缓存键、backend-architecture 路由数量 191/190、message-delivery 的外部 skill API 401。前两项通过只读替换 HEAD 源码输入复现同一失败；路由与消息 skill 本轮未改，不据此声称全量绿灯。
- 变更文件单独 rustfmt 检查及 `git diff --check` 通过。全局 `cargo fmt --check` 仍报告既有 `src/terminal/tests.rs` claim 断言格式差异；保留该文件原样。
- 编译队列请求 `180438-18d6ff5d6aa98fcb`：`cargo check --locked`、Agent 原测试模块、终端鼠标输入识别及首屏回放边界 Rust 测试；测试整体限定 180 秒，回调已收到并核验：日志 `/home/bin/webclx/compile/runs/20260920T180443-1186706-29195/build-1.log` 最终 `status=0`，`cargo check` 成功；Agent 测试 22/22、鼠标输入 4/4、首屏回放 1/1，共 27 项通过。实现提交 `a9d3482`。未部署，保留已有 5 个 dirty 文件及未跟踪现场。

### D2 后续分工计划

用户指定 `webClx_13` 执行，管理者只负责计划、复核和本计划文档，不与执行者并行编辑产品文件。

| 阶段 | 范围 | 状态与验收 |
| --- | --- | --- |
| D2.1 | WebSocket / Agent 上下文 / 内联测试 | `a9d3482`，验收记录 `f1f0bee`；已完成 |
| D2.2 | 终端 Codex 历史扫描、删除与元数据职责提取 | 已完成：`a319f27` + `f750ba7`；迁移等价及最小可见性复核通过，首轮成功验证 34 项 Rust 测试，收尾复验 7 项；未部署 |
| D2.3 | Agent 生产技能发现与描述解析提取 | 已完成：`82e7f91`；五函数等价、最小可见性、Node 2/2、队列 cargo check 与 Rust 22/22 通过；未部署 |

- 任务 `collab-20260920-d2-history`；管理者 `s6233 / webClx_8_优化评估_管理复核`，执行者 `s6477 / webClx_13`，同属 live `group-2`。工作区恢复注册表已保存。真实 TUI 显示 AGY / Gemini 3.8 Flash medium；未核验 GLM，不按计划模型记账；Agent session API 当前未返回 resume_id。
- 执行者文件归属：`src/terminal.rs`、新增历史职责模块、必要的 `terminal/tests.rs` 导入及直接受影响的源码契约测试、模块拆分专题。管理者独占本计划。保留原 5 个 dirty 文件与所有未跟踪现场；不部署、不改 AGENTS、不创建 worktree。
- 派单排程 `paste-1789898877075-6512`（2026-09-20 18:08:01 Asia/Shanghai）；排程成功不等于已接收。要求短定时 ACK、阶段交付和回调，禁止重复发正文。
- 唯一管理自检 `paste-1789898877077-6513`（2026-09-20 18:17:57 Asia/Shanghai）；检查 ACK、真实终端及新提交，无新交付不重测不重派。

- D2.2 接单确认：收到 `webClx_13` 的 `collab-20260920-d2-history ACK`，自报真实模型 Gemini 3.8 Flash，与此前 TUI 核验一致，无其他任务冲突；确认保护 dirty/untracked，先梳理依赖与基线，再纯移动并走编译队列。该回信仅证明接单，尚无实施提交或测试交付；保留既有管理自检，不重派、不提前验收。

- D2.2 管理自检（2026-09-20 18:18 Asia/Shanghai）：ACK 已收到；live API 与真实 AGY 终端均确认 webClx_13 正在推进，尚无新提交（HEAD 为 `2350a5f`）。现场新增 `terminal/conversations.rs`，入口与专题为执行中草稿。首轮编译 `181535-18d6ff5d6aa98fcc` 回调失败，终端证据为私有项被 `pub(crate)` 重导出；执行者已调整导入可见性并提交第二轮 `181758-18d6ff5d6aa98fcd`，等待回调，不提前认定通过。专题草稿仍描述旧重导出方式，最终交付时须核对纠正。本次未重跑测试、未重复派单、未查询构建日志、未部署，原五个 dirty 文件与未跟踪现场保留。原自检已消费；新唯一自检 `paste-1789899515830-6516` 定于 2026-09-20 18:28:35 Asia/Shanghai，届时按新交付证据复核，D2.3 继续等待 D2.2 验收。

- D2.2 交付复核（2026-09-20）：`a319f27` 仅含入口、conversations 模块与专题三个文件。迁移职责段（扣除移回 DTO 的 impl、可见性和空白）逐字等价；任务文件 `rustfmt --check --config skip_children=true`、提交 diff 检查通过。收到交付后核验第二轮日志 `/home/bin/webclx/compile/runs/20260920T181803-1314395-14522/build-1.log`：cargo check 成功，测试 5+2+4+1+22=34 项通过，最终 status=0。仍需最小收尾：父模块 DTO 字段与 total_deleted 无需扩大为 pub(crate)；子模块 Metadata 字段收窄至必要范围；专题错误重导出描述改为实际私有 use。已短定时回送执行者，待修订提交和可见性编译验证再关闭 D2.2；D2.3 未启动。保留既有唯一自检 `paste-1789899515830-6516`，不重复排程；原五个 dirty 及未跟踪现场未动。

- D2.2 最终验收（2026-09-20）：核对 `f750ba7` 仅修改原三文件，可见性与专题问题均已修正；管理侧任务文件 rustfmt（skip_children）与提交 diff 检查通过。收尾编译日志 `/home/bin/webclx/compile/runs/20260920T182230-1363404-10905/build-1.log` 记录 cargo check 成功、历史扫描 5/5、删除 2/2、最终 status=0。结合 `a319f27` 主体等价及此前 34 项测试证据关闭 D2.2；不宣称全量测试通过。原五个 dirty 和未跟踪现场保留。
- D2.3 任务 `collab-20260920-d2-skills` 已通过短定时消息 `paste-1789899868766-6520` 安排 webClx_13 只读评估（待 ACK）。要求梳理 skill 函数/路由可达性、Harness 边界、状态依赖与测试，提供最小移动方案及价值判断；不改文件、不编译或部署。原 D2.2 自检已取消，唯一新自检 `paste-1789899868938-6521` 定于 2026-09-20 18:34:28 Asia/Shanghai；收到方案后管理复核再安排实现。

- D2.3 接单确认：已收到 webClx_13 定时 ACK，确认只读边界及生产调用链/存活度评估范围。状态更新为评估中，保留唯一自检 `paste-1789899868938-6521`，不重复派单；尚未收到方案，未授权实现。

- D2.3 方案裁定（2026-09-20）：核对 `routes/agent.rs`，skills 列表路由仍注册，发现链具有独立边界；执行者原约 449 行方案同时包含旧 loop 脚本执行和显式调用，且遗漏父模块对 tool_* 的可见性需求，因此收窄实施。仅将 `SKILLS_DIR_RELATIVE`、`skills_dir_for_user`、`scan_all_skills`、`scan_skills_dir`、`parse_skill_description`、`parse_yaml_front_matter_field` 移至 `agent/skills.rs`；前两个函数按父模块实际调用使用 pub(super)，其余私有。HTTP、旧执行、路径安全和显式调用留在原处。允许四文件：agent.rs、新模块、builtin-terminal-message-skill 源码契约测试、模块专题；不动既有现场。管理侧已复验该 Node 基线 2/2。验收为迁移等价、任务 fmt、原断言保留、队列 cargo check 与 Agent 22 项测试；既有测试不等于完整发现行为覆盖，保留这一限制。短定时派单 `paste-1789900209060-6525`，待 ACK；继续保留现有唯一自检 `paste-1789899868938-6521`。

- D2.3 实施 ACK：webClx_13 已确认仅提取五个发现/解析函数及目录常量，四文件边界、最小可见性、原逻辑留存和现场保护均已确认。状态为执行中；等待等价比对、任务格式检查、Node 契约及编译队列 22 项测试交付，再由管理者验收。沿用唯一自检 `paste-1789899868938-6521`，不重复派单或排程。

- D2.3 最终管理验收（2026-09-20）：`82e7f91` 仅含授权四文件。管理侧对五函数迁移段移除两处 pub(super) 后逐字比较一致，目录常量不变；父模块仅增加私有模块/导入并移除原定义，Node 测试只扩展源码读取路径，原断言保留。任务文件 rustfmt（skip_children）、提交 diff 检查及 Node 2/2 复验通过。收到交付后核验队列 `183237-18d6ff5d6aa98fd0` 日志 `/home/bin/webclx/compile/runs/20260920T183243-1469258-31859/build-1.log`：cargo check 成功、Agent 测试 22 passed/0 failed，最终 status=0。这些测试不代表完整技能扫描行为覆盖或全仓绿灯。五个既有 dirty 文件及未跟踪现场保留，未部署。D2.1–D2.3 本轮计划全部验收完成；执行者保持待命，后续拆分需先评估生产职责价值。旧自检 `paste-1789899868938-6521` 已消费，当前无待执行同类排程，不再重复安排。

## 持续验证后续（todo.md）

- 2026-09-20 19:26 管理复核：webClx_11 已 ACK 接手 T1–T5，真实终端仍在整理证据。独立核对四份历史 B2 JSONL 均 93 行，SHA-256 与执行者回报一致；尚无持久化证据提交，T1 未验收。历史 warm 的 cache_scope 同样写为 fresh_browser_per_cold_sample，须保留并说明口径限制；脚本默认环境不当作历史运行实证，源码 HEAD 不当作运行二进制提交。T4 当前回报仅含提交/模型身份，尚缺真实消息 ID、时间与状态/ACK 对应，未算观察完成。已短定时发送这些复核意见（paste-1789903569462-6538）。此前短唤醒已消费，查询时队列为空；要求执行者在首个实际观察窗口建立后登记至少 24 小时的唯一后续唤醒。T2 等环境条件，T5 等发布前置。管理唯一复查 paste-1789903569463-6539 定于 19:41:09 Asia/Shanghai；不重复派原任务，不与执行者并行改 todo.md。

- 2026-09-20 19:41 持续验证复核：bc90522 四份持久 JSONL 与 /tmp 原件逐字一致，12 summary 均30/30且失败率0，数据保存部分接受。afb0269 的 T4 结论退回：源码状态查询不要求正文，空 data 仅跳过摘要冲突检查；区域 ID 未证实为真实 delivery_id，unknown 不能用于推定历史状态；原始正文未充分最小化。观察时间戳对应19:11–19:28:29，与文案20:22不符；次日20:00已超过24小时，保留执行者唯一唤醒 paste-1789903746994-6540，要求纠正文案。T1 manifest 还须明确默认值非历史实证及 warm cache_scope 限制。已发送最小修订意见，等待原执行者交付，不重派任务、不启动测量或部署；已有阶段回报排程6541，无需重复管理唤醒。

- 2026-09-20 持续验证修订复核：022299d仅修改证据及todo，提交diff检查通过。T1历史数据归档与默认环境/warm cache_scope限制说明接受；当前身份不作为历史运行实证。T4空data查询语义、group_label边界、时间换算及正文最小化主要修订接受，首窗仍为证据不足，整体未完成。剩余文字收尾已定时反馈：查询状态只需真实session_id+delivery_id，正文仅用于摘要冲突校验；ACK清单5条需与文档2条ACK及阶段回报分类对应。live API核实执行者s6244存活，唯一第二窗口排程paste-1789906015145-6550到期2026-09-21 20:00 Asia/Shanghai，已超过首窗24小时。T2待环境、T3待T2、T5待发布，未启动测量或部署；不重复排程。

- 2026-09-20 持续验证文字收尾验收：执行者回报5106018，但本仓库无法解析该提交；实际核验e1f8398（docs: 收窄验证证据收尾记录），仅3份授权文档/JSON修改，提交diff检查通过。T1仅勾选历史数据保存，运行身份项仍未完成；T4查询必要条件已纠正为真实session_id+delivery_id，5条回调明确分为4条阶段报告和1条ACK，无独立timeout ACK样本。上述收尾通过，T4整体仍待次日窗口且状态覆盖证据不足；沿用6550，不新增唤醒。以实际核验提交记账，不采信未解析的回报哈希。

- 2026-09-20 用户要求“继续测完”：已核验 webClx_11 原生终端处于待命、模型 GLM-5.3-Flash medium，通过短定时消息 `paste-1789914442901-6569` 续推原任务（待 ACK）。改为集中执行当前身份/登录态与负载预检、满足条件后的串行四轮性能测量及比较、无外部副作用的消息状态测试、当前运行版本只读烟测；24 小时观察独立保留 `6550`，不阻塞即时测试。不把旧版本烟测当拆分版本验收，不自动部署；仅真实前置缺失才记录阻塞。执行者维护 todo/证据，管理者复核结果；保留全部既有 dirty/untracked。

- 当前窗口交付复核：实际提交链 ea957da（数据）、64be7d6（编译结果）、90f68e4（环境噪声备注）均存在且提交 diff 检查通过。管理侧回读四份 JSONL，各90 sample+3 summary，各场景30有效样本、summary失败率0，p95与报告一致并低于现阈值；仅接受当前窗口结果，保留非隔离环境、warm cache_scope既有口径限制，不关闭跨时段稳定性任务。收到编译完成回报后核验224408日志，release build最终status=0；构建成功不等于Rust测试或已部署。T4源码/客户端mock检查不等于真实状态机故障覆盖，T5仍待拆分版本发布。90f68e4为最终证据补记，0d1e57e错误回报不采信。下一步保留已有跨日观察，不重跑本窗口测量。

- 用户最新裁定：取消必须跨日/间隔24小时的验收门槛，以实际持续运行时间、进程/版本连续性、期间活动与样本充分性判断。旧次日唤醒 paste-1789906015145-6550 已删除；短定时续推 paste-1789915818535-6703，要求 webClx_11 立即核实进程启动/重启证据、补当前消息状态窗口，并复用已完成360样本作有限时长验收，更新 todo。排程待ACK。环境噪声与未覆盖故障路径保留为结论限制，不以等待日历翻页代替测试，也不把存活时长当故障覆盖。T5仍待真实发布前置；不部署、不重复测量，执行者独占todo/证据，管理者复核。

- 定时消息提交故障（用户实证需要手动Enter）：定位到scheduled worker仍用原始CR且发送前删除任务，未走立即消息修复路径。本轮新增语义Enter/原生确认、有限仅Enter重试、持久状态和单任务查询、重启unknown不重发及同终端pending阻塞后续正文；skill自带脚本新增--delay-seconds，排程与提交确认分开。Python离线4项、原Node消息回归通过；编译请求230253-18d6ff5d6aa98fd9日志最终status=0，cargo check与15项scheduled_过滤Rust测试通过。专题terminal-scheduled-message-delivery.md记录边界。尚未部署/真实TUI验收，不能声称用户现场已修复；保留5个既有dirty。webClx_11的6526441有限时长交付已收到，本轮优先处理用户新报的真实消息故障，未用旧T4测试通过掩盖该缺陷。

- 定时消息修复发布进行中：候选052bd49含8a573b6，补普通命令不误锁队列与迟到回执自动只读核对。已提交部署请求232852-18d6ff5d6aa98fdc：scheduled_定向测试→release build→项目部署脚本；queued不等于成功，待源终端回调，不轮询。发布前仅将原5个无关dirty文件stash为914a5f142b50244787a0d97bc2b1fa87d9c9fa17，原二进制diff备份/tmp/webclx-scheduled-deploy-preserved.patch，SHA256 325e244a1412137db0fa52dc044799682775e75184ec073cfc04c08687ab24a3；untracked未动。回调无论成败均先核对结果和安装边界，在构建结束后apply该准确stash（不盲目pop），验证原diff哈希再清理保存项。成功后验证服务/产物身份，再经skill短定时消息对webClx_11做真实提交及ACK验收，核对唯一任务标记出现一次；不提前声称无需手动Enter的场景通过。

- 定时消息用户复报复核（2026-09-20）：用户确认普通终端Enter才让失败消息提交，故可靠性验收重新开放；此前工作终端confirmed可能受人工提交影响，撤回自动成功推断。新建隔离Codex0.155.1终端复验短消息、2613字符长消息、Working期间新消息，均自动进入原生用户记录，后两者有ACK；无人工补键，未复现用户现场失败。Working不等于工具运行已证实。未依据未复现的跨输入通道乱序假设修改产品或部署。证据见 evidence/20260920-scheduled-message-delivery/README.md；后续仍需定位用户失败时的TUI状态和按键路径，不将隔离成功等同彻底修复。

- 消息skill入口统一（2026-09-21）：全局两份旧入口仍走立即消息路径，已改为默认3秒定时，并完成Codex/Claude/Cursor/ZCode/WorkBuddy单skill同步及哈希核验。全局脚本实际隔离测试paste-1789920087601-6764自动提交、原生记录1次，无人工Enter；未取得业务ACK。保留用户现场故障开放状态，不以此次成功概括所有情形。测试终端已消失但删除返回500且restore记录残留，列为独立清理故障。证据见scheduled-message-delivery目录global-skill-*及两份可审查patch。

### 2026-09-21 消息入口补漏

- [x] 项目与内置消息 skill 默认短定时，默认参数行为测试 Python 8/8、内置 Node 2/2。
- [x] 内置技能发布及安装副本核验：001759-18d713b008774d61，SKILL.md/脚本逐字一致，健康和 HTTP 静态资源哈希通过；原五文件已恢复。
- [ ] 原用户“必须手动 Enter”故障仍待复现，不以隔离成功或 confirmed 单独判为修复。

## 2026-09-30 架构复核与收尾

复核报告：[2026-09-30 架构变更复核](../../archive/audits/2026-09-30-architecture-review.md)（基线 `d542136`，复核者 mousy-soprano，无性能测量）。

| 任务 | 内容 | 状态 | 负责人 |
| --- | --- | --- | --- |
| A | 原子写入边界修正 + 快照缓存 seq/attachment/限龄改造 | 已复核验证并提交 `f5106f2`（compile `235459-18da09ffe8cf3532`） | mousy-soprano |
| B | `process_exec` 运行时上限与 deadline（含非 Unix 无界） | 已复核验证并提交 `98a377b`（compile `000824-18da09ffe8cf3537`） | mousy-soprano |
| C | 投递账本：持久化不健康时 claim 失败、保留未知文件、确认单调 | 已复核验证并提交 `2835c70`（compile `003329-18da09ffe8cf3542`） | mousy-soprano |
| D | 最终候选部署：单一显式 `/data` target + artifact/install/running SHA-256 核验 | 已完成并核验 `005740-18da09ffe8cf3549`：候选 `769dfb30`、运行 PID `1117845`；artifact/installed/running 三份 SHA-256 一致 `86701c30…f267`，`/assets/app.js` 与仓库 `static/app.js` 一致（`182cc3c2…7e3b`） | mousy-soprano |
| E | 会话创建锁序：`state → start_lock` 改为先放 `state` 再阻塞等门并复核预约 | 已复核验证并提交 `769dfb30`（compile `004545`/`004910`；构建 `005446` 267/267；生产烟测 fixture `s9234` 通过并清理） | mousy-soprano |
