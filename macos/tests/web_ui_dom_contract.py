#!/usr/bin/env python3
"""Execute the current Mac UI in a real browser DOM and verify startup interactions."""
from pathlib import Path
import base64
import json
import os
import re
import signal
import shutil
import subprocess
import sys
import tempfile


ROOT = Path(__file__).resolve().parents[1]
WEB = ROOT / "web" / "index.html"
SOURCE = WEB.read_text(encoding="utf-8")


def chrome_binary():
    configured = os.environ.get("IMDB_TECH_MANAGER_CHROME", "").strip()
    candidates = [
        configured,
        shutil.which("google-chrome") or "",
        shutil.which("chromium") or "",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    ]
    return next((Path(item) for item in candidates if item and Path(item).is_file()), None)


scripts = re.findall(r"<script(?: [^>]*)?>([\s\S]*?)</script>", SOURCE)
assert len(scripts) == 1, f"expected one consolidated script, found {len(scripts)}"
for retired_id in ("scopeKind", "testAI", "rescanAll", "selNone"):
    assert f'id="{retired_id}"' not in SOURCE, retired_id
assert "automaticScope" in SOURCE and "kind:'selection'" in SOURCE and "kind:'current'" in SOURCE

with tempfile.TemporaryDirectory(prefix="imdb-tech-ui-") as temp_dir:
    temp = Path(temp_dir)
    script_file = temp / "app.js"
    script_file.write_text(scripts[0], encoding="utf-8")
    syntax = subprocess.run(
        ["node", "--check", str(script_file)],
        text=True,
        capture_output=True,
        timeout=20,
    )
    assert syntax.returncode == 0, syntax.stderr

    chrome = chrome_binary()
    if chrome is None:
        if os.environ.get("IMDB_TECH_REQUIRE_BROWSER") == "1":
            raise AssertionError("Chrome/Chromium is required for the release DOM gate")
        print("SKIP real DOM: Chrome/Chromium not installed; static and Node checks passed")
        raise SystemExit(0)

    profile = temp / "chrome-profile"
    layout = {
        "schema_version": 1,
        "revision": 73,
        "split_ratio": 44.25,
        "task_center": {"open": True, "cli_height_px": 315},
        "catalog": {
            "movies": {
                "sort": {"field": "added_date", "direction": "desc"},
                "columns": {
                    "visible": ["title", "year", "added_date", "spec_status", "tag_status"],
                    "order": ["title", "tag_status", "year", "added_date", "spec_status"],
                    "widths": {"title": 410, "year": 91, "added_date": 99, "spec_status": 92, "tag_status": 90},
                    "widthMode": "pixels-current",
                    "compact": False,
                },
            },
            "tv": {
                "sort": {"field": "year", "direction": "asc"},
                "columns": {
                    "visible": ["title", "year", "added_date", "spec_status", "tag_status"],
                    "order": ["title", "spec_status", "year", "added_date", "tag_status"],
                    "widths": {},
                    "widthMode": "adaptive-current",
                    "compact": False,
                },
            },
        },
    }
    encoded_layout = base64.urlsafe_b64encode(
        json.dumps(layout, separators=(",", ":")).encode("utf-8")
    ).decode("ascii").rstrip("=")
    test_web = temp / "index.html"
    test_web.write_text(
        SOURCE.replace("__IMDB_UI_LAYOUT_STATE__", encoded_layout, 1),
        encoding="utf-8",
    )
    url = test_web.as_uri() + "#ui-smoke"
    dump_file = temp / "dom.html"
    stderr_file = temp / "chrome.stderr"
    viewport_width = max(780, min(2400, int(os.environ.get("IMDB_TECH_VIEWPORT_WIDTH", "1440"))))
    with dump_file.open("w", encoding="utf-8") as dump, stderr_file.open("w", encoding="utf-8") as browser_errors:
        process = subprocess.Popen(
            [
            str(chrome),
            "--headless=new",
            "--disable-gpu",
            "--disable-background-networking",
            "--disable-component-update",
            "--disable-default-apps",
            "--no-first-run",
            # The temporary test profile has no user Keychain identity. Keep
            # this test entirely local so Chrome never asks macOS to repair or
            # create a "Chrome" keychain item on the developer's desktop.
            "--password-store=basic",
            "--use-mock-keychain",
            "--allow-file-access-from-files",
            f"--window-size={viewport_width},900",
            f"--user-data-dir={profile}",
            "--virtual-time-budget=1200",
            "--dump-dom",
            url,
            ],
            text=True,
            stdout=dump,
            stderr=browser_errors,
            start_new_session=True,
        )
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            # Chrome's updater may keep the headless parent alive after the DOM
            # has already been dumped. Reap the isolated process group so this
            # contract never leaks a browser into the user's session.
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=3)
    dom = dump_file.read_text(encoding="utf-8")
    assert dom, f"Chrome produced no DOM output: {stderr_file.read_text(encoding='utf-8')[-2000:]}"
    expected = {
        'data-ui-ready="true"': "UI initialization completed",
        'data-default-generator="ok"': "AI generation is the default workflow",
        'data-filter-control="ok"': "status filter and selection controls render in the list toolbar",
        'data-rules-preview="ok"': "rules generation exposes the shared try-write action",
        'data-workflow-row="ok"': "direct actions and generation workflow stay on one row",
        'data-task-toggle="ok"': "task center click toggles both layout states",
        'data-movie-type-hidden="true"': "movie page hides redundant level filter",
        'data-tv-type-visible="true"': "TV page shows populated level filter",
        'data-scope-selector-absent="true"': "legacy scope selector is absent",
        'data-selection-scope="ok"': "checked rows become the automatic scope",
        'data-current-scope="ok"': "focused NFO is the fallback automatic scope",
        'data-task-controls="ok"': "AI task controls stay hidden at idle and appear for running/failed states",
        'data-task-layout="ok"': "task log and action buttons do not overlap",
        'data-compact-default="ok"': "compact view defaults to off",
        'data-catalog-alignment="ok"': "catalog headers align with row cells",
        'data-splitter-hit="ok"': "catalog splitter has a mature drag target",
        'data-compact-layout="ok"': "compact rows are single-line and denser",
        'data-checkbox-metrics="ok"': "settings checkboxes and labels share metrics",
        'data-preview-semantics="ok"': "preview distinguishes generated and existing tags",
        'data-preview-footer="ok"': "preview footer note and action align",
        'data-header-geometry="ok"': "sort arrows follow labels and the gear stays on its fixed rail",
        'data-header-fit="ok"': "default headers remain complete and centered when space is available",
        'data-inspector-alignment="ok"': "preview navigation aligns with the catalog header",
        'data-responsive-containment="ok"': "narrow panes keep the filter and gear inside the catalog",
        'data-split-bounds="ok"': "both splitter extremes keep both panes and controls contained",
        'data-active-tab-indicator="ok"': "active preview tabs use the short yellow indicator",
        'data-responsive-edges="ok"': "header and rows share responsive outer spacing",
        'data-narrow-toolbar="ok"': "narrow movie and TV toolbars keep the requested controls on compact rows",
        'data-tv-toolbar-threshold="ok"': "TV switches to the wrapped toolbar only after its second-row slack is exhausted",
        'data-tv-toolbar-path-stable="ok"': "TV toolbar layout is identical after a movie-to-TV round trip at the same width",
        'data-auto-mode-startup-preference="ok"': "application startup auto mode is presented independently from login startup",
        'data-movie-selection-summary="ok"': "movie selection summary stays right-aligned at every catalog width",
        'data-toolbar-height="ok"': "movie and TV catalog headers keep the same vertical start",
        'data-catalog-control-parity="ok"': "movie and TV share the status-filter width at the same two-row catalog geometry",
        'data-text-selection-policy="ok"': "controls and structural headings cannot be selected while copyable information remains selectable",
        'data-layout-restore="ok"': "persisted split, task center, and per-space table preferences restore before first paint",
        "静态预览 · 未连接本地服务": "file preview reports missing backend",
    }
    missing = [label for marker, label in expected.items() if marker not in dom]
    root_tag = re.search(r"<html[^>]*>", dom)
    assert not missing, f"{missing}; root={root_tag.group(0) if root_tag else 'missing'}"

    corrupt_web = temp / "corrupt-layout.html"
    corrupt_web.write_text(
        SOURCE.replace("__IMDB_UI_LAYOUT_STATE__", "%%%not-base64%%%", 1),
        encoding="utf-8",
    )
    corrupt_dump = temp / "corrupt-layout-dom.html"
    corrupt_errors = temp / "corrupt-layout.stderr"
    with corrupt_dump.open("w", encoding="utf-8") as dump, corrupt_errors.open("w", encoding="utf-8") as browser_errors:
        process = subprocess.Popen(
            [
                str(chrome),
                "--headless=new",
                "--disable-gpu",
                "--disable-background-networking",
                "--disable-component-update",
                "--disable-default-apps",
                "--no-first-run",
                "--password-store=basic",
                "--use-mock-keychain",
                "--allow-file-access-from-files",
                f"--window-size={viewport_width},900",
                f"--user-data-dir={temp / 'corrupt-chrome-profile'}",
                "--virtual-time-budget=500",
                "--dump-dom",
                corrupt_web.as_uri() + "#layout-corrupt-smoke",
            ],
            text=True,
            stdout=dump,
            stderr=browser_errors,
            start_new_session=True,
        )
        try:
            process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait(timeout=3)
    fallback_dom = corrupt_dump.read_text(encoding="utf-8")
    assert 'data-ui-ready="true"' in fallback_dom, corrupt_errors.read_text(encoding="utf-8")[-2000:]
    assert 'data-layout-fallback="ok"' in fallback_dom, "corrupt persisted layout did not fall back safely"

print("OK v4.0.0 real DOM startup, persisted layout, fallback, alignment, and interaction contract")
