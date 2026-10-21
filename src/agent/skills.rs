use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use tokio::fs;

use crate::{ApiResult, AppError, AppState, builtin_skills, runtime_paths};

const SKILLS_DIR_RELATIVE: &str = ".codex/skills";

pub(super) fn skills_dir_for_user(user: &str) -> ApiResult<PathBuf> {
    runtime_paths::resolve_user_home_preferring_env(user)
        .map(|home| home.join(SKILLS_DIR_RELATIVE))
        .map_err(|e| AppError::internal(format!("解析 skills 目录失败: {e}")))
}

/// Scan both the user's ~/.codex/skills and any configured extra dirs.
/// Filters out disabled skills. Each skill entry includes `source` and `disabled`.
pub(super) async fn scan_all_skills(state: &AppState) -> ApiResult<Vec<Value>> {
    let user = state.workspace_settings.terminal_user();
    let primary_dir = skills_dir_for_user(&user)?;
    let agent_config = state.agent_config.get().await;
    let disabled: Vec<String> = agent_config.disabled_skills.clone();
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut skills: Vec<Value> = Vec::new();

    // Primary skills dir
    if primary_dir.exists() {
        scan_skills_dir(&primary_dir, "user", &disabled, &mut seen_names, &mut skills).await?;
    }
    // Extra skill dirs
    for extra_dir in &agent_config.extra_skill_dirs {
        if extra_dir.trim().is_empty() {
            continue;
        }
        let path = PathBuf::from(extra_dir);
        if path.exists() {
            scan_skills_dir(&path, "extra", &disabled, &mut seen_names, &mut skills).await?;
        }
    }
    let builtin_dir = builtin_skills::root_dir(&state.app_dir);
    if builtin_dir.exists() {
        scan_skills_dir(&builtin_dir, "builtin", &disabled, &mut seen_names, &mut skills).await?;
    }
    // Also include disabled skills as entries so the UI can show them
    for name in &disabled {
        if !seen_names.contains(name) {
            skills.push(json!({
                "name": name,
                "description": "",
                "path": "",
                "source": "disabled",
                "disabled": true,
            }));
        }
    }
    skills.sort_by(|a, b| {
        a["name"]
            .as_str()
            .unwrap_or("")
            .cmp(b["name"].as_str().unwrap_or(""))
    });
    Ok(skills)
}

async fn scan_skills_dir(
    dir: &Path,
    source: &str,
    disabled: &[String],
    seen_names: &mut std::collections::HashSet<String>,
    skills: &mut Vec<Value>,
) -> ApiResult<()> {
    let mut entries = fs::read_dir(dir).await.map_err(|e| {
        AppError::internal(format!("读取 skills 目录失败 ({}) : {e}", dir.display()))
    })?;
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| AppError::internal(format!("读取目录项失败: {e}")))?
    {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_md = path.join("SKILL.md");
        if !skill_md.exists() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if seen_names.contains(&name) {
            continue;
        }
        seen_names.insert(name.clone());
        let content = fs::read_to_string(&skill_md).await.unwrap_or_default();
        let description = parse_skill_description(&content);
        let is_disabled = disabled.contains(&name);
        skills.push(json!({
            "name": name,
            "description": description,
            "path": path.to_string_lossy(),
            "source": source,
            "disabled": is_disabled,
        }));
    }
    Ok(())
}

/// Parse YAML front-matter `description:` (or `name:`) from SKILL.md.
fn parse_skill_description(content: &str) -> String {
    if let Some(desc) = parse_yaml_front_matter_field(content, "description") {
        return desc;
    }
    if let Some(desc) = parse_yaml_front_matter_field(content, "name") {
        return desc;
    }
    // Fallback: first non-empty paragraph after front-matter + headings
    let mut in_front_matter = false;
    let mut after_front_matter = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if !after_front_matter {
                in_front_matter = !in_front_matter;
                if !in_front_matter {
                    after_front_matter = true;
                }
            }
            continue;
        }
        if in_front_matter || !after_front_matter {
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        return trimmed.to_string();
    }
    String::new()
}

fn parse_yaml_front_matter_field(content: &str, field: &str) -> Option<String> {
    let mut lines = content.lines();
    let first = lines.next()?.trim();
    if first != "---" {
        return None;
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            return None;
        }
        if let Some(rest) = trimmed.strip_prefix(&format!("{field}:")) {
            let value = rest.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}
