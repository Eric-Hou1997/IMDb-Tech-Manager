import type {BatchEngine,MediaItem} from './contracts';
import {bucket} from './catalog-presentation';
export function scopeSummary(items:MediaItem[],selected:boolean) {
 const counts={ready:0,ai:0,local:0,nospec:0,error:0,na:0};
 for(const item of items)counts[bucket(item)]++;
 return {label:items.length?selected?`当前选择 ${items.length} 项`:'当前 NFO':'未选择 NFO',text:`共 ${items.length} · 可生成 ${counts.ready} · AI 完成 ${counts.ai} · 缺 Spec ${counts.nospec}${counts.error?' · 有问题 '+counts.error:''}`};
}
export function generationCount(items:MediaItem[],engine:BatchEngine):number {
 return items.filter(item=>!['missing','empty','not-applicable'].includes(item.spec_status)&&item.inspection.tag_status!==(engine==='ai'?'ai-current':'local-current')).length;
}
