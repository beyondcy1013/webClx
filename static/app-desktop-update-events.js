const DEFAULT_DESKTOP_REMOTE_URL = "https://192.168.3.2:14083/";

function normalizeDesktopRemoteUrl(value) {
  const trimmed = typeof value === "string" ? value.trim() : "";
  if (!trimmed) {
    return DEFAULT_DESKTOP_REMOTE_URL;
  }
  const candidate = trimmed.includes("://") ? trimmed : `https://${trimmed}`;
  try {
    const url = new URL(candidate);
    if (!["http:", "https:"].includes(url.protocol) || !url.hostname) {
      return DEFAULT_DESKTOP_REMOTE_URL;
    }
    return candidate;
  } catch (_error) {
    return DEFAULT_DESKTOP_REMOTE_URL;
  }
}

function normalizeDesktopRemoteUrlHistory(values) {
  const seen = new Set();
  const history = [];
  (Array.isArray(values) ? values : []).forEach((value) => {
    const normalized = normalizeDesktopRemoteUrl(value);
    if (!seen.has(normalized)) {
      seen.add(normalized);
      history.push(normalized);
    }
  });
  return history.slice(0, 20);
}

function renderDesktopRemoteUrlHistory() {
  setDatalistOptions(desktopUrlHistoryEl, state.desktopRemoteUrlHistory);
}

// 跨域 iframe 无法可靠读取 load 事件或错误状态。这里用超时启发式：
// 若较长时间内没有触发 load 事件，认为加载被拦截（通常是自签名证书未信任），
// 显示针对自签名证书的引导。
const desktopFallbackTimer = (() => {
  let timer = null;
  const desktopFallbackEl = document.getElementById("desktop-fallback");
  const arm = () => {
    if (timer) {
      clearTimeout(timer);
    }
    timer = setTimeout(() => {
      if (desktopFallbackEl) {
        desktopFallbackEl.hidden = false;
      }
    }, 8000);
  };
  const clear = () => {
    if (timer) {
      clearTimeout(timer);
      timer = null;
    }
    if (desktopFallbackEl) {
      desktopFallbackEl.hidden = true;
    }
  };
  return { arm, clear };
})();

function getDesktopFrameUrl() {
  return state.desktopRemoteUrl || DEFAULT_DESKTOP_REMOTE_URL;
}

function applyDesktopRemoteUrl(url) {
  const normalized = normalizeDesktopRemoteUrl(url);
  state.desktopRemoteUrl = normalized;
  if (desktopUrlInputEl) {
    desktopUrlInputEl.value = normalized;
  }
  if (desktopOpenButtonEl) {
    desktopOpenButtonEl.href = normalized;
  }
  if (desktopFallbackUrlEl) {
    desktopFallbackUrlEl.textContent = normalized;
  }
  if (desktopFallbackOpenEl) {
    desktopFallbackOpenEl.href = normalized;
  }
  renderDesktopRemoteUrlHistory();
  // Set iframe src to load the desktop URL.
  if (desktopFrameEl) {
    desktopFrameEl.src = normalized;
    desktopFallbackTimer.arm();
  }
}

function rememberDesktopRemoteUrl(url) {
  const normalized = normalizeDesktopRemoteUrl(url);
  state.desktopRemoteUrlHistory = normalizeDesktopRemoteUrlHistory([
    normalized,
    ...state.desktopRemoteUrlHistory,
  ]);
  renderDesktopRemoteUrlHistory();
}

function bindDesktopFrameEvents() {
  const desktopReloadAfterTrustButtonEl = document.getElementById("desktop-reload-after-trust");

  function reloadDesktopFrame() {
    if (!desktopFrameEl) {
      return;
    }
    const url = getDesktopFrameUrl();
    // 重新赋值 src 触发刷新；先清空可避免相同 src 时部分浏览器不重载。
    desktopFrameEl.src = url;
    desktopFallbackTimer.arm();
  }

  function applyAndReloadDesktop() {
    const rawUrl = desktopUrlInputEl?.value?.trim() || "";
    const normalized = normalizeDesktopRemoteUrl(rawUrl);
    applyDesktopRemoteUrl(normalized);
    rememberDesktopRemoteUrl(normalized);
  }

  if (desktopFrameEl) {
    desktopFrameEl.addEventListener("load", desktopFallbackTimer.clear);
  }

  if (desktopReloadButtonEl) {
    desktopReloadButtonEl.addEventListener("click", reloadDesktopFrame);
  }

  if (desktopApplyUrlButtonEl) {
    desktopApplyUrlButtonEl.addEventListener("click", applyAndReloadDesktop);
  }

  if (desktopUrlInputEl) {
    desktopUrlInputEl.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        applyAndReloadDesktop();
      }
    });
    desktopUrlInputEl.addEventListener("input", () => {
      desktopUrlInputEl.setCustomValidity("");
    });
  }

  if (desktopReloadAfterTrustButtonEl) {
    // 用户在新窗口手动信任自签名证书后，点击此按钮重载 iframe。
    desktopReloadAfterTrustButtonEl.addEventListener("click", reloadDesktopFrame);
  }
}

