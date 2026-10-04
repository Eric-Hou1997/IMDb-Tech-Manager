import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
import vm from 'node:vm';
import {currentTask,resumableAI,taskControls,currentLog,historyLog,jobSummary} from '../src/task-center.ts';
import type {Task,TaskJob,AiRuntime} from '../src/contracts';
const original=await readFile(new URL('../../macos/web/index.html',import.meta.url),'utf8');
function task(state:string,engine='ai',mode='generate',extra={}){return {id:'one',space:'movie',state,processed:3,current_path:null,batch:{engine,mode,total:10},...extra} as unknown as Task;}
function legacyControls(state:string,mode:string,pause:boolean,runtime:boolean,failures:number){
 const hidden:Record<string,boolean>={};
 const context={state:{job:{running:['running','requested'].includes(state),action:mode==='preview'?'ai-preview-write-selected':'ai-generate-selected',exit_code:state==='failed'?1:0},status:{extra:{ai_batch_state:{status:state==='interrupted'?'paused':state,pause_requested:pause},ai_runtime:{paused:runtime},ai_failure_queue:{items:Array(failures).fill({})}}}},q:(id:string)=>({classList:{toggle:(_:string,value:boolean)=>{hidden[id]=value;}}})};
 vm.runInNewContext(original.split('\n').find(line=>line.startsWith('function updateTaskControls('))!+'\nupdateTaskControls()',context);
 return {pause:!hidden['#pauseTask'],recover:!hidden['#recoverAI'],resume:!hidden['#resumeTask'],retry:!hidden['#retryFailed']};
}
test('original AI task controls preserve pause, runtime recovery and explicit failure retry',()=>{
 for(const state of ['running','requested','paused','interrupted','completed','failed'])for(const pause of [false,true])for(const runtime of [false,true])for(const failures of [0,2]){
  // Persisted pause requests belong to a running/paused batch, never a completed one.
  if(pause&&!['running','paused'].includes(state))continue;
  const tasks=[task(state,'ai','generate',{batch:{engine:'ai',mode:'generate',pause_requested:pause}})];
  assert.deepEqual(taskControls(tasks,{paused:runtime} as AiRuntime,failures),legacyControls(state,'generate',pause,runtime,failures));
 }
 assert.equal(taskControls([task('running','rules')],null,0).pause,false);
 assert.equal(taskControls([task('running','ai','preview')],null,0).pause,false);
});
test('global current job follows the running owner and only the latest AI batch can resume',()=>{
 const failed=task('failed'),finished=task('completed','rules'),running=task('running','ai','generate',{space:'tv'});
 assert.equal(currentTask([finished,failed,running]),running);
 assert.equal(resumableAI([task('completed'),failed]),null);
 assert.equal(resumableAI([finished,failed]),failed);
 const tvFailed=task('paused','ai','generate',{space:'tv'});
 assert.equal(resumableAI([task('completed'),tvFailed]),tvFailed);
});
test('task and imported history text keep the recorded language and original log bytes',()=>{
 const job={job_id:'a',language:'ja-JP',language_pack_revision:3,log:'\ufeff過去\r\n',action:'ai-generate-selected',ended_at:'2026-10-03T12:00:00Z',message:'完了'} as TaskJob;
 assert.equal(currentLog(job),'[日本語 · r3 · 本次日志]\n\ufeff過去\r\n');
 assert.equal(historyLog([job]),'[日本語 · r3 · 历史日志] 2026-10-03T12:00:00Z · ai-generate-selected · 完了');
 assert.equal(currentLog(null),'等待任务…');assert.equal(historyLog([]),'暂无历史任务');
 assert.equal(jobSummary(job,task('running','ai','generate',{current_path:'D:\\媒体\\中文.nfo'}),'zh-CN'),'ai-generate-selected · 进度 3/10 · 中文.nfo');
});
