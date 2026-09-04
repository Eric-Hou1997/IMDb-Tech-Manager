#!/usr/bin/env python3
"""legacy regressions: startup, live catalog, defaults, and UI."""
import ast
import importlib.util
import json
import os
import pathlib
import re
import shutil
import subprocess
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / "engine" / "mac-engine.py"
MAIN = ROOT / "main.go"
PLATFORM = ROOT / "platform_darwin.go"
WEB = ROOT / "web" / "index.html"


def load_engine(name):
    spec = importlib.util.spec_from_file_location(name, ENGINE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def isolate(module, td, library):
    for name in (
        "CFG", "AI_FAILURE_QUEUE", "ISSUE_ACKS", "INDEX_CACHE",
        "STATUS_OVERRIDES", "ROOT_HEALTH", "JOB_PROGRESS", "AI_STATUS",
        "AI_RUNTIME", "AI_BATCH_STATE", "AI_BATCH_QUEUE", "PIPELINE_STATUS",
        "STATUS", "PREVIEW_RESULTS", "MANUAL_TASK_FLAG", "LOCK",
    ):
        setattr(module, name, td / name.lower())
    module.AI_BATCH_PAUSE = td / "ai-batch-pause.flag"
    module.OWNERSHIP_DIR = td / "ownership"
    module.UNDO_DIR = td / "undo"
    module.AI_CACHE = td / "ai-cache"
    module.CACHE = td / "cache"
    module.AI_CACHE.mkdir(exist_ok=True)
    module.CACHE.mkdir(exist_ok=True)
    module._INDEX_CACHE_MEM.update(stamp="", items={})
    module._DYNAMIC_OVERLAY_MEM.update(stamp=None, value={})
    module._LIBRARY_CATALOG.update(loaded=False, items={}, cache_mtime=None, cache_stamp=None)
    module.save_json(module.CFG, {
        "library_roots": {"movies": [str(library)], "tv": []},
        "library_roots_confirmed": True,
    })


main = MAIN.read_text(encoding="utf-8")
platform = PLATFORM.read_text(encoding="utf-8")
web = WEB.read_text(encoding="utf-8")
engine_source = ENGINE.read_text(encoding="utf-8")

# Configured startup must return before optional candidate discovery.
handler = re.search(r"func handleOnboarding\(.*?\n}\n", main, re.S).group(0)
assert handler.index("platformLibraryRootsConfirmed()") < handler.index("platformDiscoverRootCandidates()")
assert 'const appVersion = "4.0.4"' in main and "v4.0.4" in web
assert 'p.add_argument("--discover-root-candidates"' in engine_source
assert "if a.discover_root_candidates:" in engine_source

with tempfile.TemporaryDirectory(prefix="imdb-tech-contract-cli-") as home:
    env = dict(os.environ, HOME=home)
    run = subprocess.run(
        ["python3", str(ENGINE), "--discover-root-candidates"],
        text=True, capture_output=True, timeout=20, env=env,
    )
    assert run.returncode == 0, run.stderr
    assert isinstance(json.loads(run.stdout).get("candidates"), list)
print("OK 1: configured startup bypasses discovery; read-only discovery CLI works")


with tempfile.TemporaryDirectory(prefix="imdb-tech-contract-catalog-") as raw:
    td = pathlib.Path(raw)
    library = td / "library"
    library.mkdir()
    fixture = ROOT / "tests" / "fixtures" / "casino-royale-legacy-minimal.nfo"
    for index in range(60):
        shutil.copy(fixture, library / ("movie-%03d.nfo" % index))

    eng = load_engine("eng_contract_writer")
    isolate(eng, td, library)
    stamp = eng._index_context_stamp()
    stale_path = str(td / "old-test-snapshot.nfo")
    stale_entry = {"summary": {"path": stale_path, "title": "stale", "media_space": "movies", "media_type": "movie", "xml_valid": True, "issues": [], "counts": {}}}
    eng._index_cache_save(stamp, {stale_path: stale_entry})

    partial_counts = []
    original_progress = eng._progress_write

    def observe_partial(action, done, total, current, results):
        original_progress(action, done, total, current, results)
        if done == 50:
            reader = load_engine("eng_contract_reader")
            isolate(reader, td, library)
            items = reader.library_index(allow_cold_scan=False)["items"]
            partial_counts.append(len(items))
            assert any(item.get("path") == stale_path for item in items), "partial scan must not delete the reliable base"

    eng._progress_write = observe_partial
    counts = eng._catalog_reconcile("contract")
    assert partial_counts == [51], partial_counts
    assert counts == {"reason": "contract", "space": "all", "total": 60, "reused": 0, "reparsed": 60}
    final = eng.library_index(allow_cold_scan=False)["items"]
    assert len(final) == 60 and all(item.get("path") != stale_path for item in final)
    assert eng.load_json(eng.ROOT_HEALTH, {}).get(str(library), {}).get("nfo_count") == 60

    # A UI listing may stat its two local cache files but must never resolve,
    # read, parse, or enumerate a media path.
    eng.metrics_reset()
    original_realpath = eng.os.path.realpath
    eng.os.path.realpath = lambda value: (_ for _ in ()).throw(AssertionError("NAS realpath during library read"))
    try:
        assert len(eng.library_index(allow_cold_scan=False)["items"]) == 60
    finally:
        eng.os.path.realpath = original_realpath
    metrics = eng.metrics_snapshot()
    assert metrics.get("stat_count", 0) == 0
    assert metrics.get("nfo_read_count", 0) == 0
    assert metrics.get("xml_parse_count", 0) == 0

    # A fresh reconcile process must load the prior snapshot and reuse all
    # unchanged NFOs instead of repeating the legacy full reparse.
    eng._LIBRARY_CATALOG.update(loaded=False, items={}, cache_mtime=None, cache_stamp=None)
    eng._INDEX_CACHE_MEM.update(stamp="", items={})
    eng._progress_write = original_progress
    reused = eng._catalog_reconcile("contract-reuse")
    assert reused["reused"] == 60 and reused["reparsed"] == 0, reused
print("OK 2: scan publishes batches, preserves base until commit, and reuses all unchanged NFOs")


eng = load_engine("eng_contract_defaults")
with tempfile.TemporaryDirectory(prefix="imdb-tech-contract-defaults-") as raw:
    td = pathlib.Path(raw)
    library = td / "library"
    library.mkdir()
    isolate(eng, td, library)
    fresh = eng.ai_config()
    assert (fresh["max_tokens"], fresh["output_token_cap"]) == (2000, 10000)
    eng.save_json(eng.CFG, {"ai": {"max_tokens": 1800, "output_token_cap": 8192, "prompt": eng.DEFAULT_AI_PROMPT_LEGACY_STRUCTURED}})
    migrated = eng.ai_config()
    assert (migrated["max_tokens"], migrated["output_token_cap"], migrated["prompt"]) == (2000, 10000, eng.DEFAULT_AI_PROMPT)
    eng.save_json(eng.CFG, {"ai": {"max_tokens": 2500, "output_token_cap": 12000, "prompt": "custom prompt"}})
    custom = eng.ai_config()
    assert (custom["max_tokens"], custom["output_token_cap"], custom["prompt"]) == (2500, 12000, "custom prompt")

tree = ast.parse(engine_source)
python_prompt = next(ast.literal_eval(node.value) for node in tree.body if isinstance(node, ast.Assign) and any(isinstance(target, ast.Name) and target.id == "DEFAULT_AI_PROMPT" for target in node.targets))
go_prompt = re.search(r"const defaultAIPrompt = `(.*?)`\n\nconst legacyDefaultAIPromptInitial", main, re.S).group(1)
assert go_prompt == python_prompt
assert "生成完检查每一项中有没有遗漏未生成的 Tag" in python_prompt
assert "胶片规格括号内逗号分隔的多张胶卷必须逐项拆分" in python_prompt
assert "existing_tags" in python_prompt
assert "应该生成两个标签" not in python_prompt
assert "ai.max_tokens||2000" in web and "ai.output_token_cap||10000" in web
assert "MaxTokens: 2000, OutputTokenCap: 10000" in platform
print("OK 3: 2000/10000 defaults, stock migration, custom preservation, and canonical prompt")


assert "rootHealth" not in re.search(r"function renderRootGroups\(\).*?\n", web).group(0)
assert 'class="rootPrimaryActions"' in web
assert web.index('class="checkLabel autoStartLabel"') < web.index('class="autoModeHelp"')
assert "syncLibraryChanges()" in web and "lastPublishedScanProgress" in web
assert "/api/library/changes?since=" in web and "catalogRevision" in web
print("OK 4: settings layout and revisioned scan-time UI refresh are wired")


with tempfile.TemporaryDirectory(prefix="imdb-tech-contract-rule-preview-") as raw:
    td = pathlib.Path(raw)
    library = td / "library"
    library.mkdir()
    work = library / "rule-preview.nfo"
    fixture_text = (ROOT / "tests" / "fixtures" / "casino-royale-legacy-minimal.nfo").read_text(encoding="utf-8")
    # Keep a real supported Technical Specs block, but remove every root tag so
    # strict ownership has nothing external to infer or delete.
    fixture_text = re.sub(r"^\s*<tag>.*?</tag>\s*\n", "", fixture_text, flags=re.M)
    work.write_text(fixture_text, encoding="utf-8")
    rules = load_engine("eng_contract_rule_preview")
    isolate(rules, td, library)
    before = work.read_bytes()

    def no_ai(*_args, **_kwargs):
        raise AssertionError("rules preview must never call AI")

    rules.ai_generate_tags = no_ai
    assert rules.local_preview_write_results([str(work)]) == 0
    preview = rules.load_json(rules.PREVIEW_RESULTS, {})
    assert preview.get("engine") == "local-rules" and len(preview.get("records", [])) == 1, preview
    record = preview["records"][0]
    assert record.get("source_hash") and record.get("spec_hash") and record.get("rules_version") == rules.LOCAL_RULES_VERSION
    assert record.get("cleanup_mode") == "strict"
    assert record.get("new_tags") and record.get("tag_result", {}).get("engine") == "local-rules"
    assert work.read_bytes() == before, "试写阶段不得修改 NFO"
    assert not pathlib.Path(str(work) + ".imdbtech.bak").exists(), "试写阶段不得创建写入备份"

    assert rules.local_approve_write([str(work)]) == 0
    written = work.read_text(encoding="utf-8")
    obj = rules.existing_tech_object(written)
    assert obj.get("tag_engine") == "local-rules"
    assert rules.tag_state(written, obj, work) == "local-current"

    # A stale approval must not overwrite anything changed after the preview.
    assert rules.local_preview_write_results([str(work)]) == 0
    changed = work.read_text(encoding="utf-8").replace("007别传：皇家夜总会", "007别传：皇家夜总会（外部修改）")
    work.write_text(changed, encoding="utf-8")
    changed_bytes = work.read_bytes()
    assert rules.local_approve_write([str(work)]) != 0
    assert work.read_bytes() == changed_bytes

    unsafe = library / "unsafe-legacy.nfo"
    shutil.copy(ROOT / "tests" / "fixtures" / "casino-royale-legacy-minimal.nfo", unsafe)
    assert rules.local_preview_write_results([str(unsafe)]) != 0
    unsafe_preview = rules.load_json(rules.PREVIEW_RESULTS, {})
    assert unsafe_preview.get("records") == [] and unsafe_preview.get("skipped", [{}])[0].get("error")
    assert unsafe.read_bytes() == (ROOT / "tests" / "fixtures" / "casino-royale-legacy-minimal.nfo").read_bytes()
print("OK 5: rules try-write is no-AI/no-write and approval is CAS-protected")


assert '<select class="select" id="generateEngine"><option value="ai">AI 生成标签</option>' in web
assert '<button class="btn" id="previewScope">试写</button>' in web
assert "local-preview-write-selected" in web and "local-approve-selected" in web
assert "local-preview-write-selected" in main and "local-approve-selected" in main
assert "--local-preview-write-json" in platform and "--local-approve-json" in platform
assert ".scopebar>.directActions,.scopebar>.workflow{flex-wrap:nowrap" in web
assert ".workflow{width:100%" not in web
assert ".workspace{display:grid;grid-template-columns:minmax(0,var(--left)) 12px minmax(0,1fr)" in web
assert "function splitLimits(width)" in web and "function applySplitRatio(" in web
assert ".library.narrow .toolbarPrimary{" in web and ".library.narrow .toolbarSecondary{" in web
assert "minmax(280px,var(--left))" not in web and "minmax(240px,var(--left))" not in web
assert "document.title='\\u200b'" in web and "<title>&#8203;</title>" in web
print("OK 6: AI is the default and the same-row rules try-write flow is fully wired")

print("OK legacy contract")
