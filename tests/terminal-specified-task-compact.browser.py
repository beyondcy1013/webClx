import asyncio
import json
import os

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome",
)
INJECT_SOURCE = os.environ.get("WEBCLX_TEST_INJECT_SOURCE") == "1"


async def dialog_geometry(page):
    return await page.evaluate(
        """() => {
            const dialog = document.querySelector('#terminal-specified-task-dialog');
            const form = document.querySelector('#terminal-specified-task-form');
            const textarea = document.querySelector('#terminal-specified-task-text');
            const agentModes = form.querySelector('.terminal-specified-task-agent-modes');
            const presetField = form.querySelector('.terminal-specified-task-preset-field');
            const rect = dialog.getBoundingClientRect();
            const agentRect = agentModes.getBoundingClientRect();
            const presetRect = presetField.getBoundingClientRect();
            return {
                height: Math.round(rect.height),
                width: Math.round(rect.width),
                formClientHeight: Math.round(form.clientHeight),
                formScrollHeight: Math.round(form.scrollHeight),
                textareaHeight: Math.round(textarea.getBoundingClientRect().height),
                gridColumns: getComputedStyle(form).gridTemplateColumns,
                agentRect: {top: Math.round(agentRect.top), width: Math.round(agentRect.width)},
                presetRect: {top: Math.round(presetRect.top), width: Math.round(presetRect.width)},
                inViewport: rect.top >= 0 && rect.bottom <= innerHeight,
                pageOverflow: document.documentElement.scrollWidth > innerWidth,
            };
        }"""
    )


async def main():
    page_errors = []
    console_errors = []
    failed_responses = []
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = await browser.new_page(viewport={"width": 1440, "height": 900})
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
            if response.status >= 400
            else None,
        )
        await page.goto(f"{BASE_URL}/terminal", wait_until="domcontentloaded")
        if INJECT_SOURCE:
            await page.locator("#terminal-specified-task-preset").evaluate(
                "element => element.closest('label').classList.add('terminal-specified-task-preset-field')"
            )
            await page.add_style_tag(
                path="/home/codes/webClx/static/styles-terminal.css"
            )
        await page.wait_for_function(
            "() => typeof openTerminalSpecifiedTaskDialog === 'function'"
            " && !state.loadingSessions"
        )
        await page.evaluate("() => openTerminalSpecifiedTaskDialog()")
        await page.wait_for_selector("#terminal-specified-task-dialog[open]")
        await page.wait_for_function(
            "() => Boolean(document.querySelector('#terminal-specified-task-preset').value)"
        )

        fixed = await dialog_geometry(page)
        await page.screenshot(
            path="/tmp/webclx-specified-task-compact-fixed.png",
            full_page=True,
        )

        await page.locator(
            '.terminal-specified-task-modes label:has-text("临时终端")'
        ).click()
        task = await dialog_geometry(page)
        await page.screenshot(
            path="/tmp/webclx-specified-task-compact-task.png",
            full_page=True,
        )

        await page.set_viewport_size({"width": 375, "height": 812})
        mobile = await dialog_geometry(page)
        await page.screenshot(
            path="/tmp/webclx-specified-task-compact-mobile.png",
            full_page=True,
        )
        await browser.close()

    assert fixed["height"] <= 500, fixed
    assert task["height"] <= 390, task
    assert mobile["height"] <= 430, mobile
    assert fixed["textareaHeight"] <= 110, fixed
    assert task["textareaHeight"] <= 110, task
    assert fixed["inViewport"] and task["inViewport"] and mobile["inViewport"]
    assert not fixed["pageOverflow"] and not task["pageOverflow"] and not mobile["pageOverflow"]
    unexpected_responses = [
        response
        for response in failed_responses
        if not (
            response["status"] == 404
            and "/api/workspace-icon?" in response["url"]
        )
    ]
    unexpected_console_errors = [
        message
        for message in console_errors
        if not message.startswith("Failed to load resource:")
    ]
    assert not page_errors, page_errors
    assert not unexpected_console_errors, unexpected_console_errors
    assert not unexpected_responses, unexpected_responses
    print(
        json.dumps(
            {
                "fixed": fixed,
                "task": task,
                "mobile": mobile,
                "ignoredOptionalIcon404s": len(failed_responses),
            },
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    asyncio.run(main())
