const APP_TITLE_BASE = "webClx";
const LEGACY_IDLE_SESSION_STORAGE_KEY = "webclx:idle-terminal-sessions";
const TERMINAL_PATH_EXPANDED_STORAGE_KEY = "webclx:terminal-path-expanded";
const TERMINAL_WIDE_MODE_STORAGE_KEY = "webclx:terminal-wide-mode";
const TERMINAL_AUTO_CONTINUE_STORAGE_KEY = "webclx:terminal-auto-continue-on-error";
const TERMINAL_SESSION_DETAILS_STORAGE_KEY = "webclx:terminal-session-details";
const TERMINAL_SESSION_AGENT_STORAGE_KEY = "webclx:terminal-session-agent";
const TERMINAL_AUTO_CONTINUE_SCHEDULE_ACK_STORAGE_KEY =
  "webclx:terminal-auto-continue-schedule-acks";
// Legacy localStorage keys kept only for stale cross-tab cancel broadcasts.
// Canonical paste scheduled sends are persisted and triggered on the server.
const TERMINAL_PASTE_SCHEDULED_STORAGE_KEY = "webclx:terminal-paste-scheduled";
const TERMINAL_PASTE_SCHEDULED_CANCEL_STORAGE_KEY = "webclx:terminal-paste-scheduled-cancel";
const TERMINAL_AUTO_CONTINUE_SCHEDULE_ACK_TTL_MS = 24 * 60 * 60 * 1000;
const TERMINAL_SESSION_ACTIVITY_REFRESH_MS = 6000;
const TERMINAL_SESSION_ACTIVITY_INTERACTION_RETRY_MS = 1000;
const TERMINAL_SESSION_SELECT_INTERACTION_FLUSH_MS = 120;
const TERMINAL_COMPLETION_BELL_URL = "/api/terminal/completion-bell.wav";
const TERMINAL_LAYOUT_SCROLL_SUPPRESSION_MS = 260;
const TERMINAL_INPUT_FLUSH_DELAY_MS = 8;
const TERMINAL_LIVE_OUTPUT_COALESCE_MS = 8;
const TERMINAL_REPLAY_OUTPUT_MERGE_MAX_BYTES = 256 * 1024;
const TERMINAL_LIVE_OUTPUT_MERGE_MAX_BYTES = 256 * 1024;
const TERMINAL_RESIZE_FLUSH_DELAY_MS = 40;
const TERMINAL_SIZE_SETTLE_FRAMES = 3;
const TERMINAL_SIZE_SETTLE_INTERVAL_MS = 100;
const TERMINAL_VIEWPORT_RESIZE_DEBOUNCE_MS = 240;
const TERMINAL_PAGE_SCROLL_BOTTOM_TOLERANCE_PX = 8;
const TERMINAL_SESSION_PAGE_SCROLL_RESTORE_MS = 1200;
const {
  DEFAULT_FONT_SIZE_TIER_1,
  DEFAULT_FONT_SIZE_TIER_2,
  DEFAULT_FONT_SIZE_TIER_3,
  DEFAULT_FONT_SIZE_TIER_4,
  DEFAULT_FONT_SIZE_TIERS,
  DEFAULT_TERMINAL_ACTIVITY_AGENT_DISPLAY,
  DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS,
  DEFAULT_TERMINAL_FAB_ACTION_COLOR,
  DEFAULT_TERMINAL_FAB_ACTION_OPACITY,
  DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH,
  DEFAULT_TERMINAL_PAGE_FUNCTION_COMMANDS,
  DEFAULT_TERMINAL_QUICK_COMMANDS,
  DEFAULT_TERMINAL_QUICK_START_DEFAULT_KEY,
  DEFAULT_TERMINAL_RENAME_PRESETS,
  DEFAULT_TERMINAL_SLASH_COMMANDS,
  DEFAULT_TERMINAL_COMMAND_COLLECTIONS,
  TERMINAL_TOOL_ACTION_TYPES,
  TERMINAL_TOOL_ROOTS,
  DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE,
  DEFAULT_TERMINAL_SCROLLBACK_LINES,
  DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
  DEFAULT_THEME_MODE,
  MAX_TERMINAL_FUNCTION_COMMANDS,
  MAX_TERMINAL_QUICK_COMMANDS,
  TERMINAL_KEYBOARD_CHECKBOX_ACTIONS,
  THEME_MODE_STORAGE_KEY,
  cloneDefaultTerminalFunctionCommands,
  cloneDefaultTerminalCommandCollections,
  ensureBuiltInTerminalFunctionCommands,
  ensureBuiltInTerminalSlashCommands,
  ensureBuiltInTerminalToolEntries,
  normalizeFontSizeTier,
  normalizeFontSizeTiers,
  normalizeHostName,
  normalizeTerminalActivityAgentDisplay,
  normalizeTerminalAutoContinueIntervalSeconds,
  normalizeTerminalFabActionColor,
  normalizeTerminalFabActionOpacity,
  normalizeTerminalFloatingButtonOffsetVh,
  normalizeTerminalFunctionCommandLine,
  normalizeTerminalFunctionCommands,
  normalizeTerminalCommandCollections,
  normalizeTerminalToolEntries,
  normalizeTerminalQuickCommands,
  normalizeTerminalQuickStartDefaultKey,
  normalizeTerminalQuickText,
  normalizeTerminalRenamePreset,
  normalizeTerminalRenamePresets,
  normalizeTerminalSoftKeyboardScale,
  normalizeTerminalScrollbackLines,
  normalizeTerminalTouchSelectionLongPressMs,
  normalizeThemeMode,
  readStoredThemeMode,
} = globalThis.WebClxTerminalSettings;
const {
  DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
  normalizeProjectIconPath,
  enhanceWorkspaceIconSelect,
} = globalThis.WebClxWorkspaceProjectIcons;
const {
  isSessionBusy: sharedIsSessionBusy,
  isSessionErrorState: sharedIsSessionErrorState,
  nextTerminalSessionSortMode: sharedNextTerminalSessionSortMode,
  normalizeTerminalSessionSortMode: sharedNormalizeTerminalSessionSortMode,
  sessionActivityAgentLabel: sharedSessionActivityAgentLabel,
  sessionActivityAgentPrefix: sharedSessionActivityAgentPrefix,
  sessionActivityAgentSuffix: sharedSessionActivityAgentSuffix,
  sessionAfterOutputViewed: sharedSessionAfterOutputViewed,
  sessionActivityLabel: sharedSessionActivityLabel,
  sessionActivityState: sharedSessionActivityState,
  sessionActivityText: sharedSessionActivityText,
  sessionErrorContinueKey: sharedSessionErrorContinueKey,
  sessionTimestamp: sharedSessionTimestamp,
  sortSessionsByRecentActivity: sharedSortSessionsByRecentActivity,
  sortTerminalSessions: sharedSortTerminalSessions,
  terminalSessionSortModeLabel: sharedTerminalSessionSortModeLabel,
  terminalSessionActivityPrefix: sharedTerminalSessionActivityPrefix,
} = globalThis.WebClxTerminalSessionActivity;
const {
  SESSION_EVENT_STORAGE_KEY,
  RESUME_ARCHIVE_EVENT_STORAGE_KEY,
  SETTINGS_EVENT_STORAGE_KEY,
  announceSessionMutation: sharedAnnounceSessionMutation,
  getStoredGlobalSessionId,
  getStoredSessionId,
  parseSessionMutationEvent,
  readSessionPreferences,
  sessionPreferenceKey,
  shouldRefreshForSessionMutation,
  storeGlobalSessionId,
  storeSessionId,
} = globalThis.WebClxTerminalSessionStorage;
const terminalResumeExtract = globalThis.WebClxTerminalResumeExtract || null;
const DEFAULT_TERMINAL_FUNCTION_COMMANDS = DEFAULT_TERMINAL_PAGE_FUNCTION_COMMANDS;
const NEW_SESSION_QUICK_START_TIMEOUT_MS = 3000;
const TERMINAL_WIDE_MODE_WIDTH_RATIO = 1.9;
const TERMINAL_WIDE_MODE_MIN_WIDTH_PX = 760;
const TERMINAL_WIDE_MODE_MAX_WIDTH_PX = 1280;
const XTERM_CELL_ATTR_DIM_MASK = 0x8000000;
const XTERM_CELL_ATTR_INVERSE_MASK = 0x4000000;
const TERMINAL_PASTE_IMAGE_TYPES = new Set([
  "image/png",
  "image/jpeg",
  "image/jpg",
  "image/gif",
  "image/webp",
  "image/bmp",
]);
const terminalScrollPositions = new Map();

function readStoredTerminalWideMode() {
  try {
    return window.localStorage.getItem(TERMINAL_WIDE_MODE_STORAGE_KEY) === "enabled";
  } catch {
    return false;
  }
}

function storeTerminalWideMode(enabled) {
  try {
    window.localStorage.setItem(TERMINAL_WIDE_MODE_STORAGE_KEY, enabled ? "enabled" : "disabled");
  } catch {
    // Keep the terminal usable when storage is unavailable.
  }
}

