function prepareTerminalPasteText(text) {
  return String(text).replace(/\r\n|\r|\n/g, "\r");
}

function wrapBracketedTerminalPaste(text) {
  return `\u001b[200~${text}\u001b[201~`;
}

function hasTerminalPasteLineBreak(text) {
  return /[\r\n]/.test(text);
}

function setClipboardPasteBusy(disabled) {
  if (pasteClipboardButton) {
    pasteClipboardButton.disabled = disabled;
  }
}

function openTerminalPasteDialog(prefill = "") {
  if (!terminalPasteDialogEl || !terminalPasteTextEl) {
    return;
  }

  resetTerminalImeFocusContext();
  // Opening the paste dialog must not cancel already-pending scheduled sends.
  setTerminalPasteBusy(false);
  if (terminalPasteScheduleEl) {
    terminalPasteScheduleEl.hidden = true;
  }
  if (terminalPasteScheduleToggleEl) {
    terminalPasteScheduleToggleEl.textContent = "定时发送";
  }
  setTerminalPasteScheduleStatus("");
  terminalPasteTextEl.value = prefill;
  terminalPasteAssetEntries = [];
  renderTerminalPasteAssets();
  if (hasTerminalPasteScheduledTask()) {
    tickTerminalPasteScheduledCountdown();
  }
  if (typeof terminalPasteDialogEl.showModal === "function") {
    if (!terminalPasteDialogEl.open) {
      terminalPasteDialogEl.showModal();
    }
  } else {
    terminalPasteDialogEl.setAttribute("open", "");
  }

}

