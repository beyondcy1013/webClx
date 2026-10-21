const TERMINAL_MULTI_MODEL_MAX_PRESETS = 3;
const TERMINAL_MULTI_MODEL_SYNTHESIS_INPUT_CHARS = 96 * 1024;
const TERMINAL_MULTI_MODEL_INLINE_MESSAGE_CHARS = 1200;

function terminalMultiModelAdvisorReportFile() {
  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const suffix = globalThis.crypto?.randomUUID?.().slice(0, 8) || Math.random().toString(16).slice(2, 10);
  return `${stamp}-${suffix}.md`;
}

function terminalMultiModelAdvisorDom() {
  return {
    dialog: document.getElementById("terminal-multi-model-advisor-dialog"),
    form: document.getElementById("terminal-multi-model-advisor-form"),
    path: document.getElementById("terminal-multi-model-advisor-path"),
    presets: document.getElementById("terminal-multi-model-advisor-presets"),
    terminals: document.getElementById("terminal-multi-model-advisor-terminals"),
    task: document.getElementById("terminal-multi-model-advisor-task"),
    timeout: document.getElementById("terminal-multi-model-advisor-timeout"),
    status: document.getElementById("terminal-multi-model-advisor-status"),
    result: document.getElementById("terminal-multi-model-advisor-result"),
    run: document.getElementById("terminal-multi-model-advisor-run"),
    close: document.getElementById("terminal-multi-model-advisor-close"),
  };
}

function terminalMultiModelAdvisorSetStatus(message, tone = "info") {
  const status = terminalMultiModelAdvisorDom().status;
  if (!status) return;
  status.hidden = !message;
  status.dataset.tone = tone;
  status.textContent = message;
}

function terminalMultiModelAdvisorResultText(record) {
  return String(record?.result || record?.error || record?.transcript_tail || "").trim();
}

function terminalMultiModelIndependentPrompt(task, index, total) {
  return [
    `你是第 ${index + 1}/${total} 位独立技术顾问。`,
    "只读分析当前项目，不修改文件，不执行部署，不向其它 Agent 发送消息。",
    "不要假设其它模型的结论。请基于可核验的代码证据输出：结论、证据、风险、建议修改、验证方法。",
    "用户问题：",
    task,
  ].join("\n\n");
}

function terminalMultiModelSynthesisPrompt(task, results) {
  const references = results.map((item, index) => [
    `### 顾问 ${index + 1}: ${item.preset.name || item.preset.id}`,
    `实际模型: ${item.actualModel || "未报告"}`,
    item.answer,
  ].join("\n")).join("\n\n");
  const bounded = references.length > TERMINAL_MULTI_MODEL_SYNTHESIS_INPUT_CHARS
    ? `${references.slice(0, TERMINAL_MULTI_MODEL_SYNTHESIS_INPUT_CHARS)}\n\n[其余内容因输入上限被截断]`
    : references;
  return [
    "你是多模型技术讨论主持人。只读分析，不修改文件。",
    "比较下列独立意见，不能用多数票代替证据判断。输出：共识、分歧、证据强弱、推荐方案、实施风险、验证清单。",
    `原始问题：${task}`,
    bounded,
  ].join("\n\n");
}

async function broadcastTerminalMultiModelAdvice(targets, message) {
  const outcomes = [];
  for (const target of targets) {
    try {
      const response = await requestJson("/api/terminal/sessions/message", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          target: target.id,
          data: message,
          submit: true,
          submit_enters: 1,
          bracketed_paste: true,
          verify_submission: true,
          delivery_id: message,
        }),
      });
      outcomes.push({ ...target, submitted: response?.submitted === true });
    } catch (error) {
      outcomes.push({ ...target, submitted: false, error: error.message });
    }
  }
  return outcomes;
}

