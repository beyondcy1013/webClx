// 预设表格渲染、排序、拖序、测试弹窗模块。
// 由 app.js 拆出，在 app.js 之前以 <script defer> 加载，
// 通过共享全局作用域向 app.js 提供下列函数与常量，无需修改调用方。
// 依赖的全局（state.*、requestJson 等）均为 app.js 顶层声明，加载顺序保证可用。

function isPresetRowClickIgnored(event) {
  return Boolean(
    event?.target instanceof Element && event.target.closest("button, a, input, select, textarea, label, summary"),
  );
}

function makePresetRowClickable(row, preset, activate) {
  if (!row || !preset || typeof activate !== "function") {
    return;
  }

  row.classList.add("clickable-preset-row");
  row.tabIndex = 0;
  row.setAttribute("aria-label", `点击切换到 ${preset.name}`);
  row.title = `点击切换到 ${preset.name}`;

  row.addEventListener("click", (event) => {
    if (isPresetRowClickIgnored(event)) {
      return;
    }
    activate();
  });

  row.addEventListener("keydown", (event) => {
    if (isPresetRowClickIgnored(event) || (event.key !== "Enter" && event.key !== " ")) {
      return;
    }
    event.preventDefault();
    activate();
  });
}

function createPresetSelectionCell(preset, selection = {}) {
  const cell = document.createElement("td");
  cell.className = "preset-selection-cell";
  const checkbox = document.createElement("input");
  checkbox.type = "checkbox";
  checkbox.checked = selection.selectedIds?.has(preset.id) ?? false;
  checkbox.setAttribute("aria-label", `选择 ${preset.name || preset.id}`);
  checkbox.addEventListener("change", () => {
    if (checkbox.checked) {
      selection.selectedIds?.add(preset.id);
    } else {
      selection.selectedIds?.delete(preset.id);
    }
    selection.onChange?.();
  });
  cell.appendChild(checkbox);
  return cell;
}

function renderPresetTable({
  listEl,
  presets = [],
  emptyText = "还没有保存任何预设。",
  emptyColspan = 1,
  tableKey = "",
  sortColumns = [],
  group = null,
  order = null,
  selection = null,
  buildCells,
  decorateRow,
} = {}) {
  if (!listEl || typeof buildCells !== "function") {
    return;
  }

  listEl.textContent = "";

  if (presets.length === 0) {
    const row = document.createElement("tr");
    const cell = document.createElement("td");
    cell.colSpan = emptyColspan + (selection ? 1 : 0);
    cell.className = "meta-text";
    cell.textContent = emptyText;
    row.appendChild(cell);
    listEl.appendChild(row);
    return;
  }

  const visibleRows = buildPresetTableVisibleRows(
    sortPresetTableRows(presets, tableKey, sortColumns),
    group,
  );
  visibleRows.forEach((rowContext) => {
    const { preset, visibleIndex } = rowContext;
    const row = document.createElement("tr");
    if (preset.active) {
      row.classList.add("active-auth-row");
    }
    const cells = [];
    if (selection) {
      cells.push(createPresetSelectionCell(preset, selection));
    }
    cells.push(createPresetSequenceCell(visibleIndex + 1));
    if (order?.enabled) {
      const baseIndex = presets.findIndex((item) => item?.id === preset?.id);
      const orderIndex = order.grouped && group ? rowContext.groupRowIndex : baseIndex;
      const orderTotal = order.grouped && group ? rowContext.groupRowSpan : presets.length;
      cells.push(createPresetOrderCell({
        preset,
        index: orderIndex,
        total: orderTotal,
        onMove: order.onMove,
      }));
    }
    cells.push(...buildCells(preset, rowContext));
    applyPresetTableGroupCellMerge(cells, group, rowContext);
    row.append(...cells);
    if (typeof decorateRow === "function") {
      decorateRow(row, preset);
    }
    listEl.appendChild(row);
  });
}

function normalizePresetBaseUrlGroupKey(value) {
  return String(value || "")
    .trim()
    .replace(/\/+$/, "")
    .toLowerCase();
}

