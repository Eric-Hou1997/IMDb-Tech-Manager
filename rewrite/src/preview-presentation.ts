import type {AiRecord,BatchItem,WritePreview} from './contracts';
export type PreviewEntry={row:BatchItem;candidate:WritePreview|null;ai:AiRecord|null;error:string};
export function previewRecord(entry:PreviewEntry) {
 const candidate=entry.candidate;
 const result=entry.ai?.result;
 const details=result&&typeof result==='object'&&!Array.isArray(result)?result:{};
 const strings=(value:unknown)=>Array.isArray(value)?value.filter((value):value is string=>typeof value==='string'):[];
 const seen=new Set<string>();
 const tags=(candidate?.after_tags||[]).map(tag=>({value:tag.value,kind:tag.ownership==='generated'?'generated':'existing'})).filter(tag=>tag.value&&!seen.has(tag.value.toLocaleLowerCase())&&seen.add(tag.value.toLocaleLowerCase()));
 return {title:candidate?.title||entry.row.item.title||entry.row.item.path,imdb:candidate?.imdb||entry.row.item.imdb,status:details.requires_review===true?'review':'',preview_tags:tags,replaced_owned:(candidate?.before_tags||[]).filter(tag=>tag.ownership==='generated').map(tag=>tag.value),warnings:strings(details.warnings),review_reasons:strings(details.review_reasons)};
}
