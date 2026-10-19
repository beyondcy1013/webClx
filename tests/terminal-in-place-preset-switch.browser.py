import asyncio
import base64
import hashlib
import hmac
import json
import os
import time
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome",
)
RESUME_ID = "019f2350-db5f-7cf0-b476-1cf14855b05d"


def local_session_cookie():
    secret_file = os.environ.get("WEBCLX_SESSION_SECRET_FILE", "").strip()
    credentials_file = os.environ.get("WEBCLX_CREDENTIALS_FILE", "").strip()
    if not secret_file and not credentials_file:
        return None
    if not secret_file or not credentials_file:
        raise RuntimeError("both local session files are required")
    hostname = (urlparse(BASE_URL).hostname or "").lower()
    if hostname not in {"127.0.0.1", "::1", "localhost"}:
        raise RuntimeError("refusing to derive a local session cookie for a non-loopback URL")
    secret_path = Path(secret_file)
    credentials_path = Path(credentials_file)
    if secret_path.stat().st_mode & 0o077 or credentials_path.stat().st_mode & 0o077:
        raise RuntimeError("local session inputs must be owner-only files")
    secret = secret_path.read_bytes()
    credentials = json.loads(credentials_path.read_text(encoding="utf-8"))
    username = str(credentials.get("username", "")).strip()
    if len(secret) < 32 or not username:
        raise RuntimeError("local session inputs are invalid")
    payload = json.dumps(
        {"user": username, "exp": int(time.time()) + 300},
        separators=(",", ":"),
    ).encode("utf-8")
    payload_b64 = base64.urlsafe_b64encode(payload).decode("ascii").rstrip("=")
    signature = hmac.new(secret, payload_b64.encode("ascii"), hashlib.sha256).hexdigest()
    return {
        "name": "webclx_session",
        "value": f"{payload_b64}.{signature}",
        "url": BASE_URL,
        "httpOnly": True,
        "sameSite": "Strict",
    }


async def dialog_geometry(page):
    return await page.evaluate(
        """() => {
            const dialog = document.querySelector('#terminal-in-place-preset-dialog');
            const rect = dialog.getBoundingClientRect();
            return {
                open: dialog.open,
                inViewport: rect.left >= 0 && rect.top >= 0
                    && rect.right <= innerWidth && rect.bottom <= innerHeight,
                pageOverflow: document.documentElement.scrollWidth > innerWidth,
                width: Math.round(rect.width),
                height: Math.round(rect.height),
            };
        }"""
    )


async def permanent_dialog_geometry(page):
    return await page.evaluate(
        """() => {
            const dialog = document.querySelector('#terminal-permanent-preset-dialog');
            const rect = dialog.getBoundingClientRect();
            return {
                open: dialog.open,
                inViewport: rect.left >= 0 && rect.top >= 0
                    && rect.right <= innerWidth && rect.bottom <= innerHeight,
                pageOverflow: document.documentElement.scrollWidth > innerWidth,
                width: Math.round(rect.width),
                height: Math.round(rect.height),
            };
        }"""
    )


