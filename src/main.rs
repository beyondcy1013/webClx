#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod agent;
mod api_catalog;
mod artifacts;
mod auth;
mod auth_guard;
mod builtin_skills;
mod cli;
mod codex_conversation_model;
mod codex_launch;
mod codex_proxy;
mod codex_task;
mod compile_service;
mod config_files;
mod deploy_service;
mod filesystem;
mod frpc;
mod host;
mod instance_identity;
mod llm;
mod login;
mod preset_sync;
mod private_file;
mod proxy;
mod proxy_bridge;
mod quota;
mod quota_reset_cache;
mod routes;
mod runtime_paths;
mod settings;
mod shell_env;
mod startup_tools;
mod system;
mod terminal;
mod upstream_proxy;

use std::{
    env, fmt,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::{Component, Path, PathBuf},
    sync::Arc,
};

use axum::{
    extract::{Path as AxumPath, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Redirect, Response},
};
use include_dir::{Dir, include_dir};
use serde::Deserialize;
use tokio::net::TcpListener;
use tracing::{info, warn};

const NO_STORE_CACHE_CONTROL: &str = "no-store, max-age=0, must-revalidate";
const MAX_BIND_PORT_ATTEMPTS: u16 = 100;
static EMBEDDED_STATIC_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/static");

/// 获取 std::sync 锁的 guard，遇到 poison（持有者 panic）时恢复内层数据而非
/// panic 整个进程。单个请求的 panic 不应把配置/终端状态管理升级为全服务崩溃；
/// 恢复的 guard 可能读到部分写入的中间状态，但这对配置/会话存储而言严格优于宕机。
macro_rules! lock_or_recover {
    ($lock_expr:expr) => {
        match $lock_expr {
            Ok(guard) => guard,
            Err(poison) => poison.into_inner(),
        }
    };
}
pub(crate) use lock_or_recover;

#[derive(Clone)]
struct AppState {
    static_dir: PathBuf,
    listen_addr: SocketAddr,
    version: String,
    app_dir: PathBuf,
    local_api_token: Arc<str>,
    workspace_settings: settings::SettingsManager,
    auth_manager: auth::AuthPresetManager,
    codex_oauth_manager: auth::CodexOAuthManager,
    codex_proxy_history: codex_proxy::CodexProxyHistory,
    proxy_manager: proxy::ProxyManager,
    quota_reset_cache: quota_reset_cache::QuotaResetCache,
    quota_manager: quota::QuotaConfigManager,
    frpc_manager: frpc::FrpcManager,
    frps_manager: frpc::FrpsManager,
    frp_role_manager: frpc::FrpRoleManager,
    terminal_manager: terminal::TerminalManager,
    preset_test_scheduler: auth::PresetTestScheduler,
    preset_run_lease_manager: auth::PresetRunLeaseManager,
    agent_manager: agent::AgentManager,
    agent_config: agent::AgentConfigManager,
}

#[derive(Debug)]
struct AppError {
    status: StatusCode,
    message: String,
}

#[derive(Clone, Copy)]
struct EmbeddedStaticAsset {
    bytes: &'static [u8],
    content_type: &'static str,
}

type ApiResult<T> = Result<T, AppError>;

#[derive(Debug, Default, Deserialize)]
struct PathQuery {
    #[serde(default)]
    path: String,
}

impl AppError {
    fn bad_request(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: message.into(),
        }
    }

    fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.into(),
        }
    }

    fn conflict(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            message: message.into(),
        }
    }

    fn internal(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            message: message.into(),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        (self.status, self.message).into_response()
    }
}

impl AppState {
    fn workspace_root(&self) -> PathBuf {
        self.workspace_settings.current_root()
    }

    fn workspace_display_root(&self) -> PathBuf {
        self.workspace_settings.display_root()
    }

    fn show_dot_entries(&self) -> bool {
        self.workspace_settings.show_dot_entries()
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli_action = cli::parse_process_args().map_err(anyhow::Error::msg)?;
    if !matches!(cli_action, cli::CliAction::Serve) {
        return cli::execute(cli_action).await;
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "webclx=info,tower_http=info".into()),
        )
        .init();

