use std::{
    path::Path,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result};
use terminal_core::*;
use tracing::warn;

use crate::runtime_paths;

use super::sanitize_child_command;

pub(super) mod command_output;

static TMUX_SCOPE_ISOLATION_AVAILABLE: AtomicBool = AtomicBool::new(true);
const TMUX_HISTORY_LIMIT: &str = "100000";
const TMUX_TERMINAL_OVERRIDES: &str = "xterm-256color:indn@:rin@";
/// 同步 tmux socket 命令的执行预算：正常返回 <1s，10s 只为挂死的服务器
/// socket 兜底，保证阻塞线程最终能被回收。
pub(super) const TMUX_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// set-option/send-keys/kill-session 等只读 stderr 的命令的输出上限。
const TMUX_SMALL_OUTPUT_LIMIT: usize = 64 * 1024;
/// list-panes/list-clients 等列表命令的输出上限。
const TMUX_LIST_OUTPUT_LIMIT: usize = 1024 * 1024;
/// capture-pane 全量历史快照的输出上限（history-limit 10 万行）。
const TMUX_SNAPSHOT_OUTPUT_LIMIT: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TmuxSessionStatus {
    Exists,
    Missing,
    Unknown,
}

fn should_isolate_tmux_scope() -> bool {
    #[cfg(test)]
    {
        // Unit tests inherit systemd's INVOCATION_ID from the compile worker.
        // Reusing it would wrap every isolated test tmux server in a transient
        // scope, multiplying systemd churn and making load-sensitive suites fail.
        return false;
    }
    #[allow(unreachable_code)]
    {
        TMUX_SCOPE_ISOLATION_AVAILABLE.load(Ordering::Relaxed)
            && std::env::var_os("INVOCATION_ID").is_some()
    }
}

pub(super) fn tmux_socket_name(session_id: &str) -> String {
    format!("webclx_{session_id}")
}

pub(super) fn isolated_tmux_command(session_id: &str) -> Command {
    let mut command = Command::new("tmux");
    command.arg("-L").arg(tmux_socket_name(session_id));
    command
}

pub(super) fn tmux_session_status(session_id: &str) -> TmuxSessionStatus {
    let mut command = isolated_tmux_command(session_id);
    sanitize_child_command(&mut command);
    command
        .arg("has-session")
        .arg("-t")
        .arg(tmux_session_name(session_id));
    let output = command_output::bounded_command_output(
        command,
        TMUX_COMMAND_TIMEOUT,
        TMUX_SMALL_OUTPUT_LIMIT,
    );
    match output {
        Ok(output) if output.status.success() => TmuxSessionStatus::Exists,
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if tmux_missing_session_error(&stderr) {
                TmuxSessionStatus::Missing
            } else {
                warn!(
                    "tmux session status unknown for {}: {}",
                    tmux_session_name(session_id),
                    if stderr.is_empty() {
                        format!("tmux exited with {}", output.status)
                    } else {
                        stderr
                    }
                );
                TmuxSessionStatus::Unknown
            }
        }
        Err(error) => {
            warn!("tmux session status unknown for {}: {error}", tmux_session_name(session_id));
            TmuxSessionStatus::Unknown
        }
    }
}

pub(super) fn tmux_session_exists(session_id: &str) -> bool {
    tmux_session_status(session_id) == TmuxSessionStatus::Exists
}

/// Query every pane in every window before destroying the isolated session.
fn tmux_pane_roots(session_id: &str) -> Result<(Option<u32>, Vec<u32>)> {
    let mut command = isolated_tmux_command(session_id);
    sanitize_child_command(&mut command);
    command.args([
        "list-panes",
        "-s",
        "-t",
        &tmux_session_name(session_id),
        "-F",
        "#{pid} #{pane_pid}",
    ]);
    let output = command_output::bounded_command_output(
        command,
        TMUX_COMMAND_TIMEOUT,
        TMUX_LIST_OUTPUT_LIMIT,
    )
    .context("读取 tmux pane 失败")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        if tmux_missing_session_error(&stderr) {
            return Ok((None, Vec::new()));
        }
        anyhow::bail!("读取 tmux panes 失败: {stderr}");
    }
    let mut server_pid = None;
    let mut roots = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let values = line
            .split_whitespace()
            .map(str::parse::<u32>)
            .collect::<std::result::Result<Vec<_>, _>>()
            .context("tmux 返回无效 PID")?;
        anyhow::ensure!(
            values.len() == 2 && values.iter().all(|pid| *pid > 1),
            "tmux 返回无效 server/pane PID"
        );
        anyhow::ensure!(server_pid.is_none_or(|pid| pid == values[0]), "tmux server 身份不一致");
        server_pid = Some(values[0]);
        roots.push(values[1]);
    }
    anyhow::ensure!(server_pid.is_some(), "tmux 返回空 pane 列表");
    Ok((server_pid, roots))
}

