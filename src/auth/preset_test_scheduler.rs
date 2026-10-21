//! 预设 API 定时测试调度器。
//!
//! 独立于终端定时输入系统，支持按每天、每周、固定间隔等周期对指定的
//! API/Claude 预设执行连接+对话测试，结果持久化供前端展示。

use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use anyhow::{Context, Result};
use auth_core::{
    AuthPresetManager, api_preset_enables_local_upstream_proxy_on_apply,
    effective_claude_use_local_proxy,
};
use serde::{Deserialize, Serialize};
use terminal_core::current_timestamp_millis;
use time::{OffsetDateTime, Weekday};
use tokio::sync::Notify;
use tracing::{info, warn};

use super::{
    PRESET_CHAT_PROBE_DELAY, PresetTestResult,
    preset_tests::{PresetTestEnvironment, annotate_preset_test_result},
};
use crate::{AppError, settings::SettingsManager};

/// 调度文件名，与 state_file 同目录。
const PRESET_TEST_SCHEDULE_FILE_NAME: &str = ".webclx-preset-test-schedules.json";

/// 预设类型：API 预设（Codex）或 Claude 预设。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresetKind {
    Api,
    Claude,
}

/// 调度周期类型。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScheduleType {
    /// 每天固定时间，hour: 0-23, minute: 0-59
    Daily,
    /// 每周固定星期和时间，weekdays: 选中的星期 0=Sunday..6=Saturday
    Weekly,
    /// 固定间隔（分钟），从创建时开始计算
    Interval,
}

/// 调度参数。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleParams {
    /// Daily: "HH:MM"；Weekly: "HH:MM"；Interval: 间隔分钟数（字符串形式便于 JSON）
    #[serde(default)]
    pub time: String,
    /// Weekly: selected weekdays 0-6 (0=Sunday). Migrated from legacy `weekday`.
    #[serde(default)]
    pub weekdays: Vec<u32>,
    /// Legacy single weekday field, used only for deserialization migration.
    #[serde(default, skip_serializing)]
    pub weekday: Option<u32>,
    /// Interval: 间隔分钟数
    #[serde(default)]
    pub interval_minutes: u64,
}

/// 持久化的定时测试任务。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetTestSchedule {
    pub id: String,
    pub name: String,
    pub preset_kind: PresetKind,
    pub preset_id: String,
    pub preset_name: String,
    pub schedule_type: ScheduleType,
    pub schedule_params: ScheduleParams,
    pub enabled: bool,
    pub created_at_millis: u64,
    /// 上次执行的预期触发时间
    #[serde(default)]
    pub last_fired_at_millis: u64,
    /// 下次预计触发时间
    #[serde(default)]
    pub next_fire_at_millis: u64,
}

/// 最近一次测试结果记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresetTestScheduleResult {
    pub schedule_id: String,
    pub fired_at_millis: u64,
    pub ok: bool,
    pub result: PresetTestResult,
}

/// 注册表文件结构。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct StoredPresetTestScheduleRegistry {
    #[serde(default)]
    schedules: Vec<PresetTestSchedule>,
    #[serde(default)]
    results: Vec<PresetTestScheduleResult>,
}

/// 给前端的任务信息。
#[derive(Debug, Clone, Serialize)]
pub struct PresetTestScheduleInfo {
    pub id: String,
    pub name: String,
    pub preset_kind: String,
    pub preset_id: String,
    pub preset_name: String,
    pub schedule_type: String,
    pub schedule_desc: String,
    pub time: String,
    pub weekdays: Vec<u32>,
    pub interval_minutes: u64,
    pub enabled: bool,
    pub created_at_millis: u64,
    pub last_fired_at_millis: u64,
    pub next_fire_at_millis: u64,
    pub last_result: Option<PresetTestScheduleResult>,
}

