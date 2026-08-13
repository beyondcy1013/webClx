use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use auth_core::{
    PresetConfigOverride, ResolvedConfigTarget, UpstreamProxySettings, api_preset_model,
    persist_upstream_proxy_settings, resolve_effective_preset_config_targets,
};
use axum::{
    Json,
    extract::{Path as AxumPath, State},
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::{fs, sync::Mutex, time::sleep};
use toml_edit::DocumentMut;
use tracing::{error, warn};

use crate::{ApiResult, AppError, AppState};

use super::{
    apply::{
        apply_api_preset_locked, apply_auth_preset_locked, apply_claude_preset_locked,
        find_local_codex_config,
    },
    terminal_auth_write_targets,
};

const LEASE_HEARTBEAT_TIMEOUT: Duration = Duration::from_secs(45);
const LEASE_WATCHDOG_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PresetRunKind {
    OAuth,
    Api,
    Claude,
}

impl PresetRunKind {
    fn parse(value: &str) -> ApiResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "oauth" | "auth" => Ok(Self::OAuth),
            "api" | "codex_api" => Ok(Self::Api),
            "claude" | "claude_api" => Ok(Self::Claude),
            _ => Err(AppError::bad_request("preset_kind 必须是 oauth、api 或 claude。")),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::OAuth => "oauth",
            Self::Api => "api",
            Self::Claude => "claude",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct AcquirePresetRunLeaseRequest {
    preset_kind: String,
    preset_id: String,
    #[serde(default)]
    project_path: String,
    #[serde(default)]
    owner: String,
}

#[derive(Debug, Serialize)]
pub struct PresetRunLeaseResponse {
    pub ok: bool,
    pub lease_id: String,
    pub preset_kind: &'static str,
    pub preset_id: String,
    pub name: String,
    pub model: Option<String>,
    pub heartbeat_timeout_secs: u64,
}

#[derive(Debug, Serialize)]
pub struct PresetRunLeaseHeartbeatResponse {
    pub ok: bool,
    pub lease_id: String,
}

#[derive(Debug, Serialize)]
pub struct PresetRunLeaseReleaseResponse {
    pub ok: bool,
    pub lease_id: String,
    pub restored: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct AcquiredPresetRunLease {
    pub id: String,
    pub name: String,
    pub model: Option<String>,
}

#[derive(Clone)]
pub(crate) struct PresetRunLeaseManager {
    active: Arc<Mutex<Option<ActivePresetRunLease>>>,
    journal_path: Arc<PathBuf>,
}

impl PresetRunLeaseManager {
    pub(crate) fn new(journal_path: PathBuf) -> Self {
        Self {
            active: Arc::new(Mutex::new(None)),
            journal_path: Arc::new(journal_path),
        }
    }
}

struct ActivePresetRunLease {
    journal: PresetRunLeaseJournal,
    last_heartbeat: Instant,
    _config_guard: tokio::sync::OwnedMutexGuard<()>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetRunLeaseJournal {
    lease_id: String,
    preset_kind: String,
    preset_id: String,
    owner: String,
    created_at: u64,
    snapshot: PresetConfigSnapshot,
    #[serde(default)]
    pending_switch: Option<PendingPresetSwitch>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingPresetSwitch {
    preset_kind: String,
    preset_id: String,
    project_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PresetConfigSnapshot {
    files: Vec<FileSnapshot>,
    upstream_proxy_settings: UpstreamProxySettings,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileSnapshot {
    path: PathBuf,
    contents: Option<Vec<u8>>,
    mode: Option<u32>,
    uid: Option<u32>,
    gid: Option<u32>,
}

pub async fn acquire_preset_run_lease(
    State(state): State<AppState>,
    Json(payload): Json<AcquirePresetRunLeaseRequest>,
) -> ApiResult<Json<PresetRunLeaseResponse>> {
    let kind = PresetRunKind::parse(&payload.preset_kind)?;
    let acquired = begin_preset_run_lease(
        &state,
        kind,
        payload.preset_id.trim(),
        payload.project_path.trim(),
        payload.owner.trim(),
    )
    .await?;
    Ok(Json(PresetRunLeaseResponse {
        ok: true,
        lease_id: acquired.id,
        preset_kind: kind.as_str(),
        preset_id: payload.preset_id,
        name: acquired.name,
        model: acquired.model,
        heartbeat_timeout_secs: LEASE_HEARTBEAT_TIMEOUT.as_secs(),
    }))
}

pub async fn heartbeat_preset_run_lease(
    State(state): State<AppState>,
    AxumPath(lease_id): AxumPath<String>,
) -> ApiResult<Json<PresetRunLeaseHeartbeatResponse>> {
    heartbeat_preset_run_lease_internal(&state, &lease_id).await?;
    Ok(Json(PresetRunLeaseHeartbeatResponse { ok: true, lease_id }))
}

pub async fn release_preset_run_lease(
    State(state): State<AppState>,
    AxumPath(lease_id): AxumPath<String>,
) -> ApiResult<Json<PresetRunLeaseReleaseResponse>> {
    release_preset_run_lease_internal(&state, &lease_id).await?;
    Ok(Json(PresetRunLeaseReleaseResponse {
        ok: true,
        lease_id,
        restored: true,
    }))
}

pub(crate) async fn begin_preset_run_lease(
    state: &AppState,
    kind: PresetRunKind,
    preset_id: &str,
    project_path: &str,
    owner: &str,
) -> ApiResult<AcquiredPresetRunLease> {
    if preset_id.is_empty() {
        return Err(AppError::bad_request("preset_id 不能为空。"));
    }
    let mut active = state.preset_run_lease_manager.active.lock().await;
    if let Some(current) = active.as_ref() {
        return Err(AppError::conflict(format!(
            "全局预设门禁正由 `{}` 使用；请等待该 Agent 退出并恢复原配置。",
            current.journal.owner
        )));
    }

    let config_guard = state.auth_manager.lock_active_config_write().await;
    let target_config = target_config_overrides(state, kind, preset_id)?;
    let snapshot = capture_snapshot(state, kind, project_path, &target_config).await?;
    let lease_id = generate_lease_id();
    let owner = normalize_owner(owner);
    let journal = PresetRunLeaseJournal {
        lease_id: lease_id.clone(),
        preset_kind: kind.as_str().to_string(),
        preset_id: preset_id.to_string(),
        owner: owner.clone(),
        created_at: terminal_core::current_timestamp_millis(),
        snapshot: snapshot.clone(),
        pending_switch: None,
    };
    write_journal(&state.preset_run_lease_manager.journal_path, &journal).await?;

    let applied = apply_selected_preset_locked(state, kind, preset_id, project_path).await;
    let (name, model) = match applied {
        Ok(applied) => applied,
        Err(apply_error) => {
            let restore_result = restore_snapshot(state, &snapshot).await;
            drop(config_guard);
            if let Err(restore_error) = restore_result {
                return Err(AppError::internal(format!(
                    "应用指定预设失败，且恢复原配置失败：{apply_error}；恢复错误：{restore_error}"
                )));
            }
            remove_journal(&state.preset_run_lease_manager.journal_path).await?;
            return Err(apply_error);
        }
    };

    *active = Some(ActivePresetRunLease {
        journal,
        last_heartbeat: Instant::now(),
        _config_guard: config_guard,
    });
    drop(active);
    spawn_watchdog(state.clone(), lease_id.clone());

    Ok(AcquiredPresetRunLease {
        id: lease_id,
        name,
        model,
    })
}

/// Records a persistent preset selection while a temporary `webclx run`
/// lease owns the shared configuration. The active agent keeps its real
/// config files until it exits; release then restores the snapshot and applies
/// the final queued selection through the normal preset writer.
pub(crate) async fn queue_preset_switch_if_running(
    state: &AppState,
    kind: PresetRunKind,
    preset_id: &str,
    project_path: &str,
) -> ApiResult<Option<String>> {
    let name = preset_name(state, kind, preset_id)?;
    let pending_switch = PendingPresetSwitch {
        preset_kind: kind.as_str().to_string(),
        preset_id: preset_id.to_string(),
        project_path: project_path.to_string(),
    };
    let mut active = state.preset_run_lease_manager.active.lock().await;
    let Some(current) = active.as_mut() else {
        return Ok(None);
    };

    let mut journal = current.journal.clone();
    journal.pending_switch = Some(pending_switch);
    write_journal(&state.preset_run_lease_manager.journal_path, &journal).await?;
    current.journal = journal;
    Ok(Some(name))
}

pub(crate) async fn heartbeat_preset_run_lease_internal(
    state: &AppState,
    lease_id: &str,
) -> ApiResult<()> {
    let mut active = state.preset_run_lease_manager.active.lock().await;
    let current = active
        .as_mut()
        .ok_or_else(|| AppError::not_found("全局预设租约不存在或已经恢复。"))?;
    if current.journal.lease_id != lease_id {
        return Err(AppError::conflict("全局预设租约 ID 不匹配。"));
    }
    current.last_heartbeat = Instant::now();
    Ok(())
}

pub(crate) async fn release_preset_run_lease_internal(
    state: &AppState,
    lease_id: &str,
) -> ApiResult<()> {
    let mut active = state.preset_run_lease_manager.active.lock().await;
    let Some(current) = active.take() else {
        return Err(AppError::not_found("全局预设租约不存在或已经恢复。"));
    };
    if current.journal.lease_id != lease_id {
        *active = Some(current);
        return Err(AppError::conflict("全局预设租约 ID 不匹配。"));
    }

    if let Err(error) = restore_snapshot(state, &current.journal.snapshot).await {
        *active = Some(current);
        return Err(error);
    }
    if let Some(pending_switch) = current.journal.pending_switch.as_ref()
        && let Err(error) = apply_selected_preset_locked(
            state,
            PresetRunKind::parse(&pending_switch.preset_kind)?,
            &pending_switch.preset_id,
            &pending_switch.project_path,
        )
        .await
    {
        *active = Some(current);
        return Err(error);
    }
    if let Err(error) = remove_journal(&state.preset_run_lease_manager.journal_path).await {
        *active = Some(current);
        return Err(error);
    }
    drop(current);
    Ok(())
}

pub(crate) async fn recover_stale_preset_run_lease(state: &AppState) -> ApiResult<()> {
    let journal = match read_journal(&state.preset_run_lease_manager.journal_path).await? {
        Some(journal) => journal,
        None => return Ok(()),
    };
    warn!(
        lease_id = %journal.lease_id,
        owner = %journal.owner,
        "recovering stale global preset lease"
    );
    let _config_guard = state.auth_manager.lock_active_config_write().await;
    restore_snapshot(state, &journal.snapshot).await?;
    if let Some(pending_switch) = journal.pending_switch.as_ref() {
        apply_selected_preset_locked(
            state,
            PresetRunKind::parse(&pending_switch.preset_kind)?,
            &pending_switch.preset_id,
            &pending_switch.project_path,
        )
        .await?;
    }
    remove_journal(&state.preset_run_lease_manager.journal_path).await
}

fn spawn_watchdog(state: AppState, lease_id: String) {
    tokio::spawn(async move {
        loop {
            sleep(LEASE_WATCHDOG_INTERVAL).await;
            let expired = {
                let active = state.preset_run_lease_manager.active.lock().await;
                let Some(current) = active.as_ref() else {
                    return;
                };
                if current.journal.lease_id != lease_id {
                    return;
                }
                current.last_heartbeat.elapsed() >= LEASE_HEARTBEAT_TIMEOUT
            };
            if !expired {
                continue;
            }
            warn!(lease_id, "global preset lease heartbeat expired; restoring original config");
            if let Err(error) = release_preset_run_lease_internal(&state, &lease_id).await {
                error!(lease_id, error = %error, "failed to restore expired global preset lease");
                continue;
            }
            return;
        }
    });
}

async fn apply_selected_preset_locked(
    state: &AppState,
    kind: PresetRunKind,
    preset_id: &str,
    project_path: &str,
) -> ApiResult<(String, Option<String>)> {
    match kind {
        PresetRunKind::OAuth => {
            let (response, preset) =
                apply_auth_preset_locked(state, preset_id, project_path).await?;
            let defaults = codex_default_pairs(state);
            let default_pairs = defaults
                .iter()
                .map(|(key, value)| (key.as_str(), value.as_str()))
                .collect::<Vec<_>>();
            let targets =
                resolve_effective_preset_config_targets(&default_pairs, &preset.config_overrides)
                    .map_err(|error| {
                    AppError::internal(format!("OAuth 预设 config 覆盖无效: {error}"))
                })?;
            Ok((response.name, model_from_targets(&targets)))
        }
        PresetRunKind::Api => {
            let (response, preset) =
                apply_api_preset_locked(state, preset_id, project_path).await?;
            Ok((response.name, api_preset_model(&preset).map(str::to_string)))
        }
        PresetRunKind::Claude => {
            let (response, preset) = apply_claude_preset_locked(state, preset_id).await?;
            let model = preset
                .third_party_model
                .clone()
                .or(preset.default_sonnet_model.clone())
                .or(preset.default_opus_model.clone())
                .or(preset.default_haiku_model.clone());
            Ok((response.name, model))
        }
    }
}

fn preset_name(state: &AppState, kind: PresetRunKind, preset_id: &str) -> ApiResult<String> {
    let preset_id = preset_id.trim();
    if preset_id.is_empty() {
        return Err(AppError::bad_request("preset_id 不能为空。"));
    }
    let name = match kind {
        PresetRunKind::OAuth => state
            .auth_manager
            .auth_presets_snapshot()
            .into_iter()
            .find(|preset| preset.id == preset_id)
            .map(|preset| preset.name),
        PresetRunKind::Api => state
            .auth_manager
            .api_presets_snapshot()
            .into_iter()
            .find(|preset| preset.id == preset_id)
            .map(|preset| preset.name),
        PresetRunKind::Claude => state
            .auth_manager
            .claude_presets_snapshot()
            .into_iter()
            .find(|preset| preset.id == preset_id)
            .map(|preset| preset.name),
    };
    name.ok_or_else(|| AppError::not_found("找不到指定的预设。"))
}

fn target_config_overrides(
    state: &AppState,
    kind: PresetRunKind,
    preset_id: &str,
) -> ApiResult<Vec<ResolvedConfigTarget>> {
    let defaults = codex_default_pairs(state);
    let default_pairs = defaults
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect::<Vec<_>>();
    let overrides: Vec<PresetConfigOverride> = match kind {
        PresetRunKind::OAuth => {
            state
                .auth_manager
                .auth_presets_snapshot()
                .into_iter()
                .find(|preset| preset.id == preset_id)
                .ok_or_else(|| AppError::not_found("找不到指定的 OAuth 预设。"))?
                .config_overrides
        }
        PresetRunKind::Api => {
            state
                .auth_manager
                .api_presets_snapshot()
                .into_iter()
                .find(|preset| preset.id == preset_id)
                .ok_or_else(|| AppError::not_found("找不到指定的 API 预设。"))?
                .config_overrides
        }
        PresetRunKind::Claude => return Ok(Vec::new()),
    };
    resolve_effective_preset_config_targets(&default_pairs, &overrides)
        .map_err(|error| AppError::bad_request(format!("预设 config 覆盖无效: {error}")))
}

fn codex_default_pairs(state: &AppState) -> Vec<(String, String)> {
    state
        .workspace_settings
        .codex_default_config_entries()
        .into_iter()
        .map(|entry| (entry.key, entry.value))
        .collect()
}

fn model_from_targets(targets: &[ResolvedConfigTarget]) -> Option<String> {
    targets
        .iter()
        .rev()
        .find(|target| target.key.eq_ignore_ascii_case("model"))
        .map(|target| target.value.trim_matches(['\'', '"']).trim().to_string())
        .filter(|value| !value.is_empty())
}

async fn capture_snapshot(
    state: &AppState,
    kind: PresetRunKind,
    project_path: &str,
    target_config: &[ResolvedConfigTarget],
) -> ApiResult<PresetConfigSnapshot> {
    let mut paths = BTreeSet::new();
    for target in terminal_auth_write_targets(state)? {
        match kind {
            PresetRunKind::OAuth | PresetRunKind::Api => {
                collect_codex_paths(
                    &mut paths,
                    &target.config_file,
                    &target.auth_file,
                    target_config,
                )
                .await?;
            }
            PresetRunKind::Claude => {
                paths.insert(target.claude_settings_file.clone());
                if let Some(home) = target.claude_settings_file.parent().and_then(Path::parent) {
                    paths.insert(home.join(auth_core::CLAUDE_ONBOARDING_BYPASS_FILE));
                }
            }
        }
    }
    if matches!(kind, PresetRunKind::OAuth | PresetRunKind::Api)
        && let Some(local_config) = find_local_codex_config(project_path)
    {
        let local_auth = local_config.with_file_name("auth.json");
        collect_codex_paths(&mut paths, &local_config, &local_auth, target_config).await?;
    }

    let mut files = Vec::with_capacity(paths.len());
    for path in paths {
        files.push(capture_file(path).await?);
    }
    Ok(PresetConfigSnapshot {
        files,
        upstream_proxy_settings: state.auth_manager.upstream_proxy_settings(),
    })
}

async fn collect_codex_paths(
    paths: &mut BTreeSet<PathBuf>,
    config_path: &Path,
    auth_path: &Path,
    target_config: &[ResolvedConfigTarget],
) -> ApiResult<()> {
    paths.insert(config_path.to_path_buf());
    paths.insert(auth_path.to_path_buf());
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    paths.insert(config_dir.join("model_catalog.json"));

    if let Ok(content) = fs::read_to_string(config_path).await
        && let Some(path) = model_catalog_path_from_content(&content, config_path)?
    {
        paths.insert(path);
    }
    if let Some(target) = target_config
        .iter()
        .rev()
        .find(|target| target.key.eq_ignore_ascii_case("model_catalog_json"))
        && let Some(path) = model_catalog_path_from_value(&target.value, config_path)?
    {
        paths.insert(path);
    }
    Ok(())
}

fn model_catalog_path_from_content(
    content: &str,
    config_path: &Path,
) -> ApiResult<Option<PathBuf>> {
    if content.trim().is_empty() {
        return Ok(None);
    }
    let document = content
        .parse::<DocumentMut>()
        .map_err(|error| AppError::internal(format!("读取原 config.toml 模型目录失败: {error}")))?;
    Ok(document
        .get("model_catalog_json")
        .and_then(|item| item.as_str())
        .and_then(|value| resolve_catalog_path(value, config_path)))
}

fn model_catalog_path_from_value(value: &str, config_path: &Path) -> ApiResult<Option<PathBuf>> {
    let document = format!("value = {value}\n")
        .parse::<DocumentMut>()
        .map_err(|error| AppError::bad_request(format!("model_catalog_json 值无效: {error}")))?;
    Ok(document
        .get("value")
        .and_then(|item| item.as_str())
        .and_then(|value| resolve_catalog_path(value, config_path)))
}

fn resolve_catalog_path(value: &str, config_path: &Path) -> Option<PathBuf> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    let path = PathBuf::from(value);
    Some(if path.is_absolute() {
        path
    } else {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(path)
    })
}

async fn capture_file(path: PathBuf) -> ApiResult<FileSnapshot> {
    match fs::metadata(&path).await {
        Ok(metadata) => {
            let contents = fs::read(&path).await.map_err(|error| {
                AppError::internal(format!("读取原配置 {} 失败: {error}", path.display()))
            })?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::{MetadataExt, PermissionsExt};
                Ok(FileSnapshot {
                    path,
                    contents: Some(contents),
                    mode: Some(metadata.permissions().mode()),
                    uid: Some(metadata.uid()),
                    gid: Some(metadata.gid()),
                })
            }
            #[cfg(not(unix))]
            {
                Ok(FileSnapshot {
                    path,
                    contents: Some(contents),
                    mode: None,
                    uid: None,
                    gid: None,
                })
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(FileSnapshot {
            path,
            contents: None,
            mode: None,
            uid: None,
            gid: None,
        }),
        Err(error) => Err(AppError::internal(format!("读取原配置元数据失败: {error}"))),
    }
}

async fn restore_snapshot(state: &AppState, snapshot: &PresetConfigSnapshot) -> ApiResult<()> {
    for file in &snapshot.files {
        restore_file(file).await?;
    }
    persist_upstream_proxy_settings(&state.auth_manager, snapshot.upstream_proxy_settings.clone())
        .map_err(|error| AppError::internal(format!("恢复原代理设置失败: {error}")))
}

async fn restore_file(snapshot: &FileSnapshot) -> ApiResult<()> {
    match snapshot.contents.as_ref() {
        Some(contents) => {
            if let Some(parent) = snapshot.path.parent() {
                fs::create_dir_all(parent).await.map_err(|error| {
                    AppError::internal(format!("创建原配置目录 {} 失败: {error}", parent.display()))
                })?;
            }
            fs::write(&snapshot.path, contents).await.map_err(|error| {
                AppError::internal(format!("恢复原配置 {} 失败: {error}", snapshot.path.display()))
            })?;
            restore_file_metadata(snapshot)?;
        }
        None => match fs::remove_file(&snapshot.path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(AppError::internal(format!(
                    "删除租约期间生成的配置 {} 失败: {error}",
                    snapshot.path.display()
                )));
            }
        },
    }
    Ok(())
}

#[cfg(unix)]
fn restore_file_metadata(snapshot: &FileSnapshot) -> ApiResult<()> {
    use std::{
        ffi::CString,
        os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    };
    if let Some(mode) = snapshot.mode {
        std::fs::set_permissions(&snapshot.path, std::fs::Permissions::from_mode(mode))
            .map_err(|error| AppError::internal(format!("恢复原配置权限失败: {error}")))?;
    }
    if let (Some(uid), Some(gid)) = (snapshot.uid, snapshot.gid) {
        let encoded = CString::new(snapshot.path.as_os_str().as_bytes())
            .map_err(|_| AppError::internal("原配置路径包含 NUL。"))?;
        if unsafe { libc::chown(encoded.as_ptr(), uid, gid) } != 0 {
            return Err(AppError::internal(format!(
                "恢复原配置所有者失败: {}",
                std::io::Error::last_os_error()
            )));
        }
    }
    Ok(())
}

#[cfg(not(unix))]
fn restore_file_metadata(_snapshot: &FileSnapshot) -> ApiResult<()> {
    Ok(())
}

async fn write_journal(path: &Path, journal: &PresetRunLeaseJournal) -> ApiResult<()> {
    let contents = serde_json::to_vec(journal)
        .map_err(|error| AppError::internal(format!("编码预设租约恢复日志失败: {error}")))?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|error| AppError::internal(format!("创建预设租约日志目录失败: {error}")))?;
    }
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, contents)
        .await
        .map_err(|error| AppError::internal(format!("写入预设租约恢复日志失败: {error}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))
            .await
            .map_err(|error| AppError::internal(format!("设置预设租约日志权限失败: {error}")))?;
    }
    fs::rename(&temp, path)
        .await
        .map_err(|error| AppError::internal(format!("提交预设租约恢复日志失败: {error}")))
}

async fn read_journal(path: &Path) -> ApiResult<Option<PresetRunLeaseJournal>> {
    let contents = match fs::read(path).await {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(AppError::internal(format!("读取预设租约恢复日志失败: {error}")));
        }
    };
    serde_json::from_slice(&contents)
        .map(Some)
        .map_err(|error| AppError::internal(format!("解析预设租约恢复日志失败: {error}")))
}

async fn remove_journal(path: &Path) -> ApiResult<()> {
    match fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::internal(format!("删除预设租约恢复日志失败: {error}"))),
    }
}

fn normalize_owner(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        return "未命名 Agent".to_string();
    }
    value.chars().take(120).collect()
}

fn generate_lease_id() -> String {
    let nonce: u64 = rand::thread_rng().r#gen();
    format!("{:x}-{nonce:016x}", terminal_core::current_timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn file_snapshot_restores_existing_content_and_removes_created_file() {
        let root = std::env::temp_dir().join(format!("webclx-lease-test-{}", generate_lease_id()));
        fs::create_dir_all(&root).await.unwrap();
        let existing = root.join("existing.toml");
        let created = root.join("created.json");
        fs::write(&existing, b"original").await.unwrap();
        let existing_snapshot = capture_file(existing.clone()).await.unwrap();
        let created_snapshot = capture_file(created.clone()).await.unwrap();

        fs::write(&existing, b"switched").await.unwrap();
        fs::write(&created, b"generated").await.unwrap();
        restore_file(&existing_snapshot).await.unwrap();
        restore_file(&created_snapshot).await.unwrap();

        assert_eq!(fs::read(&existing).await.unwrap(), b"original");
        assert!(!created.exists());
        fs::remove_dir_all(root).await.ok();
    }
}
