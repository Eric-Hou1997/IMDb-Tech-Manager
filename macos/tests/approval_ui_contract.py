#!/usr/bin/env python3
"""v4.0.2 regressions for approval refresh and catalog UI polish."""
import importlib.util
import pathlib


ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE_PATH = ROOT / "engine" / "mac-engine.py"
WEB = (ROOT / "web" / "index.html").read_text(encoding="utf-8")
MAIN = (ROOT / "main.go").read_text(encoding="utf-8")
INFO = (ROOT.parent / "packaging" / "Info.plist").read_text(encoding="utf-8")
LAUNCHER = (ROOT / "native" / "IMDbTechManagerLauncher.m").read_text(encoding="utf-8")
FETCHER = (ROOT / "native" / "IMDbWebKitFetcher.m").read_text(encoding="utf-8")

spec = importlib.util.spec_from_file_location("eng_current", ENGINE_PATH)
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)


detail = {
    "new_tags": ["AI Tag", "Generated Same"],
    "final_tags": ["External / TMM", "AI Tag", "Generated Same", "External / TMM"],
}
assert eng._preview_tag_entries(detail) == [
    {"value": "External / TMM", "kind": "existing"},
    {"value": "AI Tag", "kind": "generated"},
    {"value": "Generated Same", "kind": "generated"},
]
assert '"preview_tags": preview_tags' in ENGINE_PATH.read_text(encoding="utf-8")
assert '"preview_tags": _preview_tag_entries(detail)' in ENGINE_PATH.read_text(encoding="utf-8")
print("OK 1: preview tag roles come from the engine result, not UI text guessing")


for required in (
    'const appVersion = "4.0.2"',
    'jobID := strings.TrimSpace(r.URL.Query().Get("id"))',
    'jobs.snapshot(jobID)',
    '"job_id": jobID',
    'jobs.complete(jobID, action, code, msg, string(b))',
):
    assert required in MAIN, required
assert "func (m *jobManager) begin" in MAIN
assert "type JobState struct {" in MAIN and '`json:"job_id,omitempty"`' in MAIN
assert "已有任务正在运行" in MAIN
print("OK 2: approval waits on an immutable job identity and overlapping jobs fail closed")


for required in (
    'content="v4.0.2"',
    "function renderCatalogHeader()",
    "header.style.gridTemplateColumns=listGrid()",
    "qa('#libraryList .row').forEach(row=>row.style.gridTemplateColumns=grid)",
    "data-compact-view",
    "const TOOL_COLUMN_WIDTH=34",
    "prefs.compact=prefs.compact===true",
    "widthMode='adaptive-current'",
    "function columnPixelWidths()",
    "function fitColumnWidths(",
    "--catalog-left-edge:clamp(6px,1.25%,14px)",
    "--catalog-right-edge:0px",
    "function installCatalogLayoutObserver()",
    "function applySplitRatio(",
    "--catalog-preheader-height",
    "--catalog-header-height",
    ".list.compact .titleCell .sub{display:none}",
    "grid-template-columns:minmax(0,var(--left)) 12px minmax(0,1fr)",
    "role=\"separator\"",
    "aria-orientation=\"vertical\"",
):
    assert required in WEB, required
assert WEB.count("function renderListHeader()") == 1
assert "headerGrid" not in WEB
assert WEB.count("event.target.closest('#resetColumns')") == 1
assert "toolColumnWidth" not in WEB
assert 'data-resize-field="__tools"' not in WEB
assert ".headerContent" in WEB and ".sortArrow{position:static" in WEB
assert ".columnTools>.resizeHandle{display:none}" in WEB
assert "max-width:calc(100% - 22px)" not in WEB
assert "max-width:calc(100% - 12px)" not in WEB
assert WEB.count(".headCell{position:relative") == 1
assert ".tabs button.active:after" in WEB and "background:var(--yellow)" in WEB
assert 'role="columnheader"' in WEB and 'aria-sort="${sort.direction===\'asc\'?\'ascending\':\'descending\'}"' in WEB
print("OK 3: header, rows, splitter, and compact mode use one maintained layout path")


for required in (
    "waitForExactJob(jobID",
    "'/api/job?id='+encodeURIComponent(jobID)",
    "refreshApprovedPaths(submitted.job_id,paths)",
    "reloadNfos(paths,{quiet:true,silentResult:true})",
    "renderLibrary();renderInspector();schedulePreflight()",
):
    assert required in WEB, required
