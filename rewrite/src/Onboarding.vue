<script setup lang="ts">
import {onUnmounted,ref} from 'vue';
import {invoke} from '@tauri-apps/api/core';
import type {Configuration,LibraryRoot,OnboardingInfo,Space} from './contracts';
const props=defineProps<{configuration:Configuration;info:OnboardingInfo}>();
const emit=defineEmits<{saved:[value:Configuration];skip:[];notice:[value:string]}>();
const roots=ref<LibraryRoot[]>([]),paths=ref<Record<Space,string>>({movie:'',tv:''}),busy=ref(false);
let disposed=false,pending:{id:string;configuration:Configuration}|null=null;
onUnmounted(()=>{disposed=true;});
function report(e:unknown){if(disposed)return;const v=e as {code?:string;message?:string;path?:string};emit('notice',v?.message?[v.code,v.message,v.path].filter(Boolean).join(' · '):String(e));}
function add(space:Space,path=paths.value[space]){
 if(busy.value||disposed)return;path=path.trim();if(!path)return;
 if(!roots.value.some(root=>root.space===space&&root.path===path))roots.value.push({id:crypto.randomUUID(),space,path});
 paths.value[space]='';pending=null;
}
function remove(id:string){if(busy.value||disposed)return;roots.value=roots.value.filter(root=>root.id!==id);pending=null;}
async function choose(space:Space){
 if(busy.value||disposed)return;busy.value=true;let path:string|null=null;
 try{path=await invoke<string|null>('choose_library_root',{space});}catch(e){report(e);}finally{busy.value=false;}
 if(path&&!disposed)add(space,path);
}
async function save(){
 if(busy.value||disposed)return;
 if(!roots.value.length){emit('notice','至少确认一个电影或电视剧目录');return;}
 busy.value=true;
 if(!pending)pending={id:crypto.randomUUID(),configuration:{...props.configuration,roots:JSON.parse(JSON.stringify(roots.value))}};
 try{const saved=await invoke<Configuration>('save_library_roots',pending);if(!disposed){emit('saved',saved);emit('notice','资料库已确认；现在可以手动启动后台模式');}}
 catch(e){report(e);}finally{busy.value=false;}
}
defineExpose({roots,paths,add,remove,choose,save});
</script>
<template>
 <Teleport to="body">
  <div class="modal open" id="onboardingModal"><div class="modalBox"><div class="modalHead"><h2>首次运行：确认资料库</h2></div><p class="muted">IMDb Tech Manager 不会在确认前扫描、抓取、写入 NFO 或启动后台任务。TMM 路径仅作为候选，请分别确认电影和电视剧目录。</p>
   <div class="rootColumns"><div v-for="space in (['movie','tv'] as const)" :key="space" class="rootGroup"><h3>{{space==='movie'?'电影资料库':'电视剧资料库'}}</h3><div :id="space==='movie'?'onboardingMovies':'onboardingTV'"><div v-for="(root,index) in roots.filter(root=>root.space===space)" :key="root.id" class="root"><span data-i18n-user>{{root.path}}</span><button class="btn danger" :data-onboard-remove="space==='movie'?'movies':'tv'" :data-onboard-index="index" :disabled="busy" @click="remove(root.id)">移除</button></div><div v-if="!roots.some(root=>root.space===space)" class="muted">尚未确认</div></div><input :id="space==='movie'?'onboardingMovieInput':'onboardingTVInput'" v-model="paths[space]" class="input" :disabled="busy" :placeholder="space==='movie'?'选择或输入电影根目录':'选择或输入电视剧根目录'"><button class="btn" :data-onboard-choose="space==='movie'?'movies':'tv'" :disabled="busy" @click="choose(space)">选择文件夹</button><button class="btn" :data-onboard-add="space==='movie'?'movies':'tv'" :disabled="busy" @click="add(space)">添加</button></div></div>
   <div class="card"><strong>TMM 候选（只读）</strong><div id="onboardingCandidates" class="muted"><div v-for="candidate in info.candidates" :key="candidate.path" class="root"><span><span data-i18n-user>{{candidate.path}}</span> <span class="muted">（建议：{{candidate.suggested_space==='tv'?'电视剧':candidate.suggested_space==='movies'?'电影':'待分类'}} · {{candidate.online?'在线':'离线'}}）</span></span><button class="btn" :data-candidate-space="candidate.suggested_space==='tv'?'tv':'movies'" :data-candidate-path="candidate.path" :disabled="busy" @click="add(candidate.suggested_space==='tv'?'tv':'movie',candidate.path)">采用</button></div><template v-if="!info.candidates.length">没有可用的 TMM 候选，请手动选择目录。</template></div></div>
   <div style="margin-top:12px"><button class="btn blue" id="onboardingSave" :disabled="busy" @click="save">确认并保存</button><button class="btn" id="onboardingSkip" :disabled="busy" @click="emit('skip')">稍后设置</button></div>
  </div></div>
 </Teleport>
</template>
