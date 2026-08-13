import asyncio
import json
import os
from urllib.parse import parse_qsl, urlparse

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111").rstrip("/")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/third_party/browser-tools/bin/chromium",
)


async def main():
    results = {}
    errors = []
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        context = await browser.new_context(viewport={"width": 1280, "height": 900})
        page = await context.new_page()
        page.on("pageerror", lambda error: errors.append(str(error)))
        launched_agent_session = False
        async def handle_agent_terminal_route(route):
            nonlocal launched_agent_session
            parsed = urlparse(route.request.url)
            if parsed.path == "/api/terminal/sessions":
                sessions = [
                    {
                        "id": "s-work-last",
                        "name": "工作代理（继续）",
                        "origin": "agent",
                        "owner_key": "terminal-agent-profile:work_agent",
                        "path": "../third_party",
                        "display_path": "/home/third_party",
                        "created_at": 100,
                        "last_opened_at": 300,
                    },
                    {
                        "id": "s-proxy-old",
                        "name": "代理设置（继续）",
                        "origin": "agent",
                        "owner_key": "terminal-agent-profile:proxy_settings_agent",
                        "path": "../system",
                        "display_path": "/home/system",
                        "created_at": 90,
                        "last_opened_at": 200,
                    },
                ]
                if launched_agent_session:
                    sessions.append({
                        "id": "s-agent-test",
                        "name": "代理设置（新会话）",
                        "origin": "agent",
                        "owner_key": "terminal-agent-profile:proxy_settings_agent",
                        "path": "../system",
                        "display_path": "/home/system",
                        "created_at": 400,
                        "last_opened_at": 400,
                    })
                await route.fulfill(
                    json={"sessions": sessions}
                )
                return
            if parsed.path == "/terminal":
                query = dict(parse_qsl(parsed.query))
                if query.get("agent_profile"):
                    launched_agent_session = True
                    await route.fulfill(
                        status=200,
                        content_type="text/html",
                        body="""<!doctype html><title>mock terminal</title>
                <main id='mock-terminal'>terminal</main><script>
                history.replaceState({}, '', '/terminal?path=system&session=s-agent-test');
                parent.postMessage({
                  type: 'webclx-agent-terminal-launch', status: 'ready',
                  profileId: 'proxy_settings_agent', profileName: '代理设置',
                  presetName: 'MiniMax3', model: 'MiniMax-M3', sessionId: 's-agent-test'
                }, location.origin);
                </script>""",
                    )
                else:
                    await route.fulfill(
                        status=200,
                        content_type="text/html",
                        body="<!doctype html><title>restored terminal</title><main>restored</main>",
                    )
                return
            await route.fallback()

        await page.route("**/*", handle_agent_terminal_route)
        await page.goto(f"{BASE_URL}/agent", wait_until="domcontentloaded")
        await page.wait_for_selector(".agent-profile-item")
        await page.wait_for_selector(".agent-terminal-shell.ready")
        restored_url = await page.locator(".agent-terminal-frame").get_attribute("src")
        assert "session=s-work-last" in restored_url, restored_url
        assert "agent_profile" not in restored_url, restored_url
        labels = await page.locator(".agent-profile-name").all_inner_texts()
        assert labels[:2] == ["代理设置", "工作代理"], labels
        card_text = await page.locator('.agent-profile-item:has-text("智能体工厂")').inner_text()
        assert "api-1776989731419" not in card_text, card_text
        assert "$webclx-codex-api-terminal-ops" not in card_text, card_text
        assert "/home/codes/webClx" not in card_text, card_text
        desktop_controls = await page.locator(".agent-profile-controls").first.evaluate(
            """element => ({
                width: element.getBoundingClientRect().width,
                buttons: Array.from(element.querySelectorAll('button')).map(button => ({
                    text: button.textContent.trim(),
                    width: button.getBoundingClientRect().width,
                    height: button.getBoundingClientRect().height,
                    wraps: button.scrollHeight > button.clientHeight,
                })),
            })"""
        )
        assert [button["text"] for button in desktop_controls["buttons"]] == ["打开", "新建", "编辑", "删除"]
        assert not any(button["wraps"] for button in desktop_controls["buttons"])
        assert all(button["width"] <= 54 for button in desktop_controls["buttons"])
        await page.locator('.agent-profile-item:has-text("代理设置") button', has_text="编辑").click()
        await page.wait_for_selector("#agent-terminal-profile-dialog[open]")
        selected_preset = await page.locator("#agent-terminal-profile-preset option:checked").inner_text()
        assert "MiniMax" in selected_preset, selected_preset
        assert await page.locator("#agent-terminal-profile-preset").is_enabled()
        await page.click("#agent-terminal-profile-cancel")
        await page.locator('.agent-profile-item:has-text("代理设置") button', has_text="新建").click()
        await page.wait_for_selector(".agent-terminal-frame")
        await page.wait_for_selector(".agent-terminal-shell.ready")
        embedded = await page.locator(".agent-terminal-shell").evaluate(
            """element => ({
                title: element.querySelector('.agent-terminal-title')?.textContent.trim(),
                frameUrl: element.querySelector('iframe')?.getAttribute('src'),
                frameHeight: element.querySelector('iframe')?.getBoundingClientRect().height,
                managerPresent: Boolean(element.querySelector('a[href^="/terminal"]')),
                sessionOptions: Array.from(
                    element.querySelector('.agent-terminal-session-switcher')?.options || [],
                    option => option.value,
                ).filter(Boolean),
                ready: element.classList.contains('ready'),
                pendingVisible: getComputedStyle(element.querySelector('.agent-terminal-pending')).display !== 'none',
            })"""
        )
        assert embedded["title"] == "代理设置 · MiniMax3 · MiniMax-M3", embedded
        assert "embedded=agent&agent_profile=proxy_settings_agent" in embedded["frameUrl"], embedded
        assert embedded["frameHeight"] > 300, embedded
        assert not embedded["managerPresent"], embedded
        assert embedded["sessionOptions"] == ["s-agent-test", "s-proxy-old"], embedded
        assert embedded["ready"], embedded
        assert not embedded["pendingVisible"], embedded
        assert "MiniMax3 · MiniMax-M3" in embedded["title"], embedded
        results["desktop"] = {
            "profiles": labels,
            "restored_url": restored_url,
            "selected_preset": selected_preset,
            "controls": desktop_controls,
            "embedded": embedded,
            "overflow": await page.evaluate("document.documentElement.scrollWidth > innerWidth"),
        }
        assert not results["desktop"]["overflow"]

        await page.set_viewport_size({"width": 375, "height": 812})
        await page.reload(wait_until="domcontentloaded")
        await page.wait_for_selector(".agent-profile-item")
        assert not await page.locator("#agent-sidebar").evaluate("el => el.classList.contains('open')")
        await page.locator("[data-agent-sidebar-toggle]").first.click()
        assert await page.locator("#agent-sidebar").evaluate("el => el.classList.contains('open')")
        assert await page.locator('.agent-profile-item:has-text("代理设置")').is_visible()
        mobile_controls = await page.locator(".agent-profile-controls").first.evaluate(
            """element => ({
                width: element.getBoundingClientRect().width,
                buttons: Array.from(element.querySelectorAll('button')).map(button => ({
                    text: button.textContent.trim(),
                    width: button.getBoundingClientRect().width,
                    height: button.getBoundingClientRect().height,
                    wraps: button.scrollHeight > button.clientHeight,
                })),
            })"""
        )
        assert not any(button["wraps"] for button in mobile_controls["buttons"])
        assert all(button["width"] <= 54 for button in mobile_controls["buttons"])
        results["mobile"] = {
            "sidebar_open": True,
            "controls": mobile_controls,
            "overflow": await page.evaluate("document.documentElement.scrollWidth > innerWidth"),
        }
        assert not results["mobile"]["overflow"]
        await context.close()

        await browser.close()

    assert not errors, errors
    print(json.dumps(results, ensure_ascii=False, sort_keys=True))


if __name__ == "__main__":
    asyncio.run(main())
