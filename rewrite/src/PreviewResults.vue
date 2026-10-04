<script setup lang="ts">
import {computed} from 'vue';
import type {BatchEngine} from './contracts';
import {previewRecord,type PreviewEntry} from './preview-presentation';
const props=defineProps<{entries:PreviewEntry[];engine:BatchEngine;busy:boolean}>();
const emit=defineEmits<{close:[];approve:[]}>();
const records=computed(()=>props.entries.filter(entry=>entry.candidate&&!entry.error).map(previewRecord));
const skipped=computed(()=>props.entries.filter(entry=>!entry.candidate||entry.error));
</script>
<template>
 <Teleport to="body"><div id="previewModal" class="modal open" @click.self="!busy&&emit('close')"><div class="modalBox">
  <div class="modalHead"><h2 id="previewTitle">{{engine==='ai'?'AI 试写审核':'规则试写审核'}}</h2><button id="previewClose" class="btn" :disabled="busy" @click="emit('close')">关闭</button></div>
  <div id="previewBody">
   <div v-for="entry in skipped" :key="entry.row.item.id" class="previewRec"><h4>跳过：<span data-i18n-user>{{entry.row.item.title||entry.row.item.path}}</span></h4><div class="muted">{{entry.error}}</div></div>
   <div v-for="(record,index) in records" :key="index" class="previewRec"><h4><span data-i18n-user>{{record.title}}</span> <span class="muted">{{record.imdb}}</span> <span v-if="record.status==='review'" class="badge">待复核</span></h4><div class="previewLegend">黄色：{{engine==='ai'?'AI':'规则'}}生成标签　·　蓝灰：NFO 现有标签</div><div class="tagChips"><span v-for="tag in record.preview_tags" :key="tag.value" class="tagChip" :class="{generated:tag.kind==='generated'}" data-i18n-user>{{tag.value}}</span><span v-if="!record.preview_tags.length" class="muted">没有生成标签</span></div><div v-if="record.replaced_owned.length" class="tagLine">将替换：{{record.replaced_owned.join('；')}}</div><div v-if="record.warnings.length" class="tagLine warnLine">警告：{{record.warnings.join('；')}}</div><div v-if="record.review_reasons.length" class="tagLine warnLine">复核说明：{{record.review_reasons.join('；')}}</div></div>
  </div>
  <div class="previewFooter"><button id="previewApprove" class="btn yellow" :disabled="busy||!records.length" @click="emit('approve')">采纳并写入</button><span id="previewApproveNote" class="muted">{{records.length?engine==='ai'?`${records.length} 个 NFO · 使用已缓存模型结果，不再次请求 AI`:`${records.length} 个 NFO · 采纳后直接写入已审核的规则结果`:'没有可采纳的结果'}}</span></div>
 </div></div></Teleport>
</template>
