import asyncio
import os

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/third_party/browser-tools/bin/chromium",
)


async def verify_viewport(page, width, height, screenshot_path):
    await page.set_viewport_size({"width": width, "height": height})
    await page.goto(f"{BASE_URL}/terminal", wait_until="domcontentloaded")
    await page.wait_for_selector("#terminal-tools-button")
    await page.click("#terminal-tools-button")
    button = page.locator("#terminal-interrupt-resume")
    await button.wait_for(state="visible")
    bounds = await button.bounding_box()
    assert bounds is not None
    assert bounds["x"] >= 0
    assert bounds["y"] >= 0
    assert bounds["x"] + bounds["width"] <= width
    assert bounds["y"] + bounds["height"] <= height
    assert (await button.text_content()).strip() == "中断并恢复"
    await page.screenshot(path=screenshot_path)


async def main():
    console_errors = []
    page_errors = []
    http_errors = []
    interrupt_requests = []

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = await browser.new_page(viewport={"width": 1440, "height": 1000})
        page.on(
            "console",
            lambda message: console_errors.append(message.text)
            if message.type == "error"
            else None,
        )
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        page.on(
            "response",
            lambda response: http_errors.append((response.status, response.url))
            if response.status >= 400
            else None,
        )
        page.on(
            "request",
            lambda request: interrupt_requests.append(request.url)
            if request.method == "POST" and request.url.endswith("/interrupt-and-resume")
            else None,
        )
        await page.add_init_script("window.confirm = () => false;")

        await verify_viewport(page, 1440, 1000, "/tmp/webclx-interrupt-resume-1440.png")
        await page.click("#terminal-interrupt-resume")
        await page.wait_for_timeout(100)
        assert interrupt_requests == []

        await verify_viewport(page, 375, 812, "/tmp/webclx-interrupt-resume-375.png")
        assert not await page.evaluate(
            "document.documentElement.scrollWidth > document.documentElement.clientWidth"
        )
        unexpected_console_errors = [
            error for error in console_errors if "status of 404" not in error
        ]
        unexpected_http_errors = [
            error for error in http_errors if "/api/workspace-icon?" not in error[1]
        ]
        assert not unexpected_console_errors, unexpected_console_errors
        assert not unexpected_http_errors, unexpected_http_errors
        assert not page_errors, page_errors
        await browser.close()

    print("terminal interrupt-and-resume browser checks passed")


if __name__ == "__main__":
    asyncio.run(main())
