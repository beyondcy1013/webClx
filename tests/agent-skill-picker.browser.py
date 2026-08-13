import asyncio
import json
import os

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111").rstrip("/")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/third_party/browser-tools/bin/chromium",
)
SESSION_TITLE = "native-skill-picker-smoke-20260731"


async def main():
    created_session_id = None
    page_errors = []
    console_errors = []
    failed_responses = []
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
        page.on(
            "response",
            lambda response: failed_responses.append(f"{response.status} {response.url}")
            if response.status >= 400
            and ("/agent" in response.url or "/api/agent" in response.url)
            else None,
        )

        try:
            await page.goto(f"{BASE_URL}/agent", wait_until="networkidle")
            card = page.locator('.agent-profile-item:has-text("智能体工厂")')
            await card.wait_for()
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

            renamed = await context.request.put(
                f"{BASE_URL}/api/agent/sessions/{created_session_id}",
                data={"title": SESSION_TITLE},
            )
            assert renamed.ok, await renamed.text()

            skills_response = await context.request.get(f"{BASE_URL}/api/agent/skills")
            assert skills_response.ok, await skills_response.text()
            skills = (await skills_response.json())["skills"]
            enabled_skills = [skill for skill in skills if not skill.get("disabled")]

            skill_button = page.get_by_role("button", name="选择 Skill")
            await skill_button.click()
            picker = page.locator("#agent-skill-picker")
            await picker.wait_for(state="visible")
            options = picker.locator('[role="option"]')
            await options.first.wait_for()
            assert await options.count() == len(enabled_skills)
            assert await page.locator("#agent-input").get_attribute("aria-expanded") == "true"
            await page.screenshot(path="/tmp/webclx-agent-skill-picker-full.png", full_page=True)

            agent_input = page.locator("#agent-input")
            await agent_input.fill("$mihomo / proxy_ops")
            first_option = picker.locator('[role="option"]').first
            await first_option.wait_for()
            assert "mihomo-proxy-ops" in await first_option.text_content()
            await agent_input.press("Enter")
            assert await agent_input.input_value() == "$mihomo-proxy-ops "

            await agent_input.fill(
                "$webclx-codex-api-terminal-ops 只确认该 Skill 已加载，不要修改文件或执行命令。"
            )
            await page.locator("#agent-send-btn").click()
            read_skill_card = page.locator(
                '.agent-tool-card:has(.tool-name:text-is("read_skill"))'
            ).first
            await read_skill_card.wait_for(timeout=120_000)
            await read_skill_card.locator(".tool-badge.ok").wait_for(timeout=120_000)
            assert "webclx-codex-api-terminal-ops" in await read_skill_card.text_content()
            await page.wait_for_function(
                "document.querySelector('#agent-send-btn')?.textContent === '发送'",
                timeout=120_000,
            )

            await page.set_viewport_size({"width": 375, "height": 812})
            await page.wait_for_timeout(300)
            assert not await page.locator("#agent-sidebar").evaluate(
                "element => element.classList.contains('open')"
            )
            await agent_input.fill("$")
            await picker.wait_for(state="visible")
            await page.wait_for_timeout(100)
            assert not await page.evaluate("document.documentElement.scrollWidth > innerWidth")
            await page.screenshot(path="/tmp/webclx-agent-skill-picker-mobile.png", full_page=True)

            result = {
                "created_session_id": created_session_id,
                "enabled_skill_count": len(enabled_skills),
                "fuzzy_match": "mihomo-proxy-ops",
                "explicit_read_skill": "ok: webclx-codex-api-terminal-ops",
                "desktop_screenshot": "/tmp/webclx-agent-skill-picker-full.png",
                "mobile_screenshot": "/tmp/webclx-agent-skill-picker-mobile.png",
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
                        and session.get("title") == SESSION_TITLE
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
    assert not failed_responses, failed_responses
    print(json.dumps(result, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    asyncio.run(main())
