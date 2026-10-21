const TERMINAL_DISCUSSION_MIN_PARTICIPANTS = 2;
const TERMINAL_DISCUSSION_AGENT_NAMES = ["codex", "claude", "deepseek"];

function terminalMultiTerminalDiscussionDom() {
  return {
    dialog: document.getElementById("terminal-multi-terminal-discussion-dialog"),
    form: document.getElementById("terminal-multi-terminal-discussion-form"),
    path: document.getElementById("terminal-multi-terminal-discussion-path"),
    topic: document.getElementById("terminal-multi-terminal-discussion-topic"),
    timebox: document.getElementById("terminal-multi-terminal-discussion-timebox"),
    terminals: document.getElementById("terminal-multi-terminal-discussion-terminals"),
    status: document.getElementById("terminal-multi-terminal-discussion-status"),
    result: document.getElementById("terminal-multi-terminal-discussion-result"),
    start: document.getElementById("terminal-multi-terminal-discussion-start"),
    close: document.getElementById("terminal-multi-terminal-discussion-close"),
  };
}

function terminalMultiTerminalDiscussionSetStatus(message, tone = "info") {
  const status = terminalMultiTerminalDiscussionDom().status;
  if (!status) return;
  status.hidden = !message;
  status.dataset.tone = tone;
  status.textContent = message;
}

function terminalMultiTerminalDiscussionCandidates(sessions, projectPath) {
  return (Array.isArray(sessions) ? sessions : []).filter((session) => {
    const agent = String(session?.activity_agent || "").toLowerCase();
    return session?.id && session.path === projectPath
      && TERMINAL_DISCUSSION_AGENT_NAMES.some((name) => agent.includes(name));
  });
}

function terminalMultiTerminalDiscussionParticipantLabel(participant) {
  return `${participant.name || participant.id} (${participant.id})`;
}

function terminalMultiTerminalDiscussionPrompt(options) {
  const {
    topic,
    participants,
    coordinator,
    recipient,
    startedAt,
    deadline,
    timeboxMinutes,
    discussionId = "",
  } = options;
  const isCoordinator = recipient.id === coordinator.id;
  const participantList = participants
    .map(terminalMultiTerminalDiscussionParticipantLabel)
    .join("、");
  const roleInstructions = isCoordinator
    ? [
      `你是协调终端 ${terminalMultiTerminalDiscussionParticipantLabel(coordinator)}。`,
      "收集各参与终端的独立意见；对证据薄弱、假设未证实或结论冲突处发起至多一轮追问；在硬截止时间到达时形成最终结论。",
      "使用终端消息向参与终端追问或宣布结论，并要求可验证的回复确认。",
    ]
    : [
      `你是参与终端 ${terminalMultiTerminalDiscussionParticipantLabel(recipient)}。`,
      "先独立分析，不等待也不迎合其他终端；完成后使用终端消息回复协调终端 "
        + `${terminalMultiTerminalDiscussionParticipantLabel(coordinator)}，仅在被追问时再补充一轮。`,
    ];

  return [
    `[多终端讨论${discussionId ? ` ${discussionId}` : ""}]`,
    ...roleInstructions,
    `参与终端：${participantList}。`,
    `开始时间：${startedAt}；时限：${timeboxMinutes} 分钟；硬截止时间：${deadline}。`,
    "讨论原则：第一性原理。先写清目标、不可违背的约束和可验证事实，再拆解关键假设；不要从惯例、权威或多数意见直接推出结论，不能用多数票代替证据。",
    "用时间说话：优先选择能在时限内产生可复现证据的最短验证路径；记录预计时间与实际耗时。已发生且可验证的结果高于口头预测。到截止时间仍无证据的主张标为未知，不以继续争论代替验证。",
    "边界：本轮只读；不修改文件、不构建、不部署，不启动或停止服务，不执行破坏性或外部副作用操作，不传递凭据或敏感信息。只讨论本题，发现需要实施的动作只列为建议。",
    "最终结论标准：目标与约束明确；关键事实有代码、日志、数据或最小实验等可复现证据；关键反例已检查；推荐结论能从事实和机制推出；风险、代价、回滚条件与验证步骤明确；未决分歧和未知项被如实保留。未达到标准时输出暂定结论，不制造共识。",
    "输出格式：结论；第一性原理拆解；证据与反证；时间记录；风险与代价；未决分歧；下一步验证。",
    "讨论问题：",
    topic,
  ].join("\n\n");
}

