"""Read-only live UI QA: all mutation requests are blocked, never save settings."""
import asyncio
import importlib.util
from urllib.parse import urlparse
import hashlib
import json
import os
from pathlib import Path
from playwright.async_api import async_playwright

ROOT = Path(__file__).resolve().parents[1]
BASE = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111")
CHROME = os.environ.get("WEBCLX_TEST_CHROMIUM", "/home/third_party/browser-tools/bin/chromium")

async def main():
    assert urlparse(BASE).hostname in {"127.0.0.1", "localhost", "::1"}, "Local QA requires loopback"
    spec = importlib.util.spec_from_file_location("viewport", ROOT / "tests/terminal-shared-viewport.browser.py")
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    helper.BASE_URL = BASE
    token = Path("/home/bin/webclx/.webclx-local-api-token").read_text().strip()
    async with async_playwright() as p:
        browser = await p.chromium.launch(executable_path=CHROME, headless=True, args=["--no-sandbox"])
        try:
            context = await browser.new_context(extra_http_headers={"X-WebClx-Local-Token": token})
            await context.add_cookies([helper.local_session_cookie()])
            blocked, errors = [], []
            async def readonly(route):
                if route.request.method not in ("GET", "HEAD", "OPTIONS"):
                    blocked.append(route.request.url)
                    await route.abort()
                else:
                    await route.continue_()
            await context.route("**/api/**", readonly)
            page = await context.new_page()
            page.on("pageerror", lambda error: errors.append(str(error)))
            page.on("dialog", lambda dialog: dialog.accept())
            # Never attach a live PTY during menu-only QA.
            await context.route_web_socket("**/api/terminal/**", lambda ws: ws.close())
            for width in (1440, 375):
                await page.set_viewport_size({"width": width, "height": 900})
                await page.goto(BASE + "/terminal", wait_until="networkidle")
                await page.wait_for_function("typeof openTerminalToolMenu === 'function'")
                await page.evaluate("""() => {
                  state.terminalToolEntries = [
                    {id:'qa-folder',kind:'folder',root_key:'tools',label:'开发环境',sort_order:0},
                    {id:'qa-action',kind:'action',root_key:'tools',parent_id:'qa-folder',label:'API 调试',sort_order:0,actions:[]},
                    {id:'qa-other',kind:'action',root_key:'other',label:'API 发布',sort_order:0,actions:[]}
                  ];
                }""")
                await page.click("#terminal-workflows-button")
                search = page.locator("#terminal-tool-search")
                await search.fill("api")
                assert await page.locator("[data-terminal-tool-entry]").count() == 2
                await search.fill("开发 API")
                assert await page.locator("[data-terminal-tool-entry]").count() == 1
                assert "开发环境" in await page.locator("#terminal-tool-menu-body").inner_text(), await page.locator("#terminal-tool-menu-body").inner_text()
                await search.press("Enter")
                assert await search.is_visible()  # Enter must not launch a workflow.
                box = await search.bounding_box()
                assert box["width"] > 150 and box["x"] >= 0 and box["x"] + box["width"] <= width + 1, box
                await page.screenshot(path=f"/tmp/workflow-search-{width}.png")
                await search.fill("不存在")
                assert "没有匹配" in await page.locator("#terminal-tool-menu-body").inner_text()
                # Empty-search navigation is covered by the unit renderer test; use a global result next.
                await search.fill("发布")
                assert await page.locator('[data-terminal-tool-entry="qa-other"]').count() == 1
            hashes = {}
            for name in ("terminal.js", "terminal-tool-actions.js"):
                response = await context.request.get(BASE + "/assets/" + name + "?v=20260926-workflow-history")
                assert response.status == 200
                digest = hashlib.sha256(await response.body()).hexdigest()
                assert digest == hashlib.sha256((ROOT / "static" / name).read_bytes()).hexdigest(), (name, digest, hashlib.sha256((ROOT / "static" / name).read_bytes()).hexdigest())
                hashes[name] = digest
            terminal_html = await (await context.request.get(BASE + "/terminal")).text()
            assert "terminal-tool-search" in terminal_html and "workflow-history" in terminal_html
            assert not blocked, blocked
            assert not errors, errors
            print(json.dumps({"viewports": [1440, 375], "mutations": blocked, "page_errors": errors, "hashes": hashes}, ensure_ascii=False))
        finally:
            await browser.close()

asyncio.run(main())
