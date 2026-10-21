(function () {
  function createFrpManager(deps) {
    const {
      state,
      requestJson,
      escapeHtml,
      setInlineStatus,
      elements,
    } = deps;
    const {
      frpcStatusSummaryEl,
      frpcConfigPathEl,
      frpcBinaryPathEl,
      frpcDownloadPlatformEl,
      frpcBinaryInputEl,
      frpcServerAddrInputEl,
      frpcServerPortInputEl,
      frpcTokenInputEl,
      frpcTlsInputEl,
      frpcProxyNameInputEl,
      frpcProxyTypeInputEl,
      frpcLocalIpInputEl,
      frpcLocalPortInputEl,
      frpcRemotePortFieldEl,
      frpcRemotePortInputEl,
      frpcCustomDomainsFieldEl,
      frpcCustomDomainsInputEl,
      frpcExtraTomlInputEl,
      frpcRefreshBtnEl,
      frpcDownloadBtnEl,
      frpcStartBtnEl,
      frpcStopBtnEl,
      frpcRestartBtnEl,
      frpcSaveBtnEl,
      frpcSaveStartBtnEl,
      frpcStatusMessageEl,
      frpcLogTailEl,
      frpsStatusSummaryEl,
      frpsConfigPathEl,
      frpsBinaryPathEl,
      frpsDownloadPlatformEl,
      frpsBinaryInputEl,
      frpsBindAddrInputEl,
      frpsBindPortInputEl,
      frpsTokenInputEl,
      frpsWebAddrInputEl,
      frpsWebPortInputEl,
      frpsDashboardUserInputEl,
      frpsDashboardPasswordInputEl,
      frpsExtraTomlInputEl,
      frpsRefreshBtnEl,
      frpsDownloadBtnEl,
      frpsStartBtnEl,
      frpsStopBtnEl,
      frpsRestartBtnEl,
      frpsSaveBtnEl,
      frpsSaveStartBtnEl,
      frpsStatusMessageEl,
      frpsLogTailEl,
      frpRoleRefreshBtnEl,
      frpSystemRefreshBtnEl,
      frpServerRoleRefreshBtnEl,
      frpServerSystemRefreshBtnEl,
      frpRoleNewFrpcBtnEl,
      frpRoleNewFrpsBtnEl,
      frpRolePlatformEl,
      frpServerRolePlatformEl,
      frpRoleCurrentSummaryEl,
      frpServerCurrentSummaryEl,
      frpServerRoleTableBodyEl,
      frpClientRoleTableBodyEl,
      frpSystemTableBodyEl,
      frpServerSystemTableBodyEl,
      frpRoleStatusMessageEl,
      frpServerRoleStatusMessageEl,
      frpRoleEditorEl,
      frpRoleCloseBtnEl,
      frpRoleEditorTitleEl,
      frpRoleIdInputEl,
      frpRoleNameInputEl,
      frpRoleComponentInputEl,
      frpRoleBinarySourceInputEl,
      frpRoleBinaryInputEl,
      frpRoleExternalConfigInputEl,
      frpRoleFrpcFieldsEl,
      frpRoleFrpcServerAddrInputEl,
      frpRoleFrpcServerPortInputEl,
      frpRoleFrpcTokenInputEl,
      frpRoleFrpcTlsInputEl,
      frpRoleFrpcProxyNameInputEl,
      frpRoleFrpcProxyTypeInputEl,
      frpRoleFrpcLocalIpInputEl,
      frpRoleFrpcLocalPortInputEl,
      frpRoleFrpcRemotePortFieldEl,
      frpRoleFrpcRemotePortInputEl,
      frpRoleFrpcCustomDomainsFieldEl,
      frpRoleFrpcCustomDomainsInputEl,
      frpRoleFrpcProxyTableBodyEl,
      frpRoleFrpcProxySelectAllEl,
      frpRoleFrpcProxyAddBtnEl,
      frpRoleFrpcProxyEditSelectedBtnEl,
      frpRoleFrpcProxyDuplicateSelectedBtnEl,
      frpRoleFrpcProxyDeleteSelectedBtnEl,
      frpRoleFrpcProxySelectedCountEl,
      frpRoleFrpcProxyStatusEl,
      frpRoleFrpcProxyEditorEl,
      frpRoleFrpcProxySaveBtnEl,
      frpRoleFrpcProxyCancelBtnEl,
      frpRoleFrpsFieldsEl,
      frpRoleFrpsPublicAddrInputEl,
      frpRoleFrpsBindAddrInputEl,
      frpRoleFrpsBindPortInputEl,
      frpRoleFrpsTokenInputEl,
      frpRoleFrpsWebAddrInputEl,
      frpRoleFrpsWebPortInputEl,
      frpRoleFrpsDashboardUserInputEl,
      frpRoleFrpsDashboardPasswordInputEl,
      frpRoleExtraTomlInputEl,
      frpRoleDownloadBtnEl,
      frpRoleStartBtnEl,
      frpRoleStopBtnEl,
      frpRoleRestartBtnEl,
      frpRoleDeleteBtnEl,
      frpRoleSaveBtnEl,
      frpRoleSaveStartBtnEl,
      frpRoleResetBtnEl,
      frpRoleEditorStatusEl,
      frpRoleLogTailEl,
      frpSourceModeInputEl,
      frpSourceManualPanelEl,
      frpSourceSystemPanelEl,
      frpSourceComponentInputEl,
      frpSourceSystemSelectEl,
      frpSourcePublicAddrInputEl,
      frpSourcePublicPortInputEl,
      frpSourceAuthTokenInputEl,
      frpSourceTestBtnEl,
      frpSourceAddBtnEl,
      frpSourceAdoptSelectedBtnEl,
      frpSourceStatusEl,
      frpServerSourceModeInputEl,
      frpServerSourceManualPanelEl,
      frpServerSourceSystemPanelEl,
      frpServerSourceSystemSelectEl,
      frpServerSourcePublicAddrInputEl,
      frpServerSourcePublicPortInputEl,
      frpServerSystemPublicAddrInputEl,
      frpServerSourceTestBtnEl,
      frpServerSourceAddBtnEl,
      frpServerSourceAdoptSelectedBtnEl,
      frpServerSourceStatusEl,
    } = elements;

function syncFrpcProxyTypeUi() {
  const type = frpcProxyTypeInputEl?.value || "tcp";
  const isTcp = type === "tcp";
  if (frpcRemotePortFieldEl) {
    frpcRemotePortFieldEl.hidden = !isTcp;
  }
  if (frpcCustomDomainsFieldEl) {
    frpcCustomDomainsFieldEl.hidden = isTcp;
  }
}

function readFrpcConfigFromForm() {
  const proxyType = frpcProxyTypeInputEl?.value || "tcp";
  return {
    enabled: true,
    binary_path: frpcBinaryInputEl?.value.trim() || "",
    server_addr: frpcServerAddrInputEl?.value.trim() || "",
    server_port: Number(frpcServerPortInputEl?.value || 7000),
    token: frpcTokenInputEl?.value.trim() || "",
    tls_enable: Boolean(frpcTlsInputEl?.checked),
    proxies: [{
      name: frpcProxyNameInputEl?.value.trim() || "webclx",
      proxy_type: proxyType,
      local_ip: frpcLocalIpInputEl?.value.trim() || "127.0.0.1",
      local_port: Number(frpcLocalPortInputEl?.value || 11111),
      remote_port: Number(frpcRemotePortInputEl?.value || 11111),
      custom_domains: frpcCustomDomainsInputEl?.value.trim() || "",
    }],
    extra_toml: frpcExtraTomlInputEl?.value || "",
  };
}

function formatFrpDownloadPlatform(platform) {
  if (!platform?.os || !platform?.arch) return "当前平台不支持自动下载";
  return `${platform.os}_${platform.arch}.${platform.archive_ext || ""}`.replace(/\.$/, "");
}

function renderFrpcStatus(data) {
  if (!frpcStatusSummaryEl) return;
  state.frpc = data || null;
  const config = data?.config || {};
  const proxy = Array.isArray(config.proxies) && config.proxies.length ? config.proxies[0] : {};

  if (frpcBinaryInputEl) frpcBinaryInputEl.value = config.binary_path || "";
  if (frpcServerAddrInputEl) frpcServerAddrInputEl.value = config.server_addr || "";
  if (frpcServerPortInputEl) frpcServerPortInputEl.value = config.server_port || 7000;
  if (frpcTokenInputEl) frpcTokenInputEl.value = config.token || "";
  if (frpcTlsInputEl) frpcTlsInputEl.checked = Boolean(config.tls_enable);
  if (frpcProxyNameInputEl) frpcProxyNameInputEl.value = proxy.name || "webclx";
  if (frpcProxyTypeInputEl) frpcProxyTypeInputEl.value = proxy.proxy_type || "tcp";
  if (frpcLocalIpInputEl) frpcLocalIpInputEl.value = proxy.local_ip || "127.0.0.1";
  if (frpcLocalPortInputEl) frpcLocalPortInputEl.value = proxy.local_port || 11111;
  if (frpcRemotePortInputEl) frpcRemotePortInputEl.value = proxy.remote_port || 11111;
  if (frpcCustomDomainsInputEl) frpcCustomDomainsInputEl.value = proxy.custom_domains || "";
  if (frpcExtraTomlInputEl) frpcExtraTomlInputEl.value = config.extra_toml || "";
  syncFrpcProxyTypeUi();

  const statusParts = [];
  statusParts.push(data?.running ? `运行中 pid=${data.pid || "-"}` : "未运行");
  if (!data?.configured) statusParts.push("配置未完整");
  if (data?.last_error) statusParts.push(data.last_error);
  frpcStatusSummaryEl.textContent = statusParts.join("；");
  frpcConfigPathEl.textContent = data?.generated_config_path || data?.config_path || "—";
  frpcBinaryPathEl.textContent = data?.binary_path || "未找到 frpc";
  if (frpcDownloadPlatformEl) frpcDownloadPlatformEl.textContent = formatFrpDownloadPlatform(data?.download_platform);
  frpcLogTailEl.textContent = data?.log_tail || "暂无日志";
  frpcStartBtnEl.disabled = Boolean(data?.running);
  frpcStopBtnEl.disabled = !data?.running;
  frpcRestartBtnEl.disabled = !data?.configured;
  if (frpcDownloadBtnEl) frpcDownloadBtnEl.disabled = Boolean(data?.running);
}

async function loadFrpcStatus() {
  if (!frpcStatusSummaryEl) return;
  try {
    const data = await requestJson("/api/frpc");
    renderFrpcStatus(data);
    if (frpcStatusMessageEl && !frpcStatusMessageEl.textContent) {
      setInlineStatus(frpcStatusMessageEl, "frpc 会由 webClx 托管；部署时把 frpc 放在 webClx 运行目录即可。", "muted");
    }
  } catch (error) {
    frpcStatusSummaryEl.textContent = "加载失败";
    frpcBinaryPathEl.textContent = error.message;
    if (frpcStatusMessageEl) {
      setInlineStatus(frpcStatusMessageEl, "读取 frpc 状态失败: " + error.message, "warn");
    }
  }
}

async function downloadFrpcBinary() {
  if (!frpcDownloadBtnEl) return;
  frpcDownloadBtnEl.disabled = true;
  setInlineStatus(frpcStatusMessageEl, "正在下载 frpc…", "info");
  try {
    const result = await requestJson("/api/frpc/download", { method: "POST" });
    await loadFrpcStatus();
    setInlineStatus(frpcStatusMessageEl, `frpc 已安装到 ${result.binary_path || "本机运行目录"}。`, "ok");
  } catch (error) {
    await loadFrpcStatus();
    setInlineStatus(frpcStatusMessageEl, "下载 frpc 失败: " + error.message, "warn");
  } finally {
    if (frpcDownloadBtnEl && !state.frpc?.running) frpcDownloadBtnEl.disabled = false;
  }
}

async function saveFrpcConfig({ start = false } = {}) {
  if (!frpcSaveBtnEl) return;
  const config = readFrpcConfigFromForm();
  setInlineStatus(frpcStatusMessageEl, "正在保存 frpc 配置…", "info");
  try {
    const data = await requestJson("/api/frpc", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ config }),
    });
    renderFrpcStatus(data);
    if (start) {
      await runFrpcCommand("start", "正在启动 frpc…", "配置已保存并启动 frpc。");
    } else {
      setInlineStatus(frpcStatusMessageEl, "frpc 配置已保存。", "ok");
    }
  } catch (error) {
    setInlineStatus(frpcStatusMessageEl, "保存 frpc 配置失败: " + error.message, "warn");
  }
}

