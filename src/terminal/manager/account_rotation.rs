//! CodeBuddy / AGY quota recovery. Owns a bounded recovery operation, never an Agent lease.
use super::*;
use serde_json::{Value, json};
use std::sync::{LazyLock, Mutex};

const EXHAUSTED: &[&str] = &[
    "余额不足",
    "积分不足",
    "insufficient balance",
    "insufficient credits",
    "credit balance is too low",
    "您的使用量已超出频率限制",
    "Individual quota reached",
    "You have exhausted your quota on this model",
    "upgrade your subscription to increase your limits",
    "to increase your limits. Resets in",
];
const RETRY_MS: u64 = 300_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AgyModelFamily {
    Claude,
    Gemini,
}

fn agy_model_family(pane: &str) -> Option<AgyModelFamily> {
    agy_model_family_with_home(pane, None)
}

fn agy_model_family_with_home(pane: &str, home: Option<&Path>) -> Option<AgyModelFamily> {
    // Only the real, most recent native footer identifies the active model.
    let footer = pane
        .lines()
        .rev()
        .filter(|l| !l.trim().is_empty())
        .take(10)
        .find(|l| l.contains("? for shortcuts") || l.contains("esc to cancel"))
        .map(|l| l.to_ascii_lowercase());
    if let Some(footer) = footer {
        if footer.contains("claude") || footer.contains("gpt") {
            return Some(AgyModelFamily::Claude);
        } else if footer.contains("gemini") {
            return Some(AgyModelFamily::Gemini);
        }
    }
    // When AGY exits or during turn execution, footer is absent. Check
    // banner/switch lines or user's settings.json under home.
    for line in pane.lines().rev().filter(|l| !l.trim().is_empty()).take(30) {
        let l = line.to_ascii_lowercase();
        if l.contains("antigravity cli") || l.starts_with("switched model to") {
            if l.contains("claude") || l.contains("gpt") {
                return Some(AgyModelFamily::Claude);
            } else if l.contains("gemini") {
                return Some(AgyModelFamily::Gemini);
            }
        }
    }
    if let Some(home) = home {
        let settings_path = home.join(".gemini/antigravity-cli/settings.json");
        if let Ok(content) = fs::read_to_string(settings_path) {
            if let Ok(v) = serde_json::from_str::<Value>(&content) {
                if let Some(model) = v.get("model").and_then(Value::as_str) {
                    let m = model.to_ascii_lowercase();
                    if m.contains("claude") || m.contains("gpt") {
                        return Some(AgyModelFamily::Claude);
                    } else if m.contains("gemini") {
                        return Some(AgyModelFamily::Gemini);
                    }
                }
            }
        }
        return Some(AgyModelFamily::Gemini);
    }
    None
}

fn usable_agy_quota(summary: &Value, family: AgyModelFamily) -> bool {
    if summary["ok"] != true {
        return false;
    }
    if let Some(groups) = summary["quotaSummary"]["groups"].as_array() {
        let matching: Vec<_> = groups
            .iter()
            .filter(|group| {
                let mut label = format!(
                    "{} {}",
                    group["displayName"].as_str().unwrap_or(""),
                    group["description"].as_str().unwrap_or("")
                )
                .to_ascii_lowercase();
                if let Some(buckets) = group["buckets"].as_array() {
                    for bucket in buckets {
                        label.push_str(bucket["bucketId"].as_str().unwrap_or(""));
                    }
                }
                match family {
                    AgyModelFamily::Gemini => label.contains("gemini"),
                    AgyModelFamily::Claude => {
                        !label.contains("gemini")
                            && ["claude", "3p", "gpt"].iter().any(|s| label.contains(s))
                    }
                }
            })
            .collect();
        if matching.len() == 1 {
            let Some(buckets) = matching[0]["buckets"].as_array() else {
                return false;
            };
            let required_windows: &[&str] = if family == AgyModelFamily::Gemini
                && !buckets.iter().any(|b| {
                    b["window"]
                        .as_str()
                        .or_else(|| b["bucketId"].as_str())
                        .unwrap_or("")
                        .to_ascii_lowercase()
                        .contains("weekly")
                }) {
                &["5h"]
            } else {
                &["weekly", "5h"]
            };
            return required_windows.iter().all(|window| {
                let windows: Vec<_> = buckets
                    .iter()
                    .filter(|b| {
                        b["window"]
                            .as_str()
                            .or_else(|| b["bucketId"].as_str())
                            .unwrap_or("")
                            .to_ascii_lowercase()
                            .contains(window)
                    })
                    .collect();
                windows.len() == 1
                    && windows[0]["remainingFraction"]
                        .as_f64()
                        .is_some_and(|v| v.is_finite() && v > 0.0 && v <= 1.0)
                    && windows[0]["disabled"] != true
                    && windows[0]["enabled"] != false
            });
        }
    }
    // When quotaSummary has no matching group (e.g. Gemini models which Google reports under `models`
    // instead of `retrieveUserQuotaSummary.groups`), inspect `models` map.
    if let Some(models) = summary["models"].as_object() {
        let matching_models: Vec<_> = models
            .iter()
            .filter(|(name, _)| {
                let lower = name.to_ascii_lowercase();
                match family {
                    AgyModelFamily::Gemini => lower.contains("gemini"),
                    AgyModelFamily::Claude => lower.contains("claude") || lower.contains("gpt"),
                }
            })
            .collect();
        if !matching_models.is_empty() {
            let any_usable = matching_models.iter().any(|(_, model)| {
                if model.get("disabled") == Some(&Value::Bool(true)) {
                    return false;
                }
                if let Some(quota_info) = model.get("quotaInfo") {
                    quota_info["remainingFraction"]
                        .as_f64()
                        .is_some_and(|v| v.is_finite() && v > 0.0)
                } else {
                    true
                }
            });
            return any_usable;
        }
    }
    false
}

