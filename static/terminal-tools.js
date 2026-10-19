// Compact dropdown terminal tools and reversible Codex full-access switch.
// Loaded before terminal.js; DOM references are initialized there before use.

const TERMINAL_TOOLS_MENU_MARGIN_PX = 8;
const TERMINAL_TOOLS_MENU_GAP_PX = 6;
let terminalToolsRestoringTriggerFocus = false;
let terminalToolsMenuActionsBound = false;

function positionCompactTerminalMenu(menu, trigger) {
  if (!menu || !trigger || menu.hidden) {
    return;
  }
  menu.style.removeProperty("max-height");
  const triggerRect = trigger.getBoundingClientRect();
  const menuRect = menu.getBoundingClientRect();
  const viewportWidth = document.documentElement.clientWidth || window.innerWidth;
  const viewportHeight = window.innerHeight;
  const availableAbove = Math.max(
    0,
    triggerRect.top - TERMINAL_TOOLS_MENU_GAP_PX - TERMINAL_TOOLS_MENU_MARGIN_PX,
  );
  const availableBelow = Math.max(
    0,
    viewportHeight
      - triggerRect.bottom
      - TERMINAL_TOOLS_MENU_GAP_PX
      - TERMINAL_TOOLS_MENU_MARGIN_PX,
  );
  const placeAbove = availableAbove >= menuRect.height || availableAbove >= availableBelow;
  const availableHeight = placeAbove ? availableAbove : availableBelow;
  if (menuRect.height > availableHeight) {
    menu.style.maxHeight = `${Math.floor(availableHeight)}px`;
  }
  const fittedRect = menu.getBoundingClientRect();
  const maxLeft = Math.max(
    TERMINAL_TOOLS_MENU_MARGIN_PX,
    viewportWidth - fittedRect.width - TERMINAL_TOOLS_MENU_MARGIN_PX,
  );
  const left = Math.min(
    Math.max(triggerRect.left, TERMINAL_TOOLS_MENU_MARGIN_PX),
    maxLeft,
  );
  const top = placeAbove
    ? triggerRect.top - fittedRect.height - TERMINAL_TOOLS_MENU_GAP_PX
    : triggerRect.bottom + TERMINAL_TOOLS_MENU_GAP_PX;
  menu.style.left = `${Math.round(left)}px`;
  menu.style.top = `${Math.round(top)}px`;
}

function setTerminalToolsStatus(message, tone = "muted") {
  if (!terminalToolsStatusEl) {
    return;
  }
  const text = String(message || "").trim();
  terminalToolsStatusEl.textContent = text;
  terminalToolsStatusEl.dataset.tone = tone;
  terminalToolsStatusEl.hidden = !text;
  window.requestAnimationFrame(positionTerminalToolsMenu);
}

function positionTerminalToolsMenu() {
  positionCompactTerminalMenu(terminalToolsMenuEl, terminalToolsButtonEl);
}

function setTerminalToolsMenuExpanded(expanded, { restoreFocus = false } = {}) {
  if (!terminalToolsMenuEl || !terminalToolsButtonEl) {
    return;
  }
  terminalToolsMenuEl.hidden = !expanded;
  terminalToolsButtonEl.setAttribute("aria-expanded", expanded ? "true" : "false");
  if (expanded) {
    positionTerminalToolsMenu();
    window.requestAnimationFrame(positionTerminalToolsMenu);
  } else {
    if (typeof terminalToolMenuEl !== "undefined" && terminalToolMenuEl) {
      terminalToolMenuEl.hidden = true;
    }
    terminalToolsMenuEl.style.removeProperty("left");
    terminalToolsMenuEl.style.removeProperty("top");
    terminalToolsMenuEl.style.removeProperty("max-height");
    if (restoreFocus) {
      terminalToolsRestoringTriggerFocus = true;
      try {
        terminalToolsButtonEl.focus({ preventScroll: true });
      } finally {
        terminalToolsRestoringTriggerFocus = false;
      }
    }
  }
}

async function refreshCodexFullAccessToggle() {
  if (!terminalCodexFullAccessToggleEl) {
    return;
  }
  terminalCodexFullAccessToggleEl.disabled = true;
  setTerminalToolsStatus("读取中…", "info");
  try {
    const result = await requestJson("/api/terminal/codex-full-access", {
      method: "GET",
    });
    terminalCodexFullAccessToggleEl.checked = result?.enabled === true;
    setTerminalToolsStatus("");
  } catch (error) {
    setTerminalToolsStatus(error?.message || "读取 Codex 权限状态失败。", "warn");
  } finally {
    terminalCodexFullAccessToggleEl.disabled = false;
    if (terminalToolsMenuEl && !terminalToolsMenuEl.hidden) {
      terminalCodexFullAccessToggleEl.focus({ preventScroll: true });
    }
  }
}