async function runFrpcCommand(command, pendingText, doneText) {
  setInlineStatus(frpcStatusMessageEl, pendingText, "info");
  try {
    const data = await requestJson(`/api/frpc/${command}`, { method: "POST" });
    renderFrpcStatus(data);
    setInlineStatus(frpcStatusMessageEl, doneText, "ok");
  } catch (error) {
    await loadFrpcStatus();
    setInlineStatus(frpcStatusMessageEl, `${doneText.replace(/。$/, "")}失败: ${error.message}`, "warn");
  }
}

function readFrpsConfigFromForm() {
  return {
    enabled: true,
    binary_path: frpsBinaryInputEl?.value.trim() || "",
    bind_addr: frpsBindAddrInputEl?.value.trim() || "0.0.0.0",
    bind_port: Number(frpsBindPortInputEl?.value || 7000),
    token: frpsTokenInputEl?.value.trim() || "",
    web_server_addr: frpsWebAddrInputEl?.value.trim() || "127.0.0.1",
    web_server_port: Number(frpsWebPortInputEl?.value || 17500),
    dashboard_user: frpsDashboardUserInputEl?.value.trim() || "",
    dashboard_password: frpsDashboardPasswordInputEl?.value.trim() || "",
    extra_toml: frpsExtraTomlInputEl?.value || "",
  };
}

function renderFrpsStatus(data) {
  if (!frpsStatusSummaryEl) return;
  state.frps = data || null;
  const config = data?.config || {};

  if (frpsBinaryInputEl) frpsBinaryInputEl.value = config.binary_path || "";
  if (frpsBindAddrInputEl) frpsBindAddrInputEl.value = config.bind_addr || "0.0.0.0";
  if (frpsBindPortInputEl) frpsBindPortInputEl.value = config.bind_port || 7000;
  if (frpsTokenInputEl) frpsTokenInputEl.value = config.token || "";
  if (frpsWebAddrInputEl) frpsWebAddrInputEl.value = config.web_server_addr || "127.0.0.1";
  if (frpsWebPortInputEl) frpsWebPortInputEl.value = config.web_server_port || 17500;
  if (frpsDashboardUserInputEl) frpsDashboardUserInputEl.value = config.dashboard_user || "";
  if (frpsDashboardPasswordInputEl) frpsDashboardPasswordInputEl.value = config.dashboard_password || "";
  if (frpsExtraTomlInputEl) frpsExtraTomlInputEl.value = config.extra_toml || "";

  const statusParts = [];
  statusParts.push(data?.running ? `运行中 pid=${data.pid || "-"}` : "未运行");
  if (!data?.configured) statusParts.push("配置未完整");
  if (data?.last_error) statusParts.push(data.last_error);
  frpsStatusSummaryEl.textContent = statusParts.join("；");
  frpsConfigPathEl.textContent = data?.generated_config_path || data?.config_path || "—";
  frpsBinaryPathEl.textContent = data?.binary_path || "未找到 frps";
  if (frpsDownloadPlatformEl) frpsDownloadPlatformEl.textContent = formatFrpDownloadPlatform(data?.download_platform);
  frpsLogTailEl.textContent = data?.log_tail || "暂无日志";
  frpsStartBtnEl.disabled = Boolean(data?.running);
  frpsStopBtnEl.disabled = !data?.running;
  frpsRestartBtnEl.disabled = !data?.configured;
  if (frpsDownloadBtnEl) frpsDownloadBtnEl.disabled = Boolean(data?.running);
}

