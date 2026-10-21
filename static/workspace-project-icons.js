(function attachWorkspaceProjectIcons(root, factory) {
  const api = factory(root);
  if (typeof module === "object" && module.exports) {
    module.exports = api;
  }
  if (root && root.document) {
    root.WebClxWorkspaceProjectIcons = api;
  }
})(typeof globalThis !== "undefined" ? globalThis : this, function createWorkspaceProjectIcons(root) {
  const DEFAULT_WORKSPACE_BROWSER_ICON_PATH = "icon.ico";
  const DEFAULT_TERMINAL_WORKSPACE_ICON_PATH = "static/favicon.svg";

  function normalizeProjectIconPath(value, fallback) {
    const normalized = String(value || "").trim().replaceAll("\\", "/");
    if (!normalized || normalized.startsWith("/") || normalized.length > 240) {
      return fallback;
    }
    const parts = normalized.split("/").filter((part) => part && part !== ".");
    if (parts.length === 0 || parts.some((part) => part === "..")) {
      return fallback;
    }
    return parts.join("/");
  }

  function workspaceProjectIconUrl(path, iconPath, nearest = false) {
    const params = new URLSearchParams({
      path: String(path || ""),
      icon_path: String(iconPath || ""),
      search: nearest ? "nearest" : "exact",
    });
    return `/api/workspace-icon?${params.toString()}`;
  }

  function normalizedProjectPath(path) {
    const parts = String(path || "")
      .trim()
      .replaceAll("\\", "/")
      .split("/")
      .filter((part) => part && part !== ".");
    return parts;
  }

  function workspaceProjectKey(path, exact = false) {
    const parts = normalizedProjectPath(path);
    if (parts.length === 0) return ".";
    if (exact) return parts.join("/");
    if (parts[0] === "..") {
      return parts.slice(0, Math.min(3, parts.length)).join("/");
    }
    if (parts[0] === "third_party" && parts.length > 1) {
      return parts.slice(0, 2).join("/");
    }
    return parts[0];
  }

  function workspaceProjectTextIcon(path, exact = false) {
    const projectName = workspaceProjectKey(path, exact).split("/").filter(Boolean).at(-1) || "project";
    const tokens = projectName.match(/[A-Z]?[a-z]+|[A-Z]+(?![a-z])|\d+|[\p{L}\p{N}]/gu) || [];
    if (tokens.length >= 2) {
      return `${Array.from(tokens[0])[0]}${Array.from(tokens[1])[0]}`.toUpperCase();
    }
    const characters = Array.from(projectName.replace(/[^\p{L}\p{N}]/gu, ""));
    return characters.slice(0, 2).join("").toUpperCase() || "PR";
  }

  function workspaceProjectColorSlots(paths, exact = false) {
    const keys = Array.from(
      new Set(Array.from(paths || [], (path) => workspaceProjectKey(path, exact))),
    ).sort();
    return new Map(keys.map((key, index) => [key, index]));
  }

  function workspaceProjectHue(path, colorSlots = null, exact = false) {
    const key = workspaceProjectKey(path, exact);
    if (colorSlots?.has(key)) {
      return Number(((210 + colorSlots.get(key) * 137.508) % 360).toFixed(3));
    }
    let hash = 2166136261;
    for (const character of key) {
      hash ^= character.codePointAt(0);
      hash = Math.imul(hash, 16777619);
    }
    return (hash >>> 0) % 360;
  }

  const EXECUTOR_EMOJIS = ["⚙️", "🛠️", "🚀", "🔧", "🧭", "🧪", "📦", "🛰️", "🧩", "🎯"];
  function terminalRoleEmoji(option) {
    const role = option.dataset.groupRole || "";
    if (!role) return "";
    if (role === "manager") return root.localStorage?.getItem("webclx:terminal-group-manager-emoji") || "👑";
    if (root.localStorage?.getItem("webclx:terminal-group-executor-emoji-mode") === "fixed") {
      return root.localStorage?.getItem("webclx:terminal-group-executor-emoji") || "⚙️";
    }
    let hash = 2166136261;
    // Random is selected once per project, so every executor terminal for the
    // same project shares one stable emoji across refreshes and sessions.
    const projectKey = workspaceProjectKey(
      option.dataset.iconProjectPath || option.dataset.workspacePath || option.value || "",
      true,
    );
    for (const character of projectKey) {
      hash ^= character.codePointAt(0);
      hash = Math.imul(hash, 16777619);
    }
    return EXECUTOR_EMOJIS[(hash >>> 0) % EXECUTOR_EMOJIS.length];
  }

  function createWorkspaceProjectIcon(
    path,
    iconPath,
    nearest = false,
    className = "",
    colorSlots = null,
    exactProjectPath = false,
  ) {
    if (!root?.document || !iconPath) {
      return null;
    }
    const icon = root.document.createElement("span");
    icon.className = ["workspace-project-icon", className].filter(Boolean).join(" ");
    icon.setAttribute("aria-hidden", "true");
    icon.dataset.projectKey = workspaceProjectKey(path, exactProjectPath);
    icon.style.setProperty(
      "--workspace-project-icon-hue",
      String(workspaceProjectHue(path, colorSlots, exactProjectPath)),
    );

    const fallback = root.document.createElement("span");
    fallback.className = "workspace-project-text-icon";
    fallback.textContent = workspaceProjectTextIcon(path, exactProjectPath);

    const image = root.document.createElement("img");
    image.className = "workspace-project-icon-image";
    image.alt = "";
    image.loading = "lazy";
    image.decoding = "async";
    image.classList.add("loading");
    image.addEventListener("load", () => {
      image.classList.remove("loading");
      icon.classList.add("image-ready");
    });
    image.addEventListener("error", () => {
      image.hidden = true;
      icon.classList.add("text-fallback");
    });
    // The workspace root has no project-relative icon. Use the app favicon
    // directly so the normal root fallback does not create a 404 request.
    image.src = !String(path || "").trim() && iconPath === DEFAULT_TERMINAL_WORKSPACE_ICON_PATH
      ? "/assets/favicon.svg"
      : workspaceProjectIconUrl(path, iconPath, nearest);
    icon.append(fallback, image);
    return icon;
  }

  function enhanceWorkspaceIconSelect(selectEl, getIconPath) {
    if (!root?.document || !selectEl) {
      return null;
    }
    if (selectEl.workspaceIconSelectController) {
      selectEl.workspaceIconSelectController.sync();
      return selectEl.workspaceIconSelectController;
    }

    const host = root.document.createElement("div");
    host.className = "workspace-icon-select";
    const trigger = root.document.createElement("button");
    trigger.className = "workspace-icon-select-trigger";
    trigger.type = "button";
    trigger.setAttribute("aria-haspopup", "listbox");
    trigger.setAttribute("aria-expanded", "false");
    trigger.setAttribute("aria-label", selectEl.getAttribute("aria-label") || "选择终端");
    const menu = root.document.createElement("div");
    menu.className = "workspace-icon-select-menu";
    menu.role = "listbox";
    menu.id = `${selectEl.id || "workspace-icon-select"}-image-menu`;
    trigger.setAttribute("aria-controls", menu.id);
    menu.hidden = true;
    host.append(trigger);
    root.document.body.append(menu);
    selectEl.before(host);
    selectEl.classList.add("workspace-icon-native-select");
    selectEl.setAttribute("aria-hidden", "true");
    selectEl.tabIndex = -1;
    const focusTarget = () => host.contains(root.document.activeElement) ? trigger : selectEl;
    selectEl.addEventListener("focus", () => {
      focusTarget().focus?.();
    });

    function iconPath() {
      return normalizeProjectIconPath(
        typeof getIconPath === "function" ? getIconPath() : getIconPath,
        DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
      );
    }

    function appendOptionContent(container, option, colorSlots) {
      const grouped = option.dataset.grouped === "true";
      const groupColor = option.dataset.groupColor || "";
      if (grouped) {
        container.style.setProperty("--terminal-group-color", groupColor);
        container.style.setProperty(
          "background",
          "color-mix(in srgb, var(--terminal-group-color) 18%, transparent)",
        );
      }
      const path = option.dataset.iconProjectPath || option.dataset.workspacePath || "";
      const icon = createWorkspaceProjectIcon(
        path,
        iconPath(),
        true,
        "workspace-icon-select-image",
        colorSlots,
        true,
      );
      const roleEmoji = terminalRoleEmoji(option);
      if (roleEmoji) {
        const role = root.document.createElement("span");
        role.className = "terminal-role-emoji";
        role.textContent = roleEmoji;
        let counterpart = null;
        try {
          const members = JSON.parse(option.dataset.groupRoleMembers || "[]");
          counterpart = Array.isArray(members)
            ? members.find((member) => member?.role && member.role !== option.dataset.groupRole)
            : null;
        } catch {
          counterpart = null;
        }
        role.title = [
          option.dataset.groupRoleName || (option.dataset.groupRole === "manager" ? "管理者" : "执行者"),
          counterpart ? `搭档：${counterpart.name || counterpart.id}` : "",
        ].filter(Boolean).join(" · ");
        container.append(role);
      }
      if (icon) {
        if (grouped) {
          icon.style.setProperty("--workspace-project-icon-hue", "");
          icon.classList.add("group-colored");
        }
        container.append(icon);
      }
      if (grouped) {
        // 编组跟随终端标记图标（第3个图标），暂时隐藏
        const marker = root.document.createElement("span");
        marker.className = "workspace-icon-select-group-marker";
        marker.setAttribute("aria-hidden", "true");
        marker.textContent = "⇄";
        marker.title = "编组跟随终端";
        marker.hidden = true;
        marker.style.display = "none";
        container.append(marker);
      }
      const text = root.document.createElement("span");
      text.className = "workspace-icon-select-text";
      const baseText = option.textContent || "";
      const separator = baseText.includes(" | ") ? " | " : " · ";
      const [base, ...details] = separator === " | " ? baseText.split(" | ") : [baseText];
      const parts = [base, ...details];
      const groupName = option.dataset.groupName || "";
      if (groupName) {
        const groupBadge = `【${groupName}】`;
        if (parts[0].includes(` · ${groupName}`)) {
          parts[0] = parts[0].replace(` · ${groupName}`, "");
        }
        if (!parts[0].includes(groupBadge)) {
          parts[0] = `${groupBadge}${parts[0]}`;
        }
      }
      text.textContent = parts.join(separator);
      attachHorizontalDragScroll(text);
      container.append(text);
    }

    function attachHorizontalDragScroll(el) {
      if (!el) return;
      let startX = 0;
      let scrollLeft = 0;
      let isDown = false;
      let hasMoved = false;

      el.addEventListener("pointerdown", (e) => {
        if (e.button !== 0) return;
        isDown = true;
        hasMoved = false;
        startX = e.clientX;
        scrollLeft = el.scrollLeft;
        el.setPointerCapture?.(e.pointerId);
      });

      el.addEventListener("pointermove", (e) => {
        if (!isDown) return;
        const walk = e.clientX - startX;
        if (Math.abs(walk) > 4) {
          hasMoved = true;
          el.scrollLeft = scrollLeft - walk;
          el.dataset.dragScrolled = "true";
        }
      });

      const stopDrag = (e) => {
        if (!isDown) return;
        isDown = false;
        if (hasMoved) {
          e?.stopPropagation?.();
        }
        window.setTimeout(() => {
          delete el.dataset.dragScrolled;
        }, 50);
      };

      el.addEventListener("pointerup", stopDrag);
      el.addEventListener("pointercancel", stopDrag);
    }

    let renderedSignature = "";
    let pendingSync = false;

    function close() {
      menu.hidden = true;
      trigger.setAttribute("aria-expanded", "false");
      host.classList.remove("open");
      if (pendingSync) sync();
    }

    function positionMenu() {
      if (menu.hidden) return;
      const viewportWidth = root.innerWidth || root.document.documentElement.clientWidth;
      const viewportHeight = root.innerHeight || root.document.documentElement.clientHeight;
      const triggerRect = trigger.getBoundingClientRect();
      const margin = 8;
      const gap = 5;

      menu.style.width = "max-content";
      const naturalWidth = menu.scrollWidth || triggerRect.width;
      const width = Math.min(Math.max(triggerRect.width, naturalWidth, 260), viewportWidth - margin * 2);
      const spaceBelow = viewportHeight - triggerRect.bottom - gap - margin;
      const spaceAbove = triggerRect.top - gap - margin;
      const openAbove = spaceBelow < 180 && spaceAbove > spaceBelow;
      const availableHeight = Math.max(80, openAbove ? spaceAbove : spaceBelow);

      menu.style.width = `${width}px`;
      menu.style.maxHeight = `${Math.min(460, availableHeight)}px`;
      menu.style.left = `${Math.min(
        Math.max(margin, triggerRect.left),
        viewportWidth - width - margin,
      )}px`;
      menu.style.top = openAbove
        ? `${Math.max(margin, triggerRect.top - gap - menu.getBoundingClientRect().height)}px`
        : `${triggerRect.bottom + gap}px`;
    }

    function open() {
      if (selectEl.disabled) return;
      menu.hidden = false;
      trigger.setAttribute("aria-expanded", "true");
      host.classList.add("open");
      positionMenu();
      const selectedItem = menu.querySelector('[aria-selected="true"]');
      if (selectedItem) {
        selectedItem.focus({ preventScroll: true });
        const itemTop = selectedItem.offsetTop;
        const itemBottom = itemTop + selectedItem.offsetHeight;
        if (itemTop < menu.scrollTop) {
          menu.scrollTop = itemTop;
        } else if (itemBottom > menu.scrollTop + menu.clientHeight) {
          menu.scrollTop = itemBottom - menu.clientHeight;
        }
      }
    }

    function sync() {
      const options = Array.from(selectEl.options);
      // Keep the hit target alive between pointerdown and click. Session events
      // can update the native select several times while this menu is open.
      if (!menu.hidden) {
        pendingSync = true;
        return;
      }
      pendingSync = false;
      const signature = JSON.stringify([
        selectEl.value, selectEl.disabled, iconPath(),
        options.map((option) => [
          option.value, option.textContent, option.title, option.disabled,
          { ...option.dataset }, terminalRoleEmoji(option),
        ]),
      ]);
      if (signature === renderedSignature) return;
      renderedSignature = signature;
      const colorSlots = workspaceProjectColorSlots(
        options.map(
          (option) => option.dataset.iconProjectPath || option.dataset.workspacePath || "",
        ),
        true,
      );
      const selected = selectEl.selectedOptions?.[0] || selectEl.options[0] || null;
      trigger.replaceChildren();
      if (selected) {
        appendOptionContent(trigger, selected, colorSlots);
      }
      const caret = root.document.createElement("span");
      caret.className = "workspace-icon-select-caret";
      caret.setAttribute("aria-hidden", "true");
      trigger.append(caret);
      trigger.disabled = selectEl.disabled;
      trigger.title = selected?.title || selected?.textContent || "";

      menu.replaceChildren();
      options.forEach((option, index) => {
        const item = root.document.createElement("button");
        item.className = "workspace-icon-select-option";
        item.type = "button";
        item.role = "option";
        item.id = `${menu.id}-option-${index}`;
        item.disabled = option.disabled;
        item.classList.toggle("workspace-icon-select-option-grouped", option.dataset.grouped === "true");
        item.setAttribute("aria-selected", option.value === selectEl.value ? "true" : "false");
        item.title = option.title || option.textContent || "";
        appendOptionContent(item, option, colorSlots);
        item.addEventListener("click", (e) => {
          if (item.querySelector(".workspace-icon-select-text")?.dataset.dragScrolled === "true") {
            return;
          }
          const currentOption = Array.from(selectEl.options).find(
            (candidate) => candidate.value === option.value && !candidate.disabled,
          );
          if (!currentOption || selectEl.disabled) {
            close();
            return;
          }
          selectEl.value = currentOption.value;
          selectEl.dispatchEvent(new root.Event("change", { bubbles: true }));
          close();
          trigger.focus();
        });
        menu.append(item);
      });
    }

    trigger.addEventListener("click", (e) => {
      if (trigger.querySelector(".workspace-icon-select-text")?.dataset.dragScrolled === "true") {
        return;
      }
      if (menu.hidden) open();
      else close();
    });
    trigger.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        close();
        return;
      }
      if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) {
        return;
      }
      event.preventDefault();
      const options = Array.from(selectEl.options).filter((option) => !option.disabled);
      if (options.length === 0) return;
      const current = Math.max(0, options.findIndex((option) => option.value === selectEl.value));
      const index = event.key === "Home"
        ? 0
        : event.key === "End"
          ? options.length - 1
          : (current + (event.key === "ArrowDown" ? 1 : -1) + options.length) % options.length;
      selectEl.value = options[index].value;
      selectEl.dispatchEvent(new root.Event("change", { bubbles: true }));
      sync();
    });
    menu.addEventListener("keydown", (event) => {
      if (event.key === "Escape") {
        event.preventDefault();
        close();
        trigger.focus();
        return;
      }
      if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      const items = Array.from(menu.querySelectorAll(".workspace-icon-select-option:not(:disabled)"));
      if (items.length === 0) return;
      const current = Math.max(0, items.indexOf(root.document.activeElement));
      const index = event.key === "Home"
        ? 0
        : event.key === "End"
          ? items.length - 1
          : (current + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
      items[index].focus();
    });
    selectEl.addEventListener("change", sync);
    root.document.addEventListener("click", (event) => {
      if (!host.contains(event.target) && !menu.contains(event.target)) close();
    });
    root.addEventListener("resize", positionMenu);
    root.addEventListener("scroll", positionMenu, true);
    const observer = new root.MutationObserver(sync);
    observer.observe(selectEl, { childList: true, subtree: true, attributes: true });

    const controller = {
      close,
      open,
      sync,
      destroy: () => {
        observer.disconnect();
        root.removeEventListener("resize", positionMenu);
        root.removeEventListener("scroll", positionMenu, true);
        menu.remove();
      },
    };
    selectEl.workspaceIconSelectController = controller;
    sync();
    return controller;
  }

  return Object.freeze({
    DEFAULT_WORKSPACE_BROWSER_ICON_PATH,
    DEFAULT_TERMINAL_WORKSPACE_ICON_PATH,
    normalizeProjectIconPath,
    workspaceProjectIconUrl,
    workspaceProjectKey,
    workspaceProjectTextIcon,
    workspaceProjectColorSlots,
    workspaceProjectHue,
    createWorkspaceProjectIcon,
    enhanceWorkspaceIconSelect,
  });
});
