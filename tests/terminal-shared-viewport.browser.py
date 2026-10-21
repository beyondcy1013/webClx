import asyncio
import base64
import hashlib
import hmac
import json
import os
import subprocess
import sys
import time
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


BASE_URL = "http://127.0.0.1:11111"
WORKSPACE_PATH = "webClx"
CHROMIUM = "/home/third_party/browser-tools/bin/chromium"


def local_session_cookie():
    secret_file = os.environ.get(
        "WEBCLX_SESSION_SECRET_FILE",
        "/home/bin/webclx/.webclx-session-secret",
    )
    credentials_file = os.environ.get(
        "WEBCLX_CREDENTIALS_FILE",
        "/home/bin/webclx/.webclx-login-credentials.json",
    )
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
        {"user": username, "exp": int(time.time()) + 600},
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


async def terminal_screen_lines(page):
    return await page.evaluate(
        """() => {
            const term = activeTerminalContext.term;
            const buffer = term.buffer.active;
            const lines = [];
            for (
                let row = buffer.viewportY;
                row < buffer.viewportY + term.rows;
                row += 1
            ) {
                lines.push(buffer.getLine(row)?.translateToString(true) || '');
            }
            return lines;
        }"""
    )


async def main():
    created_session_id = None
    marker = f"viewport-qa-{int(time.time() * 1000)}"
    page_errors = {"desktop": [], "mobile": []}
    result = {}

    async with async_playwright() as playwright:
        session_cookie = local_session_cookie()
        request = await playwright.request.new_context(
            base_url=BASE_URL,
            extra_http_headers={
                "Cookie": f"{session_cookie['name']}={session_cookie['value']}"
            },
        )
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
            await desktop_context.add_cookies([session_cookie])
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
            await mobile_context.add_cookies([session_cookie])
            mobile = await mobile_context.new_page()
            mobile.on("pageerror", lambda error: page_errors["mobile"].append(str(error)))
            await mobile.goto(terminal_url, wait_until="domcontentloaded")
            mobile_size = await wait_for_terminal(mobile, created_session_id)
            mobile_fit_size = await mobile.evaluate(
                """() => activeTerminalContext?.localTerminalFitSize
                    || (typeof fitAddon?.proposeDimensions === 'function'
                        ? fitAddon.proposeDimensions()
                        : null)"""
            )
            assert mobile_fit_size is not None, "fit addon did not expose viewport dimensions"
            assert mobile_fit_size["cols"] < desktop_size["cols"], {
                "desktop": desktop_size,
                "mobile_fit": mobile_fit_size,
            }
            await wait_for_tmux_size(created_session_id, desktop_size)
            shared_size = await asyncio.to_thread(tmux_pane_size, created_session_id)

            payload = """import sys,time
long='ABCDEFGHIJKLMNOPQRSTUVWXYZ-'*5
sys.stdout.write('\\x1b[?2026h\\x1b[H\\x1b[2JHEADER-'+long+'\\r\\nSECOND-'+long+'\\r\\nSTATUS-0000-'+long+'\\r\\nFOURTH-'+long+'\\r\\nRESULT-0000-'+long+'\\r\\n\\x1b[?2026l');sys.stdout.flush()
for i in range(1,501):
 sys.stdout.write(f'\\x1b[?2026h\\x1b[3;8H{i:04d}\\x1b[5;8H{i:04d}\\x1b[?2026l');sys.stdout.flush();time.sleep(0.002)
sys.stdout.write('\\x1b[?2026h\\x1b[6;1HSHARED_VIEWPORT_DONE\\x1b[?2026l');sys.stdout.flush()
"""
            encoded_payload = base64.b64encode(payload.encode()).decode()
            command = (
                "python3 -c \"import base64;"
                f"exec(base64.b64decode('{encoded_payload}'))\"\n"
            )
            await desktop.evaluate(
                "command => sendTerminalInput(command, { flush: true })",
                command,
            )
            for page in (desktop, mobile):
                await page.wait_for_function(
                    """() => {
                        const term = activeTerminalContext.term;
                        const buffer = term.buffer.active;
                        for (let row = 0; row < buffer.length; row += 1) {
                            if (buffer.getLine(row)?.translateToString(true)
                                .includes('SHARED_VIEWPORT_DONE')) {
                                return true;
                            }
                        }
                        return false;
                    }""",
                    timeout=15_000,
                )
                await page.wait_for_function(
                    """() => !activeTerminalContext.outputWriteInFlight
                        && activeTerminalContext.outputQueue.length === 0""",
                    timeout=10_000,
                )

            desktop_lines = await terminal_screen_lines(desktop)
            mobile_lines = await terminal_screen_lines(mobile)
            assert any("STATUS-0500" in line for line in desktop_lines), desktop_lines
            assert any("STATUS-0500" in line for line in mobile_lines), mobile_lines
            assert mobile_size == desktop_size, {
                "desktop": desktop_size,
                "mobile": mobile_size,
                "mobile_fit": mobile_fit_size,
            }
            await mobile.screenshot(
                path="/tmp/webclx-terminal-shared-viewport-mobile.png",
                full_page=True,
            )

            await desktop_context.close()
            desktop_context = None
            await wait_for_tmux_size(created_session_id, mobile_fit_size)
            mobile_only_size = await asyncio.to_thread(tmux_pane_size, created_session_id)

            assert not page_errors["desktop"], page_errors
            assert not page_errors["mobile"], page_errors
            result = {
                "session_id": created_session_id,
                "desktop": desktop_size,
                "mobile": mobile_size,
                "mobile_fit": mobile_fit_size,
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
