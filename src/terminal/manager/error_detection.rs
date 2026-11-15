use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use super::super::tmux::capture_tmux_text_pane_snapshot;

const NONFATAL_MCP_STARTUP_SUMMARY: &str = "mcp startup incomplete";

/// Codex 预设识别扫描的行数：完成行与底部信息行都落在 composer 上方，
/// 12 行足以覆盖二者，又不会读到更早的历史 turn。
const TERMINAL_CODEX_PRESET_LINE_LIMIT: usize = 20;

#[derive(Clone)]
pub(in crate::terminal) struct TerminalErrorKeywordMatch {
    pub(in crate::terminal) keyword: String,
    pub(in crate::terminal) signature: String,
    pub(in crate::terminal) continue_sent: bool,
    pub(in crate::terminal) input_queued: bool,
    pub(in crate::terminal) auto_continue_at: Option<String>,
}

#[derive(Debug)]
struct TerminalErrorSignatureCandidate {
    order: usize,
    match_count: usize,
    keyword: String,
    context: String,
}

struct IndexedCompactText {
    text: String,
    line_indexes: Vec<usize>,
}

fn newer_terminal_error_candidate(
    current: Option<TerminalErrorSignatureCandidate>,
    candidate: TerminalErrorSignatureCandidate,
) -> Option<TerminalErrorSignatureCandidate> {
    match current {
        Some(existing) if existing.order > candidate.order => Some(existing),
        _ => Some(candidate),
    }
}

pub(in crate::terminal) fn terminal_error_keyword_match(
    session_id: &str,
    line_limit: u32,
    keywords: &[String],
    auto_continue_time_patterns: &[String],
    respect_manual_interrupt: bool,
) -> Option<TerminalErrorKeywordMatch> {
    let snapshot = capture_tmux_text_pane_snapshot(session_id).ok()?;
    terminal_error_keyword_match_from_snapshot(
        &snapshot,
        line_limit,
        keywords,
        auto_continue_time_patterns,
        respect_manual_interrupt,
    )
}

pub(in crate::terminal) fn terminal_error_keyword_match_from_snapshot(
    snapshot: &[u8],
    line_limit: u32,
    keywords: &[String],
    auto_continue_time_patterns: &[String],
    respect_manual_interrupt: bool,
) -> Option<TerminalErrorKeywordMatch> {
    let keywords = normalized_terminal_error_keywords(keywords);
    if keywords.is_empty() {
        return None;
    }
    if snapshot.is_empty() {
        return None;
    }

    let text = String::from_utf8_lossy(snapshot);
    // Automatic senders also call this matcher directly, bypassing the
    // activity list's working-first check. Reuse that same bounded status
    // window so stale errors or tool output cannot interrupt an active turn.
    if terminal_tail_has_working_status(&terminal_tail_lines(&text, 10)) {
        return None;
    }
    let tail = terminal_tail_lines(&text, line_limit);
    if tail.is_empty() {
        return None;
    }
    terminal_error_keyword_match_from_tail(
        &tail,
        &keywords,
        auto_continue_time_patterns,
        respect_manual_interrupt,
    )
}

fn terminal_error_keyword_match_from_tail(
    tail: &str,
    keywords: &[String],
    auto_continue_time_patterns: &[String],
    respect_manual_interrupt: bool,
) -> Option<TerminalErrorKeywordMatch> {
    let mut best_match: Option<TerminalErrorSignatureCandidate> = None;
    let compact_tail = indexed_compact_terminal_error_text(tail);
    let compact_tail_lower = compact_tail.text.to_lowercase();
    let squashed_tail = indexed_squashed_terminal_error_text(tail);
    let squashed_tail_lower = squashed_tail.text.to_lowercase();
    for keyword in keywords {
        let needle = keyword.to_lowercase();
        let squashed_needle = squash_terminal_error_text(keyword).to_lowercase();
        for (index, line) in tail.lines().enumerate() {
            let normalized_line = line.to_lowercase();
            let compact_line = normalize_terminal_error_text(line).to_lowercase();
            let squashed_line = squash_terminal_error_text(line).to_lowercase();
            let line_match_count = count_non_overlapping_matches(&normalized_line, &needle)
                + count_non_overlapping_matches(&compact_line, &needle)
                + count_non_overlapping_matches(&squashed_line, &squashed_needle);
            if line_match_count == 0 {
                continue;
            }
            best_match = newer_terminal_error_candidate(
                best_match,
                TerminalErrorSignatureCandidate {
                    order: index,
                    match_count: line_match_count,
                    keyword: keyword.clone(),
                    context: normalize_terminal_error_text(line),
                },
            );
        }

        if let Some(offset) = compact_tail_lower.rfind(&needle) {
            let compact_match_count = count_non_overlapping_matches(&compact_tail_lower, &needle);
            let order = compact_tail
                .line_indexes
                .get(offset)
                .copied()
                .unwrap_or_else(|| tail.lines().count());
            best_match = newer_terminal_error_candidate(
                best_match,
                TerminalErrorSignatureCandidate {
                    order,
                    match_count: compact_match_count,
                    keyword: keyword.clone(),
                    context: compact_terminal_error_context(
                        &compact_tail.text,
                        offset,
                        needle.len(),
                    ),
                },
            );
        }

        if let Some(offset) = squashed_tail_lower.rfind(&squashed_needle) {
            let squashed_match_count =
                count_non_overlapping_matches(&squashed_tail_lower, &squashed_needle);
            let order = squashed_tail
                .line_indexes
                .get(offset)
                .copied()
                .unwrap_or_else(|| tail.lines().count());
            best_match = newer_terminal_error_candidate(
                best_match,
                TerminalErrorSignatureCandidate {
                    order,
                    match_count: squashed_match_count,
                    keyword: keyword.clone(),
                    context: compact_terminal_error_context(
                        &squashed_tail.text,
                        offset,
                        squashed_needle.len(),
                    ),
                },
            );
        }
    }

    let best_match = best_match?;
    if terminal_nonfatal_mcp_startup_summary_order(tail)
        .is_some_and(|summary_order| summary_order >= best_match.order)
    {
        return None;
    }
    let auto_continue_at =
        terminal_error_auto_continue_time_from_tail(tail, auto_continue_time_patterns);
    let mut hasher = DefaultHasher::new();
    best_match.keyword.hash(&mut hasher);
    best_match.order.hash(&mut hasher);
    best_match.match_count.hash(&mut hasher);
    best_match.context.hash(&mut hasher);
    let signature = format!("{:016x}", hasher.finish());
    if terminal_error_has_completion_after(tail, best_match.order)
        || (respect_manual_interrupt
            && terminal_error_has_manual_interruption_after(tail, best_match.order))
    {
        return None;
    }
    Some(TerminalErrorKeywordMatch {
        keyword: best_match.keyword,
        signature,
        continue_sent: terminal_error_has_continue_after(tail, best_match.order),
        input_queued: terminal_error_has_queued_input_after(tail, best_match.order),
        auto_continue_at,
    })
}

