"""Thin loader for the conversion_native umbrella submodule.

The Rust conversion boundary is intentionally dict-free. This module preserves
the older Python-facing shape for small control/status payloads while keeping
record conversion and mutation logic in Rust.
"""
from __future__ import annotations

import json
from importlib import import_module
import os
from typing import Any

_NATIVE: Any | None = None


_STATS_KEYS = (
    "records_translated",
    "records_vanilla_remapped",
    "records_dropped",
    "records_deferred",
    "records_failed",
)

_SIG_STATS_KEYS = (
    "seen",
    "translated",
    "vanilla_remapped",
    "dropped",
    "deferred",
    "failed",
)

_FNV_KEYS = (
    "translated_scripts",
    "translated_quests",
    "translated_scenes",
    "translated_infos",
    "dialogue_groups",
    "records_written",
    "records_failed",
    "psc_files_written",
    "psc_files_skipped",
    "skipped_records",
    "lip_regeneration_needed",
    "vmad_intents",
    "vmad_attached_in_rust",
    "generated_psc_classes",
    "voice_manifest",
    "quest_runtime_components",
)


def _json_default(value: Any) -> Any:
    if isinstance(value, bytes):
        return list(value)
    raise TypeError(f"Object of type {type(value).__name__} is not JSON serializable")


def _config_json(config: Any | None) -> str:
    return json.dumps(config or {}, default=_json_default, separators=(",", ":"))


def _configure_native_resources() -> None:
    if "CREATION_LIB_RESOURCE_DIR" in os.environ:
        return
    try:
        from creation_lib.paths import get_resource_dir

        os.environ["CREATION_LIB_RESOURCE_DIR"] = str(get_resource_dir())
    except Exception:
        pass


def _stats_from_raw(raw: Any) -> dict[str, Any]:
    stats = dict(zip(_STATS_KEYS, raw[:5], strict=True))
    stats["by_signature"] = {
        sig: dict(zip(_SIG_STATS_KEYS, values, strict=True))
        for sig, *values in raw[5]
    }
    return stats


def _phase_report_from_raw(raw: Any) -> dict[str, Any]:
    legacy_keys = (
        "records_changed",
        "records_added",
        "records_dropped",
        "assets_written",
        "warnings",
        "elapsed_ms",
        "items_failed",
    )
    if len(raw) == 7:
        report = dict(zip(legacy_keys, raw, strict=True))
        report["records_vanilla_remapped"] = 0
        report["records_deferred"] = 0
        return report
    if len(raw) == 9:
        report = dict(zip(legacy_keys, raw[:7], strict=True))
        report["records_vanilla_remapped"] = raw[7]
        report["records_deferred"] = raw[8]
        return report
    raise ValueError(f"PhaseReport raw tuple must contain 7 or 9 fields, got {len(raw)}")


def _phase_event_from_raw(raw: Any) -> dict[str, Any]:
    kind = raw[0]
    if kind == "started":
        return {"kind": "started", "phase": raw[1]}
    if kind == "progress":
        event = {"kind": "progress", "phase": raw[1], "current": raw[2], "total": raw[3]}
        if raw[4] is not None:
            event["item"] = raw[4]
        return event
    if kind == "log":
        return {"kind": "log", "phase": raw[1], "level": raw[2], "message": raw[3]}
    if kind == "completed":
        return {"kind": "completed", "phase": raw[1], "report": _phase_report_from_raw(raw[2])}
    if kind == "stage_started":
        return {"kind": "stage_started", "stage": raw[1]}
    if kind == "stage_completed":
        return {
            "kind": "stage_completed",
            "stage": raw[1],
            "items_done": raw[2],
            "items_failed": raw[3],
            "elapsed_ms": raw[4],
        }
    if kind == "stage_failed":
        return {"kind": "stage_failed", "stage": raw[1], "message": raw[2]}
    raise ValueError(f"unknown phase event kind: {kind!r}")


def _weapon_metadata_from_raw(raw: Any) -> dict[str, Any]:
    if len(raw) not in {9, 18}:
        raise ValueError(
            f"weapon metadata raw tuple must contain 9 or 18 fields, got {len(raw)}"
        )
    row = {
        "source_form_key": raw[0],
        "editor_id": raw[1],
        "base_model": raw[2],
        "model_mod1": raw[3],
        "model_mod2": raw[4],
        "model_mod3": raw[5],
        "weapon_role": raw[6],
        "ammo_decision": raw[7],
        "anim_type": raw[8],
    }
    if len(raw) == 9:
        return row

    status = raw[9]
    source_family = raw[12]
    rejection_reason = raw[17]
    row.update(
        {
            "status": status,
            "disposition": status,
            "policy": raw[10],
            "target_profile": raw[11],
            "source_family": source_family,
            "provenance": source_family,
            "world_model": raw[13],
            "first_person_model": raw[14],
            "first_person_resolution": raw[15],
            "lowering_drops": list(raw[16]),
            "rejection_reason": rejection_reason,
            "reason_code": rejection_reason or ("admitted" if status == "admitted" else ""),
        }
    )
    return row


