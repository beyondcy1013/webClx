import os

from playwright.sync_api import sync_playwright


BASE_URL = "http://127.0.0.1:11111"
ANDROID_USER_AGENT = (
    "Mozilla/5.0 (Linux; Android 15; Pixel 9) AppleWebKit/537.36 "
    "(KHTML, like Gecko) Chrome/132.0.0.0 Mobile Safari/537.36"
)


def main() -> None:
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        context = browser.new_context(
            viewport={"width": 390, "height": 844},
            has_touch=True,
            is_mobile=True,
            user_agent=ANDROID_USER_AGENT,
        )
        page = context.new_page()
        page.add_init_script(
            """
            window.__androidSelectEvents = [];
            for (const type of ['pointerdown', 'click', 'scroll']) {
              document.addEventListener(type, (event) => {
                window.__androidSelectEvents.push({
                  type,
                  id: event.target?.id || '',
                  tag: event.target?.tagName || '',
                  pointerType: event.pointerType || '',
                });
              }, true);
            }
            """
        )
        page.goto(f"{BASE_URL}/terminal", wait_until="domcontentloaded", timeout=30_000)
        page.locator("#session-switcher").wait_for(state="attached", timeout=15_000)
        page.evaluate("document.querySelector('#terminal-server-switch-dialog').showModal()")

        select = page.locator("#terminal-server-switch-select")
        option_count = select.locator("option").count()
        select.tap()

        menu = page.locator("#terminal-android-select-menu")
        try:
            menu.wait_for(state="visible", timeout=5_000)
        except Exception as error:
            events = page.evaluate("window.__androidSelectEvents")
            raise AssertionError(f"Android select menu stayed hidden; events={events}") from error
        assert menu.get_attribute("role") == "menu"
        assert menu.evaluate("element => element.parentElement.tagName") == "DIALOG"
        assert menu.locator('[role="menuitem"]').count() == option_count
        assert menu.locator('input[type="radio"], [role="radio"], [role="menuitemradio"]').count() == 0

        dimensions = page.evaluate(
            """
            () => ({
              clientWidth: document.documentElement.clientWidth,
              scrollWidth: document.documentElement.scrollWidth,
              menuWidth: document.querySelector('#terminal-android-select-menu').getBoundingClientRect().width,
            })
            """
        )
        assert dimensions["scrollWidth"] <= dimensions["clientWidth"] + 2
        assert dimensions["menuWidth"] <= dimensions["clientWidth"] - 16
        screenshot_path = os.environ.get("WEBCLX_QA_SCREENSHOT")
        if screenshot_path:
            page.screenshot(path=screenshot_path, full_page=False)

        page.evaluate(
            """
            () => {
              window.__androidSelectChangeCount = 0;
              document.querySelector('#terminal-server-switch-select').addEventListener(
                'change',
                () => { window.__androidSelectChangeCount += 1; },
              );
            }
            """
        )
        expected_value = select.locator("option").nth(1).get_attribute("value")
        menu.locator('[role="menuitem"]').nth(1).click()
        assert select.input_value() == expected_value
        assert page.evaluate("window.__androidSelectChangeCount") == 1
        assert menu.is_hidden()
        assert menu.evaluate("element => element.parentElement.tagName") == "BODY"

        context.close()

        workspace_context = browser.new_context(
            viewport={"width": 390, "height": 844},
            has_touch=True,
            is_mobile=True,
            user_agent=ANDROID_USER_AGENT,
        )
        workspace_page = workspace_context.new_page()
        workspace_page.goto(f"{BASE_URL}/workspace", wait_until="domcontentloaded", timeout=30_000)

        for select_id in ("favorite-path-select", "workspace-history-path-select"):
            if select_id == "workspace-history-path-select":
                workspace_page.locator("#tab-workspace-history").click()
                workspace_page.locator("#workspace-history-view").wait_for(
                    state="visible", timeout=5_000
                )
            workspace_page.evaluate(
                """
                selectId => {
                  const select = document.getElementById(selectId);
                  select.disabled = false;
                  select.replaceChildren(
                    new Option('选择命令', ''),
                    new Option('测试项目', 'test-project'),
                  );
                }
                """,
                select_id,
            )
            workspace_select = workspace_page.locator(f"#{select_id}")
            option_count = workspace_select.locator("option").count()
            assert option_count > 0
            workspace_select.tap()
            workspace_menu = workspace_page.locator("#terminal-android-select-menu")
            workspace_menu.wait_for(state="visible", timeout=5_000)
            assert workspace_menu.locator('[role="menuitem"]').count() == option_count
            assert workspace_menu.locator(
                'input[type="radio"], [role="radio"], [role="menuitemradio"]'
            ).count() == 0
            if select_id == "workspace-history-path-select":
                workspace_screenshot_path = os.environ.get("WEBCLX_WORKSPACE_QA_SCREENSHOT")
                if workspace_screenshot_path:
                    workspace_page.screenshot(path=workspace_screenshot_path, full_page=False)
            workspace_page.keyboard.press("Escape")
            assert workspace_menu.is_hidden()

        workspace_context.close()

        desktop_context = browser.new_context(viewport={"width": 1280, "height": 800})
        desktop_page = desktop_context.new_page()
        desktop_page.goto(f"{BASE_URL}/terminal", wait_until="domcontentloaded", timeout=30_000)
        desktop_page.locator("#terminal-server-switch-select").dispatch_event(
            "pointerdown",
            {"pointerType": "touch"},
        )
        assert desktop_page.locator("#terminal-android-select-menu").is_hidden()
        desktop_context.close()
        browser.close()


if __name__ == "__main__":
    main()
