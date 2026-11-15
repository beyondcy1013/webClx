//! Native CodeArts permission settings; never inject launch flags or environment.
use std::{fs, path::Path, sync::Mutex};

use anyhow::{Context, Result, bail};
use axum::{Json, extract::State, http::header, response::IntoResponse, routing::get};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{ApiResult, AppError, AppState};

const STATE: &str = ".codeartsdoer/cli-data/storage/permission/webclx-full-access.json";
const CONFIG: &str = ".codeartsdoer/cli-data/storage/permission/config.json";
const GLOBAL: &str = ".codeartsdoer/cli-data/storage/permission/global.json";
const SETTINGS: &str = ".codeartsdoer/cli-data/settings.json";
const EXTENSION: &str = ".codeartsdoer/codearts_cli-extension.jsonc";
const MAX_BYTES: u64 = 2 * 1024 * 1024;
// Only serialize the short read/modify/write transaction, never an Agent lifetime.
static WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Deserialize)]
struct Update {
    allow_all_permissions: bool,
}

#[derive(Clone, Serialize, Deserialize)]
struct Field {
    file: String,
    path: Vec<String>,
    present: bool,
    original: Value,
    applied: Value,
}

#[derive(Serialize, Deserialize)]
struct Saved {
    version: u8,
    enabled: bool,
    #[serde(default)]
    pending: bool,
    fields: Vec<Field>,
}

pub(super) fn routes() -> axum::Router<AppState> {
    axum::Router::new().route("/api/auth/codearts-options", get(inspect).put(update))
}

async fn inspect(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    options(state, None).await
}

async fn update(
    State(state): State<AppState>,
    Json(payload): Json<Update>,
) -> ApiResult<impl IntoResponse> {
    options(state, Some(payload.allow_all_permissions)).await
}

async fn options(state: AppState, enabled: Option<bool>) -> ApiResult<impl IntoResponse> {
    let home = state
        .workspace_settings
        .terminal_user_profile()
        .map_err(|e| AppError::internal(e.to_string()))?
        .home;
    let result = tokio::task::spawn_blocking(move || -> Result<Value> {
        let _lock = WRITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(enabled) = enabled {
            apply(&home, enabled)?;
        }
        status(&home)
    })
    .await
    .map_err(|_| AppError::internal("CodeArts 权限配置任务失败。"))?
    .map_err(|e| AppError::bad_request(format!("CodeArts 权限配置失败：{e:#}")))?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(result)))
}

fn read(home: &Path, file: &str, default: Value) -> Result<Value> {
    let path = home.join(file);
    if !crate::private_file::validate_optional_existing(&path)? {
        return Ok(default);
    }
    if fs::metadata(&path)?.len() > MAX_BYTES {
        bail!("{file} 超过 2 MiB");
    }
    // Files created here are JSON (also valid JSONC). Never overwrite an existing
    // configuration we cannot parse or silently fall back to an empty document.
    let raw = fs::read(&path)?;
    let raw = if file.ends_with(".jsonc") {
        jsonc_bytes(&raw)?
    } else {
        raw
    };
    serde_json::from_slice(&raw).with_context(|| format!("{file} 配置格式无效；原文件未覆盖"))
}

fn jsonc_bytes(raw: &[u8]) -> Result<Vec<u8>> {
    let mut out = raw.to_vec();
    let (mut index, mut quoted, mut escaped) = (0, false, false);
    while index < out.len() {
        let byte = out[index];
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else if byte == b'"' {
            quoted = true;
        } else if byte == b'/' && out.get(index + 1) == Some(&b'/') {
            while index < out.len() && out[index] != b'\n' {
                out[index] = b' ';
                index += 1;
            }
            continue;
        } else if byte == b'/' && out.get(index + 1) == Some(&b'*') {
            out[index] = b' ';
            out[index + 1] = b' ';
            index += 2;
            while index + 1 < out.len() && &out[index..index + 2] != b"*/" {
                out[index] = b' ';
                index += 1;
            }
            if index + 1 >= out.len() {
                bail!("JSONC 注释未结束，原文件未覆盖");
            }
            out[index] = b' ';
            out[index + 1] = b' ';
            index += 2;
            continue;
        }
        index += 1;
    }
    quoted = false;
    escaped = false;
    for index in 0..out.len() {
        let byte = out[index];
        if quoted {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
        } else if byte == b'"' {
            quoted = true;
        } else if byte == b','
            && out[index + 1..]
                .iter()
                .find(|b| !b.is_ascii_whitespace())
                .is_some_and(|b| matches!(b, b'}' | b']'))
        {
            out[index] = b' ';
        }
    }
    Ok(out)
}

