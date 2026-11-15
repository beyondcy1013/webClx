//! Read native CodeArts receipts without altering its database or configuration.
use std::{collections::HashSet, fs, path::Path, time::Duration};

use anyhow::{Context, Result};
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde_json::Value;

use super::{
    COMPLETE_DETECTION_SCREEN_LINE_LIMIT, capture_tmux_activity_pane_snapshot,
    descendant_process_ids, process_cmdline_args, tmux_pane_pid,
};

fn valid_session_id(id: &str) -> bool {
    id.starts_with("ses_")
        && (8..=128).contains(&id.len())
        && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn current_session_id(args: &[String], screen: &str) -> Option<String> {
    let visible: HashSet<_> = screen
        .lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once("Id:")?;
            let id = rest.split_whitespace().next()?;
            valid_session_id(id).then_some(id)
        })
        .collect();
    // Never select the latest session from a shared database. Multiple screen
    // identities are ambiguous; a visible identity supersedes startup --session.
    if visible.len() > 1 {
        return None;
    }
    if let Some(id) = visible.into_iter().next() {
        return Some(id.to_string());
    }
    let ids: HashSet<_> = args
        .iter()
        .enumerate()
        .filter_map(|(index, arg)| {
            let id = arg
                .strip_prefix("--session=")
                .or_else(|| arg.strip_prefix("-s="))
                .or_else(|| {
                    matches!(arg.as_str(), "--session" | "-s")
                        .then(|| args.get(index + 1).map(String::as_str))
                        .flatten()
                })?;
            valid_session_id(id).then_some(id)
        })
        .collect();
    (ids.len() == 1).then(|| ids.into_iter().next().unwrap().to_string())
}

fn read_messages(db: &Path, id: &str, cwd: &Path) -> Result<Option<Vec<(String, u64)>>> {
    let connection = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .context("open CodeArts native receipt database read-only")?;
    connection.busy_timeout(Duration::from_millis(100))?;
    let directory: Option<String> = connection
        .query_row("SELECT directory FROM session WHERE id=?1", [id], |row| row.get(0))
        .optional()?;
    let Some(directory) = directory else {
        return Ok(None);
    };
    let directory = Path::new(&directory);
    let same_cwd = directory == cwd
        || fs::canonicalize(directory)
            .ok()
            .zip(fs::canonicalize(cwd).ok())
            .is_some_and(|(a, b)| a == b);
    if !same_cwd {
        return Ok(None);
    }

    let mut query = connection.prepare(
        "SELECT m.id, m.time_created, m.data, p.data FROM message m
         JOIN part p ON p.message_id=m.id AND p.session_id=m.session_id
         WHERE m.session_id=?1
         ORDER BY m.time_created DESC, m.id DESC, p.id DESC",
    )?;
    let rows = query.query_map([id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
        ))
    })?;
    let rows = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    let mut messages: Vec<(String, u64)> = Vec::new();
    let mut last_id = String::new();
    for (message_id, created, message, part) in rows.into_iter().rev() {
        let message: Value = serde_json::from_str(&message)?;
        if message.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let part: Value = serde_json::from_str(&part)?;
        if part.get("type").and_then(Value::as_str) != Some("text")
            || part.get("synthetic").and_then(Value::as_bool) == Some(true)
            || part.get("ignored").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        let Some(text) = part
            .get("text")
            .and_then(Value::as_str)
            .filter(|s| !s.trim().is_empty())
        else {
            continue;
        };
        if last_id == message_id {
            if let Some((body, _)) = messages.last_mut() {
                body.push('\n');
                body.push_str(text);
            }
        } else {
            messages.push((text.to_string(), u64::try_from(created).unwrap_or(0)));
            last_id = message_id;
        }
    }
    Ok(Some(messages))
}