async function loadFrpsStatus() {
  if (!frpsStatusSummaryEl) return;
  try {
    const data = await requestJson("/api/frps");
    renderFrpsStatus(data);
    if (frpsStatusMessageEl && !frpsStatusMessageEl.textContent) {
      setInlineStatus(frpsStatusMessageEl, "frps 会由 webClx 托管；可直接下载当前平台对应二进制。", "muted");
    }
  } catch (error) {
    frpsStatusSummaryEl.textContent = "加载失败";
    frpsBinaryPathEl.textContent = error.message;
    if (frpsStatusMessageEl) {
      setInlineStatus(frpsStatusMessageEl, "读取 frps 状态失败: " + error.message, "warn");
    }
  }
}

async function downloadFrpsBinary() {
  if (!frpsDownloadBtnEl) return;
  frpsDownloadBtnEl.disabled = true;
  setInlineStatus(frpsStatusMessageEl, "正在下载 frps…", "info");
  try {
    const result = await requestJson("/api/frps/download", { method: "POST" });
    await loadFrpsStatus();
    setInlineStatus(frpsStatusMessageEl, `frps 已安装到 ${result.binary_path || "本机运行目录"}。`, "ok");
  } catch (error) {
    await loadFrpsStatus();
    setInlineStatus(frpsStatusMessageEl, "下载 frps 失败: " + error.message, "warn");
  } finally {
    if (frpsDownloadBtnEl && !state.frps?.running) frpsDownloadBtnEl.disabled = false;
  }
}

async function saveFrpsConfig({ start = false } = {}) {
  if (!frpsSaveBtnEl) return;
  const config = readFrpsConfigFromForm();
  setInlineStatus(frpsStatusMessageEl, "正在保存 frps 配置…", "info");
  try {
    const data = await requestJson("/api/frps", {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ config }),
    });
    renderFrpsStatus(data);
    if (start) {
      await runFrpsCommand("start", "正在启动 frps…", "配置已保存并启动 frps。");
    } else {
      setInlineStatus(frpsStatusMessageEl, "frps 配置已保存。", "ok");
    }
  } catch (error) {
    setInlineStatus(frpsStatusMessageEl, "保存 frps 配置失败: " + error.message, "warn");
  }
}

async function runFrpsCommand(command, pendingText, doneText) {
  setInlineStatus(frpsStatusMessageEl, pendingText, "info");
  try {
    const data = await requestJson(`/api/frps/${command}`, { method: "POST" });
    renderFrpsStatus(data);
    setInlineStatus(frpsStatusMessageEl, doneText, "ok");
  } catch (error) {
    await loadFrpsStatus();
    setInlineStatus(frpsStatusMessageEl, `${doneText.replace(/。$/, "")}失败: ${error.message}`, "warn");
  }
}

function defaultFrpRole(component = "frpc") {
  const id = `${component}-${Date.now()}`;
  return {
    id,
    name: component === "frps" ? "新 frps" : "新 frpc",
    component,
    frpc: component === "frpc" ? {
      enabled: true,
      binary_source: "auto",
      binary_path: "",
      external_config_path: "",
      server_addr: "",
      server_port: 7000,
      token: "",
      tls_enable: false,
      proxies: [{
        name: "webclx",
        proxy_type: "tcp",
        local_ip: "127.0.0.1",
        local_port: 11111,
        remote_port: 11111,
        custom_domains: "",
      }],
      extra_toml: "",
    } : null,
    frps: component === "frps" ? {
      enabled: true,
      binary_source: "auto",
      binary_path: "",
      external_config_path: "",
      bind_addr: "0.0.0.0",
      bind_port: 7000,
      public_addr: "",
      token: "",
      web_server_addr: "127.0.0.1",
      web_server_port: 17500,
      dashboard_user: "",
      dashboard_password: "",
      extra_toml: "",
    } : null,
  };
}

function defaultFrpProxyConfig() {
  return {
    name: "webclx",
    proxy_type: "tcp",
    local_ip: "127.0.0.1",
    local_port: 11111,
    remote_port: 11111,
    custom_domains: "",
  };
}

function normalizeFrpProxyConfig(proxy = {}) {
  return {
    name: (proxy.name || "webclx").trim() || "webclx",
    proxy_type: (proxy.proxy_type || "tcp").trim().toLowerCase() || "tcp",
    local_ip: (proxy.local_ip || "127.0.0.1").trim() || "127.0.0.1",
    local_port: Number(proxy.local_port || 11111),
    remote_port: Number(proxy.remote_port || 11111),
    custom_domains: (proxy.custom_domains || "").trim(),
  };
}

function reconcileSelectedFrpProxies() {
  const max = state.frpRoleDraftProxies.length;
  for (const index of Array.from(state.selectedFrpProxyIndexes)) {
    if (!Number.isInteger(index) || index < 0 || index >= max) {
      state.selectedFrpProxyIndexes.delete(index);
    }
  }
}

function selectedFrpProxyIndexes() {
  return Array.from(state.selectedFrpProxyIndexes).sort((a, b) => a - b);
}

function formatFrpProxyTarget(proxy) {
  const type = proxy.proxy_type || "tcp";
  if (type === "tcp") {
    return proxy.remote_port ? String(proxy.remote_port) : "未配置";
  }
  return proxy.custom_domains || "未配置域名";
}

function renderFrpProxyRows() {
  if (!frpRoleFrpcProxyTableBodyEl) return;
  reconcileSelectedFrpProxies();
  if (!state.frpRoleDraftProxies.length) {
    frpRoleFrpcProxyTableBodyEl.innerHTML = `<tr><td colspan="7" class="meta-text">暂无 frpc 节点，点击“新增节点”添加。</td></tr>`;
  } else {
    frpRoleFrpcProxyTableBodyEl.innerHTML = state.frpRoleDraftProxies.map((proxy, index) => {
      const selected = state.selectedFrpProxyIndexes.has(index);
      const proxyType = proxy.proxy_type || "tcp";
      const remotePort = proxyType === "tcp" ? String(proxy.remote_port || 11111) : "";
      const customDomains = proxyType === "tcp" ? "" : (proxy.custom_domains || "");
      return `<tr data-frp-proxy-index="${index}" class="${selected ? "selected-row" : ""}">
        <td><input type="checkbox" data-select-frp-proxy="${index}" aria-label="选择节点 ${escapeHtml(proxy.name || String(index + 1))}" ${selected ? "checked" : ""} /></td>
        <td class="mono-text">${escapeHtml(proxy.name || "")}</td>
        <td>${escapeHtml(proxyType.toUpperCase())}</td>
        <td class="mono-text">${escapeHtml(proxy.local_ip || "127.0.0.1")}</td>
        <td class="mono-text">${escapeHtml(String(proxy.local_port || 11111))}</td>
        <td class="mono-text">${escapeHtml(remotePort)}</td>
        <td class="mono-text">${escapeHtml(customDomains)}</td>
      </tr>`;
    }).join("");
  }
  const count = state.selectedFrpProxyIndexes.size;
  if (frpRoleFrpcProxySelectedCountEl) {
    frpRoleFrpcProxySelectedCountEl.textContent = `已选 ${count} 个节点`;
  }
  if (frpRoleFrpcProxyEditSelectedBtnEl) frpRoleFrpcProxyEditSelectedBtnEl.disabled = count !== 1;
  if (frpRoleFrpcProxyDuplicateSelectedBtnEl) frpRoleFrpcProxyDuplicateSelectedBtnEl.disabled = count !== 1;
  if (frpRoleFrpcProxyDeleteSelectedBtnEl) frpRoleFrpcProxyDeleteSelectedBtnEl.disabled = count === 0;
  if (frpRoleFrpcProxySelectAllEl) {
    frpRoleFrpcProxySelectAllEl.checked = state.frpRoleDraftProxies.length > 0 && count === state.frpRoleDraftProxies.length;
    frpRoleFrpcProxySelectAllEl.indeterminate = count > 0 && count < state.frpRoleDraftProxies.length;
  }
}

function setFrpProxyEditorVisible(visible) {
  if (frpRoleFrpcProxyEditorEl) frpRoleFrpcProxyEditorEl.hidden = !visible;
  if (!visible) {
    state.editingFrpProxyIndex = -1;
  }
}

