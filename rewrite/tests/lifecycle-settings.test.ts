import test from 'node:test';
import assert from 'node:assert/strict';
import {LifecycleSettings,type LifecycleInvoke,type LifecycleStatus} from '../src/lifecycle-settings.ts';
const initial=():LifecycleStatus=>({settings:{revision:0,close_action:'quit',launch_at_login:false,start_hidden:true},native_autostart:false,background_mode:'hide',tray_available:true,closing:false,error:null});
function transport(work:(command:string,args?:Record<string,unknown>)=>unknown){return (async(command,args)=>work(command,args)) as LifecycleInvoke;}
test('a failed initial read cannot save invented default settings and can be retried',async()=>{
 let fail=true,saves=0;const model=new LifecycleSettings(transport(cmd=>{if(cmd==='lifecycle_apply')saves++;if(fail)throw Error('unavailable');return initial();}));
 await model.read();model.settings.launch_at_login=true;await model.apply();assert.equal(saves,0);assert.equal(model.baseline,null);assert.equal(model.error,'unavailable');
 fail=false;await model.read();assert.equal(model.settings.launch_at_login,false);assert.equal(model.error,'');
});
test('lost apply reply retries the exact operation and committed settings survive status failure',async()=>{
 let calls:unknown[]=[];let failStatus=false;const model=new LifecycleSettings(transport((cmd,args)=>{
  if(cmd==='lifecycle_status'){if(failStatus)throw Error('query unavailable');return initial();}
  calls.push(structuredClone(args));if(calls.length===1)throw Error('reply lost');failStatus=true;return {phase:'committed',desired:{...(args?.settings as object),revision:1}};
 }),()=> 'same-operation');
 await model.read();model.settings.launch_at_login=true;await model.apply();assert.equal(model.dirty,true);await model.apply();assert.deepEqual(calls[0],calls[1]);assert.equal(model.dirty,false);assert.equal(model.settings.revision,1);assert.equal(model.verified,false);assert.equal(model.error,'query unavailable');
});
test('changing the draft after a failure creates a new operation and explicit read discards it',async()=>{
 let ids=0;const calls:unknown[]=[];const model=new LifecycleSettings(transport((cmd,args)=>{if(cmd==='lifecycle_status')return initial();calls.push(structuredClone(args));throw Error('failed');}),()=>String(++ids));
 await model.read();model.settings.launch_at_login=true;await model.apply();model.settings.start_hidden=false;await model.apply();assert.notEqual((calls[0] as {id:string}).id,(calls[1] as {id:string}).id);
 await model.read();assert.equal(model.dirty,false);assert.equal(model.pending,null);
});
test('duplicate in-flight calls and closing with an unsaved draft are suppressed',async()=>{
 let release:((value:unknown)=>void)|undefined,calls=0;
 const model=new LifecycleSettings(transport(cmd=>{if(cmd==='lifecycle_status')return initial();calls++;return new Promise(resolve=>release=resolve);}));
 await model.read();model.settings.close_action='background';await model.window('quit_probe');assert.equal(calls,0);
 const first=model.apply();await model.apply();assert.equal(calls,1);release!({phase:'committed',desired:{...model.settings,revision:1}});await first;
});
test('window errors are visible and late reads cannot mutate a disposed panel',async()=>{
 let release:((value:unknown)=>void)|undefined;
 const model=new LifecycleSettings(transport(cmd=>{if(cmd==='background_window')throw Error('window-unavailable');return new Promise(resolve=>release=resolve);}));
 await model.window('background_window');assert.equal(model.error,'window-unavailable');
 const pending=model.read();model.dispose();release!(initial());await pending;assert.equal(model.baseline,null);assert.equal(model.status,null);
});
