import assert from 'node:assert/strict';
import {test} from 'node:test';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
const code=await readFile(new URL('../static/account-interchange-core.js',import.meta.url),'utf8');
const context=vm.createContext({Blob,TextEncoder,crypto:globalThis.crypto});vm.runInContext(code,context);
const core=context.AccountInterchange;
const plain=value=>JSON.parse(JSON.stringify(value));
const sample={format:'webclx-account-presets',version:1,section:'api_presets',accounts:[{id:'api-1',name:'test',saved_at:1,api_key:'fixture-key',base_url:'https://example.test/v1',wire_api:'responses',responses_proxy:'direct',access_mode:'direct',terminal_env:[{key:'EXAMPLE',value:'inert'}],terminal_startup_script:'echo inert',config_overrides:[{key:'model',value:'fixture-model'}],favorite:true,switch_count:9}]};
test('webClx → Sub2API → webClx preserves client settings and never executes scripts',()=>{
 const parsed=core.parse(sample),native=core.sub2api(parsed),result=core.webclx(core.parse(native));
 assert.deepEqual(plain(result.sections.api_presets.accounts),sample.accounts);
 assert.equal(native.accounts[0].credentials.base_url,'https://example.test/v1');
});
test('all targets reject unknown versions and mismatched nested sections',()=>{
 assert.throws(()=>core.parse({...sample,version:2}));
 assert.throws(()=>core.parse({format:'webclx-account-presets-bundle',version:1,sections:{claude_presets:sample}}));
 assert.throws(()=>core.parse({type:'sub2api-data',version:99,accounts:[],proxies:[]}));
});
test('Agent import rejects mixed bundle before writing',()=>{
 const native=core.sub2api(core.parse(sample));
 native.accounts.push({name:'claude',platform:'anthropic',type:'apikey',credentials:{api_key:'fixture',base_url:'https://example.test'},concurrency:1,priority:1});
 assert.throws(()=>core.webclx(core.parse(native),'api_presets'),/全局/);
 assert.equal(Object.keys(core.webclx(core.parse(native)).sections).length,2);
});
test('CPA credential metadata survives Sub2API conversion',()=>{
 const original={type:'claude',access_token:'fixture-at',refresh_token:'fixture-rt',last_refresh:'2026-01-01T00:00:00Z',account_uuid:'account',organization_uuid:'org',organization_name:'fixture',claude_device_ids:['fixture-device'],custom_metadata:{test:true}};
 const native=core.sub2api(core.parse(original));
 const result=core.cpa(core.parse(native));
 assert.deepEqual(JSON.parse(result.files[0].text),original);
 assert.throws(()=>core.webclx(core.parse(native)),/OAuth/);
});
test('CPA API Key configuration is distinct from OAuth and keeps Chat/Responses identity',()=>{
 const native=core.sub2api(core.parse(sample));
 for(const wire of ['chat','responses']){
  native.accounts[0].credentials.wire_api=wire;
  const result=core.cpa(core.parse(native));
  assert.equal(result.files[0].name,'api-keys-1.yaml');
  const back=core.sub2api(core.parse(result.files[0].text));
  assert.equal(back.accounts[0].credentials.wire_api,wire);
 }
});
test('unsupported sections cannot be silently omitted from compatible exports',()=>{
 const p={...sample,section:'devin_presets'};
 assert.throws(()=>core.sub2api(core.parse(p)),/完整预设/);
 assert.equal(core.webclx(core.parse(p)).sections.devin_presets.accounts.length,1);
});
test('CPA multiple credentials export as real ZIP files, not a synthetic native bundle',async()=>{
 const result=core.cpa(core.parse([{type:'codex',access_token:'fixture',account_id:'a'},{type:'claude',access_token:'fixture'}]));
 assert.equal(result.files.length,2);
 const zip=Buffer.from(await core.zip(result.files).arrayBuffer());
 assert.equal(zip.readUInt32LE(0),0x04034b50);
 assert.equal(zip.readUInt32LE(zip.length-22),0x06054b50);
 assert.equal(zip.readUInt16LE(zip.length-12),2);
});
test('file and clipboard entrypoints parse identical JSON',()=>{
 assert.deepEqual(plain(core.parse(sample)),plain(core.parse(JSON.stringify(sample))));
});

test('native policy and metadata remain bounded across repeated client roundtrips',()=>{
 let native={type:'sub2api-data',version:1,proxies:[],accounts:[{name:'fixture',platform:'openai',type:'apikey',credentials:{api_key:'fixture',base_url:'https://example.test/v1',wire_api:'responses'},concurrency:7,priority:23,extra:{custom:{value:'retained'}}}]};
 for(let n=0;n<8;n++){
  native=plain(core.sub2api(core.parse(core.webclx(core.parse(native)))));
  assert.equal(native.accounts[0].concurrency,7);
  assert.equal(native.accounts[0].priority,23);
  assert.equal(native.accounts[0].extra.custom.value,'retained');
  assert.ok(JSON.stringify(native).length<5000);
 }
});

vm.runInContext(await readFile(new URL('../static/account-interchange-dialog.js',import.meta.url),'utf8'),context);
test('global export aborts on a missing section instead of silently exporting an empty category',async()=>{
 let exports=0;
 await assert.rejects(context.AccountInterchangeDialog.loadGlobalExport(async url=>{
  if(url==='/api/auth/devin-accounts')throw new Error('fixture list unavailable');
  if(url.includes('/export')){exports++;return {};}
  return url.endsWith('-accounts')?[]:{presets:[]};
 }),/fixture list unavailable/);
 assert.equal(exports,0);
 await assert.rejects(context.AccountInterchangeDialog.loadGlobalExport(async()=>({})),/读取失败/);
});
