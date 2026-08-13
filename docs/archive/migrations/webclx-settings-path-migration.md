# webClx 设置路径迁移

> 历史迁移记录：仅用于追溯旧设置路径兼容问题。

## 结论

webClx 的设置路径迁移要同时检查运行副本和仓库副本：

- `/home/bin/webclx/webclx-settings.json`：`webclx.service` 实际读取的运行配置，服务 `WorkingDirectory=/home/bin/webclx`。
- `/home/codes/webClx/webclx-settings.json`：仓库侧设置副本，用于后续同步和追踪。

常见字段：

- `favorite_paths[].path`
- `workspace_history[].path`

## 2026-05-27 修复

- 将两个设置文件里的旧 BaiduSyncdisk 本地路径全部改为 `/home/cycodes`。
- 保留迁移前备份：
  - `/home/bin/webclx/webclx-settings.json.bak-20260527-cycodes`
  - `/home/codes/webClx/webclx-settings.json.bak-20260527-cycodes`
- 确认 `/home/cycodes` 当前由 `//192.168.3.38/codes` 挂载。

## 验证

- `jq . /home/bin/webclx/webclx-settings.json`
- `jq . /home/codes/webClx/webclx-settings.json`
- `rg -n 'BaiduSyncdisk|BaiduSyncDisk|/home/cycodes' /home/bin/webclx/webclx-settings.json /home/codes/webClx/webclx-settings.json /etc/systemd/system/webclx.service`
- `systemctl restart webclx && systemctl is-active webclx`

说明：2026-05-27 后续清理已同步更新仓库内相关备份文件，搜索路径残留时不需要再排除 `*.bak*`。
