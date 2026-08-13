use std::{
    ffi::OsStr,
    path::{Component, Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};
use tokio::{fs, io::AsyncWriteExt, process::Command, time::timeout};

use crate::{ApiResult, AppError, AppState, filesystem};

const DEFAULT_TIMEOUT_SECS: u64 = 60;
const MIN_TIMEOUT_SECS: u64 = 1;
const MAX_TIMEOUT_SECS: u64 = 600;
const MAX_OUTPUT_BYTES: usize = 64 * 1024;
const MAX_CHECKPOINT_BYTES: usize = 8 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 512 * 1024;
const MAX_PATCH_BYTES: usize = 2 * 1024 * 1024;
const DEFAULT_RESULT_LIMIT: usize = 200;
const MAX_RESULT_LIMIT: usize = 1_000;

pub async fn list_files(
    state: &AppState,
    path: Option<&str>,
    max_results: Option<u64>,
) -> ApiResult<Value> {
    let cwd = resolve_cwd(state, path)?;
    let limit = normalize_result_limit(max_results);
    let ripgrep = ripgrep_executable(state)?;
    let result = run_argv(
        &ripgrep.to_string_lossy(),
        &["--files", "--color", "never"],
        &cwd,
        DEFAULT_TIMEOUT_SECS,
        None,
    )
    .await?;
    let (files, truncated) = limited_lines(&result.stdout, limit);
    Ok(json!({
        "cwd": cwd,
        "files": files,
        "truncated": truncated || result.output_truncated,
    }))
}

pub async fn search_files(
    state: &AppState,
    query: &str,
    path: Option<&str>,
    glob: Option<&str>,
    max_results: Option<u64>,
) -> ApiResult<Value> {
    if query.trim().is_empty() {
        return Err(AppError::bad_request("搜索内容不能为空。"));
    }
    let cwd = resolve_cwd(state, path)?;
    let limit = normalize_result_limit(max_results);
    let mut args = vec![
        "--line-number".to_string(),
        "--no-heading".to_string(),
        "--color".to_string(),
        "never".to_string(),
    ];
    if let Some(glob) = glob.filter(|value| !value.trim().is_empty()) {
        args.push("--glob".to_string());
        args.push(glob.to_string());
    }
    args.push("--".to_string());
    args.push(query.to_string());
    args.push(".".to_string());
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let ripgrep = ripgrep_executable(state)?;
    let result =
        run_argv(&ripgrep.to_string_lossy(), &refs, &cwd, DEFAULT_TIMEOUT_SECS, None).await?;
    // ripgrep uses status 1 when there are no matches.
    if result.exit_code != 0 && result.exit_code != 1 {
        return Err(AppError::internal(format!("搜索失败: {}", result.stderr.trim())));
    }
    let (matches, truncated) = limited_lines(&result.stdout, limit);
    Ok(json!({
        "cwd": cwd,
        "matches": matches,
        "truncated": truncated || result.output_truncated,
    }))
}

pub async fn read_file(
    state: &AppState,
    path: &str,
    cwd: Option<&str>,
    start_line: Option<u64>,
    line_count: Option<u64>,
) -> ApiResult<Value> {
    let base_dir = match cwd.filter(|value| !value.trim().is_empty()) {
        Some(cwd) => filesystem::resolve_terminal_directory_path(&state.workspace_root(), cwd)?,
        None => state.workspace_root(),
    };
    let file = filesystem::resolve_file_path(&base_dir, path)?;
    let metadata = fs::metadata(&file)
        .await
        .map_err(|error| AppError::not_found(format!("文件不存在: {error}")))?;
    if metadata.len() > MAX_FILE_BYTES {
        return Err(AppError::bad_request(format!(
            "文件超过读取上限（{} bytes）。",
            MAX_FILE_BYTES
        )));
    }
    let content = fs::read_to_string(&file)
        .await
        .map_err(|error| AppError::bad_request(format!("文件不是有效 UTF-8 文本: {error}")))?;
    let start = start_line.unwrap_or(1).max(1) as usize;
    let count = line_count.unwrap_or(400).clamp(1, 2_000) as usize;
    let lines = content.lines().collect::<Vec<_>>();
    let selected = lines
        .iter()
        .skip(start - 1)
        .take(count)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");
    Ok(json!({
        "path": file,
        "start_line": start,
        "line_count": selected.lines().count(),
        "total_lines": lines.len(),
        "truncated": start - 1 + count < lines.len(),
        "content": selected,
    }))
}

pub async fn apply_patch(state: &AppState, patch: &str, cwd: Option<&str>) -> ApiResult<Value> {
    if patch.trim().is_empty() {
        return Err(AppError::bad_request("补丁不能为空。"));
    }
    if patch.len() > MAX_PATCH_BYTES {
        return Err(AppError::bad_request(format!(
            "补丁超过大小上限（{} bytes）。",
            MAX_PATCH_BYTES
        )));
    }
    validate_patch_paths(patch)?;
    let cwd = resolve_cwd(state, cwd)?;
    ensure_git_repository(&cwd).await?;

    let auto_checkpoint = match write_auto_checkpoint(state, &cwd).await {
        Ok(value) => value,
        Err(_) => None,
    };
    let check = run_argv(
        "git",
        &["apply", "--check", "--whitespace=nowarn", "-"],
        &cwd,
        DEFAULT_TIMEOUT_SECS,
        Some(patch.as_bytes()),
    )
    .await?;
    if check.exit_code != 0 {
        return Err(AppError::bad_request(format!("补丁检查失败: {}", check.stderr.trim())));
    }
    let applied = run_argv(
        "git",
        &["apply", "--whitespace=nowarn", "-"],
        &cwd,
        DEFAULT_TIMEOUT_SECS,
        Some(patch.as_bytes()),
    )
    .await?;
    if applied.exit_code != 0 {
        return Err(AppError::internal(format!("补丁应用失败: {}", applied.stderr.trim())));
    }
    let mut result = json!({"applied": true, "cwd": cwd});
    if let Some((checkpoint_id, path, bytes)) = auto_checkpoint {
        result["checkpoint_id"] = json!(checkpoint_id);
        result["checkpoint_path"] = json!(path);
        result["checkpoint_bytes"] = json!(bytes);
        result["auto_checkpoint"] = json!(true);
    }
    Ok(result)
}

pub async fn run_command(
    state: &AppState,
    command: &str,
    cwd: Option<&str>,
    timeout_secs: Option<u64>,
) -> ApiResult<Value> {
    if command.trim().is_empty() {
        return Err(AppError::bad_request("命令不能为空。"));
    }
    let cwd = resolve_cwd(state, cwd)?;
    let timeout_secs = normalize_timeout_secs(timeout_secs);
    let result = run_bounded_shell(command, &cwd, timeout_secs).await?;
    Ok(result.into_json(&cwd, Some(command)))
}

pub async fn run_verification(
    state: &AppState,
    command: &str,
    cwd: Option<&str>,
    timeout_secs: Option<u64>,
) -> ApiResult<Value> {
    let cwd = resolve_cwd(state, cwd)?;
    let timeout_secs = normalize_timeout_secs(timeout_secs);
    let result = run_bounded_shell(command, &cwd, timeout_secs).await?;
    Ok(json!({
        "passed": !result.timed_out && result.exit_code == 0,
        "command": command,
        "cwd": cwd,
        "exit_code": result.exit_code,
        "timed_out": result.timed_out,
        "duration_ms": result.duration_ms,
        "stdout": result.stdout,
        "stderr": result.stderr,
        "output_truncated": result.output_truncated,
    }))
}

pub async fn git_diff(state: &AppState, cwd: Option<&str>, path: Option<&str>) -> ApiResult<Value> {
    let cwd = resolve_cwd(state, cwd)?;
    ensure_git_repository(&cwd).await?;
    let status = run_argv("git", &["status", "--short"], &cwd, DEFAULT_TIMEOUT_SECS, None).await?;
    let mut args = vec!["diff", "--no-ext-diff", "--binary"];
    let validated_path;
    if let Some(path) = path.filter(|value| !value.trim().is_empty()) {
        validated_path = validate_relative_path(path)?;
        args.push("--");
        args.push(
            validated_path
                .to_str()
                .ok_or_else(|| AppError::bad_request("路径必须是有效 UTF-8。"))?,
        );
    }
    let diff = run_argv("git", &args, &cwd, DEFAULT_TIMEOUT_SECS, None).await?;
    if diff.exit_code != 0 {
        return Err(AppError::internal(format!("读取 Git 差异失败: {}", diff.stderr.trim())));
    }
    Ok(json!({
        "cwd": cwd,
        "status": status.stdout,
        "diff": diff.stdout,
        "truncated": status.output_truncated || diff.output_truncated,
    }))
}

pub async fn create_checkpoint(state: &AppState, cwd: Option<&str>) -> ApiResult<Value> {
    let cwd = resolve_cwd(state, cwd)?;
    let (id, checkpoint_path, bytes) = write_auto_checkpoint(state, &cwd)
        .await?
        .ok_or_else(|| AppError::bad_request("当前没有需要保存的已跟踪文件修改。"))?;
    Ok(json!({
        "checkpoint_id": id,
        "path": checkpoint_path,
        "bytes": bytes,
        "includes_untracked_files": false,
    }))
}

async fn write_auto_checkpoint(
    state: &AppState,
    cwd: &Path,
) -> ApiResult<Option<(String, PathBuf, usize)>> {
    ensure_git_repository(&cwd).await?;
    let root = run_argv("git", &["rev-parse", "--show-toplevel"], &cwd, DEFAULT_TIMEOUT_SECS, None)
        .await?;
    if root.exit_code != 0 {
        return Err(AppError::bad_request("工作目录不在 Git 仓库中。"));
    }
    let git_root = PathBuf::from(root.stdout.trim());
    if !filesystem::is_within_access_scope(&state.workspace_root(), &git_root) {
        return Err(AppError::bad_request("Git 仓库不在允许的工作区范围内。"));
    }
    let diff = run_argv_with_limit(
        "git",
        &["diff", "--binary", "HEAD"],
        &git_root,
        DEFAULT_TIMEOUT_SECS,
        None,
        MAX_CHECKPOINT_BYTES,
    )
    .await?;
    if diff.exit_code != 0 {
        return Err(AppError::bad_request(format!("创建检查点失败: {}", diff.stderr.trim())));
    }
    if diff.output_truncated {
        return Err(AppError::bad_request(format!(
            "检查点超过大小上限（{} bytes），未写入不完整补丁。",
            MAX_CHECKPOINT_BYTES
        )));
    }
    if diff.stdout.is_empty() {
        return Ok(None);
    }
    let id = format!(
        "auto-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    );
    let checkpoint_dir = git_root.join(".git/webclx-agent-checkpoints");
    fs::create_dir_all(&checkpoint_dir)
        .await
        .map_err(|error| AppError::internal(format!("创建检查点目录失败: {error}")))?;
    let checkpoint_path = checkpoint_dir.join(format!("{id}.patch"));
    fs::write(&checkpoint_path, diff.stdout.as_bytes())
        .await
        .map_err(|error| AppError::internal(format!("写入检查点失败: {error}")))?;
    Ok(Some((id, checkpoint_path, diff.stdout.len())))
}

async fn resolve_checkpoint_dir(
    state: &AppState,
    cwd: Option<&str>,
) -> ApiResult<(PathBuf, PathBuf)> {
    let cwd = resolve_cwd(state, cwd)?;
    ensure_git_repository(&cwd).await?;
    let root = run_argv("git", &["rev-parse", "--show-toplevel"], &cwd, DEFAULT_TIMEOUT_SECS, None)
        .await?;
    if root.exit_code != 0 {
        return Err(AppError::bad_request("工作目录不在 Git 仓库中。"));
    }
    let git_root = PathBuf::from(root.stdout.trim());
    if !filesystem::is_within_access_scope(&state.workspace_root(), &git_root) {
        return Err(AppError::bad_request("Git 仓库不在允许的工作区范围内。"));
    }
    Ok((git_root.clone(), git_root.join(".git/webclx-agent-checkpoints")))
}

fn validate_checkpoint_id(checkpoint_id: &str) -> ApiResult<()> {
    let id = checkpoint_id.trim();
    if id.is_empty()
        || id.len() > 128
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err(AppError::bad_request("checkpoint_id 无效：必须是创建检查点时返回的标识。"));
    }
    Ok(())
}

pub async fn list_checkpoints(state: &AppState, cwd: Option<&str>) -> ApiResult<Value> {
    let (git_root, checkpoint_dir) = resolve_checkpoint_dir(state, cwd).await?;
    let mut checkpoints = Vec::new();
    let mut entries = match fs::read_dir(&checkpoint_dir).await {
        Ok(entries) => entries,
        Err(_) => {
            return Ok(json!({"cwd": git_root, "checkpoints": checkpoints}));
        }
    };
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|error| AppError::internal(format!("读取检查点目录失败: {error}")))?
    {
        let path = entry.path();
        if path.extension().and_then(OsStr::to_str) != Some("patch") {
            continue;
        }
        let Ok(metadata) = fs::metadata(&path).await else {
            continue;
        };
        let Some(file_name) = path.file_stem().and_then(OsStr::to_str) else {
            continue;
        };
        checkpoints.push(json!({
            "checkpoint_id": file_name,
            "bytes": metadata.len(),
            "created_at": metadata
                .modified()
                .ok()
                .and_then(|modified| modified.duration_since(SystemTime::UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs())
                .unwrap_or(0),
        }));
    }
    checkpoints.sort_by(|left, right| {
        right["checkpoint_id"]
            .as_str()
            .cmp(&left["checkpoint_id"].as_str())
    });
    Ok(json!({"cwd": git_root, "checkpoints": checkpoints}))
}

pub async fn restore_checkpoint(
    state: &AppState,
    cwd: Option<&str>,
    checkpoint_id: &str,
) -> ApiResult<Value> {
    validate_checkpoint_id(checkpoint_id)?;
    let (git_root, checkpoint_dir) = resolve_checkpoint_dir(state, cwd).await?;
    let checkpoint_path = checkpoint_dir.join(format!("{}.patch", checkpoint_id.trim()));
    let canonical_path = checkpoint_path
        .canonicalize()
        .map_err(|_| AppError::not_found(format!("检查点 `{}` 不存在。", checkpoint_id.trim())))?;
    let canonical_dir = checkpoint_dir
        .canonicalize()
        .map_err(|error| AppError::internal(format!("解析检查点目录失败: {error}")))?;
    if !canonical_path.starts_with(&canonical_dir) || !canonical_path.is_file() {
        return Err(AppError::not_found(format!("检查点 `{}` 不存在。", checkpoint_id.trim())));
    }
    let patch_text = fs::read_to_string(&canonical_path)
        .await
        .unwrap_or_default();
    let mut files = Vec::new();
    for line in patch_text.lines() {
        if let Some(paths) = line.strip_prefix("diff --git ") {
            let paths = shell_words::split(paths)
                .map_err(|_| AppError::bad_request("检查点包含无法解析的文件头。"))?;
            if let Some(path) = paths
                .first()
                .and_then(|path| path.strip_prefix("a/"))
                .or_else(|| paths.first().map(String::as_str))
            {
                files.push(path.to_string());
            }
        }
    }
    let path_arg = canonical_path
        .to_str()
        .ok_or_else(|| AppError::bad_request("检查点路径不是有效 UTF-8。"))?;
    let applied = run_argv(
        "git",
        &["apply", "--binary", "--reverse", path_arg],
        &git_root,
        DEFAULT_TIMEOUT_SECS,
        None,
    )
    .await?;
    if applied.exit_code != 0 {
        return Err(AppError::bad_request(format!(
            "恢复检查点失败（工作区可能已被后续修改覆盖）: {}",
            applied.stderr.trim()
        )));
    }
    files.sort();
    files.dedup();
    Ok(json!({
        "checkpoint_id": checkpoint_id.trim(),
        "restored": true,
        "cwd": git_root,
        "files": files,
    }))
}

pub(super) fn resolve_cwd(state: &AppState, requested: Option<&str>) -> ApiResult<PathBuf> {
    filesystem::resolve_terminal_directory_path(
        &state.workspace_root(),
        requested.unwrap_or_default(),
    )
}

fn ripgrep_executable(state: &AppState) -> ApiResult<PathBuf> {
    let user = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("终端用户无效: {error}")))?;
    let command_path = crate::auth::codex_command_path_for_user(&user);
    let codex = crate::auth::resolve_codex_executable(&command_path);
    resolve_ripgrep_executable(std::env::var_os("PATH").as_deref(), &codex).ok_or_else(|| {
        AppError::internal("未找到 ripgrep（rg）；系统 PATH 和 Codex 平台包中均不存在。")
    })
}