async function runTerminalMultiModelAdvice(options) {
  const { task, cwd, sessionId, presets, targets = [], timeoutSecs = 1800 } = options;
  const results = [];
  for (let index = 0; index < presets.length; index += 1) {
    const preset = presets[index];
    options.onProgress?.(`正在咨询 ${preset.name || preset.id}（${index + 1}/${presets.length}）…`);
    try {
      const record = await executeSpecifiedPreset({
        action: "task",
        mode: "exec",
        agent: "codex",
        preset: { id: preset.id },
        cwd,
        task: terminalMultiModelIndependentPrompt(task, index, presets.length),
        timeoutSecs,
      });
      const answer = terminalMultiModelAdvisorResultText(record);
      results.push({
        preset,
        status: record?.status || "unknown",
        actualModel: record?.actual_model || record?.model || "",
        answer: answer || "未返回可显示的结果。",
      });
    } catch (error) {
      results.push({ preset, status: "failed", actualModel: "", answer: error.message });
    }
  }

  const successful = results.filter((item) => ["succeeded", "completed"].includes(item.status) && item.answer);
  if (successful.length === 0) throw new Error("所有模型咨询均失败，未生成综合报告。");
  options.onProgress?.("独立意见已收集，正在生成综合结论…");
  const moderator = successful[0].preset;
  const synthesisRecord = await executeSpecifiedPreset({
    action: "task",
    mode: "exec",
    agent: "codex",
    preset: { id: moderator.id },
    cwd,
    task: terminalMultiModelSynthesisPrompt(task, results),
    timeoutSecs,
  });
  const synthesis = terminalMultiModelAdvisorResultText(synthesisRecord);
  if (!synthesis) throw new Error("主持模型未返回综合结论。");

  const reportPath = `.webclx/advisor-reports/${terminalMultiModelAdvisorReportFile()}`;
  const report = await requestJson("/api/file", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      path: reportPath,
      content: [
        `# ${task.slice(0, 120)}`,
        "",
        `## 任务`,
        "",
        task,
        "",
        `## 综合`,
        "",
        synthesis,
        "",
        `## 独立意见`,
        "",
        ...results.map((item, index) => [
          `### 顾问 ${index + 1}: ${item.preset.name || item.preset.id}`,
          "",
          `状态：${item.status}`,
          "",
          `实际模型：${item.actualModel || "未报告"}`,
          "",
          item.answer,
        ].join("\n")),
      ].join("\n"),
    }),
  });
  const reportDisplayPath = `${state.workspaceDir || ""}${reportPath}`;
  const sourceName = String(options.sourceName || "webClx").trim() || "webClx";
  const message = synthesis.length <= TERMINAL_MULTI_MODEL_INLINE_MESSAGE_CHARS
    ? `[from ${sourceName}] 群组建议完成：${synthesis.replace(/\s+/g, " ")}；完整报告：${reportDisplayPath}`
    : `[from ${sourceName}] 群组建议完成，内容较长，请阅读：${reportDisplayPath}`;
  const deliveries = await broadcastTerminalMultiModelAdvice(targets, message);
  return { results, synthesis, report, reportPath, deliveries };
}

function terminalMultiModelAdvisorOption(item, type, checked = false) {
  const label = document.createElement("label");
  label.className = "terminal-multi-model-advisor-option";
  const input = document.createElement("input");
  input.type = "checkbox";
  input.name = type;
  input.value = item.id;
  input.checked = checked;
  input.dataset.name = item.name || item.id;
  input.dataset.model = item.model || "";
  const text = document.createElement("span");
  text.textContent = item.name || item.id;
  if (item.model || item.detail) {
    const detail = document.createElement("small");
    detail.textContent = item.model || item.detail;
    text.append(detail);
  }
  label.append(input, text);
  return label;
}

async function loadTerminalMultiModelAdvisorChoices() {
  const dom = terminalMultiModelAdvisorDom();
  const [presetResponse, sessionResponse] = await Promise.all([
    requestJson("/api/auth/api-presets"),
    requestJson("/api/terminal/sessions?all=true"),
  ]);
  const presets = (Array.isArray(presetResponse?.presets) ? presetResponse.presets : []).filter((item) => item?.id);
  dom.presets.replaceChildren();
  presets.forEach((item, index) => dom.presets.append(terminalMultiModelAdvisorOption(
    { ...item, model: item.model || item.config_value || item.default_model || "" },
    "terminal-multi-model-advisor-preset",
    index < Math.min(2, presets.length),
  )));
  if (presets.length === 0) dom.presets.textContent = "没有可用的 Codex API 预设";

  const sessions = Array.isArray(sessionResponse?.sessions) ? sessionResponse.sessions : [];
  const current = sessions.find((item) => item.id === state.activeSessionId);
  const currentGroup = typeof currentTerminalMessageGroup === "function"
    ? currentTerminalMessageGroup(sessions)
    : null;
  const candidates = sessions.filter((item) => {
    const agent = String(item.activity_agent || "").toLowerCase();
    return item.id !== state.activeSessionId && item.path === current?.path
      && (agent.includes("codex") || agent.includes("claude"));
  });
  const scopedCandidates = currentGroup
    ? candidates.filter((item) => currentGroup.members.some((member) => member.id === item.id))
    : candidates;
  dom.terminals.replaceChildren();
  scopedCandidates.forEach((item) => dom.terminals.append(terminalMultiModelAdvisorOption(
    { id: item.id, name: item.name, detail: item.activity_agent },
    "terminal-multi-model-advisor-terminal",
  )));
  dom.path.replaceChildren();
  const pathDetail = document.createElement("span");
  pathDetail.textContent = [
    current?.path || state.currentPath || "",
    currentGroup ? `已限定群组：${terminalMessageGroupLabel(currentGroup)}（${currentGroup.members.length}）` : "未检测到当前群组，使用同项目 Agent 终端。",
  ].filter(Boolean).join(" · ");
  dom.path.append(pathDetail);
  if (scopedCandidates.length === 0) {
    dom.terminals.textContent = currentGroup ? "当前群组没有其它 Agent 终端" : "当前项目没有其它 Agent 终端";
  }
}