fn terminal_nonfatal_mcp_startup_summary_order(tail: &str) -> Option<usize> {
    tail.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            normalize_terminal_error_text(line)
                .to_lowercase()
                .contains(NONFATAL_MCP_STARTUP_SUMMARY)
                .then_some(index)
        })
        .last()
}

#[cfg(test)]
pub(in crate::terminal) fn terminal_tail_error_keyword(
    tail: &str,
    keywords: &[String],
) -> Option<String> {
    terminal_tail_error_keyword_with_manual_interrupt_policy(tail, keywords, true)
}

#[cfg(test)]
pub(in crate::terminal) fn terminal_tail_error_keyword_with_manual_interrupt_policy(
    tail: &str,
    keywords: &[String],
    respect_manual_interrupt: bool,
) -> Option<String> {
    let keywords = normalized_terminal_error_keywords(keywords);
    terminal_error_keyword_match_from_tail(tail, &keywords, &[], respect_manual_interrupt)
        .map(|matched| matched.keyword)
}

pub(in crate::terminal) fn terminal_working_status_match_from_snapshot(
    snapshot: &[u8],
    line_limit: u32,
) -> bool {
    let text = String::from_utf8_lossy(snapshot);
    let tail = terminal_tail_lines(&text, line_limit);
    terminal_tail_has_working_status(&tail)
}

/// Both Codex and Claude replace their live status line with a compaction
/// progress line while the context is rewritten
/// (`Compacting context (9s • esc to interrupt)`, `✻ Compacting conversation…`).
/// The session selector may only claim "压缩中" while that line is really on
/// screen: `/compact` typed at a bare shell prompt or rejected by a busy agent
/// never starts a compaction.
pub(in crate::terminal) fn terminal_compacting_status_from_snapshot(
    snapshot: &[u8],
    line_limit: u32,
) -> bool {
    let text = String::from_utf8_lossy(snapshot);
    let tail = terminal_tail_lines(&text, line_limit);
    terminal_tail_has_compacting_status(&tail)
}

pub(in crate::terminal) fn terminal_cline_idle_prompt_from_snapshot(
    snapshot: &[u8],
    line_limit: u32,
) -> bool {
    let text = String::from_utf8_lossy(snapshot);
    let tail = terminal_tail_lines(&text, line_limit);
    tail.lines().any(line_is_cline_idle_prompt)
}

#[cfg_attr(test, allow(dead_code))]
pub(in crate::terminal) fn terminal_tail_has_working_status(tail: &str) -> bool {
    let clean_tail = strip_codex_composer_tail(tail);
    let lines: Vec<&str> = clean_tail.lines().collect();
    // “✔ Worked for …” 之后的内容是新 turn 的提交/草稿或总结尾巴，
    // 不能再用老 spinner 或引用文本把已完成终端判回工作中。
    let last_completion = lines
        .iter()
        .rposition(|line| line_is_completion_status(line));
    let last_content_index = lines.iter().rposition(|line| !line.trim().is_empty());
    for (index, line) in lines.iter().enumerate().rev() {
        if Some(index) == last_completion {
            return false;
        }
        let after_completion = last_completion.is_some_and(|completion| index > completion);
        if line_is_codex_working_status(line) {
            return true;
        }
        if line_is_cline_idle_prompt(line) {
            continue;
        }
        if line_is_claude_working_spinner(line) || line_is_agy_working_status(line) {
            if !after_completion || line_carries_live_elapsed(line) {
                return true;
            }
            continue;
        }
        if line_is_submitted_user_prompt(line) {
            if !after_completion {
                return true;
            }
            // 完成行之后的 “> …” 多为输入框草稿（带发送提示）；只有真正提交的
            // 最后一行才算新 turn 开始。
            if !line_is_composer_draft(line) && Some(index) == last_content_index {
                return true;
            }
        }
    }
    false
}

/// 状态行自带耗时或 esc 提示，说明它是当前活动行而不是历史记录。
fn line_carries_live_elapsed(line: &str) -> bool {
    let trimmed = line.trim();
    let normalized = trimmed.to_lowercase();
    if normalized.contains("esc to interrupt") || normalized.contains("esc to cancel") {
        return true;
    }
    trailing_parenthetical(trimmed).is_some_and(|detail| {
        detail
            .split(['•', '·'])
            .any(|part| looks_like_elapsed_duration(part.trim()))
    })
}

