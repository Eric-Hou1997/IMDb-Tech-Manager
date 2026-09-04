#!/usr/bin/env python3
"""Cache settings/API/UI boundary contract for v4.0.4."""
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
WEB = (ROOT / "web" / "index.html").read_text(encoding="utf-8")
MAIN = (ROOT / "main.go").read_text(encoding="utf-8")
DARWIN = (ROOT / "platform_darwin.go").read_text(encoding="utf-8")
ENGINE = (ROOT / "engine" / "mac-engine.py").read_text(encoding="utf-8")

for required in (
    "IMDb 缓存",
    'id="imdbCacheLimit" min="64" max="65536" step="1" inputmode="numeric" value="2048"',
    'id="saveIMDbCache"',
    'id="clearIMDbCache"',
    '<div class="rootGroup cacheSettings">',
    '<div class="rootGroupHead"><h3>IMDb 缓存</h3>',
    '.cacheControls{white-space:nowrap}',
    '.rootGroupHead .cacheControls .input{width:120px;min-width:120px;flex:0 0 120px}',
    "renderIMDbCacheStatus(status.imdb_cache)",
    "JSON.stringify({imdb_cache_max_mb:value})",
    "action('cache-clear')",
):
    assert required in WEB, required
assert "<strong>IMDb 抓取缓存</strong>" not in WEB
assert "仅包含 IMDb 原始页面和解析结果；不包含 AI 缓存、索引、浏览器会话、日志和 NFO。" not in WEB

for required in (
    'IMDbCache       IMDbCacheStatus        `json:"imdb_cache"`',
    'raw["imdb_cache_max_mb"]',
    'platformRunEngine("cache-maintain", "", io.Discard)',
    '"cache-maintain", "cache-clear"',
):
    assert required in MAIN, required

for required in (
    "func platformIMDbCacheMaxMB() int",
    "func platformSetIMDbCacheMaxMB(value int) error",
    "func platformIMDbCacheStatus() IMDbCacheStatus",
    'status.State = "requested"',
):
    assert required in DARWIN, required

for required in (
    "IMDB_CACHE_DEFAULT_MAX_MB = 2048",
    "IMDB_CACHE_LOW_WATERMARK = 0.90",
    "def maintain_imdb_cache(clear=False, best_effort=False):",
    "def _response_is_waf_challenge(status, headers, src):",
    'p.add_argument("--cache-clear", action="store_true")',
):
    assert required in ENGINE, required

print("OK cache UI/API: bounded input, explicit scope, status evidence and maintenance actions are wired")
