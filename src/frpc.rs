use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use tar::Archive;
use tokio::process::{Child, Command};
use toml_edit::{DocumentMut, Item};
use tracing::{info, warn};
use zip::ZipArchive;

mod routes;
mod types;

pub use routes::{
    adopt_system_frp, delete_frp_role, discover_system_frp, download_frp_role_binary,
    download_frpc_binary, download_frps_binary, get_frpc_status, get_frps_status, list_frp_roles,
    restart_frp_role, restart_frpc, restart_frps, save_frp_role, save_frpc_config,
    save_frps_config, start_frp_role, start_frpc, start_frps, stop_frp_role, stop_frpc, stop_frps,
    test_frp_port, unmanage_frp_role,
};
pub use types::*;

const FRPC_DIR_NAME: &str = ".webclx-frpc";
const FRPC_CONFIG_JSON: &str = "frpc-config.json";
const FRPC_TOML: &str = "frpc.toml";
const FRPC_LOG: &str = "frpc.log";
const FRPS_DIR_NAME: &str = ".webclx-frps";
const FRPS_CONFIG_JSON: &str = "frps-config.json";
const FRPS_TOML: &str = "frps.toml";
const FRPS_LOG: &str = "frps.log";
const FRP_ROLES_DIR_NAME: &str = ".webclx-frp";
const FRP_ROLES_JSON: &str = "roles.json";
const FRP_RELEASE_LATEST_URL: &str = "https://api.github.com/repos/fatedier/frp/releases/latest";
const FRP_USER_AGENT: &str = "webClx-frp-downloader";
const LOG_TAIL_BYTES: u64 = 16 * 1024;

#[derive(Clone)]
pub struct FrpcManager {
    inner: Arc<Mutex<FrpcRuntime>>,
    dir: Arc<PathBuf>,
    config_path: Arc<PathBuf>,
    generated_config_path: Arc<PathBuf>,
    log_path: Arc<PathBuf>,
}

#[derive(Clone)]
pub struct FrpsManager {
    inner: Arc<Mutex<FrpcRuntime>>,
    dir: Arc<PathBuf>,
    config_path: Arc<PathBuf>,
    generated_config_path: Arc<PathBuf>,
    log_path: Arc<PathBuf>,
}

#[derive(Clone)]
pub struct FrpRoleManager {
    inner: Arc<Mutex<HashMap<String, FrpcRuntime>>>,
    dir: Arc<PathBuf>,
    roles_path: Arc<PathBuf>,
    legacy_frpc_dir: Arc<PathBuf>,
    legacy_frps_dir: Arc<PathBuf>,
}

struct FrpcRuntime {
    child: Option<Child>,
    started_at: Option<u64>,
    last_error: Option<String>,
}

#[derive(Debug, Clone)]
struct DetectedFrpProcess {
    pid: u32,
    component: FrpComponent,
    binary_path: Option<PathBuf>,
    config_path: Option<PathBuf>,
    command: String,
}

impl FrpcManager {
    pub fn load(app_dir: &Path, default_local_port: u16) -> Result<Self> {
        let dir = app_dir.join(FRPC_DIR_NAME);
        fs::create_dir_all(&dir)
            .with_context(|| format!("cannot create frpc runtime dir {}", dir.display()))?;
        let manager = Self {
            inner: Arc::new(Mutex::new(FrpcRuntime {
                child: None,
                started_at: None,
                last_error: None,
            })),
            config_path: Arc::new(dir.join(FRPC_CONFIG_JSON)),
            generated_config_path: Arc::new(dir.join(FRPC_TOML)),
            log_path: Arc::new(dir.join(FRPC_LOG)),
            dir: Arc::new(dir),
        };
        if !manager.config_path.exists() {
            manager.persist_config(&FrpcConfig::default_for_local_port(default_local_port))?;
        }
        Ok(manager)
    }

    pub fn config(&self) -> FrpcConfig {
        load_frpc_config(&self.config_path).unwrap_or_else(|error| {
            warn!("load frpc config failed: {error}");
            FrpcConfig::default()
        })
    }

    pub fn persist_config(&self, config: &FrpcConfig) -> Result<()> {
        fs::create_dir_all(self.dir.as_path())?;
        let encoded = serde_json::to_vec_pretty(config)?;
        fs::write(self.config_path.as_path(), encoded)
            .with_context(|| format!("cannot write {}", self.config_path.display()))?;
        Ok(())
    }

    pub async fn start(&self) -> Result<()> {
        self.reap_finished_child();
        if self.is_running() {
            return Ok(());
        }

        let config = self.config();
        if !config.enabled {
            anyhow::bail!("frpc 未启用");
        }
        validate_frpc_config(&config)?;
        let binary_path = self.resolve_binary(&config)?;
        let toml = render_frpc_toml(&config);
        fs::write(self.generated_config_path.as_path(), toml)
            .with_context(|| format!("cannot write {}", self.generated_config_path.display()))?;

        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_path.as_path())
            .with_context(|| format!("cannot open {}", self.log_path.display()))?;
        let log_file_err = log_file.try_clone().context("cannot clone frpc log file")?;
        append_log_line(&self.log_path, "starting frpc")?;

        let child = Command::new(&binary_path)
            .arg("-c")
            .arg(self.generated_config_path.as_path())
            .current_dir(self.dir.as_path())
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_file_err))
            .kill_on_drop(false)
            .spawn()
            .with_context(|| format!("启动 frpc 失败: {}", binary_path.display()))?;

        let pid = child.id();
        let mut runtime = crate::lock_or_recover!(self.inner.lock());
        runtime.child = Some(child);
        runtime.started_at = Some(unix_now_secs());
        runtime.last_error = None;
        info!("frpc started: pid={pid:?}");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        let mut child = {
            let mut runtime = crate::lock_or_recover!(self.inner.lock());
            runtime.started_at = None;
            runtime.child.take()
        };
        if let Some(mut child) = child.take() {
            append_log_line(&self.log_path, "stopping frpc")?;
            if let Err(error) = child.kill().await {
                warn!("kill frpc failed: {error}");
            }
            let _ = child.wait().await;
        }
        Ok(())
    }

    pub async fn restart(&self) -> Result<()> {
        self.stop().await?;
        self.start().await
    }

    pub fn status(&self) -> FrpcStatusResponse {
        self.reap_finished_child();
        let config = self.config();
        let configured = validate_frpc_config(&config).is_ok();
        let binary_path = self.resolve_binary(&config).ok();
        let (running, pid, started_at, last_error) = {
            let runtime = crate::lock_or_recover!(self.inner.lock());
            (
                runtime.child.is_some(),
                runtime.child.as_ref().and_then(Child::id),
                runtime.started_at,
                runtime.last_error.clone(),
            )
        };
        FrpcStatusResponse {
            configured,
            running,
            pid,
            started_at,
            binary_path: binary_path.map(|path| path.display().to_string()),
            config_path: self.config_path.display().to_string(),
            generated_config_path: self.generated_config_path.display().to_string(),
            log_path: self.log_path.display().to_string(),
            last_error,
            config,
            log_tail: read_log_tail(&self.log_path).unwrap_or_default(),
            download_platform: current_frp_platform().ok(),
        }
    }

    fn is_running(&self) -> bool {
        crate::lock_or_recover!(self.inner.lock()).child.is_some()
    }

    fn reap_finished_child(&self) {
        let mut runtime = crate::lock_or_recover!(self.inner.lock());
        let Some(child) = runtime.child.as_mut() else {
            return;
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                let message = format!("frpc 已退出: {status}");
                runtime.child = None;
                runtime.started_at = None;
                runtime.last_error = Some(message.clone());
                let _ = append_log_line(&self.log_path, &message);
            }
            Ok(None) => {}
            Err(error) => {
                let message = format!("检查 frpc 状态失败: {error}");
                runtime.last_error = Some(message);
            }
        }
    }

    fn resolve_binary(&self, config: &FrpcConfig) -> Result<PathBuf> {
        resolve_frp_binary(
            FrpComponent::Frpc,
            &config.binary_source,
            &config.binary_path,
            self.dir.as_path(),
            None,
        )
    }

    pub async fn download_binary(&self) -> Result<FrpDownloadResponse> {
        self.reap_finished_child();
        if self.is_running() {
            anyhow::bail!("frpc 正在运行，请先停止再下载或更新二进制");
        }
        download_frp_binary(FrpComponent::Frpc, self.dir.as_path()).await
    }
}

