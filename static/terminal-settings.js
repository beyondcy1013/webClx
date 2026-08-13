(function attachTerminalSettings(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
    return;
  }

  root.WebClxTerminalSettings = factory(root);
})(typeof globalThis !== "undefined" ? globalThis : this, function createTerminalSettings(root) {
  const DEFAULT_TERMINAL_QUICK_COMMANDS = Object.freeze([
    Object.freeze({ key: "1", label: "codex", command: "codex" }),
    Object.freeze({ key: "2", label: "claude", command: "claude" }),
  ]);
  // Toggle-style commands now surfaced as toolbar checkboxes, not dropdown options.
  const TERMINAL_KEYBOARD_CHECKBOX_ACTIONS = Object.freeze(new Set([
    "show_system_keyboard",
    "disable_system_keyboard",
    "enable_touch_selection",
    "disable_touch_selection",
  ]));

  const DEFAULT_TERMINAL_FUNCTION_COMMANDS = Object.freeze([
    Object.freeze({ key: "toggle_soft_keyboard", label: "开启/关闭软键盘", action: "toggle_soft_keyboard", command: "", shortcut: "Ctrl+K" }),
    Object.freeze({ key: "copy_window", label: "新窗口复制", action: "copy_terminal_view_in_new_window", command: "", shortcut: "" }),
    Object.freeze({ key: "reload_claude", label: "重读 Claude", action: "reload_claude", command: "claude", shortcut: "" }),
    Object.freeze({ key: "open_schedule_paste", label: "定时消息", action: "open_schedule_paste", command: "", shortcut: "" }),
    Object.freeze({ key: "toggle_width", label: "宽屏", action: "toggle_terminal_width", command: "", shortcut: "" }),
    Object.freeze({ key: "qoderclicn", label: "qoderclicn", action: "send_text", command: "qodercli --permission-mode bypass_permissions", shortcut: "" }),
    Object.freeze({ key: "agy", label: "agy", action: "send_text", command: "agy", shortcut: "" }),
    Object.freeze({ key: "current_resume_id", label: "session ID", action: "copy_current_resume_id", command: "", shortcut: "" }),
    Object.freeze({ key: "quota", label: "套餐", action: "open_quota_dialog", command: "", shortcut: "Ctrl+Q" }),
    Object.freeze({ key: "enter", label: "Enter", action: "send_sequence", command: "enter", shortcut: "" }),
  ]);
  const DEFAULT_TERMINAL_PAGE_FUNCTION_COMMANDS = Object.freeze([
    ...DEFAULT_TERMINAL_FUNCTION_COMMANDS,
    Object.freeze({ key: "sort_by_path", label: "切换终端排序", action: "sort_directory_sessions_by_path", command: "", shortcut: "Ctrl+Alt+O" }),
    Object.freeze({ key: "sort_by_status", label: "按状态", action: "sort_directory_sessions_by_status", command: "", shortcut: "" }),
    Object.freeze({ key: "save_and_poweroff", label: "保存会话并关机", action: "save_and_poweroff", command: "", shortcut: "" }),
    Object.freeze({ key: "save_and_restart", label: "保存会话并重启服务", action: "save_and_restart", command: "", shortcut: "" }),
  ]);
  const DEFAULT_TERMINAL_SLASH_COMMANDS = Object.freeze([
    Object.freeze({ key: "resume_current_session", label: "恢复会话", action: "resume_current_agent_session", command: "", shortcut: "" }),
    Object.freeze({ key: "continue", label: "继续", action: "send_text", command: "继续", shortcut: "" }),
    Object.freeze({ key: "enter", label: "Enter", action: "send_sequence", command: "enter", shortcut: "" }),
    Object.freeze({ key: "extract_resume", label: "屏幕提取id并恢复", action: "extract_resume", command: "", shortcut: "Ctrl+Alt+R" }),
    Object.freeze({ key: "copy_resume_id", label: "屏幕提取id", action: "copy_resume_id", command: "", shortcut: "" }),
    Object.freeze({ key: "extract_current_session", label: "高级复制", action: "extract_current_session", command: "", shortcut: "Ctrl+Alt+S" }),
    Object.freeze({ key: "current_resume_id", label: "session ID", action: "copy_current_resume_id", command: "", shortcut: "" }),
    Object.freeze({ key: "copy_id_and_ask", label: "复制id并提问", action: "copy_id_and_ask", command: "", shortcut: "" }),
    Object.freeze({ key: "copy_terminal_name", label: "复制终端名", action: "copy_terminal_name", command: "", shortcut: "Ctrl+Alt+T" }),
    Object.freeze({ key: "resume", label: "/resume", action: "send_slash_command", command: "/resume" }),
    Object.freeze({ key: "status", label: "/status", action: "send_slash_command", command: "/status" }),
    Object.freeze({ key: "fork", label: "/fork", action: "send_slash_command", command: "/fork" }),
    Object.freeze({ key: "compact", label: "/compact", action: "send_slash_command", command: "/compact" }),
  ]);
  const TERMINAL_SESSION_ID_COMMAND_KEYS = Object.freeze([
    "extract_resume",
    "copy_resume_id",
    "extract_current_session",
    "current_resume_id",
    "copy_id_and_ask",
  ]);
  const DEFAULT_TERMINAL_COMMAND_COLLECTIONS = Object.freeze([
              ]);
  const BUILTIN_TERMINAL_SHORTCUT_KEYS = new Set([
    "toggle_soft_keyboard",
    "extract_current_session",
    "copy_terminal_name",
    "sort_by_path",
  ]);
  const TERMINAL_TOOL_ROOTS = Object.freeze([
    Object.freeze({ key: "tools", label: "工作流" }),
  ]);
  const TERMINAL_TOOL_ACTION_TYPES = Object.freeze([
    Object.freeze({ key: "create_terminal", label: "新建终端", parameter: "none" }),
    Object.freeze({ key: "fork_session", label: "Fork 会话", parameter: "none" }),
    Object.freeze({ key: "rename_terminal", label: "更名终端", parameter: "text" }),
    Object.freeze({ key: "switch_api_preset", label: "指定临时预设", parameter: "preset" }),
    Object.freeze({ key: "switch_api_preset_revert", label: "恢复上一次临时预设", parameter: "none" }),
    Object.freeze({ key: "codex_exec", label: "Codex 单次任务", parameter: "task" }),
    Object.freeze({ key: "codex_terminal", label: "Codex 终端任务", parameter: "task" }),
    Object.freeze({ key: "codex_launch", label: "启动 Codex 工作流", parameter: "codex_launch" }),
    Object.freeze({ key: "function_command", label: "执行功能命令", parameter: "function_command" }),
    Object.freeze({ key: "run_workflow", label: "嵌套工作流", parameter: "run_workflow" }),
    Object.freeze({ key: "wait", label: "等待", parameter: "seconds" }),
    Object.freeze({ key: "send_command", label: "发送命令", parameter: "command" }),
  ]);
  const DEFAULT_TERMINAL_TOOL_ENTRIES = Object.freeze([
    Object.freeze({
      id: "fork_session",
      root_key: "tools",
      parent_id: null,
      kind: "action",
      label: "fork",
      sort_order: 30,
      actions: Object.freeze([
        Object.freeze({ kind: "fork_session", value: "", seconds: 0 }),
      ]),
    }),
    Object.freeze({
      id: "proxy_settings_workflow",
      root_key: "tools",
      parent_id: null,
      kind: "action",
      label: "代理设置",
      sort_order: 20,
      actions: Object.freeze([
        Object.freeze({
          kind: "codex_launch",
          value: "$mihomo-proxy-ops",
          seconds: 0,
          preset_selector: "miniMax",
          preset_match: "unique_contains",
          cwd: "/home/system",
          project_path: "/home/system",
          terminal_name: "代理设置",
          session_action: "new",
        }),
      ]),
    }),
  ]);
  const DEFAULT_TERMINAL_RENAME_PRESETS = Object.freeze(["完结", "复用对话"]);
  const DEFAULT_TERMINAL_QUICK_START_DEFAULT_KEY = "1";
  const MAX_TERMINAL_QUICK_COMMANDS = 20;
  const MAX_TERMINAL_FUNCTION_COMMANDS = 20;
  const MAX_TERMINAL_COMMAND_COLLECTIONS = 12;
  const MAX_TERMINAL_COMMAND_COLLECTION_ITEMS = 40;
  const DEFAULT_COMMAND_COLLECTION_ITEM_ACTION = "send_text";
  const DEFAULT_FONT_SIZE_TIER_1 = 0.64;
  const DEFAULT_FONT_SIZE_TIER_2 = 0.68;
  const DEFAULT_FONT_SIZE_TIER_3 = 0.72;
  const DEFAULT_FONT_SIZE_TIER_4 = 0.74;
  const DEFAULT_FONT_SIZE_TIERS = Object.freeze([
    DEFAULT_FONT_SIZE_TIER_1,
    DEFAULT_FONT_SIZE_TIER_2,
    DEFAULT_FONT_SIZE_TIER_3,
    DEFAULT_FONT_SIZE_TIER_4,
  ]);
  const DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH = 16;
  const DEFAULT_TERMINAL_FAB_ACTION_COLOR = "#f59e0b";
  const DEFAULT_TERMINAL_FAB_ACTION_OPACITY = 0.5;
  const DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE = 1.08;
  const DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS = 2000;
  const DEFAULT_TERMINAL_SCROLLBACK_LINES = 5000;
  const DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS = 60;
  const DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_FACTOR = 1.5;
  const DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_MAX_MINUTES = 20;
  const DEFAULT_TERMINAL_AUTO_CONTINUE_RESPECT_MANUAL_INTERRUPT = true;
  const DEFAULT_TERMINAL_ACTIVITY_AGENT_DISPLAY = "hidden";
  const DEFAULT_TERMINAL_USER = "root";
  const DEFAULT_THEME_MODE = "system";
  const THEME_MODE_STORAGE_KEY = "webclx:theme-mode";
  const TERMINAL_COMPLETION_BELL_URL = "/api/terminal/completion-bell.wav";

  function normalizeHostName(value) {
    return typeof value === "string" && value.trim() ? value.trim() : "";
  }

  function normalizeTerminalUser(value) {
    return typeof value === "string" && value.trim() ? value.trim() : DEFAULT_TERMINAL_USER;
  }

  function normalizeTerminalActivityAgentDisplay(value) {
    const normalized = String(value || "").trim().toLowerCase();
    if (normalized === "prefix" || normalized === "suffix") {
      return normalized;
    }
    return DEFAULT_TERMINAL_ACTIVITY_AGENT_DISPLAY;
  }

  function cloneDefaultTerminalQuickCommands() {
    return DEFAULT_TERMINAL_QUICK_COMMANDS.map((command) => ({ ...command }));
  }

  function cloneDefaultTerminalFunctionCommands(defaults = DEFAULT_TERMINAL_FUNCTION_COMMANDS) {
    return defaults.map((command) => ({ ...command }));
  }

  function normalizeTerminalQuickText(value, maxLength = 160) {
    return typeof value === "string"
      ? value
          .replace(/[\u0000-\u001f\u007f]/g, "")
          .trim()
          .slice(0, maxLength)
      : "";
  }

  function normalizeTerminalRenamePreset(value) {
    return normalizeTerminalQuickText(value, 24);
  }

  function normalizeTerminalRenamePresets(presets, fallback = DEFAULT_TERMINAL_RENAME_PRESETS) {
    const seen = new Set();
    const normalized = [];
    (Array.isArray(presets) ? presets : []).forEach((preset) => {
      const value = normalizeTerminalRenamePreset(preset);
      if (!value || seen.has(value)) {
        return;
      }
      seen.add(value);
      normalized.push(value);
    });

    if (normalized.length > 0 || fallback === null) {
      return normalized;
    }
    return normalizeTerminalRenamePresets(fallback, null);
  }

  function normalizeTerminalAutoContinueIntervalSeconds(value) {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
      return DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS;
    }
    if (parsed <= 0) {
      return DEFAULT_TERMINAL_AUTO_CONTINUE_INTERVAL_SECONDS;
    }
    return Math.round(Math.min(86400, Math.max(1, parsed)));
  }

  function normalizeTerminalAutoContinueBackoffFactor(value) {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
      return DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_FACTOR;
    }
    if (parsed < 1) {
      return 1;
    }
    return Math.round(Math.min(10, Math.max(1, parsed)) * 10) / 10;
  }

  function normalizeTerminalAutoContinueBackoffMaxMinutes(value) {
    const parsed = Number(value);
    if (!Number.isFinite(parsed) || parsed <= 0) {
      return DEFAULT_TERMINAL_AUTO_CONTINUE_BACKOFF_MAX_MINUTES;
    }
    return Math.round(Math.min(1440, Math.max(1, parsed)));
  }

  function normalizeTerminalQuickCommand(command, options = {}) {
    const key = normalizeTerminalQuickText(command?.key, 8);
    const commandLine =
      normalizeTerminalQuickText(command?.command, 1000) ||
      [normalizeTerminalQuickText(command?.program, 160), normalizeTerminalQuickText(command?.args, 500)]
        .filter(Boolean)
        .join(" ");
    if (!key || /\s/.test(key) || !commandLine) {
      return null;
    }

    const item = {
      key,
      label: normalizeTerminalQuickText(command?.label, 24) || commandLine.slice(0, 24),
      command: commandLine,
    };
    if (options.includeCommandLine) {
      item.commandLine = commandLine;
    }
    return item;
  }

  function normalizeTerminalQuickCommands(
    commands,
    fallback = cloneDefaultTerminalQuickCommands(),
    options = {},
  ) {
    const seen = new Set();
    const normalized = [];
    (Array.isArray(commands) ? commands : []).forEach((command) => {
      if (normalized.length >= MAX_TERMINAL_QUICK_COMMANDS) {
        return;
      }
      const item = normalizeTerminalQuickCommand(command, options);
      if (!item || seen.has(item.key)) {
        return;
      }
      seen.add(item.key);
      normalized.push(item);
    });

    if (normalized.length > 0 || fallback === null) {
      return normalized;
    }
    return normalizeTerminalQuickCommands(fallback, null, options);
  }

  function normalizeTerminalFunctionCommandLine(value, maxLength = 1000) {
    if (typeof value !== "string") {
      return "";
    }
    const withoutControls = value.replace(/[\u0000-\u001f\u007f]/g, "");
    const withoutLeading = withoutControls.trimStart();
    if (!withoutLeading.trim()) {
      return "";
    }
    return withoutLeading.slice(0, maxLength);
  }

  function normalizeTerminalFunctionCommand(command) {
    const key = normalizeTerminalQuickText(command?.key, 64);
    const action = normalizeTerminalQuickText(command?.action, 64);
    const commandLine = normalizeTerminalFunctionCommandLine(command?.command, 1000);
    const label =
      normalizeTerminalQuickText(command?.label, 24) ||
      normalizeTerminalQuickText(commandLine, 1000) ||
      action;
    const shortcut = normalizeTerminalQuickText(command?.shortcut, 80);
    if (!key || /\s/.test(key) || !label || (!action && !commandLine)) {
      return null;
    }
    return {
      key,
      label,
      action,
      command: commandLine,
      shortcut,
    };
  }

  function normalizeTerminalFunctionCommands(
    commands,
    fallback = cloneDefaultTerminalFunctionCommands(),
  ) {
    const seen = new Set();
    const normalized = [];
    (Array.isArray(commands) ? commands : []).forEach((command) => {
      if (normalized.length >= MAX_TERMINAL_FUNCTION_COMMANDS) {
        return;
      }
      const item = normalizeTerminalFunctionCommand(command);
      if (!item || seen.has(item.key)) {
        return;
      }
      seen.add(item.key);
      normalized.push(item);
    });

    if (normalized.length > 0 || fallback === null) {
      return normalized;
    }
    return normalizeTerminalFunctionCommands(fallback, null);
  }

  function isMovedSlashCommand(command) {
    const label = String(command?.label || "").trim().toLowerCase();
    return (
      command?.key === "current_resume_id" ||
      command?.action === "copy_current_resume_id" ||
      label.startsWith("session") ||
      command?.key === "enter" ||
      (command?.action === "send_sequence" && command?.command === "enter") ||
      command?.key === "copy_terminal_name" ||
      command?.action === "copy_terminal_name"
    );
  }

  function isProjectOnlyTerminalCommand(command) {
    return command?.key === "deploy_project" || command?.action === "deploy_project";
  }

  function isFunctionOnlyTerminalCommand(command) {
    return (
      command?.key === "quota" ||
      command?.action === "open_quota_dialog" ||
      command?.key === "webui" ||
      command?.action === "open_project_url"
    );
  }

  function slugifyCommandCollectionKey(label) {
    return String(label || "")
      .trim()
      .replace(/[\s\u0000-\u001f\u007f]+/g, "_")
      .slice(0, 64);
  }

  function normalizeCommandCollectionItem(rawItem) {
    if (!rawItem || typeof rawItem !== "object") {
      return null;
    }
    const commandLine = normalizeTerminalFunctionCommandLine(rawItem.command, 1000);
    const action = normalizeTerminalQuickText(rawItem.action, 64) || DEFAULT_COMMAND_COLLECTION_ITEM_ACTION;
    if (!commandLine) {
      return null;
    }
    const label =
      normalizeTerminalQuickText(rawItem.label, 32) ||
      normalizeTerminalQuickText(commandLine, 32);
    return { label, action, command: commandLine };
  }

  function normalizeCommandCollection(rawCollection, options = {}) {
    if (!rawCollection || typeof rawCollection !== "object") {
      return null;
    }
    const label = normalizeTerminalQuickText(rawCollection.label, 24);
    if (!label) {
      return null;
    }
    let key = normalizeTerminalQuickText(rawCollection.key, 64);
    if (!key || /\s/.test(key)) {
      key = options.fallbackKey || slugifyCommandCollectionKey(label);
    }
    if (!key) {
      return null;
    }
    const seenItemLabels = new Set();
    const commands = [];
    (Array.isArray(rawCollection.commands) ? rawCollection.commands : []).forEach((rawItem) => {
      if (commands.length >= MAX_TERMINAL_COMMAND_COLLECTION_ITEMS) {
        return;
      }
      const item = normalizeCommandCollectionItem(rawItem);
      if (!item || seenItemLabels.has(item.label)) {
        return;
      }
      seenItemLabels.add(item.label);
      commands.push(item);
    });
    return { key, label, commands };
  }

  function normalizeTerminalCommandCollections(
    collections,
    fallback = cloneDefaultTerminalCommandCollections(),
  ) {
    const seenKeys = new Set();
    const normalized = [];
    (Array.isArray(collections) ? collections : []).forEach((rawCollection, index) => {
      if (normalized.length >= MAX_TERMINAL_COMMAND_COLLECTIONS) {
        return;
      }
      const collection = normalizeCommandCollection(rawCollection, { fallbackKey: `collection_${index + 1}` });
      if (!collection || seenKeys.has(collection.key)) {
        return;
      }
      seenKeys.add(collection.key);
      normalized.push(collection);
    });

    if (normalized.length > 0 || fallback === null) {
      return normalized;
    }
    return normalizeTerminalCommandCollections(fallback, null);
  }

  function cloneDefaultTerminalCommandCollections() {
    return DEFAULT_TERMINAL_COMMAND_COLLECTIONS.map((collection) => ({
      key: collection.key,
      label: collection.label,
      commands: collection.commands.map((item) => ({ ...item })),
    }));
  }

  function normalizeAbsoluteWorkflowPath(value) {
    if (typeof value !== "string") {
      return "";
    }
    const trimmed = value.replace(/[\u0000-\u001f\u007f]/g, "").trim();
    if (!trimmed.startsWith("/")) {
      return "";
    }
    return trimmed.slice(0, 1024);
  }

  function normalizeTerminalToolAction(rawAction) {
    if (!rawAction || typeof rawAction !== "object") {
      return null;
    }
    const kind = String(rawAction.kind || "").trim();
    if (!TERMINAL_TOOL_ACTION_TYPES.some((entry) => entry.key === kind)) {
      return null;
    }
    let value = String(rawAction.value || "").trim();
    let seconds = 0;
    if (kind === "create_terminal" || kind === "fork_session" || kind === "switch_api_preset_revert") {
      value = "";
    } else if (kind === "wait") {
      const parsed = Number(rawAction.seconds);
      if (!Number.isFinite(parsed) || parsed < 0.1 || parsed > 600) {
        return null;
      }
      value = "";
      seconds = Math.round(parsed * 10) / 10;
    } else if (kind === "function_command") {
      const commandKey = normalizeTerminalQuickText(rawAction.value, 64);
      if (!commandKey || /\s/.test(commandKey)) {
        return null;
      }
      return { kind, value: commandKey, seconds: 0, command_key: commandKey };
    } else if (kind === "run_workflow") {
      const targetId = String(rawAction.value || "").trim();
      if (!/^[A-Za-z0-9_-]{1,64}$/.test(targetId)) {
        return null;
      }
      return { kind, value: targetId, seconds: 0, target_entry_id: targetId };
    } else if (kind === "codex_launch") {
      const maxLength = 4096;
      if (!value || value.length > maxLength) {
        return null;
      }
      const presetSelector = normalizeTerminalQuickText(rawAction.preset_selector, 128);
      const presetMatch = ["id", "exact_name", "unique_contains"].includes(rawAction.preset_match)
        ? rawAction.preset_match
        : "";
      const cwd = normalizeAbsoluteWorkflowPath(rawAction.cwd);
      const projectPath = normalizeAbsoluteWorkflowPath(rawAction.project_path);
      const terminalName = normalizeTerminalQuickText(rawAction.terminal_name, 64);
      const sessionAction = rawAction.session_action === "new" ? "new" : "";
      if (!presetSelector || !presetMatch || !cwd || !projectPath || !terminalName || !sessionAction) {
        return null;
      }
      return {
        kind,
        value,
        seconds: 0,
        preset_selector: presetSelector,
        preset_match: presetMatch,
        cwd,
        project_path: projectPath,
        terminal_name: terminalName,
        session_action: sessionAction,
      };
    } else {
      const maxLength = ["send_command", "codex_exec", "codex_terminal"].includes(kind)
        ? 4096
        : 128;
      if (!value || value.length > maxLength) {
        return null;
      }
    }
    return { kind, value, seconds };
  }

  function normalizeTerminalToolEntries(entries) {
    if (!Array.isArray(entries) || entries.length > 200) {
      return [];
    }
    const rootKeys = new Set(TERMINAL_TOOL_ROOTS.map((entry) => entry.key));
    const seenIds = new Set();
    const normalized = [];
    for (const rawEntry of entries) {
      if (!rawEntry || typeof rawEntry !== "object") {
        return [];
      }
      const id = String(rawEntry.id || "").trim();
      const rootKey = String(rawEntry.root_key || "").trim();
      const parentValue = String(rawEntry.parent_id || "").trim();
      const parentId = parentValue || null;
      const kind = String(rawEntry.kind || "").trim();
      const label = String(rawEntry.label || "").trim();
      const sortOrder = Number(rawEntry.sort_order ?? 0);
      if (
        !/^[A-Za-z0-9_-]{1,64}$/.test(id)
        || seenIds.has(id)
        || !rootKeys.has(rootKey)
        || (kind !== "folder" && kind !== "action")
        || !label
        || label.length > 64
        || !Number.isInteger(sortOrder)
        || sortOrder < 0
        || sortOrder > 10000
        || parentId === id
      ) {
        return [];
      }
      seenIds.add(id);
      const rawActions = Array.isArray(rawEntry.actions) ? rawEntry.actions : [];
      let actions = [];
      if (kind === "folder") {
        if (rawActions.length > 0) {
          return [];
        }
      } else {
        if (rawActions.length < 1 || rawActions.length > 20) {
          return [];
        }
        actions = rawActions.map(normalizeTerminalToolAction);
        if (actions.some((action) => !action)) {
          return [];
        }
      }
      normalized.push({ id, root_key: rootKey, parent_id: parentId, kind, label, sort_order: sortOrder, actions });
    }

    for (const entry of normalized) {
      if (!entry.parent_id) {
        continue;
      }
      const parent = normalized.find((candidate) => candidate.id === entry.parent_id);
      if (!parent || parent.kind !== "folder" || parent.root_key !== entry.root_key) {
        return [];
      }
    }
    for (const entry of normalized) {
      const visited = new Set();
      let cursor = entry;
      while (cursor) {
        if (visited.has(cursor.id)) {
          return [];
        }
        visited.add(cursor.id);
        cursor = cursor.parent_id
          ? normalized.find((candidate) => candidate.id === cursor.parent_id)
          : null;
      }
    }
    return normalized;
  }

  function ensureBuiltInTerminalToolEntries(entries) {
    const normalized = normalizeTerminalToolEntries(entries);
    const defaults = normalizeTerminalToolEntries(DEFAULT_TERMINAL_TOOL_ENTRIES);
    for (const builtin of defaults) {
      const index = normalized.findIndex((entry) => entry.id === builtin.id);
      if (index >= 0) {
        normalized[index] = builtin;
      } else if (normalized.length < 200) {
        normalized.push(builtin);
      }
    }
    return normalized;
  }

  function isCompactSlashCommand(command) {
    return command?.key === "compact" || command?.command === "/compact";
  }

  function moveCompactSlashCommandToEnd(commands) {
    const regularCommands = [];
    const sessionIdCommands = new Map();
    const terminalNameCommands = [];
    const slashCommands = [];
    const compactCommands = [];
    commands.forEach((command) => {
      if (isCompactSlashCommand(command)) {
        compactCommands.push(command);
      } else if (TERMINAL_SESSION_ID_COMMAND_KEYS.includes(command?.key)) {
        sessionIdCommands.set(command.key, command);
      } else if (command?.key === "copy_terminal_name" || command?.action === "copy_terminal_name") {
        terminalNameCommands.push(command);
      } else if (command?.action === "send_slash_command" || command?.command?.startsWith("/")) {
        slashCommands.push(command);
      } else {
        regularCommands.push(command);
      }
    });
    return regularCommands.concat(
      TERMINAL_SESSION_ID_COMMAND_KEYS
        .map((key) => sessionIdCommands.get(key))
        .filter(Boolean),
      terminalNameCommands,
      slashCommands,
      compactCommands,
    );
  }

  function ensureBuiltInTerminalSlashCommands(commands) {
    const normalized = normalizeTerminalFunctionCommands(commands, null).filter(
      (command) =>
        !isProjectOnlyTerminalCommand(command) && !isFunctionOnlyTerminalCommand(command),
    );
    // 内置命令的 label/action 始终以默认值为准，避免改了默认值后已保存的旧配置
    // 因 key 已存在而继续显示过期的 label/action。
    const defaultsByKey = new Map(
      normalizeTerminalFunctionCommands(DEFAULT_TERMINAL_SLASH_COMMANDS, null).map((command) => [
        command.key,
        command,
      ]),
    );
    normalized.forEach((command) => {
      const builtin = defaultsByKey.get(command.key);
      if (builtin) {
        command.label = builtin.label;
        command.action = builtin.action;
        if (command.key === "fork") {
          command.command = builtin.command;
        }
        if (BUILTIN_TERMINAL_SHORTCUT_KEYS.has(command.key)) {
          command.shortcut = builtin.shortcut;
        }
      }
    });
    const seen = new Set(normalized.map((command) => command.key));
    defaultsByKey.forEach((command) => {
      if (!seen.has(command.key)) {
        normalized.push(command);
        seen.add(command.key);
      }
    });
    return moveCompactSlashCommandToEnd(normalized).slice(0, MAX_TERMINAL_FUNCTION_COMMANDS);
  }

  function filterMovedSlashCommands(commands, fallback = null) {
    return normalizeTerminalFunctionCommands(commands, fallback).filter(
      (command) => !isMovedSlashCommand(command),
    );
  }

  function ensureBuiltInTerminalFunctionCommands(
    commands,
    defaults = DEFAULT_TERMINAL_PAGE_FUNCTION_COMMANDS,
  ) {
    const normalized = normalizeTerminalFunctionCommands(commands, null).filter(
      (command) => !isMovedSlashCommand(command) && !isProjectOnlyTerminalCommand(command),
    );
    const defaultsByKey = new Map(
      normalizeTerminalFunctionCommands(defaults, null)
        .filter(
          (command) => !isMovedSlashCommand(command) && !isProjectOnlyTerminalCommand(command),
        )
        .map((command) => [command.key, command]),
    );
    normalized.forEach((command) => {
      const builtin = defaultsByKey.get(command.key);
      if (builtin) {
        command.label = builtin.label;
        command.action = builtin.action;
        if (BUILTIN_TERMINAL_SHORTCUT_KEYS.has(command.key)) {
          command.shortcut = builtin.shortcut;
        }
      }
    });
    const seen = new Set(normalized.map((command) => command.key));
    defaultsByKey.forEach((command) => {
      if (!seen.has(command.key)) {
        normalized.push(command);
        seen.add(command.key);
      }
    });
    return normalized.slice(0, MAX_TERMINAL_FUNCTION_COMMANDS);
  }

  function normalizeTerminalQuickStartDefaultKey(key, commands) {
    const normalizedKey = normalizeTerminalQuickText(key, 8);
    if (!normalizedKey) {
      return "";
    }
    return commands.some((command) => command.key === normalizedKey) ? normalizedKey : "";
  }

  function normalizeThemeMode(mode) {
    if (mode === "dark" || mode === "light" || mode === "system") {
      return mode;
    }
    return DEFAULT_THEME_MODE;
  }

  function resolveThemeMode(mode) {
    const normalized = normalizeThemeMode(mode);
    if (normalized !== "system") {
      return normalized;
    }
    const prefersDark = root.matchMedia?.("(prefers-color-scheme: dark)")?.matches;
    return prefersDark ? "dark" : "light";
  }

  function readStoredThemeMode() {
    try {
      return normalizeThemeMode(root.localStorage.getItem(THEME_MODE_STORAGE_KEY));
    } catch {
      return DEFAULT_THEME_MODE;
    }
  }

  function normalizeFontSizeTier(value, fallback) {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
      return fallback;
    }
    return Math.min(1.2, Math.max(0.45, parsed));
  }

  function normalizeFontSizeTiers(values) {
    const next = DEFAULT_FONT_SIZE_TIERS.map((fallback, index) =>
      normalizeFontSizeTier(values?.[index], fallback)
    );
    for (let index = 1; index < next.length; index += 1) {
      if (next[index] < next[index - 1]) {
        next[index] = next[index - 1];
      }
    }
    return next.map((value) => Math.round(value * 100) / 100);
  }

  function normalizeTerminalFloatingButtonOffsetVh(value) {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
      return DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH;
    }
    return Math.round(Math.min(60, Math.max(12, parsed)) * 10) / 10;
  }

  function normalizeTerminalFabActionColor(value) {
    const normalized = String(value || "").trim().toLowerCase();
    return /^#[0-9a-f]{6}$/.test(normalized)
      ? normalized
      : DEFAULT_TERMINAL_FAB_ACTION_COLOR;
  }

  function normalizeTerminalFabActionOpacity(value) {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
      return DEFAULT_TERMINAL_FAB_ACTION_OPACITY;
    }
    return Math.round(Math.min(1, Math.max(0.1, parsed)) * 100) / 100;
  }

  function normalizeTerminalSoftKeyboardScale(value) {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
      return DEFAULT_TERMINAL_SOFT_KEYBOARD_SCALE;
    }
    return Math.round(Math.min(1.3, Math.max(0.9, parsed)) * 100) / 100;
  }

  function normalizeTerminalTouchSelectionLongPressMs(value) {
    const parsed = Number(value);
    if (!Number.isFinite(parsed)) {
      return DEFAULT_TERMINAL_TOUCH_SELECTION_LONG_PRESS_MS;
    }
    return Math.round(Math.min(10000, Math.max(2000, parsed)));
  }

  function normalizeTerminalScrollbackLines(value) {
    const parsed = Number.parseInt(value, 10);
    if (!Number.isFinite(parsed)) {
      return DEFAULT_TERMINAL_SCROLLBACK_LINES;
    }
    return Math.min(100000, Math.max(100, parsed));
  }

  return {
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
    DEFAULT_TERMINAL_FLOATING_BUTTON_OFFSET_VH,
    DEFAULT_TERMINAL_FAB_ACTION_COLOR,
    DEFAULT_TERMINAL_FAB_ACTION_OPACITY,
    DEFAULT_TERMINAL_FUNCTION_COMMANDS,
    DEFAULT_TERMINAL_PAGE_FUNCTION_COMMANDS,
    TERMINAL_KEYBOARD_CHECKBOX_ACTIONS,
    DEFAULT_TERMINAL_QUICK_COMMANDS,
    DEFAULT_TERMINAL_QUICK_START_DEFAULT_KEY,
    DEFAULT_TERMINAL_RENAME_PRESETS,
    DEFAULT_TERMINAL_SLASH_COMMANDS,
    DEFAULT_TERMINAL_COMMAND_COLLECTIONS,
    DEFAULT_TERMINAL_TOOL_ENTRIES,
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
    normalizeCommandCollection,
    normalizeCommandCollectionItem,
    ensureBuiltInTerminalToolEntries,
    ensureBuiltInTerminalFunctionCommands,
    ensureBuiltInTerminalSlashCommands,
    filterMovedSlashCommands,
    isCompactSlashCommand,
    isMovedSlashCommand,
    isProjectOnlyTerminalCommand,
    moveCompactSlashCommandToEnd,
    normalizeFontSizeTier,
    normalizeFontSizeTiers,
    normalizeHostName,
    normalizeTerminalActivityAgentDisplay,
    normalizeTerminalAutoContinueIntervalSeconds,
    normalizeTerminalAutoContinueBackoffFactor,
    normalizeTerminalAutoContinueBackoffMaxMinutes,
    normalizeTerminalFloatingButtonOffsetVh,
    normalizeTerminalFabActionColor,
    normalizeTerminalFabActionOpacity,
    normalizeTerminalFunctionCommand,
    normalizeTerminalFunctionCommandLine,
    normalizeTerminalFunctionCommands,
    normalizeTerminalCommandCollections,
    normalizeTerminalToolAction,
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
  };
});
