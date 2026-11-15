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

## 同域名多端口实例与服务器页（2026-10-25）

浏览器 Cookie 按主机和路径匹配，不按端口隔离。旧实例都写 `webclx_session; Path=/`，服务器 iframe 登录同域名另一端口后会覆盖宿主 Cookie，宿主随后因签名不匹配要求重新登录。单纯保持签名密钥或修复前端跳转无法解决名称冲突。

- 新登录与跨实例登录使用 `webclx_session_<24位十六进制实例标识>`。标识由持久会话密钥对固定命名标签计算 HMAC，不依赖请求 Host，不输出密钥，正常重启保持稳定。
- 验证优先读取本实例 Cookie；仅当该名称完全不存在时才兼容旧共享 Cookie。`GET /api/auth/session` 将有效旧值复制到实例 Cookie，保留原签名过期时间；不会续签或写其他实例 Cookie。
- 退出写入本实例的无效标记以阻止旧 Cookie 回退，不删除共享旧 Cookie，也不改其他实例登录。标记保留最长受支持会话期限（365 天），避免设置缩短会话期限后旧值复活。修改密码仍轮换签名密钥，只清除轮换前本实例 Cookie。
- `terminal-server-switch.js` 先完成宿主会话检查/迁移，再发起一次 iframe 导航；不再先加载目标再跳登录。连接序号阻止较早请求覆盖用户后选的服务器；同源连接不申请跨实例票据。返回终端使用 `views.frame`，避免引用作用域外变量。
- `preset_sync.rs` 保留远端登录实际返回的 Cookie 名，兼容新实例 Cookie 与旧服务，避免远端预设读取仍写死旧名。

回归入口：`cargo test --locked login::tests::`（命名稳定与实例区分、旧值迁移、共享值覆盖后宿主仍登录、退出后不得回退）、`cargo test --locked auth_guard::http_tests::`、`node --test tests/terminal-server-switch.test.mjs tests/login-session-persistence.test.mjs`（九项通过，含宿主迁移前不得导航及连续切换去旧结果）。后端修改须经编译部署回调后再核对线上行为。

部署验收补充：请求 `122029-18e18bb3a984f0a5` 的登录 17 项、鉴权 3 项及 release 构建均通过，但安装前 Android 终端输入回归被首次语言选择弹窗挡住，尚未替换运行二进制。回归脚本现先实际点击中文完成首次流程，再测试输入与滚动；单独重现通过。复用该次已构建 SHA256 `335d45ef2be7c5eb832de32c7db07781b7f0a6fd9c4de3c2aa3656f4f9ef937c`，继续经部署 API 执行完整安装门禁，不跳过测试。

后续安装请求 `122506-18e18bb3a984f0a6` 的 Android 门禁通过；重连门禁因轮询错过短暂的 `outputWriteInFlight` 状态超时。该重叠场景现仅在首次重连创建新 xterm 后扣住一次真实写入的完成回调，并等待测试切换后释放回调与回放结束帧，避免采样时序依赖；后续缓存淘汰重建不扣回调。单独重连回归通过，原有画面保留、滚动与输出断言保持不变。

上线验收：请求 `123105-18e18bb3a984f0a7` 于 2026-10-25 12:33 完成；完整终端滚动门禁与图标门禁通过，安装审计仅修改二进制。`webclx.service` 为 active，运行 PID 3071790 的 `/proc/<pid>/exe` 与已验证产物 SHA256 `335d45ef2be7c5eb832de32c7db07781b7f0a6fd9c4de3c2aa3656f4f9ef937c` 一致，HTTP 返回的服务器切换 JS 与源码一致。用仅驻留内存的短期测试 Cookie 对运行服务验证：旧值迁移保留原签名值、共享 Cookie 被模拟远端覆盖后本实例仍认证成功、退出只写本实例标记、有效旧值不得绕过退出标记、无 Cookie 请求仍未认证。该验收未操作用户浏览器 Cookie，也未部署其他服务器；已经被覆盖或过期的旧登录无法凭空恢复，需要重新登录一次。
