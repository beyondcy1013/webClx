use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use tokio::{io::AsyncWriteExt, process::Command, sync::RwLock};

use crate::{ApiResult, AppError};

use super::{current_timestamp, generate_id};

pub const BACKGROUND_COMMANDS_FILE: &str = ".webclx-agent-background-commands.json";
const MAX_BACKGROUND_OUTPUT_BYTES: usize = 128 * 1024;
const DEFAULT_ROWS: u16 = 30;
const DEFAULT_COLS: u16 = 120;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackgroundCommandSession {
    pub id: String,
    pub agent_session_id: String,
    pub command: String,
    pub cwd: String,
    pub status: String,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub output_truncated: bool,
    pub rows: u16,
    pub cols: u16,
    pub tmux_session: String,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Clone)]
pub struct BackgroundCommandManager {
    sessions: Arc<RwLock<HashMap<String, BackgroundCommandSession>>>,
    file_path: PathBuf,
}

impl BackgroundCommandManager {
    pub fn new(app_dir: &Path) -> Self {
        let file_path = app_dir.join(BACKGROUND_COMMANDS_FILE);
        let sessions = std::fs::read_to_string(&file_path)
            .ok()
            .and_then(|content| serde_json::from_str(&content).ok())
            .unwrap_or_default();
        Self {
            sessions: Arc::new(RwLock::new(sessions)),
            file_path,
        }
    }

    fn persist(&self, sessions: &HashMap<String, BackgroundCommandSession>) {
        if let Ok(encoded) = serde_json::to_vec_pretty(sessions) {
            let _ = std::fs::write(&self.file_path, encoded);
        }
    }

