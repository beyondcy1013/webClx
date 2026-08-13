# 编译编排器可靠性

## 目标

`POST /api/build/compile` 与 `POST /api/build/deploy` 返回 `queued: true` 后，请求身份必须保持唯一且不可被其它项目覆盖；若 worker 未能启动，API 必须直接失败，不能留下待处理请求或等待一个不会到达的回调。

## 日志所有权

- `project_dir` 只决定编译和安装命令的工作目录，不是编排器运行日志目录。
- 详细构建日志和安装审计报告统一保存在 webClx 的
  `.webclx-compile-queue/runs/<run-id>/outputs/`，并随 run 走现有保留期清理。
- worker 不得在客户端项目中创建 `docs/logs` 或要求客户端为编排器日志维护
  `.gitignore`。完成回调必须给出集中日志和审计报告的绝对路径。
- 旧版本散落的日志归档到 `.webclx-compile-queue/legacy/<原 /home/codes 相对路径>`，
  不随 run 保留期清理。使用 `scripts/migrate-compile-api-logs.sh` 先 dry-run，再加
  `--apply`；脚本会保留来源层级、改写尚存 run 的 `.path` 引用并生成哈希清单。
  不要手工搬移，否则状态页和历史回调可能继续引用旧路径。

## 2026-08-12 跨 worktree 陈旧部署覆盖

- 现象：同一项目的多个 worktree 并行构建并部署到相同 `audit_paths`。部署脚本虽然
  串行安装，但较旧请求可能较晚完成编译并最后安装，导致已经成功部署的新版本回滚。
- 根因：worker 的部署资源锁使用 `project_dir`，不同 worktree 因路径不同而没有共享
  协调状态；队列也没有记录哪个请求已成功安装到共同运行目标。
- 修复：构建仍按各自 Cargo target 并行；安装阶段对每个规范化后的 `audit_path`
  按固定顺序共享锁，路径集合只要有交集就属于冲突部署。成功安装在释放目标锁前写入
  标记。较旧请求取得锁后若发现同目标已有较新成功请求，
  跳过安装并回调“被较新成功请求取代”。较新请求失败时不产生成功标记，较旧请求仍可
  安装，避免两次部署都落空。
- 回归：`tests/compile-worker-deploy-supersession.test.mjs` 使用两个临时 worktree 复现
  “新请求先成功、旧请求后完成”的顺序，并验证最终运行目标保持新版本。

## 2026-08-12 资源等待期间持续合并

- 现象：同一 Cargo target 已有构建运行时，后续相同编译请求会被不同 worker 提前领取
  成多个 run，然后依次等待同一资源锁，最终对同一工作树重复编译多次。
- 根因：请求领取发生在构建资源锁之前；首次领取只合并当时仍位于 `requests/` 的完整
  同规格请求，后续到达的请求无法加入已经处于 waiting 状态的 run。
- 修复：每个完整构建规格只有一个 waiting owner。owner 在领取前持有非阻塞合并锁，
  等待 Cargo target、项目部署锁和全局并发槽期间继续拥有该规格；取得全部资源后，
  在 claim lock 内再次吸收所有同规格请求，然后原子释放合并锁并开始执行。
- 语义：已经运行的命令不会被中途取消；它后面的相同规格请求持续收敛为一个 follower
  run。该 run 真正开始时读取当前工作目录，因此使用最后一次合并后的最新工作树。
  命令、环境、安装参数、审计路径或必需产物不同的请求保持独立，不能伪合并。
- 回归：`tests/compile-worker-pending-coalescing.test.mjs` 让一个请求占住共享 Cargo
  target，再分三批提交同规格请求；验证三批进入同一 run、只执行一次，并读取第三批
  到达后的工作树内容。

## 2026-07-15 同秒请求覆盖事故

