#!/usr/bin/env python3
"""IMDb cache capacity, integrity and fetch-state regressions."""
import datetime as dt
import fcntl
import gzip
import importlib.util
import os
import pathlib
import tempfile


ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / "engine" / "mac-engine.py"


def load_engine():
    spec = importlib.util.spec_from_file_location("cache_budget_engine", ENGINE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def fresh_time():
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()


eng = load_engine()

with tempfile.TemporaryDirectory(prefix="imdb-cache-budget-") as raw:
    td = pathlib.Path(raw)
    eng.APP = td
    eng.CFG = td / "config.json"
    eng.CACHE = td / "cache"
    eng.CACHE_STATUS = td / "cache-status.json"
    eng.CACHE_MAINTENANCE_LOCK = td / "cache-maintenance.lock"
    eng.CACHE.mkdir()

    # Missing/invalid legacy values migrate safely to the requested 2048 MB
    # default. The UI/API range must not silently clamp corrupt values.
    assert eng.imdb_cache_max_mb() == 2048
    eng.save_json(eng.CFG, {"imdb_cache_max_mb": 63})
    assert eng.imdb_cache_max_mb() == 2048
    eng.save_json(eng.CFG, {"imdb_cache_max_mb": 64})
    assert eng.imdb_cache_max_mb() == 64

    # Two fresh sparse raw pages exceed 64 MB. The oldest raw page is evicted
    # down below the 90% low-water mark while the compact parsed result stays.
    for index, imdb in enumerate(("tt0000001", "tt0000002")):
        meta, body = eng._raw_cache_files(imdb)
        eng.save_json(meta, {"url": "https://www.imdb.com/title/%s/technical/" % imdb, "fetched_at": fresh_time()})
        with body.open("wb") as handle:
            handle.truncate(40 * 1024 * 1024)
        stamp = 1000 + index
        os.utime(meta, (stamp, stamp))
        os.utime(body, (stamp, stamp))

    parsed = eng.cache_file("tt0000003")
    eng.save_json(parsed, {
        "cache_version": eng.CACHE_VERSION, "parser_version": eng.PARSER_VERSION,
        "imdb": "tt0000003", "fetched_at": fresh_time(), "status": "ok", "ok": True,
        "specs": {"Camera": ["cached"]},
    })
    outside = td / "outside.json"
    outside.write_text("do-not-delete", encoding="utf-8")
    symlink = eng.CACHE / "tt9999999.json"
    symlink.symlink_to(outside)
    unknown = eng.CACHE / "manual.json"
    unknown.write_text("not-owned", encoding="utf-8")

    result = eng.maintain_imdb_cache()
    assert result["state"] == "ready", result
    assert result["used_bytes"] <= result["limit_bytes"] * eng.IMDB_CACHE_LOW_WATERMARK, result
    assert not eng._raw_cache_files("tt0000001")[1].exists()
    assert eng._raw_cache_files("tt0000002")[1].exists()
    assert parsed.exists(), "parsed cache must outlive fresh raw pages under pressure"
    assert symlink.is_symlink() and outside.read_text(encoding="utf-8") == "do-not-delete"

    # A live per-title singleflight lock makes clear-cache fail closed for that
    # title instead of unlinking a file underneath an active reader/writer.
    locked_meta, locked_body = eng._raw_cache_files("tt0000004")
    eng.save_json(locked_meta, {"url": "https://example.invalid", "fetched_at": fresh_time()})
    locked_body.write_bytes(b"locked")
    lock_handle = (eng.CACHE / "imdb-tt0000004.lock").open("a+")
    fcntl.flock(lock_handle.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)
    try:
        result = eng.maintain_imdb_cache(clear=True)
        assert result["state"] == "busy", result
        assert locked_body.exists()
    finally:
        fcntl.flock(lock_handle.fileno(), fcntl.LOCK_UN)
        lock_handle.close()
    result = eng.maintain_imdb_cache(clear=True)
    assert result["state"] == "ready" and result["used_bytes"] == 0, result
    assert symlink.is_symlink() and outside.exists()
    assert unknown.read_text(encoding="utf-8") == "not-owned"

    # Raw metadata/body mismatches are rejected before parsing.
    meta, body = eng._raw_cache_files("tt0000005")
    page = b"<html><script id='__NEXT_DATA__'>{}</script></html>"
    body.write_bytes(gzip.compress(page))
    eng.save_json(meta, {"url": "https://example.invalid", "fetched_at": fresh_time(), "body_hash": "0" * 64})
    assert eng._specs_from_raw_cache("tt0000005") is None

    # Typed transient failures retain the existing one-hour cooldown and never
    # become a confirmed no-tech result.
    failure = eng.cache_file("tt0000006")
    eng.save_json(failure, {
        "cache_version": eng.CACHE_VERSION, "parser_version": eng.PARSER_VERSION,
        "imdb": "tt0000006", "fetched_at": fresh_time(), "status": "imdb-waf-challenge",
        "ok": False, "specs": {},
    })
    assert eng._parsed_specs_cache("tt0000006")["status"] == "imdb-waf-challenge"
    assert eng._fetch_failure_status(["direct-waf-202"], "") == "imdb-waf-challenge"
    assert eng._fetch_failure_status(["direct-http-429"], "") == "http-429"
    assert eng._fetch_failure_status(["direct-http-429"], "rate limited", "direct-http-429") == "http-429"
    assert eng._fetch_failure_status(["direct-timeout"], "") == "timeout"
    valid_dom = "<html>IMDb __NEXT_DATA__ technicalSpecifications</html>"
    assert eng._fetch_failure_status(
        ["direct-waf-202", "webkit-dom:desktop"], valid_dom, "webkit-dom:desktop"
    ) == "parse-error"
    assert eng._response_is_waf_challenge(202, {"x-amzn-waf-action": "challenge"}, "plain response")
    assert eng._response_is_waf_challenge(202, {"X-Amzn-Waf-Action": "Challenge"}, "plain response")
    assert not eng._response_is_waf_challenge(202, {}, "plain response")

    # Header-only WAF evidence must bypass every parser and retain a typed
    # short-lived failure even when the challenge body has no known marker.
    calls = {"extract": 0}
    original_fetch_direct = eng.fetch_direct
    original_fetch_webkit = eng.fetch_webkit
    original_chrome = eng.chrome
    original_extract_specs = eng.extract_specs
    eng.fetch_direct = lambda url: ("plain response", "direct-waf-202")
    eng.fetch_webkit = lambda url: ("", "webkit-unavailable")
    eng.chrome = lambda: None

    def reject_extract(source):
        calls["extract"] += 1
        raise AssertionError("WAF response reached Technical Specs parser")

    eng.extract_specs = reject_extract
    try:
        result = eng._get_specs_network("tt0000007", force=True)
    finally:
        eng.fetch_direct = original_fetch_direct
        eng.fetch_webkit = original_fetch_webkit
        eng.chrome = original_chrome
        eng.extract_specs = original_extract_specs
    assert calls["extract"] == 0
    assert result["status"] == "imdb-waf-challenge" and not result["ok"], result

print("OK IMDb cache budget: default, LRU tiers, lock safety, symlink boundary, integrity and typed failures")
