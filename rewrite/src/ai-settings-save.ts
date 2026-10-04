import type {AiProfile,AiSettings} from './contracts';
export type AiSettingsRequest={id:string;settings:AiSettings;secret:string|null;expectedRevision:string};
export type AiInvoke=<T>(command:string,args?:Record<string,unknown>)=>Promise<T>;
function canonical(value:unknown):string {
  if(Array.isArray(value))return '['+value.map(canonical).join(',')+']';
  if(value!==null&&typeof value==='object')return '{'+Object.entries(value).sort(([a],[b])=>a<b?-1:a>b?1:0).map(([key,value])=>JSON.stringify(key)+':'+canonical(value)).join(',')+'}';
  return JSON.stringify(value);
}
export async function saveAiProfile(invoke:AiInvoke,request:AiSettingsRequest):Promise<AiProfile>{
  try{return await invoke<AiProfile>('save_ai_settings',request);}
  catch(error){
    const code=(error as {code?:string}|null)?.code;
    // A business rejection must never be masked by a receipt from an ID collision.
    // Only an unverified transport/worker failure triggers a read, never a second write.
    if(code&&code!=='ai-worker')throw error;
    try{
      const receipt=await invoke<AiProfile>('ai_settings_receipt',{id:request.id});
      const expected={...request.settings},saved={...receipt.settings};
      if(request.secret){expected.credential_account='';saved.credential_account='';}
      if(!receipt.revision||canonical(expected)!==canonical(saved))throw error;
      return receipt;
    }catch{throw error;}
  }
}
