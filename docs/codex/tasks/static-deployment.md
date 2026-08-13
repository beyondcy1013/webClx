# 静态文件源码与部署副本

## 现象与根因

代码已经修改，但页面仍缺少功能或保持旧样式，通常不是浏览器缓存，而是运行目录中的静态文件落后。

- 源码目录：`/home/codes/webClx/static/`
- 服务运行目录：`/home/bin/webclx/`
- 实际读取目录：`/home/bin/webclx/static/`

后端通过 `current_dir()/static` 读取资源。只修改源码目录，或只更新二进制，都不会自动更新运行中的前端。

## 当前处理方式

使用 [webClx rebuild skill](../skills/webclx-rebuild/SKILL.md) 的 deploy API。部署脚本会统一完成：

1. 构建 release 二进制。
2. 安装实际 `cargo metadata` 指向的产物。
3. 同步整个 `static/` 目录，而不是维护单个文件白名单。
4. 重启并检查 `webclx.service`。

不要把 `/home/bin/webclx/static/` 当成源码目录，也不要只在那里做长期修复；下一次部署会覆盖它。

工作树存在其它未发布的静态改动时，先用 `rsync -ani --delete static/ /home/bin/webclx/static/`
确认差异。若全量同步会捎带无关改动，可通过 deploy API 调用
`scripts/deploy-static-assets.sh <relative-file>...`，仅安装本次已验证的文件；脚本会拒绝绝对路径和
`..` 路径，并逐文件 `cmp` 验证。正常的干净发布仍使用全量同步，避免运行目录长期漂移。

## 排查

```bash
systemctl show webclx.service -p WorkingDirectory -p MainPID
diff -qr /home/codes/webClx/static /home/bin/webclx/static
```

如果只想核对某个资源，可比较源码和部署副本的 `sha256sum`。确认部署副本一致后，再检查浏览器缓存、Service Worker 或反向代理缓存。

## 验证

- 页面引用的所有本地脚本和样式都能返回 HTTP 200。
- 源码与部署副本内容一致。
- 页面加载后无静态资源 404 和 JavaScript 初始化错误。
