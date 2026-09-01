#!/usr/bin/env python3
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
main = (ROOT / "macos/main.go").read_text(encoding="utf-8")
platform = (ROOT / "macos/platform_darwin.go").read_text(encoding="utf-8")
engine = (ROOT / "macos/engine/mac-engine.py").read_text(encoding="utf-8")
web = (ROOT / "macos/web/index.html").read_text(encoding="utf-8")

checks = {
    "manager version 4.0.1": 'const appVersion = "4.0.1"' in main,
    "web version 4.0.1": "v4.0.1" in web,
    "onboarding API": '"/api/onboarding"' in main and "handleOnboarding" in main,
    "persistent task history API": '"/api/task-history"' in main and "jobHistoryPath" in main,
    "confirmed library gate": "library_roots_confirmed" in platform and "library_roots_confirmed" in engine,
    "fresh install does not auto discover": 'platformRunEngine("discover-roots"' not in platform,
    "discovery is read only": "--discover-root-candidates" in platform and "discover_root_candidates" in engine,
    "network access is explicit": "test-library-root" in main and "platformTestLibraryRoot" in platform,
    "onboarding UI separates spaces": "onboardingModal" in web and "onboardingMovies" in web and "onboardingTV" in web,
    "browser-local task history removed": "localStorage.taskHistory" not in web,
    "unsaved roots survive status polling": "rootsDirty" in web and "if(!state.rootsDirty)state.rootsBySpace" in web,
}

failed = [name for name, ok in checks.items() if not ok]
if failed:
    raise SystemExit("FAILED: " + ", ".join(failed))
print("OK macOS onboarding, root safety, and task history contract")
