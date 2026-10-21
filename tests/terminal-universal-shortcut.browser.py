"""Shortcut dialog interaction using production HTML and script, no real terminal input."""
import asyncio
import re
from pathlib import Path
from playwright.async_api import async_playwright

ROOT = Path(__file__).resolve().parents[1]

async def main():
    html = (ROOT / 'static/terminal.html').read_text()
    dialog = re.search(r'<dialog id="terminal-shortcut-dialog"[\s\S]*?</dialog>', html).group()
    trigger = re.search(r'<button id="terminal-universal-shortcut"[^>]*>.*?</button>', html).group()
    async with async_playwright() as p:
        browser = await p.chromium.launch(executable_path='/home/third_party/browser-tools/bin/chromium', args=['--no-sandbox'])
        try:
            for width, height in [(375, 667), (1280, 800)]:
                page = await browser.new_page(viewport={'width': width, 'height': height})
                styles = '\n'.join((ROOT / 'static' / name).read_text() for name in ['styles-base.css', 'styles-auth.css', 'terminal-universal-shortcut.css'])
                await page.set_content(f'<style>{styles}</style>{trigger}{dialog}')
                await page.evaluate('''() => {
                    window.state = {activeSessionId: 'original'};
                    window.term = {modes: {}};
                    window.sent = [];
                    window.online = true;
                    window.closeTerminalFunctionCommandMenu = () => {};
                    window.focusTerminalAfterSoftKeyboardInput = () => {};
                    window.sendTerminalInputToSession = (data, id) => { if (!online) return false; sent.push({data, id}); return true; };
                }''')
                await page.add_script_tag(path=str(ROOT / 'static/terminal-universal-shortcut.js'))
                await page.evaluate('initializeTerminalUniversalShortcut()')
                await page.click('#terminal-universal-shortcut')
                await page.check('[name=ctrl]')
                await page.check('[name=alt]')
                await page.click('#terminal-shortcut-keyboard [data-key=shift]')
                assert await page.locator('#terminal-shortcut-keyboard [data-key="1"]').inner_text() == '!'
                await page.click('#terminal-shortcut-keyboard [data-key=shift]')
                await page.click('#terminal-shortcut-keyboard [data-key=x]')
                assert await page.evaluate('sent') == []
                assert await page.locator('#terminal-shortcut-keyboard [data-key=x]').get_attribute('aria-pressed') == 'true'
                await page.click('#terminal-shortcut-keyboard [data-key=ctrl]')
                assert not await page.locator('[name=ctrl]').is_checked()
                await page.click('#terminal-shortcut-keyboard [data-key=ctrl]')
                assert await page.locator('[name=ctrl]').is_checked()
                assert await page.locator('#terminal-shortcut-keyboard button').count() >= 78
                assert await page.locator('#terminal-shortcut-keyboard').evaluate('el => el.scrollWidth <= el.clientWidth')
                assert await page.locator('#terminal-shortcut-preview').inner_text() == 'Ctrl + Alt + x'
                box = await page.locator('#terminal-shortcut-dialog').bounding_box()
                assert box['x'] >= 0 and box['x'] + box['width'] <= width
                assert box['y'] >= 0 and box['y'] + box['height'] <= height
                if width == 375:
                    await page.screenshot(path='/tmp/webclx-shortcut-keyboard-mobile.png')
                await page.evaluate("state.activeSessionId = 'other'")
                await page.click('#terminal-shortcut-form button[type=submit]')
                assert await page.evaluate('sent') == [{'data': '\x1b\x18', 'id': 'original'}]
                await page.click('#terminal-universal-shortcut')
                await page.evaluate('online = false')
                await page.click('#terminal-shortcut-form button[type=submit]')
                assert await page.locator('#terminal-shortcut-dialog').is_visible()
                assert '断开' in await page.locator('#terminal-shortcut-status').inner_text()
                await page.click('#terminal-shortcut-cancel')
                assert len(await page.evaluate('sent')) == 1
                await page.click('#terminal-universal-shortcut')
                await page.keyboard.press('Escape')
                assert not await page.locator('#terminal-shortcut-dialog').is_visible()
                await page.close()
        finally:
            await browser.close()
    print('PASS: desktop/mobile dialog, modifiers, frozen target, disconnect, cancel, Escape')

asyncio.run(main())
