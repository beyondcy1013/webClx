function getInitialTab() {
  // Navigation uses semantic paths (/workspace, /codex_api, /settings, ...).
  // Legacy hash URLs (#sessions, #settings/compile) still work for old bookmarks.
  const path = window.location.pathname.toLowerCase();
  const hash = window.location.hash.toLowerCase();

  if (
    path === "/workspace_history" ||
    path === "/history" ||
    hash === "#history" ||
    hash === "#workspace-history"
  ) {
    return "workspace-history";
  }
  if (path === "/sessions" || hash === "#sessions" || hash === "#session") {
    return "workspace";
  }
  if (path === "/archives" || hash === "#terminal-archives" || hash === "#archives") {
    return "terminal-archives";
  }
  if (
    path === "/codex_oauth" ||
    path === "/auth" ||
    hash === "#inspect" ||
    hash === "#auth"
  ) {
    return "auth";
  }
  if (path === "/codex_api" || path === "/api" || hash === "#api") {
    return "api";
  }
  if (path === "/claude_api" || path === "/claude" || hash === "#claude") {
    return "claude";
  }
  if (path === "/desktop" || hash === "#desktop") {
    return "desktop";
  }
  if (path.startsWith("/settings") || hash === "#settings" || hash.startsWith("#settings/")) {
    return "settings";
  }
  return "workspace";
}

function getInitialSettingsTab() {
  // Path: /settings/<subtab>; fallback to legacy hash #settings/<subtab>.
  const path = window.location.pathname.toLowerCase();
  const hash = window.location.hash.toLowerCase();
  if (path.startsWith("/settings/")) {
    return normalizeSettingsTab(path.slice("/settings/".length));
  }
  if (hash.startsWith("#settings/")) {
    return normalizeSettingsTab(hash.slice("#settings/".length));
  }
  return "system";
}

const WORKSPACE_HISTORY_STORAGE_KEY = "webclx:workspace-history";
const WORKSPACE_HISTORY_MIGRATED_STORAGE_KEY = "webclx:workspace-history-config-migrated";
const MAX_WORKSPACE_HISTORY_ITEMS = 50;
const SESSION_VIEWS_REFRESH_DELAY_MS = 2000;
const AUTH_FORM_DEFAULT_STATUS = "保存时会先转换成当前 auth.json 的键结构，再写入预设仓库。";
const API_FORM_DEFAULT_STATUS =
  "应用 API 预设时会同时写入 auth.json，并按设置页默认或预设覆盖去校正 config.toml。";
const CLAUDE_FORM_DEFAULT_STATUS =
  "官方模型设置与第三方模型设置通过单选框二选一。应用 Claude 预设时会更新 ~/.claude/settings.json 的 env 配置、模型字段和额外选项。";
const DOMESTIC_MODEL_BASE_URL_KEYWORDS = Object.freeze([
  "bigmodel",
  "zhipu",
  "glm",
  "deepseek",
  "minimax",
  "minimaxi",
  "moonshot",
  "kimi",
  "siliconflow",
  "dashscope",
  "qwen",
  "aliyun",
  "baichuan",
  "stepfun",
  "hunyuan",
  "tencent",
  "volcengine",
  "doubao",
  "baidu",
  "wenxin",
  "sensetime",
]);
const DEFAULT_CODEX_API_AUTO_PROXY_MATCH_PROVIDER_IDS = Object.freeze([
  "zhipu",
  "deepseek",
  "minimax",
]);
const CODEX_API_AUTO_PROXY_MATCH_PROVIDERS = Object.freeze([
  {
    id: "zhipu",
    label: "智谱 / BigModel",
    urlPatterns: Object.freeze([
      "open.bigmodel.cn/api/coding/paas/v4",
      "/api/codex-proxy/zhipu/v1",
    ]),
    displayUrls: Object.freeze([
      "https://open.bigmodel.cn/api/coding/paas/v4",
      "http://127.0.0.1:11111/api/codex-proxy/zhipu/v1",
    ]),
  },
  {
    id: "deepseek",
    label: "DeepSeek",
    urlPatterns: Object.freeze(["api.deepseek.com"]),
    displayUrls: Object.freeze(["https://api.deepseek.com"]),
  },
  {
    id: "minimax",
    label: "MiniMax",
    urlPatterns: Object.freeze(["api.minimaxi.com/v1", "api.minimax.io"]),
    displayUrls: Object.freeze(["https://api.minimaxi.com/v1"]),
  },
  {
    id: "anthropic_chat",
    label: "Anthropic 中转站",
    urlPatterns: Object.freeze(["/anthropic", "anthropic.com"]),
    displayUrls: Object.freeze([
      "https://api.deepseek.com/anthropic",
      "https://relay.example.com/anthropic",
    ]),
  },
]);
const CODEX_API_AUTO_PROXY_MATCH_PROVIDER_IDS = new Set(
  CODEX_API_AUTO_PROXY_MATCH_PROVIDERS.map((provider) => provider.id),
);
const DEFAULT_CODEX_CONFIG_KEY = "model";
const DEFAULT_CODEX_MODEL = "";
const DEFAULT_CODEX_SECONDARY_CONFIG_KEY = "model_reasoning_effort";
const DEFAULT_CODEX_SECONDARY_CONFIG_VALUE = "high";
const DEFAULT_CODEX_DEFAULT_CONFIG_ENTRIES = Object.freeze([
  Object.freeze({ key: DEFAULT_CODEX_CONFIG_KEY, value: DEFAULT_CODEX_MODEL }),
  Object.freeze({
    key: DEFAULT_CODEX_SECONDARY_CONFIG_KEY,
    value: DEFAULT_CODEX_SECONDARY_CONFIG_VALUE,
  }),
]);
const DEFAULT_CLAUDE_DEFAULT_CONFIG_ENTRIES = Object.freeze([
  Object.freeze({
    key: "ANTHROPIC_DEFAULT_HAIKU_MODEL",
    value: "claude-haiku-4-5-20251001",
  }),
  Object.freeze({ key: "ANTHROPIC_DEFAULT_SONNET_MODEL", value: "claude-sonnet-4-6" }),
  Object.freeze({ key: "ANTHROPIC_DEFAULT_OPUS_MODEL", value: "claude-opus-4-6" }),
]);
const DEFAULT_SHOW_FULL_PATH = true;
const {
  DEFAULT_FONT_SIZE_TIER_1,
  DEFAULT_FONT_SIZE_TIER_2,
  DEFAULT_FONT_SIZE_TIER_3,
  DEFAULT_FONT_SIZE_TIER_4,
  DEFAULT_FONT_SIZE_TIERS,
  DEFAULT_TERMINAL_ACTIVITY_AGENT_DISPLAY,
  DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS,
  DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_FACTOR,
  DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_MAX_MINUTES,
  DEFAULT_TERMINAL_AUTO_CONTINUE_RESPECT_MANUAL_INTERRUPT,
  DEFAULT_TERMINAL_FAB_ACTION_COLOR,
  DEFAULT_TERMINAL_FAB_ACTION_OPACITY,
  DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH,
  DEFAULT_TERMINAL_FUNCTION_COMMANDS,
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
  DEFAULT_TERMINAL_USER,
  DEFAULT_THEME_MODE,
  MAX_TERMINAL_FUNCTION_COMMANDS,
  MAX_TERMINAL_QUICK_COMMANDS,
  MAX_TERMINAL_COMMAND_COLLECTIONS,
  MAX_TERMINAL_COMMAND_COLLECTION_ITEMS,
  TERMINAL_COMPLETION_BELL_URL,
  THEME_MODE_STORAGE_KEY,
  cloneDefaultTerminalFunctionCommands,
  cloneDefaultTerminalQuickCommands,
  cloneDefaultTerminalCommandCollections,
  ensureBuiltInTerminalFunctionCommands,
  ensureBuiltInTerminalSlashCommands,
  ensureBuiltInTerminalToolEntries,
  filterMovedSlashCommands,
  isMovedSlashCommand,
  moveCompactSlashCommandToEnd,
  normalizeFontSizeTier,
  normalizeFontSizeTiers,
  normalizeHostName,
  normalizeTerminalActivityAgentDisplay,
  normalizeTerminalAutoContinueIntervalSeconds,
  normalizeTerminalAutoContinueBackoffFactor,
  normalizeTerminalAutoContinueBackoffMaxMinutes,
  normalizeTerminalFabActionColor,
  normalizeTerminalFabActionOpacity,
  normalizeTerminalFloatingButtonOffsetVh,
  normalizeTerminalFunctionCommand,
  normalizeTerminalFunctionCommandLine,
  normalizeTerminalFunctionCommands,
  normalizeTerminalCommandCollections,
  normalizeTerminalToolEntries,
  normalizeTerminalQuickCommand,
  normalizeTerminalQuickCommands,
  normalizeTerminalQuickStartDefaultKey,
  normalizeTerminalQuickText,
  normalizeTerminalRenamePreset,
  normalizeTerminalRenamePresets,
  normalizeTerminalSoftKeyboardScale,
  normalizeTerminalScrollbackLines,
  normalizeTerminalTouchSelectionLongPressMs,
  normalizeTerminalUser,
  normalizeThemeMode,
  readStoredThemeMode,
  resolveThemeMode,
} = globalThis.WebClxTerminalSettings;
const {
  DEFAULT_WORKSPACE_BROWSER_ICON_PATH,
  DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
  normalizeProjectIconPath,
  workspaceProjectColorSlots,
  createWorkspaceProjectIcon,
  enhanceWorkspaceIconSelect,
} = globalThis.WebClxWorkspaceProjectIcons;
const {
  sessionActivityAgentLabel: sharedSessionActivityAgentLabel,
  sessionActivityAgentPrefix: sharedSessionActivityAgentPrefix,
  sessionActivityAgentSuffix: sharedSessionActivityAgentSuffix,
  sessionActivityLabel: sharedSessionActivityLabel,
  sessionActivityPrefix: sharedSessionActivityPrefix,
  sessionActivityState: sharedSessionActivityState,
  sessionActivityText: sharedSessionActivityText,
  sessionTimestamp: sharedSessionTimestamp,
  sortSessionsByRecentActivity: sharedSortSessionsByRecentActivity,
} = globalThis.WebClxTerminalSessionActivity;
const {
  SESSION_EVENT_STORAGE_KEY,
  announceSessionMutation: sharedAnnounceSessionMutation,
  getStoredGlobalSessionId,
  getStoredSessionId,
  parseSessionMutationEvent,
  RESUME_ARCHIVE_EVENT_STORAGE_KEY,
  readSessionPreferences,
  sessionPreferenceKey,
  shouldRefreshForSessionMutation,
  SETTINGS_EVENT_STORAGE_KEY,
  storeGlobalSessionId,
  storeSessionId,
} = globalThis.WebClxTerminalSessionStorage;
const DEFAULT_TERMINAL_DEFAULT_ENV_VARS = Object.freeze([]);
const MAX_TERMINAL_DEFAULT_ENV_VARS = 64;
const RESERVED_TERMINAL_DEFAULT_ENV_KEYS = new Set([
  "HOME",
  "SHELL",
  "USER",
  "LOGNAME",
  "PWD",
  "OLDPWD",
  "SHLVL",
  "_",
  "TMUX",
  "TMUX_PANE",
  "TERM",
]);
const DEFAULT_SERVER_PORT_AUTO_INCREMENT = true;
const DEFAULT_COMPILE_COMMAND_TIMEOUT_SECS = 600;
const DEFAULT_COMPILE_MAX_CONCURRENCY = 5;
const DEFAULT_COMPILE_ENVIRONMENT = Object.freeze([]);
const MAX_COMPILE_ENV_VARS = 64;
const DEFAULT_SESSION_TTL_DAYS = 30;
const DEFAULT_TERMINAL_ERROR_MATCH_LINE_LIMIT = 12;
const DEFAULT_TERMINAL_COMPLETION_BELL_ENABLED = true;
const PRESET_SYNC_SECTIONS = Object.freeze([
  Object.freeze({ key: "auth_presets", label: "Codex_OAuth" }),
  Object.freeze({ key: "api_presets", label: "Codex_API" }),
  Object.freeze({ key: "claude_presets", label: "Claude_API" }),
  Object.freeze({ key: "proxy_presets", label: "代理预设/上游代理" }),
]);
const PRESET_SYNC_CONFIG_ENDPOINT = "/api/settings/preset-config";
const PRESET_SYNC_PROXY_WARNING =
  "警告：代理预设/上游代理可能只适用于源服务器。不同服务器可能无法连接同样代理，导入后可能导致本机网络不可用。";