pub(super) fn detach_tmux_clients_for_sessions(session_ids: &[String]) -> Result<()> {
    for session_id in session_ids {
        let mut command = isolated_tmux_command(session_id);
        sanitize_child_command(&mut command);
        command.arg("list-clients").arg("-F").arg("#{client_name}");
        let output = command_output::bounded_command_output(
            command,
            TMUX_COMMAND_TIMEOUT,
            TMUX_SMALL_OUTPUT_LIMIT,
        )
        .context("无法列出 tmux 终端客户端")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            if stderr.contains("no clients") || tmux_missing_session_error(&stderr) {
                continue;
            }
            anyhow::bail!("无法列出 tmux 终端客户端: {stderr}");
        }

        for client_name in tmux_client_names(&output.stdout) {
            let mut command = isolated_tmux_command(session_id);
            sanitize_child_command(&mut command);
            command.arg("detach-client").arg("-t").arg(client_name);
            let output = command_output::bounded_command_output(
                command,
                TMUX_COMMAND_TIMEOUT,
                TMUX_SMALL_OUTPUT_LIMIT,
            )
            .context("无法断开 tmux 终端客户端")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if stderr.contains("no such client") || tmux_missing_session_error(&stderr) {
                    continue;
                }
                anyhow::bail!("无法断开 tmux 终端客户端: {stderr}");
            }
        }
    }
    Ok(())
}

pub(super) fn tmux_client_names(output: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .filter(|client_name| !client_name.is_empty())
        .map(str::to_string)
        .collect()
}

/// 按指定行数抓取带转义序列的 pane 快照；行数来自设置
/// `terminal_scrollback_lines`，决定首屏回放可回看的历史深度。
pub(super) fn capture_tmux_initial_pane_snapshot_with_limit(
    session_id: &str,
    line_limit: u32,
) -> Result<Vec<u8>> {
    capture_tmux_pane_snapshot_from(session_id, &format!("-{line_limit}"), true)
}

pub(super) fn capture_tmux_recent_pane_snapshot(session_id: &str) -> Result<Vec<u8>> {
    capture_tmux_joined_pane_snapshot_from(session_id, "-200", false)
}

pub(super) fn capture_tmux_activity_pane_snapshot(
    session_id: &str,
    line_limit: u32,
) -> Result<Vec<u8>> {
    let command =
        tmux_snapshot_command(session_id, &format!("-{}", line_limit.max(200)), false, true);
    let output = command_output::bounded_command_output(
        command,
        TMUX_COMMAND_TIMEOUT,
        TMUX_SNAPSHOT_OUTPUT_LIMIT,
    )
    .context("读取 tmux 活动快照失败")?;
    decode_tmux_snapshot(output)
}

/// Only the visible pane, excluding queue banners left in scrollback.
pub(super) fn capture_tmux_visible_text_pane_snapshot(session_id: &str) -> Result<Vec<u8>> {
    capture_tmux_joined_pane_snapshot_from(session_id, "0", false)
}

pub(super) fn capture_tmux_text_pane_snapshot(session_id: &str) -> Result<Vec<u8>> {
    capture_tmux_pane_snapshot_from(session_id, "-", false)
}

fn capture_tmux_pane_snapshot_from(
    session_id: &str,
    start_line: &str,
    include_escape_sequences: bool,
) -> Result<Vec<u8>> {
    capture_tmux_pane_snapshot_from_options(session_id, start_line, include_escape_sequences, false)
}

fn capture_tmux_joined_pane_snapshot_from(
    session_id: &str,
    start_line: &str,
    include_escape_sequences: bool,
) -> Result<Vec<u8>> {
    capture_tmux_pane_snapshot_from_options(session_id, start_line, include_escape_sequences, true)
}

fn capture_tmux_pane_snapshot_from_options(
    session_id: &str,
    start_line: &str,
    include_escape_sequences: bool,
    join_wrapped_lines: bool,
) -> Result<Vec<u8>> {
    let command =
        tmux_snapshot_command(session_id, start_line, include_escape_sequences, join_wrapped_lines);
    let output = command_output::bounded_command_output(
        command,
        TMUX_COMMAND_TIMEOUT,
        TMUX_SNAPSHOT_OUTPUT_LIMIT,
    )
    .context("读取 tmux 终端历史失败")?;
    decode_tmux_snapshot(output)
}

