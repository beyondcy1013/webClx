"""Android language onboarding persists once per site; desktop retains its selector."""
from pathlib import Path
from playwright.sync_api import sync_playwright

STATIC = Path(__file__).resolve().parents[1] / 'static'
ANDROID = 'Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/130.0.0.0 Mobile Safari/537.36'
HTML = '<html><head><meta charset="utf-8"></head><body><header class="browser-topbar"><nav>工作区</nav></header><main>设置</main><script src="/assets/i18n.js"></script></body></html>'

def serve(route):
    if route.request.url.endswith('/assets/i18n.js'):
        route.fulfill(content_type='text/javascript', body=(STATIC / 'i18n.js').read_text())
    else:
        route.fulfill(content_type='text/html', body=HTML)

with sync_playwright() as p:
    browser = p.chromium.launch(executable_path='/opt/google/chrome/chrome', headless=True, args=['--no-sandbox'])
    try:
        for language in ['zh-CN', 'en']:
            context = browser.new_context(user_agent=ANDROID, viewport={'width':390,'height':844}, is_mobile=True)
            context.route('**/*', serve)
            page = context.new_page()
            errors = []
            page.on('pageerror', lambda error: errors.append(str(error)))
            page.goto('http://127.0.0.1:11111/onboarding')
            assert page.locator('#webclx-language-welcome').is_visible()
            assert page.locator('#webclx-language-control').count() == 0
            page.locator(f'button[data-locale="{language}"]').click()
            assert page.locator('#webclx-language-welcome').count() == 0
            for path in ['terminal', 'llm_api']:
                page.goto(f'http://127.0.0.1:11111/{path}')
                assert page.locator('#webclx-language-welcome').count() == 0
                assert page.locator('#webclx-language-control').count() == 0
                assert page.evaluate('document.documentElement.lang') == language
            assert not errors, errors
            context.close()
        # A choice saved before this update should skip onboarding as well.
        context = browser.new_context(user_agent=ANDROID)
        context.route('**/*', serve)
        context.add_init_script('localStorage.setItem("webclx:locale", "zh-CN")')
        page = context.new_page()
        page.goto('http://127.0.0.1:11111/llm_api')
        assert page.locator('#webclx-language-welcome, #webclx-language-control').count() == 0
        context.close()
        context = browser.new_context()
        context.route('**/*', serve)
        page = context.new_page()
        page.goto('http://127.0.0.1:11111/llm_api')
        assert page.locator('#webclx-language-control').is_visible()
        assert page.locator('#webclx-language-welcome').count() == 0
        context.close()
        print('PASS Android: initial Chinese/English choice, cross-page persistence, existing choice; desktop selector preserved')
    finally:
        browser.close()
