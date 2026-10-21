import json
import mimetypes
import os
import re
from pathlib import Path
from urllib.parse import urlparse

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
BASE_URL = os.environ.get("WEBCLX_ACCOUNT_EDIT_BASE_URL", "http://127.0.0.1:11111").rstrip("/")


def route_local_static(route, request):
    parsed = urlparse(request.url)
    if parsed.hostname not in {"127.0.0.1", "localhost"}:
        route.fulfill(status=204, content_type="text/plain", body=b"")
        return
    if parsed.path in {"/", "/codex_api"}:
        route.fulfill(
            status=200,
            content_type="text/html; charset=utf-8",
            body=(STATIC / "index.html").read_bytes(),
        )
        return
    if parsed.path.startswith("/assets/"):
        candidate = (STATIC / parsed.path.removeprefix("/assets/")).resolve()
        if candidate.is_relative_to(STATIC) and candidate.is_file():
            route.fulfill(
                status=200,
                content_type=mimetypes.guess_type(candidate.name)[0] or "application/octet-stream",
                body=candidate.read_bytes(),
            )
            return
    route.continue_()


def main():
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        context = browser.new_context(viewport={"width": 390, "height": 844}, is_mobile=True)
        context.grant_permissions(
            ["clipboard-read", "clipboard-write"],
            origin=BASE_URL,
        )
        page = context.new_page()
        console_errors = []
        page_errors = []
        request_failures = []
        page.on(
            "console",
            lambda message: console_errors.append(message.text) if message.type == "error" else None,
        )
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        page.on("requestfailed", lambda request: request_failures.append(request.url))
        page.route("**/*", route_local_static)

        page.goto(f"{BASE_URL}/codex_api", wait_until="domcontentloaded", timeout=30_000)
        page.wait_for_function(
            "state.apiPresetsLoaded === true && state.apiPresets.length > 0",
            timeout=15_000,
        )

        api_row = page.locator("#codex-account-list tr").filter(has_text="Codex_API").first
        expected_name = api_row.locator("td").nth(5).inner_text().strip()
        api_row.get_by_role("button", name=re.compile(r"^(编辑|Edit)$")).click()
        page.wait_for_function(
            'document.querySelector("#api-preset-editor-panel")?.hidden === false',
            timeout=5_000,
        )
        page.wait_for_timeout(500)

        result = page.evaluate(
            """
            () => {
              const panel = document.querySelector('#api-preset-editor-panel');
              const rect = panel.getBoundingClientRect();
              return {
                clientWidth: document.documentElement.clientWidth,
                scrollWidth: document.documentElement.scrollWidth,
                viewportHeight: window.innerHeight,
                panelHidden: panel.hidden,
                panelDisplay: getComputedStyle(panel).display,
                panelTop: Math.round(rect.top),
                panelBottom: Math.round(rect.bottom),
                name: document.querySelector('#api-preset-name').value,
                baseUrl: document.querySelector('#api-base-url-input').value,
                activeElement: document.activeElement?.id || '',
              };
            }
            """
        )

        assert result["panelHidden"] is False
        assert result["panelDisplay"] != "none"
        assert result["panelTop"] < result["viewportHeight"]
        assert result["panelBottom"] > 0
        assert result["name"] == expected_name
        assert result["baseUrl"]
        assert result["activeElement"] == "api-preset-name"
        assert result["scrollWidth"] <= result["clientWidth"] + 2

        page.locator("#api-preset-editor-panel .field-label", has_text="Base URL").click()
        assert page.evaluate("navigator.clipboard.readText()") == result["baseUrl"]
        assert "已复制Base URL内容" in page.locator("#api-form-status").inner_text()
        assert console_errors == []
        assert page_errors == []
        assert request_failures == []

        page.screenshot(path="/tmp/webclx-codex-account-edit-mobile.png", full_page=True)
        context.close()
        browser.close()
        print(json.dumps({"expectedName": expected_name, "geometry": result}, ensure_ascii=False))


if __name__ == "__main__":
    main()
