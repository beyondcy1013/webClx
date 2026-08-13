// webClx terminal paste, clipboard, image upload, and scheduled paste helpers.
// Extracted from terminal.js as global function declarations.
// Contains no top-level setup; keep focus/IME helpers in terminal.js.

const TERMINAL_AUTO_CONTINUE_TASK_NOTIFY_STORAGE_KEY =
  "webclx:terminal-auto-continue-task-notifications";
const TERMINAL_AUTO_CONTINUE_TASK_NOTIFY_TTL_MS = 48 * 60 * 60 * 1000;

function terminalPasteAssetPath(asset) {
  return String(asset?.relative_path || asset?.path || asset?.markdown || "").trim();
}

function terminalPasteAssetsPromptText(assets) {
  const paths = (Array.isArray(assets) ? assets : [])
    .map(terminalPasteAssetPath)
    .filter(Boolean);
  if (paths.length === 0) {
    return "";
  }
  if (paths.length === 1) {
    return `请查看这张图片文件：${paths[0]}`;
  }
  return `请查看这些图片文件：\n${paths.map((path) => `- ${path}`).join("\n")}`;
}

function formatBytes(value) {
  const bytes = Number(value);
  if (!Number.isFinite(bytes) || bytes <= 0) {
    return "0 B";
  }
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  if (bytes < 1024 * 1024) {
    return `${Math.round(bytes / 102.4) / 10} KB`;
  }
  return `${Math.round(bytes / 1024 / 102.4) / 10} MB`;
}

function pasteImageType(type) {
  return String(type || "").split(";")[0].trim().toLowerCase();
}

function isTerminalPasteImageType(type) {
  return TERMINAL_PASTE_IMAGE_TYPES.has(pasteImageType(type));
}

function appendTerminalPasteTextValue(current, text) {
  if (typeof text !== "string" || text.length === 0) {
    return current || "";
  }
  const base = String(current || "");
  const separator = base && !base.endsWith("\n") && !text.startsWith("\n") ? "\n" : "";
  return `${base}${separator}${text}`;
}

function appendTerminalPasteText(text) {
  if (!terminalPasteTextEl || typeof text !== "string" || text.length === 0) {
    return;
  }
  terminalPasteTextEl.value = appendTerminalPasteTextValue(terminalPasteTextEl.value, text);
}

function normalizePastedText(text) {
  return String(text || "")
    .replace(/\r\n/g, "\n")
    .replace(/\r/g, "\n")
    .trim();
}

function pushTerminalPasteTextPart(parts, text) {
  const normalized = normalizePastedText(text);
  if (normalized) {
    parts.push({ type: "text", text: normalized });
  }
}

function htmlToPasteParts(html, imageBlobs = []) {
  if (!html) {
    return [];
  }

  const parts = [];
  const doc = new DOMParser().parseFromString(html, "text/html");
  const blockTags = new Set(["address", "article", "aside", "blockquote", "div", "footer", "h1", "h2", "h3", "h4", "h5", "h6", "header", "li", "main", "nav", "ol", "p", "pre", "section", "table", "tr", "ul"]);
  let textBuffer = "";
  let imageIndex = 0;

  const appendLineBreak = () => {
    if (textBuffer && !textBuffer.endsWith("\n")) {
      textBuffer += "\n";
    }
  };
  const flushText = () => {
    pushTerminalPasteTextPart(parts, textBuffer);
    textBuffer = "";
  };
  const walk = (node) => {
    if (node.nodeType === Node.TEXT_NODE) {
      textBuffer += node.nodeValue || "";
      return;
    }
    if (node.nodeType !== Node.ELEMENT_NODE) {
      return;
    }

    const tagName = node.nodeName.toLowerCase();
    if (tagName === "br") {
      textBuffer += "\n";
      return;
    }
    if (tagName === "img") {
      flushText();
      if (imageIndex < imageBlobs.length) {
        parts.push({ type: "images", blobs: [imageBlobs[imageIndex]] });
        imageIndex += 1;
      }
      return;
    }

    const block = blockTags.has(tagName);
    if (block) {
      appendLineBreak();
    }
    node.childNodes.forEach(walk);
    if (block) {
      appendLineBreak();
    }
  };

  doc.body?.childNodes.forEach(walk);
  flushText();
  if (imageIndex < imageBlobs.length) {
    parts.push({ type: "images", blobs: imageBlobs.slice(imageIndex) });
  }
  return parts;
}

