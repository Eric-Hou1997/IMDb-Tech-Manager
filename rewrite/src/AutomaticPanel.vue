<script setup lang="ts">
import {ref, onMounted, onUnmounted} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import type {AutomaticSettings,AutomaticStatus} from './contracts';
const status=ref<AutomaticStatus|null>(null),settings=ref<AutomaticSettings>({interval_seconds:60,on_app_start:false});
const busy=ref(false),error=ref('');
let timer:ReturnType<typeof setInterval>|undefined,disposed=false,polling=false;
let pending:{id:string;fingerprint:string}|null=null;
async function refresh(initial=false){if(polling)return;polling=true;try{const value=await invoke<AutomaticStatus>('automatic_status');if(disposed)return;status.value=value;if(initial)settings.value={...value.settings};}catch(e){if(!disposed)error.value=JSON.stringify(e);}finally{polling=false;}}
async function apply(enabled:boolean){if(busy.value)return;busy.value=true;error.value='';try{const fingerprint=JSON.stringify([settings.value,enabled]);if(pending?.fingerprint!==fingerprint)pending={id:crypto.randomUUID(),fingerprint};await invoke<AutomaticStatus>('automatic_apply',{id:pending.id,settings:settings.value,enabled});pending=null;await refresh();}catch(e){error.value=JSON.stringify(e);}finally{busy.value=false;}}
onMounted(async()=>{await refresh(true);if(!disposed)timer=setInterval(()=>void refresh(),2000);});
onUnmounted(()=>{disposed=true;if(timer)clearInterval(timer);});
</script>
<template>
<section><h2>自动补全 Technical Specs</h2>
<p>启用后允许为已确认根目录中的 Movie 和 TV 补齐 IMDb 规格。索引为空时先建立索引，之后每轮复用索引；不生成标签，手动任务优先。新增文件可刷新索引。</p>
<label>每轮结束后的间隔（秒）<input v-model.number="settings.interval_seconds" type="number" min="30" max="86400" :disabled="busy"></label>
<label><input v-model="settings.on_app_start" type="checkbox" :disabled="busy">启动应用时开启自动模式</label>
<p v-if="status" role="status">{{status.enabled?(status.task_ids.length?'自动模式正在处理或等待队列':'自动模式已开启，等待下一轮'):(status.task_ids.length?'已请求停止，等待当前文件收尾':'自动模式已停止')}} · 已建立 {{status.cycle}} 轮</p>
<p>登录启动在“窗口与后台”中单独设置。完全退出应用会停止后台工作。</p>
<div class="actions"><button :disabled="busy||!status" @click="apply(status!.enabled)">保存自动模式设置</button><button :disabled="busy||!status||status.enabled" @click="apply(true)">开启自动补全</button><button :disabled="busy||!status||!status.enabled" @click="apply(false)">停止自动补全</button></div>
<pre v-if="error" role="alert">{{error}}</pre></section>
</template>
<style scoped>label{display:block;margin:10px 0}input[type=number]{width:90px;margin-left:12px}pre{overflow-wrap:anywhere}</style>
