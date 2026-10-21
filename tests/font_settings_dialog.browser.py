import mimetypes
from pathlib import Path
from urllib.parse import urlparse

from playwright.sync_api import sync_playwright


STATIC = Path("/home/bin/webclx/static")
BASE_URL = "http://127.0.0.1:11111"


def serve_deployed_static(route, request):
    path = urlparse(request.url).path
    if path == "/":
        route.fulfill(
            status=200,
            content_type="text/html; charset=utf-8",
            body=(STATIC / "index.html").read_bytes(),
        )
        return
    if path.startswith("/assets/"):
        candidate = (STATIC / path.removeprefix("/assets/")).resolve()
        if candidate.is_relative_to(STATIC) and candidate.is_file():
            route.fulfill(
                status=200,
                content_type=mimetypes.guess_type(candidate.name)[0]
                or "application/octet-stream",
                body=candidate.read_bytes(),
            )
            return
    route.fulfill(status=401, content_type="application/json", body='{"error":"qa"}')


def inspect(browser, width, height, mobile):
    page = browser.new_page(viewport={"width": width, "height": height}, is_mobile=mobile)
    page.route("**/*", serve_deployed_static)
    page.goto(BASE_URL, wait_until="domcontentloaded")
    page.locator("#tab-settings").click()
    page.locator("#settings-tab-appearance").click()

    assert page.locator("#font-size-tier-1-input").is_hidden()
    page.locator("#font-settings-open").click()
    assert page.locator("#font-settings-dialog").is_visible()
    assert page.evaluate("document.activeElement.id") == "font-settings-close"
    assert page.evaluate("document.activeElement.matches('input, textarea, [contenteditable=true]')") is False
    assert page.evaluate("document.documentElement.scrollWidth <= document.documentElement.clientWidth + 2")

    page.locator("#font-settings-close").click()
    assert page.locator("#font-settings-dialog").is_hidden()
    assert page.evaluate("document.activeElement.id") == "font-settings-open"
    page.close()


with sync_playwright() as playwright:
    browser = playwright.chromium.launch(headless=True)
    inspect(browser, 1440, 900, False)
    inspect(browser, 390, 844, True)
    browser.close()

print("font settings dialog browser checks passed")
