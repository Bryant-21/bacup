"""Install-time Tales Config: the in-game TALES CONFIG settings, applied to the deployed ini.

``resources/tales_config`` is generated from Tales' TalesConfigRules.cpp by
``mods/B21_TalesFromAppalachia/tools/sync_bacup_config.py``; do not edit it by hand. The ini
helpers mirror TalesConfigRules.cpp (SetValue, MigrateV1, Read) so a file BACUP writes is one
the plugin reads without migrating again.
"""
from __future__ import annotations

import json
import math
import re
from dataclasses import dataclass
from functools import cache
from pathlib import Path

RESOURCES = Path(__file__).with_name("resources") / "tales_config"
INI_RELATIVE_PATH = Path("F4SE/Plugins/B21_TalesFromAppalachia.ini")

_TRUE = ("1", "true", "yes", "on")
_FALSE = ("0", "false", "no", "off")


@dataclass(frozen=True)
class Choice:
    value: str
    label: str


@dataclass(frozen=True)
class Setting:
    section: str
    key: str
    label: str
    help: str
    kind: str
    default: str
    min: float
    max: float
    step: float
    applies: str
    choices: tuple[Choice, ...] = ()

    @property
    def id(self) -> str:
        return f"{self.section}/{self.key}"


@dataclass(frozen=True)
class Group:
    title: str
    settings: tuple[Setting, ...]


@dataclass(frozen=True)
class Page:
    title: str
    icon: str
    groups: tuple[Group, ...]


@dataclass(frozen=True)
class Schema:
    layout_version: int
    pages: tuple[Page, ...]
    v1_renames: tuple[tuple[str, str, str, str], ...]

    def settings(self) -> list[Setting]:
        return [s for page in self.pages for group in page.groups for s in group.settings]


def _setting(raw: dict) -> Setting:
    return Setting(
        section=raw["section"], key=raw["key"], label=raw["label"], help=raw["help"], kind=raw["kind"],
        default=raw["default"], min=float(raw["min"]), max=float(raw["max"]), step=float(raw["step"]),
        applies=raw["applies"], choices=tuple(Choice(c["value"], c["label"]) for c in raw.get("choices", ())),
    )


@cache
def load_schema() -> Schema:
    raw = json.loads((RESOURCES / "schema.json").read_text(encoding="utf-8"))
    return Schema(
        layout_version=int(raw["layout_version"]),
        pages=tuple(
            Page(page["title"], page["icon"], tuple(
                Group(group["title"], tuple(_setting(s) for s in group["settings"])) for group in page["groups"]
            ))
            for page in raw["pages"]
        ),
        v1_renames=tuple(
            (r["old_section"], r["old_key"], r["new_section"], r["new_key"]) for r in raw["v1_renames"]
        ),
    )


def template_text() -> str:
    return (RESOURCES / "B21_TalesFromAppalachia.ini").read_text(encoding="utf-8")


# --- ini text -------------------------------------------------------------------------------

def _lines(text: str) -> list[tuple[str, str]]:
    """(body, ending) pairs; the last line may have no ending."""
    return [(m.group(1), m.group(2)) for m in re.finditer(r"([^\r\n]*)(\r\n|\n|$)", text) if m.group(0)]


def _section_name(body: str) -> str | None:
    body = body.strip(" \t")
    if len(body) < 2 or body[0] != "[" or body[-1] != "]":
        return None
    return body[1:-1].strip(" \t")


def _is_setting(body: str) -> bool:
    body = body.strip(" \t")
    return bool(body) and body[0] not in ";#" and "=" in body


def parse_ini(text: str) -> dict[str, dict[str, str]]:
    """Lower-cased sections and keys to values, as Tales' IniFile::Parse reads them."""
    sections: dict[str, dict[str, str]] = {}
    current: dict[str, str] | None = None
    for body, _ in _lines(text):
        line = body.strip(" \t\r")
        if not line or line[0] in ";#":
            continue
        if line[0] == "[":
            close = line.find("]")
            if close >= 0:
                current = sections.setdefault(line[1:close].strip(" \t\r").lower(), {})
            continue
        if current is not None and "=" in line:
            key, value = line.split("=", 1)
            current[key.strip(" \t\r").lower()] = value.strip(" \t\r")
    return sections