function buildPresetTableVisibleRows(presets, group = null) {
  const rows = Array.isArray(presets) ? presets : [];
  if (!group || typeof group.getKey !== "function") {
    return rows.map((preset, visibleIndex) => ({
      preset,
      visibleIndex,
      groupKey: "",
      groupIndex: visibleIndex,
      groupRowIndex: 0,
      groupRowSpan: 1,
      isFirstInGroup: true,
    }));
  }

  const normalizeKey = typeof group.normalizeKey === "function"
    ? group.normalizeKey
    : (value) => String(value || "").trim();
  const groups = [];
  const groupsByKey = new Map();

  rows.forEach((preset, index) => {
    const rawKey = group.getKey(preset);
    const normalizedKey = normalizeKey(rawKey, preset);
    const groupKey = normalizedKey || `__ungrouped:${index}`;
    let groupRecord = groupsByKey.get(groupKey);
    if (!groupRecord) {
      groupRecord = { key: groupKey, items: [] };
      groupsByKey.set(groupKey, groupRecord);
      groups.push(groupRecord);
    }
    groupRecord.items.push(preset);
  });

  const visibleRows = [];
  groups.forEach((groupRecord, groupIndex) => {
    const groupRowSpan = groupRecord.items.length;
    groupRecord.items.forEach((preset, groupRowIndex) => {
      visibleRows.push({
        preset,
        visibleIndex: visibleRows.length,
        groupKey: groupRecord.key,
        groupIndex,
        groupRowIndex,
        groupRowSpan,
        isFirstInGroup: groupRowIndex === 0,
      });
    });
  });
  return visibleRows;
}

function applyPresetTableGroupCellMerge(cells, group, rowContext) {
  if (
    !Array.isArray(cells) ||
    !group ||
    rowContext.groupRowSpan <= 1
  ) {
    return;
  }

  const mergeCellIndex = presetTableMergeCellIndex(cells, group);
  if (mergeCellIndex < 0) {
    return;
  }
  const mergeCell = cells[mergeCellIndex];
  if (!mergeCell) {
    return;
  }
  if (rowContext.isFirstInGroup) {
    mergeCell.rowSpan = rowContext.groupRowSpan;
    mergeCell.classList.add("preset-grouped-cell");
    return;
  }
  cells.splice(mergeCellIndex, 1);
}

function presetTableMergeCellIndex(cells, group) {
  const mergeCellKey = String(group?.mergeCellKey || "").trim();
  if (mergeCellKey) {
    return cells.findIndex((cell) => cell?.dataset?.presetColumn === mergeCellKey);
  }
  if (Number.isInteger(group?.mergeCellIndex) && group.mergeCellIndex >= 0) {
    return group.mergeCellIndex;
  }
  return -1;
}

function createPresetSequenceCell(sequence) {
  const cell = document.createElement("td");
  cell.className = "preset-sequence-cell";
  cell.textContent = String(sequence);
  return cell;
}

function createPresetOrderCell({ preset, index, total, onMove } = {}) {
  const cell = document.createElement("td");
  cell.className = "preset-order-cell auth-action-cell";
  const actions = document.createElement("div");
  actions.className = "actions preset-actions preset-order-actions";
  const canMove = typeof onMove === "function" && index >= 0;
  const upButton = createActionButton("上移", () => onMove(preset.id, -1), "mini-button");
  const downButton = createActionButton("下移", () => onMove(preset.id, 1), "mini-button");
  upButton.disabled = !canMove || index <= 0;
  downButton.disabled = !canMove || index >= total - 1;
  actions.append(upButton, downButton);
  cell.appendChild(actions);
  return cell;
}

function movePresetById(presets, presetId, direction) {
  if (!Array.isArray(presets) || !presetId || !Number.isFinite(direction) || direction === 0) {
    return null;
  }
  const fromIndex = presets.findIndex((preset) => preset?.id === presetId);
  if (fromIndex < 0) {
    return null;
  }
  const toIndex = Math.max(0, Math.min(presets.length - 1, fromIndex + direction));
  if (toIndex === fromIndex) {
    return null;
  }
  const next = presets.slice();
  const [moved] = next.splice(fromIndex, 1);
  next.splice(toIndex, 0, moved);
  return next;
}

