# webClx 1.8.10

发布日期：2026-08-14

webClx 1.8.10 是开源试运行的翻译一致性补丁。它保留 1.8.9 的认证安全边界、版本化源码发布和内置终端通讯 Skill，并修复英文 Agent 页面中动态会话标题仍显示中文的问题。

## 变更

- 将动态标题 `新 Agent 会话` 翻译为 `New Agent session`。
- 浏览器 i18n 回归现在扫描动态 Agent 会话标题。
- 运行时字典测试固定该中英文映射。
- 版本升级至 1.8.10；未覆盖或静默修改已经发布的 1.8.9 归档。

## 发布物

| 文件 | SHA-256 |
| --- | --- |
| `webClx-1.8.10-source.tar.gz` | `63d28f36483aa5278dd5f33cf802a73d011ee42f20cbe96cd613ea9eec2ce839` |
| `webClx-1.8.10-source.tar.gz.sha256` | `328796dc3324f50ca79262e8d7ffee56ebf25bbf6830d143df3e2adbe9d5578c` |

归档内 `SOURCE_RELEASE`：

```text
version=1.8.10
commit=8da7339fdede
created_utc=2026-08-13T21:47:13Z
```

归档内 `STATIC_ASSETS_MANIFEST.sha256` 精确覆盖 111 个静态文件。`static/i18n.js` 的 SHA-256 为 `03d3401fa35eaad8be2153f6f5a2c9210886ef25a21971e703b6c50ef33b1b69`。

## 验证

- Rust workspace 所有测试通过；主程序测试为 415 通过、1 忽略、0 失败。
- 发布安全、客户端重定向、i18n 和内置终端通讯 Node 测试为 12/12 通过。
- release 构建成功，部署后二进制与候选 SHA-256 均为 `7438b8ac19a7283b61bfb2c9519dcb22ee4c0372728de090b754331a41514f8b`。
- 本地认证矩阵：无令牌 401、错误令牌 401、正确 loopback 令牌 200、正确令牌经非 loopback 地址 401。
- 真实认证后对 workspace、terminal、agent、login 进行桌面 1440x900 与手机 375x812 验收，共 8/8 通过；无未翻译控制、横向溢出、控制台错误或失败请求。

## 已知限制

- 当前为开发者预览。webClx 可以编辑文件和执行终端命令，不应把管理端口直接暴露到公网。
- DeepSeek Harness 的进程识别、Skill 安装和终端投递已验证；官方模型亲自执行 Skill 仍需要用户自己的 DeepSeek API 凭据。
- 当前单管理员实例不适合多个陌生客户共享。托管试用必须一客一实例，并使用独立 DNS、TLS、用户、目录、服务、工作区、凭据和备份。
- 公共 Git 仓库地址尚未配置；在此之前以版本化源码归档、许可证和校验文件作为发布来源。

## License

源码采用 GNU AGPL-3.0-or-later。无法履行 AGPL 网络源码义务的组织可另行协商商业许可、部署和支持服务。
