// 终端归档（resume 记录）渲染、加载、复制、删除模块。
// 由 app.js 拆出，在 app.js 之前以 <script defer> 加载，
// 通过共享全局作用域向 app.js 提供下列函数，无需修改调用方。
// 依赖的全局（state.*、terminalArchivesListEl 等）均为 app.js 顶层声明，加载顺序保证可用。

function shortResumeId(resumeId) {
  const normalized = String(resumeId || "");
  return normalized.length > 12 ? `${normalized.slice(0, 8)}...${normalized.slice(-4)}` : normalized;
}

function resumeCommandFromId(resumeId) {
  return `codex resume ${resumeId}`;
}

function archiveResumeId(archive) {
  return archive?.resume_id || archive?.resumeId || "";
}

function archiveIdentity(archive) {
  return archive?.id || archiveResumeId(archive);
}

function archiveCommand(archive) {
  return archive?.command || resumeCommandFromId(archiveResumeId(archive));
}

function copyTextWithHiddenTextarea(text) {
  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.readOnly = true;
  textarea.setAttribute("aria-hidden", "true");
  textarea.style.cssText = "position:fixed;left:-9999px;top:0;width:1px;height:1px;opacity:0;pointer-events:none;";
  document.body.appendChild(textarea);
  textarea.focus();
  textarea.select();
  let copied = false;
  try {
    copied = document.execCommand("copy");
  } catch {
    copied = false;
  } finally {
    textarea.remove();
  }
  return copied;
}

function copyTerminalArchiveCommand(command, button) {
  const commandText = String(command || "").trim();
  if (!commandText) {
    updateTableCardStatus(terminalArchivesStatusEl, "没有可复制的命令。", "warn");
    return;
  }

  const markCopied = () => {
    updateTableCardStatus(terminalArchivesStatusEl, "已复制归档恢复命令。", "ok");
    if (!button) {
      return;
    }
    const previousText = button.textContent;
    button.textContent = "已复制";
    window.setTimeout(() => {
      button.textContent = previousText || "复制";
    }, 1200);
  };

  if (navigator.clipboard?.writeText) {
    navigator.clipboard.writeText(commandText).then(markCopied, () => {
      if (copyTextWithHiddenTextarea(commandText)) {
        markCopied();
      } else {
        updateTableCardStatus(terminalArchivesStatusEl, "浏览器阻止自动复制，请检查剪贴板权限。", "warn");
      }
    });
    return;
  }

  if (copyTextWithHiddenTextarea(commandText)) {
    markCopied();
  } else {
    updateTableCardStatus(terminalArchivesStatusEl, "浏览器不支持自动复制，请检查剪贴板权限。", "warn");
  }
}

function archiveWorkingPath(archive) {
  const rawPath = String(archive?.cwd || archive?.working_dir || archive?.path || "").trim();
  if (!rawPath) {
    return "";
  }
  if (rawPath.startsWith("/")) {
    return relativePathBetweenAbsolute(state.workspaceDir || "/", rawPath);
  }
  return normalizeRelativePath(rawPath);
}

function sortTerminalArchives(archives) {
  if (!Array.isArray(archives)) {
    return [];
  }

  return [...archives].sort((left, right) => {
    return (
      Number(right.last_used_at || 0) - Number(left.last_used_at || 0) ||
      Number(right.updated_at || 0) - Number(left.updated_at || 0) ||
      Number(right.created_at || 0) - Number(left.created_at || 0) ||
      String(left.note || "").localeCompare(String(right.note || ""))
    );
  });
}

