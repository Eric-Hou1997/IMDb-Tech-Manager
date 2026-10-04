<script setup lang="ts">
import type {MediaItem} from './contracts';
import {specFields} from './editor-drafts';
const props=defineProps<{item:MediaItem;busy:boolean}>();
const emit=defineEmits<{edit:[string,number|null];remove:[string,number];restore:[]}>();
const tagFields=['Sound mix','Camera','Aspect ratio','Negative Format','Cinematographic Process','Printed Film Format'];
const values=(field:string)=>props.item.specs[field]||[];
const source=(field:string)=>props.item.inspection.source_specs[field]||[];
</script>
<template>
 <div v-if="item.spec_status==='missing'||item.kind==='Season'||item.error" class="empty">当前 NFO 没有可编辑的 Technical Specs。</div>
 <template v-else>
  <div class="card"><div v-for="field in specFields" :key="field" class="section"><div class="sectionHead"><strong>{{field}}</strong><span class="badge">{{values(field).length}} 条 · {{tagFields.includes(field)?'会生成 Tag':'仅保存 Spec'}}</span><button class="iconBtn" :data-add-spec="field" :disabled="busy" @click="emit('edit',field,null)">＋ 添加</button></div><div v-for="(value,index) in values(field)" :key="index" class="specItem"><span data-i18n-user>{{value}}</span><button class="iconBtn" :data-edit-spec="field" :data-index="index" :disabled="busy" @click="emit('edit',field,index)">编辑</button><button class="iconBtn danger" :data-delete-spec="field" :data-index="index" :disabled="busy" @click="emit('remove',field,index)">删除</button></div><div v-if="!values(field).length" class="muted">暂无值</div><div v-if="item.spec_status==='manual'&&JSON.stringify(source(field))!==JSON.stringify(values(field))" class="sourceBox">IMDb 原始抓取值：{{source(field).join('；')||'无'}}</div></div></div>
  <div class="card"><div class="sectionHead"><strong>规格来源与标签同步</strong><span class="badge" :class="{manual:item.spec_status==='manual'}">{{item.spec_status==='manual'?'当前使用人工修改后的规格':'当前使用 IMDb 抓取值'}}</span><button v-if="item.spec_status==='manual'" class="btn" data-restore-spec :disabled="busy" @click="emit('restore')">恢复 IMDb 抓取值</button></div><div class="impact">编辑规格后，Tag 状态会变为“待同步”；不会自动刷新 IMDb，也不会自动重建标签。请在底部选择 AI 或规则重新生成。</div></div>
 </template>
</template>
