//! WebSocket viewport lifecycle, backlog replay, and terminal input/output.

use super::session::{
    TerminalOutputChunk, TerminalSession, TerminalStreamEvent, TerminalViewportSize,
};
#[cfg(not(windows))]
use super::tmux::{capture_tmux_initial_pane_snapshot_async, request_tmux_client_refresh};
use super::{
    ClientMessage, MAX_INITIAL_REPLAY_BYTES, TERMINAL_INITIAL_SNAPSHOT_LINE_LIMIT,
    TERMINAL_OUTPUT_VIEWED_MARK_INTERVAL_MS, TERMINAL_TASK_TIMEOUT_SECS,
    TerminalBacklogReplayControl, TerminalConnectionError, TerminalInitialReplay, TerminalManager,
    TerminalManagerEvent, TerminalOutputFramingControl, TerminalSessionQuery,
    TerminalViewportSizeControl, terminal_session_for_socket,
};
use crate::AppState;
use anyhow::{Context, Result};
use axum::extract::ws::{Message, WebSocket};
use futures_util::{Sink, SinkExt, StreamExt};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};
use tokio::sync::broadcast;
use tracing::{info, warn};

/// seq 帧协议的二进制帧头长度：8 字节大端 output seq。
const TERMINAL_OUTPUT_SEQ_HEADER_BYTES: usize = 8;

pub(super) async fn handle_socket_connect(
    socket: WebSocket,
    state: AppState,
    query: TerminalSessionQuery,
) {
    let requested_session_id = query
        .session_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("<latest>")
        .to_string();
    let session_result = tokio::time::timeout(
        std::time::Duration::from_secs(TERMINAL_TASK_TIMEOUT_SECS + 2),
        terminal_session_for_socket(&state, &query),
    )
    .await;

    match session_result {
        Ok(Ok((session, manager, output_visible))) => {
            handle_socket(socket, session, manager, output_visible, query).await;
        }
        Ok(Err(error)) => {
            warn!("terminal websocket session prepare failed: {requested_session_id}: {error}");
            send_socket_connection_error(socket, error.to_string()).await;
        }
        Err(_) => {
            warn!("terminal websocket session prepare timed out: {requested_session_id}");
            send_socket_connection_error(
                socket,
                format!(
                    "终端连接超时（{} 秒），可能是 tmux 或会话恢复无响应。",
                    TERMINAL_TASK_TIMEOUT_SECS + 2
                ),
            )
            .await;
        }
    }
}

async fn send_socket_connection_error(mut socket: WebSocket, message: String) {
    let payload = serde_json::to_string(&TerminalConnectionError { message: &message })
        .unwrap_or_else(|_| {
            "{\"type\":\"terminal_connection_error\",\"message\":\"终端连接失败。\"}".to_string()
        });
    let _ = socket.send(Message::Text(payload.into())).await;
}

async fn handle_socket(
    socket: WebSocket,
    session: Arc<TerminalSession>,
    manager: TerminalManager,
    initially_visible: bool,
    query: TerminalSessionQuery,
) {
    let viewport_id = session.register_viewport(initially_visible);
    handle_registered_socket(
        socket,
        session.clone(),
        manager,
        initially_visible,
        viewport_id,
        query,
    )
    .await;
    if let Err(error) = session.unregister_viewport(viewport_id).await {
        warn!("terminal websocket viewport cleanup failed: {error}");
    }
}

/// 首屏回放的两种形态：全量快照（新客户端/无法续传），或按客户端已应用的
/// seq 水位增量补发缺失 chunk（同一终端缓冲区内的断线重连）。
enum InitialReplayPlan {
    Full(TerminalInitialReplay),
    Delta {
        chunks: Vec<TerminalOutputChunk>,
        last_output_seq: u64,
    },
}

impl InitialReplayPlan {
    fn last_output_seq(&self) -> u64 {
        match self {
            Self::Full(replay) => replay.last_output_seq,
            Self::Delta {
                last_output_seq, ..
            } => *last_output_seq,
        }
    }
}