async function openTerminalMultiModelAdvisorDialog() {
  const dom = terminalMultiModelAdvisorDom();
  if (!dom.dialog) return;
  const current = state.sessions.find((item) => item.id === state.activeSessionId);
  dom.path.textContent = current?.path || state.currentPath || "";
  dom.result.hidden = true;
  terminalMultiModelAdvisorSetStatus("正在读取模型与当前群组终端…");
  dom.dialog.showModal();
  try {
    await loadTerminalMultiModelAdvisorChoices();
    terminalMultiModelAdvisorSetStatus("");
  } catch (error) {
  terminalMultiModelAdvisorSetStatus(`读取群组终端失败：${error.message}`, "warn");
  }
}

async function submitTerminalMultiModelAdvisor(event) {
  event.preventDefault();
  const dom = terminalMultiModelAdvisorDom();
  const selectedPresets = Array.from(dom.presets.querySelectorAll("input:checked"));
  if (selectedPresets.length < 1 || selectedPresets.length > TERMINAL_MULTI_MODEL_MAX_PRESETS) {
    terminalMultiModelAdvisorSetStatus("请选择 1–3 个模型。", "warn");
    return;
  }
  const task = dom.task.value.trim();
  if (!task) return;
  const current = state.sessions.find((item) => item.id === state.activeSessionId);
  const targets = Array.from(dom.terminals.querySelectorAll("input:checked")).map((input) => ({ id: input.value, name: input.dataset.name }));
  dom.run.disabled = true;
  dom.form.querySelectorAll("input, textarea").forEach((input) => { input.disabled = true; });
  try {
    const outcome = await runTerminalMultiModelAdvice({
      task,
      cwd: typeof terminalSpecifiedTaskResolveWorkspacePath === "function"
        ? terminalSpecifiedTaskResolveWorkspacePath(current?.path || state.currentPath)
        : current?.path || state.currentPath,
      sessionId: state.activeSessionId,
      sourceName: current?.name,
      presets: selectedPresets.map((input) => ({ id: input.value, name: input.dataset.name, model: input.dataset.model })),
      targets,
      timeoutSecs: Number(dom.timeout.value),
      onProgress: (message) => terminalMultiModelAdvisorSetStatus(message),
    });
    const failedDeliveries = outcome.deliveries.filter((item) => !item.submitted).length;
    terminalMultiModelAdvisorSetStatus(
      failedDeliveries ? `报告已生成；${failedDeliveries} 个终端通知失败。` : "讨论和报告已完成。",
      failedDeliveries ? "warn" : "ok",
    );
    dom.result.hidden = false;
    dom.result.textContent = `${outcome.synthesis}\n\n报告：${outcome.report.absolute_path}`;
  } catch (error) {
    terminalMultiModelAdvisorSetStatus(`讨论失败：${error.message}`, "warn");
  } finally {
    dom.run.disabled = false;
    dom.form.querySelectorAll("input, textarea").forEach((input) => { input.disabled = false; });
  }
}

const terminalMultiModelAdvisorInitialDom = terminalMultiModelAdvisorDom();
terminalMultiModelAdvisorInitialDom.form?.addEventListener("submit", submitTerminalMultiModelAdvisor);
terminalMultiModelAdvisorInitialDom.close?.addEventListener("click", () => terminalMultiModelAdvisorInitialDom.dialog?.close());
