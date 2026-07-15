"""FormKey remapping engine for cross-game conversion.

Handles three strategies:
- vanilla_remap: record exists in target game, use its FormKey
- source_id_preserved: record is new to the mod, keep source object ID
- new_allocation: record is new to the mod, assign sequential FormKey

Persists all decisions in formkey_map.json for incremental conversion support.
"""
from __future__ import annotations

import json
import logging
import os
import re
from typing import Any

from creation_lib.esp.record_types import record_type_signature

_log = logging.getLogger("conversion.formkey_mapper")

# Matches FormKey strings: 2-6 hex digits, colon, plugin name ending in .esm/.esp/.esl
_FK_PATTERN = re.compile(r"^[0-9A-Fa-f]{2,6}:.+\.(esm|esp|esl)$")

_FIRST_ALLOCATION_ID = 0x000800

# Record types that should ALWAYS auto-vanilla-remap when an EditorID
# match exists in the target game, regardless of ``use_base_game_assets``.
# These are game-system / leaf-ish records (animation keywords, material
# types, sound categories, impact tables, etc.) that mod authors never
# legitimately clone — the mod just references the existing vanilla copy.
# Cloning them produces bloated output and creates broken references
# whenever a sub-field points at source-game-only data with no target
# equivalent (xEdit reports these as "Found a NULL reference" errors).
#
# Records NOT in this set (Weapons, Armors, NPCs, ObjectModifications,
# etc.) still respect ``use_base_game_assets`` so creature conversions
# can ship full clones of their support records.
_ALWAYS_VANILLA_REMAP_TYPES: frozenset[str] = frozenset({
    "Keywords",
    "ImpactDataSets",
    "Impacts",
    "MaterialTypes",
    "EquipTypes",
    "VoiceTypes",
    "BodyParts",
    "AnimationSoundTagSets",
    "MovementTypes",
    "SoundCategories",
    "SoundOutputModels",
    "SoundKeywordMappings",
    "SoundMarkers",
    "AimModels",
    "Zooms",
    "ZoomData",
    "InstanceNamingRules",
    "AttachParentSlots",
    "DefaultObjects",
    "Globals",
    "GameSettings",
    "Transforms",
    "Colors",
    "ImageSpaces",
    "ImageSpaceAdapters",
    "MagicEffects",
    "Layers",
})

_ALWAYS_VANILLA_REMAP_SIGNATURES: frozenset[str] = frozenset(
    record_type_signature(record_type)
    for record_type in _ALWAYS_VANILLA_REMAP_TYPES
)


def _always_vanilla_remap(record_type: str) -> bool:
    return (
        record_type in _ALWAYS_VANILLA_REMAP_TYPES
        or record_type_signature(record_type) in _ALWAYS_VANILLA_REMAP_SIGNATURES
    )


