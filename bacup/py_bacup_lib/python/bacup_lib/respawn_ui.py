from __future__ import annotations

import hashlib
import json
from pathlib import Path

from bacup_lib.reputation_catalog import NativeReader, NativeResolver, first, load_esp_native, plugin_masters
from bacup_lib.reputation_ui import reputation_translations
from bacup_lib.translations import PACKAGED_TALES_RESOURCE, merge_ui_translations, read_table

CATALOG = Path("F4SE/Plugins/B21_TalesFromAppalachia/respawn.json")
SOURCE_BAG = "Lootbag_Dropped_PaperBag"
BAG_MODEL = "SetDressing/BrownBag01/BrownBag01.nif"
BAG_ASSETS = (
    "Meshes/" + BAG_MODEL,
    "Materials/SetDressing/BrownBag01/BrownBag01.bgsm",
    *(f"Textures/SetDressing/BrownBag01/BrownBag01_{channel}.dds" for channel in ("d", "n", "s")),
)


def bag_record(source_plugin: Path, converted_plugin: Path) -> dict:
    native = load_esp_native()
    handles = []
    try:
        for path, game in ((source_plugin, "fo76"), (converted_plugin, "fo4")):
            handles.append(native.plugin_handle_load_index(str(path), game=game))
        source, converted = (NativeReader(native, handle) for handle in handles)
        source_id, source_fields = source.lookup("CONT", SOURCE_BAG)
        _, converted_fields = converted.lookup("CONT", SOURCE_BAG)
        source_model = str(first(source_fields, "MODL")).replace("\\", "/").lstrip("/")
        model = str(first(converted_fields, "MODL")).replace("\\", "/")
        if source_model.casefold() != ("Meshes/" + BAG_MODEL).casefold() or model.casefold() != BAG_MODEL.casefold():
            raise ValueError("Death bag model changed; re-audit the native conversion asset closure")
        resolved = NativeResolver(native, converted_plugin.name, handles[1],
                                  plugin_masters(native, handles[1])).resolve("CONT", SOURCE_BAG)
        if not resolved:
            raise ValueError("Converted source death bag is missing")
        return {"source": f"{source_plugin.name}:{source_id:06X}", "converted": resolved, "model": model}
    finally:
        for handle in reversed(handles):
            native.plugin_handle_close(handle)


def convert_respawn_ui(source_root: Path, output_mod: Path, *, source_plugin: Path,
                       converted_plugin: Path) -> dict:
    source_root, output_mod = Path(source_root), Path(output_mod)
    bag = bag_record(Path(source_plugin), Path(converted_plugin))
    # The normal record/mesh/material/texture phases own these assets. A UI-only
    # conversion must not advertise a bag whose native conversion is incomplete.
    assets = []
    for relative in BAG_ASSETS:
        path = output_mod / "data" / relative
        if not path.is_file() or not path.stat().st_size:
            raise FileNotFoundError(f"Death bag requires native-converted asset: {path}")
        assets.append({"path": relative, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
    source_mesh = source_root / "Meshes" / BAG_MODEL
    if not source_mesh.is_file():
        raise FileNotFoundError(f"Death bag source has not been extracted: {source_mesh}")
    lines = reputation_translations(source_root / "interface")
    lines += [line for line in read_table(Path(__file__).parent / PACKAGED_TALES_RESOURCE)
              if line.startswith("$B21_Respawn")]
    labels = dict(line.split("\t", 1) for line in lines)
    manifest = {"version": 1, "labels": labels, "files": [], "bag": bag,
                "assets": assets, "source_mesh_sha256": hashlib.sha256(source_mesh.read_bytes()).hexdigest()}
    catalog = output_mod / CATALOG
    catalog.parent.mkdir(parents=True, exist_ok=True)
    catalog.write_text(json.dumps({"version": 1, **bag, "assets": list(BAG_ASSETS), "labels": labels,
                                  "map_contract": {"version": 1, "transport": "F4SE", "message_type": "42325444"}},
                                 indent=2) + "\n", encoding="utf-8")
    manifest["files"].append({"path": CATALOG.as_posix(), "sha256": hashlib.sha256(catalog.read_bytes()).hexdigest()})
    merge_ui_translations(output_mod / "data", lines)
    return manifest
