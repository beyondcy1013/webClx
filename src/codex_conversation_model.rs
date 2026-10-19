use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    AppError, AppState,
    codex_launch::{read_codex_model, read_codex_model_provider},
    runtime_paths,
};

const BUILTIN_OPENAI_PROVIDER: &str = "openai";

#[derive(Debug, Deserialize)]
pub(crate) struct UpdateCodexConversationModelRequest {
    session: String,
    #[serde(default)]
    model: Option<String>,
}

#[derive(Debug, Serialize)]
pub(crate) struct UpdateCodexConversationModelResponse {
    session: String,
    model: String,
    model_provider: String,
    rollout_file: String,
    backup_file: Option<String>,
    rollout_entries_updated: usize,
    turn_contexts_updated: usize,
    provider_entries_updated: usize,
}

pub(crate) async fn update_codex_conversation_model(
    State(state): State<AppState>,
    Json(payload): Json<UpdateCodexConversationModelRequest>,
) -> Result<Json<UpdateCodexConversationModelResponse>, AppError> {
    let session = validated_session_id(&payload.session)
        .map_err(|error| AppError::bad_request(format!("修改 Codex 会话模型失败: {error}")))?;
    let profile = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|error| AppError::bad_request(format!("用户身份无效: {error}")))?;
    let codex_home = profile.home.join(".codex");
    let requested_model = payload.model.and_then(normalized_model);
    let model = match requested_model {
        Some(model) => model,
        None => read_codex_model(&codex_home.join("config.toml"))
            .map_err(|error| {
                AppError::bad_request(format!("读取当前 Codex 预设模型失败: {error}"))
            })?
            .and_then(normalized_model)
            .ok_or_else(|| {
                AppError::bad_request("当前 Codex 预设未配置 model，请显式传入 model。")
            })?,
    };

    let model_provider = read_codex_model_provider(&codex_home.join("config.toml"))
        .map_err(|error| AppError::bad_request(format!("读取当前 Codex provider 失败: {error}")))?
        .unwrap_or_else(|| BUILTIN_OPENAI_PROVIDER.to_string());

    tokio::task::spawn_blocking(move || {
        update_rollout_selection(&codex_home, &session, Some(&model), &model_provider)
    })
    .await
    .map_err(|error| AppError::internal(format!("修改 Codex 会话模型任务失败: {error}")))?
    .map(Json)
    .map_err(|error| AppError::bad_request(format!("修改 Codex 会话模型失败: {error}")))
}

fn normalized_model(model: String) -> Option<String> {
    let model = model.trim().to_string();
    (!model.is_empty()).then_some(model)
}

pub(crate) fn prepare_codex_history_model_for_user(
    user_name: &str,
    command_line: &str,
) -> Result<String> {
    let Some(session) = codex_resume_session_id(command_line) else {
        return Ok(command_line.trim().to_string());
    };
    let profile = runtime_paths::resolve_user_profile(user_name)
        .with_context(|| format!("无法解析终端用户 `{user_name}`"))?;
    let codex_home = profile.home.join(".codex");
    let config_path = codex_home.join("config.toml");
    let model = if codex_command_has_model_override(command_line) {
        None
    } else {
        read_codex_model(&config_path)?.and_then(normalized_model)
    };
    let model_provider = read_codex_model_provider(&config_path)?
        .unwrap_or_else(|| BUILTIN_OPENAI_PROVIDER.to_string());
    update_rollout_selection(&codex_home, &session, model.as_deref(), &model_provider)?;
    Ok(command_line.trim().to_string())
}

pub(crate) fn prepare_codex_history_args_model_for_current_user(
    agent: &str,
    args: &[String],
    model: Option<&str>,
    model_provider: &str,
) -> Result<()> {
    let Some(session) = codex_history_args_session_id(agent, args) else {
        return Ok(());
    };
    let model = (!codex_history_tokens_have_model_override(&args[1..]))
        .then_some(model)
        .flatten()
        .map(str::trim)
        .filter(|model| !model.is_empty());
    let profile = runtime_paths::resolve_current_user_profile().context("无法解析当前终端用户")?;
    update_rollout_selection(&profile.home.join(".codex"), &session, model, model_provider)?;
    Ok(())
}

