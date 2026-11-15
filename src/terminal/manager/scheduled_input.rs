//! 终端定时输入（scheduled input）任务的持久化与 DTO 转换。
//!
//! 这组函数不依赖 `TerminalManager` 状态，只读写磁盘上的注册表文件，
//! 并把存储模型转换成给前端的 `TerminalScheduledInputTaskInfo`。

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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
        delivery_stage: scheduled_input_delivery_stage(&task.status, &task.last_error),
        parked_draft_ref: scheduled_input_parked_draft_ref(&task.last_error),
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

/// 解析任务的失败/终止阶段（isolate/body/enter/confirm），供 DTO 只读展示。
/// 优先取 last_error 的 `stage=<阶段>:` 前缀（阻断与异常都带），否则由终态
/// 推导：确认类为 confirm、仅写正文类为 body。该字段不改变既有字段语义。
pub(super) fn scheduled_input_delivery_stage(status: &str, last_error: &str) -> String {
    if let Some(rest) = last_error.strip_prefix("stage=") {
        let stage = rest.split(':').next().unwrap_or("").trim();
        if !stage.is_empty() {
            return stage.to_string();
        }
    }
    match status {
        "confirmed" | "pending_confirmation" => "confirm",
        "written" | "unverified" => "body",
        _ => "",
    }
    .to_string()
}

/// 从 last_error 解析已存档但尚未安全恢复的原始草稿引用。
/// 只证明本次映射与引用存在，不证明永久可查询或可自动恢复。
pub(super) fn scheduled_input_parked_draft_ref(last_error: &str) -> String {
    last_error
        .split(';')
        .map(str::trim)
        .find_map(|part| {
            let reference = part.strip_prefix("parked=")?;
            let (buffer, digest) = reference.split_once(":sha256=")?;
            if buffer.is_empty()
                || digest.is_empty()
                || !digest.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return None;
            }
            Some(part.to_string())
        })
        .unwrap_or_default()
}

// ===== 破坏性清空前的原始草稿存档 =====
//
// 定时投递在清空用户草稿前，先把原件写进按 (session, task) 隔离的独立
// 存档；写后回读核验 digest 与全文。存档失败必须由调用方当作「零清空」
// 处理；blocked/unknown 不删除存档，保留 parked 证据供人工恢复。

/// 原始草稿存档凭据：按 (session, task) 隔离，带 sha256 可核验。
/// 可见性与引用它的 ScheduledDeliveryOutcome（pub(in crate::terminal)）一致。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::terminal) struct DraftReceipt {
    session_id: String,
    task_id: String,
    buffer: String,
    digest: String,
}

impl DraftReceipt {
    pub(super) fn new(session_id: &str, task_id: &str, text: &str) -> Self {
        Self {
            session_id: session_id.to_string(),
            task_id: task_id.to_string(),
            buffer: Self::buffer_name(session_id, task_id),
            digest: hex::encode(Sha256::digest(text.as_bytes())),
        }
    }