fn resolve_ripgrep_executable(
    path_env: Option<&OsStr>,
    codex_executable: &Path,
) -> Option<PathBuf> {
    let file_name = if cfg!(windows) { "rg.exe" } else { "rg" };
    if let Some(path_env) = path_env {
        for directory in std::env::split_paths(path_env).filter(|path| !path.as_os_str().is_empty())
        {
            let candidate = directory.join(file_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    let codex = codex_executable
        .canonicalize()
        .unwrap_or_else(|_| codex_executable.to_path_buf());
    let package_root = codex.parent()?.parent()?;
    let platform_package_root = package_root.join("node_modules").join("@openai");
    let mut platform_packages = std::fs::read_dir(platform_package_root)
        .ok()?
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("codex-"))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    platform_packages.sort();

    for platform_package in platform_packages {
        let vendor_root = platform_package.join("vendor");
        let mut targets = match std::fs::read_dir(vendor_root) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .collect::<Vec<_>>(),
            Err(_) => continue,
        };
        targets.sort();
        for target in targets {
            let candidate = target.join("codex-path").join(file_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

async fn ensure_git_repository(cwd: &Path) -> ApiResult<()> {
    let result =
        run_argv("git", &["rev-parse", "--is-inside-work-tree"], cwd, DEFAULT_TIMEOUT_SECS, None)
            .await?;
    if result.exit_code != 0 || result.stdout.trim() != "true" {
        return Err(AppError::bad_request("工作目录不在 Git 仓库中。"));
    }
    Ok(())
}

fn normalize_timeout_secs(requested: Option<u64>) -> u64 {
    requested
        .unwrap_or(DEFAULT_TIMEOUT_SECS)
        .clamp(MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS)
}

fn normalize_result_limit(requested: Option<u64>) -> usize {
    requested
        .unwrap_or(DEFAULT_RESULT_LIMIT as u64)
        .clamp(1, MAX_RESULT_LIMIT as u64) as usize
}

fn validate_relative_path(raw: &str) -> ApiResult<PathBuf> {
    let path = Path::new(raw.trim());
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err(AppError::bad_request("必须使用非空的工作区相对路径。"));
    }
    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(value) => clean.push(value),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::bad_request("路径不能离开当前工作目录。"));
            }
        }
    }
    Ok(clean)
}

