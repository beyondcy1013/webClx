use std::{
    env,
    io::{self, Write},
    path::Path,
    process::Stdio,
    time::Duration,
};

use anyhow::{Context, Result, bail};
use auth_core::{
    ApiPresetLookup, ApiPresetSelectionEntry, PresetConfigOverride, PresetTerminalEnvVar,
    model_from_config_overrides, select_api_preset_index,
};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::{Client, RequestBuilder};
use serde::Deserialize;
use tokio::process::Command;

use crate::codex_launch::{codex_history_args_with_model, read_codex_model};

const DEFAULT_WEBCLX_URL: &str = "http://127.0.0.1:11111";
#[cfg(unix)]
struct TerminalModeGuard {
    fd: libc::c_int,
    original: Option<libc::termios>,
}

#[cfg(unix)]
impl TerminalModeGuard {
    fn capture_stdin() -> Self {
        Self::capture_fd(libc::STDIN_FILENO)
    }

    fn capture_fd(fd: libc::c_int) -> Self {
        let mut original = unsafe { std::mem::zeroed::<libc::termios>() };
        // A redirected stdin is valid for non-interactive agents and needs no restoration.
        let captured = unsafe { libc::isatty(fd) == 1 && libc::tcgetattr(fd, &mut original) == 0 };
        Self {
            fd,
            original: captured.then_some(original),
        }
    }

    fn restore(&mut self) -> io::Result<()> {
        let Some(original) = self.original.as_ref() else {
            return Ok(());
        };
        if unsafe { libc::tcsetattr(self.fd, libc::TCSANOW, original) } != 0 {
            return Err(io::Error::last_os_error());
        }
        self.original = None;
        Ok(())
    }
}

#[cfg(unix)]
impl Drop for TerminalModeGuard {
    fn drop(&mut self) {
        let _ = self.restore();
    }
}

#[cfg(not(unix))]
struct TerminalModeGuard;

#[cfg(not(unix))]
impl TerminalModeGuard {
    fn capture_stdin() -> Self {
        Self
    }

    fn restore(&mut self) -> io::Result<()> {
        Ok(())
    }
}

const ROOT_HELP: &str = r#"webclx - switch webClx presets from a terminal

Usage:
  webclx <command> [arguments]

Commands:
  webclx list [type]                         List saved presets
  webclx current                             Show active presets
  webclx use <type> <preset-or-model>        Switch to a preset by selector
  webclx run <type> <preset-or-model> [--] <agent>
                                               Launch an agent with a selected preset
  webclx serve                               Start the webClx server
  webclx help [command]                      Show command help
  webclx version                             Show the webClx version

Preset types:
  oauth    Codex OAuth/login preset
  api      Codex API preset
  claude   Claude API preset

