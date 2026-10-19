# 终端性能基线

`tests/terminal-performance-baseline.py` 是独立 Playwright/Python 基线入口，不修改产品代码、现有测试或部署配置。真实测量必须提供 Playwright storage state（`WEBCLX_TEST_STORAGE_STATE`）；浏览器 context 与 `context.request` 共享同一登录现场，脚本不读取、构造或输出凭据。

## 会话、路径与清理

1. 先请求 `GET /api/settings`，以响应中的 `workspace_dir` 创建 fixture，不写死工作区路径。
2. 每个会话生成包含 `secrets.token_hex(8)` 的唯一 `owner_key`；创建响应必须回读路径和 owner。
3. 清理只删除路径和 `owner_key` 完全匹配的 fixture；HTTP 失败、归属不匹配、删除后仍存在都会汇总为 `CleanupFailed`。清理在 context/browser 关闭前、业务异常后都会执行。

真实 cold 测量按样本创建并释放两组 fixture。它们是有主测试终端，普通 `origin=normal`，路径是当前工作区根（默认 `/home/codes`），每个样本通常短暂显示为 `codes_1` / `codes_2` 后由 `terminal-performance-baseline` 删除。这不是工作现场恢复或幽灵会话泄漏；`?all=true` 与“工作区终端下拉框显示全部目录终端会话”会按设计展示它们。隔离这类管理/测试会话需要另外建立显式可见性策略，不能与 owner 清理边界混用。

## 场景与就绪条件

指标范围是 **buffer-ready**：目标 `state.activeSessionId` 和 context、WebSocket 打开、回放/输出写入排空、xterm 挂载且行列大于 0，并且本场景唯一证据出现在可视 xterm buffer 中。canvas 像素渲染未单独验证，不能称为“可见首屏渲染”。

1. `first_connection`：向 first 发送唯一证据后打开终端并等待其 buffer-ready。
2. `session_switch`：向 second 发送唯一证据后切换并等待 second 的证据。
3. `output_switch`：向 second 发送另一条唯一证据后切回 first，再切换到 second 并等待该证据；每次输出证据都不同。

输出命令只让证据出现在命令结果中，命令文本本身不包含完整证据。默认每场景 30 个记录样本，统一 `time.monotonic()`，每条样本和 summary 立即输出 JSONL；样本失败也会保留记录与 summary，最终退出码非 0。

## 冷热边界

冷热只描述本脚本的浏览器进程和 fixture 准备，不控制或宣称操作系统页缓存：

- `cold`：启动新的 Chromium 进程并创建全新 fixture，不记录 warmup。
- `warm`：同一浏览器内先执行 `--warmup-samples` 次不记录的 fixture/输出循环，再记录样本。

每次运行输出 `cache_scope=browser_and_fresh_fixtures` 和 `os_cache_controlled=false`，避免把标签误当成系统缓存控制。

## 使用

```bash
python3 tests/terminal-performance-baseline.py --help
python3 tests/terminal-performance-baseline.py --dry-run --samples 30 --warmup-samples 3 --cache warm
WEBCLX_TEST_STORAGE_STATE=/path/to/playwright-storage.json \
  python3 tests/terminal-performance-baseline.py --samples 30 --cache cold
```

环境变量 `WEBCLX_TEST_BASE_URL` 和 `WEBCLX_TEST_CHROMIUM` 可覆盖默认地址和 Chromium。脚本不构建、不部署、不启动持久子进程；输出命令有界，不执行共享环境批量压测。

## 故障注入验收

`tests/terminal-performance-baseline.test.py` 使用临时合法 storage state 和全 mock 浏览器/网络，不调用真实 `main` 启动浏览器。覆盖 API 失败、样本失败非零、部分初始化失败、清理失败/归属不匹配不删除、唯一证据与场景对齐、冷热边界和缺 storage state 的非零退出。

```bash
python3 tests/terminal-performance-baseline.test.py
```

入口还须通过 `git diff --check`、`--help`、`--dry-run`。真实测量必须等待管理授权，在隔离目标上运行，并在完成后确认无 fixture 残留；当前结果不代表性能优化已经完成。

