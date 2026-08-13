import asyncio
import json
import mimetypes
import os
import time
from pathlib import Path
from urllib.parse import parse_qs, urlparse

from playwright.async_api import TimeoutError as PlaywrightTimeoutError
from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = Path(os.environ.get("WEBCLX_TEST_STATIC_DIR", ROOT / "static")).resolve()
MOBILE = os.environ.get("WEBCLX_TEST_MOBILE") == "1"
RECONNECT = os.environ.get("WEBCLX_TEST_RECONNECT") == "1"
LEGACY_ANDROID = os.environ.get("WEBCLX_TEST_ANDROID_LEGACY") == "1"
ANDROID = os.environ.get("WEBCLX_TEST_ANDROID") == "1" or LEGACY_ANDROID
SESSIONS = [
    {
        "id": "session-a",
        "name": "Terminal A",
        "path": "demo",
        "display_path": "/demo",
        "idle": False,
        "connected": True,
    },
    {
        "id": "session-b",
        "name": "Terminal B",
        "path": "demo",
        "display_path": "/demo",
        "idle": False,
        "connected": True,
    },
]


def terminal_payload(session_id):
    lines = [f"{session_id}-line-{index:03d}" for index in range(140)]
    lines.append(f"{session_id}-ready")
    return ("\r\n".join(lines) + "\r\n").encode()


