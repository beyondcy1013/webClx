import asyncio
import base64
import hashlib
import hmac
import json
import os
import tempfile
import time
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


BASE_URL = os.environ.get("WEBCLX_TEST_BASE_URL", "http://127.0.0.1:11111").rstrip("/")
CHROMIUM = os.environ.get(
    "WEBCLX_TEST_CHROMIUM",
    "/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome",
)
PAGES = {
    "workspace": ("/", "/workspace"),
    "terminal": ("/terminal", "/terminal"),
    "agent": ("/agent", "/agent"),
    "login": ("/login", "/login"),
}
VIEWPORTS = {
    "desktop": {"width": 1440, "height": 900},
    "mobile": {"width": 375, "height": 812},
}


def local_auth_headers():
    token_file = os.environ.get("WEBCLX_LOCAL_TOKEN_FILE", "").strip()
    if not token_file:
        return {}
    hostname = (urlparse(BASE_URL).hostname or "").lower()
    if hostname not in {"127.0.0.1", "::1", "localhost"}:
        raise RuntimeError("refusing to send the local API token to a non-loopback URL")
    token = Path(token_file).read_text(encoding="utf-8").strip()
    if len(token) != 64 or any(character not in "0123456789abcdefABCDEF" for character in token):
        raise RuntimeError("WEBCLX_LOCAL_TOKEN_FILE does not contain a valid local API token")
    return {"X-WebClx-Local-Token": token}


def local_session_cookie():
    secret_file = os.environ.get("WEBCLX_SESSION_SECRET_FILE", "").strip()
    credentials_file = os.environ.get("WEBCLX_CREDENTIALS_FILE", "").strip()
    if not secret_file and not credentials_file:
        return None
    if not secret_file or not credentials_file:
        raise RuntimeError("both WEBCLX_SESSION_SECRET_FILE and WEBCLX_CREDENTIALS_FILE are required")
    hostname = (urlparse(BASE_URL).hostname or "").lower()
    if hostname not in {"127.0.0.1", "::1", "localhost"}:
        raise RuntimeError("refusing to derive a local session cookie for a non-loopback URL")
    secret_path = Path(secret_file)
    credentials_path = Path(credentials_file)
    if secret_path.stat().st_mode & 0o077 or credentials_path.stat().st_mode & 0o077:
        raise RuntimeError("local session inputs must be owner-only files")
    secret = secret_path.read_bytes()
    credentials = json.loads(credentials_path.read_text(encoding="utf-8"))
    username = str(credentials.get("username", "")).strip()
    if len(secret) < 32 or not username:
        raise RuntimeError("local session inputs are invalid")
    payload = json.dumps(
        {"user": username, "exp": int(time.time()) + 300},
        separators=(",", ":"),
    ).encode("utf-8")
    payload_b64 = base64.urlsafe_b64encode(payload).decode("ascii").rstrip("=")
    signature = hmac.new(secret, payload_b64.encode("ascii"), hashlib.sha256).hexdigest()
    return {
        "name": "webclx_session",
        "value": f"{payload_b64}.{signature}",
        "url": BASE_URL,
        "httpOnly": True,
        "sameSite": "Strict",
    }


async def inspect_page(page, name, path, expected_path, viewport_name, screenshot_dir):
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
    assert urlparse(page.url).path == expected_path, f"{name}: redirected to {page.url}"
    await page.wait_for_selector("#webclx-language-select", state="visible")
    await page.wait_for_timeout(800)

    select = page.locator("#webclx-language-select")
    assert await select.input_value() == "zh-CN"
    assert await page.locator("html").get_attribute("lang") == "zh-CN"
    assert await page.evaluate("localStorage.getItem('webclx:locale')") is None
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
          const hostBox = control.parentElement.getBoundingClientRect();
          const controlStyle = getComputedStyle(control);
          const hostStyle = getComputedStyle(control.parentElement);
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
            controlAtHostTopRight: hostBox.right - box.right <= 16 && box.top - hostBox.top <= 16,
            controlBox: { left: box.left, top: box.top, right: box.right, bottom: box.bottom },
            hostBox: { left: hostBox.left, top: hostBox.top, right: hostBox.right, bottom: hostBox.bottom },
            controlStyle: { alignSelf: controlStyle.alignSelf, justifySelf: controlStyle.justifySelf, margin: controlStyle.margin },
            hostStyle: { display: hostStyle.display, padding: hostStyle.padding, gridTemplateColumns: hostStyle.gridTemplateColumns },
            hostClass: control.parentElement.className,
            storedLocale: localStorage.getItem('webclx:locale'),
            documentLocale: document.documentElement.lang,
          };
        }
        """
    )
    assert not layout["pageOverflow"], {name: layout}
    assert layout["controlVisible"] and layout["controlInViewport"], {name: layout}
    assert layout["controlAtHostTopRight"], {name: layout}
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
    auth_headers = local_auth_headers()
    session_cookie = local_session_cookie()
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(**launch_options)
        try:
            for viewport_name, viewport in VIEWPORTS.items():
                for name, (path, expected_path) in PAGES.items():
                    context = await browser.new_context(
                        viewport=viewport,
                        locale="zh-CN",
                        extra_http_headers=auth_headers,
                    )
                    try:
                        if session_cookie:
                            await context.add_cookies([session_cookie])
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
                                page,
                                name,
                                path,
                                expected_path,
                                viewport_name,
                                screenshot_dir,
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
