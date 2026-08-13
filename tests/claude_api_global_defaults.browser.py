import json
import mimetypes
import os
from pathlib import Path
from urllib.parse import urlparse

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
BASE_URL = "http://127.0.0.1:11111"
USE_LOCAL_STATIC = os.environ.get("WEBCLX_PRESET_UI_LIVE") != "1"


def route_local_static(route, request):
    parsed = urlparse(request.url)
    if parsed.hostname not in {"127.0.0.1", "localhost"}:
        route.fulfill(status=204, content_type="text/plain", body=b"")
        return
    if parsed.path in {"/", "/claude_api"}:
        route.fulfill(
            status=200,
            content_type="text/html; charset=utf-8",
            body=(STATIC / "index.html").read_bytes(),
        )
        return
    if parsed.path.startswith("/assets/"):
        candidate = (STATIC / parsed.path.removeprefix("/assets/")).resolve()
        if candidate.is_relative_to(STATIC) and candidate.is_file():
            content_type = mimetypes.guess_type(candidate.name)[0] or "application/octet-stream"
            route.fulfill(status=200, content_type=content_type, body=candidate.read_bytes())
            return
    route.continue_()


def inspect_layout(browser, viewport, screenshot_path):
    page = browser.new_page(viewport=viewport, is_mobile=viewport["width"] < 600)
    console_errors = []
    page.on(
        "console",
        lambda message: console_errors.append(message.text) if message.type == "error" else None,
    )
    if USE_LOCAL_STATIC:
        page.route("**/*", route_local_static)
    page.goto(f"{BASE_URL}/claude_api", wait_until="domcontentloaded", timeout=30_000)
    page.wait_for_selector("#claude-default-config-list tr", timeout=10_000)

    defaults = page.locator("#claude-api-global-defaults")
    defaults.scroll_into_view_if_needed()
    assert defaults.is_visible()
    geometry = defaults.evaluate(
        """
        element => {
          const tableWrap = element.querySelector('.codex-default-config-table-wrap');
          const buttons = Array.from(element.querySelectorAll('button')).map((button) => {
            const rect = button.getBoundingClientRect();
            return { width: Math.round(rect.width), height: Math.round(rect.height) };
          });
          return {
            clientWidth: document.documentElement.clientWidth,
            scrollWidth: document.documentElement.scrollWidth,
            tableClientWidth: tableWrap.clientWidth,
            tableScrollWidth: tableWrap.scrollWidth,
            buttons,
          };
        }
        """
    )
    assert geometry["scrollWidth"] <= geometry["clientWidth"] + 2, geometry
    assert geometry["tableScrollWidth"] >= geometry["tableClientWidth"]
    assert all(button["width"] >= 24 and button["height"] >= 24 for button in geometry["buttons"])
    assert console_errors == []
    page.screenshot(path=screenshot_path, full_page=True)
    page.close()
    return geometry


def inspect_save(browser):
    page = browser.new_page(viewport={"width": 1440, "height": 900})
    console_errors = []
    captured_payloads = []
    page.on(
        "console",
        lambda message: console_errors.append(message.text) if message.type == "error" else None,
    )
    if USE_LOCAL_STATIC:
        page.route("**/*", route_local_static)
    page.goto(f"{BASE_URL}/claude_api", wait_until="domcontentloaded", timeout=30_000)
    page.wait_for_selector("#claude-default-config-list tr", timeout=10_000)

    def capture_settings_put(route, request):
        if request.method != "PUT":
            route.continue_()
            return
        payload = request.post_data_json
        captured_payloads.append(payload)
        route.fulfill(
            status=200,
            content_type="application/json",
            body=json.dumps(
                {"claude_default_config_entries": payload["claude_default_config_entries"]}
            ),
        )

    page.route("**/api/settings", capture_settings_put)
    page.locator("#claude-api-global-defaults").scroll_into_view_if_needed()
    page.locator("#claude-default-config-add").click()
    added_row = page.locator("#claude-default-config-list tr").last
    added_row.locator(".claude-default-config-key-input").fill(
        "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC"
    )
    added_row.locator(".claude-default-config-value-input").fill("1")
    page.locator("#claude-default-config-save").click()
    page.wait_for_selector("#claude-default-config-status[data-tone=ok]")

    assert len(captured_payloads) == 1
    assert set(captured_payloads[0]) == {"claude_default_config_entries"}
    assert captured_payloads[0]["claude_default_config_entries"][-1] == {
        "key": "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
        "value": "1",
    }
    assert console_errors == []
    page.screenshot(path="/tmp/webclx-claude-api-defaults-save.png", full_page=True)
    page.close()
    return len(captured_payloads[0]["claude_default_config_entries"])


def main():
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        result = {
            "mobile": inspect_layout(
                browser,
                {"width": 390, "height": 844},
                "/tmp/webclx-claude-api-defaults-mobile.png",
            ),
            "desktop": inspect_layout(
                browser,
                {"width": 1440, "height": 900},
                "/tmp/webclx-claude-api-defaults-desktop.png",
            ),
            "savedDefaultCount": inspect_save(browser),
        }
        browser.close()
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
