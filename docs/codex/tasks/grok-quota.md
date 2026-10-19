# Grok Build 余量

适用代码：`src/quota.rs::query_grok`、`static/terminal-quota.js::renderGrokQuota`。

## 结论

Grok Build CLI 的余量来自 CLI chat proxy，不是 xAI public API key：

- `GET https://cli-chat-proxy.grok.com/v1/billing?format=credits`
- `GET https://cli-chat-proxy.grok.com/v1/user?include=subscription`

请求必须带当前 OIDC access token：

```http
Authorization: Bearer ACCESS_TOKEN
X-XAI-Token-Auth: xai-grok-cli
x-userid: USER_ID
x-grok-client-version: 1.0.40
x-grok-client-mode: interactive
x-grok-client-identifier: grok-shell
x-authenticateresponse: authenticate-response
```

access token 过期时，用 `~/.grok/auth.json` 里的 `refresh_token`、
`oidc_issuer`、`oidc_client_id`、`principal_type` 和 `principal_id` 调用
issuer discovery 的 `token_endpoint`，表单字段为：

- `grant_type=refresh_token`
- `refresh_token`
- `client_id`
- 可选 `principal_type`
- 可选 `principal_id`

webClx 后端在 `base_url` 命中 `grok.com` 或 `x.ai` 时走 Grok 分支。API key
参数仅是兼容占位；真正凭据仍读取 `~/.grok/auth.json`。当前实现不回写
OIDC refresh token，避免绕过 Grok CLI 的文件锁与并发刷新机制。

`/billing?format=credits` 常见字段：

- `config.currentPeriod.start/end`
- `config.isUnifiedBillingUser`
- `config.prepaidBalance.val`（美元分，可能为负值）
- `config.onDemandUsed.val`
- `config.onDemandCap.val`

当前样本响应没有 `creditUsagePercent`，因此无法可靠显示套餐余量百分比。
UI 显示周期、重置、订阅、Credits 和 on-demand 字段，并明确提示官方接口
暂未返回百分比。`/user?include=subscription.subscriptionTier` 可显示
`GrokPro`、`SuperGrok` 等实际订阅层。

旧版 1.0.40 CLI 日志中出现过
`503 cursor billing: snapshot failed`；后来用同一凭据手动刷新 OIDC 后
同一接口返回 200。若 Grok 分支报 401/403，先在 Grok CLI 内重新登录或
运行一次 CLI 让它刷新 token，再重查。
