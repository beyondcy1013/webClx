(function attachTerminalImePolicy(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
    return;
  }

  root.WebClxTerminalImePolicy = factory();
})(typeof globalThis !== "undefined" ? globalThis : this, function createTerminalImePolicy() {
  const TERMINAL_SYSTEM_IME_SUPPRESSION_MS = 60 * 1000;

  function terminalImeToggleAction(state) {
    return state?.systemImeEnabled && state?.helperFocused ? "disable" : "focus";
  }

  function terminalImeFocusAllowed(state) {
    const now = Number.isFinite(Number(state?.now)) ? Number(state.now) : Date.now();
    const suppressedUntil = Number.isFinite(Number(state?.suppressedUntil))
      ? Number(state.suppressedUntil)
      : 0;
    return now >= suppressedUntil;
  }

  function terminalImeDirectFocusAction(state) {
    return terminalImeFocusAllowed(state) ? "focus" : "blocked";
  }

  function terminalImeFunctionAction(command, now = Date.now()) {
    const action = String(command?.action || "").trim();
    if (action === "disable_system_keyboard") {
      return {
        kind: "disable",
        suppressedUntil: now + TERMINAL_SYSTEM_IME_SUPPRESSION_MS,
      };
    }
    if (action === "show_system_keyboard") {
      return {
        kind: "show",
        suppressedUntil: 0,
      };
    }
    return {
      kind: "none",
      suppressedUntil: null,
    };
  }

  return {
    TERMINAL_SYSTEM_IME_SUPPRESSION_MS,
    terminalImeDirectFocusAction,
    terminalImeFocusAllowed,
    terminalImeFunctionAction,
    terminalImeToggleAction,
  };
});
