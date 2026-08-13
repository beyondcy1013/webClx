use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use auth_core::{
    AUTH_FILE_RELATIVE_PATH, CLAUDE_ONBOARDING_BYPASS_FILE, CLAUDE_SETTINGS_FILE_RELATIVE_PATH,
    CONFIG_FILE_RELATIVE_PATH,
};
use axum::{
    Json,
    extract::{Query, State},
};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, value};

use crate::{ApiResult, AppError, AppState, runtime_paths};

const CODEX_FULL_ACCESS_APPROVAL_POLICY: &str = "never";
const CODEX_FULL_ACCESS_SANDBOX_MODE: &str = "danger-full-access";
const CODEX_FULL_ACCESS_BACKUP_RELATIVE_PATH: &str = ".codex/.webclx-full-access-backup.json";

#[derive(Debug, Deserialize)]
pub struct ConfigFileQuery {
    #[serde(default)]
    key: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveConfigFileRequest {
    key: String,
    content: String,
}

#[derive(Debug, Serialize)]
pub struct ConfigFileOption {
    key: &'static str,
    group: &'static str,
    label: &'static str,
    relative_path: &'static str,
    display_path: String,
    exists: bool,
}

#[derive(Debug, Serialize)]
pub struct ConfigFileResponse {
    user: String,
    user_home: String,
    selected_key: &'static str,
    path: String,
    display_path: String,
    exists: bool,
    content: String,
    options: Vec<ConfigFileOption>,
}

#[derive(Debug, Serialize)]
pub struct CodexFullAccessResponse {
    ok: bool,
    enabled: bool,
    user: String,
    config_file: String,
    approval_policy: &'static str,
    sandbox_mode: &'static str,
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub struct CodexCommonConfigSelection {
    approval_never: bool,
    sandbox_full_access: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodexCommonConfigState {
    approval_never: bool,
    sandbox_full_access: bool,
    approval_policy: Option<String>,
    sandbox_mode: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CodexCommonConfigResponse {
    ok: bool,
    user: String,
    config_file: String,
    exists: bool,
    approval_never: bool,
    sandbox_full_access: bool,
    approval_policy: Option<String>,
    sandbox_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
struct CodexFullAccessBackup {
    approval_policy: Option<String>,
    sandbox_mode: Option<String>,
}

#[derive(Clone, Copy)]
struct ConfigFileTarget {
    key: &'static str,
    group: &'static str,
    label: &'static str,
    relative_path: &'static str,
}

const CONFIG_FILE_TARGETS: &[ConfigFileTarget] = &[
    ConfigFileTarget {
        key: "codex_config",
        group: "Codex 配置",
        label: "config.toml",
        relative_path: CONFIG_FILE_RELATIVE_PATH,
    },
    ConfigFileTarget {
        key: "codex_auth",
        group: "Codex 配置",
        label: "auth.json",
        relative_path: AUTH_FILE_RELATIVE_PATH,
    },
    ConfigFileTarget {
        key: "claude_settings",
        group: "Claude 配置",
        label: "settings.json",
        relative_path: CLAUDE_SETTINGS_FILE_RELATIVE_PATH,
    },
    ConfigFileTarget {
        key: "claude_onboarding",
        group: "Claude 配置",
        label: "登录态/session",
        relative_path: CLAUDE_ONBOARDING_BYPASS_FILE,
    },
];

pub async fn read_config_file(
    State(state): State<AppState>,
    Query(query): Query<ConfigFileQuery>,
) -> ApiResult<Json<ConfigFileResponse>> {
    let profile = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("读取终端用户失败: {error}")))?;
    let target = resolve_config_target(&query.key)?;
    let path = profile.home.join(target.relative_path);
    let canonical_home = canonical_user_home(&profile.home)?;
    let exists = validate_existing_config_path(&canonical_home, &path).await?;
    let content = if exists {
        tokio::fs::read_to_string(&path)
            .await
            .map_err(|error| AppError::internal(format!("读取配置文件失败: {error}")))?
    } else {
        String::new()
    };

    Ok(Json(config_file_response(
        &profile.name,
        &profile.home,
        target,
        &path,
        exists,
        content,
    )))
}

pub async fn save_config_file(
    State(state): State<AppState>,
    Json(payload): Json<SaveConfigFileRequest>,
) -> ApiResult<Json<ConfigFileResponse>> {
    let profile = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("读取终端用户失败: {error}")))?;
    let target = resolve_config_target(&payload.key)?;
    let path = profile.home.join(target.relative_path);
    let canonical_home = canonical_user_home(&profile.home)?;

    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|error| AppError::internal(format!("创建配置目录失败: {error}")))?;
        let canonical_parent = parent
            .canonicalize()
            .map_err(|error| AppError::internal(format!("读取配置目录失败: {error}")))?;
        if !is_within_home(&canonical_home, &canonical_parent) {
            return Err(AppError::bad_request("配置目录不在当前终端用户 HOME 下。"));
        }
    }

    if validate_existing_config_path(&canonical_home, &path).await? {
        let metadata = tokio::fs::metadata(&path)
            .await
            .map_err(|error| AppError::internal(format!("读取配置文件信息失败: {error}")))?;
        if !metadata.is_file() {
            return Err(AppError::bad_request("配置路径不是普通文件。"));
        }
    }

    tokio::fs::write(&path, payload.content.as_bytes())
        .await
        .map_err(|error| AppError::internal(format!("保存配置文件失败: {error}")))?;

    Ok(Json(config_file_response(
        &profile.name,
        &profile.home,
        target,
        &path,
        true,
        payload.content,
    )))
}

