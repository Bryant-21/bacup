"""FO76 world-object scrapping for Tales (CampScrap/<plugin>.json).

FO4 scraps a world object when a workshop COBJ filtered by WorkshopRecipeFilterScrap creates its base,
directly or through a FLST; Workshop::InitItemRecipeMaps turns those into base -> recipe entries and the
recipe gives the components. FO76 has no such recipes. It marks a base deletable with the PRPS actor
value WorkshopCanBeDeleted_NEW (4EE05C), falling back to the older WorkshopCanBeDeleted_OLD (017AAC);
one base sets NEW 0 over OLD 1, so NEW wins when present.

Each deletable base borrows an FO4 scrap recipe, best evidence first: the FO4 base with the same object
id and type whose EditorID or model agrees (FO76 kept FO4's ids for carried-over objects), the same
EditorID, the same model, a close name, a class rule, and finally a generic junk recipe.
"""
from __future__ import annotations

import math
import re
import struct
from collections import Counter
from dataclasses import dataclass
from pathlib import Path

from bacup_lib.esp_native_runtime import load_esp_native
from bacup_lib.plugin_catalog import form_key, write_document
from bacup_lib.reputation_catalog import plugin_masters
from bacup_lib.target_masters import resolve_official_target_master_paths

FEATURE = "CampScrap"
VERSION = 1

CAN_BE_DELETED_NEW = 0x4EE05C
CAN_BE_DELETED_OLD = 0x017AAC
SCRAP_FILTER = ("fallout4.esm", 0x106D8F)  # WorkshopRecipeFilterScrap

# A name match must cover this share of both names' token weight; the higher bar outranks the class rules.
CONFIDENT_SIMILARITY = 0.45
LOOSE_SIMILARITY = 0.25

# FO76 kit, location and condition words that say nothing about what an object is made of.
STOP_WORDS = frozenset("""
dlc fo76 static stat burn score cheap dirty dirt postwar prewar copy clean intact
damaged destroyed rusty rusted weathered nocollision nocol small large med default temp test zzz zzzz zzzzz nophys
havok phys ash toxic player house playerhouse ruin pitt mtn rem mtnrem crater swamp mud sand dry scum storm xpd
xpdac atx moon wendigo colossus gorge junkyard workshop scrap scrappable scrapable obstacle clutter assorted group
alt nochunks chunks kit dckit green blue red white black yellow brkn broke broken open closed empty full filled set
light sit lay npc the and with bare
""".split())

# Class rules, first hit wins: FO4 recipe EditorID -> words in the FO76 EditorID or model name.
CLASS_RULES = (
    ("workshop_co_ScrapCars", "car cars sedan coupe wagon stationwagon pickup truck van bus limousine apc jeep vehicle "
                              "carframe firetruck taxi ambulance towtruck loader tractor"),
    ("workshop_co_ScrapRowboat01", "boat rowboat jetboat canoe"),
    ("workshop_co_ScrapTricycle01", "bicycle unicycle tricycle bike"),
    ("workshop_co_ScrapCartWheeled", "cart trolley wheelbarrow"),
    ("workshop_co_ScrapTirePile", "tire tires"),
    ("workshop_co_ScrapMattress", "mattress sleeping bedroll"),
    ("workshop_co_ScrapRadio", "television radio"),
    ("workshop_co_ScrapMetal02", "fence fencing railing guardrail rail"),
    ("workshop_co_ScrapWoodCrates", "crate crates"),
    ("workshop_co_ScrapFurnitureWood", "table desk counter bench shelf"),
    ("workshop_co_ScrapWood04", "plywood wood wooden shack plank planks pallet lumber board boards"),
    ("workshop_co_ScrapCastleRubble", "rubble concrete cinder cinderblock brick column pillar"),
    ("workshop_co_ScrapMetal08", "girder girders beam beams"),
    ("workshop_co_ScrapBush", "flora plant bush shrub"),
    ("workshop_co_ScrapMetalVerySmall", "trash trashbin trashcan junk debris garbage pile"),
)
FALLBACK_RECIPE = "workshop_co_ScrapMetalVerySmall"


@dataclass(frozen=True)
class SourceObject:
    raw: int
    editor_id: str
    model: str


@dataclass(frozen=True)
class TargetBase:
    key: tuple[str, int]  # (lowercased plugin, object id)
    signature: str
    editor_id: str
    model: str


