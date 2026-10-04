import test from 'node:test';
import assert from 'node:assert/strict';
import {AutomaticSettingsModel} from '../src/automatic-settings.ts';
import type {AutomaticStatus} from '../src/contracts';
import type {LifecycleInvoke} from '../src/lifecycle-settings';
const status=(enabled=false):AutomaticStatus=>({settings:{interval_seconds:60,on_app_start:false},enabled,cycle:0,next_due:0,task_ids:[]});
function transport(work:(command:string,args?:Record<string,unknown>)=>unknown){return (async(command,args)=>work(command,args)) as LifecycleInvoke;}
test('initial failure can recover and invalid unsaved settings do not prevent stopping',async()=>{
 let fail=true;let received:Record<string,unknown>|undefined;
 const model=new AutomaticSettingsModel(transport((cmd,args)=>{if(cmd==='automatic_status'){if(fail)throw Error('unavailable');return status(true);}received=args;return status(false);}));
 await model.refresh();assert.equal(model.status,null);fail=false;await model.refresh(true);model.settings.interval_seconds=1;
 await model.apply(false,false);assert.equal((received?.settings as {interval_seconds:number}).interval_seconds,60);assert.equal(model.settings.interval_seconds,1);assert.equal(model.dirty,true);assert.equal(model.status?.enabled,false);
});
test('a late poll cannot overwrite the result of a start operation',async()=>{
 let reads=0,release:((value:unknown)=>void)|undefined;
 const model=new AutomaticSettingsModel(transport(cmd=>{if(cmd==='automatic_apply')return status(true);if(++reads===1)return status();return new Promise(resolve=>release=resolve);}));
 await model.refresh(true);const poll=model.refresh();await model.apply(true);release!(status(false));await poll;assert.equal(model.status?.enabled,true);
});
test('unsaved edits survive polling and changed saved settings expose a conflict',async()=>{
 let current=status();const model=new AutomaticSettingsModel(transport(()=>structuredClone(current)));
 await model.refresh(true);model.settings.interval_seconds=300;await model.refresh();assert.equal(model.settings.interval_seconds,300);assert.equal(model.conflict,false);
 current.settings.interval_seconds=900;await model.refresh();assert.equal(model.settings.interval_seconds,300);assert.equal(model.conflict,true);await model.refresh(true);assert.equal(model.settings.interval_seconds,900);assert.equal(model.dirty,false);
});
test('lost reply repeats the original expected state and operation identity',async()=>{
 const calls:unknown[]=[];const model=new AutomaticSettingsModel(transport((cmd,args)=>{if(cmd==='automatic_status')return status();calls.push(structuredClone(args));if(calls.length===1)throw Error('reply lost');return status(true);}),()=> 'operation');
 await model.refresh(true);await model.apply(true);await model.apply(true);assert.deepEqual(calls[0],calls[1]);assert.equal(model.status?.enabled,true);assert.equal(model.pending,null);
});
test('disposed panels ignore a late poll',async()=>{
 let release:((value:unknown)=>void)|undefined;const model=new AutomaticSettingsModel(transport(()=>new Promise(resolve=>release=resolve)));
 const pending=model.refresh();model.dispose();release!(status(true));await pending;assert.equal(model.status,null);
});
