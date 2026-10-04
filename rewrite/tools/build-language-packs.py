#!/usr/bin/env python3
"""v5 presentation additions; reuse the original deterministic ZIP recipe.

Default validation computes bytes in memory and does not emit release assets.
The published r1 sources and descriptors are never changed.
"""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile

ROOT = Path(__file__).resolve().parents[2]
CATALOG_PATH = ROOT / "rewrite/src-tauri/core/assets/language_catalog.json"
LOCALES = ["fr-FR", "ru-RU", "ja-JP", "es-ES", "th-TH"]


def recipe():
    spec = importlib.util.spec_from_file_location("original_packs", ROOT / "tools/build-language-packs.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def build_pack(catalog, locale):
    module = recipe()
    descriptor = catalog["languages"][locale]
    if descriptor["revision"] != 2 or descriptor["released_with"] != "v5.0.0":
        raise ValueError("v5 additions require a new, release-bound r2 descriptor")
    source = json.loads((ROOT / f"language-packs/{locale}/r1/translations.json").read_text())
    additions = json.loads((ROOT / "rewrite/language-packs/task-messages.json").read_text())
    for english, translations in additions.items():
        if len(translations) != len(LOCALES):
            raise ValueError(f"Incomplete locale coverage: {english}")
        if any(english in source[s] for s in source):
            raise ValueError(f"Do not replace an original translation: {english}")
        source["engine"][english] = translations[LOCALES.index(locale)]
    platform = json.loads((ROOT / "rewrite/language-packs/platform-messages.json").read_text())
    for row in platform.values():
        if len(row["translations"]) != len(LOCALES) or any(row["english"] in section for section in source.values()):
            raise ValueError("Platform presentation must have full, new locale entries")
        source["web"][row["english"]] = row["translations"][LOCALES.index(locale)]
    # Feed full original sections plus the additions to the unchanged validator.
    with tempfile.TemporaryDirectory(prefix="itm-language-source-") as directory:
        module.PACK_ROOT = Path(directory)
        path = module.PACK_ROOT / locale / "r2/translations.json"
        path.parent.mkdir(parents=True)
        path.write_bytes(module.json_bytes(source))
        return module.build_pack(catalog, locale, descriptor, module.required_messages())


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--update-catalog", action="store_true")
    args = parser.parse_args()
    catalog = json.loads(CATALOG_PATH.read_text())
    sets = set()
    for locale in LOCALES:
        payload, message_set = build_pack(catalog, locale)
        descriptor = catalog["languages"][locale]
        digest = hashlib.sha256(payload).hexdigest()
        sets.add(message_set)
        if args.update_catalog:
            descriptor.update(sha256=digest, message_set_hash=message_set)
        elif descriptor["sha256"] != digest or descriptor["message_set_hash"] != message_set:
            raise ValueError(f"Catalog hash mismatch: {locale}")
    if len(sets) != 1:
        raise ValueError("Locale message inventories differ")
    if args.update_catalog:
        CATALOG_PATH.write_text(json.dumps(catalog, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
    print("OK: five r2 language descriptors, full original coverage, protected tokens and exact hashes; no assets emitted")


if __name__ == "__main__":
    main()
