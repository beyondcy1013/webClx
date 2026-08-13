use std::{
    collections::HashSet,
    env,
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};
use tracing::{info, warn};

const STARTUP_INSTALL_ENV: &str = "WEBCLX_STARTUP_INSTALL";
const DISABLE_STARTUP_INSTALL_ENV: &str = "WEBCLX_DISABLE_STARTUP_INSTALL";
const STARTUP_INSTALL_LOCK_FILE: &str = ".webclx-startup-install.lock";
const STARTUP_INSTALL_LOG_FILE: &str = "webclx-startup-install.log";
const STALE_LOCK_AFTER: Duration = Duration::from_secs(60 * 60);
const NPM_REGISTRY: &str = "https://registry.npmjs.org";
#[cfg(not(windows))]
const DEFAULT_COMMAND_BIN_DIR: &str = "/usr/local/bin";

#[cfg(not(windows))]
const DEFAULT_PATH_DIRS: [&str; 6] = [
    "/usr/local/sbin",
    "/usr/local/bin",
    "/usr/sbin",
    "/usr/bin",
    "/sbin",
    "/bin",
];
#[cfg(windows)]
const DEFAULT_PATH_DIRS: [&str; 4] = [
    "C:\\Windows\\System32",
    "C:\\Windows",
    "C:\\Windows\\System32\\WindowsPowerShell\\v1.0",
    "C:\\Program Files\\nodejs",
];
const NPM_CLI_TOOLS: [NpmCliTool; 2] = [
    NpmCliTool {
        command_name: "codex",
        package_name: "@openai/codex@latest",
    },
    NpmCliTool {
        command_name: "claude",
        package_name: "@anthropic-ai/claude-code@latest",
    },
];

#[derive(Debug, Clone, Copy)]
struct NpmCliTool {
    command_name: &'static str,
    package_name: &'static str,
}

#[derive(Debug)]
struct BootstrapPaths {
    home_dir: PathBuf,
    data_dir: PathBuf,
    node_dir: PathBuf,
    npm_prefix: PathBuf,
    command_bin_dir: PathBuf,
}

struct InstallLock {
    path: PathBuf,
}

impl Drop for InstallLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub fn spawn_startup_tool_bootstrap(app_dir: PathBuf) {
    if cfg!(windows) {
        info!("startup tool bootstrap skipped on Windows");
        return;
    }

    if !startup_install_enabled() {
        info!(
            "startup tool bootstrap disabled by {STARTUP_INSTALL_ENV}/{DISABLE_STARTUP_INSTALL_ENV}"
        );
        return;
    }

    match std::thread::Builder::new()
        .name("webclx-startup-tools".to_string())
        .spawn(move || {
            if let Err(error) = run_startup_tool_bootstrap(&app_dir) {
                warn!("startup tool bootstrap failed: {error}");
            }
        }) {
        Ok(_) => info!("startup tool bootstrap scheduled"),
        Err(error) => warn!("failed to schedule startup tool bootstrap: {error}"),
    }
}

pub fn preferred_tool_bin_dirs() -> Vec<PathBuf> {
    let Some(home_dir) = crate::runtime_paths::resolve_current_user_home() else {
        return Vec::new();
    };
    preferred_tool_bin_dirs_for_home(&home_dir)
}

pub fn augment_path_env(existing: Option<&str>) -> String {
    augment_path_env_with_dirs(existing, &preferred_tool_bin_dirs())
}

fn run_startup_tool_bootstrap(app_dir: &Path) -> Result<()> {
    let log_path = app_dir.join(STARTUP_INSTALL_LOG_FILE);
    let mut log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .with_context(|| format!("cannot open {}", log_path.display()))?;
    log_line(&mut log, "startup bootstrap begin");

    let Some(home_dir) = crate::runtime_paths::resolve_current_user_home() else {
        log_line(&mut log, "skip: cannot resolve current user home");
        return Ok(());
    };

    let _lock = match acquire_install_lock(app_dir, &mut log)? {
        Some(lock) => lock,
        None => return Ok(()),
    };

    let paths = BootstrapPaths::new(home_dir);
    paths.create_dirs()?;
    let mut path_env = bootstrap_detection_path();

    ensure_node_and_npm(&paths, &mut path_env, &mut log)?;
    for tool in NPM_CLI_TOOLS {
        ensure_npm_cli(tool, &paths, &mut path_env, &mut log)?;
    }

    log_line(&mut log, "startup bootstrap end");
    Ok(())
}

