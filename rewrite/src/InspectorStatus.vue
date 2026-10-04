<script setup lang="ts">
import {ref,watch,onUnmounted} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import type {MediaItem,AnnotationAction,AnnotationRequest} from './contracts';
import {statusClass,statusLabel} from './catalog-presentation';
import {statusOptions,issueLabel,issueMessage,canIgnoreIssue} from './inspector-presentation';
const props=defineProps<{item:MediaItem}>();const emit=defineEmits<{changed:[];notice:[string];failure:[unknown]}>();
const status=ref(''),busy=ref(false),error=ref('');let pending:AnnotationRequest|null=null;
let generation=0,disposed=false;
watch(()=>[props.item.id,props.item.source_hash,props.item.inspection.status_override],()=>{++generation;status.value=props.item.inspection.status_override;error.value='';pending=null;},{immediate:true});
onUnmounted(()=>{disposed=true;++generation;});
async function save(action:AnnotationAction) {
 if(busy.value)return;
 const id=props.item.id,expectedHash=props.item.source_hash,token=generation;
 const current=()=>!disposed&&token===generation&&props.item.id===id;
 busy.value=true;error.value='';
 try {
  // A repeated gesture resolves a lost receipt using the original ID first.
  let alreadyApplied=false;
  if(pending){const receipt=pending;await invoke('annotate_item',{request:receipt});if(!current())return;alreadyApplied=JSON.stringify(receipt.action)===JSON.stringify(action);pending=null;}
  if(!alreadyApplied){
   const request={operation_id:crypto.randomUUID(),item_id:id,expected_hash:expectedHash,action};pending=request;
   await invoke('annotate_item',{request});
  }
  if(current()){pending=null;emit('changed');emit('notice',action.kind==='set-status'?(action.value?'已手动指定状态':'已清除手动指定状态'):action.kind==='ignore'?'已忽略该问题':'已恢复问题检查');}
 } catch(e){if(current()){status.value=props.item.inspection.status_override;error.value=JSON.stringify(e);emit('failure',e);}}
 finally{busy.value=false;}
}
</script>
<template>
 <div class="card"><div class="sectionHead"><strong>状态</strong><span class="badge" :class="{generated:statusClass(item)==='ai'}">{{statusLabel(item)}}</span></div><div class="impact">手动指定状态只影响本 NFO 当前版本：内容变化后自动失效；重新生成标签也会覆盖它。</div><div class="statusOverride"><select class="select" id="statusOverrideSel" v-model="status" :disabled="busy||!!item.error" @change="save({kind:'set-status',value:status})"><option value="">{{item.inspection.status_override?'清除手动指定':'未手动指定'}}</option><option v-for="[value,label] in statusOptions" :key="value" :value="value">指定为 {{label}}</option></select><button class="btn" id="statusOverrideClear" :disabled="busy||!item.inspection.status_override||!!item.error" @click="status='';save({kind:'set-status',value:''})">清除</button></div></div>
 <div class="card"><h3>问题与建议 <span v-if="item.inspection.issues.length" class="badge">{{item.inspection.issues.length}}</span><span v-else class="badge generated">最佳状态</span></h3><div v-for="issue in item.inspection.issues" :key="issue" class="issue"><span class="badge">{{issueLabel(issue)}}</span><div class="value"><strong>{{issueMessage(item,issue)}}</strong><div class="muted" data-i18n-user>{{item.path}}</div><div v-if="canIgnoreIssue(issue)" class="issueActions"><button class="btn" :data-ignore-kind="issue" :disabled="busy" @click="save({kind:'ignore',issue})">忽略</button></div></div></div><div v-if="!item.inspection.issues.length" class="muted">当前没有未处理问题。</div><div v-if="item.inspection.ignored_issues.length" class="sourceBox">已忽略 {{item.inspection.ignored_issues.length}} 项；文件与身份仍显示权威的一致性结果，NFO 内容变化后会自动重新检查。 <button class="btn" data-restore-issues :disabled="busy" @click="save({kind:'restore-issues'})">恢复检查</button></div></div>
</template>
