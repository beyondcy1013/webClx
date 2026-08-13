use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use terminal_core::tmux_session_name;
use time::{OffsetDateTime, format_description::FormatItem, macros::format_description};
use tracing::{info, warn};

use crate::{ApiResult, AppError, AppState};

const BUILD_SOURCE_REPO_DIR: &str = env!("CARGO_MANIFEST_DIR");
const SOURCE_REPO_DIR_ENV: &str = "WEBCLX_SOURCE_REPO_DIR";
const DEFAULT_SOURCE_REPO_DIR: &str = "/home/codes/webClx";
const COMPILE_WORKER_SCRIPT: &str = "docs/codex/skills/webclx-rebuild/scripts/compile-worker.sh";
const COMPILE_WORK_DIR_ENV: &str = "WEBCLX_COMPILE_WORK_DIR";
const DEFAULT_COMPILE_WORK_DIR: &str = "/data/cargo-target/webclx-compile";
const DEFAULT_DEBOUNCE_SECS: u64 = 0;
const MAX_DEBOUNCE_SECS: u64 = 900;
const COMPILE_RUN_RETENTION_SECS: u64 = 5 * 24 * 60 * 60; // 5 days
const COMPILE_INSTALL_STALLED_SECS: u64 = 120;
const REQUEST_ID_TIME_FORMAT: &[FormatItem<'static>] =
    format_description!("[hour][minute][second]");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BuildRequestKind {
    #[allow(dead_code)]
    Compile,
    Deploy,
}

impl BuildRequestKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Compile => "compile",
            Self::Deploy => "deploy",
        }
    }

    fn queued_action(self) -> &'static str {
        match self {
            Self::Compile => "编译",
            Self::Deploy => "部署",
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompileRequest {
    #[serde(default)]
    source_terminal_id: String,
    #[serde(default)]
    source_terminal_name: String,
    #[serde(default)]
    source_tmux_session: String,
    #[serde(default)]
    project_path: String,
    #[serde(default)]
    project: String,
    #[serde(default)]
    project_name: String,
    #[serde(default)]
    project_dir: String,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default)]
    install_command: Vec<String>,
    #[serde(default)]
    audit_paths: Vec<String>,
    #[serde(default)]
    required_artifacts: Vec<String>,
    #[serde(default)]
    note: String,
    #[serde(default)]
    debounce_secs: Option<u64>,
    #[serde(default)]
    skip_sync: bool,
    #[serde(default)]
    skip_restart: bool,
    #[serde(default)]
    allow_worktree: bool,
}

#[derive(Debug, Serialize)]
pub struct CompileResponse {
    ok: bool,
    request_id: String,
    request_kind: String,
    project: String,
    project_dir: String,
    source_terminal_id: String,
    source_terminal_name: String,
    source_tmux_session: String,
    queued: bool,
    debounce_secs: u64,
    command: Vec<String>,
    install_command: Vec<String>,
    audit_paths: Vec<String>,
    required_artifacts: Vec<String>,
    queue_dir: String,
    worker: String,
}