fn startup_install_enabled() -> bool {
    if env_flag_truthy(DISABLE_STARTUP_INSTALL_ENV) {
        return false;
    }

    !env_flag_falsey(STARTUP_INSTALL_ENV)
}

fn env_flag_truthy(name: &str) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn env_flag_falsey(name: &str) -> bool {
    env::var(name)
        .ok()
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off" | "disabled"
            )
        })
        .unwrap_or(false)
}

fn acquire_install_lock(app_dir: &Path, log: &mut File) -> Result<Option<InstallLock>> {
    let path = app_dir.join(STARTUP_INSTALL_LOCK_FILE);
    if lock_is_stale(&path) {
        let _ = fs::remove_file(&path);
    }

    match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(mut file) => {
            writeln!(file, "pid={}", std::process::id()).ok();
            Ok(Some(InstallLock { path }))
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            log_line(log, "skip: startup bootstrap already running");
            Ok(None)
        }
        Err(error) => Err(error).with_context(|| format!("cannot create {}", path.display())),
    }
}

fn lock_is_stale(path: &Path) -> bool {
    if lock_recorded_process_is_gone(path) {
        return true;
    }

    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .is_ok_and(|age| age > STALE_LOCK_AFTER)
}

#[cfg(unix)]
fn lock_recorded_process_is_gone(path: &Path) -> bool {
    let Ok(content) = fs::read_to_string(path) else {
        return false;
    };
    let Some(pid) = content
        .lines()
        .find_map(|line| line.strip_prefix("pid="))
        .and_then(|value| value.trim().parse::<u32>().ok())
    else {
        return false;
    };

    !PathBuf::from(format!("/proc/{pid}")).exists()
}

#[cfg(not(unix))]
fn lock_recorded_process_is_gone(_path: &Path) -> bool {
    false
}

impl BootstrapPaths {
    fn new(home_dir: PathBuf) -> Self {
        let data_dir = home_dir.join(".local/share/webclx");
        Self {
            node_dir: data_dir.join("node"),
            npm_prefix: data_dir.join("npm-global"),
            command_bin_dir: default_command_bin_dir(),
            home_dir,
            data_dir,
        }
    }

    fn create_dirs(&self) -> Result<()> {
        fs::create_dir_all(&self.data_dir)
            .with_context(|| format!("cannot create {}", self.data_dir.display()))?;
        fs::create_dir_all(&self.npm_prefix)
            .with_context(|| format!("cannot create {}", self.npm_prefix.display()))?;
        fs::create_dir_all(&self.command_bin_dir)
            .with_context(|| format!("cannot create {}", self.command_bin_dir.display()))?;
        Ok(())
    }
}

fn default_command_bin_dir() -> PathBuf {
    #[cfg(not(windows))]
    {
        PathBuf::from(DEFAULT_COMMAND_BIN_DIR)
    }
    #[cfg(windows)]
    {
        PathBuf::from(r"C:\Program Files\webClx\bin")
    }
}

fn preferred_tool_bin_dirs_for_home(home_dir: &Path) -> Vec<PathBuf> {
    vec![
        default_command_bin_dir(),
        home_dir.join(".local/share/webclx/npm-global/bin"),
        home_dir.join(".local/share/webclx/node/bin"),
    ]
}

fn bootstrap_detection_path() -> String {
    let mut path_env = augment_path_env(env::var("PATH").ok().as_deref());

    if let Ok(snapshot) = crate::shell_env::read_current_user_shell_env()
        && let Some((_, shell_path)) = snapshot.entries.iter().find(|(key, _)| key == "PATH")
    {
        path_env = merge_path_values(&path_env, shell_path);
    }

    path_env
}

