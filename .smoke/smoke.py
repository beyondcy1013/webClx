#!/usr/bin/env python3
"""webClx 前端 smoke 检查：加载 index 与 terminal 页，收集 console error 与关键 DOM。
退出码 0 = 通过，1 = 发现回归。"""
import os
import sys
from playwright.sync_api import sync_playwright

BASE = os.environ.get("WEBCLX_BASE", "http://127.0.0.1:11111")
CHROME = "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome"
FAIL = []


def probe(browser, url, label, checks):
    page = browser.new_page(viewport={"width": 1280, "height": 900})
    errors = []
    page.on("console", lambda m: errors.append(m.text) if m.type == "error" else None)
    page.on("pageerror", lambda e: errors.append(str(e)))
    try:
        page.goto(url, wait_until="load", timeout=20000)
        page.wait_for_timeout(1500)
        for name, fn in checks.items():
            try:
                val = fn(page)
            except Exception as e:
                val = f"THROW:{e}"
            if val is False or (isinstance(val, str) and val.startswith("THROW")):
                FAIL.append(f"[{label}] {name} -> {val}")
        if errors:
            FAIL.append(f"[{label}] console errors: " + " || ".join(errors[:8]))
    except Exception as e:
        FAIL.append(f"[{label}] goto failed: {e}")
    finally:
        page.close()


