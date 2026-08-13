(function initializeTerminalAgentProfiles() {
  "use strict";

  const state = {
    profiles: [],
    codexPresets: [],
    claudePresets: [],
    skills: [],
    terminalSessions: [],
    nativeSessions: [],
    activeTerminalFrame: null,
  };
  const byId = (id) => document.getElementById(id);

  async function requestJson(url, options = {}) {
    const response = await fetch(url, {
      ...options,
      headers: { "Content-Type": "application/json", ...(options.headers || {}) },
    });
    if (!response.ok) {
      throw new Error((await response.text()) || `HTTP ${response.status}`);
    }
    return response.json();
  }

  function setStatus(message = "", tone = "muted", form = false) {
    const element = byId(form ? "agent-terminal-profile-form-status" : "agent-terminal-profile-status");
    if (!element) return;
    element.textContent = message;
    element.dataset.tone = tone;
  }

  function makeButton(label, action, profileId, className = "") {
    const button = document.createElement("button");
    button.type = "button";
    button.textContent = label;
    button.dataset.action = action;
    button.dataset.profileId = profileId;
    if (className) button.className = className;
    if (action === "open") {
      button.title = "打开最近的智能体会话";
      button.setAttribute("aria-label", "打开最近的智能体会话");
    } else if (action === "launch") {
      button.title = "新建智能体会话";
      button.setAttribute("aria-label", "新建智能体会话");
    }
    return button;
  }

  function profileOwnerKey(profileId) {
    return `terminal-agent-profile:${String(profileId || "").trim()}`;
  }

  function profileAgentType(profile) {
    const value = String(profile?.agent_type || "").trim().toLowerCase();
    if (["native", "codex", "claude"].includes(value)) return value;
    return "codex";
  }

  function profileAgentTypeLabel(profile) {
    return { native: "原生", codex: "Codex", claude: "Claude" }[profileAgentType(profile)];
  }

  function profilePresets(profile) {
    return profileAgentType(profile) === "claude" ? state.claudePresets : state.codexPresets;
  }

  function normalizedPath(value) {
    return String(value || "").trim().replace(/\/+$/g, "") || "/";
  }

  function sessionRecency(session) {
    const value = Number(session?.last_opened_at) || Number(session?.updated_at) || Number(session?.created_at) || 0;
    return value > 0 && value < 1_000_000_000_000 ? value * 1000 : value;
  }

  function sessionsForProfile(profile) {
    const ownerKey = profileOwnerKey(profile?.id);
    const owned = state.terminalSessions.filter(
      (session) => session?.origin === "agent" && session?.owner_key === ownerKey,
    );
    const candidates = owned.length > 0
      ? owned
      : state.terminalSessions.filter((session) =>
          session?.origin === "agent"
          && !String(session?.owner_key || "").trim()
          && normalizedPath(session?.display_path) === normalizedPath(profile?.cwd),
        );
    return candidates.slice().sort((left, right) => sessionRecency(right) - sessionRecency(left));
  }

  function nativeSessionsForProfile(profile) {
    return state.nativeSessions
      .filter((session) => session?.profile_id === profile?.id)
      .slice()
      .sort((left, right) => sessionRecency(right) - sessionRecency(left));
  }

  function nativeAgentController() {
    const controller = window.webClxNativeAgent;
    if (!controller?.openSession || !controller?.createSession) {
      throw new Error("原生智能体界面尚未就绪，请稍后重试。");
    }
    return controller;
  }

  function agentSessionFrameUrl(session) {
    const params = new URLSearchParams({
      embedded: "agent",
      path: String(session?.path || ""),
      session: String(session?.id || ""),
    });
    return `/terminal?${params.toString()}`;
  }

  function profileLaunchFrameUrl(profile) {
    const params = new URLSearchParams({ embedded: "agent", agent_profile: profile.id });
    return `/terminal?${params.toString()}`;
  }

  function populateProfileSessionSwitcher(select, profile, selectedSessionId = "") {
    select.replaceChildren();
    const sessions = sessionsForProfile(profile);
    const placeholder = document.createElement("option");
    placeholder.value = "";
    placeholder.textContent = sessions.length ? "智能体会话" : "暂无会话";
    select.append(placeholder);
    for (const agentSession of sessions) {
      const option = document.createElement("option");
      option.value = agentSession.id;
      option.textContent = agentSession.name || agentSession.id;
      option.selected = agentSession.id === selectedSessionId;
      select.append(option);
    }
    select.disabled = sessions.length === 0;
    if (!sessions.some((agentSession) => agentSession.id === selectedSessionId)) {
      select.value = "";
    }
  }

  function showProfileInAgent(profile, { session = null, launch = false } = {}) {
    const area = byId("agent-chat-area");
    if (!area) return;
    const shell = document.createElement("section");
    shell.className = "agent-terminal-shell";
    shell.dataset.profileId = profile.id;

    const toolbar = document.createElement("div");
    toolbar.className = "agent-terminal-toolbar";
    const menu = document.createElement("button");
    menu.type = "button";
    menu.className = "agent-sidebar-toggle";
    menu.dataset.agentSidebarToggle = "";
    menu.setAttribute("aria-label", "打开 Agent 菜单");
    menu.textContent = "☰";
    const title = document.createElement("strong");
    title.className = "agent-terminal-title";
    title.textContent = launch
      ? `${profile.name} · 正在启动`
      : `${profile.name} · ${session?.name || "最近会话"}`;
    const relaunch = document.createElement("button");
    relaunch.type = "button";
    relaunch.className = "agent-terminal-action";
    relaunch.textContent = "新会话";
    relaunch.title = "使用此智能体新建终端会话";
    const sessionSwitcher = document.createElement("select");
    sessionSwitcher.className = "agent-terminal-action agent-terminal-session-switcher";
    sessionSwitcher.setAttribute("aria-label", "切换此智能体的会话");
    sessionSwitcher.title = "切换此智能体的已有会话";
    populateProfileSessionSwitcher(sessionSwitcher, profile, session?.id || "");

    const frame = document.createElement("iframe");
    frame.className = "agent-terminal-frame";
    frame.title = `${profile.name}终端会话`;
    const frameUrl = launch ? profileLaunchFrameUrl(profile) : agentSessionFrameUrl(session);
    frame.src = frameUrl;
    const stage = document.createElement("div");
    stage.className = "agent-terminal-stage";
    const pending = document.createElement("div");
    pending.className = "agent-terminal-pending";
    pending.setAttribute("role", "status");
    pending.innerHTML = '<strong>正在创建智能体会话</strong><span>正在应用预设并启动终端…</span>';
    sessionSwitcher.addEventListener("change", () => {
      const selected = sessionsForProfile(profile).find(
        (agentSession) => agentSession.id === sessionSwitcher.value,
      );
      if (selected) showProfileInAgent(profile, { session: selected });
    });
    relaunch.addEventListener("click", () => {
      launchProfileInAgent(profile).catch((error) => setStatus(error.message, "error"));
    });
    toolbar.append(menu, title, sessionSwitcher, relaunch);
    stage.append(frame, pending);
    shell.append(toolbar, stage);
    area.replaceChildren(shell);
    state.activeTerminalFrame = frame;
    if (!launch) shell.classList.add("ready");
    document.getElementById("agent-sidebar")?.classList.remove("open");
    document.getElementById("agent-sidebar-backdrop")?.classList.remove("open");
  }

  async function openProfileInAgent(profile) {
    if (profileAgentType(profile) === "native") {
      const session = nativeSessionsForProfile(profile)[0] || null;
      if (!session) return launchProfileInAgent(profile);
      setStatus("");
      await nativeAgentController().openSession(session.id);
      return true;
    }
    const session = sessionsForProfile(profile)[0] || null;
    if (!session) {
      return launchProfileInAgent(profile);
    }
    setStatus("");
    showProfileInAgent(profile, { session });
    return true;
  }

  async function launchProfileInAgent(profile) {
    setStatus("");
    if (profileAgentType(profile) === "native") {
      const session = await nativeAgentController().createSession(profile.id);
      state.nativeSessions = [
        session,
        ...state.nativeSessions.filter((item) => item.id !== session.id),
      ];
      return true;
    }
    showProfileInAgent(profile, { launch: true });
    return true;
  }

  async function restoreLastAgentSession() {
    const latest = state.profiles
      .map((profile) => ({
        profile,
        session: profileAgentType(profile) === "native"
          ? nativeSessionsForProfile(profile)[0] || null
          : sessionsForProfile(profile)[0] || null,
      }))
      .filter((entry) => entry.session)
      .sort((left, right) => sessionRecency(right.session) - sessionRecency(left.session))[0];
    if (!latest) return false;
    return await openProfileInAgent(latest.profile);
  }

  async function refreshTerminalSessions() {
    const data = await requestJson("/api/terminal/sessions?all=true");
    state.terminalSessions = Array.isArray(data.sessions) ? data.sessions : [];
  }

  function handleTerminalLaunchMessage(event) {
    if (event.origin !== window.location.origin || event.source !== state.activeTerminalFrame?.contentWindow) {
      return;
    }
    const message = event.data;
    if (!message || message.type !== "webclx-agent-terminal-launch") return;
    const shell = state.activeTerminalFrame.closest(".agent-terminal-shell");
    if (!shell || message.profileId !== shell.dataset.profileId) return;
    const title = shell.querySelector(".agent-terminal-title");
    const pending = shell.querySelector(".agent-terminal-pending");
    if (message.status === "ready") {
      const details = [message.profileName, message.presetName, message.model].filter(Boolean);
      title.textContent = details.join(" · ");
      shell.classList.remove("failed");
      shell.classList.add("ready");
      refreshTerminalSessions()
        .then(() => populateProfileSessionSwitcher(
          shell.querySelector(".agent-terminal-session-switcher"),
          state.profiles.find((profile) => profile.id === message.profileId),
          message.sessionId,
        ))
        .catch(() => {});
      return;
    }
    if (message.status === "error") {
      shell.classList.remove("ready");
      shell.classList.add("failed");
      title.textContent = `${message.profileName || "智能体"} · 启动失败`;
      pending.replaceChildren();
      const heading = document.createElement("strong");
      heading.textContent = "智能体启动失败";
      const detail = document.createElement("span");
      detail.textContent = message.message || "未能创建终端会话。";
      pending.append(heading, detail);
    }
  }

  function renderProfiles() {
    const list = byId("agent-terminal-profiles");
    if (!list) return;
    list.replaceChildren();
    if (!state.profiles.length) {
      const empty = document.createElement("div");
      empty.className = "agent-profile-status";
      empty.textContent = "暂无终端智能体";
      list.append(empty);
      return;
    }
    for (const profile of state.profiles) {
      const item = document.createElement("div");
      item.className = "agent-profile-item";
      const titleRow = document.createElement("div");
      titleRow.className = "agent-profile-title-row";
      const name = document.createElement("div");
      name.className = "agent-profile-name";
      name.textContent = profile.name;
      const type = document.createElement("span");
      type.className = "agent-profile-type";
      type.textContent = profileAgentTypeLabel(profile);
      const description = document.createElement("div");
      description.className = "agent-profile-description";
      description.textContent = profile.description || "暂无说明";
      description.title = description.textContent;
      const controls = document.createElement("div");
      controls.className = "agent-profile-controls";
      controls.append(
        makeButton("打开", "open", profile.id, "primary"),
        makeButton("新建", "launch", profile.id),
        makeButton("编辑", "edit", profile.id),
        makeButton("删除", "delete", profile.id),
      );
      titleRow.append(name, type);
      item.append(titleRow, description, controls);
      list.append(item);
    }
  }

  function populateSelect(select, items, selected, valueKey, label) {
    select.replaceChildren();
    for (const item of items) {
      const option = document.createElement("option");
      option.value = item[valueKey];
      option.textContent = label(item);
      option.selected = option.value === selected;
      select.append(option);
    }
    if (selected && !items.some((item) => item[valueKey] === selected)) {
      const option = document.createElement("option");
      option.value = selected;
      option.textContent = `${selected}（当前不可用）`;
      option.selected = true;
      select.prepend(option);
    }
  }

  function profilePresetId(profile) {
    const selector = String(profile?.preset_selector || "").trim();
    if (!selector) return "";
    if (profile?.preset_match === "id") return selector;
    const expected = selector.toLocaleLowerCase("en-US");
    const presets = profilePresets(profile);
    const exact = presets.filter(
      (preset) => String(preset?.name || "").trim().toLocaleLowerCase("en-US") === expected,
    );
    if (exact.length === 1) return exact[0].id;
    if (profile?.preset_match !== "unique_contains") return "";
    const compatible = presets.filter((preset) =>
      String(preset?.name || "").trim().toLocaleLowerCase("en-US").includes(expected),
    );
    return compatible.length === 1 ? compatible[0].id : "";
  }

  function openEditor(profile = null) {
    byId("agent-terminal-profile-title").textContent = profile ? "编辑智能体" : "新建智能体";
    byId("agent-terminal-profile-id").value = profile?.id || "";
    byId("agent-terminal-profile-name").value = profile?.name || "";
    byId("agent-terminal-profile-agent-type").value = profileAgentType(profile || { agent_type: "native" });
    byId("agent-terminal-profile-description").value = profile?.description || "";
    byId("agent-terminal-profile-cwd").value = profile?.cwd || "/home/system";
    byId("agent-terminal-profile-project").value = profile?.project_path || profile?.cwd || "/home/system";
    byId("agent-terminal-profile-terminal-name").value = profile?.terminal_name || profile?.name || "";
    byId("agent-terminal-profile-task").value = profile?.initial_task || "";
    populateSelect(
      byId("agent-terminal-profile-preset"),
      profilePresets(profile || { agent_type: "native" }),
      profilePresetId(profile),
      "id",
      (preset) => preset.name || preset.id,
    );
    populateSelect(
      byId("agent-terminal-profile-skill"),
      state.skills,
      profile?.skill_name || "",
      "name",
      (skill) => skill.name,
    );
    setStatus("", "muted", true);
    byId("agent-terminal-profile-dialog").showModal();
    byId("agent-terminal-profile-name").focus();
  }

  function closeEditor() {
    byId("agent-terminal-profile-dialog")?.close();
  }

  function handleAgentTypeChange() {
    const profile = { agent_type: byId("agent-terminal-profile-agent-type").value };
    populateSelect(
      byId("agent-terminal-profile-preset"),
      profilePresets(profile),
      "",
      "id",
      (preset) => preset.name || preset.id,
    );
  }

  async function saveProfile(event) {
    event.preventDefault();
    const id = byId("agent-terminal-profile-id").value;
    const presetId = byId("agent-terminal-profile-preset").value;
    const payload = {
      id,
      name: byId("agent-terminal-profile-name").value,
      description: byId("agent-terminal-profile-description").value,
      agent_type: byId("agent-terminal-profile-agent-type").value,
      preset_selector: presetId,
      preset_match: "id",
      cwd: byId("agent-terminal-profile-cwd").value,
      project_path: byId("agent-terminal-profile-project").value,
      skill_name: byId("agent-terminal-profile-skill").value,
      initial_task: byId("agent-terminal-profile-task").value,
      terminal_name: byId("agent-terminal-profile-terminal-name").value,
    };
    try {
      await requestJson(
        id ? `/api/agent/terminal-profiles/${encodeURIComponent(id)}` : "/api/agent/terminal-profiles",
        { method: id ? "PUT" : "POST", body: JSON.stringify(payload) },
      );
      closeEditor();
      await loadProfiles();
      setStatus("智能体已保存。", "ok");
    } catch (error) {
      setStatus(error.message, "error", true);
    }
  }

  async function handleProfileAction(event) {
    const button = event.target.closest("button[data-profile-id]");
    if (!button) return;
    const profile = state.profiles.find((item) => item.id === button.dataset.profileId);
    if (!profile) return;
    if (button.dataset.action === "open") {
      try {
        await openProfileInAgent(profile);
      } catch (error) {
        setStatus(error.message, "error");
      }
      return;
    }
    if (button.dataset.action === "launch") {
      try {
        await launchProfileInAgent(profile);
      } catch (error) {
        setStatus(error.message, "error");
      }
      return;
    }
    if (button.dataset.action === "edit") {
      openEditor(profile);
      return;
    }
    if (button.dataset.action === "delete" && window.confirm(`确定删除智能体“${profile.name}”吗？`)) {
      try {
        await requestJson(`/api/agent/terminal-profiles/${encodeURIComponent(profile.id)}`, { method: "DELETE" });
        await loadProfiles();
      } catch (error) {
        setStatus(error.message, "error");
      }
    }
  }

  async function loadProfiles() {
    const data = await requestJson("/api/agent/terminal-profiles");
    state.profiles = Array.isArray(data.profiles) ? data.profiles : [];
    renderProfiles();
  }

  async function initialize() {
    if (!byId("agent-terminal-profiles")) return;
    try {
      const [profiles, codexPresets, claudePresets, skills, terminalSessions, nativeSessions] = await Promise.all([
        requestJson("/api/agent/terminal-profiles"),
        requestJson("/api/agent/api-presets"),
        requestJson("/api/auth/claude-presets"),
        requestJson("/api/agent/skills"),
        requestJson("/api/terminal/sessions?all=true"),
        requestJson("/api/agent/sessions"),
      ]);
      state.profiles = profiles.profiles || [];
      state.codexPresets = codexPresets.presets || [];
      state.claudePresets = claudePresets.presets || [];
      state.skills = (skills.skills || []).filter((skill) => !skill.disabled);
      state.terminalSessions = Array.isArray(terminalSessions.sessions) ? terminalSessions.sessions : [];
      state.nativeSessions = Array.isArray(nativeSessions.sessions) ? nativeSessions.sessions : [];
      renderProfiles();
      await restoreLastAgentSession();
    } catch (error) {
      setStatus(`智能体加载失败：${error.message}`, "error");
    }
    byId("agent-terminal-profile-add")?.addEventListener("click", () => openEditor());
    byId("agent-terminal-profiles")?.addEventListener("click", handleProfileAction);
    byId("agent-terminal-profile-form")?.addEventListener("submit", saveProfile);
    byId("agent-terminal-profile-agent-type")?.addEventListener("change", handleAgentTypeChange);
    byId("agent-terminal-profile-close")?.addEventListener("click", closeEditor);
    byId("agent-terminal-profile-cancel")?.addEventListener("click", closeEditor);
    window.addEventListener("message", handleTerminalLaunchMessage);
  }

  document.addEventListener("DOMContentLoaded", initialize);
})();