function readStoredTerminalSessionDetails() {
  try {
    return window.localStorage.getItem(TERMINAL_SESSION_DETAILS_STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

function storeTerminalSessionDetails(enabled) {
  try {
    window.localStorage.setItem(TERMINAL_SESSION_DETAILS_STORAGE_KEY, enabled ? "true" : "false");
  } catch {
    // Keep the toggle usable when storage is unavailable.
  }
}

function readStoredTerminalSessionAgent() {
  try {
    return window.localStorage.getItem(TERMINAL_SESSION_AGENT_STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

function storeTerminalSessionAgent(enabled) {
  try {
    window.localStorage.setItem(TERMINAL_SESSION_AGENT_STORAGE_KEY, enabled ? "true" : "false");
  } catch {
    // Keep the toggle usable when storage is unavailable.
  }
}

function applyDocumentTitle(hostName = state.hostName) {
  const normalizedHostName = normalizeHostName(hostName);
  document.title = normalizedHostName
    ? `${APP_TITLE_BASE} - ${normalizedHostName}`
    : APP_TITLE_BASE;
}

function readStoredTerminalAutoContinueOnError() {
  try {
    return window.localStorage.getItem(TERMINAL_AUTO_CONTINUE_STORAGE_KEY) === "true";
  } catch {
    return false;
  }
}

function storeTerminalAutoContinueOnError(enabled) {
  try {
    window.localStorage.setItem(TERMINAL_AUTO_CONTINUE_STORAGE_KEY, enabled ? "true" : "false");
  } catch {
    // Keep the runtime toggle active even if persistence is unavailable.
  }
}

async function persistTerminalAutoContinueOnError(enabled) {
  await requestJson("/api/settings", {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ terminal_auto_continue_on_error: Boolean(enabled) }),
  });
}

function readAutoContinueScheduleAcks() {
  try {
    const parsed = JSON.parse(
      window.localStorage.getItem(TERMINAL_AUTO_CONTINUE_SCHEDULE_ACK_STORAGE_KEY) || "{}",
    );
    return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? parsed : {};
  } catch {
    return {};
  }
}

function writeAutoContinueScheduleAcks(acks) {
  try {
    window.localStorage.setItem(
      TERMINAL_AUTO_CONTINUE_SCHEDULE_ACK_STORAGE_KEY,
      JSON.stringify(acks),
    );
  } catch {
    // The in-memory handled map still suppresses repeats for this page.
  }
}

function autoContinueScheduleAckKey(sessionId, resetAt) {
  return `${String(sessionId || "").trim()}\n${String(resetAt || "").trim()}`;
}

function hasAutoContinueScheduleAck(sessionId, resetAt, now = Date.now()) {
  const key = autoContinueScheduleAckKey(sessionId, resetAt);
  if (!key.trim()) {
    return false;
  }
  const acks = readAutoContinueScheduleAcks();
  const acknowledgedAt = Number(acks[key] || 0);
  return Number.isFinite(acknowledgedAt) &&
    acknowledgedAt > 0 &&
    now - acknowledgedAt < TERMINAL_AUTO_CONTINUE_SCHEDULE_ACK_TTL_MS;
}

function rememberAutoContinueScheduleAck(sessionId, resetAt, now = Date.now()) {
  const key = autoContinueScheduleAckKey(sessionId, resetAt);
  if (!key.trim()) {
    return;
  }
  const acks = readAutoContinueScheduleAcks();
  Object.entries(acks).forEach(([storedKey, value]) => {
    const acknowledgedAt = Number(value || 0);
    if (!Number.isFinite(acknowledgedAt) || now - acknowledgedAt >= TERMINAL_AUTO_CONTINUE_SCHEDULE_ACK_TTL_MS) {
      delete acks[storedKey];
    }
  });
  acks[key] = now;
  writeAutoContinueScheduleAcks(acks);
}

function readLocationState() {
  const params = new URLSearchParams(window.location.search);
  const freshValue = (params.get("fresh") || "").trim().toLowerCase();
  const quickStartValue = (params.get("quick_start") || params.get("quickStart") || "")
    .trim()
    .toLowerCase();
  const runCommand = params.get("run") || params.get("command") || "";
  return {
    path: params.get("path") || "",
    sessionId: params.get("session") || "",
    runCommand,
    fresh: freshValue === "1" || freshValue === "true" || freshValue === "yes",
    quickStart:
      quickStartValue === "1" ||
      quickStartValue === "true" ||
      quickStartValue === "yes",
  };
}

const initialLocation = readLocationState();

const state = {
  sessions: [],
  sessionSortMode: "",
  activeSessionId: initialLocation.sessionId,
  loadingSessions: false,
  pendingSessionRefresh: null,
  hasConnectedOnce: false,
  workspaceDir: "",
  currentPath: initialLocation.path,
  showAllWorkspaceSessions: true,
  terminalWorkspaceIconPath: DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
  resumeArchives: [],
  legacyIdleMigrationAttempted: false,
  loadingResumeArchives: false,
  desktopTerminalSoftKeyboardEnabled: true,
  terminalSoftKeyboardScale: DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE,
  terminalFloatingButtonOffsetVh: DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH,
  terminalFabActionColor: DEFAULT_TERMINAL_FAB_ACTION_COLOR,
  terminalFabActionOpacity: DEFAULT_TERMINAL_FAB_ACTION_OPACITY,
  terminalTouchSelectionLongPressMs: DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
  terminalScrollbackLines: DEFAULT_TERMINAL_SCROLLBACK_LINES,
  temporaryDesktopTerminalSoftKeyboardVisible: false,
  themeMode: readStoredThemeMode(),
  fontSizeTiers: [...DEFAULT_FONT_SIZE_TIERS],
  terminalQuickCommands: normalizeTerminalQuickCommands(DEFAULT_TERMINAL_QUICK_COMMANDS, undefined, { includeCommandLine: true }),
  terminalSlashCommands: ensureBuiltInTerminalSlashCommands(DEFAULT_TERMINAL_SLASH_COMMANDS),
  terminalFunctionCommands: ensureBuiltInTerminalFunctionCommands(DEFAULT_TERMINAL_FUNCTION_COMMANDS),
  terminalCommandCollections: normalizeTerminalCommandCollections(cloneDefaultTerminalCommandCollections()),
  terminalToolEntries: [],
  terminalRenamePresets: normalizeTerminalRenamePresets(DEFAULT_TERMINAL_RENAME_PRESETS),
  terminalQuickStartDefaultKey: DEFAULT_TERMINAL_QUICK_START_DEFAULT_KEY,
  terminalActivityAgentDisplay: DEFAULT_TERMINAL_ACTIVITY_AGENT_DISPLAY,
  terminalAutoContinueIntervalSeconds: DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS,
  terminalCompletionBellEnabled: true,
  terminalWideMode: readStoredTerminalWideMode(),
  showSessionDetails: readStoredTerminalSessionDetails(),
  showSessionAgent: readStoredTerminalSessionAgent(),
  autoContinueOnError: readStoredTerminalAutoContinueOnError(),
  autoContinueHandledErrors: new Map(),
  autoContinueScheduledTimers: new Map(),
  completionSoundPreviousStates: new Map(),
  completionSoundBaselineReady: false,
  creatingSession: false,
  pendingCreatedSessionIds: new Set(),
  initialTerminalIntentPending:
    Boolean(initialLocation.fresh || initialLocation.quickStart || initialLocation.runCommand),
  hostName: "",
  historyIndex: 0,
  historyMaxIndex: 0,
  renamingSessionId: "",
  pendingRunCommand: initialLocation.runCommand,
};

const statusEl = document.getElementById("terminal-status");
const terminalPathEl = document.getElementById("terminal-path");
const sessionSelectEl = document.getElementById("session-switcher");
const agentSessionSelectEl = document.getElementById("agent-session-switcher");
const sessionDetailToggleEl = document.getElementById("session-detail-toggle");
const sessionAgentToggleEl = document.getElementById("session-agent-toggle");
const sessionAutoContinueToggleEl = document.getElementById("session-auto-continue-toggle");
const terminalSessionSortButtonEl = document.getElementById("terminal-sort-directory-sessions");
const navigateBackButton = document.getElementById("navigate-back");
const terminalWorkflowsButton = document.getElementById("terminal-workflows-button");
const navigateForwardButton = document.getElementById("navigate-forward");
const renameSessionButton = document.getElementById("rename-session");
const pasteClipboardButton = document.getElementById("paste-clipboard");
const createSessionButton = document.getElementById("create-session");
const deleteSessionButton = document.getElementById("delete-session");
const idleSessionButton = document.getElementById("idle-session");
const archiveResumeButton = document.getElementById("archive-resume");
const idleSessionSelectEl = document.getElementById("idle-session-select");
const terminalNavToggleButton = document.getElementById("toggle-terminal-path");
const sessionRenameDialogEl = document.getElementById("terminal-rename-dialog");
const sessionRenameFormEl = document.getElementById("session-rename-form");
const sessionRenameInputEl = document.getElementById("session-rename-input");
const sessionRenamePresetsEl = document.getElementById("session-rename-presets");
const sessionRenameCancelButton = document.getElementById("session-rename-cancel");
const terminalRenameDialogStatusEl = document.getElementById("terminal-rename-dialog-status");

function syncCreateSessionButton() {
  if (!createSessionButton) {
    return;
  }
  createSessionButton.disabled = state.creatingSession || state.initialTerminalIntentPending;
}

function terminalSessionInitializing() {
  return state.creatingSession || state.initialTerminalIntentPending;
}

syncCreateSessionButton();
const terminalNavPathEl = document.getElementById("terminal-nav-path");
const terminalNavScrollEl = document.getElementById("terminal-nav-scroll");
const topNavLinks = document.querySelectorAll("[data-home-path]");
const terminalPageEl = document.querySelector(".terminal-page");
const terminalPageNavEl = document.querySelector(".terminal-page-nav");
const terminalControlBarEl = document.querySelector(".terminal-control-bar");
const terminalPanelEl = document.querySelector(".terminal-panel");
const terminalHost = document.getElementById("terminal-host");
const terminalScrollShellEl = terminalHost?.closest(".terminal-scroll-shell") || null;
const pageScrollRailEl = document.getElementById("page-scroll-rail");
const pageScrollThumbEl = document.getElementById("page-scroll-thumb");
const mobileKeysEl = document.getElementById("terminal-mobile-keys");
const terminalImeToggleButton = document.getElementById("terminal-ime-toggle");
const scrollPageTopButton = document.getElementById("scroll-page-top");
const scrollTerminalBottomButton = document.getElementById("scroll-terminal-bottom");
const scrollTerminalTopButton = document.getElementById("scroll-terminal-top");
const terminalInputHistoryButton = document.getElementById("terminal-input-history-button");
const terminalSoftKeyboardToggleButton = document.getElementById("terminal-soft-keyboard-toggle");
const terminalScheduleButton = document.getElementById("terminal-schedule-button");
const terminalAgentsDocDialogEl = document.getElementById("terminal-agents-doc-dialog");
const terminalAgentsDocFormEl = document.getElementById("terminal-agents-doc-form");
const terminalAgentsDocSelectEl = document.getElementById("terminal-agents-doc-select");
const terminalAgentsDocPathEl = document.getElementById("terminal-agents-doc-path");
const terminalAgentsDocStatusEl = document.getElementById("terminal-agents-doc-status");
const terminalAgentsDocEditorEl = document.getElementById("terminal-agents-doc-editor");
const terminalAgentsDocSaveButton = document.getElementById("terminal-agents-doc-save");
const terminalAgentsDocCloseButton = document.getElementById("terminal-agents-doc-close");
const terminalAgentsDocNameInputEl = document.getElementById("terminal-agents-doc-name-input");
const terminalAgentsDocCreateButton = document.getElementById("terminal-agents-doc-create");
const terminalAgentsDocRefreshButton = document.getElementById("terminal-agents-doc-refresh");
const terminalAgentsDocFilterInputEl = document.getElementById("terminal-agents-doc-filter-input");
const terminalAgentsDocMaxAgeDaysEl = document.getElementById("terminal-agents-doc-max-age-days");
const terminalAgentsDocRecursiveDirectoriesEl = document.getElementById("terminal-agents-doc-recursive-directories");
const terminalAgentsDocShowHiddenEl = document.getElementById("terminal-agents-doc-show-hidden");
const terminalInputHistoryDialogEl = document.getElementById("terminal-input-history-dialog");
const terminalInputHistoryListEl = document.getElementById("terminal-input-history-list");
const terminalInputHistoryStatusEl = document.getElementById("terminal-input-history-status");
const terminalInputHistoryCopyButton = document.getElementById("terminal-input-history-copy");
const terminalInputHistoryCloseButton = document.getElementById("terminal-input-history-close");
const terminalPasteDialogEl = document.getElementById("terminal-paste-dialog");
const terminalPasteFormEl = document.getElementById("terminal-paste-form");
const terminalPasteTextEl = document.getElementById("terminal-paste-text");
const terminalPasteCancelButton = document.getElementById("terminal-paste-cancel");
const terminalPasteSubmitButton = document.getElementById("terminal-paste-submit");
const terminalPasteSubmitEnterButton = document.getElementById("terminal-paste-submit-enter");
const terminalPasteAssetsEl = document.getElementById("terminal-paste-assets");
const terminalPasteScheduleEl = document.getElementById("terminal-paste-schedule");
const terminalPasteScheduleToggleEl = document.getElementById("terminal-paste-schedule-toggle");
const terminalPasteScheduleDelayEl = document.getElementById("terminal-paste-schedule-delay");
const terminalPasteScheduleDelayUnitEl = document.getElementById("terminal-paste-schedule-delay-unit");
const terminalPasteScheduleDatetimeEl = document.getElementById("terminal-paste-schedule-datetime");
const terminalPasteScheduleConfirmEl = document.getElementById("terminal-paste-schedule-confirm");
const terminalPasteScheduleCancelEl = document.getElementById("terminal-paste-schedule-cancel");
const terminalPasteScheduleStatusEl = document.getElementById("terminal-paste-schedule-status");
const terminalPasteScheduleChipEl = document.getElementById("terminal-paste-schedule-chip");
const terminalPasteScheduleChipTextEl = document.getElementById("terminal-paste-schedule-chip-text");
const terminalSelectionCopyButton = document.getElementById("terminal-selection-copy");
const terminalSelectionStartHandle = document.getElementById("terminal-selection-start-handle");
const terminalSelectionEndHandle = document.getElementById("terminal-selection-end-handle");
const terminalContextMenuEl = document.getElementById("terminal-context-menu");
const terminalContextCopyAllButton = document.getElementById("terminal-context-copy-all");
const terminalQuickCommandButtonsEl = document.getElementById("terminal-quick-command-buttons");
const terminalEscapeCommandButtonEl = document.getElementById("terminal-escape-command-button");
const terminalSlashCommandButtonEl = document.getElementById("terminal-slash-command-button");
const terminalSlashCommandMenuEl = document.getElementById("terminal-slash-command-menu");
const terminalFunctionCommandButtonEl = document.getElementById("terminal-function-command-button");
const terminalFunctionCommandMenuEl = document.getElementById("terminal-function-command-menu");
const terminalFunctionCommandButtonsEl = document.getElementById("terminal-function-command-buttons");
const terminalFunctionCommandSelectEl = terminalFunctionCommandButtonsEl || null;
const terminalImageUploadInputEl = document.getElementById("terminal-image-upload-input");
const terminalSystemKeyboardCheckboxEl = document.getElementById("terminal-system-keyboard-checkbox");
const terminalTouchCopyCheckboxEl = document.getElementById("terminal-touch-copy-checkbox");
const terminalProjectCommandSelectEl = document.getElementById("terminal-project-command-select");
const terminalProjectCommandButtonEl = document.getElementById("terminal-project-command-button");
const terminalProjectCommandMenuEl = document.getElementById("terminal-project-command-menu");
const terminalNumberButtonEl = document.getElementById("terminal-number-button");
const terminalNumberMenuEl = document.getElementById("terminal-number-menu");
const terminalCommandCollectionsBtnEl = document.getElementById("terminal-command-collections-btn");
const terminalCommandCollectionsMenuEl = document.getElementById("terminal-command-collections-menu");
const terminalCommandCollectionsBodyEl = document.getElementById("terminal-command-collections-body");
const terminalToolMenuEl = document.getElementById("terminal-tool-menu");
const terminalToolMenuBodyEl = document.getElementById("terminal-tool-menu-body");
const terminalToolMenuTitleEl = document.getElementById("terminal-tool-menu-title");
const terminalToolMenuStatusEl = document.getElementById("terminal-tool-menu-status");
const terminalToolMenuBackEl = document.getElementById("terminal-tool-menu-back");
const terminalToolsButtonEl = document.getElementById("terminal-tools-button");
const terminalToolsMenuEl = document.getElementById("terminal-tools-menu");
const terminalCodexFullAccessToggleEl = document.getElementById("terminal-codex-full-access-toggle");
const terminalInterruptResumeButtonEl = document.getElementById("terminal-interrupt-resume");
const terminalToolsStatusEl = document.getElementById("terminal-tools-status");

const terminalQuotaDialogEl = document.getElementById("terminal-quota-dialog");
const terminalQuotaBodyEl = document.getElementById("terminal-quota-body");
const terminalQuotaRefreshBtnEl = document.getElementById("terminal-quota-refresh");
const terminalQuotaSettingsBtnEl = document.getElementById("terminal-quota-settings");
const terminalQuotaCloseBtnEl = document.getElementById("terminal-quota-close");
const terminalQuotaSettingsPanelEl = document.getElementById("terminal-quota-settings-panel");
const terminalQuotaApiKeyInputEl = document.getElementById("terminal-quota-api-key-input");
const terminalQuotaBaseUrlInputEl = document.getElementById("terminal-quota-base-url-input");
const terminalQuotaPresetSelectEl = document.getElementById("terminal-quota-preset-select");
const terminalQuotaSaveConfigBtnEl = document.getElementById("terminal-quota-save-config");
const terminalQuotaConfigStatusEl = document.getElementById("terminal-quota-config-status");
const terminalQuotaDefaultProviderEl = document.getElementById("terminal-quota-default-provider");
const terminalQuotaKeySelectEl = document.getElementById("terminal-quota-key-select");
const terminalQuotaKeyStatusEl = document.getElementById("terminal-quota-key-status");

const MOBILE_KEY_SEQUENCES = {
  escape: "\u001b",
  tab: "\t",
  enter: "\r",
  ctrl_c: "\u0003",
  ctrl_d: "\u0004",
  ctrl_l: "\u000c",
  ctrl_r: "\u0012",
  ctrl_v: "\u0016",
  ctrl_z: "\u001a",
  arrow_up: "\u001b[A",
  arrow_down: "\u001b[B",
  arrow_right: "\u001b[C",
  arrow_left: "\u001b[D",
  home: "\u001b[H",
  end: "\u001b[F",
  ctrl_a: "\u0001",
  ctrl_e: "\u0005",
  page_up: "\u001b[5~",
  page_down: "\u001b[6~",
};
const MOBILE_KEY_REPEATABLE_SEQUENCES = new Set([
  "arrow_up",
  "arrow_down",
  "arrow_right",
  "arrow_left",
]);
const DEVICE_ATTRIBUTE_REQUEST_PATTERN = /\u001b\[(?:>0?|0?)c/g;
const DEVICE_ATTRIBUTE_RESPONSE_PREFIX_PATTERN = /^\u001b(?:\[(?:[?>]?(?:[0-9;]*)?)?)?$/;
const DEVICE_ATTRIBUTE_RESPONSE_START_PATTERN = /^\u001b\[(?:\?[0-9;]*|>[0-9;]*)c/;
const DEVICE_ATTRIBUTE_REQUEST_TAIL_LENGTH = 4;
const MAX_PENDING_DEVICE_ATTRIBUTE_RESPONSES = 4;
const MOBILE_KEY_DRAG_THRESHOLD = 12;
const MOBILE_KEY_SUBMIT_DELAY_MS = 36;
const MOBILE_TEXT_COMMAND_ENTER_DELAY_MS = 120;
const MOBILE_SLASH_COMMAND_ENTER_DELAY_MS = 500;
const MOBILE_KEY_REPEAT_INITIAL_DELAY_MS = 320;
const MOBILE_KEY_REPEAT_INTERVAL_MS = 72;
const MOBILE_ESCAPE_LONG_PRESS_MS = 500;
const MOBILE_SLASH_COMMAND_CONFIRM_DELAY_MS = 120;
const TERMINAL_SESSION_EVENT_REFRESH_DELAY_MS = 2000;
const MOBILE_KEY_BUTTON_SELECTOR = "[data-sequence], [data-text], [data-action]";
const CANVAS_READBACK_PATCH_FLAG = "__webclxWillReadFrequentlyPatched__";
const PAGE_SCROLL_TOP_THRESHOLD = 180;
const PAGE_SCROLL_RAIL_MIN_THUMB_SIZE = 42;
const PAGE_SCROLL_RAIL_PADDING = 4;
const STATUS_AUTO_HIDE_DELAY_MS = 5000;
const TERMINAL_RESUME_SCAN_MAX_LINES = 240;
const TERMINAL_SGR_MOUSE_SEQUENCE_PATTERN = /\u001b\[<(\d+);(\d+);(\d+)([Mm])/g;
const TERMINAL_X10_MOUSE_SEQUENCE_PATTERN = /\u001b\[M([\s\S])([\s\S])([\s\S])/g;
const CODEX_RESUME_COMMAND_PATTERN =
  /\bcodex\s+resume\s+[`'"]?([^\s`"'，。；；：:<>()[\]{}]+)[`'"]?/gi;
const CODEX_RESUME_ID_PATTERN = /^[A-Za-z0-9._-]{1,160}$/;
const CLAUDE_RESUME_COMMAND_PATTERN =
  /\bclaude\s+--resume\s+[`'"]?([^\s`"'，。；；：:<>()[\]{}]+)[`'"]?/gi;
const CLAUDE_RESUME_INVOKE_PATTERN = /\bclaude\s+--resume\b/gi;
const CODEX_RESUME_UUID_PATTERN =
  /^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$/;
// Codex/Claude startup banner session label, see terminal-resume-extract.js.
const BANNER_SESSION_LABEL_PATTERN =
  /(?:^|\n)[^\S\r\n]*[│|]?\s*Session:\s*([0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12})\s*[│|]?/g;
const CLAUDE_BANNER_SESSION_LABEL_PATTERN =
  /(?:^|\n)[^\S\r\n]*[│|]?\s*session\s+id:\s*([0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12})\s*[│|]?/gi;

applyThemeMode(state.themeMode);
enableCanvasReadbackOptimization();
if (terminalPathEl) {
  terminalPathEl.textContent = "/";
}
if (terminalNavPathEl) {
  terminalNavPathEl.textContent = state.currentPath ? `/${state.currentPath}` : "/";
}
// Prevent the browser from restoring a stale scroll position on F5/refresh.
// Terminal content loads asynchronously via WebSocket backlog replay; if the
// browser restores the old document/element scroll before that content
// settles, the terminal viewport ends up stranded at the top. The code below
// owns scroll restoration entirely.
if ("scrollRestoration" in history) {
  history.scrollRestoration = "manual";
}
const {
  clampTerminalSelectionPoint,
  terminalSelectionRangeFromPoints,
  terminalSelectionPointFromClient,
} = globalThis.WebClxTerminalSelectionGeometry || {};
const terminalCursorGuard = globalThis.WebClxTerminalCursorGuard || null;
const terminalImePolicy = globalThis.WebClxTerminalImePolicy || {
  TERMINAL_SYSTEM_IME_SUPPRESSION_MS: 60 * 1000,
  terminalImeDirectFocusAction: ({ now, suppressedUntil }) =>
    Number(now || Date.now()) >= Number(suppressedUntil || 0) ? "focus" : "blocked",
  terminalImeFocusAllowed: ({ now, suppressedUntil }) =>
    Number(now || Date.now()) >= Number(suppressedUntil || 0),
  terminalImeFunctionAction: (command, now = Date.now()) => {
    if (command?.action === "disable_system_keyboard") {
      return { kind: "disable", suppressedUntil: now + 60 * 1000 };
    }
    if (command?.action === "show_system_keyboard") {
      return { kind: "show", suppressedUntil: 0 };
    }
    return { kind: "none", suppressedUntil: null };
  },
  terminalImeToggleAction: ({ systemImeEnabled, helperFocused }) =>
    systemImeEnabled && helperFocused ? "disable" : "focus",
};
const terminalTouchSelectionPolicy = globalThis.WebClxTerminalTouchSelectionPolicy || {
  TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS: 2000,
  TERMINAL_TOUCH_SELECTION_DRAG_CANCEL_PX: 8,
  terminalTouchScrollStep: ({ deltaPixels, remainderPixels, rowHeight }) => {
    const effectiveRowHeight = Math.max(Number(rowHeight) || 1, 1);
    const pixelsPerLine = Math.max(effectiveRowHeight, 8);
    const totalPixels = (Number(remainderPixels) || 0) + (Number(deltaPixels) || 0);
    const lines = Math.trunc(totalPixels / pixelsPerLine);
    return {
      lines,
      remainderPixels: totalPixels - lines * pixelsPerLine,
    };
  },
  terminalTouchSelectionContextMenuAction: ({ elapsedMs, longPressMs }) =>
    Number(elapsedMs) >= normalizeTerminalTouchSelectionLongPressMs(longPressMs)
      ? "select"
      : "ignore",
  terminalTouchSelectionInitialRange: (point, columns) => ({
    column: Math.min(Math.max(Math.trunc(Number(point?.column) || 0), 0), Math.max(Math.trunc(Number(columns) || 1) - 1, 0)),
    row: Math.max(Math.trunc(Number(point?.row) || 0), 0),
    length: 1,
  }),
  terminalTouchSelectionMoveAction: ({ offsetX, offsetY }) => {
    if (Math.hypot(Number(offsetX) || 0, Number(offsetY) || 0) <= 8) {
      return "keep";
    }
    return "cancel";
  },
  terminalTouchSelectionRangeBetweenCells: (anchorPoint, focusPoint, columns) => {
    const cols = Math.max(Math.trunc(Number(columns) || 1), 1);
    const anchorColumn = Math.min(Math.max(Math.trunc(Number(anchorPoint?.column) || 0), 0), cols - 1);
    const focusColumn = Math.min(Math.max(Math.trunc(Number(focusPoint?.column) || 0), 0), cols - 1);
    const anchorRow = Math.max(Math.trunc(Number(anchorPoint?.row) || 0), 0);
    const focusRow = Math.max(Math.trunc(Number(focusPoint?.row) || 0), 0);
    const startIndex = Math.min(anchorRow * cols + anchorColumn, focusRow * cols + focusColumn);
    const endIndex = Math.max(anchorRow * cols + anchorColumn, focusRow * cols + focusColumn);
    return {
      column: startIndex % cols,
      row: Math.floor(startIndex / cols),
      length: endIndex - startIndex + 1,
    };
  },
};

let socket = null;
let reconnectTimer = null;
let connectionToken = 0;
let lastTerminalSize = null;
let lastTerminalHostHeight = "";
let terminalOutputTail = "";
let terminalInputTail = "";
let pendingDeviceAttributeResponses = 0;
let terminalOverlayObserver = null;
let terminalLayoutObserver = null;
let terminalViewportScrollEl = null;
let pageScrollRailDrag = null;
let terminalNavScrollDrag = null;
let mobileKeyPress = null;
let mobileKeySendQueue = Promise.resolve();
let terminalTouchSelection = null;
let terminalTouchSelectionCandidate = null;
let terminalTouchSelectionDisabled = false;
let terminalSelectionHandleDrag = null;
let terminalSystemImeEnabled = !terminalSoftKeyboardAutoVisible();
let terminalSystemImeSuppressedUntil = 0;
let sessionEventRefreshTimer = null;
let statusDismissTimer = null;
let pendingNewSessionQuickStart = null;
let terminalInitialReplayPending = false;
let terminalBacklogReplayActive = false;
let terminalBacklogReplayEndQueued = false;
let terminalBacklogReplayInterrupted = false;
let terminalSwitchPlaceholderEl = null;
let terminalOutputQueue = [];
let terminalOutputWriteInFlight = false;
let terminalOutputWriteId = 0;
let terminalInputQueue = [];
let terminalInputFlushTimer = null;
let pendingTerminalSize = null;
let terminalSizeFlushTimer = null;
let terminalSizeSettleTimer = null;
let terminalSizeSettleToken = 0;
let terminalScrollSaveSuppressionDepth = 0;
let terminalScrollSaveSuppressedUntil = 0;
let terminalScrollSaveSuppressionTimer = null;
let terminalScrollLayoutRestoreToken = 0;
let pageScrollLayoutRestoreToken = 0;
let sessionPageScrollRestoreToken = 0;
let activeSessionPageScrollRestore = null;
let sessionPageScrollProgrammaticUntil = 0;
let sessionActivityRefreshTimer = null;
let sessionActivityRefreshPending = false;
let sessionDropdownInteracting = false;
let sessionDropdownFlushTimer = null;
let terminalPasteAssetEntries = [];
let terminalPasteBusy = false;
// Pending background paste scheduled sends, keyed by taskId. Supports an
// unlimited number of concurrent schedules; the 2nd task no longer clears the
// 1st. Each value: { taskId, timer, at, label, snapshot }.
let terminalPasteScheduledTasks = new Map();
let terminalAutoContinueScheduledTasks = new Map();
// Single shared interval that ticks every pending task's countdown once.
let terminalPasteScheduledCountdownTimer = null;
let terminalInputHistoryEntries = [];
let terminalInputHistoryLoading = false;
let terminalInputHistoryStatusDismissTimer = null;
let terminalAgentsDocStatusDismissTimer = null;
let terminalAgentsDocSessionId = "";
let terminalAgentsDocAllDocuments = [];

mountTerminalInstance();

function sessionTimestamp(session, snakeKey, camelKey) {
  return sharedSessionTimestamp(session, snakeKey, camelKey);
}

function sortSessionsByRecentActivity(sessions) {
  return state.sessionSortMode
    ? sharedSortTerminalSessions(sessions, state.sessionSortMode)
    : sharedSortSessionsByRecentActivity(sessions);
}

function insertOrReplaceSession(session) {
  if (!session?.id) {
    return;
  }

  const nextSessions = state.sessions.filter((item) => item.id !== session.id);
  nextSessions.push(session);
  state.sessions = sortSessionsByRecentActivity(nextSessions);
}

function isIdleSession(sessionId) {
  const session = state.sessions.find((item) => item.id === sessionId);
  return Boolean(session?.idle);
}

function readLegacyIdleSessionIds() {
  try {
    const parsed = JSON.parse(window.localStorage.getItem(LEGACY_IDLE_SESSION_STORAGE_KEY) || "[]");
    return Array.isArray(parsed)
      ? parsed.filter((value) => typeof value === "string" && value.trim())
      : [];
  } catch {
    return [];
  }
}

function visibleSessions() {
  return state.sessions.filter((session) => !isIdleSession(session.id));
}

function idleSessions() {
  return state.sessions.filter((session) => isIdleSession(session.id));
}

function updateSessionIdleState(sessionId, idle) {
  state.sessions = state.sessions.map((session) =>
    session.id === sessionId ? { ...session, idle: Boolean(idle) } : session,
  );
}

function redirectToLogin() {
  const next = window.location.pathname + window.location.search;
  window.location.assign(`/login?next=${encodeURIComponent(next)}`);
}


async function migrateLegacyIdleSessionIds() {
  if (state.legacyIdleMigrationAttempted) {
    return;
  }
  state.legacyIdleMigrationAttempted = true;

  const legacyIds = readLegacyIdleSessionIds();
  if (legacyIds.length === 0) {
    return;
  }

  try {
    const restoredIds = [];
    for (const sessionId of legacyIds) {
      const session = state.sessions.find((item) => item.id === sessionId);
      if (!session || session.idle) {
        continue;
      }
      const updated = await requestJson(`/api/terminal/sessions/${encodeURIComponent(sessionId)}/idle`, {
        method: "PUT",
      });
      updateSessionIdleState(updated.id || sessionId, true);
      restoredIds.push(sessionId);
    }

    if (restoredIds.length > 0) {
      try {
        window.localStorage.removeItem(LEGACY_IDLE_SESSION_STORAGE_KEY);
      } catch {
        // Ignore storage errors; server state is the source of truth now.
      }
    }
  } catch {
    // Keep loading even if the legacy browser state cannot be migrated.
  }
}

function markSessionOpenedLocally(sessionId) {
  if (!sessionId) {
    return;
  }

  const openedAt = Date.now();
  state.sessions = sortSessionsByRecentActivity(
    state.sessions.map((session) => {
      if (session.id !== sessionId) {
        return session;
      }

      const viewedSession = sharedSessionAfterOutputViewed(session);
      return {
        ...viewedSession,
        last_opened_at: Math.max(
          openedAt,
          sessionTimestamp(session, "last_opened_at", "lastOpenedAt"),
          sessionTimestamp(session, "created_at", "createdAt"),
        ),
      };
    }),
  );
}

function sessionActivityState(session) {
  return sharedSessionActivityState(session);
}

function sessionActivityText(session) {
  return sharedSessionActivityText(session);
}

function playTerminalCompletionSound() {
  try {
    const audio = new Audio(TERMINAL_COMPLETION_BELL_URL);
    audio.preload = "auto";
    audio.volume = 0.72;
    const playResult = audio.play();
    if (playResult && typeof playResult.catch === "function") {
      playResult.catch(() => {
        // Browser autoplay policy can block this until a user gesture.
      });
    }
  } catch {
    // Notification audio must not affect terminal operation.
  }
}

function maybePlayTerminalCompletionSound(sessions) {
  let shouldPlay = false;
  const currentSessionIds = new Set();
  (Array.isArray(sessions) ? sessions : []).forEach((session) => {
    const sessionId = String(session?.id || "").trim();
    if (!sessionId) {
      return;
    }
    currentSessionIds.add(sessionId);
    const stateValue = sessionActivityState(session);
    const previousState = state.completionSoundPreviousStates.get(sessionId) || "";
    if (state.completionSoundBaselineReady && previousState === "working" && stateValue === "completed") {
      shouldPlay = true;
    }
    state.completionSoundPreviousStates.set(sessionId, stateValue);
  });
  Array.from(state.completionSoundPreviousStates.keys()).forEach((sessionId) => {
    if (!currentSessionIds.has(sessionId)) {
      state.completionSoundPreviousStates.delete(sessionId);
    }
  });

  if (!state.completionSoundBaselineReady) {
    state.completionSoundBaselineReady = true;
    return;
  }

  if (!state.terminalCompletionBellEnabled) {
    return;
  }

  if (shouldPlay) {
    playTerminalCompletionSound();
  }
}

function sessionActivityLabel(session) {
  return sharedSessionActivityLabel(session);
}

function sessionActivityAgentLabel(session) {
  return sharedSessionActivityAgentLabel(session);
}

function sessionActivityAgentPrefix(session) {
  return sharedSessionActivityAgentPrefix(session, state.terminalActivityAgentDisplay);
}

function sessionActivityAgentSuffix(session) {
  return sharedSessionActivityAgentSuffix(session, state.terminalActivityAgentDisplay);
}

function isSessionBusy(session) {
  return sharedIsSessionBusy(session);
}

function sessionActivityPrefix(session) {
  return sharedTerminalSessionActivityPrefix(session);
}

function sessionErrorContinueKey(session) {
  return sharedSessionErrorContinueKey(session);
}

function isSessionErrorState(session) {
  return sharedIsSessionErrorState(session);
}

function isSessionDropdownInteracting() {
  return sessionDropdownInteracting;
}

function shouldDeferSessionListRender() {
  return isSessionDropdownInteracting();
}

function restartPendingSessionEventRefresh() {
  if (sessionEventRefreshTimer === null) {
    return;
  }

  window.clearTimeout(sessionEventRefreshTimer);
  sessionEventRefreshTimer = null;
  scheduleSessionEventRefresh({}, TERMINAL_SESSION_SELECT_INTERACTION_FLUSH_MS);
}

function scheduleSessionDropdownInteractionFlush(callback) {
  if (sessionDropdownFlushTimer !== null) {
    window.clearTimeout(sessionDropdownFlushTimer);
  }

  sessionDropdownFlushTimer = window.setTimeout(() => {
    sessionDropdownFlushTimer = null;
    window.requestAnimationFrame(callback);
  }, TERMINAL_SESSION_SELECT_INTERACTION_FLUSH_MS);
}

function scheduleSessionActivityRefresh(delayMs = TERMINAL_SESSION_ACTIVITY_REFRESH_MS) {
  if (sessionActivityRefreshTimer !== null) {
    return;
  }

  sessionActivityRefreshTimer = window.setTimeout(() => {
    sessionActivityRefreshTimer = null;
    if (document.visibilityState === "hidden") {
      scheduleSessionActivityRefresh();
      return;
    }

    if (isSessionDropdownInteracting()) {
      sessionActivityRefreshPending = true;
      scheduleSessionActivityRefresh(TERMINAL_SESSION_ACTIVITY_INTERACTION_RETRY_MS);
      return;
    }

    if (state.loadingSessions) {
      sessionActivityRefreshPending = true;
      scheduleSessionActivityRefresh(TERMINAL_SESSION_ACTIVITY_INTERACTION_RETRY_MS);
      return;
    }

    sessionActivityRefreshPending = false;
    loadSessions({
      preferredSessionId: state.activeSessionId,
      preserveCurrentList: true,
    }).finally(() => {
      scheduleSessionActivityRefresh();
    });
  }, delayMs);
}

function flushSessionActivityRenderAfterInteraction() {
  sessionDropdownInteracting = false;
  scheduleSessionDropdownInteractionFlush(() => {
    if (state.pendingSessionRefresh || sessionEventRefreshTimer !== null) {
      scheduleSessionEventRefresh({}, 0);
    }
  });
  if (sessionActivityRefreshPending) {
    if (sessionActivityRefreshTimer !== null) {
      window.clearTimeout(sessionActivityRefreshTimer);
      sessionActivityRefreshTimer = null;
    }
    scheduleSessionActivityRefresh(120);
  }
}

function markSessionDropdownInteracting() {
  if (sessionDropdownFlushTimer !== null) {
    window.clearTimeout(sessionDropdownFlushTimer);
    sessionDropdownFlushTimer = null;
  }
  sessionDropdownInteracting = true;
  restartPendingSessionEventRefresh();
}

function announceSessionMutation(action, session = {}) {
  sharedAnnounceSessionMutation(action, session, state.currentPath);
}

function updateStatus(message, tone) {
  if (!statusEl) {
    return;
  }

  if (statusDismissTimer !== null) {
    window.clearTimeout(statusDismissTimer);
    statusDismissTimer = null;
  }

  const nextMessage = typeof message === "string" ? message : String(message || "");
  statusEl.hidden = !nextMessage;
  statusEl.textContent = nextMessage;
  statusEl.dataset.tone = tone || "info";

  if (!nextMessage) {
    return;
  }

  statusDismissTimer = window.setTimeout(() => {
    statusEl.hidden = true;
    statusEl.textContent = "";
    statusDismissTimer = null;
  }, STATUS_AUTO_HIDE_DELAY_MS);
}

function updateTerminalInputHistoryStatus(message, tone) {
  if (!terminalInputHistoryStatusEl) {
    updateStatus(message, tone);
    return;
  }

  if (terminalInputHistoryStatusDismissTimer !== null) {
    window.clearTimeout(terminalInputHistoryStatusDismissTimer);
    terminalInputHistoryStatusDismissTimer = null;
  }

  const nextMessage = typeof message === "string" ? message : String(message || "");
  terminalInputHistoryStatusEl.hidden = !nextMessage;
  terminalInputHistoryStatusEl.textContent = nextMessage;
  terminalInputHistoryStatusEl.dataset.tone = tone || "info";

  if (!nextMessage) {
    return;
  }

  terminalInputHistoryStatusDismissTimer = window.setTimeout(() => {
    terminalInputHistoryStatusEl.hidden = true;
    terminalInputHistoryStatusEl.textContent = "";
    terminalInputHistoryStatusDismissTimer = null;
  }, STATUS_AUTO_HIDE_DELAY_MS);
}

function updateTerminalAgentsDocStatus(message, tone) {
  if (!terminalAgentsDocStatusEl) {
    updateStatus(message, tone);
    return;
  }

  if (terminalAgentsDocStatusDismissTimer !== null) {
    window.clearTimeout(terminalAgentsDocStatusDismissTimer);
    terminalAgentsDocStatusDismissTimer = null;
  }

  const nextMessage = typeof message === "string" ? message : String(message || "");
  terminalAgentsDocStatusEl.hidden = !nextMessage;
  terminalAgentsDocStatusEl.textContent = nextMessage;
  terminalAgentsDocStatusEl.dataset.tone = tone || "info";

  if (!nextMessage) {
    return;
  }

  terminalAgentsDocStatusDismissTimer = window.setTimeout(() => {
    terminalAgentsDocStatusEl.hidden = true;
    terminalAgentsDocStatusEl.textContent = "";
    terminalAgentsDocStatusDismissTimer = null;
  }, STATUS_AUTO_HIDE_DELAY_MS);
}

function updateSessionStatus(message, tone) {
  if (!message) {
    return;
  }

  updateStatus(message, tone);
}

function clearConnectingStatusForSession(sessionId) {
  if (!statusEl || statusEl.hidden || !String(statusEl.textContent || "").startsWith("正在连接")) {
    return;
  }

  const session = state.sessions.find((item) => item.id === sessionId) || activeSession();
  const sessionName = String(session?.name || "").trim();
  const statusText = String(statusEl.textContent || "");
  if (sessionName && !statusText.includes(sessionName)) {
    return;
  }

  updateStatus("", "ok");
}

function isTerminalConnected() {
  return terminalContextSocketOpen(activeTerminalContext);
}

function ensureTerminalReadyForInput() {
  if (terminalSessionInitializing()) {
    updateStatus("终端正在初始化，请稍后输入。", "info");
    return false;
  }

  if (!state.activeSessionId) {
    updateStatus("请先选择一个终端会话。", "warn");
    return false;
  }

  if (!isTerminalConnected()) {
    updateStatus("终端尚未连接，请稍后再试。", "warn");
    return false;
  }

  return true;
}

syncTerminalImePolicy();

installTerminalProgrammaticFocusGate();
installTerminalDialogFocusGate();
document.addEventListener("pointerdown", guardTerminalActionEditableFocus, true);
document.addEventListener("click", guardTerminalActionEditableFocus, true);
document.addEventListener("focusin", rejectTerminalActionEditableFocus, true);

// Global guard: when the soft keyboard is the active input method and the
// system IME is not explicitly enabled, blur the xterm helper textarea the
// instant it receives focus. This prevents the mobile system keyboard from
// popping up due to focus leaking from soft keyboard interactions (e.g. after
// a <select> change event on iOS/Android where focus may transfer to the
// previously focused helper textarea).
document.addEventListener("focusin", (event) => {
  if (!terminalSoftKeyboardVisible() || terminalSystemImeEnabled) {
    return;
  }
  const helper = terminalHelperTextarea();
  if (helper && event.target === helper) {
    helper.blur();
  }
}, true);

function refreshSessionsAfterPageResume() {
  if (document.visibilityState === "hidden" || terminalSessionInitializing()) {
    return;
  }

  refreshTerminalInputVisibilityAfterPageResume();
  scheduleTerminalSizeSettle();
  loadSessions({
    preferredSessionId: state.activeSessionId || currentLocationSessionId(),
    preserveCurrentList: true,
  });
  if (state.activeSessionId && !isTerminalConnected()) {
    scheduleReconnect();
  }
}

window.requestAnimationFrame(() => {
  syncTerminalOverlayBounds();
  updateTerminalScrollBottomButton();
  syncTerminalCursorCorrection();
});

function registerTerminalInstanceEventHandlers(context = activeTerminalContext) {
  const instance = context?.term || term;
  const disposables = context?.eventDisposables || terminalInstanceEventDisposables;
  if (typeof instance?.onScroll === "function") {
    disposables.push(
      instance.onScroll(() => {
        if (context !== activeTerminalContext) {
          return;
        }
        updateTerminalScrollBottomButton();
        scheduleTerminalCursorCorrection();
      }),
    );
  }

  if (typeof instance?.onRender === "function") {
    disposables.push(
      instance.onRender(() => {
        if (context !== activeTerminalContext) {
          return;
        }
        scheduleTerminalCursorCorrection();
      }),
    );
  }

  disposables.push(
    instance.onData((data) => {
      if (context !== activeTerminalContext) {
        return;
      }
      if (terminalSessionInitializing()) {
        updateStatus("终端正在初始化，请稍后输入。", "info");
        return;
      }

      const mouseFiltered = filterTerminalMouseInput(data);
      const filtered = filterTerminalAutoResponse(mouseFiltered);
      if (!filtered) {
        return;
      }
      if (maybeHandleNewSessionQuickStartInput(filtered)) {
        return;
      }
      sendTerminalInput(filtered);
    }),
  );

  if (typeof instance?.onSelectionChange === "function") {
    disposables.push(
      instance.onSelectionChange(() => {
        if (context !== activeTerminalContext) {
          return;
        }
        syncTerminalSelectionControls();
        // Cursor correction is suppressed while a selection is active (see
        // `terminalSelectionBlockingCursorCorrection`).  Once the selection is
        // cleared, re-evaluate so the corrected cursor state settles on the next
        // frame instead of staying frozen.
        if (!terminalSelectionBlockingCursorCorrection()) {
          scheduleTerminalCursorCorrection();
        }
      }),
    );
  }
  if (context) {
    context.eventDisposables = disposables;
  }
}

document.addEventListener("paste", handleTerminalPasteEvent, true);
document.addEventListener("keydown", handleTerminalClipboardShortcut, true);
document.addEventListener("keydown", handleTerminalFunctionShortcut, true);
document.addEventListener("selectstart", preventNativeTerminalTouchSelection, { capture: true });

terminalHost.addEventListener("click", focusTerminalFromTerminalTap);
terminalHost.addEventListener("touchstart", rememberTerminalTouchScrollGesture, { passive: true });
terminalHost.addEventListener("touchstart", rememberTerminalTouchSelectionCandidate, { passive: true });
terminalHost.addEventListener("touchend", (event) => {
  for (const touch of terminalTouchItems(event.changedTouches)) {
    clearTerminalTouchScrollGesture(touch.identifier);
    clearTerminalTouchSelectionCandidate(touch.identifier);
  }
});
terminalHost.addEventListener("touchcancel", (event) => {
  for (const touch of terminalTouchItems(event.changedTouches)) {
    clearTerminalTouchScrollGesture(touch.identifier);
    clearTerminalTouchSelectionCandidate(touch.identifier);
  }
});
terminalHost.addEventListener("contextmenu", handleTerminalContextMenuSelection);
document.addEventListener("pointerdown", closeTerminalContextMenuFromOutside, true);
document.addEventListener("keydown", handleTerminalContextMenuKeydown, true);
window.addEventListener("blur", closeTerminalContextMenu);
window.addEventListener("resize", closeTerminalContextMenu);
window.addEventListener("scroll", closeTerminalContextMenu, true);
window.addEventListener("touchmove", handleTerminalTouchSelectionMove, { passive: false, capture: true });
window.addEventListener("touchmove", cancelSessionPageScrollRestoreForUserScrollIntent, {
  passive: true,
});
window.addEventListener("wheel", cancelSessionPageScrollRestoreForUserScrollIntent, {
  passive: true,
});
window.addEventListener("touchend", handleTerminalTouchSelectionEnd, { passive: false, capture: true });
window.addEventListener("touchcancel", handleTerminalTouchSelectionEnd, { passive: false, capture: true });

if (terminalSelectionStartHandle) {
  terminalSelectionStartHandle.addEventListener("pointerdown", (event) => {
    startTerminalSelectionHandleDrag(event, "start");
  });
}

if (terminalSelectionEndHandle) {
  terminalSelectionEndHandle.addEventListener("pointerdown", (event) => {
    startTerminalSelectionHandleDrag(event, "end");
  });
}

window.addEventListener("pointermove", handleTerminalSelectionHandleMove, { passive: false, capture: true });
window.addEventListener("pointerup", stopTerminalSelectionHandleDrag, { passive: false, capture: true });
window.addEventListener("pointercancel", stopTerminalSelectionHandleDrag, { passive: false, capture: true });

if (mobileKeysEl) {
  mobileKeysEl.querySelectorAll(MOBILE_KEY_BUTTON_SELECTOR).forEach(prepareMobileKeyControl);
  mobileKeysEl.addEventListener("keydown", handleMobileKeyKeyboardEvent, true);
  mobileKeysEl.addEventListener("keyup", handleMobileKeyKeyboardEvent, true);
  mobileKeysEl.addEventListener("beforeinput", handleMobileKeyKeyboardEvent, true);
  mobileKeysEl.addEventListener("click", handleMobileKeyClick, true);
  mobileKeysEl.addEventListener("pointerup", restoreSystemImeAfterSoftKeyboardControl, true);
  mobileKeysEl.addEventListener("pointercancel", restoreSystemImeAfterSoftKeyboardControl, true);
  mobileKeysEl.addEventListener("click", restoreSystemImeAfterSoftKeyboardControl, true);
  // Block implicit focus from touch so the system keyboard does not appear
  // when tapping soft keyboard buttons (especially iOS Safari, where
  // pointerdown's preventDefault does not stop focus).
  mobileKeysEl.addEventListener("touchstart", handleMobileKeyTouchStart, { passive: true });
  // Safety net: if focus still reaches an editable element inside the mobile
  // keys area, blur it immediately while in soft keyboard mode.
  mobileKeysEl.addEventListener("focusin", handleMobileKeyFocusIn, true);
  if (window.PointerEvent) {
    mobileKeysEl.addEventListener("pointerdown", handleMobileKeyPointerDown);
    window.addEventListener("pointermove", handleMobileKeyPointerMove, { passive: true });
    window.addEventListener("pointerup", handleMobileKeyPointerEnd);
    window.addEventListener("pointercancel", handleMobileKeyPointerEnd);
  }
}

[
  terminalNumberMenuEl,
  terminalSlashCommandMenuEl,
  terminalFunctionCommandMenuEl,
  terminalProjectCommandMenuEl,
  terminalToolsMenuEl,
  terminalToolMenuEl,
  terminalCommandCollectionsMenuEl,
].forEach((surface) => {
  if (!surface) {
    return;
  }
  surface.querySelectorAll("button").forEach(prepareMobileKeyControl);
  surface.addEventListener("pointerdown", suppressSystemImeForSoftKeyboardControl, true);
  surface.addEventListener("pointerup", restoreSystemImeAfterSoftKeyboardControl, true);
  surface.addEventListener("pointercancel", restoreSystemImeAfterSoftKeyboardControl, true);
  surface.addEventListener("click", restoreSystemImeAfterSoftKeyboardControl, true);
  surface.addEventListener("focusin", handleMobileKeyFocusIn, true);
});
terminalTouchCopyCheckboxEl?.setAttribute("tabindex", "-1");

if (terminalFunctionCommandButtonEl) {
  terminalFunctionCommandButtonEl.addEventListener("click", toggleTerminalFunctionCommandMenu);
}

if (terminalImageUploadInputEl) {
  terminalImageUploadInputEl.addEventListener("change", handleTerminalImageUploadSelection);
}

if (terminalSystemKeyboardCheckboxEl) {
  terminalSystemKeyboardCheckboxEl.addEventListener("change", handleTerminalSystemKeyboardCheckboxChange);
}

if (terminalTouchCopyCheckboxEl) {
  terminalTouchCopyCheckboxEl.addEventListener("change", handleTerminalTouchCopyCheckboxChange);
}

if (terminalProjectCommandSelectEl) {
  terminalProjectCommandSelectEl.addEventListener("change", handleTerminalProjectCommandSelectChange);
}

if (terminalProjectCommandButtonEl) {
  terminalProjectCommandButtonEl.addEventListener("click", toggleTerminalProjectCommandMenu);
}

if (terminalProjectCommandMenuEl) {
  terminalProjectCommandMenuEl.addEventListener("click", handleTerminalProjectCommandMenuClick);
}

if (terminalSlashCommandButtonEl) {
  terminalSlashCommandButtonEl.addEventListener("click", toggleTerminalSlashCommandMenu);
}

if (terminalSlashCommandMenuEl) {
  terminalSlashCommandMenuEl.addEventListener("click", handleTerminalSlashCommandMenuClick);
}

if (terminalNumberButtonEl) {
  terminalNumberButtonEl.addEventListener("click", toggleTerminalNumberMenu);
}

if (terminalNumberMenuEl) {
  terminalNumberMenuEl.addEventListener("click", handleTerminalNumberMenuClick);
}

if (terminalCommandCollectionsBtnEl) {
  terminalCommandCollectionsBtnEl.addEventListener("click", toggleTerminalCommandCollectionsMenu);
}

if (terminalCommandCollectionsBodyEl) {
  terminalCommandCollectionsBodyEl.addEventListener("click", handleTerminalCommandCollectionsBodyClick);
}

if (terminalCommandCollectionsMenuEl && terminalCommandCollectionsBtnEl) {
  document.addEventListener("pointerdown", (event) => {
    if (
      terminalCommandCollectionsMenuEl.hidden
      || terminalCommandCollectionsMenuEl.contains(event.target)
      || terminalCommandCollectionsBtnEl.contains(event.target)
    ) {
      return;
    }
    closeTerminalCommandCollectionsMenu();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || terminalCommandCollectionsMenuEl.hidden) {
      return;
    }
    event.preventDefault();
    closeTerminalCommandCollectionsMenu();
    terminalCommandCollectionsBtnEl.focus({ preventScroll: true });
  });
  window.addEventListener("resize", positionTerminalCommandCollectionsMenu, { passive: true });
  window.addEventListener("scroll", positionTerminalCommandCollectionsMenu, {
    capture: true,
    passive: true,
  });
}

terminalToolMenuBodyEl?.addEventListener("click", handleTerminalToolMenuClick);
terminalToolMenuBackEl?.addEventListener("click", navigateTerminalToolMenuBack);
bindTerminalSpecifiedTaskDialog();

if (terminalQuotaRefreshBtnEl) {
  terminalQuotaRefreshBtnEl.addEventListener("click", () => {
    refreshQuotaBySelectedKey();
  });
}

if (terminalQuotaKeySelectEl) {
  terminalQuotaKeySelectEl.addEventListener("change", () => {
    refreshQuotaBySelectedKey();
  });
}

if (terminalQuotaSettingsBtnEl) {
  terminalQuotaSettingsBtnEl.addEventListener("click", () => {
    toggleTerminalQuotaSettingsPanel();
  });
}

if (terminalQuotaPresetSelectEl) {
  terminalQuotaPresetSelectEl.addEventListener("change", () => {
    onQuotaPresetSelectChange();
  });
}

if (terminalQuotaCloseBtnEl) {
  terminalQuotaCloseBtnEl.addEventListener("click", () => {
    closeTerminalQuotaDialog();
  });
}

if (terminalQuotaSaveConfigBtnEl) {
  terminalQuotaSaveConfigBtnEl.addEventListener("click", () => {
    saveTerminalQuotaConfig();
  });
}

if (terminalQuotaDialogEl) {
  terminalQuotaDialogEl.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeTerminalQuotaDialog();
  });
  terminalQuotaDialogEl.addEventListener("close", () => {
    focusTerminalSoon();
  });
}

if (pasteClipboardButton) {
  pasteClipboardButton.addEventListener("click", () => {
    pasteFromClipboard();
  });
}

if (terminalSelectionCopyButton) {
  terminalSelectionCopyButton.addEventListener("pointerdown", (event) => {
    event.preventDefault();
  });
  terminalSelectionCopyButton.addEventListener("click", async () => {
    const selection = typeof term.getSelection === "function" ? term.getSelection() : "";
    if (!selection) {
      syncTerminalSelectionControls();
      updateStatus("当前没有可复制的选中内容。", "muted");
      return;
    }

    try {
      const copied = await copyTextToClipboard(selection);
      if (!copied) {
        throw new Error("复制失败");
      }
      if (typeof term.clearSelection === "function") {
        term.clearSelection();
      }
      syncTerminalSelectionControls();
      updateStatus("已复制选中内容。", "ok");
      focusTerminalIfAllowed();
    } catch (error) {
      updateStatus(error?.message || "复制失败。", "warn");
    }
  });
}

async function copyTerminalAllText() {
  const text = readTerminalAllText();
  closeTerminalContextMenu();
  if (!text) {
    updateStatus("当前终端没有可复制的文本。", "muted");
    focusTerminalIfAllowed();
    return;
  }

  const copied = await copyTextToClipboard(text);
  updateStatus(
    copied ? "已复制终端全部文本。" : "复制终端全部文本失败，请手动复制。",
    copied ? "ok" : "warn",
  );
  if (copied) {
    focusTerminalIfAllowed();
  }
}

if (terminalContextCopyAllButton) {
  terminalContextCopyAllButton.addEventListener("click", copyTerminalAllText);
}

if (scrollPageTopButton) {
  preventPointerFocus(scrollPageTopButton);
  scrollPageTopButton.addEventListener("click", () => {
    scrollPageToTop();
    focusTerminalAfterTransientControl();
  });
}

if (scrollTerminalBottomButton) {
  preserveSystemImeStateForControl(scrollTerminalBottomButton);
  scrollTerminalBottomButton.addEventListener("click", () => {
    scrollTerminalToBottom();
  });
}

if (scrollTerminalTopButton) {
  preventPointerFocus(scrollTerminalTopButton);
  scrollTerminalTopButton.addEventListener("click", () => {
    cancelTerminalBottomAnchor();
    scrollTerminalToTop();
  });
}

if (terminalInputHistoryButton) {
  preventPointerFocus(terminalInputHistoryButton);
  terminalInputHistoryButton.addEventListener("click", () => {
    showTerminalInputHistory();
  });
}

if (terminalSoftKeyboardToggleButton) {
  preventPointerFocus(terminalSoftKeyboardToggleButton);
  terminalSoftKeyboardToggleButton.addEventListener("click", () => {
    toggleTerminalSoftKeyboard();
    focusTerminalAfterTransientControl();
  });
}

if (terminalScheduleButton) {
  preventPointerFocus(terminalScheduleButton);
  terminalScheduleButton.addEventListener("click", () => {
    openScheduledTerminalPasteDialog();
  });
}

if (terminalInputHistoryCloseButton) {
  terminalInputHistoryCloseButton.addEventListener("click", () => {
    closeTerminalInputHistoryDialog();
  });
}

if (terminalInputHistoryDialogEl) {
  terminalInputHistoryDialogEl.addEventListener("close", () => {
    updateTerminalInputHistoryStatus("", "info");
    restoreTerminalFocusAfterDialogClose();
  });
}

if (terminalPasteDialogEl) {
  terminalPasteDialogEl.addEventListener("close", () => {
    restoreTerminalFocusAfterDialogClose();
  });
}

if (terminalAgentsDocFormEl) {
  terminalAgentsDocFormEl.addEventListener("submit", (event) => {
    event.preventDefault();
    saveTerminalAgentsDoc();
  });
}

if (terminalAgentsDocSelectEl) {
  terminalAgentsDocSelectEl.addEventListener("change", () => {
    const sessionId = terminalAgentsDocSessionId || state.activeSessionId;
    if (!sessionId) {
      updateTerminalAgentsDocStatus("请先选择一个终端会话。", "warn");
      return;
    }
    setTerminalAgentsDocBusy(true);
    loadTerminalAgentsDoc(sessionId, terminalAgentsDocPathValue())
      .catch((error) => {
        updateTerminalAgentsDocStatus(error?.message || "读取文档失败。", "warn");
      })
      .finally(() => {
        setTerminalAgentsDocBusy(false);
      });
  });
}

if (terminalAgentsDocCloseButton) {
  terminalAgentsDocCloseButton.addEventListener("click", () => {
    closeTerminalAgentsDocDialog();
  });
}

if (terminalAgentsDocCreateButton) {
  preventPointerFocus(terminalAgentsDocCreateButton);
  terminalAgentsDocCreateButton.addEventListener("click", () => {
    handleCreateTerminalAgentsDoc();
  });
}

if (terminalAgentsDocRefreshButton) {
  preventPointerFocus(terminalAgentsDocRefreshButton);
  terminalAgentsDocRefreshButton.addEventListener("click", () => {
    handleRefreshTerminalAgentsDocList();
  });
}

if (terminalAgentsDocNameInputEl) {
  terminalAgentsDocNameInputEl.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      handleCreateTerminalAgentsDoc();
    }
  });
}

if (terminalAgentsDocFilterInputEl) {
  terminalAgentsDocFilterInputEl.addEventListener("input", () => {
    renderFilteredTerminalAgentsDocOptions();
  });
}

if (terminalAgentsDocMaxAgeDaysEl) {
  terminalAgentsDocMaxAgeDaysEl.addEventListener("input", () => {
    renderFilteredTerminalAgentsDocOptions();
  });
}

if (terminalAgentsDocRecursiveDirectoriesEl) {
  terminalAgentsDocRecursiveDirectoriesEl.addEventListener("change", () => {
    handleRefreshTerminalAgentsDocList();
  });
  terminalAgentsDocRecursiveDirectoriesEl.addEventListener("keydown", (event) => {
    if (event.key === "Enter") {
      event.preventDefault();
      terminalAgentsDocRecursiveDirectoriesEl.blur();
    }
  });
}

if (terminalAgentsDocShowHiddenEl) {
  terminalAgentsDocShowHiddenEl.addEventListener("change", () => {
    handleRefreshTerminalAgentsDocList();
  });
}

if (terminalAgentsDocDialogEl) {
  terminalAgentsDocDialogEl.addEventListener("close", () => {
    updateTerminalAgentsDocStatus("", "info");
    terminalAgentsDocSessionId = "";
    terminalAgentsDocAllDocuments = [];
    if (terminalAgentsDocFilterInputEl) {
      terminalAgentsDocFilterInputEl.value = "";
    }
    if (terminalAgentsDocMaxAgeDaysEl) {
      terminalAgentsDocMaxAgeDaysEl.value = "";
    }
    if (terminalAgentsDocRecursiveDirectoriesEl) {
      terminalAgentsDocRecursiveDirectoriesEl.value = "docs";
    }
    if (terminalAgentsDocShowHiddenEl) {
      terminalAgentsDocShowHiddenEl.checked = false;
    }
    restoreTerminalFocusAfterDialogClose();
  });
}

if (terminalInputHistoryCopyButton) {
  terminalInputHistoryCopyButton.addEventListener("click", () => {
    copyTerminalInputHistory();
  });
}

if (pageScrollRailEl) {
  pageScrollRailEl.addEventListener("pointerdown", handlePageScrollRailPointerDown);
  pageScrollRailEl.addEventListener("pointermove", handlePageScrollRailPointerMove);
  pageScrollRailEl.addEventListener("pointerup", (event) => {
    clearPageScrollRailDrag(event.pointerId);
  });
  pageScrollRailEl.addEventListener("pointercancel", (event) => {
    clearPageScrollRailDrag(event.pointerId);
  });
  pageScrollRailEl.addEventListener("lostpointercapture", () => {
    clearPageScrollRailDrag();
  });
  pageScrollRailEl.addEventListener("wheel", handlePageScrollRailWheel, { passive: false });
  pageScrollRailEl.addEventListener("keydown", handlePageScrollRailKeydown);
}

if (terminalNavScrollEl) {
  terminalNavScrollEl.addEventListener("pointerdown", handleTerminalNavScrollPointerDown);
  terminalNavScrollEl.addEventListener("pointermove", handleTerminalNavScrollPointerMove);
  terminalNavScrollEl.addEventListener("pointerup", (event) => {
    clearTerminalNavScrollDrag(event.pointerId);
  });
  terminalNavScrollEl.addEventListener("pointercancel", (event) => {
    clearTerminalNavScrollDrag(event.pointerId);
  });
  terminalNavScrollEl.addEventListener("lostpointercapture", () => {
    clearTerminalNavScrollDrag();
  });
}

if (navigateBackButton) {
  navigateBackButton.addEventListener("click", () => {
    navigateHistory(-1);
  });
}

if (terminalWorkflowsButton) {
  terminalWorkflowsButton.addEventListener("click", () => {
    ensureTerminalToolsMenuActionsBound();
    setTerminalToolsMenuExpanded(true);
    openTerminalToolMenu("tools", terminalWorkflowsButton);
  });
}

if (navigateForwardButton) {
  navigateForwardButton.addEventListener("click", () => {
    navigateHistory(1);
  });
}

if (sessionDetailToggleEl) {
  sessionDetailToggleEl.checked = state.showSessionDetails;
  sessionDetailToggleEl.addEventListener("change", () => {
    state.showSessionDetails = sessionDetailToggleEl.checked;
    storeTerminalSessionDetails(state.showSessionDetails);
    renderSessions();
  });
}

if (sessionAgentToggleEl) {
  sessionAgentToggleEl.checked = state.showSessionAgent;
  sessionAgentToggleEl.addEventListener("change", () => {
    state.showSessionAgent = sessionAgentToggleEl.checked;
    storeTerminalSessionAgent(state.showSessionAgent);
    renderSessions();
  });
}

if (sessionAutoContinueToggleEl) {
  sessionAutoContinueToggleEl.checked = state.autoContinueOnError;
  sessionAutoContinueToggleEl.addEventListener("change", () => {
    setAutoContinueOnError(sessionAutoContinueToggleEl.checked);
  });
}

if (terminalCodexFullAccessToggleEl) {
  terminalCodexFullAccessToggleEl.addEventListener("change", toggleCodexFullAccess);
}

if (terminalInterruptResumeButtonEl) {
  terminalInterruptResumeButtonEl.addEventListener("click", forceInterruptAndResumeTerminalAgent);
}

if (terminalToolsMenuEl && terminalToolsButtonEl) {
  document.addEventListener("pointerdown", (event) => {
    if (
      terminalToolsMenuEl.hidden
      || terminalToolsMenuEl.contains(event.target)
      || terminalToolsButtonEl.contains(event.target)
    ) {
      return;
    }
    closeTerminalToolsMenu();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || terminalToolsMenuEl.hidden) {
      return;
    }
    event.preventDefault();
    closeTerminalToolsMenu({ restoreFocus: true });
  });
  window.addEventListener("resize", positionTerminalToolsMenu, { passive: true });
  window.addEventListener("scroll", positionTerminalToolsMenu, {
    capture: true,
    passive: true,
  });
}

if (terminalFunctionCommandMenuEl && terminalFunctionCommandButtonEl) {
  document.addEventListener("pointerdown", (event) => {
    if (
      terminalFunctionCommandMenuEl.hidden
      || terminalFunctionCommandMenuEl.contains(event.target)
      || terminalFunctionCommandButtonEl.contains(event.target)
    ) {
      return;
    }
    closeTerminalFunctionCommandMenu();
  }, true);
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || terminalFunctionCommandMenuEl.hidden) {
      return;
    }
    event.preventDefault();
    closeTerminalFunctionCommandMenu();
  }, true);
  window.addEventListener("resize", positionTerminalFunctionCommandMenu, { passive: true });
  window.addEventListener("scroll", positionTerminalFunctionCommandMenu, {
    capture: true,
    passive: true,
  });
  window.addEventListener("blur", closeTerminalFunctionCommandMenu);
}

