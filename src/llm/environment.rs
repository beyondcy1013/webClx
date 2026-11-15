use std::time::Duration;

use anyhow::Context;
use auth_core::PresetTerminalEnvVar;
use settings_core::SettingsManager;
use tracing::warn;

use crate::{ApiResult, AppError, proxy::ProxyManager};

const LLM_NETWORK_ENV_KEYS: [&str; 8] = [
    "HTTP_PROXY",
    "HTTPS_PROXY",
    "ALL_PROXY",
    "http_proxy",
    "https_proxy",
    "all_proxy",
    "NO_PROXY",
    "no_proxy",
];

pub(crate) struct LlmHttpEnvironment {
    shell_env: Vec<(String, String)>,
    terminal_default_env: Vec<(String, String)>,
    proxy_env: Vec<(String, String)>,
}

pub(crate) struct LlmHttpContext {
    pub(crate) client: reqwest::Client,
    pub(crate) environment_summary: String,
}

impl LlmHttpEnvironment {
    pub(crate) async fn capture(
        proxy_manager: &ProxyManager,
        workspace_settings: &SettingsManager,
    ) -> ApiResult<Self> {
        let user_profile = workspace_settings
            .terminal_user_profile()
            .map_err(|error| AppError::bad_request(format!("终端用户环境无效: {error}")))?;
        let shell_env = match tokio::task::spawn_blocking(move || {
            crate::shell_env::read_user_shell_env(&user_profile)
        })
        .await
        {
            Ok(Ok(snapshot)) => snapshot.entries,
            Ok(Err(error)) => {
                warn!("read LLM terminal shell env failed: {error}");
                Vec::new()
            }
            Err(error) => {
                warn!("join LLM terminal shell env task failed: {error}");
                Vec::new()
            }
        };
        let shell_env = merge_inherited_network_env(&shell_env);
        Ok(Self {
            shell_env,
            terminal_default_env: workspace_settings.terminal_default_env_entries(),
            proxy_env: proxy_manager.get_proxy_env(),
        })
    }

    pub(crate) fn context_for(
        &self,
        preset_env: &[PresetTerminalEnvVar],
        timeout: Duration,
    ) -> ApiResult<LlmHttpContext> {
        let mut terminal_default_env = self.terminal_default_env.clone();
        for entry in preset_env {
            upsert_terminal_env_entry(
                &mut terminal_default_env,
                entry.key.clone(),
                entry.value.clone(),
            );
        }
        let effective_env = terminal_core::build_tmux_child_env(
            &self.shell_env,
            &terminal_default_env,
            &self.proxy_env,
        );
        let client = build_llm_client_from_env(&effective_env, timeout)
            .map_err(|error| AppError::internal(format!("创建 LLM HTTP 客户端失败: {error}")))?;
        Ok(LlmHttpContext {
            client,
            environment_summary: llm_environment_summary(&effective_env),
        })
    }
}

fn merge_inherited_network_env(entries: &[(String, String)]) -> Vec<(String, String)> {
    let inherited = LLM_NETWORK_ENV_KEYS
        .iter()
        .filter_map(|key| {
            std::env::var(key)
                .ok()
                .map(|value| ((*key).to_string(), value))
        })
        .collect::<Vec<_>>();
    merge_network_env_entries(&inherited, entries)
}

fn merge_network_env_entries(
    inherited: &[(String, String)],
    entries: &[(String, String)],
) -> Vec<(String, String)> {
    let mut merged = crate::shell_env::filter_env_entries(inherited, &LLM_NETWORK_ENV_KEYS);
    for (key, value) in crate::shell_env::filter_env_entries(entries, &LLM_NETWORK_ENV_KEYS) {
        upsert_terminal_env_entry(&mut merged, key, value);
    }
    merged
}