pub(in crate::terminal) fn codearts_user_messages_for_terminal(
    terminal: &str,
) -> Result<Option<Vec<(String, u64)>>> {
    let Some(pane_pid) = tmux_pane_pid(terminal)? else {
        return Ok(None);
    };
    let mut targets = Vec::new();
    for pid in descendant_process_ids(pane_pid) {
        let args = process_cmdline_args(pid);
        if args
            .first()
            .and_then(|s| Path::new(s).file_name())
            .and_then(|s| s.to_str())
            != Some("codearts")
        {
            continue;
        }
        let Ok(fds) = fs::read_dir(format!("/proc/{pid}/fd")) else {
            continue;
        };
        let databases: HashSet<_> = fds
            .filter_map(|entry| fs::read_link(entry.ok()?.path()).ok())
            .filter(|p| p.ends_with(".codeartsdoer/cli-data/opencode.db"))
            .collect();
        if databases.len() != 1 {
            continue;
        }
        let Ok(cwd) = fs::read_link(format!("/proc/{pid}/cwd")) else {
            continue;
        };
        targets.push((args, databases.into_iter().next().unwrap(), cwd));
    }
    if targets.len() != 1 {
        return Ok(None);
    }
    let (args, db, cwd) = targets.pop().unwrap();
    let screen =
        capture_tmux_activity_pane_snapshot(terminal, COMPLETE_DETECTION_SCREEN_LINE_LIMIT)?;
    let Some(id) = current_session_id(&args, &String::from_utf8_lossy(&screen)) else {
        return Ok(None);
    };
    read_messages(&db, &id, &cwd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::PathBuf;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let root = std::env::temp_dir()
                .join(format!("webclx-codearts-receipt-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&root).unwrap();
            Self(root)
        }
        fn db(&self) -> PathBuf {
            self.0.join("opencode.db")
        }
        fn seed(&self) {
            let c = Connection::open(self.db()).unwrap();
            c.execute_batch("CREATE TABLE session(id TEXT PRIMARY KEY, directory TEXT);
                CREATE TABLE message(id TEXT PRIMARY KEY, session_id TEXT, time_created INTEGER, data TEXT);
                CREATE TABLE part(id TEXT PRIMARY KEY, message_id TEXT, session_id TEXT, data TEXT);").unwrap();
            for id in ["ses_current123", "ses_other123"] {
                c.execute(
                    "INSERT INTO session VALUES(?1,?2)",
                    rusqlite::params![id, self.0.to_str().unwrap()],
                )
                .unwrap();
            }
            for (id, sid, role) in [
                ("m1", "ses_current123", "user"),
                ("m2", "ses_current123", "assistant"),
                ("m3", "ses_other123", "user"),
            ] {
                c.execute(
                    "INSERT INTO message VALUES(?1,?2,12345,?3)",
                    rusqlite::params![id, sid, json!({"role":role}).to_string()],
                )
                .unwrap();
            }
            for (id, mid, sid, data) in [
                (
                    "p1",
                    "m1",
                    "ses_current123",
                    json!({"type":"text", "text":"[scheduled:current] 你好"}),
                ),
                ("p2", "m1", "ses_current123", json!({"type":"text", "text":"第二段"})),
                (
                    "p3",
                    "m1",
                    "ses_current123",
                    json!({"type":"text", "text":"[scheduled:synthetic]", "synthetic":true}),
                ),
                (
                    "p4",
                    "m1",
                    "ses_current123",
                    json!({"type":"text", "text":"[scheduled:ignored]", "ignored":true}),
                ),
                (
                    "p5",
                    "m2",
                    "ses_current123",
                    json!({"type":"text", "text":"[scheduled:assistant]"}),
                ),
                ("p6", "m3", "ses_other123", json!({"type":"text", "text":"[scheduled:other]"})),
                ("p7", "m1", "ses_current123", json!({"type":"tool", "text":"[scheduled:tool]"})),
            ] {
                c.execute(
                    "INSERT INTO part VALUES(?1,?2,?3,?4)",
                    rusqlite::params![id, mid, sid, data.to_string()],
                )
                .unwrap();
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn codearts_receipt_reads_only_current_native_user_text_without_writes() {
        let fixture = Fixture::new();
        fixture.seed();
        let before = fs::read(fixture.db()).unwrap();
        let messages = read_messages(&fixture.db(), "ses_current123", &fixture.0)
            .unwrap()
            .unwrap();
        assert_eq!(messages, vec![("[scheduled:current] 你好\n第二段".into(), 12345)]);
        assert_eq!(fs::read(fixture.db()).unwrap(), before);
        assert!(
            read_messages(&fixture.db(), "ses_missing123", &fixture.0)
                .unwrap()
                .is_none()
        );
        assert!(
            read_messages(&fixture.db(), "ses_current123", &fixture.0.join("other"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn codearts_receipt_surfaces_database_errors_and_never_creates_missing_db() {
        let fixture = Fixture::new();
        assert!(read_messages(&fixture.db(), "ses_current123", &fixture.0).is_err());
        assert!(!fixture.db().exists());
        fs::write(fixture.db(), b"corrupt database").unwrap();
        assert!(read_messages(&fixture.db(), "ses_current123", &fixture.0).is_err());
    }

    #[test]
    fn codearts_receipt_uses_explicit_identity_not_latest_shared_session() {
        let args = vec!["codearts".into(), "--session=ses_startup123".into()];
        assert_eq!(
            current_session_id(&args, "  Id: ses_current123   █"),
            Some("ses_current123".into())
        );
        assert_eq!(current_session_id(&args, "no sidebar"), Some("ses_startup123".into()));
        assert_eq!(current_session_id(&args, "Id: ses_one123\nId: ses_two123"), None);
        assert_eq!(current_session_id(&["codearts".into()], "working"), None);
        assert_eq!(
            current_session_id(&["codearts".into(), "-s".into(), "ses_short123".into()], ""),
            Some("ses_short123".into())
        );
        assert_eq!(
            current_session_id(&["codearts".into(), "-s".into(), "../secret".into()], ""),
            None
        );
    }
}
