(function () {
  function createProxyManager(deps) {
    const {
      state,
      requestJson,
      escapeHtml,
      createActionButton,
      createActionCell,
      createTextCell,
      renderPresetTable,
      renderPresetTableHeader,
      movePresetById,
      persistPresetOrder,
      formatDateTimeLong,
      formatEnvList,
      DEFAULT_PROXY_CODEX_PROMPT,
      frpManager,
      elements,
    } = deps;
    const {
      proxyPresetsListEl,
      proxyNameInputEl,
      proxyTypeInputEl,
      proxyServerInputEl,
      proxyUsernameInputEl,
      proxyPasswordInputEl,
      proxyEditingIdEl,
      proxyTestBtnEl,
      proxyTestModeInputEls,
      proxyTestModeHintEl,
      proxyTestUrlFieldEl,
      proxyTestUrlInputEl,
      proxyCodexPromptFieldEl,
      proxyCodexPromptInputEl,
      proxySaveBtnEl,
      proxyClearBtnEl,
      proxyTestResultEl,
      proxyFormTitleEl,
      proxyActiveSummaryEl,
      proxyEffectiveEnvEl,
      proxyScopeSummaryEl,
      proxyHostEnvSummaryEl,
      proxyLastTestSummaryEl,
      proxyLastTestTimeEl,
      proxyClearActiveBtnEl,
      systemAppProxyActiveEl,
      systemAppProxyEnvEl,
    } = elements;

async function loadProxyPresets() {
  try {
    const data = await requestJson("/api/proxy/presets");
    state.proxyPresets = data.presets || [];
    state.activeProxyId = data.active_id || null;
    renderProxyPresets();
    await loadAppProxyStatus();
    await frpManager.loadFrpRoles();
    await frpManager.loadFrpSystemItems();
  } catch (error) {
    proxyPresetsListEl.innerHTML = `<tr><td colspan="6" class="meta-text" style="color:#cf6f6f">加载失败：${escapeHtml(error.message)}</td></tr>`;
    proxyActiveSummaryEl.textContent = '加载失败';
    proxyEffectiveEnvEl.textContent = error.message;
  }
}

async function applyProxyPreset(id) {
  try {
    await requestJson(`/api/proxy/active`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ preset_id: id }),
    });
    state.activeProxyId = id;
    renderProxyPresets();
    await loadAppProxyStatus();
    showProxyResult('程序代理已应用；本程序的后端 HTTP 请求立即生效，新建终端会继承该代理。', 'ok');
  } catch (error) {
    showProxyResult('应用失败：' + error.message, 'warn');
  }
}

async function moveProxyPresetOrder(presetId, direction) {
  const nextPresets = movePresetById(state.proxyPresets, presetId, direction);
  if (!nextPresets) {
    return;
  }
  const previousPresets = state.proxyPresets;
  state.proxyPresets = nextPresets;
  state.presetTableSort?.delete?.("proxy");
  renderProxyPresets();

  try {
    await persistPresetOrder("/api/proxy/presets/reorder", state.proxyPresets);
    showProxyResult("代理预设顺序已保存。", "ok");
  } catch (error) {
    state.proxyPresets = previousPresets;
    renderProxyPresets();
    showProxyResult("保存代理预设顺序失败：" + error.message, "warn");
  }
}

