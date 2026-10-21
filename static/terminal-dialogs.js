function prepareTerminalPasteText(text) {
  return String(text).replace(/\r\n|\r|\n/g, "\r");
}

const TERMINAL_AGENTS_DOC_DRAFTS_STORAGE_KEY = "webclx:terminal-document-drafts";
const TERMINAL_AGENTS_DOC_DRAFT_LIMIT = 20;

function readTerminalAgentsDocDrafts() {
  try {
    const parsed = JSON.parse(
      window.sessionStorage.getItem(TERMINAL_AGENTS_DOC_DRAFTS_STORAGE_KEY) || "[]",
    );
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed.filter((draft) => (
      draft
      && typeof draft.sessionId === "string"
      && typeof draft.path === "string"
      && typeof draft.content === "string"
      && Number.isFinite(Number(draft.updatedAt))
    ));
  } catch {
    return [];
  }
}

function writeTerminalAgentsDocDrafts(drafts) {
  const pending = drafts
    .slice()
    .sort((left, right) => Number(right.updatedAt) - Number(left.updatedAt))
    .slice(0, TERMINAL_AGENTS_DOC_DRAFT_LIMIT);
  while (pending.length > 0) {
    try {
      window.sessionStorage.setItem(
        TERMINAL_AGENTS_DOC_DRAFTS_STORAGE_KEY,
        JSON.stringify(pending),
      );
      return true;
    } catch {
      pending.pop();
    }
  }
  try {
    window.sessionStorage.removeItem(TERMINAL_AGENTS_DOC_DRAFTS_STORAGE_KEY);
  } catch {}
  return false;
}

function readTerminalAgentsDocDraft(sessionId, documentPath) {
  const normalizedSessionId = String(sessionId || "").trim();
  const normalizedPath = String(documentPath || "").trim();
  if (!normalizedSessionId || !normalizedPath) {
    return null;
  }
  return readTerminalAgentsDocDrafts().find((draft) => (
    draft.sessionId === normalizedSessionId && draft.path === normalizedPath
  )) || null;
}

function persistTerminalAgentsDocDraft(sessionId, documentPath, content) {
  const normalizedSessionId = String(sessionId || "").trim();
  const normalizedPath = String(documentPath || "").trim();
  if (!normalizedSessionId || !normalizedPath) {
    return false;
  }
  const drafts = readTerminalAgentsDocDrafts().filter((draft) => !(
    draft.sessionId === normalizedSessionId && draft.path === normalizedPath
  ));
  const latestUpdatedAt = drafts.reduce(
    (latest, draft) => Math.max(latest, Number(draft.updatedAt) || 0),
    0,
  );
  drafts.push({
    sessionId: normalizedSessionId,
    path: normalizedPath,
    content: String(content ?? ""),
    updatedAt: Math.max(Date.now(), latestUpdatedAt + 1),
  });
  return writeTerminalAgentsDocDrafts(drafts);
}

function clearTerminalAgentsDocDraft(sessionId, documentPath) {
  const normalizedSessionId = String(sessionId || "").trim();
  const normalizedPath = String(documentPath || "").trim();
  const drafts = readTerminalAgentsDocDrafts().filter((draft) => !(
    draft.sessionId === normalizedSessionId && draft.path === normalizedPath
  ));
  if (drafts.length === 0) {
    try {
      window.sessionStorage.removeItem(TERMINAL_AGENTS_DOC_DRAFTS_STORAGE_KEY);
    } catch {}
    return;
  }
  writeTerminalAgentsDocDrafts(drafts);
}

