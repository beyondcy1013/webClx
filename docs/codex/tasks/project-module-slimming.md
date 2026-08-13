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