impl FrpsManager {
    pub fn load(app_dir: &Path) -> Result<Self> {
        let dir = app_dir.join(FRPS_DIR_NAME);
        fs::create_dir_all(&dir)
            .with_context(|| format!("cannot create frps runtime dir {}", dir.display()))?;
        let manager = Self {
            inner: Arc::new(Mutex::new(FrpcRuntime {
                child: None,
                started_at: None,
                last_error: None,
            })),
            config_path: Arc::new(dir.join(FRPS_CONFIG_JSON)),
            generated_config_path: Arc::new(dir.join(FRPS_TOML)),
            log_path: Arc::new(dir.join(FRPS_LOG)),
            dir: Arc::new(dir),
        };
        if !manager.config_path.exists() {
            manager.persist_config(&FrpsConfig::default())?;
        }
        Ok(manager)
    }

    pub fn config(&self) -> FrpsConfig {
        load_frps_config(&self.config_path).unwrap_or_else(|error| {
            warn!("load frps config failed: {error}");
            FrpsConfig::default()
        })
    }

    pub fn persist_config(&self, config: &FrpsConfig) -> Result<()> {
        fs::create_dir_all(self.dir.as_path())?;
        let encoded = serde_json::to_vec_pretty(config)?;
        fs::write(self.config_path.as_path(), encoded)
            .with_context(|| format!("cannot write {}", self.config_path.display()))?;
        Ok(())
    }

    pub async fn start(&self) -> Result<()> {
        self.reap_finished_child();
        if self.is_running() {
            return Ok(());
        }

        let config = self.config();
        if !config.enabled {
            anyhow::bail!("frps 未启用");
        }
        validate_frps_config(&config)?;
        let binary_path = self.resolve_binary(&config)?;
        let toml = render_frps_toml(&config);
        fs::write(self.generated_config_path.as_path(), toml)
            .with_context(|| format!("cannot write {}", self.generated_config_path.display()))?;

        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.log_path.as_path())
            .with_context(|| format!("cannot open {}", self.log_path.display()))?;
        let log_file_err = log_file.try_clone().context("cannot clone frps log file")?;
        append_log_line(&self.log_path, "starting frps")?;

        let child = Command::new(&binary_path)
            .arg("-c")
            .arg(self.generated_config_path.as_path())
            .current_dir(self.dir.as_path())
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_file_err))
            .kill_on_drop(false)
            .spawn()
            .with_context(|| format!("启动 frps 失败: {}", binary_path.display()))?;

        let pid = child.id();
        let mut runtime = crate::lock_or_recover!(self.inner.lock());
        runtime.child = Some(child);
        runtime.started_at = Some(unix_now_secs());
        runtime.last_error = None;
        info!("frps started: pid={pid:?}");
        Ok(())
    }

    pub async fn stop(&self) -> Result<()> {
        let mut child = {
            let mut runtime = crate::lock_or_recover!(self.inner.lock());
            runtime.started_at = None;
            runtime.child.take()
        };
        if let Some(mut child) = child.take() {
            append_log_line(&self.log_path, "stopping frps")?;
            if let Err(error) = child.kill().await {
                warn!("kill frps failed: {error}");
            }
            let _ = child.wait().await;
        }
        Ok(())
    }

    pub async fn restart(&self) -> Result<()> {
        self.stop().await?;
        self.start().await
    }

    pub fn status(&self) -> FrpsStatusResponse {
        self.reap_finished_child();
        let config = self.config();
        let configured = validate_frps_config(&config).is_ok();
        let binary_path = self.resolve_binary(&config).ok();
        let (running, pid, started_at, last_error) = {
            let runtime = crate::lock_or_recover!(self.inner.lock());
            (
                runtime.child.is_some(),
                runtime.child.as_ref().and_then(Child::id),
                runtime.started_at,
                runtime.last_error.clone(),
            )
        };
        FrpsStatusResponse {
            configured,
            running,
            pid,
            started_at,
            binary_path: binary_path.map(|path| path.display().to_string()),
            config_path: self.config_path.display().to_string(),
            generated_config_path: self.generated_config_path.display().to_string(),
            log_path: self.log_path.display().to_string(),
            last_error,
            config,
            log_tail: read_log_tail(&self.log_path).unwrap_or_default(),
            download_platform: current_frp_platform().ok(),
        }
    }

    fn is_running(&self) -> bool {
        crate::lock_or_recover!(self.inner.lock()).child.is_some()
    }

    fn reap_finished_child(&self) {
        let mut runtime = crate::lock_or_recover!(self.inner.lock());
        let Some(child) = runtime.child.as_mut() else {
            return;
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                let message = format!("frps 已退出: {status}");
                runtime.child = None;
                runtime.started_at = None;
                runtime.last_error = Some(message.clone());
                let _ = append_log_line(&self.log_path, &message);
            }
            Ok(None) => {}
            Err(error) => {
                let message = format!("检查 frps 状态失败: {error}");
                runtime.last_error = Some(message);
            }
        }
    }

    fn resolve_binary(&self, config: &FrpsConfig) -> Result<PathBuf> {
        resolve_frp_binary(
            FrpComponent::Frps,
            &config.binary_source,
            &config.binary_path,
            self.dir.as_path(),
            None,
        )
    }

    pub async fn download_binary(&self) -> Result<FrpDownloadResponse> {
        self.reap_finished_child();
        if self.is_running() {
            anyhow::bail!("frps 正在运行，请先停止再下载或更新二进制");
        }
        download_frp_binary(FrpComponent::Frps, self.dir.as_path()).await
    }
}

