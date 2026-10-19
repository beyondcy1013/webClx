// 终端工作现场恢复：项目管理中的可视化恢复入口。
// 记录来自后端 `.webclx-terminal-workspace-restores.json`，包含结构、名称、
// 分组、目录、Agent resume 身份和输入历史。
let terminalWorkspaceRestoresItems = [];
let terminalWorkspaceRestoresSelected = new Set();
let terminalWorkspaceRestoresLoading = false;

function terminalWorkspaceRestoreDom() {
  return {
    dialog: document.getElementById("terminal-workspace-restores-dialog"),
    form: document.getElementById("terminal-workspace-restores-form"),
    status: document.getElementById("terminal-workspace-restores-status"),
    tableWrap: document.getElementById("terminal-workspace-restores-table-wrap"),
    tbody: document.getElementById("terminal-workspace-restores-tbody"),
    refreshButton: document.getElementById("terminal-workspace-restores-refresh"),
    closeButton: document.getElementById("terminal-workspace-restores-close"),
    restoreSelectedButton: document.getElementById("terminal-workspace-restore-selected"),
    restoreAllButton: document.getElementById("terminal-workspace-restore-all-button"),
    selectAll: document.getElementById("terminal-workspace-restore-all"),
    groupRestoreButton: document.getElementById("terminal-group-restore"),
  };
}

function terminalWorkspaceRestoreSetStatus(message, tone = "muted") {
  const { status } = terminalWorkspaceRestoreDom();
  if (!status) return;
  status.textContent = String(message || "");
  status.dataset.tone = tone;
  status.hidden = !String(message || "");
}

function terminalWorkspaceRestoreProgramLabel(record) {
  const program = String(record?.program || "").trim().toLowerCase();
  if (!program || program === "shell") return "Shell";
  return program.charAt(0).toUpperCase() + program.slice(1);
}

function terminalWorkspaceRestoreAgentText(record) {
  const program = terminalWorkspaceRestoreProgramLabel(record);
  return record?.resume_id ? `${program} · ${String(record.resume_id).slice(0, 8)}…` : program;
}

function terminalWorkspaceRestoreGroupText(record) {
  if (!record?.group_token) return "—";
  const name = String(record.group_name || "").trim();
  return name || `分组 #${record.group_order || 1}`;
}