#[derive(Clone, Copy)]
enum AccountAgent<'a> {
    CodeBuddy(&'a crate::codebuddy::CodeBuddyManager),
    Agy(&'a crate::agy::Manager),
}
impl AccountAgent<'_> {
    fn block_international(self) -> bool {
        matches!(self, Self::CodeBuddy(m) if m.block_international_auto_switch())
    }

    fn check_candidate_policy(self, account: &Value) -> Result<()> {
        anyhow::ensure!(
            !self.block_international() || account["variant"] != "ai",
            "已禁止自动切换到国际版账号，取消恢复"
        );
        Ok(())
    }

    fn name(self) -> &'static str {
        match self {
            Self::CodeBuddy(_) => "CodeBuddy",
            Self::Agy(_) => "AGY",
        }
    }
    fn composer(self) -> ComposerAgent {
        match self {
            Self::CodeBuddy(_) => ComposerAgent::CodeBuddy,
            Self::Agy(_) => ComposerAgent::Agy,
        }
    }
    fn manages_home(self, home: &std::path::Path) -> bool {
        match self {
            Self::CodeBuddy(m) => m.manages_home(home),
            Self::Agy(m) => m.manages_home(home),
        }
    }
    fn accounts(self) -> Vec<Value> {
        match self {
            Self::CodeBuddy(m) => m.list_accounts(),
            Self::Agy(m) => m
                .list()
                .into_iter()
                .map(|a| {
                    json!({
                        "id":a["id"], "active":a["current"], "needsRelogin":a["valid"] != true,
                        "lastSwitchAt":a["last_switch_at"], "variant":"agy"
                    })
                })
                .collect(),
        }
    }
    async fn usable(self, id: &str, family: Option<AgyModelFamily>) -> bool {
        match self {
            Self::CodeBuddy(m) => {
                tokio::time::timeout(Duration::from_secs(20), m.credits_account(id))
                    .await
                    .is_ok_and(|s| usable_credits(&s, current_timestamp_millis()))
            }
            Self::Agy(m) => {
                let family = family.unwrap_or(AgyModelFamily::Gemini);
                let live_res = tokio::time::timeout(Duration::from_secs(10), m.quota(id)).await;
                match live_res {
                    Ok(summary) if summary["ok"] == true => usable_agy_quota(&summary, family),
                    _ => {
                        let cached = m.cached_quota(id);
                        if cached["ok"] == true {
                            usable_agy_quota(&cached, family)
                        } else {
                            // If live query failed and no cached quota, assume candidate is usable rather than blocking rotation
                            true
                        }
                    }
                }
            }
        }
    }
    async fn apply(self, id: &str) -> Result<()> {
        match self {
            Self::CodeBuddy(m) => {
                let (m, id) = (m.clone(), id.to_owned());
                blocking(move || {
                    m.switch_account(&id)
                        .map(|_| ())
                        .map_err(anyhow::Error::msg)
                })
                .await
            }
            Self::Agy(m) => tokio::time::timeout(Duration::from_secs(20), m.apply(id))
                .await
                .context("AGY 账号切换超时")?
                .map_err(anyhow::Error::msg),
        }
    }
}

#[derive(Default)]
struct RotationState {
    running: HashSet<String>,
    attempted: HashMap<String, u64>,
    phases: HashMap<String, String>,
    exhausted_accounts: HashMap<String, u64>,
}
static ROTATIONS: LazyLock<Mutex<RotationState>> = LazyLock::new(Mutex::default);
pub(super) fn exhaustion_keyword(keyword: &str) -> bool {
    EXHAUSTED.iter().any(|s| keyword.eq_ignore_ascii_case(s))
}

pub(super) fn owns_error(session: &TerminalSessionInfo) -> bool {
    let has_exhaustion_keyword = session
        .activity_error_keyword
        .as_deref()
        .is_some_and(exhaustion_keyword);
    if !has_exhaustion_keyword {
        return false;
    }
    if matches!(session.activity_agent.as_deref(), Some("CodeBuddy" | "AGY")) {
        return true;
    }
    // When AGY or CodeBuddy exits to the shell upon quota exhaustion,
    // activity_agent becomes None. Specific exhaustion keywords prove
    // that this session belongs to AGY / CodeBuddy rotation.
    if let Some(keyword) = session.activity_error_keyword.as_deref() {
        if keyword.contains("Individual quota reached")
            || keyword.contains("exhausted your quota on this model")
            || keyword.contains("increase your limits")
            || keyword.contains("您的使用量已超出频率限制")
        {
            return true;
        }
    }
    false
}

pub(super) fn recovering(session_id: &str) -> bool {
    crate::lock_or_recover!(ROTATIONS.lock())
        .running
        .contains(session_id)
}

struct RecoveryGuard(String);
impl Drop for RecoveryGuard {
    fn drop(&mut self) {
        let mut state = crate::lock_or_recover!(ROTATIONS.lock());
        state.running.remove(&self.0);
        state.phases.remove(&self.0);
    }
}

fn claim(session_id: &str, now: u64) -> Option<RecoveryGuard> {
    let mut state = crate::lock_or_recover!(ROTATIONS.lock());
    state
        .attempted
        .retain(|_, at| now.saturating_sub(*at) < RETRY_MS);
    state.exhausted_accounts.retain(|_, until| *until > now);
    if state.running.contains(session_id) || state.attempted.contains_key(session_id) {
        return None;
    }
    state.running.insert(session_id.into());
    Some(RecoveryGuard(session_id.into()))
}

fn phase(id: &str, stage: &str) {
    crate::lock_or_recover!(ROTATIONS.lock())
        .phases
        .insert(id.into(), stage.into());
    info!(session_id = id, phase = stage, "Agent account recovery progress");
}

fn claim_status(id: &str) -> String {
    let state = crate::lock_or_recover!(ROTATIONS.lock());
    if state.running.contains(id) {
        format!(
            "正在恢复：{}",
            state
                .phases
                .get(id)
                .map(String::as_str)
                .unwrap_or("核查原生错误")
        )
    } else {
        let seconds = state
            .attempted
            .get(id)
            .map(|at| {
                (at.saturating_add(RETRY_MS)
                    .saturating_sub(current_timestamp_millis())
                    + 999)
                    / 1000
            })
            .unwrap_or(0);
        format!("上次恢复失败，{} 秒后重试", seconds)
    }
}