/// 创建/更新请求体。
#[derive(Debug, Clone, Deserialize)]
pub struct PresetTestScheduleRequest {
    pub name: String,
    pub preset_kind: String,
    pub preset_id: String,
    pub schedule_type: String,
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub weekdays: Vec<u32>,
    #[serde(default)]
    pub interval_minutes: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

/// 更新请求体（所有字段可选）。
#[derive(Debug, Deserialize)]
pub struct PresetTestScheduleUpdateRequest {
    pub name: Option<String>,
    pub preset_kind: Option<String>,
    pub preset_id: Option<String>,
    pub schedule_type: Option<String>,
    pub time: Option<String>,
    pub weekdays: Option<Vec<u32>>,
    pub interval_minutes: Option<u64>,
    pub enabled: Option<bool>,
}

fn default_true() -> bool {
    true
}

/// 调度器，持有所有状态。
#[derive(Clone)]
pub struct PresetTestScheduler {
    schedules: Arc<Mutex<HashMap<String, PresetTestSchedule>>>,
    results: Arc<Mutex<HashMap<String, PresetTestScheduleResult>>>,
    notify: Arc<Notify>,
    file: Arc<PathBuf>,
    next_id: Arc<std::sync::atomic::AtomicU64>,
}

impl PresetTestScheduler {
    pub fn new(state_file: &Path) -> Self {
        let file = state_file.with_file_name(PRESET_TEST_SCHEDULE_FILE_NAME);
        let registry = load_registry(&file).unwrap_or_default();
        let schedules: HashMap<String, PresetTestSchedule> = registry
            .schedules
            .into_iter()
            .filter(|s| !s.id.is_empty())
            .map(|s| (s.id.clone(), s))
            .collect();
        let results: HashMap<String, PresetTestScheduleResult> = registry
            .results
            .into_iter()
            .filter(|r| !r.schedule_id.is_empty())
            .map(|r| (r.schedule_id.clone(), r))
            .collect();
        let max_id = schedules
            .keys()
            .filter_map(|id| id.strip_prefix("pts-").and_then(|n| n.parse::<u64>().ok()))
            .max()
            .unwrap_or(0);
        Self {
            schedules: Arc::new(Mutex::new(schedules)),
            results: Arc::new(Mutex::new(results)),
            notify: Arc::new(Notify::new()),
            file: Arc::new(file),
            next_id: Arc::new(std::sync::atomic::AtomicU64::new(max_id + 1)),
        }
    }

    pub fn spawn_runner(
        &self,
        auth_manager: AuthPresetManager,
        proxy_manager: crate::proxy::ProxyManager,
        workspace_settings: SettingsManager,
    ) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let scheduler = self.clone();
        tokio::spawn(async move {
            scheduler
                .run_loop(auth_manager, proxy_manager, workspace_settings)
                .await;
        });
    }

