from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.native_runtime import load_native_module
from bacup_lib.plugin_catalog import catalog_path, write_document
from bacup_lib.translations import merge_packaged, translation_path

FEATURE = "QuestCategories"
PLUGIN = "SeventySix.esm"
OUTPUT = catalog_path(FEATURE, PLUGIN)


def inspect_presentation(source_root: Path) -> dict:
    from creation_lib.swf.native_runtime import abc_class_names, abc_disassemble

    movie = Path(source_root) / "interface/newpipboy_datapage.swf"
    source = movie.read_bytes()
    required = {"NewPipBoy_DataPage", "NewPipBoyShared", "QuestsList", "ObjectivesList"}
    if not required.issubset(abc_class_names(source)):
        raise ValueError("FO76 quest presentation classes are unavailable")
    methods = abc_disassemble(source, "NewPipBoyShared", "static initializer")
    code = "\n".join(line for method in methods for line in method["code"])
    keys = ("$QUESTS_PRIMARY", "$QUESTS_SECONDARY", "$QUESTS_DAILY", "$QUESTS_LEADS", "$QUESTS_MISC")
    if any(f'PushString "{key}"' not in code for key in keys):
        raise ValueError("Unsupported FO76 quest tab contract")
    return {"available": True, "source_movie": movie.name,
            "sha256": hashlib.sha256(source).hexdigest(), "source_labels": list(keys),
            "component_owner": "vanilla_fo4", "converted_movies": []}


def to_document(catalog: dict) -> dict:
    settings = {key: catalog[key] for key in ("categories", "presentation", "missing_converted", "source_sha256") if key in catalog}
    return {"version": catalog["schema_version"], "settings": settings, "quests": catalog["quests"]}


def emit_quest_catalog(source_plugin: Path, converted_plugin: Path,
                       source_root: Path, output_mod: Path) -> dict:
    source_plugin, converted_plugin, output_mod = map(Path, (source_plugin, converted_plugin, output_mod))
    # Never let a previous successful catalog enable a partial/new failed conversion.
    (output_mod / OUTPUT).unlink(missing_ok=True)
    presentation = inspect_presentation(source_root)
    result = json.loads(load_native_module().build_pipboy_quest_catalog(
        str(source_plugin), str(converted_plugin), json.dumps(presentation),
    ))
    with source_plugin.open("rb") as stream:
        result["source_sha256"] = hashlib.file_digest(stream, "sha256").hexdigest()
    merge_packaged(translation_path(output_mod / "data"))
    write_document(FEATURE, converted_plugin.name, to_document(result), output_mod)
    return result


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--converted-plugin", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output-mod", type=Path, required=True)
    args = parser.parse_args()
    catalog = emit_quest_catalog(args.source, args.converted_plugin, args.source_root, args.output_mod)
    print(f"Wrote {len(catalog['quests'])} quest categories; {len(catalog['missing_converted'])} unmapped")
