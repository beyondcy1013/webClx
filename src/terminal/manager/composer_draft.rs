//! 终端消息与用户未提交草稿的输入框隔离。
//!
//! PTY 只有一条输入通道：注入的正文和用户键入落在同一个输入框里。终端消息
//! 到达时如果用户正停在某个未提交草稿上，直接写入会把两者拼成一条，并随
//! 第一个 Enter 一起提交。这里只做两件事：从可见画面识别草稿，以及生成清空
//! 草稿所需的按键序列。真正的暂存/还原由 manager 执行。
//!
//! 识别按各 CLI 真实渲染的输入框样式来，不使用“底部若干行”这类猜测：
//! - Codex：`› ` 输入行固定在状态脚���（`· Context N% used · Fast …`）上方两行，
//!   空输入框显示占位符 `Ask Codex to do anything`（可能夹带动画盲文字符）。
//! - Claude Code / CodeBuddy：`❯ ` / `> ` 输入行被上下两条横线框住。
//! 多行草稿（含自动换行产生的续行）以 2 空格渲染缩进，按行还原。
//! 认不出输入框时返回 `Unrecognized`，调用方保持原有投递行为，避免因解析
//! 失败卡住投递。

use super::super::activity::TerminalAgentActivity;

const CODEX_PLACEHOLDER: &str = "Ask Codex to do anything";
/// 横线框至少要这么多字符，避免把聊天正文里的短横线当成输入框边框。
const RULE_MIN_CHARS: usize = 10;
/// 输入行与边框/状态行之间允许的空行数量上限。
const FRAME_SEARCH_LINES: usize = 4;
/// 输入框最多向上补齐的续行数量，避免误吞上方的正文缩进行。
const MAX_COMPOSER_ROWS: usize = 24;
/// Ctrl+U：删除光标到行首。
const CLEAR_LINE_KEY: char = '\u{15}';
/// DEL：行首退格，把当前行并回上一行。
const JOIN_LINE_KEY: char = '\u{7f}';

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::terminal) enum ComposerAgent {
    Codex,
    Claude,
    CodeBuddy,
    Agy,
}

impl ComposerAgent {
    pub(in crate::terminal) fn from_activity(activity: &TerminalAgentActivity) -> Option<Self> {
        activity.agents.iter().find_map(|agent| {
            if agent.eq_ignore_ascii_case("Codex") {
                Some(Self::Codex)
            } else if agent.eq_ignore_ascii_case("Claude") {
                Some(Self::Claude)
            } else if agent.eq_ignore_ascii_case("CodeBuddy") {
                Some(Self::CodeBuddy)
            } else if agent.eq_ignore_ascii_case("AGY") {
                Some(Self::Agy)
            } else {
                None
            }
        })
    }

    fn prompt_marker(self) -> char {
        match self {
            Self::Codex => '›',
            Self::Claude => '❯',
            Self::CodeBuddy | Self::Agy => '>',
        }
    }

