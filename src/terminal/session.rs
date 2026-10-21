use std::{
    collections::{HashMap, VecDeque},
    io::{ErrorKind, Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
#[cfg(not(windows))]
use terminal_core::tmux_session_name;
use terminal_core::{SessionNameState, TitleTracker};
use tokio::sync::broadcast;
use tracing::debug;
#[cfg(not(windows))]
use tracing::warn;

#[cfg(not(windows))]
use super::tmux::{capture_tmux_initial_pane_snapshot_with_limit, tmux_socket_name};
use super::{
    CHILD_PROCESS_ENV_KEYS_TO_CLEAR, DEFAULT_COLS, DEFAULT_ROWS, INITIAL_TMUX_REDRAW_SUPPRESS_MS,
    MAX_BACKLOG_BYTES, MAX_INITIAL_REPLAY_BYTES, StoredTerminalSession,
    TERMINAL_INITIAL_SNAPSHOT_LINE_LIMIT,
};

const TERMINAL_OUTPUT_CHANNEL_CAPACITY: usize = 4096;

// libc is a Unix-only dependency, so errno comparisons stay behind cfg(unix).
// Non-Unix targets report native error codes that never equal these errno values.
#[cfg(unix)]
const ERRNO_ECHILD: Option<i32> = Some(libc::ECHILD);
#[cfg(not(unix))]
const ERRNO_ECHILD: Option<i32> = None;
#[cfg(unix)]
const ERRNO_ESRCH: Option<i32> = Some(libc::ESRCH);
#[cfg(not(unix))]
const ERRNO_ESRCH: Option<i32> = None;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TerminalViewportSize {
    pub(super) cols: u16,
    pub(super) rows: u16,
}

#[derive(Clone, Copy, Debug)]
struct TerminalViewportClient {
    visible: bool,
    size: Option<TerminalViewportSize>,
}

#[derive(Debug, Default)]
pub(super) struct TerminalViewportRegistry {
    next_id: u64,
    clients: HashMap<u64, TerminalViewportClient>,
    applied_size: Option<TerminalViewportSize>,
}

impl TerminalViewportRegistry {
    fn with_applied_size(size: TerminalViewportSize) -> Self {
        Self {
            applied_size: Some(size),
            ..Self::default()
        }
    }

    pub(super) fn register(&mut self, visible: bool) -> u64 {
        self.next_id = self.next_id.saturating_add(1);
        let id = self.next_id;
        self.clients.insert(
            id,
            TerminalViewportClient {
                visible,
                size: None,
            },
        );
        id
    }

    pub(super) fn unregister(&mut self, id: u64) {
        self.clients.remove(&id);
    }

    pub(super) fn update_size(&mut self, id: u64, cols: u16, rows: u16) {
        if let Some(client) = self.clients.get_mut(&id) {
            client.size = Some(TerminalViewportSize {
                cols: cols.max(2),
                rows: rows.max(2),
            });
        }
    }

    pub(super) fn set_visibility(&mut self, id: u64, visible: bool) {
        if let Some(client) = self.clients.get_mut(&id) {
            client.visible = visible;
        }
    }

    pub(super) fn effective_size(&self) -> Option<TerminalViewportSize> {
        self.clients
            .values()
            .filter(|client| client.visible)
            .filter_map(|client| client.size)
            .max_by_key(|size| (size.cols, size.rows))
    }
}

#[derive(Clone, Debug)]
pub(super) struct TerminalOutputChunk {
    pub(super) seq: u64,
    pub(super) bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub(super) enum TerminalStreamEvent {
    Output(TerminalOutputChunk),
    ViewportSize(TerminalViewportSize),
}

#[derive(Debug, Default)]
pub(super) struct TerminalOutputBacklog {
    chunks: VecDeque<TerminalOutputChunk>,
    total_bytes: usize,
}

impl TerminalOutputBacklog {
    pub(super) fn new() -> Self {
        Self {
            chunks: VecDeque::new(),
            total_bytes: 0,
        }
    }

    pub(super) fn with_capacity(_capacity: usize) -> Self {
        Self::new()
    }

    fn push(&mut self, chunk: TerminalOutputChunk, max_bytes: usize) {
        self.total_bytes = self.total_bytes.saturating_add(chunk.bytes.len());
        self.chunks.push_back(chunk);

        while self.total_bytes > max_bytes {
            let Some(front) = self.chunks.pop_front() else {
                self.total_bytes = 0;
                break;
            };
            self.total_bytes = self.total_bytes.saturating_sub(front.bytes.len());
        }
    }

    fn snapshot(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(self.total_bytes);
        for chunk in &self.chunks {
            output.extend_from_slice(&chunk.bytes);
        }
        output
    }

    fn tail_snapshot(&self, max_bytes: usize) -> Vec<u8> {
        let mut remaining = max_bytes;
        let mut parts: Vec<&[u8]> = Vec::new();
        for chunk in self.chunks.iter().rev() {
            if remaining == 0 {
                break;
            }
            if chunk.bytes.len() <= remaining {
                parts.push(&chunk.bytes);
                remaining -= chunk.bytes.len();
            } else {
                let start = chunk.bytes.len() - remaining;
                parts.push(&chunk.bytes[start..]);
                remaining = 0;
            }
        }

        let total: usize = parts.iter().map(|part| part.len()).sum();
        let mut output = Vec::with_capacity(total);
        for part in parts.into_iter().rev() {
            output.extend_from_slice(part);
        }
        output
    }

    fn chunks_after(&self, seq: u64) -> Vec<TerminalOutputChunk> {
        self.chunks
            .iter()
            .filter(|chunk| chunk.seq > seq)
            .cloned()
            .collect()
    }

    #[cfg(test)]
    pub(super) fn pop_front_for_test(&mut self) {
        if let Some(front) = self.chunks.pop_front() {
            self.total_bytes = self.total_bytes.saturating_sub(front.bytes.len());
        }
    }

    fn tail_snapshot_with_last_seq(&self, max_bytes: usize) -> (Vec<u8>, u64) {
        let last_seq = self.chunks.back().map(|chunk| chunk.seq).unwrap_or(0);
        (self.tail_snapshot(max_bytes), last_seq)
    }
}

pub(super) struct TerminalSession {
    pub(super) id: String,
    pub(super) path: PathBuf,
    pub(super) name_state: RwLock<SessionNameState>,
    pub(super) title_tracker: Mutex<TitleTracker>,
    pub(super) master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    pub(super) viewports: Arc<Mutex<TerminalViewportRegistry>>,
    pub(super) writer: Arc<Mutex<Box<dyn Write + Send>>>,
    pub(super) _child: Arc<Mutex<Box<dyn portable_pty::Child + Send>>>,
    /// Set once the PTY child has been reaped. Keeping this state on the
    /// session lets the manager poll existing sessions without repeatedly
    /// calling `try_wait` after the underlying pid is gone.
    pub(super) child_reaped: AtomicBool,
    pub(super) broadcaster: broadcast::Sender<TerminalStreamEvent>,
    pub(super) backlog: Arc<Mutex<TerminalOutputBacklog>>,
    pub(super) next_output_seq: AtomicU64,
    /// 输出流实例标识：TerminalSession 每次创建都生成新随机值，与 seq 组合
    /// 后让重连客户端能识别"seq 已重置"的会话重建。
    pub(super) output_instance_id: u64,
    pub(super) attached_at: Instant,
    pub(super) suppressed_initial_redraw: AtomicBool,
    pub(super) last_output_at: AtomicU64,
    pub(super) alive: Arc<AtomicBool>,
    pub(super) expected_shutdown: AtomicBool,
}

impl TerminalSession {
    pub(super) fn attach(
        stored: &StoredTerminalSession,
        proxy_env: Vec<(String, String)>,
        startup_script: Option<String>,
    ) -> Result<Arc<Self>> {
        let initial_snapshot = initial_backend_snapshot(&stored.id);
        Self::attach_with_initial_snapshot(stored, proxy_env, startup_script, initial_snapshot)
    }

    pub(super) fn capture_initial_snapshot(session_id: &str) -> Option<Vec<u8>> {
        initial_backend_snapshot(session_id)
    }

    pub(super) fn attach_with_initial_snapshot(
        stored: &StoredTerminalSession,
        proxy_env: Vec<(String, String)>,
        startup_script: Option<String>,
        initial_snapshot: Option<Vec<u8>>,
    ) -> Result<Arc<Self>> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: DEFAULT_ROWS,
                cols: DEFAULT_COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("无法创建 PTY")?;

        let attach_cwd = if stored.path.is_dir() {
            stored.path.clone()
        } else {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
        };
        let mut command = attach_command(stored, &attach_cwd);
        command.cwd(attach_cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        for (key, value) in &proxy_env {
            command.env(key, value);
        }
        sanitize_pty_command(&mut command);

        let child = pair
            .slave
            .spawn_command(command)
            .context("无法连接 tmux 终端会话")?;
        drop(pair.slave);

        let reader = pair
            .master
            .try_clone_reader()
            .context("无法克隆终端 reader")?;
        let mut writer = pair.master.take_writer().context("无法打开终端 writer")?;
        if cfg!(windows)
            && let Some(script) = startup_script
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
        {
            writer.write_all(script.as_bytes())?;
            writer.flush()?;
        }

        let should_suppress_initial_redraw = initial_snapshot.is_some();
        let (broadcaster, _) = broadcast::channel(TERMINAL_OUTPUT_CHANNEL_CAPACITY);
        let session = Arc::new(Self {
            id: stored.id.clone(),
            path: stored.path.clone(),
            name_state: RwLock::new(SessionNameState::from_stored(
                stored.name.clone(),
                stored.title.clone(),
                stored.manually_renamed,
            )),
            title_tracker: Mutex::new(TitleTracker::default()),
            master: Arc::new(Mutex::new(pair.master)),
            viewports: Arc::new(Mutex::new(TerminalViewportRegistry::with_applied_size(
                TerminalViewportSize {
                    cols: DEFAULT_COLS,
                    rows: DEFAULT_ROWS,
                },
            ))),
            writer: Arc::new(Mutex::new(writer)),
            _child: Arc::new(Mutex::new(child)),
            child_reaped: AtomicBool::new(false),
            broadcaster,
            backlog: Arc::new(Mutex::new(TerminalOutputBacklog::with_capacity(MAX_BACKLOG_BYTES))),
            next_output_seq: AtomicU64::new(0),
            // 限制在 2^53 内，保证 JSON/JS Number 往返不失精度。
            output_instance_id: rand::random::<u64>() & ((1_u64 << 53) - 1),
            attached_at: Instant::now(),
            suppressed_initial_redraw: AtomicBool::new(!should_suppress_initial_redraw),
            last_output_at: AtomicU64::new(0),
            alive: Arc::new(AtomicBool::new(true)),
            expected_shutdown: AtomicBool::new(false),
        });

        // 把 attach 时的 tmux 深度快照作为 seq=0 前缀种子写入 backlog：快照
        // 覆盖 pane 在此之前的全部内容，后续 live chunk 全部晚于快照，整个
        // backlog 因此是一条连续字节流。之后任意 WebSocket 接入都在同一锁下
        // 原子读取"回放字节 + 水位线"，不再出现"即时抓快照 + 跳到当前 seq"
        // 造成的快照窗口内输出丢失（页面刷新时中间消息被吞）。必须在
        // reader 线程启动前写入，保证种子之前没有任何 live chunk。
        if let Some(snapshot_bytes) = initial_snapshot.as_deref() {
            session.seed_backlog_prefix(snapshot_bytes);
        }
        Self::spawn_reader_thread(session.clone(), reader);
        Ok(session)
    }

    fn spawn_reader_thread(session: Arc<Self>, mut reader: Box<dyn Read + Send>) {
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        if session.expected_shutdown.load(Ordering::SeqCst) {
                            session.mark_closed(b"");
                        } else {
                            session.mark_closed(b"\r\n[webclx] terminal client exited.\r\n");
                        }
                        break;
                    }
                    Ok(count) => {
                        if session.expected_shutdown.load(Ordering::SeqCst) {
                            continue;
                        }
                        if session.should_suppress_initial_tmux_redraw() {
                            continue;
                        }
                        session.note_output_now();
                        let chunk = session.record_output_chunk(&buffer[..count]);
                        session.track_title(&chunk.bytes);
                        let _ = session.broadcaster.send(TerminalStreamEvent::Output(chunk));
                    }
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(error) => {
                        let message = format!("\r\n[webclx] terminal read error: {error}\r\n");
                        session.mark_closed(message.as_bytes());
                        break;
                    }
                }
            }
        });
    }

    pub(super) fn is_alive(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Reap an exited PTY client without taking ownership of the child out of
    /// the session. This is intentionally driven by the manager's low-rate
    /// maintenance loop so restored sessions created before the reaper was
    /// added are covered as well.
    pub(super) fn reap_child_if_exited(&self) -> bool {
        if self.child_reaped.load(Ordering::SeqCst) {
            return false;
        }

        let result = crate::lock_or_recover!(self._child.lock()).try_wait();
        match result {
            Ok(Some(_status)) => {
                self.child_reaped.store(true, Ordering::SeqCst);
                self.mark_closed(b"");
                true
            }
            Ok(None) => false,
            Err(error) => {
                // ECHILD means another wait path already reaped this child.
                // Treat it as complete so the maintenance loop does not log
                // the same condition forever.
                if error.raw_os_error() == ERRNO_ECHILD {
                    self.child_reaped.store(true, Ordering::SeqCst);
                    self.mark_closed(b"");
                } else {
                    debug!("terminal child reap failed for {}: {error}", self.id);
                }
                false
            }
        }
    }

    /// Stop and reap the PTY client before its last `Arc` is dropped. This is
    /// used for explicit session deletion and failed/cancelled session setup;
    /// normal service restart deliberately leaves tmux clients attached.
    pub(super) fn terminate_child(&self) {
        if self.child_reaped.swap(true, Ordering::SeqCst) {
            return;
        }

        let mut child = crate::lock_or_recover!(self._child.lock());
        if let Err(error) = child.kill()
            && error.raw_os_error() != ERRNO_ESRCH
            && error.raw_os_error() != ERRNO_ECHILD
        {
            debug!("terminal child kill failed for {}: {error}", self.id);
        }
        let _ = child.wait();
        self.mark_closed(b"");
    }

    pub(super) fn prepare_for_shutdown(&self) {
        self.expected_shutdown.store(true, Ordering::SeqCst);
    }

    pub(super) fn last_output_at(&self) -> u64 {
        self.last_output_at.load(Ordering::SeqCst)
    }

    fn note_output_now(&self) {
        self.last_output_at
            .store(terminal_core::current_timestamp_millis(), Ordering::SeqCst);
    }

    pub(super) fn rename(&self, next_name: String) {
        crate::lock_or_recover!(self.name_state.write()).rename_manual(next_name);
    }

    pub(super) fn rename_auto(&self, next_name: String) {
        crate::lock_or_recover!(self.name_state.write()).rename_auto(next_name);
    }

    pub(super) fn name(&self) -> String {
        crate::lock_or_recover!(self.name_state.read())
            .display_name()
            .to_string()
    }

    pub(super) fn title(&self) -> Option<String> {
        crate::lock_or_recover!(self.name_state.read()).title()
    }

    pub(super) fn subscribe(&self) -> broadcast::Receiver<TerminalStreamEvent> {
        self.broadcaster.subscribe()
    }

    pub(super) fn current_viewport_size(&self) -> Option<TerminalViewportSize> {
        crate::lock_or_recover!(self.viewports.lock()).applied_size
    }

    /// 返回当前已分配的最新 output seq。
    ///
    /// 每个 chunk 的 seq 为 `fetch_add(1) + 1`，因此 `next_output_seq` 当前值
    /// 就是迄今分配过的最大 seq。新连接订阅 broadcast 后用它初始化
    /// `last_output_seq_sent`，可跳过 subscribe 时仍残留在 broadcast buffer
    /// 里的历史 chunk，避免把它们当作 live 输出重放一遍（切换终端"滚一遍"）。
    pub(super) fn current_output_seq(&self) -> u64 {
        self.next_output_seq.load(Ordering::SeqCst)
    }

    pub(super) fn output_instance_id(&self) -> u64 {
        self.output_instance_id
    }

    pub(super) fn backlog_snapshot(&self) -> Vec<u8> {
        crate::lock_or_recover!(self.backlog.lock()).snapshot()
    }

    pub(super) fn backlog_tail_snapshot_with_last_seq(&self, max_bytes: usize) -> (Vec<u8>, u64) {
        crate::lock_or_recover!(self.backlog.lock()).tail_snapshot_with_last_seq(max_bytes)
    }

    pub(super) fn backlog_tail_snapshot(&self, max_bytes: usize) -> Vec<u8> {
        crate::lock_or_recover!(self.backlog.lock()).tail_snapshot(max_bytes)
    }

    pub(super) fn backlog_chunks_after(&self, seq: u64) -> Vec<TerminalOutputChunk> {
        crate::lock_or_recover!(self.backlog.lock()).chunks_after(seq)
    }

    /// 把 attach 时的 tmux 深度快照作为 seq=0 前缀种子写入 backlog。
    ///
    /// 快照覆盖写入时刻 pane 的全部内容（含历史），而 live chunk 从 reader
    /// 线程启动后才开始记录，因此整个 backlog 是一条连续流；首屏回放据此
    /// 在同一锁下原子取得"回放字节 + 水位线"，接入不再有快照窗口竞态。
    pub(super) fn seed_backlog_prefix(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let bytes = keep_tail_bytes(bytes, MAX_INITIAL_REPLAY_BYTES);
        crate::lock_or_recover!(self.backlog.lock()).push(
            TerminalOutputChunk {
                seq: 0,
                bytes: bytes.to_vec(),
            },
            MAX_BACKLOG_BYTES,
        );
    }

    pub(super) fn should_suppress_initial_tmux_redraw(&self) -> bool {
        if self.suppressed_initial_redraw.load(Ordering::SeqCst) {
            return false;
        }

        if self.attached_at.elapsed() < Duration::from_millis(INITIAL_TMUX_REDRAW_SUPPRESS_MS) {
            return true;
        }

        self.suppressed_initial_redraw.store(true, Ordering::SeqCst);
        false
    }

    pub(super) async fn write_input(&self, data: String) -> Result<()> {
        let writer = self.writer.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut writer = crate::lock_or_recover!(writer.lock());
            super::input_pacing::write_input_paced(writer.as_mut(), &data)?;
            Ok(())
        })
        .await
        .context("终端写入任务失败")??;

        Ok(())
    }

    pub(super) fn register_viewport(&self, visible: bool) -> u64 {
        crate::lock_or_recover!(self.viewports.lock()).register(visible)
    }

    pub(super) async fn unregister_viewport(&self, id: u64) -> Result<()> {
        crate::lock_or_recover!(self.viewports.lock()).unregister(id);
        self.apply_effective_viewport_size().await
    }

    pub(super) async fn resize_viewport(&self, id: u64, cols: u16, rows: u16) -> Result<()> {
        crate::lock_or_recover!(self.viewports.lock()).update_size(id, cols, rows);
        self.apply_effective_viewport_size().await
    }

    pub(super) async fn set_viewport_visibility(&self, id: u64, visible: bool) -> Result<()> {
        crate::lock_or_recover!(self.viewports.lock()).set_visibility(id, visible);
        self.apply_effective_viewport_size().await
    }

    async fn apply_effective_viewport_size(&self) -> Result<()> {
        let master = self.master.clone();
        let viewports = self.viewports.clone();
        let broadcaster = self.broadcaster.clone();
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut viewports = crate::lock_or_recover!(viewports.lock());
            let Some(size) = viewports.effective_size() else {
                return Ok(());
            };
            if viewports.applied_size == Some(size) {
                return Ok(());
            }
            let previous_size = viewports.applied_size;
            viewports.applied_size = Some(size);
            let _ = broadcaster.send(TerminalStreamEvent::ViewportSize(size));
            crate::lock_or_recover!(master.lock())
                .resize(PtySize {
                    rows: size.rows,
                    cols: size.cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .inspect_err(|_| {
                    viewports.applied_size = previous_size;
                    if let Some(previous_size) = previous_size {
                        let _ = broadcaster.send(TerminalStreamEvent::ViewportSize(previous_size));
                    }
                })?;
            Ok(())
        })
        .await
        .context("终端 resize 任务失败")??;

        Ok(())
    }

    pub(super) fn record_output_chunk(&self, data: &[u8]) -> TerminalOutputChunk {
        let chunk = TerminalOutputChunk {
            seq: self.next_output_seq.fetch_add(1, Ordering::SeqCst) + 1,
            bytes: data.to_vec(),
        };
        crate::lock_or_recover!(self.backlog.lock()).push(chunk.clone(), MAX_BACKLOG_BYTES);
        chunk
    }

    pub(super) fn mark_closed(&self, message: &[u8]) {
        let was_alive = self.alive.swap(false, Ordering::SeqCst);
        if was_alive {
            if !message.is_empty() {
                let chunk = self.record_output_chunk(message);
                let _ = self.broadcaster.send(TerminalStreamEvent::Output(chunk));
            }
            debug!("terminal session closed: {} ({})", self.path.display(), self.id);
        }
    }

    fn track_title(&self, chunk: &[u8]) {
        let titles = crate::lock_or_recover!(self.title_tracker.lock()).push(chunk);

        if titles.is_empty() {
            return;
        }

        let mut name_state = crate::lock_or_recover!(self.name_state.write());
        for title in titles {
            name_state.update_title(title);
        }
    }
}

