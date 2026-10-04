import test,{after} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {createServer} from 'vite';
import vue from '@vitejs/plugin-vue';
import {createRenderer,createSSRApp,h,reactive,ssrContextKey} from 'vue';
import {renderToString} from 'vue/server-renderer';
import {baseParse} from '@vue/compiler-dom';
import type {Configuration,OnboardingInfo} from '../src/contracts';
const server=await createServer({configFile:false,root:new URL('..',import.meta.url).pathname,plugins:[{
 name:'onboarding-native-fixture',enforce:'pre',
 resolveId(id){if(id==='virtual:onboarding-native')return '\0onboarding-native';},
 load(id){if(id==='\0onboarding-native')return 'let handler;export const setHandler=value=>handler=value;export const invoke=(...args)=>handler(...args);';},
 transform(code,id){if(id.endsWith('/Onboarding.vue'))return code.replaceAll('@tauri-apps/api/core','virtual:onboarding-native');},
},vue()],server:{middlewareMode:true,watch:null,hmr:false,ws:false},optimizeDeps:{noDiscovery:true,entries:[]},appType:'custom'});
after(()=>server.close());
const native=await server.ssrLoadModule('virtual:onboarding-native');
const Onboarding=(await server.ssrLoadModule('/src/Onboarding.vue')).default;
const initial=():Configuration=>({revision:0,locale:'zh-CN',roots:[]});
const info:OnboardingInfo={onboarding_required:true,library_roots_confirmed:false,candidates:[]};
function mount(){
 const props=reactive({configuration:initial(),info});let state:any;const events:[string,unknown][]=[];
 const renderer=createRenderer({patchProp(){},insert(){},remove(){},createElement:()=>({}),createText:()=>({}),createComment:()=>({}),setText(){},setElementText(){},parentNode:()=>null,nextSibling:()=>null});
 const app=renderer.createApp({setup(){state=Onboarding.setup(props,{expose(){},emit:(name:string,value:unknown)=>events.push([name,value])});return()=>null;}});
 app.provide(ssrContextKey,{modules:new Set()});app.mount({});return {state,props,events,dispose:()=>app.unmount()};
}
test('empty confirmation performs no IPC; candidates and typed paths are only unsaved drafts',async()=>{
 const page=mount();native.setHandler(()=>{throw Error('Empty confirmation must not invoke');});
 try{await page.state.save();assert.deepEqual(page.events,[['notice','至少确认一个电影或电视剧目录']]);
  page.state.add('movie',' /电影 ');page.state.add('movie','/电影');page.state.add('tv','/电视剧');
  assert.equal(page.state.roots.value.length,2);assert.equal(page.props.configuration.roots.length,0);
  page.state.remove(page.state.roots.value[0].id);assert.equal(page.state.roots.value[0].space,'tv');
 }finally{page.dispose();}
});
test('native folder cancellation retains drafts and a late reply after disposal cannot save or emit',async()=>{
 const page=mount();let resolve:(v:string|null)=>void;
 native.setHandler((command:string,args:any)=>{assert.equal(command,'choose_library_root');assert.equal(args.space,'tv');return new Promise(r=>resolve=r);});
 const first=page.state.choose('tv');resolve!(null);await first;assert.equal(page.state.roots.value.length,0);
 const second=page.state.choose('tv');page.dispose();resolve!('/TV');await second;
 assert.equal(page.state.roots.value.length,0);assert.deepEqual(page.events,[]);
});
test('lost confirmation reply retries the exact operation and only success closes first-run setup',async()=>{
 const page=mount();const calls:any[]=[];let count=0;
 native.setHandler(async(command:string,args:any)=>{calls.push([command,structuredClone(args)]);assert.equal(command,'save_library_roots');if(!count++)throw Error('lost reply');return {...args.configuration,revision:1};});
 try{page.state.add('movie','/Movie');await page.state.save();assert.ok(!page.events.some(([name])=>name==='saved'));
  await page.state.save();assert.deepEqual(calls[0],calls[1]);assert.ok(page.events.some(([name])=>name==='saved'));
  assert.ok(page.events.some(([name,value])=>name==='notice'&&value==='资料库已确认；现在可以手动启动后台模式'));
 }finally{page.dispose();}
});
function normalized(html:string):unknown {
 function walk(n:any):unknown{if(n.type===3)return null;if(n.type===2)return n.content.trim()||null;if(n.type!==1)return null;return {tag:n.tag,props:n.props.filter((p:any)=>p.type===6&&!(n.tag==='input'&&p.name==='value'&&!p.value?.content)).map((p:any)=>[p.name,p.name==='style'?(p.value?.content??'').replace(/;$/,''):p.value?.content??'']).sort((a:any,b:any)=>a[0].localeCompare(b[0])),children:n.children.map(walk).filter(Boolean)};}
 return baseParse(html,{isVoidTag:tag=>tag==='input'}).children.map(walk).filter(Boolean);
}
test('first-run dialog retains original movie/TV groups, candidate rows, texts and explicit skip/save actions',async()=>{
 const source=await readFile(new URL('../../macos/web/index.html',import.meta.url),'utf8');
 for(const candidates of [[],[{path:'/候选 & Movie',suggested_space:'movies',source:'tinyMediaManager',online:true},{path:'/Other',suggested_space:'unassigned',source:'tinyMediaManager',online:false}]]){
  const hosts:Record<string,any>={};const context:any={onboardingRoots:{movies:[],tv:[]},q:(id:string)=>hosts[id]??={innerHTML:'',classList:{add(){}}},esc:(s:string)=>s.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;'),api:async()=>({onboarding_required:true,candidates}),toast:(v:any)=>{throw Error(String(v));}};
  vm.createContext(context);for(const name of ['function renderOnboardingRoots(','async function showOnboarding('])vm.runInContext(source.split('\n').find(line=>line.startsWith(name))!,context);
  await context.showOnboarding();const start=source.indexOf('<div class="modal" id="onboardingModal">');let expected=source.slice(start,source.indexOf('\n',start)).replace('class="modal"','class="modal open"');
  for(const id of ['onboardingMovies','onboardingTV'])expected=expected.replace(`<div id="${id}"></div>`,`<div id="${id}">${hosts['#'+id].innerHTML}</div>`);
  expected=expected.replace('正在读取候选…',hosts['#onboardingCandidates'].innerHTML);
  const ctx:any={};await renderToString(createSSRApp({render:()=>h(Onboarding,{configuration:initial(),info:{...info,candidates}})}),ctx);
  assert.deepEqual(normalized(ctx.teleports.body),normalized(expected));
 }
});
