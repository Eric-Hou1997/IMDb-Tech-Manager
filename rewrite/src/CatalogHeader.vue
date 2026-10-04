<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from 'vue';
import type { CatalogColumn, CatalogColumns, Sort } from './contracts';
import { columnDefinitions, defaultColumns, moveColumn, visibleColumns } from './catalog-layout';
const props = defineProps<{columns: CatalogColumns; widths: number[]; grid: Record<string,string>; sort: Sort; descending: boolean}>();
const emit = defineEmits<{change: [columns: CatalogColumns]; sort: [field: Sort]; resizing: [active: boolean]}>();
const keys = computed(() => visibleColumns(props.columns));
const menu = ref(false), host = ref<HTMLElement>();
let stop: (() => void) | null = null;
function toggle(key: CatalogColumn) {
 if(key==='title')return;
 const visible=props.columns.visible.includes(key)?props.columns.visible.filter(value=>value!==key):[...props.columns.visible,key];
 emit('change',{...props.columns,visible});
}
function reorder(event: DragEvent, target: CatalogColumn) {
 const source=event.dataTransfer?.getData('application/x-itm-column') as CatalogColumn;
 if(source && Object.prototype.hasOwnProperty.call(columnDefinitions,source))emit('change',moveColumn(props.columns,source,target));
}
function widthChange(index: number, delta: number, initial = props.widths) {
 if(index>=initial.length-1)return;
 const left=initial[index],right=initial[index+1];
 const difference=Math.max(1-left,Math.min(right-1,delta));
 const widths={...props.columns.widths};
 keys.value.forEach((key,i)=>{widths[key]=Math.round(Math.max(1,Math.min(4000,initial[i]+(i===index?difference:i===index+1?-difference:0))));});
 emit('change',{...props.columns,widths});
}
function resize(event: PointerEvent, index: number) {
 if(event.button!==0)return;event.stopPropagation();event.preventDefault();stop?.();
 const target=event.currentTarget as HTMLElement,id=event.pointerId,start=event.clientX,initial=[...props.widths];
 target.setPointerCapture(id);emit('resizing',true);
 const move=(next:PointerEvent)=>{if(next.pointerId===id)widthChange(index,next.clientX-start,initial);};
 const finish=(next:PointerEvent)=>{if(next.pointerId===id)cleanup();};
 const cleanup=()=>{target.removeEventListener('pointermove',move);target.removeEventListener('pointerup',finish);target.removeEventListener('pointercancel',finish);target.removeEventListener('lostpointercapture',finish);if(target.hasPointerCapture(id))target.releasePointerCapture(id);stop=null;emit('resizing',false);};
 stop=cleanup;target.addEventListener('pointermove',move);target.addEventListener('pointerup',finish);target.addEventListener('pointercancel',finish);target.addEventListener('lostpointercapture',finish);
}
function resizeKey(event: KeyboardEvent,index:number) {
 if(!['ArrowLeft','ArrowRight'].includes(event.key))return;event.stopPropagation();event.preventDefault();widthChange(index,(event.key==='ArrowRight'?1:-1)*(event.shiftKey?10:1));
}
function outside(event:PointerEvent){if(event.target instanceof Node&&!host.value?.contains(event.target))menu.value=false;}
onMounted(()=>document.addEventListener('pointerdown',outside));
onUnmounted(()=>{stop?.();document.removeEventListener('pointerdown',outside);});
</script>
<template>
 <div id="listHeader" ref="host" class="listHeader" :style="grid" role="row">
  <span></span><span></span>
  <span v-for="(key,index) in keys" :key="key" class="headCell" role="columnheader" :data-sort-field="key" :aria-sort="sort===columnDefinitions[key].sort ? descending?'descending':'ascending':undefined" :draggable="key!=='title'" @dragstart="$event.dataTransfer?.setData('application/x-itm-column',key)" @dragover.prevent @drop.prevent="reorder($event,key)">
   <button class="header-sort" @click="emit('sort',columnDefinitions[key].sort)"><span class="headerLabel">{{columnDefinitions[key].label}}</span><span v-if="sort===columnDefinitions[key].sort" class="sortArrow"><svg viewBox="0 0 15 15" aria-hidden="true"><path :d="descending?'M2.5 5.5 7.5 10.5 12.5 5.5':'M2.5 9.5 7.5 4.5 12.5 9.5'"/></svg></span></button>
   <span v-if="index<keys.length-1" class="resizeHandle" role="separator" aria-orientation="vertical" :aria-label="'调整 '+columnDefinitions[key].label+' 列宽'" :aria-valuenow="Math.round(widths[index])" tabindex="0" @pointerdown="resize($event,index)" @keydown="resizeKey($event,index)"></span>
  </span>
  <span class="columnTools"><button class="wrenchBtn" aria-label="选择与调整列表字段" :aria-expanded="menu" @click="menu=!menu"><svg viewBox="0 0 24 24" aria-hidden="true"><path d="M14.7 6.3a5 5 0 0 0-6.4 6.4L2.6 18.4a2.1 2.1 0 0 0 3 3l5.7-5.7a5 5 0 0 0 6.4-6.4l-3 3-3-3 3-3Z"/></svg></button>
   <div v-if="menu" class="columnMenu open" aria-label="字段设置" @keydown.esc.stop="menu=false"><label v-for="(definition,key) in columnDefinitions" :key="key"><input type="checkbox" :checked="columns.visible.includes(key)" :disabled="key==='title'" @change="toggle(key)">{{definition.label}}</label><label class="viewOption"><input type="checkbox" :checked="columns.compact" @change="emit('change',{...columns,compact:!columns.compact})">紧凑视图</label><button class="btn" @click="emit('change',defaultColumns())">恢复默认字段、顺序与宽度</button></div>
  </span>
 </div>
</template>
