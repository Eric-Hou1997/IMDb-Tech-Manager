#!/usr/bin/env python3
"""Language isolation, dateadded, and scoped refresh contracts."""
import importlib.util
import json
import os
import pathlib
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / "engine" / "mac-engine.py"
WEB = (ROOT / "web" / "index.html").read_text(encoding="utf-8")
MAIN = (ROOT / "main.go").read_text(encoding="utf-8")
PLATFORM = (ROOT / "platform_darwin.go").read_text(encoding="utf-8")
BUILD = (ROOT.parent / "tools" / "build-release.sh").read_text(encoding="utf-8")
INFO = (ROOT.parent / "packaging" / "Info.plist").read_text(encoding="utf-8")

spec = importlib.util.spec_from_file_location("eng_contract", ENGINE)
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)


def isolate(td):
    eng.CFG = td / "config.json"
    eng.ISSUE_ACKS = td / "acks.json"
    eng.INDEX_CACHE = td / "index-cache.json"
    eng.STATUS_OVERRIDES = td / "overrides.json"
    eng.ROOT_HEALTH = td / "root-health.json"
    eng.OWNERSHIP_DIR = td / "ownership"
    eng.UNDO_DIR = td / "undo"
    eng.AI_CACHE = td / "ai-cache"
    eng.AI_CACHE.mkdir(exist_ok=True)
    eng._INDEX_CACHE_MEM.update(stamp="", items={}, revision=0)
    eng._LIBRARY_CATALOG.update(loaded=False, items={}, cache_mtime=None, cache_stamp=None)


def nfo(root, title, dateadded):
    return f"""<?xml version="1.0" encoding="UTF-8"?>
<{root}>
  <title>{title}</title>
  <year>2024</year>
  <dateadded>{dateadded}</dateadded>
  <uniqueid type="imdb" default="true">tt1234567</uniqueid>
</{root}>
"""


with tempfile.TemporaryDirectory() as raw:
    td = pathlib.Path(raw)
    isolate(td)
    movies, tv = td / "movies", td / "tv"
    movies.mkdir(); tv.mkdir()
    movie = movies / "movie.nfo"
    show = tv / "tvshow.nfo"
    movie.write_text(nfo("movie", "Movie A", "2026-08-20 13:14:15"), encoding="utf-8")
    show.write_text(nfo("tvshow", "Show A", "2025-07-01"), encoding="utf-8")
    eng.save_json(eng.CFG, {
        "library_roots_confirmed": True,
        "library_roots": {"movies": [str(movies)], "tv": [str(tv)]},
        "output_language": "en-US",
        "ai": {"provider": "p", "base_url": "https://example.invalid", "model": "m", "prompt": eng.DEFAULT_AI_PROMPT},
    })

    cfg = eng.ai_config()
    assert cfg["output_language"] == "en-US"
    payload = json.loads(eng._ai_user_payload({"Camera": ["Angénieux Optimo"]}, cfg))
    assert payload["output_language"] == "en-US"
    assert payload["technical_specs"]["Camera"] == ["Angénieux Optimo"]
    assert "仅用于 warnings" in eng.DEFAULT_AI_PROMPT
    assert "绝不改变 tags.value" in eng.DEFAULT_AI_PROMPT
    assert "must never translate or rewrite tags[].value" in eng._effective_ai_prompt({"prompt": "Translate everything"})
    stock_hashes = eng._accepted_ai_prompt_hashes({"prompt": eng.DEFAULT_AI_PROMPT})
    legacy_stock_hash = __import__("hashlib").sha256(eng.DEFAULT_AI_PROMPT_LEGACY_LOCALIZED.strip().encode("utf-8")).hexdigest()[:16]
    assert legacy_stock_hash in stock_hashes
    custom_hashes = eng._accepted_ai_prompt_hashes({"prompt": "custom prompt"})
    assert legacy_stock_hash not in custom_hashes
    zh = dict(cfg, output_language="zh-CN")
    assert eng._ai_cache_key({}, cfg) != eng._ai_cache_key({}, zh)
    assert eng._failure_fingerprint(movie, {}, cfg, "request") != eng._failure_fingerprint(movie, {}, zh, "request")
    localized = eng._validate_ai_result({"tags": [], "warnings": []}, {"Camera": ["Angénieux Optimo"]}, "en-US")
    assert localized["review_reasons"] == [
        "The model returned 0 tags for non-empty Technical Specs.",
        "Camera is missing source_indexes coverage: 0",
    ]
    preserved = eng._validate_ai_result({"tags": [{
        "value": "Angénieux Optimo Lenses", "field": "Camera", "source_indexes": [0],
        "confidence": "medium", "operation": "preserve",
    }], "warnings": []}, {"Camera": ["Angénieux Optimo Lenses"]}, "zh-CN")
    assert preserved["tags"][0]["value"] == "Angénieux Optimo Lenses"
    assert preserved["review_reasons"] == ["Angénieux Optimo Lenses：置信度=medium"]
    print("OK 1: output_language is limited to review text and partitions cache/failure state")

    detail = eng.inspector_detail(str(movie))
    assert detail["year"] == "2024" and detail["added_date"] == "2026-08-20"
    assert eng._summary_from_detail(detail)["added_date"] == "2026-08-20"
    print("OK 2: authoritative root dateadded is exposed at day precision")

    eng._catalog_reconcile(reason="test-full")
    movie.write_text(nfo("movie", "Movie B", "2026-08-21"), encoding="utf-8")
    show.write_text(nfo("tvshow", "Show B", "2025-07-02"), encoding="utf-8")
    eng._catalog_reconcile(reason="test-movie", space="movies")
    indexed = {item["path"]: item for item in eng.library_index()["items"]}
    assert indexed[os.path.realpath(str(movie))]["title"] == "Movie B"
    assert indexed[os.path.realpath(str(show))]["title"] == "Show A", "TV catalog must remain untouched by movie refresh"
    print("OK 3: movie-only reconcile merges results without scanning/replacing TV state")

