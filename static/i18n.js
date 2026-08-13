(() => {
  "use strict";

  const STORAGE_KEY = "webclx:locale";
  const DEFAULT_LOCALE = "zh-CN";
  const supportedLocales = Object.freeze(["zh-CN", "en"]);
  const translatedAttributes = ["aria-label", "aria-description", "placeholder", "title"];
  const english = Object.freeze({
    "终端管理": "Terminals",
    "工作区": "Workspace",
    "历史工作区": "Workspace history",
    "设置": "Settings",
    "远程桌面": "Remote desktop",
    "归档列表": "Archives",
    "归档": "Archive",
    "闲置": "Idle",
    "选择闲置终端": "Choose idle terminal",
    "编译产物": "Build artifacts",
    "目录浏览": "Files",
    "当前目录：": "Current directory:",
    "当前目录路径": "Current directory path",
    "复制当前目录路径": "Copy current directory path",
    "点击复制当前路径": "Copy the current path",
    "当前目录终端": "Open terminal here",
    "跳转收藏": "Open favorite",
    "切换终端": "Switch terminal",
    "正在读取当前目录终端会话…": "Loading terminal sessions...",
    "直接在当前页面显示的目录新建终端，不跟随下拉框切换": "Create a terminal in the displayed directory without following the selector",
    "显示隐藏文件": "Show hidden files",
    "操作": "Actions",
    "收藏": "Favorite",
    "图标": "Icon",
    "名称": "Name",
    "大小": "Size",
    "目录": "Directory",
    "标题": "Title",
    "匹配": "Match",
    "进入": "Open",
    "编辑": "Edit",
    "改名": "Rename",
    "结束": "End",
    "指定": "Choose",
    "文本编辑": "Text editor",
    "尚未打开文件": "No file open",
    "仅支持 UTF-8 文本文件": "UTF-8 text files only",
    "剪贴板导入": "Import clipboard",
    "保存文件": "Save file",
    "点击左侧文件可在这里查看和修改内容。": "Select a file to view and edit it here.",
    "系统": "System",
    "终端": "Terminal",
    "终端工具": "Terminal tools",
    "移动端终端特殊按键": "Mobile terminal special keys",
    "webClx 顶级导航": "webClx primary navigation",
    "输入": "Input",
    "工作流": "Workflows",
    "外观": "Appearance",
    "任务": "Tasks",
    "构建": "Build",
    "智能体": "Agents",
    "网络": "Network",
    "维护": "Maintenance",
    "菜单": "Menu",
    "保存": "Save",
    "取消": "Cancel",
    "关闭": "Close",
    "确认": "Confirm",
    "删除": "Delete",
    "刷新": "Refresh",
    "刷新终端": "Refresh terminals",
    "清除": "Clear",
    "搜索": "Search",
    "搜索终端输出": "Search terminal output",
    "选择活动终端": "Choose active terminal",
    "全部终端会话": "All terminal sessions",
    "新建终端": "New terminal",
    "打开终端": "Open terminal",
    "复制": "Copy",
    "复制全部": "Copy all",
    "新建": "New",
    "新建会话": "New session",
    "+ 新建会话": "+ New session",
    "+ 新建": "+ New",
    "Agent 会话": "Agent sessions",
    "新 Agent 会话": "New Agent session",
    "新建智能体": "New agent",
    "新建智能体会话": "New agent session",
    "打开": "Open",
    "打开最近的智能体会话": "Open the latest agent session",
    "或点击左上角菜单查看已有会话": "Or use the top-left menu to view existing sessions",
    "通过自然语言驱动 Codex skill 和命令执行": "Use natural language to run Codex Skills and commands",
    "代理设置": "Proxy setup",
    "工作代理": "Work agent",
    "智能体工厂": "Agent factory",
    "检查并配置本机 Mihomo 代理，处理节点、连通性和代理环境问题。": "Inspect and configure the local Mihomo proxy, including nodes, connectivity, and proxy environment issues.",
    "在 /home/third_party 中接收并处理通用开发与运维任务。": "Handle general development and operations tasks in /home/third_party.",
    "根据需求创建、修改和检查终端智能体，并维护预设、目录、Skill 与初始任务配置。": "Create, update, and inspect terminal agents, including presets, directories, Skills, and initial tasks.",
    "内置 Agent 可检查和修改工作区、运行验证，也能按需使用专项 skill。": "The built-in Agent can inspect and edit the workspace, run verification, and use specialized Skills.",
    "发送": "Send",
    "发送消息": "Send message",
    "停止": "Stop",
    "继续": "Continue",
    "加载中…": "Loading...",
    "运行中": "Running",
    "工作中": "Working",
    "待查看": "Ready to review",
    "空闲": "Idle",
    "已完成": "Completed",
    "失败": "Failed",
    "错误": "Error",
    "账号": "Username",
    "密码": "Password",
    "登录": "Sign in",
    "登录中…": "Signing in...",
    "请输入账号和密码": "Enter your username and password",
    "登录失败，请检查账号和密码": "Sign-in failed. Check your username and password.",
    "登录请求过多，请稍后重试": "Too many sign-in requests. Try again shortly.",
    "网络错误，请稍后重试": "Network error. Try again shortly.",
    "退出登录": "Sign out",
    "选择命令": "Choose command",
    "当前项目指令": "Current project command",
    "项目管理": "Project",
    "项目指令": "Project command",
    "切换服务器": "Switch server",
    "项目 URL": "Project URL",
    "下载中心": "Downloads",
    "文档": "Docs",
    "永久切换预设": "Switch preset permanently",
    "指定（临时）": "Choose preset (temporary)",
    "终端内临时切换预设": "Switch preset in terminal",
    "指定预设临时终端": "Temporary terminal with preset",
    "维护命令": "Maintenance commands",
    "快捷操作": "Quick actions",
    "底部快捷操作": "Bottom quick actions",
    "快捷": "Shortcuts",
    "快捷命令": "Quick commands",
    "全能": "All",
    "全能命令": "All commands",
    "数": "Numbers",
    "键盘": "Keyboard",
    "显示软键盘": "Show soft keyboard",
    "定时消息": "Scheduled message",
    "跳顶部": "Jump to top",
    "跳底部": "Jump to bottom",
    "对话史": "History",
    "路径": "Path",
    "短按发送 ^C，长按发送 Esc": "Press to send ^C; hold to send Esc",
    "后退": "Back",
    "前进": "Forward",
    "顶部快捷操作": "Top actions",
    "回到页面顶部": "Back to top",
    "发送数字": "Send number",
    "终端列表": "Terminal list",
    "会话详情": "Session details",
    "发送终端消息": "Send terminal message",
    "目标终端": "Target terminal",
    "消息": "Message",
    "发送并提交": "Send and submit",
    "对话历史": "Conversation history",
    "本终端对话历史": "Terminal conversation history",
    "扫描选项": "Scan options",
    "显示隐藏目录": "Show hidden directories",
    "新建文档": "New document",
    "新建文档名": "New document name",
    "当前目录没有 AGENTS.MD，保存后会创建该文件。": "No AGENTS.MD exists here. Saving will create it.",
    "语言": "Language",
    "中文": "Chinese",
    "英文": "English",
  });

  const originalText = new WeakMap();
  const renderedText = new WeakMap();
  const originalAttributes = new WeakMap();
  let locale = normalizeLocale(localStorage.getItem(STORAGE_KEY) || navigator.languages?.[0]);
  let applying = false;

  function normalizeLocale(value) {
    return String(value || "").toLowerCase().startsWith("en") ? "en" : DEFAULT_LOCALE;
  }

  function translate(value, requestedLocale = locale) {
    if (requestedLocale !== "en" || typeof value !== "string") return value;
    const trimmed = value.trim();
    if (!trimmed) return value;
    const translated = english[trimmed] || translateTemplate(trimmed);
    if (!translated) return value;
    const start = value.indexOf(trimmed);
    return `${value.slice(0, start)}${translated}${value.slice(start + trimmed.length)}`;
  }

  function translateTemplate(value) {
    for (const [prefix, replacement] of [
      ["当前目录路径：", "Current directory path: "],
      ["空闲 - ", "Idle - "],
    ]) {
      if (value.startsWith(prefix)) return replacement + value.slice(prefix.length);
    }
    let match = value.match(/^发送数字 (\d+)$/);
    if (match) return `Send number ${match[1]}`;
    match = value.match(/^定时 (\d+)\/(\d+)$/);
    if (match) return `Scheduled ${match[1]}/${match[2]}`;
    match = value.match(/^维护命令（(\d+) 条）$/);
    if (match) return `Maintenance commands (${match[1]})`;
    return "";
  }

  function translateTextNode(node) {
    const current = node.nodeValue || "";
    const previousRendered = renderedText.get(node);
    if (!originalText.has(node) || (previousRendered !== undefined && current !== previousRendered)) {
      originalText.set(node, current);
    }
    const next = translate(originalText.get(node));
    if (current !== next) node.nodeValue = next;
    renderedText.set(node, next);
  }

  function translateElement(element) {
    let sources = originalAttributes.get(element);
    if (!sources) {
      sources = new Map();
      originalAttributes.set(element, sources);
    }
    for (const name of translatedAttributes) {
      if (!element.hasAttribute(name)) continue;
      const current = element.getAttribute(name) || "";
      const previous = sources.get(name);
      if (!previous || current !== previous.rendered) {
        sources.set(name, { source: current, rendered: current });
      }
      const active = sources.get(name);
      const next = translate(active.source);
      if (current !== next) element.setAttribute(name, next);
      active.rendered = next;
    }
  }

  function applyNode(root) {
    if (!root) return;
    if (root.nodeType === Node.TEXT_NODE) return translateTextNode(root);
    if (root.nodeType !== Node.ELEMENT_NODE && root.nodeType !== Node.DOCUMENT_NODE) return;
    if (root.nodeType === Node.ELEMENT_NODE) translateElement(root);
    const walker = document.createTreeWalker(root, NodeFilter.SHOW_ELEMENT | NodeFilter.SHOW_TEXT);
    let node;
    while ((node = walker.nextNode())) {
      if (node.nodeType === Node.TEXT_NODE) translateTextNode(node);
      else translateElement(node);
    }
  }

  function apply(root = document.body) {
    applying = true;
    document.documentElement.lang = locale;
    document.documentElement.dataset.locale = locale;
    applyNode(root);
    const select = document.getElementById("webclx-language-select");
    if (select) select.value = locale;
    applying = false;
  }

  function addLanguageControl() {
    if (document.getElementById("webclx-language-control")) return;
    const label = document.createElement("label");
    label.id = "webclx-language-control";
    label.className = "webclx-language-control";
    label.innerHTML = '<span class="sr-only">语言</span><select id="webclx-language-select" aria-label="语言"><option value="zh-CN">中文</option><option value="en">English</option></select>';
    const host = document.querySelector(".browser-topbar") || document.querySelector(".login-card") || document.body;
    host.append(label);
    label.querySelector("select").addEventListener("change", (event) => setLocale(event.target.value));
  }

  function setLocale(value) {
    locale = normalizeLocale(value);
    localStorage.setItem(STORAGE_KEY, locale);
    document.documentElement.lang = locale;
    document.documentElement.dataset.locale = locale;
    if (document.body) apply();
    window.dispatchEvent(new CustomEvent("webclx:locale-change", { detail: { locale } }));
  }

  function getLocale() {
    return locale;
  }

  function start() {
    addLanguageControl();
    apply();
    new MutationObserver((mutations) => {
      if (applying) return;
      for (const mutation of mutations) {
        if (mutation.type === "characterData") applyNode(mutation.target);
        for (const node of mutation.addedNodes || []) applyNode(node);
        if (mutation.type === "attributes") translateElement(mutation.target);
      }
    }).observe(document.body, {
      subtree: true,
      childList: true,
      characterData: true,
      attributes: true,
      attributeFilter: translatedAttributes,
    });
  }

  window.webclxI18n = Object.freeze({ supportedLocales, getLocale, setLocale, translate, apply });
  if (!document.body || document.readyState === "loading") document.addEventListener("DOMContentLoaded", start, { once: true });
  else start();
})();
