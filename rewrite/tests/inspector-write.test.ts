import test from 'node:test';
import assert from 'node:assert/strict';
import {InspectorWrite} from '../src/inspector-write.ts';
import type {WritePreview} from '../src/contracts';
const row=(phase:string):WritePreview=>({operation_id:'same-edit',after_hash:'candidate',phase,error:null} as WritePreview);
test('lost candidate and commit responses resolve the same persisted receipt',async()=>{
 let prepared=0,applied=0,lookups=0,committed=false;
 const writer=new InspectorWrite(async(command,args)=>{
  assert.equal(args.id,'same-edit');
  if(command==='operation_result'){lookups++;return {kind:'write',result:row(committed?'committed':'preview')} as never;}
  assert.equal(command,'apply_specs');assert.equal(args.reviewedHash,'candidate');applied++;committed=true;throw Error('response lost');
 });
 const result=await writer.run({id:'same-edit',prepare:async()=>{prepared++;throw Error('candidate response lost');}});
 assert.equal(result.phase,'committed');assert.equal(prepared,1);assert.equal(applied,1);assert.equal(lookups,2);assert.equal(writer.pending,null);
});
test('partial mirror recovery retains the operation and cannot start another edit',async()=>{
 let phase='preview',applies=0,firstPrepares=0,secondPrepares=0;
 const writer=new InspectorWrite(async(command,args)=>{
  assert.equal(args.id,'same-edit');
  if(command==='operation_result')return {kind:'write',result:row(phase)} as never;
  applies++;if(applies===1){phase='committed-mirror-pending';throw {code:'ownership-mirror',message:'Mirror unavailable'};}
  phase='committed';return row(phase) as never;
 });
 await assert.rejects(writer.run({id:'same-edit',prepare:async()=>{firstPrepares++;return row(phase);}}));
 assert.equal(writer.pending?.id,'same-edit');
 const result=await writer.run({id:'other-edit',prepare:async()=>{secondPrepares++;return row('preview');}});
 assert.equal(result.operation_id,'same-edit');assert.equal(firstPrepares,2);assert.equal(secondPrepares,0);assert.equal(applies,2);assert.equal(writer.pending,null);
});
test('source conflict leaves no retry intent that could overwrite a refreshed NFO',async()=>{
 const writer=new InspectorWrite(async()=>{throw Error('operation does not exist');});
 await assert.rejects(writer.run({id:'old',prepare:async()=>{throw {code:'source-conflict',message:'NFO changed'};}}));
 assert.equal(writer.pending,null);
 const result=await writer.run({id:'new',prepare:async()=>row('unchanged')});
 assert.equal(result.phase,'unchanged');assert.equal(writer.pending,null);
});
test('a second click cannot replace the owner of an in-flight edit',async()=>{
 let complete:(value:WritePreview)=>void=()=>{};
 const prepared=new Promise<WritePreview>(resolve=>{complete=resolve;});
 const writer=new InspectorWrite(async()=>row('committed') as never);
 const first=writer.run({id:'same-edit',prepare:()=>prepared});
 await assert.rejects(writer.run({id:'other',prepare:async()=>row('preview')}),/正在进行/);
 complete(row('preview'));assert.equal((await first).phase,'committed');assert.equal(writer.pending,null);
});