#[derive(Debug, Serialize)]
pub struct CompileStatusResponse {
    ok: bool,
    project: String,
    queue_dir: String,
    pending_count: usize,
    run_count: usize,
    latest_log: Option<String>,
    pending_requests: Vec<CompileRequestSummary>,
    runs: Vec<CompileRunSummary>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompileStatusQuery {
    #[serde(default)]
    include_history: Option<bool>,
}

impl CompileStatusQuery {
    fn should_include_history(&self) -> bool {
        self.include_history.unwrap_or(true)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompileNotificationRequest {
    target: String,
    message: String,
    #[serde(default)]
    tone: String,
}

#[derive(Debug, Serialize)]
pub struct CompileNotificationResponse {
    ok: bool,
    session_id: String,
    terminal_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompileCompletionRequest {
    request_id: String,
}

#[derive(Debug, Serialize)]
pub struct CompileCompletionResponse {
    ok: bool,
    request_id: String,
    cleared: bool,
}

#[derive(Debug, Serialize)]
pub struct CompileRequestSummary {
    request_id: String,
    request_kind: String,
    project: String,
    project_dir: String,
    source_terminal_id: String,
    source_terminal_name: String,
    source_tmux_session: String,
    project_path: String,
    note: String,
    command: Vec<String>,
    install_command: Vec<String>,
    audit_paths: Vec<String>,
    required_artifacts: Vec<String>,
    requested_at: u64,
    debounce_secs: u64,
    file_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CompileRunSummary {
    run_id: String,
    status: String,
    request_count: usize,
    projects: Vec<String>,
    source_terminal_ids: Vec<String>,
    source_terminal_names: Vec<String>,
    source_tmux_sessions: Vec<String>,
    started_at: Option<String>,
    finished_at: Option<String>,
    current_project: Option<String>,
    current_phase: Option<String>,
    current_command: Vec<String>,
    current_spec_index: Option<usize>,
    spec_count: usize,
    packages_completed: Option<usize>,
    packages_total: Option<usize>,
    current_package: Option<String>,
    progress_updated_at: Option<String>,
    log_path: Option<String>,
    dir_path: String,
}

#[derive(Debug, Default, Deserialize)]
struct CompileRunProgress {
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    phase: Option<String>,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default)]
    spec_index: Option<usize>,
    #[serde(default)]
    spec_count: usize,
    #[serde(default)]
    packages_completed: Option<usize>,
    #[serde(default)]
    packages_total: Option<usize>,
    #[serde(default)]
    current_package: Option<String>,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    log_path: Option<String>,
}

pub async fn request_compile(
    State(state): State<AppState>,
    Json(mut payload): Json<CompileRequest>,
) -> ApiResult<Json<CompileResponse>> {
    payload.install_command.clear();
    payload.audit_paths.clear();
    queue_build_request(state, payload, BuildRequestKind::Compile)
}

pub async fn request_deploy(
    State(state): State<AppState>,
    mut payload: Json<CompileRequest>,
) -> ApiResult<Json<CompileResponse>> {
    let repo_dir = source_repo_dir()?;
    let project_dir = compile_project_dir(&payload, &repo_dir)?;
    if payload.install_command.is_empty() {
        let resolved = detect_install_command(&repo_dir, &project_dir)?;
        payload.install_command = resolved;
    }
    validate_shell_command_argv(&payload.install_command, "install_command")?;
    validate_install_command_script(&payload.install_command, &project_dir)?;
    queue_build_request(state, payload.0, BuildRequestKind::Deploy)
}

pub async fn complete_compile_request(
    State(state): State<AppState>,
    Json(payload): Json<CompileCompletionRequest>,
) -> ApiResult<Json<CompileCompletionResponse>> {
    let request_id = payload.request_id.trim().to_string();
    if request_id.is_empty() {
        return Err(AppError::bad_request("缺少编译请求 request_id"));
    }
    let cleared = state
        .terminal_manager
        .complete_pending_build_request(&request_id);
    info!(
        request_id = %request_id,
        cleared,
        "completed build request lifecycle"
    );
    Ok(Json(CompileCompletionResponse {
        ok: true,
        request_id,
        cleared,
    }))
}

fn queue_build_request(
    state: AppState,
    payload: CompileRequest,
    request_kind: BuildRequestKind,
) -> ApiResult<Json<CompileResponse>> {
    let source_terminal_name = first_nonempty([payload.source_terminal_name.as_str()])
        .ok_or_else(|| AppError::bad_request("缺少来源终端 source_terminal_name"))?
        .to_string();
    let source_terminal_target = first_nonempty([payload.source_terminal_id.as_str()])
        .and_then(|id| state.terminal_manager.resolve_session_target(id, None).ok())
        .or_else(|| {
            state
                .terminal_manager
                .resolve_session_target(&source_terminal_name, None)
                .ok()
        });
    let source_terminal_id = source_terminal_target
        .as_ref()
        .map(|(id, _)| id.clone())
        .or_else(|| first_nonempty([payload.source_terminal_id.as_str()]).map(ToString::to_string))
        .unwrap_or_default();
    let current_source_terminal_name = source_terminal_target
        .as_ref()
        .map(|(_, name)| name.clone())
        .unwrap_or_else(|| source_terminal_name.clone());
    let source_tmux_session = first_nonempty([payload.source_tmux_session.as_str()])
        .map(ToString::to_string)
        .or_else(|| {
            (!source_terminal_id.is_empty()).then(|| tmux_session_name(&source_terminal_id))
        })
        .unwrap_or_default();

    let repo_dir = source_repo_dir()?;
    let project_dir = compile_project_dir(&payload, &repo_dir)?;
    validate_project_checkout(&project_dir, payload.allow_worktree)?;
    let project = compile_project_name(&payload, &project_dir);
    validate_required_artifacts(&payload.required_artifacts)?;
    let command = compile_command(&payload, &repo_dir, &project_dir, request_kind)?;
    validate_shell_command_argv(&command, "command")?;
    let script_path = repo_dir.join(COMPILE_WORKER_SCRIPT);
    if !script_path.is_file() {
        return Err(AppError::internal(format!(
            "编译 worker 脚本不存在: {}",
            script_path.display()
        )));
    }

    let queue_dir = compile_queue_dir()?;
    let request_dir = queue_dir.join("requests");
    fs::create_dir_all(&request_dir)
        .map_err(|error| AppError::internal(format!("创建编译队列失败: {error}")))?;

    let request_id = build_request_id();
    let debounce_secs = payload
        .debounce_secs
        .unwrap_or(DEFAULT_DEBOUNCE_SECS)
        .min(MAX_DEBOUNCE_SECS);
    let base_url = format!("http://127.0.0.1:{}", state.listen_addr.port());
    let request_path = request_dir.join(format!("{request_id}.json"));
    let requested_at = current_timestamp_secs();
    let body = json!({
        "request_id": request_id,
        "request_kind": request_kind.as_str(),
        "project": project.clone(),
        "project_dir": project_dir.display().to_string(),
        "source_terminal_id": source_terminal_id.clone(),
        "source_terminal_name": current_source_terminal_name.clone(),
        "source_tmux_session": source_tmux_session.clone(),
        "project_path": compile_project_path(&payload, &project),
        "note": payload.note.clone(),
        "command": command.clone(),
        "compile_environment": state.workspace_settings.compile_environment(),
        "install_command": payload.install_command.clone(),
        "audit_paths": payload.audit_paths.clone(),
        "required_artifacts": payload.required_artifacts.clone(),
        "requested_at": requested_at,
        "debounce_secs": debounce_secs,
        "skip_sync": payload.skip_sync,
        "skip_restart": payload.skip_restart,
        "allow_worktree": payload.allow_worktree,
    });
    let bytes = serde_json::to_vec_pretty(&body)
        .map_err(|error| AppError::internal(format!("序列化编译请求失败: {error}")))?;
    fs::write(&request_path, bytes)
        .map_err(|error| AppError::internal(format!("写入编译请求失败: {error}")))?;

    // The setting value is already clamped to 60..=3600 by the settings normalizer
    // (normalize_compile_command_timeout_secs); pass it straight through.
    let command_timeout_secs = state.workspace_settings.compile_command_timeout_secs();
    let max_concurrency = state.workspace_settings.compile_max_concurrency();
    let work_dir = compile_work_dir();
    state
        .terminal_manager
        .register_pending_build_request(&request_id, &source_terminal_id);
    let worker = match start_compile_worker_or_cleanup(&request_path, || {
        spawn_compile_worker(
            &repo_dir,
            &script_path,
            &queue_dir,
            &work_dir,
            &base_url,
            command_timeout_secs,
            max_concurrency,
            &request_id,
        )
    }) {
        Ok(worker) => worker,
        Err(error) => {
            state
                .terminal_manager
                .complete_pending_build_request(&request_id);
            return Err(error);
        }
    };
    notify_compile_request_queued(
        &state,
        if source_terminal_id.is_empty() {
            source_terminal_name.as_str()
        } else {
            source_terminal_id.as_str()
        },
        &current_source_terminal_name,
        &project,
        &request_id,
        request_kind,
        debounce_secs,
    );
    info!(
        request_id,
        project,
        source_terminal_id,
        source_terminal_name = current_source_terminal_name,
        source_tmux_session,
        debounce_secs,
        queue_dir = %queue_dir.display(),
        worker,
        request_kind = request_kind.as_str(),
        "queued build request"
    );

    Ok(Json(CompileResponse {
        ok: true,
        request_id,
        request_kind: request_kind.as_str().to_string(),
        project,
        project_dir: project_dir.display().to_string(),
        source_terminal_id,
        source_terminal_name: current_source_terminal_name,
        source_tmux_session,
        queued: true,
        debounce_secs,
        command,
        install_command: payload.install_command,
        audit_paths: payload.audit_paths,
        required_artifacts: payload.required_artifacts,
        queue_dir: queue_dir.display().to_string(),
        worker,
    }))
}

fn notify_compile_request_queued(
    state: &AppState,
    source_terminal_target: &str,
    source_terminal_name: &str,
    project: &str,
    request_id: &str,
    request_kind: BuildRequestKind,
    debounce_secs: u64,
) {
    let wait_hint = if debounce_secs == 0 {
        format!("正在启动{}", request_kind.queued_action())
    } else {
        format!("预计等待 {debounce_secs} 秒合并请求")
    };
    let message = format!(
        "已收到 {project} {}请求，{wait_hint}。请求 {request_id}",
        request_kind.queued_action()
    );
    if let Err(error) =
        state
            .terminal_manager
            .send_session_toast(source_terminal_target, None, message, "info")
    {
        warn!(
            source_terminal_target,
            source_terminal_name,
            project,
            request_id,
            error = %error,
            "failed to send compile queued toast"
        );
    }
}

pub async fn compile_status(
    State(_state): State<AppState>,
    Query(query): Query<CompileStatusQuery>,
) -> ApiResult<Json<CompileStatusResponse>> {
    let include_history = query.should_include_history();
    tokio::task::spawn_blocking(move || compile_status_snapshot(include_history))
        .await
        .map_err(|error| AppError::internal(format!("读取编译状态任务失败: {error}")))?
        .map(Json)
}

fn compile_status_snapshot(include_history: bool) -> ApiResult<CompileStatusResponse> {
    let queue_dir = compile_queue_dir()?;
    prune_expired_compile_runs(&queue_dir, compile_run_cutoff_secs());
    let pending_requests = list_pending_compile_requests(&queue_dir.join("requests"));
    let run_dirs = list_compile_run_dirs(&queue_dir);
    let run_count = run_dirs.len();
    let latest_run = run_dirs
        .first()
        .and_then(|run_dir| compile_run_summary_from_dir(run_dir));
    let runs = if include_history {
        compile_run_summaries(&run_dirs)
    } else {
        run_dirs
            .iter()
            .filter(|run_dir| !run_dir.join("run-finished-at").is_file())
            .filter_map(|run_dir| compile_run_summary_from_dir(run_dir))
            .filter(|run| matches!(run.status.as_str(), "running" | "stalled"))
            .collect()
    };
    let latest_log = latest_run.and_then(|run| run.log_path);

    Ok(CompileStatusResponse {
        ok: true,
        project: "webClx".to_string(),
        queue_dir: queue_dir.display().to_string(),
        pending_count: pending_requests.len(),
        run_count,
        latest_log,
        pending_requests,
        runs,
    })
}

pub async fn notify_compile_terminal(
    State(state): State<AppState>,
    Json(payload): Json<CompileNotificationRequest>,
) -> ApiResult<Json<CompileNotificationResponse>> {
    let target = payload.target.trim();
    if target.is_empty() {
        return Err(AppError::bad_request("缺少编译通知目标终端 target"));
    }
    let message = payload.message.trim();
    if message.is_empty() {
        return Err(AppError::bad_request("缺少编译通知内容 message"));
    }
    let sent = state
        .terminal_manager
        .send_session_toast(
            target,
            None,
            message.to_string(),
            compile_notification_tone(&payload.tone),
        )
        .map_err(|error| AppError::bad_request(format!("发送编译通知失败: {error}")))?;

    Ok(Json(CompileNotificationResponse {
        ok: true,
        session_id: sent.0,
        terminal_name: sent.1,
    }))
}

fn compile_notification_tone(tone: &str) -> &str {
    match tone.trim() {
        "ok" | "warn" | "muted" => tone.trim(),
        _ => "info",
    }
}

fn source_repo_dir() -> ApiResult<PathBuf> {
    let candidates = [
        env::var_os(SOURCE_REPO_DIR_ENV).map(PathBuf::from),
        Some(PathBuf::from(DEFAULT_SOURCE_REPO_DIR)),
        Some(PathBuf::from(BUILD_SOURCE_REPO_DIR)),
    ];
    for repo in candidates.into_iter().flatten() {
        if repo.join(COMPILE_WORKER_SCRIPT).is_file() && repo.join("Cargo.toml").is_file() {
            return repo.canonicalize().map_err(|error| {
                AppError::internal(format!("无法定位源码目录 {}: {error}", repo.display()))
            });
        }
    }
    Err(AppError::internal(format!(
        "无法定位源码目录：请设置 {SOURCE_REPO_DIR_ENV}，或确认 {DEFAULT_SOURCE_REPO_DIR} 存在"
    )))
}

const COMPILE_QUEUE_DIR_ENV: &str = "WEBCLX_COMPILE_QUEUE_DIR";

/// Resolve the compile queue directory.
///
/// Defaults to `<cwd>/compile` so logs live next to the webClx runtime data
/// (the process cwd is the app dir, e.g. `/home/bin/webclx`). Override with
/// `WEBCLX_COMPILE_QUEUE_DIR` for testing or custom layouts.
fn compile_queue_dir() -> ApiResult<PathBuf> {
    if let Some(dir) = env::var_os(COMPILE_QUEUE_DIR_ENV) {
        let dir = PathBuf::from(dir);
        return dir.canonicalize().map_err(|error| {
            AppError::internal(format!("无法定位编译队列目录 {}: {error}", dir.display()))
        });
    }
    env::current_dir()
        .map_err(|error| AppError::internal(format!("无法获取当前目录: {error}")))?
        .join("compile")
        .canonicalize()
        .or_else(|_| env::current_dir().map(|d| d.join("compile")))
        .map_err(|error| AppError::internal(format!("无法定位编译队列目录: {error}")))
}

/// Resolve the heavy build-cache work directory (cargo target trees + temp).
///
/// Defaults to `/data/cargo-target/webclx-compile` so the multi-hundred-GB
/// per-project `CARGO_TARGET_DIR` trees land on the large `/data` partition
/// instead of the source-repo filesystem. Override with `WEBCLX_COMPILE_WORK_DIR`.
fn compile_work_dir() -> PathBuf {
    if let Some(dir) = env::var_os(COMPILE_WORK_DIR_ENV) {
        return PathBuf::from(dir);
    }
    PathBuf::from(DEFAULT_COMPILE_WORK_DIR)
}

fn compile_project_dir(payload: &CompileRequest, fallback_repo_dir: &Path) -> ApiResult<PathBuf> {
    let raw_dir = first_nonempty([payload.project_dir.as_str()]);
    let dir = raw_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| fallback_repo_dir.to_path_buf());
    dir.canonicalize().map_err(|error| {
        AppError::bad_request(format!("无法定位项目工作目录 {}: {error}", dir.display()))
    })
}

fn validate_project_checkout(project_dir: &Path, allow_worktree: bool) -> ApiResult<()> {
    let git_root = Command::new("git")
        .args([
            "-C",
            project_dir.to_string_lossy().as_ref(),
            "rev-parse",
            "--show-toplevel",
        ])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| PathBuf::from(value.trim()));
    let Some(git_root) = git_root else {
        return Ok(());
    };
    let worktrees = Command::new("git")
        .args([
            "-C",
            project_dir.to_string_lossy().as_ref(),
            "worktree",
            "list",
            "--porcelain",
        ])
        .output()
        .map_err(|error| AppError::internal(format!("无法检查 Git worktree: {error}")))?;
    if !worktrees.status.success() {
        return Err(AppError::internal("无法读取 Git worktree 列表"));
    }
    let primary_root = String::from_utf8_lossy(&worktrees.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("worktree "))
        .map(PathBuf::from);
    let Some(primary_root) = primary_root else {
        return Ok(());
    };
    let git_root = git_root.canonicalize().unwrap_or(git_root);
    let primary_root = primary_root.canonicalize().unwrap_or(primary_root);
    if git_root != primary_root && !allow_worktree {
        return Err(AppError::bad_request(format!(
            "拒绝从 linked worktree {} 编译或部署；请使用主 checkout {}。只有用户在当前对话明确授权时才能设置 allow_worktree=true",
            git_root.display(),
            primary_root.display()
        )));
    }
    Ok(())
}

fn compile_project_name(payload: &CompileRequest, project_dir: &Path) -> String {
    first_nonempty([payload.project.as_str(), payload.project_name.as_str()])
        .map(ToString::to_string)
        .unwrap_or_else(|| {
            project_dir
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("project")
                .to_string()
        })
}

fn compile_project_path(payload: &CompileRequest, fallback_project: &str) -> String {
    first_nonempty([payload.project_path.as_str()])
        .map(ToString::to_string)
        .unwrap_or_else(|| fallback_project.to_string())
}

fn compile_command(
    payload: &CompileRequest,
    repo_dir: &Path,
    project_dir: &Path,
    request_kind: BuildRequestKind,
) -> ApiResult<Vec<String>> {
    if !payload.command.is_empty() {
        return Ok(payload.command.clone());
    }

    if request_kind == BuildRequestKind::Compile {
        if project_dir.join("Cargo.toml").is_file() {
            return Ok(vec![
                "cargo".to_string(),
                "build".to_string(),
                "--release".to_string(),
            ]);
        }

        if project_dir.join("package.json").is_file() {
            return Ok(vec!["npm".to_string(), "run".to_string(), "build".to_string()]);
        }

        if project_dir.join("Makefile").is_file() || project_dir.join("makefile").is_file() {
            return Ok(vec!["make".to_string()]);
        }

        return Err(AppError::bad_request(format!(
            "无法为 {} 自动推断纯编译命令，请在请求中传 command 数组",
            project_dir.display()
        )));
    }

    if request_kind == BuildRequestKind::Deploy {
        // A project with scripts/rebuild-and-deploy.sh owns the full build +
        // deploy pipeline (often a Windows/Android cross-compile). Running a
        // host-native `cargo build --release` as the compile stage would either
        // waste time on an unused native binary or fail on missing system
        // libraries (atk, gdk-pixbuf, ...). Skip the compile stage with a
        // no-op so the install stage's rebuild-and-deploy.sh does all the work.
        if project_dir.join("scripts/rebuild-and-deploy.sh").is_file() {
            let noop = COMPILE_WORKER_SCRIPT.replace("compile-worker.sh", "noop-compile.sh");
            // The worker runs the command after `cd "$project_dir"`, but
            // noop-compile.sh lives under the webClx repo_dir, not the
            // project dir. Resolve an absolute path so the file is found
            // regardless of which project is being deployed (otherwise bash
            // reports "No such file or directory" with status 127).
            let noop_abs = repo_dir.join(&noop);
            return Ok(vec!["bash".to_string(), noop_abs.display().to_string()]);
        }

        if project_dir.join("Cargo.toml").is_file() {
            return Ok(vec![
                "cargo".to_string(),
                "build".to_string(),
                "--release".to_string(),
            ]);
        }

        if project_dir.join("package.json").is_file() {
            return Ok(vec!["npm".to_string(), "run".to_string(), "build".to_string()]);
        }

        if project_dir.join("Makefile").is_file() || project_dir.join("makefile").is_file() {
            return Ok(vec!["make".to_string()]);
        }
    }

    Err(AppError::bad_request(format!(
        "无法为 {} 自动推断编译命令，请在请求中传 command 数组",
        project_dir.display()
    )))
}

fn is_shell_program(value: &str) -> bool {
    Path::new(value)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, "bash" | "sh" | "dash" | "zsh" | "ksh"))
}

