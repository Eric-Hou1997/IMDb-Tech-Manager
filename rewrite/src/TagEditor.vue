<script setup lang="ts">
import {computed,onUnmounted,reactive,watch} from 'vue';
import type {Action,MediaItem} from './contracts';
import {TagDraft} from './editor-drafts';
const props=defineProps<{item:MediaItem;busy:boolean;applied?:{hash:string;action:Action}|null}>();
const emit=defineEmits<{preview:[action:Action];dirty:[value:boolean];edited:[]}>();
const model=reactive(new TagDraft());
watch(()=>[props.item.id,props.item.source_hash],()=>model.receive(props.item,props.applied?.hash===props.item.source_hash?props.applied.action:undefined),{immediate:true});
watch(()=>model.dirty,value=>emit('dirty',value),{immediate:true});
watch(()=>[model.selected,model.value,model.ownership,model.newTag,model.externalConfirmed,model.clearConfirmed],()=>emit('edited'));
onUnmounted(()=>emit('dirty',false));
const blocked=computed(()=>props.busy||model.conflict||model.pendingSelection!==null||!!props.item.error);
function choose(index:number){if(!blocked.value)model.choose(index);}
function preview(action:Action){if(!blocked.value)emit('preview',action);}
</script>
<template>
 <section aria-label="标签编辑">
  <h5>标签与归属</h5>
  <p v-if="model.conflict" role="alert">文件中的标签已改变，当前草稿仍保留。请先核对文件，再放弃旧草稿载入当前标签。<button :disabled="busy" @click="model.discard(item)">放弃标签草稿并载入当前文件</button></p>
  <div v-if="model.pendingSelection!==null" role="alert"><p>当前标签的更改尚未保存。</p><button :disabled="busy" @click="model.pendingSelection=null">继续编辑当前标签</button><button :disabled="busy" @click="model.choose(model.pendingSelection!,true)">放弃当前标签更改并切换</button></div>
  <fieldset :disabled="blocked"><legend>选择当前文件的标签</legend>
   <table><thead><tr><th>选择</th><th>标签</th><th>归属</th></tr></thead><tbody><tr v-for="(tag,index) in model.tags" :key="index"><td><input type="radio" :checked="model.selected===index" :aria-label="`选择标签 ${tag.value}`" @click.prevent="choose(index)"></td><td>{{ tag.value }}</td><td>{{ tag.ownership==='generated'?'Generated':tag.ownership==='manual'?'Manual':'External' }} {{ tag.engine }}</td></tr></tbody></table>
   <p v-if="!model.tags.length">当前文件没有根标签。</p>
   <div v-if="model.selected>=0" class="edit-tag">
    <label>标签内容<input v-model="model.value" maxlength="320"></label>
    <p v-if="model.tags[model.selected]?.ownership==='generated'">编辑 Generated 标签会将其转为受保护的 Manual。</p>
    <button :disabled="!model.value.trim()" @click="preview({kind:'edit',root_index:model.selected,value:model.value})">预览标签修改</button>
    <label v-if="model.tags[model.selected]?.ownership==='external'"><input type="checkbox" v-model="model.externalConfirmed">我确认删除此 External 标签；它可能属于其他工具。</label>
    <button :disabled="model.tags[model.selected]?.ownership==='external'&&!model.externalConfirmed" @click="preview({kind:'delete',root_index:model.selected,confirm_external:model.externalConfirmed})">预览删除标签</button>
    <label>调整归属<select v-model="model.ownership"><option value="external">External</option><option value="manual">Manual</option><option value="ai">Generated · AI</option><option value="local-rules">Generated · 规则</option></select></label>
    <p>设为 Generated 后，后续生成可替换此标签；设为 External 或 Manual 后，自动生成会保护它。</p>
    <button @click="preview({kind:'set-ownership',root_index:model.selected,ownership:model.ownership})">预览归属调整</button>
   </div>
   <label>新增 Manual 标签<input v-model="model.newTag" maxlength="320"></label><button :disabled="!model.newTag.trim()" @click="preview({kind:'add-manual',value:model.newTag})">预览新增标签</button>
   <label><input type="checkbox" v-model="model.clearConfirmed">清除当前文件有明确 AI 归属的 Generated 标签</label><button :disabled="!model.clearConfirmed" @click="preview({kind:'clear-ai',confirmed:model.clearConfirmed})">预览清除 AI 标签</button>
  </fieldset>
 </section>
</template>
<style scoped>
fieldset{border:1px solid #65768b66;border-radius:8px;padding:12px}table{width:100%;border-collapse:collapse;text-align:left}td,th{padding:6px;overflow-wrap:anywhere}label{display:block;margin-block:10px}input:not([type=radio]):not([type=checkbox]),select{font:inherit;max-width:100%;box-sizing:border-box;margin-inline:8px}button{margin:4px}.edit-tag{border-block:1px solid #65768b44;margin-block:10px;padding-block:10px}h5{font-size:16px;margin:12px 0}
</style>