fn augment_path_env_with_dirs(existing: Option<&str>, preferred_dirs: &[PathBuf]) -> String {
    augment_path_env_with_dirs_filtered(existing, preferred_dirs, false)
}

fn augment_path_env_with_dirs_filtered(
    existing: Option<&str>,
    preferred_dirs: &[PathBuf],
    only_existing: bool,
) -> String {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    for path in preferred_dirs {
        if only_existing && !path.is_dir() {
            continue;
        }
        push_unique_path(&mut paths, &mut seen, path.clone());
    }

    if let Some(existing) = existing {
        for path in env::split_paths(existing) {
            if only_existing && !path.is_dir() {
                continue;
            }
            push_unique_path(&mut paths, &mut seen, path);
        }
    }

    for path in DEFAULT_PATH_DIRS {
        if only_existing && !Path::new(path).is_dir() {
            continue;
        }
        push_unique_path(&mut paths, &mut seen, PathBuf::from(path));
    }

    join_path_values(&paths)
}

fn merge_path_values(left: &str, right: &str) -> String {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();

    for path in env::split_paths(left) {
        push_unique_path(&mut paths, &mut seen, path);
    }
    for path in env::split_paths(right) {
        push_unique_path(&mut paths, &mut seen, path);
    }

    join_path_values(&paths)
}

fn push_unique_path(paths: &mut Vec<PathBuf>, seen: &mut HashSet<OsString>, path: PathBuf) {
    if path.as_os_str().is_empty() {
        return;
    }
    if seen.insert(path.as_os_str().to_os_string()) {
        paths.push(path);
    }
}

fn join_path_values(paths: &[PathBuf]) -> String {
    env::join_paths(paths)
        .unwrap_or_else(|_| OsString::from(DEFAULT_PATH_DIRS.join(":")))
        .to_string_lossy()
        .to_string()
}

fn ensure_node_and_npm(
    paths: &BootstrapPaths,
    path_env: &mut String,
    log: &mut File,
) -> Result<()> {
    let node_found = find_command_in_path("node", path_env).is_some();
    let npm_found = find_command_in_path("npm", path_env).is_some();

    if node_found && npm_found {
        log_tool_version("node", path_env, log);
        log_tool_version("npm", path_env, log);
        ensure_npmrc_prefix(paths, log)?;
        return Ok(());
    }

    log_line(log, "node/npm missing; installing user-local Node.js");
    install_user_local_node(paths, path_env, log)?;
    *path_env = augment_path_env(Some(path_env));

    log_tool_version("node", path_env, log);
    log_tool_version("npm", path_env, log);
    ensure_npmrc_prefix(paths, log)?;
    Ok(())
}

fn ensure_npmrc_prefix(paths: &BootstrapPaths, log: &mut File) -> Result<()> {
    let npmrc = paths.node_dir.join("etc/npmrc");
    let prefix_line = format!("prefix={}\n", paths.npm_prefix.display());
    let need_write = match fs::read_to_string(&npmrc) {
        Ok(current) => {
            if current.contains(prefix_line.trim_end()) {
                false
            } else {
                log_line(log, &format!("updating npmrc prefix in {}", npmrc.display()));
                true
            }
        }
        Err(_) => true,
    };
    if need_write {
        if let Some(parent) = npmrc.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        let updated = match fs::read_to_string(&npmrc) {
            Ok(current) => {
                let mut lines: Vec<&str> = current.lines().collect();
                if let Some(pos) = lines.iter().position(|l| l.starts_with("prefix=")) {
                    lines[pos] = prefix_line.trim_end();
                    lines.join("\n") + "\n"
                } else {
                    current + &prefix_line
                }
            }
            Err(_) => prefix_line,
        };
        fs::write(&npmrc, updated).with_context(|| format!("cannot write {}", npmrc.display()))?;
    }
    Ok(())
}