    async fn run_loop(
        self,
        auth_manager: AuthPresetManager,
        proxy_manager: crate::proxy::ProxyManager,
        workspace_settings: SettingsManager,
    ) {
        info!("preset test scheduler loop started");
        loop {
            let now = current_timestamp_millis();
            let due_schedules: Vec<PresetTestSchedule> = {
                let schedules = crate::lock_or_recover!(self.schedules.lock());
                schedules
                    .values()
                    .filter(|s| {
                        s.enabled && s.next_fire_at_millis > 0 && s.next_fire_at_millis <= now
                    })
                    .cloned()
                    .collect()
            };

            for schedule in due_schedules {
                let result = self
                    .fire_test(&schedule, &auth_manager, &proxy_manager, &workspace_settings)
                    .await;
                let now_after = current_timestamp_millis();
                let next = compute_next_fire_at(&schedule, now_after);
                {
                    let mut schedules = crate::lock_or_recover!(self.schedules.lock());
                    if let Some(s) = schedules.get_mut(&schedule.id) {
                        s.last_fired_at_millis = now_after;
                        s.next_fire_at_millis = next;
                    }
                    self.persist_locked(&schedules);
                }
                if let Ok(result) = result {
                    let record = PresetTestScheduleResult {
                        schedule_id: schedule.id.clone(),
                        fired_at_millis: now_after,
                        ok: result.ok,
                        result,
                    };
                    {
                        let mut results = crate::lock_or_recover!(self.results.lock());
                        results.insert(schedule.id.clone(), record);
                    }
                    // Persist outside the results lock to avoid re-entrant locking.
                    let schedules_snapshot = {
                        let schedules = crate::lock_or_recover!(self.schedules.lock());
                        schedules.clone()
                    };
                    let results_snapshot = {
                        let results = crate::lock_or_recover!(self.results.lock());
                        results.clone()
                    };
                    self.persist_results_locked(&schedules_snapshot, &results_snapshot);
                }
            }

            let sleep_ms = {
                let schedules = crate::lock_or_recover!(self.schedules.lock());
                schedules
                    .values()
                    .filter(|s| s.enabled && s.next_fire_at_millis > 0)
                    .map(|s| s.next_fire_at_millis)
                    .min()
                    .map(|next| {
                        next.saturating_sub(current_timestamp_millis())
                            .clamp(1000, 60_000)
                    })
                    .unwrap_or(60_000)
            };

            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(sleep_ms)) => {}
                _ = self.notify.notified() => {}
            }
        }
    }

    async fn fire_test(
        &self,
        schedule: &PresetTestSchedule,
        auth_manager: &AuthPresetManager,
        proxy_manager: &crate::proxy::ProxyManager,
        workspace_settings: &SettingsManager,
    ) -> Result<PresetTestResult, AppError> {
        let environment = PresetTestEnvironment::capture(proxy_manager, workspace_settings).await?;
        let default_config_entries = workspace_settings.codex_default_config_entries();
        let default_config_pairs = default_config_entries
            .iter()
            .map(|e| (e.key.as_str(), e.value.as_str()))
            .collect::<Vec<_>>();

        match schedule.preset_kind {
            PresetKind::Api => {
                let presets = auth_manager.api_presets_snapshot();
                let preset = presets
                    .iter()
                    .find(|p| p.id == schedule.preset_id)
                    .cloned()
                    .ok_or_else(|| {
                        AppError::not_found(format!("找不到 API 预设: {}", schedule.preset_name))
                    })?;
                let context = environment.context_for(&preset.terminal_env)?;
                let result = super::test_stored_api_preset_with_delay(
                    &context.client,
                    &preset,
                    &default_config_pairs,
                    PRESET_CHAT_PROBE_DELAY,
                )
                .await?;
                let access_mode = if api_preset_enables_local_upstream_proxy_on_apply(&preset) {
                    "本地中继"
                } else {
                    "直连上游"
                };
                Ok(annotate_preset_test_result(&context, result, access_mode))
            }
            PresetKind::Claude => {
                let presets = auth_manager.claude_presets_snapshot();
                let preset = presets
                    .iter()
                    .find(|p| p.id == schedule.preset_id)
                    .cloned()
                    .ok_or_else(|| {
                        AppError::not_found(format!("找不到 Claude 预设: {}", schedule.preset_name))
                    })?;
                let effective_preset =
                    super::claude_preset_with_global_defaults(workspace_settings, &preset)?;
                let context = environment.context_for(&[])?;
                let use_local_proxy = effective_claude_use_local_proxy(&effective_preset);
                let result = super::test_stored_claude_preset_with_delay(
                    &context.client,
                    &effective_preset,
                    use_local_proxy,
                    PRESET_CHAT_PROBE_DELAY,
                )
                .await;
                Ok(annotate_preset_test_result(
                    &context,
                    result,
                    if use_local_proxy {
                        "本地中继"
                    } else {
                        "直连上游"
                    },
                ))
            }
        }
    }

    // ---- CRUD ----

    pub fn list(&self) -> Vec<PresetTestScheduleInfo> {
        let schedules = crate::lock_or_recover!(self.schedules.lock());
        let results = crate::lock_or_recover!(self.results.lock());
        let mut infos: Vec<PresetTestScheduleInfo> = schedules
            .values()
            .map(|s| schedule_to_info(s, results.get(&s.id).cloned()))
            .collect();
        infos.sort_by(|a, b| a.next_fire_at_millis.cmp(&b.next_fire_at_millis));
        infos
    }

    pub fn create(&self, req: PresetTestScheduleRequest) -> Result<PresetTestScheduleInfo> {
        let (kind, sched_type) = validate_request(&req)?;
        let now = current_timestamp_millis();
        let id = format!(
            "pts-{}",
            self.next_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        );
        let mut schedule = PresetTestSchedule {
            id: id.clone(),
            name: req.name.trim().to_string(),
            preset_kind: kind,
            preset_id: req.preset_id.trim().to_string(),
            preset_name: String::new(), // filled by caller via lookup
            schedule_type: sched_type,
            schedule_params: ScheduleParams {
                time: req.time.trim().to_string(),
                weekdays: req.weekdays.clone(),
                weekday: None,
                interval_minutes: req.interval_minutes,
            },
            enabled: req.enabled,
            created_at_millis: now,
            last_fired_at_millis: 0,
            next_fire_at_millis: 0,
        };
        schedule.next_fire_at_millis = compute_next_fire_at(&schedule, now);
        let info = schedule_to_info(&schedule, None);
        {
            let mut schedules = crate::lock_or_recover!(self.schedules.lock());
            schedules.insert(id, schedule);
            self.persist_locked(&schedules);
        }
        self.notify.notify_one();
        Ok(info)
    }

    pub fn update(
        &self,
        id: &str,
        req: PresetTestScheduleUpdateRequest,
    ) -> Result<PresetTestScheduleInfo> {
        let mut schedules = crate::lock_or_recover!(self.schedules.lock());
        let schedule = schedules
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("定时测试任务不存在"))?;
        if let Some(name) = req.name {
            let trimmed = name.trim().to_string();
            if !trimmed.is_empty() {
                schedule.name = trimmed;
            }
        }
        let needs_recompute = req.schedule_type.is_some()
            || req.time.is_some()
            || req.weekdays.is_some()
            || req.interval_minutes.is_some()
            || req.enabled.is_some();
        if let Some(kind_str) = req.preset_kind {
            if let Some(kind) = parse_preset_kind(&kind_str) {
                schedule.preset_kind = kind;
            }
        }
        if let Some(preset_id) = req.preset_id {
            let trimmed = preset_id.trim().to_string();
            if !trimmed.is_empty() {
                schedule.preset_id = trimmed;
            }
        }
        if let Some(sched_type_str) = req.schedule_type {
            if let Some(sched_type) = parse_schedule_type(&sched_type_str) {
                schedule.schedule_type = sched_type;
            }
        }
        if let Some(time) = req.time {
            schedule.schedule_params.time = time.trim().to_string();
        }
        if let Some(weekdays) = req.weekdays {
            schedule.schedule_params.weekdays = weekdays;
        }
        if let Some(interval) = req.interval_minutes {
            schedule.schedule_params.interval_minutes = interval;
        }
        if let Some(enabled) = req.enabled {
            schedule.enabled = enabled;
        }
        // Recompute next fire time: always recalculate when schedule params
        // changed (schedule_type, time, weekdays, interval), or when the task
        // was re-enabled, or if the existing next_fire has already passed.
        let now = current_timestamp_millis();
        if schedule.enabled {
            if needs_recompute
                || schedule.next_fire_at_millis == 0
                || schedule.next_fire_at_millis <= now
            {
                schedule.next_fire_at_millis = compute_next_fire_at(schedule, now);
            }
        } else {
            schedule.next_fire_at_millis = 0;
        }
        let _info = schedule_to_info(schedule, None);
        let results_snapshot = {
            let results = crate::lock_or_recover!(self.results.lock());
            results.get(id).cloned()
        };
        self.persist_locked(&schedules);
        drop(schedules);
        self.notify.notify_one();
        // Re-fetch to include result
        Ok(schedule_to_info_by_id(self, id, results_snapshot))
    }

    pub fn delete(&self, id: &str) -> bool {
        let removed = {
            let mut schedules = crate::lock_or_recover!(self.schedules.lock());
            schedules.remove(id).is_some()
        };
        if removed {
            // Remove result and persist in consistent lock order (schedules -> results).
            let results_snapshot = {
                let mut results = crate::lock_or_recover!(self.results.lock());
                results.remove(id);
                results.clone()
            };
            let schedules_snapshot = {
                let schedules = crate::lock_or_recover!(self.schedules.lock());
                schedules.clone()
            };
            self.persist_results_locked(&schedules_snapshot, &results_snapshot);
            self.notify.notify_one();
        }
        removed
    }

    pub fn set_preset_name_if_exists(&self, preset_id: &str, preset_name: &str, kind: &PresetKind) {
        let mut schedules = crate::lock_or_recover!(self.schedules.lock());
        let mut changed = false;
        for s in schedules.values_mut() {
            if s.preset_id == preset_id && s.preset_kind == *kind {
                s.preset_name = preset_name.to_string();
                changed = true;
            }
        }
        if changed {
            self.persist_locked(&schedules);
        }
    }

    /// Fire a test for a manually-triggered schedule (used by run-now endpoint).
    pub async fn fire_test_for_manual(
        &self,
        schedule: &PresetTestSchedule,
        auth_manager: &AuthPresetManager,
        proxy_manager: &crate::proxy::ProxyManager,
        workspace_settings: &SettingsManager,
    ) -> Result<PresetTestResult, AppError> {
        self.fire_test(schedule, auth_manager, proxy_manager, workspace_settings)
            .await
    }

    /// Store a manual test result and update the schedule's last_fired timestamp.
    pub fn store_manual_result(&self, schedule_id: &str, record: PresetTestScheduleResult) {
        {
            let mut schedules = crate::lock_or_recover!(self.schedules.lock());
            if let Some(s) = schedules.get_mut(schedule_id) {
                s.last_fired_at_millis = record.fired_at_millis;
            }
            self.persist_locked(&schedules);
        }
        {
            let mut results = crate::lock_or_recover!(self.results.lock());
            results.insert(schedule_id.to_string(), record);
            let schedules = crate::lock_or_recover!(self.schedules.lock());
            self.persist_results_locked(&schedules, &results);
        }
        self.notify.notify_one();
    }

    fn persist_locked(&self, schedules: &HashMap<String, PresetTestSchedule>) {
        let registry = StoredPresetTestScheduleRegistry {
            schedules: schedules.values().cloned().collect(),
            results: {
                let results = crate::lock_or_recover!(self.results.lock());
                results.values().cloned().collect()
            },
        };
        if let Err(e) = persist_registry(&self.file, &registry) {
            warn!("persist preset test schedule registry failed: {e}");
        }
    }

    fn persist_results_locked(
        &self,
        schedules: &HashMap<String, PresetTestSchedule>,
        results: &HashMap<String, PresetTestScheduleResult>,
    ) {
        let registry = StoredPresetTestScheduleRegistry {
            schedules: schedules.values().cloned().collect(),
            results: results.values().cloned().collect(),
        };
        if let Err(e) = persist_registry(&self.file, &registry) {
            warn!("persist preset test schedule registry failed: {e}");
        }
    }
}

