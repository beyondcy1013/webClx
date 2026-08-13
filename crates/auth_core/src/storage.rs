use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::{Map, Value};
use tokio::fs;
use toml_edit::{DocumentMut, value};
use tracing::warn;

use crate::{
    API_PRESETS_FILE_NAME, ApiAuthFile, ApiProviderOptions, AuthFile, AuthPresetDetails,
    AuthPresetManager, CLAUDE_ONBOARDING_BYPASS_FILE, CLAUDE_PRESETS_FILE_NAME, PRESETS_FILE_NAME,
    PresetConfigOverride, ResolvedConfigTarget, StoredApiPreset, StoredAuthPreset,
    StoredClaudePreset, SwitchCounted, UPSTREAM_PROXY_SETTINGS_FILE_NAME, UpstreamProxySettings,
    clear_inactive_managed_config_entries_in_content,
    clear_provider_and_set_config_entry_in_config_content, clear_provider_in_config_content,
    effective_claude_config_overrides, effective_preset_config_overrides,
    infer_api_responses_proxy, parse_claude_settings_document, sanitize_api_provider_name,
    sanitize_api_wire_api, sanitize_auth_token, sanitize_base_url, sanitize_claude_model,
    sanitize_management_url, sanitize_terminal_env_vars, sanitize_terminal_startup_script,
    set_api_provider_and_config_entry_in_config_content, set_claude_settings_in_value,
    validate_claude_code_endpoint_compatibility, validate_claude_model_selection,
};

const OWNER_ONLY_FILE_MODE: u32 = 0o600;
const DEFAULT_MODEL_CATALOG_FILE_NAME: &str = "model_catalog.json";
const CUSTOM_API_MODEL_DESCRIPTION: &str = "Custom API model routed through WebClx.";

impl AuthPresetManager {
    pub fn load(app_dir: &Path) -> Result<Self> {
        let config_subdir = if cfg!(windows) { "config" } else { "" };
        let base = if config_subdir.is_empty() {
            app_dir.to_path_buf()
        } else {
            app_dir.join(config_subdir)
        };
        let preset_file = base.join(PRESETS_FILE_NAME);
        let api_preset_file = base.join(API_PRESETS_FILE_NAME);
        let claude_preset_file = base.join(CLAUDE_PRESETS_FILE_NAME);
        let upstream_proxy_settings_file = base.join(UPSTREAM_PROXY_SETTINGS_FILE_NAME);
        let auth_presets = match load_saved_auth_presets(&preset_file) {
            Ok(presets) => presets,
            Err(error) => {
                warn!("load auth presets failed, fallback to empty list: {error}");
                Vec::new()
            }
        };
        let api_presets = match load_saved_api_presets(&api_preset_file) {
            Ok(presets) => presets,
            Err(error) => {
                warn!("load api presets failed, fallback to empty list: {error}");
                Vec::new()
            }
        };
        let claude_presets = match load_saved_claude_presets(&claude_preset_file) {
            Ok(presets) => presets,
            Err(error) => {
                warn!("load claude presets failed, fallback to empty list: {error}");
                Vec::new()
            }
        };
        let upstream_proxy_settings =
            match load_upstream_proxy_settings(&upstream_proxy_settings_file) {
                Ok(settings) => settings,
                Err(error) => {
                    warn!("load upstream proxy settings failed, fallback to disabled: {error}");
                    UpstreamProxySettings::default()
                }
            };

        persist_auth_presets_file(&preset_file, &auth_presets)?;
        persist_api_presets_file(&api_preset_file, &api_presets)?;
        persist_claude_presets_file(&claude_preset_file, &claude_presets)?;
        persist_upstream_proxy_settings_file(
            &upstream_proxy_settings_file,
            &upstream_proxy_settings,
        )?;

        Ok(Self {
            auth_presets: Arc::new(RwLock::new(auth_presets)),
            api_presets: Arc::new(RwLock::new(api_presets)),
            claude_presets: Arc::new(RwLock::new(claude_presets)),
            upstream_proxy_settings: Arc::new(RwLock::new(upstream_proxy_settings)),
            active_config_write_lock: Arc::new(tokio::sync::Mutex::new(())),
            preset_file: Arc::new(preset_file),
            api_preset_file: Arc::new(api_preset_file),
            claude_preset_file: Arc::new(claude_preset_file),
            upstream_proxy_settings_file: Arc::new(upstream_proxy_settings_file),
        })
    }

    pub fn preset_file(&self) -> PathBuf {
        self.preset_file.as_ref().clone()
    }

    pub fn api_preset_file(&self) -> PathBuf {
        self.api_preset_file.as_ref().clone()
    }

    pub fn claude_preset_file(&self) -> PathBuf {
        self.claude_preset_file.as_ref().clone()
    }

    pub fn upstream_proxy_settings_file(&self) -> PathBuf {
        self.upstream_proxy_settings_file.as_ref().clone()
    }

    pub async fn lock_active_config_write(&self) -> tokio::sync::OwnedMutexGuard<()> {
        self.active_config_write_lock.clone().lock_owned().await
    }

    pub fn try_lock_active_config_write(&self) -> Option<tokio::sync::OwnedMutexGuard<()>> {
        self.active_config_write_lock.clone().try_lock_owned().ok()
    }

