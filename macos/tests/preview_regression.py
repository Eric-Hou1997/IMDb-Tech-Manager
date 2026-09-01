#!/usr/bin/env python3
import importlib.util, pathlib, tempfile, re, json, os, subprocess, sys
ROOT=pathlib.Path(__file__).resolve().parents[1]
ENGINE=ROOT/'engine'/'mac-engine.py'
FIX=ROOT/'tests'/'fixtures'/'casino-royale-legacy-minimal.nfo'
spec=importlib.util.spec_from_file_location('eng',ENGINE)
eng=importlib.util.module_from_spec(spec); spec.loader.exec_module(eng)
text=FIX.read_text(encoding='utf-8')
obj=eng.existing_tech_object(text)
have=eng._all_normal_tags(text)
legacy=eng.generated_tag_values(obj['specs'], include_legacy=True)
owned=[x for x in legacy if x.casefold() in {t.casefold() for t in have}]
expected=['Panavision Lenses','2.39 : 1','35 mm','Panavision (anamorphic)']
assert owned==expected,(owned,expected)
# Partial AI result must no longer silently pass validation.
partial={"tags":[
 {"value":"70 mm 6-Track","field":"Sound mix","source_indexes":[0],"confidence":"high","operation":"normalize"},
 {"value":"Mono","field":"Sound mix","source_indexes":[1],"confidence":"high","operation":"normalize"},
 {"value":"Dolby Digital","field":"Sound mix","source_indexes":[2],"confidence":"high","operation":"normalize"},
 {"value":"2.39:1","field":"Aspect ratio","source_indexes":[0],"confidence":"high","operation":"normalize"},
],"warnings":[]}
validated=eng._validate_ai_result(partial,obj['specs'])
reasons=';'.join(validated['review_reasons'])
for field in ('Camera','Negative Format','Cinematographic Process','Printed Film Format'):
    assert field in reasons,reasons
# Dry-run migration with a pristine TMM baseline: raw spaced ratio is replaced,
# while identical technical tags are recognized as owned but retained in final output.
with tempfile.TemporaryDirectory() as td:
    nfo=pathlib.Path(td)/'Casino Royale.nfo'; nfo.write_text(text,encoding='utf-8')
    base=re.sub(r'\s*<!-- tmm-imdb-tech:BEGIN -->.*?<!-- tmm-imdb-tech:END -->\s*','\n',text,flags=re.S)
    for tag in expected:
        base=base.replace(f'  <tag>{tag}</tag>\n','')
    pathlib.Path(str(nfo)+'.imdbtech.bak').write_text(base,encoding='utf-8')
    cur=nfo.read_text(encoding='utf-8'); old=eng.existing_tech_object(cur); info=eng.inspect_nfo(cur)
    st=eng.rewrite(nfo,old,info,tag_mode='local',cleanup_mode='strict',dry_run=True)
    d=eng.LAST_REWRITE_DETAIL
    assert st=='preview',st
    assert d['old_owned']==expected,d
    assert d['replaced_owned']==['2.39 : 1'],d
    assert d['removed']==['2.39 : 1'],d
    assert '2.39:1' in d['added'] and '2.39 : 1' not in d['new_tags'],d
print('OK preview regression: 007 legacy ownership, ratio replacement, AI field coverage')