/// 输入框草稿行（右侧带 “↵ send / ⏎ send” 提示），不是已提交消息。
fn line_is_composer_draft(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.contains('↵') || trimmed.contains('⏎')
}

/// Codex（所有模型）都会在当前 turn 结束时于 composer 上方打印完成行，
/// 例如 `Worked for 48s • 11:44 PM`。部分 provider（GLM-5.3）运行期间不渲染
/// `Working (… • esc to interrupt)` 行，画面只剩流式消息 + composer，于是
/// working 判定为空，长工具调用期间被误判为空闲。预设：Codex 面板缺少该
/// 完成行时按“工作中”处理。
pub(in crate::terminal) fn terminal_tail_is_codex_turn_in_progress(
    tail: &str,
    codex_agent: bool,
) -> bool {
    let mut looks_like_codex = codex_agent;
    let mut finished_turn = false;
    for line in tail.lines().rev().take(TERMINAL_CODEX_PRESET_LINE_LIMIT) {
        if line_is_codex_worked_for_status(line) || line_is_codex_recap_summary(line) {
            finished_turn = true;
        } else if line_is_codex_context_footer(line) {
            looks_like_codex = true;
        }
    }
    looks_like_codex && !finished_turn
}

pub(in crate::terminal) fn terminal_codex_turn_in_progress_from_snapshot(
    snapshot: &[u8],
    line_limit: u32,
    codex_agent: bool,
) -> bool {
    let text = String::from_utf8_lossy(snapshot);
    let tail = terminal_tail_lines(&text, line_limit);
    terminal_tail_is_codex_turn_in_progress(&tail, codex_agent)
}

/// Codex 在完成后打印的收尾摘要：`↳ Recap: …`。它出现在 `Worked for` 之后，
/// 长摘要会把 Worked 行挤出窗口，因此 Recap 本身也必须终止“工作中”判定。
fn line_is_codex_recap_summary(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("↳ Recap:")
        || trimmed.starts_with("↳ Recap：")
        || trimmed.to_lowercase().starts_with("recap:")
}

fn line_is_codex_worked_for_status(line: &str) -> bool {
    let trimmed = line
        .trim()
        .trim_start_matches(|c: char| !c.is_alphanumeric());
    let Some(rest) = trimmed.strip_prefix("Worked for ") else {
        return false;
    };
    rest.chars().next().is_some_and(|c| c.is_ascii_digit())
}

/// Codex 的底部信息行：`GLM-5.3 xhigh · Context 4% used · Standard`。
/// 其他 agent 的上下文提示是 `Context left until auto-compact`，不含 `% used`。
fn line_is_codex_context_footer(line: &str) -> bool {
    let normalized = line.trim().to_lowercase();
    normalized.contains("context ") && normalized.contains("% used")
}

fn line_is_codex_working_status(line: &str) -> bool {
    let normalized = line.trim().to_lowercase();
    if !normalized.ends_with("esc to interrupt)") {
        return false;
    }
    if is_terminal_queued_input_line(line) {
        return false;
    }
    trailing_parenthetical(&normalized).is_some_and(|detail| {
        detail
            .split(['•', '·'])
            .any(|part| looks_like_elapsed_duration(part.trim()))
    })
}

#[cfg_attr(test, allow(dead_code))]
/// 压缩中只在“贴底”的位置才算实时状态：压缩行一旦被后续输出顶上去，
/// 说明压缩已经结束、终端早已继续工作，不能再显示压缩中。
const TERMINAL_COMPACTING_TAIL_ROWS: usize = 6;

pub(in crate::terminal) fn terminal_tail_has_compacting_status(tail: &str) -> bool {
    let rows: Vec<&str> = tail
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect();
    for (offset, line) in rows.iter().rev().enumerate() {
        if offset >= TERMINAL_COMPACTING_TAIL_ROWS {
            return false;
        }
        if line_is_compact_completion_status(line) {
            return false;
        }
        if line_is_compacting_status(line) {
            return true;
        }
        // A newer turn marker means the pane already moved past a finished
        // compaction; an older progress line further up is not live state.
        if line_is_newer_turn_status(line) {
            return false;
        }
    }
    false
}

fn line_is_compacting_status(line: &str) -> bool {
    let trimmed = line.trim();
    let normalized = trimmed.to_lowercase();
    let has_marker = [
        "compacting context",
        "compacting conversation",
        "compacting history",
        "正在压缩",
        "压缩上下文",
    ]
    .iter()
    .any(|marker| normalized.contains(marker));
    if !has_marker {
        return false;
    }
    // 只认真实状态行：带 esc 提示、带耗时括号、带 spinner 前缀，或本身就很短。
    // 聊天记录里复述“正在压缩中”等长句、引用块不算压缩进行中。
    let short_status_row =
        trimmed.chars().count() <= 32 && !trimmed.starts_with(['-', '>', '│', '|', '›', '•']);
    normalized.contains("esc to interrupt")
        || normalized.contains("esc to cancel")
        || trailing_parenthetical(trimmed).is_some_and(|detail| {
            detail
                .split(['•', '·'])
                .any(|part| looks_like_elapsed_duration(part.trim()))
        })
        || line_has_claude_status_prefix(trimmed)
        || short_status_row
}

fn line_is_compact_completion_status(line: &str) -> bool {
    let normalized = line.trim().to_lowercase();
    [
        "context compacted",
        "conversation compacted",
        "compacted conversation",
        "上下文已压缩",
        "上下文压缩完成",
    ]
    .iter()
    .any(|marker| normalized.contains(marker))
}

