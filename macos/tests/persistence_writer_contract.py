#!/usr/bin/env python3
"""legacy contract: mirror-id fix, AI-complete persistence, single-writer
catalog, existing_tags payload, clear-ai-tags, visible skips, quit chain."""
import importlib.util
import json
import os
import pathlib
import re
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
    eng.AI_STATUS = td / "ai-status.json"
    eng.AI_RUNTIME = td / "ai-runtime.json"
    eng.AI_BATCH_STATE = td / "ai-batch-state.json"
    eng.AI_BATCH_QUEUE = td / "ai-batch-queue.json"
    eng.AI_BATCH_PAUSE = td / "ai-batch-pause.flag"
    eng.PIPELINE_STATUS = td / "pipeline-status.json"
    eng.STATUS = td / "manager-status.json"
    eng.JOB_PROGRESS = td / "job-progress.json"
    eng.PREVIEW_RESULTS = td / "preview-results.json"
    eng.MANUAL_TASK_FLAG = td / "manual-task.flag"
    eng.AI_CACHE = td / "ai-cache"
    eng.AI_CACHE.mkdir(exist_ok=True)
    eng.CACHE = td / "cache"
    eng.CACHE.mkdir(exist_ok=True)
    eng.LOCK = td / "run.lock"
    eng._INDEX_CACHE_MEM.update(stamp="", items={})
    eng._LIBRARY_CATALOG.update(loaded=False, items={}, cache_mtime=None)
    eng.save_json(eng.CFG, {"roots": roots})