function movePresetToIndexById(presets, presetId, targetIndex) {
  if (!Array.isArray(presets) || !presetId || !Number.isInteger(targetIndex)) {
    return null;
  }
  const fromIndex = presets.findIndex((preset) => preset?.id === presetId);
  if (fromIndex < 0) {
    return null;
  }
  const toIndex = Math.max(0, Math.min(presets.length - 1, targetIndex));
  if (toIndex === fromIndex) {
    return null;
  }
  const next = presets.slice();
  const [moved] = next.splice(fromIndex, 1);
  next.splice(toIndex, 0, moved);
  return next;
}

async function persistPresetOrder(endpoint, presets) {
  const ids = Array.isArray(presets) ? presets.map((preset) => preset.id).filter(Boolean) : [];
  return requestJson(endpoint, {
    method: "PUT",
    body: JSON.stringify({ ids }),
  });
}

function getPresetTableSortState(tableKey) {
  if (!tableKey || !(state.presetTableSort instanceof Map)) {
    return null;
  }
  return state.presetTableSort.get(tableKey) || null;
}

function togglePresetTableSort(tableKey, sortKey, defaultDirection = "asc") {
  if (!tableKey || !sortKey || !(state.presetTableSort instanceof Map)) {
    return null;
  }

  const current = state.presetTableSort.get(tableKey);
  const nextDirection = current?.key === sortKey
    ? (current.direction === "asc" ? "desc" : "asc")
    : (defaultDirection === "desc" ? "desc" : "asc");
  const nextState = { key: sortKey, direction: nextDirection };
  state.presetTableSort.set(tableKey, nextState);
  return nextState;
}

function normalizePresetSortColumns(sortColumns) {
  return Array.isArray(sortColumns)
    ? sortColumns.filter((column) => column && typeof column.key === "string" && column.key)
    : [];
}

function findPresetSortColumn(sortColumns, sortKey) {
  return normalizePresetSortColumns(sortColumns).find((column) => column.key === sortKey) || null;
}

function normalizePresetSortValue(value, type = "text") {
  if (value === null || value === undefined) {
    return { empty: true, value: null };
  }

  if (type === "number") {
    const number = Number(value);
    return Number.isFinite(number) ? { empty: false, value: number } : { empty: true, value: null };
  }

  if (type === "date") {
    const time = value instanceof Date ? value.getTime() : new Date(value).getTime();
    return Number.isFinite(time) ? { empty: false, value: time } : { empty: true, value: null };
  }

  if (type === "boolean") {
    return { empty: false, value: value ? 1 : 0 };
  }

  const text = String(value).trim();
  return text ? { empty: false, value: text } : { empty: true, value: "" };
}

const presetSortCollator = new Intl.Collator("zh-CN", {
  numeric: true,
  sensitivity: "base",
});

function comparePresetSortValues(left, right, type = "text") {
  if (left.empty && right.empty) {
    return 0;
  }
  if (left.empty) {
    return 1;
  }
  if (right.empty) {
    return -1;
  }
  if (type === "number" || type === "date" || type === "boolean") {
    return left.value === right.value ? 0 : (left.value > right.value ? 1 : -1);
  }
  return presetSortCollator.compare(left.value, right.value);
}

function sortPresetTableRows(presets, tableKey, sortColumns) {
  const rows = Array.isArray(presets) ? presets : [];
  const sortState = getPresetTableSortState(tableKey);
  const sortColumn = findPresetSortColumn(sortColumns, sortState?.key);
  if (!sortState || !sortColumn || typeof sortColumn.getValue !== "function") {
    return rows;
  }

  const direction = sortState.direction === "desc" ? -1 : 1;
  return rows
    .map((preset, index) => ({ preset, index }))
    .sort((left, right) => {
      const leftValue = normalizePresetSortValue(sortColumn.getValue(left.preset), sortColumn.type);
      const rightValue = normalizePresetSortValue(sortColumn.getValue(right.preset), sortColumn.type);
      const valueOrder = comparePresetSortValues(leftValue, rightValue, sortColumn.type);
      if (valueOrder !== 0) {
        return valueOrder * direction;
      }
      return left.index - right.index;
    })
    .map((row) => row.preset);
}

function createPresetConfigSortColumns(configKeys, titlePrefix = "config: ") {
  return (Array.isArray(configKeys) ? configKeys : []).map((key) => ({
    key: `config:${key}`,
    label: key,
    title: `${titlePrefix}${key}`,
    type: "text",
    getValue: (preset) => buildPresetConfigValueMap(preset).get(key) || "",
  }));
}