- 现象：stockOne 请求 `061032` 已返回入队，但一直没有回调；运行记录中只有同秒提交的 stockScreener 请求。
- 根因：`build_request_id()` 只生成 `HHMMSS`。stockOne 先启动 `webclx-compile-061032`，随后 stockScreener 覆盖同名 `requests/061032.json`，并因同名 systemd unit 已存在而启动失败。前一个 worker 最终读取并执行了被覆盖后的 stockScreener 内容。
- 修复：请求 ID 改为 `HHMMSS-<16位单调唯一后缀>`；请求文件、API `request_id` 和 systemd unit 共用同一 ID。worker 启动失败时删除本请求文件并传播 HTTP 500，queued 通知只在启动成功后发送。
- 影响范围：所有通过 webClx 编译/部署 API 并发提交的项目；不影响已有运行记录格式，时间仍可从 ID 前缀直接读取。
- 回归验证：`cargo test request_id` 覆盖格式和 1024 次快速生成不重复；`cargo test worker_launch_failure` 覆盖启动失败清理与错误传播。
- 线上验证：2026-07-15 06:51 CST 同秒并发提交 noop 请求 A/B，分别得到
  `065147-18c249c3d623a91f` 与 `065147-18c249c3d623a91e`；两个独立 systemd unit 均启动，
  worker 在同一轮合并 2 个请求后分别生成日志、审计和终端回调，0 个文件差异。

## 2026-07-15 运行状态过早结束事故

- 现象：页面同时出现多个“待合并请求”和“当前没有正在执行的构建工作”，但 worker 进程实际仍在编译；开始时间和结束时间还可能前后颠倒，页面无法显示当前命令或 Cargo 进度。
- 根因：一轮 run 会串行执行多个去重后的 spec，worker 每完成一个 spec 就写一个 `status-<key>`；后端只要读到任意 status 就把整轮判为成功或失败。时间字段通过未排序的 `read_dir().find_map()` 随机取第一个文件，页面又只在进入页签或手动点击时刷新。
- 修复：终态必须满足 `status-*` 数量覆盖 `specs.jsonl` 的全部 spec，或存在显式 `run-finished-at`；运行时间取最早 `started-*` 和最晚 `finished-*`。worker 在命令开始前写 `run-started-at`、`started-*` 和原子 `progress.json`，全部 spec 结束后才写 `run-finished-at` 并移除实时进度。
- 实时进度：worker 强制 Cargo 在非 TTY 日志中输出原生 `N/M` 构建单元进度，进度文件记录当前项目、阶段、命令序号、Cargo 分子/分母、当前 crate 和当前日志。状态页每 2 秒只请求 `include_history=false`，不会反复加载完整历史。
- 兼容约束：`progress.json`、`install-*.json` 等运行产物不能计入原始请求数；旧 run 没有显式完成标记时，仍可用完整的 status/spec 数量判断终态。
- 回归验证：Rust 测试覆盖部分完成仍为 running 以及最早/最晚时间聚合；Node worker 测试在模拟命令仍运行时读取 `37/120: tokio`，并验证结束标记和进度清理；前端测试覆盖实时进度渲染和轻量刷新入口。

## 2026-07-15 成功 toast 后终端回调延迟事故

- 现象：构建完成后浏览器立即显示成功 toast，但终端回调约 32 秒后才写入来源终端。
- 证据：`20260715T111213-1349165` 在 `11:14:30.820` 完成，来源终端保持 busy；worker 到 `11:15:01.519` 才记录终端未就绪，回调在 `11:15:02.674` 写入终端输入历史。
- 根因：worker 先发 toast，再调用 `wait_terminal_ready()`，要求来源终端同时满足 connected 且非 busy，最多轮询 30 秒；超时后仍会发送回调，因此等待既不能保证空闲投递，又固定制造延误。
- 修复：删除回调前的 busy 等待，toast 后立即调用终端消息 API。终端消息本来就是让活动中的智能体接收编译结果并继续任务的 join point，不应以智能体正在工作为阻塞条件。
- 回归验证：`tests/compile-callback-delivery.test.mjs` 明确禁止 toast 与终端消息之间出现 `wait_terminal_ready`，同时保留 delivery id 和提交确认契约。

## 2026-07-19 Cargo target 目录统一

- 目标：同一 Cargo workspace 的直接 `cargo build` 与 webClx compile/deploy
  请求必须复用同一物理 target 树，避免项目内 `target/` 与 API 私有缓存双占空间。
- 所有权：Cargo workspace 自己解析 `target_directory`。compile worker 不再注入
  私有 `CARGO_TARGET_DIR`；显式请求环境仍可覆盖 Cargo 配置。
- 存储：`scripts/unify-cargo-targets.sh` 为每个 workspace 保留正常的
  `<workspace>/target` 入口，并将其链接到
  `/data/cargo-target/webclx-compile/cargo-target/<workspace>-<canonical-path-hash>`。
  项目已有的显式目录（例如 `/data/cargo-target/stockScreener`）继续作为权威目录。
