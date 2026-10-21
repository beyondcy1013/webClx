import asyncio
import json
import os
from pathlib import Path
from urllib.parse import parse_qs, urlencode, urlparse

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111").rstrip("/")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/third_party/browser-tools/bin/chromium",
)


def query_value(url, key):
    return parse_qs(urlparse(url).query).get(key, [""])[0]


async def sessions(context):
    response = await context.request.get(f"{BASE_URL}/api/terminal/sessions?all=true")
    assert response.ok, await response.text()
    return (await response.json()).get("sessions", [])


async def unused_workspace_path(context, existing_sessions):
    response = await context.request.get(f"{BASE_URL}/api/entries?path=")
    assert response.ok, await response.text()
    entries = (await response.json()).get("entries", [])
    session_paths = {session.get("path") or "" for session in existing_sessions}
    return next(
        entry["path"]
        for entry in entries
        if entry.get("kind") == "dir" and entry.get("path") not in session_paths
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
            before = await sessions(context)
            before_ids = {session["id"] for session in before}
            target_path = await unused_workspace_path(context, before)

            await page.goto(
                f"{BASE_URL}/workspace?{urlencode({'path': target_path})}",
                wait_until="domcontentloaded",
            )
            await page.wait_for_function(
                """
                path => state.currentPath === path
                  && document.querySelector('#sessions-session-list')?.value === ''
                  && new URL(document.querySelector('#top-nav-terminal').href).searchParams.get('path') === path
                """,
                arg=target_path,
                timeout=20_000,
            )

            terminal_link = page.locator("#top-nav-terminal")
            terminal_href = await terminal_link.get_attribute("href")
            assert query_value(terminal_href, "path") == target_path, terminal_href
            assert not query_value(terminal_href, "session"), terminal_href
            assert not query_value(terminal_href, "fresh"), terminal_href
            assert not query_value(terminal_href, "quick_start"), terminal_href

            async with page.expect_response(
                lambda response: response.url.startswith(
                    f"{BASE_URL}/api/terminal/sessions?"
                )
                and response.request.method == "GET"
                and response.status == 200,
                timeout=20_000,
            ):
                await terminal_link.click()
            await page.wait_for_url(f"{BASE_URL}/terminal**", timeout=20_000)
            await page.wait_for_function(
                "() => state.loadingSessions === false && state.sessions.length > 0",
                timeout=20_000,
            )

            after_click = await sessions(context)
            after_click_ids = {session["id"] for session in after_click}
            assert after_click_ids == before_ids, {
                "added": sorted(after_click_ids - before_ids),
                "removed": sorted(before_ids - after_click_ids),
            }

            async with page.expect_response(
                lambda response: response.url.startswith(
                    f"{BASE_URL}/api/terminal/sessions?"
                )
                and response.request.method == "GET"
                and response.status == 200,
                timeout=20_000,
            ):
                await page.goto(
                    f"{BASE_URL}/terminal?{urlencode({'path': target_path})}",
                    wait_until="domcontentloaded",
                )
            await page.wait_for_function(
                "() => state.loadingSessions === false && state.sessions.length > 0",
                timeout=20_000,
            )
            after_direct = await sessions(context)
            after_direct_ids = {session["id"] for session in after_direct}
            assert after_direct_ids == before_ids, {
                "added": sorted(after_direct_ids - before_ids),
                "removed": sorted(before_ids - after_direct_ids),
            }

            assert not page_errors, page_errors
            assert not failed_responses, failed_responses
            print(
                json.dumps(
                    {
                        "path": target_path,
                        "terminal_href": terminal_href,
                        "session_count": len(before_ids),
                        "session_ids_unchanged": True,
                    },
                    ensure_ascii=False,
                    sort_keys=True,
                )
            )
        finally:
            await context.close()
            await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