    let app_dir = env::current_dir()?.canonicalize()?;
    login::initialize_session_secret(&app_dir)
        .map_err(|error| anyhow::anyhow!("初始化会话密钥失败: {error}"))?;
    login::initialize_credentials(&app_dir)
        .map_err(|error| anyhow::anyhow!("初始化登录凭据失败: {error}"))?;
    let local_api_token = auth_guard::load_or_create_local_api_token(&app_dir)
        .map_err(|error| anyhow::anyhow!("初始化本地 API 令牌失败: {error}"))?;
    startup_tools::spawn_startup_tool_bootstrap(app_dir.clone());
    let requested_addr = configured_listen_addr()?;
    let workspace_settings = settings::SettingsManager::load(&app_dir)?;
    let terminal_user_profile = workspace_settings
        .terminal_user_profile()
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    builtin_skills::install_for_user(&app_dir, &terminal_user_profile)
        .map_err(|error| anyhow::anyhow!("安装内置 Skills 失败: {error}"))?;
    let (listener, listen_addr) =
        bind_server_listener(requested_addr, workspace_settings.server_port_auto_increment())
            .await?;
    auth_core::set_local_webclx_origin(format!("http://127.0.0.1:{}", listen_addr.port()));
    let auth_manager = auth::AuthPresetManager::load(&app_dir)?;
    let proxy_manager = proxy::ProxyManager::load(&app_dir)?;
    let https_proxy_bridge_addr =
        proxy_bridge::spawn_https_proxy_bridge(proxy_manager.clone(), listen_addr).await?;
    proxy_manager.set_https_proxy_bridge_addr(https_proxy_bridge_addr);
    let quota_manager = quota::QuotaConfigManager::load(&app_dir);
    let frpc_manager = frpc::FrpcManager::load(&app_dir, listen_addr.port())?;
    let frps_manager = frpc::FrpsManager::load(&app_dir)?;
    let frp_role_manager = frpc::FrpRoleManager::load(&app_dir, listen_addr.port())?;
    let workspace_root = workspace_settings.current_root();
    let workspace_display_root = workspace_settings.display_root();
    let static_dir = resolve_static_dir(&app_dir);
    let terminal_env_snapshot = terminal::TerminalEnvironmentSnapshot {
        workspace_root: workspace_root.clone(),
        display_root: workspace_display_root.clone(),
        user_profile: terminal_user_profile,
        terminal_default_env: workspace_settings.terminal_default_env_entries(),
        proxy_env: proxy_manager.get_terminal_proxy_env(),
    };

    let quota_reset_cache = quota_reset_cache::QuotaResetCache::new();
    let state = AppState {
        static_dir: static_dir.clone(),
        listen_addr,
        version: env!("CARGO_PKG_VERSION").to_string(),
        app_dir: app_dir.clone(),
        local_api_token: Arc::from(local_api_token),
        workspace_settings,
        auth_manager,
        codex_oauth_manager: auth::CodexOAuthManager::new(),
        codex_proxy_history: codex_proxy::CodexProxyHistory::new(),
        proxy_manager,
        quota_reset_cache: quota_reset_cache.clone(),
        quota_manager,
        frpc_manager,
        frps_manager,
        frp_role_manager,
        terminal_manager: terminal::TerminalManager::new_with_environment_deferred_restore(
            app_dir.join(".webclx-terminal-sessions.json"),
            terminal_env_snapshot,
            quota_reset_cache,
        ),
        preset_test_scheduler: auth::PresetTestScheduler::new(
            &app_dir.join(".webclx-terminal-sessions.json"),
        ),
        preset_run_lease_manager: auth::PresetRunLeaseManager::new(
            app_dir.join(".webclx-preset-run-lease.json"),
        ),
        agent_manager: agent::AgentManager::new(&app_dir),
        agent_config: agent::AgentConfigManager::new(&app_dir),
    };
    artifacts::enforce_artifact_retention(&state.app_dir)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    auth::recover_stale_preset_run_lease(&state)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    codex_task::recover_interrupted_tasks(&state).await;
    state.terminal_manager.spawn_error_auto_continue_runner(
        state.workspace_settings.clone(),
        state.auth_manager.clone(),
    );
    state.preset_test_scheduler.spawn_runner(
        state.auth_manager.clone(),
        state.proxy_manager.clone(),
        state.workspace_settings.clone(),
    );

    let app = routes::app(state.clone());

    info!("app dir: {}", app_dir.display());
    if workspace_display_root == workspace_root {
        info!("workspace root: {}", workspace_root.display());
    } else {
        info!(
            "workspace root: {} (canonical: {})",
            workspace_display_root.display(),
            workspace_root.display()
        );
    }
    if static_dir.join("index.html").is_file() {
        info!("static dir: {}", static_dir.display());
    } else {
        warn!(
            "static dir {} is missing; embedded static assets will be used",
            static_dir.display()
        );
    }
    info!("server listening on http://{}", listen_addr);

    let shutdown_manager = Arc::new(state.terminal_manager.clone());
    if let Err(error) =
        axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(shutdown_signal(shutdown_manager))
            .await
    {
        warn!("server stopped: {error}");
    }