/// 从 Codex/GLM 页脚 `GPT-6.1-Sol xhigh · Context 11% used · Standard`
/// 提取归一化模型关键词（如 `gpt-6-1-sol`），用于按模型覆盖阈值。
pub(in crate::terminal) fn terminal_snapshot_compact_model_key(
    snapshot: &[u8],
    line_limit: u32,
) -> Option<String> {
    let text = String::from_utf8_lossy(snapshot);
    let tail = terminal_tail_lines(&text, line_limit);
    tail.lines().rev().find_map(|line| {
        if !line.to_lowercase().contains("% used") {
            return None;
        }
        let segment = line.split('·').next()?.trim();
        let mut words: Vec<&str> = segment.split_whitespace().collect();
        if words.last().is_some_and(|word| is_model_effort_token(word)) {
            words.pop();
        }
        let model = words.join(" ");
        (!model.is_empty()).then(|| crate::terminal::normalize_auto_compact_model_key(&model))
    })
}

fn is_model_effort_token(token: &str) -> bool {
    matches!(
        token.to_ascii_lowercase().as_str(),
        "xhigh" | "high" | "medium" | "low" | "minimal" | "max" | "standard" | "fast" | "none"
    )
}

/// 从 pane 底部读取“已用上下文百分比”。
///
/// 支持 Codex/GLM 的 `· Context 51% used ·` 与 Claude 的
/// `Context left until auto-compact: 23%`（后者换算成已用 77%）。
pub(in crate::terminal) fn terminal_snapshot_context_percent_used(
    snapshot: &[u8],
    line_limit: u32,
) -> Option<u32> {
    let text = String::from_utf8_lossy(snapshot);
    let tail = terminal_tail_lines(&text, line_limit);
    tail.lines().rev().find_map(|line| {
        let lower = line.to_lowercase();
        context_percent_used_in(&lower).or_else(|| context_percent_left_in(&lower))
    })
}

fn context_percent_used_in(line: &str) -> Option<u32> {
    // 只认 Codex/GLM 的页脚片段 "· Context 51% used ·"，
    // 避免聊天记录里提到 "Context 51% used" 时误判当前占用。
    if !line.contains("· context ") {
        return None;
    }
    let tokens: Vec<&str> = line
        .split(|c: char| c.is_whitespace() || c == '·' || c == ',' || c == '|')
        .filter(|token| !token.is_empty())
        .collect();
    for window in tokens.windows(2) {
        if !window[1].starts_with("used") {
            continue;
        }
        let Some(value) = window[0].strip_suffix('%') else {
            continue;
        };
        if let Ok(percent) = value.parse::<u32>() {
            return Some(percent.min(100));
        }
    }
    None
}

fn context_percent_left_in(line: &str) -> Option<u32> {
    let marker = "left until auto-compact";
    if !line.contains(marker) {
        return None;
    }
    let (_, rest) = line.split_once(':')?;
    let value = rest.trim().trim_end_matches('%').trim();
    value
        .parse::<u32>()
        .ok()
        .map(|left| 100u32.saturating_sub(left.min(100)))
}

/// 最近输出里是否带有压缩完成标记（`Context compacted` 等）。
/// 服务端自动压缩据此跳过刚压缩过的终端，避免同一段上下文被重复压缩。
pub(in crate::terminal) fn terminal_snapshot_has_compact_completion(
    snapshot: &[u8],
    line_limit: u32,
) -> bool {
    let text = String::from_utf8_lossy(snapshot);
    let tail = terminal_tail_lines(&text, line_limit);
    tail.lines().any(line_is_compact_completion_status)
}

fn line_is_newer_turn_status(line: &str) -> bool {
    line_is_codex_working_status(line)
        || line_is_completion_status(line)
        || line_is_agy_working_status(line)
        || line_is_claude_working_spinner(line)
}

/// Cline's Plan/Act placeholders end in nouns that also end in ing.
/// Exclude only the prompt line; a real working indicator elsewhere still wins.
fn line_is_cline_idle_prompt(line: &str) -> bool {
    let normalized = line.trim();
    normalized.starts_with('❯')
        && (normalized.contains("Plan something") || normalized.contains("Ask anything"))
}

/// Detects Claude Code's active status line (the spinner shown while the agent is
/// still working). Claude renders a spinner glyph plus a gerund verb terminated
/// by an ellipsis, e.g. `✻ Thinking…`, `✻ Acquiring optimized context…` or
/// `✻ Tallying tokens…`. Without this, such sessions were misclassified as idle
/// because the working check only knew Codex's `working (… • esc to interrupt)`.
fn line_is_claude_working_spinner(line: &str) -> bool {
    let trimmed = line.trim();
    if !line_has_claude_status_prefix(trimmed) {
        return false;
    }

    if line_is_claude_dynamic_status(trimmed) {
        return true;
    }

    // Require the spinner prefix too: ordinary prose and input placeholders
    // can also contain a word ending in ing followed by an ellipsis.
    if trimmed.contains('…') && line_contains_gerund(trimmed) {
        return true;
    }

    // Fall back to the ASCII "..." form, but only when a gerund sits directly
    // before the trailing ellipsis (e.g. `✻ Thinking...`). Anchoring on the
    // suffix avoids matching build output such as `working... 82% context left`.
    let stripped = strip_trailing_parenthetical(trimmed);
    if let Some(rest) = stripped.strip_suffix("...")
        && rest.split_whitespace().last().is_some_and(is_gerund_token)
    {
        return true;
    }

    false
}

