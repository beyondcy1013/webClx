# Agent session 测试临时目录竞态

## 现象

`cargo test --workspace` 偶发出现
`parses_codex_user_messages_dedupes_response_item_and_event_msg` 期望读取 2 条消息、实际读取 0 条消息。

## 根因

两个 Codex rollout 解析测试并行使用同一个固定临时目录 `webclx-test/.codex/sessions/`，并在各自结束时删除整个 `webclx-test`。一个测试可能在另一个测试解析前删除其 JSONL 文件。

## 修复

测试通过 `write_temp_codex_rollout` 为每个夹具创建独立根目录，同时保留解析器识别 Codex rollout 所需的 `.codex/sessions/` 路径结构；每个测试只清理自己的根目录。

## 验证

编译队列请求 `231254-18c27ed8bb0872b3` 在隔离 worktree 中并行重复运行两个相关测试 20 轮，共 40 个测试结果全部通过。