pub async fn read_codex_common_config(
    State(state): State<AppState>,
) -> ApiResult<Json<CodexCommonConfigResponse>> {
    let _active_config_guard = state.auth_manager.lock_active_config_write().await;
    let profile = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("读取终端用户失败: {error}")))?;
    let config_path = profile.home.join(CONFIG_FILE_RELATIVE_PATH);
    let canonical_home = canonical_user_home(&profile.home)?;
    let exists = validate_existing_config_path(&canonical_home, &config_path).await?;
    let content = read_optional_codex_config(&config_path, exists).await?;
    let current = codex_common_config_state_from_content(&content)
        .map_err(|error| AppError::bad_request(format!("解析 Codex config.toml 失败: {error}")))?;

    Ok(Json(codex_common_config_response(&profile.name, &config_path, exists, current)))
}

pub async fn save_codex_common_config(
    State(state): State<AppState>,
    Json(selection): Json<CodexCommonConfigSelection>,
) -> ApiResult<Json<CodexCommonConfigResponse>> {
    let _active_config_guard = state.auth_manager.lock_active_config_write().await;
    let profile = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("读取终端用户失败: {error}")))?;
    let config_path = profile.home.join(CONFIG_FILE_RELATIVE_PATH);
    let backup_path = profile.home.join(CODEX_FULL_ACCESS_BACKUP_RELATIVE_PATH);
    let canonical_home = canonical_user_home(&profile.home)?;
    let config_exists = validate_existing_config_path(&canonical_home, &config_path).await?;
    let existing = read_optional_codex_config(&config_path, config_exists).await?;
    let (updated, changed) = update_codex_common_config_in_content(&existing, selection)
        .map_err(|error| AppError::bad_request(format!("更新 Codex config.toml 失败: {error}")))?;

    if changed {
        ensure_codex_config_parent(&config_path, &canonical_home, &profile).await?;
        write_codex_config(&config_path, &updated, &profile).await?;

        // A manual per-setting change becomes authoritative, so an older bundled
        // full-access restore point must not overwrite it later.
        if validate_existing_config_path(&canonical_home, &backup_path).await? {
            tokio::fs::remove_file(&backup_path)
                .await
                .map_err(|error| AppError::internal(format!("清理 Codex 权限备份失败: {error}")))?;
        }
    }

    let current = codex_common_config_state_from_content(&updated)
        .map_err(|error| AppError::internal(format!("回读 Codex config.toml 失败: {error}")))?;
    Ok(Json(codex_common_config_response(
        &profile.name,
        &config_path,
        config_exists || changed,
        current,
    )))
}

