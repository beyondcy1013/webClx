# 静态资源位置与上线验证

## 权威目录

`/home/codes/tools/webClx/static/` 是静态资源的唯一实体目录和 Git 权威来源。
`/home/bin/webclx/static` 必须为指向该目录的绝对符号链接；当前 canonical
仓库路径为 `/home/codes/tools/webClx`。修改前检查 `readlink /home/bin/webclx/static`。
链接正确时两个路径访问同一文件，禁止在两者之间复制或 rsync。

静态修改立即对服务可见，不需要重建或重启二进制。改变脚本后同步更新引用入口
HTML 的版本参数，避免浏览器继续使用缓存；检查每个引用入口，不重复加载同一模块。
`tests/static-entry-assets.mjs` 按入口读取脚本包，可用于相关回归。

## 提交与审查

Git 直接跟踪源码 `static/` 中的文件。只提交本次修改的文件或 hunk，保留并发
工作；不要改动链接方向或创建第二份静态目录。旧记录中源码指向运行目录的布局
已不适用，部署门禁也必须按当前项目 AGENTS.md 校验运行链接指向源码。

## 验证与后端部署

- 对实际文件执行相关语法与行为检查；按项目规则经 compile API 排队。
- 回读线上 HTML，确认它引用新的资源版本；回读带版本的资源，核对 HTTP 200
  与本地权威文件 SHA-256 一致。
- 界面行为变更补充对应浏览器验收；认证或 WebSocket 失败与静态资源错误分别记录。
- 后端修改使用 compile/deploy API 和 `scripts/rebuild-and-deploy.sh`。脚本在
  源/目标目录解析一致时不复制静态文件，安装二进制后重启；部署完成仍须核对运行进程、
  产物哈希及服务健康。

原生 `<dialog>` 遮罩不可作为关闭控件，`dialog-backdrop-guard.js` 吸收遮罩 click；
从内容拖到遮罩松开的合成 click 也不得关闭。固定 overlay 使用
`bindBackdropDismissal()` 保持相同行为。