    pub fn auth_presets_snapshot(&self) -> Vec<StoredAuthPreset> {
        self.auth_presets
            .read()
            .expect("auth preset manager poisoned")
            .clone()
    }

    pub fn replace_auth_presets(&self, presets: Vec<StoredAuthPreset>) {
        *self
            .auth_presets
            .write()
            .expect("auth preset manager poisoned") = presets;
    }

    pub fn api_presets_snapshot(&self) -> Vec<StoredApiPreset> {
        self.api_presets
            .read()
            .expect("api preset manager poisoned")
            .clone()
    }

    pub fn replace_api_presets(&self, presets: Vec<StoredApiPreset>) {
        *self
            .api_presets
            .write()
            .expect("api preset manager poisoned") = presets;
    }

    pub fn claude_presets_snapshot(&self) -> Vec<StoredClaudePreset> {
        self.claude_presets
            .read()
            .expect("claude preset manager poisoned")
            .clone()
    }

    pub fn replace_claude_presets(&self, presets: Vec<StoredClaudePreset>) {
        *self
            .claude_presets
            .write()
            .expect("claude preset manager poisoned") = presets;
    }

    pub fn upstream_proxy_settings(&self) -> UpstreamProxySettings {
        self.upstream_proxy_settings
            .read()
            .expect("upstream proxy settings manager poisoned")
            .clone()
    }

    pub fn replace_upstream_proxy_settings(&self, settings: UpstreamProxySettings) {
        *self
            .upstream_proxy_settings
            .write()
            .expect("upstream proxy settings manager poisoned") = settings;
    }
}

pub async fn persist_auth_presets_async(
    manager: &AuthPresetManager,
    presets: &[StoredAuthPreset],
) -> Result<()> {
    let encoded = serde_json::to_vec_pretty(presets).context("序列化 auth 预设失败")?;
    write_bytes_file(&manager.preset_file(), encoded, "auth 预设").await?;
    manager.replace_auth_presets(presets.to_vec());
    Ok(())
}

pub async fn persist_api_presets_async(
    manager: &AuthPresetManager,
    presets: &[StoredApiPreset],
) -> Result<()> {
    let encoded = serde_json::to_vec_pretty(presets).context("序列化 API 预设失败")?;
    write_bytes_file(&manager.api_preset_file(), encoded, "API 预设").await?;
    manager.replace_api_presets(presets.to_vec());
    Ok(())
}

pub async fn persist_claude_presets_async(
    manager: &AuthPresetManager,
    presets: &[StoredClaudePreset],
) -> Result<()> {
    let encoded = serde_json::to_vec_pretty(presets).context("序列化 Claude 预设失败")?;
    write_bytes_file(&manager.claude_preset_file(), encoded, "Claude 预设").await?;
    manager.replace_claude_presets(presets.to_vec());
    Ok(())
}

pub fn persist_upstream_proxy_settings(
    manager: &AuthPresetManager,
    settings: UpstreamProxySettings,
) -> Result<()> {
    persist_upstream_proxy_settings_file(&manager.upstream_proxy_settings_file(), &settings)?;
    manager.replace_upstream_proxy_settings(settings);
    Ok(())
}

pub async fn write_login_auth_file(path: &Path, auth: &AuthFile) -> Result<()> {
    validate_auth_file(auth)?;
    write_json_file(path, auth, "auth.json").await
}

pub async fn write_api_auth_file(path: &Path, auth: &ApiAuthFile) -> Result<()> {
    validate_api_auth_file(auth)?;
    write_json_file(path, auth, "auth.json").await
}

pub async fn write_claude_settings_file(path: &Path, preset: &StoredClaudePreset) -> Result<()> {
    validate_claude_code_endpoint_compatibility(&preset.base_url)?;

    let content = match fs::read_to_string(path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            anyhow::bail!("读取 ~/.claude/settings.json 失败，无法切换 Claude 预设: {error}");
        }
    };

    let settings =
        parse_claude_settings_document(&content).context("更新 ~/.claude/settings.json 失败")?;
    let next = set_claude_settings_in_value(settings, preset)
        .context("更新 ~/.claude/settings.json 失败")?;
    write_json_file(path, &next, "~/.claude/settings.json").await?;

    // Also write onboarding bypass file to skip Anthropic login check.
    // Preserve Claude Code's own session/trust state stored in the same file.
    if let Some(parent) = path.parent() {
        let bypass_path = parent
            .parent()
            .unwrap_or(parent)
            .join(CLAUDE_ONBOARDING_BYPASS_FILE);
        write_claude_onboarding_bypass_file(&bypass_path).await?;
    }

    Ok(())
}

async fn write_claude_onboarding_bypass_file(path: &Path) -> Result<()> {
    let content = match fs::read_to_string(path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            anyhow::bail!("读取 ~/.claude.json 失败，无法切换 Claude 预设: {error}");
        }
    };
    let next =
        set_claude_onboarding_bypass_in_content(&content).context("更新 ~/.claude.json 失败")?;
    write_json_file(path, &next, "~/.claude.json").await
}

fn set_claude_onboarding_bypass_in_content(content: &str) -> Result<Value> {
    let mut root = if content.trim().is_empty() {
        Map::new()
    } else {
        let value: Value = serde_json::from_str(content).context("cannot parse .claude.json")?;
        value
            .as_object()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!(".claude.json 顶层必须是对象。"))?
    };
    root.insert("hasCompletedOnboarding".to_string(), Value::Bool(true));
    Ok(Value::Object(root))
}

