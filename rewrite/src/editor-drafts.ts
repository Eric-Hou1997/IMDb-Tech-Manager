import type {Action, MediaItem, Tag} from './contracts';
export const specFields = ['Runtime','Sound mix','Color','Aspect ratio','Camera','Laboratory','Film Length','Negative Format','Cinematographic Process','Printed Film Format'];
const same = (a:unknown,b:unknown) => JSON.stringify(a) === JSON.stringify(b);
const specValues = (item:MediaItem) => Object.fromEntries(specFields.map(field=>[field,(item.specs[field]??[]).join('\n')]));
const normalized = (value:Record<string,string>) => specFields.map(field=>(value[field]??'').split('\n').map(line=>line.trim()).filter(Boolean));
export class SpecsDraft {
  itemId = '';
  baseline:Record<string,string> = {};
  values:Record<string,string> = {};
  conflict = false;
  get dirty(){return !same(this.values,this.baseline);}
  discard(item:MediaItem){this.itemId=item.id;this.baseline=specValues(item);this.values={...this.baseline};this.conflict=false;}
  receive(item:MediaItem){
    const incoming=specValues(item);
    if(item.id!==this.itemId||!this.dirty||same(normalized(this.values),normalized(incoming))){this.discard(item);return;}
    this.conflict ||= !same(incoming,this.baseline);
    this.baseline=incoming;
  }
  acknowledge(){this.conflict=false;}
}
const owner=(tag:Tag)=>tag.ownership==='generated'?(tag.engine||'local-rules'):tag.ownership;
export class TagDraft {
  itemId=''; tags:Tag[]=[];selected=-1;value='';ownership='external';newTag='';externalConfirmed=false;clearConfirmed=false;
  pendingSelection:number|null=null;conflict=false;
  get valueDirty(){return this.selected>=0&&this.value!==(this.tags[this.selected]?.value??'');}
  get ownershipDirty(){return this.selected>=0&&this.ownership!==(this.tags[this.selected]?owner(this.tags[this.selected]):'external');}
  get editDirty(){return this.valueDirty||this.ownershipDirty;}
  get dirty(){return this.editDirty||!!this.newTag||this.externalConfirmed||this.clearConfirmed;}
  discard(item:MediaItem){this.itemId=item.id;this.tags=item.tags.map(tag=>({...tag}));this.selected=-1;this.value='';this.ownership='external';this.newTag='';this.externalConfirmed=false;this.clearConfirmed=false;this.pendingSelection=null;this.conflict=false;}
  choose(index:number,discard=false){
    if(index<0||index>=this.tags.length||this.conflict)return;
    if(index===this.selected)return;
    if(index!==this.selected&&this.editDirty&&!discard){this.pendingSelection=index;return;}
    this.selected=index;this.value=this.tags[index].value;this.ownership=owner(this.tags[index]);this.externalConfirmed=false;this.pendingSelection=null;
  }
  receive(item:MediaItem,applied?:Action){
    if(item.id!==this.itemId||!this.dirty){this.discard(item);return;}
    if(same(item.tags,this.tags))return;
    if(!applied){this.conflict=true;return;}
    const valueDirty=this.valueDirty, ownershipDirty=this.ownershipDirty;
    if(applied.kind==='add-manual'&&this.newTag===applied.value)this.newTag='';
    if(applied.kind==='delete'&&this.selected===applied.root_index){this.selected=-1;this.value='';this.ownership='external';}
    else if(applied.kind==='delete'&&this.selected>applied.root_index)this.selected--;
    if(applied.kind==='clear-ai'){
      // A selection whose positions changed cannot be silently retargeted.
      if(this.editDirty){this.conflict=true;return;}
      this.selected=-1;
    }
    this.tags=item.tags.map(tag=>({...tag}));
    if(this.selected>=0){
      const selected=this.tags[this.selected];
      if(!selected){this.conflict=true;return;}
      if(!valueDirty||(applied.kind==='edit'&&applied.root_index===this.selected&&this.value===applied.value))this.value=selected.value;
      if(!ownershipDirty||(applied.kind==='set-ownership'&&applied.root_index===this.selected&&this.ownership===applied.ownership))this.ownership=owner(selected);
    }
    this.externalConfirmed=false;this.clearConfirmed=false;this.pendingSelection=null;
  }
}
