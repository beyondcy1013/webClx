use std::{
    collections::HashSet,
    path::Path,
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
};

use anyhow::{Context, Result};
use terminal_core::*;
use tracing::warn;

use crate::{runtime_paths, shell_env};

use super::{
    CHILD_PROCESS_ENV_KEYS_TO_CLEAR, TERMINAL_SESSION_ENV_KEYS_TO_CLEAR, sanitize_child_command,
};

static TMUX_SCOPE_ISOLATION_AVAILABLE: AtomicBool = AtomicBool::new(true);
const TMUX_HISTORY_LIMIT: &str = "100000";
const TMUX_TERMINAL_OVERRIDES: &str = "xterm-256color:indn@:rin@";
const INITIAL_TMUX_SNAPSHOT_LINE_LIMIT: u32 = 800;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TmuxSessionStatus {
    Exists,
    Missing,
    Unknown,
}

fn should_isolate_tmux_scope() -> bool {
    TMUX_SCOPE_ISOLATION_AVAILABLE.load(Ordering::Relaxed)
        && std::env::var_os("INVOCATION_ID").is_some()
}

pub(super) fn tmux_session_status(session_id: &str) -> TmuxSessionStatus {
    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    match command
        .arg("has-session")
        .arg("-t")
        .arg(tmux_session_name(session_id))
        .output()
    {
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

pub(super) fn detach_tmux_clients_for_sessions(session_ids: &[String]) -> Result<()> {
    if session_ids.is_empty() {
        return Ok(());
    }

    let target_sessions = session_ids
        .iter()
        .map(|session_id| tmux_session_name(session_id))
        .collect::<HashSet<_>>();
    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    let output = command
        .arg("list-clients")
        .arg("-F")
        .arg("#{client_name}\t#{session_name}")
        .output()
        .context("无法列出 tmux 终端客户端")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.contains("no clients") || tmux_missing_session_error(&stderr) {
            return Ok(());
        }
        anyhow::bail!("无法列出 tmux 终端客户端: {stderr}");
    }

    let client_names = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (client_name, session_name) = line.split_once('\t')?;
            target_sessions
                .contains(session_name)
                .then_some(client_name.to_string())
        })
        .collect::<Vec<_>>();
    if client_names.is_empty() {
        return Ok(());
    }

    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    command.arg("detach-client");
    for (index, client_name) in client_names.iter().enumerate() {
        if index > 0 {
            command.arg(";").arg("detach-client");
        }
        command.arg("-t").arg(client_name);
    }
    let output = command.output().context("无法批量断开 tmux 终端客户端")?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.contains("no such client") || tmux_missing_session_error(&stderr) {
        return Ok(());
    }
    anyhow::bail!("无法批量断开 tmux 终端客户端: {stderr}");
}

#[cfg(test)]
pub(super) fn tmux_client_names(output: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(output)
        .lines()
        .map(str::trim)
        .filter(|client_name| !client_name.is_empty())
        .map(str::to_string)
        .collect()
}

pub(super) fn capture_tmux_initial_pane_snapshot(session_id: &str) -> Result<Vec<u8>> {
    capture_tmux_pane_snapshot_from(
        session_id,
        &format!("-{INITIAL_TMUX_SNAPSHOT_LINE_LIMIT}"),
        true,
    )
}

pub(super) fn capture_tmux_recent_pane_snapshot(session_id: &str) -> Result<Vec<u8>> {
    capture_tmux_joined_pane_snapshot_from(session_id, "-200", false)
}

pub(super) fn capture_tmux_activity_pane_snapshot(
    session_id: &str,
    line_limit: u32,
) -> Result<Vec<u8>> {
    capture_tmux_joined_pane_snapshot_from(session_id, &format!("-{}", line_limit.max(200)), false)
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
    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    command.arg("capture-pane");
    if include_escape_sequences {
        command.arg("-e");
    }
    if join_wrapped_lines {
        command.arg("-J");
    }
    let output = command
        .arg("-p")
        .arg("-S")
        .arg(start_line)
        .arg("-t")
        .arg(tmux_session_name(session_id))
        .output()
        .context("无法读取 tmux 终端历史")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "无法读取 tmux 终端历史: {}",
            if stderr.is_empty() {
                "tmux returned a non-zero status".to_string()
            } else {
                stderr
            }
        );
    }

    Ok(normalize_tmux_snapshot(&output.stdout))
}

