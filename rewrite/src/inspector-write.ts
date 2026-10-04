import type {OperationResult,WritePreview} from './contracts';
type Invoke = <T>(command:string,args:Record<string,unknown>)=>Promise<T>;
export type InspectorEdit = {id:string;prepare:()=>Promise<WritePreview>};
const retryPhases=['preview','writing','committed-index-pending','committed-mirror-pending','metadata-pending'];
const terminalCodes=['source-conflict','unsafe-skip','invalid-spec-field','invalid-tag','invalid-action','operation-conflict','unsupported-write-kind'];

// One original Inspector edit owns one operation identity from candidate to
// commit. An uncertain response cannot turn the next gesture into a new write.
export class InspectorWrite {
 pending:InspectorEdit|null=null;
 private running=false;
 private invoke:Invoke;
 constructor(invoke:Invoke){this.invoke=invoke;}
 async run(edit:InspectorEdit):Promise<WritePreview>{
  if(this.running)throw Error('Inspector 写入正在进行');
  this.running=true;
  this.pending ||= edit;
  const current=this.pending;
  let preview:WritePreview|undefined;
  try {
   try {preview=await current.prepare();}
   catch(error){
    const recovered=await this.receipt(current.id);
    if(!recovered)throw error;
    preview=recovered;
   }
   if(retryPhases.includes(preview.phase)) {
    try {preview=await this.invoke<WritePreview>('apply_specs',{id:current.id,reviewedHash:preview.after_hash});}
    catch(error){
     const recovered=await this.receipt(current.id);
     if(recovered?.phase!=='committed')throw error;
     preview=recovered;
    }
   }
   if(!['committed','unchanged'].includes(preview.phase)) {
    if(!retryPhases.includes(preview.phase))this.pending=null;
    throw preview.error||Error('NFO 写入未完成：'+preview.phase);
   }
   this.pending=null;
   return preview;
  } catch(error){
   if(error&&typeof error==='object'&&'code' in error&&terminalCodes.includes(String(error.code)))this.pending=null;
   throw error;
  } finally{this.running=false;}
 }
 async retry():Promise<WritePreview>{if(!this.pending)throw Error('没有待恢复的 Inspector 写入');return this.run(this.pending);}
 private async receipt(id:string):Promise<WritePreview|null>{
  try {const value=await this.invoke<OperationResult>('operation_result',{id});return value.kind==='write'?value.result:null;}
  catch{return null;}
 }
}