if (terminalSlashCommandMenuEl && terminalSlashCommandButtonEl) {
  document.addEventListener("pointerdown", (event) => {
    if (
      terminalSlashCommandMenuEl.hidden
      || terminalSlashCommandMenuEl.contains(event.target)
      || terminalSlashCommandButtonEl.contains(event.target)
    ) {
      return;
    }
    closeTerminalSlashCommandMenu();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || terminalSlashCommandMenuEl.hidden) {
      return;
    }
    event.preventDefault();
    closeTerminalSlashCommandMenu({ restoreFocus: true });
  });
  window.addEventListener("resize", positionTerminalSlashCommandMenu, { passive: true });
  window.addEventListener("scroll", positionTerminalSlashCommandMenu, {
    capture: true,
    passive: true,
  });
}

if (terminalNumberMenuEl && terminalNumberButtonEl) {
  document.addEventListener("pointerdown", (event) => {
    if (
      terminalNumberMenuEl.hidden
      || terminalNumberMenuEl.contains(event.target)
      || terminalNumberButtonEl.contains(event.target)
    ) {
      return;
    }
    closeTerminalNumberMenu();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || terminalNumberMenuEl.hidden) {
      return;
    }
    event.preventDefault();
    closeTerminalNumberMenu({ restoreFocus: true });
  });
  window.addEventListener("resize", positionTerminalNumberMenu, { passive: true });
  window.addEventListener("scroll", positionTerminalNumberMenu, {
    capture: true,
    passive: true,
  });
}

