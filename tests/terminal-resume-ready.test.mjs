import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import vm from "node:vm";

const require = createRequire(import.meta.url);

const readyJs = readFileSync(new URL("../static/terminal-resume-ready.js", import.meta.url), "utf8");
const resumeAgentJs = readFileSync(new URL("../static/terminal-resume-agent.js", import.meta.url), "utf8");
const terminalHtml = readFileSync(new URL("../static/terminal.html", import.meta.url), "utf8");

const context = vm.createContext({ module: { exports: {} }, exports: {} });
vm.runInContext(readyJs, context);
const ready = context.module.exports;

assert.equal(ready.terminalResumeAgentKind("codex resume 0f1e2d3c"), "codex");
assert.equal(ready.terminalResumeAgentKind("claude --resume 1234-abcd"), "claude");
assert.equal(ready.terminalResumeAgentKind("agy --conversation=abc"), "agy");
assert.equal(ready.terminalResumeAgentKind("devin -r my-session-slug"), "devin");
assert.equal(ready.terminalResumeAgentKind("ls -la"), "");

const shellPrompt = "[root@host project]# codex resume 0f1e2d3c\n";

// Codex only paints the "›" composer after the thread is restored. The reply of
// the previous turn may still hold a "› 继续" line, so the fresh marker must be
// counted against the captured baseline instead of matched anywhere.
const codexIdle = [
  "user: 继续",
  "assistant: 已完成。",
  "",
  "› Ask Codex to do anything",
  "",
  "  GPT-6.1-Sol high · Context 45% used · Fast off",
].join("\n");
assert.equal(ready.terminalResumeAgentReady("codex", codexIdle, shellPrompt), true);
assert.equal(ready.terminalResumeAgentReady("codex", codexIdle, codexIdle), false);
assert.equal(
  ready.terminalResumeAgentReady("codex", "› 继续\n\nWorking (1s • esc to interrupt)", "› 继续"),
  false,
);

// A live turn keeps rewriting its status line; "继续" must wait for it.
assert.equal(
  ready.terminalResumeAgentWorking("assistant: ...\n\n• Working (12s • esc to interrupt)\n\n›"),
  true,
);
assert.equal(
  ready.terminalResumeAgentWorking("assistant: ...\n\n✻ Thinking…\n\n›"),
  true,
);
assert.equal(
  ready.terminalResumeAgentWorking("assistant: ...\n\n─ Worked for 7m 28s ─\n\n›"),
  false,
);

// Claude and AGY are recognized by markers that only appear once their TUI is up.
const claudeBanner = "╭─ Claude Code ─╮\n│ GLM-5.3 │\n\n> ";
assert.equal(ready.terminalResumeAgentReady("claude", claudeBanner, shellPrompt), true);
// After the transcript scrolls the banner away, the idle footer still proves the
// TUI took over the pane.
assert.equal(
  ready.terminalResumeAgentReady("claude", "previous turn\n\n> \n────────────────\n? for shortcuts", shellPrompt),
  true,
);
const agyIdle = "────────────────\n> \n────────────────\n? for shortcuts";
assert.equal(ready.terminalResumeAgentReady("agy", agyIdle, shellPrompt), true);
assert.equal(ready.terminalResumeAgentReady("claude", "> ", shellPrompt), false);
assert.equal(ready.terminalResumeAgentReady("devin", "> ", shellPrompt), false);

// A resume that never starts leaves the shell prompt as the newest line.
assert.equal(ready.terminalResumeShellReturned(shellPrompt), true);
assert.equal(ready.terminalResumeShellReturned("[root@host project]#\ncodex: command not found\n[root@host project]# "), true);
assert.equal(ready.terminalResumeShellReturned(codexIdle), false);

