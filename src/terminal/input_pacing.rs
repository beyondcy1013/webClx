//! 终端输入的分段投递。
//!
//! 基于 ink 的 Agent TUI 把一次 stdin 读取当成一次按键：读取内容多于一个
//! 字节时按“粘贴文本”处理，其中的退格字节（DEL 0x7f / BS 0x08）会被整段
//! 丢弃。webClx 会把连续按键合并成一次写入，于是连续退格可能落在同一次
//! 读取里，表现为“删着删着就删不动了”。
//!
//! 这里把退格键单独成段投递，并在段之间留出间隔，避免 Agent 的读取循环
//! 再次把它们合并。

use std::{io, thread, time::Duration};

/// 两次写入之间的最小间隔。Agent 的 stdin 读取间隔约 1-2ms，实测 1ms 仍会
/// 被合并、2ms 起不再合并，取 5ms 留出余量。
pub(super) const ERASE_KEY_WRITE_GAP: Duration = Duration::from_millis(5);

fn is_erase_key_char(ch: char) -> bool {
    ch == '\u{7f}' || ch == '\u{8}'
}

/// 按退格键切分输入：每个退格字节单独成段，其余文本保持原有顺序。
pub(super) fn erase_key_segments(data: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let mut start = 0;
    for (index, ch) in data.char_indices() {
        if !is_erase_key_char(ch) {
            continue;
        }
        if start < index {
            segments.push(&data[start..index]);
        }
        segments.push(&data[index..index + 1]);
        start = index + 1;
    }
    if start < data.len() {
        segments.push(&data[start..]);
    }
    segments
}

/// 逐段写入输入，退格键各自成段并留出间隔。
pub(super) fn write_input_paced<W: io::Write + ?Sized>(
    writer: &mut W,
    data: &str,
) -> io::Result<()> {
    for (index, segment) in erase_key_segments(data).into_iter().enumerate() {
        if index > 0 {
            thread::sleep(ERASE_KEY_WRITE_GAP);
        }
        writer.write_all(segment.as_bytes())?;
        writer.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct RecordingWriter {
        writes: Vec<String>,
    }

    impl io::Write for RecordingWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.writes.push(String::from_utf8_lossy(buf).to_string());
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn erase_key_segments_keeps_plain_text_whole() {
        assert_eq!(erase_key_segments("hello"), vec!["hello"]);
        assert_eq!(erase_key_segments(""), Vec::<&str>::new());
    }

    #[test]
    fn erase_key_segments_isolates_every_erase_key() {
        assert_eq!(erase_key_segments("abc\u{7f}\u{7f}"), vec!["abc", "\u{7f}", "\u{7f}"]);
        assert_eq!(erase_key_segments("\u{8}a\u{7f}b"), vec!["\u{8}", "a", "\u{7f}", "b"]);
    }

    #[test]
    fn write_input_paced_sends_each_erase_key_separately() {
        let mut writer = RecordingWriter::default();

        write_input_paced(&mut writer, "ab\u{7f}\u{7f}").expect("write");

        assert_eq!(writer.writes, vec!["ab", "\u{7f}", "\u{7f}"]);
    }

    #[test]
    fn write_input_paced_keeps_single_write_for_text_only_input() {
        let mut writer = RecordingWriter::default();

        write_input_paced(&mut writer, "plain text\r").expect("write");

        assert_eq!(writer.writes, vec!["plain text\r"]);
    }
}
