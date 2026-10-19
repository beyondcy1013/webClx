# Android 客户端可靠性（1.8.21）

本专题记录 1.8.21 版本针对 Android WebView 客户端做的一轮可靠性收敛，
便于后续排查时区分「当前行为」与「仍开放的后续项」。

## 当前行为

### 服务器探测

- `static/terminal-server-probe.js` 只走 `fetch(mode:"no-cors")` + `AbortController`
  超时（3.5s）；定时器在 `finally` 中释放，成功、失败、abort 都不泄漏。
- 原生 `WebClxAndroid.probeHost` 同步 HTTP 桥已删除：它会用网络 I/O 阻塞 JS
  线程。`MainActivity` 不再暴露该方法。
- `reevaluate()` 内部串行化：并发调用复用同一在途探测；当前源 60s 故障宽限、
  当前源优先保持的语义不变，候选并发探测取最快可达者。

### 前台恢复

- `onResume` 不再 `webView.reload()`；返回前台时通过
  `evaluateJavascript` 派发 `pageshow` 事件，由既有的
  `refreshSessionsAfterPageResume` 刷新会话/尺寸/重连，同时调用
  `requestSourceReevaluation()` 在后台复核当前源。
- 文件选择器回调、安装权限返回与主题/font 设置路径不变；resume 不再
  reload，设计上保留 JS 侧草稿/滚动状态（未做真机逐条验证）。
- 键盘可见性：API≥30 用 `WindowInsets.Type.ime()`，更低版本保留
  `getWindowVisibleDisplayFrame` 几何估计。

### 生命周期安全

- 触及 WebView/对话框的异步回调在执行前通过 `isActivityUsable()`
  （`!destroyed && !isFinishing() && !isDestroyed()`）校验；源解析、
  源复核、`openInWebView` 与渲染器重建共用 `sourceSelectionGeneration`
  代际：每次新选择递增，旧代际的回调整体丢弃。
- `UpdateManager.check` 返回可取消 `Future` 并内部捕获 applicationContext；
  两个 Activity 在 `onDestroy` 中取消。
- APK 校验在独立单线程 `updateVerifyExecutor` 上执行，不再占用源解析
  `resolverExecutor`。
- 下载进度采用「后台线程只做 `DownloadManager.query`、结果回主线程判定」的
  单发轮询：worker 捕获不可变的 generation+downloadId，主线程处理器校验
  代际/下载 ID/对话框存活/activityResumed 后，仅在同代际时才以约 500ms
  重新排队下一次查询；暂停、关闭、销毁即递增代际停止，回到前台且对话框
  仍在时自动恢复，不会在 worker 上堆积无限任务。
- 更新提示只在 resumed 状态弹出；暂停期间到达的 manifest 记入
  `pendingUpdateManifest`，下一次 resume 时统一展示。

### 更新事务身份

- 校验前在 `UpdateManager` 偏好锁内取得不可变
  `PendingUpdateSnapshot`（downloadId/sha256/size/versionCode），pending id
  已变更时返回 null；后台校验器只读该快照，不访问偏好。
- 暂存文件用 `File.createTempFile("verified-webclx-update-<id>-", ".apk", …)`
  每次校验唯一；失败/过期时只删除本 attempt 新建的那一个文件。
- 校验返回 `(stagedPath, error)`，校验器内不写 SharedPreferences；主线程仅在
  pending id 仍是同一 ID 时通过 `UpdateManager.commitVerifiedApk` 原子提交，
  否则清理本次新建的暂存文件。提交可以在后台完成，`installPendingApk` 只在
  `activityResumed` 时真正拉起安装器，已验证结果顺延到下一次 resume。
- `enqueue`/`rememberPendingApkDownload` 会清除 `VERIFIED_APK_PATH`、
  verified id 与 installer id；`clearPending(context, id)` 只在 ID 匹配时清除，
  `NO_DOWNLOAD` 不是通配符。
- pending/verified 事务写经 `UpdateManager.mutatePreferences` 的 synchronized
  短临界区；`INSTALLER_LAUNCHED_DOWNLOAD_ID`、`WAITING_FOR_INSTALL_PERMISSION`
  等单字段写仍是普通 `apply()`（单字段、非事务）。
