"""Exercise real terminal page scripts and touch menu actions; transport is mocked."""
import asyncio
import mimetypes
import json
from pathlib import Path
from urllib.parse import urlparse
from playwright.async_api import async_playwright
ROOT = Path(__file__).resolve().parents[1]
STATIC = ROOT / 'static'

async def main():
 async with async_playwright() as p:
  browser = await p.chromium.launch(executable_path='/home/third_party/browser-tools/bin/chromium', args=['--no-sandbox'])
  try:
   for touch in [False, True]:
    context = await browser.new_context(viewport={'width':393 if touch else 1280,'height':851}, is_mobile=touch, has_touch=touch, user_agent='Mozilla/5.0 (Linux; Android 14) AppleWebKit/537.36 Chrome/130.0 Mobile Safari/537.36 webClxAndroid/1.8.21' if touch else None)
    page = await context.new_page()
    errors=[]; sent=[]
    page.on('pageerror', lambda error: errors.append(str(error)))
    async def http(route):
     path=urlparse(route.request.url).path
     if path=='/terminal': await route.fulfill(content_type='text/html',body=(STATIC/'terminal.html').read_bytes()); return
     if path.startswith('/assets/'):
      asset=STATIC/path.removeprefix('/assets/')
      if asset.is_file(): await route.fulfill(content_type=mimetypes.guess_type(asset.name)[0] or 'application/octet-stream',body=asset.read_bytes()); return
     if path=='/api/settings': await route.fulfill(json={'workspace_dir':'/home/codes','terminal_function_commands':[{'key':'system_temperature','label':'系统温度','action':'send_text','command':'! echo temperature'}]}); return
     if path=='/api/terminal/sessions': await route.fulfill(json={'path':'tools/webClx','sessions':[{'id':'fixture','name':'Fixture','path':'tools/webClx','connected':True}]}); return
     if path.endswith('/input-history'): await route.fulfill(json={'entries':[{'text':'测试对话历史','created_at':1760000000000}]}); return
     if path.startswith('/api/'): await route.fulfill(json={}); return
     await route.fulfill(status=404)
    async def ws(socket):
     socket.on_message(lambda message: sent.append(message))
     socket.send(b'\r\n[root@host webClx]# ')
    await page.route('**/*',http)
    await page.route_web_socket('**/api/terminal/ws?**',ws)
    await page.goto('http://webclx.test/terminal?path=tools%2FwebClx&session=fixture',wait_until='networkidle')
    await page.wait_for_function("state.activeSessionId === 'fixture'")
    activate = lambda selector: page.locator(selector).tap() if touch else page.locator(selector).click()
    await activate('#terminal-function-command-button')
    await activate('#terminal-universal-shortcut')
    assert await page.locator('#terminal-shortcut-dialog').is_visible(), errors
    assert not [m for m in sent if isinstance(m, str) and json.loads(m).get('type') == 'input'], sent
    await activate('#terminal-shortcut-cancel')
    if not await page.locator('#terminal-input-history-button').is_visible(): await activate('#terminal-fab-toggle')
    await activate('#terminal-input-history-button')
    await page.locator('#terminal-input-history-list textarea').first.wait_for(state='visible')
    assert await page.locator('#terminal-input-history-dialog').evaluate('el=>el.open')
    assert not errors, errors
    await context.close()
  finally: await browser.close()
 print('PASS: full desktop/Android touch page, shortcut opens without input; history dialog loads')
asyncio.run(main())
