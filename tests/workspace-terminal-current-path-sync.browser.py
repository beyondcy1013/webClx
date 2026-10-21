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


async def create_fixture(context, path, marker):
    response = await context.request.post(
        f"{BASE_URL}/api/terminal/sessions",
        data={"path": path},
    )
    assert response.ok, await response.text()
    created = await response.json()
    fixture = {
        "id": created["id"],
        "path": created.get("path") or "",
        "name": created.get("name") or "",
    }
    assert fixture["path"] == path, fixture

    rename_response = await context.request.put(
        f"{BASE_URL}/api/terminal/sessions/{fixture['id']}",
        data={"name": marker},
    )
    assert rename_response.ok, await rename_response.text()
    renamed = await rename_response.json()
    assert renamed.get("id") == fixture["id"], renamed
    assert renamed.get("name") == marker, renamed
    fixture["name"] = marker
    return fixture


async def cleanup_fixture(context, fixture):
    if not fixture:
        return
    response = await context.request.get(f"{BASE_URL}/api/terminal/sessions?all=true")
    assert response.ok, await response.text()
    sessions = (await response.json()).get("sessions", [])
    matched = next(
        (
            session
            for session in sessions
            if session.get("id") == fixture["id"]
            and (session.get("path") or "") == fixture["path"]
            and (session.get("name") or "") == fixture["name"]
        ),
        None,
    )
    assert matched is not None, f"browser QA fixture identity changed: {fixture}"
    delete_response = await context.request.delete(
        f"{BASE_URL}/api/terminal/sessions/{fixture['id']}",
        headers={
            "X-WebClx-Confirm-Session": fixture["id"],
            "X-WebClx-Delete-Source": "browser-qa",
        },
    )
    assert delete_response.ok, await delete_response.text()


async def main():
    chromium_path = Path(CHROMIUM)
    launch_options = {"headless": True, "args": ["--no-sandbox", "--disable-gpu"]}
    if chromium_path.is_file():
        launch_options["executable_path"] = str(chromium_path)

    marker = f"workspace-sync-qa-{int(time.time())}"
    fixtures = []
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(**launch_options)
        context = await browser.new_context(viewport={"width": 1440, "height": 1000})
        page = await context.new_page()
        page_errors = []
        console_errors = []
        failed_responses = []
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        page.on(
            "console",
            lambda message: console_errors.append(message.text)
            if message.type == "error" and not message.text.startswith("Failed to load resource:")
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

        try:
            for path in ("webClx", "newapi"):
                directory_response = await context.request.get(
                    f"{BASE_URL}/api/entries?{urlencode({'path': path})}"
                )
                assert directory_response.ok, await directory_response.text()

            newapi_fixture = await create_fixture(context, "newapi", f"{marker}-newapi")
            fixtures.append(newapi_fixture)
            webclx_fixture = await create_fixture(context, "webClx", f"{marker}-webclx")
            fixtures.append(webclx_fixture)

            await page.goto(
                f"{BASE_URL}/workspace?{urlencode({'path': 'webClx', 'terminal_session': webclx_fixture['id']})}",
                wait_until="domcontentloaded",
            )
            await page.wait_for_function(
                """
                fixture => state.currentPath === fixture.path
                  && document.querySelector('#directory-session-list')?.value === fixture.id
                  && document.querySelector('#sessions-session-list')?.value === fixture.id
                """,
                arg=webclx_fixture,
                timeout=20_000,
            )

            await page.evaluate(
                """
                fixtures => {
                  for (const fixture of fixtures) {
                    WebClxTerminalSessionStorage.storeSessionId(fixture.path, fixture.id);
                  }
                }
                """,
                fixtures,
            )

            await page.locator("#sessions-session-list").select_option(newapi_fixture["id"])
            await page.wait_for_function(
                """
                fixture => state.currentPath === fixture.path
                  && document.querySelector('#directory-session-list')?.value === fixture.id
                  && document.querySelector('#sessions-session-list')?.value === fixture.id
                """,
                arg=newapi_fixture,
                timeout=20_000,
            )
            assert "path=newapi" in page.url, page.url
            assert f"terminal_session={newapi_fixture['id']}" in page.url, page.url

            await page.locator("#entry-list .entry-link.dir", has_text="..").click()
            await page.wait_for_function("state.currentPath === ''", timeout=20_000)
            await page.locator("#entry-list").get_by_role(
                "link", name="webClx", exact=True
            ).click()
            await page.wait_for_function(
                """
                fixture => state.currentPath === fixture.path
                  && document.querySelector('#directory-session-list')?.value === fixture.id
                  && document.querySelector('#sessions-session-list')?.value === fixture.id
                """,
                arg=webclx_fixture,
                timeout=20_000,
            )
            assert "path=webClx" in page.url, page.url
            assert f"terminal_session={webclx_fixture['id']}" in page.url, page.url

            screenshot = "/tmp/webclx-workspace-terminal-current-path-sync.png"
            await page.screenshot(path=screenshot, full_page=True)
            assert not page_errors, page_errors
            assert not console_errors, console_errors
            assert not failed_responses, failed_responses
            print(
                json.dumps(
                    {
                        "ok": True,
                        "webclx_session": webclx_fixture["id"],
                        "newapi_session": newapi_fixture["id"],
                        "final_url": page.url,
                        "screenshot": screenshot,
                    },
                    sort_keys=True,
                )
            )
        finally:
            for fixture in reversed(fixtures):
                await cleanup_fixture(context, fixture)
            await context.close()
            await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