fn codex_history_args_session_id(agent: &str, args: &[String]) -> Option<String> {
    let is_codex = Path::new(agent)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("codex"));
    if !is_codex || !matches!(args.first().map(String::as_str), Some("resume")) {
        return None;
    }
    args.iter()
        .skip(1)
        .find_map(|argument| validated_session_id(argument).ok())
}

fn codex_resume_session_id(command_line: &str) -> Option<String> {
    let tokens = shell_words::split(command_line.trim()).ok()?;
    for (index, token) in tokens.iter().enumerate() {
        let is_codex = Path::new(token)
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("codex"));
        if index != 0 || !is_codex || tokens.get(index + 1).map(String::as_str) != Some("resume") {
            continue;
        }
        return tokens
            .iter()
            .skip(index + 2)
            .find_map(|token| validated_session_id(token).ok());
    }
    None
}

fn codex_command_has_model_override(command_line: &str) -> bool {
    let Ok(tokens) = shell_words::split(command_line.trim()) else {
        return false;
    };
    codex_history_tokens_have_model_override(&tokens[2..])
}

fn codex_history_tokens_have_model_override(tokens: &[String]) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        token == "-m"
            || token == "--model"
            || token.starts_with("--model=")
            || ((token == "-c" || token == "--config")
                && tokens
                    .get(index + 1)
                    .is_some_and(|value| value.trim_start().starts_with("model=")))
            || token
                .strip_prefix("--config=")
                .is_some_and(|value| value.trim_start().starts_with("model="))
    })
}

fn validated_session_id(raw: &str) -> Result<String> {
    let session = raw.trim();
    let valid = session.len() == 36
        && session
            .chars()
            .enumerate()
            .all(|(index, character)| match index {
                8 | 13 | 18 | 23 => character == '-',
                _ => character.is_ascii_hexdigit(),
            });
    if !valid {
        bail!("session 必须是有效的 UUID。");
    }
    Ok(session.to_ascii_lowercase())
}

fn update_rollout_selection(
    codex_home: &Path,
    session: &str,
    model: Option<&str>,
    model_provider: &str,
) -> Result<UpdateCodexConversationModelResponse> {
    let rollout_file = find_rollout_file(&codex_home.join("sessions"), session)?
        .ok_or_else(|| anyhow::anyhow!("Codex 会话 `{session}` 不存在。"))?;
    let original = fs::read_to_string(&rollout_file)
        .with_context(|| format!("无法读取 rollout: {}", rollout_file.display()))?;
    let (updated, stats) = rewrite_rollout_selection(&original, model, model_provider)?;

    let backup_file = if stats.rollout_entries_updated == 0 {
        None
    } else {
        Some(write_rollout_atomically(&rollout_file, updated.as_bytes())?)
    };
    Ok(UpdateCodexConversationModelResponse {
        session: session.to_string(),
        model: model.unwrap_or_default().to_string(),
        model_provider: model_provider.to_string(),
        rollout_file: rollout_file.display().to_string(),
        backup_file: backup_file.map(|path| path.display().to_string()),
        rollout_entries_updated: stats.rollout_entries_updated,
        turn_contexts_updated: stats.turn_contexts_updated,
        provider_entries_updated: stats.provider_entries_updated,
    })
}

fn find_rollout_file(sessions_dir: &Path, session: &str) -> Result<Option<PathBuf>> {
    if !sessions_dir.exists() {
        return Ok(None);
    }
    let mut directories = vec![sessions_dir.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("无法读取会话目录: {}", directory.display()))?
        {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                directories.push(entry.path());
            } else if file_type.is_file() {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if name.ends_with(".jsonl") && name.contains(session) {
                    return Ok(Some(entry.path()));
                }
            }
        }
    }
    Ok(None)
}

#[derive(Debug, Default, PartialEq, Eq)]
struct RolloutRewriteStats {
    rollout_entries_updated: usize,
    turn_contexts_updated: usize,
    provider_entries_updated: usize,
}

