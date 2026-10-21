# webClx 文档

## 当前文档

- [项目 README](../README.md)：功能、运行方式和部署入口。
- [项目规则](../AGENTS.md)：开发、排障、编译和提交规则。
- [Codex 主题索引](codex/index.md)：按任务查找专题文档。
- [试用与商业化执行手册](trial-commercial-playbook.md)：隔离试用、建议定价、服务边界和验证指标。
- [上市与收入验证方案](go-to-market.md)：首批客户、痛点、产品梯度、渠道漏斗和两周执行节奏。
- [隔离托管预览条款草案](hosted-preview-policy-template.md)：隐私、试用、退款、支持、导出和删除边界的发布前模板。
- [公网发布与托管试用就绪清单](public-launch-readiness.md)：公开下载、DNS/TLS、一客一实例和验收门禁。
- [中英文发布文案](launch-copy.md)：渠道顺序、可直接发布的文案和发布前替换项。
- `codex/tasks/`：仍可复用的设计约束和排障经验。
- `codex/skills/`：项目内操作流程及脚本。

## 历史资料

`archive/` 中的内容只用于追溯，不代表当前实现：

- `archive/superpowers/`：已完成的设计和实施计划。
- `archive/audits/`：带时间点的代码规模与优化审计。
- `archive/incidents/`：一次性事故复盘。
- `archive/migrations/`：已完成的路径或配置迁移记录。

`logs/` 是 gitignored 的本项目构建与部署运行日志，不属于维护文档。通过编译 API
执行其它项目时，详细日志和安装审计由 webClx 集中保存在
`.webclx-compile-queue/runs/<run-id>/outputs/`，不会写入客户端项目源码目录。
旧版本已经散落到 `/home/codes/**/docs/logs/` 的 API 日志集中归档在
`.webclx-compile-queue/legacy/`，目录层级保留原始来源。

## 维护原则

1. 先更新已有主题，不为每次修复新建一篇文档。
2. 文档只保留当前约束、根因、代码入口和验证方法，删除过程性尝试与重复结论。
3. 时间敏感的统计和现场证据必须标明日期；失去当前指导价值后移入 `archive/`。
4. 移动或删除文档后检查仓库内相对链接。