// ---- Helpers ----

fn schedule_to_info_by_id(
    scheduler: &PresetTestScheduler,
    id: &str,
    result: Option<PresetTestScheduleResult>,
) -> PresetTestScheduleInfo {
    let schedules = crate::lock_or_recover!(scheduler.schedules.lock());
    let schedule = schedules.get(id);
    match schedule {
        Some(s) => schedule_to_info(s, result),
        None => PresetTestScheduleInfo {
            id: id.to_string(),
            name: String::new(),
            preset_kind: String::new(),
            preset_id: String::new(),
            preset_name: String::new(),
            schedule_type: String::new(),
            schedule_desc: String::new(),
            time: String::new(),
            weekdays: Vec::new(),
            interval_minutes: 0,
            enabled: false,
            created_at_millis: 0,
            last_fired_at_millis: 0,
            next_fire_at_millis: 0,
            last_result: result,
        },
    }
}

fn schedule_to_info(
    schedule: &PresetTestSchedule,
    last_result: Option<PresetTestScheduleResult>,
) -> PresetTestScheduleInfo {
    PresetTestScheduleInfo {
        id: schedule.id.clone(),
        name: schedule.name.clone(),
        preset_kind: match schedule.preset_kind {
            PresetKind::Api => "api".to_string(),
            PresetKind::Claude => "claude".to_string(),
        },
        preset_id: schedule.preset_id.clone(),
        preset_name: schedule.preset_name.clone(),
        schedule_type: match schedule.schedule_type {
            ScheduleType::Daily => "daily".to_string(),
            ScheduleType::Weekly => "weekly".to_string(),
            ScheduleType::Interval => "interval".to_string(),
        },
        schedule_desc: describe_schedule(schedule),
        time: schedule.schedule_params.time.clone(),
        weekdays: schedule.schedule_params.weekdays.clone(),
        interval_minutes: schedule.schedule_params.interval_minutes,
        enabled: schedule.enabled,
        created_at_millis: schedule.created_at_millis,
        last_fired_at_millis: schedule.last_fired_at_millis,
        next_fire_at_millis: schedule.next_fire_at_millis,
        last_result,
    }
}

