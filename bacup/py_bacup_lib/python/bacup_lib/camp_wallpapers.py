"""FO76 CAMP wallpapers for Tales.

A FO76 wallpaper is an OMOD on an interior wall. It adds a model swap (MDSP: each wall mesh to its
`_wallpaper` variant) and a material swap (MSWP: the variant's blank wallpaper surface to the
pattern). FO4 cannot attach those OMODs to a wall and has no MDSP, so Tales applies both swaps to
the reference from this catalog, as it applies FO76 weapon skins from `WeaponModelSwaps.json`.

Most `_wallpaper` meshes lack the base wall's connect points, and a few its navmesh cutters. Swapped
in as they are, a wall would stop taking snapped pieces and NPCs would stop pathing through its
doorway, so each takes the base mesh's.
"""
from __future__ import annotations

import os
import tempfile
from pathlib import Path

from creation_lib.nif.nif_file import NifFile
from creation_lib.nif.operations.copy import copy_blocks

from bacup_lib.plugin_catalog import write_document
from bacup_lib.reputation_catalog import (
    NativeReader, NativeResolver, Reader, Resolver, first, load_esp_native, plugin_masters, reference_id,
)

WALLPAPER_LIST = "ATX_workshop_LL_Wallpapers"
FEATURE = "CampWallpapers"
FORM_SET, FORM_ADD = 0, 2
FORM_VALUE = 4


def mesh_path(path: str) -> str:
    path = path.strip().replace("/", "\\").lower()
    return path.removeprefix("meshes\\")


def added_forms(fields: dict[str, list]) -> list[int]:
    return [row["Value1"] for data in fields.get("Data", []) for row in data.get("Properties", [])
            if row.get("ValueType") == FORM_VALUE and row.get("FunctionType", FORM_SET) in (FORM_SET, FORM_ADD)
            and row.get("Value1")]


def model_pairs(fields: dict[str, list]) -> list[list[str]]:
    pairs = []
    for original, replacement in zip(fields.get("InitialModel", []), fields.get("NewModel", [])):
        original, replacement = mesh_path(original), mesh_path(replacement)
        if original and replacement and original != replacement:
            pairs.append([original, replacement])
    return pairs


def build_catalog(reader: Reader, resolver: Resolver) -> dict:
    """The wallpapers in FO76's menu order, Remove first (`strip`: it takes the swaps off), and the mesh pairs each one swaps."""
    _, listing = reader.lookup("LVLI", WALLPAPER_LIST)
    wallpapers, model_swaps, missing = [], {}, []
    for entry in listing.get("LVLO", []):
        object_id = reference_id(entry.get("value") if isinstance(entry, dict) else None)
        if object_id is None:
            continue
        signature, editor_id, fields = reader.fields(object_id)
        if signature != "OMOD":
            continue
        swap = models = None
        for form in added_forms(fields):
            kind, form_editor_id, form_fields = reader.fields(form)
            if kind == "MSWP":
                swap = resolver.resolve("MSWP", form_editor_id)
                if swap is None:
                    missing.append(form_editor_id)
            elif kind == "MDSP":
                models = form_editor_id
                model_swaps.setdefault(models, model_pairs(form_fields))
        art_id = reference_id(first(fields, "ArtObject"))
        art = resolver.resolve("ARTO", reader.fields(art_id)[1]) if art_id is not None else None
        form = resolver.resolve("OMOD", editor_id)
        if form is None:
            missing.append(editor_id)
            continue
        if swap is None and models is None:
            wallpapers.append({"form": form, "editor_id": editor_id, "strip": True, "art": art})
        elif swap is not None and models is not None:
            wallpapers.append({"form": form, "editor_id": editor_id, "material_swap": swap,
                               "model_swap": models, "art": art})
    return {"wallpapers": wallpapers, "model_swaps": model_swaps, "missing_converted": missing}


def _children(block) -> list[int]:
    return [child for child in block.get_field("Children") or [] if child >= 0]


def _append(block, field: str, count: str, value: int) -> None:
    values = list(block.get_field(field) or [])
    values.append(value)
    block.set_field(field, values)
    block.set_field(count, len(values))


def _cutters(nif: NifFile) -> list[int]:
    return [child for child in _children(nif.blocks[0])
            if "navmeshcut" in str(nif.blocks[child].get_field("Name") or "").lower()]


def _connect_points(nif: NifFile) -> list[int]:
    return [extra for extra in nif.blocks[0].get_field("Extra Data List") or []
            if extra >= 0 and nif.blocks[extra].type_name == "BSConnectPoint::Parents"]


def _bsx(nif: NifFile):
    return next((block for block in nif.blocks if block.type_name == "BSXFlags"), None)


def _identity(block) -> bool:
    translation = block.get_field("Translation") or {}
    rotation = block.get_field("Rotation") or {}
    return (all(abs(value) < 1e-4 for value in translation.values())
            and abs((block.get_field("Scale") or 1.0) - 1.0) < 1e-4
            and all(abs(value - (key in ("m11", "m22", "m33"))) < 1e-4 for key, value in rotation.items()))


