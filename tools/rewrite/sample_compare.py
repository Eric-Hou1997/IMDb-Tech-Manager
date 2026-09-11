#!/usr/bin/env python3
"""Compare an explicit library_acceptance JSON report with unchanged ITM pure functions.
No HTTP, credentials or media writes; the Rust characterize example must be built.
"""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]

def main():
    if len(sys.argv) != 2:
        raise SystemExit('usage: sample_compare.py READ_REPORT_JSON')
    spec = importlib.util.spec_from_file_location('legacy', ROOT / 'macos/engine/mac-engine.py')
    legacy = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(legacy)
    report = json.loads(Path(sys.argv[1]).read_text())
    differences = []
    count = tags_count = 0
    for space in report['spaces']:
        for item in space['items']:
            count += 1
            raw = Path(item['path']).read_bytes().decode('utf-8-sig')
            identity = legacy.inspect_nfo(raw)
            obj = legacy.existing_tech_object(raw)
            specs = (obj or {}).get('specs', {})
            expected = {'title': identity['title'], 'year': identity['year'], 'imdb': legacy.imdb_id(raw),
                        'specs': {k: v for k, v in specs.items() if v},
                        'tags': [(t['value'], t['ownership']) for t in legacy._tag_rows(raw, obj)]}
            actual = {k: item[k] for k in ['title', 'year', 'imdb', 'specs']}
            actual['tags'] = [(t['value'], t['ownership']) for t in item['tags']]
            expected['rules'] = legacy.local_tag_entries(specs)
            actual['rules'] = json.loads(subprocess.check_output(
                [str(ROOT / 'rewrite/src-tauri/target/debug/examples/characterize')],
                input=json.dumps({'mode': 'rules', 'specs': specs}), text=True))
            tags_count += len(actual['rules'])
            for key in expected:
                if expected[key] != actual[key]:
                    differences.append({'path': item['path'], 'field': key, 'legacy': expected[key], 'rust': actual[key]})
    print(json.dumps({'files': count, 'rule_candidates': tags_count, 'differences': differences}, ensure_ascii=False, indent=2))
    return 1 if differences else 0

if __name__ == '__main__':
    sys.exit(main())
