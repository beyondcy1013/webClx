import asyncio
import json
import os

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111").rstrip("/")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/third_party/browser-tools/bin/chromium",
)


async def main():
    created_session_id = None
    page_errors = []
    console_errors = []
    result = {}
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        context = await browser.new_context(viewport={"width": 1280, "height": 900})
        page = await context.new_page()
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        page.on(
            "console",
            lambda message: console_errors.append(message.text)
            if message.type == "error"
            else None,
        )

        try:
            await page.goto(f"{BASE_URL}/agent", wait_until="networkidle")
            card = page.locator('.agent-profile-item:has-text("智能体工厂")')
            await card.wait_for()
            assert await card.locator(".agent-profile-type").inner_text() == "原生"

            async with page.expect_response(
                lambda response: response.url.endswith("/api/agent/sessions")
                and response.request.method == "POST"
            ) as response_info:
                await card.get_by_role("button", name="新建智能体会话").click()
            response = await response_info.value
            assert response.ok, await response.text()
            created = await response.json()
            created_session_id = created["id"]
            assert created["profile_id"] == "agent_factory", created
            assert created["cwd"] == "/home/codes/webClx", created
            assert created["api_preset_id"], created
            assert created["messages"] == [], created
            assert "不是创建会话后立即执行的指令" in created["system_prompt"], created

            await page.locator("#agent-title-input").wait_for()
            assert await page.locator("#agent-title-input").input_value() == "智能体工厂"
            assert await page.locator(".agent-terminal-frame").count() == 0
            context_status = await context.request.get(
                f"{BASE_URL}/api/agent/sessions/{created_session_id}/context"
            )
            assert context_status.ok, await context_status.text()
            context_data = await context_status.json()
            assert context_data["model"] == created["model"], context_data
            assert context_data["context_window"] > 0, context_data
            assert 0 <= context_data["used_percent"] <= 100, context_data
            await page.locator("#agent-context-status").wait_for()
            context_label = await page.locator("#agent-context-status").inner_text()
            assert created["model"] in context_label and "%" in context_label, context_label

            async with page.expect_response(
                lambda response: response.url.endswith(
                    f"/api/agent/sessions/{created_session_id}/compact"
                )
                and response.request.method == "POST"
            ) as compact_response_info:
                await page.locator("#agent-compact-btn").click()
            compact_response = await compact_response_info.value
            assert compact_response.ok, await compact_response.text()
            compact_data = await compact_response.json()
            assert compact_data["compacted"] is False, compact_data
            await page.wait_for_function(
                "document.querySelector('#agent-compact-btn')?.textContent === '压缩'"
            )

            command_response = await context.request.post(
                f"{BASE_URL}/api/agent/sessions/{created_session_id}/commands",
                data={
                    "command": "printf qa-ready; read line; printf ':received:%s' \"$line\"",
                    "cwd": "/home/codes/webClx",
                },
            )
            assert command_response.ok, await command_response.text()
            command_data = await command_response.json()
            command_id = command_data["id"]
            stdin_response = await context.request.post(
                f"{BASE_URL}/api/agent/sessions/{created_session_id}/commands/{command_id}/stdin",
                data={"input": "browser-qa\n"},
            )
            assert stdin_response.ok, await stdin_response.text()
            for _ in range(30):
                command_status = await context.request.get(
                    f"{BASE_URL}/api/agent/sessions/{created_session_id}/commands/{command_id}"
                )
                assert command_status.ok, await command_status.text()
                command_data = await command_status.json()
                if command_data["status"] != "running":
                    break
                await page.wait_for_timeout(100)
            assert command_data["status"] == "completed", command_data
            assert "qa-ready:received:browser-qa" in command_data["stdout"], command_data

            await page.locator("#agent-input").fill("只回复“已待命”，不要修改文件或执行命令。")
            await page.locator("#agent-send-btn").click()
            await page.wait_for_function(
                "document.querySelector('#agent-send-btn')?.textContent === '发送'",
                timeout=120_000,
            )
            assert await page.locator(".agent-error-banner").count() == 0
            assistant_messages = await page.locator(
                ".agent-msg.assistant .agent-msg-content"
            ).all_inner_texts()
            assert assistant_messages, "native Agent returned no assistant message"

            await page.locator("#agent-input").fill(
                "请调用 run_command 执行 pwd，只报告工作目录，不要修改任何文件。"
            )
            await page.locator("#agent-send-btn").click()
            await page.wait_for_function(
                "document.querySelector('#agent-send-btn')?.textContent === '发送'",
                timeout=120_000,
            )
            run_command_card = page.locator(
                '.agent-tool-card:has(.tool-name:text-is("run_command"))'
            ).last
            await run_command_card.wait_for()
            assert await run_command_card.locator(".tool-badge.ok").count() == 1
            assert "/home/codes/webClx" in await run_command_card.text_content()
            await page.screenshot(path="/tmp/webclx-agent-native-desktop.png", full_page=True)

            await page.set_viewport_size({"width": 375, "height": 812})
            await page.reload(wait_until="networkidle")
            await page.locator("[data-agent-sidebar-toggle]").first.click()
            await page.wait_for_timeout(250)
            mobile_card = page.locator('.agent-profile-item:has-text("智能体工厂")')
            await mobile_card.wait_for()
            assert await mobile_card.locator(".agent-profile-type").inner_text() == "原生"
            sidebar_box = await page.locator("#agent-sidebar").bounding_box()
            card_box = await mobile_card.bounding_box()
            assert sidebar_box and sidebar_box["x"] == 0 and sidebar_box["width"] == 260
            assert card_box and card_box["x"] >= 0
            await page.screenshot(
                path="/tmp/webclx-agent-native-mobile-sidebar.png", full_page=True
            )
            await mobile_card.get_by_role("button", name="打开最近的智能体会话").click()
            await page.locator("#agent-title-input").wait_for()
            await page.wait_for_timeout(250)
            assert not await page.locator("#agent-sidebar").evaluate(
                "element => element.classList.contains('open')"
            )
            assert await page.locator(".agent-terminal-frame").count() == 0
            assert not await page.evaluate("document.documentElement.scrollWidth > innerWidth")
            await page.screenshot(path="/tmp/webclx-agent-native-mobile.png", full_page=True)

            result = {
                "created_session_id": created_session_id,
                "profile_id": created["profile_id"],
                "cwd": created["cwd"],
                "api_preset_id": created["api_preset_id"],
                "assistant_messages": assistant_messages,
                "run_command": "ok: /home/codes/webClx",
                "context": context_label,
                "background_command": command_data["status"],
                "desktop_screenshot": "/tmp/webclx-agent-native-desktop.png",
                "mobile_sidebar_screenshot": "/tmp/webclx-agent-native-mobile-sidebar.png",
                "mobile_screenshot": "/tmp/webclx-agent-native-mobile.png",
            }
        finally:
            if created_session_id is not None:
                verification = await context.request.get(
                    f"{BASE_URL}/api/agent/sessions/{created_session_id}"
                )
                if verification.ok:
                    session = await verification.json()
                    if (
                        session.get("id") == created_session_id
                        and session.get("profile_id") == "agent_factory"
                        and session.get("cwd") == "/home/codes/webClx"
                    ):
                        deleted = await context.request.delete(
                            f"{BASE_URL}/api/agent/sessions/{created_session_id}"
                        )
                        assert deleted.ok, await deleted.text()
                    else:
                        raise AssertionError(f"refusing ambiguous cleanup: {session}")
            await context.close()
            await browser.close()

    assert not page_errors, page_errors
    assert not console_errors, console_errors
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    asyncio.run(main())