pub async fn enable_codex_full_access(
    State(state): State<AppState>,
) -> ApiResult<Json<CodexFullAccessResponse>> {
    let _active_config_guard = state.auth_manager.lock_active_config_write().await;
    let profile = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("读取终端用户失败: {error}")))?;
    let config_path = profile.home.join(CONFIG_FILE_RELATIVE_PATH);
    let canonical_home = canonical_user_home(&profile.home)?;
    let config_exists = validate_existing_config_path(&canonical_home, &config_path).await?;
    let existing = if config_exists {
        tokio::fs::read_to_string(&config_path)
            .await
            .map_err(|error| AppError::internal(format!("读取 Codex 配置失败: {error}")))?
    } else {
        String::new()
    };
    let already_enabled = codex_full_access_enabled_in_content(&existing)
        .map_err(|error| AppError::internal(format!("读取 Codex 最高权限状态失败: {error}")))?;
    let backup = if already_enabled {
        None
    } else {
        Some(
            codex_full_access_backup_from_content(&existing)
                .map_err(|error| AppError::internal(format!("备份 Codex 权限配置失败: {error}")))?,
        )
    };
    let updated = enable_codex_full_access_in_content(&existing)
        .map_err(|error| AppError::internal(format!("更新 Codex 最高权限配置失败: {error}")))?;

    ensure_codex_config_parent(&config_path, &canonical_home, &profile).await?;
    if let Some(backup) = backup {
        let backup_path = profile.home.join(CODEX_FULL_ACCESS_BACKUP_RELATIVE_PATH);
        validate_existing_config_path(&canonical_home, &backup_path).await?;
        let encoded = serde_json::to_vec_pretty(&backup)
            .map_err(|error| AppError::internal(format!("编码 Codex 权限备份失败: {error}")))?;
        tokio::fs::write(&backup_path, encoded)
            .await
            .map_err(|error| AppError::internal(format!("保存 Codex 权限备份失败: {error}")))?;
        set_user_owned_path_mode(&backup_path, &profile, 0o600)
            .map_err(|error| AppError::internal(format!("保护 Codex 权限备份失败: {error}")))?;
    }

    write_codex_config(&config_path, &updated, &profile).await?;

    Ok(Json(CodexFullAccessResponse {
        ok: true,
        enabled: true,
        user: profile.name,
        config_file: config_path.display().to_string(),
        approval_policy: CODEX_FULL_ACCESS_APPROVAL_POLICY,
        sandbox_mode: CODEX_FULL_ACCESS_SANDBOX_MODE,
    }))
}

pub async fn codex_full_access_status(
    State(state): State<AppState>,
) -> ApiResult<Json<CodexFullAccessResponse>> {
    let _active_config_guard = state.auth_manager.lock_active_config_write().await;
    let profile = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("读取终端用户失败: {error}")))?;
    let config_path = profile.home.join(CONFIG_FILE_RELATIVE_PATH);
    let canonical_home = canonical_user_home(&profile.home)?;
    let config_exists = validate_existing_config_path(&canonical_home, &config_path).await?;
    let existing = if config_exists {
        tokio::fs::read_to_string(&config_path)
            .await
            .map_err(|error| AppError::internal(format!("读取 Codex 配置失败: {error}")))?
    } else {
        String::new()
    };
    let enabled = codex_full_access_enabled_in_content(&existing)
        .map_err(|error| AppError::internal(format!("读取 Codex 最高权限状态失败: {error}")))?;

    Ok(Json(CodexFullAccessResponse {
        ok: true,
        enabled,
        user: profile.name,
        config_file: config_path.display().to_string(),
        approval_policy: CODEX_FULL_ACCESS_APPROVAL_POLICY,
        sandbox_mode: CODEX_FULL_ACCESS_SANDBOX_MODE,
    }))
}

