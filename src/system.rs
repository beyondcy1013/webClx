use std::process::Command;
use std::{
    collections::{HashMap, HashSet},
    net::IpAddr,
    path::Path,
    time::Duration,
};

use axum::http::{HeaderValue, header};
use axum::{Json, extract::State, response::IntoResponse};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{ApiResult, AppError, AppState};

#[cfg(target_os = "linux")]
const SERVICE_PROXY_ENV_FILE: &str = "/etc/default/webclx";
#[cfg(target_os = "windows")]
const SERVICE_PROXY_ENV_FILE: &str = "C:\\webclx\\proxy.env";
#[cfg(not(any(target_os = "linux", windows)))]
const SERVICE_PROXY_ENV_FILE: &str = "/etc/default/webclx";

#[cfg(target_os = "linux")]
const REFERENCE_PROXY_ENV_FILE: &str = "/etc/environment";
#[cfg(target_os = "windows")]
const REFERENCE_PROXY_ENV_FILE: &str = "C:\\webclx\\environment";
#[cfg(not(any(target_os = "linux", windows)))]
const REFERENCE_PROXY_ENV_FILE: &str = "/etc/environment";
#[cfg(target_os = "linux")]
const RESTART_SCHEDULER_PROGRAM: &str = "/usr/bin/systemd-run";
#[cfg(target_os = "linux")]
const RESTART_SCHEDULER_ARGS: &[&str] = &[
    "--quiet",
    "--collect",
    "--on-active=1s",
    "/bin/systemctl",
    "restart",
    "webclx.service",
];
#[cfg(target_os = "linux")]
const POWEROFF_SCHEDULER_PROGRAM: &str = "/usr/bin/systemd-run";
#[cfg(target_os = "linux")]
const POWEROFF_SCHEDULER_ARGS: &[&str] = &[
    "--quiet",
    "--collect",
    "--on-active=2s",
    "/bin/systemctl",
    "poweroff",
];
#[cfg(windows)]
const RESTART_SCHEDULER_PROGRAM: &str = "powershell";
#[cfg(windows)]
const RESTART_SCHEDULER_ARGS: &[&str] =
    &["-Command", "Restart-Service", "-Name", "webclx", "-Force"];
#[cfg(not(any(target_os = "linux", windows)))]
const RESTART_SCHEDULER_PROGRAM: &str = "";
#[cfg(not(any(target_os = "linux", windows)))]
const RESTART_SCHEDULER_ARGS: &[&str] = &[];
const SYSTEM_PROXY_KEYS: [&str; 8] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "no_proxy",
];

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Serialize)]
pub struct SystemInfo {
    pub user: String,
    pub pid: u32,
    pub uid: u32,
    pub gid: u32,
    pub app_dir: String,
    pub listen_addr: String,
    pub version: String,
}

#[derive(Debug, Serialize)]
pub struct UpdateCheckResponse {
    pub current_version: String,
    pub binary_url: String,
    pub version: String,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_ip: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SystemLogsResponse {
    pub logs: String,
}

#[derive(Debug, Serialize)]
pub struct SystemProxyResponse {
    pub process_env: Vec<String>,
    pub service_env_file: Vec<String>,
    pub service_env_file_path: String,
    pub user_shell_env_file: Vec<String>,
    pub user_shell_env_file_path: String,
    pub user_shell_read_error: Option<String>,
    pub reference_env_file: Vec<String>,
    pub reference_env_file_path: String,
    pub environment_file: Vec<String>,
    pub environment_file_path: String,
    pub can_write: bool,
    pub restart_required: bool,
    pub note: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveSystemProxyRequest {
    #[serde(default)]
    pub http_proxy: Option<String>,
    #[serde(default)]
    pub https_proxy: Option<String>,
    #[serde(default)]
    pub all_proxy: Option<String>,
    #[serde(default)]
    pub no_proxy: Option<String>,
}

pub async fn get_system_info(State(state): State<AppState>) -> ApiResult<Json<SystemInfo>> {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_else(|_| "unknown".to_string());

    let pid = std::process::id();
    let uid = current_uid();
    let gid = current_gid();
    let app_dir = std::env::current_dir()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    let listen_addr = state.listen_addr.to_string();

    Ok(Json(SystemInfo {
        user,
        pid,
        uid,
        gid,
        app_dir,
        listen_addr,
        version: APP_VERSION.to_string(),
    }))
}

pub async fn get_system_logs(
    State(_state): State<AppState>,
) -> ApiResult<Json<SystemLogsResponse>> {
    #[cfg(target_os = "linux")]
    let logs = {
        let output = Command::new("journalctl")
            .args(["-u", "webclx", "-n", "30", "--no-pager"])
            .output();
        match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            }
            Ok(out) => format!(
                "[journalctl failed: {}]\n{}",
                String::from_utf8_lossy(&out.stderr),
                String::from_utf8_lossy(&out.stdout)
            ),
            Err(e) => format!("无法读取日志: {e}"),
        }
    };
    #[cfg(windows)]
    let logs = {
        let log_path = std::path::Path::new("webclx.log");
        match std::fs::read_to_string(log_path) {
            Ok(content) => content
                .lines()
                .rev()
                .take(30)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<Vec<_>>()
                .join("\n"),
            Err(e) => format!("无法读取日志: {e}"),
        }
    };

