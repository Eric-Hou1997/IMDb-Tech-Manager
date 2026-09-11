<script setup lang="ts">
import {onMounted,onUnmounted,ref,shallowRef,watch} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import {listen,type UnlistenFn} from '@tauri-apps/api/event';
import type {AiSettings,AiRecord,AppError,MediaItem,WritePreview} from './contracts';
const props=defineProps<{item:MediaItem;blocked:boolean}>();
const emit=defineEmits<{preview:[value:WritePreview];busy:[value:boolean]}>();
const settings=ref<AiSettings|null>(null),configured=ref(false),secret=ref(''),extra=ref('{}'),error=ref(''),busy=ref(false),record=shallowRef<AiRecord|null>(null),history=shallowRef<AiRecord[]>([]),requestId=ref(''),showSettings=ref(false);
let generation=0,disposed=false,executing=false,unlisten:UnlistenFn|undefined;
let settingsRequest:{id:string;fingerprint:string}|null=null;
function receive(value:AiRecord){if(disposed||value.request.item_id!==props.item.id||value.request.operation_id!==requestId.value)return;record.value=value;if(!['requested','running'].includes(value.phase)){requestId.value='';if(!executing){busy.value=false;emit('busy',false);}}}
watch(requestId,(id,_,cleanup)=>{if(!id)return;let polling=false;const timer=window.setInterval(async()=>{if(polling)return;polling=true;try{receive(await invoke<AiRecord>('ai_record',{id}));}catch(e){if(!disposed&&requestId.value===id)report(e);}finally{polling=false;}},750);cleanup(()=>window.clearInterval(timer));});
function report(e:unknown){const value=e as AppError;error.value=value.code?`${value.code}：${value.message}${value.path?'\n'+value.path:''}`:String(e);}
async function loadHistory(){const token=generation;const result=await invoke<AiRecord[]>('ai_history',{itemId:props.item.id});if(!disposed&&token===generation){history.value=result;if(!record.value)record.value=result[0]??null;const pending=result.find(row=>['requested','running'].includes(row.phase));if(pending&&!busy.value){requestId.value=pending.request.operation_id;record.value=pending;busy.value=true;emit('busy',true);}}}
watch(()=>props.item.id,()=>{generation++;requestId.value='';executing=false;busy.value=false;emit('busy',false);record.value=null;error.value='';void loadHistory().catch(report);},{immediate:true});
onMounted(async()=>{try{const [value,ready]=await invoke<[AiSettings,boolean]>('ai_settings');if(disposed)return;settings.value=value;configured.value=ready;extra.value=JSON.stringify(value.config.extra_body,null,2);const release=await listen<AiRecord>('ai-changed',event=>receive(event.payload));if(disposed)release();else unlisten=release;}catch(e){report(e);}});
onUnmounted(()=>{disposed=true;generation++;unlisten?.();emit('busy',false);});
async function run(work:(token:number)=>Promise<void>){if(busy.value||props.blocked)return;busy.value=true;executing=true;emit('busy',true);error.value='';const token=generation;try{await work(token);}catch(e){if(!disposed&&token===generation)report(e);}finally{if(!disposed&&token===generation){executing=false;busy.value=false;emit('busy',false);}}}
async function save(){await run(async()=>{if(!settings.value)return;const body=JSON.parse(extra.value);if(!body||Array.isArray(body)||typeof body!=='object')throw Error('附加请求参数必须是 JSON 对象');settings.value.config.extra_body=body;const fingerprint=JSON.stringify([settings.value,secret.value]);if(settingsRequest?.fingerprint!==fingerprint)settingsRequest={id:crypto.randomUUID(),fingerprint};settings.value=await invoke<AiSettings>('save_ai_settings',{id:settingsRequest.id,settings:settings.value,secret:secret.value||null});configured.value=configured.value||!!secret.value;secret.value='';settingsRequest=null;showSettings.value=false;});}
async function generate(force=false,retryFailed=false){await run(async(token)=>{const id=crypto.randomUUID();requestId.value=id;try{const value=await invoke<AiRecord>('generate_ai',{request:{operation_id:id,item_id:props.item.id,expected_hash:props.item.source_hash,force,retry_failed:retryFailed}});if(!disposed&&token===generation){record.value=value;await loadHistory();}}catch(e){try{const value=await invoke<AiRecord>('ai_record',{id});if(!disposed&&token===generation)record.value=value;}catch{/* Keep the original error and request ID. */}throw e;}finally{if(!disposed&&token===generation)requestId.value='';}});}
async function cancel(){try{record.value=await invoke<AiRecord>('cancel_ai',{id:requestId.value});}catch(e){report(e);}}
async function rules(){await run(async(token)=>{const value=await invoke<WritePreview>('preview_rules',{id:crypto.randomUUID(),itemId:props.item.id,expectedHash:props.item.source_hash});if(!disposed&&token===generation)emit('preview',value);});}
async function prepare(){const current=record.value;if(!current)return;await run(async(token)=>{const value=await invoke<WritePreview>('preview_ai',{id:crypto.randomUUID(),aiId:current.request.operation_id});if(!disposed&&token===generation)emit('preview',value);});}
</script>
<template>
 <section aria-label="标签生成" class="generator">
  <h5>当前文件标签生成</h5><p>生成候选后先检查差异，再确认写入当前文件。现有 External 与 Manual 标签受保护。</p>
  <div class="actions"><button :disabled="busy||blocked||!!item.error" @click="configured&&settings?.enabled?generate():showSettings=true">AI 生成标签</button><button :disabled="busy||blocked||!!item.error" @click="rules">规则生成标签</button><button :disabled="busy||blocked" @click="showSettings=!showSettings">AI 设置</button><button v-if="requestId" @click="cancel">取消 AI 请求</button></div>
  <form v-if="settings&&showSettings" @submit.prevent="save"><fieldset :disabled="busy||blocked"><legend>AI Provider 与请求设置</legend>
   <label>Provider<input v-model="settings.config.provider" required></label><label>协议<select v-model="settings.config.protocol"><option value="openai">OpenAI compatible</option><option value="anthropic">Anthropic</option></select></label>
   <label>API 地址<input v-model="settings.config.base_url" type="url" required></label><label>模型<input v-model="settings.config.model" required></label>
   <label><input v-model="settings.enabled" type="checkbox">启用 AI</label>
   <label>API Key<input v-model="secret" type="password" autocomplete="new-password" :placeholder="configured?'已配置；留空保留现有凭据':'输入 Provider 凭据'"></label>
   <label class="wide">系统提示词<textarea v-model="settings.config.prompt" rows="9" required /></label>
   <label>Temperature<input v-model.number="settings.config.temperature" type="number" min="0" max="1.9" step="0.1"></label><label>Top P<input v-model.number="settings.config.top_p" type="number" min="0.05" max="1" step="0.05"></label>
   <label>初始输出上限<input v-model.number="settings.config.max_tokens" type="number" min="128" max="32768"></label><label>截断恢复上限<input v-model.number="settings.output_token_cap" type="number" min="4096" max="32768"></label>
   <label>思考模式<select v-model="settings.config.thinking_mode"><option value="off">关闭</option><option value="on">开启</option><option value="auto">自动</option></select></label><label>提示缓存<select v-model="settings.config.prompt_cache_mode"><option value="auto">自动</option><option value="on">开启</option><option value="off">关闭</option></select></label>
   <label>JSON 模式<select v-model="settings.json_mode"><option value="auto">自动兼容</option><option value="on">开启</option><option value="off">关闭</option></select></label>
   <label>模型警告<select v-model="settings.warning_policy"><option value="review">需要人工复核</option><option value="accept">接受并保留警告</option></select></label><label>AI 失败策略<select v-model="settings.fallback_mode"><option value="abort">停止该项</option><option value="local-rules">回退到规则生成</option></select></label>
   <label>请求超时（秒）<input v-model.number="settings.timeout_seconds" type="number" min="10" max="600"></label><label>临时失败重试次数<input v-model.number="settings.retry_count" type="number" min="0" max="5"></label>
   <label>输入价格／百万 tokens<input v-model.number="settings.input_price_per_million" type="number" min="0" step="0.01"></label><label>输出价格／百万 tokens<input v-model.number="settings.output_price_per_million" type="number" min="0" step="0.01"></label>
   <label>单次任务请求上限（0 为不限）<input v-model.number="settings.run_request_limit" type="number" min="0"></label><label>单次任务 token 上限（0 为不限）<input v-model.number="settings.run_token_limit" type="number" min="0"></label><label>单次任务费用上限（0 为不限）<input v-model.number="settings.run_cost_limit" type="number" min="0" step="0.01"></label>
   <label class="wide">附加请求参数（JSON）<textarea v-model="extra" rows="4" /></label><button type="submit">保存 AI 设置</button>
  </fieldset></form>
  <section v-if="record" aria-label="AI 结果"><p>{{ record.phase }} · {{ record.cached?'缓存命中':record.meter.attempts?'Provider 请求':'未发送请求' }} · {{ record.title }} {{ record.year }}</p>
   <p v-if="record.engine==='local-rules'">AI 失败后按设置回退为规则候选，原请求费用及错误保留。</p>
   <p>本次实际尝试 {{ record.meter.attempts }} 次 · 输入 {{ record.meter.current.input }} / 输出 {{ record.meter.current.output }} tokens · 费用 {{ record.cost.toFixed(6) }}</p>
   <p v-if="record.cached">缓存来源历史 usage：{{ record.meter.historical_cache.total }} tokens · 历史费用 {{ record.historical_cost.toFixed(6) }}；不计入本次费用。</p>
   <details v-if="record.legacy_cache"><summary>旧版缓存 · {{ record.legacy_cache.created_at }} · {{ record.legacy_cache.model }}</summary><p>{{ record.legacy_cache.source }} · 导入 {{ record.legacy_cache.import_id }}</p><pre>{{ JSON.stringify(record.legacy_cache.raw_usage,null,2) }}</pre></details>
   <details v-if="record.legacy_failure"><summary>原版失败记录 · {{ record.legacy_failure.source }}</summary><p>导入 {{record.legacy_failure.import_id}} · {{record.legacy_failure.path}}</p><pre>{{JSON.stringify(record.legacy_failure.entry,null,2)}}</pre></details>
   <p v-if="record.phase==='skipped-legacy-failure'">输入与请求配置仍匹配旧失败记录，本次未发送请求。检查原因后可明确重试失败项。</p><p v-if="record.phase==='skipped-legacy-unverified'">旧失败记录没有请求指纹，无法确认输入是否改变。保留原版跳过规则；明确重试后才继续处理。</p>
   <pre v-if="record.error" role="alert">{{ record.error.code }}：{{ record.error.message }}</pre><p v-if="record.phase==='interrupted'||record.phase==='cancelled'">已发出的请求可能已被 Provider 计费；没有收到 usage 的部分无法估算，也不会自动重复发送。</p>
   <details v-for="attempt in record.attempts" :key="attempt.sequence"><summary>请求 {{ attempt.sequence }} · {{ attempt.phase }} · {{ attempt.error?.code||attempt.http_status||'等待响应' }}</summary><pre>{{ JSON.stringify(attempt.request,null,2) }}</pre><pre>{{ JSON.stringify(attempt.raw_usage,null,2) }}</pre></details>
   <pre v-if="record.result">{{ JSON.stringify(record.result,null,2) }}</pre>
   <div class="actions"><button v-if="record.phase==='review-ready'" :disabled="busy||blocked" @click="prepare">检查 AI 结果并预览写入</button><button :disabled="busy||blocked||!configured||!settings?.enabled" @click="generate(true)">强制重建 AI 候选</button><button v-if="record.error" :disabled="busy||blocked||!configured||!settings?.enabled" @click="generate(false,true)">明确重试失败项</button></div>
  </section>
  <details v-if="history.length"><summary>当前文件 AI 历史（{{ history.length }}）</summary><button v-for="row in history" :key="row.request.operation_id" :disabled="busy" @click="record=row">{{ row.started_at }} · {{ row.phase }}</button></details>
  <p v-if="busy" role="status">正在处理标签请求…</p><pre v-if="error" role="alert">{{ error }}</pre><button v-if="error.startsWith('legacy-ai-cache-invalid')" :disabled="busy||blocked||!configured||!settings?.enabled" @click="generate(true)">跳过损坏缓存并重新请求 AI</button>
 </section>
</template>
<style scoped>
.generator{margin-block:16px}.actions{display:flex;flex-wrap:wrap;gap:8px;margin-block:10px}fieldset{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,240px),1fr));gap:12px;border:1px solid #65768b66;border-radius:8px}label{display:flex;flex-direction:column;gap:6px}input,textarea,select{font:inherit;min-width:0;padding:6px;box-sizing:border-box;width:100%}.wide{grid-column:1/-1}pre{white-space:pre-wrap;overflow-wrap:anywhere}h5{font-size:16px;margin:12px 0}details{margin-block:8px}details>button{display:block;margin-block:6px}
</style>
