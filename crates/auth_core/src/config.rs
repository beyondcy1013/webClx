use anyhow::{Context, Result};
use serde_json::{Map, Value};
use toml_edit::{DocumentMut, Item, table, value};

use crate::{
    API_PROVIDER_KEY, ApiProviderOptions, CLAUDE_AUTH_TOKEN_KEY, CLAUDE_BASE_URL_KEY,
    CLAUDE_DEFAULT_HAIKU_MODEL_KEY, CLAUDE_DEFAULT_OPUS_MODEL_KEY, CLAUDE_DEFAULT_SONNET_MODEL_KEY,
    CLAUDE_LEGACY_AUTH_TOKEN_KEY, CLAUDE_LEGACY_SMALL_FAST_MODEL_KEY, CLAUDE_MODEL_KEY,
    ConfigProviderState, PresetConfigOverride, StoredClaudePreset, WEBCLX_LOCAL_API_TOKEN_ENV,
    WEBCLX_LOCAL_API_TOKEN_HEADER, validate_claude_model_selection,
};

pub(crate) fn read_current_config_provider_from_content(
    content: &str,
) -> Result<Option<ConfigProviderState>> {
    let doc = parse_config_document(content)?;
    let provider_name = doc
        .get("model_provider")
        .and_then(|item| item.as_str())
        .or_else(|| doc.get("provider").and_then(|item| item.as_str()))
        .map(str::to_string);

    let Some(provider_name) = provider_name else {
        return Ok(None);
    };

    let provider_table = doc
        .get("model_providers")
        .and_then(|item| item.as_table_like())
        .and_then(|providers| providers.get(provider_name.as_str()))
        .and_then(|item| item.as_table_like());

    Ok(Some(ConfigProviderState {
        provider_id: provider_name.clone(),
        provider_name: provider_table
            .and_then(|table| table.get("name"))
            .and_then(|item| item.as_str())
            .map(str::to_string),
        base_url: provider_table
            .and_then(|table| table.get("base_url"))
            .and_then(|item| item.as_str())
            .map(str::to_string),
        wire_api: provider_table
            .and_then(|table| table.get("wire_api"))
            .and_then(|item| item.as_str())
            .map(str::to_string),
        config_values: collect_applied_config_values(&doc),
    }))
}

/// 读取 config.toml 中由预设 config_overrides 写入的取值，
/// 用于在多个预设共享同一组 base_url+api_key+wire_api 时区分当前生效预设。
///
/// 只收集根键和二级键的标量取值，跳过 provider 结构化配置
/// (`model_provider`/`provider`/`wire_api`/`model_providers`)。
/// 取值文本化规则与 [`parse_config_value_item`] 写入端保持一致：
/// 字符串去掉引号，布尔/整数/浮点用其 toml_edit 的 Display 文本。
fn collect_applied_config_values(doc: &DocumentMut) -> std::collections::BTreeMap<String, String> {
    use std::collections::BTreeMap;
    let mut values = BTreeMap::new();
    let reserved_root: &[&str] = &["model_provider", "provider", "wire_api", "model_providers"];

    for (root_key, root_item) in doc.iter() {
        if reserved_root.contains(&root_key) {
            continue;
        }
        if let Some(table) = root_item.as_table_like() {
            for (child_key, child_item) in table.iter() {
                if let Some(text) = toml_value_as_text(child_item) {
                    values.insert(format!("{}.{child_key}", root_key), text);
                }
            }
            continue;
        }
        if let Some(text) = toml_value_as_text(root_item) {
            values.insert(root_key.to_string(), text);
        }
    }
    values
}

