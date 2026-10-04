<script setup lang="ts">

import { computed, provide, shallowRef, onMounted, onUnmounted, reactive, ref, watch, nextTick } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import ProductDialog from './ProductDialog.vue';
import CatalogContext from './CatalogContext.vue';
import PreviewResults from './PreviewResults.vue';
import TaskCenter from './TaskCenter.vue';
import {Languages,languageKey,translateDocument,bindPresentation} from './baseline-language';
import {currentTask,resumableAI,historyLog} from './task-center';
import {AiRecovery} from './ai-recovery';
import {contextItems,generationItems,type CatalogCommand} from './catalog-context';
import {BatchSubmission} from './batch-submission';
import {scopeSummary,generationCount} from './scope-presentation';
import type {PreviewEntry} from './preview-presentation';
import {PreviewAdoptionSubmission} from './preview-adoption';
import RootSettings from './RootSettings.vue';
import Onboarding from './Onboarding.vue';
import CacheSettings from './CacheSettings.vue';
const lifecycleDirty=ref(false),lifecycleBusy=ref(false),aiSettingsDirty=ref(false),aiSettingsBusy=ref(false),automaticDirty=ref(false),automaticBusy=ref(false);
const cacheModalOpen=ref(false),cacheBusy=ref(false),cacheDirty=ref(false);
const rootsDirty=ref(false),rootsBusy=ref(false);
function closeSettings(){settingsOpen.value=false;}

import AutomaticPanel from './AutomaticPanel.vue';
import LifecyclePanel from './LifecyclePanel.vue';
import AiGenerator from './AiGenerator.vue';
import AboutPanel from './AboutPanel.vue';
import { defaultPresentation, splitPixels, taskPixels } from './window-state';
import CatalogHeader from './CatalogHeader.vue';
import CatalogList from './CatalogList.vue';
import {clearFilters, pillClass, sortItems, statusLabel, tvRows} from './catalog-presentation';
import {syncProductLayout} from './product-layout';
import {columnDefinitions, defaultColumns, columnWidths, visibleColumns} from './catalog-layout';
import type {CatalogColumn, CatalogColumns, LegacyRoot, OnboardingInfo} from './contracts';
import logo from './assets/ITM_logo_letter_only.png';
const emit = defineEmits<{ ready: [] }>();
const presentation = reactive(defaultPresentation());
const loading = ref(true);
const startupFailed = ref(false);
let initializing = false;
const dragging = ref(false);
const settingsOpen = ref(false), settingsMounted=ref(false);
watch(settingsOpen,value=>{if(value)settingsMounted.value=true;});
const totals = reactive({movie: 0, tv: 0});
const dimensions = reactive({width: 1000, height: 760, toolbar: 95, library: 520, padding: 12});
const workspace = ref<HTMLElement>();
const productWindow = ref<HTMLElement>();
const libraryTools = ref<HTMLElement>();
const windowStyle = computed(() => ({
 '--left': `${splitPixels(dimensions.width, presentation.split_basis_points)}px`,
 '--task-h': `${taskPixels(dimensions.height, presentation.task_height)}px`,
 '--catalog-preheader-height': `${dimensions.toolbar}px`,
}));

import InspectorOverview from './InspectorOverview.vue';
import {copyText} from './inspector-presentation';
import InspectorSpecs from './InspectorSpecs.vue';
import InspectorTags from './InspectorTags.vue';
import {originalConfirm, originalPrompt} from './original-dialogs';
import {InspectorWrite, type InspectorEdit} from './inspector-write';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import type { AppError, CatalogPage, Configuration, MediaItem, Space, Task, TaskState, LibraryView, UiState, UiReceipt, WritePreview, Action, BatchEngine, BatchMode, OperationResult, TaskJob, AiRuntime } from './contracts';

const pendingLegacy = ref<LegacyRoot[]>([]);
const onboardingInfo = ref<OnboardingInfo|null>(null);
async function showOnboarding(){
 try{const info=await invoke<OnboardingInfo>('onboarding_info');if(!disposed&&info.onboarding_required)onboardingInfo.value=info;}
 catch(e){if(!disposed)report(e);}
}
const configuration = ref<Configuration>({ revision: 0, locale: 'zh-CN', roots: [] });
const space = ref<Space>('movie');
const emptyView=():LibraryView=>({search:'',errors:false,issues:false,lifecycle:'',roots:[],selected:[],expanded:[],offset:0,sort:'title',descending:false});
const views=reactive<{movie:LibraryView;tv:LibraryView}>({movie:emptyView(),tv:emptyView()});
let savedRevision=0, stateReady=false, saving=false;
let queuedState:UiState|null=null;
let failedSave:{id:string;value:UiState}|null=null;
const saveError=ref('');
async function flushState(){
 if(saving||failedSave)return;saving=true;
 try{while(queuedState){const value=queuedState;queuedState=null;value.revision=savedRevision;const id=crypto.randomUUID();
  try{const receipt=await invoke<UiReceipt>('save_ui_state',{id,value});savedRevision=receipt.revision;saveError.value='';}
  catch(e){failedSave={id,value};saveError.value=typeof e==='object'?JSON.stringify(e):String(e);break;}
 }}finally{saving=false;}
}
async function retryState(){if(!failedSave)return;try{const receipt=await invoke<UiReceipt>('save_ui_state',failedSave);savedRevision=receipt.revision;failedSave=null;saveError.value='';await flushState();}catch(e){saveError.value=JSON.stringify(e);}}
watch(()=>({space:space.value,movie:views.movie,tv:views.tv,presentation,dragging:dragging.value}),()=>{
 if(!stateReady||dragging.value)return;
 queuedState=JSON.parse(JSON.stringify({revision:savedRevision,active_space:space.value,movie:views.movie,tv:views.tv,presentation}));void flushState();
},{deep:true});