function latestTerminalAgentsDocDraftPath(sessionId) {
  const normalizedSessionId = String(sessionId || "").trim();
  return readTerminalAgentsDocDrafts()
    .filter((draft) => draft.sessionId === normalizedSessionId)
    .sort((left, right) => Number(right.updatedAt) - Number(left.updatedAt))[0]?.path || "";
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
    const actions = document.createElement("div");
    actions.className = "terminal-input-history-actions";

    const replaceNameButton = document.createElement("button");
    replaceNameButton.className = "button secondary terminal-input-history-copy-item";
    replaceNameButton.type = "button";
    replaceNameButton.textContent = "替换终端名";
    replaceNameButton.setAttribute("aria-label", `用第 ${index + 1} 条对话历史替换终端名`);
    replaceNameButton.addEventListener("click", () => {
      replaceTerminalNameWithInputHistoryEntry(entry, replaceNameButton);
    });
    actions.append(copyButton, replaceNameButton);
    meta.append(metaInfo, actions);

    const text = document.createElement("textarea");
    text.className = "terminal-input-history-text";
    text.readOnly = true;
    text.rows = 4;
    text.spellcheck = false;
    text.setAttribute("aria-label", `第 ${index + 1} 条对话历史`);
    text.value = String(entry?.text || "");
    text.addEventListener("contextmenu", (event) => {
      event.preventDefault();
      event.stopPropagation();
      openTerminalContextMenu(event.clientX, event.clientY, () => {
        const selected = text.value.slice(text.selectionStart, text.selectionEnd);
        return {
          text: selected || text.value,
          label: selected ? "复制选中文本" : "复制本条历史",
          onCopied: (copied) => {
            updateTerminalInputHistoryStatus(
              copied
                ? (selected ? "已复制选中的对话历史。" : "已复制本条对话历史。")
                : (selected ? "复制选中的对话历史失败。" : "复制本条对话历史失败。"),
              copied ? "ok" : "warn",
            );
          },
        };
      }, event.currentTarget.closest("dialog"));
    });

    item.append(meta, text);
    terminalInputHistoryListEl.appendChild(item);
  });
}

function terminalInputHistoryTerminalBaseName(name) {
  const currentName = String(name || "").trim();
  const numberedName = currentName.match(/^(.*?_\d+)(?:_.*)?$/);
  return numberedName?.[1] || currentName;
}

function conversationNameFromHistoryEntry(entry) {
  const text = String(entry?.text || "").trim().replace(/\s+/g, "_");
  return text.replace(/_+$/, "").slice(0, 120);
}

function terminalInputHistoryNameFromEntry(entry) {
  return conversationNameFromHistoryEntry(entry);
}