with tempfile.TemporaryDirectory() as td:
    td = pathlib.Path(td)
    lib = td / "lib"
    lib.mkdir()
    work = lib / "w.nfo"
    shutil.copy(ROOT / "tests/fixtures/casino-royale-legacy-minimal.nfo", work)
    isolate(td, [str(lib)])

    # ---------- 1. mirror-id root fix: rewrite -> sidecar matches NFO ----------
    text = work.read_text(encoding="utf-8")
    info = eng.inspect_nfo(text)
    source = {
        "imdb": "tt0061452", "fetched_at": "2026-08-22T00:00:00+00:00",
        "url": "https://www.imdb.com/title/tt0059/technical/",
        "status": "ok", "specs": {k: [] for k in eng.SECTIONS}, "ok": True, "ready": True,
    }
    source["specs"]["Camera"] = ["Arri Alexa 65"]
    assert eng.rewrite_specs_only(work, source, info) in ("updated", "current")
    st = eng.rewrite(work, eng.existing_tech_object(work.read_text(encoding="utf-8")), eng.inspect_nfo(work.read_text(encoding="utf-8")), tag_mode="local")
    assert st in ("updated", "current"), st
    detail = eng.inspector_detail(str(work))
    kinds = [i["kind"] for i in detail["issues"]]
    assert "ownership-mismatch" not in kinds, kinds
    print("OK 1: rewrite leaves NFO manifest and sidecar consistent (no mismatch issue)")

    # ---------- 2. recovery mode is not a mismatch ----------
    obj = eng.existing_tech_object(work.read_text(encoding="utf-8"))
    rec = eng.load_ownership_record(work, "tt0061452")
    assert obj and rec
    assert eng._entry_matches_sidecar(obj, rec) in (True, None)
    stripped = re.sub(r"\s*<generatedtags\b.*?</generatedtags>\s*", "\n", work.read_text(encoding="utf-8"), flags=re.S)
    work.write_text(stripped, encoding="utf-8")
    detail2 = eng.inspector_detail(str(work))
    assert "ownership-mismatch" not in [i["kind"] for i in detail2["issues"]]
    print("OK 2: TMM-stripped manifest (sidecar recovery) no longer reports mismatch")

    # ---------- 3. ordinary inspector self-heals a desynced sidecar ----------
    work.write_text(text, encoding="utf-8")
    eng.rewrite(work, eng.existing_tech_object(work.read_text(encoding="utf-8")), eng.inspect_nfo(work.read_text(encoding="utf-8")), tag_mode="local")
    obj3 = eng.existing_tech_object(work.read_text(encoding="utf-8"))
    bad = eng._manifest_result_from_obj(obj3)
    bad["entries"] = bad["entries"] + [{"value": "GHOST TAG", "field": "Camera", "source_indexes": [0]}]
    eng.save_ownership_record(work, "tt0061452", bad)
    healed_detail = eng.inspector_detail(str(work))
    assert "ownership-mismatch" not in [x["kind"] for x in healed_detail["issues"]]
    rec3 = eng.load_ownership_record(work, "tt0061452")
    values = [x.get("value") for x in rec3.get("entries", [])]
    assert "GHOST TAG" not in values, values
    print("OK 3: ordinary inspector rewrites a drifted sidecar from the authoritative NFO manifest")

    # Repair failure remains truthful, but the display issue can be ignored.
    eng.save_ownership_record(work, "tt0061452", bad)
    original_save_ownership = eng.save_ownership_record
    eng.save_ownership_record = lambda *args, **kwargs: (_ for _ in ()).throw(OSError("read-only mirror"))
    try:
        mismatch = eng.inspector_detail(str(work))
        assert mismatch["manifest_sidecar_match"] is False
        assert "ownership-mismatch" in [x["kind"] for x in mismatch["issues"]]
        ignored = eng.acknowledge_issue({
            "path": str(work), "expected_source_hash": mismatch["source_hash"],
            "kind": "ownership-mismatch", "operation": "ignore",
        })["item"]
        assert ignored["manifest_sidecar_match"] is False
        assert "ownership-mismatch" not in [x["kind"] for x in ignored["issues"]]
        assert "ownership-mismatch" in [x["kind"] for x in ignored["ignored_issues"]]
    finally:
        eng.save_ownership_record = original_save_ownership
    print("OK 3b: ignored mismatch hides only the issue; identity remains authoritatively inconsistent")

    # ---------- 4. single writer updates file + catalog together ----------
    eng._LIBRARY_CATALOG.update(loaded=True, items={}, cache_mtime=None)
    eng._INDEX_CACHE_MEM.update(stamp="", items={})
    eng.save_json(eng.INDEX_CACHE, {"schema": 2, "stamp": eng._index_context_stamp(), "items": {}})
    summary = eng.refresh_path_summary(work)
    data = eng.load_json(eng.INDEX_CACHE, {})
    real = os.path.realpath(str(work))
    assert real in data.get("items", {}) and real in eng._LIBRARY_CATALOG["items"]
    assert data["items"][real]["summary"]["title"] == summary["title"]
    print("OK 4: refresh_path_summary is the single writer for file + catalog")

    # ---------- 5. AI-complete persistence: prompt drift keeps the bucket ----------
    obj5 = eng.existing_tech_object(work.read_text(encoding="utf-8"))
    obj5["tag_engine"] = "ai"
    obj5["tag_model"] = "old-model"
    obj5["tag_prompt_hash"] = "0000"
    obj5["tag_state"] = "current"
    base5 = eng._summary_from_detail({"path": str(work), "xml_valid": True, "media_type": "movie",
        "spec_status": "ready", "tag_status": "ai-current", "tag_engine": "ai",
        "tag_model": "old-model", "tag_prompt_hash": "0000", "issues": [], "counts": {},
        "source_hash": "x", "title": "T", "imdb": "tt0061452"})
    eng.save_json(eng.CFG, {"roots": [str(lib)], "ai": {"model": "new-model", "prompt": eng.DEFAULT_AI_PROMPT, "base_url": "http://x", "enabled": True}})
    full5 = eng._apply_summary_overlay(base5)
    assert full5["tag_status"] == "ai-current" and full5.get("prompt_stale") and full5["lifecycle"] == "ai-complete", full5
    base5b = dict(base5); base5b["tag_status"] = "stale"  # spec hash mismatch (base layer)
    full5b = eng._apply_summary_overlay(base5b)
    assert full5b["lifecycle"] == "stale", full5b["lifecycle"]
    print("OK 5: prompt drift keeps AI-complete; spec edit downgrades to 可生成")

    # ---------- 6. existing_tags reach the AI request and the cache key ----------
    obj6 = eng.existing_tech_object(work.read_text(encoding="utf-8"))
    existing = eng._ai_existing_tags(obj6)
    assert existing and all(set(x) == {"value", "source"} for x in existing), existing
    cfg6 = {"provider": "p", "base_url": "u", "model": "m", "prompt": "x", "temperature": 0, "top_p": 1,
            "max_tokens": 1, "json_mode": "auto", "thinking_mode": "off", "prompt_cache_mode": "auto", "extra_body": "{}"}
    assert eng._ai_cache_key({}, cfg6, existing) != eng._ai_cache_key({}, cfg6, None)
    captured = {}
    def fake_retry(specs, cfg, with_json_mode, with_prompt_cache=False, existing=None):
        captured["existing"] = existing
        return {"tags": [], "warnings": [], "review_reasons": [], "usage": {}, "raw_model": "m"}
    eng._ai_request_with_retry = fake_retry
    eng.ai_ready = lambda **k: (True, "")
    eng.ai_config = lambda: dict(cfg6, prompt=eng.DEFAULT_AI_PROMPT)
    eng.ai_generate_tags({}, existing=existing)
    assert captured["existing"] == existing
    print("OK 6: existing_tags flow into the AI request and the cache key")

    # ---------- 7. failed preview clears stale results ----------
    eng.ai_ready = lambda **k: (False, "AI 已暂停")
    eng.ai_preview_write_results([str(work)])
    pr = eng.load_json(eng.PREVIEW_RESULTS, {})
    assert pr.get("failed") is True and pr.get("records") == [] and pr.get("error"), pr
    print("OK 7: failed preview wipes stale results (no approvable ghost)")

    # ---------- 8. clear-ai-tags removes only AI-owned tags ----------
    work.write_text(text, encoding="utf-8")
    eng.rewrite(work, eng.existing_tech_object(work.read_text(encoding="utf-8")), eng.inspect_nfo(work.read_text(encoding="utf-8")), tag_mode="local")
    d8 = eng.inspector_detail(str(work))
    assert any(t["ownership"] == "generated" for t in d8["tags"])
    eng.save_json(eng.CFG, {"roots": [str(lib)]})
    obj8 = eng.existing_tech_object(work.read_text(encoding="utf-8"))
    obj8["tag_engine"] = "ai"
    # put manifest into the NFO as an AI block so targets are AI-owned
    eng.rewrite(work, obj8, eng.inspect_nfo(work.read_text(encoding="utf-8")), tag_mode="preserve")
    d8b = eng.inspector_detail(str(work))
    try:
        eng.edit_nfo({"path": str(work), "expected_source_hash": d8b["source_hash"], "operation": "clear-ai-tags"})
        raised = False
    except ValueError:
        raised = True
    assert raised, "clear-ai-tags must require confirmation"
    res8 = eng.edit_nfo({"path": str(work), "expected_source_hash": d8b["source_hash"], "operation": "clear-ai-tags", "confirm": True})
    assert not any(t["ownership"] == "generated" for t in res8["item"]["tags"])
    print("OK 8: clear-ai-tags removes AI-owned tags only, behind a confirmation")

    # ---------- 9. visible skips in progress ----------
    eng.rewrite(work, eng.existing_tech_object(work.read_text(encoding="utf-8")), eng.inspect_nfo(work.read_text(encoding="utf-8")), tag_mode="local")
    eng.save_json(eng.JOB_PROGRESS, {"schema": 1, "action": "stale", "done": 99, "results": [{"path": "/ghost", "status": "old"}]})
    rc = eng.tag_generate("local", selected_paths=[str(work)])
    assert rc == 0, rc
    prog = eng.load_json(eng.JOB_PROGRESS, {})
    assert all(r.get("path") != "/ghost" for r in prog.get("results", [])), prog
    assert any(r.get("status", "").startswith("skipped") for r in prog.get("results", [])), prog
    print("OK 9: progress starts clean and every skip is visible with a reason")

