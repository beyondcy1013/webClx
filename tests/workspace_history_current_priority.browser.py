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


async def main():
    launch_options = {"headless": True, "args": ["--no-sandbox", "--disable-gpu"]}
    chromium_path = Path(CHROMIUM)
    if chromium_path.is_file():
        launch_options["executable_path"] = str(chromium_path)

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(**launch_options)
        context = await browser.new_context(viewport={"width": 1280, "height": 900})
        settings_response = await context.request.get(f"{BASE_URL}/api/settings")
        entries_response = await context.request.get(f"{BASE_URL}/api/entries?path=webClx")
        assert settings_response.ok, settings_response.status
        assert entries_response.ok, entries_response.status
        settings = await settings_response.json()
        directory = await entries_response.json()
        current_path = directory["display_path"]
        probe_path = f"{str(settings['workspace_dir']).rstrip('/')}/__webclx_priority_probe__"

        async def route_settings(route):
            if route.request.method != "GET":
                await route.continue_()
                return
            response = await route.fetch()
            payload = await response.json()
            payload["workspace_history"] = [
                {"path": probe_path, "last_opened_at": 9_999_999_999_999},
                {"path": current_path, "last_opened_at": 1},
            ]
            await route.fulfill(response=response, json=payload)

        await context.route("**/api/settings", route_settings)

        async def verify(page):
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
            await page.goto(f"{BASE_URL}/workspace?path=webClx", wait_until="domcontentloaded")
            await page.locator("#tab-workspace-history").click()
            await page.wait_for_url(f"{BASE_URL}/workspace_history**", timeout=15_000)

            path_select = page.locator("#workspace-history-path-select")
            await path_select.wait_for(state="visible", timeout=15_000)
            await page.wait_for_function(
                "expected => { const select = document.querySelector('#workspace-history-path-select'); return select && !select.disabled && select.value === expected && select.options[0]?.value === expected; }",
                arg=current_path,
                timeout=15_000,
            )
            result = await path_select.evaluate(
                "select => ({ selected: select.value, first: select.options[0]?.value || '', options: Array.from(select.options, option => option.value) })"
            )
            assert result["selected"] == current_path, result
            assert result["first"] == current_path, result
            assert probe_path in result["options"], result
            assert not page_errors, page_errors
            assert not failed_responses, failed_responses
            return result

        try:
            page = await context.new_page()
            switched = await verify(page)
            await page.close()

            print(
                json.dumps(
                    {
                        "current_path": current_path,
                        "probe_path": probe_path,
                        "switched": switched,
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
