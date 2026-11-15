# 下载中心 Android 安全区域适配

## 范围与验收

2026-10-25 用户明确要求「所有安卓应用都适配并发布」。执行者为当前
webClx Codex 终端（s12188 / webClx_11_resume）。范围为下载中心安卓应用的
真实源码项目；保留历史维护备份，不把同包名旧分组当成新应用重复修改。
不改应用 ID，不生成替代签名，不重启不相关后端。源码缺失时记录阻塞，
不从 APK 反编译重造项目，不恢复用户已删除文件。

适配基准见 [Android 可靠性](android-client-reliability.md)：系统栏、刘海、
IME 统一所有者，每边取最大值，原始边距幂等，避免原生与 WebView 重复扣减。
不同栈及沉浸全屏单独处理。已有正确适配不机械替换；原生真机反馈单独记录。
每次发布递增版本，沿用原证书，经队列执行定向测试、assemble/lint、签名及
产物核验，再发布下载中心和应用真正使用的更新源，完整下载校验哈希。

截至2026-10-25 03:38：43个APK分组中，22款应用（含用户已确认的webClx）及
4个同证书历史分组已发布并核验实际更新源；16个分组仍有明确阻塞，1个维护备份
保留不升级。全部可用源码已排查，不能把这次结果表述为全部应用均已升级。

## 来源清单（2026-10-25 起始快照）

通过带本地认证的 `/api/artifacts` 查询，共 43 个包含 APK 的分组。
下表版本来自已发布 APK 内部 badging，路径按现有源码重新定位。
“待适配”只表示已定位，尚未构建或发布。

| 下载中心主分组 | 当前版本 | 源码项目 | 状态/历史分组 |
| --- | --- | --- | --- |
| webClx | 1.10.24 | `/home/codes/tools/webClx` | 已发布，用户确认顶部遮挡修复 |
| dingBot | 0.1.5 | `/home/codes/apps/dingBot` | 已发布，LAN/公网完整下载验证通过 |
| GoofishMonitor | 1.2.3 | `/home/codes/apps/ai-goofish-monitor` | 已发布，双源完整下载通过；OxAlphaApp证书不同，不能覆盖 |
| zcode-rs | 0.1.2 | `/home/codes/apps/zcodeRs` | 已发布，双源完整下载通过；zcode旧分组证书不同 |
| lyyNote | 0.1.94 | `/home/codes/apps/lyyNote` | 0.1.95 适配已提交，既有服务地址契约阻塞发布，见下文 |
| patheticMan | 0.1.7 | `/home/codes/apps/patheticMan` | 已发布，独立 HTTPS 清单及完整下载验证通过 |
| remoteDesktop | 0.1.25 | `/home/codes/apps/remoteDesktop` | 已发布，LAN/公网及独立HTTPS完整下载通过 |
| youDie | 0.1.20 | `/home/codes/apps/youDie` | 已发布，LAN/公网完整下载验证通过 |
| openmaic | 0.1.15 | `/home/codes/study/openmaic` | 已发布，双源完整下载通过；/data冻结安卓快照构建，未切换课堂/TTS |
| aiSchool | 0.1.10 | `/home/codes/study/aiSchool` | 已发布，局域网/公网更新源完整下载核验通过 |
| sub2api | 0.1.187 | `/home/codes/ai/sub2api` | 已发布，双源完整下载通过；未部署主/免费后端 |
| parentWise | 0.1.5 | `/home/codes/study/parentWise` | 已发布，双源完整下载通过；kidsStudy同证书旧分组亦已同步 |
| lyyData | 0.1.5 | `/home/codes/tools/lyyData` | 已发布，下载中心双源及独立HTTPS完整下载核验通过 |
| lifeConsult | 0.1.1 | `/home/codes/study/lifeConsult` | 已发布；原生菜单Toolbar及正文同一安全区，下载中心双源及独立HTTPS完整下载通过 |
| stockOne | 0.2.19 | `/home/codes/lyyStock/stockOne` | 0.2.20适配已提交；原签名文件丢失，未发布 |
| creator-workbench | 0.1.13 | `/home/codes/media/creator-workbench` | 已发布，HTTPS双源完整下载通过；同证书旧分组亦已同步 |
| creator-planner | 0.1.0 | `/home/codes/media/creator-planner` | Android 目录存在，未发现构建文件/Activity，源码待找 |
| cyMusic | 0.1.28 | `/home/codes/cyMusic` | 源码仅0.1.25、签名文件未找到；不能用旧源码冒充最新版升级 |
| newsKB | 0.1.15 | `/home/codes/media/newsKB` | 已发布，下载中心双源及独立HTTPS完整下载通过 |
| deepseekHarnessRs | 0.1.3 | `/home/codes/dshRust` | 已发布，LAN/公网完整下载通过；旧 deepseekHarness 须核证书 |
| deepseekHarness | 0.1.1 | `/home/codes/ai/DSH` | 证书与Rs不同，且Android构建文件缺失；未发布 |
| stockJiepan | 0.1.5 | `/home/codes/lyyStock/stockJiepan` | 0.1.6适配已提交；候选证书不匹配，禁止发布 |
| loongMsg | 0.1.0 | `/home/codes/lyyStock/loongMsg` | 目录仅剩部分 Android 资源，Activity/Gradle 待找 |
| LosslessQuickCutAndroid | 0.1.3 | `/home/codes/media/lossless-cut` | 已发布，同证书旧分组同步；两分组完整下载通过 |
| stockScreener | 0.1.28 | `/home/codes/lyyStock/stockScreener` | 0.1.29适配已提交；原签名文件缺失，未发布、未部署选股服务 |
| SnapWC | 1.0.1 | `/home/Testing/snapwc-android` | 1.0.2适配已写源码；缺原签名口令，未构建发布；无Git仓库 |
| YunX | 1.2.6 | `/home/codes/third_party/YunX` | lint151错误修复并通过；下载中心双源及独立HTTPS完整下载通过 |
| ro | 0.1.2 | `/home/codes/third_party/ro` | 已发布，同证书ro-android旧分组同步；双源完整下载通过 |
| ZCode-Android | 1.3.2 | `/home/codes/third_party/zcodeOfficial/android` | 仅剩部分目录，Activity/Gradle 待找；ZCode 同包名同版本 |
| DeepTutor | 1.5.13 | `/home/kids/DeepTutor` | 已发布，双源完整下载通过；相机声明为可选硬件 |
| stockAgent | 0.2.7 | `/home/codes/lyyStock/stockAgent` | 0.2.8适配已提交；原签名密钥缺失，未构建发布 |
| longzijue | 0.3.7 | `/home/codes/longzijue` | 仅技能/文档，Android 源码待找 |
| ldxpQuery | 0.1.2 | `/home/codes/inbox/ldxpQuery` | 下载中心双源和独立HTTPS均已发布并完整下载校验 |
| openchatcut | 0.1.3 | `/home/codes/media/openchatcut` | 已发布，公网清单/安装包完整下载核验通过 |
| TrendRadar | 1.0.0 | `/home/codes/third_party/TrendRadar` | 仅部分 android-client 目录，Activity/Gradle 待找 |
| maintenance-backup-20260818-sub2api-0177 | 未读出 | 下载中心维护备份目录 | 保留备份，不作为独立应用升级 |