function createTextCell(text, className = "") {
  const cell = document.createElement("td");
  if (className) {
    cell.className = className;
  }
  cell.textContent = text;
  return cell;
}

function createPresetNameCell(name) {
  const cell = document.createElement("td");
  const nameWrap = document.createElement("div");
  nameWrap.className = "auth-preset-name";
  nameWrap.textContent = name;
  cell.appendChild(nameWrap);
  return cell;
}

function createCurrentIndicatorCell(active, { testResult = null, testKind = null, testing = false } = {}) {
  const cell = document.createElement("td");
  cell.className = "current-indicator-cell";

  if (active) {
    const arrow = document.createElement("span");
    arrow.className = "current-indicator-arrow";
    arrow.textContent = "\u2192";
    cell.appendChild(arrow);
  }

  if (testing) {
    const pending = document.createElement("span");
    pending.className = "preset-test-summary is-pending";
    pending.dataset.tone = "info";
    pending.dataset.kind = testKind || "";
    if (testResult && (testResult.preset_id || testResult.name)) {
      pending.dataset.presetId = testResult.preset_id || "";
      pending.tabIndex = 0;
      pending.setAttribute("role", "button");
      pending.setAttribute("aria-haspopup", "dialog");
      pending.setAttribute("aria-expanded", "false");
    }
    pending.textContent = "\u6d4b\u8bd5\u4e2d\u2026";
    pending.setAttribute("aria-label", "\u6b63\u5728\u6d4b\u8bd5");
    cell.appendChild(pending);
    cell.classList.add("has-test-result", "is-testing");
    return cell;
  }

  if (testResult && (testResult.preset_id || testResult.name)) {
    const summary = document.createElement("span");
    summary.className = `preset-test-summary is-${testResult.ok ? "ok" : "fail"}`;
    summary.dataset.tone = testResult.ok ? "ok" : "warn";
    summary.dataset.presetId = testResult.preset_id || "";
    summary.dataset.kind = testKind || "";
    summary.tabIndex = 0;
    summary.setAttribute("role", "button");
    summary.setAttribute("aria-haspopup", "dialog");
    summary.setAttribute("aria-expanded", "false");
    const stateText = testResult.ok ? "\u901a\u8fc7" : "\u5931\u8d25";
    const latency = Number.isFinite(Number(testResult.latency_ms))
      ? `${Number(testResult.latency_ms)}ms`
      : null;
    const shortMessage =
      typeof testResult.message === "string" ? testResult.message.trim() : "";
    const http = testResult.status ? `HTTP ${testResult.status}` : null;
    summary.textContent = [stateText, http, latency].filter(Boolean).join(" \u00b7 ");
    summary.title = shortMessage || "";

    const ariaLatency = latency || "\u65e0\u5ef6\u8fdf";
    const ariaHttp = testResult.status ? `, HTTP ${testResult.status}` : "";
    summary.setAttribute(
      "aria-label",
      `${testResult.ok ? "\u6d4b\u8bd5\u901a\u8fc7" : "\u6d4b\u8bd5\u5931\u8d25"}\uff0c${ariaLatency}${ariaHttp}${shortMessage ? `\u3002${shortMessage}` : ""}\uff08\u5c55\u5f00\u67e5\u770b\u8be6\u60c5\uff09`,
    );

    cell.appendChild(summary);
    cell.classList.add("has-test-result");
  }

  return cell;
}

