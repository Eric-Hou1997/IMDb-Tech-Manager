#!/usr/bin/env python3
import importlib.util, pathlib, tempfile
ROOT=pathlib.Path(__file__).resolve().parents[1]
ENGINE=ROOT/'engine'/'mac-engine.py'
spec=importlib.util.spec_from_file_location('eng_cache',ENGINE)
eng=importlib.util.module_from_spec(spec); spec.loader.exec_module(eng)
with tempfile.TemporaryDirectory() as td:
    eng.AI_CACHE=pathlib.Path(td)
    cfg={
      'provider':'bailian','base_url':'https://dashscope.aliyuncs.com/compatible-mode/v1','model':'qwen-plus',
      'prompt':'test prompt','temperature':0,'top_p':1,'max_tokens':1800,'json_mode':'auto','extra_body':'{}',
      'thinking_mode':'off','prompt_cache_mode':'auto','warning_policy':'review','retry_count':0,
      'input_price_per_million':0,'output_price_per_million':0
    }
    eng.ai_config=lambda:cfg
    eng.ai_ready=lambda require_enabled=False,allow_paused=False:(True,'')
    eng._record_ai_success=lambda:None
    calls=[]
    def req(specs,cfg,with_json_mode,with_prompt_cache=False,existing=None):
        calls.append((with_json_mode,with_prompt_cache))
        return {'tags':[{'value':'DTS:X','field':'Sound mix','source_indexes':[0],'confidence':'high','operation':'normalize'}],
                'warnings':[],'review_reasons':[],'usage':{'prompt_tokens':100,'completion_tokens':20,'total_tokens':120},
                'cost':0,'raw_model':'qwen-plus'}
    eng._ai_request_with_retry=req
    specs={'Sound mix':['DTS (DTS: X)']}
    first=eng.ai_generate_tags(specs)
    second=eng.ai_generate_tags(specs)
    assert len(calls)==1,calls
    assert not first['cache_hit'] and second['cache_hit'],(first,second)
    assert second['usage']=={} and second['cost']==0.0,second
print('OK preview/write cache contract: second identical pass makes no model request')
