// webClx terminal resume, visible text, agent session, and shortcut helpers.
// Extracted from terminal.js as global function declarations.
// Contains no top-level setup.

function showTerminalSwitchPlaceholder(text = "正在打开终端…") {
  if (!terminalHost) {
    return;
  }

  const snapshotText = String(text || "正在打开终端…").trimEnd();

  if (!terminalSwitchPlaceholderEl) {
    terminalSwitchPlaceholderEl = document.createElement("pre");
    terminalSwitchPlaceholderEl.className = "terminal-switch-placeholder";
    terminalSwitchPlaceholderEl.setAttribute("aria-hidden", "true");
    terminalHost.appendChild(terminalSwitchPlaceholderEl);
  }

  terminalSwitchPlaceholderEl.textContent = snapshotText;
  terminalHost.classList.add("terminal-host-switching");
}

function hideTerminalSwitchPlaceholder() {
  terminalSwitchPlaceholderEl?.remove();
  terminalSwitchPlaceholderEl = null;
  terminalHost?.classList.remove("terminal-host-switching");
}

function prepareFreshTerminalDisplay(session) {
  if (session?.id) {
    disposeTerminalSessionContext(session.id);
  }
  state.hasConnectedOnce = true;
}

function openTerminalVisibleTextCopyWindow() {
  const text = readTerminalVisibleText();
  if (!text) {
    updateStatus("当前窗口没有可复制的终端文本。", "muted");
    return false;
  }

  const copyWindow = window.open("", "_blank");
  if (!copyWindow) {
    updateStatus("浏览器阻止了新窗口，请允许弹窗后再试。", "warn");
    return false;
  }
  copyWindow.opener = null;

  copyWindow.document.title = "终端文本复制";
  copyWindow.document.body.innerHTML = "";

  const style = copyWindow.document.createElement("style");
  style.textContent = `
    html, body { margin: 0; min-height: 100%; background: #f7faf8; color: #10231c; }
    body { box-sizing: border-box; padding: 12px; font: 14px/1.5 system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    textarea { box-sizing: border-box; width: 100%; min-height: calc(100vh - 24px); padding: 12px; border: 1px solid #b7c9c0; border-radius: 6px; background: #ffffff; color: #10231c; font: 13px/1.45 ui-monospace, SFMono-Regular, Consolas, monospace; resize: vertical; white-space: pre; }
  `;
  copyWindow.document.head.appendChild(style);

  const textarea = copyWindow.document.createElement("textarea");
  textarea.readOnly = true;
  textarea.value = text;
  textarea.setAttribute("aria-label", "当前终端窗口文本");
  copyWindow.document.body.appendChild(textarea);
  textarea.focus();
  textarea.select();

  updateStatus("已打开新窗口，可在新窗口中手动复制当前终端文本。", "ok");
  return true;
}