    Ok(Json(SystemLogsResponse { logs }))
}

pub async fn get_update_check(
    State(state): State<AppState>,
) -> ApiResult<Json<UpdateCheckResponse>> {
    let public_ip = detect_public_ip(state.listen_addr.ip()).await;
    let binary_url = build_update_download_url(state.listen_addr, public_ip);

    Ok(Json(UpdateCheckResponse {
        current_version: state.version.clone(),
        binary_url: binary_url.clone(),
        version: state.version.clone(),
        url: binary_url,
        public_ip: public_ip.map(|ip| ip.to_string()),
    }))
}

async fn detect_public_ip(listen_ip: IpAddr) -> Option<IpAddr> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
        .ok()?;

    let endpoints: &[&str] = if listen_ip.is_ipv4() {
        &[
            "https://api4.ipify.org",
            "https://ipv4.icanhazip.com",
            "https://ifconfig.me/ip",
        ]
    } else {
        &[
            "https://api6.ipify.org",
            "https://ipv6.icanhazip.com",
            "https://ifconfig.me/ip",
        ]
    };

    for endpoint in endpoints {
        let Ok(response) = client.get(*endpoint).send().await else {
            continue;
        };
        if !response.status().is_success() {
            continue;
        }
        let Ok(body) = response.text().await else {
            continue;
        };
        let candidate = body.trim();
        let Ok(ip) = candidate.parse::<IpAddr>() else {
            continue;
        };
        if !ip.is_loopback() && !ip.is_unspecified() && ip.is_ipv4() == listen_ip.is_ipv4() {
            return Some(ip);
        }
    }

    None
}

fn build_update_download_url(
    listen_addr: std::net::SocketAddr,
    public_ip: Option<IpAddr>,
) -> String {
    let ip = public_ip.unwrap_or_else(|| listen_addr.ip());
    let host = match ip {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    };
    format!("http://{}:{}/api/update/download", host, listen_addr.port())
}

pub async fn get_update_binary(State(state): State<AppState>) -> impl IntoResponse {
    let binary_path = state.app_dir.join("webClx");
    let bytes = match tokio::fs::read(&binary_path).await {
        Ok(b) => b,
        Err(e) => return Err(AppError::internal(format!("读取二进制文件失败: {}", e))),
    };

    let filename = format!("webclx-{}", state.version);
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/octet-stream"));
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{}\"", filename))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );

    Ok::<_, AppError>(response)
}

pub async fn get_system_proxy(
    State(state): State<AppState>,
) -> ApiResult<Json<SystemProxyResponse>> {
    let user_profile = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("用户身份无效: {error}")))?;
    Ok(Json(build_system_proxy_response(&user_profile)))
}

pub async fn save_system_proxy(
    State(state): State<AppState>,
    Json(payload): Json<SaveSystemProxyRequest>,
) -> ApiResult<Json<SystemProxyResponse>> {
    write_system_proxy_file(&payload)
        .map_err(|error| AppError::internal(format!("写入服务代理失败: {error}")))?;
    let user_profile = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("用户身份无效: {error}")))?;
    Ok(Json(build_system_proxy_response(&user_profile)))
}

