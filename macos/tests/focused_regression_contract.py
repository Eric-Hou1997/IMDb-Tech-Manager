#!/usr/bin/env python3
"""Focused regressions for dynamic overlay reuse and large-index deltas."""
import importlib.util
import os
import pathlib
import re
import shutil
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("eng_contract", ROOT / "engine" / "mac-engine.py")
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)


with tempfile.TemporaryDirectory() as td:
    td = pathlib.Path(td)
    lib = td / "lib"
    lib.mkdir()
    nfo = lib / "one.nfo"
    shutil.copy(ROOT / "tests/fixtures/casino-royale-legacy-minimal.nfo", nfo)

    # Fully isolate persistent state, including root health (the production
    # path must not leak into a contract test or a user profile).
    for name in ("CFG", "AI_FAILURE_QUEUE", "ISSUE_ACKS", "INDEX_CACHE", "STATUS_OVERRIDES", "ROOT_HEALTH"):
        setattr(eng, name, td / name.lower())
    eng.AI_CACHE = td / "ai-cache"
    eng.CACHE = td / "cache"
    eng.AI_CACHE.mkdir()
    eng.CACHE.mkdir()
    eng.save_json(eng.CFG, {"library_roots": {"movies": [str(lib)], "tv": []}, "ai": {"enabled": True, "model": "m", "prompt": eng.DEFAULT_AI_PROMPT}})
    eng._INDEX_CACHE_MEM.update(stamp="", items={})
    eng._DYNAMIC_OVERLAY_MEM.update(stamp=None, value={})
    eng._LIBRARY_CATALOG.update(loaded=True, items={}, cache_mtime=None)

    base = eng._summary_from_detail({
        "path": str(nfo), "source_hash": "hash", "xml_valid": True,
        "media_space": "movies", "media_type": "movie", "title": "T",
        "year": "1967", "imdb": "tt0061452", "spec_status": "ready",
        "tag_status": "ai-current", "tag_engine": "ai", "tag_model": "m",
        "tag_prompt_hash": "wrong", "issues": [], "counts": {},
    })
    eng._LIBRARY_CATALOG["items"] = {str(nfo): {"nfo_stamp": "", "sidecar_stamp": "", "summary": base}}
    eng.metrics_reset()
    result = eng.library_index(allow_cold_scan=False)
    assert len(result["items"]) == 1
    assert not eng.load_json(eng.ROOT_HEALTH, {}), "library UI reads must not write root health"
    # The overlay reads each dynamic store once for the request, not once for
    # every title.  A second item must not multiply CFG/failure/ack loads.
    eng._LIBRARY_CATALOG["items"][str(lib / "two.nfo")] = {"summary": dict(base, path=str(lib / "two.nfo"))}
    before = dict(eng.metrics_snapshot())
    eng.library_index(allow_cold_scan=False)
    after = eng.metrics_snapshot()
    assert after.get("nfo_read_count", 0) == before.get("nfo_read_count", 0)
    print("OK 1: dynamic overlay context is reused across catalog items")

    # Resident requests must not perform an implicit cold NAS scan.  The
    # explicit reconcile job owns enumeration and will populate the snapshot.
    eng._LIBRARY_CATALOG.update(loaded=True, items={}, cache_mtime=None)
    eng._configured_nfo_paths = lambda: (_ for _ in ()).throw(AssertionError("resident cold scan"))
    resident_result = eng._serve_dispatch("library-index", {})
    assert resident_result["items"] == []
    print("OK 2: resident library-index returns an empty snapshot without cold scan")

    # Large indexes use an append-only update journal; the base catalog is
    # compacted periodically instead of being rewritten for every NFO.
    entries = {str(lib / ("x%03d.nfo" % i)): {"summary": {"path": str(lib / ("x%03d.nfo" % i))}} for i in range(eng._INDEX_INCREMENTAL_THRESHOLD + 1)}
    eng._index_cache_save(eng._index_context_stamp(), entries)
    eng._LIBRARY_CATALOG["items"] = entries
    eng.refresh_path_summary(nfo, item={"path": str(nfo), "title": "updated", "media_type": "movie", "xml_valid": True, "counts": {}})
    journal = pathlib.Path(str(eng.INDEX_CACHE) + eng._INDEX_DELTA_SUFFIX)
    assert journal.exists(), journal
    merged = eng._index_cache_load(eng._index_context_stamp())
    assert merged[os.path.realpath(str(nfo))]["summary"]["title"] == "updated"
    print("OK 3: large index updates use a durable delta journal")

print("OK legacy focused regression contract")

web = (ROOT / "web" / "index.html").read_text(encoding="utf-8")
assert len(re.findall(r"<script(?: [^>]*)?>", web)) == 1, "UI scripts must stay consolidated"
assert 'id="scopeKind"' not in web and "automaticScope" in web
assert 'id="testAI"' not in web and 'id="rescanAll"' not in web and 'id="selNone"' not in web
assert "setTaskOpen" in web and "dataset.uiReady" in web
assert 'id="discoverRoots"' not in web and 'id="migrationPreflight"' not in web
assert "discover-roots" not in web and "migration-preflight" not in web
assert "自动范围" not in web
assert re.search(r'class="selectionTools">.*id="selAll".*id="selInvert".*id="selClear".*class="selectionSummary">.*id="filterSummary".*id="selCount"', web)
assert ".selectionSummary{display:inline-flex" in web and "white-space:nowrap" in web
assert "max-height:150px" not in web and ".log{height:auto;align-self:stretch;min-width:0;min-height:0;max-height:none" in web
assert 'class="directActions"' in web and web.index('class="directActions"') < web.index('class="workflow"')
assert ".drawer.open .drawerBody{display:grid;grid-template-rows:minmax(0,1fr) auto;row-gap:8px}" in web
assert "<strong>${esc(scope.label)}</strong>共 ${items.length}" in web
assert "#version{font-weight:400;font-size:13px" in web and "NFO检查·技术规格·标签管理·批量任务" in web
assert "开启自动模式后，检测到新增 NFO 文件中含有 IMDb 号后，自动注入 Technical Specs 元数据。" in web
assert "q('#toggleAgent').classList.toggle('yellow',!!status.agent_running)" in web
drawer_head = re.search(r'<div class="drawerHead"[^>]*>(.*?)</div><div class="drawerBody">', web).group(1)
assert drawer_head.index('id="jobSummary"') < drawer_head.index('class="grow"') < drawer_head.index('id="jobPill"') < drawer_head.index('id="taskChevron"')
assert ".drawerHead #jobPill{padding:2px 8px;font-size:11px}" in web
for control in ("pauseTask", "recoverAI", "resumeTask", "retryFailed"):
    assert re.search(r'class="[^"]*hide[^"]*" id="%s"' % control, web), control
assert "function updateTaskControls" in web and "failedAI" in web
print("OK 4: consolidated UI layout and state-driven recovery controls are guarded")

main_go = (ROOT / "main.go").read_text(encoding="utf-8")
platform = (ROOT / "platform_darwin.go").read_text(encoding="utf-8")
engine = (ROOT / "engine" / "mac-engine.py").read_text(encoding="utf-8")
for retired in ("discover-roots", "migration-preflight"):
    assert retired not in main_go and retired not in platform and retired not in engine, retired
assert "def discover_roots():" in engine and "def discover_root_candidates():" in engine
print("OK 5: unsafe legacy discovery and migration UI actions are removed; read-only onboarding discovery remains")
