// LLM_API 统一账号编辑弹窗：各类型账号的新增/编辑表单仍在原地实现，
// 这里只在它们变为可见时把面板搬进共享 <dialog>，隐藏时搬回原位。
(() => {
  "use strict";

  const PANEL_DEFS = [
    {
      kind: "api",
      title: "Codex_API 预设",
      resolve: () => document.getElementById("api-preset-editor-panel"),
      editingId: () => (typeof state !== "undefined" ? state.editingApiPresetId : ""),
      onRequestClose: (panel) => {
        if (typeof apiManager !== "undefined" && apiManager) {
          return apiManager.cancelApiPresetEdit();
        }
        panel.hidden = true;
        return true;
      },
    },
    {
      kind: "claude",
      title: "Claude_API 预设",
      resolve: () => document.getElementById("claude-preset-editor-panel"),
      editingId: () => (typeof state !== "undefined" ? state.editingClaudePresetId : ""),
      onRequestClose: (panel) => {
        if (typeof claudeManager !== "undefined" && claudeManager) {
          claudeManager.cancelClaudePresetEdit();
          return true;
        }
        panel.hidden = true;
        return true;
      },
    },
    {
      kind: "oauth",
      title: "Codex_OAuth 账号",
      resolve: () =>
        document.querySelector("#api-view > .auth-layout-merged") ||
        document.getElementById("llm-account-editor-body")?.querySelector(".auth-layout-merged") ||
        document.querySelector(".auth-layout-merged"),
      editingId: () => (typeof state !== "undefined" ? state.editingAuthPresetId : ""),
      onRequestClose: (panel) => {
        panel.hidden = true;
        return true;
      },
    },
    {
      kind: "agy",
      title: "AGY 账号",
      resolve: () => document.getElementById("agy-editor-panel"),
      editingId: () => "",
      onRequestClose: (panel) => {
        panel.hidden = true;
        return true;
      },
    },
  ];

  let dialog = null;
  let body = null;
  let titleEl = null;
  let current = null;
  const entries = [];

  function titleFor(entry) {
    const editing = entry.editingId();
    return `${editing ? "编辑" : "新增"} ${entry.title}`;
  }

  function detachCurrent() {
    const entry = current;
    current = null;
    if (!entry) return;
    if (entry.panel.parentElement === body) {
      const { parent, next } = entry.home;
      parent.insertBefore(entry.panel, next && next.parentNode === parent ? next : null);
    }
  }

  function adopt(entry) {
    if (current === entry) {
      titleEl.textContent = titleFor(entry);
      if (!dialog.open) dialog.showModal();
      return;
    }
    if (current) {
      const previous = current;
      if (!previous.panel.hidden) {
        try {
          previous.onRequestClose(previous.panel);
        } catch {
          previous.panel.hidden = true;
        }
      }
      if (current === previous) detachCurrent();
    }
    current = entry;
    body.appendChild(entry.panel);
    titleEl.textContent = titleFor(entry);
    if (!dialog.open) dialog.showModal();
  }

  function release(entry) {
    if (entry !== current) return;
    detachCurrent();
    if (dialog.open) dialog.close();
  }

  function requestCloseCurrent() {
    if (!current) {
      if (dialog.open) dialog.close();
      return;
    }
    const entry = current;
    try {
      entry.onRequestClose(entry.panel);
    } catch {
      entry.panel.hidden = true;
    }
  }

  function onHiddenMutated(entry) {
    if (entry.panel.hidden) release(entry);
    else adopt(entry);
  }

  window.addEventListener("DOMContentLoaded", () => {
    dialog = document.getElementById("llm-account-editor-dialog");
    body = document.getElementById("llm-account-editor-body");
    titleEl = document.getElementById("llm-account-editor-title");
    if (!dialog || !body || !titleEl) return;

    for (const def of PANEL_DEFS) {
      const panel = def.resolve();
      if (!panel) continue;
      const entry = {
        ...def,
        panel,
        home: { parent: panel.parentElement, next: panel.nextSibling },
      };
      entries.push(entry);
      new MutationObserver(() => onHiddenMutated(entry)).observe(panel, {
        attributes: true,
        attributeFilter: ["hidden"],
      });
    }

    document
      .getElementById("llm-account-editor-close")
      ?.addEventListener("click", requestCloseCurrent);

    dialog.addEventListener("cancel", (event) => {
      event.preventDefault();
      requestCloseCurrent();
    });

    dialog.addEventListener("close", () => {
      if (current && !current.panel.hidden) {
        current.panel.hidden = true;
      }
      detachCurrent();
    });

    for (const entry of entries) {
      if (!entry.panel.hidden) {
        adopt(entry);
        break;
      }
    }
  });
})();
