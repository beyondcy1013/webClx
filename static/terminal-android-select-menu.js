// Replace Android's native radio-style <select> popup with a compact command menu.
(() => {
  const isAndroidClient =
    /\bAndroid\b/i.test(navigator.userAgent || "") ||
    typeof globalThis.WebClxAndroid === "object";
  if (!isAndroidClient) {
    return;
  }

  const menu = document.getElementById("terminal-android-select-menu");
  if (!menu) {
    return;
  }

  let activeSelect = null;

  function closeMenu({ restoreFocus = false } = {}) {
    const select = activeSelect;
    activeSelect = null;
    menu.hidden = true;
    menu.replaceChildren();
    menu.style.removeProperty("left");
    menu.style.removeProperty("top");
    menu.style.removeProperty("width");
    menu.style.removeProperty("max-height");
    if (menu.parentElement !== document.body) {
      document.body.appendChild(menu);
    }
    if (restoreFocus && select?.isConnected) {
      select.focus({ preventScroll: true });
    }
  }

  function positionMenu(select) {
    const anchor = select.getBoundingClientRect();
    const viewportWidth = document.documentElement.clientWidth || window.innerWidth;
    const viewportHeight = window.innerHeight;
    const margin = 8;
    const gap = 6;
    const width = Math.min(
      Math.max(anchor.width, 180),
      Math.max(180, viewportWidth - margin * 2),
    );
    const availableBelow = viewportHeight - anchor.bottom - gap - margin;
    const availableAbove = anchor.top - gap - margin;
    const placeAbove = availableBelow < 180 && availableAbove > availableBelow;
    const availableHeight = Math.max(96, placeAbove ? availableAbove : availableBelow);

    menu.style.width = `${Math.round(width)}px`;
    menu.style.maxHeight = `${Math.round(availableHeight)}px`;
    menu.style.left = `${Math.round(
      Math.min(Math.max(anchor.left, margin), Math.max(margin, viewportWidth - width - margin)),
    )}px`;

    const renderedHeight = Math.min(menu.scrollHeight, availableHeight);
    const top = placeAbove
      ? anchor.top - renderedHeight - gap
      : anchor.bottom + gap;
    menu.style.top = `${Math.round(Math.max(margin, top))}px`;
  }

  function appendOptionButton(select, option) {
    const button = document.createElement("button");
    button.type = "button";
    button.setAttribute("role", "menuitem");
    button.dataset.value = option.value;
    button.textContent = option.textContent || option.label || option.value;
    button.disabled = option.disabled;
    button.classList.toggle("is-selected", option.selected);
    if (option.selected) {
      button.setAttribute("aria-current", "true");
    }
    button.addEventListener("click", () => {
      if (option.disabled) {
        return;
      }
      const changed = select.value !== option.value;
      select.value = option.value;
      closeMenu();
      if (changed) {
        select.dispatchEvent(new Event("input", { bubbles: true }));
        select.dispatchEvent(new Event("change", { bubbles: true }));
      }
    });
    menu.appendChild(button);
  }

  function openMenu(select) {
    if (!(select instanceof HTMLSelectElement) || select.disabled || select.hidden) {
      return;
    }
    activeSelect = select;
    const menuHost = select.closest("dialog") || document.body;
    if (menu.parentElement !== menuHost) {
      menuHost.appendChild(menu);
    }
    menu.replaceChildren();
    menu.setAttribute("aria-label", select.getAttribute("aria-label") || "选择命令");

    for (const child of select.children) {
      if (child instanceof HTMLOptGroupElement) {
        const heading = document.createElement("div");
        heading.className = "terminal-android-select-menu-group";
        heading.textContent = child.label;
        heading.setAttribute("role", "presentation");
        menu.appendChild(heading);
        for (const option of child.children) {
          appendOptionButton(select, option);
        }
      } else if (child instanceof HTMLOptionElement) {
        appendOptionButton(select, child);
      }
    }

    menu.hidden = false;
    positionMenu(select);
    window.requestAnimationFrame(() => {
      if (activeSelect === select) {
        positionMenu(select);
        menu.querySelector(".is-selected")?.scrollIntoView({ block: "nearest" });
      }
    });
  }

  document.addEventListener("pointerdown", (event) => {
    const select = event.target.closest?.("select");
    if (!select || event.pointerType !== "touch") {
      return;
    }
    event.preventDefault();
    event.stopImmediatePropagation();
    openMenu(select);
  }, true);

  document.addEventListener("click", (event) => {
    const select = event.target.closest?.("select");
    if (select && event.isTrusted) {
      event.preventDefault();
      event.stopImmediatePropagation();
      openMenu(select);
      return;
    }
    if (!menu.hidden && !menu.contains(event.target)) {
      closeMenu();
    }
  }, true);

  document.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && !menu.hidden) {
      event.preventDefault();
      closeMenu({ restoreFocus: true });
    }
  });
  window.addEventListener("resize", () => activeSelect && positionMenu(activeSelect));
  window.addEventListener("scroll", () => activeSelect && positionMenu(activeSelect), true);
})();
