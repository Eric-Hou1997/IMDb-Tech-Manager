#!/usr/bin/env python3
"""UI, protocol and native macOS packaging source contract."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WEB = (ROOT / "web" / "index.html").read_text(encoding="utf-8")
MAIN = (ROOT / "main.go").read_text(encoding="utf-8")
PLATFORM = (ROOT / "platform_darwin.go").read_text(encoding="utf-8")
ENGINE = (ROOT / "engine" / "mac-engine.py").read_text(encoding="utf-8")
LAUNCHER = (ROOT / "native" / "IMDbTechManagerLauncher.m").read_text(encoding="utf-8")
FETCHER = (ROOT / "native" / "IMDbWebKitFetcher.m").read_text(encoding="utf-8")
BUILD = (ROOT.parent / "tools" / "build-release.sh").read_text(encoding="utf-8")

assert 'const appVersion = "4.0.1"' in MAIN and "v4.0.1" in WEB
assert 'id="aiProtocol"' in WEB and "OpenAI Chat Completions" in WEB and "Anthropic Messages" in WEB
assert 'id="aiProvider"' not in WEB and "api_protocol:q('#aiProtocol').value" in WEB
assert 'APIProtocol' in MAIN and '`json:"api_protocol"`' in MAIN
assert 'cfg.APIProtocol != "openai" && cfg.APIProtocol != "anthropic"' in MAIN
assert "inferAIProtocol" in PLATFORM and 'saved["api_protocol"]' in PLATFORM
assert "_build_openai_request" in ENGINE and "_build_anthropic_request" in ENGINE
assert 'headers["x-api-key"]' not in ENGINE  # literals are built in isolated branches
assert '"x-api-key": ai_api_key()' in ENGINE and '"Authorization": "Bearer " + ai_api_key()' in ENGINE

assert "全部标签" in WEB and "根节点全部标签" not in WEB
assert "手动添加 Tag" in WEB and "添加 Manual Tech Tag" not in WEB
assert ".ownBadge .caret{display:inline-block;font-size:15px" in WEB
assert "return identity+status+issues" in WEB
assert "return values+effective" in WEB
assert "treeLabelRow(name,programOnly?'':`${episodes.length} 集`" in WEB
assert WEB.index('class="rootGroupHead"><h3>电影文件夹') < WEB.index('id="movieRoots"')
assert WEB.index('class="rootGroupHead"><h3>电视剧文件夹') < WEB.index('id="tvRoots"')

assert "WKWebView" in LAUNCHER and "NSWindowTitleHidden" in LAUNCHER
assert "WKWebsiteDataStore.defaultDataStore" in FETCHER
assert "IMDB_TECH_WEBKIT_HELPER" in MAIN and "system-webkit" in ENGINE
assert "IMDbTechManagerLauncher.m" in BUILD and "IMDbWebKitFetcher.m" in BUILD
assert "ditto -c -k" in BUILD and "bare .app found" in BUILD

print("OK shared UI, dual AI protocols, native window, system WebKit, ZIP-only build source")