function renderProxyPresets() {
  const sortColumns = [
    { key: "active", type: "boolean", defaultDirection: "desc", getValue: (preset) => preset?.id === state.activeProxyId },
    { key: "name", type: "text", getValue: (preset) => preset?.name || "" },
    { key: "type", type: "text", getValue: (preset) => preset?.proxy_type || "" },
    { key: "server", type: "text", getValue: (preset) => preset?.server || "" },
  ];

  renderPresetTableHeader({
    listEl: proxyPresetsListEl,
    tableKey: "proxy",
    sortColumns,
    onSortChange: renderProxyPresets,
    baseLabels: [
      { label: "序号" },
      { label: "排序" },
      { label: "当前", sortKey: "active" },
      { label: "名称", sortKey: "name" },
      { label: "类型", sortKey: "type" },
      { label: "服务器", sortKey: "server" },
      { label: "操作" },
    ],
  });

  renderPresetTable({
    listEl: proxyPresetsListEl,
    presets: state.proxyPresets,
    emptyText: "暂无预设",
    emptyColspan: 7,
    tableKey: "proxy",
    sortColumns,
    order: {
      enabled: true,
      onMove: moveProxyPresetOrder,
    },
    buildCells: (preset) => {
      const isActive = preset.id === state.activeProxyId;
      const currentCell = document.createElement("td");
      currentCell.className = "proxy-preset-current-cell";
      if (isActive) {
        const badge = document.createElement("span");
        badge.className = "proxy-preset-active-badge";
        badge.textContent = "已应用";
        currentCell.appendChild(badge);
      } else {
        currentCell.textContent = "—";
      }

      const nameCell = createTextCell(preset.name || "—", "proxy-preset-name-cell");
      nameCell.title = preset.name || "";

      const typeCell = document.createElement("td");
      const typeBadge = document.createElement("span");
      typeBadge.className = "proxy-preset-type-badge";
      typeBadge.textContent = String(preset.proxy_type || "").toUpperCase() || "—";
      typeCell.appendChild(typeBadge);

      const serverCell = createTextCell(preset.server || "—", "mono-text proxy-preset-server-cell");
      serverCell.title = preset.server || "";

      const applyButton = createActionButton(isActive ? "已应用" : "应用", () => applyProxyPreset(preset.id), "mini-button accent");
      applyButton.disabled = isActive;
      applyButton.classList.add("apply-preset-btn");
      const testButton = createActionButton("测试", () => testProxyByPreset(preset.id), "mini-button");
      const editButton = createActionButton("编辑", () => editProxyPreset(preset.id), "mini-button");
      const deleteButton = createActionButton("删除", () => deleteProxyPreset(preset.id), "mini-button delete-btn");
      const actionsCell = createActionCell(
        [applyButton, testButton, editButton, deleteButton],
        "proxy-preset-actions-cell",
        "actions preset-actions proxy-preset-actions",
      );

      return [currentCell, nameCell, typeCell, serverCell, actionsCell];
    },
    decorateRow: (row, preset) => {
      if (preset.id === state.activeProxyId) {
        row.classList.add("proxy-preset-active-row");
      }
    },
  });
}

function editProxyPreset(id) {
  const preset = state.proxyPresets.find(p => p.id === id);
  if (!preset) return;
  proxyEditingIdEl.value = preset.id;
  proxyNameInputEl.value = preset.name;
  proxyTypeInputEl.value = preset.proxy_type;
  proxyServerInputEl.value = preset.server;
  proxyUsernameInputEl.value = preset.username || '';
  proxyPasswordInputEl.value = '';
  proxyFormTitleEl.textContent = '编辑代理预设';
  proxyTestResultEl.hidden = true;
}

function clearProxyForm() {
  proxyEditingIdEl.value = '';
  proxyNameInputEl.value = '';
  proxyTypeInputEl.value = 'http';
  proxyServerInputEl.value = '';
  proxyUsernameInputEl.value = '';
  proxyPasswordInputEl.value = '';
  if (proxyTestUrlInputEl) proxyTestUrlInputEl.value = '';
  proxyFormTitleEl.textContent = '新增代理预设';
  proxyTestResultEl.hidden = true;
}

async function saveProxyPreset() {
  const name = proxyNameInputEl.value.trim();
  const server = proxyServerInputEl.value.trim();
  if (!name) { showProxyResult('请填写预设名称。', 'warn'); return; }
  if (!server) { showProxyResult('请填写服务器地址。', 'warn'); return; }
  const editingId = proxyEditingIdEl.value;
  const url = editingId
    ? `/api/proxy/presets/${encodeURIComponent(editingId)}`
    : '/api/proxy/presets';
  const method = editingId ? 'PUT' : 'POST';
  try {
    await requestJson(url, {
      method,
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        name,
        proxy_type: proxyTypeInputEl.value,
        server,
        enabled: true,
        username: proxyUsernameInputEl.value.trim(),
        password: proxyPasswordInputEl.value,
      }),
    });
    await loadProxyPresets();
    clearProxyForm();
    showProxyResult('保存成功。', 'ok');
  } catch (error) {
    showProxyResult('保存失败：' + error.message, 'warn');
  }
}