pub async fn write_opencode_config_file(path: &Path, preset: &StoredClaudePreset) -> Result<()> {
    let content = match fs::read_to_string(path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => "{}".to_string(),
        Err(error) => {
            anyhow::bail!("读取 opencode.json 失败: {error}");
        }
    };

    let mut root: Value = serde_json::from_str(&content).context("opencode.json 格式无效")?;
    let root_map = root
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("opencode.json 顶层必须是对象。"))?;

    // Ensure provider object exists
    let provider = root_map
        .entry("provider")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("opencode.json 中 provider 必须是对象。"))?;

    // Build model entry: prefer third_party_model, then default models
    let model_name = preset
        .third_party_model
        .as_deref()
        .or(preset.default_opus_model.as_deref())
        .or(preset.default_sonnet_model.as_deref())
        .or(preset.default_haiku_model.as_deref())
        .unwrap_or("claude-sonnet-4-6");

    let provider_key = if !preset.provider_name.is_empty() {
        preset.provider_name.clone()
    } else {
        "webclx".to_string()
    };

    let mut options = Map::new();
    options.insert("baseURL".to_string(), Value::String(preset.base_url.clone()));
    options.insert("apiKey".to_string(), Value::String(preset.auth_token.clone()));
    let mut models = Map::new();
    models.insert(
        model_name.to_string(),
        Value::Object({
            let mut m = Map::new();
            m.insert("name".to_string(), Value::String(model_name.to_string()));
            m
        }),
    );
    options.insert("models".to_string(), Value::Object(models));

    let mut provider_entry = Map::new();
    provider_entry.insert("npm".to_string(), Value::String("ai-sdk/openai-compatible".to_string()));
    provider_entry.insert("name".to_string(), Value::String(provider_key.clone()));
    provider_entry.insert("options".to_string(), Value::Object(options));

    provider.insert(provider_key, Value::Object(provider_entry));

    write_json_file(path, &root, "opencode.json").await
}

async fn write_json_file<T: Serialize>(path: &Path, value: &T, label: &str) -> Result<()> {
    let encoded =
        serde_json::to_vec_pretty(value).with_context(|| format!("序列化 {label} 失败"))?;
    write_bytes_file(path, encoded, label).await
}

async fn write_bytes_file(path: &Path, content: Vec<u8>, label: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("创建 {label} 目录失败"))?;
    }

    fs::write(path, content)
        .await
        .with_context(|| format!("写入 {label} 失败"))?;
    set_file_mode(path, OWNER_ONLY_FILE_MODE)
        .await
        .with_context(|| format!("更新 {label} 权限失败"))?;
    Ok(())
}

async fn write_text_file(path: &Path, content: String, label: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .with_context(|| format!("创建 {label} 目录失败"))?;
    }

    fs::write(path, content)
        .await
        .with_context(|| format!("写入 {label} 失败"))?;
    set_file_mode(path, OWNER_ONLY_FILE_MODE)
        .await
        .with_context(|| format!("更新 {label} 权限失败"))?;
    Ok(())
}

pub async fn clear_config_provider(path: &Path) -> Result<()> {
    let content = match fs::read_to_string(path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            anyhow::bail!("读取 config.toml 失败，无法清理 provider: {error}");
        }
    };

    let next = clear_provider_in_config_content(&content).context("更新 config.toml 失败")?;
    write_text_file(path, next, "config.toml").await
}

pub async fn sync_auth_preset_config(path: &Path, targets: &[ResolvedConfigTarget]) -> Result<()> {
    let content = match fs::read_to_string(path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            anyhow::bail!("读取 config.toml 失败，无法切换 auth 预设: {error}");
        }
    };

    let mut next = content;
    for target in targets {
        next = clear_provider_and_set_config_entry_in_config_content(
            &next,
            &target.key,
            &target.value,
        )
        .context("更新 config.toml 失败")?;
    }
    write_text_file(path, next, "config.toml").await
}

pub async fn sync_api_preset_config(
    path: &Path,
    provider_name: &str,
    base_url: &str,
    provider_options: &ApiProviderOptions,
    targets: &[ResolvedConfigTarget],
    managed_keys: &[String],
) -> Result<()> {
    let content = match fs::read_to_string(path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            anyhow::bail!("读取 config.toml 失败，无法切换 API 预设: {error}");
        }
    };

    let active_keys = targets
        .iter()
        .map(|target| target.key.clone())
        .collect::<Vec<_>>();
    let mut next =
        clear_inactive_managed_config_entries_in_content(&content, managed_keys, &active_keys)
            .context("清理旧 API 预设 config 失败")?;
    for target in targets {
        next = set_api_provider_and_config_entry_in_config_content(
            &next,
            provider_name,
            base_url,
            provider_options,
            &target.key,
            &target.value,
        )
        .context("更新 config.toml 失败")?;
    }
    write_text_file(path, next, "config.toml").await
}