pub async fn clear_system_proxy(
    State(state): State<AppState>,
) -> ApiResult<Json<SystemProxyResponse>> {
    write_system_proxy_file(&SaveSystemProxyRequest {
        http_proxy: None,
        https_proxy: None,
        all_proxy: None,
        no_proxy: None,
    })
    .map_err(|error| AppError::internal(format!("清除服务代理失败: {error}")))?;
    let user_profile = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("用户身份无效: {error}")))?;
    Ok(Json(build_system_proxy_response(&user_profile)))
}

pub async fn restart_service(State(_state): State<AppState>) -> ApiResult<Json<serde_json::Value>> {
    let output = schedule_service_restart();

    match output {
        Ok(out) if out.status.success() => Ok(Json(json!({
            "ok": true,
            "message": "重启请求已提交，webclx.service 将在 1 秒后重启…"
        }))),
        Ok(out) => Err(AppError::internal(format!(
            "重启失败: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))),
        Err(e) => Err(AppError::internal(format!("重启请求失败: {e}"))),
    }
}

#[cfg(target_os = "linux")]
fn schedule_service_restart() -> std::io::Result<std::process::Output> {
    Command::new(RESTART_SCHEDULER_PROGRAM)
        .args(RESTART_SCHEDULER_ARGS)
        .output()
}

#[cfg(not(target_os = "linux"))]
fn schedule_service_restart() -> std::io::Result<std::process::Output> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "当前平台暂不支持通过 systemd 重启 webclx.service",
    ))
}

/// 保存会话并关机：先在 tmux server 与 codex/claude 子进程仍存活时把活动 agent
/// 会话的 resume 记录落盘，再调度系统关机。
///
/// 根因说明：webClx 的 graceful shutdown 保存逻辑依赖 tmux pane pid 与进程快照，
/// 而 systemd 关机默认会在 SIGTERM 后杀掉整个 cgroup（含 tmux server），导致保存
/// 读不到任何活动会话、恢复文件为空。这里把保存动作提前到关机流程之前、由用户
/// 显式触发，保证写盘时进程仍在，关机后下次启动能正常恢复。
pub async fn save_and_poweroff(
    State(state): State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    let saved = state
        .terminal_manager
        .save_shutdown_restore_registry()
        .map_err(|error| {
            tracing::warn!("save shutdown restore registry failed: {error}");
            AppError::internal(format!("保存会话失败：{error}"))
        })?;

    let output = schedule_system_poweroff();
    match output {
        Ok(out) if out.status.success() => Ok(Json(json!({
            "ok": true,
            "saved": saved,
            "message": format!("已保存 {saved} 个会话，系统将在 2 秒后关机…")
        }))),
        Ok(out) => {
            tracing::warn!(
                "schedule poweroff failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            Err(AppError::internal(format!(
                "会话已保存 {} 个，但关机请求失败：{}",
                saved,
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
        Err(e) => {
            tracing::warn!("schedule poweroff failed: {e}");
            Err(AppError::internal(format!("会话已保存 {} 个，但关机请求失败：{e}", saved)))
        }
    }
}

#[cfg(target_os = "linux")]
fn schedule_system_poweroff() -> std::io::Result<std::process::Output> {
    Command::new(POWEROFF_SCHEDULER_PROGRAM)
        .args(POWEROFF_SCHEDULER_ARGS)
        .output()
}

#[cfg(not(target_os = "linux"))]
fn schedule_system_poweroff() -> std::io::Result<std::process::Output> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "当前平台暂不支持通过 systemd 关机",
    ))
}

