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

if __name__=='__main__': unittest.main(verbosity=2)