function terminalWorkspaceRestoreFormatTime(millis) {
  const value = Number(millis);
  if (!Number.isFinite(value) || value <= 0) return "未知";
  const date = new Date(value);
  const pad = (part) => String(part).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

function terminalWorkspaceRestoreDisabled(record) {
  return !record?.recoverable || record.restore_count > 0;
}

function terminalWorkspaceRestoreTitle(record) {
  if (!record?.recoverable) return "工作目录不存在，无法恢复";
  if (record.restore_count > 0) return "该工作现场已在本次服务生命周期中恢复";
  return record?.command || "";
}

function terminalWorkspaceRestoreResultSummary(response) {
  const restored = Number(response?.restored || 0);
  const skipped = Array.isArray(response?.skipped) ? response.skipped.length : 0;
  const failed = Array.isArray(response?.failed) ? response.failed.length : 0;
  const details = [];
  if (skipped) details.push(`跳过 ${skipped}`);
  if (failed) details.push(`失败 ${failed}`);
  return details.length
    ? `已恢复 ${restored} 个终端（${details.join("，")}）。`
    : `已恢复 ${restored} 个终端。`;
}

function terminalWorkspaceRestoreSyncSelectAll() {
  const { selectAll } = terminalWorkspaceRestoreDom();
  if (!selectAll) return;
  const selectable = terminalWorkspaceRestoresItems.filter((item) => !terminalWorkspaceRestoreDisabled(item));
  selectAll.checked = selectable.length > 0
    && selectable.every((item) => terminalWorkspaceRestoresSelected.has(item.session_id));
  selectAll.indeterminate = !selectAll.checked
    && selectable.some((item) => terminalWorkspaceRestoresSelected.has(item.session_id));
  selectAll.disabled = selectable.length === 0;
}

function terminalWorkspaceRestoreRender() {
  const { tbody } = terminalWorkspaceRestoreDom();
  if (!tbody) return;
  tbody.replaceChildren();
  if (!terminalWorkspaceRestoresItems.length) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 7;
    cell.className = "terminal-workspace-restores-empty";
    cell.textContent = "暂无可恢复终端。创建、重命名、分组或发送终端消息后会自动保存工作现场。";
    row.append(cell);
    tbody.append(row);
    terminalWorkspaceRestoreSyncSelectAll();
    return;
  }

  const groupMembers = new Map();
  const groupedRecords = new Map();
  terminalWorkspaceRestoresItems.forEach((record) => {
    const token = String(record.group_token || "").trim();
    if (!token) {
      const entry = groupedRecords.get("group:ungrouped") || { record, members: [] };
      entry.members.push(record);
      groupedRecords.set("group:ungrouped", entry);
      return;
    }
    const entry = groupedRecords.get(`group:${token}`) || { record, members: [] };
    entry.members.push(record);
    groupedRecords.set(`group:${token}`, entry);
  });
  groupedRecords.forEach((entry) => {
    const token = String(entry.record.group_token || "").trim();
    groupMembers.set(token, entry.members.filter((item) => !terminalWorkspaceRestoreDisabled(item)).map((item) => item.session_id));
  });

  groupedRecords.forEach(({ record, members }) => {
    const restoreIds = members.filter((item) => !terminalWorkspaceRestoreDisabled(item)).map((item) => item.session_id);
    const row = document.createElement("tr");
    row.dataset.sessionId = record.session_id || "";
    if (terminalWorkspaceRestoreDisabled(record)) row.classList.add("terminal-workspace-restores-disabled");

    const checkCell = document.createElement("td");
    const check = document.createElement("input");
    check.type = "checkbox";
    check.value = restoreIds.join(",");
    check.checked = restoreIds.length > 0 && restoreIds.every((id) => terminalWorkspaceRestoresSelected.has(id));
    check.disabled = restoreIds.length === 0;
    check.ariaLabel = `选择恢复 ${record.group_name || record.name || record.session_id}`;
    check.addEventListener("change", () => {
      restoreIds.forEach((id) => check.checked ? terminalWorkspaceRestoresSelected.add(id) : terminalWorkspaceRestoresSelected.delete(id));
      terminalWorkspaceRestoreSyncSelectAll();
    });
    checkCell.append(check);

    const agentCell = document.createElement("td");
    agentCell.className = "terminal-workspace-restores-agent";
    agentCell.textContent = terminalWorkspaceRestoreAgentText(record);

    const nameCell = document.createElement("td");
    nameCell.className = "terminal-workspace-restores-name";
    const groupTitle = record.group_name || (record.group_token ? `分组 #${record.group_order || 1}` : "未分组");
    const expandMark = document.createElement("span");
    expandMark.textContent = "+ ";
    expandMark.className = "terminal-workspace-restore-expand";
    nameCell.append(expandMark, `${groupTitle}（${members.length} 个终端）`);
    const childRows = members.map((item) => {
      const child = document.createElement("tr");
      child.hidden = true;
      child.className = "terminal-workspace-restore-child";
      const childCell = document.createElement("td");
      childCell.colSpan = 7;
      const childCheck = document.createElement("input");
      childCheck.type = "checkbox";
      childCheck.checked = terminalWorkspaceRestoresSelected.has(item.session_id);
      childCheck.disabled = terminalWorkspaceRestoreDisabled(item);
      childCheck.ariaLabel = `选择恢复 ${item.name || item.session_id}`;
      childCheck.addEventListener("change", () => {
        if (childCheck.checked) terminalWorkspaceRestoresSelected.add(item.session_id);
        else terminalWorkspaceRestoresSelected.delete(item.session_id);
        check.checked = restoreIds.length > 0 && restoreIds.every((id) => terminalWorkspaceRestoresSelected.has(id));
        terminalWorkspaceRestoreSyncSelectAll();
      });
      childCell.append(childCheck, `  └ ${item.name || item.session_id || "未命名"}`);
      childCell.style.paddingLeft = "2.5rem";
      child.append(childCell);
      return child;
    });
    row.title = "点击展开或收起分组成员";
    row.addEventListener("click", (event) => {
      if (event.target.closest("button, input")) return;
      const expanded = childRows.length > 0 && childRows[0].hidden;
      childRows.forEach((child) => { child.hidden = !expanded; });
      expandMark.textContent = expanded ? "− " : "+ ";
      row.classList.toggle("is-expanded", expanded);
    });

    const groupCell = document.createElement("td");
    groupCell.className = "terminal-workspace-restores-group";
    if (record.group_color) groupCell.style.setProperty("--terminal-group-color", record.group_color);
    const groupLabel = document.createElement("span");
    groupLabel.textContent = terminalWorkspaceRestoreGroupText(record);
    groupCell.append(groupLabel);
    const groupToken = String(record.group_token || "").trim();

    const pathCell = document.createElement("td");
    pathCell.className = "terminal-workspace-restores-path";
    pathCell.title = record.display_path || record.path || "";
    pathCell.textContent = record.display_path || record.path || "—";

    const timeCell = document.createElement("td");
    timeCell.className = "terminal-workspace-restores-time";
    timeCell.textContent = terminalWorkspaceRestoreFormatTime(record.updated_at);

    const actionCell = document.createElement("td");
    const restoreButton = document.createElement("button");
    restoreButton.type = "button";
    restoreButton.className = "mini-button";
    restoreButton.textContent = groupToken ? `恢复分组 (${restoreIds.length})` : (record.restore_count > 0 ? "已恢复" : "恢复");
    restoreButton.disabled = restoreIds.length === 0;
    restoreButton.title = terminalWorkspaceRestoreTitle(record);
    restoreButton.addEventListener("click", () => {
      const selectedIds = restoreIds.filter((id) => terminalWorkspaceRestoresSelected.has(id));
      terminalRestoreWorkspaces(selectedIds);
    });
    actionCell.append(restoreButton);

    row.append(checkCell, agentCell, nameCell, groupCell, pathCell, timeCell, actionCell);
    tbody.append(row);
    childRows.forEach((child) => tbody.append(child));
  });
  terminalWorkspaceRestoreSyncSelectAll();
}