async def main():
    websocket_routes = {}
    websocket_messages = {}
    console_errors = []
    page_errors = []

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path="/home/third_party/browser-tools/bin/chromium",
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        page = await browser.new_page(
            viewport={"width": 390, "height": 844} if MOBILE else {"width": 1440, "height": 1000},
            is_mobile=MOBILE,
            has_touch=MOBILE,
            user_agent=(
                "Mozilla/5.0 (Linux; Android 14; Pixel Build/UP1A; wv) "
                "AppleWebKit/537.36 Version/4.0 Chrome/126.0 Mobile Safari/537.36"
                if LEGACY_ANDROID
                else (
                    "Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 "
                    "Chrome/126.0 Mobile Safari/537.36 webClxAndroid/1.0.0"
                    if ANDROID else None
                )
            ),
        )
        page.on("console", lambda message: console_errors.append(message.text) if message.type == "error" else None)
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
            if parsed.path == "/api/terminal/sessions":
                await route.fulfill(
                    json={"path": "demo", "display_path": "/demo", "sessions": SESSIONS},
                )
                return
            if parsed.path == "/api/settings":
                await route.fulfill(
                    json={
                        "terminal_scrollback_lines": 10000,
                        "terminal_fab_auto_expand": True,
                        "theme_mode": "dark",
                    },
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
            parsed = urlparse(websocket.url)
            session_id = parse_qs(parsed.query).get("session_id", [""])[0]
            websocket_routes.setdefault(session_id, []).append(websocket)
            connection_index = len(websocket_routes[session_id]) - 1
            websocket_messages.setdefault(session_id, [])
            websocket.on_message(
                lambda message: websocket_messages[session_id].append(message)
            )

            async def replay():
                await asyncio.sleep(0.05)
                websocket.send(json.dumps({"type": "terminal_backlog_replay", "action": "start"}))
                if RECONNECT and session_id == "session-a" and connection_index > 0:
                    for index in range(140):
                        websocket.send(f"{session_id}-reconnect-{index:03d}\r\n".encode())
                        await asyncio.sleep(0.02)
                else:
                    websocket.send(terminal_payload(session_id))
                websocket.send(json.dumps({"type": "terminal_backlog_replay", "action": "end"}))

            asyncio.create_task(replay())

        await page.route("**/*", handle_http)
        await page.route_web_socket("**/api/terminal/ws?**", handle_websocket)
        await page.goto("http://webclx.test/terminal?path=demo&session=session-a")
        try:
            await page.wait_for_function(
                """() => activeTerminalContext?.sessionId === 'session-a' &&
                    activeTerminalContext.hasLoadedOutput &&
                    !activeTerminalContext.outputWriteInFlight""",
                timeout=10000,
            )
        except PlaywrightTimeoutError as error:
            runtime = await page.evaluate(
                """() => ({
                    activeSessionId: typeof state === 'undefined' ? null : state.activeSessionId,
                    contextSessionId: typeof activeTerminalContext === 'undefined' ? null : activeTerminalContext?.sessionId,
                    hasLoadedOutput: typeof activeTerminalContext === 'undefined' ? null : activeTerminalContext?.hasLoadedOutput,
                    replayActive: typeof activeTerminalContext === 'undefined' ? null : activeTerminalContext?.backlogReplayActive,
                    status: document.querySelector('#terminal-status')?.textContent || '',
                })""",
            )
            raise AssertionError(
                json.dumps(
                    {
                        "runtime": runtime,
                        "websockets": sorted(websocket_routes),
                        "page_errors": page_errors,
                        "console_errors": console_errors,
                    },
                    ensure_ascii=False,
                )
            ) from error

        await page.evaluate(
            """() => {
                terminalSessionCache.get('session-a').term.element.dataset.cacheProbe = 'session-a-node';
            }""",
        )
        await page.select_option("#session-switcher", "session-b")
        await page.wait_for_function(
            """() => activeTerminalContext?.sessionId === 'session-b' &&
                activeTerminalContext.hasLoadedOutput &&
                !activeTerminalContext.outputWriteInFlight""",
        )
        await page.wait_for_timeout(50)

        await page.evaluate(
            """() => {
                const context = terminalSessionCache.get('session-b');
                context.term.element.dataset.outputRefreshCalls = '0';
                const originalRefresh = context.term.refresh.bind(context.term);
                context.term.refresh = (...args) => {
                    context.term.element.dataset.outputRefreshCalls = String(
                        Number(context.term.element.dataset.outputRefreshCalls || '0') + 1,
                    );
                    return originalRefresh(...args);
                };
            }""",
        )
        websocket_routes["session-b"][0].send(b"\r\nVISIBLE-WITHOUT-INPUT\r\n")
        await page.wait_for_function(
            """() => {
                const context = terminalSessionCache.get('session-b');
                const buffer = context.term.buffer.active;
                const hasMarker = Array.from({ length: buffer.length }, (_, row) =>
                    buffer.getLine(row)?.translateToString(true) || ''
                ).some((line) => line.includes('VISIBLE-WITHOUT-INPUT'));
                return hasMarker &&
                    Number(context.term.element.dataset.outputRefreshCalls || '0') > 0 &&
                    !document.querySelector('#terminal-host')?.classList.contains('terminal-host-replaying');
            }""",
        )

        await page.evaluate(
            """() => {
                terminalSessionCache.get('session-b').term.element.dataset.outputRefreshCalls = '0';
            }""",
        )
        await page.evaluate("() => activeTerminalContext.term.focus()")
        await page.keyboard.type("typed-repaint-probe")
        await page.wait_for_function(
            """() => Number(
                terminalSessionCache.get('session-b').term.element.dataset.outputRefreshCalls || '0'
            ) > 0""",
        )
        input_deadline = time.monotonic() + 2
        typed_input = ""
        while time.monotonic() < input_deadline:
            typed_input = "".join(
                json.loads(message).get("data", "")
                for message in websocket_messages["session-b"]
                if isinstance(message, str) and json.loads(message).get("type") == "input"
            )
            if "typed-repaint-probe" in typed_input:
                break
            await asyncio.sleep(0.02)
        assert "typed-repaint-probe" in typed_input, typed_input

        websocket_routes["session-a"][0].send(b"\r\nBACKGROUND-A-MARKER\r\n")
        await page.wait_for_function(
            """() => {
                const buffer = terminalSessionCache.get('session-a').term.buffer.active;
                for (let row = 0; row < buffer.length; row += 1) {
                    if (buffer.getLine(row)?.translateToString(true).includes('BACKGROUND-A-MARKER')) return true;
                }
                return false;
            }""",
        )
        assert await page.evaluate(
            "() => terminalSessionCache.get('session-a').term.element.hidden"
        )

        if RECONNECT:
            await websocket_routes["session-a"][0].close()
            reconnect_deadline = time.monotonic() + 5
            while len(websocket_routes["session-a"]) < 2 and time.monotonic() < reconnect_deadline:
                await asyncio.sleep(0.05)
            assert len(websocket_routes["session-a"]) == 2, websocket_routes["session-a"]
            await page.wait_for_function(
                """() => {
                    const context = terminalSessionCache.get('session-a');
                    return context?.backlogReplayActive && context.outputWriteInFlight;
                }""",
                timeout=5000,
            )

        await page.evaluate(
            """() => {
                const context = terminalSessionCache.get('session-a');
                context.term.element.dataset.scrollToBottomCalls = '0';
                const originalScrollToBottom = context.term.scrollToBottom.bind(context.term);
                context.term.scrollToBottom = () => {
                    const element = context.term.element;
                    element.dataset.scrollToBottomCalls = String(
                        Number(element.dataset.scrollToBottomCalls || '0') + 1,
                    );
                    return originalScrollToBottom();
                };
            }""",
        )

        switch_started_at = time.monotonic()
        retained_repaint_ms = None
        await page.select_option("#session-switcher", "session-a")
        await page.wait_for_function("() => activeTerminalContext?.sessionId === 'session-a'")
        if RECONNECT:
            visible_probe = await page.evaluate(
                """() => Array.from(document.querySelectorAll('#terminal-host > .xterm'))
                    .find((element) => !element.hidden && getComputedStyle(element).display !== 'none')
                    ?.dataset.cacheProbe || ''""",
            )
            assert visible_probe == "session-a-node", visible_probe
            retained_repaint_ms = round((time.monotonic() - switch_started_at) * 1000)
            await page.wait_for_function(
                """() => {
                    const context = terminalSessionCache.get('session-a');
                    return context?.hasLoadedOutput && !context.backlogReplayActive &&
                        !context.outputWriteInFlight;
                }""",
                timeout=10000,
            )
        await page.wait_for_timeout(50)
        await page.wait_for_function(
            """(android) => {
                const context = terminalSessionCache.get('session-a');
                if (!context) return false;
                if (android) {
                    return context.term.element.querySelectorAll('canvas').length === 0 &&
                        (context.term.element.querySelector('.xterm-rows')?.textContent || '')
                            .includes('session-a-ready');
                }
                let paintedPixels = 0;
                context.term.element.querySelectorAll('canvas').forEach((canvas) => {
                    const canvasContext = canvas.getContext('2d', { willReadFrequently: true });
                    if (!canvasContext || canvas.width === 0 || canvas.height === 0) return;
                    const pixels = canvasContext.getImageData(0, 0, canvas.width, canvas.height).data;
                    const [red, green, blue, alpha] = pixels;
                    for (let index = 4; index < pixels.length; index += 4) {
                        if (pixels[index] !== red || pixels[index + 1] !== green ||
                            pixels[index + 2] !== blue || pixels[index + 3] !== alpha) {
                            paintedPixels += 1;
                        }
                    }
                });
                return paintedPixels > 500;
            }""",
            arg=ANDROID,
            timeout=1000,
        )
        repaint_ms = round((time.monotonic() - switch_started_at) * 1000)
        result = await page.evaluate(
            """() => {
                const context = terminalSessionCache.get('session-a');
                const buffer = context.term.buffer.active;
                const canvases = Array.from(context.term.element.querySelectorAll('canvas'));
                let paintedPixels = 0;
                canvases.forEach((canvas) => {
                    const canvasContext = canvas.getContext('2d', { willReadFrequently: true });
                    if (!canvasContext || canvas.width === 0 || canvas.height === 0) return;
                    const pixels = canvasContext.getImageData(0, 0, canvas.width, canvas.height).data;
                    const red = pixels[0];
                    const green = pixels[1];
                    const blue = pixels[2];
                    const alpha = pixels[3];
                    for (let index = 4; index < pixels.length; index += 4) {
                        if (pixels[index] !== red || pixels[index + 1] !== green ||
                            pixels[index + 2] !== blue || pixels[index + 3] !== alpha) {
                            paintedPixels += 1;
                        }
                    }
                });
                return {
                    cacheProbe: context.term.element.dataset.cacheProbe,
                    scrollToBottomCalls: Number(
                        context.term.element.dataset.scrollToBottomCalls || '0',
                    ),
                    atBottom: buffer.viewportY === buffer.baseY,
                    visibleXterms: Array.from(document.querySelectorAll('#terminal-host > .xterm'))
                        .filter((element) => !element.hidden && getComputedStyle(element).display !== 'none').length,
                    cachedContexts: terminalSessionCache.size,
                    paintedPixels,
                    rendererType: context.term.options.rendererType,
                    domTextVisible:
                        (context.term.element.querySelector('.xterm-rows')?.textContent || '')
                            .includes('session-a-ready'),
                    pageAtBottom:
                        Math.max(document.documentElement.scrollHeight - window.innerHeight, 0) -
                            window.scrollY <= 8,
                    pageScrollY: window.scrollY,
                    pageMaxScroll: Math.max(
                        document.documentElement.scrollHeight - window.innerHeight,
                        0,
                    ),
                    primaryPointerCoarse: matchMedia('(pointer: coarse)').matches,
                };
            }""",
        )

        if not RECONNECT:
            assert result["cacheProbe"] == "session-a-node"
            assert result["scrollToBottomCalls"] == 0, result
        assert result["atBottom"] is True
        assert result["visibleXterms"] == 1
        assert result["cachedContexts"] == 2
        if ANDROID:
            assert result["rendererType"] == "dom", result
            assert result["domTextVisible"] is True, result
            assert result["paintedPixels"] == 0, result
        else:
            assert result["rendererType"] == "canvas", result
            assert result["paintedPixels"] > 500, result
        assert result["pageAtBottom"] is True, result
        assert result["primaryPointerCoarse"] is MOBILE, result
        visible_repaint_ms = retained_repaint_ms if RECONNECT else repaint_ms
        assert visible_repaint_ms is not None and visible_repaint_ms < 1000, visible_repaint_ms
        assert len(websocket_routes["session-a"]) == (2 if RECONNECT else 1)
        assert len(websocket_routes["session-b"]) == 1
        visibility_messages = {
            session_id: [
                json.loads(message)["visible"]
                for message in messages
                if isinstance(message, str)
                and json.loads(message).get("type") == "visibility"
            ]
            for session_id, messages in websocket_messages.items()
        }
        assert visibility_messages["session-a"] == (
            [True, False, False, True] if RECONNECT else [True, False, True]
        )
        assert visibility_messages["session-b"] == [True, False]
        assert not page_errors, page_errors
        assert not console_errors, console_errors

        screenshot_path = (
            "/tmp/webclx-terminal-cache-mobile.png"
            if MOBILE
            else "/tmp/webclx-terminal-cache-desktop.png"
        )
        await page.screenshot(path=screenshot_path, full_page=True)
        await browser.close()

    result["repaintMs"] = retained_repaint_ms if RECONNECT else repaint_ms
    if RECONNECT:
        result["replayCommitMs"] = repaint_ms
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    asyncio.run(main())
