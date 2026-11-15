# Codex 预设切换导致 provider 与 Base URL 混用

- 时间：2026-08-04
- 状态：已修复、部署并完成真实预设切换验收
- 影响范围：`webclx run`、终端内切换预设、已有 Codex 会话的中断恢复

## 现象

使用 GLM 预设恢复已有 Codex 会话时，启动界面和历史上下文已经显示 `GLM-5.2`，但首次请求仍发往切换前 GPT 预设的 Base URL：

```text
http://192.168.3.2:18381/v1/responses
Model "GLM-5.2" is not supported by any configured account in this group
```

三个失败 worker 的原 rollout 都只有一条用户任务、没有 assistant 回复。这证明任务已经送达，失败发生在首次模型请求，而不是 GLM 套餐额度、任务派发或终端消息提交阶段。

## 根因

旧 `webclx run` 流程在应用目标预设并启动 Codex 子进程后，只等待固定 `750ms` 就释放租约并恢复原配置。

这个实现错误地把“操作系统进程已经 spawn”当成“Codex 已经读取并固定全部模型配置”。实际启动顺序并非原子完成：Codex 可以先读取历史或顶级 `model`，稍后才初始化 `model_provider`、provider Base URL 和凭据。于是固定延时窗口内可能形成：

```text
目标预设 model + 恢复后的旧 provider/Base URL
```

延长固定等待时间不能消除竞态，只会降低复现概率。只在下游把“不支持模型”改成重试、fallback 或额度错误也不能修复上游混配。

## 运行中 Codex 如何让新配置生效

已经完成初始化的 Codex 进程应视为固定使用启动时读取的 provider、Base URL、wire API 和凭据。修改 `~/.codex/config.toml` 或 `auth.json` 只为后续新进程提供配置，不能把旧进程可靠地热切换到另一个 provider。

要在同一终端、同一对话上下文中切换预设，必须启动新进程：

1. 精确解析目标预设，禁止按相似名称任意选择。
2. 在同一个串行写入操作中更新 `auth.json`、`config.toml`、代理预设身份和项目级 `.codex/config.toml`。
3. 回读并核对 model、provider、Base URL、wire API、凭据身份和所有受管理覆盖项；任一不匹配则停止。
4. 中断旧 Codex 进程并等待它实际退出，不能向仍运行的进程宣称切换成功。
5. 使用原 thread ID 启动新的 `codex resume`；历史只恢复对话和工作状态，模型使用刚写入配置中的当前值，并通过官方 `--model` 参数覆盖历史 rollout 模型。
6. 以新 rollout 或首个真实模型请求/回复确认新进程已经工作，再向调用方报告切换完成。

当前项目策略是让选中的预设保持为共享配置的当前值。不得在固定延时后自动恢复旧预设；否则新进程仍可能在延迟初始化时读取到旧 provider。将来若需要真正的临时预设执行，必须设计可验证的每进程配置隔离或首请求握手，不能重新引入时间猜测。

由于多个 `webclx run` 仍写同一份真实共享配置，启动并行不等于配置交接可以并行。调度器必须逐个启动 worker，并等前一个 worker 出现真实 Codex TUI banner 后再启动下一个；到达该状态后，前一个进程已经固定自己的 provider/Base URL，后续 worker 才可以安全改写共享配置。worker 的实际任务执行仍可并行。

## 正确的 API 语义

- 预设 `apply` 成功表示完整配置已经写入并通过回读校验，不是“已排队、稍后也许生效”。
- apply 因旧租约返回 `deferred=true` 时，不得继续启动 Agent；调用方必须明确失败或等待实际 apply 完成。
- 已有会话的切换 API 必须组合“apply + interrupt + resume + rollout 验证”，而不是只修改磁盘文件。
- `resume` 和 `fork` 必须从本次 apply 后的实际配置文件读取模型，不能从 apply 前缓存、历史 rollout 或预设列表猜测。
- 项目祖先目录中的 `.codex/config.toml` 优先级高于用户配置，apply 必须携带终端 cwd 并同步已确认的项目级覆盖文件。
- provider/Base URL 混配与 `429` 套餐额度是两类故障：前者检查请求目标和配置来源，后者按 GLM 五小时额度窗口及 webClx 自动重试处理。

