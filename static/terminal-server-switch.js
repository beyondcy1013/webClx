function normalizeTerminalServerTarget(rawTarget) {
  const target = String(rawTarget || "").trim();
  if (!target) {
    throw new Error("请选择服务器。");
  }

  const url = new URL(/^https?:\/\//i.test(target) ? target : `http://${target}`);
  if (!url.hostname || url.username || url.password || (url.pathname && url.pathname !== "/")) {
    throw new Error("服务器地址无效。");
  }
  if (!url.port) {
    url.port = "11111";
  }
  return url.origin;
}

function buildTerminalServerSwitchUrl(rawTarget, locationLike = window.location) {
  const target = normalizeTerminalServerTarget(rawTarget);
  return `${target}${locationLike.pathname || "/"}${locationLike.search || ""}${locationLike.hash || ""}`;
}

function navigateToTerminalServer(
  rawTarget,
  locationLike = window.location,
  androidBridge = window.WebClxAndroid,
) {
  const targetUrl = buildTerminalServerSwitchUrl(rawTarget, locationLike);
  if (typeof androidBridge?.openInWebView === "function") {
    androidBridge.openInWebView(targetUrl);
    return targetUrl;
  }
  locationLike.assign(targetUrl);
  return targetUrl;
}

const TERMINAL_SERVER_FRAME_STORAGE_KEY = "webclx:terminal-server-frame";
const TERMINAL_SERVER_PRESETS_STORAGE_KEY = "webclx:terminal-server-presets";

function loadTerminalServerPresetOverrides() {
  try {
    const parsed = JSON.parse(window.localStorage.getItem(TERMINAL_SERVER_PRESETS_STORAGE_KEY) || "{}");
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function applyTerminalServerPresetOverrides(select) {
  const overrides = loadTerminalServerPresetOverrides();
  if (!select) return;
  Array.from(select.options).forEach((option, index) => {
    const value = overrides[String(index)];
    if (value) {
      option.value = value;
      option.textContent = value;
    }
  });
}

function editableTerminalServerTarget(rawTarget) {
  const normalized = normalizeTerminalServerTarget(rawTarget);
  return /^https?:\/\//i.test(String(rawTarget || "").trim())
    ? normalized
    : new URL(normalized).host;
}

function saveTerminalServerPreset(index, nextTarget) {
  if (index < 0 || !nextTarget) return;
  const overrides = loadTerminalServerPresetOverrides();
  overrides[String(index)] = nextTarget;
  try {
    window.localStorage.setItem(TERMINAL_SERVER_PRESETS_STORAGE_KEY, JSON.stringify(overrides));
  } catch {
    // Keep the edit usable for this session when storage is unavailable.
  }
}

function terminalServerViewElements() {
  return {
    body: document.body,
    terminalView: document.getElementById("terminal-workspace"),
    serverView: document.getElementById("terminal-server-workspace"),
    terminalTab: document.getElementById("top-nav-terminal"),
    serverTab: document.getElementById("top-nav-server"),
    frameSelect: document.getElementById("terminal-server-frame-select"),
    frameEdit: document.getElementById("terminal-server-frame-edit"),
    frameForm: document.getElementById("terminal-server-frame-form"),
    frameReload: document.getElementById("terminal-server-frame-reload"),
    frameOpen: document.getElementById("terminal-server-frame-open"),
    frameClose: document.getElementById("terminal-server-frame-close"),
    frame: document.getElementById("terminal-server-frame"),
    frameStatus: document.getElementById("terminal-server-frame-status"),
  };
}

function rememberTerminalServerTarget(target) {
  try {
    window.localStorage.setItem(TERMINAL_SERVER_FRAME_STORAGE_KEY, target);
  } catch {
    // Private modes can reject storage; the selector still remains usable.
  }
}

function lastTerminalServerTarget(select) {
  try {
    const saved = window.localStorage.getItem(TERMINAL_SERVER_FRAME_STORAGE_KEY);
    if (saved && select?.querySelector(`option[value="${CSS.escape(saved)}"]`)) {
      return saved;
    }
  } catch {
    // Fall through to the first configured target.
  }
  return select?.value || "";
}

function setTerminalServerFrameStatus(message, tone = "muted") {
  const { frameStatus } = terminalServerViewElements();
  if (!frameStatus) {
    return;
  }
  frameStatus.textContent = message;
  frameStatus.dataset.tone = tone;
  frameStatus.hidden = !message;
}

function syncTerminalServerFrameHeight() {
  const { serverView, frame } = terminalServerViewElements();
  if (!serverView || !frame || serverView.hidden) {
    return;
  }

  const viewRect = serverView.getBoundingClientRect();
  const frameRect = frame.getBoundingClientRect();
  const viewStyles = window.getComputedStyle(serverView);
  const availableHeight =
    window.innerHeight -
    Math.max(viewRect.top, 0) -
    (readCssPixelValue(viewStyles.paddingBottom) + readCssPixelValue(viewStyles.borderBottomWidth));

  if (availableHeight > 0) {
    frame.style.height = `${Math.max(Math.floor(availableHeight), 260)}px`;
    return;
  }

  frame.style.height = `${Math.max(Math.floor(frameRect.height), 260)}px`;
}

function readCssPixelValue(value) {
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

function connectTerminalServerFrame(rawTarget) {
  const { frameSelect, frame } = terminalServerViewElements();
  const select = frameSelect || document.getElementById("terminal-server-switch-select");
  if (!select || !frame) {
    return "";
  }

  const target = String(rawTarget || select.value || "").trim();
  if (!target) {
    setTerminalServerFrameStatus("请选择服务器。", "warn");
    return "";
  }

  let targetUrl;
  try {
    targetUrl = buildTerminalServerSwitchUrl(target);
  } catch (error) {
    setTerminalServerFrameStatus(error.message || "服务器地址无效。", "warn");
    return "";
  }

  select.value = target;
  rememberTerminalServerTarget(target);
  setTerminalServerFrameStatus(`正在连接 ${target}...`, "info");
  frame.src = targetUrl;
  return targetUrl;
}

async function ensureHostSessionForServerFrame() {
  try {
    const response = await fetch("/api/auth/session", { cache: "no-store" });
    if (!response.ok) {
      setTerminalServerFrameStatus("宿主未登录，无法使用宿主鉴权。", "warn");
      return false;
    }
    const session = await response.json();
    if (session?.authenticated) {
      return true;
    }
    setTerminalServerFrameStatus("宿主未登录，无法使用宿主鉴权。", "warn");
    return false;
  } catch {
    setTerminalServerFrameStatus("无法确认宿主登录状态。", "warn");
    return false;
  }
}

async function issueTerminalServerFrameTicket(targetUrl) {
  const response = await fetch("/api/auth/cross-instance-ticket", {
    cache: "no-store",
    headers: {
      "X-WebClx-Target-Origin": new URL(targetUrl).origin,
    },
  });
  if (!response.ok) {
    throw new Error(await response.text() || "无法获取跨实例免登录凭证。");
  }
  const payload = await response.json();
  if (!payload?.ticket) {
    throw new Error("跨实例免登录凭证无效。");
  }
  return payload.ticket;
}

function terminalServerFrameLoginUrl(targetUrl, ticket) {
  const url = new URL(targetUrl);
  url.pathname = "/login";
  url.search = new URLSearchParams({
    next: `${new URL(targetUrl).pathname || "/"}${new URL(targetUrl).search || ""}`,
    cross_instance_ticket: ticket,
  }).toString();
  return url.toString();
}

async function connectTerminalServerFrameWithHostAuth(rawTarget) {
  const targetUrl = connectTerminalServerFrame(rawTarget);
  if (!targetUrl) {
    return "";
  }
  const authenticated = await ensureHostSessionForServerFrame();
  if (authenticated) {
    try {
      const ticket = await issueTerminalServerFrameTicket(targetUrl);
      const loginUrl = terminalServerFrameLoginUrl(targetUrl, ticket);
      const frame = document.getElementById("terminal-server-frame");
      if (frame) frame.src = loginUrl;
    } catch (error) {
      setTerminalServerFrameStatus(error.message || "跨实例免登录失败。", "warn");
    }
  }
  return targetUrl;
}

function setTerminalServerView(active, rawTarget = "") {
  const views = terminalServerViewElements();
  if (!views.terminalView || !views.serverView) {
    return;
  }

  const serverActive = active === true || active === "server";
  views.terminalView.hidden = serverActive;
  views.serverView.hidden = !serverActive;
  views.body.dataset.terminalView = serverActive ? "server" : "terminal";
  views.terminalTab?.setAttribute("aria-selected", serverActive ? "false" : "true");
  views.terminalTab?.classList.toggle("active", !serverActive);
  views.serverTab?.setAttribute("aria-selected", serverActive ? "true" : "false");
  views.serverTab?.classList.toggle("active", serverActive);

  if (!serverActive) {
    setTerminalServerFrameStatus("");
    frame?.style.removeProperty("height");
    window.dispatchEvent(new Event("resize"));
    return;
  }

  if (rawTarget) {
    views.frameSelect.value = rawTarget;
  } else if (views.frameSelect && !views.frameSelect.value) {
    views.frameSelect.value = lastTerminalServerTarget(views.frameSelect);
  }
  if (!frame_has_source(views.frame)) {
    void connectTerminalServerFrameWithHostAuth(views.frameSelect.value);
  }
  requestAnimationFrame(syncTerminalServerFrameHeight);
  window.dispatchEvent(new Event("resize"));
}

function frame_has_source(frame) {
  return Boolean(frame?.src && frame.src !== "about:blank");
}

function openTerminalServerSwitchDialog(rawTarget = "") {
  setTerminalServerView(true, rawTarget);
}

(() => {
  const dialog = document.getElementById("terminal-server-switch-dialog");
  const form = document.getElementById("terminal-server-switch-form");
  const select = document.getElementById("terminal-server-switch-select");
  const cancel = document.getElementById("terminal-server-switch-cancel");
  const {
    terminalTab,
    serverTab,
    frameSelect,
    frameForm,
    frameReload,
    frameOpen,
    frameClose,
    frame,
    frameEdit,
  } = terminalServerViewElements();

  const editDialog = document.getElementById("terminal-server-edit-dialog");
  const editForm = document.getElementById("terminal-server-edit-form");
  const editInput = document.getElementById("terminal-server-edit-input");
  const editSelect = document.getElementById("terminal-server-edit-select");
  const editCancel = document.getElementById("terminal-server-edit-cancel");
  const editStatus = document.getElementById("terminal-server-edit-status");

  if (frameSelect && select) {
    applyTerminalServerPresetOverrides(select);
    frameSelect.replaceChildren(...Array.from(select.options, (option) => option.cloneNode(true)));
    frameSelect.value = lastTerminalServerTarget(frameSelect);
  }
  if (editSelect && frameSelect) {
    editSelect.replaceChildren(...Array.from(frameSelect.options, (option) => option.cloneNode(true)));
    editSelect.value = frameSelect.value;
  }

  frameEdit?.addEventListener("click", () => {
    if (!editDialog || !editInput || !editSelect || !frameSelect?.value) return;
    editSelect.value = frameSelect.value;
    editInput.value = editSelect.value;
    if (editStatus) { editStatus.hidden = true; editStatus.textContent = ""; }
    if (typeof editDialog.showModal === "function") editDialog.showModal();
    editInput.focus();
    editInput.select();
  });
  editSelect?.addEventListener("change", () => {
    editInput.value = editSelect.value;
    if (editStatus) { editStatus.hidden = true; editStatus.textContent = ""; }
  });
  editCancel?.addEventListener("click", () => editDialog?.close());
  editForm?.addEventListener("submit", (event) => {
    event.preventDefault();
    let nextTarget;
    try { nextTarget = editableTerminalServerTarget(editInput?.value); }
    catch (error) {
      if (editStatus) { editStatus.textContent = error.message || "服务器地址无效。"; editStatus.dataset.tone = "warn"; editStatus.hidden = false; }
      return;
    }
    const option = editSelect?.selectedOptions?.[0];
    if (!option || !editSelect || !frameSelect || !select) return;
    const optionIndex = option.index;
    option.value = nextTarget;
    option.textContent = option.value;
    frameSelect.options[optionIndex].value = option.value;
    frameSelect.options[optionIndex].textContent = option.textContent;
    select.options[optionIndex].value = option.value;
    select.options[optionIndex].textContent = option.textContent;
    saveTerminalServerPreset(optionIndex, option.value);
    frameSelect.value = option.value;
    editSelect.value = option.value;
    rememberTerminalServerTarget(option.value);
    editDialog?.close();
    setTerminalServerFrameStatus(`已更新服务器预设：${option.value}`, "ok");
  });

  terminalTab?.addEventListener("click", (event) => {
    event.preventDefault();
    setTerminalServerView(false);
  });
  serverTab?.addEventListener("click", () => setTerminalServerView(true));
  frameForm?.addEventListener("submit", async (event) => {
    event.preventDefault();
    await connectTerminalServerFrameWithHostAuth(frameSelect.value);
  });
  frameReload?.addEventListener("click", async () => {
    if (frame_has_source(frame)) {
      setTerminalServerFrameStatus("正在重新加载...", "info");
      frame.src = frame.src;
    } else {
      await connectTerminalServerFrameWithHostAuth(frameSelect.value);
    }
  });
  frameOpen?.addEventListener("click", () => {
    navigateToTerminalServer(frameSelect.value);
  });
  frameClose?.addEventListener("click", () => setTerminalServerView(false));
  frame?.addEventListener("load", () => {
    if (frame_has_source(frame)) {
      setTerminalServerFrameStatus(`已连接 ${frameSelect?.value || ""}`, "ok");
    }
  });
  frame?.addEventListener("error", () => {
    setTerminalServerFrameStatus("服务器加载失败，可尝试其他目标。", "warn");
  });
  window.addEventListener?.("resize", syncTerminalServerFrameHeight);
  window.visualViewport?.addEventListener?.("resize", syncTerminalServerFrameHeight);

  cancel?.addEventListener("click", () => dialog?.close());
  form?.addEventListener("submit", (event) => {
    event.preventDefault();
    dialog?.close();
    setTerminalServerView(true, select?.value);
  });
})();