const presetTestPopup = (() => {
  let overlayEl = null;
  let popupEl = null;
  let cardEl = null;
  let currentAnchor = null;
  let currentContext = null;
  let hoverTimer = null;
  let outsidePointerHandler = null;
  let escHandler = null;
  let resizeHandler = null;

  function ensureDom() {
    if (overlayEl) return;
    overlayEl = document.createElement("div");
    overlayEl.className = "preset-test-popup-overlay";
    overlayEl.hidden = true;

    popupEl = document.createElement("div");
    popupEl.className = "preset-test-popup";
    popupEl.setAttribute("role", "dialog");
    popupEl.setAttribute("aria-modal", "false");
    popupEl.tabIndex = -1;

    cardEl = document.createElement("div");
    cardEl.className = "preset-test-popup-card";

    popupEl.appendChild(cardEl);
    overlayEl.appendChild(popupEl);
    document.body.appendChild(overlayEl);
  }

  function clearTimers() {
    if (hoverTimer) {
      clearTimeout(hoverTimer);
      hoverTimer = null;
    }
  }

  function detachListeners() {
    if (outsidePointerHandler) {
      document.removeEventListener("pointerdown", outsidePointerHandler, true);
      outsidePointerHandler = null;
    }
    if (escHandler) {
      document.removeEventListener("keydown", escHandler, true);
      escHandler = null;
    }
    if (resizeHandler) {
      window.removeEventListener("resize", resizeHandler);
      window.removeEventListener("scroll", resizeHandler, true);
      resizeHandler = null;
    }
  }

  function formatTestedAt(timestamp) {
    if (!Number.isFinite(Number(timestamp))) return "";
    try {
      const date = new Date(Number(timestamp));
      if (Number.isNaN(date.getTime())) return "";
      return date.toLocaleString();
    } catch {
      return "";
    }
  }

  function buildCard(result, context) {
    cardEl.replaceChildren();
    cardEl.dataset.tone = result.ok ? "ok" : "warn";

    const header = document.createElement("div");
    header.className = "preset-test-popup-header";

    const name = document.createElement("strong");
    name.className = "preset-test-popup-name";
    name.textContent =
      result.name || context?.preset?.name || result.preset_id || "未知预设";
    name.title = name.textContent;

    const stateBadge = document.createElement("span");
    stateBadge.className = `preset-test-popup-state is-${result.ok ? "ok" : "fail"}`;
    stateBadge.textContent = result.ok ? "通过" : "失败";

    const close = document.createElement("button");
    close.type = "button";
    close.className = "preset-test-popup-close";
    close.setAttribute("aria-label", "关闭");
    close.textContent = "×";
    close.addEventListener("click", (event) => {
      event.stopPropagation();
      hide();
    });

    header.append(name, stateBadge, close);
    cardEl.appendChild(header);

    const meta = document.createElement("div");
    meta.className = "preset-test-popup-meta mono-text";
    const latency = Number.isFinite(Number(result.latency_ms))
      ? `${Number(result.latency_ms)} ms`
      : "—";
    const status = result.status ? `HTTP ${result.status}` : "无 HTTP 状态";
    const testedAt = formatTestedAt(result.tested_at);
    meta.textContent = [status, latency, testedAt].filter(Boolean).join(" · ");
    cardEl.appendChild(meta);

    const messageWrap = document.createElement("pre");
    messageWrap.className = "preset-test-popup-message";
    messageWrap.textContent = result.message || (result.ok ? "测试通过" : "测试失败");
    cardEl.appendChild(messageWrap);

    if (context?.kind) {
      const footer = document.createElement("div");
      footer.className = "preset-test-popup-footer meta-text";
      footer.textContent = context.kind === "auth"
        ? "Codex OAuth 预设测试"
        : context.kind === "api"
          ? "API 预设测试"
          : "Claude 预设测试";
      cardEl.appendChild(footer);
    }
  }

  function position() {
    if (!popupEl || !currentAnchor) return;
    const margin = 8;
    const vw = window.innerWidth;
    const vh = window.innerHeight;
    popupEl.style.visibility = "hidden";
    popupEl.style.left = "0px";
    popupEl.style.top = "0px";
    const rect = popupEl.getBoundingClientRect();
    const aRect = currentAnchor.getBoundingClientRect();
    let left = aRect.right + margin;
    let top = aRect.top;
    if (left + rect.width + margin > vw) {
      left = aRect.left - rect.width - margin;
    }
    if (left < margin) left = margin;
    if (top + rect.height + margin > vh) {
      top = vh - rect.height - margin;
    }
    if (top < margin) top = margin;
    popupEl.style.left = `${Math.round(left)}px`;
    popupEl.style.top = `${Math.round(top)}px`;
    popupEl.style.visibility = "visible";
  }

  function show(anchor, result, context = {}) {
    if (!anchor || !result) return;
    ensureDom();
    hide({ keepDom: true });
    currentAnchor = anchor;
    currentContext = context;
    buildCard(result, context);
    overlayEl.hidden = false;
    position();
    anchor.setAttribute("aria-expanded", "true");

    outsidePointerHandler = (event) => {
      if (!popupEl || !currentAnchor) return;
      if (popupEl.contains(event.target)) return;
      if (currentAnchor.contains(event.target)) return;
      hide();
    };
    document.addEventListener("pointerdown", outsidePointerHandler, true);

    escHandler = (event) => {
      if (event.key === "Escape") hide();
    };
    document.addEventListener("keydown", escHandler, true);

    resizeHandler = () => position();
    window.addEventListener("resize", resizeHandler);
    window.addEventListener("scroll", resizeHandler, true);
  }

  function hide() {
    clearTimers();
    if (!overlayEl) return;
    overlayEl.hidden = true;
    if (currentAnchor && currentAnchor.isConnected) {
      currentAnchor.setAttribute("aria-expanded", "false");
    }
    currentAnchor = null;
    currentContext = null;
    detachListeners();
  }

  function scheduleShow(anchor, result, context, delayMs = 300) {
    clearTimers();
    if (!anchor || !result) return;
    hoverTimer = window.setTimeout(() => {
      hoverTimer = null;
      show(anchor, result, context);
    }, delayMs);
  }

  function cancelScheduled() {
    clearTimers();
  }

  return { show, hide, scheduleShow, cancelScheduled };
})();