function textFromHtml(html) {
  if (!html) {
    return "";
  }
  return htmlToPasteParts(html)
    .filter((part) => part.type === "text")
    .map((part) => part.text)
    .join("\n");
}

async function clipboardItemsToPasteParts(items) {
  let html = "";
  let plain = "";
  const imageBlobs = [];
  for (const item of items || []) {
    if (!item) {
      continue;
    }
    if (item.types?.includes("text/html")) {
      const blob = await item.getType("text/html");
      html = html || (await blob.text());
    } else if (item.types?.includes("text/plain")) {
      const blob = await item.getType("text/plain");
      plain = plain || (await blob.text());
    }

    for (const type of item.types || []) {
      if (isTerminalPasteImageType(type)) {
        imageBlobs.push(await item.getType(type));
      }
    }
  }

  if (html) {
    return htmlToPasteParts(html, imageBlobs);
  }

  const parts = [];
  pushTerminalPasteTextPart(parts, plain);
  if (imageBlobs.length > 0) {
    parts.push({ type: "images", blobs: imageBlobs });
  }
  return parts;
}

function dataTransferToPasteParts(dataTransfer) {
  const html = dataTransfer?.getData("text/html") || "";
  const plain = dataTransfer?.getData("text/plain") || "";
  const imageBlobs = [];
  for (const item of dataTransfer?.items || []) {
    if (item.kind === "file" && isTerminalPasteImageType(item.type)) {
      const file = item.getAsFile();
      if (file) {
        imageBlobs.push(file);
      }
    }
  }
  for (const file of dataTransfer?.files || []) {
    if (isTerminalPasteImageType(file.type) && !imageBlobs.includes(file)) {
      imageBlobs.push(file);
    }
  }

  if (html) {
    const htmlParts = htmlToPasteParts(html, imageBlobs);
    if (htmlParts.length > 0) {
      return htmlParts;
    }
  }

  const parts = [];
  pushTerminalPasteTextPart(parts, plain);
  if (imageBlobs.length > 0) {
    parts.push({ type: "images", blobs: imageBlobs });
  }
  return parts;
}

async function uploadTerminalPasteImages(blobs) {
  const session = activeSession();
  if (!session?.id) {
    throw new Error("请先选择一个终端会话。");
  }

  const formData = new FormData();
  blobs.forEach((blob, index) => {
    const type = pasteImageType(blob.type) || "image/png";
    const extension = type.split("/")[1] || "png";
    const name = blob.name || `clipboard-${index + 1}.${extension === "jpeg" ? "jpg" : extension}`;
    formData.append("files", blob, name);
  });

  const response = await fetch(`/api/terminal/sessions/${encodeURIComponent(session.id)}/paste-assets`, {
    method: "POST",
    body: formData,
  });
  if (response.status === 401) {
    redirectToLogin();
    throw new Error("未登录，正在跳转登录页");
  }
  if (!response.ok) {
    const message = await response.text();
    throw new Error(message || `上传图片失败: ${response.status}`);
  }
  const payload = await response.json();
  return Array.isArray(payload.assets) ? payload.assets : [];
}

async function terminalPastePartsToText(parts, { collectDialogAssets = false } = {}) {
  let text = "";
  let imageCount = 0;

  for (const part of parts) {
    if (part.type === "text") {
      text = appendTerminalPasteTextValue(text, part.text);
    } else if (part.type === "images" && part.blobs?.length) {
      const assets = await uploadTerminalPasteImages(part.blobs);
      imageCount += assets.length;
      if (collectDialogAssets) {
        assets.forEach((asset, index) => {
          const blob = part.blobs[index];
          terminalPasteAssetEntries.push({
            asset,
            name: asset.name,
            type: asset.mime || blob?.type,
            size: asset.size || blob?.size,
            previewUrl: blob ? URL.createObjectURL(blob) : "",
          });
        });
      }
      text = appendTerminalPasteTextValue(text, terminalPasteAssetsPromptText(assets));
    }
  }

  return { text, imageCount };
}

async function applyTerminalPasteParts(parts, { openDialog = true, sendImmediately = false } = {}) {
  if (!parts.length) {
    updateStatus("剪贴板为空。", "warn");
    return false;
  }
  if (openDialog && !terminalPasteDialogEl?.open) {
    openTerminalPasteDialog();
  }

  setTerminalPasteBusy(true);
  let imageCount = 0;
  try {
    const prepared = await terminalPastePartsToText(parts, { collectDialogAssets: true });
    imageCount = prepared.imageCount;
    appendTerminalPasteText(prepared.text);
  } finally {
    setTerminalPasteBusy(false);
  }

  renderTerminalPasteAssets();
  updateStatus(imageCount ? `已上传 ${imageCount} 张图片，可粘贴到终端。` : "已读取剪贴板文本。", "ok");
  if (sendImmediately) {
    return submitTerminalPasteDialog();
  }
  terminalPasteTextEl?.focus();
  return true;
}