fn tmux_snapshot_command(
    session_id: &str,
    start_line: &str,
    include_escape_sequences: bool,
    join_wrapped_lines: bool,
) -> Command {
    let mut command = isolated_tmux_command(session_id);
    sanitize_child_command(&mut command);
    command.arg("capture-pane");
    if include_escape_sequences {
        command.arg("-e");
    }
    if join_wrapped_lines {
        command.arg("-J");
    }
    command
        .arg("-p")
        .arg("-S")
        .arg(start_line)
        .arg("-t")
        .arg(tmux_session_name(session_id));
    command
}

// Bound both admission and execution; no blocking worker survives a timeout.
static INITIAL_SNAPSHOT_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);
const INITIAL_SNAPSHOT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
const TMUX_CLIENT_COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub(super) async fn capture_tmux_initial_pane_snapshot_async(
    session_id: &str,
    line_limit: u32,
) -> Result<Vec<u8>> {
    let command = tmux_snapshot_command(session_id, &format!("-{line_limit}"), true, false);
    match bounded_snapshot_output(command, &INITIAL_SNAPSHOT_SLOTS, INITIAL_SNAPSHOT_TIMEOUT).await
    {
        Ok(output) => decode_tmux_snapshot(output),
        Err(err) => {
            warn!(
                "tmux 首屏深度快照失败或超时 ({session_id}, limit={line_limit}): {err}，降级抓取最近 200 行"
            );
            let fallback_cmd = tmux_snapshot_command(session_id, "-200", true, false);
            let fallback_output = bounded_snapshot_output(
                fallback_cmd,
                &INITIAL_SNAPSHOT_SLOTS,
                std::time::Duration::from_secs(3),
            )
            .await?;
            decode_tmux_snapshot(fallback_output)
        }
    }
}

async fn bounded_snapshot_output(
    command: Command,
    slots: &tokio::sync::Semaphore,
    budget: std::time::Duration,
) -> Result<std::process::Output> {
    let deadline = tokio::time::Instant::now() + budget;
    let _permit = tokio::time::timeout_at(deadline, slots.acquire())
        .await
        .context("首屏快照并发等待超时")?
        .context("首屏快照队列已关闭")?;
    let mut command = tokio::process::Command::from(command);
    // Tokio's child guard kills on cancellation/timeout and reaps the child.
    // output() drains stdout and stderr concurrently, including large snapshots.
    command.kill_on_drop(true);
    tokio::time::timeout_at(deadline, command.output())
        .await
        .context("tmux 首屏快照执行超时（已取消子进程）")?
        .context("启动或读取 tmux 首屏快照失败")
}

fn decode_tmux_snapshot(output: std::process::Output) -> Result<Vec<u8>> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "读取 tmux 终端历史失败: {}",
            if stderr.is_empty() {
                format!("tmux exited with {}", output.status)
            } else {
                stderr
            }
        );
    }
    Ok(normalize_tmux_snapshot(&output.stdout))
}

/// 接入回放后强制 tmux 客户端整屏重绘：快照抓取窗口内被重复回放的旧 chunk
/// 可能把新内容画成旧内容，整屏重绘以当前 pane 为准兜底。尽力而为，失败只告警。
pub(super) async fn request_tmux_client_refresh(session_id: &str) {
    // refresh-client 的目标必须是 list-clients 返回的 client_name（如
    // /dev/pts/17），会话名会报 "can't find client"。
    let mut list_command = isolated_tmux_command(session_id);
    sanitize_child_command(&mut list_command);
    list_command
        .arg("list-clients")
        .arg("-F")
        .arg("#{client_name}");
    let output = match bounded_snapshot_output(
        list_command,
        &INITIAL_SNAPSHOT_SLOTS,
        TMUX_CLIENT_COMMAND_TIMEOUT,
    )
    .await
    {
        Ok(output) => output,
        Err(error) => {
            warn!("tmux 整屏重绘：列出客户端失败 ({session_id}): {error}");
            return;
        }
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        // 无客户端或会话已消失属于正常竞态，静默跳过。
        if !stderr.contains("no clients") && !tmux_missing_session_error(&stderr) {
            warn!(
                "tmux 整屏重绘：列出客户端失败 ({session_id}): {}",
                if stderr.is_empty() {
                    format!("tmux exited with {}", output.status)
                } else {
                    stderr
                }
            );
        }
        return;
    }
    for client_name in tmux_client_names(&output.stdout) {
        let mut command = isolated_tmux_command(session_id);
        sanitize_child_command(&mut command);
        command.arg("refresh-client").arg("-t").arg(&client_name);
        match bounded_snapshot_output(command, &INITIAL_SNAPSHOT_SLOTS, TMUX_CLIENT_COMMAND_TIMEOUT)
            .await
        {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                // 客户端在列出后断开同样属于正常竞态。
                if !stderr.contains("no such client") && !tmux_missing_session_error(&stderr) {
                    warn!(
                        "tmux 整屏重绘失败 ({session_id}, client {client_name}): {}",
                        if stderr.is_empty() {
                            format!("tmux exited with {}", output.status)
                        } else {
                            stderr
                        }
                    );
                }
            }
            Err(error) => {
                warn!("tmux 整屏重绘失败 ({session_id}, client {client_name}): {error}");
            }
        }
    }
}

