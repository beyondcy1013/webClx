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


def install_common_config_route(page, captured_payloads=None):
    state = {
        "ok": True,
        "user": "root",
        "config_file": "/home/root/.codex/config.toml",
        "exists": True,
        "approval_never": True,
        "sandbox_full_access": False,
        "animations_disabled": True,
        "approval_policy": "never",
        "sandbox_mode": "workspace-write",
    }

    def handle(route, request):
        if request.method == "GET" and not USE_LOCAL_STATIC:
            route.continue_()
            return
        if request.method == "PUT":
            payload = request.post_data_json
            if captured_payloads is not None:
                captured_payloads.append(payload)
            state["approval_never"] = payload["approval_never"]
            state["sandbox_full_access"] = payload["sandbox_full_access"]
            state["approval_policy"] = "never" if payload["approval_never"] else None
            state["sandbox_mode"] = (
                "danger-full-access" if payload["sandbox_full_access"] else None
            )
        route.fulfill(
            status=200,
            content_type="application/json",
            body=json.dumps(state),
        )

    page.route("**/api/settings/codex-common-config", handle)


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
            content_type = mimetypes.guess_type(candidate.name)[0] or "application/octet-stream"
            route.fulfill(status=200, content_type=content_type, body=candidate.read_bytes())
            return
    route.continue_()


def route_local_api(route):
    route.fulfill(status=200, content_type="application/json", body="{}")


def inspect_layout(browser, viewport, screenshot_path):
    page = browser.new_page(viewport=viewport, is_mobile=viewport["width"] < 600)
    console_errors = []
    page.on(
        "console",
        lambda message: console_errors.append(message.text) if message.type == "error" else None,
    )
    if USE_LOCAL_STATIC:
        page.route("**/*", route_local_static)
        page.route("**/api/**", route_local_api)
    install_common_config_route(page)
    page.goto(f"{BASE_URL}/codex_api", wait_until="domcontentloaded", timeout=30_000)
    page.wait_for_selector("#codex-default-config-list tr", state="attached", timeout=10_000)
    btn_box = page.locator("#codex-global-settings-open").bounding_box()
    bar_box = page.locator("#llm-global-options-bar").bounding_box()
    if bar_box and btn_box:
        assert (bar_box["x"] + bar_box["width"]) - (btn_box["x"] + btn_box["width"]) <= 1.0
    page.locator("#codex-global-settings-open").click()
    page.wait_for_selector("#codex-global-settings-dialog[open]")

    defaults = page.locator("#codex-api-global-defaults")
    defaults.scroll_into_view_if_needed()
    assert defaults.is_visible()
    assert page.locator("#codex-common-approval-never").is_checked()
    assert page.locator("#codex-common-sandbox-full-access").is_checked() is (not USE_LOCAL_STATIC)
    assert page.locator("#codex-common-animations-disabled").is_checked()
    assert page.locator("#codex-common-config-path").text_content() == (
        "/home/root/.codex/config.toml"
    )
    assert page.locator(".codex-default-config-scope").count() >= 1
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
    captured_common_payloads = []
    page.on(
        "console",
        lambda message: console_errors.append(message.text) if message.type == "error" else None,
    )
    if USE_LOCAL_STATIC:
        page.route("**/*", route_local_static)
        page.route("**/api/**", route_local_api)
    install_common_config_route(page, captured_common_payloads)
    page.goto(f"{BASE_URL}/codex_api", wait_until="domcontentloaded", timeout=30_000)
    page.wait_for_selector("#codex-default-config-list tr", state="attached", timeout=10_000)
    page.locator("#codex-global-settings-open").click()
    page.wait_for_selector("#codex-global-settings-dialog[open]")

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
                {"codex_default_config_entries": payload["codex_default_config_entries"]}
            ),
        )

    page.route("**/api/settings", capture_settings_put)
    page.locator("#codex-api-global-defaults").scroll_into_view_if_needed()
    page.locator("#codex-common-approval-never").uncheck()
    page.locator("#codex-common-sandbox-full-access").check()
    page.locator("#codex-common-animations-disabled").uncheck()
    page.locator("#codex-common-config-save").click()
    page.wait_for_selector("#codex-common-config-status[data-tone=ok]")
    page.locator("#codex-default-config-add").click()
    added_row = page.locator("#codex-default-config-list tr").last
    added_row.locator(".codex-default-config-key-input").fill("features.goals")
    added_row.locator(".codex-default-config-value-input").fill("true")
    page.locator("#codex-default-config-save").click()
    page.wait_for_selector("#codex-default-config-status[data-tone=ok]")

    assert len(captured_payloads) == 1
    assert captured_common_payloads == [
        {
            "approval_never": False,
            "sandbox_full_access": True,
            "animations_disabled": False,
            "alt_screen_disabled": False,
        }
    ]
    assert set(captured_payloads[0]) == {"codex_default_config_entries"}
    assert captured_payloads[0]["codex_default_config_entries"][-1] == {
        "key": "features.goals",
        "value": "true",
    }
    assert set(captured_payloads[0]) == {"codex_default_config_entries"}
    assert console_errors == []
    page.screenshot(path="/tmp/webclx-codex-api-defaults-save.png", full_page=True)
    page.close()
    return len(captured_payloads[0]["codex_default_config_entries"])


def main():
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        result = {
            "mobile": inspect_layout(
                browser,
                {"width": 390, "height": 844},
                "/tmp/webclx-codex-api-defaults-mobile.png",
            ),
            "desktop": inspect_layout(
                browser,
                {"width": 1440, "height": 900},
                "/tmp/webclx-codex-api-defaults-desktop.png",
            ),
            "tablet": inspect_layout(
                browser,
                {"width": 768, "height": 1024},
                "/tmp/webclx-codex-api-defaults-tablet.png",
            ),
            "savedDefaultCount": inspect_save(browser),
        }
        browser.close()
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
