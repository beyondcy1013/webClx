(function attachNewTerminal(root) {
  function normalizeBehavior(value) {
    return String(value || "").trim().toLowerCase() === "direct" ? "direct" : "dialog";
  }

  async function openWithBehavior({ behavior, openDialog, createDirect } = {}) {
    const action = normalizeBehavior(behavior) === "direct" ? createDirect : openDialog;
    if (typeof action !== "function") {
      throw new Error("新开终端动作不可用。");
    }
    return action();
  }

  async function createSession(requestJson, options = {}) {
    if (typeof requestJson !== "function") {
      throw new Error("创建终端请求函数不可用。");
    }

    const payload = {
      path: String(options.path || ""),
      origin: String(options.origin || "normal"),
      owner_key: String(options.ownerKey || ""),
      codex_api_preset_id: String(options.codexApiPresetId || ""),
    };
    return requestJson("/api/terminal/sessions", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(payload),
    });
  }

  root.WebClxNewTerminal = Object.freeze({ createSession, normalizeBehavior, openWithBehavior });
})(typeof globalThis !== "undefined" ? globalThis : this);
