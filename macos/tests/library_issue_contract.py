#!/usr/bin/env python3
import importlib.util, json, os, tempfile
from pathlib import Path
ENGINE = Path(__file__).resolve().parents[1] / "engine" / "mac-engine.py"
with tempfile.TemporaryDirectory() as td:
    os.environ["HOME"] = td
    spec = importlib.util.spec_from_file_location("engine303", ENGINE)
    m = importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
    movies, tv = Path(td)/"Movies", Path(td)/"TV"
    movies.mkdir(); tv.mkdir()
    m.save_json(m.CFG, {"library_roots":{"movies":[str(movies)],"tv":[str(tv)]},"roots":[str(movies),str(tv)]})
    wrong = movies/"House of Cards.nfo"
    wrong.write_text('<tvshow><title>纸牌屋</title><uniqueid type="imdb">tt1856010</uniqueid></tvshow>', encoding="utf-8")
    good = movies/"Movie.nfo"
    good.write_text('<movie><title>测试电影</title><uniqueid type="imdb">tt0000001</uniqueid></movie>', encoding="utf-8")
    items = {x["path"]:x for x in m.library_index()["items"]}
    assert items[os.path.realpath(str(wrong))]["media_space"] == "movies"
    assert items[os.path.realpath(str(wrong))]["library_type_mismatch"] is True
    before = m.inspector_detail(str(good)); assert before["counts"]["issues"] == 1
    out = m.acknowledge_issue({"path":str(good),"expected_source_hash":before["source_hash"],"kind":"spec-missing"})["item"]
    assert out["counts"]["issues"] == 0 and out["counts"]["ignored_issues"] == 1
    indexed = {x["path"]:x for x in m.library_index()["items"]}[os.path.realpath(str(good))]
    assert indexed["counts"]["issues"] == 0 and indexed["counts"]["ignored_issues"] == 1
    good.write_text('<movie><title>测试电影（改）</title><uniqueid type="imdb">tt0000001</uniqueid></movie>', encoding="utf-8")
    assert m.inspector_detail(str(good))["counts"]["issues"] == 1
print("OK categorized roots and issue acknowledgement contract")