async function deleteProxyPreset(id) {
  if (!confirm('确定要删除该代理预设吗？')) return;
  try {
    await requestJson(`/api/proxy/presets/${encodeURIComponent(id)}`, { method: 'DELETE' });
    if (proxyEditingIdEl.value === id) clearProxyForm();
    await loadProxyPresets();
    await loadAppProxyStatus();
  } catch (error) {
    showProxyResult('删除失败：' + error.message, 'warn');
  }
}

function getSelectedProxyTestMode() {
  return proxyTestModeInputEls.find(input => input.checked)?.value || 'http';
}

function syncProxyTestModeUi() {
  const mode = getSelectedProxyTestMode();
  const isCodexExec = mode === 'codex_exec';

  if (proxyTestModeHintEl) {
    proxyTestModeHintEl.textContent = isCodexExec
      ? 'Codex Exec 会实际运行 `codex exec`，更接近你真正想测的 Codex 链路。'
      : 'HTTP 访问会通过代理直接请求目标 URL，适合检查网页连通性。';
  }
  if (proxyTestUrlFieldEl) {
    proxyTestUrlFieldEl.hidden = isCodexExec;
  }
  if (proxyCodexPromptFieldEl) {
    proxyCodexPromptFieldEl.hidden = !isCodexExec;
  }
  if (proxyTestBtnEl) {
    proxyTestBtnEl.textContent = isCodexExec ? '运行 Codex 测试' : '测试连接';
  }
  if (proxyCodexPromptInputEl && !proxyCodexPromptInputEl.value.trim()) {
    proxyCodexPromptInputEl.value = DEFAULT_PROXY_CODEX_PROMPT;
  }
}

function renderProxyTestPending(mode) {
  const step2 = mode === 'codex_exec'
    ? '步骤 2/2：等待开始运行 codex exec…'
    : '步骤 2/2：等待开始访问测试地址…';
  return [
    '<div style="margin-bottom:6px">步骤 1/2：正在检查代理服务器连通性…</div>',
    `<div>${step2}</div>`,
  ].join('');
}

function collectProxyHttpTestTargets() {
  const checkedUrls = Array.from(
    document.querySelectorAll('.proxy-test-url-check:checked')
  ).map(cb => cb.value);
  const customUrl = proxyTestUrlInputEl.value.trim();
  return [...new Set([...checkedUrls, ...(customUrl ? [customUrl] : [])])];
}

async function executeProxyTestSequence(payloads, sourceLabel) {
  proxyTestResultEl.hidden = false;
  proxyTestResultEl.innerHTML = renderProxyTestPending(payloads[0]?.test_mode || 'http');
  proxyTestResultEl.dataset.tone = 'muted';

  const results = [];
  for (const payload of payloads) {
    try {
      const result = await requestJson('/api/proxy/test', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(payload),
      });
      results.push(result);
    } catch (error) {
      results.push({
        ok: false,
        proxy_connect_ok: false,
        target_access_ok: false,
        error: error.message,
        test_mode: payload.test_mode || 'http',
        test_url: payload.test_url || '',
        command_prompt: payload.codex_prompt || '',
      });
    }
    proxyTestResultEl.innerHTML = renderProxyTestResults(results, payloads.length);
    proxyTestResultEl.dataset.tone = getProxyResultsTone(results, payloads.length);
  }

  proxyTestResultEl.hidden = false;
  proxyTestResultEl.dataset.tone = getProxyResultsTone(results, payloads.length);
  proxyTestResultEl.innerHTML = renderProxyTestResults(results, payloads.length);
  rememberProxyTest(results, sourceLabel);
}

function buildProxyTestPayloads(proxyType, server, options = {}) {
  const proxyFields = options.presetId
    ? { preset_id: options.presetId }
    : {
        username: options.username || '',
        password: options.password || '',
      };
  const mode = getSelectedProxyTestMode();
  if (mode === 'codex_exec') {
    const prompt = (proxyCodexPromptInputEl?.value || '').trim() || DEFAULT_PROXY_CODEX_PROMPT;
    if (proxyCodexPromptInputEl) {
      proxyCodexPromptInputEl.value = prompt;
    }
    return [{
      proxy_type: proxyType,
      server,
      ...proxyFields,
      test_mode: mode,
      codex_prompt: prompt,
    }];
  }

  const urls = collectProxyHttpTestTargets();
  if (!urls.length) {
    return [];
  }
  return urls.map(testUrl => ({
    proxy_type: proxyType,
    server,
    ...proxyFields,
    test_mode: mode,
    test_url: testUrl,
  }));
}

