import asyncio
import json
import os
import re

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/third_party/browser-tools/bin/chromium",
)
ORIGINAL_SESSION_ID = "019d1ba6-f772-7452-a391-6553ccbc0a50"


def name_without_auto_indices(name):
    cleaned = re.sub(r"[_#]\d+(?=$|[\s_])", "", name.strip())
    cleaned = re.sub(r"_{2,}", "_", cleaned)
    cleaned = re.sub(r"\s{2,}", " ", cleaned)
    return cleaned.strip("_ ")


async def dialog_geometry(page):
    return await page.evaluate(
        """() => {
            const dialog = document.querySelector('#terminal-specified-task-dialog');
            const form = document.querySelector('#terminal-specified-task-form');
            const rect = dialog.getBoundingClientRect();
            return {
                open: dialog.open,
                inViewport: rect.left >= 0 && rect.top >= 0
                    && rect.right <= innerWidth && rect.bottom <= innerHeight,
                pageOverflow: document.documentElement.scrollWidth > innerWidth,
                formScrollable: form.scrollHeight >= form.clientHeight,
            };
        }"""
    )


async def main():
    console_errors = []
    page_errors = []
    failed_responses = []
    results = {}

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = await browser.new_page(viewport={"width": 768, "height": 900})
        page.on(
            "console",
            lambda message: console_errors.append(message.text)
            if message.type == "error"
            else None,
        )
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        page.on(
            "response",
            lambda response: failed_responses.append(
                {"status": response.status, "url": response.url}
            )
            if response.status >= 400
            else None,
        )

        await page.goto(f"{BASE_URL}/terminal", wait_until="domcontentloaded")
        await page.wait_for_function(
            "() => typeof openTerminalDesignatePresetForkDialog === 'function'"
            " && !state.loadingSessions"
            " && Boolean(state.activeSessionId)"
        )

        options = await page.locator(
            "#terminal-project-command-select option"
        ).all_text_contents()
        assert "指定预设+fork（持久切换）" in options
        assert "指定预设+resume（持久切换）" in options
        assert "指定预设终端" in options

        source_terminal_name = await page.evaluate(
            "() => state.sessions.find((session) => session.id === state.activeSessionId)?.name || ''"
        )
        await page.select_option(
            "#terminal-project-command-select", "designate_preset_terminal"
        )
        await page.wait_for_selector("#terminal-specified-task-dialog[open]")
        await page.wait_for_function(
            "() => Boolean(document.querySelector('#terminal-specified-task-preset').value)"
        )
        results["terminal"] = {
            "session": await page.locator(
                "#terminal-specified-task-session-id"
            ).input_value(),
            "terminalName": await page.locator(
                "#terminal-specified-task-terminal-name"
            ).input_value(),
            "finalTerminalName": (
                await page.locator(
                    "#terminal-specified-task-terminal-name-preview"
                ).text_content()
            ).strip(),
            "title": (
                await page.locator("#terminal-specified-task-title").text_content()
            ).strip(),
            "sessionVisible": await page.locator(
                "#terminal-specified-task-session-id-field"
            ).is_visible(),
            "geometry": await dialog_geometry(page),
        }
        assert results["terminal"]["session"] == ""
        assert results["terminal"]["terminalName"] == source_terminal_name
        source_terminal_base = name_without_auto_indices(source_terminal_name)
        assert results["terminal"]["finalTerminalName"] == f"{source_terminal_base}_new"
        assert results["terminal"]["title"] == "指定预设终端"
        assert results["terminal"]["sessionVisible"]
        assert results["terminal"]["geometry"]["inViewport"]
        assert not results["terminal"]["geometry"]["pageOverflow"]
        await page.click("#terminal-specified-task-close")

        await page.evaluate(
            f"""() => {{
                const originalOpenTerminalSpecifiedTaskDialog =
                    openTerminalSpecifiedTaskDialog;
                const sourceSessionId = state.activeSessionId;
                const renderListeners = new Set();
                const sourceContext = {{
                    sessionId: sourceSessionId,
                    term: {{
                        text: 'terminal buffer before /fork',
                        onRender(callback) {{
                            renderListeners.add(callback);
                            window.__designateForkWaitAttached = true;
                            return {{ dispose: () => renderListeners.delete(callback) }};
                        }},
                    }},
                }};
                window.__designateForkOriginals = {{
                    ensureTerminalSessionCache,
                    terminalContextSocketOpen,
                    terminalInitialReplaySettled,
                    readTerminalBufferTailTextFrom,
                    openTerminalSpecifiedTaskDialog,
                }};
                window.__designateForkReadLimits = [];
                openTerminalSpecifiedTaskDialog = async (trigger, options = {{}}) => {{
                    window.__designateForkDialogOptions = {{ ...options }};
                    return originalOpenTerminalSpecifiedTaskDialog(trigger, options);
                }};
                ensureTerminalSessionCache = () => ({{
                    get: (sessionId) => sessionId === sourceSessionId ? sourceContext : null,
                }});
                terminalContextSocketOpen = (context) => context === sourceContext;
                terminalInitialReplaySettled = (context) => context === sourceContext;
                readTerminalBufferTailTextFrom = (terminal, maxLines = 240) => {{
                    window.__designateForkReadLimits.push(maxLines);
                    return String(terminal?.text || '')
                        .split('\\n')
                        .slice(-maxLines)
                        .join('\\n');
                }};
                detectAgentResumeIdComplete = async () => ({{
                    resumeId: '{ORIGINAL_SESSION_ID}',
                    command: 'codex resume {ORIGINAL_SESSION_ID}',
                    program: 'codex',
                }});
                runTerminalSlashCommandByKey = async (key, options) => {{
                    const accepted = key === 'fork' && options.sessionId === sourceSessionId;
                    window.__designateForkRenderPending = true;
                    return accepted;
                }};
                window.__resolveDesignateForkRender = () => {{
                    sourceContext.term.text = [
                        'Token usage: total=298,446 input=269,724 output=28,722',
                        'To continue this session, run codex resume {ORIGINAL_SESSION_ID}',
                        'MCP client for `openchatcut` failed to start',
                        'handshaking with MCP server failed',
                        'Send message error Transport',
                        'HTTP request failed',
                        'error sending request for url',
                        'when send initialize request',
                        'MCP startup incomplete (failed: openchatcut)',
                        '',
                        'Improve documentation in @filename',
                    ].join('\\n');
                    for (const listener of renderListeners) {{
                        listener({{ start: 0, end: 10 }});
                    }}
                }};
                window.__restoreDesignateForkFakes = () => {{
                    const originals = window.__designateForkOriginals;
                    ensureTerminalSessionCache = originals.ensureTerminalSessionCache;
                    terminalContextSocketOpen = originals.terminalContextSocketOpen;
                    terminalInitialReplaySettled = originals.terminalInitialReplaySettled;
                    readTerminalBufferTailTextFrom = originals.readTerminalBufferTailTextFrom;
                    openTerminalSpecifiedTaskDialog = originals.openTerminalSpecifiedTaskDialog;
                }};
            }}"""
        )
        await page.select_option(
            "#terminal-project-command-select", "designate_preset_fork"
        )
        await page.wait_for_function(
            "() => window.__designateForkRenderPending === true"
            " && window.__designateForkWaitAttached === true"
        )
        assert not await page.locator("#terminal-specified-task-dialog").is_visible()
        await page.evaluate("() => window.__resolveDesignateForkRender()")
        await page.wait_for_selector("#terminal-specified-task-dialog[open]")
        await page.wait_for_function(
            f"() => document.querySelector('#terminal-specified-task-session-id')"
            f".value === '{ORIGINAL_SESSION_ID}'"
        )
        results["fork"] = {
            "session": await page.locator(
                "#terminal-specified-task-session-id"
            ).input_value(),
            "sourceTerminalName": await page.locator(
                "#terminal-specified-task-terminal-name"
            ).input_value(),
            "targetTerminalName": await page.evaluate(
                "() => window.__designateForkDialogOptions?.terminalName || ''"
            ),
            "finalTerminalName": (
                await page.locator(
                    "#terminal-specified-task-terminal-name-preview"
                ).text_content()
            ).strip(),
            "title": (
                await page.locator("#terminal-specified-task-title").text_content()
            ).strip(),
            "codexLocked": await page.locator(
                'input[name="terminal-specified-task-agent"][value="codex"]'
            ).is_disabled(),
            "geometry": await dialog_geometry(page),
            "bufferReadLimits": await page.evaluate(
                "() => window.__designateForkReadLimits"
            ),
        }
        assert results["fork"]["session"] == ORIGINAL_SESSION_ID
        assert results["fork"]["bufferReadLimits"]
        assert all(limit == 20 for limit in results["fork"]["bufferReadLimits"])
        assert results["fork"]["sourceTerminalName"] == source_terminal_name
        assert results["fork"]["finalTerminalName"] == f"{source_terminal_base}_fork"
        await page.locator("#terminal-specified-task-terminal-name").fill("custom-fork")
        await page.wait_for_function(
            "() => document.querySelector('#terminal-specified-task-terminal-name-preview')"
            ".textContent.trim() === 'custom-fork_fork'"
        )
        results["fork"]["editedFinalTerminalName"] = (
            await page.locator(
                "#terminal-specified-task-terminal-name-preview"
            ).text_content()
        ).strip()
        assert results["fork"]["editedFinalTerminalName"] == "custom-fork_fork"
        assert results["fork"]["title"] == "指定预设+fork（持久切换）"
        assert results["fork"]["codexLocked"]
        assert results["fork"]["geometry"]["inViewport"]
        await page.evaluate("() => window.__restoreDesignateForkFakes()")
        await page.screenshot(
            path="/tmp/webclx-designate-preset-fork-768.png", full_page=True
        )

        await page.evaluate(
            """() => {
                executeSpecifiedPreset = async (options) => {
                    window.__designateForkSubmittedOptions = { ...options };
                    return { launchResult: { id: 'browser-qa', name: options.terminalName } };
                };
            }"""
        )
        await page.click("#terminal-specified-task-run")
        await page.wait_for_function(
            "() => !document.querySelector('#terminal-specified-task-dialog').open"
        )
        results["fork"]["submittedTerminalName"] = await page.evaluate(
            "() => window.__designateForkSubmittedOptions?.terminalName || ''"
        )
        results["fork"]["closedAfterLaunch"] = not await page.locator(
            "#terminal-specified-task-dialog"
        ).is_visible()
        assert results["fork"]["submittedTerminalName"] == "custom-fork_fork"
        assert results["fork"]["closedAfterLaunch"]

        await page.evaluate(
            f"""() => openTerminalDesignatePresetDialog({{
                cwd: state.currentPath,
                program: 'codex',
                sessionId: '{ORIGINAL_SESSION_ID}',
                sourceTerminalName: '{source_terminal_name}',
                namingAction: 'fork',
            }})"""
        )
        await page.wait_for_selector("#terminal-specified-task-dialog[open]")

        await page.set_viewport_size({"width": 375, "height": 812})
        results["mobile"] = await dialog_geometry(page)
        assert results["mobile"]["inViewport"]
        assert not results["mobile"]["pageOverflow"]
        await page.screenshot(
            path="/tmp/webclx-designate-preset-fork-375.png", full_page=True
        )

        await page.click("#terminal-specified-task-close")
        await page.evaluate(
            """() => openTerminalDesignatePresetDialog({
                cwd: state.currentPath,
                program: 'codex',
                sourceTerminalName: 'webClx_18_整合预设',
            })"""
        )
        await page.wait_for_selector("#terminal-specified-task-dialog[open]")
        probe_input_name = await page.locator(
            "#terminal-specified-task-terminal-name"
        ).input_value()
        probe_final_name = (
            await page.locator(
                "#terminal-specified-task-terminal-name-preview"
            ).text_content()
        ).strip()
        assert probe_input_name == "webClx_18_整合预设"
        assert probe_final_name.startswith("webClx_整合预设_new")
        assert not re.search(r"[_#]\d+(?=$|[\s_])", probe_final_name)
        await page.click("#terminal-specified-task-close")

        created_session_id = None
        probe_path = await page.evaluate("() => state.currentPath")
        try:
            created_response = await page.request.post(
                f"{BASE_URL}/api/terminal/sessions",
                data={"path": probe_path},
            )
            assert created_response.ok, await created_response.text()
            created_body = await created_response.json()
            created_session_id = created_body["id"]
            assert created_body["path"] == probe_path

            renamed_response = await page.request.put(
                f"{BASE_URL}/api/terminal/sessions/{created_session_id}",
                data={"name": probe_final_name},
            )
            assert renamed_response.ok, await renamed_response.text()
            renamed_body = await renamed_response.json()
            assert renamed_body["id"] == created_session_id
            assert renamed_body["name"] == probe_final_name
            results["backendRename"] = {
                "sourceName": probe_input_name,
                "finalName": probe_final_name,
                "accepted": True,
            }
        finally:
            if created_session_id is not None:
                sessions_response = await page.request.get(
                    f"{BASE_URL}/api/terminal/sessions?path={probe_path}"
                )
                assert sessions_response.ok, await sessions_response.text()
                sessions_body = await sessions_response.json()
                cleanup_target = next(
                    (
                        session
                        for session in sessions_body.get("sessions", [])
                        if session.get("id") == created_session_id
                        and session.get("path") == probe_path
                    ),
                    None,
                )
                if cleanup_target is not None:
                    deleted_response = await page.request.delete(
                        f"{BASE_URL}/api/terminal/sessions/{created_session_id}",
                        headers={
                            "X-WebClx-Confirm-Session": created_session_id,
                            "X-WebClx-Delete-Source": "browser-qa",
                        },
                    )
                    assert deleted_response.ok, await deleted_response.text()
                else:
                    raise AssertionError(
                        f"created browser QA session cannot be safely identified: {created_session_id}"
                    )

        assert page_errors == [], page_errors
        optional_icon_failures = [
            response
            for response in failed_responses
            if "/api/workspace-icon?" in response["url"]
            and response["status"] == 404
        ]
        unexpected_failed_responses = [
            response
            for response in failed_responses
            if response not in optional_icon_failures
        ]
        unexpected_console_errors = list(console_errors)
        for _ in optional_icon_failures:
            try:
                unexpected_console_errors.remove(
                    "Failed to load resource: the server responded with a status of 404 (Not Found)"
                )
            except ValueError:
                break
        assert unexpected_console_errors == [], {
            "console_errors": unexpected_console_errors,
            "failed_responses": unexpected_failed_responses,
        }
        assert unexpected_failed_responses == [], unexpected_failed_responses
        await browser.close()

    print(json.dumps(results, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    asyncio.run(main())