fn write(home: &Path, file: &str, value: &Value) -> Result<()> {
    let path = home.join(file);
    fs::create_dir_all(path.parent().context("配置目录不存在")?)?;
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    crate::private_file::write_atomic(&path, &bytes).with_context(|| format!("写入 {file}"))
}

fn saved(home: &Path) -> Result<Option<Saved>> {
    let value = read(home, STATE, Value::Null)?;
    if value.is_null() {
        return Ok(None);
    }
    let saved: Saved = serde_json::from_value(value)?;
    if saved.version != 1 {
        bail!("不支持的 CodeArts 权限快照版本");
    }
    // The persisted snapshot is data, never an arbitrary file-write instruction.
    let expected = managed_fields();
    if (!saved.fields.is_empty() || saved.enabled) && saved.fields.len() != expected.len() {
        bail!("权限快照字段不完整");
    }
    for (field, (file, path, applied)) in saved.fields.iter().zip(expected) {
        if field.file != file || field.path != path || (file != GLOBAL && field.applied != applied)
        {
            bail!("权限快照包含非权限配置字段");
        }
        if file == GLOBAL {
            validate_rules(&field.original)?;
        }
    }
    Ok(Some(saved))
}

fn managed_fields() -> Vec<(&'static str, Vec<String>, Value)> {
    vec![
        (CONFIG, vec!["bash_mode".into()], json!("always_allow")),
        (
            SETTINGS,
            vec!["security".into(), "folderTrust".into(), "enabled".into()],
            json!(false),
        ),
        (EXTENSION, vec!["bash".into(), "mode".into()], json!("always_allow")),
        (
            EXTENSION,
            vec!["security".into(), "read".into(), "enabled".into()],
            json!(false),
        ),
        (GLOBAL, vec![], Value::Null),
    ]
}

fn validate_rules(value: &Value) -> Result<&Vec<Value>> {
    let rules = value.as_array().context("CodeArts 全局权限必须为数组")?;
    for rule in rules {
        if !rule.is_object()
            || !rule["permission"].is_string()
            || !rule["pattern"].is_string()
            || !matches!(rule["action"].as_str(), Some("allow" | "ask" | "deny"))
        {
            bail!("CodeArts 权限规则格式无效");
        }
    }
    Ok(rules)
}

fn universal(rule: &Value) -> bool {
    rule["permission"] == "*" && rule["pattern"] == "*" && rule["action"] == "allow"
}

fn update_rules(current: &mut Value, original: &Value, enabled: bool) -> Result<()> {
    validate_rules(current)?;
    let original = validate_rules(original)?;
    let rules = current.as_array_mut().unwrap();
    if enabled {
        // Merge the current native rules on every save; never replay a stale array.
        for rule in rules.iter_mut() {
            rule["action"] = json!("allow");
        }
        if !rules.iter().any(universal) {
            rules.push(json!({"permission":"*", "pattern":"*", "action":"allow"}));
        }
    } else {
        let inserted = !original.iter().any(universal);
        rules.retain(|rule| !(inserted && universal(rule) && rule.as_object().unwrap().len() == 3));
        for rule in rules.iter_mut() {
            if let Some(previous) = original.iter().rev().find(|old| {
                old["permission"] == rule["permission"] && old["pattern"] == rule["pattern"]
            }) && rule["action"] == "allow"
            {
                rule["action"] = previous["action"].clone();
            }
        }
    }
    Ok(())
}

fn field_value<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    let mut value = value;
    for key in path {
        value = value.get(key)?;
    }
    Some(value)
}