function fillFrpProxyEditor(proxy = defaultFrpProxyConfig(), index = -1) {
  state.editingFrpProxyIndex = index;
  const normalized = normalizeFrpProxyConfig(proxy);
  if (frpRoleFrpcProxyNameInputEl) frpRoleFrpcProxyNameInputEl.value = normalized.name;
  if (frpRoleFrpcProxyTypeInputEl) frpRoleFrpcProxyTypeInputEl.value = normalized.proxy_type;
  if (frpRoleFrpcLocalIpInputEl) frpRoleFrpcLocalIpInputEl.value = normalized.local_ip;
  if (frpRoleFrpcLocalPortInputEl) frpRoleFrpcLocalPortInputEl.value = normalized.local_port;
  if (frpRoleFrpcRemotePortInputEl) frpRoleFrpcRemotePortInputEl.value = normalized.remote_port;
  if (frpRoleFrpcCustomDomainsInputEl) frpRoleFrpcCustomDomainsInputEl.value = normalized.custom_domains;
  setFrpProxyEditorVisible(true);
  syncFrpRoleComponentUi();
}

function readFrpProxyFromEditor() {
  return normalizeFrpProxyConfig({
    name: frpRoleFrpcProxyNameInputEl?.value || "webclx",
    proxy_type: frpRoleFrpcProxyTypeInputEl?.value || "tcp",
    local_ip: frpRoleFrpcLocalIpInputEl?.value || "127.0.0.1",
    local_port: Number(frpRoleFrpcLocalPortInputEl?.value || 11111),
    remote_port: Number(frpRoleFrpcRemotePortInputEl?.value || 11111),
    custom_domains: frpRoleFrpcCustomDomainsInputEl?.value || "",
  });
}

function saveFrpProxyFromEditor() {
  const proxy = readFrpProxyFromEditor();
  if (state.editingFrpProxyIndex >= 0 && state.editingFrpProxyIndex < state.frpRoleDraftProxies.length) {
    state.frpRoleDraftProxies[state.editingFrpProxyIndex] = proxy;
    setInlineStatus(frpRoleFrpcProxyStatusEl, `节点 ${proxy.name} 已更新。`, "ok");
  } else {
    state.frpRoleDraftProxies.push(proxy);
    state.selectedFrpProxyIndexes.clear();
    state.selectedFrpProxyIndexes.add(state.frpRoleDraftProxies.length - 1);
    setInlineStatus(frpRoleFrpcProxyStatusEl, `节点 ${proxy.name} 已添加。`, "ok");
  }
  setFrpProxyEditorVisible(false);
  renderFrpProxyRows();
}

function editSelectedFrpProxy() {
  const indexes = selectedFrpProxyIndexes();
  if (indexes.length !== 1) return;
  fillFrpProxyEditor(state.frpRoleDraftProxies[indexes[0]], indexes[0]);
}

function duplicateSelectedFrpProxy() {
  const indexes = selectedFrpProxyIndexes();
  if (indexes.length !== 1) return;
  const source = normalizeFrpProxyConfig(state.frpRoleDraftProxies[indexes[0]]);
  const duplicate = { ...source, name: `${source.name || "webclx"}-copy` };
  state.frpRoleDraftProxies.push(duplicate);
  state.selectedFrpProxyIndexes.clear();
  state.selectedFrpProxyIndexes.add(state.frpRoleDraftProxies.length - 1);
  setInlineStatus(frpRoleFrpcProxyStatusEl, `已复制节点 ${source.name}。`, "ok");
  renderFrpProxyRows();
}

function deleteSelectedFrpProxies() {
  const indexes = selectedFrpProxyIndexes();
  if (!indexes.length) return;
  if (!window.confirm(`删除选中的 ${indexes.length} 个 frpc 节点？`)) return;
  const selected = new Set(indexes);
  state.frpRoleDraftProxies = state.frpRoleDraftProxies.filter((_, index) => !selected.has(index));
  state.selectedFrpProxyIndexes.clear();
  setFrpProxyEditorVisible(false);
  setInlineStatus(frpRoleFrpcProxyStatusEl, `已删除 ${indexes.length} 个节点。`, "ok");
  renderFrpProxyRows();
}

function setActiveFrpRoleTab(tab) {
  const nextTab = tab === "frpc" ? "frpc" : "frps";
  state.activeFrpRoleTab = nextTab;
  if (frpSourceComponentInputEl) {
    frpSourceComponentInputEl.value = "frpc";
    syncFrpSourceFields();
  }
  renderFrpSourceOptions();
}

function frpRoleStatusById(id) {
  return state.frpRoles.find((item) => item.role?.id === id) || null;
}

function selectedFrpRoleStatus() {
  return frpRoleStatusById(state.editingFrpRoleId);
}

function setFrpRoleEditorVisible(visible) {
  if (frpRoleEditorEl) {
    if (visible) {
      if (typeof frpRoleEditorEl.showModal === "function" && !frpRoleEditorEl.open) {
        frpRoleEditorEl.showModal();
      } else {
        frpRoleEditorEl.hidden = false;
      }
    } else if (frpRoleEditorEl.open && typeof frpRoleEditorEl.close === "function") {
      frpRoleEditorEl.close();
    } else {
      frpRoleEditorEl.hidden = true;
    }
  }
  if (!visible) {
    state.editingFrpRoleId = "";
    if (frpRoleCurrentSummaryEl) frpRoleCurrentSummaryEl.textContent = "未选择";
    if (frpServerCurrentSummaryEl) frpServerCurrentSummaryEl.textContent = "未选择";
  }
}

function syncFrpRoleComponentUi() {
  const component = frpRoleComponentInputEl?.value || "frpc";
  if (frpRoleFrpcFieldsEl) frpRoleFrpcFieldsEl.hidden = component !== "frpc";
  if (frpRoleFrpsFieldsEl) frpRoleFrpsFieldsEl.hidden = component !== "frps";
  const proxyType = frpRoleFrpcProxyTypeInputEl?.value || "tcp";
  if (frpRoleFrpcRemotePortFieldEl) frpRoleFrpcRemotePortFieldEl.hidden = proxyType !== "tcp";
  if (frpRoleFrpcCustomDomainsFieldEl) frpRoleFrpcCustomDomainsFieldEl.hidden = proxyType === "tcp";
  if (frpRoleEditorTitleEl) frpRoleEditorTitleEl.textContent = component === "frps" ? "服务器配置 / frps" : "客户端配置 / frpc";
  if (component !== "frpc") {
    setFrpProxyEditorVisible(false);
  }
}