pub async fn disable_codex_full_access(
    State(state): State<AppState>,
) -> ApiResult<Json<CodexFullAccessResponse>> {
    let _active_config_guard = state.auth_manager.lock_active_config_write().await;
    let profile = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("读取终端用户失败: {error}")))?;
    let config_path = profile.home.join(CONFIG_FILE_RELATIVE_PATH);
    let backup_path = profile.home.join(CODEX_FULL_ACCESS_BACKUP_RELATIVE_PATH);
    let canonical_home = canonical_user_home(&profile.home)?;
    let config_exists = validate_existing_config_path(&canonical_home, &config_path).await?;
    let existing = if config_exists {
        tokio::fs::read_to_string(&config_path)
            .await
            .map_err(|error| AppError::internal(format!("读取 Codex 配置失败: {error}")))?
    } else {
        String::new()
    };
    let backup_exists = validate_existing_config_path(&canonical_home, &backup_path).await?;
    let backup = if backup_exists {
        let encoded = tokio::fs::read(&backup_path)
            .await
            .map_err(|error| AppError::internal(format!("读取 Codex 权限备份失败: {error}")))?;
        Some(
            serde_json::from_slice::<CodexFullAccessBackup>(&encoded)
                .map_err(|error| AppError::internal(format!("解析 Codex 权限备份失败: {error}")))?,
        )
    } else {
        None
    };
    let currently_enabled = codex_full_access_enabled_in_content(&existing)
        .map_err(|error| AppError::internal(format!("读取 Codex 最高权限状态失败: {error}")))?;
    if !currently_enabled {
        if backup_exists {
            tokio::fs::remove_file(&backup_path)
                .await
                .map_err(|error| AppError::internal(format!("清理 Codex 权限备份失败: {error}")))?;
        }
        return Ok(Json(CodexFullAccessResponse {
            ok: true,
            enabled: false,
            user: profile.name,
            config_file: config_path.display().to_string(),
            approval_policy: CODEX_FULL_ACCESS_APPROVAL_POLICY,
            sandbox_mode: CODEX_FULL_ACCESS_SANDBOX_MODE,
        }));
    }
    let updated = disable_codex_full_access_in_content(&existing, backup.as_ref())
        .map_err(|error| AppError::internal(format!("关闭 Codex 最高权限失败: {error}")))?;

    if config_exists || backup_exists {
        ensure_codex_config_parent(&config_path, &canonical_home, &profile).await?;
        write_codex_config(&config_path, &updated, &profile).await?;
    }
    if backup_exists {
        tokio::fs::remove_file(&backup_path)
            .await
            .map_err(|error| AppError::internal(format!("清理 Codex 权限备份失败: {error}")))?;
    }

    Ok(Json(CodexFullAccessResponse {
        ok: true,
        enabled: false,
        user: profile.name,
        config_file: config_path.display().to_string(),
        approval_policy: CODEX_FULL_ACCESS_APPROVAL_POLICY,
        sandbox_mode: CODEX_FULL_ACCESS_SANDBOX_MODE,
    }))
}

fn enable_codex_full_access_in_content(content: &str) -> Result<String> {
    let mut document = parse_codex_config(content)?;
    document["approval_policy"] = value(CODEX_FULL_ACCESS_APPROVAL_POLICY);
    document["sandbox_mode"] = value(CODEX_FULL_ACCESS_SANDBOX_MODE);
    Ok(serialize_codex_config(document))
}

fn codex_common_config_state_from_content(content: &str) -> Result<CodexCommonConfigState> {
    let document = parse_codex_config(content)?;
    codex_common_config_state_from_document(&document)
}

fn codex_common_config_state_from_document(
    document: &DocumentMut,
) -> Result<CodexCommonConfigState> {
    let approval_policy = optional_codex_string(document, "approval_policy")?;
    let sandbox_mode = optional_codex_string(document, "sandbox_mode")?;
    Ok(CodexCommonConfigState {
        approval_never: approval_policy.as_deref() == Some(CODEX_FULL_ACCESS_APPROVAL_POLICY),
        sandbox_full_access: sandbox_mode.as_deref() == Some(CODEX_FULL_ACCESS_SANDBOX_MODE),
        approval_policy,
        sandbox_mode,
    })
}

