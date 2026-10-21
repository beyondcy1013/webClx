import json
import os
from pathlib import Path

from playwright.sync_api import sync_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111").rstrip("/")
WORKSPACE_PATH = "/home/codes/tools/webClx"
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/third_party/browser-tools/bin/chromium",
)


def main():
    launch_options = {"headless": True, "args": ["--no-sandbox", "--disable-gpu"]}
    if Path(CHROMIUM).is_file():
        launch_options["executable_path"] = CHROMIUM

    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(**launch_options)
        context = browser.new_context(viewport={"width": 1440, "height": 1000})
        context.grant_permissions(["clipboard-read", "clipboard-write"], origin=BASE_URL)
        page = context.new_page()
        page_errors = []
        page.on("pageerror", lambda error: page_errors.append(str(error)))

        response = context.request.get(
            f"{BASE_URL}/api/terminal/codex-conversations?cwd={WORKSPACE_PATH}"
        )
        assert response.ok, response.text()
        conversations = response.json().get("conversations", [])
        assert conversations, "browser regression requires a workspace-history conversation"
        session_id = conversations[0]["session_id"]

        page.goto(f"{BASE_URL}/workspace?path=webClx", wait_until="domcontentloaded")
        page.locator("#tab-workspace-history").click()
        session_value = page.locator(
            f'.workspace-history-session-value:text-is("{session_id}")'
        )
        session_value.wait_for(state="visible", timeout=20_000)
        session_cell = session_value.locator("xpath=..")
        copy_button = session_cell.locator(".workspace-history-session-copy")

        desktop_metrics = session_cell.evaluate(
            """
            cell => ({
              width: cell.getBoundingClientRect().width,
              fontSize: getComputedStyle(cell).fontSize,
              titleFontSize: getComputedStyle(cell.parentElement.querySelector('.workspace-history-title-cell')).fontSize,
              visibleValue: cell.querySelector('.workspace-history-session-value').textContent,
            })
            """
        )
        assert desktop_metrics["visibleValue"] == session_id, desktop_metrics
        assert desktop_metrics["width"] >= 359, desktop_metrics
        assert desktop_metrics["fontSize"] == desktop_metrics["titleFontSize"], desktop_metrics

        copy_button.click()
        assert page.evaluate("navigator.clipboard.readText()") == session_id
        desktop_screenshot = "/tmp/webclx-workspace-history-session-id-desktop.png"
        page.screenshot(path=desktop_screenshot, full_page=True)

        page.set_viewport_size({"width": 375, "height": 812})
        mobile_metrics = page.locator("#workspace-history-view > .table-wrap").evaluate(
            """
            wrap => ({
              clientWidth: wrap.clientWidth,
              scrollWidth: wrap.scrollWidth,
              tableWidth: wrap.querySelector('.workspace-history-table').getBoundingClientRect().width,
              sessionWidth: wrap.querySelector('.workspace-history-session-cell').getBoundingClientRect().width,
            })
            """
        )
        assert mobile_metrics["scrollWidth"] > mobile_metrics["clientWidth"], mobile_metrics
        assert mobile_metrics["tableWidth"] >= 1360, mobile_metrics
        assert mobile_metrics["tableWidth"] <= 1400, mobile_metrics
        assert mobile_metrics["sessionWidth"] >= 359, mobile_metrics
        mobile_screenshot = "/tmp/webclx-workspace-history-session-id-mobile.png"
        page.screenshot(path=mobile_screenshot, full_page=True)

        assert not page_errors, page_errors
        print(
            json.dumps(
                {
                    "ok": True,
                    "session_id": session_id,
                    "desktop": desktop_metrics,
                    "mobile": mobile_metrics,
                    "desktop_screenshot": desktop_screenshot,
                    "mobile_screenshot": mobile_screenshot,
                },
                sort_keys=True,
            )
        )
        context.close()
        browser.close()


if __name__ == "__main__":
    main()