/// 将 toml_edit 标量 Item 转成与 config_overrides 预期值可比对的文本。
fn toml_value_as_text(item: &Item) -> Option<String> {
    let value = item.as_value()?;
    let text = match value {
        toml_edit::Value::String(s) => s.value().to_string(),
        toml_edit::Value::Integer(i) => i.value().to_string(),
        toml_edit::Value::Float(f) => f.value().to_string(),
        toml_edit::Value::Boolean(b) => b.value().to_string(),
        _ => value.to_string(),
    };
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn parse_config_document(content: &str) -> Result<DocumentMut> {
    if content.trim().is_empty() {
        return Ok(DocumentMut::new());
    }

    content
        .parse::<DocumentMut>()
        .context("cannot parse config.toml")
}

pub fn clear_provider_in_config_content(content: &str) -> Result<String> {
    let mut doc = parse_config_document(content)?;
    doc.remove("model_provider");
    doc.remove("provider");
    Ok(ensure_trailing_newline(doc.to_string()))
}

pub(crate) fn normalize_expected_config_key(expected_key: &str) -> Result<String> {
    let trimmed = expected_key.trim();
    if trimmed.is_empty() {
        anyhow::bail!("config 键名不能为空。");
    }
    for segment in trimmed.split('.') {
        if segment.is_empty() {
            anyhow::bail!("config 键名的路径段不能为空。");
        }
        if !segment
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
        {
            anyhow::bail!("config 键名每段只能包含字母、数字、_ 或 -。");
        }
    }
    Ok(trimmed.to_string())
}

pub(crate) fn normalize_expected_config_value(expected_value: &str) -> Result<String> {
    let trimmed = expected_value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("config 键值不能为空。");
    }
    Ok(trimmed.to_string())
}

pub fn clear_provider_and_set_config_entry_in_config_content(
    content: &str,
    expected_key: &str,
    expected_value: &str,
) -> Result<String> {
    let expected_key = normalize_expected_config_key(expected_key)?;
    let expected_value = normalize_expected_config_value(expected_value)?;
    let mut doc = parse_config_document(content)?;
    doc.remove("model_provider");
    doc.remove("provider");
    set_config_entry(&mut doc, &expected_key, &expected_value)?;
    Ok(ensure_trailing_newline(doc.to_string()))
}

pub(crate) fn clear_inactive_managed_config_entries_in_content(
    content: &str,
    managed_keys: &[String],
    active_keys: &[String],
) -> Result<String> {
    let mut doc = parse_config_document(content)?;
    for managed_key in managed_keys {
        let normalized = normalize_expected_config_key(managed_key)?;
        if active_keys
            .iter()
            .any(|active_key| active_key.eq_ignore_ascii_case(&normalized))
        {
            continue;
        }
        remove_config_entry(&mut doc, &normalized);
    }
    Ok(ensure_trailing_newline(doc.to_string()))
}

pub fn merge_codex_snapshot_projects_in_config_content(
    baseline_content: &str,
    snapshot_content: &str,
    shared_content: &str,
) -> Result<String> {
    let baseline = parse_config_document(baseline_content)?;
    let snapshot = parse_config_document(snapshot_content)?;
    let mut shared = parse_config_document(shared_content)?;
    let Some(snapshot_projects) = snapshot.get("projects").and_then(Item::as_table_like) else {
        return Ok(ensure_trailing_newline(shared.to_string()));
    };

    for (project_path, snapshot_project) in snapshot_projects.iter() {
        let snapshot_trust = project_trust_level(snapshot_project)?;
        let baseline_trust = baseline
            .get("projects")
            .and_then(Item::as_table_like)
            .and_then(|projects| projects.get(project_path))
            .map(project_trust_level)
            .transpose()?
            .flatten();
        if snapshot_trust == baseline_trust {
            continue;
        }

        let shared_trust = shared
            .get("projects")
            .and_then(Item::as_table_like)
            .and_then(|projects| projects.get(project_path))
            .map(project_trust_level)
            .transpose()?
            .flatten();
        if shared_trust != baseline_trust && shared_trust != snapshot_trust {
            continue;
        }

        set_project_trust_level(&mut shared, project_path, snapshot_trust.as_deref())?;
    }

    Ok(ensure_trailing_newline(shared.to_string()))
}