pub async fn sync_api_model_catalog(
    config_path: &Path,
    targets: &[ResolvedConfigTarget],
    bundled_catalog: Option<&Value>,
) -> Result<()> {
    let Some(model) = model_target_value(targets) else {
        return Ok(());
    };

    let content = match fs::read_to_string(config_path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            anyhow::bail!("读取 config.toml 失败，无法同步 Codex 模型 metadata: {error}");
        }
    };
    let configured_catalog_path = model_catalog_path_from_config_content(&content, config_path)?;
    if configured_catalog_path.is_none() && bundled_catalog.is_none() {
        anyhow::bail!(
            "config.toml 未配置 model_catalog_json，初始化模型目录需要 Codex bundled catalog"
        );
    }
    let catalog_path = configured_catalog_path.clone().unwrap_or_else(|| {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(DEFAULT_MODEL_CATALOG_FILE_NAME)
    });
    let existing_catalog = read_model_catalog_file(&catalog_path).await?;
    let mut catalog = merge_model_catalog_values(bundled_catalog, existing_catalog.as_ref())?;

    let context_window = model_context_window_target_value(targets);
    upsert_model_catalog_entry_in_value(&mut catalog, &model, context_window)?;
    backfill_model_catalog_reasoning_summary_capability(&mut catalog)?;
    write_json_file(&catalog_path, &catalog, DEFAULT_MODEL_CATALOG_FILE_NAME)
        .await
        .with_context(|| format!("更新 {} 失败", catalog_path.display()))?;

    if configured_catalog_path.is_none() {
        let next = set_default_model_catalog_path_in_config_content(&content)?;
        write_text_file(config_path, next, "config.toml").await?;
    }

    Ok(())
}

fn model_target_value(targets: &[ResolvedConfigTarget]) -> Option<String> {
    targets
        .iter()
        .rev()
        .find(|target| target.key.eq_ignore_ascii_case("model"))
        .map(|target| target.value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn model_context_window_target_value(targets: &[ResolvedConfigTarget]) -> Option<i64> {
    targets
        .iter()
        .rev()
        .find(|target| target.key.eq_ignore_ascii_case("model_context_window"))
        .and_then(|target| target.value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
}

fn model_catalog_path_from_config_content(
    content: &str,
    config_path: &Path,
) -> Result<Option<PathBuf>> {
    if content.trim().is_empty() {
        return Ok(None);
    }
    let doc = content
        .parse::<DocumentMut>()
        .context("cannot parse config.toml")?;
    let Some(raw_path) = doc.get("model_catalog_json").and_then(|item| item.as_str()) else {
        return Ok(None);
    };
    let raw_path = raw_path.trim();
    if raw_path.is_empty() {
        return Ok(None);
    }
    Ok(Some(resolve_config_relative_path(raw_path, config_path)))
}

fn resolve_config_relative_path(raw_path: &str, config_path: &Path) -> PathBuf {
    if let Some(rest) = raw_path.strip_prefix("~/") {
        return config_home_dir(config_path).join(rest);
    }
    let path = PathBuf::from(raw_path);
    if path.is_absolute() {
        return path;
    }
    config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(path)
}

fn config_home_dir(config_path: &Path) -> PathBuf {
    config_path
        .parent()
        .and_then(|parent| {
            if parent.file_name().is_some_and(|name| name == ".codex") {
                parent.parent()
            } else {
                None
            }
        })
        .map(Path::to_path_buf)
        .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("~"))
}

async fn read_model_catalog_file(catalog_path: &Path) -> Result<Option<Value>> {
    match fs::read_to_string(catalog_path).await {
        Ok(content) => serde_json::from_str::<Value>(&content)
            .map(Some)
            .with_context(|| format!("cannot parse {}", catalog_path.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            anyhow::bail!("读取 {} 失败: {error}", catalog_path.display());
        }
    }
}

fn merge_model_catalog_values(
    bundled_catalog: Option<&Value>,
    existing_catalog: Option<&Value>,
) -> Result<Value> {
    let mut merged = bundled_catalog
        .or(existing_catalog)
        .cloned()
        .unwrap_or_else(empty_model_catalog);
    let merged_models = model_catalog_models_mut(&mut merged)?;
    deduplicate_model_catalog_entries(merged_models);

    if bundled_catalog.is_some()
        && let Some(existing_catalog) = existing_catalog
    {
        let existing_models = model_catalog_models(existing_catalog)?;
        for entry in existing_models {
            let Some(slug) = model_catalog_slug(entry) else {
                continue;
            };
            if !merged_models.iter().any(|merged_entry| {
                model_catalog_slug(merged_entry)
                    .is_some_and(|merged_slug| merged_slug.eq_ignore_ascii_case(slug))
            }) {
                merged_models.push(entry.clone());
            }
        }
    }

    Ok(merged)
}

fn empty_model_catalog() -> Value {
    let mut root = Map::new();
    root.insert("models".to_string(), Value::Array(Vec::new()));
    Value::Object(root)
}

fn model_catalog_models(catalog: &Value) -> Result<&Vec<Value>> {
    catalog
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("model_catalog.json 顶层需要包含 models 数组"))
}

fn model_catalog_models_mut(catalog: &mut Value) -> Result<&mut Vec<Value>> {
    catalog
        .get_mut("models")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| anyhow::anyhow!("model_catalog.json 顶层需要包含 models 数组"))
}

fn backfill_model_catalog_reasoning_summary_capability(catalog: &mut Value) -> Result<()> {
    for entry in model_catalog_models_mut(catalog)? {
        let Some(object) = entry.as_object_mut() else {
            continue;
        };
        if object.contains_key("supports_reasoning_summaries") {
            continue;
        }
        let supports_reasoning_summaries = object
            .get("supported_reasoning_levels")
            .and_then(Value::as_array)
            .is_some_and(|levels| !levels.is_empty());
        object.insert(
            "supports_reasoning_summaries".to_string(),
            Value::Bool(supports_reasoning_summaries),
        );
    }
    Ok(())
}

fn deduplicate_model_catalog_entries(models: &mut Vec<Value>) -> bool {
    let mut seen = HashSet::new();
    let original_len = models.len();
    models.retain(|entry| {
        let Some(slug) = model_catalog_slug(entry) else {
            return true;
        };
        seen.insert(slug.to_ascii_lowercase())
    });
    models.len() != original_len
}

fn set_default_model_catalog_path_in_config_content(content: &str) -> Result<String> {
    let mut doc = if content.trim().is_empty() {
        DocumentMut::new()
    } else {
        content
            .parse::<DocumentMut>()
            .context("cannot parse config.toml")?
    };
    doc["model_catalog_json"] = value(DEFAULT_MODEL_CATALOG_FILE_NAME);
    let mut next = doc.to_string();
    if !next.ends_with('\n') {
        next.push('\n');
    }
    Ok(next)
}

pub(crate) fn upsert_model_catalog_entry_in_value(
    catalog: &mut Value,
    model: &str,
    context_window: Option<i64>,
) -> Result<bool> {
    let models = model_catalog_models_mut(catalog)?;
    let mut changed = deduplicate_model_catalog_entries(models);
    if let Some(existing) = models.iter_mut().find(|entry| {
        model_catalog_slug(entry).is_some_and(|slug| slug.eq_ignore_ascii_case(model))
    }) {
        let object = existing
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("model_catalog.json 模型条目必须是对象"))?;
        let previous_slug = object
            .get("slug")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if previous_slug != model {
            object.insert("slug".to_string(), Value::String(model.to_string()));
            if object.get("display_name").and_then(Value::as_str) == Some(previous_slug.as_str()) {
                object.insert("display_name".to_string(), Value::String(model.to_string()));
            }
            changed = true;
        }
        changed |= clear_inherited_custom_model_upgrade(object);
        let Some(context_window) = context_window else {
            return Ok(changed);
        };
        object.insert("context_window".to_string(), Value::Number(context_window.into()));
        object.insert("max_context_window".to_string(), Value::Number(context_window.into()));
        let compact_limit = (context_window as f64 * 0.8) as i64;
        object.insert("auto_compact_token_limit".to_string(), Value::Number(compact_limit.into()));
        return Ok(true);
    }

    let entry = custom_model_catalog_entry(models, model, context_window);
    models.push(entry);
    Ok(true)
}

