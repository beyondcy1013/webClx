use std::{
    collections::HashMap,
    ffi::CString,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use auth_core::{
    ApiPresetLookup, StoredApiPreset, api_preset_enables_local_upstream_proxy_on_apply,
    api_preset_model, api_provider_base_url_for_mode, select_api_preset_index,
};
use axum::{
    Json,
    extract::{Path as AxumPath, State},
    http::StatusCode,
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use terminal_core::current_timestamp_millis;
use tokio::{
    fs,
    process::{Child, Command},
    sync::{RwLock, Semaphore},
    time::sleep,
};
use tracing::warn;

use crate::{ApiResult, AppError, AppState, auth, filesystem, runtime_paths};

const DEFAULT_TIMEOUT_SECS: u64 = 1_800;
const MAX_TIMEOUT_SECS: u64 = 7_200;
const MAX_TASK_BYTES: usize = 128 * 1024;
const MAX_SCHEMA_BYTES: usize = 64 * 1024;
const MAX_RESULT_BYTES: usize = 512 * 1024;
const MAX_TRANSCRIPT_BYTES: usize = 64 * 1024;
const MAX_LISTED_TASKS: usize = 100;
const MAX_CONCURRENT_TASKS: usize = 4;
const LAUNCH_VERIFY_TIMEOUT: Duration = Duration::from_secs(45);
const MONITOR_INTERVAL: Duration = Duration::from_millis(250);
const LEASE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
const TASK_ROOT_NAME: &str = ".webclx-codex-tasks";
const NETWORK_ENV_KEYS: [&str; 8] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];
const PROTECTED_ENV_KEYS: [&str; 7] = ["HOME", "PATH", "USER", "LOGNAME", "SHELL", "PWD", "OLDPWD"];

static TASK_MANAGER: OnceLock<Arc<CodexTaskManager>> = OnceLock::new();
static TASK_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexTaskMode {
    Exec,
    Terminal,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodexTaskStatus {
    Queued,
    ApplyingPreset,
    Starting,
    Running,
    Collecting,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
}

impl CodexTaskStatus {
    pub(crate) fn is_final(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::TimedOut | Self::Cancelled)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CodexTaskPresetSelector {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    model: Option<String>,
}

impl CodexTaskPresetSelector {
    pub(crate) fn by_id(id: impl Into<String>) -> Self {
        Self {
            id: Some(id.into()),
            name: None,
            model: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateCodexTaskRequest {
    mode: CodexTaskMode,
    preset: CodexTaskPresetSelector,
    cwd: String,
    task: String,
    #[serde(default)]
    timeout_secs: Option<u64>,
    #[serde(default)]
    output_schema: Option<Value>,
}

impl CreateCodexTaskRequest {
    pub(crate) fn new(
        mode: CodexTaskMode,
        preset: CodexTaskPresetSelector,
        cwd: String,
        task: String,
        timeout_secs: Option<u64>,
        output_schema: Option<Value>,
    ) -> Self {
        Self {
            mode,
            preset,
            cwd,
            task,
            timeout_secs,
            output_schema,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexTaskPresetSummary {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodexTaskRecord {
    pub(crate) id: String,
    pub(crate) mode: CodexTaskMode,
    pub(crate) status: CodexTaskStatus,
    pub(crate) preset: CodexTaskPresetSummary,
    pub(crate) cwd: String,
    pub(crate) timeout_secs: u64,
    pub(crate) created_at: u64,
    pub(crate) updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) started_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) finished_at: Option<u64>,
    #[serde(default)]
    pub(crate) cancel_requested: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) terminal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) terminal_name: Option<String>,
    #[serde(default)]
    pub(crate) terminal_closed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) codex_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) actual_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) exit_code: Option<i32>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) structured_output: Option<Value>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub(crate) transcript_tail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CodexTaskListResponse {
    tasks: Vec<CodexTaskRecord>,
}

#[derive(Debug, Clone)]
struct CodexTaskPaths {
    run_dir: PathBuf,
    task_file: PathBuf,
    schema_file: Option<PathBuf>,
    result_file: PathBuf,
    transcript_file: PathBuf,
    status_file: PathBuf,
}

struct CodexTaskManager {
    records_dir: PathBuf,
    runs_dir: PathBuf,
    records: RwLock<HashMap<String, CodexTaskRecord>>,
    cancellations: Mutex<HashMap<String, Arc<AtomicBool>>>,
    semaphore: Arc<Semaphore>,
}

impl CodexTaskManager {
    fn new(app_dir: &Path) -> Self {
        let root = app_dir.join(TASK_ROOT_NAME);
        let records_dir = root.join("records");
        let runs_dir = root.join("runs");
        let _ = std::fs::create_dir_all(&records_dir);
        let _ = std::fs::create_dir_all(&runs_dir);
        let records = load_task_records(&records_dir);
        Self {
            records_dir,
            runs_dir,
            records: RwLock::new(records),
            cancellations: Mutex::new(HashMap::new()),
            semaphore: Arc::new(Semaphore::new(MAX_CONCURRENT_TASKS)),
        }
    }

    async fn insert(&self, record: CodexTaskRecord) -> ApiResult<()> {
        self.records
            .write()
            .await
            .insert(record.id.clone(), record.clone());
        self.persist(&record).await
    }

    async fn get(&self, task_id: &str) -> Option<CodexTaskRecord> {
        self.records.read().await.get(task_id).cloned()
    }

    async fn list(&self) -> Vec<CodexTaskRecord> {
        let mut records = self
            .records
            .read()
            .await
            .values()
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by(|left, right| right.created_at.cmp(&left.created_at));
        records.truncate(MAX_LISTED_TASKS);
        records
    }

    async fn update<F>(&self, task_id: &str, update: F) -> Option<CodexTaskRecord>
    where
        F: FnOnce(&mut CodexTaskRecord),
    {
        let record = {
            let mut records = self.records.write().await;
            let record = records.get_mut(task_id)?;
            update(record);
            record.updated_at = current_timestamp_millis();
            record.clone()
        };
        if let Err(error) = self.persist(&record).await {
            warn!(task_id, "failed to persist Codex task update: {error}");
        }
        Some(record)
    }

    async fn persist(&self, record: &CodexTaskRecord) -> ApiResult<()> {
        fs::create_dir_all(&self.records_dir)
            .await
            .map_err(|error| AppError::internal(format!("创建 Codex 任务记录目录失败: {error}")))?;
        let final_path = self.records_dir.join(format!("{}.json", record.id));
        let temp_path = self.records_dir.join(format!(".{}.tmp", record.id));
        let content = serde_json::to_vec_pretty(record)
            .map_err(|error| AppError::internal(format!("序列化 Codex 任务失败: {error}")))?;
        fs::write(&temp_path, content)
            .await
            .map_err(|error| AppError::internal(format!("写入 Codex 任务记录失败: {error}")))?;
        set_owner_only_mode(&temp_path, 0o600)?;
        fs::rename(&temp_path, &final_path)
            .await
            .map_err(|error| AppError::internal(format!("提交 Codex 任务记录失败: {error}")))
    }

    fn cancellation(&self, task_id: &str) -> Arc<AtomicBool> {
        let mut cancellations = crate::lock_or_recover!(self.cancellations.lock());
        cancellations
            .entry(task_id.to_string())
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone()
    }

    fn remove_cancellation(&self, task_id: &str) {
        crate::lock_or_recover!(self.cancellations.lock()).remove(task_id);
    }

    fn paths(&self, task_id: &str, has_schema: bool) -> CodexTaskPaths {
        let run_dir = self.runs_dir.join(task_id);
        CodexTaskPaths {
            task_file: run_dir.join("task.txt"),
            schema_file: has_schema.then(|| run_dir.join("output-schema.json")),
            result_file: run_dir.join("last-message.txt"),
            transcript_file: run_dir.join("transcript.log"),
            status_file: run_dir.join("exit-status"),
            run_dir,
        }
    }
}

fn task_manager(state: &AppState) -> Arc<CodexTaskManager> {
    TASK_MANAGER
        .get_or_init(|| Arc::new(CodexTaskManager::new(&state.app_dir)))
        .clone()
}

pub async fn list_tasks(State(state): State<AppState>) -> Json<CodexTaskListResponse> {
    Json(CodexTaskListResponse {
        tasks: task_manager(&state).list().await,
    })
}

pub async fn get_task(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> ApiResult<Json<CodexTaskRecord>> {
    task_manager(&state)
        .get(&task_id)
        .await
        .map(Json)
        .ok_or_else(|| AppError::not_found("找不到指定的 Codex 任务。"))
}

pub async fn create_task(
    State(state): State<AppState>,
    Json(payload): Json<CreateCodexTaskRequest>,
) -> ApiResult<Json<CodexTaskRecord>> {
    submit_task(&state, payload).await.map(Json)
}

async fn submit_task(
    state: &AppState,
    payload: CreateCodexTaskRequest,
) -> ApiResult<CodexTaskRecord> {
    validate_request(&payload)?;
    let cwd = filesystem::resolve_terminal_directory_path(&state.workspace_root(), &payload.cwd)?;
    let presets = state.auth_manager.api_presets_snapshot();
    let preset = resolve_preset(&presets, &payload.preset)?;
    let timeout_secs = payload.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    let task_id = generate_task_id();
    let manager = task_manager(&state);
    let user = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("终端用户无效: {error}")))?;
    let paths = manager.paths(&task_id, payload.output_schema.is_some());
    if let Err(error) =
        prepare_task_files(&paths, &payload.task, payload.output_schema.as_ref(), &user).await
    {
        return Err(error);
    }

    let now = current_timestamp_millis();
    let record = CodexTaskRecord {
        id: task_id.clone(),
        mode: payload.mode,
        status: CodexTaskStatus::Queued,
        preset: preset_summary(&preset),
        cwd: cwd.display().to_string(),
        timeout_secs,
        created_at: now,
        updated_at: now,
        started_at: None,
        finished_at: None,
        cancel_requested: false,
        terminal_id: None,
        terminal_name: None,
        terminal_closed: false,
        codex_session_id: None,
        actual_model: None,
        exit_code: None,
        result: String::new(),
        structured_output: None,
        transcript_tail: String::new(),
        error: None,
    };
    if let Err(error) = manager.insert(record.clone()).await {
        return Err(error);
    }
    let cancellation = manager.cancellation(&task_id);
    let task_state = state.clone();
    let task_manager = manager.clone();
    tokio::spawn(async move {
        run_task(task_state, task_manager, task_id, preset, paths, cancellation).await;
    });

    Ok(record)
}

pub(crate) async fn submit_task_and_wait(
    state: &AppState,
    payload: CreateCodexTaskRequest,
) -> ApiResult<CodexTaskRecord> {
    let record = submit_task(state, payload).await?;
    let manager = task_manager(state);
    loop {
        let current = manager
            .get(&record.id)
            .await
            .ok_or_else(|| AppError::internal("Codex 任务记录在执行期间丢失。"))?;
        if current.status.is_final() {
            return Ok(current);
        }
        sleep(MONITOR_INTERVAL).await;
    }
}

pub async fn cancel_task(
    State(state): State<AppState>,
    AxumPath(task_id): AxumPath<String>,
) -> ApiResult<Json<CodexTaskRecord>> {
    let manager = task_manager(&state);
    let record = manager
        .get(&task_id)
        .await
        .ok_or_else(|| AppError::not_found("找不到指定的 Codex 任务。"))?;
    if record.status.is_final() {
        return Ok(Json(record));
    }
    manager.cancellation(&task_id).store(true, Ordering::SeqCst);
    let updated = manager
        .update(&task_id, |record| record.cancel_requested = true)
        .await
        .ok_or_else(|| AppError::not_found("找不到指定的 Codex 任务。"))?;
    Ok(Json(updated))
}

pub async fn recover_interrupted_tasks(state: &AppState) {
    let manager = task_manager(state);
    for record in manager.list().await {
        if record.status.is_final() {
            continue;
        }
        let terminal_closed = close_owned_terminal(state, record.terminal_id.as_deref()).await;
        manager
            .update(&record.id, |record| {
                record.status = CodexTaskStatus::Failed;
                record.finished_at = Some(current_timestamp_millis());
                record.terminal_closed = terminal_closed;
                record.error = Some("webClx 重启中断了正在执行的 Codex 任务。".to_string());
            })
            .await;
    }
}

async fn run_task(
    state: AppState,
    manager: Arc<CodexTaskManager>,
    task_id: String,
    preset: StoredApiPreset,
    paths: CodexTaskPaths,
    cancellation: Arc<AtomicBool>,
) {
    let permit = loop {
        if cancellation.load(Ordering::SeqCst) {
            finish_cancelled(&state, &manager, &task_id, None).await;
            manager.remove_cancellation(&task_id);
            return;
        }
        match manager.semaphore.clone().try_acquire_owned() {
            Ok(permit) => break permit,
            Err(_) => sleep(MONITOR_INTERVAL).await,
        }
    };
    let _permit = permit;
    let started_at = current_timestamp_millis();
    manager
        .update(&task_id, |record| {
            record.status = CodexTaskStatus::ApplyingPreset;
            record.started_at = Some(started_at);
        })
        .await;
    let started = Instant::now();
    let lease = loop {
        if cancellation.load(Ordering::SeqCst) {
            finish_cancelled(&state, &manager, &task_id, None).await;
            manager.remove_cancellation(&task_id);
            return;
        }
        match auth::begin_preset_run_lease(
            &state,
            auth::PresetRunKind::Api,
            &preset.id,
            &manager
                .get(&task_id)
                .await
                .map(|record| record.cwd)
                .unwrap_or_default(),
            &format!("Codex 任务 {task_id}"),
        )
        .await
        {
            Ok(lease) => break lease,
            Err(error) if error.status == StatusCode::CONFLICT => {
                if started.elapsed()
                    >= Duration::from_secs(
                        manager
                            .get(&task_id)
                            .await
                            .map(|record| record.timeout_secs)
                            .unwrap_or(DEFAULT_TIMEOUT_SECS),
                    )
                {
                    finish_failed(&manager, &task_id, "等待全局预设门禁超时。".to_string(), true)
                        .await;
                    manager.remove_cancellation(&task_id);
                    return;
                }
                sleep(MONITOR_INTERVAL).await;
            }
            Err(error) => {
                finish_failed(&manager, &task_id, error.to_string(), true).await;
                manager.remove_cancellation(&task_id);
                return;
            }
        }
    };
    let mut last_lease_heartbeat = Instant::now();
    let mut direct_child: Option<Child> = None;
    let mut terminal_id: Option<String> = None;

    let launch = async {
        if cancellation.load(Ordering::SeqCst) {
            return Err("任务在启动前已取消。".to_string());
        }
        manager
            .update(&task_id, |record| {
                record.status = CodexTaskStatus::Starting;
                record.preset = preset_summary(&preset);
            })
            .await;

        let record = manager
            .get(&task_id)
            .await
            .ok_or_else(|| "Codex 任务记录丢失。".to_string())?;
        match record.mode {
            CodexTaskMode::Exec => {
                direct_child = Some(
                    spawn_direct_codex(&state, &paths, record.mode, Path::new(&record.cwd))
                        .await
                        .map_err(|error| error.to_string())?,
                );
            }
            CodexTaskMode::Terminal => {
                let terminal = create_task_terminal(&state, &preset, PathBuf::from(&record.cwd))
                    .await
                    .map_err(|error| error.to_string())?;
                terminal_id = Some(terminal.0.clone());
                manager
                    .update(&task_id, |record| {
                        record.terminal_id = Some(terminal.0.clone());
                        record.terminal_name = Some(terminal.1.clone());
                    })
                    .await;
                let user = state
                    .workspace_settings
                    .terminal_user_profile()
                    .map_err(|error| format!("终端用户无效: {error}"))?;
                let codex =
                    auth::resolve_codex_executable(&auth::codex_command_path_for_user(&user));
                let command = render_terminal_codex_command(
                    &paths,
                    record.mode,
                    Path::new(&record.cwd),
                    &codex,
                );
                state
                    .terminal_manager
                    .send_session_input_silent(&terminal.0, command)
                    .map_err(|error| format!("启动任务终端命令失败: {error}"))?;
            }
        }

        let expected_model = lease.model.as_deref().or_else(|| api_preset_model(&preset));
        let launch_info = await_verified_launch(
            &state,
            &lease,
            &mut last_lease_heartbeat,
            &paths,
            expected_model,
            &cancellation,
            started,
            record.timeout_secs,
            &mut direct_child,
        )
        .await?;
        manager
            .update(&task_id, |record| {
                record.status = CodexTaskStatus::Running;
                record.actual_model = Some(launch_info.model.clone());
                record.codex_session_id = launch_info.session_id.clone();
            })
            .await;
        Ok::<(), String>(())
    }
    .await;

    if let Err(error) = launch {
        terminate_direct_child(&mut direct_child).await;
        let terminal_closed = close_owned_terminal(&state, terminal_id.as_deref()).await;
        let restore_error = auth::release_preset_run_lease_internal(&state, &lease.id)
            .await
            .err();
        if let Some(restore_error) = restore_error {
            finish_failed(
                &manager,
                &task_id,
                format!("{error}；恢复原配置失败: {restore_error}"),
                terminal_closed,
            )
            .await;
        } else if cancellation.load(Ordering::SeqCst) {
            finish_cancelled(&state, &manager, &task_id, terminal_id.as_deref()).await;
        } else {
            finish_failed(&manager, &task_id, error, terminal_closed).await;
        }
        manager.remove_cancellation(&task_id);
        return;
    }

    let mut outcome = monitor_task(
        &state,
        &lease,
        &mut last_lease_heartbeat,
        &paths,
        &cancellation,
        started,
        manager
            .get(&task_id)
            .await
            .map(|record| record.timeout_secs)
            .unwrap_or(DEFAULT_TIMEOUT_SECS),
        &mut direct_child,
    )
    .await;
    let terminal_closed = close_owned_terminal(&state, terminal_id.as_deref()).await;
    if let Err(error) = auth::release_preset_run_lease_internal(&state, &lease.id).await {
        outcome = MonitorOutcome::LeaseFailed(format!("恢复原配置失败: {error}"));
    }

    manager
        .update(&task_id, |record| record.status = CodexTaskStatus::Collecting)
        .await;
    let result = read_limited(&paths.result_file, MAX_RESULT_BYTES).await;
    let transcript_tail = read_tail(&paths.transcript_file, MAX_TRANSCRIPT_BYTES).await;
    let structured_output = manager.get(&task_id).await.and_then(|record| {
        paths
            .schema_file
            .as_ref()
            .and_then(|_| parse_structured_output(&result))
            .or(record.structured_output)
    });
    let now = current_timestamp_millis();

    match outcome {
        MonitorOutcome::Exited(exit_code) => {
            let structured_ok = paths.schema_file.is_none() || structured_output.is_some();
            let ok = exit_code == 0 && !result.trim().is_empty() && structured_ok;
            manager
                .update(&task_id, |record| {
                    record.status = if ok {
                        CodexTaskStatus::Succeeded
                    } else {
                        CodexTaskStatus::Failed
                    };
                    record.finished_at = Some(now);
                    record.exit_code = Some(exit_code);
                    record.result = result.clone();
                    record.structured_output = structured_output.clone();
                    record.transcript_tail = transcript_tail.clone();
                    record.terminal_closed = terminal_closed;
                    record.error = (!ok).then(|| {
                        if !terminal_closed {
                            "Codex 任务结束，但任务终端关闭失败。".to_string()
                        } else if exit_code != 0 {
                            format!("codex exec 退出码为 {exit_code}")
                        } else if result.trim().is_empty() {
                            "codex exec 未返回最终消息。".to_string()
                        } else {
                            "codex exec 最终消息不是有效 JSON。".to_string()
                        }
                    });
                })
                .await;
        }
        MonitorOutcome::Cancelled => {
            finish_cancelled(&state, &manager, &task_id, terminal_id.as_deref()).await;
        }
        MonitorOutcome::TimedOut => {
            manager
                .update(&task_id, |record| {
                    record.status = CodexTaskStatus::TimedOut;
                    record.finished_at = Some(now);
                    record.result = result;
                    record.transcript_tail = transcript_tail;
                    record.terminal_closed = terminal_closed;
                    record.error =
                        Some(format!("codex exec 超过 {} 秒，已终止。", record.timeout_secs));
                })
                .await;
        }
        MonitorOutcome::RunnerFailed(error) => {
            finish_failed(&manager, &task_id, error, terminal_closed).await;
        }
        MonitorOutcome::LeaseFailed(error) => {
            finish_failed(&manager, &task_id, error, terminal_closed).await;
        }
    }
    manager.remove_cancellation(&task_id);
}

#[derive(Debug)]
struct LaunchInfo {
    model: String,
    session_id: Option<String>,
}

async fn await_verified_launch(
    state: &AppState,
    lease: &auth::AcquiredPresetRunLease,
    last_lease_heartbeat: &mut Instant,
    paths: &CodexTaskPaths,
    expected_model: Option<&str>,
    cancellation: &AtomicBool,
    started: Instant,
    timeout_secs: u64,
    direct_child: &mut Option<Child>,
) -> Result<LaunchInfo, String> {
    let deadline = Instant::now() + LAUNCH_VERIFY_TIMEOUT;
    loop {
        heartbeat_lease_if_due(state, lease, last_lease_heartbeat).await?;
        if cancellation.load(Ordering::SeqCst) {
            return Err("任务启动期间已取消。".to_string());
        }
        if started.elapsed() >= Duration::from_secs(timeout_secs) {
            return Err("任务在 Codex 启动确认前已超时。".to_string());
        }
        let transcript = read_limited(&paths.transcript_file, MAX_TRANSCRIPT_BYTES).await;
        let (model, session_id) = parse_launch_banner(&transcript);
        if let Some(model) = model {
            if expected_model.is_some_and(|expected| !expected.eq_ignore_ascii_case(&model)) {
                return Err(format!(
                    "Codex 实际模型 `{model}` 与预设模型 `{}` 不一致；请检查项目级配置覆盖。",
                    expected_model.unwrap_or_default()
                ));
            }
            return Ok(LaunchInfo { model, session_id });
        }
        if paths.status_file.is_file() {
            return Err(format!(
                "Codex 在报告实际模型前退出。{}",
                transcript_error_suffix(&transcript)
            ));
        }
        if let Some(child) = direct_child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return Err(format!(
                        "Codex 在报告实际模型前退出，进程状态: {status}。{}",
                        transcript_error_suffix(&transcript)
                    ));
                }
                Ok(None) => {}
                Err(error) => return Err(format!("读取 Codex 启动状态失败: {error}")),
            }
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "等待 Codex 报告实际模型超时。{}",
                transcript_error_suffix(&transcript)
            ));
        }
        sleep(MONITOR_INTERVAL).await;
    }
}