- 迁移安全：脚本与 worker 共用 `.webclx-compile-queue/worker.lock`；每棵旧缓存
  经 rsync 合并和 checksum dry-run 验证后才清空，脚本可重复运行。
- 部署：two-stage deploy wrapper 通过
  `cargo metadata --format-version 1 --no-deps` 定位产物，并兼容
  `<target-dir>/<target-triple>/release`，不再重建旧 worker 私有缓存路径。
- 结果：遍历 `/home/codes` 得到 50 个有效 Rust workspace，全部复核为
  `action=already-unified`。迁移期间到达的 API 请求在 worker 锁上等待，释放后正常
  接续，未遗失。
- 验证：

```bash
bash scripts/unify-cargo-targets.sh
node tests/unify-cargo-targets.test.mjs
node tests/compile-worker-cargo-target.test.mjs
node tests/compile-deploy-target-path.test.mjs
```

### Linked worktree 隔离与编译缓存复用

- Cargo fingerprint 和部分增量产物含绝对源码路径。不同分支的 linked worktree 直接写
  同一 target 会让同名 path crate 复用其它分支生成的 rmeta，表现为源码明明存在导出，
  下游却报告 unresolved import，或为了纠正 fingerprint 反复重编大量 workspace crate。
- 编译 worker 在命令启动前识别 Git linked worktree。若请求未显式设置
  `CARGO_TARGET_DIR`，且 `<worktree>/target` 缺失、为空或是符号链接，则改为按 worktree
  路径稳定哈希的独立 target，并让 `<worktree>/target` 指向该目录。已有非空实体 target
  保持不变，主工作树仍使用项目通过 `cargo metadata` 声明的 target。
- 若请求未显式设置 `RUSTC_WRAPPER` 且系统存在 `sccache`，worker 自动启用它；默认缓存
  目录为 `<work-dir>/sccache`，上限 10G。相同缓存配置使用稳定短路径
  `SCCACHE_SERVER_UDS`，并默认 `SCCACHE_IDLE_TIMEOUT=0`，避免每个规格因独立 `TMPDIR`
  连接到不同 server。调用方显式 server 配置优先。
- compile worker 由 `systemd-run --collect` 启动为一次性 transient unit。若由 worker 内的
  首次编译自动拉起 sccache daemon，该 daemon 会留在 worker cgroup，并在 unit 收尾时被
  systemd 清理；仅固定 socket 或设置 `SCCACHE_IDLE_TIMEOUT=0` 不能跨请求保留 server。
  当前主机用启用的 `webclx-sccache.service` 独立持有 daemon：unit 使用 `Type=oneshot`、
  `RemainAfterExit=yes`、`ExecStart=sccache --start-server` 和
  `ExecStop=sccache --stop-server`，读取 `/etc/default/webclx` 中相同的 `SCCACHE_DIR`、
  `SCCACHE_CACHE_SIZE`、`SCCACHE_SERVER_UDS` 与 `SCCACHE_IDLE_TIMEOUT`。webClx unit 通过
  drop-in `Wants/After=webclx-sccache.service` 保证启动顺序。`Type=simple` 不适用：
  `sccache --start-server` 会 fork 后退出，systemd 会立刻把 unit 判为 inactive。
- linked worktree 默认 `CARGO_INCREMENTAL=0`，避免生成不可由 sccache 复用的大体积
  incremental 树。sccache 0.16/0.17 的 Rust key 仍直接包含编译 `cwd`，因此不同绝对
  worktree 路径之间不会产生 Rust cache hit；独立 target 解决的是正确性和并发污染，
  sccache 加速主要发生在同一绝对工作区路径的重复构建及可缓存依赖 crate 上。
- 每个规格仍使用独立 `TMPDIR`，目录名只包含唯一 run ID 与规格序号，不拼接完整规格
  哈希，确保其它使用临时 Unix socket 的工具低于 Linux `sun_path` 的 108 字节上限。
- worker 可能在一个进程内串行执行多个去重规格。每个规格的 `compile_environment` 和
  自动注入的 Cargo 环境在下一规格开始前恢复为 worker 初始值，禁止前一项目的 PATH、
  toolchain、target 或 sccache 配置泄漏到后一项目。
- 不同 worktree 的独立 target 可以并发；同一 target 仍由既有资源锁串行。部署阶段继续
  受项目和审计目标锁保护。