fn sanitize_pty_command(command: &mut CommandBuilder) {
    for key in CHILD_PROCESS_ENV_KEYS_TO_CLEAR {
        command.env_remove(key);
    }
}

#[cfg(test)]
mod viewport_tests {
    use super::{TerminalViewportRegistry, TerminalViewportSize};

    #[test]
    fn widest_visible_client_controls_shared_pty_size() {
        let mut registry = TerminalViewportRegistry::default();
        let desktop = registry.register(true);
        let mobile = registry.register(true);

        registry.update_size(desktop, 160, 42);
        registry.update_size(mobile, 48, 30);
        assert_eq!(
            registry.effective_size(),
            Some(TerminalViewportSize {
                cols: 160,
                rows: 42,
            })
        );

        registry.set_visibility(desktop, false);
        assert_eq!(registry.effective_size(), Some(TerminalViewportSize { cols: 48, rows: 30 }));

        registry.set_visibility(desktop, true);
        registry.unregister(desktop);
        assert_eq!(registry.effective_size(), Some(TerminalViewportSize { cols: 48, rows: 30 }));
    }
}

#[cfg(test)]
mod tests {
    use super::{TerminalOutputBacklog, TerminalOutputChunk};

    #[test]
    fn output_backlog_tail_snapshot_keeps_byte_tail_across_chunks() {
        let mut backlog = TerminalOutputBacklog::new();
        backlog.push(
            TerminalOutputChunk {
                seq: 1,
                bytes: b"abc".to_vec(),
            },
            10,
        );
        backlog.push(
            TerminalOutputChunk {
                seq: 2,
                bytes: b"defgh".to_vec(),
            },
            10,
        );

        assert_eq!(backlog.tail_snapshot(6), b"cdefgh");
    }

