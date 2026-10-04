import type {OperationResult,WritePreview} from './contracts';
import type {PreviewEntry} from './preview-presentation';
type Port=<T>(command:string,args:Record<string,unknown>)=>Promise<T>;
export async function applyPreviewEntries(port:Port,taskId:string,entries:PreviewEntry[],stopped:()=>boolean,onReceipt:(entry:PreviewEntry,receipt:WritePreview)=>void) {
 const failures:{entry:PreviewEntry;error:unknown;terminal:boolean}[]=[];
 for(const entry of entries){
  if(stopped())break;
  const candidate=entry.candidate!;
  const matches=(value:WritePreview)=>value.operation_id===candidate.operation_id&&value.item_id===entry.row.item.id&&value.after_hash===candidate.after_hash;
  let receipt:WritePreview|null=null,failure:unknown,metadataVerified=false;
  const apply=()=>port<WritePreview>('apply_batch_item',{taskId,writeId:candidate.operation_id,reviewedHash:candidate.after_hash});
  try{
   receipt=await apply();
   if(!matches(receipt)){receipt=null;throw Error('写入回执与已审核候选不匹配');}
   metadataVerified=true;
  }catch(error){
   failure=error;
   try{
    const result=await port<OperationResult>('operation_result',{id:candidate.operation_id});
    if(result.kind==='write'&&matches(result.result))receipt=result.result;
    if(receipt&&['committed','unchanged'].includes(receipt.phase)){
     // Finish/confirm the batch row as well as the NFO receipt, using the same
     // idempotent candidate. This never plans or sends another AI request.
     const recovered=await apply();
     if(matches(recovered)){receipt=recovered;metadataVerified=true;}
    }
   }catch{/* Preserve the original failure and operation for explicit recovery. */}
  }
  if(receipt)onReceipt(entry,receipt);
  if(!metadataVerified||!receipt||!['committed','unchanged'].includes(receipt.phase)){
   const error=failure||receipt?.error||Error('候选尚未完成写入，保留原操作等待恢复');
   const code=error&&typeof error==='object'&&'code' in error?String(error.code):'';
   failures.push({entry,error,terminal:['source-conflict','unsafe-skip','review-mismatch','operation-conflict','invalid-root','invalid-scope'].includes(code)});
  }
 }
 return failures;
}
