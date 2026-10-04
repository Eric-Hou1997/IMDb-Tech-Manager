<script setup lang="ts">
import {computed,onMounted,onUnmounted,ref,watch} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import {listen,type UnlistenFn} from '@tauri-apps/api/event';
import type {CacheRequest,CacheStatus} from './contracts';
import ProductDialog from './ProductDialog.vue';
const emit=defineEmits<{modal:[open:boolean];busy:[value:boolean];dirty:[value:boolean]}>();
let off:UnlistenFn|undefined;
const listenerError=ref('');
const conflict=ref(false);
const status=ref<CacheStatus|null>(null),limit=ref(2048),busy=ref(false),error=ref(''),confirm=ref(false);
let disposed=false,pending:CacheRequest|null=null;
const changed=computed(()=>status.value&&limit.value!==status.value.settings.limit_mb);
const labels:Record<string,string>={ready:'已整理',busy:'部分缓存正在使用','over-limit':'仍超出上限',failed:'整理失败'};
function size(value:number){if(value<1024)return `${value} B`;if(value<1024*1024)return `${(value/1024).toFixed(1)} KiB`;return `${(value/1024/1024).toFixed(1)} MiB`;}
function report(e:unknown){if(e&&typeof e==='object'&&'message' in e){const value=e as {code?:string;message:string;path?:string};error.value=[value.code,value.message,value.path].filter(Boolean).join(' · ');}else error.value=String(e);}
watch(confirm,value=>emit('modal',value));watch(busy,value=>emit('busy',value));
watch(changed,value=>emit('dirty',Boolean(value)));
async function refresh(){
 if(busy.value)return;busy.value=true;error.value='';
 try{const result=await invoke<CacheStatus>('imdb_cache_status');if(!disposed){status.value=result;limit.value=result.settings.limit_mb;pending=null;conflict.value=false;}}
 catch(e){if(!disposed)report(e);}finally{busy.value=false;}
}
async function maintain(clear=false){
 if(busy.value||!status.value)return;
 if(!clear&&(!Number.isInteger(limit.value)||limit.value<64||limit.value>65536)){error.value='IMDb 缓存上限必须是 64–65536 MiB 的整数。';return;}
 busy.value=true;error.value='';
 const settings={revision:status.value.settings.revision,limit_mb:clear?status.value.settings.limit_mb:limit.value};
 if(!pending||pending.clear!==clear||JSON.stringify(pending.settings)!==JSON.stringify(settings))pending={operation_id:crypto.randomUUID(),settings,clear};
 try{const result=await invoke<CacheStatus>('maintain_imdb_cache',{request:pending});if(!disposed){status.value=result;if(!clear)limit.value=result.settings.limit_mb;pending=null;confirm.value=false;}}
 catch(e){if(!disposed)report(e);}finally{busy.value=false;}
}
async function connect(){
 if(off||disposed)return;
 try{const unlisten=await listen<CacheStatus>('cache-changed',event=>{
  if(disposed||busy.value)return;
  const editing=changed.value;
  if(editing&&status.value?.settings.revision!==event.payload.settings.revision)conflict.value=true;
  status.value=event.payload;if(!editing)limit.value=event.payload.settings.limit_mb;
 });
 if(disposed){unlisten();return;}off=unlisten;listenerError.value='';
 }catch(e){if(!disposed)listenerError.value=`缓存状态实时同步失败：${String(e)}`;}
}
async function reconnect(){await connect();if(!disposed)await refresh();}
onMounted(reconnect);
onUnmounted(()=>{disposed=true;off?.();emit('modal',false);emit('busy',false);emit('dirty',false);});
</script>
<template>
 <div class="rootGroup cacheSettings">
  <div class="rootGroupHead"><h3>IMDb 缓存</h3><div class="rootActions cacheControls"><label for="imdbCacheLimit">上限</label><input id="imdbCacheLimit" v-model.number="limit" class="input" type="number" min="64" max="65536" step="1" inputmode="numeric" :disabled="busy"><span>MB</span><button class="btn" :disabled="busy||!status||conflict" @click="maintain()">保存缓存设置</button><button class="btn danger" :disabled="busy||!status" @click="confirm=true">清空 IMDb 缓存</button></div></div>
  <div class="cacheUsage muted" role="status">{{busy?'正在统计或整理缓存…':status?`${status.last_cleanup?(labels[status.state]||status.state):'已统计'} · ${size(status.used_bytes)} / ${status.settings.limit_mb} MB · 原始页面 ${status.raw_count} · 解析结果 ${status.parsed_count}`:'用量尚未完成统计'}}</div>
  <p v-if="conflict" class="failure" role="alert">缓存设置已在其他入口更改，请重新读取后再保存。</p><p v-if="changed" class="muted">缓存上限更改尚未保存。</p><p v-if="status?.protected_count" class="muted">{{status.protected_count}} 项缓存正在使用，完成当前获取后可再次整理。</p>
  <p v-if="listenerError" class="failure" role="alert">{{listenerError}}</p>
  <p v-if="error||status?.error" class="failure" role="alert">{{error||[status?.error?.code,status?.error?.message].filter(Boolean).join(' · ')}}</p><button v-if="error||status?.error||conflict||listenerError" class="btn" :disabled="busy" @click="reconnect">重新读取缓存状态</button>
 </div>
 <ProductDialog v-if="confirm" title="清空 IMDb 缓存？" :busy="busy" @close="confirm=false"><p>清除可复用的 IMDb 原始页面和解析结果。NFO、AI 缓存、索引、浏览器会话、日志、任务记录和迁移恢复归档均保留。正在使用的缓存会跳过。</p><p v-if="error" class="failure" role="alert">{{error}}</p><div class="actions"><button class="btn" :disabled="busy" @click="confirm=false">取消</button><button class="btn danger" :disabled="busy" @click="maintain(true)">{{busy?'正在清理…':'确认清空 IMDb 缓存'}}</button></div></ProductDialog>
</template>