async function refreshTerminalWorkspaceRestores({ capture = false } = {}) {
  const dom = terminalWorkspaceRestoreDom();
  if (!dom.form || terminalWorkspaceRestoresLoading) return;
  terminalWorkspaceRestoresLoading = true;
  dom.refreshButton && (dom.refreshButton.disabled = true);
  terminalWorkspaceRestoreSetStatus(capture ? "正在捕获当前终端工作现场…" : "正在读取恢复记录…", "info");
  try {
    if (capture) {
      await requestJson("/api/terminal/workspace-restores", { method: "PUT" });
    }
    const response = await requestJson("/api/terminal/workspace-restores");
    terminalWorkspaceRestoresItems = Array.isArray(response.records) ? response.records : [];
    const currentIds = new Set(terminalWorkspaceRestoresItems.map((item) => item.session_id));
    [...terminalWorkspaceRestoresSelected].forEach((id) => {
      if (!currentIds.has(id)) terminalWorkspaceRestoresSelected.delete(id);
    });
    terminalWorkspaceRestoreRender();
    terminalWorkspaceRestoreSetStatus(
      terminalWorkspaceRestoresItems.length
        ? `已列出 ${terminalWorkspaceRestoresItems.length} 个可恢复终端。`
        : "暂无可恢复终端。",
      terminalWorkspaceRestoresItems.length ? "ok" : "muted",
    );
  } catch (error) {
    terminalWorkspaceRestoreSetStatus(error.message || "读取恢复记录失败。", "warn");
  } finally {
    terminalWorkspaceRestoresLoading = false;
    dom.refreshButton && (dom.refreshButton.disabled = false);
  }
}

