#!/usr/bin/env python3
"""legacy regressions for cache layers and incremental catalog reads."""
import importlib.util
import pathlib
import shutil
import tempfile


ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / "engine" / "mac-engine.py"
MAIN = ROOT / "main.go"
RESIDENT = ROOT / "resident_darwin.go"
WEB = ROOT / "web" / "index.html"


def load_engine(name):
    spec = importlib.util.spec_from_file_location(name, ENGINE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def isolate(module, td, library):
    for name in ("CFG", "AI_FAILURE_QUEUE", "ISSUE_ACKS", "INDEX_CACHE", "STATUS_OVERRIDES", "ROOT_HEALTH", "JOB_PROGRESS", "AI_STATUS", "AI_RUNTIME", "AI_BATCH_STATE", "AI_BATCH_QUEUE", "PIPELINE_STATUS", "STATUS", "PREVIEW_RESULTS", "MANUAL_TASK_FLAG", "LOCK"):
        setattr(module, name, td / name.lower())
    module.AI_BATCH_PAUSE = td / "ai-batch-pause.flag"
    module.OWNERSHIP_DIR = td / "ownership"
    module.UNDO_DIR = td / "undo"
    module.AI_CACHE = td / "ai-cache"
    module.CACHE = td / "cache"
    module.AI_CACHE.mkdir(exist_ok=True)
    module.CACHE.mkdir(exist_ok=True)
    module._INDEX_CACHE_MEM.update(stamp="", items={}, revision=0)
    module._DYNAMIC_OVERLAY_MEM.update(stamp=None, value={})
    module._LIBRARY_CATALOG.update(loaded=False, items={}, cache_mtime=None, cache_stamp=None)
    module.save_json(module.CFG, {"library_roots": {"movies": [str(library)], "tv": []}, "library_roots_confirmed": True})


main = MAIN.read_text(encoding="utf-8")
resident = RESIDENT.read_text(encoding="utf-8")
web = WEB.read_text(encoding="utf-8")
assert 'const appVersion = "4.0.4"' in main and "v4.0.4" in web
assert '"/api/library/changes"' in main and "handleLibraryChanges" in main
assert 'case "--library-changes-since"' in resident and 'cmd = "library-changes"' in resident
assert 'library-changes' in ENGINE.read_text(encoding="utf-8")
assert "catalogRevision" in web and "syncLibraryChanges()" in web and "/api/library/changes?since=" in web
assert "requestAnimationFrame" in web and "splitter.addEventListener('pointermove',apply)" in web
assert ".selectionTools .btn{padding:5px 9px;font-size:12px}" in web
assert "revision<state.catalogRevision" in web and "pendingCatalogPayload.reset" in web
print("OK 1: resident delta endpoint, UI batching, and splitter handling are wired")


with tempfile.TemporaryDirectory(prefix="imdb-tech-contract-") as raw:
    td = pathlib.Path(raw)
    library = td / "library"
    library.mkdir()
    nfo = library / "one.nfo"
    shutil.copy(ROOT / "tests" / "fixtures" / "casino-royale-legacy-minimal.nfo", nfo)
    eng = load_engine("eng_contract")
    isolate(eng, td, library)

    # A valid parsed cache must be returned before raw gzip is opened or HTML
    # is parsed; this is the hot path used by repeated selected-NFO actions.
    imdb = "tt0061452"
    eng.save_json(eng.cache_file(imdb), {
        "cache_version": eng.CACHE_VERSION, "parser_version": eng.PARSER_VERSION,
        "imdb": imdb, "fetched_at": "2026-08-22T00:00:00+00:00", "url": "https://example.invalid/",
        "status": "ok", "ok": True, "specs": {"Camera": ["cached"]},
    })
    eng.extract_specs = lambda _body: (_ for _ in ()).throw(AssertionError("cached result must not parse HTML"))
    cached = eng.get_specs(imdb)
    assert cached["specs"] == {"Camera": ["cached"]}
    assert eng.metrics_snapshot().get("imdb_parsed_cache_hit", 0) == 1

    # The catalog journal exposes only changes after the reader revision.
    stamp = eng._index_context_stamp()
    base = {str(nfo): {"summary": {"path": str(nfo), "title": "before", "media_space": "movies", "media_type": "movie", "xml_valid": True, "issues": [], "counts": {}}}}
    eng._index_cache_save(stamp, base)
    before_revision = eng.library_index(allow_cold_scan=False)["revision"]
    changed = {"summary": {"path": str(nfo), "title": "after", "media_space": "movies", "media_type": "movie", "xml_valid": True, "issues": [], "counts": {}}}
    eng._index_cache_append_delta(stamp, str(nfo), changed)
    delta = eng.library_changes(before_revision)
    assert not delta["reset"] and len(delta["changes"]) == 1 and delta["changes"][0]["item"]["title"] == "after", delta
    full = eng.library_changes(0)
    assert full["reset"] and full["items"][0]["title"] == "after", full

    # The authoritative scan commit must advance the base revision so a file
    # removed from the scan cannot remain forever in a delta client's UI.
    second = library / "two.nfo"
    second_entry = {"nfo_stamp": "second", "sidecar_stamp": "", "summary": {"path": str(second), "title": "removed", "media_space": "movies", "media_type": "movie", "xml_valid": True, "issues": [], "counts": {}}}
    kept_entry = dict(changed, nfo_stamp="kept", sidecar_stamp="")
    eng._index_cache_save(stamp, {str(nfo): kept_entry, str(second): second_entry})
    client_revision = eng._INDEX_CACHE_MEM["revision"]
    eng._LIBRARY_CATALOG.update(loaded=True, items={str(nfo): kept_entry})
    eng._catalog_persist()
    removed_delta = eng.library_changes(client_revision)
    assert removed_delta["reset"] and [item["path"] for item in removed_delta["items"]] == [str(nfo)], removed_delta

    # A point edit journaled after the scanner's row must survive final commit.
    point_entry = dict(kept_entry, nfo_stamp="newer", summary=dict(kept_entry["summary"], title="point edit"))
    eng._index_cache_append_delta(stamp, str(nfo), point_entry)
    eng._LIBRARY_CATALOG["items"] = {str(nfo): kept_entry}
    eng._catalog_persist()
    assert eng._LIBRARY_CATALOG["items"][str(nfo)]["summary"]["title"] == "point edit"

    # Preview paths must remain bound to configured roots before any AI work.
    outside = td / "outside.nfo"
    outside.write_text("<movie/>", encoding="utf-8")
    try:
        eng._allowed_nfo_path(str(outside))
    except ValueError:
        pass
    else:
        raise AssertionError("preview path outside configured roots was accepted")
print("OK 2: parsed-cache hot path, revision deltas/final commit, and preview root boundary are safe")
