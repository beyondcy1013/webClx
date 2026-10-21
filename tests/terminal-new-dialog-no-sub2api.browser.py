import asyncio,json,os
from pathlib import Path
from playwright.async_api import async_playwright
async def main():
 async with async_playwright() as p:
  browser=await p.chromium.launch(executable_path=os.environ.get('WEBCLX_TEST_CHROMIUM', '/home/codes/third_party/browser-tools/bin/chromium'),headless=True,args=['--no-sandbox'])
  page=await browser.new_page(extra_http_headers={'X-WebClx-Local-Token':Path('/home/bin/webclx/.webclx-local-api-token').read_text().strip()})
  import importlib.util
  spec=importlib.util.spec_from_file_location('viewport', str(Path(__file__).with_name('terminal-shared-viewport.browser.py')))
  helper=importlib.util.module_from_spec(spec);spec.loader.exec_module(helper)
  await page.context.add_cookies([helper.local_session_cookie()])
  await page.goto('http://127.0.0.1:11111/terminal')
  print('page',page.url)
  await page.wait_for_function('typeof openTerminalSpecifiedTaskDialog === "function" && !state.loadingSessions')
  await page.locator('#create-session').click()
  await page.wait_for_selector('#terminal-specified-task-dialog[open]')
  await page.wait_for_function('!document.querySelector("#terminal-specified-task-preset").disabled')
  codex_options=[]
  for agent in ['codex','claude']:
   await page.locator('label:has(input[name="terminal-specified-task-agent"][value="'+agent+'"])').click()
   await page.wait_for_function('(agent) => {const e=document.querySelector("#terminal-specified-task-preset"); return e.dataset.agent===agent && !e.disabled}',arg=agent)
   names=await page.locator('#terminal-specified-task-preset option').all_text_contents()
   matches=[n for n in names if 'sub2api' in n.lower()]
   if agent == 'codex':
    codex_options=names
   else:
    assert not matches, matches
   print(json.dumps({'agent':agent,'total':len(names),'sub2api':matches},ensure_ascii=False))
  assert codex_options and any('[OAuth]' in name for name in codex_options), codex_options
  await page.screenshot(path='/tmp/webclx-new-dialog-verified.png')
  await browser.close()
asyncio.run(main())