fn model_catalog_slug(entry: &Value) -> Option<&str> {
    entry
        .as_object()
        .and_then(|object| object.get("slug"))
        .and_then(Value::as_str)
}

fn custom_model_catalog_entry(models: &[Value], model: &str, context_window: Option<i64>) -> Value {
    let mut entry = models
        .iter()
        .find(|entry| model_catalog_slug(entry) == Some("gpt-5.4"))
        .or_else(|| {
            models
                .iter()
                .find(|entry| model_catalog_slug(entry) == Some("glm-5.1"))
        })
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));

    let context_window = context_window.unwrap_or_else(|| custom_model_context_window(model));
    let object = entry.as_object_mut().expect("entry should be object");
    object.insert("slug".to_string(), Value::String(model.to_string()));
    object.insert("display_name".to_string(), Value::String(model.to_string()));
    object.insert(
        "description".to_string(),
        Value::String(CUSTOM_API_MODEL_DESCRIPTION.to_string()),
    );
    object.insert("upgrade".to_string(), Value::Null);
    object.insert("context_window".to_string(), Value::Number(context_window.into()));
    object.insert("max_context_window".to_string(), Value::Number(context_window.into()));
    let compact_limit = (context_window as f64 * 0.8) as i64;
    object.insert("auto_compact_token_limit".to_string(), Value::Number(compact_limit.into()));
    object.insert("supported_in_api".to_string(), Value::Bool(true));
    object.insert("visibility".to_string(), Value::String("list".to_string()));
    entry
}

fn clear_inherited_custom_model_upgrade(object: &mut Map<String, Value>) -> bool {
    if object.get("description").and_then(Value::as_str) != Some(CUSTOM_API_MODEL_DESCRIPTION)
        || object.get("upgrade") == Some(&Value::Null)
    {
        return false;
    }
    object.insert("upgrade".to_string(), Value::Null);
    true
}

fn custom_model_context_window(model: &str) -> i64 {
    let lower = model.to_ascii_lowercase();
    if lower.contains("deepseek-v4") {
        1_000_000
    } else if lower.contains("deepseek") {
        64_000
    } else if lower.contains("minimax") {
        128_000
    } else if lower.starts_with("gpt-5") || lower.starts_with("glm-5") {
        256_000
    } else {
        128_000
    }
}

pub(crate) fn load_saved_auth_presets(path: &Path) -> Result<Vec<StoredAuthPreset>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let value: Value = serde_json::from_str(&content).context("cannot parse auth preset file")?;
    let presets = parse_auth_presets_value(value).context("cannot parse auth preset file")?;

    Ok(presets)
}