fn line_is_claude_dynamic_status(line: &str) -> bool {
    let trimmed = strip_trailing_parenthetical(line);
    if !trimmed.ends_with('…') && !trimmed.ends_with("...") {
        return false;
    }
    if !line_has_claude_status_prefix(trimmed) {
        return false;
    }
    let Some(parenthetical) = trailing_parenthetical(line) else {
        return false;
    };
    // CodeBuddy's custom progress labels keep an elapsed timer even before
    // tokens arrive, including while a tool is producing no output.
    text_contains_elapsed_duration(parenthetical)
}

fn line_has_claude_status_prefix(line: &str) -> bool {
    matches!(
        line.chars().next(),
        Some('✻' | '●' | '·' | '✢' | '✶' | '✷' | '✸' | '✽' | '✺' | '✹' | '∴')
    )
}

fn line_is_agy_working_status(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.starts_with(['>', '❯', '›']) {
        return false;
    }
    if line_is_agy_tool_running(trimmed) || line_is_agy_task_status(trimmed) {
        return true;
    }
    let normalized = trim_terminal_status_line_prefix(trimmed).to_lowercase();
    if normalized == "esc to cancel" || normalized == "press esc to interrupt generation." {
        return true;
    }
    // Native AGY puts the selected model on the same footer row. It may
    // also render a braille progress row with the cancellation hint inline.
    if normalized
        .strip_prefix("esc to cancel")
        .is_some_and(|model| {
            let model = model.trim_start();
            ["claude ", "gemini ", "gpt"]
                .iter()
                .any(|prefix| model.starts_with(prefix))
        })
        || (trimmed
            .chars()
            .next()
            .is_some_and(|c| ('\u{2800}'..='\u{28ff}').contains(&c))
            && normalized.ends_with("esc to cancel"))
    {
        return true;
    }

    let status = strip_trailing_parenthetical(&normalized);
    let Some(verb) = status
        .strip_suffix("...")
        .or_else(|| status.strip_suffix('…'))
    else {
        return false;
    };
    if !matches!(
        verb,
        "thinking" | "working" | "generating" | "running" | "executing" | "waiting" | "planning"
    ) {
        return false;
    }
    trailing_parenthetical(&normalized).is_none_or(|detail| {
        text_contains_elapsed_duration(detail)
            || detail.contains("esc to cancel")
            || detail.contains("ctrl+o")
    })
}

fn line_is_agy_tool_running(trimmed: &str) -> bool {
    let normalized = trim_terminal_status_line_prefix(trimmed).to_lowercase();
    if !normalized.starts_with("running (") && !normalized.starts_with("running(") {
        return false;
    }
    line_has_claude_status_prefix(trimmed)
        || trimmed.contains("(ctrl+o to expand)")
        || trimmed.contains("(ctrl+o to collapse)")
}

fn line_is_agy_task_status(trimmed: &str) -> bool {
    let lower = trimmed.to_lowercase();
    let Some(last_word) = lower.split_whitespace().last() else {
        return false;
    };
    if !matches!(last_word, "running" | "executing" | "generating" | "thinking" | "working") {
        return false;
    }
    if !line_contains_bracketed_time(trimmed) {
        return false;
    }
    line_has_claude_status_prefix(trimmed) || trimmed.starts_with('[')
}

fn line_contains_bracketed_time(text: &str) -> bool {
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        let after_open = &rest[open + 1..];
        if let Some(close) = after_open.find(']') {
            let inside = &after_open[..close];
            if looks_like_clock_timestamp(inside) || looks_like_elapsed_duration(inside) {
                return true;
            }
            rest = &after_open[close + 1..];
        } else {
            break;
        }
    }
    false
}

fn looks_like_clock_timestamp(text: &str) -> bool {
    let parts: Vec<&str> = text.split(':').collect();
    (parts.len() == 2 || parts.len() == 3)
        && parts
            .iter()
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
}

fn text_contains_elapsed_duration(text: &str) -> bool {
    text.split_whitespace().any(looks_like_elapsed_duration)
}

fn trailing_parenthetical(text: &str) -> Option<&str> {
    let trimmed = text.trim_end();
    if !trimmed.ends_with(')') {
        return None;
    }
    let open = trimmed.rfind('(')?;
    Some(trimmed[open + 1..trimmed.len().saturating_sub(1)].trim())
}

fn line_contains_gerund(text: &str) -> bool {
    text.split_whitespace().any(is_gerund_token)
}

fn is_gerund_token(token: &str) -> bool {
    let word: String = token
        .chars()
        .filter(|character| character.is_alphabetic())
        .collect();
    let lower = word.to_lowercase();
    lower.len() >= 4 && lower.ends_with("ing")
}

fn strip_trailing_parenthetical(text: &str) -> &str {
    let trimmed = text.trim_end();
    if trimmed.ends_with(')')
        && let Some(open) = trimmed.rfind('(')
    {
        return trimmed[..open].trim_end();
    }
    trimmed
}

pub(in crate::terminal) fn terminal_worked_status_match_from_snapshot(
    snapshot: &[u8],
    line_limit: u32,
) -> bool {
    let text = String::from_utf8_lossy(snapshot);
    let tail = terminal_tail_lines(&text, line_limit);
    terminal_tail_has_worked_status(&tail)
}

#[cfg_attr(test, allow(dead_code))]
pub(in crate::terminal) fn terminal_tail_has_worked_status(tail: &str) -> bool {
    let clean_tail = strip_codex_composer_tail(tail);
    for line in clean_tail.lines().rev() {
        if line_is_completion_status(line) {
            return true;
        }
        if line_is_codex_working_status(line)
            || line_is_claude_working_spinner(line)
            || line_is_agy_working_status(line)
            || line_is_submitted_user_prompt(line)
        {
            return false;
        }
    }
    false
}

