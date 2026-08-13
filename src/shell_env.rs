use std::{
    collections::HashSet,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow};

#[cfg(not(windows))]
const DEFAULT_PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
#[cfg(windows)]
const DEFAULT_PATH: &str =
    "C:\\Windows\\System32;C:\\Windows;C:\\Windows\\System32\\WindowsPowerShell\\v1.0";
const ENV_OUTPUT_SENTINEL: &str = "__WEBCLX_ENV_BEGIN__";

#[derive(Debug, Clone)]
pub struct ShellEnvironmentSnapshot {
    pub init_file_path: PathBuf,
    pub entries: Vec<(String, String)>,
}

pub fn read_current_user_shell_env() -> Result<ShellEnvironmentSnapshot> {
    let user_profile = crate::runtime_paths::resolve_current_user_profile()
        .ok_or_else(|| anyhow!("无法确定当前运行用户。"))?;
    read_user_shell_env(&user_profile)
}

pub fn read_user_shell_env(
    user_profile: &crate::runtime_paths::UserProfile,
) -> Result<ShellEnvironmentSnapshot> {
    let init_file_path = shell_init_file_path(&user_profile.shell, &user_profile.home);
    let entries = read_shell_env(
        &user_profile.shell,
        &user_profile.home,
        &init_file_path,
        &user_profile.name,
        should_read_shell_via_runuser(user_profile),
    )?;

    Ok(ShellEnvironmentSnapshot {
        init_file_path,
        entries,
    })
}

pub fn user_shell_init_file_path(user_profile: &crate::runtime_paths::UserProfile) -> PathBuf {
    shell_init_file_path(&user_profile.shell, &user_profile.home)
}

pub fn filter_env_entries(
    entries: &[(String, String)],
    allowed_keys: &[&str],
) -> Vec<(String, String)> {
    let allowed = allowed_keys.iter().copied().collect::<HashSet<_>>();
    entries
        .iter()
        .filter(|(key, _)| allowed.contains(key.as_str()))
        .cloned()
        .collect()
}

pub fn merge_inherited_env_entries(
    entries: &[(String, String)],
    allowed_keys: &[&str],
) -> Vec<(String, String)> {
    let inherited = allowed_keys
        .iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| ((*key).to_string(), value))
        })
        .collect::<Vec<_>>();
    merge_allowed_env_entries(&inherited, entries, allowed_keys)
}

fn merge_allowed_env_entries(
    inherited: &[(String, String)],
    entries: &[(String, String)],
    allowed_keys: &[&str],
) -> Vec<(String, String)> {
    let mut merged = filter_env_entries(inherited, allowed_keys);
    for (key, value) in filter_env_entries(entries, allowed_keys) {
        if let Some(existing) = merged
            .iter_mut()
            .find(|(existing_key, _)| existing_key == &key)
        {
            existing.1 = value;
        } else {
            merged.push((key, value));
        }
    }
    merged
}

fn shell_init_file_path(shell_path: &Path, home_dir: &Path) -> PathBuf {
    match shell_basename(shell_path).as_deref() {
        Some("bash") => home_dir.join(".bashrc"),
        Some("zsh") => home_dir.join(".zshrc"),
        Some("fish") => home_dir.join(".config/fish/config.fish"),
        Some("ksh") => home_dir.join(".kshrc"),
        _ => home_dir.join(".profile"),
    }
}

fn shell_basename(shell_path: &Path) -> Option<String> {
    shell_path
        .file_name()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
}