function lookupPresetTestResult(badge) {
  if (!badge) return null;
  const presetId = badge.dataset.presetId;
  const kind = badge.dataset.kind;
  if (!presetId) return null;
  if (kind === "auth") {
    return { result: state.authPresetTestResults.get(presetId) || null, kind };
  }
  if (kind === "api") {
    return { result: state.apiPresetTestResults.get(presetId) || null, kind };
  }
  if (kind === "claude") {
    return { result: state.claudePresetTestResults.get(presetId) || null, kind };
  }
  return null;
}

function installPresetTestBadgeListeners() {
  const HOVER_DELAY = 300;
  const SELECTOR = ".preset-test-summary";
  document.addEventListener("pointerover", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const badge = target.closest(SELECTOR);
    if (!badge) return;
    const payload = lookupPresetTestResult(badge);
    if (!payload || !payload.result) return;
    presetTestPopup.scheduleShow(badge, payload.result, { kind: payload.kind });
  });
  document.addEventListener("pointerout", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const badge = target.closest(SELECTOR);
    if (!badge) return;
    presetTestPopup.cancelScheduled();
  });
  document.addEventListener("focusin", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const badge = target.closest(SELECTOR);
    if (!badge) return;
    const payload = lookupPresetTestResult(badge);
    if (!payload || !payload.result) return;
    presetTestPopup.show(badge, payload.result, { kind: payload.kind });
  });
  document.addEventListener("click", (event) => {
    const target = event.target;
    if (!(target instanceof Element)) return;
    const badge = target.closest(SELECTOR);
    if (!badge) return;
    const payload = lookupPresetTestResult(badge);
    if (!payload || !payload.result) return;
    event.stopPropagation();
    presetTestPopup.show(badge, payload.result, { kind: payload.kind });
  });
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    const target = event.target;
    if (!(target instanceof Element)) return;
    const badge = target.closest(SELECTOR);
    if (!badge) return;
    const payload = lookupPresetTestResult(badge);
    if (!payload || !payload.result) return;
    event.preventDefault();
    presetTestPopup.show(badge, payload.result, { kind: payload.kind });
  });
}