function fillFrpRoleForm(statusOrRole) {
  const role = statusOrRole?.role || statusOrRole || defaultFrpRole("frpc");
  const component = role.component || "frpc";
  const config = component === "frps" ? (role.frps || {}) : (role.frpc || {});
  state.editingFrpRoleId = role.id || "";
  state.selectedFrpProxyIndexes.clear();
  state.editingFrpProxyIndex = -1;
  state.frpRoleDraftProxies = component === "frpc"
    ? (Array.isArray(role.frpc?.proxies) && role.frpc.proxies.length
      ? role.frpc.proxies.map((proxy) => normalizeFrpProxyConfig(proxy))
      : [defaultFrpProxyConfig()])
    : [];
  setFrpRoleEditorVisible(true);
  setActiveFrpRoleTab(component);
  if (frpRoleIdInputEl) {
    frpRoleIdInputEl.value = role.id || "";
    frpRoleIdInputEl.disabled = Boolean(frpRoleStatusById(role.id));
  }
  if (frpRoleNameInputEl) frpRoleNameInputEl.value = role.name || "";
  if (frpRoleComponentInputEl) {
    frpRoleComponentInputEl.value = component;
    frpRoleComponentInputEl.disabled = Boolean(frpRoleStatusById(role.id));
  }
  if (frpRoleBinarySourceInputEl) frpRoleBinarySourceInputEl.value = config.binary_source || "auto";
  if (frpRoleBinaryInputEl) frpRoleBinaryInputEl.value = config.binary_path || "";
  if (frpRoleExternalConfigInputEl) frpRoleExternalConfigInputEl.value = config.external_config_path || "";
  if (frpRoleFrpcServerAddrInputEl) frpRoleFrpcServerAddrInputEl.value = role.frpc?.server_addr || "";
  if (frpRoleFrpcServerPortInputEl) frpRoleFrpcServerPortInputEl.value = role.frpc?.server_port || 7000;
  if (frpRoleFrpcTokenInputEl) frpRoleFrpcTokenInputEl.value = role.frpc?.token || "";
  if (frpRoleFrpcTlsInputEl) frpRoleFrpcTlsInputEl.checked = Boolean(role.frpc?.tls_enable);
  setFrpProxyEditorVisible(false);
  renderFrpProxyRows();
  if (frpRoleFrpsPublicAddrInputEl) frpRoleFrpsPublicAddrInputEl.value = role.frps?.public_addr || "";
  if (frpRoleFrpsBindAddrInputEl) frpRoleFrpsBindAddrInputEl.value = role.frps?.bind_addr || "0.0.0.0";
  if (frpRoleFrpsBindPortInputEl) frpRoleFrpsBindPortInputEl.value = role.frps?.bind_port || 7000;
  if (frpRoleFrpsTokenInputEl) frpRoleFrpsTokenInputEl.value = role.frps?.token || "";
  if (frpRoleFrpsWebAddrInputEl) frpRoleFrpsWebAddrInputEl.value = role.frps?.web_server_addr || "127.0.0.1";
  if (frpRoleFrpsWebPortInputEl) frpRoleFrpsWebPortInputEl.value = role.frps?.web_server_port || 17500;
  if (frpRoleFrpsDashboardUserInputEl) frpRoleFrpsDashboardUserInputEl.value = role.frps?.dashboard_user || "";
  if (frpRoleFrpsDashboardPasswordInputEl) frpRoleFrpsDashboardPasswordInputEl.value = role.frps?.dashboard_password || "";
  if (frpRoleExtraTomlInputEl) frpRoleExtraTomlInputEl.value = config.extra_toml || "";
  if (frpRoleCurrentSummaryEl) frpRoleCurrentSummaryEl.textContent = `${role.name || role.id || "未命名"} / ${component}`;
  if (frpServerCurrentSummaryEl) frpServerCurrentSummaryEl.textContent = `${role.name || role.id || "未命名"} / ${component}`;
  if (frpRoleLogTailEl) frpRoleLogTailEl.textContent = statusOrRole?.log_tail || "暂无日志";
  renderFrpRoleEditorButtons();
  syncFrpRoleComponentUi();
}

function readFrpRoleFromForm() {
  const component = frpRoleComponentInputEl?.value || "frpc";
  const binarySource = frpRoleBinarySourceInputEl?.value || "auto";
  const base = {
    id: frpRoleIdInputEl?.value.trim() || `${component}-${Date.now()}`,
    name: frpRoleNameInputEl?.value.trim() || component,
    component,
    frpc: null,
    frps: null,
  };
  if (component === "frps") {
    base.frps = {
      enabled: true,
      binary_source: binarySource,
      binary_path: frpRoleBinaryInputEl?.value.trim() || "",
      external_config_path: frpRoleExternalConfigInputEl?.value.trim() || "",
      bind_addr: frpRoleFrpsBindAddrInputEl?.value.trim() || "0.0.0.0",
      bind_port: Number(frpRoleFrpsBindPortInputEl?.value || 7000),
      public_addr: frpRoleFrpsPublicAddrInputEl?.value.trim() || "",
      token: frpRoleFrpsTokenInputEl?.value.trim() || "",
      web_server_addr: frpRoleFrpsWebAddrInputEl?.value.trim() || "127.0.0.1",
      web_server_port: Number(frpRoleFrpsWebPortInputEl?.value || 17500),
      dashboard_user: frpRoleFrpsDashboardUserInputEl?.value.trim() || "",
      dashboard_password: frpRoleFrpsDashboardPasswordInputEl?.value.trim() || "",
      extra_toml: frpRoleExtraTomlInputEl?.value || "",
    };
  } else {
    const proxies = state.frpRoleDraftProxies.map((proxy) => normalizeFrpProxyConfig(proxy));
    base.frpc = {
      enabled: true,
      binary_source: binarySource,
      binary_path: frpRoleBinaryInputEl?.value.trim() || "",
      external_config_path: frpRoleExternalConfigInputEl?.value.trim() || "",
      server_addr: frpRoleFrpcServerAddrInputEl?.value.trim() || "",
      server_port: Number(frpRoleFrpcServerPortInputEl?.value || 7000),
      token: frpRoleFrpcTokenInputEl?.value.trim() || "",
      tls_enable: Boolean(frpRoleFrpcTlsInputEl?.checked),
      proxies: proxies.length ? proxies : [defaultFrpProxyConfig()],
      extra_toml: frpRoleExtraTomlInputEl?.value || "",
    };
  }
  return base;
}

function describeFrpRoleEndpoint(role) {
  const config = role.component === "frps" ? role.frps : role.frpc;
  if (config?.external_config_path) {
    return `外部配置 ${config.external_config_path}`;
  }
  if (role.component === "frps") {
    const bind = `${role.frps?.bind_addr || "0.0.0.0"}:${role.frps?.bind_port || 7000}`;
    const publicAddr = role.frps?.public_addr || "";
    return publicAddr ? `${bind} / ${publicAddr}:${role.frps?.bind_port || 7000}` : bind;
  }
  const proxy = Array.isArray(role.frpc?.proxies) && role.frpc.proxies.length ? role.frpc.proxies[0] : {};
  return `${role.frpc?.server_addr || "未配置"}:${role.frpc?.server_port || 7000} -> ${proxy.local_ip || "127.0.0.1"}:${proxy.local_port || 11111}`;
}

function describeFrpRoleSource(role) {
  const config = role.component === "frps" ? role.frps : role.frpc;
  if (config?.external_config_path) return "外部配置";
  const source = config?.binary_source || "auto";
  if (source === "system") return "系统 PATH";
  if (source === "bundled") return "自带/下载";
  if (source === "custom") return "指定路径";
  return "自动";
}

function describeFrpRolePublicTarget(role) {
  if (role.component === "frps") {
    const port = role.frps?.bind_port || 7000;
    const publicAddr = role.frps?.public_addr || "";
    return publicAddr ? `${publicAddr}:${port}` : `公网未填 / ${role.frps?.bind_addr || "0.0.0.0"}:${port}`;
  }
  const proxy = Array.isArray(role.frpc?.proxies) && role.frpc.proxies.length ? role.frpc.proxies[0] : {};
  if ((proxy.proxy_type || "tcp") === "tcp") {
    return `${role.frpc?.server_addr || "未配置"}:${proxy.remote_port || 11111}`;
  }
  return proxy.custom_domains || role.frpc?.server_addr || "未配置";
}

function describeFrpcProxySummary(role) {
  const proxies = Array.isArray(role.frpc?.proxies) ? role.frpc.proxies : [];
  if (!proxies.length) return "未配置节点";
  const first = normalizeFrpProxyConfig(proxies[0]);
  const firstTarget = `${first.name}:${formatFrpProxyTarget(first)}`;
  return proxies.length === 1 ? firstTarget : `${firstTarget} 等 ${proxies.length} 个`;
}

function renderFrpRoleEditorButtons() {
  const status = selectedFrpRoleStatus();
  const hasSavedRole = Boolean(status);
  const running = Boolean(status?.running);
  if (frpRoleStartBtnEl) frpRoleStartBtnEl.disabled = !hasSavedRole || running;
  if (frpRoleStopBtnEl) frpRoleStopBtnEl.disabled = !hasSavedRole || !running;
  if (frpRoleRestartBtnEl) frpRoleRestartBtnEl.disabled = !hasSavedRole || !status?.configured;
  if (frpRoleDownloadBtnEl) frpRoleDownloadBtnEl.disabled = !hasSavedRole || running;
  if (frpRoleDeleteBtnEl) frpRoleDeleteBtnEl.disabled = !hasSavedRole || running;
}