impl FrpRoleManager {
    pub fn load(app_dir: &Path, default_local_port: u16) -> Result<Self> {
        let dir = app_dir.join(FRP_ROLES_DIR_NAME);
        fs::create_dir_all(&dir)
            .with_context(|| format!("cannot create frp roles dir {}", dir.display()))?;
        let manager = Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            roles_path: Arc::new(dir.join(FRP_ROLES_JSON)),
            legacy_frpc_dir: Arc::new(app_dir.join(FRPC_DIR_NAME)),
            legacy_frps_dir: Arc::new(app_dir.join(FRPS_DIR_NAME)),
            dir: Arc::new(dir),
        };
        if !manager.roles_path.exists() {
            manager.persist_roles(&manager.default_roles(default_local_port)?)?;
        }
        Ok(manager)
    }

    pub fn roles(&self) -> Vec<FrpRole> {
        load_frp_roles(&self.roles_path).unwrap_or_else(|error| {
            warn!("load frp roles failed: {error}");
            Vec::new()
        })
    }

    pub fn status(&self) -> FrpRolesResponse {
        let roles = self.roles();
        for role in &roles {
            self.reap_finished_child(&role.id, role.component);
        }
        FrpRolesResponse {
            roles: roles
                .into_iter()
                .map(|role| self.role_status(role))
                .collect(),
            download_platform: current_frp_platform().ok(),
        }
    }

    pub fn save_role(&self, role: FrpRole) -> Result<FrpRolesResponse> {
        let role = normalize_frp_role(role);
        validate_frp_role(&role)?;
        let mut roles = self.roles();
        if let Some(existing) = roles.iter_mut().find(|item| item.id == role.id) {
            *existing = role;
        } else {
            roles.push(role);
        }
        self.persist_roles(&roles)?;
        Ok(self.status())
    }

    pub async fn delete_role(&self, id: &str) -> Result<FrpRolesResponse> {
        let role = self.role_by_id(id)?;
        self.stop_role(&role.id).await?;
        let mut roles = self.roles();
        roles.retain(|item| item.id != role.id);
        self.persist_roles(&roles)?;
        Ok(self.status())
    }

    pub fn unmanage_role(&self, id: &str) -> Result<FrpRolesResponse> {
        let role = self.role_by_id(id)?;
        if self.is_running(&role.id) {
            anyhow::bail!(
                "角色 `{}` 由 webClx 启动，不能直接取消接管；请先停止或删除角色",
                role.name
            );
        }
        let mut roles = self.roles();
        roles.retain(|item| item.id != role.id);
        self.persist_roles(&roles)?;
        Ok(self.status())
    }

    pub async fn start_role(&self, id: &str) -> Result<FrpRolesResponse> {
        let role = self.role_by_id(id)?;
        self.reap_finished_child(&role.id, role.component);
        if self.is_running(&role.id) {
            return Ok(self.status());
        }
        validate_frp_role(&role)?;
        let role_dir = self.role_dir(&role.id);
        fs::create_dir_all(&role_dir)?;
        let binary_path = self.resolve_role_binary(&role)?;
        let generated_config_path = self.effective_role_config_path(&role);
        let log_path = role_dir.join(role_log_file_name(role.component));
        if external_config_path_for_role(&role).is_none() {
            let toml = match role.component {
                FrpComponent::Frpc => {
                    render_frpc_toml(role.frpc.as_ref().context("frpc 角色缺少客户端配置")?)
                }
                FrpComponent::Frps => {
                    render_frps_toml(role.frps.as_ref().context("frps 角色缺少服务端配置")?)
                }
            };
            fs::write(&generated_config_path, toml)
                .with_context(|| format!("cannot write {}", generated_config_path.display()))?;
        }
        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .with_context(|| format!("cannot open {}", log_path.display()))?;
        let log_file_err = log_file.try_clone().context("cannot clone frp log file")?;
        append_log_line(&log_path, &format!("starting {}", role.component.executable_name()))?;

        let child = Command::new(&binary_path)
            .arg("-c")
            .arg(&generated_config_path)
            .current_dir(&role_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log_file))
            .stderr(Stdio::from(log_file_err))
            .kill_on_drop(false)
            .spawn()
            .with_context(|| format!("启动 {} 失败: {}", role.name, binary_path.display()))?;

        let pid = child.id();
        let mut runtime = crate::lock_or_recover!(self.inner.lock());
        runtime.insert(
            role.id.clone(),
            FrpcRuntime {
                child: Some(child),
                started_at: Some(unix_now_secs()),
                last_error: None,
            },
        );
        info!("frp role started: id={} pid={pid:?}", role.id);
        Ok(self.status())
    }

    pub async fn stop_role(&self, id: &str) -> Result<FrpRolesResponse> {
        let role = self.role_by_id(id).ok();
        let mut runtime = crate::lock_or_recover!(self.inner.lock()).remove(id);
        if let Some(mut runtime) = runtime.take() {
            if let Some(mut child) = runtime.child.take() {
                let log_path = self.role_log_path(id, None);
                append_log_line(&log_path, "stopping frp role")?;
                if let Err(error) = child.kill().await {
                    warn!("kill frp role {id} failed: {error}");
                }
                let _ = child.wait().await;
            }
        } else if let Some(role) = role.as_ref()
            && let Some(process) = self.detect_external_process_for_role(role)
        {
            let log_path = self.role_log_path(id, Some(role.component));
            append_log_line(&log_path, &format!("stopping external frp pid={}", process.pid))?;
            terminate_process(process.pid).await?;
        }
        Ok(self.status())
    }

    pub async fn restart_role(&self, id: &str) -> Result<FrpRolesResponse> {
        self.stop_role(id).await?;
        self.start_role(id).await
    }

    pub async fn download_role_binary(&self, id: &str) -> Result<FrpDownloadResponse> {
        let role = self.role_by_id(id)?;
        self.reap_finished_child(&role.id, role.component);
        if self.is_running(&role.id) {
            anyhow::bail!("角色 `{}` 正在运行，请先停止再下载或更新二进制", role.name);
        }
        download_frp_binary(role.component, &self.role_dir(&role.id)).await
    }

    pub fn system_discovery(&self) -> FrpSystemDiscoveryResponse {
        let mut items = Vec::new();
        let roles = self.roles();
        for component in [FrpComponent::Frpc, FrpComponent::Frps] {
            if let Some(path) = find_in_path(component.executable_name()) {
                items.push(FrpSystemEntry {
                    id: format!("path-{}", component.executable_name()),
                    component,
                    source: "PATH".to_string(),
                    pid: None,
                    binary_path: path.display().to_string(),
                    config_path: None,
                    command: component.executable_name().to_string(),
                    managed_role_id: matching_role_id(
                        &roles,
                        component,
                        Some(path.as_path()),
                        None,
                    ),
                });
            }
        }
        for process in detect_frp_processes() {
            let binary_path = process
                .binary_path
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| process.component.executable_name().to_string());
            let config_path = process
                .config_path
                .as_ref()
                .map(|path| path.display().to_string());
            items.push(FrpSystemEntry {
                id: format!("process-{}", process.pid),
                component: process.component,
                source: "进程".to_string(),
                pid: Some(process.pid),
                binary_path,
                config_path,
                command: process.command,
                managed_role_id: matching_role_id(
                    &roles,
                    process.component,
                    process.binary_path.as_deref(),
                    process.config_path.as_deref(),
                ),
            });
        }
        FrpSystemDiscoveryResponse { items }
    }

    pub fn adopt_system_entry(&self, request: AdoptFrpSystemRequest) -> Result<FrpRolesResponse> {
        let component = request.component;
        let binary_path = request.binary_path.trim();
        let config_path = request.config_path.trim();
        if binary_path.is_empty() {
            anyhow::bail!("接管 FRP 时需要二进制路径");
        }
        let binary_path_buf = PathBuf::from(binary_path);
        if !binary_path_buf.is_file() {
            anyhow::bail!("FRP 二进制不存在: {}", binary_path_buf.display());
        }
        if config_path.is_empty() {
            anyhow::bail!("接管正在运行的 FRP 需要配置文件路径");
        }
        let config_path_buf = PathBuf::from(config_path);
        if !config_path_buf.is_file() {
            anyhow::bail!("FRP 配置文件不存在: {}", config_path_buf.display());
        }
        let id = if request.role_id.trim().is_empty() {
            let stem = config_path_buf
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or_else(|| component.executable_name());
            safe_role_id(&format!("{}-{stem}", component.executable_name()))
        } else {
            safe_role_id(&request.role_id)
        };
        let name =
            non_empty_or(request.name.trim(), &format!("系统 {}", component.executable_name()));
        let binary_source = if path_matches_system_binary(component, &binary_path_buf) {
            "system".to_string()
        } else {
            "custom".to_string()
        };
        let binary_path_value = if binary_source == "system" {
            String::new()
        } else {
            binary_path_buf.display().to_string()
        };
        let role = match component {
            FrpComponent::Frpc => {
                let config = FrpcConfig {
                    binary_source,
                    binary_path: binary_path_value,
                    external_config_path: config_path_buf.display().to_string(),
                    ..Default::default()
                };
                FrpRole {
                    id,
                    name,
                    component,
                    frpc: Some(config),
                    frps: None,
                }
            }
            FrpComponent::Frps => {
                let config = FrpsConfig {
                    binary_source,
                    binary_path: binary_path_value,
                    external_config_path: config_path_buf.display().to_string(),
                    public_addr: request.public_addr.trim().to_string(),
                    ..Default::default()
                };
                FrpRole {
                    id,
                    name,
                    component,
                    frpc: None,
                    frps: Some(config),
                }
            }
        };
        self.save_role(role)
    }

    fn default_roles(&self, default_local_port: u16) -> Result<Vec<FrpRole>> {
        let frpc_config_path = self.legacy_frpc_dir.join(FRPC_CONFIG_JSON);
        let frpc = if frpc_config_path.exists() {
            load_frpc_config(&frpc_config_path)?
        } else {
            FrpcConfig::default_for_local_port(default_local_port)
        };
        let frps_config_path = self.legacy_frps_dir.join(FRPS_CONFIG_JSON);
        let frps = if frps_config_path.exists() {
            load_frps_config(&frps_config_path)?
        } else {
            FrpsConfig::default()
        };
        Ok(vec![
            FrpRole {
                id: "frpc-default".to_string(),
                name: "默认 frpc".to_string(),
                component: FrpComponent::Frpc,
                frpc: Some(frpc),
                frps: None,
            },
            FrpRole {
                id: "frps-default".to_string(),
                name: "默认 frps".to_string(),
                component: FrpComponent::Frps,
                frpc: None,
                frps: Some(frps),
            },
        ])
    }

    fn persist_roles(&self, roles: &[FrpRole]) -> Result<()> {
        fs::create_dir_all(self.dir.as_path())?;
        let encoded = serde_json::to_vec_pretty(roles)?;
        fs::write(self.roles_path.as_path(), encoded)
            .with_context(|| format!("cannot write {}", self.roles_path.display()))?;
        Ok(())
    }

    fn role_by_id(&self, id: &str) -> Result<FrpRole> {
        let id = id.trim();
        self.roles()
            .into_iter()
            .find(|role| role.id == id)
            .with_context(|| format!("找不到 FRP 角色 `{id}`"))
    }

    fn role_status(&self, role: FrpRole) -> FrpRoleStatus {
        let role = hydrate_external_role_config(role);
        let configured = validate_frp_role(&role).is_ok();
        let binary_path = self.resolve_role_binary(&role).ok();
        let (mut running, mut pid, started_at, last_error) = {
            let runtime = crate::lock_or_recover!(self.inner.lock());
            let runtime = runtime.get(&role.id);
            (
                runtime.and_then(|item| item.child.as_ref()).is_some(),
                runtime
                    .and_then(|item| item.child.as_ref())
                    .and_then(Child::id),
                runtime.and_then(|item| item.started_at),
                runtime.and_then(|item| item.last_error.clone()),
            )
        };
        if !running && let Some(process) = self.detect_external_process_for_role(&role) {
            running = true;
            pid = Some(process.pid);
        }
        let generated_config_path = self.effective_role_config_path(&role);
        let log_path = self.role_log_path(&role.id, Some(role.component));
        FrpRoleStatus {
            role,
            configured,
            running,
            pid,
            started_at,
            binary_path: binary_path.map(|path| path.display().to_string()),
            generated_config_path: generated_config_path.display().to_string(),
            log_path: log_path.display().to_string(),
            last_error,
            log_tail: read_log_tail(&log_path).unwrap_or_default(),
        }
    }

    fn is_running(&self, id: &str) -> bool {
        crate::lock_or_recover!(self.inner.lock())
            .get(id)
            .and_then(|runtime| runtime.child.as_ref())
            .is_some()
    }

    fn reap_finished_child(&self, id: &str, component: FrpComponent) {
        let mut runtime = crate::lock_or_recover!(self.inner.lock());
        let Some(entry) = runtime.get_mut(id) else {
            return;
        };
        let Some(child) = entry.child.as_mut() else {
            return;
        };
        match child.try_wait() {
            Ok(Some(status)) => {
                let message = format!("{} 已退出: {status}", component.executable_name());
                entry.child = None;
                entry.started_at = None;
                entry.last_error = Some(message.clone());
                let _ = append_log_line(&self.role_log_path(id, Some(component)), &message);
            }
            Ok(None) => {}
            Err(error) => {
                entry.last_error = Some(format!("检查 FRP 角色状态失败: {error}"));
            }
        }
    }

    fn role_dir(&self, id: &str) -> PathBuf {
        self.dir.join(safe_role_id(id))
    }

    fn role_config_path(&self, id: &str, component: FrpComponent) -> PathBuf {
        self.role_dir(id).join(role_config_file_name(component))
    }

    fn effective_role_config_path(&self, role: &FrpRole) -> PathBuf {
        external_config_path_for_role(role)
            .map(PathBuf::from)
            .unwrap_or_else(|| self.role_config_path(&role.id, role.component))
    }

    fn role_log_path(&self, id: &str, component: Option<FrpComponent>) -> PathBuf {
        let component = component.unwrap_or_else(|| {
            self.role_by_id(id)
                .map(|role| role.component)
                .unwrap_or(FrpComponent::Frpc)
        });
        self.role_dir(id).join(role_log_file_name(component))
    }

    fn resolve_role_binary(&self, role: &FrpRole) -> Result<PathBuf> {
        let (source, configured) = match role.component {
            FrpComponent::Frpc => role
                .frpc
                .as_ref()
                .map(|config| (config.binary_source.as_str(), config.binary_path.as_str())),
            FrpComponent::Frps => role
                .frps
                .as_ref()
                .map(|config| (config.binary_source.as_str(), config.binary_path.as_str())),
        }
        .unwrap_or((DEFAULT_BINARY_SOURCE, ""));
        resolve_frp_binary(
            role.component,
            source,
            configured,
            &self.role_dir(&role.id),
            match role.component {
                FrpComponent::Frpc => Some(self.legacy_frpc_dir.as_path()),
                FrpComponent::Frps => Some(self.legacy_frps_dir.as_path()),
            },
        )
    }

    fn detect_external_process_for_role(&self, role: &FrpRole) -> Option<DetectedFrpProcess> {
        let config_path = external_config_path_for_role(role)?;
        let config_path = PathBuf::from(config_path);
        detect_frp_processes().into_iter().find(|process| {
            process.component == role.component
                && process
                    .config_path
                    .as_ref()
                    .is_some_and(|path| paths_equal(path, &config_path))
        })
    }
}

