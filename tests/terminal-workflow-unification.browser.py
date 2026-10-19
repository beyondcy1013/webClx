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
            for width in (1440, 375):
                await page.set_viewport_size({"width": width, "height": 900})
                await page.goto(BASE + "/settings/tools", wait_until="networkidle")
                await page.wait_for_function("typeof openTerminalToolEditor === 'function'")
                await page.click("#terminal-tool-add-action")
                await page.select_option("#terminal-tool-template", "environment")
                assert await page.locator(".terminal-tool-action-kind").input_value() == "codex_launch"
                assert "加载后待命" in await page.locator("#terminal-tool-actions-list").inner_text()
                await page.select_option("#terminal-tool-template", "task")
                assert await page.locator(".terminal-tool-action-kind").evaluate_all("(els) => els.map(el => el.value)") == ["switch_api_preset", "codex_exec"]
                await page.select_option("#terminal-tool-template", "reference")
                assert await page.locator(".terminal-tool-action-kind").input_value() == "run_workflow"
                if width == 375:
                    parameter = await page.locator(".terminal-tool-run-workflow-field").bounding_box()
                    assert parameter["width"] > 200, parameter
                geometry = await page.locator("#terminal-tool-editor-dialog").bounding_box()
                assert geometry["x"] >= 0 and geometry["x"] + geometry["width"] <= width + 1, geometry
                await page.screenshot(path=f"/tmp/workflow-unification-{width}.png")
                await page.click("#terminal-tool-editor-cancel")
                reference = page.locator('[data-terminal-tool-reference="proxy_settings_workflow"]')
                await reference.click()
                assert await page.locator("#terminal-tool-editor-id").input_value() == ""
                assert await page.locator(".terminal-tool-run-workflow-field select").input_value() == "proxy_settings_workflow"
                await page.click("#terminal-tool-editor-cancel")
            hashes = {}
            for name in ("terminal-settings.js", "app-terminal-tools-settings.js", "terminal-tool-actions.js"):
                response = await context.request.get(BASE + "/assets/" + name + "?v=20260926-workflow-unification")
                assert response.status == 200
                digest = hashlib.sha256(await response.body()).hexdigest()
                assert digest == hashlib.sha256((ROOT / "static" / name).read_bytes()).hexdigest()
                hashes[name] = digest
            assert not blocked, blocked
            assert not errors, errors
            print(json.dumps({"viewports": [1440, 375], "mutations": blocked, "page_errors": errors, "hashes": hashes}, ensure_ascii=False))
        finally:
            await browser.close()

asyncio.run(main())
