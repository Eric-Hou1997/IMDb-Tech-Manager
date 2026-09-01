#!/usr/bin/env python3
"""legacy contract: lifecycle model, canonical ownership, delete-tag,
manual status override, index cache, AI preview-approve, serve mode."""
import importlib.util
import json
import os
import pathlib
import subprocess
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / "engine" / "mac-engine.py"
WEB = ROOT / "web" / "index.html"
MAIN = ROOT / "main.go"

spec = importlib.util.spec_from_file_location("eng_contract", ENGINE)
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)

# ---------- 1. unified lifecycle model ----------
assert eng._lifecycle_state(True, "movie", "ready", "ai-current") == "ai-complete"
assert eng._lifecycle_state(True, "movie", "ready", "local-current") == "local-complete"
assert eng._lifecycle_state(True, "movie", "ready", "none") == "no-tags"
assert eng._lifecycle_state(True, "movie", "ready", "stale") == "stale"
assert eng._lifecycle_state(True, "movie", "ready", "legacy") == "legacy"
assert eng._lifecycle_state(True, "movie", "ready", "current") == "legacy"
assert eng._lifecycle_state(True, "movie", "missing", "none") == "spec-missing"
assert eng._lifecycle_state(True, "movie", "empty", "none") == "spec-empty"
assert eng._lifecycle_state(False, "movie", "ready", "ai-current") == "xml-error"
assert eng._lifecycle_state(True, "season", "ready", "none") == "not-applicable"
assert eng._lifecycle_state(True, "movie", "ready", "stale", "ai-complete") == "ai-complete"

# ---------- 2. film-split prompt + legacy default migration ----------
assert "括号里面是三种胶卷" in eng.DEFAULT_AI_PROMPT
assert "应该生成三个标签" in eng.DEFAULT_AI_PROMPT
assert "16mm（Kodak Vision3 500T 7219）" in eng.DEFAULT_AI_PROMPT
assert "应该生成两个标签" not in eng.DEFAULT_AI_PROMPT
assert eng.DEFAULT_AI_PROMPT != eng.DEFAULT_AI_PROMPT_LEGACY_SPLIT
with tempfile.TemporaryDirectory() as td:
    td = pathlib.Path(td)
    eng.CFG = td / "config.json"
    eng.save_json(eng.CFG, {"ai": {"prompt": eng.DEFAULT_AI_PROMPT_LEGACY_SPLIT}})
    assert eng.ai_config()["prompt"] == eng.DEFAULT_AI_PROMPT, "legacy default prompt must upgrade"
    custom = eng.DEFAULT_AI_PROMPT + "\n用户自定义补充"
    eng.save_json(eng.CFG, {"ai": {"prompt": custom}})
    assert eng.ai_config()["prompt"] == custom, "customized prompts must never be overwritten"