fn read_shell_env(
    shell_path: &Path,
    home_dir: &Path,
    init_file_path: &Path,
    user_env: &str,
    use_runuser: bool,
) -> Result<Vec<(String, String)>> {
    let shell_name = shell_path
        .to_str()
        .ok_or_else(|| anyhow!("shell 路径不是有效 UTF-8"))?;
    let init_file = init_file_path
        .to_str()
        .ok_or_else(|| anyhow!("shell 启动文件路径不是有效 UTF-8"))?;
    let path_env = DEFAULT_PATH.to_string();

    let configure = |command: &mut Command| {
        configure_shell_env_command(
            command, shell_path, shell_name, init_file, home_dir, &path_env, user_env,
        );
    };

    if use_runuser {
        // Try runuser first; fall back to su on platforms where runuser is missing.
        let mut command = Command::new("runuser");
        command
            .arg("-u")
            .arg(user_env)
            .arg("--preserve-environment")
            .arg("--")
            .arg(shell_path);
        configure(&mut command);
        let output = command.output().with_context(|| {
            format!(
                "执行 shell 启动环境失败: shell={}, init={}",
                shell_path.display(),
                init_file_path.display()
            )
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            // runuser not found → retry with su
            if looks_like_command_not_found(&stderr, "runuser") {
                return read_shell_env_via_su(
                    shell_path, shell_name, init_file, home_dir, &path_env, user_env, &configure,
                );
            }
            anyhow::bail!(
                "执行 shell 启动环境失败: {}",
                if stderr.is_empty() {
                    "shell returned a non-zero status".to_string()
                } else {
                    stderr
                }
            );
        }

        return parse_shell_env_output(&output.stdout);
    }

    let mut command = Command::new(shell_path);
    configure(&mut command);
    let output = command.output().with_context(|| {
        format!(
            "执行 shell 启动环境失败: shell={}, init={}",
            shell_path.display(),
            init_file_path.display()
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "执行 shell 启动环境失败: {}",
            if stderr.is_empty() {
                "shell returned a non-zero status".to_string()
            } else {
                stderr
            }
        );
    }

    parse_shell_env_output(&output.stdout)
}

/// Read shell env via `su` instead of `runuser`. Used as a fallback on
/// minimal containers (Alpine, BusyBox) where `runuser` doesn't exist.
fn read_shell_env_via_su(
    shell_path: &Path,
    shell_name: &str,
    init_file: &str,
    home_dir: &Path,
    path_env: &str,
    user_env: &str,
    _configure: &dyn Fn(&mut Command),
) -> Result<Vec<(String, String)>> {
    // Build a one-shot command that `su` can run via -c.
    let script = shell_env_capture_script(&shell_basename(shell_path).unwrap_or_default());
    let wrapper = format!(
        r#"{shell_name} -f -i -c '{}' 'webclx-shell-env' '{}' '{}'"#,
        script.replace('\'', "'\\''"),
        init_file.replace('\'', "'\\''"),
        ENV_OUTPUT_SENTINEL.replace('\'', "'\\''"),
    );

    let mut command = Command::new("su");
    command.arg(user_env).arg("-c").arg(&wrapper);
    command
        .env_clear()
        .env("HOME", home_dir)
        .env("PATH", path_env)
        .env("SHELL", shell_name)
        .env("TERM", "xterm-256color")
        .env("USER", user_env)
        .env("LOGNAME", user_env);

    let output = command.output().with_context(|| {
        format!(
            "执行 shell 启动环境失败 (via su): shell={}, init={}",
            shell_path.display(),
            init_file
        )
    })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        anyhow::bail!(
            "执行 shell 启动环境失败 (via su): {}",
            if stderr.is_empty() {
                "shell returned a non-zero status".to_string()
            } else {
                stderr
            }
        );
    }

    parse_shell_env_output(&output.stdout)
}

/// Check whether the command's stderr indicates the binary was not found.
fn looks_like_command_not_found(stderr: &str, command_name: &str) -> bool {
    let lower = stderr.to_ascii_lowercase();
    lower.contains("not found")
        || lower.contains("no such file")
        || lower.contains("command not found")
        || lower.contains(&format!("{command_name}: not found").to_ascii_lowercase())
}

fn should_read_shell_via_runuser(user_profile: &crate::runtime_paths::UserProfile) -> bool {
    crate::runtime_paths::resolve_current_user_profile()
        .map(|current| current.uid != user_profile.uid)
        .unwrap_or(true)
}

fn configure_shell_env_command(
    command: &mut Command,
    shell_path: &Path,
    shell_name: &str,
    init_file: &str,
    home_dir: &Path,
    path_env: &str,
    user_env: &str,
) {
    let base_name = shell_basename(shell_path).unwrap_or_default();
    let script = shell_env_capture_script(&base_name);

    match base_name.as_str() {
        "bash" => {
            command.args([
                "--noprofile",
                "--norc",
                "-i",
                "-c",
                script,
                "webclx-shell-env",
            ]);
        }
        "zsh" => {
            command.args(["-f", "-i", "-c", script, "webclx-shell-env"]);
        }
        _ => {
            command.args(["-c", script, "webclx-shell-env"]);
        }
    }

    command
        .arg(init_file)
        .arg(ENV_OUTPUT_SENTINEL)
        .env_clear()
        .env("HOME", home_dir)
        .env("PATH", path_env)
        .env("SHELL", shell_name)
        .env("TERM", "xterm-256color")
        .env("USER", user_env)
        .env("LOGNAME", user_env);
}

fn shell_env_capture_script(shell_name: &str) -> &'static str {
    match shell_name {
        "fish" => {
            r#"if test -f "$argv[1]"; source "$argv[1]" >/dev/null 2>&1; end; printf '%s\0' "$argv[2]"; env -0"#
        }
        _ => r#"if [ -f "$1" ]; then . "$1" >/dev/null 2>&1; fi; printf '%s\0' "$2"; env -0"#,
    }
}