if (terminalProjectCommandMenuEl && terminalProjectCommandButtonEl) {
  document.addEventListener("pointerdown", (event) => {
    if (
      terminalProjectCommandMenuEl.hidden
      || terminalProjectCommandMenuEl.contains(event.target)
      || terminalProjectCommandButtonEl.contains(event.target)
    ) {
      return;
    }
    closeTerminalProjectCommandMenu();
  });
  document.addEventListener("keydown", (event) => {
    if (event.key !== "Escape" || terminalProjectCommandMenuEl.hidden) {
      return;
    }
    event.preventDefault();
    closeTerminalProjectCommandMenu({ restoreFocus: true });
  });
  window.addEventListener("resize", positionTerminalProjectCommandMenu, { passive: true });
  window.addEventListener("scroll", positionTerminalProjectCommandMenu, {
    capture: true,
    passive: true,
  });
}

createSessionButton.addEventListener("click", () => {
  createSession({
    enableQuickStart: true,
  });
});

function bindSessionActivitySafeSelect(selectEl) {
  if (!selectEl) {
    return;
  }

  selectEl.addEventListener("pointerdown", markSessionDropdownInteracting);
  selectEl.addEventListener("mousedown", markSessionDropdownInteracting);
  selectEl.addEventListener("touchstart", markSessionDropdownInteracting, { passive: true });
  selectEl.addEventListener("focus", markSessionDropdownInteracting);
  selectEl.addEventListener("keydown", (event) => {
    if ([" ", "Enter", "ArrowDown", "ArrowUp"].includes(event.key)) {
      markSessionDropdownInteracting();
    }
    if (["Escape", "Enter"].includes(event.key)) {
      window.setTimeout(flushSessionActivityRenderAfterInteraction, 120);
    }
  });
  selectEl.addEventListener("blur", flushSessionActivityRenderAfterInteraction);
  selectEl.addEventListener("change", flushSessionActivityRenderAfterInteraction);
}

