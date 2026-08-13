import asyncio
import json
import subprocess
import sys
import time

from playwright.async_api import async_playwright


BASE_URL = "http://127.0.0.1:11111"
WORKSPACE_PATH = "webClx"
CHROMIUM = "/home/third_party/browser-tools/bin/chromium"


async def response_json(response, action):
    body = await response.json()
    assert response.ok, f"{action} failed ({response.status}): {body}"
    return body


def tmux_pane_size(session_id):
    result = subprocess.run(
        [
            "tmux",
            "display-message",
            "-p",
            "-t",
            f"webclx_{session_id}",
            "#{pane_width} #{pane_height}",
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    cols, rows = result.stdout.strip().split()
    return {"cols": int(cols), "rows": int(rows)}


async def wait_for_tmux_size(session_id, expected, timeout_seconds=5):
    deadline = time.monotonic() + timeout_seconds
    last_size = None
    while time.monotonic() < deadline:
        try:
            last_size = await asyncio.to_thread(tmux_pane_size, session_id)
        except subprocess.CalledProcessError:
            last_size = None
        if last_size == expected:
            return last_size
        await asyncio.sleep(0.05)
    raise AssertionError(f"tmux size did not become {expected}; last size was {last_size}")


async def wait_for_terminal(page, session_id):
    await page.wait_for_function(
        """sessionId =>
            state.activeSessionId === sessionId &&
            activeTerminalContext?.sessionId === sessionId &&
            activeTerminalContext?.socket?.readyState === WebSocket.OPEN &&
            activeTerminalContext?.hasLoadedOutput &&
            !activeTerminalContext?.initialReplayPending &&
            !activeTerminalContext?.backlogReplayActive &&
            !activeTerminalContext?.outputWriteInFlight &&
            activeTerminalContext?.term?.cols > 0 &&
            activeTerminalContext?.term?.rows > 0""",
        arg=session_id,
        timeout=15_000,
    )
    return await page.evaluate(
        """() => ({
            cols: activeTerminalContext.term.cols,
            rows: activeTerminalContext.term.rows,
        })"""
    )


async def main():
    created_session_id = None
    marker = f"viewport-qa-{int(time.time() * 1000)}"
    page_errors = {"desktop": [], "mobile": []}
    result = {}

    async with async_playwright() as playwright:
        request = await playwright.request.new_context(base_url=BASE_URL)
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        desktop_context = None
        mobile_context = None
        try:
            created = await request.post(
                "/api/terminal/sessions",
                data={"path": WORKSPACE_PATH},
            )
            created_body = await response_json(created, "create terminal session")
            created_session_id = created_body["id"]
            assert created_body["path"] == WORKSPACE_PATH, created_body

            renamed = await request.put(
                f"/api/terminal/sessions/{created_session_id}",
                data={"name": marker},
            )
            renamed_body = await response_json(renamed, "rename terminal session")
            assert renamed_body["id"] == created_session_id, renamed_body
            assert renamed_body["name"] == marker, renamed_body

            terminal_url = (
                f"{BASE_URL}/terminal?path={WORKSPACE_PATH}&session={created_session_id}"
            )
            desktop_context = await browser.new_context(
                viewport={"width": 1440, "height": 1000}
            )
            desktop = await desktop_context.new_page()
            desktop.on("pageerror", lambda error: page_errors["desktop"].append(str(error)))
            await desktop.goto(terminal_url, wait_until="domcontentloaded")
            desktop_size = await wait_for_terminal(desktop, created_session_id)
            await wait_for_tmux_size(created_session_id, desktop_size)
            await desktop.screenshot(
                path="/tmp/webclx-terminal-shared-viewport-desktop.png",
                full_page=True,
            )

            mobile_context = await browser.new_context(
                viewport={"width": 390, "height": 844},
                is_mobile=True,
                has_touch=True,
            )
            mobile = await mobile_context.new_page()
            mobile.on("pageerror", lambda error: page_errors["mobile"].append(str(error)))
            await mobile.goto(terminal_url, wait_until="domcontentloaded")
            mobile_size = await wait_for_terminal(mobile, created_session_id)
            assert mobile_size["cols"] < desktop_size["cols"], {
                "desktop": desktop_size,
                "mobile": mobile_size,
            }
            await wait_for_tmux_size(created_session_id, desktop_size)
            shared_size = await asyncio.to_thread(tmux_pane_size, created_session_id)
            await mobile.screenshot(
                path="/tmp/webclx-terminal-shared-viewport-mobile.png",
                full_page=True,
            )

            await desktop_context.close()
            desktop_context = None
            await wait_for_tmux_size(created_session_id, mobile_size)
            mobile_only_size = await asyncio.to_thread(tmux_pane_size, created_session_id)

            assert not page_errors["desktop"], page_errors
            assert not page_errors["mobile"], page_errors
            result = {
                "session_id": created_session_id,
                "desktop": desktop_size,
                "mobile": mobile_size,
                "shared_while_both_visible": shared_size,
                "mobile_after_desktop_closed": mobile_only_size,
                "page_errors": page_errors,
            }
        finally:
            if desktop_context is not None:
                await desktop_context.close()
            if mobile_context is not None:
                await mobile_context.close()
            await browser.close()

            if created_session_id is not None:
                sessions_response = await request.get(
                    f"/api/terminal/sessions?path={WORKSPACE_PATH}"
                )
                sessions_body = await response_json(sessions_response, "verify cleanup target")
                cleanup_target = next(
                    (
                        session
                        for session in sessions_body.get("sessions", [])
                        if session.get("id") == created_session_id
                        and session.get("path") == WORKSPACE_PATH
                        and session.get("name") == marker
                    ),
                    None,
                )
                if cleanup_target is not None:
                    deleted = await request.delete(
                        f"/api/terminal/sessions/{created_session_id}",
                        headers={
                            "X-WebClx-Confirm-Session": created_session_id,
                            "X-WebClx-Delete-Source": "browser-qa",
                        },
                    )
                    await response_json(deleted, "delete terminal session")
                else:
                    print(
                        json.dumps(
                            {
                                "cleanup_skipped": created_session_id,
                                "reason": "created session no longer matches its path and marker",
                            },
                            sort_keys=True,
                        ),
                        file=sys.stderr,
                    )
            await request.dispose()

    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    asyncio.run(main())
