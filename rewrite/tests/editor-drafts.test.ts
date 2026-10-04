import test from 'node:test';
import assert from 'node:assert/strict';
import {SpecsDraft,TagDraft} from '../src/editor-drafts.ts';
import type {MediaItem,Tag} from '../src/contracts';
const tag=(value:string,ownership='external')=>({value,ownership,engine:''}) as Tag;
const item=(specs:Record<string,string[]>,tags:Tag[]=[])=>( {id:'item',specs,tags} as MediaItem );
test('unrelated tag writes preserve spec drafts; changed specs require a new explicit review',()=>{
 const model=new SpecsDraft();model.receive(item({Camera:['Before']}));model.values.Camera='Draft';
 model.receive(item({Camera:['Before']},[tag('Added')]));assert.equal(model.values.Camera,'Draft');assert.equal(model.conflict,false);
 model.receive(item({Camera:['External']}));assert.equal(model.values.Camera,'Draft');assert.equal(model.conflict,true);
 model.acknowledge();assert.equal(model.conflict,false);assert.equal(model.dirty,true);
 model.receive(item({Camera:['Draft']}));assert.equal(model.dirty,false);
});
test('unchanged refresh and committed normalized specs do not retain false dirty state',()=>{
 const model=new SpecsDraft();model.receive(item({Camera:['Before']}));model.values.Camera='  Written\n\n';
 model.receive(item({Camera:['Written']}));assert.equal(model.values.Camera,'Written');assert.equal(model.dirty,false);
 model.values.Camera='Draft';model.discard(item({Camera:['Actual']}));assert.equal(model.values.Camera,'Actual');assert.equal(model.dirty,false);
});
test('switching selected tags cannot silently discard edits',()=>{
 const model=new TagDraft();model.receive(item({},[tag('A'),tag('B')]));model.choose(0);model.value='edited';model.choose(0);assert.equal(model.value,'edited');model.choose(1);
 assert.equal(model.selected,0);assert.equal(model.value,'edited');assert.equal(model.pendingSelection,1);
 model.pendingSelection=null;assert.equal(model.dirty,true);model.choose(1,true);assert.equal(model.value,'B');assert.equal(model.dirty,false);
});
test('a saved older preview cannot erase input typed after the preview was created',()=>{
 const model=new TagDraft();model.receive(item({},[tag('A')]));model.choose(0);model.value='Later edit';model.ownership='manual';model.newTag='Later addition';
 model.receive(item({},[tag('Reviewed edit')]),{kind:'edit',root_index:0,value:'Reviewed edit'});
 assert.equal(model.value,'Later edit');assert.equal(model.valueDirty,true);
 model.receive(item({},[tag('Reviewed edit'),tag('Reviewed addition','manual')]),{kind:'add-manual',value:'Reviewed addition'});
 assert.equal(model.newTag,'Later addition');assert.equal(model.dirty,true);
});
test('spec-only refresh preserves tag drafts and unknown tag changes require discard',()=>{
 const model=new TagDraft();model.receive(item({},[tag('A')]));model.choose(0);model.newTag='new';
 model.receive(item({Camera:['Changed']},[tag('A')]));assert.equal(model.newTag,'new');assert.equal(model.conflict,false);
 model.receive(item({},[tag('Other')]));assert.equal(model.newTag,'new');assert.equal(model.tags[0].value,'A');assert.equal(model.conflict,true);
 model.choose(0);assert.equal(model.value,'A');model.discard(item({},[tag('Other')]));assert.equal(model.conflict,false);assert.equal(model.dirty,false);
});
test('applying an edit preserves a separate ownership or new-tag draft',()=>{
 const model=new TagDraft();model.receive(item({},[tag('A')]));model.choose(0);model.value='B';model.ownership='manual';model.newTag='Later';
 model.receive(item({},[tag('B')]),{kind:'edit',root_index:0,value:'B'});
 assert.equal(model.value,'B');assert.equal(model.valueDirty,false);assert.equal(model.ownership,'manual');assert.equal(model.ownershipDirty,true);assert.equal(model.newTag,'Later');
 model.receive(item({},[tag('B'),tag('Later','manual')]),{kind:'add-manual',value:'Later'});assert.equal(model.newTag,'');assert.equal(model.ownershipDirty,true);
});
