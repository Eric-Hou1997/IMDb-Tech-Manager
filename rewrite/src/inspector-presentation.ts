import type {MediaItem} from './contracts';
import {cell} from './catalog-presentation';
import {invoke,isTauri} from '@tauri-apps/api/core';

export const statusOptions = [
 ['ai-complete', 'AI 完成'], ['local-complete', '规则完成'], ['no-tags', 'Spec 就绪'],
 ['stale', '待同步'], ['review', '待复核'], ['tag-missing', 'Tag 缺失'],
 ['legacy', '旧数据'], ['manual-spec', '人工规格'], ['spec-missing', '缺 Spec'], ['spec-empty', '无数据'],
];
const mediaLabels: Record<string, string> = {Movie: '电影 NFO', Series: '电视剧 NFO', Episode: '单集 NFO', Season: '季 NFO'};
const tagLabels: Record<string, string> = {'ai-current': 'AI 完成', 'local-current': '规则完成', none: '未生成', stale: '待同步', review: '待复核', 'tag-missing': 'Tag 缺失', legacy: '旧数据', current: '旧数据', 'not-applicable': '不适用'};
const issueLabels: Record<string, string> = {'xml-error':'XML 解析错误','read-error':'NFO 读取错误','nfo-read':'NFO 读取错误','ownership-mismatch':'所有权不一致','library-type-mismatch':'资料库类型不一致','missing-imdb':'缺少 IMDb ID','spec-missing':'缺少 Technical Specs',stale:'待同步','tag-missing':'Tag 缺失',review:'待复核','duplicate-tag':'重复 Tag','prompt-stale':'提示词或模型已变更','ai-failure':'AI 失败',auth:'AI 认证失败',quota:'AI 额度不足','rate-limit':'AI 请求限流',transient:'AI 临时故障',request:'AI 请求失败','provider-response':'AI 服务响应异常','output-truncated':'AI 输出被截断','content-filter':'AI 内容过滤','provider-refusal':'AI 服务拒绝','context-length':'AI 上下文过长','malformed-json':'AI JSON 无效','schema-invalid':'AI JSON 结构无效',paused:'AI 已暂停',config:'AI 配置错误',exception:'未知异常','known-failure':'已知失败'};
const issueMessages: Record<string, string> = {'missing-imdb': 'NFO 缺少 IMDb ID', 'spec-missing': '尚未准备 IMDb Technical Specs', stale: '技术标签尚未与当前规格同步', 'tag-missing': 'manifest 中的生成标签在根节点缺失', review: 'AI 结果等待人工复核', 'duplicate-tag': '发现同值重复标签；为安全起见不会自动删除', 'prompt-stale': 'AI 提示词/模型已更新；建议重新生成（状态保持 AI 完成）', 'ai-failure': 'AI 处理失败', 'ownership-mismatch': 'NFO manifest 与本地 ownership 镜像不一致', 'library-type-mismatch': 'NFO 类型与所选资料库分类不一致；已停止生成操作'};
export function issueLabel(kind: string): string {return issueLabels[kind] || '未知问题';}
export function issueMessage(item: MediaItem, kind: string): string {
 if(item.inspection.issue_details?.[kind])return item.inspection.issue_details[kind].message;
 return ['xml-error', 'read-error'].includes(kind) && item.error ? item.error.message : issueMessages[kind] || kind;
}
export function canIgnoreIssue(kind: string): boolean {return !['xml-error', 'read-error', 'library-type-mismatch'].includes(kind);}
export function identityRows(item: MediaItem): {key: string; value: string; user: boolean}[] {
 const counts = {generated: 0, manual: 0, external: 0};
 for (const tag of item.tags) counts[tag.ownership]++;
 const rows: [string, string, boolean?][] = [
  ['标题', item.title, true], ['年份', item.year || '—', true], ['媒体类型', mediaLabels[item.kind] || '未知类型'],
  ['IMDb ID', item.imdb || '缺失', true], ['XML', item.inspection.xml_valid ? '有效' : '错误'],
  ['Source Hash', item.source_hash, true], ['编码', item.inspection.bom ? 'UTF-8 BOM' : 'UTF-8'], ['换行', item.inspection.newline],
  ['Spec 状态', cell(item, 'spec_status')], ['Tag 状态', tagLabels[item.inspection.tag_status] || '未生成'],
  ['Generated / Manual / External', `${counts.generated} / ${counts.manual} / ${counts.external}`],
  ['Manifest / Sidecar', item.inspection.manifest_sidecar_match == null ? '无镜像' : item.inspection.manifest_sidecar_match ? '一致' : '不一致'],
 ];
 return rows.map(([key, value, user = false]) => ({key, value, user}));
}
// The native v4 web UI also falls back to execCommand when WebKit does not
// expose the asynchronous Clipboard API. Copy runs in the button's gesture.
export async function copyText(value: string, platform=''): Promise<void> {
 if(platform==='macos'&&isTauri()){await invoke('copy_text',{value});return;}
 if (navigator.clipboard?.writeText) {await navigator.clipboard.writeText(value); return;}
 const field = document.createElement('textarea');
 field.value = value; field.style.position = 'fixed'; field.style.opacity = '0';
 document.body.appendChild(field);
 try {field.select(); if (!document.execCommand('copy')) throw Error('复制失败');}
 finally {field.remove();}
}