assert "refreshApprovedCurrent" not in WEB
print("OK 4: approval completion reloads every submitted NFO and reruns list state")


for required in (
    ".tagChip.generated",
    "kind==='generated'?'generated':''",
    "AI':'规则'}生成标签",
    "NFO 现有标签",
    "previewFooter",
    "checkLabel appLoginControl",
    "checkLabel autoStartLabel",
    "规格来源与标签同步",
    "当前使用人工修改后的规格",
):
    assert required in WEB, required
assert ".tagChip.new{" not in WEB
assert WEB.count("function showPreviewResults()") == 1
assert WEB.count("new MutationObserver(") == 1
print("OK 5: preview semantics, footer, settings controls, and specs copy are coherent")


for marker in (
    "compactDefault", "catalogAlignment", "splitterHit", "compactLayout",
    "checkboxMetrics", "previewSemantics", "previewFooter", "headerGeometry",
    "headerFit", "inspectorAlignment", "responsiveContainment", "splitBounds",
    "activeTabIndicator", "responsiveEdges", "narrowToolbar", "movieSelectionSummary", "toolbarHeight", "catalogControlParity", "tvToolbarThreshold", "tvToolbarPathStable", "autoModeStartupPreference", "textSelectionPolicy",
    "layoutRestore", "layoutFallback",
):
    assert f"root.dataset.{marker}" in WEB, marker
print("OK 6: v4.0.2 browser smoke covers the reported visual regressions")


for required in (
    "--catalog-status-ratio:35%",
    "grid-template-columns:minmax(0,1fr) minmax(var(--catalog-status-min),var(--catalog-status-ratio))",
    "height:var(--catalog-header-height)",
    "max-height:var(--catalog-header-height)",
    "function runCurrentToolbarHeight()",
    "root.dataset.catalogControlParity=",
    "button,select,option,.pill,.badge",
    "function runCurrentTextSelectionSmoke()",
):
    assert required in WEB, required
assert "filter.style.width=" not in WEB
assert "--catalog-header-height',`${headerHeight" not in WEB
assert "catalog-narrow-status" not in WEB
assert "--catalog-status-ratio:34%" not in WEB
print("OK 6b: catalog control widths and header height have one geometry owner")


for required in (
    'mux.HandleFunc("/api/ui-layout", requireToken(handleUILayout))',
    "func saveUILayoutBytes(raw []byte) error",
    "incoming.Revision < current.Revision",
    "return atomicWriteUILayout(raw)",
    "tmp.Sync()",
    'return filepath.Join(baseDir(), "ui-layout.json")',
    "base64.RawURLEncoding.EncodeToString(loadUILayoutBytes())",
):
    assert required in MAIN, required
for required in (
    'name="ui-layout-state"',
    "function captureUILayout(",
    "function persistUILayout()",
    "uiLayoutSaveChain=uiLayoutSaveChain",
    "if(Number(snapshot.revision)<Number(uiLayout.revision))return",
    "function applyTaskHeight(",
    "function tvToolbarNeedsWrap(",
    "function resetCatalogToolbarLayout(",
    "function tvToolbarSnapshot(",
    "function runAutoModeStartupPreferenceSmoke(",
    "secondaryMinimum=",
    "root.dataset.tvToolbarPathStable=",
    "addEventListener('pagehide'",
    "navigator.sendBeacon(`/api/ui-layout?token=",
):
    assert required in WEB, required
for required in (
    "IMDBMainWindowFrameV1",
    "loadStoredWindowFrame:",
    "constrainedStoredFrame:",
    "screenForStoredFrame:",
    "windowDidEndLiveResize:",
    "windowWillEnterFullScreen:",
    "windowEverVisible",
):
    assert required in LAUNCHER, required
assert "localStorage.taskHeight=height" not in WEB
print("OK 7: native window and web workspace geometry use durable, validated lifecycle ownership")


assert "<key>CFBundleVersion</key><string>4.0.2</string>" in INFO
assert "<key>CFBundleShortVersionString</key><string>4.0.2</string>" in INFO
assert 'IMDbTechManager/4.0.2' in LAUNCHER
assert 'IMDbTechManagerFetcher/4.0.2' in FETCHER
print("OK 8: core, Web UI, bundle metadata, and native user agents share v4.0.2")