- 两个入口的 `enqueue` 失败都会给用户可见提示；安装器仍然每次下载只自动
  弹一次（`INSTALLER_LAUNCHED_DOWNLOAD_ID`）。

### 渲染进程崩溃恢复

- API≥26 实现 `onRenderProcessGone`：返回 `true`、作废 file chooser、把
  失败 WebView 从父容器移除并 destroy，再新建并按原路径配置
  （`createWebView` + `configureWebView`）。
- 仅恢复跟踪过的 `lastLoadedUrl`（onPageStarted 时仅记录当前选中源——含
  `openInWebView` 自定义源——或本机 harness 端口页面），否则走
  `beginSourceResolution`；不调用失败 view 的 reload；旧 view 与当前
  `webView` 不一致时不动现有状态。
- 连续崩溃且未完成一次成功页面加载超过 `MAX_RENDER_PROCESS_RETRIES` 次时
  停止自动重建，回到连接失败面板由用户重试；显式重试会清零崩溃计数，
  自动恢复不清零。
- `onSaveInstanceState`/`onCreate` 保存并恢复 selectedSource、
  selectedSourceOrigin、lastLoadedUrl；恢复前由
  `isRestorableSelectedPage` 校验 origin 为合法 http(s)、无 userinfo、
  URL 与 origin 同源（或同 host 的 harness:3080）、持久化 source 下标与
  `sourceIndexForOrigin` 一致（自定义源为 -1），全部通过才赋值加载。

### 重连退避

- 每个终端上下文独立计数 `reconnectAttempts`，延迟为
  `min(30s, base * 2^attempts + jitter)`，jitter 仍由 sessionId 哈希确定；
  活动上下文 base 1500ms，后台 1800ms。
- 计数只在 `endTerminalBacklogReplay` 非中断完成时清零（`preserveInterrupted`
  路径保留计数）；单纯 WS open 不再重置，避免抖动连接反复回到最小延迟。
- 页面 hidden 或设备 offline 时不再排定时器：已有定时器在 hidden/offline
  事件里被取消并标记 `reconnectDeferred`（不消耗次数），定时器回调触发时
  也会再次检查可见性/在线状态；回到前台/上线时活动上下文取消残余定时并
  立即重连，后台上下文走正常退避。
- 用户显式 select/connect 不受退避限制，输入队列保持不变。

### Android 终端上下文缓存

- `createTerminalSessionCache` 新增 `maxContexts`（默认 `Infinity`，桌面端
  不变）与 `canEvictContext`；仅当 `terminalRendererType() === "dom"`
  （Android WebView）时上限为 5。
- LRU 按创建/激活排序；裁剪只针对非活动、非本次新建、且
  `canEvictContext` 通过（无 pendingInputChars/输入队列/在途写/回放/
  retainedTerm）的上下文。全被钉住时允许软超额，等下一次 trim。
- 预算覆盖 preconnect 创建的上下文；Android 端已关闭自动后台 preconnect
  （保留显式会话切换），避免逐出后立即回填的抖动。
- `trimTerminalSessionCache()` 暴露在 `terminal-shell-settings.js`，除创建/
  激活外，在输入 flush、输出解析完成、回放结束等 settle 点保守触发，让软
  超额在不再被钉住时回收。

## 仍开放的后续项

- HTTPS / 桥接源边界：当前候选源与 harness 仍以 HTTP+cleartext 白名单为主，
  桥的原子权限边界（origin policy）未重新设计。
- 输出流控：尚无按字节丢弃的终端输出上限或背压协议，本版只修生命周期与
  重连，不处理大输出内存。
- 渲染器基准：dom/canvas 渲染器在 Android 上的对比没有性能数据，本版
  维持 `terminalRendererType()` 现状，不做切换。

## 验证记录

- 本次 JS 门禁：`node --check` 覆盖全部改动模块；定向 `node --test` 48 项
  通过（android-client-contract、server-probe、reconnect-backoff、
  session-cache、background-preconnect、input-reconnect、
  deleted-session-reconnect、workspace-mobile-regressions，以及按名称过滤的
  server-switch 子集，未包含与本改动无关的 navbar 排序基线失败）。日志：
  `/tmp/android-opt-tests.log`。