const DEFAULT_TERMINAL_ERROR_KEYWORDS = Object.freeze([
  "stream disconnected before completion:",
  "Concurrency limit exceeded for user, please retry later",
  "Selected model is at capacity. Please try a different model.",
  "API Error: Request rejected (429)",
  "已达到 5 小时的使用上限",
  "sending request for url",
  "(https://ai.router.team/responses)",
  "exceeded retry limit",
  "last status: 429",
  "last status: 503",
  "last status: 404",
  "unexpected status 502 Bad Gateway: Upstream service temporarily unavailable, url:",
  "429 Too Many Requests",
  "503 Service Unavailable",
  "404 Not Found",
]);
const TERMINAL_ERROR_KEYWORD_ACTION_CONTINUE = "continue";
const TERMINAL_ERROR_KEYWORD_ACTION_COMPACT_THEN_CONTINUE = "compact_then_continue";
const TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY = "mark_only";
const TERMINAL_ERROR_KEYWORD_ACTION_LABELS = Object.freeze({
  [TERMINAL_ERROR_KEYWORD_ACTION_CONTINUE]: "继续",
  [TERMINAL_ERROR_KEYWORD_ACTION_COMPACT_THEN_CONTINUE]: "先 /compact 再继续",
  [TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY]: "仅标记错误",
});
const DEFAULT_TERMINAL_ERROR_KEYWORD_ACTIONS = Object.freeze([
  { keyword: "ran out of room in the model's context window", action: TERMINAL_ERROR_KEYWORD_ACTION_COMPACT_THEN_CONTINUE },
  { keyword: "last status: 404", action: TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY },
  { keyword: "404 Not Found", action: TERMINAL_ERROR_KEYWORD_ACTION_MARK_ONLY },
]);
const DEFAULT_TERMINAL_AUTO_CONTINUE_TIME_PATTERNS = Object.freeze([
  "限额将在 {time} 重置",
  "reset at {time}",
  "resets at {time}",
  "will reset at {time}",
]);
const DEFAULT_TERMINAL_SCHEDULED_INPUT_AVOID_WINDOW = "14:00-18:00";
const DEFAULT_PROXY_CODEX_PROMPT = "hi";
const CONFIG_OVERRIDE_KEY_OPTIONS_ID = "config-override-key-options";
const CONFIG_OVERRIDE_VALUE_OPTIONS_ID = "config-override-value-options";
const DEFAULT_CLAUDE_MODEL_OPTIONS = [
  "claude-sonnet-4-6",
  "claude-opus-4-6",
  "claude-opus-4-6-thinking",
  "GLM-5.1",
  "GLM-4.7",
];
const APP_TITLE_BASE = "webClx";

function applyDocumentTitle(hostName = state.hostName) {
  const normalizedHostName = normalizeHostName(hostName);
  document.title = normalizedHostName
    ? `${APP_TITLE_BASE} - ${normalizedHostName}`
    : APP_TITLE_BASE;
}

const state = {
  currentPath: new URLSearchParams(window.location.search).get("path") || "",
  returnTerminalSessionId:
    new URLSearchParams(window.location.search).get("terminal_session") || "",
  currentFilePath: "",
  currentFileEditable: false,
  dirty: false,
  activeTab: getInitialTab(),
  activeSettingsTab: getInitialSettingsTab(),
  currentDirectory: null,
  currentWorkspaceDirectoryPath: "",
  workspaceDir: "",
  defaultWorkspaceDir: "",
  terminalUser: DEFAULT_TERMINAL_USER,
  terminalUserHome: "",
  defaultTerminalUser: DEFAULT_TERMINAL_USER,
  availableUsers: [],
  terminalQuickCommands: cloneDefaultTerminalQuickCommands(),
  terminalSlashCommands: normalizeTerminalFunctionCommands(DEFAULT_TERMINAL_SLASH_COMMANDS),
  terminalFunctionCommands: cloneDefaultTerminalFunctionCommands(),
  terminalCommandCollections: normalizeTerminalCommandCollections(cloneDefaultTerminalCommandCollections()),
  terminalToolEntries: [],
  terminalRenamePresets: normalizeTerminalRenamePresets(DEFAULT_TERMINAL_RENAME_PRESETS),
  terminalQuickStartDefaultKey: DEFAULT_TERMINAL_QUICK_START_DEFAULT_KEY,
  terminalDefaultEnvVars: [],
  terminalShortcutExpandedGroups: { slash: false, function: false },
  defaultTerminalQuickCommands: cloneDefaultTerminalQuickCommands(),
  defaultTerminalSlashCommands: normalizeTerminalFunctionCommands(DEFAULT_TERMINAL_SLASH_COMMANDS),
  defaultTerminalFunctionCommands: cloneDefaultTerminalFunctionCommands(),
  defaultTerminalCommandCollections: normalizeTerminalCommandCollections(cloneDefaultTerminalCommandCollections()),
  defaultTerminalToolEntries: [],
  defaultTerminalRenamePresets: normalizeTerminalRenamePresets(DEFAULT_TERMINAL_RENAME_PRESETS),
  defaultTerminalQuickStartDefaultKey: DEFAULT_TERMINAL_QUICK_START_DEFAULT_KEY,
  defaultTerminalDefaultEnvVars: [],
  terminalQuickEditingIndex: -1,
  showDotEntries: false,
  showAllWorkspaceSessions: true,
  desktopTerminalSoftKeyboardEnabled: true,
  terminalSoftKeyboardScale: DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE,
  terminalFloatingButtonOffsetVh: DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH,
  terminalFabActionColor: DEFAULT_TERMINAL_FAB_ACTION_COLOR,
  terminalFabActionOpacity: DEFAULT_TERMINAL_FAB_ACTION_OPACITY,
  terminalFabAutoExpand: true,
  terminalTouchSelectionLongPressMs: DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS,
  terminalScrollbackLines: DEFAULT_TERMINAL_SCROLLBACK_LINES,
  terminalErrorMatchLineLimit: DEFAULT_TERMINAL_ERROR_MATCH_LINE_LIMIT,
  terminalAutoContinueIntervalSeconds: DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS,
  terminalAutoContinueBackoffFactor: DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_FACTOR,
  terminalAutoContinueBackoffMaxMinutes: DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_MAX_MINUTES,
  terminalAutoContinueRespectManualInterrupt:
    DEFAULT_TERMINAL_AUTO_CONTINUE_RESPECT_MANUAL_INTERRUPT,
  terminalAutoContinueTimePatterns: [...DEFAULT_TERMINAL_AUTO_CONTINUE_TIME_PATTERNS],
  terminalAutoContinueActiveWindow: "",
  terminalScheduledInputAvoidWindow: DEFAULT_TERMINAL_SCHEDULED_INPUT_AVOID_WINDOW,
  terminalErrorKeywords: [...DEFAULT_TERMINAL_ERROR_KEYWORDS],
  terminalErrorKeywordActions: [...DEFAULT_TERMINAL_ERROR_KEYWORD_ACTIONS.map((action) => ({ ...action }))],
  terminalActivityAgentDisplay: DEFAULT_TERMINAL_ACTIVITY_AGENT_DISPLAY,
  terminalCompletionBellEnabled: DEFAULT_TERMINAL_COMPLETION_BELL_ENABLED,
  serverPortAutoIncrement: DEFAULT_SERVER_PORT_AUTO_INCREMENT,
  compileCommandTimeoutSecs: DEFAULT_COMPILE_COMMAND_TIMEOUT_SECS,
  compileMaxConcurrency: DEFAULT_COMPILE_MAX_CONCURRENCY,
  compileEnvironment: [],
  sessionTtlDays: DEFAULT_SESSION_TTL_DAYS,
  favoritePaths: [],
  claudeModelOptions: [...DEFAULT_CLAUDE_MODEL_OPTIONS],
  claudeDefaultConfigEntries: cloneDefaultClaudeDefaultConfigEntries(),
  codexDefaultConfigEntries: cloneDefaultCodexDefaultConfigEntries(),
  codexApiAutoProxyMatchProviderIds: [...DEFAULT_CODEX_API_AUTO_PROXY_MATCH_PROVIDER_IDS],
  codexConfigKey: DEFAULT_CODEX_CONFIG_KEY,
  codexConfigValue: DEFAULT_CODEX_MODEL,
  codexSecondaryConfigKey: DEFAULT_CODEX_SECONDARY_CONFIG_KEY,
  codexSecondaryConfigValue: DEFAULT_CODEX_SECONDARY_CONFIG_VALUE,
  showFullPath: DEFAULT_SHOW_FULL_PATH,
  workspaceBrowserIconPath: DEFAULT_WORKSPACE_BROWSER_ICON_PATH,
  terminalWorkspaceIconPath: DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
  themeMode: readStoredThemeMode(),
  fontSizeTiers: [...DEFAULT_FONT_SIZE_TIERS],
  proxyPresets: [],
  presetTableSort: new Map(),
  activeProxyId: null,
  activeProxy: null,
  frpc: null,
  frps: null,
  frpRoles: [],
  frpSystemItems: [],
  frpRoleDownloadPlatform: null,
  editingFrpRoleId: "",
  activeFrpRoleTab: "frps",
  editingFrpProxyIndex: -1,
  frpRoleDraftProxies: [],
  selectedFrpProxyIndexes: new Set(),
  hostName: "",
  serverVersion: "",
  settingsConfigFileKey: "codex_config",
  settingsConfigFileLoaded: false,
  settingsConfigFileDirty: false,
  settingsConfigFileOptions: [],
  compileStatus: null,
  compileStatusRequestToken: 0,
  autoContinueTaskRequestToken: 0,
  pasteScheduledTaskTickTimer: null,
  pasteScheduledTaskLastSnapshot: null,
  pasteScheduledTaskLastSnapshots: [],
  updateDownloadUrl: "",
  lastProxyTestSummary: "",
  lastProxyTestTime: 0,
  authPresets: [],
  authPresetExportSelection: new Set(),
  authPresetsLoaded: false,
  authPresetsLoading: false,
  authRefreshingPresetIds: new Set(),
  authRefreshErrorsByPresetId: new Map(),
  authPresetTestResults: new Map(),
  authPresetsTesting: new Set(),
  authOauthSessionId: "",
  apiPresets: [],
  apiPresetGroupMode: "base_url",
  apiPresetSearchTerm: "",
  apiPresetSelectionMode: false,
  apiPresetExportSelection: new Set(),
  apiPresetsLoaded: false,
  apiPresetsLoading: false,
  apiPresetTestResults: new Map(),
  apiPresetsTesting: new Set(),
  claudePresets: [],
  claudePresetExportSelection: new Set(),
  claudePresetsLoaded: false,
  claudePresetsLoading: false,
  claudePresetTestResults: new Map(),
  claudePresetsTesting: new Set(),
  remotePresetConfigSummary: null,
  remotePresetConfigSourceUrl: "",
  presetSyncRemoteUrlHistory: [],
  desktopRemoteUrl: "",
  desktopRemoteUrlHistory: [],
  upstreamProxy: {
    codex_api_proxy_enabled: false,
    claude_proxy_enabled: false,
    active_api_proxy_preset_id: null,
    active_claude_proxy_preset_id: null,
  },
  editingAuthPresetId: "",
  editingApiPresetId: "",
  editingClaudePresetId: "",
  apiApplyProxyManuallyChanged: false,
  sessions: [],
  directorySessions: [],
  terminalArchives: [],
  codexConversations: [],
  directorySessionId: "",
  directorySessionUiMode: "placeholder",
  directorySessionPlaceholderMessage: "",
  directorySessionUiBlocked: false,
  pendingDirectorySessionUiSync: false,
  sessionsSessionUiBlocked: false,
  pendingSessionsSessionUiSync: false,
  preferredSessionId: "",
  workspaceHistory: readWorkspaceHistory(),
  workspaceHistorySelectedPath: "",
  workspaceHistoryPersistToken: 0,
  workspaceHistorySearchQuery: "",
  workspaceHistorySearchToken: 0,
  workspaceHistorySearchDebounceId: 0,
  workspaceHistorySettingsReady: false,
 workspaceHistorySearchAllWorkspaces: false,
  workspaceHistoryRecentOnly: true,
 sessionRequestToken: 0,
  terminalArchiveRequestToken: 0,
  codexConversationRequestToken: 0,
  directorySessionRequestToken: 0,
  sessionSearchRequestToken: 0,
  sessionSearchQuery: "",
  sessionSearchResults: [],
  renamingSessionId: "",
};

const entryList = document.getElementById("entry-list");
const currentPathEl = document.getElementById("current-path");
const currentPathCopyButton = document.getElementById("copy-current-path");
const favoritePathSelectEl = document.getElementById("favorite-path-select");
const currentFileEl = document.getElementById("current-file");
const fileMetaEl = document.getElementById("file-meta");
const editorPanelEl = document.querySelector(".editor-panel");
const fileStatusEl = document.getElementById("file-status");
const editorEl = document.getElementById("editor");

