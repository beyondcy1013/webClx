import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const terminalTmuxRs = readFileSync(new URL("../src/terminal/tmux.rs", import.meta.url), "utf8");

assert.match(
  terminalTmuxRs,
  /const TMUX_TERMINAL_OVERRIDES:\s*&str\s*=\s*"xterm-256color:indn@:rin@";[\s\S]*fn configure_tmux_session\(session_id: &str\) -> Result<\(\)> \{[\s\S]*"history-limit"[\s\S]*TMUX_HISTORY_LIMIT[\s\S]*"status"[\s\S]*"off"[\s\S]*"terminal-overrides"[\s\S]*TMUX_TERMINAL_OVERRIDES[\s\S]*"focus-events"[\s\S]*"on"[\s\S]*"无法开启 tmux focus-events"/,
  "tmux sessions should keep focus-events enabled and disable indn/rin so browser xterm scrollback does not lose lines during normal multi-line output",
);
