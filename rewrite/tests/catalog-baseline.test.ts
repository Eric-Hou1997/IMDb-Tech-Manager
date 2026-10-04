import {fileURLToPath} from 'node:url';
import test, {after} from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {createServer} from 'vite';
import vue from '@vitejs/plugin-vue';
import {createSSRApp, h} from 'vue';
import {renderToString} from 'vue/server-renderer';
import {baseParse} from '@vue/compiler-dom';
import {cell, clearFilters, movieChunk, sortItems, statusClass, statusLabel, tvRows} from '../src/catalog-presentation.ts';
import type {LibraryView, MediaItem, AiRecord, BatchItem, WritePreview} from '../src/contracts';

const source = await readFile(new URL('../../macos/web/index.html', import.meta.url), 'utf8');
const columns = ['title', 'year', 'added_date', 'spec_status', 'tag_status'];
const grid = '26px 9px 350.00px 82.00px 88.00px 84.00px 82.00px 34px';
const view = (): LibraryView => ({search: '', errors: false, issues: false, lifecycle: '', roots: [], selected: [], expanded: [], offset: 0, sort: 'title', descending: false});
function item(id: string, extra: Partial<MediaItem> = {}): MediaItem {
 return {id, root_id: 'root', space: 'movie', path: '/media/' + id + '.nfo', source_hash: '', title: id, series_key: '', year: '2026', kind: 'Movie', season: '', episode: '', specs: {}, tags: [], error: null, modified_at: 0, parser_revision: 6, spec_status: 'missing', imdb: '', inspection: {lifecycle: 'spec-missing', xml_valid: true, status_override: '', issues: [], ignored_issues: []} as MediaItem['inspection'], ...extra};
}
function legacy(item: MediaItem) {
 return {...item, media_space: item.space === 'movie' ? 'movies' : 'tv', media_type: item.kind === 'Series' ? 'tvshow' : item.kind === 'Episode' ? 'episode' : 'movie', season: item.season === '' ? null : +item.season, episode: item.episode === '' ? null : +item.episode, lifecycle: item.inspection.lifecycle, xml_valid: item.inspection.xml_valid, status_override: item.inspection.status_override, issues: item.inspection.issues.map(kind => ({kind})), tag_status: item.inspection.tag_status};
}
const hosts: Record<string, any> = {};
const context: any = {uiLanguage: 'zh-CN', renderedCount: 0, chunkObserver: null,
 qa: () => [], columnPrefs: () => ({visible: columns}), window: {IntersectionObserver: true},
 activeColumns: () => columns.map(key => [key]), listGrid: () => grid,
 sortState: () => ({field: context.sortField || 'title', direction: context.descending ? 'desc' : 'asc'}),
 renderListHeader: () => {}, updateSelectionUI: () => {}, syncCatalogLayout: () => {}, schedulePreflight: () => {},
 q: (id: string) => hosts[id] ||= {value: 'all', textContent: '', innerHTML: '', scrollTop: 0, classList: {toggle: () => {}}, appendChild: (node: any) => {hosts[id].innerHTML += `<div class="${node.className}" id="${node.id}"></div>`;}},
 document: {createElement: () => ({})}, IntersectionObserver: class {observe() {} disconnect() {}}
};
vm.createContext(context);
for (const prefix of ['const CHUNK=', 'const BUCKET_TEXT=', 'const MEDIA_TYPE_TEXT=', 'const LIST_COLUMNS=', 'function esc(', 'function lcOf(', 'function specStatusText(', 'function bucketOf(', 'function statusClass(', 'function statusText(', 'function formatUIDate(', 'function compareItems(', 'function sortItems(', 'function cellValue(', 'function matches(', 'function visibleItems(', 'function itemRow(', 'function treeLabelRow(', 'function renderLibrary(', 'function overviewHTML(', 'function specsHTML(', 'function tagsHTML(', 'function mediaTypeText(', 'function tagStatusText(', 'function issueKindText(', 'function showContextMenu(', 'function previewRecordHTML(', 'function automaticScope(', 'function localScopeSummary(']) {
 const line = source.split('\n').find(line => line.startsWith(prefix)); assert.ok(line, prefix); vm.runInContext(line, context);
}
const server = await createServer({configFile: false, root: fileURLToPath(new URL('..', import.meta.url)), plugins: [vue()], server: {middlewareMode: true, watch: null, hmr: false, ws: false}, optimizeDeps: {noDiscovery: true, entries: []}, appType: 'custom'});
after(() => server.close());
const List = (await server.ssrLoadModule('/src/CatalogList.vue')).default;
const Overview = (await server.ssrLoadModule('/src/InspectorOverview.vue')).default;
const Specs = (await server.ssrLoadModule('/src/InspectorSpecs.vue')).default;
const Tags = (await server.ssrLoadModule('/src/InspectorTags.vue')).default;
const ContextMenu = (await server.ssrLoadModule('/src/CatalogContext.vue')).default;
const Preview = (await server.ssrLoadModule('/src/PreviewResults.vue')).default;
const TaskCenter = (await server.ssrLoadModule('/src/TaskCenter.vue')).default;
const {previewRecord} = await server.ssrLoadModule('/src/preview-presentation.ts');
const {scopeSummary,generationCount} = await server.ssrLoadModule('/src/scope-presentation.ts');
const {contextItems,generationItems} = await server.ssrLoadModule('/src/catalog-context.ts');
const {specFields} = await server.ssrLoadModule('/src/editor-drafts.ts');
const {identityRows, issueMessage, issueLabel} = await server.ssrLoadModule('/src/inspector-presentation.ts');
function normalized(html: string): unknown {
 const walk = (node: any): unknown => {
  if (node.type === 3) return null;
  if (node.type === 2) return node.content.trim() ? node.content.trim() : null;
  if (node.type === 0) return node.children.map(walk).filter(Boolean);
  return [node.tag, Object.fromEntries(node.props.filter((p: any) => p.type === 6 && p.name !== 'indeterminate' && !(p.name === 'class' && !p.value?.content?.trim())).map((p: any) => [p.name, p.name === 'class' ? (p.value?.content || '').split(/\s+/).filter(Boolean).sort().join(' ') : p.name === 'style' ? (p.value?.content || '').replace(/;$/, '') : p.value?.content || '']).sort((a: any, b: any) => a[0].localeCompare(b[0]))), node.children.map(walk).filter(Boolean)];
 };
 return walk(baseParse(html, {isVoidTag: tag => ['input', 'img', 'br', 'hr', 'meta', 'link'].includes(tag)}));
}
function fragment(html:string,id:string):string {
 const pending:any[]=[baseParse(html)];
 while(pending.length){const node=pending.pop();if(node.type===1&&node.props.some((prop:any)=>prop.name==='id'&&prop.value?.content===id))return html.slice(html.indexOf('>',node.loc.start.offset)+1,node.loc.end.offset-(`</${node.tag}>`.length));pending.push(...(node.children||[]));}
 throw Error('Missing '+id);
}
test('task drawer restores the original plain log, tabs, six-part footer and header',async()=>{
 const html=await renderToString(createSSRApp({render:()=>h(TaskCenter,{open:false,height:240,history:false,historyText:'暂无历史任务',historyFixed:false,job:null,tasks:[],runtime:null,failures:0,busy:false,locale:'zh-CN'})}));
 const expected=source.split('\n').find(line=>line.includes('<section class="drawer" id="taskDrawer">'))!.trim().replace('class="log" id="jobLog"','class="log" id="jobLog" data-fixed-language="false"');
 // Retain the existing rewrite's keyboard separator metadata; it has no
 // effect on the baseline styling or the original task-center controls.
 const actual=html.replace(/<div([^>]*id="taskResize"[^>]*)>/,(_tag,attrs)=>'<div'+attrs.replace(/ (role|aria-orientation|aria-valuenow|aria-valuemin|aria-valuemax|tabindex)="[^"]*"/g,'')+'>');
 assert.deepEqual(normalized(actual),normalized(expected));
});
async function compare(items: MediaItem[], currentView = view(), all = items) {
 const space = items[0]?.space || 'movie';
 for (const key of Object.keys(hosts)) delete hosts[key];
 context.state = {space: space === 'movie' ? 'movies' : 'tv', items: all.map(legacy), selected: {movies: new Set(currentView.selected.map(id => all.find(item => item.id === id)!.path)), tv: new Set(currentView.selected.map(id => all.find(item => item.id === id)!.path))}, current: {movies: '', tv: ''}, maxSeen: {movies: 0, tv: 0}, treeOpen: Object.fromEntries(currentView.expanded.map(key => [key, true]))};
 context.q('#search').value = currentView.search; context.q('#filter').value = currentView.catalog_filter || 'all'; context.q('#typeFilter').value = currentView.media_level || 'all';
 context.sortField = ({'added-date': 'added_date', 'specs-status': 'spec_status', 'tags-status': 'tag_status'} as Record<string, string>)[currentView.sort] || currentView.sort;
 context.descending = currentView.descending;
 vm.runInContext('renderLibrary()', context);
 const html = await renderToString(createSSRApp({render: () => h(List, {space, items: sortItems(items, currentView), allTvItems: all, view: currentView, columns, grid: {gridTemplateColumns: grid}, current: null, locale: 'zh-CN'})}));
 assert.deepEqual(normalized(html), normalized(context.q('#libraryList').innerHTML));
}
test('complete movie rows and initial 600-row chunk reproduce the original DOM', async () => {
 assert.equal(movieChunk, vm.runInContext('CHUNK', context));
 for (const count of [127, 607]) await compare(Array.from({length: count}, (_, index) => item(`电影 ${String(index).padStart(3, '0')}`, {year: '2026-extended', added_date: '2026-10-03'})));
});
test('TV groups, show rows, season selection and collapsed episode order reproduce v4.1.0', async () => {
 const rows = [item('show', {space: 'tv', kind: 'Series', title: '同名节目'}), item('episode-2', {space: 'tv', kind: 'Episode', title: '第二集', series_key: '同名节目', season: '1', episode: '2'}), item('episode-10', {space: 'tv', kind: 'Episode', title: '第十集', series_key: '同名节目', season: '1', episode: '10'}), item('second-folder', {space: 'tv', kind: 'Episode', title: '另一目录', series_key: '同名节目', season: '2', episode: '1'})];
 for (const expanded of [[], ['show:同名节目'], ['show:同名节目', 'show:同名节目:1']]) await compare(rows, {...view(), expanded, selected: ['episode-2']});
 const filtered = {...view(), search: '第二集', expanded: ['show:同名节目', 'show:同名节目:1']};
 await compare([rows[1]], filtered, rows);
 assert.deepEqual(tvRows([rows[1]], rows, filtered).rows.find(row => row.kind === 'season')!.members, ['episode-2', 'episode-10']);
});
test('status precedence, labels, dates and semantic sorting match original functions', () => {
 for (const lifecycle of ['ai-complete', 'local-complete', 'spec-empty', 'spec-missing', 'no-tags', 'stale', 'not-applicable', 'xml-error']) for (const manual of ['', 'ai-complete']) {
  const row = item(lifecycle); row.inspection.lifecycle = lifecycle; row.inspection.status_override = manual;
  assert.equal(statusLabel(row), context.statusText(legacy(row))); assert.equal(statusClass(row), context.statusClass(legacy(row)));
  assert.equal(cell(row, 'spec_status'), context.specStatusText(row.spec_status));
 }
 const rows = [item('中文', {year: '2024', spec_status: 'manual'}), item('Alpha', {year: '2026', spec_status: 'empty'}), item('映画', {year: '2024', spec_status: 'ready'})];
 for (const [sort, field] of [['title', 'title'], ['year', 'year'], ['specs-status', 'spec_status']] as const) for (const descending of [false, true]) {
  context.sortField = field; context.descending = descending;
  assert.deepEqual(sortItems(rows, {...view(), sort, descending}).map(row => row.id), Array.from(context.sortItems(rows.map(legacy)), (row: any) => row.id));
 }
});
test('clear-all resets search and both filters while preserving selection and sorting', () => {
 const current = {...view(), search: 'old', catalog_filter: 'ai' as const, media_level: 'episode' as const, selected: ['keep'], descending: true};
 clearFilters(current);
 assert.equal(current.search, ''); assert.equal(current.catalog_filter, 'all'); assert.equal(current.media_level, 'all'); assert.deepEqual(current.selected, ['keep']); assert.equal(current.descending, true);
});
test('old parser caches remain an actionable problem until the read-only launch scan finishes', () => {
 const row = item('old'); row.inspection.lifecycle='index-refresh-required';
 assert.equal(statusLabel(row),'有问题'); assert.equal(statusClass(row),'bad');
});
test('context menu preserves original scope, disabled actions and six-button DOM',async()=>{
 for(const count of [1,10,11])for(const ready of [false,true]){
  const rows=Array.from({length:count},(_,index)=>{const row=item('menu-'+index);if(ready)row.inspection.lifecycle='no-tags';return row;});
  context.state={items:rows.map(legacy)};context.innerWidth=1000;context.innerHeight=760;
  context.q('#ctxMenu').style={};context.q('#ctxMenu').offsetHeight=180;
  context.showContextMenu({clientX:40,clientY:50},rows.map(row=>row.path));
  const ssr:any={};await renderToString(createSSRApp({render:()=>h(ContextMenu,{items:rows,x:40,y:50,platform:'macos'})}),ssr);
  assert.deepEqual(normalized(fragment(ssr.teleports.body,'ctxMenu')),normalized(context.q('#ctxMenu').innerHTML));
  assert.equal(generationItems(rows).length,ready?count:0);
 }
 const rows=[item('visible'),item('hidden'),item('other')],selected=['hidden','visible'];
 assert.deepEqual(contextItems(rows[0],selected,rows).map((row:MediaItem)=>row.id),selected);
 assert.deepEqual(contextItems(rows[2],selected,rows).map((row:MediaItem)=>row.id),['other']);assert.deepEqual(selected,['hidden','visible']);
});
test('preview chips, protected existing tags and review explanations reproduce original DOM',async()=>{
 for(const engine of ['ai','rules']){
  const row={item:item('试写'),phase:'review-ready',write_id:'write',request_id:'request',candidate_hash:'hash',error:null} as BatchItem;
  const candidate={title:'试写',imdb:'tt0061452',before_tags:[{value:'Old',ownership:'generated',engine:'ai',field:''}],after_tags:[{value:'Generated',ownership:'generated',engine,field:'Camera'},{value:'External',ownership:'external',engine:'',field:''},{value:'Manual',ownership:'manual',engine:'',field:''},{value:'manual',ownership:'manual',engine:'',field:''}]} as WritePreview;
  const entry={row,candidate,ai:engine==='ai'?{result:{requires_review:true,warnings:['请检查来源'],review_reasons:['需要人工复核']}} as AiRecord:null,error:''};
  const ssr:any={};await renderToString(createSSRApp({render:()=>h(Preview,{entries:[entry],engine,busy:false})}),ssr);
  assert.deepEqual(normalized(fragment(ssr.teleports.body,'previewBody')),normalized(context.previewRecordHTML(previewRecord(entry),engine==='ai'?'ai':'local-rules')));
 }
});
test('scope summary and generation forecast use original selection and actual tag status',()=>{
 const rows=['ai-complete','local-complete','no-tags','spec-missing','xml-error'].map((lifecycle,index)=>{const row=item('scope-'+index);row.inspection.lifecycle=lifecycle;row.spec_status=lifecycle==='spec-missing'?'missing':'ready';row.inspection.tag_status=lifecycle==='ai-complete'?'ai-current':lifecycle==='local-complete'?'local-current':'none';return row;});
 for(const selected of [false,true]){
  const items=selected?rows:[rows[0]];
  context.state={space:'movies',items:rows.map(legacy),selected:{movies:new Set(selected?rows.map(row=>row.path):[])},current:{movies:rows[0].path}};
  context.localScopeSummary();const summary=scopeSummary(items,selected);
  assert.equal(context.q('#scopeSummary').innerHTML,`<strong>${summary.label}</strong>${summary.text}`);
 }
 assert.equal(generationCount(rows,'ai'),3);assert.equal(generationCount(rows,'rules'),3);
 rows[0].inspection.status_override='local-complete';rows[0].inspection.lifecycle='local-complete';
 assert.equal(generationCount(rows,'ai'),3); // A presentation annotation does not change generator eligibility.
});

test('Inspector overview identity, status controls and issues reproduce the original DOM', async () => {
 for(const kind of ['Movie', 'Series', 'Episode', 'Season']) for(const manual of ['', 'ai-complete']) for(const mirrored of [null, true, false]) {
  const row=item('电影 & 特别篇', {kind, title: '电影 & 特别篇', year: '1967', imdb: 'tt0061452', source_hash: 'a'.repeat(64), spec_status: 'manual', tags: [{value:'AI', ownership:'generated', engine:'ai'}, {value:'Manual', ownership:'manual', engine:''}, {value:'TMM', ownership:'external', engine:''}]});
  Object.assign(row.inspection, {xml_valid:true, bom:true, newline:'CRLF', tag_status:'stale', lifecycle:manual||'stale', status_override:manual, manifest_sidecar_match:mirrored, issues:manual?[]:['stale','duplicate-tag'], ignored_issues:manual?['spec-missing']:[]});
  const data=legacy(row);
  Object.assign(data, {bom:row.inspection.bom, newline:row.inspection.newline, manifest_sidecar_match:mirrored, counts:{generated:1,manual:1,external:1}, ignored_issues:row.inspection.ignored_issues, issues:row.inspection.issues.map(kind=>({kind, message:issueMessage(row,kind), path:row.path}))});
  if(kind==='Season')data.media_type='season';
  const actual=await renderToString(createSSRApp({render:()=>h(Overview, {item:row,platform:'macos'})}));
  assert.deepEqual(normalized(actual), normalized(context.overviewHTML(data)));
  assert.equal(identityRows(row).find(row=>row.key==='Manifest / Sidecar')!.value, mirrored==null?'无镜像':mirrored?'一致':'不一致');
 }
 const malformed=item('bad', {error:{code:'invalid-xml',message:'Unclosed movie element',path:'/media/bad.nfo',operation_id:null,retryable:false}});
 assert.equal(issueMessage(malformed, 'xml-error'), 'Unclosed movie element');
});
test('original AI hints and persisted failure text use the same Inspector DOM and labels',async()=>{
 const kinds:string[]=vm.runInContext('Object.keys(ISSUE_KIND_TEXT)',context);
 for(const kind of kinds){
  assert.equal(issueLabel(kind),context.issueKindText(kind));
  const row=item('AI fixture',{spec_status:'ready'});
  Object.assign(row.inspection,{xml_valid:true,bom:false,newline:'LF',tag_status:'ai-current',lifecycle:'ai-complete',status_override:'',issues:[kind],ignored_issues:[],issue_details:kind==='prompt-stale'?{}:{[kind]:{code:kind,message:'Provider detail: 人工文本 & <keep>',path:row.path,operation_id:'original-task',retryable:false}},manifest_sidecar_match:null});
  const data=legacy(row);Object.assign(data,{bom:false,newline:'LF',manifest_sidecar_match:null,counts:{generated:0,manual:0,external:0},ignored_issues:[],issues:[{kind,message:issueMessage(row,kind),path:row.path}]});
  const html=await renderToString(createSSRApp({render:()=>h(Overview,{item:row,platform:'macos'})}));
  assert.deepEqual(normalized(html),normalized(context.overviewHTML(data)));
  if(kind==='prompt-stale')assert.equal(issueMessage(row,kind),'AI 提示词/模型已更新；建议重新生成（状态保持 AI 完成）');
  else assert.equal(issueMessage(row,kind),'Provider detail: 人工文本 & <keep>');
 }
});

test('Inspector specification and root-tag rows reproduce the original DOM', async () => {
 for(const status of ['missing','ready','manual','empty']) {
  const row=item('原版', {spec_status:status, specs:{Camera:['ARRI','IMAX'], Runtime:['120 min']}, tags:[{value:'AI',ownership:'generated',engine:'ai',field:'Camera'},{value:'Rules',ownership:'generated',engine:'local-rules',field:'Aspect ratio'},{value:'User',ownership:'manual',engine:'',field:''},{value:'TMM',ownership:'external',engine:'',field:''}]});
  Object.assign(row.inspection,{source_specs:{Camera:['Panavision'],Runtime:['120 min']},tag_engine:'ai'});
  const data=legacy(row);
  Object.assign(data,{tag_engine:'ai', tags:row.tags.map((tag,root_index)=>({...tag,root_index})), technical_specs:status==='missing'?null:{effective:Object.fromEntries(specFields.map((field:string)=>[field,row.specs[field]||[]])), source:row.inspection.source_specs, modified:status==='manual',tag_fields:['Sound mix','Camera','Aspect ratio','Negative Format','Cinematographic Process','Printed Film Format']}});
  const specHtml=await renderToString(createSSRApp({render:()=>h(Specs,{item:row,busy:false})}));
  assert.deepEqual(normalized(specHtml), normalized(context.specsHTML(data)));
  const tagHtml=await renderToString(createSSRApp({render:()=>h(Tags,{item:row,busy:false})}));
  assert.deepEqual(normalized(tagHtml), normalized(context.tagsHTML(data)));
 }
});