fn validate_shell_command_argv(command: &[String], field: &str) -> ApiResult<()> {
    if !command.first().is_some_and(|value| is_shell_program(value)) {
        return Ok(());
    }

    let Some(option) = command.get(1) else {
        return Ok(());
    };
    let is_command_option = option == "-c"
        || (option.starts_with('-') && !option.starts_with("--") && option[1..].contains('c'));
    if !is_command_option {
        return Ok(());
    }

    let Some(shell_command) = command.get(2).filter(|value| !value.trim().is_empty()) else {
        return Err(AppError::bad_request(format!(
            "{field} 中 shell {option} 后缺少单一命令字符串"
        )));
    };
    let split_shell_program = shell_words::split(shell_command)
        .is_ok_and(|words| words.len() == 1 && is_shell_program(&words[0]) && command.len() > 3);
    if split_shell_program {
        return Err(AppError::bad_request(format!(
            "{field} 疑似错误拆分 shell {option} 参数；请把完整命令放在一个字符串中，例如 bash -lc \"bash scripts/build.sh\""
        )));
    }

    Ok(())
}

fn validate_required_artifacts(required_artifacts: &[String]) -> ApiResult<()> {
    if required_artifacts.iter().any(|path| path.trim().is_empty()) {
        return Err(AppError::bad_request("required_artifacts 不能包含空路径"));
    }
    Ok(())
}

