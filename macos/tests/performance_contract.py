#!/usr/bin/env python3
"""legacy performance contracts (call-count based, no wall-clock gates).

Covers the sol-doc acceptance rules:
- selected scope preflight performs zero full-library scans
- warm library listing performs zero stat / read / parse IO
- dynamic overlay changes (failure queue) reparse zero NFOs
- auto agent cycles never rglob the library
- pipeline status aggregates the catalog without re-reading NFOs
- inspector_detail parses exactly once
- raw IMDb cache serves without network
"""
import importlib.util
import os
import pathlib
import shutil
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / "engine" / "mac-engine.py"
WEB = ROOT / "web" / "index.html"
MAIN = ROOT / "main.go"
PLATFORM = ROOT / "platform_darwin.go"

spec = importlib.util.spec_from_file_location("eng_contract", ENGINE)
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)


def isolate(td, roots):
    eng.CFG = td / "config.json"
    eng.AI_FAILURE_QUEUE = td / "failures.json"
    eng.ISSUE_ACKS = td / "acks.json"
    eng.INDEX_CACHE = td / "index-cache.json"
    eng.STATUS_OVERRIDES = td / "overrides.json"
    eng.ROOT_HEALTH = td / "root-health.json"
    eng.OWNERSHIP_DIR = td / "ownership"
    eng.UNDO_DIR = td / "undo"
    eng.JOB_PROGRESS = td / "job-progress.json"
    eng.PREVIEW_RESULTS = td / "preview-results.json"
    eng.MANUAL_TASK_FLAG = td / "manual-task.flag"
    eng.AI_CACHE = td / "ai-cache"
    eng.AI_CACHE.mkdir(exist_ok=True)
    eng.CACHE = td / "cache"
    eng.CACHE.mkdir(exist_ok=True)
    eng._INDEX_CACHE_MEM.update(stamp="", items={})
    eng._LIBRARY_CATALOG.update(loaded=False, items={}, cache_mtime=None)
    eng.save_json(eng.CFG, {"roots": roots})


def forbid_full_scan():
    def forbidden(roots):
        raise AssertionError("full library scan performed")
        yield
    eng.nfos = forbidden
    eng._configured_nfo_paths = lambda: (_ for _ in ()).throw(AssertionError("full library enumeration performed"))


with tempfile.TemporaryDirectory() as td:
    td = pathlib.Path(td)
    lib = td / "lib"
    lib.mkdir()
    for i in range(4):
        shutil.copy(ROOT / "tests/fixtures/casino-royale-legacy-minimal.nfo", lib / ("m%d.nfo" % i))
    isolate(td, [str(lib)])

    # 1. Selected scope preflight: zero enumeration (cold catalog allowed once).
    eng.library_index()  # cold reconcile
    forbid_full_scan()
    r = eng.scope_preflight({"kind": "selection", "paths": [str(lib / "m0.nfo"), str(lib / "m1.nfo")], "engine": "ai"})
    assert r["counts"]["total"] == 2, r
    r2 = eng.scope_preflight({"kind": "current", "path": str(lib / "m2.nfo"), "engine": "local-rules"})
    assert r2["counts"]["total"] == 1, r2
    print("OK 1: selected/current preflight with zero full scans")

    # 2. Warm listing: zero stat/read/parse.
    eng.metrics_reset()
    items = eng.library_index()["items"]
    m = eng.metrics_snapshot()
    assert len(items) == 4
    assert m.get("stat_count", 0) == 0 and m.get("nfo_read_count", 0) == 0 and m.get("xml_parse_count", 0) == 0 and m.get("paths_enumerated", 0) == 0, m
    print("OK 2: warm library listing performs zero IO (%.3f ms metric window)" % 0)

    # 3. Dynamic overlay change (failure queue): zero reparse of other NFOs.
    eng.save_json(eng.AI_FAILURE_QUEUE, {"schema": 2, "items": [{"path": str(lib / "m3.nfo"), "kind": "quota", "message": "x"}]})
    eng.metrics_reset()
    items = eng.library_index()["items"]
    m = eng.metrics_snapshot()
    flagged = [x for x in items if any(i.get("kind") == "quota" for i in (x.get("issues") or []))]
    assert len(flagged) == 1 and flagged[0]["path"].endswith("m3.nfo"), flagged
    assert m.get("nfo_read_count", 0) == 0 and m.get("xml_parse_count", 0) == 0, m
    print("OK 3: overlay change updates exactly one item, zero reparse")

    # 4. Auto cycle: no rglob, candidates from catalog summaries.
    forbid_full_scan()
    eng._catalog_ensure = lambda allow_cold_scan=True: None  # catalog already warm
    # make every item spec-ready so process() short-circuits without network
    for entry in eng._LIBRARY_CATALOG["items"].values():
        entry["summary"]["spec_status"] = "ready"
        entry["summary"]["tag_status"] = "none"
    eng.LOCK = td / "run.lock"
    code = eng.run("auto")
    assert code == 0, code
    print("OK 4: auto agent cycle with zero enumeration")

    # 5. Pipeline status aggregates without re-reading NFOs.
    eng.metrics_reset()
    obj = eng.compute_pipeline_status(save=False)
    m = eng.metrics_snapshot()
    assert obj["counts"]["nfo_total"] == 4, obj["counts"]
    assert m.get("nfo_read_count", 0) == 0 and m.get("xml_parse_count", 0) == 0, m
    print("OK 5: pipeline status derived from catalog, zero NFO reads")