async function replaceTerminalNameWithInputHistoryEntry(entry, button) {
  const session = activeSession();
  const currentName = String(session?.name || "").trim();
  const conversationName = terminalInputHistoryNameFromEntry(entry);
  if (!session?.id || !currentName) {
    updateTerminalInputHistoryStatus("找不到当前终端，无法替换终端名。", "warn");
    return;
  }
  if (!conversationName) {
    updateTerminalInputHistoryStatus("这条对话历史为空，无法替换终端名。", "warn");
    return;
  }

  const baseName = terminalInputHistoryTerminalBaseName(currentName);
  const nextName = `${baseName}_${conversationName}`.slice(0, 160);
  state.renamingSessionId = session.id;
  openTerminalRenameDialog(nextName, button);
  updateTerminalInputHistoryStatus("已生成建议名称，请在弹窗中修改后保存。", "info");
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
  terminalActionKeyboardPolicyActive = false;
  terminalActionEditableFocusBlocked = false;
  resetTerminalImeFocusContext({ desktopInput: true });
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
    const stillExists = documents.some((doc) => doc?.path === previousPath)
      || Boolean(readTerminalAgentsDocDraft(sessionId, previousPath));
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
    window.requestAnimationFrame(() => {
      focusTextInputToEnd(terminalAgentsDocEditorEl);
    });
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
    const selectedPath = latestTerminalAgentsDocDraftPath(session.id)
      || payload.documents?.[0]?.path
      || "AGENTS.MD";
    renderTerminalAgentsDocOptions(payload.documents || [], selectedPath);
    await loadTerminalAgentsDoc(session.id, selectedPath);
  } catch (error) {
    updateTerminalAgentsDocStatus(error?.message || "读取文档列表失败。", "warn");
  } finally {
    if (terminalAgentsDocSessionId === session.id) {
      setTerminalAgentsDocBusy(false);
      window.requestAnimationFrame(() => {
        focusTextInputToEnd(terminalAgentsDocEditorEl);
      });
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
  const serverContent = payload.content || "";
  const draft = readTerminalAgentsDocDraft(sessionId, payload.path || selectedPath);
  if (terminalAgentsDocEditorEl) {
    if (draft) {
      terminalAgentsDocEditorEl.value = draft.content;
    } else {
      terminalAgentsDocEditorEl.value = serverContent;
    }
  }
  if (terminalAgentsDocPathEl) {
    terminalAgentsDocPathEl.textContent = payload.display_path || payload.path || selectedPath;
  }
  if (draft && draft.content !== serverContent) {
    updateTerminalAgentsDocStatus("已恢复未保存的编辑内容。", "info");
  } else {
    if (draft) {
      clearTerminalAgentsDocDraft(sessionId, payload.path || selectedPath);
    }
    updateTerminalAgentsDocStatus(
      payload.exists ? "已读取文档。" : "当前目录没有该文档。",
      payload.exists ? "ok" : "info",
    );
  }
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
    clearTerminalAgentsDocDraft(sessionId, documentPath);
    if (terminalAgentsDocPathEl) {
      terminalAgentsDocPathEl.textContent = payload.display_path || payload.path || documentPath;
    }
    updateTerminalAgentsDocStatus("已保存文档。", "ok");
  } catch (error) {
    updateTerminalAgentsDocStatus(error?.message || "保存文档失败。", "warn");
  } finally {
    setTerminalAgentsDocBusy(false);
    window.requestAnimationFrame(() => {
      focusTextInputToEnd(terminalAgentsDocEditorEl);
    });
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

// ---- 终端消息：把终端消息 API 和终端分组结合起来 ----
// 目标按终端名称列出，支持一键选中整个分组；消息逐个目标发送，并按终端汇报
// bracketed-paste + rollout 确认的投递结果。
const TERMINAL_MESSAGE_MAX_TARGETS = 32;

function terminalMessageSessionId(session) {
  return String(session?.id || "").trim();
}

function terminalMessageSessionName(session) {
  return String(session?.name || "").trim() || terminalMessageSessionId(session);
}

function terminalMessageGroupToken(session) {
  return String(session?.group_token || session?.groupToken || "").trim();
}

function terminalMessageGroupName(session) {
  return String(session?.group_name || session?.groupName || "").trim();
}

// 分组完全由会话列表推导：成员共用 group_token，两个以上会话持有同一 token
// 才算一个分组，因此不需要额外的分组注册中心。
function terminalMessageGroups(sessions) {
  const byToken = new Map();
  for (const session of Array.isArray(sessions) ? sessions : []) {
    const id = terminalMessageSessionId(session);
    const token = terminalMessageGroupToken(session);
    if (!id || !token) continue;
    if (!byToken.has(token)) {
      byToken.set(token, { token, name: "", color: "", members: [] });
    }
    const group = byToken.get(token);
    if (!group.name) group.name = terminalMessageGroupName(session);
    if (!group.color) {
      const color = String(session?.group_color || session?.groupColor || "").trim();
      group.color = /^#[0-9a-fA-F]{6}$/.test(color) ? color.toLowerCase() : "";
    }
    group.members.push(session);
  }
  return [...byToken.values()]
    .filter((group) => group.members.length > 1)
    .map((group) => ({
      ...group,
      roles: terminalMessageGroupRoles(group),
    }))
    .sort((left, right) => terminalMessageGroupLabel(left).localeCompare(
      terminalMessageGroupLabel(right),
      "zh-Hans-CN",
      { numeric: true },
    ));
}

function terminalMessageGroupRoleKey(role) {
  const normalized = String(role || "").trim().toLowerCase();
  if (["manager", "管理者", "管理", "计划者", "planner"].includes(normalized)) return "manager";
  if (["executor", "执行者", "执行", "worker"].includes(normalized)) return "executor";
  if (["reviewer", "复核者", "复核"].includes(normalized)) return "reviewer";
  return "";
}

function terminalMessageGroupRoles(group) {
  const members = Array.isArray(group?.members) ? group.members : [];
  // 优先使用用户在分组对话框里指定的角色；没有人被指定为管理者时，
  // 才按成员顺序把第一个成员当作管理者。
  const designated = members.find(
    (member) => terminalMessageGroupRoleKey(member.group_role) === "manager",
  );
  const manager = designated || members[0] || null;
  return new Map(members.map((member) => [
    terminalMessageSessionId(member),
    terminalMessageGroupRoleKey(member.group_role)
      || (member.id === manager?.id ? "manager" : "executor"),
  ]));
}

function terminalMessageGroupLabel(group) {
  const name = String(group?.name || "").trim();
  if (name) return name;
  const members = Array.isArray(group?.members) ? group.members : [];
  if (members.length === 0) return "未命名分组";
  return members.map(terminalMessageSessionName).join("、");
}

function terminalMessageGroupMemberIds(group) {
  return (Array.isArray(group?.members) ? group.members : [])
    .map(terminalMessageSessionId)
    .filter(Boolean);
}

// 目标行保留自己所属的分组，这样列表既能显示组名徽标，也能整组勾选。
function terminalMessageTargets(sessions) {
  const list = Array.isArray(sessions) ? sessions : [];
  const groupByToken = new Map(
    terminalMessageGroups(list).map((group) => [group.token, group]),
  );
  return list
    .filter((session) => Boolean(terminalMessageSessionId(session)))
    .map((session) => {
      const token = terminalMessageGroupToken(session);
      const group = groupByToken.get(token) || null;
      const role = group?.roles?.get(terminalMessageSessionId(session)) || "";
      const counterpart = Array.from(group?.roles || [])
        .map(([id, memberRole]) => {
          const member = (group?.members || []).find((item) => terminalMessageSessionId(item) === id);
          return { id, name: terminalMessageSessionName(member), role: memberRole };
        })
        .find((member) => member.id && member.role && member.role !== role);
      const members = Array.from(group?.roles || []).map(([id, memberRole]) => {
        const member = (group?.members || []).find((item) => terminalMessageSessionId(item) === id);
        return { id, name: terminalMessageSessionName(member), role: memberRole };
      });
      return {
        id: terminalMessageSessionId(session),
        name: terminalMessageSessionName(session),
        agent: String(session?.activity_agent || "").trim(),
        idle: Boolean(session?.idle),
        groupToken: group ? token : "",
        groupLabel: group ? terminalMessageGroupLabel(group) : "",
        groupRole: role,
        groupRoleName: terminalGroupRoleLabel(role),
        groupRoleMembers: members,
        groupRoleCounterpart: counterpart ? { name: counterpart.name, role: counterpart.role } : null,
        groupColor: group?.color || "",
      };
    });
}

function terminalMessageRecipients(targets, selectedIds) {
  const wanted = new Set((selectedIds || []).map((id) => String(id)));
  const deduped = new Map();
  for (const target of targets || []) {
    if (!wanted.has(target.id) || deduped.has(target.id)) continue;
    deduped.set(target.id, { id: target.id, name: target.name });
  }
  const recipients = [...deduped.values()].slice(0, TERMINAL_MESSAGE_MAX_TARGETS);
  return {
    recipients,
    error: recipients.length === 0 ? "请至少选择一个目标终端。" : "",
  };
}

function terminalMessageBody(options) {
  const {
    target,
    message,
    deliveryId,
    submit = true,
    verifySubmission = true,
  } = options;
  const submitEnters = submit ? 1 : 0;
  const body = {
    target,
    data: message,
    submit,
    submit_enters: submitEnters,
    bracketed_paste: true,
    verify_submission: verifySubmission && submitEnters > 0,
  };
  if (body.verify_submission) {
    body.delivery_id = deliveryId || target;
  }
  return body;
}

function terminalMessageBatchId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  return `message-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function terminalMessageSenderTag(senderName) {
  const name = String(senderName || "").trim();
  return name ? `[from ${name}] ` : "";
}

// 结果只看真实投递证据：HTTP 200 不代表目标终端收到了消息。
function terminalMessageDeliveryState(result) {
  if (result?.submitted === true) return "confirmed";
  if (result?.error) return "failed";
  return "unconfirmed";
}

function terminalMessageSummary(recipients) {
  const rows = recipients.map((item) => ({
    name: item.name || item.id,
    state: terminalMessageDeliveryState(item),
    error: item.error || "",
  }));
  const confirmed = rows.filter((row) => row.state === "confirmed").length;
  const failed = rows.filter((row) => row.state === "failed");
  const unconfirmed = rows.filter((row) => row.state === "unconfirmed");
  const delivered = rows.filter((row) => row.state !== "failed");
  const lines = [
    `已确认投递：${confirmed}/${rows.length}`,
    `已送达：${delivered.map((row) => row.name).join("、") || "无"}`,
  ];
  lines.push(failed.length
    ? `失败：${failed.map((row) => `${row.name}（${row.error || "未知错误"}）`).join("、")}`
    : "失败：无");
  if (unconfirmed.length) {
    lines.push(`未获 Agent 确认：${unconfirmed.map((row) => row.name).join("、")}`);
  }
  return { confirmed, failed: failed.length, total: rows.length, text: lines.join("\n") };
}

function terminalCollaborationBrief({
  manager,
  executors = [],
  groupName = "",
  task = "",
  skillName = "",
} = {}) {
  const managerName = String(manager?.name || manager || "").trim();
  const normalizedExecutors = (Array.isArray(executors) ? executors : [])
    .map((executor) => String(executor?.name || executor || "").trim())
    .filter(Boolean);
  const normalizedTask = String(task || "").trim();
  const normalizedSkill = String(skillName || "").trim();
  if (!managerName || normalizedExecutors.length === 0 || !normalizedTask) return "";
  const targetList = normalizedExecutors.map((name) => `「${name}」`).join("、");
  const lines = [
    `[系统消息][群组:${groupName || "terminal-group"}] 已建立协作分组。发起终端：「${managerName}」。`,
    `你当前角色是执行者；本组执行者：${targetList}。这是系统配置通知，不是新任务授权。`,
  ];
  if (normalizedSkill) {
    lines.push(`先用 $${normalizedSkill} 技能加载工作须知，再用终端定时消息流程回报。协作消息一律通过 /api/terminal/scheduled-inputs 设置未来短延时发送（实时消息也延迟数秒），不得同步等待投递返回；排程成功不等于接收，以对方定时 ACK 或原生记录确认，不重复发送正文。`);
  }
  lines.push(`系统说明：${normalizedTask}`);
  return lines.filter(Boolean).join(" ");
}

function terminalSessionGroups(sessions) {
  return terminalMessageGroups(sessions);
}

function currentTerminalMessageGroup(sessions) {
  const groups = terminalMessageGroups(sessions);
  return groups.find((group) => group.members.some(
    (member) => terminalMessageSessionId(member) === String(state.activeSessionId || ""),
  )) || null;
}

let terminalMessageTargetRows = [];

function terminalMessageDom() {
  return {
    dialog: document.getElementById("terminal-message-dialog"),
    form: document.getElementById("terminal-message-form"),
    targets: document.getElementById("terminal-message-targets"),
    groupActions: document.getElementById("terminal-message-group-actions"),
    message: document.getElementById("terminal-message-text"),
    identify: document.getElementById("terminal-message-identify"),
    verify: document.getElementById("terminal-message-verify"),
    selected: document.getElementById("terminal-message-selected"),
    status: document.getElementById("terminal-message-status"),
    result: document.getElementById("terminal-message-result"),
    send: document.getElementById("terminal-message-send"),
    close: document.getElementById("terminal-message-close"),
  };
}

function setTerminalMessageStatus(message, tone = "info") {
  const status = terminalMessageDom().status;
  if (!status) return;
  status.hidden = !message;
  status.dataset.tone = tone;
  status.textContent = message;
}

function terminalMessageSelectedIds() {
  const host = terminalMessageDom().targets;
  if (!host) return [];
  return Array.from(
    host.querySelectorAll("input[type=checkbox]:checked"),
    (input) => input.value,
  );
}

function setTerminalMessageTargetChecked(id, checked) {
  const input = terminalMessageDom().targets
    ?.querySelector(`input[type=checkbox][value="${CSS.escape(id)}"]`);
  if (input) input.checked = checked;
}

function syncTerminalMessageSelectionCount() {
  const label = terminalMessageDom().selected;
  if (!label) return;
  const count = terminalMessageSelectedIds().length;
  label.textContent = count ? `已选 ${count} 个目标` : "未选择目标";
}

function renderTerminalMessageGroupActions(rows) {
  const host = terminalMessageDom().groupActions;
  if (!host) return;
  host.textContent = "";
  const groups = terminalMessageGroups(rows
    .filter((row) => row.groupToken)
    .map((row) => ({
      id: row.id,
      name: row.name,
      group_token: row.groupToken,
      group_name: row.groupLabel,
    })));
  if (groups.length === 0) return;
  for (const group of groups) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "terminal-message-group-button";
    button.textContent = `选中整组：${terminalMessageGroupLabel(group)}（${group.members.length}）`;
    button.addEventListener("click", () => {
      const ids = terminalMessageGroupMemberIds(group);
      const selected = terminalMessageSelectedIds();
      const allSelected = ids.every((id) => selected.includes(id));
      ids.forEach((id) => setTerminalMessageTargetChecked(id, !allSelected));
      syncTerminalMessageSelectionCount();
    });
    host.append(button);
  }
  const clear = document.createElement("button");
  clear.type = "button";
  clear.className = "terminal-message-group-button terminal-message-group-clear";
  clear.textContent = "清空选择";
  clear.addEventListener("click", () => {
    terminalMessageSelectedIds().forEach((id) => setTerminalMessageTargetChecked(id, false));
    syncTerminalMessageSelectionCount();
  });
  host.append(clear);
}

function renderTerminalMessageTargets(sessions, activeSessionId) {
  const host = terminalMessageDom().targets;
  if (!host) return;
  terminalMessageTargetRows = terminalMessageTargets(sessions);
  host.textContent = "";
  if (terminalMessageTargetRows.length === 0) {
    host.textContent = "没有可用的终端。";
    renderTerminalMessageGroupActions([]);
    syncTerminalMessageSelectionCount();
    return;
  }
  for (const target of terminalMessageTargetRows) {
    const option = document.createElement("label");
    option.className = "terminal-message-target terminal-group-session-option";
    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.value = target.id;
    checkbox.checked = target.id === activeSessionId;
    checkbox.addEventListener("change", syncTerminalMessageSelectionCount);
    option.append(checkbox);
    const text = document.createElement("span");
    text.className = "terminal-group-session-name";
    text.textContent = target.name;
    const detail = document.createElement("small");
    const counterpart = target.groupRoleCounterpart
      ? `${terminalGroupRoleLabel(target.groupRoleCounterpart.role) || "成员"}:${target.groupRoleCounterpart.name}`
      : "";
    detail.textContent = [
      target.id,
      target.agent || "shell",
      target.idle ? "闲置" : "",
      target.groupRoleName || "",
      target.id === activeSessionId ? "当前" : "",
      counterpart,
    ].filter(Boolean).join(" · ");
    text.append(detail);
    option.append(text);
    if (target.groupLabel) {
      const badge = document.createElement("span");
      badge.className = "terminal-message-target-group";
      badge.textContent = target.groupLabel;
      if (target.groupColor) {
        badge.style.setProperty("--terminal-group-color", target.groupColor);
      }
      option.append(badge);
    }
    host.append(option);
  }
  renderTerminalMessageGroupActions(terminalMessageTargetRows);
  syncTerminalMessageSelectionCount();
}

async function loadTerminalMessageTargets() {
  const response = await requestJson("/api/terminal/sessions?all=true");
  const sessions = Array.isArray(response?.sessions) ? response.sessions : [];
  renderTerminalMessageTargets(sessions, state.activeSessionId);
}

async function openTerminalMessageDialog() {
  const refs = terminalMessageDom();
  if (!refs.dialog) return;
  refs.result.hidden = true;
  setTerminalMessageStatus("正在读取终端…");
  if (typeof refs.dialog.showModal === "function") {
    if (!refs.dialog.open) refs.dialog.showModal();
  } else {
    refs.dialog.setAttribute("open", "");
  }
  try {
    await loadTerminalMessageTargets();
    setTerminalMessageStatus("");
  } catch (error) {
    setTerminalMessageStatus(`读取终端失败：${error.message}`, "warn");
  }
}

async function sendTerminalMessageToTarget(target, message, batchId, verifySubmission) {
  const deliveryId = `${batchId}:${target.id}`;
  try {
    const response = await requestJson("/api/terminal/sessions/message", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(terminalMessageBody({
        target: target.id,
        message,
        deliveryId,
        submit: true,
        verifySubmission,
      })),
    });
    return {
      ...target,
      submitted: response?.submitted === true,
      error: response?.submitted === true ? "" : "提交未获终端确认",
    };
  } catch (error) {
    return { ...target, submitted: false, error: error.message };
  }
}

async function submitTerminalMessage(event) {
  event.preventDefault();
  const refs = terminalMessageDom();
  const message = refs.message.value.trim();
  if (!message) {
    setTerminalMessageStatus("请输入要发送的消息。", "warn");
    refs.message.focus();
    return;
  }
  const selected = terminalMessageSelectedIds();
  if (selected.length > TERMINAL_MESSAGE_MAX_TARGETS) {
    setTerminalMessageStatus(
      `一次最多向 ${TERMINAL_MESSAGE_MAX_TARGETS} 个终端发送消息。`,
      "warn",
    );
    return;
  }
  const { recipients, error } = terminalMessageRecipients(terminalMessageTargetRows, selected);
  if (error) {
    setTerminalMessageStatus(error, "warn");
    return;
  }
  const identify = refs.identify?.checked !== false;
  const verifySubmission = refs.verify?.checked !== false;
  const prefix = identify ? terminalMessageSenderTag(activeSession()?.name || "") : "";
  const batchId = terminalMessageBatchId();
  refs.result.hidden = true;
  setTerminalMessageStatus(`正在向 ${recipients.length} 个终端发送消息…`);
  refs.form.querySelectorAll("input, textarea, button").forEach((input) => {
    input.disabled = true;
  });
  try {
    const results = await Promise.all(recipients.map(
      (target) => sendTerminalMessageToTarget(target, `${prefix}${message}`, batchId, verifySubmission),
    ));
    const summary = terminalMessageSummary(results);
    setTerminalMessageStatus(
      summary.failed
        ? `已送达 ${summary.total - summary.failed}/${summary.total} 个终端；${summary.failed} 个失败。`
        : `已向 ${summary.total} 个终端发送消息${verifySubmission ? `，${summary.confirmed} 个已确认` : ""}。`,
      summary.failed ? "warn" : "ok",
    );
    refs.result.hidden = false;
    refs.result.textContent = [`发送批次：${batchId}`, summary.text].join("\n");
  } finally {
    refs.form.querySelectorAll("input, textarea, button").forEach((input) => {
      input.disabled = false;
    });
    syncTerminalMessageSelectionCount();
  }
}

let terminalMessageTrigger = null;

function attachTerminalMessageUi() {
  const refs = terminalMessageDom();
  if (!refs.dialog || !refs.form) return;
  refs.form.addEventListener("submit", submitTerminalMessage);
  refs.close?.addEventListener("click", () => {
    refs.dialog.close();
    terminalMessageTrigger?.focus?.({ preventScroll: true });
    terminalMessageTrigger = null;
  });
  document.getElementById("terminal-message-open")?.addEventListener("click", (event) => {
    terminalMessageTrigger = event.currentTarget;
    openTerminalMessageDialog();
  });
}

if (typeof document !== "undefined") {
  attachTerminalMessageUi();
}

if (typeof module === "object" && module.exports) {
  module.exports = {
    TERMINAL_MESSAGE_MAX_TARGETS,
    terminalMessageGroups,
    terminalMessageGroupLabel,
    terminalMessageGroupMemberIds,
    terminalMessageGroupRoleKey,
    terminalCollaborationBrief,
    terminalGroupRoleLabel,
    terminalMessageTargets,
    terminalMessageRecipients,
    terminalMessageBody,
    terminalMessageBatchId,
    terminalMessageSenderTag,
    terminalMessageDeliveryState,
    terminalMessageSummary,
  };
}

function terminalGroupRoleLabel(role) {
  const normalized = String(role || "").trim();
  if (normalized === "manager" || normalized === "管理者") return "管理者";
  if (normalized === "reviewer" || normalized === "复核者") return "复核者";
  if (normalized === "executor" || normalized === "执行者") return "执行者";
  return "";
}
