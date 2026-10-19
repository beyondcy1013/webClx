//! 短命「spawn → 收集输出 → 退出」命令的受限执行器。
//!
//! 裸 `Command::output()` / `wait_with_output()` 在子进程挂死时会永久占住一个
//! 阻塞线程。这里的两个入口统一加上墙钟超时与聚合字节上限：
//!
//! - [`bounded_output`] / [`bounded_output_with_stdin`]：同步版本，Unix 下用
//!   非阻塞管道 + `poll` 同时 drain stdout/stderr（可选地向 stdin 喂数据），
//!   超过 `budget` 或输出超过 `max_bytes` 时通过 RAII guard 杀掉并回收子进程；
//!   非 Unix 平台经 `bounded_output_portable` 在独立线程的 current_thread
//!   Tokio 运行时上走同一 async 核心，时限与字节上限一致生效。
//! - [`bounded_output_tokio`]：async 版本，`kill_on_drop(true)` + `timeout_at`
//!   保证超时/取消终止子进程；读取循环在缓冲前做聚合字节检查，超限即拒绝——
//!   `max_bytes` 是运行期硬上限而非事后校验。

use std::{
    process::{Command, Output},
    time::Duration,
};

use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// 执行命令并 drain stdout/stderr 至 EOF。`max_bytes` 是 stdout+stderr 的
/// 聚合上限——超限或超时都会返回错误并杀掉子进程（`Output` 不会被静默截断）。
/// stdin 固定为 null；需要喂 stdin 的调用点用 [`bounded_output_with_stdin`]。
pub(crate) fn bounded_output(
    command: Command,
    budget: Duration,
    max_bytes: usize,
) -> Result<Output> {
    imp::run(command, None, budget, max_bytes)
}

/// 与 [`bounded_output`] 相同，但先把 `stdin_data` 写入子进程 stdin 再收集输出
/// （供 `crontab -` 这类从 stdin 读取负载的命令使用）。写入与 drain 在同一
/// 轮询循环内推进：子进程不读 stdin 时不会无限阻塞，超时同样生效。
pub(crate) fn bounded_output_with_stdin(
    command: Command,
    stdin_data: &[u8],
    budget: Duration,
    max_bytes: usize,
) -> Result<Output> {
    imp::run(command, Some(stdin_data), budget, max_bytes)
}

/// async 版本：强制 `kill_on_drop(true)`，超时、调用方放弃 future 或输出
/// 超限都会终止子进程。`max_bytes` 是读取循环内的聚合硬上限而非事后校验。
pub(crate) async fn bounded_output_tokio(
    command: tokio::process::Command,
    budget: Duration,
    max_bytes: usize,
) -> Result<Output> {
    bounded_output_tokio_inner(command, None, budget, max_bytes).await
}