- 回归：`tests/compile-worker-cargo-target.test.mjs` 使用真实 Git worktree 验证默认隔离、
  已有非空 target 保留、显式环境优先和稳定 sccache server 配置；并发测试继续验证资源
  锁语义。运行验证应再确认 `systemd-cgls /system.slice/webclx-sccache.service` 中 daemon
  PID 在 compile worker 退出后不变，并用同一 cwd 的两次可缓存 Rust lib 编译观察第二次
  `Cache hits (Rust)` 增加。

## 2026-07-20 部署审计误扫根目录事故

- 现象：stockScreener 回放自身仅耗时 `13.915s`，webClx deploy worker 却耗时
  `766s`；安装审计候选包含 `//`、URL 片段和整个项目目录。
- 根因：worker 深入扫描了 compile/验证脚本正文，路径正则把 jq 的 `//` 运算符、
  `http://` 的双斜杠和脚本中的项目根常量当成部署输出；安全检查只拒绝精确的
  `/`，而 Linux 上 `//` 同样解析为根目录。命令前后快照因此各遍历一次根文件系统。
- 修复：部署审计不再解析 compile/验证脚本正文，只从安装命令、显式
  `audit_paths`、Cargo 二进制和已运行 webClx 路径推断输出；候选过滤双斜杠和项目
  根目录，快照前再通过 `realpath` 拒绝根目录、伪文件系统及其符号链接别名。
- 回归验证：`node tests/compile-worker-audit-paths.test.mjs` 覆盖 `//`、URL、项目根、
  根目录符号链接和合法安装输出；`bash -n` 及既有 Cargo target、实时进度、回调
  契约测试继续通过。

## 2026-07-21 跨项目编译兼容性收敛

- 审计范围：Codex session 中有 456 条失败回调，覆盖 79 个会话和 66 个项目显示名；
  队列保留的 1208 个执行 spec 中有 366 个非零退出。至少 217 条回调属于预期 RED、
  test、lint 等验证失败，不能直接解释为编译器失败。
- 环境丢失：后端请求 JSON 已保存 `compile_environment`，但 worker 的去重 spec 未保留
  该字段，导致项目指定的 `PATH`、`CARGO_HOME`、`RUSTUP_HOME` 等从未进入实际命令。
  worker 现在保留环境并将其纳入 spec 身份；同项目同命令但不同工具链环境不会互相去重。
- 动作分类：`/api/build/compile` 恢复为真正的纯编译入口，清除遗留安装字段，只推断
  Cargo、npm 或 make 构建命令，绝不推断 `rebuild-and-deploy.sh`。纯编译 wrapper 和
  两阶段 wrapper 的第一阶段均使用该入口，不再以 deploy + noop 模拟编译。
- 入队校验：deploy 入口使用 shell argv 解析部署脚本路径，并在项目目录中验证相对路径
  或验证绝对路径；脚本不存在时立即返回 4xx，不再生成一个必然以 127 失败的队列任务。
- 诊断语义：三个 wrapper 在解析来源终端前先检查 webClx API；连接失败明确报告 API
  unavailable，只有 API 可用但身份确实无法定位时才报告来源终端名错误。
- shell argv 防错：stockOne 请求 `111849-18c41d6fffa0fea1` 把命令提交为
  `["bash","-lc","bash","scripts/build-windows.sh"]`，导致 `-lc` 只执行空的子 shell，
  后续脚本只是位置参数。三个 wrapper 与后端现同时拒绝缺少命令字符串及这种拆参形式；
  shell 语法必须通过一个完整字符串提交。
- 产物门禁：请求可声明 `required_artifacts`。worker 在编译成功后、安装前按项目目录
  解析相对路径并检查存在性；缺失时该 spec 失败且不执行 install，字段同时进入去重身份。
- 审计清洗：安装脚本中的 `/../../` 与 `/c` 等伪候选在快照前做词法规范化过滤，
  同时保留尚不存在但合法的预期部署输出，以便安装后报告 `created`。
- 回归验证：`tests/compile-api-compatibility.test.mjs` 覆盖端点、payload、三类 wrapper
  和断线提示；`tests/compile-worker-environment.test.mjs` 真实执行两套环境并验证不会去重；
  Rust 单测覆盖部署脚本存在性和纯编译不选择部署脚本。

