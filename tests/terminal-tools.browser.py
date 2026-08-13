import asyncio
import json
import os

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/third_party/browser-tools/bin/chromium",
)


async def main():
    console_errors = []
    page_errors = []
    results = {}

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = await browser.new_page(viewport={"width": 1440, "height": 1000})
        page.on(
            "console",
            lambda message: console_errors.append(message.text)
            if message.type == "error"
            else None,
        )
        page.on("pageerror", lambda error: page_errors.append(str(error)))

        await page.goto(f"{BASE_URL}/settings/tools", wait_until="domcontentloaded")
        await page.wait_for_selector(
            "#terminal-tool-entries-body tr", state="attached"
        )
        await page.click("#tab-settings")
        await page.click("#settings-tab-tools")
        await page.wait_for_function(
            "() => !document.querySelector('#settings-view').hidden"
            " && !document.querySelector('#settings-panel-tools').hidden"
        )
        fork_row = page.locator(
            '#terminal-tool-entries-body tr[data-terminal-tool-id="fork_session"]'
        )
        await fork_row.locator('[data-terminal-tool-edit="fork_session"]').click()
        await page.wait_for_selector("#terminal-tool-editor-dialog[open]")
        fork_parameter = page.locator(
            "#terminal-tool-actions-list .terminal-tool-action-parameter"
        )
        assert (await fork_parameter.text_content()).strip() == "自动提取 resume"
        assert await fork_parameter.evaluate("(element) => element.tagName") == "SPAN"
        await page.click("#terminal-tool-editor-cancel")
        await page.click("#terminal-tool-add-folder")
        await page.wait_for_selector("#terminal-tool-editor-dialog[open]")
        roots = await page.locator("#terminal-tool-editor-root option").all_text_contents()
        assert "利器" in roots
        await page.click("#terminal-tool-editor-cancel")
        results["settings_desktop"] = await page.evaluate(
            """() => ({
                rows: document.querySelectorAll(
                    '#terminal-tool-entries-body tr[data-terminal-tool-id]'
                ).length,
                page_overflow: document.documentElement.scrollWidth > innerWidth,
            })"""
        )
        assert not results["settings_desktop"]["page_overflow"]
        await page.screenshot(path="/tmp/webclx-tools-settings-1440.png", full_page=True)

        await page.set_viewport_size({"width": 375, "height": 812})
        results["settings_mobile"] = await page.evaluate(
            """() => ({
                page_overflow: document.documentElement.scrollWidth > innerWidth,
                table_overflow: document.querySelector('.terminal-tool-table-wrap').scrollWidth
                    > document.querySelector('.terminal-tool-table-wrap').clientWidth,
            })"""
        )
        assert not results["settings_mobile"]["page_overflow"]
        assert results["settings_mobile"]["table_overflow"]
        await page.screenshot(path="/tmp/webclx-tools-settings-375.png", full_page=True)

        await page.set_viewport_size({"width": 768, "height": 900})
        await page.goto(f"{BASE_URL}/terminal", wait_until="domcontentloaded")
        await page.wait_for_function(
            "() => typeof normalizeTerminalToolEntries === 'function'"
        )
        await page.wait_for_function(
            "() => state.terminalToolEntries.some("
            "(entry) => entry.label === 'sub2api_gpt-5.6_1M'"
            ") && state.terminalToolEntries.some("
            "(entry) => entry.label === '智谱5.2 ZCODE API 1M'"
            ") && state.terminalToolEntries.some("
            "(entry) => entry.label === 'fork'"
            ")"
        )
        await page.wait_for_function(
            "() => !state.loadingSessions"
            " && Boolean(state.activeSessionId)"
            " && state.sessions.some((session) => session.id === state.activeSessionId)"
        )
        results["fork_slash_command"] = await page.evaluate(
            """() => {
                const command = state.terminalSlashCommands.find(
                    (item) => item.key === 'fork'
                );
                const button = document.querySelector(
                    '#terminal-slash-command-menu button[data-key="fork"]'
                );
                return {
                    command: command
                        ? {
                            key: command.key,
                            label: command.label,
                            action: command.action,
                            command: command.command,
                        }
                        : null,
                    buttonLabel: button?.textContent?.trim() || '',
                };
            }"""
        )
        assert results["fork_slash_command"] == {
            "command": {
                "key": "fork",
                "label": "/fork",
                "action": "send_slash_command",
                "command": "/fork",
            },
            "buttonLabel": "/fork",
        }

        specified_task_requests = []

        async def handle_specified_task(route):
            request = route.request
            if request.method == "POST":
                payload = request.post_data_json
                specified_task_requests.append(payload)
                await route.fulfill(
                    json={
                        "id": "ct_browser_specified",
                        "mode": payload["mode"],
                        "status": "queued",
                        "preset": {
                            "id": payload["preset"]["id"],
                            "name": "Browser preset",
                            "model": "gpt-browser",
                        },
                        "cwd": payload["cwd"],
                        "timeout_secs": payload["timeout_secs"],
                        "created_at": 1,
                        "updated_at": 1,
                        "cancel_requested": False,
                        "terminal_closed": False,
                    }
                )
                return
            await route.fulfill(
                json={
                    "id": "ct_browser_specified",
                    "mode": "terminal",
                    "status": "succeeded",
                    "preset": {
                        "id": specified_task_requests[0]["preset"]["id"],
                        "name": "Browser preset",
                        "model": "gpt-browser",
                    },
                    "cwd": specified_task_requests[0]["cwd"],
                    "timeout_secs": 60,
                    "created_at": 1,
                    "updated_at": 2,
                    "finished_at": 2,
                    "cancel_requested": False,
                    "terminal_id": "s-browser-owned",
                    "terminal_name": "codex_task_browser",
                    "terminal_closed": True,
                    "actual_model": "gpt-browser",
                    "exit_code": 0,
                    "result": "浏览器模拟任务完成",
                    "transcript_tail": "",
                }
            )

        await page.route("**/api/codex/tasks", handle_specified_task)
        await page.route("**/api/codex/tasks/**", handle_specified_task)
        await page.select_option(
            "#terminal-project-command-select", "open_specified_task"
        )
        await page.wait_for_selector("#terminal-specified-task-dialog[open]")
        await page.wait_for_function(
            "() => Boolean(document.querySelector('#terminal-specified-task-preset').value)"
        )
        assert await page.locator(
            'input[name="terminal-specified-task-mode"][value="fixed"]'
        ).is_checked()
        assert await page.locator(
            "#terminal-specified-task-fixed-options"
        ).is_visible()
        assert not await page.locator(
            "#terminal-specified-task-timeout-field"
        ).is_visible()
        selected_preset = await page.locator(
            "#terminal-specified-task-preset"
        ).input_value()
        await page.locator(".terminal-specified-task-modes label", has_text="临时终端").click()
        assert await page.locator(
            'input[name="terminal-specified-task-mode"][value="terminal"]'
        ).is_checked()
        await page.fill("#terminal-specified-task-text", "浏览器模拟任务")
        await page.fill("#terminal-specified-task-timeout", "60")
        expected_task_path = await page.evaluate(
            "() => terminalSpecifiedTaskCurrentPath()"
        )
        await page.click("#terminal-specified-task-run")
        specified_status = ""
        for _ in range(20):
            specified_status = (
                await page.locator("#terminal-specified-task-status").text_content()
            ).strip()
            if "已完成" in specified_status:
                break
            await page.wait_for_timeout(250)
        assert "已完成" in specified_status, {
            "status": specified_status,
            "requests": specified_task_requests,
            "page_errors": page_errors,
            "console_errors": console_errors,
        }
        results["specified_task"] = await page.evaluate(
            """() => {
                const dialog = document.querySelector('#terminal-specified-task-dialog');
                const rect = dialog.getBoundingClientRect();
                return {
                    open: dialog.open,
                    status: document.querySelector('#terminal-specified-task-status')
                        .textContent.trim(),
                    result: document.querySelector('#terminal-specified-task-result')
                        .textContent.trim(),
                    in_viewport: rect.left >= 0 && rect.top >= 0
                        && rect.right <= innerWidth && rect.bottom <= innerHeight,
                };
            }"""
        )
        assert results["specified_task"]["open"]
        assert "实际模型 gpt-browser" in results["specified_task"]["status"]
        assert "临时终端已关闭" in results["specified_task"]["status"]
        assert results["specified_task"]["result"] == "浏览器模拟任务完成"
        assert results["specified_task"]["in_viewport"]
        assert specified_task_requests == [
            {
                "mode": "terminal",
                "preset": {"id": selected_preset},
                "cwd": expected_task_path,
                "task": "浏览器模拟任务",
                "timeout_secs": 60,
            }
        ], {
            "actual": specified_task_requests,
            "selected_preset": selected_preset,
            "expected_task_path": expected_task_path,
        }
        await page.screenshot(
            path="/tmp/webclx-specified-task-768.png", full_page=True
        )
        await page.set_viewport_size({"width": 375, "height": 812})
        results["specified_task_mobile"] = await page.evaluate(
            """() => {
                const dialog = document.querySelector('#terminal-specified-task-dialog');
                const form = document.querySelector('#terminal-specified-task-form');
                const rect = dialog.getBoundingClientRect();
                return {
                    in_viewport: rect.left >= 0 && rect.top >= 0
                        && rect.right <= innerWidth && rect.bottom <= innerHeight,
                    form_scrollable: form.scrollHeight >= form.clientHeight,
                    page_overflow: document.documentElement.scrollWidth > innerWidth,
                };
            }"""
        )
        assert results["specified_task_mobile"]["in_viewport"]
        assert not results["specified_task_mobile"]["page_overflow"]
        await page.screenshot(
            path="/tmp/webclx-specified-task-375.png", full_page=True
        )
        await page.click("#terminal-specified-task-close")
        await page.unroute("**/api/codex/tasks", handle_specified_task)
        await page.unroute("**/api/codex/tasks/**", handle_specified_task)
        await page.set_viewport_size({"width": 768, "height": 900})

        results["api_execution_runtime"] = await page.evaluate(
            """async () => {
                const originals = {
                    ensureTerminalSessionCache,
                    terminalContextSocketOpen,
                    terminalInitialReplaySettled,
                    sendTerminalAutoTypedInput,
                    sendTerminalInput,
                };
                const fakeContext = { sessionId: 'tool-session' };
                const apiCalls = [];
                const websocketInputs = [];
                let replaySettled = false;
                try {
                    ensureTerminalSessionCache = () => ({
                        get: (sessionId) => sessionId === 'tool-session'
                            ? fakeContext
                            : null,
                    });
                    terminalContextSocketOpen = (context) => context === fakeContext;
                    terminalInitialReplaySettled = (context) =>
                        context === fakeContext && replaySettled;
                    sendTerminalAutoTypedInput = async (command, options) => {
                        apiCalls.push({ command, options });
                        return true;
                    };
                    sendTerminalInput = (data) => websocketInputs.push(data);

                    let ready = false;
                    const pendingReady = waitForTerminalToolSessionReady(
                        'tool-session',
                        1000,
                    ).then(() => {
                        ready = true;
                    });
                    await new Promise((resolve) => setTimeout(resolve, 30));
                    const waitedForReplay = !ready;
                    replaySettled = true;
                    await pendingReady;

                    await executeTerminalToolAction(
                        {
                            kind: 'send_command',
                            value: 'webclx run api preset-browser-check -- codex',
                        },
                        { sessionId: 'tool-session' },
                    );
                    return { waitedForReplay, apiCalls, websocketInputs };
                } finally {
                    ensureTerminalSessionCache = originals.ensureTerminalSessionCache;
                    terminalContextSocketOpen = originals.terminalContextSocketOpen;
                    terminalInitialReplaySettled = originals.terminalInitialReplaySettled;
                    sendTerminalAutoTypedInput = originals.sendTerminalAutoTypedInput;
                    sendTerminalInput = originals.sendTerminalInput;
                }
            }"""
        )
        assert results["api_execution_runtime"] == {
            "waitedForReplay": True,
            "apiCalls": [
                {
                    "command": "webclx run api preset-browser-check -- codex",
                    "options": {
                        "sessionId": "tool-session",
                        "throwOnError": True,
                    },
                }
            ],
            "websocketInputs": [],
        }
        results["codex_preset_runtime"] = await page.evaluate(
            """async () => {
                const originals = {
                    executeSpecifiedPreset,
                    showTerminalCodexTaskResult,
                };
                const calls = [];
                const shown = [];
                try {
                    executeSpecifiedPreset = async (options) => {
                        calls.push({
                            action: options.action,
                            mode: options.mode,
                            presetId: options.presetId,
                            cwd: options.cwd,
                            task: options.task,
                        });
                        return {
                            id: 'ct-browser-tool',
                            mode: options.mode,
                            status: 'succeeded',
                            preset: { id: options.presetId, name: 'Browser tool preset' },
                            result: '利器指定任务完成',
                            terminal_closed: true,
                        };
                    };
                    showTerminalCodexTaskResult = (record, options) => {
                        shown.push({ result: record.result, source: options.source });
                    };
                    const execution = {
                        sourcePath: 'webClx',
                        presetId: '',
                        deferPresetApply: true,
                    };
                    await executeTerminalToolAction(
                        { kind: 'switch_api_preset', value: 'preset-browser-tool' },
                        execution,
                    );
                    await executeTerminalToolAction(
                        { kind: 'codex_terminal', value: '检查项目并汇报' },
                        execution,
                    );
                    return { selectedPresetId: execution.presetId, calls, shown };
                } finally {
                    executeSpecifiedPreset = originals.executeSpecifiedPreset;
                    showTerminalCodexTaskResult = originals.showTerminalCodexTaskResult;
                }
            }"""
        )
        assert results["codex_preset_runtime"] == {
            "selectedPresetId": "preset-browser-tool",
            "calls": [
                {
                    "action": "task",
                    "mode": "terminal",
                    "presetId": "preset-browser-tool",
                    "cwd": "webClx",
                    "task": "检查项目并汇报",
                }
            ],
            "shown": [
                {"result": "利器指定任务完成", "source": "tool"}
            ],
        }
        results["fork_execution_runtime"] = await page.evaluate(
            """async () => {
                const originals = {
                    ensureTerminalSessionCache,
                    terminalContextSocketOpen,
                    terminalInitialReplaySettled,
                    readTerminalBufferTailTextFrom,
                    extractLatestResumeInfo,
                    sendTerminalAutoTypedInput,
                    createSession,
                    requestJson,
                    isIdleSession,
                    announceSessionMutation,
                    sortSessionsByRecentActivity,
                    renderSessions,
                    activeSessionId: state.activeSessionId,
                    sessions: state.sessions,
                };
                const sourceContext = {
                    sessionId: 'fork-source',
                    term: {},
                };
                const newContext = {
                    sessionId: 'fork-new',
                    term: {},
                };
                const contexts = new Map([
                    [sourceContext.sessionId, sourceContext],
                    [newContext.sessionId, newContext],
                ]);
                const apiCalls = [];
                const createCalls = [];
                const renameCalls = [];
                try {
                    state.activeSessionId = 'fork-source';
                    state.sessions = [
                        { id: 'fork-source', name: 'browser_source', path: 'webClx' },
                    ];
                    ensureTerminalSessionCache = () => ({
                        get: (sessionId) => contexts.get(sessionId) || null,
                    });
                    terminalContextSocketOpen = (context) => contexts.has(context?.sessionId);
                    terminalInitialReplaySettled = (context) => contexts.has(context?.sessionId);
                    readTerminalBufferTailTextFrom = () =>
                        'Session: 019d1ba6-f772-7452-a391-6553ccbc0a50';
                    extractLatestResumeInfo = () => ({
                        id: '019d1ba6-f772-7452-a391-6553ccbc0a50',
                        program: 'codex',
                    });
                    sendTerminalAutoTypedInput = async (command, options) => {
                        apiCalls.push({ command, options });
                        return true;
                    };
                    createSession = async (options) => {
                        createCalls.push(options);
                        const created = {
                            id: 'fork-new', name: 'browser_new', path: options.path,
                        };
                        state.activeSessionId = created.id;
                        state.sessions = state.sessions.concat(created);
                        return created;
                    };
                    requestJson = async (url, options) => {
                        const body = JSON.parse(options.body);
                        renameCalls.push({ url, method: options.method, body });
                        return {
                            id: 'fork-new', name: body.name, path: body.path,
                        };
                    };
                    isIdleSession = () => false;
                    announceSessionMutation = () => {};
                    sortSessionsByRecentActivity = (sessions) => sessions;
                    renderSessions = () => {};

                    const execution = {
                        sessionId: 'fork-source',
                        sourceSessionId: 'fork-source',
                        sourceSessionName: 'browser_source',
                        sourcePath: 'webClx',
                    };
                    await executeTerminalToolAction(
                        { kind: 'fork_session' },
                        execution,
                    );
                    return {
                        finalSessionId: execution.sessionId,
                        apiCalls,
                        createPath: createCalls[0]?.path || '',
                        renameCalls,
                    };
                } finally {
                    ensureTerminalSessionCache = originals.ensureTerminalSessionCache;
                    terminalContextSocketOpen = originals.terminalContextSocketOpen;
                    terminalInitialReplaySettled = originals.terminalInitialReplaySettled;
                    readTerminalBufferTailTextFrom = originals.readTerminalBufferTailTextFrom;
                    extractLatestResumeInfo = originals.extractLatestResumeInfo;
                    sendTerminalAutoTypedInput = originals.sendTerminalAutoTypedInput;
                    createSession = originals.createSession;
                    requestJson = originals.requestJson;
                    isIdleSession = originals.isIdleSession;
                    announceSessionMutation = originals.announceSessionMutation;
                    sortSessionsByRecentActivity = originals.sortSessionsByRecentActivity;
                    renderSessions = originals.renderSessions;
                    state.activeSessionId = originals.activeSessionId;
                    state.sessions = originals.sessions;
                }
            }"""
        )
        assert results["fork_execution_runtime"] == {
            "finalSessionId": "fork-new",
            "apiCalls": [
                {
                    "command": (
                        "codex fork 019d1ba6-f772-7452-a391-6553ccbc0a50"
                    ),
                    "options": {
                        "sessionId": "fork-new",
                        "throwOnError": True,
                    },
                },
            ],
            "createPath": "webClx",
            "renameCalls": [
                {
                    "url": "/api/terminal/sessions/fork-new",
                    "method": "PUT",
                    "body": {
                        "path": "webClx",
                        "name": "browser_source_fork",
                    },
                }
            ],
        }
        results["menu_viewports"] = {}
        for width, height in ((375, 812), (768, 900), (1440, 1000)):
            await page.set_viewport_size({"width": width, "height": height})
            await page.evaluate("() => toggleTerminalToolsMenu()")
            viewport_result = await page.evaluate(
                """() => {
                    const menu = document.querySelector('#terminal-tools-menu');
                    const weaponSection = document.querySelector('#terminal-tool-menu');
                    const rect = menu.getBoundingClientRect();
                    return {
                        labels: [...weaponSection.querySelectorAll(
                            '[data-terminal-tool-entry] .terminal-tool-menu-item-label'
                        )].map((element) => element.textContent.trim()),
                        visible: !menu.hidden && !weaponSection.hidden,
                        expanded: document.querySelector('#terminal-tools-button')
                            .getAttribute('aria-expanded'),
                        position: getComputedStyle(menu).position,
                        in_viewport: rect.left >= 0 && rect.top >= 0
                            && rect.right <= innerWidth && rect.bottom <= innerHeight,
                        width: Math.round(rect.width),
                        has_dialog: Boolean(document.querySelector('#terminal-tool-dialog')),
                    };
                }"""
            )
            assert viewport_result["labels"] == [
                "sub2api_gpt-5.6_1M",
                "智谱5.2 ZCODE API 1M",
                "fork",
            ]
            assert viewport_result["visible"]
            assert viewport_result["expanded"] == "true"
            assert viewport_result["position"] == "fixed"
            assert viewport_result["in_viewport"]
            assert viewport_result["width"] <= width - 16
            assert not viewport_result["has_dialog"]
            results["menu_viewports"][str(width)] = viewport_result
            await page.screenshot(
                path=f"/tmp/webclx-tools-terminal-{width}.png", full_page=True
            )
            await page.evaluate("() => closeTerminalToolsMenu()")

        await page.set_viewport_size({"width": 768, "height": 900})
        await page.evaluate(
            """() => {
                state.terminalToolEntries = normalizeTerminalToolEntries([
                    {
                        id: 'folder_a', root_key: 'tools', parent_id: null,
                        kind: 'folder', label: '常用', sort_order: 10, actions: [],
                    },
                    {
                        id: 'folder_b', root_key: 'tools', parent_id: 'folder_a',
                        kind: 'folder', label: '会话', sort_order: 10, actions: [],
                    },
                    {
                        id: 'wait_action', root_key: 'tools', parent_id: 'folder_b',
                        kind: 'action', label: '短暂等待', sort_order: 10,
                        actions: [{ kind: 'wait', value: '', seconds: 0.2 }],
                    },
                ]);
                renderTerminalToolRootButtons();
                toggleTerminalToolsMenu();
            }"""
        )
        await page.click('[data-terminal-tool-entry="folder_a"]')
        await page.click('[data-terminal-tool-entry="folder_b"]')
        results["hierarchy"] = await page.evaluate(
            """() => ({
                title: document.querySelector('#terminal-tool-menu-title').textContent.trim(),
                action_visible: Boolean(
                    document.querySelector('[data-terminal-tool-entry="wait_action"]')
                ),
                menu_visible: !document.querySelector('#terminal-tools-menu').hidden
                    && !document.querySelector('#terminal-tool-menu').hidden,
                expanded: document.querySelector('#terminal-tools-button')
                    .getAttribute('aria-expanded'),
                in_viewport: (() => {
                    const rect = document.querySelector('#terminal-tools-menu')
                        .getBoundingClientRect();
                    return rect.left >= 0 && rect.top >= 0
                        && rect.right <= innerWidth && rect.bottom <= innerHeight;
                })(),
            })"""
        )
        assert results["hierarchy"] == {
            "title": "常用 / 会话",
            "action_visible": True,
            "menu_visible": True,
            "expanded": "true",
            "in_viewport": True,
        }
        await page.click("#terminal-tool-menu-back")
        assert (
            await page.locator("#terminal-tool-menu-title").text_content()
        ).strip() == "常用"
        await page.click('[data-terminal-tool-entry="folder_b"]')
        await page.screenshot(path="/tmp/webclx-tools-terminal-768.png", full_page=True)
        await page.click('[data-terminal-tool-entry="wait_action"]')
        results["closed_immediately"] = await page.evaluate(
            "() => document.querySelector('#terminal-tools-menu').hidden"
            " && document.querySelector('#terminal-tools-button')"
            ".getAttribute('aria-expanded') === 'false'"
        )
        assert results["closed_immediately"]
        await page.wait_for_timeout(300)
        results["completion_status"] = (
            await page.locator("#terminal-status").text_content()
        ).strip()
        assert "短暂等待" in results["completion_status"]
        assert "执行完成" in results["completion_status"]

        await page.evaluate("() => toggleTerminalToolsMenu()")
        await page.mouse.click(2, 2)
        assert await page.locator("#terminal-tools-menu").is_hidden()

        await page.evaluate("() => toggleTerminalToolsMenu()")
        await page.keyboard.press("Escape")
        results["escape"] = await page.evaluate(
            """() => ({
                hidden: document.querySelector('#terminal-tools-menu').hidden,
                expanded: document.querySelector('#terminal-tools-button')
                    .getAttribute('aria-expanded'),
                focus: document.activeElement?.id || '',
            })"""
        )
        assert results["escape"] == {
            "hidden": True,
            "expanded": "false",
            "focus": "terminal-tools-button",
        }
        results["keyboard"] = await page.evaluate(
            """() => {
                const rows = [...document.querySelectorAll(
                    '#terminal-mobile-keys .terminal-mobile-row'
                )];
                const tools = document.querySelector('#terminal-tools-button');
                const projectSelect = document.querySelector('#terminal-project-command-select');
                const specified = projectSelect?.querySelector(
                    'option[value="open_specified_task"]'
                );
                return {
                    rows: rows.length,
                    specified_in_project_commands: Boolean(specified),
                    tools_in_first_row: rows[0]?.contains(tools),
                    standalone_weapon_button: Boolean(
                        document.querySelector('#terminal-tool-root-tools')
                    ),
                    labels: [tools.textContent.trim(), specified.textContent.trim()],
                };
            }"""
        )
        assert results["keyboard"] == {
            "rows": 2,
            "specified_in_project_commands": True,
            "tools_in_first_row": True,
            "standalone_weapon_button": False,
            "labels": ["终端工具", "指定"],
        }

        await browser.close()

    assert not page_errors, page_errors
    results["console_errors"] = console_errors
    results["page_errors"] = page_errors
    print(json.dumps(results, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    asyncio.run(main())
