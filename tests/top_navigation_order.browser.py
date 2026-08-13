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


async def inspect_navigation(browser, label, path, viewport, is_mobile=False):
    context = await browser.new_context(
        viewport=viewport,
        is_mobile=is_mobile,
        has_touch=is_mobile,
    )
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
        await page.goto(f"{BASE_URL}{path}", wait_until="domcontentloaded")
        navigation = page.locator(".page-tabs").first
        await navigation.wait_for(state="visible", timeout=15_000)
        labels = await navigation.locator(":scope > .tab-button").all_inner_texts()
        labels = [" ".join(label.split()) for label in labels]
        assert labels[0] == "终端管理", labels
        assert labels[-3:] == ["Agent", "Codex_OAuth", "归档列表"], labels

        layout = await navigation.evaluate(
            """
            element => ({
              clientWidth: element.clientWidth,
              scrollWidth: element.scrollWidth,
              overflowX: getComputedStyle(element).overflowX,
            })
            """
        )
        if is_mobile and layout["scrollWidth"] > layout["clientWidth"]:
            assert layout["overflowX"] in ("auto", "scroll"), layout

        screenshot = None
        if is_mobile:
            await navigation.evaluate("element => { element.scrollLeft = element.scrollWidth; }")
            screenshot = f"/tmp/webclx-top-navigation-{label}-mobile.png"
            await page.screenshot(path=screenshot)

        return {
            "labels": labels,
            "layout": layout,
            "page_errors": page_errors,
            "failed_responses": failed_responses,
            "screenshot": screenshot,
        }
    finally:
        await context.close()


async def main():
    launch_options = {"headless": True, "args": ["--no-sandbox", "--disable-gpu"]}
    chromium_path = Path(CHROMIUM)
    if chromium_path.is_file():
        launch_options["executable_path"] = str(chromium_path)

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(**launch_options)
        try:
            results = {}
            for label, path in (
                ("workspace", "/workspace?path=webClx"),
                ("terminal", "/terminal"),
                ("agent", "/agent"),
            ):
                results[f"{label}_desktop"] = await inspect_navigation(
                    browser,
                    label,
                    path,
                    {"width": 1280, "height": 900},
                )
                results[f"{label}_mobile"] = await inspect_navigation(
                    browser,
                    label,
                    path,
                    {"width": 390, "height": 844},
                    is_mobile=True,
                )

            terminal = results["terminal_desktop"]
            assert terminal["labels"][0] == "终端管理", terminal

            context = await browser.new_context(viewport={"width": 1280, "height": 900})
            page = await context.new_page()
            await page.goto(
                f"{BASE_URL}/workspace?path=webClx",
                wait_until="domcontentloaded",
            )
            await page.locator(".page-tabs > .tab-button", has_text="终端管理").click()
            await page.wait_for_url(f"{BASE_URL}/terminal", timeout=15_000)
            assert await page.locator("#top-nav-terminal.active").is_visible()
            await context.close()

            for result in results.values():
                assert not result["page_errors"], result
                assert not result["failed_responses"], result

            print(json.dumps(results, ensure_ascii=False, sort_keys=True))
        finally:
            await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