watch(()=>configuration.value.roots,values=>{
 const ids=new Set(values.map(root=>root.id));
 for(const current of [views.movie,views.tv])current.roots=current.roots.filter(id=>ids.has(id));
},{deep:true});
const view = computed(() => views[space.value]);
const roots = computed(() => configuration.value.roots.filter(r => r.space === space.value));
const page = ref<CatalogPage>({ total: 0, items: [] });
const allTvItems = shallowRef<MediaItem[]>([]);
const tvTree = computed(() => tvRows(page.value.items, allTvItems.value, views.tv));
const tasks = shallowRef<Task[]>([]);
const job=shallowRef<TaskJob|null>(null),aiRuntime=shallowRef<AiRuntime|null>(null),aiFailures=shallowRef<MediaItem[]>([]);
const historyText=ref('暂无历史任务'),historyFixed=ref(false);
let historyToken=0;
async function taskTab(history:boolean){
 presentation.task_history=history;const token=++historyToken;
 if(!history)return;
 try{const jobs=await invoke<TaskJob[]>('job_history');if(!disposed&&token===historyToken){historyText.value=historyLog(jobs);historyFixed.value=!!jobs.length;}}
 catch(e){if(!disposed&&token===historyToken)report(e);}
}
const aiRecovery=new AiRecovery(invoke);
const languages=reactive(new Languages(invoke,showNotice));
provide(languageKey,languages);
let translator:ReturnType<typeof translateDocument>|undefined,unbindPresentation:(()=>void)|undefined,languageEvents:UnlistenFn|undefined;
watch(()=>languages.snapshot,()=>{if(languages.ready)translator?.refresh();},{deep:true,flush:"post"});
onMounted(()=>{translator=translateDocument(document,languages);unbindPresentation=bindPresentation(languages);});
const detail = ref<MediaItem | null>(null);
const error = ref('');
const notice = ref(''), noticeUndo=ref(false);
const lastWrite=shallowRef<WritePreview|null>(null);
const inspectorWriter=new InspectorWrite(invoke);
const nativePlatform = /Mac/i.test(navigator.platform) ? 'macos' : 'other';
let noticeTimer: ReturnType<typeof setTimeout> | undefined;
function showNotice(message: string, undo=false) {clearTimeout(noticeTimer); noticeUndo.value=undo; notice.value = message; noticeTimer = setTimeout(() => {notice.value = '';}, 5500);}
async function copy(value: string) {try {await copyText(value,nativePlatform); showNotice('已复制');} catch(e) {report(e);}}
async function inspectorChanged() {await restoreCurrent(); await refresh();}
const busy = ref(false);
let unlisten: UnlistenFn | undefined;
let failures: UnlistenFn | undefined;
let cacheFailures: UnlistenFn | undefined;
let configEvents:UnlistenFn|undefined;
let aiEvents:UnlistenFn|undefined,aiSettingsEvents:UnlistenFn|undefined, runtimeEvents:UnlistenFn|undefined;
let disposed = false;
let refreshToken = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
function report(e: unknown) {
  if (e && typeof e === 'object' && 'code' in e) {
    const failure = e as AppError;
    error.value = `${failure.code}：${failure.message}${failure.path ? '\n' + failure.path : ''}`;
  } else error.value = String(e);
}
async function refresh(strict = false) {
  const token = ++refreshToken;
  try {
    const requestedSpace = space.value, snapshot = JSON.parse(JSON.stringify(view.value));
    const result = await invoke<CatalogPage>('browse', {space: requestedSpace, view: snapshot, complete: true});
    const all = await invoke<CatalogPage>('browse', {space: requestedSpace, view: emptyView(), complete: true});
    if (!disposed && token === refreshToken) {
      page.value = {...result, items: sortItems(result.items, snapshot)};
      allTvItems.value = all.items;
      const inspected = detail.value && all.items.find(item=>item.id===detail.value?.id&&item.source_hash===detail.value?.source_hash);
      // The cached NFO layer is unchanged. Refresh only the read-time hints,
      // retaining Inspector facts and any unsaved spec/tag drafts.
      if(inspected)detail.value={...detail.value!,inspection:inspected.inspection};
      const ids = new Set(all.items.map(item=>item.id));
      views[requestedSpace].selected=views[requestedSpace].selected.filter(id=>ids.has(id));
    }
    const [history,runtime,failures]=await Promise.all([invoke<Task[]>('task_history'),invoke<AiRuntime>('ai_runtime'),invoke<MediaItem[]>('ai_failure_items')]);
    const current=currentTask(history), currentJob=current?await invoke<TaskJob>('task_job',{id:current.id}):null;
    if (!disposed && token === refreshToken) {tasks.value = history;job.value=currentJob;aiRuntime.value=runtime;aiFailures.value=failures;}
    if (!disposed && token === refreshToken) checkPreview(history);
    if (!disposed && token === refreshToken) checkAdoptions(history);
  } catch (e) { if (strict) throw e; if (!disposed && token === refreshToken) report(e); }
}
function scheduleRefresh() { clearTimeout(timer); timer = setTimeout(() => { void refresh(); void refreshTotals(); }, 150); }
async function action(work: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true; error.value = '';
  try { await work(); } catch (e) { report(e); } finally { busy.value = false; }
}
async function addRoot(target: Space = space.value) {
  await action(async () => {
    const result = await invoke<Configuration | null>('add_library_root', { space: target, operationId: crypto.randomUUID() });
    if (result) configuration.value = result;
  });
}
async function scan() {
  await action(async () => {
    await invoke<Task>('scan_library', { request: { operation_id: crypto.randomUUID(), space: space.value, root_ids: view.value.roots.length ? [...view.value.roots] : roots.value.map(root => root.id) } });
    await refresh();
  });
}
async function control(task: Task, state: TaskState) {
  await action(async () => {
    if(state==='requested'&&task.batch&&!task.batch.approved){
     if(!await originalConfirm(`继续提交上次未确认启动的任务？\n范围：该任务已保存的 ${task.batch.total} 个 NFO\n类型：${task.batch.engine==='ai'?'AI':task.batch.engine==='rules'?'规则':'Technical Specs'}`)||disposed)return;
     await invoke<Task>('approve_batch_scope',{id:task.id,reviewedHash:task.batch.plan_hash});
     if(task.batch.mode==='preview')pendingPreviews.add(task.id);
    }else await invoke<Task>('task_control', { request: { operation_id: crypto.randomUUID(), task_id: task.id, state } });
    presentation.task_open=true;showNotice('任务已提交');
    await refresh();
  });
}
async function inspect(item: MediaItem) { await inspectId(item.id); }
function toggleExpanded(id:string){const values=new Set(views.tv.expanded);if(values.has(id))values.delete(id);else values.add(id);views.tv.expanded=[...values];}
function checkItem(item: MediaItem, checked: boolean) {
 const selected = new Set(views[item.space].selected);
 if (checked) selected.add(item.id); else selected.delete(item.id);
 views[item.space].selected = [...selected]; lastClicked[item.space] = item.id;
}
function selectSeason(ids: string[], checked: boolean) {
 const selected = new Set(views.tv.selected);
 for (const id of ids) {if (checked) selected.add(id); else selected.delete(id);}
 views.tv.selected = [...selected];
}
function switchSpace(next: Space) { if(next === space.value) return; space.value = next; detail.value = null; ++inspectionToken; void refresh(); void restoreCurrent(); }
function releaseListeners() {
 unlisten?.(); failures?.(); cacheFailures?.(); configEvents?.(); aiEvents?.();aiSettingsEvents?.();runtimeEvents?.();languageEvents?.();languageEvents=undefined;
 unlisten = failures = cacheFailures = configEvents = undefined;
 aiEvents=aiSettingsEvents=runtimeEvents=undefined;
}
async function initialize() {
 if(initializing || disposed)return;
 initializing=true;loading.value=true;startupFailed.value=false;error.value='';stateReady=false;
 releaseListeners();
 try {
  const config = await invoke<Configuration>('configuration');
  const pending = await invoke<LegacyRoot[]>('pending_legacy_roots');
  await languages.read();
  if(!languages.ready)throw new Error("界面语言尚未读取，请重新连接");
  const state = await invoke<UiState>('ui_state');
  if(disposed)return;
  configuration.value=config;pendingLegacy.value=pending;savedRevision=state.revision;space.value=state.active_space;
  Object.assign(views.movie,emptyView(),state.movie);Object.assign(views.tv,emptyView(),state.tv);
  views.movie.offset=0;views.tv.offset=0;
  Object.assign(presentation,defaultPresentation(),state.presentation ?? {});
  await nextTick();
  if(disposed)return;
  const off = await listen<Task>('task-changed', scheduleRefresh);
  if(disposed){off();return;}unlisten=off;
  const offFailure = await listen<AppError>('worker-failed', e => report(e.payload));
  if(disposed){offFailure();return;}failures=offFailure;
  const offCache=await listen<AppError>('cache-maintenance-failed',e=>report(e.payload));
  if(disposed){offCache();return;}cacheFailures=offCache;
  const offConfig=await listen<Configuration>('configuration-changed',e=>{
   if(!disposed){const changedRoots=JSON.stringify(configuration.value.roots)!==JSON.stringify(e.payload.roots);configuration.value=e.payload;if(changedRoots)detail.value=null;scheduleRefresh();void languages.read();}
  });
  if(disposed){offConfig();return;}configEvents=offConfig;
  const offAI=await listen('ai-changed',scheduleRefresh);
  if(disposed){offAI();return;}aiEvents=offAI;
  const offAISettings=await listen('ai-settings-changed',scheduleRefresh);
  if(disposed){offAISettings();return;}aiSettingsEvents=offAISettings;
  const offRuntime=await listen('ai-runtime-changed',scheduleRefresh);
  if(disposed){offRuntime();return;}runtimeEvents=offRuntime;
  const offLanguage=await listen("language-state-changed",()=>void languages.read());
  if(disposed){offLanguage();return;}languageEvents=offLanguage;
  await refresh(true);await refreshTotals(true);await restoreCurrent();
  if(presentation.task_history)await taskTab(true);
  if(disposed)return;
  stateReady=true;loading.value=false;await showOnboarding();await nextTick();if(!disposed){translator?.refresh();emit('ready');void languages.restore();}
 } catch(e) {
  releaseListeners();clearTimeout(timer);++refreshToken;++totalToken;
  if(!disposed){report(e);startupFailed.value=true;loading.value=false;}
 } finally {initializing=false;}
}
onMounted(initialize);
onUnmounted(() => { disposed = true; languages.dispose();translator?.dispose();unbindPresentation?.(); ++refreshToken; ++totalToken; ++inspectionToken; ++previewToken; ++historyToken; clearTimeout(timer); clearTimeout(noticeTimer); releaseListeners(); });

