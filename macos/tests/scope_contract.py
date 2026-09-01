#!/usr/bin/env python3
import importlib.util
import pathlib
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / "engine" / "mac-engine.py"
spec = importlib.util.spec_from_file_location("eng_scope", ENGINE)
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)


def write_nfo(path, root, title, imdb, season="", episode=""):
    extra = ""
    if root == "episodedetails":
        extra = f"<showtitle>示例剧</showtitle><season>{season}</season><episode>{episode}</episode>"
    path.write_text(f"<{root}><title>{title}</title>{extra}<imdbid>{imdb}</imdbid></{root}>", encoding="utf-8")


with tempfile.TemporaryDirectory() as td:
    td = pathlib.Path(td)
    library = td / "媒体"
    library.mkdir()
    movie = library / "movie.nfo"
    series = library / "tvshow.nfo"
    ep1 = library / "s01e01.nfo"
    ep2 = library / "s02e01.nfo"
    season = library / "season.nfo"
    write_nfo(movie, "movie", "电影", "tt1000001")
    write_nfo(series, "tvshow", "示例剧", "tt1000002")
    write_nfo(ep1, "episodedetails", "第一集", "tt1000003", "1", "1")
    write_nfo(ep2, "episodedetails", "第二季第一集", "tt1000004", "2", "1")
    season.write_text("<season><title>Season 01</title></season>", encoding="utf-8")
    eng.CFG = td / "config.json"
    eng.AI_FAILURE_QUEUE = td / "failures.json"
    eng.ISSUE_ACKS = td / "acks.json"
    eng.STATUS_OVERRIDES = td / "overrides.json"
    eng.INDEX_CACHE = td / "index-cache.json"
    eng.ROOT_HEALTH = td / "root-health.json"
    eng.JOB_PROGRESS = td / "job-progress.json"
    eng._LIBRARY_CATALOG.update(loaded=False, items={}, cache_mtime=None)
    eng.save_json(eng.CFG, {"roots": [str(library)]})

    index = eng.library_index()["items"]
    by_name = {pathlib.Path(x["path"]).name: x for x in index}
    assert by_name["movie.nfo"]["media_space"] == "movies"
    assert by_name["tvshow.nfo"]["media_space"] == "tv"
    assert by_name["s01e01.nfo"]["season"] == 1
    assert by_name["season.nfo"]["tag_status"] == "not-applicable"

    current = eng.scope_preflight({"kind": "current", "path": str(movie), "engine": "local-rules"})
    assert current["resolved_paths"] == [str(movie.resolve())]
    selected = eng.scope_preflight({"kind": "selection", "paths": [str(movie), str(ep1)], "engine": "ai"})
    assert selected["counts"]["total"] == 2
    season_scope = eng.scope_preflight({
        "kind": "current-season", "path": str(ep1), "series_key": "示例剧", "season": 1, "engine": "ai",
    })
    assert season_scope["resolved_paths"] == [str(ep1.resolve())]
    all_movies = eng.scope_preflight({"kind": "all-movies", "engine": "local-rules"})
    assert all_movies["resolved_paths"] == [str(movie.resolve())]
    all_tv = eng.scope_preflight({"kind": "all-tv", "engine": "local-rules"})
    assert str(movie.resolve()) not in all_tv["resolved_paths"]
    assert str(season.resolve()) not in all_tv["resolved_paths"]

print("OK Scope contract: current/selection/movie/TV/season snapshots are isolated and explicit")