- `tests/android-update-state-jvm.sh` 提供 JVM 行为回归（编译真实
  `UpdateManager.java` + `tests/fixtures/android-update-state/` 下的最小
  Android 编译 stub；stub 只为编译与内存态存在，不代表设备数据），覆盖
  stale-id 拒绝、快照不可变、NO_DOWNLOAD 非通配、路径越界拒删等。该脚本
  是编译步骤，随编译队列一起跑，本地未执行，结果以队列为准。
- Android `assembleRelease`/`lintRelease` 走编译队列；本机存在 Chrome for
  Testing（`/home/codes/third_party/browser-tools/bin/chromium`），
  `tests/terminal-session-cache.browser.py` 冒烟结果：桌面 canvas 与
  Android+移动视口 dom 两条组合全量断言通过（含 session-b 可见性序列、
  Android 禁止后台 preconnect 断言、截图
  `/tmp/webclx-terminal-cache-desktop.png`、`/tmp/webclx-terminal-cache-mobile.png`）；
  桌面 RECONNECT 组合通过；Android+RECONNECT 组合在 1000ms 重绘等待处
  超时，HEAD 基线同样失败，属既有测试组合限制而非本版回归。
- JVM 行为回归已由编译队列执行：真实 `UpdateManager.java` + 最小 stub
  编译通过，全部 27 项检查通过（队列日志为准）。
- 队列 lint（`lintRelease` 已并入 `scripts/build-android-apk.sh` 同一 Gradle
  调用以共享 SDK 环境）曾报 `UnspecifiedRegisterReceiverFlag`：下载完成
  广播接收器改为 `ContextCompat.registerReceiver(..., RECEIVER_EXPORTED)`，
  移除旧的无 flag 重载分支，EXPORTED 语义不变以接收 DownloadProvider
  跨进程广播。
- 最终队列 request232557 全量通过（日志
  `/home/bin/webclx/compile/runs/20261004T232602-2870011-3087/build-1.log`）：
  contract 24 项、JVM 27 项、assemble+lintRelease 成功；产物
  `android/app/build/outputs/apk/published/webClx-1.8.21.apk` 本机复核
  （`/tmp/android-1821-verification.log`）：size 133686，sha256
  40b16632…4de7，packageName com.webclx.app，versionCode 1008021，
  versionName 1.8.21，v2/v3 签名，signer SHA-256
  915c0c49…5ec4 与既有发布密钥一致。
- 发布已完成并实测：artifact ID 1791127641326918880，sha256/size 与构建
  产物一致；更新清单与完整 APK 下载在本机（localhost:11111）、局域网
  （HOST:PORT）与公网（PUBLIC_DOMAIN:11112）三条路径均验证可取。
  发布后 webClx 清单曾含 1.8.21（1791127641326918880）、1.8.20
  （1787300692092056720）、1.8.19（1787298042450438948）；1.8.18 已按
  保留策略自动移除。
- 旧版本清理经受控删除 API 完成：新增受保护端点
  `DELETE /api/artifacts/{artifact_id}`（deploy234544，
  `/home/bin/webclx/compile/runs/20261004T234549-3064057-7497/build-1.log`
  artifacts::tests 25 项通过，安装哈希与进程一致，服务已重启生效），要求
  请求体携带精确匹配的 `project`/`name`，并要求
  `x-webclx-confirm-artifact` 头与路径 `{artifact_id}` 完全一致；删除前
  校验产物路径仍在受管项目目录内且不是符号链接，并拒绝删除同项目最新
  产物、当前 Android 更新清单指向的最高版本产物与最新 Windows 清单产物。
  已按授权执行两次删除：1.8.20（1787300692092056720）与 1.8.19
  （1787298042450438948）均返回 `deleted:true, file_deleted:true`；旧
  下载路径已 404，webClx 目录仅余
  `1791127641326918880__webClx-1.8.21.apk`，其他项目产物目录记录清理前后
  逐条一致，LAN/公网更新清单与完整 APK（size 133686，sha256
  40b16632…4de7）未受影响。