fn parse_auth_presets_value(value: Value) -> Result<Vec<StoredAuthPreset>> {
    let items = value
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("auth preset file must be a JSON array"))?;
    let mut presets = Vec::with_capacity(items.len());
    let loaded_at = current_timestamp_secs();

    for (index, item) in items.iter().enumerate() {
        let mut preset = if is_stored_auth_preset_shape(item) {
            serde_json::from_value::<StoredAuthPreset>(item.clone())
                .context("cannot parse stored auth preset")?
        } else {
            stored_auth_preset_from_cpa_export(item, index, loaded_at, &presets)?
        };

        validate_auth_file_sync(&preset.auth)
            .with_context(|| format!("invalid auth preset `{}`", preset.name))?;
        normalize_auth_preset(&mut preset)
            .with_context(|| format!("invalid auth preset `{}`", preset.name))?;
        presets.push(preset);
    }

    Ok(presets)
}

fn is_stored_auth_preset_shape(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return false;
    };
    object.contains_key("auth")
        || (object.contains_key("id") && object.contains_key("name"))
        || object.contains_key("saved_at")
}

fn stored_auth_preset_from_cpa_export(
    value: &Value,
    index: usize,
    loaded_at: u64,
    existing_presets: &[StoredAuthPreset],
) -> Result<StoredAuthPreset> {
    let auth: AuthFile =
        serde_json::from_value(value.clone()).context("cannot parse CPA auth export")?;
    let details = auth_preset_details_from_cpa_export(value);
    let raw_name =
        first_string_field(value, &["name", "email", "account_name"]).unwrap_or_default();
    let name = resolve_auth_preset_name(&raw_name, &auth, existing_presets, None);
    let saved_at = json_u64_field(value, "saved_at")
        .or_else(|| json_u64_field(value, "created_at"))
        .unwrap_or(loaded_at.saturating_add(index as u64));

    Ok(StoredAuthPreset {
        id: format!("auth-cpa-{loaded_at}-{index}"),
        name,
        saved_at,
        details,
        config_overrides: Vec::new(),
        legacy_config_key: None,
        legacy_config_value: None,
        legacy_secondary_config_key: None,
        legacy_secondary_config_value: None,
        auth,
        switch_count: 0,
    })
}

fn auth_preset_details_from_cpa_export(value: &Value) -> AuthPresetDetails {
    AuthPresetDetails {
        email: json_string_field(value, "email"),
        plan_type: json_string_field(value, "plan_type").map(|value| value.to_uppercase()),
        account_name: json_string_field(value, "account_name"),
        login_method: first_string_field(value, &["auth_provider", "auth_mode"]),
        hourly_percentage: json_u64_field(value, "hourly_percentage"),
        hourly_reset_time: json_u64_field(value, "hourly_reset_time"),
        weekly_percentage: json_u64_field(value, "weekly_percentage"),
        weekly_reset_time: json_u64_field(value, "weekly_reset_time"),
    }
}

fn first_string_field(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| json_string_field(value, key))
}

fn json_string_field(value: &Value, key: &str) -> Option<String> {
    value
        .as_object()
        .and_then(|object| object.get(key))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn json_u64_field(value: &Value, key: &str) -> Option<u64> {
    let value = value.as_object().and_then(|object| object.get(key))?;
    if let Some(number) = value.as_u64() {
        return Some(number);
    }
    value
        .as_str()
        .and_then(|value| value.trim().parse::<u64>().ok())
}

pub(crate) fn load_saved_api_presets(path: &Path) -> Result<Vec<StoredApiPreset>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut presets: Vec<StoredApiPreset> =
        serde_json::from_str(&content).context("cannot parse api preset file")?;

    for preset in &mut presets {
        normalize_api_preset(preset)
            .with_context(|| format!("invalid api preset `{}`", preset.name))?;
    }

    Ok(presets)
}

pub(crate) fn load_upstream_proxy_settings(path: &Path) -> Result<UpstreamProxySettings> {
    if !path.exists() {
        return Ok(UpstreamProxySettings::default());
    }

    let content =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let settings: UpstreamProxySettings =
        serde_json::from_str(&content).context("cannot parse upstream proxy settings file")?;
    Ok(settings)
}

pub(crate) fn load_saved_claude_presets(path: &Path) -> Result<Vec<StoredClaudePreset>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let content =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut presets: Vec<StoredClaudePreset> =
        serde_json::from_str(&content).context("cannot parse claude preset file")?;

    for preset in &mut presets {
        normalize_claude_preset(preset)
            .with_context(|| format!("invalid claude preset `{}`", preset.name))?;
    }

    Ok(presets)
}

pub(crate) fn persist_auth_presets_file(path: &Path, presets: &[StoredAuthPreset]) -> Result<()> {
    let content = serde_json::to_vec_pretty(presets).context("cannot encode auth preset file")?;
    std::fs::write(path, content).with_context(|| format!("cannot write {}", path.display()))?;
    set_file_mode_sync(path, OWNER_ONLY_FILE_MODE)
        .context("cannot update auth preset permissions")?;
    Ok(())
}

pub(crate) fn persist_api_presets_file(path: &Path, presets: &[StoredApiPreset]) -> Result<()> {
    let content = serde_json::to_vec_pretty(presets).context("cannot encode api preset file")?;
    std::fs::write(path, content).with_context(|| format!("cannot write {}", path.display()))?;
    set_file_mode_sync(path, OWNER_ONLY_FILE_MODE)
        .context("cannot update api preset permissions")?;
    Ok(())
}

