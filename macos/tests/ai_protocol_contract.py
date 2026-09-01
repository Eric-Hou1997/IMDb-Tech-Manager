#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import tempfile
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer

ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / "engine" / "mac-engine.py"
spec = importlib.util.spec_from_file_location("eng_ai_protocol370", ENGINE)
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)


def valid_result(value="DTS:X"):
    return {
        "tags": [{
            "value": value,
            "field": "Sound mix",
            "source_indexes": [0],
            "confidence": "high",
            "operation": "normalize",
        }],
        "warnings": [],
    }


def run_mock(protocol, responses):
    seen = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *args):
            pass

        def do_POST(self):
            size = int(self.headers.get("Content-Length", "0"))
            seen.append({
                "path": self.path,
                "headers": {key.lower(): value for key, value in self.headers.items()},
                "body": json.loads(self.rfile.read(size).decode("utf-8")),
            })
            payload = responses[min(len(seen) - 1, len(responses) - 1)]
            data = json.dumps(payload).encode("utf-8")
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(data)))
            self.end_headers()
            self.wfile.write(data)

    server = HTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    base = "http://127.0.0.1:%d" % server.server_port
    if protocol == "anthropic":
        base += "/apps/anthropic"
    else:
        base += "/compatible-mode/v1"
    return server, base, seen


eng.ai_api_key = lambda: "protocol-test-key"
specs = {"Sound mix": ["DTS (DTS: X)"]}
common = {
    "model": "qwen-plus",
    "prompt": "Only JSON",
    "temperature": 0,
    "top_p": 1,
    "max_tokens": 2000,
    "output_token_cap": 10000,
    "timeout_seconds": 5,
    "json_mode": "on",
    "extra_body": "{}",
    "thinking_mode": "off",
    "prompt_cache_mode": "off",
    "retry_count": 0,
    "input_price_per_million": 0,
    "output_price_per_million": 0,
}

# OpenAI Chat Completions: system prompt is a message; Bearer auth and
# choices[0].message.content are the protocol boundary.
openai_payload = {
    "model": "qwen-plus",
    "choices": [{
        "finish_reason": "stop",
        "message": {"content": json.dumps(valid_result())},
    }],
    "usage": {"prompt_tokens": 11, "completion_tokens": 7, "total_tokens": 18},
}
server, base, seen = run_mock("openai", [openai_payload])
cfg = dict(common, api_protocol="openai", provider="bailian", base_url=base)
try:
    result = eng._ai_http_request(specs, cfg, True, False, existing=["external"])
finally:
    server.shutdown()
assert seen[0]["path"].endswith("/compatible-mode/v1/chat/completions"), seen
assert seen[0]["headers"].get("authorization") == "Bearer protocol-test-key", seen
assert "x-api-key" not in seen[0]["headers"], seen
assert seen[0]["body"]["messages"][0]["role"] == "system", seen
user_payload = json.loads(seen[0]["body"]["messages"][1]["content"])
assert user_payload["existing_tags"] == ["external"], user_payload
assert user_payload["output_language"] == "zh-CN", user_payload
assert result["api_protocol"] == "openai" and result["usage"]["total_tokens"] == 18, result

# Anthropic Messages: system is top-level, auth uses x-api-key, response text
# is collected from content blocks, and cache token counters are not lost.
anthropic_payload = {
    "model": "qwen-plus",
    "stop_reason": "end_turn",
    "content": [{"type": "text", "text": json.dumps(valid_result())}],
    "usage": {
        "input_tokens": 20,
        "output_tokens": 9,
        "cache_creation_input_tokens": 3,
        "cache_read_input_tokens": 4,
    },
}
server, base, seen = run_mock("anthropic", [anthropic_payload])
cfg = dict(common, api_protocol="anthropic", provider="bailian", base_url=base)
try:
    result = eng._ai_http_request(specs, cfg, True, False)
finally:
    server.shutdown()
assert seen[0]["path"].endswith("/apps/anthropic/v1/messages"), seen
assert seen[0]["headers"].get("x-api-key") == "protocol-test-key", seen
assert seen[0]["headers"].get("anthropic-version") == "2023-06-01", seen
assert "authorization" not in seen[0]["headers"], seen
assert seen[0]["body"]["system"].startswith("Only JSON\n\n"), seen
assert "must never translate or rewrite tags[].value" in seen[0]["body"]["system"], seen
assert all(item["role"] != "system" for item in seen[0]["body"]["messages"]), seen
assert seen[0]["body"]["thinking"] == {"type": "disabled"}, seen
assert result["usage"]["prompt_tokens"] == 27, result
assert result["usage"]["completion_tokens"] == 9, result
assert result["usage"]["total_tokens"] == 36, result

# Anthropic max_tokens is the same semantic truncation condition and must
# increase the output limit before retrying.
truncated = dict(anthropic_payload, stop_reason="max_tokens")
server, base, seen = run_mock("anthropic", [truncated, anthropic_payload])
cfg = dict(common, api_protocol="anthropic", provider="bailian", base_url=base)
try:
    result = eng._ai_request_with_retry(specs, cfg, True, False)
finally:
    server.shutdown()
assert len(seen) == 2, seen
assert seen[0]["body"]["max_tokens"] == 2000, seen
assert seen[1]["body"]["max_tokens"] >= 4096, seen
retry_payload = json.loads(seen[1]["body"]["messages"][0]["content"])
assert "上次输出被截断" in retry_payload["recovery_instruction"], retry_payload

# Old configuration migration follows the saved provider/URL. Explicit new
# selection wins, and URL/model/provider remain untouched.
with tempfile.TemporaryDirectory() as temp_dir:
    cfg_path = pathlib.Path(temp_dir) / "config.json"
    old_cfg_path = eng.CFG
    eng.CFG = cfg_path
    try:
        cfg_path.write_text(json.dumps({"ai": {
            "provider": "anthropic-compatible",
            "base_url": "https://dashscope.aliyuncs.com/apps/anthropic",
            "model": "qwen-plus",
        }}), encoding="utf-8")
        migrated = eng.ai_config()
        assert migrated["api_protocol"] == "anthropic", migrated
        assert migrated["base_url"].endswith("/apps/anthropic"), migrated
        assert migrated["model"] == "qwen-plus", migrated

        cfg_path.write_text(json.dumps({"ai": {
            "api_protocol": "openai",
            "provider": "anthropic-compatible",
            "base_url": "https://example.invalid/custom",
            "model": "kept-model",
        }}), encoding="utf-8")
        migrated = eng.ai_config()
        assert migrated["api_protocol"] == "openai", migrated
        assert migrated["base_url"] == "https://example.invalid/custom", migrated
        assert migrated["model"] == "kept-model", migrated
    finally:
        eng.CFG = old_cfg_path

print("OK AI protocol contract: OpenAI/Anthropic wire formats, usage, truncation, migration")
