/**
 * Agent settings tab: skill management, model config, extra skill dirs.
 * Dynamically loads skills from the backend, supports enable/disable toggling,
 * adding/removing extra skill directories, and saving default model + system prompt.
 */
(function () {
  "use strict";

  const $ = (s) => document.querySelector(s);
  const $$ = (s) => document.querySelectorAll(s);
  let allSkills = [];
  let skillDirs = [];
  let terminalAgentProfiles = [];

  function escapeHtml(s) {
    if (!s) return "";
    return String(s)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;")
      .replace(/"/g, "&quot;");
  }

  async function fetchJson(url, options) {
    const resp = await fetch(url, options);
    if (!resp.ok) {
      const text = await resp.text();
      throw new Error(text || `HTTP ${resp.status}`);
    }
    return resp.json();
  }

  let selectedPresetId = "";
  let presetList = [];
  // Backend-resolved effective info for the "current application" preset.
  let currentAppliedEffective = null;

  async function loadConfig() {
    try {
      const config = await fetchJson("/api/agent/config");
      $("#agent-default-model-input").value = config.default_model || "";
      $("#agent-system-prompt-input").value = config.system_prompt_override || "";
      selectedPresetId = config.api_preset_id || "";
      skillDirs = config.extra_skill_dirs || [];
      terminalAgentProfiles = config.terminal_agent_profiles || [];
      renderSkillDirs();
    } catch (e) {
      console.warn("agent config load failed:", e);
    }
  }

  async function loadApiPresets() {
    const select = $("#agent-api-preset-select");
    if (!select) return;
    try {
      const data = await fetchJson("/api/agent/api-presets");
      presetList = data.presets || [];
      const current = data.current_preset_id || selectedPresetId;
      // Preserve the leading "当前应用" option
      select.innerHTML = '<option value="">当前应用</option>';
      for (const p of presetList) {
        const opt = document.createElement("option");
        opt.value = p.id;
        const label = p.name || p.id;
        const modelTag = p.model ? ` [${p.model}]` : "";
        opt.textContent = p.provider_name
          ? `${label}${modelTag} (${p.provider_name})`
          : `${label}${modelTag}`;
        select.appendChild(opt);
      }
      select.value = current || "";
      renderEffectiveInfo(data.effective);
    } catch (e) {
      console.warn("agent api-presets load failed:", e);
    }
  }

  function renderEffectiveInfo(effective) {
    // effective is the backend-resolved info for the "current application".
    if (effective) currentAppliedEffective = effective;
    updateEffectiveDisplay();
  }

  // Recompute the effective display from the current select value, the default
  // model input, and the backend-resolved current-application info.
  function updateEffectiveDisplay() {
    const box = $("#agent-effective-preset-info");
    const nameEl = $("#agent-effective-preset-name");
    const providerEl = $("#agent-effective-provider");
    const modelEl = $("#agent-effective-model");
    if (!box) return;
    const select = $("#agent-api-preset-select");
    const modelInput = $("#agent-default-model-input");
    const selectedId = select ? select.value : "";
    const manualModel = modelInput ? modelInput.value.trim() : "";

    let presetName;
    let provider;
    let model;

    if (selectedId) {
      // A specific preset is chosen: preview its name/model.
      const preset = presetList.find((p) => p.id === selectedId);
      presetName = preset ? preset.name : "预设已删除";
      provider = preset ? preset.provider_name : "";
      model = manualModel || (preset ? preset.model : "") || "gpt-4o";
    } else {
      // "Current application": use backend-resolved info.
      presetName = currentAppliedEffective
        ? currentAppliedEffective.preset_name
        : null;
      provider = currentAppliedEffective
        ? currentAppliedEffective.provider_name
        : null;
      model = manualModel || (currentAppliedEffective ? currentAppliedEffective.model : "") || "gpt-4o";
    }

    if (!presetName && !model) {
      box.hidden = true;
      return;
    }
    box.hidden = false;
    nameEl.textContent = presetName || "未匹配预设";
    providerEl.textContent = provider ? `(${provider})` : "";
    modelEl.textContent = model || "gpt-4o";
  }

  async function loadSkills() {
    const container = $("#agent-skills-list");
    container.innerHTML =
      '<div class="inline-status" data-tone="muted">加载中...</div>';
    try {
      const data = await fetchJson("/api/agent/skills");
      allSkills = data.skills || [];
      renderSkills();
    } catch (e) {
      container.innerHTML = `<div class="inline-status" data-tone="error">加载失败: ${escapeHtml(e.message)}</div>`;
    }
  }

  function renderSkills() {
    const container = $("#agent-skills-list");
    const filter = ($("#agent-skill-search-input")?.value || "").toLowerCase().trim();
    let skills = allSkills;
    if (filter) {
      skills = skills.filter(
        (s) =>
          (s.name || "").toLowerCase().includes(filter) ||
          (s.description || "").toLowerCase().includes(filter),
      );
    }

    if (skills.length === 0) {
      container.innerHTML =
        '<div class="inline-status" data-tone="muted">暂无已识别 skill。</div>';
      return;
    }

    const enabledCount = skills.filter((s) => !s.disabled).length;
    const summary = `<div class="agent-skills-summary">共 ${skills.length} 个 skill，${enabledCount} 个启用，${skills.length - enabledCount} 个屏蔽</div>`;
    const rows = skills
      .map((s) => {
        const toggleLabel = s.disabled ? "启用" : "屏蔽";
        const toggleClass = s.disabled
          ? "agent-skill-toggle disabled"
          : "agent-skill-toggle";
        const sourceLabel =
          s.source === "extra"
            ? '<span class="agent-skill-source extra">项目</span>'
            : s.source === "disabled"
              ? '<span class="agent-skill-source missing">缺失</span>'
              : '<span class="agent-skill-source user">用户</span>';
        return `<tr class="${s.disabled ? "is-disabled" : ""}">
          <td class="col-name">${escapeHtml(s.name)}</td>
          <td class="col-source">${sourceLabel}</td>
          <td class="col-desc" title="${escapeHtml(s.description || "")}">${escapeHtml(s.description || "")}</td>
          <td class="col-path" title="${escapeHtml(s.path || "")}">${escapeHtml(s.path || "")}</td>
          <td class="col-action">
            <button class="${toggleClass}" data-skill="${escapeHtml(s.name)}" data-disable="${!s.disabled}">
              ${toggleLabel}
            </button>
          </td>
        </tr>`;
      })
      .join("");

    container.innerHTML =
      summary +
      `<div class="agent-skills-table-wrap">
        <table class="agent-skills-table">
          <thead>
            <tr>
              <th>Skill</th>
              <th>来源</th>
              <th>描述</th>
              <th>路径</th>
              <th>操作</th>
            </tr>
          </thead>
          <tbody>${rows}</tbody>
        </table>
      </div>`;
  }

  function renderSkillDirs() {
    const container = $("#agent-skill-dirs-list");
    if (skillDirs.length === 0) {
      container.innerHTML =
        '<div class="inline-status" data-tone="muted">未配置额外 skill 目录。</div>';
      return;
    }
    container.innerHTML = skillDirs
      .map(
        (d) =>
          `<div class="settings-value-list-item"><span class="mono-text">${escapeHtml(d)}</span><button class="button secondary danger small-btn" data-remove-dir="${escapeHtml(d)}">&times;</button></div>`,
      )
      .join("");
  }

  async function toggleSkill(name, disable) {
    try {
      await fetchJson("/api/agent/skills/toggle", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ skill_name: name, disable }),
      });
      // Update local state
      const skill = allSkills.find((s) => s.name === name);
      if (skill) skill.disabled = disable;
      renderSkills();
    } catch (e) {
      alert("切换失败: " + e.message);
    }
  }

  async function addSkillDir(dir) {
    dir = dir.trim();
    if (!dir) return;
    try {
      const data = await fetchJson("/api/agent/skill-dirs", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ dir }),
      });
      skillDirs = data.extra_skill_dirs || [];
      renderSkillDirs();
      $("#agent-skill-dir-input").value = "";
      await loadSkills();
    } catch (e) {
      alert("添加失败: " + e.message);
    }
  }

  async function removeSkillDir(dir) {
    try {
      const data = await fetchJson("/api/agent/skill-dirs", {
        method: "DELETE",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ dir }),
      });
      skillDirs = data.extra_skill_dirs || [];
      renderSkillDirs();
      await loadSkills();
    } catch (e) {
      alert("删除失败: " + e.message);
    }
  }

  async function saveConfig() {
    const config = {
      default_model: $("#agent-default-model-input").value.trim(),
      api_preset_id: $("#agent-api-preset-select")?.value || "",
      system_prompt_override:
        $("#agent-system-prompt-input").value.trim() || null,
      disabled_skills: allSkills.filter((s) => s.disabled).map((s) => s.name),
      extra_skill_dirs: skillDirs,
      terminal_agent_profiles: terminalAgentProfiles,
    };
    try {
      await fetchJson("/api/agent/config", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(config),
      });
      return true;
    } catch (e) {
      alert("保存 Agent 配置失败: " + e.message);
      return false;
    }
  }

  function init() {
    // Defer until DOM ready
    if (!$("#agent-skills-list")) return;

    // Bind events
    $("#agent-refresh-skills-btn")?.addEventListener("click", loadSkills);

    $("#agent-api-preset-select")?.addEventListener("change", (e) => {
      const preset = presetList.find((p) => p.id === e.target.value);
      const modelInput = $("#agent-default-model-input");
      // Only auto-fill model when the user has not typed one, to avoid
      // clobbering an intentional override.
      if (preset && preset.model && modelInput && !modelInput.value.trim()) {
        modelInput.value = preset.model;
      }
      updateEffectiveDisplay();
    });
    $("#agent-default-model-input")?.addEventListener("input", updateEffectiveDisplay);
    $("#agent-skill-search-input")?.addEventListener("input", renderSkills);

    $("#agent-add-skill-dir-btn")?.addEventListener("click", () => {
      addSkillDir($("#agent-skill-dir-input").value);
    });
    $("#agent-skill-dir-input")?.addEventListener("keydown", (e) => {
      if (e.key === "Enter") addSkillDir(e.target.value);
    });

    // Delegate skill toggle + dir removal
    $("#agent-skills-list")?.addEventListener("click", (e) => {
      const btn = e.target.closest(".agent-skill-toggle");
      if (btn) {
        const name = btn.dataset.skill;
        const disable = btn.dataset.disable === "true";
        toggleSkill(name, disable);
      }
    });

    $("#agent-skill-dirs-list")?.addEventListener("click", (e) => {
      const btn = e.target.closest("[data-remove-dir]");
      if (btn) removeSkillDir(btn.dataset.removeDir);
    });

    // Hook into the main save-settings flow
    const saveBtn = $("#save-settings");
    if (saveBtn) {
      saveBtn.addEventListener("click", () => {
        saveConfig();
      }, true);
    }

    // Load data on init
    loadConfig();
    loadApiPresets();
    loadSkills();
  }

  // Wait for DOM
  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", init);
  } else {
    init();
  }
})();