async function testProxyByPreset(id) {
  const preset = state.proxyPresets.find(p => p.id === id);
  if (!preset) return;
  const payloads = buildProxyTestPayloads(preset.proxy_type, preset.server, {
    presetId: preset.id,
  });
  if (!payloads.length) {
    showProxyResult('请选择或输入至少一个测试目标。', 'warn');
    return;
  }
  await executeProxyTestSequence(payloads, `预设 ${preset.name}`);
}

async function testProxyFromForm() {
  const server = proxyServerInputEl.value.trim();
  if (!server) { showProxyResult('请先填写服务器地址。', 'warn'); return; }
  const payloads = buildProxyTestPayloads(proxyTypeInputEl.value, server, {
    username: proxyUsernameInputEl.value.trim(),
    password: proxyPasswordInputEl.value,
  });
  if (!payloads.length) {
    showProxyResult('请选择或输入至少一个测试目标。', 'warn');
    return;
  }

  proxyTestBtnEl.disabled = true;
  try {
    await executeProxyTestSequence(payloads, '当前输入');
  } finally {
    proxyTestBtnEl.disabled = false;
  }
}

function showProxyTestResult(result) {
  proxyTestResultEl.hidden = false;
  proxyTestResultEl.innerHTML = renderProxyTestResults([result], 1);
  proxyTestResultEl.dataset.tone = result.ok ? 'ok' : 'warn';
}

function showProxyResult(msg, tone) {
  proxyTestResultEl.hidden = false;
  proxyTestResultEl.textContent = msg;
  proxyTestResultEl.dataset.tone = tone;
}

function getProxyResultsTone(results, expectedCount = results.length) {
  if (!Array.isArray(results) || !results.length) return 'muted';
  if (results.length < expectedCount) return 'muted';
  return results.every(r => r.ok) ? 'ok' : 'warn';
}

