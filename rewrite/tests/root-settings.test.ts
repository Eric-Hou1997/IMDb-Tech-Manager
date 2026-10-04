import {fileURLToPath} from 'node:url';
import test,{after} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {createServer} from 'vite';
import vue from '@vitejs/plugin-vue';
import {createRenderer,createSSRApp,h,nextTick,reactive,ssrContextKey} from 'vue';
import {renderToString} from 'vue/server-renderer';
import {baseParse} from '@vue/compiler-dom';
import type {Configuration} from '../src/contracts';

const server=await createServer({configFile:false,root:fileURLToPath(new URL('..',import.meta.url)),plugins:[{
 name:'root-native-fixture',enforce:'pre',
 resolveId(id){if(id==='@tauri-apps/api/core'||id==='virtual:root-native')return '\0root-native';},
 load(id){if(id==='\0root-native')return 'let handler; export const setHandler=value=>handler=value; export const invoke=(...args)=>handler(...args);';},
 transform(code,id){if(id.endsWith('/RootSettings.vue'))return code.replaceAll('@tauri-apps/api/core','virtual:root-native');},
},vue()],server:{middlewareMode:true,watch:null,hmr:false,ws:false},optimizeDeps:{noDiscovery:true,entries:[]},appType:'custom'});
after(()=>server.close());
const native=await server.ssrLoadModule('virtual:root-native');
const Root=(await server.ssrLoadModule('/src/RootSettings.vue')).default;
const initial=():Configuration=>({revision:0,locale:'zh-CN',roots:[]});
function mount(configuration=initial()){
 const props=reactive({configuration});let state:any;const events:[string,unknown][]=[];
 const renderer=createRenderer({patchProp(){},insert(){},remove(){},createElement:()=>({}),createText:()=>({}),createComment:()=>({}),setText(){},setElementText(){},parentNode:()=>null,nextSibling:()=>null});
 const app=renderer.createApp({setup(){state=Root.setup(props,{expose(){},emit:(name:string,value:unknown)=>events.push([name,value])});return ()=>null;}});
 app.provide(ssrContextKey,{modules:new Set()});app.mount({});
 return {state,props,events,dispose:()=>app.unmount()};
}
test('choosing a folder adds an unsaved classified root; cancellation and duplicates preserve the draft',async()=>{
 const page=mount();const calls:string[]=[];let picked:string|null='/media/电影';
 native.setHandler(async(command:string)=>{calls.push(command);assert.equal(command,'choose_library_root');return picked;});
 try{
  await page.state.choose('movie');await nextTick();
  assert.deepEqual(page.state.draft.value.roots.map((r:any)=>[r.space,r.path]),[['movie','/media/电影']]);
  assert.equal(page.state.paths.value.movie,'');assert.equal(page.props.configuration.roots.length,0);
  await page.state.choose('movie');assert.equal(page.state.draft.value.roots.length,1);assert.equal(page.state.error.value,'');
  picked=null;await page.state.choose('tv');assert.equal(page.state.draft.value.roots.length,1);
  assert.deepEqual(calls,['choose_library_root','choose_library_root','choose_library_root']);
 }finally{page.dispose();}
});
test('access tests retain drafts; a lost save reply retries the same operation before showing the original saved notice',async()=>{
 const page=mount();const calls:any[]=[];let saved:any;
 native.setHandler(async(command:string,args:any)=>{
  calls.push([command,structuredClone(args)]);
  if(command==='test_library_root')throw {code:'unconfirmed-root',message:'只能测试已确认的资料库根目录',path:args.path};
  assert.equal(command,'save_library_roots');
  if(!saved){saved={...structuredClone(args.configuration),revision:1};throw Error('lost reply');}
  return saved;
 });
 try{
  page.state.paths.value.movie='/media/电影';page.state.add('movie');
  await page.state.test(page.state.draft.value.roots[0]);
  assert.match(page.state.error.value,/只能测试已确认/);assert.equal(page.state.draft.value.roots.length,1);
  await page.state.save();assert.match(page.state.error.value,/lost reply/);
  await page.state.save();assert.deepEqual(calls[1],calls[2]);
  assert.equal(page.state.draft.value.revision,1);assert.equal(page.state.dirty.value,false);
  assert.ok(page.events.some(([name,value])=>name==='notice'&&value==='分类资料库已保存'));
 }finally{page.dispose();}
});
test('an explicit save also confirms an empty or unchanged classification',async()=>{
 const page=mount();let calls=0;
 native.setHandler(async(command:string,args:any)=>{calls++;assert.equal(command,'save_library_roots');return {...args.configuration,revision:1};});
 try{await page.state.save();assert.equal(calls,1);assert.deepEqual(page.state.draft.value.roots,[]);assert.equal(page.state.draft.value.revision,1);}finally{page.dispose();}
});
function normalized(html:string):unknown {
 function walk(node:any):unknown{if(node.type===3)return null;if(node.type===2)return node.content.trim()||null;if(node.type===0)return node.children.map(walk).filter(Boolean);return [node.tag,Object.fromEntries(node.props.filter((p:any)=>p.type===6&&!(p.name==='value'&&p.value?.content==='')).map((p:any)=>[p.name,p.value?.content||'']).sort((a:any,b:any)=>a[0].localeCompare(b[0]))),node.children.map(walk).filter(Boolean)];}return walk(baseParse(html,{isVoidTag:tag=>tag==='input'}));
}
test('empty and populated root groups restore original wrappers, access buttons, danger actions and spacing hooks',async()=>{
 const source=await readFile(new URL('../../macos/web/index.html',import.meta.url),'utf8');
 for(const roots of [[],[{id:'m',space:'movie',path:'/media/电影 & 设置'},{id:'t',space:'tv',path:'/media/电视剧'}]] as Configuration['roots'][]){
  const hosts:Record<string,any>={};const context:any={state:{rootsBySpace:{movies:roots.filter(r=>r.space==='movie'),tv:roots.filter(r=>r.space==='tv'),unassigned:[]}},q:(id:string)=>hosts[id]??=( {innerHTML:''}),esc:(s:string)=>s.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;')};
  vm.createContext(context);vm.runInContext(source.split('\n').find(line=>line.startsWith('function renderRootGroups('))!,context);context.renderRootGroups();
  const start=source.indexOf('<div class="rootColumns">');let expected=source.slice(start,source.indexOf('<div id="unassignedRoots"',start)).trim();
  for(const [id,space,label] of [['movieRoots','movies','电影'],['tvRoots','tv','电视剧']])expected=expected.replace(`<div id="${id}"></div>`,`<div id="${id}">${hosts['#'+id].innerHTML}</div>`).replace(`id="${space==='movies'?'movieRootInput':'tvRootInput'}"`,`id="${space==='movies'?'movieRootInput':'tvRootInput'}" aria-label="${label}资料库路径"`);
  const html=await renderToString(createSSRApp({render:()=>h(Root,{configuration:{...initial(),roots}})}));
  const root:any=baseParse(html,{isVoidTag:tag=>tag==='input'}).children.find((n:any)=>n.type===1);const columns=root.children.find((n:any)=>n.type===1&&n.props.some((p:any)=>p.name==='class'&&p.value?.content==='rootColumns'));
  assert.deepEqual(normalized(columns.loc.source),normalized(expected));
 }
});
test('legacy unclassified paths use the original pending directory card and remain user text',async()=>{
 const source=await readFile(new URL('../../macos/web/index.html',import.meta.url),'utf8');
 const pending=[{path:'/旧目录 & 未分类',space:null,enabled:true,state:'unassigned'}];
 const hosts:Record<string,any>={};const context:any={state:{rootsBySpace:{movies:[],tv:[],unassigned:pending}},q:(id:string)=>hosts[id]??={innerHTML:''},esc:(s:string)=>s.replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;')};
 vm.createContext(context);vm.runInContext(source.split('\n').find(line=>line.startsWith('function renderRootGroups('))!,context);context.renderRootGroups();
 const html=await renderToString(createSSRApp({render:()=>h(Root,{configuration:initial(),pendingLegacy:pending})}));
 const root:any=baseParse(html,{isVoidTag:tag=>tag==='input'}).children.find((n:any)=>n.type===1);
 const card=root.children.find((n:any)=>n.type===1&&n.props.some((p:any)=>p.name==='id'&&p.value?.content==='unassignedRoots'));
 assert.deepEqual(normalized(card.children.filter((n:any)=>n.type!==3).map((n:any)=>n.loc.source).join('')),normalized(hosts['#unassignedRoots'].innerHTML));
});