fn update_codex_common_config_in_content(
    content: &str,
    selection: CodexCommonConfigSelection,
) -> Result<(String, bool)> {
    let mut document = parse_codex_config(content)?;
    let current = codex_common_config_state_from_document(&document)?;
    let changed = current.approval_never != selection.approval_never
        || current.sandbox_full_access != selection.sandbox_full_access;
    if !changed {
        return Ok((content.to_string(), false));
    }

    update_codex_special_string(
        &mut document,
        "approval_policy",
        CODEX_FULL_ACCESS_APPROVAL_POLICY,
        selection.approval_never,
    );
    update_codex_special_string(
        &mut document,
        "sandbox_mode",
        CODEX_FULL_ACCESS_SANDBOX_MODE,
        selection.sandbox_full_access,
    );
    Ok((serialize_codex_config(document), true))
}

fn update_codex_special_string(
    document: &mut DocumentMut,
    key: &str,
    enabled_value: &str,
    enabled: bool,
) {
    if enabled {
        document[key] = value(enabled_value);
    } else if document.get(key).and_then(|item| item.as_str()) == Some(enabled_value) {
        document.remove(key);
    }
}

fn codex_full_access_backup_from_content(content: &str) -> Result<CodexFullAccessBackup> {
    let document = parse_codex_config(content)?;
    Ok(CodexFullAccessBackup {
        approval_policy: optional_codex_string(&document, "approval_policy")?,
        sandbox_mode: optional_codex_string(&document, "sandbox_mode")?,
    })
}

fn codex_full_access_enabled_in_content(content: &str) -> Result<bool> {
    let document = parse_codex_config(content)?;
    Ok(document
        .get("approval_policy")
        .and_then(|item| item.as_str())
        == Some(CODEX_FULL_ACCESS_APPROVAL_POLICY)
        && document.get("sandbox_mode").and_then(|item| item.as_str())
            == Some(CODEX_FULL_ACCESS_SANDBOX_MODE))
}

fn disable_codex_full_access_in_content(
    content: &str,
    backup: Option<&CodexFullAccessBackup>,
) -> Result<String> {
    let mut document = parse_codex_config(content)?;
    restore_codex_string(
        &mut document,
        "approval_policy",
        backup.and_then(|value| value.approval_policy.as_deref()),
    );
    restore_codex_string(
        &mut document,
        "sandbox_mode",
        backup.and_then(|value| value.sandbox_mode.as_deref()),
    );
    Ok(serialize_codex_config(document))
}

fn parse_codex_config(content: &str) -> Result<DocumentMut> {
    if content.trim().is_empty() {
        Ok(DocumentMut::new())
    } else {
        content
            .parse::<DocumentMut>()
            .context("cannot parse Codex config.toml")
    }
}

fn optional_codex_string(document: &DocumentMut, key: &str) -> Result<Option<String>> {
    let Some(item) = document.get(key) else {
        return Ok(None);
    };
    item.as_str()
        .map(|value| Some(value.to_string()))
        .ok_or_else(|| anyhow::anyhow!("Codex config key {key} must be a string"))
}

fn restore_codex_string(document: &mut DocumentMut, key: &str, previous: Option<&str>) {
    if let Some(previous) = previous {
        document[key] = value(previous);
    } else {
        document.remove(key);
    }
}

fn serialize_codex_config(document: DocumentMut) -> String {
    let mut content = document.to_string();
    if !content.ends_with('\n') {
        content.push('\n');
    }
    content
}