def _fnv_result_from_raw(raw: Any) -> dict[str, Any]:
    if len(raw) == len(_FNV_KEYS) - 3:
        legacy_keys = tuple(
            key for key in _FNV_KEYS
            if key
            not in {
                "generated_psc_classes",
                "voice_manifest",
                "quest_runtime_components",
            }
        )
        result = dict(zip(legacy_keys, raw, strict=True))
        result["generated_psc_classes"] = []
        result["voice_manifest"] = {"version": 1, "entries": []}
        result["quest_runtime_components"] = []
    elif len(raw) == len(_FNV_KEYS) - 2:
        legacy_keys = tuple(
            key
            for key in _FNV_KEYS
            if key not in {"voice_manifest", "quest_runtime_components"}
        )
        result = dict(zip(legacy_keys, raw, strict=True))
        result["voice_manifest"] = {"version": 1, "entries": []}
        result["quest_runtime_components"] = []
    elif len(raw) == len(_FNV_KEYS) - 1:
        legacy_keys = tuple(
            key for key in _FNV_KEYS if key != "quest_runtime_components"
        )
        result = dict(zip(legacy_keys, raw, strict=True))
        result["quest_runtime_components"] = []
    else:
        result = dict(zip(_FNV_KEYS, raw, strict=True))
    if isinstance(result["voice_manifest"], str):
        result["voice_manifest"] = json.loads(result["voice_manifest"])
    if isinstance(result["quest_runtime_components"], str):
        result["quest_runtime_components"] = json.loads(
            result["quest_runtime_components"]
        )
    result["vmad_intents"] = [
        {"target_form_key": target_form_key, "script_class_name": script_class_name}
        for target_form_key, script_class_name in result["vmad_intents"]
    ]
    return result


