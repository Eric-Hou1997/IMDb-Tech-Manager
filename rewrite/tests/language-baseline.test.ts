import {fileURLToPath} from 'node:url';
import test,{after} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {baseParse} from '@vue/compiler-dom';
import {createSSRApp,h} from 'vue';
import {renderToString} from 'vue/server-renderer';
import {createServer} from 'vite';
import vue from '@vitejs/plugin-vue';
const server=await createServer({configFile:false,root:fileURLToPath(new URL('..',import.meta.url)),plugins:[vue()],server:{middlewareMode:true,watch:null,hmr:false,ws:false},optimizeDeps:{noDiscovery:true,entries:[]},appType:'custom'});
after(()=>server.close());
const language=await server.ssrLoadModule('/src/baseline-language.ts');
import type {Locale,LanguageSnapshot} from '../src/contracts';
const {Languages,languageData,languageKey,message,stableMessageID,translateDocument}=language;
const source=await readFile(new URL('../../macos/web/index.html',import.meta.url),'utf8');
test('platform-only update wording has all eight presentations and no external translations embedded in the frontend',async()=>{
 const presentation=JSON.parse(await readFile(new URL('../src/assets/platform-presentation.json',import.meta.url),'utf8'));
 const packs=JSON.parse(await readFile(new URL('../language-packs/platform-messages.json',import.meta.url),'utf8'));
 assert.deepEqual(Object.keys(presentation),Object.keys(packs));
 for(const [chinese,row] of Object.entries(packs) as [string,{english:string;traditional:string;translations:string[]}][]){
  assert.deepEqual(presentation[chinese],{english:row.english,traditional:row.traditional});
  assert.equal(message(chinese,'zh-CN'),chinese);assert.equal(message(chinese,'zh-Hant'),row.traditional);assert.equal(message(chinese,'en-US'),row.english);
  for(const [index,locale] of ['fr-FR','ru-RU','ja-JP','es-ES','th-TH'].entries())assert.equal(message(chinese,locale,{[stableMessageID(row.english)]:row.translations[index]}),row.translations[index]);
 }
});
const snapshot=(locale:Locale='zh-CN'):LanguageSnapshot=>({locale,options:languageData.options.map(o=>({...o,error:null})),web_messages:{},native_messages:{}});
function normalize(html:string):unknown {
 function walk(node:any):unknown{if(node.type===3)return null;if(node.type===2)return node.content.trim()||null;if(node.type===0)return node.children.map(walk).filter(Boolean);return [node.tag,Object.fromEntries(node.props.filter((p:any)=>p.type===6).map((p:any)=>[p.name,p.name==='class'?(p.value?.content||'').split(/\s+/).filter(Boolean).sort().join(' '):p.value?.content||'']).sort((a:any,b:any)=>a[0].localeCompare(b[0]))),node.children.map(walk).filter(Boolean)];}return walk(baseParse(html));
}
function original(locale:string,options:any[],downloading:string){
 const nodes=new Map<string,any>();const $=(id:string)=>{if(!nodes.has(id))nodes.set(id,{innerHTML:'',textContent:''});return nodes.get(id);};
 const c:any={q:$,uiLanguage:locale,esc:(v:string)=>v.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;'),localizedText:(s:string)=>message(s,locale)};vm.createContext(c);
 const start=source.indexOf('const LANGUAGE_NAMES='),end=source.indexOf('async function loadLanguageWebMessages',start);
 vm.runInContext(source.slice(start,end)+`;languageDownloadInFlight=${JSON.stringify(downloading)};`,c);c.renderLanguageOptions(options);return $;
}
test('all eight language rows, flags, native names and four action states match original rendered markup',async()=>{
 const Picker=(await server.ssrLoadModule('/src/LanguagePicker.vue')).default;
 for(const locale of ['zh-CN','zh-Hant','en-US','fr-FR','ru-RU','ja-JP','es-ES','th-TH'] as Locale[])for(const state of ['not-installed','installed','downloading','failed']){
  const model=new Languages(async()=>assert.fail('SSR cannot call native commands'),()=>{});model.snapshot=snapshot(locale);for(const option of model.snapshot.options.filter(o=>!o.built_in)){option.state=state;option.installed=state==='installed';}const old=original(locale,model.snapshot.options,'');
  const html=await renderToString(createSSRApp({setup(){return ()=>h(Picker);}}).provide(languageKey,model));
  const ast:any=baseParse(html);const menu=ast.children.find((n:any)=>n.tag==='div'&&n.props.some((p:any)=>p.name==='id'&&p.value?.content==='languagePicker')).children.find((n:any)=>n.tag==='div');
  assert.deepEqual(normalize(menu.loc.source),normalize('<div class="languageMenu" id="languageMenu" role="listbox" aria-label="选择语言">'+old('#languageMenu').innerHTML+'</div>'));
  assert.ok(html.includes(old('#languageCurrentFlag').innerHTML));assert.ok(html.includes(old('#languageCurrentName').textContent));
 }
});
test('all baseline literals, dynamic UI messages and original external IDs match original localization',async()=>{
 const c:any={TextEncoder,DEFAULT_UI_LANGUAGE:'zh-CN'};vm.createContext(c);
 const a=source.indexOf('const ENGLISH_UI='),b=source.indexOf('const HAN_TEXT=');
 vm.runInContext(source.slice(a,b)+source.split('\n').find(l=>l.startsWith('function localizedSystemText('))+`;globalThis.translate=(text,locale,external)=>{uiLanguage=locale;if(!['zh-CN','zh-Hant','en-US'].includes(locale))UI_LOCALES[locale].messages=external;return localizedText(text)};`,c);
 for(const locale of ['zh-CN','zh-Hant','en-US','fr-FR','ru-RU','ja-JP','es-ES','th-TH']){
  const external:Record<string,string>={};if(!['zh-CN','zh-Hant','en-US'].includes(locale)){const data=JSON.parse(await readFile(new URL(`../../language-packs/${locale}/r1/translations.json`,import.meta.url),'utf8'));for(const [key,value] of Object.entries(data.web))external[stableMessageID(key)]=value as string;}
  for(const text of [...Object.keys(languageData.english),'  设置  ','显示 8 / 19','第 12 季','23 条 · 会生成 Tag','已选 27','采纳并写入 2 个 NFO 的规则试写结果？\n写入前会再次校验 NFO 是否发生变化。','unmapped',...Object.values(languageData.names[locale])])assert.equal(message(text,locale,external),c.translate(text,locale,external));
 }
});
test('failed downloads keep the prior locale, clear the spinner and read back persisted state',async()=>{
 const calls:any[]=[];const messages:string[]=[];const model=new Languages(async<T>(name,args)=>{calls.push([name,args]);if(name==='choose_language')throw {message:'语言包摘要验证失败'};const value=snapshot();value.options[3].state='failed';return value as T;},message=>messages.push(message));
 assert.equal(await model.choose('fr-FR'),false);assert.equal(model.snapshot.locale,'zh-CN');assert.equal(model.snapshot.options[3].state,'failed');assert.equal(model.downloading,'');assert.equal(model.busy,false);assert.deepEqual(calls.map(c=>c[0]),['choose_language','language_status']);assert.deepEqual(messages,['语言包摘要验证失败']);
});
test('startup restoration is coalesced, allows language selection and reloads the current state',async()=>{
 let complete!:(s:LanguageSnapshot)=>void;let current=snapshot();const calls:string[]=[];
 const model=new Languages(async<T>(name)=>{calls.push(name);if(name==='restore_language_packs')return await new Promise<LanguageSnapshot>(r=>complete=r) as T;if(name==='choose_language'){current=snapshot('en-US');return current as T;}return current as T;},()=>{});
 const restoring=model.restore();await model.restore();assert.equal(model.busy,false);assert.equal(await model.choose('en-US'),true);complete(snapshot('fr-FR'));await restoring;assert.equal(model.snapshot.locale,'en-US');assert.deepEqual(calls,['restore_language_packs','choose_language','language_status']);
 const late=model.restore();model.dispose();complete(snapshot('fr-FR'));await late;assert.equal(calls.at(-1),'restore_language_packs');assert.equal(model.snapshot.locale,'en-US');
});
test('duplicate choices and stale status responses cannot undo a successful language change',async()=>{
 let read!:(s:LanguageSnapshot)=>void,choose!:(s:LanguageSnapshot)=>void;let calls=0;const model=new Languages(async<T>(name)=>{calls++;return await new Promise<LanguageSnapshot>(r=>{if(name==='language_status')read=r;else choose=r;}) as T;},()=>{});
 const pending=model.read(),changed=model.choose('en-US');assert.equal(await model.choose('zh-Hant'),false);choose(snapshot('en-US'));assert.equal(await changed,true);read(snapshot());await pending;assert.equal(model.snapshot.locale,'en-US');assert.equal(calls,2);
 const next=model.choose('fr-FR'),during=model.read();choose(snapshot('fr-FR'));assert.equal(await next,true);read(snapshot('en-US'));await during;assert.equal(model.snapshot.locale,'fr-FR');
 const late=model.read();model.dispose();read(snapshot('zh-Hant'));await late;assert.equal(model.snapshot.locale,'fr-FR');
 const before=calls;await model.read();assert.equal(await model.choose('zh-CN'),false);assert.equal(calls,before);
});
test('translation round trips preserve inputs, user content and new Vue text while releasing the observer',()=>{
 class Text {nodeType=3;parentElement:El|null=null;data:string;constructor(data:string){this.data=data;}}
 class El {nodeType=1;parentElement:El|null=null;childNodes:(El|Text)[]=[];attrs:Record<string,string>={};value='';scrollTop=0;skip=false;tagName:string;constructor(tagName:string){this.tagName=tagName;}append(...nodes:(El|Text)[]){for(const node of nodes){node.parentElement=this;this.childNodes.push(node);}}closest(selector:string):El|null{return this.skip&&selector!=='[data-i18n-user-attributes]'?this:this.parentElement?.closest(selector)||null;}hasAttribute(k:string){return k in this.attrs;}getAttribute(k:string){return this.attrs[k]??null;}setAttribute(k:string,v:string){this.attrs[k]=v;}}
 const previous=globalThis.MutationObserver;let callback:MutationCallback=()=>{};let disconnected=false;
 globalThis.MutationObserver=class {constructor(fn:MutationCallback){callback=fn;}observe(){}takeRecords(){return [];}disconnect(){disconnected=true;}} as any;
 try{const body=new El('BODY'),button=new El('BUTTON'),text=new Text('设置'),input=new El('INPUT'),user=new El('CODE'),protectedText=new Text('电影 设置 / data.nfo');user.skip=true;user.append(protectedText);button.attrs.title='打开设置';button.append(text);input.value='Casino Royale';input.scrollTop=73;body.append(button,input,user);
  const doc={body,documentElement:{lang:''},defaultView:{dispatchEvent(){}}};const model=new Languages(async()=>assert.fail(),()=>{});const adapter=translateDocument(doc as any,model);adapter.refresh();
  for(const locale of ['en-US','zh-Hant','zh-CN'] as Locale[]){model.snapshot=snapshot(locale);adapter.refresh();assert.equal(text.data,message('设置',locale));assert.equal(button.attrs.title,message('打开设置',locale));assert.equal(protectedText.data,'电影 设置 / data.nfo');assert.equal(input.value,'Casino Royale');assert.equal(input.scrollTop,73);}
  model.snapshot=snapshot('en-US');adapter.refresh();text.data='关闭';callback([{type:'characterData',target:text}] as any,{} as any);assert.equal(text.data,'Close');
  model.snapshot=snapshot('zh-CN');text.data='Vue 新状态';adapter.refresh();assert.equal(text.data,'Vue 新状态');adapter.dispose();assert.ok(disconnected);
 }finally{globalThis.MutationObserver=previous;}
});


test('changing only the UI locale preserves unsaved root edits while actual root changes remain conflicts',async()=>{
 const {reactive,nextTick,createRenderer,ssrContextKey}=await import('vue');
 const Root=(await server.ssrLoadModule('/src/RootSettings.vue')).default;
 const props=reactive({configuration:{revision:0,locale:'zh-CN',roots:[{id:'original',space:'movie',path:'/media/电影 设置'}]}});
 let exposed:any;
 let state:any;
 const renderer=createRenderer({patchProp(){},insert(){},remove(){},createElement:()=>({}),createText:()=>({}),createComment:()=>({}),setText(){},setElementText(){},parentNode:()=>null,nextSibling:()=>null});
 const app=renderer.createApp({setup(){state=Root.setup(props,{expose:(value:any)=>exposed=value,emit:()=>{}});return ()=>null;}});
 app.provide(ssrContextKey,{modules:new Set()});app.mount({});
 state.paths.value.movie='/media/additional';state.add('movie');await nextTick();
 assert.equal(exposed.isDirty.value,true);
 props.configuration={...props.configuration,revision:1,locale:'en-US'};await nextTick();
 assert.equal(exposed.conflict.value,false);assert.equal(state.draft.value.locale,'en-US');assert.equal(state.draft.value.revision,1);
 assert.deepEqual(state.draft.value.roots.map((r:any)=>r.path),['/media/电影 设置','/media/additional']);
 props.configuration={...props.configuration,revision:2,roots:[{id:'new',space:'movie',path:'/changed/root'}]};await nextTick();
 assert.equal(exposed.conflict.value,true);assert.equal(state.draft.value.roots[0].path,'/media/电影 设置');app.unmount();
});
test('About links follow all eight existing legal-document paths',async()=>{
 const About=(await server.ssrLoadModule('/src/AboutPanel.vue')).default;
 for(const locale of languageData.options.map((o:any)=>o.code)){
  const model=new Languages(async()=>assert.fail('SSR cannot invoke native APIs'),()=>{});model.snapshot=snapshot(locale);
  const html=await renderToString(createSSRApp({render:()=>h(About)}).provide(languageKey,model));
  const suffix=({'zh-Hant':'zh-Hant','en-US':'en','fr-FR':'fr','ru-RU':'ru','ja-JP':'ja','es-ES':'es','th-TH':'th'} as Record<string,string>)[locale];
  for(const name of ['PRIVACY','TERMS']){const path=suffix?`docs/legal/${name}.${suffix}.md`:`${name}.md`;await readFile(new URL('../../'+path,import.meta.url));assert.ok(html.includes('/blob/main/'+path));}
 }
});

test('native confirmation labels use the original launcher dictionary, independently of user input',async()=>{
 const {bindPresentation,nativeText}=language;
 const model=new Languages(async()=>assert.fail(),()=>{});const release=bindPresentation(model);
 try {
  model.snapshot=snapshot('zh-Hant');assert.equal(nativeText('退出 IMDb Tech Manager','Quit IMDb Tech Manager'),'結束 IMDb Tech Manager');assert.equal(nativeText('确认','Confirm'),'確認');
  for(const locale of ['fr-FR','ru-RU','ja-JP','es-ES','th-TH'] as Locale[]){
   const source=JSON.parse(await readFile(new URL(`../../language-packs/${locale}/r1/translations.json`,import.meta.url),'utf8'));
   model.snapshot=snapshot(locale);model.snapshot.native_messages=Object.fromEntries(Object.entries(source.native).map(([key,text])=>[stableMessageID(key),text]));
   for(const [zh,en] of [['确认','Confirm'],['取消','Cancel'],['关于 IMDb Tech Manager','About IMDb Tech Manager']])assert.equal(nativeText(zh,en),source.native[en]);
  }
 }finally{release();}
});