fn install_user_local_node(paths: &BootstrapPaths, path_env: &str, log: &mut File) -> Result<()> {
    if paths.node_dir.join("bin/node").is_file() && paths.node_dir.join("bin/npm").is_file() {
        log_line(log, "user-local Node.js already exists; refreshing symlinks");
        link_node_bins(paths, log)?;
        return Ok(());
    }

    let script = user_local_node_install_script();
    let output = Command::new("sh")
        .arg("-c")
        .arg(script)
        .arg("webclx-node-install")
        .arg(&paths.data_dir)
        .env("PATH", path_env)
        .env("HOME", &paths.home_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to run Node.js installer")?;
    log_output(log, "node install", &output);

    if !output.status.success() {
        anyhow::bail!("Node.js install failed with status {}", output_status_text(&output));
    }

    link_node_bins(paths, log)?;
    Ok(())
}

fn user_local_node_install_script() -> &'static str {
    r#"
set -eu
root="$1"
node_dir="$root/node"
arch="$(uname -m)"
case "$arch" in
  x86_64|amd64) node_arch="x64" ;;
  aarch64|arm64) node_arch="arm64" ;;
  armv7l) node_arch="armv7l" ;;
  *) echo "unsupported architecture: $arch" >&2; exit 42 ;;
esac

if ! command -v tar >/dev/null 2>&1; then
  echo "tar is required to install Node.js" >&2
  exit 43
fi

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cd "$tmp"

if command -v curl >/dev/null 2>&1; then
  curl -fsSL --retry 2 "https://nodejs.org/dist/latest-v22.x/SHASUMS256.txt" -o SHASUMS256.txt
elif command -v wget >/dev/null 2>&1; then
  wget -q "https://nodejs.org/dist/latest-v22.x/SHASUMS256.txt" -O SHASUMS256.txt
else
  echo "curl or wget is required to install Node.js" >&2
  exit 44
fi

tarball="$(awk -v arch="$node_arch" '$2 ~ "^node-v[0-9].*-linux-" arch "\\.tar\\.xz$" {print $2; exit}' SHASUMS256.txt)"
if [ -z "$tarball" ]; then
  echo "cannot find Node.js linux-$node_arch tarball in SHASUMS256.txt" >&2
  exit 45
fi

if command -v curl >/dev/null 2>&1; then
  curl -fsSLO --retry 2 "https://nodejs.org/dist/latest-v22.x/$tarball"
else
  wget -q "https://nodejs.org/dist/latest-v22.x/$tarball"
fi

if command -v sha256sum >/dev/null 2>&1; then
  sha256sum -c --ignore-missing SHASUMS256.txt
fi

rm -rf "$node_dir.tmp"
mkdir -p "$root"
tar -xJf "$tarball" -C "$root"
extracted="${tarball%.tar.xz}"
mv "$root/$extracted" "$node_dir.tmp"
rm -rf "$node_dir"
mv "$node_dir.tmp" "$node_dir"
"#
}

fn link_node_bins(paths: &BootstrapPaths, log: &mut File) -> Result<()> {
    for name in ["node", "npm", "npx", "corepack"] {
        let source = paths.node_dir.join("bin").join(name);
        if source.exists() {
            safe_symlink(&source, &paths.command_bin_dir.join(name), log)?;
        }
    }
    Ok(())
}

