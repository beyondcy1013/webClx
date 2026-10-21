const SETTINGS_CATEGORY_REGISTRY = Object.freeze([
  Object.freeze({
    key: "tools",
    label: "工作流",
    defaultTab: "tools",
    tabs: Object.freeze([
      Object.freeze({ key: "tools", label: "工作流" }),
    ]),
  }),
  Object.freeze({
    key: "system",
    label: "系统",
    defaultTab: "system",
    tabs: Object.freeze([
      Object.freeze({ key: "system", label: "系统" }),
    ]),
  }),
  Object.freeze({
    key: "terminal",
    label: "终端",
    defaultTab: "terminal",
    tabs: Object.freeze([
      Object.freeze({ key: "terminal", label: "终端行为" }),
    ]),
  }),
  Object.freeze({
    key: "input",
    label: "终端输入",
    defaultTab: "soft-keyboard",
    tabs: Object.freeze([
      Object.freeze({ key: "soft-keyboard", label: "软键盘与命令" }),
      Object.freeze({ key: "shortcuts", label: "快捷键" }),
    ]),
  }),
  Object.freeze({
    key: "appearance",
    label: "外观",
    defaultTab: "appearance",
    tabs: Object.freeze([
      Object.freeze({ key: "appearance", label: "外观" }),
    ]),
  }),
  Object.freeze({
    key: "tasks",
    label: "任务",
    defaultTab: "auto-continue-tasks",
    tabs: Object.freeze([
      Object.freeze({ key: "auto-continue-tasks", label: "定时任务" }),
    ]),
  }),
  Object.freeze({
    key: "build",
    label: "构建",
    defaultTab: "compile",
    tabs: Object.freeze([
      Object.freeze({ key: "compile", label: "编译任务" }),
    ]),
  }),
  Object.freeze({
    key: "ai",
    label: "AI",
    defaultTab: "model",
    tabs: Object.freeze([
      Object.freeze({ key: "model", label: "模型" }),
      Object.freeze({ key: "agent", label: "Agent" }),
      Object.freeze({ key: "config-files", label: "高级配置" }),
    ]),
  }),
  Object.freeze({
    key: "network",
    label: "网络",
    defaultTab: "proxy",
    tabs: Object.freeze([
      Object.freeze({ key: "proxy", label: "代理" }),
      Object.freeze({ key: "frpc", label: "FRP 客户端" }),
      Object.freeze({ key: "frps", label: "FRP 服务器" }),
    ]),
  }),
  Object.freeze({
    key: "maintenance",
    label: "维护",
    defaultTab: "preset-sync",
    tabs: Object.freeze([
      Object.freeze({ key: "preset-sync", label: "预设同步" }),
      Object.freeze({ key: "update", label: "版本更新" }),
    ]),
  }),
]);

const SETTINGS_TAB_ALIASES = Object.freeze({
  workspace: "system",
  display: "appearance",
  theme: "appearance",
  font: "appearance",
});

const SETTINGS_REMOTE_COPY_TABS = new Set([
  "system",
  "terminal",
  "appearance",
  "compile",
  "auto-continue-tasks",
  "soft-keyboard",
  "shortcuts",
  "tools",
  "model",
]);

function normalizeSettingsTab(value) {
  const raw = String(value || "").trim().toLowerCase();
  const candidate = SETTINGS_TAB_ALIASES[raw] || raw;
  for (const category of SETTINGS_CATEGORY_REGISTRY) {
    if (category.tabs.some((tab) => tab.key === candidate)) {
      return candidate;
    }
  }
  return "system";
}

function settingsCategoryForTab(value) {
  const tab = normalizeSettingsTab(value);
  return SETTINGS_CATEGORY_REGISTRY.find((category) =>
    category.tabs.some((entry) => entry.key === tab)
  ) || SETTINGS_CATEGORY_REGISTRY[0];
}

function defaultSettingsTabForCategory(categoryKey) {
  return SETTINGS_CATEGORY_REGISTRY.find((category) => category.key === categoryKey)?.defaultTab
    || "system";
}

function settingsTabSupportsRemoteCopy(value) {
  return SETTINGS_REMOTE_COPY_TABS.has(normalizeSettingsTab(value));
}

function syncSettingsCategoryNavigation(activeTab) {
  const tab = normalizeSettingsTab(activeTab);
  const category = settingsCategoryForTab(tab);

  settingsCategoryButtons.forEach((button) => {
    const active = button.dataset.settingsCategory === category.key;
    button.classList.toggle("active", active);
    button.setAttribute("aria-selected", active ? "true" : "false");
  });

  if (!settingsSubtabsEl) {
    return;
  }
  settingsSubtabsEl.replaceChildren();
  settingsSubtabsEl.hidden = category.tabs.length < 2;
  for (const entry of category.tabs) {
    const button = document.createElement("button");
    button.id = `settings-tab-${entry.key}`;
    button.className = "tab-button settings-subtab-button";
    button.type = "button";
    button.role = "tab";
    button.dataset.settingsTab = entry.key;
    button.setAttribute("aria-controls", `settings-panel-${entry.key}`);
    button.setAttribute("aria-selected", entry.key === tab ? "true" : "false");
    button.classList.toggle("active", entry.key === tab);
    button.textContent = entry.label;
    button.addEventListener("click", () => setActiveSettingsTab(entry.key));
    settingsSubtabsEl.append(button);
  }
}
