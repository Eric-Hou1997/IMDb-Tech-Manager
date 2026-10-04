<script setup lang="ts">
import { nextTick, onUnmounted, onMounted, ref, watch } from 'vue';
const props=withDefaults(defineProps<{ title: string; busy?: boolean; inactive?: boolean; visible?:boolean; id?:string }>(),{visible:true});
const emit = defineEmits<{ close: [] }>();
const box = ref<HTMLElement>();
let previous: HTMLElement | null = null;
function keydown(event: KeyboardEvent) {
  if (event.key !== 'Tab' || !box.value) return;
  const controls = Array.from(box.value.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], [tabindex="0"]')).filter(el => el.getClientRects().length);
  const first = controls[0], last = controls[controls.length - 1];
  if (!first) { event.preventDefault(); box.value.focus(); return; }
  if (event.shiftKey && (document.activeElement === first || document.activeElement === box.value)) { event.preventDefault(); last?.focus(); }
  else if (!event.shiftKey && document.activeElement === last) { event.preventDefault(); first.focus(); }
}
watch(()=>props.visible,async visible=>{if(visible){previous=document.activeElement instanceof HTMLElement?document.activeElement:null;await nextTick();box.value?.focus();}else if(previous?.isConnected)previous.focus();});
onMounted(async () => {
  previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  await nextTick();
  box.value?.focus();
});
onUnmounted(() => { if (previous?.isConnected) previous.focus(); });
</script>
<template>
  <Teleport to="body">
    <div :id="id" v-show="visible" class="modal open" @click.self="!busy && emit('close')" @keydown.esc.stop.prevent="!busy && emit('close')" @keydown="keydown" :inert="inactive||undefined" :aria-hidden="inactive||undefined">
      <div ref="box" class="modalBox product-dialog" role="dialog" aria-modal="true" :aria-label="title" tabindex="-1">
        <div class="modalHead"><h2>{{ title }}</h2><button class="btn" :disabled="busy" @click="emit('close')">关闭</button></div>
        <slot />
      </div>
    </div>
  </Teleport>
</template>
