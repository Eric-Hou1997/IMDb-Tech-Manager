#!/usr/bin/env python3
import importlib.util, pathlib
ROOT=pathlib.Path(__file__).resolve().parents[1]
ENGINE=ROOT/'engine'/'mac-engine.py'
spec=importlib.util.spec_from_file_location('eng_pru',ENGINE)
eng=importlib.util.module_from_spec(spec); spec.loader.exec_module(eng)
specs={k:[] for k in eng.SECTIONS}
specs['Sound mix']=['SDDS','Auro 11.1','Dolby Atmos (Dolby Atmos+Vision)','DTS (DTS: X)','DTS:X','Dolby Atmos']
specs['Aspect ratio']=['2.39 : 1']
specs['Camera']=[
 'Arri Alexa Mini, Panavision Primo, Retro C-, E-, G- and T-Series Lenses',
 'Arri Alexa XT Plus, Panavision Primo, Retro C-, E-, G- T-Series, ATZ and AWZ2 Lenses',
]
specs['Negative Format']=['Codex ARRIRAW (2.8K, 3.4K)']
specs['Cinematographic Process']=['Digital Intermediate (2K, master format)','Panavision (anamorphic, source format)']
specs['Printed Film Format']=['D-Cinema (also 3-D version)']
obj={'tags':[
 {'value':'SDDS','field':'Sound mix','source_indexes':[0],'confidence':'high','operation':'preserve'},
 {'value':'Auro 11.1','field':'Sound mix','source_indexes':[1],'confidence':'high','operation':'preserve'},
 {'value':'Dolby Atmos','field':'Sound mix','source_indexes':[2,5],'confidence':'high','operation':'normalize'},
 {'value':'DTS:X','field':'Sound mix','source_indexes':[3,4],'confidence':'high','operation':'normalize'},
 {'value':'2.39:1','field':'Aspect ratio','source_indexes':[0],'confidence':'high','operation':'normalize'},
 {'value':'Arri Alexa Mini','field':'Camera','source_indexes':[0],'confidence':'high','operation':'split-camera-lens'},
 {'value':'Panavision Primo Lenses','field':'Camera','source_indexes':[0,1],'confidence':'high','operation':'shared-suffix'},
 {'value':'Panavision Retro C-Series Lenses','field':'Camera','source_indexes':[0,1],'confidence':'high','operation':'series-expansion'},
 {'value':'Panavision Retro E-Series Lenses','field':'Camera','source_indexes':[0,1],'confidence':'high','operation':'series-expansion'},
 {'value':'Panavision Retro G-Series Lenses','field':'Camera','source_indexes':[0,1],'confidence':'high','operation':'series-expansion'},
 {'value':'Panavision Retro T-Series Lenses','field':'Camera','source_indexes':[0,1],'confidence':'high','operation':'series-expansion'},
 {'value':'Arri Alexa XT Plus','field':'Camera','source_indexes':[1],'confidence':'high','operation':'split-camera-lens'},
 {'value':'Panavision ATZ Lenses','field':'Camera','source_indexes':[1],'confidence':'high','operation':'shared-prefix'},
 {'value':'Panavision AWZ2 Lenses','field':'Camera','source_indexes':[1],'confidence':'high','operation':'shared-prefix'},
 {'value':'Codex ARRIRAW (2.8K, 3.4K)','field':'Negative Format','source_indexes':[0],'confidence':'high','operation':'preserve'},
 {'value':'Digital Intermediate (2K, master format)','field':'Cinematographic Process','source_indexes':[0],'confidence':'high','operation':'preserve'},
 {'value':'Panavision (anamorphic, source format)','field':'Cinematographic Process','source_indexes':[1],'confidence':'high','operation':'preserve'},
 {'value':'D-Cinema (also 3-D version)','field':'Printed Film Format','source_indexes':[0],'confidence':'high','operation':'preserve'},
], 'warnings':[]}
r=eng._validate_ai_result(obj,specs)
assert not r['review_reasons'],r
vals=[x['value'] for x in r['tags']]
assert vals.count('Dolby Atmos')==1 and vals.count('DTS:X')==1,vals
for expected in ('Panavision Retro C-Series Lenses','Panavision Retro T-Series Lenses','Panavision ATZ Lenses','Panavision AWZ2 Lenses'):
 assert expected in vals,expected
print('OK Pacific Rim contract: source coverage, sound dedupe, complex Camera target shape')