fn project_trust_level(project: &Item) -> Result<Option<String>> {
    let Some(project) = project.as_table_like() else {
        anyhow::bail!("Codex projects 条目必须是表。");
    };
    let Some(trust_level) = project.get("trust_level") else {
        return Ok(None);
    };
    trust_level
        .as_str()
        .map(|value| Some(value.to_string()))
        .ok_or_else(|| anyhow::anyhow!("Codex projects trust_level 必须是字符串。"))
}

fn set_project_trust_level(
    doc: &mut DocumentMut,
    project_path: &str,
    trust_level: Option<&str>,
) -> Result<()> {
    if doc.get("projects").is_none() {
        doc["projects"] = table();
    }
    let projects = doc
        .get_mut("projects")
        .and_then(Item::as_table_like_mut)
        .ok_or_else(|| anyhow::anyhow!("Codex config projects 必须是表。"))?;
    if projects.get(project_path).is_none() {
        projects.insert(project_path, table());
    }
    let project = projects
        .get_mut(project_path)
        .and_then(Item::as_table_like_mut)
        .ok_or_else(|| anyhow::anyhow!("Codex projects 条目必须是表。"))?;
    if let Some(trust_level) = trust_level {
        project.insert("trust_level", value(trust_level));
    } else {
        project.remove("trust_level");
    }
    Ok(())
}

#[cfg(test)]
pub fn clear_provider_and_set_model_in_config_content(
    content: &str,
    expected_model: &str,
) -> Result<String> {
    clear_provider_and_set_config_entry_in_config_content(content, "model", expected_model)
}

#[cfg(test)]
pub fn set_api_provider_in_config_content(
    content: &str,
    provider_name: &str,
    base_url: &str,
    provider_options: &ApiProviderOptions,
) -> Result<String> {
    let mut doc = parse_config_document(content)?;
    doc["model_provider"] = value(API_PROVIDER_KEY);
    doc.remove("provider");
    doc.remove("wire_api");
    doc["model_providers"][API_PROVIDER_KEY]["name"] = value(provider_name);
    doc["model_providers"][API_PROVIDER_KEY]["base_url"] = value(base_url);
    doc["model_providers"][API_PROVIDER_KEY]["wire_api"] =
        value(provider_options.wire_api.as_str());
    set_local_proxy_auth_header(&mut doc, is_local_webclx_api_url(base_url))?;
    Ok(ensure_trailing_newline(doc.to_string()))
}

pub fn set_api_provider_and_config_entry_in_config_content(
    content: &str,
    provider_name: &str,
    base_url: &str,
    provider_options: &ApiProviderOptions,
    expected_key: &str,
    expected_value: &str,
) -> Result<String> {
    let expected_key = normalize_expected_config_key(expected_key)?;
    let expected_value = normalize_expected_config_value(expected_value)?;
    let mut doc = parse_config_document(content)?;
    set_config_entry(&mut doc, &expected_key, &expected_value)?;
    doc["model_provider"] = value(API_PROVIDER_KEY);
    doc.remove("provider");
    doc.remove("wire_api");
    doc["model_providers"][API_PROVIDER_KEY]["name"] = value(provider_name);
    doc["model_providers"][API_PROVIDER_KEY]["base_url"] = value(base_url);
    doc["model_providers"][API_PROVIDER_KEY]["wire_api"] =
        value(provider_options.wire_api.as_str());
    set_local_proxy_auth_header(&mut doc, is_local_webclx_api_url(base_url))?;
    Ok(ensure_trailing_newline(doc.to_string()))
}

pub fn set_local_proxy_auth_header_in_config_content(
    content: &str,
    enabled: bool,
) -> Result<String> {
    let mut doc = parse_config_document(content)?;
    set_local_proxy_auth_header(&mut doc, enabled)?;
    Ok(ensure_trailing_newline(doc.to_string()))
}

