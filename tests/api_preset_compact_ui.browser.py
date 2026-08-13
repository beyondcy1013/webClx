import json
import mimetypes
import os
from pathlib import Path
from urllib.parse import urlparse

from playwright.sync_api import sync_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
BASE_URL = os.environ.get("WEBCLX_PRESET_UI_BASE_URL", "http://127.0.0.1:11111").rstrip("/")
USE_LOCAL_STATIC = os.environ.get("WEBCLX_PRESET_UI_LIVE") != "1"


def route_local_static(route, request):
    parsed = urlparse(request.url)
    if parsed.hostname not in {"127.0.0.1", "localhost"}:
        route.fulfill(status=204, content_type="text/plain", body=b"")
        return
    path = parsed.path
    if path in {"/", "/codex_api"}:
        route.fulfill(
            status=200,
            content_type="text/html; charset=utf-8",
            body=(STATIC / "index.html").read_bytes(),
        )
        return
    if path.startswith("/assets/"):
        relative_path = path.removeprefix("/assets/")
        candidate = (STATIC / relative_path).resolve()
        if candidate.is_relative_to(STATIC) and candidate.is_file():
            content_type = mimetypes.guess_type(candidate.name)[0] or "application/octet-stream"
            route.fulfill(status=200, content_type=content_type, body=candidate.read_bytes())
            return
    route.continue_()


def inspect_mobile(browser):
    page = browser.new_page(viewport={"width": 390, "height": 844}, is_mobile=True)
    console_errors = []
    page.on(
        "console",
        lambda message: console_errors.append(message.text) if message.type == "error" else None,
    )
    if USE_LOCAL_STATIC:
        page.route("**/*", route_local_static)
    page.goto(f"{BASE_URL}/codex_api", wait_until="domcontentloaded", timeout=30_000)
    page.wait_for_selector(".api-mobile-preset-row", timeout=10_000)

    first_row = page.locator(".api-mobile-preset-row").first
    first_name = first_row.locator(".api-mobile-preset-name").inner_text()
    assert first_name.strip()
    assert first_row.locator(".api-mobile-preset-primary").is_visible()
    assert first_row.locator(".preset-action-menu-trigger").is_visible()
    assert page.locator(".api-desktop-table-wrap").evaluate(
        "element => getComputedStyle(element).display"
    ) == "none"
    default_columns = first_row.locator(".api-mobile-preset-summary").evaluate(
        "element => getComputedStyle(element).gridTemplateColumns"
    )
    assert len(default_columns.split()) == 3
    page.screenshot(path="/tmp/webclx-api-presets-mobile-default.png", full_page=True)

    first_row.locator(".api-mobile-preset-identity").click()
    assert first_row.locator(".api-mobile-preset-details").is_visible()
    assert "Base URL" in first_row.locator(".api-mobile-preset-details").inner_text()

    first_row.locator(".preset-action-menu-trigger").click()
    menu = page.locator(".preset-action-menu")
    assert menu.is_visible()
    menu_text = menu.inner_text()
    for expected_action in ("切换并启动", "测试", "编辑", "删除"):
        assert expected_action in menu_text
    page.keyboard.press("Escape")
    assert not menu.is_visible()

    selection_button = page.locator("#api-preset-selection-mode")
    selection_button.click()
    assert selection_button.get_attribute("aria-pressed") == "true"
    first_checkbox = page.locator(".api-mobile-preset-checkbox").first
    assert first_checkbox.is_visible()
    first_checkbox.check()
    assert not page.locator("#api-clipboard-export").is_disabled()

    search = page.locator("#api-preset-search")
    search.fill("__no_matching_api_preset__")
    assert page.locator(".api-mobile-preset-row").count() == 0
    assert "没有匹配" in page.locator(".api-mobile-preset-empty").inner_text()
    search.fill(first_name)
    assert page.locator(".api-mobile-preset-row").count() >= 1

    geometry = page.evaluate(
        """
        () => {
          const list = document.querySelector('.api-preset-mobile-list');
          const summary = document.querySelector('.api-mobile-preset-summary');
          const controls = Array.from(summary.children).map((element) => {
            const rect = element.getBoundingClientRect();
            return { width: Math.round(rect.width), right: Math.round(rect.right) };
          });
          return {
            clientWidth: document.documentElement.clientWidth,
            scrollWidth: document.documentElement.scrollWidth,
            listClientWidth: list.clientWidth,
            listScrollWidth: list.scrollWidth,
            columns: getComputedStyle(summary).gridTemplateColumns,
            controls,
          };
        }
        """
    )
    assert geometry["scrollWidth"] <= geometry["clientWidth"] + 2
    assert geometry["listScrollWidth"] <= geometry["listClientWidth"] + 2
    assert console_errors == []
    page.screenshot(path="/tmp/webclx-api-presets-mobile.png", full_page=True)
    page.close()
    return {
        "firstName": first_name,
        "defaultColumns": default_columns,
        "geometry": geometry,
        "consoleErrors": console_errors,
    }


def inspect_desktop(browser):
    page = browser.new_page(viewport={"width": 1440, "height": 900})
    console_errors = []
    page.on(
        "console",
        lambda message: console_errors.append(message.text) if message.type == "error" else None,
    )
    if USE_LOCAL_STATIC:
        page.route("**/*", route_local_static)
    page.goto(f"{BASE_URL}/codex_api", wait_until="domcontentloaded", timeout=30_000)
    page.wait_for_selector("#api-preset-list tr", timeout=10_000)

    headers = page.locator("#api-view .auth-table thead th").all_inner_texts()
    normalized_headers = ["".join(header.split()) for header in headers]
    for removed_header in ("临切", "测试", "编辑", "删除"):
        assert removed_header not in normalized_headers
    assert normalized_headers.count("操作") == 1
    assert (
        normalized_headers.index("操作")
        == normalized_headers.index("序号") + 1
    )
    assert (
        normalized_headers.index("操作")
        < normalized_headers.index("状态指示↕")
        < normalized_headers.index("名字↕")
        < normalized_headers.index("BaseURL↕")
    )
    assert page.locator(".api-preset-mobile-list").evaluate(
        "element => getComputedStyle(element).display"
    ) == "none"
    first_actions = page.locator("#api-preset-list .api-preset-operation-cell").first
    assert first_actions.get_by_role("button", name="切换").is_visible()
    first_actions.locator(".preset-action-menu-trigger").click()
    assert page.locator(".preset-action-menu").is_visible()
    page.keyboard.press("Escape")

    selection_button = page.locator("#api-preset-selection-mode")
    selection_button.click()
    select_all = page.locator("#api-view .auth-table thead input[type=checkbox]")
    assert select_all.is_visible()
    select_all.check()
    assert page.locator("#api-preset-list .preset-selection-cell input:checked").count() > 1
    assert not page.locator("#api-clipboard-export").is_disabled()
    assert console_errors == []

    page.screenshot(path="/tmp/webclx-api-presets-desktop.png", full_page=True)
    page.close()
    return {"headers": normalized_headers, "consoleErrors": console_errors}


def main():
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(headless=True)
        result = {
            "mobile": inspect_mobile(browser),
            "desktop": inspect_desktop(browser),
        }
        browser.close()
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
