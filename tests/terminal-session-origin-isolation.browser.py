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
TEST_PATH = "webClx"
TEST_OWNER_KEY = "browser-qa-terminal-origin-isolation"


async def create_session(context, payload, created_sessions):
    response = await context.request.post(
        f"{BASE_URL}/api/terminal/sessions",
        data=payload,
    )
    assert response.ok, await response.text()
    session = await response.json()
    assert session["path"] == TEST_PATH, session
    created_sessions[session["id"]] = {
        "path": TEST_PATH,
        "origin": payload["origin"],
        "owner_key": payload["owner_key"],
    }
    return session


async def cleanup_created_sessions(context, created_sessions):
    listing = await context.request.get(f"{BASE_URL}/api/terminal/sessions?all=true")
    assert listing.ok, await listing.text()
    sessions_by_id = {
        session["id"]: session for session in (await listing.json()).get("sessions", [])
    }

    cleanup_errors = []
    for session_id, expected in created_sessions.items():
        session = sessions_by_id.get(session_id)
        if session is None:
            continue
        if (
            session.get("path") != expected["path"]
            or session.get("origin") != expected["origin"]
            or session.get("owner_key", "") != expected["owner_key"]
        ):
            cleanup_errors.append({"id": session_id, "actual": session, "expected": expected})
            continue
        response = await context.request.delete(
            f"{BASE_URL}/api/terminal/sessions/{session_id}",
            headers={
                "X-WebClx-Confirm-Session": session_id,
                "X-WebClx-Delete-Source": "browser-qa",
            },
        )
        if not response.ok:
            cleanup_errors.append({"id": session_id, "error": await response.text()})

    assert not cleanup_errors, cleanup_errors


async def main():
    launch_options = {"headless": True, "args": ["--no-sandbox", "--disable-gpu"]}
    chromium_path = Path(CHROMIUM)
    if chromium_path.is_file():
        launch_options["executable_path"] = str(chromium_path)

    created_sessions = {}
    results = {}
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(**launch_options)
        context = await browser.new_context(viewport={"width": 1440, "height": 900})
        await context.route(
            "**/api/workspace-icon?*",
            lambda route: route.fulfill(
                status=200,
                content_type="image/svg+xml",
                body='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"/>',
            ),
        )
        page = await context.new_page()
        page_errors = []
        console_errors = []
        failed_responses = []
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        page.on(
            "console",
            lambda message: console_errors.append(message.text)
            if message.type == "error"
            else None,
        )
        page.on(
            "response",
            lambda response: failed_responses.append(
                {"status": response.status, "url": response.url}
            )
            if response.status >= 400 and response.url.startswith(BASE_URL)
            else None,
        )

        try:
            normal = await create_session(
                context,
                {"path": TEST_PATH, "origin": "normal", "owner_key": ""},
                created_sessions,
            )
            agent = await create_session(
                context,
                {"path": TEST_PATH, "origin": "agent", "owner_key": TEST_OWNER_KEY},
                created_sessions,
            )

            await page.goto(f"{BASE_URL}/terminal", wait_until="domcontentloaded")
            await page.wait_for_function(
                """
                ids => !state.loadingSessions
                  && ids.every(id => state.sessions.some(session => session.id === id))
                  && typeof terminalWorkflowStandbyPrompt === 'function'
                  && state.terminalToolEntries.some(entry => entry.id === 'proxy_settings_workflow')
                """,
                arg=[normal["id"], agent["id"]],
                timeout=20_000,
            )

            results["desktop"] = await page.evaluate(
                """
                ids => {
                  const primaryOptions = Array.from(
                    document.querySelector('#session-switcher').options,
                    option => ({ value: option.value, text: option.textContent }),
                  );
                  const primaryIds = primaryOptions.map(option => option.value).filter(Boolean);
                  const agentSelectorPresent = Boolean(document.querySelector('#agent-session-switcher'));
                  const workflow = state.terminalToolEntries.find(
                    entry => entry.id === 'proxy_settings_workflow',
                  );
                  const prompt = terminalWorkflowStandbyPrompt(workflow.actions[0].value);
                  return {
                    primaryIds,
                    agentSelectorPresent,
                    normalFixtureVisible: primaryIds.includes(ids.normal),
                    agentFixtureVisible: primaryIds.includes(ids.agent),
                    coverHidden: !primaryOptions.some(option => option.text === '终端列表'),
                    prompt,
                    pageOverflow: document.documentElement.scrollWidth > innerWidth,
                  };
                }
                """,
                arg={"normal": normal["id"], "agent": agent["id"]},
            )
            assert results["desktop"]["normalFixtureVisible"], results
            assert results["desktop"]["agentFixtureVisible"], results
            assert results["desktop"]["coverHidden"], results
            assert not results["desktop"]["agentSelectorPresent"], results
            assert not results["desktop"]["pageOverflow"], results
            await page.select_option("#session-switcher", agent["id"])
            await page.wait_for_function(
                "sessionId => state.activeSessionId === sessionId"
                " && document.querySelector('#session-switcher')?.value === sessionId",
                arg=agent["id"],
                timeout=20_000,
            )
            results["desktop"]["selectedAgent"] = await page.locator(
                "#session-switcher"
            ).evaluate(
                "select => ({ id: select.value, text: select.selectedOptions[0]?.textContent || '' })"
            )
            assert results["desktop"]["selectedAgent"]["id"] == agent["id"], results
            expected_prompt = (
                "$mihomo-proxy-ops\n\n"
                "\u8bf7\u4ec5\u52a0\u8f7d\u4e0a\u8ff0\u6280\u80fd\u53ca\u5fc5\u8981\u4e0a\u4e0b\u6587\uff0c"
                "\u7136\u540e\u5f85\u547d\u7b49\u5f85\u7528\u6237\u8fdb\u4e00\u6b65\u6307\u4ee4\u3002"
                "\u4e0d\u8981\u4e3b\u52a8\u68c0\u67e5\u3001\u4fee\u6539\u6216\u6267\u884c\u4efb\u4f55\u5de5\u4f5c\u3002"
            )
            assert results["desktop"]["prompt"] == expected_prompt, results
            await page.screenshot(
                path="/tmp/webclx-terminal-origin-isolation-1440.png",
                full_page=True,
            )

            await page.set_viewport_size({"width": 375, "height": 812})
            results["mobile"] = await page.evaluate(
                """
                () => {
                  return {
                    pageOverflow: document.documentElement.scrollWidth > innerWidth,
                    normalSelectorVisible: Boolean(document.querySelector('#session-switcher')),
                    agentSelectorPresent: Boolean(document.querySelector('#agent-session-switcher')),
                  };
                }
                """
            )
            assert not results["mobile"]["pageOverflow"], results
            assert results["mobile"]["normalSelectorVisible"], results
            assert not results["mobile"]["agentSelectorPresent"], results
            await page.screenshot(
                path="/tmp/webclx-terminal-origin-isolation-375.png",
                full_page=True,
            )

            assert not failed_responses, failed_responses
            assert not page_errors, page_errors
            assert not console_errors, console_errors
            print(json.dumps(results, ensure_ascii=False, sort_keys=True))
        finally:
            await cleanup_created_sessions(context, created_sessions)
            await context.close()
            await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
