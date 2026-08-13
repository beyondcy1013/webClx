//! crontab 操作原语：读写系统 crontab、清理过期 marker、解析 due 元数据。
//!
//! 这些函数不依赖 `TerminalManager` 状态，是纯 OS 层工具，方便单测和复用。

use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, Result};

/// 读取当前用户的 crontab 内容；不存在时返回空串。
pub(super) fn current_crontab() -> Result<String> {
    let output = Command::new("crontab")
        .arg("-l")
        .output()
        .context("read crontab")?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("no crontab") {
        return Ok(String::new());
    }
    anyhow::bail!("crontab -l failed: {}", stderr.trim())
}

/// 把内容写回用户 crontab（整体替换）。
pub(super) fn install_crontab(content: &str) -> Result<()> {
    let mut child = Command::new("crontab")
        .arg("-")
        .stdin(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn crontab installer")?;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(content.as_bytes())
            .context("write crontab content")?;
    }
    let output = child.wait_with_output().context("wait crontab installer")?;
    if output.status.success() {
        Ok(())
    } else {
        anyhow::bail!("crontab install failed: {}", String::from_utf8_lossy(&output.stderr).trim())
    }
}

/// 把任意字符串规整成只含 `[A-Za-z0-9_-]` 的安全 cron 文件名片段。
pub(super) fn sanitize_cron_file_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || *character == '-' || *character == '_'
        })
        .collect();
    if sanitized.is_empty() {
        "terminal".to_string()
    } else {
        sanitized
    }
}

/// 单引号转义，供 cron 脚本里安全引用路径/参数。
pub(super) fn shell_quote_cron(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// 把给定路径的权限设为 0700（仅 Unix 有意义）。
#[cfg(unix)]
pub(super) fn set_executable_mode(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(metadata) = fs::metadata(path) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o700);
        let _ = fs::set_permissions(path, permissions);
    }
}

#[cfg(not(unix))]
pub(super) fn set_executable_mode(_path: &Path) {}

/// 从 crontab 中删除所有匹配 `markers`（及对应的 due 注释行）的条目，并整体重写。
pub(super) fn rewrite_crontab_without_markers(markers: &[String]) -> Result<()> {
    let current = current_crontab()?;
    let due_prefixes: Vec<String> = markers
        .iter()
        .filter_map(|marker| {
            // marker looks like webclx-auto-continue:{session}:{signature}
            let body = marker.strip_prefix("webclx-auto-continue:")?;
            Some(format!("# webclx-auto-continue-due:{body}:"))
        })
        .collect();
    let next_lines: Vec<String> = current
        .lines()
        .filter(|line| {
            if markers.iter().any(|marker| line.contains(marker)) {
                return false;
            }
            let trimmed = line.trim();
            if due_prefixes
                .iter()
                .any(|prefix| trimmed.starts_with(prefix.as_str()))
            {
                return false;
            }
            true
        })
        .map(ToString::to_string)
        .collect();
    let next = format!("{}\n", next_lines.join("\n"));
    install_crontab(&next)
}

/// 解析 `webclx-auto-continue-due:{session}:{signature}:{epoch}` 元数据注释，
/// 返回以 (session_id, signature) 为键、触发 epoch 秒为值的查找表。
pub(in crate::terminal) fn parse_auto_continue_due_epochs(
    crontab: &str,
) -> HashMap<(String, String), i64> {
    const PREFIX: &str = "# webclx-auto-continue-due:";
    let mut map = HashMap::new();
    for line in crontab.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix(PREFIX) else {
            continue;
        };
        let parts: Vec<&str> = rest.splitn(3, ':').collect();
        if parts.len() != 3 {
            continue;
        }
        let session_id = parts[0].trim();
        let signature = parts[1].trim();
        if let Ok(epoch) = parts[2].trim().parse::<i64>() {
            map.insert((session_id.to_string(), signature.to_string()), epoch);
        }
    }
    map
}
