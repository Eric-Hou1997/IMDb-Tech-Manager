#!/usr/bin/env python3
import importlib.util
import json
import pathlib
import threading
from http.server import BaseHTTPRequestHandler, HTTPServer

ROOT = pathlib.Path(__file__).resolve().parents[1]
ENGINE = ROOT / 'engine' / 'mac-engine.py'
spec = importlib.util.spec_from_file_location('eng_ai_recovery300', ENGINE)
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)

requests = []


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *args):
        pass

    def do_POST(self):
        size = int(self.headers.get('Content-Length', '0'))
        body = json.loads(self.rfile.read(size).decode('utf-8'))
        requests.append(body)
        if len(requests) == 1:
            content = '{"tags":[{"value":"DTS:X"'
            finish_reason = 'length'
            usage = {'prompt_tokens': 90, 'completion_tokens': 1800, 'total_tokens': 1890}
        else:
            content = json.dumps({'tags': [{
                'value': 'DTS:X', 'field': 'Sound mix', 'source_indexes': [0],
                'confidence': 'high', 'operation': 'normalize',
            }], 'warnings': []})
            finish_reason = 'stop'
            usage = {'prompt_tokens': 95, 'completion_tokens': 40, 'total_tokens': 135}
        payload = {
            'model': 'qwen-plus',
            'choices': [{'finish_reason': finish_reason, 'message': {'content': content}}],
            'usage': usage,
        }
        data = json.dumps(payload).encode('utf-8')
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        self.end_headers()
        self.wfile.write(data)


server = HTTPServer(('127.0.0.1', 0), Handler)
threading.Thread(target=server.serve_forever, daemon=True).start()
eng.ai_api_key = lambda: 'test-key'
eng._meter_reset()
cfg = {
    'provider': 'bailian', 'base_url': 'http://127.0.0.1:%d/v1' % server.server_port,
    'model': 'qwen-plus', 'prompt': '只返回 JSON', 'temperature': 0, 'top_p': 1,
    'max_tokens': 1800, 'output_token_cap': 8192, 'timeout_seconds': 5,
    'json_mode': 'on', 'extra_body': '{}', 'thinking_mode': 'off',
    'prompt_cache_mode': 'off', 'retry_count': 0,
    'input_price_per_million': 0, 'output_price_per_million': 0,
}
try:
    result = eng._ai_request_with_retry({'Sound mix': ['DTS (DTS: X)']}, cfg, True, False)
finally:
    server.shutdown()

assert result['tags'][0]['value'] == 'DTS:X', result
assert len(requests) == 2, requests
assert requests[0]['max_tokens'] == 1800, requests
assert requests[1]['max_tokens'] == 8192, requests
meter = eng._meter_snapshot()
assert meter['http_attempts'] == 2, meter
assert meter['http_2xx'] == 2, meter
assert meter['usage']['total_tokens'] == 2025, meter
print('OK AI recovery contract: finish_reason=length retries once at the configured output cap and meters both requests')
