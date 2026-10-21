import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const html = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");
const tools = readFileSync(new URL("../static/terminal-tools.js", import.meta.url), "utf8");
const routes = readFileSync(new URL("../src/routes/terminal.rs", import.meta.url), "utf8");
const manager = readFileSync(new URL("../src/terminal/manager.rs", import.meta.url), "utf8");

assert.match(html, /id="terminal-interrupt-resume"[\s\S]*?>中断并恢复<\/button>/);
assert.match(
  tools,
  /forceInterruptAndResumeTerminalAgent[\s\S]*window\.confirm[\s\S]*\/interrupt-and-resume[\s\S]*method: "POST"/,
);
assert.match(routes, /sessions\/\{session_id\}\/interrupt-and-resume[\s\S]*force_interrupt_and_resume_session/);
assert.match(
  manager,
  /current_resume_session_complete[\s\S]*current_resume_agent_process_ids[\s\S]*libc::SIGINT[\s\S]*send_session_input_silent/,
);
assert.doesNotMatch(tools, /\bkill\b|\bSIGINT\b/, "the browser must not construct process signals");

console.log("terminal interrupt-and-resume contract tests passed");
