import asyncio
import mimetypes
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROMIUM = "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome"
SESSION_ID = "cursor-ime-anchor-session"


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
                await route.fulfill(
                    json={
                        "workspace_dir": "/home/codes",
                        "desktop_terminal_soft_keyboard_enabled": False,
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
                                "name": "Cursor IME anchor",
                                "path": "webClx",
                                "display_path": "/webClx",
                                "connected": True,
                            }
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
            websocket.send(b"\r\n[root@webclx webClx]# ")

        await page.route("**/*", handle_http)
        await page.route_web_socket("**/api/terminal/ws?**", handle_websocket)
        await page.goto(
            f"http://webclx.test/terminal?path=webClx&session={SESSION_ID}",
            wait_until="domcontentloaded",
        )
        await page.wait_for_function(
            """() => state.activeSessionId === 'cursor-ime-anchor-session'
                && terminalHelperTextarea()
                && activeTerminalContext?.hasLoadedOutput
                && !activeTerminalContext?.outputWriteInFlight
                && activeTerminalContext?.outputQueue?.length === 0"""
        )

        anchor = await page.evaluate(
            """async () => {
              term.reset();
              const rows = term.rows;
              const inputRow = rows - 3;
              await new Promise((resolve) => term.write(
                `\u001b[2J\u001b[H\u001b[${inputRow + 1};1H› 输入中文`
                  + `\u001b[${rows - 1};1H\u001b[${rows};1HEsc cancel   Ctrl+C quit`,
                resolve,
              ));
              const target = terminalCursorCorrectionTarget();
              syncTerminalCursorCorrection();
              const helper = terminalHelperTextarea();
              const installedBeforeComposition = terminalCursorCorrectionImeHelpers.has(helper);
              helper.value = '';
              helper.dispatchEvent(new CompositionEvent('compositionstart', { bubbles: true }));
              helper.value = '中';
              helper.dispatchEvent(new CompositionEvent('compositionupdate', {
                bubbles: true,
                data: '中',
              }));
              await new Promise((resolve) => setTimeout(resolve, 120));
              const composition = term.element.querySelector('.composition-view.active');
              const dimensions = terminalCellDimensions();
              const result = {
                target,
                correctionActive: terminalCursorCorrectionActive,
                installedBeforeComposition,
                imeFrame: terminalCursorCorrectionImeFrame,
                expectedTop: `${inputRow * dimensions.height}px`,
                compositionTop: composition?.style.top || '',
                compositionLeft: composition?.style.left || '',
                helperTop: helper.style.top,
                helperLeft: helper.style.left,
              };
              helper.dispatchEvent(new CompositionEvent('compositionend', { bubbles: true }));
              return result;
            }"""
        )
        await page.screenshot(path="/tmp/webclx-terminal-cursor-ime-anchor.png")
        await browser.close()

    assert anchor["target"] is not None, anchor
    assert anchor["correctionActive"], anchor
    assert anchor["compositionTop"] == anchor["expectedTop"], anchor
    assert anchor["helperTop"] == anchor["expectedTop"], anchor
    assert anchor["compositionLeft"] == anchor["helperLeft"], anchor
    assert not page_errors, page_errors
    print(f"terminal cursor IME anchor browser test passed: {anchor}")


if __name__ == "__main__":
    asyncio.run(main())
