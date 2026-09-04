#!/usr/bin/env python3
"""WKWebView form metrics and current packaging contract."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
WEB = (ROOT / "web" / "index.html").read_text(encoding="utf-8")
MAIN = (ROOT / "main.go").read_text(encoding="utf-8")
INFO = (ROOT.parent / "packaging" / "Info.plist").read_text(encoding="utf-8")
BUILD = (ROOT.parent / "tools" / "build-release.sh").read_text(encoding="utf-8")

assert 'const appVersion = "4.1.0"' in MAIN
assert 'content="v4.1.0"' in WEB and "v4.1.0" in WEB
assert "<key>CFBundleVersion</key><string>4.1.0</string>" in INFO
assert "tools/test-source.sh" in BUILD and "packaging/Info.plist" in BUILD
assert "ditto -c -k" in BUILD and "bare .app found" in BUILD

# WebKit's native select appearance ignores the shared input padding. The
# custom appearance, explicit metric, and single SVG chevron are all required.
assert ".select{-webkit-appearance:none;appearance:none;min-height:34px" in WEB
assert "background-image:url(\"data:image/svg+xml" in WEB
assert "background-position:right 10px center" in WEB
assert "#version{font-weight:400;font-size:13px" in WEB
assert "API 格式 <span class=\"fieldInlineHelp\">（按服务商提供的 URL 格式选择）</span>" in WEB
assert "按服务商提供的 URL 格式选择；标签生成与写入流程相同。" not in WEB
assert '<label>推理模式</label><select class="select" id="aiThinking"><option value="auto">模型默认</option><option value="off">关闭</option><option value="on">开启</option>' in WEB
assert "推理模式（Qwen 默认关闭）" not in WEB
assert ".rootPrimaryActions{grid-column:2;grid-row:1/3;justify-self:end" in WEB
assert ".autoModeHelp{grid-column:1;grid-row:2;" in WEB
print("OK WKWebView UI and packaging contract")