fn validate_patch_paths(patch: &str) -> ApiResult<()> {
    let mut header_count = 0usize;
    for line in patch.lines() {
        if let Some(paths) = line.strip_prefix("diff --git ") {
            let paths = shell_words::split(paths)
                .map_err(|_| AppError::bad_request("补丁包含无法解析的 diff --git 路径。"))?;
            if paths.len() != 2 {
                return Err(AppError::bad_request("补丁的 diff --git 文件头无效。"));
            }
            validate_patch_path(&paths[0])?;
            validate_patch_path(&paths[1])?;
            continue;
        }
        if let Some(path) = ["rename from ", "rename to ", "copy from ", "copy to "]
            .iter()
            .find_map(|prefix| line.strip_prefix(prefix))
        {
            validate_patch_path(path)?;
            continue;
        }
        let raw = if let Some(path) = line.strip_prefix("--- ") {
            Some(path)
        } else {
            line.strip_prefix("+++ ")
        };
        let Some(raw) = raw else { continue };
        header_count += 1;
        let path = raw.split('\t').next().unwrap_or(raw).trim();
        validate_patch_path(path)?;
    }
    if header_count < 2 || !header_count.is_multiple_of(2) {
        return Err(AppError::bad_request("补丁缺少完整的 ---/+++ 文件头。"));
    }
    Ok(())
}

