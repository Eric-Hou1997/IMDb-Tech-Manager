import type { CatalogColumn, CatalogColumns, Sort } from './contracts';

export const columnDefinitions: Record<CatalogColumn, {label: string; preferred: number; floor: number; sort: Sort}> = {
  title: {label: '标题', preferred: 350, floor: 120, sort: 'title'},
  year: {label: '发行年份', preferred: 82, floor: 72, sort: 'year'},
  added_date: {label: '添加日期', preferred: 88, floor: 76, sort: 'added-date'},
  spec_status: {label: 'Spec 状态', preferred: 84, floor: 82, sort: 'specs-status'},
  tag_status: {label: 'Tag 状态', preferred: 82, floor: 74, sort: 'tags-status'},
};
export function defaultColumns(): CatalogColumns {
  const order = Object.keys(columnDefinitions) as CatalogColumn[];
  return {order, visible: [...order], widths: {}, compact: false};
}
export function visibleColumns(columns: CatalogColumns): CatalogColumn[] {
  return columns.order.filter(key => columns.visible.includes(key));
}
export function fitWidths(requested: number[], floors: number[], available: number): number[] {
  if (!requested.length) return [];
  if (available <= 0) return requested;
  const floorTotal = floors.reduce((sum, value) => sum + value, 0);
  if (available < floorTotal) return floors.map(value => value * available / floorTotal);
  const requestedTotal = requested.reduce((sum, value) => sum + Math.max(1, value), 0) || 1;
  const widths = requested.map(value => Math.max(1, value) * available / requestedTotal);
  for (let pass = 0; pass < requested.length; pass++) {
    let deficit = 0;
    for (let i = 0; i < widths.length; i++) if (widths[i] < floors[i]) {deficit += floors[i] - widths[i]; widths[i] = floors[i];}
    if (deficit < .01) break;
    const donorTotal = widths.reduce((sum, value, i) => sum + Math.max(0, value - floors[i]), 0);
    if (donorTotal <= deficit) break;
    for (let i = 0; i < widths.length; i++) {const excess = Math.max(0, widths[i] - floors[i]); if (excess) widths[i] -= deficit * excess / donorTotal;}
  }
  return widths;
}
export function columnWidths(columns: CatalogColumns, available: number): number[] {
  const keys = visibleColumns(columns);
  const floors = keys.map(key => columnDefinitions[key].floor);
  const custom = keys.every(key => (columns.widths[key] ?? 0) > 0);
  const metadata = keys.reduce((sum, key) => sum + (key === 'title' ? 0 : columnDefinitions[key].preferred), 0);
  const requested = keys.map(key => custom ? columns.widths[key]! : key === 'title' ? Math.max(350, available - metadata) : columnDefinitions[key].preferred);
  return fitWidths(requested, floors, available);
}
export function moveColumn(columns: CatalogColumns, source: CatalogColumn, target: CatalogColumn): CatalogColumns {
  if(source === 'title' || target === 'title' || source === target || !columns.order.includes(source) || !columns.order.includes(target)) return columns;
  const order = columns.order.filter(key => key !== source);
  order.splice(order.indexOf(target), 0, source);
  return {...columns, order};
}