class _ConversionNativeProxy:
    def __init__(self, raw: Any) -> None:
        self._raw = raw

    def __getattr__(self, name: str) -> Any:
        return getattr(self._raw, name)

    def conversion_diagnose_navmesh_links(
        self, plugin_path: str, game: str, workers: int | None = None
    ) -> dict[str, Any]:
        keys = (
            "navmeshes_seen",
            "navmeshes_touched",
            "bad_internal_links",
            "linked_edge_vertex_mismatches",
            "opposite_normal_linked_pairs",
            "missing_internal_links",
            "same_direction_internal_edges",
            "ambiguous_local_edges",
            "external_links_added",
            "missing_external_links",
            "ambiguous_external_edges",
            "external_link_caps_hit",
            "winding_conflicts",
            "residual_warning_count",
        )
        args = (plugin_path, game)
        raw = (
            self._raw.conversion_diagnose_navmesh_links(*args)
            if workers is None
            else self._raw.conversion_diagnose_navmesh_links(*args, workers)
        )
        return dict(zip(keys, raw, strict=True))

    def conversion_merge_sources(self, opts: dict[str, Any]) -> dict[str, Any]:
        return json.loads(self._raw.conversion_merge_sources(_config_json(opts)))

    def conversion_run_create_from_paths(
        self,
        source_game: str,
        target_game: str,
        source_plugin_path: str | None,
        target_plugin_name: str | None,
        target_plugin_path: str | None,
        master_plugin_paths: list[str],
        source_strings_dir: str | None,
        config: dict[str, Any] | None = None,
    ) -> int:
        return self._raw.conversion_run_create_from_paths(
            source_game,
            target_game,
            source_plugin_path,
            target_plugin_name,
            target_plugin_path,
            master_plugin_paths,
            source_strings_dir,
            _config_json(config),
        )

    def conversion_pipeline_run(self, plan_json: str) -> dict[str, Any]:
        stages, elapsed_ms, counters = self._raw.conversion_pipeline_run(plan_json)
        return {
            "stages": [tuple(row[:5]) for row in stages],
            "phase_reports": {
                row[0]: _phase_report_from_raw(row[5])
                for row in stages if len(row) > 5 and row[5] is not None
            },
            "elapsed_ms": elapsed_ms,
            "counters": dict(counters),
        }

    def conversion_run_sync_cell_regions_from_source(
        self, run_id: int, source_worldspace_editor_id: str, target_worldspace_editor_id: str
    ) -> dict[str, Any]:
        return json.loads(
            self._raw.conversion_run_sync_cell_regions_from_source(
                run_id, source_worldspace_editor_id, target_worldspace_editor_id
            )
        )

    def conversion_run_sync_cell_locations_from_lctn(self, run_id: int) -> dict[str, Any]:
        return json.loads(self._raw.conversion_run_sync_cell_locations_from_lctn(run_id))

    def conversion_run_drain_decisions(self, run_id: int) -> list[dict[str, str]]:
        return [
            {"kind": kind, "message": message}
            for kind, message in self._raw.conversion_run_drain_decisions(run_id)
        ]

    def conversion_run_translate_all(self, run_id: int, progress_callback: Any = None) -> dict[str, Any]:
        return _stats_from_raw(self._raw.conversion_run_translate_all(run_id, progress_callback))

    def conversion_run_preflight_legacy_packs(self, run_id: int) -> None:
        self._raw.conversion_run_preflight_legacy_packs(run_id)

    def conversion_run_translate_records(
        self,
        run_id: int,
        form_keys: list[str],
        progress_callback: Any = None,
    ) -> dict[str, Any]:
        return _stats_from_raw(
            self._raw.conversion_run_translate_records(run_id, form_keys, progress_callback)
        )

    def conversion_run_fnv_legacy_scripting_from_run(
        self,
        run_id: int,
        mod_prefix: str,
        source_plugin: str,
        mod_path: str,
    ) -> dict[str, Any]:
        return _fnv_result_from_raw(
            self._raw.conversion_run_fnv_legacy_scripting_from_run(
                run_id, mod_prefix, source_plugin, mod_path
            )
        )

    def conversion_run_fnv_reconcile_compiled_scripts(
        self, run_id: int, compiled_scripts: list[dict[str, Any]]
    ) -> dict[str, Any]:
        return dict(
            self._raw.conversion_run_fnv_reconcile_compiled_scripts(
                run_id, compiled_scripts
            )
        )

    def conversion_run_form_key_map(self, run_id: int, source_form_keys: list[str]) -> dict[str, str]:
        return dict(self._raw.conversion_run_form_key_map(run_id, source_form_keys))

    def conversion_run_target_form_keys(self, run_id: int, source_form_keys: list[str]) -> list[str | None]:
        return list(self._raw.conversion_run_target_form_keys(run_id, source_form_keys))

    def conversion_run_weapon_metadata(self, run_id: int, source_form_keys: list[str]) -> list[dict[str, Any]]:
        return [
            _weapon_metadata_from_raw(row)
            for row in self._raw.conversion_run_weapon_metadata(run_id, source_form_keys)
        ]

    def conversion_run_apply_registry_mappings(self, run_id: int, mappings: dict[str, str]) -> int:
        return self._raw.conversion_run_apply_registry_mappings(
            run_id,
            [[source, target] for source, target in mappings.items()],
        )

    def conversion_run_phase(self, run_id: int, name: str, params: dict[str, Any]) -> dict[str, Any]:
        return _phase_report_from_raw(
            self._raw.conversion_run_phase(run_id, name, json.dumps(params, default=_json_default))
        )

    def conversion_run_drain_events(self, run_id: int, max: int = 256) -> list[dict[str, Any]]:
        return [
            _phase_event_from_raw(event)
            for event in self._raw.conversion_run_drain_events(run_id, max)
        ]

    def conversion_run_repair_placed_child_refs(self, run_id: int) -> dict[str, int]:
        return {"records_changed": self._raw.conversion_run_repair_placed_child_refs(run_id)}

    def conversion_run_synthesize_encounter_zones(
        self, run_id: int, identity_resolve: bool = False
    ) -> dict[str, int]:
        return {
            "records_changed": self._raw.conversion_run_synthesize_encounter_zones(
                run_id, identity_resolve
            )
        }

    def conversion_run_synthesize_sky_regions(self, run_id: int) -> dict[str, int]:
        return {
            "records_changed": self._raw.conversion_run_synthesize_sky_regions(run_id)
        }

    def conversion_run_synthesize_vendor_dialogue(self, run_id: int) -> dict[str, int]:
        return {
            "records_changed": self._raw.conversion_run_synthesize_vendor_dialogue(
                run_id
            )
        }


def load_native_module() -> Any:
    global _NATIVE
    if _NATIVE is not None:
        return _NATIVE
    _configure_native_resources()
    umbrella = import_module("bacup_lib._native")
    mod = getattr(umbrella, "conversion_native", None)
    if mod is None:
        try:
            mod = import_module("bacup_lib._native.conversion_native")
        except ImportError as exc:
            raise RuntimeError("conversion_native not loaded — rebuild BACUP umbrella") from exc
    _NATIVE = _ConversionNativeProxy(mod)
    return _NATIVE
