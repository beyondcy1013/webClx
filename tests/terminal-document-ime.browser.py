import asyncio
import json
import mimetypes
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROMIUM = "/home/third_party/browser-tools/bin/chromium"
SESSION_ID = "document-ime-session"
WINDOWS_UA = (
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/146.0.0.0 Safari/537.36"
)


async def run_case(browser, *, soft_keyboard, touch=False, launch_dialog=False, dialog_audit=False):
    page = await browser.new_page(
        viewport={"width": 390 if touch else 1280, "height": 800},
        user_agent=None if touch else WINDOWS_UA,
        is_mobile=touch,
        has_touch=touch,
    )
    page.set_default_timeout(5000)
    page_errors = []
    received_input = []
    page.on("pageerror", lambda error: page_errors.append(str(error)))
    sessions = [{"id": SESSION_ID, "name": "Document IME", "path": "webClx", "connected": True}]
    documents = [
        {"path": "AGENTS.md", "label": "AGENTS.md", "exists": True},
        {"path": "docs/readme.md", "label": "docs/readme.md", "exists": True},
    ]

    async def handle_http(route):
        parsed = urlparse(route.request.url)
        if parsed.path == "/terminal":
            await route.fulfill(
                content_type="text/html; charset=utf-8",
                body=(STATIC / "terminal.html").read_bytes(),
            )
        elif parsed.path.startswith("/assets/"):
            asset = STATIC / parsed.path.removeprefix("/assets/")
            if asset.is_file():
                await route.fulfill(
                    content_type=mimetypes.guess_type(asset.name)[0]
                    or "application/octet-stream",
                    body=asset.read_bytes(),
                )
            else:
                await route.fulfill(status=404, body="missing asset")
        elif parsed.path == "/api/settings":
            await route.fulfill(json={
                "workspace_dir": "/home/codes",
                "desktop_terminal_soft_keyboard_enabled": soft_keyboard,
            })
        elif parsed.path == "/api/terminal/auto-typed-input":
            command = route.request.post_data_json["command_line"]
            received_input.append(command)
            await route.fulfill(json={"data": command})
        elif parsed.path == "/api/agy/presets":
            await route.fulfill(json={"presets": [{"id": "agy-test", "name": "AGY test", "current": True}]})
        elif parsed.path == "/api/terminal/sessions":
            if route.request.method == "POST":
                session = {"id": f"launch-{len(sessions)}", "name": "AGY", "path": "webClx", "connected": True}
                sessions.append(session)
                await route.fulfill(json=session)
            else:
                await route.fulfill(json={"path": "webClx", "display_path": "/webClx", "sessions": sessions})
        elif parsed.path.startswith("/api/terminal/sessions/") and route.request.method == "PUT":
            session = next(item for item in sessions if item["id"] == parsed.path.rsplit("/", 1)[-1])
            session.update(route.request.post_data_json)
            await route.fulfill(json=session)
        elif parsed.path.endswith("/agents-docs"):
            await route.fulfill(json={"documents": documents})
        elif parsed.path.endswith("/agents-doc"):
            await route.fulfill(json={
                "path": "AGENTS.md",
                "exists": True,
                "content": "document content",
                "documents": documents,
            })
        elif parsed.path.startswith("/api/"):
            await route.fulfill(json={"tasks": [], "archives": []})
        else:
            await route.fulfill(status=404, body="not found")

    async def handle_websocket(websocket):
        def receive(message):
            if isinstance(message, str):
                payload = json.loads(message)
                if payload.get("type") == "input":
                    received_input.append(payload["data"])

        websocket.on_message(receive)
        websocket.send(json.dumps({"type": "terminal_backlog_replay", "action": "start"}))
        websocket.send(b"\r\n[root@webclx webClx]# ")
        websocket.send(json.dumps({"type": "terminal_backlog_replay", "action": "end"}))

    try:
        await page.route("**/*", handle_http)
        await page.route_web_socket("**/api/terminal/ws?**", handle_websocket)
        if touch:
            await page.add_init_script("""window.WebClxAndroid = {
                isSystemKeyboardVisible: () => false,
            };""")
        await page.goto(
            f"http://webclx.test/terminal?path=webClx&session={SESSION_ID}",
            wait_until="domcontentloaded",
        )
        await page.wait_for_function("""() => terminalHelperTextarea()
            && terminalInitialReplaySettled()
            && state.desktopTerminalSoftKeyboardEnabled === %s"""
            % str(soft_keyboard).lower())
        await page.evaluate("""() => {
            window.__docImeFocusTrace = [];
            document.addEventListener('focusin', (event) => {
                window.__docImeFocusTrace.push({
                    reset: event.target.dataset?.terminalImeReset === 'true',
                    helper: event.target === terminalHelperTextarea(),
                    focused: document.activeElement === event.target,
                    blocked: terminalActionEditableFocusBlocked,
                    policyActive: terminalActionKeyboardPolicyActive,
                });
            }, true);
            window.__docImeComposition = [];
            for (const name of ['compositionstart', 'compositionend']) {
                terminalHelperTextarea().addEventListener(name, (event) => {
                    window.__docImeComposition.push({ name, data: event.data });
                });
            }
        }""")
        cdp = await page.context.new_cdp_session(page)

        async def compose(text, settle_ms=0):
            await cdp.send("Input.imeSetComposition", {
                "text": text,
                "selectionStart": len(text),
                "selectionEnd": len(text),
            })
            if settle_ms:
                await page.wait_for_timeout(settle_ms)
                assert await page.evaluate("""terminalHelperTextareaFocused()
                    && Boolean(term.element.querySelector('.composition-view.active'))""")
            await cdp.send("Input.insertText", {"text": text})

        if dialog_audit:
            # Enumerate every static terminal dialog, including nested pickers.
            # Native close and Esc cover the shared lifecycle independently of
            # APIs that might change presets or message real terminals.
            dialog_ids = await page.locator("dialog").evaluate_all("items => items.map(item => item.id)")
            close_buttons = {
                "terminal-rename-dialog": "session-rename-cancel",
                "terminal-group-dialog": "terminal-group-cancel",
                "terminal-group-manager-dialog": "terminal-group-manager-cancel",
                "terminal-future-tasks-dialog": "terminal-future-tasks-close",
                "terminal-workspace-restores-dialog": "terminal-workspace-restores-close",
                "terminal-multi-model-advisor-dialog": "terminal-multi-model-advisor-close",
                "terminal-multi-terminal-discussion-dialog": "terminal-multi-terminal-discussion-close",
                "terminal-message-dialog": "terminal-message-close",
                "terminal-permanent-preset-dialog": "terminal-permanent-preset-close",
                "terminal-in-place-preset-dialog": "terminal-in-place-preset-close",
                "terminal-quota-dialog": "terminal-quota-close",
                "terminal-paste-dialog": "terminal-paste-cancel",
                "terminal-input-history-dialog": "terminal-input-history-close",
            }
            for dialog_id in dialog_ids:
                methods = ["close", "escape"] + (["button"] if dialog_id in close_buttons else [])
                for method in methods:
                    await page.locator("#terminal-host").click()
                    await page.evaluate("""id => {
                        terminalActionKeyboardPolicyActive = true;
                        terminalActionEditableFocusBlocked = true;
                        document.getElementById(id).showModal();
                    }""", dialog_id)
                    await page.evaluate("window.__docImeFocusTrace = []")
                    if method == "close":
                        await page.evaluate("id => document.getElementById(id).close()", dialog_id)
                    elif method == "button":
                        await page.locator("#" + close_buttons[dialog_id]).click()
                    else:
                        await page.keyboard.press("Escape")
                    await page.wait_for_timeout(50)
                    await page.wait_for_function("terminalHelperTextareaFocused()")
                    trace = await page.evaluate("window.__docImeFocusTrace")
                    assert sum(event["reset"] and event["focused"] for event in trace) == 1, (dialog_id, method, trace)
                    received_input.clear()
                    await compose("审查中文", settle_ms=220)
                    await page.wait_for_function("activeTerminalContext.pendingInputChars === 0")
                    await page.wait_for_timeout(30)
                    assert "".join(received_input) == "审查中文", (dialog_id, method, received_input)
            # Closing a nested picker must leave the parent's editable focus intact.
            await page.evaluate("""() => {
                terminalSpecifiedTaskDom().dialog.showModal();
                terminalSpecifiedTaskDom().pathDialog.showModal();
                terminalSpecifiedTaskDom().pathDialog.close();
            }""")
            await page.wait_for_timeout(220)
            assert await page.evaluate("!terminalHelperTextareaFocused() && terminalSpecifiedTaskDom().dialog.open")
            await page.evaluate("terminalSpecifiedTaskDom().dialog.close()")
            await page.wait_for_timeout(220)
            assert await page.evaluate("terminalHelperTextareaFocused()")
            assert not page_errors, page_errors
            print(f"all-dialog IME passed: {len(dialog_ids)} dialogs close/Esc + {len(close_buttons)} actual cancel buttons, soft_keyboard={soft_keyboard}; nested picker preserved")
            return

        # Exercise the actual project menu, with the terminal initially focused.
        # On touch devices, starting collapsed must remain collapsed on close.
        if not touch:
            await page.locator("#terminal-host").click()
            await compose("打开前")
            await page.wait_for_function("activeTerminalContext.pendingInputChars === 0")
            await page.wait_for_timeout(50)
            assert "".join(received_input) == "打开前", received_input

        for close_action in (["button", "escape", "launch", "new"] if launch_dialog else ["button", "escape", "button"]):
            if launch_dialog:
                await page.evaluate("""action => openTerminalSpecifiedTaskDialog(
                    document.getElementById("terminal-project-command-button"), {agent: 'agy', mode: 'fixed', sessionAction: action, temporary: false}
                )""", "new" if close_action == "new" else "resume")
                await page.wait_for_function("terminalSpecifiedTaskDom().preset.value === 'agy-test'")
                await page.evaluate("window.__docImeFocusTrace = []")
                if close_action == "button":
                    await page.locator("#terminal-specified-task-close").click()
                elif close_action == "escape":
                    await page.keyboard.press("Escape")
                else:
                    await page.locator("#terminal-specified-task-run").click()
                await page.wait_for_function("!terminalSpecifiedTaskDom().dialog.open")
                # Native dialog close restores focus before its queued close event.
                await page.wait_for_timeout(50)
            else:
                if soft_keyboard or touch:
                    await page.locator("#terminal-project-command-button").click()
                    await page.locator(
                        '#terminal-project-command-menu [data-project-action="open_agents_doc"]'
                    ).click()
                else:
                    # Open the same menu action when its soft-key row is hidden.
                    await page.evaluate("""() => {
                        terminalProjectCommandMenuEl.hidden = false;
                    }""")
                    await page.locator(
                        '#terminal-project-command-menu [data-project-action="open_agents_doc"]'
                    ).click()
                await page.wait_for_function(
                    "document.activeElement === terminalAgentsDocEditorEl && !terminalAgentsDocEditorEl.disabled"
                )
                terminal_input_before = "".join(received_input)
                await compose("文档中文")
                assert "文档中文" in await page.locator("#terminal-agents-doc-editor").input_value()
                assert "".join(received_input) == terminal_input_before

                await page.evaluate("window.__docImeFocusTrace = []")
                if close_action == "button":
                    await page.locator("#terminal-agents-doc-close").click()
                else:
                    await page.keyboard.press("Escape")
                await page.wait_for_function("!terminalAgentsDocDialogEl.open")
            if touch:
                await page.wait_for_timeout(380)
            else:
                await page.wait_for_function("terminalHelperTextareaFocused()")
            focus = await page.evaluate("""() => ({
                helperFocused: terminalHelperTextareaFocused(),
                inputmode: terminalHelperTextarea().getAttribute('inputmode'),
                trace: window.__docImeFocusTrace,
            })""")
            if touch:
                if not (launch_dialog and close_action in ["launch", "new"]):
                    assert not focus["helperFocused"], focus
                assert not any(event["reset"] for event in focus["trace"]), focus
                await page.locator("#terminal-host").click()
            else:
                # Windows needs a successful neutral editable-focus transition,
                # even when the desktop webClx soft-key row is displayed.
                assert any(event["reset"] and event["focused"] for event in focus["trace"]), focus
                assert focus["helperFocused"] and focus["inputmode"] == "text", focus
                assert all(not event["blocked"] and not event["policyActive"]
                           for event in focus["trace"]
                           if event["focused"] and (event["helper"] or event["reset"])), focus

            if launch_dialog and close_action in ["launch", "new"]:
                expected_command = "agy -c" if close_action == "launch" else "agy"
                assert expected_command in received_input, received_input
            received_input.clear()
            # Composition begun immediately after close must survive the
            # delayed focus retries, not just work after they have expired.
            await compose("返回中文", settle_ms=220)
            await page.wait_for_function("activeTerminalContext.pendingInputChars === 0")
            await page.wait_for_timeout(50)
            assert "".join(received_input) == "返回中文", received_input
            events = await page.evaluate("window.__docImeComposition.slice(-2)")
            assert [event["name"] for event in events] == ["compositionstart", "compositionend"], events
            if touch:
                await page.evaluate("setTerminalSystemImeEnabled(false)")

        assert not page_errors, page_errors
        print(f"document IME passed: soft_keyboard={soft_keyboard}, touch={touch}, launch_dialog={launch_dialog}")
    except Exception:
        print("document IME diagnostics:", page_errors, await page.evaluate("""() => ({
            status: terminalSpecifiedTaskDom().status?.textContent,
            bodyStatus: document.getElementById("terminal-specified-task-status")?.textContent,
            session: state.activeSessionId,
            initializing: terminalSessionInitializing(),
            loaded: activeTerminalContext?.hasLoadedOutput,
            pending: activeTerminalContext?.initialReplayPending,
            softKeyboard: state.desktopTerminalSoftKeyboardEnabled,
            trace: window.__docImeFocusTrace,
        })"""))
        raise
    finally:
        await page.close()


async def main():
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        try:
            for soft_keyboard in [True, False]:
                await run_case(browser, soft_keyboard=soft_keyboard, dialog_audit=True)
                await run_case(browser, soft_keyboard=soft_keyboard)
            await run_case(browser, soft_keyboard=False, touch=True)
            for soft_keyboard in [True, False]:
                await run_case(browser, soft_keyboard=soft_keyboard, launch_dialog=True)
            await run_case(browser, soft_keyboard=False, touch=True, launch_dialog=True)
        finally:
            await browser.close()


if __name__ == "__main__":
    asyncio.run(asyncio.wait_for(main(), timeout=180))