let inspectionToken = 0;
let totalToken = 0;
const lastClicked: Record<Space, string | null> = {movie: null, tv: null};
const scanning = computed(() => tasks.value.some(task => !task.batch && ['requested','running'].includes(task.state)));
const running = computed(() => tasks.value.some(task => ['requested','running'].includes(task.state)));
const actionIds = computed(() => view.value.selected.length ? [...view.value.selected] : detail.value ? [detail.value.id] : []);
const scopeInfo=computed(()=>scopeSummary(actionIds.value.map(id=>detail.value?.id===id?detail.value:allTvItems.value.find(item=>item.id===id)).filter((item):item is MediaItem=>!!item),!!view.value.selected.length));
const columns = computed(() => (space.value==='movie' ? presentation.movie_columns : presentation.tv_columns) ?? defaultColumns());
const columnKeys = computed(()=>visibleColumns(columns.value));
const widths = computed(()=>columnWidths(columns.value,Math.max(0,dimensions.library-dimensions.padding-26-9-34-2-8*(columnKeys.value.length+2))));
const gridStyle = computed(()=>({gridTemplateColumns:['26px','9px',...widths.value.map(width=>`${width.toFixed(2)}px`),'34px'].join(' ')}));
function saveColumns(value:CatalogColumns) {if(space.value==='movie')presentation.movie_columns=value;else presentation.tv_columns=value;}
const filterOptions = [ ['all','全部状态'], ['non-optimal','非最佳（未达 AI 完成）'], ['ai','AI 完成'], ['local','规则完成'], ['ready','可生成'], ['nospec','缺 Spec'], ['error','有问题'], ['missing-imdb','缺 IMDb ID'], ['xml-error','XML 错误'], ['clear-all','清除全部筛选'] ];
const selectedFilter = computed<NonNullable<LibraryView['catalog_filter']> | 'clear-all'>({get: () => view.value.catalog_filter ?? 'all', set: value => {if(value==='clear-all'){clearFilters(view.value);scheduleRefresh();return;}view.value.catalog_filter = value; view.value.lifecycle = ''; view.value.issues = false; view.value.errors = false; view.value.offset = 0; scheduleRefresh();}});
const level = computed({get: () => views.tv.media_level ?? 'all', set: value => {views.tv.media_level = value; views.tv.offset=0; scheduleRefresh();}});
async function refreshTotals(strict = false) {
 const token = ++totalToken;
 try {
  const [movie, tv] = await Promise.all([invoke<CatalogPage>('browse',{space:'movie',view:emptyView()}),invoke<CatalogPage>('browse',{space:'tv',view:emptyView()})]);
  if (!disposed && token === totalToken) {totals.movie=movie.total;totals.tv=tv.total;}
 } catch(e) {if(strict)throw e;if(!disposed && token===totalToken)report(e);}
}
async function inspectId(id: string) {
 const token = ++inspectionToken, requestedSpace = space.value;
 try {
  const item = await invoke<MediaItem>('inspector',{id});
  if(disposed || token !== inspectionToken || requestedSpace !== space.value) return;
  if(item.space !== requestedSpace) throw Error('当前检查项不属于此媒体空间');
  detail.value = item;
  if(requestedSpace==='movie')presentation.current_movie=id;else presentation.current_tv=id;
 } catch(e) {if(!disposed && token===inspectionToken)report(e);}
}
async function restoreCurrent() {
 const id = space.value==='movie' ? presentation.current_movie : presentation.current_tv;
 if(id) await inspectId(id);
}
function selectFiltered(invert = false) {
 const selected = new Set(view.value.selected);
 for (const item of page.value.items) {
  if (invert && selected.has(item.id)) selected.delete(item.id); else selected.add(item.id);
 }
 view.value.selected = [...selected];
}