fn set_field(value: &mut Value, path: &[String], replacement: Option<Value>) -> Result<()> {
    if path.is_empty() {
        *value = replacement.unwrap_or_else(|| json!([]));
        return Ok(());
    }
    let mut object = value;
    for key in &path[..path.len() - 1] {
        object = object
            .as_object_mut()
            .context("权限配置父节点必须为对象")?
            .entry(key)
            .or_insert_with(|| json!({}));
    }
    let object = object.as_object_mut().context("权限配置节点必须为对象")?;
    let key = path.last().unwrap();
    if let Some(replacement) = replacement {
        object.insert(key.clone(), replacement);
    } else {
        object.remove(key);
    }
    Ok(())
}

fn snapshot(home: &Path) -> Result<Vec<Field>> {
    let mut fields = Vec::new();
    for (file, path, applied) in managed_fields()
        .into_iter()
        .filter(|(file, _, _)| *file != GLOBAL)
    {
        let value = read(home, file, json!({}))?;
        if !value.is_object() {
            bail!("{file} 顶层必须为对象");
        }
        let original = field_value(&value, &path).cloned();
        let mut probe = value.clone();
        set_field(&mut probe, &path, Some(applied.clone()))?;
        fields.push(Field {
            file: file.into(),
            path,
            present: original.is_some(),
            original: original.unwrap_or(Value::Null),
            applied,
        });
    }
    let original = read(home, GLOBAL, json!([]))?;
    let mut applied = original.clone();
    update_rules(&mut applied, &original, true)?;
    fields.push(Field {
        file: GLOBAL.into(),
        path: vec![],
        present: true,
        original,
        applied,
    });
    Ok(fields)
}

fn apply(home: &Path, enabled: bool) -> Result<()> {
    let previous = saved(home)?;
    let fields = match previous {
        Some(previous) if previous.enabled || previous.pending || !enabled => previous.fields,
        _ if enabled => snapshot(home)?,
        _ => Vec::new(),
    };
    // Persist restoration values before any native mutation. A partial write is
    // visible as applied=false and may be repaired without losing the baseline.
    let mut state = Saved {
        version: 1,
        enabled,
        pending: true,
        fields,
    };
    // Preflight every source before mutating any native configuration.
    let mut documents = std::collections::BTreeMap::new();
    for field in &state.fields {
        let default = if field.path.is_empty() {
            json!([])
        } else {
            json!({})
        };
        if !documents.contains_key(&field.file) {
            documents.insert(field.file.clone(), read(home, &field.file, default)?);
        }
        let value = documents.get_mut(&field.file).unwrap();
        if field.file == GLOBAL {
            update_rules(value, &field.original, enabled)?;
        } else {
            let replacement = if enabled {
                Some(field.applied.clone())
            } else {
                field.present.then(|| field.original.clone())
            };
            set_field(value, &field.path, replacement)?;
        }
    }
    write(home, STATE, &serde_json::to_value(&state)?)?;
    for (file, value) in documents {
        write(home, &file, &value)?;
    }
    state.pending = false;
    write(home, STATE, &serde_json::to_value(&state)?)?;
    Ok(())
}

