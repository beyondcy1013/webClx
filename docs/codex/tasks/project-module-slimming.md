# 模块拆分约定

## 后端

- 优先做不改变行为的内聚提取：路由、DTO、测试和纯 helper 分开移动。
- 路由名、响应结构和运行路径保持不变。
- 可见性限制在所属领域，例如 `pub(in crate::terminal)` 或 `pub(in crate::frpc)`。
- `src/terminal/manager.rs` 中不依赖 `self` 的函数组可放到 `src/terminal/manager/<topic>.rs`；需要跨兄弟模块使用时由 `manager.rs` 重新导出。
- 每次提取后运行 `cargo fmt --check`、`cargo check` 和相关测试。

### 当前模块入口

- HTTP 路由按领域注册在 `src/routes/*.rs`，由 `src/routes/mod.rs::app` 组合 state、认证中间件、SPA fallback 和压缩层；`main.rs` 只负责初始化并调用该入口。新增或移动 handler 时同步修改所属领域路由文件，不把路由链放回 `main.rs`。
- `src/upstream_proxy.rs` 负责预设选择、凭据优先级、访问边界和网络转发；Anthropic/OpenAI 的纯 JSON 协议转换位于私有模块 `src/upstream_proxy/transform.rs`，测试位于 `src/upstream_proxy/tests.rs`。协议转换不得顺带改变 direct、本地中继或 preset-scoped token 的路由语义。

### 终端与 Agent 拆分（2026-09-20）

- `src/terminal/websocket.rs` 接管 WebSocket 视口生命周期、首屏回放、丢帧补偿及输入转发；`terminal.rs` 保留 HTTP 入口和会话解析。通过父模块私有导入连接，路由及响应格式不变。
- `src/agent/context.rs` 负责 API 消息构造、token 估算、上下文状态和历史压缩。父模块重新导出原有公开结果类型，内部 helper 使用 `pub(super)`，其余 helper 保持私有。
- `src/agent/terminal_profile_tests.rs` 承接原同名内联测试模块，保留测试名称与断言。源码契约测试同时读取终端入口与 WebSocket 子模块，保留原断言。
- 本轮只做代码移动、必要导入与可见性调整；迁移函数体经忽略格式及可见性差异的逐段比对，与拆分前一致。编译和测试状态见 [astra 优化计划](astra_optimization_plan.md)。

### 终端 Codex 历史职责族拆分（2026-09-20 D2.2）

- `src/terminal/conversations.rs` 承接原 `terminal.rs` 中 `scan_codex_conversations` 至 `clamp_text_chars` 的 Codex 历史扫描、删除、SQLite 与 jsonl 索引维护、元数据解析及标题清洗职责族。
- `CODEX_METADATA_CACHE` 与扫描行数/标题字符等常量移入子模块私有管理。
- 父模块 `terminal.rs` 通过 `use conversations::{...}` 引入 HTTP handler 依赖，并通过 `#[cfg(test)] use conversations::{...}` 将子模块测试 helper 引入父模块私有作用域，保留现有 `terminal/tests.rs` 对其测试调用的兼容，无需扩大为 crate 级重导出。
- `DeleteCodexConversationResponse::total_deleted` 作为 DTO 固有计算方法内联于父模块 DTO 声明处，保持 `delete_codex_conversation` 的 404 判定语义及测试契约完全不变；父模块 DTO 字段及方法保持原私有（子模块天然可访问祖先私有项）。
- 迁移为纯函数移动与最小可见性调整，函数体、权限、错误语义、缓存与删除逻辑完全保持原样。

### Agent 技能发现职责族拆分（2026-09-20 D2.3）

- `src/agent/skills.rs` 承接原 `agent.rs` 中生产存活的技能发现与解析职责族（`SKILLS_DIR_RELATIVE` 常量，以及 `skills_dir_for_user`、`scan_all_skills`、`scan_skills_dir`、`parse_skill_description`、`parse_yaml_front_matter_field`）。
- 仅 `skills_dir_for_user` 与 `scan_all_skills` 赋予 `pub(super)` 可见性，由父模块 `agent.rs` 私有导入以支撑生产路由 `GET /api/agent/skills` 及已有调用；其余内部扫描与 YAML 解析 helper 保持子模块私有。
- `list_skills_api` HTTP 适配层、路径安全解析（`resolve_skill_dir_from_roots`、`resolve_skill_script_path` 等）、旧 loop 遗留 tool 运行器、执行函数及显式语法解析全部保留在父模块原处，避免对已交接 Harness 的旧 chat loop 死代码做无价值重构。
- 源码契约测试 `tests/builtin-terminal-message-skill.test.mjs` 扩展为同时读取 `src/agent.rs` 与 `src/agent/skills.rs`，全量断言保持不变。
- 迁移为纯移动与最小可见性调整，扫描优先级、去重、排序、disabled 标记与函数体经逐段比对完全一致。

### 规模与 panic 审计

- 文件字节数只用于定位候选，不直接等价于职责混乱或高严重度。先区分生产实现、内联测试、生成数据和同一领域内聚逻辑，再决定是否存在稳定拆分边界。
- 统计 `.unwrap()` / `.expect()` 时必须排除 `#[cfg(test)]` 模块，并逐处确认是否在请求输入、外部 I/O 或可变状态路径上可触发。测试断言、`unwrap_or*` 和已被类型/分支证明的不变量不能合并成“生产 panic 点”数量。
- 面向请求的可失败操作使用现有 `ApiResult<AppError>` 或 `anyhow::Result` 返回错误；不要仅为统一错误外观引入新的错误处理依赖。

## 前端

首页和终端页使用按顺序加载的经典 `<script defer>`，共享同一个全局作用域。拆分时采用两种现有模式：

- 有私有状态的功能使用 `globalThis.WebClxXxxManager.create(deps)` manager。
- 纯函数子系统可直接移动全局函数声明，由 HTML 在主入口脚本前加载。

只移动函数声明。依赖 DOM、状态变量或 observer 的顶层初始化必须留在主入口，避免子脚本提前执行。不要为了拆文件改成 ES module，除非同时完成依赖边界设计和浏览器回归。

拆 manager 后要检查主脚本是否仍裸调用 manager 内部函数。`node --check` 只能发现语法错误，不能发现这种运行时 `ReferenceError`。

## 验证

```bash
node --check static/app.js
node --check static/terminal.js
python3 .smoke/smoke.py
```

同时检查两个 HTML 入口的脚本顺序、浏览器 console error，以及新增静态文件是否被部署目录完整同步。具体同步规则见 [静态文件源码与部署副本](static-deployment.md)。
