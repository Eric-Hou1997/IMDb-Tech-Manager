import type {MediaItem} from './contracts';
import {bucket} from './catalog-presentation';
export type CatalogCommand = 'reload'|'ai'|'local'|'preview'|'finder'|'copy';
export function contextItems(clicked:MediaItem,selected:string[],all:MediaItem[]):MediaItem[] {
 if(!selected.includes(clicked.id))return [clicked];
 const byId=new Map(all.map(item=>[item.id,item]));
 return selected.map(id=>byId.get(id)).filter((item):item is MediaItem=>!!item);
}
export function generationItems(items:MediaItem[]):MediaItem[] {return items.filter(item=>!['nospec','error'].includes(bucket(item)));}
