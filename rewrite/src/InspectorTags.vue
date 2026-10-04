<script setup lang="ts">
import {ref,nextTick,watch,onMounted,onUnmounted} from 'vue';
import type {MediaItem,Tag} from './contracts';
const props=defineProps<{item:MediaItem;busy:boolean}>();
const emit=defineEmits<{edit:[number];remove:[number];add:[];clear:[];ownership:[number,string]}>();
const ownershipLabel=(tag:Tag)=>tag.ownership==='external'?'外部 / TMM':tag.ownership==='manual'?'Manual':(tag.engine||props.item.inspection.tag_engine)==='local-rules'?'规则生成':'AI 生成';
const choices=[['external','外部 / TMM'],['ai','AI 生成'],['local-rules','规则生成'],['manual','Manual']];
const menu=ref<{index:number;x:number;y:number}|null>(null),menuElement=ref<HTMLElement>();
let disposed=false,generation=0;
function close(){++generation;menu.value=null;}
async function open(event:MouseEvent,index:number){
 event.stopPropagation();if(props.busy)return;const token=++generation;
 menu.value={index,x:Math.min(event.clientX,innerWidth-170),y:event.clientY};
 await nextTick();if(!disposed&&token===generation&&menu.value)menu.value.y=Math.min(event.clientY,innerHeight-(menuElement.value?.offsetHeight||0)-10);
}
function choose(value:string){if(!menu.value)return;const index=menu.value.index;close();emit('ownership',index,value);}
function outside(event:MouseEvent){if(!(event.target instanceof Element)||!event.target.closest('#ownMenu,[data-own-badge]'))close();}
watch(()=>[props.item.id,props.item.source_hash],close);
onMounted(()=>{document.addEventListener('click',outside);window.addEventListener('blur',close);});
onUnmounted(()=>{disposed=true;close();document.removeEventListener('click',outside);window.removeEventListener('blur',close);});
</script>
<template>
 <div class="card"><div class="sectionHead tagsHeader"><strong>全部标签</strong><span class="muted tagHelp">点徽章可修改所有权（外部/TMM · AI 生成 · 规则生成 · Manual），确认后写入 NFO。</span><button class="btn" data-add-manual :disabled="busy||!!item.error" @click="emit('add')">手动添加 Tag</button></div><div v-for="(tag,index) in item.tags" :key="index" class="tagItem"><span class="badge ownBadge" :class="{generated:tag.ownership==='generated',manual:tag.ownership==='manual'}" :data-own-badge="index" @click="open($event,index)">{{ownershipLabel(tag)}}<span class="caret">›</span></span><span class="value" data-i18n-user>{{tag.value}}</span><span class="muted" data-i18n-user>{{tag.field}}</span><button class="iconBtn" :data-edit-tag="index" :disabled="busy||!!item.error" @click="emit('edit',index)">编辑</button><button class="iconBtn danger" :data-delete-tag="index" title="删除标签" :disabled="busy||!!item.error" @click="emit('remove',index)">删除</button></div><div v-if="!item.tags.length" class="muted">没有根级标签。</div><div style="margin-top:12px"><button class="btn danger" id="clearAiTags" :disabled="busy||!!item.error" @click="emit('clear')">一键清除 AI 标签</button></div></div>
 <Teleport to="body"><div v-if="menu" id="ownMenu" ref="menuElement" class="ownMenu" :style="{display:'block',left:menu.x+'px',top:menu.y+'px'}"><button v-for="[value,label] in choices" :key="value" :data-own-choice="value" @click="choose(value)">{{label}}</button></div></Teleport>
</template>