/// Shared by deletion, recreation and failed/cancelled startup cleanup.
pub(super) fn destroy_tmux_session(session_id: &str) -> Result<()> {
    let (server_pid, roots) = tmux_pane_roots(session_id)?;
    // tmux automatically SIGCONTs stopped panes. Pause only this isolated
    // server while capturing; RAII resumes it even if capture fails.
    #[cfg(target_os = "linux")]
    let mut server = server_pid
        .map(process_tree::Process::pause)
        .transpose()?
        .flatten();
    #[cfg(target_os = "linux")]
    let mut tree = {
        anyhow::ensure!(
            roots.is_empty() || server.is_some(),
            "采集前 tmux server 已退出，请重新确认会话状态"
        );
        process_tree::ProcessTree::capture(&roots)?
    };
    #[cfg(target_os = "linux")]
    if let Some(server) = &mut server {
        server.resume()?;
    }
    #[cfg(not(target_os = "linux"))]
    anyhow::ensure!(roots.is_empty(), "完整终端进程回收需要 Linux pidfd 支持");

    // Capture identity before tmux can orphan descendants. Even if tmux fails,
    // finish releasing the captured processes and report both errors.
    let tmux_result = kill_tmux_session(session_id);
    #[cfg(target_os = "linux")]
    let process_result = tree.terminate();
    #[cfg(not(target_os = "linux"))]
    let process_result: Result<Vec<u32>> = Ok(Vec::new());
    match (tmux_result, process_result) {
        (Ok(()), Ok(pending)) => {
            if !pending.is_empty() {
                warn!(
                    session_id,
                    pending_pids = ?pending,
                    "terminal processes remain in uninterruptible IO after SIGKILL; kernel cleanup is pending"
                );
            }
            Ok(())
        }
        (Err(a), Err(b)) => anyhow::bail!("{a:#}; 进程回收失败: {b:#}"),
        (Err(error), Ok(_)) | (Ok(()), Err(error)) => Err(error),
    }
}

fn kill_tmux_session(session_id: &str) -> Result<()> {
    let mut command = isolated_tmux_command(session_id);
    sanitize_child_command(&mut command);
    command.args(["kill-session", "-t", &tmux_session_name(session_id)]);
    let output = command_output::bounded_command_output(
        command,
        TMUX_COMMAND_TIMEOUT,
        TMUX_SMALL_OUTPUT_LIMIT,
    )
    .context("结束 tmux 终端会话失败")?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() || tmux_missing_session_error(&stderr) {
        return Ok(());
    }
    anyhow::bail!("结束 tmux 终端会话失败 ({}): {stderr}", output.status)
}

#[cfg(not(windows))]
pub(super) fn terminate_tmux_pane_process_tree(
    session_id: &str,
    _user_profile: &runtime_paths::UserProfile,
) -> Result<()> {
    destroy_tmux_session(session_id)
}

#[cfg(windows)]
pub(super) fn terminate_tmux_pane_process_tree(
    _session_id: &str,
    _user_profile: &runtime_paths::UserProfile,
) -> Result<()> {
    Ok(())
}

#[cfg(target_os = "linux")]
#[path = "process_tree.rs"]
mod process_tree;

pub(super) fn send_tmux_startup_script(session_id: &str, script: &str) -> Result<()> {
    let script = normalize_tmux_startup_script(script);

    for line in script.lines().filter(|line| !line.trim().is_empty()) {
        let command_text = line.strip_suffix(';').unwrap_or(line);
        send_tmux_literal_keys(session_id, command_text)?;
        send_tmux_keys(session_id, &["\\;", "C-m"])?;
    }

    Ok(())
}

pub(super) fn send_tmux_input(session_id: &str, data: &str) -> Result<()> {
    let (text, submit_count) = split_tmux_input_submit_keys(data);
    if !text.is_empty() {
        send_tmux_literal_keys(session_id, text)?;
    }
    for _ in 0..submit_count {
        send_tmux_keys(session_id, &["C-m"])?;
    }
    Ok(())
}

/// Send the terminal's Enter key through tmux key handling rather than as a
/// raw PTY byte. This mirrors the soft-keyboard Enter action and avoids a
/// connected agent TUI treating a raw carriage return as inserted text.
pub(super) fn send_tmux_enter_key(session_id: &str) -> Result<()> {
    send_tmux_keys(session_id, &["C-m"])
}