def _get(ini: dict[str, dict[str, str]], section: str, key: str) -> str | None:
    return ini.get(section.lower(), {}).get(key.lower())


def set_value(text: str, section: str, key: str, value: str) -> str:
    """Set ``key`` in ``[section]`` keeping every other line intact (TalesConfigRules SetValue)."""
    lines = _lines(text)
    eol = next((ending for _, ending in lines if ending), "\n")
    insert_after: int | None = None
    in_section = False
    for i, (body, _) in enumerate(lines):
        name = _section_name(body)
        if name is not None:
            if in_section:
                break
            in_section = name.lower() == section.lower()
            if in_section:
                insert_after = i
            continue
        if not in_section or not _is_setting(body):
            continue
        equals = body.index("=")
        if body[:equals].strip(" \t").lower() == key.lower():
            after = body[equals + 1:]
            padded = after[:1] in (" ", "\t")
            lines[i] = (body[:equals + 1] + (" " if padded else "") + value, lines[i][1])
            return "".join(b + e for b, e in lines)
        insert_after = i

    entry = f"{key}={value}"
    if insert_after is None:
        out = text
        if out:
            if not out.endswith("\n"):
                out += eol
            out += eol
        return f"{out}[{section}]{eol}{entry}{eol}"
    out = []
    for j, (body, ending) in enumerate(lines):
        out.append(body)
        if j == insert_after:
            out.append(ending or eol)
            out.append(entry)
            if ending:
                out.append(ending)
        else:
            out.append(ending)
    return "".join(out)


def _section_body(text: str, name: str) -> str | None:
    body: list[str] | None = None
    for line, ending in _lines(text):
        header = _section_name(line)
        if header is not None:
            if body is not None:
                break
            if header.lower() == name.lower():
                body = []
            continue
        if body is not None:
            body.append(line + ending)
    return None if body is None else "".join(body)


def _replace_section_body(text: str, name: str, body: str | None) -> str:
    if body is None:
        return text
    out: list[str] = []
    inside = False
    for line, ending in _lines(text):
        header = _section_name(line)
        if header is not None:
            inside = header.lower() == name.lower()
            out.append(line + ending)
            if inside:
                out.append(body)
            continue
        if not inside:
            out.append(line + ending)
    return "".join(out)


def layout_version(text: str) -> int:
    try:
        return int(_get(parse_ini(text), "General", "iVersion") or 1)
    except ValueError:
        return 1


def migrate_v1(v1: str, v2_template: str, schema: Schema | None = None) -> str:
    """The v2 template with every value the v1 file set, under its new name (TalesConfigRules MigrateV1)."""
    schema = schema or load_schema()
    old = parse_ini(v1)
    out = v2_template
    for old_section, old_key, new_section, new_key in schema.v1_renames:
        if not old_key:
            out = _replace_section_body(out, new_section, _section_body(v1, old_section))
            continue
        value = _get(old, old_section, old_key)
        if value is None:
            continue
        if (new_key.startswith("b") or new_key == "iRespawnMode") and value.lower() in _TRUE + _FALSE:
            value = "1" if value.lower() in _TRUE else "0"
        out = set_value(out, new_section, new_key, value)
    return out


def carry_forward(existing: str | None, template: str | None = None, schema: Schema | None = None) -> str:
    """The shipped template holding every value an already-deployed ini set.

    Table sections (whole-section renames, e.g. ConditionTiers) are copied verbatim so rows the
    player removed stay removed; every other key is carried one by one, so new template keys and
    comments arrive with each install.
    """
    schema = schema or load_schema()
    template = template_text() if template is None else template
    if not existing:
        return template
    if layout_version(existing) < schema.layout_version:
        return migrate_v1(existing, template, schema)
    tables = {new_section.lower() for _, old_key, new_section, _ in schema.v1_renames if not old_key}
    out = template
    for section in tables:
        out = _replace_section_body(out, section, _section_body(existing, section))
    section = None
    for line, _ in _lines(existing):
        header = _section_name(line)
        if header is not None:
            section = header
            continue
        if section is None or section.lower() in tables or not _is_setting(line):
            continue
        key, value = line.split("=", 1)
        key = key.strip(" \t")
        if section.lower() == "general" and key.lower() == "iversion":
            continue
        out = set_value(out, section, key, value.strip(" \t"))
    return out