同包名分组的证书与实际更新源复核前，不跨分组覆盖发布。每批完成后在本表
记录版本、提交、队列 request_id、产物 SHA-256 和更新源验证；不把排队当完成。

## 发布证据（2026-10-25）

以下六项均已通过 release 构建、lint、原证书比对、版本递增及发布验证。
除 ParentWise 使用项目原有签名门禁外，其余还通过旧/新 APK 升级身份脚本。
Java helper 的 16 项 JVM 检查验证真实生产 helper 的逻辑，不代表所有手机真机验收。
ParentWise 使用同一 helper，运行项目构建和策略测试。六项安装审计均无 missing；
`unchanged=2` 是安装阶段 APK/验证脚本未被修改，不是未生成新 APK。

| 项目 | 版本/code | 源码提交 | 队列请求 | 产物 ID |
| --- | --- | --- | --- | --- |
| creator-workbench | 0.1.13/1013 | `7273a7f` | `024850-18e18bb3a984eff1` | `1792867775449213683` |
| openchatcut | 0.1.3/1003 | `f7ff6b7`、`347e8e1`、`384a40d` | `025237-18e18bb3a984eff5` | `1792867994477018475` |
| aiSchool | 0.1.10/1010 | 父仓库 `0639cf077` | `025355-18e18bb3a984eff6` | `1792868082627211272` |
| parentWise | 0.1.5/1005 | 父仓库 `0639cf077` | `025355-18e18bb3a984eff7` | `1792868088576639801` |
| ro | 0.1.2/1002 | `2196ab9` | `025742-18e18bb3a984eff8` | `1792868311963406679` |
| LosslessQuickCutAndroid | 0.1.3/1003 | `0e2e4d9` | `025742-18e18bb3a984eff9` | `1792868320375219295` |

