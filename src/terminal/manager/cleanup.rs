use super::*;

impl TerminalManager {
    /// Only in-memory snapshots/merges hold state. Backend IO and PTY reaping
    /// must never run under the manager-wide lock.
    pub(super) fn cleanup_sessions(&self, path: Option<&PathBuf>) -> bool {
        self.cleanup_sessions_with(path, |stored, live| {
            #[cfg(not(windows))]
            {
                let _ = live;
                tmux_session_status(&stored.id)
            }
            #[cfg(windows)]
            {
                let _ = stored;
                if live.is_some_and(|session| !session.is_alive()) {
                    TmuxSessionStatus::Missing
                } else {
                    TmuxSessionStatus::Unknown
                }
            }
        })
    }

    fn cleanup_sessions_with(
        &self,
        path: Option<&PathBuf>,
        probe: impl Fn(&StoredTerminalSession, Option<&Arc<TerminalSession>>) -> TmuxSessionStatus
        + Sync,
    ) -> bool {
        let candidates = {
            let state = crate::lock_or_recover!(self.state.read());
            state
                .sessions_by_id
                .values()
                .filter(|stored| path.is_none_or(|path| stored.path == *path))
                .cloned()
                .collect::<Vec<_>>()
        };
        // Reuse bounded worker admission; this is not a whole-scan deadline.
        map_activity_probes(&candidates, |candidate| {
            let start_lock = self.session_start_lock(&candidate.id);
            let _start_guard = match start_lock.try_lock() {
                Ok(guard) => guard,
                Err(std::sync::TryLockError::WouldBlock) => return false,
                Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
            };
            let live = {
                let state = crate::lock_or_recover!(self.state.read());
                if !same_session(&state, candidate) {
                    return false;
                }
                state.live_sessions.get(&candidate.id).cloned()
            };
            let status = probe(candidate, live.as_ref());
            let (dirty, retired) = {
                let mut state = crate::lock_or_recover!(self.state.write());
                // A deleted/replaced record or newly attached PTY invalidates
                // the observation. Never apply a stale Missing result.
                if !same_session(&state, candidate)
                    || !same_live_session(state.live_sessions.get(&candidate.id), live.as_ref())
                {
                    return false;
                }
                if status == TmuxSessionStatus::Missing {
                    state.sessions_by_id.remove(&candidate.id);
                    remove_session_from_path_locked(&mut state, &candidate.path, &candidate.id);
                    state.input_histories.remove(&candidate.id);
                    state.output_observations.remove(&candidate.id);
                    let retired = state.live_sessions.remove(&candidate.id);
                    (true, retired)
                } else {
                    // Unknown must retain the stored session, just as before.
                    let retired = if live.as_ref().is_some_and(|session| !session.is_alive()) {
                        state.live_sessions.remove(&candidate.id)
                    } else {
                        None
                    };
                    (false, retired)
                }
            };
            if let Some(session) = retired {
                session.terminate_child();
            }
            if dirty {
                // 会话已永久移除：释放其内存态运行时缓存。先放开本次探测
                // 持有的启动锁，再按引用计数决定是否移除锁条目。
                drop(_start_guard);
                self.purge_session_runtime_state(&candidate.id);
                self.purge_session_persisted_tasks(&candidate.id);
                self.release_session_start_lock_if_idle(&candidate.id, &start_lock);
            }
            dirty
        })
        .into_iter()
        .any(|dirty| dirty)
    }
}

fn same_session(state: &TerminalState, candidate: &StoredTerminalSession) -> bool {
    state
        .sessions_by_id
        .get(&candidate.id)
        .is_some_and(|current| {
            current.created_at == candidate.created_at && current.path == candidate.path
        })
}

