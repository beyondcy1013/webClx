// Resume readiness detection for the Ctrl+Alt+R "extract id and resume" action.
// The resume command is injected asynchronously: the shell must start the agent,
// the agent must restore the thread and paint its input box. Typing "继续" before
// that point either lands in the shell (if the command failed) or in a still
// repainting TUI. Everything here is pure text analysis over the captured tail;
// the caller owns the polling loop and timers.
(function attachTerminalResumeReady(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
    return;
  }

  root.WebClxTerminalResumeReady = factory();
})(typeof globalThis !== "undefined" ? globalThis : this, function createTerminalResumeReady() {
  const RESUME_SCAN_LINES = 80;
  const RESUME_READY_SETTLE_MS = 900;
  const RESUME_READY_TIMEOUT_MS = 30000;
  const RESUME_READY_POLL_MS = 120;

  // The resume command already names the agent, so the detector never has to
  // guess from the pane whether the resumed session is Codex, Claude or AGY.
  function terminalResumeAgentKind(command) {
    const normalized = String(command || "").trim().toLowerCase();
    if (!normalized) return "";
    if (/\bcodex\b/.test(normalized)) return "codex";
    if (/\bclaude\b/.test(normalized)) return "claude";
    if (/\bcodebuddy\b/.test(normalized)) return "codebuddy";
    if (/\bagy\b/.test(normalized)) return "agy";
    if (/\bdevin\b/.test(normalized)) return "devin";
    return "";
  }

  function resumeTailLines(tail) {
    return String(tail || "").split("\n");
  }

  function resumeTailLastLines(tail, count = 8) {
    const lines = resumeTailLines(tail).filter((line) => line.trim().length > 0);
    return lines.slice(Math.max(0, lines.length - Math.max(1, count)));
  }

  // Codex idle input box is the only reliable ready marker that survives a
  // restore: its placeholder/footer change per version, the "›" composer is
  // stable and only drawn after the thread is loaded.
  function isCodexComposerLine(line) {
    return /^\s*\u203a(?:\s|$)/u.test(String(line || ""));
  }

  // Claude Code and AGY both keep a "? for shortcuts" footer while idle, and
  // fall back to their "Claude Code" banner. Devin has no stable banner, so it
  // is never auto-continued rather than guessed at.
  function terminalResumeMarkerCount(kind, tail) {
    const lines = resumeTailLines(tail);
    if (kind === "codex") {
      return lines.filter(isCodexComposerLine).length;
    }
    if (kind === "claude") {
      // The startup banner scrolls out of the restored transcript, so the always
      // visible "? for shortcuts" footer is the marker that actually survives.
      return lines.filter((line) => line.includes("Claude Code") || line.includes("? for shortcuts")).length;
    }
    if (kind === "codebuddy" || kind === "agy") {
      return lines.filter((line, index) => /^\s*>\s*$/u.test(line)
        && lines.slice(Math.max(0, index - 3), index).some(isRuleLine)
        && lines.slice(index + 1, index + 4).some(isRuleLine)).length;
    }
    return 0;
  }

  function isRuleLine(line) {
    return /^\s*[─━═-]{10,}\s*$/u.test(line);
  }

  // Only text that appeared after the resume command counts as a fresh marker:
  // the scrollback may still hold the previous agent UI from before the session
  // was interrupted.
  function terminalResumeAgentReady(kind, tail, baselineTail = "") {
    if (!kind) return false;
    const current = terminalResumeMarkerCount(kind, tail);
    if (current === 0) return false;
    return tail !== baselineTail && (current > terminalResumeMarkerCount(kind, baselineTail)
      || terminalResumeShellReturned(baselineTail));
  }

  // A live turn rewrites its status line every second; any of these in the tail
  // means the agent is not sitting at an idle composer yet.
  function terminalResumeAgentWorking(tail) {
    return resumeTailLastLines(tail, 8).some((line) => {
      const value = String(line || "");
      if (/esc to interrupt\)/i.test(value)) return true;
      if (/esc to cancel/i.test(value)) return true;
      const trimmed = value.trim().replace(/^[^\p{L}\p{N}]+/u, "");
      return /^(?:Thinking|Working|Generating|Running|Executing|Loading|Reconnecting)\b[^\n]*(?:\.\.\.|…)/iu.test(trimmed);
    });
  }

  // Used only as a fail-fast: a resume that never started leaves the shell
  // prompt as the newest line, so the caller can stop waiting instead of
  // burning the whole timeout.
  function terminalResumeShellReturned(tail) {
    const lines = resumeTailLines(tail).filter((line) => line.trim().length > 0);
    const last = String(lines.at(-1) || "").trim();
    if (!last) return false;
    return /^\[[^\]]*\]\s*[#$]\s*/u.test(last)
      || /^[^\s@]+@[^\s:]+:[^\s]*[#$]\s*/u.test(last)
      || /^[#$>]\s*$/u.test(last);
  }

  return Object.freeze({
    RESUME_SCAN_LINES,
    RESUME_READY_SETTLE_MS,
    RESUME_READY_TIMEOUT_MS,
    RESUME_READY_POLL_MS,
    terminalResumeAgentKind,
    terminalResumeAgentReady,
    terminalResumeAgentWorking,
    terminalResumeShellReturned,
  });
});
