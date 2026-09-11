<script setup lang="ts">
import {onMounted,onUnmounted,ref,shallowRef,watch} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import {listen,type UnlistenFn} from '@tauri-apps/api/event';
import type {AiSettings,AiProfile,AiRuntime,AiRecord,AppError,MediaItem,WritePreview} from './contracts';
const props=defineProps<{item?:MediaItem;blocked:boolean}>();
const emit=defineEmits<{preview:[value:WritePreview];busy:[value:boolean]}>();
const settings=ref<AiSettings|null>(null),configured=ref(false),secret=ref(''),extra=ref('{}'),error=ref(''),busy=ref(false),record=shallowRef<AiRecord|null>(null),history=shallowRef<AiRecord[]>([]),requestId=ref(''),showSettings=ref(!props.item),runtime=ref<AiRuntime|null>(null),credentialError=ref<AppError|null>(null);
const historyItem=()=>props.item?.id??'__connection_test__';
let generation=0,disposed=false,executing=false,unlisten:UnlistenFn|undefined,runtimeUnlisten:UnlistenFn|undefined;
let settingsRequest:{id:string;fingerprint:string}|null=null;
function receive(value:AiRecord){if(disposed||value.request.operation_id!==requestId.value)return;record.value=value;if(!['requested','running'].includes(value.phase)){requestId.value='';void refreshRuntime().catch(report);if(!executing){busy.value=false;emit('busy',false);}}}
watch(requestId,(id,_,cleanup)=>{if(!id)return;let polling=false;const timer=window.setInterval(async()=>{if(polling)return;polling=true;try{receive(await invoke<AiRecord>('ai_record',{id}));}catch(e){if(!disposed&&requestId.value===id)report(e);}finally{polling=false;}},750);cleanup(()=>window.clearInterval(timer));});
function report(e:unknown){const value=e as AppError;error.value=value.code?`${value.code}：${value.message}${value.path?'\n'+value.path:''}`:String(e);}
async function loadHistory(){const token=generation;const result=await invoke<AiRecord[]>('ai_history',{itemId:historyItem()});if(!disposed&&token===generation){history.value=result;if(!record.value)record.value=result[0]??null;const pending=result.find(row=>['requested','running'].includes(row.phase));if(pending&&!busy.value){requestId.value=pending.request.operation_id;record.value=pending;busy.value=true;emit('busy',true);}}}
watch(()=>props.item?.id,()=>{generation++;requestId.value='';executing=false;busy.value=false;emit('busy',false);record.value=null;error.value='';void loadHistory().catch(report);},{immediate:true});
async function refreshRuntime(){const value=await invoke<AiRuntime>('ai_runtime');if(!disposed)runtime.value=value;}
async function loadProfile(){const profile=await invoke<AiProfile>('ai_settings');if(disposed)return;settings.value=profile.settings;configured.value=profile.credential_ready;credentialError.value=profile.credential_error;extra.value=JSON.stringify(profile.settings.config.extra_body,null,2);}
onMounted(async()=>{try{await Promise.all([loadProfile(),refreshRuntime()]);if(disposed)return;const release=await listen<AiRecord>('ai-changed',event=>receive(event.payload));if(disposed)release();else unlisten=release;const releaseRuntime=await listen<AiRuntime>('ai-runtime-changed',event=>{runtime.value=event.payload;});if(disposed)releaseRuntime();else runtimeUnlisten=releaseRuntime;}catch(e){report(e);}});
onUnmounted(()=>{disposed=true;generation++;unlisten?.();runtimeUnlisten?.();emit('busy',false);});
async function run(work:(token:number)=>Promise<void>){if(busy.value||props.blocked)return;busy.value=true;executing=true;emit('busy',true);error.value='';const token=generation;try{await work(token);}catch(e){if(!disposed&&token===generation)report(e);}finally{if(!disposed&&token===generation){executing=false;busy.value=!!requestId.value;emit('busy',busy.value);}}}
async function persistSettings(){if(!settings.value)throw Error('AI 设置尚未加载');const body=JSON.parse(extra.value);if(!body||Array.isArray(body)||typeof body!=='object')throw Error('附加请求参数必须是 JSON 对象');settings.value.config.extra_body=body;const fingerprint=JSON.stringify([settings.value,secret.value]);if(settingsRequest?.fingerprint!==fingerprint)settingsRequest={id:crypto.randomUUID(),fingerprint};settings.value=await invoke<AiSettings>('save_ai_settings',{id:settingsRequest.id,settings:settings.value,secret:secret.value||null});secret.value='';settingsRequest=null;await loadProfile();await refreshRuntime();}
async function save(){await run(async()=>{await persistSettings();if(props.item)showSettings.value=false;});}
async function request(command:'generate_ai'|'test_ai_connection',token:number,force=false,retryFailed=false){const id=crypto.randomUUID();requestId.value=id;try{const value=await invoke<AiRecord>(command,command==='test_ai_connection'?{id}:{request:{operation_id:id,item_id:props.item!.id,expected_hash:props.item!.source_hash,force,retry_failed:retryFailed}});if(!disposed&&token===generation){record.value=value;await loadHistory();}}catch(e){try{const value=await invoke<AiRecord>('ai_record',{id});if(!disposed&&token===generation)record.value=value;}catch{/* Preserve the original failure when no operation receipt exists. */}throw e;}finally{if(!disposed&&token===generation){if(record.value?.request.operation_id!==id||!['requested','running'].includes(record.value.phase))requestId.value='';await refreshRuntime();}}}
async function generate(force=false,retryFailed=false){if(!props.item)return;await run(token=>request('generate_ai',token,force,retryFailed));}
async function testConnection(){await run(async(token)=>{await persistSettings();if(disposed||token!==generation)return;if(!configured.value)throw credentialError.value??Error('请先配置可读取的 Provider 凭据');await request('test_ai_connection',token);});}
async function resume(){await run(async()=>{await invoke('resume_ai_runtime',{id:crypto.randomUUID()});await refreshRuntime();});}
async function cancel(){try{record.value=await invoke<AiRecord>('cancel_ai',{id:requestId.value});}catch(e){report(e);}}
async function rules(){if(!props.item)return;const item=props.item;await run(async(token)=>{const value=await invoke<WritePreview>('preview_rules',{id:crypto.randomUUID(),itemId:historyItem(),expectedHash:item.source_hash});if(!disposed&&token===generation)emit('preview',value);});}
async function prepare(){const current=record.value;if(!current)return;await run(async(token)=>{const value=await invoke<WritePreview>('preview_ai',{id:crypto.randomUUID(),aiId:current.request.operation_id});if(!disposed&&token===generation)emit('preview',value);});}
</script>
<template>
 <section :aria-label="item?'标签生成':'AI 设置与运行状态'" class="generator">
  <h5>{{item?'当前文件标签生成':'AI 设置与运行状态'}}</h5><p v-if="item">生成候选后先检查差异，再确认写入当前文件。现有 External 与 Manual 标签受保护。</p>
  <div class="actions" v-if="item"><button :disabled="busy||blocked||!!item.error" @click="configured&&settings?.enabled?generate():showSettings=true">AI 生成标签</button><button :disabled="busy||blocked||!!item.error" @click="rules">规则生成标签</button><button :disabled="busy||blocked" @click="showSettings=!showSettings">AI 设置</button><button v-if="requestId" @click="cancel">取消 AI 请求</button></div>
  <p v-if="credentialError" role="alert">凭据暂不可用：{{credentialError.code}} · {{credentialError.message}}。设置仍可查看和修改。</p>
  <section v-if="runtime" aria-label="AI 运行状态"><p>{{runtime.paused?'AI 已暂停':'AI 未暂停'}}<span v-if="runtime.reason"> · {{runtime.reason_kind}}：{{runtime.reason}}</span></p><p v-if="runtime.paused_at">暂停时间：{{runtime.paused_at}}</p><p v-if="runtime.last_success_at">最近成功请求：{{runtime.last_success_at}}</p><button v-if="runtime.paused" :disabled="busy||blocked" @click="resume">清除暂停（不发送请求）</button><p v-if="runtime.paused">保存设置不会解除暂停。成功的连接测试可清除鉴权、额度、限流或临时错误暂停；预算和无法确认的旧状态需手动清除。</p></section>
  <button v-if="!item&&requestId" @click="cancel">取消 AI 请求</button>
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
   <label class="wide">附加请求参数（JSON）<textarea v-model="extra" rows="4" /></label><button type="submit">保存 AI 设置</button><button type="button" @click="testConnection">保存并连接测试（会请求模型）</button><p class="wide">连接测试使用内置 Technical Specs 样本，记录每次请求及费用，不写入媒体文件。即使未启用 AI 生成，也可手动测试。</p>
  </fieldset></form>
  <section v-if="record" aria-label="AI 结果"><p>{{record.purpose==='connection-test'?'AI 连接测试 · ':''}}{{ record.phase }} · {{ record.cached?'缓存命中':record.meter.attempts?'Provider 请求':'未发送请求' }} · {{ record.title }} {{ record.year }}</p>
   <p>{{record.resolved_model?(record.legacy_cache?'原记录模型':'响应模型'):'请求模型'}}：{{record.resolved_model||record.settings.config.model}}</p>
   <p v-if="record.engine==='local-rules'">AI 失败后按设置回退为规则候选，原请求费用及错误保留。</p>
   <p>本次实际尝试 {{ record.meter.attempts }} 次 · 输入 {{ record.meter.current.input }} / 输出 {{ record.meter.current.output }} tokens · 费用 {{ record.cost.toFixed(6) }}</p>
   <p v-if="record.cached">缓存来源历史 usage：{{ record.meter.historical_cache.total }} tokens · 历史费用 {{ record.historical_cost.toFixed(6) }}；不计入本次费用。</p>
   <details v-if="record.legacy_cache"><summary>旧版缓存 · {{ record.legacy_cache.created_at }} · {{ record.legacy_cache.model }}</summary><p>{{ record.legacy_cache.source }} · 导入 {{ record.legacy_cache.import_id }}</p><pre>{{ JSON.stringify(record.legacy_cache.raw_usage,null,2) }}</pre></details>
   <details v-if="record.legacy_failure"><summary>原版失败记录 · {{ record.legacy_failure.source }}</summary><p>导入 {{record.legacy_failure.import_id}} · {{record.legacy_failure.path}}</p><pre>{{JSON.stringify(record.legacy_failure.entry,null,2)}}</pre></details>
   <p v-if="record.phase==='skipped-legacy-failure'">输入与请求配置仍匹配旧失败记录，本次未发送请求。检查原因后可明确重试失败项。</p><p v-if="record.phase==='skipped-legacy-unverified'">旧失败记录没有请求指纹，无法确认输入是否改变。保留原版跳过规则；明确重试后才继续处理。</p>
   <pre v-if="record.error" role="alert">{{ record.error.code }}：{{ record.error.message }}</pre><p v-if="record.phase==='interrupted'||record.phase==='cancelled'">已发出的请求可能已被 Provider 计费；没有收到 usage 的部分无法估算，也不会自动重复发送。</p>
   <details v-for="attempt in record.attempts" :key="attempt.sequence"><summary>请求 {{ attempt.sequence }} · {{ attempt.phase }} · {{ attempt.error?.code||attempt.http_status||'等待响应' }}</summary><p v-if="attempt.model">响应模型：{{attempt.model}}</p><pre>{{ JSON.stringify(attempt.request,null,2) }}</pre><pre>{{ JSON.stringify(attempt.raw_usage,null,2) }}</pre></details>
   <pre v-if="record.result">{{ JSON.stringify(record.result,null,2) }}</pre>
   <div v-if="item&&record.purpose!=='connection-test'" class="actions"><button v-if="record.phase==='review-ready'" :disabled="busy||blocked" @click="prepare">检查 AI 结果并预览写入</button><button :disabled="busy||blocked||!configured||!settings?.enabled" @click="generate(true)">强制重建 AI 候选</button><button v-if="record.error" :disabled="busy||blocked||!configured||!settings?.enabled" @click="generate(false,true)">明确重试失败项</button></div>
  </section>
  <details v-if="history.length"><summary>{{item?'当前文件 AI 历史':'AI 连接测试历史'}}（{{ history.length }}）</summary><button v-for="row in history" :key="row.request.operation_id" :disabled="busy" @click="record=row">{{ row.started_at }} · {{ row.phase }}</button></details>
  <p v-if="busy" role="status">正在处理 AI 操作…</p><pre v-if="error" role="alert">{{ error }}</pre><button v-if="item&&error.startsWith('legacy-ai-cache-invalid')" :disabled="busy||blocked||!configured||!settings?.enabled" @click="generate(true)">跳过损坏缓存并重新请求 AI</button>
 </section>
</template>
<style scoped>
.generator{margin-block:16px}.actions{display:flex;flex-wrap:wrap;gap:8px;margin-block:10px}fieldset{display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,240px),1fr));gap:12px;border:1px solid #65768b66;border-radius:8px}label{display:flex;flex-direction:column;gap:6px}input,textarea,select{font:inherit;min-width:0;padding:6px;box-sizing:border-box;width:100%}.wide{grid-column:1/-1}pre{white-space:pre-wrap;overflow-wrap:anywhere}h5{font-size:16px;margin:12px 0}details{margin-block:8px}details>button{display:block;margin-block:6px}
</style>