async fn heartbeat_lease_if_due(
    state: &AppState,
    lease: &auth::AcquiredPresetRunLease,
    last_heartbeat: &mut Instant,
) -> Result<(), String> {
    if last_heartbeat.elapsed() < LEASE_HEARTBEAT_INTERVAL {
        return Ok(());
    }
    auth::heartbeat_preset_run_lease_internal(state, &lease.id)
        .await
        .map_err(|error| format!("全局预设门禁续租失败: {error}"))?;
    *last_heartbeat = Instant::now();
    Ok(())
}

enum MonitorOutcome {
    Exited(i32),
    Cancelled,
    TimedOut,
    RunnerFailed(String),
    LeaseFailed(String),
}

fn direct_child_exit_outcome(status: std::process::ExitStatus) -> MonitorOutcome {
    if let Some(exit_code) = status.code() {
        return MonitorOutcome::Exited(exit_code);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;

        if let Some(signal) = status.signal() {
            return MonitorOutcome::RunnerFailed(format!(
                "原生 Codex runner 被 signal {signal} 终止，进程状态: {status}"
            ));
        }
    }

    MonitorOutcome::RunnerFailed(format!(
        "原生 Codex runner 退出但没有可用退出码，进程状态: {status}"
    ))
}

async fn monitor_task(
    state: &AppState,
    lease: &auth::AcquiredPresetRunLease,
    last_lease_heartbeat: &mut Instant,
    paths: &CodexTaskPaths,
    cancellation: &AtomicBool,
    started: Instant,
    timeout_secs: u64,
    direct_child: &mut Option<Child>,
) -> MonitorOutcome {
    loop {
        if let Err(error) = heartbeat_lease_if_due(state, lease, last_lease_heartbeat).await {
            terminate_direct_child(direct_child).await;
            return MonitorOutcome::LeaseFailed(error);
        }
        if cancellation.load(Ordering::SeqCst) {
            terminate_direct_child(direct_child).await;
            return MonitorOutcome::Cancelled;
        }
        if started.elapsed() >= Duration::from_secs(timeout_secs) {
            terminate_direct_child(direct_child).await;
            return MonitorOutcome::TimedOut;
        }
        if let Some(child) = direct_child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    return direct_child_exit_outcome(status);
                }
                Ok(None) => {}
                Err(error) => {
                    return MonitorOutcome::RunnerFailed(format!(
                        "读取 Codex runner 状态失败: {error}"
                    ));
                }
            }
        } else if let Ok(status) = fs::read_to_string(&paths.status_file).await
            && let Ok(exit_code) = status.trim().parse::<i32>()
        {
            return MonitorOutcome::Exited(exit_code);
        }
        sleep(MONITOR_INTERVAL).await;
    }
}