if (sessionSelectEl) {
  enhanceWorkspaceIconSelect(sessionSelectEl, () => state.terminalWorkspaceIconPath);
  bindSessionActivitySafeSelect(sessionSelectEl);
  sessionSelectEl.addEventListener("change", () => {
    if (!sessionSelectEl.value || sessionSelectEl.value === state.activeSessionId) {
      return;
    }

    closeSessionRenameEditor();
    selectSession(sessionSelectEl.value, { pushHistory: true });
  });
}

if (agentSessionSelectEl) {
  enhanceWorkspaceIconSelect(agentSessionSelectEl, () => state.terminalWorkspaceIconPath);
  bindSessionActivitySafeSelect(agentSessionSelectEl);
  agentSessionSelectEl.addEventListener("change", () => {
    if (!agentSessionSelectEl.value || agentSessionSelectEl.value === state.activeSessionId) {
      return;
    }

    closeSessionRenameEditor();
    selectSession(agentSessionSelectEl.value, { pushHistory: true });
  });
}

if (renameSessionButton) {
  renameSessionButton.addEventListener("click", () => {
    const current = activeSession();
    if (current) {
      startSessionRename(current, renameSessionButton);
    }
  });
}

if (deleteSessionButton) {
  deleteSessionButton.addEventListener("click", () => {
    const current = activeSession();
    if (current) {
      deleteSession(current);
    }
  });
}