@dataclass
class ScrapTargets:
    """FO4 bases a scrap recipe creates, and every scrap recipe by EditorID (lowercased)."""
    recipe_of: dict[tuple[str, int], tuple[str, int]]
    bases: dict[tuple[str, int], TargetBase]
    recipes_by_editor_id: dict[str, tuple[str, int]]
    plugin_names: dict[str, str]  # lowercased -> as the load order spells it


def can_be_deleted(prps: bytes) -> bool:
    """FO76 PRPS rows are (actor value, value, curve table), 12 bytes each."""
    values = {}
    for offset in range(0, len(prps) - 11, 12):
        actor_value, value = struct.unpack_from("<If", prps, offset)
        values[actor_value] = value
    value = values.get(CAN_BE_DELETED_NEW, values.get(CAN_BE_DELETED_OLD))
    return value is not None and value > 0


def tokens(*names: str) -> set[str]:
    out = set()
    for name in names:
        stem = name.replace("\\", "/").rsplit("/", 1)[-1].rsplit(".", 1)[0]
        for part in re.findall(r"[A-Z]+(?![a-z])|[A-Z]?[a-z]+|[0-9]+", stem):
            word = part.lower()
            if not word.isdigit() and len(word) >= 3 and word not in STOP_WORDS:
                out.add(word)
    return out


def _editor_id(text: str) -> str:
    return re.sub(r"fo76$", "", text, flags=re.IGNORECASE).lower()


def _model(text: str) -> str:
    return text.lower().replace("/", "\\")


class RecipeMatcher:
    def __init__(self, targets: ScrapTargets):
        self.targets = targets
        scrappable = [targets.bases[key] for key in targets.recipe_of if key in targets.bases]
        self.by_editor_id: dict[str, TargetBase] = {}
        self.by_model: dict[str, TargetBase] = {}
        for base in scrappable:
            self.by_editor_id.setdefault(_editor_id(base.editor_id), base)
            self.by_model.setdefault(_model(base.model), base)
        self.words = {base.key: tokens(base.editor_id, base.model) for base in scrappable}
        counts = Counter(word for words in self.words.values() for word in words)
        self.unseen_weight = math.log(max(len(self.words), 1))
        self.weight = {word: math.log(len(self.words) / count) for word, count in counts.items()}
        self.rules = [(targets.recipes_by_editor_id.get(recipe.lower()), set(words.split()))
                      for recipe, words in CLASS_RULES]
        self.fallback = targets.recipes_by_editor_id.get(FALLBACK_RECIPE.lower())

    def match(self, item: SourceObject, signature: str) -> tuple[tuple[str, int], str] | None:
        """(recipe key, how it was found), or None when no recipe is available at all."""
        recipe_of = self.targets.recipe_of
        same = self.targets.bases.get(("fallout4.esm", item.raw & 0xFFFFFF))
        if (same and same.key in recipe_of and same.signature == signature
                and (_editor_id(same.editor_id) == _editor_id(item.editor_id) or _model(same.model) == _model(item.model))):
            return recipe_of[same.key], "id"
        if base := self.by_editor_id.get(_editor_id(item.editor_id)):
            return recipe_of[base.key], "editor_id"
        if item.model and (base := self.by_model.get(_model(item.model))):
            return recipe_of[base.key], "model"
        words = tokens(item.editor_id, item.model)
        best, score = self._closest(words)
        if score >= CONFIDENT_SIMILARITY:
            return recipe_of[best], "similar"
        for recipe, rule_words in self.rules:
            if recipe and words & rule_words:
                return recipe, "class"
        if score >= LOOSE_SIMILARITY:
            return recipe_of[best], "similar"
        return (self.fallback, "fallback") if self.fallback else None

    def _closest(self, words: set[str]) -> tuple[tuple[str, int] | None, float]:
        best, score = None, 0.0
        for key, other in self.words.items():
            shared = words & other
            if not shared:
                continue
            total = sum(self.weight.get(word, self.unseen_weight) for word in words | other)
            value = sum(self.weight[word] for word in shared) / total
            if value > score:
                best, score = key, value
        return best, score


def _text(subrecords, signature: str) -> str:
    return next((data.rstrip(b"\0").decode("latin-1") for name, data in subrecords if name == signature), "")