function renderTerminalArchives() {
  if (!terminalArchivesListEl) {
    return;
  }

  terminalArchivesListEl.textContent = "";

  if (state.terminalArchives.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = 7;
    cell.className = "session-empty";
    cell.textContent = "还没有 Codex 归档。";
    row.appendChild(cell);
    terminalArchivesListEl.appendChild(row);
    return;
  }

  state.terminalArchives.forEach((archive) => {
    const resumeId = archiveResumeId(archive);
    const command = archiveCommand(archive);
    const archiveId = archiveIdentity(archive);
    const row = document.createElement("tr");

    const runLink = createActionLink(
      "运行",
      buildTerminalUrl(archiveWorkingPath(archive), "", { fresh: true, runCommand: command }),
      "mini-button accent",
    );
    runLink.addEventListener("click", (event) => {
      openFreshTerminalRunLink(event, archiveWorkingPath(archive), command, {
        beforeNavigate: () => touchTerminalArchive(archiveId),
      });
    });

    const deleteButton = createActionButton("删除", () => {
      deleteTerminalArchive(archive);
    }, "mini-button danger");

    const runCell = document.createElement("td");
    runCell.className = "session-action-cell";
    runCell.appendChild(runLink);

    const deleteCell = document.createElement("td");
    deleteCell.className = "session-action-cell";
    deleteCell.appendChild(deleteButton);

    const noteCell = createTextCell(
      String(archive.note || "").trim() || `Codex ${shortResumeId(resumeId)}`,
      "terminal-archive-note-cell",
    );
    const resumeCell = createTextCell(resumeId || "—", "mono-text terminal-archive-resume-cell");
    resumeCell.title = resumeId;
    const cwd = archiveWorkingPath(archive);
    const cwdCell = createTextCell(cwd ? displayPath(cwd) : "/", "mono-text terminal-archive-cwd-cell");
    cwdCell.title = cwd ? displayPath(cwd) : "/";
    const commandCell = document.createElement("td");
    commandCell.className = "terminal-archive-command-cell";
    commandCell.title = command;
    const copyCommandButton = createActionButton("复制", () => {
      copyTerminalArchiveCommand(command, copyCommandButton);
    }, "mini-button");
    copyCommandButton.title = command;
    commandCell.appendChild(copyCommandButton);
    const timeCell = createTextCell(
      formatDateLikeMonthDayTime(archive.last_used_at || archive.updated_at || archive.created_at),
      "terminal-archive-time-cell",
    );

    row.append(runCell, deleteCell, noteCell, resumeCell, cwdCell, commandCell, timeCell);
    terminalArchivesListEl.appendChild(row);
  });
}

async function loadTerminalArchives() {
  if (!hasTerminalArchiveControls) {
    return;
  }

  const requestToken = ++state.terminalArchiveRequestToken;
  updateTableCardStatus(terminalArchivesStatusEl, "正在读取归档列表…", "info");
  if (refreshTerminalArchivesButton) {
    refreshTerminalArchivesButton.disabled = true;
  }

  try {
    const response = await requestJson("/api/terminal/resume-archives");
    if (requestToken !== state.terminalArchiveRequestToken) {
      return;
    }
    state.terminalArchives = sortTerminalArchives(response.archives);
    renderTerminalArchives();
    updateStatus(
      terminalArchivesStatusEl,
      state.terminalArchives.length === 0 ? "还没有 Codex 归档。" : "归档列表已更新。",
      state.terminalArchives.length === 0 ? "muted" : "ok",
    );
  } catch (error) {
    if (requestToken !== state.terminalArchiveRequestToken) {
      return;
    }
    state.terminalArchives = [];
    renderTerminalArchives();
    updateTableCardStatus(terminalArchivesStatusEl, error.message || "读取归档列表失败。", "warn");
  } finally {
    if (requestToken === state.terminalArchiveRequestToken && refreshTerminalArchivesButton) {
      refreshTerminalArchivesButton.disabled = false;
    }
  }
}

async function touchTerminalArchive(archiveId) {
  if (!archiveId) {
    return;
  }

  try {
    const touched = await requestJson(
      `/api/terminal/resume-archives/${encodeURIComponent(archiveId)}/touch`,
      { method: "PUT" },
    );
    state.terminalArchives = sortTerminalArchives(
      state.terminalArchives.map((archive) => (archiveIdentity(archive) === archiveIdentity(touched) ? touched : archive)),
    );
    renderTerminalArchives();
  } catch {
    // Opening the terminal should not be blocked by a best-effort recency update.
  }
}

async function deleteTerminalArchive(archive) {
  const archiveId = archiveIdentity(archive);
  if (!archiveId) {
    return;
  }

  const label = String(archive.note || "").trim() || shortResumeId(archiveResumeId(archive));
  if (!window.confirm(`删除归档"${label}"？`)) {
    return;
  }

  updateTableCardStatus(terminalArchivesStatusEl, `正在删除归档 ${label}…`, "info");
  try {
    await requestJson(`/api/terminal/resume-archives/${encodeURIComponent(archiveId)}`, {
      method: "DELETE",
    });
    state.terminalArchives = state.terminalArchives.filter((item) => archiveIdentity(item) !== archiveId);
    renderTerminalArchives();
    updateTableCardStatus(terminalArchivesStatusEl, `已删除归档 ${label}。`, "ok");
  } catch (error) {
    updateTableCardStatus(terminalArchivesStatusEl, error.message || "删除归档失败。", "warn");
  }
}
