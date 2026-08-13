import asyncio
import json
import mimetypes
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROMIUM = "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome"
SESSION_ID = "ime-policy-session"


async def main():
    page_errors = []
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = await browser.new_page(viewport={"width": 390, "height": 700})
        page.on("pageerror", lambda error: page_errors.append(str(error)))

        async def handle_http(route):
            parsed = urlparse(route.request.url)
            if parsed.path == "/terminal":
                await route.fulfill(
                    status=200,
                    content_type="text/html; charset=utf-8",
                    body=(STATIC / "terminal.html").read_bytes(),
                )
                return
            if parsed.path.startswith("/assets/"):
                asset = STATIC / parsed.path.removeprefix("/assets/")
                if asset.is_file():
                    content_type = mimetypes.guess_type(asset.name)[0] or "application/octet-stream"
                    await route.fulfill(status=200, content_type=content_type, body=asset.read_bytes())
                else:
                    await route.fulfill(status=404, body="missing asset")
                return
            if parsed.path == "/api/settings":
                await route.fulfill(
                    json={
                        "workspace_dir": "/home/codes",
                        "desktop_terminal_soft_keyboard_enabled": True,
                        "terminal_function_commands": [
                            {
                                "key": "copy_terminal_name",
                                "label": "复制终端名",
                                "action": "copy_terminal_name",
                                "command": "",
                                "shortcut": "",
                            }
                        ],
                    }
                )
                return
            if parsed.path == "/api/terminal/sessions":
                await route.fulfill(
                    json={
                        "path": "webClx",
                        "display_path": "/webClx",
                        "sessions": [
                            {
                                "id": SESSION_ID,
                                "name": "IME policy",
                                "path": "webClx",
                                "display_path": "/webClx",
                                "idle": False,
                                "connected": True,
                            }
                        ],
                    }
                )
                return
            if parsed.path in {
                "/api/terminal/scheduled-inputs",
                "/api/terminal/auto-continue-tasks",
                "/api/terminal/resume-archives",
            }:
                await route.fulfill(json={"tasks": [], "archives": []})
                return
            if parsed.path.startswith("/api/"):
                await route.fulfill(json={})
                return
            await route.fulfill(status=404, body="not found")

        async def handle_websocket(websocket):
            websocket.send(b"\r\n[root@webclx webClx]# ")

        await page.route("**/*", handle_http)
        await page.route_web_socket("**/api/terminal/ws?**", handle_websocket)
        await page.goto(
            f"http://webclx.test/terminal?path=webClx&session={SESSION_ID}",
            wait_until="domcontentloaded",
        )
        await page.wait_for_function(
            """() => state.activeSessionId === 'ime-policy-session'
                && terminalSoftKeyboardVisible()
                && terminalHelperTextarea()"""
        )

        detached_soft_key_menus = [
            ("#terminal-escape-command-button", "#terminal-escape-command-menu", "button"),
            ("#terminal-number-button", "#terminal-number-menu", "button"),
            ("#terminal-slash-command-button", "#terminal-slash-command-menu", "button"),
            ("#terminal-function-command-button", "#terminal-function-command-menu", "button"),
            ("#terminal-project-command-button", "#terminal-project-command-menu", "button"),
            ("#terminal-tools-button", "#terminal-tools-menu", "button"),
        ]

        async def assert_detached_menu_preserves_ime(enabled):
            await page.evaluate(
                """(enabled) => {
                  setTerminalSystemImeEnabled(enabled);
                  if (enabled) {
                    terminalHelperTextarea().focus({ preventScroll: true });
                  } else {
                    terminalHelperTextarea().blur();
                  }
                }""",
                enabled,
            )
            for trigger_selector, menu_selector, item_selector in detached_soft_key_menus:
                await page.locator(trigger_selector).click()
                menu = page.locator(menu_selector)
                assert await menu.is_visible(), (trigger_selector, menu_selector)
                await page.evaluate(
                    """() => {
                      document.addEventListener('click', (event) => {
                        event.preventDefault();
                        event.stopImmediatePropagation();
                      }, { capture: true, once: true });
                    }"""
                )
                await menu.locator(item_selector).first.click()
                state = await page.evaluate(
                    """() => ({
                      systemImeEnabled: terminalSystemImeEnabled,
                      helperFocused: document.activeElement === terminalHelperTextarea(),
                    })"""
                )
                assert state["systemImeEnabled"] is enabled, (menu_selector, state)
                assert state["helperFocused"] is enabled, (menu_selector, state)
                await page.evaluate(
                    """([triggerSelector, menuSelector]) => {
                      const trigger = document.querySelector(triggerSelector);
                      const menu = document.querySelector(menuSelector);
                      menu.hidden = true;
                      trigger.setAttribute('aria-expanded', 'false');
                    }""",
                    [trigger_selector, menu_selector],
                )
            return len(detached_soft_key_menus)

        detached_menu_count = await assert_detached_menu_preserves_ime(True)
        assert await assert_detached_menu_preserves_ime(False) == detached_menu_count

        async def assert_direct_soft_keys_preserve_ime(enabled):
            original_trigger_saved = await page.evaluate(
                """() => {
                  window.__originalTriggerMobileKey = triggerMobileKey;
                  triggerMobileKey = () => {};
                  return true;
                }"""
            )
            assert original_trigger_saved
            buttons = page.locator("#terminal-mobile-keys button:not(:disabled)")
            button_count = await buttons.count()
            assert button_count > 0
            assert await page.locator("#terminal-mobile-keys select:not([hidden])").count() == 0
            try:
                for index in range(button_count):
                    await page.evaluate(
                        """(enabled) => {
                          setTerminalSystemImeEnabled(enabled);
                          if (enabled) {
                            terminalHelperTextarea().focus({ preventScroll: true });
                          } else {
                            terminalHelperTextarea().blur();
                          }
                          document.addEventListener('click', (event) => {
                            event.preventDefault();
                            event.stopImmediatePropagation();
                          }, { capture: true, once: true });
                        }""",
                        enabled,
                    )
                    button = buttons.nth(index)
                    label = (await button.get_attribute("aria-label")) or (await button.inner_text())
                    await button.click()
                    state = await page.evaluate(
                        """() => ({
                          systemImeEnabled: terminalSystemImeEnabled,
                          helperFocused: document.activeElement === terminalHelperTextarea(),
                        })"""
                    )
                    assert state["systemImeEnabled"] is enabled, (label, state)
                    assert state["helperFocused"] is enabled, (label, state)
            finally:
                await page.evaluate(
                    """() => {
                      triggerMobileKey = window.__originalTriggerMobileKey;
                      delete window.__originalTriggerMobileKey;
                    }"""
                )
            return button_count

        direct_soft_key_count = await assert_direct_soft_keys_preserve_ime(True)
        assert await assert_direct_soft_keys_preserve_ime(False) == direct_soft_key_count

        await page.evaluate(
            """() => {
              setTerminalSystemImeEnabled(true);
              terminalHelperTextarea().focus({ preventScroll: true });
            }"""
        )
        await page.click("#terminal-function-command-button")
        assert await page.locator("#terminal-function-command-menu").is_visible()
        await page.locator('#terminal-mobile-keys [data-sequence="tab"]').click()
        assert await page.locator("#terminal-function-command-menu").is_hidden()
        assert await page.evaluate(
            "terminalSystemImeEnabled && document.activeElement === terminalHelperTextarea()"
        )

        await page.click("#terminal-function-command-button")
        assert await page.locator("#terminal-function-command-menu").is_visible()
        await page.evaluate("window.dispatchEvent(new Event('blur'))")
        assert await page.locator("#terminal-function-command-menu").is_hidden()
        assert await page.evaluate(
            "terminalSystemImeEnabled && document.activeElement === terminalHelperTextarea()"
        )

        await page.click("#terminal-function-command-button")
        assert await page.locator("#terminal-function-command-menu").is_visible()
        assert await page.evaluate("document.activeElement === terminalHelperTextarea()")

        await page.locator('label:has-text("触摸复制")').click()
        assert await page.evaluate("document.activeElement === terminalHelperTextarea()")
        await page.keyboard.press("Escape")
        assert await page.locator("#terminal-function-command-menu").is_hidden()
        assert await page.evaluate("document.activeElement === terminalHelperTextarea()")

        await page.evaluate(
            """() => {
              setTerminalSystemImeEnabled(false);
              terminalHelperTextarea().blur();
            }"""
        )
        await page.click("#terminal-function-command-button")
        assert not await page.evaluate("document.activeElement === terminalHelperTextarea()")
        await page.locator('label:has-text("触摸复制")').click()
        assert not await page.evaluate("document.activeElement === terminalHelperTextarea()")

        await page.locator('label:has-text("系统键盘")').click()
        assert await page.evaluate(
            "terminalSystemImeEnabled && document.activeElement === terminalHelperTextarea()"
        )

        result = await page.evaluate(
            """() => ({
              systemImeEnabled: terminalSystemImeEnabled,
              helperFocused: document.activeElement === terminalHelperTextarea(),
              touchCopyEnabled: !terminalTouchSelectionDisabled,
            })"""
        )
        result["detachedMenuCount"] = detached_menu_count
        result["directSoftKeyCount"] = direct_soft_key_count
        await browser.close()

    assert not page_errors, page_errors
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    asyncio.run(main())
