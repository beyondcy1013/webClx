// webClx "工作代理" entrypoint: opens the existing "指定预设终端" / "Agent 任务"
// dialog (terminal-specified-task-dialog) with the working directory pinned to
// /home/third_party. The terminal name is left to the dialog's current logic
// (auto-derived from the active terminal / naming action); it is NOT forced to
// "工作代理". The button lives in the Agent page sidebar and links here via
// /terminal?work_agent=1; this module auto-opens the designate dialog.

const WORK_AGENT_WORKING_DIR = "/home/third_party";

function workAgentDom() {
  return {
    button: document.getElementById("terminal-work-agent-button"),
  };
}

function workAgentRequestedOnLoad() {
  try {
    const params = new URLSearchParams(window.location.search);
    return String(params.get("work_agent") || "").trim() === "1";
  } catch {
    return false;
  }
}

function terminalAgentProfileRequestedOnLoad() {
  try {
    return String(new URLSearchParams(window.location.search).get("agent_profile") || "").trim();
  } catch {
    return "";
  }
}

function clearWorkAgentQueryParam() {
  try {
    const url = new URL(window.location.href);
    if (!url.searchParams.has("work_agent")) {
      return;
    }
    url.searchParams.delete("work_agent");
    url.searchParams.delete("agent_profile");
    window.history.replaceState({}, "", `${url.pathname}${url.search}${url.hash}`);
  } catch {
    // URL manipulation may fail in sandboxed contexts; non-blocking.
  }
}

function resolveTerminalAgentPreset(presets, profile) {
  const selector = String(profile?.preset_selector || "").trim();
  const match = String(profile?.preset_match || "id").trim();
  if (match === "id") {
    const preset = presets.find((item) => String(item?.id || "").trim() === selector);
    if (!preset) throw new Error(`没有找到智能体指定的预设：${selector}`);
    return preset;
  }
  const expected = selector.toLocaleLowerCase("en-US");
  const exact = presets.filter(
    (item) => String(item?.name || "").trim().toLocaleLowerCase("en-US") === expected,
  );
  if (exact.length === 1) return exact[0];
  if (exact.length > 1 || match === "exact_name") {
    throw new Error(exact.length > 1 ? `预设名称不唯一：${selector}` : `没有找到预设：${selector}`);
  }
  const compatible = presets.filter((item) =>
    String(item?.name || "").trim().toLocaleLowerCase("en-US").includes(expected),
  );
  if (compatible.length !== 1) {
    throw new Error(
      compatible.length ? `找到多个匹配 ${selector} 的预设。` : `没有找到匹配 ${selector} 的预设。`,
    );
  }
  return compatible[0];
}

function profileAgentType(profile) {
  const agentType = String(profile?.agent_type || "codex").trim().toLowerCase();
  if (agentType === "native") {
    throw new Error("原生智能体应从 Agent 页面直接打开，不能作为终端启动。");
  }
  return agentType === "claude" ? "claude" : "codex";
}

async function launchTerminalAgentProfile(profileId) {
  updateStatus("正在加载智能体配置…", "info");
  const profile = await requestJson(`/api/agent/terminal-profiles/${encodeURIComponent(profileId)}`);
  const agentType = profileAgentType(profile);
  const response = await requestJson(specifiedPresetListEndpoint(agentType));
  const presets = Array.isArray(response?.presets) ? response.presets : [];
  const preset = resolveTerminalAgentPreset(presets, profile);
  const skillTask = `$${profile.skill_name}${profile.initial_task ? ` ${profile.initial_task}` : ""}`;
  const result = await executeSpecifiedPreset({
    action: "launch",
    agent: agentType,
    presetId: preset.id,
    cwd: profile.cwd,
    projectPath: profile.project_path,
    sessionAction: "new",
    task: skillTask,
    terminalName: profile.terminal_name || profile.name,
    quickStart: false,
    origin: "agent",
    ownerKey: `terminal-agent-profile:${profile.id}`,
    launchTerminal: launchTerminalSpecifiedPreset,
  });
  const launched = result.launchResult;
  updateStatus(`智能体“${profile.name}”已启动：${launched?.name || launched?.id || agentType}。`, "ok");
  if (window.parent !== window) {
    window.parent.postMessage({
      type: "webclx-agent-terminal-launch",
      status: "ready",
      profileId: profile.id,
      profileName: profile.name,
      presetName: preset.name || "",
      model: specifiedPresetModel(preset, agentType),
      sessionId: launched?.id || "",
    }, window.location.origin);
  }
}

function reportTerminalAgentLaunchFailure(profileId, error) {
  if (window.parent === window) return;
  window.parent.postMessage({
    type: "webclx-agent-terminal-launch",
    status: "error",
    profileId,
    profileName: "智能体",
    message: error?.message || String(error || "智能体启动失败。"),
  }, window.location.origin);
}

// Open the designate-preset ("Agent 任务") dialog with cwd pinned to the shared
// work-agent directory. terminalName is intentionally NOT set, so the dialog
// keeps its current name-derivation logic (source terminal name + naming action).
// Derive a default terminal name from the working directory instead of the
// current active terminal. /home/third_party -> "third_party". The dialog then
// applies its usual naming-action suffix (_new / _resume / _fork) on top of it.
function workAgentDefaultTerminalName(path = WORK_AGENT_WORKING_DIR) {
  const trimmed = String(path || "").trim().replace(/\/+$/g, "");
  if (!trimmed) {
    return "";
  }
  const basename = trimmed.split("/").filter(Boolean).pop() || "";
  return basename.trim();
}

function openWorkAgentDesignateDialog() {
  // cwd is pinned to /home/third_party, and the default terminal name is derived
  // from that directory (not the current active terminal). The dialog still
  // applies its own naming-action suffix (_new / _resume / _fork).
  const sourceTerminalName = workAgentDefaultTerminalName();
  openTerminalDesignatePresetDialog({
    cwd: WORK_AGENT_WORKING_DIR,
    sourceTerminalName,
  }).catch((error) => {
    updateStatus(error?.message || "打开工作代理对话框失败。", "warn");
  });
}

function initTerminalWorkAgentButton() {
  const dom = workAgentDom();
  if (dom.button) {
    dom.button.addEventListener("click", (event) => {
      event.preventDefault();
      openWorkAgentDesignateDialog();
    });
  }

  const profileId = terminalAgentProfileRequestedOnLoad();
  if (profileId) {
    clearWorkAgentQueryParam();
    window.addEventListener("load", () => {
      window.setTimeout(() => {
        launchTerminalAgentProfile(profileId).catch((error) => {
          updateStatus(`智能体启动失败：${error?.message || error}`, "warn");
          reportTerminalAgentLaunchFailure(profileId, error);
        });
      }, 300);
    });
  } else if (workAgentRequestedOnLoad()) {
    clearWorkAgentQueryParam();
    // Defer until the terminal surface has initialized so the designate dialog
    // runs against a ready session list and API preset loader.
    window.addEventListener("load", () => {
      window.setTimeout(openWorkAgentDesignateDialog, 300);
    });
  }
}

document.addEventListener("DOMContentLoaded", initTerminalWorkAgentButton);
