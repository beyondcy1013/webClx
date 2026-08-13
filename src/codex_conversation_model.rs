use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{AppError, AppState, codex_launch::read_codex_model, runtime_paths};

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
    rollout_file: String,
    backup_file: Option<String>,
    turn_contexts_updated: usize,
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

    tokio::task::spawn_blocking(move || update_rollout_model(&codex_home, &session, &model))
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
    let Some(model) = read_codex_model(&codex_home.join("config.toml"))?.and_then(normalized_model)
    else {
        return Ok(command_line.trim().to_string());
    };
    update_rollout_model(&codex_home, &session, &model)?;
    Ok(command_line.trim().to_string())
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
        if codex_history_tokens_have_model_override(&tokens[index + 2..]) {
            return None;
        }
        return validated_session_id(tokens.get(index + 2)?).ok();
    }
    None
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

fn update_rollout_model(
    codex_home: &Path,
    session: &str,
    model: &str,
) -> Result<UpdateCodexConversationModelResponse> {
    let rollout_file = find_rollout_file(&codex_home.join("sessions"), session)?
        .ok_or_else(|| anyhow::anyhow!("Codex 会话 `{session}` 不存在。"))?;
    let original = fs::read_to_string(&rollout_file)
        .with_context(|| format!("无法读取 rollout: {}", rollout_file.display()))?;
    let (updated, turn_contexts_updated, _) = rewrite_turn_context_models(&original, model)?;

    let backup_file = if turn_contexts_updated == 0 {
        None
    } else {
        Some(write_rollout_atomically(&rollout_file, updated.as_bytes())?)
    };
    Ok(UpdateCodexConversationModelResponse {
        session: session.to_string(),
        model: model.to_string(),
        rollout_file: rollout_file.display().to_string(),
        backup_file: backup_file.map(|path| path.display().to_string()),
        turn_contexts_updated,
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

fn rewrite_turn_context_models(content: &str, model: &str) -> Result<(String, usize, usize)> {
    let mut output = String::with_capacity(content.len());
    let mut updated = 0;
    let mut seen = 0;
    for line_with_newline in content.split_inclusive('\n') {
        let line = line_with_newline
            .strip_suffix('\n')
            .unwrap_or(line_with_newline);
        if !line.contains("\"turn_context\"") {
            output.push_str(line_with_newline);
            continue;
        }
        let mut value: Value = serde_json::from_str(line).context("turn_context JSON 无效")?;
        if value.get("type").and_then(Value::as_str) != Some("turn_context") {
            output.push_str(line_with_newline);
            continue;
        }
        seen += 1;
        let payload = value
            .get_mut("payload")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| anyhow::anyhow!("turn_context 缺少 payload 对象"))?;
        let mut changed = false;
        let model_value = payload.entry("model").or_insert(Value::Null);
        if model_value.as_str() != Some(model) {
            *model_value = Value::String(model.to_string());
            changed = true;
        }
        if let Some(collaboration_mode) = payload
            .get_mut("collaboration_mode")
            .and_then(Value::as_object_mut)
        {
            let settings = collaboration_mode
                .entry("settings")
                .or_insert_with(|| Value::Object(Default::default()));
            if let Some(settings) = settings.as_object_mut() {
                let settings_model = settings.entry("model").or_insert(Value::Null);
                if settings_model.as_str() != Some(model) {
                    *settings_model = Value::String(model.to_string());
                    changed = true;
                }
            }
        }
        if changed {
            updated += 1;
        }
        output.push_str(&serde_json::to_string(&value)?);
        if line_with_newline.ends_with('\n') {
            output.push('\n');
        }
    }
    Ok((output, updated, seen))
}

fn write_rollout_atomically(path: &Path, content: &[u8]) -> Result<PathBuf> {
    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let backup = path.with_extension(format!("jsonl.webclx-model-{timestamp}.bak"));
    fs::copy(path, &backup).with_context(|| format!("无法备份 rollout: {}", backup.display()))?;

    let temporary = path.with_extension(format!("jsonl.webclx-model-{timestamp}.tmp"));
    let result = (|| -> Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(content)?;
        file.sync_all()?;
        fs::set_permissions(&temporary, fs::metadata(path)?.permissions())?;
        fs::rename(&temporary, path)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        let _ = fs::remove_file(&backup);
        return Err(error).with_context(|| format!("无法原子更新 rollout: {}", path.display()));
    }
    Ok(backup)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_only_turn_context_models() {
        let source = concat!(
            "{\"type\":\"session_meta\",\"payload\":{\"model\":\"keep\"}}\n",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"old\",\"cwd\":\"/tmp\",\"collaboration_mode\":{\"settings\":{\"model\":\"old\"}}}}\n",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"new\"}}\n"
        );
        let (output, updated, seen) = rewrite_turn_context_models(source, "new").unwrap();
        assert_eq!(updated, 1);
        assert_eq!(seen, 2);
        assert!(output.contains("\"session_meta\",\"payload\":{\"model\":\"keep\"}"));
        assert_eq!(output.matches("\"model\":\"new\"").count(), 3);
    }

    #[test]
    fn rewrites_collaboration_mode_settings_model() {
        let source = concat!(
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"old\",\"collaboration_mode\":{\"settings\":{\"model\":\"old\"}}}}\n",
            "{\"type\":\"turn_context\",\"payload\":{\"model\":\"old\",\"collaboration_mode\":{}}}\n"
        );
        let (output, updated, seen) = rewrite_turn_context_models(source, "new").unwrap();
        assert_eq!(updated, 2);
        assert_eq!(seen, 2);
        assert_eq!(output.matches("\"model\":\"new\"").count(), 4);
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
        assert_eq!(codex_resume_session_id(&format!("codex resume --model old {session}")), None);
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
    fn rejects_invalid_session_ids_and_turn_contexts() {
        assert!(validated_session_id("not-a-session").is_err());
        assert!(rewrite_turn_context_models("{bad json \"turn_context\"}\n", "new").is_err());
    }
}