fn rewrite_rollout_selection(
    content: &str,
    model: Option<&str>,
    model_provider: &str,
) -> Result<(String, RolloutRewriteStats)> {
    let mut output = String::with_capacity(content.len());
    let mut stats = RolloutRewriteStats::default();
    for line_with_newline in content.split_inclusive('\n') {
        let line = line_with_newline
            .strip_suffix('\n')
            .unwrap_or(line_with_newline);
        if !line.contains("\"turn_context\"")
            && !line.contains("\"session_meta\"")
            && !line.contains("\"thread_settings_applied\"")
        {
            output.push_str(line_with_newline);
            continue;
        }
        let mut value: Value = serde_json::from_str(line).context("rollout JSON 无效")?;
        let entry_type = value
            .get("type")
            .and_then(Value::as_str)
            .map(str::to_string);
        let payload = value
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| anyhow::anyhow!("rollout 条目缺少 payload 对象"))?;
        let mut changed = false;
        let mut provider_changed = false;
        let mut turn_context_changed = false;

        match entry_type.as_deref() {
            Some("session_meta") => {
                let provider_value = payload.entry("model_provider").or_insert(Value::Null);
                if provider_value.as_str() != Some(model_provider) {
                    *provider_value = Value::String(model_provider.to_string());
                    changed = true;
                    provider_changed = true;
                }
            }
            Some("turn_context") => {
                if let Some(model) = model {
                    let model_value = payload.entry("model").or_insert(Value::Null);
                    if model_value.as_str() != Some(model) {
                        *model_value = Value::String(model.to_string());
                        changed = true;
                        turn_context_changed = true;
                    }
                    if rewrite_collaboration_model(payload, model) {
                        changed = true;
                        turn_context_changed = true;
                    }
                }
            }
            Some("event_msg")
                if payload.get("type").and_then(Value::as_str)
                    == Some("thread_settings_applied") =>
            {
                if let Some(thread_settings) = payload
                    .get_mut("thread_settings")
                    .and_then(Value::as_object_mut)
                {
                    let provider_value = thread_settings
                        .entry("model_provider_id")
                        .or_insert(Value::Null);
                    if provider_value.as_str() != Some(model_provider) {
                        *provider_value = Value::String(model_provider.to_string());
                        changed = true;
                        provider_changed = true;
                    }
                    if let Some(model) = model {
                        let model_value = thread_settings.entry("model").or_insert(Value::Null);
                        if model_value.as_str() != Some(model) {
                            *model_value = Value::String(model.to_string());
                            changed = true;
                        }
                        if rewrite_collaboration_model(thread_settings, model) {
                            changed = true;
                        }
                    }
                }
            }
            _ => {}
        }
        if changed {
            stats.rollout_entries_updated += 1;
        }
        if turn_context_changed {
            stats.turn_contexts_updated += 1;
        }
        if provider_changed {
            stats.provider_entries_updated += 1;
        }
        output.push_str(&serde_json::to_string(&value)?);
        if line_with_newline.ends_with('\n') {
            output.push('\n');
        }
    }
    Ok((output, stats))
}

fn rewrite_collaboration_model(payload: &mut serde_json::Map<String, Value>, model: &str) -> bool {
    let Some(collaboration_mode) = payload
        .get_mut("collaboration_mode")
        .and_then(Value::as_object_mut)
    else {
        return false;
    };
    let settings = collaboration_mode
        .entry("settings")
        .or_insert_with(|| Value::Object(Default::default()));
    let Some(settings) = settings.as_object_mut() else {
        return false;
    };
    let settings_model = settings.entry("model").or_insert(Value::Null);
    if settings_model.as_str() == Some(model) {
        return false;
    }
    *settings_model = Value::String(model.to_string());
    true
}

