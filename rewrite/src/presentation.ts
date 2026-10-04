import type {LanguageMessage,LanguagePack,PresentationCatalog} from './contracts';
const product='itm';
export const builtInLocales=['zh-CN','zh-Hant','en-US'] as const;
export const externalLocales=['fr-FR','ru-RU','ja-JP','es-ES','th-TH'] as const;
export type MessageValues=Record<string,string|number>;
type Part={text:string}|{parameter:string};
type Dictionary=Map<string,Part[]>;
function failure(code:string):never {throw Object.assign(new Error(code),{code});}
function parse(template:string):Part[]{
 const result:Part[]=[];let text='';const chars=[...template];
 for(let i=0;i<chars.length;i++){
  const c=chars[i];
  if((c==='{'||c==='}')&&chars[i+1]===c){text+=c;i++;continue;}
  if(c==='}')failure('language-placeholder-invalid');
  if(c!=='{'){text+=c;continue;}
  if(text){result.push({text});text='';}
  let name='';while(++i<chars.length&&chars[i]!=='}')name+=chars[i];
  if(i===chars.length||!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name))failure('language-placeholder-invalid');
  result.push({parameter:name});
 }
 if(text)result.push({text});return result;
}
const parameters=(parts:Part[])=>parts.flatMap(part=>'parameter' in part?[part.parameter]:[]).sort();
function dictionary(definitions:Map<string,LanguageMessage>,messages:Record<string,string|undefined>):Dictionary{
 if(JSON.stringify([...definitions.keys()].sort())!==JSON.stringify(Object.keys(messages).sort()))failure('language-coverage');
 const result:Dictionary=new Map();
 for(const [key,source] of definitions){
  const text=messages[key];if(typeof text!=='string'||!text.trim()||new TextEncoder().encode(text).byteLength>16384||/[\u0000-\u0008\u000b-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/u.test(text))failure('language-format');
  const translated=parse(text);
  if(JSON.stringify(parameters(translated))!==JSON.stringify(parameters(parse(source.text))))failure('language-placeholder-invalid');
  for(const token of source.protected)if(!token||text.split(token).length!==source.text.split(token).length)failure('language-protected-token');
  result.set(key,translated);
 }
 return result;
}
function format(messages:Dictionary,key:string,values:MessageValues={}):string{
 const parts=messages.get(key);if(!parts)failure('language-message-missing');
 const expected=[...new Set(parameters(parts))];
 if(JSON.stringify(expected)!==JSON.stringify(Object.keys(values).sort())||Object.values(values).some(value=>typeof value!=='string'&&(typeof value!=='number'||!Number.isFinite(value))))failure('language-parameters');
 return parts.map(part=>'text' in part?part.text:String(values[part.parameter])).join('');
}
/** Catalog/pack bytes must first be validated by the native presentation module.
 * This layer stores text templates only; dynamic values are inserted once and
 * must be rendered through Vue text bindings, never through v-html/innerHTML. */
export class Presentation {
 locale='zh-CN';
 private readonly catalog:PresentationCatalog;
 private readonly definitions:Map<string,LanguageMessage>;
 private readonly dictionaries=new Map<string,Dictionary>();
 constructor(catalog:PresentationCatalog,builtins:Record<string,Record<string,string>>){
  if(catalog.schema!==2||catalog.product!==product||!/^[a-f0-9]{64}$/.test(catalog.message_set_hash))failure('language-catalog-invalid');
  if(JSON.stringify(Object.keys(catalog.external).sort())!==JSON.stringify([...externalLocales].sort()))failure('language-catalog-invalid');
  this.catalog=JSON.parse(JSON.stringify(catalog));
  this.definitions=new Map(Object.entries(this.catalog.messages).map(([key,value])=>{if(!value)failure('language-catalog-invalid');return [key,value];}));
  if(!this.definitions.size)failure('language-catalog-invalid');
  for(const locale of builtInLocales){if(!builtins[locale])failure('language-coverage');this.dictionaries.set(locale,dictionary(this.definitions,builtins[locale]));}
 }
 acceptVerifiedPack(pack:LanguagePack){
  const descriptor=this.catalog.external[pack.locale];
  if(!externalLocales.some(locale=>locale===pack.locale)||!descriptor||pack.schema!==2||pack.product!==product||pack.message_set_hash!==this.catalog.message_set_hash||pack.revision!==descriptor.revision)failure('language-incompatible');
  const candidate=dictionary(this.definitions,pack.messages);
  this.dictionaries.set(pack.locale,candidate);
 }
 available(locale:string){return this.dictionaries.has(locale);}
 use(locale:string){if(!this.available(locale))failure('language-pack-required');this.locale=locale;}
 message(key:string,values:MessageValues={},locale=this.locale){return this.capture(locale)(key,values);}
 capture(locale=this.locale){
  const messages=this.dictionaries.get(locale);if(!messages)failure('language-pack-required');
  // Keep the exact template set for work that has already started, even when
  // the visible language or a presentation resource changes afterwards.
  return (key:string,values:MessageValues={})=>format(messages,key,values);
 }
}
