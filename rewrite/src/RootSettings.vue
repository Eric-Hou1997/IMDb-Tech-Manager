<script setup lang="ts">
import {computed,ref,watch} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import type {Configuration,LibraryRoot,Space,LibraryRootAccess,LegacyRoot} from './contracts';
const props=defineProps<{configuration:Configuration;pendingLegacy?:LegacyRoot[]}>();
const emit=defineEmits<{changed:[configuration:Configuration];confirmed:[];dirty:[value:boolean];busy:[value:boolean];notice:[value:string]}>();
const clone=<T,>(value:T):T=>JSON.parse(JSON.stringify(value));
const baseline=ref(clone(props.configuration)),draft=ref(clone(props.configuration));
const paths=ref<Record<Space,string>>({movie:'',tv:''});
const busy=ref(false),error=ref(''),saved=ref(false),conflict=ref(false);
let pending:{id:string;configuration:Configuration}|null=null;
const dirty=computed(()=>JSON.stringify(draft.value.roots)!==JSON.stringify(baseline.value.roots));
watch(dirty,value=>emit('dirty',value));watch(busy,value=>emit('busy',value));
watch(()=>props.configuration,value=>{
 if(value.revision===baseline.value.revision)return;
 if(dirty.value){
  if(!busy.value&&!pending&&JSON.stringify(value.roots)===JSON.stringify(baseline.value.roots)){
   baseline.value=clone(value);draft.value.locale=value.locale;draft.value.revision=value.revision;return;
  }
  conflict.value=true;return;
 }
 baseline.value=clone(value);draft.value=clone(value);pending=null;conflict.value=false;
},{deep:true});
function report(e:unknown){
 if(e&&typeof e==='object'&&'message' in e){const value=e as {message:unknown;code?:unknown;path?:unknown};error.value=[value.code,value.message,value.path].filter(Boolean).join(' · ');}else error.value=String(e);
}
function add(space:Space){
 const path=paths.value[space].trim();if(!path||busy.value)return;
 if(draft.value.roots.some(root=>root.space===space&&root.path===path)){return;}
 draft.value.roots.push({id:crypto.randomUUID(),space,path});paths.value[space]='';pending=null;saved.value=false;error.value='';
}
function remove(root:LibraryRoot){if(busy.value)return;draft.value.roots=draft.value.roots.filter(value=>value.id!==root.id);pending=null;saved.value=false;error.value='';}
async function choose(space:Space){
 if(busy.value)return;busy.value=true;error.value='';
 let selected:string|null=null;
 try{selected=await invoke<string|null>('choose_library_root',{space});}
 catch(e){report(e);}finally{busy.value=false;}
 if(selected){paths.value[space]=selected;add(space);}
}
async function test(root:LibraryRoot){
 if(busy.value)return;busy.value=true;error.value='';
 try{const result=await invoke<LibraryRootAccess>('test_library_root',{path:root.path});emit('notice',`${result.path}：${result.online?'可访问':'不可访问'}`);}
 catch(e){report(e);}finally{busy.value=false;}
}
async function save(){
 if(busy.value||conflict.value)return;busy.value=true;error.value='';saved.value=false;
 if(!pending)pending={id:crypto.randomUUID(),configuration:clone(draft.value)};
 try{
  const result=await invoke<Configuration>('save_library_roots',pending);
  baseline.value=clone(result);draft.value=clone(result);pending=null;conflict.value=false;saved.value=true;emit('changed',result);emit('confirmed');emit('notice','分类资料库已保存');
 }catch(e){report(e);}finally{busy.value=false;}
}
async function reload(){
 if(busy.value)return;busy.value=true;error.value='';
 try{const current=await invoke<Configuration>('configuration');baseline.value=clone(current);draft.value=clone(current);pending=null;conflict.value=false;saved.value=false;emit('changed',current);}
 catch(e){report(e);}finally{busy.value=false;}
}
defineExpose({isDirty:dirty,isBusy:busy,save,conflict});
</script>
<template>
 <div class="rootSettingsEditor">
  <div class="rootColumns"><div v-for="space in (['movie','tv'] as const)" :key="space" class="rootGroup"><div class="rootGroupHead"><h3>{{space==='movie'?'电影文件夹':'电视剧文件夹'}}</h3><div class="rootActions"><input :id="space==='movie'?'movieRootInput':'tvRootInput'" v-model="paths[space]" class="input" :disabled="busy" :aria-label="space==='movie'?'电影资料库路径':'电视剧资料库路径'" :placeholder="space==='movie'?'选择或输入电影资料库路径':'选择或输入电视剧资料库路径'" @keydown.enter.prevent="add(space)"><button class="btn" :data-choose-root="space==='movie'?'movies':'tv'" :disabled="busy" @click="choose(space)">选择文件夹</button><button class="btn" :data-add-root="space==='movie'?'movies':'tv'" :disabled="busy" @click="add(space)">添加路径</button></div></div><div :id="space==='movie'?'movieRoots':'tvRoots'"><div v-for="(root,index) in draft.roots.filter(root=>root.space===space)" :key="root.id" class="root"><span data-i18n-user>{{root.path}}</span><button class="btn" :data-test-root="root.path" :disabled="busy" @click="test(root)">测试访问</button><button class="btn danger" :data-remove-space="space==='movie'?'movies':'tv'" :data-remove-index="index" :disabled="busy" @click="remove(root)">移除</button></div><div v-if="!draft.roots.some(root=>root.space===space)" class="muted">尚未添加</div></div></div></div>
  <div id="unassignedRoots"><div v-if="pendingLegacy?.length" class="card"><strong>旧版待分类目录</strong><div class="muted">请将这些目录重新添加到电影或电视剧分类后保存。</div><div v-for="root in pendingLegacy" :key="`${root.space}:${root.path}`" class="root"><span data-i18n-user>{{root.path}}</span></div></div></div>
  <p v-if="conflict" class="failure" role="alert">目录配置已在其他入口发生变化，请先重新读取，再检查需要保存的目录。</p><p v-if="error" class="failure" role="alert">{{error}}</p>
  <slot name="after-roots" />
  <slot name="footer" :save="save" :busy="busy" :dirty="dirty" :conflict="conflict"><div class="rootSettingsFooter"><button class="btn blue" :disabled="busy||conflict" @click="save">保存分类资料库</button></div></slot>

 </div>
</template>
