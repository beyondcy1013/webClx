"""Real terminal page UI, intercepted session APIs; no user terminals are modified."""
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

async def check(browser, width, height):
    page = await browser.new_page(viewport={'width': width, 'height': height})
    errors, requests, message_requests = [], [], []
    page.on('pageerror', lambda error: errors.append(str(error)))
    sessions = [dict(id=f's{i}', name=f'分组测试终端 {i}', path='webClx', display_path='/webClx', idle=False,
                     connected=True, created_at=i, last_opened_at=10-i, group_token='') for i in range(1, 4)]
    fail_save = False
    async def http(route):
        nonlocal fail_save
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
            if fail_save:
                await route.fulfill(status=400, json={'error': '测试保存失败'})
            else:
                members = route.request.post_data_json['member_ids']
                for session in sessions:
                    session['group_token'] = 'test-group' if len(members) > 1 and session['id'] in members else ''
                await route.fulfill(json={'ok': True})
        elif path == '/api/terminal/sessions/message':
            message_requests.append((path, route.request.post_data_json))
            # 模拟较慢的终端投递确认（1 秒），验证对话框窗口不会等待其返回
            await asyncio.sleep(1.0)
            await route.fulfill(json={'ok': True, 'submitted': True})
        elif path.startswith('/api/terminal/sessions/') and route.request.method == 'PUT':
            session = next(s for s in sessions if s['id'] == path.removeprefix('/api/terminal/sessions/'))
            session['name'] = route.request.post_data_json['name']
            await route.fulfill(json=dict(session))
        elif path == '/api/terminal/resume-archives':
            await route.fulfill(json={'archives': []})
        elif path in ['/api/terminal/scheduled-inputs', '/api/terminal/auto-continue-tasks']:
            await route.fulfill(json={'tasks': []})
        else:
            await route.fulfill(json={})
    async def websocket(ws):
        ws.send(b'group fixture ready\r\n')
    await page.route('**/*', http)
    await page.route_web_socket('**/api/terminal/ws?**', websocket)
    await page.goto('http://webclx.test/terminal?path=webClx&session=s1', wait_until='domcontentloaded')
    await page.wait_for_function("typeof state !== 'undefined' && state.activeSessionId === 's1' && state.sessions.length === 3")
    async def open_dialog():
        await page.locator('#terminal-tools-button').click()
        await page.locator('#terminal-tools-menu #terminal-group-sessions').click()
        await page.locator('#terminal-group-dialog').wait_for(state='visible')
        box = await page.locator('#terminal-group-dialog').bounding_box()
        assert box['x'] >= 0 and box['x'] + box['width'] <= width + 1, box
        assert box['y'] >= 0 and box['y'] + box['height'] <= height + 1, box
        help_btn = page.locator('#terminal-group-dialog .card-desc')
        help_text = page.locator('#terminal-group-dialog .card-desc-text')
        assert await help_btn.is_visible()
        assert not await help_text.is_visible()
        await help_btn.click()
        assert await help_text.is_visible()
        assert '勾选成员后为每个终端指定角色' in await help_text.text_content()
        await help_btn.click()
        assert not await help_text.is_visible()
        name_box = await page.locator('#terminal-group-name').bounding_box()
        color_box = await page.locator('#terminal-group-color').bounding_box()
        assert name_box and color_box
        # 分组名称在左，分组颜色在后（同一行）
        assert name_box['x'] < color_box['x'], (name_box, color_box)
        assert abs(name_box['y'] - color_box['y']) < 20, (name_box, color_box)
    await open_dialog()
    await page.locator('#terminal-group-session-list input[value="s2"]').check()
    # A background selection change must not redirect this dialog's save.
    await page.evaluate("state.activeSessionId = 's3'")
    loop = asyncio.get_event_loop()
    t0 = loop.time()
    await page.locator('#terminal-group-form [type="submit"]').click()
    await page.locator('#terminal-group-dialog').wait_for(state='hidden')
    duration = loop.time() - t0
    # 对话框窗口立即返回（远小于通知投递延迟 1.0s）
    assert duration < 0.7, f"对话框等待了通知发送，耗时 {duration:.2f}s"
    # 等待后台异步通知任务完成，确保消息发送动作被正确提交并执行
    await page.evaluate("() => window.__webclxGroupNotificationPromise")
    assert len(message_requests) > 0, "后台通知发送动作未被触发"
    assert requests[-1] == ('/api/terminal/sessions/s1/group', {
        'member_ids': ['s1', 's2'],
        'group_roles': [
            {'session_id': 's1', 'role': 'manager'},
            {'session_id': 's2', 'role': 'executor'},
        ],
        'fixed_order': False,
        'group_color': '#4f8cff',
        'group_name': '',
    }), requests
    await page.evaluate("state.activeSessionId = 's1'; renderSessions()")
    await open_dialog()
    # Sorting tabs explicitly select terminal order, ascending, or descending.
    terminal_order_tab = page.locator('.terminal-group-sort-tab[data-sort-mode=""]')
    name_asc_tab = page.locator('.terminal-group-sort-tab[data-sort-mode="name-asc"]')
    name_desc_tab = page.locator('.terminal-group-sort-tab[data-sort-mode="name-desc"]')

    async def assert_sort_tabs(active_mode):
        states = await page.evaluate("""
            [...document.querySelectorAll('.terminal-group-sort-tab')].map((tab) => ({
                mode: tab.dataset.sortMode,
                active: tab.classList.contains('is-active'),
                selected: tab.getAttribute('aria-selected'),
            }))
        """)
        assert states == [
            {'mode': '', 'active': active_mode == '', 'selected': 'true' if active_mode == '' else 'false'},
            {'mode': 'name-asc', 'active': active_mode == 'name-asc', 'selected': 'true' if active_mode == 'name-asc' else 'false'},
            {'mode': 'name-desc', 'active': active_mode == 'name-desc', 'selected': 'true' if active_mode == 'name-desc' else 'false'},
        ], states

    await assert_sort_tabs('')
    assert await page.evaluate("[...document.querySelectorAll('#terminal-group-session-list .terminal-group-session-name')].map(e => e.textContent.split('（')[0])") == ['分组测试终端 1', '分组测试终端 2', '分组测试终端 3']
    await name_asc_tab.click()
    await assert_sort_tabs('name-asc')
    assert await page.evaluate("[...document.querySelectorAll('#terminal-group-session-list .terminal-group-session-name')].map(e => e.textContent.split('（')[0])") == ['分组测试终端 1', '分组测试终端 2', '分组测试终端 3']
    await name_desc_tab.click()
    await assert_sort_tabs('name-desc')
    assert await page.evaluate("[...document.querySelectorAll('#terminal-group-session-list .terminal-group-session-name')].map(e => e.textContent.split('（')[0])") == ['分组测试终端 3', '分组测试终端 2', '分组测试终端 1']
    await terminal_order_tab.click()
    await assert_sort_tabs('')
    assert await page.evaluate("[...document.querySelectorAll('#terminal-group-session-list .terminal-group-session-name')].map(e => e.textContent.split('（')[0])") == ['分组测试终端 1', '分组测试终端 2', '分组测试终端 3']

    # Keep the manually moved order when a background list refresh reopens/re-renders the dialog.
    await page.locator('#terminal-group-session-list input[value="s2"]').uncheck()
    await page.locator('#terminal-group-session-list input[value="s3"]').check()
    await page.locator('#terminal-group-move-up').click()
    assert await page.evaluate("[...document.querySelectorAll('#terminal-group-session-list input')].map(e => e.value)") == ['s1', 's3', 's2']
    await name_asc_tab.click()
    await name_desc_tab.click()
    await terminal_order_tab.click()
    await page.evaluate("state.sessions.reverse(); openTerminalGroupDialog()")
    assert await page.evaluate("[...document.querySelectorAll('#terminal-group-session-list input')].map(e => e.value)") == ['s1', 's3', 's2']
    print('reopen checked', await page.evaluate("[...document.querySelectorAll('#terminal-group-session-list input')].map(e => e.checked)"))
    await page.evaluate("state.sessions.reverse(); openTerminalGroupDialog()")

    # Manual ordering controls exist and disable the current terminal as target.
    print(await page.evaluate("() => ({open: document.querySelector('#terminal-group-dialog').open, hidden: document.querySelector('#terminal-tools-menu').hidden, active: document.activeElement?.id || document.activeElement?.tagName, policy: window.terminalActionKeyboardPolicyActive})"))
    assert await page.locator('#terminal-group-session-list input[value="s2"]').is_checked()
    await page.locator('#terminal-group-session-list input[value="s2"]').uncheck()
    fail_save = True
    await page.locator('#terminal-group-form [type="submit"]').click()
    await page.wait_for_function("document.querySelector('#terminal-group-dialog-status').dataset.tone === 'warn'")
    assert await page.locator('#terminal-group-dialog').is_visible()
    fail_save = False
    await page.locator('#terminal-group-form [type="submit"]').click()
    await page.locator('#terminal-group-dialog').wait_for(state='hidden')
    assert requests[-1] == ('/api/terminal/sessions/s1/group', {
        'member_ids': ['s1'],
        'group_roles': [{'session_id': 's1', 'role': 'manager'}],
        'fixed_order': False,
        'group_color': '#4f8cff',
        'group_name': '',
    })
    assert all(not s['group_token'] for s in sessions)

    # Manager designation: each row carries a role select, the checked members
    # always keep exactly one manager, and rename stays on one row.
    await open_dialog()
    await page.locator('#terminal-group-session-list input[value="s2"]').check()
    await page.locator('#terminal-group-session-list input[value="s3"]').check()
    role_of = "sid => document.querySelector(`#terminal-group-session-list select[data-role-for='${sid}']`).value"
    assert await page.evaluate(f"({role_of})('s1')") == 'manager'
    assert await page.evaluate(f"({role_of})('s2')") == 'executor'
    await page.select_option("#terminal-group-session-list select[data-role-for='s2']", 'manager')
    assert await page.evaluate(f"({role_of})('s1')") == 'executor'
    assert await page.evaluate(f"({role_of})('s2')") == 'manager'
    await page.select_option("#terminal-group-session-list select[data-role-for='s3']", 'manager')
    assert await page.evaluate(f"({role_of})('s2')") == 'executor'
    await page.select_option("#terminal-group-session-list select[data-role-for='s3']", 'executor')
    # Nobody designated anymore: the current terminal is promoted back.
    assert await page.evaluate(f"({role_of})('s1')") == 'manager'
    await page.locator('#terminal-group-form [type="submit"]').click()
    await page.locator('#terminal-group-dialog').wait_for(state='hidden')
    assert len(requests) == 4
    path, payload = requests[-1]
    # 行顺序跟随终端列表的既有排序，这里按集合和角色映射校验保存内容。
    assert path == '/api/terminal/sessions/s1/group'
    assert payload['member_ids'].sort() == ['s1', 's2', 's3'].sort()
    assert sorted(payload['group_roles'], key=lambda item: item['session_id']) == [
        {'session_id': member, 'role': role}
        for member, role in sorted({
            's1': 'manager', 's2': 'executor', 's3': 'executor',
        }.items())
    ]
    assert payload['fixed_order'] is False and payload['group_color'] == '#4f8cff' and payload['group_name'] == ''

    # Reopening restores persisted roles (backend stores normalized Chinese labels).
    for session in sessions:
        session['group_token'] = 'test-group'
        session['group_role'] = '管理者' if session['id'] == 's1' else '执行者'
    # 直接把与后端持久化一致的状态写进页面，避免依赖后台轮询的时序。
    await page.evaluate("""
        (() => {
            for (const s of state.sessions) {
                s.group_token = 'test-group';
                s.group_role = s.id === 's1' ? '管理者' : '执行者';
            }
            renderSessions();
        })()
    """)
    # 激活轮转（编组跟随终端）只移动组内显示顺序；下拉皇冠必须钉在
    # 指定管理者上，不能跟随“被激活成当前终端”的成员漂移。
    await page.evaluate("""
        (() => {
            state.sessions.find((item) => item.id === 's2').group_order = 1;
            state.sessions.find((item) => item.id === 's1').group_order = 2;
            state.activeSessionId = 's2';
            renderSessions();
        })()
    """)
    switcher_role = "sid => document.querySelector(`#session-switcher option[value='${sid}']`).dataset.groupRole"
    assert await page.evaluate(f"({switcher_role})('s1')") == 'manager'
    assert await page.evaluate(f"({switcher_role})('s2')") == 'executor'
    await open_dialog()
    assert await page.evaluate(f"({role_of})('s1')") == 'manager'
    assert await page.evaluate(f"({role_of})('s2')") == 'executor'
    # Unchecking the manager row keeps its own group role; promoting s2 must not
    # touch the unchecked row (it belongs to the same persisted group here, but
    # it is no longer part of this edit).
    await page.select_option("#terminal-group-session-list select[data-role-for='s3']", 'manager')
    await page.locator('#terminal-group-session-list input[value="s3"]').uncheck()
    assert await page.evaluate(f"({role_of})('s3')") == 'manager'
    # 取消勾选管理者行后，皇冠自动提升到当前终端 s2；s1 保持执行者，
    # 未勾选的 s3 保留自己行内的角色值但不会进入保存载荷。
    assert await page.evaluate(f"({role_of})('s2')") == 'manager'
    assert await page.evaluate(f"({role_of})('s1')") == 'executor'
    assert await page.evaluate(f"({role_of})('s3')") == 'manager'
    # Per-row rename keeps the group dialog open with unsaved role edits intact.
    await page.evaluate(
        "document.querySelector(`#terminal-group-session-list select[data-role-for='s1']`)"
        ".closest('label').querySelector('.terminal-group-session-rename').click()",
    )
    await page.locator('#terminal-group-manager-dialog').wait_for(state='visible')
    assert await page.locator('#terminal-group-manager-rename-input').input_value() == '分组测试终端 1'
    # 无头浏览器空闲时不出帧，焦点门控在 showModal 里设置的瞬时 inert 依赖
    # requestAnimationFrame 移除；主动泵一帧后再点击输入框。
    await page.evaluate("new Promise(resolve => requestAnimationFrame(() => resolve()))")
    # 先真实点击输入框（pointerdown 会解除按钮点击后的聚焦阻止），再填写新名称。
    input_box = await page.locator('#terminal-group-manager-rename-input').bounding_box()
    await page.mouse.click(input_box['x'] + input_box['width'] / 2, input_box['y'] + input_box['height'] / 2)
    await page.locator('#terminal-group-manager-rename-input').fill('分组测试终端 壹')
    await page.locator('#terminal-group-manager-form [type="submit"]').click()
    await page.locator('#terminal-group-manager-dialog').wait_for(state='hidden')
    assert await page.locator('#terminal-group-dialog').is_visible()
    assert await page.evaluate(f"({role_of})('s2')") == 'manager'
    row_name = await page.evaluate(
        "document.querySelector(`#terminal-group-session-list select[data-role-for='s1']`)"
        ".closest('label').querySelector('.terminal-group-session-name').textContent.split('（')[0]",
    )
    assert row_name == '分组测试终端 壹'
    # Rename used the rename API; no extra group save left the page.
    assert len(requests) == 4
    await page.locator('#terminal-group-cancel').click()
    await page.locator('#terminal-group-dialog').wait_for(state='hidden')
    for session in sessions:
        session.pop('group_role', None)

    assert not errors, errors
    print(f'{width}x{height}: menu, geometry, frozen session, save, failure/retry, unlink, roles passed')
    await page.close()

async def main():
    async with async_playwright() as playwright:
        browser = await playwright.chromium.launch(executable_path=CHROMIUM, headless=True, args=['--no-sandbox'])
        try:
            for width, height in [(1440, 1000), (375, 812)]:
                await check(browser, width, height)
        finally:
            await browser.close()

asyncio.run(main())
