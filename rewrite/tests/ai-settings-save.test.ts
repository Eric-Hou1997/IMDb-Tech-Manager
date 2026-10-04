import test from 'node:test';
import assert from 'node:assert/strict';
import {saveAiProfile} from '../src/ai-settings-save.ts';
import type {AiInvoke,AiSettingsRequest} from '../src/ai-settings-save';
import type {AiProfile,AiSettings} from '../src/contracts';
const settings=()=>({config:{model:'fixture',extra_body:{z:1,a:2}},credential_account:'old'}) as AiSettings;
const request=():AiSettingsRequest=>({id:'same-operation',settings:settings(),secret:null,expectedRevision:'old-revision'});
const receipt=():AiProfile=>({settings:settings(),revision:'saved-revision',credential_ready:false,credential_error:null});
const transport=(work:(command:string,args?:Record<string,unknown>)=>unknown)=>(async(command,args)=>work(command,args)) as AiInvoke;
test('successful save uses its authoritative receipt without another profile read',async()=>{
 const calls:string[]=[];const value=await saveAiProfile(transport(command=>{calls.push(command);return receipt();}),request());assert.equal(value.revision,'saved-revision');assert.deepEqual(calls,['save_ai_settings']);
});
test('lost IPC reply reads the same operation and never repeats the write',async()=>{
 const calls:unknown[]=[];const value=await saveAiProfile(transport((command,args)=>{calls.push([command,args]);if(command==='save_ai_settings')throw Error('reply lost');const value=receipt();value.settings.config.extra_body={a:2,z:1};return value;}),request());
 assert.equal(value.revision,'saved-revision');assert.deepEqual(calls.map((call:any)=>call[0]),['save_ai_settings','ai_settings_receipt']);assert.deepEqual((calls[1] as any[])[1],{id:'same-operation'});
});
test('structured conflicts and absent or mismatched receipts retain the original failure',async()=>{
 const conflict={code:'operation-conflict',message:'different payload'};let calls=0;
 await assert.rejects(saveAiProfile(transport(()=>{calls++;throw conflict;}),request()),error=>error===conflict);assert.equal(calls,1);
 for(const missing of [true,false]){const original=Error('lost');await assert.rejects(saveAiProfile(transport(command=>{if(command==='save_ai_settings'||missing)throw original;const value=receipt();value.settings.config.model='different';return value;}),request()),error=>error===original);}
});
test('new credential references and denied credential reads do not negate a committed save',async()=>{
 const input=request();input.secret='non-secret-fixture';const saved=receipt();saved.settings.credential_account='new-account';saved.credential_error={code:'credential-read',message:'denied',path:null,operation_id:null,retryable:false};
 const value=await saveAiProfile(transport(command=>{if(command==='save_ai_settings')throw Error('lost');return saved;}),input);assert.equal(value.credential_error?.code,'credential-read');assert.equal(value.settings.credential_account,'new-account');
});