# ---------- 3. temp library: lifecycle, canonical ownership, edits ----------
with tempfile.TemporaryDirectory() as td:
    td = pathlib.Path(td)
    library = td / "媒体库"
    library.mkdir()
    eng.CFG = td / "config.json"
    eng.AI_FAILURE_QUEUE = td / "failures.json"
    eng.ISSUE_ACKS = td / "acks.json"
    eng.INDEX_CACHE = td / "index-cache.json"
    eng.STATUS_OVERRIDES = td / "status-overrides.json"
    eng.ROOT_HEALTH = td / "root-health.json"
    eng.OWNERSHIP_DIR = td / "ownership"
    eng.UNDO_DIR = td / "undo"
    eng.JOB_PROGRESS = td / "job-progress.json"
    eng.PREVIEW_RESULTS = td / "preview-results.json"
    eng.MANUAL_TASK_FLAG = td / "manual-task.flag"
    eng.AI_CACHE = td / "ai-cache"
    eng.AI_CACHE.mkdir(exist_ok=True)
    eng._INDEX_CACHE_MEM["stamp"] = ""
    eng._INDEX_CACHE_MEM["items"] = {}
    eng.save_json(eng.CFG, {"roots": [str(library)]})

    plain = library / "plain.nfo"
    plain.write_text("<movie><title>普通片</title><uniqueid type=\"imdb\">tt9000001</uniqueid></movie>", encoding="utf-8")
    work = library / "work.nfo"
    work.write_text("<movie><title>工作片</title><uniqueid type=\"imdb\">tt9000002</uniqueid><tag>TMM 外部标签</tag></movie>", encoding="utf-8")

    idx = {pathlib.Path(x["path"]).name: x for x in eng.library_index()["items"]}
    assert idx["plain.nfo"]["lifecycle"] == "spec-missing", idx["plain.nfo"]
    assert idx["work.nfo"]["lifecycle"] == "spec-missing", idx["work.nfo"]

    # Local-rule generation writes specs+tags and the ownership manifest.
    info = eng.inspect_nfo(work.read_text(encoding="utf-8"))
    source = {
        "imdb": "tt9000002", "fetched_at": "2026-08-20T00:00:00+00:00",
        "url": "https://www.imdb.com/title/tt9000002/technical/",
        "status": "ok", "specs": {k: [] for k in eng.SECTIONS}, "ok": True, "ready": True,
    }
    source["specs"]["Aspect ratio"] = ["2.39 : 1"]
    source["specs"]["Camera"] = ["Arri Alexa 65"]
    assert eng.rewrite_specs_only(work, source, info) == "updated"
    st = eng.rewrite(work, eng.existing_tech_object(work.read_text(encoding="utf-8")), info, tag_mode="local")
    assert st in ("updated", "current"), st
    after = work.read_text(encoding="utf-8")
    obj = eng.existing_tech_object(after)
    manifest_aspect = [v for v in eng._manifest_values(obj) if ":" in v]
    assert manifest_aspect, eng._manifest_values(obj)

    # Canonical ownership: the spaced legacy spelling must still be ours.
    spaced = manifest_aspect[0].replace(":", " : ") + " (some scenes)"
    mutated = after.replace("<tag>%s</tag>" % manifest_aspect[0], "<tag>%s</tag>" % spaced)
    assert mutated != after, "fixture must actually contain the normalized tag"
    work.write_text(mutated, encoding="utf-8")
    obj2 = eng.existing_tech_object(mutated)
    rows = {r["value"]: r["ownership"] for r in eng._tag_rows(mutated, obj2)}
    assert rows.get(spaced) == "generated", rows
    assert eng.tag_state(mutated, obj2, work) != "tag-missing"
    detail = eng.inspector_detail(str(work))
    assert detail["lifecycle"] == "local-complete", detail["lifecycle"]

    # ---------- 4. delete-tag: external needs confirmation, owned cleans manifest ----------
    ext_index = [i for i, r in enumerate(detail["tags"]) if r["ownership"] == "external"]
    assert ext_index, detail["tags"]
    base_payload = {"path": str(work), "expected_source_hash": detail["source_hash"], "operation": "delete-tag", "target": {"root_index": ext_index[0]}}
    try:
        eng.edit_nfo(dict(base_payload))
        raise AssertionError("external delete must require confirmation")
    except ValueError as exc:
        assert "外部标签" in str(exc), exc
    result = eng.edit_nfo(dict(base_payload, confirm_external=True))
    values = [t["value"] for t in result["item"]["tags"]]
    assert "TMM 外部标签" not in values, values

    gen_index = [i for i, r in enumerate(result["item"]["tags"]) if r["ownership"] == "generated"]
    assert gen_index, result["item"]["tags"]
    target_value = result["item"]["tags"][gen_index[0]]["value"]
    result2 = eng.edit_nfo({"path": str(work), "expected_source_hash": result["item"]["source_hash"], "operation": "delete-tag", "target": {"root_index": gen_index[0]}})
    obj3 = eng.existing_tech_object(work.read_text(encoding="utf-8"))
    assert target_value not in eng._manifest_values(obj3), eng._manifest_values(obj3)
    assert target_value not in eng._all_normal_tags(work.read_text(encoding="utf-8"))

    # ---------- 5. manual status override and auto-invalidation ----------
    before = eng.inspector_detail(str(work))
    r = eng.set_status_override({"path": str(work), "value": "ai-complete"})
    assert r["item"]["lifecycle"] == "ai-complete" and r["item"]["status_override"] == "ai-complete"
    try:
        eng.set_status_override({"path": str(work), "value": "bogus"})
        raise AssertionError("invalid override must be rejected")
    except ValueError:
        pass
    # Content change invalidates the override automatically.
    text = work.read_text(encoding="utf-8")
    work.write_text(text.replace("<title>工作片</title>", "<title>工作片 2</title>"), encoding="utf-8")
    after2 = eng.inspector_detail(str(work))
    assert after2["status_override"] == "" and after2["lifecycle"] != "ai-complete"
    eng.set_status_override({"path": str(work), "value": ""})

    # ---------- 6. LibraryCatalog: epoch serving, point-read freshness, reconcile ----------
    eng._INDEX_CACHE_MEM["stamp"] = ""
    eng._INDEX_CACHE_MEM["items"] = {}
    eng._LIBRARY_CATALOG["loaded"] = False
    eng._LIBRARY_CATALOG["items"] = {}
    first = eng.library_index()["items"]
    key = os.path.realpath(str(work))
    assert key in eng._LIBRARY_CATALOG["items"], "library_index must populate the catalog"
    marker = dict(eng._LIBRARY_CATALOG["items"][key]["summary"])
    marker["title"] = "CACHE-MARKER"
    eng._LIBRARY_CATALOG["items"][key]["summary"] = marker
    second = eng.library_index()["items"]
    assert any(x.get("title") == "CACHE-MARKER" for x in second), "listing serves the catalog without re-stat"
    # Listing stays epoch-based, but point reads validate stamps per path, so
    # external edits sync through the per-path fast path...
    os.utime(str(work), None)
    fresh = eng._cached_summary_for_path(work)
    assert fresh.get("title") != "CACHE-MARKER", "per-path fast path must invalidate on mtime change"
    # ...and an explicit reconcile re-stats everything.
    eng._LIBRARY_CATALOG["items"][key]["summary"] = marker
    eng._catalog_reconcile(reason="test")
    third = eng.library_index()["items"]
    assert not any(x.get("title") == "CACHE-MARKER" for x in third), "reconcile must invalidate changed entries"

    # ---------- 7. preview-approve writes the cached AI result, never the network ----------
    eng.save_json(eng.CFG, {
        "roots": [str(library)],
        "ai": {"enabled": True, "base_url": "http://127.0.0.1:9", "model": "test-model", "prompt": eng.DEFAULT_AI_PROMPT},
    })
    eng._INDEX_CACHE_MEM["stamp"] = ""
    eng._INDEX_CACHE_MEM["items"] = {}
    assert eng.rewrite_specs_only(work, source, eng.inspect_nfo(work.read_text(encoding="utf-8"))) in ("updated", "current")
    obj4 = eng.existing_tech_object(work.read_text(encoding="utf-8"))
    cfg = eng.ai_config()
    tags = []
    for field in eng.TAG_SECTIONS:
        for i, value in enumerate(obj4["specs"].get(field, [])):
            tags.append({"value": value, "field": field, "source_indexes": [i], "confidence": "high", "operation": "approve-test"})
    key = eng._ai_cache_key(obj4["specs"], cfg, eng._ai_existing_tags(obj4) or None)
    eng.save_json(eng.AI_CACHE / ("%s.json" % key), {
        "cache_schema": eng.AI_CACHE_SCHEMA, "model": "test-model",
        "prompt_hash": eng.hashlib.sha256(cfg["prompt"].encode("utf-8")).hexdigest()[:16],
        "spec_hash": eng._specs_hash(obj4["specs"]), "usage": {}, "cost": 0,
        "result": {"tags": tags, "warnings": []},
    })

    def _no_network(*a, **k):
        raise AssertionError("approve must not call the AI provider")

    eng.ai_generate_tags = _no_network
    eng.resolve_tag_entries = _no_network
    eng.ai_ready = lambda **k: (True, "")
    code = eng.ai_approve_write([str(work)])
    assert code == 0, code
    final = eng.existing_tech_object(work.read_text(encoding="utf-8"))
    assert final.get("tag_engine") == "ai", final.get("tag_engine")
    assert eng.tag_state(work.read_text(encoding="utf-8"), final, work) == "ai-current"
    # Without a cached preview result the approve path must refuse, not call AI.
    code2 = eng.ai_approve_write([str(plain)])
    assert code2 != 0

    # ---------- 8. manual task flag + progress file ----------
    assert not eng._manual_task_requested()
    eng.MANUAL_TASK_FLAG.write_text("test", encoding="utf-8")
    assert eng._manual_task_requested()
    with eng._manual_task_session():
        assert not eng._manual_task_requested(), "the owner must not be preempted by its own flag"
    assert not eng.MANUAL_TASK_FLAG.exists()
    eng._progress_write("ai-generate", 1, 3, "/tmp/x.nfo", [{"path": "/tmp/x.nfo", "status": "updated", "tag_status": "ai-current"}])
    pr = json.loads(eng.JOB_PROGRESS.read_text(encoding="utf-8"))
    assert pr["schema"] == 1 and pr["done"] == 1 and pr["total"] == 3 and pr["results"][0]["tag_status"] == "ai-current"
    eng._progress_clear()