本轮不把外部 SDK、Android/Windows 交叉工具链、非交互签名凭据或项目自定义打包流程
隐藏在协调器猜测中。这些差异应由项目构建/部署脚本表达，Codex 通过
`webclx-compile-and-deploy` skill 选择 wrapper、提交完整 argv、声明必需产物和审计路径；
不为工具链变量、项目相对路径或项目产物增加协调器 profile。

## 2026-07-21 按 Cargo target 并发编译

- 原因：旧 worker 在整个队列生命周期持有一把独占 `worker.lock`，不同项目即使使用
  不同 Cargo `target_directory` 也只能串行。
- 调度：`worker.lock` 现在是共享维护锁；缓存迁移继续独占该锁。worker 通过短时
  `claim.lock` 原子领取同一去重规格的请求，再按解析出的 Cargo `target_directory`
  获取资源锁。非 Cargo 项目按项目目录加锁。
- 部署：deploy 在构建资源锁之外再按项目目录加锁，避免同一项目的不同 target 并发
  执行安装或服务重启。
- 并发：不同资源可并行执行，但必须先取得全局并发槽。设置项
  `compile_max_concurrency` 位于“设置 → 系统”，范围 `1..=32`，默认 `5`。
- 状态：轻量状态接口返回全部活动 run，而不是只返回最新 run。
- 回归验证：`tests/compile-worker-concurrency.test.mjs` 覆盖不同 target 并行、相同
  target 串行和全局上限；`tests/compile-concurrency-settings.test.mjs` 覆盖设置页与
  API/worker 传递链。

## 2026-07-31 部署审计跨入 NAS 挂载事故

- 现象：lyyData 重启请求 `230458-18c7663aa56c2817` 长时间停在 `preparing`，编译命令
  `/usr/bin/true` 和安装脚本均未开始执行。
- 根因：安装脚本创建 `/mnt/lyydata`，worker 因而把该目录加入部署审计。目录快照的
  `find` 未限制文件系统边界，继续进入 `/mnt/lyydata/jiguang` 的 CIFS 挂载并扫描用户
  数据；`SNAPSHOT_MAX_FILES` 只跳过最终哈希，不能避免此前的多次全量遍历。
- 修复：所有递归目录快照使用 `find -xdev`，不再跨入嵌套挂载；审计目标本身是挂载点时
  只记录挂载根元数据，明确不递归读取其内容。部署审计仍能判断目标路径是否存在，又不
  会把外部存储容量和内容纳入安装差异。
- 回归验证：`tests/compile-worker-audit-paths.test.mjs` 锁定所有目录遍历的 `-xdev` 边界，
  并用 `/proc` 挂载点验证 metadata-only 快照；`bash -n` 验证 worker 脚本语法。

## 2026-08-12 显式审计路径仍被脚本推断扩张

- 现象：lyyData 部署已完成且运行二进制与构建产物哈希一致，但来源终端长期显示
  `编译中`；run 日志停在安装前快照并出现 `sort: Broken pipe`，没有写终态或发送完成回调。
- 根因：请求已明确提交唯一 `audit_paths`，worker 仍同时扫描安装脚本，把数据目录、挂载点、
  配置目录和脚本自身扩张成审计候选。无关目录快照异常使 worker 在安装命令前退出，请求生命
  周期因此没有收敛。
- 修复：非空 `audit_paths` 是部署审计的完整白名单，只规范化并校验这些路径；仅当请求没有
  提供显式路径时，才从安装命令、Cargo 二进制和 webClx 运行目标推断候选。
- 回归验证：`tests/compile-worker-audit-paths.test.mjs` 实际执行候选收集函数，确认包含大量路径
  引用的部署脚本也不能扩张显式审计集合。

## 排障证据

无回调时只检查一次以下边界，避免盲目轮询或重复入队：

1. `/api/build/compile/status` 中是否存在对应 pending/run。
2. `journalctl -u webclx.service` 是否记录 queued、systemd unit 名和 worker 启动失败。
3. `.webclx-compile-queue/requests/` 或对应 run 目录中的 JSON 是否仍属于原项目和终端。
4. systemd transient unit 是否实际创建。

若故障属于 wrapper、skill 或 webClx 编排器，应直接修复对应所有者并补回归测试；编译 API 自身损坏期间使用项目文档中的手工 fallback，修复部署后只重试原请求一次。

## 相关文件

- `src/compile_service.rs`
- `docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh`
- `.codex/skills/webclx-compile-and-deploy/SKILL.md`