function bindUpdateEventHandlers() {
  async function startRemoteUpdate(url) {
    updateStatus(updateProgressEl, "正在下载更新...", "muted");
    updateDownloadBtnEl.disabled = true;
    try {
      const response = await fetch(url, { signal: AbortSignal.timeout(300000) });
      if (!response.ok) throw new Error(`HTTP ${response.status}`);
      const blob = await response.blob();
      updateStatus(updateProgressEl, `下载完成，大小: ${(blob.size / 1024 / 1024).toFixed(2)} MB`, "ok");
      updateStatus(updateProgressEl, "注意：实际替换需要手动操作或将服务配置为自动更新", "muted");
    } catch (error) {
      updateStatus(updateProgressEl, `下载失败: ${error.message}`, "warn");
      updateDownloadBtnEl.disabled = false;
    }
  }

  if (updateCopyUrlBtn) {
    updateCopyUrlBtn.addEventListener("click", () => {
      const updateUrl = getUpdateDownloadUrl();
      if (navigator.clipboard?.writeText) {
        navigator.clipboard.writeText(updateUrl).then(() => {
          updateStatus(updateCopyUrlStatusEl, "已复制更新地址", "ok");
        }).catch(() => {
          updateStatus(updateCopyUrlStatusEl, "复制失败", "warn");
        });
      } else {
        updateStatus(updateCopyUrlStatusEl, "浏览器不支持剪贴板", "warn");
      }
    });
  }

  if (updateCheckRemoteBtn) {
    updateCheckRemoteBtn.addEventListener("click", async () => {
      const remoteUrl = updateRemoteCheckUrlInput?.value?.trim();
      if (!remoteUrl) {
        updateStatus(updateCheckStatusEl, "请输入远程版本检查 URL", "warn");
        return;
      }
      updateStatus(updateCheckStatusEl, "正在检查...", "muted");
      try {
        const response = await fetch(remoteUrl, { signal: AbortSignal.timeout(10000) });
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        const info = await response.json();
        const remoteVersion = normalizeVersionText(info.version || info.current_version);
        const downloadUrl = normalizeVersionText(info.url || info.binary_url);
        const currentVersion = normalizeVersionText(state.serverVersion) || "0";
        const versionCheck = describeRemoteVersionCheck(remoteVersion, currentVersion);
        updateLatestVersionEl.textContent = remoteVersion || "未知";
        updateDownloadUrlEl.textContent = downloadUrl || "未知";
        updateAvailablePanelEl.style.display = "grid";
        updateDownloadBtnEl.disabled = !downloadUrl;
        if (downloadUrl) {
          updateDownloadBtnEl.onclick = () => {
            const current = normalizeVersionText(state.serverVersion) || "0";
            const comparison = compareNumericVersions(remoteVersion, current);
            if (comparison === null || comparison <= 0) {
              if (!confirmForcedRemoteUpdate({
                remoteVersion,
                currentVersion: current,
                comparison,
              })) {
                updateStatus(updateProgressEl, "已取消强制更新。", "muted");
                return;
              }
            }
            startRemoteUpdate(downloadUrl);
          };
        }
        updateStatus(updateCheckStatusEl, versionCheck.message, versionCheck.tone);
      } catch (error) {
        updateStatus(updateCheckStatusEl, `检查失败: ${error.message}`, "warn");
        updateAvailablePanelEl.style.display = "none";
      }
    });
  }

  const remoteCopySettingsButton = document.getElementById("remote-copy-settings");
  if (remoteCopySettingsButton) {
    remoteCopySettingsButton.addEventListener("click", () => {
      openRemoteCopyDialog();
    });
  }
}