# ---------- 9. resident serve mode loopback ----------
proc = subprocess.Popen(
    [sys.executable, str(ENGINE), "--serve"],
    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
    text=True, encoding="utf-8",
)

def call(req):
    proc.stdin.write(json.dumps(req) + "\n")
    proc.stdin.flush()
    return json.loads(proc.stdout.readline())

try:
    pong = call({"id": 1, "cmd": "ping"})
    assert pong["ok"] and pong["result"]["pong"] is True, pong
    bad = call({"id": 2, "cmd": "inspector-detail", "path": "/nonexistent/outside.nfo"})
    assert bad["ok"] is False and bad["kind"] == "PathOutsideLibraryError", bad
    unknown = call({"id": 3, "cmd": "does-not-exist"})
    assert unknown["ok"] is False and "未知" in unknown["error"], unknown
finally:
    proc.stdin.close()
    proc.wait(timeout=15)

# ---------- 10. UI/Go wiring ----------
web = WEB.read_text(encoding="utf-8")
main_go = MAIN.read_text(encoding="utf-8")
checks = {
    "lifecycle filter 非最佳": "非最佳（未达 AI 完成）" in web,
    "AI 完成 rename": "AI 完成" in web and "AI 当前" not in web and "AI Current" not in web,
    "spec-ready excludes AI-complete in UI": "if(lc==='ai-complete')return'ai'" in web and "return'ready'" in web,
    "select all toolbar": "selAll" in web and "selInvert" in web and "selClear" in web,
    "shift/ctrl multi-select": "event.shiftKey" in web and "event.ctrlKey" in web,
    "sorting control": "data-sort-field" in web and "sortArrow" in web and "sortSelect" not in web,
    "AI bright yellow dot": ".stateDot.ai{background:var(--yellow)" in web,
    "manual outlined yellow": ".stateDot.manual{background:transparent;border:2px solid var(--yellow)" in web,
    "spec-ready not green": ".stateDot.no-tags{background:var(--blue)" in web,
    "delete-tag button": "data-delete-tag" in web,
    "no pencil emoji": "Manual ✎" not in web,
    "manual status selector": "statusOverrideSel" in web,
    "preview approve flow": "ai-preview-write-selected" in web and "ai-approve-selected" in web and "local-preview-write-selected" in web and "local-approve-selected" in web and "采纳并写入" in web,
    "api timeout": "AbortController" in web,
    "fast polling": "setTimeout(refreshJob,900)" in web,
    "scope auto-selection": "automaticScope" in web and "selected.length" in web,
    "go approve endpoint": "ai-approve-selected" in main_go,
    "go preview results endpoint": "/api/preview/results" in main_go,
    "go status override endpoint": "/api/inspector/status" in main_go,
    "go job progress": "loadJobProgress" in main_go,
    "resident process": "residentInspectorJSON" in (ROOT / "platform_darwin.go").read_text(encoding="utf-8"),
    "engine serve flag": "--serve" in ENGINE.read_text(encoding="utf-8"),
}
failed = [name for name, ok in checks.items() if not ok]
if failed:
    raise SystemExit("FAILED: " + ", ".join(failed))

print("OK legacy contract: lifecycle, canonical ownership, delete-tag, override, index cache, approve-without-network, serve mode, UI wiring")
