// Refresh the displayed terminal preset from the provider reported by the
// running Codex process, rather than the preset active at terminal creation.

let terminalPresetExtractionRunning = false;

const extractSessionPresetButton = document.getElementById("extract-session-preset");
const extractAllSessionPresetsButton = document.getElementById("extract-all-session-presets");

function setTerminalPresetExtractionBusy(busy) {
  terminalPresetExtractionRunning = Boolean(busy);
  [extractSessionPresetButton, extractAllSessionPresetsButton].forEach((button) => {
    if (button) {
      button.disabled = terminalPresetExtractionRunning;
    }
  });
}

function applyExtractedTerminalPreset(extracted) {
  const sessionId = String(extracted?.session_id || "").trim();
  const presetName = String(extracted?.preset_name || "").trim();
  if (!sessionId || !presetName || !Array.isArray(state?.sessions)) {
    return;
  }

  const session = state.sessions.find((item) => item.id === sessionId);
  if (!session) {
    return;
  }

  session.codex_api_preset_name = presetName;
  session.codex_api_base_url = String(extracted?.base_url || "").trim();
  renderSessions();
}

async function extractTerminalPresetByCommand(sessionId) {
  const normalizedSessionId = String(sessionId || "").trim();
  if (!normalizedSessionId) {
    throw new Error("未选择终端。");
  }

  const extracted = await requestJson(
    `/api/terminal/sessions/${encodeURIComponent(normalizedSessionId)}/extract-preset`,
    { method: "POST" },
  );
  applyExtractedTerminalPreset(extracted);
  return extracted;
}

async function extractCurrentTerminalPreset() {
  if (terminalPresetExtractionRunning) {
    return;
  }

  const session = activeSession();
  if (!session?.id) {
    updateStatus("未选择可提取预设的终端。", "warn");
    return;
  }

  setTerminalPresetExtractionBusy(true);
  updateStatus(`正在向 ${session.name || session.id} 发送 /status…`, "info");
  try {
    const extracted = await extractTerminalPresetByCommand(session.id);
    updateStatus(`已提取预设：${extracted.preset_name}。`, "ok");
  } catch (error) {
    updateStatus(error.message || "命令提取预设失败。", "warn");
  } finally {
    setTerminalPresetExtractionBusy(false);
  }
}

async function extractAllTerminalPresets() {
  if (terminalPresetExtractionRunning) {
    return;
  }

  setTerminalPresetExtractionBusy(true);
  try {
    const listing = await requestJson("/api/terminal/sessions?all=true");
    const sessions = Array.isArray(listing?.sessions) ? listing.sessions : [];
    if (sessions.length === 0) {
      updateStatus("没有可更新预设的终端。", "muted");
      return;
    }

    let updated = 0;
    const failures = [];
    for (const [index, session] of sessions.entries()) {
      const label = session.name || session.id || "未命名终端";
      updateStatus(`正在提取 ${index + 1}/${sessions.length}：${label}…`, "info");
      try {
        await extractTerminalPresetByCommand(session.id);
        updated += 1;
      } catch (error) {
        failures.push(label);
      }
    }

    if (failures.length === 0) {
      updateStatus(`已更新全部 ${updated} 个终端预设。`, "ok");
    } else {
      updateStatus(`已更新 ${updated}/${sessions.length} 个终端预设；${failures.length} 个未提取。`, "warn");
    }
  } catch (error) {
    updateStatus(error.message || "更新所有终端预设失败。", "warn");
  } finally {
    setTerminalPresetExtractionBusy(false);
  }
}

if (extractSessionPresetButton) {
  extractSessionPresetButton.addEventListener("click", extractCurrentTerminalPreset);
}

if (extractAllSessionPresetsButton) {
  extractAllSessionPresetsButton.addEventListener("click", extractAllTerminalPresets);
}
