import asyncio
import json
import os
import tempfile
from pathlib import Path

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111").rstrip("/")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome",
)
PAGES = {
    "workspace": "/",
    "terminal": "/terminal",
    "agent": "/agent",
    "login": "/login",
}
VIEWPORTS = {
    "desktop": {"width": 1440, "height": 900},
    "mobile": {"width": 375, "height": 812},
}


async def inspect_page(page, name, path, viewport_name, screenshot_dir):
    console_errors = []
    page_errors = []
    failed_responses = []
    page.on(
        "console",
        lambda message: console_errors.append(message.text)
        if message.type == "error"
        else None,
    )
    page.on("pageerror", lambda error: page_errors.append(str(error)))
    page.on(
        "response",
        lambda response: failed_responses.append(
            {"status": response.status, "url": response.url}
        )
        if response.status >= 400 and response.url.startswith(BASE_URL)
        else None,
    )

    response = await page.goto(f"{BASE_URL}{path}", wait_until="domcontentloaded")
    assert response is not None and response.ok, f"{name}: navigation failed"
    await page.wait_for_selector("#webclx-language-select", state="visible")
    await page.wait_for_timeout(800)

    select = page.locator("#webclx-language-select")
    await select.focus()
    assert await select.evaluate("element => element === document.activeElement")
    await select.select_option("en")
    await page.wait_for_function("document.documentElement.lang === 'en'")
    assert await select.get_attribute("aria-label") == "Language"

    dynamic_text = await page.evaluate(
        """
        async () => {
          const marker = document.createElement('button');
          marker.id = 'i18n-live-dynamic-marker';
          marker.textContent = '发送消息';
          marker.setAttribute('aria-label', '关闭');
          document.body.append(marker);
          await new Promise(resolve => setTimeout(resolve, 0));
          const result = {
            text: marker.textContent,
            ariaLabel: marker.getAttribute('aria-label'),
          };
          marker.remove();
          return result;
        }
        """
    )
    assert dynamic_text == {"text": "Send message", "ariaLabel": "Close"}

    untranslated_controls = await page.evaluate(
        """
        () => {
          const chinese = /[\u3400-\u9fff]/;
          const selectors = [
            'button', 'label', 'option', 'th', 'h1', 'h2', 'h3',
            '[aria-label]', '[aria-description]', '[placeholder]', '[title]',
            '.section-label', '.agent-empty-text', '.terminal-fab-item-label',
            '.agent-session-item > span:first-child',
          ].join(',');
          const ignored = [
            '#terminal-output', '.xterm', '.terminal-screen',
            '#session-switcher', '#idle-session-switcher',
            '#directory-session-list', '#sessions-session-list',
            '.workspace-icon-select-trigger',
            '.terminal-session-picker',
          ].join(',');
          const values = [];
          for (const element of document.querySelectorAll(selectors)) {
            if (element.closest(ignored)) continue;
            if (element.getClientRects().length === 0) continue;
            for (const [kind, value] of [
              ['text', element.matches('button, label, option, th, h1, h2, h3, .section-label, .agent-empty-text, .terminal-fab-item-label') ? element.textContent : ''],
              ['aria-label', element.getAttribute('aria-label')],
              ['aria-description', element.getAttribute('aria-description')],
              ['placeholder', element.getAttribute('placeholder')],
              ['title', element.getAttribute('title')],
            ]) {
              const normalized = String(value || '').replace(/\\s+/g, ' ').trim();
              if (normalized && chinese.test(normalized)) {
                const identity = `${element.tagName.toLowerCase()}#${element.id}.${element.className || ''}`;
                values.push(`${kind} [${identity}]: ${normalized}`);
              }
            }
          }
          return [...new Set(values)].sort();
        }
        """
    )
    assert not untranslated_controls, {name: untranslated_controls}

    layout = await page.evaluate(
        """
        () => {
          const control = document.querySelector('#webclx-language-control');
          const box = control.getBoundingClientRect();
          return {
            pageOverflow: document.documentElement.scrollWidth > innerWidth + 1,
            overflowElements: Array.from(document.querySelectorAll('body *'))
              .map(element => {
                const box = element.getBoundingClientRect();
                return {
                  identity: `${element.tagName.toLowerCase()}#${element.id}.${element.className || ''}`,
                  left: Math.round(box.left),
                  right: Math.round(box.right),
                  width: Math.round(box.width),
                  text: String(element.textContent || '').replace(/\\s+/g, ' ').trim().slice(0, 80),
                };
              })
              .filter(item => item.width > 0 && (item.left < -1 || item.right > innerWidth + 1))
              .slice(0, 30),
            controlVisible: box.width > 0 && box.height > 0,
            controlInViewport: box.left >= 0 && box.right <= innerWidth + 1,
            storedLocale: localStorage.getItem('webclx:locale'),
            documentLocale: document.documentElement.lang,
          };
        }
        """
    )
    assert not layout["pageOverflow"], {name: layout}
    assert layout["controlVisible"] and layout["controlInViewport"], {name: layout}
    assert layout["storedLocale"] == "en" and layout["documentLocale"] == "en"

    await page.screenshot(
        path=str(screenshot_dir / f"{name}-{viewport_name}-en.png"),
        full_page=True,
    )
    await select.select_option("zh-CN")
    await page.wait_for_function("document.documentElement.lang === 'zh-CN'")
    assert await select.get_attribute("aria-label") == "语言"

    await page.reload(wait_until="domcontentloaded")
    await page.wait_for_selector("#webclx-language-select", state="visible")
    assert await page.locator("#webclx-language-select").input_value() == "zh-CN"
    assert await page.locator("html").get_attribute("lang") == "zh-CN"

    assert not page_errors and not console_errors and not failed_responses, {
        name: {
            "pageErrors": page_errors,
            "consoleErrors": console_errors,
            "failedResponses": failed_responses,
        }
    }
    return {
        "path": path,
        "viewport": viewport_name,
        "languageControl": "keyboard-focusable",
        "dynamicTranslation": dynamic_text,
        "untranslatedControls": 0,
        "overflow": False,
        "consoleErrors": 0,
        "failedResponses": 0,
    }


async def main():
    screenshot_dir = Path(tempfile.mkdtemp(prefix="webclx-i18n-qa-"))
    launch_options = {"headless": True, "args": ["--no-sandbox", "--disable-gpu"]}
    chromium = Path(CHROMIUM)
    if chromium.is_file():
        launch_options["executable_path"] = str(chromium)

    results = []
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(**launch_options)
        try:
            for viewport_name, viewport in VIEWPORTS.items():
                for name, path in PAGES.items():
                    context = await browser.new_context(viewport=viewport, locale="zh-CN")
                    try:
                        await context.route(
                            "**/api/workspace-icon?*",
                            lambda route: route.fulfill(
                                status=200,
                                content_type="image/svg+xml",
                                body='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1 1"/>',
                            ),
                        )
                        page = await context.new_page()
                        results.append(
                            await inspect_page(
                                page, name, path, viewport_name, screenshot_dir
                            )
                        )
                    finally:
                        await context.close()
        finally:
            await browser.close()

    print(
        json.dumps(
            {"screenshots": str(screenshot_dir), "checks": results},
            ensure_ascii=False,
            indent=2,
        )
    )


if __name__ == "__main__":
    asyncio.run(main())
