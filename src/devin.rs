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
use sha2::{Digest, Sha256};
use terminal_core::current_timestamp_millis;
use tracing::info;

use crate::{ApiResult, AppError, AppState};

fn now_ms() -> i64 {
    current_timestamp_millis() as i64
}

pub(crate) const DEVIN_ACCOUNTS_FILE: &str = "webclx-devin-accounts.json";

/// CLI tokens and Desktop encrypted login snapshots are separate credential formats.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DevinClientType {
    #[default]
    Cli,
    Desktop,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct DevinAccount {
    #[serde(default)]
    pub client_type: DevinClientType,
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
    pub client_type: DevinClientType,
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
        // 历史数据里可能出现同一 id 的多条记录，加载时先收敛，保证后续按 id 切换/删除只命中一条
        dedupe_accounts(&mut accounts);

        let current_token = read_current_token(&home);
        // 如果当前有登录凭据，但账号库为空，自动将当前登录加入账号库
        if let Some(ref token) = current_token {
            let exists = accounts
                .iter()
                .any(|a| a.client_type == DevinClientType::Cli && a.token == *token);
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
                // 同一账号已在账号库中但凭据不同时原位更新，避免同 id 产生重复记录
                upsert_current_login(
                    &mut accounts,
                    DevinAccount {
                        client_type: DevinClientType::Cli,
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
                    },
                );
            }
        }

        // 同步 active 状态
        for a in accounts.iter_mut() {
            a.active = current_token
                .as_ref()
                .map_or(false, |tok| a.client_type == DevinClientType::Cli && tok == &a.token);
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
            a.active = current_token
                .as_ref()
                .map_or(false, |tok| a.client_type == DevinClientType::Cli && tok == &a.token);
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
        require_cli_type(target.client_type)?;
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
            a.active = a.client_type == DevinClientType::Cli && a.id == account_id;
        }

        let switched = accounts[target_idx].clone();
        drop(accounts);
        self.persist_internal()?;

        info!("Switched Devin account to {}", switched.name);
        Ok(switched)
    }

    pub fn add_account(&self, req: AddDevinAccountRequest) -> Result<DevinAccount, String> {
        require_cli_type(req.client_type)?;
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
            client_type: DevinClientType::Cli,
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

        if let Some(pos) = accounts.iter().position(|a| {
            a.client_type == DevinClientType::Cli && (a.id == id || a.token == account.token)
        }) {
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
        accounts.retain(|a| !(a.client_type == DevinClientType::Cli && a.id == account_id));
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
            client_type: DevinClientType::Cli,
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
        if let Some(pos) = accounts.iter().position(|a| {
            a.client_type == DevinClientType::Cli
                && (a.id == account.id || a.token == account.token)
        }) {
            accounts[pos] = account.clone();
        } else {
            accounts.push(account.clone());
        }
        for a in accounts.iter_mut() {
            a.active = a.client_type == DevinClientType::Cli && a.id == account.id;
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
            let matched_pos = accounts
                .iter()
                .position(|a| a.client_type == DevinClientType::Cli && &a.token == token);
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
        let mut skipped: Vec<Value> = Vec::new();

        for item in raw_items {
            let token = item.token.trim().to_string();
            if token.is_empty() {
                continue;
            }
            let id = sanitize_account_id(&item.email);
            let active = current_token.as_ref().map_or(false, |tok| tok == &token);
            // 导入数据自带保存时间时以来源时间为准，缺失才用本次导入时间
            let incoming_saved_at = if item.saved_at > 0 {
                item.saved_at
            } else {
                now_ms()
            };

            let new_acct = DevinAccount {
                client_type: DevinClientType::Cli,
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
                saved_at: incoming_saved_at,
            };

            if let Some(pos) = accounts.iter().position(|a| {
                a.client_type == DevinClientType::Cli && (a.id == id || a.token == new_acct.token)
            }) {
                let existing = &accounts[pos];
                // 新凭据覆盖旧凭据；旧的导入项不覆盖已有较新记录，改为报告跳过
                if incoming_saved_at < existing.saved_at {
                    skipped.push(json!({
                        "id": existing.id,
                        "email": existing.email,
                        "incoming_saved_at": incoming_saved_at,
                        "existing_saved_at": existing.saved_at,
                    }));
                    continue;
                }
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
            "updated": updated,
            "skipped": skipped
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

    /// Reads only Desktop account metadata. Authentication secrets stay in the
    /// Electron database and are never returned to the browser or stored here.
    pub fn import_desktop_current(&self) -> Result<DevinAccount, String> {
        let profile = self.home.join(".config/Devin");
        let database = profile.join("User/globalStorage/state.vscdb");
        let metadata = probe_desktop_profile(&profile, &database)?;
        let account = DevinAccount {
            client_type: DevinClientType::Desktop,
            id: metadata.id.clone(),
            name: metadata
                .email
                .clone()
                .unwrap_or_else(|| "本机 Devin Desktop".to_string()),
            email: metadata.email.unwrap_or_default(),
            token: String::new(),
            org_id: String::new(),
            user_id: String::new(),
            plan: "Desktop".to_string(),
            active: metadata.logged_in,
            saved_at: now_ms(),
        };

        let mut accounts = self.accounts.write().map_err(|e| e.to_string())?;
        let position = accounts
            .iter()
            .position(|a| a.client_type == DevinClientType::Desktop && a.id == account.id);
        if let Some(pos) = position {
            accounts[pos] = account.clone();
        } else {
            accounts.push(account.clone());
        }
        drop(accounts);
        self.persist_internal()?;
        Ok(account)
    }

    pub fn detect_desktop(&self) -> Value {
        let profile = self.home.join(".config/Devin");
        let database = profile.join("User/globalStorage/state.vscdb");
        match probe_desktop_profile(&profile, &database) {
            Ok(metadata) => json!({
                "ok": true,
                "supported": true,
                "source": "webclx_server",
                "profile": profile.display().to_string(),
                "account": {
                    "client_type": "desktop",
                    "id": metadata.id,
                    "email": metadata.email,
                    "api_server_url": metadata.api_server_url,
                    "logged_in": metadata.logged_in,
                    "auth_status_present": metadata.auth_status_present,
                    "sessions_present": metadata.sessions_present,
                    "backup_supported": true,
                    "restore_supported": true,
                },
                "message": "已读取 webClx 所在主机的 Desktop 配置；未验证服务端登录有效性，浏览器不能直接读取其他设备磁盘"
            }),
            Err(error) => json!({
                "ok": false,
                "supported": true,
                "source": "webclx_server",
                "profile": profile.display().to_string(),
                "error": error,
            }),
        }
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

/// 收敛同一客户端类型下 id 重复的记录，只保留一条。
/// 优先保留当前生效的记录，其次保留最近保存的记录；不改变账号类型边界。
fn dedupe_accounts(accounts: &mut Vec<DevinAccount>) {
    let mut kept: Vec<DevinAccount> = Vec::with_capacity(accounts.len());
    for account in accounts.drain(..) {
        match kept
            .iter()
            .position(|a| a.client_type == account.client_type && a.id == account.id)
        {
            Some(pos) => {
                let existing = &kept[pos];
                let prefer_new = (account.active && !existing.active)
                    || (account.active == existing.active && account.saved_at > existing.saved_at);
                if prefer_new {
                    kept[pos] = account;
                }
            }
            None => kept.push(account),
        }
    }
    *accounts = kept;
}

/// 当前登录凭据尚未入库时的登记入口：同一账号已存在则原位更新，不新增同 id 记录。
fn upsert_current_login(accounts: &mut Vec<DevinAccount>, account: DevinAccount) {
    match accounts.iter().position(|a| {
        a.client_type == DevinClientType::Cli && (a.id == account.id || a.token == account.token)
    }) {
        Some(pos) => {
            let id = accounts[pos].id.clone();
            let mut updated = account;
            updated.id = id;
            updated.saved_at = now_ms();
            accounts[pos] = updated;
        }
        None => accounts.push(account),
    }
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

#[derive(Debug, PartialEq, Eq, serde::Serialize)]
struct DesktopProfileMetadata {
    id: String,
    email: Option<String>,
    api_server_url: Option<String>,
    logged_in: bool,
    auth_status_present: bool,
    sessions_present: bool,
}

fn probe_desktop_profile(
    profile: &Path,
    database_path: &Path,
) -> Result<DesktopProfileMetadata, String> {
    if !profile.is_absolute() || !database_path.starts_with(profile) {
        return Err("Desktop 配置目录无效".to_string());
    }
    let database = rusqlite::Connection::open_with_flags(
        database_path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
            | rusqlite::OpenFlags::SQLITE_OPEN_URI
            | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(|_| "无法只读打开 Devin Desktop 登录数据库".to_string())?;
    let row = database
        .query_row("SELECT value FROM ItemTable WHERE key=?1", ["codeium.windsurf"], |row| {
            row.get::<_, String>(0)
        })
        .ok();
    let settings = row
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({}));
    let email = settings
        .get("lastLoginEmail")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let api_server_url = settings
        .get("apiServerUrl")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let exists = |key: &str| -> bool {
        database
            .query_row("SELECT 1 FROM ItemTable WHERE key=?1 AND value IS NOT NULL", [key], |_| {
                Ok(())
            })
            .is_ok()
    };
    let auth_status_present = exists("windsurfAuthStatus");
    let sessions_present = exists(
        "secret://{\"extensionId\":\"codeium.windsurf\",\"key\":\"windsurf_auth.sessions\"}",
    );
    let api_server_url_present = database
        .query_row(
            "SELECT 1 FROM ItemTable WHERE key=?1 AND value IS NOT NULL",
            ["secret://{\"extensionId\":\"codeium.windsurf\",\"key\":\"windsurf_auth.apiServerUrl\"}"],
            |_| Ok(()),
        )
        .is_ok();
    let logged_in = auth_status_present && sessions_present && api_server_url_present;
    let mut digest = Sha256::new();
    digest.update(profile.as_os_str().as_encoded_bytes());
    let profile_hash = hex::encode(digest.finalize());
    Ok(DesktopProfileMetadata {
        id: format!("devin-desktop-{}", &profile_hash[..16]),
        email,
        api_server_url,
        logged_in,
        auth_status_present,
        sessions_present,
    })
}

fn collect_devin_import_items(body: &Value) -> Result<Vec<DevinAccount>, String> {
    let mut list = Vec::new();

    // 格式 1: 直接数组
    if let Some(arr) = body.as_array() {
        for v in arr {
            if let Some(a) = parse_devin_account_value(v)? {
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
                    if let Some(a) = parse_devin_account_value(v)? {
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
            if let Some(a) = parse_devin_account_value(v)? {
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
                        client_type: DevinClientType::Cli,
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
                    client_type: DevinClientType::Cli,
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

fn require_cli_type(client_type: DevinClientType) -> Result<(), String> {
    if client_type != DevinClientType::Cli {
        return Err(
            "Desktop 登录快照不能通过 CLI Token 接口导入或应用，请使用 Desktop 加密备份恢复工具"
                .to_string(),
        );
    }
    Ok(())
}

fn parse_devin_account_value(v: &Value) -> Result<Option<DevinAccount>, String> {
    if !v.is_object() {
        return Ok(None);
    }
    let client_type = match v.get("client_type") {
        None => DevinClientType::Cli,
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|_| "Devin client_type 必须为 cli 或 desktop".to_string())?,
    };
    require_cli_type(client_type)?;
    let Some(token) = v
        .get("token")
        .or_else(|| v.get("windsurf_api_key"))
        .or_else(|| v.get("api_key"))
        .or_else(|| v.get("session_token"))
        .and_then(Value::as_str)
    else {
        return Ok(None);
    };
    let token = token.trim();
    if token.is_empty() {
        return Ok(None);
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

    // 保留来源保存时间，导入时才能判断新旧；缺失才用本次导入时间
    let saved_at = ["saved_at", "savedAt", "updated_at"]
        .iter()
        .find_map(|key| v.get(*key).and_then(Value::as_i64))
        .filter(|value| *value > 0)
        .unwrap_or_else(now_ms);

    Ok(Some(DevinAccount {
        client_type: DevinClientType::Cli,
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
        saved_at,
    }))
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
        .route(
            "/api/auth/devin-accounts/import-desktop-current",
            post(import_desktop_current_devin_account),
        )
        .route("/api/auth/devin-accounts/detect-desktop", post(detect_desktop_devin_account))
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

async fn detect_desktop_devin_account(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    let result = state.devin_manager.detect_desktop();
    if result["ok"] == false {
        return Err(AppError::bad_request(
            result["error"]
                .as_str()
                .unwrap_or("无法读取本机 Devin Desktop 配置")
                .to_string(),
        ));
    }
    Ok(Json(result))
}

async fn import_desktop_current_devin_account(
    State(state): State<AppState>,
) -> ApiResult<Json<Value>> {
    state
        .devin_manager
        .import_desktop_current()
        .map(|account| Json(json!({"ok": true, "account": account})))
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

    #[test]
    fn test_client_type_legacy_and_import_boundary() {
        let legacy = json!({"id":"old", "name":"old", "token":"cli-token"});
        let account: DevinAccount = serde_json::from_value(legacy.clone()).unwrap();
        assert_eq!(account.client_type, DevinClientType::Cli);
        assert_eq!(serde_json::to_value(account).unwrap()["client_type"], "cli");
        assert!(collect_devin_import_items(&json!([legacy])).unwrap().len() == 1);
        for kind in ["desktop", "unknown"] {
            let input = json!({"accounts":[
                {"token":"valid-cli", "client_type":"cli"},
                {"token":"must-not-import", "client_type":kind}
            ]});
            assert!(collect_devin_import_items(&input).is_err());
        }
        assert!(require_cli_type(DevinClientType::Desktop).is_err());
    }

    #[test]
    fn test_desktop_cannot_write_cli_credentials() {
        let dir = test_temp_dir("desktop-boundary");
        let manager = DevinManager {
            file: dir.join("accounts.json"),
            home: dir.join("home"),
            accounts: Arc::new(RwLock::new(vec![DevinAccount {
                client_type: DevinClientType::Desktop,
                id: "desktop-test".into(),
                token: "desktop-not-cli".into(),
                ..Default::default()
            }])),
        };
        assert!(manager.switch_account("desktop-test").is_err());
        let request = serde_json::from_value(json!({
            "client_type":"desktop", "token":"desktop-not-cli"
        }))
        .unwrap();
        assert!(manager.add_account(request).is_err());
        assert!(!manager.home.exists());
        assert!(!manager.list_accounts()[0].active);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn desktop_probe_returns_metadata_without_secrets() {
        let dir = test_temp_dir("desktop-probe");
        let profile = dir.join(".config/Devin");
        let database_path = profile.join("User/globalStorage/state.vscdb");
        std::fs::create_dir_all(database_path.parent().unwrap()).unwrap();
        let database = rusqlite::Connection::open(&database_path).unwrap();
        database
            .execute("CREATE TABLE ItemTable(key TEXT PRIMARY KEY, value BLOB)", [])
            .unwrap();
        database
            .execute(
                "INSERT INTO ItemTable(key,value) VALUES (?1,?2)",
                rusqlite::params![
                    "codeium.windsurf",
                    json!({
                        "lastLoginEmail": "desktop@example.test",
                        "apiServerUrl": "https://example.test",
                    })
                    .to_string()
                ],
            )
            .unwrap();
        for key in [
            "windsurfAuthStatus",
            "secret://{\"extensionId\":\"codeium.windsurf\",\"key\":\"windsurf_auth.sessions\"}",
            "secret://{\"extensionId\":\"codeium.windsurf\",\"key\":\"windsurf_auth.apiServerUrl\"}",
        ] {
            database
                .execute(
                    "INSERT INTO ItemTable(key,value) VALUES (?1,?2)",
                    rusqlite::params![key, "do-not-return"],
                )
                .unwrap();
        }
        let metadata = probe_desktop_profile(&profile, &database_path).unwrap();
        assert!(metadata.id.starts_with("devin-desktop-"));
        assert_eq!(metadata.email.as_deref(), Some("desktop@example.test"));
        assert_eq!(metadata.api_server_url.as_deref(), Some("https://example.test"));
        assert!(metadata.logged_in);
        let serialized = json!(metadata);
        assert!(!serialized.to_string().contains("do-not-return"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn cli_account(id: &str, token: &str, active: bool, saved_at: i64) -> DevinAccount {
        DevinAccount {
            client_type: DevinClientType::Cli,
            id: id.into(),
            name: id.into(),
            email: "user@example.com".into(),
            token: token.into(),
            plan: "Devin Pro".into(),
            active,
            saved_at,
            ..Default::default()
        }
    }

    fn import_test_manager(label: &str) -> (PathBuf, DevinManager) {
        let dir = test_temp_dir(label);
        let manager = DevinManager {
            file: dir.join("accounts.json"),
            home: dir.join("home"),
            accounts: Arc::new(RwLock::new(Vec::new())),
        };
        (dir, manager)
    }

    #[test]
    fn import_newer_credential_overwrites_silently() {
        let (dir, manager) = import_test_manager("import-newer");
        manager
            .import_accounts(
                &json!([{"email":"user@example.com","token":"token-old","saved_at":100}]),
            )
            .unwrap();
        let result = manager
            .import_accounts(
                &json!([{"email":"user@example.com","token":"token-new","saved_at":200}]),
            )
            .unwrap();
        assert_eq!(result["updated"], 1);
        assert_eq!(result["skipped"].as_array().unwrap().len(), 0);
        let accounts = manager.list_accounts();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].token, "token-new");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn import_older_credential_is_skipped_and_reported() {
        let (dir, manager) = import_test_manager("import-older");
        manager
            .import_accounts(
                &json!([{"email":"user@example.com","token":"token-new","saved_at":200}]),
            )
            .unwrap();
        let result = manager
            .import_accounts(
                &json!([{"email":"user@example.com","token":"token-old","saved_at":100}]),
            )
            .unwrap();
        assert_eq!(result["updated"], 0);
        let skipped = result["skipped"].as_array().unwrap();
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0]["incoming_saved_at"], 100);
        assert_eq!(skipped[0]["existing_saved_at"], 200);
        let accounts = manager.list_accounts();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].token, "token-new");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn import_without_timestamp_counts_as_newest() {
        let (dir, manager) = import_test_manager("import-fresh");
        manager
            .import_accounts(
                &json!([{"email":"user@example.com","token":"token-old","saved_at":200}]),
            )
            .unwrap();
        let result = manager
            .import_accounts(&json!([{"email":"user@example.com","token":"token-fresh"}]))
            .unwrap();
        assert_eq!(result["updated"], 1);
        assert!(result["skipped"].as_array().unwrap().is_empty());
        assert_eq!(manager.list_accounts()[0].token, "token-fresh");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn dedupe_accounts_keeps_one_record_per_id() {
        let mut accounts = vec![
            cli_account("devin-same", "token-live", true, 10),
            cli_account("devin-same", "token-stale", false, 20),
            cli_account("devin-other", "token-other", false, 5),
        ];
        dedupe_accounts(&mut accounts);
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].id, "devin-same");
        // 与当前登录一致的记录优先保留
        assert_eq!(accounts[0].token, "token-live");
        assert_eq!(accounts[1].id, "devin-other");
    }

    #[test]
    fn dedupe_accounts_prefers_newest_when_active_state_equal() {
        let mut accounts = vec![
            cli_account("devin-same", "token-old", false, 10),
            cli_account("devin-same", "token-new", false, 20),
        ];
        dedupe_accounts(&mut accounts);
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].token, "token-new");
    }

    #[test]
    fn upsert_current_login_updates_existing_id() {
        let mut accounts = vec![cli_account(
            "devin-user-example-com",
            "token-old",
            false,
            10,
        )];
        upsert_current_login(
            &mut accounts,
            cli_account("devin-user-example-com", "token-new", true, 0),
        );
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, "devin-user-example-com");
        assert_eq!(accounts[0].token, "token-new");
    }

    #[test]
    fn load_collapses_duplicate_ids_from_store() {
        let dir = test_temp_dir("dedupe-load");
        let app_dir = dir.join("app");
        let home = dir.join("home");
        std::fs::create_dir_all(&app_dir).unwrap();
        std::fs::create_dir_all(&home).unwrap();
        let file = app_dir.join(DEVIN_ACCOUNTS_FILE);
        let stored = json!([
            cli_account("devin-same", "token-old", false, 10),
            cli_account("devin-same", "token-new", true, 20)
        ]);
        std::fs::write(&file, serde_json::to_vec(&stored).unwrap()).unwrap();

        let manager = DevinManager::load(app_dir, home);
        let accounts = manager.list_accounts();
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].token, "token-new");

        let persisted: Vec<DevinAccount> =
            serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
        assert_eq!(persisted.len(), 1, "重复记录必须同时从持久化文件收敛");
        std::fs::remove_dir_all(dir).unwrap();
    }

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
                client_type: DevinClientType::Cli,
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