## B3 初步回归阈值

B3 阈值只适用于同版本、同服务身份、同浏览器和视口、同 buffer-ready 口径的新测量；环境或采集定义变化时须另建基线。正式比较前先确认环境可信度：warm 两轮同场景 p95 变异系数不超过 20%，且各轮每场景至少 30 个有效样本。

| cache | scenario | p95 upper bound (ms) | failure rate |
| --- | --- | ---: | ---: |
| warm | first_connection | 1400 | < 5% |
| warm | session_switch | 1050 | < 5% |
| warm | output_switch | 550 | < 5% |
| cold | first_connection | 1900 | < 5% |
| cold | session_switch | 1300 | < 5% |
| cold | output_switch | 550 | < 5% |

每轮可接受条件：每个 warm 场景 p95 与失败率达标；cold 也须逐项达标，且与 warm 比较前确认两轮 warm CV ≤ 20%。所有场景必须有精确 3 份 summary 和可追溯逐样本 JSONL。失败率越界或样本数不足优先判测量/环境问题；p95 越界在资源无泄漏、口径一致且环境可信时判性能回归。

历史 B2 实测输入为 warm p95 963.184–1121.899 / 758.586–854.421 / 407.104–429.422ms，cold p95 1100.899–1569.054 / 862.943–1066.346 / 367.474–423.072ms；对应 CV 分别为 10.765%/8.402%/3.773% 与 24.797%/14.910%/9.946%。首连接和会话切换阈值是同 cache 观测最大值加 20%；output_switch 因 warm/cold 分布重叠使用统一 550ms。cold 首连接初期 CV 24.797% 超过 20%，因此 cold first_connection 只作单独报警线；后续须以连续 4 组 CV ≤ 20% 的滚动数据复核后再用于趋势判断。

滚动更新：同一运行身份每新增两组后并入滚动窗口并保留最新四组；阈值更新为窗口最大值加 20%，warm 三项下限分别为 900/700/350ms。连续 4 组同 cache CV > 20% 时停止比较并先扩轮排查环境，不以噪声更新阈值。

## B1 复核修订

2026-09-20 复核发现并已修正：`run` 通过生产 `output` 回调输出 stdout JSONL；任一样本失败仍输出全部三份 summary 并使进程非零退出。warm 等待它刚发送的同一条 evidence。输出命令用两个 `printf` 参数拼接完整证据，回显命令不包含完整 marker。`output_switch` 先回到 first，再向 second 发送四次有界唯一输出请求，最后切到 second 等待该证据；这测的是有界输出后的切换，不是持续输出负载。cold 的每个记录样本都创建新 browser/context 和新 fixture；warm 只使用一个 browser/context，并先执行未记录 warmup。清理逐项捕获 transport/status/recheck 失败，owner 响应必须严格相等，不匹配的已登记 fixture 不删除但会报告清理失败。

行为测试在 `async_playwright` 边界全 mock，不启动真实浏览器；`StatefulPage` 维护导航、选中目标、已发送证据和失败模式，`wait_buffer_ready` 不接受“永远 ready”。测试捕获 stdout JSONL、精确三份 summary、指定失败原因、warm 的真实 goto/select、回 first 后等待 first 证据、output 前的有界重复输出、cold 每样本新 browser/context/fixtures、清理先于关闭、单点关闭异常仍继续关闭、逐项 delete transport 失败和 owner 严格相等；`main` 只对缺 storage state / dry-run 做边界 mock，不执行真实测量。指标仍限定 buffer-ready，冷热不控制系统缓存。`launch` 在受保护生命周期内创建；真实 mock main 覆盖成功、样本失败和 launch 失败的 stdout/退出码。cold 每轮资源独立 try/finally；释放前先解除外层引用，旧资源只清理/关闭/释放一次，失败后不复用旧 request/context，并覆盖第二次 launch/new_context 失败。行为测试断言每个 browser/context close 恰好一次、request dispose 恰好一次，关闭后再调用必须报错。真实 mock main 覆盖成功、样本失败和 launch 失败；样本失败保留真实 run 并只让状态化 page 选择失败，断言失败 sample、精确三份 summary 和退出码。