async function pasteTerminalPartsDirectly(parts, options = {}) {
  if (!parts.length) {
    updateStatus(options.emptyMessage || "剪贴板为空。", "warn");
    return false;
  }

  cancelNewSessionQuickStart();
  updateStatus(options.progressMessage || "正在上传剪贴板图片…", "info");
  setClipboardPasteBusy(true);
  try {
    const prepared = await terminalPastePartsToText(parts);
    const sent = sendPastedText(prepared.text, options.emptyMessage || "剪贴板为空。", {
      forceBracketedPaste: true,
    });
    if (sent && prepared.imageCount > 0) {
      updateStatus(`已上传 ${prepared.imageCount} 张图片，并粘贴到终端输入区。`, "ok");
    }
    return sent;
  } finally {
    setClipboardPasteBusy(false);
  }
}

function openTerminalImageUploadPicker() {
  cancelNewSessionQuickStart();
  if (!ensureTerminalReadyForInput()) {
    return false;
  }
  if (!terminalImageUploadInputEl) {
    updateStatus("当前页面不支持选择图片。", "warn");
    return false;
  }
  terminalImageUploadInputEl.value = "";
  terminalImageUploadInputEl.click();
  return true;
}

async function handleTerminalImageUploadSelection() {
  const files = Array.from(terminalImageUploadInputEl?.files || []);
  if (terminalImageUploadInputEl) {
    terminalImageUploadInputEl.value = "";
  }
  if (files.length === 0) {
    return;
  }

  try {
    await pasteTerminalPartsDirectly(
      [{ type: "images", blobs: files }],
      {
        emptyMessage: "未选择图片。",
        progressMessage: "正在上传所选图片…",
      },
    );
  } catch (error) {
    updateStatus(error?.message || "上传图片失败。", "warn");
  }
}

function sendPastedText(text, emptyMessage = "剪贴板为空。", options = {}) {
  if (!ensureTerminalReadyForInput()) {
    return false;
  }

  if (typeof text !== "string" || text.length === 0) {
    updateStatus(emptyMessage, "warn");
    return false;
  }

  cancelNewSessionQuickStart();
  const multiline = hasTerminalPasteLineBreak(text);
  const bracketMultilinePaste = multiline && options.forceBracketedPaste !== false;
  if (bracketMultilinePaste) {
    const prepared = prepareTerminalPasteText(text);
    sendTerminalInput(wrapBracketedTerminalPaste(prepared));
  } else if (typeof term.paste === "function") {
    term.paste(text);
  } else {
    sendTerminalInput(prepareTerminalPasteText(text));
  }
  refreshTerminalInputVisibilityAfterPaste();
  updateStatus(bracketMultilinePaste ? "已安全粘贴多行内容到终端输入区。" : "已粘贴到终端输入区。", "ok");
  focusTerminalAfterSoftKeyboardInput();
  return true;
}

function submitTerminalPasteDialog() {
  if (!terminalPasteTextEl) {
    return false;
  }

  const sent = sendPastedText(terminalPasteTextEl.value, "请先粘贴要发送的内容。", {
    forceBracketedPaste: true,
  });
  if (sent) {
    closeTerminalPasteDialog();
  }
  return sent;
}

function submitTerminalPasteDialogAndSend() {
  if (!terminalPasteTextEl) {
    return false;
  }

  const text = terminalPasteTextEl.value;
  const multiline = hasTerminalPasteLineBreak(text);
  const sent = sendPastedText(text, "请先粘贴要发送的内容。", {
    forceBracketedPaste: true,
  });
  if (!sent) {
    return false;
  }

  sendTerminalInput(MOBILE_KEY_SEQUENCES.enter);
  updateStatus(multiline ? "已安全粘贴多行内容并发送回车。" : "已粘贴到终端输入区并发送回车。", "ok");
  closeTerminalPasteDialog();
  focusTerminalAfterSoftKeyboardInput();
  return true;
}

