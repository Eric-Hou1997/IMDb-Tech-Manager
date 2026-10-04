import type {BatchRequest,Task} from './contracts';
type Port=<T>(command:string,args:Record<string,unknown>)=>Promise<T>;
// Own exactly one user-requested batch across unknown IPC outcomes. Retrying
// its saved IDs never plans a second set of AI requests or a different scope.
export class BatchSubmission {
 private port:Port;
 pending:BatchRequest|null=null;
 private running=false;
 constructor(port:Port){this.port=port;}
 async run(request:BatchRequest):Promise<Task> {
  if(this.running)throw Error('任务正在提交');
  this.running=true;
  this.pending??={...request,item_ids:[...request.item_ids],root_ids:[...request.root_ids]};
  try {
   const task=await this.port<Task>('plan_batch',{request:this.pending});
   if(!task.batch||task.id!==this.pending.operation_id)throw Error('批量任务回执类型不匹配');
   const approved=await this.port<Task>('approve_batch_scope',{id:task.id,reviewedHash:task.batch.plan_hash});
   if(approved.id!==task.id||!approved.batch?.approved)throw Error('批量任务启动回执不匹配');
   this.pending=null;return approved;
  }catch(error){
   const code=error&&typeof error==='object'&&'code' in error?String(error.code):'';
   if(['invalid-operation-id','operation-conflict','invalid-scope','preview-limit','ai-disabled','invalid-ai-settings','configuration-conflict','invalid-transition'].includes(code))this.pending=null;
   throw error;
  }finally{this.running=false;}
 }
}