fn load_frpc_config(path: &Path) -> Result<FrpcConfig> {
    if !path.exists() {
        return Ok(FrpcConfig::default());
    }
    let content = fs::read_to_string(path)?;
    let config: FrpcConfig = serde_json::from_str(&content)?;
    Ok(normalize_frpc_config(config))
}

fn load_frps_config(path: &Path) -> Result<FrpsConfig> {
    if !path.exists() {
        return Ok(FrpsConfig::default());
    }
    let content = fs::read_to_string(path)?;
    let config: FrpsConfig = serde_json::from_str(&content)?;
    Ok(normalize_frps_config(config))
}

fn load_frp_roles(path: &Path) -> Result<Vec<FrpRole>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read_to_string(path)?;
    let roles: Vec<FrpRole> = serde_json::from_str(&content)?;
    Ok(roles.into_iter().map(normalize_frp_role).collect())
}

fn normalize_frp_role(mut role: FrpRole) -> FrpRole {
    role.id = safe_role_id(&role.id);
    if role.id.is_empty() {
        role.id = format!(
            "{}-{}",
            match role.component {
                FrpComponent::Frpc => "frpc",
                FrpComponent::Frps => "frps",
            },
            unix_now_secs()
        );
    }
    role.name = non_empty_or(
        role.name.trim(),
        match role.component {
            FrpComponent::Frpc => "frpc",
            FrpComponent::Frps => "frps",
        },
    );
    match role.component {
        FrpComponent::Frpc => {
            let config = role.frpc.take().unwrap_or_default();
            role.frpc = Some(normalize_frpc_config(config));
            role.frps = None;
        }
        FrpComponent::Frps => {
            let config = role.frps.take().unwrap_or_default();
            role.frps = Some(normalize_frps_config(config));
            role.frpc = None;
        }
    }
    role
}