fn status(home: &Path) -> Result<Value> {
    let state = saved(home)?;
    let enabled = state.as_ref().is_none_or(|s| s.enabled);
    let mut applied = state.as_ref().is_some_and(|s| !s.pending);
    if let Some(state) = &state {
        for field in &state.fields {
            let current = read(
                home,
                &field.file,
                if field.path.is_empty() {
                    json!([])
                } else {
                    json!({})
                },
            )?;
            if field.file == GLOBAL {
                let rules = validate_rules(&current)?;
                if enabled {
                    applied &=
                        rules.iter().all(|r| r["action"] == "allow") && rules.iter().any(universal);
                }
                continue;
            }
            let expected = if enabled {
                Some(&field.applied)
            } else {
                field.present.then_some(&field.original)
            };
            applied &= field_value(&current, &field.path) == expected;
        }
    }
    Ok(
        json!({"allowAllPermissions":enabled, "initialized":state.is_some(), "applied":applied,
        "restartRequired":true, "bashMode":read(home, CONFIG, json!({}))?.get("bash_mode"),
        "note":"配置已保存后，重新启动原生 CodeArts 会话可完整生效；不会终止当前终端。"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let home = std::env::temp_dir()
                .join(format!("webclx-codearts-options-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&home).unwrap();
            Self(home)
        }
        fn seed(&self) {
            write(&self.0, CONFIG, &json!({"bash_mode":"sandbox","keep":7})).unwrap();
            write(&self.0, SETTINGS, &json!({"security":{"folderTrust":{"enabled":true},"trustedFolders":["/project"]},"theme":"dark"})).unwrap();
            write(&self.0, EXTENSION, &json!({"bash":{"mode":"always_ask","whitelist":["git status"]},"security":{"read":{"enabled":true,"rules":{"secret":"deny"}}},"url":"https://example.invalid/a//b"})).unwrap();
            write(&self.0, GLOBAL, &json!([{"permission":"bash","pattern":"rm *","action":"ask"},{"permission":"dotfile","pattern":"*","action":"deny"}])).unwrap();
        }
        fn get(&self, file: &str) -> Value {
            read(&self.0, file, Value::Null).unwrap()
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn codearts_options_default_on_read_does_not_mutate() {
        let f = Fixture::new();
        let result = status(&f.0).unwrap();
        assert_eq!(result["allowAllPermissions"], true);
        assert_eq!(result["initialized"], false);
        assert_eq!(result["applied"], false);
        assert_eq!(fs::read_dir(&f.0).unwrap().count(), 0);
    }

    #[test]
    fn codearts_options_native_enable_disable_preserves_unrelated_fields() {
        let f = Fixture::new();
        f.seed();
        let originals: Vec<_> = [CONFIG, GLOBAL, SETTINGS, EXTENSION]
            .map(|file| (file, f.get(file)))
            .into();
        let auth = f.0.join(".codeartsdoer/cli-data/auth.json");
        fs::write(&auth, b"credential-fixture-do-not-touch").unwrap();
        apply(&f.0, true).unwrap();
        assert_eq!(status(&f.0).unwrap()["applied"], true);
        assert_eq!(f.get(CONFIG)["bash_mode"], "always_allow");
        assert_eq!(f.get(EXTENSION)["bash"]["mode"], "always_allow");
        assert_eq!(f.get(EXTENSION)["security"]["read"]["enabled"], false);
        assert_eq!(f.get(SETTINGS)["security"]["folderTrust"]["enabled"], false);
        apply(&f.0, true).unwrap();
        apply(&f.0, false).unwrap();
        for (file, original) in originals {
            assert_eq!(f.get(file), original, "{file}");
        }
        assert_eq!(status(&f.0).unwrap()["applied"], true);
        assert_eq!(fs::read(auth).unwrap(), b"credential-fixture-do-not-touch");
    }

    #[test]
    fn codearts_options_merges_new_rules_without_losing_original_baseline() {
        let f = Fixture::new();
        f.seed();
        apply(&f.0, true).unwrap();
        let mut current = f.get(GLOBAL);
        current
            .as_array_mut()
            .unwrap()
            .push(json!({"permission":"plugin","pattern":"new","action":"ask","extra":1}));
        write(&f.0, GLOBAL, &current).unwrap();
        assert_eq!(status(&f.0).unwrap()["applied"], false);
        apply(&f.0, true).unwrap();
        let current = f.get(GLOBAL);
        assert_eq!(
            current
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| universal(r))
                .count(),
            1
        );
        assert_eq!(current.as_array().unwrap().last().unwrap()["extra"], 1);
        apply(&f.0, false).unwrap();
        let current = f.get(GLOBAL);
        assert_eq!(current[0]["action"], "ask");
        assert_eq!(current[1]["action"], "deny");
        assert_eq!(current[2]["permission"], "plugin");
        assert!(!current.as_array().unwrap().iter().any(universal));
    }

    #[test]
    fn codearts_options_reenable_takes_fresh_baseline_and_keeps_original_wildcard() {
        let f = Fixture::new();
        f.seed();
        let rules = json!([{"permission":"*","pattern":"*","action":"allow"}]);
        write(&f.0, GLOBAL, &rules).unwrap();
        apply(&f.0, true).unwrap();
        apply(&f.0, false).unwrap();
        assert_eq!(f.get(GLOBAL), rules);
        write(&f.0, CONFIG, &json!({"bash_mode":"always_ask"})).unwrap();
        apply(&f.0, true).unwrap();
        apply(&f.0, false).unwrap();
        assert_eq!(f.get(CONFIG)["bash_mode"], "always_ask");
    }

    #[test]
    fn codearts_options_jsonc_comments_trailing_commas_and_string_literals() {
        let raw = br#"{ // comment
          "url":"https://a.invalid/x//y", /* block */ "quote":"\"//", "items":[1,2,],
        }"#;
        let value: Value = serde_json::from_slice(&jsonc_bytes(raw).unwrap()).unwrap();
        assert_eq!(value["url"], "https://a.invalid/x//y");
        assert_eq!(value["items"], json!([1, 2]));
        assert!(jsonc_bytes(b"{/* unfinished").is_err());
        let f = Fixture::new();
        f.seed();
        fs::write(f.0.join(EXTENSION), raw).unwrap();
        apply(&f.0, true).unwrap();
        assert_eq!(f.get(EXTENSION)["items"], json!([1, 2]));
    }

    #[test]
    fn codearts_options_invalid_native_config_is_not_overwritten() {
        for invalid in [b"broken".as_slice(), b"{\"bash\":false}".as_slice()] {
            let f = Fixture::new();
            f.seed();
            fs::write(f.0.join(EXTENSION), invalid).unwrap();
            let before = fs::read(f.0.join(CONFIG)).unwrap();
            assert!(apply(&f.0, true).is_err());
            assert_eq!(fs::read(f.0.join(CONFIG)).unwrap(), before);
            assert!(!f.0.join(STATE).exists());
            assert_eq!(fs::read(f.0.join(EXTENSION)).unwrap(), invalid);
        }
    }

    #[test]
    fn codearts_options_snapshot_cannot_target_other_configuration() {
        let f = Fixture::new();
        f.seed();
        apply(&f.0, true).unwrap();
        let mut state = f.get(STATE);
        state["fields"][0]["path"] = json!(["unrelated"]);
        write(&f.0, STATE, &state).unwrap();
        let before = f.get(CONFIG);
        assert!(apply(&f.0, false).is_err());
        assert_eq!(f.get(CONFIG), before);
    }

    #[test]
    fn codearts_options_pending_transaction_keeps_baseline_on_repair() {
        let f = Fixture::new();
        f.seed();
        apply(&f.0, true).unwrap();
        let mut state = f.get(STATE);
        state["pending"] = json!(true);
        state["enabled"] = json!(false);
        write(&f.0, STATE, &state).unwrap();
        assert_eq!(status(&f.0).unwrap()["applied"], false);
        apply(&f.0, true).unwrap();
        apply(&f.0, false).unwrap();
        assert_eq!(f.get(CONFIG)["bash_mode"], "sandbox");
    }

    #[test]
    fn codearts_options_absent_fields_restore_without_removing_other_settings() {
        let f = Fixture::new();
        apply(&f.0, true).unwrap();
        let mut current = f.get(EXTENSION);
        current["new-setting"] = json!(17);
        write(&f.0, EXTENSION, &current).unwrap();
        apply(&f.0, false).unwrap();
        assert_eq!(f.get(EXTENSION)["new-setting"], 17);
        assert!(f.get(CONFIG).get("bash_mode").is_none());
        assert!(f.get(EXTENSION)["bash"].get("mode").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn codearts_options_private_files_and_symlink_rejection() {
        use std::os::unix::{fs::PermissionsExt, fs::symlink};
        let f = Fixture::new();
        f.seed();
        let outside = f.0.join("outside");
        fs::write(&outside, b"{}").unwrap();
        fs::remove_file(f.0.join(CONFIG)).unwrap();
        symlink(&outside, f.0.join(CONFIG)).unwrap();
        assert!(apply(&f.0, true).is_err());
        assert_eq!(fs::read(&outside).unwrap(), b"{}");
        fs::remove_file(f.0.join(CONFIG)).unwrap();
        apply(&f.0, true).unwrap();
        for file in [STATE, CONFIG, GLOBAL, SETTINGS, EXTENSION] {
            assert_eq!(fs::metadata(f.0.join(file)).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
}
