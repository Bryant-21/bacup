"""Emit source-derived Bullet Storm and Onslaught bindings for Tales."""
from __future__ import annotations

import hashlib
import json
from pathlib import Path
from typing import Protocol

from bacup_lib.esp_native_runtime import load_esp_native
from bacup_lib.reputation_catalog import NativeReader, NativeResolver, plugin_masters

SCHEMA_VERSION = 1
OUTPUT = Path("F4SE/Plugins/B21_TalesFromAppalachia/combat_perks.json")

CARDS = (
    ("bullet_storm_1", "HeavyGunner01", 1),
    ("bullet_storm_2", "HeavyGunner02", 2),
    ("bullet_storm_3", "HeavyGunner03", 3),
    ("bringing_the_big_guns", "HeavyGunnerMaster01", 1),
    ("bear_arms", "BearArms01", 1),
    ("lock_and_load", "LockAndLoad01", 1),
    ("gunslinger_expert", "GunslingerExpert01", 1),
    ("gunslinger_master", "GunslingerMaster01", 1),
    ("guerrilla_expert", "GuerrillaExpert01", 1),
    ("guerrilla_master", "GuerrillaMaster01", 1),
)

ITEMS = (
    ("action_hero", "mod_Custom_TheActionHero", 0),
    ("valkyrie", "RD01_Mod_Custom_Valkyrie_CustomName", 0),
    ("final_word", "mod_Custom_FinalWord", 0),
    ("foundations_vengeance", "E08B_mod_Custom_FoundationsVengeance", 0),
    ("furious", "mod_Legendary_Weapon1_DmgConsecutiveHits", 9),
    ("elders_mark", "EldersMark_Perk", 0),
    ("ticket_to_revenge", "custom_TickettoRevenge_Perk", 0),
    ("whacker_smacker", "E09B_mod_Custom_WhackerSmacker", 0),
    ("splinter", "P62_Mod_Custom_Splinter_SpecialEffect", 10),
    ("pounders", "mod_Legendary_Weapon4_Melee_Pounders", 10),
)


class Reader(Protocol):
    def lookup(self, signature: str, editor_id: str) -> tuple[int, dict[str, list]]: ...


class Resolver(Protocol):
    def resolve(self, signature: str, editor_id: str, source_id: int) -> str | None: ...


class ConvertedResolver:
    def __init__(self, native, plugin: str, handle: int, masters: list[str]):
        self.native, self.plugin, self.handle = native, plugin, handle
        self.by_editor_id = NativeResolver(native, plugin, handle, masters)

    def resolve(self, signature: str, editor_id: str, source_id: int) -> str | None:
        exact = self.by_editor_id.resolve(signature, editor_id)
        if exact:
            return exact
        collision = self.by_editor_id.resolve(signature, editor_id + "fo76")
        if collision:
            return collision
        try:
            raw = self.native.plugin_handle_inspect_record(self.handle, source_id)
            if not raw:
                return None
            inspected = json.loads(raw)
        except (RuntimeError, TypeError, ValueError):
            return None
        found_signature = inspected.get("signature")
        found_editor_id = inspected.get("record", {}).get("eid") or ""
        if found_signature != signature or not found_editor_id.casefold().startswith(editor_id.casefold()):
            return None
        return f"{self.plugin}:{source_id:06X}"


def source_form(plugin: str, object_id: int) -> str:
    return f"{plugin}:{object_id:06X}"


def build_catalog(reader: Reader, resolver: Resolver, output_plugin: str,
                  source_name: str, source_sha256: str) -> dict:
    cards = {}
    items = {}
    missing = []
    missing_source = []

    for key, editor_id, rank in CARDS:
        try:
            object_id, _fields = reader.lookup("PERK", editor_id)
        except (KeyError, ValueError):
            missing_source.append(f"PERK:{editor_id}")
            cards[key] = {"form": None, "source": None, "editor_id": editor_id,
                          "rank": rank, "available": False}
            continue
        target = resolver.resolve("PERK", editor_id, object_id)
        if not target:
            missing.append(f"PERK:{editor_id}")
        cards[key] = {"form": target, "source": source_form(source_name, object_id),
                      "editor_id": editor_id, "rank": rank, "available": target is not None}

    for key, editor_id, maximum in ITEMS:
        signature = "PERK" if key in {"elders_mark", "ticket_to_revenge"} else "OMOD"
        try:
            object_id, _fields = reader.lookup(signature, editor_id)
        except (KeyError, ValueError):
            missing_source.append(f"{signature}:{editor_id}")
            items[key] = {"form": None, "source": None, "editor_id": editor_id,
                          "maximum_stacks": maximum, "available": False}
            continue
        target = resolver.resolve(signature, editor_id, object_id)
        if not target:
            missing.append(f"{signature}:{editor_id}")
        items[key] = {"form": target, "source": source_form(source_name, object_id),
                      "editor_id": editor_id, "maximum_stacks": maximum,
                      "available": target is not None}

    return {
        "schema_version": SCHEMA_VERSION,
        "source": {"plugin": source_name, "sha256": source_sha256},
        "output_plugin": output_plugin,
        "available": not missing and not missing_source,
        "cards": cards,
        "items": items,
        "damage_ownership": {
            "owner": "B21_TalesFromAppalachia",
            "effects": ["furious", "splinter"],
            "percent_per_stack": 1,
            "converted_entry_points": [189, 190],
            "converted_entry_points_active": False,
        },
        "runtime_supported": {
            "generic_projectile_damage": ["bullet_storm", "furious", "splinter"],
            "stack_rules": ["bringing_the_big_guns", "lock_and_load", "final_word", "foundations_vengeance"],
        },
        "stack_contract": {
            "bullet_storm_ammo_per_stack": 30,
            "onslaught_loss_per_second": 1,
            "onslaught_reverse_gain_per_second": 1,
        },
        "missing_converted": missing,
        "missing_source": missing_source,
    }


def emit_combat_perk_catalog(source_plugin: Path, converted_plugin: Path, output_mod: Path,
                             *, source_sha256: str | None = None) -> dict:
    source_plugin, converted_plugin, output_mod = map(Path, (source_plugin, converted_plugin, output_mod))
    for path in (source_plugin, converted_plugin):
        if not path.is_file():
            raise FileNotFoundError(f"Combat perk catalog input is missing: {path}")
    native = load_esp_native()
    handles = []
    try:
        source = native.plugin_handle_load_index(str(source_plugin), game="fo76")
        converted = native.plugin_handle_load_index(str(converted_plugin), game="fo4")
        handles += [source, converted]
        if source_sha256 is None:
            with source_plugin.open("rb") as stream:
                source_sha256 = hashlib.file_digest(stream, "sha256").hexdigest()
        catalog = build_catalog(
            NativeReader(native, source),
            ConvertedResolver(native, converted_plugin.name, converted, plugin_masters(native, converted)),
            converted_plugin.name, source_plugin.name, source_sha256,
        )
    finally:
        for handle in reversed(handles):
            native.plugin_handle_close(handle)
    destination = output_mod / OUTPUT
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(json.dumps(catalog, indent=2, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    return catalog
