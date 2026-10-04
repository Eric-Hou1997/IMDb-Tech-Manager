import test from 'node:test';
import assert from 'node:assert/strict';
import {Presentation,externalLocales} from '../src/presentation.ts';
import type {LanguagePack,PresentationCatalog} from '../src/contracts';
function setup(){
 const catalog:PresentationCatalog={schema:2,product:'itm',app_version:'v5.0.0',message_set_hash:'a'.repeat(64),messages:{close:{text:'关闭',protected:[]},done:{text:'{count} 个 NFO：{title}',protected:['NFO']}},external:{'fr-FR':{locale:'fr-FR',revision:1,asset:'ITM-Language-fr-FR-r1.json',released_with:'v4.1.0',sha256:'b'.repeat(64)}}};
 for(const locale of externalLocales)catalog.external[locale]??={locale,revision:1,asset:`ITM-Language-${locale}-r1.json`,released_with:'v4.1.0',sha256:'b'.repeat(64)};
 const builtins={'zh-CN':{close:'关闭',done:'{count} 个 NFO：{title}'},'zh-Hant':{close:'關閉',done:'{count} 個 NFO：{title}'},'en-US':{close:'Close',done:'{count} NFO: {title}'}};
 const pack:LanguagePack={schema:2,product:'itm',locale:'fr-FR',revision:1,message_set_hash:catalog.message_set_hash,messages:{close:'Fermer',done:'{count} NFO : {title}'}};
 return {catalog,builtins,pack};
}
test('uninstalled language cannot activate and failed packs cannot replace the current one',()=>{
 const {catalog,builtins,pack}=setup();const view=new Presentation(catalog,builtins);
 assert.throws(()=>view.use('fr-FR'),/language-pack-required/);assert.equal(view.locale,'zh-CN');view.acceptVerifiedPack(pack);view.use('fr-FR');assert.equal(view.message('close'),'Fermer');
 for(const changed of [{...pack,product:'tcm'},{...pack,revision:2},{...pack,messages:{close:'Partial'}},{...pack,messages:{...pack.messages,done:'{count} films : {title}'}}])assert.throws(()=>view.acceptVerifiedPack(changed));
 assert.equal(view.message('close'),'Fermer');assert.throws(()=>view.acceptVerifiedPack({...pack,locale:'en-US'}));
});
test('language switching never translates or re-interpolates media data and task context stays fixed',()=>{
 const {catalog,builtins,pack}=setup();const view=new Presentation(catalog,builtins);view.acceptVerifiedPack(pack);view.use('fr-FR');const task=view.capture();
 const title=' 电影 <tag>Manual</tag> {count} C:\\媒体\\IMDb.nfo ';view.use('en-US');assert.equal(view.message('close'),'Close');assert.equal(task('done',{count:2,title}),`2 NFO : ${title}`);
 pack.messages.close='Changed';assert.equal(task('close'),'Fermer');assert.equal(view.message('close',{},'fr-FR'),'Fermer');
});
test('missing arguments, unknown messages and malformed templates fail explicitly',()=>{
 const {catalog,builtins,pack}=setup();const view=new Presentation(catalog,builtins);assert.throws(()=>view.message('done',{count:1}),/language-parameters/);assert.throws(()=>view.message('close',{extra:'data'}),/language-parameters/);assert.throws(()=>view.message('unknown'),/language-message-missing/);
 for(const done of ['{count NFO {title}','{count} NFO {path}','{count} NFO {title}\u202e'])assert.throws(()=>view.acceptVerifiedPack({...pack,messages:{...pack.messages,done}}));
 assert.throws(()=>view.message('done',{count:Infinity,title:'x'}),/language-parameters/);
});
test('all builtins require complete coverage and source mutation cannot change registered templates',()=>{
 const {catalog,builtins}=setup();assert.throws(()=>new Presentation(catalog,{'zh-CN':builtins['zh-CN']}),/language-coverage/);
 const view=new Presentation(catalog,builtins);builtins['zh-CN'].close='Mutated';catalog.messages.close!.text='Mutated';assert.equal(view.message('close'),'关闭');
});
