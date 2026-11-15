//! Read native credentials without applying presets or changing a running agent.
use std::{collections::BTreeMap, io::Read, path::Path};

use axum::{
    Json,
    extract::{Path as AxumPath, State},
    http::header,
    response::IntoResponse,
    routing::get,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};

use crate::{ApiResult, AppError, AppState};

const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

pub(super) fn routes() -> axum::Router<AppState> {
    axum::Router::new().route("/api/auth/native-login/{kind}", get(inspect).post(backup))
}

fn paths(kind: &str) -> Result<&'static [&'static str], String> {
    match kind {
        "oauth" | "api" => Ok(&[".codex/auth.json", ".codex/config.toml"]),
        "claude" => Ok(&[".claude/.credentials.json", ".claude/settings.json"]),
        "codebuddy" => Ok(&[".codebuddy/settings.json"]),
        "devin" => Ok(&[
            ".local/share/devin/credentials.toml",
            ".config/devin/config.json",
        ]),
        "agy" => Ok(&[
            ".gemini/antigravity-cli/antigravity-oauth-token",
            ".gemini/antigravity-cli/settings.json",
        ]),
        "opencode" => Ok(&[
            ".local/share/opencode/auth.json",
            ".config/opencode/opencode.json",
        ]),
        "codearts" => Ok(&[
            ".codeartsdoer/cli-data/auth.json",
            ".codeartsdoer/cli-data/settings.json",
            ".codeartsdoer/codearts_cli.json",
            ".codeartsdoer/codearts_cli.jsonc",
        ]),
        "qoder" => Ok(&[
            ".qoder/.auth/user",
            ".qoder/.auth/machine_id",
            ".qoder/settings.json",
            ".qoder-cn/.auth/user",
            ".qoder-cn/.auth/machine_id",
            ".qoder-cn/settings.json",
        ]),
        _ => Err("不支持的智能体类型。".into()),
    }
}

fn nonempty(value: &Value, pointer: &str) -> bool {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty())
}

fn json_file(files: &BTreeMap<String, Vec<u8>>, name: &str) -> Result<Value, String> {
    match files.get(name) {
        Some(bytes) => serde_json::from_slice(bytes).map_err(|_| format!("{name} JSON 格式无效。")),
        None => Ok(Value::Null),
    }
}

fn has_credentials(kind: &str, files: &BTreeMap<String, Vec<u8>>) -> Result<bool, String> {
    Ok(match kind {
        "oauth" => nonempty(&json_file(files, ".codex/auth.json")?, "/tokens/access_token"),
        "api" => nonempty(&json_file(files, ".codex/auth.json")?, "/OPENAI_API_KEY"),
        "claude" => {
            let oauth = json_file(files, ".claude/.credentials.json")?;
            let settings = json_file(files, ".claude/settings.json")?;
            nonempty(&oauth, "/claudeAiOauth/accessToken")
                || nonempty(&settings, "/env/ANTHROPIC_AUTH_TOKEN")
                || nonempty(&settings, "/env/ANTHROPIC_API_KEY")
        }
        "codebuddy" => {
            nonempty(&json_file(files, ".codebuddy/settings.json")?, "/env/CODEBUDDY_AUTH_TOKEN")
        }
        "devin" => match files.get(".local/share/devin/credentials.toml") {
            Some(bytes) => {
                let value = std::str::from_utf8(bytes)
                    .map_err(|_| "Devin 凭据不是 UTF-8。")?
                    .parse::<toml_edit::DocumentMut>()
                    .map_err(|_| "Devin 凭据 TOML 格式无效。")?;
                value
                    .get("windsurf_api_key")
                    .and_then(toml_edit::Item::as_str)
                    .is_some_and(|s| !s.trim().is_empty())
            }
            None => false,
        },
        "agy" => {
            let value = json_file(files, ".gemini/antigravity-cli/antigravity-oauth-token")?;
            [
                "/access_token",
                "/token/access_token",
                "/refresh_token",
                "/token/refresh_token",
                "/refreshToken",
                "/token/refreshToken",
                "/token",
            ]
            .into_iter()
            .any(|pointer| nonempty(&value, pointer))
        }
        "opencode" | "codearts" => json_file(
            files,
            if kind == "codearts" {
                ".codeartsdoer/cli-data/auth.json"
            } else {
                ".local/share/opencode/auth.json"
            },
        )?
        .as_object()
        .is_some_and(|providers| {
            providers
                .values()
                .any(|auth| match auth.get("type").and_then(Value::as_str) {
                    Some("oauth") => nonempty(auth, "/access") && nonempty(auth, "/refresh"),
                    Some("api") => nonempty(auth, "/key"),
                    _ => false,
                })
        }),
        "qoder" => {
            let has_intl = files
                .get(".qoder/.auth/user")
                .is_some_and(|b| !b.is_empty())
                && files
                    .get(".qoder/.auth/machine_id")
                    .is_some_and(|b| !b.is_empty());
            let has_cn = files
                .get(".qoder-cn/.auth/user")
                .is_some_and(|b| !b.is_empty())
                && files
                    .get(".qoder-cn/.auth/machine_id")
                    .is_some_and(|b| !b.is_empty());
            has_intl || has_cn
        }
        _ => return Err("不支持的智能体类型。".into()),
    })
}

