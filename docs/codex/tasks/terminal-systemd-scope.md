# 终端 systemd scope 与 fallback

日期：2026-04-16

## 现象

网页终端新建会话时报错：

```text
创建终端会话失败: 无法创建 tmux 终端会话: Failed to start transient scope unit: Interactive authentication required.
```

## 触发条件

- `webclx` 作为 `systemd` 服务运行
- 后端在创建终端时优先调用 `systemd-run --scope`
- 当前服务上下文没有足够权限创建 transient scope，或被 polkit 拦截

## 根因判断

这是“环境触发的软件缺陷”。

- 环境因素：服务运行环境不允许当前进程无交互地创建 transient scope
- 软件缺陷：代码把“检测到 `INVOCATION_ID`”直接当成“可以安全使用 `systemd-run --scope`”，没有做失败回退

因此根因不只是部署环境，也不只是运维配置，主要问题在于后端容错不足。

## 修复内容

修改文件：

- `src/terminal.rs`
- `README.md`

修复策略：

1. 如果在 `systemd` 环境中，仍然先尝试 `systemd-run --scope`
2. 若返回权限、认证、bus 连接相关错误，则记录警告日志
3. 自动回退到普通 `tmux new-session`
4. 在当前进程内关闭后续的 scope 隔离尝试，避免每次创建终端都重复撞到同一错误

## 影响

- 修复后，网页终端可以正常创建新会话
- 若 `systemd-run --scope` 可用，仍然保留原来的隔离能力
- 若回退到普通 `tmux`，终端仍能使用，但服务重启后会话是否继续保留，取决于宿主进程管理方式

## 验证

已验证：

- `cargo test terminal -- --nocapture` 通过
- 运行中的 `webclx.service` 已更新并验证终端接口可以正常创建会话

## 后续建议

- 以后若以 `/home/beyondcy/codes2/webClx` 作为母本继续复制项目，应以本次修复后的版本为准
- 如果之后改动部署方式，也不要移除回退逻辑，因为不同机器上的 `systemd` / polkit 行为可能不同

## 服务重启启动路径

终端注册表和恢复记录在服务启动阶段同步加载，但已存在的 tmux 会话不能在 HTTP 服务监听前逐个重新 attach。`TerminalManager::new_with_environment_deferred_restore`
会先返回可服务的状态，再在后台恢复 tmux/PTY；自动继续和定时输入 runner 要等恢复任务结束后启动。

这样重启耗时不再随持久化终端数量线性阻塞服务启动，同时保留会话恢复和 runner 的先后关系。需要验证启动性能时，应分别记录 systemd 停止耗时、进程启动到 `server listening` 的耗时，以及后台恢复完成日志中的 `elapsed_ms`。

普通 SIGTERM 只更新终端输出观察快照，不扫描或中断 agent 来生成 resume 注册表；systemd 服务重启依赖独立 tmux scope 保留会话。只有用户明确调用“保存会话并关机”或“保存会话并重启服务”时，才在信号到达前执行完整 resume 探测并持久化恢复记录。完整探测会批量读取一次 tmux pane PID 和 `/proc` 进程关系，不能按会话重复扫描整个进程表。
