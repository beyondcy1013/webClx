//! 终端定时输入（scheduled input）任务的持久化与 DTO 转换。
//!
//! 这组函数不依赖 `TerminalManager` 状态，只读写磁盘上的注册表文件，
//! 并把存储模型转换成给前端的 `TerminalScheduledInputTaskInfo`。

use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result};
// 类型定义在祖父模块 crate::terminal 中，manager.rs 已把它们再导出给本模块。
use super::super::{
    StoredTerminalScheduledInputRegistry, TerminalScheduledInputTask,
    TerminalScheduledInputTaskInfo,
};
use tracing::warn;

/// 读取并过滤定时输入注册表，返回以 task id 为键的有效任务表。
pub(super) fn load_terminal_scheduled_input_tasks(
    scheduled_input_file: &Path,
) -> HashMap<String, TerminalScheduledInputTask> {
    let registry = match load_terminal_scheduled_input_registry(scheduled_input_file) {
        Ok(registry) => registry,
        Err(error) => {
            warn!(
                "load terminal scheduled input registry failed {}, fallback to empty state: {error}",
                scheduled_input_file.display()
            );
            return HashMap::new();
        }
    };
    registry
        .tasks
        .into_iter()
        .filter(|task| !task.id.trim().is_empty())
        .filter(|task| !task.session_id.trim().is_empty())
        .filter(|task| !task.text.trim().is_empty())
        .map(|task| (task.id.clone(), task))
        .collect()
}

/// 解析定时输入注册表 JSON；文件不存在时返回空注册表。
pub(super) fn load_terminal_scheduled_input_registry(
    scheduled_input_file: &Path,
) -> Result<StoredTerminalScheduledInputRegistry> {
    if !scheduled_input_file.exists() {
        return Ok(StoredTerminalScheduledInputRegistry { tasks: Vec::new() });
    }
    let content = fs::read(scheduled_input_file).with_context(|| {
        format!(
            "cannot read terminal scheduled input registry {}",
            scheduled_input_file.display()
        )
    })?;
    let registry = serde_json::from_slice(&content).with_context(|| {
        format!(
            "cannot parse terminal scheduled input registry {}",
            scheduled_input_file.display()
        )
    })?;
    Ok(registry)
}

/// 写回定时输入注册表 JSON（自动创建父目录）。
pub(super) fn persist_terminal_scheduled_input_registry(
    scheduled_input_file: &Path,
    registry: &StoredTerminalScheduledInputRegistry,
) -> Result<()> {
    if let Some(parent) = scheduled_input_file.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_vec_pretty(registry)
        .context("cannot encode terminal scheduled input registry")?;
    fs::write(scheduled_input_file, content).with_context(|| {
        format!(
            "cannot write terminal scheduled input registry {}",
            scheduled_input_file.display()
        )
    })
}

/// 把任务表按到期时间排序后转成给前端的 DTO 列表。
pub(super) fn scheduled_input_task_infos(
    tasks: &HashMap<String, TerminalScheduledInputTask>,
) -> Vec<TerminalScheduledInputTaskInfo> {
    let mut infos = tasks
        .values()
        .map(scheduled_input_task_info)
        .collect::<Vec<_>>();
    infos.sort_by(|a, b| {
        a.due_at_millis
            .cmp(&b.due_at_millis)
            .then_with(|| a.created_at_millis.cmp(&b.created_at_millis))
            .then_with(|| a.id.cmp(&b.id))
    });
    infos
}

/// 单个任务的 DTO 转换：截断预览文本、映射字段。
pub(super) fn scheduled_input_task_info(
    task: &TerminalScheduledInputTask,
) -> TerminalScheduledInputTaskInfo {
    TerminalScheduledInputTaskInfo {
        id: task.id.clone(),
        task_id: task.id.clone(),
        session_id: task.session_id.clone(),
        terminal_name: task.terminal_name.clone(),
        due_at: task.due_at_millis,
        due_at_millis: task.due_at_millis,
        created_at_millis: task.created_at_millis,
        label: task.label.clone(),
        preview: terminal_scheduled_input_preview(&task.text, 80),
        text: task.text.clone(),
        send_enter: task.send_enter,
        task_type: task.task_type.clone(),
        working_dir: task.working_dir.clone(),
    }
}

/// 把任务文本折叠空白并截断到 max_chars 字符（超出加省略号）。
pub(super) fn terminal_scheduled_input_preview(text: &str, max_chars: usize) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.chars().count() <= max_chars {
        return collapsed;
    }
    let mut preview = collapsed.chars().take(max_chars).collect::<String>();
    preview.push('…');
    preview
}

/// 统一换行：把 CRLF/CR 全部归一为 LF，便于后续比较。
pub(super) fn normalize_scheduled_input_text(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}
