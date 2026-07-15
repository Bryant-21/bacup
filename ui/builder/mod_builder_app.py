"""Mod Builder app — business logic for Create, Deploy, Import, Release, Migrate, Spellcheck."""
from __future__ import annotations

import logging
import os
import queue
import re
import shutil
import zipfile
from pathlib import Path

from imgui_bundle import imgui

from creation_lib.build.archive_plan import DEFAULT_ARCHIVE_MAX_BYTES, discover_mod_archives, gib_to_bytes
from creation_lib.core.game_profiles import GAME_PROFILES
from ui.builder.release_metadata import (
    latest_tracked_version,
    read_mod_version,
    render_release_notes,
    sanitize_release_token,
    update_release_history,
    write_mod_version,
)
from app.paths import get_app_root as _get_app_root
from creation_lib.ui.theme.window_chrome import AsyncWorker
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import (
    begin_form, end_form, draw_path_row, draw_text_field, draw_combo_field, draw_float_field,
    draw_int_field,
)

PROJECT_ROOT = _get_app_root()
_log = logging.getLogger("toolkit.mod_builder")

MODS_DIR = os.path.join(str(PROJECT_ROOT), "mods")
TEX_OPTIONS_COL_W = 360


def _mod_kind(mod_dir: str) -> str:
    """Classify a mods/<name>/ folder as 'mod', 'xse', or 'combined' from its contents."""
    has_xmake = os.path.isfile(os.path.join(mod_dir, "xmake.lua"))
    has_src = os.path.isdir(os.path.join(mod_dir, "src"))
    is_xse = has_xmake and has_src
    name = os.path.basename(mod_dir)
    has_esp = any(
        os.path.isfile(os.path.join(mod_dir, f"{name}.{ext}"))
        for ext in ("esp", "esm", "esl")
    )
    yaml_dir = Path(mod_dir) / "yaml"
    has_yaml = yaml_dir.is_dir() and any(
        path.is_file() and path.suffix.lower() in {".yaml", ".yml"}
        for path in yaml_dir.rglob("*")
    )
    is_mod_authored = has_esp or has_yaml
    if is_xse and is_mod_authored:
        return "combined"
    if is_xse:
        return "xse"
    return "mod"

_XSE_NAMES: dict[str, str] = {
    "fo4":       "F4SE",
    "skyrimse":  "SKSE",
    "starfield": "SFSE",
    "fnv":       "NVSE",
    "fo3":       "FOSE",
}


def _xse_name_for(plugin_dir: str) -> str:
    """Read <plugin_dir>/.game and return the extender display name.

    Falls back to "F4SE" if no .game file is present.
    """
    game_file = os.path.join(plugin_dir, ".game")
    if os.path.isfile(game_file):
        with open(game_file, encoding="utf-8") as f:
            game = f.read().strip()
        return _XSE_NAMES.get(game, "F4SE")
    return "F4SE"
_PC_RES_OPTIONS = ["No Limit", "4096", "2048", "1024", "512"]
_PC_RES_VALUES = [0, 4096, 2048, 1024, 512]
_XBOX_RES_OPTIONS = ["1024", "512"]
_XBOX_RES_VALUES = [1024, 512]
_PLUGIN_TYPES = ["esp", "esm", "esl"]
_HEADER_FLAG_MASTER = 0x00000001
_HEADER_FLAG_LOCALIZED = 0x00000080
_HEADER_FLAG_LIGHT = 0x00000200
_HEADER_FLAG_MASTER_NAME = "MasterFile"
_HEADER_FLAG_LOCALIZED_NAME = "Localized"
_HEADER_FLAG_LIGHT_NAME = "LightPlugin"
_HEADER_FLAG_SYMBOLS = (
    (_HEADER_FLAG_MASTER_NAME, _HEADER_FLAG_MASTER),
    (_HEADER_FLAG_LOCALIZED_NAME, _HEADER_FLAG_LOCALIZED),
    (_HEADER_FLAG_LIGHT_NAME, _HEADER_FLAG_LIGHT),
)
_HEADER_FLAG_NAME_TO_BIT = {name: bit for name, bit in _HEADER_FLAG_SYMBOLS}
_PROGRESS_LINE_LIMIT = 3
_DEFAULT_ARCHIVE_MAX_SIZE_GB = DEFAULT_ARCHIVE_MAX_BYTES / 1024**3
_FO4_ARCHIVE_INI_SECTION = "Archive"
_FO4_ARCHIVE_MAIN_KEY = "SResourceArchiveList"
_FO4_ARCHIVE_ANIMATION_KEY = "SResourceArchiveList2"
_FO4_ARCHIVE_TEXTURE_KEY = "sResourceIndexFileList"
_FO4_MANAGED_ARCHIVE_KEYS = (
    _FO4_ARCHIVE_MAIN_KEY,
    _FO4_ARCHIVE_ANIMATION_KEY,
    _FO4_ARCHIVE_TEXTURE_KEY,
)


def _plugin_ext(mod_dir: str) -> str:
    """Read plugin extension (.esl/.esp/.esm) from authoring metadata, default esp."""
    plugin_yaml = os.path.join(mod_dir, "yaml", "plugin.yaml")
    if os.path.isfile(plugin_yaml):
        try:
            text = Path(plugin_yaml).read_text(encoding="utf-8")
            match = re.search(r"^plugin:\s*(\S+)", text[:2048], re.MULTILINE)
            if match:
                ext_match = re.search(r"\.(es[plm])$", match.group(1), re.IGNORECASE)
                if ext_match:
                    return ext_match.group(1).lower()
        except Exception:
            pass

    for ext in ("esl", "esm", "esp"):
        if os.path.isfile(os.path.join(mod_dir, f"{os.path.basename(mod_dir)}.{ext}")):
            return ext
    return "esp"


def _header_flag_bits(flags) -> int:
    if isinstance(flags, list):
        bits = 0
        for flag in flags:
            bits |= _header_flag_bits(flag)
        return bits
    if isinstance(flags, dict):
        if "raw" in flags:
            return _header_flag_bits(flags.get("raw"))
        return _header_flag_bits(flags.get("flags"))
    if isinstance(flags, int):
        return flags
    if flags in (None, ""):
        return 0
    text = str(flags).strip()
    bit = _HEADER_FLAG_NAME_TO_BIT.get(text)
    if bit is not None:
        return bit
    return int(text, 16)


def _has_header_flag(mod_dir: str, flag_name: str, bit: int) -> bool:
    plugin_yaml = os.path.join(mod_dir, "yaml", "plugin.yaml")
    if os.path.isfile(plugin_yaml):
        try:
            from ruamel.yaml import YAML
            yaml = YAML()
            with open(plugin_yaml, encoding="utf-8") as f:
                doc = yaml.load(f)
            if not isinstance(doc, dict):
                return False
            header = doc.get("header") or {}
            flags = header.get("flags") or []
            return bool(_header_flag_bits(flags) & bit)
        except Exception:
            return False

    return False


def _is_light_tagged(mod_dir: str) -> bool:
    """Return True if the selected authoring layout is flagged as light."""
    return _has_header_flag(mod_dir, _HEADER_FLAG_LIGHT_NAME, _HEADER_FLAG_LIGHT)


def _is_master_tagged(mod_dir: str) -> bool:
    """Return True if the selected authoring layout is flagged as a master."""
    return _has_header_flag(mod_dir, _HEADER_FLAG_MASTER_NAME, _HEADER_FLAG_MASTER)


def _set_symbolic_header_flags(header: dict, *, add_flags=(), remove_flags=()) -> None:
    from ruamel.yaml.comments import CommentedSeq

    bits = _header_flag_bits(header.get("flags"))
    for flag in remove_flags:
        bits &= ~_HEADER_FLAG_NAME_TO_BIT[flag]
    for flag in add_flags:
        bits |= _HEADER_FLAG_NAME_TO_BIT[flag]

    known_bits = 0
    for _, bit in _HEADER_FLAG_SYMBOLS:
        known_bits |= bit
    if bits & ~known_bits:
        header["flags"] = f"{bits:08X}"
        return

    header["flags"] = CommentedSeq(
        [name for name, bit in _HEADER_FLAG_SYMBOLS if bits & bit]
    )


def _set_plugin_header_flags(mod_dir: str, *, add_flags=(), remove_flags=()) -> None:
    from ruamel.yaml import YAML

    plugin_yaml = Path(mod_dir) / "yaml" / "plugin.yaml"
    if not plugin_yaml.is_file():
        raise FileNotFoundError(f"No canonical plugin.yaml found under {Path(mod_dir) / 'yaml'}")

    yaml = YAML()
    with plugin_yaml.open(encoding="utf-8") as f:
        doc = yaml.load(f)
    if not isinstance(doc, dict):
        return

    header = doc.get("header")
    if not isinstance(header, dict):
        header = {}
        doc["header"] = header

    _set_symbolic_header_flags(header, add_flags=add_flags, remove_flags=remove_flags)

    with plugin_yaml.open("w", encoding="utf-8") as f:
        yaml.dump(doc, f)


def _rewrite_plugin_key_references(yaml_dir: str | Path, old_key: str, new_key: str, *, rename_paths: bool) -> None:
    """Rewrite plugin-key references across a canonical authoring YAML tree.

    Record files and some record folders embed the plugin filename in both their
    contents and their path names (for example `000800_MyMod.esp.yaml`). When a
    mod switches between `.esp`, `.esm`, and `.esl`, those embedded keys must be
    updated everywhere or validation will treat the old plugin key as an
    external master.
    """
    if not old_key or not new_key or old_key == new_key:
        return

    root = Path(yaml_dir)
    if not root.is_dir():
        return

    for file_path in root.rglob("*"):
        if not file_path.is_file():
            continue
        try:
            text = file_path.read_text(encoding="utf-8")
        except UnicodeDecodeError:
            continue
        if old_key not in text:
            continue
        file_path.write_text(text.replace(old_key, new_key), encoding="utf-8")

    if not rename_paths:
        return

    rename_targets = sorted(
        (path for path in root.rglob("*") if old_key in path.name),
        key=lambda path: len(path.parts),
        reverse=True,
    )
    for path in rename_targets:
        target = path.with_name(path.name.replace(old_key, new_key))
        if target.exists():
            continue
        path.rename(target)


def _set_plugin_type_files(mod_dir: str, new_ext: str, needs_light: bool) -> None:
    """Update canonical authoring-dir plugin.yaml for a plugin type change.

    Args:
        mod_dir: Absolute path to the mod directory.
        new_ext: Target extension without dot — "esp", "esm", or "esl".
        needs_light: Whether header.flags should contain "LightPlugin".
    """
    from ruamel.yaml import YAML

    plugin_yaml = Path(mod_dir) / "yaml" / "plugin.yaml"
    if not plugin_yaml.is_file():
        raise FileNotFoundError(f"No canonical plugin.yaml found under {Path(mod_dir) / 'yaml'}")

    yaml = YAML()
    with plugin_yaml.open(encoding="utf-8") as f:
        doc = yaml.load(f)
    if not isinstance(doc, dict):
        return

    old_key = str(doc.get("plugin") or "")
    new_key = old_key
    for ext in ("esl", "esm", "esp"):
        if old_key.lower().endswith(f".{ext}"):
            new_key = old_key[: -len(ext)] + new_ext
            doc["plugin"] = new_key
            break

    header = doc.get("header")
    if not isinstance(header, dict):
        header = {}
        doc["header"] = header

    add_flags = (_HEADER_FLAG_LIGHT_NAME,) if needs_light else ()
    _set_symbolic_header_flags(header, add_flags=add_flags, remove_flags=(_HEADER_FLAG_LIGHT_NAME,))

    with plugin_yaml.open("w", encoding="utf-8") as f:
        yaml.dump(doc, f)

    _rewrite_plugin_key_references(Path(mod_dir) / "yaml", old_key, new_key, rename_paths=True)

    patches_root = Path(mod_dir) / "patches"
    if patches_root.is_dir():
        for patch_yaml_dir in patches_root.glob("*/yaml"):
            _rewrite_plugin_key_references(patch_yaml_dir, old_key, new_key, rename_paths=False)


def _fmt_size(nbytes: int) -> str:
    if nbytes < 1024:
        return f"{nbytes} B"
    elif nbytes < 1024 * 1024:
        return f"{nbytes / 1024:.1f} KB"
    else:
        return f"{nbytes / (1024 * 1024):.1f} MB"


def _mod_plugin_type_label(mod_dir: str, mod_name: str) -> str:
    """Return a short label for the mod's plugin type."""
    ext = _plugin_ext(mod_dir)
    if ext == "esp":
        tags = []
        if _is_master_tagged(mod_dir):
            tags.append("Master")
        if _is_light_tagged(mod_dir):
            tags.append("Light")
        if tags:
            return f"ESP ({', '.join(tags)})"
    for ext in ("esp", "esm", "esl"):
        if os.path.isfile(os.path.join(mod_dir, f"{mod_name}.{ext}")):
            if ext == "esp":
                tags = []
                if _is_master_tagged(mod_dir):
                    tags.append("Master")
                if _is_light_tagged(mod_dir):
                    tags.append("Light")
                if tags:
                    return f"ESP ({', '.join(tags)})"
            return ext.upper()

    return "N/A"


def _mod_plugin_text_color(mod_dir: str, mod_name: str) -> imgui.ImVec4:
    """Return a text color used to mark a mod row by plugin type."""
    ext = _plugin_ext(mod_dir)
    light_tagged = ext == "esp" and _is_light_tagged(mod_dir)
    master_tagged = ext == "esp" and _is_master_tagged(mod_dir)

    if master_tagged:
        return imgui.ImVec4(0.88, 0.30, 0.28, 1.0)
    elif light_tagged:
        return imgui.ImVec4(0.30, 0.62, 0.88, 1.0)
    elif ext == "esm":
        return imgui.ImVec4(0.88, 0.30, 0.28, 1.0)
    elif ext == "esl":
        return imgui.ImVec4(0.24, 0.78, 0.32, 1.0)
    else:
        return imgui.ImVec4(0.22, 0.44, 0.88, 1.0)


_XSE_COLOR = imgui.ImVec4(0.90, 0.68, 0.18, 1.0)   # gold — XSE / combined entries


def _xse_plugin_label(mod_name: str, kind: str) -> str:
    """Return list label for an xse or combined entry (resolves extender name from .game)."""
    if kind == "combined":
        mods_dir = os.path.join(MODS_DIR, mod_name)
        xse_name = _xse_name_for(mods_dir)
        ext = _plugin_ext(mods_dir)
        return f"{xse_name}+{ext.upper()}"
    plugin_dir = os.path.join(MODS_DIR, mod_name)
    return _xse_name_for(plugin_dir)


def _mod_list_label(mod_name: str, plugin_label: str, *, deployed: bool) -> str:
    prefix = "* " if deployed else ""
    return f"{prefix}{mod_name} [{plugin_label}]"


def _is_mod_deployed(
    mod_dir: str,
    mod_name: str,
    kind: str,
    game_data_dir: str | os.PathLike | None,
) -> bool:
    if not game_data_dir:
        return False

    data_dir = Path(game_data_dir)
    if not data_dir.is_dir():
        return False

    if kind != "xse":
        for ext in ("esp", "esl", "esm"):
            if (data_dir / f"{mod_name}.{ext}").is_file():
                return True

    if kind in ("xse", "combined"):
        xse_name = _xse_name_for(mod_dir)
        source_dir = Path(mod_dir) / xse_name
        deployed_dir = data_dir / xse_name
        if not source_dir.is_dir() or not deployed_dir.is_dir():
            return False
        for srcfile in source_dir.rglob("*"):
            if srcfile.is_file() and (deployed_dir / srcfile.relative_to(source_dir)).is_file():
                return True

    return False


def _read_mod_game(mod_dir: str, fallback_game: str) -> str:
    game_file = os.path.join(mod_dir, ".game")
    if os.path.isfile(game_file):
        try:
            with open(game_file, encoding="utf-8") as f:
                game = f.read().strip()
            if game:
                return game
        except Exception:
            pass
    return fallback_game


def _progress_fraction_from_line(line: str) -> float | None:
    match = re.match(r"^\[(\d+)/(\d+)\]", line.strip())
    if match:
        current = max(int(match.group(1)) - 1, 0)
        total = max(int(match.group(2)), 1)
        return min(current / total, 1.0)

    lowered = line.strip().lower()
    if lowered in ("done", "done.") or (lowered.startswith("===") and "complete" in lowered):
        return 1.0

    return None


def _resolve_fo4_game_ini_path() -> Path:
    return Path.home() / "Documents" / "My Games" / "Fallout4" / "Fallout4.ini"


def _resolve_fo4_custom_ini_path() -> Path:
    return Path.home() / "Documents" / "My Games" / "Fallout4" / "Fallout4Custom.ini"


def _ini_section_bounds(lines: list[str], section_name: str) -> tuple[int, int] | None:
    header = f"[{section_name.lower()}]"
    start = None
    for index, line in enumerate(lines):
        if line.strip().lower() == header:
            start = index
            break
    if start is None:
        return None

    end = len(lines)
    for index in range(start + 1, len(lines)):
        stripped = lines[index].strip()
        if stripped.startswith("[") and stripped.endswith("]"):
            end = index
            break
    return start, end


def _ensure_ini_section(lines: list[str], section_name: str) -> tuple[int, int]:
    bounds = _ini_section_bounds(lines, section_name)
    if bounds is not None:
        return bounds

    if lines and not lines[-1].endswith(("\n", "\r")):
        lines[-1] = f"{lines[-1]}\n"
    if lines and lines[-1].strip():
        lines.append("\n")
    lines.append(f"[{section_name}]\n")
    start = len(lines) - 1
    return start, len(lines)


def _ini_key_value(line: str) -> tuple[str, str] | None:
    stripped = line.strip()
    if not stripped or stripped.startswith(("#", ";")) or "=" not in line:
        return None
    key, value = line.split("=", 1)
    return key.strip(), value.strip()


def _ini_line_newline(line: str) -> str:
    if line.endswith("\r\n"):
        return "\r\n"
    if line.endswith("\n"):
        return "\n"
    return ""


def _ini_list_values(value: str) -> list[str]:
    return [part.strip() for part in value.split(",") if part.strip()]


def _find_ini_key_line(
    lines: list[str],
    start: int,
    end: int,
    key: str,
) -> tuple[int, str, list[str]] | None:
    for index in range(start + 1, end):
        parsed = _ini_key_value(lines[index])
        if parsed is None:
            continue
        line_key, value = parsed
        if line_key.lower() == key.lower():
            return index, line_key, _ini_list_values(value)
    return None


def _archive_ini_values_by_key(
    ini_path: Path | None,
    keys: tuple[str, ...],
) -> dict[str, list[str]]:
    if ini_path is None or not ini_path.is_file():
        return {}
    lines = ini_path.read_text(encoding="utf-8-sig").splitlines(keepends=True)
    bounds = _ini_section_bounds(lines, _FO4_ARCHIVE_INI_SECTION)
    if bounds is None:
        return {}

    start, end = bounds
    values_by_key: dict[str, list[str]] = {}
    for key in keys:
        existing_line = _find_ini_key_line(lines, start, end, key)
        if existing_line is None:
            continue
        _index, _line_key, existing_values = existing_line
        if existing_values:
            values_by_key[key] = _unique_archive_names(existing_values)
    return values_by_key


def _archive_label_base(archive_name: str) -> str:
    stem = Path(archive_name).stem
    if stem.lower().endswith("_xbox"):
        stem = stem[: -len("_xbox")]
    if " - " not in stem:
        return ""
    label = stem.rsplit(" - ", 1)[1]
    return label.rstrip("0123456789")


def _fo4_archive_ini_key_for_archive(archive_name: str) -> str | None:
    path = Path(archive_name)
    if path.suffix.lower() != ".ba2":
        return None
    label_base = _archive_label_base(path.name)
    if not label_base:
        return None
    if label_base == "Textures":
        return _FO4_ARCHIVE_TEXTURE_KEY
    if label_base == "Animations":
        return _FO4_ARCHIVE_ANIMATION_KEY
    return _FO4_ARCHIVE_MAIN_KEY


def _unique_archive_names(archive_names: list[str]) -> list[str]:
    seen: set[str] = set()
    unique: list[str] = []
    for archive_name in archive_names:
        normalized = Path(archive_name).name
        key = normalized.lower()
        if key in seen:
            continue
        seen.add(key)
        unique.append(normalized)
    return unique


def _group_fo4_archive_ini_entries(archive_names: list[str]) -> dict[str, list[str]]:
    grouped: dict[str, list[str]] = {key: [] for key in _FO4_MANAGED_ARCHIVE_KEYS}
    for archive_name in _unique_archive_names(archive_names):
        if Path(archive_name).stem.lower().endswith("_xbox"):
            continue
        key = _fo4_archive_ini_key_for_archive(archive_name)
        if key is None:
            continue
        grouped[key].append(archive_name)
    return {key: names for key, names in grouped.items() if names}


