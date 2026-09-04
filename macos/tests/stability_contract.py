from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
main = (ROOT / 'main.go').read_text()
web = (ROOT / 'web/index.html').read_text()
platform = (ROOT / 'platform_darwin.go').read_text()

checks = {
    'app version 4.1.0': 'const appVersion = "4.1.0"' in main,
    'web UI version 4.1.0': '4.1.0' in web,
    'Darwin embed excludes Windows assets': 'engine/windows-engine.ps1' not in main and 'engine/technical-specs-card.js' not in main,
    'app-start auto mode preference is independent': 'AutoModeOnAppStartConfigured' in main and 'auto_mode_on_app_start' in web,
    'legacy agent login item is retired safely': 'migrateAutoModeOnAppStartPreference' in main and 'platformSetAutoStart(false, io.Discard)' in main,
    'agent process has no login-item branch': 'platformAutoStartEnabled()' not in platform[platform.index('func platformStartAgentProcessOnly'):platform.index('func platformStopAgentProcessOnly')],
    'application login item remains independent': 'platformSetAppAutoStart' in platform,
    'autostart polling race guarded': 'autoStartDirty' in web,
    'official Tech UI mark': 'assets/ITM_logo_letter_only.png' in main and 'class="logo"' in web and 'object-fit:contain' in web,
    'preview candidate endpoint': '/api/preview-candidates' in main,
    'scope preview action': 'ai-preview-selected' in main and 'previewScope' in web,
    'paired AI/rules try-write actions': all(action in main and action in web for action in ('ai-preview-write-selected', 'ai-approve-selected', 'local-preview-write-selected', 'local-approve-selected')),
    'paired scoped write actions': 'ai-generate-selected' in main and 'local-generate-selected' in main and 'runGeneration' in web and '--ai-generate-paths-json' in platform and '--local-generate-paths-json' in platform,
    'single auto mode control': 'toggleAgent' in web and 'data-action="start"' not in web and 'data-action="stop"' not in web,
    'qwen thinking control': 'ThinkingMode' in main and 'aiThinking' in web,
    'qwen prompt cache control': 'PromptCacheMode' in main and 'aiPromptCache' in web,
    'Movie/TV searchable NFO manager': 'data-space="movies"' in web and 'data-space="tv"' in web and 'id="search"' in web and 'id="filter"' in web,
    'Inspector API and tabs': '/api/inspector/edit' in main and 'data-tab="overview"' in web and 'data-tab="specs"' in web and 'data-tab="tags"' in web and '/api/inspector/issues' in web,
    'automatic safe scope preflight': '/api/scope/preflight' in main and 'automaticScope' in web and 'kind:\'selection\'' in web and 'kind:\'current\'' in web and 'id="scopeKind"' not in web,
    'AI batch pause action': 'ai-task-pause' in main and 'pauseTask' in web,
    'AI batch resume action': 'ai-resume-task' in main and '--ai-resume-task' in platform and 'resumeTask' in web,
    'AI retry failure action': 'ai-retry-failed' in main and '--ai-retry-failed' in platform and 'retryFailed' in web,
    'AI quota recovery action': 'ai-recover' in main and '--ai-recover' in platform and 'recoverAI' in web,
    'quota pause gives recovery guidance': '请先点击“恢复 AI”' in (ROOT / 'engine/mac-engine.py').read_text(),
    'AI batch state surfaced': 'ai_batch_state' in platform and 'ai_failure_queue' in platform,
    'real HTTP meter': 'http_attempts' in (ROOT / 'engine/mac-engine.py').read_text() and '_meter_attempt()' in (ROOT / 'engine/mac-engine.py').read_text(),
}
failed = [name for name, ok in checks.items() if not ok]
for name, ok in checks.items():
    print(('OK  ' if ok else 'FAIL') + name)
if failed:
    raise SystemExit('failed: ' + ', '.join(failed))
