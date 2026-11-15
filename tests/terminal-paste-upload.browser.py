"""Browser test for Ctrl+V paste dialog and image upload in webClx terminal."""
import asyncio
import json
import mimetypes
from pathlib import Path
from urllib.parse import urlparse
from playwright.async_api import async_playwright

ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROMIUM = "/home/third_party/browser-tools/bin/chromium"
SESSION_ID = "paste-upload-session"

async def main():
    page_errors = []
    upload_requests = []

    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        try:
            for is_mobile in [True, False]:
                viewport = {"width": 390, "height": 700} if is_mobile else {"width": 1280, "height": 800}
                context = await browser.new_context(
                    viewport=viewport,
                    is_mobile=is_mobile,
                    has_touch=is_mobile,
                )
                page = await context.new_page()
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
                    if parsed.path == "/api/settings":
                        await route.fulfill(
                            json={
                                "workspace_dir": "/home/codes",
                                "desktop_terminal_soft_keyboard_enabled": True,
                                "terminal_function_commands": [
                                    {"key": "ctrl_v", "label": "Ctrl+V", "action": "send_sequence", "command": "ctrl_v"},
                                ],
                            }
                        )
                        return
                    if parsed.path == "/api/terminal/sessions":
                        await route.fulfill(
                            json={
                                "path": "tools/webClx",
                                "display_path": "/webClx",
                                "sessions": [
                                    {
                                        "id": SESSION_ID,
                                        "name": "Paste upload",
                                        "path": "tools/webClx",
                                        "display_path": "/webClx",
                                        "idle": False,
                                        "connected": True,
                                    }
                                ],
                            }
                        )
                        return
                    if parsed.path == f"/api/terminal/sessions/{SESSION_ID}/paste-assets":
                        upload_requests.append(
                            {
                                "content_type": route.request.headers.get("content-type", ""),
                                "body": route.request.post_data_buffer,
                            }
                        )
                        await route.fulfill(
                            json={
                                "assets": [
                                    {
                                        "name": "uploaded-test-image.png",
                                        "relative_path": ".webclx-paste/uploaded-test-image.png",
                                        "mime": "image/png",
                                        "size": 64,
                                    }
                                ]
                            }
                        )
                        return
                    if parsed.path in {
                        "/api/terminal/scheduled-inputs",
                        "/api/terminal/auto-continue-tasks",
                        "/api/terminal/resume-archives",
                    }:
                        await route.fulfill(json={"tasks": [], "archives": []})
                        return
                    if parsed.path.startswith("/api/"):
                        await route.fulfill(json={})
                        return
                    await route.fulfill(status=404, body="not found")

                async def handle_websocket(websocket):
                    websocket.send(b"\r\n[root@webclx webClx]# ")

                await page.route("**/*", handle_http)
                await page.route_web_socket("**/api/terminal/ws?**", handle_websocket)
                await page.goto(
                    f"http://webclx.test/terminal?path=tools%2FwebClx&session={SESSION_ID}",
                    wait_until="domcontentloaded",
                )
                await page.wait_for_function(
                    f"() => state.activeSessionId === '{SESSION_ID}'"
                )

                activate = lambda selector: page.locator(selector).tap() if is_mobile else page.locator(selector).click()

                # 打开全能菜单
                await activate("#terminal-function-command-button")
                menu = page.locator("#terminal-function-command-menu")
                assert await menu.is_visible()

                # 检查粘贴对话框按钮
                paste_btn = page.locator("#terminal-paste-dialog-button")
                assert await paste_btn.is_visible()

                # 点击打开粘贴对话框
                await activate("#terminal-paste-dialog-button")
                assert await menu.is_hidden()

                dialog = page.locator("#terminal-paste-dialog")
                assert await dialog.evaluate("el => el.open")

                # 检查对话框中的上传图片按钮
                upload_img_btn = page.locator("#terminal-paste-upload-image-button")
                assert await upload_img_btn.is_visible()
                assert await upload_img_btn.inner_text() == "上传图片"

                # 点击上传图片按钮选择图片
                async with page.expect_file_chooser() as chooser_info:
                    await activate("#terminal-paste-upload-image-button")
                chooser = await chooser_info.value
                assert chooser.is_multiple()
                await chooser.set_files(
                    {
                        "name": "test-screen.png",
                        "mimeType": "image/png",
                        "buffer": b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDRtest",
                    }
                )

                # 等待图片上传并显示在资产区与文本框
                await page.wait_for_function("() => terminalPasteAssetEntries.length > 0")
                textarea_val = await page.locator("#terminal-paste-text").input_value()
                assert ".webclx-paste/uploaded-test-image.png" in textarea_val

                # 右上角关闭
                await activate("#terminal-paste-close")
                assert not await dialog.evaluate("el => el.open")

                # 再次打开全能菜单，验证独立的“粘贴图片”入口
                await activate("#terminal-function-command-button")
                assert await menu.is_visible()
                img_paste_btn = page.locator("#terminal-paste-image-button")
                assert await img_paste_btn.is_visible()
                assert "粘贴图片" in (await img_paste_btn.inner_text())

                # 点击打开独立的粘贴图片对话框
                await activate("#terminal-paste-image-button")
                assert await menu.is_hidden()
                img_dialog = page.locator("#terminal-image-paste-dialog")
                assert await img_dialog.evaluate("el => el.open")

                # 检查独立图片对话框中的选择图片按钮与文本框
                select_btn = page.locator("#terminal-image-paste-select-button")
                assert await select_btn.is_visible()
                prompt_input = page.locator("#terminal-image-paste-text")
                assert await prompt_input.is_visible()

                # 通过选择图片按钮上传图片
                async with page.expect_file_chooser() as chooser_info:
                    await activate("#terminal-image-paste-select-button")
                chooser = await chooser_info.value
                await chooser.set_files(
                    {
                        "name": "photo-sample.png",
                        "mimeType": "image/png",
                        "buffer": b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDRtestphoto",
                    }
                )
                await page.wait_for_function("() => terminalImagePasteAssetEntries.length > 0")
                img_textarea_val = await prompt_input.input_value()
                assert ".webclx-paste/uploaded-test-image.png" in img_textarea_val

                # 取消关闭独立图片对话框
                await activate("#terminal-image-paste-cancel")
                assert not await img_dialog.evaluate("el => el.open")

                # 打开终端工具菜单，验证终端工具中的“粘贴图片”和“Ctrl+V 粘贴”
                if is_mobile:
                    bounds = await page.locator("#terminal-tools-button").bounding_box()
                    await page.touchscreen.tap(bounds["x"] + bounds["width"] / 2, bounds["y"] + bounds["height"] / 2)
                else:
                    await page.locator("#terminal-tools-button").click()

                tools_menu = page.locator("#terminal-tools-menu")
                await tools_menu.wait_for(state="visible")

                tools_img_btn = page.locator("#terminal-tools-paste-image-button")
                assert await tools_img_btn.is_visible()
                assert "粘贴图片" in (await tools_img_btn.inner_text())

                tools_paste_btn = page.locator("#terminal-tools-paste-dialog-button")
                assert await tools_paste_btn.is_visible()
                assert "Ctrl+V 粘贴" in (await tools_paste_btn.inner_text())

                # 从终端工具菜单打开独立图片对话框
                await activate("#terminal-tools-paste-image-button")
                assert await tools_menu.is_hidden()
                assert await img_dialog.evaluate("el => el.open")
                await activate("#terminal-image-paste-cancel")
                assert not await img_dialog.evaluate("el => el.open")

                await context.close()
        finally:
            await browser.close()

    assert not page_errors, page_errors
    assert len(upload_requests) >= 2, f"expected uploads for both mobile and desktop, got {len(upload_requests)}"
    print("PASS: browser test for Ctrl+V paste dialog and image upload verified")

if __name__ == "__main__":
    asyncio.run(main())