// The action must send the resume command, wait for readiness, then use the
// canonical continue endpoint; never send "继续" unconditionally.
assert.match(resumeAgentJs, /function waitForTerminalResumeReady\(/);
assert.match(resumeAgentJs, /await sendTerminalAutoTypedInput\(command, \{ sessionId \}\)/);
assert.match(resumeAgentJs, /function terminalResumeContinueWhenReady\(/);
assert.match(resumeAgentJs, /waitForTerminalResumeReady\(sourceContext, kind, baselineTail\)/);
assert.match(resumeAgentJs, /if \(!ready\)/);
assert.match(resumeAgentJs, /\/api\/terminal\/sessions\/\$\{encodeURIComponent\(sessionId\)\}\/continue/);
assert.match(resumeAgentJs, /readiness\.terminalResumeAgentWorking\(tail\)/);
assert.match(
  terminalHtml,
  /<script defer src="\/assets\/terminal-resume-ready\.js\?v=[^"]+"><\/script>\s*<script defer src="\/assets\/terminal-resume-agent\.js\?v=/,
);

// End-to-end wiring: the real detector gates the continue call. The pane starts
// at a shell prompt; only the injected resume produces the Codex composer, so
// the continue endpoint must be called exactly once and only after that.
const resumeUuid = "12345678-1234-1234-1234-123456789abc";
const shellScreen = `[root@host project]# codex resume ${resumeUuid}`;
const codexReadyScreen = [
  "user: 继续",
  "assistant: 已完成。",
  "",
  "› Ask Codex to do anything",
  "",
  "  GPT-6.1-Sol high · Context 45% used · Fast off",
].join("\n");
function createActionContext(initialTail = shellScreen, command = `codex resume ${resumeUuid}`, readyScreen = codexReadyScreen) {
  const scenario = {
    paneTail: initialTail,
    sentCommands: [],
    continueUrls: [],
  };
  const context = vm.createContext({
    console,
    setInterval,
    clearInterval,
    terminalResumeExtract: require("../static/terminal-resume-extract.js"),
    readTerminalBufferTailText: () => command,
    readTerminalBufferTailTextFrom: () => scenario.paneTail,
    state: { activeSessionId: "s1" },
    activeTerminalContext: { sessionId: "s1", term: {} },
    requestJson: async (url) => {
      scenario.continueUrls.push(url);
      return { ok: true };
    },
    ensureTerminalReadyForInput: () => true,
    mobileKeySendQueue: Promise.resolve(),
    sendTerminalAutoTypedInput: async (command, options) => {
      scenario.sentCommands.push(command);
      scenario.targetSession = options.sessionId;
      // The TUI paints shortly after the bytes are written, not instantly.
      setTimeout(() => {
        scenario.paneTail = readyScreen;
      }, 30);
      return true;
    },
    updateStatus: () => {},
    focusTerminalAfterSoftKeyboardInput: () => {},
  });
  // No CommonJS binding here: the modules attach themselves to the context
  // global, exactly like the deferred <script> tags do in the browser.
  vm.runInContext(readyJs, context);
  vm.runInContext(resumeAgentJs, context);
  return { context, scenario };
}

async function waitForContinue(scenario) {
  const deadline = Date.now() + 5000;
  while (scenario.continueUrls.length === 0 && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}

const single = createActionContext();
single.context.injectLatestResumeCommand();
await single.context.mobileKeySendQueue;
// The readiness wait runs outside the send queue, so wait for the detached
// continue step instead of the queue itself.
await waitForContinue(single.scenario);
assert.deepEqual(single.scenario.sentCommands, [`codex resume ${resumeUuid}`]);
assert.deepEqual(single.scenario.continueUrls, ["/api/terminal/sessions/s1/continue"]);

// Two quick presses queue two resume commands but must produce a single
// follow-up turn: the newer wait supersedes the older one.
const twice = createActionContext();
twice.context.injectLatestResumeCommand();
twice.context.injectLatestResumeCommand();
await twice.context.mobileKeySendQueue;
await waitForContinue(twice.scenario);
assert.equal(twice.scenario.sentCommands.length, 2);
assert.deepEqual(twice.scenario.continueUrls, ["/api/terminal/sessions/s1/continue"]);

// The real incident is CodeBuddy: its command was extractable but had no
// readiness kind, so the old action immediately reported failure at startup.
const cbScreen = "Restored conversation\n────────────────────\n> \n────────────────────\n? for shortcuts";
assert.equal(ready.terminalResumeAgentKind(`codebuddy --resume=${resumeUuid}`), "codebuddy");
assert.equal(ready.terminalResumeAgentReady("codebuddy", cbScreen, shellScreen), true);
assert.equal(ready.terminalResumeAgentReady("codebuddy", "> ", shellScreen), false);
assert.equal(ready.terminalResumeAgentReady("codebuddy", cbScreen, cbScreen), false);
// Alternate-screen redraw can replace old composers instead of incrementing
// their count. A baseline ending at the shell still establishes freshness.
assert.equal(ready.terminalResumeAgentReady("codebuddy", cbScreen, `${cbScreen}\n[root@host project]# `), true);
const cb = createActionContext(shellScreen, `codebuddy --resume=${resumeUuid}`, cbScreen);
cb.context.injectLatestResumeCommand();
// Switching tabs while the send is queued must never send to the new terminal.
cb.context.state.activeSessionId = "other";
await cb.context.mobileKeySendQueue;
assert.equal(cb.scenario.targetSession, "s1");
assert.equal(cb.scenario.continueUrls.length, 0, "do not continue before the native composer exists");
await waitForContinue(cb.scenario);
assert.deepEqual(cb.scenario.continueUrls, ["/api/terminal/sessions/s1/continue"]);

const agyNativeIdle = "⚠ Individual quota reached. Please upgrade your\nsubscription to increase your limits. Resets in 4h54m8s.\n────────────────────────\n>\n────────────────────────\n? for shortcuts       Claude Opus 4.6 (Thinking)";
assert.equal(ready.terminalResumeAgentReady("agy", agyNativeIdle, "[root@host webClx]#"), true);
assert.equal(ready.terminalResumeAgentReady("agy", agyNativeIdle.replace("\n>\n", "\n> 保留草稿\n"), "[root@host webClx]#"), false);
assert.equal(ready.terminalResumeAgentReady("agy", "? for shortcuts", "[root@host webClx]#"), false);
assert.equal(ready.terminalResumeAgentWorking(agyNativeIdle + "\n⣻ Running command... esc to cancel"), true);
