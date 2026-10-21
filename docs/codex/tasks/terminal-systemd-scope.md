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

当前实现也会在 `server exited unexpectedly` 后关闭本进程后续的 scope
尝试并回退直启，但该字符串本身不能证明 scope 权限故障或瞬时竞态。
如果直启同样失败，必须继续检查 tmux daemon 启动条件；不要仅增加重试次数。

## `server exited unexpectedly`：先检查 `/dev/null`

Linux 下 tmux 创建命令非零退出时，后端现在会只读检查 `/dev/null` 的状态。
缺失、读取失败、非字符设备、设备号不是 `1:3` 或权限不是 `0666` 时，原始
错误后追加实际状态和修复提示，同时写入服务日志。该类错误使用独立错误类型，
立即停止本次 scope 回退和直启重试，也不永久关闭 scope 功能；管理员修复后
下一次创建正常重试。设备正常时保持原有报错与重试策略，非 Linux 不套用
Linux 设备号。诊断不执行写入、删除、chmod 或 mknod；测试使用合成元数据，
不修改宿主机设备节点。

2026-09-16 的故障发生在 tmux 后台化阶段，尚未执行 `runuser`：宿主机
`/dev/null` 变成了 `0644` 普通文件。strace 显示后台子进程打开该路径后立即
退出；在独立 mount namespace 中将普通文件 bind 到 `/dev/null`，可稳定复现
同样报错，且捕获到 `newfstatat(... S_IFREG ...)` 后 `exit_group(1)`。

- 用 `stat /dev/null` / `ls -l /dev/null` 检查：必须是字符设备，主次设备号
  `1,3`，权限 `0666`，而不是零字节普通文件。
- 确认设备节点损坏后，使用 `mknod` 在 `/dev` 创建同设备号的临时字符设备，
  设置 `0666`，再原子替换 `/dev/null`。不要仅清空文件或修改权限。
- 不要在宿主机破坏 `/dev/null` 做复现；使用独立 mount namespace，保持
  mount propagation private，并限定测试时长。
- 设备修复后通过真实 `POST /api/terminal/sessions` 在原工作目录连续创建，
  再通过列表 API 核对 `alive/connected`，通过独立 tmux socket 核对 pane PID
  的命令是 `runuser -u USER -- SHELL -l`。删除测试终端时须携带匹配的
  `x-webclx-confirm-session`，只删除本次创建的 ID。
- 本次恢复后 `/home/codes/stocks/tdxRs` 连续 5 次创建及连接全部成功，测试
  终端已清理，没有重启服务或修改启动参数。最初替换设备节点的进程尚未确定；
  如复发，应对 `/dev` 的 unlink/rename/mknod 做限时 audit，追查实际写入者，
  不应宣称重试或重启已消除上游原因。

## 影响

- 修复后，网页终端可以正常创建新会话
- 若 `systemd-run --scope` 可用，仍然保留原来的隔离能力
- 若回退到普通 `tmux`，终端仍能使用，但服务重启后会话是否继续保留，取决于宿主进程管理方式

## 验证

已验证：

- `cargo test terminal -- --nocapture` 通过
- 运行中的 `webclx.service` 已更新并验证终端接口可以正常创建会话
- 2026-09-15 使用 Agent 入口连续创建 5 个终端，全部成功；删除验证终端后，
  日志中不再出现 scope 与直启连续失败的 `server exited unexpectedly`。

## 后续建议

- 以后若以 `/home/beyondcy/codes2/webClx` 作为母本继续复制项目，应以本次修复后的版本为准
- 如果之后改动部署方式，也不要移除回退逻辑，因为不同机器上的 `systemd` / polkit 行为可能不同

## 服务重启启动路径

终端注册表和恢复记录在服务启动阶段同步加载，但已存在的 tmux 会话不能在 HTTP 服务监听前逐个重新 attach。`TerminalManager::new_with_environment_deferred_restore`
会先返回可服务的状态，再在后台恢复 tmux/PTY；自动继续和定时输入 runner 要等恢复任务结束后启动。

这样重启耗时不再随持久化终端数量线性阻塞服务启动，同时保留会话恢复和 runner 的先后关系。需要验证启动性能时，应分别记录 systemd 停止耗时、进程启动到 `server listening` 的耗时，以及后台恢复完成日志中的 `elapsed_ms`。

普通 SIGTERM/SIGINT（systemd stop/restart、整机重启/关机）现在也会自动保存会话：信号到达时先执行一次完整 resume 探测并落盘（`TerminalManager::save_shutdown_restore_registry_for_signal`，在 `finalize_output_observations_for_shutdown` 之前运行），再更新终端输出观察快照，用户不再需要先点「保存会话并关机/重启服务」菜单。自动保存绝不向工作中的 agent 发送 Ctrl-C——服务重启依赖独立 tmux scope 保留会话，保存动作不能打断正在运行的终端；探测不到 resume id 的会话本次不记录。只有用户显式调用「保存会话并关机」或「保存会话并重启服务」时，才允许 Ctrl-C 兜底并在信号到达前确认落盘。关机竞态中 tmux scope 可能先于本进程停止，导致探测结果为空；此时若仍有登记会话且所有 tmux 后端都不可达，保留现有恢复文件，不用空结果覆盖上一次成功保存的记录（2026-09-25 引入）。完整探测会批量读取一次 tmux pane PID 和 `/proc` 进程关系，不能按会话重复扫描整个进程表。

2026-09-29 复核真实关机日志发现：普通 transient scope 与 `webclx.service` 没有停止顺序，systemd 在整机关机时可能先停 scope，再向 webClx 发送 SIGTERM。此时所有 tmux socket 已经不可达，信号保存只能走“保留旧恢复文件”兜底，无法补抓当前 agent 会话。修复是创建 scope 时显式追加 `After=webclx.service`（等价地把 webclx 排在停止顺序前面），确保 webClx 完成探测与写盘后 systemd 才停止 tmux scope。该顺序只影响 shutdown/reboot 事务；`systemctl restart webclx.service` 本身不会级联停止独立 scope，终端仍保持存活。旧 scope 缺少该依赖，修复部署后新建终端才携带正确顺序；整机关机可靠性必须通过真实 poweroff/reboot 日志验证，不能用单元测试替代。
