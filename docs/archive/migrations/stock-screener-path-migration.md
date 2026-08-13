# stockScreener 路径迁移残留

> 历史迁移记录：仅用于追溯旧路径兼容问题。

## 结论

股票项目工作区迁到 `stockScreener` 后，webClx 不只会在当前设置里保留路径，还可能在历史/恢复账本里保留旧项目名或旧路径。

重点检查：

- `/home/bin/webclx/webclx-settings.json`
  - `favorite_paths`
  - `workspace_history`
- `/home/codes/webClx/webclx-settings.json`
  - 仓库侧设置样本/同步配置
- `/home/bin/webclx/.webclx-codex-resume-archives.json`
  - `archives[].cwd`
  - `archives[].note`

## 2026-05-26 修复

- 将旧源码路径统一迁到 `/home/codes/stockScreener`。
- 将历史 `/home/codes/...` 路径统一迁到 `/home/codes/stockScreener`。
- 将旧 deploy 历史项迁到 `/home/codes/stockScreener/deploy`，该目录已确认存在。
- 将 Codex 恢复归档中的旧 `cwd` 和 note 前缀同步为 `stockScreener`。
- 对 `favorite_paths` 和 `workspace_history` 按路径去重，保留最新 `last_opened_at`。

## 验证

- 当前生效 JSON 中旧项目名和旧路径命中为 0。
- `webclx.service` 已重启，状态为 `active`。
- 进程环境里没有股票项目路径变量，只有动态解析的 `HOME=/home/root`。
- `cargo test -p terminal_core resume_archive_cwd_keeps_relative_workspace_path -- --nocapture` 通过。

说明：本次新增的 `.bak-20260526-stockScreener-path-migration` 和 `path-migration-backup-20260526T142947/` 保留迁移前内容，搜索旧名时应排除这些备份。
