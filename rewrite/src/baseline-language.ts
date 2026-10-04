import {uiPatterns} from './baseline-ui-patterns';
import data from './assets/baseline-languages.json' with {type:'json'};
import platformMessages from './assets/platform-presentation.json' with {type:'json'};
import type {LanguageOption,LanguageSnapshot,Locale} from './contracts';
export const languageData=data as {english:Record<string,string>;traditional:string[][];native_traditional:string[][];names:Record<string,Record<string,string>>;options:Omit<LanguageOption,'error'>[];flags:Record<string,string>};
export const languageKey='itm-baseline-language';
const phraseReplacements=Object.entries(languageData.english).filter(([key])=>key.length>=4).sort((a,b)=>b[0].length-a[0].length);
type Invoke=<T>(command:string,args?:Record<string,unknown>)=>Promise<T>;
export function stableMessageID(value:string){let hash=14695981039346656037n;for(const byte of new TextEncoder().encode(value.trim())){hash^=BigInt(byte);hash=BigInt.asUintN(64,hash*1099511628211n);}return 'legacy.'+hash.toString(16).padStart(16,'0');}
export function message(source:string,locale:string,external:Record<string,string>={}):string {
 const row=(platformMessages as Record<string,{english:string;traditional:string}>)[source.trim()];
 if(row){const value=locale==='zh-CN'?source.trim():locale==='zh-Hant'?row.traditional:locale==='en-US'?row.english:external[stableMessageID(row.english)]||row.english;return source.replace(source.trim(),value);}
 const current='v5.0.0',baseline='v4.1.0',versioned=source.includes(current);
 if(versioned)source=source.replaceAll(current,baseline);
 const finish=(value:string)=>versioned?value.replaceAll(baseline,current):value;
 if(locale==='zh-CN')return finish(source);
 if(locale==='zh-Hant'){for(const [from,to] of languageData.traditional)source=source.replaceAll(from,to);return finish(source);}
 const trimmed=source.trim(),english=languageData.english[source]||languageData.english[trimmed];
 if(!['zh-CN','zh-Hant','en-US'].includes(locale)&&english){const translated=external[stableMessageID(english)];if(translated)return finish(source===trimmed?translated:source.replace(trimmed,translated));}
 if(Object.prototype.hasOwnProperty.call(languageData.english,source))return finish(languageData.english[source]);
 if(Object.prototype.hasOwnProperty.call(languageData.english,trimmed))return finish(source.replace(trimmed,languageData.english[trimmed]));
 const local=(value:string)=>message(value,locale,external);
 const system=(value:string,fallback='The operation could not be completed. Check the Task Center for details.')=>{const text=local(value);return /[\u3400-\u9fff]/.test(text.replaceAll('侯雁泽',''))?fallback:text;};
 for(const [pattern,replacement] of uiPatterns(local,system))if(pattern.test(source))return finish(source.replace(pattern,replacement));
 for(const [from,to] of phraseReplacements)source=source.replaceAll(from,to);
 return finish(source);
}
type LanguagePresentation=Pick<Languages,'snapshot'|'text'>;
let presenter:LanguagePresentation|undefined;
export function bindPresentation(model:LanguagePresentation){presenter=model;return ()=>{if(presenter===model)presenter=undefined;};}
export function presentationText(source:string){return presenter?.text(source)||source;}
export function nativeText(chinese:string,english:string){
 const snapshot=presenter?.snapshot;
 if(!snapshot||snapshot.locale==='zh-CN')return chinese;
 if(snapshot.locale==='zh-Hant'){for(const [from,to] of languageData.native_traditional)chinese=chinese.replaceAll(from,to);return chinese;}
 return snapshot.native_messages[stableMessageID(english)]||english;
}
export class Languages {
 snapshot:LanguageSnapshot={locale:'zh-CN',options:languageData.options.map(o=>({...o,error:null})),web_messages:{},native_messages:{}};
 busy=false;downloading='';ready=false;alive=true;
 private generation=0;
 private restoring=false;
 private invoke:Invoke;private notify:(message:string)=>void;
 constructor(invoke:Invoke,notify:(message:string)=>void){this.invoke=invoke;this.notify=notify;}
 text(source:string){return message(source,this.snapshot.locale,this.snapshot.web_messages as Record<string,string>);}
 async read(){if(!this.alive)return;const generation=++this.generation;try{const value=await this.invoke<LanguageSnapshot>('language_status');if(this.alive&&generation===this.generation){this.snapshot=value;this.ready=true;}}catch(error){if(this.alive&&generation===this.generation)this.notify((error as {message?:string})?.message||String(error));}}
 async restore(){
  if(!this.alive||this.restoring)return;this.restoring=true;
  try{await this.invoke('restore_language_packs');if(this.alive)await this.read();}
  catch(error){if(this.alive)this.notify((error as {message?:string})?.message||String(error));}
  finally{this.restoring=false;}
 }
 async choose(locale:Locale):Promise<boolean>{
  const option=this.snapshot.options.find(o=>o.code===locale);if(!this.alive||this.busy||!option||!option.installed&&!option.built_in&&!option.downloadable)return false;
  ++this.generation;this.busy=true;this.downloading=!option.installed&&!option.built_in?locale:'';
  try{const value=await this.invoke<LanguageSnapshot>('choose_language',{locale});if(!this.alive)return false;++this.generation;this.snapshot=value;this.notify(this.text('应用设置已保存'));return true;}
  catch(error){if(this.alive){this.notify((error as {message?:string})?.message||String(error));await this.read();}return false;}
  finally{if(this.alive){this.busy=false;this.downloading='';}}
 }
 dispose(){this.alive=false;}
}
/** Same presentation boundaries as 4.1.0: values, paths and user text are excluded.
 * Vue owns product state; this adapter only translates text and presentation attributes. */