const importAuthButton = document.getElementById("import-auth-clipboard");
const saveButton = document.getElementById("save-file");
const terminalLink = document.getElementById("open-terminal-root");
const directorySessionListEl = document.getElementById("directory-session-list");
const workspaceHistoryViewEl = document.getElementById("workspace-history-view");
const workspaceHistoryStatusEl = document.getElementById("workspace-history-status");
const workspaceHistoryListEl = document.getElementById("workspace-history-list");
const workspaceHistoryPathSelectEl = document.getElementById("workspace-history-path-select");
const workspaceHistoryTerminalButton = document.getElementById("workspace-history-terminal");
const workspaceHistoryDeleteButton = document.getElementById("workspace-history-delete");
const workspaceHistoryRefreshButton = document.getElementById("workspace-history-refresh");
const workspaceHistorySearchFormEl = document.getElementById("workspace-history-search-form");
const workspaceHistorySearchInputEl = document.getElementById("workspace-history-search-input");
const workspaceHistorySearchSubmitButton = document.getElementById("workspace-history-search-submit");
const workspaceHistorySearchClearButton = document.getElementById("workspace-history-search-clear");
const workspaceHistorySearchAllEl = document.getElementById("workspace-history-search-all");
const workspaceHistoryRecentOnlyEl = document.getElementById("workspace-history-recent-only");
const sessionsStatusEl = document.getElementById("sessions-status");
const sessionsSearchFormEl = document.getElementById("sessions-search-form");
const sessionsSearchInputEl = document.getElementById("sessions-search-input");
const sessionsSearchSubmitButton = document.getElementById("sessions-search-submit");
const sessionsSearchClearButton = document.getElementById("sessions-search-clear");
const refreshSessionsButton = document.getElementById("refresh-sessions");
const createSessionButton = document.getElementById("create-session");
const sessionsSessionListEl = document.getElementById("sessions-session-list");
const sessionTerminalLink = document.getElementById("open-terminal-session-root");
const topNavTerminalLink = document.getElementById("top-nav-terminal");
const sessionsListEl = document.getElementById("sessions-list");
const terminalArchivesViewEl = document.getElementById("terminal-archives-view");
const terminalArchivesStatusEl = document.getElementById("terminal-archives-status");
const refreshTerminalArchivesButton = document.getElementById("refresh-terminal-archives");
const terminalArchivesListEl = document.getElementById("terminal-archives-list");
const sessionRenameDialogEl = document.getElementById("terminal-rename-dialog");
const sessionRenameFormEl = document.getElementById("session-rename-form");
const sessionRenameInputEl = document.getElementById("session-rename-input");
const sessionRenameCancelButton = document.getElementById("session-rename-cancel");
const terminalRenameDialogStatusEl = document.getElementById("terminal-rename-dialog-status");
const authImportDialogEl = document.getElementById("auth-import-dialog");
const authImportFormEl = document.getElementById("auth-import-form");
const authImportTextEl = document.getElementById("auth-import-text");
const authImportCancelButton = document.getElementById("auth-import-cancel");
const authOauthStartButton = document.getElementById("auth-oauth-start");
const authOauthOpenLinkEl = document.getElementById("auth-oauth-open-link");
const authOauthCopyCodeButton = document.getElementById("auth-oauth-copy-code");
const authOauthSessionPanelEl = document.getElementById("auth-oauth-session-panel");
const authOauthSessionSummaryEl = document.getElementById("auth-oauth-session-summary");
const authOauthUserCodeEl = document.getElementById("auth-oauth-user-code");
const authOauthStatusEl = document.getElementById("auth-oauth-status");
const tabButtons = document.querySelectorAll(".tab-button[data-tab]");
const workspaceViewEl = document.getElementById("workspace-view");
const authViewEl = document.getElementById("auth-view");
const apiViewEl = document.getElementById("api-view");
const claudeViewEl = document.getElementById("claude-view");
const settingsViewEl = document.getElementById("settings-view");
const desktopViewEl = document.getElementById("desktop-view");
const desktopFrameEl = document.getElementById("desktop-frame");
const desktopReloadButtonEl = document.getElementById("desktop-reload");
const desktopUrlInputEl = document.getElementById("desktop-url-input");
const desktopUrlHistoryEl = document.getElementById("desktop-url-history");
const desktopApplyUrlButtonEl = document.getElementById("desktop-apply-url");
const desktopOpenButtonEl = document.getElementById("desktop-open-button");
const desktopFallbackUrlEl = document.getElementById("desktop-fallback-url");
const desktopFallbackOpenEl = document.getElementById("desktop-fallback-open");
const settingsCategoryButtons = document.querySelectorAll(
  ".settings-category-button[data-settings-category]",
);
const settingsSubtabsEl = document.getElementById("settings-subtabs");
const settingsSubpanels = document.querySelectorAll(".settings-subpanel[data-settings-panel]");
const settingsCurrentDirEl = document.getElementById("settings-current-dir");
const settingsCurrentUserEl = document.getElementById("settings-current-user");
const settingsUserHomeEl = document.getElementById("settings-user-home");
const settingsConfigFileEl = document.getElementById("settings-config-file");
const serverPortAutoIncrementInputEl = document.getElementById("server-port-auto-increment-input");
const compileCommandTimeoutInputEl = document.getElementById("compile-command-timeout-input");
const compileMaxConcurrencyInputEl = document.getElementById("compile-max-concurrency-input");
const compileEnvironmentInputEl = document.getElementById("compile-environment-input");
const sessionTtlDaysInputEl = document.getElementById("session-ttl-days-input");
const workspaceDirInputEl = document.getElementById("workspace-dir-input");
const terminalUserSelectEl = document.getElementById("terminal-user-select");
const terminalQuickCommandsListEl = document.getElementById("terminal-quick-commands-list");
const terminalQuickCommandAddButtonEl = document.getElementById("terminal-quick-command-add");
const terminalQuickCommandFormEl = document.getElementById("terminal-quick-command-form");
const terminalQuickCommandEditIndexEl = document.getElementById("terminal-quick-command-edit-index");
const terminalQuickCommandEditingLabelEl = document.getElementById(
  "terminal-quick-command-editing-label",
);
const terminalQuickCommandKeyInputEl = document.getElementById("terminal-quick-command-key-input");
const terminalQuickCommandLabelInputEl = document.getElementById(
  "terminal-quick-command-label-input",
);
const terminalQuickCommandCommandInputEl = document.getElementById(
  "terminal-quick-command-command-input",
);
const terminalQuickCommandCancelButtonEl = document.getElementById(
  "terminal-quick-command-cancel",
);
const terminalQuickStartDefaultSelectEl = document.getElementById(
  "terminal-quick-start-default-select",
);
const terminalDefaultEnvInputEl = document.getElementById("terminal-default-env-input");
const terminalSlashCommandsInputEl = document.getElementById("terminal-slash-commands-input");
const terminalFunctionCommandsInputEl = document.getElementById("terminal-function-commands-input");
const terminalCommandCollectionsEditorEl = document.getElementById("terminal-command-collections-editor");
const terminalCommandCollectionAddBtnEl = document.getElementById("terminal-command-collection-add");
const terminalShortcutsListEl = document.getElementById("terminal-shortcuts-list");
const terminalShortcutsResetButtonEl = document.getElementById("terminal-shortcuts-reset");
const terminalRenamePresetsInputEl = document.getElementById("terminal-rename-presets-input");
const showDotEntriesInputEl = document.getElementById("show-dot-entries-input");
const workspaceShowHiddenInputEl = document.getElementById("workspace-show-hidden-input");
const showAllWorkspaceSessionsInputEl = document.getElementById("show-all-workspace-sessions-input");
const desktopTerminalSoftKeyboardInputEl = document.getElementById(
  "desktop-terminal-soft-keyboard-input",
);
const terminalSoftKeyboardScaleInputEl = document.getElementById(
  "terminal-soft-keyboard-scale-input",
);
const terminalFloatingButtonOffsetInputEl = document.getElementById(
  "terminal-floating-button-offset-input",
);
const terminalFabActionColorInputEl = document.getElementById(
  "terminal-fab-action-color-input",
);
const terminalFabActionOpacityInputEl = document.getElementById(
  "terminal-fab-action-opacity-input",
);
const terminalFabActionOpacityOutputEl = document.getElementById(
  "terminal-fab-action-opacity-output",
);
const terminalFabAutoExpandInputEl = document.getElementById(
  "terminal-fab-auto-expand-input",
);
const terminalTouchSelectionLongPressInputEl = document.getElementById(
  "terminal-touch-selection-long-press-input",
);
const terminalScrollbackLinesInputEl = document.getElementById(
  "terminal-scrollback-lines-input",
);
const terminalErrorLineLimitInputEl = document.getElementById("terminal-error-line-limit-input");
const terminalAutoContinueIntervalInputEl = document.getElementById(
  "terminal-auto-continue-interval-input",
);
const terminalAutoContinueBackoffFactorInputEl = document.getElementById(
  "terminal-auto-continue-backoff-factor-input",
);
const terminalAutoContinueBackoffMaxMinutesInputEl = document.getElementById(
  "terminal-auto-continue-backoff-max-minutes-input",
);
const terminalAutoContinueRespectManualInterruptInputEl = document.getElementById(
  "terminal-auto-continue-respect-manual-interrupt-input",
);
const terminalAutoContinueTimePatternsInputEl = document.getElementById(
  "terminal-auto-continue-time-patterns-input",
);
const terminalAutoContinueActiveWindowInputEl = document.getElementById(
  "terminal-auto-continue-active-window-input",
);
const terminalScheduledInputAvoidWindowInputEl = document.getElementById(
  "terminal-scheduled-input-avoid-window-input",
);
const terminalErrorKeywordRulesBodyEl = document.getElementById("terminal-error-keyword-rules-body");
const terminalErrorKeywordAddBtnEl = document.getElementById("terminal-error-keyword-add-btn");
const terminalActivityAgentDisplaySelectEl = document.getElementById(
  "terminal-activity-agent-display-select",
);
const terminalCompletionBellEnabledInputEl = document.getElementById(
  "terminal-completion-bell-enabled-input",
);
const terminalCompletionBellTestButtonEl = document.getElementById("terminal-completion-bell-test");
const claudeModelOptionsInputEl = document.getElementById("claude-model-options-input");
const claudeDefaultConfigListEl = document.getElementById("claude-default-config-list");
const claudeDefaultConfigAddButtonEl = document.getElementById("claude-default-config-add");
const claudeDefaultConfigResetButtonEl = document.getElementById("claude-default-config-reset");
const claudeDefaultConfigSaveButtonEl = document.getElementById("claude-default-config-save");
const claudeDefaultConfigStatusEl = document.getElementById("claude-default-config-status");
const codexApiAutoProxyProviderInputEls = Array.from(
  document.querySelectorAll("[data-codex-api-auto-proxy-provider]"),
);
const codexDefaultConfigListEl = document.getElementById("codex-default-config-list");
const codexDefaultConfigAddButtonEl = document.getElementById("codex-default-config-add");
const codexDefaultConfigResetButtonEl = document.getElementById("codex-default-config-reset");
const codexDefaultConfigSaveButtonEl = document.getElementById("codex-default-config-save");
const codexDefaultConfigStatusEl = document.getElementById("codex-default-config-status");
const codexCommonApprovalNeverInputEl = document.getElementById("codex-common-approval-never");
const codexCommonSandboxFullAccessInputEl = document.getElementById(
  "codex-common-sandbox-full-access",
);
const codexCommonConfigPathEl = document.getElementById("codex-common-config-path");
const codexCommonConfigRefreshButtonEl = document.getElementById("codex-common-config-refresh");
const codexCommonConfigSaveButtonEl = document.getElementById("codex-common-config-save");
const codexCommonConfigStatusEl = document.getElementById("codex-common-config-status");
const settingsConfigFileSelectEl = document.getElementById("settings-config-file-select");
const settingsConfigFilePathEl = document.getElementById("settings-config-file-path");
const settingsConfigFileMetaEl = document.getElementById("settings-config-file-meta");
const settingsConfigFileEditorEl = document.getElementById("settings-config-file-editor");
const settingsConfigFileStatusEl = document.getElementById("settings-config-file-status");
const settingsConfigFileRefreshButtonEl = document.getElementById("settings-config-file-refresh");
const settingsConfigFileSaveButtonEl = document.getElementById("settings-config-file-save");
const showFullPathInputEl = document.getElementById("show-full-path-input");
const workspaceBrowserIconPathInputEl = document.getElementById(
  "workspace-browser-icon-path-input",
);
const terminalWorkspaceIconPathInputEl = document.getElementById(
  "terminal-workspace-icon-path-input",
);
const presetSyncRemoteUrlInputEl = document.getElementById("preset-sync-remote-url");
const presetSyncRemoteUrlHistoryEl = document.getElementById("preset-sync-remote-url-history");
const presetSyncPreviewBtnEl = document.getElementById("preset-sync-preview-btn");
const presetSyncImportBtnEl = document.getElementById("preset-sync-import-btn");
const presetSyncStatusEl = document.getElementById("preset-sync-status");
const presetSyncAuthCountEl = document.getElementById("preset-sync-auth-count");
const presetSyncApiCountEl = document.getElementById("preset-sync-api-count");
const presetSyncClaudeCountEl = document.getElementById("preset-sync-claude-count");
const presetSyncProxyCountEl = document.getElementById("preset-sync-proxy-count");
const presetSyncAuthEnabledEl = document.getElementById("preset-sync-auth-enabled");
const presetSyncApiEnabledEl = document.getElementById("preset-sync-api-enabled");
const presetSyncClaudeEnabledEl = document.getElementById("preset-sync-claude-enabled");
const presetSyncProxyEnabledEl = document.getElementById("preset-sync-proxy-enabled");
const presetSyncProxyStateEl = document.getElementById("preset-sync-proxy-state");
const themeModeSelectEl = document.getElementById("theme-mode-select");
const fontSizeTier1InputEl = document.getElementById("font-size-tier-1-input");
const fontSizeTier2InputEl = document.getElementById("font-size-tier-2-input");
const fontSizeTier3InputEl = document.getElementById("font-size-tier-3-input");
const fontSizeTier4InputEl = document.getElementById("font-size-tier-4-input");
const fontSettingsOpenButtonEl = document.getElementById("font-settings-open");
const fontSettingsDialogEl = document.getElementById("font-settings-dialog");
const fontSettingsCloseButtonEl = document.getElementById("font-settings-close");
const fontSettingsSummaryEl = document.getElementById("font-settings-summary");
const claudeSharedModelOptionsEl = document.getElementById("claude-shared-model-options");
const settingsStatusEl = document.getElementById("settings-status");
const saveSettingsButton = document.getElementById("save-settings");
const resetSettingsButton = document.getElementById("reset-settings");
const compileStatusRefreshButtonEl = document.getElementById("compile-status-refresh");
const compileStatusMessageEl = document.getElementById("compile-status-message");
const compilePendingListEl = document.getElementById("compile-pending-list");
const compileRunningListEl = document.getElementById("compile-running-list");
const compileRunListEl = document.getElementById("compile-run-list");
const compileHistoryClearButtonEl = document.getElementById("compile-history-clear");
const autoContinueTaskRefreshButtonEl = document.getElementById("auto-continue-task-refresh");
const autoContinueTaskStatusEl = document.getElementById("auto-continue-task-status");
const autoContinueTaskListEl = document.getElementById("auto-continue-task-list");
const pasteScheduledTaskListEl = document.getElementById("paste-scheduled-task-list");
const pasteScheduledTaskStatusEl = document.getElementById("paste-scheduled-task-status");
const pasteScheduledTaskRefreshButtonEl = document.getElementById("paste-scheduled-task-refresh");
const autoContinueHistoryListEl = document.getElementById("auto-continue-history-list");
const autoContinueHistoryToggleEl = document.getElementById("auto-continue-history-toggle");
const autoContinueHistoryClearEl = document.getElementById("auto-continue-history-clear");
const autoContinueHistoryWrapEl = document.getElementById("auto-continue-history-table-wrap");