/// 判定客户端上报的 seq 水位能否增量续传：会话实例必须一致（重建后 seq 会
/// 重置）、水位不能超过当前已分配 seq、backlog 中保留的 chunk 必须从
/// resume_seq+1 连续开始。任一条件不满足都回退到全量快照回放。
pub(super) fn delta_replay_chunks(
    session: &TerminalSession,
    query: &TerminalSessionQuery,
) -> Option<Vec<TerminalOutputChunk>> {
    if !query.seq_framed {
        return None;
    }
    let resume_seq = query.resume_seq?;
    let resume_instance = query.resume_instance?;
    // resume_seq=0 意味着客户端只收到过 seq=0 的种子快照字节，其缓冲状态不
    // 等价于"已应用 seq 0 的连续流"，不能按水位续传。
    if resume_seq == 0
        || resume_instance != session.output_instance_id()
        || resume_seq > session.current_output_seq()
    {
        return None;
    }
    let chunks = session.backlog_chunks_after(resume_seq);
    match chunks.first() {
        None => Some(chunks),
        Some(first) if first.seq == resume_seq + 1 => Some(chunks),
        Some(_) => None,
    }
}

async fn handle_registered_socket(
    socket: WebSocket,
    session: Arc<TerminalSession>,
    manager: TerminalManager,
    initially_visible: bool,
    viewport_id: u64,
    query: TerminalSessionQuery,
) {
    // Subscribe before capturing replay so later chunks remain observable. The
    // replay watermark is read before the tmux snapshot: chunks produced inside
    // the capture window are appended to the replay by seq, while the live loop
    // skips everything at or below the handed-off boundary, so stateful
    // cursor-control bytes are neither lost nor applied twice.
    let mut receiver = session.subscribe();
    let replay_started = Instant::now();
    let initial_replay = match delta_replay_chunks(&session, &query) {
        Some(chunks) => {
            let last_output_seq = chunks
                .last()
                .map(|chunk| chunk.seq)
                .unwrap_or_else(|| query.resume_seq.unwrap_or_default());
            InitialReplayPlan::Delta {
                chunks,
                last_output_seq,
            }
        }
        None => match initial_replay_for_socket(&session).await {
            Ok(replay) => InitialReplayPlan::Full(replay),
            Err(error) => {
                warn!(session_id = %session.id, elapsed_ms = replay_started.elapsed().as_millis(),
                    "terminal initial replay failed: {error:#}");
                send_socket_connection_error(socket, format!("终端首屏快照失败: {error:#}")).await;
                return;
            }
        },
    };
    let delta_replay = matches!(initial_replay, InitialReplayPlan::Delta { .. });
    let replay_last_seq = initial_replay.last_output_seq();
    info!(session_id = %session.id, elapsed_ms = replay_started.elapsed().as_millis(),
        delta = delta_replay, "terminal initial replay captured");
    let mut event_receiver = manager.subscribe_events();
    let (mut sender, mut socket_receiver) = socket.split();
    let input_session = session.clone();
    let input_manager = manager.clone();
    let output_visible = Arc::new(AtomicBool::new(initially_visible));
    let input_output_visible = output_visible.clone();

    let mut input_task = tokio::spawn(async move {
        while let Some(next_input) = socket_receiver.next().await {
            match next_input {
                Ok(message) => {
                    if process_socket_message(
                        &input_session,
                        &input_manager,
                        &input_output_visible,
                        viewport_id,
                        message,
                    )
                    .await
                    .is_err()
                    {
                        break;
                    }
                }
                Err(error) => {
                    warn!("terminal websocket receive error: {error}");
                    break;
                }
            }
        }
    });

    let connect_event = TerminalManagerEvent::SessionListChanged {
        action: "connected".to_string(),
        session_id: session.id.clone(),
    };
    if let Ok(payload) = serde_json::to_string(&connect_event)
        && sender.send(Message::Text(payload.into())).await.is_err()
    {
        input_task.abort();
        return;
    }

    if let Some(size) = session.current_viewport_size()
        && send_terminal_viewport_size(&mut sender, size)
            .await
            .is_err()
    {
        input_task.abort();
        return;
    }

    // seq 帧协商必须先于所有二进制输出：客户端据此决定是否剥离 8 字节 seq
    // 前缀，旧客户端不请求 seq_framed，因此永远不会收到该控制消息。
    if query.seq_framed && send_output_framing_control(&mut sender).await.is_err() {
        input_task.abort();
        return;
    }

    if send_backlog_replay_control(&mut sender, "start", delta_replay, session.output_instance_id())
        .await
        .is_err()
    {
        input_task.abort();
        return;
    }

    let replay_send_failed = match &initial_replay {
        InitialReplayPlan::Full(replay) => {
            !replay.bytes.is_empty()
                && send_binary_chunks(
                    &mut sender,
                    &replay.bytes,
                    query.seq_framed.then_some(replay.last_output_seq),
                )
                .await
                .is_err()
        }
        InitialReplayPlan::Delta { chunks, .. } => {
            let mut failed = false;
            for chunk in chunks {
                if send_terminal_output_chunk(&mut sender, chunk, query.seq_framed)
                    .await
                    .is_err()
                {
                    failed = true;
                    break;
                }
            }
            failed
        }
    };
    if replay_send_failed {
        input_task.abort();
        return;
    }

    if send_backlog_replay_control(&mut sender, "end", delta_replay, session.output_instance_id())
        .await
        .is_err()
    {
        input_task.abort();
        return;
    }

    let output_viewed_mark_interval_duration =
        std::time::Duration::from_millis(TERMINAL_OUTPUT_VIEWED_MARK_INTERVAL_MS as u64);
    let mut output_viewed_mark_interval = tokio::time::interval_at(
        tokio::time::Instant::now() + output_viewed_mark_interval_duration,
        output_viewed_mark_interval_duration,
    );
    output_viewed_mark_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut pending_output_viewed_mark = false;
    let mut last_output_seq_sent = replay_last_seq;
    'socket_loop: loop {
        tokio::select! {
            _ = &mut input_task => break,
            _ = output_viewed_mark_interval.tick(), if pending_output_viewed_mark => {
                if output_visible.load(Ordering::SeqCst) {
                    let manager = manager.clone();
                    let session = session.clone();
                    if let Err(error) = tokio::task::spawn_blocking(move || {
                        manager.mark_session_output_viewed_in_memory(&session.id);
                    }).await {
                        warn!("terminal viewed-state task failed: {error}");
                    }
                    pending_output_viewed_mark = false;
                }
            }
            next_output = receiver.recv() => {
                match next_output {
                    Ok(TerminalStreamEvent::ViewportSize(size)) => {
                        if send_terminal_viewport_size(&mut sender, size).await.is_err() {
                            break;
                        }
                    }
                    Ok(TerminalStreamEvent::Output(chunk)) => {
                        if chunk.seq <= last_output_seq_sent {
                            continue;
                        }
                        if chunk.seq > last_output_seq_sent + 1 {
                            warn!(
                                "terminal websocket observed output gap before chunk {}, recovering from backlog after {}",
                                chunk.seq, last_output_seq_sent
                            );
                            for recovered in session.backlog_chunks_after(last_output_seq_sent) {
                                if recovered.seq <= last_output_seq_sent {
                                    continue;
                                }
                                if recovered.seq > last_output_seq_sent + 1 {
                                    warn!(
                                        "terminal websocket backlog recovery has unrecoverable gap before chunk {} after {}",
                                        recovered.seq, last_output_seq_sent
                                    );
                                }
                                if send_terminal_output_chunk(&mut sender, &recovered, query.seq_framed).await.is_err() {
                                    break 'socket_loop;
                                }
                                last_output_seq_sent = recovered.seq;
                                if output_visible.load(Ordering::SeqCst) {
                                    pending_output_viewed_mark = true;
                                }
                            }
                            continue;
                        }
                        if send_terminal_output_chunk(&mut sender, &chunk, query.seq_framed).await.is_err() {
                            break;
                        }
                        last_output_seq_sent = chunk.seq;
                        if output_visible.load(Ordering::SeqCst) {
                            pending_output_viewed_mark = true;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(
                            "terminal websocket lagged by {skipped} chunks, recovering from backlog after {}",
                            last_output_seq_sent
                        );
                        let recovered_chunks = session.backlog_chunks_after(last_output_seq_sent);
                        if recovered_chunks.is_empty() {
                            warn!(
                                "terminal websocket lagged but no backlog chunks are available after {}",
                                last_output_seq_sent
                            );
                            continue;
                        }
                        for recovered in recovered_chunks {
                            if recovered.seq <= last_output_seq_sent {
                                continue;
                            }
                            if recovered.seq > last_output_seq_sent + 1 {
                                warn!(
                                    "terminal websocket backlog recovery has unrecoverable gap before chunk {} after {}",
                                    recovered.seq, last_output_seq_sent
                                );
                            }
                            if send_terminal_output_chunk(&mut sender, &recovered, query.seq_framed).await.is_err() {
                                break 'socket_loop;
                            }
                            last_output_seq_sent = recovered.seq;
                            if output_visible.load(Ordering::SeqCst) {
                                pending_output_viewed_mark = true;
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            next_event = event_receiver.recv() => {
                match next_event {
                    Ok(event) => {
                        let payload = match serde_json::to_string(&event) {
                            Ok(payload) => payload,
                            Err(error) => {
                                warn!("serialize terminal session event failed: {error}");
                                continue;
                            }
                        };

                        if sender.send(Message::Text(payload.into())).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!("terminal websocket lagged, skipped {skipped} session events");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }

    input_task.abort();
    if output_visible.load(Ordering::SeqCst)
        && let Err(error) = tokio::task::spawn_blocking(move || {
            manager.mark_session_output_viewed(&session.id);
        })
        .await
    {
        warn!("terminal final viewed-state task failed: {error}");
    }
}

async fn send_terminal_output_chunk<S>(
    socket: &mut S,
    chunk: &TerminalOutputChunk,
    seq_framed: bool,
) -> Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let frame = if seq_framed {
        let mut frame = Vec::with_capacity(chunk.bytes.len() + TERMINAL_OUTPUT_SEQ_HEADER_BYTES);
        frame.extend_from_slice(&chunk.seq.to_be_bytes());
        frame.extend_from_slice(&chunk.bytes);
        frame
    } else {
        chunk.bytes.clone()
    };
    socket
        .send(Message::Binary(frame.into()))
        .await
        .context("send terminal output chunk failed")?;
    Ok(())
}

async fn send_terminal_viewport_size<S>(socket: &mut S, size: TerminalViewportSize) -> Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let payload = serde_json::to_string(&TerminalViewportSizeControl {
        cols: size.cols,
        rows: size.rows,
    })?;
    socket.send(Message::Text(payload.into())).await?;
    Ok(())
}

#[cfg(not(windows))]
pub(super) async fn initial_replay_for_socket(
    session: &TerminalSession,
) -> Result<TerminalInitialReplay> {
    // 主回放改为每次接入即时抓取 tmux 快照：快照覆盖抓取瞬间的 pane 全量
    // 状态，不再整段重放 attach 以来的字节流。水位线取抓取前 seq，抓取窗口
    // 内产生的 chunk 仍按 seq 续传，不丢输出；快照前的旧 chunk 重复覆盖
    // 可能把新内容画回旧内容，由 refresh-client 整屏重绘兜底。attach 时的
    // 种子 backlog（seq=0 快照 + live chunk）保留，仍是 lag 恢复与下方
    // buffered 兜底共用的连续流。
    if session.is_alive() {
        let seq_before = session.current_output_seq();
        let line_limit = TERMINAL_INITIAL_SNAPSHOT_LINE_LIMIT.load(Ordering::SeqCst);
        match capture_tmux_initial_pane_snapshot_async(&session.id, line_limit).await {
            Ok(snapshot) if !snapshot.is_empty() => {
                let mut bytes = keep_replay_tail_bytes(&snapshot, MAX_INITIAL_REPLAY_BYTES);
                let mut last_output_seq = seq_before;
                for chunk in session.backlog_chunks_after(seq_before) {
                    bytes.extend_from_slice(&chunk.bytes);
                    last_output_seq = chunk.seq;
                }
                request_tmux_client_refresh(&session.id).await;
                return Ok(TerminalInitialReplay {
                    bytes,
                    last_output_seq,
                });
            }
            Ok(_) => {}
            Err(err) => {
                warn!(
                    "initial_replay_for_socket: capture tmux snapshot failed for {}: {err}",
                    session.id
                );
            }
        }
    }
    Ok(buffered_replay_for_socket(session))
}

#[cfg(windows)]
pub(super) async fn initial_replay_for_socket(
    session: &TerminalSession,
) -> Result<TerminalInitialReplay> {
    Ok(buffered_replay_for_socket(session))
}

/// 保留回放字节尾部。快照深度受设置控制，个别会话（长时间 TUI 重绘）可能
/// 超过回放上限；丢头部保尾部，保证回放流连续延伸到最新输出。
#[cfg(not(windows))]
fn keep_replay_tail_bytes(bytes: &[u8], max_bytes: usize) -> Vec<u8> {
    if bytes.len() <= max_bytes {
        return bytes.to_vec();
    }
    bytes[bytes.len() - max_bytes..].to_vec()
}

fn buffered_replay_for_socket(session: &TerminalSession) -> TerminalInitialReplay {
    let (bytes, last_output_seq) =
        session.backlog_tail_snapshot_with_last_seq(MAX_INITIAL_REPLAY_BYTES);
    TerminalInitialReplay {
        bytes,
        last_output_seq,
    }
}

async fn send_binary_chunks<S>(socket: &mut S, bytes: &[u8], seq: Option<u64>) -> Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    const CHUNK_SIZE: usize = 64 * 1024;

    for chunk in bytes.chunks(CHUNK_SIZE) {
        let frame = match seq {
            // 全量快照回放的字节流整体代表 last_output_seq 水位处的终端状态，
            // 每段都盖上该水位，客户端回放完成后的水位即为它。
            Some(seq) => {
                let mut frame = Vec::with_capacity(chunk.len() + TERMINAL_OUTPUT_SEQ_HEADER_BYTES);
                frame.extend_from_slice(&seq.to_be_bytes());
                frame.extend_from_slice(chunk);
                frame
            }
            None => chunk.to_vec(),
        };
        socket.send(Message::Binary(frame.into())).await?;
    }

    Ok(())
}

async fn send_backlog_replay_control<S>(
    socket: &mut S,
    action: &str,
    delta: bool,
    instance: u64,
) -> Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let payload = serde_json::to_string(&TerminalBacklogReplayControl {
        action,
        delta,
        instance,
    })?;
    socket.send(Message::Text(payload.into())).await?;
    Ok(())
}

async fn send_output_framing_control<S>(socket: &mut S) -> Result<()>
where
    S: Sink<Message> + Unpin,
    S::Error: std::error::Error + Send + Sync + 'static,
{
    let payload = serde_json::to_string(&TerminalOutputFramingControl { version: 2 })?;
    socket.send(Message::Text(payload.into())).await?;
    Ok(())
}

/// 判断输入是否仅由 xterm 鼠标报告序列组成。
///
/// 对应前端 `filterTerminalMouseInput` 识别的两种鼠标报告：
/// - SGR（DECSET 1006）：`\x1b[<<btn>;<col>;<row>(M|m)`
/// - X10/普通（DECSET 9/1000）：`\x1b[M<b1><b2><b3>`（三个任意 payload 字节）
///
/// 鼠标报告是尽力而为的交互输入，单次写入失败时降级丢弃而非断开整条连接，
/// 避免拖动选区时高频鼠标报告偶发写失败导致连接震荡闪烁。
pub(super) fn is_mouse_only_input(data: &str) -> bool {
    let bytes = data.as_bytes();
    let mut cursor = 0;
    let mut matched_any = false;

    while cursor < bytes.len() {
        // SGR 鼠标报告：ESC [ < digits ; digits ; digits (M|m)
        if bytes[cursor] == 0x1b
            && cursor + 2 < bytes.len()
            && bytes[cursor + 1] == b'['
            && bytes[cursor + 2] == b'<'
        {
            let mut scan = cursor + 3;
            // 三个由 ';' 分隔的非空数字段
            let mut fields = 0;
            while fields < 3 && scan < bytes.len() && bytes[scan].is_ascii_digit() {
                while scan < bytes.len() && bytes[scan].is_ascii_digit() {
                    scan += 1;
                }
                fields += 1;
                if fields < 3 {
                    if scan >= bytes.len() || bytes[scan] != b';' {
                        return false;
                    }
                    scan += 1;
                }
            }
            if fields != 3 {
                return false;
            }
            if scan >= bytes.len() || (bytes[scan] != b'M' && bytes[scan] != b'm') {
                return false;
            }
            cursor = scan + 1;
            matched_any = true;
            continue;
        }

        // X10 鼠标报告：ESC [ M <b1> <b2> <b3>
        if bytes[cursor] == 0x1b
            && cursor + 5 < bytes.len()
            && bytes[cursor + 1] == b'['
            && bytes[cursor + 2] == b'M'
        {
            cursor += 6;
            matched_any = true;
            continue;
        }

        return false;
    }

    matched_any
}

/// 写入终端输入，对纯鼠标报告的写入失败降级处理。
///
/// 真实键盘输入/粘贴/控制字符写失败仍返回 Err 以触发断连，因为那说明 PTY
/// 确实不可用；纯鼠标报告写失败只记日志并丢弃，避免选区拖动产生的高频报告
/// 偶发撞上 PTY 关闭窗口时整条连接反复断连重放。
async fn write_terminal_input(
    session: &TerminalSession,
    manager: &TerminalManager,
    data: &str,
) -> Result<()> {
    let started = Instant::now();
    // Capture the submitted input before the PTY write. Agent UIs do not expose
    // their prompt through shell history, so this is the durable fallback when
    // the rollout file cannot be located after a reconnect or restart.
    if !is_mouse_only_input(data) {
        let manager = manager.clone();
        let session_id = session.id.clone();
        let input = data.to_string();
        tokio::task::spawn_blocking(move || {
            manager.record_session_input_deferred(&session_id, &input);
        })
        .await
        .context("终端输入记录任务失败")?;
    }
    let captured_at = Instant::now();
    let result = session.write_input(data.to_string()).await;
    if started.elapsed() >= std::time::Duration::from_millis(100) {
        warn!(session_id = %session.id,
            capture_ms = captured_at.duration_since(started).as_millis(),
            pty_write_ms = captured_at.elapsed().as_millis(),
            total_ms = started.elapsed().as_millis(),
            "slow terminal input");
    }
    match result {
        Ok(()) => Ok(()),
        Err(error) if is_mouse_only_input(data) => {
            warn!("dropping mouse-only terminal input after write failure: {error}");
            Ok(())
        }
        Err(error) => Err(error),
    }
}

async fn process_socket_message(
    session: &TerminalSession,
    manager: &TerminalManager,
    output_visible: &AtomicBool,
    viewport_id: u64,
    message: Message,
) -> Result<()> {
    match message {
        Message::Text(text) => {
            if let Ok(client_message) = serde_json::from_str::<ClientMessage>(&text) {
                match client_message {
                    ClientMessage::Input { data } => {
                        write_terminal_input(session, manager, &data).await?;
                    }
                    ClientMessage::Resize { cols, rows } => {
                        session.resize_viewport(viewport_id, cols, rows).await?
                    }
                    ClientMessage::Visibility { visible } => {
                        session
                            .set_viewport_visibility(viewport_id, visible)
                            .await?;
                        let was_visible = output_visible.swap(visible, Ordering::SeqCst);
                        let manager = manager.clone();
                        let session_id = session.id.clone();
                        tokio::task::spawn_blocking(move || {
                            if visible {
                                manager.mark_session_opened(&session_id);
                                manager.mark_session_output_viewed(&session_id);
                            } else if was_visible {
                                manager.mark_session_output_viewed(&session_id);
                            }
                        })
                        .await
                        .context("终端可见状态更新任务失败")?;
                    }
                }
            }
        }
        Message::Binary(bytes) => {
            if let Ok(text) = String::from_utf8(bytes.to_vec()) {
                write_terminal_input(session, manager, &text).await?;
            }
        }
        Message::Ping(_) | Message::Pong(_) => {}
        Message::Close(_) => return Err(anyhow::anyhow!("websocket closed")),
    }

    Ok(())
}
