"""Test UI for standalone terminals joining existing groups."""
import asyncio
import mimetypes
import os
from pathlib import Path
from urllib.parse import urlparse
from playwright.async_api import async_playwright

STATIC = Path(__file__).resolve().parents[1] / 'static'
CHROMIUM = os.environ.get(
    'WEBCLX_TEST_CHROMIUM',
    '/home/third_party/browser-tools/bin/chromium'
    if Path('/home/third_party/browser-tools/bin/chromium').is_file()
    else '/home/root/.cache/ms-playwright/chromium-1217/chrome-linux64/chrome',
)

async def main():
    async with async_playwright() as p:
        browser = await p.chromium.launch(
            executable_path=CHROMIUM,
            headless=True,
            args=['--no-sandbox', '--disable-setuid-sandbox'],
        )
        page = await browser.new_page(viewport={'width': 1200, 'height': 800})
        errors, requests, message_requests = [], [], []
        page.on('pageerror', lambda err: errors.append(str(err)))

        # 场景：
        # s1: 现有群组 terminal-team 的普通管理员
        # s2: 现有群组 terminal-team 的执行者
        # s3: 单独终端，没有群组 (standalone)
        sessions = [
            dict(id='s1', name='项目_管理员', path='webClx', display_path='/webClx', idle=False,
                 connected=True, created_at=1, last_opened_at=10, group_token='group-team',
                 group_name='开发攻坚组', group_color='#3388ff', group_role='普通管理员'),
            dict(id='s2', name='项目_执行者1', path='webClx', display_path='/webClx', idle=False,
                 connected=True, created_at=2, last_opened_at=9, group_token='group-team',
                 group_name='开发攻坚组', group_color='#3388ff', group_role='执行者'),
            dict(id='s3', name='项目_独立终端', path='webClx', display_path='/webClx', idle=False,
                 connected=True, created_at=3, last_opened_at=8, group_token='',
                 group_name='', group_color='', group_role=''),
            dict(id='s4', name='其它群组_终端', path='webClx', display_path='/webClx', idle=False,
                 connected=True, created_at=4, last_opened_at=7, group_token='group-other',
                 group_name='其它无关业务组', group_color='#22aa66', group_role='执行者'),
        ]

        async def http(route):
            path = urlparse(route.request.url).path
            if path == '/terminal':
                await route.fulfill(content_type='text/html', body=(STATIC / 'terminal.html').read_bytes())
            elif path.startswith('/assets/'):
                asset = STATIC / path.removeprefix('/assets/')
                await route.fulfill(status=200 if asset.is_file() else 404,
                                    content_type=mimetypes.guess_type(asset.name)[0] or 'application/octet-stream',
                                    body=asset.read_bytes() if asset.is_file() else b'')
            elif path == '/api/settings':
                await route.fulfill(json={'workspace_dir': '/home/codes', 'desktop_terminal_soft_keyboard_enabled': True})
            elif path == '/api/terminal/sessions':
                await route.fulfill(json={'path': 'webClx', 'display_path': '/webClx', 'sessions': sessions})
            elif path.endswith('/group'):
                requests.append((path, route.request.post_data_json))
                # 模拟后端更新
                members = route.request.post_data_json['member_ids']
                for s in sessions:
                    if s['id'] in members:
                        s['group_token'] = 'group-team-merged'
                        s['group_name'] = route.request.post_data_json.get('group_name', '')
                        s['group_color'] = route.request.post_data_json.get('group_color', '')
                await route.fulfill(json={'ok': True})
            elif path == '/api/terminal/sessions/message':
                message_requests.append((path, route.request.post_data_json))
                await route.fulfill(json={'ok': True, 'submitted': True})
            elif path.startswith('/api/terminal/sessions/') and route.request.method == 'PUT':
                session = next(s for s in sessions if s['id'] == path.removeprefix('/api/terminal/sessions/'))
                session['name'] = route.request.post_data_json.get('name', session['name'])
                await route.fulfill(json=dict(session))
            elif path == '/api/terminal/resume-archives':
                await route.fulfill(json={'archives': []})
            elif path in ['/api/terminal/scheduled-inputs', '/api/terminal/auto-continue-tasks']:
                await route.fulfill(json={'tasks': []})
            else:
                await route.fulfill(json={})

        async def websocket(ws):
            ws.send(b'ready\r\n')

        await page.route('**/*', http)
        await page.route_web_socket('**/api/terminal/ws?**', websocket)

        # 以独立终端 s3 身份进入页面
        await page.goto('http://webclx.test/terminal?path=webClx&session=s3', wait_until='domcontentloaded')
        await page.wait_for_function("typeof state !== 'undefined' && state.activeSessionId === 's3'")

        # 打开终端分组对话框
        await page.locator('#terminal-tools-button').click()
        await page.locator('#terminal-tools-menu #terminal-group-sessions').click()
        await page.locator('#terminal-group-dialog').wait_for(state='visible')

        # 1. 验证加入现有群组容器可见，且下拉框中有现有群组选项
        join_container = page.locator('#terminal-group-join-container')
        assert await join_container.is_visible(), "加入现有群组容器应在有群组时可见"

        join_select = page.locator('#terminal-group-join-select')
        options_text = await join_select.locator('option').all_text_contents()
        assert any('开发攻坚组' in opt for opt in options_text), f"下拉选项中应包含现有群组: {options_text}"

        # 2. 检查会话列表中各终端的标记：s1 和 s2 应该有 '加入: 开发攻坚组' 徽章按钮
        s1_marker = page.locator('#terminal-group-session-list .terminal-group-session-option[data-session-name="项目_管理员"] .terminal-group-session-marker.is-other-group')
        assert await s1_marker.is_visible(), "其他现有群组成员应展示加入徽章"
        assert '加入: 开发攻坚组' in await s1_marker.text_content()

        # 3. 先测试切回空，再测试通过点击成员行里的徽章加入群组
        # 先切回空选择
        rename_button = page.locator('#terminal-group-rename-button')
        assert await rename_button.is_visible(), "命名/更名按钮应可见"
        await join_select.select_option(value='')
        assert await rename_button.is_disabled(), "未选群组时命名/更名按钮应处于禁用状态"
        checked_after_reset = await page.evaluate("""
            [...document.querySelectorAll('#terminal-group-session-list input[type="checkbox"]')]
                .filter(cb => cb.checked).map(cb => cb.value)
        """)
        assert checked_after_reset == ['s3'], f"清空选择后应只勾选当前终端: {checked_after_reset}"

        # 点击 s1 行旁边的「加入: 开发攻坚组」徽章按钮
        await s1_marker.click()
        assert not await rename_button.is_disabled(), "选中群组后命名/更名按钮应可用"

        # 验证自动勾选了现有群组的所有成员，并保留当前终端 s3
        checked_boxes = await page.evaluate("""
            [...document.querySelectorAll('#terminal-group-session-list input[type="checkbox"]')]
                .filter(cb => cb.checked).map(cb => cb.value)
        """)
        assert set(checked_boxes) == {'s1', 's2', 's3'}, f"点击徽章后勾选应包含原群组全员和当前终端: {checked_boxes}"

        # 验证下拉框也同步选中了
        assert await join_select.input_value() == 'group-team', "点击徽章后下拉框应同步选中"

        # 验证分组名称与颜色已自动同步
        name_val = await page.locator('#terminal-group-name').input_value()
        color_val = await page.locator('#terminal-group-color').input_value()
        assert name_val == '开发攻坚组', f"分组名称应填入: {name_val}"
        assert color_val == '#3388ff', f"分组颜色应填入: {color_val}"

        # 3.5 测试「命名/更名」按钮
        dialog_prompt_handled = []
        async def handle_dialog(dialog):
            dialog_prompt_handled.append(dialog.default_value)
            await dialog.accept("新攻坚舰队")
        page.on('dialog', lambda d: asyncio.create_task(handle_dialog(d)))
        await rename_button.click()
        assert len(dialog_prompt_handled) == 1, "点击命名/更名按钮应触发 prompt 提示框"
        assert dialog_prompt_handled[0] == '开发攻坚组', "prompt 默认值应为当前群组名"
        assert await page.locator('#terminal-group-name').input_value() == '新攻坚舰队'

        # 4. 提交保存
        await page.locator('#terminal-group-form [type="submit"]').click()
        await page.locator('#terminal-group-dialog').wait_for(state='hidden')

        # 验证发送的 PUT /api/terminal/sessions/s3/group 请求
        assert len(requests) == 1
        path, payload = requests[0]
        assert path == '/api/terminal/sessions/s3/group'
        assert set(payload['member_ids']) == {'s1', 's2', 's3'}
        assert payload['group_name'] == '新攻坚舰队'
        assert payload['group_color'] == '#3388ff'

        # 等待通知完成，验证通知内容准确包含“新成员：「项目_独立终端」”
        await page.evaluate("() => window.__webclxGroupNotificationPromise")
        assert len(message_requests) > 0, "应触发群组更新通知"
        notification_texts = [req['data'] for _, req in message_requests]
        assert any('新成员：「项目_独立终端」' in text for text in notification_texts), f"通知应包含新成员: {notification_texts}"
        
        # 严格验证：绝不向其它无关群组（如 s4）发送广播通知
        notified_targets = [req['target'] for _, req in message_requests]
        assert 's4' not in notified_targets, f"其它群组终端 s4 绝不应收到广播通知，实际通知目标: {notified_targets}"

        print("✔ Standalone terminal join existing group end-to-end browser test PASSED!")
        await browser.close()

if __name__ == '__main__':
    asyncio.run(main())