const proxyPresetsListEl = document.getElementById("proxy-presets-list");
const proxyNameInputEl = document.getElementById("proxy-name-input");
const proxyTypeInputEl = document.getElementById("proxy-type-input");
const proxyServerInputEl = document.getElementById("proxy-server-input");
const proxyUsernameInputEl = document.getElementById("proxy-username-input");
const proxyPasswordInputEl = document.getElementById("proxy-password-input");
const proxyEditingIdEl = document.getElementById("proxy-editing-id");
const proxyTestBtnEl = document.getElementById("proxy-test-btn");
const proxyTestModeInputEls = Array.from(document.querySelectorAll('input[name="proxy-test-mode"]'));
const proxyTestModeHintEl = document.getElementById("proxy-test-mode-hint");
const proxyTestUrlFieldEl = document.getElementById("proxy-test-url-field");
const proxyTestUrlInputEl = document.getElementById("proxy-test-url-input");
const proxyCodexPromptFieldEl = document.getElementById("proxy-codex-prompt-field");
const proxyCodexPromptInputEl = document.getElementById("proxy-codex-prompt-input");
const proxySaveBtnEl = document.getElementById("proxy-save-btn");
const proxyClearBtnEl = document.getElementById("proxy-clear-btn");
const proxyTestResultEl = document.getElementById("proxy-test-result");
const proxyFormTitleEl = document.getElementById("proxy-form-title");
const proxyActiveSummaryEl = document.getElementById("proxy-active-summary");
const proxyEffectiveEnvEl = document.getElementById("proxy-effective-env");
const proxyScopeSummaryEl = document.getElementById("proxy-scope-summary");
const proxyHostEnvSummaryEl = document.getElementById("proxy-host-env-summary");