fn ensure_npm_cli(
    tool: NpmCliTool,
    paths: &BootstrapPaths,
    path_env: &mut String,
    log: &mut File,
) -> Result<()> {
    let command_name = tool.command_name;
    let package_name = tool.package_name;

    if command_version_check_succeeds(command_name, path_env) {
        if command_name == "claude" {
            ensure_claude_command_link(paths, path_env, log)?;
            *path_env = augment_path_env(Some(path_env));
        }
        log_tool_version(command_name, path_env, log);
        return Ok(());
    }

    let Some(npm_path) = find_command_in_path("npm", path_env) else {
        log_line(
            log,
            &format!("skip {command_name}: npm is not available after Node.js bootstrap"),
        );
        return Ok(());
    };

    log_line(log, &format!("{command_name} missing; installing {package_name}"));
    fs::create_dir_all(&paths.npm_prefix)
        .with_context(|| format!("cannot create {}", paths.npm_prefix.display()))?;

    let output = Command::new(npm_path)
        .args([
            "install",
            "-g",
            package_name,
            "--registry",
            NPM_REGISTRY,
            "--fund=false",
            "--audit=false",
        ])
        .arg("--prefix")
        .arg(&paths.npm_prefix)
        .env("PATH", path_env.as_str())
        .env("HOME", &paths.home_dir)
        .env("NPM_CONFIG_UPDATE_NOTIFIER", "false")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to run npm install for {package_name}"))?;
    log_output(log, &format!("npm install {package_name}"), &output);

    if !output.status.success() {
        log_line(
            log,
            &format!(
                "{command_name} install failed with status {}; continuing",
                output_status_text(&output)
            ),
        );
        return Ok(());
    }

    *path_env = augment_path_env(Some(path_env));
    if command_name == "claude" {
        ensure_claude_command_link(paths, path_env, log)?;
    } else if let Some(installed_command) = find_command_in_path(command_name, path_env) {
        safe_symlink(&installed_command, &paths.command_bin_dir.join(command_name), log)?;
    }
    log_tool_version(command_name, path_env, log);
    Ok(())
}

fn ensure_claude_command_link(
    paths: &BootstrapPaths,
    path_env: &str,
    log: &mut File,
) -> Result<()> {
    let command_path = paths.command_bin_dir.join("claude");
    remove_managed_claude_snapshot_script(&command_path, log)?;
    remove_legacy_user_local_claude_snapshot_script(paths, log)?;
    if is_executable_file(&command_path) {
        return Ok(());
    }
    let Some(real_claude) = find_claude_real_command(paths, path_env, &command_path) else {
        log_line(log, "skip claude command link: real executable not found");
        return Ok(());
    };
    safe_symlink(&real_claude, &command_path, log)?;
    log_line(
        log,
        &format!("claude command link: {} -> {}", command_path.display(), real_claude.display()),
    );
    Ok(())
}

fn remove_legacy_user_local_claude_snapshot_script(
    paths: &BootstrapPaths,
    log: &mut File,
) -> Result<()> {
    let legacy_path = paths.home_dir.join(".local/bin/claude");
    if legacy_path == paths.command_bin_dir.join("claude") {
        return Ok(());
    }
    remove_managed_claude_snapshot_script(&legacy_path, log)
}

fn remove_managed_claude_snapshot_script(path: &Path, log: &mut File) -> Result<()> {
    let Ok(content) = fs::read_to_string(path) else {
        return Ok(());
    };
    if !content.contains(".cache/webclx/claude-config-snapshots") {
        return Ok(());
    }
    fs::remove_file(path).with_context(|| format!("cannot remove legacy {}", path.display()))?;
    log_line(log, &format!("removed legacy Claude snapshot script {}", path.display()));
    Ok(())
}

fn find_claude_real_command(
    paths: &BootstrapPaths,
    path_env: &str,
    wrapper_path: &Path,
) -> Option<PathBuf> {
    let managed_npm_bin = paths.npm_prefix.join("bin/claude");
    if is_executable_file(&managed_npm_bin) && managed_npm_bin != wrapper_path {
        return Some(managed_npm_bin);
    }

    find_command_in_path_excluding("claude", path_env, &[wrapper_path])
}

fn find_command_in_path_excluding(
    command_name: &str,
    path_env: &str,
    excluded_paths: &[&Path],
) -> Option<PathBuf> {
    env::split_paths(path_env)
        .map(|dir| dir.join(command_name))
        .find(|path| {
            is_executable_file(path)
                && excluded_paths
                    .iter()
                    .all(|excluded| path.as_path() != *excluded)
        })
}

fn find_command_in_path(command_name: &str, path_env: &str) -> Option<PathBuf> {
    if command_name.contains('/') {
        let path = PathBuf::from(command_name);
        return is_executable_file(&path).then_some(path);
    }

    env::split_paths(path_env)
        .map(|dir| dir.join(command_name))
        .find(|path| is_executable_file(path))
}

