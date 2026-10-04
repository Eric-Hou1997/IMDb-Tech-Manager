import test from 'node:test';
import assert from 'node:assert/strict';
import {applyPreviewEntries} from '../src/preview-approval.ts';
import type {PreviewEntry} from '../src/preview-presentation.ts';
import type {WritePreview} from '../src/contracts';
const entry=(id:string):PreviewEntry=>({row:{item:{id,path:'/media/'+id+'.nfo'}},candidate:{operation_id:id,item_id:id,after_hash:id,phase:'preview'},ai:null,error:''} as PreviewEntry);
const receipt=(id:string,phase='committed')=>({operation_id:id,item_id:id,after_hash:id,phase} as WritePreview);
test('lost write response confirms the same NFO and batch row without another AI request',async()=>{
 const current=entry('one');let applies=0;
 const failures=await applyPreviewEntries(async(command,args)=>{
  if(command==='operation_result'){assert.equal(args.id,'one');return {kind:'write',result:receipt('one')} as never;}
  assert.equal(command,'apply_batch_item');assert.deepEqual(args,{taskId:'task',writeId:'one',reviewedHash:'one'});
  if(++applies===1)throw Error('reply lost');return receipt('one') as never;
 },'task',[current],()=>false,(row,value)=>{row.candidate=value;});
 assert.equal(failures.length,0);assert.equal(applies,2);assert.equal(current.candidate!.phase,'committed');
});
test('source conflict is terminal for that candidate while other reviewed NFOs still complete',async()=>{
 const entries=[entry('changed'),entry('safe')],writes:string[]=[];
 const failures=await applyPreviewEntries(async(command,args)=>{
  if(command==='operation_result')return {kind:'write',result:receipt('changed','preview')} as never;
  writes.push(String(args.writeId));if(args.writeId==='changed')throw {code:'source-conflict',message:'TMM changed this NFO'};
  return receipt('safe') as never;
 },'task',entries,()=>false,(row,value)=>{row.candidate=value;});
 assert.deepEqual(writes,['changed','safe']);assert.equal(failures.length,1);assert.equal(failures[0].terminal,true);assert.equal(entries[1].candidate!.phase,'committed');
});
test('a committed NFO does not prove unfinished batch metadata completed',async()=>{
 const failures=await applyPreviewEntries(async(command)=>{if(command==='operation_result')return {kind:'write',result:receipt('one')} as never;throw Error('batch metadata unavailable');},'task',[entry('one')],()=>false,()=>{});
 assert.equal(failures.length,1);assert.equal(failures[0].terminal,false);
});
test('a stopped owner never starts the next reviewed write',async()=>{
 let stopped=false;const writes:string[]=[];
 await applyPreviewEntries(async(command,args)=>{assert.equal(command,'apply_batch_item');writes.push(String(args.writeId));stopped=true;return receipt('one') as never;},'task',[entry('one'),entry('two')],()=>stopped,()=>{});
 assert.deepEqual(writes,['one']);
});
