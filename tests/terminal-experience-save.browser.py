"""Exercise the real terminal UI with mocked APIs; no real terminal I/O."""
import asyncio
import mimetypes
from pathlib import Path
from urllib.parse import urlparse
from playwright.async_api import async_playwright

import os
import shutil

STATIC = Path(__file__).resolve().parents[1] / "static"
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    shutil.which("chromium") or "/home/third_party/browser-tools/bin/chromium",
)


async def check(browser, width, height):
    page = await browser.new_page(viewport={"width": width, "height": height})
    errors, mutations = [], []
    close_states = []
    hide_fails = False
    close_fails = False
    delay_close = False
    experience_send_fails = True
    page.on("pageerror", lambda error: errors.append(str(error)))
    sessions = [dict(id=f"s{i}", name=f"测试终端 {i}", path="tools/webClx",
                     display_path="/home/codes/tools/webClx", idle=False, connected=True,
                     created_at=i, last_opened_at=10-i, group_token="pair") for i in [1, 2, 3]]

    async def http(route):
        nonlocal sessions
        path = urlparse(route.request.url).path
        method = route.request.method
        if path == "/terminal":
            await route.fulfill(content_type="text/html", body=(STATIC / "terminal.html").read_bytes())
        elif path.startswith("/assets/"):
            asset = STATIC / path.removeprefix("/assets/")
            await route.fulfill(status=200 if asset.is_file() else 404,
                                content_type=mimetypes.guess_type(asset.name)[0] or "application/octet-stream",
                                body=asset.read_bytes() if asset.is_file() else b"")
        elif path == "/api/settings":
            await route.fulfill(json={"workspace_dir": "/home/codes", "terminal_auto_continue_on_error": False})
        elif path == "/api/terminal/sessions":
            await route.fulfill(json={"sessions": sessions, "path": "tools/webClx"})
        elif method != "GET":
            mutations.append((method, path, route.request.post_data_json))
            if path == "/api/terminal/auto-typed-input":
                if experience_send_fails:
                    await route.fulfill(status=503, body="测试落盘发送失败")
                else:
                    await route.fulfill(json={"data": route.request.post_data_json["command_line"]})
            elif path.endswith("/hide"):
                if hide_fails:
                    await route.fulfill(status=503, body="测试隐藏失败")
                    return
                session = next(s for s in sessions if s["id"] == path.split("/")[-2])
                session["hidden"] = True
                await route.fulfill(json=dict(session))
            elif path.endswith("/restore"):
                session = next(s for s in sessions if s["id"] == path.split("/")[-2])
                session["idle"] = False
                session["hidden"] = False
                await route.fulfill(json=dict(session))
            elif path.endswith("/close"):
                close_states.append(await page.evaluate("({active: state.activeSessionId, hidden: state.sessions.find(s => s.id === 's1')?.hidden})"))
                if delay_close:
                    await page.wait_for_function("window.releaseClose === true", timeout=5000)
                if close_fails:
                    await route.fulfill(status=503, body="测试关闭失败")
                    return
                session_id = path.split("/")[-2]
                sessions = [s for s in sessions if s["id"] != session_id]
                await route.fulfill(json={"ok": True, "session_id": session_id, "deleted": True})
            elif path == "/api/terminal/scheduled-inputs":
                await route.fulfill(status=503, body="测试排程失败")
            else:
                await route.fulfill(json={})
        elif path.endswith("resume-archives"):
            await route.fulfill(json={"archives": []})
        else:
            await route.fulfill(json={"tasks": [], "entries": []})

    await page.route("**/*", http)
    await page.route_web_socket("**/api/terminal/ws?**", lambda ws: None)
    await page.goto("http://webclx.test/terminal?path=tools/webClx&session=s1")
    await page.wait_for_function("state.activeSessionId === 's1' && state.sessions.length === 3")

    dialog = page.locator("#terminal-experience-save-dialog")

    # 独立“经验落盘”只发送并提交消息，不隐藏、关闭或切换当前终端。
    await page.locator("#terminal-tools-button").click()
    experience_btn = page.locator("#terminal-save-experience")
    await experience_btn.click()
    await page.wait_for_function("document.getElementById('terminal-tools-status').textContent.includes('测试落盘发送失败')")
    assert await experience_btn.is_enabled()
    assert await experience_btn.is_visible(), "send failure keeps menu available"
    experience_send_fails = False
    await experience_btn.click()
    await page.wait_for_function("document.getElementById('terminal-tools-menu').hidden")
    sent = [body for _, path, body in mutations if path == "/api/terminal/auto-typed-input"]
    assert len(sent) == 2 and all(body["session_id"] == "s1" for body in sent)
    prompt = sent[-1]["command_line"]
    assert "skill-bidirectional-sync" in prompt and ".agents/skills/<项目名>-ops/SKILL.md" in prompt
    assert "不要关闭或退出本终端" in prompt
    assert "webclx terminal close" not in prompt and "/close" not in prompt
    assert not any(path.endswith(("/hide", "/close", "/scheduled-inputs")) for _, path, _ in mutations)
    assert await page.evaluate("state.activeSessionId === 's1' && !state.sessions.find(s => s.id === 's1').hidden")
    mutations.clear()

    # 1. 点击“落盘并结束”：无需对话框，直接发起经验落盘并转入隐藏专区
    save_btn = page.locator("#save-experience-session")
    await save_btn.click()
    assert not await dialog.is_visible(), "dialog must not open when clicking 落盘并结束"
    await page.wait_for_function("state.sessions.find(s => s.id === 's1').hidden && state.activeSessionId === 's2'")
    assert not sessions[1].get("hidden"), "single hide must preserve partner"
    scheduled = [body for _, path, body in mutations if path == "/api/terminal/scheduled-inputs"]
    assert len(scheduled) == 1, scheduled
    assert scheduled[0]["session_id"] == "s1" and scheduled[0]["task_type"] == "agent_message"
    assert scheduled[0]["send_enter"] is True

    # 恢复 s1 进行先隐藏再关闭测试
    await page.evaluate("restoreHiddenSession('s1')")
    await page.wait_for_function("state.activeSessionId === 's1' && !state.sessions.find(s => s.id === 's1').hidden")

    # 2. 隐藏失败不得关闭，也不切走当前终端。
    delete_btn = page.locator("#delete-session")
    hide_fails = True
    await delete_btn.click()
    await page.wait_for_function("!deleteSessionButton.disabled && statusEl.textContent.includes('测试隐藏失败')")
    assert not any(path.endswith("/close") for _, path, _ in mutations)
    assert await page.evaluate("state.activeSessionId === 's1' && !state.sessions.find(s => s.id === 's1').hidden")
    hide_fails = False

    # 3. 关闭失败时，已切换到其他终端，目标保留在隐藏专区，可恢复重试。
    close_fails = True
    await delete_btn.click()
    await page.wait_for_function("!deleteSessionButton.disabled && state.activeSessionId === 's2' && statusEl.textContent.includes('测试关闭失败')")
    assert close_states == [{"active": "s2", "hidden": True}], close_states
    assert await page.evaluate("state.sessions.find(s => s.id === 's1').hidden")
    await page.evaluate("restoreHiddenSession('s1')")
    await page.wait_for_function("state.activeSessionId === 's1' && !state.sessions.find(s => s.id === 's1').hidden")
    close_fails = False
    mutations.clear()
    close_states.clear()

    # 4. Closing may be slow; switching must work before it finishes and its
    # completion must preserve the newer selection.
    delay_close = True
    await delete_btn.click()
    assert not await dialog.is_visible(), "dialog must not open when clicking 结束"
    await page.wait_for_function("state.activeSessionId === 's2' && state.sessions.find(s => s.id === 's1').hidden")
    assert await page.evaluate("!createSessionButton.disabled && !deleteSessionButton.disabled && !saveExperienceSessionButton.disabled"), "old close must not lock new work"
    # Repeating the old target must not send a second hide/close transaction.
    await page.evaluate("deleteSession(state.sessions.find(s => s.id === 's1'))")
    assert len([path for _, path, _ in mutations if path.endswith('/hide')]) == 1
    trigger = page.get_by_role("button", name="切换当前终端", exact=True)
    # Use the actual menu hit targets, rather than calling selectSession.
    await trigger.click()
    await page.locator('#session-switcher-image-menu [role="option"]').filter(has_text="测试终端 3").click()
    assert await page.evaluate("state.activeSessionId === 's3'")
    # Start real new work while the old terminal is still closing. Completion
    # must leave the new-session dialog open and preserve its focused control.
    await page.locator("#create-session").click(timeout=2000)
    new_dialog = page.locator("#terminal-specified-task-dialog")
    await new_dialog.wait_for(state="visible", timeout=2000)
    await page.evaluate("window.closeCompletionFocus = document.activeElement")
    await page.evaluate("window.releaseClose = true")
    await page.wait_for_function("state.sessions.every(s => s.id !== 's1') && state.activeSessionId === 's3'")
    assert await new_dialog.is_visible(), "background close must not dismiss new work"
    assert await page.evaluate("document.activeElement === window.closeCompletionFocus"), "toast must not steal focus"
    assert any(path.endswith("/close") for _, path, _ in mutations), "delete button must invoke terminal close API"
    lifecycle = [path.rsplit("/", 1)[-1] for _, path, _ in mutations if path.endswith(("/hide", "/close"))]
    assert lifecycle == ["hide", "close"], lifecycle
    assert close_states == [{"active": "s2", "hidden": True}], close_states
    assert not any(path == "/api/terminal/scheduled-inputs" for _, path, _ in mutations), mutations
    assert not any(method == "DELETE" for method, _, _ in mutations), mutations
    assert not errors, errors
    await page.close()
    print(f"terminal direct buttons browser passed: {width}x{height}")


async def main():
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(executable_path=CHROMIUM, headless=True, args=["--no-sandbox"])
        try:
            await check(browser, 1440, 1000)
            await check(browser, 375, 812)
        finally:
            await browser.close()


asyncio.run(main())
