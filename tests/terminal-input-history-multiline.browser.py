import asyncio
import json
import mimetypes
from pathlib import Path
from urllib.parse import urlparse

from playwright.async_api import async_playwright


ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / "static"
CHROME = "/home/codes/third_party/browser-tools/bin/chromium"
SESSION_ID = "history-drag-fixture"


async def main():
    errors = []

    async with async_playwright() as p:
        browser=await p.chromium.launch(executable_path=CHROME,headless=True,args=['--no-sandbox','--disable-gpu'])
        context=await browser.new_context(viewport={'width':1280,'height':820})
        page=await context.new_page()
        page.on('pageerror',lambda e:errors.append(str(e)))
        page.on('console',lambda m:errors.append(m.text) if m.type=='error' and 'favicon' not in m.text.lower() else None)
        async def http(route):
            u=urlparse(route.request.url); path=u.path
            if path=='/terminal': await route.fulfill(status=200,content_type='text/html; charset=utf-8',body=(STATIC/'terminal.html').read_bytes()); return
            if path.startswith('/assets/') or path.startswith('/vendor/'):
                asset=STATIC/path.removeprefix('/assets/')
                if asset.is_file(): await route.fulfill(status=200,content_type=mimetypes.guess_type(asset.name)[0] or 'application/octet-stream',body=asset.read_bytes()); return
            if path=='/api/settings': await route.fulfill(json={'workspace_dir':'/home/codes'}); return
            if path==f'/api/terminal/sessions/{SESSION_ID}/input-history':
                await route.fulfill(json={'entries':[{'created_at':1760000000000,'text':'第一行历史内容\n第二行历史内容\n第三行历史内容'}]}); return
            if path=='/api/terminal/sessions': await route.fulfill(json={'path':'webClx','display_path':'/webClx','sessions':[{'id':SESSION_ID,'name':'History drag','path':'webClx','display_path':'/webClx','idle':False,'connected':True}]}); return
            if path.startswith('/api/'): await route.fulfill(json={}); return
            await route.fulfill(status=404,body='not found')
        async def ws(socket): socket.send(b'\r\n[root@webclx webClx]# ')
        await page.route('**/*',http); await page.route_web_socket('**/api/terminal/ws?**',ws)
        await page.goto(f'http://webclx.test/terminal?path=webClx&session={SESSION_ID}',wait_until='networkidle')
        await page.wait_for_function(f"() => state.activeSessionId === '{SESSION_ID}'",timeout=15000)
        await page.locator('#terminal-input-history-button').click()
        ta=page.locator('#terminal-input-history-list textarea').first; await ta.wait_for(state='visible',timeout=10000)
        box=await ta.bounding_box(); assert box
        await page.mouse.move(box['x']+12,box['y']+10); await page.mouse.down()
        await page.mouse.move(box['x']+box['width']-8,box['y']+box['height']-8,steps=12)
        await page.mouse.move(25,25,steps=8); await page.mouse.up()
        assert await page.locator('#terminal-input-history-dialog').evaluate('el=>el.open'),'dialog closed after drag selection ended outside'
        assert await ta.evaluate('el=>el.readOnly'),'history textarea must be read-only'
        assert await ta.evaluate("el=>el.value.includes('\\n')"),'multiline content lost'
        await page.evaluate("""() => {
          window.__historyCopiedText = null;
          Object.defineProperty(navigator, 'clipboard', {
            configurable: true,
            value: { writeText: async (text) => { window.__historyCopiedText = text; } },
          });
        }""")
        await ta.select_text()
        await ta.click(button='right')
        menu=page.locator('#terminal-context-menu')
        await menu.wait_for(state='visible',timeout=5000)
        assert await menu.locator('#terminal-context-copy-all').inner_text()=='复制选中文本','history context menu does not expose selected-copy'
        await menu.locator('#terminal-context-copy-all').click()
        await page.wait_for_function("""() => {
          const text = window.__historyCopiedText || '';
          return text.includes('第一行历史内容') && text.includes('第三行历史内容');
        }""",timeout=5000)
        assert await page.locator('#terminal-input-history-dialog').evaluate('el=>el.open'),'history dialog closed after context copy'
        await page.screenshot(path='/tmp/webclx-input-history-multiline.png',clip=await page.locator('#terminal-input-history-dialog').bounding_box())
        await page.locator('#terminal-input-history-close').click()
        assert not await page.locator('#terminal-input-history-dialog').evaluate('el=>el.open')
        await page.locator('#terminal-host').click(button='right')
        await page.locator('#terminal-context-menu').wait_for(state='visible',timeout=5000)
        assert await page.evaluate("() => document.getElementById('terminal-context-menu').parentElement === document.body"),'terminal context menu was not restored to the page after dialog use'
        await page.locator('#terminal-context-copy-all').click()
        await page.wait_for_function("() => Boolean(window.__historyCopiedText)",timeout=5000)
        await context.close()
        await browser.close()
    assert not errors,errors
    print('PASS: dialog stays open; multiline textarea is selectable; context menu copies the selection; close button still works')
asyncio.run(main())