pub(super) fn split_tmux_input_submit_keys(data: &str) -> (&str, usize) {
    let mut text_end = data.len();
    let mut submit_count = 0;

    while text_end > 0 {
        let Some((index, ch)) = data[..text_end].char_indices().next_back() else {
            break;
        };
        if ch != '\r' && ch != '\n' {
            break;
        }
        text_end = index;
        submit_count += 1;
    }

    (&data[..text_end], submit_count)
}

fn send_tmux_literal_keys(session_id: &str, text: &str) -> Result<()> {
    let mut command = isolated_tmux_command(session_id);
    sanitize_child_command(&mut command);
    command
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(session_id))
        .arg("-l")
        .arg(text);
    let output = command_output::bounded_command_output(
        command,
        TMUX_COMMAND_TIMEOUT,
        TMUX_SMALL_OUTPUT_LIMIT,
    )
    .context("无法发送终端启动脚本")?;

    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    anyhow::bail!(
        "无法发送终端启动脚本: {}",
        if stderr.is_empty() {
            "tmux returned a non-zero status".to_string()
        } else {
            stderr
        }
    );
}

fn send_tmux_keys(session_id: &str, keys: &[&str]) -> Result<()> {
    let mut command = isolated_tmux_command(session_id);
    sanitize_child_command(&mut command);
    command
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(session_id))
        .args(keys);
    let output = command_output::bounded_command_output(
        command,
        TMUX_COMMAND_TIMEOUT,
        TMUX_SMALL_OUTPUT_LIMIT,
    )
    .context("无法执行终端启动脚本")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    anyhow::bail!(
        "无法执行终端启动脚本: {}",
        if stderr.is_empty() {
            "tmux returned a non-zero status".to_string()
        } else {
            stderr
        }
    );
}

