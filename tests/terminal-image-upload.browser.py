import asyncio
import json
import mimetypes
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROMIUM = "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome"
SESSION_ID = "image-upload-session"


async def main():
    page_errors = []
    upload_requests = []

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
                        "desktop_terminal_soft_keyboard_enabled": True,
                        "terminal_function_commands": [],
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
                                "name": "Image upload",
                                "path": "webClx",
                                "display_path": "/webClx",
                                "idle": False,
                                "connected": True,
                            }
                        ],
                    }
                )
                return
            if parsed.path == f"/api/terminal/sessions/{SESSION_ID}/paste-assets":
                upload_requests.append(
                    {
                        "content_type": route.request.headers.get("content-type", ""),
                        "body": route.request.post_data_buffer,
                    }
                )
                await route.fulfill(
                    json={
                        "assets": [
                            {
                                "name": "paste-client-image.png",
                                "relative_path": ".webclx-paste/paste-client-image.png",
                                "mime": "image/png",
                                "size": 16,
                            }
                        ]
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
            """() => state.activeSessionId === 'image-upload-session'
                && terminalSoftKeyboardVisible()
                && typeof handleTerminalImageUploadSelection === 'function'"""
        )
        await page.evaluate(
            """() => {
              sendPastedText = (text) => {
                window.__imageUploadPrompt = String(text);
                return true;
              };
            }"""
        )

        await page.click("#terminal-function-command-button")
        menu = page.locator("#terminal-function-command-menu")
        upload_button = page.locator("#terminal-image-upload-button")
        assert await menu.is_visible()
        assert await upload_button.is_visible()

        layout = await upload_button.evaluate(
            """(button) => {
              const rect = button.getBoundingClientRect();
              return {
                left: rect.left,
                right: rect.right,
                top: rect.top,
                bottom: rect.bottom,
                viewportWidth: document.documentElement.clientWidth,
              };
            }"""
        )
        assert layout["left"] >= 0, layout
        assert layout["right"] <= layout["viewportWidth"], layout
        assert layout["bottom"] > layout["top"], layout
        await page.screenshot(
            path="/tmp/webclx-terminal-image-upload-menu-mobile.png",
            full_page=True,
        )

        async with page.expect_file_chooser() as chooser_info:
            await upload_button.click()
        chooser = await chooser_info.value
        assert await menu.is_hidden()
        assert chooser.is_multiple()
        await chooser.set_files(
            {
                "name": "client-image.png",
                "mimeType": "image/png",
                "buffer": b"\x89PNG\r\n\x1a\nclient",
            }
        )

        await page.wait_for_function("() => Boolean(window.__imageUploadPrompt)")
        prompt = await page.evaluate("window.__imageUploadPrompt")
        status = await page.locator("#terminal-status").inner_text()
        await page.screenshot(path="/tmp/webclx-terminal-image-upload-mobile.png", full_page=True)
        await browser.close()

    assert not page_errors, page_errors
    assert len(upload_requests) == 1, upload_requests
    assert upload_requests[0]["content_type"].startswith("multipart/form-data; boundary="), upload_requests
    assert b'filename="client-image.png"' in upload_requests[0]["body"], upload_requests
    assert prompt == "请查看这张图片文件：.webclx-paste/paste-client-image.png", prompt
    assert "已上传 1 张图片" in status, status
    print(
        json.dumps(
            {
                "content_type": upload_requests[0]["content_type"].split(";", 1)[0],
                "menu_fits_mobile_viewport": True,
                "prompt": prompt,
                "status": status,
            },
            ensure_ascii=False,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    asyncio.run(main())
