<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { Action, AppError, FetchRecord, MediaItem, SpecsEdit, WritePreview } from './contracts';
import TagEditor from './TagEditor.vue';
import AiGenerator from './AiGenerator.vue';
const props=defineProps<{item:MediaItem}>();
const emit=defineEmits<{changed:[]}>();
const fields=['Runtime','Sound mix','Color','Aspect ratio','Camera','Laboratory','Film Length','Negative Format','Cinematographic Process','Printed Film Format'];
const editing=ref(false),busy=ref(false),aiBusy=ref(false),error=ref('');
const completed=ref<WritePreview|null>(null), fetched=ref<FetchRecord|null>(null), fetchingId=ref('');
function current(token:number){return !disposed&&token===generation;}
const draft=ref<Record<string,string>>({});
const preview=ref<WritePreview|null>(null),history=ref<WritePreview[]>([]),fetchHistory=ref<FetchRecord[]>([]);
let request:SpecsEdit|null=null, generation=0,disposed=false;
const changed=computed(()=>fields.filter(field=>JSON.stringify(preview.value?.before_specs[field]||[])!==JSON.stringify(preview.value?.after_specs[field]||[])));
function report(value:unknown){const e=value as AppError;error.value=e?.code?`${e.code}：${e.message}${e.path?'\n'+e.path:''}`:String(value);}
async function reloadHistory(){const token=generation;const [rows,requests]=await Promise.all([invoke<WritePreview[]>('write_history'),invoke<FetchRecord[]>('fetch_history',{itemId:props.item.id})]);if(!disposed&&token===generation){history.value=rows.filter(row=>row.item_id===props.item.id);fetchHistory.value=requests;if(!fetched.value)fetched.value=requests[0]??null;}}
function resetDraft(){draft.value=Object.fromEntries(fields.map(field=>[field,(props.item.specs[field]||[]).join('\n')]));preview.value=null;request=null;error.value='';}
watch(()=>[props.item.id,props.item.source_hash],(value,previous)=>{if(value[0]!==previous?.[0]){completed.value=null;fetched.value=null;}generation++;editing.value=false;resetDraft();void reloadHistory().catch(report);},{immediate:true});
onUnmounted(()=>{disposed=true;generation++;});
async function run(work:(token:number)=>Promise<void>){if(busy.value||aiBusy.value)return;const token=generation;busy.value=true;error.value='';try{await work(token);}catch(e){if(current(token))report(e);}finally{busy.value=false;}}
function draftChanged(){request=null;preview.value=null;}
async function fetchSpecs(refresh:boolean){await run(async(token)=>{
 const id=crypto.randomUUID();fetchingId.value=id;
 try {const value=await invoke<FetchRecord>('fetch_specs',{request:{operation_id:id,item_id:props.item.id,expected_hash:props.item.source_hash,refresh}});if(current(token)){fetched.value=value;if(value.error)report(value.error);await reloadHistory();}}
 finally {fetchingId.value='';}
});}
async function cancelFetch(){const id=fetchingId.value;if(!id)return;try{await invoke('cancel_fetch',{id});}catch(e){report(e);}}
async function sourcePreview(){const source=fetched.value;if(!source)return;await run(async(token)=>{const value=await invoke<WritePreview>('preview_source',{id:crypto.randomUUID(),fetchId:source.request.operation_id});if(current(token)){preview.value=value;await reloadHistory();}});}
async function tagPreview(action:Action){await run(async(token)=>{const value=await invoke<WritePreview>('preview_tags',{request:{operation_id:crypto.randomUUID(),item_id:props.item.id,expected_hash:props.item.source_hash,action}});if(current(token)){preview.value=value;await reloadHistory();}});}
async function prepare(){await run(async(token)=>{
 request??={operation_id:crypto.randomUUID(),item_id:props.item.id,expected_hash:props.item.source_hash,specs:Object.fromEntries(fields.map(field=>[field,draft.value[field].split('\n')]))};
 const value=await invoke<WritePreview>('preview_specs',{request});if(current(token)){preview.value=value;await reloadHistory();}
});}
async function apply(){const reviewed=preview.value;if(!reviewed)return;await run(async(token)=>{
 try {const value=await invoke<WritePreview>('apply_specs',{id:reviewed.operation_id,reviewedHash:reviewed.after_hash});if(current(token)){preview.value=value;if(value.phase==='committed')completed.value=value;}}
 catch(e){try{const status=await invoke<{kind:string;result:WritePreview}>('operation_result',{id:reviewed.operation_id});if(status.kind==='write'&&current(token))preview.value=status.result;}catch{/* The original write error remains visible; keep the same operation ID. */}throw e;}
 if(current(token)){await reloadHistory();emit('changed');}
});}
async function undo(row:WritePreview){await run(async(token)=>{const value=await invoke<WritePreview>('preview_undo',{id:crypto.randomUUID(),originalId:row.operation_id});if(current(token)){preview.value=value;editing.value=true;}});}
async function reload(){await run(async()=>{resetDraft();emit('changed');});}
</script>
<template>
 <section class="specs-editor">
  <p v-if="completed" role="status">{{ completed.undo_of?'撤销已完成':'写入已完成' }} · 备份和操作记录已保留。</p>
  <div class="actions"><button :disabled="busy||!!item.error||!item.imdb" @click="fetchSpecs(false)">获取 IMDb 规格</button><button :disabled="busy||!!item.error||!item.imdb" @click="fetchSpecs(true)">刷新 IMDb 来源</button><button v-if="fetchingId" @click="cancelFetch">取消获取</button></div>
  <section v-if="fetched" aria-label="IMDb 获取结果"><p>{{ fetched.cached?'已读取缓存':'IMDb 请求' }} · {{ fetched.phase }} · {{ fetched.imdb }}</p><p v-if="fetched.source">来源时间：{{ fetched.source.fetched_at }}。获取不会自动写入 NFO；手动修改的有效规格会保留。</p><ul><li v-for="(attempt,index) in fetched.attempts" :key="index">{{ attempt.transport }} · {{ attempt.error?attempt.error.code:'完成' }}</li></ul><button v-if="fetched.phase==='completed'" :disabled="busy" @click="sourcePreview">预览写入 IMDb 来源</button></section>
  <button v-if="!editing" :disabled="busy||!!item.error" @click="resetDraft();editing=true">编辑 Technical Specs</button>
  <div v-if="editing">
   <p>每行一个值。修改后先预览；确认写入时创建校验备份，并将派生标签标记过期。</p>
   <fieldset :disabled="busy"><legend>当前文件的规格</legend><label v-for="field in fields" :key="field">{{ field }}<textarea v-model="draft[field]" rows="2" @input="draftChanged" /></label></fieldset>
   <div class="actions"><button :disabled="busy" @click="prepare">预览更改</button><button :disabled="busy" @click="editing=false;preview=null;request=null">取消编辑</button><button :disabled="busy" @click="reload">重新读取文件</button></div>
  </div>
  <AiGenerator :item="item" :blocked="busy" @busy="aiBusy=$event" @preview="preview=$event;reloadHistory().catch(report)" />
  <TagEditor :item="item" :busy="busy||aiBusy" @preview="tagPreview" />
  <section v-if="preview" aria-label="写入预览" class="write-preview">
   <h5>{{ preview.undo_of?'撤销预览':'写入预览' }} · {{ preview.title }} {{ preview.year }}</h5>
   <p>{{ preview.imdb }} · {{ preview.media_kind }}</p><pre>{{ preview.path }}</pre>
   <div v-if="JSON.stringify(preview.before_tags)!==JSON.stringify(preview.after_tags)"><h5>根标签与归属变化</h5><p>原标签</p><ul><li v-for="(tag,index) in preview.before_tags" :key="index">{{ tag.value }} · {{ tag.ownership }} {{ tag.engine }}</li></ul><p>写入后的标签</p><ul><li v-for="(tag,index) in preview.after_tags" :key="index">{{ tag.value }} · {{ tag.ownership }} {{ tag.engine }}</li></ul></div>
   <details v-if="preview.before_xml||preview.after_xml"><summary>完整节点与来源元数据差异</summary><p>原节点</p><pre>{{ preview.before_xml||'（无）' }}</pre><p>候选节点</p><pre>{{ preview.after_xml||'（移除）' }}</pre></details>
   <dl><template v-for="field in changed" :key="field"><dt>{{ field }}</dt><dd><span>原值</span><pre>{{ (preview.before_specs[field]||[]).join('\n')||'（空）' }}</pre><span>新值</span><pre>{{ (preview.after_specs[field]||[]).join('\n')||'（空）' }}</pre></dd></template></dl>
   <p v-if="preview.phase==='unchanged'">内容没有变化，无需写入。</p>
   <p v-else-if="preview.phase==='committed'" role="status">写入已完成，备份及操作记录已保留。</p>
   <p v-else>操作状态：{{ preview.phase }}</p><p v-if="!changed.length&&preview.phase!=='unchanged'&&preview.intent.kind==='specs'">有效规格没有变化；本次预览更新来源时间、来源快照或其他 Technical Specs 元数据。</p>
   <pre v-if="preview.error" role="alert">{{ preview.error.code }}：{{ preview.error.message }}</pre>
   <button v-if="['preview','writing','committed-index-pending','committed-mirror-pending','metadata-pending'].includes(preview.phase)" :disabled="busy" @click="apply">{{ preview.phase==='preview'?'确认写入当前文件':'查询并恢复本次操作' }}</button>
   <p v-else-if="!['committed','unchanged'].includes(preview.phase)">本次操作未完成。请重新读取文件后建立新的预览；原操作记录与备份保留。</p>
  </section>
  <p v-if="busy" role="status">正在处理当前文件…</p><pre v-if="error" role="alert">{{ error }}</pre>
  <details v-if="fetchHistory.length"><summary>当前文件最近的获取记录</summary><article v-for="row in fetchHistory" :key="row.request.operation_id"><p>{{ row.started_at }} · {{ row.phase }} · {{ row.cached?'缓存':'网络' }}</p><button :disabled="busy" @click="fetched=row">查看来源与请求结果</button></article></details>
  <details v-if="history.length"><summary>当前文件写入记录（{{ history.length }}）</summary><article v-for="row in history" :key="row.operation_id"><p>{{ row.phase }} · {{ row.operation_id }}</p><button v-if="row.phase==='committed'" :disabled="busy" @click="undo(row)">预览撤销</button><button v-else-if="row.phase!=='unchanged'" :disabled="busy" @click="preview=row;editing=true">查看操作</button></article></details>
 </section>
</template>
<style scoped>
.specs-editor{margin-block:16px}fieldset{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,260px),1fr));gap:12px;border:1px solid #65768b66;border-radius:8px}label{display:flex;flex-direction:column;gap:6px}textarea{font:inherit;min-width:0;resize:vertical;padding:8px}.actions{display:flex;flex-wrap:wrap;gap:8px;margin-block:12px}.write-preview{border:1px solid #65768b66;padding:12px;border-radius:8px}pre{white-space:pre-wrap;overflow-wrap:anywhere}dt{font-weight:600}dd{margin-inline:0}dd span{font-size:12px}article{padding:8px 0;border-top:1px solid #65768b44}h5{font-size:16px;margin:0}
</style>