    Ok(())
}

async fn shutdown_signal(terminal_manager: Arc<terminal::TerminalManager>) {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};
        if let Ok(mut signal) = signal(SignalKind::terminate()) {
            let _ = signal.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    terminal_manager.finalize_output_observations_for_shutdown();
}

fn configured_listen_addr() -> anyhow::Result<SocketAddr> {
    let bind_addr = env::var("WEBCLX_ADDR").unwrap_or_else(|_| "0.0.0.0:11111".to_string());
    let parsed: SocketAddr = bind_addr
        .parse()
        .map_err(|error| anyhow::anyhow!("WEBCLX_ADDR `{bind_addr}` is invalid: {error}"))?;
    Ok(force_unspecified_listen_host(parsed))
}

fn force_unspecified_listen_host(addr: SocketAddr) -> SocketAddr {
    if !addr.ip().is_unspecified() {
        warn!("WEBCLX_ADDR host {} is ignored; webClx listens on 0.0.0.0", addr.ip());
    }
    SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), addr.port())
}

async fn bind_server_listener(
    preferred_addr: SocketAddr,
    port_auto_increment: bool,
) -> anyhow::Result<(TcpListener, SocketAddr)> {
    let attempts = if port_auto_increment {
        MAX_BIND_PORT_ATTEMPTS
    } else {
        1
    };
    let mut last_error = None;
    let mut last_addr = preferred_addr;

    for offset in 0..attempts {
        let Some(port) = preferred_addr.port().checked_add(offset) else {
            break;
        };
        let candidate = SocketAddr::new(preferred_addr.ip(), port);
        match TcpListener::bind(candidate).await {
            Ok(listener) => {
                let bound_addr = listener.local_addr().unwrap_or(candidate);
                if candidate != preferred_addr {
                    warn!(
                        "preferred listen address {} was in use; using {}",
                        preferred_addr, bound_addr
                    );
                }
                return Ok((listener, bound_addr));
            }
            Err(error) if port_auto_increment && error.kind() == std::io::ErrorKind::AddrInUse => {
                warn!("listen address {} is in use; trying next port", candidate);
                last_addr = candidate;
                last_error = Some(error);
            }
            Err(error) => {
                return Err(anyhow::anyhow!("failed to bind {candidate}: {error}"));
            }
        }
    }

    let message = match last_error {
        Some(error) => format!(
            "failed to bind {} or the next {} ports; last attempt {} failed: {}",
            preferred_addr, attempts, last_addr, error
        ),
        None => format!("failed to bind {preferred_addr}: no valid port candidates"),
    };
    Err(anyhow::anyhow!(message))
}

fn resolve_static_dir(app_dir: &Path) -> PathBuf {
    let mut candidates = Vec::new();

    if let Some(path) = env::var_os("WEBCLX_STATIC_DIR")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
    {
        push_unique_path(&mut candidates, path);
    }

    push_unique_path(&mut candidates, app_dir.join("static"));

    if let Ok(exe_path) = env::current_exe()
        && let Some(exe_dir) = exe_path.parent()
    {
        for ancestor in exe_dir.ancestors().take(4) {
            push_unique_path(&mut candidates, ancestor.join("static"));
        }
    }

    candidates
        .into_iter()
        .find(|candidate| {
            candidate.join("index.html").is_file() && candidate.join("app.js").is_file()
        })
        .unwrap_or_else(|| app_dir.join("static"))
}

fn push_unique_path(paths: &mut Vec<PathBuf>, candidate: PathBuf) {
    if !paths.iter().any(|existing| existing == &candidate) {
        paths.push(candidate);
    }
}

async fn index_page(State(state): State<AppState>) -> Response {
    html_page_response(state.static_dir.join("index.html"), include_str!("../static/index.html"))
        .await
}

