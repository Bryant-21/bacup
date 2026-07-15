from __future__ import annotations

from pathlib import Path

import yaml

from bacup_lib.models import AssetRef, DependencyGraph, RecordNode


def _flatten_fields(record: dict) -> dict[str, object]:
    flat: dict[str, object] = {}
    for entry in record.get("fields", []) or []:
        if isinstance(entry, dict) and len(entry) == 1:
            key, value = next(iter(entry.items()))
            flat[key] = value
    return flat


def _weap_assets(record: dict, fixture_root: Path) -> list[AssetRef]:
    flat = _flatten_fields(record)
    assets: list[AssetRef] = []
    seen: set[tuple[str, str]] = set()

    def add_asset(asset_type: str, source_path: str | None) -> None:
        if not isinstance(source_path, str) or not source_path.strip():
            return
        key = (asset_type, source_path)
        if key in seen:
            return
        seen.add(key)
        assets.append(
            AssetRef(
                asset_type,
                source_path,
                str(fixture_root / Path(source_path.replace("\\", "/"))),
            )
        )

    modl = flat.get("MODL")
    if isinstance(modl, dict):
        add_asset("nif", modl.get("Filename"))
    for slot in (1, 2, 3):
        add_asset("nif", flat.get(f"ModelMod{slot}"))
    attack_animation = flat.get("AttackAnimation")
    if isinstance(attack_animation, dict):
        add_asset("kf_animation", attack_animation.get("Filename"))
    return assets


def build_graph_from_yaml_dir(
    yaml_dir: str | Path,
    *,
    source_plugin: str = "FalloutNV.esm",
) -> DependencyGraph:
    """Build a tiny test-only graph from fixture YAML files."""
    yaml_root = Path(yaml_dir)
    fixture_root = yaml_root.parent
    all_records: list[RecordNode] = []
    all_assets: list[AssetRef] = []
    seen_assets: set[tuple[str, str]] = set()

    for yaml_path in sorted(yaml_root.glob("*.yaml")):
        payload = yaml.safe_load(yaml_path.read_text(encoding="utf-8")) or {}
        record_type = str(payload.pop("record_type"))
        form_id = f"{int(str(payload.get('form_id', '0')), 16) & 0x00FFFFFF:06X}"
        editor_id = str(payload.get("eid") or yaml_path.stem)
        assets = _weap_assets(payload, fixture_root) if record_type == "WEAP" else []
        for asset in assets:
            key = (asset.asset_type, asset.source_path)
            if key in seen_assets:
                continue
            seen_assets.add(key)
            all_assets.append(asset)
        all_records.append(
            RecordNode(
                form_key=f"{form_id}:{source_plugin}",
                editor_id=editor_id,
                record_type=record_type,
                assets=assets,
            )
        )

    if not all_records:
        raise ValueError(f"No YAML records found in {yaml_root}")

    root = next((record for record in all_records if record.record_type == "WEAP"), all_records[0])
    return DependencyGraph(
        root=root,
        all_records=all_records,
        all_assets=all_assets,
        errors=[],
    )


