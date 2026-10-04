import test from 'node:test';
import assert from 'node:assert/strict';
import {BatchSubmission} from '../src/batch-submission.ts';
import type {BatchRequest,Task} from '../src/contracts';
const request=(id='original'):BatchRequest=>({operation_id:id,space:'movie',item_ids:['one','two'],root_ids:[],engine:'ai',mode:'preview',retry_failed:false});
const task=(approved=false):Task=>({id:'original',state:approved?'requested':'paused',batch:{approved,plan_hash:'bound-scope'}} as Task);
test('lost plan and launch replies reuse the original operation and scope',async()=>{
 let plans=0,launches=0;
 const owner=new BatchSubmission(async(command,args)=>{
  if(command==='plan_batch'){
   assert.deepEqual(args.request,request());plans++;
   if(plans===1)throw Error('reply lost after durable plan');
   return task() as never;
  }
  assert.deepEqual(args,{id:'original',reviewedHash:'bound-scope'});launches++;
  if(launches===1)throw Error('reply lost after launch');
  return task(true) as never;
 });
 const initial=request();await assert.rejects(owner.run(initial));initial.item_ids.push('changed');
 await assert.rejects(owner.run(request('other')));
 assert.equal((await owner.run(request('third'))).id,'original');assert.equal(owner.pending,null);
 assert.equal(plans,3);assert.equal(launches,2);
});
test('known rejected scope cannot retain an AI retry intent',async()=>{
 const owner=new BatchSubmission(async()=>{throw {code:'ai-disabled'};});
 await assert.rejects(owner.run(request()));assert.equal(owner.pending,null);
});
test('a different plan receipt cannot authorize a batch',async()=>{
 let approvals=0;
 const owner=new BatchSubmission(async(command)=>{if(command==='approve_batch_scope')approvals++;return {...task(),id:'other'} as never;});
 await assert.rejects(owner.run(request()),/回执/);assert.equal(approvals,0);
});
test('repeated click cannot change the owner while a plan is pending',async()=>{
 let release:(value:Task)=>void=()=>{};
 const pending=new Promise<Task>(resolve=>{release=resolve;});
 const owner=new BatchSubmission(async command=>(command==='plan_batch'?await pending:task(true)) as never);
 const first=owner.run(request());await assert.rejects(owner.run(request('other')),/正在提交/);
 release(task());assert.equal((await first).id,'original');
});
