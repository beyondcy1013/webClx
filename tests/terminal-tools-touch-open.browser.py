import asyncio
import os

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome",
)


async def main():
    extraction_requests = []

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        context = await browser.new_context(
            viewport={"width": 375, "height": 812},
            has_touch=True,
            is_mobile=True,
        )
        page = await context.new_page()

        async def block_extraction(route, request):
            extraction_requests.append(request.url)
            await route.fulfill(
                status=409,
                content_type="application/json",
                body='{"error":"browser QA blocked preset extraction"}',
            )

        await page.route("**/api/terminal/sessions/*/extract-preset", block_extraction)
        await page.goto(f"{BASE_URL}/terminal", wait_until="domcontentloaded")
        trigger = page.locator("#terminal-tools-button")
        await trigger.wait_for(state="visible")
        await page.wait_for_function(
            "() => typeof triggerMobileKey === 'function'"
            " && Array.isArray(state?.sessions)"
            " && state.sessions.length > 0"
        )
        await page.wait_for_timeout(400)

        bounds = await trigger.bounding_box()
        assert bounds is not None
        await page.touchscreen.tap(
            bounds["x"] + bounds["width"] / 2,
            bounds["y"] + bounds["height"] / 2,
        )
        await page.wait_for_timeout(300)

        assert await page.locator("#terminal-tools-menu").is_visible()
        assert await trigger.get_attribute("aria-expanded") == "true"
        assert extraction_requests == [], extraction_requests
        await browser.close()

    print("terminal tools touch-open check passed")


if __name__ == "__main__":
    asyncio.run(main())
