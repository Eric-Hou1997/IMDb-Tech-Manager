<script setup lang="ts">
import {computed, onMounted, onUnmounted, ref, watch, nextTick} from 'vue';
import type {CatalogColumn, LibraryView, MediaItem, Space} from './contracts';
import {cell, movieChunk, statusClass, tvRows} from './catalog-presentation';
import CatalogItem from './CatalogItem.vue';
const props = defineProps<{space: Space; items: MediaItem[]; allTvItems: MediaItem[]; view: LibraryView; columns: CatalogColumn[]; grid: {gridTemplateColumns: string}; current: string | null; locale: string}>();
const emit = defineEmits<{inspect: [MediaItem, MouseEvent]; check: [MediaItem, boolean]; context:[MediaItem,MouseEvent]; expand: [string]; season: [string[], boolean]}>();
const count = ref(movieChunk), sentinel = ref<HTMLElement>();
const tree = computed(() => tvRows(props.items, props.allTvItems, props.view));
const more = computed(() => props.space === 'movie' && count.value < props.items.length);
let observer: IntersectionObserver | undefined, disposed = false, observation = 0;
async function observe() {
 const token = ++observation;
 observer?.disconnect(); observer = undefined;
 await nextTick();
 if (disposed || token !== observation || !more.value || !sentinel.value) return;
 if (!('IntersectionObserver' in window)) {count.value = props.items.length; return;}
 observer = new IntersectionObserver(entries => {
  if (!disposed && token === observation && entries.some(entry => entry.isIntersecting)) {count.value += movieChunk; void observe();}
 }, {root: sentinel.value.closest('.list')});
 observer.observe(sentinel.value);
}
watch(() => JSON.stringify([props.space, props.view.search, props.view.catalog_filter, props.view.media_level, props.view.sort, props.view.descending, props.columns]), () => {count.value = movieChunk; void observe();});
watch(() => props.items, observe);
onMounted(observe);
onUnmounted(() => {disposed = true; ++observation; observer?.disconnect();});
</script>
<template>
 <template v-if="space === 'movie'">
  <CatalogItem v-for="(item, index) in items.slice(0, count)" :key="item.id" :item="item" :columns="columns" :grid="grid" :selected="view.selected" :current="current" :stripe="index" :locale="locale" @inspect="(item, event) => emit('inspect', item, event)" @check="(item, checked) => emit('check', item, checked)" @context="(item,event)=>emit('context',item,event)" />
  <div v-if="more" id="chunkSentinel" ref="sentinel" class="chunkSentinel"></div>
 </template>
 <template v-else>
  <template v-for="(row, index) in tree.rows" :key="row.key">
   <CatalogItem v-if="row.kind === 'item' && row.item" :item="row.item" :columns="columns" :grid="grid" :selected="view.selected" :current="current" :stripe="index" :css="row.css" :locale="locale" @inspect="(item, event) => emit('inspect', item, event)" @check="(item, checked) => emit('check', item, checked)" @context="(item,event)=>emit('context',item,event)" />
   <div v-else class="row treeRow treeHeader" :class="[row.css, {stripe: index % 2}]" :style="grid" :data-tree="row.key" @click="emit('expand', row.key)">
    <span class="treeGridSpacer" aria-hidden="true"></span><span class="treeGridSpacer" aria-hidden="true"></span>
    <button class="treeToggle treeControl" :aria-label="(row.open ? '收起' : '展开') + ' ' + row.name">{{row.open ? '▾' : '▸'}}</button>
    <input v-if="row.kind === 'season'" class="seasonCheck treeControl" type="checkbox" data-season-check :data-season-paths="JSON.stringify(allTvItems.filter(item => row.members.includes(item.id)).map(item => item.path))" aria-label="选择本季所有单集" title="选择本季所有单集" :checked="row.members.length > 0 && row.members.every(id => view.selected.includes(id))" :indeterminate="row.members.some(id => view.selected.includes(id)) && !row.members.every(id => view.selected.includes(id))" @click.stop @change="emit('season', row.members, ($event.target as HTMLInputElement).checked)">
    <span class="stateDot treeControl" :class="row.item ? statusClass(row.item) : ''"></span>
    <div v-for="key in columns" :key="key" class="cell" :class="{titleCell: key === 'title'}" :data-field="key">
     <template v-if="key === 'title'"><div class="title" :data-i18n-user="row.kind === 'show' ? '' : undefined">{{row.name}}</div><div class="sub">{{row.subtitle}}</div></template>
     <template v-else>{{row.item ? cell(row.item, key, locale) : ''}}</template>
    </div>
    <span class="toolSpacer" aria-hidden="true"></span>
   </div>
  </template>
 </template>
</template>
