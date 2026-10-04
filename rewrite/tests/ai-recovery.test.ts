import test from 'node:test';
import assert from 'node:assert/strict';
import {AiRecovery} from '../src/ai-recovery.ts';
const record=(id:string,extra={})=>({request:{operation_id:id},purpose:'connection-test',phase:'review-ready',error:null,meter:{attempts:1},...extra});
test('lost connection and runtime replies reuse one real test without blindly clearing pause',async()=>{
 let testID='',reads=0;
 const owner=new AiRecovery(async(command,args)=>{
  if(command==='test_ai_connection'){testID||=String(args.id);assert.equal(args.id,testID);throw Error('reply lost');}
  if(command==='ai_record'){assert.equal(args.id,testID);return record(testID) as never;}
  assert.equal(command,'ai_runtime');reads++;if(reads===1)throw Error('runtime reply lost');return {paused:false} as never;
 });
 await assert.rejects(owner.run());assert.equal((await owner.run()).paused,false);assert.equal(reads,2);
});
test('failed, unmetered or unrelated tests cannot clear runtime pause',async()=>{
 for(const extra of [{phase:'failed',error:{code:'auth'}},{meter:{attempts:0}},{purpose:'generate'}]){
  let resumes=0;
  const owner=new AiRecovery(async(command,args)=>{if(command==='ai_runtime'){resumes++;return {paused:false} as never;}return record(String(args.id),extra) as never;});
  await assert.rejects(owner.run());assert.equal(resumes,0);
 }
});
test('successful real tests preserve budget and unknown imported pause reasons',async()=>{
 for(const reason_kind of ['budget','legacy-runtime-unverified']){
  const commands:string[]=[];
  const owner=new AiRecovery(async(command,args)=>{commands.push(command);return (command==='test_ai_connection'?record(String(args.id)):{paused:true,reason_kind}) as never;});
  await assert.rejects(owner.run(),{code:reason_kind});assert.deepEqual(commands,['test_ai_connection','ai_runtime']);
 }
});
test('a repeated Recover click cannot start a second request while the first is owned',async()=>{
 let release:()=>void=()=>{};const pending=new Promise<void>(resolve=>release=resolve);let tests=0;
 const owner=new AiRecovery(async(command,args)=>{if(command==='test_ai_connection'){tests++;await pending;return record(String(args.id)) as never;}return {paused:false} as never;});
 const first=owner.run();await assert.rejects(owner.run(),/正在恢复/);release();await first;assert.equal(tests,1);
});