fn is_codex_status_footer(line: &str) -> bool {
    line.contains(" · Context ") && line.contains("% used")
}

fn is_particle(ch: char) -> bool {
    matches!(ch, '⠁' | '⠂' | '⠄' | '⠈' | '⠐' | '⠠' | '⡀' | '⢀')
}

fn strip_codex_composer_tail<'a>(tail: &'a str) -> &'a str {
    let lines: Vec<&str> = tail.lines().collect();
    if lines.is_empty() {
        return tail;
    }
    if let Some(footer_idx) = lines.iter().rposition(|l| is_codex_status_footer(l)) {
        if footer_idx > 0 {
            let mut prompt_idx = footer_idx - 1;
            while prompt_idx > 0 && lines[prompt_idx].trim().is_empty() {
                prompt_idx -= 1;
            }
            if lines[prompt_idx].trim_start().starts_with('›') {
                let end_offset = lines[..prompt_idx]
                    .iter()
                    .map(|l| l.len() + 1)
                    .sum::<usize>();
                return &tail[..end_offset.min(tail.len())];
            }
        }
    }
    tail
}

fn line_is_submitted_user_prompt(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('›') {
        return false;
    }
    let body = trimmed.strip_prefix('›').unwrap_or_default().trim();
    if body.is_empty() {
        return false;
    }
    let clean: String = body.chars().filter(|c| !is_particle(*c)).collect();
    let clean = clean.trim();
    if clean.is_empty() || clean == "Ask Codex to do anything" {
        return false;
    }
    true
}

/// Detects an agent's turn-completion line: `<past-tense verb> for <duration>`.
/// Codex prints `Worked for 3m 14s` while Claude Code prints `Churned for 53s`
/// (and varies the verb), so we accept any single alphabetic verb ending in
/// "ed" rather than hard-coding "worked".
fn line_is_completion_status(line: &str) -> bool {
    if line_is_codex_recap_summary(line) {
        return true;
    }
    let normalized = trim_terminal_status_line_prefix(line).to_lowercase();
    let Some((verb, duration)) = normalized.split_once(" for ") else {
        return false;
    };
    is_completion_verb(verb) && looks_like_elapsed_duration(duration)
}

fn is_completion_verb(verb: &str) -> bool {
    !verb.is_empty()
        && verb.len() >= 3
        && verb.chars().all(|character| character.is_alphabetic())
        && verb.ends_with("ed")
}

fn trim_terminal_status_line_prefix(line: &str) -> &str {
    line.trim()
        .trim_start_matches(|character: char| !character.is_alphanumeric())
        .trim_start()
}

fn looks_like_elapsed_duration(value: &str) -> bool {
    let mut previous_was_digit = false;
    let mut saw_unit = false;
    let mut remainder_start: Option<usize> = None;
    for (index, character) in value.char_indices() {
        if character.is_ascii_digit() {
            previous_was_digit = true;
            continue;
        }
        if matches!(character, 'h' | 'm' | 's') && previous_was_digit {
            saw_unit = true;
            previous_was_digit = false;
            continue;
        }
        if character.is_whitespace() {
            previous_was_digit = false;
            continue;
        }
        // Hit a non-duration character. Remember where the remainder starts so we
        // can tell harmless trailing decoration (Codex's `─────`) from trailing
        // prose (`3m 14s yesterday`) that should disqualify the match.
        remainder_start = Some(index);
        break;
    }

    if !saw_unit {
        return false;
    }

    let Some(start) = remainder_start else {
        return true; // Pure duration, nothing trailing.
    };

    is_valid_completion_remainder(&value[start..])
}

fn is_valid_completion_remainder(remainder: &str) -> bool {
    let trimmed = remainder.trim();
    if trimmed.is_empty() {
        return true;
    }
    // Allow trailing decoration (box-drawing / punctuation / symbols).
    if trimmed
        .chars()
        .all(|character| !character.is_alphanumeric())
    {
        return true;
    }
    // Codex may print trailing timestamps like `• 10:38 AM`, `• 11:05 PM`, or `· 10:38`.
    looks_like_completion_timestamp(trimmed)
}

fn looks_like_completion_timestamp(text: &str) -> bool {
    let stripped = text
        .trim()
        .trim_start_matches(|c: char| !c.is_alphanumeric())
        .trim_start();
    let tokens: Vec<&str> = stripped.split_whitespace().collect();
    if tokens.is_empty() {
        return false;
    }
    let time_part = tokens[0].trim_end_matches(|c: char| !c.is_alphanumeric());
    let mut parts = time_part.split(':');
    let Some(hours) = parts.next() else {
        return false;
    };
    let Some(minutes) = parts.next() else {
        return false;
    };
    if !hours.chars().all(|c| c.is_ascii_digit()) || hours.is_empty() || hours.len() > 2 {
        return false;
    }
    if !minutes.chars().all(|c| c.is_ascii_digit()) || minutes.len() != 2 {
        return false;
    }
    if let Some(seconds) = parts.next() {
        if !seconds.chars().all(|c| c.is_ascii_digit()) || seconds.len() != 2 {
            return false;
        }
    }
    if parts.next().is_some() {
        return false;
    }
    for token in &tokens[1..] {
        let clean = token
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        if clean.is_empty() {
            continue;
        }
        if clean != "am" && clean != "pm" {
            return false;
        }
    }
    true
}

#[cfg_attr(test, allow(dead_code))]
pub(in crate::terminal) fn terminal_error_has_continue_after(
    tail: &str,
    error_line_index: usize,
) -> bool {
    tail.lines()
        .skip(error_line_index.saturating_add(1))
        .any(is_terminal_continue_line)
}

