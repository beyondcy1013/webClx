function normalizeTerminalServerTarget(rawTarget) {
  const target = String(rawTarget || "").trim();
  if (!target) {
    throw new Error("请选择服务器。");
  }

  const url = new URL(/^https?:\/\//i.test(target) ? target : `http://${target}`);
  if (!url.hostname || url.username || url.password || (url.pathname && url.pathname !== "/")) {
    throw new Error("服务器地址无效。");
  }
  if (!url.port) {
    url.port = "11111";
  }
  return url.origin;
}

function buildTerminalServerSwitchUrl(rawTarget, locationLike = window.location) {
  const target = normalizeTerminalServerTarget(rawTarget);
  return `${target}${locationLike.pathname || "/"}${locationLike.search || ""}${locationLike.hash || ""}`;
}

function navigateToTerminalServer(
  rawTarget,
  locationLike = window.location,
  androidBridge = window.WebClxAndroid,
) {
  const targetUrl = buildTerminalServerSwitchUrl(rawTarget, locationLike);
  if (typeof androidBridge?.openInWebView === "function") {
    androidBridge.openInWebView(targetUrl);
    return targetUrl;
  }
  locationLike.assign(targetUrl);
  return targetUrl;
}

function openTerminalServerSwitchDialog() {
  const dialog = document.getElementById("terminal-server-switch-dialog");
  const select = document.getElementById("terminal-server-switch-select");
  if (!dialog) {
    return;
  }
  if (!dialog.open) {
    dialog.showModal();
  }
  select?.focus();
}

(() => {
  const dialog = document.getElementById("terminal-server-switch-dialog");
  const form = document.getElementById("terminal-server-switch-form");
  const select = document.getElementById("terminal-server-switch-select");
  const cancel = document.getElementById("terminal-server-switch-cancel");

  cancel?.addEventListener("click", () => dialog?.close());
  form?.addEventListener("submit", (event) => {
    event.preventDefault();
    navigateToTerminalServer(select?.value);
  });
})();
