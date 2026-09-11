<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { AppError, MediaItem, SpecsEdit, WritePreview } from './contracts';
const props=defineProps<{item:MediaItem}>();
const emit=defineEmits<{changed:[]}>();
const fields=['Runtime','Sound mix','Color','Aspect ratio','Camera','Laboratory','Film Length','Negative Format','Cinematographic Process','Printed Film Format'];
const editing=ref(false),busy=ref(false),error=ref('');
const completed=ref<WritePreview|null>(null);
function current(token:number){return !disposed&&token===generation;}
const draft=ref<Record<string,string>>({});
const preview=ref<WritePreview|null>(null),history=ref<WritePreview[]>([]);
let request:SpecsEdit|null=null, generation=0,disposed=false;
const changed=computed(()=>fields.filter(field=>JSON.stringify(preview.value?.before_specs[field]||[])!==JSON.stringify(preview.value?.after_specs[field]||[])));
function report(value:unknown){const e=value as AppError;error.value=e?.code?`${e.code}：${e.message}${e.path?'\n'+e.path:''}`:String(value);}
async function reloadHistory(){const token=generation;const rows=await invoke<WritePreview[]>('write_history');if(!disposed&&token===generation)history.value=rows.filter(row=>row.item_id===props.item.id);}
function resetDraft(){draft.value=Object.fromEntries(fields.map(field=>[field,(props.item.specs[field]||[]).join('\n')]));preview.value=null;request=null;error.value='';}
watch(()=>[props.item.id,props.item.source_hash],(value,previous)=>{if(value[0]!==previous?.[0])completed.value=null;generation++;editing.value=false;resetDraft();void reloadHistory().catch(report);},{immediate:true});
onUnmounted(()=>{disposed=true;generation++;});
async function run(work:(token:number)=>Promise<void>){if(busy.value)return;const token=generation;busy.value=true;error.value='';try{await work(token);}catch(e){if(current(token))report(e);}finally{busy.value=false;}}
function draftChanged(){request=null;preview.value=null;}
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
  <button v-if="!editing" :disabled="busy||!!item.error" @click="resetDraft();editing=true">编辑 Technical Specs</button>
  <div v-if="editing">
   <p>每行一个值。修改后先预览；确认写入时创建校验备份，并将派生标签标记过期。</p>
   <fieldset :disabled="busy"><legend>当前文件的规格</legend><label v-for="field in fields" :key="field">{{ field }}<textarea v-model="draft[field]" rows="2" @input="draftChanged" /></label></fieldset>
   <div class="actions"><button :disabled="busy" @click="prepare">预览更改</button><button :disabled="busy" @click="editing=false;preview=null;request=null">取消编辑</button><button :disabled="busy" @click="reload">重新读取文件</button></div>
  </div>
  <section v-if="preview" aria-label="写入预览" class="write-preview">
   <h5>{{ preview.undo_of?'撤销预览':'写入预览' }} · {{ preview.title }} {{ preview.year }}</h5>
   <p>{{ preview.imdb }} · {{ preview.media_kind }}</p><pre>{{ preview.path }}</pre>
   <dl><template v-for="field in changed" :key="field"><dt>{{ field }}</dt><dd><span>原值</span><pre>{{ (preview.before_specs[field]||[]).join('\n')||'（空）' }}</pre><span>新值</span><pre>{{ (preview.after_specs[field]||[]).join('\n')||'（空）' }}</pre></dd></template></dl>
   <p v-if="preview.phase==='unchanged'">内容没有变化，无需写入。</p>
   <p v-else-if="preview.phase==='committed'" role="status">写入已完成，备份及操作记录已保留。</p>
   <p v-else>操作状态：{{ preview.phase }}</p>
   <pre v-if="preview.error" role="alert">{{ preview.error.code }}：{{ preview.error.message }}</pre>
   <button v-if="['preview','writing','committed-index-pending'].includes(preview.phase)" :disabled="busy" @click="apply">{{ preview.phase==='preview'?'确认写入当前文件':'查询并恢复本次操作' }}</button>
   <p v-else-if="!['committed','unchanged'].includes(preview.phase)">本次操作未完成。请重新读取文件后建立新的预览；原操作记录与备份保留。</p>
  </section>
  <p v-if="busy" role="status">正在处理当前文件…</p><pre v-if="error" role="alert">{{ error }}</pre>
  <details v-if="history.length"><summary>当前文件写入记录（{{ history.length }}）</summary><article v-for="row in history" :key="row.operation_id"><p>{{ row.phase }} · {{ row.operation_id }}</p><button v-if="row.phase==='committed'" :disabled="busy" @click="undo(row)">预览撤销</button><button v-else-if="row.phase!=='unchanged'" :disabled="busy" @click="preview=row;editing=true">查看操作</button></article></details>
 </section>
</template>
<style scoped>
.specs-editor{margin-block:16px}fieldset{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,260px),1fr));gap:12px;border:1px solid #65768b66;border-radius:8px}label{display:flex;flex-direction:column;gap:6px}textarea{font:inherit;min-width:0;resize:vertical;padding:8px}.actions{display:flex;flex-wrap:wrap;gap:8px;margin-block:12px}.write-preview{border:1px solid #65768b66;padding:12px;border-radius:8px}pre{white-space:pre-wrap;overflow-wrap:anywhere}dt{font-weight:600}dd{margin-inline:0}dd span{font-size:12px}article{padding:8px 0;border-top:1px solid #65768b44}h5{font-size:16px;margin:0}
</style>
