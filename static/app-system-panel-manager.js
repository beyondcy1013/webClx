(function () {
  function createSystemPanelManager(deps) {
    const {
      state,
      requestJson,
      updateStatus,
      setInlineStatus,
      setButtonBusy,
      formatEnvList,
      getUpdateDownloadUrl,
      parseEnvEntriesToMap,
      elements,
    } = deps;
    const {
      systemRestartBtnEl,
      systemSystemProxyFilePathEl,
      systemProcessProxySummaryEl,
      systemFileProxySummaryEl,
      systemUserShellProxySummaryEl,
      systemUserShellProxyEnvEl,
      systemProxyHttpInputEl,
      systemProxyHttpsInputEl,
      systemProxyAllInputEl,
      systemProxyNoInputEl,
      systemSaveProxyBtnEl,
      systemClearProxyBtnEl,
      systemProxyStatusEl,
      systemCopyFromAppProxyBtnEl,
      updateCurrentVersionEl,
      updateLocalDownloadUrlEl,
      updateCopyUrlStatusEl,
    } = elements;

    let systemRestartRecoveryTimer = null;

    function clearSystemRestartRecoveryTimer() {
      if (systemRestartRecoveryTimer) {
        window.clearTimeout(systemRestartRecoveryTimer);
        systemRestartRecoveryTimer = null;
      }
    }

    async function pollSystemServiceRecovery(remainingAttempts = 20) {
      try {
        const response = await fetch("/api/system/info", {
          cache: "no-store",
          headers: { Accept: "application/json" },
        });
        if (!response.ok) {
          throw new Error(`请求失败: ${response.status}`);
        }
        clearSystemRestartRecoveryTimer();
        showToast("服务已恢复，正在刷新页面…", "ok");
        window.setTimeout(() => {
          window.location.reload();
        }, 600);
        return;
      } catch {
        if (remainingAttempts <= 0) {
          clearSystemRestartRecoveryTimer();
          showToast("服务重启请求已发出，但 30 秒内还没有恢复响应；请手动刷新页面确认。", "warn", 8000);
          setButtonBusy(systemRestartBtnEl, false);
          return;
        }
      }

      systemRestartRecoveryTimer = window.setTimeout(() => {
        pollSystemServiceRecovery(remainingAttempts - 1);
      }, 1500);
    }

    function waitForSystemServiceRecovery() {
      clearSystemRestartRecoveryTimer();
      systemRestartRecoveryTimer = window.setTimeout(() => {
        pollSystemServiceRecovery();
      }, 1200);
    }

    async function restartSystemService() {
      if (!systemRestartBtnEl) {
        return;
      }
      if (!window.confirm("确定要重启当前 webclx.service 吗？页面会短暂断开，然后自动刷新。")) {
        return;
      }

      clearSystemRestartRecoveryTimer();
      setButtonBusy(systemRestartBtnEl, true, "重启中…");
      showToast("正在提交重启请求…", "info");

      try {
        const result = await requestJson("/api/system/restart", { method: "POST" });
        showToast(result?.message || "重启请求已提交，正在等待服务恢复…", "info");
        waitForSystemServiceRecovery();
      } catch (error) {
        if (error instanceof TypeError || error instanceof SyntaxError) {
          showToast("连接已中断，通常表示服务正在重启；正在等待恢复…", "info");
          waitForSystemServiceRecovery();
          return;
        }

        showToast("重启失败: " + error.message, "warn", 6000);
        setButtonBusy(systemRestartBtnEl, false);
      }
    }

    async function loadSystemProxyStatus() {
      try {
        const data = await requestJson("/api/system/proxy");
        const serviceEnv = Array.isArray(data.service_env_file)
          ? data.service_env_file
          : (Array.isArray(data.environment_file) ? data.environment_file : []);
        const serviceEnvPath = data.service_env_file_path || data.environment_file_path || '/etc/default/webclx';
        const userShellEnv = Array.isArray(data.user_shell_env_file) ? data.user_shell_env_file : [];
        const userShellPath = data.user_shell_env_file_path || '~/.bashrc';
        const userShellError = typeof data.user_shell_read_error === 'string'
          ? data.user_shell_read_error.trim()
          : '';
        const envMap = parseEnvEntriesToMap(serviceEnv);
        const processEnvCount = Array.isArray(data.process_env) ? data.process_env.length : 0;
        const fileEnvCount = serviceEnv.length;
        systemSystemProxyFilePathEl.textContent = `${serviceEnvPath} ${data.can_write ? '' : '(只读)'}`.trim();
        systemProcessProxySummaryEl.textContent = processEnvCount > 0
          ? `运行中的 webclx 进程检测到 ${processEnvCount} 个代理变量${data.restart_required ? '；与启动配置不一致，重启后才会切换。' : '。'}`
          : `运行中的 webclx 进程没有检测到代理变量${data.restart_required ? '；但启动配置里已有代理，重启后会生效。' : '。'}`;
        systemFileProxySummaryEl.textContent = data.can_write
          ? `${serviceEnvPath} 可写，当前记录 ${fileEnvCount} 项代理变量；修改后需重启 webclx.service。`
          : `${serviceEnvPath} 当前不可写，只能查看。`;
        systemUserShellProxySummaryEl.textContent = userShellError
          ? `${userShellPath} 启动失败：${userShellError}`
          : userShellEnv.length > 0
            ? `通过当前服务用户的 shell 启动过程（入口 ${userShellPath}）检测到 ${userShellEnv.length} 项代理变量。`
            : `通过当前服务用户的 shell 启动过程（入口 ${userShellPath}）未检测到代理变量。`;
        systemUserShellProxyEnvEl.textContent = userShellError
          ? userShellError
          : formatEnvList(userShellEnv, '无');
        systemProxyHttpInputEl.value = envMap.get('HTTP_PROXY') || envMap.get('http_proxy') || '';
        systemProxyHttpsInputEl.value = envMap.get('HTTPS_PROXY') || envMap.get('https_proxy') || '';
        systemProxyAllInputEl.value = envMap.get('ALL_PROXY') || envMap.get('all_proxy') || '';
        systemProxyNoInputEl.value = envMap.get('NO_PROXY') || envMap.get('no_proxy') || '';
        systemSaveProxyBtnEl.disabled = !data.can_write;
        systemClearProxyBtnEl.disabled = !data.can_write;
        if (
          systemProxyStatusEl &&
          (!systemProxyStatusEl.textContent || systemProxyStatusEl.dataset.tone === 'muted')
          ) {
          setInlineStatus(systemProxyStatusEl, data.note || '', 'muted');
        }
      } catch (error) {
        systemSystemProxyFilePathEl.textContent = '—';
        systemProcessProxySummaryEl.textContent = '加载失败';
        systemFileProxySummaryEl.textContent = error.message;
        systemUserShellProxySummaryEl.textContent = '加载失败';
        systemUserShellProxyEnvEl.textContent = error.message;
        systemSaveProxyBtnEl.disabled = true;
        systemClearProxyBtnEl.disabled = true;
      }
    }

    async function loadUpdatePanel() {
      if (!updateCurrentVersionEl && !updateLocalDownloadUrlEl) {
        return;
      }
      try {
        const response = await requestJson("/api/update/check");
        const currentVersion = (
          typeof response.current_version === "string" && response.current_version.trim()
            ? response.current_version.trim()
            : typeof response.version === "string" && response.version.trim()
              ? response.version.trim()
              : state.serverVersion
        ) || "未知";
        const downloadUrl = (
          typeof response.binary_url === "string" && response.binary_url.trim()
            ? response.binary_url.trim()
            : typeof response.url === "string" && response.url.trim()
              ? response.url.trim()
              : getUpdateDownloadUrl()
        );

        state.serverVersion = currentVersion;
        state.updateDownloadUrl = downloadUrl;

        if (updateCurrentVersionEl) {
          updateCurrentVersionEl.textContent = currentVersion;
        }
        if (updateLocalDownloadUrlEl) {
          updateLocalDownloadUrlEl.textContent = downloadUrl;
        }
      } catch (error) {
        if (updateCurrentVersionEl) {
          updateCurrentVersionEl.textContent = state.serverVersion || "未知";
        }
        if (updateLocalDownloadUrlEl) {
          updateLocalDownloadUrlEl.textContent = getUpdateDownloadUrl();
        }
        updateStatus(updateCopyUrlStatusEl, error.message || "读取版本信息失败。", "warn");
      }
    }

    async function saveSystemProxy() {
      setInlineStatus(systemProxyStatusEl, '正在写入…', 'muted');
      try {
        await requestJson("/api/system/proxy", {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            http_proxy: systemProxyHttpInputEl.value.trim(),
            https_proxy: systemProxyHttpsInputEl.value.trim(),
            all_proxy: systemProxyAllInputEl.value.trim(),
            no_proxy: systemProxyNoInputEl.value.trim(),
          }),
        });
        await loadSystemProxyStatus();
        setInlineStatus(systemProxyStatusEl, '服务代理已写入 /etc/default/webclx；重启 webclx.service 后生效。', 'ok');
      } catch (error) {
        setInlineStatus(systemProxyStatusEl, '写入失败: ' + error.message, 'warn');
      }
    }

    async function clearProxyFromSystem() {
      if (!confirm('确定要清除 webclx.service 启动代理吗？')) return;
      setInlineStatus(systemProxyStatusEl, '正在清除…', 'muted');
      try {
        await requestJson("/api/system/proxy", { method: 'DELETE' });
        await loadSystemProxyStatus();
        setInlineStatus(systemProxyStatusEl, '服务代理已从 /etc/default/webclx 清除；重启 webclx.service 后生效。', 'ok');
      } catch (error) {
        setInlineStatus(systemProxyStatusEl, '清除失败: ' + error.message, 'warn');
      }
    }

    function copySystemProxyFromAppProxy() {
      const active = state.activeProxy;
      if (!active) {
        setInlineStatus(systemProxyStatusEl, '当前没有启用程序代理可复制。', 'warn');
        return;
      }
      if (active.has_password) {
        setInlineStatus(
          systemProxyStatusEl,
          '当前程序代理包含认证密码，不能安全复制到服务代理表单；请在服务环境中单独配置。',
          'warn',
        );
        return;
      }

      const proxyUrl = `${active.proxy_type}://${active.server}`;
      if (active.proxy_type === 'socks5') {
        systemProxyHttpInputEl.value = '';
        systemProxyHttpsInputEl.value = '';
        systemProxyAllInputEl.value = proxyUrl;
      } else {
        systemProxyHttpInputEl.value = proxyUrl;
        systemProxyHttpsInputEl.value = proxyUrl;
        systemProxyAllInputEl.value = proxyUrl;
      }
      if (!systemProxyNoInputEl.value.trim()) {
        systemProxyNoInputEl.value = '127.0.0.1,localhost,::1';
      }
      setInlineStatus(systemProxyStatusEl, '已从程序代理复制到服务启动配置表单，写入后重启 webclx.service 生效。', 'muted');
    }

    if (systemRestartBtnEl) {
      systemRestartBtnEl.addEventListener("click", restartSystemService);
    }
    systemSaveProxyBtnEl.addEventListener("click", saveSystemProxy);
    systemClearProxyBtnEl.addEventListener("click", clearProxyFromSystem);
    if (systemCopyFromAppProxyBtnEl) {
      systemCopyFromAppProxyBtnEl.addEventListener("click", copySystemProxyFromAppProxy);
    }

    return {
      loadSystemProxyStatus,
      loadUpdatePanel,
    };
  }

  globalThis.WebClxSystemPanelManager = Object.freeze({ create: createSystemPanelManager });
})();
