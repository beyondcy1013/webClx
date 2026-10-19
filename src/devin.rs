//! Devin CLI 账号管理与选项配置：
//! - 读取配置中的账号信息（~/.local/share/devin/credentials.toml, ~/.config/devin/config.json, ~/.Ddevin/in_use.json）
//! - 多账号切换与持久化（webclx-devin-accounts.json）
//! - 导入 / 导出账号（剪贴板或 JSON 格式）
//! - 全局 Bypass 选项开关（设置 permissions.allow: ["*"], skip_workspace_trust: true, mode: bypass）

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, RwLock},
};

use axum::{
    Json,
    extract::{Path as AxumPath, State},
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use terminal_core::current_timestamp_millis;
use tracing::info;

use crate::{ApiResult, AppError, AppState};

fn now_ms() -> i64 {
    current_timestamp_millis() as i64
}

pub(crate) const DEVIN_ACCOUNTS_FILE: &str = "webclx-devin-accounts.json";

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DevinAccount {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub org_id: String,
    #[serde(default)]
    pub user_id: String,
    #[serde(default)]
    pub plan: String,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub saved_at: i64,
}

#[derive(Debug, Deserialize)]
pub struct AddDevinAccountRequest {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub token: String,
    #[serde(default)]
    pub org_id: String,
    #[serde(default)]
    pub plan: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateDevinOptionsRequest {
    pub bypass: bool,
}

#[derive(Debug, Deserialize)]
pub struct ExportDevinAccountsRequest {
    #[serde(default)]
    pub ids: Vec<String>,
}

#[derive(Clone)]
pub struct DevinManager {
    file: PathBuf,
    home: PathBuf,
    accounts: Arc<RwLock<Vec<DevinAccount>>>,
}

impl DevinManager {
    pub fn load(app_dir: PathBuf, home: PathBuf) -> Self {
        let file = app_dir.join(DEVIN_ACCOUNTS_FILE);
        let mut accounts: Vec<DevinAccount> = fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();

        let current_token = read_current_token(&home);
        // 如果当前有登录凭据，但账号库为空，自动将当前登录加入账号库
        if let Some(ref token) = current_token {
            let exists = accounts.iter().any(|a| a.token == *token);
            if !exists {
                let probed = probe_local_account(&home);
                let email = if !probed.email.is_empty() {
                    probed.email
                } else {
                    read_in_use_email(&home).unwrap_or_else(|| "current-account@devin".to_string())
                };
                let org_id = if !probed.org_id.is_empty() {
                    probed.org_id
                } else {
                    read_devin_org_id(&home).unwrap_or_default()
                };
                let id = sanitize_account_id(&email);
                accounts.push(DevinAccount {
                    id,
                    name: if !probed.name.is_empty() {
                        probed.name
                    } else {
                        email.clone()
                    },
                    email,
                    token: token.clone(),
                    org_id,
                    user_id: probed.user_id,
                    plan: if !probed.plan.is_empty() {
                        probed.plan
                    } else {
                        "Devin Pro".to_string()
                    },
                    active: true,
                    saved_at: now_ms(),
                });
            }
        }

        // 同步 active 状态
        for a in accounts.iter_mut() {
            a.active = current_token.as_ref().map_or(false, |tok| tok == &a.token);
        }

        let mgr = Self {
            file,
            home,
            accounts: Arc::new(RwLock::new(accounts)),
        };
        let _ = mgr.persist_internal();
        mgr
    }

    fn persist_internal(&self) -> Result<(), String> {
        let list = self.accounts.read().map_err(|e| e.to_string())?;
        crate::atomic_file::write_atomic(
            &self.file,
            &serde_json::to_vec_pretty(&*list).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }

    pub fn list_accounts(&self) -> Vec<DevinAccount> {
        let current_token = read_current_token(&self.home);
        let mut list = self.accounts.read().unwrap().clone();
        for a in list.iter_mut() {
            a.active = current_token.as_ref().map_or(false, |tok| tok == &a.token);
        }
        list
    }

    pub fn switch_account(&self, account_id: &str) -> Result<DevinAccount, String> {
        let mut accounts = self.accounts.write().map_err(|e| e.to_string())?;
        let target_idx = accounts
            .iter()
            .position(|a| a.id == account_id)
            .ok_or_else(|| format!("找不到 ID 为 {account_id} 的 Devin 账号"))?;

        let target = accounts[target_idx].clone();
        if target.token.trim().is_empty() {
            return Err("该账号没有可用的 Session Token".to_string());
        }

        // 1. 写入 credentials.toml
        write_current_token(&self.home, &target.token)?;

        // 2. 如果账号记录包含 org_id，写入 ~/.config/devin/config.json
        if !target.org_id.trim().is_empty() {
            let _ = write_devin_org_id(&self.home, &target.org_id);
        }

        // 3. 更新 ~/.Ddevin/in_use.json
        if !target.email.trim().is_empty() {
            let _ = write_in_use_email(&self.home, &target.email);
        }

        // 4. 更新内存中的 active 状态
        for a in accounts.iter_mut() {
            a.active = a.id == account_id;
        }

        let switched = accounts[target_idx].clone();
        drop(accounts);
        self.persist_internal()?;

        info!("Switched Devin account to {}", switched.name);
        Ok(switched)
    }

    pub fn add_account(&self, req: AddDevinAccountRequest) -> Result<DevinAccount, String> {
        let token = req.token.trim().to_string();
        if token.is_empty() {
            return Err("Session Token (windsurf_api_key) 不能为空".to_string());
        }

        let email = if !req.email.trim().is_empty() {
            req.email.trim().to_string()
        } else {
            format!("devin-user-{}", &token[token.len().saturating_sub(8)..])
        };

        let name = if !req.name.trim().is_empty() {
            req.name.trim().to_string()
        } else {
            email.clone()
        };

        let mut accounts = self.accounts.write().map_err(|e| e.to_string())?;
        let id = sanitize_account_id(&email);

        let current_token = read_current_token(&self.home);
        let active = current_token.as_ref().map_or(false, |tok| tok == &token);

        let account = DevinAccount {
            id: id.clone(),
            name,
            email,
            token,
            org_id: req.org_id.trim().to_string(),
            user_id: String::new(),
            plan: if !req.plan.trim().is_empty() {
                req.plan.trim().to_string()
            } else {
                "Devin Pro".to_string()
            },
            active,
            saved_at: now_ms(),
        };

        if let Some(pos) = accounts
            .iter()
            .position(|a| a.id == id || a.token == account.token)
        {
            accounts[pos] = account.clone();
        } else {
            accounts.push(account.clone());
        }

        drop(accounts);
        self.persist_internal()?;
        Ok(account)
    }

    pub fn delete_account(&self, account_id: &str) -> Result<(), String> {
        let mut accounts = self.accounts.write().map_err(|e| e.to_string())?;
        let before_len = accounts.len();
        accounts.retain(|a| a.id != account_id);
        if accounts.len() == before_len {
            return Err(format!("找不到 ID 为 {account_id} 的 Devin 账号"));
        }
        drop(accounts);
        self.persist_internal()
    }

    pub fn import_current(&self) -> Result<DevinAccount, String> {
        let token = read_current_token(&self.home).ok_or_else(|| {
            "本地尚未登录 Devin（未找到 credentials.toml 中的 windsurf_api_key）".to_string()
        })?;

        let probed = probe_local_account(&self.home);
        let email = if !probed.email.is_empty() {
            probed.email
        } else {
            read_in_use_email(&self.home).unwrap_or_else(|| "current-account@devin".to_string())
        };

        let org_id = if !probed.org_id.is_empty() {
            probed.org_id
        } else {
            read_devin_org_id(&self.home).unwrap_or_default()
        };

        let account = DevinAccount {
            id: sanitize_account_id(&email),
            name: if !probed.name.is_empty() {
                probed.name
            } else {
                email.clone()
            },
            email,
            token,
            org_id,
            user_id: probed.user_id,
            plan: if !probed.plan.is_empty() {
                probed.plan
            } else {
                "Devin Pro".to_string()
            },
            active: true,
            saved_at: now_ms(),
        };

        let mut accounts = self.accounts.write().map_err(|e| e.to_string())?;
        if let Some(pos) = accounts
            .iter()
            .position(|a| a.id == account.id || a.token == account.token)
        {
            accounts[pos] = account.clone();
        } else {
            accounts.push(account.clone());
        }
        for a in accounts.iter_mut() {
            a.active = a.id == account.id;
        }

        drop(accounts);
        self.persist_internal()?;
        Ok(account)
    }

    pub fn detect_current(&self) -> Result<Value, String> {
        let mut accounts = self.accounts.write().map_err(|e| e.to_string())?;
        if let Some(disk_accounts) = fs::read(&self.file)
            .ok()
            .and_then(|b| serde_json::from_slice::<Vec<DevinAccount>>(&b).ok())
        {
            *accounts = disk_accounts;
        }

        let current_token = read_current_token(&self.home);
        if let Some(ref token) = current_token {
            let matched_pos = accounts.iter().position(|a| &a.token == token);
            if let Some(pos) = matched_pos {
                for (i, a) in accounts.iter_mut().enumerate() {
                    a.active = i == pos;
                }
                let active_acc = accounts[pos].clone();
                drop(accounts);
                self.persist_internal()?;
                Ok(json!({
                    "ok": true,
                    "matched": true,
                    "account": active_acc,
                }))
            } else {
                drop(accounts);
                let imported = self.import_current()?;
                Ok(json!({
                    "ok": true,
                    "matched": true,
                    "imported": true,
                    "account": imported,
                }))
            }
        } else {
            for a in accounts.iter_mut() {
                a.active = false;
            }
            drop(accounts);
            self.persist_internal()?;
            Ok(json!({
                "ok": true,
                "matched": false,
                "account": null,
            }))
        }
    }

    pub fn import_accounts(&self, body: &Value) -> Result<Value, String> {
        let raw_items = collect_devin_import_items(body)?;
        if raw_items.is_empty() {
            return Err("没有可导入的有效 Devin 账号".to_string());
        }

        let mut accounts = self.accounts.write().map_err(|e| e.to_string())?;
        let current_token = read_current_token(&self.home);
        let mut imported = 0usize;
        let mut updated = 0usize;

        for item in raw_items {
            let token = item.token.trim().to_string();
            if token.is_empty() {
                continue;
            }
            let id = sanitize_account_id(&item.email);
            let active = current_token.as_ref().map_or(false, |tok| tok == &token);

            let new_acct = DevinAccount {
                id: id.clone(),
                name: if !item.name.is_empty() {
                    item.name
                } else {
                    item.email.clone()
                },
                email: item.email,
                token,
                org_id: item.org_id,
                user_id: item.user_id,
                plan: if !item.plan.is_empty() {
                    item.plan
                } else {
                    "Devin Pro".to_string()
                },
                active,
                saved_at: now_ms(),
            };

            if let Some(pos) = accounts
                .iter()
                .position(|a| a.id == id || a.token == new_acct.token)
            {
                accounts[pos] = new_acct;
                updated += 1;
            } else {
                accounts.push(new_acct);
                imported += 1;
            }
        }

        drop(accounts);
        self.persist_internal()?;
        Ok(json!({
            "ok": true,
            "imported": imported,
            "updated": updated
        }))
    }

    pub fn export_accounts(&self, ids: Option<Vec<String>>) -> Result<Value, String> {
        let filter_set: Option<HashSet<String>> = ids.map(|list| list.into_iter().collect());
        let accounts = self.accounts.read().map_err(|e| e.to_string())?;
        let selected: Vec<DevinAccount> = accounts
            .iter()
            .filter(|a| filter_set.as_ref().map_or(true, |set| set.contains(&a.id)))
            .cloned()
            .collect();

        Ok(json!({
            "format": "webclx-account-presets-bundle",
            "version": 1,
            "section": "devin_presets",
            "accounts": selected
        }))
    }

    pub fn get_options(&self) -> Value {
        let bypass = read_bypass_status(&self.home);
        json!({
            "bypass": bypass
        })
    }

    pub fn set_options(&self, bypass: bool) -> Result<Value, String> {
        write_bypass_status(&self.home, bypass)?;
        Ok(json!({
            "ok": true,
            "bypass": bypass
        }))
    }
}

/// 从本地环境检索 Devin 当前账号的元数据
fn probe_local_account(home: &Path) -> DevinAccount {
    let mut probed = DevinAccount::default();
    // 优先尝试执行 devin auth status（短命令，限时收集输出）
    let devin_auth_status = |program: PathBuf| {
        let mut command = Command::new(program);
        command.args(["auth", "status"]);
        crate::process_exec::bounded_output(command, std::time::Duration::from_secs(15), 256 * 1024)
    };
    let output = devin_auth_status(PathBuf::from("devin"))
        .or_else(|_| devin_auth_status(home.join(".local/bin/devin")));

    if let Ok(out) = output {
        let stdout = String::from_utf8_lossy(&out.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("Email:") {
                probed.email = trimmed.trim_start_matches("Email:").trim().to_string();
            } else if trimmed.starts_with("Name:") {
                probed.name = trimmed.trim_start_matches("Name:").trim().to_string();
            } else if trimmed.starts_with("User ID:") {
                probed.user_id = trimmed.trim_start_matches("User ID:").trim().to_string();
            } else if trimmed.starts_with("Plan:") {
                probed.plan = trimmed.trim_start_matches("Plan:").trim().to_string();
            } else if trimmed.starts_with("Team ID:") {
                probed.org_id = trimmed.trim_start_matches("Team ID:").trim().to_string();
            }
        }
    }

    if probed.org_id.is_empty() {
        if let Some(org) = read_devin_org_id(home) {
            probed.org_id = org;
        }
    }

    if probed.email.is_empty() {
        if let Some(email) = read_in_use_email(home) {
            probed.email = email;
        }
    }

    probed
}

fn read_current_token(home: &Path) -> Option<String> {
    let creds_path = home.join(".local/share/devin/credentials.toml");
    let content = fs::read_to_string(&creds_path).ok()?;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("windsurf_api_key") {
            if let Some(val) = trimmed.split('=').nth(1) {
                let token = val.trim().trim_matches('"').trim_matches('\'').trim();
                if !token.is_empty() {
                    return Some(token.to_string());
                }
            }
        }
    }
    None
}

fn write_current_token(home: &Path, token: &str) -> Result<(), String> {
    let creds_dir = home.join(".local/share/devin");
    fs::create_dir_all(&creds_dir).map_err(|e| e.to_string())?;
    let creds_path = creds_dir.join("credentials.toml");
    let mut lines = Vec::new();
    let mut replaced = false;
    if let Ok(content) = fs::read_to_string(&creds_path) {
        for line in content.lines() {
            if line.trim().starts_with("windsurf_api_key") {
                lines.push(format!("windsurf_api_key = \"{}\"", token));
                replaced = true;
            } else {
                lines.push(line.to_string());
            }
        }
    }
    if !replaced {
        lines.insert(0, format!("windsurf_api_key = \"{}\"", token));
        if !lines.iter().any(|l| l.contains("api_server_url")) {
            lines.push("api_server_url = \"https://server.codeium.com\"".to_string());
        }
        if !lines.iter().any(|l| l.contains("devin_webapp_host")) {
            lines.push("devin_webapp_host = \"app.devin.ai\"".to_string());
        }
        if !lines.iter().any(|l| l.contains("devin_api_url")) {
            lines.push("devin_api_url = \"https://api.devin.ai\"".to_string());
        }
    }
    let data = lines.join("\n") + "\n";
    crate::private_file::write_atomic(&creds_path, data.as_bytes()).map_err(|e| e.to_string())
}

fn read_devin_org_id(home: &Path) -> Option<String> {
    let cfg_path = home.join(".config/devin/config.json");
    let content = fs::read_to_string(&cfg_path).ok()?;
    let val: Value = serde_json::from_str(&content).ok()?;
    val.get("devin")
        .and_then(|d| d.get("org_id"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn write_devin_org_id(home: &Path, org_id: &str) -> Result<(), String> {
    if org_id.trim().is_empty() {
        return Ok(());
    }
    let cfg_dir = home.join(".config/devin");
    fs::create_dir_all(&cfg_dir).map_err(|e| e.to_string())?;
    let cfg_path = cfg_dir.join("config.json");
    let mut val: Value = fs::read_to_string(&cfg_path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_else(|| json!({}));
    if let Some(map) = val.as_object_mut() {
        let devin_obj = map.entry("devin").or_insert_with(|| json!({}));
        if let Some(obj) = devin_obj.as_object_mut() {
            obj.insert("org_id".to_string(), json!(org_id));
        }
    }
    crate::atomic_file::write_atomic(
        &cfg_path,
        serde_json::to_string_pretty(&val)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())
}

fn read_in_use_email(home: &Path) -> Option<String> {
    let path = home.join(".Ddevin/in_use.json");
    let content = fs::read_to_string(&path).ok()?;
    let val: Value = serde_json::from_str(&content).ok()?;
    val.get("claims")
        .and_then(Value::as_object)
        .and_then(|map| map.keys().next().cloned())
}

fn write_in_use_email(home: &Path, email: &str) -> Result<(), String> {
    let dir = home.join(".Ddevin");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("in_use.json");
    let mut val: Value = fs::read_to_string(&path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_else(|| json!({"version": 1, "claims": {}}));

    if let Some(obj) = val.as_object_mut() {
        let claims = obj.entry("claims").or_insert_with(|| json!({}));
        if let Some(cmap) = claims.as_object_mut() {
            cmap.clear();
            cmap.insert(
                email.to_string(),
                json!({
                    "username": email,
                    "windowId": "w_webclx",
                    "pid": std::process::id(),
                    "ts": now_ms(),
                }),
            );
        }
        obj.insert("updatedAt".to_string(), json!(chrono_iso_now()));
    }
    crate::private_file::write_atomic(
        &path,
        serde_json::to_string_pretty(&val)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())
}

fn chrono_iso_now() -> String {
    // 简易 ISO 时间戳
    let now = now_ms() / 1000;
    format!("{now}")
}

fn read_bypass_status(home: &Path) -> bool {
    let cfg_path = home.join(".config/devin/config.json");
    if let Ok(content) = fs::read_to_string(&cfg_path) {
        if let Ok(val) = serde_json::from_str::<Value>(&content) {
            let skip_trust = val
                .get("skip_workspace_trust")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let has_allow_all = val
                .get("permissions")
                .and_then(|p| p.get("allow"))
                .and_then(Value::as_array)
                .map(|arr| arr.iter().any(|item| item.as_str() == Some("*")))
                .unwrap_or(false);
            if skip_trust || has_allow_all {
                return true;
            }
        }
    }
    let desktop_settings_path = home.join(".config/Devin/User/settings.json");
    if let Ok(content) = fs::read_to_string(&desktop_settings_path) {
        if let Ok(val) = serde_json::from_str::<Value>(&content) {
            if let Some(mode) = val
                .get("devin.acp.agentPreferences")
                .and_then(|p| p.get("devin-cli"))
                .and_then(|c| c.get("mode"))
                .and_then(Value::as_str)
            {
                if mode == "bypass" || mode == "dangerous" {
                    return true;
                }
            }
        }
    }
    false
}

fn write_bypass_status(home: &Path, bypass: bool) -> Result<(), String> {
    // 1. ~/.config/devin/config.json
    let cfg_dir = home.join(".config/devin");
    fs::create_dir_all(&cfg_dir).map_err(|e| e.to_string())?;
    let cfg_path = cfg_dir.join("config.json");
    let mut val: Value = fs::read_to_string(&cfg_path)
        .ok()
        .and_then(|c| serde_json::from_str(&c).ok())
        .unwrap_or_else(|| json!({}));
    if !val.is_object() {
        val = json!({});
    }
    if let Some(obj) = val.as_object_mut() {
        if bypass {
            obj.insert("skip_workspace_trust".to_string(), json!(true));
            obj.insert(
                "permissions".to_string(),
                json!({
                    "allow": ["*"],
                    "deny": [],
                    "ask": []
                }),
            );
        } else {
            obj.insert("skip_workspace_trust".to_string(), json!(false));
            obj.insert(
                "permissions".to_string(),
                json!({
                    "allow": [],
                    "deny": [],
                    "ask": []
                }),
            );
        }
    }
    crate::atomic_file::write_atomic(
        &cfg_path,
        serde_json::to_string_pretty(&val)
            .map_err(|e| e.to_string())?
            .as_bytes(),
    )
    .map_err(|e| e.to_string())?;

    // 2. ~/.config/Devin/User/settings.json
    let desktop_dir = home.join(".config/Devin/User");
    let desktop_settings_path = desktop_dir.join("settings.json");
    if fs::create_dir_all(&desktop_dir).is_ok() {
        let mut dval: Value = fs::read_to_string(&desktop_settings_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_else(|| json!({}));
        if let Some(dobj) = dval.as_object_mut() {
            let prefs = dobj
                .entry("devin.acp.agentPreferences")
                .or_insert_with(|| json!({}));
            if let Some(pobj) = prefs.as_object_mut() {
                let cli_pref = pobj.entry("devin-cli").or_insert_with(|| json!({}));
                if let Some(cobj) = cli_pref.as_object_mut() {
                    cobj.insert("mode".to_string(), json!(if bypass { "bypass" } else { "auto" }));
                }
            }
            if bypass {
                let envs = dobj
                    .entry("devin.acp.agentEnv")
                    .or_insert_with(|| json!({}));
                if let Some(eobj) = envs.as_object_mut() {
                    let cli_env = eobj.entry("devin-cli").or_insert_with(|| json!({}));
                    if let Some(cenv) = cli_env.as_object_mut() {
                        cenv.insert("DEVIN_PERMISSION_MODE".to_string(), json!("dangerous"));
                    }
                }
            }
        }
        let _ = fs::write(
            &desktop_settings_path,
            serde_json::to_string_pretty(&dval).unwrap_or_default(),
        );
    }

    // 3. ~/.bashrc
    let bashrc_path = home.join(".bashrc");
    if let Ok(bashrc_content) = fs::read_to_string(&bashrc_path) {
        let has_env = bashrc_content.contains("DEVIN_PERMISSION_MODE=");
        if bypass && !has_env {
            let mut new_content = bashrc_content;
            new_content.push_str("\n# Devin CLI auto approval / bypass permissions mode\nexport DEVIN_PERMISSION_MODE=\"dangerous\"\nalias devin=\"devin --permission-mode dangerous\"\n");
            let _ = fs::write(&bashrc_path, new_content);
        } else if !bypass && has_env {
            let filtered: Vec<&str> = bashrc_content
                .lines()
                .filter(|l| !l.contains("DEVIN_PERMISSION_MODE") && !l.contains("alias devin="))
                .collect();
            let _ = fs::write(&bashrc_path, filtered.join("\n") + "\n");
        }
    }

    Ok(())
}

fn sanitize_account_id(email: &str) -> String {
    let clean = email
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if clean.is_empty() {
        format!("devin-{}", now_ms())
    } else {
        format!("devin-{}", clean.to_lowercase())
    }
}

fn collect_devin_import_items(body: &Value) -> Result<Vec<DevinAccount>, String> {
    let mut list = Vec::new();

    // 格式 1: 直接数组
    if let Some(arr) = body.as_array() {
        for v in arr {
            if let Some(a) = parse_devin_account_value(v) {
                list.push(a);
            }
        }
        return Ok(list);
    }

    // 格式 2: bundle 格式 { "sections": { "devin_presets": { "accounts": [...] } } }
    if let Some(sections) = body.get("sections").and_then(Value::as_object) {
        if let Some(devin_sec) = sections
            .get("devin_presets")
            .or_else(|| sections.get("devin_accounts"))
        {
            if let Some(arr) = devin_sec.get("accounts").and_then(Value::as_array) {
                for v in arr {
                    if let Some(a) = parse_devin_account_value(v) {
                        list.push(a);
                    }
                }
                return Ok(list);
            }
        }
    }

    // 格式 3: { "accounts": [...] }
    if let Some(arr) = body.get("accounts").and_then(Value::as_array) {
        for v in arr {
            if let Some(a) = parse_devin_account_value(v) {
                list.push(a);
            }
        }
        return Ok(list);
    }

    // 格式 4: 文本导入 { "text": "..." } 或直接传纯文本字符串
    let text = body
        .get("text")
        .and_then(Value::as_str)
        .or_else(|| body.as_str());
    if let Some(text) = text {
        // 尝试作为 JSON 解析
        if let Ok(parsed) = serde_json::from_str::<Value>(text) {
            return collect_devin_import_items(&parsed);
        }
        // 按行解析
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            // 分隔符：----
            let parts: Vec<&str> = trimmed.split("----").collect();
            if parts.len() >= 2 {
                let email = parts[0].trim();
                let token = parts.last().unwrap().trim();
                if !token.is_empty() {
                    list.push(DevinAccount {
                        id: sanitize_account_id(email),
                        name: email.to_string(),
                        email: email.to_string(),
                        token: token.to_string(),
                        org_id: String::new(),
                        user_id: String::new(),
                        plan: "Devin Pro".to_string(),
                        active: false,
                        saved_at: now_ms(),
                    });
                }
            } else if trimmed.starts_with("devin-session-token$")
                || trimmed.contains("windsurf_api_key")
            {
                let token = if let Some(idx) = trimmed.find("devin-session-token$") {
                    trimmed[idx..].trim_matches('"').trim_matches('\'').trim()
                } else {
                    trimmed
                };
                list.push(DevinAccount {
                    id: format!("devin-{}", now_ms()),
                    name: "Imported Token".to_string(),
                    email: String::new(),
                    token: token.to_string(),
                    org_id: String::new(),
                    user_id: String::new(),
                    plan: "Devin Pro".to_string(),
                    active: false,
                    saved_at: now_ms(),
                });
            }
        }
    }

    Ok(list)
}

fn parse_devin_account_value(v: &Value) -> Option<DevinAccount> {
    if !v.is_object() {
        return None;
    }
    let token = v
        .get("token")
        .or_else(|| v.get("windsurf_api_key"))
        .or_else(|| v.get("api_key"))
        .or_else(|| v.get("session_token"))
        .and_then(Value::as_str)?
        .trim();
    if token.is_empty() {
        return None;
    }

    let email = v
        .get("email")
        .or_else(|| v.get("username"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();

    let name = v
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(email)
        .trim();

    let org_id = v.get("org_id").and_then(Value::as_str).unwrap_or("").trim();

    let user_id = v
        .get("user_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();

    let plan = v
        .get("plan")
        .and_then(Value::as_str)
        .unwrap_or("Devin Pro")
        .trim();

    let id = v
        .get("id")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| sanitize_account_id(if !email.is_empty() { email } else { name }));

    Some(DevinAccount {
        id,
        name: if !name.is_empty() {
            name.to_string()
        } else {
            "Devin Account".to_string()
        },
        email: email.to_string(),
        token: token.to_string(),
        org_id: org_id.to_string(),
        user_id: user_id.to_string(),
        plan: plan.to_string(),
        active: false,
        saved_at: now_ms(),
    })
}

// ---------------------------------------------------------------------------
// HTTP Routes
// ---------------------------------------------------------------------------

pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/api/auth/devin-accounts", get(list_devin_accounts).post(add_devin_account))
        .route(
            "/api/auth/devin-accounts/{account_id}",
            axum::routing::delete(delete_devin_account),
        )
        .route("/api/auth/devin-accounts/{account_id}/switch", put(switch_devin_account))
        .route("/api/auth/devin-accounts/import", post(import_devin_accounts))
        .route("/api/auth/devin-accounts/export", post(export_devin_accounts))
        .route("/api/auth/devin-accounts/import-current", post(import_current_devin_account))
        .route("/api/auth/devin-accounts/detect-current", post(detect_current_devin_account))
        .route("/api/auth/devin-options", get(get_devin_options).put(set_devin_options))
}

async fn detect_current_devin_account(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    state
        .devin_manager
        .detect_current()
        .map(Json)
        .map_err(AppError::bad_request)
}

async fn list_devin_accounts(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.devin_manager.list_accounts()))
}

async fn add_devin_account(
    State(state): State<AppState>,
    Json(req): Json<AddDevinAccountRequest>,
) -> ApiResult<Json<Value>> {
    state
        .devin_manager
        .add_account(req)
        .map(|acct| Json(json!(acct)))
        .map_err(AppError::bad_request)
}

async fn delete_devin_account(
    State(state): State<AppState>,
    AxumPath(account_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    state
        .devin_manager
        .delete_account(&account_id)
        .map(|_| Json(json!({"ok": true})))
        .map_err(AppError::bad_request)
}

async fn switch_devin_account(
    State(state): State<AppState>,
    AxumPath(account_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    state
        .devin_manager
        .switch_account(&account_id)
        .map(|acct| Json(json!({"ok": true, "account": acct})))
        .map_err(AppError::bad_request)
}

async fn import_devin_accounts(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> ApiResult<Json<Value>> {
    state
        .devin_manager
        .import_accounts(&body)
        .map(Json)
        .map_err(AppError::bad_request)
}

async fn export_devin_accounts(
    State(state): State<AppState>,
    Json(req): Json<ExportDevinAccountsRequest>,
) -> ApiResult<Json<Value>> {
    let ids = if req.ids.is_empty() {
        None
    } else {
        Some(req.ids)
    };
    state
        .devin_manager
        .export_accounts(ids)
        .map(Json)
        .map_err(AppError::bad_request)
}

async fn import_current_devin_account(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    state
        .devin_manager
        .import_current()
        .map(|acct| Json(json!({"ok": true, "account": acct})))
        .map_err(AppError::bad_request)
}

async fn get_devin_options(State(state): State<AppState>) -> Json<Value> {
    Json(state.devin_manager.get_options())
}

async fn set_devin_options(
    State(state): State<AppState>,
    Json(req): Json<UpdateDevinOptionsRequest>,
) -> ApiResult<Json<Value>> {
    state
        .devin_manager
        .set_options(req.bypass)
        .map(Json)
        .map_err(AppError::bad_request)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_temp_dir(label: &str) -> PathBuf {
        let nonce = current_timestamp_millis();
        let path = std::env::temp_dir()
            .join(format!("webclx-devin-{label}-{}-{nonce}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn test_devin_manager_lifecycle_and_bypass() {
        let dir = test_temp_dir("lifecycle");
        let app_dir = dir.join("app");
        let home = dir.join("home");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let manager = DevinManager::load(app_dir, home.clone());

        // 1. Initial accounts should be empty
        assert!(manager.list_accounts().is_empty());

        // 2. Add an account
        let added = manager
            .add_account(AddDevinAccountRequest {
                name: "Test Account".to_string(),
                email: "test@example.com".to_string(),
                token: "token-12345678".to_string(),
                org_id: "org-abc".to_string(),
                plan: "Pro".to_string(),
            })
            .unwrap();
        assert_eq!(added.email, "test@example.com");
        assert_eq!(added.name, "Test Account");

        // 3. List accounts
        let list = manager.list_accounts();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, added.id);

        // 4. Switch account
        let switched = manager.switch_account(&added.id).unwrap();
        assert!(switched.active);

        // Check credentials written to ~/.local/share/devin/credentials.toml
        let cred_path = home.join(".local/share/devin/credentials.toml");
        assert!(cred_path.exists());
        let cred_content = std::fs::read_to_string(cred_path).unwrap();
        assert!(cred_content.contains("token-12345678"));

        // 5. Options: toggle bypass
        let opts = manager.get_options();
        assert_eq!(opts["bypass"], false);

        manager.set_options(true).unwrap();
        let opts = manager.get_options();
        assert_eq!(opts["bypass"], true);

        // Verify config.json has permissions.allow: ["*"] and skip_workspace_trust: true
        let config_path = home.join(".config/devin/config.json");
        assert!(config_path.exists());
        let config_json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&config_path).unwrap()).unwrap();
        assert_eq!(config_json["skip_workspace_trust"], true);
        assert_eq!(config_json["permissions"]["allow"], serde_json::json!(["*"]));

        // Toggle back off
        manager.set_options(false).unwrap();
        let opts = manager.get_options();
        assert_eq!(opts["bypass"], false);

        // 6. Delete account
        manager.delete_account(&added.id).unwrap();
        assert!(manager.list_accounts().is_empty());
    }
}
