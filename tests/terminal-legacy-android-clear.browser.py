import asyncio
import json
import mimetypes
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = (ROOT / "static").resolve()


async def main():
    websocket_messages = []
    page_errors = []
    console_errors = []

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path="/home/third_party/browser-tools/bin/chromium",
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = await browser.new_page(
            viewport={"width": 390, "height": 844},
            is_mobile=True,
            has_touch=True,
            user_agent=(
                "Mozilla/5.0 (Linux; Android 14; Pixel Build/UP1A; wv) "
                "AppleWebKit/537.36 Version/4.0 Chrome/126.0 Mobile Safari/537.36"
            ),
        )
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        page.on(
            "console",
            lambda message: console_errors.append(message.text)
            if message.type == "error"
            else None,
        )

        async def handle_http(route):
            path = urlparse(route.request.url).path
            if path == "/terminal":
                await route.fulfill(
                    status=200,
                    content_type="text/html; charset=utf-8",
                    body=(STATIC / "terminal.html").read_bytes(),
                )
                return
            if path.startswith("/assets/"):
                asset = STATIC / path.removeprefix("/assets/")
                if asset.is_file():
                    await route.fulfill(
                        status=200,
                        content_type=mimetypes.guess_type(asset.name)[0]
                        or "application/octet-stream",
                        body=asset.read_bytes(),
                    )
                else:
                    await route.fulfill(status=404, body="missing asset")
                return
            if path == "/api/terminal/sessions":
                await route.fulfill(
                    json={
                        "path": "demo",
                        "display_path": "/demo",
                        "sessions": [
                            {
                                "id": "legacy-android",
                                "name": "Legacy Android",
                                "path": "demo",
                                "display_path": "/demo",
                                "idle": False,
                                "connected": True,
                            }
                        ],
                    }
                )
                return
            if path == "/api/settings":
                await route.fulfill(
                    json={"terminal_scrollback_lines": 10000, "theme_mode": "dark"}
                )
                return
            if path in {
                "/api/terminal/resume-archives",
                "/api/terminal/scheduled-inputs",
                "/api/terminal/auto-continue-tasks",
            }:
                await route.fulfill(json={"archives": [], "tasks": []})
                return
            if path.startswith("/api/"):
                await route.fulfill(json={})
                return
            await route.fulfill(status=404, body="not found")

        async def handle_websocket(websocket):
            websocket.on_message(lambda message: websocket_messages.append(message))
            websocket.send(json.dumps({"type": "terminal_backlog_replay", "action": "start"}))
            websocket.send(b"OLD-CODEX-LOGO\r\n")
            websocket.send(b"\x1b[2J\x1b[H\xe2\x80\xba READY")
            websocket.send(json.dumps({"type": "terminal_backlog_replay", "action": "end"}))

        await page.route("**/*", handle_http)
        await page.route_web_socket("**/api/terminal/ws?**", handle_websocket)
        await page.goto(
            "http://webclx.test/terminal?path=demo&session=legacy-android",
            wait_until="domcontentloaded",
        )
        await page.wait_for_function(
            """() => activeTerminalContext?.sessionId === 'legacy-android' &&
                activeTerminalContext.hasLoadedOutput &&
                !activeTerminalContext.backlogReplayActive &&
                !activeTerminalContext.outputWriteInFlight""",
            timeout=5000,
        )
        await page.wait_for_timeout(30)

        result = await page.evaluate(
            """() => ({
                rendererType: activeTerminalContext.term.options.rendererType,
                canvasCount: activeTerminalContext.term.element.querySelectorAll('canvas').length,
                rowsText: activeTerminalContext.term.element.querySelector('.xterm-rows')?.textContent || '',
            })"""
        )

        assert result["rendererType"] == "dom", result
        assert result["canvasCount"] == 0, result
        assert "READY" in result["rowsText"], result
        assert "OLD-CODEX-LOGO" not in result["rowsText"], result
        assert not page_errors, page_errors
        assert not console_errors, console_errors

        await page.screenshot(
            path="/tmp/webclx-terminal-legacy-android-clear.png", full_page=True
        )
        await browser.close()

    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    asyncio.run(main())