function handleTerminalToolsMenuAction(event) {
  const button = event.target instanceof Element
    ? event.target.closest("button[data-action]")
    : null;
  if (!(button instanceof HTMLButtonElement) || !terminalToolsMenuEl?.contains(button)) {
    return;
  }
  event.preventDefault();
  triggerMobileKey(button);
  closeTerminalToolsMenu();
}

function ensureTerminalToolsMenuActionsBound() {
  if (!terminalToolsMenuEl || terminalToolsMenuActionsBound) {
    return;
  }
  terminalToolsMenuEl.addEventListener("click", handleTerminalToolsMenuAction);
  terminalToolsMenuActionsBound = true;
}

function closeTerminalToolsMenu(options = {}) {
  setTerminalToolsMenuExpanded(false, options);
}

function toggleTerminalToolsMenu() {
  if (!terminalToolsMenuEl) {
    return;
  }
  ensureTerminalToolsMenuActionsBound();
  const expanded = terminalToolsMenuEl.hidden;
  setTerminalToolsMenuExpanded(expanded);
  if (expanded) {
    if (typeof openTerminalToolMenu === "function") {
      openTerminalToolMenu("tools", terminalToolsButtonEl);
    }
    refreshCodexFullAccessToggle();
  }
}

async function toggleCodexFullAccess() {
  if (!terminalCodexFullAccessToggleEl) {
    return;
  }

  const enabled = terminalCodexFullAccessToggleEl.checked;
  const previous = !enabled;
  if (enabled && (!state.activeSessionId || !isTerminalConnected())) {
    terminalCodexFullAccessToggleEl.checked = previous;
    setTerminalToolsStatus("当前终端尚未连接。", "warn");
    return;
  }
  const targetSessionId = state.activeSessionId;
  terminalCodexFullAccessToggleEl.disabled = true;
  setTerminalToolsStatus(enabled ? "正在开启…" : "正在关闭…", "info");
  try {
    const result = await requestJson("/api/terminal/codex-full-access", {
      method: enabled ? "PUT" : "DELETE",
    });
    if (result?.enabled !== enabled) {
      throw new Error("Codex 权限状态与请求不一致。");
    }
    terminalCodexFullAccessToggleEl.checked = enabled;
    const user = String(result?.user || "当前终端用户");
    if (!enabled) {
      setTerminalToolsStatus("已关闭", "ok");
      updateStatus(`已恢复 ${user} 的 Codex 权限配置。`, "ok");
      return;
    }

    setTerminalToolsStatus("已开启，正在启动…", "info");
    const sent = await sendTerminalAutoTypedInput("codex", { sessionId: targetSessionId });
    if (!sent) {
      setTerminalToolsStatus("已开启，启动失败", "warn");
      updateStatus("Codex 最高权限已开启，但启动命令发送失败。", "warn");
      return;
    }
    setTerminalToolsStatus("已开启", "ok");
    updateStatus(`已为 ${user} 启动 Codex 最高权限。`, "ok");
  } catch (error) {
    terminalCodexFullAccessToggleEl.checked = previous;
    setTerminalToolsStatus(error?.message || "切换 Codex 最高权限失败。", "warn");
  } finally {
    terminalCodexFullAccessToggleEl.disabled = false;
  }
}

async function forceInterruptAndResumeTerminalAgent() {
  const sessionId = String(state.activeSessionId || "").trim();
  const session = state.sessions.find((item) => item.id === sessionId);
  if (!sessionId || !session || !isTerminalConnected()) {
    setTerminalToolsStatus("当前终端尚未连接。", "warn");
    return;
  }
  if (!window.confirm(
    `将中断终端“${session.name}”当前正在执行的智能体任务，并从原会话恢复。确定继续吗？`,
  )) {
    return;
  }

  terminalInterruptResumeButtonEl.disabled = true;
  setTerminalToolsStatus("正在中断并恢复…", "info");
  try {
    const result = await requestJson(
      `/api/terminal/sessions/${encodeURIComponent(sessionId)}/interrupt-and-resume`,
      { method: "POST" },
    );
    if (result?.ok !== true || result?.outcome !== "resumed") {
      throw new Error("后端未确认会话已恢复。");
    }
    setTerminalToolsStatus("已中断并恢复", "ok");
    updateStatus(`终端“${session.name}”已中断等待并恢复原会话。`, "ok");
    closeTerminalToolsMenu();
    window.setTimeout(() => loadSessions({ preserveLocalActivity: true }), 500);
  } catch (error) {
    const message = error?.message || "中断并恢复失败。";
    setTerminalToolsStatus(message, "warn");
    updateStatus(message, "warn");
  } finally {
    terminalInterruptResumeButtonEl.disabled = false;
  }
}