def read_source(native, handle: int) -> list[tuple[SourceObject, str]]:
    signatures = {row[4]: row[2] for row in native.plugin_handle_record_index_rows(handle, signatures=None, form_keys=None)}
    objects = []
    for raw in native.plugin_handle_record_form_ids_with_subrecords(handle, ["PRPS"]):
        subrecords = [(name, bytes(data)) for name, data, _ in native.plugin_handle_record_subrecords(handle, raw) or []]
        prps = next((data for name, data in subrecords if name == "PRPS"), b"")
        if can_be_deleted(prps):
            objects.append((SourceObject(raw, _text(subrecords, "EDID"), _text(subrecords, "MODL")), signatures[raw]))
    return objects


def read_targets(native, plugins: list[Path]) -> ScrapTargets:
    bases: dict[tuple[str, int], TargetBase] = {}
    lists: dict[tuple[str, int], list[tuple[str, int]]] = {}
    recipes: list[tuple[tuple[str, int], str, tuple[str, int]]] = []
    for path in plugins:
        handle = native.plugin_handle_load_index(str(path), game="fo4")
        try:
            masters = plugin_masters(native, handle)

            def key(raw: int) -> tuple[str, int]:
                index = raw >> 24
                return (masters[index] if index < len(masters) else path.name).lower(), raw & 0xFFFFFF

            def subrecords(raw: int):
                return [(name, bytes(data)) for name, data, _ in native.plugin_handle_record_subrecords(handle, raw) or []]

            signatures = {}
            for _, editor_id, signature, _, raw in native.plugin_handle_record_index_rows(handle, signatures=None, form_keys=None):
                signatures[raw] = signature
                if signature in ("FLST", "COBJ"):
                    rows = subrecords(raw)
                    if signature == "FLST":
                        lists[key(raw)] = [key(struct.unpack("<I", data)[0]) for name, data in rows if name == "LNAM"]
                        continue
                    filters = next((data for name, data in rows if name == "FNAM"), b"")
                    created = next((data for name, data in rows if name == "CNAM" and len(data) == 4), None)
                    if created and SCRAP_FILTER in [key(value) for value in struct.unpack(f"<{len(filters) // 4}I", filters)]:
                        recipes.append((key(raw), editor_id, key(struct.unpack("<I", created)[0])))
            for raw in native.plugin_handle_record_form_ids_with_subrecords(handle, ["MODL"]):
                rows = subrecords(raw)
                bases[key(raw)] = TargetBase(key(raw), signatures.get(raw, ""), _text(rows, "EDID"), _text(rows, "MODL"))
        finally:
            native.plugin_handle_close(handle)
    recipe_of: dict[tuple[str, int], tuple[str, int]] = {}
    for recipe, _, created in recipes:
        for base in lists.get(created, [created]):
            recipe_of.setdefault(base, recipe)
    return ScrapTargets(recipe_of, bases, {editor_id.lower(): recipe for recipe, editor_id, _ in recipes},
                        {path.name.lower(): path.name for path in plugins})


def build_document(source_plugin: str, masters: list[str], objects: list[tuple[SourceObject, str]],
                   targets: ScrapTargets) -> tuple[dict, Counter]:
    matcher = RecipeMatcher(targets)
    entries, tiers = {}, Counter()
    for item, signature in sorted(objects, key=lambda pair: pair[0].raw):
        found = matcher.match(item, signature)
        if found is None:
            tiers["unmatched"] += 1
            continue
        (plugin, object_id), tier = found
        index = item.raw >> 24
        owner = masters[index] if index < len(masters) else source_plugin
        entries[form_key(owner, item.raw & 0xFFFFFF)] = {
            "recipe": form_key(targets.plugin_names.get(plugin, plugin), object_id), "match": tier}
        tiers[tier] += 1
    return {"version": VERSION, "settings": {}, "objects": entries}, tiers


def emit_camp_scrap(source_plugin: Path, target_data_dir: Path, output_mod: Path) -> dict:
    source_plugin = Path(source_plugin)
    native = load_esp_native()
    targets_paths, missing = resolve_official_target_master_paths("fo4", target_data_dir=target_data_dir)
    targets = read_targets(native, targets_paths)
    handle = native.plugin_handle_load_index(str(source_plugin), game="fo76")
    try:
        objects = read_source(native, handle)
        masters = plugin_masters(native, handle)
    finally:
        native.plugin_handle_close(handle)
    document, tiers = build_document(source_plugin.name, masters, objects, targets)
    path = write_document(FEATURE, source_plugin.name, document, output_mod)
    return {"path": path, "objects": len(document["objects"]), "tiers": dict(tiers), "missing_masters": missing}