fn read_native(home: &Path, kind: &str) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let mut files = BTreeMap::new();
    for relative in paths(kind)? {
        let path = home.join(relative);
        match std::fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return Err(format!("{relative} 不是普通文件。")),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(format!("无法检查 {relative}。")),
        }
        let file = match std::fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(format!("无法读取 {relative}。")),
        };
        if !file
            .metadata()
            .map_err(|_| format!("无法检查 {relative}。"))?
            .is_file()
        {
            return Err(format!("{relative} 不是普通文件。"));
        }
        let mut bytes = Vec::new();
        file.take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| format!("无法读取 {relative}。"))?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err(format!("{relative} 超过 2 MiB，拒绝备份。"));
        }
        files.insert((*relative).into(), bytes);
    }
    Ok(files)
}

// Only explicitly selected identity fields leave the inspection endpoint.
// Never return the provider object: it also contains complete login secrets.
fn codearts_accounts(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<Value>, String> {
    let auth = json_file(files, ".codeartsdoer/cli-data/auth.json")?;
    Ok(auth
        .as_object()
        .into_iter()
        .flatten()
        .filter_map(|(provider, auth)| {
            let auth_type = auth.get("type").and_then(Value::as_str)?;
            let valid = match auth_type {
                "oauth" => nonempty(auth, "/access") && nonempty(auth, "/refresh"),
                "api" => nonempty(auth, "/key"),
                _ => false,
            };
            valid.then(|| {
                json!({
                    "provider": provider,
                    "account_id": auth.get("accountId").and_then(Value::as_str).unwrap_or_default(),
                    "auth_type": auth_type,
                })
            })
        })
        .collect())
}

async fn native_files(
    state: &AppState,
    kind: String,
) -> ApiResult<(String, BTreeMap<String, Vec<u8>>)> {
    let home = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|e| AppError::internal(e.to_string()))?
        .home;
    tokio::task::spawn_blocking(move || {
        let files = read_native(&home, &kind)?;
        Ok::<_, String>((kind, files))
    })
    .await
    .map_err(|_| AppError::internal("读取智能体登录任务失败。"))?
    .map_err(AppError::bad_request)
}

fn qoder_account(home: &Path) -> Option<Value> {
    for (config_dir_rel, binary) in [
        (".qoder", "qodercli"),
        (".qoder-cn", "qoderclicn"),
    ] {
        let base = home.join(config_dir_rel);
        if !base.join(".auth/user").is_file() || !base.join(".auth/machine_id").is_file() {
            continue;
        }
        let Ok(output) = std::process::Command::new(binary)
            .arg("--config-dir")
            .arg(&base)
            .args(["status", "-o", "json"])
            .output()
        else {
            continue;
        };
        if output.status.success() {
            if let Ok(value) = serde_json::from_slice::<Value>(&output.stdout) {
                let username = value.get("username").and_then(Value::as_str).unwrap_or_default();
                let email = value.get("email").and_then(Value::as_str).unwrap_or_default();
                if !username.is_empty() || !email.is_empty() {
                    return Some(json!({
                        "name": username,
                        "email": email,
                    }));
                }
            }
        }
    }
    None
}

