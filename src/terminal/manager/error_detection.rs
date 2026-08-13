use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
};

use super::super::tmux::capture_tmux_text_pane_snapshot;

const NONFATAL_MCP_STARTUP_SUMMARY: &str = "mcp startup incomplete";

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

#[cfg_attr(test, allow(dead_code))]
pub(in crate::terminal) fn terminal_tail_has_working_status(tail: &str) -> bool {
    tail.lines().any(|line| {
        let normalized = line.trim().to_lowercase();
        if normalized.contains("working (") && normalized.contains("esc to interrupt") {
            return true;
        }
        line_is_claude_working_spinner(line)
    })
}

/// Detects Claude Code's active status line (the spinner shown while the agent is
/// still working). Claude renders a spinner glyph plus a gerund verb terminated
/// by an ellipsis, e.g. `✻ Thinking…`, `✻ Acquiring optimized context…` or
/// `✻ Tallying tokens…`. Without this, such sessions were misclassified as idle
/// because the working check only knew Codex's `working (… • esc to interrupt)`.
fn line_is_claude_working_spinner(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }

    if line_is_claude_dynamic_status(trimmed) {
        return true;
    }

    // The primary signal: the U+2026 ellipsis combined with a gerund. Plain
    // shell/build output almost never mixes these two, so this is reliable.
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
    if !trimmed.contains('…') {
        return false;
    }
    if !line_has_claude_status_prefix(trimmed) {
        return false;
    }
    let Some(parenthetical) = trailing_parenthetical(line) else {
        return false;
    };
    parenthetical.contains("tokens") && text_contains_elapsed_duration(parenthetical)
}

fn line_has_claude_status_prefix(line: &str) -> bool {
    matches!(line.chars().next(), Some('✻' | '●' | '·' | '✢' | '✶' | '✽' | '✺' | '✹'))
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
    tail.lines().any(line_is_completion_status)
}

/// Detects an agent's turn-completion line: `<past-tense verb> for <duration>`.
/// Codex prints `Worked for 3m 14s` while Claude Code prints `Churned for 53s`
/// (and varies the verb), so we accept any single alphabetic verb ending in
/// "ed" rather than hard-coding "worked".
fn line_is_completion_status(line: &str) -> bool {
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

    // Allow trailing decoration (box-drawing / punctuation / symbols) but reject
    // trailing letters or digits, which indicate prose rather than a status line.
    value[start..]
        .chars()
        .all(|character| !character.is_alphanumeric())
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

fn is_terminal_queued_input_line(line: &str) -> bool {
    line.contains("Messages to be submitted at end of turn")
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
    let lines = text.lines().collect::<Vec<_>>();
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