fn validate_frp_role(role: &FrpRole) -> Result<()> {
    if role.id.trim().is_empty() {
        anyhow::bail!("角色 ID 不能为空");
    }
    if role.name.trim().is_empty() {
        anyhow::bail!("角色名称不能为空");
    }
    match role.component {
        FrpComponent::Frpc => {
            validate_frpc_config(role.frpc.as_ref().context("frpc 角色缺少客户端配置")?)
        }
        FrpComponent::Frps => {
            validate_frps_config(role.frps.as_ref().context("frps 角色缺少服务端配置")?)
        }
    }
}

fn normalize_frpc_config(mut config: FrpcConfig) -> FrpcConfig {
    config.binary_source = normalize_binary_source(&config.binary_source);
    config.binary_path = config.binary_path.trim().to_string();
    config.external_config_path = config.external_config_path.trim().to_string();
    config.server_addr = config.server_addr.trim().to_string();
    config.token = config.token.trim().to_string();
    config.web_server_addr = non_empty_or(config.web_server_addr.trim(), DEFAULT_WEB_SERVER_ADDR);
    if config.web_server_port == 0 {
        config.web_server_port = DEFAULT_WEB_SERVER_PORT;
    }
    config.proxies = config
        .proxies
        .into_iter()
        .map(|mut proxy| {
            proxy.name = non_empty_or(proxy.name.trim(), "webclx");
            proxy.proxy_type = non_empty_or(proxy.proxy_type.trim(), "tcp").to_lowercase();
            proxy.local_ip = non_empty_or(proxy.local_ip.trim(), DEFAULT_LOCAL_IP);
            if proxy.local_port == 0 {
                proxy.local_port = DEFAULT_LOCAL_PORT;
            }
            if proxy.remote_port == 0 {
                proxy.remote_port = DEFAULT_REMOTE_PORT;
            }
            proxy.custom_domains = proxy.custom_domains.trim().to_string();
            proxy
        })
        .collect();
    if config.proxies.is_empty() {
        config.proxies.push(FrpcProxyConfig::default());
    }
    config
}

fn normalize_frps_config(mut config: FrpsConfig) -> FrpsConfig {
    config.binary_source = normalize_binary_source(&config.binary_source);
    config.binary_path = config.binary_path.trim().to_string();
    config.external_config_path = config.external_config_path.trim().to_string();
    config.bind_addr = non_empty_or(config.bind_addr.trim(), DEFAULT_FRPS_BIND_ADDR);
    config.public_addr = config.public_addr.trim().to_string();
    if config.bind_port == 0 {
        config.bind_port = DEFAULT_FRPS_BIND_PORT;
    }
    config.token = config.token.trim().to_string();
    config.web_server_addr = non_empty_or(config.web_server_addr.trim(), DEFAULT_WEB_SERVER_ADDR);
    if config.web_server_port == 0 {
        config.web_server_port = DEFAULT_FRPS_WEB_SERVER_PORT;
    }
    config.dashboard_user = config.dashboard_user.trim().to_string();
    config.dashboard_password = config.dashboard_password.trim().to_string();
    config
}

fn validate_frpc_config(config: &FrpcConfig) -> Result<()> {
    if !config.enabled {
        anyhow::bail!("frpc 未启用");
    }
    if !config.external_config_path.trim().is_empty() {
        let path = PathBuf::from(config.external_config_path.trim());
        if !path.is_file() {
            anyhow::bail!("外部 frpc 配置文件不存在: {}", path.display());
        }
        return Ok(());
    }
    if config.server_addr.trim().is_empty() {
        anyhow::bail!("请填写 frps 服务器地址");
    }
    if config.server_port == 0 {
        anyhow::bail!("请填写有效 frps 端口");
    }
    for proxy in &config.proxies {
        if proxy.name.trim().is_empty() {
            anyhow::bail!("代理名称不能为空");
        }
        match proxy.proxy_type.as_str() {
            "tcp" => {
                if proxy.remote_port == 0 {
                    anyhow::bail!("TCP 代理 `{}` 需要远端端口", proxy.name);
                }
            }
            "http" | "https" => {
                if proxy.custom_domains.trim().is_empty() {
                    anyhow::bail!("HTTP/HTTPS 代理 `{}` 需要 customDomains", proxy.name);
                }
            }
            _ => anyhow::bail!("不支持的代理类型 `{}`，请使用 tcp/http/https", proxy.proxy_type),
        }
        if proxy.local_ip.trim().is_empty() || proxy.local_port == 0 {
            anyhow::bail!("代理 `{}` 的本地地址无效", proxy.name);
        }
    }
    Ok(())
}

fn validate_frps_config(config: &FrpsConfig) -> Result<()> {
    if !config.enabled {
        anyhow::bail!("frps 未启用");
    }
    if config.public_addr.trim().is_empty() {
        anyhow::bail!("服务器必须填写公网地址");
    }
    if !config.external_config_path.trim().is_empty() {
        let path = PathBuf::from(config.external_config_path.trim());
        if !path.is_file() {
            anyhow::bail!("外部 frps 配置文件不存在: {}", path.display());
        }
        return Ok(());
    }
    if config.bind_addr.trim().is_empty() {
        anyhow::bail!("请填写 frps 绑定地址");
    }
    if config.bind_port == 0 {
        anyhow::bail!("请填写有效 frps 绑定端口");
    }
    if config.web_server_port == config.bind_port {
        anyhow::bail!("frps 管理端口不能和 bindPort 相同");
    }
    Ok(())
}

fn hydrate_external_role_config(mut role: FrpRole) -> FrpRole {
    match role.component {
        FrpComponent::Frpc => {
            if let Some(config) = role.frpc.take() {
                role.frpc = Some(hydrate_external_frpc_config(config));
            }
        }
        FrpComponent::Frps => {}
    }
    role
}

fn hydrate_external_frpc_config(config: FrpcConfig) -> FrpcConfig {
    let external_path = config.external_config_path.trim();
    if external_path.is_empty() {
        return config;
    }
    let path = PathBuf::from(external_path);
    match parse_frpc_toml_config(&path) {
        Ok(parsed) => merge_external_frpc_config(config, parsed),
        Err(error) => {
            warn!("parse external frpc config {} failed: {error}", path.display());
            config
        }
    }
}

fn merge_external_frpc_config(mut base: FrpcConfig, parsed: FrpcConfig) -> FrpcConfig {
    base.server_addr = parsed.server_addr;
    base.server_port = parsed.server_port;
    base.token = parsed.token;
    base.tls_enable = parsed.tls_enable;
    base.web_server_addr = parsed.web_server_addr;
    base.web_server_port = parsed.web_server_port;
    base.proxies = parsed.proxies;
    base.extra_toml = parsed.extra_toml;
    normalize_frpc_config(base)
}

fn parse_frpc_toml_config(path: &Path) -> Result<FrpcConfig> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("cannot read external frpc config {}", path.display()))?;
    parse_frpc_toml_config_content(&content)
}