/// 保存会话并重启服务：先在 tmux server 与 codex/claude 子进程仍存活时把活动 agent
/// 会话的 resume 记录落盘，再调度 webclx.service 重启。
///
/// 与 `save_and_poweroff` 对称，区别在于只重启服务、不停整机。服务重启不会杀掉
/// tmux server，所以正常情况下 `restore_live_sessions` 会直接重连既有终端；但显式
/// 保存 resume 记录仍有兜底价值——若服务在重启过程中异常退出、或 tmux 会话因别的原因
/// 丢失，下一次启动仍能凭记录重建终端并发送 `codex resume <id>` / `claude --resume <id>`。
pub async fn save_and_restart_service(
    State(state): State<AppState>,
) -> ApiResult<Json<serde_json::Value>> {
    let saved = state
        .terminal_manager
        .save_shutdown_restore_registry()
        .map_err(|error| {
            tracing::warn!("save shutdown restore registry failed: {error}");
            AppError::internal(format!("保存会话失败：{error}"))
        })?;

    // 先落盘 resume 记录（依赖 tmux 与 codex/claude 子进程仍存活），再杀掉 webClx 的
    // tmux scope，使服务重启后走「从恢复记录重建终端」的完整路径——否则 systemd scope
    // 隔离会让 tmux 续命，重启只是重连、不会触发恢复链路。
    state.terminal_manager.stop_tmux_servers();

    let output = schedule_service_restart();
    match output {
        Ok(out) if out.status.success() => Ok(Json(json!({
            "ok": true,
            "saved": saved,
            "message": format!("已保存 {saved} 个会话，webclx.service 将在 1 秒后重启…")
        }))),
        Ok(out) => {
            tracing::warn!(
                "schedule restart failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            Err(AppError::internal(format!(
                "会话已保存 {} 个，但重启服务请求失败：{}",
                saved,
                String::from_utf8_lossy(&out.stderr).trim()
            )))
        }
        Err(e) => {
            tracing::warn!("schedule restart failed: {e}");
            Err(AppError::internal(format!("会话已保存 {} 个，但重启服务请求失败：{e}", saved)))
        }
    }
}

fn build_system_proxy_response(
    user_profile: &crate::runtime_paths::UserProfile,
) -> SystemProxyResponse {
    let process_env_pairs = collect_process_proxy_env_pairs();
    let process_env = process_env_pairs
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    let service_env_entries =
        read_proxy_env_file(Path::new(SERVICE_PROXY_ENV_FILE)).unwrap_or_default();
    let service_env_file = service_env_entries
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    let reference_env_file = read_proxy_env_file(Path::new(REFERENCE_PROXY_ENV_FILE))
        .unwrap_or_default()
        .into_iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    let (user_shell_env_entries, user_shell_env_file_path, user_shell_read_error) =
        read_user_shell_proxy_env(user_profile);
    let user_shell_env_file = user_shell_env_entries
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>();
    let restart_required =
        canonical_proxy_map(&process_env_pairs) != canonical_proxy_map(&service_env_entries);

    SystemProxyResponse {
        process_env,
        service_env_file: service_env_file.clone(),
        service_env_file_path: SERVICE_PROXY_ENV_FILE.to_string(),
        user_shell_env_file,
        user_shell_env_file_path: user_shell_env_file_path.clone(),
        user_shell_read_error: user_shell_read_error.clone(),
        reference_env_file,
        reference_env_file_path: REFERENCE_PROXY_ENV_FILE.to_string(),
        environment_file: service_env_file,
        environment_file_path: SERVICE_PROXY_ENV_FILE.to_string(),
        can_write: can_write_system_proxy_file(),
        restart_required,
        note: if restart_required {
            format!(
                "这里显示的是运行中的 webclx 进程环境和 {SERVICE_PROXY_ENV_FILE} 启动配置。当前两者不一致；修改 {SERVICE_PROXY_ENV_FILE} 后，需要重启 webclx.service 才会让运行中的服务读到新代理。另附通过执行当前设置用户 shell 启动过程得到的代理变量（入口文件 {user_shell_env_file_path}）作为 shell 参考；该环境不会自动注入 webclx.service。{REFERENCE_PROXY_ENV_FILE} 仅作为参考。"
            )
        } else {
            format!(
                "这里显示的是运行中的 webclx 进程环境和 {SERVICE_PROXY_ENV_FILE} 启动配置。修改 {SERVICE_PROXY_ENV_FILE} 后，需要重启 webclx.service 才会让运行中的服务读到新代理。另附通过执行当前设置用户 shell 启动过程得到的代理变量（入口文件 {user_shell_env_file_path}）作为 shell 参考；该环境不会自动注入 webclx.service。{REFERENCE_PROXY_ENV_FILE} 仅作为参考。"
            )
        },
    }
}

fn can_write_system_proxy_file() -> bool {
    cfg!(target_os = "linux") && current_uid() == 0
}

#[cfg(unix)]
fn current_uid() -> u32 {
    unsafe { libc::geteuid() }
}

#[cfg(not(unix))]
fn current_uid() -> u32 {
    0
}

#[cfg(unix)]
fn current_gid() -> u32 {
    unsafe { libc::getegid() }
}

#[cfg(not(unix))]
fn current_gid() -> u32 {
    0
}