fn parse_shell_env_output(stdout: &[u8]) -> Result<Vec<(String, String)>> {
    let marker = format!("{ENV_OUTPUT_SENTINEL}\0");
    let marker = marker.as_bytes();
    let Some(start) = stdout
        .windows(marker.len())
        .position(|window| window == marker)
    else {
        anyhow::bail!("无法定位 shell 环境输出标记");
    };

    let payload = &stdout[start + marker.len()..];
    let mut entries = Vec::new();
    for entry in payload.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let text = String::from_utf8_lossy(entry);
        if let Some((key, value)) = text.split_once('=')
            && !key.trim().is_empty()
        {
            entries.push((key.to_string(), value.to_string()));
        }
    }

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::{
        filter_env_entries, merge_allowed_env_entries, parse_shell_env_output, read_shell_env,
    };
    use std::path::Path;

    #[test]
    fn filter_env_entries_keeps_requested_keys() {
        let entries = vec![
            ("HTTP_PROXY".to_string(), "http://127.0.0.1:7890".to_string()),
            ("PATH".to_string(), "/usr/bin".to_string()),
        ];

        let filtered = filter_env_entries(&entries, &["HTTP_PROXY"]);

        assert_eq!(filtered, vec![("HTTP_PROXY".to_string(), "http://127.0.0.1:7890".to_string())]);
    }

    #[test]
    fn inherited_network_env_keeps_no_proxy_and_applies_shell_override() {
        let merged = merge_allowed_env_entries(
            &[
                ("NO_PROXY".to_string(), "127.0.0.1,192.168.3.2".to_string()),
                ("HTTP_PROXY".to_string(), "http://service-proxy:7890".to_string()),
                ("IGNORED".to_string(), "service".to_string()),
            ],
            &[
                ("HTTP_PROXY".to_string(), "http://shell-proxy:17890".to_string()),
                ("PATH".to_string(), "/usr/bin".to_string()),
            ],
            &["HTTP_PROXY", "NO_PROXY"],
        );

        assert_eq!(
            merged,
            vec![
                ("NO_PROXY".to_string(), "127.0.0.1,192.168.3.2".to_string(),),
                ("HTTP_PROXY".to_string(), "http://shell-proxy:17890".to_string(),),
            ]
        );
    }

    #[test]
    fn parse_shell_env_output_ignores_prelude_before_marker() {
        let stdout = b"hello from rc\n__WEBCLX_ENV_BEGIN__\0HTTP_PROXY=http://127.0.0.1:7890\0PATH=/usr/bin\0";

        let parsed = parse_shell_env_output(stdout).expect("parse shell env output");

        assert_eq!(
            parsed,
            vec![
                ("HTTP_PROXY".to_string(), "http://127.0.0.1:7890".to_string()),
                ("PATH".to_string(), "/usr/bin".to_string()),
            ]
        );
    }

    #[test]
    fn read_shell_env_supports_sourced_proxy_scripts() {
        let dir =
            std::env::temp_dir().join(format!("webclx-shell-env-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let shell_path = dir.join("proxy.sh");
        let bashrc_path = dir.join(".bashrc");

        std::fs::write(
            &shell_path,
            r#"#!/bin/bash
case "$1" in
  "127")
    export http_proxy="http://127.0.0.1:7890"
    export HTTPS_PROXY="http://127.0.0.1:7890"
    export NO_PROXY="localhost,127.0.0.1"
    ;;
esac
"#,
        )
        .expect("write proxy shell");
        std::fs::write(&bashrc_path, format!("source \"{}\" 127\n", shell_path.display()))
            .expect("write bashrc");

        let mut entries = filter_env_entries(
            &read_shell_env(Path::new("/bin/bash"), &dir, &bashrc_path, "testuser", false)
                .expect("read shell env"),
            &["HTTPS_PROXY", "NO_PROXY", "http_proxy"],
        );

        std::fs::remove_file(&bashrc_path).expect("cleanup bashrc");
        std::fs::remove_file(&shell_path).expect("cleanup proxy shell");
        std::fs::remove_dir(&dir).expect("cleanup temp dir");

        entries.sort();

        let mut expected = vec![
            ("HTTPS_PROXY".to_string(), "http://127.0.0.1:7890".to_string()),
            ("NO_PROXY".to_string(), "localhost,127.0.0.1".to_string()),
            ("http_proxy".to_string(), "http://127.0.0.1:7890".to_string()),
        ];
        expected.sort();

        assert_eq!(entries, expected);
    }
}
