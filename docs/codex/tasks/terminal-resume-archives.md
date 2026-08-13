# 终端恢复归档与原名称

## 稳定行为

- 恢复归档以 agent `resume_id` 为身份，持久保存恢复命令、工作目录和原终端名称。
- “历史工作区”解析出活动终端的 agent session 后，会通过 `POST /api/terminal/resume-archives` 补写映射；名称和目录未变化时不重复写入。
- agent session 可以并行探测，但补写请求必须在浏览器端串行发送；恢复归档是单文件注册表，并发读改写会产生 500 或覆盖更新。
- 活动终端已有同一 `resume_id` 时不额外显示归档行；终端结束后，归档行继续显示原终端名称。
- “终端管理”和“历史工作区”共用终端改名模态弹窗（保存/Enter 提交，取消/Escape/点击遮罩关闭，关闭后焦点返回触发按钮）：终端管理进入改名时在原名称后补 `_` 并把光标放在末尾；保存时清理首尾空白和所有末尾 `_`，名称中间的 `_` 保留。活动终端先更新实际会话并同步恢复归档，非活动归档或纯历史对话直接更新或创建恢复归档；后续恢复沿用修改后的名称。
- 点击“恢复”先用 `POST /api/terminal/sessions` 创建具体终端，再用 `PUT /api/terminal/sessions/{session_id}` 改回原名称；若名称已被占用，则依次尝试原名称加后缀 `2`、`3`，最后打开终端并运行 resume 命令。
- “历史工作区”的操作列对 Codex 会话直接显示 `fork`：前端只接受规范 UUID，并在新终端的原工作目录执行 `codex fork <session UUID>`；有归档终端名时，新终端命名为 `<原终端名>_fork`。Claude 会话和无效 session ID 不显示该入口。
- 修改 Codex 会话模型使用 `PUT /api/terminal/codex-conversations/model`，请求体 `{ "session": "<uuid>", "model": "" }`。`model` 省略或为空时取当前生效的 Codex API 预设模型；指定时优先使用指定模型。接口会改写该会话 rollout 中所有 `turn_context` 的 `payload.model` 与 `collaboration_mode.settings.model`，使后续 `codex resume <uuid>` 不再沿用旧模型，并保留原会话上下文。
- 历史工作区点击“恢复”时，webClx 会在发送 resume 命令前按当前模型改写对应 rollout，再附带 `--model`，避免出现“Banner 已显示新模型、实际请求仍传旧模型”的错位。
- “历史工作区”的删除动作只允许用于非活动会话。`DELETE /api/terminal/codex-conversations/{session_id}` 会清理 rollout、Codex 会话索引、输入历史、SQLite 线程元数据和 webClx 恢复归档；活动终端必须先结束，避免 Codex 继续写回已删除记录。删除成功后前端只从本地 `codexConversations` 和 `terminalArchives` 移除该 session 并重绘，不重新请求全部历史；“刷新对话”仍提供完整同步。
- “历史工作区”首次进入优先选择浏览器当前实际目录，并默认只读取下拉框当前工作目录：活动终端使用目录级 sessions 查询，Codex 对话使用 `GET /api/terminal/codex-conversations?cwd=<absolute-path>`。切换目录会重新读取该目录；已删除历史目录的 sessions 404 降级为空终端列表，对话仍可显示；只有用户显式启用“搜索全部工作区”时才请求全量终端和对话，避免大量历史目录让默认加载延迟或失败。
- 终端工具的“中断并恢复”用于处理无法响应 `Esc`/`Ctrl-C` 的 Codex 或 Claude 等待。后端必须先从当前 tmux 进程树确认恢复 ID，只向持有对应 rollout 的进程发送 `SIGINT`；确认进程退出后，才在同一终端发送检测得到的恢复命令。检测失败或退出超时必须停止，不得猜测 PID、恢复 ID或并行启动第二个智能体。

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
node --check static/app-workspace-history.js
node --check static/app.js
```

前端改动必须同步整个 `static/` 到运行目录；只修改源码不会让 `/home/bin/webclx` 的页面生效。