async fn prepare_task_files(
    paths: &CodexTaskPaths,
    task: &str,
    output_schema: Option<&Value>,
    user: &runtime_paths::UserProfile,
) -> ApiResult<()> {
    fs::create_dir_all(&paths.run_dir)
        .await
        .map_err(|error| AppError::internal(format!("创建 Codex 任务目录失败: {error}")))?;
    set_user_owned_path_mode(&paths.run_dir, user, 0o700)?;
    write_user_file(&paths.task_file, task.as_bytes(), user, 0o600).await?;
    if let (Some(schema), Some(schema_file)) = (output_schema, paths.schema_file.as_ref()) {
        let content = serde_json::to_vec_pretty(schema)
            .map_err(|error| AppError::bad_request(format!("output_schema 无效: {error}")))?;
        write_user_file(schema_file, &content, user, 0o600).await?;
    }
    for path in [
        &paths.result_file,
        &paths.transcript_file,
        &paths.status_file,
    ] {
        match fs::remove_file(path).await {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(AppError::internal(format!(
                    "清理 Codex 任务输出 {} 失败: {error}",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn append_codex_exec_args(
    command: &mut Command,
    paths: &CodexTaskPaths,
    mode: CodexTaskMode,
    cwd: &Path,
) {
    command.arg("exec");
    if matches!(mode, CodexTaskMode::Exec) {
        command.arg("--ephemeral");
    }
    command
        .arg("--skip-git-repo-check")
        .arg("--color")
        .arg("never")
        .arg("--output-last-message")
        .arg(&paths.result_file)
        .arg("--cd")
        .arg(cwd);
    if let Some(schema_file) = paths.schema_file.as_ref() {
        command.arg("--output-schema").arg(schema_file);
    }
    command.arg("-");
}

fn render_terminal_codex_command(
    paths: &CodexTaskPaths,
    mode: CodexTaskMode,
    cwd: &Path,
    codex: &Path,
) -> String {
    let mut arguments = vec!["exec".to_string()];
    if matches!(mode, CodexTaskMode::Exec) {
        arguments.push("--ephemeral".to_string());
    }
    arguments.extend([
        "--skip-git-repo-check".to_string(),
        "--color".to_string(),
        "never".to_string(),
        "--output-last-message".to_string(),
        shell_quote(&paths.result_file.display().to_string()),
        "--cd".to_string(),
        shell_quote(&cwd.display().to_string()),
    ]);
    if let Some(schema_file) = paths.schema_file.as_ref() {
        arguments.push("--output-schema".to_string());
        arguments.push(shell_quote(&schema_file.display().to_string()));
    }
    arguments.push("-".to_string());

    let unset_config_homes = auth_core::forbidden_config_home_env_keys()
        .iter()
        .map(|key| format!("unset {key}"))
        .collect::<Vec<_>>()
        .join("\n");
    let script = format!(
        "set -uo pipefail\n{unset_config_homes}\nrm -f {status} {result} {transcript}\nset +e\n{codex} {arguments} < {task} 2>&1 | tee {transcript}\nexit_code=${{PIPESTATUS[0]}}\nstatus_tmp={status}.tmp.$$\nprintf '%s\\n' \"$exit_code\" > \"$status_tmp\"\nmv -f \"$status_tmp\" {status}\nexit \"$exit_code\"",
        codex = shell_quote(&codex.display().to_string()),
        arguments = arguments.join(" "),
        task = shell_quote(&paths.task_file.display().to_string()),
        result = shell_quote(&paths.result_file.display().to_string()),
        transcript = shell_quote(&paths.transcript_file.display().to_string()),
        status = shell_quote(&paths.status_file.display().to_string()),
    );
    format!("bash -lc {}\r", shell_quote(&script))
}

async fn spawn_direct_codex(
    state: &AppState,
    paths: &CodexTaskPaths,
    mode: CodexTaskMode,
    cwd: &Path,
) -> ApiResult<Child> {
    let user = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("终端用户无效: {error}")))?;
    let current_user = runtime_paths::resolve_current_user_profile();
    let codex = auth::resolve_codex_executable(&auth::codex_command_path_for_user(&user));
    let task_input = std::fs::File::open(&paths.task_file)
        .map_err(|error| AppError::internal(format!("打开 Codex 任务输入失败: {error}")))?;
    let transcript = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&paths.transcript_file)
        .map_err(|error| AppError::internal(format!("打开 Codex 任务输出失败: {error}")))?;
    set_user_owned_path_mode(&paths.transcript_file, &user, 0o600)?;
    let transcript_error = transcript
        .try_clone()
        .map_err(|error| AppError::internal(format!("复制 Codex 任务输出句柄失败: {error}")))?;
    let mut command = if current_user
        .as_ref()
        .is_some_and(|current| current.uid == user.uid)
    {
        Command::new(&codex)
    } else {
        #[cfg(unix)]
        {
            let mut command = Command::new("runuser");
            command
                .arg("-u")
                .arg(&user.name)
                .arg("--preserve-environment")
                .arg("--")
                .arg(&codex);
            command
        }
        #[cfg(not(unix))]
        {
            return Err(AppError::bad_request("当前平台不支持以其他终端用户执行 Codex 任务。"));
        }
    };
    append_codex_exec_args(&mut command, paths, mode, cwd);
    command
        .env_clear()
        .env("HOME", &user.home)
        .env("USER", &user.name)
        .env("LOGNAME", &user.name)
        .env("SHELL", &user.shell)
        .env("PATH", auth::codex_command_path_for_user(&user))
        .env("TERM", "dumb")
        .current_dir(cwd)
        .stdin(task_input)
        .stdout(transcript)
        .stderr(transcript_error)
        .kill_on_drop(true);
    for (key, value) in resolve_native_environment(state, &user).await {
        if !PROTECTED_ENV_KEYS
            .iter()
            .any(|protected| key.eq_ignore_ascii_case(protected))
            && !auth_core::is_forbidden_config_home_env_key(&key)
        {
            command.env(key, value);
        }
    }
    command.env(auth_core::WEBCLX_LOCAL_API_TOKEN_ENV, state.local_api_token.as_ref());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    command
        .spawn()
        .map_err(|error| AppError::internal(format!("启动原生 codex exec 失败: {error}")))
}

async fn resolve_native_environment(
    state: &AppState,
    user: &runtime_paths::UserProfile,
) -> Vec<(String, String)> {
    let shell_user = user.clone();
    let shell_env = tokio::task::spawn_blocking(move || {
        crate::shell_env::read_user_shell_env(&shell_user).map(|snapshot| {
            crate::shell_env::merge_inherited_env_entries(&snapshot.entries, &NETWORK_ENV_KEYS)
        })
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default();
    let mut effective = shell_env;
    merge_env(&mut effective, state.workspace_settings.terminal_default_env_entries());
    merge_env(&mut effective, state.proxy_manager.get_terminal_proxy_env());
    effective
}

async fn create_task_terminal(
    state: &AppState,
    preset: &StoredApiPreset,
    cwd: PathBuf,
) -> ApiResult<(String, String)> {
    let base_dir = state.workspace_root();
    let display_root = state.workspace_display_root();
    let user = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("终端用户无效: {error}")))?;
    let use_local_proxy = api_preset_enables_local_upstream_proxy_on_apply(preset);
    let session = state
        .terminal_manager
        .create_session(
            &base_dir,
            &display_root,
            cwd,
            user,
            state.workspace_settings.terminal_default_env_entries(),
            state.proxy_manager.get_terminal_proxy_env(),
            None,
            preset.name.clone(),
            api_provider_base_url_for_mode(preset, use_local_proxy),
        )
        .map_err(|error| AppError::internal(format!("创建 Codex 任务终端失败: {error}")))?;
    let session_id = session.id().to_string();
    let short_id = session_id.trim_start_matches('s');
    let renamed = state
        .terminal_manager
        .rename_session(&base_dir, &display_root, &session_id, format!("codex_task_{short_id}"))
        .map_err(|error| AppError::internal(format!("命名 Codex 任务终端失败: {error}")))?;
    Ok((session_id, renamed.name().to_string()))
}

async fn close_owned_terminal(state: &AppState, terminal_id: Option<&str>) -> bool {
    let Some(terminal_id) = terminal_id else {
        return true;
    };
    if !state.terminal_manager.has_session(terminal_id) {
        return true;
    }
    state
        .terminal_manager
        .delete_session(&state.workspace_root(), &state.workspace_display_root(), terminal_id)
        .is_ok()
}

async fn finish_cancelled(
    state: &AppState,
    manager: &CodexTaskManager,
    task_id: &str,
    terminal_id: Option<&str>,
) {
    let terminal_closed = close_owned_terminal(state, terminal_id).await;
    manager
        .update(task_id, |record| {
            record.status = CodexTaskStatus::Cancelled;
            record.finished_at = Some(current_timestamp_millis());
            record.terminal_closed = terminal_closed;
            record.error = Some("Codex 任务已取消。".to_string());
        })
        .await;
}

async fn finish_failed(
    manager: &CodexTaskManager,
    task_id: &str,
    error: String,
    terminal_closed: bool,
) {
    manager
        .update(task_id, |record| {
            record.status = CodexTaskStatus::Failed;
            record.finished_at = Some(current_timestamp_millis());
            record.terminal_closed = terminal_closed;
            record.error = Some(error);
        })
        .await;
}

async fn terminate_direct_child(child: &mut Option<Child>) {
    let Some(child) = child.as_mut() else {
        return;
    };
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
        sleep(Duration::from_millis(200)).await;
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill().await;
    let _ = child.wait().await;
}

fn validate_request(payload: &CreateCodexTaskRequest) -> ApiResult<()> {
    if payload.task.trim().is_empty() {
        return Err(AppError::bad_request("task 不能为空。"));
    }
    if payload.task.len() > MAX_TASK_BYTES {
        return Err(AppError::bad_request(format!("task 过大，最多允许 {MAX_TASK_BYTES} 字节。")));
    }
    let timeout_secs = payload.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);
    if timeout_secs == 0 || timeout_secs > MAX_TIMEOUT_SECS {
        return Err(AppError::bad_request(format!(
            "timeout_secs 必须在 1..={MAX_TIMEOUT_SECS} 之间。"
        )));
    }
    if let Some(schema) = payload.output_schema.as_ref() {
        if !schema.is_object() {
            return Err(AppError::bad_request("output_schema 必须是 JSON 对象。"));
        }
        let encoded = serde_json::to_vec(schema)
            .map_err(|error| AppError::bad_request(format!("output_schema 无效: {error}")))?;
        if encoded.len() > MAX_SCHEMA_BYTES {
            return Err(AppError::bad_request(format!(
                "output_schema 过大，最多允许 {MAX_SCHEMA_BYTES} 字节。"
            )));
        }
    }
    Ok(())
}

fn resolve_preset(
    presets: &[StoredApiPreset],
    selector: &CodexTaskPresetSelector,
) -> ApiResult<StoredApiPreset> {
    let supplied = usize::from(selector.id.as_deref().is_some_and(nonempty))
        + usize::from(selector.name.as_deref().is_some_and(nonempty))
        + usize::from(selector.model.as_deref().is_some_and(nonempty));
    if supplied != 1 {
        return Err(AppError::bad_request(
            "preset 必须且只能提供 id、name、model 其中一个选择器。",
        ));
    }
    let lookup = if let Some(id) = selector.id.as_deref() {
        ApiPresetLookup::Id(id)
    } else if let Some(name) = selector.name.as_deref() {
        ApiPresetLookup::Name(name)
    } else {
        ApiPresetLookup::Model(selector.model.as_deref().unwrap_or_default())
    };
    let index = select_api_preset_index(presets, lookup)
        .map_err(|error| AppError::bad_request(error.to_string()))?;
    Ok(presets[index].clone())
}

fn nonempty(value: &str) -> bool {
    !value.trim().is_empty()
}

fn preset_summary(preset: &StoredApiPreset) -> CodexTaskPresetSummary {
    CodexTaskPresetSummary {
        id: preset.id.clone(),
        name: preset.name.clone(),
        model: api_preset_model(preset).map(str::to_string),
    }
}

fn parse_launch_banner(transcript: &str) -> (Option<String>, Option<String>) {
    let mut model = None;
    let mut session_id = None;
    for line in transcript.lines().take(80) {
        let line = line.trim();
        if model.is_none()
            && let Some(value) = line.strip_prefix("model:").map(str::trim)
            && !value.is_empty()
        {
            model = Some(value.to_string());
        }
        if session_id.is_none()
            && let Some(value) = line.strip_prefix("session id:").map(str::trim)
            && !value.is_empty()
        {
            session_id = Some(value.to_string());
        }
    }
    (model, session_id)
}

fn parse_structured_output(output: &str) -> Option<Value> {
    let trimmed = output.trim();
    if let Ok(value) = serde_json::from_str(trimmed) {
        return Some(value);
    }
    if let Some(fenced) = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        && let Some((body, _)) = fenced.rsplit_once("```")
        && let Ok(value) = serde_json::from_str(body.trim())
    {
        return Some(value);
    }
    let start = trimmed.find(['{', '['])?;
    let end = trimmed.rfind(['}', ']'])?;
    (start <= end)
        .then(|| &trimmed[start..=end])
        .and_then(|candidate| serde_json::from_str(candidate).ok())
}

fn transcript_error_suffix(transcript: &str) -> String {
    let tail = truncate_tail(transcript, 4_096);
    if tail.trim().is_empty() {
        String::new()
    } else {
        format!(" 终端尾部: {}", tail.trim())
    }
}

async fn read_limited(path: &Path, max_bytes: usize) -> String {
    match fs::read(path).await {
        Ok(bytes) => truncate_text(&String::from_utf8_lossy(&bytes), max_bytes),
        Err(_) => String::new(),
    }
}

async fn read_tail(path: &Path, max_bytes: usize) -> String {
    match fs::read(path).await {
        Ok(bytes) => truncate_tail(&String::from_utf8_lossy(&bytes), max_bytes),
        Err(_) => String::new(),
    }
}

fn truncate_text(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n... [truncated]", &value[..end])
}

fn truncate_tail(value: &str, max_bytes: usize) -> String {
    if value.len() <= max_bytes {
        return value.to_string();
    }
    let mut start = value.len().saturating_sub(max_bytes);
    while !value.is_char_boundary(start) {
        start += 1;
    }
    format!("... [truncated]\n{}", &value[start..])
}

fn merge_env(base: &mut Vec<(String, String)>, overlay: Vec<(String, String)>) {
    for (key, value) in overlay {
        if let Some(existing) = base.iter_mut().find(|(existing, _)| existing == &key) {
            existing.1 = value;
        } else {
            base.push((key, value));
        }
    }
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn generate_task_id() -> String {
    let timestamp = current_timestamp_millis();
    let counter = TASK_COUNTER.fetch_add(1, Ordering::Relaxed);
    let random: u32 = rand::thread_rng().r#gen();
    format!("ct_{timestamp:x}_{counter:x}_{random:08x}")
}

fn load_task_records(records_dir: &Path) -> HashMap<String, CodexTaskRecord> {
    let mut records = HashMap::new();
    let Ok(entries) = std::fs::read_dir(records_dir) else {
        return records;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        if let Ok(record) = serde_json::from_str::<CodexTaskRecord>(&content) {
            records.insert(record.id.clone(), record);
        }
    }
    records
}

async fn write_user_file(
    path: &Path,
    contents: &[u8],
    user: &runtime_paths::UserProfile,
    mode: u32,
) -> ApiResult<()> {
    fs::write(path, contents)
        .await
        .map_err(|error| AppError::internal(format!("写入 Codex 任务文件失败: {error}")))?;
    set_user_owned_path_mode(path, user, mode)
}

#[cfg(unix)]
fn set_user_owned_path_mode(
    path: &Path,
    user: &runtime_paths::UserProfile,
    mode: u32,
) -> ApiResult<()> {
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt, fs::PermissionsExt};

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|error| AppError::internal(format!("设置 Codex 任务权限失败: {error}")))?;
    let metadata = std::fs::metadata(path)
        .map_err(|error| AppError::internal(format!("读取 Codex 任务权限失败: {error}")))?;
    if metadata.uid() == user.uid && metadata.gid() == user.gid {
        return Ok(());
    }
    if unsafe { libc::geteuid() } != 0 {
        return Err(AppError::internal(format!(
            "当前进程无权把 Codex 任务文件交给用户 `{}`。",
            user.name
        )));
    }
    let encoded = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| AppError::internal("Codex 任务路径包含 NUL。"))?;
    if unsafe { libc::chown(encoded.as_ptr(), user.uid, user.gid) } != 0 {
        return Err(AppError::internal(format!(
            "修改 Codex 任务文件所有者失败: {}",
            std::io::Error::last_os_error()
        )));
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_user_owned_path_mode(
    _path: &Path,
    _user: &runtime_paths::UserProfile,
    _mode: u32,
) -> ApiResult<()> {
    Ok(())
}

#[cfg(unix)]
fn set_owner_only_mode(path: &Path, mode: u32) -> ApiResult<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .map_err(|error| AppError::internal(format!("设置 Codex 任务记录权限失败: {error}")))
}

#[cfg(not(unix))]
fn set_owner_only_mode(_path: &Path, _mode: u32) -> ApiResult<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preset(id: &str, name: &str, model: &str) -> StoredApiPreset {
        let mut preset = StoredApiPreset {
            id: id.to_string(),
            name: name.to_string(),
            saved_at: 0,
            provider_name: name.to_string(),
            base_url: "https://example.test/v1".to_string(),
            management_url: None,
            wire_api: None,
            responses_proxy: None,
            apply_upstream_proxy_on_switch: false,
            config_overrides: Vec::new(),
            legacy_config_key: None,
            legacy_config_value: None,
            legacy_secondary_config_key: None,
            legacy_secondary_config_value: None,
            terminal_env: Vec::new(),
            terminal_startup_script: None,
            api_key: "test-key".to_string(),
            access_token: String::new(),
            account_id: String::new(),
            access_mode: None,
            switch_count: 0,
        };
        preset
            .config_overrides
            .push(auth_core::PresetConfigOverride {
                key: Some("model".to_string()),
                value: Some(model.to_string()),
            });
        preset
    }

    #[test]
    fn preset_selector_accepts_exact_name_or_model() {
        let presets = vec![
            preset("api-1", "Primary", "gpt-5.6-sol"),
            preset("api-2", "Grok", "grok-4.5"),
        ];
        let by_name = resolve_preset(
            &presets,
            &CodexTaskPresetSelector {
                id: None,
                name: Some("Grok".to_string()),
                model: None,
            },
        )
        .unwrap();
        let by_model = resolve_preset(
            &presets,
            &CodexTaskPresetSelector {
                id: None,
                name: None,
                model: Some("GROK-4.5".to_string()),
            },
        )
        .unwrap();
        assert_eq!(by_name.id, "api-2");
        assert_eq!(by_model.id, "api-2");
    }

    #[test]
    fn terminal_command_uses_shared_home_without_model_or_sandbox_overrides() {
        let run_dir = PathBuf::from("/tmp/webclx task");
        let paths = CodexTaskPaths {
            task_file: run_dir.join("task.txt"),
            schema_file: None,
            result_file: run_dir.join("last-message.txt"),
            transcript_file: run_dir.join("transcript.log"),
            status_file: run_dir.join("exit-status"),
            run_dir,
        };
        let command = render_terminal_codex_command(
            &paths,
            CodexTaskMode::Terminal,
            Path::new("/home/codes/project"),
            Path::new("/usr/local/bin/codex"),
        );
        assert!(command.starts_with("bash -lc "));
        assert!(command.contains("/usr/local/bin/codex"));
        assert!(command.contains(" exec --skip-git-repo-check"));
        assert!(command.contains("task.txt"));
        assert!(!command.contains("run.sh"));
        assert!(!command.contains("--model"));
        assert!(!command.contains("--sandbox"));
        assert!(!command.contains("--ephemeral"));
    }

    #[test]
    fn launch_banner_reports_model_and_session() {
        let (model, session_id) = parse_launch_banner(
            "OpenAI Codex\nmodel: GLM-5.2\nsession id: 019f9214-a004-79b0-904b-47b7392766dc\n",
        );
        assert_eq!(model.as_deref(), Some("GLM-5.2"));
        assert_eq!(session_id.as_deref(), Some("019f9214-a004-79b0-904b-47b7392766dc"));
    }

    #[cfg(unix)]
    #[test]
    fn direct_child_exit_zero_is_successful_exit() {
        use std::os::unix::process::ExitStatusExt;

        let outcome = direct_child_exit_outcome(std::process::ExitStatus::from_raw(0));
        assert!(matches!(outcome, MonitorOutcome::Exited(0)));
    }

    #[cfg(unix)]
    #[test]
    fn direct_child_nonzero_exit_preserves_code() {
        use std::os::unix::process::ExitStatusExt;

        let outcome = direct_child_exit_outcome(std::process::ExitStatus::from_raw(7 << 8));
        assert!(matches!(outcome, MonitorOutcome::Exited(7)));
    }

    #[cfg(unix)]
    #[test]
    fn direct_child_signal_exit_is_runner_failure() {
        use std::os::unix::process::ExitStatusExt;

        let outcome = direct_child_exit_outcome(std::process::ExitStatus::from_raw(libc::SIGTERM));
        let MonitorOutcome::RunnerFailed(error) = outcome else {
            panic!("signal termination must fail the runner");
        };
        assert!(error.contains("signal 15"), "unexpected error: {error}");
    }
}