const presetActionMenu = (() => {
  let menuEl = null;
  let currentTrigger = null;
  let outsidePointerHandler = null;
  let keydownHandler = null;
  let repositionHandler = null;

  function ensureDom() {
    if (menuEl) return;
    menuEl = document.createElement("div");
    menuEl.className = "preset-action-menu";
    menuEl.setAttribute("role", "menu");
    menuEl.hidden = true;
    document.body.appendChild(menuEl);
  }

  function menuItems() {
    return Array.from(menuEl?.querySelectorAll('[role="menuitem"]:not(:disabled)') || []);
  }

  function detachListeners() {
    if (outsidePointerHandler) {
      document.removeEventListener("pointerdown", outsidePointerHandler, true);
      outsidePointerHandler = null;
    }
    if (keydownHandler) {
      document.removeEventListener("keydown", keydownHandler, true);
      keydownHandler = null;
    }
    if (repositionHandler) {
      window.removeEventListener("resize", repositionHandler);
      window.removeEventListener("scroll", repositionHandler, true);
      repositionHandler = null;
    }
  }

  function hide({ restoreFocus = true } = {}) {
    if (!menuEl || menuEl.hidden) return;
    menuEl.hidden = true;
    menuEl.replaceChildren();
    if (currentTrigger?.isConnected) {
      currentTrigger.setAttribute("aria-expanded", "false");
      if (restoreFocus) currentTrigger.focus();
    }
    currentTrigger = null;
    detachListeners();
  }

  function position() {
    if (!menuEl || !currentTrigger || menuEl.hidden) return;
    const margin = 8;
    const triggerRect = currentTrigger.getBoundingClientRect();
    const menuRect = menuEl.getBoundingClientRect();
    let left = triggerRect.right - menuRect.width;
    let top = triggerRect.bottom + 5;
    if (left < margin) left = margin;
    if (left + menuRect.width + margin > window.innerWidth) {
      left = window.innerWidth - menuRect.width - margin;
    }
    if (top + menuRect.height + margin > window.innerHeight) {
      top = triggerRect.top - menuRect.height - 5;
    }
    if (top < margin) top = margin;
    menuEl.style.left = `${Math.round(left)}px`;
    menuEl.style.top = `${Math.round(top)}px`;
  }

  function createMenuItem(action) {
    const item = action.href ? document.createElement("a") : document.createElement("button");
    item.className = "preset-action-menu-item";
    if (action.danger) item.classList.add("danger");
    item.setAttribute("role", "menuitem");
    item.textContent = action.label;
    if (action.title) item.title = action.title;

    if (action.href) {
      item.href = action.href;
      item.target = "_blank";
      item.rel = "noopener noreferrer";
      item.addEventListener("click", () => hide({ restoreFocus: false }));
      return item;
    }

    item.type = "button";
    item.disabled = Boolean(action.disabled);
    item.addEventListener("click", (event) => {
      event.stopPropagation();
      hide({ restoreFocus: false });
      action.handler?.();
    });
    return item;
  }

  function show(trigger, actions, label) {
    ensureDom();
    hide({ restoreFocus: false });
    currentTrigger = trigger;
    menuEl.setAttribute("aria-label", label);
    menuEl.replaceChildren(...actions.map(createMenuItem));
    menuEl.hidden = false;
    trigger.setAttribute("aria-expanded", "true");
    position();

    outsidePointerHandler = (event) => {
      if (menuEl.contains(event.target) || currentTrigger?.contains(event.target)) return;
      hide();
    };
    document.addEventListener("pointerdown", outsidePointerHandler, true);

    keydownHandler = (event) => {
      const items = menuItems();
      const currentIndex = items.indexOf(document.activeElement);
      if (event.key === "Escape") {
        event.preventDefault();
        hide();
      } else if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        event.preventDefault();
        const step = event.key === "ArrowDown" ? 1 : -1;
        const nextIndex = currentIndex < 0
          ? (step > 0 ? 0 : items.length - 1)
          : (currentIndex + step + items.length) % items.length;
        items[nextIndex]?.focus();
      } else if (event.key === "Home") {
        event.preventDefault();
        items[0]?.focus();
      } else if (event.key === "End") {
        event.preventDefault();
        items.at(-1)?.focus();
      } else if (event.key === "Tab") {
        hide({ restoreFocus: false });
      }
    };
    document.addEventListener("keydown", keydownHandler, true);

    repositionHandler = position;
    window.addEventListener("resize", repositionHandler);
    window.addEventListener("scroll", repositionHandler, true);
    menuItems()[0]?.focus();
  }

  return { hide, show };
})();

function createPresetActionMenu(actions, { label = "预设操作" } = {}) {
  const normalizedActions = Array.isArray(actions) ? actions.filter((action) => action?.label) : [];
  const trigger = document.createElement("button");
  trigger.type = "button";
  trigger.className = "mini-button preset-action-menu-trigger";
  trigger.textContent = "⋮";
  trigger.title = label;
  trigger.setAttribute("aria-label", label);
  trigger.setAttribute("aria-haspopup", "menu");
  trigger.setAttribute("aria-expanded", "false");
  trigger.disabled = normalizedActions.length === 0;
  trigger.addEventListener("click", (event) => {
    event.stopPropagation();
    presetActionMenu.show(trigger, normalizedActions, label);
  });
  return trigger;
}