/// 受限执行核心：spawn 后由单个 `try_join!` 并发推进 stdin 喂入与
/// stdout/stderr drain（外加 `child.wait()`），`timeout_at` 统一约束三者
/// 的完成时限——stdin 喂不完、管道不到 EOF、进程不退出都会触发超时。
/// 读取循环在缓冲前检查聚合字节上限，超限立即 bail，不等待进程退出。
/// 出错时 best-effort `start_kill` + 1 秒内的 `wait` 回收；整个 future 被
/// 丢弃（取消）时由 Tokio child 的 kill_on_drop 兜底。本 API 只对直接子
/// 进程负责，不承诺子孙进程组清理。
async fn bounded_output_tokio_inner(
    mut command: tokio::process::Command,
    stdin_data: Option<&[u8]>,
    budget: Duration,
    max_bytes: usize,
) -> Result<Output> {
    let deadline = tokio::time::Instant::now()
        .checked_add(budget)
        .context("命令超时预算溢出")?;
    command.kill_on_drop(true);
    if stdin_data.is_some() {
        command.stdin(std::process::Stdio::piped());
    } else {
        command.stdin(std::process::Stdio::null());
    }
    command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    // child 归本函数作用域持有：operation future 借用它，超时返回后借用
    // 随 future 一起释放，随后才能做 start_kill/wait 清理。
    let mut child = command.spawn().context("启动命令失败")?;
    let stdin = child.stdin.take();
    let mut stdout = child.stdout.take().context("命令 stdout 管道缺失")?;
    let mut stderr = child.stderr.take().context("命令 stderr 管道缺失")?;

    let operation = async {
        let write_input = async {
            if let Some(mut pipe) = stdin {
                match pipe.write_all(stdin_data.unwrap_or(&[])).await {
                    Ok(()) => {}
                    // 子进程不读 stdin 即退出时按 EOF 处理：其输出/退出
                    // 状态解释原因。
                    Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
                    Err(error) => return Err(error).context("写入命令 stdin 失败"),
                }
                // 关闭 stdin，让子进程读到 EOF。
                drop(pipe);
            }
            Ok::<(), anyhow::Error>(())
        };
        let collect = async {
            let mut out = Vec::new();
            let mut err = Vec::new();
            let mut out_done = false;
            let mut err_done = false;
            let mut status = None;
            let mut obuf = [0u8; 8192];
            let mut ebuf = [0u8; 8192];
            while !out_done || !err_done || status.is_none() {
                tokio::select! {
                    n = stdout.read(&mut obuf), if !out_done => {
                        let n = n.context("读取命令 stdout 失败")?;
                        if n == 0 {
                            out_done = true;
                        } else if n > max_bytes.saturating_sub(out.len().saturating_add(err.len())) {
                            anyhow::bail!("命令输出超过限制");
                        } else {
                            out.extend_from_slice(&obuf[..n]);
                        }
                    }
                    n = stderr.read(&mut ebuf), if !err_done => {
                        let n = n.context("读取命令 stderr 失败")?;
                        if n == 0 {
                            err_done = true;
                        } else if n > max_bytes.saturating_sub(out.len().saturating_add(err.len())) {
                            anyhow::bail!("命令输出超过限制");
                        } else {
                            err.extend_from_slice(&ebuf[..n]);
                        }
                    }
                    s = child.wait(), if status.is_none() => {
                        status = Some(s.context("等待命令失败")?);
                    }
                }
            }
            Ok::<Output, anyhow::Error>(Output {
                status: status.expect("collect loop exits only after exit status"),
                stdout: out,
                stderr: err,
            })
        };
        let (_, output) = tokio::try_join!(write_input, collect)?;
        Ok(output)
    };
    let result = tokio::time::timeout_at(deadline, operation)
        .await
        .context("命令执行超时")
        .and_then(|r| r);
    if result.is_err() {
        // best effort 清理：通知 SIGKILL 后最多等 1 秒回收；失败只记录，
        // 保留原始错误返回。
        let _ = child.start_kill();
        match tokio::time::timeout(Duration::from_secs(1), child.wait()).await {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => tracing::warn!(%error, "process_exec 子进程回收失败"),
            Err(_) => tracing::warn!("process_exec 子进程回收超时"),
        }
    }
    result
}

/// 非 Unix 平台（以及 Unix 下测试该路径）的受限执行入口：在独立作用域
/// 线程内构建 current_thread Tokio 运行时执行同一 async 核心，时限与
/// 聚合字节上限跨平台一致生效，无静默降级。scoped join 保证线程先于
/// 函数返回结束，线程内 panic 转为普通错误；即使调用方已在 Tokio
/// 运行时内，独立线程也不会触发 nested-runtime panic。
#[cfg(any(not(unix), test))]
fn bounded_output_portable(
    command: Command,
    stdin_data: Option<&[u8]>,
    budget: Duration,
    max_bytes: usize,
) -> Result<Output> {
    std::thread::scope(|scope| {
        scope
            .spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .context("创建命令执行运行时失败")?;
                runtime.block_on(bounded_output_tokio_inner(
                    tokio::process::Command::from(command),
                    stdin_data,
                    budget,
                    max_bytes,
                ))
            })
            .join()
            .map_err(|_| anyhow::anyhow!("命令执行线程 panic"))?
    })
}

#[cfg(unix)]
mod imp {
    use std::{
        io::{self, Read, Write},
        os::unix::io::AsRawFd,
        process::{Child, Command, Output, Stdio},
        time::{Duration, Instant},
    };

    use anyhow::{Context, Result};