fn command_version_check_succeeds(command_name: &str, path_env: &str) -> bool {
    let Some(command_path) = find_command_in_path(command_name, path_env) else {
        return false;
    };

    Command::new(command_path)
        .arg("--version")
        .env("PATH", path_env)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn log_tool_version(command_name: &str, path_env: &str, log: &mut File) {
    let Some(command_path) = find_command_in_path(command_name, path_env) else {
        log_line(log, &format!("{command_name}: not found"));
        return;
    };

    let output = Command::new(&command_path)
        .arg("--version")
        .env("PATH", path_env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
            log_line(
                log,
                &format!(
                    "{command_name}: {} ({})",
                    if version.is_empty() {
                        "installed"
                    } else {
                        &version
                    },
                    command_path.display()
                ),
            );
        }
        Ok(output) => {
            log_line(
                log,
                &format!(
                    "{command_name}: version check failed at {} ({})",
                    command_path.display(),
                    output_status_text(&output)
                ),
            );
        }
        Err(error) => {
            log_line(
                log,
                &format!(
                    "{command_name}: version check failed at {} ({error})",
                    command_path.display()
                ),
            );
        }
    }
}

#[cfg(unix)]
fn safe_symlink(source: &Path, destination: &Path, log: &mut File) -> Result<()> {
    use std::os::unix::fs::symlink;

    if let Ok(existing_target) = fs::read_link(destination) {
        if existing_target == source {
            return Ok(());
        }
        log_line(
            log,
            &format!(
                "skip symlink {} -> {}: destination already links to {}",
                destination.display(),
                source.display(),
                existing_target.display()
            ),
        );
        return Ok(());
    } else if destination.exists() {
        log_line(
            log,
            &format!(
                "skip symlink {} -> {}: destination already exists",
                destination.display(),
                source.display()
            ),
        );
        return Ok(());
    }

    symlink(source, destination).with_context(|| {
        format!("cannot symlink {} -> {}", destination.display(), source.display())
    })?;
    Ok(())
}

#[cfg(not(unix))]
fn safe_symlink(_source: &Path, _destination: &Path, _log: &mut File) -> Result<()> {
    Ok(())
}

fn log_line(log: &mut File, message: &str) {
    writeln!(log, "[{}] {message}", timestamp_seconds()).ok();
    log.flush().ok();
}