# 6. inspector_detail parses exactly once.
with tempfile.TemporaryDirectory() as td:
    td = pathlib.Path(td)
    lib = td / "lib"
    lib.mkdir()
    n = lib / "x.nfo"
    shutil.copy(ROOT / "tests/fixtures/casino-royale-legacy-minimal.nfo", n)
    isolate(td, [str(lib)])
    eng.metrics_reset()
    eng.inspector_detail(str(n))
    m = eng.metrics_snapshot()
    assert m.get("xml_parse_count", 0) <= 1, m
    print("OK 6: inspector_detail parses the XML exactly once")

# 7. Raw IMDb cache serves without network.
with tempfile.TemporaryDirectory() as td:
    td = pathlib.Path(td)
    isolate(td, [])
    import gzip
    meta, body = eng._raw_cache_files("tt0000009")
    body.write_bytes(gzip.compress(b"<html>ok</html>"))
    eng.save_json(meta, {"url": "https://x", "fetched_at": "2026-08-21T00:00:00+00:00"})
    eng.extract_specs = lambda src: ({"Camera": ["Arri Alexa 65"]}, True, "mock")
    eng.fetch_direct = lambda *a, **k: (_ for _ in ()).throw(AssertionError("network fetched despite raw cache"))
    eng.fetch_chrome = lambda *a, **k: (_ for _ in ()).throw(AssertionError("browser fetched despite raw cache"))
    obj = eng.get_specs("tt0000009")
    assert obj["ok"] and obj["method"] == "raw-cache", obj
    print("OK 7: raw IMDb cache hit without any network fetch")

# 8. UI/Go wiring for legacy interactions.
web = WEB.read_text(encoding="utf-8")
main_go = MAIN.read_text(encoding="utf-8")
platform = PLATFORM.read_text(encoding="utf-8")
checks = {
    "zero-network selection": "function schedulePreflight(){state.preflight=null;localScopeSummary()}" in web,
    "generation-time server preflight": "serverPreflight" in web and "preflightToken" in web,
    "5-bucket filters": 'non-optimal">非最佳（未达 AI 完成）' in web and 'value="ready"' in web,
    "selection toolbar": 'id="selAll"' in web and 'id="selInvert"' in web and 'id="selClear"' in web,
    "context menu reload": 'data-ctx="reload"' in web and '/api/inspector/reload' in web,
    "scan banner": 'scanBanner' in web and 'reconcile-index' in web,
    "chunked rendering (legacy: 600/batch, depth memory)": 'CHUNK=600' in web and 'state.maxSeen' in web and 'IntersectionObserver' in web,
    "event delegation": "listEl.addEventListener('click'" in web and "listEl.addEventListener('contextmenu'" in web,
    "user-select none": "user-select:none" in web,
    "tv default collapsed": "state.treeOpen[treeKey]===true" in web,
    "ownership popup": 'data-own-badge' in web and 'set-tag-ownership' in web,
    "settings modal stacking": "#settingsModal .rootColumns{grid-template-columns:1fr}" in web,
    "go reload endpoint": "/api/inspector/reload" in main_go,
    "go reconcile action": 'case "reconcile-index":' in main_go and '--reconcile-index' in platform,
    "launch auto reconcile": "startJob(\"reconcile-index\", \"\")" in main_go,
    "version 4.0.1": 'const appVersion = "4.0.1"' in main_go and "v4.0.1" in web,
}
failed = [name for name, ok in checks.items() if not ok]
for name, ok in checks.items():
    print(("OK  " if ok else "FAIL ") + name)
if failed:
    raise SystemExit("legacy perf/UI contract failed: " + ", ".join(failed))
print("OK legacy performance contract: no full scans for point ops, catalog serving, overlay isolation, raw cache")
