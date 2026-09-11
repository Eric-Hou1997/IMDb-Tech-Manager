#!/usr/bin/env python3
"""Execute the original ITM engine against isolated fixtures; no production data."""
import importlib.util
import pathlib
import tempfile
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('rewrite_baseline_engine', ROOT / 'macos/engine/mac-engine.py')
eng = importlib.util.module_from_spec(spec)
spec.loader.exec_module(eng)

class WriterBaseline(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='itm-characterization-')
        self.addCleanup(self.tmp.cleanup)
        self.root = pathlib.Path(self.tmp.name)
        # Every engine Path rooted in the original APP is redirected, including future additions.
        original_app = eng.APP
        for name, value in list(vars(eng).items()):
            if isinstance(value, pathlib.Path):
                try: relative = value.relative_to(original_app)
                except ValueError: continue
                patcher = patch.object(eng, name, self.root / 'state' / relative)
                patcher.start(); self.addCleanup(patcher.stop)
        eng.APP.mkdir()
        self.nfo = self.root / '电影.nfo'
        self.raw = b'\xef\xbb\xbf' + ('<movie>\r\n<title>原片名</title>\r\n<uniqueid type="imdb">tt0064757</uniqueid>\r\n<tag>外部标签</tag>\r\n<unknown a="keep">untouched</unknown>\r\n</movie>\r\n').encode()
        self.nfo.write_bytes(self.raw); self.nfo.chmod(0o640)
        eng.save_json(eng.CFG, {'roots': [str(self.root)]})
        self.obj = {'imdb':'tt0064757','fetched_at':'2026-09-10T00:00:00+00:00','url':'https://www.imdb.com/title/tt0064757/technical/','status':'ok','specs':{k:[] for k in eng.SECTIONS},'ok':True,'ready':True}
        self.obj['specs']['Camera']=['Arri Alexa 65']

    def test_specs_only_preserves_external_xml_bytes_mode_and_backup(self):
        result=eng.rewrite_specs_only(self.nfo,self.obj,eng.inspect_nfo(self.raw.decode('utf-8-sig')))
        self.assertEqual(result,'updated')
        data=self.nfo.read_bytes()
        self.assertTrue(data.startswith(b'\xef\xbb\xbf'))
        self.assertNotIn(b'\n',data.replace(b'\r\n',b''))
        self.assertIn(b'<unknown a="keep">untouched</unknown>',data)
        self.assertEqual([x.text for x in ET.fromstring(data).findall('tag')],['外部标签'])
        self.assertEqual(self.nfo.stat().st_mode & 0o777,0o640)
        self.assertEqual(pathlib.Path(str(self.nfo)+'.imdbtech.bak').read_bytes(),self.raw)

    def test_dry_run_does_not_write_or_change_mtime(self):
        before=self.nfo.stat().st_mtime_ns
        eng.rewrite_specs_only(self.nfo,self.obj,eng.inspect_nfo(self.raw.decode('utf-8-sig')),dry_run=True)
        self.assertEqual(self.nfo.read_bytes(),self.raw)
        self.assertEqual(self.nfo.stat().st_mtime_ns,before)

    def write_edit(self):
        return eng._atomic_inspector_write(self.nfo,self.raw,self.raw.decode('utf-8-sig').replace('原片名','候选片名'),eng._source_hash(self.raw),'characterization')

    def test_replace_permission_failure_keeps_original(self):
        replace=eng.os.replace
        def fail(source,dest):
            if pathlib.Path(dest)==self.nfo: raise PermissionError('injected final replacement failure')
            return replace(source,dest)
        with patch.object(eng.os,'replace',side_effect=fail):
            with self.assertRaises(PermissionError): self.write_edit()
        self.assertEqual(self.nfo.read_bytes(),self.raw)

    def test_fsync_failure_keeps_original(self):
        with patch.object(eng.os,'fsync',side_effect=OSError('injected disk failure')):
            with self.assertRaises(OSError): self.write_edit()
        self.assertEqual(self.nfo.read_bytes(),self.raw)

    def test_external_change_before_commit_is_preserved(self):
        external=self.raw.replace('原片名'.encode(),'外部编辑'.encode())
        replace=eng.os.replace
        def race(source,dest):
            result=replace(source,dest)
            if str(dest).endswith('.imdbtech.bak'): self.nfo.write_bytes(external)
            return result
        with patch.object(eng.os,'replace',side_effect=race):
            with self.assertRaises(eng.EditConflictError): self.write_edit()
        self.assertEqual(self.nfo.read_bytes(),external)

    def test_stale_edit_hash_rejected(self):
        with self.assertRaises(eng.EditConflictError):
            eng.edit_nfo({'path':str(self.nfo),'expected_source_hash':'stale','operation':'edit-tag','target':{'root_index':0},'value':'new'})
        self.assertEqual(self.nfo.read_bytes(),self.raw)

    def test_outside_root_rejected(self):
        with tempfile.TemporaryDirectory() as other:
            outside=pathlib.Path(other)/'outside.nfo';outside.write_bytes(self.raw)
            with self.assertRaises((ValueError,PermissionError)):
                eng.edit_nfo({'path':str(outside),'expected_source_hash':eng._source_hash(self.raw),'operation':'edit-tag','target':{'root_index':0},'value':'new'})
            self.assertEqual(outside.read_bytes(),self.raw)

    def test_resident_auto_prioritizes_recent_then_one_backfill_without_tags(self):
        now=eng.time.time()
        entries={}
        paths={}
        for name,age in [('recent-a',120),('recent-b',150),('old-a',2000),('old-b',3000)]:
            path=self.root/(name+'.nfo');path.write_bytes(self.raw);paths[name]=path
            entries[str(path.resolve())]={'summary':{'nfo_mtime':now-age,'media_type':'movie','spec_status':'missing','imdb':'tt0064757'}}
        with patch.object(eng,'_catalog_ensure'), patch.object(eng,'_LIBRARY_CATALOG',{'items':entries}), patch.object(eng,'get_specs',return_value=self.obj), patch.object(eng,'_manual_task_requested',return_value=False), patch.object(eng.time,'time',return_value=now):
            self.assertEqual(eng.run('auto'),0)
        changed=[name for name,path in paths.items() if path.read_bytes()!=self.raw]
        self.assertEqual(changed,['recent-a','recent-b','old-a'])
        for path in paths.values():
            self.assertEqual([tag.text for tag in ET.fromstring(path.read_bytes()).findall('tag')],['外部标签'])

    def test_failure_queue_requires_explicit_retry_when_input_is_unchanged_or_unfingerprinted(self):
        cfg=eng.ai_config();specs={'Camera':['Camera Model']}
        entry={'path':str(self.nfo),'kind':'schema-invalid','message':'fixture error','fingerprint':eng._failure_fingerprint(self.nfo,specs,cfg,'schema-invalid')}
        eng._failure_upsert(entry)
        self.assertTrue(eng._known_failure_unchanged(self.nfo,specs,cfg))
        self.assertTrue(eng._known_failure_unchanged(self.nfo,specs,dict(cfg,input_price_per_million=99)))
        self.assertFalse(eng._known_failure_unchanged(self.nfo,dict(specs,Runtime=['100 min']),cfg))
        self.assertFalse(eng._known_failure_unchanged(self.nfo,specs,dict(cfg,prompt=cfg['prompt']+'!')))
        entry.pop('fingerprint');eng._failure_upsert(entry)
        self.assertTrue(eng._known_failure_unchanged(self.nfo,specs,dict(cfg,prompt='changed')))
        eng._failure_remove(self.nfo)
        self.assertFalse(eng._known_failure_unchanged(self.nfo,specs,cfg))
        self.assertEqual(self.nfo.read_bytes(),self.raw)

    def test_ai_cache_reuses_exact_request_with_zero_current_and_separate_historical_cost(self):
        cfg=eng.ai_config()
        specs={'Camera':['Camera Model']}
        tags=[{'value':'Camera Model','field':'Camera','source_indexes':[0],'confidence':'medium'}]
        key=eng._ai_cache_key(specs,cfg,[])
        eng.save_json(eng.AI_CACHE/(key+'.json'),{'cache_schema':2,'created_at':'2024-01-02T03:04:05+00:00','model':'resolved-model','prompt_hash':eng.hashlib.sha256(eng._effective_ai_prompt(cfg).encode()).hexdigest()[:16],'spec_hash':eng._specs_hash(specs),'output_language':cfg['output_language'],'usage':{'prompt_tokens':100,'completion_tokens':20,'total_tokens':120},'cost':0.25,'result':{'tags':tags,'warnings':[]}})
        before=self.nfo.stat().st_mtime_ns
        with patch.object(eng,'ai_config',return_value=cfg), patch.object(eng,'ai_ready',return_value=(True,'')), patch.object(eng,'_ai_request_with_retry',return_value={'tags':tags,'warnings':[]}) as network:
            for current in (specs,dict(specs,Runtime=['900 min'])):
                result=eng.ai_generate_tags(current)
                self.assertTrue(result['cache_hit'])
                self.assertEqual(result['usage'],{})
                self.assertEqual(result['cost'],0.0)
                self.assertEqual(result['cached_usage']['total_tokens'],120)
                self.assertEqual(result['cached_cost'],0.25)
                self.assertTrue(result['review_required'])
                self.assertEqual(result['model'],'resolved-model')
            network.assert_not_called()
            eng.ai_generate_tags(specs,force=True)
            network.assert_called_once()
        self.assertEqual(self.nfo.read_bytes(),self.raw)
        self.assertEqual(self.nfo.stat().st_mtime_ns,before)

    def test_parsed_cache_retains_positive_empty_and_failure_expiry(self):
        for age,status,hit in [(29*86400,'ok',True),(31*86400,'ok',False),(6*86400,'no-tech',True),(8*86400,'no-tech',False),(60,'timeout',True),(3601,'timeout',False)]:
            value=dict(self.obj,cache_version=8,parser_version=1,status=status,ok=status=='ok',fetched_at=(eng.dt.datetime.now(eng.dt.timezone.utc)-eng.dt.timedelta(seconds=age)).isoformat())
            eng.save_json(eng.cache_file('tt0064757'),value)
            self.assertEqual(eng._parsed_specs_cache('tt0064757') is not None,hit,(age,status))
            if status=='timeout': self.assertIsNone(eng._parsed_specs_cache('tt0064757',retry_failed=True))

    def test_inspector_undo_journal_restores_exact_bytes_and_is_consumed(self):
        eng.edit_nfo({'path':str(self.nfo),'expected_source_hash':eng._source_hash(self.raw),'operation':'edit-tag','target':{'root_index':0},'value':'修改后的外部标签'})
        after=self.nfo.read_bytes()
        journal=eng.load_json(eng._undo_path(self.nfo),{})
        self.assertEqual(journal['before_hash'],eng._source_hash(self.raw))
        self.assertEqual(journal['after_hash'],eng._source_hash(after))
        self.assertEqual(eng.base64.b64decode(journal['before'],validate=True),self.raw)
        eng.undo_nfo({'path':str(self.nfo),'expected_source_hash':eng._source_hash(after)})
        self.assertEqual(self.nfo.read_bytes(),self.raw)
        self.assertFalse(eng._undo_path(self.nfo).exists())

    def test_inspector_undo_expiry_and_external_change_are_not_overwritten(self):
        self.write_edit()
        after=self.nfo.read_bytes()
        external=after.replace('候选片名'.encode(),'后续修改'.encode())
        self.nfo.write_bytes(external)
        with self.assertRaises(eng.EditConflictError):
            eng.undo_nfo({'path':str(self.nfo),'expected_source_hash':eng._source_hash(external)})
        self.assertEqual(self.nfo.read_bytes(),external)
        self.nfo.write_bytes(after)
        journal=eng.load_json(eng._undo_path(self.nfo),{})
        journal['expires_at']=(eng.dt.datetime.now(eng.dt.timezone.utc)-eng.dt.timedelta(seconds=1)).isoformat()
        eng.save_json(eng._undo_path(self.nfo),journal)
        with self.assertRaises(ValueError):
            eng.undo_nfo({'path':str(self.nfo),'expected_source_hash':eng._source_hash(after)})
        self.assertEqual(self.nfo.read_bytes(),after)

    def test_issue_ack_and_status_follow_original_independent_hash_contract(self):
        detail=eng.inspector_detail(str(self.nfo))
        eng.acknowledge_issue({'path':str(self.nfo),'expected_source_hash':detail['source_hash'],'kind':'spec-missing'})
        eng.set_status_override({'path':str(self.nfo),'value':'ai-complete'})
        current=eng.inspector_detail(str(self.nfo))
        self.assertEqual(current['lifecycle'],'ai-complete')
        self.assertEqual([i['kind'] for i in current['ignored_issues']],['spec-missing'])
        self.assertEqual(self.nfo.read_bytes(),self.raw)
        self.nfo.write_bytes(self.raw.replace('原片名'.encode(),'外部编辑'.encode()))
        current=eng.inspector_detail(str(self.nfo))
        self.assertEqual(current['lifecycle'],'spec-missing')
        self.assertFalse(current['ignored_issues'])
        eng.set_status_override({'path':str(self.nfo),'value':'review'})
        self.nfo.write_bytes(self.raw)
        current=eng.inspector_detail(str(self.nfo))
        self.assertEqual(current['lifecycle'],'spec-missing')
        self.assertEqual([i['kind'] for i in current['ignored_issues']],['spec-missing'])

    def test_original_tag_status_priority(self):
        specs={k:[] for k in eng.SECTIONS};specs['Camera']=['Arri Alexa']
        digest=eng._specs_hash(specs)
        for engine,fingerprint,state,tag,want in [('ai',digest,'current',True,'ai-complete'),('local-rules',digest,'current',True,'local-complete'),('ai','old','review',True,'stale'),('ai',digest,'review',True,'review'),('ai','old','current',False,'tag-missing')]:
            raw='<movie><uniqueid type="imdb">tt1234567</uniqueid>'+('<tag>Camera: Arri Alexa</tag>' if tag else '')+'<technicalspecs source="IMDb" imdbid="tt1234567"><section name="Camera"><item>Arri Alexa</item></section><generatedtags owner="IMDb Tech Manager" schema="2" engine="'+engine+'" specHash="'+fingerprint+'" state="'+state+'"><tag>Camera: Arri Alexa</tag></generatedtags></technicalspecs></movie>'
            self.nfo.write_text(raw,encoding='utf-8')
            self.assertEqual(eng.inspector_detail(str(self.nfo))['lifecycle'],want)

if __name__=='__main__': unittest.main(verbosity=2)