下载链接统一为 `http://fpsq.xyz:11112/api/artifacts/download/{产物ID}/{项目}-{版本}.apk`，
下载中心为 `http://fpsq.xyz:11112/downloads`，清单为
`/api/artifacts/update/android/{项目}`。完整下载 SHA-256：

| 项目 | SHA-256 |
| --- | --- |
| creator-workbench | `a84afae30efddf63f5aa7aed6dfd8bdc587f6f16cfc740087f9cddd4a75535b0` |
| openchatcut | `d9cf9117278871afd594457a45f56db868e3ed6b1387f41af4c3145fbfddd9b5` |
| aiSchool | `b42bd0fd1007251d6113856dc3f714bcc70dd4ec8ffc07d9d41b52a04185c6a5` |
| parentWise | `26d0b17443365a98706a3529996b67dab460fabe2e28555639438d2d2facd4b9` |
| ro | `810f694bfb9aee3b00e0f8d4456c2791fc8652e5fb12bfc1d6e4cb06f50c899b` |
| LosslessQuickCutAndroid | `1e211511b354fd08386f75fec268a5b5897b10ce8362ddaf4140fb6322597c81` |

creator-workbench 实际客户端读取 `https://fpsq.xyz:16071` 和
`https://192.168.3.2:16071` 的同路径清单，均已按系统信任链完整下载验哈希；
Node 默认 CA 与系统 CA 不同，使用 `node --use-system-ca` 后通过，未关闭 TLS 校验。
aiSchool/ParentWise 的 `http://192.168.3.2:11111` 和公网 11112 清单/完整文件均通过。

### 发现并已处理的发布问题

- OpenChatCut 和无损剪辑签名属性遗留迁移前的绝对路径，改用项目相对的原有
  `.android-signing/*.p12`，没有重新生成证书；发布前与历史 APK 逐一比对。
- 下载中心由文件名版本推导 versionCode（major×1000000+minor×1000+patch）。
  OpenChatCut 中间版 0.1.2/code3 与清单1002不一致，已另发 0.1.3/code1003，
  未覆盖同版文件。后续项目必须同时核对实际 APK 与清单，不能只检查版本名称。

### 待完成与阻塞

新增批次证据（均为2026-10-25；公网读取不等于真机安装验收）：

| 项目 | 版本/code | 提交 | 成功请求 | 产物ID / SHA-256 |
| --- | --- | --- | --- | --- |
| deepseekHarnessRs | 0.1.3/1003 | `457d3c9` | `030121-18e18bb3a984effb` | `1792868543151913096` / `65e95e27bb5cbea900d4118f7f055992f4a5c5358b1d03387ebca25aa80315c5` |
| lyyData | 0.1.5/1005 | `fdbf70d`、`772acff`、`44eee32` | 构建 `030612-18e18bb3a984f000`；独立源 `030706-18e18bb3a984f001` | `1792868801033286292` / `785a060e8267cb354d467ac27963a42ad2151576613f130e652e3a0147270fe7` |
| remoteDesktop | 0.1.25/1025 | `a8daf519c`、`e9c6f9c7d`、`61d22a22f` | 构建 `030724-18e18bb3a984f002`；独立源 `030858-18e18bb3a984f004` | `1792868908826620253` / `57ad9efbb25d306785b38a1ccbe0bad325073b00780a639b42b7f6601d95d446` |
| dingBot | 0.1.5/1005 | `6a8bf29` | `031008-18e18bb3a984f006` | `1792869064559437939` / `af59aef8cc68a86161570614569d8d79997a3e91cac3b984c37d176f4988d18d` |
| youDie | 0.1.20/1020 | `7523501` | `031024-18e18bb3a984f008` | `1792869110622886868` / `09a7c2fbc22ef357fcde3345a3a10914254102cdc5f63c9dc49f0b9f49746cdd` |
| patheticMan | 0.1.7/1007 | `be37794` | 构建 `031023-18e18bb3a984f007`；独立源 `031417-18e18bb3a984f00c` | 独立源 `patheticMan-0.1.7.apk` / `4c3fca7a2e795e960ebf30004b022718343c883411ebd51c8606aa0644265b52` |
| sub2api | 0.1.187/1187 | `1ce6ff674`、`4e7bddf4c`、`b411faff2` | `031659-18e18bb3a984f010` | `1792869438677415230` / `b4509bd8eb74c8ee04a7d6beae8af6d1a09479c5ea22333688e593e2661848e6` |
| zcode-rs | 0.1.2/1002 | `13665e59b`、`0a8a8b2a7`、`7fa13e9d5` | `031659-18e18bb3a984f011` | `1792869463918592017` / `2f0417d65c98b6a83b2d7fc6167f050854c11647f2894594baa40af7618d63f9` |

