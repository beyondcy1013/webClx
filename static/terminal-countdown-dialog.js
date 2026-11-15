// webClx 终端右侧中间排队倒计时浮动对话框组件
// 位于跳顶部与跳底部按钮之间，实时展示当前终端的排队倒计时，并支持快捷延迟 20秒 / 1分钟 / 2分钟。
(() => {
  "use strict";

  let activeCountdownTask = null;
  let countdownTickTimer = null;
  let cardExpanded = false;

  function findCurrentTerminalCountdownTask() {
    const activeId = typeof state !== "undefined" ? state?.activeSessionId : "";
    if (typeof terminalPasteScheduledTasks === "undefined" || !terminalPasteScheduledTasks) {
      return null;
    }

    const now = Date.now();
    let earliest = null;

    for (const task of terminalPasteScheduledTasks.values()) {
      if (!task || task.status === "completed" || task.status === "cancelled") {
        continue;
      }
      const dueAt = Number(task.at || task.dueAt || task.due_at || 0);
      if (!Number.isFinite(dueAt) || dueAt <= now - 2000) {
        continue;
      }

      // 优先匹配当前活动终端
      const taskSessionId = String(task.sessionId || task.session_id || "").trim();
      const isMatch = !activeId || taskSessionId === activeId;
      if (!isMatch && earliest && earliest.isCurrentSession) {
        continue;
      }

      const item = {
        taskId: task.taskId || task.id,
        sessionId: taskSessionId,
        terminalName: task.terminalName || task.terminal_name || "",
        dueAt,
        text: task.text || task.preview || "",
        isCurrentSession: Boolean(activeId && taskSessionId === activeId),
      };

      if (!earliest) {
        earliest = item;
      } else if (item.isCurrentSession && !earliest.isCurrentSession) {
        earliest = item;
      } else if (item.isCurrentSession === earliest.isCurrentSession && item.dueAt < earliest.dueAt) {
        earliest = item;
      }
    }

    return earliest;
  }

  function formatCountdownTime(totalSeconds) {
    if (!Number.isFinite(totalSeconds) || totalSeconds <= 0) {
      return "00:00";
    }
    const mins = Math.floor(totalSeconds / 60);
    const secs = totalSeconds % 60;
    if (mins < 60) {
      return `${String(mins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
    }
    const hours = Math.floor(mins / 60);
    const remMins = mins % 60;
    return `${String(hours).padStart(2, "0")}:${String(remMins).padStart(2, "0")}:${String(secs).padStart(2, "0")}`;
  }

  function updateCountdownView() {
    const dockEl = document.getElementById("terminal-countdown-dock");
    const toggleTextEl = document.getElementById("terminal-countdown-toggle-text");
    const toggleBtn = document.getElementById("terminal-countdown-toggle");
    const digitsEl = document.getElementById("terminal-countdown-digits");
    const descEl = document.getElementById("terminal-countdown-desc");
    const previewEl = document.getElementById("terminal-countdown-preview");
    const delayButtons = document.querySelectorAll(".terminal-countdown-delay-btn");
    const flushBtn = document.getElementById("terminal-countdown-flush-btn");
    const cancelBtn = document.getElementById("terminal-countdown-cancel-btn");

    if (!dockEl) return;

    activeCountdownTask = findCurrentTerminalCountdownTask();

    if (activeCountdownTask) {
      const now = Date.now();
      const remainingSeconds = Math.max(0, Math.ceil((activeCountdownTask.dueAt - now) / 1000));
      const formatted = formatCountdownTime(remainingSeconds);

      if (toggleTextEl) toggleTextEl.textContent = formatted;
      if (toggleBtn) {
        toggleBtn.classList.add("active");
        toggleBtn.title = `排队倒计时中：剩余 ${formatted}，点击展开设置`;
      }
      if (digitsEl) digitsEl.textContent = formatted;
      if (descEl) {
        const timeStr = new Date(activeCountdownTask.dueAt).toLocaleTimeString();
        descEl.textContent = `预计 ${timeStr} 投递${activeCountdownTask.terminalName ? ` (${activeCountdownTask.terminalName})` : ""}`;
      }
      if (previewEl) {
        const previewText = String(activeCountdownTask.text || "").trim();
        if (previewText) {
          previewEl.hidden = false;
          previewEl.textContent = previewText.length > 50 ? `${previewText.slice(0, 50)}…` : previewText;
        } else {
          previewEl.hidden = true;
        }
      }

      delayButtons.forEach((btn) => { btn.disabled = false; });
      if (flushBtn) flushBtn.disabled = false;
      if (cancelBtn) cancelBtn.disabled = false;
    } else {
      if (toggleTextEl) toggleTextEl.textContent = "倒计时";
      if (toggleBtn) {
        toggleBtn.classList.remove("active");
        toggleBtn.title = "排队倒计时（当前无进行中的任务）";
      }
      if (digitsEl) digitsEl.textContent = "--:--";
      if (descEl) descEl.textContent = "当前终端无排队任务";
      if (previewEl) {
        previewEl.hidden = true;
        previewEl.textContent = "";
      }

      delayButtons.forEach((btn) => { btn.disabled = true; });
      if (flushBtn) flushBtn.disabled = true;
      if (cancelBtn) cancelBtn.disabled = true;
    }
  }

  async function delayActiveCountdownTask(seconds) {
    if (!activeCountdownTask || !activeCountdownTask.taskId) {
      if (typeof updateStatus === "function") {
        updateStatus("当前终端没有进行中的排队任务，无需推迟。", "info");
      }
      return;
    }

    const taskId = activeCountdownTask.taskId;
    const now = Date.now();
    const newDueAt = Math.max(activeCountdownTask.dueAt, now) + seconds * 1000;

    try {
      if (typeof updateStatus === "function") {
        updateStatus(`正在推迟倒计时 ${seconds} 秒…`, "info");
      }
      await requestJson(`/api/terminal/scheduled-inputs/${encodeURIComponent(taskId)}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ due_at: newDueAt }),
      });

      // 更新内存中的任务时间
      if (typeof terminalPasteScheduledTasks !== "undefined" && terminalPasteScheduledTasks) {
        const cached = terminalPasteScheduledTasks.get(taskId);
        if (cached) {
          cached.at = newDueAt;
          cached.dueAt = newDueAt;
          cached.due_at = newDueAt;
        }
      }
      activeCountdownTask.dueAt = newDueAt;
      updateCountdownView();

      const label = seconds >= 60 ? `${Math.floor(seconds / 60)} 分钟` : `${seconds} 秒`;
      if (typeof updateStatus === "function") {
        updateStatus(`已将排队倒计时推迟 ${label}。`, "ok");
      }
    } catch (error) {
      if (typeof updateStatus === "function") {
        updateStatus(`推迟倒计时失败：${error.message || error}`, "warn");
      }
    }
  }

  async function flushActiveCountdownTask() {
    if (!activeCountdownTask || !activeCountdownTask.taskId) return;
    const taskId = activeCountdownTask.taskId;
    try {
      if (typeof updateStatus === "function") {
        updateStatus("正在立即发送排队消息…", "info");
      }
      await requestJson(`/api/terminal/scheduled-inputs/${encodeURIComponent(taskId)}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ due_at: Date.now() + 100 }),
      });
      if (typeof refreshTerminalPasteScheduledTasks === "function") {
        await refreshTerminalPasteScheduledTasks();
      }
      updateCountdownView();
      if (typeof updateStatus === "function") {
        updateStatus("排队消息已提前触发发送。", "ok");
      }
    } catch (error) {
      if (typeof updateStatus === "function") {
        updateStatus(`提前发送失败：${error.message || error}`, "warn");
      }
    }
  }

  async function cancelActiveCountdownTask() {
    if (!activeCountdownTask || !activeCountdownTask.taskId) return;
    const task = activeCountdownTask;
    const taskId = task.taskId;

    // 二次确认防误触：取消必须要求确认
    const targetInfo = task.terminalName ? `目标终端：${task.terminalName}\n` : "";
    const now = Date.now();
    const remainingSeconds = Math.max(0, Math.ceil((task.dueAt - now) / 1000));
    const timeInfo = `剩余倒计时：${formatCountdownTime(remainingSeconds)}\n`;
    const previewText = String(task.text || "").trim();
    const previewInfo = previewText ? `消息预览：${previewText.length > 80 ? `${previewText.slice(0, 80)}…` : previewText}\n` : "";

    const confirmMessage = `确定要取消该排队倒计时任务吗？\n\n${targetInfo}${timeInfo}${previewInfo}\n⚠️ 取消后该排队消息将被直接删除，无法撤销！`;
    const hasConfirm = typeof window !== "undefined" && typeof window.confirm === "function";
    if (hasConfirm && !window.confirm(confirmMessage)) {
      return;
    }

    try {
      if (typeof updateStatus === "function") {
        updateStatus("正在取消排队任务…", "info");
      }
      await requestJson(`/api/terminal/scheduled-inputs/${encodeURIComponent(taskId)}`, {
        method: "DELETE",
      });
      if (typeof terminalPasteScheduledTasks !== "undefined" && terminalPasteScheduledTasks) {
        terminalPasteScheduledTasks.delete(taskId);
      }
      activeCountdownTask = null;
      updateCountdownView();
      if (typeof updateStatus === "function") {
        updateStatus("已取消排队任务。", "ok");
      }
    } catch (error) {
      if (typeof updateStatus === "function") {
        updateStatus(`取消排队任务失败：${error.message || error}`, "warn");
      }
    }
  }

  function toggleCountdownCard(force) {
    const cardEl = document.getElementById("terminal-countdown-card");
    const toggleBtn = document.getElementById("terminal-countdown-toggle");
    if (!cardEl) return;
    const nextState = typeof force === "boolean" ? force : cardEl.hidden;
    cardEl.hidden = !nextState;
    cardExpanded = nextState;
    if (toggleBtn) {
      toggleBtn.setAttribute("aria-expanded", String(nextState));
    }
    if (nextState) {
      updateCountdownView();
    }
  }

  function bindCountdownDockEvents() {
    const toggleBtn = document.getElementById("terminal-countdown-toggle");
    const closeBtn = document.getElementById("terminal-countdown-close");
    const flushBtn = document.getElementById("terminal-countdown-flush-btn");
    const cancelBtn = document.getElementById("terminal-countdown-cancel-btn");
    const delayButtons = document.querySelectorAll(".terminal-countdown-delay-btn");

    toggleBtn?.addEventListener("click", () => {
      toggleCountdownCard();
    });

    closeBtn?.addEventListener("click", () => {
      toggleCountdownCard(false);
    });

    delayButtons.forEach((btn) => {
      btn.addEventListener("click", async () => {
        const seconds = Number(btn.dataset.delaySeconds || 20);
        await delayActiveCountdownTask(seconds);
      });
    });

    flushBtn?.addEventListener("click", async () => {
      await flushActiveCountdownTask();
    });

    cancelBtn?.addEventListener("click", async () => {
      await cancelActiveCountdownTask();
    });
  }

  function startCountdownTickLoop() {
    if (countdownTickTimer) return;
    countdownTickTimer = window.setInterval(() => {
      updateCountdownView();
    }, 1000);
  }

  // 暴露给全局，方便外部调用或测试
  const exportTarget = typeof window !== "undefined" ? window : globalThis;
  exportTarget.WebClxTerminalCountdownDialog = {
    updateCountdownView,
    delayActiveCountdownTask,
    flushActiveCountdownTask,
    cancelActiveCountdownTask,
    toggleCountdownCard,
    findCurrentTerminalCountdownTask,
    formatCountdownTime,
  };

  if (typeof document !== "undefined") {
    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", () => {
        bindCountdownDockEvents();
        startCountdownTickLoop();
        updateCountdownView();
      });
    } else {
      bindCountdownDockEvents();
      startCountdownTickLoop();
      updateCountdownView();
    }
  }
})();