pub(crate) fn build_llm_client_from_env(
    effective_env: &[(String, String)],
    timeout: Duration,
) -> anyhow::Result<reqwest::Client> {
    let all_proxy = terminal_env_nonempty_value(effective_env, &["ALL_PROXY", "all_proxy"]);
    let http_proxy =
        terminal_env_nonempty_value(effective_env, &["HTTP_PROXY", "http_proxy"]).or(all_proxy);
    let https_proxy =
        terminal_env_nonempty_value(effective_env, &["HTTPS_PROXY", "https_proxy"]).or(all_proxy);
    let no_proxy = terminal_env_raw_value(effective_env, &["NO_PROXY", "no_proxy"])
        .and_then(reqwest::NoProxy::from_string);
    let mut builder = reqwest::Client::builder().timeout(timeout).no_proxy();

    match (http_proxy, https_proxy) {
        (Some(http), Some(https)) if http == https => {
            let proxy = reqwest::Proxy::all(http)
                .with_context(|| format!("终端代理地址无效: {http}"))?
                .no_proxy(no_proxy);
            builder = builder.proxy(proxy);
        }
        (http, https) => {
            if let Some(http) = http {
                let proxy = reqwest::Proxy::http(http)
                    .with_context(|| format!("终端 HTTP_PROXY 无效: {http}"))?
                    .no_proxy(no_proxy.clone());
                builder = builder.proxy(proxy);
            }
            if let Some(https) = https {
                let proxy = reqwest::Proxy::https(https)
                    .with_context(|| format!("终端 HTTPS_PROXY 无效: {https}"))?
                    .no_proxy(no_proxy);
                builder = builder.proxy(proxy);
            }
        }
    }

    builder.build().context("创建终端环境 HTTP 客户端失败")
}

fn terminal_env_raw_value<'a>(entries: &'a [(String, String)], keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|key| {
        entries
            .iter()
            .rev()
            .find(|(entry_key, _)| entry_key == key)
            .map(|(_, value)| value.trim())
    })
}

fn terminal_env_nonempty_value<'a>(
    entries: &'a [(String, String)],
    keys: &[&str],
) -> Option<&'a str> {
    terminal_env_raw_value(entries, keys).filter(|value| !value.is_empty())
}

fn upsert_terminal_env_entry(entries: &mut Vec<(String, String)>, key: String, value: String) {
    if let Some(existing) = entries
        .iter_mut()
        .find(|(existing_key, _)| existing_key == &key)
    {
        existing.1 = value;
    } else {
        entries.push((key, value));
    }
}

fn llm_environment_summary(effective_env: &[(String, String)]) -> String {
    let state = |keys: &[&str]| {
        if terminal_env_nonempty_value(effective_env, keys).is_some() {
            "已配置"
        } else {
            "未配置"
        }
    };
    let no_proxy = terminal_env_raw_value(effective_env, &["NO_PROXY", "no_proxy"])
        .map(|value| {
            let mut summary = value.chars().take(240).collect::<String>();
            if value.chars().count() > 240 {
                summary.push_str("...");
            }
            summary
        })
        .unwrap_or_else(|| "未配置".to_string());
    format!(
        "与新终端一致；HTTP_PROXY={}，HTTPS_PROXY={}，ALL_PROXY={}，NO_PROXY={no_proxy}",
        state(&["HTTP_PROXY", "http_proxy"]),
        state(&["HTTPS_PROXY", "https_proxy"]),
        state(&["ALL_PROXY", "all_proxy"]),
    )
}

#[cfg(test)]
mod tests {
    use super::merge_network_env_entries;

    #[test]
    fn inherited_network_env_keeps_no_proxy_and_applies_shell_override() {
        let merged = merge_network_env_entries(
            &[
                ("NO_PROXY".to_string(), "127.0.0.1,192.168.3.2".to_string()),
                ("HTTP_PROXY".to_string(), "http://service-proxy:7890".to_string()),
                ("IGNORED".to_string(), "service".to_string()),
            ],
            &[
                ("HTTP_PROXY".to_string(), "http://shell-proxy:17890".to_string()),
                ("PATH".to_string(), "/usr/bin".to_string()),
            ],
        );

        assert_eq!(
            merged,
            vec![
                ("NO_PROXY".to_string(), "127.0.0.1,192.168.3.2".to_string(),),
                ("HTTP_PROXY".to_string(), "http://shell-proxy:17890".to_string(),),
            ]
        );
    }
}