function normalizeTerminalPasteScheduledTask(task) {
  if (!task) {
    return null;
  }
  const taskId = String(task.taskId || task.id || "").trim();
  const dueAt = Number(task.dueAt ?? task.due_at ?? task.due_at_millis);
  if (!taskId || !Number.isFinite(dueAt)) {
    return null;
  }
  return {
    taskId,
    sessionId: String(task.sessionId || task.session_id || ""),
    terminalName: String(task.terminalName || task.terminal_name || ""),
    at: dueAt,
    label: String(task.label || ""),
    preview: String(task.preview || ""),
  };
}

function normalizeTerminalAutoContinueScheduledTask(task) {
  if (!task) {
    return null;
  }
  const sessionId = String(task.session_id || task.sessionId || "").trim();
  const marker = String(task.marker || "").trim();
  const signature = String(task.signature || "").trim();
  const dueEpoch = Number(task.due_epoch ?? task.dueEpoch ?? 0);
  const taskId =
    marker ||
    [sessionId, signature, Number.isFinite(dueEpoch) && dueEpoch > 0 ? dueEpoch : ""]
      .filter(Boolean)
      .join(":");
  if (!sessionId || !taskId) {
    return null;
  }
  return {
    taskId,
    sessionId,
    terminalName: String(task.webclx_terminal_name || task.session_name || task.terminalName || ""),
    at: Number.isFinite(dueEpoch) && dueEpoch > 0 ? dueEpoch * 1000 : 0,
    label: String(task.task_label || task.task_kind || "自动继续"),
    taskKind: String(task.task_kind || "auto_continue"),
  };
}

function terminalAutoContinueTaskNotifyKey(task) {
  const sessionId = String(task?.sessionId || "").trim();
  const taskKind = String(task?.taskKind || task?.label || "auto_continue").trim();
  const dueAt = Number(task?.at || 0);
  const stableTime = Number.isFinite(dueAt) && dueAt > 0 ? String(dueAt) : "";
  const fallbackTaskId = stableTime ? "" : String(task?.taskId || "").trim();
  const parts = [sessionId, taskKind, stableTime || fallbackTaskId].filter(Boolean);
  return parts.length >= 3 ? parts.join("\n") : "";
}

function readTerminalAutoContinueTaskNotifyAcks() {
  try {
    const parsed = JSON.parse(
      window.localStorage.getItem(TERMINAL_AUTO_CONTINUE_TASK_NOTIFY_STORAGE_KEY) || "{}",
    );
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  } catch (_error) {
    return {};
  }
}

function writeTerminalAutoContinueTaskNotifyAcks(acks) {
  try {
    window.localStorage.setItem(
      TERMINAL_AUTO_CONTINUE_TASK_NOTIFY_STORAGE_KEY,
      JSON.stringify(acks),
    );
  } catch (_error) {
    // The chip still updates; only duplicate-toast suppression is best effort.
  }
}

function pruneTerminalAutoContinueTaskNotifyAcks(acks, now = Date.now()) {
  Object.entries(acks).forEach(([key, value]) => {
    const notifiedAt = Number(value || 0);
    if (
      !Number.isFinite(notifiedAt) ||
      notifiedAt <= 0 ||
      now - notifiedAt >= TERMINAL_AUTO_CONTINUE_TASK_NOTIFY_TTL_MS
    ) {
      delete acks[key];
    }
  });
  return acks;
}

function filterUnnotifiedTerminalAutoContinueTasks(tasks, now = Date.now()) {
  const acks = pruneTerminalAutoContinueTaskNotifyAcks(
    readTerminalAutoContinueTaskNotifyAcks(),
    now,
  );
  const unnotifiedTasks = [];
  (Array.isArray(tasks) ? tasks : []).forEach((task) => {
    const key = terminalAutoContinueTaskNotifyKey(task);
    if (!key || acks[key]) {
      return;
    }
    acks[key] = now;
    unnotifiedTasks.push(task);
  });
  writeTerminalAutoContinueTaskNotifyAcks(acks);
  return unnotifiedTasks;
}

function applyTerminalPasteScheduledTaskList(tasks, { refreshChip = true } = {}) {
  terminalPasteScheduledTasks.clear();
  (Array.isArray(tasks) ? tasks : []).forEach((task) => {
    const normalized = normalizeTerminalPasteScheduledTask(task);
    if (normalized) {
      terminalPasteScheduledTasks.set(normalized.taskId, normalized);
    }
  });
  if (refreshChip) {
    tickTerminalPasteScheduledCountdown();
  }
}

