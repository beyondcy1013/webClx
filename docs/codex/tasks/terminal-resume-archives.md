# 终端恢复归档与原名称

## 稳定行为

- 终端工具 → “经验落盘”复用“落盘并结束”的运维 skill 落盘与同步要求，但去掉自毁关闭命令，仅向点击时的当前终端发送消息并提交；不隐藏、不切换或关闭终端。提示词统一由 `static/terminal-experience-save.js` 的 `terminalExperienceSavePrompt` 维护。

- “结束”先等待隐藏接口成功、移出活动列表并切换到其他可见终端，再调用关闭接口；隐藏失败不继续关闭，关闭失败则保留隐藏会话供恢复。“落盘并结束”仍在隐藏后由 Agent 完成落盘再自行关闭。

- 恢复归档以 agent `resume_id` 为身份，持久保存恢复命令、工作目录和原终端名称。
- “历史工作区”解析出活动终端的 agent session 后，会通过 `POST /api/terminal/resume-archives` 补写映射；名称和目录未变化时不重复写入。
- agent session 可以并行探测，但补写请求必须在浏览器端串行发送；恢复归档是单文件注册表，并发读改写会产生 500 或覆盖更新。
- 活动终端已有同一 `resume_id` 时不额外显示归档行；终端结束后，归档行继续显示原终端名称。
- “终端管理”和“历史工作区”共用终端改名模态弹窗（保存/Enter 提交，取消/Escape 关闭，遮罩点击不关闭，关闭后焦点返回触发按钮）：终端管理进入改名时在原名称后补 `_` 并把光标放在末尾；保存时清理首尾空白和所有末尾 `_`，名称中间的 `_` 保留。活动终端先更新实际会话并同步恢复归档，非活动归档或纯历史对话直接更新或创建恢复归档；后续恢复沿用修改后的名称。
- 点击“恢复”先用 `POST /api/terminal/sessions` 创建具体终端，再用 `PUT /api/terminal/sessions/{session_id}` 改回原名称；若名称已被占用，则依次尝试原名称加后缀 `2`、`3`，最后打开终端并运行 resume 命令。
- “历史工作区”的操作列对 Codex 会话直接显示 `fork`：前端只接受规范 UUID，并在新终端的原工作目录执行 `codex fork <session UUID>`；有归档终端名时，新终端命名为 `<原终端名>_fork`。Claude 会话和无效 session ID 不显示该入口。
- 修改 Codex 会话模型使用 `PUT /api/terminal/codex-conversations/model`，请求体 `{ "session": "<uuid>", "model": "" }`。`model` 省略或为空时取当前生效的 Codex API 预设模型；指定时优先使用指定模型。接口会改写该会话 rollout 中所有 `turn_context` 的 `payload.model` 与 `collaboration_mode.settings.model`，使后续 `codex resume <uuid>` 不再沿用旧模型，并保留原会话上下文。
- 历史工作区点击“恢复”时，webClx 会在发送 resume 命令前按当前模型改写对应 rollout，再附带 `--model`，避免出现“Banner 已显示新模型、实际请求仍传旧模型”的错位。
- “历史工作区”的删除动作只允许用于非活动会话。`DELETE /api/terminal/codex-conversations/{session_id}` 会清理 rollout、Codex 会话索引、输入历史、SQLite 线程元数据和 webClx 恢复归档；活动终端必须先结束，避免 Codex 继续写回已删除记录。删除成功后前端只从本地 `codexConversations` 和 `terminalArchives` 移除该 session 并重绘，不重新请求全部历史；“刷新对话”仍提供完整同步。
- “历史工作区”首次进入优先选择浏览器当前实际目录，并默认只读取下拉框当前工作目录：活动终端使用目录级 sessions 查询，Codex 对话使用 `GET /api/terminal/codex-conversations?cwd=<absolute-path>`。切换目录会重新读取该目录；已删除历史目录的 sessions 404 降级为空终端列表，对话仍可显示；只有用户显式启用“搜索全部工作区”时才请求全量终端和对话，避免大量历史目录让默认加载延迟或失败。
- 历史工作区详情请求（`agent-session`、`input-history`）通过 `runWorkspaceHistoryWithConcurrency` 以固定并发上限（当前 4）执行，不再一次并发两组最多 48 个请求。每轮 `refreshWorkspaceHistoryConversations` 开始时先 `abort()` 上一轮在途的核心与详情 GET（`state.workspaceHistoryDetailAbortController`），被中止请求不落状态；归档补写 POST 仍走浏览器端串行队列且不传中止信号，保证单文件注册表的写入顺序。`agent_session_id`、`input_history_text` 应用前仍检查 `requestToken`，快速切换目录时旧结果不得覆盖新请求。
- 后端 `GET /api/terminal/codex-conversations` 对相同 `codex_home` + `cwd_filter` 的并发请求共享一次在途扫描：首个请求成为 leader，其余请求通过 watch 通道等待共享结果（成功与错误都照常传播，不吞错、不假成功）。leader 完成（含超时错误）后从注册表移除槽位；超过 `TERMINAL_TASK_TIMEOUT_SECS + 2` 秒仍未完成的孤儿槽位可被新请求接管，避免丢失的 leader 永久阻塞。扫描元数据缓存（`history.jsonl`/`session_index.jsonl` mtime 失效）命中时返回 `Arc` 共享，避免每次请求克隆整个 HashMap。
- `run_terminal_task` 对排队 ≥1 秒或执行 ≥2 秒的后台任务输出慢任务日志（queue_ms/execution_ms）；命中 10 秒统一超时时输出“blocking work continues in background”日志。超时阈值保持 10 秒不变，优化方向是减少任务本身而非延长时间。
- 不引入持久化增量索引：sessions 目录遍历仅有 read_dir + metadata 成本，rollout 回退读取只发生在索引缺失的旧会话，且已有 >250ms 慢扫描日志（metadata_ms/rollouts_ms/sort_ms）观测分阶段耗时。历史上触发 10 秒超时的真实来源是重复刷新叠加全量扫描和 48 个并发详情请求，已由扫描合并与前端限并发解决；若未来慢扫描日志持续告警再评估持久化索引。
- 终端工具的“中断并恢复”用于处理无法响应 `Esc`/`Ctrl-C` 的 Codex 或 Claude 等待。后端必须先从当前 tmux 进程树确认恢复 ID，只向持有对应 rollout 的进程发送 `SIGINT`；确认进程退出后，才在同一终端发送检测得到的恢复命令。检测失败或退出超时必须停止，不得猜测 PID、恢复 ID或并行启动第二个智能体。
- 删除、重建、启动失败及启动取消共用 `destroy_tmux_session`。销毁前通过 `list-panes -s` 枚举全部窗口的 pane，以 Linux pidfd 固定进程身份；采集期间短暂暂停该终端专属 tmux server（tmux 默认会对停止的 pane 自动发送 SIGCONT），再暂停父进程并逐层读取所有线程的 children，避免采集期间继续 fork。`D` 状态线程已经停在不可中断内核 IO，不能 fork，且 pending `SIGSTOP` 会在返回用户态前生效，因此按静止线程采集，不等待它转换为 `T`。采集完成立即恢复 server，异常路径由 RAII 恢复。关闭 tmux 后，无论 tmux 命令是否成功，都对已捕获进程 TERM/CONT，短暂等待后 KILL；仍处于 `D` 状态的进程保留不可捕获的 KILL，由内核在 IO 返回后异步回收并记录告警，不阻塞终端从 UI 删除。不按裸 PID 发信号，不扫描全局进程猜测孤儿。Linux 内核必须支持 pidfd，缺失时报错而不降级。
- 终端删除以已有的 per-session lifecycle lock 串行化同一会话的启动、连接和删除；全局 `TerminalState` 锁只用于读取快照和最终移除状态。进程树采集、tmux 命令、退出等待、恢复归档探测等慢操作必须在全局锁之外执行，避免一个远程盘/FUSE 故障阻塞其他终端。
- Codex writer lock 由持锁进程退出释放，不删除 `.lock` 文件：unlink 后重建会产生不同 inode，可能破坏互斥。无 TTY、sleeping、低 CPU 或锁文件存在均不单独证明进程已失联。历史遗留孤儿需另行核实所有权；新销毁逻辑不会全局清理旧会话。
- 回归测试 `cargo test --locked --bin webclx terminal_reap -- --test-threads=1` 覆盖多窗口 pane、忽略 TERM/HUP 的持锁子进程、重建、幂等销毁、pidfd 退出身份和无关进程保护。通过编译 API 提交，完整命令加 `timeout --signal=KILL`。