// Update panel elements
const updateCurrentVersionEl = document.getElementById("update-current-version");
const updateLocalDownloadUrlEl = document.getElementById("update-local-download-url");
const updateCopyUrlBtn = document.getElementById("update-copy-url-btn");
const updateCopyUrlStatusEl = document.getElementById("update-copy-url-status");
const updateRemoteCheckUrlInput = document.getElementById("update-remote-check-url");
const updateCheckRemoteBtn = document.getElementById("update-check-remote-btn");
const updateCheckStatusEl = document.getElementById("update-check-status");
const updateAvailablePanelEl = document.getElementById("update-available-panel");
const updateLatestVersionEl = document.getElementById("update-latest-version");
const updateDownloadUrlEl = document.getElementById("update-download-url");
const updateDownloadBtnEl = document.getElementById("update-download-btn");
const updateProgressEl = document.getElementById("update-progress");
const proxyLastTestSummaryEl = document.getElementById("proxy-last-test-summary");
const proxyLastTestTimeEl = document.getElementById("proxy-last-test-time");
const proxyConflictSummaryEl = document.getElementById("proxy-conflict-summary");
const proxyClearActiveBtnEl = document.getElementById("proxy-clear-active-btn");
const frpcStatusSummaryEl = document.getElementById("frpc-status-summary");
const frpcConfigPathEl = document.getElementById("frpc-config-path");
const frpcBinaryPathEl = document.getElementById("frpc-binary-path");
const frpcDownloadPlatformEl = document.getElementById("frpc-download-platform");
const frpcBinaryInputEl = document.getElementById("frpc-binary-input");
const frpcServerAddrInputEl = document.getElementById("frpc-server-addr-input");
const frpcServerPortInputEl = document.getElementById("frpc-server-port-input");
const frpcTokenInputEl = document.getElementById("frpc-token-input");
const frpcTlsInputEl = document.getElementById("frpc-tls-input");
const frpcProxyNameInputEl = document.getElementById("frpc-proxy-name-input");
const frpcProxyTypeInputEl = document.getElementById("frpc-proxy-type-input");
const frpcLocalIpInputEl = document.getElementById("frpc-local-ip-input");
const frpcLocalPortInputEl = document.getElementById("frpc-local-port-input");
const frpcRemotePortFieldEl = document.getElementById("frpc-remote-port-field");
const frpcRemotePortInputEl = document.getElementById("frpc-remote-port-input");
const frpcCustomDomainsFieldEl = document.getElementById("frpc-custom-domains-field");
const frpcCustomDomainsInputEl = document.getElementById("frpc-custom-domains-input");
const frpcExtraTomlInputEl = document.getElementById("frpc-extra-toml-input");
const frpcRefreshBtnEl = document.getElementById("frpc-refresh-btn");
const frpcDownloadBtnEl = document.getElementById("frpc-download-btn");
const frpcStartBtnEl = document.getElementById("frpc-start-btn");
const frpcStopBtnEl = document.getElementById("frpc-stop-btn");
const frpcRestartBtnEl = document.getElementById("frpc-restart-btn");
const frpcSaveBtnEl = document.getElementById("frpc-save-btn");
const frpcSaveStartBtnEl = document.getElementById("frpc-save-start-btn");
const frpcStatusMessageEl = document.getElementById("frpc-status-message");
const frpcLogTailEl = document.getElementById("frpc-log-tail");
const frpsStatusSummaryEl = document.getElementById("frps-status-summary");
const frpsConfigPathEl = document.getElementById("frps-config-path");
const frpsBinaryPathEl = document.getElementById("frps-binary-path");
const frpsDownloadPlatformEl = document.getElementById("frps-download-platform");
const frpsBinaryInputEl = document.getElementById("frps-binary-input");
const frpsBindAddrInputEl = document.getElementById("frps-bind-addr-input");
const frpsBindPortInputEl = document.getElementById("frps-bind-port-input");
const frpsTokenInputEl = document.getElementById("frps-token-input");
const frpsWebAddrInputEl = document.getElementById("frps-web-addr-input");
const frpsWebPortInputEl = document.getElementById("frps-web-port-input");
const frpsDashboardUserInputEl = document.getElementById("frps-dashboard-user-input");
const frpsDashboardPasswordInputEl = document.getElementById("frps-dashboard-password-input");
const frpsExtraTomlInputEl = document.getElementById("frps-extra-toml-input");
const frpsRefreshBtnEl = document.getElementById("frps-refresh-btn");
const frpsDownloadBtnEl = document.getElementById("frps-download-btn");
const frpsStartBtnEl = document.getElementById("frps-start-btn");
const frpsStopBtnEl = document.getElementById("frps-stop-btn");
const frpsRestartBtnEl = document.getElementById("frps-restart-btn");
const frpsSaveBtnEl = document.getElementById("frps-save-btn");
const frpsSaveStartBtnEl = document.getElementById("frps-save-start-btn");
const frpsStatusMessageEl = document.getElementById("frps-status-message");
const frpsLogTailEl = document.getElementById("frps-log-tail");
const frpRoleRefreshBtnEl = document.getElementById("frp-role-refresh-btn");
const frpSystemRefreshBtnEl = document.getElementById("frp-system-refresh-btn");
const frpServerRoleRefreshBtnEl = document.getElementById("frp-server-role-refresh-btn");
const frpServerSystemRefreshBtnEl = document.getElementById("frp-server-system-refresh-btn");
const frpRoleNewFrpcBtnEl = document.getElementById("frp-role-new-frpc-btn");
const frpRoleNewFrpsBtnEl = document.getElementById("frp-role-new-frps-btn");
const frpRolePlatformEl = document.getElementById("frp-role-platform");
const frpServerRolePlatformEl = document.getElementById("frp-server-role-platform");
const frpRoleCurrentSummaryEl = document.getElementById("frp-role-current-summary");
const frpServerCurrentSummaryEl = document.getElementById("frp-server-current-summary");
const frpServerRoleTableBodyEl = document.getElementById("frp-server-role-table-body");
const frpClientRoleTableBodyEl = document.getElementById("frp-client-role-table-body");
const frpSystemTableBodyEl = document.getElementById("frp-system-table-body");
const frpServerSystemTableBodyEl = document.getElementById("frp-server-system-table-body");
const frpRoleStatusMessageEl = document.getElementById("frp-role-status-message");
const frpServerRoleStatusMessageEl = document.getElementById("frp-server-role-status-message");
const frpRoleEditorEl = document.getElementById("frp-role-editor");
const frpRoleCloseBtnEl = document.getElementById("frp-role-close-btn");
const frpRoleEditorTitleEl = document.getElementById("frp-role-editor-title");
const frpRoleIdInputEl = document.getElementById("frp-role-id-input");
const frpRoleNameInputEl = document.getElementById("frp-role-name-input");
const frpRoleComponentInputEl = document.getElementById("frp-role-component-input");
const frpRoleBinarySourceInputEl = document.getElementById("frp-role-binary-source-input");
const frpRoleBinaryInputEl = document.getElementById("frp-role-binary-input");
const frpRoleExternalConfigInputEl = document.getElementById("frp-role-external-config-input");
const frpRoleFrpcFieldsEl = document.getElementById("frp-role-frpc-fields");
const frpRoleFrpcServerAddrInputEl = document.getElementById("frp-role-frpc-server-addr-input");
const frpRoleFrpcServerPortInputEl = document.getElementById("frp-role-frpc-server-port-input");
const frpRoleFrpcTokenInputEl = document.getElementById("frp-role-frpc-token-input");
const frpRoleFrpcTlsInputEl = document.getElementById("frp-role-frpc-tls-input");
const frpRoleFrpcProxyNameInputEl = document.getElementById("frp-role-frpc-proxy-name-input");
const frpRoleFrpcProxyTypeInputEl = document.getElementById("frp-role-frpc-proxy-type-input");
const frpRoleFrpcLocalIpInputEl = document.getElementById("frp-role-frpc-local-ip-input");
const frpRoleFrpcLocalPortInputEl = document.getElementById("frp-role-frpc-local-port-input");
const frpRoleFrpcRemotePortFieldEl = document.getElementById("frp-role-frpc-remote-port-field");
const frpRoleFrpcRemotePortInputEl = document.getElementById("frp-role-frpc-remote-port-input");
const frpRoleFrpcCustomDomainsFieldEl = document.getElementById("frp-role-frpc-custom-domains-field");
const frpRoleFrpcCustomDomainsInputEl = document.getElementById("frp-role-frpc-custom-domains-input");
const frpRoleFrpcProxyTableBodyEl = document.getElementById("frp-role-frpc-proxy-table-body");
const frpRoleFrpcProxySelectAllEl = document.getElementById("frp-role-frpc-proxy-select-all");
const frpRoleFrpcProxyAddBtnEl = document.getElementById("frp-role-frpc-proxy-add-btn");
const frpRoleFrpcProxyEditSelectedBtnEl = document.getElementById("frp-role-frpc-proxy-edit-selected-btn");
const frpRoleFrpcProxyDuplicateSelectedBtnEl = document.getElementById("frp-role-frpc-proxy-duplicate-selected-btn");
const frpRoleFrpcProxyDeleteSelectedBtnEl = document.getElementById("frp-role-frpc-proxy-delete-selected-btn");
const frpRoleFrpcProxySelectedCountEl = document.getElementById("frp-role-frpc-proxy-selected-count");
const frpRoleFrpcProxyStatusEl = document.getElementById("frp-role-frpc-proxy-status");
const frpRoleFrpcProxyEditorEl = document.getElementById("frp-role-frpc-proxy-editor");
const frpRoleFrpcProxySaveBtnEl = document.getElementById("frp-role-frpc-proxy-save-btn");
const frpRoleFrpcProxyCancelBtnEl = document.getElementById("frp-role-frpc-proxy-cancel-btn");
const frpRoleFrpsFieldsEl = document.getElementById("frp-role-frps-fields");
const frpRoleFrpsPublicAddrInputEl = document.getElementById("frp-role-frps-public-addr-input");
const frpRoleFrpsBindAddrInputEl = document.getElementById("frp-role-frps-bind-addr-input");
const frpRoleFrpsBindPortInputEl = document.getElementById("frp-role-frps-bind-port-input");
const frpRoleFrpsTokenInputEl = document.getElementById("frp-role-frps-token-input");
const frpRoleFrpsWebAddrInputEl = document.getElementById("frp-role-frps-web-addr-input");
const frpRoleFrpsWebPortInputEl = document.getElementById("frp-role-frps-web-port-input");
const frpRoleFrpsDashboardUserInputEl = document.getElementById("frp-role-frps-dashboard-user-input");
const frpRoleFrpsDashboardPasswordInputEl = document.getElementById("frp-role-frps-dashboard-password-input");
const frpRoleExtraTomlInputEl = document.getElementById("frp-role-extra-toml-input");
const frpRoleDownloadBtnEl = document.getElementById("frp-role-download-btn");
const frpRoleStartBtnEl = document.getElementById("frp-role-start-btn");
const frpRoleStopBtnEl = document.getElementById("frp-role-stop-btn");
const frpRoleRestartBtnEl = document.getElementById("frp-role-restart-btn");
const frpRoleDeleteBtnEl = document.getElementById("frp-role-delete-btn");
const frpRoleSaveBtnEl = document.getElementById("frp-role-save-btn");
const frpRoleSaveStartBtnEl = document.getElementById("frp-role-save-start-btn");
const frpRoleResetBtnEl = document.getElementById("frp-role-reset-btn");
const frpRoleEditorStatusEl = document.getElementById("frp-role-editor-status");
const frpRoleLogTailEl = document.getElementById("frp-role-log-tail");
const frpSourceModeInputEl = document.getElementById("frp-source-mode-input");
const frpSourceManualPanelEl = document.getElementById("frp-source-manual-panel");
const frpSourceSystemPanelEl = document.getElementById("frp-source-system-panel");
const frpSourceComponentInputEl = document.getElementById("frp-source-component-input");
const frpSourceSystemSelectEl = document.getElementById("frp-source-system-select");
const frpSourcePublicAddrInputEl = document.getElementById("frp-source-public-addr-input");
const frpSourcePublicPortInputEl = document.getElementById("frp-source-public-port-input");
const frpSourceAuthTokenInputEl = document.getElementById("frp-source-auth-token-input");
const frpSourceTestBtnEl = document.getElementById("frp-source-test-btn");
const frpSourceAddBtnEl = document.getElementById("frp-source-add-btn");
const frpSourceAdoptSelectedBtnEl = document.getElementById("frp-source-adopt-selected-btn");
const frpSourceStatusEl = document.getElementById("frp-source-status");
const frpServerSourceModeInputEl = document.getElementById("frp-server-source-mode-input");
const frpServerSourceManualPanelEl = document.getElementById("frp-server-source-manual-panel");
const frpServerSourceSystemPanelEl = document.getElementById("frp-server-source-system-panel");
const frpServerSourceSystemSelectEl = document.getElementById("frp-server-source-system-select");
const frpServerSourcePublicAddrInputEl = document.getElementById("frp-server-source-public-addr-input");
const frpServerSourcePublicPortInputEl = document.getElementById("frp-server-source-public-port-input");
const frpServerSystemPublicAddrInputEl = document.getElementById("frp-server-system-public-addr-input");
const frpServerSourceTestBtnEl = document.getElementById("frp-server-source-test-btn");
const frpServerSourceAddBtnEl = document.getElementById("frp-server-source-add-btn");
const frpServerSourceAdoptSelectedBtnEl = document.getElementById("frp-server-source-adopt-selected-btn");
const frpServerSourceStatusEl = document.getElementById("frp-server-source-status");
let frpManager = null;
let proxyManager = null;
let authManager = null;
let apiManager = null;
let claudeManager = null;
let accountClipboardManager = null;
let presetSyncManager = null;
let compileStatusManager = null;
let systemPanelManager = null;
const systemArgsEl = document.getElementById("system-args");
const systemEnvEl = document.getElementById("system-env");
const systemProxyStatusEl = document.getElementById("system-proxy-status");
const systemCopyFromAppProxyBtnEl = document.getElementById("system-copy-from-app-proxy-btn");
const systemRestartBtnEl = document.getElementById("system-restart-btn");
const systemLogoutBtnEl = document.getElementById("system-logout-btn");
const systemAppProxyActiveEl = document.getElementById("system-app-proxy-active");
const systemAppProxyEnvEl = document.getElementById("system-app-proxy-env");
const systemProcessProxySummaryEl = document.getElementById("system-process-proxy-summary");
const systemFileProxySummaryEl = document.getElementById("system-file-proxy-summary");
const systemUserShellProxySummaryEl = document.getElementById("system-user-shell-proxy-summary");
const systemUserShellProxyEnvEl = document.getElementById("system-user-shell-proxy-env");
const systemSystemProxyFilePathEl = document.getElementById("system-system-proxy-file-path");
const systemProxyHttpInputEl = document.getElementById("system-proxy-http-input");
const systemProxyHttpsInputEl = document.getElementById("system-proxy-https-input");
const systemProxyAllInputEl = document.getElementById("system-proxy-all-input");
const systemProxyNoInputEl = document.getElementById("system-proxy-no-input");
const systemSaveProxyBtnEl = document.getElementById("system-save-proxy-btn");
const systemClearProxyBtnEl = document.getElementById("system-clear-proxy-btn");
const authCurrentFileEl = document.getElementById("auth-current-file");
const authCurrentAccountEl = document.getElementById("auth-current-account");
const authConfigFileEl = document.getElementById("auth-config-file");
const authPresetFileEl = document.getElementById("auth-preset-file");
const authPresetNameEl = document.getElementById("auth-preset-name");
const authConfigOverridesDetailsEl = document.getElementById("auth-config-overrides-details");
const authConfigOverridesSummaryEl = document.getElementById("auth-config-overrides-summary");
const authConfigOverridesListEl = document.getElementById("auth-config-overrides-list");
const authAddConfigOverrideButton = document.getElementById("auth-add-config-override");
const authPresetInputEl = document.getElementById("auth-preset-input");
const authImportFileButton = document.getElementById("auth-import-file-button");
const authImportFileInputEl = document.getElementById("auth-import-file");
const authClipboardImportButton = document.getElementById("auth-clipboard-import");
const authClipboardExportButton = document.getElementById("auth-clipboard-export");
const apiAccountImportFileButton = document.getElementById("api-account-import-file-button");
const apiAccountImportFileInputEl = document.getElementById("api-account-import-file");
const apiAccountImportTextButton = document.getElementById("api-account-import-text-button");
const apiAccountImportDialogEl = document.getElementById("api-account-import-dialog");
const apiAccountImportFormEl = document.getElementById("api-account-import-form");
const apiAccountImportTextEl = document.getElementById("api-account-import-text");
const apiAccountImportCancelButton = document.getElementById("api-account-import-cancel");
const apiAccountImportSubmitButton = document.getElementById("api-account-import-submit");
const apiAccountImportStatusEl = document.getElementById("api-account-import-dialog-status");
const apiAccountImportInlineStatusEl = document.getElementById("api-account-import-status");
const apiClipboardImportButton = document.getElementById("api-clipboard-import");
const apiClipboardExportButton = document.getElementById("api-clipboard-export");
const authSavePresetButton = document.getElementById("auth-save-preset");
const authSaveAsNewPresetButton = document.getElementById("auth-save-as-new-preset");
const authApplyEditedPresetButton = document.getElementById("auth-apply-edited-preset");
const authClearInputButton = document.getElementById("auth-clear-input");
const authRefreshAllQuotaButton = document.getElementById("auth-refresh-all-quotas");
const authTestAllPresetsButton = document.getElementById("auth-test-all-presets");
const authFormStatusEl = document.getElementById("auth-form-status");
const authPresetListEl = document.getElementById("auth-preset-list");
const apiManagerStatusEl = document.getElementById("api-manager-status");
const apiCurrentFileEl = document.getElementById("api-current-file");
const apiConfigFileEl = document.getElementById("api-config-file");
const apiCurrentTargetEl = document.getElementById("api-current-target");
const apiPresetFileEl = document.getElementById("api-preset-file");
const apiPresetGroupModeInputEl = document.getElementById("api-preset-group-mode");
const apiPresetSearchInputEl = document.getElementById("api-preset-search");
const apiPresetSelectionModeButtonEl = document.getElementById("api-preset-selection-mode");
const apiPresetNameEl = document.getElementById("api-preset-name");
const apiPresetRowNumberEl = document.getElementById("api-preset-row-number");
const apiConfigOverridesDetailsEl = document.getElementById("api-config-overrides-details");
const apiConfigOverridesSummaryEl = document.getElementById("api-config-overrides-summary");
const apiConfigOverridesListEl = document.getElementById("api-config-overrides-list");
const apiAddConfigOverrideButton = document.getElementById("api-add-config-override");
const apiKeyInputEl = document.getElementById("api-key-input");
const apiBaseUrlInputEl = document.getElementById("api-base-url-input");
const apiBaseUrlPresetsEl = document.getElementById("api-base-url-presets");
const apiWireApiInputEl = document.getElementById("api-wire-api-input");
const apiResponsesProxyInputEl = document.getElementById("api-responses-proxy-input");
const apiModelInputEl = document.getElementById("api-model-input");
const apiModelPresetsEl = document.getElementById("api-model-presets");
const apiManagementUrlInputEl = document.getElementById("api-management-url-input");
const apiManagementUrlPanelEl = document.getElementById("api-management-url-panel");
const apiManagementUrlSameAsBaseInputEl = document.getElementById("api-management-url-same-as-base");
const apiApplyUpstreamProxyOnSwitchInputEl = document.getElementById("api-apply-upstream-proxy-on-switch");
const apiTerminalStartupDetailsEl = document.getElementById("api-terminal-startup-details");
const apiTerminalEnvInputEl = document.getElementById("api-terminal-env-input");
const apiTerminalStartupScriptInputEl = document.getElementById("api-terminal-startup-script-input");
const apiAddPresetButton = document.getElementById("api-add-preset");
const apiPresetEditorPanelEl = document.getElementById("api-preset-editor-panel");
const apiSavePresetButton = document.getElementById("api-save-preset");
const apiSaveAsNewPresetButton = document.getElementById("api-save-as-new-preset");
const apiApplyEditedPresetButton = document.getElementById("api-apply-edited-preset");
const apiClearInputButton = document.getElementById("api-clear-input");
const apiTestAllPresetsButton = document.getElementById("api-test-all-presets");
const apiFormStatusEl = document.getElementById("api-form-status");
const apiPresetListEl = document.getElementById("api-preset-list");
const apiPresetMobileListEl = document.getElementById("api-preset-mobile-list");
const claudeManagerStatusEl = document.getElementById("claude-manager-status");
const claudeCurrentFileEl = document.getElementById("claude-current-file");
const claudeCurrentTargetEl = document.getElementById("claude-current-target");
const claudePresetFileEl = document.getElementById("claude-preset-file");
const claudeProviderNameInputEl = document.getElementById("claude-provider-name-input");
const claudeAuthTokenInputEl = document.getElementById("claude-auth-token-input");
const claudeBaseUrlInputEl = document.getElementById("claude-base-url-input");
const claudeBaseUrlPresetsEl = document.getElementById("claude-base-url-presets");
const claudeManagementUrlInputEl = document.getElementById("claude-management-url-input");
const claudeDefaultHaikuModelInputEl = document.getElementById("claude-default-haiku-model-input");
const claudeDefaultSonnetModelInputEl = document.getElementById("claude-default-sonnet-model-input");
const claudeDefaultOpusModelInputEl = document.getElementById("claude-default-opus-model-input");
const claudeThirdPartyModelInputEl = document.getElementById("claude-third-party-model-input");
const claudeModelModeOfficialInputEl = document.getElementById("claude-model-mode-official");
const claudeModelModeThirdPartyInputEl = document.getElementById("claude-model-mode-third-party");
const claudeOfficialModelGroupEl = document.getElementById("claude-official-model-settings");
const claudeThirdPartyModelGroupEl = document.getElementById("claude-third-party-model-settings");
const claudeConfigOverridesDetailsEl = document.getElementById("claude-config-overrides-details");
const claudeConfigOverridesSummaryEl = document.getElementById("claude-config-overrides-summary");
const claudeConfigOverridesListEl = document.getElementById("claude-config-overrides-list");
const claudeAddConfigOverrideButton = document.getElementById("claude-add-config-override");
const claudeSavePresetButton = document.getElementById("claude-save-preset");
const claudeSaveAsNewPresetButton = document.getElementById("claude-save-as-new-preset");
const claudeApplyEditedPresetButton = document.getElementById("claude-apply-edited-preset");
const claudeClearInputButton = document.getElementById("claude-clear-input");
const claudeTestAllPresetsButton = document.getElementById("claude-test-all-presets");
const claudeClipboardImportButton = document.getElementById("claude-clipboard-import");
const claudeClipboardExportButton = document.getElementById("claude-clipboard-export");
const claudeFormStatusEl = document.getElementById("claude-form-status");
const claudePresetListEl = document.getElementById("claude-preset-list");
const claudeUpstreamProxyToggleEl = document.getElementById("claude-upstream-proxy-toggle");
const claudeAccessModeInputEl = document.getElementById("claude-access-mode-input");
const authApplyInputButton = document.getElementById("auth-apply-input");
const authConfigOverrideControls = {
  detailsEl: authConfigOverridesDetailsEl,
  summaryEl: authConfigOverridesSummaryEl,
  listEl: authConfigOverridesListEl,
  addButton: authAddConfigOverrideButton,
  emptyMessage: "当前没有额外覆盖项。",
};
const apiConfigOverrideControls = {
  detailsEl: apiConfigOverridesDetailsEl,
  summaryEl: apiConfigOverridesSummaryEl,
  listEl: apiConfigOverridesListEl,
  addButton: apiAddConfigOverrideButton,
  emptyMessage: "当前没有额外覆盖项。",
};
const claudeConfigOverrideControls = {
  detailsEl: claudeConfigOverridesDetailsEl,
  summaryEl: claudeConfigOverridesSummaryEl,
  listEl: claudeConfigOverridesListEl,
  addButton: claudeAddConfigOverrideButton,
  emptyMessage: "当前没有额外 env 选项。",
};