fn set_local_proxy_auth_header(doc: &mut DocumentMut, enabled: bool) -> Result<()> {
    let provider = doc["model_providers"][API_PROVIDER_KEY]
        .as_table_like_mut()
        .ok_or_else(|| anyhow::anyhow!("Codex API provider 必须是表。"))?;
    if enabled {
        if provider.get("env_http_headers").is_none() {
            provider.insert("env_http_headers", table());
        }
        let headers = provider
            .get_mut("env_http_headers")
            .and_then(Item::as_table_like_mut)
            .ok_or_else(|| anyhow::anyhow!("Codex API provider env_http_headers 必须是表。"))?;
        headers.insert(WEBCLX_LOCAL_API_TOKEN_HEADER, value(WEBCLX_LOCAL_API_TOKEN_ENV));
        return Ok(());
    }

    let remove_headers = provider
        .get_mut("env_http_headers")
        .and_then(Item::as_table_like_mut)
        .is_some_and(|headers| {
            headers.remove(WEBCLX_LOCAL_API_TOKEN_HEADER);
            headers.iter().next().is_none()
        });
    if remove_headers {
        provider.remove("env_http_headers");
    }
    Ok(())
}

fn is_local_webclx_api_url(base_url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(base_url) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") || !url.path().starts_with("/api/") {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<std::net::IpAddr>()
            .is_ok_and(|address| address.is_loopback())
}

fn set_config_entry(doc: &mut DocumentMut, key_path: &str, raw_value: &str) -> Result<()> {
    let segments = key_path.split('.').collect::<Vec<_>>();
    let item = parse_config_value_item(raw_value);
    if segments.len() == 1 {
        doc[segments[0]] = item;
        return Ok(());
    }
    if segments.len() > 2 {
        anyhow::bail!("config 键名目前只支持根键或二级键。");
    }

    let parent_key = segments[0];
    let child_key = segments[1];
    let parent = &mut doc[parent_key];
    if parent.is_none() {
        *parent = table();
    }
    let Some(parent_table) = parent.as_table_like_mut() else {
        anyhow::bail!("config 键名 {parent_key} 已存在但不是表，无法写入 {key_path}。");
    };
    parent_table.insert(child_key, item);
    Ok(())
}

fn remove_config_entry(doc: &mut DocumentMut, key_path: &str) {
    let segments = key_path.split('.').collect::<Vec<_>>();
    if segments.len() == 1 {
        let actual_key = {
            doc.iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(segments[0]))
                .map(|(key, _)| key.to_string())
        };
        if let Some(actual_key) = actual_key {
            doc.remove(&actual_key);
        }
        return;
    }
    if segments.len() != 2 {
        return;
    }

    let Some(parent_key) = doc
        .iter()
        .find(|(key, _)| key.eq_ignore_ascii_case(segments[0]))
        .map(|(key, _)| key.to_string())
    else {
        return;
    };
    let remove_parent = {
        let Some(parent_table) = doc.get_mut(&parent_key).and_then(Item::as_table_like_mut) else {
            return;
        };
        let child_key = {
            parent_table
                .iter()
                .find(|(key, _)| key.eq_ignore_ascii_case(segments[1]))
                .map(|(key, _)| key.to_string())
        };
        if let Some(child_key) = child_key {
            parent_table.remove(&child_key);
        }
        parent_table.iter().next().is_none()
    };
    if remove_parent {
        doc.remove(&parent_key);
    }
}

fn parse_config_value_item(raw_value: &str) -> Item {
    let trimmed = raw_value.trim();
    let probe = format!("value = {trimmed}\n");
    match probe.parse::<DocumentMut>() {
        Ok(mut doc) => doc.remove("value").unwrap_or_else(|| value(trimmed)),
        Err(_) => value(trimmed),
    }
}

