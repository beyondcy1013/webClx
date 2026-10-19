//! Linux process identities for terminal destruction. Never signal a raw PID.
use anyhow::{Context, Result};
use std::{
    collections::HashSet,
    fs, io,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    thread,
    time::{Duration, Instant},
};

pub(super) struct Process {
    pid: u32,
    fd: OwnedFd,
    resume_on_drop: bool,
}

impl Process {
    pub(super) fn pause(pid: u32) -> Result<Option<Self>> {
        let Some(mut process) = Self::open(pid)? else {
            return Ok(None);
        };
        process.freeze(Instant::now() + Duration::from_secs(2))?;
        if process.exited()? {
            return Ok(None);
        }
        Ok(Some(process))
    }

    pub(super) fn resume(&mut self) -> Result<()> {
        self.signal(libc::SIGCONT)?;
        self.resume_on_drop = false;
        Ok(())
    }

    fn open(pid: u32) -> Result<Option<Self>> {
        anyhow::ensure!(pid > 1 && pid != std::process::id(), "拒绝回收服务自身或系统进程");
        // SAFETY: pidfd_open takes integers and returns an owned descriptor.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) };
        if fd < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(libc::ESRCH) {
                return Ok(None);
            }
            return Err(error).context("打开终端进程 pidfd 失败（不降级为裸 PID 信号）");
        }
        Ok(Some(Self {
            pid,
            // SAFETY: successful syscall returned a new descriptor owned here.
            fd: unsafe { OwnedFd::from_raw_fd(fd as i32) },
            resume_on_drop: false,
        }))
    }

    fn signal(&self, signal: i32) -> Result<()> {
        // SAFETY: valid owned pidfd; null siginfo requests normal signal delivery.
        let result = unsafe {
            libc::syscall(
                libc::SYS_pidfd_send_signal,
                self.fd.as_raw_fd(),
                signal,
                std::ptr::null::<libc::siginfo_t>(),
                0,
            )
        };
        if result < 0 {
            let error = io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error)
                    .with_context(|| format!("向终端进程 {} 发送信号 {signal} 失败", self.pid));
            }
        }
        Ok(())
    }

    fn exited(&self) -> Result<bool> {
        let mut poll = libc::pollfd {
            fd: self.fd.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: one valid pollfd, nonblocking poll.
        let result = unsafe { libc::poll(&mut poll, 1, 0) };
        if result < 0 {
            return Err(io::Error::last_os_error()).context("检查终端进程退出失败");
        }
        Ok(poll.revents & libc::POLLIN != 0)
    }

    fn freeze(&mut self, deadline: Instant) -> Result<()> {
        if self.exited()? {
            return Ok(());
        }
        self.signal(libc::SIGSTOP)?;
        self.resume_on_drop = true;
        // SIGSTOP is asynchronous. Wait until *all* threads stop before reading
        // their children, preventing fork races during recursive capture.
        loop {
            if self.exited()? {
                return Ok(());
            }
            let mut stopped = true;
            let tasks = match fs::read_dir(format!("/proc/{}/task", self.pid)) {
                Ok(tasks) => tasks,
                Err(e) if proc_entry_disappeared(&e) && self.exited()? => return Ok(()),
                Err(e) => return Err(e.into()),
            };
            for task in tasks {
                let task = match task {
                    Ok(task) => task,
                    Err(e) if proc_entry_disappeared(&e) => continue,
                    Err(e) => return Err(e.into()),
                };
                let status = match fs::read_to_string(task.path().join("status")) {
                    Ok(status) => status,
                    Err(e) if proc_entry_disappeared(&e) => continue,
                    Err(e) => return Err(e.into()),
                };
                stopped &= status.lines().any(task_status_line_is_quiescent);
            }
            if stopped {
                return Ok(());
            }
            anyhow::ensure!(Instant::now() < deadline, "暂停终端进程树超时 pid={}", self.pid);
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn has_uninterruptible_task(&self) -> Result<bool> {
        let tasks = match fs::read_dir(format!("/proc/{}/task", self.pid)) {
            Ok(tasks) => tasks,
            Err(e) if proc_entry_disappeared(&e) && self.exited()? => return Ok(false),
            Err(e) => return Err(e.into()),
        };
        for task in tasks {
            let task = match task {
                Ok(task) => task,
                Err(e) if proc_entry_disappeared(&e) => continue,
                Err(e) => return Err(e.into()),
            };
            let status = match fs::read_to_string(task.path().join("status")) {
                Ok(status) => status,
                Err(e) if proc_entry_disappeared(&e) => continue,
                Err(e) => return Err(e.into()),
            };
            if status
                .lines()
                .any(|line| task_status_line_has_state(line, "D"))
            {
                return Ok(true);
            }
        }
        Ok(false)
    }
}

// procfs may report ESRCH (not just ENOENT) when a task exits after open.
// Permission, IO and malformed data errors must still abort reclamation.
fn proc_entry_disappeared(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::NotFound || error.raw_os_error() == Some(libc::ESRCH)
}

fn task_status_line_has_state(line: &str, expected: &str) -> bool {
    line.starts_with("State:") && line.split_whitespace().nth(1) == Some(expected)
}

fn task_status_line_is_quiescent(line: &str) -> bool {
    line.starts_with("State:")
        && matches!(
            line.split_whitespace().nth(1),
            // A task in uninterruptible kernel IO cannot fork. SIGSTOP remains
            // pending and takes effect before it can return to userspace, so it
            // is safe to capture its children without waiting forever for T.
            Some("D" | "T" | "t" | "Z" | "X")
        )
}

impl Drop for Process {
    fn drop(&mut self) {
        // Roll back temporary stops on every error path, including partial capture.
        if self.resume_on_drop {
            let _ = self.signal(libc::SIGCONT);
        }
    }
}

pub(super) struct ProcessTree(Vec<Process>);

impl ProcessTree {
    pub(super) fn capture(roots: &[u32]) -> Result<Self> {
        let mut tree = Self(Vec::new());
        let mut seen = HashSet::new();
        let mut pending: Vec<(u32, Option<u32>)> = roots.iter().map(|pid| (*pid, None)).collect();
        let deadline = Instant::now() + Duration::from_secs(2);
        while let Some((pid, parent)) = pending.pop() {
            if !seen.insert(pid) {
                continue;
            }
            anyhow::ensure!(Instant::now() < deadline, "采集终端进程树超时");
            let Some(mut process) = Process::open(pid)? else {
                continue;
            };
            if let Some(parent) = parent {
                let status = match fs::read_to_string(format!("/proc/{pid}/status")) {
                    Ok(status) => status,
                    Err(e) if proc_entry_disappeared(&e) => continue,
                    Err(e) => return Err(e.into()),
                };
                let actual = status
                    .lines()
                    .find_map(|line| line.strip_prefix("PPid:"))
                    .and_then(|value| value.trim().parse::<u32>().ok());
                // A child that disappeared/reparented is no longer proven owned.
                // pidfd keeps this check tied to the opened identity, even on reuse.
                if process.exited()? || actual != Some(parent) {
                    continue;
                }
            }
            process.freeze(deadline)?;
            if process.exited()? {
                continue;
            }
            // Parents stay stopped and retain their children while we pin each
            // child's identity. Read all threads: a worker thread may fork too.
            let tasks = match fs::read_dir(format!("/proc/{pid}/task")) {
                Ok(tasks) => tasks,
                Err(e) if proc_entry_disappeared(&e) && process.exited()? => continue,
                Err(e) => return Err(e.into()),
            };
            for task in tasks {
                let task = match task {
                    Ok(task) => task,
                    Err(e) if proc_entry_disappeared(&e) => continue,
                    Err(e) => return Err(e.into()),
                };
                let children = match fs::read_to_string(task.path().join("children")) {
                    Ok(children) => children,
                    Err(e) if proc_entry_disappeared(&e) => continue,
                    Err(e) => return Err(e.into()),
                };
                for child in children.split_whitespace() {
                    pending.push((child.parse::<u32>().context("无效子进程 PID")?, Some(pid)));
                }
            }
            tree.0.push(process);
        }
        Ok(tree)
    }

    pub(super) fn terminate(&mut self) -> Result<Vec<u32>> {
        let mut errors = Vec::new();
        let uninterruptible_pids = self
            .0
            .iter()
            .map(|process| Ok((process.pid, process.has_uninterruptible_task()?)))
            .collect::<Result<HashSet<_>>>()?
            .into_iter()
            .filter_map(|(pid, blocked)| blocked.then_some(pid))
            .collect::<HashSet<_>>();
        let has_uninterruptible = !uninterruptible_pids.is_empty();
        // Leaves first; continue after individual signal failures so a single
        // vanished/inaccessible process cannot skip cleanup of the remaining tree.
        for process in self.0.iter_mut().rev() {
            for signal in [libc::SIGTERM, libc::SIGCONT] {
                if let Err(error) = process.signal(signal) {
                    errors.push(format!("{error:#}"));
                }
            }
        }
        self.wait(if has_uninterruptible {
            Duration::from_millis(250)
        } else {
            Duration::from_secs(1)
        })?;
        for process in &self.0 {
            if !process.exited()? {
                if let Err(error) = process.signal(libc::SIGKILL) {
                    errors.push(format!("{error:#}"));
                }
            }
        }
        self.wait(if has_uninterruptible {
            Duration::from_millis(250)
        } else {
            Duration::from_secs(1)
        })?;
        let mut pending = Vec::new();
        for process in &mut self.0 {
            if process.exited()? {
                process.resume_on_drop = false;
            } else if uninterruptible_pids.contains(&process.pid)
                || process.has_uninterruptible_task()?
            {
                // pidfd_send_signal(SIGKILL) succeeded. An uninterruptible task
                // can only exit after its kernel IO returns; retain the pending
                // signal and let the kernel finish reaping it asynchronously.
                process.resume_on_drop = false;
                pending.push(process.pid);
            } else {
                errors.push(format!("终端进程 {} 在 SIGKILL 后仍未退出", process.pid));
            }
        }
        anyhow::ensure!(errors.is_empty(), "{}", errors.join("; "));
        Ok(pending)
    }

    fn wait(&self, budget: Duration) -> Result<()> {
        let deadline = Instant::now() + budget;
        loop {
            let exited = self
                .0
                .iter()
                .map(Process::exited)
                .collect::<Result<Vec<_>>>()?;
            if exited.iter().all(|v| *v) || Instant::now() >= deadline {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::OpenOptions, path::PathBuf};
    use tokio::process::Command;

    struct Fixture {
        id: String,
        dir: PathBuf,
        processes: std::cell::RefCell<Vec<Process>>,
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // Runs even on assertions/timeouts; only the test's isolated server.
            for process in self.processes.get_mut() {
                let _ = process.signal(libc::SIGKILL);
            }
            let _ = super::super::destroy_tmux_session(&self.id);
            let _ = super::super::isolated_tmux_command(&self.id)
                .arg("kill-server")
                .output();
            let _ = fs::remove_dir_all(&self.dir);
        }
    }
    impl Fixture {
        fn new() -> Self {
            let id = format!(
                "reap_test_{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            );
            let dir = std::env::temp_dir().join(&id);
            fs::create_dir(&dir).unwrap();
            Self {
                id,
                dir,
                processes: Default::default(),
            }
        }
        async fn tmux(&self, args: &[&str]) {
            let mut command = Command::from(super::super::isolated_tmux_command(&self.id));
            let output = tokio::time::timeout(
                Duration::from_secs(3),
                command.args(args).kill_on_drop(true).output(),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        }
        async fn pane(&self, index: usize) {
            let lock = self.dir.join(format!("{index}.lock"));
            let ready = self.dir.join(format!("{index}.ready"));
            // Root -> child, both ignoring TERM/HUP. Child retains a real flock.
            let code = format!(
                "import os,signal,fcntl,time\nsignal.signal(signal.SIGTERM,signal.SIG_IGN)\nsignal.signal(signal.SIGHUP,signal.SIG_IGN)\nif os.fork()==0:\n f=open({:?},'w');fcntl.flock(f,fcntl.LOCK_EX);open({:?},'w').write(str(os.getpid())+' '+str(os.getppid()))\nwhile True: time.sleep(1)",
                lock.to_str().unwrap(),
                ready.to_str().unwrap()
            );
            let script = self.dir.join(format!("{index}.py"));
            fs::write(&script, code).unwrap();
            let command = format!("python3 {}", script.display());
            let name = terminal_core::tmux_session_name(&self.id);
            if index == 0 {
                self.tmux(&["new-session", "-d", "-s", &name, &command])
                    .await;
            } else {
                self.tmux(&["new-window", "-d", "-t", &name, &command])
                    .await;
            }
            tokio::time::timeout(Duration::from_secs(3), async {
                while !fs::read_to_string(&ready)
                    .is_ok_and(|text| text.split_whitespace().count() == 2)
                {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .unwrap();
            for pid in fs::read_to_string(&ready).unwrap().split_whitespace() {
                if let Some(process) = Process::open(pid.parse().unwrap()).unwrap() {
                    self.processes.borrow_mut().push(process);
                }
            }
        }
        fn assert_locks_released(&self) {
            for index in 0..2 {
                let path = self.dir.join(format!("{index}.lock"));
                let file = OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(path)
                    .unwrap();
                // SAFETY: valid file descriptor; no pointers.
                assert_eq!(
                    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
                    0
                );
            }
        }
    }

    #[tokio::test]
    async fn terminal_reap_all_panes_releases_locks_and_preserves_unrelated_process() {
        let fixture = Fixture::new();
        let mut unrelated = Command::new("sleep")
            .arg("30")
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        fixture.pane(0).await;
        fixture.pane(1).await;
        // Bounded product operation; outer submitted test command has a hard timeout.
        tokio::time::timeout(Duration::from_secs(8), async {
            super::super::destroy_tmux_session(&fixture.id).unwrap();
        })
        .await
        .unwrap();
        fixture.assert_locks_released();
        assert!(unrelated.try_wait().unwrap().is_none());
        super::super::destroy_tmux_session(&fixture.id).unwrap();
        unrelated.kill().await.unwrap();
    }

    #[tokio::test]
    async fn terminal_reap_recreation_releases_old_writer_locks() {
        let fixture = Fixture::new();
        fixture.pane(0).await;
        fixture.pane(1).await;
        let profile = crate::runtime_paths::resolve_current_user_profile().unwrap();
        tokio::time::timeout(Duration::from_secs(8), async {
            super::super::create_fresh_tmux_session(&fixture.id, &fixture.dir, &profile).unwrap();
        })
        .await
        .unwrap();
        fixture.assert_locks_released();
        assert!(super::super::tmux_session_exists(&fixture.id));
    }

    #[tokio::test]
    async fn terminal_reap_pidfd_remains_bound_after_exit() {
        let mut child = Command::new("sleep")
            .arg("30")
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        let process = Process::open(child.id().unwrap()).unwrap().unwrap();
        tokio::time::timeout(Duration::from_secs(3), child.kill())
            .await
            .unwrap()
            .unwrap();
        assert!(process.exited().unwrap());
        assert!(!process.has_uninterruptible_task().unwrap());
        // An exited descriptor can never signal a later process using this PID.
        process.signal(libc::SIGKILL).unwrap();
    }

    #[test]
    fn terminal_reap_procfs_exit_errors_are_narrowly_classified() {
        assert!(proc_entry_disappeared(&io::Error::from_raw_os_error(libc::ESRCH)));
        assert!(proc_entry_disappeared(&io::Error::from_raw_os_error(libc::ENOENT)));
        for code in [libc::EACCES, libc::EPERM, libc::EIO] {
            assert!(!proc_entry_disappeared(&io::Error::from_raw_os_error(code)));
        }
    }

    #[test]
    fn terminal_reap_rejects_service_and_system_processes() {
        assert!(ProcessTree::capture(&[1]).is_err());
        assert!(ProcessTree::capture(&[std::process::id()]).is_err());
    }

    #[test]
    fn terminal_reap_treats_uninterruptible_io_as_quiescent() {
        assert!(task_status_line_is_quiescent("State:\tD (disk sleep)"));
        assert!(task_status_line_is_quiescent("State:\tT (stopped)"));
        assert!(!task_status_line_is_quiescent("State:\tS (sleeping)"));
        assert!(!task_status_line_is_quiescent("Threads:\t1"));
    }
}