fn finish_success(id: &str) {
    // The five-minute backoff belongs to failed recovery operations. A new
    // native quota error after a successful switch must try the next account
    // immediately, while the exhausted-account quarantine remains in force.
    crate::lock_or_recover!(ROTATIONS.lock())
        .attempted
        .remove(id);
}

fn has_bogus_shell_continue(snapshot: &[u8]) -> bool {
    let text = String::from_utf8_lossy(snapshot);
    text.contains("继续: 未找到命令") || text.contains("继续: command not found")
}

fn exhaustion(snapshot: &[u8], respect_interrupt: bool) -> Option<TerminalErrorKeywordMatch> {
    error_detection::terminal_error_keyword_match_from_snapshot(
        snapshot,
        30,
        &EXHAUSTED.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
        &[],
        respect_interrupt,
    )
    .filter(|m| !m.input_queued && (!m.continue_sent || has_bogus_shell_continue(snapshot)))
    .filter(|m| {
        let text = String::from_utf8_lossy(snapshot);
        text.lines()
            .rev()
            .filter(|l| !l.trim().is_empty())
            .take(30)
            .any(|line| {
                let line = line.trim();
                let lower = line.to_ascii_lowercase();
                let needle = m.keyword.to_ascii_lowercase();
                lower.contains(&needle)
                    && (line.starts_with("⚠ ")
                        || line.starts_with("└ ")
                        || line.starts_with("API Error:")
                        || line.starts_with("Error:")
                        || line.starts_with("✕ ")
                        || line.starts_with("✖ ")
                        || lower.contains("resets in")
                        || lower.contains("to increase your limits"))
            })
    })
}

fn usable_credits(summary: &Value, now: u64) -> bool {
    // Points from expired packages must not make an account appear usable.
    summary["ok"] == true
        && summary["resources"].as_array().is_some_and(|resources| {
            resources.iter().any(|r| {
                r["remaining"]
                    .as_f64()
                    .is_some_and(|v| v.is_finite() && v > 0.0)
                    && r["expired"] != true
                    && r["expireAt"].as_u64().is_none_or(|at| at > now)
            })
        })
}

fn candidates(accounts: Vec<Value>, now: u64, block_international: bool) -> Vec<Value> {
    if !accounts.iter().any(|a| a["active"] == true) {
        return vec![];
    }
    // Regional policy applies only to automatic recovery; native manual
    // account switching remains available independently of this setting.
    let state = crate::lock_or_recover!(ROTATIONS.lock());
    let mut accounts: Vec<_> = accounts
        .iter()
        .filter(|a| {
            a["active"] != true
                && (!block_international || a["variant"] != "ai")
                && a["needsRelogin"] != true
                && !state
                    .exhausted_accounts
                    .get(a["id"].as_str().unwrap_or(""))
                    .is_some_and(|at| *at > now)
        })
        .cloned()
        .collect();
    accounts.sort_by_key(|a| {
        (
            a["lastSwitchAt"].as_i64().unwrap_or(0),
            a["id"].as_str().unwrap_or("").to_owned(),
        )
    });
    accounts
}

fn shell_ready(text: &str) -> bool {
    let last = text
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim();
    (last.starts_with('[') && (last.ends_with("#") || last.ends_with("$")))
        || (last.contains('@')
            && last.contains(':')
            && (last.ends_with('#') || last.ends_with('$')))
        || matches!(last, "#" | "$")
}

pub(super) fn schedule(
    manager: &TerminalManager,
    codebuddy: &crate::codebuddy::CodeBuddyManager,
    agy: &crate::agy::Manager,
    sessions: &[TerminalSessionInfo],
) {
    for session in sessions {
        if !owns_error(session) || session.hidden || session.activity_error_input_queued {
            continue;
        }
        let Some(guard) = claim(&session.id, current_timestamp_millis()) else {
            continue;
        };
        let manager = manager.clone();
        let codebuddy = codebuddy.clone();
        let agy = agy.clone();
        let id = session.id.clone();
        tokio::spawn(async move {
            let _guard = guard;
            if let Err(error) = recover(&manager, &codebuddy, &agy, &id, true).await {
                warn!("Agent account recovery {id}: {error:#}");
                toast(&manager, &id, &format!("自动换号恢复停止：{error}"), "warn");
            }
        });
    }
}

fn toast(manager: &TerminalManager, id: &str, message: &str, tone: &str) {
    let _ = manager.event_sender.send(TerminalManagerEvent::Toast {
        session_id: id.into(),
        message: message.into(),
        tone: tone.into(),
    });
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T> + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(f)
        .await
        .context("恢复任务异常")?
}

async fn snapshot(id: &str) -> Result<Vec<u8>> {
    let id = id.to_owned();
    blocking(move || capture_tmux_text_pane_snapshot(&id)).await
}

async fn activity(id: &str) -> Result<TerminalAgentActivity> {
    let id = id.to_owned();
    blocking(move || Ok(TerminalAgentDetector::new().detect(&id))).await
}

async fn input(manager: &TerminalManager, id: &str, text: &str) -> Result<()> {
    let (manager, id, text) = (manager.clone(), id.to_owned(), text.to_owned());
    blocking(move || manager.send_session_input_silent(&id, text)).await
}

pub(in crate::terminal) async fn manual_recover(
    manager: &TerminalManager,
    codebuddy: &crate::codebuddy::CodeBuddyManager,
    agy: &crate::agy::Manager,
    id: &str,
) -> Result<Value> {
    let _guard =
        claim(id, current_timestamp_millis()).ok_or_else(|| anyhow::anyhow!(claim_status(id)))?;
    recover(manager, codebuddy, agy, id, true).await
}