function createPresetDeleteButton(handler) {
  const button = createActionButton("删除", handler, "mini-button");
  button.classList.add("preset-delete-button");
  return button;
}

function buildPresetConfigCells(configKeys, configValues) {
  return configKeys.map((key) => createTextCell(textOrDash(configValues.get(key)), "mono-text"));
}

function createActionLink(label, href, className = "mini-button accent") {
  const link = document.createElement("a");
  link.className = className;
  link.textContent = label;
  link.href = href;
  return link;
}

function createActionHandlerLink(label, handler, className = "mini-button accent") {
  const link = createActionLink(label, "#", className);
  link.addEventListener("click", (event) => {
    event.preventDefault();
    handler();
  });
  return link;
}

function createEntryLink(label, href, handler, className = "entry-link") {
  const link = document.createElement("a");
  link.className = className;
  link.textContent = label;
  link.href = href;
  link.addEventListener("click", (event) => {
    event.preventDefault();
    handler();
  });
  return link;
}

function createKindBadge(kind) {
  const badge = document.createElement("span");
  badge.className = `entry-kind ${kind}`;
  badge.textContent = kind === "dir" ? "目录" : kind === "file" ? "文件" : kind;
  return badge;
}

function createExternalUrlCell(url) {
  const cell = document.createElement("td");
  cell.className = "mono-text";

  const normalizedUrl = typeof url === "string" ? url.trim() : "";
  if (!normalizedUrl) {
    cell.textContent = "—";
    return cell;
  }

  try {
    const parsedUrl = new URL(normalizedUrl);
    if (parsedUrl.protocol === "http:" || parsedUrl.protocol === "https:") {
      const link = document.createElement("a");
      link.className = "table-url-link";
      link.textContent = normalizedUrl;
      link.href = parsedUrl.href;
      link.target = "_blank";
      link.rel = "noopener noreferrer";
      cell.appendChild(link);
      return cell;
    }
  } catch {
    // Fall through to plain text for invalid URLs.
  }

  cell.textContent = normalizedUrl;
  return cell;
}

/**
 * Shared preset move-and-persist helper.
 *
 * Both auth and api managers have identical move-order logic that differs only
 * in state key, sort table key, reorder URL, render fn, and status setter.
 * This function centralises the optimistic-update + rollback pattern.
 *
 * @param {object}   opts
 * @param {Array}    opts.presets      - current preset array
 * @param {string}   opts.presetId     - id of preset to move
 * @param {string}   opts.direction    - "up" | "down"
 * @param {string}   opts.sortTableKey - key in state.presetTableSort to clear
 * @param {string}   opts.reorderUrl   - API endpoint for persist
 * @param {string}   opts.label        - e.g. "auth" / "API" for status messages
 * @param {function} opts.renderFn     - (presets) => void
 * @param {function} opts.setStatus    - (presets) => void  optimistic update
 * @param {function} opts.getStatus    - () => presets
 * @param {function} opts.persistOrder - (url, presets) => Promise
 * @param {function} opts.updateStatus - (msg, tone) => void
 */
async function movePresetOrderWithPersist(opts) {
  const { presets, presetId, direction, sortTableKey, reorderUrl, label, renderFn } = opts;
  const nextPresets = movePresetById(presets, presetId, direction);
  if (!nextPresets) {
    return;
  }
  const previousPresets = opts.getStatus();
  opts.setStatus(nextPresets);
  if (state.presetTableSort && sortTableKey) {
    state.presetTableSort.delete(sortTableKey);
  }
  renderFn(nextPresets);

  try {
    await opts.persistOrder(reorderUrl, nextPresets);
    opts.updateStatus(`${label} 预设顺序已保存。`, "success");
  } catch (error) {
    opts.setStatus(previousPresets);
    renderFn(previousPresets);
    opts.updateStatus(`保存 ${label} 预设顺序失败：${error.message}`, "warn");
  }
}