#[cfg_attr(test, allow(dead_code))]
pub(in crate::terminal) fn terminal_error_has_queued_input_after(
    tail: &str,
    error_line_index: usize,
) -> bool {
    tail.lines()
        .skip(error_line_index.saturating_add(1))
        .any(is_terminal_queued_input_line)
}

fn terminal_error_has_completion_after(tail: &str, error_line_index: usize) -> bool {
    tail.lines()
        .skip(error_line_index.saturating_add(1))
        .any(line_is_completion_status)
}

fn terminal_error_has_manual_interruption_after(tail: &str, error_line_index: usize) -> bool {
    let after_error = tail
        .lines()
        .skip(error_line_index.saturating_add(1))
        .collect::<Vec<_>>()
        .join("\n");
    text_has_manual_interruption(&after_error)
}

fn text_has_manual_interruption(text: &str) -> bool {
    let squashed = squash_terminal_error_text(text).to_lowercase();
    squashed.contains("conversationinterrupted-tellthemodelwhattododifferently")
        || squashed.contains("theuserinterruptedthepreviousturnonpurpose")
        || squashed.contains("<turn_aborted>")
}

pub(super) fn is_terminal_queued_input_line(line: &str) -> bool {
    let line = line.trim().trim_start_matches(['•', '●']).trim();
    line == "Messages to be submitted at end of turn"
        || line == "Messages to be submitted after next tool call"
        || line.starts_with("Messages to be submitted after next tool call (press esc to interrupt")
        || line == "Queued follow-up inputs"
        || line.starts_with("Queued follow-up inputs:")
        || line == "Press up to edit queued messages"
        || (line.starts_with('▸') && line.ends_with("(alt+m to send immediately)"))
}

#[cfg(test)]
pub(in crate::terminal) fn terminal_error_reset_time_from_tail(
    tail: &str,
    patterns: &[String],
) -> Option<String> {
    terminal_error_auto_continue_time_from_tail(tail, patterns)
}

fn terminal_error_auto_continue_time_from_tail(tail: &str, patterns: &[String]) -> Option<String> {
    for pattern in patterns {
        let Some((prefix, suffix)) = pattern.split_once("{time}") else {
            continue;
        };
        let prefix = prefix.trim();
        let suffix = suffix.trim();
        for candidate in tail.lines().chain(std::iter::once(tail)) {
            if let Some(value) = extract_terminal_auto_continue_time(candidate, prefix, suffix) {
                return Some(value);
            }
        }
    }
    None
}

fn extract_terminal_auto_continue_time(line: &str, prefix: &str, suffix: &str) -> Option<String> {
    let start = if prefix.is_empty() {
        0
    } else {
        line.find(prefix)? + prefix.len()
    };
    let rest = line.get(start..)?.trim_start();
    let raw = if suffix.is_empty() {
        terminal_timeish_prefix(rest)
    } else {
        let end = rest.find(suffix)?;
        &rest[..end]
    };
    let value = raw
        .trim()
        .trim_matches(|character: char| {
            matches!(character, '[' | ']' | '(' | ')' | '。' | '.' | ',' | '，' | ';' | '；')
        })
        .trim();
    if looks_like_terminal_reset_time(value) {
        Some(normalize_terminal_error_text(value))
    } else {
        None
    }
}

fn terminal_timeish_prefix(value: &str) -> &str {
    let mut end = 0;
    for (index, character) in value.char_indices() {
        if character.is_ascii_digit()
            || matches!(character, '-' | '/' | ':' | ' ' | 'T' | 't' | 'Z' | 'z' | '+' | '.')
        {
            end = index + character.len_utf8();
            continue;
        }
        break;
    }
    &value[..end]
}

fn looks_like_terminal_reset_time(value: &str) -> bool {
    let digit_count = value
        .chars()
        .filter(|character| character.is_ascii_digit())
        .count();
    digit_count >= 8 && (value.contains(':') || value.contains('-') || value.contains('/'))
}

#[cfg_attr(test, allow(dead_code))]
pub(in crate::terminal) fn is_terminal_continue_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed == "继续"
        || trimmed.starts_with("› 继续")
        || trimmed.starts_with("↳ 继续")
        || terminal_shell_prompt_continue_line(trimmed)
}

fn terminal_shell_prompt_continue_line(line: &str) -> bool {
    ["# ", "$ ", "% ", "> "]
        .iter()
        .filter_map(|marker| line.rsplit_once(marker).map(|(_, command)| command.trim()))
        .any(is_repeated_continue_command)
}

fn is_repeated_continue_command(command: &str) -> bool {
    !command.is_empty()
        && command.len().is_multiple_of("继续".len())
        && command
            .as_bytes()
            .chunks("继续".len())
            .all(|chunk| chunk == "继续".as_bytes())
}

fn normalized_terminal_error_keywords(keywords: &[String]) -> Vec<String> {
    keywords
        .iter()
        .map(|keyword| normalize_terminal_error_text(keyword))
        .filter(|keyword| !keyword.is_empty())
        .collect()
}

fn normalize_terminal_error_text(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn indexed_compact_terminal_error_text(text: &str) -> IndexedCompactText {
    let mut compact = String::new();
    let mut line_indexes = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        for token in line.split_whitespace() {
            if !compact.is_empty() {
                compact.push(' ');
                line_indexes.push(line_index);
            }
            compact.push_str(token);
            line_indexes.extend(std::iter::repeat_n(line_index, token.len()));
        }
    }
    IndexedCompactText {
        text: compact,
        line_indexes,
    }
}

