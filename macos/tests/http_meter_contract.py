#!/usr/bin/env python3
import importlib.util, json, pathlib, threading
from http.server import BaseHTTPRequestHandler, HTTPServer
ROOT=pathlib.Path(__file__).resolve().parents[1]; ENGINE=ROOT/'engine'/'mac-engine.py'
spec=importlib.util.spec_from_file_location('eng_meter210',ENGINE); eng=importlib.util.module_from_spec(spec); spec.loader.exec_module(eng)
class H(BaseHTTPRequestHandler):
    def log_message(self,*a): pass
    def do_POST(self):
        n=int(self.headers.get('Content-Length','0')); self.rfile.read(n)
        payload={'model':'qwen-plus','choices':[{'message':{'content':'{"tags": [BROKEN]'}}], 'usage':{'prompt_tokens':100,'completion_tokens':20,'total_tokens':120,'completion_tokens_details':{'reasoning_tokens':0}}}
        data=json.dumps(payload).encode(); self.send_response(200); self.send_header('Content-Type','application/json'); self.send_header('Content-Length',str(len(data))); self.end_headers(); self.wfile.write(data)
server=HTTPServer(('127.0.0.1',0),H); threading.Thread(target=server.serve_forever,daemon=True).start()
eng.ai_api_key=lambda:'x'; eng._meter_reset()
cfg={'provider':'bailian','base_url':f'http://127.0.0.1:{server.server_port}/v1','model':'qwen-plus','prompt':'p','temperature':0,'top_p':1,'max_tokens':256,'timeout_seconds':3,'json_mode':'off','extra_body':'{}','thinking_mode':'off','prompt_cache_mode':'off','retry_count':1,'input_price_per_million':0,'output_price_per_million':0}
try:
    eng._ai_request_with_retry({'Sound mix':['DTS']},cfg,False,False)
    raise AssertionError('expected invalid-json')
except eng.AIRequestError as e:
    assert e.kind=='malformed-json',e.kind
finally:
    server.shutdown()
m=eng._meter_snapshot()
assert m['http_attempts']==2,m
assert m['http_2xx']==2,m
assert m['usage']['total_tokens']==240,m
print('OK HTTP meter contract: retries and invalid JSON still count real requests/tokens')