pub(crate) fn persist_claude_presets_file(
    path: &Path,
    presets: &[StoredClaudePreset],
) -> Result<()> {
    let content = serde_json::to_vec_pretty(presets).context("cannot encode claude preset file")?;
    std::fs::write(path, content).with_context(|| format!("cannot write {}", path.display()))?;
    set_file_mode_sync(path, OWNER_ONLY_FILE_MODE)
        .context("cannot update claude preset permissions")?;
    Ok(())
}

pub(crate) fn persist_upstream_proxy_settings_file(
    path: &Path,
    settings: &UpstreamProxySettings,
) -> Result<()> {
    let content = serde_json::to_vec_pretty(settings)
        .context("cannot encode upstream proxy settings file")?;
    std::fs::write(path, content).with_context(|| format!("cannot write {}", path.display()))?;
    set_file_mode_sync(path, OWNER_ONLY_FILE_MODE)
        .context("cannot update upstream proxy settings permissions")?;
    Ok(())
}

pub(crate) fn validate_auth_file(auth: &AuthFile) -> Result<()> {
    validate_auth_file_sync(auth)
}

pub fn validate_auth_file_sync(auth: &AuthFile) -> Result<()> {
    if auth.last_refresh.trim().is_empty() {
        anyhow::bail!("last_refresh 不能为空。");
    }
    if auth.tokens.access_token.trim().is_empty() {
        anyhow::bail!("access_token 不能为空。");
    }
    if auth.tokens.account_id.trim().is_empty() {
        anyhow::bail!("account_id 不能为空。");
    }
    Ok(())
}

fn validate_api_auth_file(auth: &ApiAuthFile) -> Result<()> {
    validate_api_auth_file_sync(auth)
}

pub fn validate_api_auth_file_sync(auth: &ApiAuthFile) -> Result<()> {
    validate_api_key_sync(&auth.openai_api_key)
}

pub fn validate_api_key_sync(api_key: &str) -> Result<()> {
    if api_key.trim().is_empty() {
        anyhow::bail!("OPENAI_API_KEY 不能为空。");
    }
    Ok(())
}

pub fn normalize_api_preset(preset: &mut StoredApiPreset) -> Result<()> {
    validate_api_key_sync(&preset.api_key)?;
    let base_url = sanitize_base_url(preset.base_url.clone())?;
    let provider_name = sanitize_api_provider_name(preset.provider_name.clone(), &base_url);
    let management_url = sanitize_management_url(preset.management_url.clone())?;
    let mut config_overrides = effective_preset_config_overrides(
        std::mem::take(&mut preset.config_overrides),
        preset.legacy_config_key.take(),
        preset.legacy_config_value.take(),
        preset.legacy_secondary_config_key.take(),
        preset.legacy_secondary_config_value.take(),
    )?;
    let mut wire_api = preset.wire_api.take();
    config_overrides.retain(|item| {
        let is_wire_api = item
            .key
            .as_deref()
            .is_some_and(|key| key.eq_ignore_ascii_case("wire_api"));
        if is_wire_api {
            if wire_api.is_none() {
                wire_api = item.value.clone();
            }
            return false;
        }
        true
    });
    let wire_api = sanitize_api_wire_api(wire_api)?;
    preset.base_url = base_url;
    preset.provider_name = provider_name;
    preset.management_url = management_url;
    preset.wire_api = wire_api;
    preset.config_overrides = config_overrides;
    preset.terminal_env = sanitize_terminal_env_vars(std::mem::take(&mut preset.terminal_env));
    preset.terminal_startup_script =
        sanitize_terminal_startup_script(preset.terminal_startup_script.take());
    if preset.responses_proxy.is_none() {
        preset.responses_proxy = infer_api_responses_proxy(preset);
    }
    backfill_auto_compact_token_limit(&mut preset.config_overrides);
    Ok(())
}

pub fn normalize_auth_preset(preset: &mut StoredAuthPreset) -> Result<()> {
    preset.config_overrides = effective_preset_config_overrides(
        std::mem::take(&mut preset.config_overrides),
        preset.legacy_config_key.take(),
        preset.legacy_config_value.take(),
        preset.legacy_secondary_config_key.take(),
        preset.legacy_secondary_config_value.take(),
    )?;
    backfill_auto_compact_token_limit(&mut preset.config_overrides);
    Ok(())
}

/// 当预设 config_overrides 中存在 model_context_window 但缺少 model_auto_compact_token_limit 时，
/// 自动补全压缩阈值为上下文窗口的 80%，确保旧预设迁移后也拥有该属性。
fn backfill_auto_compact_token_limit(overrides: &mut Vec<PresetConfigOverride>) {
    let has_auto_compact = overrides.iter().any(|item| {
        item.key
            .as_deref()
            .is_some_and(|k| k.eq_ignore_ascii_case("model_auto_compact_token_limit"))
    });
    if has_auto_compact {
        return;
    }
    let Some(context_window) = overrides
        .iter()
        .rev()
        .find(|item| {
            item.key
                .as_deref()
                .is_some_and(|k| k.eq_ignore_ascii_case("model_context_window"))
        })
        .and_then(|item| item.value.as_deref())
        .and_then(|value| value.trim().parse::<i64>().ok())
        .filter(|value| *value > 0)
    else {
        return;
    };
    let compact_limit = (context_window as f64 * 0.8) as i64;
    if compact_limit <= 0 {
        return;
    }
    overrides.push(PresetConfigOverride {
        key: Some("model_auto_compact_token_limit".to_string()),
        value: Some(compact_limit.to_string()),
    });
}

