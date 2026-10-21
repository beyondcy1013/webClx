# 登录会话持久化

webClx 登录会话由两个独立边界共同保证：

- `src/login.rs` 把 HMAC 密钥保存在运行目录的 `.webclx-session-secret`，cookie 同时携带服务端签名过期时间和浏览器 `Max-Age`。服务重启不能重建仍可读取的密钥，也不能主动清除未过期 cookie。
- `static/login.js` 加载登录页时查询 `/api/auth/session`。如果浏览器仍带有有效 cookie，就立即回到安全校验后的 `next` 页面；只有过期、无效或无法验证的会话才保留登录表单。

## 管理接口与网关认证

- `src/routes/mod.rs` 把页面和管理 API 放在 `require_auth` 后面。有效会话可访问两者；未登录页面跳转 `/login`，管理 API 返回 JSON `401`，避免调用方把登录 HTML 当成 API 结果。
- 本机自动化使用运行目录 `.webclx-local-api-token` 的令牌，通过 `X-WebClx-Local-Token` 请求头提交。只有真实连接来源为 loopback 且令牌匹配时才能访问管理 API。来源地址不读取 `X-Forwarded-For`，远程请求携带本机令牌也不获得权限。
- 登录、会话查询等 public 路由保持独立。`src/routes/gateway.rs::proxy_routes` 只注册模型转发入口，由其 handler 执行 `gateway_listen_non_loopback` 和上游凭据规则；代理预设与配额管理仍在管理认证层内。没有按 `/api/*` 前缀统一放行的逻辑。
- 本机管理令牌在进入业务 handler 或网关前移除，不转发给模型服务。网关不需要浏览器 cookie，现有模型客户端的调用路径不变。
- `src/auth_guard/http_tests.rs` 通过真实路由覆盖远程、IPv4/IPv6 loopback、错误令牌、有效会话、伪造转发头、未登录写入请求以及网关来源隔离。

如果用户在重启后看到了登录页，先分别验证 `/api/auth/session` 的结果和登录页的恢复跳转，不要只根据页面外观判断 cookie 已过期。部署前端修复时必须同步运行目录的 `static/` 文件。

针对性回归命令：

```bash
node --test tests/login-session-persistence.test.mjs
```