const hasDirectorySessionControls = Boolean(directorySessionListEl);
const hasSessionsSessionControls = Boolean(sessionsSessionListEl);
const hasTerminalArchiveControls = Boolean(terminalArchivesListEl);
let sessionViewsRefreshTimer = null;
let pendingSessionViewsRefresh = null;
let sessionsStatusTimer = null;
let sessionSelectInteractionFlushTimer = null;
const tableCardStatusTimers = new WeakMap();

function updateStatus(element, message, tone) {
  if (!element) {
    return;
  }
  element.hidden = false;
  element.textContent = message;
  element.dataset.tone = tone;
}

function updateTableCardStatus(element, message, tone, { sticky = String(message || "").includes("正在") } = {}) {
  if (!element) {
    return;
  }
  const existingTimer = tableCardStatusTimers.get(element);
  if (existingTimer !== undefined) {
    window.clearTimeout(existingTimer);
    tableCardStatusTimers.delete(element);
  }
  updateStatus(element, message, tone);
  if (sticky) {
    return;
  }
  const delay = tone === "warn" ? 6000 : 2800;
  const timer = window.setTimeout(() => {
    tableCardStatusTimers.delete(element);
    element.hidden = true;
    element.textContent = "";
  }, delay);
  tableCardStatusTimers.set(element, timer);
}

function updateSessionsStatus(message, tone = "muted", { sticky = false } = {}) {
  if (!sessionsStatusEl) {
    return;
  }
  if (sessionsStatusTimer !== null) {
    window.clearTimeout(sessionsStatusTimer);
    sessionsStatusTimer = null;
  }
  updateStatus(sessionsStatusEl, message, tone);
  if (sticky) {
    return;
  }
  const delay = tone === "warn" ? 6000 : 2800;
  sessionsStatusTimer = window.setTimeout(() => {
    sessionsStatusTimer = null;
    sessionsStatusEl.hidden = true;
    sessionsStatusEl.textContent = "";
  }, delay);
}

function setTextContent(element, value) {
  if (!element) {
    return;
  }
  element.textContent = value;
}

function setInlineStatus(element, message, tone = "muted", hidden = false) {
  if (!element) {
    return;
  }
  element.textContent = message;
  element.dataset.tone = tone;
  element.hidden = hidden;
}

function showToast(message, tone = "info", duration = 0) {
  const container = document.getElementById("webclx-toast-container");
  if (!container) {
    return;
  }
  const item = document.createElement("div");
  item.className = "webclx-toast-item";
  item.dataset.tone = tone;
  item.textContent = message;
  container.appendChild(item);
  requestAnimationFrame(() => item.classList.add("show"));

  if (duration > 0) {
    setTimeout(() => {
      item.classList.remove("show");
      setTimeout(() => item.remove(), 300);
    }, duration);
  }
  return item;
}

function updateAuthOauthStatus(message, tone = "muted") {
  setInlineStatus(authOauthStatusEl, message, tone, !message);
}

applyThemeMode(state.themeMode);

if (window.matchMedia) {
  window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", () => {
    if (state.themeMode === "system") {
      applyThemeMode("system");
    }
  });
}

function enableQuickSelectInput(input) {
  if (!input) {
    return;
  }

  input.addEventListener("mousedown", () => {
    input.dataset.quickSelectMouse = document.activeElement === input ? "" : "1";
  });

  input.addEventListener("focus", () => {
    window.requestAnimationFrame(() => {
      if (document.activeElement === input) {
        input.select();
      }
    });
  });

  input.addEventListener("mouseup", (event) => {
    if (input.dataset.quickSelectMouse === "1") {
      event.preventDefault();
      input.dataset.quickSelectMouse = "";
    }
  });

  input.addEventListener("blur", () => {
    input.dataset.quickSelectMouse = "";
  });
}

function focusTextInputToEnd(input) {
  if (!input) {
    return;
  }

  input.focus();
  if (typeof input.setSelectionRange === "function") {
    const cursor = input.value.length;
    input.setSelectionRange(cursor, cursor);
  }
}

function sessionRenameDraftName(sessionName) {
  const base = String(sessionName || "").trim();
  return `${base}_`;
}

function sessionRenameSavedName(sessionName) {
  return String(sessionName || "").trim().replace(/_+$/, "");
}

function displayPath(path) {
  return path ? `/${path}` : "/";
}

function renderCurrentPath(
  displayValue = state.currentDirectory?.display_path || state.workspaceDir || "/",
  basePath = state.workspaceDir,
) {
  const absolutePath = normalizeAbsolutePath(displayValue || "/");
  currentPathEl.textContent = "";
  currentPathEl.setAttribute("aria-label", `当前目录路径：${absolutePath}`);

  if (!basePath) {
    currentPathEl.textContent = absolutePath;
    return;
  }

  // When showFullPath is true, display the full absolute path.
  // Otherwise abbreviate the current user home directory prefix as ~.
  const prefix = state.showFullPath ? "" : state.terminalUserHome || "";
  const parts = splitPathParts(absolutePath);
  let skipParts = 0;

  if (prefix && (absolutePath === prefix || absolutePath.startsWith(prefix + "/"))) {
    skipParts = splitPathParts(prefix).length;
  }

  const fragment = document.createDocumentFragment();
  const visibleParts = parts.slice(skipParts);

  if (skipParts > 0) {
    const tilde = document.createElement("a");
    tilde.className = "browser-breadcrumb-link";
    tilde.href = buildWorkspaceUrl(relativePathBetweenAbsolute(basePath, prefix));
    tilde.textContent = "~";
    tilde.addEventListener("click", (event) => {
      event.preventDefault();
      navigateTo(relativePathBetweenAbsolute(basePath, prefix));
    });
    fragment.appendChild(tilde);
  } else {
    const root = document.createElement("span");
    root.className = "browser-breadcrumb-separator";
    root.textContent = "/";
    fragment.appendChild(root);
  }

  if (visibleParts.length === 0) {
    currentPathEl.appendChild(fragment);
    return;
  }

  if (skipParts > 0) {
    const sep = document.createElement("span");
    sep.className = "browser-breadcrumb-separator";
    sep.textContent = "/";
    fragment.appendChild(sep);
  }

  visibleParts.forEach((part, index) => {
    const absolutePartIndex = skipParts + index;
    const targetAbsolutePath = `/${parts.slice(0, absolutePartIndex + 1).join("/")}`;
    const targetPath = relativePathBetweenAbsolute(basePath, targetAbsolutePath);
    const link = document.createElement("a");
    link.className = "browser-breadcrumb-link";
    link.href = buildWorkspaceUrl(targetPath);
    link.textContent = part;
    link.addEventListener("click", (event) => {
      event.preventDefault();
      navigateTo(targetPath);
    });
    fragment.appendChild(link);

    if (index < visibleParts.length - 1) {
      const separator = document.createElement("span");
      separator.className = "browser-breadcrumb-separator";
      separator.textContent = "/";
      fragment.appendChild(separator);
    }
  });

  currentPathEl.appendChild(fragment);
}

async function copyCurrentPath(button) {
  const path = normalizeAbsolutePath(state.currentDirectory?.display_path || state.workspaceDir || "/");
  let copied = false;

  if (navigator.clipboard?.writeText) {
    try {
      await navigator.clipboard.writeText(path);
      copied = true;
    } catch {
      // Some browsers only allow execCommand while handling the original click.
    }
  }

  if (!copied) {
    const textarea = document.createElement("textarea");
    textarea.value = path;
    textarea.setAttribute("readonly", "true");
    textarea.style.position = "fixed";
    textarea.style.opacity = "0";
    textarea.style.pointerEvents = "none";
    document.body.appendChild(textarea);
    textarea.focus();
    textarea.select();
    try {
      copied = Boolean(document.execCommand?.("copy"));
    } catch {
      copied = false;
    } finally {
      textarea.remove();
    }
  }

  if (copied) {
    const previousLabel = button?.textContent;
    if (button) {
      button.textContent = "已复制";
      window.setTimeout(() => {
        if (button.textContent === "已复制") {
          button.textContent = previousLabel;
        }
      }, 1200);
    }
    updateStatus(fileStatusEl, `已复制当前路径：${path}`, "ok");
    showToast("已复制当前路径。", "ok", 2000);
  } else {
    updateStatus(fileStatusEl, "复制当前路径失败，请手动选择路径复制。", "warn");
    showToast("复制当前路径失败。", "warn", 4000);
  }
  return copied;
}

