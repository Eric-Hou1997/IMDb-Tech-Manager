<script setup lang="ts">
import { ref, computed, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { InstallationIdentity, UpdateProgress } from './contracts';
const identity=ref<InstallationIdentity|null>(null),progress=ref<UpdateProgress|null>(null),busy=ref(false),error=ref(''),dismissed=ref(false);
let unlisten:UnlistenFn|undefined;let alive=true;
const failure=(value:unknown)=>{const e=value as {code?:string;message?:string};return e?.code==='update-package-manager'?'请从 GitHub Release 下载对应 DEB/RPM 包，并使用系统包管理器升级。':e?.message||String(value);};
const installText=computed(()=>identity.value?.os==='windows'?'将下载并验证新版；软件会退出，运行对应架构的安装程序，然后尝试重新启动。':identity.value?.channel==='appimage'?'将下载并验证新版；软件会退出、替换当前 AppImage，然后尝试重新启动。':'将下载并验证新版；软件会退出、自动替换当前');
async function check(){if(busy.value)return;busy.value=true;error.value='';dismissed.value=false;try{progress.value=await invoke<UpdateProgress>('update_check',{id:crypto.randomUUID()});}catch(e){error.value=failure(e);}finally{busy.value=false;}}
async function install(){if(!progress.value||busy.value)return;busy.value=true;error.value='';try{progress.value=await invoke<UpdateProgress>('update_install',{id:progress.value.operation_id});}catch(e){error.value=failure(e);}finally{busy.value=false;}}
onMounted(async()=>{try{identity.value=await invoke<InstallationIdentity>('update_identity');progress.value=await invoke<UpdateProgress|null>('update_status');const stop=await listen<UpdateProgress>('update-progress',event=>{if(alive)progress.value=event.payload;});if(alive)unlisten=stop;else stop();}catch(e){error.value=failure(e);}});
onUnmounted(()=>{alive=false;unlisten?.();});
const labels:Record<string,string>={'checking':'检查更新','available':'可更新','up-to-date':'已是最新版本','downloading':'正在下载','verifying':'正在验签','installing':'正在安装','installed-awaiting-health':'等待新版启动验证','verified':'更新已验证','failed':'更新失败','cancelled':'已取消','interrupted':'上次更新已中断','recovery-required':'需要恢复检查'};
</script>
<template>
 <div class="aboutUpdate"><div><strong>软件更新</strong><span class="aboutUpdateState" role="status">{{progress?labels[progress.phase]||progress.phase:'尚未检查更新'}}<template v-if="progress?.version"> · {{progress.version}}</template><template v-if="progress?.phase==='downloading'"> · {{progress.downloaded}} / {{progress.total||'?'}} B</template></span></div><button class="btn ghostYellow" :disabled="busy" @click="check">检查更新</button></div>
 <div v-if="progress?.phase==='available'&&!dismissed" class="aboutInstall"><strong>最新版本可用</strong><p>{{installText}}<template v-if="identity?.os!=='windows'&&identity?.channel!=='appimage'"> <code>IMDb Tech Manager.app</code>，然后尝试重新启动。</template></p><div><button class="btn" :disabled="busy" @click="dismissed=true">取消</button><button class="btn yellow" :disabled="busy" @click="install">继续下载并安装</button></div></div>
 <p v-if="error||progress?.error" class="failure" role="alert">{{error||failure(progress?.error)}}</p>
</template>
