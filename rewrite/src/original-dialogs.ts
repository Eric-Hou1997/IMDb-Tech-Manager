import {presentationText,nativeText} from './baseline-language';
import {invoke} from '@tauri-apps/api/core';
export async function originalPrompt(platform:string,message:string,value=''):Promise<string|null> {
 if(platform!=='macos')return window.prompt(presentationText(message),value);
 return invoke<string|null>('native_dialog',{message:presentationText(message),defaultValue:value,confirm:nativeText('确认','Confirm'),cancel:nativeText('取消','Cancel')});
}
export async function originalConfirm(message:string):Promise<boolean> {
 return (await invoke<string|null>('native_dialog',{message:presentationText(message),defaultValue:null,confirm:nativeText('确认','Confirm'),cancel:nativeText('取消','Cancel')}))!==null;
}