独立HTTPS清单统一为 `https://fpsq.xyz:11113/android-updates/{项目}.json`。
lyyData审计的missing1是本地临时目录模板；remoteDesktop审计的missing1是
远端目录被本地审计器当成本地路径。均已依据HTTPS清单和完整APK下载验哈希，
不把审计missing直接解释为成功，也不把远端发布误报为安装包缺失。

- YunX历史失败（后续已修复）：`5048c9f`，1.2.6/code1002006；请求 `030919-18e18bb3a984f005`
  编译及单测完成，lint有151错误（142 RestrictedApi、5 NewApi、3缩进、1桥接）。
  SDK迁移路径已修正；不得关闭lint或直接发布绕过兼容错误。
- stockJiepan：`a632b0fe9`；请求 `030121-18e18bb3a984effc` 构建/lint通过，
  签名门禁失败。历史证书 `fd13dd75c00ec9bfb21d1eac3554b84e7ae77af5b6b3107098a9a7852ff4767d`，
  兄弟项目找到的同名密钥为 `b0cbe5058d0662b2ad4034f10f845fe1fd4f022cc4fff391378357d6cd3bcb1d`，
  不可作为升级证书。stockAgent原签名文件亦缺失，禁止自动生成替代密钥。
- newsKB历史失败（后续已修复）：前两次构建发现SDK缺少配置和night主题API27限定问题，`73b8b958`修正后构建及生产更新源均已发布。

- remoteDesktop：最终为0.1.25/code1025，父仓库提交 `61d22a22f`，
  构建请求 `030724-18e18bb3a984f002`，HTTPS发布请求 `030858-18e18bb3a984f004`。
  早期0.1.24的16071地址属于其他服务，已在0.1.25改为独立11113仓库。
- lyyNote：提交 `32fa4e8` 已完成0.1.95安全区适配；请求
  `025013-18e18bb3a984eff4` 在 client-shell 定向测试失败（41通过/1失败）。
  `src-tauri/src/lib.rs` 在本次修改前已仅从进程环境读取服务URL，Android构建
  不注入运行环境，测试仍要求原有LAN/公网候选。需要查清配置交付的真正所有者，
  不能删掉测试断言就发布可能无服务地址的客户端。当前对外仍为0.1.94。
- 同包名历史分组仍需原证书/更新源核对，尚未跨分组覆盖。
- 其余条目继续逐项目处理；源码缺失项目保留明确阻塞，不假报全量完成。

## 后续批次验收与阻塞明细

| 项目 | 版本/code | 提交 | 成功构建发布请求 | 产物ID / SHA-256 |
| --- | --- | --- | --- | --- |
| newsKB | 0.1.15/1015 | `eb3a494a`、`73b8b958` | `031912-18e18bb3a984f013` | `1792869586906573752` / `70ea7a4ac08910b6d9f79869c323f7ee25fb5e9b327386c7f452ffca550b9aa9` |
| lifeConsult | 0.1.1/1001 | `41ec437` | `032017-18e18bb3a984f014` | `1792869648093043138` / `b418d574f2a8e4ee6ad155c36a6fa99645260f61d1cb3e3b27913bb06a039b62` |
| GoofishMonitor | 1.2.3/1002003 | `b73dd07` | `032425-18e18bb3a984f01d` | `1792869913797249371` / `4b3e029c6f1a3cd376f29bbba15248b59e14edd37a82856eb6a0b48c82e63e86` |
| openmaic | 0.1.15/1015 | `d5e99090`、`f0742b17`、`03c0fc38` | `032458-18e18bb3a984f01e` | `1792869958971481152` / `785b72221cdf03a516c2a56c57cf6af2baa7d4082dff2b19759a987cd28313ba` |
| DeepTutor | 1.5.13/1005013 | `fc509cf`、`a7624b6` | `032803-18e18bb3a984f021` | `1792870137580848135` / `bb0b86d2d439d77650d1c941fc1944cee75e4d7cd79615287805c164728b74f3` |
| ldxpQuery | 0.1.2/1002 | `aef0702` | `033303-18e18bb3a984f025` | `1792870417371598595` / `f2a356ed425732568a6b1fdc9f7cec12ea668c76af9b5ef13828b26257308b3c` |
| YunX | 1.2.6/1002006 | `5048c9f`、`4131e4e`、`3320986`、`140f8a7` | `033214-18e18bb3a984f024` | `1792870470347425806` / `9b730cd76b022194507a75afa864d746af401985ce2fa52bbff7b7add1fbcaa4` |