pub(super) fn kill_tmux_session(session_id: &str) -> Result<()> {
    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    let output = command
        .arg("kill-session")
        .arg("-t")
        .arg(tmux_session_name(session_id))
        .output()
        .context("无法结束 tmux 终端会话")?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.contains("can't find session") {
        return Ok(());
    }

    anyhow::bail!(
        "无法结束 tmux 终端会话: {}",
        if stderr.is_empty() {
            "tmux returned a non-zero status".to_string()
        } else {
            stderr
        }
    );
}

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
    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    let output = command
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(session_id))
        .arg("-l")
        .arg(text)
        .output()
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
    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    let output = command
        .arg("send-keys")
        .arg("-t")
        .arg(tmux_session_name(session_id))
        .args(keys)
        .output()
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
    terminal_default_env: Vec<(String, String)>,
    proxy_env: Vec<(String, String)>,
) -> Result<()> {
    let child_env = resolve_tmux_child_env(user_profile, &terminal_default_env, &proxy_env);
    if tmux_session_exists(session_id) {
        configure_tmux_server()?;
        configure_tmux_session(session_id)?;
        if !child_env.is_empty() {
            set_tmux_session_env(session_id, &child_env)?;
        }
        return Ok(());
    }

    // Under systemd, place the detached tmux server in its own scope so a
    // webclx service restart does not kill the shell it manages.
    let mut scope_failure = None;
    if should_isolate_tmux_scope() {
        match create_tmux_session_in_scope(
            session_id,
            path,
            user_profile,
            &terminal_default_env,
            &proxy_env,
        ) {
            Ok(()) => {
                configure_tmux_server()?;
                configure_tmux_session(session_id)?;
                return Ok(());
            }
            Err(error) => {
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

    create_tmux_session(session_id, path, user_profile, &terminal_default_env, &proxy_env)
        .map_err(|error| {
            if let Some(scope_failure) = scope_failure {
                anyhow::anyhow!(
                    "{}；回退到直接创建 tmux 终端会话时也失败: {}",
                    scope_failure,
                    error
                )
            } else {
                error
            }
        })?;

    configure_tmux_server()?;
    configure_tmux_session(session_id)?;
    Ok(())
}

fn create_tmux_session(
    session_id: &str,
    path: &Path,
    user_profile: &runtime_paths::UserProfile,
    terminal_default_env: &[(String, String)],
    proxy_env: &[(String, String)],
) -> Result<()> {
    let command = Command::new("tmux");
    run_tmux_new_session(
        command,
        session_id,
        path,
        "无法创建 tmux 终端会话",
        user_profile,
        terminal_default_env,
        proxy_env,
    )
}

fn create_tmux_session_in_scope(
    session_id: &str,
    path: &Path,
    user_profile: &runtime_paths::UserProfile,
    terminal_default_env: &[(String, String)],
    proxy_env: &[(String, String)],
) -> Result<()> {
    let mut command = Command::new("systemd-run");
    command
        .arg("--scope")
        .arg("--quiet")
        .arg("--unit")
        .arg(tmux_scope_unit_name(session_id))
        .arg("tmux");
    run_tmux_new_session(
        command,
        session_id,
        path,
        "无法通过 systemd scope 创建 tmux 终端会话",
        user_profile,
        terminal_default_env,
        proxy_env,
    )
}

fn run_tmux_new_session(
    mut command: Command,
    session_id: &str,
    path: &Path,
    context: &'static str,
    user_profile: &runtime_paths::UserProfile,
    terminal_default_env: &[(String, String)],
    proxy_env: &[(String, String)],
) -> Result<()> {
    sanitize_child_command(&mut command);
    let shell = &user_profile.shell;
    let child_env = resolve_tmux_child_env(user_profile, terminal_default_env, proxy_env);
    let launch_env = build_tmux_launch_env(user_profile, terminal_default_env, proxy_env);
    let build_tmux_args = |cmd: &mut Command| {
        cmd.arg("new-session")
            .arg("-d")
            .arg("-s")
            .arg(tmux_session_name(session_id))
            .arg("-c")
            .arg(path)
            .arg("env")
            .args(
                CHILD_PROCESS_ENV_KEYS_TO_CLEAR
                    .iter()
                    .flat_map(|key| ["-u", *key]),
            )
            .args(
                launch_env
                    .iter()
                    .map(|(key, value)| format!("{key}={value}")),
            );
    };

    let runuser_needed = should_launch_shell_via_runuser(user_profile);
    let output = if runuser_needed {
        let mut cmd = Command::new(command.get_program());
        build_tmux_args(&mut cmd);
        cmd.arg("runuser")
            .arg("-u")
            .arg(&user_profile.name)
            .arg("--preserve-environment")
            .arg("--")
            .arg(shell)
            .arg("-l");
        cmd.output().with_context(|| context)?
    } else {
        build_tmux_args(&mut command);
        command.arg(shell).arg("-l");
        command.output().with_context(|| context)?
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        // runuser not found on this system → retry with su
        if runuser_needed && looks_like_runuser_missing(&stderr) {
            let mut cmd = Command::new("tmux");
            sanitize_child_command(&mut cmd);
            build_tmux_args(&mut cmd);
            let shell_path = shell.to_string_lossy();
            let su_cmd = format!("{} -l", shell_path);
            cmd.arg("su").arg(&user_profile.name).arg("-c").arg(&su_cmd);
            let retry = cmd.output().with_context(|| context)?;
            if !retry.status.success() {
                let retry_stderr = String::from_utf8_lossy(&retry.stderr).trim().to_string();
                anyhow::bail!(
                    "{} (via su): {}",
                    context,
                    if retry_stderr.is_empty() {
                        "tmux returned a non-zero status".to_string()
                    } else {
                        retry_stderr
                    }
                );
            }
            set_tmux_session_env(session_id, &child_env)?;
            return Ok(());
        }

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

    set_tmux_session_env(session_id, &child_env)?;

    Ok(())
}

fn looks_like_runuser_missing(stderr: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("runuser")
        && (lower.contains("not found")
            || lower.contains("no such file")
            || lower.contains("command not found"))
}

pub(super) fn build_tmux_launch_env(
    user_profile: &runtime_paths::UserProfile,
    terminal_default_env: &[(String, String)],
    proxy_env: &[(String, String)],
) -> Vec<(String, String)> {
    terminal_core::build_tmux_launch_env(
        &user_profile.home.display().to_string(),
        &user_profile.shell.display().to_string(),
        &user_profile.name,
        terminal_default_env,
        proxy_env,
    )
}

fn should_launch_shell_via_runuser(user_profile: &runtime_paths::UserProfile) -> bool {
    runtime_paths::resolve_current_user_profile()
        .map(|current| current.uid != user_profile.uid)
        .unwrap_or(true)
}

pub(super) fn resolve_tmux_child_env(
    user_profile: &runtime_paths::UserProfile,
    terminal_default_env: &[(String, String)],
    proxy_env: &[(String, String)],
) -> Vec<(String, String)> {
    let shell_entries = match shell_env::read_user_shell_env(user_profile) {
        Ok(snapshot) => snapshot.entries,
        Err(error) => {
            warn!("read current user shell env for tmux failed: {error}");
            Vec::new()
        }
    };

    terminal_core::build_tmux_child_env(&shell_entries, terminal_default_env, proxy_env)
}

fn set_tmux_session_env(session_id: &str, env: &[(String, String)]) -> Result<()> {
    let tmux_name = tmux_session_name(session_id);
    for key in TERMINAL_SESSION_ENV_KEYS_TO_CLEAR {
        let mut command = Command::new("tmux");
        sanitize_child_command(&mut command);
        let output = command
            .arg("set-environment")
            .arg("-r")
            .arg("-t")
            .arg(&tmux_name)
            .arg(key)
            .output()
            .with_context(|| format!("无法清理 tmux 环境变量 {key}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("tmux clear-environment {} failed: {}", key, stderr.trim());
        }
    }

    for (key, value) in env {
        let mut command = Command::new("tmux");
        sanitize_child_command(&mut command);
        let output = command
            .arg("set-environment")
            .arg("-t")
            .arg(&tmux_name)
            .arg(key)
            .arg(value)
            .output()
            .with_context(|| format!("无法设置 tmux 环境变量 {key}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("tmux set-environment {} failed: {}", key, stderr.trim());
        }
    }
    Ok(())
}

pub(super) fn create_fresh_tmux_session(
    session_id: &str,
    path: &Path,
    user_profile: &runtime_paths::UserProfile,
    terminal_default_env: Vec<(String, String)>,
    proxy_env: Vec<(String, String)>,
) -> Result<()> {
    if tmux_session_exists(session_id) {
        kill_tmux_session(session_id)?;
    }

    ensure_tmux_session(session_id, path, user_profile, terminal_default_env, proxy_env)
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
    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    let output = command
        .arg("set-option")
        .arg("-t")
        .arg(tmux_session_name(session_id))
        .arg(option)
        .arg(value)
        .output()
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

fn configure_tmux_server() -> Result<()> {
    let mut command = Command::new("tmux");
    sanitize_child_command(&mut command);
    let output = command
        .arg("set-option")
        .arg("-sg")
        .arg("escape-time")
        .arg("0")
        .output()
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