if (idleSessionButton) {
  idleSessionButton.addEventListener("click", () => {
    idleCurrentSession();
  });
}

if (archiveResumeButton) {
  archiveResumeButton.addEventListener("click", () => {
    archiveCurrentAgentResume();
  });
}

if (idleSessionSelectEl) {
  bindSessionActivitySafeSelect(idleSessionSelectEl);
  idleSessionSelectEl.addEventListener("change", () => {
    const sessionId = idleSessionSelectEl.value;
    if (!sessionId) {
      return;
    }
    // 闲置下拉和活动下拉一致：选中即切换（恢复并连接）。
    restoreIdleSession(sessionId);
  });
}

if (terminalNavToggleButton) {
  preventPointerFocus(terminalNavToggleButton);
  terminalNavToggleButton.addEventListener("click", () => {
    const expanded = terminalNavToggleButton.getAttribute("aria-expanded") === "true";
    setTerminalPathExpanded(!expanded, { forceEnd: !expanded });
    focusTerminalAfterTransientControl();
  });
}

if (sessionRenameDialogEl) {
  sessionRenameDialogEl.addEventListener("cancel", (event) => {
    event.preventDefault();
    closeSessionRenameEditor();
  });
  sessionRenameDialogEl.addEventListener("click", (event) => {
    if (event.target === sessionRenameDialogEl) {
      closeSessionRenameEditor();
    }
  });
}