fn parse_frpc_toml_config_content(content: &str) -> Result<FrpcConfig> {
    let doc = content
        .parse::<DocumentMut>()
        .context("外部 frpc TOML 格式无效")?;
    let mut config = FrpcConfig::default();
    config.server_addr = toml_string(&doc, "serverAddr").unwrap_or_default();
    config.server_port = toml_u16(&doc, "serverPort").unwrap_or(DEFAULT_FRPS_BIND_PORT);
    config.token = toml_nested_string(&doc, &["auth", "token"]).unwrap_or_default();
    config.tls_enable = toml_nested_bool(&doc, &["transport", "tls", "enable"]).unwrap_or(false);
    config.web_server_addr = toml_nested_string(&doc, &["webServer", "addr"])
        .unwrap_or_else(|| DEFAULT_WEB_SERVER_ADDR.to_string());
    config.web_server_port =
        toml_nested_u16(&doc, &["webServer", "port"]).unwrap_or(DEFAULT_WEB_SERVER_PORT);
    config.proxies = toml_frpc_proxies(&doc);
    if config.proxies.is_empty() {
        config.proxies.push(FrpcProxyConfig::default());
    }
    Ok(normalize_frpc_config(config))
}

fn toml_frpc_proxies(doc: &DocumentMut) -> Vec<FrpcProxyConfig> {
    let Some(proxies) = doc.get("proxies").and_then(Item::as_array_of_tables) else {
        return Vec::new();
    };
    proxies
        .iter()
        .map(|proxy| FrpcProxyConfig {
            name: proxy
                .get("name")
                .and_then(|item| item.as_str())
                .unwrap_or("webclx")
                .to_string(),
            proxy_type: proxy
                .get("type")
                .and_then(|item| item.as_str())
                .unwrap_or("tcp")
                .to_string(),
            local_ip: proxy
                .get("localIP")
                .and_then(|item| item.as_str())
                .or_else(|| proxy.get("localIp").and_then(|item| item.as_str()))
                .unwrap_or(DEFAULT_LOCAL_IP)
                .to_string(),
            local_port: proxy
                .get("localPort")
                .and_then(|item| item.as_integer())
                .and_then(i64_to_u16)
                .unwrap_or(DEFAULT_LOCAL_PORT),
            remote_port: proxy
                .get("remotePort")
                .and_then(|item| item.as_integer())
                .and_then(i64_to_u16)
                .unwrap_or(DEFAULT_REMOTE_PORT),
            custom_domains: proxy
                .get("customDomains")
                .map(toml_string_list)
                .unwrap_or_default(),
        })
        .collect()
}

fn toml_string(doc: &DocumentMut, key: &str) -> Option<String> {
    doc.get(key)
        .and_then(|item| item.as_str())
        .map(str::to_string)
}

fn toml_u16(doc: &DocumentMut, key: &str) -> Option<u16> {
    doc.get(key)
        .and_then(|item| item.as_integer())
        .and_then(i64_to_u16)
}

fn toml_nested_string(doc: &DocumentMut, path: &[&str]) -> Option<String> {
    toml_nested_item(doc, path)
        .and_then(|item| item.as_str())
        .map(str::to_string)
}

fn toml_nested_u16(doc: &DocumentMut, path: &[&str]) -> Option<u16> {
    toml_nested_item(doc, path)
        .and_then(|item| item.as_integer())
        .and_then(i64_to_u16)
}

fn toml_nested_bool(doc: &DocumentMut, path: &[&str]) -> Option<bool> {
    toml_nested_item(doc, path).and_then(|item| item.as_bool())
}

fn toml_nested_item<'a>(doc: &'a DocumentMut, path: &[&str]) -> Option<&'a Item> {
    let (first, rest) = path.split_first()?;
    let mut item = doc.get(first)?;
    for key in rest {
        item = item.get(key)?;
    }
    Some(item)
}

fn toml_string_list(item: &Item) -> String {
    if let Some(value) = item.as_str() {
        return value.to_string();
    }
    item.as_array()
        .map(|array| {
            array
                .iter()
                .filter_map(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(",")
        })
        .unwrap_or_default()
}

fn i64_to_u16(value: i64) -> Option<u16> {
    u16::try_from(value).ok()
}

fn render_frpc_toml(config: &FrpcConfig) -> String {
    let mut lines = Vec::new();
    lines.push(format!("serverAddr = {}", toml_quote(&config.server_addr)));
    lines.push(format!("serverPort = {}", config.server_port));
    if !config.token.is_empty() {
        lines.push(String::new());
        lines.push("[auth]".to_string());
        lines.push(format!("token = {}", toml_quote(&config.token)));
    }
    if config.tls_enable {
        lines.push(String::new());
        lines.push("[transport.tls]".to_string());
        lines.push("enable = true".to_string());
    }
    lines.push(String::new());
    lines.push("[webServer]".to_string());
    lines.push(format!("addr = {}", toml_quote(&config.web_server_addr)));
    lines.push(format!("port = {}", config.web_server_port));
    lines.push(String::new());
    lines.push("[[proxies]]".to_string());

    for (index, proxy) in config.proxies.iter().enumerate() {
        if index > 0 {
            lines.push(String::new());
            lines.push("[[proxies]]".to_string());
        }
        lines.push(format!("name = {}", toml_quote(&proxy.name)));
        lines.push(format!("type = {}", toml_quote(&proxy.proxy_type)));
        lines.push(format!("localIP = {}", toml_quote(&proxy.local_ip)));
        lines.push(format!("localPort = {}", proxy.local_port));
        if proxy.proxy_type == "tcp" {
            lines.push(format!("remotePort = {}", proxy.remote_port));
        }
        if matches!(proxy.proxy_type.as_str(), "http" | "https") && !proxy.custom_domains.is_empty()
        {
            let domains = proxy
                .custom_domains
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(toml_quote)
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("customDomains = [{domains}]"));
        }
    }

    if !config.extra_toml.trim().is_empty() {
        lines.push(String::new());
        lines.push("# Extra frpc TOML from webClx UI".to_string());
        lines.push(config.extra_toml.trim().to_string());
    }

    lines.push(String::new());
    lines.join("\n")
}

fn render_frps_toml(config: &FrpsConfig) -> String {
    let mut lines = Vec::new();
    lines.push(format!("bindAddr = {}", toml_quote(&config.bind_addr)));
    lines.push(format!("bindPort = {}", config.bind_port));
    if !config.token.is_empty() {
        lines.push(String::new());
        lines.push("[auth]".to_string());
        lines.push(format!("token = {}", toml_quote(&config.token)));
    }
    lines.push(String::new());
    lines.push("[webServer]".to_string());
    lines.push(format!("addr = {}", toml_quote(&config.web_server_addr)));
    lines.push(format!("port = {}", config.web_server_port));
    if !config.dashboard_user.is_empty() {
        lines.push(format!("user = {}", toml_quote(&config.dashboard_user)));
    }
    if !config.dashboard_password.is_empty() {
        lines.push(format!("password = {}", toml_quote(&config.dashboard_password)));
    }

    if !config.extra_toml.trim().is_empty() {
        lines.push(String::new());
        lines.push("# Extra frps TOML from webClx UI".to_string());
        lines.push(config.extra_toml.trim().to_string());
    }

    lines.push(String::new());
    lines.join("\n")
}

async fn download_frp_binary(
    component: FrpComponent,
    destination_dir: &Path,
) -> Result<FrpDownloadResponse> {
    let platform = current_frp_platform()?;
    let release = fetch_latest_frp_release().await?;
    let asset = select_frp_release_asset(&release.assets, &platform).with_context(|| {
        format!("找不到适配当前平台 {}_{} 的 frp Release 资产", platform.os, platform.arch)
    })?;
    let bytes = reqwest::Client::builder()
        .build()?
        .get(&asset.browser_download_url)
        .header(reqwest::header::USER_AGENT, FRP_USER_AGENT)
        .send()
        .await
        .context("请求 frp 下载地址失败")?
        .error_for_status()
        .context("frp 下载响应失败")?
        .bytes()
        .await
        .context("读取 frp 下载内容失败")?;

    let binary_name = component.executable_name();
    let binary = extract_binary_from_archive(&asset.name, &bytes, binary_name)?;
    fs::create_dir_all(destination_dir)
        .with_context(|| format!("cannot create {}", destination_dir.display()))?;
    let destination = destination_dir.join(binary_name);
    let temporary = destination.with_extension("download.tmp");
    fs::write(&temporary, binary)
        .with_context(|| format!("cannot write {}", temporary.display()))?;
    make_executable(&temporary)?;
    fs::rename(&temporary, &destination)
        .with_context(|| format!("cannot install {}", destination.display()))?;

    Ok(FrpDownloadResponse {
        component,
        version: release.tag_name,
        platform,
        asset_name: asset.name.clone(),
        download_url: asset.browser_download_url.clone(),
        binary_path: destination.display().to_string(),
    })
}

