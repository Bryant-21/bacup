"""Per-weapon diagnostics for the FNV/FO3 weapon conversion path."""
from __future__ import annotations

import json
import os
from dataclasses import asdict, dataclass, field
from typing import Any


@dataclass
class WeaponSlotDiagnostic:
    n: int
    omod_emitted: bool = False
    nif_diff_block_count: int = 0
    status: str = "absent"


@dataclass
class WeaponDiagnostic:
    weap_eid: str
    ammo_decision: str
    slots: list[WeaponSlotDiagnostic] = field(default_factory=list)
    anim_family: str = ""
    overlay_emitted: bool = False


def _diagnostic_store(orchestrator) -> dict[str, dict[str, Any]]:
    store = getattr(orchestrator, "_weapon_diagnostics", None)
    if store is None:
        store = {}
        orchestrator._weapon_diagnostics = store
    return store


def initialize_weapon_diagnostic(
    orchestrator,
    weap_eid: str,
    *,
    ammo_decision: str,
    populated_slots: set[int],
) -> None:
    """Seed slot/ammo diagnostics for a translated weapon."""
    weapon = _diagnostic_store(orchestrator).setdefault(
        weap_eid,
        {"ammo_decision": ammo_decision, "slots": {}},
    )
    weapon["ammo_decision"] = ammo_decision
    slots = weapon.setdefault("slots", {})
    for slot in (1, 2, 3):
        entry = slots.setdefault(slot, {"n": slot})
        if slot in populated_slots:
            entry.update(
                {
                    "omod_emitted": True,
                    "nif_diff_block_count": -1,
                    "status": "ok",
                }
            )
        else:
            entry.update(
                {
                    "omod_emitted": False,
                    "nif_diff_block_count": 0,
                    "status": "absent",
                }
            )


def update_weapon_slot_diagnostic(
    orchestrator,
    weap_eid: str,
    slot: int,
    *,
    status: str,
    omod_emitted: bool,
    nif_diff_block_count: int,
) -> None:
    """Record the final phase-3 outcome for one slot."""
    weapon = _diagnostic_store(orchestrator).setdefault(
        weap_eid,
        {"ammo_decision": "converted", "slots": {}},
    )
    slots = weapon.setdefault("slots", {})
    slots[slot] = {
        "n": slot,
        "omod_emitted": omod_emitted,
        "nif_diff_block_count": nif_diff_block_count,
        "status": status,
    }


def _form_key_variants(form_key: str) -> tuple[str, ...]:
    if ":" not in form_key:
        return (form_key,)
    left, right = form_key.split(":", 1)
    variants = [form_key]
    if "." in left:
        try:
            variants.append(f"{int(right, 16) & 0x00FFFFFF:06X}:{left}")
        except ValueError:
            pass
    elif "." in right:
        try:
            variants.append(f"{right}:{int(left, 16) & 0x00FFFFFF:06X}")
        except ValueError:
            pass
    return tuple(dict.fromkeys(variants))


def weapon_metadata_index(orchestrator) -> dict[str, dict[str, Any]]:
    cached = getattr(orchestrator, "_weapon_metadata_index", None)
    if cached is not None:
        return cached
    rust_run = getattr(orchestrator, "_rust_conversion_run", None)
    if rust_run is None:
        orchestrator._weapon_metadata_index = {}
        return {}
    form_keys = [
        str(getattr(node, "form_key", "") or "")
        for node in getattr(orchestrator.graph, "all_records", [])
        if orchestrator._record_type_signature(getattr(node, "record_type", "")) == "WEAP"
    ]
    if not form_keys:
        orchestrator._weapon_metadata_index = {}
        return {}
    from bacup_lib.native_runtime import load_native_module

    rows = load_native_module().conversion_run_weapon_metadata(rust_run.id, form_keys)
    index: dict[str, dict[str, Any]] = {}
    for row in rows:
        for key in _form_key_variants(str(row.get("source_form_key") or "")):
            index[key] = row
        editor_id = str(row.get("editor_id") or "")
        if editor_id:
            index[f"eid:{editor_id.casefold()}"] = row
    orchestrator._weapon_metadata_index = index
    return index


def _overlay_emitted(mod_path: str, overlay_relpath: str | None) -> bool:
    if not overlay_relpath:
        return False
    parts = [part for part in overlay_relpath.replace("\\", "/").split("/") if part]
    return os.path.isfile(os.path.join(mod_path, "data", *parts))


def collect_weapon_diagnostics(orchestrator) -> list[WeaponDiagnostic]:
    """Build the persisted diagnostics payload from orchestrator state."""
    from bacup_lib.animation.phases import _weapon_overlay_output_rel
    from bacup_lib.animation.weapon_family_classifier import classify_weapon

    diagnostics: list[WeaponDiagnostic] = []
    stored = dict(getattr(orchestrator, "_weapon_diagnostics", {}) or {})

    for node in getattr(orchestrator.graph, "all_records", []):
        record_sig = orchestrator._record_type_signature(getattr(node, "record_type", ""))
        if record_sig != "WEAP":
            continue

        weap_eid = str(getattr(node, "editor_id", "") or "")
        if not weap_eid:
            continue

        metadata = weapon_metadata_index(orchestrator).get(node.form_key) or weapon_metadata_index(
            orchestrator
        ).get(f"eid:{weap_eid.casefold()}", {})
        anim_type = str(metadata.get("anim_type") or "")
        anim_family, _keep_bones, _remap = classify_weapon(weap_eid, anim_type)
        overlay_relpath = _weapon_overlay_output_rel(orchestrator, weap_eid)

        stored_weapon = stored.get(weap_eid, {})
        stored_slots = stored_weapon.get("slots", {}) if isinstance(stored_weapon, dict) else {}
        slot_diagnostics: list[WeaponSlotDiagnostic] = []
        for slot in (1, 2, 3):
            entry = stored_slots.get(slot) or stored_slots.get(str(slot)) or {
                "n": slot,
                "omod_emitted": False,
                "nif_diff_block_count": 0,
                "status": "absent",
            }
            slot_diagnostics.append(
                WeaponSlotDiagnostic(
                    n=int(entry.get("n", slot)),
                    omod_emitted=bool(entry.get("omod_emitted", False)),
                    nif_diff_block_count=int(entry.get("nif_diff_block_count", 0)),
                    status=str(entry.get("status", "absent")),
                )
            )

        ammo_decision = str(
            stored_weapon.get("ammo_decision")
            if isinstance(stored_weapon, dict)
            else ""
        ) or str(metadata.get("ammo_decision") or "unknown")

        diagnostics.append(
            WeaponDiagnostic(
                weap_eid=weap_eid,
                ammo_decision=ammo_decision,
                slots=slot_diagnostics,
                anim_family=anim_family,
                overlay_emitted=_overlay_emitted(
                    orchestrator.mod_path,
                    overlay_relpath if isinstance(overlay_relpath, str) else None,
                ),
            )
        )

    return diagnostics


def write_weapon_conversion_report(
    mod_path: str,
    diagnostics: list[WeaponDiagnostic],
) -> str:
    """Write ``weapon_conversion_report.json`` to ``mod_path``."""
    os.makedirs(mod_path, exist_ok=True)
    output_path = os.path.join(mod_path, "weapon_conversion_report.json")
    payload = {
        "version": 1,
        "weapons": [asdict(diagnostic) for diagnostic in diagnostics],
    }
    with open(output_path, "w", encoding="utf-8") as stream:
        json.dump(payload, stream, indent=2)
    return output_path
