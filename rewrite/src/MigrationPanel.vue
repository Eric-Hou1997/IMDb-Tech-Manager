<script setup lang="ts">
import { ref, shallowRef, watch } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { MigrationPlan, MigrationReceipt } from './contracts';
const props=defineProps<{product:'ITM'|'TCM'}>();
const plan=shallowRef<MigrationPlan|null>(null),receipt=shallowRef<MigrationReceipt|null>(null),busy=ref(false),error=ref('');
const adapterPage=ref(0);
watch(plan,()=>adapterPage.value=0);
function targetName(target:string){return target==='ai-runtime'?'AI 暂停与运行状态':target==='automatic'?'自动模式设置':target.startsWith('inspector:')?'问题确认与手动状态':target.startsWith('legacy-ai-failure:')?'旧 AI 失败队列':'AI 设置';}
function appliedSummary(targets:string[]){const counts=new Map<string,number>();for(const target of targets){const name=targetName(target);counts.set(name,(counts.get(name)||0)+1);}return [...counts].map(([name,count])=>`${name} ${count} 项`).join('、');}
async function inspect(kind:string){if(busy.value)return;busy.value=true;error.value='';try{plan.value=await invoke<MigrationPlan|null>('migration_plan',{id:crypto.randomUUID(),sourceKind:kind});receipt.value=null;}catch(e){error.value=JSON.stringify(e);}finally{busy.value=false;}}
async function apply(){if(!plan.value||busy.value)return;busy.value=true;error.value='';try{receipt.value=await invoke<MigrationReceipt>('migration_apply',{id:plan.value.id,fingerprint:plan.value.fingerprint});plan.value=null;}catch(e){error.value=JSON.stringify(e);}finally{busy.value=false;}}
function displayConfig(value:unknown){return JSON.stringify(value,(key,v)=>key==='credential_account'?undefined:v,2);}
</script>
<template>
<section aria-labelledby="migration-title">
<h2 id="migration-title">旧版数据迁移</h2>
<p>先检查所选目录和迁移范围，再导入。原数据保持原状，未完成的旧任务需重新预演和确认。</p>
<div class="actions">
<template v-if="props.product==='ITM'"><button :disabled="busy" @click="inspect('itm-manager')">选择旧 Manager 数据</button><button :disabled="busy" @click="inspect('itm-engine')">选择旧业务数据</button></template>
<template v-else><button :disabled="busy" @click="inspect('tcm-portable')">选择旧便携版目录</button><button :disabled="busy" @click="inspect('tcm-state')">选择旧 Emby 索引数据</button></template>
</div>
<p v-if="busy" role="status">正在核对或导入数据，请稍候…</p>
<article v-if="plan">
<h3>核对迁移计划</h3><p class="path">{{ plan.source }}</p><p>{{ plan.files.length }} 个文件，{{ plan.roots.length }} 个媒体根目录</p>
<ul><li v-for="root in plan.roots" :key="root.path" class="path">{{ root.path }} · {{ root.space || '待指定类型' }} · {{ root.state }}</li></ul>
<details><summary>文件清单</summary><ul><li v-for="file in plan.files" :key="file.relative">{{ file.relative }} · {{ file.bytes }} B</li></ul></details>
<ul><li v-for="warning in plan.warnings" :key="warning">{{ warning }}</li></ul>
<details v-if="plan.cache_entries.length"><summary>IMDb 缓存适配（{{plan.cache_entries.length}}）</summary><ul><li v-for="entry in plan.cache_entries" :key="entry.source">{{entry.source}} · {{entry.state}}<br>{{entry.detail}}</li></ul></details>
<p v-if="props.product==='ITM'&&plan.files.some(f=>f.category==='ai-cache')">旧 AI 缓存会在生成时按完整输入、配置和语言匹配，并重新校验结果。命中不发送请求；历史用量和费用单独展示。导入文件数量不代表全部缓存都能复用。</p>
<p v-if="plan.adapters.length">共 {{plan.adapters.length}} 项配置与历史状态；第 {{adapterPage+1}} / {{Math.ceil(plan.adapters.length/20)}} 页。</p>
<article v-for="adapter in plan.adapters.slice(adapterPage*20,(adapterPage+1)*20)" :key="adapter.target"><h4>{{adapter.source}} → {{targetName(adapter.target)}}</h4><p>{{adapter.before_hash?'将替换当前配置或状态；确认后生效。':'将导入为新的配置或状态。'}}</p><pre>{{displayConfig(adapter.value)}}</pre><ul><li v-for="warning in adapter.warnings" :key="warning">{{warning}}</li></ul></article>
<div v-if="plan.adapters.length>20"><button :disabled="busy||adapterPage===0" @click="adapterPage--">上一页迁移项</button><button :disabled="busy||(adapterPage+1)*20>=plan.adapters.length" @click="adapterPage++">下一页迁移项</button></div>
<button :disabled="busy" @click="apply">确认导入这份快照和所列配置</button><button :disabled="busy" @click="plan=null">取消</button>
</article>
<article v-if="receipt" role="status"><h3>快照已导入</h3><p>已保留 {{ receipt.imported_files }} 个文件的原始字节。业务适配与使用结果仍需逐项核对。</p><p v-if="receipt.applied_adapters.length">已应用：{{appliedSummary(receipt.applied_adapters)}}。请打开对应设置核对。</p><p v-if="receipt.pending_roots.length">{{ receipt.pending_roots.length }} 个目录需要恢复连接、配置映射或指定类型。</p><p>刷新媒体工作台以读取导入后的目录配置。</p></article>
<p v-if="receipt?.applied_cache_entries">已适配 {{receipt.applied_cache_entries}} 条 IMDb 缓存；保留原始时间，过期后按正常规则重新获取。</p>
<p v-if="error" role="alert">{{ error }}</p>
</section>
</template>
<style scoped>.path{overflow-wrap:anywhere}</style>