pub(super) fn normalize_tmux_startup_script(script: &str) -> String {
    script
        .trim_end()
        .lines()
        .map(|line| {
            let trimmed = line.trim_end();
            if trimmed.is_empty() || trimmed.ends_with(';') {
                trimmed.to_string()
            } else {
                format!("{trimmed};")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn ensure_tmux_session(
    session_id: &str,
    path: &Path,
    user_profile: &runtime_paths::UserProfile,
) -> Result<()> {
    if tmux_session_exists(session_id) {
        configure_tmux_server(session_id)?;
        configure_tmux_session(session_id)?;
        return Ok(());
    }

    // Under systemd, place the detached tmux server in its own scope so a
    // webclx service restart does not kill the shell it manages.
    let mut scope_failure = None;
    if should_isolate_tmux_scope() {
        match create_tmux_session_in_scope(session_id, path, user_profile) {
            Ok(()) => {
                configure_tmux_server(session_id)?;
                configure_tmux_session(session_id)?;
                return Ok(());
            }
            Err(error) => {
                // A broken host device affects scope and direct launches alike.
                // Preserve scope availability for launches after the host is repaired.
                if error.is::<NullDeviceStartupError>() {
                    return Err(error);
                }
                let message = error.to_string();
                if should_disable_tmux_scope_isolation(&message) {
                    TMUX_SCOPE_ISOLATION_AVAILABLE.store(false, Ordering::Relaxed);
                }
                warn!(
                    "tmux scope creation failed for {}: {}; falling back to direct tmux launch",
                    session_id, message
                );
                scope_failure = Some(message);
            }
        }
    }

    create_tmux_session_with_retry(session_id, path, user_profile).map_err(|error| {
        if let Some(scope_failure) = scope_failure {
            anyhow::anyhow!("{}；回退到直接创建 tmux 终端会话时也失败: {}", scope_failure, error)
        } else {
            error
        }
    })?;

    configure_tmux_server(session_id)?;
    configure_tmux_session(session_id)?;
    Ok(())
}

fn create_tmux_session_with_retry(
    session_id: &str,
    path: &Path,
    user_profile: &runtime_paths::UserProfile,
) -> Result<()> {
    let mut last_error = None;
    for attempt in 0..5 {
        match create_tmux_session(session_id, path, user_profile) {
            Ok(()) => return Ok(()),
            Err(error) => {
                let retryable = !error.is::<NullDeviceStartupError>();
                let message = error.to_string();
                last_error = Some(error);
                if !retryable || attempt == 4 || !message.contains("server exited unexpectedly") {
                    break;
                }
                let mut command = isolated_tmux_command(session_id);
                sanitize_child_command(&mut command);
                command.arg("kill-server");
                let _ = command_output::bounded_command_output(
                    command,
                    TMUX_COMMAND_TIMEOUT,
                    TMUX_SMALL_OUTPUT_LIMIT,
                );
                remove_stale_tmux_lock(session_id);
                let delay = 100 * (attempt as u64 + 1);
                std::thread::sleep(std::time::Duration::from_millis(delay));
                warn!("tmux 创建失败，将重试 ({}/4): {}", attempt + 1, message);
            }
        }
    }
    Err(last_error.expect("tmux 创建至少尝试一次"))
}

fn remove_stale_tmux_lock(session_id: &str) {
    let lock_name = format!("{}.lock", tmux_socket_name(session_id));
    let Ok(entries) = std::fs::read_dir("/tmp/tmux-0") else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.file_name().and_then(|name| name.to_str()) != Some(&lock_name) {
            continue;
        }
        if let Ok(metadata) = std::fs::metadata(&path) {
            if metadata
                .modified()
                .ok()
                .and_then(|time| time.elapsed().ok())
                .is_some_and(|age| age > std::time::Duration::from_secs(2))
            {
                if std::fs::remove_file(&path).is_ok() {
                    warn!("removed stale tmux lock {}", path.display());
                }
            }
        }
    }
}

fn create_tmux_session(
    session_id: &str,
    path: &Path,
    user_profile: &runtime_paths::UserProfile,
) -> Result<()> {
    let command = isolated_tmux_command(session_id);
    run_tmux_new_session(command, session_id, path, "无法创建 tmux 终端会话", user_profile)
}

fn create_tmux_session_in_scope(
    session_id: &str,
    path: &Path,
    user_profile: &runtime_paths::UserProfile,
) -> Result<()> {
    let mut command = Command::new("systemd-run");
    command
        .arg("--scope")
        .arg("--quiet")
        // Keep tmux later than webclx.service in the shutdown transaction. A
        // plain transient scope has no ordering relative to the service, so
        // systemd may stop it first during poweroff/reboot; webClx would then
        // find every backend unreachable and could not capture resume records.
        .arg("--property=After=webclx.service")
        .arg("--unit")
        .arg(tmux_scope_unit_name(session_id))
        .arg("tmux")
        .arg("-L")
        .arg(tmux_socket_name(session_id));
    run_tmux_new_session(
        command,
        session_id,
        path,
        "无法通过 systemd scope 创建 tmux 终端会话",
        user_profile,
    )
}

fn run_tmux_new_session(
    mut command: Command,
    session_id: &str,
    path: &Path,
    context: &'static str,
    user_profile: &runtime_paths::UserProfile,
) -> Result<()> {
    sanitize_child_command(&mut command);
    let shell = &user_profile.shell;
    let build_tmux_args = |cmd: &mut Command| {
        cmd.arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg(tmux_session_name(session_id))
            .arg("-c")
            .arg(path);
    };

    // Always enter through the user's login shell. This preserves the user's
    // own environment/configuration while preventing webClx's service
    // environment from being copied into the PTY.
    let runuser_needed = true;
    if runuser_needed {
        build_tmux_args(&mut command);
        append_runuser_login_shell(&mut command, user_profile);
    } else {
        build_tmux_args(&mut command);
        command.arg(shell).arg("-l");
    }
    let output = command_output::bounded_command_output(
        command,
        TMUX_COMMAND_TIMEOUT,
        TMUX_SMALL_OUTPUT_LIMIT,
    )
    .with_context(|| context)?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        // runuser not found on this system → retry with su
        if runuser_needed && looks_like_runuser_missing(&stderr) {
            let mut cmd = isolated_tmux_command(session_id);
            sanitize_child_command(&mut cmd);
            build_tmux_args(&mut cmd);
            let shell_path = shell.to_string_lossy();
            let su_cmd = format!("{} -l", shell_path);
            cmd.arg("su").arg(&user_profile.name).arg("-c").arg(&su_cmd);
            let retry = command_output::bounded_command_output(
                cmd,
                TMUX_COMMAND_TIMEOUT,
                TMUX_SMALL_OUTPUT_LIMIT,
            )
            .with_context(|| context)?;
            if !retry.status.success() {
                let retry_stderr = String::from_utf8_lossy(&retry.stderr).trim().to_string();
                return Err(tmux_creation_error(&format!("{context} (via su)"), &retry_stderr));
            }
            return Ok(());
        }

        return Err(tmux_creation_error(context, &stderr));
    }

    Ok(())
}

#[derive(Debug)]
struct NullDeviceStartupError(String);

impl std::fmt::Display for NullDeviceStartupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NullDeviceStartupError {}

fn tmux_creation_error(context: &str, stderr: &str) -> anyhow::Error {
    tmux_creation_error_with_diagnostic(context, stderr, null_device_diagnostic())
}

fn tmux_creation_error_with_diagnostic(
    context: &str,
    stderr: &str,
    diagnostic: Option<String>,
) -> anyhow::Error {
    let detail = if stderr.is_empty() {
        "tmux returned a non-zero status"
    } else {
        stderr
    };
    let original = format!("{context}: {detail}");
    if let Some(diagnostic) = diagnostic {
        let message = format!(
            "{original}；/dev/null 检查异常：{diagnostic}；应为字符设备 1:3、权限 0666。请管理员修复系统设备节点后重试；已停止本次自动重试，未修改设备。"
        );
        warn!("{message}");
        NullDeviceStartupError(message).into()
    } else {
        anyhow::anyhow!(original)
    }
}

fn null_device_diagnostic() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::MetadataExt;
        return match std::fs::metadata("/dev/null") {
            Ok(metadata) => null_device_metadata_problem(
                metadata.mode(),
                libc::major(metadata.rdev()),
                libc::minor(metadata.rdev()),
            ),
            Err(error) => Some(format!("读取设备状态失败：{error}")),
        };
    }
    // Device numbers are platform-specific; do not apply Linux's 1:3 elsewhere.
    #[cfg(not(target_os = "linux"))]
    None
}

