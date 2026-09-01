#!/usr/bin/env python3
import importlib.util
import pathlib
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / 'engine' / 'mac-engine.py'
spec = importlib.util.spec_from_file_location('eng_failure300', ENGINE)
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)

specs = {'Sound mix': ['DTS (DTS: X)']}
cfg = {
    'provider': 'bailian', 'base_url': 'https://example.invalid/v1', 'model': 'qwen-plus',
    'prompt': 'prompt-a', 'temperature': 0, 'top_p': 1, 'max_tokens': 1800,
    'output_token_cap': 8192, 'json_mode': 'on', 'extra_body': '{}',
    'thinking_mode': 'off', 'prompt_cache_mode': 'off',
}

with tempfile.TemporaryDirectory() as td:
    td = pathlib.Path(td)
    eng.AI_FAILURE_QUEUE = td / 'ai-failure-queue.json'
    nfo = td / 'movie.nfo'
    nfo.write_text('<movie/>', encoding='utf-8')
    fingerprint = eng._failure_fingerprint(nfo, specs, cfg, 'malformed-json')
    eng._failure_upsert({'path': str(nfo), 'kind': 'malformed-json', 'fingerprint': fingerprint, 'retryable': True})
    assert eng._known_failure_unchanged(nfo, specs, cfg)

    changed = dict(cfg)
    changed['prompt'] = 'prompt-b'
    assert not eng._known_failure_unchanged(nfo, specs, changed)

    changed = dict(cfg)
    changed['max_tokens'] = 4096
    assert not eng._known_failure_unchanged(nfo, specs, changed)

with tempfile.TemporaryDirectory() as td:
    app = pathlib.Path(td)
    eng.APP = app
    eng.CFG = app / 'config.json'
    eng.AI_RUNTIME = app / 'ai-runtime.json'
    eng.save_json(eng.CFG, {'ai': {
        'enabled': True, 'provider': 'openai-compatible', 'base_url': 'https://example.invalid/v1',
        'model': 'test-model', 'prompt': '只返回 JSON',
    }})
    eng._save_ai_runtime({'paused': True, 'reason_kind': 'quota', 'reason': 'AI HTTP 403: insufficient_quota'})
    eng.ai_api_key = lambda: 'test-key'
    ok, message = eng.ai_ready(require_enabled=True)
    assert not ok and '恢复 AI' in message and '保存并测试' in message, message
    calls = []
    def fake_generate(specs, force=False, ignore_pause=False):
        calls.append((force, ignore_pause))
        return {'tags': [], 'warnings': [], 'usage': {}}
    eng.ai_generate_tags = fake_generate
    assert eng.ai_recover() == 0
    assert calls == [(True, True)], calls
    assert not eng._load_ai_runtime()['paused'], eng._load_ai_runtime()

print('OK failure fingerprint contract: unchanged failures skip; prompt/params changes retry; quota recovery tests live provider before unpausing')
