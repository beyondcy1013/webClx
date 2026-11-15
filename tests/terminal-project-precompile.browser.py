"""Real menu interactions with all HTTP/WebSocket traffic intercepted; never queue a build."""
import asyncio
import json
import mimetypes
import os
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright

STATIC = Path(__file__).resolve().parents[1] / "static"


async def check(browser, width, height):
    context = await browser.new_context(
        viewport={"width": width, "height": height},
        has_touch=width < 600, service_workers="block",
    )
    page = await context.new_page()
    errors, builds, deployments = [], [], []
    page.on("pageerror", lambda error: errors.append(str(error)))
    config = None
    fail_queue = False

    async def http(route):
        nonlocal config, fail_queue
        path = urlparse(route.request.url).path
        if path == "/terminal":
            await route.fulfill(content_type="text/html", body=(STATIC / "terminal.html").read_bytes())
        elif path.startswith("/assets/"):
            asset = STATIC / path.removeprefix("/assets/")
            await route.fulfill(
                status=200 if asset.is_file() else 404,
                content_type=mimetypes.guess_type(asset.name)[0] or "application/octet-stream",
                body=asset.read_bytes() if asset.is_file() else b"",
            )
        elif path == "/api/settings":
            await route.fulfill(json={
                "workspace_dir": "/home/codes",
                "desktop_terminal_soft_keyboard_enabled": True,
            })
        elif path == "/api/terminal/sessions":
            await route.fulfill(json={
                "path": "demo", "display_path": "/home/codes/demo",
                "sessions": [{
                    "id": "s1", "name": "预热测试终端", "path": "demo",
                    "display_path": "/home/codes/demo", "idle": False, "connected": True,
                }],
            })
        elif path == "/api/file":
            if config is None:
                await route.fulfill(status=404, body="file not found")
            else:
                await route.fulfill(json={"editable": True, "content": json.dumps(config)})
        elif path == "/api/build/compile":
            builds.append(route.request.post_data_json)
            if fail_queue:
                await route.fulfill(status=503, body="测试：编译队列暂不可用")
            else:
                await route.fulfill(json={
                    "queued": True, "request_id": "precompile-browser-test",
                    "project": "demo",
                    "command": builds[-1].get("command", ["cargo", "build", "--release"]),
                })
        elif path == "/api/build/deploy":
            deployments.append(route.request.post_data_json)
            await route.fulfill(status=500, body="unexpected deployment")
        elif path == "/api/terminal/resume-archives":
            await route.fulfill(json={"archives": []})
        elif path in ["/api/terminal/scheduled-inputs", "/api/terminal/auto-continue-tasks"]:
            await route.fulfill(json={"tasks": []})
        else:
            await route.fulfill(json={})

    async def websocket(socket):
        socket.send(b"precompile fixture ready\r\n")

    await page.route("**/*", http)
    await page.route_web_socket("**/api/terminal/ws?**", websocket)
    try:
        await page.goto("http://webclx.test/terminal?path=demo&session=s1", wait_until="domcontentloaded")
        await page.wait_for_function("typeof state !== 'undefined' && state.activeSessionId === 's1' && state.workspaceDir")
        button = page.locator("#terminal-project-command-button")
        if not await button.is_visible():
            await page.keyboard.press("Control+k")
        await button.click()
        precompile = page.locator('#terminal-project-command-menu [data-project-action="precompile_project"]')
        assert await precompile.evaluate("node => node.previousElementSibling.dataset.projectAction") == "deploy_project"
        await precompile.click()
        await page.wait_for_function("document.body.textContent.includes('precompile-browser-test')")
        assert len(builds) == 1, builds
        assert builds[0]["project_dir"] == "/home/codes/demo"
        assert builds[0]["source_terminal_id"] == "s1"
        assert "command" not in builds[0]
        assert "install_command" not in builds[0]
        assert not await precompile.is_visible()

        config = {"precompile": {"command": ["bash", "scripts/build-custom-target.sh"]}}
        await button.click()
        await precompile.click()
        await page.wait_for_function("document.body.textContent.includes('scripts/build-custom-target.sh')")
        assert len(builds) == 2, builds
        assert builds[-1]["command"] == config["precompile"]["command"]

        fail_queue = True
        await button.click()
        await precompile.click()
        await page.wait_for_function("document.body.textContent.includes('测试：编译队列暂不可用')")
        assert len(builds) == 3, builds
        assert not deployments, deployments
        assert not errors, errors
        print(f"precompile menu {width}x{height}: default, custom command, queue failure passed")
    finally:
        await context.close()


async def main():
    async with async_playwright() as playwright:
        candidates = sorted((Path.home() / ".cache/ms-playwright").glob("chromium-*/chrome-linux*/chrome"))
        executable = os.environ.get("CHROMIUM_PATH") or (str(candidates[-1]) if candidates else None)
        browser = await playwright.chromium.launch(executable_path=executable, headless=True, args=["--no-sandbox"])
        try:
            await check(browser, 1280, 900)
            await check(browser, 390, 844)
        finally:
            await browser.close()


if __name__ == "__main__":
    asyncio.run(main())
