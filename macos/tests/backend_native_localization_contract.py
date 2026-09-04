#!/usr/bin/env python3
"""Phase 3 backend, Engine, native UI, history, and packaging contracts."""
import importlib.util
import pathlib
import tempfile


ROOT = pathlib.Path(__file__).resolve().parents[1]
REPO = ROOT.parent
ENGINE = ROOT / "engine" / "mac-engine.py"
WEB = (ROOT / "web" / "index.html").read_text(encoding="utf-8")
NATIVE = (ROOT / "native" / "IMDbTechManagerLauncher.m").read_text(encoding="utf-8")
BUILD = (REPO / "tools" / "build-release.sh").read_text(encoding="utf-8")
MAIN = (ROOT / "main.go").read_text(encoding="utf-8")
PLATFORM = (ROOT / "platform_darwin.go").read_text(encoding="utf-8")
RESIDENT = (ROOT / "resident_darwin.go").read_text(encoding="utf-8")

spec = importlib.util.spec_from_file_location("phase3_localization_engine", ENGINE)
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)

with tempfile.TemporaryDirectory() as raw:
    td = pathlib.Path(raw)
    movies = td / "movies"
    movies.mkdir()
    nfo = movies / "电影.nfo"
    nfo.write_text("""<?xml version="1.0" encoding="UTF-8"?>
<movie><title>电影</title><year>2026</year></movie>
""", encoding="utf-8")
    eng.CFG = td / "config.json"
    eng.ISSUE_ACKS = td / "acks.json"
    eng.STATUS_OVERRIDES = td / "overrides.json"
    eng.ROOT_HEALTH = td / "health.json"
    eng.OWNERSHIP_DIR = td / "ownership"
    eng.UNDO_DIR = td / "undo"
    eng.AI_FAILURES = td / "failures.json"
    eng.save_json(eng.CFG, {
        "output_language": "zh-CN",
        "library_roots_confirmed": True,
        "library_roots": {"movies": [str(movies)], "tv": []},
        "ai": {"prompt": eng.DEFAULT_AI_PROMPT},
    })
    eng._set_runtime_output_language("en-US")
    assert eng.ai_config()["output_language"] == "en-US", "task language must override mutable config"
    detail = eng.inspector_detail(str(nfo))
    assert detail["title"] == "电影", "user data must not be translated"
    assert {issue["kind"] for issue in detail["issues"]} == {"missing-imdb", "spec-missing"}
    assert all(not any("\u3400" <= char <= "\u9fff" for char in issue["message"]) for issue in detail["issues"])
    sample = eng._localized_runtime_text("✅ [1/2] AI Tag 已写入：电影")
    assert "Tags written" in sample and "电影" in sample, sample
    original = {"Camera": ["电影 Camera"], "Color": ["Color"]}
    validated = eng._validate_ai_result({"tags": [], "warnings": []}, original, "en-US")
    assert original == {"Camera": ["电影 Camera"], "Color": ["Color"]}
    assert validated["review_reasons"] and all("\u3400" > char or char > "\u9fff" for reason in validated["review_reasons"] for char in reason)

for required in (
    'name:@"language"', "__imdbNativeSetLanguage", "IMDBLanguageDefaultsKey",
    '@"About IMDb Tech Manager"', '@"Quit IMDb Tech Manager"',
    '@"Confirm"', '@"Cancel"', '@"OK"',
):
    assert required in NATIVE, required
assert "window.__imdbNativeSetLanguage(uiLanguage)" in WEB
assert '"--output-language", normalizedLanguage(language)' in PLATFORM
assert "performActionWithWriterLanguage(action, arg, st.Language, f)" in MAIN
assert 'req["language"] = normalizedLanguage(language)' in RESIDENT
assert 'data-fixed-language="true"' in WEB
assert "job.language||'zh-CN'" in WEB, "legacy task history must be explicitly treated as Chinese"
assert "PRIVACY.en.md" in WEB and "TERMS.en.md" in WEB

for relative in ("packaging/zh-Hans.lproj/InfoPlist.strings", "packaging/en.lproj/InfoPlist.strings"):
    text = (REPO / relative).read_text(encoding="utf-8")
    for key in ("NSDocumentsFolderUsageDescription", "NSNetworkVolumesUsageDescription", "NSRemovableVolumesUsageDescription"):
        assert key in text, (relative, key)
assert 'cp -R "$ROOT/packaging/zh-Hans.lproj" "$ROOT/packaging/en.lproj"' in BUILD
assert (REPO / "PRIVACY.en.md").is_file() and (REPO / "TERMS.en.md").is_file()
assert "--- English ---" in (REPO / "packaging/README.txt").read_text(encoding="utf-8")
assert "--- English ---" in (REPO / "packaging/CHANGELOG.txt").read_text(encoding="utf-8")

print("OK Phase 3 task-language isolation, Engine/native localization, history preservation, and localized release inputs")