with sync_playwright() as pw:
    browser = pw.chromium.launch(executable_path=CHROME, args=["--no-sandbox", "--disable-gpu"])

    probe(browser, f"{BASE}/", "index", {
        "hasBody": lambda p: p.query_selector("body") is not None,
        "tabsRendered": lambda p: len(p.query_selector_all('[class*="tab"], nav, #tab-')) > 0,
        "featureManagersWired": lambda p: p.evaluate(
            "() => typeof globalThis.WebClxFrpManager === 'object' && typeof globalThis.WebClxAuthManager === 'object'"
        ),
        "appJsParsed": lambda p: p.evaluate("() => typeof window.getInitialTab === 'function'"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal", {
        "hasTerminalHost": lambda p: p.query_selector("#terminal-host") is not None,
        "hasSessionSwitcher": lambda p: p.query_selector("#session-switcher") is not None,
        "xtermLoaded": lambda p: p.evaluate("() => typeof window.Termial !== 'undefined' || typeof window.Terminal !== 'undefined'"),
        "terminalSettingsLoaded": lambda p: p.evaluate("() => typeof globalThis.WebClxTerminalSettings === 'object'"),
        "terminalStatusEl": lambda p: p.query_selector("#terminal-status") is not None,
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-common", {
        "requestJsonGlobal": lambda p: p.evaluate("() => typeof requestJson === 'function'"),
        "copyTextToClipboardGlobal": lambda p: p.evaluate("() => typeof copyTextToClipboard === 'function'"),
        "normalizeTerminalPathGlobal": lambda p: p.evaluate("() => typeof normalizeTerminalPath === 'function'"),
        "sessionOptionLabelGlobal": lambda p: p.evaluate("() => typeof sessionOptionLabel === 'function'"),
        "terminalCommonModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-common.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-shell-settings", {
        "createTerminalInstanceGlobal": lambda p: p.evaluate("() => typeof createTerminalInstance === 'function'"),
        "replaceTerminalInstanceGlobal": lambda p: p.evaluate("() => typeof replaceTerminalInstance === 'function'"),
        "applyThemeModeGlobal": lambda p: p.evaluate("() => typeof applyThemeMode === 'function'"),
        "terminalShellSettingsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-shell-settings.js'))"),
    })

    probe(browser, f"{BASE}/", "ws-history", {
        "renderWorkspaceHistoryGlobal": lambda p: p.evaluate("() => typeof renderWorkspaceHistory === 'function'"),
        "recordWorkspaceHistoryGlobal": lambda p: p.evaluate("() => typeof recordWorkspaceHistory === 'function'"),
        "groupsCallable": lambda p: p.evaluate("() => { try { return Array.isArray(workspaceHistoryGroups()); } catch(e){ return 'ERR:'+e; } }"),
        "readCallable": lambda p: p.evaluate("() => { try { return Array.isArray(readWorkspaceHistory()); } catch(e){ return 'ERR:'+e; } }"),
        "extractedModuleLoaded": lambda p: p.evaluate("() => { const s = Array.from(document.scripts).map(x=>x.src); return s.some(x=>x.includes('app-workspace-history.js')); }"),
    })

    probe(browser, f"{BASE}/", "config-override", {
        "normalizeConfigOverrideValueGlobal": lambda p: p.evaluate("() => typeof normalizeConfigOverrideValue === 'function'"),
        "renderConfigOverrideEditorGlobal": lambda p: p.evaluate("() => typeof renderConfigOverrideEditor === 'function'"),
        "normalizeCallable": lambda p: p.evaluate("() => { try { return normalizeConfigOverrideValue('x')==='x'; } catch(e){ return 'ERR:'+e; } }"),
        "configOverrideModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-config-override.js'))"),
    })

    probe(browser, f"{BASE}/", "terminal-commands", {
        "parseTerminalRenamePresetsInputGlobal": lambda p: p.evaluate("() => typeof parseTerminalRenamePresetsInput === 'function'"),
        "renderTerminalShortcutSettingsGlobal": lambda p: p.evaluate("() => typeof renderTerminalShortcutSettings === 'function'"),
        "renderTerminalCommandCollectionsEditorGlobal": lambda p: p.evaluate("() => typeof renderTerminalCommandCollectionsEditor === 'function'"),
        "normalizeEnvVarsCallable": lambda p: p.evaluate("() => { try { return JSON.stringify(normalizeTerminalDefaultEnvVars([{key:'A',value:'1'}])) === JSON.stringify([{key:'A',value:'1'}]); } catch(e){ return 'ERR:'+e; } }"),
        "terminalCommandsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-terminal-commands.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "mobile-keys", {
        "triggerMobileKeyGlobal": lambda p: p.evaluate("() => typeof triggerMobileKey === 'function'"),
        "sendSlashCommandGlobal": lambda p: p.evaluate("() => typeof sendSlashCommand === 'function'"),
        "handleMobileKeyClickGlobal": lambda p: p.evaluate("() => typeof handleMobileKeyClick === 'function'"),
        "runTerminalFunctionCommandGlobal": lambda p: p.evaluate("() => typeof runTerminalFunctionCommand === 'function'"),
        "mobileKeysModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-mobile-keys.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-paste", {
        "pasteFromClipboardGlobal": lambda p: p.evaluate("() => typeof pasteFromClipboard === 'function'"),
        "handleTerminalPasteEventGlobal": lambda p: p.evaluate("() => typeof handleTerminalPasteEvent === 'function'"),
        "confirmTerminalPasteScheduleGlobal": lambda p: p.evaluate("() => typeof confirmTerminalPasteSchedule === 'function'"),
        "terminalPastePartsToTextGlobal": lambda p: p.evaluate("() => typeof terminalPastePartsToText === 'function'"),
        "terminalPasteModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-paste.js'))"),
    })

    probe(browser, f"{BASE}/", "app-version-check", {
        "normalizeVersionTextGlobal": lambda p: p.evaluate("() => typeof normalizeVersionText === 'function'"),
        "compareNumericVersionsGlobal": lambda p: p.evaluate("() => typeof compareNumericVersions === 'function'"),
        "describeRemoteVersionCheckGlobal": lambda p: p.evaluate("() => typeof describeRemoteVersionCheck === 'function'"),
        "appVersionCheckModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-version-check.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-dialogs", {
        "openTerminalPasteDialogGlobal": lambda p: p.evaluate("() => typeof openTerminalPasteDialog === 'function'"),
        "renderTerminalInputHistoryGlobal": lambda p: p.evaluate("() => typeof renderTerminalInputHistory === 'function'"),
        "openTerminalAgentsDocEditorGlobal": lambda p: p.evaluate("() => typeof openTerminalAgentsDocEditor === 'function'"),
        "renderTerminalPasteAssetsGlobal": lambda p: p.evaluate("() => typeof renderTerminalPasteAssets === 'function'"),
        "terminalDialogsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-dialogs.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-focus-selection", {
        "focusTerminalForUserInputGlobal": lambda p: p.evaluate("() => typeof focusTerminalForUserInput === 'function'"),
        "syncTerminalImePolicyGlobal": lambda p: p.evaluate("() => typeof syncTerminalImePolicy === 'function'"),
        "startTerminalTouchSelectionGlobal": lambda p: p.evaluate("() => typeof startTerminalTouchSelection === 'function'"),
        "filterTerminalAutoResponseGlobal": lambda p: p.evaluate("() => typeof filterTerminalAutoResponse === 'function'"),
        "readTerminalVisibleTextGlobal": lambda p: p.evaluate("() => typeof readTerminalVisibleText === 'function'"),
        "renameEditorKeepsFocusDuringAutoTerminalFocus": lambda p: p.evaluate("""() => {
            try {
                state.renamingSessionId = 'smoke-rename-session';
                sessionRenameInlineEl.hidden = false;
                sessionRenameInputEl.value = 'smoke-name';
                sessionRenameInputEl.focus();
                focusTerminalIfAllowed();
                return document.activeElement === sessionRenameInputEl;
            } finally {
                state.renamingSessionId = '';
                sessionRenameInlineEl.hidden = true;
            }
        }"""),
        "terminalFocusSelectionModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-focus-selection.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-resume-agent", {
        "injectLatestResumeCommandGlobal": lambda p: p.evaluate("() => typeof injectLatestResumeCommand === 'function'"),
        "extractLatestResumeInfoGlobal": lambda p: p.evaluate("() => typeof extractLatestResumeInfo === 'function'"),
        "copyCurrentAgentResumeIdGlobal": lambda p: p.evaluate("() => typeof copyCurrentAgentResumeId === 'function'"),
        "archiveCurrentAgentResumeGlobal": lambda p: p.evaluate("() => typeof archiveCurrentAgentResume === 'function'"),
        "handleTerminalFunctionShortcutGlobal": lambda p: p.evaluate("() => typeof handleTerminalFunctionShortcut === 'function'"),
        "terminalResumeAgentModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-resume-agent.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-output-scroll", {
        "queueTerminalOutputGlobal": lambda p: p.evaluate("() => typeof queueTerminalOutput === 'function'"),
        "drainTerminalOutputQueueGlobal": lambda p: p.evaluate("() => typeof drainTerminalOutputQueue === 'function'"),
        "preservePageScrollDuringLayoutGlobal": lambda p: p.evaluate("() => typeof preservePageScrollDuringLayout === 'function'"),
        "updatePageScrollRailGlobal": lambda p: p.evaluate("() => typeof updatePageScrollRail === 'function'"),
        "terminalOutputScrollModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-output-scroll.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-quota", {
        "renderQuotaReportGlobal": lambda p: p.evaluate("() => typeof renderQuotaReport === 'function'"),
        "refreshTerminalQuotaGlobal": lambda p: p.evaluate("() => typeof refreshTerminalQuota === 'function'"),
        "openTerminalQuotaDialogGlobal": lambda p: p.evaluate("() => typeof openTerminalQuotaDialog === 'function'"),
        "saveTerminalQuotaConfigGlobal": lambda p: p.evaluate("() => typeof saveTerminalQuotaConfig === 'function'"),
        "terminalQuotaModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-quota.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-command-quickstart", {
        "sendTerminalQuickCommandGlobal": lambda p: p.evaluate("() => typeof sendTerminalQuickCommand === 'function'"),
        "renderTerminalCommandCollectionsBodyGlobal": lambda p: p.evaluate("() => typeof renderTerminalCommandCollectionsBody === 'function'"),
        "runNewSessionQuickStartGlobal": lambda p: p.evaluate("() => typeof runNewSessionQuickStart === 'function'"),
        "maybeHandleNewSessionQuickStartInputGlobal": lambda p: p.evaluate("() => typeof maybeHandleNewSessionQuickStartInput === 'function'"),
        "terminalCommandQuickstartModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-command-quickstart.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-settings-loader", {
        "loadTerminalSettingsGlobal": lambda p: p.evaluate("() => typeof loadTerminalSettings === 'function'"),
        "terminalSettingsLoaderModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-settings-loader.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-input-transport", {
        "sendTerminalInputGlobal": lambda p: p.evaluate("() => typeof sendTerminalInput === 'function'"),
        "sendMessageGlobal": lambda p: p.evaluate("() => typeof sendMessage === 'function'"),
        "runPendingTerminalCommandGlobal": lambda p: p.evaluate("() => typeof runPendingTerminalCommand === 'function'"),
        "maybeRunTerminalStartupActionsGlobal": lambda p: p.evaluate("() => typeof maybeRunTerminalStartupActions === 'function'"),
        "terminalInputTransportModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-input-transport.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-cursor-correction", {
        "syncTerminalCursorCorrectionGlobal": lambda p: p.evaluate("() => typeof syncTerminalCursorCorrection === 'function'"),
        "filterTerminalMouseInputGlobal": lambda p: p.evaluate("() => typeof filterTerminalMouseInput === 'function'"),
        "setTerminalCursorHiddenForCorrectionGlobal": lambda p: p.evaluate("() => typeof setTerminalCursorHiddenForCorrection === 'function'"),
        "terminalCursorCorrectionModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-cursor-correction.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-auto-continue", {
        "syncAutoContinueHandledErrorsGlobal": lambda p: p.evaluate("() => typeof syncAutoContinueHandledErrors === 'function'"),
        "maybeAutoContinueErroredSessionsGlobal": lambda p: p.evaluate("() => typeof maybeAutoContinueErroredSessions === 'function'"),
        "setAutoContinueOnErrorGlobal": lambda p: p.evaluate("() => typeof setAutoContinueOnError === 'function'"),
        "scheduleAutoContinueAtResetTimeGlobal": lambda p: p.evaluate("() => typeof scheduleAutoContinueAtResetTime === 'function'"),
        "terminalAutoContinueModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-auto-continue.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-navigation-layout", {
        "syncTerminalHostHeightGlobal": lambda p: p.evaluate("() => typeof syncTerminalHostHeight === 'function'"),
        "buildTerminalUrlGlobal": lambda p: p.evaluate("() => typeof buildTerminalUrl === 'function'"),
        "terminalScrollMetricsGlobal": lambda p: p.evaluate("() => typeof terminalScrollMetrics === 'function'"),
        "refreshTerminalInputVisibilityAfterUserInputGlobal": lambda p: p.evaluate("() => typeof refreshTerminalInputVisibilityAfterUserInput === 'function'"),
        "terminalNavigationLayoutModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-navigation-layout.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-session-render", {
        "activeSessionGlobal": lambda p: p.evaluate("() => typeof activeSession === 'function'"),
        "renderSessionsGlobal": lambda p: p.evaluate("() => typeof renderSessions === 'function'"),
        "syncHistoryGlobal": lambda p: p.evaluate("() => typeof syncHistory === 'function'"),
        "startSessionRenameGlobal": lambda p: p.evaluate("() => typeof startSessionRename === 'function'"),
        "terminalSessionRenderModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-session-render.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "sessions", {
        "loadSessionsGlobal": lambda p: p.evaluate("() => typeof loadSessions === 'function'"),
        "createSessionGlobal": lambda p: p.evaluate("() => typeof createSession === 'function'"),
        "renameSessionGlobal": lambda p: p.evaluate("() => typeof renameSession === 'function'"),
        "idleCurrentSessionGlobal": lambda p: p.evaluate("() => typeof idleCurrentSession === 'function'"),
        "refreshTerminalViewportLayoutGlobal": lambda p: p.evaluate("() => typeof refreshTerminalViewportLayout === 'function'"),
        "sessionsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-sessions.js'))"),
    })

    probe(browser, f"{BASE}/assets/terminal.html", "terminal-layout-connection", {
        "syncTerminalSizeGlobal": lambda p: p.evaluate("() => typeof syncTerminalSize === 'function'"),
        "connectTerminalGlobal": lambda p: p.evaluate("() => typeof connectTerminal === 'function'"),
        "selectSessionGlobal": lambda p: p.evaluate("() => typeof selectSession === 'function'"),
        "handleServerControlMessageGlobal": lambda p: p.evaluate("() => typeof handleServerControlMessage === 'function'"),
        "terminalLayoutConnectionModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('terminal-layout-connection.js'))"),
    })

    probe(browser, f"{BASE}/", "settings-formatters", {
        "normalizeTerminalErrorMatchLineLimitGlobal": lambda p: p.evaluate("() => typeof normalizeTerminalErrorMatchLineLimit === 'function'"),
        "applyThemeModeGlobal": lambda p: p.evaluate("() => typeof applyThemeMode === 'function'"),
        "normalizeAvailableUsersGlobal": lambda p: p.evaluate("() => typeof normalizeAvailableUsers === 'function'"),
        "renderTerminalQuickCommandsGlobal": lambda p: p.evaluate("() => typeof renderTerminalQuickCommands === 'function'"),
        "softKeyboardScaleClamps": lambda p: p.evaluate("() => { try { return formatTerminalSoftKeyboardScale('5') === '1.3'; } catch(e){ return 'ERR:'+e; } }"),
        "themeApplied": lambda p: p.evaluate("() => { const t = document.body.getAttribute('data-theme') || document.documentElement.getAttribute('data-theme'); return t === 'light' || t === 'dark'; }"),
        "settingsFormattersModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-settings-formatters.js'))"),
    })

    probe(browser, f"{BASE}/", "app-datetime-format", {
        "formatDateTimeGlobal": lambda p: p.evaluate("() => typeof formatDateTime === 'function'"),
        "formatElapsedSinceGlobal": lambda p: p.evaluate("() => typeof formatElapsedSince === 'function'"),
        "formatQuotaWindowGlobal": lambda p: p.evaluate("() => typeof formatQuotaWindow === 'function'"),
        "firstFiniteNumberGlobal": lambda p: p.evaluate("() => typeof firstFiniteNumber === 'function'"),
        "datetimeFormatModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-datetime-format.js'))"),
    })

    probe(browser, f"{BASE}/", "app-preset-table", {
        "renderPresetTableGlobal": lambda p: p.evaluate("() => typeof renderPresetTable === 'function'"),
        "sortPresetTableRowsGlobal": lambda p: p.evaluate("() => typeof sortPresetTableRows === 'function'"),
        "createCurrentIndicatorCellGlobal": lambda p: p.evaluate("() => typeof createCurrentIndicatorCell === 'function'"),
        "presetTableModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-preset-table.js'))"),
    })

    probe(browser, f"{BASE}/", "app-terminal-archives", {
        "renderTerminalArchivesGlobal": lambda p: p.evaluate("() => typeof renderTerminalArchives === 'function'"),
        "loadTerminalArchivesGlobal": lambda p: p.evaluate("() => typeof loadTerminalArchives === 'function'"),
        "terminalArchivesModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-terminal-archives.js'))"),
    })

    probe(browser, f"{BASE}/", "app-path-utils", {
        "resolveAbsolutePathGlobal": lambda p: p.evaluate("() => typeof resolveAbsolutePath === 'function'"),
        "normalizeAbsolutePathGlobal": lambda p: p.evaluate("() => typeof normalizeAbsolutePath === 'function'"),
        "formatSizeGlobal": lambda p: p.evaluate("() => typeof formatSize === 'function'"),
        "pathUtilsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-path-utils.js'))"),
    })

    probe(browser, f"{BASE}/", "app-session-views", {
        "refreshSessionViewsGlobal": lambda p: p.evaluate("() => typeof refreshSessionViews === 'function'"),
        "scheduleSessionViewsRefreshGlobal": lambda p: p.evaluate("() => typeof scheduleSessionViewsRefresh === 'function'"),
        "sortAndRenderDirectorySessionsGlobal": lambda p: p.evaluate("() => typeof sortAndRenderDirectorySessions === 'function'"),
        "sessionViewsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-session-views.js'))"),
    })

    probe(browser, f"{BASE}/", "app-session-controls", {
        "renderSessionsSessionPickerGlobal": lambda p: p.evaluate("() => typeof renderSessionsSessionPicker === 'function'"),
        "renderDirectorySessionsGlobal": lambda p: p.evaluate("() => typeof renderDirectorySessions === 'function'"),
        "loadDirectorySessionsGlobal": lambda p: p.evaluate("() => typeof loadDirectorySessions === 'function'"),
        "startSessionRenameGlobal": lambda p: p.evaluate("() => typeof startSessionRename === 'function'"),
        "sessionControlsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-session-controls.js'))"),
    })

    probe(browser, f"{BASE}/", "app-home-session-render", {
        "renderSessionsHomeGlobal": lambda p: p.evaluate("() => typeof renderSessions === 'function'"),
        "rememberPreferredSessionGlobal": lambda p: p.evaluate("() => typeof rememberPreferredSession === 'function'"),
        "homeSessionRenderModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-home-session-render.js'))"),
    })

    probe(browser, f"{BASE}/", "app-preset-ops", {
        "formatCurrentApiStatusGlobal": lambda p: p.evaluate("() => typeof formatCurrentApiStatus === 'function'"),
        "loadApiPresetsGlobal": lambda p: p.evaluate("() => typeof loadApiPresets === 'function'"),
        "applyApiPresetGlobal": lambda p: p.evaluate("() => typeof applyApiPreset === 'function'"),
        "normalizeTestResultGlobal": lambda p: p.evaluate("() => typeof normalizeTestResult === 'function'"),
        "presetOpsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-preset-ops.js'))"),
    })

    probe(browser, f"{BASE}/", "app-frp-proxy-ops", {
        "loadProxyPresetsGlobal": lambda p: p.evaluate("() => typeof loadProxyPresets === 'function'"),
        "saveFrpRoleGlobal": lambda p: p.evaluate("() => typeof saveFrpRole === 'function'"),
        "runFrpsCommandGlobal": lambda p: p.evaluate("() => typeof runFrpsCommand === 'function'"),
        "frpProxyOpsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-frp-proxy-ops.js'))"),
    })

    probe(browser, f"{BASE}/", "app-feature-managers-init", {
        "initializeFeatureManagersGlobal": lambda p: p.evaluate("() => typeof initializeFeatureManagers === 'function'"),
        "featureManagersModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-feature-managers-init.js'))"),
    })

    probe(browser, f"{BASE}/", "app-auto-continue-tasks", {
        "loadAutoContinueTasksGlobal": lambda p: p.evaluate("() => typeof loadAutoContinueTasks === 'function'"),
        "loadPasteScheduledTasksGlobal": lambda p: p.evaluate("() => typeof loadPasteScheduledTasks === 'function'"),
        "cancelPasteScheduledTaskGlobal": lambda p: p.evaluate("() => typeof cancelPasteScheduledTask === 'function'"),
        "autoContinueTasksModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-auto-continue-tasks.js'))"),
    })

    probe(browser, f"{BASE}/", "app-workspace-browser", {
        "navigateToGlobal": lambda p: p.evaluate("() => typeof navigateTo === 'function'"),
        "renderEntriesGlobal": lambda p: p.evaluate("() => typeof renderEntries === 'function'"),
        "loadDirectoryGlobal": lambda p: p.evaluate("() => typeof loadDirectory === 'function'"),
        "workspaceBrowserModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-workspace-browser.js'))"),
    })

    probe(browser, f"{BASE}/", "app-session-actions", {
        "loadSessionsHomeGlobal": lambda p: p.evaluate("() => typeof loadSessions === 'function'"),
        "searchSessionsOutputGlobal": lambda p: p.evaluate("() => typeof searchSessionsOutput === 'function'"),
        "createSessionHomeGlobal": lambda p: p.evaluate("() => typeof createSession === 'function'"),
        "sessionActionsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-session-actions.js'))"),
    })

    probe(browser, f"{BASE}/", "app-navigation-tabs", {
        "setActiveTabGlobal": lambda p: p.evaluate("() => typeof setActiveTab === 'function'"),
        "setActiveSettingsTabGlobal": lambda p: p.evaluate("() => typeof setActiveSettingsTab === 'function'"),
        "navigationTabsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-navigation-tabs.js'))"),
    })

    probe(browser, f"{BASE}/", "app-core-event-bindings", {
        "bindCoreEventHandlersGlobal": lambda p: p.evaluate("() => typeof bindCoreEventHandlers === 'function'"),
        "coreEventBindingsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-core-event-bindings.js'))"),
    })

    probe(browser, f"{BASE}/", "app-frp-proxy-event-bindings", {
        "bindFrpProxyEventHandlersGlobal": lambda p: p.evaluate("() => typeof bindFrpProxyEventHandlers === 'function'"),
        "frpProxyEventBindingsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-frp-proxy-event-bindings.js'))"),
    })

    probe(browser, f"{BASE}/", "app-preset-form-event-bindings", {
        "bindPresetFormEventHandlersGlobal": lambda p: p.evaluate("() => typeof bindPresetFormEventHandlers === 'function'"),
        "presetFormEventBindingsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-preset-form-event-bindings.js'))"),
    })

    probe(browser, f"{BASE}/", "app-settings-event-bindings", {
        "bindSettingsEventHandlersGlobal": lambda p: p.evaluate("() => typeof bindSettingsEventHandlers === 'function'"),
        "settingsEventBindingsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-settings-event-bindings.js'))"),
    })

    probe(browser, f"{BASE}/", "app-settings-load-save", {
        "loadSettingsGlobal": lambda p: p.evaluate("() => typeof loadSettings === 'function'"),
        "saveSettingsGlobal": lambda p: p.evaluate("() => typeof saveSettings === 'function'"),
        "settingsLoadSaveModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-settings-load-save.js'))"),
    })

    probe(browser, f"{BASE}/", "app-desktop-update-events", {
        "bindDesktopFrameEventsGlobal": lambda p: p.evaluate("() => typeof bindDesktopFrameEvents === 'function'"),
        "bindUpdateEventHandlersGlobal": lambda p: p.evaluate("() => typeof bindUpdateEventHandlers === 'function'"),
        "desktopUpdateEventsModuleLoaded": lambda p: p.evaluate("() => Array.from(document.scripts).map(x=>x.src).some(x=>x.includes('app-desktop-update-events.js'))"),
    })

    browser.close()

if FAIL:
    print("SMOKE FAIL:\n" + "\n".join(FAIL), file=sys.stderr)
    sys.exit(1)
print("SMOKE OK: index + terminal 页面无 console error，关键 DOM 与 manager 已加载。")