fn write_rollout_atomically(path: &Path, content: &[u8]) -> Result<PathBuf> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let backup = path.with_extension(format!("jsonl.webclx-model-{timestamp}.bak"));
    fs::copy(path, &backup).with_context(|| format!("无法备份 rollout: {}", backup.display()))?;

    if let Err(error) = crate::atomic_file::write_atomic(path, content) {
        let _ = fs::remove_file(&backup);
        return Err(error).with_context(|| format!("无法原子更新 rollout: {}", path.display()));
    }
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_model_and_provider_from_the_same_current_selection() {
        let source = concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"model_provider\":\"cdx_sub2api\"}}\n",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"old\",\"cwd\":\"/tmp\",\"collaboration_mode\":{\"settings\":{\"model\":\"old\"}}}}\n",
            "{\"type\":\"event_msg\",\"payload\":{\"type\":\"thread_settings_applied\",\"thread_settings\":{\"model\":\"old\",\"model_provider_id\":\"cdx_sub2api\",\"collaboration_mode\":{\"settings\":{\"model\":\"old\"}}}}}\n"
        );
        let (output, stats) =
            rewrite_rollout_selection(source, Some("gpt-6-astra"), "openai").unwrap();
        assert_eq!(stats.rollout_entries_updated, 3);
        assert_eq!(stats.turn_contexts_updated, 1);
        assert_eq!(stats.provider_entries_updated, 2);
        assert_eq!(output.matches("\"model_provider\":\"openai\"").count(), 1);
        assert_eq!(output.matches("\"model_provider_id\":\"openai\"").count(), 1);
        assert_eq!(output.matches("\"model\":\"gpt-6-astra\"").count(), 4);
    }

    #[test]
    fn rewrites_collaboration_mode_settings_model() {
        let source = concat!(
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"old\",\"collaboration_mode\":{\"settings\":{\"model\":\"old\"}}}}\n",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"old\",\"collaboration_mode\":{}}}\n"
        );
        let (output, stats) = rewrite_rollout_selection(source, Some("new"), "openai").unwrap();
        assert_eq!(stats.rollout_entries_updated, 2);
        assert_eq!(stats.turn_contexts_updated, 2);
        assert_eq!(output.matches("\"model\":\"new\"").count(), 4);
    }

    #[test]
    fn explicit_model_override_still_migrates_the_provider() {
        let source = concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"model_provider\":\"cdx_sub2api\"}}\n",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"explicit-model\"}}\n"
        );
        let (output, stats) = rewrite_rollout_selection(source, None, "openai").unwrap();
        assert_eq!(stats.provider_entries_updated, 1);
        assert!(output.contains("\"model_provider\":\"openai\""));
        assert!(output.contains("\"model\":\"explicit-model\""));
    }

    #[test]
    fn extracts_resume_session_id_without_explicit_model_override() {
        let session = "019fab94-06ef-77d0-a208-cfd209751c77";
        assert_eq!(
            codex_resume_session_id(&format!("codex resume {session}")).as_deref(),
            Some(session)
        );
        assert_eq!(
            codex_resume_session_id(&format!("/usr/local/bin/codex resume {session} --search"))
                .as_deref(),
            Some(session)
        );
        assert_eq!(
            codex_resume_session_id(&format!("codex resume --model old {session}")).as_deref(),
            Some(session)
        );
        assert_eq!(
            codex_resume_session_id("claude --resume 019fab94-06ef-77d0-a208-cfd209751c77"),
            None
        );
        assert_eq!(
            codex_resume_session_id(
                "webclx run api api-1 -- codex resume 019fab94-06ef-77d0-a208-cfd209751c77"
            ),
            None
        );
    }

    #[test]
    fn extracts_only_resume_ids_from_process_arguments() {
        let session = "019fab94-06ef-77d0-a208-cfd209751c77";
        assert_eq!(
            codex_history_args_session_id(
                "/usr/local/bin/codex",
                &["resume".into(), session.into()]
            )
            .as_deref(),
            Some(session)
        );
        assert_eq!(
            codex_history_args_session_id(
                "codex",
                &["fork".into(), "--search".into(), session.into()]
            ),
            None
        );
        assert_eq!(
            codex_history_args_session_id(
                "codex",
                &[
                    "resume".into(),
                    "--model".into(),
                    "explicit".into(),
                    session.into(),
                ]
            ),
            Some(session.to_string())
        );
        assert_eq!(
            codex_history_args_session_id("claude", &["resume".into(), session.into()]),
            None
        );
    }

    #[test]
    fn rejects_invalid_session_ids_and_turn_contexts() {
        assert!(validated_session_id("not-a-session").is_err());
        assert!(
            rewrite_rollout_selection("{bad json \"turn_context\"}\n", Some("new"), "openai")
                .is_err()
        );
    }
}
