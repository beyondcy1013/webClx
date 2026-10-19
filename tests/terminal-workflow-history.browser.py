"""Read-only workflow history QA: execution and API mutations are stubbed or blocked."""
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
            blocked = []
            async def readonly(route):
                if route.request.url.endswith("/api/terminal/auto-typed-input") and route.request.method == "POST":
                    if page and await page.evaluate("window.__historyFailOnce === true"):
                        await page.evaluate("window.__historyFailOnce = false")
                        await route.abort()
                    else:
                        await route.fulfill(status=200, content_type="application/json", body='{"ok":true}')
                    return
                if route.request.method not in ("GET", "HEAD", "OPTIONS"):
                    blocked.append(route.request.url)
                    await route.abort()
                else:
                    await route.continue_()
            await context.route("**/api/**", readonly)
            await context.route_web_socket("**/api/terminal/**", lambda ws: ws.close())
            page = await context.new_page()
            errors = []
            page.on("pageerror", lambda error: errors.append(str(error)))
            for width in (1440, 375):
                await page.set_viewport_size({"width": width, "height": 900})
                await page.goto(BASE + "/terminal", wait_until="networkidle")
                await page.wait_for_function("typeof executeTerminalToolEntry === 'function'")
                await page.evaluate("""() => {
                  window.localStorage.removeItem('webclx:workflow-runs');
                  state.terminalToolEntries = [
                    {id:'qa-history',kind:'action',root_key:'tools',label:'历史流程',sort_order:1,actions:[
                      {kind:'send_command',value:'echo first',seconds:0},
                      {kind:'send_command',value:'echo second',seconds:0}
                    ]}
                  ];
                  window.__historyRuns = [];
                  window.__historyExecuteAction = executeTerminalToolAction;
                  executeTerminalToolAction = async (action, context) => {
                    if (action.value === 'echo second' && !window.__historyRecovered) {
                    window.__historyRecovered = true;
                    throw new Error('fixture failure');
                  }
                    window.__historyRuns.push({value: action.value, entry: context.workflowEntryId});
                  };
                }""")
                await page.click("#terminal-workflows-button")
                await page.evaluate("""async () => {
                  const entry = state.terminalToolEntries[0];
                  const plan = WebClxTerminalSettings.inspectTerminalWorkflow(entry, state.terminalToolEntries);
                  const action = plan.steps[0];
                  terminalToolFailedExecution = {
                    runId: 'qa_browser_failed', entryId: entry.id,
                    definitionStamp: terminalWorkflowDefinitionStamp(entry, plan.steps),
                    nextIndex: 0, actions: plan.steps,
                    executionContext: { workflowStack: [entry.id], runtimeParameters: {}, sessionId: "qa-source" },
                    runtimeParameters: {}, startedAt: Date.now(),
                  };
                  recordTerminalWorkflowRun({
                    id: 'qa_browser_failed', entryId: entry.id, entryLabel: entry.label,
                    status: 'failed', progress: '1/2', startedAt: Date.now(), finishedAt: Date.now(),
                    definitionStamp: terminalToolFailedExecution.definitionStamp,
                  });
                }""")
                await page.click("#terminal-workflows-button")
                await page.wait_for_selector(".terminal-tool-menu-history-failed")
                assert await page.locator('[data-terminal-workflow-run-action="retry"]').count() == 1
                assert await page.locator('[data-terminal-workflow-run-action="restart"]').count() == 1
                stored = await page.evaluate("JSON.parse(localStorage.getItem('webclx:workflow-runs'))")
                assert len(stored) == 1 and stored[0]["status"] == "failed", stored
                assert "echo first" not in json.dumps(stored) and "echo second" not in json.dumps(stored)
                box = await page.locator(".terminal-tool-menu-history").first.bounding_box()
                assert box and box["x"] >= 0 and box["x"] + box["width"] <= width + 1, box
                await page.click('[data-terminal-workflow-run-action="clear"]')
                assert await page.locator(".terminal-tool-menu-history").count() == 0
            hashes = {}
            for name in ("terminal-settings.js", "terminal-tool-actions.js", "styles-terminal.css"):
                response = await context.request.get(BASE + "/assets/" + name + "?v=20260926-workflow-history")
                assert response.status == 200
                digest = hashlib.sha256(await response.body()).hexdigest()
                assert digest == hashlib.sha256((ROOT / "static" / name).read_bytes()).hexdigest(), name
                hashes[name] = digest
            terminal_html = await (await context.request.get(BASE + "/terminal")).text()
            assert "workflow-history" in terminal_html and "terminal-tool-search" in terminal_html
            assert not blocked, blocked
            assert not errors, errors
            print(json.dumps({"viewports": [1440, 375], "mutations": blocked, "page_errors": errors, "hashes": hashes}, ensure_ascii=False))
        finally:
            await browser.close()

asyncio.run(main())
