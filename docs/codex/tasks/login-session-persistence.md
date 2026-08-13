# 登录会话持久化

webClx 登录会话由两个独立边界共同保证：

- `src/login.rs` 把 HMAC 密钥保存在运行目录的 `.webclx-session-secret`，cookie 同时携带服务端签名过期时间和浏览器 `Max-Age`。服务重启不能重建仍可读取的密钥，也不能主动清除未过期 cookie。
- `static/login.js` 加载登录页时查询 `/api/auth/session`。如果浏览器仍带有有效 cookie，就立即回到安全校验后的 `next` 页面；只有过期、无效或无法验证的会话才保留登录表单。

如果用户在重启后看到了登录页，先分别验证 `/api/auth/session` 的结果和登录页的恢复跳转，不要只根据页面外观判断 cookie 已过期。部署前端修复时必须同步运行目录的 `static/` 文件。

针对性回归命令：

```bash
node --test tests/login-session-persistence.test.mjs
```
