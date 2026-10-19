"""Terminal restore dialog browser contract; APIs are intercepted and no real terminal is touched."""
import asyncio
import mimetypes
from pathlib import Path
from urllib.parse import urlparse
from playwright.async_api import async_playwright

STATIC = Path(__file__).resolve().parents[1] / 'static'
CHROMIUM = '/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome'

async def check(browser, width, height):
    page = await browser.new_page(viewport={'width': width, 'height': height})
    errors, requests = [], []
    restore_records = [
        {
            'session_id': 's1', 'name': 'Codex 修复', 'user_name': 'root', 'path': 'webClx',
            'display_path': '/codes/webClx', 'group_name': '协作组', 'group_token': 'g1',
            'group_color': '#4f8cff', 'group_order': 1, 'group_fixed_order': True,
            'program': 'codex', 'resume_id': '019f-demo-session', 'command': 'codex resume 019f-demo-session',
            'updated_at': 1783400000000, 'created_at': 1783300000000, 'restore_count': 0,
            'recoverable': True,
        },
        {
            'session_id': 's2', 'name': 'Shell 构建', 'user_name': 'root', 'path': 'webClx',
            'display_path': '/codes/webClx', 'group_name': '', 'group_token': '', 'group_color': '',
            'group_order': 0, 'group_fixed_order': False, 'program': '', 'resume_id': '', 'command': '',
            'updated_at': 1783390000000, 'created_at': 1783290000000, 'restore_count': 0,
            'recoverable': True,
        },
    ]

    async def http(route):
        path = urlparse(route.request.url).path
        if path == '/terminal':
            await route.fulfill(content_type='text/html', body=(STATIC / 'terminal.html').read_bytes())
        elif path.startswith('/assets/'):
            asset = STATIC / path.removeprefix('/assets/')
            await route.fulfill(
                status=200 if asset.is_file() else 404,
                content_type=mimetypes.guess_type(asset.name)[0] or 'application/octet-stream',
                body=asset.read_bytes() if asset.is_file() else b'',
            )
        elif path == '/api/settings':
            await route.fulfill(json={'workspace_dir': '/home/codes'})
        elif path == '/api/terminal/sessions':
            await route.fulfill(json={
                'path': 'webClx', 'display_path': '/codes/webClx',
                'sessions': [{
                    'id': 's1', 'name': 'Codex 修复', 'path': 'webClx',
                    'display_path': '/codes/webClx', 'idle': False, 'connected': True,
                }],
            })
        elif path == '/api/terminal/workspace-restores':
            requests.append((route.request.method, route.request.post_data_json))
            if route.request.method == 'PUT':
                await route.fulfill(json={'ok': True, 'updated': len(restore_records)})
            elif route.request.method == 'POST':
                body = route.request.post_data_json or {}
                keep = set(body.get('session_ids') or [])
                for record in restore_records:
                    if record['session_id'] in keep:
                        record['restore_count'] += 1
                await route.fulfill(json={'ok': True, 'restored': len(keep)})
            else:
                await route.fulfill(json={'records': restore_records})
        elif path == '/api/terminal/resume-archives':
            await route.fulfill(json={'archives': []})
        elif path in ['/api/terminal/scheduled-inputs', '/api/terminal/auto-continue-tasks']:
            await route.fulfill(json={'tasks': []})
        else:
            await route.fulfill(json={})

    async def websocket(ws):
        ws.send(b'restore fixture ready\r\n')

    await page.route('**/*', http)
    await page.route_web_socket('**/api/terminal/ws?**', websocket)
    await page.goto('http://webclx.test/terminal?path=webClx&session=s1', wait_until='domcontentloaded')
    await page.wait_for_function("typeof state !== 'undefined' && state.activeSessionId === 's1'")

    await page.locator('#terminal-project-command-button').click()
    await page.locator('#terminal-project-command-menu [data-project-action="open_workspace_restores"]').click()
    await page.locator('#terminal-workspace-restores-dialog').wait_for(state='visible')
    await page.wait_for_function("document.querySelectorAll('#terminal-workspace-restores-tbody tr').length === 2")
    await page.wait_for_selector('#terminal-workspace-restores-tbody tr:nth-child(1) td:nth-child(2)')
    assert await page.locator('#terminal-workspace-restores-tbody tr:nth-child(1) td:nth-child(2)').text_content() == 'Codex · 019f-dem…'
    assert await page.locator('#terminal-workspace-restores-tbody tr:nth-child(1) td:nth-child(3)').text_content() == 'Codex 修复'
    assert await page.locator('#terminal-workspace-restores-tbody tr:nth-child(2) td:nth-child(2)').text_content() == 'Shell'
    box = await page.locator('#terminal-workspace-restores-dialog').bounding_box()
    assert box and box['x'] >= 0 and box['x'] + box['width'] <= width + 1, box
    assert box and box['y'] >= 0 and box['y'] + box['height'] <= height + 1, box

    await page.locator('#terminal-workspace-restores-tbody input[value="s2"]').check()
    await page.locator('#terminal-workspace-restore-selected').click()
    await page.wait_for_function("([method, body]) => method === 'POST' && body?.session_ids?.[0] === 's2'", arg=requests[-1] if requests else None)
    assert ('POST', {'session_ids': ['s2']}) in requests, requests
    await page.locator('#terminal-workspace-restores-close').click()
    assert not errors, errors
    await page.close()

async def main():
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(executable_path=CHROMIUM, headless=True)
        try:
            await check(browser, 1280, 800)
            await check(browser, 375, 700)
            print('terminal workspace restore dialog browser checks passed')
        finally:
            await browser.close()

if __name__ == '__main__':
    asyncio.run(main())
