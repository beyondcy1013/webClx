use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    process::Command,
};

#[cfg(not(windows))]
use terminal_core::tmux_session_name;
use tracing::warn;

#[derive(Debug, Clone, Default)]
pub(super) struct TerminalAgentActivity {
    pub(super) agents: Vec<String>,
}

impl TerminalAgentActivity {
    pub(super) fn is_active(&self) -> bool {
        !self.agents.is_empty()
    }

    pub(super) fn label(&self) -> String {
        match self.agents.as_slice() {
            [] => String::new(),
            [agent] => agent.clone(),
            agents => agents.join("/"),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct TerminalAgentDetector {
    #[cfg(not(windows))]
    process_table: HashMap<u32, ProcessInfo>,
    #[cfg(not(windows))]
    pane_pids: HashMap<String, u32>,
}

impl TerminalAgentDetector {
    pub(super) fn new() -> Self {
        Self {
            #[cfg(not(windows))]
            process_table: read_process_table(),
            #[cfg(not(windows))]
            pane_pids: read_tmux_pane_pids(),
        }
    }

    #[cfg(windows)]
    pub(super) fn detect(&self, _session_id: &str) -> TerminalAgentActivity {
        TerminalAgentActivity::default()
    }

    #[cfg(not(windows))]
    pub(super) fn detect(&self, session_id: &str) -> TerminalAgentActivity {
        let Some(root_pid) = self.pane_pids.get(&tmux_session_name(session_id)).copied() else {
            return TerminalAgentActivity::default();
        };

        let descendant_pids = descendant_pids(root_pid, &self.process_table);
        let mut agents = Vec::new();

        for pid in descendant_pids {
            let Some(process) = self.process_table.get(&pid) else {
                continue;
            };
            for agent in agent_names_for_process(process) {
                if !agents.iter().any(|existing| existing == agent) {
                    agents.push(agent.to_string());
                }
            }
        }

        TerminalAgentActivity { agents }
    }
}

#[cfg(not(windows))]
fn read_tmux_pane_pids() -> HashMap<String, u32> {
    let Ok(output) = Command::new("tmux")
        .arg("list-panes")
        .arg("-a")
        .arg("-F")
        .arg("#{session_name}\t#{pane_pid}")
        .output()
    else {
        return HashMap::new();
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("tmux pane pid list failed: {}", stderr.trim());
        return HashMap::new();
    }

    parse_tmux_pane_pids(&output.stdout)
}

#[cfg(not(windows))]
fn parse_tmux_pane_pids(output: &[u8]) -> HashMap<String, u32> {
    String::from_utf8_lossy(output)
        .lines()
        .filter_map(|line| {
            let (session_name, pane_pid) = line.split_once('\t')?;
            Some((session_name.to_string(), pane_pid.parse().ok()?))
        })
        .collect()
}

#[derive(Debug, Clone)]
struct ProcessInfo {
    ppid: u32,
    comm: String,
    cmdline: String,
}

#[cfg(windows)]
fn read_process_table() -> HashMap<u32, ProcessInfo> {
    HashMap::new()
}

#[cfg(not(windows))]
fn read_process_table() -> HashMap<u32, ProcessInfo> {
    let mut table = HashMap::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return table;
    };

    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        let Ok(pid) = name.parse::<u32>() else {
            continue;
        };
        let proc_dir = entry.path();
        let Some(ppid) = read_process_ppid(&proc_dir.join("stat")) else {
            continue;
        };
        let comm = fs::read_to_string(proc_dir.join("comm"))
            .unwrap_or_default()
            .trim()
            .to_string();
        let cmdline = fs::read(proc_dir.join("cmdline"))
            .map(|bytes| normalize_cmdline(&bytes))
            .unwrap_or_default();
        table.insert(
            pid,
            ProcessInfo {
                ppid,
                comm,
                cmdline,
            },
        );
    }

    table
}

#[cfg(not(windows))]
fn read_process_ppid(stat_path: &std::path::Path) -> Option<u32> {
    let stat = fs::read_to_string(stat_path).ok()?;
    let close_paren = stat.rfind(')')?;
    let remainder = stat.get(close_paren + 1..)?.trim_start();
    let mut fields = remainder.split_whitespace();
    let _state = fields.next()?;
    fields.next()?.parse().ok()
}

#[cfg(not(windows))]
fn normalize_cmdline(bytes: &[u8]) -> String {
    bytes
        .split(|byte| *byte == 0)
        .filter(|part| !part.is_empty())
        .map(|part| String::from_utf8_lossy(part))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(windows)]
fn descendant_pids(_root_pid: u32, _process_table: &HashMap<u32, ProcessInfo>) -> Vec<u32> {
    Vec::new()
}

#[cfg(not(windows))]
fn descendant_pids(root_pid: u32, process_table: &HashMap<u32, ProcessInfo>) -> Vec<u32> {
    let mut children_by_parent: HashMap<u32, Vec<u32>> = HashMap::new();
    for (pid, process) in process_table {
        children_by_parent
            .entry(process.ppid)
            .or_default()
            .push(*pid);
    }

    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([root_pid]);

    while let Some(pid) = queue.pop_front() {
        if !seen.insert(pid) {
            continue;
        }
        result.push(pid);
        if let Some(children) = children_by_parent.get(&pid) {
            queue.extend(children.iter().copied());
        }
    }

    result
}

#[cfg(not(windows))]
fn agent_names_for_process(process: &ProcessInfo) -> Vec<&'static str> {
    let comm = process.comm.to_ascii_lowercase();
    let cmdline = process.cmdline.to_ascii_lowercase();
    let mut agents = Vec::new();