    pub async fn start(
        &self,
        agent_session_id: &str,
        command_text: &str,
        cwd: &Path,
        rows: Option<u16>,
        cols: Option<u16>,
    ) -> ApiResult<BackgroundCommandSession> {
        if command_text.trim().is_empty() {
            return Err(AppError::bad_request("后台命令不能为空。"));
        }
        let rows = rows.unwrap_or(DEFAULT_ROWS).clamp(2, 500);
        let cols = cols.unwrap_or(DEFAULT_COLS).clamp(20, 1000);
        let id = format!("cmd-{}", generate_id());
        let tmux_session = format!("webclx-agent-bg-{}", id.trim_start_matches("cmd-"));
        let cwd_text = cwd.to_string_lossy().to_string();
        let cols_text = cols.to_string();
        let rows_text = rows.to_string();
        run_tmux([
            "new-session",
            "-d",
            "-s",
            tmux_session.as_str(),
            "-c",
            cwd_text.as_str(),
            "-x",
            cols_text.as_str(),
            "-y",
            rows_text.as_str(),
            "bash",
        ])
        .await
        .map_err(|error| AppError::internal(format!("启动 tmux 后台会话失败: {error}")))?;
        if let Err(error) = configure_tmux_session(&tmux_session, rows, cols).await {
            let _ = run_tmux(["kill-session", "-t", tmux_session.as_str()]).await;
            return Err(AppError::internal(format!("配置 tmux 后台会话失败: {error}")));
        }
        let wrapper = format!("exec bash -lc {}", shell_quote(command_text));
        if let Err(error) = send_literal_keys(&tmux_session, &wrapper, true).await {
            let _ = run_tmux(["kill-session", "-t", tmux_session.as_str()]).await;
            return Err(AppError::internal(format!("启动后台命令失败: {error}")));
        }

        let timestamp = current_timestamp();
        let record = BackgroundCommandSession {
            id: id.clone(),
            agent_session_id: agent_session_id.to_string(),
            command: command_text.to_string(),
            cwd: cwd.display().to_string(),
            status: "running".to_string(),
            pid: None,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            output_truncated: false,
            rows,
            cols,
            tmux_session,
            created_at: timestamp,
            updated_at: timestamp,
        };
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(id.clone(), record);
            self.persist(&sessions);
        }
        self.get(agent_session_id, &id).await
    }

    pub async fn recover_sessions(&self) {
        let ids = self
            .sessions
            .read()
            .await
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for id in ids {
            let _ = self.refresh(&id).await;
        }
    }

    pub async fn list(&self, agent_session_id: &str) -> Vec<BackgroundCommandSession> {
        self.recover_sessions().await;
        let mut records = self
            .sessions
            .read()
            .await
            .values()
            .filter(|record| record.agent_session_id == agent_session_id)
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        records
    }

    pub async fn get(
        &self,
        agent_session_id: &str,
        command_id: &str,
    ) -> ApiResult<BackgroundCommandSession> {
        self.ensure_owned(agent_session_id, command_id).await?;
        self.refresh(command_id).await
    }

    pub async fn write_stdin(
        &self,
        agent_session_id: &str,
        command_id: &str,
        input: &str,
    ) -> ApiResult<BackgroundCommandSession> {
        let record = self.get(agent_session_id, command_id).await?;
        if record.status != "running" {
            return Err(AppError::bad_request("后台命令已经结束，无法写入。"));
        }
        paste_buffer(&record.tmux_session, input)
            .await
            .map_err(|error| AppError::bad_request(format!("写入后台命令失败: {error}")))?;
        self.get(agent_session_id, command_id).await
    }

    pub async fn terminate(
        &self,
        agent_session_id: &str,
        command_id: &str,
    ) -> ApiResult<BackgroundCommandSession> {
        let mut record = self.get(agent_session_id, command_id).await?;
        if record.status == "running" {
            run_tmux(["kill-session", "-t", record.tmux_session.as_str()])
                .await
                .map_err(|error| AppError::internal(format!("终止后台命令失败: {error}")))?;
            record.status = "terminated".to_string();
            record.updated_at = current_timestamp();
            let mut sessions = self.sessions.write().await;
            sessions.insert(command_id.to_string(), record.clone());
            self.persist(&sessions);
        }
        Ok(record)
    }

    pub async fn terminate_all(&self, agent_session_id: &str) {
        let command_ids = self
            .list(agent_session_id)
            .await
            .into_iter()
            .filter(|record| record.status == "running")
            .map(|record| record.id)
            .collect::<Vec<_>>();
        for command_id in command_ids {
            let _ = self.terminate(agent_session_id, &command_id).await;
        }
    }

    async fn ensure_owned(&self, agent_session_id: &str, command_id: &str) -> ApiResult<()> {
        let sessions = self.sessions.read().await;
        let record = sessions
            .get(command_id)
            .ok_or_else(|| AppError::not_found("后台命令会话不存在。"))?;
        if record.agent_session_id != agent_session_id {
            return Err(AppError::not_found("后台命令会话不存在。"));
        }
        Ok(())
    }

    async fn refresh(&self, command_id: &str) -> ApiResult<BackgroundCommandSession> {
        let mut record = self
            .sessions
            .read()
            .await
            .get(command_id)
            .cloned()
            .ok_or_else(|| AppError::not_found("后台命令会话不存在。"))?;
        if record.status == "running" {
            match pane_status(&record.tmux_session).await {
                Ok(status) => {
                    record.pid = status.pid;
                    record.rows = status.rows;
                    record.cols = status.cols;
                    if status.dead {
                        record.exit_code = status.exit_code;
                        record.status = if status.exit_code == Some(0) {
                            "completed"
                        } else {
                            "failed"
                        }
                        .to_string();
                    }
                    if let Ok(output) = capture_pane(&record.tmux_session).await {
                        record.output_truncated |= replace_bounded(&mut record.stdout, output);
                    }
                }
                Err(_) => {
                    record.status = "lost".to_string();
                }
            }
            record.updated_at = current_timestamp();
            let mut sessions = self.sessions.write().await;
            sessions.insert(command_id.to_string(), record.clone());
            self.persist(&sessions);
        }
        Ok(record)
    }
}

async fn configure_tmux_session(session: &str, rows: u16, cols: u16) -> Result<(), String> {
    let rows = rows.to_string();
    let cols = cols.to_string();
    run_tmux(["set-option", "-t", session, "status", "off"]).await?;
    run_tmux(["set-window-option", "-t", session, "window-size", "manual"]).await?;
    run_tmux([
        "resize-window",
        "-t",
        session,
        "-x",
        cols.as_str(),
        "-y",
        rows.as_str(),
    ])
    .await?;
    run_tmux(["set-option", "-t", session, "remain-on-exit", "on"]).await?;
    Ok(())
}

struct PaneStatus {
    dead: bool,
    exit_code: Option<i32>,
    pid: Option<u32>,
    cols: u16,
    rows: u16,
}