fn stopped_resume_info(
    before: &[u8],
    after: &[u8],
    program: &str,
    known: Option<&terminal_core::ResumeCommandInfo>,
) -> Result<terminal_core::ResumeCommandInfo> {
    const MARKER: &str = "To resume this session:";
    let before = String::from_utf8_lossy(before);
    let after = String::from_utf8_lossy(after);
    let native = after
        .rsplit_once(MARKER)
        .and_then(|(_, tail)| terminal_core::resume_command_info_from_text(tail))
        .filter(|info| info.program == program);
    if program == "codebuddy" {
        let info = native.context("CodeBuddy 已退出，但未提取到原生退出恢复命令，未切换账号")?;
        anyhow::ensure!(
            terminal_core::codebuddy_session_id_from_path(std::path::Path::new(&format!(
                "/.codebuddy/projects/native/{}.jsonl",
                info.resume_id
            )))
            .is_some(),
            "原生退出 ID 无效，未切换账号"
        );
        if let Some(known) = known {
            anyhow::ensure!(
                known.resume_id == info.resume_id,
                "原生退出 ID 与运行中会话不符，未切换账号"
            );
        } else {
            anyhow::ensure!(
                after.matches(MARKER).count() > before.matches(MARKER).count()
                    || before
                        .rsplit_once(MARKER)
                        .and_then(|(_, tail)| terminal_core::resume_command_info_from_text(tail))
                        .is_none_or(
                            |old| old.program != info.program || old.resume_id != info.resume_id
                        ),
                "没有新的原生退出提示，未使用旧 ID 换号"
            );
        }
        Ok(info)
    } else if program == "agy" {
        const AGY_MARKER: &str = "Resume with";
        let native_agy = after
            .rsplit_once(AGY_MARKER)
            .and_then(|(_, tail)| terminal_core::resume_command_info_from_text(tail))
            .filter(|info| info.program == "agy")
            .or_else(|| {
                terminal_core::resume_command_info_from_text(&after)
                    .filter(|info| info.program == "agy")
            });
        let info = native_agy
            .or_else(|| known.cloned())
            .context("AGY 已退出，但未提取到原生退出恢复命令，未切换账号")?;
        anyhow::ensure!(
            info.resume_id.len() == 36
                && info.resume_id.bytes().enumerate().all(|(i, b)| {
                    if [8, 13, 18, 23].contains(&i) {
                        b == b'-'
                    } else {
                        b.is_ascii_hexdigit()
                    }
                }),
            "原生 AGY 退出 ID 格式无效，未切换账号"
        );
        if let Some(known) = known {
            anyhow::ensure!(
                known.resume_id == info.resume_id,
                "原生退出 ID 与运行中会话不符，未切换账号"
            );
        }
        Ok(info)
    } else {
        known.cloned().context("未确认原会话 ID，未切换账号")
    }
}