fn collect_process_proxy_env_pairs() -> Vec<(String, String)> {
    SYSTEM_PROXY_KEYS
        .iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| ((*key).to_string(), value))
        })
        .collect()
}

fn read_user_shell_proxy_env(
    user_profile: &crate::runtime_paths::UserProfile,
) -> (Vec<(String, String)>, String, Option<String>) {
    let path = crate::shell_env::user_shell_init_file_path(user_profile);
    let path_text = path.display().to_string();

    match crate::shell_env::read_user_shell_env(user_profile) {
        Ok(snapshot) => (
            crate::shell_env::filter_env_entries(&snapshot.entries, &SYSTEM_PROXY_KEYS),
            snapshot.init_file_path.display().to_string(),
            None,
        ),
        Err(error) => (Vec::new(), path_text, Some(error.to_string())),
    }
}

fn read_proxy_env_file(path: &Path) -> anyhow::Result<Vec<(String, String)>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(path)?;
    let mut values = Vec::new();
    for line in content.lines() {
        if let Some((key, value)) = parse_env_assignment(line)
            && SYSTEM_PROXY_KEYS.contains(&key.as_str())
        {
            values.push((key, value));
        }
    }
    Ok(values)
}

fn write_system_proxy_file(payload: &SaveSystemProxyRequest) -> anyhow::Result<()> {
    if !cfg!(target_os = "linux") {
        anyhow::bail!("当前平台暂不支持写入服务代理环境变量");
    }
    if !can_write_system_proxy_file() {
        anyhow::bail!("当前进程没有写入 /etc/default/webclx 的权限");
    }

    let path = Path::new(SERVICE_PROXY_ENV_FILE);
    let existing = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    let desired = desired_system_proxy_map(payload);
    let mut seen = HashSet::new();
    let mut next_lines = Vec::new();

    for line in existing.lines() {
        match parse_env_assignment(line) {
            Some((key, _)) if SYSTEM_PROXY_KEYS.contains(&key.as_str()) => {
                if let Some(value) = desired.get(&key) {
                    next_lines.push(format!("{key}=\"{}\"", escape_env_value(value)));
                    seen.insert(key);
                }
            }
            _ => next_lines.push(line.to_string()),
        }
    }

    for key in SYSTEM_PROXY_KEYS {
        if let Some(value) = desired.get(key)
            && seen.insert(key.to_string())
        {
            next_lines.push(format!("{key}=\"{}\"", escape_env_value(value)));
        }
    }

    let mut output = next_lines.join("\n");
    if !output.is_empty() {
        output.push('\n');
    }
    std::fs::write(path, output.as_bytes())?;
    Ok(())
}

fn canonical_proxy_map(entries: &[(String, String)]) -> HashMap<String, String> {
    let mut map = HashMap::new();

    for (key, value) in entries {
        let canonical = match key.to_ascii_uppercase().as_str() {
            "HTTP_PROXY" => Some("HTTP_PROXY"),
            "HTTPS_PROXY" => Some("HTTPS_PROXY"),
            "ALL_PROXY" => Some("ALL_PROXY"),
            "NO_PROXY" => Some("NO_PROXY"),
            _ => None,
        };
        if let Some(canonical) = canonical {
            map.insert(canonical.to_string(), value.clone());
        }
    }

    map
}

fn desired_system_proxy_map(payload: &SaveSystemProxyRequest) -> HashMap<String, String> {
    let mut map = HashMap::new();

    if let Some(value) = normalize_optional_env(&payload.http_proxy) {
        map.insert("HTTP_PROXY".to_string(), value.clone());
        map.insert("http_proxy".to_string(), value);
    }
    if let Some(value) = normalize_optional_env(&payload.https_proxy) {
        map.insert("HTTPS_PROXY".to_string(), value.clone());
        map.insert("https_proxy".to_string(), value);
    }
    if let Some(value) = normalize_optional_env(&payload.all_proxy) {
        map.insert("ALL_PROXY".to_string(), value.clone());
        map.insert("all_proxy".to_string(), value);
    }
    if let Some(value) = normalize_optional_env(&payload.no_proxy) {
        map.insert("NO_PROXY".to_string(), value.clone());
        map.insert("no_proxy".to_string(), value);
    }

    map
}