fn describe_schedule(schedule: &PresetTestSchedule) -> String {
    match schedule.schedule_type {
        ScheduleType::Daily => format!("每天 {}", schedule.schedule_params.time),
        ScheduleType::Weekly => format!(
            "每周{} {}",
            weekdays_desc(&schedule.schedule_params.weekdays),
            schedule.schedule_params.time
        ),
        ScheduleType::Interval => {
            let mins = schedule.schedule_params.interval_minutes;
            if mins >= 60 && mins % 60 == 0 {
                format!("每 {} 小时", mins / 60)
            } else {
                format!("每 {} 分钟", mins)
            }
        }
    }
}

fn weekday_name(weekday: u32) -> &'static str {
    match weekday {
        0 => "日",
        1 => "一",
        2 => "二",
        3 => "三",
        4 => "四",
        5 => "五",
        6 => "六",
        _ => "?",
    }
}

/// Build a human-readable description of selected weekdays, e.g. "一、三、五".
fn weekdays_desc(weekdays: &[u32]) -> String {
    if weekdays.is_empty() {
        return String::new();
    }
    let mut sorted: Vec<u32> = weekdays.to_vec();
    sorted.sort();
    sorted.dedup();
    // Order by Monday-first for display: 1..=6 then 0
    sorted.sort_by_key(|&d| if d == 0 { 7 } else { d });
    sorted
        .iter()
        .map(|&d| format!("周{}", weekday_name(d)))
        .collect::<Vec<_>>()
        .join("、")
}