if (sessionRenamePresetsEl) {
  sessionRenamePresetsEl.addEventListener("click", (event) => {
    const button = event.target?.closest?.('[data-action="append-session-rename-preset"]');
    if (!button) {
      return;
    }
    appendSessionRenamePreset(button.dataset.preset || "");
  });
}

if (sessionRenameCancelButton) {
  sessionRenameCancelButton.addEventListener("click", () => {
    closeSessionRenameEditor();
  });
}

if (sessionRenameFormEl) {
  sessionRenameFormEl.addEventListener("submit", (event) => {
    event.preventDefault();
    renameSession();
  });
}

if (terminalPasteCancelButton) {
  terminalPasteCancelButton.addEventListener("click", () => {
    closeTerminalPasteDialog();
    focusTerminalAfterTransientControl();
  });
}

if (terminalPasteSubmitButton) {
  terminalPasteSubmitButton.addEventListener("click", () => {
    if (terminalPasteBusy) {
      return;
    }
    if (!submitTerminalPasteDialog()) {
      terminalPasteTextEl?.focus();
    }
  });
}

if (terminalPasteTextEl) {
  terminalPasteTextEl.addEventListener("paste", (event) => {
    const parts = dataTransferToPasteParts(event.clipboardData);
    if (!parts.some((part) => part.type === "images")) {
      return;
    }
    event.preventDefault();
    applyTerminalPasteParts(parts, { openDialog: false }).catch((error) => {
      updateStatus(error?.message || "处理剪贴板图片失败。", "warn");
      terminalPasteTextEl.focus();
    });
  });
}

