import asyncio
import mimetypes
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROMIUM = "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome"
SESSION_ID = "document-draft-session"
DRAFT_STORAGE_KEY = "webclx:terminal-document-drafts"


async def main():
    page_errors = []
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = await browser.new_page(viewport={"width": 1280, "height": 800})
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
                    await route.fulfill(
                        status=200,
                        content_type=mimetypes.guess_type(asset.name)[0]
                        or "application/octet-stream",
                        body=asset.read_bytes(),
                    )
                else:
                    await route.fulfill(status=404, body="missing asset")
                return
            if parsed.path == "/api/settings":
                await route.fulfill(json={"workspace_dir": "/home/codes"})
                return
            if parsed.path == "/api/terminal/sessions":
                await route.fulfill(
                    json={
                        "path": "webClx",
                        "display_path": "/webClx",
                        "sessions": [
                            {
                                "id": SESSION_ID,
                                "name": "Document draft",
                                "path": "webClx",
                                "display_path": "/webClx",
                                "idle": False,
                                "connected": True,
                            }
                        ],
                    }
                )
                return
            if parsed.path.endswith("/agents-docs"):
                await route.fulfill(
                    json={
                        "documents": [
                            {
                                "path": "AGENTS.md",
                                "display_path": "/webClx/AGENTS.md",
                                "label": "AGENTS.md",
                                "exists": True,
                            }
                        ]
                    }
                )
                return
            if parsed.path.endswith("/agents-doc"):
                await route.fulfill(
                    json={
                        "path": "AGENTS.md",
                        "display_path": "/webClx/AGENTS.md",
                        "exists": True,
                        "content": "server content",
                        "documents": [
                            {
                                "path": "AGENTS.md",
                                "display_path": "/webClx/AGENTS.md",
                                "label": "AGENTS.md",
                                "exists": True,
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
        await page.wait_for_function(f"activeSession()?.id === '{SESSION_ID}'")

        await page.evaluate("openTerminalAgentsDocEditor()")
        editor = page.locator("#terminal-agents-doc-editor")
        await editor.wait_for(state="visible")
        await editor.fill("draft survives reload")
        assert await page.evaluate(
            f"JSON.parse(sessionStorage.getItem('{DRAFT_STORAGE_KEY}'))[0].content"
        ) == "draft survives reload"

        await page.reload(wait_until="domcontentloaded")
        await page.wait_for_function(f"activeSession()?.id === '{SESSION_ID}'")
        await page.evaluate("openTerminalAgentsDocEditor()")
        await editor.wait_for(state="visible")
        assert await editor.input_value() == "draft survives reload"
        assert "已恢复未保存的编辑内容" in await page.locator(
            "#terminal-agents-doc-status"
        ).inner_text()

        await page.locator("#terminal-agents-doc-save").click()
        await page.wait_for_function(
            f"sessionStorage.getItem('{DRAFT_STORAGE_KEY}') === null"
        )

        # Verify Chinese input in document editor without manual focus
        await page.wait_for_function("document.activeElement?.id === 'terminal-agents-doc-editor'")
        await page.keyboard.type("中文内容")
        assert "中文内容" in await editor.input_value()

        # Close doc dialog and verify terminal focus is restored
        await page.locator("#terminal-agents-doc-close").click()
        await page.wait_for_function("!document.getElementById('terminal-agents-doc-dialog').open")
        await page.locator("#terminal-host").click()
        await page.wait_for_function("terminalHelperTextareaFocused()")

        assert not page_errors, page_errors
        await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