    /// Claude Code 与 CodeBuddy 的输入框始终夹在两条横线之间，横线是可靠锚点。
    fn requires_rule_frame(self) -> bool {
        !matches!(self, Self::Codex)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::terminal) enum ComposerScan {
    /// 输入框为空（或只剩占位符/空白），没有用户草稿。
    Empty,
    /// 用户草稿，按渲染行拆分，第一行不含提示符。
    Draft(Vec<String>),
    /// 画面里找不到可信的输入框，无法判断草稿状态。
    Unrecognized,
}

pub(in crate::terminal) fn scan_composer_draft(agent: ComposerAgent, pane: &str) -> ComposerScan {
    let pane = composer_text_without_suggestions(agent, pane);
    let lines: Vec<&str> = pane.lines().collect();
    let Some((prompt_index, frame_end)) = composer_frame(agent, &lines) else {
        return ComposerScan::Unrecognized;
    };

    let mut draft: Vec<String> = Vec::new();
    let head = draft_head(agent, lines[prompt_index]);
    if !head.is_empty() {
        draft.push(head);
    }
    for line in &lines[prompt_index + 1..frame_end] {
        let text = continuation_text(line);
        if !text.is_empty() {
            draft.push(text);
        }
    }

    if draft.is_empty() {
        return ComposerScan::Empty;
    }
    ComposerScan::Draft(draft)
}

/// tmux -e preserves SGR. CodeBuddy renders ghost suggestions dim, except for
/// the first glyph under its inverse cursor; genuine input has normal glyphs.
/// Never classify a suggestion by its wording (which may also be a real draft).
fn composer_text_without_suggestions(agent: ComposerAgent, pane: &str) -> String {
    let mut dim = false;
    let mut inverse = false;
    let mut chars = pane.chars().peekable();
    let mut rows: Vec<Vec<(char, bool, bool)>> = vec![Vec::new()];
    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            let mut params = String::new();
            for next in chars.by_ref() {
                if ('@'..='~').contains(&next) {
                    if next == 'm' {
                        let codes: Vec<u16> = params
                            .split(';')
                            .map(|code| code.parse().unwrap_or(0))
                            .collect();
                        let mut index = 0;
                        while index < codes.len() {
                            match codes[index] {
                                0 => {
                                    dim = false;
                                    inverse = false;
                                }
                                2 => dim = true,
                                7 => inverse = true,
                                22 => dim = false,
                                27 => inverse = false,
                                // Color components such as 38;5;2 are not SGR 2.
                                38 | 48 | 58 => {
                                    index += match codes.get(index + 1) {
                                        Some(5) => 2,
                                        Some(2) => 4,
                                        _ => 0,
                                    };
                                }
                                _ => {}
                            }
                            index += 1;
                        }
                    }
                    break;
                }
                params.push(next);
            }
        } else if ch == '\n' {
            rows.push(Vec::new());
        } else {
            rows.last_mut().unwrap().push((ch, dim, inverse));
        }
    }
    // Codex's dim status footer must stay intact. Only its actual composer rows
    // participate in ghost filtering, not dim transcript/status text.
    let codex_frame = if agent == ComposerAgent::Codex {
        let plain: Vec<String> = rows
            .iter()
            .map(|row| row.iter().map(|g| g.0).collect())
            .collect();
        let lines: Vec<&str> = plain.iter().map(String::as_str).collect();
        composer_frame(agent, &lines)
    } else {
        None
    };
    rows.into_iter()
        .enumerate()
        .map(|(index, row)| {
            let plain: String = row.iter().map(|glyph| glyph.0).collect();
            if agent == ComposerAgent::Codex
                && !codex_frame.is_some_and(|(start, end)| (start..end).contains(&index))
            {
                return plain;
            }
            let start = row.iter().position(|glyph| !glyph.0.is_whitespace());
            let prompt = start.filter(|&index| row[index].0 == agent.prompt_marker());
            let body = &row[prompt.map_or(0, |index| index + 1)..];
            let is_ghost = body.iter().any(|glyph| !glyph.0.is_whitespace() && glyph.1)
                && body
                    .iter()
                    .filter(|glyph| !glyph.0.is_whitespace())
                    .all(|glyph| glyph.1 || glyph.2);
            if is_ghost {
                if prompt.is_some() {
                    return format!("{} ", agent.prompt_marker());
                } else {
                    return String::new();
                }
            }
            plain
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// 清空草稿的按键序列：每多一行先删掉本行正文再并回上一行，最后清空首行。
pub(in crate::terminal) fn composer_clear_keystrokes(line_count: usize) -> String {
    let line_count = line_count.max(1);
    let mut keys = String::new();
    for index in 0..line_count {
        keys.push(CLEAR_LINE_KEY);
        if index + 1 < line_count {
            keys.push(JOIN_LINE_KEY);
        }
    }
    keys
}

/// 返回 (输入行下标, 输入框结束下标（不含）)。
fn composer_frame(agent: ComposerAgent, lines: &[&str]) -> Option<(usize, usize)> {
    if agent.requires_rule_frame() {
        let last = lines.len().checked_sub(1)?;
        let prompt_index = (0..=last)
            .rev()
            .find(|&index| lines[index].trim_start().starts_with(agent.prompt_marker()))?;
        let frame_end = (prompt_index + 1..=last.min(prompt_index + FRAME_SEARCH_LINES))
            .find(|&index| is_rule_line(lines[index]))?;
        let frame_start = (prompt_index.saturating_sub(FRAME_SEARCH_LINES)..prompt_index)
            .rev()
            .any(|index| is_rule_line(lines[index]));
        return frame_start.then_some((prompt_index, frame_end));
    }

    let footer_index = lines
        .iter()
        .rposition(|line| is_codex_status_footer(line))?;
    if footer_index == 0 {
        return None;
    }
    // 状态行上方可能有空行；Input 行本身可能带多行续行，因此从最后一行
    // 往上补齐缩进续行，而不是固定取状态行上方两行。
    let mut first = footer_index - 1;
    while first > 0 && lines[first].trim().is_empty() {
        first -= 1;
    }
    let mut steps = 0;
    while first > 0 && steps < MAX_COMPOSER_ROWS && is_continuation_row(lines[first]) {
        first -= 1;
        steps += 1;
    }
    lines[first]
        .trim_start()
        .starts_with(agent.prompt_marker())
        .then_some((first, footer_index))
}

fn draft_head(agent: ComposerAgent, line: &str) -> String {
    let body = line
        .trim_start()
        .strip_prefix(agent.prompt_marker())
        .unwrap_or_default();
    let body = strip_particles(body.trim_end());
    let body = body.strip_prefix(' ').unwrap_or(&body);
    let body = body.trim_end();
    if agent == ComposerAgent::Codex && body == CODEX_PLACEHOLDER {
        return String::new();
    }
    body.to_string()
}

fn is_continuation_row(line: &str) -> bool {
    !line.trim().is_empty() && line.starts_with("  ")
}

fn continuation_text(line: &str) -> String {
    let trimmed = line.trim_end();
    let body = trimmed
        .strip_prefix("  ")
        .unwrap_or_else(|| trimmed.trim_start());
    body.trim_end().to_string()
}

fn is_rule_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.chars().count() >= RULE_MIN_CHARS && trimmed.chars().all(|ch| ch == '─' || ch == '━')
}

/// Match the native context meter; Fast/Standard and clipped suffixes are UI
/// preferences, not composer ownership requirements.
fn is_codex_status_footer(line: &str) -> bool {
    line.split_once(" · Context ")
        .and_then(|(_, rest)| rest.split_once("% used · "))
        .and_then(|(percent, _)| percent.parse::<f64>().ok())
        .is_some_and(|percent| (0.0..=100.0).contains(&percent))
}

fn strip_particles(text: &str) -> String {
    text.chars()
        .filter(|ch| !matches!(ch, '⠁' | '⠂' | '⠄' | '⠈' | '⠐' | '⠠' | '⡀' | '⢀'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codex_pane(composer: &str) -> String {
        format!(
            "\n  >_ OpenAI Codex (v0.163.0)\n     /home/codes/tools/webClx\n  permissions: YOLO mode\n\n  Tip: Start a fresh idea with /new.\n \n \n{composer}\n \n  GPT-6.1-Sol medium · Context 0% used · Fast off\n\n\n"
        )
    }

    fn claude_pane(composer: &str) -> String {
        let rule = "─".repeat(110);
        format!("header\n\n{rule}\n{composer}\n{rule}\n  ? for shortcuts\n")
    }

    fn codebuddy_pane(composer: &str) -> String {
        let rule = "─".repeat(110);
        format!("header\n\n{rule}\n{composer}\n{rule}\n⏵⏵ bypass permissions on\n")
    }

    #[test]
    fn codex_placeholder_is_empty() {
        assert_eq!(
            scan_composer_draft(ComposerAgent::Codex, &codex_pane("› Ask Codex to do anything")),
            ComposerScan::Empty
        );
        assert_eq!(
            scan_composer_draft(ComposerAgent::Codex, &codex_pane("›⠁Ask Codex to do anything⡀ ")),
            ComposerScan::Empty
        );
    }

    #[test]
    fn codex_single_line_draft_is_captured() {
        assert_eq!(
            scan_composer_draft(ComposerAgent::Codex, &codex_pane("› DRAFT ONE               ")),
            ComposerScan::Draft(vec!["DRAFT ONE".to_string()])
        );
    }

    #[test]
    fn composer_codex_standard_and_clipped_footer_capture_draft() {
        for mode in ["Standard", "Standa…", "Fast off"] {
            let pane = format!(
                "history\n› 用户真实草稿\n\n  GPT-6.1-Sol xhigh · Context 70% used · {mode}\n"
            );
            assert_eq!(
                scan_composer_draft(ComposerAgent::Codex, &pane),
                ComposerScan::Draft(vec!["用户真实草稿".into()])
            );
        }
    }

    #[test]
    fn composer_codex_multiline_ghost_is_empty_but_same_wording_can_be_real() {
        let ghost = "› \x1b[7mT\x1b[27m\x1b[2mry another approach\x1b[0m\n  \x1b[2mcontinue investigating\x1b[0m";
        let footer = "\n\n\x1b[2m  GPT-6.1-Sol medium · Context 20% used · Standard\x1b[0m\n";
        assert_eq!(
            scan_composer_draft(ComposerAgent::Codex, &format!("{ghost}{footer}")),
            ComposerScan::Empty
        );
        assert_eq!(
            scan_composer_draft(ComposerAgent::Codex, &format!("› Try another approach{footer}")),
            ComposerScan::Draft(vec!["Try another approach".into()])
        );
    }

    #[test]
    fn codex_multiline_draft_keeps_rows() {
        assert_eq!(
            scan_composer_draft(ComposerAgent::Codex, &codex_pane("› LINE1\n  LINE2\n  LINE3")),
            ComposerScan::Draft(vec![
                "LINE1".to_string(),
                "LINE2".to_string(),
                "LINE3".to_string()
            ])
        );
    }

    #[test]
    fn codex_wrapped_draft_is_captured_as_rows() {
        let pane =
            codex_pane("› AAA BBB CCC DDD EEE FFF GGG HHH III JJJ KKK LLL MMM NNN\n  OOO PPP");
        assert_eq!(
            scan_composer_draft(ComposerAgent::Codex, &pane),
            ComposerScan::Draft(vec![
                "AAA BBB CCC DDD EEE FFF GGG HHH III JJJ KKK LLL MMM NNN".to_string(),
                "OOO PPP".to_string()
            ])
        );
    }

    #[test]
    fn codex_transcript_prompt_is_not_the_composer() {
        let pane = "› earlier submitted message\n\n  assistant reply\n\n› \n \n  GPT-6.1-Sol medium · Context 0% used · Fast off\n";
        assert_eq!(scan_composer_draft(ComposerAgent::Codex, pane), ComposerScan::Empty);
    }

    #[test]
    fn codex_without_status_footer_is_unrecognized() {
        let pane = "› LINE1\n  LINE2\n";
        assert_eq!(scan_composer_draft(ComposerAgent::Codex, pane), ComposerScan::Unrecognized);
    }

    #[test]
    fn claude_draft_and_empty_are_distinguished() {
        assert_eq!(
            scan_composer_draft(ComposerAgent::Claude, &claude_pane("❯ DRAFT_PROBE_XYZ ")),
            ComposerScan::Draft(vec!["DRAFT_PROBE_XYZ".to_string()])
        );
        assert_eq!(
            scan_composer_draft(ComposerAgent::Claude, &claude_pane("❯      ")),
            ComposerScan::Empty
        );
        assert_eq!(
            scan_composer_draft(
                ComposerAgent::Claude,
                &claude_pane(
                    "❯ LINE1 \n  LINE2                                                        \n  LINE3   "
                )
            ),
            ComposerScan::Draft(vec![
                "LINE1".to_string(),
                "LINE2".to_string(),
                "LINE3".to_string()
            ])
        );
    }

    #[test]
    fn claude_without_rule_frame_is_unrecognized() {
        assert_eq!(
            scan_composer_draft(ComposerAgent::Claude, "❯ DRAFT\n"),
            ComposerScan::Unrecognized
        );
    }

    #[test]
    fn codebuddy_draft_and_empty_are_distinguished() {
        assert_eq!(
            scan_composer_draft(ComposerAgent::CodeBuddy, &codebuddy_pane("> DRAFT_PROBE_XYZ ")),
            ComposerScan::Draft(vec!["DRAFT_PROBE_XYZ".to_string()])
        );
        assert_eq!(
            scan_composer_draft(ComposerAgent::CodeBuddy, &codebuddy_pane(">  ")),
            ComposerScan::Empty
        );
        assert_eq!(
            scan_composer_draft(
                ComposerAgent::CodeBuddy,
                &codebuddy_pane("> LINE1\n  LINE2\n  LINE3 ")
            ),
            ComposerScan::Draft(vec![
                "LINE1".to_string(),
                "LINE2".to_string(),
                "LINE3".to_string()
            ])
        );
    }

    #[test]
    fn composer_codebuddy_live_suggestion_is_empty() {
        // s11390: the first suggested glyph is the inverse cursor, the rest dim.
        let pane = codebuddy_pane(
            "\x1b[38;5;188m>\x1b[39m \x1b[7m验\x1b[0;2m证回信已被对端确认收到\x1b[0m\n                 \x1b[2m↵ send\x1b[0m",
        );
        assert_eq!(scan_composer_draft(ComposerAgent::CodeBuddy, &pane), ComposerScan::Empty);
    }

    #[test]
    fn composer_codebuddy_multiline_live_suggestion_is_empty() {
        // s12455: CodeBuddy suggestion wraps across multiple lines; continuation rows are also dim.
        let pane = codebuddy_pane(
            "\x1b[38;5;188m>\x1b[39m \x1b[7m4\x1b[0;2m3-51 执行前重点核对 unwind 与 \x1b[0m        \x1b[2m↵ send\x1b[0m\n\x1b[0m  \x1b[2minsert_poison\x1b[0m",
        );
        assert_eq!(scan_composer_draft(ComposerAgent::CodeBuddy, &pane), ComposerScan::Empty);
    }

    #[test]
    fn composer_claude_multiline_live_suggestion_is_empty() {
        let pane = claude_pane(
            "❯ \x1b[2mfirst line of ghost suggestion\x1b[0m\n  \x1b[2msecond line of ghost suggestion\x1b[0m",
        );
        assert_eq!(scan_composer_draft(ComposerAgent::Claude, &pane), ComposerScan::Empty);
    }

    #[test]
    fn composer_suggestion_wording_can_be_real_draft() {
        for (agent, pane) in [
            (
                ComposerAgent::CodeBuddy,
                codebuddy_pane("> 保持待机等主侧日志\n                 \x1b[2m↵ send\x1b[0m"),
            ),
            (ComposerAgent::Claude, claude_pane("❯ 保持待机等主侧日志")),
        ] {
            assert_eq!(
                scan_composer_draft(agent, &pane),
                ComposerScan::Draft(vec!["保持待机等主侧日志".into()])
            );
        }
    }

    #[test]
    fn composer_styles_preserve_real_input_and_color_components() {
        for text in [
            "\x1b[38;5;2mZ\x1b[39m\x1b[7m \x1b[0m",
            "\x1b[38;2;2;7;22mZ\x1b[39m",
            "\x1b[7mZ\x1b[0m",
            "\x1b[2mghost\x1b[22mZ",
        ] {
            let pane = codebuddy_pane(&format!("> {text}"));
            let expected = if text.contains("ghost") {
                "ghostZ"
            } else {
                "Z"
            };
            assert_eq!(
                scan_composer_draft(ComposerAgent::CodeBuddy, &pane),
                ComposerScan::Draft(vec![expected.into()])
            );
        }
    }

    #[test]
    fn composer_dim_suggestion_with_cursor_and_style_reset() {
        let pane = claude_pane("❯ \x1b[2m保持待机等主侧日志\x1b[0m");
        assert_eq!(scan_composer_draft(ComposerAgent::Claude, &pane), ComposerScan::Empty);
        let pane = codebuddy_pane("> \x1b[2m建议文字\x1b[0m\n  REAL SECOND ROW");
        assert_eq!(
            scan_composer_draft(ComposerAgent::CodeBuddy, &pane),
            ComposerScan::Draft(vec!["REAL SECOND ROW".into()])
        );
    }

    #[test]
    fn clear_keystrokes_cover_every_row() {
        assert_eq!(composer_clear_keystrokes(0), "\u{15}");
        assert_eq!(composer_clear_keystrokes(1), "\u{15}");
        assert_eq!(composer_clear_keystrokes(3), "\u{15}\u{7f}\u{15}\u{7f}\u{15}");
    }

    #[test]
    fn agents_map_from_activity_labels() {
        let activity = |names: &[&str]| TerminalAgentActivity {
            agents: names.iter().map(|name| name.to_string()).collect(),
            orphaned_agents: Vec::new(),
        };
        assert_eq!(ComposerAgent::from_activity(&activity(&["Codex"])), Some(ComposerAgent::Codex));
        assert_eq!(
            ComposerAgent::from_activity(&activity(&["Claude"])),
            Some(ComposerAgent::Claude)
        );
        assert_eq!(
            ComposerAgent::from_activity(&activity(&["CodeBuddy"])),
            Some(ComposerAgent::CodeBuddy)
        );
        assert_eq!(ComposerAgent::from_activity(&activity(&["AGY"])), Some(ComposerAgent::Agy));
    }
}

#[cfg(test)]
mod agy_composer_tests {
    use super::*;
    #[test]
    fn agy_composer_distinguishes_native_empty_input_from_draft() {
        for (body, expected) in [
            ("", ComposerScan::Empty),
            ("保留草稿", ComposerScan::Draft(vec!["保留草稿".into()])),
        ] {
            let pane = format!(
                "⚠ Individual quota reached. Please upgrade your\nsubscription to increase your limits.\n────────────────────\n> {body}\n────────────────────\n? for shortcuts     Claude Opus 4.6 (Thinking)\n"
            );
            assert_eq!(scan_composer_draft(ComposerAgent::Agy, &pane), expected);
        }
    }
}