pub fn parse_preset_kind_public(s: &str) -> Option<PresetKind> {
    parse_preset_kind(s)
}

fn parse_preset_kind(s: &str) -> Option<PresetKind> {
    match s.trim().to_lowercase().as_str() {
        "api" => Some(PresetKind::Api),
        "claude" => Some(PresetKind::Claude),
        _ => None,
    }
}

fn parse_schedule_type(s: &str) -> Option<ScheduleType> {
    match s.trim().to_lowercase().as_str() {
        "daily" => Some(ScheduleType::Daily),
        "weekly" => Some(ScheduleType::Weekly),
        "interval" => Some(ScheduleType::Interval),
        _ => None,
    }
}

fn validate_request(req: &PresetTestScheduleRequest) -> Result<(PresetKind, ScheduleType)> {
    if req.name.trim().is_empty() {
        anyhow::bail!("任务名称不能为空");
    }
    if req.preset_id.trim().is_empty() {
        anyhow::bail!("预设 ID 不能为空");
    }
    let kind = parse_preset_kind(&req.preset_kind)
        .ok_or_else(|| anyhow::anyhow!("预设类型无效，请使用 api 或 claude"))?;
    let sched_type = parse_schedule_type(&req.schedule_type)
        .ok_or_else(|| anyhow::anyhow!("调度类型无效，请使用 daily、weekly 或 interval"))?;
    match sched_type {
        ScheduleType::Daily | ScheduleType::Weekly => {
            if parse_hhmm(&req.time).is_none() {
                anyhow::bail!("时间格式无效，请使用 HH:MM 格式");
            }
            if sched_type == ScheduleType::Weekly && req.weekdays.is_empty() {
                anyhow::bail!("每周模式至少选择一个星期");
            }
            if sched_type == ScheduleType::Weekly && req.weekdays.iter().any(|&d| d > 6) {
                anyhow::bail!("星期无效，请使用 0-6（0=周日）");
            }
        }
        ScheduleType::Interval => {
            if req.interval_minutes == 0 {
                anyhow::bail!("间隔分钟数必须大于 0");
            }
        }
    }
    Ok((kind, sched_type))
}

fn parse_hhmm(s: &str) -> Option<(u8, u8)> {
    let (h, m) = s.trim().split_once(':')?;
    let h: u8 = h.parse().ok()?;
    let m: u8 = m.parse().ok()?;
    (h < 24 && m < 60).then_some((h, m))
}