    pub(super) fn run(
        mut command: Command,
        stdin_data: Option<&[u8]>,
        budget: Duration,
        max_bytes: usize,
    ) -> Result<Output> {
        if stdin_data.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let deadline = Instant::now()
            .checked_add(budget)
            .context("命令超时预算溢出")?;
        let child = command.spawn().context("启动命令失败")?;
        let mut guard = ChildGuard::new(child);
        let child = guard.child_mut();
        let mut stdin = child.stdin.take();
        let mut stdout = child.stdout.take().context("命令 stdout 管道缺失")?;
        let mut stderr = child.stderr.take().context("命令 stderr 管道缺失")?;
        set_nonblocking(stdout.as_raw_fd()).context("设置 stdout 非阻塞失败")?;
        set_nonblocking(stderr.as_raw_fd()).context("设置 stderr 非阻塞失败")?;
        if let Some(pipe) = stdin.as_ref() {
            set_nonblocking(pipe.as_raw_fd()).context("设置 stdin 非阻塞失败")?;
        }

        let input = stdin_data.unwrap_or(&[]);
        let mut stdin_pos = 0usize;
        let mut stdin_done = stdin.is_none();
        let mut out = Vec::new();
        let mut err = Vec::new();
        let mut done_out = false;
        let mut done_err = false;
        let mut status = None;
        let mut buf = [0u8; 8192];

        loop {
            if Instant::now() >= deadline {
                anyhow::bail!("命令执行超时");
            }
            if !done_out {
                let remaining = max_bytes.saturating_sub(err.len());
                done_out = drain_pipe(&mut stdout, &mut out, &mut buf, remaining)?;
            }
            if !done_err {
                let remaining = max_bytes.saturating_sub(out.len());
                done_err = drain_pipe(&mut stderr, &mut err, &mut buf, remaining)?;
            }
            if !stdin_done {
                let pipe = stdin.as_mut().expect("stdin pipe missing while pending");
                match feed_pipe(pipe, input, &mut stdin_pos) {
                    Ok(true) => {
                        stdin_done = true;
                        // 关闭 stdin，让子进程读到 EOF。
                        drop(stdin.take());
                    }
                    Ok(false) => {}
                    Err(error) => return Err(error).context("写入命令 stdin 失败"),
                }
            }
            if status.is_none() {
                status = child.try_wait().context("等待命令失败")?;
            }
            if done_out
                && done_err
                && let Some(status) = status.take()
            {
                guard.disarm();
                return Ok(Output {
                    status,
                    stdout: out,
                    stderr: err,
                });
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            let timeout_ms = remaining.as_millis().clamp(1, 10) as i32;
            let mut fds = [
                libc::pollfd {
                    fd: if done_out { -1 } else { stdout.as_raw_fd() },
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: if done_err { -1 } else { stderr.as_raw_fd() },
                    events: libc::POLLIN,
                    revents: 0,
                },
                libc::pollfd {
                    fd: if stdin_done {
                        -1
                    } else {
                        stdin.as_ref().map_or(-1, AsRawFd::as_raw_fd)
                    },
                    events: libc::POLLOUT,
                    revents: 0,
                },
            ];
            // SAFETY: descriptors are owned by the pipes for this scope.
            let result = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as _, timeout_ms) };
            if result < 0 && io::Error::last_os_error().kind() != io::ErrorKind::Interrupted {
                return Err(io::Error::last_os_error()).context("等待命令输出失败");
            }
        }
    }

    /// Drain a nonblocking pipe into `target`, capped at `max_bytes` additional
    /// bytes. Returns `true` on EOF. Errors instead of truncating when the
    /// caller-supplied cap would be exceeded.
    fn drain_pipe(
        pipe: &mut dyn Read,
        target: &mut Vec<u8>,
        buf: &mut [u8],
        max_bytes: usize,
    ) -> io::Result<bool> {
        for _ in 0..8 {
            match pipe.read(buf) {
                Ok(0) => return Ok(true),
                Ok(n) => {
                    if target.len().saturating_add(n) > max_bytes {
                        return Err(io::Error::other("命令输出超过限制"));
                    }
                    target.extend_from_slice(&buf[..n]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            }
        }
        Ok(false)
    }

    /// Feed `data` into a nonblocking stdin pipe starting at `pos`. Returns
    /// `true` once all bytes are written or the child closed stdin (EPIPE —
    /// the child's own output/exit status explains why), `false` when the
    /// pipe would block.
    fn feed_pipe(pipe: &mut impl Write, data: &[u8], pos: &mut usize) -> io::Result<bool> {
        while *pos < data.len() {
            match pipe.write(&data[*pos..]) {
                Ok(0) => return Ok(true),
                Ok(n) => *pos += n,
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(false),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) if error.kind() == io::ErrorKind::BrokenPipe => return Ok(true),
                Err(error) => return Err(error),
            }
        }
        Ok(true)
    }

    /// `pub(super)` 仅供同 crate 的 process_exec 测试构造 panic 回收用例。
    pub(super) struct ChildGuard {
        child: Option<Child>,
    }

    impl ChildGuard {
        pub(super) fn new(child: Child) -> Self {
            Self { child: Some(child) }
        }
        fn child_mut(&mut self) -> &mut Child {
            self.child.as_mut().expect("child guard consumed")
        }
        fn disarm(&mut self) {
            self.child.take();
        }
    }

    impl Drop for ChildGuard {
        fn drop(&mut self) {
            if let Some(child) = self.child.as_mut() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn set_nonblocking(fd: i32) -> io::Result<()> {
        // SAFETY: fcntl operates on a valid pipe descriptor.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }
        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }
}

/// 非 Unix 平台没有 `poll` 式 drain：经 `bounded_output_portable` 在独立
/// 线程的 current_thread Tokio 运行时上执行同一 async 核心，运行时限与
/// 聚合字节上限与 Unix 一致生效，无事后兜底。
#[cfg(not(unix))]
mod imp {
    use std::{
        process::{Command, Output},
        time::Duration,
    };

    use anyhow::Result;

    pub(super) fn run(
        command: Command,
        stdin_data: Option<&[u8]>,
        budget: Duration,
        max_bytes: usize,
    ) -> Result<Output> {
        super::bounded_output_portable(command, stdin_data, budget, max_bytes)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::imp::ChildGuard;
    use super::{bounded_output, bounded_output_with_stdin};
    use std::io;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn bounded_output_preserves_nonzero_and_both_pipes() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf out; printf err >&2; exit 7"]);
        let output = bounded_output(command, Duration::from_secs(1), 1024).unwrap();
        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr, b"err");
        assert_eq!(output.status.code(), Some(7));
    }

    #[test]
    fn bounded_output_drains_both_pipes_beyond_pipe_capacity() {
        let mut command = Command::new("python3");
        command.args(["-c", "import sys; sys.stdout.buffer.write(b'o' * 131072); sys.stderr.buffer.write(b'e' * 131072)"]);
        let output = bounded_output(command, Duration::from_secs(3), 300_000).unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, vec![b'o'; 131072]);
        assert_eq!(output.stderr, vec![b'e'; 131072]);
    }

    #[test]
    fn bounded_output_rejects_aggregate_limit() {
        let mut command = Command::new("python3");
        command.args(["-c", "import sys; sys.stdout.buffer.write(b'o' * 6000); sys.stderr.buffer.write(b'e' * 6000)"]);
        let error = bounded_output(command, Duration::from_secs(1), 10_000).unwrap_err();
        assert!(error.to_string().contains("命令输出超过限制"));
    }

    #[test]
    fn bounded_output_rejects_continuous_output_before_deadline() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "while :; do printf x; done"]);
        let error = bounded_output(command, Duration::from_secs(2), 4096).unwrap_err();
        assert!(error.to_string().contains("命令输出超过限制"));
    }

    #[test]
    fn bounded_output_times_out_and_reaps_child() {
        let path = std::env::temp_dir().join(format!(
            "webclx-bounded-output-{}-{}.pid",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = std::fs::remove_file(&path);
        let _cleanup = TempFileGuard(path.clone());
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            &format!("echo $$ > {}; exec sleep 30", shell_quote(&path)),
        ]);
        let error = bounded_output(command, Duration::from_millis(500), 1024).unwrap_err();
        assert!(error.to_string().contains("命令执行超时"));
        let pid = std::fs::read_to_string(&path)
            .unwrap()
            .trim()
            .parse::<libc::pid_t>()
            .unwrap();
        // SAFETY: signal zero only probes whether the exact child still exists.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    }

    #[test]
    fn bounded_output_with_stdin_feeds_and_collects() {
        let command = Command::new("/bin/cat");
        let input = vec![b'x'; 200_000];
        let output =
            bounded_output_with_stdin(command, &input, Duration::from_secs(5), 400_000).unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, input);
    }

    #[test]
    fn bounded_output_with_stdin_tolerates_early_pipe_close() {
        // `head -c 0` exits without reading; the writer must see EPIPE instead
        // of blocking, and the child's exit status still drives the result.
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "head -c 0 >/dev/null; exit 3"]);
        let output =
            bounded_output_with_stdin(command, b"ignored", Duration::from_secs(5), 1024).unwrap();
        assert_eq!(output.status.code(), Some(3));
    }

    #[test]
    fn bounded_output_spawn_failure_reports_context() {
        let command = Command::new("/nonexistent-webclx-process-exec-test");
        let error = bounded_output(command, Duration::from_secs(1), 1024).unwrap_err();
        assert!(error.to_string().contains("启动命令失败"));
    }

    #[test]
    fn child_guard_reaps_on_panic() {
        let child = Command::new("sleep").arg("30").spawn().unwrap();
        let pid = child.id() as libc::pid_t;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = ChildGuard::new(child);
            panic!("intentional test panic");
        }));
        assert!(result.is_err());
        // SAFETY: signal zero only probes whether the exact child still exists.
        assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
        assert_eq!(io::Error::last_os_error().raw_os_error(), Some(libc::ESRCH));
    }

    #[test]
    fn portable_adapter_drains_pipes_feeds_stdin_and_passes_status() {
        // portable 路径（非 Unix 的实际执行路径，Unix 下借测试覆盖同一 async
        // 核心）：stdin 喂入超过 pipe 容量时写必须与 stdout/stderr drain
        // 并发推进；cat 回显全部输入，非零退出状态原样返回。
        let input = vec![b'x'; 200_000];
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "cat; printf err >&2; exit 3"]);
        let output =
            super::bounded_output_portable(command, Some(&input), Duration::from_secs(5), 300_000)
                .unwrap();
        assert_eq!(output.status.code(), Some(3));
        assert_eq!(output.stdout, input);
        assert_eq!(output.stderr, b"err");
    }

    /// `pub(super)` 供 tokio_tests 的 pid 文件用例复用同一清理语义。
    pub(super) struct TempFileGuard(pub(super) std::path::PathBuf);

    impl Drop for TempFileGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn shell_quote(path: &std::path::Path) -> String {
        format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
    }
}

