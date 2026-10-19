use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
};

#[cfg(not(windows))]
use tracing::warn;

use super::tmux::isolated_tmux_command;

#[derive(Debug, Clone, Default)]
pub(super) struct TerminalAgentActivity {
    pub(super) agents: Vec<String>,
    pub(super) orphaned_agents: Vec<String>,
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

    pub(super) fn has_orphaned_agents(&self) -> bool {
        !self.orphaned_agents.is_empty()
    }
}

pub(super) fn terminal_compact_command_blocked_agent<'a>(
    command: &str,
    activity: &'a TerminalAgentActivity,
) -> Option<&'a str> {
    if command.split_whitespace().next() != Some("/compact") {
        return None;
    }
    activity.agents.iter().find_map(|agent| {
        ["CodeBuddy", "AGY", "Antigravity", "Cline"]
            .iter()
            .any(|blocked| agent.eq_ignore_ascii_case(blocked))
            .then_some(agent.as_str())
    })
}

#[derive(Debug, Clone)]
pub(super) struct TerminalAgentDetector {
    #[cfg(not(windows))]
    process_table: HashMap<u32, ProcessInfo>,
}

impl TerminalAgentDetector {
    pub(super) fn new() -> Self {
        Self {
            #[cfg(not(windows))]
            process_table: read_process_table(),
        }
    }

    #[cfg(windows)]
    pub(super) fn detect(&self, _session_id: &str) -> TerminalAgentActivity {
        TerminalAgentActivity::default()
    }

    #[cfg(windows)]
    pub(super) fn api_preset_id(&self, _session_id: &str) -> Option<String> {
        None
    }

    #[cfg(not(windows))]
    pub(super) fn detect(&self, session_id: &str) -> TerminalAgentActivity {
        let Some(root_pid) = read_tmux_pane_pid(session_id) else {
            return TerminalAgentActivity::default();
        };

        let scope_pids = tmux_scope_process_ids(session_id, &self.process_table);
        detect_agent_activity_from_process_sets(root_pid, &scope_pids, &self.process_table)
    }

    #[cfg(not(windows))]
    pub(super) fn api_preset_id(&self, _session_id: &str) -> Option<String> {
        // Native Agent processes no longer carry a webClx run wrapper, so
        // their preset identity cannot be inferred from process arguments.
        None
    }
}

#[cfg(not(windows))]
fn read_tmux_pane_pid(session_id: &str) -> Option<u32> {
    let mut command = isolated_tmux_command(session_id);
    command.args(["display-message", "-p", "-F", "#{pane_pid}"]);
    let result = super::tmux::command_output::bounded_command_output(
        command,
        super::tmux::TMUX_COMMAND_TIMEOUT,
        64 * 1024,
    );
    let output = match result {
        Ok(output) => output,
        Err(error) => {
            warn!("tmux pane pid lookup failed for {session_id}: {error:#}");
            return None;
        }
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        warn!("tmux pane pid lookup failed: {}", stderr.trim());
        return None;
    }

    parse_tmux_pane_pid(&output.stdout)
}

#[cfg(not(windows))]
fn parse_tmux_pane_pid(output: &[u8]) -> Option<u32> {
    String::from_utf8_lossy(output)
        .lines()
        .find_map(|line| line.trim().parse().ok())
}

#[derive(Debug, Clone)]
struct ProcessInfo {
    ppid: u32,
    comm: String,
    cmdline: String,
    cgroup: String,
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
        let cgroup = fs::read_to_string(proc_dir.join("cgroup")).unwrap_or_default();
        table.insert(
            pid,
            ProcessInfo {
                ppid,
                comm,
                cmdline,
                cgroup,
            },
        );
    }

    table
}

#[cfg(not(windows))]
fn tmux_scope_process_ids(session_id: &str, process_table: &HashMap<u32, ProcessInfo>) -> Vec<u32> {
    let scope_name = format!("webclx-tmux-{session_id}.scope");
    process_table
        .iter()
        .filter_map(|(pid, process)| process.cgroup.contains(&scope_name).then_some(*pid))
        .collect()
}

