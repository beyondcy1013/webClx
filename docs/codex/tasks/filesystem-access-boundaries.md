# 文件系统访问边界

webClx 的工作区文件 API 允许访问当前工作区及其上一层目录。这个范围是产品能力边界，不等同于只允许工作区本身。

## 稳定约束

- 默认工作目录可以是任意存在且可访问的绝对目录，不限制在 `/home` 下；切换工作区后，文件 API 的允许范围仍是当前工作区及其上一层目录。
- `/api/entries`、`/api/file` 和 `/api/file/rename` 必须通过 `src/filesystem.rs` 的统一路径解析函数进入文件系统。
- 普通文件读写先 `canonicalize`，再以 canonical 路径检查允许根；不能只检查用户输入或显示路径的字符串前缀。
- 终端 cwd 可以保留用户配置的符号链接显示路径，但任何基于会话 cwd 的文件读写都必须先调用 `canonical_directory_in_access_scope`。终端文档和粘贴资源上传遵守同一规则。
- 保存、重命名和粘贴资源上传属于状态变更操作，成功后记录 canonical 路径及必要的审计字段。

## 2026-07-15 根因与修复

终端粘贴资源上传曾直接使用会话保存的 cwd，并只做词法前缀检查。当 cwd 是位于允许范围内、但实际指向范围外目录的符号链接时，上传会沿链接写入范围外。

修复位于 `src/terminal.rs`：写入前使用 `canonical_directory_in_access_scope` 解析真实目录并重新授权。`src/terminal/docs.rs` 同步复用该 helper，避免同类校验分叉。回归测试 `canonical_directory_rejects_symlink_escape_outside_access_root` 覆盖符号链接目录逃逸。

## 验证

```bash
cargo test --workspace filesystem::tests
```

在 webClx 终端中必须通过编译 API 排队运行该命令，不能直接本地编译。
