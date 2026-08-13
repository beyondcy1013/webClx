(function () {
  function createCompileStatusManager(deps) {
    const COMPILE_LIVE_REFRESH_MS = 2_000;
    const {
      state,
      requestJson,
      escapeHtml,
      setTextContent,
      setInlineStatus,
      setButtonBusy,
      formatDateTimeLong,
      setActiveTab,
      loadFile,
      showToast,
      elements,
    } = deps;
    const {
      compilePendingListEl,
      compileRunningListEl,
      compileRunListEl,
      compileStatusMessageEl,
      compileStatusRefreshButtonEl,
      compileHistoryClearButtonEl,
    } = elements;
    let liveRefreshActive = false;
    let liveRefreshTimer = null;

    function formatCompileTimestamp(timestampSeconds) {
      const value = Number(timestampSeconds);
      if (!Number.isFinite(value) || value <= 0) {
        return "—";
      }
      return formatDateTimeLong(new Date(value * 1000));
    }

    function formatCompileTextList(values, emptyText = "-") {
      const list = Array.isArray(values)
        ? values.map((value) => String(value || "").trim()).filter(Boolean)
        : [];
      return list.length ? list.join(", ") : emptyText;
    }

    function formatCompileCommand(command) {
      return formatCompileTextList(command, "-");
    }

    function formatCompileTerminalIdentity(item) {
      const name = String(item?.source_terminal_name || "").trim();
      const id = String(item?.source_terminal_id || "").trim();
      const tmux = String(item?.source_tmux_session || "").trim();
      const details = [id, tmux].filter(Boolean);
      if (name && details.length) {
        return `${name} (${details.join(" / ")})`;
      }
      return name || details.join(" / ") || "-";
    }

    function formatCompileTerminalList(run) {
      const names = Array.isArray(run?.source_terminal_names)
        ? run.source_terminal_names.map((value) => String(value || "").trim()).filter(Boolean)
        : [];
      const ids = Array.isArray(run?.source_terminal_ids)
        ? run.source_terminal_ids.map((value) => String(value || "").trim()).filter(Boolean)
        : [];
      const tmuxSessions = Array.isArray(run?.source_tmux_sessions)
        ? run.source_tmux_sessions.map((value) => String(value || "").trim()).filter(Boolean)
        : [];
      if (!names.length) {
        return formatCompileTextList([...ids, ...tmuxSessions]);
      }
      if (names.length === ids.length) {
        return names.map((name, index) => {
          const details = [ids[index], tmuxSessions[index]].filter(Boolean);
          return details.length ? `${name} (${details.join(" / ")})` : name;
        }).join(", ");
      }
      const details = [...ids, ...tmuxSessions];
      return details.length ? `${names.join(", ")} (${details.join(", ")})` : names.join(", ");
    }

    function compileRunStatusLabel(status) {
      const normalized = String(status || "").trim().toLowerCase();
      if (normalized === "success") {
        return { text: "成功", tone: "ok" };
      }
      if (normalized === "failed") {
        return { text: "失败", tone: "warn" };
      }
      if (normalized === "running") {
        return { text: "运行中", tone: "info" };
      }
      if (normalized === "stalled") {
        return { text: "安装停滞", tone: "warn" };
      }
      if (normalized === "timed_out") {
        return { text: "已超时", tone: "warn" };
      }
      if (normalized === "unknown") {
        return { text: "未知", tone: "muted" };
      }
      return { text: normalized || "未知", tone: "muted" };
    }

    function isCompileRunEnded(run) {
      const normalized = String(run?.status || "").trim().toLowerCase();
      return normalized === "success" || normalized === "failed" || normalized === "timed_out";
    }

    function formatCompileRunProgress(run) {
      const phaseLabels = {
        preparing: "准备",
        compile: "编译",
        install: "安装",
      };
      const phase = phaseLabels[String(run?.current_phase || "")] || String(run?.current_phase || "");
      const specIndex = Number(run?.current_spec_index) || 0;
      const specCount = Number(run?.spec_count) || 0;
      const packagesCompleted = Number(run?.packages_completed);
      const packagesTotal = Number(run?.packages_total);
      const currentPackage = String(run?.current_package || "").trim();
      const parts = [];
      if (phase) {
        parts.push(phase);
      }
      if (specIndex > 0 && specCount > 0) {
        parts.push(`第 ${specIndex}/${specCount} 项`);
      }
      if (Number.isFinite(packagesCompleted) && Number.isFinite(packagesTotal) && packagesTotal > 0) {
        parts.push(`${packagesCompleted}/${packagesTotal} 包`);
      }
      if (currentPackage) {
        parts.push(currentPackage);
      }
      return parts.join(" · ") || "正在启动";
    }


    function copyTextWithHiddenTextarea(text) {
      const textarea = document.createElement("textarea");
      textarea.value = text;
      textarea.readOnly = true;
      textarea.setAttribute("aria-hidden", "true");
      textarea.style.cssText = "position:fixed;left:0;top:0;width:1px;height:1px;opacity:0;pointer-events:none;";
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

    function copyRowToClipboard(fields, button) {
      const text = fields
        .map(([key, value]) => `${key}: ${value || "-"}`)
        .join("\n");
      const markCopied = () => {
        if (showToast) {
          showToast("已复制该行全部字段。", "ok", 2000);
        }
        if (button) {
          const previousText = button.textContent;
          button.textContent = "已复制";
          setTimeout(() => { button.textContent = previousText || "复制"; }, 1500);
        }
      };
      if (navigator.clipboard?.writeText) {
        navigator.clipboard.writeText(text).then(markCopied, () => {
          if (copyTextWithHiddenTextarea(text)) {
            markCopied();
          } else if (showToast) {
            showToast("浏览器阻止自动复制，请检查剪贴板权限。", "warn", 3000);
          }
        });
      } else if (copyTextWithHiddenTextarea(text)) {
        markCopied();
      } else if (showToast) {
        showToast("当前浏览器不支持自动复制。", "warn", 3000);
      }
    }

    function renderCompilePendingRows(requests) {
      if (!compilePendingListEl) {
        return;
      }
      const items = Array.isArray(requests) ? requests : [];
      if (!items.length) {
        compilePendingListEl.innerHTML = `<tr><td colspan="8" class="meta-text">当前没有等待合并的构建请求。</td></tr>`;
        return;
      }
      compilePendingListEl.innerHTML = items.map((request) => {
        const waitText = Number(request.debounce_secs) > 0
          ? `${Number(request.debounce_secs)} 秒`
          : "立即";
        const requestLabel = request.request_id || request.file_path || "-";
        const requestKind = String(request.request_kind || "compile").toLowerCase() === "deploy" ? "部署" : "编译";
        const terminalIdentity = formatCompileTerminalIdentity(request);
        const installCommand = formatCompileCommand(request.install_command);
        const auditPaths = formatCompileTextList(request.audit_paths);
        const commandText = formatCompileCommand(request.command);
        const projectDisplay = request.project || "-";
        const projectDirText = request.project_dir || "";
        const projectPathText = request.project_path || "";
        const typeFullText = `${requestKind}\n${projectDisplay}${projectDirText ? "\n" + projectDirText : ""}`;
        const terminalFullText = `${terminalIdentity}${projectPathText ? "\n项目路径：" + projectPathText : ""}`;
        const copyFields = [
          ["请求", requestLabel],
          ["类型/项目", `${requestKind} / ${projectDisplay}${projectDirText ? " (" + projectDirText + ")" : ""}`],
          ["来源终端", terminalIdentity],
          ["项目路径", projectPathText],
          ["等待", waitText],
          ["命令", commandText],
          ["安装命令", installCommand],
          ["审计路径", auditPaths],
          ["备注", request.note || "-"],
        ];
        const copyData = escapeHtml(JSON.stringify(copyFields));
        return `
          <tr>
            <td class="mono-text compile-id-cell" title="${escapeHtml(requestLabel)}">${escapeHtml(requestLabel)}</td>
            <td title="${escapeHtml(typeFullText)}">
              <div><span class="compile-status-pill" data-tone="${requestKind === "部署" ? "info" : "muted"}">${escapeHtml(requestKind)}</span></div>
              <div class="mono-text" title="${escapeHtml(projectDisplay)}">${escapeHtml(projectDisplay)}</div>
              <div class="meta-text compile-path-text" title="${escapeHtml(projectDirText)}">${escapeHtml(projectDirText)}</div>
            </td>
            <td title="${escapeHtml(terminalFullText)}">
              <div class="mono-text" title="${escapeHtml(terminalIdentity)}">${escapeHtml(terminalIdentity)}</div>
              <div class="meta-text compile-path-text" title="${escapeHtml(projectPathText)}">项目路径：${escapeHtml(projectPathText)}</div>
            </td>
            <td class="mono-text" title="${escapeHtml(waitText)}">${escapeHtml(waitText)}</td>
            <td class="mono-text compile-command-cell" title="${escapeHtml(commandText)}">${escapeHtml(commandText)}</td>
            <td class="mono-text compile-command-cell" title="${escapeHtml(`${installCommand}\n${auditPaths}`)}">
              <div title="${escapeHtml(installCommand)}">${escapeHtml(installCommand)}</div>
              <div class="meta-text compile-path-text" title="${escapeHtml(auditPaths)}">${escapeHtml(auditPaths)}</div>
            </td>
            <td class="compile-note-cell" title="${escapeHtml(request.note || "-")}">${escapeHtml(request.note || "-")}</td>
            <td class="compile-copy-cell"><button class="mini-button compile-copy-btn" type="button" data-compile-copy="${copyData}">复制</button></td>
          </tr>
        `;
      }).join("");
    }

    function renderCompileRunningRows(runs) {
      if (!compileRunningListEl) {
        return;
      }
      const items = (Array.isArray(runs) ? runs : [])
        .filter((run) => {
          const normalized = String(run.status || "").toLowerCase();
          return normalized === "running" || normalized === "stalled";
        });
      if (!items.length) {
        compileRunningListEl.innerHTML = `<tr><td colspan="8" class="meta-text">当前没有正在执行的构建工作。</td></tr>`;
        return;
      }
      compileRunningListEl.innerHTML = items.map((run) => {
        const logPath = String(run.log_path || "");
        const logButton = logPath
          ? `<button class="mini-button" type="button" data-compile-log-path="${escapeHtml(logPath)}">打开</button>`
          : `<span class="meta-text">无</span>`;
        const projectsText = formatCompileTextList(run.projects);
        const terminalsText = formatCompileTerminalList(run);
        const progressText = formatCompileRunProgress(run);
        const progressTitle = [
          run.current_project,
          progressText,
          formatCompileCommand(run.current_command),
        ].filter(Boolean).join(" · ");
        const startedText = run.started_at || "-";
        const requestCount = String(Number(run.request_count) || 0);
        const copyFields = [
          ["运行", run.run_id || "-"],
          ["请求数", requestCount],
          ["项目", projectsText],
          ["来源终端", terminalsText],
          ["进度", progressText],
          ["开始时间", startedText],
          ["日志", logPath || "-"],
        ];
        const copyData = escapeHtml(JSON.stringify(copyFields));
        return `
          <tr>
            <td class="mono-text compile-id-cell" title="${escapeHtml(run.run_id || "")}">${escapeHtml(run.run_id || "-")}</td>
            <td class="mono-text" title="${escapeHtml(requestCount)}">${requestCount}</td>
            <td class="compile-list-cell" title="${escapeHtml(projectsText)}">${escapeHtml(projectsText)}</td>
            <td class="compile-list-cell" title="${escapeHtml(terminalsText)}">${escapeHtml(terminalsText)}</td>
            <td class="compile-progress-cell" title="${escapeHtml(progressTitle)}">${escapeHtml(progressText)}</td>
            <td class="mono-text compile-time-cell" title="${escapeHtml(startedText)}">${escapeHtml(startedText)}</td>
            <td>${logButton}</td>
            <td class="compile-copy-cell"><button class="mini-button compile-copy-btn" type="button" data-compile-copy="${copyData}">复制</button></td>
          </tr>
        `;
      }).join("");
    }

    function compileRunRowHtml(run) {
      const status = compileRunStatusLabel(run.status);
      const timeText = [run.started_at, run.finished_at].filter(Boolean).join(" -> ") || "-";
      const logPath = String(run.log_path || "");
      const logButton = logPath
        ? `<button class="mini-button" type="button" data-compile-log-path="${escapeHtml(logPath)}">打开</button>`
        : `<span class="meta-text">无</span>`;
      const projectsText = formatCompileTextList(run.projects);
      const terminalsText = formatCompileTerminalList(run);
      const requestCount = String(Number(run.request_count) || 0);
      const copyFields = [
        ["运行", run.run_id || "-"],
        ["状态", status.text],
        ["请求数", requestCount],
        ["项目", projectsText],
        ["来源终端", terminalsText],
        ["时间", timeText],
        ["日志", logPath || "-"],
      ];
      const copyData = escapeHtml(JSON.stringify(copyFields));
      return `
        <tr>
          <td class="mono-text compile-id-cell" title="${escapeHtml(run.run_id || "")}">${escapeHtml(run.run_id || "-")}</td>
          <td title="${escapeHtml(status.text)}"><span class="compile-status-pill" data-tone="${escapeHtml(status.tone)}">${escapeHtml(status.text)}</span></td>
          <td class="mono-text" title="${escapeHtml(requestCount)}">${requestCount}</td>
          <td class="compile-list-cell" title="${escapeHtml(projectsText)}">${escapeHtml(projectsText)}</td>
          <td class="compile-list-cell" title="${escapeHtml(terminalsText)}">${escapeHtml(terminalsText)}</td>
          <td class="mono-text compile-time-cell" title="${escapeHtml(timeText)}">${escapeHtml(timeText)}</td>
          <td>${logButton}</td>
          <td class="compile-copy-cell"><button class="mini-button compile-copy-btn" type="button" data-compile-copy="${copyData}">复制</button></td>
        </tr>
      `;
    }

    function yieldCompileHistoryRender() {
      return new Promise((resolve) => setTimeout(resolve, 0));
    }

    async function renderCompileRunRows(runs, requestToken) {
      if (!compileRunListEl) {
        return false;
      }
      const items = (Array.isArray(runs) ? runs : []).filter(isCompileRunEnded);
      if (!items.length) {
        compileRunListEl.innerHTML = `<tr><td colspan="8" class="meta-text">暂无已结束的构建运行历史。</td></tr>`;
        return true;
      }
      compileRunListEl.innerHTML = "";
      const chunkSize = 40;
      for (let start = 0; start < items.length; start += chunkSize) {
        if (requestToken !== state.compileStatusRequestToken) {
          return false;
        }
        if (start > 0) {
          await yieldCompileHistoryRender();
          if (requestToken !== state.compileStatusRequestToken) {
            return false;
          }
        }
        const html = items.slice(start, start + chunkSize).map(compileRunRowHtml).join("");
        compileRunListEl.insertAdjacentHTML("beforeend", html);
      }
      return true;
    }

    function renderCompileStatusMessage(data, historyLoading = false) {
      const pendingCount = Number(data?.pending_count) || 0;
      const runningCount = (Array.isArray(data?.runs) ? data.runs : [])
        .filter((run) => ["running", "stalled"].includes(String(run.status || "").toLowerCase()))
        .length;
      const runCount = Number(data?.run_count) || 0;
      setInlineStatus(
        compileStatusMessageEl,
        `队列：${data?.queue_dir || "-"} · 等待 ${pendingCount} · 进行中 ${runningCount} · 历史 ${runCount} · ${historyLoading ? "历史加载中 · " : ""}每 2 秒更新`,
        pendingCount > 0 || runningCount > 0 ? "info" : "muted",
      );
    }

    function renderCompileLiveStatus(data, options = {}) {
      const preserveHistory = options.preserveHistory === true;
      const historyLoading = options.historyLoading === true;
      state.compileStatus = data || null;
      const pendingCount = Number(data?.pending_count) || 0;
      const runningCount = (Array.isArray(data?.runs) ? data.runs : [])
        .filter((run) => ["running", "stalled"].includes(String(run.status || "").toLowerCase()))
        .length;
      const runCount = Number(data?.run_count) || 0;
      renderCompilePendingRows(data?.pending_requests);
      renderCompileRunningRows(data?.runs);
      if (compileRunListEl && !preserveHistory) {
        compileRunListEl.innerHTML = `<tr><td colspan="8" class="meta-text">正在后台加载 ${runCount} 条历史记录…</td></tr>`;
      }
      renderCompileStatusMessage(data, historyLoading);
    }

    async function loadCompileHistory(requestToken, liveData) {
      try {
        const data = await requestJson("/api/build/compile/status");
        if (requestToken !== state.compileStatusRequestToken) {
          return;
        }
        const rendered = await renderCompileRunRows(data?.runs, requestToken);
        if (!rendered || requestToken !== state.compileStatusRequestToken) {
          return;
        }
        const endedCount = Array.isArray(data?.runs)
          ? data.runs.filter(isCompileRunEnded).length
          : Number(data?.run_count) || 0;
        const mergedData = {
          ...liveData,
          run_count: endedCount,
          latest_log: data?.latest_log,
          runs: data?.runs,
          logs: data?.logs,
        };
        state.compileStatus = mergedData;
        renderCompileStatusMessage(mergedData);
      } catch (error) {
        if (requestToken !== state.compileStatusRequestToken) {
          return;
        }
        if (compileRunListEl) {
          compileRunListEl.innerHTML = `<tr><td colspan="8" class="meta-text">历史加载失败：${escapeHtml(error.message || "")}</td></tr>`;
        }
        renderCompileStatusMessage(liveData);
      }
    }

    function compileLogFileApiPath(path) {
      const raw = String(path || "").trim();
      if (!raw.startsWith("/")) {
        return raw;
      }
      const roots = [state.workspaceDir, state.defaultWorkspaceDir]
        .map((root) => String(root || "").replace(/\/+$/, ""))
        .filter(Boolean);
      for (const root of roots) {
        if (raw === root) {
          return "";
        }
        if (raw.startsWith(`${root}/`)) {
          return raw.slice(root.length + 1);
        }
      }
      return raw;
    }

    async function loadCompileStatus(options = {}) {
      if (!compilePendingListEl && !compileRunListEl) {
        return;
      }
      const includeHistory = options.includeHistory !== false;
      const silent = options.silent === true;
      const requestToken = includeHistory
        ? state.compileStatusRequestToken + 1
        : state.compileStatusRequestToken;
      if (includeHistory) {
        state.compileStatusRequestToken = requestToken;
      }
      if (!silent) {
        setButtonBusy(compileStatusRefreshButtonEl, true, "刷新中…");
        setInlineStatus(compileStatusMessageEl, "正在读取实时编译队列状态…", "info");
      }
      try {
        const data = await requestJson("/api/build/compile/status?include_history=false");
        if (requestToken !== state.compileStatusRequestToken || (!includeHistory && !liveRefreshActive)) {
          return;
        }
        renderCompileLiveStatus(data, {
          preserveHistory: !includeHistory,
          historyLoading: includeHistory,
        });
        if (includeHistory) {
          void loadCompileHistory(requestToken, data);
        }
      } catch (error) {
        if (requestToken !== state.compileStatusRequestToken) {
          return;
        }
        setInlineStatus(compileStatusMessageEl, error.message || "读取编译状态失败。", "warn");
        if (compilePendingListEl) {
          compilePendingListEl.innerHTML = `<tr><td colspan="8" class="meta-text">加载失败：${escapeHtml(error.message || "")}</td></tr>`;
        }
        if (compileRunningListEl) {
          compileRunningListEl.innerHTML = `<tr><td colspan="8" class="meta-text">加载失败：${escapeHtml(error.message || "")}</td></tr>`;
        }
        if (compileRunListEl) {
          compileRunListEl.innerHTML = `<tr><td colspan="8" class="meta-text">加载失败：${escapeHtml(error.message || "")}</td></tr>`;
        }
      } finally {
        if (!silent && requestToken === state.compileStatusRequestToken) {
          setButtonBusy(compileStatusRefreshButtonEl, false);
        }
      }
    }

    function stopLiveRefresh() {
      liveRefreshActive = false;
      if (liveRefreshTimer !== null) {
        clearTimeout(liveRefreshTimer);
        liveRefreshTimer = null;
      }
    }

    function scheduleLiveRefresh() {
      if (!liveRefreshActive) {
        return;
      }
      if (liveRefreshTimer !== null) {
        clearTimeout(liveRefreshTimer);
      }
      liveRefreshTimer = setTimeout(async () => {
        liveRefreshTimer = null;
        if (!liveRefreshActive) {
          return;
        }
        await loadCompileStatus({ includeHistory: false, silent: true });
        scheduleLiveRefresh();
      }, COMPILE_LIVE_REFRESH_MS);
    }

    async function startLiveRefresh() {
      stopLiveRefresh();
      liveRefreshActive = true;
      await loadCompileStatus();
      scheduleLiveRefresh();
    }

    async function openCompileLog(path) {
      const logPath = compileLogFileApiPath(path);
      if (!logPath) {
        return;
      }
      setActiveTab("workspace");
      await loadFile(logPath);
    }

    compileStatusRefreshButtonEl?.addEventListener("click", () => {
      if (liveRefreshActive) {
        if (liveRefreshTimer !== null) {
          clearTimeout(liveRefreshTimer);
          liveRefreshTimer = null;
        }
        void loadCompileStatus().finally(scheduleLiveRefresh);
      } else {
        loadCompileStatus();
      }
    });

    compileHistoryClearButtonEl?.addEventListener("click", async () => {
      if (!window.confirm("确定清空全部构建运行历史？正在进行和等待中的任务会保留，此操作不可撤销。")) {
        return;
      }
      setButtonBusy(compileHistoryClearButtonEl, true, "清空中…");
      setInlineStatus(compileStatusMessageEl, "正在清空构建运行历史…", "info");
      try {
        const result = await requestJson("/api/build/compile/history", { method: "DELETE" });
        const deletedCount = Number(result?.deleted_count) || 0;
        if (compileRunListEl) {
          compileRunListEl.innerHTML = `<tr><td colspan="8" class="meta-text">暂无已结束的构建运行历史。</td></tr>`;
        }
        setInlineStatus(
          compileStatusMessageEl,
          `已清空 ${deletedCount} 条构建运行历史，正在后台刷新状态…`,
          "ok",
        );
        showToast?.(`已清空 ${deletedCount} 条构建运行历史。`, "ok", 2500);
        setButtonBusy(compileHistoryClearButtonEl, false);
        void loadCompileStatus({ silent: true });
      } catch (error) {
        setInlineStatus(compileStatusMessageEl, error.message || "清空构建运行历史失败。", "warn");
        showToast?.(error.message || "清空构建运行历史失败。", "warn", 3500);
        setButtonBusy(compileHistoryClearButtonEl, false);
      }
    });

    compileRunningListEl?.addEventListener("click", (event) => {
      const button = event.target.closest("[data-compile-log-path]");
      if (!button) {
        return;
      }
      openCompileLog(button.dataset.compileLogPath);
    });

    compileRunListEl?.addEventListener("click", (event) => {
      const logButton = event.target.closest("[data-compile-log-path]");
      if (logButton) {
        openCompileLog(logButton.dataset.compileLogPath);
        return;
      }
      const copyButton = event.target.closest("[data-compile-copy]");
      if (copyButton) {
        let fields = [];
        try { fields = JSON.parse(copyButton.dataset.compileCopy || "[]"); } catch (_) {}
        copyRowToClipboard(fields, copyButton);
      }
    });

    compilePendingListEl?.addEventListener("click", (event) => {
      const copyButton = event.target.closest("[data-compile-copy]");
      if (copyButton) {
        let fields = [];
        try { fields = JSON.parse(copyButton.dataset.compileCopy || "[]"); } catch (_) {}
        copyRowToClipboard(fields, copyButton);
      }
    });

    compileRunningListEl?.addEventListener("click", (event) => {
      const copyButton = event.target.closest("[data-compile-copy]");
      if (copyButton) {
        let fields = [];
        try { fields = JSON.parse(copyButton.dataset.compileCopy || "[]"); } catch (_) {}
        copyRowToClipboard(fields, copyButton);
      }
    });

    return {
      loadCompileStatus,
      startLiveRefresh,
      stopLiveRefresh,
      openCompileLog,
    };
  }

  globalThis.WebClxCompileStatusManager = Object.freeze({ create: createCompileStatusManager });
})();
