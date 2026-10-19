import asyncio
import mimetypes
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROMIUM = "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome"
SESSION_ID = "rename-focus-session"


async def main():
    page_errors = []
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = await browser.new_page(viewport={"width": 390, "height": 700})
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
                await route.fulfill(json={"workspace_dir": "/home/codes"})
                return
            if parsed.path == f"/api/terminal/sessions/{SESSION_ID}/input-history":
                await route.fulfill(
                    json={
                        "entries": [
                            {
                                "text": "修复改名按钮并验证异步加载",
                                "created_at": 1,
                            }
                        ]
                    }
                )
                return
            if parsed.path == "/api/terminal/sessions":
                await route.fulfill(
                    json={
                        "path": "webClx",
                        "display_path": "/webClx",
                        "sessions": [
                            {
                                "id": SESSION_ID,
                                "name": "Rename focus",
                                "path": "webClx",
                                "display_path": "/webClx",
                                "idle": False,
                                "connected": True,
                            }
                        ],
                    }
                )
                return
            if parsed.path.startswith("/api/"):
                await route.fulfill(json={})
                return
            await route.fulfill(status=404, body="not found")

        async def handle_websocket(websocket):
            websocket.send(b"\r\n[root@webclx webClx]# ")

        await page.route("**/*", handle_http)
        await page.route_web_socket("**/api/terminal/ws?**", handle_websocket)
        await page.goto(
            f"http://webclx.test/terminal?path=webClx&session={SESSION_ID}",
            wait_until="domcontentloaded",
        )
        await page.wait_for_function(f"() => state.activeSessionId === '{SESSION_ID}'")

        await page.locator("#rename-session").click()
        await page.wait_for_function(
            "() => document.activeElement?.id === 'session-rename-input'"
        )
        canceled_drag_stayed_open = await page.evaluate("""() => {
          const dialog = document.getElementById('terminal-rename-dialog');
          const input = document.getElementById('session-rename-input');
          input.dispatchEvent(new PointerEvent('pointerdown', {bubbles: true, pointerId: 7}));
          input.dispatchEvent(new PointerEvent('pointercancel', {bubbles: true, pointerId: 7}));
          dialog.dispatchEvent(new MouseEvent('click', {bubbles: true}));
          return dialog.open;
        }""")
        assert canceled_drag_stayed_open
        box = await page.locator("#terminal-rename-dialog").bounding_box()
        assert box
        # Backdrops are not dismissal controls. A direct outside click and a
        # drag from input to backdrop must both leave the dialog open.
        await page.mouse.click(25, 25)
        await page.wait_for_timeout(80)
        assert await page.locator("#terminal-rename-dialog").evaluate("el => el.open")
        await page.mouse.move(box["x"] + 100, box["y"] + 80)
        await page.mouse.down()
        await page.mouse.move(25, 25, steps=8)
        await page.mouse.up()
        await page.wait_for_timeout(80)
        result = await page.evaluate(
            """() => ({
              dialogOpen: document.getElementById('terminal-rename-dialog').open,
              activeId: document.activeElement?.id || '',
              inputValue: document.getElementById('session-rename-input').value,
              historyButtonText: document.querySelector(
                '#session-rename-presets [data-history-preset="true"]',
              )?.textContent,
              historyButtonDisabled: Boolean(
                document.querySelector(
                  '#session-rename-presets [data-history-preset="true"]',
                )?.disabled,
              ),
            })"""
        )
        assert await page.locator("#terminal-rename-dialog").evaluate("el => el.open")
        await browser.close()

    assert not page_errors, page_errors
    assert result == {
        "dialogOpen": True,
        "activeId": "session-rename-input",
        "inputValue": "Rename focus_",
        "historyButtonText": "修复改名按钮并验证异步加载",
        "historyButtonDisabled": False,
    }, result


if __name__ == "__main__":
    asyncio.run(main())