function applyTerminalAutoContinueScheduledTaskList(tasks, { refreshChip = true } = {}) {
  const previousTaskIds = new Set(terminalAutoContinueScheduledTasks.keys());
  const newlyDetectedTasks = [];
  terminalAutoContinueScheduledTasks.clear();
  (Array.isArray(tasks) ? tasks : []).forEach((task) => {
    const normalized = normalizeTerminalAutoContinueScheduledTask(task);
    if (normalized) {
      terminalAutoContinueScheduledTasks.set(normalized.taskId, normalized);
      if (!previousTaskIds.has(normalized.taskId)) {
        newlyDetectedTasks.push(normalized);
      }
    }
  });
  if (refreshChip) {
    tickTerminalPasteScheduledCountdown();
  }
  return newlyDetectedTasks;
}

function notifyNewTerminalAutoContinueScheduledTasks(tasks) {
  const detectedTasks = Array.isArray(tasks) ? tasks : [];
  if (detectedTasks.length === 0) {
    return;
  }
  const counts = terminalScheduledTaskCounts();
  const currentTasks = detectedTasks.filter(terminalScheduledTaskMatchesActiveSession);
  if (currentTasks.length > 0) {
    updateStatus(
      `检测到当前终端 ${currentTasks.length} 个自动继续定时任务，已更新定时 ${counts.current}/${counts.total}。`,
      "info",
    );
    return;
  }
  updateStatus(
    `检测到 ${detectedTasks.length} 个自动继续定时任务，已更新定时 ${counts.current}/${counts.total}。`,
    "info",
  );
}

async function refreshTerminalPasteScheduledTasks() {
  let newlyDetectedAutoContinueTasks = [];
  const [pasteResult, autoContinueResult] = await Promise.allSettled([
    requestJson("/api/terminal/scheduled-inputs"),
    requestJson("/api/terminal/auto-continue-tasks"),
  ]);
  if (pasteResult.status === "fulfilled") {
    applyTerminalPasteScheduledTaskList(pasteResult.value?.tasks || [], { refreshChip: false });
  } else {
    console.warn("refresh scheduled terminal paste tasks failed", pasteResult.reason);
  }
  if (autoContinueResult.status === "fulfilled") {
    const rawNewlyDetectedAutoContinueTasks = applyTerminalAutoContinueScheduledTaskList(autoContinueResult.value?.auto_continue_tasks || [], {
      refreshChip: false,
    });
    newlyDetectedAutoContinueTasks = filterUnnotifiedTerminalAutoContinueTasks(
      rawNewlyDetectedAutoContinueTasks,
    );
  } else {
    console.warn("refresh scheduled terminal auto-continue tasks failed", autoContinueResult.reason);
  }
  tickTerminalPasteScheduledCountdown();
  notifyNewTerminalAutoContinueScheduledTasks(newlyDetectedAutoContinueTasks);
}

function ensureTerminalPasteScheduledRefreshTimer() {
  if (terminalPasteScheduledCountdownTimer) {
    return;
  }
  terminalPasteScheduledCountdownTimer = window.setInterval(refreshTerminalPasteScheduledTasks, 10000);
}

// Remove one pending task by id from the local server-list cache.
function removeTerminalPasteScheduledTask(taskId, options = {}) {
  const task = terminalPasteScheduledTasks.get(taskId);
  if (!task) {
    return false;
  }
  terminalPasteScheduledTasks.delete(taskId);
  if (!options.silent) {
    tickTerminalPasteScheduledCountdown();
  }
  return true;
}

// Cancel all pending server-side tasks from the compact chip.
async function clearAllTerminalPasteScheduledTasks() {
  const taskIds = Array.from(terminalPasteScheduledTasks.keys());
  for (const taskId of taskIds) {
    try {
      await requestJson(`/api/terminal/scheduled-inputs/${encodeURIComponent(taskId)}`, {
        method: "DELETE",
      });
    } catch (error) {
      console.warn("cancel scheduled terminal paste task failed", error);
    }
  }
  await refreshTerminalPasteScheduledTasks();
  setTerminalPasteScheduleStatus("");
}

function terminalPasteScheduledTaskCount() {
  return terminalPasteScheduledTasks.size;
}

