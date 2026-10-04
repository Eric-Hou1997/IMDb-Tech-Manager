<script setup lang="ts">
import {computed,onMounted,onUnmounted,ref,shallowRef,watch} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import {listen,type UnlistenFn} from '@tauri-apps/api/event';
import {saveAiProfile,type AiSettingsRequest} from './ai-settings-save';
import type {AiSettings,AiProfile,AiRuntime,AiRecord,AppError,MediaItem,WritePreview} from './contracts';
const props=defineProps<{item?:MediaItem;blocked:boolean;active?:boolean}>();
const emit=defineEmits<{preview:[value:WritePreview];busy:[value:boolean];dirty:[value:boolean];notice:[value:string]}>();
const settings=ref<AiSettings|null>(null),configured=ref(false),secret=ref(''),extra=ref('{}'),error=ref(''),busy=ref(false),record=shallowRef<AiRecord|null>(null),history=shallowRef<AiRecord[]>([]),requestId=ref(''),showSettings=ref(!props.item),runtime=ref<AiRuntime|null>(null),credentialError=ref<AppError|null>(null);
const historyItem=()=>props.item?.id??'__connection_test__';
let generation=0,disposed=false,executing=false,unlisten:UnlistenFn|undefined,runtimeUnlisten:UnlistenFn|undefined,profileUnlisten:UnlistenFn|undefined;
const profileConflict=ref(false),profileRevision=ref('');
const profileBaseline=ref('');
let profileLoad=0,savingProfile=false;
const draftFingerprint=()=>JSON.stringify([settings.value,extra.value]);
const profileDirty=computed(()=>settings.value!==null&&(!!secret.value||draftFingerprint()!==profileBaseline.value));
watch(profileDirty,value=>emit('dirty',value));
let settingsRequest:{fingerprint:string;request:AiSettingsRequest}|null=null;
function receive(value:AiRecord){if(disposed||value.request.operation_id!==requestId.value)return;record.value=value;if(!['requested','running'].includes(value.phase)){requestId.value='';void refreshRuntime().catch(report);if(!executing){busy.value=false;emit('busy',false);}}}
watch(requestId,(id,_,cleanup)=>{if(!id)return;let polling=false;const timer=window.setInterval(async()=>{if(polling)return;polling=true;try{receive(await invoke<AiRecord>('ai_record',{id}));}catch(e){if(!disposed&&requestId.value===id)report(e);}finally{polling=false;}},750);cleanup(()=>window.clearInterval(timer));});
function report(e:unknown){const value=e as AppError;error.value=value.code?`${value.code}：${value.message}${value.path?'\n'+value.path:''}`:String(e);}
async function loadHistory(){const token=generation;const result=await invoke<AiRecord[]>('ai_history',{itemId:historyItem()});if(!disposed&&token===generation){history.value=result;if(!record.value)record.value=result[0]??null;const pending=result.find(row=>['requested','running'].includes(row.phase));if(pending&&!busy.value){requestId.value=pending.request.operation_id;record.value=pending;busy.value=true;emit('busy',true);}}}
watch(()=>props.item?.id,()=>{generation++;requestId.value='';executing=false;busy.value=false;emit('busy',false);record.value=null;error.value='';void loadHistory().catch(report);},{immediate:true});
async function refreshRuntime(){const value=await invoke<AiRuntime>('ai_runtime');if(!disposed)runtime.value=value;}
async function loadProfile(discardDraft=false){const token=++profileLoad;const profile=await invoke<AiProfile>('ai_settings');if(disposed||token!==profileLoad)return;configured.value=profile.credential_ready;credentialError.value=profile.credential_error;const dirty=!!secret.value||draftFingerprint()!==profileBaseline.value;if(settings.value&&!discardDraft&&dirty){profileConflict.value=profile.revision!==profileRevision.value;return;}settings.value=profile.settings;profileRevision.value=profile.revision;extra.value=JSON.stringify(profile.settings.config.extra_body,null,2);profileBaseline.value=draftFingerprint();profileConflict.value=false;if(discardDraft){secret.value='';settingsRequest=null;}}
let initializingProfile=false;
async function initializeProfile(){
 if(initializingProfile||disposed)return;initializingProfile=true;
 try{
  if(!profileUnlisten){const release=await listen('ai-settings-changed',()=>{if(!savingProfile)void loadProfile().catch(report);});if(disposed){release();return;}profileUnlisten=release;}
  if(!unlisten){const release=await listen<AiRecord>('ai-changed',event=>receive(event.payload));if(disposed){release();return;}unlisten=release;}
  if(!runtimeUnlisten){const release=await listen<AiRuntime>('ai-runtime-changed',event=>{if(!disposed)runtime.value=event.payload;});if(disposed){release();return;}runtimeUnlisten=release;}
  await Promise.all([loadProfile(),refreshRuntime()]);
 }catch(e){if(!disposed)report(e);}finally{initializingProfile=false;}
}
watch(()=>props.active,async(value,old)=>{if(value&&old===false){const enteredSecret=secret.value;await loadProfile(true).catch(report);if(!disposed)secret.value=enteredSecret;}});
onMounted(initializeProfile);
onUnmounted(()=>{disposed=true;generation++;unlisten?.();runtimeUnlisten?.();profileUnlisten?.();emit('busy',false);emit('dirty',false);});
async function run(work:(token:number)=>Promise<void>){if(busy.value||props.blocked)return;busy.value=true;executing=true;emit('busy',true);error.value='';const token=generation;try{await work(token);}catch(e){if(!disposed&&token===generation)report(e);}finally{if(!disposed&&token===generation){executing=false;busy.value=!!requestId.value;emit('busy',busy.value);}}}
async function persistSettings(){
 if(!settings.value)throw Error('AI 设置尚未加载');
 const body=JSON.parse(extra.value);
 if(!body||Array.isArray(body)||typeof body!=='object')throw Error('附加请求参数必须是 JSON 对象');
 settings.value.config.extra_body=body;
 settings.value.config.model=settings.value.config.model.trim();
 settings.value.config.base_url=settings.value.config.base_url.trim();
 const fingerprint=JSON.stringify([settings.value,secret.value]);
 if(profileConflict.value&&settingsRequest?.fingerprint!==fingerprint)throw Error('AI 设置已在其他位置更新，请先载入当前设置。');
 if(settingsRequest?.fingerprint!==fingerprint)settingsRequest={fingerprint,request:{id:crypto.randomUUID(),settings:JSON.parse(JSON.stringify(settings.value)),secret:secret.value||null,expectedRevision:profileRevision.value}};
 savingProfile=true;
 try{
  const profile=await saveAiProfile(invoke,settingsRequest.request);
  ++profileLoad;settings.value=profile.settings;profileRevision.value=profile.revision;configured.value=profile.credential_ready;credentialError.value=profile.credential_error;
  extra.value=JSON.stringify(profile.settings.config.extra_body,null,2);secret.value='';settingsRequest=null;profileBaseline.value=draftFingerprint();profileConflict.value=false;emit('notice','AI 设置已保存');
 }catch(e){await loadProfile().catch(report);throw e;}finally{savingProfile=false;}
 // A receipt already proved the save. Follow-up status failure must not turn it
 // into an apparent failed commit or leave the previous revision as the baseline.
 await Promise.all([loadProfile(),refreshRuntime()]).catch(e=>{report(e);error.value='AI 设置已保存；后续状态暂未刷新。'+error.value;});
}
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
 <section :aria-label="item?'标签生成':'AI 标签生成'" class="generator">
  <h3>{{item?'当前文件标签生成':'AI 标签生成'}}</h3><p v-if="item">生成候选后先检查差异，再确认写入当前文件。现有 External 与 Manual 标签受保护。</p>
  <div class="actions" v-if="item"><button :disabled="busy||blocked||!!item.error" @click="configured&&settings?.enabled?generate():showSettings=true">AI 生成标签</button><button :disabled="busy||blocked||!!item.error" @click="rules">规则生成标签</button><button :disabled="busy||blocked" @click="showSettings=!showSettings">AI 设置</button><button v-if="requestId" @click="cancel">取消 AI 请求</button></div>
  <p v-if="profileConflict" role="alert">AI 设置已在其他位置更新。当前未保存的内容已保留；重新载入后才能保存。<button :disabled="busy||blocked" @click="run(()=>loadProfile(true))">放弃当前草稿并载入已保存设置</button></p>
  <p v-if="credentialError" role="alert">凭据暂不可用：{{credentialError.code}} · {{credentialError.message}}。设置仍可查看和修改。</p>
  <section v-if="item&&runtime" aria-label="AI 运行状态"><p>{{runtime.paused?'AI 已暂停':'AI 未暂停'}}<span v-if="runtime.reason"> · {{runtime.reason_kind}}：{{runtime.reason}}</span></p><p v-if="runtime.paused_at">暂停时间：{{runtime.paused_at}}</p><p v-if="runtime.last_success_at">最近成功请求：{{runtime.last_success_at}}</p><button v-if="runtime.paused" :disabled="busy||blocked" @click="resume">清除暂停（不发送请求）</button><p v-if="runtime.paused">保存设置不会解除暂停。成功的连接测试可清除鉴权、额度、限流或临时错误暂停；预算和无法确认的旧状态需手动清除。</p></section>
  <button v-if="!item&&requestId" @click="cancel">取消 AI 请求</button>
  <div v-if="settings&&showSettings" class="baselineAI"><div class="formGrid"><div class="field"><label for="aiProtocol">API 格式 <span class="fieldInlineHelp">（按服务商提供的 URL 格式选择）</span></label><select class="select" id="aiProtocol" v-model="settings.config.protocol" :disabled="busy||blocked"><option value="openai">OpenAI Chat Completions</option><option value="anthropic">Anthropic Messages</option></select></div><div class="field"><label for="aiModel">模型</label><input class="input" id="aiModel" v-model="settings.config.model" :disabled="busy||blocked"></div><div class="field wide"><label for="aiBase">API Base URL</label><input class="input" id="aiBase" v-model="settings.config.base_url" :disabled="busy||blocked"></div><div class="field"><label for="aiKey">API Key（留空保持不变）</label><input class="input" type="password" id="aiKey" v-model="secret" :disabled="busy||blocked"></div><div class="field"><label for="aiPromptCache">Prompt Cache</label><select class="select" id="aiPromptCache" v-model="settings.config.prompt_cache_mode" :disabled="busy||blocked"><option value="auto">自动</option><option value="on">开启</option><option value="off">关闭</option></select></div><div class="field"><label for="aiThinking">推理模式</label><select class="select" id="aiThinking" v-model="settings.config.thinking_mode" :disabled="busy||blocked"><option value="auto">模型默认</option><option value="off">关闭</option><option value="on">开启</option></select></div><div class="field"><label for="aiEnabled">启用 AI</label><select class="select" id="aiEnabled" v-model="settings.enabled" :disabled="busy||blocked"><option :value="true">启用</option><option :value="false">停用</option></select></div><div class="field"><label for="aiTokens">首次 AI 输出长度上限（Tokens）</label><input class="input" type="number" id="aiTokens" v-model.number="settings.config.max_tokens" :disabled="busy||blocked"></div><div class="field"><label for="aiTokenCap">超长后重试上限（Tokens）</label><input class="input" type="number" id="aiTokenCap" v-model.number="settings.output_token_cap" :disabled="busy||blocked"></div><div class="field wide"><label for="aiPrompt">系统提示词</label><textarea class="textarea" id="aiPrompt" v-model="settings.config.prompt" :disabled="busy||blocked"></textarea></div><details class="field wide advancedAI"><summary>高级 AI 参数与本次预算</summary><div class="formGrid"><div class="field"><label for="aiTemperature">Temperature</label><input class="input" type="number" step="0.1" id="aiTemperature" v-model.number="settings.config.temperature" :disabled="busy||blocked"></div><div class="field"><label for="aiTopP">Top P</label><input class="input" type="number" step="0.05" id="aiTopP" v-model.number="settings.config.top_p" :disabled="busy||blocked"></div><div class="field"><label for="aiTimeout">超时（秒）</label><input class="input" type="number" id="aiTimeout" v-model.number="settings.timeout_seconds" :disabled="busy||blocked"></div><div class="field"><label for="aiRetries">重试次数</label><input class="input" type="number" id="aiRetries" v-model.number="settings.retry_count" :disabled="busy||blocked"></div><div class="field"><label for="aiRequestBudget">请求数预算（0=不限）</label><input class="input" type="number" id="aiRequestBudget" v-model.number="settings.run_request_limit" :disabled="busy||blocked"></div><div class="field"><label for="aiTokenBudget">Token 预算（0=不限）</label><input class="input" type="number" id="aiTokenBudget" v-model.number="settings.run_token_limit" :disabled="busy||blocked"></div><div class="field"><label for="aiCostBudget">费用预算（0=不限）</label><input class="input" type="number" step="0.01" id="aiCostBudget" v-model.number="settings.run_cost_limit" :disabled="busy||blocked"></div><div class="field"><label for="aiJSONMode">JSON 模式</label><select class="select" id="aiJSONMode" v-model="settings.json_mode" :disabled="busy||blocked"><option value="auto">自动</option><option value="on">开启</option><option value="off">关闭</option></select></div></div></details></div><div style="margin-top:10px"><button class="btn yellow" id="saveAI" :disabled="busy||blocked" @click="save">保存 AI 设置</button></div></div>
  <section v-if="item&&record" aria-label="AI 结果"><p>{{record.purpose==='connection-test'?'AI 连接测试 · ':''}}{{ record.phase }} · {{ record.cached?'缓存命中':record.meter.attempts?'Provider 请求':'未发送请求' }} · {{ record.title }} {{ record.year }}</p>
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
  <details v-if="item&&history.length"><summary>{{item?'当前文件 AI 历史':'AI 连接测试历史'}}（{{ history.length }}）</summary><button v-for="row in history" :key="row.request.operation_id" :disabled="busy" @click="record=row">{{ row.started_at }} · {{ row.phase }}</button></details>
  <button v-if="!settings||error" :disabled="busy||blocked" @click="run(initializeProfile)">重新载入 AI 设置与状态</button><p v-if="busy" role="status">正在处理 AI 操作…</p><pre v-if="error" role="alert">{{ error }}</pre><button v-if="item&&error.startsWith('legacy-ai-cache-invalid')" :disabled="busy||blocked||!configured||!settings?.enabled" @click="generate(true)">跳过损坏缓存并重新请求 AI</button>
 </section>
</template>
<style scoped>
.generator{margin:0}.actions{display:flex;flex-wrap:wrap;gap:8px;margin-block:10px}.baselineAI .field>label{display:block}.baselineAI input,.baselineAI textarea,.baselineAI select{width:100%;min-width:0;box-sizing:border-box}.baselineAI #aiPrompt{min-height:300px;line-height:1.5}.baselineAI .advancedAI{margin-top:10px;border-top:1px solid var(--line);padding-top:10px}.baselineAI .advancedAI summary{cursor:pointer;color:var(--muted)}pre{white-space:pre-wrap;overflow-wrap:anywhere}
</style>
