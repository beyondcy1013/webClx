import json
from pathlib import Path
from urllib.parse import urlparse
from playwright.sync_api import sync_playwright
# Uses source HTML with the running service's assets and read-only APIs; no browser login assertion.
root=Path(__file__).resolve().parents[1]
token=Path('/home/bin/webclx/.webclx-local-api-token').read_text().strip()
with sync_playwright() as p:
 browser=p.chromium.launch(executable_path='/opt/google/chrome/chrome',headless=True,args=['--no-sandbox'])
 try:
  for width in [320,390,768,1366]:
   page=browser.new_page(viewport={'width':width,'height':900},is_mobile=width<640,has_touch=width<640)
   # Inspect official auth settings navigation without sending credentials.
   page.context.route('https://codearts.huaweicloud.com/portal/settings/cli-auth', lambda r: r.fulfill(content_type='text/html',body='<title>Native login navigation fixture</title>'))
   errors=[]
   page.on('pageerror',lambda e: errors.append(str(e)))
   def route(r):
    u=urlparse(r.request.url)
    if u.hostname!='127.0.0.1': r.abort(); return
    if u.path=='/llm_api': r.fulfill(content_type='text/html',body=(root/'static/index.html').read_text()); return
    # Keep the layout regression read-only; real native writes are verified separately.
    if u.path=='/api/auth/codearts-options':
     r.fulfill(content_type='application/json',body=json.dumps({'initialized':True,'allowAllPermissions':True,'applied':True})); return
    if u.path.startswith('/api/'):
     r.continue_(headers={**r.request.headers,'X-WebClx-Local-Token':token}); return
    r.continue_()
   page.route('**/*',route)
   page.goto('http://127.0.0.1:11111/llm_api',wait_until='networkidle')
   assert 'strict is not defined' not in page.locator('#codex-account-status').inner_text()
   api_requests=[]
   page.on('request',lambda r: api_requests.append(r.url) if '/api/' in r.url else None)
   usage=page.locator('#account-usage-window')
   assert usage.count()==1 and usage.input_value()=='48h',(width,'default usage window')
   for kind in ['oauth','api','claude','codebuddy','devin','agy','opencode','codearts','qoder']:
    page.locator(f'.account-type-tab[data-kind="{kind}"]').click()
    page.wait_for_timeout(100)
    tools=page.locator('#api-view .subtab-options-bar:not([hidden]) .llm-tab-tools > summary')
    if tools.count():
     tools.click()
     assert page.locator('#api-view .llm-tab-tools[open]').count()==1
     tools.click()
    expected=kind in ['oauth','api','codebuddy','devin','agy']
    assert page.locator('#llm-usage-window-bar').is_visible(),(width,kind,'usage window bar visible on every sub-tab')
    ids=['account-refresh-all-quotas','codebuddy-refresh-credits','agy-quota-all']
    assert sum(page.locator('#'+i).is_visible() for i in ids)==int(expected),(width,kind,'quota visibility')
    if kind=='codearts':
     assert page.locator('#codearts-options-bar').is_visible()
     page.locator('#codearts-options-bar > details > summary').click()
     page.locator('#codearts-options-bar .llm-permission-options > summary').click()
     assert page.locator('#codearts-option-full-access').is_checked()
     assert page.locator('#codearts-option-full-access').is_enabled()
     assert page.evaluate('document.documentElement.scrollWidth')<=width+1,(width,'expanded permissions overflow')
     page.screenshot(path=f'/tmp/webclx-codearts-options-{width}.png',full_page=True)
     page.locator('#codearts-options-bar > details > summary').click()
     login=page.locator('#native-login-codearts-auth')
     assert login.is_visible()
     assert login.inner_text()=='CLI 认证设置（AK/SK）'
     assert login.get_attribute('href')=='https://codearts.huaweicloud.com/portal/settings/cli-auth'
     details=page.locator('#native-login-backup .llm-login-details')
     details.locator('summary').click()
     assert 'CODEARTS_CLI_AK' in details.inner_text()
     assert '备份不包含仅存在于环境变量中的 AK/SK' in details.inner_text()
     details.locator('summary').click()
     with page.expect_popup() as popup:
      login.click()
     popup.value.wait_for_load_state()
     assert popup.value.url=='https://codearts.huaweicloud.com/portal/settings/cli-auth'
     popup.value.close()
     assert page.locator('#native-login-read').is_visible()
     assert page.locator('#native-login-download').is_enabled()
     assert page.locator('#codex-account-detect-current').is_hidden()
     assert page.locator('#codex-account-save-current').is_hidden()
     with page.expect_response(lambda r: '/api/auth/native-login/codearts' in r.url and r.request.method=='GET') as response:
      page.locator('#native-login-read').click()
     native=response.value.json()
     rows=page.locator('#codex-account-list tr.current-row')
     if native.get('available'):
      assert native.get('accounts'),(width,'CodeArts inspection must include account identities')
      page.wait_for_function('(count) => document.querySelectorAll("#codex-account-list tr.current-row").length === count',arg=len(native['accounts']))
      assert rows.count()==len(native['accounts']),(width,'read populates account table')
      assert rows.first.locator('td').count()==17
      assert '本机当前' in rows.first.inner_text()
      assert rows.first.get_by_role('button',name='下载登录备份').is_visible()
      for account in native['accounts']:
       if account['account_id']:
        assert account['account_id'] in page.locator('#codex-account-list').inner_text()
      # Exercise the real POST and download generation without saving credentials to disk.
      page.evaluate('''() => {
       window.nativeBackupDownloads=[];
       const click=HTMLAnchorElement.prototype.click;
       HTMLAnchorElement.prototype.click=function() {
        if (this.download) { window.nativeBackupDownloads.push(this.download); return; }
        return click.call(this);
       };
      }''')
      with page.expect_response(lambda r: '/api/auth/native-login/codearts' in r.url and r.request.method=='POST') as backup:
       rows.first.get_by_role('button',name='下载登录备份').click()
      assert backup.value.status==200,(width,'row backup POST')
      assert backup.value.json()['kind']=='codearts'
      page.wait_for_function('() => window.nativeBackupDownloads.length === 1')
      assert page.evaluate('window.nativeBackupDownloads[0]').startswith('webclx-codearts-login-')
     else:
      page.wait_for_function('() => document.querySelectorAll("#codex-account-list tr.current-row").length === 0')
    else:
     assert page.locator('#codearts-options-bar').is_hidden(),(width,kind,'CodeArts-only permissions')
     assert page.locator('#native-login-codearts-auth').is_hidden(),(width,kind,'CodeArts-only login entry')
    result=page.evaluate('''() => { const t=document.querySelector('.account-type-tabs').getBoundingClientRect(); const a=document.querySelector('#native-login-backup').getBoundingClientRect(); return {left:t.left,right:t.right,height:t.height,actions:a.height,width:innerWidth,scroll:document.documentElement.scrollWidth}; }''')
    assert result['scroll']<=width+1,(width,kind,result)
    assert result['right']<=width+1 and result['left']>=0,(width,kind,result)
    assert result['height']<=100,(width,kind,result)
    assert result['actions']<=190,(width,kind,result)
   page.locator('.account-type-tab[data-kind="oauth"]').click()
   before=len(api_requests)
   usage.select_option('24h')
   page.wait_for_timeout(150)
   assert len(api_requests)==before,(width,'changing usage window must not fetch',before,len(api_requests))
   cell=page.locator('.codex-account-usage-cell').first
   if cell.count():
    assert '最近 24 小时' in (cell.get_attribute('title') or ''),(width,'usage cell title reflects selected window')
   usage.select_option('48h')
   page.locator('.llm-more-actions > summary').click()
   assert page.locator('#codex-account-login-oauth').is_visible()
   page.locator('.llm-more-actions > summary').click()
   page.screenshot(path=f'/tmp/webclx-llm-{width}.png',full_page=True)
   assert not errors,errors
   print(f'PASS {width}px: nine tabs, Qoder & CodeArts backup controls, scoped quota actions, compact navigation, no JS errors')
   page.close()
 finally: browser.close()