async fn pane_status(session: &str) -> Result<PaneStatus, String> {
    let output = run_tmux([
        "list-panes",
        "-t",
        session,
        "-F",
        "#{pane_dead}\t#{pane_dead_status}\t#{pane_pid}\t#{pane_width}\t#{pane_height}",
    ])
    .await?;
    let values = output.trim().split('\t').collect::<Vec<_>>();
    if values.len() != 5 {
        return Err("tmux 返回了无效的 pane 状态。".to_string());
    }
    Ok(PaneStatus {
        dead: values[0] == "1",
        exit_code: values[1].parse().ok(),
        pid: values[2].parse().ok(),
        cols: values[3].parse().unwrap_or(DEFAULT_COLS),
        rows: values[4].parse().unwrap_or(DEFAULT_ROWS),
    })
}

async fn capture_pane(session: &str) -> Result<String, String> {
    run_tmux(["capture-pane", "-p", "-J", "-S", "-32768", "-t", session]).await
}

async fn send_literal_keys(session: &str, text: &str, enter: bool) -> Result<(), String> {
    run_tmux(["send-keys", "-t", session, "-l", text]).await?;
    if enter {
        run_tmux(["send-keys", "-t", session, "Enter"]).await?;
    }
    Ok(())
}

async fn paste_buffer(session: &str, input: &str) -> Result<(), String> {
    let buffer = format!("webclx-agent-input-{}", generate_id());
    let mut child = Command::new("tmux")
        .args(["load-buffer", "-b", buffer.as_str(), "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|error| error.to_string())?;
    }
    let output = child
        .wait_with_output()
        .await
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    run_tmux(["paste-buffer", "-d", "-b", buffer.as_str(), "-t", session]).await?;
    Ok(())
}

async fn run_tmux<const N: usize>(args: [&str; N]) -> Result<String, String> {
    let output = Command::new("tmux")
        .args(args)
        .output()
        .await
        .map_err(|error| error.to_string())?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn replace_bounded(target: &mut String, mut output: String) -> bool {
    if output.len() <= MAX_BACKGROUND_OUTPUT_BYTES {
        *target = output;
        return false;
    }
    let mut start = output.len() - MAX_BACKGROUND_OUTPUT_BYTES;
    while start < output.len() && !output.is_char_boundary(start) {
        start += 1;
    }
    output.drain(..start);
    *target = output;
    true
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::sleep;

    use super::*;

    fn test_manager() -> BackgroundCommandManager {
        BackgroundCommandManager::new(Path::new("/tmp"))
    }

    #[test]
    fn background_output_keeps_the_latest_bounded_content() {
        let mut output = String::new();
        assert!(replace_bounded(
            &mut output,
            format!("{}latest", "a".repeat(MAX_BACKGROUND_OUTPUT_BYTES))
        ));
        assert_eq!(output.len(), MAX_BACKGROUND_OUTPUT_BYTES);
        assert!(output.ends_with("latest"));
    }

    #[tokio::test]
    async fn background_command_accepts_stdin_and_keeps_final_output() {
        let manager = test_manager();
        let started = manager
            .start(
                "agent-test",
                "printf ready; read line; printf ':received:%s' \"$line\"",
                Path::new("/tmp"),
                Some(24),
                Some(100),
            )
            .await
            .expect("start background command");
        assert_eq!(started.rows, 24);
        assert_eq!(started.cols, 100);
        wait_for_status(&manager, &started.id, |record| record.stdout.contains("ready")).await;
        manager
            .write_stdin("agent-test", &started.id, "hello\n")
            .await
            .expect("write command stdin");
        let completed =
            wait_for_status(&manager, &started.id, |record| record.status == "completed").await;
        assert_eq!(completed.exit_code, Some(0));
        assert!(completed.stdout.contains("ready:received:hello"));
        let _ = run_tmux(["kill-session", "-t", completed.tmux_session.as_str()]).await;
    }

    #[tokio::test]
    async fn background_command_termination_stops_the_tmux_session() {
        let manager = test_manager();
        let started = manager
            .start("agent-test", "sleep 30", Path::new("/tmp"), None, None)
            .await
            .expect("start background command");
        let terminated = manager
            .terminate("agent-test", &started.id)
            .await
            .expect("terminate command");
        assert_eq!(terminated.status, "terminated");
    }

    async fn wait_for_status(
        manager: &BackgroundCommandManager,
        command_id: &str,
        predicate: impl Fn(&BackgroundCommandSession) -> bool,
    ) -> BackgroundCommandSession {
        for _ in 0..100 {
            let record = manager
                .get("agent-test", command_id)
                .await
                .expect("read background command");
            if predicate(&record) {
                return record;
            }
            sleep(Duration::from_millis(50)).await;
        }
        panic!("background command did not reach the expected state");
    }
}