async fn fetch_latest_frp_release() -> Result<GithubRelease> {
    reqwest::Client::builder()
        .build()?
        .get(FRP_RELEASE_LATEST_URL)
        .header(reqwest::header::USER_AGENT, FRP_USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .context("请求 frp latest release 失败")?
        .error_for_status()
        .context("frp latest release 响应失败")?
        .json::<GithubRelease>()
        .await
        .context("解析 frp latest release 失败")
}

fn current_frp_platform() -> Result<FrpPlatform> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "windows",
        "freebsd" => "freebsd",
        other => anyhow::bail!("当前系统 `{other}` 暂不支持自动下载 frp"),
    };
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "x86" | "i686" => "386",
        "aarch64" => "arm64",
        "arm" => "arm",
        other => anyhow::bail!("当前 CPU 架构 `{other}` 暂不支持自动下载 frp"),
    };
    let archive_ext = if os == "windows" { "zip" } else { "tar.gz" };
    Ok(FrpPlatform {
        os: os.to_string(),
        arch: arch.to_string(),
        archive_ext: archive_ext.to_string(),
    })
}

fn select_frp_release_asset<'a>(
    assets: &'a [GithubReleaseAsset],
    platform: &FrpPlatform,
) -> Option<&'a GithubReleaseAsset> {
    let suffix = format!("_{}_{}.{}", platform.os, platform.arch, platform.archive_ext);
    assets.iter().find(|asset| {
        asset.name.starts_with("frp_")
            && asset.name.ends_with(&suffix)
            && !asset.name.contains("sha256")
    })
}

fn extract_binary_from_archive(
    archive_name: &str,
    bytes: &[u8],
    binary_name: &str,
) -> Result<Vec<u8>> {
    if archive_name.ends_with(".zip") {
        return extract_binary_from_zip(bytes, binary_name);
    }
    if archive_name.ends_with(".tar.gz") || archive_name.ends_with(".tgz") {
        return extract_binary_from_targz(bytes, binary_name);
    }
    anyhow::bail!("不支持的 frp 压缩包格式: {archive_name}")
}

fn extract_binary_from_zip(bytes: &[u8], binary_name: &str) -> Result<Vec<u8>> {
    let mut archive = ZipArchive::new(Cursor::new(bytes)).context("打开 frp zip 失败")?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index).context("读取 frp zip 条目失败")?;
        if entry.is_dir() || entry.enclosed_name().is_none() {
            continue;
        }
        if entry
            .name()
            .replace('\\', "/")
            .ends_with(&format!("/{binary_name}"))
            || entry.name() == binary_name
        {
            let mut output = Vec::new();
            entry
                .read_to_end(&mut output)
                .context("读取 frp zip 二进制失败")?;
            return Ok(output);
        }
    }
    anyhow::bail!("frp zip 中找不到 {binary_name}")
}

fn extract_binary_from_targz(bytes: &[u8], binary_name: &str) -> Result<Vec<u8>> {
    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = Archive::new(decoder);
    for entry in archive.entries().context("打开 frp tar.gz 失败")? {
        let mut entry = entry.context("读取 frp tar.gz 条目失败")?;
        let path = entry.path().context("读取 frp tar.gz 路径失败")?;
        if path.file_name().and_then(|name| name.to_str()) == Some(binary_name) {
            let mut output = Vec::new();
            entry
                .read_to_end(&mut output)
                .context("读取 frp tar.gz 二进制失败")?;
            return Ok(output);
        }
    }
    anyhow::bail!("frp tar.gz 中找不到 {binary_name}")
}

#[cfg(unix)]
pub(crate) fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
pub(crate) fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

fn read_log_tail(path: &Path) -> Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    let metadata = fs::metadata(path)?;
    let content = fs::read(path)?;
    let slice = if metadata.len() > LOG_TAIL_BYTES {
        let start = content.len().saturating_sub(LOG_TAIL_BYTES as usize);
        &content[start..]
    } else {
        &content
    };
    Ok(String::from_utf8_lossy(slice).to_string())
}

fn append_log_line(path: &Path, message: &str) -> Result<()> {
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(file, "[webClx {}] {message}", unix_now_secs())?;
    Ok(())
}

fn find_in_path(program: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    std::env::split_paths(&path_var)
        .map(|dir| dir.join(program))
        .find(|path| path.is_file())
}

fn external_config_path_for_role(role: &FrpRole) -> Option<&str> {
    let value = match role.component {
        FrpComponent::Frpc => role.frpc.as_ref()?.external_config_path.trim(),
        FrpComponent::Frps => role.frps.as_ref()?.external_config_path.trim(),
    };
    if value.is_empty() { None } else { Some(value) }
}

fn matching_role_id(
    roles: &[FrpRole],
    component: FrpComponent,
    binary_path: Option<&Path>,
    config_path: Option<&Path>,
) -> Option<String> {
    roles.iter().find_map(|role| {
        if role.component != component {
            return None;
        }
        if let (Some(role_config), Some(config_path)) =
            (external_config_path_for_role(role), config_path)
            && paths_equal(&PathBuf::from(role_config), config_path)
        {
            return Some(role.id.clone());
        }
        let configured_binary = match component {
            FrpComponent::Frpc => role.frpc.as_ref()?.binary_path.trim(),
            FrpComponent::Frps => role.frps.as_ref()?.binary_path.trim(),
        };
        if let (Some(binary_path), false) = (binary_path, configured_binary.is_empty())
            && paths_equal(&PathBuf::from(configured_binary), binary_path)
        {
            return Some(role.id.clone());
        }
        None
    })
}

fn path_matches_system_binary(component: FrpComponent, path: &Path) -> bool {
    find_in_path(component.executable_name())
        .as_ref()
        .is_some_and(|candidate| paths_equal(candidate, path))
}

fn paths_equal(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}

#[cfg(unix)]
fn detect_frp_processes() -> Vec<DetectedFrpProcess> {
    let mut processes = Vec::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return processes;
    };
    for entry in entries.flatten() {
        let Some(pid_text) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Ok(pid) = pid_text.parse::<u32>() else {
            continue;
        };
        let proc_dir = entry.path();
        let Ok(raw_cmdline) = fs::read(proc_dir.join("cmdline")) else {
            continue;
        };
        if raw_cmdline.is_empty() {
            continue;
        }
        let args = raw_cmdline
            .split(|byte| *byte == 0)
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).to_string())
            .collect::<Vec<_>>();
        let Some(component) = frp_component_from_args(&args, proc_dir.join("exe").as_path()) else {
            continue;
        };
        let binary_path = fs::read_link(proc_dir.join("exe"))
            .ok()
            .or_else(|| args.first().map(PathBuf::from));
        let cwd = fs::read_link(proc_dir.join("cwd")).ok();
        let config_path = extract_frp_config_arg(&args).map(|path| {
            let path = PathBuf::from(path);
            if path.is_absolute() {
                path
            } else if let Some(cwd) = cwd.as_ref() {
                cwd.join(path)
            } else {
                path
            }
        });
        processes.push(DetectedFrpProcess {
            pid,
            component,
            binary_path,
            config_path,
            command: args.join(" "),
        });
    }
    processes
}

