import asyncio
import json
import os
from pathlib import Path

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111").rstrip("/")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/third_party/browser-tools/bin/chromium",
)
WORKFLOW_ID = "proxy_settings_workflow"
EXPECTED_PATH = "../system"


async def list_sessions(context):
    response = await context.request.get(f"{BASE_URL}/api/terminal/sessions?all=true")
    assert response.ok, await response.text()
    return (await response.json()).get("sessions", [])


async def cleanup_created_session(context, created_session_id):
    if not created_session_id:
        return
    session = next(
        (
            item
            for item in await list_sessions(context)
            if item.get("id") == created_session_id
        ),
        None,
    )
    if session is None:
        return
    assert session.get("origin") == "agent", session
    assert session.get("owner_key") == WORKFLOW_ID, session
    assert session.get("path") == EXPECTED_PATH, session
    response = await context.request.delete(
        f"{BASE_URL}/api/terminal/sessions/{created_session_id}",
        headers={
            "X-WebClx-Confirm-Session": created_session_id,
            "X-WebClx-Delete-Source": "browser-qa",
        },
    )
    assert response.ok, await response.text()


async def run_workflow(page, expect_creation):
    await page.click("#terminal-workflows-button")
    workflow_button = page.locator(
        f'[data-terminal-tool-entry="{WORKFLOW_ID}"]'
    )
    await workflow_button.wait_for(state="visible")

    if expect_creation:
        async with page.expect_response(
            lambda response: response.url == f"{BASE_URL}/api/terminal/sessions"
            and response.request.method == "POST"
            and response.status == 200,
            timeout=20_000,
        ) as response_info:
            await workflow_button.click()
        created = await (await response_info.value).json()
    else:
        created = None
        await workflow_button.click()

    await page.wait_for_function(
        """
        () => !terminalToolExecutionRunning
          && document.querySelector('#terminal-tool-menu-status')
            ?.textContent.includes('\u6267\u884c\u5b8c\u6210')
        """,
        timeout=30_000,
    )
    return created


async def main():
    launch_options = {"headless": True, "args": ["--no-sandbox", "--disable-gpu"]}
    chromium_path = Path(CHROMIUM)
    if chromium_path.is_file():
        launch_options["executable_path"] = str(chromium_path)

    created_session_id = None
    results = {}
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(**launch_options)
        context = await browser.new_context(viewport={"width": 1440, "height": 900})
        page = await context.new_page()
        page_errors = []
        console_errors = []
        failed_responses = []
        session_posts = []
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        page.on(
            "console",
            lambda message: console_errors.append(message.text)
            if message.type == "error"
            and message.text
            != "Failed to load resource: the server responded with a status of 404 (Not Found)"
            else None,
        )
        page.on(
            "response",
            lambda response: failed_responses.append(
                {"status": response.status, "url": response.url}
            )
            if response.status >= 400
            and response.url.startswith(BASE_URL)
            and not (
                response.status == 404
                and response.url.startswith(f"{BASE_URL}/api/workspace-icon?")
            )
            else None,
        )
        page.on(
            "request",
            lambda request: session_posts.append(request.url)
            if request.method == "POST"
            and request.url == f"{BASE_URL}/api/terminal/sessions"
            else None,
        )

        try:
            before = await list_sessions(context)
            existing = [
                session
                for session in before
                if session.get("origin") == "agent"
                and session.get("owner_key") == WORKFLOW_ID
                and not session.get("idle")
            ]
            assert not existing, {
                "message": "live QA must not take over or delete an existing agent",
                "sessions": existing,
            }

            await page.goto(f"{BASE_URL}/terminal", wait_until="domcontentloaded")
            await page.wait_for_function(
                """
                () => !state.loadingSessions
                  && Boolean(state.activeSessionId)
                  && state.terminalToolEntries.some(entry => entry.id === 'proxy_settings_workflow')
                """,
                timeout=20_000,
            )

            created = await run_workflow(page, expect_creation=True)
            created_session_id = created["id"]
            assert created.get("origin") == "agent", created
            assert created.get("owner_key") == WORKFLOW_ID, created
            assert created.get("path") == EXPECTED_PATH, created

            await page.wait_for_function(
                """
                sessionId => {
                  const context = ensureTerminalSessionCache().get(sessionId);
                  if (!context?.term) return false;
                  const text = readTerminalBufferTailTextFrom(context.term, 120);
                  return text.includes('$mihomo-proxy-ops')
                    && text.includes('\u4ec5\u52a0\u8f7d\u4e0a\u8ff0\u6280\u80fd\u53ca\u5fc5\u8981\u4e0a\u4e0b\u6587')
                    && !text.includes('\u8bf7\u68c0\u67e5\u5f53\u524d\u4ee3\u7406\u914d\u7f6e');
                }
                """,
                arg=created_session_id,
                timeout=30_000,
            )
            first_sessions = await list_sessions(context)
            first_agent = next(
                session
                for session in first_sessions
                if session.get("id") == created_session_id
            )
            assert first_agent.get("codex_api_preset_name") == "MiniMax3", first_agent

            await run_workflow(page, expect_creation=False)
            await page.wait_for_timeout(500)
            second_sessions = await list_sessions(context)
            owned_agents = [
                session
                for session in second_sessions
                if session.get("origin") == "agent"
                and session.get("owner_key") == WORKFLOW_ID
                and not session.get("idle")
            ]
            assert [session["id"] for session in owned_agents] == [created_session_id], owned_agents
            assert len(session_posts) == 1, session_posts
            assert await page.locator("#agent-session-switcher").count() == 0
            assert await page.evaluate("state.activeSessionId") == created_session_id
            assert await page.locator("#session-switcher").input_value() != created_session_id
            results = {
                "session_id": created_session_id,
                "session_name": first_agent.get("name"),
                "path": first_agent.get("path"),
                "preset": first_agent.get("codex_api_preset_name"),
                "owner_key": first_agent.get("owner_key"),
                "session_post_count_after_two_runs": len(session_posts),
                "reused_same_session": True,
            }
            await page.screenshot(
                path="/tmp/webclx-proxy-workflow-live.png",
                full_page=True,
            )
            assert not page_errors, page_errors
            assert not console_errors, console_errors
            assert not failed_responses, failed_responses
            print(json.dumps(results, ensure_ascii=False, sort_keys=True))
        finally:
            await cleanup_created_session(context, created_session_id)
            await context.close()
            await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