/// 计算下次触发时间（epoch millis）。
fn compute_next_fire_at(schedule: &PresetTestSchedule, now_millis: u64) -> u64 {
    match schedule.schedule_type {
        ScheduleType::Daily => {
            let Some((h, m)) = parse_hhmm(&schedule.schedule_params.time) else {
                return 0;
            };
            next_daily_fire(now_millis, h, m)
        }
        ScheduleType::Weekly => {
            let Some((h, m)) = parse_hhmm(&schedule.schedule_params.time) else {
                return 0;
            };
            if schedule.schedule_params.weekdays.is_empty() {
                return 0;
            }
            // Find the earliest next fire across all selected weekdays.
            schedule
                .schedule_params
                .weekdays
                .iter()
                .filter_map(|&d| {
                    let t = next_weekly_fire(now_millis, weekday_from_u32(d), h, m);
                    (t > 0).then_some(t)
                })
                .min()
                .unwrap_or(0)
        }
        ScheduleType::Interval => {
            let interval_ms = schedule.schedule_params.interval_minutes * 60 * 1000;
            if interval_ms == 0 {
                return 0;
            }
            // If the schedule has never fired, fire after one interval from now.
            // Otherwise, fire at the next interval boundary after the last fire.
            let base = if schedule.last_fired_at_millis > 0 {
                schedule.last_fired_at_millis
            } else {
                now_millis
            };
            let next = base + interval_ms;
            // If already past, keep adding intervals until future.
            let mut result = next;
            while result <= now_millis {
                result += interval_ms;
            }
            result
        }
    }
}

fn weekday_from_u32(n: u32) -> Weekday {
    match n {
        0 => Weekday::Sunday,
        1 => Weekday::Monday,
        2 => Weekday::Tuesday,
        3 => Weekday::Wednesday,
        4 => Weekday::Thursday,
        5 => Weekday::Friday,
        6 => Weekday::Saturday,
        _ => Weekday::Sunday,
    }
}

fn next_daily_fire(now_millis: u64, hour: u8, minute: u8) -> u64 {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let today_target = now
        .date()
        .with_time(time::Time::from_hms(hour, minute, 0).unwrap_or(time::Time::MIDNIGHT))
        .assume_offset(now.offset());
    let today_ms = today_target.unix_timestamp() as u64 * 1000;
    if today_ms > now_millis {
        today_ms
    } else {
        // Tomorrow
        let tomorrow = now.date().next_day().unwrap_or(now.date());
        tomorrow
            .with_time(time::Time::from_hms(hour, minute, 0).unwrap_or(time::Time::MIDNIGHT))
            .assume_offset(now.offset())
            .unix_timestamp() as u64
            * 1000
    }
}

fn next_weekly_fire(now_millis: u64, target_weekday: Weekday, hour: u8, minute: u8) -> u64 {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    let target_time = time::Time::from_hms(hour, minute, 0).unwrap_or(time::Time::MIDNIGHT);
    let mut candidate = now
        .date()
        .with_time(target_time)
        .assume_offset(now.offset());
    // Walk forward up to 7 days to find the target weekday.
    for _ in 0..8 {
        if candidate.weekday() == target_weekday
            && (candidate.unix_timestamp() as u64 * 1000) > now_millis
        {
            return candidate.unix_timestamp() as u64 * 1000;
        }
        candidate = candidate
            .date()
            .next_day()
            .unwrap_or(candidate.date())
            .with_time(target_time)
            .assume_offset(now.offset());
    }
    // Fallback: 7 days from now
    now_millis + 7 * 24 * 60 * 60 * 1000
}

fn load_registry(file: &Path) -> Result<StoredPresetTestScheduleRegistry> {
    if !file.exists() {
        return Ok(StoredPresetTestScheduleRegistry::default());
    }
    let content = fs::read(file)
        .with_context(|| format!("cannot read preset test schedule registry {}", file.display()))?;
    let mut registry: StoredPresetTestScheduleRegistry = serde_json::from_slice(&content)
        .with_context(|| {
            format!("cannot parse preset test schedule registry {}", file.display())
        })?;
    // Migrate legacy single weekday field to weekdays array.
    for schedule in registry.schedules.iter_mut() {
        if schedule.schedule_params.weekdays.is_empty() {
            if let Some(legacy) = schedule.schedule_params.weekday.take() {
                schedule.schedule_params.weekdays = vec![legacy];
            }
        }
    }
    Ok(registry)
}

fn persist_registry(file: &Path, registry: &StoredPresetTestScheduleRegistry) -> Result<()> {
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_vec_pretty(registry)
        .context("cannot encode preset test schedule registry")?;
    fs::write(file, content).with_context(|| {
        format!("cannot write preset test schedule registry {}", file.display())
    })?;
    Ok(())
}
