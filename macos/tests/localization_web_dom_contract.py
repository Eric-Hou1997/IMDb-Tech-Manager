#!/usr/bin/env python3
"""Real-DOM contract for the complete zh-CN/en-US Web UI switch."""
from html.parser import HTMLParser
from pathlib import Path
import os
import re
import shutil
import signal
import subprocess
import tempfile


ROOT = Path(__file__).resolve().parents[1]
SOURCE = (ROOT / "web" / "index.html").read_text(encoding="utf-8")


class VisibleText(HTMLParser):
    def __init__(self):
        super().__init__()
        self.skip = 0
        self.user_tags = []
        self.values = []

    def handle_starttag(self, tag, attrs):
        user_attributes = any(key == "data-i18n-user-attributes" for key, _ in attrs)
        if any(key == "data-i18n-user" for key, _ in attrs):
            self.user_tags.append(tag)
        if tag in {"script", "style", "textarea"}:
            self.skip += 1
        elif not self.skip and not self.user_tags and not user_attributes:
            self.values.extend(value for key, value in attrs if key in {"placeholder", "title", "aria-label"})

    def handle_endtag(self, tag):
        if tag in {"script", "style", "textarea"} and self.skip:
            self.skip -= 1
        if self.user_tags and self.user_tags[-1] == tag:
            self.user_tags.pop()

    def handle_data(self, data):
        if not self.skip and not self.user_tags:
            self.values.append(data)


def chrome_binary():
    candidates = [
        os.environ.get("IMDB_TECH_MANAGER_CHROME", "").strip(),
        shutil.which("google-chrome") or "",
        shutil.which("chromium") or "",
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    ]
    return next((Path(item) for item in candidates if item and Path(item).is_file()), None)


for required in (
    "const ENGLISH_UI=Object.freeze({",
    "function setUILanguage(language)",
    "function localizeDOM(",
    "window.__setUILanguage=setUILanguage",
    "localizedNodeState=new WeakMap()",
    "LOCALIZED_ATTRIBUTES=['placeholder','title','aria-label']",
    "function flexContentWidth(",
    "required>available+.5",
):
    assert required in SOURCE, required

chrome = chrome_binary()
if chrome is None:
    if os.environ.get("IMDB_TECH_REQUIRE_BROWSER") == "1":
        raise AssertionError("Chrome/Chromium is required for the localization DOM gate")
    print("SKIP localization DOM: Chrome/Chromium not installed")
    raise SystemExit(0)

with tempfile.TemporaryDirectory(prefix="imdb-tech-i18n-") as temp_dir:
    temp = Path(temp_dir)
    web = temp / "index.html"
    web.write_text(SOURCE, encoding="utf-8")
    dump_path = temp / "dom.html"
    error_path = temp / "chrome.stderr"
    with dump_path.open("w", encoding="utf-8") as dump, error_path.open("w", encoding="utf-8") as errors:
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
                "--window-size=780,720",
                f"--user-data-dir={temp / 'profile'}",
                "--virtual-time-budget=1200",
                "--dump-dom",
                web.as_uri() + "#i18n-smoke",
            ],
            stdout=dump,
            stderr=errors,
            text=True,
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

    dom = dump_path.read_text(encoding="utf-8")
    assert 'lang="en-US"' in dom and 'data-i18n-ready="true"' in dom, error_path.read_text(encoding="utf-8")[-2000:]
    assert 'data-i18n-state="ok"' in dom, "language switch lost selection, filters, modal, or unsaved form state"
    assert 'data-i18n-roundtrip="ok"' in dom, "zh-CN/en-US round trip changed state or failed to restore labels"
    assert 'data-i18n-inspector-statuses="ok"' in dom, "Inspector protocol-derived status values were not fully localized"
    assert 'data-i18n-protocol-registry="ok"' in dom, "one or more Inspector protocol values lack an English presentation label"
    assert 'data-badge-typography="ok"' in dom, "status and ownership badges do not share the same readable font weight"
    assert 'data-i18n-locale="ok"' in dom, "English dates or numbers did not use en-US formatting"
    semantic_detail = re.search(r'data-i18n-semantics-detail="([^"]*)"', dom)
    html_state = re.search(r"<html[^>]*>", dom)
    assert 'data-i18n-semantics="ok"' in dom, "key English labels, menus, language names, or task logs are incorrect: " + (semantic_detail.group(1) if semantic_detail else (html_state.group(0) if html_state else "unknown"))
    assert 'data-localized-toolbar-responsive="ok"' in dom, "Chinese or English toolbar clipped, jumped rows, or wrapped before measured overflow"
    assert 'data-localized-region-responsive="ok"' in dom, "header or action region ignored its measured localized content width"
    assert 'data-future-locale-responsive="ok"' in dom, "future long Latin/CJK labels did not use the content-driven layout path"
    assert 'data-i18n-region-layout="ok"' in dom, "header/status or scope controls disappeared, overlapped, or escaped their container"
    assert 'data-i18n-layout="ok"' in dom, "English text overflowed the minimum window or a modal"
    assert re.search(r'data-i18n-user(?:="")?>电影</', dom), "Chinese user data was translated instead of preserved"
    parser = VisibleText()
    parser.feed(dom)
    residue = []
    for value in parser.values:
        cleaned = value.replace("侯雁泽", "")
        if re.search(r"[\u3400-\u9fff]", cleaned):
            residue.append(value.strip())
    assert not residue, "English DOM retains Chinese UI text: " + repr(list(dict.fromkeys(residue))[:30])

print("OK complete English Web UI DOM, accessibility text, and narrow layout contract")
