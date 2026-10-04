import type {AiRuntime, Task, TaskJob} from './contracts';
export function currentTask(tasks:Task[]):Task|null {
 return tasks.find(t=>t.state==='running')||tasks.find(t=>t.state==='requested')||tasks[0]||null;
}
export function resumableAI(tasks:Task[]):Task|null {
 const seen=new Set<string>();
 return tasks.find(t=>{if(t.batch?.engine!=='ai'||!['generate','rebuild'].includes(t.batch.mode)||seen.has(t.space))return false;seen.add(t.space);return ['paused','interrupted','failed'].includes(t.state);})||null;
}
export function taskControls(tasks:Task[],runtime:AiRuntime|null,failures:number) {
 const ai=currentTask(tasks.filter(t=>t.batch?.engine==='ai'&&['generate','rebuild'].includes(t.batch.mode)&&['running','requested'].includes(t.state)));
 const resume=resumableAI(tasks);
 return {pause:!!ai&&!ai.batch?.pause_requested,recover:!!runtime?.paused,resume:!!resume,retry:failures>0||resume?.state==='failed'};
}
const names:Record<string,string>={'zh-CN':'简体中文','zh-Hant':'繁體中文','en-US':'English (United States)','fr-FR':'Français','ru-RU':'Русский','ja-JP':'日本語','es-ES':'Español','th-TH':'ไทย',ja:'日本語',en:'English (United States)'};
function identity(job:TaskJob){return (names[job.language]||job.language||names['zh-CN'])+(job.language_pack_revision?` · r${job.language_pack_revision}`:'');}
export function jobActionLabel(action:string){return ({'cache-maintain':'整理 IMDb 缓存','cache-clear':'清空 IMDb 缓存'} as Record<string,string>)[action]||action||'';}
export function currentLog(job:TaskJob|null){return job?.log?`[${identity(job)} · 本次日志]\n${job.log}`:'等待任务…';}
export function historyLog(jobs:TaskJob[]){return jobs.map(job=>`[${identity(job)} · 历史日志] ${job.ended_at||job.started_at} · ${jobActionLabel(job.action)} · ${job.message}`).join('\n')||'暂无历史任务';}
export function jobSummary(job:TaskJob|null,task:Task|null,locale:string){
 if(task?.batch?.total)return `${jobActionLabel(job?.action||'')} · 进度 ${task.processed}/${task.batch.total}${task.current_path?' · '+task.current_path.split(/[\\/]/).pop():''}`;
 return job?[jobActionLabel(job.action),job.message,job.ended_at?new Date(job.ended_at).toLocaleString(locale):''].filter(Boolean).join(' · ')||'暂无任务':'暂无任务';
}
