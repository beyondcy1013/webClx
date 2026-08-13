# webClx 开源试运行发布文案

这些文案可由项目所有者发布。它们不是已发布记录；发布到各社区前仍需按对应版规调整标题和格式。

## 中文长文

### 标题

webClx：在手机和浏览器里管理 Codex、Claude 与 DeepSeek Harness 的持久终端

### 正文

我开源了 webClx，一个自托管的浏览器工作区。它不替代 Codex、Claude 或 DeepSeek Harness，而是把它们原有的终端和上下文保留下来，再提供统一的工作区、持久 tmux 会话、预设切换、任务转交、相互 review、编译部署队列和手机端操作。

这次试运行包括中英文界面，以及内置的 `webclx-terminal-message` Skill。Codex、Claude 和 DeepSeek Harness 可以通过现有终端消息 API 交换任务；同一工作树建议只保留一个写入者，其它 Agent 做只读 review。

项目采用 AGPL-3.0-or-later。适合自己部署的个人开发者可以免费使用；我也在小规模测试隔离托管、部署支持和商业许可。当前仍是开发者预览，请放在可信网络、TLS 反向代理、防火墙或 VPN 后面，不要直接裸露管理端口。

- 源码：https://github.com/beyondcy1013/webClx
- 版本化源码包和 SHA-256：https://github.com/beyondcy1013/webClx/releases/tag/v1.8.11
- 英文说明：https://github.com/beyondcy1013/webClx/blob/main/README.en.md

我尤其想听到三类反馈：手机端终端是否真正可用、跨 Harness 任务转交是否节省时间、你愿意为什么样的托管或支持付费。

## 中文短文

开源试运行：webClx 是 Codex、Claude、DeepSeek Harness 的自托管浏览器工作区，保留原生终端与上下文，支持持久会话、手机操作、任务转交和相互 review。中英文 UI 与终端通讯 Skill 已内置。AGPL 自托管免费，隔离托管正在小规模招募试用。源码：https://github.com/beyondcy1013/webClx

## Show HN / Product Hunt English

### Title

Show HN: webClx, a self-hosted mobile workspace for Codex, Claude, and DeepSeek Harness

### Body

I built webClx to keep native coding-agent terminals persistent and usable from a browser or phone. It complements Codex, Claude, and DeepSeek Harness rather than replacing them.

The current developer preview provides persistent tmux-backed sessions, workspace browsing and editing, provider presets, build/deploy queues, downloadable artifacts, and a built-in terminal messaging Skill. The Skill can hand a task from one Harness to another and request a read-only review while keeping one writer per working tree.

The UI is available in Chinese and English. The project is AGPL-3.0-or-later and free to self-host. I am also validating isolated managed hosting, deployment support, and commercial licensing. Because webClx has administrative access to files and terminals, it should run behind TLS and network controls, not on an exposed management port.

- Source: https://github.com/beyondcy1013/webClx
- Versioned source archive and SHA-256: https://github.com/beyondcy1013/webClx/releases/tag/v1.8.11
- Security model: https://github.com/beyondcy1013/webClx/blob/main/SECURITY.md

I would value feedback on mobile terminal ergonomics, cross-Harness handoffs, and the smallest managed plan you would actually pay for.

## 渠道顺序

| 阶段 | 海外 | 国内 | 发布条件 |
| --- | --- | --- | --- |
| 1. 技术验证 | GitHub Release, Show HN, Dev.to | V2EX, 掘金, 开源中国 | 公共仓库、版本包、安装文档和安全说明可访问 |
| 2. 产品验证 | Product Hunt, Indie Hackers, Reddit 相关社区 | 少数派、知乎、即刻 | 有真实演示视频、3 至 5 位试用反馈和明确试用入口 |
| 3. 长尾收录 | AlternativeTo, Hashnode | 项目周报与案例文章 | 有稳定版本、定价页、隐私和服务条款 |

每个社区先阅读版规，避免同日机械群发。首批内容以技术实现、真实限制和演示为主，不伪造用户数、收入、客户评价或安全认证。外部发帖需要项目所有者的账号与明确授权；自动化代理只准备文案和检查链接。

## 发布前检查项

- 试用申请入口：独立表单或邮箱，不使用共享管理员登录页。
- 演示素材：桌面和手机的真实产品画面，隐藏工作区、终端历史、模型凭据和客户数据。
