import json
import os
from pathlib import Path

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
        context = browser.new_context(viewport={"width": 1440, "height": 1000})
        context.grant_permissions(
            ["clipboard-read", "clipboard-write"],
            origin=BASE_URL,
        )
        page = context.new_page()
        page_errors = []
        failed_responses = []
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        page.on(
            "response",
            lambda response: failed_responses.append(
                {"status": response.status, "url": response.url}
            )
            if response.status >= 400
            and response.url.startswith(BASE_URL)
            and not (
                response.status == 404
                and response.url.startswith(f"{BASE_URL}/api/workspace-icon?")
            )
            else None,
        )

        sessions_response = context.request.get(f"{BASE_URL}/api/terminal/sessions?all=true")
        assert sessions_response.ok, sessions_response.text()
        sessions = [
            session
            for session in sessions_response.json().get("sessions", [])
            if not session.get("idle") and (session.get("path") or "")
        ]
        assert sessions, "browser regression requires a non-root active terminal session"
        requested_session_id = os.environ.get("WEBCLX_TEST_SESSION_ID", "").strip()
        if requested_session_id:
            target = next(
                (session for session in sessions if session.get("id") == requested_session_id),
                None,
            )
            assert target is not None, f"requested active terminal not found: {requested_session_id}"
        else:
            target = sessions[0]

        directory_response = context.request.get(
            f"{BASE_URL}/api/terminal/sessions/{target['id']}/current-directory"
        )
        assert directory_response.ok, directory_response.text()
        directory = directory_response.json()

        context.add_init_script(
            f"""
            localStorage.setItem(
              "webclx:last-terminal-sessions",
              JSON.stringify({{"__global_last_opened__": {json.dumps(target['id'])}}})
            );
            """
        )
        page.goto(f"{BASE_URL}/downloads", wait_until="domcontentloaded")
        page.locator('a[href="/workspace"]').first.click()
        page.wait_for_url(f"{BASE_URL}/workspace**")
        page.wait_for_function(
            """
            expected => state.currentPath === expected.path
              && state.returnTerminalSessionId === expected.sessionId
              && state.preferredSessionId === expected.sessionId
              && state.currentDirectory?.display_path === expected.displayPath
              && document.querySelector("#current-path")?.textContent.trim() === expected.displayPath
              && document.querySelector("#sessions-session-list")?.value === expected.sessionId
            """,
            arg={
                "path": directory.get("path") or "",
                "displayPath": directory.get("display_path") or "",
                "sessionId": target["id"],
            },
            timeout=20_000,
        )

        page.locator("#copy-current-path").click()
        workspace_clipboard = page.evaluate("navigator.clipboard.readText()")
        assert workspace_clipboard == directory.get("display_path"), workspace_clipboard

        page.locator("#tab-workspace-history").click()
        page.wait_for_function(
            """
            () => {
              const select = document.querySelector("#workspace-history-path-select");
              return document.querySelector("#workspace-history-view")?.classList.contains("active")
                && select && !select.disabled && Boolean(select.value);
            }
            """,
            timeout=20_000,
        )
        selected_history_path = page.locator("#workspace-history-path-select").input_value()
        page.locator("#copy-workspace-history-path").click()
        history_clipboard = page.evaluate("navigator.clipboard.readText()")
        assert history_clipboard == selected_history_path, history_clipboard

        desktop_screenshot = "/tmp/webclx-history-current-directory-copy-desktop.png"
        page.screenshot(path=desktop_screenshot, full_page=True)
        page.set_viewport_size({"width": 375, "height": 812})
        mobile_screenshot = "/tmp/webclx-history-current-directory-copy-mobile.png"
        page.screenshot(path=mobile_screenshot, full_page=True)
        assert not page_errors, page_errors
        assert not failed_responses, failed_responses
        print(
            json.dumps(
                {
                    "ok": True,
                    "session_id": target["id"],
                    "session_name": target.get("name") or "",
                    "current_path": directory.get("path") or "",
                    "display_path": directory.get("display_path") or "",
                    "workspace_clipboard": workspace_clipboard,
                    "history_selected_path": selected_history_path,
                    "history_clipboard": history_clipboard,
                    "final_url": page.url,
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