function renderFrpRoleActionCell(item) {
  return `<td>
    <div class="toolbar compact-toolbar">
      <button class="button secondary" data-frp-role-action="edit">配置</button>
      <button class="button secondary" data-frp-role-action="test">测试</button>
      <button class="button secondary" data-frp-role-action="start" ${item.running ? "disabled" : ""}>启动</button>
      <button class="button secondary" data-frp-role-action="stop" ${item.running ? "" : "disabled"}>停止</button>
    </div>
  </td>`;
}

function renderFrpServerRoleRows(items) {
  if (!items.length) {
    return `<tr><td colspan="7" class="meta-text">暂无已管理 frps 服务器。通过下方“新建来源”添加；未添加的系统 FRP 不会被 webClx 管理。</td></tr>`;
  }
  return items.map((item) => {
    const role = item.role || {};
    const status = item.running ? `运行中 pid=${item.pid || "-"}` : (item.configured ? "未运行" : "配置未完整");
    return `<tr data-frp-role-id="${escapeHtml(role.id || "")}">
      <td>${escapeHtml(role.name || role.id || "")}</td>
      <td>${escapeHtml(describeFrpRoleSource(role))}</td>
      <td class="mono-text">${escapeHtml(`${role.frps?.bind_addr || "0.0.0.0"}:${role.frps?.bind_port || 7000}`)}</td>
      <td class="mono-text">${escapeHtml(role.frps?.public_addr ? `${role.frps.public_addr}:${role.frps?.bind_port || 7000}` : "公网未填")}</td>
      <td class="mono-text">${escapeHtml(item.generated_config_path || item.binary_path || "未生成")}</td>
      <td>${escapeHtml(status)}</td>
      ${renderFrpRoleActionCell(item)}
    </tr>`;
  }).join("");
}

function renderFrpClientRoleRows(items) {
  if (!items.length) {
    return `<tr><td colspan="7" class="meta-text">暂无已管理 frpc 客户端。通过下方“新建来源”添加；每个客户端的节点可在配置弹窗内管理。</td></tr>`;
  }
  return items.map((item) => {
    const role = item.role || {};
    const status = item.running ? `运行中 pid=${item.pid || "-"}` : (item.configured ? "未运行" : "配置未完整");
    return `<tr data-frp-role-id="${escapeHtml(role.id || "")}">
      <td>${escapeHtml(role.name || role.id || "")}</td>
      <td>${escapeHtml(describeFrpRoleSource(role))}</td>
      <td class="mono-text">${escapeHtml(`${role.frpc?.server_addr || "未配置"}:${role.frpc?.server_port || 7000}`)}</td>
      <td class="mono-text">${escapeHtml(describeFrpcProxySummary(role))}</td>
      <td class="mono-text">${escapeHtml(item.generated_config_path || item.binary_path || "未生成")}</td>
      <td>${escapeHtml(status)}</td>
      ${renderFrpRoleActionCell(item)}
    </tr>`;
  }).join("");
}

function renderFrpRoles(data) {
  state.frpRoles = Array.isArray(data?.roles) ? data.roles : [];
  state.frpRoleDownloadPlatform = data?.download_platform || null;
  if (frpRolePlatformEl) frpRolePlatformEl.textContent = formatFrpDownloadPlatform(state.frpRoleDownloadPlatform);
  if (frpServerRolePlatformEl) frpServerRolePlatformEl.textContent = formatFrpDownloadPlatform(state.frpRoleDownloadPlatform);
  const serverRoles = state.frpRoles.filter((item) => item.role?.component === "frps");
  const clientRoles = state.frpRoles.filter((item) => item.role?.component !== "frps");
  if (frpServerRoleTableBodyEl) frpServerRoleTableBodyEl.innerHTML = renderFrpServerRoleRows(serverRoles);
  if (frpClientRoleTableBodyEl) frpClientRoleTableBodyEl.innerHTML = renderFrpClientRoleRows(clientRoles);
  if (!state.frpRoles.length) {
    setFrpRoleEditorVisible(false);
    renderFrpSourceOptions();
    return;
  }
  if (state.editingFrpRoleId) {
    const selected = frpRoleStatusById(state.editingFrpRoleId);
    if (selected) {
      fillFrpRoleForm(selected);
    } else {
      setFrpRoleEditorVisible(false);
    }
  }
  renderFrpSourceOptions();
}

function frpSystemItemById(id) {
  return state.frpSystemItems.find((item) => item.id === id) || null;
}

function roleFromFrpSystemItem(item) {
  const component = item?.component || "frpc";
  const role = defaultFrpRole(component);
  role.id = `${component}-system-${Date.now()}`;
  role.name = `系统 ${component}`;
  const config = component === "frps" ? role.frps : role.frpc;
  if (config) {
    config.binary_source = "system";
    config.binary_path = "";
    if (item?.config_path) config.external_config_path = item.config_path;
    if (component === "frps" && frpServerSourcePublicAddrInputEl) {
      config.public_addr = frpServerSystemPublicAddrInputEl?.value.trim() || frpServerSourcePublicAddrInputEl.value.trim();
    }
  }
  return role;
}

function syncFrpCreateSourceModeUi(component = "frpc") {
  const isServer = component === "frps";
  const modeInput = isServer ? frpServerSourceModeInputEl : frpSourceModeInputEl;
  const manualPanel = isServer ? frpServerSourceManualPanelEl : frpSourceManualPanelEl;
  const systemPanel = isServer ? frpServerSourceSystemPanelEl : frpSourceSystemPanelEl;
  const mode = modeInput?.value || "";
  if (manualPanel) manualPanel.hidden = mode !== "manual";
  if (systemPanel) systemPanel.hidden = mode !== "system";
}

function setFrpCreateSourceMode(component, mode) {
  const modeInput = component === "frps" ? frpServerSourceModeInputEl : frpSourceModeInputEl;
  if (modeInput) modeInput.value = mode;
  syncFrpCreateSourceModeUi(component);
}

function frpCreateSourceStatusElement(component) {
  return component === "frps" ? frpServerSourceStatusEl : frpSourceStatusEl;
}

function syncFrpSourceFields() {
  if (frpSourceComponentInputEl) frpSourceComponentInputEl.value = "frpc";
  if (frpSourcePublicAddrInputEl) frpSourcePublicAddrInputEl.placeholder = "frps 公网地址";
  if (frpSourcePublicPortInputEl && !frpSourcePublicPortInputEl.value) frpSourcePublicPortInputEl.value = "11111";
  syncFrpCreateSourceModeUi("frpc");
  syncFrpCreateSourceModeUi("frps");
  renderFrpSourceOptions();
}

function renderFrpSourceOptions() {
  renderFrpSourceSelect(frpSourceSystemSelectEl, "frpc");
  renderFrpSourceSelect(frpServerSourceSystemSelectEl, "frps");
}

function renderFrpSourceSelect(selectEl, component) {
  if (!selectEl) return;
  const currentValue = selectEl.value;
  const options = [`<option value="">选择检测到的 ${escapeHtml(component)}</option>`];
  state.frpSystemItems.forEach((item) => {
    if (item.managed_role_id || item.component !== component) return;
    const label = `${item.component === "frps" ? "服务器" : "客户端"} / ${item.source || "系统"} / ${item.config_path || item.binary_path || item.id}`;
    options.push(`<option value="${escapeHtml(item.id || "")}">${escapeHtml(label)}</option>`);
  });
  selectEl.innerHTML = options.join("");
  if (currentValue && state.frpSystemItems.some((item) => item.id === currentValue && !item.managed_role_id && item.component === component)) {
    selectEl.value = currentValue;
  } else {
    selectEl.value = "";
  }
}

function renderFrpSystemItems(data) {
  state.frpSystemItems = Array.isArray(data?.items) ? data.items : [];
  renderFrpSourceOptions();
  renderFrpSystemTable(frpSystemTableBodyEl, "frpc");
  renderFrpSystemTable(frpServerSystemTableBodyEl, "frps");
}