function redirectToLogin() {
  // 保留当前路径作为 next 参数，登录成功后跳回。
  const next = window.location.pathname + window.location.search;
  window.location.assign(`/login?next=${encodeURIComponent(next)}`);
}

async function requestJson(url, options = {}) {
  const nextOptions = { ...options };
  if (nextOptions.body && !(nextOptions.body instanceof FormData)) {
    const headers = new Headers(nextOptions.headers || {});
    if (!headers.has("Content-Type")) {
      headers.set("Content-Type", "application/json");
    }
    nextOptions.headers = headers;
  }
  const response = await fetch(url, nextOptions);
  if (response.status === 401) {
    redirectToLogin();
    throw new Error("未登录，正在跳转登录页");
  }
  if (!response.ok) {
    const message = await response.text();
    throw new Error(message || `请求失败: ${response.status}`);
  }
  return response.json();
}

function setButtonBusy(button, isBusy, busyLabel = "") {
  if (!button) {
    return;
  }
  if (!button.dataset.defaultLabel) {
    button.dataset.defaultLabel = button.textContent;
  }
  button.disabled = isBusy;
  button.textContent = isBusy && busyLabel ? busyLabel : button.dataset.defaultLabel;
}

function setSettingsConfigFileBusy(isBusy, activeButton = null, busyLabel = "") {
  [settingsConfigFileRefreshButtonEl, settingsConfigFileSaveButtonEl].forEach((button) => {
    if (!button) {
      return;
    }
    if (!button.dataset.defaultLabel) {
      button.dataset.defaultLabel = button.textContent;
    }
    button.disabled = isBusy;
    button.textContent = isBusy && button === activeButton && busyLabel
      ? busyLabel
      : button.dataset.defaultLabel;
  });
  if (settingsConfigFileSelectEl) {
    settingsConfigFileSelectEl.disabled = isBusy;
  }
}

function renderSettingsConfigFileOptions(options, selectedKey) {
  if (!settingsConfigFileSelectEl) {
    return;
  }
  settingsConfigFileSelectEl.innerHTML = "";
  const groups = new Map();
  (Array.isArray(options) ? options : []).forEach((option) => {
    const groupName = option.group || "配置";
    let groupEl = groups.get(groupName);
    if (!groupEl) {
      groupEl = document.createElement("optgroup");
      groupEl.label = groupName;
      groups.set(groupName, groupEl);
      settingsConfigFileSelectEl.appendChild(groupEl);
    }
    const optionEl = document.createElement("option");
    optionEl.value = option.key;
    const optionPath = option.display_path || option.relative_path || option.label || option.key;
    optionEl.textContent = `${optionPath}${option.exists ? "" : "（未创建）"}`;
    optionEl.title = option.display_path || option.relative_path || "";
    groupEl.appendChild(optionEl);
  });
  settingsConfigFileSelectEl.value = selectedKey || state.settingsConfigFileKey || "codex_config";
}

function applySettingsConfigFileResponse(data, { saved = false } = {}) {
  state.settingsConfigFileKey = data.selected_key || state.settingsConfigFileKey || "codex_config";
  state.settingsConfigFileLoaded = true;
  state.settingsConfigFileDirty = false;
  state.settingsConfigFileOptions = Array.isArray(data.options) ? data.options : [];
  renderSettingsConfigFileOptions(state.settingsConfigFileOptions, state.settingsConfigFileKey);
  if (settingsConfigFilePathEl) {
    settingsConfigFilePathEl.textContent = data.display_path || data.path || "未选择";
  }
  if (settingsConfigFileMetaEl) {
    const user = data.user ? `用户 ${data.user}` : "当前用户";
    const status = data.exists ? "已存在" : "未创建";
    settingsConfigFileMetaEl.textContent = `${user} | ${status}`;
  }
  if (settingsConfigFileEditorEl) {
    settingsConfigFileEditorEl.value = typeof data.content === "string" ? data.content : "";
    settingsConfigFileEditorEl.disabled = false;
  }
  updateStatus(
    settingsConfigFileStatusEl,
    saved
      ? `已保存 ${data.display_path || data.path || "配置文件"}。`
      : `已读取 ${data.display_path || data.path || "配置文件"}。`,
    saved ? "ok" : "info",
  );
}

async function loadSettingsConfigFile(key = state.settingsConfigFileKey || "codex_config") {
  if (!settingsConfigFileEditorEl) {
    return;
  }
  setSettingsConfigFileBusy(true, settingsConfigFileRefreshButtonEl, "刷新中…");
  updateStatus(settingsConfigFileStatusEl, "正在读取配置文件…", "info");
  try {
    const data = await requestJson(`/api/settings/config-file?key=${encodeURIComponent(key)}`);
    applySettingsConfigFileResponse(data);
  } catch (error) {
    updateStatus(settingsConfigFileStatusEl, error.message, "warn");
  } finally {
    setSettingsConfigFileBusy(false);
  }
}

async function saveSettingsConfigFile() {
  if (!settingsConfigFileEditorEl) {
    return;
  }
  const key = settingsConfigFileSelectEl?.value || state.settingsConfigFileKey || "codex_config";
  setSettingsConfigFileBusy(true, settingsConfigFileSaveButtonEl, "保存中…");
  updateStatus(settingsConfigFileStatusEl, "正在保存配置文件…", "info");
  try {
    const data = await requestJson("/api/settings/config-file", {
      method: "PUT",
      body: JSON.stringify({
        key,
        content: settingsConfigFileEditorEl.value,
      }),
    });
    applySettingsConfigFileResponse(data, { saved: true });
  } catch (error) {
    updateStatus(settingsConfigFileStatusEl, error.message, "warn");
  } finally {
    setSettingsConfigFileBusy(false);
  }
}

function buildTerminalUrl(
  path,
  sessionId = "",
  { fresh = false, quickStart = false, runCommand = "" } = {},
) {
  const params = new URLSearchParams();
  if (path) {
    params.set("path", path);
  }
  if (sessionId) {
    params.set("session", sessionId);
  } else if (fresh) {
    params.set("fresh", "1");
  }
  if (quickStart) {
    params.set("quick_start", "1");
  }
  if (runCommand) {
    params.set("run", runCommand);
  }
  return params.toString() ? `/terminal?${params.toString()}` : "/terminal";
}

function buildFreshTerminalUrl(path) {
  return buildTerminalUrl(path, "", { fresh: true, quickStart: true });
}

function shouldUseNativeLinkNavigation(event) {
  return Boolean(
    event.defaultPrevented ||
      event.button !== 0 ||
      event.metaKey ||
      event.ctrlKey ||
      event.shiftKey ||
      event.altKey,
  );
}

