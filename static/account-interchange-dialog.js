/* Shared account import/export dialog. All inputs use AccountInterchange. */
(function (root) {
  'use strict';
  function create({requestJson,refresh,report=()=>{},documentRef=document,clipboard=navigator.clipboard,loadExport}) {
    const core=root.AccountInterchange, endpoint='/api/settings/preset-config/clipboard';
    let dialog=null, state=null, generation=0;
    const el=(tag,attrs={},content='')=>{const e=documentRef.createElement(tag);Object.assign(e,attrs);if(content)e.textContent=content;return e;};
    function download(blob,name) {const url=URL.createObjectURL(blob),a=el('a',{href:url,download:name});documentRef.body.append(a);a.click();a.remove();setTimeout(()=>URL.revokeObjectURL(url),1000);}
    async function copy(text) {
      try {if(clipboard?.writeText){await clipboard.writeText(text);return true;}} catch {}
      const area=el('textarea',{value:text});area.style.cssText='position:fixed;left:-9999px;top:0';documentRef.body.append(area);area.select();let ok=false;
      try {ok=!!documentRef.execCommand?.('copy');} finally {area.remove();}return ok;
    }
    function close(){if(state?.busy)return;generation++;if(state)state.text.value='';dialog?.close();state=null;}
    function status(message,tone='info'){if(!state)return;state.status.textContent=message;state.status.dataset.tone=tone;state.status.hidden=false;}
    function busy(value){state.busy=value;state.submit.disabled=value || !state.ready;state.cancel.disabled=value;state.controls.forEach(e=>e.disabled=value);}
    function shell(title) {
      if (state?.busy) return false;
      if(dialog){dialog.close();dialog.remove();}
      const epoch=++generation;
      dialog=el('dialog',{className:'auth-import-dialog',id:'account-interchange-dialog'});
      const form=el('form',{className:'auth-import-form'}),heading=el('h2',{},title),content=el('div'),statusEl=el('div',{className:'inline-status'}),actions=el('div',{className:'toolbar dialog-actions'});
      heading.id='account-interchange-title';dialog.setAttribute('aria-labelledby',heading.id);
      const cancel=el('button',{type:'button',className:'button secondary'},'关闭'),submit=el('button',{type:'submit',className:'button primary'},'继续');actions.append(cancel,submit);
      form.append(heading,content,statusEl,actions);dialog.append(form);documentRef.body.append(dialog);
      state={epoch,content,status:statusEl,cancel,submit,controls:[],ready:false,busy:false,text:el('textarea'),result:null};
      cancel.onclick=close;dialog.addEventListener('cancel',e=>{e.preventDefault();close();});form.onsubmit=e=>{e.preventDefault();if(state?.ready && !state.busy)void state.action();};
      dialog.showModal();return true;
    }
    function select(label,options,value){const wrap=el('label',{},label),input=el('select',{className:'input'});for(const [v,t]of options)input.append(el('option',{value:v},t));input.value=value;wrap.append(input);state.content.append(wrap);state.controls.push(input);return input;}
    function showPreview(parsed,target){const rows=core.preview(parsed,target);return `${parsed.format} · ${rows.length} 个账号\n`+rows.map(r=>`${r.index}. ${r.name} → ${r.section}${r.compatible?'':'（请使用全局导入）'}`).join('\n');}
    async function openImport({target='',text='',source='粘贴内容'}={}) {
      if(!shell(target?'当前 Agent · 导入账号':'全局 · 导入账号'))return;
      const current=state;
      const hint=el('p',{className:'field-help'},'粘贴或选择 JSON 文件，自动识别 webClx、Sub2API、CPA JSON（含 JSON 写法的配置片段）。只保存账号，不切换当前配置。');
      const files=el('input',{type:'file',accept:'.json,.yaml,.yml,application/json',multiple:true});
      const read=el('button',{type:'button',className:'button secondary'},'读取剪贴板');
      const area=current.text;area.className='dialog-textarea';area.rows=8;area.spellcheck=false;area.setAttribute('aria-label','账号 JSON');area.value=text;
      const preview=el('pre',{className:'field-help'});preview.style.cssText='white-space:pre-wrap;max-height:16rem;overflow:auto';
      current.content.append(hint,files,read,area,preview);current.controls.push(files,read,area);
      const update=(inputs=[area.value])=>{
        current.ready=false;current.result=null;current.submit.disabled=true;
        try {
          const parsed=inputs.map(input=>core.parse(input));
          const merged={format:[...new Set(parsed.map(p=>p.format))].join(' / '),entries:parsed.flatMap(p=>p.entries),proxies:parsed.flatMap(p=>p.proxies),warnings:parsed.flatMap(p=>p.warnings)};
          preview.textContent=showPreview(merged,target);
          const bundle=core.webclx(merged,target);
          current.result=bundle;current.ready=true;current.submit.disabled=false;
          current.submit.textContent=`确认导入（${merged.entries.length}）`;
          status(merged.warnings.join('\n') || '同 ID 预设会更新；其他账号保留。源网关的调度策略不会自动应用。');
        } catch(error){preview.textContent='';status(error.message,'warn');}
      };
      let readVersion=0;
      area.oninput=()=>{readVersion++;update();};files.onchange=async()=>{
        const revision=++readVersion;current.ready=false;current.result=null;current.submit.disabled=true;
        const selected=Array.from(files.files||[]);if(!selected.length)return;
        if(selected.some(f=>f.size>8*1024*1024)){status('单个文件不能超过 8 MiB。','warn');return;}
        try{const inputs=await Promise.all(selected.map(f=>f.text()));if(state!==current || revision!==readVersion)return;area.value=inputs.length===1?inputs[0]:'';source=`${inputs.length} 个文件`;update(inputs);}catch{status('读取文件失败。','warn');}
      };
      read.onclick=async()=>{
        const revision=++readVersion;current.ready=false;current.result=null;current.submit.disabled=true;
        try{const value=await clipboard.readText();if(state!==current || revision!==readVersion)return;area.value=value;source='剪贴板';update();}catch{if(state===current)status('无法读取剪贴板，请手动粘贴或选择文件。','warn');}
      };
      current.action=async()=>{
        busy(true);const results=[];
        try{
          for(const [section,payload]of Object.entries(current.result.sections)) {
            const result=await requestJson(`${endpoint}/${section}/import`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(payload)});
            if(result.imported_count!==payload.accounts.length)throw new Error('接收数量不一致，请核对本次结果后再操作。');
            results.push(`${section}：${result.imported_count}`);
          }
          await refresh();current.ready=false;status(`已从${source}导入\n${results.join('\n')}`,'ok');report('账号导入完成。','ok');
        }catch(error){current.ready=false;status(`导入未全部完成；已完成：${results.join('、')||'无'}。${error.message} 请核对结果后重新预览。`,'warn');report('导入未全部完成，请查看弹窗结果。','warn');}
        finally{busy(false);}
      };
      if(text)update();else {current.submit.disabled=true;status('粘贴内容或选择文件后预览。');}
      area.focus();
    }
    async function openExport({scope='selected',transport='clipboard',load=loadExport,scopes=[['selected','选中账号']]}={}) {
      if(!shell('导出账号'))return;
      const current=state;
      const scopeInput=select('范围',scopes,scope);
      const format=select('格式',[['webclx','webClx 完整预设 JSON'],['sub2api','Sub2API 账号 JSON'],['cpa','CPA 原生凭证 / API Key 配置']],'webclx');
      const destination=select('导出位置',[['clipboard','复制到剪贴板'],['file','下载文件']],transport);
      const preview=el('pre',{className:'field-help'});preview.style.cssText='white-space:pre-wrap;max-height:16rem;overflow:auto';current.content.append(preview);
      const fallback=current.text;fallback.className='dialog-textarea';fallback.readOnly=true;fallback.hidden=true;fallback.setAttribute('aria-label','导出内容');current.content.append(fallback);
      let change=0;
      async function update(){const token=++change;current.ready=false;current.submit.disabled=true;fallback.value='';fallback.hidden=true;
        try{
          status('正在准备导出预览…');
          const payload=await load(scopeInput.value);if(state!==current || token!==change)return;
          const parsed=core.parse(payload);preview.textContent=showPreview(parsed,'');
          let files,warnings=[];
          if(format.value==='cpa') {const result=core.cpa(parsed);if(result.issues.length)throw new Error(result.issues.join('\n'));files=result.files;warnings=result.warnings;}
          else {const result=format.value==='sub2api'?core.sub2api(parsed):payload;files=[{name:`${format.value}-accounts.json`,text:JSON.stringify(result,null,2)+'\n'}];if(format.value==='sub2api')warnings.push('webClx 专属配置保存在 extra.webclx；其他接收器未必理解或保留此扩展。');}
          if(destination.value==='clipboard' && files.length!==1)throw new Error('CPA 多账号需下载 ZIP，包含独立凭证文件；请选择“下载文件”。');
          current.result=files;current.ready=true;current.submit.disabled=false;current.submit.textContent=destination.value==='file'?'下载文件':'复制到剪贴板';status(warnings.join('\n')||'将保留此范围内的完整 webClx 预设。');
        }catch(error){if(state===current && token===change)status(error.message,'warn');}
      }
      scopeInput.onchange=update;format.onchange=update;destination.onchange=update;
      current.action=async()=>{
        busy(true);
        try{const files=current.result;
          if(destination.value==='file'){if(files.length===1)download(new Blob([files[0].text],{type:'application/json'}),files[0].name);else download(core.zip(files),'cpa-accounts.zip');status(`已下载 ${files.length} 个账号文件。`,'ok');}
          else if(await copy(files[0].text))status('已复制到剪贴板。','ok');
          else{fallback.value=files[0].text;fallback.hidden=false;fallback.focus();fallback.select();status('自动复制失败，可复制下方内容或改为下载文件。','warn');}
        }catch{status('导出失败，请重新操作。','warn');}finally{busy(false);}
      };
      await update();
    }
    return {openImport,openExport};
  }
  async function loadGlobalExport(requestJson) {
    const sources=[
      ['auth_presets','/api/auth/presets',false],
      ['api_presets','/api/auth/api-presets',false],
      ['claude_presets','/api/auth/claude-presets',false],
      ['devin_presets','/api/auth/devin-accounts',true],
      ['codebuddy_presets','/api/auth/codebuddy-accounts',true]
    ];
    const lists=await Promise.all(sources.map(async ([section,url,array])=>{
      const value=await requestJson(url),accounts=array?value:value?.presets;
      if(!Array.isArray(accounts) || accounts.some(a=>typeof a?.id!=='string' || !a.id))throw new Error(`${section} 账号列表读取失败，本次未导出。`);
      return [section,accounts.map(a=>a.id)];
    }));
    const sections={};
    for(const [section,ids] of lists){
      if(!ids.length)continue;
      const value=await requestJson(`/api/settings/preset-config/clipboard/${section}/export`,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({ids})});
      if(value?.accounts?.length!==ids.length)throw new Error(`${section} 导出数量不一致，本次未导出。`);
      sections[section]=value;
    }
    return {format:'webclx-account-presets-bundle',version:1,sections};
  }
  root.AccountInterchangeDialog=Object.freeze({create,loadGlobalExport});
})(globalThis);