    #[test]
    fn output_backlog_chunks_after_returns_unsent_sequences() {
        let mut backlog = TerminalOutputBacklog::new();
        for seq in 1..=4 {
            backlog.push(
                TerminalOutputChunk {
                    seq,
                    bytes: vec![b'0' + seq as u8],
                },
                16,
            );
        }

        let recovered: Vec<u64> = backlog
            .chunks_after(2)
            .into_iter()
            .map(|chunk| chunk.seq)
            .collect();
        assert_eq!(recovered, vec![3, 4]);
    }
}

#[cfg(windows)]
fn attach_command(_stored: &StoredTerminalSession, _cwd: &std::path::Path) -> CommandBuilder {
    let shell = std::env::var("WEBCLX_WINDOWS_SHELL").unwrap_or_else(|_| "powershell.exe".into());
    let mut command = CommandBuilder::new(shell);
    command.arg("-NoLogo");
    command
}

#[cfg(not(windows))]
fn attach_command(stored: &StoredTerminalSession, _cwd: &std::path::Path) -> CommandBuilder {
    let mut command = CommandBuilder::new("tmux");
    command.arg("-L");
    command.arg(tmux_socket_name(&stored.id));
    command.arg("attach-session");
    command.arg("-t");
    command.arg(tmux_session_name(&stored.id));
    command
}

#[cfg(windows)]
fn initial_backend_snapshot(_session_id: &str) -> Option<Vec<u8>> {
    None
}

#[cfg(not(windows))]
fn initial_backend_snapshot(session_id: &str) -> Option<Vec<u8>> {
    let line_limit = TERMINAL_INITIAL_SNAPSHOT_LINE_LIMIT.load(Ordering::SeqCst);
    match capture_tmux_initial_pane_snapshot_with_limit(session_id, line_limit) {
        Ok(snapshot) if !snapshot.is_empty() => {
            Some(keep_tail_bytes(&snapshot, MAX_INITIAL_REPLAY_BYTES))
        }
        Ok(_) => None,
        Err(error) => {
            warn!("capture tmux pane snapshot failed for {session_id}: {error}");
            None
        }
    }
}

/// 保留字节尾部。快照深度受设置控制，个别会话（长时间 TUI 重绘）可能超过
/// 回放上限；此时丢头部保尾部，保证回放流仍连续延伸到当前输出。
fn keep_tail_bytes(bytes: &[u8], max_bytes: usize) -> Vec<u8> {
    if bytes.len() <= max_bytes {
        return bytes.to_vec();
    }
    bytes[bytes.len() - max_bytes..].to_vec()
}
