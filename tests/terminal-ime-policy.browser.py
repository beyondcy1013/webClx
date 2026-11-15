import asyncio
import json
import mimetypes
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROMIUM = "/home/third_party/browser-tools/bin/chromium"
SESSION_ID = "ime-policy-session"


async def main(android=False):
    page_errors = []
    received_input = []
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = await browser.new_page(
            viewport={"width": 390, "height": 700},
            has_touch=True,
            is_mobile=android,
            user_agent=(
                "Mozilla/5.0 (Linux; Android 14; WebClx Test; wv) "
                "AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 "
                "Chrome/146.0.0.0 Mobile Safari/537.36 webClxAndroid/1.10.22"
            ) if android else None,
        )
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
            def receive(message):
                if isinstance(message, str):
                    payload = json.loads(message)
                    if payload.get("type") == "input":
                        received_input.append(payload["data"])

            websocket.on_message(receive)
            websocket.send(b"\r\n[root@webclx webClx]# ")

        await page.route("**/*", handle_http)
        await page.route_web_socket("**/api/terminal/ws?**", handle_websocket)
        await page.goto(
            f"http://webclx.test/terminal?path=webClx&session={SESSION_ID}",
            wait_until="domcontentloaded",
        )
        if android:
            await page.locator('#webclx-language-welcome button[data-locale="zh-CN"]').click()
        await page.wait_for_function(
            """() => state.activeSessionId === 'ime-policy-session'
                && terminalSoftKeyboardVisible()
                && terminalHelperTextarea()"""
        )
        assert await page.evaluate("term.options.rendererType") == ("dom" if android else "canvas")

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

        # Android Back can dismiss the IME without blurring xterm's helper.
        # Exercise real touch dispatch and WebSocket input, including that state.
        for visible, focused in ((True, True), (False, False), (False, True)):
            await page.evaluate(
                """({ visible, focused }) => {
                  terminalActionKeyboardPolicyActive = false;
                  terminalActionEditableFocusBlocked = false;
                  terminalNativeKeyboardVisible = window.__originalTerminalNativeKeyboardVisible;
                  window.WebClxAndroid = { isSystemKeyboardVisible: () => visible };
                  setTerminalSystemImeEnabled(true);
                  const helper = terminalHelperTextarea();
                  if (focused) helper.focus({ preventScroll: true });
                  else helper.blur();
                  window.__imePolicyHelperFocusCalls = 0;
                  window.__imePolicyAttributeChanges = [];
                  window.__imePolicyObserver = new MutationObserver((records) => {
                    window.__imePolicyAttributeChanges.push(...records.map((r) => r.attributeName));
                  });
                  window.__imePolicyObserver.observe(helper, {
                    attributes: true,
                    attributeFilter: ['inputmode', 'enterkeyhint', 'autocomplete',
                      'autocapitalize', 'autocorrect', 'spellcheck'],
                  });
                }""",
                {"visible": visible, "focused": focused},
            )
            received_input.clear()
            for sequence in ("home", "tab", "end"):
                await page.locator(f'#terminal-mobile-keys [data-sequence="{sequence}"]').tap()
            await page.evaluate("mobileKeySendQueue")
            await page.wait_for_timeout(380)
            real_touch_state = await page.evaluate(
                """() => {
                  window.__imePolicyObserver.disconnect();
                  return {
                    enabled: terminalSystemImeEnabled,
                    focused: terminalHelperTextareaFocused(),
                    focusCalls: window.__imePolicyHelperFocusCalls,
                    attributeChanges: window.__imePolicyAttributeChanges.length,
                  };
                }"""
            )
            assert "".join(received_input) == "\x1b[H\t\x1b[F", received_input
            assert real_touch_state == {
                "enabled": True,
                "focused": visible,
                "focusCalls": 0,
                "attributeChanges": 0,
            }, (visible, focused, real_touch_state)
        # A real held touch must select without ever restarting the input
        # connection, including a WebView retaining focus after Android Back.
        cdp = await page.context.new_cdp_session(page)
        for visible, focused, enabled in (
            (True, True, True), (False, False, True),
            (False, True, True), (False, False, False),
        ):
            await page.evaluate(
                """({ visible, focused, enabled }) => {
                  terminalActionKeyboardPolicyActive = false;
                  terminalActionEditableFocusBlocked = false;
                  window.WebClxAndroid = { isSystemKeyboardVisible: () => visible };
                  setTerminalSystemImeEnabled(enabled, { clearSuppression: true });
                  term.clearSelection();
                  const helper = terminalHelperTextarea();
                  if (focused) helper.focus({ preventScroll: true });
                  else helper.blur();
                  window.__imePolicyHelperFocusCalls = 0;
                  window.__selectionFocusEvents = [];
                  window.__selectionNativeRangeClears = 0;
                  window.__imeMenuCalls = [];
                  window.__originalRemoveAllRanges ??= Selection.prototype.removeAllRanges;
                  Selection.prototype.removeAllRanges = function () {
                    window.__selectionNativeRangeClears++;
                    return window.__originalRemoveAllRanges.call(this);
                  };
                  window.__selectionTouchDefaultPrevented = null;
                  terminalHost.addEventListener('touchstart', (event) => {
                    window.__selectionTouchDefaultPrevented = event.defaultPrevented;
                  }, { capture: true, once: true });
                  helper.onfocus = () => window.__selectionFocusEvents.push('focus');
                  helper.onblur = () => window.__selectionFocusEvents.push('blur');
                  window.__imePolicyAttributeChanges = [];
                  window.__imePolicyObserver.observe(helper, {
                    attributes: true,
                    attributeFilter: ['inputmode', 'enterkeyhint', 'autocomplete',
                      'autocapitalize', 'autocorrect', 'spellcheck'],
                  });
                }""",
                {"visible": visible, "focused": focused, "enabled": enabled},
            )
            received_input.clear()
            point = await page.evaluate("""() => {
              const rect = term.element.querySelector('.xterm-screen').getBoundingClientRect();
              return { x: Math.max(0, rect.left) + 25,
                y: Math.max(0, rect.top) + 10 };
            }""")
            await cdp.send('Input.dispatchTouchEvent', {
                "type": "touchStart", "touchPoints": [point],
            })
            # Observe the beginning separately: the native WebView must not
            # receive an uncancelled touchstart before the long-press timer.
            await page.wait_for_timeout(100)
            start_state = await page.evaluate("""() => ({
              defaultPrevented: window.__selectionTouchDefaultPrevented,
              focused: terminalHelperTextareaFocused(),
              focusCalls: window.__imePolicyHelperFocusCalls,
              focusEvents: window.__selectionFocusEvents,
              candidate: Boolean(terminalTouchSelectionCandidate),
              selected: term.hasSelection(),
            })""")
            assert start_state == {
                "defaultPrevented": True, "focused": focused,
                "focusCalls": 0, "focusEvents": [], "candidate": True,
                "selected": False,
            }, (visible, focused, enabled, start_state)
            await page.wait_for_timeout(2050)
            assert await page.evaluate("""() => Boolean(
              terminalTouchSelection?.anchor && term.hasSelection()
              && !terminalSelectionStartHandle.hidden
              && !terminalSelectionEndHandle.hidden)"""), 'timer must select before contextmenu'
            # Headless Chromium does not consistently emit the platform menu.
            # Deliver it to the real xterm element while the touch is held.
            await page.evaluate(
                """({ x, y }) => document.elementFromPoint(x, y).dispatchEvent(
                  new PointerEvent('contextmenu', {
                    bubbles: true, cancelable: true, pointerType: 'touch',
                    clientX: x, clientY: y,
                  }))""", point,
            )
            await cdp.send('Input.dispatchTouchEvent', {
                "type": "touchEnd", "touchPoints": [],
            })
            await page.wait_for_timeout(380)
            handle = await page.locator('#terminal-selection-end-handle').bounding_box()
            assert handle, 'long press must expose the selection handle'
            handle_point = {"x": handle["x"] + handle["width"] / 2,
                            "y": handle["y"] + handle["height"] / 2}
            await cdp.send('Input.dispatchTouchEvent', {
                "type": "touchStart", "touchPoints": [handle_point],
            })
            await cdp.send('Input.dispatchTouchEvent', {
                "type": "touchMove",
                "touchPoints": [{**handle_point, "x": handle_point["x"] + 20}],
            })
            await cdp.send('Input.dispatchTouchEvent', {
                "type": "touchEnd", "touchPoints": [],
            })
            await page.wait_for_timeout(100)
            selection_state = await page.evaluate(
                """() => {
                  window.__imePolicyObserver.disconnect();
                  const result = {
                    enabled: terminalSystemImeEnabled,
                    focused: terminalHelperTextareaFocused(),
                    focusCalls: window.__imePolicyHelperFocusCalls,
                    focusEvents: window.__selectionFocusEvents,
                    nativeRangeClears: window.__selectionNativeRangeClears,
                    attributeChanges: window.__imePolicyAttributeChanges,
                    selected: term.hasSelection(),
                    handlesVisible: !terminalSelectionStartHandle.hidden
                      && !terminalSelectionEndHandle.hidden,
                  };
                  terminalHelperTextarea().onfocus = null;
                  terminalHelperTextarea().onblur = null;
                  return result;
                }"""
            )
            assert selection_state == {
                "enabled": enabled, "focused": focused, "focusCalls": 0,
                "focusEvents": [], "nativeRangeClears": 0, "attributeChanges": [],
                "selected": True, "handlesVisible": True,
            }, (visible, focused, enabled, selection_state)
            assert not received_input, received_input
            assert await page.evaluate("""() => terminalHelperTextareaFocused()
              ? true
              : terminalHelperTextarea().getAttribute('inputmode') === 'none'"""), (
              "an unfocused helper must stay inputmode=none so long-press cannot raise the IME",
              visible, focused, enabled,
            )
            if not focused:
                # Even if some native or scripted path focuses the helper during
                # a selection gesture, the IME must stay off: the helper is only
                # editable for the native keyboard after an explicit IME action.
                stray = await page.evaluate("""() => {
                  const helper = terminalHelperTextarea();
                  const before = helper.getAttribute('inputmode');
                  helper.focus({ preventScroll: true });
                  const result = {
                    before,
                    after: helper.getAttribute('inputmode'),
                    armed: terminalSystemImeArmed,
                  };
                  helper.blur();
                  return result;
                }""")
                assert stray == {"before": "none", "after": "none", "armed": False}, (
                  visible, focused, enabled, stray,
                )
            menu = await page.evaluate("""() => ({
              copyVisible: !terminalSelectionCopyButton.hidden,
              copyAllVisible: !terminalSelectionCopyAllButton.hidden,
              imeVisible: !terminalSelectionOpenImeButton.hidden,
            })""")
            assert menu == {
                "copyVisible": True, "copyAllVisible": True, "imeVisible": True,
            }, menu
            await page.evaluate("""() => {
              window.__imeBridgeShowCalls = 0;
              window.WebClxAndroid.showSystemKeyboard = () => {
                window.__imeBridgeShowCalls += 1;
                return true;
              };
              const button = terminalSelectionOpenImeButton;
              button.dispatchEvent(new PointerEvent('pointerdown', {
                bubbles: true, cancelable: true, pointerType: 'touch',
              }));
              button.click();
            }""")
            await page.wait_for_timeout(100)
            opened = await page.evaluate("""() => ({
              enabled: terminalSystemImeEnabled,
              focused: terminalHelperTextareaFocused(),
              selected: term.hasSelection(),
              bridgeCalls: window.__imeBridgeShowCalls,
            })""")
            assert opened == {
                "enabled": True, "focused": True, "selected": True,
                "bridgeCalls": 1,
            }, (visible, focused, enabled, opened)
        await cdp.detach()
        # Selection suppression ends on the next touch; ordinary taps still
        # enter the terminal's direct-input path.
        await page.evaluate("""() => {
          terminalActionKeyboardPolicyActive = false;
          terminalActionEditableFocusBlocked = false;
          setTerminalSystemImeEnabled(true, { clearSuppression: true });
          terminalHelperTextarea().blur();
        }""")
        await page.touchscreen.tap(point["x"], point["y"])
        await page.wait_for_function("terminalHelperTextareaFocused()", timeout=2000)
        assert await page.evaluate("""() => !term.hasSelection()"""), "ordinary tap clears prior long-press selection"
        # Double tap selects the word under cursor and displays handles
        await page.wait_for_timeout(350)
        await page.touchscreen.tap(point["x"], point["y"])
        await page.touchscreen.tap(point["x"], point["y"])
        assert await page.evaluate("""() => Boolean(
            term.hasSelection() &&
            !terminalSelectionStartHandle.hidden &&
            !terminalSelectionEndHandle.hidden &&
            !terminalSelectionCopyButton.hidden &&
            !terminalSelectionCopyAllButton.hidden &&
            !terminalSelectionOpenImeButton.hidden
        )"""), "double tap must select word and display handles and the whole selection menu"
        # Tapping away clears the selection
        await page.wait_for_timeout(350)
        await page.touchscreen.tap(point["x"] + 100, point["y"] + 60)
        await page.wait_for_timeout(350)
        assert await page.evaluate("""() => Boolean(
            !term.hasSelection() &&
            terminalSelectionCopyButton.hidden &&
            terminalSelectionCopyAllButton.hidden &&
            terminalSelectionOpenImeButton.hidden
        )"""), "tapping away clears selection and hides the whole selection menu"
        await page.evaluate(
            """() => {
              delete window.WebClxAndroid;
              window.__setTerminalNativeKeyboardVisible(false);
            }"""
        )

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
        result["android"] = android
        result["realTouchStateCount"] = 3
        result["longPressSelectionStateCount"] = 4
        await browser.close()

    assert not page_errors, page_errors
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    asyncio.run(asyncio.wait_for(main(), timeout=90))
    asyncio.run(asyncio.wait_for(main(android=True), timeout=90))