    if is_codex_process(&comm, &cmdline) {
        agents.push("Codex");
    }
    if is_claude_process(&comm, &cmdline) {
        agents.push("Claude");
    }
    if is_deepseek_process(&comm, &cmdline) {
        agents.push("DeepSeek");
    }

    agents
}

#[cfg(not(windows))]
fn is_codex_process(comm: &str, cmdline: &str) -> bool {
    comm == "codex"
        || comm == "codex-cli"
        || command_token_matches(cmdline, "codex")
        || cmdline.contains("@openai/codex")
}

#[cfg(not(windows))]
fn is_claude_process(comm: &str, cmdline: &str) -> bool {
    comm == "claude"
        || command_token_matches(cmdline, "claude")
        || cmdline.contains("@anthropic-ai/claude-code")
}

#[cfg(not(windows))]
fn is_deepseek_process(comm: &str, cmdline: &str) -> bool {
    let mut tokens = cmdline.split_whitespace();
    let executable = tokens.next().and_then(|token| token.rsplit('/').next());
    let script = tokens.next().unwrap_or_default();
    comm == "dsh"
        || executable == Some("dsh")
        || (executable == Some("node")
            && (script.rsplit('/').next() == Some("dsh") || script.contains("@deepseek-ai/dsh")))
}

#[cfg(not(windows))]
fn command_token_matches(cmdline: &str, expected: &str) -> bool {
    cmdline
        .split_whitespace()
        .filter_map(|token| token.rsplit('/').next())
        .any(|token| token == expected)
}

#[cfg(test)]
#[cfg(not(windows))]
mod tests {
    use super::*;

    #[test]
    fn parse_proc_stat_ppid_after_comm_with_spaces() {
        let temp_dir =
            std::env::temp_dir().join(format!("webclx-proc-stat-test-{}", std::process::id()));
        let _ = fs::create_dir_all(&temp_dir);
        let stat_path = temp_dir.join("stat");
        fs::write(&stat_path, "123 (name with spaces) S 45 1 1 0").unwrap();

        assert_eq!(read_process_ppid(&stat_path), Some(45));

        let _ = fs::remove_file(stat_path);
        let _ = fs::remove_dir(temp_dir);
    }

    #[test]
    fn command_token_detection_uses_executable_token() {
        assert!(command_token_matches("/usr/local/bin/codex --help", "codex"));
        assert!(!command_token_matches("/home/codes/webClx/build", "codex"));
    }

    #[test]
    fn parses_all_tmux_pane_pids_from_one_listing() {
        let pane_pids = parse_tmux_pane_pids(
            b"webclx_s1\t101\nwebclx_s2\t202\ninvalid\nwebclx_s3\tnot-a-pid\n",
        );

        assert_eq!(pane_pids.get("webclx_s1"), Some(&101));
        assert_eq!(pane_pids.get("webclx_s2"), Some(&202));
        assert_eq!(pane_pids.len(), 2);
    }

    #[test]
    fn agent_detection_recognizes_node_wrappers() {
        let process = ProcessInfo {
            ppid: 1,
            comm: "node".to_string(),
            cmdline: "node /usr/bin/claude @anthropic-ai/claude-code".to_string(),
        };

        assert_eq!(agent_names_for_process(&process), vec!["Claude"]);
    }

    #[test]
    fn agent_detection_recognizes_deepseek_harness_launchers() {
        for process in [
            ProcessInfo {
                ppid: 1,
                comm: "node".to_string(),
                cmdline: "node /usr/local/bin/dsh --profile headless".to_string(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "node".to_string(),
                cmdline: "node /opt/@deepseek-ai/dsh/lib/bin.js web".to_string(),
            },
        ] {
            assert_eq!(agent_names_for_process(&process), vec!["DeepSeek"]);
        }

        let search = ProcessInfo {
            ppid: 1,
            comm: "rg".to_string(),
            cmdline: "rg @deepseek-ai/dsh src".to_string(),
        };
        assert!(agent_names_for_process(&search).is_empty());

        let argument = ProcessInfo {
            ppid: 1,
            comm: "echo".to_string(),
            cmdline: "echo dsh".to_string(),
        };
        assert!(agent_names_for_process(&argument).is_empty());
    }
}