pub fn normalize_claude_preset(preset: &mut StoredClaudePreset) -> Result<()> {
    let auth_token = sanitize_auth_token(preset.auth_token.clone())?;
    let base_url = sanitize_base_url(preset.base_url.clone())?;
    let provider_name = sanitize_api_provider_name(preset.provider_name.clone(), &base_url);
    let management_url = sanitize_management_url(preset.management_url.clone())?;
    let config_overrides = effective_claude_config_overrides(
        std::mem::take(&mut preset.config_overrides),
        preset.legacy_config_key.take(),
        preset.legacy_config_value.take(),
        preset.legacy_secondary_config_key.take(),
        preset.legacy_secondary_config_value.take(),
    )?;
    let default_haiku_model = sanitize_claude_model(preset.default_haiku_model.clone());
    let default_sonnet_model = sanitize_claude_model(preset.default_sonnet_model.clone());
    let default_opus_model = sanitize_claude_model(preset.default_opus_model.clone());
    let third_party_model = sanitize_claude_model(preset.third_party_model.clone());
    validate_claude_model_selection(
        default_haiku_model.as_deref(),
        default_sonnet_model.as_deref(),
        default_opus_model.as_deref(),
        third_party_model.as_deref(),
    )?;
    preset.auth_token = auth_token;
    preset.base_url = base_url;
    preset.provider_name = provider_name;
    preset.management_url = management_url;
    preset.config_overrides = config_overrides;
    preset.default_haiku_model = default_haiku_model;
    preset.default_sonnet_model = default_sonnet_model;
    preset.default_opus_model = default_opus_model;
    preset.third_party_model = third_party_model;
    preset.use_local_proxy = crate::effective_claude_use_local_proxy(preset);
    Ok(())
}

pub fn resolve_auth_preset_name(
    raw_name: &str,
    auth: &AuthFile,
    presets: &[StoredAuthPreset],
    current_id: Option<&str>,
) -> String {
    let base = if raw_name.trim().is_empty() {
        let short = auth
            .tokens
            .account_id
            .chars()
            .rev()
            .take(6)
            .collect::<String>();
        let short = short.chars().rev().collect::<String>();
        format!("账号 {short}")
    } else {
        raw_name.trim().to_string()
    };

    unique_name(
        base,
        presets
            .iter()
            .filter(|preset| Some(preset.id.as_str()) != current_id)
            .map(|preset| preset.name.as_str()),
    )
}

pub fn resolve_api_preset_name(
    raw_name: &str,
    base_url: &str,
    presets: &[StoredApiPreset],
    current_id: Option<&str>,
) -> String {
    let base = if raw_name.trim().is_empty() {
        format!("API {}", suggest_api_label(base_url))
    } else {
        sanitize_url_like_preset_name(raw_name)
    };

    unique_name(
        base,
        presets
            .iter()
            .filter(|preset| Some(preset.id.as_str()) != current_id)
            .map(|preset| preset.name.as_str()),
    )
}

pub(crate) fn suggest_api_label(base_url: &str) -> String {
    sanitize_url_like_preset_name(base_url)
}

fn sanitize_url_like_preset_name(value: &str) -> String {
    let trimmed = value.trim();
    let without_scheme = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed);
    without_scheme.trim_matches('/').replace('/', "")
}

fn unique_name<'a>(base: String, names: impl Iterator<Item = &'a str>) -> String {
    let existing: Vec<&str> = names.collect();
    if !existing.iter().any(|name| *name == base) {
        return base;
    }

    let mut index = 2;
    loop {
        let candidate = format!("{base} {index}");
        if !existing.iter().any(|name| *name == candidate) {
            return candidate;
        }
        index += 1;
    }
}

pub fn resolve_claude_preset_name(
    raw_name: &str,
    base_url: &str,
    presets: &[StoredClaudePreset],
    current_id: Option<&str>,
) -> String {
    let base = if raw_name.trim().is_empty() {
        format!("Claude {}", suggest_api_label(base_url))
    } else {
        sanitize_url_like_preset_name(raw_name)
    };

    unique_name(
        base,
        presets
            .iter()
            .filter(|preset| Some(preset.id.as_str()) != current_id)
            .map(|preset| preset.name.as_str()),
    )
}

pub fn generate_preset_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    format!("{prefix}-{millis}")
}

pub fn current_timestamp_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Increment the `switch_count` for the preset matching `preset_id` in place.
/// Returns `true` if a preset was found and incremented, `false` otherwise.
/// The caller is responsible for persisting the slice after this call.
pub fn bump_switch_count<T: SwitchCounted>(presets: &mut [T], preset_id: &str) -> bool {
    for preset in presets.iter_mut() {
        if preset.preset_id() == preset_id {
            *preset.switch_count_mut() = (*preset.switch_count_mut()).saturating_add(1);
            return true;
        }
    }
    false
}

#[cfg(unix)]
async fn set_file_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .await
        .with_context(|| format!("cannot chmod {}", path.display()))
}

#[cfg(not(unix))]
async fn set_file_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_file_mode_sync(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("cannot chmod {}", path.display()))
}

#[cfg(not(unix))]
fn set_file_mode_sync(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}