Environment:
  WEBCLX_URL    Local webClx URL (default: http://127.0.0.1:11111)

API model selection:
  For type `api`, the selector may be an exact ID, exact preset name, or model.
  A model selects the first matching API preset in the saved table order.

Examples:
  webclx list api
  webclx use api "primary"
  webclx run api "primary" -- codex
  webclx run oauth "plus-account" -- codex resume <session-id>
  webclx run claude "anthropic" -- claude
"#;

const LIST_HELP: &str = r#"Usage:
  webclx list [type]

List all presets, or only oauth, api, or claude presets.
The active preset is marked with `*`.
"#;

const CURRENT_HELP: &str = r#"Usage:
  webclx current

Show the active Codex OAuth/API preset and the active Claude preset.
"#;

const USE_HELP: &str = r#"Usage:
  webclx use <type> <preset-or-model>

Switch the shared configuration to a preset selected by exact name or ID.
For type `api`, an exact model is also accepted and selects its first preset
in the saved table order. Exact ID and preset name take precedence.
Existing agent processes keep their startup configuration.

Example:
  webclx use api "primary"
  webclx use api "gpt-5.6-sol"
"#;

const RUN_HELP: &str = r#"Usage:
  webclx run <type> <preset-or-model> [--] <agent> [agent arguments...]

Apply the selected preset to the real shared configuration, then launch the
agent. The selected preset remains active for subsequent processes. Existing
agent processes do not hot-reload it; restart, resume, or fork a new process.
For type `api`, a model selects its first preset in the saved table order.

Examples:
  webclx run api "primary" -- codex
  webclx run api "primary" -- codex resume <session-id>
  webclx run oauth "plus-account" -- codex
  webclx run claude "anthropic" -- claude --continue
"#;

const SERVE_HELP: &str = r#"Usage:
  webclx serve

Start the webClx HTTP server. The deployed `webClx` service binary also starts
the server when invoked without arguments.
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresetKind {
    OAuth,
    Api,
    Claude,
}

impl PresetKind {
    fn parse(value: &str) -> std::result::Result<Self, String> {
        match value {
            "oauth" | "auth" => Ok(Self::OAuth),
            "api" | "codex-api" => Ok(Self::Api),
            "claude" => Ok(Self::Claude),
            _ => Err(format!("unknown preset type `{value}`; expected oauth, api, or claude")),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::OAuth => "oauth",
            Self::Api => "api",
            Self::Claude => "claude",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::OAuth => "Codex OAuth",
            Self::Api => "Codex API",
            Self::Claude => "Claude API",
        }
    }

    fn endpoint(self) -> &'static str {
        match self {
            Self::OAuth => "/api/auth/presets",
            Self::Api => "/api/auth/api-presets",
            Self::Claude => "/api/auth/claude-presets",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum CliAction {
    Serve,
    Help(Option<String>),
    Version,
    List(Option<PresetKind>),
    Current,
    Use {
        kind: PresetKind,
        selector: String,
    },
    Run {
        kind: PresetKind,
        selector: String,
        agent: String,
        args: Vec<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
struct PresetSummary {
    id: String,
    name: String,
    #[serde(default)]
    active: bool,
    #[serde(default)]
    config_overrides: Vec<PresetConfigOverride>,
    #[serde(default)]
    terminal_env: Vec<PresetTerminalEnvVar>,
}

impl PresetSummary {
    #[cfg(test)]
    fn new(id: &str, name: &str, active: bool) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            active,
            config_overrides: Vec::new(),
            terminal_env: Vec::new(),
        }
    }

    #[cfg(test)]
    fn new_with_model(id: &str, name: &str, active: bool, model: &str) -> Self {
        Self {
            config_overrides: vec![PresetConfigOverride {
                key: Some("model".to_string()),
                value: Some(model.to_string()),
            }],
            ..Self::new(id, name, active)
        }
    }
}

impl ApiPresetSelectionEntry for PresetSummary {
    fn api_preset_id(&self) -> &str {
        &self.id
    }

    fn api_preset_name(&self) -> &str {
        &self.name
    }

    fn api_preset_model(&self) -> Option<&str> {
        model_from_config_overrides(&self.config_overrides)
    }
}

#[derive(Debug, Deserialize)]
struct PresetListResponse {
    #[serde(default)]
    presets: Vec<PresetSummary>,
}

#[derive(Debug, Deserialize)]
struct PresetApplyResponse {
    #[serde(default)]
    deferred: bool,
    #[serde(default)]
    config_file: Option<String>,
    #[serde(default)]
    local_config_file: Option<String>,
}

impl PresetApplyResponse {
    fn codex_model(&self) -> Result<Option<String>> {
        let config_file = self
            .local_config_file
            .as_deref()
            .filter(|path| !path.trim().is_empty())
            .or_else(|| {
                self.config_file
                    .as_deref()
                    .filter(|path| !path.trim().is_empty())
            });
        let Some(config_file) = config_file else {
            return Ok(None);
        };
        read_codex_model(Path::new(config_file))
            .with_context(|| format!("failed to read the applied Codex model from {config_file}"))
    }
}

struct PresetLists {
    oauth: Vec<PresetSummary>,
    api: Vec<PresetSummary>,
    claude: Vec<PresetSummary>,
}

impl PresetLists {
    fn for_kind(&self, kind: PresetKind) -> &[PresetSummary] {
        match kind {
            PresetKind::OAuth => &self.oauth,
            PresetKind::Api => &self.api,
            PresetKind::Claude => &self.claude,
        }
    }
}

#[derive(Clone)]
struct CliClient {
    base_url: String,
    client: Client,
}

impl CliClient {
    fn new() -> Result<Self> {
        let base_url = env::var("WEBCLX_URL")
            .unwrap_or_else(|_| DEFAULT_WEBCLX_URL.to_string())
            .trim_end_matches('/')
            .to_string();
        if base_url.is_empty() {
            bail!("WEBCLX_URL cannot be empty");
        }

        let client = Client::builder()
            .no_proxy()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(120))
            .build()
            .context("failed to create webClx HTTP client")?;
        Ok(Self { base_url, client })
    }

    async fn presets(&self, kind: PresetKind) -> Result<Vec<PresetSummary>> {
        let response: PresetListResponse = self
            .request_json(self.client.get(self.url(kind.endpoint())))
            .await
            .with_context(|| format!("failed to list {} presets", kind.key()))?;
        Ok(response.presets)
    }

    async fn all_presets(&self) -> Result<PresetLists> {
        let (oauth, api, claude) = tokio::try_join!(
            self.presets(PresetKind::OAuth),
            self.presets(PresetKind::Api),
            self.presets(PresetKind::Claude)
        )?;
        Ok(PresetLists { oauth, api, claude })
    }

    async fn apply(
        &self,
        kind: PresetKind,
        preset: &PresetSummary,
        project_path: Option<&Path>,
    ) -> Result<PresetApplyResponse> {
        let preset_id = utf8_percent_encode(&preset.id, NON_ALPHANUMERIC);
        let path = format!("{}/{preset_id}/apply", kind.endpoint());
        let mut request = self.client.put(self.url(&path));
        if let Some(project_path) = project_path {
            request = request.query(&[("project_path", project_path)]);
        }
        self.request_json(request).await.with_context(|| {
            format!("failed to apply {} preset `{}` ({})", kind.key(), preset.name, preset.id)
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    async fn request_json<T>(&self, request: RequestBuilder) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let response = request
            .send()
            .await
            .with_context(|| format!("cannot reach webClx at {}", self.base_url))?;
        let status = response.status();
        let body = response
            .bytes()
            .await
            .context("failed to read webClx response")?;
        if !status.is_success() {
            bail!("webClx API returned {status}: {}", String::from_utf8_lossy(&body).trim());
        }
        serde_json::from_slice(&body).context("webClx returned invalid JSON")
    }
}

pub fn parse_process_args() -> std::result::Result<CliAction, String> {
    let mut args = env::args();
    let program = args.next().unwrap_or_else(|| "webclx".to_string());
    let program_name = Path::new(&program)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("webclx");
    parse_args(program_name, args)
}

fn parse_args<I>(program_name: &str, args: I) -> std::result::Result<CliAction, String>
where
    I: IntoIterator<Item = String>,
{
    let args = args.into_iter().collect::<Vec<_>>();
    if args.is_empty() {
        return if program_name == "webClx" {
            Ok(CliAction::Serve)
        } else {
            Ok(CliAction::Help(None))
        };
    }

    let command = args[0].as_str();
    let rest = &args[1..];
    match command {
        "-h" | "--help" => no_extra_args(command, rest, CliAction::Help(None)),
        "-V" | "--version" | "version" => no_extra_args(command, rest, CliAction::Version),
        "serve" => {
            if first_is_help(rest) {
                Ok(CliAction::Help(Some("serve".to_string())))
            } else {
                no_extra_args(command, rest, CliAction::Serve)
            }
        }
        "help" => match rest {
            [] => Ok(CliAction::Help(None)),
            [topic] => {
                help_text(Some(topic))?;
                Ok(CliAction::Help(Some(topic.clone())))
            }
            _ => Err("usage: webclx help [command]".to_string()),
        },
        "list" | "ls" => {
            if first_is_help(rest) {
                return Ok(CliAction::Help(Some("list".to_string())));
            }
            match rest {
                [] => Ok(CliAction::List(None)),
                [kind] => Ok(CliAction::List(Some(PresetKind::parse(kind)?))),
                _ => Err("usage: webclx list [type]".to_string()),
            }
        }
        "current" | "status" => {
            if first_is_help(rest) {
                Ok(CliAction::Help(Some("current".to_string())))
            } else {
                no_extra_args(command, rest, CliAction::Current)
            }
        }
        "use" => {
            if first_is_help(rest) {
                return Ok(CliAction::Help(Some("use".to_string())));
            }
            match rest {
                [kind, selector] if !selector.trim().is_empty() => Ok(CliAction::Use {
                    kind: PresetKind::parse(kind)?,
                    selector: selector.clone(),
                }),
                _ => Err("usage: webclx use <type> <preset-or-model>".to_string()),
            }
        }
        "run" => {
            if first_is_help(rest) {
                return Ok(CliAction::Help(Some("run".to_string())));
            }
            if rest.len() < 3 {
                return Err(
                    "usage: webclx run <type> <preset-or-model> [--] <agent> [arguments...]"
                        .to_string(),
                );
            }
            let kind = PresetKind::parse(&rest[0])?;
            let selector = rest[1].clone();
            let mut command_start = 2;
            if rest.get(command_start).is_some_and(|value| value == "--") {
                command_start += 1;
            }
            let Some(agent) = rest.get(command_start).filter(|value| !value.is_empty()) else {
                return Err(
                    "usage: webclx run <type> <preset-or-model> [--] <agent> [arguments...]"
                        .to_string(),
                );
            };
            Ok(CliAction::Run {
                kind,
                selector,
                agent: agent.clone(),
                args: rest[command_start + 1..].to_vec(),
            })
        }
        _ => Err(format!("unknown command `{command}`\n\n{ROOT_HELP}")),
    }
}

fn no_extra_args(
    command: &str,
    rest: &[String],
    action: CliAction,
) -> std::result::Result<CliAction, String> {
    if rest.is_empty() {
        Ok(action)
    } else {
        Err(format!("command `{command}` does not accept arguments"))
    }
}

fn first_is_help(args: &[String]) -> bool {
    matches!(args.first().map(String::as_str), Some("-h" | "--help"))
}

fn help_text(command: Option<&str>) -> std::result::Result<&'static str, String> {
    match command {
        None => Ok(ROOT_HELP),
        Some("list" | "ls") => Ok(LIST_HELP),
        Some("current" | "status") => Ok(CURRENT_HELP),
        Some("use") => Ok(USE_HELP),
        Some("run") => Ok(RUN_HELP),
        Some("serve") => Ok(SERVE_HELP),
        Some("help") => Ok("Usage:\n  webclx help [command]\n"),
        Some("version") => Ok("Usage:\n  webclx version\n"),
        Some(command) => Err(format!("unknown help topic `{command}`")),
    }
}

fn resolve_preset(
    presets: &[PresetSummary],
    selector: &str,
) -> std::result::Result<PresetSummary, String> {
    if let Some(preset) = presets.iter().find(|preset| preset.id == selector) {
        return Ok(preset.clone());
    }

    let matches = presets
        .iter()
        .filter(|preset| preset.name == selector)
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [preset] => Ok((*preset).clone()),
        [] => Err(format!("preset `{selector}` was not found")),
        matches => Err(format!(
            "preset name `{selector}` is ambiguous; use one of these IDs: {}",
            matches
                .iter()
                .map(|preset| preset.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn resolve_preset_for_kind(
    kind: PresetKind,
    presets: &[PresetSummary],
    selector: &str,
) -> std::result::Result<PresetSummary, String> {
    if kind == PresetKind::Api {
        let index = select_api_preset_index(presets, ApiPresetLookup::Auto(selector))
            .map_err(|error| error.to_string())?;
        return Ok(presets[index].clone());
    }
    resolve_preset(presets, selector)
}

fn active_preset(presets: &[PresetSummary]) -> Option<&PresetSummary> {
    presets.iter().find(|preset| preset.active)
}

pub async fn execute(action: CliAction) -> Result<()> {
    match action {
        CliAction::Serve => bail!("internal error: serve action reached CLI executor"),
        CliAction::Help(command) => {
            print!("{}", help_text(command.as_deref()).map_err(anyhow::Error::msg)?);
            Ok(())
        }
        CliAction::Version => {
            println!("webclx {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
        CliAction::List(kind) => list_presets(kind).await,
        CliAction::Current => show_current().await,
        CliAction::Use { kind, selector } => use_preset(kind, &selector).await,
        CliAction::Run {
            kind,
            selector,
            agent,
            args,
        } => run_agent(kind, &selector, &agent, &args).await,
    }
}

async fn list_presets(kind: Option<PresetKind>) -> Result<()> {
    let client = CliClient::new()?;
    if let Some(kind) = kind {
        let presets = client.presets(kind).await?;
        print_preset_list(kind, &presets);
        return Ok(());
    }

    let lists = client.all_presets().await?;
    for kind in [PresetKind::OAuth, PresetKind::Api, PresetKind::Claude] {
        print_preset_list(kind, lists.for_kind(kind));
    }
    Ok(())
}

fn print_preset_list(kind: PresetKind, presets: &[PresetSummary]) {
    println!("{} presets:", kind.title());
    if presets.is_empty() {
        println!("  (none)");
    } else {
        for preset in presets {
            let marker = if preset.active { '*' } else { ' ' };
            println!("{marker} {}  [{}]", preset.name, preset.id);
        }
    }
    println!();
}

async fn show_current() -> Result<()> {
    let client = CliClient::new()?;
    let lists = client.all_presets().await?;
    println!("Active presets:");
    for kind in [PresetKind::OAuth, PresetKind::Api, PresetKind::Claude] {
        match active_preset(lists.for_kind(kind)) {
            Some(preset) => println!("  {:<7} {}  [{}]", kind.key(), preset.name, preset.id),
            None => println!("  {:<7} (none)", kind.key()),
        }
    }
    Ok(())
}

async fn use_preset(kind: PresetKind, selector: &str) -> Result<()> {
    let client = CliClient::new()?;
    let presets = client.presets(kind).await?;
    let preset = resolve_preset_for_kind(kind, &presets, selector).map_err(anyhow::Error::msg)?;
    let cwd = env::current_dir().context("failed to resolve current directory")?;
    let response = client.apply(kind, &preset, Some(&cwd)).await?;
    if response.deferred {
        println!(
            "Queued {} preset `{}` [{}]. It will be written after the current serialized preset operation completes.",
            kind.title(),
            preset.name,
            preset.id
        );
    } else {
        println!(
            "Switched {} to `{}` [{}]. New agent processes will use this preset.",
            kind.title(),
            preset.name,
            preset.id
        );
    }
    Ok(())
}

async fn run_agent(kind: PresetKind, selector: &str, agent: &str, args: &[String]) -> Result<()> {
    let client = CliClient::new()?;
    let lists = client.all_presets().await?;
    let target = resolve_preset_for_kind(kind, lists.for_kind(kind), selector)
        .map_err(anyhow::Error::msg)?;

    #[cfg(unix)]
    let _interrupt_signal =
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
            .context("failed to install terminal interrupt handler")?;
    #[cfg(unix)]
    let _quit_signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::quit())
        .context("failed to install terminal quit handler")?;

    let cwd = env::current_dir().context("failed to resolve current directory")?;
    let mut applied = client.apply(kind, &target, Some(&cwd)).await?;
    let mut deferred_notice_printed = false;
    while applied.deferred {
        if !deferred_notice_printed {
            println!(
                "{} preset `{}` [{}] is queued; waiting for the current serialized preset operation to finish before launching `{agent}`...",
                kind.title(),
                target.name,
                target.id
            );
            io::stdout().flush().ok();
            deferred_notice_printed = true;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
        applied = client.apply(kind, &target, Some(&cwd)).await?;
    }
    let codex_model = applied.codex_model()?;
    println!(
        "Launching `{agent}` with {} preset `{}` [{}]; the selected preset remains active for subsequent processes.",
        kind.title(),
        target.name,
        target.id
    );
    io::stdout().flush().ok();

    let mut terminal_mode = TerminalModeGuard::capture_stdin();

    let launch_args = codex_history_args_with_model(agent, args, codex_model.as_deref());
    let mut command = Command::new(agent);
    command.args(&launch_args);
    for (key, _) in env::vars_os() {
        if key
            .to_str()
            .is_some_and(auth_core::is_forbidden_config_home_env_key)
        {
            command.env_remove(key);
        }
    }
    let mut child = command
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("failed to launch agent `{agent}`"))?;
    let status = child
        .wait()
        .await
        .with_context(|| format!("failed to wait for agent `{agent}`"))?;
    let terminal_restore = terminal_mode.restore();
    if let Err(error) = terminal_restore {
        eprintln!("Warning: failed to restore terminal mode: {error}");
    }

    if !status.success() {
        bail!("agent `{agent}` exited with status {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deployed_service_name_without_arguments_starts_server() {
        assert_eq!(parse_args("webClx", Vec::<String>::new()), Ok(CliAction::Serve));
    }

    #[test]
    fn cli_name_without_arguments_prints_help() {
        assert_eq!(parse_args("webclx", Vec::<String>::new()), Ok(CliAction::Help(None)));
    }

    #[test]
    fn parses_preset_commands_and_preserves_agent_arguments() {
        assert_eq!(
            parse_args("webclx", ["list", "api"].map(String::from)),
            Ok(CliAction::List(Some(PresetKind::Api)))
        );
        assert_eq!(
            parse_args("webclx", ["use", "oauth", "main account"].map(String::from)),
            Ok(CliAction::Use {
                kind: PresetKind::OAuth,
                selector: "main account".to_string(),
            })
        );
        assert_eq!(
            parse_args(
                "webclx",
                ["run", "api", "fast", "--", "codex", "resume", "thread-id"].map(String::from)
            ),
            Ok(CliAction::Run {
                kind: PresetKind::Api,
                selector: "fast".to_string(),
                agent: "codex".to_string(),
                args: vec!["resume".to_string(), "thread-id".to_string()],
            })
        );
        assert_eq!(
            parse_args(
                "webclx",
                ["run", "claude", "work", "claude", "--continue"].map(String::from)
            ),
            Ok(CliAction::Run {
                kind: PresetKind::Claude,
                selector: "work".to_string(),
                agent: "claude".to_string(),
                args: vec!["--continue".to_string()],
            })
        );
    }

    #[test]
    fn parses_root_and_subcommand_help() {
        assert_eq!(parse_args("webclx", ["--help"].map(String::from)), Ok(CliAction::Help(None)));
        assert_eq!(
            parse_args("webclx", ["help", "run"].map(String::from)),
            Ok(CliAction::Help(Some("run".to_string())))
        );
        assert_eq!(
            parse_args("webclx", ["run", "--help"].map(String::from)),
            Ok(CliAction::Help(Some("run".to_string())))
        );
    }

    #[test]
    fn rejects_missing_or_unknown_arguments() {
        assert!(parse_args("webclx", ["use", "api"].map(String::from)).is_err());
        assert!(parse_args("webclx", ["list", "other"].map(String::from)).is_err());
        assert!(parse_args("webclx", ["unknown"].map(String::from)).is_err());
    }

    #[test]
    fn resolves_exact_id_or_unique_name_and_rejects_duplicates() {
        let presets = vec![
            PresetSummary::new("api-1", "same", false),
            PresetSummary::new("api-2", "same", true),
            PresetSummary::new("api-3", "other", false),
        ];

        assert_eq!(resolve_preset(&presets, "api-1").unwrap().id, "api-1");
        assert_eq!(resolve_preset(&presets, "other").unwrap().id, "api-3");
        assert!(resolve_preset(&presets, "same").is_err());
        assert!(resolve_preset(&presets, "missing").is_err());
    }

    #[test]
    fn api_model_selector_uses_first_saved_match_after_id_and_name() {
        let presets = vec![
            PresetSummary::new_with_model("api-1", "primary", false, "gpt-5.6-sol"),
            PresetSummary::new_with_model("api-2", "backup", false, "gpt-5.6-sol"),
            PresetSummary::new_with_model("api-3", "gpt-5.6-sol", false, "other-model"),
        ];

        assert_eq!(
            resolve_preset_for_kind(PresetKind::Api, &presets, "GPT-5.6-SOL")
                .unwrap()
                .id,
            "api-1"
        );
        assert_eq!(
            resolve_preset_for_kind(PresetKind::Api, &presets, "gpt-5.6-sol")
                .unwrap()
                .id,
            "api-3",
            "an exact preset name must win over model fallback"
        );
        assert!(
            resolve_preset_for_kind(PresetKind::OAuth, &presets, "GPT-5.6-SOL").is_err(),
            "model fallback is only valid for Codex API presets"
        );
    }

    #[cfg(unix)]
    #[test]
    fn run_restores_inherited_terminal_mode() {
        let mut master = -1;
        let mut slave = -1;
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null(),
                )
            },
            0
        );

        let mut original = unsafe { std::mem::zeroed::<libc::termios>() };
        assert_eq!(unsafe { libc::tcgetattr(slave, &mut original) }, 0);
        let mut terminal_mode = TerminalModeGuard::capture_fd(slave);

        let mut raw = original;
        unsafe { libc::cfmakeraw(&mut raw) };
        assert_eq!(unsafe { libc::tcsetattr(slave, libc::TCSANOW, &raw) }, 0);

        terminal_mode.restore().unwrap();
        let mut restored = unsafe { std::mem::zeroed::<libc::termios>() };
        assert_eq!(unsafe { libc::tcgetattr(slave, &mut restored) }, 0);
        assert_eq!(restored.c_iflag, original.c_iflag);
        assert_eq!(restored.c_oflag, original.c_oflag);
        assert_eq!(restored.c_cflag, original.c_cflag);
        assert_eq!(restored.c_lflag, original.c_lflag);
        assert_eq!(restored.c_cc, original.c_cc);

        unsafe {
            libc::close(slave);
            libc::close(master);
        }
    }

    #[test]
    fn help_documents_switch_run_and_environment_usage() {
        let root = help_text(None).unwrap();
        assert!(root.contains("webclx use <type> <preset-or-model>"));
        assert!(root.contains("webclx run <type> <preset-or-model>"));
        assert!(root.contains("WEBCLX_URL"));

        let run = help_text(Some("run")).unwrap();
        assert!(run.contains("webclx run api"));
        assert!(run.contains("codex"));
    }
}