fn install_command_script_candidates(command: &[String]) -> BTreeSet<String> {
    fn is_script_path(value: &str) -> bool {
        !value.is_empty()
            && !value.contains('$')
            && (value == "noop-deploy.sh"
                || value.ends_with(".sh")
                || value.ends_with(".bash")
                || value.starts_with("scripts/")
                || value.starts_with("./scripts/")
                || value.contains("/scripts/"))
    }

    let mut candidates = BTreeSet::new();
    for (index, arg) in command.iter().enumerate() {
        let value = arg.trim();
        let is_shell_code = index > 1
            && is_shell_program(&command[index - 2])
            && command[index - 1].starts_with('-')
            && command[index - 1].contains('c');
        if !is_shell_code && is_script_path(value) {
            candidates.insert(value.to_string());
        }
        if is_shell_code && let Ok(words) = shell_words::split(value) {
            for word in words {
                let word = word.trim_matches([';', '&', '|', '(', ')']);
                if is_script_path(word) {
                    candidates.insert(word.to_string());
                }
            }
        }
    }
    candidates
}

fn validate_install_command_script(command: &[String], project_dir: &Path) -> ApiResult<()> {
    let candidates = install_command_script_candidates(command);
    if candidates.is_empty() {
        return Err(AppError::bad_request(
            "部署请求必须显式指定部署脚本 install_command，例如 bash scripts/deploy.sh；纯编译请使用 /api/build/compile",
        ));
    }

    let resolved = candidates
        .iter()
        .map(|candidate| {
            let path = Path::new(candidate);
            if path.is_absolute() {
                path.to_path_buf()
            } else {
                project_dir.join(path)
            }
        })
        .collect::<Vec<_>>();
    let missing = resolved
        .iter()
        .filter(|path| !path.is_file())
        .collect::<Vec<_>>();
    if missing.is_empty() {
        return Ok(());
    }

    Err(AppError::bad_request(format!(
        "部署脚本不存在: {}",
        missing
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )))
}

/// Detect the best deploy script for a project directory.
///
/// Priority order (consistent with `compile_command` and AGENTS.md):
/// 1. `scripts/rebuild-and-deploy.sh` — full rebuild + deploy (self-hosted services)
/// 2. `scripts/deploy.sh` — explicit deploy script
/// 3. `scripts/install-service.sh` — service install script
/// 4. `deploy.sh` — root-level deploy script
/// 5. For the webClx repo itself, use the full `rebuild-and-deploy.sh` in docs/
fn detect_install_command(repo_dir: &Path, project_dir: &Path) -> ApiResult<Vec<String>> {
    let candidates = [
        "scripts/rebuild-and-deploy.sh",
        "scripts/deploy.sh",
        "scripts/install-service.sh",
        "deploy.sh",
    ];
    for script in &candidates {
        if project_dir.join(script).is_file() {
            return Ok(vec!["bash".to_string(), script.to_string()]);
        }
    }

    // webClx itself uses the full rebuild-and-deploy pipeline.
    if project_dir == repo_dir {
        let script = COMPILE_WORKER_SCRIPT.replace("compile-worker.sh", "rebuild-and-deploy.sh");
        return Ok(vec!["bash".to_string(), script]);
    }

    Err(AppError::bad_request(format!(
        "未找到 {} 的部署脚本。请在项目中添加 scripts/deploy.sh 或 scripts/rebuild-and-deploy.sh，或在请求中传 install_command 数组。",
        project_dir.display()
    )))
}

fn spawn_compile_worker(
    repo_dir: &Path,
    script_path: &Path,
    queue_dir: &Path,
    work_dir: &Path,
    base_url: &str,
    command_timeout_secs: u64,
    max_concurrency: u32,
    request_id: &str,
) -> ApiResult<String> {
    let unit_name = format!("webclx-compile-{request_id}");
    let mut command = Command::new("/usr/bin/systemd-run");
    command
        .arg("--quiet")
        .arg("--collect")
        .arg("--unit")
        .arg(&unit_name)
        .arg("--property")
        .arg(format!("WorkingDirectory={}", repo_dir.display()))
        .arg("/usr/bin/env")
        .arg("bash")
        .arg(script_path)
        .arg("--queue-dir")
        .arg(queue_dir)
        .arg("--base-url")
        .arg(base_url)
        .arg("--repo-dir")
        .arg(repo_dir)
        .arg("--work-dir")
        .arg(work_dir)
        .arg("--command-timeout")
        .arg(command_timeout_secs.to_string())
        .arg("--max-concurrency")
        .arg(max_concurrency.to_string());

    match command.output() {
        Ok(output) if output.status.success() => Ok(format!("systemd-run:{unit_name}")),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            warn!(unit_name, stderr, "failed to start compile worker through systemd-run");
            Err(AppError::internal(format!(
                "启动编译 worker 失败: {}",
                if stderr.is_empty() {
                    output.status.to_string()
                } else {
                    stderr
                }
            )))
        }
        Err(error) => Err(AppError::internal(format!(
            "无法执行 /usr/bin/systemd-run 启动编译 worker: {error}"
        ))),
    }
}

fn start_compile_worker_or_cleanup(
    request_path: &Path,
    start_worker: impl FnOnce() -> ApiResult<String>,
) -> ApiResult<String> {
    match start_worker() {
        Ok(worker) => Ok(worker),
        Err(error) => {
            if let Err(cleanup_error) = fs::remove_file(request_path) {
                warn!(
                    request_path = %request_path.display(),
                    %cleanup_error,
                    "failed to remove request after compile worker launch failure"
                );
            }
            Err(error)
        }
    }
}

fn first_nonempty<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    values
        .into_iter()
        .map(str::trim)
        .find(|value| !value.is_empty())
}

fn list_pending_compile_requests(dir: &Path) -> Vec<CompileRequestSummary> {
    let mut requests = list_json_files(dir)
        .into_iter()
        .filter_map(|path| compile_request_summary_from_file(&path))
        .collect::<Vec<_>>();
    requests.sort_by_key(|b| std::cmp::Reverse(b.requested_at));
    requests
}

fn list_compile_run_dirs(queue_dir: &Path) -> Vec<PathBuf> {
    let runs_dir = queue_dir.join("runs");
    let Ok(entries) = fs::read_dir(&runs_dir) else {
        return Vec::new();
    };
    let cutoff = compile_run_cutoff_secs();
    let mut run_dirs = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| run_dir_is_within_retention(path, cutoff))
        .collect::<Vec<_>>();
    run_dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    run_dirs
}

fn compile_run_summaries(run_dirs: &[PathBuf]) -> Vec<CompileRunSummary> {
    run_dirs
        .iter()
        .filter_map(|run_dir| compile_run_summary_from_dir(run_dir))
        .collect()
}

/// Return the Unix-seconds cutoff for the compile run retention window.
fn compile_run_cutoff_secs() -> u64 {
    current_timestamp_secs().saturating_sub(COMPILE_RUN_RETENTION_SECS)
}

fn prune_expired_compile_runs(queue_dir: &Path, cutoff: u64) {
    let runs_dir = queue_dir.join("runs");
    let Ok(entries) = fs::read_dir(&runs_dir) else {
        return;
    };
    for run_dir in entries.filter_map(Result::ok).map(|entry| entry.path()) {
        if !run_dir.is_dir()
            || run_dir_is_within_retention(&run_dir, cutoff)
            || !run_dir.join("run-finished-at").is_file()
        {
            continue;
        }
        if let Err(error) = fs::remove_dir_all(&run_dir) {
            warn!(run_dir = %run_dir.display(), %error, "failed to prune expired compile run");
            continue;
        }
    }
}

