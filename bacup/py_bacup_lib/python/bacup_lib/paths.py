from __future__ import annotations

import re
from dataclasses import dataclass
from typing import Any

from creation_lib.core.game_profiles import GAME_PROFILES, GameProfile


@dataclass(frozen=True)
class CreatureEntry:
    source_dir: str
    archetype_key: str
    target_name: str
    crea_eids: tuple[str, ...] = ()
    bone_map_key: str | None = None

_KNOWN_ROOTS = frozenset({"Meshes", "Textures", "Materials", "Sound"})
_KNOWN_ASSET_PREFIXES = frozenset(
    profile_id.lower()
    for profile_id in GAME_PROFILES
)
_FORMKEY_RE = re.compile(r"^[0-9A-Fa-f]{6,8}:[^/\\]+$")

_ASSET_EXTENSIONS_BY_ROOT = {
    "Meshes": frozenset({".nif", ".hkx", ".kf", ".egm", ".egt"}),
    "Textures": frozenset({".dds", ".tga", ".png"}),
    "Materials": frozenset({".bgsm", ".bgem", ".mat"}),
    "Sound": frozenset({".wav", ".xwm", ".ogg", ".mp3"}),
}
_GENERIC_FIELD_ROOTS = {
    "MODL": "Meshes",
    "MOD2": "Meshes",
    "MOD3": "Meshes",
    "MOD4": "Meshes",
    "MOD5": "Meshes",
    "Model": "Meshes",
    "ModelFileName": "Meshes",
}
_RECORD_FIELD_ROOTS = {
    **{("TXST", f"TX{index:02d}"): "Textures" for index in range(8)},
    ("MSWP", "BNAM"): "Materials",
    ("MSWP", "MNAM"): "Materials",
    ("MSWP", "OriginalMaterial"): "Materials",
    ("MSWP", "ReplacementMaterial"): "Materials",
    ("SOUN", "FNAM"): "Sound",
    ("SOUN", "SoundFilename"): "Sound",
    ("MUSC", "ANAM"): "Sound",
    ("MUSC", "FNAM"): "Sound",
    ("MUST", "ANAM"): "Sound",
    ("MUST", "FNAM"): "Sound",
    ("MUST", "TrackFileName"): "Sound",
    ("MUST", "FinaleFileName"): "Sound",
    ("SNDR", "ANAM"): "Sound",
    ("SNDR", "Sound"): "Sound",
    ("SNDR", "FNAM"): "Sound",
    ("SNDR", "File"): "Sound",
    ("IDLE", "ANAM"): "Meshes",
    ("IDLE", "AnimationFile"): "Meshes",
    ("IDLE", "BNAM"): "Meshes",
    ("IDLE", "BehaviorGraph"): "Meshes",
    ("RACE", "ANAM"): "Meshes",
    ("RACE", "BNAM"): "Meshes",
}
_PATH_SUBFIELDS = frozenset({"File", "Path"})


def _canonical_root(root: str) -> str | None:
    return next(
        (known_root for known_root in _KNOWN_ROOTS if known_root.lower() == root.lower()),
        None,
    )


def _normalize_path(path: str) -> str:
    return path.replace("\\", "/").strip().lstrip("/")


def _strip_data_prefix_before_known_root(path: str) -> str:
    parts = path.split("/")
    if len(parts) > 2 and parts[0].lower() == "data" and _canonical_root(parts[1]):
        return "/".join(parts[1:])
    return path


def _asset_root_for_field(record_type: str, field_name: str) -> str | None:
    record_sig = record_type.upper()
    return _RECORD_FIELD_ROOTS.get(
        (record_sig, field_name),
        _GENERIC_FIELD_ROOTS.get(field_name),
    )


def _looks_like_relative_asset_path(path: str, root: str) -> bool:
    if not path or _FORMKEY_RE.match(path):
        return False
    if path.lower() in {"null", "none"}:
        return False
    if path.lower().startswith("0x"):
        return False
    if ":" in path:
        return False
    leaf = path.rsplit("/", 1)[-1]
    if "." not in leaf:
        return False
    suffix = f".{leaf.rsplit('.', 1)[-1].lower()}"
    return suffix in _ASSET_EXTENSIONS_BY_ROOT.get(root, frozenset())