fn validate_patch_path(path: &str) -> ApiResult<()> {
    let path = path.trim();
    if path == "/dev/null" {
        return Ok(());
    }
    if path.starts_with('"') {
        return Err(AppError::bad_request("补丁不支持带引号的路径。"));
    }
    let path = path
        .strip_prefix("a/")
        .or_else(|| path.strip_prefix("b/"))
        .unwrap_or(path);
    validate_relative_path(path).map(|_| ())
}

fn limited_lines(output: &str, limit: usize) -> (Vec<&str>, bool) {
    let mut lines = output.lines();
    let values = lines.by_ref().take(limit).collect::<Vec<_>>();
    let truncated = lines.next().is_some();
    (values, truncated)
}

struct ProcessResult {
    exit_code: i32,
    stdout: String,
    stderr: String,
    timed_out: bool,
    duration_ms: u128,
    output_truncated: bool,
}

impl ProcessResult {
    fn into_json(self, cwd: &Path, command: Option<&str>) -> Value {
        json!({
            "command": command,
            "cwd": cwd,
            "exit_code": self.exit_code,
            "stdout": self.stdout,
            "stderr": self.stderr,
            "timed_out": self.timed_out,
            "duration_ms": self.duration_ms,
            "output_truncated": self.output_truncated,
        })
    }
}

