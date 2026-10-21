import asyncio
import json
import mimetypes
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROMIUM = "/home/third_party/browser-tools/bin/chromium"


async def main():
    page_errors = []
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        try:
            page = await browser.new_page(viewport={"width": 1200, "height": 760})
            page.on("pageerror", lambda error: page_errors.append(str(error)))

            async def handle_http(route):
                parsed = urlparse(route.request.url)
                if parsed.path == "/terminal":
                    await route.fulfill(
                        status=200,
                        content_type="text/html; charset=utf-8",
                        body=(STATIC / "terminal.html").read_bytes(),
                    )
                    return
                if parsed.path.startswith("/assets/"):
                    asset = STATIC / parsed.path.removeprefix("/assets/")
                    if asset.is_file():
                        content_type = mimetypes.guess_type(asset.name)[0] or "application/octet-stream"
                        await route.fulfill(status=200, content_type=content_type, body=asset.read_bytes())
                    else:
                        await route.fulfill(status=404, body="missing asset")
                    return
                if parsed.path == "/api/settings":
                    await route.fulfill(json={"workspace_dir": "/home/codes", "desktop_terminal_soft_keyboard_enabled": False})
                    return
                if parsed.path == "/api/terminal/sessions":
                    await route.fulfill(
                        json={
                            "path": "webClx",
                            "display_path": "/home/codes/tools/webClx",
                            "sessions": [
                                {
                                    "id": "cline-idle",
                                    "name": "Cline idle",
                                    "path": "webClx",
                                    "display_path": "/home/codes/tools/webClx",
                                    "idle": False,
                                    "connected": True,
                                    "activity_state": "working",
                                    "activity_label": "工作中",
                                    "activity_agent": "Cline",
                                    "last_output_at": 1,
                                    "last_opened_at": 2,
                                }
                            ],
                        }
                    )
                    return
                if parsed.path in {"/api/terminal/scheduled-inputs", "/api/terminal/auto-continue-tasks", "/api/terminal/resume-archives"}:
                    await route.fulfill(json={"tasks": [], "archives": []})
                    return
                if parsed.path.startswith("/api/"):
                    await route.fulfill(json={})
                    return
                await route.fulfill(status=404, body="not found")

            frame_rows = 24
            input_row = frame_rows - 3
            input_column = 9
            input_messages = []

            async def handle_websocket(websocket):
                websocket.on_message(lambda message: input_messages.append(message))
                websocket.send(json.dumps({"type": "terminal_backlog_replay", "action": "start"}))
                # Cline draws a right-side bar in the input row and leaves its
                # logical cursor on the bottom status row while the input is idle.
                websocket.send(
                    (
                        "\x1b[2J\x1b[H"
                        f"\x1b[{input_row + 1};1H"
                        f"\x1b[{input_row + 1};{input_column + 1}H"
                        "\x1b[38;2;154;158;163m"
                        + (" " * 60) + "\x1b[7m \x1b[0m"
                        + "\r\n\x1b[K\x1b[K"
                        + "\x1b[38;2;255;234;127m● Plan\x1b[0m \x1b[38;2;128;128;128m○ Act (Tab)\x1b[0m"
                        + f"\x1b[{frame_rows};{input_column + 1}H"
                    ).encode()
                )
                websocket.send(json.dumps({"type": "terminal_backlog_replay", "action": "end"}))

            await page.route("**/*", handle_http)
            await page.route_web_socket("**/api/terminal/ws?**", handle_websocket)
            await page.goto("http://webclx.test/terminal?path=webClx&session=cline-idle", wait_until="domcontentloaded")
            await page.wait_for_function(
                """() => state.activeSessionId === 'cline-idle'
                    && activeTerminalContext?.hasLoadedOutput === true"""
            )
            await page.wait_for_timeout(250)

            state = await page.evaluate(
                """() => ({
                    dropdown: sessionSelectEl.selectedOptions[0]?.textContent || '',
                    helperFocused: document.activeElement === terminalHelperTextarea(),
                    nativeCursorHidden: isTerminalCursorHidden(),
                    correctionMarkerHidden: document.querySelector('.terminal-cursor-correction')?.hidden,
                    softCursorHidden: document.querySelector('.terminal-soft-keyboard-cursor')?.hidden,
                    cursor: {
                        row: term.buffer.active.cursorY,
                        column: term.buffer.active.cursorX,
                    },
                    applicationCursorRows: terminalViewportEditableApplicationCursorRows(),
                })"""
            )
            assert state["dropdown"] == "[工作中] Cline idle", state
            assert state["helperFocused"] is True, state
            assert state["nativeCursorHidden"] is False, state
            assert state["correctionMarkerHidden"] is not False, state
            assert state["applicationCursorRows"] == [input_row], state
            assert state["cursor"] == {"row": frame_rows - 1, "column": input_column}, state

            await page.keyboard.type("x")
            await page.wait_for_timeout(100)
            assert any('"type":"input"' in message and '"data":"x"' in message for message in input_messages), input_messages
            input_state = await page.evaluate(
                """() => ({
                    cursor: { row: term.buffer.active.cursorY, column: term.buffer.active.cursorX },
                    correctionMarkerHidden: document.querySelector('.terminal-cursor-correction')?.hidden,
                })"""
            )
            assert input_state == {
                "cursor": {"row": frame_rows - 1, "column": input_column},
                "correctionMarkerHidden": None,
            }, input_state
            assert not page_errors, page_errors
        finally:
            await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
