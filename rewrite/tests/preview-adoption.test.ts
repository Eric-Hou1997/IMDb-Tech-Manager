import test from 'node:test';
import assert from 'node:assert/strict';
import {PreviewAdoptionSubmission} from '../src/preview-adoption.ts';
import type {PreviewAdoption,Task} from '../src/contracts';
const request=():PreviewAdoption=>({operation_id:'adoption',task_id:'preview',items:[{write_id:'write',reviewed_hash:'reviewed'}]});
const receipt=(extra={})=>({id:'adoption',batch:{approved:true,mode:'adopt-preview',total:1,items:[{write_id:'write',candidate_hash:'reviewed'}]},...extra}) as Task;
test('lost submission replies recover the actual owned task without front-end NFO writes',async()=>{
 const calls:string[]=[];
 const owner=new PreviewAdoptionSubmission(async(command,args)=>{calls.push(command);if(command==='adopt_preview'){assert.deepEqual(args.request,request());throw Error('reply lost after durable submission');}assert.deepEqual(args,{id:'adoption'});return receipt() as never;});
 assert.equal((await owner.run(request())).id,'adoption');assert.deepEqual(calls,['adopt_preview','task_result']);assert.equal(owner.pending,null);
});
test('unknown outcomes retain immutable reviewed hashes and reject a mismatched receipt',async()=>{
 let stage=0;
 const owner=new PreviewAdoptionSubmission(async(command,args)=>{
  if(command==='task_result')throw Error('unavailable');
  assert.deepEqual(args.request,request());stage++;
  if(stage===1)throw Error('lost');if(stage===2)return receipt({id:'other'}) as never;return receipt() as never;
 });
 const initial=request();await assert.rejects(owner.run(initial));initial.items[0]!.reviewed_hash='changed';
 await assert.rejects(owner.run(initial),/不匹配/);assert.equal((await owner.run(initial)).id,'adoption');
});
test('known stale review cannot leave a writable pending adoption',async()=>{
 const owner=new PreviewAdoptionSubmission(async(command)=>{throw command==='adopt_preview'?{code:'review-mismatch'}:Error('not submitted');});
 await assert.rejects(owner.run(request()));assert.equal(owner.pending,null);
});