function terminalMultiTerminalDiscussionId() {
  if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
  return `discussion-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

async function broadcastTerminalDiscussion(options) {
  const discussionId = options.discussionId || terminalMultiTerminalDiscussionId();
  return Promise.all(options.participants.map(async (recipient) => {
    const message = terminalMultiTerminalDiscussionPrompt({
      ...options,
      discussionId,
      recipient,
    });
    try {
      const response = await requestJson("/api/terminal/sessions/message", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          target: recipient.id,
          data: message,
          submit: true,
          submit_enters: 1,
          bracketed_paste: true,
          verify_submission: true,
          delivery_id: `${discussionId}:${recipient.id}`,
        }),
      });
      return {
        ...recipient,
        submitted: response?.submitted === true,
        error: response?.submitted === true ? "" : "提交未获终端确认",
      };
    } catch (error) {
      return { ...recipient, submitted: false, error: error.message };
    }
  }));
}

function terminalMultiTerminalDiscussionOption(session, checked) {
  const label = document.createElement("label");
  label.className = "terminal-multi-model-advisor-option";
  const input = document.createElement("input");
  input.type = "checkbox";
  input.name = "terminal-multi-terminal-discussion-terminal";
  input.value = session.id;
  input.checked = checked;
  input.dataset.name = session.name || session.id;
  const text = document.createElement("span");
  text.textContent = session.name || session.id;
  const detail = document.createElement("small");
  detail.textContent = `${session.id} · ${session.activity_agent || "Agent"}${checked ? " · 当前" : ""}`;
  text.append(detail);
  label.append(input, text);
  return label;
}

async function loadTerminalMultiTerminalDiscussionChoices() {
  const dom = terminalMultiTerminalDiscussionDom();
  const response = await requestJson("/api/terminal/sessions?all=true");
  const sessions = Array.isArray(response?.sessions) ? response.sessions : [];
  const current = sessions.find((session) => session.id === state.activeSessionId)
    || state.sessions.find((session) => session.id === state.activeSessionId);
  const projectPath = current?.path || state.currentPath;
  const projectCandidates = terminalMultiTerminalDiscussionCandidates(sessions, projectPath);
  const currentGroup = typeof currentTerminalMessageGroup === "function"
    ? currentTerminalMessageGroup(sessions)
    : null;
  const candidates = currentGroup
    ? projectCandidates.filter((session) => currentGroup.members.some(
      (member) => member.id === session.id,
    ))
    : projectCandidates;
  dom.path.textContent = projectPath || "";
  const pathDetail = document.createElement("span");
  pathDetail.className = "terminal-multi-model-advisor-path-detail";
  pathDetail.textContent = currentGroup
    ? `已限定群组：${terminalMessageGroupLabel(currentGroup)}（${currentGroup.members.length}）`
    : "未检测到当前群组，使用同项目 Agent 终端。";
  dom.path.replaceChildren(pathDetail);
  dom.terminals.replaceChildren();
  candidates.forEach((session) => {
    dom.terminals.append(terminalMultiTerminalDiscussionOption(
      session,
      session.id === state.activeSessionId,
    ));
  });
  if (candidates.length === 0) {
    dom.terminals.textContent = currentGroup
      ? "当前群组没有可参与讨论的 Agent 终端"
      : "当前项目没有可参与讨论的 Agent 终端";
  }
}

async function openTerminalMultiTerminalDiscussionDialog() {
  const dom = terminalMultiTerminalDiscussionDom();
  if (!dom.dialog) return;
  dom.result.hidden = true;
  terminalMultiTerminalDiscussionSetStatus("正在读取当前群组终端…");
  dom.dialog.showModal();
  try {
    await loadTerminalMultiTerminalDiscussionChoices();
    terminalMultiTerminalDiscussionSetStatus("");
  } catch (error) {
  terminalMultiTerminalDiscussionSetStatus(`读取群组终端失败：${error.message}`, "warn");
  }
}

async function submitTerminalMultiTerminalDiscussion(event) {
  event.preventDefault();
  const dom = terminalMultiTerminalDiscussionDom();
  const participants = Array.from(dom.terminals.querySelectorAll("input:checked")).map((input) => ({
    id: input.value,
    name: input.dataset.name || input.value,
  }));
  if (participants.length < TERMINAL_DISCUSSION_MIN_PARTICIPANTS) {
    terminalMultiTerminalDiscussionSetStatus("请至少选择 2 个终端。", "warn");
    return;
  }
  const topic = dom.topic.value.trim();
  if (!topic) return;
  const timeboxMinutes = Math.min(240, Math.max(1, Number(dom.timebox.value) || 15));
  const coordinator = participants.find((item) => item.id === state.activeSessionId) || participants[0];
  const started = new Date();
  const deadline = new Date(started.getTime() + timeboxMinutes * 60 * 1000);
  const discussionId = terminalMultiTerminalDiscussionId();
  dom.start.disabled = true;
  dom.form.querySelectorAll("input, textarea").forEach((input) => { input.disabled = true; });
  terminalMultiTerminalDiscussionSetStatus(`正在向 ${participants.length} 个终端发送讨论契约…`);
  try {
    const results = await broadcastTerminalDiscussion({
      discussionId,
      topic,
      participants,
      coordinator,
      startedAt: started.toISOString(),
      deadline: deadline.toISOString(),
      timeboxMinutes,
    });
    const delivered = results.filter((item) => item.submitted);
    const failed = results.filter((item) => !item.submitted);
    terminalMultiTerminalDiscussionSetStatus(
      failed.length
        ? `已送达 ${delivered.length}/${results.length} 个终端；${failed.length} 个失败。`
        : `已向 ${delivered.length} 个终端发起讨论。`,
      failed.length ? "warn" : "ok",
    );
    dom.result.hidden = false;
    dom.result.textContent = [
      `讨论编号：${discussionId}`,
      `协调终端：${terminalMultiTerminalDiscussionParticipantLabel(coordinator)}`,
      `硬截止时间：${deadline.toISOString()}`,
      `已送达：${delivered.map((item) => item.name || item.id).join("、") || "无"}`,
      failed.length
        ? `失败：${failed.map((item) => `${item.name || item.id}（${item.error || "未知错误"}）`).join("、")}`
        : "失败：无",
    ].join("\n");
  } finally {
    dom.start.disabled = false;
    dom.form.querySelectorAll("input, textarea").forEach((input) => { input.disabled = false; });
  }
}

const terminalMultiTerminalDiscussionInitialDom = terminalMultiTerminalDiscussionDom();
terminalMultiTerminalDiscussionInitialDom.form?.addEventListener(
  "submit",
  submitTerminalMultiTerminalDiscussion,
);
terminalMultiTerminalDiscussionInitialDom.close?.addEventListener(
  "click",
  () => terminalMultiTerminalDiscussionInitialDom.dialog?.close(),
);