if (terminalPasteFormEl) {
  terminalPasteFormEl.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (terminalPasteBusy) {
      return;
    }
    const schedulePanelOpen = terminalPasteScheduleEl && !terminalPasteScheduleEl.hidden;
    if (schedulePanelOpen) {
      const scheduled = await confirmTerminalPasteSchedule();
      if (scheduled && terminalPasteScheduleToggleEl) {
        terminalPasteScheduleToggleEl.textContent = "定时发送";
      }
      if (!scheduled) {
        terminalPasteTextEl?.focus();
      }
      return;
    }
    if (!submitTerminalPasteDialogAndSend()) {
      terminalPasteTextEl?.focus();
    }
  });
}

if (terminalPasteScheduleToggleEl) {
  terminalPasteScheduleToggleEl.addEventListener("click", () => {
    if (!terminalPasteScheduleEl) {
      return;
    }
    const willOpen = terminalPasteScheduleEl.hasAttribute("hidden");
    if (willOpen) {
      terminalPasteScheduleEl.hidden = false;
      const now = new Date();
      now.setSeconds(0, 0);
      now.setMinutes(now.getMinutes() + 5);
      const pad = (value) => String(value).padStart(2, "0");
      if (terminalPasteScheduleDatetimeEl) {
        terminalPasteScheduleDatetimeEl.min = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}T${pad(now.getHours())}:${pad(now.getMinutes())}`;
        terminalPasteScheduleDatetimeEl.value = `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}T${pad(now.getHours())}:${pad(now.getMinutes())}`;
      }
      terminalPasteScheduleToggleEl.textContent = "收起定时";
      terminalPasteScheduleDelayEl?.focus();
    } else {
      terminalPasteScheduleEl.hidden = true;
      terminalPasteScheduleToggleEl.textContent = "定时发送";
    }
  });
}

if (terminalPasteScheduleConfirmEl) {
  terminalPasteScheduleConfirmEl.addEventListener("click", async () => {
    if (terminalPasteBusy) {
      return;
    }
    const scheduled = await confirmTerminalPasteSchedule();
    if (scheduled && terminalPasteScheduleToggleEl) {
      terminalPasteScheduleToggleEl.textContent = "定时发送";
    }
  });
}

// Enter key inside schedule inputs confirms the scheduled send, so users
// don't have to tap the confirm button separately.
[terminalPasteScheduleDelayEl, terminalPasteScheduleDelayUnitEl, terminalPasteScheduleDatetimeEl].forEach((el) => {
  if (!el) {
    return;
  }
  el.addEventListener("keydown", async (event) => {
    if (event.key !== "Enter" || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey) {
      return;
    }
    event.preventDefault();
    if (terminalPasteBusy) {
      return;
    }
    const scheduled = await confirmTerminalPasteSchedule();
    if (scheduled && terminalPasteScheduleToggleEl) {
      terminalPasteScheduleToggleEl.textContent = "定时发送";
    }
  });
});

if (terminalPasteScheduleCancelEl) {
  terminalPasteScheduleCancelEl.addEventListener("click", () => {
    setTerminalPasteBusy(false);
    setTerminalPasteScheduleStatus("");
    if (terminalPasteScheduleEl) {
      terminalPasteScheduleEl.hidden = true;
    }
    if (terminalPasteScheduleToggleEl) {
      terminalPasteScheduleToggleEl.textContent = "定时发送";
    }
    terminalPasteTextEl?.focus();
  });
}

window.addEventListener("storage", (event) => {
  if (event.key !== SESSION_EVENT_STORAGE_KEY || !event.newValue) {
    return;
  }

  const mutation = parseSessionMutationEvent(event.newValue);
  if (!shouldRefreshForSessionMutation(mutation?.action)) {
    return;
  }

  scheduleSessionEventRefresh({ preferredSessionId: state.activeSessionId });
});

window.addEventListener("storage", (event) => {
  if (event.key !== RESUME_ARCHIVE_EVENT_STORAGE_KEY || !event.newValue) {
    return;
  }
  loadResumeArchives();
});

async function refreshTerminalSettingsFromBroadcast() {
  const previousShowAllWorkspaceSessions = state.showAllWorkspaceSessions;
  const previousAutoContinueOnError = state.autoContinueOnError;
  await loadTerminalSettings();

  if (previousShowAllWorkspaceSessions !== state.showAllWorkspaceSessions) {
    await loadSessions({
      preferredSessionId: state.activeSessionId,
      preserveCurrentList: true,
    });
  } else {
    renderSessions();
    syncAutoContinueHandledErrors();
    maybeAutoContinueErroredSessions();
  }

  if (previousAutoContinueOnError !== state.autoContinueOnError || state.autoContinueOnError) {
    syncAutoContinueHandledErrors();
    maybeAutoContinueErroredSessions();
  }
  await refreshTerminalPasteScheduledTasks();
}

window.addEventListener("storage", (event) => {
  if (event.key !== SETTINGS_EVENT_STORAGE_KEY || !event.newValue) {
    return;
  }
  refreshTerminalSettingsFromBroadcast();
});

window.addEventListener("storage", (event) => {
  if (event.key !== THEME_MODE_STORAGE_KEY) {
    return;
  }
  applyThemeMode(event.newValue || DEFAULT_THEME_MODE);
  setTerminalCursorHiddenForCorrection(terminalCursorCorrectionActive, { force: true });
  fitTerminal({ force: true });
  scheduleTerminalSizeSettle();
});

// Settings panel -> terminal page: cancel a pending background paste send.
window.addEventListener("storage", (event) => {
  if (event.key !== TERMINAL_PASTE_SCHEDULED_CANCEL_STORAGE_KEY || !event.newValue) {
    return;
  }
  let data = null;
  try {
    data = JSON.parse(event.newValue);
  } catch (_error) {
    return;
  }
  terminalPasteScheduledApplyCancelBroadcast({ data });
});

window.addEventListener("pageshow", refreshSessionsAfterPageResume);
window.addEventListener("online", refreshSessionsAfterPageResume);
window.addEventListener("focus", () => {
  refreshSessionsAfterPageResume();
  refreshTerminalPasteScheduledTasks();
});
document.addEventListener("visibilitychange", () => {
  syncActiveTerminalContextOutputVisibility();
  if (document.visibilityState === "visible") {
    refreshSessionsAfterPageResume();
    refreshTerminalPasteScheduledTasks();
  }
});

window.addEventListener(
  "scroll",
  () => {
    syncScrollTopButtonOffset();
    updateScrollTopButton();
    updateTerminalScrollBottomButton();
    updatePageScrollRail();
  },
  { passive: true },
);

// Re-fit the terminal when the mobile system keyboard (or other viewport
// resize) shows or hides.  visualViewport.resize fires for system keyboard
// open/close; window.resize and orientationchange are fallbacks.
let terminalViewportResizeTimer = null;
let terminalViewportResizeFrame = null;
let terminalViewportResizePageSnapshot = null;

function applyTerminalViewportResizeLayout(pageSnapshot, { settle = false } = {}) {
  fitTerminal();
  if (settle) {
    scheduleTerminalSizeSettle();
  }
  restorePageScrollSnapshotForLayout(pageSnapshot);
  schedulePageScrollSnapshotRestore(pageSnapshot);
  syncScrollTopButtonOffset();
  updatePageScrollRail();
}

function handleTerminalViewportResize() {
  if (terminalViewportResizeTimer === null) {
    terminalViewportResizePageSnapshot = capturePageScrollSnapshotForLayout();
    suppressTerminalScrollSaveForLayout(
      TERMINAL_VIEWPORT_RESIZE_DEBOUNCE_MS + TERMINAL_LAYOUT_SCROLL_SUPPRESSION_MS,
    );
    if (terminalViewportResizeFrame === null) {
      const pageSnapshot = terminalViewportResizePageSnapshot;
      terminalViewportResizeFrame = window.requestAnimationFrame(() => {
        terminalViewportResizeFrame = null;
        applyTerminalViewportResizeLayout(pageSnapshot);
      });
    }
  }
  if (terminalViewportResizeTimer !== null) {
    window.clearTimeout(terminalViewportResizeTimer);
  }
  terminalViewportResizeTimer = window.setTimeout(() => {
    terminalViewportResizeTimer = null;
    const pageSnapshot = terminalViewportResizePageSnapshot;
    applyTerminalViewportResizeLayout(pageSnapshot, { settle: true });
    terminalViewportResizePageSnapshot = null;
  }, TERMINAL_VIEWPORT_RESIZE_DEBOUNCE_MS);
}

if (window.visualViewport) {
  window.visualViewport.addEventListener("resize", handleTerminalViewportResize);
  window.visualViewport.addEventListener("scroll", handleTerminalViewportResize, { passive: true });
}
window.addEventListener("resize", handleTerminalViewportResize, { passive: true });
window.addEventListener("orientationchange", handleTerminalViewportResize, { passive: true });

window.addEventListener("popstate", async (event) => {
  if (event.state?.webclxTerminal && Number.isInteger(event.state.index)) {
    state.historyIndex = event.state.index;
    state.historyMaxIndex = Math.max(state.historyMaxIndex, event.state.index);
  }
  updateNavigationButtons();

  const nextLocation = readLocationState();
  const pathChanged = nextLocation.path !== state.currentPath;
  const sessionChanged = nextLocation.sessionId !== state.activeSessionId;

  if (!pathChanged && !sessionChanged) {
    syncTopNavigation();
    return;
  }

  state.currentPath = normalizeTerminalPath(nextLocation.path);
  state.activeSessionId = nextLocation.sessionId;
  closeSessionRenameEditor();
  renderSessions();
  await loadSessions({ preferredSessionId: nextLocation.sessionId });
});

window.addEventListener("beforeunload", () => {
  cancelNewSessionQuickStart();
  if (sessionEventRefreshTimer !== null) {
    window.clearTimeout(sessionEventRefreshTimer);
    sessionEventRefreshTimer = null;
  }
  if (sessionActivityRefreshTimer !== null) {
    window.clearTimeout(sessionActivityRefreshTimer);
    sessionActivityRefreshTimer = null;
  }
  sessionActivityRefreshPending = false;
  sessionDropdownInteracting = false;
  if (terminalOverlayObserver) {
    terminalOverlayObserver.disconnect();
    terminalOverlayObserver = null;
  }
  if (terminalLayoutObserver) {
    terminalLayoutObserver.disconnect();
    terminalLayoutObserver = null;
  }
  disposeAllTerminalSessionContexts();
});

window.addEventListener("load", async () => {
  const urlPath = normalizeTerminalPath(initialLocation.path);
  state.currentPath = urlPath;
  restoreTerminalPathExpanded();
  applyTerminalWideModeLayout();
  syncTerminalHostHeight();
  syncTerminalStickyOffsets();
  syncTerminalNavScroll({ forceEnd: true });
  syncScrollTopButtonOffset();
  updateScrollTopButton();
  updateTerminalScrollBottomButton();
  updatePageScrollRail();
  initializeNavigationState();
  syncCurrentPathDisplay();
  await loadTerminalSettings();
  await loadResumeArchives();
  if (shouldCreateInitialTerminalSession()) {
    await createSession({
      autoSelect: true,
      pushHistoryOnSelect: false,
      enableQuickStart: !initialLocation.runCommand,
      allowDuringInitialIntent: true,
    });
    state.initialTerminalIntentPending = false;
    syncCreateSessionButton();
    syncHistory();
  } else {
    state.initialTerminalIntentPending = false;
    syncCreateSessionButton();
    await loadSessions({ preferredSessionId: currentLocationSessionId() });

    if (
      initialLocation.quickStart &&
      initialLocation.sessionId &&
      state.activeSessionId === initialLocation.sessionId
    ) {
      armNewSessionQuickStart(initialLocation.sessionId);
    }
  }
  syncTerminalHostHeight();
  syncTerminalStickyOffsets();
  syncTerminalNavScroll();
  syncScrollTopButtonOffset();
  updateScrollTopButton();
  updateTerminalScrollBottomButton();
  updatePageScrollRail();
  setTerminalPasteScheduleChip(false);
  await refreshTerminalPasteScheduledTasks();
  ensureTerminalPasteScheduledRefreshTimer();
  scheduleSessionActivityRefresh();
});
