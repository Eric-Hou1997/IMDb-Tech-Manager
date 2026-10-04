import type { CatalogColumn, LibraryView, MediaItem } from './contracts';

export const movieChunk = 600;
const bucketLabels = {ai: 'AI 完成', local: '规则完成', ready: '可生成', nospec: '缺 Spec', error: '有问题', na: '不适用'};
const bucketClasses = {ai: 'ai', local: 'ok', ready: 'no-tags', nospec: 'warn', error: 'bad', na: ''};
const specLabels: Record<string, string> = {ready: '就绪', manual: '人工', empty: '无数据', missing: '缺失', 'not-applicable': '不适用'};
const specRanks: Record<string, number> = {missing: 0, empty: 1, manual: 2, ready: 3, 'not-applicable': 4};
const tagRanks: Record<string, number> = {none: 0, 'tag-missing': 1, stale: 2, review: 3, legacy: 4, 'local-current': 5, 'ai-current': 6, 'not-applicable': 7};

export function bucket(item: MediaItem): keyof typeof bucketLabels {
  const lifecycle = item.inspection.lifecycle;
  if (['xml-error', 'index-refresh-required'].includes(lifecycle)) return 'error';
  if (lifecycle === 'not-applicable') return 'na';
  if (['spec-missing', 'spec-empty'].includes(lifecycle)) return 'nospec';
  if (lifecycle === 'ai-complete') return 'ai';
  if (lifecycle === 'local-complete') return 'local';
  if (item.inspection.issues.some(issue => ['xml-error', 'read-error', 'ownership-mismatch', 'library-type-mismatch'].includes(issue))) return 'error';
  return 'ready';
}
export function statusLabel(item: MediaItem): string {
  let label: string = bucketLabels[bucket(item)];
  if (item.inspection.status_override) label += ' ·手动';
  if (bucket(item) === 'ready' && item.spec_status === 'manual') label = '人工规格 · 可生成';
  return label;
}
export function statusClass(item: MediaItem): string { return bucketClasses[bucket(item)]; }
export function pillClass(item: MediaItem): string {
  const value = statusClass(item);
  return ['bad', 'ai', 'manual', 'ok'].includes(value) ? value : 'warn';
}
export function cell(item: MediaItem, key: CatalogColumn, locale = 'zh-CN'): string {
  if (key === 'title') return item.title || '(无标题)';
  if (key === 'year') return String(item.year || '').slice(0, 4) || '—';
  if (key === 'spec_status') return specLabels[item.spec_status] || '缺失';
  if (key === 'tag_status') return statusLabel(item);
  const value = item.added_date || '';
  const match = value.match(/^(\d{4})-(\d{2})-(\d{2})$/);
  return match ? new Intl.DateTimeFormat(locale, {year: 'numeric', month: '2-digit', day: '2-digit', timeZone: 'UTC'}).format(new Date(Date.UTC(+match[1], +match[2] - 1, +match[3]))) : value || '—';
}
export function subtitle(item: MediaItem): string {
  const season = item.season !== '' ? `S${String(number(item.season)).padStart(2, '0')}E${String(number(item.episode)).padStart(2, '0')}` : '';
  return [season, item.imdb, item.path].filter(Boolean).join(' · ');
}
export function sortItems(items: MediaItem[], view: LibraryView): MediaItem[] {
  const title = (item: MediaItem) => String(item.title || '').toLocaleLowerCase();
  const value = (item: MediaItem): string | number => {
    switch (view.sort) {
      case 'year': return parseInt(String(item.year || '').slice(0, 4)) || 0;
      case 'added-date': return item.added_date || '';
      case 'specs-status': return specRanks[item.spec_status] ?? -1;
      case 'tags-status': return tagRanks[item.inspection.tag_status] ?? -1;
      default: return title(item);
    }
  };
  return [...items].sort((a, b) => {
    const av = value(a), bv = value(b);
    const primary = typeof av === 'number' && typeof bv === 'number' ? av - bv : String(av).localeCompare(String(bv));
    return primary * (view.descending ? -1 : 1) || title(a).localeCompare(title(b));
  });
}
export function clearFilters(view: LibraryView): void {
  view.catalog_filter = 'all'; view.media_level = 'all'; view.search = '';
  view.lifecycle = ''; view.errors = false; view.issues = false; view.offset = 0;
}
const number = (value: string) => /^\d+$/.test(value) ? Number(value) : 0;
const groupName = (item: MediaItem) => item.series_key || item.title || '未归属节目';
export type CatalogTreeRow = {
  key: string; kind: 'show' | 'season' | 'item'; name: string; subtitle: string;
  css: string; open: boolean; item: MediaItem | null; members: string[];
};
export function tvRows(items: MediaItem[], all: MediaItem[], view: LibraryView): {rows: CatalogTreeRow[]; order: string[]} {
  const groups = new Map<string, MediaItem[]>();
  for (const item of items) {
    const name = groupName(item), group = groups.get(name) || [];
    group.push(item); groups.set(name, group);
  }
  const representatives = [...groups].map(([name, group]) => ({name, item: group.find(item => item.kind === 'Series') || group[0]}));
  const sorted = sortItems(representatives.map(group => group.item), view);
  const rows: CatalogTreeRow[] = [], order: string[] = [];
  for (const representative of sorted) {
    const name = groupName(representative), group = groups.get(name)!;
    const show = group.find(item => item.kind === 'Series') || null;
    const episodes = group.filter(item => item.kind === 'Episode');
    const key = `show:${name}`, open = view.expanded.includes(key);
    if (show) order.push(show.id);
    rows.push({key, kind: 'show', name, subtitle: view.media_level === 'tvshow' ? '' : `${episodes.length} 集`, css: 'treeShowHeader', open, item: show, members: []});
    if (!open) continue;
    if (show) rows.push({key: show.id, kind: 'item', name: '', subtitle: '', css: 'treeShowItem', open: false, item: show, members: []});
    const seasons = new Map<number, MediaItem[]>();
    for (const episode of episodes) {
      const season = number(episode.season), entries = seasons.get(season) || [];
      entries.push(episode); seasons.set(season, entries);
    }
    for (const [season, entries] of [...seasons].sort((a, b) => a[0] - b[0])) {
      entries.sort((a, b) => number(a.episode) - number(b.episode));
      const seasonKey = `${key}:${season}`, seasonOpen = view.expanded.includes(seasonKey);
      const members = all.filter(item => item.kind === 'Episode' && groupName(item) === name && number(item.season) === season).map(item => item.id);
      order.push(...entries.map(item => item.id));
      rows.push({key: seasonKey, kind: 'season', name: `第 ${season} 季`, subtitle: `${entries.length} 集`, css: 'treeSeasonHeader', open: seasonOpen, item: null, members});
      if (seasonOpen) for (const item of entries) rows.push({key: item.id, kind: 'item', name: '', subtitle: '', css: 'treeEpisode', open: false, item, members: []});
    }
  }
  return {rows, order};
}
