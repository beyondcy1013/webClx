//! CodeBuddy（workbuddy）账号管理：账号库、OAuth 添加、切换 CLI 账号、
//! 自动签到、派猫猫旅行与积分查询。
//!
//! 接口与状态机对齐 changexbc/workbuddy-switch（MIT）的 wb-switch-core 实现：
//! - 官方 API 域名 https://www.codebuddy.cn
//! - OAuth：POST /v2/plugin/auth/state?platform=workbuddy → 轮询
//!   GET /v2/plugin/auth/token?state= → GET /v2/plugin/login/account?state=
//! - Token 刷新：POST /v2/plugin/auth/token/refresh（X-Refresh-Token 头）
//! - 签到：POST /v2/billing/meter/checkin-activity-status（回退 checkin-status）
//!   与 POST /v2/billing/meter/daily-checkin
//! - 旅行：/activity/growth/buddy/travel/{config,status,depart,claim}，
//!   状态机 idle ->(depart)-> traveling ->(到点)-> arrived ->(claim)-> idle
//! - 积分：POST /v2/billing/meter/get-user-resource
//!
//! 账号与状态保存在 webClx 数据目录（webclx-codebuddy-accounts.json /
//! webclx-codebuddy-state.json）。切换 CodeBuddy CLI 账号只写入真实共享配置
//! （~/.codebuddy/settings.json 的 env.CODEBUDDY_AUTH_TOKEN），
//! 不注入终端环境变量。

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use terminal_core::current_timestamp_millis;
use tokio::sync::Mutex as AsyncMutex;
use tracing::{info, warn};

pub(crate) const CODEBUDDY_ACCOUNTS_FILE: &str = "webclx-codebuddy-accounts.json";
pub(crate) const CODEBUDDY_STATE_FILE: &str = "webclx-codebuddy-state.json";

const PLUGIN_PREFIX: &str = "/v2/plugin";
const CHECKIN_PREFIX: &str = "/v2/billing/meter";
const TRAVEL_PREFIX: &str = "/activity/growth/buddy/travel";
const USER_RESOURCE_PATH: &str = "/v2/billing/meter/get-user-resource";
/// 套餐页三路资源接口（summary / 付费包 / 免费包），对齐官方用户中心取数。
const RESOURCE_SUMMARY_PATH: &str = "/billing/meter/get-user-resource-summary";
const RESOURCE_PAID_PACKAGES_PATH: &str = "/billing/meter/get-user-resource-paid-packages";
const RESOURCE_FREE_PACKAGES_PATH: &str = "/billing/meter/get-user-resource-free-packages";
const PRODUCT_CODE: &str = "p_tcaca";
const OAUTH_TIMEOUT_SECS: i64 = 600;
const USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/152.0.0.0 Safari/537.36";

/// CodeBuddy CLI ProductManager 写入 ~/.codebuddy/local_storage/entry_<md5(key)>.info 的缓存文件名。
const CLI_ENV_CACHE_FILE: &str = "entry_3bab4ce61838088127d444e4cc042d6d.info";

/// CodeBuddy bypassPermissions 只放行 SAFE/LOW 命令；HIGH/CRITICAL 仍需要单独
/// 的精确 allow 规则（Bash(prefix:*) 前缀匹配）绕过安全检查。这里的列表覆盖
/// 远程部署（ssh/scp/rsync）、包管理（apt/dnf/brew）、服务控制（systemctl/
/// journalctl）、Git 破坏性操作（push --force/branch -D）等常见高危前缀。
const BYPASS_PERMISSION_ALLOW_RULES: &[&str] = &[
    "Bash(scp:*)",
    "Bash(ssh:*)",
    "Bash(rsync:*)",
    "Bash(sudo:*)",
    "Bash(su:*)",
    "Bash(systemctl:*)",
    "Bash(journalctl:*)",
    "Bash(service:*)",
    "Bash(apt:*)",
    "Bash(apt-get:*)",
    "Bash(yum:*)",
    "Bash(dnf:*)",
    "Bash(brew:*)",
    "Bash(docker:*)",
    "Bash(kubectl:*)",
    "Bash(helm:*)",
    "Bash(terraform:*)",
    "Bash(git:*)",
    "Bash(cargo:*)",
    "Bash(npm:*)",
    "Bash(yarn:*)",
    "Bash(pnpm:*)",
    "Bash(pip:*)",
    "Bash(pip3:*)",
    "Bash(curl:*)",
    "Bash(wget:*)",
    "Bash(rm:*)",
    "Bash(chmod:*)",
    "Bash(chown:*)",
];

/// 按 CodeBuddy CLI 源码：allow 规则的 `Bash(prefix:*)` 走 isCommandAllowed 的
/// prefix-matching，对所有以 prefix 开头的子命令生效；返回已排序去重的 Vec。
fn bypass_permission_allow_rules() -> Vec<Value> {
    let mut unique: Vec<&str> = BYPASS_PERMISSION_ALLOW_RULES.to_vec();
    unique.sort_unstable();
    unique.dedup();
    unique.into_iter().map(|rule| json!(rule)).collect()
}

const CLI_ENDPOINT_CACHE_FILE: &str = "entry_933d5543e80177622c17a73869c0fad7.info";
const CLI_PRODUCT_CACHE_FILE: &str = "entry_604f48c944053e01d9546675443286c1.info";

/// CodeBuddy / WorkBuddy 档位（国内版 / 国际版）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodeBuddyVariant {
    Cn,
    Ai,
}

impl Default for CodeBuddyVariant {
    fn default() -> Self {
        Self::Cn
    }
}

impl CodeBuddyVariant {
    pub const ALL: [CodeBuddyVariant; 2] = [CodeBuddyVariant::Cn, CodeBuddyVariant::Ai];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cn => "cn",
            Self::Ai => "ai",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Cn => "国内版",
            Self::Ai => "国际版",
        }
    }

    pub fn parse(raw: Option<&str>) -> Self {
        match raw
            .map(str::trim)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "ai" | "intl" | "global" => Self::Ai,
            _ => Self::Cn,
        }
    }

    pub fn from_domain(domain: &str) -> Self {
        let domain = domain.trim().to_ascii_lowercase();
        if domain.ends_with(".workbuddy.ai")
            || domain.ends_with(".codebuddy.ai")
            || domain == "workbuddy.ai"
            || domain == "codebuddy.ai"
        {
            Self::Ai
        } else {
            Self::Cn
        }
    }

    pub fn api_endpoint(self) -> &'static str {
        match self {
            Self::Cn => "https://www.codebuddy.cn",
            Self::Ai => "https://www.workbuddy.ai",
        }
    }

    pub fn oauth_platform(self) -> &'static str {
        match self {
            Self::Cn => "workbuddy",
            Self::Ai => "workbuddy-ai",
        }
    }

    pub fn cli_endpoint(self) -> &'static str {
        match self {
            Self::Cn => "https://copilot.tencent.com",
            Self::Ai => "https://www.codebuddy.ai",
        }
    }

    pub fn cli_openai_base_url(self) -> Option<&'static str> {
        match self {
            Self::Cn => None,
            Self::Ai => Some("https://www.codebuddy.ai/v2"),
        }
    }

    pub fn cli_internet_environment(self) -> &'static str {
        match self {
            Self::Cn => "internal",
            Self::Ai => "public",
        }
    }

    pub fn supports_checkin(self) -> bool {
        matches!(self, Self::Cn)
    }

    pub fn supports_travel(self) -> bool {
        matches!(self, Self::Cn)
    }

    pub fn billing_paths(self, path: &str) -> Vec<String> {
        match self {
            Self::Cn => vec![path.to_string()],
            Self::Ai => {
                let primary = path
                    .strip_prefix(CHECKIN_PREFIX)
                    .map(|rest| format!("/billing/meter{rest}"))
                    .unwrap_or_else(|| path.to_string());
                let fallback = format!("/v2{primary}");
                if fallback == primary {
                    vec![primary]
                } else {
                    vec![primary, fallback]
                }
            }
        }
    }
}

/// 自动签到补签周期：30 分钟。
const CHECKIN_RETRY_INTERVAL_MS: i64 = 30 * 60 * 1000;
/// 旅行领取/派发检查周期：15 分钟。
const TRAVEL_RETRY_INTERVAL_MS: i64 = 15 * 60 * 1000;
/// 自动循环调度间隔：5 分钟。
const AUTO_CYCLE_TICK: Duration = Duration::from_secs(5 * 60);
/// 旅行状态机已知状态。
const TRAVEL_STATES: [&str; 3] = ["idle", "traveling", "arrived"];
/// 积分临期阈值：7 天。
const EXPIRING_SOON_DAYS: i64 = 7;
/// DeductionEndTime 比 CycleEndTime 晚超过该天数时视为长期占位值，改用 CycleEndTime。
const EXPIRY_CYCLE_OVERRIDE_DAYS: i64 = 365;
/// 到期时间距现在超过该天数时视为长期有效（如 2049 占位值），不向前端下发。
const FAR_FUTURE_EXPIRY_DAYS: i64 = 730;
/// 积分自动刷新周期：30 分钟。
const CREDITS_RETRY_INTERVAL_MS: i64 = 30 * 60 * 1000;
/// 签到日志保留条数。
const CHECKIN_LOG_MAX: usize = 200;

/// 官方商品码 → 官方中文名（快照自官方客户端 package-name-resolver）。
/// 未登记的码回落到服务端 PackageName，再回落到商品码本身。
const CREDIT_PACKAGE_NAMES: &[(&str, &str)] = &[
    ("TCACA_code_001_PqouKr6QWV", "CodeBuddy 个人体验版"),
    ("TCACA_code_002_AkiJS3ZHF5", "版本基础用量"),
    ("TCACA_code_003_FAnt7lcmRT", "CodeBuddy 个人标准版"),
    ("TCACA_code_005_maRGyrHhw1", "版本基础用量"),
    ("TCACA_code_006_DbXS0lrypC", "CodeBuddy 个人体验版"),
    ("TCACA_code_007_nzdH5h4Nl0", "平台奖励积分"),
    ("TCACA_code_008_cfWoLwvjU4", "版本基础用量"),
    ("TCACA_code_009_0XmEQc2xOf", "购买积分"),
    ("TCACA_code_023_4xbGhMrE6q", "版本基础用量"),
    ("TCACA_code_026_BaESVICNoi", "版本基础用量"),
    ("TCACA_code_027_0FCGVA6vSa", "版本基础用量"),
    ("TCACA_code_028_NtpWi0jzXs", "版本赠送用量"),
    ("TCACA_code_029_6wCGEWquYy", "平台奖励积分"),
    ("TCACA_code_030_BjSt89qTvr", "平台奖励积分"),
    ("TCACA_code_035_ArVxJcGDsm", "版本基础用量"),
    ("TCACA_code_036_lupO5WgNdG", "购买积分"),
    ("TCACA_code_037_WxOD3MpI2o", "版本赠送用量"),
    ("TCACA_code_038_OhvqZtiPKr", "购买积分"),
    ("TCACA_code_039_KRcQj7wUat", "版本基础用量"),
    ("TCACA_code_040_mi9rCYg46x", "版本基础用量"),
];

/// 付费包商品码（付费/免费接口请求体使用，同时决定资源分类）。
const PAID_PACKAGE_CODES: &[&str] = &[
    "TCACA_code_002_AkiJS3ZHF5",
    "TCACA_code_023_4xbGhMrE6q",
    "TCACA_code_026_BaESVICNoi",
    "TCACA_code_027_0FCGVA6vSa",
    "TCACA_code_009_0XmEQc2xOf",
    "TCACA_code_038_OhvqZtiPKr",
    "TCACA_code_003_FAnt7lcmRT",
    "TCACA_code_036_lupO5WgNdG",
];
/// 免费包商品码。
const FREE_PACKAGE_CODES: &[&str] = &[
    "TCACA_code_008_cfWoLwvjU4",
    "TCACA_code_007_nzdH5h4Nl0",
    "TCACA_code_028_NtpWi0jzXs",
    "TCACA_code_029_6wCGEWquYy",
    "TCACA_code_030_BjSt89qTvr",
    "TCACA_code_001_PqouKr6QWV",
    "TCACA_code_006_DbXS0lrypC",
    "TCACA_code_035_ArVxJcGDsm",
    "TCACA_code_037_WxOD3MpI2o",
    "TCACA_code_039_KRcQj7wUat",
    "TCACA_code_040_mi9rCYg46x",
];

// ---------------------------------------------------------------------------
// 数据模型
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CodeBuddyAccount {
    pub id: String,
    pub uid: String,
    pub email: String,
    pub nickname: String,
    pub enterprise_id: String,
    pub enterprise_name: String,
    pub domain: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub token_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_expires_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refreshed_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(default, skip_serializing_if = "skip_if_false")]
    pub needs_relogin: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub needs_relogin_reason: String,
    #[serde(default, skip_serializing_if = "skip_if_false")]
    pub active: bool,
    #[serde(default)]
    pub switch_count: u64,
}

fn skip_if_false(value: &bool) -> bool {
    !*value
}

impl CodeBuddyAccount {
    pub fn variant(&self) -> CodeBuddyVariant {
        if let Some(ref v) = self.variant {
            if !v.trim().is_empty() {
                return CodeBuddyVariant::parse(Some(v));
            }
        }
        CodeBuddyVariant::from_domain(&self.domain)
    }