async fn ensure_codex_config_parent(
    config_path: &Path,
    canonical_home: &Path,
    profile: &runtime_paths::UserProfile,
) -> ApiResult<()> {
    let parent = config_path
        .parent()
        .ok_or_else(|| AppError::internal("Codex 配置路径缺少父目录。"))?;
    let parent_existed = parent.exists();
    tokio::fs::create_dir_all(parent)
        .await
        .map_err(|error| AppError::internal(format!("创建 Codex 配置目录失败: {error}")))?;
    let canonical_parent = parent
        .canonicalize()
        .map_err(|error| AppError::internal(format!("读取 Codex 配置目录失败: {error}")))?;
    if !is_within_home(canonical_home, &canonical_parent) {
        return Err(AppError::bad_request("Codex 配置目录不在当前终端用户 HOME 下。"));
    }
    if !parent_existed {
        set_user_owned_path_mode(parent, profile, 0o700)
            .map_err(|error| AppError::internal(format!("保护 Codex 配置目录失败: {error}")))?;
    }
    Ok(())
}

async fn read_optional_codex_config(path: &Path, exists: bool) -> ApiResult<String> {
    if !exists {
        return Ok(String::new());
    }
    tokio::fs::read_to_string(path)
        .await
        .map_err(|error| AppError::internal(format!("读取 Codex 配置失败: {error}")))
}

fn codex_common_config_response(
    user: &str,
    config_path: &Path,
    exists: bool,
    state: CodexCommonConfigState,
) -> CodexCommonConfigResponse {
    CodexCommonConfigResponse {
        ok: true,
        user: user.to_string(),
        config_file: config_path.display().to_string(),
        exists,
        approval_never: state.approval_never,
        sandbox_full_access: state.sandbox_full_access,
        approval_policy: state.approval_policy,
        sandbox_mode: state.sandbox_mode,
    }
}

async fn write_codex_config(
    config_path: &Path,
    content: &str,
    profile: &runtime_paths::UserProfile,
) -> ApiResult<()> {
    tokio::fs::write(config_path, content.as_bytes())
        .await
        .map_err(|error| AppError::internal(format!("保存 Codex 配置失败: {error}")))?;
    set_user_owned_path_mode(config_path, profile, 0o600)
        .map_err(|error| AppError::internal(format!("保护 Codex 配置文件失败: {error}")))?;
    Ok(())
}

#[cfg(unix)]
fn set_user_owned_path_mode(
    path: &Path,
    profile: &runtime_paths::UserProfile,
    mode: u32,
) -> Result<()> {
    use std::{
        ffi::CString,
        os::unix::{ffi::OsStrExt, fs::MetadataExt, fs::PermissionsExt},
    };

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("cannot chmod {}", path.display()))?;
    let metadata =
        std::fs::metadata(path).with_context(|| format!("cannot stat {}", path.display()))?;
    if metadata.uid() == profile.uid && metadata.gid() == profile.gid {
        return Ok(());
    }
    if unsafe { libc::geteuid() } != 0 {
        anyhow::bail!("当前进程无权修改 {} 的所有者", path.display());
    }
    let encoded = CString::new(path.as_os_str().as_bytes()).context("Codex 配置路径包含 NUL")?;
    if unsafe { libc::chown(encoded.as_ptr(), profile.uid, profile.gid) } != 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("修改 {} 的所有者失败", path.display()));
    }
    Ok(())
}

#[cfg(not(unix))]
fn set_user_owned_path_mode(
    _path: &Path,
    _profile: &runtime_paths::UserProfile,
    _mode: u32,
) -> Result<()> {
    Ok(())
}

fn resolve_config_target(key: &str) -> ApiResult<ConfigFileTarget> {
    let normalized = key.trim();
    if normalized.is_empty() {
        return Ok(CONFIG_FILE_TARGETS[0]);
    }
    CONFIG_FILE_TARGETS
        .iter()
        .copied()
        .find(|target| target.key == normalized)
        .ok_or_else(|| AppError::bad_request("未知配置文件。"))
}

fn canonical_user_home(home: &Path) -> ApiResult<PathBuf> {
    home.canonicalize()
        .map_err(|error| AppError::not_found(format!("用户 HOME 不存在: {error}")))
}

