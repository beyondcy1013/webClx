(() => {
  "use strict";
  const endpoint = "/api/auth/codearts-options";
  let generation = 0;
  let active = false;
  let confirmed = true;
  const input = () => document.getElementById("codearts-option-full-access");
  function note(text, tone = "muted") {
    const node = document.getElementById("codearts-options-note");
    if (node) { node.textContent = text; node.dataset.tone = tone; }
  }
  function render(result) {
    confirmed = Boolean(result.allowAllPermissions);
    input().checked = confirmed;
    note(result.applied
      ? `${confirmed ? "最高权限已保存" : "已恢复原权限配置"}。重新启动原生 CodeArts 会话可完整生效。`
      : "原生配置尚未完整同步，请重新保存开关。", result.applied ? "ok" : "warn");
  }
  async function save(enabled, current) {
    const result = await requestJson(endpoint, {
      method: "PUT", body: JSON.stringify({ allow_all_permissions: enabled }),
    });
    if (current === generation && active) render(result);
  }
  async function select(kind) {
    active = kind === "codearts";
    const current = ++generation;
    const bar = document.getElementById("codearts-options-bar");
    if (!bar || !input()) return;
    bar.hidden = !active;
    if (!active) return;
    input().disabled = true;
    note("正在读取 CodeArts 原生权限配置…");
    try {
      const result = await requestJson(endpoint);
      if (current !== generation || !active) return;
      if (!result.initialized || (result.allowAllPermissions && !result.applied)) {
        note("正在启用默认最高权限…");
        await save(true, current);
      } else render(result);
    } catch (error) {
      if (current === generation && active) note(`读取/同步权限失败：${error.message}`, "warn");
    } finally {
      if (current === generation && active) input().disabled = false;
    }
  }
  window.codeartsOptions = { select };
  window.addEventListener("DOMContentLoaded", () => {
    input()?.addEventListener("change", async () => {
      if (!active) return;
      const current = generation;
      const enabled = input().checked;
      input().disabled = true;
      note("正在保存 CodeArts 原生权限配置…");
      try { await save(enabled, current); }
      catch (error) {
        if (current === generation && active) {
          input().checked = confirmed;
          note(`保存失败：${error.message}`, "warn");
        }
      } finally {
        if (current === generation && active) input().disabled = false;
      }
    });
  });
})();
