from playwright.sync_api import sync_playwright


with sync_playwright() as playwright:
    browser = playwright.chromium.launch(headless=True)
    for width, height, name in [(1440, 1000, "desktop"), (390, 844, "mobile")]:
        page = browser.new_page(viewport={"width": width, "height": height})
        route_state = {"clear_completed": False, "pending": []}
        console_errors = []
        failed_responses = []
        page.on(
            "console",
            lambda message: console_errors.append(message.text)
            if message.type == "error"
            else None,
        )
        page.on(
            "response",
            lambda response: failed_responses.append((response.status, response.url))
            if response.status >= 400
            else None,
        )
        page.goto("http://127.0.0.1:11111/", wait_until="networkidle")
        page.click("#tab-settings")
        page.click("#settings-tab-compile")
        button = page.locator("#compile-history-clear")
        button.scroll_into_view_if_needed()
        box = button.bounding_box()
        title = button.locator("..").locator(".section-label").text_content()
        overflow = button.evaluate(
            "el => ({scrollWidth:el.scrollWidth,clientWidth:el.clientWidth,scrollHeight:el.scrollHeight,clientHeight:el.clientHeight})"
        )
        screenshot = f"/tmp/webclx-clear-history-{name}.png"
        page.screenshot(path=screenshot)
        unexpected_failures = [
            (status, url)
            for status, url in failed_responses
            if not (status == 404 and "/api/workspace-icon?" in url)
        ]
        print(
            name,
            {
                "title": title,
                "button": box,
                "overflow": overflow,
                "console_errors": console_errors,
                "failed_responses": failed_responses,
                "unexpected_failures": unexpected_failures,
                "screenshot": screenshot,
            },
            flush=True,
        )
        assert title == "运行历史"
        assert box and box["x"] >= 0 and box["x"] + box["width"] <= width
        assert overflow["scrollWidth"] <= overflow["clientWidth"]
        assert overflow["scrollHeight"] <= overflow["clientHeight"]
        assert not unexpected_failures
        if name == "desktop":
            def route_build_api(route):
                if route.request.method == "DELETE":
                    route_state["clear_completed"] = True
                    route.fulfill(
                        status=200,
                        content_type="application/json",
                        body='{"ok":true,"deleted_count":7}',
                    )
                    return
                if route_state["clear_completed"]:
                    route_state["pending"].append(route)
                    return
                route.continue_()

            page.route("**/api/build/compile/**", route_build_api)
            page.on("dialog", lambda dialog: dialog.accept())
            button.click()
            page.wait_for_function(
                "() => document.querySelector('#compile-status-message')?.textContent.includes('已清空 7 条')"
            )
            assert button.is_enabled()
            assert button.text_content() == "清空历史"
            assert "暂无已结束的构建运行历史" in page.locator("#compile-run-list").text_content()
            assert "已清空 7 条构建运行历史" in page.locator("#compile-status-message").text_content()
            for pending_route in route_state["pending"]:
                pending_route.fulfill(
                    status=200,
                    content_type="application/json",
                    body='{"ok":true,"queue_dir":"/tmp/compile","pending_count":0,"run_count":0,"pending_requests":[],"runs":[]}',
                )
        page.close()
    browser.close()
