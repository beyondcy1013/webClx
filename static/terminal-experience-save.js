const TERMINAL_EXPERIENCE_SAVE_DELAY_MS = 3000;

function terminalExperienceSavePrompt(session = {}, { closeAfterSave = true } = {}) {
  const sessionId = session?.id || "";
  const encodedId = typeof encodeURIComponent === "function" ? encodeURIComponent(sessionId) : sessionId;
  const closeCmd = sessionId
    ? `webclx terminal close "${sessionId}" || curl -s -X POST -H "X-WebClx-Local-Token: \${WEBCLX_LOCAL_API_TOKEN:-$(cat ~/.webclx-local-api-token 2>/dev/null)}" "http://127.0.0.1:11111/api/terminal/sessions/${encodedId}/close"`
    : `webclx terminal close || curl -s -X POST -H "X-WebClx-Local-Token: \${WEBCLX_LOCAL_API_TOKEN:-$(cat ~/.webclx-local-api-token 2>/dev/null)}" "http://127.0.0.1:11111/api/terminal/sessions/close"`;
  const completion = closeAfterSave
    ? `确认完成后，请立即调用自行关闭终端接口自毁并退出本终端：${closeCmd}。`
    : "完成后请汇报落盘结果，保留当前终端继续对话，不要关闭或退出本终端。";
  return `[经验落盘] 请把本次关键运维经验落盘到当前项目工作目录 .agents 下的局部运维 skill（运维技能名为：项目名-ops）。优先更新已有运维 skill；如果有现成的运维技能只是不同名则改名为“项目名-ops”并同步更新目录与 name 元数据；没有则创建 .agents/skills/<项目名>-ops/SKILL.md，包含 name、description 元数据。落盘原则：落盘仅为未来遇到同类问题时提供关键字供反查追溯，只需指路，无需过度详细；除非内容较多或涉及多个项目，可写入对应文档中并在运维技能或主文档中仅保留关键字指路反查，以保持追加到运维技能或主文档的简洁性；若旧内容与新经验矛盾，更新旧内容而非重复追加；无重要经验则不写空泛内容。技能修改或创建完成后，使用技能同步技能 skill-bidirectional-sync 同步到各个 Agent（Codex、Claude、Cursor、Gemini、ZCode、Devin 等）配置目录中。${completion}`;
}

const TERMINAL_EXPERIENCE_SAVE_PROMPT = terminalExperienceSavePrompt();

async function sendTerminalExperienceSave(event) {
  const button = event.currentTarget;
  if (button.disabled) return;
  const sessionId = state.activeSessionId;
  const session = state.sessions.find((item) => item.id === sessionId);
  if (!session || !isTerminalConnected()) {
    setTerminalToolsStatus("当前终端尚未连接。", "warn");
    return;
  }
  button.disabled = true;
  setTerminalToolsStatus("正在发送经验落盘要求…", "info");
  try {
    const sent = await sendTerminalAutoTypedInput(
      terminalExperienceSavePrompt(session, { closeAfterSave: false }),
      { sessionId, throwOnError: true },
    );
    if (!sent) throw new Error("经验落盘要求未发送。");
    setTerminalToolsStatus("已发送经验落盘要求", "ok");
    updateStatus(`已向终端“${session.name || sessionId}”发送经验落盘要求。`, "ok");
    closeTerminalToolsMenu();
  } catch (error) {
    setTerminalToolsStatus(error?.message || "经验落盘要求发送失败。", "warn");
  } finally {
    button.disabled = false;
  }
}

function terminalExperienceSaveDialogRefs() {
  return {
    dialog: document.getElementById("terminal-experience-save-dialog"),
    form: document.getElementById("terminal-experience-save-form"),
    title: document.getElementById("terminal-experience-save-title"),
    countdown: document.getElementById("terminal-experience-save-countdown"),
    status: document.getElementById("terminal-experience-save-status"),
    yes: document.getElementById("terminal-experience-save-yes"),
    no: document.getElementById("terminal-experience-save-no"),
    cancel: document.getElementById("terminal-experience-save-cancel"),
  };
}

function setTerminalExperienceSaveStatus(refs, message, tone = "muted") {
  if (!refs.status) return;
  refs.status.textContent = message;
  refs.status.dataset.tone = tone;
  refs.status.hidden = !message;
}