// 全部用例经 /bin/sh、kill(0) 与 pid 文件构造，仅 Unix 下有效。
#[cfg(all(test, unix))]
mod tokio_tests {
    use super::bounded_output_tokio;
    use super::tests::TempFileGuard;
    use std::io;
    use std::time::{Duration, Instant};
    use tokio::process::Command;

    #[tokio::test]
    async fn tokio_bounded_output_collects_and_passes_exit_code() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "printf out; printf err >&2; exit 7"]);
        let output = tokio::time::timeout(
            Duration::from_secs(3),
            bounded_output_tokio(command, Duration::from_secs(2), 1024),
        )
        .await
        .expect("test-local deadline")
        .unwrap();
        assert_eq!(output.stdout, b"out");
        assert_eq!(output.stderr, b"err");
        assert_eq!(output.status.code(), Some(7));
    }

    #[tokio::test]
    async fn tokio_bounded_output_timeout_kills_child() {
        let mut command = Command::new("/bin/sleep");
        command.arg("30");
        let error = tokio::time::timeout(
            Duration::from_secs(3),
            bounded_output_tokio(command, Duration::from_millis(100), 1024),
        )
        .await
        .expect("test-local deadline")
        .unwrap_err();
        assert!(error.to_string().contains("命令执行超时"));
    }

    #[tokio::test]
    async fn tokio_bounded_output_rejects_oversized_output() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "head -c 4096 /dev/zero | tr '\\0' x"]);
        let error = tokio::time::timeout(
            Duration::from_secs(3),
            bounded_output_tokio(command, Duration::from_secs(2), 1024),
        )
        .await
        .expect("test-local deadline")
        .unwrap_err();
        assert!(error.to_string().contains("命令输出超过限制"));
    }

    #[tokio::test]
    async fn tokio_bounded_output_rejects_continuous_output_before_deadline() {
        // 子进程持续输出：上限必须在 2s 预算耗尽前触发，而不是超时错误。
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "while :; do printf x; done"]);
        let error = tokio::time::timeout(
            Duration::from_secs(3),
            bounded_output_tokio(command, Duration::from_secs(2), 4096),
        )
        .await
        .expect("test-local deadline")
        .unwrap_err();
        assert!(error.to_string().contains("命令输出超过限制"));
    }

    #[tokio::test]
    async fn tokio_bounded_output_rejects_aggregate_of_both_pipes() {
        // stdout 与 stderr 单独都未超限，聚合超过 max_bytes 必须拒绝。
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "head -c 6000 /dev/zero | tr '\\0' o; head -c 6000 /dev/zero | tr '\\0' e >&2",
        ]);
        let error = tokio::time::timeout(
            Duration::from_secs(3),
            bounded_output_tokio(command, Duration::from_secs(2), 10_000),
        )
        .await
        .expect("test-local deadline")
        .unwrap_err();
        assert!(error.to_string().contains("命令输出超过限制"));
    }

    #[tokio::test]
    async fn tokio_bounded_output_accepts_exact_aggregate_cap() {
        // stdout+stderr 恰好等于上限：边界值必须放行且不截断。
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "head -c 512 /dev/zero | tr '\\0' o; head -c 512 /dev/zero | tr '\\0' e >&2",
        ]);
        let output = tokio::time::timeout(
            Duration::from_secs(3),
            bounded_output_tokio(command, Duration::from_secs(2), 1024),
        )
        .await
        .expect("test-local deadline")
        .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, vec![b'o'; 512]);
        assert_eq!(output.stderr, vec![b'e'; 512]);
    }

    #[tokio::test]
    async fn tokio_bounded_output_timeout_kills_exact_child() {
        // 超时后 helper 返回时对应的子进程必须已被回收：pid 文件经位置
        // 参数 $1 传入，避免路径注入引号风险。
        let path = pid_file_path("timeout");
        let _cleanup = TempFileGuard(path.clone());
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "echo $$ > \"$1\"; exec sleep 30",
            "webclx-bounded-tokio-timeout",
            &path.display().to_string(),
        ]);
        let error = tokio::time::timeout(
            Duration::from_secs(3),
            bounded_output_tokio(command, Duration::from_millis(500), 1024),
        )
        .await
        .expect("test-local deadline")
        .unwrap_err();
        assert!(error.to_string().contains("命令执行超时"));
        let pid = wait_pid_file(&path).await;
        assert_pid_gone(pid).await;
    }

    #[tokio::test]
    async fn tokio_bounded_output_cancel_kills_child_via_drop() {
        // 调用方在子进程存活期间丢弃 future：kill_on_drop 必须终止它。
        let path = pid_file_path("cancel");
        let _cleanup = TempFileGuard(path.clone());
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "echo $$ > \"$1\"; exec sleep 30",
            "webclx-bounded-tokio-cancel",
            &path.display().to_string(),
        ]);
        let pid = {
            let future = bounded_output_tokio(command, Duration::from_secs(30), 1024);
            tokio::pin!(future);
            // 以短步进推进 future 直到 pid 文件出现（即子进程已 spawn 并
            // 存活）；每次 timeout 返回 Err 只放弃本次借用，不丢弃 future。
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                if let Ok(text) = std::fs::read_to_string(&path)
                    && let Ok(pid) = text.trim().parse::<libc::pid_t>()
                {
                    break pid;
                }
                assert!(Instant::now() < deadline, "child pid file not written in time");
                let _ = tokio::time::timeout(Duration::from_millis(20), future.as_mut()).await;
            }
            // 离开块即丢弃 future（取消）：kill_on_drop 必须终止子进程。
        };
        assert_pid_gone(pid).await;
    }

    fn pid_file_path(tag: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "webclx-bounded-tokio-{tag}-{}-{}.pid",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let _ = std::fs::remove_file(&path);
        path
    }

    /// 有界等待 pid 文件出现（子进程已写入自身 PID）。
    async fn wait_pid_file(path: &std::path::Path) -> libc::pid_t {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Ok(text) = std::fs::read_to_string(path)
                && let Ok(pid) = text.trim().parse::<libc::pid_t>()
            {
                return pid;
            }
            assert!(Instant::now() < deadline, "child pid file not written in time");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// 有界等待子进程消失：SIGKILL 投递与孤儿回收需要一点时间，不能
    /// 断言瞬时 ESRCH。
    async fn assert_pid_gone(pid: libc::pid_t) {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            // SAFETY: signal zero only probes whether the exact child still exists.
            if unsafe { libc::kill(pid, 0) } == -1
                && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH)
            {
                return;
            }
            assert!(Instant::now() < deadline, "child pid {pid} still alive");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}
