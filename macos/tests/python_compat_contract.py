from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parents[1]
SRC = (ROOT / 'engine' / 'mac-engine.py').read_text(encoding='utf-8')

checks = {
    'no PEP701 nested same-quote f-string regression': 'f"https://www.imdb.com/title/{obj["imdb"]}' not in SRC,
    'no match/case syntax': re.search(r'^\s*(match|case)\b', SRC, re.M) is None,
    'no PEP604 union annotations': (
        re.search(r'^[ \t]*def[ \t]+[^\n]*->[^\n]*\|', SRC, re.M) is None and
        re.search(r'^[ \t]*[A-Za-z_]\w*[ \t]*:[ \t]*[^=\n#]*\|', SRC, re.M) is None
    ),
    'no builtin generic annotations': re.search(r'(^|[(:,>]\s*)(list|dict|set|tuple)\[[^\n]+\]', SRC, re.M) is None,
}

bad = [name for name, ok in checks.items() if not ok]
for name, ok in checks.items():
    print(('OK   ' if ok else 'FAIL ') + name)
if bad:
    print('Python 3.8-3.11 compatibility contract failed: ' + ', '.join(bad), file=sys.stderr)
    raise SystemExit(1)