#[cfg(not(windows))]
fn detect_agent_activity_from_process_sets(
    root_pid: u32,
    scope_pids: &[u32],
    process_table: &HashMap<u32, ProcessInfo>,
) -> TerminalAgentActivity {
    let descendant_pids = descendant_pids(root_pid, process_table);
    let descendant_set = descendant_pids.iter().copied().collect::<HashSet<_>>();
    let mut candidate_pids = descendant_pids;
    candidate_pids.extend(scope_pids.iter().copied());
    candidate_pids.sort_unstable();
    candidate_pids.dedup();

    let mut agents = Vec::new();
    let mut orphaned_agents = Vec::new();
    for pid in candidate_pids {
        let Some(process) = process_table.get(&pid) else {
            continue;
        };
        for agent in agent_names_for_process(process) {
            if !agents.iter().any(|existing| existing == agent) {
                agents.push(agent.to_string());
            }
            if !descendant_set.contains(&pid)
                && !orphaned_agents.iter().any(|existing| existing == agent)
            {
                orphaned_agents.push(agent.to_string());
            }
        }
    }

    TerminalAgentActivity {
        agents,
        orphaned_agents,
    }
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

    if is_cdx_process(&comm, &cmdline) {
        agents.push("CDX");
    } else if is_codex_process(&comm, &cmdline) {
        agents.push("Codex");
    }
    if is_claude_process(&comm, &cmdline) {
        agents.push("Claude");
    }
    if is_deepseek_process(&comm, &cmdline) {
        agents.push("DeepSeek");
    }
    if is_agy_process(&comm, &cmdline) {
        agents.push("AGY");
    }
    if is_codebuddy_process(&comm, &cmdline) {
        agents.push("CodeBuddy");
    }
    if is_grok_process(&comm, &cmdline) {
        agents.push("Grok");
    }
    if is_devin_process(&comm, &cmdline) {
        agents.push("Devin");
    }
    if is_cline_process(&comm, &cmdline) {
        agents.push("Cline");
    }
    if is_opencode_process(&comm, &cmdline) {
        agents.push("OpenCode");
    }

    agents
}

#[cfg(not(windows))]
fn is_cdx_process(comm: &str, cmdline: &str) -> bool {
    let mut tokens = cmdline.split_whitespace();
    let executable = tokens.next().and_then(|token| token.rsplit('/').next());
    let script = tokens.next().unwrap_or_default();
    let script_name = script.rsplit('/').next().unwrap_or_default();
    comm == "cdx"
        || comm == "cdx-cli"
        || comm == "cdx-sub2api-cli"
        || comm.starts_with("cdx-sub2api")
        || executable == Some("cdx")
        || executable == Some("cdx-cli")
        || executable == Some("cdx-sub2api-cli")
        || executable
            .map(|e| e.starts_with("cdx-sub2api"))
            .unwrap_or(false)
        || (matches!(executable, Some("bash") | Some("sh"))
            && (script_name == "cdx" || script.contains("/bin/cdx")))
}

#[cfg(not(windows))]
fn is_codex_process(comm: &str, cmdline: &str) -> bool {
    comm == "codex"
        || comm == "codex-cli"
        || executable_matches(cmdline, "codex")
        || cmdline.contains("@openai/codex")
}