async fn inspect(
    State(state): State<AppState>,
    AxumPath(kind): AxumPath<String>,
) -> ApiResult<impl IntoResponse> {
    let (kind, files) = native_files(&state, kind).await?;
    let available = has_credentials(&kind, &files).map_err(AppError::bad_request)?;
    let mut result = json!({
        "kind": kind, "available": available,
        "files": files.keys().collect::<Vec<_>>(),
        "note": "本机凭据检测不代表服务端登录仍有效；备份不会切换或修改当前账号。",
    });
    if kind == "codearts" {
        result["accounts"] = json!(codearts_accounts(&files).map_err(AppError::bad_request)?);
    } else if kind == "agy" {
        let detected = state.agy_manager.detect_current();
        if detected.get("logged_in").and_then(Value::as_bool) == Some(true)
            || detected.get("matched").and_then(Value::as_bool) == Some(true)
        {
            result["account"] = json!({
                "id": detected.get("id").and_then(Value::as_str).unwrap_or_default(),
                "name": detected.get("name").and_then(Value::as_str).unwrap_or_default(),
                "email": detected.get("email").and_then(Value::as_str).unwrap_or_default(),
                "imported": detected.get("imported").and_then(Value::as_bool).unwrap_or(false),
            });
        }
    } else if kind == "qoder" {
        let home = state
            .workspace_settings
            .terminal_user_profile()
            .map(|p| p.home)
            .unwrap_or_default();
        let account = tokio::task::spawn_blocking(move || qoder_account(&home))
            .await
            .ok()
            .flatten();
        if let Some(account) = account {
            result["account"] = account;
        }
    }
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(result)))
}