function renderFrpSystemTable(tableBodyEl, component) {
  if (!tableBodyEl) return;
  const items = state.frpSystemItems.filter((item) => item.component === component);
  if (!items.length) {
    tableBodyEl.innerHTML = `<tr><td colspan="8" class="meta-text">未检测到系统 ${escapeHtml(component)} PATH 二进制或正在运行的进程。</td></tr>`;
    return;
  }
  tableBodyEl.innerHTML = items.map((item) => {
    const managed = item.managed_role_id ? `已接管: ${item.managed_role_id}` : "未接管";
    const canAdopt = Boolean(item.config_path);
    const actionButton = item.managed_role_id
      ? `<button class="button secondary danger" data-frp-system-action="unmanage">取消接管</button>`
      : `<button class="button secondary" data-frp-system-action="${canAdopt ? "adopt" : "use"}">添加</button>`;
    return `<tr data-frp-system-id="${escapeHtml(item.id || "")}">
      <td class="mono-text">${escapeHtml(item.component || "")}</td>
      <td>${escapeHtml(item.source || "")}</td>
      <td class="mono-text">${escapeHtml(item.pid ? String(item.pid) : "-")}</td>
      <td class="mono-text">${escapeHtml(item.binary_path || "")}</td>
      <td class="mono-text">${escapeHtml(item.config_path || "-")}</td>
      <td class="mono-text">${escapeHtml(item.command || "")}</td>
      <td>${escapeHtml(managed)}</td>
      <td>
        <div class="toolbar compact-toolbar">
          ${actionButton}
        </div>
      </td>
    </tr>`;
  }).join("");
}

async function loadFrpSystemItems() {
  if (!frpSystemTableBodyEl && !frpServerSystemTableBodyEl) return;
  if (frpSystemTableBodyEl) frpSystemTableBodyEl.innerHTML = `<tr><td colspan="8" class="meta-text">正在检测系统 frpc…</td></tr>`;
  if (frpServerSystemTableBodyEl) frpServerSystemTableBodyEl.innerHTML = `<tr><td colspan="8" class="meta-text">正在检测系统 frps…</td></tr>`;
  try {
    const data = await requestJson("/api/frp/system");
    renderFrpSystemItems(data);
    setInlineStatus(frpRoleStatusMessageEl, "系统 FRP 检测完成。", "muted");
    setInlineStatus(frpServerRoleStatusMessageEl, "系统 FRP 检测完成。", "muted");
  } catch (error) {
    const errorRow = `<tr><td colspan="8" class="meta-text" style="color:#cf6f6f">检测失败：${escapeHtml(error.message)}</td></tr>`;
    if (frpSystemTableBodyEl) frpSystemTableBodyEl.innerHTML = errorRow;
    if (frpServerSystemTableBodyEl) frpServerSystemTableBodyEl.innerHTML = errorRow;
    setInlineStatus(frpRoleStatusMessageEl, "检测系统 FRP 失败: " + error.message, "warn");
    setInlineStatus(frpServerRoleStatusMessageEl, "检测系统 FRP 失败: " + error.message, "warn");
  }
}

async function adoptFrpSystemItem(item) {
  if (!item) return;
  const statusEl = frpCreateSourceStatusElement(item.component);
  const serverPublicAddrInput = frpServerSystemPublicAddrInputEl || frpServerSourcePublicAddrInputEl;
  if (item.component === "frps" && !serverPublicAddrInput?.value.trim()) {
    setInlineStatus(statusEl, "接管服务器前必须填写公网地址。", "warn");
    serverPublicAddrInput?.focus();
    return;
  }
  if (!item.config_path) {
    fillFrpRoleForm(roleFromFrpSystemItem(item));
    setInlineStatus(frpRoleEditorStatusEl, "已选择系统 PATH 来源，请补齐配置后保存。", "muted");
    return;
  }
  setInlineStatus(statusEl, "正在接管系统 FRP…", "info");
  try {
    const data = await requestJson("/api/frp/system/adopt", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        component: item.component,
        binary_path: item.binary_path || "",
        config_path: item.config_path || "",
        public_addr: item.component === "frps" ? (serverPublicAddrInput?.value.trim() || "") : "",
      }),
    });
    renderFrpRoles(data);
    await loadFrpSystemItems();
    const adopted = state.frpRoles.find((roleStatus) => {
      const role = roleStatus.role || {};
      const config = role.component === "frps" ? role.frps : role.frpc;
      return config?.external_config_path === item.config_path;
    });
    if (adopted) fillFrpRoleForm(adopted);
    setInlineStatus(statusEl, "系统 FRP 已接管到角色表。", "ok");
  } catch (error) {
    setInlineStatus(statusEl, "接管系统 FRP 失败: " + error.message, "warn");
  }
}

async function unmanageFrpSystemItem(item) {
  const roleId = item?.managed_role_id || "";
  if (!roleId) return;
  const label = item.config_path || item.binary_path || roleId;
  if (!window.confirm(`取消接管 ${label}？系统中的 frp 进程和配置文件不会被停止或删除。`)) return;
  const statusEl = frpCreateSourceStatusElement(item.component);
  setInlineStatus(statusEl, `正在取消接管 ${roleId}…`, "info");
  try {
    const data = await requestJson(`/api/frp/roles/${encodeURIComponent(roleId)}/unmanage`, { method: "POST" });
    if (state.editingFrpRoleId === roleId) {
      state.editingFrpRoleId = "";
      setFrpRoleEditorVisible(false);
    }
    renderFrpRoles(data);
    await loadFrpSystemItems();
    setInlineStatus(statusEl, `已取消接管 ${roleId}。`, "ok");
  } catch (error) {
    setInlineStatus(statusEl, "取消接管失败: " + error.message, "warn");
  }
}

async function loadFrpRoles() {
  if (!frpServerRoleTableBodyEl && !frpClientRoleTableBodyEl) return;
  try {
    const data = await requestJson("/api/frp/roles");
    renderFrpRoles(data);
    if (frpRoleStatusMessageEl && !frpRoleStatusMessageEl.textContent) {
      setInlineStatus(frpRoleStatusMessageEl, "FRPC 客户端已加载。", "muted");
    }
    if (frpServerRoleStatusMessageEl && !frpServerRoleStatusMessageEl.textContent) {
      setInlineStatus(frpServerRoleStatusMessageEl, "FRP 服务器已加载。", "muted");
    }
  } catch (error) {
    const errorRow = `<tr><td colspan="7" class="meta-text" style="color:#cf6f6f">加载失败：${escapeHtml(error.message)}</td></tr>`;
    if (frpServerRoleTableBodyEl) frpServerRoleTableBodyEl.innerHTML = errorRow;
    if (frpClientRoleTableBodyEl) frpClientRoleTableBodyEl.innerHTML = errorRow;
    if (frpRoleStatusMessageEl) setInlineStatus(frpRoleStatusMessageEl, "读取 FRP 角色失败: " + error.message, "warn");
    if (frpServerRoleStatusMessageEl) setInlineStatus(frpServerRoleStatusMessageEl, "读取 FRP 角色失败: " + error.message, "warn");
  }
}

async function saveFrpRole({ start = false } = {}) {
  if (frpRoleComponentInputEl?.value === "frpc" && frpRoleFrpcProxyEditorEl && !frpRoleFrpcProxyEditorEl.hidden) {
    saveFrpProxyFromEditor();
  }
  const role = readFrpRoleFromForm();
  if (role.component === "frps" && !role.frps?.public_addr) {
    setInlineStatus(frpRoleEditorStatusEl, "服务器必须填写公网地址。", "warn");
    frpRoleFrpsPublicAddrInputEl?.focus();
    return;
  }
  setInlineStatus(frpRoleEditorStatusEl, "正在保存 FRP 角色…", "info");
  try {
    const data = await requestJson("/api/frp/roles", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ role }),
    });
    state.editingFrpRoleId = role.id;
    renderFrpRoles(data);
    if (start) {
      await runFrpRoleCommand("start", "正在启动 FRP 角色…", "FRP 角色已保存并启动。");
    } else {
      setInlineStatus(frpRoleEditorStatusEl, "FRP 角色已保存。", "ok");
    }
  } catch (error) {
    setInlineStatus(frpRoleEditorStatusEl, "保存 FRP 角色失败: " + error.message, "warn");
  }
}