// Opens the paste dialog with the schedule panel already expanded. Opening a
// command surface must not focus an editable control and summon the system IME.
function openScheduledTerminalPasteDialog(prefill = "") {
  if (!terminalPasteDialogEl || !terminalPasteTextEl) {
    return;
  }

  openTerminalPasteDialog(prefill);

  if (terminalPasteScheduleEl) {
    terminalPasteScheduleEl.hidden = false;
  }
  if (terminalPasteScheduleToggleEl) {
    terminalPasteScheduleToggleEl.textContent = "收起定时";
  }

  // Pre-fill the datetime picker to now + 5 minutes (same as toggle handler).
  const now = new Date();
  now.setSeconds(0, 0);
  now.setMinutes(now.getMinutes() + 5);
  const pad = (value) => String(value).padStart(2, "0");
  const datetimeValue = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}T${pad(now.getHours())}:${pad(now.getMinutes())}`;
  if (terminalPasteScheduleDatetimeEl) {
    terminalPasteScheduleDatetimeEl.min = datetimeValue;
    terminalPasteScheduleDatetimeEl.value = datetimeValue;
  }

}

function closeTerminalPasteDialog() {
  if (!terminalPasteDialogEl) {
    return;
  }

  // Scheduled snapshots are already captured and managed independently.
  setTerminalPasteBusy(false);
  revokeTerminalPasteAssetPreviews();
  terminalPasteAssetEntries = [];
  renderTerminalPasteAssets();
  if (typeof terminalPasteDialogEl.close === "function") {
    if (terminalPasteDialogEl.open) {
      terminalPasteDialogEl.close();
    }
  } else {
    terminalPasteDialogEl.removeAttribute("open");
  }
}

function setTerminalPasteBusy(disabled) {
  terminalPasteBusy = Boolean(disabled);
  setClipboardPasteBusy(disabled);
  [terminalPasteTextEl, terminalPasteSubmitButton, terminalPasteSubmitEnterButton].forEach((element) => {
    if (element) {
      element.disabled = disabled;
    }
  });
}

function formatTerminalInputHistoryTime(value) {
  const timestamp = Number(value);
  if (!Number.isFinite(timestamp) || timestamp <= 0) {
    return "";
  }
  try {
    return new Intl.DateTimeFormat("zh-CN", {
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    }).format(new Date(timestamp));
  } catch {
    return new Date(timestamp).toLocaleString();
  }
}

function renderTerminalInputHistory(entries = terminalInputHistoryEntries) {
  terminalInputHistoryEntries = Array.isArray(entries) ? entries : [];
  if (!terminalInputHistoryListEl) {
    return;
  }

  terminalInputHistoryListEl.replaceChildren();
  if (terminalInputHistoryLoading) {
    const empty = document.createElement("p");
    empty.className = "terminal-input-history-empty";
    empty.textContent = "加载中…";
    terminalInputHistoryListEl.appendChild(empty);
    return;
  }

  if (terminalInputHistoryEntries.length === 0) {
    const empty = document.createElement("p");
    empty.className = "terminal-input-history-empty";
    empty.textContent = "暂无对话历史";
    terminalInputHistoryListEl.appendChild(empty);
    return;
  }

  terminalInputHistoryEntries.forEach((entry, index) => {
    const item = document.createElement("article");
    item.className = "terminal-input-history-item";

    const meta = document.createElement("div");
    meta.className = "terminal-input-history-meta";
    const metaInfo = document.createElement("div");
    metaInfo.className = "terminal-input-history-meta-info";
    const order = document.createElement("span");
    order.textContent = String(index + 1).padStart(3, "0");
    metaInfo.appendChild(order);
    const timeText = formatTerminalInputHistoryTime(entry?.created_at ?? entry?.createdAt);
    if (timeText) {
      const time = document.createElement("time");
      time.textContent = timeText;
      metaInfo.appendChild(time);
    }
    meta.appendChild(metaInfo);

    const copyButton = document.createElement("button");
    copyButton.className = "button secondary terminal-input-history-copy-item";
    copyButton.type = "button";
    copyButton.textContent = "复制";
    copyButton.setAttribute("aria-label", `复制第 ${index + 1} 条对话历史`);
    copyButton.addEventListener("click", () => {
      copyTerminalInputHistoryEntry(entry);
    });
    meta.appendChild(copyButton);

    const text = document.createElement("pre");
    text.className = "terminal-input-history-text";
    text.textContent = String(entry?.text || "");

    item.append(meta, text);
    terminalInputHistoryListEl.appendChild(item);
  });
}

async function copyTerminalInputHistoryEntry(entry) {
  const text = String(entry?.text || "");
  if (!text.trim()) {
    updateTerminalInputHistoryStatus("这条对话历史为空，无法复制。", "warn");
    return;
  }

  const copied = await copyTextToClipboard(text);
  updateTerminalInputHistoryStatus(
    copied ? "已复制本条对话历史。" : "复制本条对话历史失败。",
    copied ? "ok" : "warn",
  );
}

function openTerminalInputHistoryDialog() {
  if (!terminalInputHistoryDialogEl) {
    return;
  }
  resetTerminalImeFocusContext();
  if (typeof terminalInputHistoryDialogEl.showModal === "function") {
    if (!terminalInputHistoryDialogEl.open) {
      terminalInputHistoryDialogEl.showModal();
    }
  } else {
    terminalInputHistoryDialogEl.setAttribute("open", "");
  }
}

function closeTerminalInputHistoryDialog() {
  if (!terminalInputHistoryDialogEl) {
    return;
  }
  if (typeof terminalInputHistoryDialogEl.close === "function") {
    if (terminalInputHistoryDialogEl.open) {
      terminalInputHistoryDialogEl.close();
    }
  } else {
    terminalInputHistoryDialogEl.removeAttribute("open");
  }
}

function openTerminalAgentsDocDialog() {
  if (!terminalAgentsDocDialogEl) {
    return;
  }
  resetTerminalImeFocusContext();
  if (typeof terminalAgentsDocDialogEl.showModal === "function") {
    if (!terminalAgentsDocDialogEl.open) {
      terminalAgentsDocDialogEl.showModal();
    }
  } else {
    terminalAgentsDocDialogEl.setAttribute("open", "");
  }
}

function closeTerminalAgentsDocDialog() {
  if (!terminalAgentsDocDialogEl) {
    return;
  }
  if (typeof terminalAgentsDocDialogEl.close === "function") {
    if (terminalAgentsDocDialogEl.open) {
      terminalAgentsDocDialogEl.close();
    }
  } else {
    terminalAgentsDocDialogEl.removeAttribute("open");
  }
}

function setTerminalAgentsDocBusy(disabled) {
  [
    terminalAgentsDocEditorEl,
    terminalAgentsDocSelectEl,
    terminalAgentsDocSaveButton,
    terminalAgentsDocCloseButton,
    terminalAgentsDocCreateButton,
    terminalAgentsDocRefreshButton,
    terminalAgentsDocNameInputEl,
    terminalAgentsDocMaxAgeDaysEl,
    terminalAgentsDocRecursiveDirectoriesEl,
    terminalAgentsDocShowHiddenEl,
  ].forEach((element) => {
    if (element) {
      element.disabled = Boolean(disabled);
    }
  });
}

function terminalAgentsDocPathValue() {
  return String(terminalAgentsDocSelectEl?.value || "AGENTS.MD").trim() || "AGENTS.MD";
}

// 与 Rust 端 is_terminal_doc_file 保持一致的扩展名白名单。
const TERMINAL_DOC_EXTENSIONS = new Set([
  "md",
  "markdown",
  "txt",
  "toml",
  "json",
  "yaml",
  "yml",
]);

function normalizeNewTerminalDocName(rawName) {
  const trimmed = String(rawName || "").trim();
  if (!trimmed) {
    return { error: "请输入文档名。" };
  }
  if (trimmed.includes("\0")) {
    return { error: "文档名包含非法字符。" };
  }
  if (/^[\\/]/.test(trimmed)) {
    return { error: "请输入相对于当前目录的文档名。" };
  }
  const segments = trimmed.split(/[\\/]+/).filter(Boolean);
  if (segments.some((segment) => segment === "..")) {
    return { error: "文档名不能包含上级目录引用。" };
  }
  const fileName = segments[segments.length - 1];
  if (!fileName || fileName === ".") {
    return { error: "请输入文档名。" };
  }

  // 自动补 .md：仅当完全没有扩展名或扩展名不在白名单内时才补。
  const dotIndex = fileName.lastIndexOf(".");
  let finalName = fileName;
  if (dotIndex > 0) {
    const ext = fileName.slice(dotIndex + 1).toLowerCase();
    if (!TERMINAL_DOC_EXTENSIONS.has(ext)) {
      return { error: "文档扩展名不被支持，只能使用 md/markdown/txt/toml/json/yaml/yml。" };
    }
  } else if (dotIndex === -1) {
    finalName = `${fileName}.md`;
  } else {
    // dotIndex === 0：以点开头的隐藏文件，没有扩展名，按 .md 补齐。
    finalName = `${fileName}.md`;
  }

  const finalSegments =
    segments.length === 1 ? [finalName] : segments.slice(0, -1).concat(finalName);
  return { path: finalSegments.join("/") };
}

async function handleCreateTerminalAgentsDoc() {
  const sessionId = terminalAgentsDocSessionId || state.activeSessionId;
  if (!sessionId) {
    updateTerminalAgentsDocStatus("请先选择一个终端会话。", "warn");
    return;
  }
  if (!terminalAgentsDocSelectEl) {
    return;
  }
  const { path: newPath, error } = normalizeNewTerminalDocName(
    terminalAgentsDocNameInputEl?.value,
  );
  if (error) {
    updateTerminalAgentsDocStatus(error, "warn");
    return;
  }

  // 同名选项已存在：仅切换选中并触发加载。
  const existing = Array.from(terminalAgentsDocSelectEl.options).find(
    (option) => option.value === newPath,
  );
  if (existing) {
    terminalAgentsDocSelectEl.value = newPath;
    terminalAgentsDocSelectEl.dispatchEvent(new Event("change"));
    if (terminalAgentsDocNameInputEl) {
      terminalAgentsDocNameInputEl.value = "";
    }
    return;
  }

  const option = document.createElement("option");
  option.value = newPath;
  option.textContent = `${newPath} (新建)`;
  terminalAgentsDocSelectEl.appendChild(option);
  terminalAgentsDocSelectEl.value = newPath;

  if (terminalAgentsDocEditorEl) {
    terminalAgentsDocEditorEl.value = "";
  }
  if (terminalAgentsDocPathEl) {
    terminalAgentsDocPathEl.textContent = newPath;
  }
  if (terminalAgentsDocNameInputEl) {
    terminalAgentsDocNameInputEl.value = "";
  }
  updateTerminalAgentsDocStatus("已加入待保存列表。点击保存后写入磁盘。", "info");
  window.requestAnimationFrame(() => {
    focusTextInputToEnd(terminalAgentsDocEditorEl);
  });
}

async function handleRefreshTerminalAgentsDocList() {
  const sessionId = terminalAgentsDocSessionId || state.activeSessionId;
  if (!sessionId) {
    updateTerminalAgentsDocStatus("请先选择一个终端会话。", "warn");
    return;
  }
  const previousPath = terminalAgentsDocPathValue();
  setTerminalAgentsDocBusy(true);
  updateTerminalAgentsDocStatus("正在刷新文档列表…", "info");
  try {
    const payload = await fetchTerminalAgentsDocList(sessionId);
    const documents = Array.isArray(payload?.documents) ? payload.documents : [];
    const stillExists = documents.some((doc) => doc?.path === previousPath);
    const fallback = documents[0]?.path || "AGENTS.MD";
    const nextPath = stillExists ? previousPath : fallback;
    renderTerminalAgentsDocOptions(documents, nextPath);
    if (nextPath !== previousPath) {
      await loadTerminalAgentsDoc(sessionId, nextPath);
    } else {
      updateTerminalAgentsDocStatus("文档列表已刷新。", "ok");
    }
  } catch (error) {
    updateTerminalAgentsDocStatus(error?.message || "刷新文档列表失败。", "warn");
  } finally {
    setTerminalAgentsDocBusy(false);
  }
}

function renderTerminalAgentsDocOptions(documents, selectedPath = "AGENTS.MD") {
  if (!terminalAgentsDocSelectEl) {
    return;
  }

  // 保存服务端按隐藏目录开关返回的列表，过滤输入框变化时复用。
  terminalAgentsDocAllDocuments = Array.isArray(documents) ? documents : [];
  const normalizedSelected = String(selectedPath || "AGENTS.MD").trim() || "AGENTS.MD";
  renderFilteredTerminalAgentsDocOptions(normalizedSelected);
}

function terminalAgentsDocModifiedWithinDays(documentInfo, days, nowSeconds) {
  const normalizedDays = Number(days);
  if (!Number.isInteger(normalizedDays) || normalizedDays <= 0) {
    return true;
  }

  const modifiedSeconds = Number(documentInfo?.modified);
  const referenceSeconds = Number(nowSeconds);
  if (
    !Number.isFinite(modifiedSeconds)
    || modifiedSeconds <= 0
    || !Number.isFinite(referenceSeconds)
  ) {
    return true;
  }

  return modifiedSeconds >= referenceSeconds - normalizedDays * 86_400;
}

// 根据名称和修改天数，从服务端已经限定范围的列表中筛选选项。
function renderFilteredTerminalAgentsDocOptions(selectedPath = null) {
  if (!terminalAgentsDocSelectEl) {
    return;
  }
  const normalizedSelected = String(
    selectedPath || terminalAgentsDocPathValue() || "AGENTS.MD",
  ).trim() || "AGENTS.MD";

  const filterText = (terminalAgentsDocFilterInputEl?.value || "")
    .trim()
    .toLowerCase();
  const maxAgeDays = terminalAgentsDocMaxAgeDaysEl?.value || "";
  const nowSeconds = Date.now() / 1000;
  const filtered = terminalAgentsDocAllDocuments.filter((documentInfo) => {
    const path = String(documentInfo?.path || "").trim();
    if (!path) {
      return false;
    }
    if (filterText) {
      const label = String(documentInfo?.label || path).toLowerCase();
      if (!label.includes(filterText)) {
        return false;
      }
    }
    if (!terminalAgentsDocModifiedWithinDays(documentInfo, maxAgeDays, nowSeconds)) {
      return false;
    }
    return true;
  });

  terminalAgentsDocSelectEl.replaceChildren();
  let hasSelected = false;

  filtered.forEach((documentInfo) => {
    const path = String(documentInfo?.path || "").trim();
    if (!path) {
      return;
    }
    const option = document.createElement("option");
    option.value = path;
    option.textContent = String(documentInfo?.label || path);
    if (!documentInfo?.exists) {
      option.textContent += " (新建)";
    }
    if (path === normalizedSelected) {
      option.selected = true;
      hasSelected = true;
    }
    terminalAgentsDocSelectEl.appendChild(option);
  });

  // 过滤后选中文档不可见时，仍保留一个临时选项，避免丢失当前编辑目标。
  if (!hasSelected) {
    const option = document.createElement("option");
    option.value = normalizedSelected;
    option.textContent = normalizedSelected;
    option.selected = true;
    terminalAgentsDocSelectEl.appendChild(option);
  }
}

function terminalAgentsDocShowHidden() {
  return Boolean(terminalAgentsDocShowHiddenEl?.checked);
}

function terminalAgentsDocRecursiveDirectories() {
  const value = String(terminalAgentsDocRecursiveDirectoriesEl?.value || "").trim();
  return value || "docs";
}

async function openTerminalAgentsDocEditor() {
  const session = activeSession();
  if (!session?.id) {
    updateStatus("请先选择一个终端会话。", "warn");
    return;
  }

  terminalAgentsDocSessionId = session.id;
  if (terminalAgentsDocEditorEl) {
    terminalAgentsDocEditorEl.value = "";
  }
  if (terminalAgentsDocPathEl) {
    terminalAgentsDocPathEl.textContent = "AGENTS.MD";
  }
  if (terminalAgentsDocFilterInputEl) {
    terminalAgentsDocFilterInputEl.value = "";
  }
  if (terminalAgentsDocMaxAgeDaysEl) {
    terminalAgentsDocMaxAgeDaysEl.value = "";
  }
  if (terminalAgentsDocRecursiveDirectoriesEl) {
    terminalAgentsDocRecursiveDirectoriesEl.value = "docs";
  }
  if (terminalAgentsDocShowHiddenEl) {
    terminalAgentsDocShowHiddenEl.checked = false;
  }
  renderTerminalAgentsDocOptions([], "AGENTS.MD");
  updateTerminalAgentsDocStatus("正在读取文档列表…", "info");
  openTerminalAgentsDocDialog();
  setTerminalAgentsDocBusy(true);

  try {
    const payload = await fetchTerminalAgentsDocList(session.id);
    if (terminalAgentsDocSessionId !== session.id) {
      return;
    }
    const selectedPath = payload.documents?.[0]?.path || "AGENTS.MD";
    renderTerminalAgentsDocOptions(payload.documents || [], selectedPath);
    await loadTerminalAgentsDoc(session.id, selectedPath);
  } catch (error) {
    updateTerminalAgentsDocStatus(error?.message || "读取文档列表失败。", "warn");
  } finally {
    if (terminalAgentsDocSessionId === session.id) {
      setTerminalAgentsDocBusy(false);
    }
  }
}

// 拉取文档列表：打开对话框、扫描选项与"刷新"按钮共享同一段算法。
async function fetchTerminalAgentsDocList(
  sessionId,
  showHidden = terminalAgentsDocShowHidden(),
) {
  const query = new URLSearchParams({
    _: String(Date.now()),
    show_hidden: String(Boolean(showHidden)),
    recursive_dirs: terminalAgentsDocRecursiveDirectories(),
  });
  return requestJson(
    `/api/terminal/sessions/${encodeURIComponent(sessionId)}/agents-docs?${query}`,
  );
}

async function loadTerminalAgentsDoc(sessionId, documentPath) {
  const selectedPath = String(documentPath || "AGENTS.MD").trim() || "AGENTS.MD";
  const query = new URLSearchParams({
    path: selectedPath,
    show_hidden: String(terminalAgentsDocShowHidden()),
    recursive_dirs: terminalAgentsDocRecursiveDirectories(),
  });
  updateTerminalAgentsDocStatus("正在读取文档…", "info");
  const payload = await requestJson(
    `/api/terminal/sessions/${encodeURIComponent(sessionId)}/agents-doc?${query}`,
  );
  if (terminalAgentsDocSessionId !== sessionId) {
    return;
  }
  renderTerminalAgentsDocOptions(payload.documents || [], payload.path || selectedPath);
  if (terminalAgentsDocEditorEl) {
    terminalAgentsDocEditorEl.value = payload.content || "";
  }
  if (terminalAgentsDocPathEl) {
    terminalAgentsDocPathEl.textContent = payload.display_path || payload.path || selectedPath;
  }
  updateTerminalAgentsDocStatus(
    payload.exists ? "已读取文档。" : "当前目录没有该文档。",
    payload.exists ? "ok" : "info",
  );
  window.requestAnimationFrame(() => {
    focusTextInputToEnd(terminalAgentsDocEditorEl);
  });
}

async function saveTerminalAgentsDoc() {
  const sessionId = terminalAgentsDocSessionId || state.activeSessionId;
  if (!sessionId) {
    updateTerminalAgentsDocStatus("请先选择一个终端会话。", "warn");
    return;
  }
  if (!terminalAgentsDocEditorEl) {
    updateTerminalAgentsDocStatus("文档编辑器不可用。", "warn");
    return;
  }

  setTerminalAgentsDocBusy(true);
  const documentPath = terminalAgentsDocPathValue();
  updateTerminalAgentsDocStatus("正在保存文档…", "info");
  try {
    const payload = await requestJson(
      `/api/terminal/sessions/${encodeURIComponent(sessionId)}/agents-doc`,
      {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          path: documentPath,
          content: terminalAgentsDocEditorEl.value,
          show_hidden: terminalAgentsDocShowHidden(),
          recursive_dirs: terminalAgentsDocRecursiveDirectories(),
        }),
      },
    );
    renderTerminalAgentsDocOptions(payload.documents || [], payload.path || documentPath);
    if (terminalAgentsDocPathEl) {
      terminalAgentsDocPathEl.textContent = payload.display_path || payload.path || documentPath;
    }
    updateTerminalAgentsDocStatus("已保存文档。", "ok");
  } catch (error) {
    updateTerminalAgentsDocStatus(error?.message || "保存文档失败。", "warn");
  } finally {
    setTerminalAgentsDocBusy(false);
  }
}

async function showTerminalInputHistory() {
  if (!state.activeSessionId) {
    updateStatus("请先选择一个终端会话。", "warn");
    return;
  }

  updateTerminalInputHistoryStatus("", "info");
  terminalInputHistoryLoading = true;
  renderTerminalInputHistory([]);
  openTerminalInputHistoryDialog();

  try {
    const payload = await requestJson(
      `/api/terminal/sessions/${encodeURIComponent(state.activeSessionId)}/input-history`,
    );
    terminalInputHistoryLoading = false;
    renderTerminalInputHistory(payload.entries || []);
  } catch (error) {
    terminalInputHistoryLoading = false;
    renderTerminalInputHistory([]);
    updateTerminalInputHistoryStatus(error?.message || "获取对话历史失败。", "warn");
  }
}

async function copyTerminalInputHistory() {
  const text = terminalInputHistoryEntries
    .map((entry) => String(entry?.text || "").trim())
    .filter(Boolean)
    .join("\n");
  if (!text) {
    updateTerminalInputHistoryStatus("暂无可复制的对话历史。", "warn");
    return;
  }

  const copied = await copyTextToClipboard(text);
  updateTerminalInputHistoryStatus(
    copied ? "已复制对话历史。" : "复制对话历史失败。",
    copied ? "ok" : "warn",
  );
}

function revokeTerminalPasteAssetPreviews() {
  terminalPasteAssetEntries.forEach((entry) => {
    if (entry.previewUrl) {
      URL.revokeObjectURL(entry.previewUrl);
    }
  });
}

function renderTerminalPasteAssets() {
  if (!terminalPasteAssetsEl) {
    return;
  }
  terminalPasteAssetsEl.textContent = "";
  terminalPasteAssetsEl.hidden = terminalPasteAssetEntries.length === 0;
  terminalPasteAssetEntries.forEach((entry, index) => {
    const item = document.createElement("div");
    item.className = "terminal-paste-asset";

    if (entry.previewUrl) {
      const image = document.createElement("img");
      image.src = entry.previewUrl;
      image.alt = entry.name || `clipboard image ${index + 1}`;
      item.appendChild(image);
    }

    const meta = document.createElement("div");
    meta.className = "terminal-paste-asset-meta";
    const title = document.createElement("strong");
    title.textContent = entry.asset?.relative_path || entry.name || `图片 ${index + 1}`;
    const detail = document.createElement("span");
    detail.textContent = `${entry.type || entry.asset?.mime || "image"} · ${formatBytes(entry.size || entry.asset?.size || 0)}`;
    meta.append(title, detail);
    item.appendChild(meta);

    terminalPasteAssetsEl.appendChild(item);
  });
}