def apply_asset_prefix(path: str, source_profile: GameProfile) -> str:
    normalized = _strip_data_prefix_before_known_root(_normalize_path(path))
    if not normalized:
        return path

    parts = normalized.split("/")
    if not parts:
        return path

    root = next((known_root for known_root in _KNOWN_ROOTS if known_root.lower() == parts[0].lower()), None)
    if root is None:
        return path

    parts[0] = root
    if len(parts) > 1 and parts[1].lower() in _KNOWN_ASSET_PREFIXES:
        parts = [parts[0], *parts[2:]]

    return "/".join(parts)


def apply_asset_prefix_for_root(
    path: str,
    source_profile: GameProfile,
    root: str,
) -> str:
    canonical_root = _canonical_root(root)
    if canonical_root is None:
        return path

    normalized = _strip_data_prefix_before_known_root(_normalize_path(path))
    if not normalized:
        return path

    parts = normalized.split("/")
    if parts and _canonical_root(parts[0]) is not None:
        return apply_asset_prefix(normalized, source_profile)

    if not _looks_like_relative_asset_path(normalized, canonical_root):
        return path

    return apply_asset_prefix(f"{canonical_root}/{normalized}", source_profile)


def apply_record_asset_prefixes(
    record: dict[str, Any],
    record_type: str,
    source_profile: GameProfile,
) -> dict[str, Any]:
    out: dict[str, Any] = dict(record)
    fields = record.get("fields")
    if isinstance(fields, list):
        out["fields"] = [
            _prefix_canonical_field(entry, record_type, source_profile)
            for entry in fields
        ]
        return out

    for field_name, value in record.items():
        root = _asset_root_for_field(record_type, field_name)
        if root is not None:
            out[field_name] = _prefix_asset_value(value, source_profile, root)
    return out


def _prefix_canonical_field(
    entry: Any,
    record_type: str,
    source_profile: GameProfile,
) -> Any:
    if not isinstance(entry, dict) or len(entry) != 1:
        return entry
    field_name, value = next(iter(entry.items()))
    root = _asset_root_for_field(record_type, field_name)
    if root is None:
        return entry
    return {field_name: _prefix_asset_value(value, source_profile, root)}


def _prefix_asset_value(value: Any, source_profile: GameProfile, root: str) -> Any:
    if isinstance(value, str):
        return apply_asset_prefix_for_root(value, source_profile, root)
    if isinstance(value, list):
        return [_prefix_asset_value(item, source_profile, root) for item in value]
    if isinstance(value, dict):
        out = dict(value)
        for key in _PATH_SUBFIELDS:
            if key in out:
                out[key] = _prefix_asset_value(out[key], source_profile, root)
        return out
    return value


def creature_output_path(
    asset_path: str,
    creature_entry: CreatureEntry,
    source_profile: GameProfile,
    kind: str,
) -> str:
    base = f"Meshes/Actors/{creature_entry.target_name}"
    if kind == "skeleton":
        return f"{base}/CharacterAssets/skeleton.hkx"
    if kind == "project":
        return f"{base}/{creature_entry.target_name}Project.hkx"
    if kind == "character_hkx":
        return f"{base}/Characters/{creature_entry.target_name}.hkx"
    if kind == "character_xml":
        return f"{base}/Characters/{creature_entry.target_name}.xml"
    if kind == "behavior_root":
        return f"{base}/Behaviors/{creature_entry.target_name}RootBehavior.hkx"
    if kind == "behavior_everything":
        return f"{base}/Behaviors/{creature_entry.target_name}Everything.hkx"
    if kind == "animation":
        return f"{base}/Animations/{_creature_animation_suffix(asset_path)}"
    raise ValueError(f"unsupported creature output kind: {kind}")


def _creature_animation_suffix(asset_path: str) -> str:
    normalized = _strip_data_prefix_before_known_root(_normalize_path(asset_path))
    parts = normalized.split("/")
    for index, part in enumerate(parts):
        if part.lower() == "animations" and index + 1 < len(parts):
            return "/".join(parts[index + 1 :])
    return parts[-1]
