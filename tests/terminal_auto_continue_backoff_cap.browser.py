import json
from pathlib import Path

from playwright.sync_api import sync_playwright


BASE_URL = "http://127.0.0.1:11111"
CHROMIUM = Path("/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome")


def inspect_settings(browser, viewport, screenshot_path):
    page = browser.new_page(viewport=viewport, is_mobile=viewport["width"] < 600)
    console_errors = []
    page_errors = []
    page.on(
        "console",
        lambda message: console_errors.append(message.text) if message.type == "error" else None,
    )
    page.on("pageerror", lambda error: page_errors.append(str(error)))
    page.goto(f"{BASE_URL}/settings", wait_until="networkidle", timeout=30_000)
    page.click("#tab-settings")
    page.click("#settings-tab-terminal")
    page.wait_for_function(
        "() => !document.querySelector('#settings-view').hidden"
        " && !document.querySelector('#settings-panel-terminal').hidden"
    )

    control = page.locator("#terminal-auto-continue-backoff-max-minutes-input")
    control.wait_for(state="visible", timeout=10_000)
    assert control.input_value() == "2"
    assert control.get_attribute("min") == "1"
    assert control.get_attribute("max") == "1440"
    assert control.get_attribute("step") == "1"
    assert page.locator("#settings-panel-terminal").is_visible()

    geometry = page.evaluate(
        """
        () => {
          const control = document.querySelector(
            '#terminal-auto-continue-backoff-max-minutes-input'
          );
          const label = control.closest('label');
          const previous = label.previousElementSibling;
          const next = label.nextElementSibling;
          const rect = element => {
            if (!element) return null;
            const value = element.getBoundingClientRect();
            return {
              left: value.left,
              right: value.right,
              top: value.top,
              bottom: value.bottom,
              width: value.width,
              height: value.height,
            };
          };
          return {
            viewportWidth: document.documentElement.clientWidth,
            pageScrollWidth: document.documentElement.scrollWidth,
            control: rect(control),
            label: rect(label),
            previous: rect(previous),
            next: rect(next),
          };
        }
        """
    )
    assert geometry["pageScrollWidth"] <= geometry["viewportWidth"] + 2, geometry
    assert geometry["control"]["width"] > 0 and geometry["control"]["height"] >= 32, geometry
    assert geometry["label"]["left"] >= 0, geometry
    assert geometry["label"]["right"] <= geometry["viewportWidth"] + 1, geometry
    if geometry["previous"]:
        assert geometry["previous"]["bottom"] <= geometry["label"]["top"] + 1, geometry
    if geometry["next"]:
        assert geometry["label"]["bottom"] <= geometry["next"]["top"] + 1, geometry
    assert console_errors == [], console_errors
    assert page_errors == [], page_errors

    control.scroll_into_view_if_needed()
    page.screenshot(path=screenshot_path, full_page=False)
    page.close()
    return geometry


def main():
    assert CHROMIUM.is_file(), CHROMIUM
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(
            executable_path=str(CHROMIUM),
            headless=True,
            args=["--no-sandbox", "--disable-gpu"],
        )
        result = {
            "desktop": inspect_settings(
                browser,
                {"width": 1440, "height": 900},
                "/tmp/webclx-backoff-cap-desktop.png",
            ),
            "mobile": inspect_settings(
                browser,
                {"width": 390, "height": 844},
                "/tmp/webclx-backoff-cap-mobile.png",
            ),
        }
        browser.close()
    print(json.dumps(result, ensure_ascii=True, indent=2))


if __name__ == "__main__":
    main()