fn normalize_optional_env(value: &Option<String>) -> Option<String> {
    value.as_ref().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn parse_env_assignment(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }

    let assignment = trimmed
        .strip_prefix("export ")
        .map(str::trim_start)
        .unwrap_or(trimmed);
    let (key, value) = assignment.split_once('=')?;
    let key = key.trim();
    if key.is_empty()
        || !key
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    {
        return None;
    }

    let value = value.trim();
    let value = if value.len() >= 2
        && ((value.starts_with('"') && value.ends_with('"'))
            || (value.starts_with('\'') && value.ends_with('\'')))
    {
        value[1..value.len() - 1].to_string()
    } else {
        value.to_string()
    };

    Some((key.to_string(), value))
}

fn escape_env_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_proxy_map_ignores_key_case() {
        let entries = vec![
            ("http_proxy".to_string(), "http://127.0.0.1:7890".to_string()),
            ("HTTPS_PROXY".to_string(), "http://127.0.0.1:7890".to_string()),
            ("all_proxy".to_string(), "socks5://127.0.0.1:7891".to_string()),
        ];

        let map = canonical_proxy_map(&entries);

        assert_eq!(map.get("HTTP_PROXY").map(String::as_str), Some("http://127.0.0.1:7890"));
        assert_eq!(map.get("HTTPS_PROXY").map(String::as_str), Some("http://127.0.0.1:7890"));
        assert_eq!(map.get("ALL_PROXY").map(String::as_str), Some("socks5://127.0.0.1:7891"));
    }

    #[test]
    fn read_proxy_env_file_filters_only_proxy_keys() {
        let path = std::env::temp_dir()
            .join(format!("webclx-system-proxy-test-{}.env", std::process::id()));
        std::fs::write(
            &path,
            "HTTP_PROXY=\"http://127.0.0.1:7890\"\nIGNORED_KEY=/home/demo\nno_proxy=localhost\n",
        )
        .expect("write temp env file");

        let entries = read_proxy_env_file(&path).expect("read temp env file");
        std::fs::remove_file(&path).expect("cleanup temp env file");

        assert_eq!(
            entries,
            vec![
                ("HTTP_PROXY".to_string(), "http://127.0.0.1:7890".to_string()),
                ("no_proxy".to_string(), "localhost".to_string()),
            ]
        );
    }

    #[test]
    fn read_proxy_env_file_supports_export_lines() {
        let path = std::env::temp_dir()
            .join(format!("webclx-system-proxy-export-test-{}.env", std::process::id()));
        std::fs::write(
            &path,
            "export HTTPS_PROXY=\"http://127.0.0.1:7890\"\nexport no_proxy=localhost,127.0.0.1\n",
        )
        .expect("write temp env file");

        let entries = read_proxy_env_file(&path).expect("read temp env file");
        std::fs::remove_file(&path).expect("cleanup temp env file");

        assert_eq!(
            entries,
            vec![
                ("HTTPS_PROXY".to_string(), "http://127.0.0.1:7890".to_string()),
                ("no_proxy".to_string(), "localhost,127.0.0.1".to_string()),
            ]
        );
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn restart_service_uses_delayed_transient_systemd_unit() {
        assert_eq!(RESTART_SCHEDULER_PROGRAM, "/usr/bin/systemd-run");
        assert_eq!(
            RESTART_SCHEDULER_ARGS,
            &[
                "--quiet",
                "--collect",
                "--on-active=1s",
                "/bin/systemctl",
                "restart",
                "webclx.service",
            ]
        );
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn restart_service_returns_unsupported_outside_linux() {
        let error = schedule_service_restart().expect_err("restart should be unsupported");
        assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
        assert!(error.to_string().contains("systemd"));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn save_and_poweroff_uses_delayed_transient_systemd_unit() {
        assert_eq!(POWEROFF_SCHEDULER_PROGRAM, "/usr/bin/systemd-run");
        assert_eq!(
            POWEROFF_SCHEDULER_ARGS,
            &[
                "--quiet",
                "--collect",
                "--on-active=2s",
                "/bin/systemctl",
                "poweroff",
            ]
        );
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn save_and_poweroff_returns_unsupported_outside_linux() {
        let error = schedule_system_poweroff().expect_err("poweroff should be unsupported");
        assert_eq!(error.kind(), std::io::ErrorKind::Unsupported);
        assert!(error.to_string().contains("systemd"));
    }
}