def _register_fo4_runtime_archive_ini_entries(
    archive_names: list[str],
    *,
    ini_path: Path | None = None,
    base_ini_path: Path | None = None,
) -> list[str]:
    grouped = _group_fo4_archive_ini_entries(archive_names)
    if not grouped:
        return []

    ini_path = ini_path or _resolve_fo4_custom_ini_path()
    base_ini_path = base_ini_path or _resolve_fo4_game_ini_path()
    base_values_by_key = _archive_ini_values_by_key(
        base_ini_path,
        _FO4_MANAGED_ARCHIVE_KEYS,
    )

    lines = (
        ini_path.read_text(encoding="utf-8-sig").splitlines(keepends=True)
        if ini_path.is_file()
        else []
    )
    start, end = _ensure_ini_section(lines, _FO4_ARCHIVE_INI_SECTION)

    added: list[str] = []
    changed = False
    for key in _FO4_MANAGED_ARCHIVE_KEYS:
        names = grouped.get(key, [])
        if not names:
            continue
        base_values = base_values_by_key.get(key, [])
        existing_line = _find_ini_key_line(lines, start, end, key)
        existing_values = existing_line[2] if existing_line is not None else []
        seen = {value.lower() for value in [*base_values, *existing_values]}
        missing = [name for name in names if name.lower() not in seen]
        merged_values = _unique_archive_names([*base_values, *existing_values, *names])
        if existing_line is None:
            lines.insert(end, f"{key}={', '.join(merged_values)}\n")
            end += 1
            added.extend(missing)
            changed = True
            continue
        index, line_key, _existing_values = existing_line
        if merged_values == existing_values:
            continue
        newline = _ini_line_newline(lines[index])
        lines[index] = f"{line_key}={', '.join(merged_values)}{newline}"
        added.extend(missing)
        changed = True

    if changed:
        ini_path.parent.mkdir(parents=True, exist_ok=True)
        ini_path.write_text("".join(lines), encoding="utf-8")
    return _unique_archive_names(added)


def _remove_fo4_archive_ini_entries(
    archive_names: list[str],
    *,
    ini_path: Path | None = None,
) -> list[str]:
    names = _unique_archive_names(archive_names)
    if not names:
        return []

    ini_path = ini_path or _resolve_fo4_custom_ini_path()
    if not ini_path.is_file():
        return []

    names_by_key = {name.lower(): name for name in names}
    lines = ini_path.read_text(encoding="utf-8-sig").splitlines(keepends=True)
    bounds = _ini_section_bounds(lines, _FO4_ARCHIVE_INI_SECTION)
    if bounds is None:
        return []
    start, end = bounds
    removed: list[str] = []

    for key in _FO4_MANAGED_ARCHIVE_KEYS:
        existing_line = _find_ini_key_line(lines, start, end, key)
        if existing_line is None:
            continue
        index, line_key, existing_values = existing_line
        kept_values: list[str] = []
        for value in existing_values:
            if value.lower() in names_by_key:
                removed.append(names_by_key[value.lower()])
                continue
            kept_values.append(value)
        if len(kept_values) == len(existing_values):
            continue
        newline = _ini_line_newline(lines[index])
        lines[index] = f"{line_key}={', '.join(kept_values)}{newline}"

    if removed:
        ini_path.write_text("".join(lines), encoding="utf-8")
    return _unique_archive_names(removed)


def _fo4_ini_archive_names_for_mod(
    mod_name: str,
    *,
    ini_path: Path | None = None,
) -> list[str]:
    ini_path = ini_path or _resolve_fo4_custom_ini_path()
    if not ini_path.is_file():
        return []

    lines = ini_path.read_text(encoding="utf-8-sig").splitlines(keepends=True)
    bounds = _ini_section_bounds(lines, _FO4_ARCHIVE_INI_SECTION)
    if bounds is None:
        return []

    start, end = bounds
    mod_stem = Path(mod_name).stem.lower()
    archive_names: list[str] = []
    for key in _FO4_MANAGED_ARCHIVE_KEYS:
        existing_line = _find_ini_key_line(lines, start, end, key)
        if existing_line is None:
            continue
        _index, _line_key, existing_values = existing_line
        for value in existing_values:
            path = Path(value)
            if path.suffix.lower() != ".ba2" or " - " not in path.stem:
                continue
            if path.stem.split(" - ", 1)[0].lower() == mod_stem:
                archive_names.append(path.name)
    return _unique_archive_names(archive_names)


def _shorten_progress_text(text: str, max_chars: int = 96) -> str:
    if len(text) <= max_chars:
        return text
    return f"{text[:max_chars - 3]}..."


