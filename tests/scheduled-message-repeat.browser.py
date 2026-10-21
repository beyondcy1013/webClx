import asyncio
import json
import mimetypes
import time
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROMIUM = "/home/third_party/browser-tools/bin/chromium"


async def main():
    page_errors = []
    terminal_errors = []
    scheduled_posts = []
    scheduled_patches = []
    now_ms = int(time.time() * 1000)
    scheduled_tasks = [
        {
            "id": "parent-1",
            "task_id": "parent-1",
            "session_id": "s1",
            "terminal_name": "循环终端",
            "status": "",
            "last_error": "",
            "due_at": now_ms + 300_000,
            "due_at_millis": now_ms + 300_000,
            "created_at_millis": now_ms,
            "label": "",
            "preview": "loop body",
            "text": "loop body",
            "send_enter": False,
            "task_type": "paste",
            "working_dir": "/home/codes/tools/webClx",
            "repeat_interval_millis": 300_000,
            "parent_task_id": "",
        },
        {
            "id": "parent-1-fire-0a",
            "task_id": "parent-1-fire-0a",
            "session_id": "s1",
            "terminal_name": "循环终端",
            "status": "confirmed",
            "last_error": "",
            "due_at": now_ms - 300_000,
            "due_at_millis": now_ms - 300_000,
            "created_at_millis": now_ms - 300_000,
            "label": "",
            "preview": "loop body",
            "text": "loop body",
            "send_enter": False,
            "task_type": "paste",
            "working_dir": "/home/codes/tools/webClx",
            "repeat_interval_millis": 0,
            "parent_task_id": "parent-1",
        },
    ]
    sessions = {
        "sessions": [
            {
                "id": "s1",
                "name": "循环终端",
                "path": "/home/codes/tools/webClx",
                "display_path": "/home/codes/tools/webClx",
                "idle": False,
                "connected": True,
            }
        ]
    }

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        context = await browser.new_context(viewport={"width": 1280, "height": 900})
        try:
            async def handle_http(route):
                parsed = urlparse(route.request.url)
                path = parsed.path
                method = route.request.method
                if path == "/terminal":
                    await route.fulfill(
                        status=200,
                        content_type="text/html; charset=utf-8",
                        body=(STATIC / "terminal.html").read_bytes(),
                    )
                    return
                if path.startswith("/assets/"):
                    asset = STATIC / path.removeprefix("/assets/")
                    if asset.is_file():
                        content_type = (
                            mimetypes.guess_type(asset.name)[0] or "application/octet-stream"
                        )
                        await route.fulfill(
                            status=200, content_type=content_type, body=asset.read_bytes()
                        )
                    else:
                        await route.fulfill(status=404, body="missing asset")
                    return
                if path == "/api/auth/session":
                    await route.fulfill(json={"authenticated": True, "user": "test"})
                    return
                if path == "/api/settings":
                    await route.fulfill(
                        json={
                            "workspace_dir": "/home/codes/tools/webClx",
                            "terminal_user": "root",
                        }
                    )
                    return
                if path == "/api/terminal/sessions":
                    await route.fulfill(json=sessions)
                    return
                if path == "/api/terminal/scheduled-inputs" and method == "GET":
                    await route.fulfill(json={"tasks": scheduled_tasks})
                    return
                if path == "/api/terminal/scheduled-inputs" and method == "POST":
                    body = json.loads(route.request.post_data or "{}")
                    scheduled_posts.append(body)
                    task = {
                        "id": f"paste-{now_ms}-1",
                        "task_id": f"paste-{now_ms}-1",
                        "session_id": body.get("session_id", "s1"),
                        "terminal_name": "循环终端",
                        "status": "",
                        "last_error": "",
                        "due_at": body.get("due_at", 0),
                        "due_at_millis": body.get("due_at", 0),
                        "created_at_millis": now_ms,
                        "label": body.get("label", ""),
                        "preview": body.get("text", ""),
                        "text": body.get("text", ""),
                        "send_enter": body.get("send_enter", False),
                        "task_type": body.get("task_type", "paste"),
                        "working_dir": body.get("working_dir", ""),
                        "repeat_interval_millis": body.get("repeat_interval_millis", 0),
                        "parent_task_id": "",
                    }
                    scheduled_tasks.append(task)
                    await route.fulfill(json={"ok": True, "task": task, "tasks": scheduled_tasks})
                    return
                if path.startswith("/api/terminal/scheduled-inputs/") and method == "PATCH":
                    scheduled_patches.append(
                        {"task_id": path.rsplit("/", 1)[-1], "body": json.loads(route.request.post_data or "{}")}
                    )
                    await route.fulfill(json={"ok": True, "tasks": scheduled_tasks})
                    return
                if path == "/api/terminal/auto-continue-tasks":
                    await route.fulfill(json={"auto_continue_tasks": [], "expired_tasks": []})
                    return
                if path == "/api/terminal/resume-archives":
                    await route.fulfill(json={"archives": []})
                    return
                if path in {
                    "/api/auth/preset-test-schedules",
                    "/api/auth/api-presets",
                    "/api/auth/claude-presets",
                }:
                    await route.fulfill(json={"schedules": [], "presets": []})
                    return
                if path == "/api/entries":
                    await route.fulfill(
                        json={
                            "path": "/home/codes/tools/webClx",
                            "display_path": "/home/codes/tools/webClx",
                            "entries": [],
                            "parent_path": "/home/codes/tools",
                        }
                    )
                    return
                if path.startswith("/api/"):
                    await route.fulfill(json={})
                    return
                await route.fulfill(
                    status=200,
                    content_type="text/html; charset=utf-8",
                    body=(STATIC / "index.html").read_bytes(),
                )

            async def handle_websocket(websocket):
                websocket.on_message(lambda _message: None)

            await context.route("**/*", handle_http)
            await context.route_web_socket("**/api/terminal/ws?**", handle_websocket)

            page = await context.new_page()
            page.on("pageerror", lambda error: page_errors.append(str(error)))

            await page.goto(
                "http://webclx.test/settings/auto-continue-tasks",
                wait_until="domcontentloaded",
            )
            await page.wait_for_function(
                """() => document.getElementById('unified-task-list')
                    && document.getElementById('unified-task-list').innerHTML.includes('每隔 5 分钟')""",
                timeout=15_000,
            )
            table_html = await page.locator("#unified-task-list").inner_html()
            assert "每隔 5 分钟" in table_html, table_html
            assert "投递记录" in table_html, table_html
            parent_row = await page.locator(
                '#unified-task-list tr[data-task-id="parent-1"]'
            ).inner_html()
            child_row = await page.locator(
                '#unified-task-list tr[data-task-id="parent-1-fire-0a"]'
            ).inner_html()
            assert "unified-task-run-now" in parent_row, parent_row
            assert "unified-task-run-now" not in child_row, child_row
            assert "循环任务 parent-1" in child_row, child_row

            await page.locator("#unified-task-new-btn").click()
            await page.wait_for_selector("#unified-task-form-card:not([hidden])", timeout=5_000)
            await page.wait_for_selector('#unified-task-terminal option[value="s1"]', state="attached", timeout=5_000)
            await page.locator("#unified-task-text").fill("browser ping")
            await page.locator("#unified-task-terminal").select_option("s1")
            await page.locator("#unified-task-schedule-mode").select_option("interval")
            await page.locator("#unified-task-delay-value").fill("3")
            await page.locator("#unified-task-delay-unit").select_option("hours")
            before = len(scheduled_posts)
            await page.locator("#unified-task-create-btn").click()
            for _ in range(60):
                if len(scheduled_posts) > before:
                    break
                await asyncio.sleep(0.05)
            assert len(scheduled_posts) == before + 1, scheduled_posts
            create_body = scheduled_posts[-1]
            assert create_body["repeat_interval_millis"] == 10_800_000, create_body
            assert create_body["due_at"] > now_ms, create_body
            assert create_body["session_id"] == "s1", create_body

            await page.locator("#unified-task-new-btn").click()
            await page.wait_for_selector("#unified-task-form-card:not([hidden])", timeout=5_000)
            await page.locator("#unified-task-text").fill("browser ping")
            await page.locator("#unified-task-terminal").select_option("s1")
            await page.locator("#unified-task-schedule-mode").select_option("interval")
            await page.locator("#unified-task-delay-value").fill("0.5")
            await page.locator("#unified-task-delay-unit").select_option("seconds")
            before = len(scheduled_posts)
            await page.locator("#unified-task-create-btn").click()
            await page.wait_for_timeout(300)
            assert len(scheduled_posts) == before, scheduled_posts
            status_text = await page.locator("#unified-task-create-status").text_content()
            assert "循环间隔" in status_text, status_text

            terminal_page = await context.new_page()
            terminal_page.on("pageerror", lambda error: terminal_errors.append(str(error)))
            await terminal_page.goto(
                "http://webclx.test/terminal?path=webClx&session=s1",
                wait_until="domcontentloaded",
            )
            await terminal_page.wait_for_function(
                "() => typeof openScheduledTasksDialog === 'function'",
                timeout=15_000,
            )
            await terminal_page.evaluate("openScheduledTasksDialog()")
            await terminal_page.wait_for_selector(
                "#terminal-scheduled-tasks-dialog[open]", timeout=5_000
            )
            await terminal_page.locator('[data-unified-task-view="all"]').click()
            await terminal_page.wait_for_function(
                """() => {
                    const el = document.querySelector('#terminal-scheduled-tasks-dialog #unified-task-list');
                    return !!el && el.innerHTML.includes('每隔 5 分钟');
                }""",
                timeout=10_000,
            )
            dialog_table = await terminal_page.locator(
                "#terminal-scheduled-tasks-dialog #unified-task-list"
            ).inner_html()
            assert "每隔 5 分钟" in dialog_table, dialog_table

            await terminal_page.locator("#unified-task-new-btn").click()
            await terminal_page.wait_for_selector(
                "#unified-task-form-card:not([hidden])", timeout=5_000
            )
            await terminal_page.wait_for_selector(
                '#unified-task-terminal option[value="s1"]', state="attached", timeout=5_000
            )
            await terminal_page.locator("#unified-task-text").fill("dialog ping")
            await terminal_page.locator("#unified-task-terminal").select_option("s1")
            await terminal_page.locator("#unified-task-schedule-mode").select_option("interval")
            await terminal_page.locator("#unified-task-delay-value").fill("2")
            await terminal_page.locator("#unified-task-delay-unit").select_option("seconds")
            before = len(scheduled_posts)
            await terminal_page.locator("#unified-task-create-btn").click()
            for _ in range(60):
                if len(scheduled_posts) > before:
                    break
                await asyncio.sleep(0.05)
            assert not terminal_errors, f"terminal page script errors blocked the create POST: {terminal_errors}"
            assert len(scheduled_posts) == before + 1, scheduled_posts
            dialog_body = scheduled_posts[-1]
            assert dialog_body["repeat_interval_millis"] == 2_000, dialog_body

            settings_screenshot = "/tmp/webclx-scheduled-repeat-settings.png"
            dialog_screenshot = "/tmp/webclx-scheduled-repeat-dialog.png"
            await page.screenshot(path=settings_screenshot, full_page=True)
            await terminal_page.screenshot(path=dialog_screenshot, full_page=True)
            print(
                json.dumps(
                    {
                        "ok": True,
                        "posts": scheduled_posts,
                        "patches": scheduled_patches,
                        "settings_screenshot": settings_screenshot,
                        "dialog_screenshot": dialog_screenshot,
                        "page_errors": page_errors,
                        "terminal_page_errors": terminal_errors,
                    },
                    sort_keys=True,
                )
            )
        finally:
            await context.close()
            await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