async fn validate_existing_config_path(canonical_home: &Path, path: &Path) -> ApiResult<bool> {
    let path_metadata = match tokio::fs::symlink_metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(AppError::internal(format!("读取配置文件信息失败: {error}"))),
    };
    if !path_metadata.is_file() && !path_metadata.file_type().is_symlink() {
        return Err(AppError::bad_request("配置路径不是普通文件。"));
    }
    let canonical_path = path
        .canonicalize()
        .map_err(|error| AppError::not_found(format!("配置文件不存在: {error}")))?;
    let metadata = tokio::fs::metadata(&canonical_path)
        .await
        .map_err(|error| AppError::not_found(format!("配置文件不存在: {error}")))?;
    if !metadata.is_file() {
        return Err(AppError::bad_request("配置路径不是普通文件。"));
    }
    if !is_within_home(canonical_home, &canonical_path) {
        return Err(AppError::bad_request("配置文件不在当前终端用户 HOME 下。"));
    }
    Ok(true)
}

fn is_within_home(canonical_home: &Path, path: &Path) -> bool {
    path == canonical_home || path.starts_with(canonical_home)
}

fn config_file_response(
    user: &str,
    home: &Path,
    target: ConfigFileTarget,
    path: &Path,
    exists: bool,
    content: String,
) -> ConfigFileResponse {
    ConfigFileResponse {
        user: user.to_string(),
        user_home: home.display().to_string(),
        selected_key: target.key,
        path: path.display().to_string(),
        display_path: path.display().to_string(),
        exists,
        content,
        options: CONFIG_FILE_TARGETS
            .iter()
            .map(|candidate| {
                let candidate_path = home.join(candidate.relative_path);
                ConfigFileOption {
                    key: candidate.key,
                    group: candidate.group,
                    label: candidate.label,
                    relative_path: candidate.relative_path,
                    display_path: candidate_path.display().to_string(),
                    exists: candidate_path.is_file(),
                }
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CodexCommonConfigSelection, codex_common_config_state_from_content,
        codex_full_access_backup_from_content, codex_full_access_enabled_in_content,
        disable_codex_full_access_in_content, enable_codex_full_access_in_content,
        update_codex_common_config_in_content,
    };
    use toml_edit::DocumentMut;

    #[test]
    fn codex_full_access_update_preserves_existing_config() {
        let existing = r#"# keep this comment
model = "GLM-5.2"
approval_policy = "on-request"
sandbox_mode = "workspace-write"

[model_providers.webclx_api]
name = "Example API"
base_url = "https://api.example.com/v1"
wire_api = "responses"
"#;

        let updated = enable_codex_full_access_in_content(existing)
            .expect("Codex full access config should update");
        let document = updated
            .parse::<DocumentMut>()
            .expect("updated config should remain valid TOML");

        assert_eq!(
            document
                .get("approval_policy")
                .and_then(|item| item.as_str()),
            Some("never")
        );
        assert_eq!(
            document.get("sandbox_mode").and_then(|item| item.as_str()),
            Some("danger-full-access")
        );
        assert_eq!(document.get("model").and_then(|item| item.as_str()), Some("GLM-5.2"));
        assert!(updated.contains("# keep this comment"));
        assert!(updated.contains("[model_providers.webclx_api]"));
        assert!(updated.ends_with('\n'));
    }

    #[test]
    fn codex_full_access_update_creates_valid_config_from_empty_content() {
        let updated = enable_codex_full_access_in_content("")
            .expect("empty Codex config should support full access initialization");
        let document = updated
            .parse::<DocumentMut>()
            .expect("initialized config should be valid TOML");

        assert_eq!(
            document
                .get("approval_policy")
                .and_then(|item| item.as_str()),
            Some("never")
        );
        assert_eq!(
            document.get("sandbox_mode").and_then(|item| item.as_str()),
            Some("danger-full-access")
        );
    }

    #[test]
    fn codex_full_access_toggle_restores_previous_permission_values() {
        let existing = r#"model = "GLM-5.2"
approval_policy = "on-request"
sandbox_mode = "workspace-write"
"#;
        let backup = codex_full_access_backup_from_content(existing)
            .expect("existing permission values should be captured");
        let enabled = enable_codex_full_access_in_content(existing)
            .expect("Codex full access should be enabled");

        assert!(
            codex_full_access_enabled_in_content(&enabled)
                .expect("enabled config should have a readable state")
        );

        let restored = disable_codex_full_access_in_content(&enabled, Some(&backup))
            .expect("previous permission values should be restored");
        let document = restored
            .parse::<DocumentMut>()
            .expect("restored config should remain valid TOML");

        assert_eq!(
            document
                .get("approval_policy")
                .and_then(|item| item.as_str()),
            Some("on-request")
        );
        assert_eq!(
            document.get("sandbox_mode").and_then(|item| item.as_str()),
            Some("workspace-write")
        );
        assert_eq!(document.get("model").and_then(|item| item.as_str()), Some("GLM-5.2"));
    }

    #[test]
    fn codex_full_access_toggle_removes_permission_keys_that_were_absent() {
        let existing = "model = \"gpt-5.6\"\n";
        let backup = codex_full_access_backup_from_content(existing)
            .expect("missing permission values should still be captured");
        let enabled = enable_codex_full_access_in_content(existing)
            .expect("Codex full access should be enabled");
        let restored = disable_codex_full_access_in_content(&enabled, Some(&backup))
            .expect("missing permission values should be removed again");
        let document = restored
            .parse::<DocumentMut>()
            .expect("restored config should remain valid TOML");

        assert!(document.get("approval_policy").is_none());
        assert!(document.get("sandbox_mode").is_none());
        assert_eq!(document.get("model").and_then(|item| item.as_str()), Some("gpt-5.6"));
        assert!(
            !codex_full_access_enabled_in_content(&restored)
                .expect("restored config should have a readable state")
        );
    }

    #[test]
    fn codex_common_config_updates_supported_root_keys_independently() {
        let existing = r#"# keep this comment
model = "GLM-5.2"
approval_policy = "on-request"
sandbox_mode = "workspace-write"

[model_providers.webclx_api]
name = "Example API"
base_url = "https://api.example.com/v1"
wire_api = "responses"
"#;

        let (updated, changed) = update_codex_common_config_in_content(
            existing,
            CodexCommonConfigSelection {
                approval_never: true,
                sandbox_full_access: false,
            },
        )
        .expect("supported common settings should update");
        let document = updated
            .parse::<DocumentMut>()
            .expect("updated config should remain valid TOML");

        assert!(changed);
        assert_eq!(
            document
                .get("approval_policy")
                .and_then(|item| item.as_str()),
            Some("never")
        );
        assert_eq!(
            document.get("sandbox_mode").and_then(|item| item.as_str()),
            Some("workspace-write"),
            "an unchecked special mode must preserve a different explicit value"
        );
        assert!(updated.contains("# keep this comment"));
        assert!(updated.contains("[model_providers.webclx_api]"));
    }

    #[test]
    fn codex_common_config_unchecked_removes_only_matching_special_values() {
        let existing = r#"model = "gpt-5.6"
approval_policy = "never"
sandbox_mode = "danger-full-access"
"#;
        let (updated, changed) = update_codex_common_config_in_content(
            existing,
            CodexCommonConfigSelection {
                approval_never: false,
                sandbox_full_access: false,
            },
        )
        .expect("special values should be removable");
        let document = updated
            .parse::<DocumentMut>()
            .expect("updated config should remain valid TOML");
        let state = codex_common_config_state_from_content(&updated)
            .expect("updated common settings should be readable");

        assert!(changed);
        assert!(document.get("approval_policy").is_none());
        assert!(document.get("sandbox_mode").is_none());
        assert!(!state.approval_never);
        assert!(!state.sandbox_full_access);
        assert_eq!(document.get("model").and_then(|item| item.as_str()), Some("gpt-5.6"));
    }
}
