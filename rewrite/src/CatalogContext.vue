<script setup lang="ts">
import {computed,nextTick,onMounted,onUnmounted,ref,watch} from 'vue';
import type {MediaItem} from './contracts';
import {generationItems,type CatalogCommand} from './catalog-context';
const props=defineProps<{items:MediaItem[];x:number;y:number;platform:string}>();
const emit=defineEmits<{close:[];command:[CatalogCommand,MediaItem[]]}>();
const element=ref<HTMLElement>(),top=ref(props.y),left=ref(props.x);
const ready=computed(()=>generationItems(props.items).length);
let disposed=false,position=0;
async function place(){const token=++position;top.value=props.y;left.value=Math.min(props.x,innerWidth-210);await nextTick();if(!disposed&&token===position)top.value=Math.min(props.y,innerHeight-(element.value?.offsetHeight||0)-10);}
function choose(command:CatalogCommand){emit('command',command,[...props.items]);emit('close');}
function outside(event:MouseEvent){if(!(event.target instanceof Element)||!event.target.closest('#ctxMenu'))emit('close');}
function close(){emit('close');}
watch(()=>[props.x,props.y,props.items],place);
onMounted(()=>{void place();document.addEventListener('click',outside);window.addEventListener('blur',close);});
onUnmounted(()=>{disposed=true;++position;document.removeEventListener('click',outside);window.removeEventListener('blur',close);});
</script>
<template>
 <Teleport to="body"><div id="ctxMenu" ref="element" class="ctxMenu" :style="{display:'block',left:left+'px',top:top+'px'}">
  <button data-ctx="reload" @click="choose('reload')">↻ 重新读取（{{items.length}} 个）</button>
  <button data-ctx="ai" :disabled="!ready" @click="choose('ai')">✦ AI 生成标签</button>
  <button data-ctx="local" :disabled="!ready" @click="choose('local')">⚙ 规则生成标签</button>
  <button data-ctx="preview" :disabled="!ready||ready>10" @click="choose('preview')">👁 AI 预演（≤10）</button>
  <button data-ctx="finder" @click="choose('finder')">📂 {{platform==='macos'?'在 Finder 中显示':'在文件管理器中显示'}}</button>
  <button data-ctx="copy" @click="choose('copy')">⧉ 复制路径</button>
 </div></Teleport>
</template>