# ---------- 10. prompt migration + wiring ----------
web = WEB.read_text(encoding="utf-8")
main_go = MAIN.read_text(encoding="utf-8")
platform = PLATFORM.read_text(encoding="utf-8")
checks = {
    "legacy default prompt auto-upgrades": True,  # verified via ai_config tuple below
    "quit endpoint": '/api/quit' in main_go and "handleQuit" in main_go,
    "quit unloads agent session": "bootout" in platform and 'launchctl", "remove"' in platform.replace("/bin/", ""),
    "keepalive disabled": "<key>KeepAlive</key><false/>" in platform,
    "heartbeat 15s": "15*time.Second" in main_go.replace(" ", "") or "> 15*time.Second" in main_go,
    "resident killed on exit": "residentShutdown" in main_go and "residentShutdown" in platform,
    "post-task auto reconcile": 'startJob("reconcile-index", "")' in main_go,
    "progress alias broadened": 'HasPrefix(action, "ai-")' in main_go,
    "theme-color titlebar": '<meta name="theme-color" content="#0d1117">' in web,
    "native title cleared": "document.title='\\u200b'" in web and "<title>&#8203;</title>" in web,
    "path wraps": "word-break:break-all;max-width:none" in web,
    "status pill never squeezed": "#inspectStatus{flex-shrink:0}" in web,
    "version light weight": "#version{font-weight:400;font-size:13px" in web,
    "prompt textarea taller": "#aiPrompt{min-height:300px" in web,
    "settings header sticky": "#settingsModal .modalHead{position:sticky" in web,
    "settings title de-versioned": ">设置</h2>" in web,
    "presets removed": "filterPreset" not in web and "savePreset" not in web,
    "save-and-test removed": 'id="testAI"' not in web,
    "rescan-all removed": 'id="rescanAll"' not in web,
    "detail-toggle removed": "taskDetailToggle" not in web,
    "refresh = current-space reconcile": "刷新当前媒体库" in web and "action('reconcile-index',{path:state.space})" in web,
    "try-write placement": web.index('id="generateEngine"') < web.index('id="previewScope"') < web.index('id="generateNow"') and '>试写</button>' in web,
    "AI + rules try-write": "local-preview-write-selected" in web and "local-approve-selected" in web and "classList.toggle('hide',q('#generateEngine').value!=='ai')" not in web,
    "badge popup ownership": "data-own-badge" in web and "ownMenu" in web and "›" in web,
    "clear ai tags button": 'id="clearAiTags"' in web and "clear-ai-tags" in web,
    "batch card gone": "批量摘要" not in web,
    "history sticky": "viewingHistory" in web,
    "cli autoscroll": "q('#jobLog').scrollTop=q('#jobLog').scrollHeight" in web,
    "chunk 600 + depth memory": "CHUNK=600" in web and "state.maxSeen" in web,
    "progress patches rows only": "window.__patchRowLib" in web and "if(patched)renderLibrary()" not in web,
    "quit overlay": 'id="quitOverlay"' in web and 'id="quitApp"' in web,
    "preview gated on success": "if(job.exit_code)" in web,
    "version 4.1.0": 'const appVersion = "4.1.0"' in main_go and "v4.1.0" in web,
    "engine prompt rule existing_tags": "existing_tags" in ENGINE.read_text(encoding="utf-8"),
    "prompt migration includes legacy prompt": "DEFAULT_AI_PROMPT_LEGACY_EXISTING_TAGS" in ENGINE.read_text(encoding="utf-8"),
}
# verify migration functionally
import importlib.util as _iu
_s = _iu.spec_from_file_location("eng_contract", ENGINE)
_m = _iu.module_from_spec(_s); _s.loader.exec_module(_m)
with tempfile.TemporaryDirectory() as td2:
    _m.CFG = pathlib.Path(td2) / "c.json"
    _m.save_json(_m.CFG, {"ai": {"prompt": _m.DEFAULT_AI_PROMPT_LEGACY_EXISTING_TAGS}})
    checks["legacy default prompt auto-upgrades"] = _m.ai_config()["prompt"] == _m.DEFAULT_AI_PROMPT

failed = [name for name, ok in checks.items() if not ok]
for name, ok in checks.items():
    print(("OK  " if ok else "FAIL ") + name)
if failed:
    raise SystemExit("legacy contract failed: " + ", ".join(failed))
print("OK legacy contract: mirror fix, persistence, single writer, existing_tags, clear-ai-tags, skips, quit chain, UI")
