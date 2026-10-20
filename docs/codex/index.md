# webClx Codex Context Index

这是 Codex/Claude 处理 `webClx` 任务前的主题导航。先读 [AGENTS.md](/home/codes/tools/webClx/AGENTS.md)，再按任务选择一个专题；不要把本文件扩写成实现日志。

## 必读入口

| 任务 | 先读 |
| --- | --- |
| API 预设、切换、测试、隔离核验、代理或协议转换 | [API 预设路由边界](tasks/api-preset-routing-boundaries.md) |
| 各类模型预设切换流程、不同提供商与 LLM 协议转换 | [模型预设切换与协议转换](tasks/model-preset-switching.md)；[provider/Base URL 混用事故](../archive/incidents/2026-08-04-codex-provider-base-url-mixing.md) |
| 编译、部署或重启 webClx | [webClx rebuild skill](skills/webclx-rebuild/SKILL.md) |
| 运行或维护仓库自动检查 | [仓库检查入口](tasks/repo-check-entry.md) |
| 前端静态文件改动后页面未更新 | [静态文件同步](tasks/static-deployment.md) |
| 定时协作消息停留输入框、未提交或确认状态 | [定时消息提交可靠性](tasks/terminal-scheduled-message-delivery.md) |
| 终端切换、回放、滚动或输出错乱 | [终端会话切换与输出](tasks/terminal-session-switch-output.md) |
| 后端或前端大文件拆分 | [模块拆分约定](tasks/project-module-slimming.md) |

## 主题索引

### API 与配置

- [登录会话持久化](tasks/login-session-persistence.md)：cookie、签名密钥和登录页恢复跳转的跨重启边界。
- [FRP 角色管理边界](tasks/frp-role-management.md)：外部配置保存、进程所有权、重复启动和状态锁死锁边界。
- [API 预设路由边界](tasks/api-preset-routing-boundaries.md)：`direct`、本地中继、协议转换、测试探针和客户端凭据的边界。
- [模型预设切换与协议转换](tasks/model-preset-switching.md)：Codex_OAuth / Codex_API / Claude_API 切换流程、responses_proxy 推断、本机中继与协议转换路由。
- [Devin Desktop 登录备份与恢复](tasks/devin-desktop-auth.md)：CLI/Desktop 类型边界、加密快照、原环境恢复和验收限制。
- [CodeBuddy 账号类型](tasks/codebuddy-accounts.md)：账号库、扫码添加、CLI 切换（env token）、自动签到/旅行、积分查询。
- [预设手动排序](tasks/preset-manual-ordering.md)：顺序持久化和重排 API 约束。
- [MiniMax TokenPlan 字段](tasks/minimax-tokenplan.md)：限额字段、状态码和自动重查。
- [Grok Build 余量](tasks/grok-quota.md)：OIDC 刷新、billing/subscription 接口和前端展示。

### 编译与部署

- [webClx rebuild skill](skills/webclx-rebuild/SKILL.md)：编译队列、部署命令、回调和手工兜底。
- [仓库检查入口](tasks/repo-check-entry.md)：统一只读检查脚本、报告格式、编译队列触发清单与浏览器冒烟边界。
- [编译编排器可靠性](tasks/compile-coordinator-reliability.md)：请求唯一身份、worker 启动失败语义和无回调排障边界。
- [静态文件同步](tasks/static-deployment.md)：源码目录与运行目录的区别。
- [产物下载中心](tasks/artifact-download-center.md)：构建产物发布入口。
- [远程部署](tasks/remote-sync-xiaoshuai.md)：远端主机路径和同步约束。
- [Windows 兼容](tasks/windows-compatibility.md)：原生 Windows 与 WSL2 支持边界。

### 终端会话

- [会话切换与输出](tasks/terminal-session-switch-output.md)：WebSocket、backlog、滚动恢复、输出合并和新会话隔离。
- [终端按键投递与退格失效](tasks/terminal-input-keystroke-delivery.md)：按读取批次解析按键、退格字节分段投递与间隔。
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
- [Android 横竖屏切换](tasks/android-orientation-toggle.md)：悬浮按钮三态方向、原生 bridge、持久化与 APK 验证。
- [Android 客户端可靠性](tasks/android-client-reliability.md)：1.8.21 探测、恢复、更新事务、重连退避与终端上下文缓存边界。
- [恢复命令提取](tasks/terminal-resume-hard-wrap.md)：硬换行 UUID 和不完整命令处理。
- [软键盘项目指令](tasks/terminal-soft-keyboard-deploy.md)：部署动作的项目路径解析，以及项目级 `.webclx.json` Web 入口配置。

### Agent 与外部集成

- [Agent 与外部智能体集成](tasks/agent-api.md)：跨平台持久讨论组、Markdown 历史、DeepSeek Harness、终端 profile、隔离预设执行、终端消息 API 和认证方式。
- [cyMusic 远程诊断](tasks/cymusic-remote-diagnostics.md)：Android 播放日志上传、服务端保留和认证查询边界。

### 代码与工作区

- [可插拔架构与动态链接库评估](tasks/astra_pluggable_architecture.md)：模块适配、进程插件协议、C ABI 边界与渐进迁移验收。
- [模块拆分约定](tasks/project-module-slimming.md)：Rust 模块和经典脚本拆分边界。
- [浏览器 i18n 运行时](tasks/i18n-runtime.md)：系统语言默认值、用户选择持久化、顶栏右上角布局和跨页面浏览器验收。
- [文件系统访问边界](tasks/filesystem-access-boundaries.md)：工作区允许根、canonical 授权、符号链接 cwd 和文件写入审计。
- [工作区文件改名](tasks/workspace-file-rename.md)：重命名 API、安全限制和前端同步。
- [工作区项目图标](tasks/workspace-project-icons.md)：项目相对图标路径、工作区图标列与活动终端图文选择器。
- [systemd scope fallback](tasks/terminal-systemd-scope.md)：终端创建时的 scope 回退。
- [Codex skill 上下文预算](tasks/codex-skills-budget.md)：skill description 预算治理、离线库与 agy/Gemini 技能发现。

## 文档维护

- 当前行为放在 `README.md`、`AGENTS.md` 或对应专题中。
- `docs/codex/index.md` 只做导航，不复制专题正文。
- 可复用的跨文件结论放在 `docs/codex/tasks/<topic>.md`，按主题维护，不按每次任务新建文件。
- 一次性审计、事故复盘、迁移记录和已完成方案放入 `docs/archive/`。
- webClx 自身的直接构建日志属于运行产物，可保留在 gitignored 的 `docs/logs/`；编译
  API 的日志和安装审计统一保存在 `.webclx-compile-queue/runs/<run-id>/outputs/`，不得写入客户端项目。

归档内容仅用于追溯，不代表当前实现。归档目录说明见 [docs/README.md](../README.md)。
