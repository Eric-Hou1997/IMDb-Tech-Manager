<script setup lang="ts">
import {ref,watch} from 'vue';
import type {Action,MediaItem} from './contracts';
const props=defineProps<{item:MediaItem;busy:boolean}>();
const emit=defineEmits<{preview:[action:Action]}>();
const selected=ref(-1),value=ref(''),ownership=ref('external'),externalConfirmed=ref(false),clearConfirmed=ref(false),newTag=ref('');
watch(()=>[props.item.id,props.item.source_hash],()=>{selected.value=-1;value.value='';externalConfirmed.value=false;clearConfirmed.value=false;newTag.value='';});
function choose(index:number){selected.value=index;value.value=props.item.tags[index].value;ownership.value=props.item.tags[index].ownership==='generated'?(props.item.tags[index].engine||'local-rules'):props.item.tags[index].ownership;externalConfirmed.value=false;}
</script>
<template>
 <section aria-label="标签编辑">
  <h5>标签与归属</h5>
  <fieldset :disabled="busy||!!item.error"><legend>选择当前文件的标签</legend>
   <table><thead><tr><th>选择</th><th>标签</th><th>归属</th></tr></thead><tbody><tr v-for="(tag,index) in item.tags" :key="index"><td><input type="radio" :checked="selected===index" :aria-label="`选择标签 ${tag.value}`" @change="choose(index)"></td><td>{{ tag.value }}</td><td>{{ tag.ownership==='generated'?'Generated':tag.ownership==='manual'?'Manual':'External' }} {{ tag.engine }}</td></tr></tbody></table>
   <p v-if="!item.tags.length">当前文件没有根标签。</p>
   <div v-if="selected>=0" class="edit-tag">
    <label>标签内容<input v-model="value" maxlength="320"></label>
    <p v-if="item.tags[selected]?.ownership==='generated'">编辑 Generated 标签会将其转为受保护的 Manual。</p>
    <button :disabled="!value.trim()" @click="emit('preview',{kind:'edit',root_index:selected,value})">预览标签修改</button>
    <label v-if="item.tags[selected]?.ownership==='external'"><input type="checkbox" v-model="externalConfirmed">我确认删除此 External 标签；它可能属于其他工具。</label>
    <button :disabled="item.tags[selected]?.ownership==='external'&&!externalConfirmed" @click="emit('preview',{kind:'delete',root_index:selected,confirm_external:externalConfirmed})">预览删除标签</button>
    <label>调整归属<select v-model="ownership"><option value="external">External</option><option value="manual">Manual</option><option value="ai">Generated · AI</option><option value="local-rules">Generated · 规则</option></select></label>
    <p>设为 Generated 后，后续生成可替换此标签；设为 External 或 Manual 后，自动生成会保护它。</p>
    <button @click="emit('preview',{kind:'set-ownership',root_index:selected,ownership})">预览归属调整</button>
   </div>
   <label>新增 Manual 标签<input v-model="newTag" maxlength="320"></label><button :disabled="!newTag.trim()" @click="emit('preview',{kind:'add-manual',value:newTag})">预览新增标签</button>
   <label><input type="checkbox" v-model="clearConfirmed">清除当前文件有明确 AI 归属的 Generated 标签</label><button :disabled="!clearConfirmed" @click="emit('preview',{kind:'clear-ai',confirmed:clearConfirmed})">预览清除 AI 标签</button>
  </fieldset>
 </section>
</template>
<style scoped>
fieldset{border:1px solid #65768b66;border-radius:8px;padding:12px}table{width:100%;border-collapse:collapse;text-align:left}td,th{padding:6px;overflow-wrap:anywhere}label{display:block;margin-block:10px}input:not([type=radio]):not([type=checkbox]),select{font:inherit;max-width:100%;box-sizing:border-box;margin-inline:8px}button{margin:4px}.edit-tag{border-block:1px solid #65768b44;margin-block:10px;padding-block:10px}h5{font-size:16px;margin:12px 0}
</style>
