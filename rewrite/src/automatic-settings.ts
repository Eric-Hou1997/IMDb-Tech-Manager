import type {AutomaticSettings,AutomaticStatus,AutomaticExpected} from './contracts';
import type {LifecycleInvoke} from './lifecycle-settings';
export class AutomaticSettingsModel {
 status:AutomaticStatus|null=null;
 settings:AutomaticSettings={interval_seconds:60,on_app_start:false};
 baseline:AutomaticSettings|null=null;
 busy=false;polling=false;error='';conflict=false;alive=true;generation=0;
 invoke:LifecycleInvoke;identity:()=>string;
 pending:{id:string;settings:AutomaticSettings;enabled:boolean;expected:AutomaticExpected;fingerprint:string}|null=null;
 constructor(invoke:LifecycleInvoke,identity=()=>crypto.randomUUID()){this.invoke=invoke;this.identity=identity;}
 get dirty(){return this.baseline!==null&&JSON.stringify(this.settings)!==JSON.stringify(this.baseline);}
 get valid(){return Number.isInteger(this.settings.interval_seconds)&&this.settings.interval_seconds>=30&&this.settings.interval_seconds<=86400;}
 report(e:unknown){if(!this.alive)return;this.error=e&&typeof e==='object'&&'message' in e?String(e.message):String(e);if(e&&typeof e==='object'&&'code' in e&&e.code==='automatic-settings-conflict')this.conflict=true;}
 async refresh(discard=false){
  if(this.polling||this.busy||!this.alive)return;this.polling=true;const token=this.generation;
  try{const value=await this.invoke<AutomaticStatus>('automatic_status');if(!this.alive||token!==this.generation)return;
   this.status=value;
   if(discard||!this.dirty){this.settings={...value.settings};this.baseline={...value.settings};this.conflict=false;if(discard){this.pending=null;this.error='';}}
   else if(JSON.stringify(value.settings)!==JSON.stringify(this.baseline))this.conflict=true;
  }catch(e){if(token===this.generation)this.report(e);}finally{this.polling=false;}
 }
 async apply(enabled:boolean,useDraft=true){
  if(this.busy||!this.alive||!this.status||useDraft&&(!this.valid||this.conflict))return;this.busy=true;this.error='';++this.generation;
  const settings={...(useDraft?this.settings:this.status.settings)};
  const fingerprint=JSON.stringify([settings,enabled]);
  if(this.pending?.fingerprint!==fingerprint)this.pending={id:this.identity(),settings,enabled,expected:{settings:{...this.status.settings},enabled:this.status.enabled},fingerprint};
  try{const {fingerprint:_,...request}=this.pending;const value=await this.invoke<AutomaticStatus>('automatic_apply',request);if(!this.alive)return;
   this.status=value;this.baseline={...value.settings};if(useDraft)this.settings={...value.settings};this.conflict=false;this.pending=null;
  }catch(e){this.report(e);}finally{this.busy=false;}
 }
 dispose(){this.alive=false;++this.generation;}
}
