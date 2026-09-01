#!/usr/bin/env python3
"""legacy system-WebKit fetch routing and native-host source contracts."""
import importlib.util
import json
import pathlib
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / "engine" / "mac-engine.py"
MAIN = ROOT / "main.go"
LAUNCHER = ROOT / "native" / "IMDbTechManagerLauncher.m"
FETCHER = ROOT / "native" / "IMDbWebKitFetcher.m"

spec = importlib.util.spec_from_file_location("eng_webkit370", ENGINE)
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)


def imdb_html():
    title = {
        "runtimes": {"edges": []},
        "technicalSpecifications": {
            "soundMixes": {"items": [{"text": "Dolby Atmos", "attributes": []}]},
            "colorations": {"items": []},
            "aspectRatios": {"items": []},
            "cameras": {"items": []},
            "laboratories": {"items": []},
            "negativeFormats": {"items": []},
            "processes": {"items": []},
            "printedFormats": {"items": []},
            "filmLengths": {"items": []},
        },
    }
    payload = {"props": {"pageProps": {"aboveTheFoldData": title}}}
    return '<html><script id="__NEXT_DATA__" type="application/json">%s</script></html>' % json.dumps(payload)


with tempfile.TemporaryDirectory(prefix="imdb-tech-webkit-contract-") as raw:
    temp = pathlib.Path(raw)
    old = (eng.APP, eng.CACHE)
    eng.APP = temp
    eng.CACHE = temp / "cache"
    eng.CACHE.mkdir()
    eng.fetch_direct = lambda _url: ("awsWafCookieDomainList", "direct-waf-202")
    eng.fetch_webkit = lambda _url: (imdb_html(), "webkit-dom")
    eng.chrome = lambda: None
    eng.fetch_chrome = lambda _url: (_ for _ in ()).throw(AssertionError("Chrome is not required"))
    result = eng._get_specs_network("tt0000001", force=True)
    assert result["ok"] and result["specs"]["Sound mix"] == ["Dolby Atmos"], result
    assert result["renderer"] == "system-webkit", result
    assert result["attempts"] == ["direct-waf-202", "webkit-dom:desktop"], result
    eng.APP, eng.CACHE = old

main = MAIN.read_text(encoding="utf-8")
launcher = LAUNCHER.read_text(encoding="utf-8")
fetcher = FETCHER.read_text(encoding="utf-8")
assert 'case "--native-ui"' in main and "IMDB_TECH_MANAGER_UI_URL=" in main
assert "WKWebView" in launcher and '@"--native-ui"' in launcher
assert "NSWindowTitleHidden" in launcher and 'window.title = @""' in launcher
assert "WKWebsiteDataStore.defaultDataStore" in fetcher
assert '@"www.imdb.com"' in fetcher and '@"m.imdb.com"' in fetcher
assert "IMDB_TECH_WEBKIT_HELPER" in main and "system-webkit" in ENGINE.read_text(encoding="utf-8")

print("OK WebKit fallback contract: native host, persistent system renderer, no Chrome dependency")