#[cfg(test)]
pub fn set_api_provider_and_model_in_config_content(
    content: &str,
    provider_name: &str,
    base_url: &str,
    expected_model: &str,
) -> Result<String> {
    set_api_provider_and_config_entry_in_config_content(
        content,
        provider_name,
        base_url,
        &crate::default_api_provider_options(),
        "model",
        expected_model,
    )
}

pub fn parse_claude_settings_document(content: &str) -> Result<Value> {
    if content.trim().is_empty() {
        return Ok(Value::Object(Map::new()));
    }

    let settings: Value = serde_json::from_str(content).context("cannot parse settings.json")?;
    if !settings.is_object() {
        anyhow::bail!("settings.json 顶层必须是对象。");
    }
    Ok(settings)
}

fn set_optional_env_value(env: &mut Map<String, Value>, key: &str, value: Option<&str>) {
    match value {
        Some(value) => {
            env.insert(key.to_string(), Value::String(value.to_string()));
        }
        None => {
            env.remove(key);
        }
    }
}

pub fn set_claude_settings_in_value(settings: Value, preset: &StoredClaudePreset) -> Result<Value> {
    set_claude_settings_in_value_with_endpoint(
        settings,
        preset,
        &preset.base_url,
        &preset.auth_token,
    )
}

pub fn set_claude_settings_in_value_with_endpoint(
    settings: Value,
    preset: &StoredClaudePreset,
    base_url: &str,
    auth_token: &str,
) -> Result<Value> {
    let mut root = settings
        .as_object()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("settings.json 顶层必须是对象。"))?;
    validate_claude_model_selection(
        preset.default_haiku_model.as_deref(),
        preset.default_sonnet_model.as_deref(),
        preset.default_opus_model.as_deref(),
        preset.third_party_model.as_deref(),
    )?;

    let mut env = match root.remove("env") {
        Some(Value::Object(env)) => env,
        Some(Value::Null) | None => Map::new(),
        Some(_) => anyhow::bail!("settings.json 中 env 必须是对象。"),
    };

    env.insert(CLAUDE_AUTH_TOKEN_KEY.to_string(), Value::String(auth_token.to_string()));
    env.remove(CLAUDE_LEGACY_AUTH_TOKEN_KEY);
    env.insert(CLAUDE_BASE_URL_KEY.to_string(), Value::String(base_url.to_string()));
    set_optional_env_value(
        &mut env,
        CLAUDE_DEFAULT_HAIKU_MODEL_KEY,
        preset.default_haiku_model.as_deref(),
    );
    set_optional_env_value(
        &mut env,
        CLAUDE_DEFAULT_SONNET_MODEL_KEY,
        preset.default_sonnet_model.as_deref(),
    );
    set_optional_env_value(
        &mut env,
        CLAUDE_DEFAULT_OPUS_MODEL_KEY,
        preset.default_opus_model.as_deref(),
    );
    set_optional_env_value(&mut env, CLAUDE_MODEL_KEY, preset.third_party_model.as_deref());
    env.remove(CLAUDE_LEGACY_SMALL_FAST_MODEL_KEY);
    apply_claude_config_overrides_to_env(&mut env, &preset.config_overrides)?;

    root.insert("env".to_string(), Value::Object(env));
    Ok(Value::Object(root))
}

fn apply_claude_config_overrides_to_env(
    env: &mut Map<String, Value>,
    config_overrides: &[PresetConfigOverride],
) -> Result<()> {
    for (index, item) in config_overrides.iter().enumerate() {
        let key = item
            .key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("第 {} 个 Claude 额外选项缺少键名。", index + 1))?;
        let value = item
            .value
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("第 {} 个 Claude 额外选项缺少键值。", index + 1))?;
        env.insert(key.to_string(), Value::String(value.to_string()));
    }
    Ok(())
}

fn ensure_trailing_newline(content: String) -> String {
    if content.is_empty() || content.ends_with('\n') {
        content
    } else {
        format!("{content}\n")
    }
}
