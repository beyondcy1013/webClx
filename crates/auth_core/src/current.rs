use std::path::Path;

use tokio::fs;

use crate::{
    AuthFileContent, ConfigProviderState, CurrentAuthState, CurrentClaudeState, StoredClaudePreset,
    derive_current_claude_state, parse_claude_settings_document,
    read_current_config_provider_from_content, validate_api_auth_file_sync,
    validate_auth_file_sync,
};

pub async fn read_current_auth_state(
    path: &Path,
) -> std::result::Result<Option<CurrentAuthState>, String> {
    let content = match fs::read_to_string(path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("读取当前 auth.json 失败: {error}")),
    };

    let parsed = serde_json::from_str::<AuthFileContent>(&content)
        .map_err(|error| format!("当前 auth.json 格式无效: {error}"))?;

    match parsed {
        AuthFileContent::Login(auth) => {
            validate_auth_file_sync(&auth)
                .map_err(|error| format!("当前 auth.json 格式无效: {error}"))?;
            Ok(Some(CurrentAuthState::Login(auth)))
        }
        AuthFileContent::Api(auth) => {
            validate_api_auth_file_sync(&auth)
                .map_err(|error| format!("当前 auth.json 格式无效: {error}"))?;
            Ok(Some(CurrentAuthState::Api(auth)))
        }
    }
}

pub async fn read_current_config_provider(
    path: &Path,
) -> std::result::Result<Option<ConfigProviderState>, String> {
    let content = match fs::read_to_string(path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("读取当前 config.toml 失败: {error}")),
    };

    read_current_config_provider_from_content(&content)
        .map_err(|error| format!("当前 config.toml 格式无效: {error}"))
}

pub async fn read_current_claude_state(
    path: &Path,
    presets: &[StoredClaudePreset],
) -> std::result::Result<Option<CurrentClaudeState>, String> {
    let content = match fs::read_to_string(path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("读取当前 ~/.claude/settings.json 失败: {error}")),
    };

    let settings = parse_claude_settings_document(&content)
        .map_err(|error| format!("当前 ~/.claude/settings.json 格式无效: {error}"))?;
    Ok(derive_current_claude_state(&settings, presets))
}