fn log_output(log: &mut File, label: &str, output: &Output) {
    log_line(log, &format!("{label}: {}", output_status_text(output)));
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !stdout.trim().is_empty() {
        log_line(log, &format!("{label} stdout:\n{}", stdout.trim()));
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.trim().is_empty() {
        log_line(log, &format!("{label} stderr:\n{}", stderr.trim()));
    }
}

fn output_status_text(output: &Output) -> String {
    output
        .status
        .code()
        .map(|code| format!("exit code {code}"))
        .unwrap_or_else(|| "terminated by signal".to_string())
}

fn timestamp_seconds() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        BootstrapPaths, STARTUP_INSTALL_LOCK_FILE, augment_path_env_with_dirs,
        command_version_check_succeeds, default_command_bin_dir, find_claude_real_command,
        find_command_in_path, lock_is_stale, merge_path_values, preferred_tool_bin_dirs_for_home,
    };
    use std::{fs, path::PathBuf};

    #[test]
    fn preferred_tool_bin_dirs_use_standard_command_path_first() {
        let dirs = preferred_tool_bin_dirs_for_home(&PathBuf::from("/home/demo"));

        assert_eq!(
            dirs,
            vec![
                default_command_bin_dir(),
                PathBuf::from("/home/demo/.local/share/webclx/npm-global/bin"),
                PathBuf::from("/home/demo/.local/share/webclx/node/bin"),
            ]
        );
    }

    #[test]
    fn augment_path_env_prepends_preferred_dirs_and_deduplicates() {
        let preferred = vec![PathBuf::from("/home/demo/.local/bin")];
        let path =
            augment_path_env_with_dirs(Some("/usr/bin:/home/demo/.local/bin:/bin"), &preferred);
        let parts = std::env::split_paths(&path).collect::<Vec<_>>();

        assert_eq!(parts.first(), Some(&PathBuf::from("/home/demo/.local/bin")));
        assert_eq!(
            parts
                .iter()
                .filter(|path| *path == &PathBuf::from("/home/demo/.local/bin"))
                .count(),
            1
        );
        assert!(parts.contains(&PathBuf::from("/usr/bin")));
    }

    #[test]
    fn merge_path_values_preserves_unique_order() {
        let merged = merge_path_values("/a:/b", "/b:/c");
        let parts = std::env::split_paths(&merged).collect::<Vec<_>>();

        assert_eq!(
            parts,
            vec![
                PathBuf::from("/a"),
                PathBuf::from("/b"),
                PathBuf::from("/c")
            ]
        );
    }

    #[test]
    fn find_command_in_path_requires_executable_file() {
        let dir =
            std::env::temp_dir().join(format!("webclx-find-command-test-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");
        let command_path = dir.join("demo-command");
        fs::write(&command_path, b"#!/bin/sh\n").expect("write command");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&command_path, fs::Permissions::from_mode(0o755))
                .expect("chmod command");
        }

        let found = find_command_in_path("demo-command", &dir.display().to_string());
        let _ = fs::remove_file(&command_path);
        let _ = fs::remove_dir(&dir);

        assert_eq!(found, Some(command_path));
    }

    #[test]
    fn command_health_check_rejects_broken_executable() {
        let dir =
            std::env::temp_dir().join(format!("webclx-command-health-test-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");
        let command_path = dir.join("demo-command");
        fs::write(&command_path, b"#!/bin/sh\necho native binary not installed >&2\nexit 1\n")
            .expect("write command");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&command_path, fs::Permissions::from_mode(0o755))
                .expect("chmod command");
        }

        let healthy = command_version_check_succeeds("demo-command", &dir.display().to_string());
        let _ = fs::remove_file(&command_path);
        let _ = fs::remove_dir(&dir);

        assert!(!healthy);
    }

    #[test]
    fn claude_real_command_prefers_managed_npm_bin_over_command_link() {
        let unique = format!("webclx-claude-real-command-test-{}", std::process::id());
        let dir = std::env::temp_dir().join(unique);
        let home_dir = dir.join("home");
        let mut paths = BootstrapPaths::new(home_dir.clone());
        paths.command_bin_dir = dir.join("bin");
        fs::create_dir_all(paths.npm_prefix.join("bin")).expect("create npm bin");
        fs::create_dir_all(&paths.command_bin_dir).expect("create command bin");
        let command_link = paths.command_bin_dir.join("claude");
        let real_path = paths.npm_prefix.join("bin/claude");
        fs::write(&command_link, b"#!/bin/sh\nexit 1\n").expect("write command link");
        fs::write(&real_path, b"#!/bin/sh\nexit 0\n").expect("write real");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&command_link, fs::Permissions::from_mode(0o755))
                .expect("chmod command link");
            fs::set_permissions(&real_path, fs::Permissions::from_mode(0o755)).expect("chmod real");
        }

        let path_env = format!(
            "{}:{}",
            paths.command_bin_dir.display(),
            paths.npm_prefix.join("bin").display()
        );
        let found = find_claude_real_command(&paths, &path_env, &command_link);

        let _ = fs::remove_dir_all(&dir);

        assert_eq!(found, Some(real_path));
    }

    #[test]
    fn startup_lock_is_stale_when_recorded_pid_is_gone() {
        let dir =
            std::env::temp_dir().join(format!("webclx-startup-lock-test-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("create temp dir");
        let lock_path = dir.join(STARTUP_INSTALL_LOCK_FILE);
        fs::write(&lock_path, "pid=99999999\n").expect("write lock");

        let stale = lock_is_stale(&lock_path);
        let _ = fs::remove_file(&lock_path);
        let _ = fs::remove_dir(&dir);

        assert!(stale);
    }
}
