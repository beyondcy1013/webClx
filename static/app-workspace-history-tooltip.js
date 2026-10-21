// Floating tooltip for workspace-history rows: shows full detail on hover/focus
// without inflating the row height. Lazily creates a singleton element appended
// to document.body; positioned with fixed coordinates so it never affects layout.
//
// The tooltip is itself hoverable so the user can move the mouse into it (across
// the gap between cell and tooltip) and scroll long content. It stays open while
// the pointer is over either the cell or the tooltip, and hides after a grace
// period once the pointer leaves both.

let tooltipEl = null;
let activeTarget = null;
let hideTimer = null;
let showTimer = null;
const SHOW_DELAY = 220;
const HIDE_DELAY = 220;
// Allow moving the mouse from the cell to the tooltip across a small gap.
const PING_INTERVAL = 250;

function ensureWorkspaceHistoryTooltip() {
  if (tooltipEl && document.body.contains(tooltipEl)) {
    return tooltipEl;
  }
  tooltipEl = document.createElement("div");
  tooltipEl.className = "workspace-history-detail-tooltip";
  tooltipEl.setAttribute("role", "tooltip");
  tooltipEl.hidden = true;
  document.body.appendChild(tooltipEl);

  // Hovering the tooltip itself keeps it open and enables scrolling.
  tooltipEl.addEventListener("mouseenter", cancelHideWorkspaceHistoryTooltip);
  tooltipEl.addEventListener("mouseleave", scheduleHideWorkspaceHistoryTooltip);
  // Any wheel scroll over the tooltip must not be cancelled or hide it.
  tooltipEl.addEventListener(
    "wheel",
    (event) => {
      event.stopPropagation();
    },
    { passive: true },
  );
  return tooltipEl;
}

function positionWorkspaceHistoryTooltip(target, tooltip) {
  const rect = target.getBoundingClientRect();
  const margin = 8;
  let left = rect.left;
  let top = rect.bottom + margin;

  const tipRect = tooltip.getBoundingClientRect();
  const vw = window.innerWidth;
  const vh = window.innerHeight;

  // Flip above if not enough room below.
  if (top + tipRect.height > vh - margin) {
    top = rect.top - tipRect.height - margin;
  }
  if (top < margin) {
    top = margin;
  }
  // Clamp horizontally.
  if (left + tipRect.width > vw - margin) {
    left = Math.max(margin, vw - tipRect.width - margin);
  }
  if (left < margin) {
    left = margin;
  }

  tooltip.style.left = `${Math.round(left)}px`;
  tooltip.style.top = `${Math.round(top)}px`;
}

function showWorkspaceHistoryTooltip(target, { title, dir }) {
  const tooltip = ensureWorkspaceHistoryTooltip();
  activeTarget = target;
  tooltip.textContent = "";

  const titleEl = document.createElement("div");
  titleEl.className = "workspace-history-detail-tooltip-title";
  titleEl.textContent = title || "—";
  tooltip.appendChild(titleEl);

  if (dir) {
    const dirEl = document.createElement("div");
    dirEl.className = "workspace-history-detail-tooltip-dir";
    dirEl.textContent = dir;
    tooltip.appendChild(dirEl);
  }

  tooltip.hidden = false;
  // Reset scroll to top when (re)showing new content.
  tooltip.scrollTop = 0;
  // Measure with the actual content before positioning.
  positionWorkspaceHistoryTooltip(target, tooltip);
  tooltip.classList.add("is-visible");
  cancelHideWorkspaceHistoryTooltip();
}

function cancelHideWorkspaceHistoryTooltip() {
  if (hideTimer) {
    window.clearTimeout(hideTimer);
    hideTimer = null;
  }
}

function scheduleHideWorkspaceHistoryTooltip() {
  cancelHideWorkspaceHistoryTooltip();
  hideTimer = window.setTimeout(() => {
    hideWorkspaceHistoryTooltipNow();
    hideTimer = null;
  }, HIDE_DELAY);
}

function hideWorkspaceHistoryTooltipNow() {
  if (!tooltipEl) {
    return;
  }
  tooltipEl.classList.remove("is-visible");
  tooltipEl.hidden = true;
  activeTarget = null;
}

function attachWorkspaceHistoryTooltip(target, { title, dir }) {
  target.addEventListener("mouseenter", () => {
    cancelHideWorkspaceHistoryTooltip();
    if (showTimer) {
      window.clearTimeout(showTimer);
    }
    showTimer = window.setTimeout(() => {
      showWorkspaceHistoryTooltip(target, { title, dir });
      showTimer = null;
    }, SHOW_DELAY);
  });

  // Use a short defer so moving from the cell toward the tooltip does not hide
  // it before the tooltip's own mouseenter fires.
  target.addEventListener("mouseleave", () => {
    if (showTimer) {
      window.clearTimeout(showTimer);
      showTimer = null;
    }
    scheduleHideWorkspaceHistoryTooltip();
  });

  target.addEventListener("focus", () => {
    if (showTimer) {
      window.clearTimeout(showTimer);
    }
    showWorkspaceHistoryTooltip(target, { title, dir });
    showTimer = null;
  });

  target.addEventListener("blur", () => {
    if (showTimer) {
      window.clearTimeout(showTimer);
      showTimer = null;
    }
    scheduleHideWorkspaceHistoryTooltip();
  });
}

window.ensureWorkspaceHistoryTooltip = ensureWorkspaceHistoryTooltip;
window.attachWorkspaceHistoryTooltip = attachWorkspaceHistoryTooltip;
