<script setup lang="ts">
import {computed} from 'vue';
import type {MediaItem} from './contracts';
import {identityRows} from './inspector-presentation';
import InspectorStatus from './InspectorStatus.vue';
const props = defineProps<{item: MediaItem; platform: string}>();
const emit = defineEmits<{changed: []; reveal: []; copy: [string]; notice: [string]; failure: [unknown]}>();
const rows = computed(() => identityRows(props.item));
</script>
<template>
 <div class="card"><h3>文件与身份</h3><div class="grid"><div v-for="row in rows" :key="row.key" class="kv"><div class="k">{{row.key}}</div><div class="v" :data-i18n-user="row.user ? '' : undefined">{{row.value}}</div></div></div></div>
 <div class="card"><button class="btn" data-open-path @click="emit('reveal')">{{platform === 'macos' ? '在 Finder 中显示' : '在文件管理器中显示'}}</button> <button class="btn" data-copy="path" @click="emit('copy', item.path)">复制路径</button> <button class="btn" data-copy="imdb" @click="emit('copy', item.imdb)">复制 IMDb ID</button> <button class="btn" data-copy="title" @click="emit('copy', item.title)">复制标题</button></div>
 <InspectorStatus :item="item" @changed="emit('changed')" @notice="emit('notice', $event)" @failure="emit('failure', $event)" />
</template>