function clickItem(item: MediaItem, event: MouseEvent) {
 const previous = lastClicked[space.value];
 if(event.shiftKey && previous && previous !== item.id) {
  const ids = space.value==='movie' ? page.value.items.map(row=>row.id) : tvTree.value.order;
  const from=ids.indexOf(previous),to=ids.indexOf(item.id);
  if(from>=0 && to>=0){view.value.selected=[...new Set([...view.value.selected,...ids.slice(Math.min(from,to),Math.max(from,to)+1)])];return;}
 }
 if(event.metaKey || event.ctrlKey) {const selected=new Set(view.value.selected);if(selected.has(item.id))selected.delete(item.id);else selected.add(item.id);view.value.selected=[...selected];}
 else void inspect(item);
 lastClicked[space.value]=item.id;
}
function sortBy(field: LibraryView['sort']) {
 if(view.value.sort===field)view.value.descending=!view.value.descending;else{view.value.sort=field;view.value.descending=false;}
 view.value.offset=0;scheduleRefresh();
}
function arrow(field: LibraryView['sort']) {return view.value.sort===field ? view.value.descending ? '⌄' : '⌃' : '';}
const generationEngine = ref<'ai'|'local-rules'>('ai');
const contextMenu=shallowRef<{items:MediaItem[];x:number;y:number}|null>(null);
function showContext(item:MediaItem,event:MouseEvent){contextMenu.value={items:contextItems(item,view.value.selected,allTvItems.value),x:event.clientX,y:event.clientY};}
watch(space,()=>{contextMenu.value=null;});
async function contextCommand(command:CatalogCommand,items:MediaItem[]){
 if(command==='copy'){try{await copyText(items.map(item=>item.path).join('\n'),nativePlatform);showNotice('已复制路径');}catch(e){report(e);}return;}
 if(command==='finder'){if(items[0])await action(async()=>{await invoke('reveal_item',{id:items[0]!.id});});return;}
 if(command==='reload'){await reloadItems(items.map(item=>item.id));return;}
 await submitBatch(command==='local'?'rules':'ai',command==='preview'?'preview':'generate',generationItems(items).map(item=>item.id),false);
}
async function reloadItems(ids:string[]){await action(async()=>{
 const unique=[...new Set(ids)];if(!unique.length)return;showNotice(`正在重新读取 ${unique.length} 个 NFO…`);
 const current=detail.value?.id;
 for(const id of unique){const item=await invoke<MediaItem>('inspector',{id});if(!disposed&&detail.value?.id===id)detail.value=item;}
 await refresh();await refreshTotals();
 if(!disposed&&detail.value?.id===current)showNotice(`已重新读取 ${unique.length} 个 NFO，列表与预览已同步`);
});}
const submission=new BatchSubmission(invoke);
const retrySubmission=new BatchSubmission(invoke);
let retryQueue:import('./contracts').BatchRequest[]|null=null;
async function taskAction(name:'pause'|'recover'|'resume'|'retry'){
 if(name==='pause'){
  const task=currentTask(tasks.value.filter(task=>task.batch?.engine==='ai'&&['generate','rebuild'].includes(task.batch.mode)&&['running','requested'].includes(task.state)));
  if(task)await control(task,'paused');return;
 }
 if(name==='resume'){
  if(aiRuntime.value?.paused){showNotice('请先恢复 AI，再继续任务');return;}
  const task=resumableAI(tasks.value);if(task)await control(task,'requested');return;
 }
 await action(async()=>{
  if(name==='recover'){showNotice('正在真实测试当前 AI 供应商…');aiRuntime.value=await aiRecovery.run();showNotice('AI 已恢复；可继续任务或重试失败项');await refresh();return;}
  if(aiRuntime.value?.paused){showNotice('请先恢复 AI，再重试失败项');return;}
  if(tasks.value.some(task=>['requested','running'].includes(task.state))){showNotice('已有任务正在运行，请先完成当前任务');return;}
  if(!retryQueue){
   const failures=await invoke<MediaItem[]>('ai_failure_items');
   if(!failures.length){showNotice('当前没有可重试的 AI 失败项。');return;}
   const prepared:import('./contracts').BatchRequest[]=[];
   for(const target of ['movie','tv'] as const){
    const ids=failures.filter(item=>item.space===target).map(item=>item.id);if(!ids.length)continue;
    const items=await invoke<MediaItem[]>('preflight_items',{space:target,ids});if(disposed)return;
    prepared.push({operation_id:crypto.randomUUID(),space:target,engine:'ai',mode:'generate',item_ids:items.map(item=>item.id),root_ids:[],retry_failed:true});
   }
   retryQueue=prepared;
  }
  while(retryQueue.length&&!disposed){await retrySubmission.run(retryQueue[0]!);retryQueue.shift();}
  retryQueue=null;if(disposed)return;presentation.task_open=true;showNotice('任务已提交');await refresh();
 });
}
const pendingPreviews=new Set<string>();
const adoption=new PreviewAdoptionSubmission(invoke);
const pendingAdoptions=new Set<string>();
function checkAdoptions(history:Task[]){
 for(const id of pendingAdoptions){const task=history.find(task=>task.id===id);if(!task||!['completed','failed','cancelled','interrupted'].includes(task.state))continue;
  pendingAdoptions.delete(id);void restoreCurrent();
  if(task.state==='completed')showNotice(`已重新读取 ${task.processed} 个 NFO，列表与预览已同步`);
  else{showNotice(`采纳任务有 ${task.errors} 个失败项；请查看任务日志`);if(task.failure)report(task.failure);}
 }
}
const preview=shallowRef<{task:Task;entries:PreviewEntry[]}|null>(null);
let previewToken=0;
async function loadPreview(id:string){
 const token=++previewToken;
 try{
  const task=await invoke<Task>('batch_detail',{id});if(!task.batch)return;
  const entries:PreviewEntry[]=[];
  for(const row of task.batch.items){
   let candidate:WritePreview|null=null,ai:PreviewEntry['ai']=null,error=row.error?.message||'';
   if(row.candidate_hash){
    const receipt=await invoke<OperationResult>('operation_result',{id:row.write_id});
    if(receipt.kind!=='write'||receipt.result.operation_id!==row.write_id||receipt.result.item_id!==row.item.id||receipt.result.after_hash!==row.candidate_hash)throw Error('试写候选与任务回执不匹配');
    candidate=receipt.result;
    if(task.batch.engine==='ai'){
     const result=await invoke<OperationResult>('operation_result',{id:row.request_id});
     if(result.kind!=='ai'||result.result.request.item_id!==row.item.id)throw Error('AI 回执与试写范围不匹配');
     ai=result.result;
    }
   }else if(!error){
    const reasons:Record<string,string>={'skipped-empty-specs':'尚未准备 Technical Specs','skipped-current':'标签已完成，无需重复生成','skipped-spec-ready':'Technical Specs 已准备','skipped-unchanged-failure':'相同输入的上次 AI 失败尚未显式重试'};
    error=reasons[row.phase]||row.phase;
   }
   entries.push({row,candidate,ai,error});
  }
  if(!disposed&&token===previewToken&&entries.length)preview.value={task,entries};
 }catch(e){if(!disposed&&token===previewToken)report(e);}
}
function checkPreview(history:Task[]){
 for(const task of history)if(pendingPreviews.has(task.id)&&['completed','failed','cancelled'].includes(task.state)){
  pendingPreviews.delete(task.id);void loadPreview(task.id);if(task.state==='failed')showNotice('试写失败：请查看任务日志');
 }
}
async function submitBatch(engine:BatchEngine,mode:BatchMode,ids:string[],confirmScope:boolean){await action(async()=>{
 const requestedSpace=space.value,scope=view.value.selected.length?'当前选择':'当前 NFO';
 let resolved=ids;
 if(!submission.pending){
  if(!ids.length){showNotice('请先选择一个 NFO，或勾选需要批量处理的 NFO');return;}
  if(tasks.value.some(task=>['requested','running'].includes(task.state))){showNotice('已有任务正在运行，请先完成当前任务');return;}
  const name=engine==='ai'?'AI':'规则';
  const items=await invoke<MediaItem[]>('preflight_items',{space:requestedSpace,ids:[...ids]});
  if(disposed)return;resolved=items.map(item=>item.id);
  if(mode==='preview'&&resolved.length>10){showNotice(`${name}试写一次最多 10 个 NFO`);return;}
  if(confirmScope){
   const message=engine==='specs'?`只更新 Technical Specs，不生成标签。\n范围：${scope}\n共 ${resolved.length} 个 NFO，继续？`:`${name}生成标签\n范围：${scope}\n总数：${resolved.length}\n预计需生成：${generationCount(items,engine)}\n\n只有校验成功后才会替换软件拥有的旧 Generated Tag。`;
   if(!await originalConfirm(message)||disposed)return;
  }
 }
 const task=await submission.run({operation_id:crypto.randomUUID(),space:requestedSpace,item_ids:[...resolved],root_ids:[],engine,mode,retry_failed:false});
 if(disposed)return;
 if(task.batch?.mode==='preview')pendingPreviews.add(task.id);
 presentation.task_open=true;showNotice('任务已提交');await refresh();
});}
async function openBatch(previewOnly: boolean) {
 await submitBatch(generationEngine.value==='ai'?'ai':'rules',previewOnly?'preview':'generate',[...actionIds.value],!previewOnly);
}
async function fetchScope(){await submitBatch('specs','rebuild',[...actionIds.value],true);}
async function approvePreview(){const shown=preview.value;if(!shown&&!adoption.pending)return;await action(async()=>{
 const entries=shown?.entries.filter(entry=>entry.candidate&&!entry.error)||[],name=shown?.task.batch?.engine==='ai'?'AI':'规则';
 if(!adoption.pending&&(!entries.length||!await originalConfirm(`采纳并写入 ${entries.length} 个 NFO 的${name}试写结果？\n写入前会再次校验 NFO 是否发生变化。`)||disposed))return;
 const request=adoption.pending||{operation_id:crypto.randomUUID(),task_id:shown!.task.id,items:entries.map(entry=>({write_id:entry.row.write_id,reviewed_hash:entry.candidate!.after_hash}))};
 preview.value=null;++previewToken;presentation.task_open=true;
 const task=await adoption.run(request);if(disposed)return;
 pendingAdoptions.add(task.id);showNotice('任务已提交');await refresh();
});}
async function deliverEdit(receipt:WritePreview) {
 if(disposed)return;
 if(detail.value?.id===receipt.item_id)await inspectId(receipt.item_id);
 await refresh();
 if(receipt.phase==='committed'){lastWrite.value=receipt;showNotice('已安全写入 NFO',true);}
 else showNotice('内容没有变化，无需写入。');
}
async function runInspector(work:(item:MediaItem)=>Promise<void>) {
 const item=detail.value;if(!item)return;
 await action(async()=>{if(inspectorWriter.pending){await deliverEdit(await inspectorWriter.retry());return;}await work(item);});
}
async function commitInspector(id:string,prepare:InspectorEdit['prepare']) {if(!disposed)await deliverEdit(await inspectorWriter.run({id,prepare}));}
async function commitSpecs(item:MediaItem,specs:MediaItem['specs']) {
 const request={operation_id:crypto.randomUUID(),item_id:item.id,expected_hash:item.source_hash,specs};
 await commitInspector(request.operation_id,()=>invoke<WritePreview>('preview_specs',{request}));
}
async function commitTag(item:MediaItem,tagAction:Action) {
 const request={operation_id:crypto.randomUUID(),item_id:item.id,expected_hash:item.source_hash,action:tagAction};
 await commitInspector(request.operation_id,()=>invoke<WritePreview>('preview_tags',{request}));
}
async function editSpec(field:string,index:number|null) {await runInspector(async item=>{
 const previous=index==null?'':item.specs[field]?.[index]||'';
 const value=await originalPrompt(nativePlatform,index==null?'新增 '+field+' 规格值':'编辑 '+field,previous);
 if(value==null||!value.trim()||index!=null&&value.trim()===previous)return;
 const specs=Object.fromEntries(Object.entries(item.specs).map(([field,values])=>[field,[...(values||[])]])),values=[...(specs[field]||[])];
 if(index==null)values.push(value.trim());else values[index]=value.trim();specs[field]=values;
 await commitSpecs(item,specs);
});}
async function deleteSpec(field:string,index:number) {await runInspector(async item=>{
 if(!await originalConfirm('删除这条规格值？标签不会自动重建。'))return;
 const specs=Object.fromEntries(Object.entries(item.specs).map(([field,values])=>[field,[...(values||[])]]));specs[field]=[...(specs[field]||[])];specs[field]!.splice(index,1);await commitSpecs(item,specs);
});}
async function restoreSpec(){await runInspector(async item=>{
 if(!await originalConfirm('恢复为保存的 IMDb 原始抓取值？当前人工规格将被替换，标签只会标记为待同步。'))return;
 const id=crypto.randomUUID();await commitInspector(id,()=>invoke<WritePreview>('preview_restore_specs',{id,itemId:item.id,expectedHash:item.source_hash}));
});}
async function editTag(index:number){await runInspector(async item=>{
 const tag=item.tags[index];if(!tag)return;const value=await originalPrompt(nativePlatform,'编辑标签',tag.value);
 if(value!=null&&value.trim()&&value.trim()!==tag.value)await commitTag(item,{kind:'edit',root_index:index,value:value.trim()});
});}
async function addTag(){await runInspector(async item=>{const value=await originalPrompt(nativePlatform,'输入新的 Manual Tech Tag');if(value?.trim())await commitTag(item,{kind:'add-manual',value:value.trim()});});}
async function deleteTag(index:number){await runInspector(async item=>{
 const tag=item.tags[index];if(!tag)return;const external=tag.ownership==='external';
 const message=external?`该标签来自外部应用（如 TMM）：\n${tag.value}\n\n删除仅移除这一个根节点 <tag>，可通过“撤销”恢复。确认删除？`:`删除标签“${tag.value}”？\n（可用“撤销”恢复）`;
 if(await originalConfirm(message))await commitTag(item,{kind:'delete',root_index:index,confirm_external:external});
});}
async function setOwnership(index:number,ownership:string){await runInspector(async item=>{
 const names:Record<string,string>={external:'外部 / TMM',ai:'AI 生成','local-rules':'规则生成',manual:'Manual'},tag=item.tags[index];if(!tag||!names[ownership])return;
 if(await originalConfirm(`将标签“${tag.value}”的所有权改为 ${names[ownership]}？\n（写入 NFO ownership 清单，可撤销）`))await commitTag(item,{kind:'set-ownership',root_index:index,ownership});
});}
async function clearAiTags(){await runInspector(async item=>{
 const targets=item.tags.filter(tag=>tag.ownership==='generated'&&(tag.engine||item.inspection.tag_engine||'ai')==='ai');
 if(!targets.length){showNotice('当前 NFO 没有 AI 生成的标签');return;}
 if(await originalConfirm(`即将删除以下 ${targets.length} 个 AI 生成标签：\n\n${targets.map(tag=>'· '+tag.value).join('\n')}\n\n确认删除？（可撤销）`))await commitTag(item,{kind:'clear-ai',confirmed:true});
});}
async function undoInspector(){const original=lastWrite.value;if(!original)return;await action(async()=>{
 if(inspectorWriter.pending){await deliverEdit(await inspectorWriter.retry());return;}
 const id=crypto.randomUUID();await commitInspector(id,()=>invoke<WritePreview>('preview_undo',{id,originalId:original.operation_id}));
});}
let observer: ResizeObserver | undefined;
let stopDrag: (() => void) | null = null;
function measure() {
 if (productWindow.value) syncProductLayout(productWindow.value, space.value === 'tv');
 dimensions.width=workspace.value?.clientWidth ?? innerWidth;
 dimensions.height=innerHeight;
 dimensions.toolbar=Math.max(67, libraryTools.value?.offsetHeight ?? 95);
 dimensions.library=libraryTools.value?.clientWidth ?? 520;
 const header=workspace.value?.querySelector<HTMLElement>('#listHeader');
 if(header){const style=getComputedStyle(header);dimensions.padding=(parseFloat(style.paddingLeft)||0)+(parseFloat(style.paddingRight)||0);}
}
function drag(event: PointerEvent, kind: 'split'|'task') {
 if(event.button!==0)return;stopDrag?.();event.preventDefault();
 const target=event.currentTarget as HTMLElement, id=event.pointerId;
 dragging.value=true;target.setPointerCapture(id);target.classList.add('dragging');
 const startY=event.clientY,startHeight=presentation.task_height;
 const move=(next:PointerEvent)=>{
  if(next.pointerId!==id)return;
  if(kind==='split' && workspace.value){const rect=workspace.value.getBoundingClientRect();presentation.split_basis_points=Math.round(Math.max(100,Math.min(9900,(next.clientX-rect.left)/rect.width*10000)));}
  else presentation.task_height=Math.max(190,Math.min(4000,startHeight+startY-next.clientY));
 };
 const finish=(next:PointerEvent)=>{if(next.pointerId===id)cleanup();};
 const cleanup=()=>{target.removeEventListener('pointermove',move);target.removeEventListener('pointerup',finish);target.removeEventListener('pointercancel',finish);target.removeEventListener('lostpointercapture',finish);if(target.hasPointerCapture(id))target.releasePointerCapture(id);target.classList.remove('dragging');stopDrag=null;dragging.value=false;};
 stopDrag=cleanup;target.addEventListener('pointermove',move);target.addEventListener('pointerup',finish);target.addEventListener('pointercancel',finish);target.addEventListener('lostpointercapture',finish);
}
function splitKey(event: KeyboardEvent) {
 if(!['ArrowLeft','ArrowRight'].includes(event.key))return;event.preventDefault();
 presentation.split_basis_points=Math.max(100,Math.min(9900,presentation.split_basis_points+(event.key==='ArrowLeft'?-1:1)*(event.shiftKey?500:100)));
}
function taskKey(event: KeyboardEvent) {
 if(!['ArrowUp','ArrowDown'].includes(event.key))return;event.preventDefault();presentation.task_height=Math.max(190,Math.min(4000,presentation.task_height+(event.key==='ArrowUp'?10:-10)));
}
let layoutFrame = 0;
function scheduleLayout() {if (!layoutFrame) layoutFrame = requestAnimationFrame(() => {layoutFrame = 0; measure();});}
watch(() => [space.value, view.value.selected.length, page.value.total, totals.movie, totals.tv], () => {void nextTick(scheduleLayout);});
onMounted(()=>{observer=new ResizeObserver(scheduleLayout);if(workspace.value)observer.observe(workspace.value);if(libraryTools.value)observer.observe(libraryTools.value);for(const node of Array.from(productWindow.value?.querySelectorAll('.top,.scopebar') ?? []))observer.observe(node);addEventListener('resize',scheduleLayout);measure();});
onUnmounted(()=>{observer?.disconnect();removeEventListener('resize',scheduleLayout);cancelAnimationFrame(layoutFrame);stopDrag?.();++inspectionToken;++totalToken;});

