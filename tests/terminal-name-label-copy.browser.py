import json
import os
from pathlib import Path
from urllib.parse import quote

from playwright.sync_api import sync_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111").rstrip("/")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/third_party/browser-tools/bin/chromium",
)


def main():
    launch_options = {"headless": True, "args": ["--no-sandbox", "--disable-gpu"]}
    chromium_path = Path(CHROMIUM)
    if chromium_path.is_file():
        launch_options["executable_path"] = str(chromium_path)

    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(**launch_options)
        context = browser.new_context(viewport={"width": 1440, "height": 900})
        context.grant_permissions(
            ["clipboard-read", "clipboard-write"],
            origin=BASE_URL,
        )
        page = context.new_page()
        page_errors = []
        page.on("pageerror", lambda error: page_errors.append(str(error)))

        sessions_response = context.request.get(f"{BASE_URL}/api/terminal/sessions?all=true")
        assert sessions_response.ok, sessions_response.text()
        sessions = [session for session in sessions_response.json().get("sessions", []) if not session.get("idle")]
        requested_session_id = os.environ.get("WEBCLX_TEST_SESSION_ID", "").strip()
        target = next(
            (
                session
                for session in sessions
                if not requested_session_id or session.get("id") == requested_session_id
            ),
            None,
        )
        assert target is not None, f"active terminal not found: {requested_session_id or 'any'}"
        expected_name = str(target.get("name") or "").strip()
        assert expected_name, f"terminal has no name: {target.get('id')}"

        page.goto(
            f"{BASE_URL}/terminal?session={quote(target['id'])}",
            wait_until="domcontentloaded",
        )
        page.wait_for_function(
            "expected => state.activeSessionId === expected && document.querySelector('#session-switcher')?.value === expected",
            arg=target["id"],
            timeout=20_000,
        )
        page.locator("#copy-current-terminal-name").click()
        clipboard = page.evaluate("navigator.clipboard.readText()")
        assert clipboard == expected_name, {"expected": expected_name, "actual": clipboard}
        page.locator("#terminal-status").wait_for(state="visible")
        assert expected_name in page.locator("#terminal-status").inner_text()

        desktop_screenshot = "/tmp/webclx-terminal-name-copy-desktop.png"
        page.screenshot(path=desktop_screenshot, full_page=True)
        page.set_viewport_size({"width": 375, "height": 812})
        mobile_screenshot = "/tmp/webclx-terminal-name-copy-mobile.png"
        page.screenshot(path=mobile_screenshot, full_page=True)

        assert not page_errors, page_errors
        print(
            json.dumps(
                {
                    "ok": True,
                    "session_id": target["id"],
                    "terminal_name": expected_name,
                    "clipboard": clipboard,
                    "desktop_screenshot": desktop_screenshot,
                    "mobile_screenshot": mobile_screenshot,
                },
                ensure_ascii=False,
                sort_keys=True,
            )
        )
        context.close()
        browser.close()


if __name__ == "__main__":
    main()
