---
name: webclx-network-security
description: webClx 网络安全、API 鉴权门禁、FRP 公网穿透与伪造回环防范子技能。处理接口免密后门排查、未授权枚举防御、401 统一熔断拦截及公网暴露治理。
---

# 网络安全与 API 鉴权

关键词：网络安全、API 鉴权门禁、FRP 穿透风险、127.0.0.1 伪回环、免密漏洞、接口遍历枚举、401 统一熔断、高熵 ID、Fail2ban。

## 事故背景与根因（2026-10-15 真实案例）

2026-10-15 15:27，外部恶意扫描器通过第三方公共 FRP 节点（`PUBLIC_DOMAIN:PORT`）发起遍历，向 `POST /api/terminal/sessions/s{0..150000}/close` 发送 15 万次请求，导致本机运行的全部 22 个终端会话被全自动逐一销毁。

### 核心安全漏洞与根因
1. **FRP 4 层 TCP 转发导致“伪本地回环”假象**：
   FRP 为 4 层 TCP 端口转发，外部公网连接在 FRP 服务器终止后，由本地 `frpc` 进程以 `127.0.0.1` 重新向本地目标端口握手。服务端看到的 `client_addr.ip()` 恒为 `127.0.0.1`。
2. **鉴权门禁盲信 `is_loopback()`**：
   代码曾错误地假设“只要来源是 127.0.0.1 就是安全的本机操作”，对 `/close` 接口设置了免密放行，导致外部扫描者通过 FRP 畅通无阻。
3. **前端静态资源泄露 API 路径**：
   公开的静态 JS 文件中明文包含了 `/api/terminal/sessions/${encodedId}/close` 示例，让未认证攻击者迅速定位攻击目标。
4. **低熵弱规律自增 ID**：
   会话 ID 采用 `s0`, `s1`, `s2` 等简单规律，让攻击者只需几行 Python 脚本即可无脑暴力遍历。

---

## 防护铁律与规约（不可违反的红线）

### 1. 绝对禁止“仅凭 IP 回环”免除任何写操作鉴权
* 严禁在任何修改状态、删除会话、执行命令的接口中使用 `if addr.ip().is_loopback()` 直接 `return Ok()`。
* 本地 CLI（如 `webclx close <session_id>`）或内部自动化工具，**必须统一携带 `X-WebClx-Local-Token` 请求头**。
* 服务端校验 Token 必须使用**恒定时间比较**（`subtle::ConstantTimeEq`），防止时序攻击（Timing Attack）。
* 浏览器操作必须校验有效的会话 Cookie（`verify_session_from_headers`）。

### 2. 全局前置鉴权墙与零知识响应（防未授权枚举）
* **前置短路拦截**：所有 `/api/*` 请求进入具体业务路由之前，在最外层 Auth Guard 中统一校验。无凭证请求一律拦截。
* **零知识统一报错**：无论目标 API 是否存在、会话 ID 是否有效，未认证请求**统一返回相同的 `401 Unauthorized` 及相同报文格式**，绝不暴露 `404 Not Found` 或 `400/500` 业务错误，使外部探测器无法测出接口或会话的存在性。

### 3. 公共穿透平台（FRP）治理规范
* **严禁裸奔暴露管理端口**：禁止在第三方公共/免备案 FRP 平台（如 `PUBLIC_DOMAIN`、各种免费 FRP 服务商）上映射 `11111`（webClx）、`22`（SSH）、`3389`（RDP）等敏感端口。此类公网节点时刻处于全网黑客扫描器的 24 小时探测之下。
* **安全的公网访问拓扑**：
  * **优选**：通过私有组网工具（Tailscale、WireGuard、EasyTier）访问，不对公网开放任何管理端口。
  * **备选**：公网访问必须前置 Nginx，配置 TLS 证书、强密码 HTTP Basic Auth 或 Cloudflare Access 白名单，绝不直连裸 TCP 服务。

### 4. 暴力枚举频控与自动熔断
* **针对 401 状态码的短时熔断（已全网落地）**：
  * **规则**：单 IP 在 5 分钟（300 秒）内产生超过 10 次 `401 Unauthorized`，立即触发熔断器，封禁该 IP 15 分钟（900 秒）。
  * **响应**：封禁期间短路拦截并返回 `HTTP 429 Too Many Requests`，携带标准 `Retry-After: 900` 响应头与友好的等待倒计时提示。
  * **良性保护机制**：只要有任意一次合法凭据（登录 Cookie 或本地有效 Token）请求成功通过，系统立即清零该 IP 的错误计数与封禁状态，绝不误杀真实用户。
* **业务高危操作频控防线（终端批量销毁保护）**：
  * **规则**：即使携带合法凭据，单 IP 在 10 秒内调用 `/api/terminal/sessions/{id}/close` 或 `DELETE` 超过 5 次，立即触发降频限制，返回 `HTTP 429`（`终端关闭或删除操作过于频繁，请稍候重试`），彻底阻断由于脚本死循环、自动化 Agent 误操作或恶意攻击导致的终端瞬间被批量清空。
* **资源标识符高熵化**：会话与资源 ID 优先采用 UUIDv4 或随机 NanoID，消除数学遍历可行性。

---

## 验证命令与测试集

在 `/home/codes/tools/webClx` 执行以下检查：

```bash
# 1. 验证未鉴权请求被严格阻断（必须返回 401 Unauthorized）
curl -s -i -X POST http://127.0.0.1:11111/api/terminal/sessions/s1/close | grep "401 Unauthorized"

# 2. 验证不存在的路径同样返回 401（无差异零知识响应，防枚举）
curl -s -i -X POST http://127.0.0.1:11111/api/nonexistent_test_path | grep "401 Unauthorized"

# 3. 验证携带有效本地 Token 能正常通行
curl -s -i -X POST -H "X-WebClx-Local-Token: $(cat /home/bin/webclx/.webclx-local-api-token)" http://127.0.0.1:11111/api/terminal/sessions/test/close

# 4. 执行自动化鉴权与管理路由回归测试
cargo test --bin webclx auth_guard
node --test tests/terminal-experience-save.test.mjs
```
