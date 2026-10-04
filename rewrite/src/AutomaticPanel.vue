<script setup lang="ts">
import {reactive,onMounted,onUnmounted,watch} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import {AutomaticSettingsModel} from './automatic-settings';
const model=reactive(new AutomaticSettingsModel(invoke));
const emit=defineEmits<{dirty:[boolean];busy:[boolean]}>();
watch(()=>model.dirty,value=>emit('dirty',value));watch(()=>model.busy,value=>emit('busy',value));
let timer:ReturnType<typeof setInterval>|undefined;
onMounted(async()=>{await model.refresh(true);if(model.alive)timer=setInterval(()=>void model.refresh(),2000);});
onUnmounted(()=>{model.dispose();if(timer)clearInterval(timer);emit('busy',false);emit('dirty',false);});
</script>
<template>
 <div class="rootSettingsFooter">
  <div class="rootPrimaryActions"><slot /><button id="toggleAgent" class="btn" :class="{yellow:model.status?.enabled}" :disabled="model.busy||!model.status" @click="model.apply(!model.status!.enabled,false)">{{model.status?.enabled?'关闭自动模式':'开启自动模式'}}</button></div>
  <label class="checkLabel autoStartLabel"><input id="autoStart" v-model="model.settings.on_app_start" type="checkbox" :disabled="model.busy||!model.status" @change="model.apply(model.status!.enabled)"> 启动后自动开启后台模式</label>
  <div class="autoModeHelp">每次启动 IMDb Tech Manager 应用后自动开启后台模式；不控制开机或登录启动。开启自动模式后，检测到新增 NFO 文件中含有 IMDb 号后，自动注入 Technical Specs 元数据。</div>
 </div>
 <p v-if="model.error" class="failure" role="alert">{{model.error}}</p>
</template>
