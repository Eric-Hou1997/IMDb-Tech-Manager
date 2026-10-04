import type {AiRecord,AiRuntime} from './contracts';
type Port=<T>(command:string,args:Record<string,unknown>)=>Promise<T>;
// One real connection test owns an explicit Recover click across lost replies.
export class AiRecovery {
 private pending:{test:string}|null=null;
 private running=false;
 private port:Port;
 constructor(port:Port){this.port=port;}
 async run():Promise<AiRuntime> {
  if(this.running)throw Error('正在恢复 AI');this.running=true;
  this.pending??={test:crypto.randomUUID()};
  try{
   let result:AiRecord;
   try{result=await this.port<AiRecord>('test_ai_connection',{id:this.pending.test});}
   catch(error){try{result=await this.port<AiRecord>('ai_record',{id:this.pending.test});}catch{throw error;}}
   if(result.request.operation_id!==this.pending.test||result.purpose!=='connection-test')throw Error('AI 恢复测试回执不匹配');
   if(['requested','running'].includes(result.phase))throw Error('AI 恢复测试尚未完成');
   if(result.phase!=='review-ready'||result.error||result.meter.attempts<1){this.pending=null;throw result.error||Error('AI 恢复测试没有成功');}
   // The backend test clears only the original auth/quota/network reasons.
   // Budget and unknown imported pauses must never be overridden here.
   const runtime=await this.port<AiRuntime>('ai_runtime',{});
   if(runtime.paused){this.pending=null;throw Object.assign(Error(runtime.reason||'AI 仍处于暂停状态'),{code:runtime.reason_kind||'paused'});}
   this.pending=null;return runtime;
  }finally{this.running=false;}
 }
}