async fn spa_fallback(State(state): State<AppState>, uri: axum::http::Uri) -> Response {
    let path = uri.path();
    // Real backend/asset namespaces must keep 404-ing; only UI paths get the shell.
    if path.starts_with("/api/") || path.starts_with("/assets/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    index_page(State(state)).await
}

async fn login_page(State(state): State<AppState>) -> Response {
    html_page_response(state.static_dir.join("login.html"), include_str!("../static/login.html"))
        .await
}

async fn terminal_page(State(state): State<AppState>, Query(query): Query<PathQuery>) -> Response {
    match filesystem::resolve_directory_path(&state.workspace_root(), &query.path) {
        Ok(_) => {
            html_page_response(
                state.static_dir.join("terminal.html"),
                include_str!("../static/terminal.html"),
            )
            .await
        }
        Err(_) if !query.path.trim().is_empty() => Redirect::to("/terminal").into_response(),
        Err(error) => error.into_response(),
    }
}

async fn static_asset(
    State(state): State<AppState>,
    AxumPath(asset_path): AxumPath<String>,
) -> Response {
    let Some(normalized_path) = normalize_static_asset_path(&asset_path) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let normalized_path = if normalized_path == Path::new("favicon.svg") {
        PathBuf::from(instance_identity::icon_path(&host::current_host_name()))
    } else {
        normalized_path
    };
    let normalized_asset_path = normalized_path.to_string_lossy().replace('\\', "/");
    let disk_path = state.static_dir.join(&normalized_path);

    match tokio::fs::read(&disk_path).await {
        Ok(contents) => {
            return static_asset_response(
                contents,
                static_asset_content_type(&normalized_asset_path),
            );
        }
        Err(error) => {
            if let Some(asset) = embedded_static_asset(&normalized_asset_path) {
                warn!(
                    "failed to read {} from disk: {error}; serving embedded fallback",
                    disk_path.display()
                );
                return static_asset_response(asset.bytes.to_vec(), asset.content_type);
            }

            if error.kind() != std::io::ErrorKind::NotFound {
                warn!("failed to read {} from disk: {error}", disk_path.display());
            }
        }
    }

    StatusCode::NOT_FOUND.into_response()
}

fn normalize_static_asset_path(raw: &str) -> Option<PathBuf> {
    let trimmed = raw.trim_start_matches('/');
    if trimmed.is_empty() || trimmed.contains('\\') {
        return None;
    }

    let mut normalized = PathBuf::new();
    for component in Path::new(trimmed).components() {
        match component {
            Component::Normal(part) => normalized.push(part),
            Component::CurDir => {}
            _ => return None,
        }
    }

    (!normalized.as_os_str().is_empty()).then_some(normalized)
}

fn embedded_static_asset(asset_path: &str) -> Option<EmbeddedStaticAsset> {
    let file = EMBEDDED_STATIC_DIR.get_file(asset_path)?;
    Some(EmbeddedStaticAsset {
        bytes: file.contents(),
        content_type: static_asset_content_type(asset_path),
    })
}

fn static_asset_content_type(asset_path: &str) -> &'static str {
    if asset_path.ends_with(".css") {
        "text/css; charset=utf-8"
    } else if asset_path.ends_with(".html") {
        "text/html; charset=utf-8"
    } else if asset_path.ends_with(".js") {
        "text/javascript; charset=utf-8"
    } else if asset_path.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

fn static_asset_response(contents: Vec<u8>, content_type: &'static str) -> Response {
    let mut response = contents.into_response();
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static(NO_STORE_CACHE_CONTROL));
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    response
}

async fn html_page_response(path: PathBuf, fallback: &'static str) -> Response {
    let contents = match tokio::fs::read_to_string(&path).await {
        Ok(contents) => contents,
        Err(error) => {
            warn!("failed to read {} from disk: {error}", path.display());
            fallback.to_string()
        }
    };
    let contents = inject_host_name_into_title(contents);
    ([(header::CACHE_CONTROL, NO_STORE_CACHE_CONTROL)], Html(contents)).into_response()
}

fn inject_host_name_into_title(contents: String) -> String {
    let Some(title_start) = contents.find("<title>") else {
        return contents;
    };
    let title_content_start = title_start + "<title>".len();
    let Some(title_end_offset) = contents[title_content_start..].find("</title>") else {
        return contents;
    };
    let title_content_end = title_content_start + title_end_offset;
    let title = &contents[title_content_start..title_content_end];
    let updated_title = format!("{title} - {}", host::current_host_name());

    let mut next = String::with_capacity(contents.len() + updated_title.len() + 3);
    next.push_str(&contents[..title_content_start]);
    next.push_str(&updated_title);
    next.push_str(&contents[title_content_end..]);
    next
}

#[cfg(test)]
mod tests {
    use super::{embedded_static_asset, normalize_static_asset_path, static_asset_content_type};
    use std::path::PathBuf;

    #[test]
    fn normalize_static_asset_path_accepts_nested_assets() {
        assert_eq!(
            normalize_static_asset_path("/vendor/xterm.js"),
            Some(PathBuf::from("vendor/xterm.js"))
        );
    }

    #[test]
    fn normalize_static_asset_path_rejects_traversal() {
        assert_eq!(normalize_static_asset_path("../app.js"), None);
        assert_eq!(normalize_static_asset_path("vendor/../app.js"), None);
        assert_eq!(normalize_static_asset_path("vendor\\xterm.js"), None);
    }

