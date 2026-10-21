(function () {
  "use strict";

  function create({ checkbox, saveButton, refreshButton, status, request }) {
    const endpoint = "/api/auth/model-catalog-policy";
    let busy = false;
    let loaded = false;

    function renderBusy() {
      checkbox.disabled = busy || !loaded;
      saveButton.disabled = busy || !loaded;
      refreshButton.disabled = busy;
    }

    function show(message, tone) {
      status.textContent = message;
      status.dataset.tone = tone;
    }

    async function run(save) {
      if (busy || (save && !loaded)) return;
      busy = true;
      renderBusy();
      show(save ? "正在保存压缩策略…" : "正在读取压缩策略…", "info");
      try {
        const response = await request(endpoint, save ? {
          method: "PUT",
          body: JSON.stringify({ suppress_comp_hash_changes: checkbox.checked }),
        } : undefined);
        if (typeof response?.suppress_comp_hash_changes !== "boolean") {
          throw new Error("服务未返回有效的压缩策略，请确认后端已更新。");
        }
        checkbox.checked = response.suppress_comp_hash_changes;
        loaded = true;
        show(save
          ? "策略已保存；下次应用任意 Codex_API 预设写入目录，再重新启动 Codex 生效。当前终端未改动。"
          : (checkbox.checked ? "已启用统一空 hash 策略。" : "未启用：保留模型原有 hash。"), "success");
      } catch (error) {
        show(`${save ? "保存" : "读取"}失败：${error.message}`, "error");
      } finally {
        busy = false;
        renderBusy();
      }
    }

    saveButton.addEventListener("click", () => run(true));
    refreshButton.addEventListener("click", () => run(false));
    return { load: () => run(false), save: () => run(true) };
  }

  globalThis.WebClxModelCatalogPolicy = Object.freeze({ create });
  const checkbox = document.getElementById("codex-suppress-comp-hash");
  if (checkbox) {
    const controller = create({
      checkbox,
      saveButton: document.getElementById("codex-comp-hash-save"),
      refreshButton: document.getElementById("codex-comp-hash-refresh"),
      status: document.getElementById("codex-comp-hash-status"),
      request: (url, options) => requestJson(url, options),
    });
    controller.load();
  }
})();