export function translateDocument(doc:Document,languages:LanguagePresentation){
 const sources=new WeakMap<Text,{source:string;rendered:string}>(),attributes=new WeakMap<Element,Record<string,{source:string;rendered:string}>>();
 const skip='code,textarea,[data-i18n-user],[data-fixed-language="true"],.root>span';
 const skipped=(node:Node)=>{const parent=node.nodeType===3?node.parentElement:node as Element;return !parent||!!parent.closest(skip)||['SCRIPT','STYLE'].includes(parent.tagName);};
 function text(node:Text){
  if(skipped(node))return;
  let record=sources.get(node);if(!record||node.data!==record.rendered)record={source:node.data,rendered:node.data};
  // These original lines join a translated label to NFO source/tag values.
  // Translate only the label, preserving the value bytes in every locale.
  const prefix=node.parentElement?.closest('.sourceBox')&&record.source.startsWith('IMDb 原始抓取值：')?'IMDb 原始抓取值：':node.parentElement?.closest('.tagLine:not(.warnLine)')&&record.source.startsWith('将替换：')?'将替换：':'';
  record.rendered=prefix?languages.text(prefix)+record.source.slice(prefix.length):languages.text(record.source);
  if(node.data!==record.rendered)node.data=record.rendered;sources.set(node,record);
 }
 function element(node:Element){
  if(skipped(node))return;
  let records=attributes.get(node);if(!records){records={};attributes.set(node,records);}
  if(!node.closest('[data-i18n-user-attributes]'))for(const attr of ['placeholder','title','aria-label']){
   if(!node.hasAttribute(attr))continue;const value=node.getAttribute(attr)!;
   let record=records[attr];if(!record||value!==record.rendered)record={source:value,rendered:value};
   record.rendered=languages.text(record.source);if(value!==record.rendered)node.setAttribute(attr,record.rendered);records[attr]=record;
  }
  for(const child of Array.from(node.childNodes)){if(child.nodeType===3)text(child as Text);else if(child.nodeType===1)element(child as Element);}
 }
 const observer=new MutationObserver(records=>{for(const record of records){if(record.type==='characterData')text(record.target as Text);else if(record.type==='attributes')element(record.target as Element);else for(const node of Array.from(record.addedNodes)){if(node.nodeType===3)text(node as Text);else if(node.nodeType===1)element(node as Element);}}});
 observer.observe(doc.body,{subtree:true,childList:true,characterData:true,attributes:true,attributeFilter:['placeholder','title','aria-label']});
 return {refresh(){observer.takeRecords();doc.documentElement.lang=languages.snapshot.locale;element(doc.body);observer.takeRecords();doc.defaultView?.dispatchEvent(new Event('resize'));},dispose(){observer.disconnect();}};
}