    /// 固定且按 task/session 隔离：不同任务/会话不会互相覆盖尚未恢复的原件。
    /// 非字母数字统一替换，避免 buffer 名注入 tmux 参数。
    fn buffer_name(session_id: &str, task_id: &str) -> String {
        let safe = |value: &str| {
            value
                .chars()
                .map(|ch| {
                    if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                        ch
                    } else {
                        '_'
                    }
                })
                .collect::<String>()
        };
        format!("webclx-user-draft-{}-{}", safe(session_id), safe(task_id))
    }

    pub(super) fn session_id(&self) -> &str {
        &self.session_id
    }
    pub(super) fn task_id(&self) -> &str {
        &self.task_id
    }
    pub(super) fn buffer(&self) -> &str {
        &self.buffer
    }
    pub(super) fn digest(&self) -> &str {
        &self.digest
    }

    /// 写进 last_error 的紧凑引用（人/机可读，便于人工恢复定位）。
    pub(super) fn reference(&self) -> String {
        format!("parked={}:sha256={}", self.buffer, &self.digest[..16])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoredScheduledDraftRegistry {
    #[serde(default)]
    entries: Vec<StoredScheduledDraft>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredScheduledDraft {
    session_id: String,
    task_id: String,
    buffer: String,
    digest: String,
    text: String,
}

pub(super) fn scheduled_drafts_path(scheduled_input_file: &Path) -> PathBuf {
    scheduled_input_file.with_file_name("terminal-scheduled-drafts.json")
}

fn load_scheduled_drafts(path: &Path) -> Result<StoredScheduledDraftRegistry> {
    if !path.exists() {
        return Ok(StoredScheduledDraftRegistry::default());
    }
    let content = fs::read(path)
        .with_context(|| format!("cannot read scheduled draft archive {}", path.display()))?;
    serde_json::from_slice(&content)
        .with_context(|| format!("cannot parse scheduled draft archive {}", path.display()))
}

fn write_scheduled_drafts(path: &Path, registry: &StoredScheduledDraftRegistry) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let content =
        serde_json::to_vec_pretty(registry).context("cannot encode scheduled draft archive")?;
    crate::atomic_file::write_atomic(path, &content)
        .with_context(|| format!("cannot write scheduled draft archive {}", path.display()))
}

fn draft_matches(entry: &StoredScheduledDraft, receipt: &DraftReceipt) -> bool {
    entry.session_id == receipt.session_id()
        && entry.task_id == receipt.task_id()
        && entry.buffer == receipt.buffer()
        && entry.digest == receipt.digest()
}

/// 重算正文 sha256；与凭据/存档 digest 不一致即视为损坏或错配。
fn scheduled_draft_digest(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}

/// 破坏性清空前的原子存档：先重算正文 hash 与凭据一致才写入，写入后
/// 立即回读核验 digest 与全文。任何错配/写入/解析/回读不一致都返回
/// Err，调用方必须零清空退出。
pub(super) fn save_scheduled_draft(path: &Path, receipt: &DraftReceipt, text: &str) -> Result<()> {
    if scheduled_draft_digest(text) != receipt.digest() {
        anyhow::bail!("scheduled draft text hash mismatch with receipt; save refused");
    }
    let mut registry = load_scheduled_drafts(path)?;
    registry.entries.retain(|entry| {
        !(entry.session_id == receipt.session_id() && entry.task_id == receipt.task_id())
    });
    registry.entries.push(StoredScheduledDraft {
        session_id: receipt.session_id().to_string(),
        task_id: receipt.task_id().to_string(),
        buffer: receipt.buffer().to_string(),
        digest: receipt.digest().to_string(),
        text: text.to_string(),
    });
    write_scheduled_drafts(path, &registry)?;
    let read_back = load_scheduled_drafts(path)?;
    let entry = read_back
        .entries
        .iter()
        .find(|entry| draft_matches(entry, receipt))
        .context("scheduled draft archive entry missing after save")?;
    if entry.text != text {
        anyhow::bail!("scheduled draft archive verification failed: content mismatch");
    }
    if scheduled_draft_digest(&entry.text) != entry.digest {
        anyhow::bail!("scheduled draft archive verification failed: digest mismatch");
    }
    Ok(())
}

/// 按凭据回读原件：重算正文 hash，与存档 digest 和凭据 digest 双重一致
/// 才返回文本。缺失返回 None；错配/损坏返回可见 Err，不写回、不删证据。
pub(super) fn load_scheduled_draft(path: &Path, receipt: &DraftReceipt) -> Result<Option<String>> {
    let registry = load_scheduled_drafts(path)?;
    let entry = registry.entries.into_iter().find(|entry| {
        entry.session_id == receipt.session_id() && entry.task_id == receipt.task_id()
    });
    let Some(entry) = entry else {
        return Ok(None);
    };
    let recomputed = scheduled_draft_digest(&entry.text);
    if recomputed != entry.digest {
        anyhow::bail!("scheduled draft archive digest mismatch (stored entry corrupted)");
    }
    if recomputed != receipt.digest() {
        anyhow::bail!("scheduled draft archive digest mismatch with receipt");
    }
    Ok(Some(entry.text))
}

/// 仅在确认草稿已安全写回输入框后调用；blocked/unknown 不删除以保留证据。
pub(super) fn remove_scheduled_draft(path: &Path, receipt: &DraftReceipt) -> Result<()> {
    let mut registry = load_scheduled_drafts(path)?;
    let before = registry.entries.len();
    registry
        .entries
        .retain(|entry| !draft_matches(entry, receipt));
    if registry.entries.len() != before {
        write_scheduled_drafts(path, &registry)?;
    }
    Ok(())
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

pub(crate) fn deduplicate_consecutive_sender_tags(text: &str) -> String {
    if !text.contains("[from ") {
        return text.to_string();
    }
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("[from ") {
        result.push_str(&rest[..pos]);
        let after_start = &rest[pos..];
        if let Some(close_bracket) = after_start.find(']') {
            let tag = &after_start[..=close_bracket];
            result.push_str(tag);
            let mut remaining = &after_start[close_bracket + 1..];
            let mut skipped_any = false;
            loop {
                let trimmed = remaining.trim_start();
                if trimmed.starts_with(tag) {
                    remaining = &trimmed[tag.len()..];
                    skipped_any = true;
                } else {
                    break;
                }
            }
            if skipped_any && !remaining.starts_with(' ') && !remaining.is_empty() {
                result.push(' ');
            }
            rest = remaining;
        } else {
            result.push_str(after_start);
            rest = "";
        }
    }
    result.push_str(rest);
    result
}

/// 统一换行与去重：把 CRLF/CR 全部归一为 LF，并清理连续重复的来源终端标签。
pub(super) fn normalize_scheduled_input_text(text: &str) -> String {
    let replaced = text.replace("\r\n", "\n").replace('\r', "\n");
    deduplicate_consecutive_sender_tags(&replaced)
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

/// 必须立即独立投递、不得与其他消息合并的智能体重要消息。
pub(crate) fn is_priority_agent_message(text: &str) -> bool {
    text.contains("kind=escalation")
        || text.contains("[紧急中断]")
        || text.contains("[质疑]")
        || text.contains("[紧急]")
        || text.contains("kind=task")
        || text.contains("authority=manager")
        || text.contains("authority=user")
        || text.contains("[派单]")
        || text.contains("[wake-key:")
}

/// 判定是否属于可合并的智能体汇报消息。
/// 必须是 agent_message 且无重复周期。
/// 严格排除紧急中断、派单、自唤醒探针；包含阶段汇报、完成或证据链接。
pub(crate) fn is_report_agent_message(
    task_type: &str,
    text: &str,
    repeat_interval_millis: u64,
) -> bool {
    if repeat_interval_millis > 0 {
        return false;
    }
    if task_type != "agent_message" {
        return false;
    }
    if is_priority_agent_message(text) {
        return false;
    }
    text.contains("kind=progress")
        || text.contains("kind=completed")
        || text.contains("[汇报]")
        || text.contains("[阶段汇报]")
        || text.contains("[完成]")
        || text.contains("evidence=")
        || text.contains("[progress]")
        || text.contains("[completed]")
}

/// 排队中的普通智能体消息可以合并成一次投递。
/// 只合并 agent_message 且无重复周期的任务；重要消息、自唤醒和
/// 由定时管理器指定时间的任务（task_type 非 agent_message）一律不合并。
pub(crate) fn is_mergeable_agent_message(
    task_type: &str,
    text: &str,
    label: &str,
    repeat_interval_millis: u64,
) -> bool {
    if repeat_interval_millis > 0 || task_type != "agent_message" {
        return false;
    }
    if label.trim_start().starts_with("webclx-wake-v1:") {
        return false;
    }
    !is_priority_agent_message(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_report_agent_message() {
        // 汇报类消息识别
        assert!(is_report_agent_message(
            "agent_message",
            "[from worker1] kind=progress 进度汇报：已完成 3/5",
            0
        ));
        assert!(is_report_agent_message(
            "agent_message",
            "[from worker2] kind=completed 任务已完成，evidence=/data/evidence/test.md",
            0
        ));
        assert!(is_report_agent_message("agent_message", "[阶段汇报] 模块已交付", 0));

        // 排除紧急中断、质疑
        assert!(!is_report_agent_message(
            "agent_message",
            "[from worker1] kind=escalation 发现重大矛盾",
            0
        ));
        assert!(!is_report_agent_message("agent_message", "[质疑] 发现管理者逻辑有漏洞", 0));

        // 排除派单与管理者/用户指令（即使附带 evidence 亦不得误作为汇报）
        assert!(!is_report_agent_message(
            "agent_message",
            "[from manager] kind=task 分配任务给 worker1",
            0
        ));
        assert!(!is_report_agent_message(
            "agent_message",
            "[from stockScreener_99_超级管理/复核] P85定向复核已落盘：A/E方向支持 [authority=manager:/data/review.md; evidence=/data/review.md]",
            0
        ));
        assert!(!is_report_agent_message(
            "agent_message",
            "[from user] 请处理故障 [authority=user:instruction; evidence=/data/error.log]",
            0
        ));

        // 排除自唤醒探针
        assert!(!is_report_agent_message("agent_message", "[wake-key:s12345] 自唤醒检查", 0));

        // 排除非 agent_message 或重复任务
        assert!(!is_report_agent_message("plain", "kind=progress 汇报", 0));
        assert!(!is_report_agent_message("agent_message", "kind=progress 汇报", 10000));
    }

    #[test]
    fn test_deduplicate_consecutive_sender_tags() {
        assert_eq!(
            deduplicate_consecutive_sender_tags(
                "[from stockScreener_33_kimi3.1Pro] [from stockScreener_33_kimi3.1Pro] FOLLOWUP-V2"
            ),
            "[from stockScreener_33_kimi3.1Pro] FOLLOWUP-V2"
        );
        assert_eq!(
            deduplicate_consecutive_sender_tags("[from a] [from a] [from a] text"),
            "[from a] text"
        );
        assert_eq!(
            deduplicate_consecutive_sender_tags("[from a] [from b] text"),
            "[from a] [from b] text"
        );
        assert_eq!(
            deduplicate_consecutive_sender_tags("normal text without tags"),
            "normal text without tags"
        );
        assert_eq!(deduplicate_consecutive_sender_tags("[from a]"), "[from a]");
        assert_eq!(deduplicate_consecutive_sender_tags("[from a] [from a]"), "[from a]");
    }

    #[test]
    fn test_normalize_scheduled_input_text_deduplicates_tags() {
        assert_eq!(
            normalize_scheduled_input_text(
                "[from worker#1] [from worker#1] task line 1\r\ntask line 2"
            ),
            "[from worker#1] task line 1\ntask line 2"
        );
    }
}
