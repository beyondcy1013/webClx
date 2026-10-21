import asyncio
import json
import mimetypes
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROMIUM = "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome"
SESSION_ID = "shortcut-session"
TERMINAL_NAME = "Shortcut Terminal 20260813001234567890"
RESUME_ID = "123e4567-e89b-12d3-a456-426614174000"
INTERRUPTED_RESUME_ID = "019f8d03-c14d-7712-b5ac-2a63ebd7af36"
INTERRUPTED_RESUME_OUTPUT = "\r\n".join(
    [
        "exceeded retry limit, last status: 429 Too Many",
        "Requests",
        "Token usage: total=52,954,470 input=52,452,152 (+",
        " 106,606,875 cached) output=502,318 (reasoning 16",
        "0,154)",
        "To continue this session, run codex resume, then",
        "select glm接着修 (019f8d03-c14d-7712-b5ac-2a63ebd",
        "7af36)",
        "[root@openeuler longzijue]# codex resume then",
        "bash: /home/root/.local/bin/codex: No such file or directory",
        "[root@openeuler longzijue]# codex resume then",
        "bash: /home/root/.local/bin/codex: No such file or directory",
        "[root@openeuler longzijue]# ",
    ]
)


async def main():
    deploy_payloads = []
    auto_typed_payloads = []
    page_errors = []

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = await browser.new_page(viewport={"width": 1440, "height": 1000})
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        await page.add_init_script(
            """
            Object.defineProperty(navigator, "clipboard", {
              configurable: true,
              value: {
                writeText: async (text) => { window.__shortcutCopiedText = String(text); },
              },
            });
            """
        )

        sessions = [
            {
                "id": SESSION_ID,
                "name": TERMINAL_NAME,
                "path": "webClx",
                "display_path": "/webClx",
                "idle": False,
                "connected": True,
            }
        ]

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
                        "desktop_terminal_soft_keyboard_enabled": False,
                        "terminal_function_commands": [
                            {
                                "key": "copy_terminal_name",
                                "label": "复制终端名",
                                "action": "copy_terminal_name",
                                "command": "",
                                "shortcut": "",
                            },
                            {
                                "key": "ctrl_v",
                                "label": "Ctrl+V",
                                "action": "send_sequence",
                                "command": "ctrl_v",
                                "shortcut": "",
                            },
                            {
                                "key": "deploy_project",
                                "label": "本项目部署脚本",
                                "action": "deploy_project",
                                "command": "",
                                "shortcut": "^B",
                            },
                        ],
                        "terminal_slash_commands": [
                            {
                                "key": "extract_current_session",
                                "label": "高级复制",
                                "action": "extract_current_session",
                                "command": "",
                                "shortcut": "",
                            },
                            {
                                "key": "resume_current_session",
                                "label": "恢复会话",
                                "action": "resume_current_agent_session",
                                "command": "",
                                "shortcut": "",
                            },
                        ],
                    }
                )
                return
            if parsed.path == "/api/terminal/sessions":
                await route.fulfill(
                    json={"path": "webClx", "display_path": "/webClx", "sessions": sessions}
                )
                return
            if parsed.path == f"/api/terminal/sessions/{SESSION_ID}/agent-session":
                await route.fulfill(
                    json={"resume_id": "", "command": "", "source": "browser_test"}
                )
                return
            if parsed.path == "/api/terminal/auto-typed-input":
                auto_typed_payloads.append(route.request.post_data_json)
                await route.fulfill(
                    json={"data": f"codex resume {INTERRUPTED_RESUME_ID}\n"}
                )
                return
            if parsed.path == "/api/build/deploy":
                deploy_payloads.append(route.request.post_data_json)
                await route.fulfill(
                    json={
                        "queued": True,
                        "project": "webClx",
                        "install_command": ["bash", "scripts/rebuild-and-deploy.sh"],
                    }
                )
                return
            if parsed.path == "/api/terminal/resume-archives":
                await route.fulfill(json={"archives": []})
                return
            if parsed.path in {
                "/api/terminal/scheduled-inputs",
                "/api/terminal/auto-continue-tasks",
            }:
                await route.fulfill(json={"tasks": []})
                return
            if parsed.path.startswith("/api/"):
                await route.fulfill(json={})
                return
            await route.fulfill(status=404, body="not found")

        async def handle_websocket(websocket):
            await asyncio.sleep(0.05)
            websocket.send(INTERRUPTED_RESUME_OUTPUT.encode())

        await page.route("**/*", handle_http)
        await page.route_web_socket("**/api/terminal/ws?**", handle_websocket)
        await page.goto(
            f"http://webclx.test/terminal?path=webClx&session={SESSION_ID}",
            wait_until="domcontentloaded",
        )
        await page.wait_for_function(
            """() => state.activeSessionId === 'shortcut-session'
                && state.workspaceDir === '/home/codes'
                && typeof handleTerminalFunctionShortcut === 'function'"""
        )
        await page.wait_for_function(
            "() => readTerminalBufferTailText().includes('7af36)')"
        )

        await page.set_viewport_size({"width": 390, "height": 500})
        await page.wait_for_function(
            """() => {
              const topMenu = document.getElementById('terminal-fab-top-menu');
              const bottomMenu = document.getElementById('terminal-fab-menu');
              return topMenu && !topMenu.hidden && bottomMenu && !bottomMenu.hidden
                && getComputedStyle(document.documentElement)
                  .getPropertyValue('--terminal-visible-viewport-height').trim() === '500px';
            }"""
        )
        await page.wait_for_function(
            """() => ['terminal-fab-top-menu', 'terminal-fab-menu'].every((id) => {
              const styles = getComputedStyle(document.getElementById(id));
              return styles.opacity === '1'
                && ['none', 'matrix(1, 0, 0, 1, 0, 0)'].includes(styles.transform);
            })"""
        )
        await page.wait_for_function(
            """() => Array.from(
              document.querySelectorAll('.terminal-fab-item:not([hidden])')
            ).every((item) => item.getAnimations().every(
              (animation) => animation.playState === 'finished'
            ))"""
        )
        fab_layout = await page.evaluate(
            """() => {
              const readItems = (ids) => ids.map((id) => {
                const rect = document.getElementById(id).getBoundingClientRect();
                return { id, top: rect.top, bottom: rect.bottom, right: rect.right };
              });
              const topItems = readItems([
                'terminal-schedule-button',
                'scroll-terminal-top',
              ]);
              const bottomItems = readItems([
                'terminal-input-history-button',
                'scroll-terminal-bottom',
                'terminal-soft-keyboard-toggle',
              ]);
              const topMenuRect = document
                .getElementById('terminal-fab-top-menu')
                .getBoundingClientRect();
              const bottomMenuRect = document
                .getElementById('terminal-fab-menu')
                .getBoundingClientRect();
              const bottomGroupRect = document
                .getElementById('terminal-fab')
                .getBoundingClientRect();
              const terminalHostRect = document
                .getElementById('terminal-host')
                .getBoundingClientRect();
              const viewport = window.visualViewport;
              return {
                topItems,
                bottomItems,
                topMenu: { top: topMenuRect.top, bottom: topMenuRect.bottom },
                bottomMenu: { top: bottomMenuRect.top, bottom: bottomMenuRect.bottom },
                bottomGroup: { top: bottomGroupRect.top, bottom: bottomGroupRect.bottom },
                terminalHost: {
                  top: terminalHostRect.top,
                  bottom: terminalHostRect.bottom,
                },
                viewportTop: viewport?.offsetTop || 0,
                viewportBottom: (viewport?.offsetTop || 0) + (viewport?.height || innerHeight),
                clientWidth: document.documentElement.clientWidth,
                scrollWidth: document.documentElement.scrollWidth,
              };
            }"""
        )
        assert [item["id"] for item in fab_layout["topItems"]] == [
            "terminal-schedule-button",
            "scroll-terminal-top",
        ], fab_layout
        assert [item["id"] for item in fab_layout["bottomItems"]] == [
            "terminal-input-history-button",
            "scroll-terminal-bottom",
            "terminal-soft-keyboard-toggle",
        ], fab_layout
        assert all(
            current["top"] < following["top"]
            for current, following in zip(
                fab_layout["topItems"], fab_layout["topItems"][1:]
            )
        ), fab_layout
        assert all(
            current["top"] < following["top"]
            for current, following in zip(
                fab_layout["bottomItems"], fab_layout["bottomItems"][1:]
            )
        ), fab_layout
        assert fab_layout["topMenu"]["top"] >= fab_layout["terminalHost"]["top"], fab_layout
        assert fab_layout["topMenu"]["bottom"] <= fab_layout["bottomMenu"]["top"], fab_layout
        assert fab_layout["bottomGroup"]["bottom"] <= min(
            fab_layout["terminalHost"]["bottom"], fab_layout["viewportBottom"]
        ) + 1, fab_layout
        assert all(
            abs(item["right"] - fab_layout["clientWidth"]) <= 0.1
            for item in fab_layout["topItems"] + fab_layout["bottomItems"]
        ), fab_layout
        assert fab_layout["scrollWidth"] <= fab_layout["clientWidth"] + 1, fab_layout

        await page.screenshot(path="/tmp/webclx-terminal-fab-mobile-viewport.png")
        await page.locator("#terminal-schedule-button").click()
        assert await page.locator("#terminal-paste-dialog").get_attribute("open") == ""
        assert not await page.locator("#terminal-paste-schedule").get_attribute("hidden")
        await page.screenshot(
            path="/tmp/webclx-terminal-schedule-dialog-mobile.png"
        )
        await page.evaluate("closeTerminalPasteDialog()")
        await page.set_viewport_size({"width": 1440, "height": 1000})
        await page.wait_for_function(
            """() => {
              const dialog = document.getElementById('terminal-paste-dialog');
              return !dialog.open
                && !dialog.contains(document.activeElement)
                && getComputedStyle(document.documentElement)
                  .getPropertyValue('--terminal-visible-viewport-height').trim() === '1000px';
            }"""
        )

        shortcuts = await page.evaluate(
            """() => Object.fromEntries(
              state.terminalSlashCommands.concat(state.terminalFunctionCommands)
                .filter((command) => [
                  'toggle_soft_keyboard', 'extract_current_session', 'copy_terminal_name'
                ].includes(command.key))
                .map((command) => [command.key, command.shortcut])
            )"""
        )
        assert shortcuts == {
            "toggle_soft_keyboard": "Ctrl+K",
            "extract_current_session": "Ctrl+Alt+S",
            "copy_terminal_name": "Ctrl+Alt+T",
        }, shortcuts
        assert not await page.locator(
            '#terminal-function-command-menu [data-key="copy_terminal_name"]'
        ).count()
        assert await page.locator(
            '#terminal-slash-command-menu [data-key="copy_terminal_name"]'
        ).count() == 1
        assert not await page.locator(
            '#terminal-slash-command-menu [data-key="webui"]'
        ).count()
        assert await page.locator(
            '#terminal-project-command-menu [data-project-action="open_project_url"]'
        ).count() == 1
        assert await page.locator(
            '#terminal-slash-command-menu [data-key="copy_id_and_ask"]'
        ).count() == 1
        quick_menu_keys = await page.locator(
            "#terminal-slash-command-menu button[data-key]"
        ).evaluate_all("buttons => buttons.map((button) => button.dataset.key)")
        id_command_keys = [
            "extract_resume",
            "copy_resume_id",
            "extract_current_session",
            "current_resume_id",
            "copy_id_and_ask",
        ]
        slash_command_keys = [
            "resume",
            "status",
            "fork",
            "compact",
        ]
        assert quick_menu_keys[-len(slash_command_keys):] == slash_command_keys, quick_menu_keys
        assert quick_menu_keys[-len(slash_command_keys) - 1] == "copy_terminal_name", quick_menu_keys
        assert quick_menu_keys[
            -(len(slash_command_keys) + len(id_command_keys) + 1):-(len(slash_command_keys) + 1)
        ] == id_command_keys, quick_menu_keys
        assert not await page.locator(
            '#terminal-slash-command-menu [data-key="quota"]'
        ).count()
        assert await page.locator(
            '#terminal-function-command-buttons [data-key="quota"]'
        ).count() == 1
        assert not await page.locator(
            '#terminal-function-command-select option[value="deploy_project"]'
        ).count()
        assert not await page.locator(
            '#terminal-slash-command-menu [data-key="deploy_project"]'
        ).count()
        assert await page.locator(
            '#terminal-project-command-select option[value="deploy_project"]'
        ).get_attribute("data-shortcut") == "^B"

        soft_keyboard_state = await page.locator("body").get_attribute(
            "data-terminal-soft-keyboard"
        )
        assert soft_keyboard_state == "closed", soft_keyboard_state

        await page.keyboard.press("Control+K")
        assert await page.locator("body").get_attribute("data-terminal-soft-keyboard") == "open"

        await page.set_viewport_size({"width": 390, "height": 700})
        await page.evaluate(
            """() => {
              window.__escapeKeyInputs = [];
              window.__originalSendTerminalInput = sendTerminalInput;
              sendTerminalInput = (data) => {
                window.__escapeKeyInputs.push(data);
                return true;
              };
            }"""
        )
        escape_key_layout = await page.evaluate(
            """() => {
              const trigger = document.getElementById('terminal-escape-command-button');
              const triggerStyle = getComputedStyle(trigger);
              return {
                action: trigger.getAttribute('data-action'),
                label: trigger.getAttribute('aria-label'),
                menuCount: document.querySelectorAll('#terminal-escape-command-menu').length,
                triggerFontSize: triggerStyle.fontSize,
                scrollWidth: document.documentElement.scrollWidth,
                clientWidth: document.documentElement.clientWidth,
              };
            }"""
        )
        assert escape_key_layout["action"] == "escape_ctrl_c", escape_key_layout
        assert "长按" in escape_key_layout["label"], escape_key_layout
        assert escape_key_layout["menuCount"] == 0, escape_key_layout
        assert escape_key_layout["scrollWidth"] <= escape_key_layout["clientWidth"] + 1, escape_key_layout

        await page.locator("#terminal-escape-command-button").click()
        await page.evaluate("async () => { await mobileKeySendQueue; }")
        escape_key_inputs = await page.evaluate("window.__escapeKeyInputs")
        assert escape_key_inputs == ["\u0003"], escape_key_inputs

        await page.evaluate("() => { window.__escapeKeyInputs = []; }")
        escape_button = page.locator("#terminal-escape-command-button")
        escape_box = await escape_button.bounding_box()
        assert escape_box is not None
        await page.mouse.move(
            escape_box["x"] + escape_box["width"] / 2,
            escape_box["y"] + escape_box["height"] / 2,
        )
        await page.mouse.down()
        await page.wait_for_timeout(620)
        await page.mouse.up()
        await page.evaluate("async () => { await mobileKeySendQueue; }")
        escape_key_inputs = await page.evaluate("window.__escapeKeyInputs")
        assert escape_key_inputs == ["\u001b"], escape_key_inputs
        await page.evaluate(
            """() => {
              sendTerminalInput = window.__originalSendTerminalInput;
              delete window.__originalSendTerminalInput;
            }"""
        )

        await page.locator("#terminal-function-command-button").click()
        await page.wait_for_timeout(50)
        function_menu_layout = await page.evaluate(
            """() => {
              const trigger = document.getElementById('terminal-function-command-button');
              const menu = document.getElementById('terminal-function-command-menu');
              const triggerRect = trigger.getBoundingClientRect();
              const menuRect = menu.getBoundingClientRect();
              return {
                triggerTop: triggerRect.top,
                triggerLeft: triggerRect.left,
                menuBottom: menuRect.bottom,
                menuLeft: menuRect.left,
                menuTop: menuRect.top,
              };
            }"""
        )
        assert function_menu_layout["menuBottom"] <= function_menu_layout["triggerTop"] - 5, function_menu_layout
        assert function_menu_layout["menuLeft"] <= function_menu_layout["triggerLeft"] + 1, function_menu_layout
        assert function_menu_layout["menuTop"] >= 7, function_menu_layout
        await page.locator("#terminal-function-command-button").click()

        await page.locator("#terminal-project-command-button").click()
        compact_project_menu = await page.evaluate(
            """() => {
              const menu = document.getElementById('terminal-project-command-menu');
              const item = menu.querySelector('button');
              const menuRect = menu.getBoundingClientRect();
              const itemRect = item.getBoundingClientRect();
              const style = getComputedStyle(item);
              return {
                width: menuRect.width,
                itemHeight: itemRect.height,
                fontSize: style.fontSize,
                textAlign: style.textAlign,
                scrollWidth: document.documentElement.scrollWidth,
                clientWidth: document.documentElement.clientWidth,
              };
            }"""
        )
        assert compact_project_menu["width"] <= 176.1, compact_project_menu
        assert abs(compact_project_menu["itemHeight"] - 28) <= 0.1, compact_project_menu
        assert compact_project_menu["fontSize"] == "12px", compact_project_menu
        assert compact_project_menu["textAlign"] == "left", compact_project_menu
        assert compact_project_menu["scrollWidth"] <= compact_project_menu["clientWidth"] + 1, compact_project_menu
        await page.locator("#terminal-project-command-button").click()

        await page.evaluate("setTerminalToolsMenuExpanded(true)")
        compact_tools_menu = await page.evaluate(
            """() => {
              const menu = document.getElementById('terminal-tools-menu');
              const action = menu.querySelector('.terminal-tools-action');
              const option = menu.querySelector('.terminal-tools-option');
              const menuRect = menu.getBoundingClientRect();
              const actionRect = action.getBoundingClientRect();
              const optionRect = option.getBoundingClientRect();
              const actionStyle = getComputedStyle(action);
              return {
                width: menuRect.width,
                actionHeight: actionRect.height,
                optionHeight: optionRect.height,
                actionFontSize: actionStyle.fontSize,
                actionTextAlign: actionStyle.textAlign,
                scrollWidth: document.documentElement.scrollWidth,
                clientWidth: document.documentElement.clientWidth,
              };
            }"""
        )
        assert compact_tools_menu["width"] <= 176.1, compact_tools_menu
        assert abs(compact_tools_menu["actionHeight"] - 28) <= 0.1, compact_tools_menu
        assert abs(compact_tools_menu["optionHeight"] - 28) <= 0.1, compact_tools_menu
        assert compact_tools_menu["actionFontSize"] == "12px", compact_tools_menu
        assert compact_tools_menu["actionTextAlign"] == "left", compact_tools_menu
        assert compact_tools_menu["scrollWidth"] <= compact_tools_menu["clientWidth"] + 1, compact_tools_menu
        await page.evaluate("closeTerminalToolsMenu()")
        await page.set_viewport_size({"width": 1440, "height": 1000})

        assert not await page.locator('[data-action="paste_clipboard"]').count()
        await page.evaluate(
            """() => {
              window.__pasteFromClipboardCalls = 0;
              pasteFromClipboard = () => { window.__pasteFromClipboardCalls += 1; };
            }"""
        )
        await page.locator("#terminal-function-command-button").click()
        await page.locator('#terminal-function-command-buttons [data-key="ctrl_v"]').click()
        assert await page.evaluate("window.__pasteFromClipboardCalls") == 1

        await page.keyboard.press("Control+B")
        for _ in range(100):
            if deploy_payloads:
                break
            await page.wait_for_timeout(10)
        assert len(deploy_payloads) == 1, deploy_payloads
        assert deploy_payloads[0]["project_dir"] == "/home/codes/tools/webClx"
        assert deploy_payloads[0]["source_terminal_id"] == SESSION_ID

        await page.locator("#terminal-slash-command-button").click()
        await page.locator(
            '#terminal-slash-command-menu [data-key="copy_resume_id"]'
        ).click()
        copy_resume_overlay = page.locator("#terminal-copy-resume-overlay")
        await copy_resume_overlay.wait_for(state="visible")
        assert await copy_resume_overlay.locator("textarea").input_value() == INTERRUPTED_RESUME_ID
        await copy_resume_overlay.get_by_role("button", name="复制", exact=True).click()
        await page.wait_for_function(
            f"() => window.__shortcutCopiedText === '{INTERRUPTED_RESUME_ID}'"
        )
        await page.wait_for_function(
            f"() => document.getElementById('terminal-status')?.textContent === "
            f"'已复制终端文本中的 Session ID：{INTERRUPTED_RESUME_ID}'"
        )

        await page.locator("#terminal-slash-command-button").click()
        await page.locator(
            '#terminal-slash-command-menu [data-key="current_resume_id"]'
        ).click()
        await copy_resume_overlay.wait_for(state="visible")
        current_resume_command = f"codex resume {INTERRUPTED_RESUME_ID}"
        assert await copy_resume_overlay.locator("textarea").input_value() == current_resume_command
        await copy_resume_overlay.get_by_role("button", name="复制", exact=True).click()
        await page.wait_for_function(
            f"() => window.__shortcutCopiedText === '{current_resume_command}'"
        )
        await page.wait_for_function(
            f"() => document.getElementById('terminal-status')?.textContent === "
            f"'已复制：{current_resume_command}'"
        )

        await page.keyboard.press("Control+Alt+S")
        await page.wait_for_function(
            f"() => window.__shortcutCopiedText === '{INTERRUPTED_RESUME_ID}'"
        )
        await page.wait_for_function(
            f"() => document.getElementById('terminal-status')?.textContent === "
            f"'已提取并复制 Session ID：{INTERRUPTED_RESUME_ID}'"
        )

        await page.evaluate(
            """() => {
              window.__copyIdAndAskInputs = [];
              window.__originalSendTerminalInputToSession = sendTerminalInputToSession;
              sendTerminalInputToSession = (data, sessionId) => {
                window.__copyIdAndAskInputs.push({ data, sessionId });
                return true;
              };
              window.__shortcutCopiedText = '';
            }"""
        )
        await page.locator("#terminal-slash-command-button").click()
        await page.locator(
            '#terminal-slash-command-menu [data-key="copy_id_and_ask"]'
        ).click()
        await page.wait_for_function(
            """() => window.__shortcutCopiedText ===
              '调用codex db维护skill读取session id为 '
              + '019f8d03-c14d-7712-b5ac-2a63ebd7af36'
              + '并接受指令：'"""
        )
        copy_id_and_ask_inputs = await page.evaluate("window.__copyIdAndAskInputs")
        assert copy_id_and_ask_inputs == [], copy_id_and_ask_inputs
        copy_id_and_ask_clipboard = await page.evaluate("window.__shortcutCopiedText")
        assert copy_id_and_ask_clipboard == (
            "调用codex db维护skill读取session id为 "
            f"{INTERRUPTED_RESUME_ID}并接受指令："
        ), copy_id_and_ask_clipboard
        await page.wait_for_function(
            f"() => document.getElementById('terminal-status')?.textContent === "
            f"'已复制 Session 提问文字：{INTERRUPTED_RESUME_ID}'"
        )
        await page.evaluate(
            """() => {
              sendTerminalInputToSession = window.__originalSendTerminalInputToSession;
              delete window.__originalSendTerminalInputToSession;
            }"""
        )

        await page.locator("#terminal-slash-command-menu").evaluate(
            "menu => { menu.style.maxHeight = '84px'; }"
        )
        await page.locator("#terminal-slash-command-button").click()
        await page.wait_for_timeout(50)
        quick_menu_scroll = await page.locator("#terminal-slash-command-menu").evaluate(
            """menu => ({
              scrollTop: menu.scrollTop,
              maxScroll: menu.scrollHeight - menu.clientHeight,
            })"""
        )
        assert quick_menu_scroll["maxScroll"] > 0, quick_menu_scroll
        assert abs(quick_menu_scroll["scrollTop"] - quick_menu_scroll["maxScroll"]) <= 1, quick_menu_scroll
        await page.locator(
            '#terminal-slash-command-menu [data-key="resume_current_session"]'
        ).click()
        for _ in range(200):
            if auto_typed_payloads:
                break
            await page.wait_for_timeout(10)
        await page.evaluate("async () => { await mobileKeySendQueue; }")
        assert auto_typed_payloads == [
            {
                "command_line": f"codex resume {INTERRUPTED_RESUME_ID}",
                "session_id": SESSION_ID,
                "submit_enters": 0,
            }
        ], auto_typed_payloads

        await page.evaluate(
            """() => {
              window.__originalTerminalSoftKeyboardAutoVisible = terminalSoftKeyboardAutoVisible;
              terminalSoftKeyboardAutoVisible = () => false;
              state.temporaryDesktopTerminalSoftKeyboardVisible = false;
              document.body.dataset.terminalSoftKeyboard = "closed";
              syncTerminalSoftKeyboardVisibility();
            }"""
        )
        assert await page.locator("body").get_attribute("data-terminal-soft-keyboard") == "closed"
        jump_focus_state = await page.evaluate(
            """async () => {
              setTerminalSystemImeEnabled(false);
              terminalHelperTextarea().blur();
              await new Promise((resolve) => {
                term.write(Array.from({ length: 120 }, (_, index) => `jump-line-${index}\\r\\n`).join(''), resolve);
              });
              updateTerminalScrollBottomButton();
              return {
                topDisabled: document.getElementById('scroll-terminal-top').disabled,
                bottomDisabled: document.getElementById('scroll-terminal-bottom').disabled,
              };
            }"""
        )
        assert jump_focus_state == {"topDisabled": False, "bottomDisabled": False}, jump_focus_state

        await page.wait_for_timeout(2100)
        await page.evaluate(
            """() => {
              setTerminalSystemImeEnabled(false);
              terminalHelperTextarea().blur();
            }"""
        )

        await page.locator("#toggle-terminal-path").click()
        await page.wait_for_timeout(50)
        transient_control_state = await page.evaluate(
            """() => ({
              systemImeEnabled: terminalSystemImeEnabled,
              helperFocused: document.activeElement === terminalHelperTextarea(),
            })"""
        )
        assert not transient_control_state["systemImeEnabled"], transient_control_state
        assert not transient_control_state["helperFocused"], transient_control_state

        await page.locator("#scroll-terminal-top").click()
        await page.wait_for_timeout(50)
        jump_top_state = await page.evaluate(
            """() => ({
              systemImeEnabled: terminalSystemImeEnabled,
              helperFocused: document.activeElement === terminalHelperTextarea(),
              viewportY: term.buffer.active.viewportY,
            })"""
        )
        assert jump_top_state["viewportY"] == 0, jump_top_state
        assert not jump_top_state["systemImeEnabled"], jump_top_state
        assert not jump_top_state["helperFocused"], jump_top_state

        await page.locator("#scroll-terminal-bottom").click()
        await page.wait_for_timeout(50)
        jump_bottom_state = await page.evaluate(
            """() => ({
              systemImeEnabled: terminalSystemImeEnabled,
              helperFocused: document.activeElement === terminalHelperTextarea(),
              viewportY: term.buffer.active.viewportY,
              baseY: term.buffer.active.baseY,
            })"""
        )
        assert jump_bottom_state["viewportY"] == jump_bottom_state["baseY"], jump_bottom_state
        assert not jump_bottom_state["systemImeEnabled"], jump_bottom_state
        assert not jump_bottom_state["helperFocused"], jump_bottom_state
        await page.evaluate(
            """() => {
              terminalSoftKeyboardAutoVisible = window.__originalTerminalSoftKeyboardAutoVisible;
              delete window.__originalTerminalSoftKeyboardAutoVisible;
              syncTerminalSoftKeyboardVisibility();
            }"""
        )

        await page.set_viewport_size({"width": 390, "height": 500})
        await page.keyboard.press("Control+Alt+T")
        await page.wait_for_function(
            f"() => window.__shortcutCopiedText === '{TERMINAL_NAME}'"
        )
        await page.wait_for_function(
            f"() => document.getElementById('terminal-status')?.textContent === "
            f"'已复制终端名：{TERMINAL_NAME}'"
        )
        copy_status_layout = await page.locator("#terminal-status").evaluate(
            """status => ({
              text: status.textContent,
              clientWidth: status.clientWidth,
              scrollWidth: status.scrollWidth,
              viewportWidth: document.documentElement.clientWidth,
              rect: status.getBoundingClientRect().toJSON(),
            })"""
        )
        assert copy_status_layout["text"] == f"已复制终端名：{TERMINAL_NAME}", copy_status_layout
        assert copy_status_layout["scrollWidth"] <= copy_status_layout["clientWidth"] + 1, copy_status_layout
        assert copy_status_layout["rect"]["left"] >= 0, copy_status_layout
        assert copy_status_layout["rect"]["right"] <= copy_status_layout["viewportWidth"] + 1, copy_status_layout

        assert not page_errors, page_errors
        await browser.close()

    print(
        json.dumps(
            {
                "shortcuts": shortcuts,
                "soft_keyboard_toggled": True,
                "escape_key_layout": escape_key_layout,
                "escape_key_inputs": [ord(value) for value in escape_key_inputs],
                "function_menu_layout": function_menu_layout,
                "compact_project_menu": compact_project_menu,
                "compact_tools_menu": compact_tools_menu,
                "quick_menu_scroll": quick_menu_scroll,
                "quick_menu_keys": quick_menu_keys,
                "ctrl_v_shared_paste_calls": 1,
                "fab_mobile_layout": fab_layout,
                "deploy_payloads": len(deploy_payloads),
                "session_copied": INTERRUPTED_RESUME_ID,
                "current_resume_command_copied": current_resume_command,
                "copy_id_and_ask_inputs": copy_id_and_ask_inputs,
                "copy_id_and_ask_clipboard": copy_id_and_ask_clipboard,
                "resume_command": auto_typed_payloads[0]["command_line"],
                "terminal_name_copied": TERMINAL_NAME,
                "copy_status_layout": copy_status_layout,
                "page_errors": page_errors,
            },
            ensure_ascii=False,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    asyncio.run(main())
