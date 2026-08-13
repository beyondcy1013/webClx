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
WORKSPACE_PATH = os.environ.get("WEBCLX_TEST_WORKSPACE_PATH", "webClx")


async def inspect_workspace(browser, label, viewport, is_mobile=False):
    context = await browser.new_context(
        viewport=viewport,
        is_mobile=is_mobile,
        has_touch=is_mobile,
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
        and not message.text.startswith("Failed to load resource:")
        else None,
    )
    page.on(
        "response",
        lambda response: failed_responses.append(
            {"status": response.status, "url": response.url}
        )
        if response.status >= 400 and response.url.startswith(BASE_URL)
        and not (
            response.status == 404
            and response.url.startswith(f"{BASE_URL}/api/workspace-icon?")
        )
        else None,
    )

    try:
        await page.goto(
            f"{BASE_URL}/workspace?path={WORKSPACE_PATH}",
            wait_until="domcontentloaded",
        )
        await page.locator("#workspace-view.active #sessions-view").wait_for(
            state="visible",
            timeout=15_000,
        )
        await page.locator("#sessions-list tr").first.wait_for(timeout=15_000)

        layout = await page.evaluate(
            """
            () => {
              const workspace = document.querySelector('#workspace-view');
              const browserPanel = workspace.querySelector('.browser-panel');
              const editorPanel = workspace.querySelector('.editor-panel');
              const sessionsPanel = workspace.querySelector('#sessions-view');
              const rect = element => {
                const value = element.getBoundingClientRect();
                return {
                  left: value.left,
                  right: value.right,
                  top: value.top,
                  bottom: value.bottom,
                  width: value.width,
                  height: value.height,
                };
              };
              const control = selector => {
                const element = sessionsPanel.querySelector(selector);
                return {
                  ...rect(element),
                  whiteSpace: getComputedStyle(element).whiteSpace,
                };
              };
              return {
                workspace: rect(workspace),
                browser: rect(browserPanel),
                editor: rect(editorPanel),
                sessions: rect(sessionsPanel),
                sessionsParent: sessionsPanel.parentElement?.id,
                sessionsVisible: getComputedStyle(sessionsPanel).display !== 'none',
                hasSessionsTab: Boolean(document.querySelector('#tab-sessions')),
                bodyOverflow: document.documentElement.scrollWidth
                  - document.documentElement.clientWidth,
                controls: {
                  search: control('#sessions-search-form'),
                  refresh: control('#refresh-sessions'),
                  create: control('#create-session'),
                  picker: control('.toolbar > .workspace-icon-select'),
                  open: control('#open-terminal-session-root'),
                },
              };
            }
            """
        )
        assert layout["sessionsParent"] == "workspace-view", layout
        assert layout["sessionsVisible"], layout
        assert not layout["hasSessionsTab"], layout
        assert layout["bodyOverflow"] <= 1, layout
        assert layout["sessions"]["top"] >= max(
            layout["browser"]["bottom"],
            layout["editor"]["bottom"],
        ) - 1, layout
        assert layout["sessions"]["left"] <= layout["browser"]["left"] + 1, layout
        assert layout["sessions"]["right"] >= layout["editor"]["right"] - 1, layout
        if is_mobile:
            actions = [
                layout["controls"][key]
                for key in ("refresh", "create", "picker", "open")
            ]
            assert max(action["height"] for action in actions) <= 44, layout
            assert max(action["top"] for action in actions) - min(
                action["top"] for action in actions
            ) <= 2, layout
            assert layout["controls"]["picker"]["width"] >= 80, layout
            assert layout["controls"]["search"]["bottom"] <= min(
                action["top"] for action in actions
            ) + 1, layout
            assert all(
                layout["controls"][key]["whiteSpace"] == "nowrap"
                for key in ("refresh", "create", "open")
            ), layout

        screenshot = f"/tmp/webclx-workspace-terminal-{label}.png"
        await page.screenshot(path=screenshot, full_page=True)
        return {
            "layout": layout,
            "page_errors": page_errors,
            "console_errors": console_errors,
            "failed_responses": failed_responses,
            "screenshot": screenshot,
        }
    finally:
        await context.close()


async def main():
    chromium_path = Path(CHROMIUM)
    launch_options = {
        "headless": True,
        "args": ["--no-sandbox", "--disable-gpu"],
    }
    if chromium_path.is_file():
        launch_options["executable_path"] = str(chromium_path)

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(**launch_options)
        try:
            desktop = await inspect_workspace(
                browser,
                "desktop",
                {"width": 1440, "height": 1000},
            )
            mobile = await inspect_workspace(
                browser,
                "mobile",
                {"width": 390, "height": 844},
                is_mobile=True,
            )

            context = await browser.new_context(viewport={"width": 1280, "height": 900})
            page = await context.new_page()
            await page.goto(
                f"{BASE_URL}/sessions?path={WORKSPACE_PATH}",
                wait_until="domcontentloaded",
            )
            await page.wait_for_url(
                f"{BASE_URL}/workspace?path={WORKSPACE_PATH}",
                timeout=15_000,
            )
            assert await page.locator("#workspace-view.active #sessions-view").is_visible()
            await context.close()

            for result in (desktop, mobile):
                assert not result["page_errors"], result
                assert not result["console_errors"], result
                assert not result["failed_responses"], result

            print(
                json.dumps(
                    {"desktop": desktop, "mobile": mobile},
                    ensure_ascii=False,
                    sort_keys=True,
                )
            )
        finally:
            await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
