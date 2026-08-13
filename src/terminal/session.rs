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
use super::capture_tmux_initial_pane_snapshot;
use super::{
    CHILD_PROCESS_ENV_KEYS_TO_CLEAR, DEFAULT_COLS, DEFAULT_ROWS, INITIAL_TMUX_REDRAW_SUPPRESS_MS,
    MAX_BACKLOG_BYTES, StoredTerminalSession,
};

const TERMINAL_OUTPUT_CHANNEL_CAPACITY: usize = 4096;

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
    pub(super) broadcaster: broadcast::Sender<TerminalOutputChunk>,
    pub(super) backlog: Arc<Mutex<TerminalOutputBacklog>>,
    pub(super) next_output_seq: AtomicU64,
    pub(super) initial_snapshot: Arc<Mutex<Option<Vec<u8>>>>,
    pub(super) attached_at: Instant,
    pub(super) suppressed_initial_redraw: AtomicBool,
    pub(super) last_output_at: AtomicU64,
    pub(super) alive: Arc<AtomicBool>,
}

impl TerminalSession {
    pub(super) fn attach(
        stored: &StoredTerminalSession,
        proxy_env: Vec<(String, String)>,
        startup_script: Option<String>,
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

        let initial_snapshot = initial_backend_snapshot(&stored.id);
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
            broadcaster,
            backlog: Arc::new(Mutex::new(TerminalOutputBacklog::with_capacity(MAX_BACKLOG_BYTES))),
            next_output_seq: AtomicU64::new(0),
            initial_snapshot: Arc::new(Mutex::new(initial_snapshot)),
            attached_at: Instant::now(),
            suppressed_initial_redraw: AtomicBool::new(!should_suppress_initial_redraw),
            last_output_at: AtomicU64::new(0),
            alive: Arc::new(AtomicBool::new(true)),
        });

        Self::spawn_reader_thread(session.clone(), reader);
        Ok(session)
    }

    fn spawn_reader_thread(session: Arc<Self>, mut reader: Box<dyn Read + Send>) {
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => {
                        session.mark_closed(b"\r\n[webclx] terminal client exited.\r\n");
                        break;
                    }
                    Ok(count) => {
                        if session.should_suppress_initial_tmux_redraw() {
                            continue;
                        }
                        session.note_output_now();
                        let chunk = session.record_output_chunk(&buffer[..count]);
                        session.track_title(&chunk.bytes);
                        let _ = session.broadcaster.send(chunk);
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

    pub(super) fn subscribe(&self) -> broadcast::Receiver<TerminalOutputChunk> {
        self.broadcaster.subscribe()
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

    pub(super) fn backlog_snapshot(&self) -> Vec<u8> {
        crate::lock_or_recover!(self.backlog.lock()).snapshot()
    }

    pub(super) fn backlog_tail_snapshot(&self, max_bytes: usize) -> Vec<u8> {
        crate::lock_or_recover!(self.backlog.lock()).tail_snapshot(max_bytes)
    }

    pub(super) fn backlog_chunks_after(&self, seq: u64) -> Vec<TerminalOutputChunk> {
        crate::lock_or_recover!(self.backlog.lock()).chunks_after(seq)
    }

    pub(super) fn initial_backlog_snapshot(&self) -> Option<Vec<u8>> {
        crate::lock_or_recover!(self.initial_snapshot.lock()).take()
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
            writer.write_all(data.as_bytes())?;
            writer.flush()?;
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
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut viewports = crate::lock_or_recover!(viewports.lock());
            let Some(size) = viewports.effective_size() else {
                return Ok(());
            };
            if viewports.applied_size == Some(size) {
                return Ok(());
            }
            crate::lock_or_recover!(master.lock()).resize(PtySize {
                rows: size.rows,
                cols: size.cols,
                pixel_width: 0,
                pixel_height: 0,
            })?;
            viewports.applied_size = Some(size);
            Ok(())
        })
        .await
        .context("终端 resize 任务失败")??;

        Ok(())
    }

    fn record_output_chunk(&self, data: &[u8]) -> TerminalOutputChunk {
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
            let chunk = self.record_output_chunk(message);
            let _ = self.broadcaster.send(chunk);
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
    match capture_tmux_initial_pane_snapshot(session_id) {
        Ok(snapshot) if !snapshot.is_empty() => Some(snapshot),
        Ok(_) => None,
        Err(error) => {
            warn!("capture tmux pane snapshot failed for {session_id}: {error}");
            None
        }
    }
}