/// Decide whether a run directory is still within the retention window.
///
/// The run_id encodes a timestamp in the form `YYYYmmddTHHMMSS-xxxxx`. We parse
/// the date+time prefix into Unix seconds and compare against the cutoff. If
/// parsing fails we keep the run (never accidentally hide data we cannot
/// timestamp).
fn run_dir_is_within_retention(run_dir: &Path, cutoff: u64) -> bool {
    let Some(run_id) = run_dir.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    // Expected prefix: "20260711T081538"
    if run_id.len() < 15 || run_id.as_bytes()[8] != b'T' {
        return true; // unparseable, keep it
    }
    let bytes = run_id.as_bytes();
    let nums: Option<Vec<u32>> = (0..15)
        .filter(|&i| i != 8) // skip 'T' at position 8
        .map(|i| (bytes[i] as char).to_digit(10))
        .collect();
    let Some(digits) = nums else {
        return true;
    };
    // digits = [Y,Y,Y,Y, m,m, d,d, H,H, M,M, S,S] -> 14 digits
    let year =
        digits[0] as i32 * 1000 + digits[1] as i32 * 100 + digits[2] as i32 * 10 + digits[3] as i32;
    let month = digits[4] * 10 + digits[5];
    let day = digits[6] * 10 + digits[7];
    let hour = digits[8] * 10 + digits[9];
    let minute = digits[10] * 10 + digits[11];
    let second = digits[12] * 10 + digits[13];

    let Some(month_enum) = time::Month::try_from(month as u8).ok() else {
        return true;
    };
    let Ok(date) = time::Date::from_calendar_date(year, month_enum, day as u8) else {
        return true;
    };
    let Ok(time_val) = time::Time::from_hms(hour as u8, minute as u8, second as u8) else {
        return true;
    };
    let dt = time::PrimitiveDateTime::new(date, time_val);
    // Assume local timezone (UTC+8 for this server).
    let offset = time::UtcOffset::from_hms(8, 0, 0).unwrap_or(time::UtcOffset::UTC);
    dt.assume_offset(offset).unix_timestamp().max(0) as u64 >= cutoff
}

fn compile_request_summary_from_file(path: &Path) -> Option<CompileRequestSummary> {
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    Some(CompileRequestSummary {
        request_id: json_string(&value, "request_id").unwrap_or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("unknown")
                .to_string()
        }),
        request_kind: json_string(&value, "request_kind").unwrap_or_else(|| "compile".to_string()),
        project: json_string(&value, "project").unwrap_or_else(|| "webClx".to_string()),
        project_dir: json_string(&value, "project_dir").unwrap_or_default(),
        source_terminal_id: json_string(&value, "source_terminal_id").unwrap_or_default(),
        source_terminal_name: json_string(&value, "source_terminal_name").unwrap_or_default(),
        source_tmux_session: json_string(&value, "source_tmux_session").unwrap_or_default(),
        project_path: json_string(&value, "project_path").unwrap_or_default(),
        note: json_string(&value, "note").unwrap_or_default(),
        command: value
            .get("command")
            .and_then(|command| command.as_array())
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default(),
        install_command: json_string_array(&value, "install_command"),
        audit_paths: json_string_array(&value, "audit_paths"),
        required_artifacts: json_string_array(&value, "required_artifacts"),
        requested_at: value
            .get("requested_at")
            .and_then(|timestamp| timestamp.as_u64())
            .unwrap_or_default(),
        debounce_secs: value
            .get("debounce_secs")
            .and_then(|timestamp| timestamp.as_u64())
            .unwrap_or_default(),
        file_path: path.display().to_string(),
    })
}

fn json_string_array(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|items| items.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn compile_run_summary_from_dir(run_dir: &Path) -> Option<CompileRunSummary> {
    let run_id = run_dir.file_name()?.to_str()?.to_string();
    let requests = list_json_files(run_dir)
        .into_iter()
        .filter(|path| is_compile_request_file(path))
        .filter_map(|path| compile_request_summary_from_file(&path))
        .collect::<Vec<_>>();
    let mut projects = BTreeSet::new();
    let mut source_terminal_ids = BTreeSet::new();
    let mut source_terminal_names = BTreeSet::new();
    let mut source_tmux_sessions = BTreeSet::new();
    for request in &requests {
        if !request.project.is_empty() {
            projects.insert(request.project.clone());
        }
        if !request.source_terminal_id.is_empty() {
            source_terminal_ids.insert(request.source_terminal_id.clone());
        }
        if !request.source_terminal_name.is_empty() {
            source_terminal_names.insert(request.source_terminal_name.clone());
        }
        if !request.source_tmux_session.is_empty() {
            source_tmux_sessions.insert(request.source_tmux_session.clone());
        }
    }
    let log_path = run_dir.join("run.log");
    let run_log_paths = list_run_log_paths(run_dir);
    let status_codes = list_status_codes(run_dir);
    let spec_count = count_nonempty_lines(&run_dir.join("specs.jsonl")).unwrap_or(requests.len());
    let run_finished_at = read_trimmed(&run_dir.join("run-finished-at"));
    let all_specs_finished =
        run_finished_at.is_some() || (spec_count > 0 && status_codes.len() >= spec_count);
    let progress =
        read_json_file::<CompileRunProgress>(&run_dir.join("progress.json")).unwrap_or_default();
    let status = if !all_specs_finished {
        if progress.phase.as_deref() == Some("install")
            && file_age_secs(&run_dir.join("progress.json"))
                .is_some_and(|age| age >= COMPILE_INSTALL_STALLED_SECS)
        {
            "stalled"
        } else if compile_run_recently_modified(run_dir, &log_path, &run_log_paths) {
            "running"
        } else {
            "unknown"
        }
    } else if run_dir_has_file_prefix(run_dir, "timedout-") {
        "timed_out"
    } else if status_codes.iter().any(|status| *status != 0) {
        "failed"
    } else {
        "success"
    }
    .to_string();
    let progress = if matches!(status.as_str(), "running" | "stalled") {
        progress
    } else {
        CompileRunProgress::default()
    };
    let current_log_path = progress.log_path.clone();
    Some(CompileRunSummary {
        run_id,
        status,
        request_count: requests.len(),
        projects: projects.into_iter().collect(),
        source_terminal_ids: source_terminal_ids.into_iter().collect(),
        source_terminal_names: source_terminal_names.into_iter().collect(),
        source_tmux_sessions: source_tmux_sessions.into_iter().collect(),
        started_at: read_trimmed(&run_dir.join("run-started-at"))
            .or_else(|| extreme_file_trimmed(run_dir, "started-", true)),
        finished_at: all_specs_finished
            .then(|| run_finished_at.or_else(|| extreme_file_trimmed(run_dir, "finished-", false)))
            .flatten(),
        current_project: progress.project,
        current_phase: progress.phase,
        current_command: progress.command,
        current_spec_index: progress.spec_index,
        spec_count: progress.spec_count.max(spec_count),
        packages_completed: progress.packages_completed,
        packages_total: progress.packages_total,
        current_package: progress.current_package,
        progress_updated_at: progress.updated_at,
        log_path: current_log_path.or_else(|| {
            run_log_paths
                .first()
                .or_else(|| log_path.is_file().then_some(&log_path))
                .map(|path| path.display().to_string())
        }),
        dir_path: run_dir.display().to_string(),
    })
}

fn file_age_secs(path: &Path) -> Option<u64> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .elapsed()
        .ok()
        .map(|elapsed| elapsed.as_secs())
}

fn run_dir_has_file_prefix(run_dir: &Path, prefix: &str) -> bool {
    fs::read_dir(run_dir).ok().is_some_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry.file_name().to_string_lossy().starts_with(prefix) && entry.path().is_file()
        })
    })
}

fn compile_run_recently_modified(
    run_dir: &Path,
    log_path: &Path,
    run_log_paths: &[PathBuf],
) -> bool {
    const RUNNING_STALE_AFTER_SECS: u64 = 1800;
    let latest_modified = std::iter::once(run_dir)
        .chain(std::iter::once(log_path))
        .chain(run_log_paths.iter().map(PathBuf::as_path))
        .filter_map(|path| path.metadata().ok())
        .filter_map(|metadata| metadata.modified().ok())
        .map(Some)
        .map(system_time_secs)
        .max()
        .unwrap_or_default();
    latest_modified > 0
        && current_timestamp_secs().saturating_sub(latest_modified) <= RUNNING_STALE_AFTER_SECS
}