#[cfg(target_os = "linux")]
fn null_device_metadata_problem(mode: u32, major: u32, minor: u32) -> Option<String> {
    if mode & libc::S_IFMT != libc::S_IFCHR {
        let kind = match mode & libc::S_IFMT {
            libc::S_IFREG => "普通文件",
            libc::S_IFDIR => "目录",
            _ => "非字符设备",
        };
        Some(format!("实际类型为{kind}，mode={mode:06o}"))
    } else if (major, minor) != (1, 3) {
        Some(format!("实际字符设备号为 {major}:{minor}"))
    } else if mode & 0o7777 != 0o666 {
        Some(format!("实际权限为 {:04o}", mode & 0o7777))
    } else {
        None
    }
}

fn append_runuser_login_shell(command: &mut Command, user_profile: &runtime_paths::UserProfile) {
    command
        .arg("runuser")
        .arg("-u")
        .arg(&user_profile.name)
        .arg("--")
        .arg(&user_profile.shell)
        .arg("-l");
}

fn looks_like_runuser_missing(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("runuser")
        && (lower.contains("not found")
            || lower.contains("no such file")
            || lower.contains("command not found"))
}

pub(super) fn create_fresh_tmux_session(
    session_id: &str,
    path: &Path,
    user_profile: &runtime_paths::UserProfile,
) -> Result<()> {
    destroy_tmux_session(session_id)?;

    ensure_tmux_session(session_id, path, user_profile)
}

fn configure_tmux_session(session_id: &str) -> Result<()> {
    set_tmux_session_option(
        session_id,
        "history-limit",
        TMUX_HISTORY_LIMIT,
        "无法配置 tmux 历史上限",
    )?;
    set_tmux_session_option(session_id, "status", "off", "无法关闭 tmux 状态栏")?;
    set_tmux_session_option(
        session_id,
        "terminal-overrides",
        TMUX_TERMINAL_OVERRIDES,
        "无法配置 tmux 终端能力覆盖",
    )?;
    set_tmux_session_option(session_id, "focus-events", "on", "无法开启 tmux focus-events")
}

fn set_tmux_session_option(
    session_id: &str,
    option: &str,
    value: &str,
    context: &'static str,
) -> Result<()> {
    let mut command = isolated_tmux_command(session_id);
    sanitize_child_command(&mut command);
    command
        .arg("set-option")
        .arg("-t")
        .arg(tmux_session_name(session_id))
        .arg(option)
        .arg(value);
    let output = command_output::bounded_command_output(
        command,
        TMUX_COMMAND_TIMEOUT,
        TMUX_SMALL_OUTPUT_LIMIT,
    )
    .context(context)?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    anyhow::bail!(
        "{}: {}",
        context,
        if stderr.is_empty() {
            "tmux returned a non-zero status".to_string()
        } else {
            stderr
        }
    );
}

fn configure_tmux_server(session_id: &str) -> Result<()> {
    let mut command = isolated_tmux_command(session_id);
    sanitize_child_command(&mut command);
    command
        .arg("set-option")
        .arg("-sg")
        .arg("escape-time")
        .arg("0");
    let output = command_output::bounded_command_output(
        command,
        TMUX_COMMAND_TIMEOUT,
        TMUX_SMALL_OUTPUT_LIMIT,
    )
    .context("无法配置 tmux escape-time")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    anyhow::bail!(
        "无法配置 tmux escape-time: {}",
        if stderr.is_empty() {
            "tmux returned a non-zero status".to_string()
        } else {
            stderr
        }
    );
}

#[cfg(test)]
mod isolation_tests {
    use super::*;

