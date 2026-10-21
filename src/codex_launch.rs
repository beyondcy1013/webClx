use std::{fs, path::Path};

use anyhow::{Context, Result, bail};
use toml_edit::DocumentMut;

use crate::runtime_paths;

const CODEX_CONFIG_RELATIVE_PATH: &str = ".codex/config.toml";

pub(crate) fn prepare_codex_history_command_for_user(
    user_name: &str,
    command_line: &str,
) -> Result<String> {
    if codex_history_command_target(command_line).is_none() {
        return Ok(command_line.trim().to_string());
    }

    let profile = runtime_paths::resolve_user_profile(user_name)
        .with_context(|| format!("无法解析终端用户 `{user_name}`"))?;
    let config_path = profile.home.join(CODEX_CONFIG_RELATIVE_PATH);
    let model = read_codex_model(&config_path)?;
    Ok(codex_history_command_with_model(command_line, model.as_deref()))
}

pub(crate) fn codex_history_command_with_model(command_line: &str, model: Option<&str>) -> String {
    let command_line = command_line.trim();
    let Some(model) = normalized_model(model) else {
        return command_line.to_string();
    };
    let Ok(mut tokens) = shell_words::split(command_line) else {
        return command_line.to_string();
    };
    let Some(target) = codex_history_tokens_target(&tokens) else {
        return command_line.to_string();
    };
    if codex_history_tokens_have_model_override(&tokens[target.codex_index + 2..]) {
        return command_line.to_string();
    }

    tokens.insert(target.codex_index + 2, "--model".to_string());
    tokens.insert(target.codex_index + 3, model.to_string());
    shell_words::join(tokens.iter().map(String::as_str))
}

pub(crate) fn codex_history_args_with_model(
    agent: &str,
    args: &[String],
    model: Option<&str>,
) -> Vec<String> {
    let Some(model) = normalized_model(model) else {
        return args.to_vec();
    };
    if !is_program(agent, "codex")
        || !matches!(args.first().map(String::as_str), Some("resume" | "fork"))
        || codex_history_tokens_have_model_override(&args[1..])
    {
        return args.to_vec();
    }

    let mut prepared = args.to_vec();
    prepared.insert(1, "--model".to_string());
    prepared.insert(2, model.to_string());
    prepared
}

pub(crate) fn read_codex_model(config_path: &Path) -> Result<Option<String>> {
    let content = match fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("无法读取 {}", config_path.display()));
        }
    };
    codex_model_from_config_content(&content)
        .with_context(|| format!("无法解析 {}", config_path.display()))
}

pub(crate) fn read_codex_model_provider(config_path: &Path) -> Result<Option<String>> {
    let content = match fs::read_to_string(config_path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("无法读取 {}", config_path.display()));
        }
    };
    codex_model_provider_from_config_content(&content)
        .with_context(|| format!("无法解析 {}", config_path.display()))
}

fn codex_model_from_config_content(content: &str) -> Result<Option<String>> {
    let document = content
        .parse::<DocumentMut>()
        .context("config.toml 格式无效")?;
    let Some(item) = document.get("model") else {
        return Ok(None);
    };
    let Some(model) = item.as_str() else {
        bail!("config.toml 顶级 model 必须是字符串");
    };
    Ok(normalized_model(Some(model)).map(str::to_string))
}

fn codex_model_provider_from_config_content(content: &str) -> Result<Option<String>> {
    let document = content
        .parse::<DocumentMut>()
        .context("config.toml 格式无效")?;
    let Some(item) = document
        .get("model_provider")
        .or_else(|| document.get("provider"))
    else {
        return Ok(None);
    };
    let Some(provider) = item.as_str() else {
        bail!("config.toml 顶级 model_provider/provider 必须是字符串");
    };
    let Some(provider) = normalized_model(Some(provider)) else {
        bail!("config.toml 顶级 model_provider/provider 不能为空");
    };
    Ok(Some(provider.to_string()))
}

#[derive(Clone, Copy)]
struct CodexHistoryCommandTarget {
    codex_index: usize,
}

fn codex_history_command_target(command_line: &str) -> Option<CodexHistoryCommandTarget> {
    let tokens = shell_words::split(command_line.trim()).ok()?;
    let target = codex_history_tokens_target(&tokens)?;
    if target.codex_index != 0 {
        return None;
    }
    if codex_history_tokens_have_model_override(&tokens[target.codex_index + 2..]) {
        return None;
    }
    Some(target)
}

