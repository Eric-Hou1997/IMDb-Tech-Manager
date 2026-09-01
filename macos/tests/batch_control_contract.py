#!/usr/bin/env python3
import importlib.util, pathlib, tempfile, shutil
ROOT=pathlib.Path(__file__).resolve().parents[1]
ENGINE=ROOT/'engine'/'mac-engine.py'; FIX=ROOT/'tests/fixtures/casino-royale-legacy-minimal.nfo'
spec=importlib.util.spec_from_file_location('eng_batch210',ENGINE)
eng=importlib.util.module_from_spec(spec); spec.loader.exec_module(eng)
with tempfile.TemporaryDirectory() as td:
    td=pathlib.Path(td)
    a=td/'a.nfo'; b=td/'b.nfo'; shutil.copy(FIX,a); shutil.copy(FIX,b)
    eng.AI_BATCH_STATE=td/'ai-batch-state.json'; eng.AI_BATCH_QUEUE=td/'ai-batch-queue.json'; eng.AI_BATCH_PAUSE=td/'ai-batch-pause.flag'; eng.AI_FAILURE_QUEUE=td/'ai-failure-queue.json'; eng.AI_RUNTIME=td/'ai-runtime.json'; eng.LOCK=td/'run.lock'
    eng._configured_nfo_paths=lambda:[a,b]
    eng._configured_roots_for_access=lambda:[str(td)]
    eng.ai_ready=lambda require_enabled=True,allow_paused=False:(True,'')
    eng.ai_config=lambda:{'legacy_cleanup_mode':'strict','model':'qwen-plus','run_request_limit':0,'run_token_limit':0,'run_cost_limit':0}
    eng.compute_pipeline_status=lambda paths,save=True:{}
    eng._save_ai_status=lambda *a,**k:None
    touched=[]
    def rw_pause(p,old,info,tag_mode,cleanup_mode,dry_run=False):
        touched.append(pathlib.Path(p).name)
        eng.LAST_REWRITE_DETAIL.clear(); eng.LAST_REWRITE_DETAIL.update({'tag_engine':'ai','usage':{},'cost':0,'cache_hit':False})
        if len(touched)==1:
            eng.AI_BATCH_PAUSE.write_text('pause')
        return 'updated'
    eng.rewrite=rw_pause
    assert eng.tag_generate('ai')==0
    st=eng.load_json(eng.AI_BATCH_STATE,{})
    assert st['status']=='paused',st
    assert st['next_index']==1,st
    assert touched==['a.nfo'],touched
    def rw_resume(p,old,info,tag_mode,cleanup_mode,dry_run=False):
        touched.append(pathlib.Path(p).name)
        eng.LAST_REWRITE_DETAIL.clear(); eng.LAST_REWRITE_DETAIL.update({'tag_engine':'ai','usage':{},'cost':0,'cache_hit':False})
        return 'updated'
    eng.rewrite=rw_resume
    assert eng.tag_generate('ai',resume_task=True)==0
    st=eng.load_json(eng.AI_BATCH_STATE,{})
    assert st['status']=='completed',st
    assert st['next_index']==2,st
    assert touched==['a.nfo','b.nfo'],touched

    # Create a persistent malformed-json failure and confirm retry-failed only touches it.
    touched.clear()
    first=True
    def rw_fail(p,old,info,tag_mode,cleanup_mode,dry_run=False):
        touched.append(pathlib.Path(p).name)
        if pathlib.Path(p).name=='a.nfo':
            raise eng.AIRequestError('malformed-json','bad json')
        eng.LAST_REWRITE_DETAIL.clear(); eng.LAST_REWRITE_DETAIL.update({'tag_engine':'ai','usage':{},'cost':0,'cache_hit':False})
        return 'updated'
    eng.rewrite=rw_fail
    assert eng.tag_generate('ai',force_all=True)==0
    fq=eng._load_failure_queue()['items']
    assert len(fq)==1 and pathlib.Path(fq[0]['path']).name=='a.nfo',fq
    touched.clear()
    eng.rewrite=rw_resume
    assert eng.tag_generate('ai',force_all=True)==0
    assert touched==['b.nfo'],touched
    assert eng.load_json(eng.AI_BATCH_STATE,{})['counts']['known_failure_skipped']==1
    touched.clear()
    assert eng.tag_generate('ai',retry_failed=True)==0
    assert touched==['a.nfo'],touched
    assert eng._load_failure_queue()['items']==[],eng._load_failure_queue()
    eng.save_json(eng.AI_BATCH_STATE, {'schema':1,'task_id':'legacy','scope':'selected','next_index':0,'status':'paused'})
    eng.save_json(eng.AI_BATCH_QUEUE, {'schema':1,'task_id':'legacy','scope':'selected','paths':[str(a)],'created_at':'2026-01-01T00:00:00+00:00'})
    migrated_state,migrated_queue=eng._batch_load_resume()
    assert migrated_state['schema']==2 and migrated_state['engine']=='ai',migrated_state
    assert migrated_state['scope']['kind']=='selection',migrated_state
    assert migrated_queue['scope']['resolved_paths']==[str(a)],migrated_queue
print('OK batch control contract: safe pause/resume and persistent retry-failed queue')
