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

        await page.evaluate(
            """() => {
              window.__originalTerminalNativeKeyboardVisible = terminalNativeKeyboardVisible;
              window.__setTerminalNativeKeyboardVisible = (visible) => {
                terminalNativeKeyboardVisible = () => visible;
              };
            }"""
        )

        global_gate_state = await page.evaluate(
            """() => {
              const input = document.createElement('input');
              const button = document.createElement('button');
              button.type = 'button';
              button.addEventListener('click', () => input.focus());
              document.body.append(input, button);
              button.click();
              const blocked = document.activeElement !== input;
              input.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
              input.focus();
              const directEditableAllowed = document.activeElement === input;
              input.remove();
              button.remove();
              return { blocked, directEditableAllowed };
            }"""
        )
        assert global_gate_state == {
            "blocked": True,
            "directEditableAllowed": True,
        }, global_gate_state

        terminal_tap_state = await page.evaluate(
            """() => {
              terminalActionKeyboardPolicyActive = true;
              terminalActionEditableFocusBlocked = true;
              window.__setTerminalNativeKeyboardVisible(false);
              terminalHelperTextarea().blur();
              document.getElementById('terminal-host').click();
              return {
                policyActive: terminalActionKeyboardPolicyActive,
                focusBlocked: terminalActionEditableFocusBlocked,
                systemImeEnabled: terminalSystemImeEnabled,
                helperFocused: document.activeElement === terminalHelperTextarea(),
              };
            }"""
        )
        assert terminal_tap_state == {
            "policyActive": False,
            "focusBlocked": False,
            "systemImeEnabled": True,
            "helperFocused": True,
        }, terminal_tap_state

        await page.evaluate(
            """() => {
              window.__imePolicyEditableFocusCalls = 0;
              window.__imePolicyHelperFocusCalls = 0;
              for (const prototype of [HTMLTextAreaElement.prototype, HTMLInputElement.prototype]) {
                const originalFocus = prototype.focus;
                prototype.focus = function (...args) {
                  window.__imePolicyEditableFocusCalls += 1;
                  if (this === terminalHelperTextarea()) {
                    window.__imePolicyHelperFocusCalls += 1;
                  }
                  return originalFocus.apply(this, args);
                };
              }
            }"""
        )

        detached_soft_key_menus = [
            ("#terminal-number-button", "#terminal-number-menu", "button:not([hidden])"),
            ("#terminal-slash-command-button", "#terminal-slash-command-menu", "button:not([hidden])"),
            ("#terminal-function-command-button", "#terminal-function-command-menu", "button:not([hidden])"),
            ("#terminal-project-command-button", "#terminal-project-command-menu", "button:not([hidden])"),
            ("#terminal-tools-button", "#terminal-tools-menu", "button:not([hidden])"),
        ]

        async def assert_detached_menu_preserves_ime(enabled):
            await page.evaluate(
                """(enabled) => {
                  terminalActionKeyboardPolicyActive = false;
                  terminalActionEditableFocusBlocked = false;
                  window.__setTerminalNativeKeyboardVisible(enabled);
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
                await page.wait_for_timeout(380)
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

        await page.evaluate(
            """() => {
              terminalActionKeyboardPolicyActive = false;
              terminalActionEditableFocusBlocked = false;
              window.__setTerminalNativeKeyboardVisible(true);
              setTerminalSystemImeEnabled(true);
              terminalHelperTextarea().focus({ preventScroll: true });
            }"""
        )
        await page.locator("#terminal-project-command-button").click()
        await page.wait_for_timeout(380)
        project_menu_state = await page.evaluate(
            """() => ({
              expanded: document.getElementById('terminal-project-command-button').getAttribute('aria-expanded'),
              menuHidden: document.getElementById('terminal-project-command-menu').hidden,
              helperFocused: document.activeElement === terminalHelperTextarea(),
              visualHeight: window.visualViewport?.height || window.innerHeight,
              layoutHeight: window.innerHeight,
            })"""
        )
        assert project_menu_state["expanded"] == "true", project_menu_state
        assert not project_menu_state["menuHidden"], project_menu_state
        assert project_menu_state["helperFocused"], project_menu_state
        await page.locator("#terminal-project-command-button").click()

        await page.evaluate(
            """() => {
              terminalActionKeyboardPolicyActive = false;
              terminalActionEditableFocusBlocked = false;
              window.__setTerminalNativeKeyboardVisible(false);
              window.__imePolicyEditableFocusCalls = 0;
              terminalHelperTextarea().blur();
            }"""
        )
        await page.locator("#terminal-schedule-button").click()
        await page.wait_for_timeout(380)
        schedule_state = await page.evaluate(
            """() => ({
              dialogOpen: document.getElementById('terminal-scheduled-tasks-dialog').open,
              activeId: document.activeElement?.id || '',
              activeEditable: document.activeElement instanceof HTMLTextAreaElement
                || document.activeElement instanceof HTMLInputElement,
            })"""
        )
        assert schedule_state["dialogOpen"] is True
        assert schedule_state["activeEditable"] is False
        await page.evaluate("document.getElementById('terminal-scheduled-tasks-dialog').close()")
        await page.wait_for_timeout(380)
        assert await page.evaluate("""() => hasPrimaryTouchInput()
            ? !(document.activeElement instanceof HTMLTextAreaElement || document.activeElement instanceof HTMLInputElement)
            : terminalHelperTextareaFocused() && terminalHelperTextarea().getAttribute('inputmode') === 'text'
        """)

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
                          terminalActionKeyboardPolicyActive = false;
                          terminalActionEditableFocusBlocked = false;
                          window.__setTerminalNativeKeyboardVisible(enabled);
                          setTerminalSystemImeEnabled(enabled);
                          if (enabled) {
                            terminalHelperTextarea().focus({ preventScroll: true });
                          } else {
                            terminalHelperTextarea().blur();
                          }
                          window.__imePolicyHelperFocusCalls = 0;
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
                    await page.wait_for_timeout(380)
                    state = await page.evaluate(
                        """() => ({
                          systemImeEnabled: terminalSystemImeEnabled,
                          helperFocused: document.activeElement === terminalHelperTextarea(),
                          helperFocusCalls: window.__imePolicyHelperFocusCalls,
                        })"""
                    )
                    assert state["systemImeEnabled"] is enabled, (label, state)
                    assert state["helperFocusCalls"] == 0, (label, state)
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

        end_button = page.locator('#terminal-mobile-keys [data-sequence="end"]')
        assert await end_button.count() == 1
        for enabled in (True, False):
            await page.evaluate(
                """(enabled) => {
                  terminalActionKeyboardPolicyActive = false;
                  terminalActionEditableFocusBlocked = false;
                  window.__setTerminalNativeKeyboardVisible(enabled);
                  setTerminalSystemImeEnabled(enabled);
                  if (enabled) terminalHelperTextarea().focus({ preventScroll: true });
                  else terminalHelperTextarea().blur();
                }""",
                enabled,
            )
            await end_button.click()
            await page.wait_for_timeout(380)
            end_state = await page.evaluate(
                """() => ({
                  enabled: terminalSystemImeEnabled,
                  focused: document.activeElement === terminalHelperTextarea(),
                })"""
            )
            assert end_state == {"enabled": enabled, "focused": enabled}, end_state

        for selector in (
            '#terminal-mobile-keys [data-sequence="home"]',
            '#scroll-terminal-bottom',
        ):
            await page.evaluate(
                """() => {
                  terminalActionKeyboardPolicyActive = false;
                  terminalActionEditableFocusBlocked = false;
                  window.__setTerminalNativeKeyboardVisible(false);
                  setTerminalSystemImeEnabled(true);
                  terminalHelperTextarea().blur();
                  window.__imePolicyHelperFocusCalls = 0;
                }"""
            )
            if selector == '#scroll-terminal-bottom':
                await page.evaluate(
                    """async () => {
                      await new Promise((resolve) => {
                        term.write(
                          Array.from({ length: 120 }, (_, index) => `bottom-test-${index}\\r\\n`).join(''),
                          resolve,
                        );
                      });
                      scrollTerminalToTop();
                      updateTerminalScrollBottomButton();
                    }"""
                )
                assert not await page.locator(selector).is_disabled()
            await page.locator(selector).click()
            await page.wait_for_timeout(380)
            dismissed_keyboard_state = await page.evaluate(
                """() => ({
                  enabled: terminalSystemImeEnabled,
                  focused: document.activeElement === terminalHelperTextarea(),
                  helperFocusCalls: window.__imePolicyHelperFocusCalls,
                })"""
            )
            assert dismissed_keyboard_state == {
                "enabled": True,
                "focused": False,
                "helperFocusCalls": 0,
            }, (selector, dismissed_keyboard_state)

        await page.evaluate(
            """() => {
              terminalActionKeyboardPolicyActive = false;
              terminalActionEditableFocusBlocked = false;
              window.__setTerminalNativeKeyboardVisible(true);
              setTerminalSystemImeEnabled(true);
              terminalHelperTextarea().focus({ preventScroll: true });
            }"""
        )
        await page.click("#terminal-function-command-button")
        assert await page.locator("#terminal-function-command-menu").is_visible()
        await page.evaluate("window.__imePolicyHelperFocusCalls = 0")
        await page.locator('#terminal-mobile-keys [data-sequence="tab"]').click()
        assert await page.locator("#terminal-function-command-menu").is_hidden()
        assert await page.evaluate("window.__imePolicyHelperFocusCalls === 0")

        await page.click("#terminal-function-command-button")
        assert await page.locator("#terminal-function-command-menu").is_visible()
        await page.evaluate("window.dispatchEvent(new Event('blur'))")
        assert await page.locator("#terminal-function-command-menu").is_hidden()
        assert await page.evaluate("window.__imePolicyHelperFocusCalls === 0")

        await page.click("#terminal-function-command-button")
        assert await page.locator("#terminal-function-command-menu").is_visible()

        await page.evaluate("window.__imePolicyHelperFocusCalls = 0")
        await page.locator('label:has-text("触摸复制")').click()
        assert await page.evaluate("window.__imePolicyHelperFocusCalls === 0")
        await page.keyboard.press("Escape")
        assert await page.locator("#terminal-function-command-menu").is_hidden()
        assert await page.evaluate("window.__imePolicyHelperFocusCalls === 0")

        await page.evaluate(
            """() => {
              terminalActionKeyboardPolicyActive = false;
              terminalActionEditableFocusBlocked = false;
              window.__setTerminalNativeKeyboardVisible(false);
              setTerminalSystemImeEnabled(false);
              terminalHelperTextarea().blur();
            }"""
        )
        await page.click("#terminal-function-command-button")
        assert not await page.evaluate("document.activeElement === terminalHelperTextarea()")
        await page.locator('label:has-text("触摸复制")').click()
        assert not await page.evaluate("document.activeElement === terminalHelperTextarea()")

        unauthorized_show_result = await page.evaluate(
            """() => ({
              accepted: runTerminalKeyboardCommand('show_system_keyboard'),
              helperFocused: document.activeElement === terminalHelperTextarea(),
            })"""
        )
        assert unauthorized_show_result == {
            "accepted": False,
            "helperFocused": False,
        }, unauthorized_show_result

        await page.locator('label:has-text("系统键盘")').click()
        authorized_keyboard_state = await page.evaluate(
            """() => ({
              enabled: terminalSystemImeEnabled,
              helperFocused: document.activeElement === terminalHelperTextarea(),
              activeId: document.activeElement?.id || '',
              policyActive: terminalActionKeyboardPolicyActive,
              focusBlocked: terminalActionEditableFocusBlocked,
            })"""
        )
        assert authorized_keyboard_state == {
            "enabled": True,
            "helperFocused": True,
            "activeId": "",
            "policyActive": False,
            "focusBlocked": False,
        }, authorized_keyboard_state

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
