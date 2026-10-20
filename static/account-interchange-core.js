/* Account interchange contract v1. Pure conversion: no IO, execution or credential logging. */
(function (root) {
  'use strict';
  const SINGLE = 'webclx-account-presets', BUNDLE = 'webclx-account-presets-bundle';
  const SECTIONS = ['auth_presets', 'api_presets', 'claude_presets', 'devin_presets', 'codebuddy_presets'];
  const record = x => x !== null && typeof x === 'object' && !Array.isArray(x);
  const clone = x => JSON.parse(JSON.stringify(x));
  const text = x => typeof x === 'string' ? x : '';
  const error = message => { throw new Error(message); };
  const name = (x, index) => text(x.name) || text(x.email) || `account-${index + 1}`;
  function version(x) { if (x.version !== undefined && x.version !== 1) error('不支持的数据版本，请使用 version=1。'); }
  function tokenOf(p) { return p.auth?.tokens || p.tokens || p.credentials || p; }
  function sectionOf(a) {
    if (a.platform === 'openai') return a.type === 'oauth' ? 'auth_presets' : 'api_presets';
    if (a.platform === 'anthropic') return 'claude_presets';
    return '';
  }
  function nativeFromPreset(section, p, index) {
    const c = tokenOf(p), oauth = section === 'auth_presets';
    if (!['auth_presets','api_presets','claude_presets'].includes(section)) error(`${section} 不能转换为 Sub2API 账号，请使用 webClx 完整预设。`);
    if (section === 'api_presets' && (p.access_mode === 'chatgpt_oauth' || p.access_token)) error(`${name(p,index)} 使用 API 页 OAuth 模式，请使用 webClx 完整预设。`);
    const credentials = oauth ? clone(c) : {
      api_key: p.api_key || p.auth_token || p.key || '', base_url: p.base_url || '',
      wire_api: p.wire_api || (section === 'claude_presets' ? 'messages' : 'responses')
    };
    if (oauth) {
      if (credentials.account_id) credentials.chatgpt_account_id = credentials.account_id;
      if (p.auth?.last_refresh) credentials.last_refresh = p.auth.last_refresh;
      for (const key of ['email','plan_type']) if (p.details?.[key]) credentials[key] = p.details[key];
    }
    if (!(oauth ? credentials.access_token || credentials.refresh_token : credentials.api_key)) error(`${name(p,index)} 缺少凭证。`);
    const mode = section === 'claude_presets' ? p.access_mode : p.responses_proxy;
    if (!oauth && mode && !['direct','anthropic_proxy','anthropic_relay'].includes(mode)) {
      credentials.wire_api = mode === 'anthropic_chat' ? 'messages' : mode === 'openai_responses' ? 'responses' : 'chat';
    }
    return { ...(p.interchange_source || {}), name: name(p,index), platform: section === 'claude_presets' && credentials.wire_api === 'messages' ? 'anthropic' : section === 'claude_presets' && !mode ? 'anthropic' : 'openai',
      type: oauth ? 'oauth' : 'apikey', credentials: {...(p.interchange_source?.credentials || {}),...credentials},
      extra: { ...(p.interchange_source?.extra || {}), webclx: { version: 1, section, preset: clone(p) } }, concurrency: p.interchange_source?.concurrency ?? 1, priority: p.interchange_source?.priority ?? 1 };
  }
  function nativeFromCpa(p, index) {
    const providers = {codex:'openai',claude:'anthropic',gemini:'gemini',antigravity:'antigravity'};
    if (!providers[p.type]) error(`第 ${index+1} 项是不支持的 CPA 供应商。`);
    const credentials = clone(p.type === 'gemini' && record(p.token) ? {...p,...p.token} : p);
    if (!credentials.access_token && !credentials.refresh_token) error(`第 ${index+1} 项缺少 OAuth 凭证。`);
    if (p.account_id) credentials.chatgpt_account_id = p.account_id;
    if (p.expired !== undefined) credentials.expires_at = p.expired;
    return {name:name(p,index),platform:providers[p.type],type:'oauth',credentials,concurrency:10,priority:1};
  }
  function parse(input) {
    const p = typeof input === 'string' ? JSON.parse(input) : clone(input);
    const result = {format:'', entries:[], proxies:[], warnings:[]};
    function web(section, value) {
      if (!SECTIONS.includes(section)) error(`不支持的预设类别：${section}。`);
      if (value.format !== SINGLE || value.section !== section) error('预设类别或格式不一致。');
      version(value);
      if (!Array.isArray(value.accounts)) error('账号列表必须是数组。');
      value.accounts.forEach((preset,index) => {
        if (!record(preset)) error(`第 ${index+1} 个预设无效。`);
        result.entries.push({section,preset,index:result.entries.length});
      });
    }
    if (!record(p) && !Array.isArray(p)) error('数据必须是 JSON 对象或账号数组。');
    if (p.format === BUNDLE) {
      version(p); if (!record(p.sections)) error('缺少 sections。');
      for (const [section,value] of Object.entries(p.sections)) web(section,value);
      result.format = 'webClx';
    } else if (p.format === SINGLE) { web(p.section,p); result.format='webClx'; }
    else if (p.format) error('无法识别的数据格式。');
    else if (['sub2api-data','sub2api-bundle'].includes(p.type) || (Array.isArray(p.accounts) && Array.isArray(p.proxies))) {
      version(p); result.format='Sub2API'; result.proxies=clone(p.proxies || []);
      if (!Array.isArray(p.accounts)) error('缺少 accounts。');
      for (const [index,native] of p.accounts.entries()) {
        if (!record(native) || !record(native.credentials) || !native.platform || typeof native.type !== 'string') error(`第 ${index+1} 个 Sub2API 账号无效或类型不受支持。`);
        result.entries.push({section:sectionOf(native),native,index});
      }
    } else if (record(p['api-keys'])) {
      result.format='CPA 配置';
      for (const [provider,groups] of Object.entries(p['api-keys'])) {
        if (!['codex','claude','openai-compatibility'].includes(provider) || !Array.isArray(groups)) error('CPA 配置包含不支持的供应商，请拆分后导入。');
        for (const group of groups) for (const key of group.keys || []) {
          if (!key['api-key']) error('CPA API Key 配置缺少密钥。');
          const native={name:group.name || provider,platform:provider==='claude'?'anthropic':'openai',type:'apikey',credentials:{api_key:key['api-key'],base_url:group['base-url'] || (provider==='claude'?'https://api.anthropic.com':'https://api.openai.com/v1'),wire_api:provider==='codex'?'responses':provider==='claude'?'messages':'chat'},concurrency:1,priority:group.priority || 0};
          result.entries.push({native,section:sectionOf(native),index:result.entries.length});
        }
      }
      result.warnings.push('仅导入 API Key、地址和协议；CPA 专属策略需在目标端重新配置。');
    } else {
      const accounts=Array.isArray(p)?p:[p]; result.format='CPA';
      for (const [index,value] of accounts.entries()) {
        if (!record(value)) error(`第 ${index+1} 项无效。`);
        const native=value.platform && value.credentials ? value : nativeFromCpa(value,index);
        result.entries.push({native,section:sectionOf(native),index});
      }
    }
    if (!result.entries.length) error('没有可导入的账号。');
    if (result.entries.length>5000) error('单次最多处理 5000 个账号，请分批操作。');
    return result;
  }
  function sub2api(parsed) {
    const accounts=parsed.entries.map((e,i)=> e.native ? clone(e.native) : nativeFromPreset(e.section,e.preset,i));
    return {type:'sub2api-data',version:1,exported_at:new Date().toISOString(),proxies:clone(parsed.proxies),accounts};
  }
  function presetFromNative(a,index) {
    const c=a.credentials, saved=a.extra?.webclx;
    if (saved && saved.version !== 1) error('不支持的 webClx 扩展版本。');
    const section=saved?.section || sectionOf(a);
    if (!SECTIONS.includes(section)) error(`第 ${index+1} 项无法映射至 webClx Agent，请保留在原网关。`);
    if (a.type === 'oauth' && a.platform !== 'openai') error(`第 ${index+1} 项 OAuth 类型不能导入 webClx 的 API Key 预设。`);
    const p=saved?.preset ? clone(saved.preset) : {id:globalThis.crypto?.randomUUID?.() || `import-${Date.now()}-${index}`,name:name(a,index),saved_at:Math.floor(Date.now()/1000)};
    p.name=name(a,index);
    if (!saved) p.interchange_source=clone(a);
    if (section === 'auth_presets') p.auth={...(p.auth||{}),OPENAI_API_KEY:null,last_refresh:c.last_refresh || p.auth?.last_refresh || '1970-01-01T00:00:00Z',tokens:{...(p.auth?.tokens || {}),...c,account_id:c.chatgpt_account_id || c.account_id || ''}};
    else {
      p.base_url=c.base_url || p.base_url || (section==='claude_presets'?'https://api.anthropic.com':'https://api.openai.com/v1');
      if (!saved) p.provider_name=p.provider_name || a.platform;
      if (section==='claude_presets') p.auth_token=c.api_key || c.key || '';
      else {p.api_key=c.api_key || c.key || '';p.wire_api=c.wire_api || p.wire_api || 'responses'; if (!saved) p.responses_proxy=c.wire_api==='chat'?'openai_chat':'direct';}
    }
    if (section === 'auth_presets' && (!p.auth.tokens.access_token || !p.auth.tokens.account_id)) error(`第 ${index+1} 项缺少 webClx 必需的 access_token / account_id。`);
    return {section,preset:p};
  }
  function webclx(parsed, target='') {
    const entries=parsed.entries.map((e,i)=>e.preset?e:presetFromNative(e.native,i));
    if (target && entries.some(e=>e.section!==target)) error('包含其他 Agent 的账号，请使用全局账号导入；本次未导入。');
    const sections={};
    for (const e of entries) {
      if (!sections[e.section]) sections[e.section]={format:SINGLE,version:1,section:e.section,accounts:[]};
      sections[e.section].accounts.push(clone(e.preset));
    }
    return {format:BUNDLE,version:1,sections};
  }
  function cpa(parsed) {
    const native=sub2api(parsed), files=[], issues=[], warnings=[...parsed.warnings];
    native.accounts.forEach((a,index)=> {
      const c=clone(a.credentials), provider={openai:'codex',anthropic:'claude',gemini:'gemini',antigravity:'antigravity'}[a.platform];
      if (!provider) {issues.push(`第 ${index+1} 项供应商不受 CPA 导出支持。`);return;}
      if (!['oauth','apikey'].includes(a.type)) {issues.push(`第 ${index+1} 项鉴权类型不受 CPA 导出支持。`);return;}
      if (a.type==='oauth') {
        const p={...c,type:provider};
        if (provider==='codex') p.account_id=c.chatgpt_account_id || c.account_id || '';
        if (c.expires_at) p.expired=typeof c.expires_at==='number'?new Date(c.expires_at*1000).toISOString():c.expires_at;
        if (provider==='gemini') p.token=clone(c);
        files.push({name:`${provider}-${index+1}.json`,text:JSON.stringify(p,null,2)+'\n'});
      } else {
        const wire=c.wire_api || 'responses';
        const family=a.platform==='anthropic'?'claude':wire==='chat'?'openai-compatibility':'codex';
        const p={'api-keys':{[family]:[{name:name(a,index),'base-url':c.base_url,keys:[{'api-key':c.api_key || c.key}]}]}};
        files.push({name:`api-keys-${index+1}.yaml`,text:JSON.stringify(p,null,2)+'\n'});
      }
    });
    warnings.push('CPA 导出不包含 webClx 终端配置、访问模式及网关调度策略；API Key 文件为可被 YAML 读取的 JSON 配置片段，需合并至 CPA 配置。');
    return {files,issues,warnings};
  }
  function preview(parsed,target='') {
    return parsed.entries.map((e,i)=>({index:i+1,name:name(e.preset || e.native,i),section:e.section || 'unsupported',compatible:!target || e.section===target}));
  }
  // ZIP STORE, deterministic per-entry names, no dependency and no executable content.
  function zip(files) {
    const encoder=new TextEncoder(), parts=[], directory=[];let offset=0;
    function crc(bytes){let c=0xffffffff;for(const b of bytes){c^=b;for(let n=0;n<8;n++)c=(c>>>1)^((c&1)?0xedb88320:0);}return (c^0xffffffff)>>>0;}
    function header(size){const bytes=new Uint8Array(size);return {bytes,view:new DataView(bytes.buffer)};}
    for(const file of files){const n=encoder.encode(file.name),b=encoder.encode(file.text),checksum=crc(b),h=header(30+n.length),d=header(46+n.length);
      h.view.setUint32(0,0x04034b50,true);h.view.setUint16(4,20,true);h.view.setUint16(6,0x800,true);h.view.setUint32(14,checksum,true);h.view.setUint32(18,b.length,true);h.view.setUint32(22,b.length,true);h.view.setUint16(26,n.length,true);h.bytes.set(n,30);
      d.view.setUint32(0,0x02014b50,true);d.view.setUint16(4,20,true);d.view.setUint16(6,20,true);d.view.setUint16(8,0x800,true);d.view.setUint32(16,checksum,true);d.view.setUint32(20,b.length,true);d.view.setUint32(24,b.length,true);d.view.setUint16(28,n.length,true);d.view.setUint32(42,offset,true);d.bytes.set(n,46);parts.push(h.bytes,b);directory.push(d.bytes);offset+=h.bytes.length+b.length;
    }
    const end=header(22),len=directory.reduce((n,b)=>n+b.length,0);end.view.setUint32(0,0x06054b50,true);end.view.setUint16(8,files.length,true);end.view.setUint16(10,files.length,true);end.view.setUint32(12,len,true);end.view.setUint32(16,offset,true);
    return new Blob([...parts,...directory,end.bytes],{type:'application/zip'});
  }
  root.AccountInterchange=Object.freeze({parse,sub2api,webclx,cpa,preview,zip,SECTIONS});
})(globalThis);