fn list_run_log_paths(run_dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(run_dir) else {
        return Vec::new();
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            if !name.starts_with("log-") || !name.ends_with(".path") {
                return None;
            }
            let value = fs::read_to_string(path).ok()?.trim().to_string();
            (!value.is_empty()).then(|| PathBuf::from(value))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
}

fn list_json_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect()
}

fn is_compile_request_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(".json") && name != "progress.json" && !name.starts_with("install-")
        })
}

fn list_status_codes(dir: &Path) -> Vec<i32> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            if !name.starts_with("status-") {
                return None;
            }
            fs::read_to_string(path).ok()?.trim().parse::<i32>().ok()
        })
        .collect()
}

fn extreme_file_trimmed(dir: &Path, prefix: &str, earliest: bool) -> Option<String> {
    let values = fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            if !name.starts_with(prefix) {
                return None;
            }
            let value = fs::read_to_string(path).ok()?.trim().to_string();
            (!value.is_empty()).then_some(value)
        });
    if earliest { values.min() } else { values.max() }
}

fn read_trimmed(path: &Path) -> Option<String> {
    let value = fs::read_to_string(path).ok()?.trim().to_string();
    (!value.is_empty()).then_some(value)
}

fn read_json_file<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn count_nonempty_lines(path: &Path) -> Option<usize> {
    let value = fs::read_to_string(path).ok()?;
    Some(value.lines().filter(|line| !line.trim().is_empty()).count())
}

fn json_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|value| value.as_str())
        .map(ToString::to_string)
}

fn system_time_secs(value: Option<SystemTime>) -> u64 {
    value
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn build_request_id() -> String {
    static REQUEST_SEQUENCE: OnceLock<AtomicU64> = OnceLock::new();

    let readable_time = OffsetDateTime::now_local()
        .unwrap_or_else(|_| OffsetDateTime::now_utc())
        .format(REQUEST_ID_TIME_FORMAT)
        .unwrap_or_else(|_| current_timestamp_secs().to_string());
    let sequence = REQUEST_SEQUENCE
        .get_or_init(|| {
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos() as u64)
                .unwrap_or_default();
            AtomicU64::new(seed)
        })
        .fetch_add(1, Ordering::Relaxed);

    format!("{readable_time}-{sequence:016x}")
}

fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile_run_fixture(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "webclx-compile-run-{name}-{}-{}",
            std::process::id(),
            build_request_id()
        ));
        let run_dir = root.join("runs/20260715T092500-test");
        fs::create_dir_all(&run_dir).expect("create run fixture");
        for (request_id, project) in [("request-1", "runAny"), ("request-2", "stockScreener")] {
            fs::write(
                run_dir.join(format!("{request_id}.json")),
                serde_json::json!({
                    "request_id": request_id,
                    "project": project,
                    "project_dir": format!("/home/codes/{project}"),
                })
                .to_string(),
            )
            .expect("write request fixture");
        }
        fs::write(
            run_dir.join("specs.jsonl"),
            "{\"project\":\"runAny\"}\n{\"project\":\"stockScreener\"}\n",
        )
        .expect("write specs fixture");
        root
    }

    #[test]
    fn partially_finished_compile_run_stays_running_with_live_progress() {
        let root = compile_run_fixture("partial");
        let run_dir = root.join("runs/20260715T092500-test");
        fs::write(run_dir.join("started-first"), "2026-07-15 09:25:00\n")
            .expect("write first start");
        fs::write(run_dir.join("finished-first"), "2026-07-15 09:27:34\n")
            .expect("write first finish");
        fs::write(run_dir.join("status-first"), "0\n").expect("write first status");
        fs::write(
            run_dir.join("progress.json"),
            serde_json::json!({
                "project": "stockScreener",
                "phase": "compile",
                "spec_index": 2,
                "spec_count": 2,
                "command": ["cargo", "test"],
                "packages_completed": 37,
                "packages_total": 120,
                "current_package": "tokio",
                "updated_at": "2026-07-15 09:30:00"
            })
            .to_string(),
        )
        .expect("write progress fixture");

        let summary = compile_run_summary_from_dir(&run_dir).expect("run summary");

        assert_eq!(summary.status, "running");
        assert_eq!(summary.request_count, 2);
        assert_eq!(summary.started_at.as_deref(), Some("2026-07-15 09:25:00"));
        assert_eq!(summary.finished_at, None);
        assert_eq!(summary.current_project.as_deref(), Some("stockScreener"));
        assert_eq!(summary.current_phase.as_deref(), Some("compile"));
        assert_eq!(summary.current_spec_index, Some(2));
        assert_eq!(summary.spec_count, 2);
        assert_eq!(summary.packages_completed, Some(37));
        assert_eq!(summary.packages_total, Some(120));
        assert_eq!(summary.current_package.as_deref(), Some("tokio"));

        fs::remove_dir_all(root).expect("remove run fixture");
    }

    #[test]
    fn inactive_install_progress_is_reported_as_stalled() {
        let root = compile_run_fixture("stalled-install");
        let run_dir = root.join("runs/20260715T092500-test");
        let progress_path = run_dir.join("progress.json");
        fs::write(
            &progress_path,
            serde_json::json!({
                "project": "stockScreener",
                "phase": "install",
                "spec_index": 1,
                "spec_count": 2,
                "command": ["bash", "scripts/deploy.sh"],
                "updated_at": "2026-07-15 09:30:00"
            })
            .to_string(),
        )
        .expect("write stalled progress fixture");
        let stale_time =
            SystemTime::now() - std::time::Duration::from_secs(COMPILE_INSTALL_STALLED_SECS + 1);
        fs::File::options()
            .write(true)
            .open(&progress_path)
            .expect("open progress fixture")
            .set_times(fs::FileTimes::new().set_modified(stale_time))
            .expect("age progress fixture");

        let summary = compile_run_summary_from_dir(&run_dir).expect("run summary");

        assert_eq!(summary.status, "stalled");
        assert_eq!(summary.current_phase.as_deref(), Some("install"));
        assert_eq!(summary.current_command, ["bash", "scripts/deploy.sh"]);
        fs::remove_dir_all(root).expect("remove run fixture");
    }

    #[test]
    fn timeout_marker_is_reported_as_timed_out() {
        let root = compile_run_fixture("timed-out");
        let run_dir = root.join("runs/20260715T092500-test");
        fs::write(run_dir.join("status-first"), "124\n").expect("write timeout status");
        fs::write(run_dir.join("status-second"), "0\n").expect("write second status");
        fs::write(run_dir.join("timedout-first"), "30 分\n").expect("write timeout marker");
        fs::write(run_dir.join("run-finished-at"), "2026-07-15 10:00:00\n")
            .expect("write run finish");

        let summary = compile_run_summary_from_dir(&run_dir).expect("run summary");

        assert_eq!(summary.status, "timed_out");
        fs::remove_dir_all(root).expect("remove run fixture");
    }

    #[test]
    fn finished_compile_run_uses_earliest_start_and_latest_finish() {
        let root = compile_run_fixture("finished");
        let run_dir = root.join("runs/20260715T092500-test");
        fs::write(run_dir.join("started-second"), "2026-07-15 09:29:40\n")
            .expect("write second start");
        fs::write(run_dir.join("started-first"), "2026-07-15 09:25:00\n")
            .expect("write first start");
        fs::write(run_dir.join("finished-first"), "2026-07-15 09:27:34\n")
            .expect("write first finish");
        fs::write(run_dir.join("finished-second"), "2026-07-15 09:38:03\n")
            .expect("write second finish");
        fs::write(run_dir.join("status-first"), "0\n").expect("write first status");
        fs::write(run_dir.join("status-second"), "1\n").expect("write second status");

        let summary = compile_run_summary_from_dir(&run_dir).expect("run summary");

        assert_eq!(summary.status, "failed");
        assert_eq!(summary.started_at.as_deref(), Some("2026-07-15 09:25:00"));
        assert_eq!(summary.finished_at.as_deref(), Some("2026-07-15 09:38:03"));

        fs::remove_dir_all(root).expect("remove run fixture");
    }

    #[test]
    fn compile_status_query_can_skip_history() {
        let query = CompileStatusQuery {
            include_history: Some(false),
        };

        assert!(!query.should_include_history());
        assert!(CompileStatusQuery::default().should_include_history());
    }

    #[test]
    fn prune_expired_compile_runs_removes_only_finished_expired_records() {
        let root = std::env::temp_dir().join(format!(
            "webclx-compile-prune-{}-{}",
            std::process::id(),
            build_request_id()
        ));
        let runs_dir = root.join("runs");
        fs::create_dir_all(&runs_dir).expect("create runs directory");

        let expired_finished = runs_dir.join("20260720T000000-expired");
        let expired_running = runs_dir.join("20260720T000001-running");
        let retained_finished = runs_dir.join("20260728T000000-retained");
        let malformed_finished = runs_dir.join("legacy-record");
        for run_dir in [
            &expired_finished,
            &expired_running,
            &retained_finished,
            &malformed_finished,
        ] {
            fs::create_dir_all(run_dir).expect("create run directory");
        }
        for run_dir in [&expired_finished, &retained_finished, &malformed_finished] {
            fs::write(run_dir.join("run-finished-at"), "finished\n")
                .expect("write completion marker");
        }
        let cutoff_date = time::Date::from_calendar_date(2026, time::Month::July, 23)
            .expect("create cutoff date");
        let cutoff = time::PrimitiveDateTime::new(cutoff_date, time::Time::MIDNIGHT)
            .assume_offset(time::UtcOffset::from_hms(8, 0, 0).expect("create UTC+8 offset"))
            .unix_timestamp() as u64;
        prune_expired_compile_runs(&root, cutoff);

        assert!(!expired_finished.exists());
        assert!(expired_running.exists());
        assert!(retained_finished.exists());
        assert!(malformed_finished.exists());

        fs::remove_dir_all(root).expect("remove prune fixture");
    }

    #[test]
    fn compile_request_path_is_not_a_terminal_notification_filter() {
        let payload = CompileRequest {
            source_terminal_name: "dingBot_4_pg数据库".to_string(),
            project_path: "stockScreener".to_string(),
            project: "stockScreener".to_string(),
            project_dir: "/home/codes/stockScreener".to_string(),
            ..CompileRequest::default()
        };

        assert_eq!(compile_project_path(&payload, "fallback"), "stockScreener");
    }

    #[test]
    fn source_terminal_name_is_required_as_primary_compile_field() {
        let payload: CompileRequest =
            serde_json::from_str(r#"{"source_terminal_name":"webClx_10"}"#)
                .expect("source_terminal_name should deserialize");
        assert_eq!(payload.source_terminal_name, "webClx_10");
    }

    #[test]
    fn compile_notification_has_independent_target_and_tone() {
        let payload: CompileNotificationRequest =
            serde_json::from_str(r#"{"target":"s2346","message":"编译完成","tone":"ok"}"#)
                .expect("compile notification should deserialize");

        assert_eq!(payload.target, "s2346");
        assert_eq!(payload.message, "编译完成");
        assert_eq!(compile_notification_tone(&payload.tone), "ok");
        assert_eq!(compile_notification_tone("unexpected"), "info");
    }

    #[test]
    fn legacy_target_is_rejected_in_compile_request() {
        let error = serde_json::from_str::<CompileRequest>(r#"{"target":"webClx_10"}"#)
            .expect_err("target is not a compile request field");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn legacy_path_is_rejected_in_compile_request() {
        let error = serde_json::from_str::<CompileRequest>(r#"{"path":"stockScreener"}"#)
            .expect_err("path is not a compile request field");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn deploy_request_can_carry_install_command_and_audit_paths() {
        let payload: CompileRequest = serde_json::from_str(
            r#"{"source_terminal_name":"webClx_10","install_command":["bash","scripts/install.sh"],"audit_paths":["/home/bin/webclx/webClx"],"required_artifacts":["target/release/webclx"]}"#,
        )
        .expect("deploy payload should deserialize");
        assert_eq!(payload.install_command, ["bash", "scripts/install.sh"]);
        assert_eq!(payload.audit_paths, ["/home/bin/webclx/webClx"]);
        assert_eq!(payload.required_artifacts, ["target/release/webclx"]);
    }

    #[test]
    fn shell_command_argv_rejects_missing_and_split_command_strings() {
        for command in [
            vec!["bash", "-lc"],
            vec!["bash", "-lc", ""],
            vec!["bash", "-lc", "bash", "scripts/build-windows.sh"],
            vec!["/bin/sh", "-c", "sh", "/tmp/deploy.sh"],
        ] {
            let command = command
                .into_iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            assert!(
                validate_shell_command_argv(&command, "command").is_err(),
                "suspicious shell argv must be rejected: {command:?}"
            );
        }

        for command in [
            vec!["bash", "scripts/build-windows.sh"],
            vec!["bash", "-lc", "bash scripts/build-windows.sh"],
            vec!["bash", "-lc", "printf '%s\\n' \"$0\"", "label"],
            vec!["cargo", "build", "--release"],
        ] {
            let command = command
                .into_iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>();
            assert!(
                validate_shell_command_argv(&command, "command").is_ok(),
                "valid argv must be accepted: {command:?}"
            );
        }
    }

    #[test]
    fn required_artifacts_reject_empty_paths() {
        assert!(validate_required_artifacts(&["target/release/app".to_string()]).is_ok());
        assert!(validate_required_artifacts(&["".to_string()]).is_err());
        assert!(validate_required_artifacts(&["   ".to_string()]).is_err());
    }

    #[test]
    fn deploy_install_command_must_invoke_script() {
        let tmp = std::env::temp_dir().join(format!(
            "webclx-install-command-{}-{}",
            std::process::id(),
            build_request_id()
        ));
        fs::create_dir_all(tmp.join("scripts")).unwrap();
        fs::write(tmp.join("scripts/deploy.sh"), "#!/bin/bash\n").unwrap();
        fs::write(tmp.join("scripts/deploy with spaces.sh"), "#!/bin/bash\n").unwrap();
        let relative = ["bash".to_string(), "scripts/deploy.sh".to_string()];
        let absolute = [
            "bash".to_string(),
            tmp.join("scripts/deploy.sh").display().to_string(),
        ];
        let missing = ["bash".to_string(), "scripts/missing.sh".to_string()];
        let inline = [
            "bash".to_string(),
            "-lc".to_string(),
            "bash scripts/deploy.sh --skip-build".to_string(),
        ];
        let direct_with_spaces = [
            "bash".to_string(),
            "scripts/deploy with spaces.sh".to_string(),
        ];
        let quoted_with_spaces = [
            "bash".to_string(),
            "-lc".to_string(),
            "bash 'scripts/deploy with spaces.sh'".to_string(),
        ];
        let partly_missing = [
            "bash".to_string(),
            "-lc".to_string(),
            "bash scripts/deploy.sh && bash scripts/missing.sh".to_string(),
        ];
        let shell_fragment = [
            "bash".to_string(),
            "-lc".to_string(),
            "install -m 0755 target/release/my-service /home/bin/my-service && systemctl restart my-service.service".to_string(),
        ];

        assert!(validate_install_command_script(&relative, &tmp).is_ok());
        assert!(validate_install_command_script(&absolute, &tmp).is_ok());
        assert!(validate_install_command_script(&inline, &tmp).is_ok());
        assert!(validate_install_command_script(&direct_with_spaces, &tmp).is_ok());
        assert!(validate_install_command_script(&quoted_with_spaces, &tmp).is_ok());
        assert!(validate_install_command_script(&partly_missing, &tmp).is_err());
        let error = validate_install_command_script(&missing, &tmp)
            .expect_err("missing deploy scripts must be rejected before queueing");
        assert!(error.to_string().contains("不存在"));
        assert!(validate_install_command_script(&shell_fragment, &tmp).is_err());

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn request_id_keeps_readable_time_prefix_and_unique_suffix() {
        let request_id = build_request_id();
        let (time_prefix, unique_suffix) = request_id
            .split_once('-')
            .expect("request id should separate readable time and uniqueness suffix");

        assert_eq!(time_prefix.len(), "103847".len());
        assert!(time_prefix.chars().all(|ch| ch.is_ascii_digit()));
        assert_eq!(unique_suffix.len(), 16);
        assert!(unique_suffix.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn rapid_request_ids_are_unique() {
        let ids = (0..1_024)
            .map(|_| build_request_id())
            .collect::<BTreeSet<_>>();

        assert_eq!(ids.len(), 1_024);
    }

    #[test]
    fn worker_launch_failure_removes_request_and_propagates_error() {
        let dir = std::env::temp_dir().join(format!(
            "webclx-worker-launch-failure-{}-{}",
            std::process::id(),
            build_request_id()
        ));
        fs::create_dir_all(&dir).expect("create temp queue");
        let request_path = dir.join("request.json");
        fs::write(&request_path, "{}").expect("write pending request");

        let result = start_compile_worker_or_cleanup(&request_path, || {
            Err(AppError::internal("systemd-run rejected unit"))
        });

        assert!(result.is_err());
        assert_eq!(
            result.expect_err("worker launch should fail").to_string(),
            "systemd-run rejected unit"
        );
        assert!(!request_path.exists());
        fs::remove_dir_all(&dir).expect("remove temp queue");
    }

    #[test]
    fn request_summary_reads_stable_terminal_identity_fields() {
        let path = std::env::temp_dir()
            .join(format!("webclx-compile-request-{}.json", std::process::id()));
        fs::write(
            &path,
            r#"{
              "request_id":"request-1",
              "source_terminal_id":"s1547",
              "source_terminal_name":"signIn_5",
              "source_tmux_session":"webclx_s1547",
              "project":"signIn",
              "project_dir":"/home/codes/signIn"
            }"#,
        )
        .expect("write temp request");

        let summary = compile_request_summary_from_file(&path).expect("request summary");
        let _ = fs::remove_file(&path);

        assert_eq!(summary.source_terminal_id, "s1547");
        assert_eq!(summary.source_terminal_name, "signIn_5");
        assert_eq!(summary.source_tmux_session, "webclx_s1547");
    }

    #[test]
    fn compile_project_name_falls_back_to_project_dir_name() {
        let payload = CompileRequest::default();
        let project_dir = Path::new("/home/codes/signIn");

        assert_eq!(compile_project_name(&payload, project_dir), "signIn");
    }

    #[test]
    fn linked_worktree_requires_explicit_authorization() {
        let root = std::env::temp_dir().join(format!(
            "webclx-compile-worktree-{}-{}",
            std::process::id(),
            build_request_id()
        ));
        let primary = root.join("primary");
        let linked = root.join("linked");
        fs::create_dir_all(&primary).expect("create primary checkout");
        for args in [
            vec!["init"],
            vec!["config", "user.email", "test@example.com"],
            vec!["config", "user.name", "Test User"],
        ] {
            assert!(
                Command::new("git")
                    .args(args)
                    .current_dir(&primary)
                    .status()
                    .unwrap()
                    .success()
            );
        }
        fs::write(primary.join("README.md"), "fixture\n").expect("write fixture");
        assert!(
            Command::new("git")
                .args(["add", "README.md"])
                .current_dir(&primary)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args(["commit", "-m", "fixture"])
                .current_dir(&primary)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args([
                    "worktree",
                    "add",
                    "--detach",
                    linked.to_string_lossy().as_ref()
                ])
                .current_dir(&primary)
                .status()
                .unwrap()
                .success()
        );

        validate_project_checkout(&primary, false).expect("primary checkout must be accepted");
        let error = validate_project_checkout(&linked, false)
            .expect_err("linked worktree must require authorization");
        assert!(error.to_string().contains("allow_worktree=true"));
        validate_project_checkout(&linked, true).expect("explicit authorization must be accepted");

        assert!(
            Command::new("git")
                .args([
                    "worktree",
                    "remove",
                    "--force",
                    linked.to_string_lossy().as_ref()
                ])
                .current_dir(&primary)
                .status()
                .unwrap()
                .success()
        );
        fs::remove_dir_all(root).expect("remove worktree fixture");
    }

    #[test]
    fn compile_project_path_prefers_explicit_workspace_label() {
        let payload = CompileRequest {
            project_path: "signIn".to_string(),
            ..CompileRequest::default()
        };

        assert_eq!(compile_project_path(&payload, "fallback"), "signIn");
    }

    #[test]
    fn tmux_session_is_derived_from_source_terminal_id() {
        assert_eq!(tmux_session_name("s1547"), "webclx_s1547");
    }

    #[test]
    fn detect_install_command_finds_deploy_sh() {
        let tmp = std::env::temp_dir().join(format!("webclx-detect-deploy-{}", std::process::id()));
        fs::create_dir_all(tmp.join("scripts")).unwrap();
        fs::write(tmp.join("scripts/deploy.sh"), "#!/bin/bash\necho deploy\n").unwrap();

        let resolved = detect_install_command(std::path::Path::new("/nonexistent"), &tmp).unwrap();
        assert_eq!(resolved, vec!["bash".to_string(), "scripts/deploy.sh".to_string()]);

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn detect_install_command_prefers_rebuild_and_deploy() {
        let tmp =
            std::env::temp_dir().join(format!("webclx-detect-rebuild-{}", std::process::id()));
        fs::create_dir_all(tmp.join("scripts")).unwrap();
        fs::write(tmp.join("scripts/rebuild-and-deploy.sh"), "#!/bin/bash\n").unwrap();
        fs::write(tmp.join("scripts/deploy.sh"), "#!/bin/bash\n").unwrap();

        let resolved = detect_install_command(std::path::Path::new("/nonexistent"), &tmp).unwrap();
        assert_eq!(
            resolved,
            vec![
                "bash".to_string(),
                "scripts/rebuild-and-deploy.sh".to_string()
            ]
        );

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn compile_command_deploy_skips_native_build_when_rebuild_and_deploy_present() {
        let tmp = std::env::temp_dir().join(format!("webclx-deploy-noop-{}", std::process::id()));
        fs::create_dir_all(tmp.join("scripts")).unwrap();
        fs::write(tmp.join("Cargo.toml"), "[package]\nname = \"demo\"\nversion = \"0.0.0\"\n")
            .unwrap();
        fs::write(tmp.join("scripts/rebuild-and-deploy.sh"), "#!/bin/bash\n").unwrap();
        let payload = CompileRequest::default();

        let cmd = compile_command(
            &payload,
            std::path::Path::new("/nonexistent"),
            &tmp,
            BuildRequestKind::Deploy,
        )
        .unwrap();
        assert!(cmd.iter().any(|s| s.ends_with("noop-compile.sh")));

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn compile_command_deploy_runs_cargo_when_no_rebuild_and_deploy() {
        let tmp = std::env::temp_dir().join(format!("webclx-deploy-cargo-{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("Cargo.toml"), "[package]\nname = \"demo\"\nversion = \"0.0.0\"\n")
            .unwrap();
        let payload = CompileRequest::default();

        let cmd = compile_command(
            &payload,
            std::path::Path::new("/nonexistent"),
            &tmp,
            BuildRequestKind::Deploy,
        )
        .unwrap();
        assert_eq!(
            cmd,
            vec![
                "cargo".to_string(),
                "build".to_string(),
                "--release".to_string()
            ]
        );

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn compile_command_never_infers_a_deploy_script() {
        let tmp = std::env::temp_dir().join(format!(
            "webclx-compile-no-deploy-{}-{}",
            std::process::id(),
            build_request_id()
        ));
        fs::create_dir_all(tmp.join("scripts")).unwrap();
        fs::write(tmp.join("Cargo.toml"), "[package]\nname = \"demo\"\nversion = \"0.0.0\"\n")
            .unwrap();
        fs::write(tmp.join("scripts/rebuild-and-deploy.sh"), "#!/bin/bash\n").unwrap();

        let cmd =
            compile_command(&CompileRequest::default(), &tmp, &tmp, BuildRequestKind::Compile)
                .unwrap();
        assert_eq!(
            cmd,
            vec![
                "cargo".to_string(),
                "build".to_string(),
                "--release".to_string()
            ]
        );

        fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn detect_install_command_errors_when_no_script() {
        let tmp = std::env::temp_dir().join(format!("webclx-detect-none-{}", std::process::id()));
        fs::create_dir_all(&tmp).unwrap();

        let result = detect_install_command(std::path::Path::new("/nonexistent"), &tmp);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("未找到"));

        fs::remove_dir_all(&tmp).unwrap();
    }
}