function terminalScheduledTaskActiveSessionIds() {
  const ids = new Set();
  const activeSessionId = String(state.activeSessionId || "").trim();
  const selectedSessionId = String(sessionSelectEl?.value || "").trim();
  const selectedAgentSessionId = String(agentSessionSelectEl?.value || "").trim();
  const currentSessionId = String(activeSession()?.id || "").trim();

  [activeSessionId, selectedSessionId, selectedAgentSessionId, currentSessionId].forEach((sessionId) => {
    if (sessionId) {
      ids.add(sessionId);
    }
  });

  return ids;
}

function terminalScheduledTaskMatchesActiveSession(task) {
  const sessionId = String(task?.sessionId || "").trim();
  return Boolean(sessionId && terminalScheduledTaskActiveSessionIds().has(sessionId));
}

function terminalScheduledTaskCounts() {
  const pasteTasks = Array.from(terminalPasteScheduledTasks.values());
  const autoContinueTasks = Array.from(terminalAutoContinueScheduledTasks.values());
  const current =
    pasteTasks.filter(terminalScheduledTaskMatchesActiveSession).length +
    autoContinueTasks.filter(terminalScheduledTaskMatchesActiveSession).length;
  const total = pasteTasks.length + autoContinueTasks.length;
  return {
    current,
    total,
    pasteCurrent: pasteTasks.filter(terminalScheduledTaskMatchesActiveSession).length,
    pasteTotal: pasteTasks.length,
    autoContinueCurrent: autoContinueTasks.filter(terminalScheduledTaskMatchesActiveSession).length,
    autoContinueTotal: autoContinueTasks.length,
  };
}

function terminalScheduledTaskChipText() {
  const counts = terminalScheduledTaskCounts();
  return `定时 ${counts.current}/${counts.total}`;
}

function hasTerminalPasteScheduledTask() {
  return terminalPasteScheduledTasks.size > 0;
}

// Publish the full current task list as a single array payload. The settings
// panel re-renders from the array, so any add/remove/update is reflected.
function broadcastTerminalPasteScheduledTasks() {
  try {
    window.localStorage.removeItem(TERMINAL_PASTE_SCHEDULED_STORAGE_KEY);
  } catch (_error) {
    // localStorage may be unavailable; server-side scheduling is unaffected.
  }
}

async function clearTerminalPasteScheduledTimer() {
  await clearAllTerminalPasteScheduledTasks();
}

function setTerminalPasteScheduleChip(visible, message = "") {
  if (!terminalPasteScheduleChipEl) {
    return;
  }
  const counts = terminalScheduledTaskCounts();
  terminalPasteScheduleChipEl.hidden = false;
  terminalPasteScheduleChipEl.dataset.pending = counts.total > 0 ? "true" : "false";
  if (terminalPasteScheduleChipTextEl) {
    terminalPasteScheduleChipTextEl.textContent = visible && message ? message : terminalScheduledTaskChipText();
  }
}

// ---- Cross-tab broadcast of the background paste scheduled send ----
function buildTerminalPasteScheduledPreview(text, maxLen = 48) {
  if (typeof text !== "string") {
    return "";
  }
  const collapsed = text.replace(/\s+/g, " ").trim();
  if (collapsed.length <= maxLen) {
    return collapsed;
  }
  return `${collapsed.slice(0, maxLen)}…`;
}

function readTerminalPasteScheduledTerminalName() {
  const session = activeSession();
  return (session && String(session.name || "").trim()) || state.activeSessionId || "当前终端";
}

function broadcastTerminalPasteScheduledState(publish) {
  // Legacy compatibility: server-side scheduling no longer publishes task
  // state through localStorage.
  try {
    broadcastTerminalPasteScheduledTasks();
  } catch (_error) {
    // localStorage may be unavailable; server-side scheduling is unaffected.
  }
}

function terminalPasteScheduledApplyCancelBroadcast(event) {
  // Legacy localStorage cancel requests are best-effort only; the canonical
  // cancel path is DELETE /api/terminal/scheduled-inputs/{task_id}.
  const cancelId = event?.data?.cancelTaskId;
  if (!cancelId || !terminalPasteScheduledTasks.has(cancelId)) {
    return;
  }
  requestJson(`/api/terminal/scheduled-inputs/${encodeURIComponent(cancelId)}`, {
    method: "DELETE",
  })
    .then(refreshTerminalPasteScheduledTasks)
    .catch((error) => console.warn("cancel scheduled terminal paste task failed", error));
}

function setTerminalPasteScheduleStatus(message, tone = "muted") {
  if (!terminalPasteScheduleStatusEl) {
    return;
  }
  terminalPasteScheduleStatusEl.textContent = message || "";
  terminalPasteScheduleStatusEl.dataset.tone = tone;
}

