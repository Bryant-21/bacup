"""Phase 3-post weapon attachment surgery for FNV/FO3 weapon conversions."""
from __future__ import annotations

import os
import tempfile
from dataclasses import dataclass, field

from bacup_lib.attach_point_keyword_naming import (
    association_keyword_eid,
    attachment_nif_relpath,
)
from bacup_lib.weapon_report import (
    update_weapon_slot_diagnostic,
    weapon_metadata_index,
)
from bacup_lib.paths import apply_asset_prefix_for_root

from creation_lib.nif import native_runtime as nif_native_runtime

_FNV_LIKE_IDS = frozenset({"fnv", "fo3"})


def is_fnv_source(profile) -> bool:
    """Return True when the profile is one of the FNV-like sources."""
    return getattr(profile, "id", None) in _FNV_LIKE_IDS


@dataclass
class SurgeryResult:
    attachments_written: int = 0
    slots_retracted: int = 0
    logs: list[tuple[str, str]] = field(default_factory=list)


def _normalize_asset_key(path: str) -> str:
    return path.replace("\\", "/").lower()


def _root_name(payload: dict) -> str:
    header = payload.get("header") or {}
    root_ids = [
        root_id
        for root_id in (header.get("footer_roots") or [])
        if isinstance(root_id, int) and root_id >= 0
    ]
    root_id = root_ids[0] if root_ids else 0
    blocks = payload.get("blocks") or []
    if not (0 <= root_id < len(blocks)):
        return ""
    return str(((blocks[root_id].get("fields") or {}).get("Name")) or "")


def _retract_slot(orchestrator, weap_eid: str, slot: int) -> list[str]:
    update_weapon_slot_diagnostic(
        orchestrator,
        weap_eid,
        slot,
        status="retracted",
        omod_emitted=False,
        nif_diff_block_count=0,
    )
    eids = list(orchestrator._summary.weapon_slot_records.pop((weap_eid, slot), []))
    drop_association = not any(
        owner_eid == weap_eid
        for owner_eid, _slot in orchestrator._summary.weapon_slot_records
    )
    retract_set = set(eids)
    if drop_association:
        retract_set.add(association_keyword_eid(orchestrator.mod_prefix, weap_eid))
    return list(retract_set)


def run_weapon_attachment_surgery(orchestrator) -> SurgeryResult:
    result = SurgeryResult()
    if not is_fnv_source(getattr(orchestrator, "_source_profile", None)):
        return result

    source_profile = getattr(orchestrator, "_source_profile", None)
    asset_prefix = getattr(source_profile, "asset_prefix", "") or ""
    asset_lookup = {
        _normalize_asset_key(asset.source_path): asset
        for asset in getattr(orchestrator.graph, "all_assets", [])
        if getattr(asset, "asset_type", "") == "nif"
    }

    for node in getattr(orchestrator.graph, "all_records", []):
        if str(getattr(node, "record_type", "")).upper() != "WEAP":
            continue
        metadata = weapon_metadata_index(orchestrator).get(node.form_key) or weapon_metadata_index(
            orchestrator
        ).get(f"eid:{str(node.editor_id).casefold()}", {})
        base_source = metadata.get("base_model")
        if not isinstance(base_source, str) or not base_source.strip():
            continue

        slot_numbers = [
            slot for slot in (1, 2, 3) if (node.editor_id, slot) in orchestrator._summary.weapon_slot_records
        ]
        if not slot_numbers:
            continue

        base_output_rel = apply_asset_prefix_for_root(base_source, source_profile, "Meshes")
        base_output_path = os.path.join(orchestrator.mod_path, "data", base_output_rel)
        if not os.path.isfile(base_output_path):
            for slot in slot_numbers:
                _retract_slot(orchestrator, node.editor_id, slot)
                result.slots_retracted += 1
            result.logs.append(
                ("ERROR", f"{node.editor_id}: base NIF missing for surgery: {base_output_rel}")
            )
            continue

        base_role = str(metadata.get("weapon_role") or "")
        if base_role == "gun":
            try:
                payload = nif_native_runtime.load_nif_raw(base_output_path)
            except Exception as exc:
                for slot in slot_numbers:
                    _retract_slot(orchestrator, node.editor_id, slot)
                    result.slots_retracted += 1
                result.logs.append(
                    ("ERROR", f"{node.editor_id}: failed to read converted base NIF: {exc}")
                )
                continue
            if _root_name(payload) != "Weapon":
                for slot in slot_numbers:
                    _retract_slot(orchestrator, node.editor_id, slot)
                    result.slots_retracted += 1
                result.logs.append(
                    ("ERROR", f"{node.editor_id}: converted base root is not named Weapon")
                )
                continue

        for slot in slot_numbers:
            sibling_source = metadata.get(f"model_mod{slot}")
            if not isinstance(sibling_source, str) or not sibling_source.strip():
                continue
            sibling_asset = asset_lookup.get(_normalize_asset_key(sibling_source))
            if sibling_asset is None or not sibling_asset.resolved_path:
                _retract_slot(orchestrator, node.editor_id, slot)
                result.slots_retracted += 1
                result.logs.append(
                    ("ERROR", f"{node.editor_id} slot {slot}: source combo NIF missing")
                )
                continue

            attachment_rel = apply_asset_prefix_for_root(
                attachment_nif_relpath(orchestrator.mod_prefix, node.editor_id, slot),
                source_profile,
                "Meshes",
            )
            attachment_path = os.path.join(orchestrator.mod_path, "data", attachment_rel)
            role = str(metadata.get("weapon_role") or "")

            with tempfile.TemporaryDirectory(prefix="weapon_combo_") as temp_dir:
                temp_sibling = os.path.join(temp_dir, f"{node.editor_id}_slot{slot}.nif")
                try:
                    report = nif_native_runtime.convert_nif_file_raw(
                        sibling_asset.resolved_path,
                        temp_sibling,
                        orchestrator.source_game,
                        orchestrator.target_game,
                        None,
                        {
                            "source_path": sibling_asset.source_path,
                            "asset_prefix": asset_prefix,
                            "addon_index_map": dict(getattr(orchestrator, "_addon_index_map", {})),
                            **({"weapon_role": role} if role else {}),
                        },
                    )
                    if not report.get("supported"):
                        raise RuntimeError("; ".join(report.get("errors", [])) or "unsupported")
                    extraction = nif_native_runtime.extract_attachment_raw(
                        base_output_path,
                        temp_sibling,
                        slot,
                        attachment_path,
                        "Weapon",
                    )
                except Exception as exc:
                    _retract_slot(orchestrator, node.editor_id, slot)
                    result.slots_retracted += 1
                    result.logs.append(
                        ("ERROR", f"{node.editor_id} slot {slot}: attachment surgery failed: {exc}")
                    )
                    continue

            blocks_copied = int(extraction.get("blocks_copied", 0))
            if blocks_copied <= 0:
                _retract_slot(orchestrator, node.editor_id, slot)
                result.slots_retracted += 1
                result.logs.append(
                    ("WARN", f"{node.editor_id} slot {slot}: empty diff; retracting slot records")
                )
                continue

            update_weapon_slot_diagnostic(
                orchestrator,
                node.editor_id,
                slot,
                status="ok",
                omod_emitted=True,
                nif_diff_block_count=blocks_copied,
            )
            result.attachments_written += 1
            result.logs.append(
                ("INFO", f"{node.editor_id} slot {slot}: wrote attachment {attachment_rel}")
            )

    return result
