<script setup lang="ts">
import { ref } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import LibraryPanel from './LibraryPanel.vue';
import './styles/baseline.css';
import './styles/product.css';
const startupError = ref('');
let ready = false, confirming = false;
async function frontendReady() {
 if(ready || confirming)return;
 confirming=true;
 try { await invoke('frontend_ready'); ready=true;startupError.value=''; }
 catch(error){startupError.value=String(error);}
 finally {confirming=false;}
}
</script>
<template>
 <LibraryPanel @ready="frontendReady" />
 <div v-if="startupError" class="startup-error" role="alert">启动状态未确认：{{startupError}} <button class="btn" @click="frontendReady">重试</button></div>
</template>
