import asyncio
import json
import os
import time
from pathlib import Path
from urllib.parse import urlencode

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111").rstrip("/")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/third_party/browser-tools/bin/chromium",
)


async def wait_for_current_directory(context, session_id, expected_path):
    last_result = None
    for _ in range(100):
        response = await context.request.get(
            f"{BASE_URL}/api/terminal/sessions/{session_id}/current-directory"
        )
        if response.ok:
            last_result = await response.json()
            if last_result.get("display_path") == expected_path:
                return last_result
        else:
            last_result = {"status": response.status, "body": await response.text()}
        await asyncio.sleep(0.1)
    raise AssertionError(
        json.dumps(
            {"expected_path": expected_path, "last_result": last_result},
            ensure_ascii=False,
        )
    )


async def main():
    launch_options = {"headless": True, "args": ["--no-sandbox", "--disable-gpu"]}
    chromium_path = Path(CHROMIUM)
    if chromium_path.is_file():
        launch_options["executable_path"] = str(chromium_path)

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(**launch_options)
        context = await browser.new_context(viewport={"width": 1280, "height": 900})
        page = await context.new_page()
        created_session_id = None
        created_session_path = None
        marker = f"browser_live_cwd_{int(time.time() * 1000)}"
        page_errors = []
        failed_responses = []
        page.on("pageerror", lambda error: page_errors.append(str(error)))
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

        try:
            target_response = await context.request.get(
                f"{BASE_URL}/api/entries?path=webClx/static"
            )
            assert target_response.ok, target_response.status
            target_directory = await target_response.json()
            target_path = target_directory["display_path"]

            create_response = await context.request.post(
                f"{BASE_URL}/api/terminal/sessions",
                data={"path": "webClx"},
            )
            assert create_response.ok, await create_response.text()
            created = await create_response.json()
            created_session_id = created["id"]
            created_session_path = created.get("path") or ""
            assert created_session_path == "webClx", created

            rename_response = await context.request.put(
                f"{BASE_URL}/api/terminal/sessions/{created_session_id}",
                data={"name": marker},
            )
            assert rename_response.ok, await rename_response.text()
            renamed = await rename_response.json()
            assert renamed.get("id") == created_session_id, renamed
            assert renamed.get("name") == marker, renamed

            terminal_url = f"{BASE_URL}/terminal?{urlencode({'path': created_session_path, 'session': created_session_id})}"
            await page.goto(terminal_url, wait_until="domcontentloaded")
            await page.wait_for_function(
                "sessionId => document.querySelector('#session-switcher')?.value === sessionId",
                arg=created_session_id,
                timeout=15_000,
            )

            input_response = await context.request.post(
                f"{BASE_URL}/api/terminal/sessions/{created_session_id}/input",
                data={"data": f"cd {target_path}\r"},
            )
            assert input_response.ok, await input_response.text()
            live_directory = await wait_for_current_directory(
                context,
                created_session_id,
                target_path,
            )

            history_link = page.locator("#top-nav-workspace-history")
            history_href = await history_link.get_attribute("href")
            assert f"path={created_session_path}" in history_href, history_href
            assert target_path not in history_href, history_href
            await history_link.click()
            await page.wait_for_url(f"{BASE_URL}/workspace_history**", timeout=15_000)

            path_select = page.locator("#workspace-history-path-select")
            await path_select.wait_for(state="visible", timeout=15_000)
            await page.wait_for_function(
                "expected => { const select = document.querySelector('#workspace-history-path-select'); return select && !select.disabled && select.value === expected && select.options[0]?.value === expected; }",
                arg=target_path,
                timeout=15_000,
            )
            selected = await path_select.evaluate(
                "select => ({ selected: select.value, first: select.options[0]?.value || '', options: Array.from(select.options, option => option.value) })"
            )
            assert selected["selected"] == target_path, selected
            assert selected["first"] == target_path, selected
            assert not page_errors, page_errors
            assert not failed_responses, failed_responses

            print(
                json.dumps(
                    {
                        "session_id": created_session_id,
                        "registered_path": created_session_path,
                        "live_directory": live_directory,
                        "history_href": history_href,
                        "history_selection": selected,
                    },
                    ensure_ascii=False,
                    sort_keys=True,
                )
            )
        finally:
            await page.close()
            if created_session_id is not None:
                sessions_response = await context.request.get(
                    f"{BASE_URL}/api/terminal/sessions?all=true"
                )
                if sessions_response.ok:
                    sessions = (await sessions_response.json()).get("sessions", [])
                    fixture = next(
                        (session for session in sessions if session.get("id") == created_session_id),
                        None,
                    )
                    if (
                        fixture
                        and fixture.get("name") == marker
                        and (fixture.get("path") or "") == created_session_path
                    ):
                        delete_response = await context.request.delete(
                            f"{BASE_URL}/api/terminal/sessions/{created_session_id}",
                            headers={
                                "X-WebClx-Confirm-Session": created_session_id,
                                "X-WebClx-Delete-Source": "browser-qa",
                            },
                        )
                        assert delete_response.ok, await delete_response.text()
            await context.close()
            await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