function renderProxyTestResults(results, expectedCount = results.length) {
  if (!Array.isArray(results) || !results.length) {
    return '<div>暂无测试结果</div>';
  }

  const first = results[0] || {};
  const mode = first.test_mode || 'http';
  const proxyLabel = escapeHtml(first.proxy_url || '');
  const proxyOk = Boolean(first.proxy_connect_ok);
  const proxyElapsed = first.proxy_connect_elapsed_ms != null
    ? ` <span style="color:var(--text-secondary)">${first.proxy_connect_elapsed_ms}ms</span>`
    : '';
  const proxyLine = proxyOk
    ? `<div style="margin-bottom:8px"><span style="color:#27ae60">✓</span> 步骤 1/2：代理服务器可连接 <span class="mono-text">${proxyLabel}</span>${proxyElapsed}</div>`
    : `<div style="margin-bottom:8px"><span style="color:#e74c3c">✗</span> 步骤 1/2：代理服务器不可连接 <span class="mono-text">${proxyLabel}</span>${proxyElapsed}${first.proxy_connect_error ? ` <span style="color:#e74c3c">${escapeHtml(first.proxy_connect_error)}</span>` : ''}</div>`;

  if (mode === 'codex_exec') {
    const execRows = results.map(r => {
      const exitLabel = r.exit_code == null ? '未拿到退出码' : `退出 ${r.exit_code}`;
      const elapsed = r.elapsed_ms != null
        ? ` <span style="color:var(--text-secondary)">${r.elapsed_ms}ms</span>`
        : '';
      const statusLine = r.ok
        ? `<div style="margin-bottom:4px"><span style="color:#27ae60">✓</span> <span class="mono-text">codex exec</span> ${exitLabel}${elapsed}</div>`
        : `<div style="margin-bottom:4px"><span style="color:#e74c3c">✗</span> <span class="mono-text">codex exec</span> ${exitLabel}${elapsed}</div>`;
      const promptLine = r.command_prompt
        ? `<div class="proxy-test-result-meta"><strong>提示词：</strong><span class="mono-text">${escapeHtml(r.command_prompt)}</span></div>`
        : '';
      const lastMessageBlock = r.command_last_message
        ? `<pre class="proxy-test-command-output">${escapeHtml(r.command_last_message)}</pre>`
        : '';
      const outputBlock = r.command_output
        ? `<pre class="proxy-test-command-output">${escapeHtml(r.command_output)}</pre>`
        : '';
      const errorLine = !r.ok && r.error && !r.command_output
        ? `<div class="proxy-test-result-meta"><strong>错误：</strong>${escapeHtml(r.error)}</div>`
        : '';
      return [statusLine, promptLine, lastMessageBlock, outputBlock, errorLine].join('');
    }).join('');

    const waitingLine = results.length < expectedCount
      ? `<div style="margin-top:4px;color:var(--text-secondary)">步骤 2/2：正在运行 codex exec… ${results.length}/${expectedCount}</div>`
      : '';
    const accessTitle = proxyOk
      ? '步骤 2/2：通过代理运行 codex exec'
      : '步骤 2/2：未执行 codex exec';

    return [
      proxyLine,
      `<div style="margin-bottom:6px">${accessTitle}</div>`,
      execRows,
      waitingLine,
    ].join('');
  }

  const accessRows = results.map(r => {
    const shortUrl = String(r.test_url || '').replace('https://', '').replace('http://', '');
    if (r.target_access_ok) {
      return `<div style="margin-bottom:4px"><span style="color:#27ae60">✓</span> <span class="mono-text">${escapeHtml(shortUrl)}</span> ${r.status} ${escapeHtml(r.status_text || '')} <span style="color:var(--text-secondary)">${r.elapsed_ms}ms</span></div>`;
    }
    const message = r.proxy_connect_ok
      ? (r.error || '访问失败')
      : (r.error || r.proxy_connect_error || '未执行');
    return `<div style="margin-bottom:4px"><span style="color:#e74c3c">✗</span> <span class="mono-text">${escapeHtml(shortUrl)}</span> <span style="color:#e74c3c">${escapeHtml(message)}</span></div>`;
  }).join('');

  const waitingLine = results.length < expectedCount
    ? `<div style="margin-top:4px;color:var(--text-secondary)">步骤 2/2：正在访问测试地址… ${results.length}/${expectedCount}</div>`
    : '';
  const accessTitle = proxyOk
    ? '步骤 2/2：通过代理访问测试地址'
    : '步骤 2/2：未执行目标访问';

  return [
    proxyLine,
    `<div style="margin-bottom:6px">${accessTitle}</div>`,
    accessRows,
    waitingLine,
  ].join('');
}

function rememberProxyTest(results, sourceLabel) {
  const okCount = results.filter(item => item.ok).length;
  const failCount = results.length - okCount;
  const activeLabel = sourceLabel || '当前输入';
  const mode = results[0]?.test_mode === 'codex_exec' ? 'Codex Exec' : 'HTTP';
  const targetLabel = mode === 'Codex Exec' ? '个测试' : '个地址';
  const lead = failCount === 0
    ? `${activeLabel} ${mode} 测试通过，共 ${okCount} ${targetLabel}。`
    : `${activeLabel} ${mode} 测试完成：成功 ${okCount}，失败 ${failCount}。`;
  const detail = results
    .slice(0, 3)
    .map(item => {
      if (item.test_mode === 'codex_exec') {
        const prompt = String(item.command_prompt || DEFAULT_PROXY_CODEX_PROMPT).trim();
        return item.ok
          ? `codex exec "${prompt}" ${item.exit_code ?? 0}`.trim()
          : `codex exec "${prompt}" ${item.error || '失败'}`.trim();
      }
      const shortUrl = String(item.test_url || '').replace('https://', '').replace('http://', '');
      return item.ok
        ? `${shortUrl} ${item.status || ''}`.trim()
        : `${shortUrl} ${item.error || '失败'}`.trim();
    })
    .filter(Boolean)
    .join('；');
  state.lastProxyTestSummary = detail ? `${lead} ${detail}` : lead;
  state.lastProxyTestTime = Date.now();
  if (proxyLastTestSummaryEl) {
    proxyLastTestSummaryEl.textContent = state.lastProxyTestSummary;
  }
  if (proxyLastTestTimeEl) {
    proxyLastTestTimeEl.textContent = formatDateTimeLong(new Date(state.lastProxyTestTime));
  }
}

