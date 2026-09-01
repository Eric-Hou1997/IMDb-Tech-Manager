#!/usr/bin/env python3
import importlib.util, json, pathlib, threading
from http.server import BaseHTTPRequestHandler, HTTPServer
ROOT=pathlib.Path(__file__).resolve().parents[1]
ENGINE=ROOT/'engine'/'mac-engine.py'
spec=importlib.util.spec_from_file_location('eng209', ENGINE)
eng=importlib.util.module_from_spec(spec); spec.loader.exec_module(eng)
seen={}
class H(BaseHTTPRequestHandler):
    def log_message(self,*a): pass
    def do_POST(self):
        n=int(self.headers.get('Content-Length','0')); raw=self.rfile.read(n)
        seen['body']=json.loads(raw.decode())
        payload={
          'model':'qwen-plus','choices':[{'message':{'content':json.dumps({'tags':[
            {'value':'DTS:X','field':'Sound mix','source_indexes':[0],'confidence':'high','operation':'normalize'}
          ],'warnings':[]})}}],
          'usage':{'prompt_tokens':1300,'completion_tokens':80,'total_tokens':1380,'completion_tokens_details':{'reasoning_tokens':0},'prompt_tokens_details':{'cached_tokens':1100}}
        }
        data=json.dumps(payload).encode(); self.send_response(200); self.send_header('Content-Type','application/json'); self.send_header('Content-Length',str(len(data))); self.end_headers(); self.wfile.write(data)
server=HTTPServer(('127.0.0.1',0),H); threading.Thread(target=server.serve_forever,daemon=True).start()
eng.ai_api_key=lambda:'test-key'
cfg={
 'provider':'openai-compatible','base_url':f'http://127.0.0.1:{server.server_port}/v1','model':'qwen-plus',
 'prompt':'x '*1300,'temperature':0,'top_p':1,'max_tokens':1800,'timeout_seconds':5,'json_mode':'auto','extra_body':'{}',
 'thinking_mode':'off','prompt_cache_mode':'on','input_price_per_million':0,'output_price_per_million':0
}
# Explicit provider marker is necessary for localhost mock while retaining Qwen behavior.
cfg['provider']='bailian'
res=eng._ai_http_request({'Sound mix':['DTS (DTS: X)']},cfg,True,True)
server.shutdown()
b=seen['body']
assert b.get('enable_thinking') is False,b
assert isinstance(b['messages'][0]['content'],list),b
assert b['messages'][0]['content'][0]['cache_control']=={'type':'ephemeral'},b
up=json.loads(b['messages'][1]['content'])
assert set(up)=={'technical_specs','output_language'} and up['output_language']=='zh-CN',up
assert res['reasoning_tokens']==0,res
assert res['usage']['prompt_tokens_details']['cached_tokens']==1100,res
print('OK Qwen token contract: thinking off, explicit prompt cache, compact per-NFO payload')
