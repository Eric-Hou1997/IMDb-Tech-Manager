#!/usr/bin/env python3
import importlib.util, pathlib
ROOT=pathlib.Path(__file__).resolve().parents[1]; ENGINE=ROOT/'engine'/'mac-engine.py'; FIX=ROOT/'tests/fixtures/casino-royale-1967-real.nfo'
spec=importlib.util.spec_from_file_location('eng_real007',ENGINE); eng=importlib.util.module_from_spec(spec); spec.loader.exec_module(eng)
text=FIX.read_text(encoding='utf-8'); obj=eng.existing_tech_object(text)
have={x.casefold():x for x in eng._all_normal_tags(text)}
legacy=eng.generated_tag_values(obj['specs'],include_legacy=True)
owned=[have[x.casefold()] for x in legacy if x.casefold() in have]
assert owned==['Panavision Lenses','2.39 : 1','35 mm','Panavision (anamorphic)'],owned
print('OK real 007 NFO fixture: legacy technical tags recognized exactly')