# --- values ---------------------------------------------------------------------------------

def format_bool(existing: str, on: bool) -> str:
    existing = existing.strip(" \t").lower()
    for yes, no in (("true", "false"), ("yes", "no"), ("on", "off")):
        if existing in (yes, no):
            return yes if on else no
    return "1" if on else "0"


def key_code(value: str) -> int:
    number = value.strip(" \t").split(",", 1)[0]
    if not number.isdigit():
        return 0
    code = int(number)
    return code if 0 <= code <= 255 else 0


def format_key(vk: int) -> str:
    return f"{vk if 0 < vk < 256 else 0},0"


def format_number(value: float, step: float) -> str:
    decimals = 0
    scaled = step
    while decimals < 4 and abs(scaled - round(scaled)) > 1e-6:
        decimals += 1
        scaled *= 10
    if decimals == 0 and step >= 1:
        return str(int(math.copysign(math.floor(abs(value) + 0.5), value)))
    text = f"{value:.{max(decimals, 1)}f}"
    while len(text) > 1 and text.endswith("0") and text[-2] != ".":
        text = text[:-1]
    return text


def read(ini: dict[str, dict[str, str]], setting: Setting) -> str:
    value = _get(ini, setting.section, setting.key)
    if value is None and setting.section == "World" and setting.key == "iRespawnMode":
        return "1" if read_bool(_get(ini, "World", "bRespawn"), True) else "0"
    if value is None or (setting.kind != "text" and not value.strip()):
        return setting.default
    return value


def read_bool(value: str | None, fallback: bool) -> bool:
    lowered = (value or "").lower()
    return True if lowered in _TRUE else False if lowered in _FALSE else fallback


def read_toggle(ini: dict[str, dict[str, str]], setting: Setting) -> bool:
    return read_bool(_get(ini, setting.section, setting.key), setting.default == "1")


def read_number(ini: dict[str, dict[str, str]], setting: Setting) -> float:
    fallback = float(setting.default)
    raw = _get(ini, setting.section, setting.key) or ""
    try:
        value = int(raw) if setting.kind == "int" else float(raw)
        if not math.isfinite(value):
            value = fallback
    except ValueError:
        value = fallback
    return min(max(value, setting.min), setting.max)


def is_default(ini: dict[str, dict[str, str]], setting: Setting) -> bool:
    """"1.0" in the file and "1" as the default are the same value."""
    if setting.kind == "toggle":
        return read_toggle(ini, setting) == (setting.default == "1")
    if setting.kind in ("int", "float"):
        return abs(read_number(ini, setting) - float(setting.default)) < 1e-6
    if setting.kind == "key":
        return key_code(read(ini, setting)) == key_code(setting.default)
    return read(ini, setting) == setting.default


# --- install --------------------------------------------------------------------------------

def apply_edits(text: str, edits: dict[str, str], schema: Schema | None = None) -> str:
    """Write the dialog's pending ``{"Section/key": value}`` edits into ``text``."""
    known = {s.id for s in (schema or load_schema()).settings()}
    for setting_id, value in edits.items():
        if setting_id in known:
            section, key = setting_id.split("/", 1)
            text = set_value(text, section, key, value)
    return text


def read_deployed(data_dir: Path | None) -> str | None:
    if data_dir is None:
        return None
    path = data_dir / INI_RELATIVE_PATH
    try:
        return path.read_text(encoding="utf-8-sig")
    except (OSError, UnicodeDecodeError):
        return None


def effective_text(data_dir: Path | None, edits: dict[str, str]) -> str:
    """What the ini will hold after the next install: shipped template ← deployed ini ← edits."""
    return apply_edits(carry_forward(read_deployed(data_dir)), edits)


def write_install_ini(data_dir: Path, existing: str | None, edits: dict[str, str]) -> Path:
    path = data_dir / INI_RELATIVE_PATH
    path.parent.mkdir(parents=True, exist_ok=True)
    text = apply_edits(carry_forward(existing), edits)
    path.write_bytes(text.encode("utf-8"))
    return path