async fn run_argv(
    program: &str,
    args: &[&str],
    cwd: &Path,
    timeout_secs: u64,
    stdin: Option<&[u8]>,
) -> ApiResult<ProcessResult> {
    run_argv_with_limit(program, args, cwd, timeout_secs, stdin, MAX_OUTPUT_BYTES).await
}

async fn run_bounded_shell(
    command: &str,
    cwd: &Path,
    timeout_secs: u64,
) -> ApiResult<ProcessResult> {
    let deadline = format!("{timeout_secs}s");
    let mut result = run_argv_with_limit(
        "timeout",
        &[
            "--signal=TERM",
            "--kill-after=2s",
            &deadline,
            "bash",
            "-lc",
            command,
        ],
        cwd,
        timeout_secs.saturating_add(4),
        None,
        MAX_OUTPUT_BYTES,
    )
    .await?;
    if matches!(result.exit_code, 124 | 137) {
        result.timed_out = true;
        if result.stderr.trim().is_empty() {
            result.stderr = format!("命令超过 {timeout_secs} 秒限制，已终止进程组。");
        }
    }
    Ok(result)
}

async fn run_argv_with_limit(
    program: &str,
    args: &[&str],
    cwd: &Path,
    timeout_secs: u64,
    stdin: Option<&[u8]>,
    output_limit: usize,
) -> ApiResult<ProcessResult> {
    let started = Instant::now();
    let mut command = Command::new(program);
    command
        .args(args)
        .current_dir(cwd)
        .kill_on_drop(true)
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| AppError::internal(format!("启动 {program} 失败: {error}")))?;
    if let Some(input) = stdin {
        let mut child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::internal("无法写入子进程标准输入。"))?;
        child_stdin
            .write_all(input)
            .await
            .map_err(|error| AppError::internal(format!("写入子进程失败: {error}")))?;
    }
    let duration = Duration::from_secs(timeout_secs.clamp(1, MAX_TIMEOUT_SECS + 10));
    let output = match timeout(duration, child.wait_with_output()).await {
        Ok(result) => {
            result.map_err(|error| AppError::internal(format!("等待 {program} 失败: {error}")))?
        }
        Err(_) => {
            return Ok(ProcessResult {
                exit_code: -1,
                stdout: String::new(),
                stderr: format!("命令超过 {timeout_secs} 秒限制，已终止。"),
                timed_out: true,
                duration_ms: started.elapsed().as_millis(),
                output_truncated: false,
            });
        }
    };
    let (stdout, stdout_truncated) = truncate_bytes_with_limit(&output.stdout, output_limit);
    let (stderr, stderr_truncated) = truncate_bytes_with_limit(&output.stderr, output_limit);
    Ok(ProcessResult {
        exit_code: output.status.code().unwrap_or(-1),
        stdout,
        stderr,
        timed_out: false,
        duration_ms: started.elapsed().as_millis(),
        output_truncated: stdout_truncated || stderr_truncated,
    })
}

