<script setup lang="ts">
import type { CatalogColumn, MediaItem } from './contracts';
import { cell, statusClass, statusLabel, subtitle } from './catalog-presentation';
defineProps<{item: MediaItem; columns: CatalogColumn[]; grid: {gridTemplateColumns: string}; selected: string[]; current: string | null; stripe: number; css?: string; locale: string}>();
const emit = defineEmits<{inspect: [MediaItem, MouseEvent]; check: [MediaItem, boolean]; context:[MediaItem,MouseEvent]}>();
</script>
<template>
 <div class="row" :class="[css, {'treeRow': !!css, stripe: stripe % 2, current: item.id === current}]" :style="grid" :data-path="item.path" @click="emit('inspect', item, $event)" @contextmenu.prevent="emit('context',item,$event)">
  <template v-if="css"><span class="treeGridSpacer" aria-hidden="true"></span><span class="treeGridSpacer" aria-hidden="true"></span></template>
  <input type="checkbox" :class="css ? 'treeCheck treeControl' : undefined" :data-check="item.path" :checked="selected.includes(item.id)" @click.stop @change="emit('check', item, ($event.target as HTMLInputElement).checked)">
  <span class="stateDot" :class="[css ? 'treeControl' : undefined, statusClass(item)]" :title="statusLabel(item)"></span>
  <div v-for="key in columns" :key="key" class="cell" :class="{titleCell: key === 'title', rowStatus: key === 'tag_status'}" :data-field="key">
   <template v-if="key === 'title'"><div class="title" data-i18n-user>{{cell(item, key, locale)}}</div><div class="sub" data-i18n-user>{{subtitle(item)}}</div></template>
   <template v-else>{{cell(item, key, locale)}}</template>
  </div>
  <span class="toolSpacer" aria-hidden="true"></span>
 </div>
</template>
