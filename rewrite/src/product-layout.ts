// v4.1.0 wraps according to rendered text width, including translated controls.
function naturalWidth(node: HTMLElement): number {
 const saved = {position: node.style.position, width: node.style.width, minWidth: node.style.minWidth, maxWidth: node.style.maxWidth, flex: node.style.flex, whiteSpace: node.style.whiteSpace, visibility: node.style.visibility};
 Object.assign(node.style, {position: 'fixed', width: 'max-content', minWidth: '0', maxWidth: 'none', flex: 'none', whiteSpace: 'nowrap', visibility: 'hidden'});
 const width = node.getBoundingClientRect().width;
 Object.assign(node.style, saved);
 return width;
}
function contentWidth(node: HTMLElement): number {
 const style = getComputedStyle(node), gap = parseFloat(style.columnGap) || parseFloat(style.gap) || 0;
 const children = Array.from(node.children).filter((child): child is HTMLElement => child instanceof HTMLElement && getComputedStyle(child).display !== 'none');
 return children.reduce((sum, child) => sum + naturalWidth(child), 0) + Math.max(0, children.length - 1) * gap;
}
export function syncProductLayout(root: HTMLElement, tv: boolean): void {
 const top = root.querySelector<HTMLElement>('.top'), brand = root.querySelector<HTMLElement>('.brand'), logo = root.querySelector<HTMLElement>('.logo'), status = root.querySelector<HTMLElement>('.statusLine'), actions = root.querySelector<HTMLElement>('.topActions');
 if (top && brand && logo && status && actions) {
  top.classList.remove('contentWrapped');
  const style = getComputedStyle(top), gap = parseFloat(style.columnGap) || parseFloat(style.gap) || 0;
  const required = (parseFloat(style.paddingLeft) || 0) + (parseFloat(style.paddingRight) || 0) + logo.getBoundingClientRect().width + naturalWidth(brand) + contentWidth(status) + naturalWidth(actions) + gap * 3;
  top.classList.toggle('contentWrapped', required > top.clientWidth + .5);
 }
 const scope = root.querySelector<HTMLElement>('.scopebar'), summary = root.querySelector<HTMLElement>('#scopeSummary'), direct = root.querySelector<HTMLElement>('.directActions'), workflow = root.querySelector<HTMLElement>('.workflow');
 if (scope && summary && direct && workflow) {
  scope.classList.remove('contentWrapped');
  const style = getComputedStyle(scope), gap = parseFloat(style.columnGap) || parseFloat(style.gap) || 0;
  const required = (parseFloat(style.paddingLeft) || 0) + (parseFloat(style.paddingRight) || 0) + naturalWidth(summary) + naturalWidth(direct) + naturalWidth(workflow) + gap * 2;
  scope.classList.toggle('contentWrapped', required > scope.clientWidth + .5);
 }
 const library = root.querySelector<HTMLElement>('.library'), selection = root.querySelector<HTMLElement>('.selectionTools'), home = root.querySelector<HTMLElement>('#typeFilterHome'), level = root.querySelector<HTMLElement>('#typeFilter');
 if (!library || !selection) return;
 library.classList.remove('narrow');
 if (home && level) home.after(level);
 const narrow = contentWidth(selection) > selection.clientWidth + .5;
 library.classList.toggle('narrow', narrow);
 if (narrow && tv && level) library.querySelector('.toolbarPrimary')?.append(level);
}