fn codex_history_tokens_target(tokens: &[String]) -> Option<CodexHistoryCommandTarget> {
    for (index, token) in tokens.iter().enumerate() {
        if !is_program(token, "codex")
            || !matches!(tokens.get(index + 1).map(String::as_str), Some("resume" | "fork"))
        {
            continue;
        }
        let direct = index == 0;
        if direct {
            return Some(CodexHistoryCommandTarget { codex_index: index });
        }
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

fn normalized_model(model: Option<&str>) -> Option<&str> {
    model.map(str::trim).filter(|model| !model.is_empty())
}

fn is_program(candidate: &str, expected: &str) -> bool {
    Path::new(candidate)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case(expected))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_config_model_overrides_history_for_resume_and_fork() {
        let resume_id = "019f2350-db5f-7cf0-b476-1cf14855b05d";
        assert_eq!(
            codex_history_command_with_model(
                &format!("codex resume {resume_id}"),
                Some("gpt-5.6-sol"),
            ),
            format!("codex resume --model gpt-5.6-sol {resume_id}")
        );
        assert_eq!(
            codex_history_command_with_model(
                &format!("codex fork {resume_id}"),
                Some("gpt-5.6-terra"),
            ),
            format!("codex fork --model gpt-5.6-terra {resume_id}")
        );
    }

    #[test]
    fn selected_preset_run_receives_the_applied_model() {
        let resume_id = "019f2350-db5f-7cf0-b476-1cf14855b05d";
        assert_eq!(
            codex_history_command_with_model(
                &format!("codex resume {resume_id}"),
                Some("gpt-5.6-sol"),
            ),
            format!("codex resume --model gpt-5.6-sol {resume_id}")
        );
    }

    #[test]
    fn unresolvable_user_fails_closed_for_resume_commands() {
        let command = "codex resume 019f2350-db5f-7cf0-b476-1cf14855b05d";
        assert!(codex_history_command_target(command).is_some());
        // 恢复/fork 必须迁移当前配置的 model；用户无法解析时必须失败并上报，
        // 禁止静默放行导致用历史 rollout 的旧 model 启动。
        let error = prepare_codex_history_command_for_user("missing-webclx-test-user", command)
            .expect_err("unresolvable user must fail closed");
        assert!(error.to_string().contains("missing-webclx-test-user"));
        assert!(codex_history_command_target("codex resume session-id").is_some());
    }

    #[test]
    fn explicit_model_and_unrelated_commands_are_preserved() {
        let resume_id = "019f2350-db5f-7cf0-b476-1cf14855b05d";
        let explicit = format!("codex resume --model old-model {resume_id}");
        assert_eq!(codex_history_command_with_model(&explicit, Some("gpt-5.6-sol")), explicit);
        assert_eq!(
            codex_history_command_with_model("echo codex resume unsafe", Some("gpt-5.6-sol")),
            "echo codex resume unsafe"
        );
    }

    #[test]
    fn model_is_shell_quoted_without_execution_syntax() {
        let prepared = codex_history_command_with_model(
            "codex resume session-id",
            Some("model'; echo unsafe"),
        );
        assert_eq!(
            shell_words::split(&prepared).unwrap(),
            [
                "codex",
                "resume",
                "--model",
                "model'; echo unsafe",
                "session-id",
            ]
        );
    }

    #[test]
    fn parses_only_a_top_level_string_model() {
        assert_eq!(
            codex_model_from_config_content(
                "model = \"gpt-5.6-sol\"\nmodel_provider = \"webclx_api\"\n",
            )
            .unwrap(),
            Some("gpt-5.6-sol".to_string())
        );
        assert_eq!(codex_model_from_config_content("model_provider = \"openai\"\n").unwrap(), None);
        assert!(codex_model_from_config_content("model = 56\n").is_err());
    }

    #[test]
    fn parses_explicit_provider_and_leaves_builtin_openai_implicit() {
        assert_eq!(
            codex_model_provider_from_config_content(
                "model = \"gpt-6-astra\"\nmodel_provider = \"webclx_api\"\n",
            )
            .unwrap(),
            Some("webclx_api".to_string())
        );
        assert_eq!(
            codex_model_provider_from_config_content(
                "model = \"gpt-6-astra\"\nprovider = \"legacy_api\"\n",
            )
            .unwrap(),
            Some("legacy_api".to_string())
        );
        assert_eq!(
            codex_model_provider_from_config_content("model = \"gpt-6-astra\"\n").unwrap(),
            None
        );
        assert!(codex_model_provider_from_config_content("model_provider = 6\n").is_err());
        assert!(codex_model_provider_from_config_content("model_provider = \" \"\n").is_err());
    }

    #[test]
    fn process_args_insert_model_after_history_subcommand() {
        let args = vec!["resume".to_string(), "session-id".to_string()];
        assert_eq!(
            codex_history_args_with_model("/usr/local/bin/codex", &args, Some("gpt-5.6-sol")),
            ["resume", "--model", "gpt-5.6-sol", "session-id"]
        );
    }
}
