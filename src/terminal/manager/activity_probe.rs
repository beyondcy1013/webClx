use std::collections::HashMap;
use std::thread;
use std::time::{Duration, Instant};

// The existing scan lock admits one cache refresh at a time. Bound the
// synchronous per-session tmux work within that refresh, retaining input order.
const TERMINAL_ACTIVITY_PROBE_WORKERS: usize = 4;

/// tmux pane 更新与 attach PTY reader 的观测不同步：输出可能延迟或被
/// 初始重绘抑制，因此 `current_output_seq` 未变仅是提示，不能作为 pane
/// 未变的证明，快照复用另有此固定年龄上限。
pub(in crate::terminal) const PROBE_SNAPSHOT_MAX_AGE: Duration = Duration::from_secs(1);

/// Per-session cache of the last activity-pane capture, keyed by the PTY
/// output sequence observed at capture time together with the attachment
/// instance that produced it. Reuse is bounded by `PROBE_SNAPSHOT_MAX_AGE`
/// because pane updates are not synchronized with reader observation.
///
/// Only the raw snapshot plus its fingerprint are stored; the working /
/// error / worked / model-family results are re-derived from it each cycle
/// by the pure match functions, keeping keyword, time-pattern and
/// manual-interrupt settings changes effective immediately and the match
/// logic itself cache-free. `error_line_limit` is recorded because it also
/// selects the capture depth (`line_limit.max(200)` lines), so a changed
/// limit must re-capture rather than reuse a shallower snapshot.
#[derive(Clone, Debug)]
pub(in crate::terminal) struct ProbeSnapshotCacheEntry {
    /// `TerminalSession::current_output_seq` read when the capture ran.
    pub(in crate::terminal) live_output_seq: u64,
    /// `TerminalSession::attached_at` of the attachment that produced the
    /// capture; a re-attached session can never alias an older entry.
    pub(in crate::terminal) attachment_started_at: Instant,
    /// When this entry was written; bounds snapshot reuse.
    pub(in crate::terminal) captured_at: Instant,
    /// `error_line_limit` the snapshot was captured with.
    pub(in crate::terminal) error_line_limit: u32,
    /// Raw capture-pane output (`None` when the capture itself failed).
    pub(in crate::terminal) snapshot: Option<Vec<u8>>,
    pub(in crate::terminal) snapshot_fingerprint: Option<u64>,
}

/// session_id → last validated capture. Bounded by the session count and
/// purged alongside the other per-session runtime state on session removal.
pub(in crate::terminal) type ProbeSnapshotCache = HashMap<String, ProbeSnapshotCacheEntry>;

pub(super) fn map_activity_probes<T: Sync, R: Send>(
    items: &[T],
    probe: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    if items.len() <= 1 {
        return items.iter().map(probe).collect();
    }
    let chunk_size = items.len().div_ceil(TERMINAL_ACTIVITY_PROBE_WORKERS);
    thread::scope(|scope| {
        let probe = &probe;
        let handles: Vec<_> = items
            .chunks(chunk_size)
            .map(|chunk| {
                thread::Builder::new()
                    .name("terminal-activity".into())
                    .spawn_scoped(scope, move || chunk.iter().map(probe).collect::<Vec<_>>())
                    // Admission failure must not omit sessions or fail the scan.
                    .map_err(|error| {
                        tracing::warn!(
                            "activity probe thread creation failed, probing chunk inline: {error}"
                        );
                        chunk.iter().map(probe).collect::<Vec<_>>()
                    })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| match handle {
                Ok(handle) => handle
                    .join()
                    .unwrap_or_else(|panic| std::panic::resume_unwind(panic)),
                Err(probes) => probes,
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Condvar, Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::Duration;

    #[test]
    fn activity_probe_workers_bound_concurrency_preserve_order_and_failures() {
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let calls = AtomicUsize::new(0);
        let barrier = (Mutex::new(0usize), Condvar::new());
        let timed_out = AtomicUsize::new(0);
        let inputs: Vec<_> = (0..8).collect();
        let results = map_activity_probes(&inputs, |value| {
            let running = active.fetch_add(1, Ordering::SeqCst) + 1;
            peak.fetch_max(running, Ordering::SeqCst);
            calls.fetch_add(1, Ordering::SeqCst);
            let (arrivals, notify) = &barrier;
            let mut arrivals = arrivals.lock().unwrap();
            *arrivals += 1;
            notify.notify_all();
            let (arrivals, wait) = notify
                .wait_timeout_while(arrivals, Duration::from_secs(2), |n| {
                    *n < TERMINAL_ACTIVITY_PROBE_WORKERS
                })
                .unwrap();
            if wait.timed_out() && *arrivals < TERMINAL_ACTIVITY_PROBE_WORKERS {
                timed_out.fetch_add(1, Ordering::SeqCst);
            }
            drop(arrivals);
            active.fetch_sub(1, Ordering::SeqCst);
            if value % 3 == 0 {
                Err(*value)
            } else {
                Ok(*value)
            }
        });
        assert_eq!(
            timed_out.load(Ordering::SeqCst),
            0,
            "workers must overlap before bounded barrier expires"
        );
        assert_eq!(peak.load(Ordering::SeqCst), TERMINAL_ACTIVITY_PROBE_WORKERS);
        assert_eq!(calls.load(Ordering::SeqCst), inputs.len());
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert_eq!(
            results,
            inputs
                .iter()
                .map(|v| if v % 3 == 0 { Err(*v) } else { Ok(*v) })
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn activity_probe_workers_empty_and_singleton_stay_inline() {
        let empty: Vec<u8> = map_activity_probes::<u8, u8>(&[], |_| panic!("empty probe"));
        assert!(empty.is_empty());
        let caller = thread::current().id();
        assert_eq!(
            map_activity_probes(&[7], |n| {
                assert_eq!(thread::current().id(), caller);
                *n
            }),
            vec![7]
        );
    }

    #[test]
    fn activity_probe_workers_propagate_panics() {
        let result = std::panic::catch_unwind(|| {
            map_activity_probes(&[0, 1], |n| {
                assert_ne!(*n, 0, "injected probe panic");
                *n
            })
        });
        assert!(result.is_err(), "a panicking scan must not cache partial success");
    }
}
