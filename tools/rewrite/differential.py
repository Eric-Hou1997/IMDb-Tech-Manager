#!/usr/bin/env python3
"""Compare new Rust pure functions with the unchanged legacy Python engine.

Only repository fixtures and in-memory requests are used. No HTTP or credentials.
Build `cargo build -p itm-core --example characterize` in rewrite/src-tauri first.
"""
import importlib.util
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("legacy", ROOT / "macos/engine/mac-engine.py")
legacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(legacy)
binary = ROOT / "rewrite/src-tauri/target/debug/examples/characterize"


def rust(value):
    return json.loads(subprocess.check_output([str(binary)], input=json.dumps(value, ensure_ascii=False), text=True))


for fixture in sorted((ROOT / "macos/tests/fixtures").glob("*.nfo")):
    raw = fixture.read_bytes().decode("utf-8-sig")
    actual = rust({"mode": "nfo", "raw": raw})
    identity = legacy.inspect_nfo(raw)
    assert actual["title"] == identity["title"], fixture.name
    assert actual["year"] == identity["year"], fixture.name
    assert actual["imdb"] == legacy.imdb_id(raw), fixture.name
    old = legacy.existing_tech_object(raw)
    old_tags=legacy._tag_rows(raw,old)
    assert [(t['value'],t['ownership']) for t in actual['tags']]==[(t['value'],t['ownership']) for t in old_tags],fixture.name
    if old:
        expected = {k: v for k, v in old["specs"].items() if v}
        assert actual["specs"] == expected, (fixture.name, actual["specs"], expected)
    print("PASS NFO identity and effective specs:", fixture.name)

specs = {"Camera": ["ARRI (IMAX)", "Sony"], "Runtime": ["124 min"], "Sound mix": ["Dolby Atmos"]}
for protocol in ("openai", "anthropic"):
    cfg = {"protocol": protocol, "provider": "qwen", "base_url": "https://dashscope.aliyuncs.com/compatible-mode/v1", "model": "qwen-plus", "prompt": legacy.DEFAULT_AI_PROMPT, "output_language": "zh-CN", "temperature": 0, "top_p": 1, "max_tokens": 2000, "thinking_mode": "off", "prompt_cache_mode": "on", "json_mode": True, "extra_body": {}}
    old_cfg = {**cfg, "api_protocol": protocol, "extra_body": "{}"}
    producer = legacy._build_openai_request if protocol == "openai" else legacy._build_anthropic_request
    expected = producer(specs, old_cfg, True, True, [])
    actual = rust({"mode": "request", "config": cfg, "specs": specs})
    # JSON key ordering in the compact user payload is not a semantic change.
    index = 1 if protocol == "openai" else 0
    expected["messages"][index]["content"] = json.loads(expected["messages"][index]["content"])
    actual["messages"][index]["content"] = json.loads(actual["messages"][index]["content"])
    assert actual == expected, protocol
    print("PASS request, default prompt and thinking/cache policy:", protocol)

payload = {"runtimes": {"edges": [{"node": {"displayableProperty": {"value": {"plainText": "2h 4m"}}, "seconds": 7440}}]}, "technicalSpecifications": {"cameras": {"items": [{"camera": "ARRI", "attributes": [{"text": "IMAX"}]}, {"camera": "Sony"}, {"camera": "Sony"}]}}}
source = '<script id="__NEXT_DATA__" type="application/json">' + json.dumps(payload) + '</script>'
expected, found = legacy.parse_next_data_specs(source)
assert found
assert rust({"mode": "next-data", "data": payload}) == expected
print("PASS structured IMDb parser:", "ten fields and separate bullets")

rule_cases = [
    "Panavision B- and C-Series Lenses",
    "Panavision C-, D-, E- and H-Series Lenses, Sony Venice (aerial shots)",
    "Zeiss Ultra Prime and Angenieux Optimo Lenses",
    "ARRI ALEXA (IMAX, scenes), Canon C300 (underwater shots)",
    "Bell and Howell 2709, Sony F65",
    "Arri Alexa Mini, Panavision Primo, Retro C-, E-, G- and T-Series Lenses",
    "Arri Alexa XT Plus, Panavision Primo, Retro C-, E-, G- T-Series, ATZ and AWZ2 Lenses",
    "中文品牌 [镜头, 备注]; Sony &amp; Canon Lenses",
    "STRASSE, Straße, ARRI&nbsp;Alexa",
]
for value in rule_cases:
    specs = {"Camera": [value], "Sound mix": ["DTS (DTS:X)", "Dolby Atmos (theatrical)"], "Aspect ratio": ["2.39 : 1 (theatrical)"], "Negative Format": ["35 mm"]}
    assert rust({"mode": "rules", "specs": specs}) == legacy.local_tag_entries(specs), value
    print("PASS local rules, provenance and order:", value)