function restoredTerminalNameForAttempt(terminalName, attempt, freshTerminalName = "") {
  const restoredTerminalName = String(terminalName || "").trim();
  if (attempt <= 1) {
    return restoredTerminalName;
  }

  const freshName = String(freshTerminalName || "").trim();
  const freshAutoMatch = freshName.match(/^(.*?)([_#])\d+$/);
  if (!freshAutoMatch || !restoredTerminalName.startsWith(freshAutoMatch[1])) {
    return `${restoredTerminalName}${attempt}`;
  }

  const restoredTail = restoredTerminalName.slice(freshAutoMatch[1].length);
  const restoredAutoMatch = restoredTail.match(/^[_#]\d+(?=$|[_\s])/);
  if (!restoredAutoMatch) {
    return `${restoredTerminalName}${attempt}`;
  }

  const suffix = restoredTail
    .slice(restoredAutoMatch[0].length)
    .replace(/^[_#\s]+/, "")
    .replace(/([_#])(\d+)(?=$|[_\s])/g, "$1n$2");
  return suffix ? `${freshName}_${suffix}` : freshName;
}

function isRestoredTerminalNameConflict(error) {
  const message = String(error?.message || error || "");
  return message.includes("名称") && (message.includes("已存在") || message.includes("自动编号已被"));
}

async function renameFreshTerminalForRestore(session, requestedPath, terminalName) {
  const restoredTerminalName = String(terminalName || "").trim();
  if (!restoredTerminalName || restoredTerminalName === session.name) {
    return session;
  }

  const maxAttempts = 8;
  const fallbackName = restoredTerminalNameForAttempt(restoredTerminalName, 2, session.name);
  for (let attempt = 1; attempt <= maxAttempts; attempt += 1) {
    const candidateName = restoredTerminalNameForAttempt(
      restoredTerminalName,
      attempt,
      session.name,
    );
    if (candidateName === session.name) {
      return session;
    }
    try {
      const renamedSession = await requestJson(
        `/api/terminal/sessions/${encodeURIComponent(session.id)}`,
        {
          method: "PUT",
          headers: {
            "Content-Type": "application/json",
          },
          body: JSON.stringify({
            path: session.path || requestedPath,
            name: candidateName,
          }),
        },
      );
      announceSessionMutation("renamed", renamedSession);
      return renamedSession;
    } catch (error) {
      if (!isRestoredTerminalNameConflict(error)) {
        throw error;
      }
      if (
        attempt > 1 &&
        candidateName === fallbackName &&
        fallbackName !== `${restoredTerminalName}${attempt}`
      ) {
        return session;
      }
    }
  }

  return session;
}

async function openFreshTerminalSession(
  path,
  { runCommand = "", quickStart = !runCommand, terminalName = "" } = {},
) {
  const requestedPath = String(path || "").trim().startsWith("/")
    ? normalizeAbsolutePath(path)
    : normalizeRelativePath(path);
  const command = String(runCommand || "");

  let session;
  try {
    session = await requestJson("/api/terminal/sessions", {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        path: requestedPath,
      }),
    });
  } catch {
    window.location.assign(
      buildTerminalUrl(requestedPath, "", {
        fresh: true,
        quickStart,
        runCommand: command,
      }),
    );
    return;
  }

  announceSessionMutation("created", session);
  try {
    session = await renameFreshTerminalForRestore(session, requestedPath, terminalName);
  } catch (error) {
    updateStatus(
      workspaceHistoryStatusEl,
      `恢复会话前未能还原终端名称：${error.message || "改名失败"}`,
      "warn",
    );
  }

  rememberPreferredSession(session.path || requestedPath, session.id);
  window.location.assign(
    buildTerminalUrl(session.path || requestedPath, session.id, {
      quickStart,
      runCommand: command,
    }),
  );
}

function openFreshTerminalLink(event, path) {
  if (shouldUseNativeLinkNavigation(event)) {
    return;
  }

  event.preventDefault();
  openFreshTerminalSession(path);
}

function openFreshTerminalRunLink(event, path, command, {
  beforeNavigate = null,
  terminalName = "",
} = {}) {
  if (shouldUseNativeLinkNavigation(event)) {
    if (typeof beforeNavigate === "function") {
      beforeNavigate();
    }
    return;
  }

  event.preventDefault();
  if (typeof beforeNavigate === "function") {
    beforeNavigate();
  }
  openFreshTerminalSession(path, {
    runCommand: command,
    quickStart: false,
    terminalName,
  });
}

function buildTerminalRunUrl(command) {
  return buildTerminalUrl(state.currentPath, "", {
    fresh: true,
    runCommand: command,
  });
}

function updateEditorState() {
  const authImportAvailable = state.currentFileEditable && isCodexAuthJsonPath(state.currentFilePath);
  importAuthButton.hidden = !isCodexAuthJsonPath(state.currentFilePath);
  importAuthButton.disabled = !authImportAvailable;
  saveButton.disabled = !state.currentFileEditable || !state.currentFilePath || !state.dirty;
}

function isCodexAuthJsonPath(path) {
  return typeof path === "string" && /(^|\/)\.codex\/auth\.json$/.test(path);
}

const remoteCopyDialogEl = document.getElementById("remote-copy-dialog");
const remoteCopyUrlInputEl = document.getElementById("remote-copy-url");
const remoteCopyModeSelectEl = document.getElementById("remote-copy-mode");
const remoteCopyStatusEl = document.getElementById("remote-copy-status");
const remoteCopySubmitBtnEl = document.getElementById("remote-copy-submit");
const remoteCopyCancelBtnEl = document.getElementById("remote-copy-cancel");

function openRemoteCopyDialog() {
  remoteCopyUrlInputEl.value = "";
  const tabOption = remoteCopyModeSelectEl.querySelector('option[value="tab"]');
  const supportsTabCopy = settingsTabSupportsRemoteCopy(state.activeSettingsTab);
  if (tabOption) {
    tabOption.disabled = !supportsTabCopy;
  }
  remoteCopyModeSelectEl.value = supportsTabCopy ? "tab" : "all";
  remoteCopyStatusEl.textContent = supportsTabCopy
    ? ""
    : "当前分类使用独立配置存储，只能拷贝通用设置。";
  remoteCopyStatusEl.dataset.tone = "muted";
  if (typeof remoteCopyDialogEl.showModal === "function") {
    if (!remoteCopyDialogEl.open) {
      remoteCopyDialogEl.showModal();
    }
  } else {
    remoteCopyDialogEl.setAttribute("open", "");
  }
  window.requestAnimationFrame(() => {
    remoteCopyUrlInputEl.focus();
  });
}

function closeRemoteCopyDialog() {
  if (typeof remoteCopyDialogEl.close === "function") {
    if (remoteCopyDialogEl.open) {
      remoteCopyDialogEl.close();
    }
  } else {
    remoteCopyDialogEl.removeAttribute("open");
  }
}

async function performRemoteCopy() {
  const remoteUrl = remoteCopyUrlInputEl.value.trim();
  if (!remoteUrl) {
    remoteCopyStatusEl.textContent = "请输入远程地址";
    remoteCopyStatusEl.dataset.tone = "error";
    return;
  }

  const mode = remoteCopyModeSelectEl.value;
  const tab = state.activeSettingsTab || "system";

  remoteCopySubmitBtnEl.disabled = true;
  remoteCopyStatusEl.textContent = "正在连接远程服务器...";
  remoteCopyStatusEl.dataset.tone = "muted";

  try {
    let response;
    if (mode === "tab") {
      response = await requestJson("/api/settings/merge-tab", {
        method: "POST",
        body: JSON.stringify({ remote_url: remoteUrl, tab }),
      });
    } else {
      response = await requestJson("/api/settings/merge-all", {
        method: "POST",
        body: JSON.stringify({ remote_url: remoteUrl }),
      });
    }

    remoteCopyStatusEl.textContent = "拷贝成功！正在刷新设置...";
    remoteCopyStatusEl.dataset.tone = "success";
    closeRemoteCopyDialog();

    await loadSettings();
    updateStatus(settingsStatusEl, "配置已从远程更新", "info");
  } catch (error) {
    remoteCopyStatusEl.textContent = error.message || "拷贝失败";
    remoteCopyStatusEl.dataset.tone = "error";
  } finally {
    remoteCopySubmitBtnEl.disabled = false;
  }
}

if (remoteCopyDialogEl) {
  remoteCopyDialogEl.addEventListener("click", (event) => {
    if (event.target === remoteCopyDialogEl) {
      closeRemoteCopyDialog();
    }
  });

  document.getElementById("remote-copy-cancel")?.addEventListener("click", () => {
    closeRemoteCopyDialog();
  });

  document.getElementById("remote-copy-submit")?.addEventListener("click", (event) => {
    event.preventDefault();
    performRemoteCopy();
  });
}

function tryAutoApplyAuthImportFromDialog() {
  const rawText = authImportTextEl.value.trim();
  if (!rawText) {
    return false;
  }

  try {
    applyAuthImportText(rawText);
    closeAuthImportDialog();
    return true;
  } catch (error) {
    updateStatus(fileStatusEl, error.message, "warn");
    return false;
  }
}

function shouldContinueAuthImport() {
  if (!state.dirty) {
    return true;
  }

  return window.confirm("当前文件有未保存修改，继续会用导入内容覆盖编辑区。是否继续？");
}

function clearEditor(message = "点击左侧文件可在这里查看和修改内容。") {
  state.currentFilePath = "";
  state.currentFileEditable = false;
  state.dirty = false;
  currentFileEl.textContent = "尚未打开文件";
  fileMetaEl.textContent = "仅支持 UTF-8 文本文件";
  editorEl.value = "";
  editorEl.disabled = true;
  if (editorPanelEl) {
    editorPanelEl.hidden = true;
  }
  updateStatus(fileStatusEl, message, "muted");
  updateEditorState();
}

function createActionButton(label, handler, className = "mini-button") {
  const button = document.createElement("button");
  button.type = "button";
  button.className = className;
  button.textContent = label;
  button.addEventListener("click", (event) => {
    event.stopPropagation();
    handler();
  });
  return button;
}

function createActionCell(items, cellClassName = "", wrapClassName = "actions") {
  const cell = document.createElement("td");
  if (cellClassName) {
    cell.className = cellClassName;
  }

  const wrap = document.createElement("div");
  wrap.className = wrapClassName;
  items.filter(Boolean).forEach((item) => wrap.appendChild(item));
  cell.appendChild(wrap);
  return cell;
}

// ── Proxy Presets ──────────────────────────────────────────────────────────

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function formatEnvList(lines, emptyText = '无') {
  return Array.isArray(lines) && lines.length ? lines.join('\n') : emptyText;
}

function formatDateTimeLong(date) {
  if (!(date instanceof Date) || Number.isNaN(date.getTime())) {
    return '—';
  }
  return date.toLocaleString("zh-CN", {
    hour12: false,
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

function parseEnvEntriesToMap(lines) {
  const map = new Map();
  if (!Array.isArray(lines)) return map;
  lines.forEach(line => {
    const index = String(line).indexOf('=');
    if (index <= 0) return;
    map.set(String(line).slice(0, index), String(line).slice(index + 1));
  });
  return map;
}

async function loadFile(path) {
  updateStatus(fileStatusEl, "正在读取文件…", "info");
  try {
    const file = await requestJson(`/api/file?path=${encodeURIComponent(path)}`);
    state.currentFilePath = file.path;
    state.currentFileEditable = file.editable;
    state.dirty = false;
    currentFileEl.textContent = file.display_path;
    fileMetaEl.textContent = `${formatSize(file.size)} · ${file.editable ? "可编辑" : "只读"}`;
    editorEl.value = file.content;
    editorEl.disabled = !file.editable;
    if (editorPanelEl) {
      editorPanelEl.hidden = false;
    }
    updateStatus(fileStatusEl, file.message || "文件已加载。", file.message ? "warn" : "ok");
    updateEditorState();
  } catch (error) {
    currentFileEl.textContent = displayPath(path);
    state.currentFilePath = "";
    state.currentFileEditable = false;
    state.dirty = false;
    editorEl.value = "";
    editorEl.disabled = true;
    if (editorPanelEl) {
      editorPanelEl.hidden = true;
    }
    updateEditorState();
    updateStatus(fileStatusEl, error.message, "warn");
  }
}

async function saveCurrentFile() {
  if (!state.currentFilePath || !state.currentFileEditable) {
    return;
  }

  updateStatus(fileStatusEl, "正在保存文件…", "info");
  saveButton.disabled = true;

  try {
    await requestJson("/api/file", {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        path: state.currentFilePath,
        content: editorEl.value,
      }),
    });
    state.dirty = false;
    updateEditorState();
    updateStatus(fileStatusEl, "保存完成。", "ok");
  } catch (error) {
    updateStatus(fileStatusEl, error.message, "warn");
    updateEditorState();
  }
}

async function importAuthFromClipboard() {
  if (!state.currentFileEditable || !isCodexAuthJsonPath(state.currentFilePath)) {
    return;
  }

  if (!shouldContinueAuthImport()) {
    return;
  }

  if (!navigator.clipboard?.readText) {
    openAuthImportDialog();
    updateStatus(fileStatusEl, "浏览器禁止读取剪贴板，请在弹窗里粘贴内容。", "warn");
    updateEditorState();
    return;
  }

  updateStatus(fileStatusEl, "正在读取剪贴板并转换…", "info");
  importAuthButton.disabled = true;

  try {
    const rawText = await navigator.clipboard.readText();
    if (!rawText.trim()) {
      throw new Error("剪贴板为空。");
    }

    applyAuthImportText(rawText);
  } catch (error) {
    if (error?.name === "NotAllowedError" || error?.name === "SecurityError") {
      openAuthImportDialog();
      updateStatus(fileStatusEl, "浏览器禁止读取剪贴板，请在弹窗里粘贴内容。", "warn");
      updateEditorState();
    } else {
      updateStatus(fileStatusEl, error.message, "warn");
      updateEditorState();
    }
  }
}

function playTerminalCompletionBellTest() {
  try {
    const audio = new Audio(TERMINAL_COMPLETION_BELL_URL);
    audio.preload = "auto";
    audio.volume = 0.72;
    const playResult = audio.play();
    if (playResult && typeof playResult.then === "function") {
      playResult
        .then(() => {
          updateStatus(settingsStatusEl, "已播放测试铃声。", "ok");
        })
        .catch(() => {
          updateStatus(settingsStatusEl, "浏览器暂时阻止播放，请点击页面后重试。", "warn");
        });
      return;
    }
    updateStatus(settingsStatusEl, "已播放测试铃声。", "ok");
  } catch (error) {
    updateStatus(settingsStatusEl, error?.message || "无法播放测试铃声。", "warn");
  }
}

bindCoreEventHandlers();

async function importAuthAccounts(rawText) {
  return await requestJson("/api/auth/api-presets/import", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ raw_text: rawText }),
  });
}

async function importAuthAccountFiles(sourceFiles) {
  const files = Array.from(sourceFiles || []).filter(Boolean);
  const formData = new FormData();
  files.forEach((sourceFile, index) => {
    formData.append("file", sourceFile, sourceFile.name || `accounts-${index + 1}.json`);
  });
  return await requestJson("/api/auth/api-presets/import-file", {
    method: "POST",
    body: formData,
  });
}

initializeFeatureManagers();

function applyAuthImportText(rawText) {
  return authManager.applyAuthImportText(rawText);
}

function openAuthImportDialog(prefill = "") {
  return authManager.openAuthImportDialog(prefill);
}

function closeAuthImportDialog() {
  return authManager.closeAuthImportDialog();
}

function renderAuthOauthSession(session = null) {
  return authManager.renderAuthOauthSession(session);
}

function saveAuthPreset() {
  return authManager.saveAuthPreset();
}

function importAuthJsonFile(sourceFile) {
  return authManager.importAuthJsonFile(sourceFile);
}

function saveAuthPresetAsNew() {
  return authManager.saveAuthPresetAsNew();
}

function applyEditingAuthPreset() {
  return authManager.applyEditingAuthPreset();
}

function startAuthOauthSession() {
  return authManager.startAuthOauthSession();
}

function copyAuthOauthUserCode() {
  return authManager.copyAuthOauthUserCode();
}

function refreshAllAuthPresetQuotas() {
  return authManager.refreshAllAuthPresetQuotas();
}

function testAllAuthPresets() {
  return authManager.testAllAuthPresets();
}

function handleAuthApplyInputAction() {
  return authManager.handleAuthApplyInputAction();
}

bindFrpProxyEventHandlers();

bindSettingsEventHandlers();

bindPresetFormEventHandlers();

bindDesktopFrameEvents();

function init() {
  enhanceWorkspaceIconSelect(directorySessionListEl, () => state.terminalWorkspaceIconPath);
  enhanceWorkspaceIconSelect(sessionsSessionListEl, () => state.terminalWorkspaceIconPath);
  if (systemLogoutBtnEl) {
    systemLogoutBtnEl.addEventListener("click", async () => {
      systemLogoutBtnEl.disabled = true;
      showToast("正在退出登录…", "info");
      try {
        await fetch("/api/auth/logout", { method: "POST" });
      } catch {}
      window.location.assign("/login");
    });
  }
  installPresetTestBadgeListeners();
  clearEditor();
  syncDirectorySessionScopeLabel();
  syncWorkspaceTerminalLink();
  setDirectorySessionPlaceholder(directorySessionLoadingMessage());
  setActiveSettingsTab(state.activeSettingsTab);
  setActiveTab(state.activeTab);

  systemPanelManager.loadUpdatePanel().catch((error) => {
    console.error("update panel init failed:", error);
  });

  loadSettings()
    .then(async () => {
      await loadDirectory();
      await refreshCurrentWorkspaceDirectoryFromTerminal();
    })
    .catch((error) => {
      console.error("init failed:", error);
    });

  function applyUrlTab() {
    const tab = getInitialTab();
    const settingsTab = getInitialSettingsTab();
    if (state.activeTab !== tab) {
      setActiveTab(tab);
    }
    if (state.activeSettingsTab !== settingsTab) {
      setActiveSettingsTab(settingsTab);
    }
  }

  // Path-based navigation (back/forward between /workspace, /settings/...).
  window.addEventListener("popstate", applyUrlTab);
  // Legacy hash-based bookmarks.
  window.addEventListener("hashchange", applyUrlTab);
}

if (document.readyState === "loading") {
  document.addEventListener("DOMContentLoaded", init);
} else {
  init();
}

bindUpdateEventHandlers();