function formatScheduleRemaining(ms) {
  if (ms <= 0) {
    return "0 秒";
  }
  const totalSeconds = Math.ceil(ms / 1000);
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  const parts = [];
  if (hours > 0) {
    parts.push(`${hours} 小时`);
  }
  if (minutes > 0 || hours > 0) {
    parts.push(`${minutes} 分`);
  }
  parts.push(`${seconds} 秒`);
  return parts.join(" ");
}

function tickTerminalPasteScheduledCountdown() {
  const counts = terminalScheduledTaskCounts();
  if (counts.total === 0) {
    setTerminalPasteScheduleStatus("");
    setTerminalPasteScheduleChip(false);
    return;
  }
  setTerminalPasteScheduleStatus(
    `已安排 ${counts.total} 个待发送任务，当前终端 ${counts.current} 个。`,
    "info",
  );
  setTerminalPasteScheduleChip(true, terminalScheduledTaskChipText());
}

function readTerminalPasteScheduleMode() {
  const checked = document.querySelector('input[name="terminal-paste-schedule-mode"]:checked');
  return checked?.value === "datetime" ? "datetime" : "delay";
}

function resolveTerminalPasteScheduleDueAt() {
  const mode = readTerminalPasteScheduleMode();
  const now = Date.now();
  if (mode === "datetime") {
    if (!terminalPasteScheduleDatetimeEl?.value) {
      return { error: "请选择一个发送时间。" };
    }
    const dueAtMs = Date.parse(`${terminalPasteScheduleDatetimeEl.value}:00`);
    if (!Number.isFinite(dueAtMs)) {
      return { error: "时间格式无效。" };
    }
    if (dueAtMs <= now) {
      return { error: "指定时间必须晚于当前时间。" };
    }
    const label = new Date(dueAtMs).toLocaleString();
    return { dueAtMs, label };
  }

  const rawDelay = Number.parseFloat(terminalPasteScheduleDelayEl?.value || "");
  if (!Number.isFinite(rawDelay) || rawDelay <= 0) {
    return { error: "请输入有效的延迟数值。" };
  }
  const unit = terminalPasteScheduleDelayUnitEl?.value || "minutes";
  const multiplier = unit === "seconds" ? 1000 : unit === "hours" ? 3600000 : 60000;
  const dueAtMs = now + Math.round(rawDelay * multiplier);
  const label = `${rawDelay} ${unit === "seconds" ? "秒" : unit === "hours" ? "小时" : "分钟"}后`;
  return { dueAtMs, label };
}

async function confirmTerminalPasteSchedule() {
  if (!state.activeSessionId) {
    setTerminalPasteScheduleStatus("请先选择一个目标终端。", "warn");
    return false;
  }
  if (!terminalPasteTextEl || !terminalPasteTextEl.value.trim()) {
    setTerminalPasteScheduleStatus("请先粘贴要发送的内容。", "warn");
    terminalPasteTextEl?.focus();
    return false;
  }
  const resolved = resolveTerminalPasteScheduleDueAt();
  if (resolved.error) {
    setTerminalPasteScheduleStatus(resolved.error, "warn");
    return false;
  }

  setTerminalPasteBusy(true);
  setTerminalPasteScheduleStatus("正在提交到服务端定时任务…", "info");
  try {
    const payload = await requestJson("/api/terminal/scheduled-inputs", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        session_id: state.activeSessionId,
        text: terminalPasteTextEl.value,
        due_at: resolved.dueAtMs,
        label: resolved.label,
        send_enter: true,
      }),
    });
    applyTerminalPasteScheduledTaskList(payload?.tasks || []);
  } catch (error) {
    setTerminalPasteScheduleStatus(error?.message || "创建服务端定时任务失败。", "warn");
    setTerminalPasteBusy(false);
    terminalPasteTextEl?.focus();
    return false;
  }
  const counts = terminalScheduledTaskCounts();
  updateStatus(
    `已提交服务端定时发送：${resolved.label}（当前终端 ${counts.current} 个，全部 ${counts.total} 个）。`,
    "ok",
  );
  setTerminalPasteBusy(false);
  closeTerminalPasteDialogForSchedule();
  setTerminalPasteScheduleChip(true, terminalScheduledTaskChipText());
  return true;
}