#[cfg(not(unix))]
fn detect_frp_processes() -> Vec<DetectedFrpProcess> {
    Vec::new()
}

#[cfg(unix)]
fn frp_component_from_args(args: &[String], exe_path: &Path) -> Option<FrpComponent> {
    let executable = fs::read_link(exe_path)
        .ok()
        .and_then(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(str::to_string)
        })
        .or_else(|| {
            args.first().and_then(|arg| {
                Path::new(arg)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
        })?;
    let executable = executable.trim_end_matches(".exe");
    match executable {
        "frpc" => Some(FrpComponent::Frpc),
        "frps" => Some(FrpComponent::Frps),
        _ => None,
    }
}

fn extract_frp_config_arg(args: &[String]) -> Option<String> {
    for (index, arg) in args.iter().enumerate() {
        if matches!(arg.as_str(), "-c" | "--config") {
            return args.get(index + 1).cloned();
        }
        if let Some(value) = arg.strip_prefix("-c=") {
            return Some(value.to_string());
        }
        if let Some(value) = arg.strip_prefix("--config=") {
            return Some(value.to_string());
        }
    }
    None
}

async fn terminate_process(pid: u32) -> Result<()> {
    #[cfg(unix)]
    {
        let status = Command::new("kill")
            .arg("-TERM")
            .arg(pid.to_string())
            .status()
            .await
            .with_context(|| format!("发送 TERM 到进程 {pid} 失败"))?;
        if !status.success() {
            anyhow::bail!("停止外部 FRP 进程 {pid} 失败: {status}");
        }
        Ok(())
    }
    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .arg("/PID")
            .arg(pid.to_string())
            .arg("/T")
            .status()
            .await
            .with_context(|| format!("停止进程 {pid} 失败"))?;
        if !status.success() {
            anyhow::bail!("停止外部 FRP 进程 {pid} 失败: {status}");
        }
        Ok(())
    }
}

fn resolve_frp_binary(
    component: FrpComponent,
    source: &str,
    configured: &str,
    runtime_dir: &Path,
    legacy_dir: Option<&Path>,
) -> Result<PathBuf> {
    let source = normalize_binary_source(source);
    let configured = configured.trim();
    let program = if cfg!(windows) {
        component.executable_name()
    } else {
        match component {
            FrpComponent::Frpc => "frpc",
            FrpComponent::Frps => "frps",
        }
    };
    if source == "custom" || (source == "auto" && !configured.is_empty()) {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Ok(path);
        }
        if configured.is_empty() {
            anyhow::bail!("{} 来源为指定路径，请填写二进制路径", program);
        }
        anyhow::bail!("{} 二进制不存在: {}", program, path.display());
    }

    if source == "auto" || source == "bundled" {
        let mut candidates = vec![
            runtime_dir.join(program),
            runtime_dir.join(format!("{program}.exe")),
        ];
        if let Some(legacy_dir) = legacy_dir {
            candidates.push(legacy_dir.join(program));
            candidates.push(legacy_dir.join(format!("{program}.exe")));
        }
        if let Some(parent) = runtime_dir.parent().and_then(Path::parent) {
            candidates.push(parent.join(program));
            candidates.push(parent.join(format!("{program}.exe")));
        }
        for candidate in candidates {
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        if source == "bundled" {
            anyhow::bail!("找不到自带 {program}；请先下载或放到角色/运行目录")
        }
    }

    if source == "auto" || source == "system" {
        if let Some(path) = find_in_path(program) {
            return Ok(path);
        }
        if cfg!(windows)
            && let Some(path) = find_in_path(&format!("{program}.exe"))
        {
            return Ok(path);
        }
        if source == "system" {
            anyhow::bail!("系统 PATH 中找不到 {program}")
        }
    }
    anyhow::bail!("找不到 {program}；请下载、放到运行目录，或填写绝对路径")
}

fn role_config_file_name(component: FrpComponent) -> &'static str {
    match component {
        FrpComponent::Frpc => FRPC_TOML,
        FrpComponent::Frps => FRPS_TOML,
    }
}

fn role_log_file_name(component: FrpComponent) -> &'static str {
    match component {
        FrpComponent::Frpc => FRPC_LOG,
        FrpComponent::Frps => FRPS_LOG,
    }
}

fn safe_role_id(value: &str) -> String {
    let mut output = String::new();
    for character in value.trim().chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
            output.push(character.to_ascii_lowercase());
        } else if !output.ends_with('-') {
            output.push('-');
        }
    }
    output.trim_matches('-').to_string()
}

fn toml_quote(value: &str) -> String {
    format!("{:?}", value)
}

fn non_empty_or(value: &str, fallback: &str) -> String {
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

fn unix_now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::{
        FrpPlatform, FrpsConfig, GithubReleaseAsset, normalize_binary_source,
        parse_frpc_toml_config_content, select_frp_release_asset, validate_frps_config,
    };

    #[test]
    fn frp_release_asset_selection_matches_current_platform_suffix() {
        let assets = vec![
            GithubReleaseAsset {
                name: "frp_0.65.0_linux_arm64.tar.gz".to_string(),
                browser_download_url: "https://example.com/arm64".to_string(),
            },
            GithubReleaseAsset {
                name: "frp_0.65.0_linux_amd64.tar.gz".to_string(),
                browser_download_url: "https://example.com/amd64".to_string(),
            },
            GithubReleaseAsset {
                name: "frp_0.65.0_linux_amd64.tar.gz.sha256".to_string(),
                browser_download_url: "https://example.com/sha".to_string(),
            },
        ];
        let platform = FrpPlatform {
            os: "linux".to_string(),
            arch: "amd64".to_string(),
            archive_ext: "tar.gz".to_string(),
        };

        let selected = select_frp_release_asset(&assets, &platform).expect("asset should match");

        assert_eq!(selected.browser_download_url, "https://example.com/amd64");
    }

    #[test]
    fn frps_config_rejects_dashboard_port_collision() {
        let config = FrpsConfig {
            bind_port: 7000,
            web_server_port: 7000,
            ..FrpsConfig::default()
        };

        assert!(validate_frps_config(&config).is_err());
    }

    #[test]
    fn binary_source_normalization_rejects_unknown_values() {
        assert_eq!(normalize_binary_source("system"), "system");
        assert_eq!(normalize_binary_source("custom"), "custom");
        assert_eq!(normalize_binary_source("bad"), "auto");
    }

    #[test]
    fn external_frpc_toml_parses_server_auth_and_proxies() {
        let config = parse_frpc_toml_config_content(
            r#"
serverAddr = "117.72.45.252"
serverPort = 13389
auth.token = "3166"

[[proxies]]
name = "newapi"
type = "tcp"
localIP = "192.168.3.2"
localPort = 13000
remotePort = 25400

[[proxies]]
name = "web"
type = "http"
localIP = "127.0.0.1"
localPort = 8080
customDomains = ["a.example.com", "b.example.com"]
"#,
        )
        .expect("external frpc toml should parse");

        assert_eq!(config.server_addr, "117.72.45.252");
        assert_eq!(config.server_port, 13389);
        assert_eq!(config.token, "3166");
        assert_eq!(config.proxies.len(), 2);
        assert_eq!(config.proxies[0].name, "newapi");
        assert_eq!(config.proxies[0].local_ip, "192.168.3.2");
        assert_eq!(config.proxies[0].local_port, 13000);
        assert_eq!(config.proxies[0].remote_port, 25400);
        assert_eq!(config.proxies[1].proxy_type, "http");
        assert_eq!(config.proxies[1].custom_domains, "a.example.com,b.example.com");
    }
}
