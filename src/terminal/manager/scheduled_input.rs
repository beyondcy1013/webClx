//! 终端定时输入（scheduled input）任务的持久化与 DTO 转换。
//!
//! 这组函数不依赖 `TerminalManager` 状态，只读写磁盘上的注册表文件，
//! 并把存储模型转换成给前端的 `TerminalScheduledInputTaskInfo`。

use std::{collections::HashMap, fs, path::Path};

use anyhow::{Context, Result};
// 类型定义在祖父模块 crate::terminal 中，manager.rs 已把它们再导出给本模块。
use super::super::{
    MAX_TERMINAL_SCHEDULED_REPEAT_INTERVAL_MILLIS, MIN_TERMINAL_SCHEDULED_REPEAT_INTERVAL_MILLIS,
    StoredTerminalScheduledInputRegistry, TerminalScheduledInputTask,
    TerminalScheduledInputTaskInfo,
};
use tracing::warn;

pub(super) const SCHEDULED_INPUT_COMPLETED_TASK_LIMIT: usize = 30;

/// 保留最多 SCHEDULED_INPUT_COMPLETED_TASK_LIMIT 条已完成/终态历史任务，清理超额的最早已完成记录。
/// 待执行任务（status 为空）和派发中任务（status == "dispatching"）绝不淘汰。
pub(crate) fn prune_completed_scheduled_input_tasks(
    tasks: &mut HashMap<String, TerminalScheduledInputTask>,
) {
    let mut completed = tasks
        .iter()
        .filter(|(_, task)| !task.status.is_empty() && task.status != "dispatching")
        .map(|(id, task)| (id.clone(), task.due_at_millis))
        .collect::<Vec<_>>();
    if completed.len() > SCHEDULED_INPUT_COMPLETED_TASK_LIMIT {
        completed.sort_by_key(|(_, due_at)| *due_at);
        let excess = completed.len() - SCHEDULED_INPUT_COMPLETED_TASK_LIMIT;
        for (id, _) in completed.into_iter().take(excess) {
            tasks.remove(&id);
        }
    }
}

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
    let mut tasks = registry
        .tasks
        .into_iter()
        .filter(|task| !task.id.trim().is_empty())
        .filter(|task| !task.session_id.trim().is_empty())
        .filter(|task| !task.text.trim().is_empty())
        .map(|mut task| {
            if task.status == "dispatching" {
                task.status = "unknown".into();
                task.last_error = "service restarted during delivery; do not resend body".into();
            }
            (task.id.clone(), task)
        })
        .collect::<HashMap<_, _>>();
    prune_completed_scheduled_input_tasks(&mut tasks);
    tasks
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
    // Never expose a truncated registry after a crash during delivery claiming.
    crate::atomic_file::write_atomic(scheduled_input_file, &content).with_context(|| {
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
        status: if task.status.is_empty() {
            "queued".into()
        } else {
            task.status.clone()
        },
        last_error: task.last_error.clone(),
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
        repeat_interval_millis: task.repeat_interval_millis,
        parent_task_id: task.parent_task_id.clone(),
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

pub(crate) fn validate_scheduled_input_repeat_interval(repeat_interval_millis: u64) -> Result<()> {
    if repeat_interval_millis == 0 {
        return Ok(());
    }
    if repeat_interval_millis < MIN_TERMINAL_SCHEDULED_REPEAT_INTERVAL_MILLIS
        || repeat_interval_millis > MAX_TERMINAL_SCHEDULED_REPEAT_INTERVAL_MILLIS
    {
        anyhow::bail!(
            "repeat interval must be 0 or between {MIN_TERMINAL_SCHEDULED_REPEAT_INTERVAL_MILLIS} and {MAX_TERMINAL_SCHEDULED_REPEAT_INTERVAL_MILLIS} ms"
        );
    }
    Ok(())
}

pub(crate) fn claim_due_scheduled_input_tasks(
    tasks: &mut HashMap<String, TerminalScheduledInputTask>,
    now: u64,
    defer_until: Option<u64>,
) -> Result<Vec<TerminalScheduledInputTask>> {
    // Collect tasks that are actually due.
    // Tasks that are not subject to the avoid window (such as agent_message) can fire
    // immediately even when defer_until is active!
    let due_ids = tasks
        .values()
        .filter(|task| {
            task.status.is_empty()
                && task.due_at_millis <= now
                && (defer_until.is_none() || task.task_type == "agent_message")
        })
        .map(|task| task.id.clone())
        .collect::<Vec<_>>();
    let mut recurring = Vec::new();
    for task_id in &due_ids {
        let task = &tasks[task_id];
        let interval = task.repeat_interval_millis;
        if interval == 0 {
            continue;
        }
        validate_scheduled_input_repeat_interval(interval)?;
        let missed = (now - task.due_at_millis) / interval;
        let steps = missed
            .checked_add(1)
            .context("scheduled input repeat step overflow")?;
        let advance = steps
            .checked_mul(interval)
            .context("scheduled input repeat advance overflow")?;
        let next_due = task
            .due_at_millis
            .checked_add(advance)
            .context("scheduled input repeat next due overflow")?;
        let mut child = task.clone();
        child.id = format!("{}-fire-{:032x}", task.id, rand::random::<u128>());
        child.status = "dispatching".into();
        child.last_error.clear();
        child.repeat_interval_millis = 0;
        child.parent_task_id = task.id.clone();
        child.created_at_millis = now;
        recurring.push((task_id.clone(), next_due, child));
    }
    let mut due_tasks = Vec::with_capacity(due_ids.len());
    for task_id in &due_ids {
        if let Some(task) = tasks.get_mut(task_id) {
            if task.repeat_interval_millis == 0 {
                task.status = "dispatching".into();
                due_tasks.push(task.clone());
            }
        }
    }
    for (parent_id, next_due, child) in recurring {
        if let Some(parent) = tasks.get_mut(&parent_id) {
            parent.due_at_millis = next_due;
        }
        tasks.insert(child.id.clone(), child.clone());
        due_tasks.push(child);
    }
    due_tasks.sort_by(|a, b| {
        (a.due_at_millis, a.created_at_millis, &a.id).cmp(&(
            b.due_at_millis,
            b.created_at_millis,
            &b.id,
        ))
    });
    Ok(due_tasks)
}

pub(crate) fn claim_and_persist_due_scheduled_input_tasks(
    tasks: &mut HashMap<String, TerminalScheduledInputTask>,
    now: u64,
    defer_until: Option<u64>,
    persist: impl FnOnce(&HashMap<String, TerminalScheduledInputTask>) -> Result<()>,
) -> Result<Vec<TerminalScheduledInputTask>> {
    let original = tasks.clone();
    let due_tasks = match claim_due_scheduled_input_tasks(tasks, now, defer_until) {
        Ok(due_tasks) => due_tasks,
        Err(error) => {
            *tasks = original;
            return Err(error);
        }
    };
    if !due_tasks.is_empty() {
        if let Err(error) = persist(tasks) {
            *tasks = original;
            return Err(error);
        }
    }
    Ok(due_tasks)
}
