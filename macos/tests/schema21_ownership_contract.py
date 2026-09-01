#!/usr/bin/env python3
import importlib.util
import pathlib
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / "engine" / "mac-engine.py"
spec = importlib.util.spec_from_file_location("eng_schema21", ENGINE)
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)


def specs(sound):
    result = {key: [] for key in eng.SECTIONS}
    result["Sound mix"] = [sound]
    return result


legacy = """<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<movie>
  <title>旧版</title>
  <imdbid>tt1234567</imdbid>
  <technicalspecs source="IMDb" imdbid="tt1234567" mediatype="movie" fetched="2026-01-01T00:00:00+00:00" formatVersion="20" specHash="old" status="ok">
    <section name="Sound mix"><item>Dolby Atmos</item></section>
    <url>https://www.imdb.com/title/tt1234567/technical/</url>
  </technicalspecs>
</movie>
"""
legacy_obj = eng.existing_tech_object(legacy)
assert legacy_obj["source_specs"] == legacy_obj["specs"]
assert legacy_obj["modified"] is False

render_obj = {
    "imdb": "tt1234567",
    "fetched_at": "2026-08-19T00:00:00+00:00",
    "url": "https://www.imdb.com/title/tt1234567/technical/",
    "status": "ok",
    "specs": specs("手工保留音轨"),
    "source_specs": specs("Dolby Atmos"),
    "source_fetched_at": "2026-08-18T00:00:00+00:00",
    "modified": True,
    "modified_at": "2026-08-19T00:00:00+00:00",
    "manual_entries": [{
        "id": "manual-1", "value": "手工标签", "origin": "manual-add",
        "field": "Sound mix", "created": "2026-08-19T00:00:00+00:00",
        "modified": "2026-08-19T00:00:00+00:00",
    }],
}
block = eng.technical_block(render_obj, "\n", "movie", "测试电影", tag_result={
    "entries": [{"value": "生成标签", "field": "Sound mix", "source_indexes": [0]}],
    "engine": "local-rules", "spec_hash": eng._specs_hash(render_obj["specs"]),
    "write_manifest": True,
})
parsed = eng.existing_tech_object("<movie>\n" + block + "</movie>\n")
assert parsed["format_version"] == 21
assert parsed["modified"] is True
assert parsed["specs"]["Sound mix"] == ["手工保留音轨"]
assert parsed["source_specs"]["Sound mix"] == ["Dolby Atmos"]
assert parsed["manual_entries"][0]["value"] == "手工标签"
assert parsed["owned_entries"][0]["value"] == "生成标签"
assert parsed["owned_entries"][0]["id"]

with tempfile.TemporaryDirectory() as td:
    td = pathlib.Path(td)
    eng.OWNERSHIP_DIR = td / "ownership"
    nfo = td / "movie.nfo"
    old_obj = {
        "imdb": "tt1234567", "fetched_at": "2026-08-18T00:00:00+00:00",
        "url": "https://www.imdb.com/title/tt1234567/technical/", "status": "ok",
        "specs": specs("Dolby Atmos"), "source_specs": specs("Dolby Atmos"),
        "source_fetched_at": "2026-08-18T00:00:00+00:00", "modified": True,
        "modified_at": "2026-08-19T00:00:00+00:00",
        "manual_entries": [{"id": "manual-keep", "value": "手工保留", "origin": "manual-add"}],
    }
    old_block = eng.technical_block(old_obj, "\n", "movie", "测试电影", tag_result={
        "entries": [{"value": "旧生成标签", "field": "Sound mix", "source_indexes": [0]}],
        "engine": "local-rules", "spec_hash": eng._specs_hash(old_obj["specs"]),
        "write_manifest": True,
    })
    nfo.write_text(
        "<movie>\n  <title>测试电影</title>\n  <imdbid>tt1234567</imdbid>\n"
        "  <tag>外部同名标签</tag>\n  <tag>手工保留</tag>\n  <tag>旧生成标签</tag>\n"
        "  <tag>用户自己的标签</tag>\n" + old_block + "</movie>\n",
        encoding="utf-8",
    )
    new_obj = dict(old_obj)
    eng.resolve_tag_entries = lambda obj, mode="local", existing=None: {
        "entries": [
            {"value": "外部同名标签", "field": "Sound mix", "source_indexes": [0]},
            {"value": "手工保留", "field": "Sound mix", "source_indexes": [0]},
            {"value": "新生成标签", "field": "Sound mix", "source_indexes": [0]},
        ],
        "engine": "local-rules", "model": "test", "prompt_hash": "",
        "spec_hash": eng._specs_hash(new_obj["specs"]), "state": "current",
        "write_manifest": True, "review_required": False, "warnings": [],
    }
    info = {"media_type": "movie", "display_title": "测试电影", "root_tag": "movie"}
    assert eng.rewrite(nfo, new_obj, info, tag_mode="local", cleanup_mode="strict") == "updated"
    rewritten = nfo.read_text(encoding="utf-8")
    values = eng._all_normal_tags(rewritten)
    assert values == ["外部同名标签", "手工保留", "用户自己的标签", "新生成标签"], values
    result_obj = eng.existing_tech_object(rewritten)
    assert [x["value"] for x in result_obj["owned_entries"]] == ["新生成标签"]
    assert [x["value"] for x in result_obj["manual_entries"]] == ["手工保留"]

    refresh_obj = dict(new_obj)
    refresh_obj["specs"] = specs("IMDb 新抓取值")
    refresh_obj["source_specs"] = refresh_obj["specs"]
    refresh_obj["fetched_at"] = "2026-08-20T00:00:00+00:00"
    assert eng.rewrite_specs_only(nfo, refresh_obj, info) == "updated"
    refreshed_text = nfo.read_text(encoding="utf-8")
    refreshed = eng.existing_tech_object(refreshed_text)
    assert refreshed["specs"]["Sound mix"] == ["Dolby Atmos"]
    assert refreshed["source_specs"]["Sound mix"] == ["IMDb 新抓取值"]
    assert refreshed["modified"] is True
    assert eng._all_normal_tags(refreshed_text) == values

print("OK schema 21 ownership contract: source snapshot, manual edits, and external tags stay independent")