function sanitizeResumeToken(rawValue) {
  if (terminalResumeExtract?.sanitizeResumeToken) {
    return terminalResumeExtract.sanitizeResumeToken(rawValue);
  }

  return String(rawValue || "").replace(/^[`'"]+|[`'".,!?，。；：:;)\]}]+$/g, "");
}

function isValidResumeId(resumeId) {
  if (terminalResumeExtract?.isValidResumeId) {
    return terminalResumeExtract.isValidResumeId(resumeId);
  }

  return CODEX_RESUME_ID_PATTERN.test(String(resumeId || ""));
}

function resumeCommandFromId(resumeId, program) {
  if (terminalResumeExtract?.resumeCommandFromId) {
    return terminalResumeExtract.resumeCommandFromId(resumeId, program);
  }

  if (program === "claude") {
    return `claude --resume ${resumeId}`;
  }
  return `codex resume ${resumeId}`;
}

function extractLatestResumeInfo(bufferText) {
  if (terminalResumeExtract?.extractLatestResumeInfo) {
    return terminalResumeExtract.extractLatestResumeInfo(bufferText);
  }

  const text = String(bufferText || "");
  if (!text) {
    return { id: "", program: "codex" };
  }

  let latestId = "";
  let latestProgram = "codex";
  let latestIndex = -1;

  CODEX_RESUME_COMMAND_PATTERN.lastIndex = 0;
  for (const match of text.matchAll(CODEX_RESUME_COMMAND_PATTERN)) {
    const resumeId = sanitizeResumeToken(match[1]);
    if (isValidResumeId(resumeId) && match.index > latestIndex) {
      latestId = resumeId;
      latestProgram = "codex";
      latestIndex = match.index;
    }
  }

  CLAUDE_RESUME_COMMAND_PATTERN.lastIndex = 0;
  for (const match of text.matchAll(CLAUDE_RESUME_COMMAND_PATTERN)) {
    const resumeId = sanitizeResumeToken(match[1]);
    if (isValidResumeId(resumeId) && match.index > latestIndex) {
      latestId = resumeId;
      latestProgram = "claude";
      latestIndex = match.index;
    }
  }

  if (!latestId) {
    const bannerText = text;
    BANNER_SESSION_LABEL_PATTERN.lastIndex = 0;
    for (const match of bannerText.matchAll(BANNER_SESSION_LABEL_PATTERN)) {
      const resumeId = match[1].toLowerCase();
      if (CODEX_RESUME_UUID_PATTERN.test(resumeId) && match.index > latestIndex) {
        latestId = resumeId;
        latestProgram = "codex";
        latestIndex = match.index;
      }
    }
    CLAUDE_BANNER_SESSION_LABEL_PATTERN.lastIndex = 0;
    for (const match of bannerText.matchAll(CLAUDE_BANNER_SESSION_LABEL_PATTERN)) {
      const resumeId = match[1].toLowerCase();
      if (CODEX_RESUME_UUID_PATTERN.test(resumeId) && match.index > latestIndex) {
        latestId = resumeId;
        latestProgram = "claude";
        latestIndex = match.index;
      }
    }
  }

  return { id: latestId, program: latestProgram };
}

function extractLatestResumeId(bufferText) {
  if (terminalResumeExtract?.extractLatestResumeId) {
    return terminalResumeExtract.extractLatestResumeId(bufferText);
  }

  return extractLatestResumeInfo(bufferText).id;
}

function parseResumeIdInput(rawValue) {
  if (terminalResumeExtract?.parseResumeIdInput) {
    return terminalResumeExtract.parseResumeIdInput(rawValue);
  }

  return parseResumeInputInfo(rawValue).id;
}

function parseResumeInputInfo(rawValue) {
  if (terminalResumeExtract?.parseResumeInputInfo) {
    return terminalResumeExtract.parseResumeInputInfo(rawValue);
  }

  const text = String(rawValue || "").trim();
  if (!text) {
    return { id: "", program: "codex", command: "" };
  }

  const commandInfo = extractLatestResumeInfo(text);
  if (commandInfo.id) {
    return {
      ...commandInfo,
      command: resumeCommandFromId(commandInfo.id, commandInfo.program),
    };
  }

  const resumeId = sanitizeResumeToken(text);
  if (!isValidResumeId(resumeId)) {
    return { id: "", program: "codex", command: "" };
  }

  return {
    id: resumeId,
    program: "codex",
    command: resumeCommandFromId(resumeId, "codex"),
  };
}

function extractLatestResumeCommand(bufferText) {
  if (terminalResumeExtract?.extractLatestResumeCommand) {
    return terminalResumeExtract.extractLatestResumeCommand(bufferText);
  }

  const { id, program } = extractLatestResumeInfo(bufferText);
  return id ? resumeCommandFromId(id, program) : "";
}

function injectLatestResumeCommand() {
  const command = extractLatestResumeCommand(readTerminalBufferTailText());
  if (!command) {
    updateStatus("没找到最近的 resume 恢复命令。", "warn");
    return;
  }

  if (!ensureTerminalReadyForInput()) {
    return;
  }

  mobileKeySendQueue = mobileKeySendQueue
    .catch(() => {})
    .then(async () => {
      // WebClx 自动注入的 resume 命令 → 走 auto-typed 通道，避免污染
      // "本终端对话历史" 面板。服务端的 `build_terminal_quick_command_input`
      // 已经在串尾自带 `\n`，这里不需要额外的 Enter。
      await sendTerminalAutoTypedInput(command);
    });
  updateStatus(`已发送：${command} 并执行。`, "ok");
  focusTerminalAfterSoftKeyboardInput();
}
function showCopyResumeOverlay(value, { successMessage = `已复制：${value}` } = {}) {
  // Remove any existing overlay
  const existing = document.getElementById("terminal-copy-resume-overlay");
  if (existing) {
    existing.remove();
  }

  const overlay = document.createElement("div");
  overlay.id = "terminal-copy-resume-overlay";
  overlay.style.cssText = "position:fixed;inset:0;background:rgba(0,0,0,0.45);z-index:9999;display:flex;align-items:center;justify-content:center;padding:16px;";

  const card = document.createElement("div");
  card.style.cssText = "background:var(--panel-bg,#1e1e2e);border:1px solid var(--border,#333);border-radius:12px;padding:16px;max-width:420px;width:100%;box-shadow:0 8px 32px rgba(0,0,0,0.4);";

  const heading = document.createElement("div");
  heading.textContent = "复制 resume ID";
  heading.style.cssText = "font-size:15px;font-weight:600;margin-bottom:10px;color:var(--fg,#cdd6f4);";

  const textarea = document.createElement("textarea");
  textarea.value = value;
  textarea.readOnly = true;
  textarea.style.cssText = "width:100%;min-height:48px;padding:10px;border:1px solid var(--border,#444);border-radius:8px;background:var(--input-bg,#181825);color:var(--fg,#cdd6f4);font-family:monospace;font-size:13px;resize:vertical;box-sizing:border-box;";

  const btnRow = document.createElement("div");
  btnRow.style.cssText = "display:flex;gap:8px;margin-top:10px;justify-content:flex-end;";

  const copyBtn = document.createElement("button");
  copyBtn.textContent = "复制";
  copyBtn.style.cssText = "padding:6px 16px;border:none;border-radius:8px;background:var(--accent,#89b4fa);color:#1e1e2e;font-size:13px;font-weight:600;cursor:pointer;";
  copyBtn.addEventListener("click", async () => {
    const copied = await copyTextToClipboard(value);
    updateStatus(copied ? successMessage : "复制失败，请手动复制。", copied ? "ok" : "warn");
    if (copied) {
      overlay.remove();
    }
  });

  const closeBtn = document.createElement("button");
  closeBtn.textContent = "关闭";
  closeBtn.style.cssText = "padding:6px 16px;border:1px solid var(--border,#444);border-radius:8px;background:transparent;color:var(--fg,#cdd6f4);font-size:13px;cursor:pointer;";
  closeBtn.addEventListener("click", () => overlay.remove());

  btnRow.appendChild(copyBtn);
  btnRow.appendChild(closeBtn);

  card.appendChild(heading);
  card.appendChild(textarea);
  card.appendChild(btnRow);
  overlay.appendChild(card);

  overlay.addEventListener("click", (e) => {
    if (e.target === overlay) {
      overlay.remove();
    }
  });

  document.body.appendChild(overlay);
  textarea.focus();
  textarea.select();
}

function copyLatestResumeId() {
  const resumeId = extractLatestResumeId(readTerminalBufferTailText());
  if (!resumeId) {
    updateStatus("没找到最近的 resume 会话 ID。", "warn");
    return;
  }

  showCopyResumeOverlay(resumeId, { successMessage: `已复制终端文本中的 Session ID：${resumeId}` });
}

async function copyCurrentAgentResumeId() {
  updateStatus("正在识别当前会话…", "info");
  try {
    const { resumeId, command } = await detectCurrentAgentResumeId();
    if (!resumeId) {
      updateStatus("没找到当前会话 ID。", "warn");
      return;
    }

    showCopyResumeOverlay(command || resumeCommandFromId(resumeId));
    updateStatus(`已找到当前会话：${resumeId}。`, "ok");
  } catch (error) {
    updateStatus(error?.message || "读取当前会话失败。", "warn");
  } finally {
    focusTerminalAfterTransientControl();
  }
}

async function copyCurrentTerminalName() {
  const session = activeSession();
  const terminalName = String(session?.name || "").trim();
  if (!terminalName) {
    updateStatus("当前没有可复制的终端名。", "warn");
    return;
  }

  try {
    const copied = await copyTextToClipboard(terminalName);
    if (copied) {
      updateStatus(`已复制终端名：${terminalName}`, "ok");
    } else {
      updateStatus("复制终端名失败。", "warn");
    }
  } catch (error) {
    updateStatus(error?.message || "复制终端名失败。", "warn");
  } finally {
    focusTerminalAfterTransientControl();
  }
}

async function copyCurrentSessionIdAndAsk() {
  const sourceSessionId = state.activeSessionId;
  const sourceContext = activeTerminalContext;
  if (!sourceSessionId || sourceContext?.sessionId !== sourceSessionId) {
    updateStatus("当前终端尚未连接，无法提取 Session ID。", "warn");
    return;
  }

  updateStatus("正在识别当前会话…", "info");
  try {
    const detected = await detectAgentResumeIdComplete(sourceSessionId, sourceContext);
    if (!detected?.resumeId) {
      updateStatus("没找到当前会话 ID。", "warn");
      return;
    }

    const prompt = `调用codex对话数据库skill读取session id为 ${detected.resumeId}并回答我的问题 `;
    const copied = await copyTextToClipboard(prompt);

    updateStatus(
      copied
        ? `已复制 Session 提问文字：${detected.resumeId}`
        : `复制 Session 提问文字失败：${detected.resumeId}`,
      copied ? "ok" : "warn",
    );
  } catch (error) {
    updateStatus(error?.message || "提取当前会话失败。", "warn");
  } finally {
    if (state.activeSessionId === sourceSessionId) {
      focusTerminalAfterTransientControl();
    }
  }
}

// 高级复制：执行统一检测链（屏幕 → /status → 后端完整探测），
// 提取到 ID 后复制到剪贴板并 toast。返回检测到的会话信息，供恢复会话等命令复用。
async function extractCurrentAgentSessionId() {
  updateStatus("正在识别当前会话…", "info");
  try {
    const detected = await detectCurrentAgentResumeId();
    if (!detected?.resumeId) {
      updateStatus("没找到当前会话 ID。", "warn");
      return null;
    }

    const sessionId = detected.resumeId;
    const copied = await copyTextToClipboard(sessionId);
    if (copied) {
      updateStatus(`已提取并复制 Session ID：${sessionId}`, "ok");
    } else {
      updateStatus(`已提取 Session ID：${sessionId}（复制失败，请手动复制）`, "warn");
    }
    return detected;
  } catch (error) {
    updateStatus(error?.message || "提取当前会话失败。", "warn");
    return null;
  } finally {
    focusTerminalAfterTransientControl();
  }
}

async function resumeCurrentAgentSession() {
  // 复用高级复制的检测逻辑，拿到 ID 后再发送 resume 命令。
  const detected = await extractCurrentAgentSessionId();
  if (!detected?.resumeId) {
    return;
  }

  if (!ensureTerminalReadyForInput()) {
    return;
  }

  const command = detected.command || resumeCommandFromId(detected.resumeId, detected.program);
  mobileKeySendQueue = mobileKeySendQueue
    .catch(() => {})
    .then(async () => {
      const sent = await sendTerminalAutoTypedInput(command);
      if (sent) {
        updateStatus(`已按当前预设恢复：${command}`, "ok");
      } else {
        updateStatus("恢复命令发送失败。", "warn");
      }
    });
  focusTerminalSoon();
}


function terminalShortcutKeyName(event) {
  const key = String(event.key || "").trim();
  if (!key || ["Control", "Shift", "Alt", "Meta"].includes(key)) {
    return "";
  }
  if (key === " ") {
    return "Space";
  }
  if (key.length === 1) {
    return key.toUpperCase();
  }
  const aliases = new Map([
    ["Esc", "Escape"],
    ["Del", "Delete"],
    ["ArrowUp", "Up"],
    ["ArrowDown", "Down"],
    ["ArrowLeft", "Left"],
    ["ArrowRight", "Right"],
  ]);
  return aliases.get(key) || key;
}

function shortcutFromKeyboardEvent(event) {
  const key = terminalShortcutKeyName(event);
  if (!key) {
    return "";
  }
  return [
    event.ctrlKey ? "Ctrl" : "",
    event.shiftKey ? "Shift" : "",
    event.altKey ? "Alt" : "",
    event.metaKey ? "Meta" : "",
    key,
  ]
    .filter(Boolean)
    .join("+");
}

function normalizeTerminalShortcutText(shortcut) {
  const trimmed = normalizeTerminalQuickText(shortcut, 80);
  if (!trimmed) {
    return "";
  }
  const parts = trimmed
    .split("+")
    .map((part) => part.trim())
    .filter(Boolean);
  if (!parts.length) {
    return "";
  }

  const modifiers = new Set();
  let key = "";
  parts.forEach((part) => {
    const lower = part.toLowerCase();
    if (lower === "control" || lower === "ctrl") {
      modifiers.add("Ctrl");
    } else if (lower === "shift") {
      modifiers.add("Shift");
    } else if (lower === "alt" || lower === "option") {
      modifiers.add("Alt");
    } else if (lower === "meta" || lower === "cmd" || lower === "command") {
      modifiers.add("Meta");
    } else {
      key = part.length === 1 ? part.toUpperCase() : part;
    }
  });

  if (!key) {
    return "";
  }

  return ["Ctrl", "Shift", "Alt", "Meta"].filter((modifier) => modifiers.has(modifier)).concat(key).join("+");
}

function shouldHandleTerminalFunctionShortcut(event) {
  if (
    event.defaultPrevented ||
    !shortcutFromKeyboardEvent(event)
  ) {
    return false;
  }

  if (!state.activeSessionId || terminalPasteDialogEl?.open || state.renamingSessionId) {
    return false;
  }

  const target = event.target;
  if (target instanceof Node && term.element?.contains(target)) {
    return true;
  }

  if (target instanceof Element && isEditableElement(target)) {
    return false;
  }

  const activeElement = document.activeElement;
  if (activeElement instanceof Node && term.element?.contains(activeElement)) {
    return true;
  }

  if (activeElement instanceof Element && isEditableElement(activeElement)) {
    return false;
  }

  return true;
}

function findTerminalFunctionCommandByShortcut(event) {
  const eventShortcut = normalizeTerminalShortcutText(shortcutFromKeyboardEvent(event)).toLowerCase();
  if (!eventShortcut) {
    return null;
  }
  const projectOption = Array.from(terminalProjectCommandSelectEl?.options || []).find(
    (option) =>
      option.value
      && normalizeTerminalShortcutText(option.dataset.shortcut).toLowerCase() === eventShortcut,
  );
  if (projectOption) {
    return {
      key: projectOption.value,
      label: projectOption.textContent.trim(),
      action: projectOption.value,
      command: "",
      shortcut: projectOption.dataset.shortcut,
    };
  }
  const commands = state.terminalSlashCommands.concat(state.terminalFunctionCommands);
  return (
    commands.find(
      (command) => normalizeTerminalShortcutText(command.shortcut).toLowerCase() === eventShortcut,
    ) || null
  );
}

function handleTerminalFunctionShortcut(event) {
  if (!shouldHandleTerminalFunctionShortcut(event)) {
    return;
  }

  const command = findTerminalFunctionCommandByShortcut(event);
  if (!command) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();
  runTerminalFunctionCommand(command);
}

function shortResumeId(resumeId) {
  const normalized = String(resumeId || "");
  return normalized.length > 12 ? `${normalized.slice(0, 8)}...${normalized.slice(-4)}` : normalized;
}

function formatArchiveTimestamp(timestamp = Date.now()) {
  const date = new Date(timestamp);
  const pad = (value) => String(value).padStart(2, "0");
  return `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function defaultResumeArchiveNote(resumeId) {
  const session = activeSession();
  const baseName = session?.title || session?.name || `Codex ${shortResumeId(resumeId)}`;
  return `${baseName} ${formatArchiveTimestamp()}`;
}

async function loadResumeArchives() {
  state.loadingResumeArchives = true;
  try {
    const response = await requestJson("/api/terminal/resume-archives");
    state.resumeArchives = Array.isArray(response.archives) ? response.archives : [];
  } catch (error) {
    state.resumeArchives = [];
    updateStatus(error.message || "读取 Codex 归档失败。", "warn");
  } finally {
    state.loadingResumeArchives = false;
  }
}

function detectedResumeInfo(info, source = "terminal_buffer") {
  if (!info?.id) {
    return null;
  }
  const program = info.program || "codex";
  return {
    resumeId: info.id,
    command: info.command || resumeCommandFromId(info.id, program),
    program,
    source,
  };
}

const TERMINAL_STATUS_SESSION_TIMEOUT_MS = 6000;
const TERMINAL_STATUS_SESSION_SCAN_MAX_LINES = 80;

async function probeTerminalStatusResumeId(sourceContext, initialBufferText) {
  const context = sourceContext || {};
  const targetSessionId = String(context.sessionId || "").trim();
  if (!context.term || !targetSessionId) {
    return null;
  }
  const sent = await runTerminalSlashCommandByKey("status", { sessionId: targetSessionId });
  if (!sent) {
    return null;
  }
  try {
    const rendered = await waitForTerminalToolResumeCommand(
      context,
      "",
      TERMINAL_STATUS_SESSION_TIMEOUT_MS,
      {
        allowBaseline: true,
        initialBufferText,
        maxLines: TERMINAL_STATUS_SESSION_SCAN_MAX_LINES,
      },
    );
    return detectedResumeInfo(parseResumeInputInfo(rendered), "terminal_status");
  } catch {
    const finalText = readTerminalBufferTailTextFrom(
      context.term,
      TERMINAL_STATUS_SESSION_SCAN_MAX_LINES,
    );
    if (finalText === initialBufferText) {
      return null;
    }
    return detectedResumeInfo(extractLatestResumeInfo(finalText), "terminal_status");
  }
}

async function detectAgentResumeIdForSession(sessionId, { complete = false } = {}) {
  const targetSessionId = String(sessionId || "").trim();
  if (!targetSessionId) {
    return { resumeId: "", command: "", program: "codex", source: "manual" };
  }
  const response = await requestJson(
    `/api/terminal/sessions/${encodeURIComponent(targetSessionId)}/agent-session${complete ? "/complete" : ""}`,
  );
  const parsed = parseResumeInputInfo(response.command || response.resume_id || "");
  return {
    resumeId: parsed.id,
    command: parsed.command,
    program: parsed.program,
    source: response.source || "process_fd",
  };
}

async function detectAgentResumeIdComplete(sessionId, sourceContext = null) {
  const targetSessionId = String(sessionId || "").trim();
  const context = sourceContext?.sessionId === targetSessionId
    ? sourceContext
    : ensureTerminalSessionCache().get(targetSessionId) || null;

  if (context?.term) {
    const initialBufferText = readTerminalBufferTailTextFrom(
      context.term,
      TERMINAL_STATUS_SESSION_SCAN_MAX_LINES,
    );
    const screenInfo = detectedResumeInfo(
      extractLatestResumeInfo(initialBufferText),
      "terminal_buffer",
    );
    if (screenInfo) {
      return screenInfo;
    }

    const statusInfo = await probeTerminalStatusResumeId(context, initialBufferText);
    if (statusInfo) {
      return statusInfo;
    }
  }

  if (targetSessionId) {
    try {
      const detected = await detectAgentResumeIdForSession(targetSessionId, { complete: true });
      if (detected.resumeId) {
        return detected;
      }
    } catch {
      // The complete backend probe is the final automatic fallback.
    }
  }

  return {
    resumeId: "",
    command: "",
    program: "codex",
    source: "manual",
  };
}

async function detectCurrentAgentResumeId() {
  return detectAgentResumeIdComplete(state.activeSessionId, activeTerminalContext);
}

function promptManualResumeInfo() {
  const rawValue = window.prompt("没有自动找到会话 ID。请粘贴 codex resume <id>、claude --resume <id> 或直接粘贴 ID：", "");
  if (rawValue === null) {
    return { id: "", program: "codex", command: "" };
  }
  return parseResumeInputInfo(rawValue);
}

async function archiveCurrentAgentResume() {
  if (archiveResumeButton) {
    archiveResumeButton.disabled = true;
  }
  updateStatus("正在识别当前 Codex 会话…", "info");

  try {
    let { resumeId, command, source } = await detectCurrentAgentResumeId();
    if (!resumeId) {
      const manual = promptManualResumeInfo();
      resumeId = manual.id;
      command = manual.command;
      source = "manual";
    }

    if (!resumeId) {
      updateStatus("未保存：没有可用的 Codex 会话 ID。", "warn");
      return;
    }

    const existing = state.resumeArchives.find((archive) => archive.resume_id === resumeId);
    const defaultNote = existing?.note || defaultResumeArchiveNote(resumeId);
    const note = window.prompt("归档备注：", defaultNote);
    if (note === null) {
      updateStatus("已取消归档。", "muted");
      return;
    }

    const saved = await requestJson("/api/terminal/resume-archives", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        cwd: state.currentPath,
        resume_id: resumeId,
        command: command || resumeCommandFromId(resumeId),
        terminal_name: activeSession()?.name || "",
        note,
        source,
      }),
    });

    announceResumeArchiveMutation("saved", saved);
    await loadResumeArchives();
    updateStatus(`已归档：${saved.note || shortResumeId(resumeId)}。`, "ok");
  } catch (error) {
    updateStatus(error.message || "保存 Codex 归档失败。", "warn");
  } finally {
    if (archiveResumeButton) {
      archiveResumeButton.disabled = false;
    }
    focusTerminalAfterTransientControl();
  }
}