class FormKeyMapper:
    """Map source-game FormKeys to target-game FormKeys."""

    def __init__(
        self,
        mod_name: str,
        target_game: str,
        target_loader,  # RecordLoader for target game DB
        mod_path: str,
        use_base_game_assets: bool = True,
        preserve_source_ids: bool = True,
        output_plugin_extension: str = ".esp",
        target_master_handles: list | None = None,
    ):
        self.mod_name = mod_name
        self.target_game = target_game
        self._target_loader = target_loader
        self._target_master_handles = list(target_master_handles or [])
        self._mod_path = mod_path
        self._use_base_game = use_base_game_assets
        self._preserve_source_ids = preserve_source_ids
        self._output_plugin_extension = self._normalize_plugin_extension(
            output_plugin_extension
        )
        self._output_plugin_name = f"{self.mod_name}{self._output_plugin_extension}"
        self._next_id = _FIRST_ALLOCATION_ID
        self._mappings: dict[str, dict] = {}
        self._local_object_ids: set[str] = set()
        self._target_handle_eid_rows_cache: dict[int, list[dict[str, str]]] = {}

        # Load existing map if present (incremental conversion)
        self._load_existing()
        self._rebuild_local_object_id_index()

    def _load_existing(self) -> None:
        map_path = os.path.join(self._mod_path, "formkey_map.json")
        if not os.path.isfile(map_path):
            return
        try:
            with open(map_path, encoding="utf-8") as f:
                data = json.load(f)
            self._mappings = data.get("mappings", {})
            next_id_str = data.get("next_id", "")
            if next_id_str:
                self._next_id = int(next_id_str, 16)
            _log.info(
                "Loaded existing formkey_map.json: %d mappings, next_id=%s",
                len(self._mappings), next_id_str,
            )
        except (json.JSONDecodeError, ValueError, KeyError) as e:
            _log.warning("Failed to load formkey_map.json: %s", e)

    @staticmethod
    def _normalize_plugin_extension(extension: str) -> str:
        ext = str(extension or ".esp").strip().lower()
        if not ext.startswith("."):
            ext = f".{ext}"
        if ext not in {".esm", ".esp", ".esl"}:
            return ".esp"
        return ext

    def _rebuild_local_object_id_index(self) -> None:
        self._local_object_ids.clear()
        for mapping in self._mappings.values():
            fk = str(mapping.get("new_formkey", ""))
            object_id, plugin = self._split_formkey(fk)
            if object_id and plugin.lower() == self._output_plugin_name.lower():
                self._local_object_ids.add(object_id)
        while f"{self._next_id:06X}" in self._local_object_ids:
            self._next_id += 1

    @staticmethod
    def _split_formkey(formkey: str) -> tuple[str, str]:
        if not isinstance(formkey, str) or ":" not in formkey:
            return "", ""
        object_id, plugin = formkey.split(":", 1)
        try:
            normalized = f"{int(object_id, 16):06X}"
        except ValueError:
            return "", plugin
        return normalized, plugin

    def _allocate_local_formkey(self) -> str:
        while f"{self._next_id:06X}" in self._local_object_ids:
            self._next_id += 1
        object_id = f"{self._next_id:06X}"
        self._local_object_ids.add(object_id)
        self._next_id += 1
        return f"{object_id}:{self._output_plugin_name}"

    @staticmethod
    def _target_handle_cache_key(handle) -> int:
        file_path = getattr(handle, "file_path", None)
        if file_path is not None:
            return hash(str(file_path).casefold())
        return id(handle)

    @classmethod
    def _form_key_to_legacy_shape(cls, form_key: str) -> str:
        if ":" not in form_key:
            return form_key
        left, right = form_key.split(":", 1)
        if "." not in left or not re.fullmatch(r"[0-9A-Fa-f]{1,8}", right.strip()):
            return form_key
        return f"{int(right, 16) & 0x00FFFFFF:06X}:{left}"

    @classmethod
    def _form_key_to_native_shape(cls, form_key: str) -> str:
        if ":" not in form_key:
            return form_key
        left, right = form_key.split(":", 1)
        if "." not in right or not re.fullmatch(r"[0-9A-Fa-f]{1,8}", left.strip()):
            return form_key
        return f"{right}:{int(left, 16) & 0x00FFFFFF:06X}"

    @classmethod
    def _target_form_key_lookup_candidates(cls, form_key: str) -> list[str]:
        candidates: list[str] = []
        for candidate in (
            form_key,
            cls._form_key_to_native_shape(form_key),
            cls._form_key_to_legacy_shape(form_key),
        ):
            if candidate and candidate not in candidates:
                candidates.append(candidate)
        return candidates

    @staticmethod
    def _record_signature(value: Any) -> str:
        if isinstance(value, dict):
            return str(
                value.get("signature")
                or value.get("record_type")
                or value.get("type")
                or ""
            )
        return str(
            getattr(value, "signature", "")
            or getattr(value, "record_type", "")
            or getattr(value, "type", "")
            or ""
        )

    @staticmethod
    def _record_editor_id(value: Any) -> str:
        if isinstance(value, dict):
            return str(value.get("editor_id") or value.get("eid") or "")
        return str(getattr(value, "editor_id", "") or getattr(value, "eid", "") or "")

    @staticmethod
    def _signature_matches(actual: str, expected: str) -> bool:
        return bool(actual and expected and record_type_signature(actual) == expected)

    def _target_handle_eid_rows(self, handle) -> list[dict[str, str]]:
        key = self._target_handle_cache_key(handle)
        cached = self._target_handle_eid_rows_cache.get(key)
        if cached is not None:
            return cached
        file_path = getattr(handle, "file_path", None)
        if file_path is None:
            rows = []
        else:
            try:
                rows = handle.record_index_rows()
            except Exception as exc:
                _log.debug("Target master EID row collection failed for %r: %s", handle, exc)
                rows = []
        normalized = [
            {
                "editor_id": str(editor_id or ""),
                "signature": str(signature or ""),
                "form_key": self._form_key_to_legacy_shape(str(form_key or "")),
            }
            for form_key, editor_id, signature, _object_id, _raw_form_id in rows
            if editor_id and signature and form_key
        ]
        self._target_handle_eid_rows_cache[key] = normalized
        return normalized

    def _native_lookup_from_eid_rows(
        self, handle, editor_id: str, expected_signature: str
    ) -> dict[str, str] | None:
        for row in self._target_handle_eid_rows(handle):
            actual_editor_id = self._record_editor_id(row)
            if actual_editor_id.casefold() != editor_id.casefold():
                continue
            actual_signature = self._record_signature(row) or expected_signature
            if not self._signature_matches(actual_signature, expected_signature):
                continue
            form_key = str(row.get("form_key") or row.get("FormKey") or "")
            if not form_key:
                continue
            return {
                "form_key": self._form_key_to_legacy_shape(form_key),
                "editor_id": actual_editor_id,
                "record_type": actual_signature,
            }
        return None

    def _find_vanilla_in_target_handles(
        self,
        editor_id: str,
        record_type: str,
    ) -> dict[str, str] | None:
        expected_signature = record_type_signature(record_type)
        if not expected_signature or not editor_id:
            return None
        for handle in self._target_master_handles:
            match = self._native_lookup_from_eid_rows(
                handle,
                editor_id,
                expected_signature,
            )
            if match is not None:
                return match
        return None

    def _find_vanilla_match(self, editor_id: str, record_type: str) -> dict[str, str] | None:
        handle_match = self._find_vanilla_in_target_handles(editor_id, record_type)
        if handle_match is not None:
            return handle_match
        if self._target_loader is None:
            return None
        try:
            rows = self._target_loader.search_by_editor_id_and_type(
                editor_id,
                record_type,
            )
        except Exception as exc:
            _log.debug(
                "Target DB vanilla lookup failed for %s (%s): %s",
                editor_id,
                record_type,
                exc,
            )
            return None
        expected_signature = record_type_signature(record_type)
        for row in rows:
            actual_editor_id = self._record_editor_id(row)
            if actual_editor_id.casefold() != editor_id.casefold():
                continue
            actual_signature = self._record_signature(row) or expected_signature
            if (
                expected_signature
                and not self._signature_matches(actual_signature, expected_signature)
            ):
                continue
            form_key = str(row.get("form_key") or row.get("FormKey") or "")
            if not form_key:
                continue
            return {
                "form_key": self._form_key_to_legacy_shape(form_key),
                "editor_id": actual_editor_id,
                "record_type": actual_signature,
            }
        return None

    def _refresh_cached_local_plugin(self, mapping: dict) -> None:
        object_id, plugin = self._split_formkey(str(mapping.get("new_formkey", "")))
        if not object_id:
            return
        plugin_stem, plugin_ext = os.path.splitext(plugin)
        if plugin_stem.lower() != self.mod_name.lower():
            return
        if plugin_ext.lower() == self._output_plugin_extension:
            return
        mapping["new_formkey"] = f"{object_id}:{self._output_plugin_name}"
        self._local_object_ids.add(object_id)

    def map_formkey(
        self,
        source_formkey: str,
        editor_id: str,
        record_type: str,
        source_game: str = "",
    ) -> dict:
        """Map a single source FormKey. Returns the mapping dict.

        If already mapped (from a previous run), returns the existing mapping.
        Otherwise, tries vanilla remap, then falls back to new allocation.
        """
        if source_formkey in self._mappings:
            cached = self._mappings[source_formkey]
            if cached.get("strategy") in {"new_allocation", "source_id_preserved"}:
                self._refresh_cached_local_plugin(cached)
            # Self-heal stale new_allocation mappings: if a previous run
            # allocated a fresh FormKey (because use_base_game_assets was
            # off or the target DB lacked the record), but this run has
            # vanilla remap available AND a match now exists, upgrade the
            # cached entry to vanilla_remap. This is safe because the mod
            # hasn't shipped yet when we reconvert. We NEVER downgrade a
            # vanilla_remap back to new_allocation, and stable
            # source_id_preserved mappings are only revalidated for
            # always-remapped system records.
            cached_strategy = cached.get("strategy")
            can_upgrade_cached_local = cached_strategy == "new_allocation" or (
                cached_strategy == "source_id_preserved"
                and _always_vanilla_remap(record_type)
            )
            if can_upgrade_cached_local:
                allow_vanilla_remap = (
                    self._use_base_game
                    or _always_vanilla_remap(record_type)
                )
                if allow_vanilla_remap:
                    match = self._find_vanilla_match(editor_id, record_type)
                    if match:
                        _log.info(
                            "Upgrading stale new_allocation to vanilla_remap: "
                            "%s (%s) %s -> %s",
                            editor_id, record_type,
                            cached.get("new_formkey"), match["form_key"],
                        )
                        cached["new_formkey"] = match["form_key"]
                        cached["strategy"] = "vanilla_remap"
                        cached["editor_id"] = editor_id
                        cached["record_type"] = record_type
                        if source_game:
                            cached["source_game"] = source_game
            return cached

        mapping: dict[str, str] = {
            "editor_id": editor_id,
            "record_type": record_type,
            "source_game": source_game,
        }

        # Try vanilla remap. Fires when use_base_game_assets is on, or the
        # record type is always-remapped (see _ALWAYS_VANILLA_REMAP_TYPES).
        allow_vanilla_remap = (
            self._use_base_game
            or _always_vanilla_remap(record_type)
        )
        if allow_vanilla_remap:
            match = self._find_vanilla_match(editor_id, record_type)
            if match:
                mapping["new_formkey"] = match["form_key"]
                mapping["strategy"] = "vanilla_remap"
                self._mappings[source_formkey] = mapping
                _log.debug(
                    "Vanilla remap: %s (%s) -> %s",
                    editor_id, source_formkey, mapping["new_formkey"],
                )
                return mapping

        source_object_id, _source_plugin = self._split_formkey(source_formkey)
        if (
            self._preserve_source_ids
            and source_object_id
            and source_object_id != "000000"
            and source_object_id not in self._local_object_ids
        ):
            mapping["new_formkey"] = f"{source_object_id}:{self._output_plugin_name}"
            mapping["strategy"] = "source_id_preserved"
            self._local_object_ids.add(source_object_id)
        else:
            mapping["new_formkey"] = self._allocate_local_formkey()
            mapping["strategy"] = "new_allocation"
        self._mappings[source_formkey] = mapping
        _log.debug(
            "Local mapping: %s (%s) -> %s [%s]",
            editor_id, source_formkey, mapping["new_formkey"],
            mapping["strategy"],
        )
        return mapping

    @property
    def mappings(self) -> dict[str, dict]:
        return self._mappings

    def get_masters(self) -> list[str]:
        """Collect master ESM/ESP names from vanilla-remapped FormKeys, ordered by first occurrence."""
        seen: set[str] = set()
        masters: list[str] = []
        for m in self._mappings.values():
            if m.get("strategy") != "vanilla_remap":
                continue
            fk = m.get("new_formkey", "")
            plugin = fk.split(":", 1)[1] if ":" in fk else ""
            if plugin and plugin not in seen:
                seen.add(plugin)
                masters.append(plugin)
        return masters

    def is_vanilla_remap(self, source_formkey: str) -> bool:
        m = self._mappings.get(source_formkey)
        return m is not None and m.get("strategy") == "vanilla_remap"

    def find_vanilla(self, editor_id: str, record_type: str) -> str | None:
        """Look up vanilla target FormKey by EditorID + record type.

        Queries target master handles first, then the legacy target DB fallback.
        Independent of ``use_base_game_assets`` and the cached mapping table —
        used by features (like additive race generation) that always need
        vanilla resolution regardless of the standalone-mod flag.
        """
        match = self._find_vanilla_match(editor_id, record_type)
        return match["form_key"] if match else None

    def save(self) -> None:
        """Write formkey_map.json to the mod folder."""
        data = {
            "mod_name": self.mod_name,
            "target_game": self.target_game,
            "use_base_game_assets": self._use_base_game,
            "preserve_source_ids": self._preserve_source_ids,
            "output_plugin_extension": self._output_plugin_extension,
            "next_id": f"{self._next_id:06X}",
            "mappings": self._mappings,
        }
        map_path = os.path.join(self._mod_path, "formkey_map.json")
        with open(map_path, "w", encoding="utf-8") as f:
            json.dump(data, f, indent=2, ensure_ascii=False)
        _log.info("Saved formkey_map.json: %d mappings", len(self._mappings))

    @staticmethod
    def rewrite_formkeys(data: Any, mapping: dict[str, dict]) -> Any:
        """Recursively rewrite FormKey references in a YAML data structure.

        Handles two reference shapes:
        1. Bare ``"OBJID:Plugin.esm"`` strings (legacy/shorthand).
        2. Canonical ``{reference: {plugin, object_id}}`` dicts emitted by
           the canonical extractor — present in records under
           ``data/<game>_esm_yaml/.../records/``.

        Any string or canonical-ref whose source FormKey appears in
        ``mapping`` is rewritten to the corresponding ``new_formkey``.
        Unmapped references are left unchanged.
        """
        # Local imports to avoid cycles with yaml_helpers.
        from bacup_lib.yaml_helpers import from_ref, to_ref

        if isinstance(data, str):
            if _FK_PATTERN.match(data) and data in mapping:
                return mapping[data]["new_formkey"]
            return data
        if isinstance(data, dict):
            # Canonical-ref shape: {"reference": {"plugin": ..., "object_id": ...}}
            # Detect by presence of a "reference" key with a dict value that
            # has "plugin" and "object_id" — same shape ``from_ref`` accepts.
            ref_inner = data.get("reference") if "reference" in data else None
            if (
                isinstance(ref_inner, dict)
                and "plugin" in ref_inner
                and "object_id" in ref_inner
            ):
                src_fk = from_ref(data)
                if src_fk and src_fk in mapping:
                    new_fk = mapping[src_fk].get("new_formkey")
                    new_ref = to_ref(new_fk) if new_fk else None
                    if new_ref is not None:
                        # Preserve any sibling keys on the outer dict.
                        rewritten = dict(data)
                        rewritten["reference"] = new_ref["reference"]
                        return rewritten
                return data
            return {
                k: FormKeyMapper.rewrite_formkeys(v, mapping)
                for k, v in data.items()
            }
        if isinstance(data, list):
            return [FormKeyMapper.rewrite_formkeys(item, mapping) for item in data]
        return data

    @staticmethod
    def rewrite_formkeys_batch(
        records: list[Any],
        mapping: dict[str, dict],
    ) -> list[Any]:
        """Batch variant of :meth:`rewrite_formkeys` over many records.

        Dispatches to the native Rust implementation when available — one
        JSON round-trip for the entire list instead of N Python recursions.
        Falls back to a per-record Python loop on import error or empty
        mapping.
        """
        if not records:
            return list(records)
        if not mapping:
            return list(records)
        try:
            from creation_lib.esp.native_runtime import (
                rewrite_formkeys_batch as _native_batch,
            )

            result = _native_batch(records, mapping)
        except Exception:
            result = None
        if result is None:
            return [
                FormKeyMapper.rewrite_formkeys(record, mapping) for record in records
            ]
        return list(result)
