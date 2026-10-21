import asyncio
import json
import mimetypes
from pathlib import Path
from urllib.parse import parse_qs, urlparse

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROMIUM = "/home/third_party/browser-tools/bin/chromium"


async def main():
    page_errors = []
    created_ids = []
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        try:
            page = await browser.new_page(viewport={"width": 1440, "height": 900})
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
                    await route.fulfill(
                        json={
                            "workspace_dir": "/home/codes",
                            "terminal_new_session_behavior": "direct",
                            "desktop_terminal_soft_keyboard_enabled": True,
                        }
                    )
                    return
                if parsed.path == "/api/terminal/sessions" and route.request.method == "POST":
                    session = {
                        "id": "created-focus",
                        "name": "Created Focus",
                        "path": "webClx",
                        "display_path": "/webClx",
                        "idle": False,
                        "connected": True,
                    }
                    created_ids.append(session["id"])
                    await route.fulfill(json=session)
                    return
                if parsed.path == "/api/terminal/sessions":
                    await route.fulfill(
                        json={
                            "path": "webClx",
                            "display_path": "/webClx",
                            "sessions": [
                                {
                                    "id": "initial-session",
                                    "name": "Initial",
                                    "path": "webClx",
                                    "display_path": "/webClx",
                                    "idle": False,
                                    "connected": True,
                                },
                                *(
                                    [
                                        {
                                            "id": "created-focus",
                                            "name": "Created Focus",
                                            "path": "webClx",
                                            "display_path": "/webClx",
                                            "idle": False,
                                            "connected": True,
                                        },
                                    ]
                                    if created_ids
                                    else []
                                ),
                            ],
                        }
                    )
                    return
                if parsed.path in {
                    "/api/terminal/scheduled-inputs",
                    "/api/terminal/auto-continue-tasks",
                    "/api/terminal/resume-archives",
                }:
                    await route.fulfill(json={"tasks": [], "archives": []})
                    return
                if parsed.path.startswith("/api/"):
                    await route.fulfill(json={})
                    return
                await route.fulfill(status=404, body="not found")

            async def handle_websocket(websocket):
                websocket.send(json.dumps({"type": "terminal_backlog_replay", "action": "start"}))
                websocket.send(b"\r\n[root@webclx webClx]# ")
                websocket.send(json.dumps({"type": "terminal_backlog_replay", "action": "end"}))

            await page.route("**/*", handle_http)
            await page.route_web_socket("**/api/terminal/ws?**", handle_websocket)
            await page.goto(
                "http://webclx.test/terminal?path=webClx&session=initial-session",
                wait_until="domcontentloaded",
            )
            await page.wait_for_function(
                """() => state.activeSessionId === 'initial-session'
                    && activeTerminalContext?.hasLoadedOutput === true"""
            )

            await page.click("#create-session")
            await page.wait_for_function(
                """() => state.activeSessionId === 'created-focus'
                    && activeTerminalContext?.sessionId === 'created-focus'
                    && activeTerminalContext?.hasLoadedOutput === true"""
            )
            focus_state = await page.evaluate(
                """() => ({
                    activeSessionId: state.activeSessionId,
                    helperFocused: document.activeElement === terminalHelperTextarea(),
                    createButtonFocused: document.activeElement === createSessionButton,
                })"""
            )
            assert focus_state == {
                "activeSessionId": "created-focus",
                "helperFocused": True,
                "createButtonFocused": False,
            }, focus_state

            await page.keyboard.press("ArrowUp")
            keyboard_state = await page.evaluate(
                """() => ({
                    activeSessionId: state.activeSessionId,
                    helperFocused: document.activeElement === terminalHelperTextarea(),
                })"""
            )
            assert keyboard_state == {
                "activeSessionId": "created-focus",
                "helperFocused": True,
            }, keyboard_state
            assert not page_errors, page_errors
        finally:
            await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
