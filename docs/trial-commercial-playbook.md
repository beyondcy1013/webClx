# webClx 试用与商业化执行手册

English summary follows the Chinese operating plan.

## 产品与许可

- 免费自托管版：GNU AGPL-3.0-or-later，用户自行部署、维护并承担模型费用。
- 商业许可：面向无法履行 AGPL 网络源码义务的组织，按合同授权，不改变开源版可用性。
- 托管服务：出售隔离部署、升级、备份和支持，不出售或转售 Codex、Claude、DeepSeek 的模型账号。
- 当前阶段是开发者预览，不宣传为企业级零停机平台，也不承诺尚未验证的合规认证。

## 建议试运行价格

| 方案 | 试运行价 | 适用范围 | 包含内容 |
| --- | ---: | --- | --- |
| 自托管社区版 | 免费 | 熟悉 Linux 的个人开发者 | 源码、双语 UI、内置终端通讯 Skill、社区问题反馈 |
| 托管基础版 | CNY 49/月或 USD 8/月 | 个人远程手机编程 | 单用户、1 个隔离实例、每周备份、工作日尽力支持 |
| 托管专业版 | CNY 129/月或 USD 19/月 | 重度个人用户 | 单用户、独立实例、每日备份、优先升级、工作日 24 小时内响应 |
| 团队试运行版 | CNY 399/月起或 USD 59/月起 | 2 至 5 人小团队 | 独立实例或专属主机、成员边界设计、升级窗口、月度恢复演练 |
| 商业许可与部署支持 | 单独报价 | 不采用 AGPL 的组织 | 商业许可、部署评审、迁移和约定支持 |

价格不包含模型 API、云主机、域名、短信、对象存储和第三方软件费用。先以 10 至 20 名付费用户验证支持成本，再决定是否调整价格，避免一开始承诺无限终端、无限存储或无限支持。

## 试用流程

1. 申请人提交邮箱、地区、用途、预计工作区大小和所用 Harness，不提交模型密钥。
2. 人工审核后创建独立 OS 用户、运行目录、端口、服务单元、工作区和制品目录。
3. 为该实例生成独立域名、TLS 证书、随机初始密码、本地自动化令牌和会话密钥。
4. 发送 7 天试用邀请；首次登录后恢复密码文件自动删除。
5. 到期前 48 小时提醒。用户选择付费、导出或删除，不默认续费。
6. 未付费实例进入 7 天只读导出期，随后删除；备份按公布的保留期清除。

## 强制隔离边界

- 不给陌生客户共享当前 `fpsq.xyz:11112` 管理员实例、管理员 Cookie 或一个公共账号。
- 每位客户至少使用独立 OS 用户、webClx app 目录、systemd service、监听端口、工作区根目录和模型配置目录。
- 生产入口只暴露 TLS 反向代理；源端口由防火墙限制。反向代理不能伪造 loopback 身份。
- 模型凭据由客户直接写入自己的隔离实例，不经终端消息、工单、聊天或日志传递。
- 备份按客户加密和分目录保留；恢复测试不得写入另一客户的工作区。
- 备份收件人必须是客户确认的 GPG 公钥指纹；只备份工作区，不把构建制品、登录凭据或服务 secret 混入客户归档。恢复必须校验 SHA-256，并落入独立空目录供人工核对后再决定迁移。
- 宿主无法提供硬配额时，必须定时测量工作区和制品用量；超限操作先停止实例，再冻结数据目录。应用级闸门不能宣传成文件系统硬配额，也不能替代宿主总容量告警。
- 管理操作记录客户、操作人、时间、目标实例和结果；日志不得记录密码、Cookie、API key 或本地令牌。
- 当前单管理员登录模型不适合把多人放进同一实例。团队版上线前必须增加成员身份、最小权限和审计能力，或继续使用每人独立实例。

## 上线前业务清单

- 发布隐私说明：收集字段、用途、保存期限、处理方、导出和删除渠道。
- 发布服务条款：可接受用途、AGPL/商业许可边界、模型费用、停服和数据处理方式。
- 发布退款规则：首次购买 7 天内可退款；已发生的第三方资源费用可按事先披露扣除。
- 公布支持时间：试运行期仅工作日支持，不写“7x24”或“99.9% SLA”。
- 配置支付订单、发票/收据、续费提醒和人工退款台账；未完成前只接受人工邀请试用。
- 准备事故联系方式、密钥轮换、备份恢复、客户导出和彻底删除流程。
- 每周记录激活率、首次成功终端率、7 日留存、支持工时、付费转化和退款原因。

## 首月验证指标

| 指标 | 试运行目标 |
| --- | ---: |
| 获得有效申请 | 30 |
| 开通隔离试用 | 10 |
| 首日成功连接手机终端 | 80% |
| 7 日内至少使用 3 天 | 50% |
| 试用转付费 | 20% |
| 每位付费用户月支持工时 | 小于 1.5 小时 |
| 严重数据或认证事故 | 0 |

指标不足时先访谈和修复激活流程，不通过降价掩盖产品或隔离问题。

## English Summary

webClx remains free for AGPL-compliant self-hosting. Revenue should initially come from isolated hosting, deployment support, and commercial licensing. Suggested preview pricing is USD 8/month for a personal managed instance, USD 19/month for a professional instance, and USD 59/month for a small-team pilot. Model usage and infrastructure costs are separate.

Never share one administrative instance with unrelated customers. Each trial needs an isolated OS user, app directory, service, port, workspace root, secrets, model configuration, artifact storage, backups, and TLS hostname. Offer a seven-day invite-only trial, a clear export/deletion window, weekday best-effort support, and no unverified uptime promise. Publish privacy, terms, refund, support, incident, and deletion procedures before accepting unattended paid signups.