function renderAppProxyStatus(proxyData) {
  const active = proxyData && proxyData.active;
  state.activeProxy = active || null;
  const inheritedEnvCount = Array.isArray(proxyData?.inherited_proxy_env)
    ? proxyData.inherited_proxy_env.length
    : 0;
  const summary = active
    ? `${active.name} (${active.proxy_type}://${active.server})`
    : '未启用程序代理';
  const extra = [];
  const scopeText = active
    ? '当前已覆盖 webclx 后端 HTTP 请求；新建终端也会继承该代理。'
    : '当前不使用程序代理；webclx 后端请求将直接连接。';
  const hostEnvText = inheritedEnvCount > 0
    ? `检测到宿主环境代理变量 ${inheritedEnvCount} 项，但程序请求默认忽略它们。`
    : '未检测到宿主环境代理变量；程序代理仍保持独立。';
  if (proxyData?.ignores_system_proxy_env) {
    extra.push(scopeText);
    extra.push(hostEnvText);
  }
  if (inheritedEnvCount > 0) {
    extra.push(...proxyData.inherited_proxy_env);
  }

  proxyActiveSummaryEl.textContent = summary;
  proxyScopeSummaryEl.textContent = scopeText;
  proxyHostEnvSummaryEl.textContent = hostEnvText;
  proxyEffectiveEnvEl.textContent = formatEnvList(
    [
      ...(Array.isArray(proxyData?.effective_env) ? proxyData.effective_env : []),
      ...extra,
    ],
    '无'
  );
  systemAppProxyActiveEl.textContent = summary;
  systemAppProxyEnvEl.textContent = formatEnvList(
    Array.isArray(proxyData?.effective_env) ? proxyData.effective_env : [],
    '无'
  );
  proxyClearActiveBtnEl.disabled = !active;
  if (proxyLastTestSummaryEl) {
    proxyLastTestSummaryEl.textContent = state.lastProxyTestSummary || '尚未测试';
  }
  if (proxyLastTestTimeEl) {
    proxyLastTestTimeEl.textContent = state.lastProxyTestTime
      ? formatDateTimeLong(new Date(state.lastProxyTestTime))
      : '—';
  }
}

async function loadAppProxyStatus() {
  try {
    const proxyData = await requestJson("/api/proxy/active");
    renderAppProxyStatus(proxyData);
  } catch (error) {
    proxyActiveSummaryEl.textContent = '加载失败';
    proxyScopeSummaryEl.textContent = '加载失败';
    proxyHostEnvSummaryEl.textContent = error.message;
    proxyEffectiveEnvEl.textContent = error.message;
    systemAppProxyActiveEl.textContent = '加载失败';
    systemAppProxyEnvEl.textContent = error.message;
    proxyClearActiveBtnEl.disabled = true;
    if (proxyLastTestSummaryEl) {
      proxyLastTestSummaryEl.textContent = state.lastProxyTestSummary || '尚未测试';
    }
    if (proxyLastTestTimeEl) {
      proxyLastTestTimeEl.textContent = state.lastProxyTestTime
        ? formatDateTimeLong(new Date(state.lastProxyTestTime))
        : '—';
    }
  }
}

async function clearActiveAppProxy() {
  if (!confirm('确定要清除程序内代理吗？')) return;
  try {
    await requestJson("/api/proxy/active", { method: 'DELETE' });
    state.activeProxyId = null;
    renderProxyPresets();
    await loadAppProxyStatus();
    showProxyResult('程序代理已清除；本程序后续 HTTP 请求将不再使用代理。', 'ok');
  } catch (error) {
    showProxyResult('清除失败：' + error.message, 'warn');
  }
}

    return {
      applyProxyPreset,
      buildProxyTestPayloads,
      clearActiveAppProxy,
      clearProxyForm,
      deleteProxyPreset,
      editProxyPreset,
      executeProxyTestSequence,
      getSelectedProxyTestMode,
      loadAppProxyStatus,
      loadProxyPresets,
      renderAppProxyStatus,
      renderProxyPresets,
      renderProxyTestResults,
      saveProxyPreset,
      showProxyResult,
      showProxyTestResult,
      syncProxyTestModeUi,
      testProxyByPreset,
      testProxyFromForm,
    };
  }

  globalThis.WebClxProxyManager = Object.freeze({ create: createProxyManager });
})();
