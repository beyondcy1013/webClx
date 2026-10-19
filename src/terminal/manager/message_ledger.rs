//! 终端消息投递账本（message delivery ledger）。
//!
//! 把 verify_submission 投递的去重/冲突状态持久化到磁盘 JSON，让 dedup 语义
//! 跨服务重启保留，并让每条投递都有显式状态，便于排查“消息需要手工 Enter”
//! 之类的问题。
//!
//! 持久性边界：认领（[`TerminalMessageLedger::try_claim`]）与确认写入
//! （[`TerminalMessageLedger::upsert_confirmed`]）必须先成功落盘才算准入。
//! 文件损坏或版本未知时服务仍启动，但账本拒绝一切变更与落盘，原文件按字节
//! 保留（不删除、不被静默覆盖）；运维修复文件并重启服务后恢复。dedup TTL
//! 只是状态展示口径：过期不代表正文没有到达终端，任何已有记录都阻止同一
//! delivery_id 自动重发正文。

use std::{
    collections::HashMap,
    fs, io,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tracing::warn;

use super::super::TERMINAL_MESSAGE_DEDUP_TTL_SECS;

const MESSAGE_LEDGER_VERSION: u32 = 1;
/// 投递记录最多保留 7 天。
const MESSAGE_LEDGER_MAX_AGE_SECS: u64 = 7 * 24 * 60 * 60;
/// 账本最多保留最近 MESSAGE_LEDGER_MAX_RECORDS 条记录（按 updated_at）。
const MESSAGE_LEDGER_MAX_RECORDS: usize = 2000;

/// 单条投递的生命周期状态，JSON 中以 snake_case 存储。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DeliveryState {
    /// 已认领投递权，正文尚未写入终端。
    Claimed,
    /// 正文已写入终端，等待 Enter 提交。
    Injected,
    /// 已执行过 Enter 提交尝试。
    Submitted,
    /// rollout 历史确认投递已成为真实 user message。
    Confirmed,
    /// 提交尝试完成但 rollout 未确认。
    Unconfirmed,
    /// 正文发送或 Enter 补发失败。
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DeliveryRecord {
    pub session_id: String,
    pub delivery_id: String,
    /// 消息正文 SHA-256，用于识别同一 delivery_id 的内容冲突。
    pub digest: String,
    pub state: DeliveryState,
    /// 首次认领时刻（epoch 秒），dedup TTL 以此为基准。
    pub claimed_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub submit_attempts: u32,
    #[serde(default)]
    pub last_error: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct StoredMessageLedger {
    /// 必须存在且等于 MESSAGE_LEDGER_VERSION；缺字段或未知版本拒绝加载。
    version: u32,
    /// 必须存在；记录逐条校验（非空 id、无 NUL、键不重复）。
    records: Vec<DeliveryRecord>,
}

/// 以 `{session_id}\0{delivery_id}` 为键的投递记录表，可选持久化到 JSON。
/// path 为 None 时仅驻留内存（测试构造路径使用）。
///
/// `load_error`/`persist_error` 记录账本健康：任一存在时准入一律拒绝——
/// 内存视图可能与磁盘不一致，不能据此放行新的投递认领。
pub(crate) struct TerminalMessageLedger {
    path: Option<PathBuf>,
    records: HashMap<String, DeliveryRecord>,
    load_error: Option<String>,
    persist_error: Option<String>,
}

fn delivery_key(session_id: &str, delivery_id: &str) -> String {
    format!("{session_id}\0{delivery_id}")
}

/// 校验可入账的投递身份：非空（全空白按调用方 trim 语义同样拒绝）且不
/// 含 `\0`——`\0` 是持久化键分隔符，混入记录会让账本在下次启动时整体
/// 被拒绝。load、try_claim 与 upsert_confirmed 在变更前统一校验。
fn validate_delivery_identity(session_id: &str, delivery_id: &str) -> Result<()> {
    if session_id.trim().is_empty() || delivery_id.trim().is_empty() {
        anyhow::bail!("terminal message delivery identity must be non-empty");
    }
    if session_id.contains('\0') || delivery_id.contains('\0') {
        anyhow::bail!("terminal message delivery identity must not contain NUL");
    }
    Ok(())
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_secs())
        .unwrap_or(0)
}

impl TerminalMessageLedger {
    /// 从磁盘加载账本；文件缺失（NotFound）是唯一合法的空账本起点。
    /// 任何加载失败都让服务带着只读空账本启动：不删除、不覆盖原文件，
    /// 之后一切准入/迁移被拒绝，运维修复文件并重启服务后恢复。
    pub fn load(path: Option<PathBuf>) -> Self {
        let (records, load_error) = match path.as_deref() {
            Some(path) => match Self::load_records(path) {
                Ok(records) => (records, None),
                Err(error) => {
                    warn!(
                        "terminal message delivery ledger disabled {}: {error:#}",
                        path.display()
                    );
                    (HashMap::new(), Some(format!("{error:#}")))
                }
            },
            None => (HashMap::new(), None),
        };
        Self {
            path,
            records,
            load_error,
            persist_error: None,
        }
    }

    pub fn in_memory() -> Self {
        Self {
            path: None,
            records: HashMap::new(),
            load_error: None,
            persist_error: None,
        }
    }

    /// 严格加载：读取与解析错误一律上报，缺字段/未知版本/重复键/非法 id
    /// （空串或含 `\0` 分隔符）一律拒绝——不静默折叠成空账本。
    fn load_records(path: &Path) -> Result<HashMap<String, DeliveryRecord>> {
        let content = match fs::read(path) {
            Ok(content) => content,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok(HashMap::new());
            }
            Err(error) => {
                return Err(error).context("读取终端消息投递账本失败");
            }
        };
        let stored: StoredMessageLedger =
            serde_json::from_slice(&content).context("解析终端消息投递账本失败")?;
        if stored.version != MESSAGE_LEDGER_VERSION {
            anyhow::bail!("终端消息投递账本版本不受支持: {}", stored.version);
        }
        let mut records = HashMap::with_capacity(stored.records.len());
        for record in stored.records {
            validate_delivery_identity(&record.session_id, &record.delivery_id)
                .context("终端消息投递账本存在非法记录键")?;
            let key = delivery_key(&record.session_id, &record.delivery_id);
            if records.insert(key, record).is_some() {
                anyhow::bail!("终端消息投递账本存在重复记录键");
            }
        }
        Ok(records)
    }

    /// 账本不健康时拒绝投递相关的状态判断：加载失败或上次落盘失败都
    /// 意味着内存视图可能与磁盘不一致，不能假装 unknown 放行重发。
    /// 消息不携带文件路径与文件内容。
    pub fn ensure_healthy(&self) -> Result<()> {
        if let Some(error) = &self.load_error {
            anyhow::bail!("terminal message delivery ledger unavailable: load failed: {error}");
        }
        if let Some(error) = &self.persist_error {
            anyhow::bail!("terminal message delivery ledger unavailable: persist failed: {error}");
        }
        Ok(())
    }

    /// 准入（写新记录）前的健康门：load_error 直接拒绝；persist_error
    /// 先重试一次落盘——瞬时失败借此恢复，仍失败则本次准入拒绝。
    /// manager 认领路径的锁内预检也用本门而不是 ensure_healthy：否则
    /// persist_error 会在到达重试点之前先把请求挡死，永远无法恢复。
    pub(super) fn ensure_healthy_for_admission(&mut self) -> Result<()> {
        if let Some(error) = &self.load_error {
            anyhow::bail!("terminal message delivery ledger unavailable: load failed: {error}");
        }
        if self.persist_error.is_some() {
            self.persist()?;
        }
        Ok(())
    }

    pub fn record(&self, session_id: &str, delivery_id: &str) -> Option<DeliveryRecord> {
        self.records
            .get(&delivery_key(session_id, delivery_id))
            .cloned()
    }

    /// 同一 delivery_id 已有不同正文的记录时视为冲突；账本不健康时直接报错。
    pub fn ensure_no_conflict(
        &self,
        session_id: &str,
        delivery_id: &str,
        digest: &str,
    ) -> Result<()> {
        self.ensure_healthy()?;
        if self
            .records
            .get(&delivery_key(session_id, delivery_id))
            .is_some_and(|record| record.digest != digest)
        {
            anyhow::bail!("delivery_id content conflict");
        }
        Ok(())
    }

    /// 是否仍在“在途”窗口：已认领/注入/提交且未超过 dedup TTL。
    pub fn is_active(&self, session_id: &str, delivery_id: &str, now: u64) -> bool {
        self.records
            .get(&delivery_key(session_id, delivery_id))
            .is_some_and(|record| {
                matches!(
                    record.state,
                    DeliveryState::Claimed | DeliveryState::Injected | DeliveryState::Submitted
                ) && now.saturating_sub(record.claimed_at) <= TERMINAL_MESSAGE_DEDUP_TTL_SECS
            })
    }

    /// 尝试认领投递。认领是持久化边界：写入 Claimed 记录并成功落盘才
    /// 返回 true。
    ///
    /// 同一 (session, delivery_id) 只要已有记录——无论状态、无论是否过
    /// 期——都返回 false：dedup TTL 过期不代表正文没有写入过终端，因此
    /// Claimed/Injected/Submitted/Unconfirmed/Failed 一律不自动重发。
    /// 容量满（剔除过期 Confirmed 后仍达到上限）时拒绝新认领；不为接纳
    /// 新投递淘汰结果不确定的记录。dedup 保证以留存记录的生命周期为界，
    /// 不承诺超出保留期的 exactly-once。
    pub fn try_claim(
        &mut self,
        session_id: &str,
        delivery_id: &str,
        digest: String,
        claimed_at: u64,
    ) -> Result<bool> {
        validate_delivery_identity(session_id, delivery_id)?;
        self.ensure_healthy_for_admission()?;
        self.ensure_no_conflict(session_id, delivery_id, &digest)?;
        self.prune();
        let key = delivery_key(session_id, delivery_id);
        if self.records.contains_key(&key) {
            return Ok(false);
        }
        if self.records.len() >= MESSAGE_LEDGER_MAX_RECORDS {
            anyhow::bail!("terminal message delivery ledger capacity reached");
        }
        self.records.insert(
            key,
            DeliveryRecord {
                session_id: session_id.to_string(),
                delivery_id: delivery_id.to_string(),
                digest,
                state: DeliveryState::Claimed,
                claimed_at,
                updated_at: claimed_at,
                submit_attempts: 0,
                last_error: String::new(),
            },
        );
        // 落盘失败时保留内存里的 Claimed 记录（rename/目录 fsync 成败存在
        // 歧义）并拒绝发送；同一 key 的后续请求仍被保守去重，不同 key 在下
        // 次准入重试成功后可继续。
        self.persist()?;
        Ok(true)
    }

    /// rollout 历史已确认该投递：写入/更新 Confirmed 记录，返回方视为不投递。
    /// 锁内重新校验健康与冲突（调用方的预检在锁外已失效）；digest 冲突直接
    /// 报错、绝不覆盖既有 digest；容量满且键不存在时拒绝插入，不淘汰不确定
    /// 记录。
    pub fn upsert_confirmed(
        &mut self,
        session_id: &str,
        delivery_id: &str,
        digest: String,
    ) -> Result<()> {
        validate_delivery_identity(session_id, delivery_id)?;
        self.ensure_healthy_for_admission()?;
        self.ensure_no_conflict(session_id, delivery_id, &digest)?;
        self.prune();
        let key = delivery_key(session_id, delivery_id);
        if !self.records.contains_key(&key) && self.records.len() >= MESSAGE_LEDGER_MAX_RECORDS {
            anyhow::bail!("terminal message delivery ledger capacity reached");
        }
        let now = now_secs();
        let changed = match self.records.entry(key) {
            std::collections::hash_map::Entry::Occupied(mut entry) => {
                let record = entry.get_mut();
                if record.state == DeliveryState::Confirmed {
                    false
                } else {
                    record.state = DeliveryState::Confirmed;
                    record.updated_at = now;
                    true
                }
            }
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(DeliveryRecord {
                    session_id: session_id.to_string(),
                    delivery_id: delivery_id.to_string(),
                    digest,
                    state: DeliveryState::Confirmed,
                    claimed_at: now,
                    updated_at: now,
                    submit_attempts: 0,
                    last_error: String::new(),
                });
                true
            }
        };
        if changed {
            self.persist()?;
        }
        Ok(())
    }

    /// 会话被移除时清理其所有的投递记录——这是唯一合法的记录删除路径
    /// （删除终端即放弃其投递保证）；发送失败只能 mark_failed，不能移除。
    pub fn remove_session(&mut self, session_id: &str) {
        let before_len = self.records.len();
        self.records
            .retain(|_, record| record.session_id != session_id);
        if self.records.len() != before_len {
            self.persist_observed();
        }
    }

    /// 正文已写入终端。只允许 Claimed -> Injected：迟到的 worker
    /// bookkeeping 不得回退 Submitted/Unconfirmed/Failed。
    pub fn mark_injected(&mut self, session_id: &str, delivery_id: &str) {
        self.transition(session_id, delivery_id, |record| {
            if record.state != DeliveryState::Claimed {
                return false;
            }
            record.state = DeliveryState::Injected;
            true
        });
    }

    /// 记录 Enter 提交尝试总数并把状态推进到 Submitted；提交次数单调
    /// 不减（max），无变化时不写盘。Confirmed 记录由 transition 拦截。
    pub fn record_submit_attempt(&mut self, session_id: &str, delivery_id: &str, attempts: u32) {
        self.transition(session_id, delivery_id, |record| {
            let attempts = record.submit_attempts.max(attempts);
            if record.state == DeliveryState::Submitted && record.submit_attempts == attempts {
                return false;
            }
            record.state = DeliveryState::Submitted;
            record.submit_attempts = attempts;
            true
        });
    }

    /// rollout 确认投递完成；Confirmed 是终态，幂等确认不重复写盘。
    pub fn mark_confirmed(&mut self, session_id: &str, delivery_id: &str) {
        self.transition(session_id, delivery_id, |record| {
            record.state = DeliveryState::Confirmed;
            true
        });
    }

    /// 提交尝试结束但 rollout 未确认；不覆盖已确认的记录。
    pub fn mark_unconfirmed(&mut self, session_id: &str, delivery_id: &str) {
        self.transition(session_id, delivery_id, |record| {
            if record.state == DeliveryState::Unconfirmed {
                return false;
            }
            record.state = DeliveryState::Unconfirmed;
            true
        });
    }

    /// 投递链路报错：标记 Failed 并保留去重记录——正文是否已到达终端
    /// 不确定，记录消失会让同一 delivery_id 被自动重发；不覆盖已确认
    /// 的记录。
    pub fn mark_failed(&mut self, session_id: &str, delivery_id: &str, error: &str) {
        self.transition(session_id, delivery_id, |record| {
            if record.state == DeliveryState::Failed && record.last_error == error {
                return false;
            }
            record.state = DeliveryState::Failed;
            record.last_error = error.to_string();
            true
        });
    }

    /// 对存在的记录执行状态迁移；返回 false 表示未变化、不写盘。
    /// apply 必须返回“是否真的改动了记录”。
    /// 账本损坏（load_error）时拒绝一切内存变更——视图不可信。
    /// Confirmed 是终态：除幂等确认外不得迁移（rollout 已确认它就是
    /// user message，迟到的失败/未确认不能改变这个事实）。
    fn transition(
        &mut self,
        session_id: &str,
        delivery_id: &str,
        apply: impl FnOnce(&mut DeliveryRecord) -> bool,
    ) {
        if self.load_error.is_some() {
            warn!(
                "terminal message delivery ledger is unhealthy; transition skipped for {delivery_id}"
            );
            return;
        }
        let changed = match self.records.get_mut(&delivery_key(session_id, delivery_id)) {
            Some(record) => {
                // Confirmed 单调：除幂等确认（无副作用）外不得迁移。
                if record.state == DeliveryState::Confirmed {
                    return;
                }
                if apply(record) {
                    record.updated_at = now_secs();
                    true
                } else {
                    false
                }
            }
            None => return,
        };
        if changed {
            self.persist_observed();
        }
    }

    /// 每次变更后落盘；写盘成功清除 persist_error，失败记入 persist_error
    /// 并保留内存视图——后续准入借此保守失败，而不是假装写盘成功。
    /// 账本加载失败时拒绝写入：原子覆盖会销毁未知/损坏文件的原始内容。
    /// 写入经 atomic_file：临时文件 fsync + rename + 目录 fsync，崩溃后
    /// 只会看到旧账本或新账本，不会看到半个 JSON。
    fn persist(&mut self) -> Result<()> {
        let Some(path) = self.path.clone() else {
            return Ok(());
        };
        if self.load_error.is_some() {
            anyhow::bail!("terminal message delivery ledger persist refused: load failed");
        }
        self.prune();
        let mut records: Vec<DeliveryRecord> = self.records.values().cloned().collect();
        records.sort_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.session_id.cmp(&right.session_id))
                .then_with(|| left.delivery_id.cmp(&right.delivery_id))
        });
        let stored = StoredMessageLedger {
            version: MESSAGE_LEDGER_VERSION,
            records,
        };
        let result = serde_json::to_vec_pretty(&stored)
            .map_err(|error| {
                anyhow::anyhow!("encode terminal message delivery ledger failed: {error}")
            })
            .and_then(|content| {
                crate::atomic_file::write_atomic(&path, &content)
                    .context("persist terminal message delivery ledger failed")
            });
        match &result {
            Ok(()) => self.persist_error = None,
            Err(error) => {
                warn!(
                    "persist terminal message delivery ledger failed {}: {error:#}",
                    path.display()
                );
                self.persist_error = Some(error.to_string());
            }
        }
        result
    }

    /// 对变更后不需要向调用方传播落盘结果的路径使用：写盘失败只告警，
    /// 但 persist_error 已被记录，后续准入会保守失败或重试。
    fn persist_observed(&mut self) {
        if let Err(error) = self.persist() {
            warn!("terminal message delivery ledger write failed: {error:#}");
        }
    }

    /// 只淘汰已 Confirmed 且 updated_at 早于 7 天的记录。未确认/在途/
    /// 失败的记录代表投递结果不确定，绝不为接纳新投递而淘汰——容量
    /// 满时由准入方直接拒绝。
    fn prune(&mut self) {
        let now = now_secs();
        self.records.retain(|_, record| {
            record.state != DeliveryState::Confirmed
                || now.saturating_sub(record.updated_at) <= MESSAGE_LEDGER_MAX_AGE_SECS
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let directory = std::env::temp_dir()
                .join(format!("webclx-message-ledger-{label}-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&directory).unwrap();
            Self(directory)
        }

        fn ledger_path(&self) -> PathBuf {
            self.0.join("terminal-message-deliveries.json")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn test_record(
        session_id: &str,
        delivery_id: &str,
        state: DeliveryState,
        timestamp: u64,
    ) -> DeliveryRecord {
        DeliveryRecord {
            session_id: session_id.into(),
            delivery_id: delivery_id.into(),
            digest: "a".into(),
            state,
            claimed_at: timestamp,
            updated_at: timestamp,
            submit_attempts: 0,
            last_error: String::new(),
        }
    }

    #[test]
    fn claim_is_idempotent_and_rejects_conflicting_digest() {
        let mut ledger = TerminalMessageLedger::in_memory();
        let now = now_secs();

        assert!(
            ledger
                .try_claim("s1", "d1", "digest-a".into(), now)
                .unwrap()
        );
        assert!(
            !ledger
                .try_claim("s1", "d1", "digest-a".into(), now)
                .unwrap()
        );
        assert!(
            ledger
                .try_claim("s1", "d1", "digest-b".into(), now)
                .is_err()
        );
        // 不同 session 的相同 delivery_id 互不影响。
        assert!(
            ledger
                .try_claim("s2", "d1", "digest-a".into(), now)
                .unwrap()
        );
    }

    #[test]
    fn every_existing_state_blocks_reclaim_across_ttl_and_restart() {
        let directory = TestDirectory::new("dedup-states");
        let path = directory.ledger_path();
        let states = [
            ("d-claimed", DeliveryState::Claimed),
            ("d-injected", DeliveryState::Injected),
            ("d-submitted", DeliveryState::Submitted),
            ("d-confirmed", DeliveryState::Confirmed),
            ("d-unconfirmed", DeliveryState::Unconfirmed),
            ("d-failed", DeliveryState::Failed),
        ];
        {
            let mut ledger = TerminalMessageLedger::load(Some(path.clone()));
            let now = now_secs();
            for (id, state) in states {
                ledger
                    .records
                    .insert(delivery_key("s1", id), test_record("s1", id, state, now));
            }
            ledger.persist().unwrap();
        }

        // 重启重载后：所有状态的记录都阻止重复认领，dedup TTL 过期不
        // 授权重发——过期不代表正文没有到达过终端。
        let mut ledger = TerminalMessageLedger::load(Some(path));
        let after_ttl = now_secs() + TERMINAL_MESSAGE_DEDUP_TTL_SECS + 1;
        for (id, _) in states {
            assert!(
                !ledger.try_claim("s1", id, "a".into(), after_ttl).unwrap(),
                "{id} must stay deduplicated"
            );
        }
    }

    #[test]
    fn confirmed_record_blocks_new_claims() {
        let mut ledger = TerminalMessageLedger::in_memory();
        ledger
            .upsert_confirmed("s1", "d1", "digest".into())
            .unwrap();

        assert!(
            !ledger
                .try_claim("s1", "d1", "digest".into(), now_secs())
                .unwrap()
        );
        assert_eq!(ledger.record("s1", "d1").unwrap().state, DeliveryState::Confirmed);
    }

    #[test]
    fn confirmed_record_never_regresses() {
        let mut ledger = TerminalMessageLedger::in_memory();
        let now = now_secs();
        assert!(ledger.try_claim("s1", "d1", "a".into(), now).unwrap());
        ledger.record_submit_attempt("s1", "d1", 3);
        ledger.mark_confirmed("s1", "d1");

        // 之后到达的任何迁移都不得回退 Confirmed。
        ledger.mark_injected("s1", "d1");
        ledger.record_submit_attempt("s1", "d1", 9);
        ledger.mark_unconfirmed("s1", "d1");
        ledger.mark_failed("s1", "d1", "boom");
        ledger.mark_confirmed("s1", "d1"); // 幂等
        let record = ledger.record("s1", "d1").unwrap();
        assert_eq!(record.state, DeliveryState::Confirmed);
        assert_eq!(record.submit_attempts, 3);
        assert_eq!(record.last_error, "");

        // digest 冲突的确认写入被拒绝，且不覆盖既有 digest。
        assert!(
            ledger
                .upsert_confirmed("s1", "d1", "different".into())
                .is_err()
        );
        assert_eq!(ledger.record("s1", "d1").unwrap().digest, "a");
    }

    #[test]
    fn mark_injected_only_advances_claimed() {
        let mut ledger = TerminalMessageLedger::in_memory();
        let now = now_secs();
        for (id, state) in [
            ("claimed", DeliveryState::Claimed),
            ("injected", DeliveryState::Injected),
            ("submitted", DeliveryState::Submitted),
            ("unconfirmed", DeliveryState::Unconfirmed),
            ("failed", DeliveryState::Failed),
        ] {
            ledger
                .records
                .insert(delivery_key("s1", id), test_record("s1", id, state, now));
            ledger.mark_injected("s1", id);
        }
        // 迟到的 worker bookkeeping 只能推进 Claimed，不得回退后续状态。
        assert_eq!(ledger.record("s1", "claimed").unwrap().state, DeliveryState::Injected);
        assert_eq!(ledger.record("s1", "injected").unwrap().state, DeliveryState::Injected);
        assert_eq!(ledger.record("s1", "submitted").unwrap().state, DeliveryState::Submitted);
        assert_eq!(ledger.record("s1", "unconfirmed").unwrap().state, DeliveryState::Unconfirmed);
        assert_eq!(ledger.record("s1", "failed").unwrap().state, DeliveryState::Failed);
    }

    #[test]
    fn submit_attempts_are_monotonic() {
        let mut ledger = TerminalMessageLedger::in_memory();
        assert!(
            ledger
                .try_claim("s1", "d1", "a".into(), now_secs())
                .unwrap()
        );
        ledger.record_submit_attempt("s1", "d1", 3);
        ledger.record_submit_attempt("s1", "d1", 1); // 低水位不覆盖
        assert_eq!(ledger.record("s1", "d1").unwrap().submit_attempts, 3);
        assert_eq!(ledger.record("s1", "d1").unwrap().state, DeliveryState::Submitted);
    }

    #[test]
    fn remove_session_drops_records_and_persists() {
        let directory = TestDirectory::new("remove-session");
        let path = directory.ledger_path();
        {
            let mut ledger = TerminalMessageLedger::load(Some(path.clone()));
            ledger
                .try_claim("s1", "d1", "a".into(), now_secs())
                .unwrap();
            ledger
                .try_claim("s1", "d2", "a".into(), now_secs())
                .unwrap();
            ledger
                .try_claim("s2", "d1", "b".into(), now_secs())
                .unwrap();

            ledger.remove_session("s1");
            assert!(ledger.record("s1", "d1").is_none());
            assert!(ledger.record("s1", "d2").is_none());
            assert!(ledger.record("s2", "d1").is_some());

            // 清理未知会话不写盘、不报错。
            ledger.remove_session("unknown");
        }

        let ledger = TerminalMessageLedger::load(Some(path.clone()));
        assert!(ledger.record("s1", "d1").is_none());
        assert!(ledger.record("s1", "d2").is_none());
        assert!(ledger.record("s2", "d1").is_some());
    }

    #[test]
    fn missing_ledger_file_is_healthy_and_claim_persists() {
        let directory = TestDirectory::new("missing");
        let path = directory.ledger_path();
        let mut ledger = TerminalMessageLedger::load(Some(path.clone()));
        ledger.ensure_healthy().unwrap();
        assert!(
            ledger
                .try_claim("s1", "d1", "a".into(), now_secs())
                .unwrap()
        );

        let reloaded = TerminalMessageLedger::load(Some(path));
        assert_eq!(reloaded.record("s1", "d1").unwrap().state, DeliveryState::Claimed);
    }

    #[test]
    fn persisted_ledger_survives_reload() {
        let directory = TestDirectory::new("reload");
        let path = directory.ledger_path();
        let claimed_at = now_secs();
        {
            let mut ledger = TerminalMessageLedger::load(Some(path.clone()));
            ledger
                .try_claim("s1", "d1", "digest-x".into(), claimed_at)
                .unwrap();
            ledger.mark_injected("s1", "d1");
            ledger.record_submit_attempt("s1", "d1", 2);
        }

        let ledger = TerminalMessageLedger::load(Some(path.clone()));
        let record = ledger.record("s1", "d1").expect("record survives reload");
        assert_eq!(record.state, DeliveryState::Submitted);
        assert_eq!(record.submit_attempts, 2);
        assert_eq!(record.digest, "digest-x");
        assert_eq!(record.session_id, "s1");
        assert!(ledger.is_active("s1", "d1", claimed_at));

        // 重新加载后的账本仍以 {session_id, delivery_id} 为键去重。
        let raw: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(raw["version"], 1);
    }

    #[test]
    fn malformed_ledger_is_preserved_and_refuses_mutation() {
        let directory = TestDirectory::new("malformed");
        let path = directory.ledger_path();
        let original = b"{not-json";
        fs::write(&path, original).unwrap();

        let mut ledger = TerminalMessageLedger::load(Some(path.clone()));
        // 服务带空内存视图启动，但账本不健康：准入与状态查询一律报错。
        assert!(ledger.ensure_healthy().is_err());
        assert!(ledger.record("s1", "d1").is_none());
        assert!(
            ledger
                .try_claim("s1", "d1", "a".into(), now_secs())
                .is_err()
        );
        assert!(ledger.upsert_confirmed("s1", "d1", "a".into()).is_err());
        // 账本损坏时迁移不做任何内存变更。
        ledger.mark_confirmed("s1", "d1");
        ledger.remove_session("s1");
        // 损坏文件按字节保留：不删除、不覆盖，运维修复后重启恢复。
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn unknown_version_ledger_is_preserved_and_rejected() {
        let directory = TestDirectory::new("version");
        let path = directory.ledger_path();
        let original = br#"{"version":99,"records":[]}"#;
        fs::write(&path, original).unwrap();

        let mut ledger = TerminalMessageLedger::load(Some(path.clone()));
        assert!(ledger.ensure_healthy().is_err());
        assert!(
            ledger
                .try_claim("s1", "d1", "a".into(), now_secs())
                .is_err()
        );
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn missing_top_level_fields_are_rejected() {
        for (label, original) in [
            ("noversion", r#"{"records":[]}"#),
            ("norecords", r#"{"version":1}"#),
        ] {
            let directory = TestDirectory::new(label);
            let path = directory.ledger_path();
            fs::write(&path, original).unwrap();
            let mut ledger = TerminalMessageLedger::load(Some(path.clone()));
            assert!(ledger.ensure_healthy().is_err(), "{label}");
            assert!(
                ledger
                    .try_claim("s1", "d1", "a".into(), now_secs())
                    .is_err(),
                "{label}"
            );
            assert_eq!(fs::read(&path).unwrap(), original.as_bytes(), "{label}");
        }
    }

    #[test]
    fn duplicate_and_invalid_record_keys_are_rejected() {
        for (label, original) in [
            // 两个记录折叠成同一 {session}\0{delivery} 键：拒绝而非取其一。
            (
                "duplicate",
                concat!(
                    r#"{"version":1,"records":["#,
                    r#"{"session_id":"s1","delivery_id":"d1","digest":"a","state":"confirmed","claimed_at":1,"updated_at":1},"#,
                    r#"{"session_id":"s1","delivery_id":"d1","digest":"b","state":"claimed","claimed_at":1,"updated_at":1}"#,
                    r#"]}"#
                ),
            ),
            (
                "empty-session",
                r#"{"version":1,"records":[{"session_id":"","delivery_id":"d1","digest":"a","state":"claimed","claimed_at":1,"updated_at":1}]}"#,
            ),
            (
                "empty-delivery",
                r#"{"version":1,"records":[{"session_id":"s1","delivery_id":"","digest":"a","state":"claimed","claimed_at":1,"updated_at":1}]}"#,
            ),
            (
                "nul-session",
                r#"{"version":1,"records":[{"session_id":"s\u00001","delivery_id":"d1","digest":"a","state":"claimed","claimed_at":1,"updated_at":1}]}"#,
            ),
            (
                "nul-delivery",
                r#"{"version":1,"records":[{"session_id":"s1","delivery_id":"d\u00001","digest":"a","state":"claimed","claimed_at":1,"updated_at":1}]}"#,
            ),
        ] {
            let directory = TestDirectory::new(label);
            let path = directory.ledger_path();
            fs::write(&path, original).unwrap();
            let mut ledger = TerminalMessageLedger::load(Some(path.clone()));
            assert!(ledger.ensure_healthy().is_err(), "{label}");
            assert!(
                ledger
                    .try_claim("s1", "d1", "a".into(), now_secs())
                    .is_err(),
                "{label}"
            );
            // 非法文件按字节保留。
            assert_eq!(fs::read(&path).unwrap(), original.as_bytes(), "{label}");
        }
    }

    #[test]
    fn invalid_delivery_identity_is_rejected_before_mutation() {
        let directory = TestDirectory::new("bad-id");
        let path = directory.ledger_path();
        let mut ledger = TerminalMessageLedger::load(Some(path.clone()));
        // 先写一条合法记录作为基线。
        assert!(
            ledger
                .try_claim("s1", "d1", "a".into(), now_secs())
                .unwrap()
        );
        let on_disk = fs::read(&path).unwrap();

        // 空/全空白/含键分隔符 NUL 的身份一律在变更前拒绝：否则含 NUL
        // 的记录落盘后会让账本在下次启动时被整体判为损坏。
        for (session, delivery) in [
            ("", "d2"),
            ("   ", "d2"),
            ("s1", ""),
            ("s1", "  "),
            ("s\0x", "d2"),
            ("s1", "d\0x"),
        ] {
            assert!(
                ledger
                    .try_claim(session, delivery, "a".into(), now_secs())
                    .is_err(),
                "{session:?}/{delivery:?} must be rejected"
            );
            assert!(
                ledger
                    .upsert_confirmed(session, delivery, "a".into())
                    .is_err(),
                "{session:?}/{delivery:?} must be rejected"
            );
        }
        // 非法输入不改写磁盘文件也不改内存；账本仍健康可重载。
        assert_eq!(fs::read(&path).unwrap(), on_disk);
        ledger.ensure_healthy().unwrap();
        assert!(ledger.record("s1", "d2").is_none());
        let reloaded = TerminalMessageLedger::load(Some(path));
        assert_eq!(reloaded.record("s1", "d1").unwrap().state, DeliveryState::Claimed);
    }

    #[test]
    fn unwritable_target_fails_claim_and_recovers_after_repair() {
        let directory = TestDirectory::new("unwritable");
        // 先让父路径是合法目录：账本文件缺失是健康的空账本。
        let blocker = directory.0.join("blocked");
        fs::create_dir(&blocker).unwrap();
        let path = blocker.join("terminal-message-deliveries.json");

        let mut ledger = TerminalMessageLedger::load(Some(path.clone()));
        ledger.ensure_healthy().unwrap();

        // 之后把父目录换成普通文件：临时文件 open 必然失败。
        fs::remove_dir(&blocker).unwrap();
        fs::write(&blocker, b"not a directory").unwrap();

        // 落盘失败的认领被拒绝——失败准入不得放行发送。
        assert!(
            ledger
                .try_claim("s1", "d1", "a".into(), now_secs())
                .is_err()
        );
        // 保守的 Claimed 记录留在内存；同一 key 再次认领仍然失败，
        // 绝不假装成功放行重发。
        assert!(
            ledger
                .try_claim("s1", "d1", "a".into(), now_secs())
                .is_err()
        );

        // 修复父路径后准入重试成功：d1 记录与新认领的 d2 一起落盘。
        fs::remove_file(&blocker).unwrap();
        fs::create_dir(&blocker).unwrap();
        assert!(
            ledger
                .try_claim("s1", "d2", "b".into(), now_secs())
                .unwrap()
        );
        assert!(
            !ledger
                .try_claim("s1", "d1", "a".into(), now_secs())
                .unwrap()
        );

        let reloaded = TerminalMessageLedger::load(Some(path));
        assert_eq!(reloaded.record("s1", "d1").unwrap().state, DeliveryState::Claimed);
        assert_eq!(reloaded.record("s1", "d2").unwrap().state, DeliveryState::Claimed);
    }

    #[test]
    fn capacity_full_rejects_claim_without_evicting_uncertain_records() {
        let mut ledger = TerminalMessageLedger::in_memory();
        // 填满账本的全部是过期但不确定的记录：年龄与容量压力都不能淘汰它们。
        let aged = now_secs() - MESSAGE_LEDGER_MAX_AGE_SECS - 10;
        for index in 0..MESSAGE_LEDGER_MAX_RECORDS {
            let id = format!("d{index}");
            ledger.records.insert(
                delivery_key("s1", &id),
                test_record("s1", &id, DeliveryState::Failed, aged),
            );
        }

        assert!(
            ledger
                .try_claim("s-new", "d-new", "a".into(), now_secs())
                .is_err()
        );
        // 一条不少：不确定记录绝不因容量压力被丢弃。
        assert_eq!(ledger.records.len(), MESSAGE_LEDGER_MAX_RECORDS);
    }

    #[test]
    fn prune_drops_aged_confirmed_and_keeps_uncertain_records() {
        let directory = TestDirectory::new("prune");
        let path = directory.ledger_path();
        let now = now_secs();
        let aged = now - MESSAGE_LEDGER_MAX_AGE_SECS - 10;
        let mut ledger = TerminalMessageLedger::load(Some(path.clone()));
        for (id, state) in [
            ("old-confirmed", DeliveryState::Confirmed),
            ("old-failed", DeliveryState::Failed),
            ("old-claimed", DeliveryState::Claimed),
        ] {
            ledger
                .records
                .insert(delivery_key("s1", id), test_record("s1", id, state, aged));
        }
        ledger.persist().unwrap();

        let reloaded = TerminalMessageLedger::load(Some(path));
        // 只有过期的 Confirmed 被淘汰；不确定的记录保留。
        assert!(reloaded.record("s1", "old-confirmed").is_none());
        assert!(reloaded.record("s1", "old-failed").is_some());
        assert!(reloaded.record("s1", "old-claimed").is_some());
    }
}