def _wall_collision(nif: NifFile, cutters: list[int]) -> int | None:
    """The one collision object sharing the cutters' physics system, if it sits where the wallpaper
    mesh's root can carry it unchanged: on the root, or on a root child with no transform."""
    systems = {nif.blocks[nif.blocks[cutter].get_field("Collision Object")].get_field("Data")
               for cutter in cutters if (nif.blocks[cutter].get_field("Collision Object") or -1) >= 0}
    found = [index for index, block in enumerate(nif.blocks)
             if block.type_name == "bhkNPCollisionObject" and block.get_field("Data") in systems
             and block.get_field("Target") not in cutters]
    if len(found) != 1:
        return None
    target = nif.blocks[found[0]].get_field("Target")
    if target == 0 or (target in _children(nif.blocks[0]) and _identity(nif.blocks[target])):
        return found[0]
    return None


def _copy(base: NifFile, wallpaper: NifFile, block_ids: list[int]) -> dict[int, int]:
    for block_id in block_ids:
        name = base.blocks[block_id].get_field("Name")
        if isinstance(name, str):
            wallpaper.add_string(name)
    return copy_blocks(base, block_ids, wallpaper)


def adopt_base_fixtures(base: NifFile, wallpaper: NifFile) -> tuple[list[str], list[str]]:
    """Gives the wallpaper mesh the base wall's connect points and, when it has none, its navmesh
    cutters. A cutter is a body in the wall's own physics system, so the wall collision comes with
    it and replaces the wallpaper mesh's. Returns what changed and what could not be."""
    changed = []
    if _connect_points(base) and not _connect_points(wallpaper):
        names = {block.get_field("Name") for block in wallpaper.blocks}
        parents = {point["Parent"] for extra in _connect_points(base)
                   for point in base.blocks[extra].get_field("Connect Points") or []}
        nodes = [child for child in _children(base.blocks[0])
                 if base.blocks[child].get_field("Name") in parents - names]
        extras = _connect_points(base)
        copied = _copy(base, wallpaper, extras + nodes)
        root = wallpaper.blocks[0]
        for extra in extras:
            _append(root, "Extra Data List", "Num Extra Data List", copied[extra])
        for node in nodes:
            _append(root, "Children", "Num Children", copied[node])
        changed.append("connect points")
    cutters = _cutters(base)
    if not cutters or _cutters(wallpaper):
        return changed, []
    collision = _wall_collision(base, cutters)
    if collision is None:
        return changed, ["navmesh cutters"]
    own = [index for index, block in enumerate(wallpaper.blocks) if block.type_name == "bhkNPCollisionObject"]
    systems = {wallpaper.blocks[index].get_field("Data") for index in own} - {None, -1}
    wallpaper.remove_blocks(sorted(set(own) | systems))
    copied = _copy(base, wallpaper, [collision, *cutters])
    root = wallpaper.blocks[0]
    root.set_field("Collision Object", copied[collision])
    wallpaper.blocks[copied[collision]].set_field("Target", 0)
    for cutter in cutters:
        _append(root, "Children", "Num Children", copied[cutter])
    base_bsx, own_bsx = _bsx(base), _bsx(wallpaper)
    if base_bsx is not None and own_bsx is not None:
        own_bsx.set_field("Integer Data", base_bsx.get_field("Integer Data"))
    changed.append("navmesh cutters")
    return changed, []


def _save(nif: NifFile, path: Path) -> None:
    descriptor, pending = tempfile.mkstemp(suffix=".nif", dir=path.parent)
    os.close(descriptor)
    try:
        nif.save(pending)
        os.replace(pending, path)
    except BaseException:
        Path(pending).unlink(missing_ok=True)
        raise


def prepare_wallpaper_meshes(meshes: Path, model_swaps: dict[str, list[list[str]]]) -> dict:
    report = {"repaired": [], "unchanged": 0, "missing": [], "unrepaired": []}
    seen = set()
    for pairs in model_swaps.values():
        for original, replacement in pairs:
            if replacement in seen:
                continue
            seen.add(replacement)
            base_path, wallpaper_path = meshes / original, meshes / replacement
            if not base_path.is_file() or not wallpaper_path.is_file():
                report["missing"].append(replacement)
                continue
            wallpaper = NifFile.load(str(wallpaper_path))
            changed, unrepaired = adopt_base_fixtures(NifFile.load(str(base_path)), wallpaper)
            if unrepaired:
                report["unrepaired"].append({"mesh": replacement, "missing": unrepaired})
            if changed:
                _save(wallpaper, wallpaper_path)
                report["repaired"].append({"mesh": replacement, "changed": changed})
            else:
                report["unchanged"] += 1
    return report


def emit_camp_wallpapers(source_plugin: Path, converted_plugin: Path, output_mod: Path) -> dict:
    native = load_esp_native()
    handles = []
    try:
        source = native.plugin_handle_load_index(str(source_plugin), game="fo76")
        handles.append(source)
        converted = native.plugin_handle_load_index(str(converted_plugin), game="fo4")
        handles.append(converted)
        catalog = build_catalog(NativeReader(native, source),
                                NativeResolver(native, converted_plugin.name, converted, plugin_masters(native, converted)))
    finally:
        for handle in handles:
            native.plugin_handle_close(handle)
    meshes = prepare_wallpaper_meshes(Path(output_mod) / "data" / "Meshes", catalog["model_swaps"])
    catalog["missing_meshes"] = meshes["missing"]
    write_document(FEATURE, converted_plugin.name,
                   {"version": 1, "wallpapers": catalog["wallpapers"], "model_swaps": catalog["model_swaps"]}, output_mod)
    return {**catalog, "repaired_meshes": meshes["repaired"], "unchanged_meshes": meshes["unchanged"],
            "unrepaired_meshes": meshes["unrepaired"]}
