#!/usr/bin/env python3
"""Upgrade-safe localization foundation and legacy presentation contracts."""
import importlib.util
import pathlib


ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / "engine" / "mac-engine.py"
WEB = (ROOT / "web" / "index.html").read_text(encoding="utf-8")
MAIN = (ROOT / "main.go").read_text(encoding="utf-8")
LOCALIZATION = (ROOT / "localization.go").read_text(encoding="utf-8")

spec = importlib.util.spec_from_file_location("localization_upgrade_engine", ENGINE)
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)

assert eng.normalized_output_language("zh-CN") == "zh-CN"
assert eng.normalized_output_language("en-US") == "en-US"
assert eng.normalized_output_language("fr-FR") == "zh-CN"
assert set(eng.LANGUAGE_OPTIONS) == {"zh-CN", "en-US"}

legacy = {
    "path": "/tmp/legacy.nfo",
    "source_hash": "abc",
    "xml_valid": True,
    "media_type": "movie",
    "spec_status": "ready",
    "tag_status": "stale",
    "issues": [{"kind": "stale", "message": "技术标签尚未与当前规格同步"}],
    "counts": {"issues": 1},
}
overlay = eng._apply_summary_overlay(legacy, {
    "config": {}, "accepted_prompt_hashes": [], "failures": {},
    "acknowledgements": {}, "overrides": {},
})
assert overlay["issues"][0]["message_code"] == "issue.stale"
assert overlay["issues"][0]["message"] == "技术标签尚未与当前规格同步"
assert "message_code" not in legacy["issues"][0], "legacy cache object must not be rewritten in memory"

for required in (
    "defaultLanguage", "languageOptions", "supportedLanguage", "migrateLanguagePreference",
    "saveLanguagePreference", "platformReadOutputLanguage", "LanguageSyncStatus",
):
    assert required in LOCALIZATION + MAIN, required

for required in (
    "DEFAULT_UI_LANGUAGE", "UI_LOCALES", "normalizeUILanguage", "setUILanguage",
    "uiText", "librarySpace", "renderLanguageOptions",
):
    assert required in WEB, required

assert 'Space         string `json:"space,omitempty"`' in MAIN
assert 'MessageCode          string `json:"message_code,omitempty"`' in MAIN
assert 'Language             string `json:"language,omitempty"`' in MAIN
assert 'LanguagePackRevision int    `json:"language_pack_revision,omitempty"`' in MAIN
assert 'LanguageCatalogHash  string `json:"language_catalog_hash,omitempty"`' in MAIN
assert "root?.space" in WEB and "librarySpace(x)==='movies'" in WEB
assert "x.kind==='电影'" not in WEB and "x.kind==='电视剧'" not in WEB
assert "state:'pending'" in WEB and "state:'待扫描'" not in WEB
assert '{Code: "zh-CN", NativeName: "简体中文", EnglishName: "Simplified Chinese", ReviewLanguage: "zh-CN", Flag: "cn"' in LOCALIZATION
assert '{Code: "zh-Hant", NativeName: "繁體中文", EnglishName: "Traditional Chinese", ReviewLanguage: "zh-CN", Flag: "cn"' in LOCALIZATION
assert '{Code: "en-US", NativeName: "English (United States)", EnglishName: "English (United States)", ReviewLanguage: "en-US", Flag: "us"' in LOCALIZATION
assert "const LANGUAGE_NAMES={" in WEB and "native=option.code===uiLanguage?'':option.native_name" in WEB
assert "function languageFlag(code)" in WEB and "const star='<path" in WEB
assert "function languageActionIcon(option)" in WEB and "M12 3v12" in WEB
assert "🇨🇳" not in WEB and "🇺🇸" not in WEB

print("OK upgrade-safe language registry, legacy issue adapter, and stable library spaces")
