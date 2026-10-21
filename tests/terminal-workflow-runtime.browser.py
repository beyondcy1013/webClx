"""Read-only runtime parameter QA: workflow definitions are injected in-page and execution is stubbed."""
import asyncio
import hashlib
import importlib.util
import json
import os
from pathlib import Path
from urllib.parse import urlparse
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
            await context.route_web_socket("**/api/terminal/**", lambda ws: ws.close())
            page = await context.new_page()
            page.on("pageerror", lambda error: errors.append(str(error)))
            for width in (1440, 375):
                await page.set_viewport_size({"width": width, "height": 900})
                await page.goto(BASE + "/terminal", wait_until="networkidle")
                await page.wait_for_function("typeof executeTerminalToolEntry === 'function'")
                await page.evaluate("""() => {
                  state.terminalToolEntries = [
                    {id:'qa-inner',kind:'action',root_key:'tools',label:'内部流程',sort_order:1,actions:[
                      {kind:'codex_exec',value:'处理 {{branch}}：{{任务描述}}',seconds:0}
                    ]},
                    {id:'qa-runtime',kind:'action',root_key:'tools',label:'运行参数流程',sort_order:0,actions:[
                      {kind:'send_command',value:'git checkout {{branch}}',seconds:0},
                      ]}
                  ];
                  window.__workflowRuntimeCaptured = [];
                  executeTerminalToolAction = async (action, context) => {
                    window.__workflowRuntimeCaptured.push({value: action.value, parameters: context.runtimeParameters});
                  };
                  closeTerminalToolMenu = () => {};
                }""")
                await page.click("#terminal-workflows-button")
                await page.click('[data-terminal-tool-entry="qa-runtime"]')
                dialog = page.locator("#terminal-workflow-parameters-dialog")
                await dialog.wait_for(state="visible")
                fields = dialog.locator("[data-terminal-workflow-parameter]")
                assert await fields.count() == 1, await fields.count()
                assert await fields.nth(0).get_attribute("data-terminal-workflow-parameter") == "branch"
                for index, value in enumerate(("release",)):
                    await fields.nth(index).fill(value)
                assert await dialog.locator("button[type=submit]").is_enabled()
                box = await page.locator("#terminal-workflow-parameters-body").bounding_box()
                assert box and box["width"] > 160 and box["x"] >= 0 and box["x"] + box["width"] <= width + 1, box
                await page.evaluate("document.querySelector('#terminal-workflow-parameters-form').requestSubmit()")
                await page.wait_for_function("!document.querySelector('#terminal-workflow-parameters-dialog').open")
                captured = await page.evaluate("window.__workflowRuntimeCaptured")
                assert captured == [
                    {"value": "git checkout release", "parameters": {"branch": "release"}},
                ], captured
            index_html = await (await context.request.get(BASE + "/")).text()
            assert "{{参数名}}" in index_html
            hashes = {}
            for name in ("terminal-tool-actions.js", "terminal.js", "styles-terminal.css"):
                response = await context.request.get(BASE + "/assets/" + name + "?v=20260926-workflow-history")
                assert response.status == 200
                digest = hashlib.sha256(await response.body()).hexdigest()
                assert digest == hashlib.sha256((ROOT / "static" / name).read_bytes()).hexdigest()
                hashes[name] = digest
            terminal_html = await (await context.request.get(BASE + "/terminal")).text()
            assert "terminal-workflow-parameters-dialog" in terminal_html and "workflow-history" in terminal_html
            assert not blocked, blocked
            assert not errors, errors
            print(json.dumps({"viewports": [1440, 375], "mutations": blocked, "page_errors": errors, "hashes": hashes}, ensure_ascii=False))
        finally:
            await browser.close()

asyncio.run(main())
