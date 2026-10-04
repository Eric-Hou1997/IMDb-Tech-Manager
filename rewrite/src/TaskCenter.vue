<script setup lang="ts">
import {computed,nextTick,ref,watch} from 'vue';
import type {AiRuntime,Task,TaskJob} from './contracts';
import {currentLog,currentTask,jobSummary,taskControls} from './task-center';
const props=defineProps<{open:boolean;height:number;history:boolean;historyText:string;historyFixed:boolean;job:TaskJob|null;tasks:Task[];runtime:AiRuntime|null;failures:number;busy:boolean;locale:string}>();
const emit=defineEmits<{toggle:[];resize:[event:PointerEvent];key:[event:KeyboardEvent];tab:[history:boolean];action:[name:'pause'|'recover'|'resume'|'retry']}>();
const log=ref<HTMLElement>();
const task=computed(()=>currentTask(props.tasks));
const controls=computed(()=>taskControls(props.tasks,props.runtime,props.failures));
const text=computed(()=>props.history?props.historyText:currentLog(props.job));
watch(text,async()=>{if(props.history)return;await nextTick();if(!props.history&&log.value)log.value.scrollTop=log.value.scrollHeight;});
</script>
<template>
 <section id="taskDrawer" class="drawer" :class="{open}">
  <div id="taskResize" class="taskResize" role="separator" title="拖动调整任务中心高度" aria-label="拖动调整任务中心高度" aria-orientation="horizontal" :aria-valuenow="height" aria-valuemin="190" aria-valuemax="4000" tabindex="0" @pointerdown="emit('resize',$event)" @keydown="emit('key',$event)"></div>
  <div class="drawerHead" role="button" tabindex="0" :aria-expanded="open" @click="emit('toggle')" @keydown.enter.prevent="emit('toggle')" @keydown.space.prevent="emit('toggle')"><strong>任务中心</strong><span id="jobSummary" class="muted">{{jobSummary(job,task,locale)}}</span><div class="grow"></div><span id="jobPill" class="pill" :class="job?.running?'warn':job?.exit_code?'bad':job?.action?'ok':''">{{job?.running?'运行中':job?.action?(job.exit_code?'失败':'完成'):'空闲'}}</span><span id="taskChevron">{{open?'⌄':'⌃'}}</span></div>
  <div class="drawerBody"><pre id="jobLog" ref="log" class="log" :data-fixed-language="String(history?historyFixed:!!job?.log)">{{text}}</pre><div class="taskActions"><button id="currentTaskTab" class="btn" @click="emit('tab',false)">本次日志</button><button id="historyTaskTab" class="btn" @click="emit('tab',true)">历史任务</button><span class="grow"></span><button id="pauseTask" class="btn" :class="{hide:!controls.pause}" :disabled="busy" @click="emit('action','pause')">暂停 AI</button><button id="recoverAI" class="btn yellow" :class="{hide:!controls.recover}" :disabled="busy" title="额度或认证错误恢复后，真实测试当前供应商；成功才解除 AI 暂停" @click="emit('action','recover')">恢复 AI</button><button id="resumeTask" class="btn" :class="{hide:!controls.resume}" :disabled="busy" @click="emit('action','resume')">继续任务</button><button id="retryFailed" class="btn" :class="{hide:!controls.retry}" :disabled="busy" @click="emit('action','retry')">重试失败</button></div></div>
 </section>
</template>
