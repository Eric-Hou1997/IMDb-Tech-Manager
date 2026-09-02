#!/usr/bin/env python3
"""Regression contract for formal release metadata and settings update checks."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
web = (ROOT / "web" / "index.html").read_text(encoding="utf-8")
main = (ROOT / "main.go").read_text(encoding="utf-8")
info = (ROOT.parent / "packaging" / "Info.plist").read_text(encoding="utf-8")
build = (ROOT.parent / "tools" / "build-release.sh").read_text(encoding="utf-8")
update = (ROOT / "update.go").read_text(encoding="utf-8")

checks = {
    "formal version is synchronized": (
        'const appVersion = "4.0.2"' in main
        and 'content="v4.0.2"' in web
        and "<key>CFBundleVersion</key><string>4.0.2</string>" in info
        and "<key>CFBundleShortVersionString</key><string>4.0.2</string>" in info
    ),
    "about metadata has a real release date": (
        "v4.0.2　2026-09-02 发布　macOS · Apple Silicon" in web
        and "正式发布时写入日期" not in web
    ),
    "every settings open starts an update check": (
        "function openSettingsModal()" in web
        and "checkTechUpdate()" in web
        and "q('#openSettings').onclick=openSettingsModal" in web
    ),
    "update state remains in the about panel": all(
        value in web for value in [
            'id="techUpdateState"',
            "正在检查 GitHub 正式发布…",
            "已是最新版本 ",
            "最新版本 ",
            "检查失败：",
        ]
    ),
    "repeated settings opens cannot overlap checks": (
        "techUpdateCheckInFlight" in web
        and "if(techUpdateCheckInFlight)return" in web
        and "techUpdateCheckInFlight=false" in web
    ),
    "release filenames use the short canonical scheme": all(
        value in build for value in [
            'ARTIFACT_BASE="ITM-v${VERSION}-MacOS-AArch64-APP"',
            'MAC_ZIP_NAME="${ARTIFACT_BASE}.zip"',
            'SIG_NAME="$MAC_ZIP_NAME.sig"',
            'SHA_NAME="${ARTIFACT_BASE}-SHA256SUMS.txt"',
        ]
    ),
    "OTA selects the exact versioned macOS package": (
        'ITM-v%s.%s.%s-MacOS-AArch64-APP.zip' in update
        and "selectTechUpdateAssets" in update
        and "asset.Name == expectedArchive" in update
        and 'asset.Name == expectedArchive+".sig"' in update
    ),
}

failed = [name for name, ok in checks.items() if not ok]
for name, ok in checks.items():
    print(("OK  " if ok else "FAIL ") + name)
if failed:
    raise SystemExit("macOS release metadata/update contract failed: " + ", ".join(failed))
print("OK IMDb Tech Manager v4.0.2 release metadata and settings update contract")
