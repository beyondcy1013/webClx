# 公开发布脱敏与仓库边界

关键词：PUBLIC_RELEASE、脱敏快照、SKILL.md 禁令、skill 不上传、frpc 真实地址与 token、include_dir 内嵌 static、cargo fmt 三路合并、filter-repo、refs/pull 残留、quoteNetzipRs 私有化、lyyNote 停发 APK。

## 发布包必须从脱敏树构建

- webClx 的 `static/` 经 `include_dir!` 编进二进制；用私有树构建会把真实域名和端口带进发布物。
- 发布树 = 上次公开快照 + 增量（对 `cargo fmt` 后的 master 做逐文件三路合并）。二进制文件（ico/png）不能参与文本合并，曾把 icon 合并成 0 字节，导致 Windows 交叉编译 `windres: unexpected EOF`。
- `scripts/package-source-release.sh` 负责裁剪；发布前复核 `tar -tzf <包> | grep -cE '(.codex/skills|SKILL\.md)'` 必须为 0，并 `tar -xzOf <包> <dir>/SOURCE_RELEASE` 确认版本与提交。

## skill 禁令

- 公开仓库与 release 资产一律不含 `.codex/skills` 与任何 `SKILL.md`；脚本已 `rm -rf` + `find -name 'SKILL.md' -delete` + `rmdir .codex`。
- `docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh` 是运行时依赖（`src/compile_service.rs`），只能删 SKILL.md，不能删 scripts。
- 依赖被删技能脚本的测试要从发布包一并剔除，否则用户在公开包上跑测试会失败。

## 夹具与文档脱敏

- frpc TOML 夹具曾写入真实 `serverAddr`/`serverPort`/`auth.token`；一律改为 `frp.example.com` / `example-token` / `7000`。
- 内网地址统一用 `192.168.1.x` 示例值；运行时默认值（如 `DEFAULT_GATEWAY`、脚本 CLI 默认）改动会影响行为，需单独评估。
- 协议取证、真实账号记录等目录（如 `docs/forensics`、`windows_debug`）不进公开仓库。
- 交付前查三处：源码文本、发布二进制 `strings`、APK 的 dex 与 `lib/*/*.so`。

## 历史与 refs

- 下线旧快照：`gh release delete --cleanup-tag` + 删分支；要彻底抹除用孤儿提交重建 `main`（树保持字节一致），再重新打 tag。
- GitHub 仍保留 `refs/pull/N/head` 与 dangling commit，CLI 删不掉，只能提交 Support 工单或重建仓库。
- 凭据泄漏先轮换（如 FRP token），清理历史只是防御纵深，不是止损手段。

## 跨仓库治理（2026-10-21）

- `quoteNetzipRs`：含 15MB 行情协议取证、真实账号记录、`.agents`/`.codex`/`.grok` 与内网地址 → 仓库转私有并删除这些目录；本地 537MB 未跟踪抓包移出仓库而非删除（`/data/archive/`）。
- `netzip-fullpull-suite`：删除 `.agents/skills/` 并去掉共享 skill 链接。
- `lyyNote`：服务端地址改为运行时读 `LYYNOTE_SERVER_URL` / `LYYNOTE_LAN_SERVER_URL`，`LYYNOTE_PUBLIC_BASE_URL` 改为必填；Android 读不到环境变量，因此不再打包 APK（GitHub release 与 Gitee 仓库的 APK/EXE 资产一并清空，只留 Windows EXE 通道）。
- 客户端更新链路是三级回退（服务器清单 → GitHub raw → Gitee raw），删掉后两级只影响兜底；服务器仍托管 APK 时老客户端照常更新。
