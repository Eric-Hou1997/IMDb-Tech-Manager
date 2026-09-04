#!/usr/bin/env python3
"""Guard every current-version surface used by the 4.1.0 source candidate."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
VERSION = "4.1.0"

files = {
    "core": ROOT / "macos" / "main.go",
    "web": ROOT / "macos" / "web" / "index.html",
    "engine": ROOT / "macos" / "engine" / "mac-engine.py",
    "launcher": ROOT / "macos" / "native" / "IMDbTechManagerLauncher.m",
    "fetcher": ROOT / "macos" / "native" / "IMDbWebKitFetcher.m",
    "plist": ROOT / "packaging" / "Info.plist",
    "build": ROOT / "tools" / "build-release.sh",
    "source_test": ROOT / "tools" / "test-source.sh",
    "readme": ROOT / "packaging" / "README.txt",
    "changelog": ROOT / "packaging" / "CHANGELOG.txt",
}
text = {name: path.read_text(encoding="utf-8") for name, path in files.items()}

checks = {
    "Core version": f'const appVersion = "{VERSION}"' in text["core"],
    "Web metadata and fallback": (
        f'content="v{VERSION}"' in text["web"]
        and f"String(value||'{VERSION}')" in text["web"]
    ),
    "Engine user agent": text["engine"].count(f"IMDb-Tech-Manager/{VERSION}") == 2,
    "native user agents": (
        f"IMDbTechManager/{VERSION}" in text["launcher"]
        and f"IMDbTechManagerFetcher/{VERSION}" in text["fetcher"]
    ),
    "bundle versions": all(
        marker in text["plist"]
        for marker in (
            f"<key>CFBundleVersion</key><string>{VERSION}</string>",
            f"<key>CFBundleShortVersionString</key><string>{VERSION}</string>",
            f"IMDb Tech Manager {VERSION}",
        )
    ),
    "build recipe": f'VERSION="{VERSION}"' in text["build"],
    "source gate": f"v{VERSION} source" in text["source_test"],
    "release instructions": (
        f"v{VERSION}" in text["readme"]
        and f"ITM-v{VERSION}-MacOS-AArch64-APP.zip" in text["readme"]
    ),
    "bilingual release notes finalized": (
        text["changelog"].startswith(f"IMDb Tech Manager macOS v{VERSION}\n")
        and "（未发布）" not in text["changelog"].split("--- Published", 1)[0]
        and "(Unreleased)" not in text["changelog"].split("--- Published", 1)[0]
    ),
}

failed = [name for name, ok in checks.items() if not ok]
for name, ok in checks.items():
    print(("OK   " if ok else "FAIL ") + name)
if failed:
    raise SystemExit("4.1.0 closeout contract failed: " + ", ".join(failed))
print("OK IMDb Tech Manager v4.1.0 current-version surfaces are synchronized")