    #[test]
    fn embedded_assets_include_page_dependencies() {
        assert!(embedded_static_asset("app.js").is_some());
        assert!(embedded_static_asset("styles.css").is_some());
        assert!(embedded_static_asset("terminal-cursor-guard.js").is_some());
        assert!(embedded_static_asset("terminal-ime-policy.js").is_some());
        assert!(embedded_static_asset("terminal-resume-extract.js").is_some());
        assert!(embedded_static_asset("terminal-selection-geometry.js").is_some());
        assert!(embedded_static_asset("terminal-session-activity.js").is_some());
        assert!(embedded_static_asset("terminal-session-storage.js").is_some());
        assert!(embedded_static_asset("terminal-settings.js").is_some());
        assert!(embedded_static_asset("workspace-project-icons.js").is_some());
        for index in 0..crate::instance_identity::ICON_COUNT {
            assert!(embedded_static_asset(&format!("favicon-{index}.svg")).is_some());
        }
        assert!(embedded_static_asset("terminal-touch-selection-policy.js").is_some());
        assert!(embedded_static_asset("vendor/xterm.js").is_some());
    }

    #[test]
    fn terminal_tools_full_access_controls_are_embedded() {
        let terminal_html =
            embedded_static_asset("terminal.html").expect("terminal.html should be embedded");
        let html =
            std::str::from_utf8(terminal_html.bytes).expect("terminal.html should be valid UTF-8");
        let terminal_tools_js = embedded_static_asset("terminal-tools.js")
            .expect("terminal-tools.js should be embedded");
        let tools_js = std::str::from_utf8(terminal_tools_js.bytes)
            .expect("terminal-tools.js should be valid UTF-8");
        let terminal_mobile_keys_js = embedded_static_asset("terminal-mobile-keys.js")
            .expect("terminal-mobile-keys.js should be embedded");
        let mobile_keys_js = std::str::from_utf8(terminal_mobile_keys_js.bytes)
            .expect("terminal-mobile-keys.js should be valid UTF-8");
        let tools_position = html
            .find("id=\"terminal-tools-button\"")
            .expect("terminal tools button should exist");
        let keyboard_position = html
            .find("id=\"terminal-mobile-keys\"")
            .expect("terminal mobile keyboard should exist");
        let fab_position = html
            .find("id=\"terminal-fab\"")
            .expect("terminal FAB should exist");
        let menu_start = html
            .find("id=\"terminal-tools-menu\"")
            .expect("terminal tools dropdown menu should exist");
        let menu_end = html[menu_start..]
            .find("</div>")
            .map(|offset| menu_start + offset)
            .expect("terminal tools dropdown menu should close");
        let menu = &html[menu_start..menu_end];

        assert!(keyboard_position < tools_position);
        assert!(tools_position < fab_position);
        assert!(fab_position < menu_start);
        assert!(!html.contains("data-action=\"paste_clipboard\""));
        assert!(menu.contains("id=\"terminal-copy-all\""));
        assert!(menu.contains("data-action=\"copy_all_text\""));
        assert!(!html.contains("id=\"terminal-tools-dialog\""));
        assert!(!html.contains("id=\"terminal-tools-panel\""));
        assert!(html.contains("aria-controls=\"terminal-tools-menu\""));
        assert!(html.contains("aria-haspopup=\"menu\""));
        assert!(html.contains("aria-expanded=\"false\""));
        for id in [
            "session-detail-toggle",
            "session-agent-toggle",
            "session-auto-continue-toggle",
            "terminal-codex-full-access-toggle",
        ] {
            assert_eq!(html.matches(&format!("id=\"{id}\"")).count(), 1);
            assert!(menu.contains(&format!("id=\"{id}\"")));
        }
        assert!(menu.contains("role=\"menu\""));
        assert!(menu.contains("role=\"switch\""));
        assert!(menu.contains("type=\"checkbox\""));
        assert!(!html.contains("terminal-codex-full-access-start"));
        assert!(embedded_static_asset("terminal-tools.js").is_some());
        assert!(tools_js.contains("terminalToolsRestoringTriggerFocus = true"));
        assert!(
            mobile_keys_js
                .contains("target === terminalToolsButtonEl && terminalToolsRestoringTriggerFocus")
        );
    }

    #[test]
    fn static_asset_content_type_uses_browser_types() {
        assert_eq!(static_asset_content_type("terminal.js"), "text/javascript; charset=utf-8");
        assert_eq!(static_asset_content_type("favicon.svg"), "image/svg+xml");
    }
}
