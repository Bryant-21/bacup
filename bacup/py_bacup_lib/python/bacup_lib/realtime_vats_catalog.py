from __future__ import annotations

import json
import math
from pathlib import Path

from bacup_lib.plugin_catalog import write_document
from bacup_lib.reputation_catalog import first, reference_id

FEATURE = "RealtimeVATS"
PLUGIN = "SeventySix.esm"
LUCK_CURVE = "LuckVATSCriticalChargeCurve_DO"


def build_catalog(source, source_root: Path) -> dict:
    _, default_object = source.lookup("DFOB", LUCK_CURVE)
    curve_id = reference_id(first(default_object, "Object"))
    if curve_id is None:
        raise ValueError(f"{LUCK_CURVE} has no curve")
    signature, editor_id, fields = source.fields(curve_id)
    relative = first(fields, "JASF") or first(fields, "CRVE")
    if signature != "CURV" or not isinstance(relative, str):
        raise ValueError(f"{LUCK_CURVE} does not name a curve table")
    root = (Path(source_root) / "misc/curvetables/json").resolve()
    path = (root / relative.replace("\\", "/")).resolve()
    if not path.is_relative_to(root):
        raise ValueError("Luck critical curve escapes the source root")
    points = []
    for point in json.loads(path.read_text(encoding="utf-8"))["curve"]:
        x, y = float(point["x"]), float(point["y"])
        if not (math.isfinite(x) and math.isfinite(y)) or y < 0 or (points and x <= points[-1]["x"]):
            raise ValueError(f"Invalid Luck critical curve {relative}")
        points.append({"x": x, "y": y})
    if not points:
        raise ValueError(f"Empty Luck critical curve {relative}")
    return {"version": 1, "luck_critical_charge_curve": points,
            "source": {"default_object": LUCK_CURVE, "curve": editor_id, "file": relative}}


def to_document(catalog: dict) -> dict:
    return {"version": catalog["version"], "settings": {key: value for key, value in catalog.items() if key != "version"}}


def emit_catalog(source, output_mod: Path, source_root: Path) -> dict:
    catalog = build_catalog(source, source_root)
    document = to_document(catalog)
    json.dumps(document, allow_nan=False)
    write_document(FEATURE, PLUGIN, document, output_mod)
    return catalog


def emit_from_plugin(source_plugin: Path, output_mod: Path, source_root: Path) -> dict:
    from bacup_lib.esp_native_runtime import load_esp_native
    from bacup_lib.player_ghoul import Source

    native = load_esp_native()
    handle = native.plugin_handle_load_index(str(source_plugin), game="fo76")
    try:
        return emit_catalog(Source(native, handle), output_mod, source_root)
    finally:
        native.plugin_handle_close(handle)


if __name__ == "__main__":
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--source-root", type=Path, required=True)
    parser.add_argument("--output-mod", type=Path, required=True)
    args = parser.parse_args()
    result = emit_from_plugin(args.source, args.output_mod, args.source_root)
    print(f"Real-time VATS catalog: {len(result['luck_critical_charge_curve'])} Luck curve points")
