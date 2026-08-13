# MiniMax TokenPlan 套餐字段

适用代码：`src/quota.rs::query_minimax`、`static/terminal-quota.js::renderMinimaxRemains` /
`MINIMAX_STATUS_TEXT` / `scheduleMinimaxResetRefresh`。

## 0. 核实结论摘要（2026-07-09 真实响应）

通过 `127.0.0.1:11111/api/quota/query` 带 MiniMax3 preset 的 api_key
+ `base_url=https://api.minimaxi.com/v1` 拿到真实响应：

```
{
  "base_resp": {"status_code": 0, "status_msg": "success"},
  "model_remains": [
    {"model_name": "general",
     "current_interval_remaining_percent": 94,
     "current_interval_status": 1,
     "current_interval_total_count": 0,
     "current_interval_usage_count": 0,
     "current_weekly_remaining_percent": 65,
     "current_weekly_status": 1,
     "current_weekly_total_count": 0,
     "current_weekly_usage_count": 0,
     "end_time": 1783562400000,
     "remains_time": 6300051,
     "start_time": 1783544400000,
     "weekly_boost_permille": 1500,
     "weekly_end_time": 1783872000000,
     "weekly_remains_time": 315900051,
     "weekly_start_time": 1783267200000},
    {"model_name": "video",
     "current_interval_remaining_percent": 100,
     "current_interval_status": 3,
     ...}
  ]
}
```

| 假设 | 真实情况 | 结论 |
|---|---|---|
| `remains_time` 单位毫秒 | 6300051 ms = 1h 45m，与 `end_time - now` 一致 | 对 |
| `end_time - start_time = 5h` | 1783562400000 - 1783544400000 = 18000000 ms = 5h | 对 |
| `weekly_end_time - weekly_start_time = 7d` | 1783872000000 - 1783267200000 = 604800000 ms = 7d | 对 |
| `weekly_boost_permille` 是千分比 | general=1500 ⇒ 1.5x；video=null | 对（千分比假设成立） |
| `model_remains` 在顶层（后端已 unwrap data） | 渲染路径走通 | 对 |
| `current_interval_status=1` 表示"剩余正常" | general 94% + status=1 | 对 |
| `current_interval_status=3` 表示"剩余耗尽" | **video 100% + status=3** | **错**：status 不是"剩余"语义，是"该模型对账号的可用性" |

**关键纠正**：原代码注释 `1=可用 / 2=预警 / 3=耗尽` 是错误的。
`status` 是"该模型对当前账号是否开通/启用"的状态，`3` 在剩余 100%
时出现，说明模型未对该账号开通。已把 `MINIMAX_STATUS_TEXT` 改为
`{1:"已开通", 2:"预警", 3:"未开通"}`。`2` 与 `0/4+` 仍未在真实样本中
观察到，仍走 default 兜底。

## 1. 顶层响应结构

| 字段 | 出处 | 类别 | 样本核实 |
|---|---|---|---|
| `base_resp.status_code = 0` | `terminal-quota.js:230` 注释 | 推断 | `0` 表示成功 |
| `base_resp.status_msg = "success"` | 同上 | 推断 | 字符串 |
| `model_remains: [...]` | `terminal-quota.js:231` 注释、`:245` 读取 | 推断 | 样本是顶层 key（后端已 unwrap data） |

后端 `quota.rs:337` 做 `value.get("data").cloned().unwrap_or(value)`；
若响应是 `{ base_resp, data: { model_remains } }` 也能被读到。

## 2. 每个模型的两套限额字段

