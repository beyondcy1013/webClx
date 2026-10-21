import assert from 'node:assert/strict';
import {chromium} from '/home/root/.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright/index.mjs';
const browser=await chromium.launch({executablePath:'/opt/google/chrome/chrome',headless:true,args:['--no-sandbox']});
try {
 for(const width of [390,1280]) {
  const page=await browser.newPage({viewport:{width,height:900}});
  await page.setContent('<html><body></body></html>');
  await page.addScriptTag({path:'/home/bin/webclx/static/account-interchange-core.js'});
  await page.addScriptTag({path:'/home/bin/webclx/static/account-interchange-dialog.js'});
  await page.evaluate(()=>{
   globalThis.writes=[];
   globalThis.sample={format:'webclx-account-presets',version:1,section:'api_presets',accounts:[{id:'fixture',name:'fixture',api_key:'fixture-key',base_url:'https://example.test/v1',wire_api:'responses'}]};
   globalThis.manager=AccountInterchangeDialog.create({refresh:async()=>{},requestJson:async(url,options)=>{writes.push({url,payload:JSON.parse(options.body)});return {imported_count:JSON.parse(options.body).accounts.length};},clipboard:{readText:async()=>JSON.stringify(sample),writeText:async()=>{}}});
   return manager.openImport({text:JSON.stringify(sample),target:'api_presets'});
  });
  await page.getByRole('button',{name:'确认导入（1）',exact:true}).click();
  await page.getByText('已从粘贴内容导入',{exact:false}).waitFor();
  assert.equal(await page.evaluate(()=>writes.length),1);
  assert.equal(await page.getByRole('button',{name:'确认导入（1）',exact:true}).isDisabled(),true);
  await page.getByRole('button',{name:'关闭',exact:true}).click();
  await page.evaluate(()=>manager.openImport({target:'api_presets',text:JSON.stringify({format:'webclx-account-presets-bundle',version:1,sections:{api_presets:sample,claude_presets:{...sample,section:'claude_presets'}}})}));
  await page.getByText('包含其他 Agent 的账号，请使用全局账号导入；本次未导入。',{exact:false}).waitFor();
  assert.equal(await page.evaluate(()=>writes.length),1);
  await page.getByRole('button',{name:'关闭',exact:true}).click();
  await page.evaluate(()=>manager.openImport({text:JSON.stringify(sample)}));
  await page.locator('input[type=file]').setInputFiles({name:'large.json',mimeType:'application/json',buffer:Buffer.alloc(8*1024*1024+1,32)});
  await page.getByText('单个文件不能超过 8 MiB。',{exact:true}).waitFor();
  assert.equal(await page.getByRole('button',{name:'确认导入（1）',exact:true}).isDisabled(),true);
  await page.getByRole('button',{name:'关闭',exact:true}).click();
  await page.evaluate(()=>manager.openExport({scope:'all',transport:'file',scopes:[['all','全部账号']],load:async()=>sample}));
  await page.getByRole('button',{name:'下载文件',exact:true}).waitFor();
  assert.equal(await page.getByRole('button',{name:'下载文件',exact:true}).isEnabled(),true);
  await page.getByRole('button',{name:'关闭',exact:true}).click();
  await page.close();
  console.log(`PASS account interchange browser width=${width}; mocked writes only`);
 }
} finally {await browser.close();}
