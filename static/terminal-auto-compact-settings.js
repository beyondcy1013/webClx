// 自动压缩设置：默认阈值 + 按模型覆盖（/api/terminal/auto-compact-settings）。
(() => {
  const defaultInput = document.getElementById("terminal-auto-compact-threshold-input");
  const modelsInput = document.getElementById("terminal-auto-compact-model-thresholds-input");
  if (!defaultInput && !modelsInput) return;

  const clampPercent = (value) => {
    const percent = Math.round(Number(value));
    if (!Number.isFinite(percent)) return null;
    return Math.min(100, Math.max(1, percent));
  };

  const normalizeKey = (value) => String(value || "")
    .trim()
    .toLowerCase()
    .replace(/[._/ ]+/g, "-")
    .replace(/^-+|-+$/g, "");

  const parseModelThresholds = () => {
    const result = {};
    String(modelsInput?.value || "").split(/\r?\n/).forEach((line) => {
      const trimmed = line.trim();
      if (!trimmed || trimmed.startsWith("#")) return;
      const separator = trimmed.indexOf("=");
      if (separator <= 0) return;
      const key = normalizeKey(trimmed.slice(0, separator));
      const percent = clampPercent(trimmed.slice(separator + 1));
      if (key && percent !== null) result[key] = percent;
    });
    return result;
  };

  const formatModelThresholds = (value) => Object.entries(value || {})
    .sort(([left], [right]) => left.localeCompare(right))
    .map(([key, percent]) => `${key}=${percent}`)
    .join("\n");

  const load = async () => {
    try {
      const data = await requestJson("/api/terminal/auto-compact-settings");
      const percent = clampPercent(data?.context_threshold_percent);
      if (defaultInput && percent !== null) defaultInput.value = String(percent);
      if (modelsInput) modelsInput.value = formatModelThresholds(data?.model_thresholds);
    } catch (error) {
      console.warn("load auto compact settings failed", error);
    }
  };

  const save = async () => {
    const payload = { model_thresholds: parseModelThresholds() };
    if (defaultInput) {
      const percent = clampPercent(defaultInput.value);
      if (percent === null) {
        defaultInput.value = "50";
        return;
      }
      defaultInput.value = String(percent);
      payload.context_threshold_percent = percent;
    }
    try {
      const data = await requestJson("/api/terminal/auto-compact-settings", {
        method: "POST",
        body: JSON.stringify(payload),
      });
      const applied = clampPercent(data?.context_threshold_percent);
      if (defaultInput && applied !== null) defaultInput.value = String(applied);
      if (modelsInput) modelsInput.value = formatModelThresholds(data?.model_thresholds);
    } catch (error) {
      console.warn("save auto compact settings failed", error);
      if (typeof updateStatus === "function") {
        updateStatus("保存自动压缩设置失败。", "warn");
      }
    }
  };

  defaultInput?.addEventListener("change", save);
  modelsInput?.addEventListener("change", save);
  load();
})();