</script>
<template>
 <div id="productWindow" ref="productWindow" class="app" :class="{'task-open':presentation.task_open}" :style="windowStyle">
  <header class="top">
   <img class="logo" :src="logo" alt="IMDb Tech Manager">
   <div class="brand"><h1>IMDb Tech Manager <span class="muted" id="version">v5.0.0</span></h1><p>NFO检查·技术规格·标签管理·批量任务</p></div>
   <div class="statusLine"><span class="pill" :class="{ok:!loading&&!error,bad:!!error}">{{loading?'正在读取资料库…':error?'资料库需要检查':`${totals.movie+totals.tv} 个 NFO`}}</span><span v-if="running" class="pill optional warn">正在处理当前媒体库</span></div>
   <div class="topActions"><button id="refreshLibrary" class="btn" :disabled="busy||loading||!roots.length" @click="scan">刷新当前媒体库</button><button id="openSettings" class="btn" @click="settingsOpen=true">设置</button></div>
  </header>
  <nav class="nav" aria-label="媒体空间"><button :class="{active:space==='movie'}" :aria-pressed="space==='movie'" @click="switchSpace('movie')">电影 <span>({{totals.movie}})</span></button><button :class="{active:space==='tv'}" :aria-pressed="space==='tv'" @click="switchSpace('tv')">电视剧 <span>({{totals.tv}})</span></button></nav>
  <main ref="workspace" class="workspace">
   <section class="pane library" :class="{tv:space==='tv'}" aria-label="NFO 媒体列表">
    <div ref="libraryTools">
     <div class="toolbar">
      <div class="toolbarPrimary"><input id="search" v-model="view.search" class="input" aria-label="搜索标题、年份、IMDb ID 或路径" placeholder="搜索标题、年份、IMDb ID 或路径" @input="view.offset=0;scheduleRefresh()"><select id="filter" v-model="selectedFilter" class="select" aria-label="状态筛选"><option v-for="[value,label] in filterOptions" :key="value" :value="value">{{label}}</option></select></div>
      <div class="toolbarSecondary"><span class="typeFilterHome" id="typeFilterHome" aria-hidden="true"></span><select id="typeFilter" :class="{hide:space!=='tv'}" v-model="level" class="select" aria-label="电视剧层级"><option value="all">全部层级</option><option value="tvshow">节目</option><option value="episode">单集</option></select><div class="selectionTools"><button id="selAll" class="btn" :disabled="busy||loading" @click="selectFiltered()">全选</button><button id="selInvert" class="btn" :disabled="busy||loading" @click="selectFiltered(true)">反选</button><button id="selClear" class="btn" @click="view.selected=[]">清空选择</button><span class="selectionSummary"><span>显示 {{page.total}} / {{totals[space]}}</span><span class="sep">·</span><span>已选 {{view.selected.length}}</span></span></div></div>
     </div>
     <div v-if="scanning" class="scanBanner" role="status"><span class="spin"></span><span>正在扫描资料库 NFO… 请稍候</span></div>
    </div>
    <CatalogHeader :columns="columns" :widths="widths" :grid="gridStyle" :sort="view.sort" :descending="view.descending" @change="saveColumns" @sort="sortBy" @resizing="dragging=$event" />
    <div id="libraryList" class="list" :class="{compact:columns.compact}" role="list" aria-label="NFO 列表">
     <div v-if="loading" class="empty" role="status">正在读取 NFO…</div>
     <div v-else-if="startupFailed" class="empty" role="alert">启动未完成。<button class="btn" @click="initialize">重新连接</button></div>
     <div v-else-if="!roots.length" class="empty">当前媒体空间尚未配置资料库。<br><button class="btn" @click="settingsOpen=true">设置资料库</button></div>
     <div v-else-if="!page.total" class="empty">当前组合条件没有匹配的 NFO</div>
     <CatalogList v-if="!loading && !startupFailed && page.total" :space="space" :items="page.items" :all-tv-items="allTvItems" :view="view" :columns="columnKeys" :grid="gridStyle" :current="detail?.id ?? null" :locale="configuration.locale" @inspect="clickItem" @check="checkItem" @context="showContext" @expand="toggleExpanded" @season="selectSeason" />
    </div>
   </section>
   <div id="splitter" class="splitter" role="separator" aria-orientation="vertical" aria-label="调整 NFO 列表与预览宽度" :aria-valuenow="Math.round(presentation.split_basis_points/100)" aria-valuemin="1" aria-valuemax="99" tabindex="0" @pointerdown="drag($event,'split')" @keydown="splitKey"></div>
   <section class="pane inspector" aria-label="NFO 检查器">
    <div class="inspectHead"><div><h2 id="inspectTitle" :data-i18n-user="detail ? '' : undefined">{{detail ? detail.title||'(无标题)' : '请选择一个 NFO'}}</h2><p id="inspectPath" :title="detail?.path">{{detail ? detail.path+(view.selected.length>1?`　（已选 ${view.selected.length} 个，点击其他行可继续查看）`:'') : view.selected.length?`已选 ${view.selected.length} 个 · 点击列表中的任意一行查看其详情（不影响选择）`:'左侧列表支持单选检查与多选批量处理'}}</p></div><div class="grow"></div><span id="inspectStatus" class="pill" :class="detail ? pillClass(detail) : ''">{{detail?statusLabel(detail):'未选择'}}</span></div>
    <div class="tabs" role="tablist" aria-label="检查内容"><button role="tab" :aria-selected="presentation.inspector_tab==='overview'" :class="{active:presentation.inspector_tab==='overview'}" @click="presentation.inspector_tab='overview'">概览 <span v-if="detail?.inspection.issues.length">({{detail.inspection.issues.length}})</span></button><button role="tab" :aria-selected="presentation.inspector_tab==='specs'" :class="{active:presentation.inspector_tab==='specs'}" @click="presentation.inspector_tab='specs'">Technical Specs</button><button role="tab" :aria-selected="presentation.inspector_tab==='tags'" :class="{active:presentation.inspector_tab==='tags'}" @click="presentation.inspector_tab='tags'">标签</button></div>
    <div id="inspectBody" class="inspectBody" role="tabpanel">
     <div v-if="error" class="card failure" role="alert"><p>{{error}}</p><button class="btn" @click="startupFailed?initialize():(error='',refresh(),restoreCurrent())">{{startupFailed?'重新连接':'重新读取'}}</button></div>
     <div v-if="saveError" class="card failure" role="alert"><p>界面状态尚未保存：{{saveError}}</p><button class="btn" @click="retryState">重试保存</button></div>
     <div v-if="!detail" class="empty">选择一个 NFO 后在这里查看详情。</div>
     <template v-else>
      <div v-if="detail.error" class="card failure" role="alert">{{detail.error.code}}：{{detail.error.message}}</div>
      <template v-if="presentation.inspector_tab==='overview'">
       <InspectorOverview :item="detail" :platform="nativePlatform" @changed="inspectorChanged" @notice="showNotice" @failure="report" @reveal="action(async()=>{await invoke('reveal_item',{id:detail!.id});})" @copy="copy" />
      </template>
      <InspectorSpecs v-else-if="presentation.inspector_tab==='specs'" :item="detail" :busy="busy" @edit="editSpec" @remove="deleteSpec" @restore="restoreSpec" />
      <InspectorTags v-else :item="detail" :busy="busy" @edit="editTag" @remove="deleteTag" @add="addTag" @ownership="setOwnership" @clear="clearAiTags" />
     </template>
    </div>
   </section>
  </main>
  <section class="scopebar"><div id="scopeSummary" class="summary"><strong>{{scopeInfo.label}}</strong>{{scopeInfo.text}}</div><div class="directActions"><button id="rescanCurrent" class="btn blue" :disabled="!detail||busy" @click="reloadItems(detail?[detail.id]:[])">重新读取当前 NFO</button><button id="fetchSpec" class="btn blue" :disabled="!actionIds.length||busy" @click="fetchScope">手动获取 Tech Spec</button></div><div class="workflow"><select id="generateEngine" v-model="generationEngine" class="select" aria-label="标签生成方式"><option value="ai">AI 生成标签</option><option value="local-rules">规则生成标签</option></select><button id="previewScope" class="btn" :disabled="!actionIds.length||busy" @click="openBatch(true)">试写</button><button id="generateNow" class="btn yellow" :disabled="!actionIds.length||busy" @click="openBatch(false)">生成</button></div></section>
  <TaskCenter :open="presentation.task_open" :height="presentation.task_height" :history="presentation.task_history" :history-text="historyText" :history-fixed="historyFixed" :job="job" :tasks="tasks" :runtime="aiRuntime" :failures="aiFailures.length" :busy="busy" :locale="configuration.locale" @toggle="presentation.task_open=!presentation.task_open" @resize="drag($event,'task')" @key="taskKey" @tab="taskTab" @action="taskAction" />
 </div>
 <div class="toast" id="toast" :class="{show:notice}"><span id="toastText">{{notice}}</span><button class="btn" :class="{hide:!noticeUndo||!lastWrite}" id="undoBtn" :disabled="busy" @click="undoInspector">撤销</button></div>
 <Onboarding v-if="onboardingInfo" :configuration="configuration" :info="onboardingInfo" @skip="onboardingInfo=null" @saved="configuration=$event;pendingLegacy=[];onboardingInfo=null;scheduleRefresh()" @notice="showNotice" />
 <ProductDialog v-if="settingsMounted" id="settingsModal" :visible="settingsOpen" title="设置" :inactive="cacheModalOpen" :busy="rootsBusy||cacheBusy||lifecycleBusy||aiSettingsBusy||automaticBusy" @close="closeSettings"><div class="card"><h3>应用</h3><LifecyclePanel @dirty="lifecycleDirty=$event" @busy="lifecycleBusy=$event" /></div><div class="card" id="rootSettings"><h3>文件来源与 Tech Spec 自动获取</h3><RootSettings :configuration="configuration" :pending-legacy="pendingLegacy" @notice="showNotice" @changed="configuration=$event;detail=null;scheduleRefresh()" @confirmed="pendingLegacy=[]" @dirty="rootsDirty=$event" @busy="rootsBusy=$event"><template #after-roots><CacheSettings @dirty="cacheDirty=$event" @modal="cacheModalOpen=$event" @busy="cacheBusy=$event" /></template><template #footer="root"><AutomaticPanel @dirty="automaticDirty=$event" @busy="automaticBusy=$event"><button id="saveRoots" class="btn blue" :disabled="root.busy||root.conflict" @click="root.save">保存分类资料库</button></AutomaticPanel></template></RootSettings></div><div class="card"><AiGenerator :active="settingsOpen" :blocked="false" @notice="showNotice" @dirty="aiSettingsDirty=$event" @busy="aiSettingsBusy=$event" /></div><div class="card" style="border-color:#6d3037"><h3>退出</h3><div class="muted" style="margin-bottom:9px">退出将同时停止后台 Agent 和常驻引擎（本次会话不再自动重启；登录自启仍按上方设置生效）。</div><button id="quitApp" class="btn danger" @click="action(()=>invoke('quit_probe'))">退出 IMDb Tech Manager</button></div><AboutPanel :active="settingsOpen" @notice="showNotice" /></ProductDialog>
 <CatalogContext v-if="contextMenu" :items="contextMenu.items" :x="contextMenu.x" :y="contextMenu.y" :platform="nativePlatform" @close="contextMenu=null" @command="contextCommand" />
 <PreviewResults v-if="preview" :entries="preview.entries" :engine="preview.task.batch!.engine" :busy="busy" @close="preview=null;++previewToken" @approve="approvePreview" />
</template>