async fn backup(
    State(state): State<AppState>,
    AxumPath(kind): AxumPath<String>,
) -> ApiResult<impl IntoResponse> {
    let (kind, files) = native_files(&state, kind).await?;
    if !has_credentials(&kind, &files).map_err(AppError::bad_request)? {
        return Err(AppError::bad_request(
            "未检测到该智能体的本机登录凭据，请先在原生客户端登录。",
        ));
    }
    let encoded: BTreeMap<_, _> = files
        .into_iter()
        .map(|(path, bytes)| (path, STANDARD.encode(bytes)))
        .collect();
    Ok((
        [
            (header::CACHE_CONTROL, "no-store"),
            (header::CONTENT_DISPOSITION, "attachment; filename=webclx-native-login.json"),
        ],
        Json(json!({
            "format": "webclx-native-login", "version": 1, "kind": kind,
            "created_at": terminal_core::current_timestamp_millis(),
            "encoding": "base64", "files": encoded,
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(path: &str, value: Value) -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([(path.into(), serde_json::to_vec(&value).unwrap())])
    }

    #[test]
    fn native_login_requires_real_credentials_not_settings_or_saved_accounts() {
        for kind in [
            "oauth",
            "api",
            "claude",
            "codebuddy",
            "devin",
            "agy",
            "opencode",
            "codearts",
            "qoder",
        ] {
            assert!(!has_credentials(kind, &BTreeMap::new()).unwrap());
            assert!(!paths(kind).unwrap().is_empty());
        }
        let codex = sample(".codex/auth.json", json!({"tokens":{"access_token":"test-oauth"}}));
        assert!(has_credentials("oauth", &codex).unwrap());
        assert!(!has_credentials("api", &codex).unwrap());
        let claude = sample(
            ".claude/.credentials.json",
            json!({"claudeAiOauth":{"accessToken":"test-claude"}}),
        );
        assert!(has_credentials("claude", &claude).unwrap());
        let opencode = sample(
            ".local/share/opencode/auth.json",
            json!({"test":{"type":"api", "key":"test-key"}}),
        );
        assert!(has_credentials("opencode", &opencode).unwrap());
        for (kind, path, value) in [
            ("api", ".codex/auth.json", json!({"OPENAI_API_KEY":"test-api"})),
            (
                "claude",
                ".claude/settings.json",
                json!({"env":{"ANTHROPIC_AUTH_TOKEN":"test-api"}}),
            ),
            (
                "codebuddy",
                ".codebuddy/settings.json",
                json!({"env":{"CODEBUDDY_AUTH_TOKEN":"test-buddy"}}),
            ),
            (
                "agy",
                ".gemini/antigravity-cli/antigravity-oauth-token",
                json!({"token":{"access_token":"test-agy"}}),
            ),
            (
                "agy",
                ".gemini/antigravity-cli/antigravity-oauth-token",
                json!({"token":{"refresh_token":"1//test-agy-refresh"}}),
            ),
        ] {
            assert!(has_credentials(kind, &sample(path, value)).unwrap(), "{kind}");
        }
        assert!(
            has_credentials(
                "devin",
                &BTreeMap::from([(
                    ".local/share/devin/credentials.toml".into(),
                    b"windsurf_api_key = 'test-devin'".to_vec()
                )])
            )
            .unwrap()
        );
        assert!(
            !has_credentials(
                "opencode",
                &sample(".local/share/opencode/auth.json", json!({"test":{}}))
            )
            .unwrap()
        );
        assert!(paths("../auth").is_err());
    }

    #[test]
    fn native_login_rejects_malformed_and_ignores_workspace_credentials() {
        let mut files = BTreeMap::from([(".codex/auth.json".into(), b"broken".to_vec())]);
        assert!(has_credentials("oauth", &files).is_err());
        files.clear();
        files.insert("auth.json".into(), b"not a native home credential".to_vec());
        assert!(!has_credentials("oauth", &files).unwrap());
    }

    #[test]
    fn codearts_credentials_are_native_and_independent_of_opencode() {
        let path = ".codeartsdoer/cli-data/auth.json";
        for credential in [
            json!({"type":"oauth", "access":"test-access", "refresh":"test-refresh"}),
            json!({"type":"api", "key":"test-key"}),
        ] {
            let files = sample(path, json!({"inferhub-provider": credential}));
            assert!(has_credentials("codearts", &files).unwrap());
            assert!(!has_credentials("opencode", &files).unwrap());
        }
        for credential in [
            json!({}),
            json!({"type":"oauth", "access":"test-access"}),
            json!({"type":"oauth", "access":" ", "refresh":"test-refresh"}),
            json!({"type":"api", "key":""}),
        ] {
            assert!(
                !has_credentials(
                    "codearts",
                    &sample(path, json!({"inferhub-provider": credential}))
                )
                .unwrap()
            );
        }
        assert!(
            !has_credentials(
                "codearts",
                &sample(
                    ".local/share/opencode/auth.json",
                    json!({"provider":{"type":"api", "key":"test-key"}})
                )
            )
            .unwrap()
        );
        assert!(
            has_credentials("codearts", &BTreeMap::from([(path.into(), b"broken".to_vec())]))
                .is_err()
        );
    }

    #[test]
    fn codearts_inspection_only_exposes_account_identity() {
        let files = sample(
            ".codeartsdoer/cli-data/auth.json",
            json!({
                "provider": {"type":"oauth", "accountId":"account-test", "access":"secret-access", "refresh":"secret-refresh", "enterpriseUrl":"secret-extra"},
                "api-provider": {"type":"api", "key":"secret-api"},
                "invalid": {"type":"oauth", "access":"incomplete"}
            }),
        );
        let accounts = codearts_accounts(&files).unwrap();
        assert_eq!(accounts.len(), 2);
        assert!(accounts.contains(
            &json!({"provider":"provider", "account_id":"account-test", "auth_type":"oauth"})
        ));
        assert!(
            accounts
                .contains(&json!({"provider":"api-provider", "account_id":"", "auth_type":"api"}))
        );
        assert!(!serde_json::to_string(&accounts).unwrap().contains("secret"));
        assert!(codearts_accounts(&BTreeMap::new()).unwrap().is_empty());
        assert!(
            codearts_accounts(&BTreeMap::from([(
                ".codeartsdoer/cli-data/auth.json".into(),
                b"broken".to_vec()
            )]))
            .is_err()
        );
    }

    #[test]
    fn codearts_backup_reads_fresh_native_files_only() {
        struct TestHome(std::path::PathBuf);
        impl Drop for TestHome {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let path = std::env::temp_dir().join(format!(
            "webclx-codearts-backup-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::create_dir(&path).unwrap();
        let home = TestHome(path);
        let auth_dir = home.0.join(".codeartsdoer/cli-data");
        std::fs::create_dir_all(&auth_dir).unwrap();
        let auth = auth_dir.join("auth.json");
        let first = br#"{"inferhub-provider":{"type":"api","key":"test-first"}}"#;
        let second = br#"{"inferhub-provider":{"type":"api","key":"test-second"}}"#;
        std::fs::write(&auth, first).unwrap();
        std::fs::write(auth_dir.join("opencode.db"), b"not a credential").unwrap();
        let files = read_native(&home.0, "codearts").unwrap();
        assert!(has_credentials("codearts", &files).unwrap());
        assert_eq!(files.len(), 1);
        assert_eq!(files[".codeartsdoer/cli-data/auth.json"], first);
        std::fs::write(&auth, second).unwrap();
        let fresh = read_native(&home.0, "codearts").unwrap();
        assert_eq!(fresh[".codeartsdoer/cli-data/auth.json"], second);
        assert_eq!(std::fs::read(&auth).unwrap(), second);
        std::fs::write(&auth, vec![b' '; MAX_FILE_BYTES as usize + 1]).unwrap();
        assert!(read_native(&home.0, "codearts").is_err());
    }

    #[test]
    fn qoder_credentials_require_user_and_machine_id() {
        let qoder_intl = BTreeMap::from([
            (".qoder/.auth/user".to_string(), b"intl-user".to_vec()),
            (".qoder/.auth/machine_id".to_string(), b"intl-machine".to_vec()),
        ]);
        assert!(has_credentials("qoder", &qoder_intl).unwrap());

        let qoder_cn = BTreeMap::from([
            (".qoder-cn/.auth/user".to_string(), b"cn-user".to_vec()),
            (".qoder-cn/.auth/machine_id".to_string(), b"cn-machine".to_vec()),
        ]);
        assert!(has_credentials("qoder", &qoder_cn).unwrap());

        let qoder_missing_machine = BTreeMap::from([
            (".qoder/.auth/user".to_string(), b"intl-user".to_vec()),
        ]);
        assert!(!has_credentials("qoder", &qoder_missing_machine).unwrap());

        let qoder_missing_user = BTreeMap::from([
            (".qoder/.auth/machine_id".to_string(), b"intl-machine".to_vec()),
        ]);
        assert!(!has_credentials("qoder", &qoder_missing_user).unwrap());

        let qoder_empty = BTreeMap::from([
            (".qoder/.auth/user".to_string(), b"".to_vec()),
            (".qoder/.auth/machine_id".to_string(), b"intl-machine".to_vec()),
        ]);
        assert!(!has_credentials("qoder", &qoder_empty).unwrap());
    }

    #[test]
    fn qoder_backup_reads_fresh_native_files_only() {
        struct TestHome(std::path::PathBuf);
        impl Drop for TestHome {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let path = std::env::temp_dir().join(format!(
            "webclx-qoder-backup-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::create_dir(&path).unwrap();
        let home = TestHome(path);
        let auth_dir = home.0.join(".qoder/.auth");
        std::fs::create_dir_all(&auth_dir).unwrap();
        let user_file = auth_dir.join("user");
        let machine_file = auth_dir.join("machine_id");
        std::fs::write(&user_file, b"test-user-token").unwrap();
        std::fs::write(&machine_file, b"test-machine-id").unwrap();
        let files = read_native(&home.0, "qoder").unwrap();
        assert!(has_credentials("qoder", &files).unwrap());
        assert_eq!(files.len(), 2);
        assert_eq!(files[".qoder/.auth/user"], b"test-user-token");
        assert_eq!(files[".qoder/.auth/machine_id"], b"test-machine-id");
    }
}
