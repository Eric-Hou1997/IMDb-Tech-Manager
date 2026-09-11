<script setup lang="ts">
import {ref,shallowRef,watch} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import type {Task,Space,BatchEngine,BatchMode,BatchItem,WritePreview} from './contracts';
const props=defineProps<{space:Space;selected:string[];roots:string[];tasks:Task[]}>();
const emit=defineEmits<{refresh:[];inspect:[id:string]}>();
const mode=ref<BatchMode>('generate'), full=ref(false), busy=ref(false), error=ref('');
const review=shallowRef<WritePreview|null>(null);
const pending=shallowRef<{task:Task;row:BatchItem}|null>(null);
const active=shallowRef<Task|null>(null);
const detailTask=shallowRef<Task|null>(null), rowOffset=ref(0);
async function load(task:Task){await run(async()=>{detailTask.value=await invoke<Task>('batch_detail',{id:task.id});rowOffset.value=0;});}
watch(()=>props.space,()=>{active.value=null;review.value=null;full.value=false;});
async function run(fn:()=>Promise<void>){if(busy.value)return;busy.value=true;error.value='';try{await fn();}catch(e){error.value=JSON.stringify(e);}finally{busy.value=false;}}
async function plan(engine:BatchEngine,retry?:Task){await run(async()=>{
 if(retry)retry=await invoke<Task>('batch_detail',{id:retry.id});
 const request={operation_id:crypto.randomUUID(),space:props.space,engine,mode:retry?'rebuild':mode.value,item_ids:retry?retry.batch!.items.filter(i=>['failed','skipped-unchanged-failure','skipped-legacy-failure','skipped-legacy-unverified'].includes(i.phase)).map(i=>i.item.id):full.value?[]:[...props.selected],root_ids:retry||!full.value?[]:[...props.roots],retry_failed:!!retry};
 active.value=await invoke<Task>('plan_batch',{request});emit('refresh');
});}
async function approve(task:Task){await run(async()=>{active.value=await invoke<Task>('approve_batch_scope',{id:task.id,reviewedHash:task.batch!.plan_hash});emit('refresh');});}
async function show(task:Task,row:BatchItem){await run(async()=>{const result=await invoke<{kind:string;result:WritePreview}>('operation_result',{id:row.write_id});if(result.kind!=='write')throw Error('候选记录类型不匹配');review.value=result.result;pending.value={task,row};});}
async function apply(){if(!review.value||!pending.value)return;const value=review.value;await run(async()=>{review.value=await invoke<WritePreview>('apply_batch_item',{taskId:pending.value!.task.id,writeId:value.operation_id,reviewedHash:value.after_hash});if(detailTask.value)detailTask.value=await invoke<Task>('batch_detail',{id:detailTask.value.id});emit('refresh');});}
</script>
<template>
 <section class="batch-panel"><h3>批量操作</h3>
 <label>处理方式 <select v-model="mode"><option value="generate">补全 / 生成（跳过已完成项）</option><option value="rebuild">刷新 / 重建</option><option value="preview">只预演（最多 10 项）</option></select></label>
 <label><input v-model="full" type="checkbox">明确选择勾选根目录的全部已索引 {{space==='movie'?'Movie':'TV'}} 条目</label>
 <p>{{full?'将逐项列出所勾选根目录的完整范围，确认后开始。':`仅处理已选 ${selected.length} 项，包含所选季的全部已索引剧集。`}}</p>
 <div class="actions"><button :disabled="busy||(!full&&!selected.length)||(full&&!roots.length)" @click="plan('ai')">AI 生成标签</button><button :disabled="busy||(!full&&!selected.length)||(full&&!roots.length)" @click="plan('rules')">规则生成标签</button><button :disabled="busy||(!full&&!selected.length)||(full&&!roots.length)" @click="plan('specs')">IMDb 规格获取</button></div>
 <pre v-if="error" role="alert">{{error}}</pre>
 <article v-if="active?.batch&&!active.batch.approved"><h4>确认批量范围</h4><p>{{active.batch.engine}} · {{active.batch.mode}} · {{active.batch.items.length}} 项 · {{active.locale}}</p><p v-if="active.batch.engine==='ai'">AI 将按保存的配置发出请求并可能产生费用；需要审核的结果保留为候选。</p><p v-if="active.batch.mode!=='preview'">确认后允许写入上述范围；每项使用校验备份，外部修改会阻止写入。</p><div class="scope"><p v-for="row in active.batch.items" :key="row.item.id">{{row.item.title}} · {{row.item.year}} · {{row.item.imdb}}<br>{{row.item.path}}</p></div><button :disabled="busy" @click="approve(active)">确认此范围并开始</button></article>
 <article v-for="task in tasks.filter(t=>t.space===space&&t.batch)" :key="task.id">
 <h4>{{task.automatic?'自动补全 · ':''}}{{task.batch!.engine}} · {{task.batch!.mode}} · {{task.processed}} / {{task.batch!.total}}</h4>
 <p v-if="task.batch!.pause_requested&&task.state==='running'">已请求暂停，当前 NFO 完成后停止取下一项。</p><p v-if="task.batch!.cancel_requested&&task.state==='running'">已请求取消，正在完成当前 NFO 的事务收尾。</p>
 <button v-if="!task.batch!.approved&&task.state==='paused'" @click="run(async()=>{active=await invoke<Task>('batch_detail',{id:task.id});})">查看待确认范围</button>
 <button v-if="['completed','failed','paused','interrupted','cancelled'].includes(task.state)&&(task.errors>0||task.batch!.engine==='ai')" :disabled="busy" @click="plan(task.batch!.engine,task)">仅重试失败项（先确认范围）</button>
 <button :disabled="busy" @click="load(task)">查看逐项结果与审批</button><div v-if="detailTask?.id===task.id"><div class="scope"><p v-for="row in detailTask.batch!.items.slice(rowOffset,rowOffset+100)" :key="row.item.id"><button @click="emit('inspect',row.item.id)">{{row.item.title||row.item.path}}</button> · {{row.phase}}<br>{{row.item.year}} · {{row.item.imdb}} · {{row.item.path}}<br><span v-if="row.error">{{row.error.code}}：{{row.error.message}}</span><button v-if="row.candidate_hash" :disabled="busy" @click="show(task,row)">查看差异与结果</button></p></div><button :disabled="rowOffset===0" @click="rowOffset=Math.max(0,rowOffset-100)">上一页结果</button><button :disabled="rowOffset+100>=detailTask.batch!.total" @click="rowOffset+=100">下一页结果</button></div>
 </article>
 <aside v-if="review"><h4>批量候选审批 · {{review.title}}</h4><p>{{review.path}} · {{review.phase}}</p><h5>Technical Specs 与归属原文</h5><pre>{{review.before_xml}}</pre><h5>候选内容</h5><pre>{{review.after_xml}}</pre><h5>原根标签</h5><pre>{{review.before_tags}}</pre><h5>候选根标签</h5><pre>{{review.after_tags}}</pre><button v-if="!['committed','unchanged'].includes(review.phase)" :disabled="busy" @click="apply">确认该候选并写入（不重新请求 AI）</button><button @click="review=null">关闭差异</button></aside>
 </section>
</template>
<style scoped>.batch-panel{border:1px solid #65768b66;padding:12px;border-radius:8px;margin:16px 0}.batch-panel label{display:block;margin:10px 0}.scope{max-height:300px;overflow:auto;overflow-wrap:anywhere}.batch-panel pre{max-height:300px;overflow:auto;white-space:pre-wrap;overflow-wrap:anywhere}.batch-panel article{border-top:1px solid #65768b44;margin-top:12px}</style>
