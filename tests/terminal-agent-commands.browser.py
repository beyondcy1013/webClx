import asyncio, json, os
from pathlib import Path
from playwright.async_api import async_playwright

async def main():
  async with async_playwright() as p:
    browser = await p.chromium.launch(
      executable_path=os.environ.get(
        "WEBCLX_TEST_CHROMIUM",
        "/home/codes/third_party/browser-tools/bin/chromium",
      ),
      headless=True,
      args=["--no-sandbox"],
    )
    page = await browser.new_page(
      extra_http_headers={
        "X-WebClx-Local-Token": Path("/home/bin/webclx/.webclx-local-api-token").read_text().strip()
      }
    )
    import importlib.util
    spec = importlib.util.spec_from_file_location(
      "viewport", str(Path(__file__).with_name("terminal-shared-viewport.browser.py"))
    )
    helper = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(helper)
    await page.context.add_cookies([helper.local_session_cookie()])
    await page.goto("http://127.0.0.1:11111/terminal")
    await page.wait_for_function(
      'typeof openTerminalSpecifiedTaskDialog === "function" && !state.loadingSessions'
    )
    await page.evaluate('openTerminalSpecifiedTaskDialog(null, { temporary: true })')
    await page.wait_for_selector("#terminal-specified-task-dialog[open]")
    await page.wait_for_selector('#terminal-specified-task-agent-list input[value="devin"]')
    default_agents = await page.eval_on_selector_all(
      "#terminal-specified-task-agent-list label",
      "nodes => nodes.map(node => node.innerText.trim())",
    )
    title_gap = await page.evaluate(
      """() => {
        const title = document.querySelector(".terminal-specified-task-agent-modes .terminal-specified-task-group-title");
        const first = document.querySelector(".terminal-specified-task-agent-list label");
        const a = title.getBoundingClientRect();
        const b = first.getBoundingClientRect();
        return {left: a.right, right: b.left, gap: b.left - a.right};
      }"""
    )
    await page.evaluate(
      """() => terminalSpecifiedTaskSetAgentCommands([
        {key: 'custom', label: 'Custom CLI', new_command: 'custom-agent', resume_command: 'custom-agent --resume {session_id}'},
        {key: 'other', label: '其它', new_command: '', resume_command: ''},
      ])"""
    )
    await page.wait_for_selector('#terminal-specified-task-agent-list input[value="custom"]')
    await page.locator('#terminal-specified-task-agent-list label', has_text="Custom CLI").click()
    await page.check('input[name="terminal-specified-task-session-action"][value="resume"]', force=True)
    session_id = "019d1ba6-f772-7452-a391-6553ccbc0a50"
    await page.evaluate("(value) => { document.querySelector('#terminal-specified-task-session-id').value = value; }", session_id)
    command = await page.evaluate('terminalSpecifiedTaskAgentLaunchCommand("resume", document.querySelector("#terminal-specified-task-session-id").value)')
    assert command == f"custom-agent --resume '{session_id}'", command
    print(json.dumps({"default_agents": default_agents, "title_gap": title_gap, "command": command}, ensure_ascii=False))
    await page.screenshot(path="/tmp/webclx-agent-commands-verified.png")
    await browser.close()

asyncio.run(main())
