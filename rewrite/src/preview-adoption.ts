import type {PreviewAdoption,Task} from './contracts';
type Port=<T>(command:string,args:Record<string,unknown>)=>Promise<T>;
export class PreviewAdoptionSubmission {
 pending:PreviewAdoption|null=null;
 private running=false;
 constructor(privatePort:Port){this.port=privatePort;}
 private port:Port;
 async run(request:PreviewAdoption):Promise<Task>{
  if(this.running)throw Error('试写结果正在提交');this.running=true;
  this.pending??={...request,items:request.items.map(item=>({...item}))};
  try{
   let task:Task;
   try{task=await this.port<Task>('adopt_preview',{request:this.pending});}
   catch(error){try{task=await this.port<Task>('task_result',{id:this.pending.operation_id});}catch{
    const code=error&&typeof error==='object'&&'code' in error?String(error.code):'';
    if(['invalid-operation-id','operation-conflict','invalid-scope','review-mismatch','active-task','root-identity-changed','configuration-conflict','recovery-required'].includes(code))this.pending=null;
    throw error;
   }}
   const expected=this.pending.items;
   if(task.id!==this.pending.operation_id||!task.batch?.approved||task.batch.mode!=='adopt-preview'||task.batch.total!==expected.length||expected.some(item=>!task.batch!.items.some(row=>row.write_id===item.write_id&&row.candidate_hash===item.reviewed_hash)))throw Error('采纳任务与审核回执不匹配');
   this.pending=null;return task;
  }finally{this.running=false;}
 }
}