    #[test]
    fn null_device_error_preserves_original_and_is_non_retryable() {
        let error = tmux_creation_error_with_diagnostic(
            "create failed",
            "server exited unexpectedly",
            Some("实际类型为普通文件".to_string()),
        );
        assert!(error.is::<NullDeviceStartupError>());
        let message = error.to_string();
        assert!(message.contains("create failed: server exited unexpectedly"));
        assert!(message.contains("/dev/null"));
        assert!(message.contains("普通文件"));
        assert!(message.contains("1:3"));
        assert!(message.contains("0666"));
        assert!(message.contains("未修改设备"));
    }

    #[test]
    fn null_device_healthy_preserves_original_error_behavior() {
        let error = tmux_creation_error_with_diagnostic("create failed", "original", None);
        assert!(!error.is::<NullDeviceStartupError>());
        assert_eq!(error.to_string(), "create failed: original");
        assert_eq!(
            tmux_creation_error_with_diagnostic("create failed", "", None).to_string(),
            "create failed: tmux returned a non-zero status"
        );
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn null_device_metadata_checks_type_number_and_permissions_without_touching_host() {
        assert_eq!(null_device_metadata_problem(libc::S_IFCHR | 0o666, 1, 3), None);
        assert!(
            null_device_metadata_problem(libc::S_IFREG | 0o644, 0, 0)
                .unwrap()
                .contains("普通文件")
        );
        assert!(
            null_device_metadata_problem(libc::S_IFDIR | 0o755, 0, 0)
                .unwrap()
                .contains("目录")
        );
        assert!(
            null_device_metadata_problem(libc::S_IFCHR | 0o666, 1, 5)
                .unwrap()
                .contains("1:5")
        );
        assert!(
            null_device_metadata_problem(libc::S_IFCHR | 0o600, 1, 3)
                .unwrap()
                .contains("0600")
        );
    }

    #[test]
    fn tmux_socket_name_is_unique_per_terminal_session() {
        assert_eq!(tmux_socket_name("s41"), "webclx_s41");
        assert_ne!(tmux_socket_name("s41"), tmux_socket_name("s42"));
    }

    #[test]
    fn isolated_tmux_command_targets_the_session_socket() {
        let command = isolated_tmux_command("s41");
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(args, vec!["-L", "webclx_s41"]);
    }

    #[test]
    fn runuser_launch_uses_shell_login_mode_without_conflicting_login_option() {
        let user_profile = runtime_paths::resolve_current_user_profile()
            .expect("current user profile should resolve");
        let mut command = Command::new("tmux");
        append_runuser_login_shell(&mut command, &user_profile);
        let args = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        assert_eq!(
            args,
            vec![
                "runuser".to_string(),
                "-u".to_string(),
                user_profile.name,
                "--".to_string(),
                user_profile.shell.display().to_string(),
                "-l".to_string(),
            ]
        );
        assert!(!args.iter().any(|arg| arg == "--login"));
    }
}

#[cfg(all(test, unix))]
mod initial_snapshot_async_tests {
    use super::*;
    use std::time::Duration;
    use tokio::sync::Semaphore;

    // Product output() owns a kill-on-drop Child RAII guard. Every awaited
    // process operation also has a test-local deadline, including assertions.
    #[tokio::test]
    async fn snapshot_drains_both_pipes_and_reports_nonzero_exit() {
        let slots = Semaphore::new(1);
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf snapshot; printf failure >&2; exit 7"]);
        let output = tokio::time::timeout(
            Duration::from_secs(2),
            bounded_snapshot_output(command, &slots, Duration::from_secs(1)),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(output.stdout, b"snapshot");
        assert_eq!(output.stderr, b"failure");
        assert_eq!(output.status.code(), Some(7));
        assert!(
            decode_tmux_snapshot(output)
                .unwrap_err()
                .to_string()
                .contains("failure")
        );
        assert_eq!(slots.available_permits(), 1);
    }

    #[tokio::test]
    async fn snapshot_deadline_cancels_child_without_blocking_executor() {
        let slots = Semaphore::new(1);
        let mut command = Command::new("/bin/sleep");
        command.arg("5");
        let error = tokio::time::timeout(
            Duration::from_secs(1),
            bounded_snapshot_output(command, &slots, Duration::from_millis(40)),
        )
        .await
        .expect("executor must remain responsive")
        .unwrap_err();
        assert!(error.to_string().contains("执行超时"));
        assert_eq!(slots.available_permits(), 1);
    }

    #[tokio::test]
    async fn snapshot_queue_deadline_expires_before_spawning() {
        let slots = Semaphore::new(0);
        // A spawn would fail with ENOENT, rather than the expected queue timeout.
        let command = Command::new("/nonexistent-webclx-snapshot-test");
        let error = tokio::time::timeout(
            Duration::from_secs(1),
            bounded_snapshot_output(command, &slots, Duration::from_millis(20)),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert!(error.to_string().contains("并发等待超时"));
    }
}
