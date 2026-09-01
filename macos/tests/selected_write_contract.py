#!/usr/bin/env python3
import importlib.util, pathlib, tempfile, shutil
ROOT=pathlib.Path(__file__).resolve().parents[1]
ENGINE=ROOT/'engine'/'mac-engine.py'; FIX=ROOT/'tests/fixtures/casino-royale-legacy-minimal.nfo'
spec=importlib.util.spec_from_file_location('eng_sel',ENGINE)
eng=importlib.util.module_from_spec(spec); spec.loader.exec_module(eng)
with tempfile.TemporaryDirectory() as td:
    td=pathlib.Path(td)
    a=td/'a.nfo'; b=td/'b.nfo'; shutil.copy(FIX,a); shutil.copy(FIX,b)
    eng.AI_BATCH_STATE=td/'ai-batch-state.json'; eng.AI_BATCH_QUEUE=td/'ai-batch-queue.json'; eng.AI_BATCH_PAUSE=td/'ai-batch-pause.flag'; eng.AI_FAILURE_QUEUE=td/'ai-failure-queue.json'; eng.AI_RUNTIME=td/'ai-runtime.json'; eng.LOCK=td/'run.lock'
    eng._configured_nfo_paths=lambda:[a,b]
    eng._configured_roots_for_access=lambda:[str(td)]
    eng.ai_ready=lambda require_enabled=True:(True,'')
    eng.ai_config=lambda:{'legacy_cleanup_mode':'strict','model':'qwen-plus','run_request_limit':0,'run_token_limit':0,'run_cost_limit':0}
    touched=[]
    def rw(p,old,info,tag_mode,cleanup_mode,dry_run=False):
        touched.append(pathlib.Path(p).name)
        eng.LAST_REWRITE_DETAIL.clear(); eng.LAST_REWRITE_DETAIL.update({'tag_engine':'ai','usage':{},'cost':0})
        return 'updated'
    eng.rewrite=rw
    eng.compute_pipeline_status=lambda paths,save=True:{}
    eng._save_ai_status=lambda *a,**k:None
    rc=eng.tag_generate('ai',selected_paths=[str(b)])
    assert rc==0,rc
    assert touched==['b.nfo'],touched
print('OK selected write contract: explicit NFO selection does not touch unselected files')
