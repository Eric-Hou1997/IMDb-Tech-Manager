import {fileURLToPath} from 'node:url';
import test,{after} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import {createServer} from 'vite';
import vue from '@vitejs/plugin-vue';
import {createRenderer,reactive,nextTick,ssrContextKey} from 'vue';
const server=await createServer({configFile:false,root:fileURLToPath(new URL('..',import.meta.url)),plugins:[{
 name:'update-native-fixture',enforce:'pre',
 resolveId(id){if(id==='virtual:update-native')return '\0update-native';},
 load(id){if(id==='\0update-native')return 'let handler;export const setHandler=value=>handler=value;export const invoke=(...args)=>handler(...args);export const listen=async()=>()=>{};';},
 transform(code,id){if(id.endsWith('/UpdatePanel.vue'))return code.replaceAll('@tauri-apps/api/core','virtual:update-native').replaceAll('@tauri-apps/api/event','virtual:update-native');},
},vue()],server:{middlewareMode:true,watch:null,hmr:false,ws:false},optimizeDeps:{noDiscovery:true,entries:[]},appType:'custom'});
after(()=>server.close());
const native=await server.ssrLoadModule('virtual:update-native');
const Panel=(await server.ssrLoadModule('/src/UpdatePanel.vue')).default;
function mount(active:boolean){
 const props=reactive({active});let state:any;
 const renderer=createRenderer({patchProp(){},insert(){},remove(){},createElement:()=>({}),createText:()=>({}),createComment:()=>({}),setText(){},setElementText(){},parentNode:()=>null,nextSibling:()=>null});
 const app=renderer.createApp({setup(){state=Panel.setup(props,{expose(){}});return()=>null;}});
 app.provide(ssrContextKey,{modules:new Set()});app.mount({});return {props,get state(){return state;},dispose:()=>app.unmount()};
}
const settled=()=>new Promise<void>(resolve=>setImmediate(resolve));
test('each original settings opening checks once; closing and concurrent reopening do not duplicate requests',async()=>{
 const original=await readFile(new URL('../../macos/web/index.html',import.meta.url),'utf8');
 assert.match(original.split('\n').find(s=>s.startsWith('function openSettingsModal('))!,/checkTechUpdate\(\)/);
 const calls:string[]=[];let finish!:(value:any)=>void;
 native.setHandler(async(command:string)=>{calls.push(command);if(command==='update_identity')return {product:'ITM',os:'darwin',arch:'aarch64',channel:'app'};if(command==='update_status')return null;if(command==='update_check')return new Promise(r=>finish=r);assert.fail(command);});
 const page=mount(false);try{
  await settled();assert.equal(calls.filter(c=>c==='update_check').length,0);
  page.props.active=true;await nextTick();assert.equal(calls.filter(c=>c==='update_check').length,1);
  page.props.active=false;await nextTick();page.props.active=true;await nextTick();assert.equal(calls.filter(c=>c==='update_check').length,1);
  finish({operation_id:'check',phase:'up-to-date'});await settled();
  page.props.active=false;await nextTick();assert.equal(calls.filter(c=>c==='update_check').length,1);
  page.props.active=true;await nextTick();assert.equal(calls.filter(c=>c==='update_check').length,2);finish({operation_id:'again',phase:'up-to-date'});await settled();
 }finally{page.dispose();}
});
test('initial visible settings checks after native initialization and keeps structured failure text',async()=>{
 let checks=0;native.setHandler(async(command:string)=>{if(command==='update_identity')return {os:'windows'};if(command==='update_status')return null;assert.equal(command,'update_check');checks++;throw {code:'update-offline',message:'Offline'};});
 const page=mount(true);try{await settled();assert.equal(checks,1);assert.equal(page.state.error.value,'Offline');assert.equal(page.state.busy.value,false);}finally{page.dispose();}
});
test('closing the window before initialization completes never starts an update check',async()=>{
 let complete!:(value:any)=>void;const calls:string[]=[];native.setHandler(async(command:string)=>{calls.push(command);if(command==='update_identity')return new Promise(r=>complete=r);if(command==='update_status')return null;assert.fail(command);});
 const page=mount(true);page.dispose();complete({os:'darwin'});await settled();assert.ok(!calls.includes('update_check'));
});