newsKB独立源请求 `032017-18e18bb3a984f015`、lifeConsult独立源请求
`032103-18e18bb3a984f016`，均完成HTTPS清单与完整文件下载校验。
ldxpQuery独立源请求 `033401-18e18bb3a984f026` 同样通过完整下载校验；
内嵌原生库与旧APK逐字节相同，未重编译或重启查询后端。
YunX独立源请求 `033457-18e18bb3a984f027` 已完成，HTTPS清单与完整安装包
SHA-256校验通过；下载中心LAN/公网完整下载亦通过。

YunX没有使用lint基线或关闭检查：改用公开MaterialKolor配色API，API23并发集合
改用兼容方法，MediaStore私有方法标记API29且入口保留版本判断，网页接口用具体
公开字节码类注册并保留JavascriptInterface注解；修正缩进后151项错误全部通过。
JavaScript桥接泛型remember的T类型不能作为接口注解证明，注册处传具体实例。
MaterialKolor3.0.1保持TonalSpot算法及完整颜色角色；未作安卓真机配色验收。

同证书历史分组同步完成，并通过LAN/公网完整下载校验：

| 分组 | 版本 | 成功请求 | 产物ID |
| --- | --- | --- | --- |
| kidsStudy | 0.1.5 | `032141-18e18bb3a984f017` | `1792869713007326434` |
| ro-android | 0.1.2 | `032141-18e18bb3a984f018` | `1792869722233553293` |
| creator-workbench-android | 0.1.13 | `032142-18e18bb3a984f019` | `1792869727371152523` |
| lossless-quickcut-android | 0.1.3 | `032142-18e18bb3a984f01a` | `1792869733146862965` |

不能发布的16个分组按实际缺口登记，维护备份另保留：

| 分组 | 阻塞与已做工作 |
| --- | --- |
| lyyNote | 0.1.95适配提交32fa4e8；运行时仅环境变量提供服务地址，Android未交付候选配置；42项测试41通过1失败，不能删测试后发布无服务地址客户端 |
| stockOne | 0.2.20适配a000feb/权限保留ab063e7；原stockone-release.jks不存在；033005-18e18bb3a984f022因先检查旧APK版本失败，重建前须恢复原密钥并将产物合同安排到构建后；本次未生成替代密钥 |
| stockScreener | 0.1.29适配ace500a；033005-18e18bb3a984f023签名验证失败，旧绝对路径及新项目均无原stockScreener-release.jks |
| stockJiepan | 0.1.6适配已提交；同名兄弟项目密钥证书不同，746f79f9f已解除错误绑定，仍要求项目原密钥 |
| stockAgent | 0.2.8/code2008适配7268875；原密钥缺失，未构建发布；不得运行会补造密钥的原构建脚本 |
| SnapWC | 1.0.2/code1000002适配已写入app/脚本；keystore存在但未发现签名口令交付文件；无Git仓库，未构建发布 |
| cyMusic | 当前可见源码0.1.25低于已发布0.1.28且未发现原密钥；需恢复最新版源码及签名，不能回退功能后只抬版本号 |
| zcode | 历史证书f118f489…与新zcode-rs的e40f85db…不同，不跨证书发布；新版源码已适配，旧证书身份仍需恢复 |
| OxAlphaApp | 旧证书c07522ae…与GoofishMonitor的52e5d3b2…不同；项目既有说明记录旧stockAgent共享密钥已丢失，不能冒充原位升级 |
| deepseekHarness | 旧证书a543f47a…与Rs版2bf43c20…不同；/home/codes/ai/DSH/android仅剩Activity/资源，缺Gradle构建配置 |
| creator-planner | 项目仅剩部分Android/签名目录，缺Activity及Gradle构建文件 |
| loongMsg | 仅剩部分Android资源/JNI/settings，缺Activity及完整构建入口 |
| ZCode-Android / ZCode | 两分组同包名xyz.fpsq.zcode；原项目Android仅部分目录，无Activity/Gradle；与com.zcode.client不同应用 |
| longzijue | 现有目录仅技能/文档，未找到Android源码 |
| TrendRadar | 仅部分android-client目录，缺Activity/Gradle |

密钥检索限定现有项目.android-toolchain、相关/home/bin备份、/data/android-build及已知
迁移路径；缺失结论不代表不存在离线备份。源码缺失不授权恢复用户已删除文件。
原生真机视觉、旋转及从旧安装直接升级仍需实际设备反馈；服务器验证不替代真机。
