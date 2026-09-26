from __future__ import annotations

import json
from pathlib import Path

from bacup_lib.run.run_handle import ConversionRun

OUTPUT = Path("F4SE/Plugins/B21_TalesFromAppalachia/challenges.json")
REPORT = Path("debug/challenges_report.json")


def emit_challenge_catalog(source_plugin: Path, converted_mod: Path, target_data_dir: Path | None,
                           output_plugin_name: str = "SeventySix.esm", *, source_sha256: str | None = None) -> dict:
    source_plugin, converted_mod = Path(source_plugin), Path(converted_mod)
    converted_plugin = converted_mod / output_plugin_name
    for path in (source_plugin, converted_plugin):
        if not path.is_file():
            raise FileNotFoundError(f"Challenge catalog input is missing: {path}")
    if target_data_dir is not None and not Path(target_data_dir).is_dir():
        raise FileNotFoundError(f"Challenge catalog target data directory is missing: {target_data_dir}")
    catalog_path = converted_mod / OUTPUT
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
    return json.loads(catalog_path.read_text(encoding="utf-8"))


if __name__ == "__main__":
    import argparse

    parser = argparse.ArgumentParser(description="Emit the Tales challenge catalog from FO76 CHAL records.")
    parser.add_argument("--source", type=Path, required=True, help="FO76 SeventySix.esm")
    parser.add_argument("--converted", type=Path, required=True, help="converted mod directory holding the output master")
    parser.add_argument("--target-data", type=Path, help="Fallout 4 Data directory for vanilla EditorID lookups")
    parser.add_argument("--output-plugin", default="SeventySix.esm")
    args = parser.parse_args()
    result = emit_challenge_catalog(args.source, args.converted, args.target_data, args.output_plugin)
    print(f"Wrote {len(result['challenges'])} challenges to {args.converted / OUTPUT}; report at {args.converted / REPORT}")
