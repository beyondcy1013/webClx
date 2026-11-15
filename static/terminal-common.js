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
  const openDialogs = document.querySelectorAll("dialog[open]");
  const container = openDialogs.length > 0 ? openDialogs[openDialogs.length - 1] : document.body;
  container.appendChild(overlay);

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

  const openDialogs = document.querySelectorAll("dialog[open]");
  const container = openDialogs.length > 0 ? openDialogs[openDialogs.length - 1] : document.body;
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.dataset.terminalClipboardHelper = "true";
  textarea.setAttribute("tabindex", "-1");
  textarea.setAttribute("aria-hidden", "true");
  textarea.style.position = "fixed";
  textarea.style.top = "-9999px";
  textarea.style.left = "-9999px";
  textarea.style.width = "2em";
  textarea.style.height = "2em";
  textarea.style.padding = "0";
  textarea.style.border = "none";
  textarea.style.outline = "none";
  textarea.style.boxShadow = "none";
  textarea.style.background = "transparent";
  textarea.style.opacity = "0";
  container.appendChild(textarea);
  textarea.focus({ preventScroll: true });
  textarea.select();
  if (typeof textarea.setSelectionRange === "function") {
    textarea.setSelectionRange(0, textarea.value.length);
  }

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
        // Fall through to browser clipboard APIs outside the native client.
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

function formatTerminalTokenCount(value, { fixedMillions = false } = {}) {
  if (value == null || value === "") {
    return "";
  }
  const number = Number(value);
  if (!Number.isFinite(number) || number < 0) {
    return "";
  }
  if (number >= 1_000_000) {
    const formatted = (number / 1_000_000).toFixed(2);
    return `${fixedMillions ? formatted : formatted.replace(/\.00$/, "")}M`;
  }
  if (number >= 1_000) {
    return `${(number / 1_000).toFixed(1).replace(/\.0$/, "")}K`;
  }
  return String(Math.round(number));
}

function sessionContextDetailLabel(session) {
  const cached =
    session?.context_window ||
    session?.codex_status?.context_window ||
    (typeof state?.sessionContextCache?.get === "function"
      ? state.sessionContextCache.get(session?.id)
      : state?.sessionContextCache?.[session?.id]);

  if (!cached) {
    return "未记录";
  }

  const usedTokens = cached.used_tokens ?? cached.usedTokens;
  const totalTokens = cached.total_tokens ?? cached.totalTokens;

  if (usedTokens == null && totalTokens == null) {
    return "未记录";
  }

  const usedStr = formatTerminalTokenCount(usedTokens);
  const totalStr = formatTerminalTokenCount(totalTokens);

  if (usedStr && totalStr) {
    return `${usedStr} / ${totalStr}`;
  }
  if (usedStr) {
    return usedStr;
  }
  return "未记录";
}

function sessionModelFamilyLabel(session) {
  if (typeof sharedSessionModelFamilyLabel === "function") {
    return sharedSessionModelFamilyLabel(session);
  }
  const explicit = String(session?.model_family || session?.modelFamily || "").trim();
  if (explicit) {
    return explicit;
  }
  const statusModel = String(
    session?.codex_status?.model ||
    (typeof state !== "undefined" && typeof state?.sessionContextCache?.get === "function"
      ? state.sessionContextCache.get(session?.id)?.model
      : null) ||
    ""
  ).trim();
  if (statusModel) {
    const lower = statusModel.toLowerCase();
    if (lower.includes("deepseek")) return "deepseek";
    if (lower.includes("glm") || lower.includes("zhipu")) return "glm";
    if (lower.includes("claude") || lower.includes("sonnet") || lower.includes("opus") || lower.includes("haiku")) return "claude";
    if (lower.includes("gpt") || lower.includes("o1") || lower.includes("o3")) return "gpt";
    if (lower.includes("gemini")) return "gemini";
    if (lower.includes("qwen")) return "qwen";
    if (lower.includes("kimi") || lower.includes("moonshot")) return "kimi";
    return statusModel;
  }
  const text = `${session?.name || ""} ${session?.codex_api_preset_name || ""} ${session?.title || ""}`.toLowerCase();
  if (text.includes("deepseek")) return "deepseek";
  if (text.includes("glm") || text.includes("zhipu")) return "glm";
  if (text.includes("claude") || text.includes("sonnet") || text.includes("opus") || text.includes("haiku")) return "claude";
  if (text.includes("gpt") || text.includes("openai") || text.includes("o1") || text.includes("o3")) return "gpt";
  if (text.includes("gemini")) return "gemini";
  if (text.includes("qwen")) return "qwen";
  if (text.includes("kimi") || text.includes("moonshot")) return "kimi";
  return "";
}

function sessionOptionLabel(session) {
  const groupName = String(session?.group_name || session?.groupName || "").trim();
  const groupBadge = groupName ? `[${groupName}] ` : "";
  const label = `${groupBadge}${sessionActivityAgentPrefix(session)}${sessionActivityPrefix(session)}${session?.name || session?.id || "未命名终端"}${sessionActivityAgentSuffix(session)}`;
  if (!state.showSessionDetails && !state.showSessionAgent && !state.showSessionModelFamily && !state.showSessionContext) {
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
  if (state.showSessionModelFamily) {
    const modelFamilyDetail = sessionModelFamilyLabel(session) || "未记录";
    detailParts.push(modelFamilyDetail);
  }
  if (state.showSessionContext) {
    const contextDetail = sessionContextDetailLabel(session) || "未记录";
    detailParts.push(contextDetail);
  }
  return `${label} | ${detailParts.join(" | ")}`;
}

function sessionOptionTitle(session) {
  const label = sessionOptionLabel(session);
  const activity = sessionActivityLabel(session);
  const title = session?.title ? ` - ${session.title}` : "";
  return `${label} - ${activity}${title}`;
}