async function testFrpPublicPort(host, port, statusEl = frpSourceStatusEl) {
  const trimmedHost = (host || "").trim();
  const numericPort = Number(port || 0);
  if (!trimmedHost || !numericPort) {
    setInlineStatus(statusEl, "请填写公网地址和端口。", "warn");
    return null;
  }
  setInlineStatus(statusEl, `正在测试 ${trimmedHost}:${numericPort}…`, "info");
  try {
    const result = await requestJson("/api/frp/test-port", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ host: trimmedHost, port: numericPort, timeout_secs: 5 }),
    });
    if (result.ok) {
      setInlineStatus(statusEl, `测试成功：${result.target} 可连接，耗时 ${result.elapsed_ms}ms。`, "ok");
    } else {
      setInlineStatus(statusEl, `测试失败：${result.target} 不可连接，${result.error || "未知错误"}。`, "warn");
    }
    return result;
  } catch (error) {
    setInlineStatus(statusEl, "测试失败: " + error.message, "warn");
    return null;
  }
}

async function testFrpRolePublicPort(id) {
  const status = frpRoleStatusById(id);
  const role = status?.role;
  if (!role) return;
  let host = "";
  let port = 0;
  if (role.component === "frps") {
    host = role.frps?.public_addr || role.frps?.bind_addr || "";
    port = role.frps?.bind_port || 7000;
  } else {
    const proxy = Array.isArray(role.frpc?.proxies) && role.frpc.proxies.length ? role.frpc.proxies[0] : {};
    host = role.frpc?.server_addr || "";
    port = proxy.remote_port || role.frpc?.server_port || 7000;
  }
  await testFrpPublicPort(host, port, frpRoleStatusMessageEl);
}

async function addFrpSourceToManaged() {
  const mode = frpSourceModeInputEl?.value || "";
  if (!mode) {
    setInlineStatus(frpSourceStatusEl, "请先选择来源。", "warn");
    frpSourceModeInputEl?.focus();
    return;
  }
  if (mode === "system") {
    const selectedId = frpSourceSystemSelectEl?.value || "";
    const item = selectedId ? frpSystemItemById(selectedId) : null;
    if (!item) {
      setInlineStatus(frpSourceStatusEl, "请选择一个检测到的 frpc 来源，或点击表格中的添加。", "warn");
      frpSourceSystemSelectEl?.focus();
      return;
    }
    await adoptFrpSystemItem(item);
    return;
  }
  const component = frpSourceComponentInputEl?.value || "frpc";
  const role = defaultFrpRole(component);
  if (component === "frps") {
    role.name = "新 frps 服务器";
    role.frps.public_addr = frpSourcePublicAddrInputEl?.value.trim() || "";
    role.frps.bind_port = Number(frpSourcePublicPortInputEl?.value || 7000);
    if (!role.frps.public_addr) {
      setInlineStatus(frpSourceStatusEl, "服务器必须填写公网地址。", "warn");
      frpSourcePublicAddrInputEl?.focus();
      return;
    }
  } else {
    role.name = "新 frpc 客户端";
    role.frpc.server_addr = frpSourcePublicAddrInputEl?.value.trim() || "";
    role.frpc.server_port = Number(frpSourcePublicPortInputEl?.value || 7000);
    role.frpc.token = frpSourceAuthTokenInputEl?.value.trim() || "";
    role.frpc.proxies[0].remote_port = Number(frpSourcePublicPortInputEl?.value || 11111);
  }
  fillFrpRoleForm(role);
  setInlineStatus(frpRoleEditorStatusEl, "已创建草稿，补齐配置后保存。", "muted");
}

async function addFrpServerSourceToManaged() {
  const mode = frpServerSourceModeInputEl?.value || "";
  if (!mode) {
    setInlineStatus(frpServerSourceStatusEl, "请先选择来源。", "warn");
    frpServerSourceModeInputEl?.focus();
    return;
  }
  if (mode === "system") {
    const selectedId = frpServerSourceSystemSelectEl?.value || "";
    const item = selectedId ? frpSystemItemById(selectedId) : null;
    if (!item) {
      setInlineStatus(frpServerSourceStatusEl, "请选择一个检测到的 frps 来源，或点击表格中的添加。", "warn");
      frpServerSourceSystemSelectEl?.focus();
      return;
    }
    await adoptFrpSystemItem(item);
    return;
  }
  const role = defaultFrpRole("frps");
  role.name = "新 frps 服务器";
  role.frps.public_addr = frpServerSourcePublicAddrInputEl?.value.trim() || "";
  role.frps.bind_port = Number(frpServerSourcePublicPortInputEl?.value || 7000);
  if (!role.frps.public_addr) {
    setInlineStatus(frpServerSourceStatusEl, "服务器必须填写公网地址。", "warn");
    frpServerSourcePublicAddrInputEl?.focus();
    return;
  }
  fillFrpRoleForm(role);
  setInlineStatus(frpRoleEditorStatusEl, "已创建服务器草稿，补齐配置后保存。", "muted");
}

async function runFrpRoleCommand(command, pendingText, doneText, id = state.editingFrpRoleId) {
  if (!id) return;
  setInlineStatus(frpRoleEditorStatusEl, pendingText, "info");
  try {
    const data = await requestJson(`/api/frp/roles/${encodeURIComponent(id)}/${command}`, { method: "POST" });
    renderFrpRoles(data);
    setInlineStatus(frpRoleEditorStatusEl, doneText, "ok");
  } catch (error) {
    await loadFrpRoles();
    setInlineStatus(frpRoleEditorStatusEl, `${doneText.replace(/。$/, "")}失败: ${error.message}`, "warn");
  }
}

async function downloadSelectedFrpRoleBinary() {
  const id = state.editingFrpRoleId;
  if (!id) return;
  setInlineStatus(frpRoleEditorStatusEl, "正在下载 FRP 二进制…", "info");
  try {
    const result = await requestJson(`/api/frp/roles/${encodeURIComponent(id)}/download`, { method: "POST" });
    await loadFrpRoles();
    setInlineStatus(frpRoleEditorStatusEl, `已安装到 ${result.binary_path || "角色目录"}。`, "ok");
  } catch (error) {
    await loadFrpRoles();
    setInlineStatus(frpRoleEditorStatusEl, "下载 FRP 二进制失败: " + error.message, "warn");
  }
}

async function deleteSelectedFrpRole() {
  const id = state.editingFrpRoleId;
  if (!id) return;
  if (!window.confirm("删除当前 FRP 角色？")) return;
  try {
    const data = await requestJson(`/api/frp/roles/${encodeURIComponent(id)}`, { method: "DELETE" });
    state.editingFrpRoleId = "";
    renderFrpRoles(data);
    setInlineStatus(frpRoleEditorStatusEl, "FRP 角色已删除。", "ok");
  } catch (error) {
    setInlineStatus(frpRoleEditorStatusEl, "删除 FRP 角色失败: " + error.message, "warn");
  }
}

    return {
      addFrpServerSourceToManaged,
      addFrpSourceToManaged,
      adoptFrpSystemItem,
      defaultFrpProxyConfig,
      defaultFrpRole,
      deleteSelectedFrpProxies,
      deleteSelectedFrpRole,
      downloadFrpcBinary,
      downloadFrpsBinary,
      downloadSelectedFrpRoleBinary,
      duplicateSelectedFrpProxy,
      editSelectedFrpProxy,
      fillFrpProxyEditor,
      fillFrpRoleForm,
      frpRoleStatusById,
      frpSystemItemById,
      loadFrpRoles,
      loadFrpSystemItems,
      loadFrpcStatus,
      loadFrpsStatus,
      renderFrpProxyRows,
      runFrpRoleCommand,
      runFrpcCommand,
      runFrpsCommand,
      saveFrpProxyFromEditor,
      saveFrpRole,
      saveFrpcConfig,
      saveFrpsConfig,
      selectedFrpRoleStatus,
      setActiveFrpRoleTab,
      setFrpCreateSourceMode,
      setFrpProxyEditorVisible,
      setFrpRoleEditorVisible,
      syncFrpCreateSourceModeUi,
      syncFrpcProxyTypeUi,
      syncFrpRoleComponentUi,
      testFrpPublicPort,
      testFrpRolePublicPort,
      unmanageFrpSystemItem,
    };
  }

  globalThis.WebClxFrpManager = Object.freeze({ create: createFrpManager });
})();