function scheduleTerminalExperienceSave(session) {
  return requestJson("/api/terminal/scheduled-inputs", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      session_id: session.id,
      text: terminalExperienceSavePrompt(session),
      due_at: Date.now() + TERMINAL_EXPERIENCE_SAVE_DELAY_MS,
      label: "经验落盘",
      send_enter: true,
      task_type: "agent_message",
      terminal_mode: "existing",
      repeat_interval_millis: 0,
    }),
  });
}

function reportTerminalExperienceSaveOutcome(confirmation, report) {
  confirmation.experienceResult?.then((outcome) => {
    if (outcome.experienceScheduled) report("已登记经验落盘指令。", "ok");
    else if (outcome.error) report(`经验落盘指令登记结果未确认：${outcome.error}。请核查定时任务，勿重复发送。`, "warn");
  });
}

async function confirmTerminalExperienceSaveAndHide(session) {
  const refs = terminalExperienceSaveDialogRefs();
  if (!refs.dialog || !refs.yes || !refs.no || !session?.id) {
    return { shouldHide: false, shouldClose: false, action: "cancel", experienceScheduled: false, error: "隐藏终端确认窗口不可用。" };
  }
  if (refs.dialog.open) return { shouldHide: false, shouldClose: false, action: "cancel" };

  return new Promise((resolve) => {
    const attempt = {
      timer: null,
      deadline: Date.now() + 10000,
      settled: false,
    };

    // outcome: "yes" = 落盘并隐藏, "no" = 正常关闭, "cancel" = 中止操作
    const finishYes = () => settle("yes");
    const finishNo = () => settle("no");
    const finishCancel = () => settle("cancel");

    const handleFormCancel = (event) => {
      event.preventDefault();
      finishCancel();
    };

    const handleWindowKeyDown = (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        finishCancel();
      }
    };

    const settle = (outcome) => {
      if (attempt.settled) return;
      attempt.settled = true;
      window.clearInterval(attempt.timer);
      window.removeEventListener("keydown", handleWindowKeyDown);
      refs.dialog.removeEventListener("close", finishNo);
      refs.dialog.removeEventListener("cancel", handleFormCancel);
      refs.form?.removeEventListener("submit", handleFormCancel);
      refs.yes.removeEventListener("click", finishYes);
      refs.no.removeEventListener("click", finishNo);
      refs.cancel?.removeEventListener("click", finishCancel);

      // Cancel: aborts entirely; no change, no close, no hide.
      if (outcome === "cancel") {
        if (refs.dialog.open) refs.dialog.close("cancel");
        resolve({ shouldHide: false, shouldClose: false, action: "cancel" });
        return;
      }

      if (refs.dialog.open) refs.dialog.close(outcome);

      if (outcome === "yes") {
        // "是"：需要长时间后台处理经验落盘，转入隐藏终端并在落盘后自毁关闭
        const experienceResult = Promise.resolve().then(() => scheduleTerminalExperienceSave(session)).then(
          () => ({ experienceScheduled: true, error: "" }),
          (error) => ({ experienceScheduled: false, error: error?.message || "经验落盘指令排程失败。" }),
        );
        resolve({
          shouldHide: true,
          shouldClose: false,
          action: "save_and_hide",
          experienceResult,
        });
      } else {
        // "否" 或超时：无需经验落盘，正常关闭终端
        resolve({
          shouldHide: false,
          shouldClose: true,
          action: "close",
        });
      }
    };

    const tick = () => {
      const remaining = Math.max(0, Math.ceil((attempt.deadline - Date.now()) / 1000));
      if (refs.countdown) refs.countdown.textContent = `${remaining} 秒后默认直接关闭`;
      if (remaining === 0) finishNo();
    };

    window.addEventListener("keydown", handleWindowKeyDown);
    refs.dialog.addEventListener("close", finishNo);
    refs.dialog.addEventListener("cancel", handleFormCancel);
    refs.form?.addEventListener("submit", handleFormCancel);
    if (refs.title) refs.title.textContent = `结束终端"${session.name || session.id}"`;
    setTerminalExperienceSaveStatus(refs, "");
    refs.yes.disabled = false;
    refs.no.disabled = false;
    if (refs.cancel) refs.cancel.disabled = false;
    tick();
    attempt.timer = window.setInterval(tick, 250);
    if (typeof refs.dialog.show === "function") {
      refs.dialog.show();
    } else {
      refs.dialog.showModal();
    }
    refs.no?.focus();
    refs.yes.addEventListener("click", finishYes);
    refs.no.addEventListener("click", finishNo);
    refs.cancel?.addEventListener("click", finishCancel);
  });
}