async def main():
    console_errors = []
    page_errors = []
    failed_responses = []

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        context = await browser.new_context(viewport={"width": 1440, "height": 900})
        await context.add_init_script(
            "localStorage.setItem('webclx:locale', 'zh-CN');"
        )
        session_cookie = local_session_cookie()
        if session_cookie:
            await context.add_cookies([session_cookie])
        page = await context.new_page()
        page.on(
            "console",
            lambda message: console_errors.append(message.text)
            if message.type == "error"
            else None,
        )
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        page.on(
            "response",
            lambda response: failed_responses.append(
                {"status": response.status, "url": response.url}
            )
            if response.status >= 400
            else None,
        )

        await page.goto(f"{BASE_URL}/terminal", wait_until="domcontentloaded")
        await page.wait_for_function(
            "() => typeof openTerminalInPlacePresetSwitchDialog === 'function'"
            " && !state.loadingSessions"
            " && Boolean(state.activeSessionId)"
        )
        source_session_id = await page.evaluate("() => state.activeSessionId")
        options = await page.locator(
            "#terminal-project-command-select option"
        ).all_text_contents()
        assert "永久" in options
        assert "临时" in options
        assert "临时/新终端/fork" in options
        assert "临时/新终端/resume" in options
        assert "临时/原地/resume" in options
        assert "临时/原地/新对话" in options
        assert "临时/新终端/新对话" in options

        await page.click("#terminal-project-command-button")
        await page.click(
            "#terminal-project-command-menu "
            "button[data-project-action='permanent_switch_preset']"
        )
        await page.wait_for_selector("#terminal-permanent-preset-dialog[open]")
        await page.wait_for_function(
            "() => Boolean(document.querySelector('#terminal-permanent-preset-select').value)"
        )
        permanent_desktop = await permanent_dialog_geometry(page)
        assert permanent_desktop["open"] and permanent_desktop["inViewport"]
        assert not permanent_desktop["pageOverflow"]
        await page.screenshot(
            path="/tmp/webclx-permanent-preset-switch-1440.png", full_page=True
        )
        await page.set_viewport_size({"width": 375, "height": 812})
        permanent_mobile = await permanent_dialog_geometry(page)
        assert permanent_mobile["open"] and permanent_mobile["inViewport"]
        assert not permanent_mobile["pageOverflow"]
        await page.screenshot(
            path="/tmp/webclx-permanent-preset-switch-375.png", full_page=True
        )
        await page.click("#terminal-permanent-preset-close")
        await page.set_viewport_size({"width": 1440, "height": 900})

        await page.evaluate(
            f"""() => {{
                const sourceSessionId = state.activeSessionId;
                const renderListeners = new Set();
                let cursorLine = 'Working';
                const sourceContext = {{
                    sessionId: sourceSessionId,
                    term: {{
                        buffer: {{
                            active: {{
                                baseY: 0,
                                cursorY: 0,
                                getLine: () => ({{
                                    translateToString: () => cursorLine,
                                }}),
                            }},
                        }},
                        onRender(callback) {{
                            renderListeners.add(callback);
                            return {{ dispose: () => renderListeners.delete(callback) }};
                        }},
                    }},
                }};
                window.__inPlacePresetCalls = [];
                window.__persistDetectedSession = true;
                const originalRequestJson = requestJson;
                requestJson = async (endpoint, options) => {{
                    if (endpoint === '/api/terminal/codex-conversations') {{
                        return {{
                            conversations: window.__persistDetectedSession
                                ? [{{ session_id: '{RESUME_ID}' }}]
                                : [],
                        }};
                    }}
                    return originalRequestJson(endpoint, options);
                }};
                waitForTerminalToolSessionReady = async (sessionId) => {{
                    window.__inPlacePresetCalls.push(['ready', sessionId]);
                }};
                ensureTerminalSessionCache = () => ({{
                    get: (sessionId) => sessionId === sourceSessionId ? sourceContext : null,
                }});
                detectAgentResumeIdComplete = async (sessionId, context) => {{
                    window.__inPlacePresetCalls.push([
                        'detect', sessionId, context === sourceContext,
                    ]);
                    return {{
                        resumeId: '{RESUME_ID}',
                        command: 'codex resume {RESUME_ID}',
                        program: 'codex',
                        source: 'browser_qa',
                    }};
                }};
                sendSlashCommand = (command, options) => {{
                    window.__inPlacePresetCalls.push([
                        'exit', command, options.sessionId,
                    ]);
                    mobileKeySendQueue = Promise.resolve().then(() => {{
                        cursorLine = '[root@host webClx]#';
                        window.__inPlacePresetCalls.push('exit-complete');
                        for (const listener of renderListeners) listener({{ start: 0, end: 0 }});
                    }});
                    return true;
                }};
                sendTerminalAutoTypedInput = async (command, options) => {{
                    window.__inPlacePresetCalls.push([
                        'resume', command, options.sessionId, options.throwOnError,
                    ]);
                    return true;
                }};
            }}"""
        )

        await page.click("#terminal-project-command-button")
        await page.click(
            "#terminal-project-command-menu "
            "button[data-project-action='switch_preset_in_terminal']"
        )
        await page.wait_for_selector("#terminal-in-place-preset-dialog[open]")
        await page.wait_for_function(
            "() => Boolean(document.querySelector('#terminal-in-place-preset-select').value)"
        )
        calls_before_submit = await page.evaluate("() => window.__inPlacePresetCalls")
        assert calls_before_submit == [
            ["ready", source_session_id],
            ["detect", source_session_id, True],
        ]
        assert (
            await page.locator("#terminal-in-place-preset-session").input_value()
            == RESUME_ID
        )
        desktop = await dialog_geometry(page)
        assert desktop["open"] and desktop["inViewport"]
        assert not desktop["pageOverflow"]
        await page.screenshot(
            path="/tmp/webclx-in-place-preset-switch-1440.png", full_page=True
        )

        await page.set_viewport_size({"width": 375, "height": 812})
        mobile = await dialog_geometry(page)
        assert mobile["open"] and mobile["inViewport"]
        assert not mobile["pageOverflow"]
        await page.screenshot(
            path="/tmp/webclx-in-place-preset-switch-375.png", full_page=True
        )

        selected_preset = await page.locator(
            "#terminal-in-place-preset-select"
        ).input_value()
        await page.click("#terminal-in-place-preset-submit")
        await page.wait_for_function(
            "() => !document.querySelector('#terminal-in-place-preset-dialog').open"
        )
        calls = await page.evaluate("() => window.__inPlacePresetCalls")
        assert calls[:4] == [
            ["ready", source_session_id],
            ["detect", source_session_id, True],
            ["exit", "/exit", source_session_id],
            "exit-complete",
        ]
        assert calls[4] == [
            "resume",
            f"webclx run api '{selected_preset}' -- codex resume {RESUME_ID}",
            source_session_id,
            True,
        ], calls

        await page.evaluate(
            "() => {"
            " window.__inPlacePresetCalls = [];"
            " window.__persistDetectedSession = false;"
            "}"
        )
        await page.click("#terminal-project-command-button")
        await page.click(
            "#terminal-project-command-menu "
            "button[data-project-action='switch_preset_in_terminal']"
        )
        await page.wait_for_selector("#terminal-in-place-preset-dialog[open]")
        await page.wait_for_function(
            "() => Boolean(document.querySelector('#terminal-in-place-preset-select').value)"
        )
        assert await page.locator("#terminal-in-place-preset-title").inner_text() == "终端内临时切换预设（新会话）"
        assert (
            await page.locator("#terminal-in-place-preset-session").input_value()
            == "新会话（当前 Session 尚未持久化）"
        )
        assert "尚无可恢复的对话记录" in await page.locator(
            "#terminal-status"
        ).inner_text()
        await page.evaluate("() => { terminalInPlacePresetTarget.agentExited = true; }")
        await page.click("#terminal-in-place-preset-submit")
        await page.wait_for_function(
            "() => !document.querySelector('#terminal-in-place-preset-dialog').open"
        )
        unpersisted_calls = await page.evaluate("() => window.__inPlacePresetCalls")
        unpersisted_preset = await page.locator(
            "#terminal-in-place-preset-select"
        ).input_value()
        assert unpersisted_calls[2] == [
            "resume",
            f"webclx run api '{unpersisted_preset}' -- codex",
            source_session_id,
            True,
        ]

        await page.evaluate(
            "() => {"
            " window.__inPlacePresetCalls = [];"
            " window.__persistDetectedSession = true;"
            "}"
        )
        await page.click("#terminal-project-command-button")
        menu_actions = await page.locator(
            "#terminal-project-command-menu button[data-project-action]"
        ).evaluate_all("buttons => buttons.map(button => button.dataset.projectAction)")
        resume_index = menu_actions.index("switch_preset_in_terminal")
        assert menu_actions[resume_index + 1] == "switch_preset_in_terminal_new_session"
        await page.click(
            "#terminal-project-command-menu "
            "button[data-project-action='switch_preset_in_terminal_new_session']"
        )
        await page.wait_for_selector("#terminal-in-place-preset-dialog[open]")
        await page.wait_for_function(
            "() => Boolean(document.querySelector('#terminal-in-place-preset-select').value)"
        )
        assert await page.locator("#terminal-in-place-preset-title").inner_text() == "终端内临时切换预设（新会话）"
        assert await page.locator("#terminal-in-place-preset-session").input_value() == "新会话（不恢复）"
        assert await page.locator("#terminal-in-place-preset-submit").inner_text() == "切换并新建"
        await page.evaluate("() => { terminalInPlacePresetTarget.agentExited = true; }")
        new_session_preset = await page.locator(
            "#terminal-in-place-preset-select"
        ).input_value()
        await page.click("#terminal-in-place-preset-submit")
        await page.wait_for_function(
            "() => !document.querySelector('#terminal-in-place-preset-dialog').open"
        )
        new_session_calls = await page.evaluate("() => window.__inPlacePresetCalls")
        assert new_session_calls[:2] == [
            ["ready", source_session_id],
            ["detect", source_session_id, True],
        ]
        assert new_session_calls[2] == [
            "resume",
            f"webclx run api '{new_session_preset}' -- codex",
            source_session_id,
            True,
        ]
        assert " resume " not in new_session_calls[2][1]

        await browser.close()

    optional_icon_failures = [
        response
        for response in failed_responses
        if response["status"] == 404 and "/api/workspace-icon?" in response["url"]
    ]
    unexpected_failed_responses = [
        response
        for response in failed_responses
        if response not in optional_icon_failures
    ]
    unexpected_console_errors = list(console_errors)
    for _ in optional_icon_failures:
        try:
            unexpected_console_errors.remove(
                "Failed to load resource: the server responded with a status of 404 (Not Found)"
            )
        except ValueError:
            break

    assert not page_errors, page_errors
    assert not unexpected_console_errors, unexpected_console_errors
    assert not unexpected_failed_responses, unexpected_failed_responses
    print(
        json.dumps(
            {
                "source_session_id": source_session_id,
                "permanent_desktop": permanent_desktop,
                "permanent_mobile": permanent_mobile,
                "desktop": desktop,
                "mobile": mobile,
                "calls": calls,
                "page_errors": page_errors,
                "console_errors": unexpected_console_errors,
                "failed_responses": unexpected_failed_responses,
                "ignored_optional_icon_404s": len(optional_icon_failures),
            },
            ensure_ascii=False,
            sort_keys=True,
        )
    )


if __name__ == "__main__":
    asyncio.run(main())