class ModBuilderApp:
    """Business logic for the mod builder: mod list, tabs, actions, command runner."""

    def __init__(self, toolkit_settings=None):
        self._toolkit_settings = toolkit_settings
        self._runner: object | None = None
        self._running = False
        self._mod_list: list[str] = []
        self._mod_kinds: list[str] = []   # parallel to _mod_list: "mod" | "xse" | "combined"
        self._mod_deployed: list[bool] = []  # parallel to _mod_list
        self._selected_mod_idx = 0
        self._new_mod_name = ""
        self._create_plugin_type_idx = _PLUGIN_TYPES.index("esl")
        self._skip_build = False
        self._skip_pack = False
        self._skip_papyrus_compile = False
        self._esp_only = False
        self._xbox = False
        self._expanded_archives = False
        self._update_fo4_archive_ini = False
        self._pc_max_res_idx = 0  # [No Limit, 4096, 2048, 1024, 512]
        self._pc_effects_max_res_idx: int | None = None
        self._xbox_max_res_idx = 0  # [1024, 512]
        self._xbox_effects_max_res_idx: int | None = None
        self._fo4_install_idx = 0  # 0 = primary install; >0 = an extra deploy target
        self._fo4_install_choices: list[dict] = []
        workspace_settings = self._mod_builder_settings()
        self._deploy_to_mo2 = bool(workspace_settings.get("deploy_to_mo2", False))
        self._mo2_deploy_dir = str(workspace_settings.get("mo2_deploy_dir", "") or "")
        self._move_archives = bool(workspace_settings.get("move_archives", False))
        self._import_src_idx = 0
        self._mod_filter_text = ""
        self._info_text = "No mod selected"
        self._migrate_src_dir = ""
        self._migrate_name_override = ""
        self._migrate_add_prefix = True
        self._migrate_game_idx = 0
        self._create_git_repo = True
        self._migrate_git_repo = True
        self._deploy_patches = False
        self._skip_validation = False
        self._on_done_callback = None
        self._loading_label = ""
        self._progress_fraction: float | None = None
        self._progress_message = ""
        self._progress_lines: list[str] = []

        # Release options
        self._release_localize = False
        self._release_create_fuz = False
        self._release_create_xwm = False
        self._release_previs = False
        self._release_anim_data = False
        self._release_xbox = False
        self._release_expanded_archives = False
        self._release_pc_max_res_idx = 0    # [No Limit, 4096, 2048, 1024, 512]
        self._release_pc_effects_max_res_idx: int | None = None
        self._release_xbox_max_res_idx = 0  # [1024, 512]
        self._release_xbox_effects_max_res_idx: int | None = None
        self._current_mod_version = ""
        self._release_version = ""
        self._release_notes = ""
        self._release_audio_worker = None   # AsyncWorker — chains audio → repack → package
        self._release_audio_done_cb = None

        # Utils options (on-demand, standalone — no chaining)
        self._utils_create_fuz = False
        self._utils_create_xwm = False
        self._utils_audio_worker = None
        self._utils_translate_worker = None

        # Release log — dedicated log for reviewing release output
        self._release_log: list[tuple[int, str]] = []  # (level, message)
        self._release_active = False
        self._release_log_auto_scroll = True
        self._release_log_handler = _ReleaseLogHandler(self)
        self._release_error_popup_open = False
        self._release_error_message = ""
        self._release_archive_backup_dir: str | None = None
        self._last_runner_exit_code: int | None = None
        self._validation_errors: list[str] = []
        self._validation_errors_popup_open = False
        _log.addHandler(self._release_log_handler)

        # Delete mod popup state
        self._delete_popup_open = False
        self._delete_also_gitea = False
        self._delete_also_addon_nodes = True
        self._delete_addon_count: int | None = None
        self._delete_addon_count_mod = ""

        # Addon registry browser state
        self._addon_registry_entries: list[tuple[int, dict]] = []
        self._addon_registry_selected_index: int | None = None
        self._addon_registry_status = "Registry not loaded"
        self._addon_registry_needs_refresh = True

        # Spellcheck state
        self._spellcheck_results: list = []
        self._spellcheck_ignored: set[int] = set()  # indices of ignored issues
        self._spellcheck_worker = None  # AsyncWorker for spellcheck
        self._dict_popup_open = False
        self._dict_new_term = ""

        self._refresh_mods()

    @property
    def mod_prefix(self) -> str:
        if self._toolkit_settings and self._toolkit_settings.mod_prefix:
            return self._toolkit_settings.mod_prefix
        return ""

    @property
    def active_game(self) -> str:
        if self._toolkit_settings:
            return self._toolkit_settings.get_active_game()
        return "fo4"

    @property
    def game_data_dir(self) -> str:
        """Return the Data/ directory for the active game."""
        if self._toolkit_settings:
            gp = self._toolkit_settings.get_game_paths(self.active_game)
            root = gp.get("root_dir", "")
            if root:
                return os.path.join(root, "Data")
        return ""

    def _mod_builder_settings(self) -> dict:
        if not self._toolkit_settings:
            return {}
        get = getattr(self._toolkit_settings, "get_workspace_settings", None)
        return dict(get("mod_builder") or {}) if callable(get) else {}

    def _set_mod_builder_settings(self, settings: dict) -> None:
        if not self._toolkit_settings:
            return
        set_workspace = getattr(self._toolkit_settings, "set_workspace_settings", None)
        if callable(set_workspace):
            set_workspace("mod_builder", settings)
        save = getattr(self._toolkit_settings, "save", None)
        if callable(save):
            save()

    def _update_mod_builder_settings(self, updates: dict) -> None:
        settings = self._mod_builder_settings()
        settings.update(updates)
        self._set_mod_builder_settings(settings)

    def _archive_max_size_gb(self) -> float:
        ws = self._mod_builder_settings()
        try:
            value = float(ws.get("archive_max_size_gb", _DEFAULT_ARCHIVE_MAX_SIZE_GB))
        except (TypeError, ValueError):
            return _DEFAULT_ARCHIVE_MAX_SIZE_GB
        return value if value > 0 else _DEFAULT_ARCHIVE_MAX_SIZE_GB

    def _set_archive_max_size_gb(self, value: float) -> None:
        self._update_mod_builder_settings({"archive_max_size_gb": max(0.01, float(value))})

    def _set_move_archives(self, value: bool) -> None:
        self._move_archives = bool(value)
        self._update_mod_builder_settings({"move_archives": self._move_archives})

    def _draw_archive_max_size_field(self) -> None:
        changed, archive_max_size = draw_float_field(
            "Archive Max",
            self._archive_max_size_gb(),
            step=0.25,
            step_fast=1.0,
            fmt="%.2f",
            min_val=0.01,
        )
        if imgui.is_item_hovered():
            imgui.set_tooltip(
                "Maximum BA2/BSA archive size in GiB before splitting.\n"
                "This setting is shared by Deploy and Release."
            )
        if changed:
            self._set_archive_max_size_gb(archive_max_size)

    def _asset_workers(self) -> int:
        ws = self._mod_builder_settings()
        try:
            value = int(ws.get("asset_workers", ws.get("archive_workers", 0)))
        except (TypeError, ValueError):
            return 0
        return max(0, value)

    def _set_asset_workers(self, value: int) -> None:
        self._update_mod_builder_settings({"asset_workers": max(0, int(value))})

    def _draw_asset_workers_field(self) -> None:
        changed, asset_workers = draw_int_field(
            "Asset Workers",
            self._asset_workers(),
            step=1,
            step_fast=4,
            min_val=0,
        )
        if imgui.is_item_hovered():
            imgui.set_tooltip(
                "Worker count for archive packing and loose asset copying.\n"
                "0 = auto. This setting is shared by Deploy and Release."
            )
        if changed:
            self._set_asset_workers(asset_workers)

    def _game_data_path_for_game(self, game: str) -> Path | None:
        if game == "fo4" and self._deploy_to_mo2:
            mo2_dir = self._mo2_deploy_dir.strip()
            return Path(mo2_dir) if mo2_dir else None
        if game == "fo4":
            override = self._selected_fo4_install_root()
            if override:
                return Path(override) / "Data"
        if not self._toolkit_settings:
            return None
        paths = self._toolkit_settings.get_game_paths(game)
        root = paths.get("root_dir", "")
        if not root:
            return None
        return Path(root) / "Data"

    def _resolve_deploy_data_path(self, game: str) -> "Path":
        if game == "fo4" and self._deploy_to_mo2:
            mo2_dir = self._mo2_deploy_dir.strip()
            if not mo2_dir:
                raise RuntimeError("MO2 deploy folder is not set")
            return Path(mo2_dir)
        return self._resolve_game_data_path(game)

    # ── Public entry point ──────────────────────────────────────────────────

    def draw_main(self):
        """Draw the full mod builder panel (called from dockable window gui_function)."""
        expanded, _ = imgui.begin("Mod Builder##mod_builder", True)
        if expanded:
            imgui.text_disabled(self._info_text)
            imgui.separator()
            self._draw_tabs()
            self._draw_release_error_popup()
            self._draw_validation_errors_popup()
            self._draw_delete_popup()
            self._draw_loading_overlay()
        imgui.end()

    def poll_runner(self):
        """Poll the running command — call once per frame from workspace draw()."""
        if not self._runner:
            pass
        else:
            for line in self._runner.drain():
                self._record_progress_line(line)
                if line.startswith("VALIDATION_ERROR:"):
                    self._validation_errors.append(line[len("VALIDATION_ERROR:"):].strip())
                    _log.warning(line)
                else:
                    _log.info(line)
            if self._runner.finished:
                exit_code = self._runner.exit_code
                self._last_runner_exit_code = exit_code
                if exit_code == 0:
                    _log.info("Done.")
                else:
                    _log.error("Failed (exit code %d)", exit_code)
                if self._validation_errors:
                    self._validation_errors_popup_open = True
                self._running = False
                self._loading_label = ""
                self._runner = None
                self._refresh_deployed_state()
                self._on_mod_changed()
                cb = self._on_done_callback
                self._on_done_callback = None
                if cb:
                    cb()

        # Poll spellcheck worker
        if self._spellcheck_worker and self._spellcheck_worker.done:
            worker = self._spellcheck_worker
            self._spellcheck_worker = None
            self._running = False
            self._loading_label = ""
            if worker.error:
                _log.error("Spellcheck failed: %s", worker.error)
            else:
                results = worker.result or []
                self._spellcheck_results = results
                self._spellcheck_ignored.clear()
                if results:
                    _log.warning("Spellcheck found %d issue(s)", len(results))
                else:
                    _log.info("Spellcheck passed — no issues found")

        # Poll release audio worker (chains to BA2 repack → package on completion)
        if self._release_audio_worker and self._release_audio_worker.done:
            cb = self._release_audio_done_cb
            if cb:
                self._release_audio_done_cb = None
                cb()

        # Poll utils audio worker
        if self._utils_audio_worker and self._utils_audio_worker.done:
            worker = self._utils_audio_worker
            self._utils_audio_worker = None
            self._running = False
            self._loading_label = ""
            if worker.error:
                _log.error("Audio processing failed: %s", worker.error)
            else:
                r = worker.result or {}
                _log.info("Audio done — FUZ: %d, XWM: %d, skipped: %d, errors: %d",
                          len(r.get("fuz", [])), len(r.get("xwm", [])),
                          len(r.get("skipped", [])), len(r.get("errors", [])))

        # Poll utils translate worker (does not set/clear _running)
        if self._utils_translate_worker and self._utils_translate_worker.done:
            worker = self._utils_translate_worker
            self._utils_translate_worker = None
            if worker.error:
                _log.error("Translation failed: %s", worker.error)
            else:
                r = worker.result or {}
                _log.info("Translation done — %d record(s) translated, %d skipped, %d error(s)",
                          r.get("translated", 0), r.get("skipped", 0), len(r.get("errors", [])))
                _log.info("Use 'Build ESP' in the Deploy tab to rebuild with localized strings.")

    # ── Mod selector ────────────────────────────────────────────────────────

    def _draw_mod_selector(self):
        imgui.text("Mods & Plugins")
        imgui.same_line()
        imgui.text_disabled(f"({len(self._mod_list)})")

        avail_width = imgui.get_content_region_avail().x
        clear_width = 68.0
        imgui.set_next_item_width(max(140.0, avail_width - clear_width - imgui.get_style().item_spacing.x))
        _, self._mod_filter_text = imgui.input_text_with_hint(
            "##mod_filter",
            "Filter mods...",
            self._mod_filter_text,
        )
        imgui.same_line()
        if imgui.button("Clear"):
            self._mod_filter_text = ""

        imgui.spacing()
        imgui.separator()

        filtered = self._filtered_mods()
        list_height = max(180.0, imgui.get_content_region_avail().y - 120.0)
        if imgui.begin_child("##mod_list", imgui.ImVec2(0, list_height), child_flags=imgui.ChildFlags_.borders.value):
            if not self._mod_list:
                imgui.text_disabled("No mods found in mods/")
            elif not filtered:
                imgui.text_disabled("No matches")
            else:
                for idx, mod in filtered:
                    is_selected = idx == self._selected_mod_idx
                    kind = self._mod_kinds[idx] if idx < len(self._mod_kinds) else "mod"
                    deployed = idx < len(self._mod_deployed) and self._mod_deployed[idx]
                    if kind in ("xse", "combined"):
                        plugin_label = _xse_plugin_label(mod, kind)
                        text_color = _XSE_COLOR
                        entry_dir = os.path.join(MODS_DIR, mod)
                    else:
                        mod_dir = os.path.join(MODS_DIR, mod)
                        plugin_label = _mod_plugin_type_label(mod_dir, mod)
                        text_color = _mod_plugin_text_color(mod_dir, mod)
                        entry_dir = mod_dir
                    imgui.push_style_color(imgui.Col_.text, text_color)
                    clicked, _ = imgui.selectable(
                        f"{_mod_list_label(mod, plugin_label, deployed=deployed)}##mod_{idx}",
                        is_selected,
                        imgui.SelectableFlags_.span_all_columns,
                    )
                    imgui.pop_style_color()
                    if clicked and idx != self._selected_mod_idx:
                        self._selected_mod_idx = idx
                        self._on_mod_changed()
                    if imgui.is_item_hovered():
                        status = "Deployed" if deployed else "Not deployed"
                        imgui.set_item_tooltip(f"{entry_dir}\nPlugin: {plugin_label}\nStatus: {status}")
        imgui.end_child()

        imgui.spacing()
        no_mod = not self._mod_list
        if no_mod or self._running:
            imgui.begin_disabled()
        if imgui.button("Refresh"):
            self._refresh_mods()
        imgui.same_line()
        if imgui.button("Delete..."):
            self._delete_also_gitea = False
            self._delete_addon_count = None
            self._delete_addon_count_mod = ""
            self._delete_popup_open = True
        if no_mod or self._running:
            imgui.end_disabled()

    def _filtered_mods(self) -> list[tuple[int, str]]:
        """Return the visible mod rows after applying the filter text."""
        if not self._mod_list:
            return []
        filt = self._mod_filter_text.strip().lower()
        if not filt:
            return list(enumerate(self._mod_list))
        return [
            (idx, mod)
            for idx, mod in enumerate(self._mod_list)
            if filt in mod.lower()
        ]

    def _draw_release_error_popup(self):
        if self._release_error_popup_open:
            imgui.open_popup("Release Error##mod_builder")
            self._release_error_popup_open = False
        opened, _ = imgui.begin_popup_modal(
            "Release Error##mod_builder",
            None,
            imgui.WindowFlags_.always_auto_resize,
        )
        if opened:
            imgui.text_wrapped(self._release_error_message or "Release failed.")
            imgui.spacing()
            if imgui.button("OK", imgui.ImVec2(120, 0)):
                self._release_error_message = ""
                imgui.close_current_popup()
            imgui.end_popup()

    def _draw_validation_errors_popup(self):
        if self._validation_errors_popup_open:
            imgui.open_popup("Validation Errors##mod_builder")
            self._validation_errors_popup_open = False
        opened, _ = imgui.begin_popup_modal(
            "Validation Errors##mod_builder",
            None,
            imgui.WindowFlags_.always_auto_resize,
        )
        if opened:
            count = len(self._validation_errors)
            imgui.text_colored(
                imgui.ImVec4(1.0, 0.6, 0.1, 1.0),
                f"WARNING: {count} validation error(s) found — .esp was still built.",
            )
            imgui.spacing()
            imgui.separator()
            imgui.spacing()
            imgui.begin_child(
                "##val_err_scroll",
                imgui.ImVec2(620, min(count * 18 + 12, 300)),
                False,
            )
            for err in self._validation_errors:
                imgui.text_unformatted(err)
            imgui.end_child()
            imgui.spacing()
            if imgui.button("OK", imgui.ImVec2(120, 0)):
                self._validation_errors = []
                imgui.close_current_popup()
            imgui.end_popup()

    def _draw_loading_overlay(self):
        """Draw a semi-transparent overlay with a spinner when an operation is running."""
        if not self._running:
            return

        win_pos = imgui.get_window_pos()
        win_size = imgui.get_window_size()

        draw_list = imgui.get_foreground_draw_list()

        # Semi-transparent dim over the entire window
        bg_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.0, 0.0, 0.0, 0.60))
        draw_list.add_rect_filled(
            imgui.ImVec2(win_pos.x, win_pos.y),
            imgui.ImVec2(win_pos.x + win_size.x, win_pos.y + win_size.y),
            bg_col,
        )

        # Spinner character cycles with time
        t = imgui.get_time()
        spinner = ["|", "/", "-", "\\"][int(t * 8) % 4]
        panel_width = min(max(420.0, win_size.x * 0.62), max(260.0, win_size.x - 80.0))
        pad = 18.0
        text_chars = max(24, int((panel_width - pad * 2) / 7.0))
        title = f"{spinner}  {self._loading_label or 'Working...'}"
        message = _shorten_progress_text(self._progress_message or "Starting...", text_chars)
        detail_lines = [
            _shorten_progress_text(line, text_chars)
            for line in self._progress_lines
            if line != self._progress_message
        ][-_PROGRESS_LINE_LIMIT:]

        line_height = imgui.get_text_line_height()
        row_gap = 8.0
        bar_height = 10.0
        panel_height = pad * 2 + line_height * 2 + row_gap
        if self._progress_fraction is not None:
            panel_height += bar_height + row_gap
        if detail_lines:
            panel_height += row_gap + len(detail_lines) * (line_height + 2.0)

        cx = win_pos.x + (win_size.x - panel_width) * 0.5
        cy = win_pos.y + (win_size.y - panel_height) * 0.5

        # Panel background behind text
        panel_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.14, 0.14, 0.16, 1.0))
        border_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.35, 0.35, 0.40, 1.0))
        panel_min = imgui.ImVec2(cx, cy)
        panel_max = imgui.ImVec2(cx + panel_width, cy + panel_height)
        draw_list.add_rect_filled(
            panel_min,
            panel_max,
            panel_col,
            6.0,
        )
        draw_list.add_rect(
            panel_min,
            panel_max,
            border_col,
            6.0,
        )

        text_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.90, 0.90, 0.90, 1.0))
        muted_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.66, 0.66, 0.70, 1.0))
        bar_bg_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.24, 0.24, 0.28, 1.0))
        bar_fill_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.30, 0.62, 0.88, 1.0))

        x = panel_min.x + pad
        y = panel_min.y + pad
        draw_list.add_text(imgui.ImVec2(x, y), text_col, _shorten_progress_text(title, text_chars))
        y += line_height + row_gap
        draw_list.add_text(imgui.ImVec2(x, y), text_col, message)
        y += line_height + row_gap

        if self._progress_fraction is not None:
            fraction = max(0.0, min(self._progress_fraction, 1.0))
            bar_min = imgui.ImVec2(x, y)
            bar_max = imgui.ImVec2(panel_max.x - pad, y + bar_height)
            fill_max = imgui.ImVec2(bar_min.x + (bar_max.x - bar_min.x) * fraction, bar_max.y)
            draw_list.add_rect_filled(bar_min, bar_max, bar_bg_col, 3.0)
            draw_list.add_rect_filled(bar_min, fill_max, bar_fill_col, 3.0)
            y += bar_height + row_gap

        for line in detail_lines:
            draw_list.add_text(imgui.ImVec2(x, y), muted_col, line)
            y += line_height + 2.0

    def _draw_delete_popup(self):
        if self._delete_popup_open:
            imgui.open_popup("Delete Mod##mod_builder")
            self._delete_popup_open = False
        opened, _ = imgui.begin_popup_modal(
            "Delete Mod##mod_builder",
            None,
            imgui.WindowFlags_.always_auto_resize,
        )
        if opened:
            mod = self._selected_mod()
            imgui.text(f"Delete '{mod}'?")
            imgui.text("This cannot be undone.")
            has_git = self._selected_mod_has_git_repo()
            if has_git:
                imgui.spacing()
                _, self._delete_also_gitea = imgui.checkbox(
                    "Also delete remote Gitea repository##delete",
                    self._delete_also_gitea,
                )
            # Check for addon node allocations
            if self._delete_addon_count_mod != mod or self._delete_addon_count is None:
                self._delete_addon_count = self._get_mod_addon_count(mod)
                self._delete_addon_count_mod = mod
            addon_count = self._delete_addon_count
            if addon_count > 0:
                imgui.spacing()
                _, self._delete_also_addon_nodes = imgui.checkbox(
                    f"Remove {addon_count} AddonNode index allocation(s) from global tracking##delete",
                    self._delete_also_addon_nodes,
                )
            imgui.spacing()
            imgui.separator()
            imgui.spacing()
            if imgui.button("Delete##confirm_delete", imgui.ImVec2(120, 0)):
                imgui.close_current_popup()
                self._on_delete_mod()
            imgui.same_line()
            if imgui.button("Cancel##confirm_delete", imgui.ImVec2(120, 0)):
                imgui.close_current_popup()
            imgui.end_popup()

    def _addon_registry_entry_is_stale(self, info: dict) -> bool:
        """Return whether a registry allocation points at a missing mod folder."""
        mod_name = str(info.get("mod", "")).strip()
        if not mod_name:
            return True
        return not os.path.isdir(os.path.join(MODS_DIR, mod_name))

    def _refresh_addon_registry_view(self):
        """Reload the addon registry into the UI cache."""
        try:
            from creation_lib.addon_registry import AddonNodeRegistry

            registry = AddonNodeRegistry(MODS_DIR)
            registry.load()
            entries = registry.items()
            selected = self._addon_registry_selected_index
            if selected is not None and all(idx != selected for idx, _ in entries):
                selected = None
            if selected is None and entries:
                selected = entries[0][0]

            stale_count = sum(1 for _, info in entries if self._addon_registry_entry_is_stale(info))
            self._addon_registry_entries = entries
            self._addon_registry_selected_index = selected
            self._addon_registry_status = (
                f"{len(entries)} allocation(s), {stale_count} stale, next={registry.next_index()}"
            )
        except Exception as exc:
            self._addon_registry_entries = []
            self._addon_registry_selected_index = None
            self._addon_registry_status = f"Failed to load addon registry: {exc}"
        finally:
            self._addon_registry_needs_refresh = False

    def _on_addon_registry_refresh(self):
        self._addon_registry_needs_refresh = True
        self._refresh_addon_registry_view()

    def _on_addon_registry_remove_selected(self):
        idx = self._addon_registry_selected_index
        if idx is None:
            return
        try:
            from creation_lib.addon_registry import AddonNodeRegistry

            registry = AddonNodeRegistry(MODS_DIR)
            registry.remove(idx)
            self._addon_registry_needs_refresh = True
            self._refresh_addon_registry_view()
        except Exception as exc:
            _log.error("Failed to remove addon registry entry %s: %s", idx, exc)

    def _on_addon_registry_prune_stale(self):
        try:
            from creation_lib.addon_registry import AddonNodeRegistry

            registry = AddonNodeRegistry(MODS_DIR)
            registry.release_stale()
            self._addon_registry_needs_refresh = True
            self._refresh_addon_registry_view()
        except Exception as exc:
            _log.error("Failed to prune stale addon registry entries: %s", exc)

    def _on_delete_mod_done(self):
        """Refresh mod and registry views after a delete run completes."""
        self._refresh_mods()
        self._addon_registry_needs_refresh = True

    # ── Tabs ────────────────────────────────────────────────────────────────

    def _draw_tabs(self):
        if imgui.begin_tab_bar("##mod_tabs"):
            if imgui.begin_tab_item("Create")[0]:
                self._draw_create_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Deploy")[0]:
                self._draw_deploy_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Addon Registry")[0]:
                self._draw_addon_registry_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Import")[0]:
                self._draw_import_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Release")[0]:
                self._draw_release_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Utils")[0]:
                self._draw_utils_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Migrate")[0]:
                self._draw_migrate_tab()
                imgui.end_tab_item()
            imgui.end_tab_bar()

    def _draw_create_tab(self):
        _btn = imgui.ImVec2(-1, 0)
        disabled = self._running
        if disabled:
            imgui.begin_disabled()

        if begin_form("##create_form"):
            changed, self._new_mod_name = draw_text_field(f"{self.mod_prefix}_", self._new_mod_name)
            if changed and imgui.is_key_pressed(imgui.Key.enter):
                self._on_setup()
            _, self._create_plugin_type_idx = draw_combo_field("Plugin Type", _PLUGIN_TYPES, self._create_plugin_type_idx)
            if imgui.is_item_hovered():
                imgui.set_tooltip("Choose the plugin type for the new mod (esp / esm / esl).")
            end_form()

        imgui.separator()
        _, self._create_git_repo = imgui.checkbox("Create Git repo##create", self._create_git_repo)
        if imgui.is_item_hovered():
            imgui.set_tooltip("Initialize a git repo and push to Gitea")
        imgui.separator()

        if imgui.begin_table("##create_btns", 5, imgui.TableFlags_.sizing_stretch_same.value):
            for i in range(5):
                imgui.table_setup_column(f"C{i}", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_next_row()
            imgui.table_set_column_index(0)
            if imgui.button("Create Mod##create", _btn):
                self._on_setup()
            imgui.end_table()

        if disabled:
            imgui.end_disabled()

    def _draw_xse_action_row(self, _btn, *, suffix: str):
        """Build/Deploy/Undeploy DLL buttons. Used as a debug-helper row at the
        bottom of the deploy tab for both xse-only and combined entries.

        ``suffix`` disambiguates ImGui ids so the row can be drawn under different
        layouts without colliding (e.g. "##xse" vs "##combined").
        """
        mod = self._selected_mod()
        src_dir = self._selected_xse_src_dir()
        mods_dir = os.path.join(MODS_DIR, mod) if mod else ""
        xse_name = _xse_name_for(mods_dir) if mods_dir else "F4SE"
        has_xmake = bool(src_dir) and os.path.isfile(os.path.join(src_dir, "xmake.lua"))
        disabled = self._running or not self._mod_list

        imgui.text_disabled(
            f"{xse_name} debug: build / re-deploy / remove the plugin DLL"
            + (f"  ·  staged: mods/{mod}/{xse_name}/Plugins/{mod}.dll" if mod else "")
        )
        imgui.spacing()

        if imgui.begin_table(f"##xse_actions_{suffix}", 3, imgui.TableFlags_.sizing_stretch_same.value):
            for i in range(3):
                imgui.table_setup_column(f"X{suffix}{i}", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_next_row()

            imgui.table_set_column_index(0)
            if disabled or not has_xmake:
                imgui.begin_disabled()
            if imgui.button(f"Build DLL (xmake)##{suffix}", _btn):
                self._on_xse_build()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    f"Run xmake install -y in mods/<name>/ to compile\n"
                    f"and stage the DLL to mods/<name>/{xse_name}/Plugins/."
                    if has_xmake else "No xmake.lua found — cannot build DLL."
                )
            if disabled or not has_xmake:
                imgui.end_disabled()

            imgui.table_set_column_index(1)
            if disabled:
                imgui.begin_disabled()
            if imgui.button(f"Deploy DLL Only##{suffix}", _btn):
                self._on_xse_deploy()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    f"Copy ONLY the staged {xse_name}/ tree to game Data/{xse_name}/.\n"
                    f"(For debug iteration — the regular Deploy button already handles this.)"
                )
            if disabled:
                imgui.end_disabled()

            imgui.table_set_column_index(2)
            if disabled:
                imgui.begin_disabled()
            if imgui.button(f"Undeploy DLL Only##{suffix}", _btn):
                self._on_xse_undeploy()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    f"Remove ONLY the deployed {xse_name}/ files from game Data/{xse_name}/.\n"
                    f"(For debug iteration — the regular Undeploy button already handles this.)"
                )
            if disabled:
                imgui.end_disabled()

            imgui.end_table()

    def _draw_fo4_install_picker(self):
        """Deploy-target selector, shown only when extra FO4 installs are configured."""
        if self._get_mod_game() != "fo4":
            return
        self._refresh_fo4_install_choices()
        choices = self._fo4_install_choices
        if len(choices) < 2:
            return
        labels = [c.get("label", "") for c in choices]
        if begin_form("##deploy_fo4_install"):
            changed, new_idx = draw_combo_field("FO4 Install", labels, self._fo4_install_idx)
            if imgui.is_item_hovered():
                imgui.set_tooltip(
                    "Which Fallout 4 install to use for build-time game data.\n"
                    "When MO2 deploy is off, this is also the deploy target.\n"
                    "Add more installs in Settings → Paths → Fallout 4."
                )
            end_form()
            if changed:
                self._fo4_install_idx = new_idx
                self._refresh_deployed_state()
        imgui.spacing()

    def _draw_mo2_deploy_option(self):
        if self._get_mod_game() != "fo4":
            return
        changed, self._deploy_to_mo2 = imgui.checkbox(
            "Deploy to MO2 virtual Data folder##deploy_mo2",
            self._deploy_to_mo2,
        )
        if imgui.is_item_hovered():
            imgui.set_tooltip(
                "Copy deploy outputs to an MO2 mod folder treated as the Data root.\n"
                "Build steps still use the selected Fallout 4 install."
            )
        if changed:
            self._set_mod_builder_settings({"deploy_to_mo2": self._deploy_to_mo2})
            self._refresh_deployed_state()

        if self._deploy_to_mo2:
            if begin_form("##deploy_mo2_path"):
                _, clicked = draw_path_row("MO2 Folder", self._mo2_deploy_dir)
                end_form()
                if clicked:
                    path = pick_folder("Select MO2 Mod Folder", self._mo2_deploy_dir)
                    if path:
                        self._mo2_deploy_dir = os.path.normpath(path)
                        self._set_mod_builder_settings(
                            {"mo2_deploy_dir": self._mo2_deploy_dir}
                        )
                        self._refresh_deployed_state()

    def _draw_deploy_tab(self):
        _btn = imgui.ImVec2(-1, 0)

        kind = self._selected_mod_kind()
        is_xse_only = kind == "xse"
        has_xse = kind in ("xse", "combined")

        self._draw_fo4_install_picker()

        if is_xse_only:
            mod = self._selected_mod()
            xse_name = _xse_name_for(os.path.join(MODS_DIR, mod)) if mod else "F4SE"
            imgui.text_disabled(
                f"{xse_name} plugin (no .esp). Deploy copies mods/{mod}/{xse_name}/ → game Data/{xse_name}/."
                if mod else f"{xse_name} plugin (no .esp)."
            )
            imgui.spacing()
            self._draw_mo2_deploy_option()
            imgui.spacing()
            imgui.separator()
            imgui.spacing()

        _split_flags = imgui.TableFlags_.sizing_stretch_prop.value
        if not is_xse_only and imgui.begin_table("##deploy_split", 2, _split_flags):
            imgui.table_setup_column("Flags", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_setup_column("Tex", imgui.TableColumnFlags_.width_fixed.value, TEX_OPTIONS_COL_W)
            imgui.table_next_row()

            imgui.table_set_column_index(0)
            _, self._skip_build = imgui.checkbox("Skip Build", self._skip_build)
            if imgui.is_item_hovered():
                imgui.set_tooltip("Use existing .esp without rebuilding")
            _, self._skip_pack = imgui.checkbox("Skip Pack", self._skip_pack)
            if imgui.is_item_hovered():
                imgui.set_tooltip("Use existing BA2s without repacking")
            _, self._skip_papyrus_compile = imgui.checkbox("Skip Compile", self._skip_papyrus_compile)
            if imgui.is_item_hovered():
                imgui.set_tooltip("Skip Papyrus (.psc → .pex) compilation; use existing .pex")
            _, self._esp_only = imgui.checkbox("ESP Only", self._esp_only)
            if imgui.is_item_hovered():
                imgui.set_tooltip("Deploy only the .esp (no BA2 packing)")
            _, self._xbox = imgui.checkbox("Xbox BA2s", self._xbox)
            if imgui.is_item_hovered():
                imgui.set_tooltip("Also create Xbox-format BA2 archives (_xbox suffix)")
            _, self._expanded_archives = imgui.checkbox("Expanded BA2s", self._expanded_archives)
            if imgui.is_item_hovered():
                imgui.set_tooltip(
                    "Use family archive labels such as Meshes, Sounds, and Scripts.\n"
                    "Off keeps small mods in Main + Textures when they fit."
                )
            changed, move_archives = imgui.checkbox("Move BA2s", self._move_archives)
            if imgui.is_item_hovered():
                imgui.set_tooltip(
                    "Move generated BA2/BSA archives into the deploy target instead of copying them.\n"
                    "Use this to avoid keeping a second archive copy in the mod folder."
                )
            if changed:
                self._set_move_archives(move_archives)
            if self._get_mod_game() == "fo4":
                _, self._update_fo4_archive_ini = imgui.checkbox(
                    "FO4 Archive INI",
                    self._update_fo4_archive_ini,
                )
                if imgui.is_item_hovered():
                    imgui.set_tooltip(
                        "Register deployed BA2s in Fallout4Custom.ini.\n"
                        "Use this when FO4 needs explicit archive list entries."
                    )
            _, self._deploy_patches = imgui.checkbox("Include Patches", self._deploy_patches)
            if imgui.is_item_hovered():
                imgui.set_tooltip("Also build and deploy all patch plugins from patches/")
            _, self._skip_validation = imgui.checkbox("Skip Validation", self._skip_validation)
            if imgui.is_item_hovered():
                imgui.set_tooltip("Skip the validation pass before building the .esp")
            self._draw_mo2_deploy_option()

            imgui.table_set_column_index(1)
            if begin_form("##deploy_tex"):
                _, self._pc_max_res_idx = draw_combo_field("PC Max", _PC_RES_OPTIONS, self._pc_max_res_idx)
                if imgui.is_item_hovered():
                    imgui.set_tooltip("Max texture dimension for PC BA2s (0 = no resize)")
                pc_effects_idx = self._pc_effects_max_res_idx if self._pc_effects_max_res_idx is not None else self._pc_max_res_idx
                changed, pc_effects_idx = draw_combo_field("PC Effects", _PC_RES_OPTIONS, pc_effects_idx)
                if changed:
                    self._pc_effects_max_res_idx = pc_effects_idx
                if imgui.is_item_hovered():
                    imgui.set_tooltip(
                        "Max texture dimension for Textures/Effects in the PC archive.\n"
                        "Use this to compress effects more than the rest of the texture set."
                    )
                if self._xbox:
                    _, self._xbox_max_res_idx = draw_combo_field("Xbox Max", _XBOX_RES_OPTIONS, self._xbox_max_res_idx)
                    if imgui.is_item_hovered():
                        imgui.set_tooltip("Max texture dimension for Xbox BA2s")
                    xbox_effects_idx = self._xbox_effects_max_res_idx if self._xbox_effects_max_res_idx is not None else self._xbox_max_res_idx
                    changed, xbox_effects_idx = draw_combo_field("Xbox Effects", _XBOX_RES_OPTIONS, xbox_effects_idx)
                    if changed:
                        self._xbox_effects_max_res_idx = xbox_effects_idx
                    if imgui.is_item_hovered():
                        imgui.set_tooltip(
                            "Max texture dimension for Textures/Effects in the Xbox archive.\n"
                            "Use this to compress Xbox effects more than the rest of the texture set."
                        )
                self._draw_archive_max_size_field()
                self._draw_asset_workers_field()
                end_form()

            imgui.end_table()

        imgui.separator()

        disabled = self._running or not self._mod_list
        git_disabled = disabled or not self._selected_mod_has_git_repo()
        if disabled:
            imgui.begin_disabled()
        if imgui.begin_table("##deploy_btns", 2, imgui.TableFlags_.sizing_stretch_same.value):
            for i in range(2):
                imgui.table_setup_column(f"B{i}", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_next_row()

            imgui.table_set_column_index(0)
            if imgui.button("Deploy##deploy", _btn):
                self._on_deploy()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Copy plugin DLL/INI tree to game Data."
                    if is_xse_only else
                    "Build + pack archives + copy to game Data.\n"
                    "Also copies F4SE/SKSE/SFSE/NVSE/FOSE plugin tree if present."
                )

            imgui.table_set_column_index(1)
            if imgui.button("Undeploy##deploy", _btn):
                self._on_undeploy()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Remove deployed plugin DLL/INI tree from game Data."
                    if is_xse_only else
                    "Remove mod files from game Data (esp, BA2s, Strings, Meshes, plugin DLL tree)."
                )

            imgui.end_table()

        if not is_xse_only:
            imgui.spacing()
            if imgui.begin_table("##deploy_loose_btns", 2, imgui.TableFlags_.sizing_stretch_same.value):
                imgui.table_setup_column("L0", imgui.TableColumnFlags_.width_stretch.value)
                imgui.table_setup_column("L1", imgui.TableColumnFlags_.width_stretch.value)
                imgui.table_next_row()

                imgui.table_set_column_index(0)
                if imgui.button("Deploy Loose Assets##deploy_loose", _btn):
                    self._on_deploy_loose()
                if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                    imgui.set_tooltip(
                        "Build + compile, then copy all mod assets to the game\n"
                        "Data folder as loose files (no BA2 packing).\n"
                        "Applies PC texture size settings to data/Textures and data/Textures/Effects.\n"
                        "Remembers what was deployed via .loose_manifest.json."
                    )

                imgui.table_set_column_index(1)
                if imgui.button("Undeploy Loose##deploy_loose", _btn):
                    self._on_undeploy_loose()
                if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                    imgui.set_tooltip(
                        "Remove every file listed in .loose_manifest.json\n"
                        "from the game Data folder, then delete the manifest."
                    )

                imgui.end_table()

        if has_xse:
            imgui.spacing()
            imgui.separator()
            self._draw_xse_action_row(_btn, suffix=kind)

        imgui.spacing()
        if imgui.begin_table("##deploy_git_btns", 3, imgui.TableFlags_.sizing_stretch_same.value):
            for i in range(3):
                imgui.table_setup_column(f"G{i}", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_next_row()

            imgui.table_set_column_index(0)
            if git_disabled:
                imgui.begin_disabled()
            if imgui.button("Commit to Git##deploy", _btn):
                self._on_utils_git_commit()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Stage all local changes, create an automatic commit, and push it to origin."
                    if self._selected_mod_has_git_repo() else
                    "This mod does not have a local Git repository yet."
                )
            if git_disabled:
                imgui.end_disabled()

            imgui.table_set_column_index(1)
            if git_disabled:
                imgui.begin_disabled()
            if imgui.button("Pull From Git##deploy", _btn):
                self._on_utils_git_pull()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Pull the latest remote changes into this mod repository (fast-forward only)."
                    if self._selected_mod_has_git_repo() else
                    "This mod does not have a local Git repository yet."
                )
            if git_disabled:
                imgui.end_disabled()

            imgui.table_set_column_index(2)
            if git_disabled:
                imgui.begin_disabled()
            if imgui.button("Checkout from Git##deploy", _btn):
                self._on_utils_git_checkout()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Discard all local tracked and untracked changes in this mod repository."
                    if self._selected_mod_has_git_repo() else
                    "This mod does not have a local Git repository yet."
                )
            if git_disabled:
                imgui.end_disabled()

            imgui.end_table()
        if disabled:
            imgui.end_disabled()

    def _draw_addon_registry_tab(self):
        if self._addon_registry_needs_refresh:
            self._refresh_addon_registry_view()

        _btn = imgui.ImVec2(-1, 0)
        stale_count = sum(1 for _, info in self._addon_registry_entries if self._addon_registry_entry_is_stale(info))
        imgui.text_disabled("Registry file: mods/.addon_registry.json")
        if self._addon_registry_status.startswith("Failed"):
            imgui.text_colored(imgui.ImVec4(1.0, 0.35, 0.35, 1.0), self._addon_registry_status)
        else:
            imgui.text_disabled(self._addon_registry_status)

        if imgui.begin_table("##addon_registry_actions", 3, imgui.TableFlags_.sizing_stretch_same.value):
            for i in range(3):
                imgui.table_setup_column(f"A{i}", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_next_row()

            imgui.table_set_column_index(0)
            refresh_disabled = self._running
            if refresh_disabled:
                imgui.begin_disabled()
            if imgui.button("Refresh Registry##addon_registry", _btn):
                self._on_addon_registry_refresh()
            if refresh_disabled:
                imgui.end_disabled()

            imgui.table_set_column_index(1)
            remove_disabled = self._running or self._addon_registry_selected_index is None
            if remove_disabled:
                imgui.begin_disabled()
            if imgui.button("Remove Selected##addon_registry", _btn):
                self._on_addon_registry_remove_selected()
            if remove_disabled:
                imgui.end_disabled()

            imgui.table_set_column_index(2)
            prune_disabled = self._running or stale_count == 0
            if prune_disabled:
                imgui.begin_disabled()
            if imgui.button("Remove Stale##addon_registry", _btn):
                self._on_addon_registry_prune_stale()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Remove allocations whose mod folder no longer exists."
                )
            if prune_disabled:
                imgui.end_disabled()

            imgui.end_table()

        imgui.spacing()
        child_h = max(260.0, imgui.get_content_region_avail().y - 120.0)
        if imgui.begin_child("##addon_registry_scroll", imgui.ImVec2(0, child_h), child_flags=imgui.ChildFlags_.borders.value):
            if not self._addon_registry_entries:
                imgui.text_disabled("No addon allocations found.")
            else:
                flags = (
                    imgui.TableFlags_.borders_inner.value
                    | imgui.TableFlags_.row_bg.value
                    | imgui.TableFlags_.resizable.value
                )
                if imgui.begin_table("##addon_registry_table", 5, flags):
                    imgui.table_setup_column("Index", imgui.TableColumnFlags_.width_fixed.value, 70)
                    imgui.table_setup_column("Mod", imgui.TableColumnFlags_.width_stretch.value)
                    imgui.table_setup_column("EditorID", imgui.TableColumnFlags_.width_stretch.value)
                    imgui.table_setup_column("Game", imgui.TableColumnFlags_.width_fixed.value, 90)
                    imgui.table_setup_column("Status", imgui.TableColumnFlags_.width_fixed.value, 90)
                    imgui.table_headers_row()
                    for idx, info in self._addon_registry_entries:
                        mod_name = str(info.get("mod", "")).strip() or "<missing>"
                        editor_id = str(info.get("editor_id", "")).strip() or "-"
                        game = str(info.get("game", "")).strip() or "-"
                        is_stale = self._addon_registry_entry_is_stale(info)
                        row_color = imgui.ImVec4(1.0, 0.45, 0.35, 1.0) if is_stale else imgui.ImVec4(0.90, 0.90, 0.90, 1.0)
                        imgui.table_next_row()
                        imgui.push_style_color(imgui.Col_.text, row_color)

                        imgui.table_set_column_index(0)
                        selected = idx == self._addon_registry_selected_index
                        clicked, _ = imgui.selectable(f"{idx}##addon_registry_{idx}", selected)
                        if clicked:
                            self._addon_registry_selected_index = idx
                        if imgui.is_item_hovered():
                            tooltip = (
                                f"Mod: {mod_name}\n"
                                f"EditorID: {editor_id}\n"
                                f"Game: {game}\n"
                                f"Path: {os.path.join(MODS_DIR, mod_name) if mod_name != '<missing>' else '<missing>'}"
                            )
                            if is_stale:
                                tooltip += "\nStatus: stale (mod folder missing)"
                            imgui.set_tooltip(tooltip)

                        imgui.table_set_column_index(1)
                        imgui.text(mod_name)
                        imgui.table_set_column_index(2)
                        imgui.text(editor_id)
                        imgui.table_set_column_index(3)
                        imgui.text(game)
                        imgui.table_set_column_index(4)
                        imgui.text("Stale" if is_stale else "OK")
                        imgui.pop_style_color()
                    imgui.end_table()
            imgui.end_child()

    def _draw_import_tab(self):
        if self._selected_mod_kind() == "xse":
            xse_name = _xse_name_for(os.path.join(MODS_DIR, self._selected_mod()))
            imgui.text_disabled(f"Not applicable for {xse_name}-only plugin entries.")
            imgui.text_disabled(f"Select the combined [{xse_name}+ESP] entry or a standard mod.")
            return

        _ESP_SRC_OPTIONS = ["Deployed (Game Data)", "Local (mod folder)"]
        _btn = imgui.ImVec2(-1, 0)

        if begin_form("##import_form"):
            _, self._import_src_idx = draw_combo_field("ESP Source", _ESP_SRC_OPTIONS, self._import_src_idx)
            if imgui.is_item_hovered():
                imgui.set_tooltip(
                    "Where to read the plugin from:\n"
                    "  Deployed - the copy in game Data (after CK save)\n"
                    "  Local - the plugin in mods/<mod>/"
                )
            end_form()

        imgui.separator()

        disabled = self._running or not self._mod_list
        if disabled:
            imgui.begin_disabled()
        if imgui.begin_table("##import_btns", 5, imgui.TableFlags_.sizing_stretch_same.value):
            for i in range(5):
                imgui.table_setup_column(f"C{i}", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_next_row()
            imgui.table_set_column_index(0)
            if imgui.button("Import CK Changes##import", _btn):
                self._on_import()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip("Serialize .esp back to source YAML")

            imgui.table_set_column_index(1)
            if imgui.button("Import Loose Assets##import", _btn):
                self._on_import_loose()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Pull changes from loose-deployed assets back into the\n"
                    "mod folder. Updates files that changed and detects new\n"
                    "files the CK added inside mod-owned directories (e.g. a\n"
                    "new texture under Textures/<mod>/)."
                )
            imgui.end_table()
        if disabled:
            imgui.end_disabled()

    def _draw_release_tab(self):
        if self._selected_mod_kind() == "xse":
            xse_name = _xse_name_for(os.path.join(MODS_DIR, self._selected_mod()))
            imgui.text_disabled(f"Not applicable for {xse_name}-only plugin entries.")
            imgui.text_disabled(f"Select the combined [{xse_name}+ESP] entry or a standard mod.")
            return

        imgui.text_colored(
            imgui.ImVec4(0.67, 0.67, 0.67, 1.0),
            "Package the mod for distribution. Creates a release zip containing the .esp,",
        )
        imgui.text_colored(
            imgui.ImVec4(0.67, 0.67, 0.67, 1.0),
            "BA2 archives, and any checked options below.",
        )
        imgui.spacing()

        _btn = imgui.ImVec2(-1, 0)

        mod = self._selected_mod()
        mod_dir = os.path.join(MODS_DIR, mod) if mod else ""
        release_dir = os.path.join(mod_dir, "release") if mod_dir else ""
        strings_dir = os.path.join(mod_dir, "Strings") if mod_dir else ""
        tracked_version = latest_tracked_version(release_dir) if release_dir else ""
        has_strings = (
            os.path.isdir(strings_dir) and any(
                f for f in os.listdir(strings_dir)
                if f.upper().endswith((".STRINGS", ".DLSTRINGS", ".ILSTRINGS"))
            )
        ) if strings_dir and os.path.isdir(strings_dir) else False

        is_fo4 = self._get_mod_game() == "fo4"
        has_ck = self._has_creation_kit()
        previs_enabled = is_fo4 and has_ck

        if imgui.begin_table("##release_split", 2, imgui.TableFlags_.sizing_stretch_prop.value):
            imgui.table_setup_column("Opts", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_setup_column("Tex", imgui.TableColumnFlags_.width_fixed.value, TEX_OPTIONS_COL_W)
            imgui.table_next_row()

            # ── Left: vertical checkboxes ────────────────────────────────────
            imgui.table_set_column_index(0)
            _, self._release_localize = imgui.checkbox("Localize", self._release_localize)
            if imgui.is_item_hovered():
                imgui.set_tooltip(
                    "Include Strings/ files in the release zip for localized in-game text."
                    if has_strings else
                    "No Strings/ files yet — enabling this will run Translate first,\n"
                    "then include the generated files in the release zip."
                )
            _, self._release_xbox = imgui.checkbox("Xbox BA2s", self._release_xbox)
            if imgui.is_item_hovered():
                imgui.set_tooltip(
                    "Create Xbox-format BA2 archives with tiled textures (_xbox suffix).\n"
                    "Uses xtexconv to tile DDS textures for Xbox hardware."
                )
            _, self._release_expanded_archives = imgui.checkbox(
                "Expanded BA2s",
                self._release_expanded_archives,
            )
            if imgui.is_item_hovered():
                imgui.set_tooltip(
                    "Use family archive labels such as Meshes, Sounds, and Scripts.\n"
                    "Off keeps small mods in Main + Textures when they fit."
                )
            _, self._release_create_xwm = imgui.checkbox("Create XWM", self._release_create_xwm)
            if imgui.is_item_hovered():
                imgui.set_tooltip(
                    "Convert non-voice WAV sound effects to XWM format before packaging.\n"
                    "WAV files with loop points or cue markers are kept as-is."
                )
            if not is_fo4:
                imgui.begin_disabled()
            _, self._release_create_fuz = imgui.checkbox("Create LIP/FUZ", self._release_create_fuz)
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Generate LIP sync + FUZ archives from voice WAVs before packaging.\n"
                    "Requires dialogue transcript text in YAML responses."
                    if is_fo4 else "LIP/FUZ generation is only available for Fallout 4 mods"
                )
            if not is_fo4:
                self._release_create_fuz = False
                imgui.end_disabled()
            if not previs_enabled:
                imgui.begin_disabled()
            _, self._release_previs = imgui.checkbox("PreCombines/PreVis", self._release_previs)
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                if not is_fo4:
                    imgui.set_tooltip("PreVis generation is only available for Fallout 4 mods.")
                elif not has_ck:
                    imgui.set_tooltip(
                        "PreVis generation requires Creation Kit.\n"
                        "Set the Fallout 4 path in Settings \u2192 Paths."
                    )
                else:
                    imgui.set_tooltip(
                        "Generate precombined meshes and previsibility data via Creation Kit.\n"
                        "This may take several minutes."
                    )
            _, self._release_anim_data = imgui.checkbox("Generate Anim Data", self._release_anim_data)
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                if not is_fo4:
                    imgui.set_tooltip("Anim data generation is only available for Fallout 4 mods.")
                elif not has_ck:
                    imgui.set_tooltip(
                        "Anim data generation requires Creation Kit.\n"
                        "Set the Fallout 4 path in Settings \u2192 Paths."
                    )
                else:
                    imgui.set_tooltip(
                        "Generate animation info data via Creation Kit (-GenerateAnimInfo).\n"
                        "Output goes to data/ inside the mod folder."
                    )
            if not previs_enabled:
                self._release_previs = False
                self._release_anim_data = False
                imgui.end_disabled()

            # ── Right: texture resolution form ───────────────────────────────
            imgui.table_set_column_index(1)
            if begin_form("##release_tex"):
                _, self._release_pc_max_res_idx = draw_combo_field("PC Max", _PC_RES_OPTIONS, self._release_pc_max_res_idx)
                if imgui.is_item_hovered():
                    imgui.set_tooltip("Max texture dimension for PC archives (0 = no resize)")
                release_pc_effects_idx = (
                    self._release_pc_effects_max_res_idx
                    if self._release_pc_effects_max_res_idx is not None
                    else self._release_pc_max_res_idx
                )
                changed, release_pc_effects_idx = draw_combo_field("PC Effects", _PC_RES_OPTIONS, release_pc_effects_idx)
                if changed:
                    self._release_pc_effects_max_res_idx = release_pc_effects_idx
                if imgui.is_item_hovered():
                    imgui.set_tooltip(
                        "Max texture dimension for Textures/Effects in the PC archive.\n"
                        "Use this to compress effects more than the rest of the texture set."
                    )
                if self._release_xbox:
                    _, self._release_xbox_max_res_idx = draw_combo_field("Xbox Max", _XBOX_RES_OPTIONS, self._release_xbox_max_res_idx)
                    if imgui.is_item_hovered():
                        imgui.set_tooltip("Max texture dimension for Xbox BA2s")
                    release_xbox_effects_idx = (
                        self._release_xbox_effects_max_res_idx
                        if self._release_xbox_effects_max_res_idx is not None
                        else self._release_xbox_max_res_idx
                    )
                    changed, release_xbox_effects_idx = draw_combo_field(
                        "Xbox Effects", _XBOX_RES_OPTIONS, release_xbox_effects_idx
                    )
                    if changed:
                        self._release_xbox_effects_max_res_idx = release_xbox_effects_idx
                    if imgui.is_item_hovered():
                        imgui.set_tooltip(
                            "Max texture dimension for Textures/Effects in the Xbox archive.\n"
                            "Use this to compress Xbox effects more than the rest of the texture set."
                        )
                self._draw_archive_max_size_field()
                self._draw_asset_workers_field()
                end_form()

            imgui.end_table()

        imgui.separator()

        detected_version_text = tracked_version or "none"
        current_mod_version = self._current_mod_version or "none"
        imgui.text_colored(
            imgui.ImVec4(0.67, 0.67, 0.67, 1.0),
            f"Current mod version: {current_mod_version}",
        )
        imgui.text_colored(
            imgui.ImVec4(0.67, 0.67, 0.67, 1.0),
            f"Tracked release version: {detected_version_text}",
        )
        if begin_form("##release_meta"):
            _, self._release_version = draw_text_field("Release Version", self._release_version)
            if imgui.is_item_hovered():
                imgui.set_tooltip(
                    "Optional version label stored in release history and notes.\n"
                    "Leave blank to keep the release unversioned."
                )
            imgui.table_next_row()
            imgui.table_set_column_index(0)
            imgui.align_text_to_frame_padding()
            imgui.text("Release Notes")
            imgui.table_set_column_index(1)
            imgui.set_next_item_width(-1)
            _, self._release_notes = imgui.input_text_multiline(
                "##release_notes",
                self._release_notes,
                imgui.ImVec2(-1, 110),
            )
            end_form()

        imgui.separator()

        disabled = self._running or not self._mod_list
        if disabled:
            imgui.begin_disabled()
        if imgui.begin_table("##release_btns", 5, imgui.TableFlags_.sizing_stretch_same.value):
            for i in range(5):
                imgui.table_setup_column(f"C{i}", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_next_row()
            imgui.table_set_column_index(0)
            if imgui.button("Package Release##release", _btn):
                self._on_release()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip("Create a distributable zip of the mod")
            imgui.end_table()
        if disabled:
            imgui.end_disabled()

        # ── Release Log ──
        imgui.spacing()
        imgui.separator()
        imgui.spacing()

        # Header with controls
        imgui.text("Release Log")
        if self._release_active:
            imgui.same_line()
            imgui.text_colored(imgui.ImVec4(0.3, 0.8, 0.3, 1.0), "(running)")
        if self._release_log:
            imgui.same_line(imgui.get_content_region_avail().x - 50)
            if imgui.button("Clear##release_log"):
                self._release_log.clear()

        # Count warnings/errors for summary
        n_warn = sum(1 for lvl, _ in self._release_log if lvl == logging.WARNING)
        n_err = sum(1 for lvl, _ in self._release_log if lvl >= logging.ERROR)
        if n_err:
            imgui.text_colored(imgui.ImVec4(1.0, 0.3, 0.3, 1.0), f"{n_err} error(s)")
            imgui.same_line()
        if n_warn:
            imgui.text_colored(imgui.ImVec4(1.0, 0.8, 0.2, 1.0), f"{n_warn} warning(s)")

        # Scrollable log region
        _LOG_COLORS = {
            logging.DEBUG: imgui.ImVec4(0.5, 0.5, 0.5, 1.0),
            logging.INFO: imgui.ImVec4(0.8, 0.8, 0.8, 1.0),
            logging.WARNING: imgui.ImVec4(1.0, 0.8, 0.2, 1.0),
            logging.ERROR: imgui.ImVec4(1.0, 0.3, 0.3, 1.0),
            logging.CRITICAL: imgui.ImVec4(1.0, 0.2, 0.2, 1.0),
        }
        imgui.begin_child("release_log_scroll", imgui.ImVec2(0, 0))
        if not self._release_log:
            imgui.text_colored(
                imgui.ImVec4(0.5, 0.5, 0.5, 1.0),
                "Click 'Package Release' to start. Output will appear here.",
            )
        else:
            for level, text in self._release_log:
                color = _LOG_COLORS.get(level, _LOG_COLORS[logging.INFO])
                # Prefix warnings/errors with a level tag
                if level >= logging.ERROR:
                    imgui.text_colored(color, f"[ERROR] {text}")
                elif level == logging.WARNING:
                    imgui.text_colored(color, f"[WARN]  {text}")
                else:
                    imgui.text_colored(color, text)
            if self._release_log_auto_scroll:
                if imgui.get_scroll_y() >= imgui.get_scroll_max_y() - 10:
                    imgui.set_scroll_here_y(1.0)
        imgui.end_child()

    def _draw_migrate_tab(self):
        _game_ids = list(GAME_PROFILES.keys())
        _game_labels = [GAME_PROFILES[g].display_name for g in _game_ids]
        _btn = imgui.ImVec2(-1, 0)

        if begin_form("##migrate_form"):
            _, clicked = draw_path_row("Source Dir", self._migrate_src_dir)
            if clicked:
                path = pick_folder("Select Mod Folder to Migrate")
                if path:
                    self._migrate_src_dir = path
            if imgui.is_item_hovered():
                imgui.set_tooltip("Full path to the mod folder to migrate, e.g. C:/Mods/B21_PlasmaCaster")

            _, self._migrate_name_override = draw_text_field("Name Override", self._migrate_name_override)
            if imgui.is_item_hovered():
                imgui.set_tooltip("Optional. Defaults to the source folder basename.")

            _, self._migrate_game_idx = draw_combo_field("Game", _game_labels, self._migrate_game_idx)
            if imgui.is_item_hovered():
                imgui.set_tooltip("Game the source mod was built for — sets the .game file and serializer")

            end_form()

        imgui.separator()
        _, self._migrate_add_prefix = imgui.checkbox(
            f"Add {self.mod_prefix}_ prefix##migrate_prefix", self._migrate_add_prefix
        )
        if imgui.is_item_hovered():
            imgui.set_tooltip(
                f"Prepend {self.mod_prefix}_ to the mod name if it doesn't already start with it.\n"
                f"Uncheck if the source folder is already prefixed."
            )
        imgui.same_line()
        _, self._migrate_git_repo = imgui.checkbox("Create Git repo##migrate", self._migrate_git_repo)
        if imgui.is_item_hovered():
            imgui.set_tooltip("Initialize a git repo and push to Gitea")
        imgui.separator()

        disabled = self._running or not self._migrate_src_dir.strip()
        if disabled:
            imgui.begin_disabled()
        if imgui.begin_table("##migrate_btns", 5, imgui.TableFlags_.sizing_stretch_same.value):
            for i in range(5):
                imgui.table_setup_column(f"C{i}", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_next_row()
            imgui.table_set_column_index(0)
            if imgui.button("Migrate Mod##migrate", _btn):
                self._on_migrate()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip("Run migrate_mod.sh on the source directory")
            imgui.end_table()
        if disabled:
            imgui.end_disabled()

    def _draw_utils_tab(self):
        is_fo4 = self._get_mod_game() == "fo4"
        has_ck = self._has_creation_kit()
        base_disabled = self._running or not self._mod_list
        git_disabled = base_disabled or not self._selected_mod_has_git_repo()
        no_git_disabled = base_disabled or self._selected_mod_has_git_repo()
        fo4_ck_disabled = base_disabled or not is_fo4 or not has_ck
        translate_disabled = self._utils_translate_worker is not None or not self._mod_list
        _btn = imgui.ImVec2(-1, 0)

        # ── Main utility grid ────────────────────────────────────────────────
        if imgui.begin_table("##utils_cols", 4, imgui.TableFlags_.sizing_stretch_same.value):
            imgui.table_setup_column("Tools", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_setup_column("Text", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_setup_column("Plugin", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_setup_column("Git", imgui.TableColumnFlags_.width_stretch.value)
            imgui.table_next_row()

            # Left column: preview/anim + audio utilities
            imgui.table_set_column_index(0)
            if fo4_ck_disabled:
                imgui.begin_disabled()
            if imgui.button("Generate PreVis##utils_previs", _btn):
                self._on_utils_previs()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                if not is_fo4:
                    imgui.set_tooltip("PreVis generation is only available for Fallout 4 mods.")
                elif not has_ck:
                    imgui.set_tooltip(
                        "PreVis generation requires Creation Kit.\n"
                        "Set the Fallout 4 path in Settings \u2192 Paths."
                    )
                else:
                    imgui.set_tooltip(
                        "Generate precombined meshes and previsibility data via Creation Kit.\n"
                        "Output goes to data/Meshes/PreCombined/ and data/Vis/."
                    )
            if imgui.button("Generate Anim Data##utils_anim_data", _btn):
                self._on_utils_anim_data()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                if not is_fo4:
                    imgui.set_tooltip("Anim data generation is only available for Fallout 4 mods.")
                elif not has_ck:
                    imgui.set_tooltip(
                        "Anim data generation requires Creation Kit.\n"
                        "Set the Fallout 4 path in Settings \u2192 Paths."
                    )
                else:
                    imgui.set_tooltip(
                        "Generate animation info data via Creation Kit (-GenerateAnimInfo).\n"
                        "Output goes to data/ inside the mod folder."
                    )
            if fo4_ck_disabled:
                imgui.end_disabled()

            # CK-free native AnimTextData — no Creation Kit required (only fo4 + a mod).
            native_anim_disabled = base_disabled or not is_fo4
            if native_anim_disabled:
                imgui.begin_disabled()
            if imgui.button("Native Anim Data##utils_native_anim", _btn):
                self._on_utils_native_anim_data()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                if not is_fo4:
                    imgui.set_tooltip("Anim data generation is only available for Fallout 4 mods.")
                else:
                    imgui.set_tooltip(
                        "Generate AnimTextData without Creation Kit (native CK-free generator).\n"
                        "Writes the AnimationFileData bucket into data/Meshes/AnimTextData.\n"
                        "Resolves clips against the mod's meshes first (overrides win), then\n"
                        "the FO4 base meshes (Settings → Paths → extracted_dir) for weapon/\n"
                        "character subgraphs that reference base-game behaviors."
                    )
            if native_anim_disabled:
                imgui.end_disabled()

            imgui.spacing()
            if base_disabled:
                imgui.begin_disabled()
            if imgui.button("Build Archlist##utils_archlist", _btn):
                self._on_utils_build_archlist()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                if not self._mod_list:
                    imgui.set_tooltip("Select a mod first.")
                elif self._running:
                    imgui.set_tooltip("Wait for the current operation to finish.")
                else:
                    imgui.set_tooltip(
                        "Build a Data-relative .archlist for the selected mod's loose files.\n"
                        "Includes data/ (including compiled Scripts/.pex), Meshes/ (.hkx only),\n"
                        "and Strings/."
                    )
            if base_disabled:
                imgui.end_disabled()

            if base_disabled or not is_fo4:
                imgui.begin_disabled()
            if imgui.button("Create LIP/FUZ##utils_lip_fuz", _btn):
                self._utils_create_fuz = True
                self._utils_create_xwm = False
                self._on_utils_audio()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Generate LIP sync + FUZ archives from voice WAVs.\n"
                    "Requires dialogue transcript text in YAML responses."
                    if is_fo4 else "LIP/FUZ generation is only available for Fallout 4 mods"
                )

            if imgui.button("Create XWM##utils_xwm", _btn):
                self._utils_create_fuz = False
                self._utils_create_xwm = True
                self._on_utils_audio()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Convert non-voice WAV sound effects to XWM format.\n"
                    "WAV files with loop points or cue markers are kept as-is."
                )
            if base_disabled or not is_fo4:
                imgui.end_disabled()

            # Middle column: text / spelling / dialogue tools
            imgui.table_set_column_index(1)
            if translate_disabled:
                imgui.begin_disabled()
            lbl = "Translating...##utils_translate" if self._utils_translate_worker else "Translate Strings##utils_translate"
            if imgui.button(lbl, _btn):
                self._on_utils_translate()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Auto-translate all Name/Description fields to 11 languages using NLLB-200.\n"
                    "Sets the Localized flag in plugin.yaml. Use 'Build ESP' in Deploy tab after translating."
            )
            if translate_disabled:
                imgui.end_disabled()

            if base_disabled:
                imgui.begin_disabled()
            if imgui.button("Run Spellcheck##utils_spellcheck", _btn):
                self._on_spellcheck()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip("Scan YAML files for spelling/grammar issues (uses LanguageTool)")
            if imgui.button("Manage Dictionary##utils_dictionary", _btn):
                self._dict_popup_open = True
            if base_disabled:
                imgui.end_disabled()

            if fo4_ck_disabled:
                imgui.begin_disabled()
            if imgui.button("Export Dialogue##utils_dialogue", _btn):
                self._on_utils_export_dialogue()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                if not is_fo4:
                    imgui.set_tooltip("Dialogue export is only available for Fallout 4 mods.")
                elif not has_ck:
                    imgui.set_tooltip(
                        "Dialogue export requires Creation Kit.\n"
                        "Set the Fallout 4 path in Settings \u2192 Paths."
                    )
                else:
                    imgui.set_tooltip(
                        "Export dialogue lines for this mod to dialogue_export.txt\n"
                        "via Creation Kit (-ExportDialogue)."
                    )
            if fo4_ck_disabled:
                imgui.end_disabled()

            # Third column: plugin conversion tools
            imgui.table_set_column_index(2)
            if base_disabled:
                imgui.begin_disabled()
            if imgui.button("Convert to ESP##utils_convert_esp", _btn):
                self._on_utils_convert_plugin_type("esp")
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Rewrite plugin.yaml as .esp and clear the LightPlugin flag."
                )
            if imgui.button("Convert to ESL##utils_convert_esl", _btn):
                self._on_utils_convert_plugin_type("esl")
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Rewrite plugin.yaml as .esl and add the LightPlugin flag."
                )
            if imgui.button("Convert to ESM##utils_convert_esm", _btn):
                self._on_utils_convert_plugin_type("esm")
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Rewrite plugin.yaml as .esm and clear the LightPlugin flag."
                )
            if imgui.button("Tag as Light##utils_tag_light", _btn):
                self._on_utils_tag_light()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Add the LightPlugin flag to the selected .esp plugin without changing its extension."
                )
            if imgui.button("Remove Light Tag##utils_remove_light_tag", _btn):
                self._on_utils_remove_light_tag()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Remove the LightPlugin flag from the selected plugin without changing its extension."
                )
            if imgui.button("Tag as Master##utils_tag_master", _btn):
                self._on_utils_tag_master()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Add the MasterFile flag to the selected plugin without changing its extension."
                )
            if imgui.button("Remove Master Flag##utils_remove_master_flag", _btn):
                self._on_utils_remove_master_flag()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Remove the MasterFile flag from the selected plugin without changing its extension."
                )
            if base_disabled:
                imgui.end_disabled()

            # Right column: git commands
            imgui.table_set_column_index(3)
            if no_git_disabled:
                imgui.begin_disabled()
            if imgui.button("Create Git##utils_git_create", _btn):
                self._on_utils_git_create()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "This mod already has a local Git repository."
                    if self._selected_mod_has_git_repo() else
                    "Initialize a git repo for this mod and push to Gitea."
                )
            if no_git_disabled:
                imgui.end_disabled()

            if git_disabled:
                imgui.begin_disabled()
            if imgui.button("Commit to Git##utils_git_commit", _btn):
                self._on_utils_git_commit()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Stage all local changes, create an automatic commit, and push it to origin."
                    if self._selected_mod_has_git_repo() else
                    "This mod does not have a local Git repository yet."
                )
            if git_disabled:
                imgui.end_disabled()

            if git_disabled:
                imgui.begin_disabled()
            if imgui.button("Pull From Git##utils_git_pull", _btn):
                self._on_utils_git_pull()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Pull the latest remote changes into this mod repository (fast-forward only)."
                    if self._selected_mod_has_git_repo() else
                    "This mod does not have a local Git repository yet."
                )
            if git_disabled:
                imgui.end_disabled()

            if git_disabled:
                imgui.begin_disabled()
            if imgui.button("Checkout from Git##utils_git_checkout", _btn):
                self._on_utils_git_checkout()
            if imgui.is_item_hovered(imgui.HoveredFlags_.allow_when_disabled.value):
                imgui.set_tooltip(
                    "Discard all local tracked and untracked changes in this mod repository."
                    if self._selected_mod_has_git_repo() else
                    "This mod does not have a local Git repository yet."
                )
            if git_disabled:
                imgui.end_disabled()

            imgui.end_table()

        self._draw_dictionary_popup()

        # ── Spellcheck results (below the grid) ──────────────────────────────
        visible_results = [
            (i, r) for i, r in enumerate(self._spellcheck_results)
            if i not in self._spellcheck_ignored
        ]

        if self._spellcheck_results:
            imgui.separator()
            total = len(self._spellcheck_results)
            ignored = len(self._spellcheck_ignored)
            imgui.text(f"Spellcheck: {total - ignored} issue(s) ({ignored} ignored)")
            if visible_results:
                imgui.same_line(400)
                if imgui.small_button("Fix All"):
                    self._on_fix_all()
            imgui.spacing()
            flags = (imgui.TableFlags_.borders_inner.value
                     | imgui.TableFlags_.row_bg.value
                     | imgui.TableFlags_.resizable.value
                     | imgui.TableFlags_.scroll_y.value)
            if imgui.begin_table("##spellcheck_results", 5, flags, imgui.ImVec2(0, 300)):
                imgui.table_setup_column("File", imgui.TableColumnFlags_.width_fixed.value, 180)
                imgui.table_setup_column("Field", imgui.TableColumnFlags_.width_fixed.value, 120)
                imgui.table_setup_column("Issue", imgui.TableColumnFlags_.width_stretch.value)
                imgui.table_setup_column("Suggestion", imgui.TableColumnFlags_.width_fixed.value, 120)
                imgui.table_setup_column("Actions", imgui.TableColumnFlags_.width_fixed.value, 130)
                imgui.table_headers_row()
                for idx, issue in visible_results:
                    imgui.table_next_row()
                    imgui.table_next_column()
                    short_file = os.path.basename(issue.file)
                    imgui.text(short_file)
                    if imgui.is_item_hovered():
                        imgui.set_tooltip(issue.file)
                    imgui.table_next_column()
                    imgui.text(issue.field)
                    imgui.table_next_column()
                    imgui.text_colored(imgui.ImVec4(1.0, 0.4, 0.4, 1.0), f'"{issue.error_text}"')
                    if imgui.is_item_hovered():
                        imgui.set_tooltip(issue.message)
                    imgui.table_next_column()
                    if issue.suggestions:
                        imgui.text_colored(imgui.ImVec4(0.4, 1.0, 0.4, 1.0), issue.suggestions[0])
                    else:
                        imgui.text_disabled("—")
                    imgui.table_next_column()
                    if issue.suggestions:
                        if imgui.small_button(f"Fix##{idx}"):
                            self._on_fix_issue(idx, issue.suggestions[0])
                        imgui.same_line()
                    if imgui.small_button(f"Ignore##{idx}"):
                        self._spellcheck_ignored.add(idx)
                    imgui.same_line()
                    if imgui.small_button(f"+Dict##{idx}"):
                        self._on_add_to_dict(idx)
                imgui.end_table()

    def _draw_dictionary_popup(self):
        if self._dict_popup_open:
            imgui.open_popup("##dict_popup")
            self._dict_popup_open = False
        if imgui.begin_popup("##dict_popup"):
            imgui.text("Custom Dictionary")
            imgui.separator()
            imgui.push_item_width(200)
            _, self._dict_new_term = imgui.input_text("##new_term", self._dict_new_term, 128)
            imgui.pop_item_width()
            imgui.same_line()
            if imgui.button("Add Term"):
                term = self._dict_new_term.strip()
                if term:
                    from creation_lib.mod.spellcheck import add_to_dictionary
                    add_to_dictionary(term)
                    self._dict_new_term = ""
                    _log.info("Added '%s' to dictionary", term)
            imgui.separator()
            from creation_lib.mod.spellcheck import get_dictionary_terms, remove_from_dictionary
            terms = get_dictionary_terms()
            if terms:
                imgui.text(f"{len(terms)} terms:")
                child_h = min(300.0, len(terms) * 20.0 + 10)
                if imgui.begin_child("##dict_list", imgui.ImVec2(300, child_h), child_flags=imgui.ChildFlags_.borders.value):
                    for t in terms:
                        imgui.text(t)
                        imgui.same_line(260)
                        if imgui.small_button(f"X##{t}"):
                            remove_from_dictionary(t)
                imgui.end_child()
            else:
                imgui.text("(empty)")
            imgui.end_popup()

    # ── Spellcheck actions ─────────────────────────────────────────────────

    def _on_spellcheck(self):
        mod = self._selected_mod()
        if not mod:
            return
        yaml_dir = os.path.join(MODS_DIR, mod, "yaml")
        if not os.path.isdir(yaml_dir):
            _log.warning("No yaml/ directory for %s", mod)
            return
        self._running = True
        self._loading_label = f"Spell checking {mod}..."
        self._spellcheck_results = []
        self._spellcheck_ignored.clear()
        _log.info("Running spellcheck on %s...", mod)

        def _run():
            from creation_lib.mod.spellcheck import HAS_LANGUAGE_TOOL, check_mod_text
            if not HAS_LANGUAGE_TOOL:
                _log.error("language_tool_python not installed. Run: uv add language-tool-python")
                return []
            return check_mod_text(yaml_dir)

        self._spellcheck_worker = AsyncWorker(target_fn=_run)
        self._spellcheck_worker.start()

    def _on_fix_issue(self, idx: int, replacement: str):
        """Apply a single spellcheck fix."""
        if idx >= len(self._spellcheck_results):
            return
        issue = self._spellcheck_results[idx]
        mod = self._selected_mod()
        if not mod:
            return
        yaml_dir = os.path.join(MODS_DIR, mod, "yaml")
        from creation_lib.mod.spellcheck import apply_fix
        if apply_fix(yaml_dir, issue, replacement):
            self._spellcheck_ignored.add(idx)
        else:
            _log.error("Failed to apply fix for issue #%d", idx)

    def _on_fix_all(self):
        """Apply all available fixes."""
        mod = self._selected_mod()
        if not mod:
            return
        yaml_dir = os.path.join(MODS_DIR, mod, "yaml")
        from creation_lib.mod.spellcheck import apply_fix
        fixed = 0
        for idx, issue in enumerate(self._spellcheck_results):
            if idx in self._spellcheck_ignored:
                continue
            if issue.suggestions:
                if apply_fix(yaml_dir, issue, issue.suggestions[0]):
                    self._spellcheck_ignored.add(idx)
                    fixed += 1
        _log.info("Applied %d fix(es)", fixed)

    def _on_add_to_dict(self, idx: int):
        """Add the flagged word to the custom dictionary."""
        if idx >= len(self._spellcheck_results):
            return
        issue = self._spellcheck_results[idx]
        term = issue.error_text
        from creation_lib.mod.spellcheck import add_to_dictionary
        add_to_dictionary(term)
        self._spellcheck_ignored.add(idx)
        _log.info("Added '%s' to dictionary", term)

    # ── Mod list management ─────────────────────────────────────────────────

    def _refresh_mods(self):
        # Preserve selection by (name, kind) so duplicate names stay pinned.
        old_key = ("", "")
        if self._mod_list and 0 <= self._selected_mod_idx < len(self._mod_list):
            old_key = (self._mod_list[self._selected_mod_idx],
                       self._mod_kinds[self._selected_mod_idx] if self._mod_kinds else "mod")

        in_mods: list[str] = []
        if os.path.isdir(MODS_DIR):
            in_mods = sorted(d for d in os.listdir(MODS_DIR)
                             if os.path.isdir(os.path.join(MODS_DIR, d)) and not d.startswith("."))

        # Single source: mods/. Kind is derived from folder contents.
        entries: list[tuple[str, str]] = [
            (name, _mod_kind(os.path.join(MODS_DIR, name))) for name in in_mods
        ]

        self._mod_list = [e[0] for e in entries]
        self._mod_kinds = [e[1] for e in entries]
        self._refresh_deployed_state()

        try:
            self._selected_mod_idx = next(
                i for i, e in enumerate(entries) if e == old_key
            )
        except StopIteration:
            self._selected_mod_idx = 0
        self._on_mod_changed()

    def _refresh_deployed_state(self):
        self._mod_deployed = []
        for idx, mod in enumerate(self._mod_list):
            kind = self._mod_kinds[idx] if idx < len(self._mod_kinds) else "mod"
            mod_dir = os.path.join(MODS_DIR, mod)
            game = _read_mod_game(mod_dir, self.active_game)
            game_data_dir = self._game_data_path_for_game(game)
            self._mod_deployed.append(_is_mod_deployed(mod_dir, mod, kind, game_data_dir))

    def _reset_progress_state(self, label: str):
        self._loading_label = label
        self._progress_fraction = None
        self._progress_message = "Starting..."
        self._progress_lines = []

    def _record_progress_line(self, line: str):
        message = line.strip()
        if not message:
            return
        self._progress_message = message
        self._progress_lines.append(message)
        self._progress_lines = self._progress_lines[-_PROGRESS_LINE_LIMIT:]
        fraction = _progress_fraction_from_line(message)
        if fraction is not None:
            self._progress_fraction = fraction

    def _on_mod_changed(self):
        if not self._mod_list or self._selected_mod_idx >= len(self._mod_list):
            self._info_text = "No mod selected"
            return
        mod = self._mod_list[self._selected_mod_idx]
        kind = self._selected_mod_kind()

        if kind == "xse":
            self._on_mod_changed_xse(mod)
            return

        mod_dir = os.path.join(MODS_DIR, mod)
        has_yaml = os.path.isdir(os.path.join(mod_dir, "yaml"))
        self._current_mod_version = read_mod_version(mod_dir)
        self._release_version = self._current_mod_version
        self._release_notes = ""
        plugin_ext = _plugin_ext(mod_dir)
        has_esp = os.path.isfile(os.path.join(mod_dir, f"{mod}.{plugin_ext}"))
        has_data = os.path.isdir(os.path.join(mod_dir, "data"))
        has_scripts = (
            os.path.isdir(os.path.join(mod_dir, "Scripts", "Source", "User"))
            or os.path.isdir(os.path.join(mod_dir, "scripts"))
        )
        archives = discover_mod_archives(Path(mod_dir), mod)
        # Read target game from .game file
        game_file = os.path.join(mod_dir, ".game")
        game_label = ""
        if os.path.isfile(game_file):
            try:
                gid = open(game_file).read().strip()
                profile = GAME_PROFILES.get(gid)
                game_label = profile.display_name if profile else gid
            except Exception:
                pass

        parts = [mod]
        if game_label:
            parts.append(game_label)
        if self._current_mod_version:
            parts.append(f"Version: {self._current_mod_version}")
        if has_yaml:
            parts.append("YAML")
        if has_esp:
            parts.append(f"{plugin_ext.upper()}: {_fmt_size(os.path.getsize(os.path.join(mod_dir, f'{mod}.{plugin_ext}')))}")
        else:
            parts.append("Plugin: not built")
        if has_scripts:
            parts.append("Scripts: yes")
        if has_data:
            parts.append("Loose data")
        for archive in archives:
            parts.append(f"{archive.name}: {_fmt_size(archive.stat().st_size)}")
        self._info_text = " | ".join(parts)

        # Auto-enable Localize checkbox if Strings/ dir has files
        strings_dir = os.path.join(mod_dir, "Strings")
        has_strings = os.path.isdir(strings_dir) and any(
            f for f in os.listdir(strings_dir)
            if f.upper().endswith((".STRINGS", ".DLSTRINGS", ".ILSTRINGS"))
        ) if os.path.isdir(strings_dir) else False
        if has_strings and not self._release_localize:
            self._release_localize = True

    def _on_mod_changed_xse(self, mod: str):
        """Info text for a pure xse plugin entry (mods/<name>/ with src + xmake.lua, no esp)."""
        self._current_mod_version = ""
        self._release_version = ""
        self._release_notes = ""
        src_dir = os.path.join(MODS_DIR, mod)
        xse_name = _xse_name_for(src_dir)
        dll_path = os.path.join(MODS_DIR, mod, xse_name, "Plugins", f"{mod}.dll")
        parts = [f"{mod} [{xse_name} src]"]
        game_file = os.path.join(src_dir, ".game")
        if os.path.isfile(game_file):
            try:
                gid = open(game_file).read().strip()
                profile = GAME_PROFILES.get(gid)
                parts.append(profile.display_name if profile else gid)
            except Exception:
                pass
        if os.path.isfile(dll_path):
            parts.append(f"DLL: {_fmt_size(os.path.getsize(dll_path))}")
        else:
            parts.append("DLL: not built")
        has_xmake = os.path.isfile(os.path.join(src_dir, "xmake.lua"))
        has_cmake = os.path.isfile(os.path.join(src_dir, "CMakeLists.txt"))
        if has_xmake:
            parts.append("Build: xmake")
        elif has_cmake:
            parts.append("Build: cmake")
        self._info_text = " | ".join(parts)

    def _selected_mod(self) -> str:
        if self._mod_list and 0 <= self._selected_mod_idx < len(self._mod_list):
            return self._mod_list[self._selected_mod_idx]
        return ""

    def _selected_mod_kind(self) -> str:
        """Return "mod", "xse", or "combined" for the selected entry."""
        if self._mod_kinds and 0 <= self._selected_mod_idx < len(self._mod_kinds):
            return self._mod_kinds[self._selected_mod_idx]
        return "mod"

    def _is_selected_xse(self) -> bool:
        return self._selected_mod_kind() in ("xse", "combined")

    def _selected_mod_dir(self) -> str:
        """Primary working directory: mods/<name>/ for every entry kind."""
        mod = self._selected_mod()
        if not mod:
            return ""
        return os.path.join(MODS_DIR, mod)

    def _selected_xse_src_dir(self) -> str:
        """Path to mods/<name>/ when the selected entry has an xse plugin (xse or combined)."""
        mod = self._selected_mod()
        kind = self._selected_mod_kind()
        if mod and kind in ("xse", "combined"):
            return os.path.join(MODS_DIR, mod)
        return ""

    def _selected_mod_has_git_repo(self) -> bool:
        mod_dir = self._selected_mod_dir()
        return bool(mod_dir) and os.path.isdir(os.path.join(mod_dir, ".git"))

    def _get_mod_game(self) -> str:
        """Read the .game file for the selected mod (mods/<name>/.game)."""
        mod = self._selected_mod()
        if not mod:
            return self.active_game
        return _read_mod_game(os.path.join(MODS_DIR, mod), self.active_game)

    def _has_creation_kit(self) -> bool:
        """Check if CreationKit.exe exists for the current mod's game."""
        game = self._get_mod_game()
        if game != "fo4":
            return False
        try:
            data_dir = self._resolve_game_data_path(game)
        except RuntimeError:
            return False
        return os.path.isfile(os.path.join(os.path.dirname(str(data_dir)), "CreationKit.exe"))

    # ── Actions ─────────────────────────────────────────────────────────────

    def _on_setup(self):
        suffix = self._new_mod_name.strip()
        if not suffix:
            _log.warning("Enter a mod name first.")
            return
        mod_name = f"{self.mod_prefix}_{suffix}" if not suffix.startswith(f"{self.mod_prefix}_") else suffix
        gitea_env = self._get_gitea_env(self._create_git_repo)
        game = self.active_game
        prefix = self.mod_prefix
        plugin_ext = _PLUGIN_TYPES[self._create_plugin_type_idx]

        def _do(on_progress):
            from app.paths import get_app_root
            from creation_lib.mod.scaffold import create_mod
            create_mod(
                mod_name, game=game, mod_prefix=prefix,
                plugin_ext=plugin_ext,
                init_git=self._create_git_repo,
                gitea_url=gitea_env.get("GITEA_URL", ""),
                gitea_user=gitea_env.get("GITEA_USER", ""),
                gitea_org=gitea_env.get("GITEA_ORG", ""),
                gitea_token=gitea_env.get("GITEA_TOKEN", ""),
                project_root=get_app_root(),
                on_progress=on_progress,
            )

        self._run_fn(
            _do,
            on_done=lambda: (self._refresh_mods(), setattr(self, "_new_mod_name", "")),
            description=f"Creating mod {mod_name}",
        )

    def _on_delete_mod(self):
        mod = self._selected_mod()
        if not mod:
            return
        kind = self._selected_mod_kind()
        mod_dir = self._selected_mod_dir()
        also_gitea = self._delete_also_gitea
        also_addon_nodes = self._delete_also_addon_nodes and kind != "xse"
        creds = self._gitea_creds()

        def _do(on_progress):
            import shutil
            import stat
            from pathlib import Path
            if also_gitea:
                on_progress(f"Deleting remote Gitea repo for {mod}...")
                from creation_lib.mod.git_ops import gitea_delete_repo
                gitea_delete_repo(Path(mod_dir), gitea_token=creds.get("gitea_token", ""))
            if also_addon_nodes:
                on_progress("Removing AddonNode allocations from global registry...")
                try:
                    from creation_lib.addon_registry import AddonNodeRegistry
                    registry = AddonNodeRegistry(MODS_DIR)
                    registry.load()
                    removed = registry.release_mod(mod)
                    if removed:
                        on_progress(f"Removed {removed} AddonNode allocation(s).")
                except Exception as e:
                    on_progress(f"Warning: failed to clean addon registry: {e}")
            on_progress(f"Deleting local directory: {mod_dir}")

            def _remove_readonly(func, path, _exc):
                os.chmod(path, stat.S_IWRITE)
                func(path)

            shutil.rmtree(mod_dir, onerror=_remove_readonly)
            on_progress(f"Deleted {mod}.")

        self._run_fn(
            _do,
            on_done=self._on_delete_mod_done,
            description=f"Deleting mod {mod}",
        )

    def _get_mod_addon_count(self, mod_name: str) -> int:
        """Return number of AddonNode allocations for a mod in the global registry."""
        try:
            from creation_lib.addon_registry import AddonNodeRegistry
            registry = AddonNodeRegistry(MODS_DIR)
            registry.load()
            return len(registry.get_mod_allocations(mod_name))
        except Exception:
            return 0

    def _on_utils_convert_plugin_type(self, new_ext: str):
        """Convert the selected mod to a different plugin extension."""
        mod = self._selected_mod()
        if not mod:
            return
        if new_ext not in _PLUGIN_TYPES:
            _log.error("Unsupported plugin type: %s", new_ext)
            return
        mod_dir = os.path.join(MODS_DIR, mod)

        def _do(on_progress):
            cur_ext = _plugin_ext(mod_dir)
            needs_light = new_ext == "esl"
            _set_plugin_type_files(mod_dir, new_ext, needs_light=needs_light)
            on_progress(
                f"Converted {mod} from .{cur_ext} to .{new_ext}"
                + (" with LightPlugin flag" if needs_light else "")
                + ". Rebuild the plugin to materialize the new extension."
            )

        self._run_fn(_do, description=f"Converting {mod} to .{new_ext}")

    def _on_utils_tag_light(self):
        """Tag the selected ESP as a light plugin."""
        mod = self._selected_mod()
        if not mod:
            return
        mod_dir = os.path.join(MODS_DIR, mod)

        def _do(on_progress):
            cur_ext = _plugin_ext(mod_dir)
            if cur_ext == "esm":
                on_progress("Tag as Light skipped — .esm plugins do not support the LightPlugin flag.")
                return
            if cur_ext == "esl":
                on_progress("Tag as Light skipped — .esl plugins already carry the LightPlugin flag.")
                return
            _set_plugin_header_flags(mod_dir, add_flags=(_HEADER_FLAG_LIGHT_NAME,))
            on_progress("Tagged the selected .esp as a light plugin.")

        self._run_fn(_do, description=f"Tagging {mod} as light")

    def _on_utils_remove_light_tag(self):
        """Remove the light-plugin flag from the selected plugin."""
        mod = self._selected_mod()
        if not mod:
            return
        mod_dir = os.path.join(MODS_DIR, mod)

        def _do(on_progress):
            cur_ext = _plugin_ext(mod_dir)
            if cur_ext == "esl":
                on_progress("Remove Light Tag skipped - convert the plugin to .esp before clearing LightPlugin.")
                return
            _set_plugin_header_flags(mod_dir, remove_flags=(_HEADER_FLAG_LIGHT_NAME,))
            on_progress("Removed the LightPlugin flag from the selected plugin.")

        self._run_fn(_do, description=f"Removing light tag from {mod}")

    def _on_utils_tag_master(self):
        """Tag the selected plugin as a master without changing its extension."""
        mod = self._selected_mod()
        if not mod:
            return
        mod_dir = os.path.join(MODS_DIR, mod)

        def _do(on_progress):
            _set_plugin_header_flags(mod_dir, add_flags=(_HEADER_FLAG_MASTER_NAME,))
            on_progress("Tagged the selected plugin as a master.")

        self._run_fn(_do, description=f"Tagging {mod} as master")

    def _on_utils_remove_master_flag(self):
        """Remove the master-plugin flag from the selected plugin."""
        mod = self._selected_mod()
        if not mod:
            return
        mod_dir = os.path.join(MODS_DIR, mod)

        def _do(on_progress):
            _set_plugin_header_flags(mod_dir, remove_flags=(_HEADER_FLAG_MASTER_NAME,))
            on_progress("Removed the MasterFile flag from the selected plugin.")

        self._run_fn(_do, description=f"Removing master flag from {mod}")

    def _on_utils_build_archlist(self):
        """Build a loose-file archlist for the selected mod."""
        mod = self._selected_mod()
        if not mod:
            return
        mod_dir = self._selected_mod_dir()
        output_file = os.path.join(mod_dir, f"{mod}.archlist")

        def _do(on_progress):
            from ui.tools.assets.archlist_creator import create_loose_archlist
            on_progress(f"Building archlist for {mod}...")
            ok, count = create_loose_archlist(mod_dir, output_file)
            if not ok:
                raise RuntimeError(f"Failed to build archlist for {mod}")
            on_progress(f"Archlist created: {count} file(s) -> {output_file}")

        self._run_fn(_do, description=f"Building archlist for {mod}")

    def _on_build(self):
        mod = self._selected_mod()
        if not mod:
            return
        game = self._get_mod_game()
        mod_dir = os.path.join(MODS_DIR, mod)
        include_patches = self._deploy_patches

        def _do(on_progress):
            from pathlib import Path
            from creation_lib.esp.authoring import deserialize, get_plugin_ext
            from creation_lib.esp.validate import validate_authoring
            errors, _ = validate_authoring(Path(mod_dir) / "yaml")
            if errors:
                on_progress(f"WARNING: {len(errors)} validation error(s)")
            ext = get_plugin_ext(Path(mod_dir))
            data_folder = self._resolve_game_data_path(game)
            deserialize(
                Path(mod_dir) / "yaml",
                Path(mod_dir) / f"{mod}.{ext}",
                game=game,
                data_folder=data_folder if data_folder and data_folder.is_dir() else None,
                on_progress=on_progress,
            )
            # Build patches
            if include_patches:
                from creation_lib.mod.patches import list_patches, get_patch_yaml_dir, get_patch_plugin_name
                for pname in list_patches(Path(mod_dir)):
                    patch_yaml = get_patch_yaml_dir(Path(mod_dir), pname)
                    plugin_file = get_patch_plugin_name(Path(mod_dir), pname)
                    on_progress(f"Building patch: {plugin_file}")
                    deserialize(
                        patch_yaml,
                        Path(mod_dir) / plugin_file,
                        game=game,
                        data_folder=data_folder if data_folder and data_folder.is_dir() else None,
                        on_progress=on_progress,
                    )

        self._run_fn(_do, description=f"Building {mod}")

    def _on_xse_build(self):
        """Run xmake install -y in mods/<name>/ to compile and stage the DLL."""
        mod = self._selected_mod()
        src_dir = self._selected_xse_src_dir()
        if not mod or not src_dir:
            return

        def _do(on_progress):
            import subprocess
            on_progress(f"Running: xmake install -y  (cwd={src_dir})")
            result = subprocess.run(
                ["xmake", "install", "-y"],
                cwd=src_dir,
                capture_output=True, text=True,
            )
            for line in (result.stdout + result.stderr).splitlines():
                on_progress(line)
            if result.returncode != 0:
                raise RuntimeError("xmake install failed — see output above.")
            on_progress("DLL built and staged.")

        xse_name = _xse_name_for(src_dir) if src_dir else "XSE"
        self._run_fn(_do, description=f"Building {xse_name} plugin {mod}")

    def _on_xse_deploy(self):
        """Deploy staged DLL from mods/<name>/<XSE>/ to game Data/<XSE>/Plugins/."""
        mod = self._selected_mod()
        if not mod:
            return
        game = self._get_mod_game()
        xse_name = _xse_name_for(os.path.join(MODS_DIR, mod))

        def _do(on_progress):
            from app.paths import get_app_root, get_resource_dir
            from creation_lib.build.deployer import deploy_mod
            game_data = self._resolve_game_data_path(game)
            deploy_data = self._resolve_deploy_data_path(game)
            deploy_mod(
                mod, game=game, game_data_dir=game_data,
                deploy_data_dir=deploy_data,
                skip_build=True, skip_pack=True,
                esp_only=False, no_esp=True, xbox=False,
                pc_max_res=0, pc_effects_max_res=0,
                xbox_max_res=1024, xbox_effects_max_res=1024,
                patches=None,
                project_root=get_app_root(),
                resource_dir=get_resource_dir(),
                on_progress=on_progress,
            )

        self._run_fn(_do, description=f"Deploying {xse_name} plugin {mod}")

    def _on_xse_undeploy(self):
        """Remove the deployed DLL from game Data/<XSE>/Plugins/."""
        mod = self._selected_mod()
        if not mod:
            return
        game = self._get_mod_game()
        xse_name = _xse_name_for(os.path.join(MODS_DIR, mod))

        def _do(on_progress):
            from app.paths import get_app_root
            from creation_lib.build.deployer import undeploy_mod
            game_data = self._resolve_deploy_data_path(game)
            undeploy_mod(
                mod, game=game, game_data_dir=game_data,
                no_esp=True, patches=None,
                project_root=get_app_root(),
                on_progress=on_progress,
            )

        self._run_fn(_do, description=f"Undeploying {xse_name} plugin {mod}")

    def _on_deploy(self):
        mod = self._selected_mod()
        if not mod:
            return
        game = self._get_mod_game()
        is_xse_only = self._selected_mod_kind() == "xse"
        skip_build = self._skip_build
        skip_pack = self._skip_pack
        skip_papyrus_compile = self._skip_papyrus_compile
        esp_only = self._esp_only
        xbox = self._xbox
        expanded_archives = self._expanded_archives
        update_fo4_archive_ini = self._update_fo4_archive_ini and not self._deploy_to_mo2
        include_patches = self._deploy_patches
        skip_validation = self._skip_validation
        archive_max_bytes = gib_to_bytes(self._archive_max_size_gb())
        archive_workers = self._asset_workers()
        archive_transfer_mode = "move" if self._move_archives else "copy"
        pc_res = _PC_RES_VALUES[self._pc_max_res_idx]
        pc_effects_idx = self._pc_effects_max_res_idx if self._pc_effects_max_res_idx is not None else self._pc_max_res_idx
        pc_effects_res = _PC_RES_VALUES[pc_effects_idx]
        xbox_res = _XBOX_RES_VALUES[self._xbox_max_res_idx] if xbox else 1024
        xbox_effects_idx = self._xbox_effects_max_res_idx if self._xbox_effects_max_res_idx is not None else self._xbox_max_res_idx
        xbox_effects_res = _XBOX_RES_VALUES[xbox_effects_idx] if xbox else 1024

        def _do(on_progress):
            from app.paths import get_app_root, get_resource_dir
            from creation_lib.build.deployer import deploy_mod
            game_data = self._resolve_game_data_path(game)
            deploy_data = self._resolve_deploy_data_path(game)
            result = deploy_mod(
                mod, game=game, game_data_dir=game_data,
                deploy_data_dir=deploy_data,
                skip_build=skip_build, skip_pack=skip_pack,
                skip_papyrus_compile=skip_papyrus_compile,
                esp_only=esp_only, no_esp=is_xse_only, xbox=xbox,
                pc_max_res=pc_res, pc_effects_max_res=pc_effects_res,
                xbox_max_res=xbox_res, xbox_effects_max_res=xbox_effects_res,
                skip_validation=skip_validation,
                patches=None if is_xse_only else (["all"] if include_patches else None),
                project_root=get_app_root(),
                resource_dir=get_resource_dir(),
                archive_max_bytes=archive_max_bytes,
                expanded_archives=expanded_archives,
                archive_workers=archive_workers,
                archive_transfer_mode=archive_transfer_mode,
                on_progress=on_progress,
            )
            if update_fo4_archive_ini and game == "fo4" and not is_xse_only:
                archive_names = list(getattr(result, "archives_deployed", []) or [])
                registered = _register_fo4_runtime_archive_ini_entries(archive_names)
                if registered:
                    on_progress(
                        "Updated Fallout4Custom.ini archive entries: "
                        + ", ".join(registered)
                    )
                elif archive_names:
                    on_progress("Fallout4Custom.ini archive entries already current")

        self._run_fn(_do, description=f"Deploying {mod}")

    def _on_import(self):
        mod = self._selected_mod()
        if not mod:
            return
        game = self._get_mod_game()
        mod_dir = os.path.join(MODS_DIR, mod)
        use_local = self._import_src_idx == 1

        def _do(on_progress):
            import shutil
            from pathlib import Path
            from creation_lib.esp.authoring import serialize, get_plugin_ext

            ext = get_plugin_ext(Path(mod_dir))
            if use_local:
                esp = Path(mod_dir) / f"{mod}.{ext}"
            else:
                data_path = self._resolve_game_data_path(game)
                esp = data_path / f"{mod}.{ext}"

            if not esp.is_file():
                raise FileNotFoundError(f"{esp} not found")

            # Serialize to temp, then swap
            temp_dir = Path(mod_dir) / "_import_tmp"
            if temp_dir.is_dir():
                shutil.rmtree(temp_dir)
            try:
                data_folder = self._resolve_game_data_path(game)
                yaml_dir = serialize(
                    esp, temp_dir, game=game,
                    data_folder=data_folder if data_folder.is_dir() else None,
                    on_progress=on_progress,
                )
                old_yaml = Path(mod_dir) / "yaml"
                if old_yaml.is_dir():
                    shutil.rmtree(old_yaml)
                shutil.move(str(yaml_dir), str(old_yaml))
                on_progress(f"Imported CK changes to: {old_yaml}")
            finally:
                if temp_dir.is_dir():
                    shutil.rmtree(temp_dir, ignore_errors=True)

        self._run_fn(_do, description=f"Importing CK changes for {mod}")

    def _on_undeploy(self):
        mod = self._selected_mod()
        if not mod:
            return
        game = self._get_mod_game()
        is_xse_only = self._selected_mod_kind() == "xse"
        cleanup_fo4_archive_ini = game == "fo4" and not self._deploy_to_mo2 and not is_xse_only
        include_patches = self._deploy_patches

        def _do(on_progress):
            from app.paths import get_app_root
            from creation_lib.build.deployer import undeploy_mod
            game_data = self._resolve_deploy_data_path(game)
            removed_files = undeploy_mod(
                mod, game=game, game_data_dir=game_data,
                no_esp=is_xse_only,
                patches=None if is_xse_only else (["all"] if include_patches else None),
                project_root=get_app_root(),
                on_progress=on_progress,
            )
            if cleanup_fo4_archive_ini:
                removed_archives = [
                    Path(name).name
                    for name in (removed_files or [])
                    if Path(name).suffix.lower() == ".ba2"
                ]
                archive_names = _unique_archive_names(
                    [
                        *removed_archives,
                        *_fo4_ini_archive_names_for_mod(mod),
                    ]
                )
                removed_ini_entries = _remove_fo4_archive_ini_entries(archive_names)
                if removed_ini_entries:
                    on_progress(
                        "Removed Fallout4Custom.ini archive entries: "
                        + ", ".join(removed_ini_entries)
                    )

        self._run_fn(_do, description=f"Undeploying {mod}")

    def _on_deploy_loose(self):
        mod = self._selected_mod()
        if not mod:
            return
        game = self._get_mod_game()
        skip_build = self._skip_build
        skip_validation = self._skip_validation
        skip_papyrus_compile = self._skip_papyrus_compile
        asset_workers = self._asset_workers()
        pc_res = _PC_RES_VALUES[self._pc_max_res_idx]
        pc_effects_idx = (
            self._pc_effects_max_res_idx
            if self._pc_effects_max_res_idx is not None
            else self._pc_max_res_idx
        )
        pc_effects_res = _PC_RES_VALUES[pc_effects_idx]

        def _do(on_progress):
            from app.paths import get_app_root
            from creation_lib.build.loose_deploy import deploy_loose_assets
            game_data = self._resolve_game_data_path(game)
            deploy_data = self._resolve_deploy_data_path(game)
            deploy_loose_assets(
                mod, game=game, game_data_dir=game_data,
                deploy_data_dir=deploy_data,
                skip_build=skip_build,
                skip_papyrus_compile=skip_papyrus_compile,
                pc_max_res=pc_res,
                pc_effects_max_res=pc_effects_res,
                skip_validation=skip_validation,
                workers=asset_workers,
                project_root=get_app_root(),
                on_progress=on_progress,
            )

        self._run_fn(_do, description=f"Deploying {mod} (loose)")

    def _on_undeploy_loose(self):
        mod = self._selected_mod()
        if not mod:
            return
        game = self._get_mod_game()

        def _do(on_progress):
            from app.paths import get_app_root
            from creation_lib.build.loose_deploy import undeploy_loose_assets
            game_data = self._resolve_deploy_data_path(game)
            undeploy_loose_assets(
                mod,
                game_data_dir=game_data,
                project_root=get_app_root(),
                on_progress=on_progress,
            )

        self._run_fn(_do, description=f"Undeploying {mod} (loose)")

    def _on_import_loose(self):
        mod = self._selected_mod()
        if not mod:
            return
        game = self._get_mod_game()

        def _do(on_progress):
            from app.paths import get_app_root
            from creation_lib.build.loose_deploy import import_loose_assets
            game_data = self._resolve_deploy_data_path(game)
            import_loose_assets(
                mod,
                game_data_dir=game_data,
                project_root=get_app_root(),
                on_progress=on_progress,
            )

        self._run_fn(_do, description=f"Importing loose assets for {mod}")

    def _on_migrate(self):
        src = self._migrate_src_dir.strip()
        if not src:
            return
        name = self._migrate_name_override.strip() or os.path.basename(src.rstrip("/\\"))
        prefix = f"{self.mod_prefix}_"
        if self._migrate_add_prefix and not name.startswith(prefix):
            name = prefix + name
        _game_ids = list(GAME_PROFILES.keys())
        game_id = _game_ids[self._migrate_game_idx] if _game_ids else "fo4"
        gitea_env = self._get_gitea_env(self._migrate_git_repo, game_override=game_id)
        mod_prefix = self.mod_prefix if self._migrate_add_prefix else None
        try:
            _game_dir = str(self._resolve_game_dir_path(game_id))
        except RuntimeError:
            _game_dir = ""

        def _do(on_progress):
            from pathlib import Path
            from app.paths import get_app_root
            from creation_lib.mod.scaffold import migrate_mod
            migrate_mod(
                Path(src), mod_name=name, game=game_id, game_dir=_game_dir,
                mod_prefix=mod_prefix,
                init_git=self._migrate_git_repo,
                gitea_url=gitea_env.get("GITEA_URL", ""),
                gitea_user=gitea_env.get("GITEA_USER", ""),
                gitea_org=gitea_env.get("GITEA_ORG", ""),
                gitea_token=gitea_env.get("GITEA_TOKEN", ""),
                project_root=get_app_root(),
                on_progress=on_progress,
            )

        self._run_fn(_do, on_done=self._refresh_mods, description=f"Migrating {name}")

    def _release_clean_archives(self, mod: str):
        """Remove existing BA2/BSA archives before a fresh release build."""
        mod_dir = Path(MODS_DIR) / mod
        backup_dir = mod_dir / "_release_archive_backup"
        if backup_dir.is_dir():
            shutil.rmtree(backup_dir)
        backup_dir.mkdir(parents=True, exist_ok=True)
        moved = False
        for archive in discover_mod_archives(mod_dir, mod):
            shutil.move(str(archive), str(backup_dir / archive.name))
            moved = True
            _log.info("Moved old archive to backup: %s", archive.name)
        if moved:
            self._release_archive_backup_dir = str(backup_dir)
        else:
            shutil.rmtree(backup_dir, ignore_errors=True)
            self._release_archive_backup_dir = None

    def _restore_release_archive_backup(self):
        backup_dir = self._release_archive_backup_dir
        if not backup_dir:
            return
        backup_path = Path(backup_dir)
        mod_dir = backup_path.parent
        if not backup_path.is_dir():
            self._release_archive_backup_dir = None
            return
        for archive in discover_mod_archives(mod_dir, mod_dir.name):
            archive.unlink()
        for archive in sorted(backup_path.iterdir(), key=lambda path: path.name.lower()):
            if archive.is_file():
                shutil.move(str(archive), str(mod_dir / archive.name))
                _log.info("Restored old archive: %s", archive.name)
        shutil.rmtree(backup_path, ignore_errors=True)
        self._release_archive_backup_dir = None

    def _discard_release_archive_backup(self):
        backup_dir = self._release_archive_backup_dir
        if backup_dir:
            shutil.rmtree(backup_dir, ignore_errors=True)
        self._release_archive_backup_dir = None

    def _release_pack_options(self) -> dict:
        """Return archive packing options selected in the release panel."""
        pc_res = _PC_RES_VALUES[self._release_pc_max_res_idx]
        pc_effects_idx = (
            self._release_pc_effects_max_res_idx
            if self._release_pc_effects_max_res_idx is not None
            else self._release_pc_max_res_idx
        )
        pc_effects_res = _PC_RES_VALUES[pc_effects_idx]
        options = {
            "pc": True,
            "xbox": self._release_xbox,
            "pc_max_res": pc_res,
            "pc_effects_max_res": pc_effects_res,
            "xbox_max_res": 1024,
            "xbox_effects_max_res": 1024,
            "archive_max_bytes": gib_to_bytes(self._archive_max_size_gb()),
            "expanded_archives": self._release_expanded_archives,
            "archive_workers": self._asset_workers(),
        }
        if self._release_xbox:
            xbox_res = _XBOX_RES_VALUES[self._release_xbox_max_res_idx]
            xbox_effects_idx = (
                self._release_xbox_effects_max_res_idx
                if self._release_xbox_effects_max_res_idx is not None
                else self._release_xbox_max_res_idx
            )
            xbox_effects_res = _XBOX_RES_VALUES[xbox_effects_idx]
            options["xbox_max_res"] = xbox_res
            options["xbox_effects_max_res"] = xbox_effects_res
        return options

    def _run_release_esp(self, mod: str, on_done) -> None:
        game = self._get_mod_game()
        mod_dir = Path(MODS_DIR) / mod

        def _do(on_progress):
            from creation_lib.esp.authoring import deserialize, get_plugin_ext
            from creation_lib.esp.validate import validate_authoring

            yaml_dir = mod_dir / "yaml"
            errors, _ = validate_authoring(yaml_dir)
            if errors:
                for error in errors:
                    on_progress(f"VALIDATION_ERROR:{error}")
                raise RuntimeError(f"{len(errors)} validation error(s)")
            ext = get_plugin_ext(mod_dir)
            data_folder = self._resolve_game_data_path(game)
            deserialize(
                yaml_dir,
                mod_dir / f"{mod}.{ext}",
                game=game,
                data_folder=data_folder if data_folder and data_folder.is_dir() else None,
                on_progress=on_progress,
            )

        self._run_fn(_do, on_done=on_done, description=f"Building ESP for {mod}")

    def _run_release_previs(self, mod: str, on_done) -> None:
        game = self._get_mod_game()
        mod_dir = Path(MODS_DIR) / mod

        def _do(on_progress):
            from creation_lib.ck.automation import run_previs

            game_dir = self._resolve_game_dir_path(game)
            run_previs(
                mod,
                game=game,
                game_dir=game_dir,
                game_data_dir=game_dir / "Data",
                mod_dir=mod_dir,
                on_progress=on_progress,
            )

        self._run_fn(_do, on_done=on_done, description=f"Generating PreVis for {mod}")

    def _check_plugin_errors_before_animdata(self, plugin_file: Path, game: str, game_data_dir: Path) -> None:
        from creation_lib.esp.editor import EditorSession, validate

        session = EditorSession(
            default_game=game,
            auto_scan_conflicts=False,
            master_search_paths=[plugin_file.parent, game_data_dir],
        )
        try:
            loaded = session.load(plugin_file, game=game)
            report = validate(session, handle=loaded.handle)
            if not report:
                return
            lines = []
            for issue in list(report)[:10]:
                form_id = f"{int(issue.form_id):08X}" if issue.form_id is not None else "--------"
                lines.append(
                    f"{issue.severity.value.upper()} {issue.plugin_name} "
                    f"{form_id}: {issue.message}"
                )
            extra = "" if len(report) <= 10 else f"\n... {len(report) - 10} more issue(s)"
            raise RuntimeError(
                "ESP validation failed before AnimTextData generation:\n"
                + "\n".join(lines)
                + extra
            )
        finally:
            session.close_all()

    def _run_release_anim_data(self, mod: str, on_done) -> None:
        game = self._get_mod_game()
        mod_dir = Path(MODS_DIR) / mod

        def _do(on_progress):
            from app.paths import get_db_dir, get_resource_dir
            from creation_lib.build.deployer import deploy_mod
            from creation_lib.ck.automation import generate_anim_data

            game_dir = self._resolve_game_dir_path(game)
            game_data_dir = game_dir / "Data"
            animtext_dir = mod_dir / "data" / "meshes" / "AnimTextData"
            if animtext_dir.exists():
                shutil.rmtree(animtext_dir)
                on_progress(f"Cleared stale AnimTextData: {animtext_dir}")
            deploy_mod(
                mod,
                game=game,
                game_data_dir=game_data_dir,
                skip_build=False,
                skip_pack=False,
                esp_only=False,
                no_esp=False,
                xbox=False,
                project_root=PROJECT_ROOT,
                db_dir=get_db_dir(),
                resource_dir=get_resource_dir(),
                on_progress=on_progress,
            )
            self._check_plugin_errors_before_animdata(mod_dir / f"{mod}.esp", game, game_data_dir)
            on_progress("ESP error check passed before AnimTextData generation.")
            result = generate_anim_data(
                mod,
                game=game,
                game_dir=game_dir,
                game_data_dir=game_data_dir,
                mod_dir=mod_dir,
                deploy_loose_data=False,
                on_progress=on_progress,
            )
            if result:
                on_progress(f"Output: {result}")
            else:
                on_progress("No AnimTextData was generated.")

        self._run_fn(_do, on_done=on_done, description=f"Generating Anim Data for {mod}")

    def _on_release_previs(self, mod: str):
        """Run previs generation as part of the release pipeline.

        On success: proceeds to audio/pack/zip.
        On failure: logs warning and continues without previs.
        """
        localize = self._release_localize

        def _after_previs():
            if self._last_runner_exit_code not in (None, 0):
                _log.warning("PreVis generation failed (exit %s). Continuing release without previs.",
                             self._last_runner_exit_code)
            if self._release_anim_data:
                self._on_release_anim_data(mod)
            elif self._release_create_fuz or self._release_create_xwm:
                self._on_release_audio(mod)
            else:
                self._on_release_pack(mod, localize)

        self._run_release_previs(mod, on_done=_after_previs)

    def _on_release_anim_data(self, mod: str):
        """Run anim data generation as part of the release pipeline.

        On success or failure: proceeds to audio/pack/zip (non-fatal).
        """
        localize = self._release_localize

        def _after_anim():
            if self._last_runner_exit_code not in (None, 0):
                _log.warning("Anim data generation failed (exit %s). Continuing release without anim data.",
                             self._last_runner_exit_code)
            if self._release_create_fuz or self._release_create_xwm:
                self._on_release_audio(mod)
            else:
                self._on_release_pack(mod, localize)

        self._run_release_anim_data(mod, on_done=_after_anim)

    def _on_release(self):
        mod = self._selected_mod()
        if not mod:
            return
        localize = self._release_localize
        self._last_runner_exit_code = None

        # Start release log capture
        self._release_log.clear()
        self._release_active = True
        _log.info("=== Release started for %s ===", mod)

        # Clean old archives
        self._release_clean_archives(mod)

        # Chain: build ESP → previs (optional) → audio (optional) → pack BA2s → zip
        def _after_build():
            if self._last_runner_exit_code not in (None, 0):
                self._fail_release(f"Release failed while building the plugin for {mod}. Check the release log for details.")
                return
            if self._release_previs:
                self._on_release_previs(mod)
            elif self._release_anim_data:
                self._on_release_anim_data(mod)
            elif self._release_create_fuz or self._release_create_xwm:
                self._on_release_audio(mod)
            else:
                self._on_release_pack(mod, localize)

        mod_dir = os.path.join(MODS_DIR, mod)

        # Build ESP first
        if os.path.isdir(os.path.join(mod_dir, "yaml")):
            self._run_release_esp(mod, on_done=_after_build)
        else:
            _after_build()

    def _on_release_pack(self, mod: str, localize: bool):
        """Pack BA2s then create the release zip."""
        mod_dir = os.path.join(MODS_DIR, mod)
        data_dir = os.path.join(mod_dir, "data")
        if os.path.isdir(data_dir):
            def _after_pack():
                if self._last_runner_exit_code not in (None, 0):
                    self._fail_release(f"Release failed while packing archives for {mod}. Check the release log for details.")
                    return
                self._do_release_package(mod, localize=localize)

            def _do(on_progress):
                from app.paths import get_resource_dir
                from creation_lib.build.packer import pack_mod

                game = self._get_mod_game()
                on_progress(f"Packing archives for {mod}...")
                pack_mod(
                    mod,
                    game=game,
                    use_archive2=False,
                    game_dir=str(self._resolve_game_dir_path(game)),
                    project_root=PROJECT_ROOT,
                    resource_dir=get_resource_dir(),
                    **self._release_pack_options(),
                )
                on_progress("Archive packing complete.")

            self._run_fn(_do, on_done=_after_pack, description=f"Packing archives for {mod}")
        else:
            self._do_release_package(mod, localize=localize)

    def _on_release_audio(self, mod: str):
        """Run audio processing as part of release pipeline, then repack BA2s and package."""
        mod_dir = os.path.join(MODS_DIR, mod)
        create_fuz = self._release_create_fuz
        create_xwm = self._release_create_xwm
        localize = self._release_localize

        fallback = "none"
        if self._toolkit_settings:
            ws = self._toolkit_settings.get_workspace_settings("mod_builder")
            fallback = ws.get("transcription_fallback", "none")

        self._running = True
        self._loading_label = f"Processing audio for {mod}..."
        _log.info("Processing audio for %s...", mod)

        def _process():
            from app.paths import get_resource_dir
            from creation_lib.audio.release import (
                build_transcript_map, find_sfx_wavs, find_voice_wavs,
                process_sfx_wav, process_voice_wav, transcribe_wav,
            )
            results = {"fuz": [], "xwm": [], "skipped": [], "errors": []}
            resource_dir = get_resource_dir()
            ext = _plugin_ext(mod_dir)
            plugin_name = f"{mod}.{ext}"

            if create_fuz:
                yaml_dir = os.path.join(mod_dir, "yaml")
                transcript_map = build_transcript_map(yaml_dir, plugin_name)
                voice_wavs = find_voice_wavs(mod_dir, plugin_name)
                _log.info("Found %d voice WAVs, %d transcripts",
                          len(voice_wavs), len(transcript_map))
                for wav in voice_wavs:
                    stem = os.path.splitext(os.path.basename(wav))[0].lower()
                    transcript = transcript_map.get(stem, "")
                    if not transcript and fallback != "none":
                        _log.info("No YAML transcript for %s, trying %s...",
                                  os.path.basename(wav), fallback)
                        transcript = transcribe_wav(wav, fallback) or ""
                    if not transcript:
                        _log.warning("No transcript for %s — skipping LIP/FUZ",
                                     os.path.basename(wav))
                        results["skipped"].append(os.path.basename(wav))
                        continue
                    fuz = process_voice_wav(wav, transcript, resource_dir=resource_dir)
                    if fuz:
                        results["fuz"].append(fuz)
                    else:
                        results["errors"].append(os.path.basename(wav))

            if create_xwm:
                sfx_wavs = find_sfx_wavs(mod_dir)
                _log.info("Found %d SFX WAVs", len(sfx_wavs))
                for wav in sfx_wavs:
                    xwm = process_sfx_wav(wav, resource_dir=resource_dir)
                    if xwm:
                        results["xwm"].append(xwm)

            return results

        def _on_audio_done():
            worker = self._release_audio_worker
            self._release_audio_worker = None
            if worker and worker.error:
                _log.error("Audio processing failed: %s", worker.error)
                self._fail_release(f"Release failed during audio processing for {mod}: {worker.error}")
                return
            r = (worker.result if worker else None) or {}
            _log.info("Audio done: %d FUZ, %d XWM, %d skipped, %d errors",
                      len(r.get("fuz", [])), len(r.get("xwm", [])),
                      len(r.get("skipped", [])), len(r.get("errors", [])))
            _log.info("Packing BA2 archives for release...")
            self._on_release_pack(mod, localize)

        self._release_audio_worker = AsyncWorker(target_fn=_process)
        self._release_audio_done_cb = _on_audio_done
        self._release_audio_worker.start()

    def _on_utils_previs(self):
        """Run standalone previs generation for the selected mod."""
        mod = self._selected_mod()
        if not mod:
            return
        game = self._get_mod_game()

        def _do(on_progress):
            from pathlib import Path
            from creation_lib.ck.automation import run_previs
            game_dir = self._resolve_game_dir_path(game)
            run_previs(
                mod, game=game, game_dir=game_dir,
                game_data_dir=game_dir / "Data",
                mod_dir=Path(MODS_DIR) / mod,
                on_progress=on_progress,
            )

        self._run_fn(_do, description=f"Generating previs for {mod}")

    def _on_utils_anim_data(self):
        """Run standalone anim data generation for the selected mod."""
        mod = self._selected_mod()
        if not mod:
            return
        game = self._get_mod_game()

        def _do(on_progress):
            from pathlib import Path
            from creation_lib.ck.automation import generate_anim_data
            game_dir = self._resolve_game_dir_path(game)
            generate_anim_data(
                mod, game=game, game_dir=game_dir,
                game_data_dir=game_dir / "Data",
                mod_dir=Path(MODS_DIR) / mod,
                on_progress=on_progress,
            )

        self._run_fn(_do, description=f"Generating anim data for {mod}")

    def _on_utils_native_anim_data(self):
        """CK-free native AnimTextData generation for the selected mod (no Creation Kit)."""
        mod = self._selected_mod()
        if not mod:
            return
        game = self._get_mod_game()

        def _do(on_progress):
            from pathlib import Path
            from creation_lib.ck.anim_text_data import generate_anim_text_data

            mod_dir = Path(MODS_DIR) / mod
            # Prefer the canonical <mod>.es[mpl]; skip hidden/cache files such as
            # .regen_land_cache.esm (pathlib glob matches dotfiles and they sort first).
            plugin = None
            for ext in ("esm", "esp", "esl"):
                cand = mod_dir / f"{mod}.{ext}"
                if cand.is_file():
                    plugin = cand
                    break
            if plugin is None:
                candidates = [
                    p for p in sorted(mod_dir.glob("*.es[mpl]"))
                    if not p.name.startswith(".")
                ]
                if not candidates:
                    on_progress(f"No .esm/.esp/.esl found in {mod_dir}")
                    return
                plugin = candidates[0]
            meshes = mod_dir / "data" / "Meshes"
            if not meshes.is_dir():
                alt = mod_dir / "data" / "meshes"
                meshes = alt if alt.is_dir() else meshes
            if not meshes.is_dir():
                on_progress(f"No loose meshes at {mod_dir / 'data' / 'Meshes'}")
                return

            # Weapon/character subgraphs resolve against the FO4 base meshes; the UI
            # reads the extracted dir from ToolkitSettings (never os.environ).
            base = None
            if self._toolkit_settings:
                extracted = self._toolkit_settings.get_game_paths(game).get("extracted_dir", "")
                if extracted and (Path(extracted) / "Meshes").is_dir():
                    base = Path(extracted) / "Meshes"
            if base is None:
                on_progress(
                    "WARNING: no base meshes (set extracted_dir in Settings → Paths); "
                    "weapon/character subgraphs will be skipped"
                )

            on_progress(f"CK-free AnimTextData from {plugin.name} (base={base})")
            count = generate_anim_text_data(
                plugin,
                game=game,
                source_meshes_root=meshes,
                output_meshes_root=meshes,
                base_meshes_root=base,
                progress_callback=on_progress,
            )
            on_progress(
                f"Wrote {count} AnimTextData bucket file(s) → {meshes / 'AnimTextData'}"
            )

        self._run_fn(_do, description=f"Generating native anim data for {mod}")

    def _on_utils_export_dialogue(self):
        """Run standalone dialogue export for the selected mod."""
        mod = self._selected_mod()
        if not mod:
            return
        game = self._get_mod_game()

        def _do(on_progress):
            from pathlib import Path
            from creation_lib.ck.automation import export_dialogue
            game_dir = self._resolve_game_dir_path(game)
            export_dialogue(
                mod, game=game, game_dir=game_dir,
                game_data_dir=game_dir / "Data",
                mod_dir=Path(MODS_DIR) / mod,
                on_progress=on_progress,
            )

        self._run_fn(_do, description=f"Exporting dialogue for {mod}")

    def _on_utils_audio(self):
        """Run audio processing (FUZ/XWM) via AsyncWorker for the selected mod."""
        mod = self._selected_mod()
        if not mod:
            return
        mod_dir = os.path.join(MODS_DIR, mod)
        create_fuz = self._utils_create_fuz
        create_xwm = self._utils_create_xwm

        # Read transcription fallback setting
        fallback = "none"
        if self._toolkit_settings:
            ws = self._toolkit_settings.get_workspace_settings("mod_builder")
            fallback = ws.get("transcription_fallback", "none")

        self._running = True
        self._loading_label = f"Processing audio for {mod}..."
        _log.info("Processing audio for %s...", mod)

        def _process():
            from app.paths import get_resource_dir
            from creation_lib.audio.release import (
                build_transcript_map, find_sfx_wavs, find_voice_wavs,
                process_sfx_wav, process_voice_wav, transcribe_wav,
            )
            results = {"fuz": [], "xwm": [], "skipped": [], "errors": []}
            resource_dir = get_resource_dir()
            ext = _plugin_ext(mod_dir)
            plugin_name = f"{mod}.{ext}"

            if create_fuz:
                yaml_dir = os.path.join(mod_dir, "yaml")
                transcript_map = build_transcript_map(yaml_dir, plugin_name)
                voice_wavs = find_voice_wavs(mod_dir, plugin_name)
                _log.info("Found %d voice WAVs, %d transcripts",
                          len(voice_wavs), len(transcript_map))
                for wav in voice_wavs:
                    stem = os.path.splitext(os.path.basename(wav))[0].lower()
                    transcript = transcript_map.get(stem, "")
                    if not transcript and fallback != "none":
                        _log.info("No YAML transcript for %s, trying %s...",
                                  os.path.basename(wav), fallback)
                        transcript = transcribe_wav(wav, fallback) or ""
                    if not transcript:
                        _log.warning("No transcript for %s — skipping LIP/FUZ",
                                     os.path.basename(wav))
                        results["skipped"].append(os.path.basename(wav))
                        continue
                    fuz = process_voice_wav(wav, transcript, resource_dir=resource_dir)
                    if fuz:
                        results["fuz"].append(fuz)
                    else:
                        results["errors"].append(os.path.basename(wav))

            if create_xwm:
                sfx_wavs = find_sfx_wavs(mod_dir)
                _log.info("Found %d SFX WAVs", len(sfx_wavs))
                for wav in sfx_wavs:
                    xwm = process_sfx_wav(wav, resource_dir=resource_dir)
                    if xwm:
                        results["xwm"].append(xwm)

            return results

        self._utils_audio_worker = AsyncWorker(target_fn=_process)
        self._utils_audio_worker.start()

    def _on_utils_translate(self):
        """Run NLLB-200 translation on the selected mod's YAML string fields."""
        mod = self._selected_mod()
        if not mod:
            return
        mod_dir = os.path.join(MODS_DIR, mod)
        _log.info("Starting translation for %s...", mod)

        def _run():
            from creation_lib.mod.translation import translate_mod
            return translate_mod(mod_dir, progress_cb=_log.info)

        self._utils_translate_worker = AsyncWorker(target_fn=_run)
        self._utils_translate_worker.start()

    def _gitea_creds(self) -> dict:
        """Return gitea_user/gitea_token kwargs from toolkit settings."""
        if not self._toolkit_settings:
            return {}
        gitea = self._toolkit_settings.gitea
        user = gitea.get("username", "")
        token = gitea.get("token", "")
        if user and token:
            return {"gitea_user": user, "gitea_token": token}
        return {}

    def _on_utils_git_create(self):
        """Initialize a git repo for the selected entry and push to Gitea.

        For combined (XSE+ESP) entries two repos are created: one in mods/ and
        one inside mods/<name>/ — unified single-repo pattern (xse source + esp share one repo).
        """
        mod = self._selected_mod()
        if not mod:
            return
        kind = self._selected_mod_kind()
        game = self._get_mod_game()
        gitea_env = self._get_gitea_env(True, game_override=game)
        target_dir = self._selected_mod_dir()
        xse_src_dir = self._selected_xse_src_dir()

        def _do(on_progress):
            from pathlib import Path
            from creation_lib.mod.git_ops import gitea_init
            _url = gitea_env.get("GITEA_URL", "") if gitea_env else ""
            _user = gitea_env.get("GITEA_USER", "") if gitea_env else ""
            _org = gitea_env.get("GITEA_ORG", "") if gitea_env else ""
            _token = gitea_env.get("GITEA_TOKEN", "") if gitea_env else ""
            gitea_init(Path(target_dir), mod, game=game,
                       gitea_url=_url, gitea_user=_user, gitea_org=_org, gitea_token=_token,
                       on_progress=on_progress)
            on_progress("Git repo created (mod).")
            if kind == "combined" and xse_src_dir:
                # Create a second repo for the C++ source — named <mod>-src to distinguish
                src_repo_name = f"{mod}-src"
                gitea_init(Path(xse_src_dir), src_repo_name, game=game,
                           gitea_url=_url, gitea_user=_user, gitea_org=_org, gitea_token=_token,
                           on_progress=on_progress)
                on_progress("Git repo created (XSE src).")

        self._run_fn(_do, description=f"Git init {mod}")

    def _on_utils_git_commit(self):
        """Commit and push all local changes for the selected entry's repo."""
        mod = self._selected_mod()
        if not mod:
            return
        creds = self._gitea_creds()
        target_dir = self._selected_mod_dir()

        def _do(on_progress):
            from pathlib import Path
            from creation_lib.mod.git_ops import git_commit
            sha = git_commit(Path(target_dir), mod, **creds)
            if sha:
                on_progress(f"Committed: {sha[:8]}")
            else:
                on_progress("No changes to commit.")

        self._run_fn(_do, description=f"Git commit {mod}")

    def _on_utils_git_pull(self):
        """Pull the latest changes for the selected entry's repo."""
        mod = self._selected_mod()
        if not mod:
            return
        creds = self._gitea_creds()
        target_dir = self._selected_mod_dir()

        def _do(on_progress):
            from pathlib import Path
            from creation_lib.mod.git_ops import git_pull
            git_pull(Path(target_dir), **creds)
            on_progress("Pull complete.")

        self._run_fn(_do, description=f"Git pull {mod}")

    def _on_utils_git_checkout(self):
        """Discard all local changes for the selected entry's repo."""
        mod = self._selected_mod()
        if not mod:
            return
        target_dir = self._selected_mod_dir()

        def _do(on_progress):
            from pathlib import Path
            from creation_lib.mod.git_ops import git_checkout
            git_checkout(Path(target_dir))
            on_progress("Local repository reset to HEAD.")

        self._run_fn(_do, description=f"Git checkout {mod}")

    def _do_release_package(self, mod: str, localize: bool = False):
        """Create the release zip (ESP + BA2s + optionally Strings/)."""
        mod_dir = os.path.join(MODS_DIR, mod)
        release_dir = os.path.join(mod_dir, "release")
        os.makedirs(release_dir, exist_ok=True)
        files_to_pack = []
        ext = _plugin_ext(mod_dir)
        plugin = os.path.join(mod_dir, f"{mod}.{ext}")
        if os.path.isfile(plugin):
            files_to_pack.append((plugin, os.path.basename(plugin)))
        for archive in discover_mod_archives(Path(mod_dir), mod):
            files_to_pack.append((str(archive), archive.name))
        # Include .cdx (cell index) if present — shipped loose alongside .esp
        cdx = os.path.join(mod_dir, f"{mod}.cdx")
        if os.path.isfile(cdx):
            files_to_pack.append((cdx, os.path.basename(cdx)))
        # Include patch plugin ESPs
        from creation_lib.mod.patches import list_patches, get_patch_plugin_name
        for pname in list_patches(Path(mod_dir)):
            plugin_file = get_patch_plugin_name(Path(mod_dir), pname)
            patch_esp = os.path.join(mod_dir, plugin_file)
            if os.path.isfile(patch_esp):
                files_to_pack.append((patch_esp, os.path.basename(patch_esp)))
        if not files_to_pack:
            self._fail_release(f"Nothing to package for {mod}. Build and deploy the mod first.")
            return
        if localize:
            strings_dir = os.path.join(mod_dir, "Strings")
            if os.path.isdir(strings_dir):
                for fname in os.listdir(strings_dir):
                    if fname.upper().endswith((".STRINGS", ".DLSTRINGS", ".ILSTRINGS")):
                        files_to_pack.append(
                            (os.path.join(strings_dir, fname), f"Strings/{fname}")
                        )
        zip_path = os.path.join(release_dir, f"{mod}.zip")
        extra_files = self._write_release_metadata(mod, mod_dir, release_dir, files_to_pack, os.path.basename(zip_path))
        files_to_pack.extend(extra_files)
        _log.info("Packaging %s...", mod)
        try:
            with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
                for fp, arcname in files_to_pack:
                    zf.write(fp, arcname)
                    _log.info("  + %s", arcname)
            _log.info("Created: %s (%s)", zip_path, _fmt_size(os.path.getsize(zip_path)))
            _log.info("=== Release complete ===")
            self._discard_release_archive_backup()
            self._release_active = False
        except Exception as exc:
            _log.error("Release packaging failed: %s", exc)
            self._fail_release(f"Release failed while creating the package for {mod}: {exc}")

    def _fail_release(self, message: str):
        self._restore_release_archive_backup()
        self._release_active = False
        self._running = False
        self._release_error_message = message
        self._release_error_popup_open = True

    def _release_option_labels(self) -> list[str]:
        labels: list[str] = []
        if self._release_localize:
            labels.append("Localized strings")
        if self._release_xbox:
            labels.append("Xbox BA2 archives")
        if self._release_expanded_archives:
            labels.append("Expanded BA2 archives")
        if self._release_create_xwm:
            labels.append("XWM audio conversion")
        if self._release_create_fuz:
            labels.append("LIP/FUZ generation")
        if self._release_previs:
            labels.append("PreCombines/PreVis generation")
        if self._release_anim_data:
            labels.append("Anim data generation")
        labels.append(f"PC max texture size: {_PC_RES_OPTIONS[self._release_pc_max_res_idx]}")
        labels.append(f"Archive max size: {self._archive_max_size_gb():.3f} GiB")
        archive_workers = self._asset_workers()
        labels.append(
            f"Asset workers: {archive_workers if archive_workers > 0 else 'auto'}"
        )
        pc_effects_idx = (
            self._release_pc_effects_max_res_idx
            if self._release_pc_effects_max_res_idx is not None
            else self._release_pc_max_res_idx
        )
        labels.append(f"PC effects max texture size: {_PC_RES_OPTIONS[pc_effects_idx]}")
        if self._release_xbox:
            labels.append(f"Xbox max texture size: {_XBOX_RES_OPTIONS[self._release_xbox_max_res_idx]}")
            xbox_effects_idx = (
                self._release_xbox_effects_max_res_idx
                if self._release_xbox_effects_max_res_idx is not None
                else self._release_xbox_max_res_idx
            )
            labels.append(f"Xbox effects max texture size: {_XBOX_RES_OPTIONS[xbox_effects_idx]}")
        return labels

    def _write_release_metadata(
        self,
        mod: str,
        mod_dir: str,
        release_dir: str,
        files_to_pack: list[tuple[str, str]],
        zip_name: str,
    ) -> list[tuple[str, str]]:
        version = self._release_version.strip()
        previous_version = self._current_mod_version.strip()
        entry = {
            "mod": mod,
            "version": version,
            "previous_version": previous_version,
            "released_at": self._current_release_timestamp(),
            "game": self._get_mod_game(),
            "plugin": f"{mod}.{_plugin_ext(mod_dir)}",
            "git_commit": self._get_git_commit(mod_dir),
            "zip_name": zip_name,
            "archive_max_size_gb": self._archive_max_size_gb(),
            "asset_workers": self._asset_workers(),
            "options": self._release_option_labels(),
            "artifacts": [arcname for _, arcname in files_to_pack],
            "notes": self._release_notes.strip(),
        }
        update_release_history(release_dir, entry)

        version_token = sanitize_release_token(version) if version else "unversioned"
        notes_name = f"Release Notes - {version_token}.md"
        latest_name = "RELEASE_NOTES.md"
        notes_body = render_release_notes(entry)
        notes_path = os.path.join(release_dir, notes_name)
        latest_path = os.path.join(release_dir, latest_name)
        with open(notes_path, "w", encoding="utf-8") as f:
            f.write(notes_body)
        with open(latest_path, "w", encoding="utf-8") as f:
            f.write(notes_body)
        if version:
            write_mod_version(mod_dir, version)
            self._current_mod_version = version
        _log.info("Generated release notes: %s", notes_name)
        _log.info("Updated changelog: CHANGELOG.md")
        return [
            (notes_path, notes_name),
            (latest_path, latest_name),
            (os.path.join(release_dir, "CHANGELOG.md"), "CHANGELOG.md"),
        ]

    def _current_release_timestamp(self) -> str:
        from datetime import datetime
        return datetime.now().astimezone().strftime("%Y-%m-%d %H:%M:%S %Z")

    def _get_git_commit(self, mod_dir: str) -> str:
        import subprocess

        try:
            result = subprocess.run(
                ["git", "-C", mod_dir, "rev-parse", "--short", "HEAD"],
                capture_output=True,
                text=True,
                check=False,
                timeout=5,
            )
        except Exception:
            return ""
        if result.returncode != 0:
            return ""
        return result.stdout.strip()

    # ── Command runner ──────────────────────────────────────────────────────

    def _refresh_fo4_install_choices(self) -> None:
        """Cache the FO4 deploy-target pick list (primary first, then extras)."""
        get = getattr(self._toolkit_settings, "get_fo4_install_choices", None)
        self._fo4_install_choices = list(get()) if callable(get) else []
        if self._fo4_install_idx >= len(self._fo4_install_choices):
            self._fo4_install_idx = 0

    def _selected_fo4_install_root(self) -> str | None:
        """Root dir of the selected non-primary FO4 deploy install, else None."""
        choices = self._fo4_install_choices
        idx = self._fo4_install_idx
        if 0 < idx < len(choices):
            return choices[idx].get("root_dir") or None
        return None

    def _resolve_game_data_path(self, game: str) -> "Path":
        """Resolve game Data/ path from toolkit settings."""
        from pathlib import Path
        if game == "fo4":
            override = self._selected_fo4_install_root()
            if override:
                return Path(override) / "Data"
        if self._toolkit_settings:
            paths = self._toolkit_settings.get_game_paths(game)
            root = paths.get("root_dir", "")
            if root:
                return Path(root) / "Data"
        raise RuntimeError(f"Cannot resolve game Data/ dir for {game}")

    def _resolve_game_dir_path(self, game: str) -> "Path":
        """Resolve game install directory from toolkit settings."""
        from pathlib import Path
        if game == "fo4":
            override = self._selected_fo4_install_root()
            if override:
                return Path(override)
        if self._toolkit_settings:
            paths = self._toolkit_settings.get_game_paths(game)
            root = paths.get("root_dir", "")
            if root:
                return Path(root)
        raise RuntimeError(f"Cannot resolve game directory for {game}")

    def _get_gitea_env(self, init_git: bool, game_override: str | None = None) -> dict[str, str] | None:
        """Build env overrides for Gitea settings from toolkit settings."""
        if not init_git:
            return {"INIT_GIT": "false"}
        if not self._toolkit_settings:
            return None
        gitea = self._toolkit_settings.gitea
        url = gitea.get("url", "")
        username = gitea.get("username", "")
        if not url or not username:
            return None
        env = {"INIT_GIT": "true", "GITEA_URL": url, "GITEA_USER": username}
        game_key = game_override or self.active_game
        org = gitea.get("orgs", {}).get(game_key, "")
        if org:
            env["GITEA_ORG"] = org
        token = gitea.get("token", "")
        if token:
            env["GITEA_TOKEN"] = token
        return env

    def _run_fn(self, target_fn, on_done=None, description: str = ""):
        """Run a Python callable in the background using the same runner pattern.

        The callable should accept an on_progress callback for status messages.
        Uses _FnRunner which is compatible with poll_runner()'s drain/finished interface.
        """
        if self._running:
            _log.warning("A command is already running.")
            return
        self._running = True
        self._reset_progress_state(description or "Working...")
        self._validation_errors = []
        self._on_done_callback = on_done
        if description:
            _log.info("> %s", description)
        self._runner = _FnRunner(target_fn)
        self._runner.start()