## 根因边界

Codex 对话 JSONL 只记录 agent session 与 cwd，不知道 webClx 终端名称。终端名称不能在历史页临时推断，必须在活动终端仍能和 agent session 对应时写入恢复归档。

历史列表不能为每次请求逐个解析所有 rollout 的前 512 行。大量会话会让 `GET /api/terminal/codex-conversations` 超过统一的 10 秒终端任务超时；`spawn_blocking` 在调用方超时后仍会继续扫描，重复刷新还会连带拖慢终端连接。列表应先合并小型 `history.jsonl` 和 `session_index.jsonl` 中的标题、cwd 元数据，仅对索引缺失的旧会话回退 rollout 扫描。后端改动需要重新构建并部署二进制；验证时同时检查历史接口 HTTP 200、实际耗时和返回条数，不能只确认 tmux 已安装。

## 相关文件

- `static/app-workspace-history.js`
- `static/app.js`
- `static/app-terminal-archives.js`
- `crates/terminal_core/src/lib.rs`
- `src/codex_conversation_model.rs`
- `src/terminal.rs`
- `src/terminal/manager.rs`

## 验证

```bash
node tests/workspace-history-fork.test.mjs
node tests/workspace-history-terminal-name-restore.test.mjs
node tests/workspace-terminal-fresh-link.test.mjs
node tests/workspace-history-detail-concurrency.test.mjs
node --check static/app-workspace-history.js
node --check static/app.js
```

前端改动必须同步整个 `static/` 到运行目录；只修改源码不会让 `/home/bin/webclx` 的页面生效。
