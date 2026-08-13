// Error keyword rules table: renders keyword + action rows, syncs with state.
// Loaded after app.js (globals/state) and app-settings-formatters.js.

function renderTerminalErrorKeywordRulesTable() {
  if (!terminalErrorKeywordRulesBodyEl) {
    return;
  }
  const actions = normalizeTerminalErrorKeywordActions(state.terminalErrorKeywordActions);
  const keywords = normalizeTerminalErrorKeywords(state.terminalErrorKeywords);
  // Build a merged view: every keyword is a row; keywords with explicit actions
  // show their action, others default to "continue".
  const actionByKey = new Map(
    actions.map((entry) => [entry.keyword.toLowerCase(), entry.action]),
  );
  const seen = new Set();
  const rows = [];
  actions.forEach((entry) => {
    const key = entry.keyword.toLowerCase();
    if (seen.has(key)) {
      return;
    }
    seen.add(key);
    rows.push({ keyword: entry.keyword, action: entry.action });
  });
  keywords.forEach((keyword) => {
    const key = keyword.toLowerCase();
    if (seen.has(key)) {
      return;
    }
    seen.add(key);
    rows.push({ keyword, action: TERMINAL_ERROR_KEYWORD_ACTION_CONTINUE });
  });

  terminalErrorKeywordRulesBodyEl.innerHTML = rows
    .map((row, index) => buildTerminalErrorKeywordRuleRow(row, index))
    .join("");

  bindTerminalErrorKeywordRuleRowEvents();
}

function buildTerminalErrorKeywordRuleRow(row, index) {
  const keyword = escapeHtml(row.keyword);
  const options = Object.entries(TERMINAL_ERROR_KEYWORD_ACTION_LABELS)
    .map(
      ([value, label]) =>
        `<option value="${value}"${value === row.action ? " selected" : ""}>${escapeHtml(label)}</option>`,
    )
    .join("");
  return `
    <tr class="terminal-error-keyword-rule-row" data-rule-index="${index}">
      <td>
        <input
          type="text"
          class="text-input mono-text terminal-error-keyword-input"
          value="${keyword}"
          spellcheck="false"
          autocomplete="off"
        />
      </td>
      <td>
        <select class="terminal-error-keyword-action-select">${options}</select>
      </td>
      <td>
        <button type="button" class="button secondary terminal-error-keyword-remove-btn">删除</button>
      </td>
    </tr>
  `;
}

function bindTerminalErrorKeywordRuleRowEvents() {
  if (!terminalErrorKeywordRulesBodyEl) {
    return;
  }
  terminalErrorKeywordRulesBodyEl
    .querySelectorAll(".terminal-error-keyword-rule-row")
    .forEach((rowEl) => {
      const inputEl = rowEl.querySelector(".terminal-error-keyword-input");
      const selectEl = rowEl.querySelector(".terminal-error-keyword-action-select");
      const removeBtn = rowEl.querySelector(".terminal-error-keyword-remove-btn");
      if (inputEl) {
        inputEl.addEventListener("input", syncTerminalErrorKeywordRulesFromTable);
      }
      if (selectEl) {
        selectEl.addEventListener("change", syncTerminalErrorKeywordRulesFromTable);
      }
      if (removeBtn) {
        removeBtn.addEventListener("click", () => {
          rowEl.remove();
          syncTerminalErrorKeywordRulesFromTable();
        });
      }
    });
}

function syncTerminalErrorKeywordRulesFromTable() {
  if (!terminalErrorKeywordRulesBodyEl) {
    return;
  }
  const rows = Array.from(
    terminalErrorKeywordRulesBodyEl.querySelectorAll(".terminal-error-keyword-rule-row"),
  );
  const actions = rows
    .map((rowEl) => {
      const keyword = rowEl.querySelector(".terminal-error-keyword-input")?.value || "";
      const action = rowEl.querySelector(".terminal-error-keyword-action-select")?.value || "";
      return { keyword, action };
    })
    .filter((entry) => entry.keyword.trim());
  state.terminalErrorKeywordActions = normalizeTerminalErrorKeywordActions(actions);
}

function collectTerminalErrorKeywordsFromTable() {
  syncTerminalErrorKeywordRulesFromTable();
  return state.terminalErrorKeywordActions.map((entry) => entry.keyword);
}

function addTerminalErrorKeywordRule() {
  state.terminalErrorKeywordActions = normalizeTerminalErrorKeywordActions([
    ...state.terminalErrorKeywordActions,
    { keyword: "", action: TERMINAL_ERROR_KEYWORD_ACTION_CONTINUE },
  ]);
  renderTerminalErrorKeywordRulesTable();
  const inputs = terminalErrorKeywordRulesBodyEl?.querySelectorAll(
    ".terminal-error-keyword-input",
  );
  if (inputs && inputs.length > 0) {
    inputs[inputs.length - 1]?.focus();
  }
}