function closeTerminalPasteDialogForSchedule() {
  // Close the visible dialog while preserving the background scheduled send.
  // Unlike closeTerminalPasteDialog(), this does NOT cancel the timer and
  // does not revoke captured content, since the snapshot is already captured.
  if (!terminalPasteDialogEl) {
    return;
  }
  if (typeof terminalPasteDialogEl.close === "function") {
    if (terminalPasteDialogEl.open) {
      terminalPasteDialogEl.close();
    }
  } else {
    terminalPasteDialogEl.removeAttribute("open");
  }
}

function isEditableElement(element) {
  return (
    element instanceof HTMLInputElement ||
    element instanceof HTMLTextAreaElement ||
    element instanceof HTMLSelectElement ||
    Boolean(element?.isContentEditable)
  );
}

function shouldHandleTerminalPaste(target) {
  if (terminalSessionInitializing()) {
    return false;
  }

  if (!state.activeSessionId) {
    return false;
  }

  if (terminalPasteDialogEl?.open) {
    return false;
  }

  if (target instanceof Node && terminalPasteDialogEl?.contains(target)) {
    return false;
  }

  if (target instanceof Node && term.element?.contains(target)) {
    return true;
  }

  const activeElement = document.activeElement;
  if (activeElement instanceof Node && terminalPasteDialogEl?.contains(activeElement)) {
    return false;
  }

  if (activeElement instanceof Node && term.element?.contains(activeElement)) {
    return true;
  }

  if (target instanceof Element && isEditableElement(target)) {
    return false;
  }

  if (activeElement instanceof Element && isEditableElement(activeElement)) {
    return false;
  }

  return true;
}

function handleTerminalPasteEvent(event) {
  if (!shouldHandleTerminalPaste(event.target)) {
    return;
  }

  const richParts = dataTransferToPasteParts(event.clipboardData);
  if (richParts.some((part) => part.type === "images")) {
    if (!ensureTerminalReadyForInput()) {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    pasteTerminalPartsDirectly(richParts).catch((error) => {
      updateStatus(error?.message || "处理剪贴板图片失败。", "warn");
    });
    return;
  }

  const text = event.clipboardData?.getData("text/plain");
  if (typeof text !== "string" || text.length === 0) {
    return;
  }

  if (!ensureTerminalReadyForInput()) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();
  sendPastedText(text);
}

function isTerminalCtrlVPasteShortcut(event) {
  return (
    event.ctrlKey &&
    !event.shiftKey &&
    !event.altKey &&
    !event.metaKey &&
    String(event.key || "").toLowerCase() === "v"
  );
}

function handleTerminalClipboardShortcut(event) {
  if (event.defaultPrevented || !isTerminalCtrlVPasteShortcut(event)) {
    return;
  }

  if (!shouldHandleTerminalPaste(event.target)) {
    return;
  }

  event.preventDefault();
  event.stopPropagation();
  pasteFromClipboard();
}

async function pasteFromClipboard() {
  cancelNewSessionQuickStart();
  if (!ensureTerminalReadyForInput()) {
    return;
  }

  if (navigator.clipboard?.read) {
    updateStatus("正在读取剪贴板…", "info");
    setClipboardPasteBusy(true);
    try {
      const items = await navigator.clipboard.read();
      const parts = await clipboardItemsToPasteParts(items);
      if (parts.some((part) => part.type === "images")) {
        await applyTerminalPasteParts(parts);
        return;
      }
      const text = parts
        .filter((part) => part.type === "text")
        .map((part) => part.text)
        .join("\n")
        .trim();
      sendPastedText(text, "剪贴板为空。", {
        forceBracketedPaste: true,
      });
      return;
    } catch (_error) {
      // Some remote desktop/browser environments expose readText() but fail rich
      // clipboard reads with host clipboard errors. Continue with plain text.
    } finally {
      setClipboardPasteBusy(false);
    }
  }

  if (!navigator.clipboard?.readText) {
    openTerminalPasteDialog();
    updateStatus("浏览器禁止直接读取剪贴板，请在弹窗里粘贴内容。", "warn");
    return;
  }

  updateStatus("正在读取剪贴板…", "info");
  setClipboardPasteBusy(true);

  try {
    const text = await navigator.clipboard.readText();
    sendPastedText(text, "剪贴板为空。", {
      forceBracketedPaste: true,
    });
  } catch (_error) {
    openTerminalPasteDialog();
    updateStatus("浏览器无法直接读取剪贴板，请在弹窗里粘贴内容。", "warn");
  } finally {
    setClipboardPasteBusy(false);
  }
}
