import type {AppError,Settings,SettingsOperation} from './contracts';
export interface LifecycleStatus {settings:Settings;native_autostart:boolean|null;background_mode:string;tray_available:boolean;closing:boolean;error:AppError|null}
export type LifecycleInvoke=<T>(command:string,args?:Record<string,unknown>)=>Promise<T>;
const defaults=():Settings=>({revision:0,close_action:'quit',launch_at_login:false,start_hidden:true});
export class LifecycleSettings {
 status:LifecycleStatus|null=null;
 settings=defaults();
 baseline:Settings|null=null;
 busy=false;
 error='';
 closing=false;
 alive=true;
 verified=false;
 pending:{id:string;settings:Settings}|null=null;
 invoke:LifecycleInvoke;
 identity:()=>string;
 constructor(invoke:LifecycleInvoke,identity=()=>crypto.randomUUID()){this.invoke=invoke;this.identity=identity;}
 get dirty(){return this.baseline!==null&&JSON.stringify(this.settings)!==JSON.stringify(this.baseline);}
 report(e:unknown){if(!this.alive)return;this.error=e&&typeof e==='object'&&'message' in e?String(e.message):String(e);}
 async read(){
  if(this.busy||!this.alive)return;this.busy=true;this.error='';this.verified=false;
  try{const value=await this.invoke<LifecycleStatus>('lifecycle_status');if(!this.alive)return;this.status=value;this.baseline={...value.settings};this.settings={...value.settings};this.closing=value.closing;this.pending=null;this.verified=true;}
  catch(e){this.report(e);}finally{this.busy=false;}
 }
 async apply(){
  if(this.busy||!this.alive||!this.dirty)return;this.busy=true;this.error='';
  if(!this.pending||JSON.stringify(this.pending.settings)!==JSON.stringify(this.settings))this.pending={id:this.identity(),settings:{...this.settings}};
  try{
   const result=await this.invoke<SettingsOperation>('lifecycle_apply',this.pending);
   if(!this.alive)return;
   if(result.phase!=='committed')throw result.error??Error('系统设置结果尚未确认，请重新读取后核对');
   this.baseline={...result.desired};this.settings={...result.desired};this.pending=null;this.verified=false;
   const status=await this.invoke<LifecycleStatus>('lifecycle_status');if(!this.alive)return;
   this.status=status;this.verified=true;this.closing=status.closing;if(JSON.stringify(status.settings)!==JSON.stringify(this.baseline))this.error='系统设置已在其他位置更改，请重新读取后核对';
  }catch(e){this.report(e);}finally{this.busy=false;}
 }
 async window(command:'background_window'|'quit_probe'){
  if(this.busy||!this.alive||this.dirty||this.closing)return;this.busy=true;this.error='';
  try{await this.invoke(command);}catch(e){this.report(e);}finally{this.busy=false;}
 }
 dispose(){this.alive=false;}
}