| 字段 | 含义 | 类别 | 样本值 | 对账结果 |
|---|---|---|---|---|
| `model_name` | 模型名 | 应无歧义 | `general` / `video` | 对 |
| `current_interval_remaining_percent` | 5h 剩余 %（0-100） | 推断 | `94` / `100` | 对 |
| `current_interval_status` | 5h 状态码（**非"剩余"语义**） | 推断 | `1` / `3` | 对，但语义修正见 §0 |
| `current_interval_total_count` | 5h 总额 | 新发现 | `0` | 永远 0？前端忽略 |
| `current_interval_usage_count` | 5h 已用 | 新发现 | `0` | 永远 0？前端忽略 |
| `end_time` | 5h 窗口结束（ms） | ms 假设 | `1783562400000` | 对 |
| `remains_time` | 5h 剩余（ms） | ms 假设 | `6300051` | 对 |
| `start_time` | 5h 窗口开始（ms） | ms 假设 | `1783544400000` | 对 |
| `current_weekly_remaining_percent` | 周剩余 % | 推断 | `65` / `100` | 对 |
| `current_weekly_status` | 周状态码 | 同上 | `1` / `3` | 对 |
| `current_weekly_total_count` / `_usage_count` | 周总额/已用 | 新发现 | `0` | 同上 |
| `weekly_end_time` | 周结束（ms） | ms 假设 | `1783872000000` | 对 |
| `weekly_remains_time` | 周剩余（ms） | ms 假设 | `315900051` | 对 |
| `weekly_start_time` | 周开始（ms） | ms 假设 | `1783267200000` | 对 |
| `weekly_boost_permille` | 周 Boost 千分比 | 拍脑袋 → 验证 | `1500` ⇒ 1.5x；`null` | 对 |

`total_count` / `usage_count` 一直为 0，说明 MiniMax TokenPlan 当前不
暴露具体 token 计数，只暴露"剩余百分比"。前端忽略这两个字段是合理
选择。

## 3. "到时间没复位" — 真问题

MiniMax TokenPlan 的 5h 窗口按整点切（00/05/10/15/20/...），到点
`current_interval_remaining_percent` 跳到 100%。但前端原来是"打开对话
框 / 切预设时拉一次"，到点不会自动重查，UI 上旧值会停留。

修复：在 `renderMinimaxRemains` 末尾按 `end_time` / `weekly_end_time`
各起一个 `setTimeout`，到点自动 `refreshTerminalQuota`。详见
[§5](#5-到点自动重查) 与代码 `scheduleMinimaxResetRefresh`。

## 4. 状态码语义（最重要的修正）

旧：`{1:"可用", 2:"预警", 3:"耗尽"}` —— 完全错误。
新：`{1:"已开通", 2:"预警", 3:"未开通"}`。

观察依据：
- `general` 剩余 94%，`status=1` ⇒ 已开通且可用
- `video` 剩余 100%，`status=3` ⇒ 100% 是因为该模型根本未对该账号开通，
  并非"无限 / 已用 0%"

`2` 与 `0/4+` 仍未在样本中观察到，需要真实触发"快耗尽"或"限免"时再
回头核对。

## 5. 到点自动重查

新增 `scheduleMinimaxResetRefresh(list)`：
- 遍历 `model_remains` 中所有 `end_time` / `weekly_end_time`
- 每个时间戳起一个 `setTimeout(delay)`，`delay = max(0, ts - Date.now())`
- `delay > 2_147_483_647` 时 clamp 到上限（32-bit setTimeout 上限 ≈ 24.8d）
- 对话框打开且 timer 到点时调用 `refreshTerminalQuota`
- `cancelMinimaxResetRefresh` 在 `openTerminalQuotaDialog` 与
  `closeTerminalQuotaDialog` 入口处都调用，防止 timer 叠加

## 6. Boost 单位

`weekly_boost_permille` 是千分比。`1500 ⇒ 150%`。代码 `(value / 10).toFixed(0) + "%"` 保留。

## 7. 后端 `detect_platform` 的边界

`src/quota.rs:73-82` 末尾 `|| lower.contains("minimax")` 兜底过宽，
目前没造成问题，保留。

## 8. 已知遗留

- `terminal-quota.js:248` 与 `:291` 标题重复（兜底 / 正常分支各一份）。
- `2=预警` 状态码语义仍待真实预警样本验证。
- `total_count` / `usage_count` 字段被忽略；后续如 MiniMax 暴露具体
  token 数，可重新启用做"已用 X / Y"展示。
