# 仓库自动检查入口

`scripts/repo-check.sh` 是统一只读检查入口：一次触发得到固定命令集，报告版本、12 位提交、每条命令的退出码、耗时和日志路径。检查只读，不含部署、发布或长压测；webClx 自身编译遵循编译队列规则，不在检查脚本里直接重建。

来源：`docs/archive/audits/astra_optimization_recommendations_2026-09-19.md` P1-2。

## 检查组

| 组 | 内容 | 边界 |
| --- | --- | --- |
| rust | `cargo fmt --all --check`、`cargo check --workspace`、`cargo test --workspace` | 仅在允许直接构建的环境执行；本机走编译队列（见下） |
| node | `find tests -name '*.test.mjs'` 全部 Node 契约测试 | 每条用 `timeout --signal=KILL` 包裹，单条上限默认 600s |
| browser | 固定冒烟清单：`terminal-session-switch-focus`、`workspace_project_icons`、`codex_status_compact` | Playwright 自包含（拦截路由 + 本地静态资源），无需真实服务与本机 token |

- 默认三组全跑；`--node` / `--browser` / `--rust` 单独触发，`--filter SUBSTR` 按路径过滤，`--list` 只打印命令集不执行，`--timeout SECS` 调整单条上限。
- 命令集确定性：node 按文件名排序、browser 按固定清单顺序，同一提交两次 `--list` 输出一致。
- 日志与报告：`${WEBCLX_REPO_CHECK_LOG_DIR:-/tmp/webclx-repo-check}/<UTC时间>-<12位提交>[-dirty]/`，含逐条日志、`report.json`、`report.txt`；整体退出码 0 仅当全部通过，失败清单同时打印并列出。
- 长驻进程测试由 `timeout --signal=KILL` 兜底；进程类测试本身仍须遵守 AGENTS.md 的 kill-on-drop/RAII 与短超时约定。

## 本机触发清单（编译队列边界）

- Node + 浏览器组直接跑，不涉及 cargo：

```bash
scripts/repo-check.sh --node --browser
```

- Rust 组必须走编译队列，不在本机直接构建：

```bash
bash /home/root/.codex/skills/webclx-compile-and-deploy/scripts/request-webclx-compile-api.sh \
  --project webClx \
  --project-dir /home/codes/tools/webClx \
  --command 'cargo fmt --all --check && cargo check --workspace && cargo test --workspace' \
  --skip-restart --skip-sync \
  --note "仓库检查 Rust 组：回调后核对退出码与日志路径。"
```

  worker 自动把 `CARGO_TARGET_DIR` 映射到 `/data/cargo-target/webclx-compile/`；返回 `queued: true` 后不轮询日志，以源终端回调为验证入口。
- 在允许直接构建的环境（如 CI runner）执行 Rust 组时，脚本会把未指向 `/data/cargo-target/` 的 `CARGO_TARGET_DIR` 自动改写为 `/data/cargo-target/local/webclx-repo-check` 并记录在报告中。

## 浏览器冒烟清单边界

固定清单只收已在本机验证通过且自包含的用例。以下类别不进默认清单：

- 需要本机 `X-WebClx-Local-Token` 或真实登录：`terminal-session-origin-isolation`、`font_settings_dialog`。
- 需要真实 tmux/PTY：`terminal-shared-viewport`。
- 对 live static 具体内容断言、易随运行时漂移：`terminal-session-cache`（当前即红）。

新增冒烟用例先单独跑绿，再写入脚本的 `BROWSER_SMOKE_TESTS`。

## 基线记录（2026-09-19，`b082075470d4` dirty）

- `--list` 两次输出一致；注入必失败临时测试后脚本退出码 1，报告列出失败项。
- browser 组 3/3 通过。
- node 组 181 项：135 通过、46 失败。失败主因是 live static 与已跟踪测试漂移（例：`terminal.html` 已不含测试断言的 `styles-terminal.css?v=20260912c` 缓存键），属既有状态，非检查入口引入；对齐测试或静态补丁另案处理。
- `cargo fmt --all --check` 在 HEAD 即红（`crates/settings_core/src/tests.rs` import 排序），同为既有状态。
- 首次全量报告存档：`docs/logs/repo-check/20260919T161039Z-b082075470d4-dirty/`（gitignored 运行产物，仅本机可追溯）。

## 验收对照

- 同一提交两次触发得到相同命令集：`--list` 确定性已验证。
- 关键行为失败能让检查失败：注入自测已验证；node 组内的 `terminal-session-selection-race`、`terminal-session-cache` 等回归测试在真实回归时会失败（审计中曾有实测）。
- 报告包含提交号、退出码、耗时、日志路径和测试统计：`report.json` / `report.txt`。
- 长驻测试超时后无遗留进程：每条 `timeout --signal=KILL`；本次全量运行无超时用例（退出码只有 0/1）。
- webClx 自身编译按规则排队：Rust 组只提供编译队列触发清单，检查脚本在本机不直接重建。