fn indexed_squashed_terminal_error_text(text: &str) -> IndexedCompactText {
    let mut compact = String::new();
    let mut line_indexes = Vec::new();
    for (line_index, line) in text.lines().enumerate() {
        for character in line.chars().filter(|character| !character.is_whitespace()) {
            compact.push(character);
            line_indexes.extend(std::iter::repeat_n(line_index, character.len_utf8()));
        }
    }
    IndexedCompactText {
        text: compact,
        line_indexes,
    }
}

fn squash_terminal_error_text(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}

fn compact_terminal_error_context(text: &str, offset: usize, needle_len: usize) -> String {
    const CONTEXT_BYTES: usize = 120;
    let start = offset.saturating_sub(CONTEXT_BYTES);
    let end = (offset + needle_len + CONTEXT_BYTES).min(text.len());
    text.get(start..end).unwrap_or(text).to_string()
}

fn terminal_tail_lines(text: &str, line_limit: u32) -> String {
    let limit = usize::try_from(line_limit.max(1)).unwrap_or(1);
    // capture-pane includes the unused rows below a TUI. They must not push
    // its actual status/footer out of the bounded tail.
    let lines = text.trim_end().lines().collect::<Vec<_>>();
    let start = lines.len().saturating_sub(limit);
    lines[start..].join("\n")
}

pub(in crate::terminal) fn count_non_overlapping_matches(haystack: &str, needle: &str) -> usize {
    if needle.is_empty() {
        return 0;
    }

    let mut count = 0;
    let mut cursor = 0;
    while let Some(offset) = haystack[cursor..].find(needle) {
        count += 1;
        cursor += offset + needle.len();
    }
    count
}

pub(in crate::terminal) fn compact_terminal_search_line(line: &str) -> String {
    const MAX_CHARS: usize = 220;
    let mut compact = line.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.is_empty() {
        compact = line.trim().to_string();
    }
    if compact.chars().count() <= MAX_CHARS {
        return compact;
    }
    let mut truncated = compact.chars().take(MAX_CHARS).collect::<String>();
    truncated.push_str("...");
    truncated
}

#[cfg(test)]
mod tests {
    use super::terminal_error_keyword_match_from_snapshot;

    #[test]
    fn codebuddy_forbidden_uses_loaded_mark_only_rule_instead_of_old_quota_error() {
        struct SettingsFixture(std::path::PathBuf);
        impl Drop for SettingsFixture {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let fixture = SettingsFixture(std::env::temp_dir().join(format!(
            "webclx-codebuddy-forbidden-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        )));
        std::fs::create_dir_all(&fixture.0).unwrap();
        let settings = settings_core::SettingsManager::load(&fixture.0).unwrap();
        for error in ["403 request illegal", "403 request\n    illegal"] {
            let snapshot = format!(
                "● REPL\n  └ 429 您的使用量已超出频率限制\n> 继续\n● REPL\n  └ {error}\n────────────────────\n> \n────────────────────"
            );
            let matched = terminal_error_keyword_match_from_snapshot(
                snapshot.as_bytes(),
                12,
                &settings.terminal_error_keywords(),
                &[],
                true,
            )
            .expect("native forbidden error must not appear completed or remain a quota error");
            assert_eq!(matched.keyword, "403 request illegal");
            assert!(!matched.continue_sent);
            assert_eq!(
                super::super::resolve_terminal_error_keyword_action(
                    &matched.keyword,
                    &settings.terminal_error_keyword_actions(),
                ),
                settings_core::TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY,
            );
            assert!(!super::super::account_rotation::exhaustion_keyword(&matched.keyword));
            assert!(
                terminal_error_keyword_match_from_snapshot(
                    format!("{snapshot}\n• Working (12s • esc to interrupt)").as_bytes(),
                    12,
                    &settings.terminal_error_keywords(),
                    &[],
                    true,
                )
                .is_none()
            );
        }
    }

    #[test]
    fn terminal_error_snapshot_does_not_retry_while_agent_is_working() {
        let keywords = vec!["sending request for url".to_string()];
        for status in ["Working (9m 20s • esc to interrupt)", "✻ Thinking… (31s)"] {
            let snapshot = format!(
                "error sending request for url (https://example.test)\n{status}\n\n› Ask Codex to do anything\n\n \n"
            );
            // The status guard uses the same 10-line window as the activity
            // list even when the configured error window is much larger.
            assert!(
                terminal_error_keyword_match_from_snapshot(
                    snapshot.as_bytes(),
                    20,
                    &keywords,
                    &[],
                    true,
                )
                .is_none(),
                "active status must suppress retries: {status}"
            );
        }

        // Once the active status disappears, a real failure remains eligible.
        assert!(
            terminal_error_keyword_match_from_snapshot(
                b"error sending request for url (https://example.test)\n\n> ",
                20,
                &keywords,
                &[],
                true,
            )
            .is_some()
        );
    }

    #[test]
    fn terminal_error_snapshot_ignores_unused_pane_rows_and_stale_errors() {
        let keywords = vec!["last status: 429".to_string()];
        let padding = "\n \n".repeat(30);
        let snapshot = format!("last status: 429\n> {padding}");
        assert!(
            terminal_error_keyword_match_from_snapshot(
                snapshot.as_bytes(),
                12,
                &keywords,
                &[],
                true,
            )
            .is_some()
        );

        for newer_status in ["✔ Worked for 8m 15s", "MCP startup incomplete"] {
            let snapshot = format!("last status: 429\n{newer_status}\n> {padding}");
            assert!(
                terminal_error_keyword_match_from_snapshot(
                    snapshot.as_bytes(),
                    12,
                    &keywords,
                    &[],
                    true,
                )
                .is_none()
            );
        }
    }
}
