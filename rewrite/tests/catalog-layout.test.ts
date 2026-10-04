import test from 'node:test';
import assert from 'node:assert/strict';
import {columnWidths, defaultColumns, moveColumn, visibleColumns} from '../src/catalog-layout.ts';
import {defaultPresentation, splitPixels, taskPixels} from '../src/window-state.ts';

test('all original columns fit narrow, default and wide containers without overflow',()=>{
 for(const width of [180,250,330,420,700,1600]){
  const columns=defaultColumns();const pixels=columnWidths(columns,width);
  assert.equal(pixels.length,5);
  assert.ok(pixels.every(value=>value>0&&Number.isFinite(value)));
  assert.ok(Math.abs(pixels.reduce((sum,value)=>sum+value,0)-width)<.02);
 }
});
test('column reorder preserves title and hidden columns retain their position',()=>{
 const original=defaultColumns();const changed=moveColumn(original,'tag_status','year');
 assert.deepEqual(changed.order,['title','tag_status','year','added_date','spec_status']);
 assert.equal(moveColumn(changed,'title','year'),changed);
 assert.equal(moveColumn(changed,'year','title'),changed);
 changed.visible=changed.visible.filter(key=>key!=='year');
 assert.deepEqual(visibleColumns(changed),['title','tag_status','added_date','spec_status']);
 changed.visible.push('year');
 assert.deepEqual(visibleColumns(changed),changed.order);
 assert.deepEqual(original.order,defaultColumns().order);
});
test('custom column widths adapt to viewport while preserving the saved preference',()=>{
 const columns=defaultColumns();columns.widths={title:410,year:91,added_date:99,spec_status:92,tag_status:90};
 const original=JSON.stringify(columns);
 for(const width of [200,500,1000])assert.ok(Math.abs(columnWidths(columns,width).reduce((a,b)=>a+b,0)-width)<.02);
 assert.equal(JSON.stringify(columns),original);
});
test('window clamps are temporary and never mutate the restored ratio or task preference',()=>{
 const layout=defaultPresentation();layout.split_basis_points=4425;layout.task_height=315;
 assert.equal(splitPixels(1280,layout.split_basis_points),566.4);
 assert.equal(splitPixels(700,100),330);
 assert.equal(splitPixels(1280,9900),848);
 assert.equal(taskPixels(300,layout.task_height),210);
 assert.equal(taskPixels(900,layout.task_height),315);
 assert.equal(layout.split_basis_points,4425);
 assert.equal(layout.task_height,315);
});