#[cfg(not(windows))]
fn is_claude_process(comm: &str, cmdline: &str) -> bool {
    comm == "claude"
        || executable_matches(cmdline, "claude")
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
fn is_agy_process(comm: &str, cmdline: &str) -> bool {
    let mut tokens = cmdline.split_whitespace();
    let executable = tokens.next().and_then(|token| token.rsplit('/').next());
    let script = tokens.next().unwrap_or_default();
    comm == "agy"
        || comm == "antigravity"
        || executable == Some("agy")
        || executable == Some("antigravity")
        || (executable == Some("node")
            && (script.rsplit('/').next() == Some("agy")
                || script.rsplit('/').next() == Some("antigravity")
                || script.contains("antigravity-cli")))
}

#[cfg(not(windows))]
fn is_codebuddy_process(comm: &str, cmdline: &str) -> bool {
    let mut tokens = cmdline.split_whitespace();
    let executable = tokens.next().and_then(|token| token.rsplit('/').next());
    let script = tokens.next().unwrap_or_default();
    comm == "codebuddy"
        || executable == Some("codebuddy")
        || (executable == Some("node")
            && (script.rsplit('/').next() == Some("codebuddy")
                || script.contains("@tencent-ai/codebuddy-code")))
}

#[cfg(not(windows))]
fn is_grok_executable_name(name: &str) -> bool {
    name == "grok"
        || name.starts_with("grok-linux-")
        || name.starts_with("grok-darwin-")
        || name.starts_with("grok-macos-")
}

#[cfg(not(windows))]
fn is_grok_process(comm: &str, cmdline: &str) -> bool {
    if is_grok_executable_name(comm) {
        return true;
    }
    let mut tokens = cmdline.split_whitespace();
    let executable = tokens.next().unwrap_or_default();
    let executable_name = executable.rsplit('/').next().unwrap_or_default();
    let script = tokens.next().unwrap_or_default();
    let script_name = script.rsplit('/').next().unwrap_or_default();
    if is_grok_executable_name(executable_name) {
        return true;
    }
    if executable_name == "node" && is_grok_executable_name(script_name) {
        return true;
    }
    // The official CLI also installs `~/.grok/bin/agent`. A bare `agent`
    // command is not specific enough to label as Grok.
    executable_name == "agent" && executable.contains("/.grok/")
}

#[cfg(not(windows))]
fn is_devin_process(comm: &str, cmdline: &str) -> bool {
    // `/usr/local/bin/devin` on this host is a Windsurf symlink. comm is the
    // resolved executable name, so only a process whose own name is devin counts.
    if comm == "windsurf" {
        return false;
    }
    comm == "devin" || (comm.is_empty() && executable_matches(cmdline, "devin"))
}

#[cfg(not(windows))]
fn is_cline_process(comm: &str, cmdline: &str) -> bool {
    let mut tokens = cmdline.split_whitespace();
    let executable = tokens.next().and_then(|token| token.rsplit('/').next());
    let script = tokens.next().unwrap_or_default();
    comm == "cline"
        || executable == Some("cline")
        || (executable == Some("node")
            && (script.rsplit('/').next() == Some("cline")
                || script.contains("node_modules/cline/")
                || script.contains("node_modules/@cline/")))
}

#[cfg(not(windows))]
fn is_opencode_process(comm: &str, cmdline: &str) -> bool {
    let mut tokens = cmdline.split_whitespace();
    let executable = tokens.next().and_then(|token| token.rsplit('/').next());
    let script = tokens.next().unwrap_or_default();
    comm == "opencode"
        || executable == Some("opencode")
        || (matches!(executable, Some("node") | Some("bun"))
            && (script.rsplit('/').next() == Some("opencode") || script.contains("opencode-ai")))
}

#[cfg(not(windows))]
fn executable_matches(cmdline: &str, expected: &str) -> bool {
    let mut tokens = cmdline.split_whitespace();
    let executable = tokens.next().and_then(|token| token.rsplit('/').next());
    executable == Some(expected)
}

#[cfg(test)]
#[cfg(not(windows))]
mod tests {
    use super::*;

    #[test]
    fn terminal_compact_agent_guard_blocks_incompatible_agents_and_command_arguments() {
        for agent in [
            "CodeBuddy",
            "codebuddy",
            "AGY",
            "agy",
            "Antigravity",
            "Cline",
        ] {
            let activity = TerminalAgentActivity {
                agents: vec!["Codex".to_string(), agent.to_string()],
                orphaned_agents: Vec::new(),
            };
            for command in ["/compact", " /compact\r\n", "/compact focus on recent work"] {
                assert_eq!(terminal_compact_command_blocked_agent(command, &activity), Some(agent));
            }
            for command in ["/status", "/compactness", "echo /compact", "继续"] {
                assert!(terminal_compact_command_blocked_agent(command, &activity).is_none());
            }
        }
        for agent in ["Codex", "Claude", ""] {
            let activity = TerminalAgentActivity {
                agents: vec![agent.to_string()],
                orphaned_agents: Vec::new(),
            };
            assert!(terminal_compact_command_blocked_agent("/compact", &activity).is_none());
        }
    }

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
        assert!(executable_matches("/usr/local/bin/codex --help", "codex"));
        assert!(!executable_matches("/home/codes/tools/webClx/build", "codex"));
    }

    #[test]
    fn parses_tmux_pane_pid_from_isolated_server() {
        assert_eq!(parse_tmux_pane_pid(b"101\n"), Some(101));
        assert_eq!(parse_tmux_pane_pid(b"not-a-pid\n"), None);
    }

    #[test]
    fn agent_detection_recognizes_node_wrappers() {
        let process = ProcessInfo {
            ppid: 1,
            comm: "node".to_string(),
            cmdline: "node /usr/bin/claude @anthropic-ai/claude-code".to_string(),
            cgroup: String::new(),
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
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "node".to_string(),
                cmdline: "node /opt/@deepseek-ai/dsh/lib/bin.js web".to_string(),
                cgroup: String::new(),
            },
        ] {
            assert_eq!(agent_names_for_process(&process), vec!["DeepSeek"]);
        }

        let search = ProcessInfo {
            ppid: 1,
            comm: "rg".to_string(),
            cmdline: "rg @deepseek-ai/dsh src".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&search).is_empty());

        let argument = ProcessInfo {
            ppid: 1,
            comm: "echo".to_string(),
            cmdline: "echo dsh".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&argument).is_empty());
    }

    #[test]
    fn agent_detection_recognizes_agy_launchers() {
        for process in [
            ProcessInfo {
                ppid: 1,
                comm: "agy".to_string(),
                cmdline: "agy".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "agy".to_string(),
                cmdline: "/home/root/.local/bin/agy --mode plan".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "antigravity".to_string(),
                cmdline: "antigravity".to_string(),
                cgroup: String::new(),
            },
        ] {
            assert_eq!(agent_names_for_process(&process), vec!["AGY"]);
        }

        let search = ProcessInfo {
            ppid: 1,
            comm: "rg".to_string(),
            cmdline: "rg agy src".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&search).is_empty());
    }

    #[test]
    fn agent_detection_recognizes_codebuddy_launchers() {
        for process in [
            ProcessInfo {
                ppid: 1,
                comm: "codebuddy".to_string(),
                cmdline: "codebuddy".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "node".to_string(),
                cmdline: "node /usr/local/bin/codebuddy".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "node".to_string(),
                cmdline: "node /opt/@tencent-ai/codebuddy-code/bin/codebuddy --resume id"
                    .to_string(),
                cgroup: String::new(),
            },
        ] {
            assert_eq!(agent_names_for_process(&process), vec!["CodeBuddy"]);
        }

        let search = ProcessInfo {
            ppid: 1,
            comm: "rg".to_string(),
            cmdline: "rg codebuddy src".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&search).is_empty());

        let argument = ProcessInfo {
            ppid: 1,
            comm: "echo".to_string(),
            cmdline: "echo codebuddy".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&search).is_empty());
        assert!(agent_names_for_process(&argument).is_empty());
    }

    #[test]
    fn agent_detection_recognizes_grok_launchers() {
        for process in [
            ProcessInfo {
                ppid: 1,
                comm: "grok".to_string(),
                cmdline: "grok".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "grok".to_string(),
                cmdline: "/home/root/.grok/bin/grok --resume 01a0c6b9-27b8-7281-ba83-a6e9f444c547"
                    .to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "grok-linux-x86_".to_string(),
                cmdline: "/home/root/.grok/downloads/grok-linux-x86_64".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "node".to_string(),
                cmdline: "node /usr/local/bin/grok".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "agent".to_string(),
                cmdline: "/home/root/.grok/bin/agent".to_string(),
                cgroup: String::new(),
            },
        ] {
            assert_eq!(agent_names_for_process(&process), vec!["Grok"]);
        }

        let search = ProcessInfo {
            ppid: 1,
            comm: "rg".to_string(),
            cmdline: "rg grok src".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&search).is_empty());

        let argument = ProcessInfo {
            ppid: 1,
            comm: "echo".to_string(),
            cmdline: "echo grok".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&argument).is_empty());

        let inhibit = ProcessInfo {
            ppid: 1,
            comm: "systemd-inhibit".to_string(),
            cmdline:
                "systemd-inhibit --what=idle --who=grok --why=agent turn in progress sleep infinity"
                    .to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&inhibit).is_empty());

        let bare_agent = ProcessInfo {
            ppid: 1,
            comm: "agent".to_string(),
            cmdline: "agent".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&bare_agent).is_empty());

        let codex_on_grok_model = ProcessInfo {
            ppid: 1,
            comm: "codex".to_string(),
            cmdline: "/usr/local/lib/node_modules/@openai/codex/vendor/codex resume --model grok-4.7 01a0c444"
                .to_string(),
            cgroup: String::new(),
        };
        assert_eq!(agent_names_for_process(&codex_on_grok_model), vec!["Codex"]);
    }

    #[test]
    fn agent_detection_recognizes_devin_launchers() {
        for process in [
            ProcessInfo {
                ppid: 1,
                comm: "devin".to_string(),
                cmdline: "devin".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "devin".to_string(),
                cmdline: "/home/root/.local/bin/devin".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "devin".to_string(),
                cmdline: "/home/root/.local/share/devin/cli/_versions/current/bin/devin"
                    .to_string(),
                cgroup: String::new(),
            },
        ] {
            assert_eq!(agent_names_for_process(&process), vec!["Devin"]);
        }

        let search = ProcessInfo {
            ppid: 1,
            comm: "rg".to_string(),
            cmdline: "rg devin src".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&search).is_empty());

        let argument = ProcessInfo {
            ppid: 1,
            comm: "echo".to_string(),
            cmdline: "echo devin".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&argument).is_empty());

        let windsurf_symlink = ProcessInfo {
            ppid: 1,
            comm: "windsurf".to_string(),
            cmdline: "/usr/local/bin/devin".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&windsurf_symlink).is_empty());
    }

    #[test]
    fn agent_detection_recognizes_cline_launchers() {
        for process in [
            ProcessInfo {
                ppid: 1,
                comm: "cline".to_string(),
                cmdline: "cline".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "cline".to_string(),
                cmdline: "/home/root/.local/bin/cline --id abc123".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "node".to_string(),
                cmdline: "node /usr/local/bin/cline".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "node".to_string(),
                cmdline: "node /usr/local/lib/node_modules/cline/bin/cline.js".to_string(),
                cgroup: String::new(),
            },
        ] {
            assert_eq!(agent_names_for_process(&process), vec!["Cline"]);
        }

        let search = ProcessInfo {
            ppid: 1,
            comm: "rg".to_string(),
            cmdline: "rg cline src".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&search).is_empty());

        let argument = ProcessInfo {
            ppid: 1,
            comm: "echo".to_string(),
            cmdline: "echo cline".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&argument).is_empty());
    }

    #[test]
    fn agent_detection_recognizes_opencode_launchers() {
        for process in [
            ProcessInfo {
                ppid: 1,
                comm: "opencode".to_string(),
                cmdline: "opencode".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "opencode".to_string(),
                cmdline: "/home/root/.opencode/bin/opencode --session ses_abc".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "node".to_string(),
                cmdline: "node /usr/local/bin/opencode".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "bun".to_string(),
                cmdline: "bun /usr/local/lib/node_modules/opencode-ai/bin/opencode".to_string(),
                cgroup: String::new(),
            },
        ] {
            assert_eq!(agent_names_for_process(&process), vec!["OpenCode"]);
        }

        let search = ProcessInfo {
            ppid: 1,
            comm: "rg".to_string(),
            cmdline: "rg opencode src".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&search).is_empty());

        let argument = ProcessInfo {
            ppid: 1,
            comm: "echo".to_string(),
            cmdline: "echo opencode".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&argument).is_empty());
    }

    #[test]
    fn agent_detection_recognizes_cdx_launchers() {
        for process in [
            ProcessInfo {
                ppid: 1,
                comm: "cdx".to_string(),
                cmdline: "cdx".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "bash".to_string(),
                cmdline: "bash /usr/local/bin/cdx".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "cdx-sub2api-cli".to_string(),
                cmdline: "/usr/local/bin/cdx-sub2api-cli -c model=gpt-6".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "cdx-sub2api-cl".to_string(),
                cmdline: "/usr/local/bin/cdx-sub2api-cli".to_string(),
                cgroup: String::new(),
            },
            ProcessInfo {
                ppid: 1,
                comm: "cdx-cli".to_string(),
                cmdline: "/usr/local/bin/cdx-cli resume 1234".to_string(),
                cgroup: String::new(),
            },
        ] {
            assert_eq!(agent_names_for_process(&process), vec!["CDX"]);
        }

        let search = ProcessInfo {
            ppid: 1,
            comm: "rg".to_string(),
            cmdline: "rg cdx src".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&search).is_empty());

        let argument = ProcessInfo {
            ppid: 1,
            comm: "echo".to_string(),
            cmdline: "echo cdx".to_string(),
            cgroup: String::new(),
        };
        assert!(agent_names_for_process(&argument).is_empty());
    }

    #[test]
    fn scope_agent_detection_distinguishes_orphaned_agents_from_shell_descendants() {
        let process_table = HashMap::from([
            (
                10,
                ProcessInfo {
                    ppid: 9,
                    comm: "bash".to_string(),
                    cmdline: "bash -l".to_string(),
                    cgroup: String::new(),
                },
            ),
            (
                11,
                ProcessInfo {
                    ppid: 10,
                    comm: "node".to_string(),
                    cmdline: "node /usr/local/bin/codex".to_string(),
                    cgroup: String::new(),
                },
            ),
            (
                12,
                ProcessInfo {
                    ppid: 11,
                    comm: "codex".to_string(),
                    cmdline: "codex fork source-id".to_string(),
                    cgroup: String::new(),
                },
            ),
            (
                20,
                ProcessInfo {
                    ppid: 1,
                    comm: "codex".to_string(),
                    cmdline: "codex fork orphan-id".to_string(),
                    cgroup: String::new(),
                },
            ),
        ]);

        let healthy = detect_agent_activity_from_process_sets(10, &[10, 11, 12], &process_table);
        assert_eq!(healthy.agents, vec!["Codex"]);
        assert!(healthy.orphaned_agents.is_empty());

        let orphaned = detect_agent_activity_from_process_sets(10, &[10, 20], &process_table);
        assert_eq!(orphaned.agents, vec!["Codex"]);
        assert_eq!(orphaned.orphaned_agents, vec!["Codex"]);
    }
}
