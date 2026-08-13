# webClx Codex Context Index

这是 Codex/Claude 处理 `webClx` 任务前的主题导航。先读 [AGENTS.md](/home/codes/webClx/AGENTS.md)，再按任务选择一个专题；不要把本文件扩写成实现日志。

## 必读入口

| 任务 | 先读 |
| --- | --- |
| API 预设、切换、测试、隔离核验、代理或协议转换 | [API 预设路由边界](tasks/api-preset-routing-boundaries.md) |
| 各类模型预设切换流程、不同提供商与 LLM 协议转换 | [模型预设切换与协议转换](tasks/model-preset-switching.md)；[provider/Base URL 混用事故](../archive/incidents/2026-08-04-codex-provider-base-url-mixing.md) |
| 编译、部署或重启 webClx | [webClx rebuild skill](skills/webclx-rebuild/SKILL.md) |
| 前端静态文件改动后页面未更新 | [静态文件同步](tasks/static-deployment.md) |
| 终端切换、回放、滚动或输出错乱 | [终端会话切换与输出](tasks/terminal-session-switch-output.md) |
| 后端或前端大文件拆分 | [模块拆分约定](tasks/project-module-slimming.md) |

## 主题索引

### API 与配置

- [登录会话持久化](tasks/login-session-persistence.md)：cookie、签名密钥和登录页恢复跳转的跨重启边界。
- [API 预设路由边界](tasks/api-preset-routing-boundaries.md)：`direct`、本地中继、协议转换、测试探针和客户端凭据的边界。
- [模型预设切换与协议转换](tasks/model-preset-switching.md)：Codex_OAuth / Codex_API / Claude_API 切换流程、responses_proxy 推断、本机中继与协议转换路由。
- [预设手动排序](tasks/preset-manual-ordering.md)：顺序持久化和重排 API 约束。
- [MiniMax TokenPlan 字段](tasks/minimax-tokenplan.md)：限额字段、状态码和自动重查。

### 编译与部署

- [webClx rebuild skill](skills/webclx-rebuild/SKILL.md)：编译队列、部署命令、回调和手工兜底。
- [编译编排器可靠性](tasks/compile-coordinator-reliability.md)：请求唯一身份、worker 启动失败语义和无回调排障边界。
- [静态文件同步](tasks/static-deployment.md)：源码目录与运行目录的区别。
- [产物下载中心](tasks/artifact-download-center.md)：构建产物发布入口。
- [远程部署](tasks/remote-sync-xiaoshuai.md)：远端主机路径和同步约束。
- [Windows 兼容](tasks/windows-compatibility.md)：原生 Windows 与 WSL2 支持边界。

### 终端会话

- [会话切换与输出](tasks/terminal-session-switch-output.md)：WebSocket、backlog、滚动恢复、输出合并和新会话隔离。
- [会话活动状态](tasks/terminal-session-activity.md)：工作、错误、重试、完成、输出和空闲状态。
- [继续发送统一入口](tasks/terminal-continue-send-unification.md)：`继续` API、自动重试和定时任务。
- [会话输出搜索](tasks/terminal-session-search.md)：跨活动会话搜索。
- [会话改名预设](tasks/terminal-rename-presets.md)：改名配置与焦点保护。
- [恢复归档与原名称](tasks/terminal-resume-archives.md)：恢复 ID、工作目录、原终端名持久化和恢复改名顺序。
- [长历史回放](tasks/terminal-long-history-duplication.md)：tmux 快照重复和历史容量。

### 终端渲染与移动端

- [光标闪烁与输入法](tasks/terminal-codex-cursor-flicker.md)：光标纠偏、主题重绘、尺寸同步和移动光标。
- [拖选闪烁与选区](tasks/terminal-drag-selection-flicker.md)：选区期间重绘、断线回放和高亮对比度。
- [移动端触摸选择](tasks/terminal-touch-selection.md)：长按、手柄和原生选择冲突。
- [移动端滚动稳定](tasks/terminal-mobile-scroll-settle.md)：IME 与 `visualViewport` settle window。
- [恢复命令提取](tasks/terminal-resume-hard-wrap.md)：硬换行 UUID 和不完整命令处理。
- [软键盘项目指令](tasks/terminal-soft-keyboard-deploy.md)：部署动作的项目路径解析，以及项目级 `.webclx.json` Web 入口配置。

### Agent 与外部集成

- [Agent API 与外部智能体集成](tasks/agent-api.md)：内置 Agent API、隔离预设执行、SSE 事件格式、终端消息 API 和认证方式。

### 代码与工作区

- [模块拆分约定](tasks/project-module-slimming.md)：Rust 模块和经典脚本拆分边界。
- [文件系统访问边界](tasks/filesystem-access-boundaries.md)：工作区允许根、canonical 授权、符号链接 cwd 和文件写入审计。
- [工作区文件改名](tasks/workspace-file-rename.md)：重命名 API、安全限制和前端同步。
- [工作区项目图标](tasks/workspace-project-icons.md)：项目相对图标路径、工作区图标列与活动终端图文选择器。
- [systemd scope fallback](tasks/terminal-systemd-scope.md)：终端创建时的 scope 回退。
- [Codex skill 上下文预算](tasks/codex-skills-budget.md)：skill description 预算治理。

## 文档维护

- 当前行为放在 `README.md`、`AGENTS.md` 或对应专题中。
- `docs/codex/index.md` 只做导航，不复制专题正文。
- 可复用的跨文件结论放在 `docs/codex/tasks/<topic>.md`，按主题维护，不按每次任务新建文件。
- 一次性审计、事故复盘、迁移记录和已完成方案放入 `docs/archive/`。
- webClx 自身的直接构建日志属于运行产物，可保留在 gitignored 的 `docs/logs/`；编译
  API 的日志和安装审计统一保存在 `.webclx-compile-queue/runs/<run-id>/outputs/`，不得写入客户端项目。

归档内容仅用于追溯，不代表当前实现。归档目录说明见 [docs/README.md](../README.md)。