class _FnRunner:
    """Adapter that wraps a Python callable for the background runner interface.

    The target_fn receives an on_progress callback. Messages are queued for
    drain() and poll_runner() just like streamed process output.
    """

    def __init__(self, target_fn):
        self.target_fn = target_fn
        self._queue: queue.Queue[str] = queue.Queue()
        self._thread = None
        self.exit_code: int | None = None
        self.finished = False

    def _progress(self, msg: str):
        self._queue.put(msg)

    def _run(self):
        try:
            self.target_fn(on_progress=self._progress)
            self.exit_code = 0
        except Exception as e:
            self._queue.put(f"ERROR: {e}")
            self.exit_code = 1
        finally:
            self.finished = True

    def start(self):
        import threading
        self._thread = threading.Thread(target=self._run, daemon=True)
        self._thread.start()

    def drain(self):
        lines = []
        while not self._queue.empty():
            try:
                lines.append(self._queue.get_nowait())
            except Exception:
                break
        return lines


class _ReleaseLogHandler(logging.Handler):
    """Captures log entries into ModBuilderApp._release_log when a release is active."""

    def __init__(self, app: ModBuilderApp):
        super().__init__()
        self._app = app

    def emit(self, record):
        if not self._app._release_active:
            return
        try:
            msg = record.getMessage()
            self._app._release_log.append((record.levelno, msg))
        except Exception:
            pass
