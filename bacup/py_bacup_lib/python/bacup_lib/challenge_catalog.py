from __future__ import annotations

import json
from pathlib import Path

from bacup_lib.plugin_catalog import form_key, write_document
from bacup_lib.run.run_handle import ConversionRun

FEATURE = "Challenges"
RAW_CATALOG = Path("debug/challenges_native.json")
REPORT = Path("debug/challenges_report.json")


def to_document(raw: dict, output_plugin: str) -> dict:
    """The native phase's catalog (bare hex ids relative to the output plugin) as a Challenges document."""

    def link(value):
        return form_key(output_plugin, int(value, 16)) if value else value

    challenges = []
    for row in raw["challenges"]:
        row = dict(row, id=link(row["id"]), parent=link(row.get("parent")),
                   prerequisites=[link(v) for v in row.get("prerequisites", [])],
                   sub_challenges=[link(v) for v in row.get("sub_challenges", [])])
        challenges.append(row)
    return {"version": 1, "settings": {"source": raw.get("source", {}), "output_plugin": output_plugin},
            "categories": raw.get("categories", []), "challenges": challenges}


def emit_challenge_catalog(source_plugin: Path, converted_mod: Path, target_data_dir: Path | None,
                           output_plugin_name: str = "SeventySix.esm", *, source_sha256: str | None = None) -> dict:
    source_plugin, converted_mod = Path(source_plugin), Path(converted_mod)
    converted_plugin = converted_mod / output_plugin_name
    for path in (source_plugin, converted_plugin):
        if not path.is_file():
            raise FileNotFoundError(f"Challenge catalog input is missing: {path}")
    if target_data_dir is not None and not Path(target_data_dir).is_dir():
        raise FileNotFoundError(f"Challenge catalog target data directory is missing: {target_data_dir}")
    catalog_path = converted_mod / RAW_CATALOG
    catalog_path.unlink(missing_ok=True)
    catalog_path.parent.mkdir(parents=True, exist_ok=True)
    with ConversionRun.create_new("fo76", "fo4", None, output_plugin_name) as run:
        run.run_phase(
            "emit_challenge_catalog", mod_path=str(converted_mod),
            target_data_dir=str(target_data_dir) if target_data_dir is not None else None,
            params={
                "source_plugin": str(source_plugin), "converted_plugin": str(converted_plugin),
                "catalog_path": str(catalog_path), "report_path": str(converted_mod / REPORT),
                "source_sha256": source_sha256,
            },
        )
    document = to_document(json.loads(catalog_path.read_text(encoding="utf-8")), output_plugin_name)
    write_document(FEATURE, output_plugin_name, document, converted_mod)
    return document


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Emit the Tales challenge catalog from FO76 CHAL records.")
    parser.add_argument("--source", type=Path, required=True, help="FO76 SeventySix.esm")
    parser.add_argument("--converted", type=Path, required=True, help="converted mod directory holding the output master")
    parser.add_argument("--target-data", type=Path, help="Fallout 4 Data directory for vanilla EditorID lookups")
    parser.add_argument("--output-plugin", default="SeventySix.esm")
    args = parser.parse_args()
    result = emit_challenge_catalog(args.source, args.converted, args.target_data, args.output_plugin)
    print(f"Wrote {len(result['challenges'])} challenges to {args.converted}; report at {args.converted / REPORT}")