async fn recover(
    manager: &TerminalManager,
    codebuddy: &crate::codebuddy::CodeBuddyManager,
    agy: &crate::agy::Manager,
    id: &str,
    allow_exited: bool,
) -> Result<Value> {
    let baseline = snapshot(id).await?;
    let Some(error) = exhaustion(&baseline, !allow_exited) else {
        return Ok(json!({"recovered":false,"reason":"没有当前余额或频率额度耗尽错误"}));
    };
    phase(id, "检测到原生额度耗尽");
    // Mark the attempt before any network work; a failed query must not spin.
    crate::lock_or_recover!(ROTATIONS.lock())
        .attempted
        .insert(id.into(), current_timestamp_millis());
    let manager_copy = manager.clone();
    let target = id.to_owned();
    let detected = blocking(move || manager_copy.current_resume_session_complete(&target)).await?;
    let live = activity(id).await?;
    let program = match live.agents.as_slice() {
        [name] if name == "CodeBuddy" => "codebuddy",
        [name] if name == "AGY" => "agy",
        [] if allow_exited => detected
            .as_ref()
            .map(|d| d.info.program.as_str())
            .or_else(|| {
                terminal_core::resume_command_info_from_text(&String::from_utf8_lossy(&baseline))
                    .map(|info| match info.program.as_str() {
                        "agy" => "agy",
                        "codebuddy" => "codebuddy",
                        _ => "",
                    })
            })
            .unwrap_or(""),
        _ => "",
    };
    let agent = match program {
        "codebuddy" => AccountAgent::CodeBuddy(codebuddy),
        "agy" => AccountAgent::Agy(agy),
        _ => anyhow::bail!("未识别到 CodeBuddy / AGY 原生进程或退出恢复命令"),
    };
    let program = program.to_owned();
    let detected = detected.filter(|d| d.info.program == program);
    let user = runtime_paths::resolve_user_profile(&manager.session_user_name(id)?)?;
    anyhow::ensure!(agent.manages_home(&user.home), "目标终端用户与账号管理用户不同，未切换");
    let model_family = if matches!(agent, AccountAgent::Agy(_)) {
        Some(
            agy_model_family_with_home(&String::from_utf8_lossy(&baseline), Some(&user.home))
                .context("未识别原生 AGY 模型，未切换")?,
        )
    } else {
        None
    };
    let exited = live.agents.is_empty() && shell_ready(&String::from_utf8_lossy(&baseline));
    anyhow::ensure!(
        live.agents == [agent.name()] || (allow_exited && exited),
        "目标终端 Agent 已退出或被其他进程接管"
    );
    if !exited {
        let styled = {
            let id = id.to_owned();
            blocking(move || capture_tmux_visible_styled_pane_snapshot(&id)).await?
        };
        anyhow::ensure!(
            composer_draft::scan_composer_draft(
                agent.composer(),
                &String::from_utf8_lossy(&styled)
            ) == ComposerScan::Empty,
            "输入框非空或无法识别，保留用户输入，未中断"
        );
    }
    let accounts = agent.accounts();
    let original = accounts
        .iter()
        .find(|a| a["active"] == true)
        .context("无法确定当前账号，未切换")?["id"]
        .as_str()
        .context("当前账号 ID 无效")?
        .to_owned();
    // Keep a known exhausted account out of later rotations. A fresh reset-time
    // probe can improve this conservative one-hour quarantine in future.
    crate::lock_or_recover!(ROTATIONS.lock())
        .exhausted_accounts
        .insert(original.clone(), current_timestamp_millis() + 3_600_000);
    let mut selected = None;
    for account in candidates(accounts, current_timestamp_millis(), agent.block_international()) {
        let account_id = account["id"].as_str().context("候选账号 ID 无效")?;
        phase(id, &format!("查询候选账号额度 {account_id}"));
        if agent.usable(account_id, model_family).await {
            selected = Some(account);
            break;
        } else {
            crate::lock_or_recover!(ROTATIONS.lock())
                .exhausted_accounts
                .insert(account_id.to_string(), current_timestamp_millis() + 3_600_000);
            warn!(session_id = id, account_id, "候选账号额度不足，已记录隔离并尝试下一个账号");
        }
    }
    let selected_account = selected.context(if agent.block_international() {
        "已禁止自动切换到国际版账号，且所有国内账号均已循环尝试或额度不足，保留原终端"
    } else {
        "所有候选账号均已额度不足或已隔离，已循环一遍无可用账号，保留原终端"
    })?;
    let selected = selected_account["id"]
        .as_str()
        .context("候选账号 ID 无效")?;
    // Network queries may take time. Require the same error and pane before
    // sending any control bytes or modifying shared native credentials.
    let fresh = snapshot(id).await?;
    anyhow::ensure!(
        exhaustion(&fresh, !allow_exited).is_some_and(|m| m.signature == error.signature),
        "终端错误已变化，取消换号"
    );
    anyhow::ensure!(
        agent
            .accounts()
            .iter()
            .any(|a| a["id"] == original && a["active"] == true),
        "当前账号已被其他操作切换，取消恢复"
    );
    if let Some(family) = model_family {
        anyhow::ensure!(
            agy_model_family_with_home(&String::from_utf8_lossy(&fresh), Some(&user.home))
                == Some(family),
            "原生模型已变化，取消换号"
        );
    }
    let (copy, target) = (manager.clone(), id.to_owned());
    let current = blocking(move || copy.current_resume_session_complete(&target)).await?;
    if let Some(known) = &detected {
        anyhow::ensure!(
            current.is_some_and(|s| s.info.program == known.info.program
                && s.info.resume_id == known.info.resume_id),
            "原会话已变化，取消恢复"
        );
    }
    if !exited {
        let live = activity(id).await?;
        anyhow::ensure!(live.agents == [agent.name()], "Agent 状态已变化，取消恢复");
        let target = id.to_owned();
        let styled = blocking(move || capture_tmux_visible_styled_pane_snapshot(&target)).await?;
        anyhow::ensure!(
            composer_draft::scan_composer_draft(
                agent.composer(),
                &String::from_utf8_lossy(&styled)
            ) == ComposerScan::Empty,
            "查询期间用户已输入内容，取消恢复"
        );
        agent.check_candidate_policy(&selected_account)?;
        phase(id, "发送两次 Ctrl+C");
        input(manager, id, "\u{3}").await?;
        tokio::time::sleep(Duration::from_millis(250)).await;
        input(manager, id, "\u{3}").await?;
    }
    let started = Instant::now();
    loop {
        if activity(id).await?.agents.is_empty()
            && shell_ready(&String::from_utf8_lossy(&snapshot(id).await?))
        {
            break;
        }
        anyhow::ensure!(
            started.elapsed() < Duration::from_secs(12),
            "两次 Ctrl+C 后未确认退出，未切换账号"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    let stopped = snapshot(id).await?;
    let resume =
        stopped_resume_info(&fresh, &stopped, &program, detected.as_ref().map(|d| &d.info))?;
    agent.check_candidate_policy(&selected_account)?;
    phase(id, &format!("原生退出 ID 已核对 {}，切换账号 {selected}", resume.resume_id));
    agent.apply(selected).await?;
    phase(id, &format!("已切换账号 {original} -> {selected}，恢复原生会话"));
    // Use exactly the native command from the same extractor as the shortcut.
    input(manager, id, &resume.command).await?;
    tokio::time::sleep(Duration::from_millis(200)).await;
    let target = id.to_owned();
    let manager_copy = manager.clone();
    blocking(move || manager_copy.send_session_enter_key(&target)).await?;
    phase(id, "等待恢复后的原生空输入框");
    let started = Instant::now();
    let mut stable_since = Instant::now();
    let mut previous = Vec::new();
    loop {
        let bytes = snapshot(id).await?;
        if bytes != previous {
            previous = bytes.clone();
            stable_since = Instant::now();
        }
        let text = String::from_utf8_lossy(&bytes);
        let styled = {
            let id = id.to_owned();
            blocking(move || capture_tmux_visible_styled_pane_snapshot(&id)).await?
        };
        let lines: Vec<_> = text.lines().collect();
        let last_prompt = lines.iter().rposition(|l| l.trim_start().starts_with('>'));
        let last_launch = lines.iter().rposition(|l| l.contains(&resume.command));
        let fresh_frame =
            last_prompt.is_some_and(|prompt| last_launch.is_none_or(|launch| prompt > launch));
        if fresh_frame
            && composer_draft::scan_composer_draft(
                agent.composer(),
                &String::from_utf8_lossy(&styled),
            ) == ComposerScan::Empty
            && !error_detection::terminal_tail_has_working_status(
                &text
                    .trim_end()
                    .lines()
                    .rev()
                    .take(10)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
            && stable_since.elapsed() >= Duration::from_millis(900)
            && activity(id).await?.agents == [agent.name()]
        {
            break;
        }
        anyhow::ensure!(
            started.elapsed() < Duration::from_secs(30),
            "未确认恢复后的空闲输入框，未发送继续"
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    let (copy, target) = (manager.clone(), id.to_owned());
    let resumed = blocking(move || copy.current_resume_session_complete(&target)).await?;
    anyhow::ensure!(
        resumed.is_some_and(
            |s| s.info.program == resume.program && s.info.resume_id == resume.resume_id
        ),
        "恢复后的会话 ID 不符，未发送继续"
    );
    let (copy, target) = (manager.clone(), id.to_owned());
    phase(id, "原生会话及输入框已核对，发送继续");
    blocking(move || copy.send_session_continue(&target)).await?;
    finish_success(id);
    info!(session_id = id, agent = agent.name(), previous_account = %original, account_id = %selected, resume_id = %resume.resume_id, "Agent account recovery completed");
    toast(
        manager,
        id,
        &format!(
            "已切换到最近未使用且余额/配额查询通过的 {} 账号，恢复原会话并发送“继续”。",
            agent.name()
        ),
        "ok",
    );
    Ok(json!({"recovered":true,"account_id":selected,"resume_id":resume.resume_id}))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn account_rotation_matches_real_wrapped_quota_error_but_not_working_or_generic_429() {
        let real = "● REPL\n  └ 429 您的使用量已超出频率限制，将在 2026-10-26\n    08:23:26 UTC+8\n    重置，您也可以切换其他模型继续使用。\n────────────────────\n> \n────────────────────";
        assert!(exhaustion(real.as_bytes(), true).is_some());
        assert!(
            exhaustion(format!("{real}\n• Working (12s • esc to interrupt)").as_bytes(), true)
                .is_none()
        );
        assert!(exhaustion(b"429 Too Many Requests", true).is_none());
        assert!(exhaustion(format!("{real}\n> 继续").as_bytes(), true).is_none());
    }
    #[test]
    fn account_rotation_agy_requires_both_live_model_windows() {
        let bucket =
            |window: &str, remaining| json!({"window":window,"remainingFraction":remaining});
        let summary = |name: &str, weekly, five| json!({"ok":true,"quotaSummary":{"groups":[{"displayName":name,"buckets":[bucket("weekly",weekly),bucket("5h",five)]}]}});
        assert!(usable_agy_quota(
            &summary("Claude and GPT models", 0.5, 1.0),
            AgyModelFamily::Claude
        ));
        assert!(usable_agy_quota(&summary("Gemini models", 0.5, 1.0), AgyModelFamily::Gemini));
        for value in [
            summary("Claude models", 0.0, 1.0),
            summary("Claude models", 0.8, 0.0),
            summary("Gemini models", 1.0, 1.0),
            json!({"ok":true}),
            json!({"ok":false}),
        ] {
            assert!(!usable_agy_quota(&value, AgyModelFamily::Claude));
        }
        let mut value = summary("Claude models", 0.5, 1.0);
        value["quotaSummary"]["groups"][0]["buckets"][1]["disabled"] = json!(true);
        assert!(!usable_agy_quota(&value, AgyModelFamily::Claude));
        value["quotaSummary"]["groups"][0]["buckets"]
            .as_array_mut()
            .unwrap()
            .pop();
        assert!(!usable_agy_quota(&value, AgyModelFamily::Claude));
    }

    #[test]
    fn account_rotation_agy_gemini_models_quota_support() {
        let summary_with_models = json!({
            "ok": true,
            "models": {
                "gemini-3.8-flash-tiered": {
                    "quotaInfo": {
                        "remainingFraction": 1.0,
                        "resetTime": "2026-11-01T12:51:15Z"
                    }
                },
                "claude-opus-4-6-thinking": {
                    "quotaInfo": {
                        "remainingFraction": 1.0
                    }
                }
            },
            "quotaSummary": {
                "groups": [
                    {
                        "displayName": "Claude and GPT models",
                        "buckets": [
                            {"bucketId": "3p-weekly", "window": "weekly", "remainingFraction": 1.0},
                            {"bucketId": "3p-5h", "window": "5h", "remainingFraction": 1.0}
                        ]
                    }
                ]
            }
        });
        assert!(usable_agy_quota(&summary_with_models, AgyModelFamily::Gemini));
        assert!(usable_agy_quota(&summary_with_models, AgyModelFamily::Claude));

        let exhausted_gemini = json!({
            "ok": true,
            "models": {
                "gemini-3.8-flash-tiered": {
                    "quotaInfo": {
                        "remainingFraction": 0.0
                    }
                }
            },
            "quotaSummary": {
                "groups": [
                    {
                        "displayName": "Claude and GPT models",
                        "buckets": [
                            {"bucketId": "3p-weekly", "window": "weekly", "remainingFraction": 1.0},
                            {"bucketId": "3p-5h", "window": "5h", "remainingFraction": 1.0}
                        ]
                    }
                ]
            }
        });
        assert!(!usable_agy_quota(&exhausted_gemini, AgyModelFamily::Gemini));
        assert!(usable_agy_quota(&exhausted_gemini, AgyModelFamily::Claude));
    }
    #[test]
    fn account_rotation_agy_real_error_footer_and_prose_guards() {
        let real = "⚠ Individual quota reached. Please upgrade your\nsubscription to increase your limits. Resets in\n4h54m8s.\nError ID: a0159324\n────────────────────\n>\n────────────────────\n? for shortcuts       Claude Opus 4.6 (Thinking)";
        assert!(exhaustion(real.as_bytes(), true).is_some());
        assert_eq!(agy_model_family(real), Some(AgyModelFamily::Claude));
        assert_eq!(
            agy_model_family(&real.replace("Claude Opus", "Gemini")),
            Some(AgyModelFamily::Gemini)
        );
        assert_eq!(agy_model_family("assistant: Claude Opus\n>"), None);
        assert!(
            exhaustion(format!("{real}\n⣻ Running command... esc to cancel").as_bytes(), true)
                .is_none()
        );
        assert!(
            exhaustion(b"assistant: Handle Individual quota reached by switching account", true)
                .is_none()
        );
        assert!(
            exhaustion(
                "用户说余额不足时如何处理\n────────────────────\n>\n────────────────────"
                    .as_bytes(),
                true
            )
            .is_none()
        );
    }
    #[test]
    fn account_rotation_extracts_fresh_native_exit_id_before_switch() {
        let id = "01a15e5f-93d6-70c9-acba-b299d4566e53";
        let exit =
            format!("To resume this session:\ncodebuddy --resume={id}\n[root@host project]#");
        let info = stopped_resume_info(b"original quota error", exit.as_bytes(), "codebuddy", None)
            .unwrap();
        assert_eq!(info.resume_id, id);
        assert!(stopped_resume_info(b"error", exit.as_bytes(), "codebuddy", Some(&info)).is_ok());
        assert!(stopped_resume_info(exit.as_bytes(), exit.as_bytes(), "codebuddy", None).is_err());
        assert!(
            stopped_resume_info(
                format!("{exit}\nold quota error").as_bytes(),
                format!("{exit}\nnew shell output").as_bytes(),
                "codebuddy",
                None
            )
            .is_err()
        );
        assert!(stopped_resume_info(b"error", b"[root@host project]#", "codebuddy", None).is_err());
        let mut wrong = info.clone();
        wrong.resume_id = "01a15e5f-93d6-70c9-acba-b299d4566e54".into();
        assert!(stopped_resume_info(b"error", exit.as_bytes(), "codebuddy", Some(&wrong)).is_err());
    }
    #[test]
    fn account_rotation_success_allows_next_quota_error_but_failures_back_off() {
        let id = "account-rotation-success-backoff-regression";
        let now = current_timestamp_millis();
        let guard = claim(id, now).unwrap();
        crate::lock_or_recover!(ROTATIONS.lock())
            .attempted
            .insert(id.into(), now);
        assert!(claim(id, now + 1).is_none());
        finish_success(id);
        assert!(claim(id, now + 1).is_none(), "in-flight guard still owns the operation");
        drop(guard);
        let next =
            claim(id, now + 1).expect("a new quota error must try the next account immediately");
        crate::lock_or_recover!(ROTATIONS.lock())
            .attempted
            .insert(id.into(), now);
        drop(next);
        assert!(claim(id, now + 2).is_none(), "failed recovery retains five-minute backoff");
        crate::lock_or_recover!(ROTATIONS.lock())
            .attempted
            .remove(id);
    }
    #[test]
    fn account_rotation_credits_require_live_nonexpired_positive_resource() {
        let now = current_timestamp_millis();
        assert!(usable_credits(
            &json!({"ok":true,"resources":[{"remaining":4,"expireAt":now+1000}]}),
            now
        ));
        for value in [
            json!({"ok":true,"totalRemaining":100}),
            json!({"ok":false,"resources":[{"remaining":9}]}),
            json!({"ok":true,"resources":[{"remaining":0}]}),
            json!({"ok":true,"resources":[{"remaining":9,"expired":true}]}),
            json!({"ok":true,"resources":[{"remaining":9,"expireAt":now-1}]} ),
        ] {
            assert!(!usable_credits(&value, now));
        }
    }
    #[test]
    fn account_rotation_orders_unused_then_oldest_across_codebuddy_regions() {
        let a = |id: &str, active, at, variant: &str| json!({"id":id,"active":active,"lastSwitchAt":at,"variant":variant});
        let picked = candidates(
            vec![
                a("current", true, 7, "cn"),
                a("recent", false, 9, "cn"),
                a("old", false, 2, "cn"),
                a("unused", false, 0, "cn"),
                a("foreign", false, 0, "ai"),
                a("foreign_old", false, 1, "ai"),
                json!({"id":"bad","variant":"cn","needsRelogin":true}),
            ],
            current_timestamp_millis(),
            false,
        );
        assert_eq!(
            picked
                .iter()
                .map(|v| v["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["foreign", "unused", "foreign_old", "old", "recent"]
        );

        let picked = candidates(
            vec![
                a("current_intl", true, 7, "ai"),
                a("domestic_unused", false, 0, "cn"),
                a("intl_recent", false, 9, "ai"),
            ],
            current_timestamp_millis(),
            false,
        );
        assert_eq!(
            picked
                .iter()
                .map(|v| v["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["domestic_unused", "intl_recent"]
        );
    }

    #[test]
    fn account_rotation_cross_region_still_excludes_quarantine_and_unknown_current() {
        let now = current_timestamp_millis();
        let blocked = "cross-region-exhausted-intl-regression";
        crate::lock_or_recover!(ROTATIONS.lock())
            .exhausted_accounts
            .insert(blocked.into(), now + 1000);
        let accounts = vec![
            json!({"id":"cross-region-current","active":true,"variant":"cn"}),
            json!({"id":blocked,"variant":"ai"}),
            json!({"id":"cross-region-relogin","variant":"ai","needsRelogin":true}),
            json!({"id":"cross-region-available","variant":"ai"}),
        ];
        let picked = candidates(accounts.clone(), now, false);
        assert_eq!(
            picked
                .iter()
                .map(|v| v["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["cross-region-available"]
        );
        let picked = candidates(accounts.clone(), now + 1001, false);
        assert!(picked.iter().any(|v| v["id"] == blocked));
        assert!(candidates(accounts.into_iter().skip(1).collect(), now, false).is_empty());
        crate::lock_or_recover!(ROTATIONS.lock())
            .exhausted_accounts
            .remove(blocked);
    }

    #[test]
    fn account_rotation_blocks_international_candidates_without_blocking_domestic_or_agy() {
        let now = current_timestamp_millis();
        let accounts = vec![
            json!({"id":"policy-current-intl","active":true,"variant":"ai"}),
            json!({"id":"policy-domestic-recent","variant":"cn","lastSwitchAt":9}),
            json!({"id":"policy-intl-unused","variant":"ai"}),
            json!({"id":"policy-domestic-old","variant":"cn","lastSwitchAt":2}),
            json!({"id":"policy-domestic-relogin","variant":"cn","needsRelogin":true}),
        ];
        let picked = candidates(accounts.clone(), now, true);
        assert_eq!(
            picked
                .iter()
                .map(|v| v["id"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["policy-domestic-old", "policy-domestic-recent"]
        );
        assert_eq!(candidates(accounts.clone(), now, false)[0]["id"], "policy-intl-unused");
        assert!(candidates(vec![accounts[0].clone(), accounts[2].clone()], now, true).is_empty());
        let agy = vec![
            json!({"id":"policy-agy-current","active":true,"variant":"agy"}),
            json!({"id":"policy-agy-unused","variant":"agy"}),
        ];
        assert_eq!(candidates(agy, now, false)[0]["id"], "policy-agy-unused");
    }

    #[test]
    fn account_rotation_rechecks_international_policy_after_candidate_query() {
        let dir = std::env::temp_dir().join(format!(
            "webclx-rotation-policy-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let manager =
            crate::codebuddy::CodeBuddyManager::load(dir.clone(), dir.join("home")).unwrap();
        let agent = AccountAgent::CodeBuddy(&manager);
        let international = json!({"id":"policy-intl","variant":"ai"});
        let domestic = json!({"id":"policy-cn","variant":"cn"});
        assert!(agent.block_international());
        assert!(agent.check_candidate_policy(&international).is_err());
        assert!(agent.check_candidate_policy(&domestic).is_ok());
        manager.set_auto_config(None, None, Some(false)).unwrap();
        assert!(!agent.block_international());
        assert!(agent.check_candidate_policy(&international).is_ok());
        // Enabling the switch during a quota query invalidates a selected
        // international candidate before interrupting or applying credentials.
        manager.set_auto_config(None, None, Some(true)).unwrap();
        assert!(agent.check_candidate_policy(&international).is_err());
        assert!(agent.check_candidate_policy(&domestic).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn account_rotation_agy_native_exit_extracts_resume_and_recovers_after_bogus_shell_continue() {
        let conversation_id = "7197a95e-156b-4e2e-a02f-bc117e7c66b6";
        let real_terminal = format!(
            "⚠ Individual quota reached. Please upgrade your\nsubscription to increase your limits. Resets in\n2h37m22s.\nError ID: 87f7abc6-a079-4596-abf3-ce445caae598-2231\n\n How's the CLI experience so far? Help us improv\n [1] Good  [2] Fine  [3] Bad  [0] Skip\n\n\nResume with -c (or command below):\nagy --conversation={conversation_id}\n[root@openeuler webClx]# 继续\nbash: 继续: 未找到命令\n[root@openeuler webClx]# "
        );
        assert!(exhaustion(real_terminal.as_bytes(), false).is_some());
        let info = stopped_resume_info(b"old error", real_terminal.as_bytes(), "agy", None)
            .expect("should extract agy resume command from native exit output");
        assert_eq!(info.program, "agy");
        assert_eq!(info.resume_id, conversation_id);
        assert_eq!(info.command, format!("agy --conversation={conversation_id}"));
    }

    #[test]
    fn account_rotation_owns_error_recognizes_agy_quota_when_process_exited() {
        let mut session = TerminalSessionInfo {
            id: "s13934".into(),
            name: "agy_term".into(),
            previous_names: Vec::new(),
            title: None,
            user_name: "root".into(),
            codex_api_preset_name: String::new(),
            codex_api_base_url: String::new(),
            origin: TerminalSessionOrigin::Normal,
            owner_key: String::new(),
            path: "/home/codes/tools/webClx".into(),
            icon_project_path: String::new(),
            display_path: "/home/codes/tools/webClx".into(),
            alive: true,
            connected: true,
            busy: false,
            activity_state: "error".into(),
            activity_label: "Individual quota reached".into(),
            activity_agent: None,
            model_family: String::new(),
            activity_error_keyword: Some("Individual quota reached".into()),
            activity_error_signature: Some("sig123".into()),
            activity_error_continue_sent: false,
            activity_error_input_queued: false,
            activity_error_auto_continue_at: None,
            last_output_at: 1000,
            idle: true,
            hidden: false,
            auto_compact_disabled: false,
            created_at: 1000,
            last_opened_at: 1000,
            group_token: String::new(),
            group_color: String::new(),
            group_name: String::new(),
            group_role: String::new(),
            group_order: 0,
            group_fixed_order: false,
        };
        assert!(owns_error(&session));
        session.activity_error_keyword = Some("You have exhausted your quota on this model".into());
        assert!(owns_error(&session));
        session.activity_error_keyword = Some("您的使用量已超出频率限制".into());
        assert!(owns_error(&session));
        session.activity_error_keyword = Some("503 Service Unavailable".into());
        assert!(!owns_error(&session));
    }

    #[test]
    fn account_rotation_agy_model_family_resolves_from_home_settings_when_footer_absent() {
        let dir = std::env::temp_dir().join(format!(
            "webclx-agy-model-family-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let agy_dir = dir.join(".gemini/antigravity-cli");
        std::fs::create_dir_all(&agy_dir).unwrap();
        std::fs::write(
            agy_dir.join("settings.json"),
            json!({"model": "Gemini 3.8 Flash (High)"}).to_string(),
        )
        .unwrap();
        let pane_no_footer = "How's the CLI experience so far?\nResume with -c:\nagy --conversation=7197a95e-156b-4e2e-a02f-bc117e7c66b6\n[root@host]# ";
        assert_eq!(
            agy_model_family_with_home(pane_no_footer, Some(&dir)),
            Some(AgyModelFamily::Gemini)
        );
        std::fs::write(
            agy_dir.join("settings.json"),
            json!({"model": "Claude 3.7 Sonnet (Thinking)"}).to_string(),
        )
        .unwrap();
        assert_eq!(
            agy_model_family_with_home(pane_no_footer, Some(&dir)),
            Some(AgyModelFamily::Claude)
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
