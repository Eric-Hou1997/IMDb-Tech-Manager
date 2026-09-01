#!/usr/bin/env python3
import importlib.util
import pathlib
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / "engine" / "mac-engine.py"
spec = importlib.util.spec_from_file_location("eng_inspector", ENGINE)
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)


def make_specs(camera="Arri Alexa 65"):
    result = {key: [] for key in eng.SECTIONS}
    result["Camera"] = [camera]
    return result


with tempfile.TemporaryDirectory() as td:
    td = pathlib.Path(td)
    library = td / "资料库"
    library.mkdir()
    nfo = library / "movie.nfo"
    outside = td / "outside.nfo"
    eng.CFG = td / "config.json"
    eng.OWNERSHIP_DIR = td / "ownership"
    eng.UNDO_DIR = td / "undo"
    eng.save_json(eng.CFG, {"roots": [str(library)]})

    obj = {
        "imdb": "tt1234567", "fetched_at": "2026-08-19T00:00:00+00:00",
        "url": "https://www.imdb.com/title/tt1234567/technical/", "status": "ok",
        "specs": make_specs(), "source_specs": make_specs(),
        "source_fetched_at": "2026-08-19T00:00:00+00:00", "modified": False,
        "manual_entries": [],
    }
    block = eng.technical_block(obj, "\r\n", "movie", "测试电影", tag_result={
        "entries": [{"value": "Arri Alexa 65", "field": "Camera", "source_indexes": [0]}],
        "engine": "local-rules", "model": "test", "spec_hash": eng._specs_hash(obj["specs"]),
        "write_manifest": True,
    })
    original = (
        '<?xml version="1.0" encoding="UTF-8" standalone="yes"?>\r\n<movie>\r\n'
        '  <title>测试电影</title>\r\n  <year>2026</year>\r\n  <imdbid>tt1234567</imdbid>\r\n'
        '  <tag>剧情</tag>\r\n  <tag>Arri Alexa 65</tag>\r\n' + block + '</movie>\r\n'
    )
    nfo.write_bytes(b"\xef\xbb\xbf" + original.encode("utf-8"))
    outside.write_text("<movie><title>越界</title></movie>", encoding="utf-8")

    detail = eng.inspector_detail(str(nfo))
    assert detail["xml_valid"] is True
    assert detail["bom"] is True and detail["newline"] == "CRLF"
    assert [x["ownership"] for x in detail["tags"]] == ["external", "generated"]
    generated = detail["tags"][1]

    edited = eng.edit_nfo({
        "path": str(nfo), "expected_source_hash": detail["source_hash"],
        "operation": "edit-tag", "target": {"root_index": generated["root_index"]},
        "value": "ARRI Alexa 65（人工）",
    })
    assert edited["ok"] is True
    detail = edited["item"]
    assert detail["bom"] is True and detail["newline"] == "CRLF"
    assert [x["ownership"] for x in detail["tags"]] == ["external", "manual"]
    assert detail["tags"][1]["value"] == "ARRI Alexa 65（人工）"
    assert detail["technical_specs"]["modified"] is False

    edited = eng.edit_nfo({
        "path": str(nfo), "expected_source_hash": detail["source_hash"],
        "operation": "edit-spec", "target": {"section": "Camera", "index": 0},
        "value": "Sony Venice 2",
    })
    detail = edited["item"]
    assert detail["technical_specs"]["modified"] is True
    assert detail["technical_specs"]["effective"]["Camera"] == ["Sony Venice 2"]
    assert detail["technical_specs"]["source"]["Camera"] == ["Arri Alexa 65"]
    assert detail["tag_status"] == "stale"

    stale_hash = detail["source_hash"]
    nfo.write_bytes(nfo.read_bytes() + b"\r\n")
    try:
        eng.edit_nfo({
            "path": str(nfo), "expected_source_hash": stale_hash,
            "operation": "add-manual-tag", "value": "Cooke S8/i Lenses",
        })
        raise AssertionError("并发修改后编辑必须失败")
    except eng.EditConflictError:
        pass
    nfo.write_bytes(nfo.read_bytes()[:-2])

    before_add = eng.inspector_detail(str(nfo))
    added = eng.edit_nfo({
        "path": str(nfo), "expected_source_hash": before_add["source_hash"],
        "operation": "add-manual-tag", "value": "Cooke S8/i Lenses",
    })
    assert added["item"]["counts"]["manual"] == 2, added["item"]
    undone = eng.undo_nfo({"path": str(nfo), "expected_source_hash": added["item"]["source_hash"]})
    assert undone["ok"] is True
    assert "Cooke S8/i Lenses" not in [x["value"] for x in undone["item"]["tags"]]

    try:
        eng.inspector_detail(str(outside))
        raise AssertionError("资料库外路径必须拒绝")
    except eng.PathOutsideLibraryError:
        pass

print("OK Inspector edit contract: allowlist, CAS, provenance, manual Specs, BOM/CRLF and Undo")
