// webClx terminal common helpers shared by extracted terminal modules.
// Extracted from terminal.js as global declarations; no top-level DOM setup.

function showManualCopyFallback(text) {
  const existing = document.getElementById("terminal-manual-copy-overlay");
  if (existing) {
    existing.remove();
  }

  const overlay = document.createElement("div");
  overlay.id = "terminal-manual-copy-overlay";
  overlay.style.cssText = "position:fixed;inset:0;z-index:10000;display:flex;align-items:center;justify-content:center;padding:16px;background:rgba(0,0,0,0.45);";

  const card = document.createElement("div");
  card.style.cssText = "width:min(560px,100%);padding:16px;border:1px solid var(--border,#333);border-radius:8px;background:var(--panel-bg,#1e1e2e);box-shadow:0 12px 32px rgba(0,0,0,0.38);";

  const heading = document.createElement("div");
  heading.textContent = "手动复制";
  heading.style.cssText = "margin-bottom:10px;font-size:15px;font-weight:600;color:var(--fg,#cdd6f4);";

  const hint = document.createElement("p");
  hint.textContent = "浏览器阻止了自动写入剪贴板，请复制下面已选中的内容。";
  hint.style.cssText = "margin:0 0 10px;color:var(--muted,#a6adc8);font-size:13px;line-height:1.5;";

  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.readOnly = true;
  textarea.style.cssText = "width:100%;min-height:120px;box-sizing:border-box;padding:10px;border:1px solid var(--border,#444);border-radius:8px;background:var(--input-bg,#181825);color:var(--fg,#cdd6f4);font:13px/1.5 monospace;resize:vertical;";

  const buttonRow = document.createElement("div");
  buttonRow.style.cssText = "display:flex;gap:8px;justify-content:flex-end;margin-top:10px;";

  const selectButton = document.createElement("button");
  selectButton.type = "button";
  selectButton.className = "button secondary";
  selectButton.textContent = "选中文本";
  selectButton.addEventListener("click", () => {
    textarea.focus();
    textarea.select();
  });

  const closeButton = document.createElement("button");
  closeButton.type = "button";
  closeButton.className = "button primary";
  closeButton.textContent = "关闭";
  closeButton.addEventListener("click", () => overlay.remove());

  buttonRow.append(selectButton, closeButton);
  card.append(heading, hint, textarea, buttonRow);
  overlay.appendChild(card);
  document.body.appendChild(overlay);

  window.requestAnimationFrame(() => {
    textarea.focus();
    textarea.select();
  });
}

async function copyTextToClipboard(text) {
  if (!text) {
    return false;
  }

  try {
    if (
      typeof window.WebClxAndroid?.copyText === "function"
      && window.WebClxAndroid.copyText(text) === true
    ) {
      return true;
    }
  } catch {
    // Fall through to browser clipboard APIs outside the native client.
  }

  let clipboardWriteFailed = false;
  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(text);
      return true;
    } catch {
      clipboardWriteFailed = true;
      // Some browsers deny Clipboard API writes while still allowing the
      // user-gesture based execCommand fallback.
    }
  }

  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.setAttribute("readonly", "true");
  textarea.dataset.terminalClipboardHelper = "true";
  textarea.style.position = "fixed";
  textarea.style.opacity = "0";
  textarea.style.pointerEvents = "none";
  document.body.appendChild(textarea);
  textarea.focus();
  textarea.select();

  try {
    const copied = Boolean(document.execCommand?.("copy"));
    if (!copied) {
      showManualCopyFallback(text);
      return false;
    }
    if (clipboardWriteFailed && navigator.clipboard?.readText) {
      try {
        const currentText = await navigator.clipboard.readText();
        if (currentText !== text) {
          showManualCopyFallback(text);
          return false;
        }
      } catch {
        showManualCopyFallback(text);
        return false;
      }
    }
    return true;
  } catch {
    showManualCopyFallback(text);
    return false;
  } finally {
    textarea.remove();
  }
}

function requestJson(url, options) {
  return fetch(url, options).then(async (response) => {
    if (!response.ok) {
      const message = await response.text();
      throw new Error(message || `请求失败: ${response.status}`);
    }
    return response.json();
  });
}

function announceResumeArchiveMutation(action, archive = {}) {
  try {
    window.localStorage.setItem(
      RESUME_ARCHIVE_EVENT_STORAGE_KEY,
      JSON.stringify({
        action,
        archive_id: archive.id || "",
        resume_id: archive.resume_id || archive.resumeId || "",
        at: Date.now(),
      }),
    );
  } catch {
    // Keep working even if localStorage is unavailable.
  }
}

function normalizeRelativePath(pathValue) {
  return String(pathValue || "")
    .replace(/^\/+|\/+$/g, "")
    .split("/")
    .filter(Boolean)
    .join("/");
}

function normalizeAbsolutePath(pathValue) {
  const parts = String(pathValue || "")
    .split("/")
    .filter(Boolean);
  return parts.length > 0 ? `/${parts.join("/")}` : "/";
}

function normalizeTerminalPath(pathValue) {
  const rawPath = String(pathValue || "").trim();
  if (!rawPath || rawPath === "/") {
    return "";
  }
  return rawPath.startsWith("/")
    ? normalizeAbsolutePath(rawPath)
    : normalizeRelativePath(rawPath);
}

function terminalDisplayPath(pathValue) {
  const normalizedPath = normalizeTerminalPath(pathValue);
  if (!normalizedPath) {
    return "/";
  }
  return normalizedPath.startsWith("/") ? normalizedPath : `/${normalizedPath}`;
}

function sessionPath(session) {
  return normalizeTerminalPath(session?.path || "");
}

function sessionDisplayPath(session) {
  if (typeof session?.display_path === "string" && session.display_path.trim()) {
    return session.display_path;
  }

  return terminalDisplayPath(sessionPath(session));
}

function syncCurrentPathDisplay(displayPath = "") {
  const nextDisplayPath = displayPath || terminalDisplayPath(state.currentPath);
  if (terminalPathEl) {
    terminalPathEl.textContent = nextDisplayPath;
  }
  if (terminalNavPathEl) {
    terminalNavPathEl.textContent = nextDisplayPath;
    terminalNavPathEl.title = nextDisplayPath;
  }
  syncTerminalNavScroll({ forceEnd: true });
  syncTopNavigation();
}

function sessionOptionLabel(session) {
  const label = `${sessionActivityAgentPrefix(session)}${sessionActivityPrefix(session)}${session?.name || session?.id || "未命名终端"}${sessionActivityAgentSuffix(session)}`;
  if (!state.showSessionDetails && !state.showSessionAgent) {
    return label;
  }
  const detailParts = [];
  if (state.showSessionDetails) {
    const apiDetail =
      typeof session?.codex_api_preset_name === "string" && session.codex_api_preset_name.trim()
        ? session.codex_api_preset_name.trim()
        : typeof session?.codex_api_base_url === "string" && session.codex_api_base_url.trim()
          ? session.codex_api_base_url.trim()
          : "未记录";
    detailParts.push(apiDetail);
  }
  if (state.showSessionAgent) {
    const agentDetail = sessionActivityAgentLabel(session) || "未记录";
    detailParts.push(agentDetail);
  }
  return `${label} | ${detailParts.join(" | ")}`;
}

function sessionOptionTitle(session) {
  const label = sessionOptionLabel(session);
  const activity = sessionActivityLabel(session);
  const title = session?.title ? ` - ${session.title}` : "";
  return `${label} - ${activity}${title}`;
}