    fn display_name(&self) -> String {
        for value in [&self.email, &self.nickname, &self.uid] {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
        "unknown".to_string()
    }

    /// 摘要（不含 token）。
    fn meta(&self) -> Value {
        let variant = self.variant();
        json!({
            "id": self.id,
            "uid": self.uid,
            "email": self.email,
            "nickname": self.nickname,
            "enterpriseName": self.enterprise_name,
            "domain": self.domain,
            "variant": variant.as_str(),
            "variantLabel": variant.label(),
            "supportsCheckin": variant.supports_checkin(),
            "supportsTravel": variant.supports_travel(),
            "expiresAt": self.expires_at,
            "refreshExpiresAt": self.refresh_expires_at,
            "refreshedAt": self.refreshed_at,
            "createdAt": self.created_at,
            "needsRelogin": self.needs_relogin,
            "needsReloginReason": self.needs_relogin_reason,
            "active": self.active,
            "switchCount": self.switch_count,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct AutoConfig {
    checkin: bool,
    travel: bool,
}

impl Default for AutoConfig {
    fn default() -> Self {
        // 用户要求：默认支持自动签到和自动旅行。
        Self {
            checkin: true,
            travel: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct CodeBuddyState {
    active_account_id: String,
    auto: AutoConfig,
    last_checkin_run_ms: i64,
    last_travel_run_ms: i64,
    last_credits_run_ms: i64,
    #[serde(rename = "lastKeepaliveDay")]
    last_keepalive_day: String,
    checkin_logs: Vec<Value>,
    /// 每账号旅行缓存：key = account id。
    travel: HashMap<String, Value>,
    /// 每账号积分快照：key = account id。
    credits: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
struct OAuthInfo {
    state: String,
    variant: CodeBuddyVariant,
    expires_at_secs: i64,
}

#[derive(Debug, Default)]
struct Inner {
    accounts: Vec<CodeBuddyAccount>,
    state: CodeBuddyState,
}

// ---------------------------------------------------------------------------
// 管理器
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct CodeBuddyManager {
    app_dir: PathBuf,
    user_home: PathBuf,
    inner: Arc<RwLock<Inner>>,
    oauth_states: Arc<Mutex<HashMap<String, OAuthInfo>>>,
    write_lock: Arc<AsyncMutex<()>>,
    client: Arc<reqwest::Client>,
}

impl CodeBuddyManager {
    pub fn load(app_dir: PathBuf, user_home: PathBuf) -> anyhow::Result<Self> {
        let accounts = read_json_file(&app_dir.join(CODEBUDDY_ACCOUNTS_FILE))
            .and_then(|value| serde_json::from_value::<Vec<CodeBuddyAccount>>(value).ok())
            .unwrap_or_default();
        let state = read_json_file(&app_dir.join(CODEBUDDY_STATE_FILE))
            .and_then(|value| serde_json::from_value::<CodeBuddyState>(value).ok())
            .unwrap_or_default();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|error| anyhow::anyhow!("构建 CodeBuddy HTTP 客户端失败: {error}"))?;
        Ok(Self {
            app_dir,
            user_home,
            inner: Arc::new(RwLock::new(Inner { accounts, state })),
            oauth_states: Arc::new(Mutex::new(HashMap::new())),
            write_lock: Arc::new(AsyncMutex::new(())),
            client: Arc::new(client),
        })
    }

    fn accounts_file(&self) -> PathBuf {
        self.app_dir.join(CODEBUDDY_ACCOUNTS_FILE)
    }

    fn state_file(&self) -> PathBuf {
        self.app_dir.join(CODEBUDDY_STATE_FILE)
    }

    fn read_inner(&self) -> std::sync::RwLockReadGuard<'_, Inner> {
        crate::lock_or_recover!(self.inner.read())
    }

    fn write_inner(&self) -> std::sync::RwLockWriteGuard<'_, Inner> {
        crate::lock_or_recover!(self.inner.write())
    }

    fn persist_accounts(&self, accounts: &[CodeBuddyAccount]) -> Result<(), String> {
        let content = serde_json::to_string_pretty(accounts).map_err(|error| error.to_string())?;
        atomic_write(&self.accounts_file(), &content)
    }

    fn persist_state(&self, state: &CodeBuddyState) -> Result<(), String> {
        let content = serde_json::to_string_pretty(state).map_err(|error| error.to_string())?;
        atomic_write(&self.state_file(), &content)
    }

    // -----------------------------------------------------------------
    // 账号列表 / 手动添加 / 删除
    // -----------------------------------------------------------------

    pub fn list_accounts(&self) -> Vec<Value> {
        let inner = self.read_inner();
        let credits = inner.state.credits.clone();
        inner
            .accounts
            .iter()
            .map(|account| {
                let mut meta = account.meta();
                if let Some(summary) = credits.get(&account.id) {
                    meta["credits"] = summary.clone();
                }
                meta
            })
            .collect()
    }

    /// 导出指定账号（含 access_token / refresh_token 等凭据，供剪贴板与备份使用）。
    pub fn export_accounts(&self, selected_ids: &HashSet<&str>) -> Vec<CodeBuddyAccount> {
        let inner = self.read_inner();
        inner
            .accounts
            .iter()
            .filter(|account| selected_ids.contains(account.id.as_str()))
            .cloned()
            .collect()
    }

    /// 手动添加账号（token 方式）。会尝试拉取账号资料；失败也允许保存。
    pub async fn add_manual_account(
        &self,
        access_token: String,
        refresh_token: String,
        name: String,
        variant: Option<String>,
    ) -> Result<Value, String> {
        let token = clean_bearer_token(&access_token).ok_or("access_token 不能为空。")?;
        let parsed_variant = variant.as_deref().map(|v| CodeBuddyVariant::parse(Some(v)));
        let mut account = CodeBuddyAccount {
            id: new_id(),
            email: name.trim().to_string(),
            variant: parsed_variant.map(|v| v.as_str().to_string()),
            access_token: token,
            refresh_token: clean_bearer_token(&refresh_token).unwrap_or_default(),
            token_type: "Bearer".to_string(),
            created_at: Some(now_ms() as i64),
            ..Default::default()
        };
        if let Some(profile) = self.fetch_account_profile(&account).await {
            apply_profile(&mut account, &profile);
        }
        if account.variant.is_none() {
            account.variant = Some(account.variant().as_str().to_string());
        }
        let meta = self.upsert_account(account)?;
        Ok(meta)
    }

    /// 批量导入账号（支持原始 JSON 数组、wb-switch 导出的 JSON、剪贴板 bundle 或包含 accounts 的对象）。
    pub fn import_accounts(&self, body: &Value) -> Result<Value, String> {
        let raw_items = if let Some(arr) = body.as_array() {
            arr.clone()
        } else if let Some(sections) = body.get("sections").and_then(Value::as_object) {
            sections
                .get("codebuddy_presets")
                .or_else(|| sections.get("codebuddy_accounts"))
                .and_then(|s| s.get("accounts"))
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
        } else if let Some(arr) = body.get("accounts").and_then(Value::as_array) {
            arr.clone()
        } else if let Some(obj) = body.as_object() {
            vec![Value::Object(obj.clone())]
        } else {
            Vec::new()
        };

        if raw_items.is_empty() {
            return Err("没有可导入的有效 CodeBuddy 账号数据。".to_string());
        }

        let mut imported = 0usize;
        let mut updated = 0usize;
        for raw in raw_items {
            if let Some(account) = parse_codebuddy_import_account(&raw) {
                let inner = self.read_inner();
                let uid = account.uid.trim();
                let email = identity_email(&account);
                let exists = inner.accounts.iter().any(|existing| {
                    (!uid.is_empty() && existing.uid.trim() == uid)
                        || email
                            .as_ref()
                            .is_some_and(|e| Some(e) == identity_email(existing).as_ref())
                });
                drop(inner);
                if self.upsert_account(account).is_ok() {
                    if exists {
                        updated += 1;
                    } else {
                        imported += 1;
                    }
                }
            }
        }

        Ok(json!({
            "ok": true,
            "imported": imported,
            "updated": updated,
            "total": imported + updated,
        }))
    }

    pub fn delete_account(&self, account_id: &str) -> Result<Value, String> {
        let mut inner = self.write_inner();
        let before = inner.accounts.len();
        inner.accounts.retain(|account| account.id != account_id);
        if inner.accounts.len() == before {
            return Err("账号不存在。".to_string());
        }
        if inner.state.active_account_id == account_id {
            inner.state.active_account_id.clear();
        }
        inner.state.travel.remove(account_id);
        inner.state.credits.remove(account_id);
        self.persist_accounts(&inner.accounts)?;
        self.persist_state(&inner.state)?;
        Ok(json!({"ok": true, "deletedId": account_id}))
    }

    fn upsert_account(&self, account: CodeBuddyAccount) -> Result<Value, String> {
        let mut inner = self.write_inner();
        // 稳定身份：非空 uid 优先，其次真实 email。
        let new_uid = account.uid.trim().to_string();
        let new_email = identity_email(&account);
        let existing_index = inner.accounts.iter().position(|existing| {
            (!new_uid.is_empty() && existing.uid.trim() == new_uid)
                || new_email
                    .as_ref()
                    .is_some_and(|email| Some(email) == identity_email(existing).as_ref())
        });
        let saved = match existing_index {
            Some(index) => {
                let mut saved = account;
                saved.id = inner.accounts[index].id.clone();
                if saved.active {
                    for other in inner.accounts.iter_mut() {
                        other.active = false;
                    }
                } else {
                    saved.active = inner.accounts[index].active;
                }
                // 保留已有 refresh_token，防止覆盖为空
                if saved.refresh_token.trim().is_empty()
                    && !inner.accounts[index].refresh_token.trim().is_empty()
                {
                    saved.refresh_token = inner.accounts[index].refresh_token.clone();
                    if saved.refresh_expires_at.is_none() {
                        saved.refresh_expires_at = inner.accounts[index].refresh_expires_at;
                    }
                } else if saved.refresh_expires_at.is_none()
                    && saved.refresh_token == inner.accounts[index].refresh_token
                {
                    saved.refresh_expires_at = inner.accounts[index].refresh_expires_at;
                }
                // 保留已有 access_token，如果新传入的为空
                if saved.access_token.trim().is_empty()
                    && !inner.accounts[index].access_token.trim().is_empty()
                {
                    saved.access_token = inner.accounts[index].access_token.clone();
                    if saved.expires_at.is_none() {
                        saved.expires_at = inner.accounts[index].expires_at;
                    }
                }
                // 保留已有企业信息
                if saved.enterprise_id.trim().is_empty()
                    && !inner.accounts[index].enterprise_id.trim().is_empty()
                {
                    saved.enterprise_id = inner.accounts[index].enterprise_id.clone();
                    if saved.enterprise_name.trim().is_empty() {
                        saved.enterprise_name = inner.accounts[index].enterprise_name.clone();
                    }
                }
                // 保留已有 nickname
                if saved.nickname.trim().is_empty()
                    && !inner.accounts[index].nickname.trim().is_empty()
                {
                    saved.nickname = inner.accounts[index].nickname.clone();
                }
                if saved.refreshed_at.is_none() {
                    saved.refreshed_at = inner.accounts[index].refreshed_at;
                }
                if saved.created_at.is_none() {
                    saved.created_at = inner.accounts[index].created_at;
                }
                if saved.switch_count == 0 && inner.accounts[index].switch_count > 0 {
                    saved.switch_count = inner.accounts[index].switch_count;
                }
                inner.accounts[index] = saved.clone();
                saved
            }
            None => {
                let saved = account;
                if saved.active {
                    for other in inner.accounts.iter_mut() {
                        other.active = false;
                    }
                }
                inner.accounts.push(saved.clone());
                saved
            }
        };
        self.persist_accounts(&inner.accounts)?;
        Ok(saved.meta())
    }

    fn find_account(&self, account_id: &str) -> Result<CodeBuddyAccount, String> {
        let inner = self.read_inner();
        inner
            .accounts
            .iter()
            .find(|account| account.id == account_id)
            .cloned()
            .ok_or_else(|| "账号不存在。".to_string())
    }

    fn replace_account(&self, updated: CodeBuddyAccount) -> Result<(), String> {
        let mut inner = self.write_inner();
        if let Some(slot) = inner
            .accounts
            .iter_mut()
            .find(|account| account.id == updated.id)
        {
            let active = slot.active;
            *slot = updated;
            slot.active = active;
        }
        self.persist_accounts(&inner.accounts)
    }

    /// 重新从本地配置和状态中读取当前生效的 CodeBuddy 账号
    pub fn detect_current(&self) -> Result<Value, String> {
        let mut inner = self.write_inner();
        if let Some(accounts) = read_json_file(&self.accounts_file())
            .and_then(|val| serde_json::from_value::<Vec<CodeBuddyAccount>>(val).ok())
        {
            inner.accounts = accounts;
        }
        if let Some(state) = read_json_file(&self.state_file())
            .and_then(|val| serde_json::from_value::<CodeBuddyState>(val).ok())
        {
            inner.state = state;
        }

        let cli_settings = self.read_cli_settings().unwrap_or_else(|_| json!({}));
        let env_token = cli_settings
            .get("env")
            .and_then(|env| env.get("CODEBUDDY_AUTH_TOKEN"))
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("");

        let mut matched_id = None;
        if !env_token.is_empty() {
            if let Some(acc) = inner
                .accounts
                .iter()
                .find(|a| a.access_token.trim() == env_token)
            {
                matched_id = Some(acc.id.clone());
            }
        }

        if matched_id.is_none() && !inner.state.active_account_id.is_empty() {
            if inner
                .accounts
                .iter()
                .any(|a| a.id == inner.state.active_account_id)
            {
                matched_id = Some(inner.state.active_account_id.clone());
            }
        }

        for acc in inner.accounts.iter_mut() {
            acc.active = matched_id.as_deref() == Some(acc.id.as_str());
        }
        if let Some(ref id) = matched_id {
            inner.state.active_account_id = id.clone();
        }
        self.persist_accounts(&inner.accounts)?;
        self.persist_state(&inner.state)?;

        if let Some(ref id) = matched_id {
            let acc = inner.accounts.iter().find(|a| &a.id == id).cloned();
            Ok(json!({
                "ok": true,
                "matched": true,
                "id": id,
                "name": acc.as_ref().map(|a| a.display_name()).unwrap_or_else(|| id.clone()),
                "account": acc.as_ref().map(|a| a.nickname.clone()).unwrap_or_default(),
            }))
        } else {
            Ok(json!({
                "ok": true,
                "matched": false,
            }))
        }
    }

    // -----------------------------------------------------------------
    // OAuth 扫码登录
    // -----------------------------------------------------------------

    pub async fn oauth_start(&self, variant: Option<&str>) -> Result<Value, String> {
        let variant = CodeBuddyVariant::parse(variant);
        let url = format!(
            "{}{}/auth/state?platform={}",
            variant.api_endpoint(),
            PLUGIN_PREFIX,
            variant.oauth_platform()
        );
        let response = self
            .request_json(reqwest::Method::POST, &url, Some(json!({})), HeaderMap::new())
            .await;
        let data = response.get("data").cloned().unwrap_or_else(|| json!({}));
        let state = data
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        if state.is_empty() {
            return Err(format!(
                "auth/state 响应缺少 state：{}",
                serde_json::to_string(&response)
                    .unwrap_or_default()
                    .chars()
                    .take(300)
                    .collect::<String>()
            ));
        }
        let auth_url = data
            .get("authUrl")
            .and_then(Value::as_str)
            .or_else(|| data.get("auth_url").and_then(Value::as_str))
            .or_else(|| data.get("url").and_then(Value::as_str))
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}/login?state={state}", variant.api_endpoint()));
        let login_id = format!("wb_{}", new_id_simple());
        let info = OAuthInfo {
            state,
            variant,
            expires_at_secs: (now_ms() / 1000) as i64 + OAUTH_TIMEOUT_SECS,
        };
        self.oauth_states
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .insert(login_id.clone(), info);
        Ok(json!({
            "loginId": login_id,
            "verificationUri": auth_url,
            "expiresIn": OAUTH_TIMEOUT_SECS,
            "variant": variant.as_str(),
            "variantLabel": variant.label(),
        }))
    }

    /// 轮询一次 OAuth 登录状态；成功则采集资料并入库。
    pub async fn oauth_poll(&self, login_id: &str) -> Value {
        let (state, variant) = {
            let mut states = self
                .oauth_states
                .lock()
                .unwrap_or_else(|poison| poison.into_inner());
            let Some(info) = states.get(login_id) else {
                return json!({"done": true, "error": "登录请求不存在"});
            };
            if (now_ms() / 1000) as i64 > info.expires_at_secs {
                states.remove(login_id);
                return json!({"done": true, "error": "登录超时"});
            }
            (info.state.clone(), info.variant)
        };

        let token_url =
            format!("{}{}/auth/token?state={state}", variant.api_endpoint(), PLUGIN_PREFIX);
        let response = self
            .request_json(reqwest::Method::GET, &token_url, None, HeaderMap::new())
            .await;
        let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code != 0 && code != 200 {
            return json!({"done": false});
        }
        let data = response.get("data").cloned().unwrap_or_else(|| json!({}));
        let access_token = data
            .get("accessToken")
            .and_then(Value::as_str)
            .or_else(|| data.get("access_token").and_then(Value::as_str))
            .unwrap_or("")
            .trim()
            .to_string();
        if access_token.is_empty() {
            return json!({"done": false});
        }
        self.oauth_states
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .remove(login_id);

        let mut account = CodeBuddyAccount {
            id: new_id(),
            variant: Some(variant.as_str().to_string()),
            access_token,
            refresh_token: data
                .get("refreshToken")
                .and_then(Value::as_str)
                .or_else(|| data.get("refresh_token").and_then(Value::as_str))
                .unwrap_or("")
                .to_string(),
            token_type: "Bearer".to_string(),
            domain: data
                .get("domain")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            created_at: Some(now_ms() as i64),
            ..Default::default()
        };
        account.expires_at = normalize_ts(data.get("expiresAt").or_else(|| data.get("expires_at")))
            .or_else(|| {
                data.get("expiresIn")
                    .and_then(Value::as_i64)
                    .map(|secs| now_ms() as i64 + secs * 1000)
            });
        account.refresh_expires_at = normalize_ts(
            data.get("refreshExpiresAt")
                .or_else(|| data.get("refresh_expires_at")),
        )
        .or_else(|| {
            data.get("refreshExpiresIn")
                .and_then(Value::as_i64)
                .map(|secs| now_ms() as i64 + secs * 1000)
        });
        if let Some(profile) = self.fetch_account_profile(&account).await {
            apply_profile(&mut account, &profile);
        }
        match self.upsert_account(account) {
            Ok(meta) => json!({"done": true, "account": meta}),
            Err(error) => json!({"done": true, "error": format!("保存账号失败: {error}")}),
        }
    }

    /// 用 Bearer token 拉取账号资料（uid / 昵称 / 邮箱 / 企业）。
    async fn fetch_account_profile(&self, account: &CodeBuddyAccount) -> Option<Value> {
        let mut headers = auth_headers(account);
        if !account.domain.is_empty() {
            push_header(&mut headers, "X-Domain", &account.domain);
        }
        let url = format!("{}{}/login/account", account.variant().api_endpoint(), PLUGIN_PREFIX);
        let response = self
            .request_json(reqwest::Method::GET, &url, None, headers)
            .await;
        let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code != 0 && code != 200 {
            return None;
        }
        Some(response.get("data").cloned().unwrap_or_else(|| json!({})))
    }

    // -----------------------------------------------------------------
    // Token 刷新
    // -----------------------------------------------------------------

    async fn refresh_account_token(&self, mut account: CodeBuddyAccount) -> CodeBuddyAccount {
        if account.refresh_token.trim().is_empty() {
            account.needs_relogin = true;
            account.needs_relogin_reason = "缺少 refresh token，无法刷新，需重新登录".to_string();
            let _ = self.replace_account(account.clone());
            return account;
        }
        let mut headers = auth_headers(&account);
        push_header(&mut headers, "X-Refresh-Token", &account.refresh_token);
        push_header(&mut headers, "X-Auth-Refresh-Source", "plugin");
        let url =
            format!("{}{}/auth/token/refresh", account.variant().api_endpoint(), PLUGIN_PREFIX);
        let response = self
            .request_json(reqwest::Method::POST, &url, Some(json!({})), headers)
            .await;
        let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code != 0 && code != 200 {
            let message = resp_message(&response, code);
            account.needs_relogin = true;
            account.needs_relogin_reason = format!("刷新失败(code={code}): {message}");
            let _ = self.replace_account(account.clone());
            return account;
        }
        let data = response.get("data").cloned().unwrap_or_else(|| json!({}));
        let Some(new_token) = data
            .get("accessToken")
            .and_then(Value::as_str)
            .or_else(|| data.get("access_token").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
        else {
            account.needs_relogin = true;
            account.needs_relogin_reason = "刷新响应缺少 accessToken".to_string();
            let _ = self.replace_account(account.clone());
            return account;
        };
        let old_token = account.access_token.trim().to_string();
        account.access_token = new_token;
        if let Some(new_refresh) = data
            .get("refreshToken")
            .and_then(Value::as_str)
            .or_else(|| data.get("refresh_token").and_then(Value::as_str))
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            account.refresh_token = new_refresh.to_string();
        }
        let expires_at = normalize_ts(data.get("expiresAt").or_else(|| data.get("expires_at")))
            .or_else(|| {
                data.get("expiresIn")
                    .and_then(Value::as_i64)
                    .map(|secs| now_ms() as i64 + secs * 1000)
            });
        if expires_at.is_some() {
            account.expires_at = expires_at;
        }
        let refresh_expires_at = normalize_ts(
            data.get("refreshExpiresAt")
                .or_else(|| data.get("refresh_expires_at")),
        )
        .or_else(|| {
            data.get("refreshExpiresIn")
                .and_then(Value::as_i64)
                .map(|secs| now_ms() as i64 + secs * 1000)
        });
        if refresh_expires_at.is_some() {
            account.refresh_expires_at = refresh_expires_at;
        }
        account.refreshed_at = Some(now_ms() as i64);
        account.needs_relogin = false;
        account.needs_relogin_reason.clear();
        let _ = self.replace_account(account.clone());
        // 唯一成功路径：活动账号刷新后把 CLI settings.json 里的旧 env token 换成新 token。
        self.sync_active_cli_token(&old_token, account.access_token.trim());
        account
    }

    /// 惰性刷新：缺少 expiresAt 或 24 小时内到期时先刷新。
    async fn ensure_fresh_token(&self, account: CodeBuddyAccount) -> CodeBuddyAccount {
        let stale = match account.expires_at {
            Some(expires_at) => {
                (now_ms() as i64) >= expires_at || expires_at - (now_ms() as i64) < 24 * 3600 * 1000
            }
            None => true,
        };
        if stale && !account.refresh_token.trim().is_empty() {
            self.refresh_account_token(account).await
        } else {
            account
        }
    }

    // -----------------------------------------------------------------
    // 签到
    // -----------------------------------------------------------------

    async fn checkin_request(&self, path: &str, account: &CodeBuddyAccount) -> Value {
        let endpoint = account.variant().api_endpoint();
        let url = format!("{endpoint}{path}");
        let mut response = self
            .request_json(reqwest::Method::POST, &url, Some(json!({})), auth_headers(account))
            .await;
        if is_unauthorized(&response) && !account.refresh_token.trim().is_empty() {
            let refreshed = self.refresh_account_token(account.clone()).await;
            response = self
                .request_json(
                    reqwest::Method::POST,
                    &url,
                    Some(json!({})),
                    auth_headers(&refreshed),
                )
                .await;
        }
        response
    }

    async fn checkin_status(&self, account: &CodeBuddyAccount) -> Value {
        for path in [
            format!("{CHECKIN_PREFIX}/checkin-activity-status"),
            format!("{CHECKIN_PREFIX}/checkin-status"),
        ] {
            let response = self.checkin_request(&path, account).await;
            let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
            if code == 0 || code == 200 {
                let data = response.get("data").cloned().unwrap_or_else(|| json!({}));
                let today_checked_in = data
                    .get("today_checked_in")
                    .or_else(|| data.get("todayCheckedIn"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                return json!({"ok": true, "todayCheckedIn": today_checked_in});
            }
        }
        json!({"ok": false, "error": "查询签到状态失败"})
    }

    async fn perform_checkin(&self, account: &CodeBuddyAccount) -> Value {
        let response = self
            .checkin_request(&format!("{CHECKIN_PREFIX}/daily-checkin"), account)
            .await;
        let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code == 0 || code == 200 {
            return json!({"ok": true});
        }
        let message = resp_message(&response, code);
        if message.contains("已签到") || message.to_lowercase().contains("repeat") {
            return json!({"ok": true, "already": true, "message": message});
        }
        json!({"ok": false, "error": message})
    }

    /// 对单个账号执行签到：查状态 → 未签到时提交 → 写日志。
    pub async fn checkin_account(&self, account_id: &str) -> Value {
        let Ok(account) = self.find_account(account_id) else {
            return json!({"result": "error", "error": "账号不存在"});
        };
        if !account.variant().supports_checkin() {
            return json!({
                "result": "unsupported",
                "message": "国际版账号不支持每日签到",
            });
        }
        let account = self.ensure_fresh_token(account).await;
        let status = self.checkin_status(&account).await;
        if status.get("ok").and_then(Value::as_bool) != Some(true) {
            let error = status
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("查询签到状态失败")
                .to_string();
            return json!({"result": "error", "error": error});
        }
        if status.get("todayCheckedIn").and_then(Value::as_bool) == Some(true) {
            return json!({"result": "already"});
        }
        let result = self.perform_checkin(&account).await;
        let result_value = if result.get("ok").and_then(Value::as_bool) == Some(true) {
            if result.get("already").and_then(Value::as_bool) == Some(true) {
                "already"
            } else {
                "success"
            }
        } else {
            "error"
        };
        let mut entry = json!({
            "ts": now_ms() as i64,
            "accountId": account.id,
            "email": account.display_name(),
            "result": result_value,
        });
        if result_value == "error" {
            entry["error"] = result.get("error").cloned().unwrap_or(Value::Null);
        }
        self.append_checkin_log(entry);
        json!({"result": result_value, "error": result.get("error").and_then(Value::as_str)})
    }

    fn append_checkin_log(&self, entry: Value) {
        let mut inner = self.write_inner();
        inner.state.checkin_logs.push(entry);
        let overflow = inner
            .state
            .checkin_logs
            .len()
            .saturating_sub(CHECKIN_LOG_MAX);
        if overflow > 0 {
            inner.state.checkin_logs.drain(0..overflow);
        }
        let _ = self.persist_state(&inner.state);
    }

    pub fn checkin_status_summary(&self) -> Value {
        let inner = self.read_inner();
        json!({
            "auto": inner.state.auto,
            "logs": inner.state.checkin_logs.iter().rev().take(50).collect::<Vec<_>>(),
        })
    }

    // -----------------------------------------------------------------
    // 旅行
    // -----------------------------------------------------------------

    async fn travel_request(
        &self,
        path: &str,
        method: reqwest::Method,
        body: Option<Value>,
        account: &CodeBuddyAccount,
    ) -> Value {
        let endpoint = account.variant().api_endpoint();
        let url = format!("{endpoint}{path}");
        let mut headers = auth_headers(account);
        push_header(&mut headers, "x-client-platform", "web");
        push_header(&mut headers, "origin", endpoint);
        push_header(&mut headers, "referer", &format!("{endpoint}/profile/growth-center"));
        let mut response = self
            .request_json(method.clone(), &url, body.clone(), headers)
            .await;
        if is_unauthorized(&response) && !account.refresh_token.trim().is_empty() {
            let refreshed = self.refresh_account_token(account.clone()).await;
            let mut headers = auth_headers(&refreshed);
            push_header(&mut headers, "x-client-platform", "web");
            push_header(&mut headers, "origin", endpoint);
            push_header(&mut headers, "referer", &format!("{endpoint}/profile/growth-center"));
            response = self.request_json(method, &url, body, headers).await;
        }
        response
    }

    async fn travel_config(&self, account: &CodeBuddyAccount) -> Value {
        let response = self
            .travel_request(&format!("{TRAVEL_PREFIX}/config"), reqwest::Method::GET, None, account)
            .await;
        let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code == 0 || code == 200 {
            let data = response.get("data").cloned().unwrap_or_else(|| json!({}));
            let locations: Vec<Value> = data
                .get("locations")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|location| {
                    json!({
                        "id": location.get("id").cloned().unwrap_or(Value::Null),
                        "name": location.get("name").and_then(Value::as_str).unwrap_or(""),
                    })
                })
                .collect();
            let enabled = data.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            return json!({
                "ok": true,
                "enabled": enabled && !locations.is_empty(),
                "locations": locations,
            });
        }
        json!({"ok": false, "error": resp_message(&response, code)})
    }

    async fn travel_status(&self, account: &CodeBuddyAccount) -> Value {
        let response = self
            .travel_request(&format!("{TRAVEL_PREFIX}/status"), reqwest::Method::GET, None, account)
            .await;
        let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code == 0 || code == 200 {
            let data = response.get("data").cloned().unwrap_or_else(|| json!({}));
            let raw_state = data.get("state").and_then(Value::as_str).unwrap_or("");
            let state = TRAVEL_STATES
                .iter()
                .find(|candidate| **candidate == raw_state.trim().to_ascii_lowercase());
            return match state {
                Some(state) => json!({
                    "ok": true,
                    "state": state,
                    "dailyLimitReached": data
                        .get("daily_limit_reached")
                        .and_then(Value::as_bool)
                        .unwrap_or(false),
                    "recordId": data
                        .get("record_id")
                        .and_then(Value::as_i64)
                        .unwrap_or(0),
                    "locationName": data
                        .get("location")
                        .and_then(|location| location.get("name"))
                        .and_then(Value::as_str)
                        .unwrap_or(""),
                }),
                None => json!({"ok": false, "error": "旅行状态未知"}),
            };
        }
        json!({"ok": false, "error": resp_message(&response, code)})
    }

    async fn travel_depart(&self, account: &CodeBuddyAccount, location_id: &Value) -> Value {
        let response = self
            .travel_request(
                &format!("{TRAVEL_PREFIX}/depart"),
                reqwest::Method::POST,
                Some(json!({"location_id": location_id})),
                account,
            )
            .await;
        let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code == 0 || code == 200 {
            let data = response.get("data").cloned().unwrap_or_else(|| json!({}));
            return json!({
                "ok": true,
                "state": data.get("state").and_then(Value::as_str).unwrap_or("traveling"),
            });
        }
        json!({"ok": false, "code": code, "message": resp_message(&response, code)})
    }

    async fn travel_claim(&self, account: &CodeBuddyAccount, record_id: i64) -> Value {
        let payload = if record_id > 0 {
            json!({"record_id": record_id})
        } else {
            json!({})
        };
        let response = self
            .travel_request(
                &format!("{TRAVEL_PREFIX}/claim"),
                reqwest::Method::POST,
                Some(payload),
                account,
            )
            .await;
        let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code == 0 || code == 200 {
            let data = response.get("data").cloned().unwrap_or_else(|| json!({}));
            let reward = data
                .get("reward_credit")
                .and_then(Value::as_i64)
                .or_else(|| {
                    data.get("reward_credit")
                        .and_then(Value::as_f64)
                        .map(|value| value as i64)
                });
            return json!({"ok": true, "rewardCredit": reward});
        }
        json!({"ok": false, "code": code, "message": resp_message(&response, code)})
    }

    /// 对单个账号执行一步旅行状态机：arrived → 领奖；traveling → 等待；
    /// idle 未达上限 → 派发；idle + daily_limit → 当日结束。
    pub async fn travel_account(&self, account_id: &str) -> Value {
        let Ok(account) = self.find_account(account_id) else {
            return json!({"ok": false, "skip": "error", "message": "账号不存在"});
        };
        if !account.variant().supports_travel() {
            return json!({
                "ok": false,
                "skip": "unsupported",
                "message": "国际版账号不支持成长中心旅行",
            });
        }
        let account = self.ensure_fresh_token(account).await;
        let status = self.travel_status(&account).await;
        if status.get("ok").and_then(Value::as_bool) != Some(true) {
            return json!({
                "ok": false,
                "skip": "status-error",
                "message": status.get("error").and_then(Value::as_str).unwrap_or("查询旅行状态失败"),
            });
        }
        match status.get("state").and_then(Value::as_str).unwrap_or("") {
            "arrived" => {
                let record_id = status.get("recordId").and_then(Value::as_i64).unwrap_or(0);
                let claimed = self.travel_claim(&account, record_id).await;
                if claimed.get("ok").and_then(Value::as_bool) == Some(true) {
                    let entry = json!({
                        "ok": true,
                        "claimed": true,
                        "rewardCredit": claimed.get("rewardCredit").cloned().unwrap_or(Value::Null),
                        "claimedAt": now_ms() as i64,
                        "at": now_ms() as i64,
                    });
                    self.update_travel_cache(&account.id, entry.clone());
                    entry
                } else {
                    json!({
                        "ok": false,
                        "skip": "error",
                        "message": claimed.get("message").and_then(Value::as_str).unwrap_or("领取奖励失败"),
                    })
                }
            }
            "traveling" => json!({"ok": true, "state": "traveling", "message": "旅行中"}),
            _ => {
                if status
                    .get("dailyLimitReached")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    let entry = json!({
                        "ok": true,
                        "dailyDone": true,
                        "at": now_ms() as i64,
                    });
                    self.update_travel_cache(&account.id, entry.clone());
                    entry
                } else {
                    let config = self.travel_config(&account).await;
                    if config.get("ok").and_then(Value::as_bool) != Some(true)
                        || config.get("enabled").and_then(Value::as_bool) != Some(true)
                    {
                        return json!({
                            "ok": false,
                            "skip": "no-location",
                            "message": "无旅行地点或配置不可用",
                        });
                    }
                    let locations = config
                        .get("locations")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default();
                    let mut last_message = "派发失败".to_string();
                    for location in &locations {
                        let result = self.travel_depart(&account, &location["id"]).await;
                        if result.get("ok").and_then(Value::as_bool) == Some(true) {
                            let entry = json!({
                                "ok": true,
                                "state": result.get("state").and_then(Value::as_str).unwrap_or("traveling"),
                                "locationName": location.get("name").cloned().unwrap_or(Value::Null),
                                "at": now_ms() as i64,
                            });
                            self.update_travel_cache(&account.id, entry.clone());
                            return entry;
                        }
                        let message = result
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            .to_string();
                        let lowered = message.to_lowercase();
                        if lowered.contains("already traveling") {
                            return json!({"ok": true, "state": "traveling", "message": message});
                        }
                        if lowered.contains("daily limit") || lowered.contains("daily_limit") {
                            let entry =
                                json!({"ok": true, "dailyDone": true, "at": now_ms() as i64});
                            self.update_travel_cache(&account.id, entry.clone());
                            return entry;
                        }
                        // 暂无 Buddy 是可恢复状态：不能标记当日完成，否则后续轮次不再重试。
                        if lowered.contains("no buddy") || lowered.contains("no active buddy") {
                            return json!({
                                "ok": false,
                                "skip": "no-buddy",
                                "retryable": true,
                                "message": if message.is_empty() { "暂无可旅行的 Buddy".to_string() } else { message },
                            });
                        }
                        last_message = message;
                    }
                    json!({"ok": false, "skip": "error", "message": last_message})
                }
            }
        }
    }

    fn update_travel_cache(&self, account_id: &str, entry: Value) {
        let mut inner = self.write_inner();
        inner.state.travel.insert(account_id.to_string(), entry);
        let _ = self.persist_state(&inner.state);
    }

    pub fn travel_status_summary(&self) -> Value {
        let inner = self.read_inner();
        json!({
            "auto": inner.state.auto,
            "travel": inner.state.travel,
        })
    }

    // -----------------------------------------------------------------
    // 积分
    // -----------------------------------------------------------------

    /// 查询账号积分资源并缓存摘要（含分类明细与到期时间）。
    pub async fn credits_account(&self, account_id: &str) -> Value {
        let Ok(account) = self.find_account(account_id) else {
            return json!({"ok": false, "error": "账号不存在"});
        };
        let account = self.ensure_fresh_token(account).await;
        let now = now_ms() as i64;
        let summary = match self.fetch_credit_resources(&account, now).await {
            Ok(resources) => credit_summary(&resources, now),
            Err(error) => json!({"ok": false, "error": error, "updatedAt": now}),
        };
        let mut inner = self.write_inner();
        inner
            .state
            .credits
            .insert(account.id.clone(), summary.clone());
        let _ = self.persist_state(&inner.state);
        summary
    }

    /// 取积分资源明细：先走套餐页三路接口（summary / 付费包 / 免费包），
    /// 三路都不可用时回退旧接口 `/v2/billing/meter/get-user-resource`。
    async fn fetch_credit_resources(
        &self,
        account: &CodeBuddyAccount,
        now: i64,
    ) -> Result<Vec<Value>, String> {
        let summary_body = json!({});
        let paid_body = paid_packages_body();
        let free_body = free_packages_body();
        let (summary_response, paid_response, free_response) = tokio::join!(
            self.resource_post(RESOURCE_SUMMARY_PATH, summary_body.clone(), account, false),
            self.resource_post(RESOURCE_PAID_PACKAGES_PATH, paid_body.clone(), account, false,),
            self.resource_post(RESOURCE_FREE_PACKAGES_PATH, free_body.clone(), account, false,),
        );
        let responses = [summary_response, paid_response, free_response];
        let responses = if responses.iter().any(is_unauthorized)
            && !account.refresh_token.trim().is_empty()
        {
            // 三路并发各撞 401 时只刷新一次，然后只重试未授权的那一路，
            // 避免三次刷新互相覆盖刚落盘的 token。
            let refreshed = self.refresh_account_token(account.clone()).await;
            let paths = [
                RESOURCE_SUMMARY_PATH,
                RESOURCE_PAID_PACKAGES_PATH,
                RESOURCE_FREE_PACKAGES_PATH,
            ];
            let bodies = [summary_body, paid_body, free_body];
            let mut retried: Vec<Value> = Vec::with_capacity(3);
            for (index, response) in responses.into_iter().enumerate() {
                if is_unauthorized(&response) {
                    retried.push(
                        self.resource_post(paths[index], bodies[index].clone(), &refreshed, false)
                            .await,
                    );
                } else {
                    retried.push(response);
                }
            }
            [retried[0].clone(), retried[1].clone(), retried[2].clone()]
        } else {
            responses
        };
        if let Some(resources) =
            normalized_resources(&responses[0], &responses[1], &responses[2], now)
        {
            return Ok(resources);
        }
        self.legacy_credit_resources(account, now).await
    }

    /// 旧接口回退：仍然自带一次 401 刷新重试。
    async fn legacy_credit_resources(
        &self,
        account: &CodeBuddyAccount,
        now: i64,
    ) -> Result<Vec<Value>, String> {
        let now_local =
            time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
        // get-user-resource 的时间范围参数是本地语义的 "yyyy-MM-dd HH:mm:ss"。
        let begin_text = local_datetime_text(now_local);
        let end_text = local_datetime_text(now_local + time::Duration::days(365 * 101));
        let body = json!({
            "PageNumber": 1,
            "PageSize": 100,
            "ProductCode": PRODUCT_CODE,
            "Status": [0, 3],
            "PackageEndTimeRangeBegin": begin_text,
            "PackageEndTimeRangeEnd": end_text,
        });
        let response = self
            .resource_post(USER_RESOURCE_PATH, body, account, true)
            .await;
        let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
        if code != 0 && code != 200 {
            return Err(resp_message(&response, code));
        }
        let items = resource_items_of(&response);
        if items.is_empty() {
            return Err("积分接口未返回资源明细。".to_string());
        }
        Ok(items
            .into_iter()
            .map(|package| credit_resource_summary(&package, now))
            .collect())
    }

    /// 积分资源接口请求：带官方用户中心同源头；未授权时按 `allow_refresh` 刷新一次并重试；
    /// 支持按档位候选路径在 404 时自动回退。
    async fn resource_post(
        &self,
        path: &str,
        body: Value,
        account: &CodeBuddyAccount,
        allow_refresh: bool,
    ) -> Value {
        let variant = account.variant();
        let paths = variant.billing_paths(path);
        let endpoint = variant.api_endpoint();
        let build_headers = |account: &CodeBuddyAccount| {
            let mut headers = auth_headers(account);
            push_header(&mut headers, "x-client-platform", "web");
            push_header(&mut headers, "origin", endpoint);
            push_header(&mut headers, "referer", &format!("{endpoint}/profile/plans-usage"));
            headers
        };

        let mut last_response = json!({});
        for p in &paths {
            let url = format!("{endpoint}{p}");
            let response = self
                .request_json(
                    reqwest::Method::POST,
                    &url,
                    Some(body.clone()),
                    build_headers(account),
                )
                .await;
            if allow_refresh
                && is_unauthorized(&response)
                && !account.refresh_token.trim().is_empty()
            {
                let refreshed = self.refresh_account_token(account.clone()).await;
                let retried = self
                    .request_json(
                        reqwest::Method::POST,
                        &url,
                        Some(body.clone()),
                        build_headers(&refreshed),
                    )
                    .await;
                if is_route_missing(&retried) && paths.len() > 1 {
                    last_response = retried;
                    continue;
                }
                return retried;
            }
            if is_route_missing(&response) && paths.len() > 1 {
                last_response = response;
                continue;
            }
            return response;
        }
        last_response
    }

    // -----------------------------------------------------------------
    // 切换 CodeBuddy CLI 账号
    // -----------------------------------------------------------------

    pub fn switch_account(&self, account_id: &str) -> Result<Value, String> {
        let account = self.find_account(account_id)?;
        if account.access_token.trim().is_empty() {
            return Err("该账号没有 access token，无法切换。".to_string());
        }
        self.install_cli_credentials(&account)?;
        {
            let mut inner = self.write_inner();
            inner.state.active_account_id = account.id.clone();
            for other in inner.accounts.iter_mut() {
                other.active = other.id == account.id;
                if other.id == account.id {
                    other.switch_count += 1;
                }
            }
            self.persist_accounts(&inner.accounts)?;
            self.persist_state(&inner.state)?;
        }
        Ok(json!({
            "ok": true,
            "id": account.id,
            "name": account.display_name(),
        }))
    }

    /// 把账号写入真实共享的 CodeBuddy CLI 配置：Unix 与 Windows 统一写
    /// settings.json 的 env.CODEBUDDY_AUTH_TOKEN 与区域环境变量，
    /// 并同步 ~/.codebuddy/local_storage 运行时缓存。
    /// 已配置第三方 apiKeyHelper（路径不含 .codebuddy-rotate）时拒绝切换；
    /// 本家历史 helper 或残留 apiKeyHelper 键会被移除。
    fn install_cli_credentials(&self, account: &CodeBuddyAccount) -> Result<(), String> {
        let variant = account.variant();
        let settings_dir = self.user_home.join(".codebuddy");
        let settings_file = settings_dir.join("settings.json");
        std::fs::create_dir_all(&settings_dir)
            .map_err(|error| format!("无法创建 {settings_dir:?}: {error}"))?;
        let previous = std::fs::read_to_string(&settings_file).ok();
        let mut settings: Value = previous
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|_| "CodeBuddy settings.json 不是有效 JSON")?
            .unwrap_or_else(|| json!({}));
        if !settings.is_object() {
            return Err("CodeBuddy settings.json 顶层不是对象".to_string());
        }

        // 保护性检查：第三方 apiKeyHelper 不擅自覆盖；本家历史 helper
        // （路径含 .codebuddy-rotate）或空值则移除该键，统一走 env token。
        let existing_helper = settings
            .get("apiKeyHelper")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        if !existing_helper.is_empty() && !existing_helper.contains(".codebuddy-rotate") {
            return Err("CodeBuddy settings.json 已配置其他 apiKeyHelper，请先确认后移除再切换。"
                .to_string());
        }
        if let Some(object) = settings.as_object_mut() {
            object.remove("apiKeyHelper");
        }

        let env = settings
            .as_object_mut()
            .unwrap()
            .entry("env")
            .or_insert_with(|| json!({}));
        if !env.is_object() {
            return Err("CodeBuddy settings.json 的 env 字段不是对象".to_string());
        }
        let env_obj = env.as_object_mut().unwrap();
        env_obj.insert("CODEBUDDY_AUTH_TOKEN".to_string(), json!(account.access_token.trim()));
        env_obj.insert(
            "CODEBUDDY_INTERNET_ENVIRONMENT".to_string(),
            json!(variant.cli_internet_environment()),
        );
        match variant {
            CodeBuddyVariant::Cn => {
                env_obj.remove("CODEBUDDY_BASE_URL");
            }
            CodeBuddyVariant::Ai => {
                if let Some(openai_base) = variant.cli_openai_base_url() {
                    env_obj.insert("CODEBUDDY_BASE_URL".to_string(), json!(openai_base));
                }
            }
        }

        let content = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
        atomic_write(&settings_file, &content)?;

        // 同步 local_storage 缓存，避免 CLI 启动时 ProductManager 读取陈旧环境缓存
        let storage_dir = settings_dir.join("local_storage");
        let _ = std::fs::create_dir_all(&storage_dir);
        let env_path = storage_dir.join(CLI_ENV_CACHE_FILE);
        let endpoint_path = storage_dir.join(CLI_ENDPOINT_CACHE_FILE);
        let product_path = storage_dir.join(CLI_PRODUCT_CACHE_FILE);
        let _ = std::fs::remove_file(&product_path);
        let _ = write_cli_json_string_cache(&env_path, variant.cli_internet_environment());
        let _ = write_cli_json_string_cache(&endpoint_path, variant.cli_endpoint());

        info!(
            "codebuddy: 已切换 CLI 账号到 {}（{}，{}）",
            account.display_name(),
            account.id,
            variant.label()
        );
        Ok(())
    }

    /// token 保活同步：活动账号刷新成功后，仅当 settings.json 的
    /// env.CODEBUDDY_AUTH_TOKEN 仍等于刷新前的旧 token 时才改写为新 token；
    /// 同时移除本家 apiKeyHelper（路径含 .codebuddy-rotate），第三方 helper 不动。
    /// 所有 IO/JSON 错误只记 warn!，不影响刷新结果。
    fn sync_active_cli_token(&self, old_token: &str, new_token: &str) {
        if old_token.is_empty() || new_token.is_empty() || old_token == new_token {
            return;
        }
        // 只有本次刷新的账号仍是当前活动账号时才同步 CLI 配置。
        let active_matches = {
            let inner = self.read_inner();
            inner
                .accounts
                .iter()
                .find(|account| account.id == inner.state.active_account_id)
                .is_some_and(|account| account.access_token.trim() == new_token)
        };
        if !active_matches {
            return;
        }
        let settings_file = self.user_home.join(".codebuddy").join("settings.json");
        let mut settings = match self.read_cli_settings() {
            Ok(settings) => settings,
            Err(error) => {
                warn!("codebuddy: 同步 CLI token 读取 settings.json 失败: {error}");
                return;
            }
        };
        let Some(object) = settings.as_object_mut() else {
            return;
        };
        let env_matches_old = object
            .get("env")
            .and_then(|env| env.get("CODEBUDDY_AUTH_TOKEN"))
            .and_then(Value::as_str)
            .map(str::trim)
            .is_some_and(|token| token == old_token);
        if !env_matches_old {
            return;
        }
        if let Some(env) = object.get_mut("env").and_then(Value::as_object_mut) {
            env.insert("CODEBUDDY_AUTH_TOKEN".to_string(), json!(new_token));
        }
        let own_helper = object
            .get("apiKeyHelper")
            .and_then(Value::as_str)
            .is_some_and(|path| path.contains(".codebuddy-rotate"));
        if own_helper {
            object.remove("apiKeyHelper");
        }
        match serde_json::to_string_pretty(&settings) {
            Ok(content) => {
                if let Err(error) = atomic_write(&settings_file, &content) {
                    warn!("codebuddy: 同步 CLI token 写入 settings.json 失败: {error}");
                }
            }
            Err(error) => warn!("codebuddy: 同步 CLI token 序列化失败: {error}"),
        }
    }

    // -----------------------------------------------------------------
    // 自动循环
    // -----------------------------------------------------------------

    pub fn auto_config(&self) -> Value {
        let inner = self.read_inner();
        json!({
            "auto": inner.state.auto,
            "lastCheckinRunMs": inner.state.last_checkin_run_ms,
            "lastTravelRunMs": inner.state.last_travel_run_ms,
            "lastCreditsRunMs": inner.state.last_credits_run_ms,
            "accountCount": inner.accounts.len(),
        })
    }

    pub fn set_auto_config(&self, checkin: bool, travel: bool) -> Result<Value, String> {
        let mut inner = self.write_inner();
        inner.state.auto = AutoConfig { checkin, travel };
        self.persist_state(&inner.state)?;
        Ok(json!({"ok": true, "auto": inner.state.auto}))
    }

    // -----------------------------------------------------------------
    // CLI 选项（真实 ~/.codebuddy/settings.json 是权威来源）
    // -----------------------------------------------------------------

    fn read_cli_settings(&self) -> Result<Value, String> {
        let settings_file = self.user_home.join(".codebuddy").join("settings.json");
        let previous = std::fs::read_to_string(&settings_file).ok();
        let settings: Value = previous
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|_| "CodeBuddy settings.json 不是有效 JSON")?
            .unwrap_or_else(|| json!({}));
        if !settings.is_object() {
            return Err("CodeBuddy settings.json 顶层不是对象".to_string());
        }
        Ok(settings)
    }

    /// 读取 CLI 选项：permissions.defaultMode 是否为 bypassPermissions，
    /// 以及 env.CODEBUDDY_IS_SANDBOX 是否开启（危险操作也不询问，仅隔离环境）。
    pub fn cli_options(&self) -> Value {
        let settings = self.read_cli_settings().ok();
        let bypass = settings
            .as_ref()
            .and_then(|settings| {
                settings
                    .get("permissions")
                    .and_then(|permissions| permissions.get("defaultMode"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .map(|mode| mode == "bypassPermissions")
            .unwrap_or(false);
        let sandbox = settings
            .as_ref()
            .and_then(|settings| {
                settings
                    .get("env")
                    .and_then(|env| env.get("CODEBUDDY_IS_SANDBOX"))
                    .and_then(Value::as_str)
            })
            .map(|value| value == "1")
            .unwrap_or(false);
        json!({"bypassPermissions": bypass, "sandbox": sandbox})
    }

    /// 写入或撤销 CLI 选项，其余 settings.json 内容保持不变：
    /// - bypassPermissions：把 permissions.defaultMode 设为 bypassPermissions 或移除；
    /// - sandbox：把 env.CODEBUDDY_IS_SANDBOX 设为 "1" 或移除。注意 CodeBuddy CLI
    ///   只从进程环境变量读取该开关（help 原文 "process env only"），settings.json
    ///   的 env 不生效，因此同时向 ~/.bashrc 写入/移除 export 行，新终端会话生效。
    pub fn set_cli_options(
        &self,
        bypass_permissions: bool,
        sandbox: bool,
    ) -> Result<Value, String> {
        let mut settings = self.read_cli_settings()?;
        {
            let object = settings
                .as_object_mut()
                .ok_or("CodeBuddy settings.json 顶层不是对象")?;
            if bypass_permissions {
                let permissions = object.entry("permissions").or_insert_with(|| json!({}));
                if !permissions.is_object() {
                    return Err("CodeBuddy settings.json 的 permissions 字段不是对象".to_string());
                }
                permissions
                    .as_object_mut()
                    .unwrap()
                    .insert("defaultMode".to_string(), json!("bypassPermissions"));
                permissions
                    .as_object_mut()
                    .unwrap()
                    .insert("allow".to_string(), json!(bypass_permission_allow_rules()));
            } else if let Some(permissions) =
                object.get_mut("permissions").and_then(Value::as_object_mut)
            {
                permissions.remove("defaultMode");
                permissions.remove("allow");
            }
            let env = object.entry("env").or_insert_with(|| json!({}));
            if !env.is_object() {
                return Err("CodeBuddy settings.json 的 env 字段不是对象".to_string());
            }
            let env = env.as_object_mut().unwrap();
            if sandbox {
                env.insert("CODEBUDDY_IS_SANDBOX".to_string(), json!("1"));
            } else {
                env.remove("CODEBUDDY_IS_SANDBOX");
            }
        }
        let settings_file = self.user_home.join(".codebuddy").join("settings.json");
        let content = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
        atomic_write(&settings_file, &content)?;
        self.sync_sandbox_bashrc(sandbox)?;
        info!(
            "codebuddy: 已更新 CLI 选项 bypassPermissions={bypass_permissions}, sandbox={sandbox}"
        );
        Ok(json!({
            "ok": true,
            "bypassPermissions": bypass_permissions,
            "sandbox": sandbox,
        }))
    }

    /// 把 CODEBUDDY_IS_SANDBOX 的 export 写入/移出 ~/.bashrc。
    /// CodeBuddy CLI 只读进程环境变量，交互式 shell 启动时必须已经导出该变量。
    fn sync_sandbox_bashrc(&self, sandbox: bool) -> Result<(), String> {
        const MARKER: &str = "# CodeBuddy isolated-sandbox full pass";
        let bashrc = self.user_home.join(".bashrc");
        let content = fs::read_to_string(&bashrc).unwrap_or_default();
        let has_env = content.contains("CODEBUDDY_IS_SANDBOX=");
        if sandbox && has_env {
            return Ok(());
        }
        if !sandbox && !has_env && !content.contains(MARKER) {
            return Ok(());
        }
        let next = if sandbox {
            let mut next = content.trim_end().to_string();
            next.push_str(
                "\n\n# CodeBuddy isolated-sandbox full pass（危险也不问，仅隔离环境）\nexport CODEBUDDY_IS_SANDBOX=1\n",
            );
            next
        } else {
            content
                .lines()
                .filter(|line| !line.contains("CODEBUDDY_IS_SANDBOX") && !line.contains(MARKER))
                .collect::<Vec<_>>()
                .join("\n")
                + "\n"
        };
        atomic_write(&bashrc, &next)
    }

    /// 执行一轮自动维护：签到补签、旅行状态机推进、积分刷新、每日 token 保活。
    async fn run_auto_cycle(&self) {
        let (
            checkin_enabled,
            travel_enabled,
            last_checkin,
            last_travel,
            last_credits,
            last_keepalive_day,
        ) = {
            let inner = self.read_inner();
            (
                inner.state.auto.checkin,
                inner.state.auto.travel,
                inner.state.last_checkin_run_ms,
                inner.state.last_travel_run_ms,
                inner.state.last_credits_run_ms,
                inner.state.last_keepalive_day.clone(),
            )
        };
        let now = now_ms() as i64;
        if checkin_enabled && now - last_checkin >= CHECKIN_RETRY_INTERVAL_MS {
            let account_ids: Vec<String> = {
                let inner = self.read_inner();
                inner
                    .accounts
                    .iter()
                    .filter(|account| account.variant().supports_checkin())
                    .map(|account| account.id.clone())
                    .collect()
            };
            for account_id in &account_ids {
                let result = self.checkin_account(account_id).await;
                if result.get("result").and_then(Value::as_str) == Some("error") {
                    let error = result.get("error").and_then(Value::as_str).unwrap_or("");
                    warn!("codebuddy: 自动签到失败 {account_id}: {error}");
                }
            }
            let mut inner = self.write_inner();
            inner.state.last_checkin_run_ms = now;
            let _ = self.persist_state(&inner.state);
        }
        if travel_enabled && now - last_travel >= TRAVEL_RETRY_INTERVAL_MS {
            let account_ids: Vec<String> = {
                let inner = self.read_inner();
                inner
                    .accounts
                    .iter()
                    .filter(|account| account.variant().supports_travel())
                    .map(|account| account.id.clone())
                    .collect()
            };
            for account_id in &account_ids {
                let result = self.travel_account(account_id).await;
                if result.get("ok").and_then(Value::as_bool) != Some(true) {
                    let message = result.get("message").and_then(Value::as_str).unwrap_or("");
                    warn!("codebuddy: 自动旅行异常 {account_id}: {message}");
                }
            }
            let mut inner = self.write_inner();
            inner.state.last_travel_run_ms = now;
            let _ = self.persist_state(&inner.state);
        }
        // 积分刷新：列展示依赖快照，周期性兜底刷新保证列表里的总量/分类/到期不长期陈旧。
        if now - last_credits >= CREDITS_RETRY_INTERVAL_MS {
            let account_ids: Vec<String> = {
                let inner = self.read_inner();
                inner
                    .accounts
                    .iter()
                    .map(|account| account.id.clone())
                    .collect()
            };
            for account_id in &account_ids {
                let summary = self.credits_account(account_id).await;
                if summary.get("ok").and_then(Value::as_bool) != Some(true) {
                    let error = summary.get("error").and_then(Value::as_str).unwrap_or("");
                    warn!("codebuddy: 自动积分刷新失败 {account_id}: {error}");
                }
            }
            let mut inner = self.write_inner();
            inner.state.last_credits_run_ms = now;
            let _ = self.persist_state(&inner.state);
        }
        // 每日保活：无条件刷新所有带 refresh token 的账号，避免闲置会话被清理。
        let today = local_day_string();
        if last_keepalive_day != today {
            let accounts: Vec<CodeBuddyAccount> = {
                let inner = self.read_inner();
                inner
                    .accounts
                    .iter()
                    .filter(|account| !account.refresh_token.trim().is_empty())
                    .cloned()
                    .collect()
            };
            for account in accounts {
                self.refresh_account_token(account).await;
            }
            let mut inner = self.write_inner();
            inner.state.last_keepalive_day = today;
            let _ = self.persist_state(&inner.state);
        }
    }

    /// 启动后台自动循环（签到 / 旅行 / 保活）。
    pub fn spawn_auto_runner(&self) {
        let manager = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(AUTO_CYCLE_TICK);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                manager.run_auto_cycle().await;
            }
        });
    }

    // -----------------------------------------------------------------
    // HTTP 基础
    // -----------------------------------------------------------------

    async fn request_json(
        &self,
        method: reqwest::Method,
        url: &str,
        body: Option<Value>,
        headers: reqwest::header::HeaderMap,
    ) -> Value {
        let mut request = self.client.request(method, url).headers(headers);
        if let Some(body) = body {
            request = request.json(&body);
        }
        match request.send().await {
            Ok(response) => match response.json::<Value>().await {
                Ok(value) => value,
                Err(error) => json!({"code": -1, "message": format!("响应解析失败: {error}")}),
            },
            Err(error) => json!({"code": -1, "message": format!("请求失败: {error}")}),
        }
    }
}

// ---------------------------------------------------------------------------
// 辅助函数
// ---------------------------------------------------------------------------

type HeaderMap = reqwest::header::HeaderMap;

fn now_ms() -> u64 {
    current_timestamp_millis()
}

fn new_id() -> String {
    format!("cb-{}", new_id_simple())
}

fn new_id_simple() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    (0..16)
        .map(|_| format!("{:x}", rng.gen_range(0..16)))
        .collect()
}

fn push_header(headers: &mut reqwest::header::HeaderMap, name: &str, value: &str) {
    if let (Ok(name), Ok(value)) = (
        reqwest::header::HeaderName::from_bytes(name.as_bytes()),
        reqwest::header::HeaderValue::from_str(value),
    ) {
        headers.insert(name, value);
    }
}

fn auth_headers(account: &CodeBuddyAccount) -> reqwest::header::HeaderMap {
    let mut headers = reqwest::header::HeaderMap::new();
    push_header(
        &mut headers,
        "Authorization",
        &format!("Bearer {}", account.access_token.trim()),
    );
    push_header(&mut headers, "Accept", "application/json");
    push_header(&mut headers, "Content-Type", "application/json");
    if !account.uid.trim().is_empty() {
        push_header(&mut headers, "X-User-Id", account.uid.trim());
    }
    let enterprise = if account.enterprise_id.trim().is_empty() {
        None
    } else {
        Some(account.enterprise_id.trim())
    };
    if let Some(enterprise) = enterprise {
        push_header(&mut headers, "X-Enterprise-Id", enterprise);
        push_header(&mut headers, "X-Tenant-Id", enterprise);
    }
    if !account.domain.trim().is_empty() {
        push_header(&mut headers, "X-Domain", account.domain.trim());
    }
    headers
}

fn is_unauthorized(response: &Value) -> bool {
    let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code == 401 || code == 403 {
        return true;
    }
    let message = resp_message(response, code).to_lowercase();
    ["unauthorized", "401", "登录", "失效", "过期", "token"]
        .iter()
        .any(|keyword| message.contains(keyword))
}

fn is_route_missing(response: &Value) -> bool {
    let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code == 404 || code == 10086 {
        return true;
    }
    if let Some(status) = response.get("status").and_then(Value::as_i64) {
        if status == 404 {
            return true;
        }
    }
    let message = resp_message(response, code).to_lowercase();
    message.contains("404") || message.contains("not found")
}

fn resp_message(response: &Value, code: i64) -> String {
    response
        .get("message")
        .or_else(|| response.get("msg"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| format!("code={code}"))
}

fn clean_bearer_token(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let stripped = trimmed
        .strip_prefix("Bearer ")
        .or_else(|| trimmed.strip_prefix("bearer "))
        .unwrap_or(trimmed);
    Some(stripped.trim().to_string())
}

fn extract_jwt_claims(token: &str) -> Option<Value> {
    let clean = clean_bearer_token(token)?;
    let parts: Vec<&str> = clean.split('.').collect();
    if parts.len() < 2 {
        return None;
    }
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .or_else(|_| base64::engine::general_purpose::STANDARD.decode(parts[1]))
        .ok()?;
    serde_json::from_slice::<Value>(&bytes).ok()
}

fn identity_email(account: &CodeBuddyAccount) -> Option<String> {
    for candidate in [&account.email, &account.nickname] {
        let trimmed = candidate.trim();
        if !trimmed.is_empty()
            && trimmed.contains('@')
            && !trimmed.eq_ignore_ascii_case("unknown")
            && trimmed != "手动添加"
        {
            return Some(trimmed.to_ascii_lowercase());
        }
    }
    if let Some(claims) = extract_jwt_claims(&account.access_token) {
        if let Some(email) = claims
            .get("email")
            .or_else(|| claims.get("preferred_username"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty() && s.contains('@'))
        {
            return Some(email.to_ascii_lowercase());
        }
    }
    None
}

fn parse_codebuddy_import_account(raw: &Value) -> Option<CodeBuddyAccount> {
    let obj = raw.as_object()?;
    let auth_obj = obj
        .get("auth")
        .or_else(|| obj.get("auth_raw"))
        .and_then(Value::as_object);
    let account_obj = obj
        .get("account")
        .or_else(|| obj.get("profile_raw"))
        .and_then(Value::as_object);

    let access_token = obj
        .get("access_token")
        .or_else(|| obj.get("accessToken"))
        .or_else(|| obj.get("token"))
        .or_else(|| {
            auth_obj.and_then(|a| {
                a.get("accessToken")
                    .or_else(|| a.get("access_token"))
                    .or_else(|| a.get("token"))
            })
        })
        .and_then(Value::as_str)
        .and_then(clean_bearer_token)?;

    let refresh_token = obj
        .get("refresh_token")
        .or_else(|| obj.get("refreshToken"))
        .or_else(|| auth_obj.and_then(|a| a.get("refreshToken").or_else(|| a.get("refresh_token"))))
        .and_then(Value::as_str)
        .and_then(clean_bearer_token)
        .unwrap_or_default();

    let jwt_claims = extract_jwt_claims(&access_token);

    let id = obj
        .get("id")
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(new_id);

    let uid = obj
        .get("uid")
        .or_else(|| account_obj.and_then(|a| a.get("uid")))
        .and_then(Value::as_str)
        .or_else(|| {
            jwt_claims
                .as_ref()
                .and_then(|c| c.get("sub").and_then(Value::as_str))
        })
        .unwrap_or("")
        .trim()
        .to_string();

    let mut email = obj
        .get("email")
        .or_else(|| account_obj.and_then(|a| a.get("email")))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();

    if email.is_empty() || !email.contains('@') {
        if let Some(ref claims) = jwt_claims {
            if let Some(jwt_email) = claims
                .get("email")
                .or_else(|| claims.get("preferred_username"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.contains('@'))
            {
                email = jwt_email.to_string();
            }
        }
    }

    let mut nickname = obj
        .get("nickname")
        .or_else(|| obj.get("name"))
        .or_else(|| account_obj.and_then(|a| a.get("nickname").or_else(|| a.get("name"))))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();

    if nickname.is_empty() {
        if let Some(ref claims) = jwt_claims {
            if let Some(name) = claims
                .get("name")
                .or_else(|| claims.get("preferred_username"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                nickname = name.to_string();
            }
        }
    }

    if email.is_empty() && nickname.contains('@') {
        email = nickname.clone();
    }

    let domain = obj
        .get("domain")
        .or_else(|| auth_obj.and_then(|a| a.get("domain")))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();

    let variant_str = obj
        .get("variant")
        .or_else(|| auth_obj.and_then(|a| a.get("variant").or_else(|| a.get("realm"))))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let variant = if let Some(v) = variant_str {
        Some(CodeBuddyVariant::parse(Some(v)).as_str().to_string())
    } else if !domain.is_empty() {
        Some(CodeBuddyVariant::from_domain(&domain).as_str().to_string())
    } else {
        Some(CodeBuddyVariant::Cn.as_str().to_string())
    };

    let created_at = normalize_ts(
        obj.get("createdAt")
            .or_else(|| obj.get("created_at"))
            .or_else(|| auth_obj.and_then(|a| a.get("createdAt").or_else(|| a.get("created_at")))),
    )
    .unwrap_or_else(|| now_ms() as i64);

    let expires_at = normalize_ts(
        obj.get("expiresAt")
            .or_else(|| obj.get("expires_at"))
            .or_else(|| auth_obj.and_then(|a| a.get("expiresAt").or_else(|| a.get("expires_at")))),
    )
    .or_else(|| {
        let in_secs = obj
            .get("expiresIn")
            .or_else(|| obj.get("expires_in"))
            .or_else(|| auth_obj.and_then(|a| a.get("expiresIn").or_else(|| a.get("expires_in"))))
            .and_then(Value::as_i64)?;
        Some(created_at + in_secs * 1000)
    })
    .or_else(|| {
        jwt_claims
            .as_ref()
            .and_then(|c| c.get("exp").and_then(Value::as_i64).map(|secs| secs * 1000))
    });

    let refresh_expires_at = normalize_ts(
        obj.get("refreshExpiresAt")
            .or_else(|| obj.get("refresh_expires_at"))
            .or_else(|| {
                auth_obj.and_then(|a| {
                    a.get("refreshExpiresAt")
                        .or_else(|| a.get("refresh_expires_at"))
                })
            }),
    )
    .or_else(|| {
        let refresh_in_secs = obj
            .get("refreshExpiresIn")
            .or_else(|| obj.get("refresh_expires_in"))
            .or_else(|| {
                auth_obj.and_then(|a| {
                    a.get("refreshExpiresIn")
                        .or_else(|| a.get("refresh_expires_in"))
                })
            })
            .and_then(Value::as_i64)?;
        Some(created_at + refresh_in_secs * 1000)
    });

    let enterprise_id = obj
        .get("enterpriseId")
        .or_else(|| obj.get("enterprise_id"))
        .or_else(|| {
            account_obj.and_then(|a| a.get("enterpriseId").or_else(|| a.get("enterprise_id")))
        })
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();

    let enterprise_name = obj
        .get("enterpriseName")
        .or_else(|| obj.get("enterprise_name"))
        .or_else(|| {
            account_obj.and_then(|a| a.get("enterpriseName").or_else(|| a.get("enterprise_name")))
        })
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();

    let token_type = obj
        .get("token_type")
        .or_else(|| obj.get("tokenType"))
        .or_else(|| auth_obj.and_then(|a| a.get("tokenType").or_else(|| a.get("token_type"))))
        .and_then(Value::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Bearer")
        .trim()
        .to_string();

    Some(CodeBuddyAccount {
        id,
        uid,
        email,
        nickname,
        enterprise_id,
        enterprise_name,
        domain,
        variant,
        access_token,
        refresh_token,
        token_type,
        expires_at,
        refresh_expires_at,
        created_at: Some(created_at),
        ..Default::default()
    })
}

fn apply_profile(account: &mut CodeBuddyAccount, profile: &Value) {
    account.uid = profile
        .get("uid")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    account.nickname = profile
        .get("nickname")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if let Some(email) = profile.get("email").and_then(Value::as_str).map(str::trim) {
        if !email.is_empty() {
            account.email = email.to_string();
        }
    }
    account.enterprise_name = profile
        .get("enterpriseName")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    account.enterprise_id = profile
        .get("enterpriseId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
}

fn normalize_ts(value: Option<&Value>) -> Option<i64> {
    let raw = value?;
    if let Some(secs) = raw.as_i64() {
        // 秒级时间戳（约 1971-2286 年范围）转换为毫秒。
        if secs < 10_000_000_000 {
            return Some(secs * 1000);
        }
        return Some(secs);
    }
    raw.as_f64().map(|value| value as i64)
}

/// 资源明细数组：官方不同接口分别用 `Packages`（summary）与 `Accounts`（付费/免费包、旧接口）。
fn resource_items_of(response: &Value) -> Vec<Value> {
    for path in [
        &["data", "Accounts"][..],
        &["data", "data", "Accounts"][..],
        &["data", "Response", "Data", "Accounts"][..],
        &["data", "data", "Response", "Data", "Accounts"][..],
        &["data", "accounts"][..],
        &["data", "data", "accounts"][..],
        &["data", "Packages"][..],
        &["data", "data", "Packages"][..],
        &["data", "Response", "Data", "Packages"][..],
        &["data", "data", "Response", "Data", "Packages"][..],
        &["data", "packages"][..],
        &["data", "data", "packages"][..],
    ] {
        let mut current = response;
        let mut ok = true;
        for key in path {
            match current.get(key) {
                Some(value) => current = value,
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if ok && current.is_array() {
            return current.as_array().cloned().unwrap_or_default();
        }
    }
    Vec::new()
}

fn first_number(raw: &Value, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(value) = raw.get(*key) {
            if let Some(number) = value.as_f64() {
                return Some(number);
            }
            if let Some(text) = value.as_str() {
                if let Ok(number) = text.trim().parse::<f64>() {
                    return Some(number);
                }
            }
        }
    }
    None
}

fn package_totals(package: &Value) -> (f64, f64) {
    let slice = package
        .get("SlicePeriodUsageDetails")
        .or_else(|| package.get("slicePeriodUsageDetails"))
        .and_then(Value::as_array)
        .and_then(|items| items.first());
    let total_keys = [
        "CycleCapacitySizePrecise",
        "CycleCapacitySize",
        "CycleTotalCapacity",
        "CapacitySizePrecise",
        "CapacitySize",
        "SlicePeriodCapacitySizePrecise",
        "SlicePeriodCapacitySize",
    ];
    let remaining_keys = [
        "CycleCapacityRemainPrecise",
        "CycleCapacityRemain",
        "CycleRemainCapacity",
        "CapacityRemainPrecise",
        "CapacityRemain",
        "SlicePeriodCapacityRemainPrecise",
        "SlicePeriodCapacityRemain",
    ];
    let used_keys = [
        "CycleCapacityUsedPrecise",
        "CycleCapacityUsed",
        "CycleUsedCapacity",
        "CapacityUsedPrecise",
        "CapacityUsed",
        "SlicePeriodCapacityUsedPrecise",
        "SlicePeriodCapacityUsed",
    ];
    let source = slice.unwrap_or(package);
    let raw_total = first_number(source, &total_keys);
    let raw_remaining = first_number(source, &remaining_keys);
    let raw_used = first_number(source, &used_keys);
    let total = raw_total
        .or_else(|| {
            raw_remaining
                .zip(raw_used)
                .map(|(remaining, used)| remaining + used)
        })
        .or(raw_remaining)
        .or(raw_used)
        .unwrap_or(0.0)
        .max(0.0);
    let remaining = raw_remaining
        .unwrap_or_else(|| (total - raw_used.unwrap_or(0.0)).max(0.0))
        .max(0.0);
    (total, remaining)
}

/// 按候选键解析时间戳（毫秒）。
fn parse_ts_value(raw: &Value, keys: &[&str]) -> Option<i64> {
    for key in keys {
        let Some(value) = raw.get(*key) else {
            continue;
        };
        if let Some(text) = value.as_str() {
            if let Some(parsed) = parse_datetime_text_ms(text) {
                return Some(parsed);
            }
            continue;
        }
        if let Some(parsed) = normalize_ts(Some(value)) {
            return Some(parsed);
        }
    }
    None
}

/// 资源到期时间：优先 DeductionEndTime/ExpiredTime；仅当 CycleEndTime 比其早超过
/// `EXPIRY_CYCLE_OVERRIDE_DAYS` 时改用周期结束时间（视前者为长期占位值）。
/// 超过 `FAR_FUTURE_EXPIRY_DAYS` 的占位值（如 2049）视为长期有效，返回 None。
fn resolve_expire_ms(package: &Value, now: i64) -> Option<i64> {
    let deduction_end = parse_ts_value(
        package,
        &[
            "DeductionEndTime",
            "deductionEndTime",
            "ExpiredTime",
            "expiredTime",
        ],
    );
    let cycle_end = parse_ts_value(package, &["CycleEndTime", "cycleEndTime"]);
    let override_ms = EXPIRY_CYCLE_OVERRIDE_DAYS * 24 * 3600 * 1000;
    let expire_at = match (deduction_end, cycle_end) {
        (Some(deduction), Some(cycle)) if deduction.saturating_sub(cycle) > override_ms => {
            Some(cycle)
        }
        (Some(deduction), _) => Some(deduction),
        (None, cycle) => cycle,
    };
    let far_future_ms = FAR_FUTURE_EXPIRY_DAYS * 24 * 3600 * 1000;
    expire_at.filter(|value| value.saturating_sub(now) <= far_future_ms)
}

/// 单个资源包 → 前端可直接展示的明细（分类名、用量、到期）。
fn credit_resource_summary(package: &Value, now: i64) -> Value {
    let (total, remaining) = package_totals(package);
    let used = (total - remaining).max(0.0);
    let expire_at = resolve_expire_ms(package, now);
    let expired = expire_at.map(|value| value <= now).unwrap_or(false);
    let expiring_soon = expire_at
        .map(|value| value > now && value - now <= EXPIRING_SOON_DAYS * 24 * 3600 * 1000)
        .unwrap_or(false);
    let package_code = package
        .get("PackageCode")
        .or_else(|| package.get("packageCode"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let package_name = credit_package_name(&package_code)
        .map(str::to_string)
        .or_else(|| {
            package
                .get("PackageName")
                .or_else(|| package.get("packageName"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| package_code.clone());
    json!({
        "packageCode": package_code,
        "packageName": package_name,
        "category": package_category(&package_code),
        "total": total,
        "remaining": remaining,
        "used": used,
        "expireAt": expire_at,
        "expired": expired,
        "expiringSoon": expiring_soon,
    })
}

/// 官方商品码 → 官方中文名；未登记返回 None（由调用方回落 PackageName）。
fn credit_package_name(package_code: &str) -> Option<&'static str> {
    CREDIT_PACKAGE_NAMES
        .iter()
        .find(|(code, _)| *code == package_code)
        .map(|(_, name)| *name)
}

/// 资源分类：付费包 / 免费包 / 未登记码。
fn package_category(package_code: &str) -> &'static str {
    if PAID_PACKAGE_CODES.contains(&package_code) {
        "paid"
    } else if FREE_PACKAGE_CODES.contains(&package_code) {
        "free"
    } else {
        "other"
    }
}

fn paid_packages_body() -> Value {
    json!({
        "PageNumber": 1,
        "PageSize": 200,
        "Status": [0, 3],
        "PackageCodes": PAID_PACKAGE_CODES,
        "NeedRenewInfo": true,
    })
}

fn free_packages_body() -> Value {
    let now_local =
        time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc());
    let start =
        now_local.replace_time(time::Time::from_hms(0, 0, 0).unwrap_or(time::Time::MIDNIGHT));
    let end =
        now_local.replace_time(time::Time::from_hms(23, 59, 59).unwrap_or(time::Time::MIDNIGHT));
    json!({
        "PageNumber": 1,
        "PageSize": 200,
        "Status": [0, 3],
        "SlicePeriodStartTime": local_datetime_text(start),
        "SlicePeriodEndTime": local_datetime_text(end),
        "PackageCodes": FREE_PACKAGE_CODES,
    })
}

/// 三路响应 → 合并后的资源明细；三路都不可用时返回 None（交给旧接口回退）。
fn normalized_resources(
    summary_response: &Value,
    paid_response: &Value,
    free_response: &Value,
    now: i64,
) -> Option<Vec<Value>> {
    let ok = |response: &Value| {
        let code = response.get("code").and_then(Value::as_i64).unwrap_or(-1);
        (code == 0 || code == 200) && !resource_items_of(response).is_empty()
    };
    let summary_ok = ok(summary_response);
    let paid_ok = ok(paid_response);
    let free_ok = ok(free_response);
    if !(summary_ok || paid_ok || free_ok) {
        return None;
    }
    let mut detail: Vec<Value> = Vec::new();
    if paid_ok {
        detail.extend(
            resource_items_of(paid_response)
                .into_iter()
                .map(|package| credit_resource_summary(&package, now)),
        );
    }
    if free_ok {
        detail.extend(
            resource_items_of(free_response)
                .into_iter()
                .map(|package| credit_resource_summary(&package, now)),
        );
    }
    if summary_ok {
        // summary 的包可能缺时间字段，已被明细覆盖的同商品码不再重复计入。
        let known: Vec<String> = detail
            .iter()
            .filter_map(|resource| resource.get("packageCode").and_then(Value::as_str))
            .map(str::to_string)
            .collect();
        detail.extend(
            resource_items_of(summary_response)
                .into_iter()
                .map(|package| credit_resource_summary(&package, now))
                .filter(|resource| {
                    resource
                        .get("packageCode")
                        .and_then(Value::as_str)
                        .map(|code| !known.iter().any(|known| known == code))
                        .unwrap_or(true)
                }),
        );
    }
    Some(detail)
}

/// 资源明细 → 积分摘要：总量、分类汇总、临期/过期与最早到期时间。
fn credit_summary(resources: &[Value], now: i64) -> Value {
    let number =
        |resource: &Value, key: &str| resource.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    let total_remaining: f64 = resources
        .iter()
        .map(|resource| number(resource, "remaining"))
        .sum();
    let total_capacity: f64 = resources
        .iter()
        .map(|resource| number(resource, "total"))
        .sum();
    let soonest_expire_at = resources
        .iter()
        .filter(|resource| number(resource, "remaining") > 0.0)
        .filter_map(|resource| resource.get("expireAt").and_then(Value::as_i64))
        .min();
    let flag = |key: &str| {
        resources.iter().any(|resource| {
            resource.get(key).and_then(Value::as_bool) == Some(true)
                && number(resource, "remaining") > 0.0
        })
    };
    let sum_where = |key: &str| {
        resources
            .iter()
            .filter(|resource| resource.get(key).and_then(Value::as_bool) == Some(true))
            .map(|resource| number(resource, "remaining"))
            .sum::<f64>()
    };
    let mut groups: Vec<Value> = ["paid", "free", "other"]
        .iter()
        .filter_map(|category| {
            let items: Vec<&Value> = resources
                .iter()
                .filter(|resource| {
                    resource
                        .get("category")
                        .and_then(Value::as_str)
                        .unwrap_or("other")
                        == *category
                })
                .collect();
            if items.is_empty() {
                return None;
            }
            let remaining: f64 = items
                .iter()
                .map(|resource| number(resource, "remaining"))
                .sum();
            let total: f64 = items.iter().map(|resource| number(resource, "total")).sum();
            Some(json!({
                "category": category,
                "label": match *category {
                    "paid" => "购买",
                    "free" => "赠送",
                    _ => "其他",
                },
                "remaining": remaining,
                "total": total,
                "expireAt": items
                    .iter()
                    .filter(|resource| number(resource, "remaining") > 0.0)
                    .filter_map(|resource| resource.get("expireAt").and_then(Value::as_i64))
                    .min(),
            }))
        })
        .collect();
    groups.sort_by(|left, right| {
        number(right, "remaining")
            .partial_cmp(&number(left, "remaining"))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    json!({
        "ok": true,
        "updatedAt": now,
        "totalRemaining": total_remaining,
        "totalCapacity": total_capacity,
        "expiringSoonRemaining": sum_where("expiringSoon"),
        "expiredRemaining": sum_where("expired"),
        "soonestExpireAt": soonest_expire_at,
        "expiringSoon": flag("expiringSoon"),
        "expired": flag("expired"),
        "resourceCount": resources.len(),
        "groups": groups,
        "resources": resources,
    })
}

/// 解析 "yyyy-MM-dd HH:mm:ss"（按本地时区）为毫秒时间戳；
/// 同时容忍 RFC3339 形式（"T" 分隔、带时区后缀）。
fn parse_datetime_text_ms(text: &str) -> Option<i64> {
    let normalized = text.trim().replace('T', " ");
    let normalized: String = normalized
        .chars()
        .take_while(|char| !matches!(char, 'Z' | '+' | '.'))
        .collect();
    let mut parts = normalized.trim().split(['-', ' ', ':']);
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u8>().ok()?;
    let day = parts.next()?.parse::<u8>().ok()?;
    let hour = parts.next()?.parse::<u8>().ok()?;
    let minute = parts.next()?.parse::<u8>().ok()?;
    let second = parts.next()?.parse::<u8>().ok()?;
    let date =
        time::Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()?;
    let clock = time::Time::from_hms(hour, minute, second).ok()?;
    let offset = time::OffsetDateTime::now_local().ok()?.offset();
    Some(
        date.with_time(clock)
            .assume_utc()
            .to_offset(offset)
            .unix_timestamp()
            * 1000,
    )
}

fn local_datetime_text(value: time::OffsetDateTime) -> String {
    let Ok(format) =
        time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]")
    else {
        return String::new();
    };
    value.format(&format).unwrap_or_default()
}

fn local_day_string() -> String {
    let Ok(now) = time::OffsetDateTime::now_local() else {
        return String::new();
    };
    let Ok(format) = time::format_description::parse("[year]-[month]-[day]") else {
        return String::new();
    };
    now.format(&format).unwrap_or_default()
}

fn read_json_file(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn atomic_write(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("无法创建 {parent:?}: {error}"))?;
    }
    crate::atomic_file::write_atomic(path, content.as_bytes())
        .map_err(|error| format!("写入 {path:?} 失败: {error}"))
}

fn write_cli_json_string_cache(path: &Path, value: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let content = serde_json::to_string(&json!(value))
        .map_err(|error| format!("无法序列化 CodeBuddy CLI 缓存: {error}"))?;
    atomic_write(path, &content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_expiry_with_cycle_override_and_far_future_placeholder() {
        let now = 1_800_000_000_000_i64;
        let day_ms = 24 * 3600 * 1000;
        // DeductionEndTime 是 2049 占位值，相距 CycleEndTime 超过 365 天 → 用周期结束时间。
        let package = json!({
            "DeductionEndTime": now + 900 * day_ms,
            "CycleEndTime": now + 10 * day_ms,
        });
        assert_eq!(resolve_expire_ms(&package, now), Some(now + 10 * day_ms));
        // 只有一个超远期值时视为长期有效。
        assert_eq!(resolve_expire_ms(&json!({"CycleEndTime": now + 900 * day_ms}), now), None);
    }

    #[test]
    fn maps_package_code_to_official_name_and_category() {
        let now = 1_800_000_000_000_i64;
        let resource = credit_resource_summary(
            &json!({
                "PackageCode": "TCACA_code_007_nzdH5h4Nl0",
                "PackageName": "活动赠送包",
                "CycleCapacitySizePrecise": "100",
                "CycleCapacityRemainPrecise": "40",
                "CycleEndTime": now + 3 * 24 * 3600 * 1000,
            }),
            now,
        );
        assert_eq!(resource["packageName"], "平台奖励积分");
        assert_eq!(resource["category"], "free");
        assert_eq!(resource["remaining"], 40.0);
        assert_eq!(resource["used"], 60.0);
        assert_eq!(resource["expiringSoon"], true);
        // 未登记码回落服务端 PackageName。
        let fallback = credit_resource_summary(&json!({"PackageName": "活动赠送包"}), now);
        assert_eq!(fallback["packageName"], "活动赠送包");
        assert_eq!(fallback["category"], "other");
    }

    #[test]
    fn summarizes_totals_groups_and_soonest_expiry() {
        let now = 1_800_000_000_000_i64;
        let day_ms = 24 * 3600 * 1000;
        let resources = vec![
            credit_resource_summary(
                &json!({
                    "PackageCode": "TCACA_code_009_0XmEQc2xOf",
                    "CycleCapacitySizePrecise": "100",
                    "CycleCapacityRemainPrecise": "30",
                    "CycleEndTime": now + 20 * day_ms,
                }),
                now,
            ),
            credit_resource_summary(
                &json!({
                    "PackageCode": "TCACA_code_007_nzdH5h4Nl0",
                    "CycleCapacitySizePrecise": "50",
                    "CycleCapacityRemainPrecise": "20",
                    "CycleEndTime": now + 2 * day_ms,
                }),
                now,
            ),
        ];
        let summary = credit_summary(&resources, now);
        assert_eq!(summary["totalRemaining"], 50.0);
        assert_eq!(summary["totalCapacity"], 150.0);
        assert_eq!(summary["soonestExpireAt"], now + 2 * day_ms);
        assert_eq!(summary["expiringSoon"], true);
        assert_eq!(summary["resourceCount"], 2);
        let groups = summary["groups"].as_array().cloned().unwrap_or_default();
        assert_eq!(groups.len(), 2);
        // 按剩余积分降序：免费包 20 < 付费包 30，付费在前。
        assert_eq!(groups[0]["category"], "paid");
        assert_eq!(groups[0]["remaining"], 30.0);
        assert_eq!(groups[1]["category"], "free");
    }

    #[test]
    fn parses_and_infers_variants_correctly() {
        assert_eq!(CodeBuddyVariant::parse(None), CodeBuddyVariant::Cn);
        assert_eq!(CodeBuddyVariant::parse(Some("")), CodeBuddyVariant::Cn);
        assert_eq!(CodeBuddyVariant::parse(Some("cn")), CodeBuddyVariant::Cn);
        assert_eq!(CodeBuddyVariant::parse(Some("CN")), CodeBuddyVariant::Cn);
        assert_eq!(CodeBuddyVariant::parse(Some("unknown")), CodeBuddyVariant::Cn);
        assert_eq!(CodeBuddyVariant::parse(Some("ai")), CodeBuddyVariant::Ai);
        assert_eq!(CodeBuddyVariant::parse(Some("AI")), CodeBuddyVariant::Ai);
        assert_eq!(CodeBuddyVariant::parse(Some("intl")), CodeBuddyVariant::Ai);
        assert_eq!(CodeBuddyVariant::parse(Some("global")), CodeBuddyVariant::Ai);

        assert_eq!(CodeBuddyVariant::from_domain(""), CodeBuddyVariant::Cn);
        assert_eq!(CodeBuddyVariant::from_domain("www.codebuddy.cn"), CodeBuddyVariant::Cn);
        assert_eq!(CodeBuddyVariant::from_domain("www.workbuddy.cn"), CodeBuddyVariant::Cn);
        assert_eq!(CodeBuddyVariant::from_domain("www.workbuddy.ai"), CodeBuddyVariant::Ai);
        assert_eq!(CodeBuddyVariant::from_domain("www.codebuddy.ai"), CodeBuddyVariant::Ai);
        assert_eq!(CodeBuddyVariant::from_domain("workbuddy.ai"), CodeBuddyVariant::Ai);
        assert_eq!(
            CodeBuddyVariant::from_domain("test.workbuddy.ai.fake.com"),
            CodeBuddyVariant::Cn
        );
    }

    #[test]
    fn variant_endpoints_and_cli_configs_match_specs() {
        assert_eq!(CodeBuddyVariant::Cn.api_endpoint(), "https://www.codebuddy.cn");
        assert_eq!(CodeBuddyVariant::Ai.api_endpoint(), "https://www.workbuddy.ai");

        assert_eq!(CodeBuddyVariant::Cn.oauth_platform(), "workbuddy");
        assert_eq!(CodeBuddyVariant::Ai.oauth_platform(), "workbuddy-ai");

        assert_eq!(CodeBuddyVariant::Cn.cli_endpoint(), "https://copilot.tencent.com");
        assert_eq!(CodeBuddyVariant::Ai.cli_endpoint(), "https://www.codebuddy.ai");

        assert_eq!(CodeBuddyVariant::Cn.cli_openai_base_url(), None);
        assert_eq!(CodeBuddyVariant::Ai.cli_openai_base_url(), Some("https://www.codebuddy.ai/v2"));

        assert_eq!(CodeBuddyVariant::Cn.cli_internet_environment(), "internal");
        assert_eq!(CodeBuddyVariant::Ai.cli_internet_environment(), "public");

        assert!(CodeBuddyVariant::Cn.supports_checkin());
        assert!(!CodeBuddyVariant::Ai.supports_checkin());

        assert!(CodeBuddyVariant::Cn.supports_travel());
        assert!(!CodeBuddyVariant::Ai.supports_travel());

        let billing = CodeBuddyVariant::Ai.billing_paths("/v2/billing/meter/get-user-resource");
        assert_eq!(
            billing,
            vec![
                "/billing/meter/get-user-resource".to_string(),
                "/v2/billing/meter/get-user-resource".to_string()
            ]
        );
        let cn_billing = CodeBuddyVariant::Cn.billing_paths("/v2/billing/meter/get-user-resource");
        assert_eq!(cn_billing, vec!["/v2/billing/meter/get-user-resource".to_string()]);
    }

    #[test]
    fn account_meta_reports_variant_attributes() {
        let cn_acc = CodeBuddyAccount {
            id: "cn-1".to_string(),
            domain: "www.codebuddy.cn".to_string(),
            ..Default::default()
        };
        let cn_meta = cn_acc.meta();
        assert_eq!(cn_meta["variant"], "cn");
        assert_eq!(cn_meta["variantLabel"], "国内版");
        assert_eq!(cn_meta["supportsCheckin"], true);
        assert_eq!(cn_meta["supportsTravel"], true);

        let ai_acc = CodeBuddyAccount {
            id: "ai-1".to_string(),
            variant: Some("ai".to_string()),
            ..Default::default()
        };
        let ai_meta = ai_acc.meta();
        assert_eq!(ai_meta["variant"], "ai");
        assert_eq!(ai_meta["variantLabel"], "国际版");
        assert_eq!(ai_meta["supportsCheckin"], false);
        assert_eq!(ai_meta["supportsTravel"], false);
    }

    #[test]
    fn imports_wb_switch_accounts_json_correctly() {
        let sample = json!([
            {
                "id": "11111111-1111-4111-8111-111111111111",
                "uid": "00000000-0000-4000-8000-000000000000",
                "domain": "www.workbuddy.ai",
                "variant": "ai",
                "nickname": "test.user@example.invalid",
                "email": null,
                "access_token": "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIwMDAwMDAwMC0wMDAwLTQwMDAtODAwMC0wMDAwMDAwMDAwMDAiLCJlbWFpbCI6InRlc3QudXNlckBleGFtcGxlLmludmFsaWQifQ==.signature",
                "refresh_token": "refresh-123",
                "expiresAt": 1823165039883i64
            }
        ]);
        let parsed = parse_codebuddy_import_account(&sample[0]).expect("should parse");
        assert_eq!(parsed.id, "11111111-1111-4111-8111-111111111111");
        assert_eq!(parsed.uid, "00000000-0000-4000-8000-000000000000");
        assert_eq!(parsed.variant, Some("ai".to_string()));
        assert_eq!(parsed.email, "test.user@example.invalid");
        assert_eq!(parsed.nickname, "test.user@example.invalid");
        assert_eq!(
            parsed.access_token,
            "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIwMDAwMDAwMC0wMDAwLTQwMDAtODAwMC0wMDAwMDAwMDAwMDAiLCJlbWFpbCI6InRlc3QudXNlckBleGFtcGxlLmludmFsaWQifQ==.signature"
        );
        assert_eq!(parsed.refresh_token, "refresh-123");
        assert_eq!(parsed.expires_at, Some(1823165039883i64));
    }

    #[test]
    fn imports_nested_auth_and_account_json_correctly() {
        let sample = json!({
            "account": {
                "uid": "wb-uid-test",
                "nickname": "测试用户",
                "enterpriseId": "ent-1"
            },
            "auth": {
                "accessToken": "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJ3Yi11aWQtdGVzdCIsImVtYWlsIjoidGVzdEBleGFtcGxlLmNvbSJ9.sig",
                "refreshToken": "nested-refresh-token-xyz",
                "domain": "www.codebuddy.cn",
                "realm": "cn",
                "expiresIn": 3600
            }
        });
        let parsed =
            parse_codebuddy_import_account(&sample).expect("should parse nested auth format");
        assert_eq!(parsed.uid, "wb-uid-test");
        assert_eq!(parsed.nickname, "测试用户");
        assert_eq!(parsed.email, "test@example.com");
        assert_eq!(parsed.enterprise_id, "ent-1");
        assert_eq!(parsed.domain, "www.codebuddy.cn");
        assert_eq!(parsed.variant, Some("cn".to_string()));
        assert_eq!(parsed.refresh_token, "nested-refresh-token-xyz");
        assert!(parsed.expires_at.is_some());
    }

    #[test]
    fn imports_auth_raw_with_refresh_expires_in() {
        let sample = json!({
            "access_token": "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJyYXctdWlkIiwiZW1haWwiOiJyYXdAZXhhbXBsZS5jb20ifQ.sig",
            "nickname": "RawUser",
            "auth_raw": {
                "accessToken": "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiJyYXctdWlkIiwiZW1haWwiOiJyYXdAZXhhbXBsZS5jb20ifQ.sig",
                "refreshToken": "auth-raw-refresh-tok",
                "expiresIn": 5184000,
                "refreshExpiresIn": 7776000
            }
        });
        let parsed = parse_codebuddy_import_account(&sample).expect("should parse auth_raw format");
        assert_eq!(parsed.uid, "raw-uid");
        assert_eq!(parsed.refresh_token, "auth-raw-refresh-tok");
        assert!(parsed.refresh_expires_at.is_some());
    }

    #[test]
    fn upsert_account_preserves_existing_refresh_token_when_incoming_empty() {
        let dir = std::env::temp_dir().join(format!(
            "webclx-cb-upsert-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let manager = CodeBuddyManager::load(dir.clone(), dir.join("home")).unwrap();

        // 1. 添加带有完整 refresh_token 的账号
        let initial = CodeBuddyAccount {
            id: "acc-1".to_string(),
            uid: "uid-001".to_string(),
            email: "user@example.com".to_string(),
            nickname: "初始昵称".to_string(),
            access_token: "jwt-token-1".to_string(),
            refresh_token: "precious-refresh-token".to_string(),
            refresh_expires_at: Some(1999999999000),
            ..Default::default()
        };
        manager.upsert_account(initial).unwrap();

        // 2. 传入相同 uid 但 refresh_token 为空的更新记录
        let update = CodeBuddyAccount {
            id: "ignored-id".to_string(),
            uid: "uid-001".to_string(),
            email: "user@example.com".to_string(),
            nickname: "".to_string(),
            access_token: "jwt-token-2".to_string(),
            refresh_token: "".to_string(),
            refresh_expires_at: None,
            ..Default::default()
        };
        manager.upsert_account(update).unwrap();

        // 3. 验证保留了已有 refresh_token 和 nickname
        let inner = manager.read_inner();
        let acc = inner.accounts.iter().find(|a| a.uid == "uid-001").unwrap();
        assert_eq!(acc.access_token, "jwt-token-2");
        assert_eq!(acc.refresh_token, "precious-refresh-token");
        assert_eq!(acc.refresh_expires_at, Some(1999999999000));
        assert_eq!(acc.nickname, "初始昵称");

        // 4. 验证 export_accounts 保留凭据
        let mut selected = HashSet::new();
        selected.insert(acc.id.as_str());
        let exported = manager.export_accounts(&selected);
        assert_eq!(exported.len(), 1);
        assert_eq!(exported[0].access_token, "jwt-token-2");
        assert_eq!(exported[0].refresh_token, "precious-refresh-token");

        drop(inner);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bypass_permissions_fills_prefix_allow_rules_and_off_removes_them() {
        let dir = std::env::temp_dir().join(format!(
            "webclx-codebuddy-options-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let manager = CodeBuddyManager::load(dir.clone(), dir.join("home")).unwrap();
        let settings_file = dir.join("home/.codebuddy/settings.json");
        fs::create_dir_all(settings_file.parent().unwrap()).unwrap();
        fs::write(&settings_file, r#"{"model":"glm-5.3-flash"}"#).unwrap();

        manager.set_cli_options(true, false).unwrap();
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&settings_file).unwrap()).unwrap();
        assert_eq!(settings["permissions"]["defaultMode"], json!("bypassPermissions"));
        let allow = settings["permissions"]["allow"].as_array().unwrap();
        assert!(allow.contains(&json!("Bash(scp:*)")));
        assert!(allow.contains(&json!("Bash(ssh:*)")));
        assert!(allow.contains(&json!("Bash(sudo:*)")));
        assert!(allow.contains(&json!("Bash(systemctl:*)")));
        assert!(allow.len() >= BYPASS_PERMISSION_ALLOW_RULES.len());
        assert_eq!(manager.cli_options()["bypassPermissions"], json!(true));

        manager.set_cli_options(false, false).unwrap();
        let settings: Value =
            serde_json::from_str(&fs::read_to_string(&settings_file).unwrap()).unwrap();
        assert!(settings["permissions"].get("defaultMode").is_none());
        // 关闭 bypass 时不吞掉用户已有 allow；本测试的空 settings 不写入 allow。
        assert!(
            settings
                .pointer("/permissions/allow")
                .and_then(Value::as_array)
                .map(|allow| allow.is_empty())
                .unwrap_or(true)
        );

        fs::remove_dir_all(dir).unwrap();
    }
}
