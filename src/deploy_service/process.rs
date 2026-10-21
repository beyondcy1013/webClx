use std::{io, process::Stdio, time::Duration};

use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
};

const MAX_STDERR_BYTES: usize = 64 * 1024;

/// Own the process as soon as it is spawned. Dropping a request must terminate
/// its script too; dropping a spawn_blocking JoinHandle would only detach it.
struct RunningCommand {
    child: Child,
    #[cfg(unix)]
    process_group: Option<i32>,
}

impl RunningCommand {
    fn spawn(command: &mut Command) -> io::Result<Self> {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        command.process_group(0);
        let child = command.spawn()?;
        Ok(Self {
            #[cfg(unix)]
            process_group: child.id().map(|pid| pid as i32),
            child,
        })
    }

    fn kill(&mut self) {
        #[cfg(unix)]
        if let Some(pgid) = self.process_group.take() {
            // This group was created exclusively for this child. Kill bash's
            // descendants as well, including a child holding stderr open.
            unsafe { libc::kill(-pgid, libc::SIGKILL) };
        }
        let _ = self.child.start_kill();
    }
}

impl Drop for RunningCommand {
    fn drop(&mut self) {
        self.kill();
    }
}

async fn read_stderr(mut reader: impl AsyncRead + Unpin) -> io::Result<String> {
    let mut kept = Vec::new();
    let mut buffer = [0; 8192];
    let mut truncated = false;
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let retain = count.min(MAX_STDERR_BYTES - kept.len());
        kept.extend_from_slice(&buffer[..retain]);
        truncated |= retain < count;
        // Continue draining after the limit so a noisy script cannot deadlock
        // on a full pipe or grow server memory without bound.
    }
    let mut message = String::from_utf8_lossy(&kept).trim().to_string();
    if truncated {
        message.push_str("\n[stderr 超过 64 KiB，后续输出已省略]");
    }
    Ok(message)
}

pub(super) async fn run(command: &mut Command, limit: Duration) -> io::Result<()> {
    let mut running = RunningCommand::spawn(command)?;
    let stderr = running.child.stderr.take().expect("piped stderr");
    let result = tokio::time::timeout(limit, async {
        tokio::try_join!(running.child.wait(), read_stderr(stderr))
    })
    .await;

    let error = match result {
        Ok(Ok((status, stderr))) => {
            // Both child and pipes have finished. Do not kill a reaped PID or
            // leave a process-group ID armed after successful completion.
            #[cfg(unix)]
            {
                running.process_group = None;
            }
            return if status.success() {
                Ok(())
            } else {
                Err(io::Error::other(format!("{status}: {stderr}")))
            };
        }
        Ok(Err(error)) => error,
        Err(_) => io::Error::new(
            io::ErrorKind::TimedOut,
            format!("超过 {} 秒，已终止部署进程", limit.as_secs_f64()),
        ),
    };
    running.kill();
    // Reap the killed child without allowing cleanup to hang an HTTP worker.
    let _ = tokio::time::timeout(Duration::from_secs(2), running.child.wait()).await;
    Err(error)
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn bash(script: &str) -> Command {
        let mut command = Command::new("/bin/bash");
        command.args(["-c", script]);
        command
    }

    #[tokio::test]
    async fn large_output_is_drained_and_failure_is_bounded() {
        let error = tokio::time::timeout(
            Duration::from_secs(5),
            run(
                &mut bash("head -c 262144 /dev/zero; head -c 262144 /dev/zero >&2; exit 7"),
                Duration::from_secs(3),
            ),
        )
        .await
        .unwrap()
        .unwrap_err();
        let message = error.to_string();
        assert!(message.contains("7"));
        assert!(message.contains("stderr 超过 64 KiB"));
        assert!(message.len() < MAX_STDERR_BYTES + 256);
    }

    #[tokio::test]
    async fn timeout_covers_children_holding_stderr_open() {
        let fixture =
            super::super::DeployScript::create("sleep 60 & echo $! > \"$0.child\"; exit 0")
                .unwrap();
        let mut command = Command::new("/bin/bash");
        command.arg(&fixture.path);
        let error = tokio::time::timeout(
            Duration::from_secs(5),
            run(&mut command, Duration::from_millis(500)),
        )
        .await
        .unwrap()
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        let pid = std::fs::read_to_string(format!("{}.child", fixture.path.display())).unwrap();
        assert_stopped(pid.trim().parse().unwrap()).await;
    }

    #[tokio::test]
    async fn dropping_process_guard_kills_its_child() {
        let running = RunningCommand::spawn(&mut bash("exec sleep 60")).unwrap();
        let pid = running.child.id().unwrap();
        drop(running);
        assert_stopped(pid).await;
    }

    async fn assert_stopped(pid: u32) {
        tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                if unsafe { libc::kill(pid as i32, 0) } == -1 {
                    break;
                }
                // An orphaned descendant may await the host init's reaper.
                #[cfg(target_os = "linux")]
                if std::fs::read_to_string(format!("/proc/{pid}/status"))
                    .is_ok_and(|status| status.lines().any(|line| line.starts_with("State:\tZ")))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("deployment child remained alive after cancellation");
    }
}
