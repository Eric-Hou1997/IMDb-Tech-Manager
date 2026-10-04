<script setup lang="ts">
import {reactive,onMounted,onUnmounted,watch} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import {listen,type UnlistenFn} from '@tauri-apps/api/event';
import type {AppError} from './contracts';
import LanguagePicker from './LanguagePicker.vue';
import {LifecycleSettings} from './lifecycle-settings';
const model=reactive(new LifecycleSettings(invoke));
const emit=defineEmits<{dirty:[boolean];busy:[boolean]}>();
let off:UnlistenFn|undefined,offClosing:UnlistenFn|undefined;
let reconnecting=false;
watch(()=>model.dirty,value=>emit('dirty',value));watch(()=>model.busy,value=>emit('busy',value));
async function reconnect(){
 if(model.busy||reconnecting||!model.alive)return;reconnecting=true;
 off?.();offClosing?.();off=offClosing=undefined;
 await model.read();if(!model.alive){reconnecting=false;return;}
 try {
  const a=await listen<AppError>('lifecycle-error',event=>{if(model.alive){model.report(event.payload);model.closing=false;}});
  if(!model.alive){a();return;}off=a;
  const b=await listen<boolean>('lifecycle-closing',event=>{if(model.alive)model.closing=event.payload;});
  if(!model.alive){b();off?.();off=undefined;return;}offClosing=b;
 }catch(e){off?.();offClosing?.();off=offClosing=undefined;model.report(e);}finally{reconnecting=false;}
}
onMounted(reconnect);
onUnmounted(()=>{model.dispose();off?.();offClosing?.();emit('busy',false);emit('dirty',false);});
</script>
<template><div class="appSettingsGrid"><LanguagePicker />
<span class="appSettingLabel">登录</span><label class="checkLabel appLoginControl"><input id="appAutoStart" v-model="model.settings.launch_at_login" type="checkbox" :disabled="model.busy||!model.baseline" @change="model.apply()"> 登录后启动 IMDb Tech Manager 应用</label>
</div><p v-if="model.error||model.status?.error" class="failure" role="alert">{{model.error||model.status?.error?.message}}</p></template>