assert 'const appVersion = "4.0.1"' in MAIN
for required in (
    "刷新当前媒体库", "发行年份", "添加日期", "Spec 状态", "Tag 状态",
    "columnWrench", "data-resize-field", "appAutoStart", 'id="language"',
    "action('reconcile-index',{path:state.space})", "状态</strong>",
    'value="clear-all"', "columnMenuOpen", "TOOL_COLUMN_WIDTH=34",
    "sortPath=sort.direction==='asc'", "appSettingsGrid", "<svg viewBox=\"0 0 15 15\"",
    'data-reorderable="${key!==\'title\'}"', "headerContent", "resizeHandle ${index===cols.length-1?'passive':''}",
    "header.addEventListener('selectstart',event=>event.preventDefault())", "function renderCatalogHeader()",
    "header.style.gridTemplateColumns=listGrid()", ".columnTools:before", "data-compact-view",
):
    assert required in WEB, required
assert "状态（自动计算）" not in WEB
assert "emoji" not in WEB.lower()
assert 'id="clearFilters"' not in WEB
assert "function displayVersion(value){return 'v'+String(value||'4.0.1')}" in WEB
assert "platformRepairAppAutoStartAfterBundleReplacement" in PLATFORM
assert "app bundle upgrade login item repair" in MAIN
assert 'MAC_APP_NAME="IMDb Tech Manager.app"' in BUILD
assert 'packaging/Info.plist' in BUILD and 'tools/test-source.sh' in BUILD
assert '<key>CFBundleIdentifier</key><string>local.imdb-tech-manager</string>' in INFO
header_segment = WEB[WEB.index("function renderCatalogHeader"):WEB.index("function cellValue")]
assert 'class="gridSpacer"' in header_segment and 'class="headerContent"' in header_segment
assert 'data-resize-field="__tools"' not in header_segment and "toolColumnWidth" not in WEB
print("OK 4: list/settings/current-library UI contracts are wired")
