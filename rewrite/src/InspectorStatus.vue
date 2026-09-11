<script setup lang="ts">
import {ref,watch} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import type {MediaItem,AnnotationAction,AnnotationRequest} from './contracts';
const props=defineProps<{item:MediaItem}>();const emit=defineEmits<{changed:[]}>();
const status=ref(''),busy=ref(false),error=ref('');let pending:AnnotationRequest|null=null;
const labels:Record<string,string>={'ai-complete':'AI 完成','local-complete':'规则完成','spec-ready':'规格就绪','spec-missing':'缺少规格','spec-empty':'已确认无规格','no-tags':'尚无技术标签','stale':'标签已过期','review':'待复核','tag-missing':'生成标签缺失','legacy':'历史标签','manual-spec':'手动规格','xml-error':'XML 错误','not-applicable':'不适用'};
const statusOptions=Object.entries(labels).filter(([value])=>!['xml-error','not-applicable'].includes(value));
const issues:Record<string,string>={'missing-imdb':'缺少 IMDb ID','spec-missing':'尚未准备 IMDb Technical Specs','duplicate-tag':'存在同值重复标签；不会自动删除','stale':'技术标签尚未与当前规格同步','review':'AI 结果等待人工复核','tag-missing':'manifest 中的生成标签在根节点缺失','library-type-mismatch':'NFO 类型与配置的 Movie／TV 空间不一致，生成已停止','xml-error':'XML 无效，不能忽略','read-error':'文件读取失败，不能忽略','ownership-mismatch':'归属信息与镜像不一致'};
watch(()=>[props.item.id,props.item.source_hash],()=>{status.value=props.item.inspection.status_override;error.value='';pending=null;},{immediate:true});
async function save(action?:AnnotationAction){if(busy.value)return;const id=props.item.id;busy.value=true;error.value='';try{
 if(action)pending={operation_id:crypto.randomUUID(),item_id:id,expected_hash:props.item.source_hash,action};
 if(!pending)return;await invoke('annotate_item',{request:pending});pending=null;if(props.item.id===id)emit('changed');
}catch(e){if(props.item.id===id)error.value=JSON.stringify(e);}finally{busy.value=false;}}
</script>
<template>
<section aria-label="状态与问题">
 <h4>状态与问题</h4><p>{{labels[item.inspection.lifecycle]||item.inspection.lifecycle}} · XML {{item.inspection.xml_valid?'有效':'无效'}} · {{item.inspection.newline}} · {{item.inspection.bom?'UTF-8 BOM':'无 BOM'}}</p>
 <p>手动状态只改变展示，不改变 NFO、标签归属或任务权限。文件内容改变后，对应的问题确认和手动状态自动失效。</p>
 <label>手动状态 <select v-model="status" :disabled="busy||!!pending||!!item.error"><option value="">按文件实际状态显示</option><option v-for="[value,label] in statusOptions" :key="value" :value="value">{{label}}</option></select></label>
 <button :disabled="busy||!!pending||!!item.error" @click="save({kind:'set-status',value:status})">保存展示状态</button>
 <ul><li v-for="issue in item.inspection.issues" :key="issue">{{issues[issue]||issue}} <button v-if="!['xml-error','read-error','library-type-mismatch'].includes(issue)" :disabled="busy||!!pending" @click="save({kind:'ignore',issue})">确认此版本的问题</button></li></ul>
 <p v-if="!item.inspection.issues.length">当前没有未确认的问题。</p>
 <details v-if="item.inspection.ignored_issues.length"><summary>已确认的问题（{{item.inspection.ignored_issues.length}}）</summary><ul><li v-for="issue in item.inspection.ignored_issues" :key="issue">{{issues[issue]||issue}}</li></ul><button :disabled="busy||!!pending" @click="save({kind:'restore-issues'})">恢复全部问题提示</button></details>
 <details v-if="Object.keys(item.inspection.source_specs).length"><summary>IMDb 来源快照</summary><p>有效规格时间 {{item.inspection.fetched_at||'未记录'}} · 来源时间 {{item.inspection.source_fetched_at||item.inspection.fetched_at||'未记录'}}</p><dl><template v-for="(values,field) in item.inspection.source_specs" :key="field"><dt>{{field}}</dt><dd>{{(values||[]).join('；')}}</dd></template></dl></details>
 <pre v-if="error" role="alert">{{error}}</pre><button v-if="pending&&!busy" @click="save()">查询并重试本次状态操作</button>
</section>
</template>