fn same_live_session(
    current: Option<&Arc<TerminalSession>>,
    observed: Option<&Arc<TerminalSession>>,
) -> bool {
    match (current, observed) {
        (None, None) => true,
        (Some(current), Some(observed)) => Arc::ptr_eq(current, observed),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (TerminalManager, StoredTerminalSession, PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "webclx-cleanup-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let manager = TerminalManager::new(path.join("sessions.json"));
        let stored = reserve_session_locked(
            &mut manager.state.write().unwrap(),
            &manager.next_id,
            path.clone(),
            &manager.env_snapshot.user_profile,
            String::new(),
            String::new(),
            TerminalSessionOrigin::Normal,
            String::new(),
        )
        .unwrap();
        (manager, stored, path)
    }

    #[test]
    fn cleanup_probe_leaves_state_and_input_available() {
        let (manager, stored, path) = fixture();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let release_rx = Mutex::new(release_rx);
        let safety_timeout = AtomicBool::new(false);
        thread::scope(|scope| {
            struct Release(Option<std::sync::mpsc::Sender<()>>);
            impl Drop for Release {
                fn drop(&mut self) {
                    if let Some(sender) = self.0.take() {
                        let _ = sender.send(());
                    }
                }
            }
            let release = Release(Some(release_tx));
            let worker = scope.spawn(|| {
                manager.cleanup_sessions_with(None, |_, _| {
                    entered_tx.send(()).unwrap();
                    if release_rx
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .is_err()
                    {
                        safety_timeout.store(true, Ordering::SeqCst);
                    }
                    TmuxSessionStatus::Exists
                })
            });
            entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            let state_available = manager.state.try_write().is_ok();
            let input_completed = if state_available {
                manager.capture_session_input(&stored.id, "hello\r")
            } else {
                false
            };
            drop(release);
            assert!(!worker.join().unwrap());
            assert!(!safety_timeout.load(Ordering::SeqCst), "probe safety timeout fired");
            assert!(state_available, "probe held the global state lock");
            assert!(input_completed, "input capture stalled behind cleanup");
        });
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn cleanup_unknown_retains_missing_removes_and_startup_skips() {
        let (manager, stored, path) = fixture();
        assert!(!manager.cleanup_sessions_with(None, |_, _| TmuxSessionStatus::Unknown));
        assert!(manager.session_path(&stored.id).is_some());
        let start_lock = manager.session_start_lock(&stored.id);
        let guard = start_lock.lock().unwrap();
        assert!(!manager.cleanup_sessions_with(None, |_, _| panic!("startup probed")));
        drop(guard);
        assert!(manager.cleanup_sessions_with(None, |_, _| TmuxSessionStatus::Missing));
        assert!(manager.session_path(&stored.id).is_none());
        assert!(
            !manager
                .state
                .read()
                .unwrap()
                .sessions_by_path
                .contains_key(&path)
        );
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn cleanup_discards_stale_missing_and_preserves_new_records() {
        let (manager, stored, path) = fixture();
        assert!(!manager.cleanup_sessions_with(None, |candidate, _| {
            let mut state = manager.state.write().unwrap();
            state
                .sessions_by_id
                .get_mut(&candidate.id)
                .unwrap()
                .created_at += 1;
            reserve_session_locked(
                &mut state,
                &manager.next_id,
                path.clone(),
                &manager.env_snapshot.user_profile,
                String::new(),
                String::new(),
                TerminalSessionOrigin::Normal,
                String::new(),
            )
            .unwrap();
            TmuxSessionStatus::Missing
        }));
        assert!(manager.session_path(&stored.id).is_some());
        assert_eq!(manager.state.read().unwrap().sessions_by_id.len(), 2);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn purge_session_runtime_state_clears_only_target_session() {
        let (manager, stored, path) = fixture();
        let session_a = stored.id.clone();
        let session_b = "other-session".to_string();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        for session_id in [&session_a, &session_b] {
            {
                let mut runtime = manager.per_session_runtime.lock().unwrap();
                let entry = runtime.entry(session_id.clone()).or_default();
                entry.error_auto_continue = Some(TerminalErrorAutoContinueRecord {
                    key: "error-key".into(),
                    sent_at_millis: 1,
                    reset_at: String::new(),
                    consecutive_attempts: 1,
                });
                entry.auto_compact_signature = Some("sig".into());
                entry.auto_continue_last_sent_at = Some(1);
                entry.canceled_signatures.insert("sig".into());
            }
            manager
                .terminal_message_ledger
                .lock()
                .unwrap()
                .try_claim(session_id, "d1", "digest".into(), now)
                .unwrap();
        }
        {
            let mut cache = manager.activity_probe_cache.lock().unwrap();
            cache.key = Some(TerminalActivityProbeCacheKey {
                session_ids: vec![session_a.clone(), session_b.clone()],
                error_line_limit: 0,
                error_keywords: Vec::new(),
                auto_continue_time_patterns: Vec::new(),
                auto_continue_respect_manual_interrupt: false,
            });
            cache.completed_at = Some(Instant::now());
        }

        manager.purge_session_runtime_state(&session_a);

        // 条目中两把锁都无外部持有：整个 per-session 条目被移除。
        assert!(
            manager
                .per_session_runtime
                .lock()
                .unwrap()
                .get(&session_a)
                .is_none()
        );
        assert!(
            manager
                .terminal_message_ledger
                .lock()
                .unwrap()
                .record(&session_a, "d1")
                .is_none()
        );
        // 探针缓存的键含已删会话：整条快照作废，避免下发陈旧探测结果。
        assert!(manager.activity_probe_cache.lock().unwrap().key.is_none());

        // 其它会话的运行时状态不受影响（per-session 隔离）。
        {
            let runtime = manager.per_session_runtime.lock().unwrap();
            let entry_b = runtime.get(&session_b).expect("session B entry kept");
            assert!(entry_b.error_auto_continue.is_some());
            assert_eq!(entry_b.auto_compact_signature.as_deref(), Some("sig"));
            assert_eq!(entry_b.auto_continue_last_sent_at, Some(1));
            assert!(entry_b.canceled_signatures.contains("sig"));
        }
        assert!(
            manager
                .terminal_message_ledger
                .lock()
                .unwrap()
                .record(&session_b, "d1")
                .is_some()
        );

        // 再次清理是幂等的。
        manager.purge_session_runtime_state(&session_a);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn purge_session_persisted_tasks_clears_only_target_session() {
        let (manager, stored, path) = fixture();
        let session_a = stored.id.clone();
        let session_b = "other-session".to_string();
        let now_millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        for session_id in [&session_a, &session_b] {
            manager.auto_continue_schedules.lock().unwrap().insert(
                session_id.clone(),
                TerminalAutoContinueTask {
                    session_id: session_id.clone(),
                    terminal_name: String::new(),
                    schedule: TerminalAutoContinueSchedule {
                        signature: format!("sig-{session_id}"),
                        keyword: "continue".into(),
                        reset_at: String::new(),
                        due_at_millis: now_millis + 60_000,
                    },
                    backend_due_at_millis: now_millis + 60_000,
                    created_at_millis: now_millis,
                    error_line_limit: 0,
                    auto_continue_interval_seconds: 0,
                    auto_continue_respect_manual_interrupt: false,
                    error_keywords: Vec::new(),
                    auto_continue_time_patterns: Vec::new(),
                },
            );
            manager.scheduled_input_tasks.lock().unwrap().insert(
                format!("task-{session_id}"),
                TerminalScheduledInputTask {
                    status: String::new(),
                    last_error: String::new(),
                    id: format!("task-{session_id}"),
                    session_id: session_id.clone(),
                    terminal_name: String::new(),
                    due_at_millis: now_millis + 60_000,
                    created_at_millis: now_millis,
                    label: String::new(),
                    text: "echo hi".into(),
                    send_enter: true,
                    task_type: String::new(),
                    working_dir: String::new(),
                    repeat_interval_millis: 0,
                    parent_task_id: String::new(),
                },
            );
            manager.pending_build_requests.lock().unwrap().insert(
                format!("req-{session_id}"),
                TerminalPendingBuildRequest {
                    request_id: format!("req-{session_id}"),
                    session_id: session_id.clone(),
                    queued_at_millis: now_millis,
                },
            );
        }

        // 注入 marker 记录器代替真实 crontab 重写：既避免测试触碰系统
        // crontab，又能核对将清除的 marker 覆盖该会话的条目行与 due 注释。
        let recorded = Arc::new(Mutex::new(Vec::<Vec<String>>::new()));
        let recorder = recorded.clone();
        manager.purge_session_persisted_tasks_with(&session_a, move |markers| {
            recorder.lock().unwrap().push(markers.to_vec());
            Ok(())
        });

        // A 的三类持久化任务全部移除。
        assert!(
            !manager
                .auto_continue_schedules
                .lock()
                .unwrap()
                .contains_key(&session_a)
        );
        assert!(
            !manager
                .scheduled_input_tasks
                .lock()
                .unwrap()
                .values()
                .any(|task| task.session_id == session_a)
        );
        assert!(
            !manager
                .pending_build_requests
                .lock()
                .unwrap()
                .values()
                .any(|request| request.session_id == session_a)
        );
        assert_eq!(
            recorded.lock().unwrap().as_slice(),
            &[vec![
                format!("webclx-auto-continue:{session_a}:"),
                format!("# webclx-auto-continue-due:{session_a}:"),
            ]]
        );

        // B 的三类持久化任务不受影响。
        assert!(
            manager
                .auto_continue_schedules
                .lock()
                .unwrap()
                .contains_key(&session_b)
        );
        assert!(
            manager
                .scheduled_input_tasks
                .lock()
                .unwrap()
                .contains_key(&format!("task-{session_b}"))
        );
        assert!(
            manager
                .pending_build_requests
                .lock()
                .unwrap()
                .contains_key(&format!("req-{session_b}"))
        );

        // 从磁盘重新加载注册表，确认持久化内容同样只剩 B。
        let auto_continue = load_terminal_auto_continue_tasks(manager.auto_continue_file.as_path());
        assert!(!auto_continue.contains_key(&session_a));
        assert!(auto_continue.contains_key(&session_b));
        let scheduled = load_terminal_scheduled_input_tasks(manager.scheduled_input_file.as_path());
        assert_eq!(scheduled.len(), 1);
        assert!(scheduled.contains_key(&format!("task-{session_b}")));
        let pending = load_terminal_pending_build_requests(manager.pending_build_file.as_path());
        assert_eq!(pending.len(), 1);
        assert!(pending.contains_key(&format!("req-{session_b}")));

        // 空 session_id 与重复清理均为无操作，不再触发 crontab 重写。
        recorded.lock().unwrap().clear();
        manager.purge_session_persisted_tasks_with("", |markers| {
            recorded.lock().unwrap().push(markers.to_vec());
            Ok(())
        });
        manager.purge_session_persisted_tasks_with(&session_a, |markers| {
            recorded.lock().unwrap().push(markers.to_vec());
            Ok(())
        });
        assert!(recorded.lock().unwrap().is_empty());
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn cleanup_missing_purges_runtime_state() {
        let (manager, stored, path) = fixture();
        manager
            .per_session_runtime
            .lock()
            .unwrap()
            .entry(stored.id.clone())
            .or_default()
            .error_auto_continue = Some(TerminalErrorAutoContinueRecord {
            key: "error-key".into(),
            sent_at_millis: 1,
            reset_at: String::new(),
            consecutive_attempts: 0,
        });
        manager.session_start_lock(&stored.id);
        assert!(
            manager
                .per_session_runtime
                .lock()
                .unwrap()
                .contains_key(&stored.id)
        );

        assert!(manager.cleanup_sessions_with(None, |_, _| TmuxSessionStatus::Missing));

        // purge 清空字段后条目因锁仅被 map 持有而整条移除；
        // 调用方持有的克隆由 release_session_start_lock_if_idle 收尾。
        assert!(
            manager
                .per_session_runtime
                .lock()
                .unwrap()
                .get(&stored.id)
                .is_none()
        );
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn release_session_start_lock_if_idle_keeps_shared_or_foreign_locks() {
        let (manager, stored, path) = fixture();
        let lock = manager.session_start_lock(&stored.id);

        // 条目 + 调用方各持一份（strong_count == 2）：移除条目。
        manager.release_session_start_lock_if_idle(&stored.id, &lock);
        assert!(
            manager
                .per_session_runtime
                .lock()
                .unwrap()
                .get(&stored.id)
                .is_none()
        );

        // 还有第三方克隆持有时必须保留条目。
        let lock = manager.session_start_lock(&stored.id);
        let outstanding = lock.clone();
        manager.release_session_start_lock_if_idle(&stored.id, &lock);
        assert!(
            manager
                .per_session_runtime
                .lock()
                .unwrap()
                .get(&stored.id)
                .is_some()
        );
        drop(outstanding);

        // 传入的锁不是条目中那把（例如条目已被替换）时不得误删现有条目。
        let current = Arc::new(Mutex::new(()));
        manager
            .per_session_runtime
            .lock()
            .unwrap()
            .entry(stored.id.clone())
            .or_default()
            .start_lock = current.clone();
        manager.release_session_start_lock_if_idle(&stored.id, &lock);
        assert!(
            manager
                .per_session_runtime
                .lock()
                .unwrap()
                .get(&stored.id)
                .is_some_and(|entry| Arc::ptr_eq(&entry.start_lock, &current))
        );
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn canceled_auto_continue_signatures_are_scoped_per_session() {
        let (manager, stored, path) = fixture();
        let session_a = stored.id.clone();
        let session_b = "other-session".to_string();

        // 同一裸签名落在两个会话上不得互相碰撞。
        manager.remember_canceled_auto_continue_signature(&session_a, "sig");
        manager.remember_canceled_auto_continue_signature(&session_b, "sig");
        {
            let runtime = manager.per_session_runtime.lock().unwrap();
            assert!(runtime[&session_a].canceled_signatures.contains("sig"));
            assert!(runtime[&session_b].canceled_signatures.contains("sig"));
        }

        manager.forget_canceled_auto_continue_signature(&session_a, "sig");
        {
            let runtime = manager.per_session_runtime.lock().unwrap();
            assert!(!runtime.contains_key(&session_a));
            assert!(runtime[&session_b].canceled_signatures.contains("sig"));
        }
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn runtime_prune_keeps_entry_while_locks_are_shared() {
        let (manager, stored, path) = fixture();
        let lock = manager.session_start_lock(&stored.id);
        let outstanding = lock.clone();

        // 条目业务字段为空但启动锁仍有外部克隆：修剪必须保留条目，
        // 避免持有旧锁的等待者被换到另一把锁上。
        manager.prune_error_auto_continue_records(&[]);
        manager.prune_auto_continue_last_sent_at(&[]);
        assert!(
            manager
                .per_session_runtime
                .lock()
                .unwrap()
                .contains_key(&stored.id)
        );

        // 只剩 map 持有时条目才可移除。
        drop(outstanding);
        drop(lock);
        manager.prune_error_auto_continue_records(&[]);
        assert!(
            !manager
                .per_session_runtime
                .lock()
                .unwrap()
                .contains_key(&stored.id)
        );
        let _ = fs::remove_dir_all(path);
    }

    /// 一轮"启动门被占用"的 helper 调用：线程 1 持门等待放行信号
    /// （2 秒安全超时 + RAII 放行兜底），线程 2 持 state 写锁调用生产
    /// helper——预期走 WouldBlock 分支：先放 state、再阻塞等门、复核
    /// 预约。invalidate=true 时主线程在放行前改动预约（created_at+1），
    /// helper 必须报"已取消或替换"。返回 helper 是否成功取得启动门。
    fn contended_reserved_start_round(
        manager: &TerminalManager,
        stored: &StoredTerminalSession,
        invalidate: bool,
    ) -> bool {
        struct Release(Option<std::sync::mpsc::Sender<()>>);
        impl Drop for Release {
            fn drop(&mut self) {
                if let Some(sender) = self.0.take() {
                    let _ = sender.send(());
                }
            }
        }

        let start_lock = manager.session_start_lock(&stored.id);
        let holder_lock = start_lock.clone();
        let (gate_held_tx, gate_held_rx) = std::sync::mpsc::channel::<()>();
        let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
        let release_rx = Mutex::new(release_rx);
        let (caller_waiting_tx, caller_waiting_rx) = std::sync::mpsc::channel::<()>();
        let safety_timeout = AtomicBool::new(false);

        let acquired = thread::scope(|scope| {
            let release = Release(Some(release_tx));
            let holder = scope.spawn(|| {
                let _guard = crate::lock_or_recover!(holder_lock.lock());
                gate_held_tx.send(()).expect("gate-held signal");
                if release_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(2))
                    .is_err()
                {
                    safety_timeout.store(true, Ordering::SeqCst);
                }
            });
            gate_held_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("gate holder must report in");
            let caller = scope.spawn(|| {
                let state = crate::lock_or_recover!(manager.state.write());
                caller_waiting_tx.send(()).expect("caller signal");
                manager
                    .lock_reserved_session_start(&start_lock, state, stored)
                    .is_ok()
            });
            caller_waiting_rx
                .recv_timeout(Duration::from_secs(2))
                .expect("contended caller must acquire state");

            // 因果检查：启动门仍被线程 1 占有时全局 state 必须可写——
            // helper 在阻塞等门前释放了写锁。
            let deadline = Instant::now() + Duration::from_secs(2);
            while manager.state.try_write().is_err() {
                assert!(
                    Instant::now() < deadline,
                    "global state stayed write-locked behind the contended start gate"
                );
                std::thread::yield_now();
            }

            if invalidate {
                crate::lock_or_recover!(manager.state.write())
                    .sessions_by_id
                    .get_mut(&stored.id)
                    .unwrap()
                    .created_at += 1;
            }
            drop(release);
            let outcome = caller.join().expect("caller thread finishes");
            holder.join().expect("gate holder finishes");
            outcome
        });
        assert!(
            !safety_timeout.load(Ordering::SeqCst),
            "start-gate holder hit the safety timeout"
        );
        acquired
    }

    #[test]
    fn reserved_session_start_waits_without_holding_global_state() {
        let (manager, stored, path) = fixture();

        // 预约被改动：等门后的复核必须让认领失败。
        assert!(!contended_reserved_start_round(&manager, &stored, true));
        // 复核失败不改写登记：round 2 用当前登记记录重跑，必须成功。
        let stored = crate::lock_or_recover!(manager.state.read())
            .sessions_by_id
            .get(&stored.id)
            .unwrap()
            .clone();
        assert!(contended_reserved_start_round(&manager, &stored, false));

        let _ = fs::remove_dir_all(path);
    }
}
