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
            sessions_response = await context.request.get(
                f"{BASE_URL}/api/terminal/sessions?all=true"
            )
            assert sessions_response.ok, sessions_response.status
            sessions_payload = await sessions_response.json()
            sessions = [session for session in sessions_payload.get("sessions", []) if not session.get("idle")]
            assert sessions, "browser regression requires at least one active terminal session"
            target = sessions[-1]
            target_id = target["id"]
            target_path = target.get("path") or ""

            terminal_url = f"{BASE_URL}/terminal?{urlencode({'path': target_path, 'session': target_id})}"
            await page.goto(terminal_url, wait_until="domcontentloaded")
            await page.locator("#session-switcher").wait_for(state="visible", timeout=15_000)
            await page.wait_for_function(
                "sessionId => document.querySelector('#session-switcher')?.value === sessionId",
                arg=target_id,
                timeout=15_000,
            )

            workspace_link = page.locator("#top-nav-workspace")
            workspace_href = await workspace_link.get_attribute("href")
            assert query_value(workspace_href, "terminal_session") == target_id, workspace_href
            await workspace_link.click()
            await page.wait_for_url(f"{BASE_URL}/workspace**", timeout=15_000)
            assert query_value(page.url, "terminal_session") == target_id, page.url

            terminal_link = page.locator("#top-nav-terminal")
            await terminal_link.wait_for(state="visible", timeout=15_000)
            await page.wait_for_function(
                "sessionId => new URL(document.querySelector('#top-nav-terminal').href).searchParams.get('session') === sessionId",
                arg=target_id,
                timeout=15_000,
            )
            terminal_href = await terminal_link.get_attribute("href")
            assert query_value(terminal_href, "session") == target_id, terminal_href

            await terminal_link.click()
            await page.wait_for_url(f"{BASE_URL}/terminal**", timeout=15_000)
            assert query_value(page.url, "session") == target_id, page.url
            await page.wait_for_function(
                "sessionId => document.querySelector('#session-switcher')?.value === sessionId",
                arg=target_id,
                timeout=15_000,
            )

            assert not page_errors, page_errors
            assert not failed_responses, failed_responses
            print(
                json.dumps(
                    {
                        "session_id": target_id,
                        "session_name": target.get("name") or "",
                        "session_path": target_path,
                        "workspace_href": workspace_href,
                        "terminal_href": terminal_href,
                        "final_url": page.url,
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