fn truncate_bytes(bytes: &[u8]) -> (String, bool) {
    truncate_bytes_with_limit(bytes, MAX_OUTPUT_BYTES)
}

fn truncate_bytes_with_limit(bytes: &[u8], limit: usize) -> (String, bool) {
    if bytes.len() <= limit {
        return (String::from_utf8_lossy(bytes).into_owned(), false);
    }
    let half = limit / 2;
    let omitted = bytes.len() - limit;
    let value = format!(
        "{}\n\n... [output truncated, {omitted} bytes omitted] ...\n\n{}",
        String::from_utf8_lossy(&bytes[..half]),
        String::from_utf8_lossy(&bytes[bytes.len() - half..]),
    );
    (value, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ripgrep_resolution_falls_back_to_the_codex_platform_package() {
        let root =
            std::env::temp_dir().join(format!("webclx-agent-rg-resolution-{}", std::process::id()));
        let package_root = root.join("codex");
        let codex = package_root.join("bin/codex.js");
        let bundled_rg = package_root
            .join("node_modules/@openai/codex-linux-test/vendor/test-target/codex-path")
            .join(if cfg!(windows) { "rg.exe" } else { "rg" });
        std::fs::create_dir_all(codex.parent().expect("codex bin dir"))
            .expect("create codex bin dir");
        std::fs::create_dir_all(bundled_rg.parent().expect("bundled rg dir"))
            .expect("create bundled rg dir");
        std::fs::write(&codex, "").expect("write fake codex launcher");
        std::fs::write(&bundled_rg, "").expect("write fake bundled rg");

        let resolved = resolve_ripgrep_executable(Some(std::ffi::OsStr::new("")), &codex);
        assert_eq!(resolved.as_deref(), Some(bundled_rg.as_path()));

        std::fs::remove_dir_all(root).expect("remove rg resolution fixture");
    }

    #[test]
    fn relative_paths_reject_escape_and_absolute_inputs() {
        assert!(validate_relative_path("src/agent.rs").is_ok());
        assert!(validate_relative_path("../outside").is_err());
        assert!(validate_relative_path("/etc/passwd").is_err());
    }

    #[test]
    fn patch_headers_reject_paths_outside_the_working_directory() {
        let patch = "--- a/src/ok.rs\n+++ b/../outside.rs\n@@ -1 +1 @@\n-a\n+b\n";
        assert!(validate_patch_paths(patch).is_err());
    }

    #[test]
    fn patch_metadata_cannot_introduce_unchecked_paths() {
        let patch = "diff --git a/ok.rs b/ok.rs\nrename from ok.rs\nrename to ../outside.rs\n--- a/ok.rs\n+++ b/ok.rs\n@@ -1 +1 @@\n-a\n+b\n";
        assert!(validate_patch_paths(patch).is_err());
    }

    #[test]
    fn oversized_patches_are_rejected_before_process_execution() {
        let patch = format!("--- a/a\n+++ b/a\n{}", "x".repeat(MAX_PATCH_BYTES));
        assert!(patch.len() > MAX_PATCH_BYTES);
    }

    #[test]
    fn command_timeout_is_bounded() {
        assert_eq!(normalize_timeout_secs(Some(0)), MIN_TIMEOUT_SECS);
        assert_eq!(normalize_timeout_secs(None), DEFAULT_TIMEOUT_SECS);
        assert_eq!(normalize_timeout_secs(Some(10_000)), MAX_TIMEOUT_SECS);
    }

    #[test]
    fn checkpoint_ids_reject_path_traversal_and_bad_characters() {
        assert!(validate_checkpoint_id("1754000000000").is_ok());
        assert!(validate_checkpoint_id("checkpoint-2026_08").is_ok());
        assert!(validate_checkpoint_id("").is_err());
        assert!(validate_checkpoint_id("../outside").is_err());
        assert!(validate_checkpoint_id("a/b").is_err());
        assert!(validate_checkpoint_id("a; rm -rf /").is_err());
        assert!(validate_checkpoint_id(&"x".repeat(200)).is_err());
    }

    #[test]
    fn output_truncation_is_utf8_safe() {
        let input = "你".repeat(MAX_OUTPUT_BYTES);
        let (output, truncated) = truncate_bytes(input.as_bytes());
        assert!(truncated);
        assert!(output.contains("output truncated"));
    }
}