function openTerminalWorkspaceRestoresDialog() {
  const { dialog } = terminalWorkspaceRestoreDom();
  if (!dialog) return;
  dialog.showModal();
  refreshTerminalWorkspaceRestores({ capture: true });
}

function closeTerminalWorkspaceRestoresDialog() {
  const { dialog } = terminalWorkspaceRestoreDom();
  dialog?.close();
}

async function terminalRestoreWorkspaces(sessionIds) {
  const ids = Array.isArray(sessionIds)
    ? sessionIds.map((id) => String(id || "").trim()).filter(Boolean)
    : [];
  if (!ids.length) {
    terminalWorkspaceRestoreSetStatus("请选择要恢复的终端。", "warn");
    return;
  }
  const dom = terminalWorkspaceRestoreDom();
  dom.restoreSelectedButton && (dom.restoreSelectedButton.disabled = true);
  dom.restoreAllButton && (dom.restoreAllButton.disabled = true);
  terminalWorkspaceRestoreSetStatus(`正在恢复 ${ids.length} 个终端工作现场…`, "info");
  try {
    const response = await requestJson("/api/terminal/workspace-restores", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ session_ids: ids }),
    });
    terminalWorkspaceRestoreSetStatus(
      `${terminalWorkspaceRestoreResultSummary(response)}正在刷新会话列表…`,
      response?.failed?.length ? "warn" : "ok",
    );
    updateStatus(`${terminalWorkspaceRestoreResultSummary(response)}工作现场。`, response?.failed?.length ? "warn" : "ok");
    if (typeof loadSessions === "function") {
      await loadSessions({
        preferredSessionId: ids[0],
        pushHistoryOnSelect: true,
        forcePreferredSession: true,
      });
    }
    await refreshTerminalWorkspaceRestores();
    if (ids.length === 1 && typeof selectSession === "function") {
      selectSession(ids[0], { connect: true, pushHistory: false });
    }
  } catch (error) {
    terminalWorkspaceRestoreSetStatus(error.message || "恢复终端工作现场失败。", "warn");
  } finally {
    dom.restoreSelectedButton && (dom.restoreSelectedButton.disabled = false);
    dom.restoreAllButton && (dom.restoreAllButton.disabled = false);
  }
}

function bindTerminalWorkspaceRestoresDialog() {
  const dom = terminalWorkspaceRestoreDom();
  if (!dom.dialog || !dom.form || dom.dialog.dataset.bound === "true") return;
  dom.dialog.dataset.bound = "true";
  dom.refreshButton?.addEventListener("click", (event) => {
    event.preventDefault();
    refreshTerminalWorkspaceRestores({ capture: true });
  });
  dom.closeButton?.addEventListener("click", (event) => {
    event.preventDefault();
    closeTerminalWorkspaceRestoresDialog();
  });
  dom.restoreSelectedButton?.addEventListener("click", (event) => {
    event.preventDefault();
    terminalRestoreWorkspaces([...terminalWorkspaceRestoresSelected]);
  });
  dom.restoreAllButton?.addEventListener("click", (event) => {
    event.preventDefault();
    terminalRestoreWorkspaces(
      terminalWorkspaceRestoresItems
        .filter((item) => !terminalWorkspaceRestoreDisabled(item))
        .map((item) => item.session_id),
    );
  });
  dom.selectAll?.addEventListener("change", () => {
    const selectable = terminalWorkspaceRestoresItems.filter((item) => !terminalWorkspaceRestoreDisabled(item));
    selectable.forEach((item) => {
      if (dom.selectAll.checked) {
        terminalWorkspaceRestoresSelected.add(item.session_id);
      } else {
        terminalWorkspaceRestoresSelected.delete(item.session_id);
      }
    });
    terminalWorkspaceRestoreRender();
  });
  dom.groupRestoreButton?.addEventListener("click", () => {
    closeTerminalWorkspaceRestoresDialog();
    const groupDialog = document.getElementById("terminal-group-dialog");
    groupDialog?.close();
    openTerminalWorkspaceRestoresDialog();
  });
}