## 自定义模型被误报为 GPT-5.4 退役

修复 provider/Base URL 竞态后，GLM worker 一度停在 Codex 的
`Choose how you'd like Codex to proceed` 页面，并提示 GPT-5.4 已不可用。实际活动配置仍是
`model=GLM-5.2`，不是 webClx 或调度器选择了 GPT-5.4。

根因在项目级 `.codex/model_catalog.json`：webClx 创建自定义模型条目时复用了官方
GPT-5.4 模板，但没有清掉模板中的 `upgrade` 字段，导致 GLM 条目错误携带
`{"model":"gpt-5.6-terra"}`。Codex 据此把自定义 GLM 误识别为等待迁移的旧官方模型。

修复规则：

- 新建 WebClx 自定义 API 模型条目时显式写入 `upgrade: null`。
- 刷新已有、且带有 WebClx 自定义模型描述的条目时清理陈旧 `upgrade`。
- 官方模型条目的升级元数据保持原样，不能全目录无差别清除。
- 调度器遇到模型迁移选择页时报告 model-catalog drift，禁止自动选择 GPT-5.6 Terra、切换 provider 或重复投递任务。

部署后从 `/home/codes/stock` 执行
`webclx use api api-1783324599538`，项目级配置写入 `model=GLM-5.2`、智谱 provider 和本机中继
Base URL；同目录 `model_catalog.json` 中 `GLM-5.2.upgrade=null`。随后 P1-B worker
`s3549` 一次启动成功并通过 rollout 确认收到唯一任务，证明迁移提示不再阻断 GLM 启动。

## 防复发验证

代码回归至少覆盖：

- `webclx run` 不包含固定启动延时、租约释放后恢复旧配置或其它时间猜测。
- 目标预设和项目 cwd 在 Agent 启动前完成 apply。
- `deferred=true` 时 Agent 没有 spawn。
- `resume/fork` 的 `--model` 来自 apply 后实际生效的配置文件。
- 已有终端切换后保留原 thread ID，但产生新的 Codex 进程和新的 rollout 请求。
- 慢启动场景下，新请求的 model、provider 与 Base URL 始终属于同一个预设。

## 最终验收

- `tests/webclx-run-preset-atomicity.test.mjs` 4/4 通过，覆盖 cwd 同步、apply 后启动、`deferred=true` 禁止 spawn 和退出后不恢复旧配置。
- 使用项目内临时假 Codex 做端到端验证：`webclx use` 切换 GPT 时同时更新用户级与 cwd 项目级配置；`webclx run` 切换 GLM 并恢复原 thread 时实际参数包含 `resume --model GLM-5.2 <thread-id>`；子进程退出后 GLM 仍保持为当前共享预设。临时目录已清理。
- 验收结束后恢复活动 API 预设 `api-1783646277593`：`model=gpt-5.6-sol`、`provider=webclx_api`、`base_url=http://192.168.3.2:18381/v1`、`wire_api=responses`。
- 初次修复部署请求 `201843-18c889a330fc26fc` 成功；Cargo 版本来源修正后的最终部署请求 `202732-18c899a4468f3075` 成功。
- 模型目录回归测试先在请求 `212350-18c89c23266d3d01` 精确红灯，再由请求 `212545-18c89c23266d3d02` 验证 8 项聚焦测试通过；部署请求 `212649-18c89c23266d3d03` 成功。
- 最终运行验证：`webclx.service=active`，`/api/system/info` 返回 `version=1.8.2`；构建产物、安装二进制和运行中 `/proc/<pid>/exe` 的 SHA-256 均为 `da5c399a20782bb012736b4751ff8c527ee2e68c729ba2fd21b3ec724e1516a0`。

## 相关文件

- `src/cli.rs`
- `src/auth/apply.rs`
- `src/codex_launch.rs`
- `src/terminal.rs`
- `crates/auth_core/src/storage.rs`
- `crates/auth_core/src/tests.rs`
- `tests/webclx-run-preset-atomicity.test.mjs`
- `docs/codex/tasks/model-preset-switching.md`
- `docs/codex/tasks/api-preset-routing-boundaries.md`
