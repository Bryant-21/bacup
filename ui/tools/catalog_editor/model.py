"""Catalog editor model (no imgui): the layered, schema-keyed view of one plugin's Tales catalogs.

Layers per feature are the masters' catalogs in header order, then the author's own
(plan: mods/B21_TalesFromAppalachia/docs/2026-10-12-plugin-catalogs-plan.md, section 7.2).
"""

from __future__ import annotations

import copy
import json
import os
import shutil
import tempfile
from collections.abc import Callable, Sequence
from dataclasses import dataclass
from functools import lru_cache
from pathlib import Path
from typing import Any

import jsonschema
from referencing import Registry, Resource

from creation_lib.catalog.plugin_catalog import (
    ENVELOPE_VERSION,
    SCHEMAS_DIR,
    catalog_path,
    merge_section,
    merge_settings,
    parse_form_key,
    write_document,
)
from creation_lib.previs.sources import read_masters

from ui.tools.catalog_editor.forms import FormSource

NOT_SECTIONS = ("version", "settings")
MAP = "map"
KEY_SEPARATOR = " | "
INHERITED, ADDED, OVERRIDDEN, REMOVED = "inherited", "added", "overridden", "removed"


@dataclass(frozen=True)
class MasterInfo:
    name: str
    path: Path | None


@dataclass(frozen=True)
class Problem:
    feature: str
    section: str | None
    key: str | None
    message: str
    blocking: bool = True  # False: a master's file is unreadable; the author's save is unaffected


@dataclass
class Row:
    key: str
    entry: Any
    source: str
    status: str
    inherited: Any | None
    origin: str = ""  # the master plugin the inherited entry comes from; "" when the author added it


class CatalogSaveError(ValueError):
    def __init__(self, message: str, problems: Sequence[Problem] = ()):
        super().__init__(message)
        self.problems = list(problems)


# -- schemas --


def _schema_files(directory: Path) -> list[dict]:
    return [json.loads(p.read_text(encoding="utf-8")) for p in sorted(directory.glob("*.schema.json"))]


def load_schemas(directory: Path = SCHEMAS_DIR) -> dict[str, dict]:
    """Feature name -> schema; refs into envelope.schema.json resolve through `resolve_ref`."""
    return {
        p.name.removesuffix(".schema.json"): json.loads(p.read_text(encoding="utf-8"))
        for p in sorted(Path(directory).glob("*.schema.json"))
        if p.name != "envelope.schema.json"
    }


@lru_cache(maxsize=4)
def _envelope(directory: str) -> dict:
    return json.loads((Path(directory) / "envelope.schema.json").read_text(encoding="utf-8"))


def _schema_registry(schemas: dict[str, dict]) -> Registry:
    contents = list(schemas.values())
    if not any(item.get("$id", "").endswith("/envelope.schema.json") for item in contents):
        contents.append(_envelope(str(SCHEMAS_DIR)))
    return Registry().with_resources([(item["$id"], Resource.from_contents(item)) for item in contents])


def _pointer(document: Any, pointer: str) -> Any:
    target = document
    for part in pointer.lstrip("#/").split("/") if pointer.lstrip("#/") else []:
        target = target[part.replace("~1", "/").replace("~0", "~")]
    return target


def resolve_ref(schema: dict, node: dict) -> dict:
    """Follows `$ref` (local `#/...` or `envelope.schema.json#/...`); sibling keywords win over the target's."""
    for _ in range(32):
        if not isinstance(node, dict) or "$ref" not in node:
            return node
        ref = node["$ref"]
        file_part, _, pointer = ref.partition("#")
        document = schema if not file_part else _envelope(str(SCHEMAS_DIR))
        target = _pointer(document, pointer) if pointer else document
        node = {**target, **{k: v for k, v in node.items() if k != "$ref"}}
    raise ValueError("$ref cycle")


# -- keys --


def _normalize_part(value: Any) -> str:
    if isinstance(value, str):
        parsed = parse_form_key(value)
        return f"{parsed[0]}:{parsed[1]:X}" if parsed else value
    return json.dumps(value, sort_keys=True, separators=(",", ":"))


def _display_part(value: Any) -> str:
    if isinstance(value, str):
        return value
    return "" if value is None else json.dumps(value, sort_keys=True, separators=(",", ":"))


def _is_remove(entry: Any) -> bool:
    return isinstance(entry, dict) and entry.get("remove") is True


@dataclass(frozen=True)
class _Section:
    name: str
    kind: str  # "array" | "map" | "opaque"
    key_fields: tuple[str, ...]
    ordered: bool
    node: dict

    def key_of(self, entry: Any, map_key: str) -> tuple[str, str] | None:
        """(identity, display) of an entry; None when it has no usable key."""
        if self.kind == "map":
            return (_normalize_part(map_key), map_key) if map_key else None
        if not isinstance(entry, dict) or not any(f in entry for f in self.key_fields):
            return None
        values = [entry.get(f) for f in self.key_fields]
        return (
            KEY_SEPARATOR.join(_normalize_part(v) for v in values),
            KEY_SEPARATOR.join(_display_part(v) for v in values),
        )

    def identity_of_text(self, text: str) -> str:
        if self.kind == "map":
            return _normalize_part(text)
        return KEY_SEPARATOR.join(_normalize_part(part) for part in text.split(KEY_SEPARATOR))


@dataclass
class _Layer:
    plugin: str
    path: Path
    doc: dict


@dataclass
class _Item:
    entry: Any
    source: str
    display: str


@dataclass
class _Authored:
    identity: str | None
    display: str
    map_key: str | None
    entry: Any


def _known_versions(schema: dict) -> set[Any]:
    version = schema.get("properties", {}).get("version", {})
    if "const" in version:
        return {version["const"]}
    if "enum" in version:
        return set(version["enum"])
    return {ENVELOPE_VERSION}


class FeatureDocument:
    def __init__(self, name: str, schema: dict, *, plugin: str, author_path: Path, layers: Sequence[_Layer],
                 author_doc: dict | None, registry: Registry, forms: Callable[[], FormSource | None] = lambda: None,
                 problems: Sequence[Problem] = (), author_problems: Sequence[Problem] = (), guard: Callable[[Path], None] | None = None):
        self.name = name
        self.schema = schema
        self.plugin = plugin
        self.author_path = author_path
        self._registry = registry
        self._forms = forms
        self._guard = guard
        self._layer_problems = list(problems)
        self._author_problems = list(author_problems)
        self._layers = list(layers)
        self._base_cache: dict[str, dict[str, _Item]] = {}
        self._sections = self._build_sections()
        self._author: dict[str, list[_Authored]] = {name: [] for name in self._sections}
        self._author_settings: dict[str, Any] = {}
        self._extra: dict[str, Any] = {}
        self.revision = 0  # bumped on every edit, so views can cache per edit
        self._dirty_cache: tuple[Any, bool] = (None, False)
        self._load_author(author_doc or {})
        self._baseline = self.author_document()

    # -- schema --

    def _build_sections(self) -> dict[str, _Section]:
        sections: dict[str, _Section] = {}
        for name, node in self.schema.get("properties", {}).items():
            if name in NOT_SECTIONS:
                continue
            node = resolve_ref(self.schema, node)
            key = node.get("x-key")
            if node.get("x-opaque"):
                kind, fields = "opaque", ()
            elif key == MAP:
                kind, fields = "map", ()
            else:
                kind, fields = "array", tuple(key or ())
            sections[name] = _Section(name, kind, fields, node.get("x-order") == "entries", node)
        return sections

    def _section(self, name: str) -> _Section:
        try:
            return self._sections[name]
        except KeyError:
            raise KeyError(f"{self.name} has no section {name!r}") from None

    def _editable(self, name: str) -> _Section:
        section = self._section(name)
        if section.kind == "opaque":
            raise ValueError(f"{self.name}.{name} is read-only")
        return section

    def sections(self) -> list[str]:
        return list(self._sections)

    def section_kind(self, section: str) -> str:
        return self._section(section).kind

    def entry_schema(self, section: str) -> dict:
        info = self._section(section)
        if info.kind == "array":
            node = info.node.get("items", {})
        elif info.kind == MAP:
            node = info.node.get("additionalProperties", {})
        else:
            return info.node
        return resolve_ref(self.schema, node if isinstance(node, dict) else {})

    def settings_schema(self) -> dict:
        return resolve_ref(self.schema, self.schema.get("properties", {}).get("settings", {}))

    @property
    def version(self) -> Any:
        node = self.schema.get("properties", {}).get("version", {})
        return node.get("const", node.get("default", ENVELOPE_VERSION))

    # -- layers --

    def set_layers(self, layers: Sequence[_Layer], problems: Sequence[Problem]) -> None:
        self._layers = list(layers)
        self._layer_problems = list(problems)
        self._base_cache.clear()
        self.revision += 1

    def _base(self, section: str) -> dict[str, _Item]:
        cached = self._base_cache.get(section)
        if cached is not None:
            return cached
        info = self._section(section)
        merged: dict[str, Any] = {}
        items: dict[str, _Item] = {}
        for layer in self._layers:
            value = layer.doc.get(section)
            if value is None:
                continue
            touched: list[tuple[str, str]] = []

            def key_of(entry: Any, map_key: str) -> str | None:
                key = info.key_of(entry, map_key)
                if key is None:
                    return None
                touched.append(key)
                return key[0]

            merge_section(merged, value, key_of)
            for identity, display in touched:
                if identity in merged:
                    previous = items.get(identity)
                    items[identity] = _Item(merged[identity], layer.plugin, previous.display if previous else display)
                else:
                    items.pop(identity, None)
        result = {identity: items[identity] for identity in merged if identity in items}
        self._base_cache[section] = result
        return result

    def _opaque_rows(self, section: str) -> list[Row]:
        holder: tuple[Any, str] | None = None
        for layer in [*self._layers, _Layer(self.plugin, self.author_path, {section: self._extra.get(section)})]:
            if layer.doc.get(section) is not None:
                holder = (layer.doc[section], layer.plugin)
        if holder is None:
            return []
        value, source = holder
        pairs = value.items() if isinstance(value, dict) else ((str(i), v) for i, v in enumerate(value))
        return [Row(key, entry, source, INHERITED, entry) for key, entry in pairs]

    # -- rows --

    def _authored_index(self, section: str) -> dict[str, _Authored]:
        return {a.identity: a for a in self._author[section] if a.identity is not None}

    def rows(self, section: str) -> list[Row]:
        info = self._section(section)
        if info.kind == "opaque":
            return self._opaque_rows(section)
        base = self._base(section)
        authored = self._authored_index(section)
        rows: list[Row] = []
        for identity, item in base.items():
            entry = authored.get(identity)
            if entry is None:
                rows.append(Row(item.display, item.entry, item.source, INHERITED, item.entry, item.source))
            elif _is_remove(entry.entry):
                rows.append(Row(item.display, item.entry, self.plugin, REMOVED, item.entry, item.source))
            else:
                rows.append(Row(item.display, entry.entry, self.plugin, OVERRIDDEN, item.entry, item.source))
        for identity, entry in authored.items():
            if identity not in base and not _is_remove(entry.entry):
                rows.append(Row(entry.display, entry.entry, self.plugin, ADDED, None))
        if not info.ordered:
            rows.sort(key=lambda row: row.key.casefold())
        return rows

    def section_keys(self, section: str) -> list[str]:
        return [row.key for row in self.rows(section) if row.status != REMOVED]

    # -- settings --

    def _inherited_settings(self) -> tuple[dict[str, Any], dict[str, str]]:
        merged: dict[str, Any] = {}
        sources: dict[str, str] = {}
        for layer in self._layers:
            settings = layer.doc.get("settings")
            merge_settings(merged, settings)
            if isinstance(settings, dict):
                sources.update({name: layer.plugin for name in settings})
        return merged, sources

    def settings(self) -> list[tuple[str, Any, str, bool]]:
        """Every schema-declared setting plus any other present one; unset ones carry the schema default and source ""."""
        merged, sources = self._inherited_settings()
        declared = self.settings_schema().get("properties", {})
        names = [*declared, *(n for n in (*merged, *self._author_settings) if n not in declared)]
        out = []
        for name in dict.fromkeys(names):
            if name in self._author_settings:
                out.append((name, self._author_settings[name], self.plugin, True))
            elif name in merged:
                out.append((name, merged[name], sources[name], False))
            else:
                default = resolve_ref(self.schema, declared.get(name, {})).get("default")
                out.append((name, default, "", False))
        return out

    def set_setting(self, name: str, value: Any) -> None:
        self._author_settings[name] = copy.deepcopy(value)
        self.revision += 1

    def revert_setting(self, name: str) -> None:
        self._author_settings.pop(name, None)
        self.revision += 1

    # -- editing --

    def set_entry(self, section: str, value: Any, map_key: str | None = None) -> str:
        info = self._editable(section)
        value = copy.deepcopy(value)
        if info.kind == MAP:
            if not map_key:
                raise ValueError(f"{self.name}.{section} is a map: map_key is required")
            key = info.key_of(value, map_key)
        else:
            map_key = None
            key = info.key_of(value, "")
            if key is None:
                raise ValueError(f"{self.name}.{section} entry needs one of its key fields {list(info.key_fields)}")
        assert key is not None
        identity, display = key
        base = self._base(section).get(identity)
        authored = _Authored(identity, base.display if base else display, map_key, value)
        entries = self._author[section]
        position = next((i for i, a in enumerate(entries) if a.identity == identity), None)
        entries[:] = [a for a in entries if a.identity != identity]
        entries.insert(len(entries) if position is None else position, authored)
        self.revision += 1
        return authored.display

    def _locate(self, section: str, key: str) -> tuple[str, _Item | None]:
        info = self._editable(section)
        identity = info.identity_of_text(key)
        return identity, self._base(section).get(identity)

    def remove(self, section: str, key: str) -> None:
        info = self._editable(section)
        identity, base = self._locate(section, key)
        entries = self._author[section]
        present = any(a.identity == identity for a in entries)
        if base is None:
            if not present:
                raise KeyError(f"{self.name}.{section}: no entry {key!r}")
            entries[:] = [a for a in entries if a.identity != identity]
            self.revision += 1
            return
        if present and all(_is_remove(a.entry) for a in entries if a.identity == identity):
            return
        marker: dict[str, Any] = {"remove": True}
        map_key = None
        if info.kind == MAP:
            map_key = base.display
        else:
            marker = {**{f: base.entry[f] for f in info.key_fields if f in base.entry}, "remove": True}
        position = next((i for i, a in enumerate(entries) if a.identity == identity), len(entries))
        entries[:] = [a for a in entries if a.identity != identity]
        entries.insert(min(position, len(entries)), _Authored(identity, base.display, map_key, marker))
        self.revision += 1

    def revert(self, section: str, key: str) -> None:
        identity, _ = self._locate(section, key)
        self._author[section][:] = [a for a in self._author[section] if a.identity != identity]
        self.revision += 1

    # -- author document --

    def _load_author(self, doc: dict) -> None:
        for name, value in doc.items():
            if name in ("version", "settings"):
                continue
            info = self._sections.get(name)
            if info is None or info.kind == "opaque":
                self._extra[name] = value
                continue
            if isinstance(value, dict) and info.kind == MAP:
                pairs = list(value.items())
            elif isinstance(value, list) and info.kind == "array":
                pairs = [("", entry) for entry in value]
            else:
                self._extra[name] = value
                continue
            for map_key, entry in pairs:
                key = info.key_of(entry, map_key)
                identity, display = key if key else (None, "")
                self._author[name].append(_Authored(identity, display, map_key or None, entry))
        settings = doc.get("settings")
        if isinstance(settings, dict):
            self._author_settings = dict(settings)
        elif settings is not None:
            self._extra["settings"] = settings

    @property
    def edit_count(self) -> int:
        return sum(len(entries) for entries in self._author.values()) + len(self._author_settings)

    @property
    def dirty(self) -> bool:
        # Comparing deep-copies the author's catalog, which is every list when the plugin is SeventySix.esm.
        key = (self.revision, id(self._baseline))
        if self._dirty_cache[0] != key:
            self._dirty_cache = (key, self.author_document() != self._baseline)
        return self._dirty_cache[1]

    def author_document(self) -> dict:
        doc: dict[str, Any] = {"version": self.version}
        if self._author_settings:
            doc["settings"] = copy.deepcopy(self._author_settings)
        for name, info in self._sections.items():
            entries = self._author[name]
            if info.kind == MAP:
                if entries:
                    doc[name] = {a.map_key or a.display: copy.deepcopy(a.entry) for a in entries}
            elif entries:
                doc[name] = [copy.deepcopy(a.entry) for a in entries]
        for name, value in self._extra.items():
            doc.setdefault(name, copy.deepcopy(value))
        return doc

    # -- validation and save --

    def validate(self, forms: FormSource | None) -> list[Problem]:
        problems = [*self._layer_problems, *self._author_problems]
        doc = self.author_document()
        for name, entries in self._author.items():
            seen: set[str] = set()
            for entry in entries:
                if entry.identity is None:
                    problems.append(Problem(self.name, name, None, "entry has no key"))
                elif entry.identity in seen:
                    problems.append(Problem(self.name, name, entry.display, "duplicate key"))
                seen.add(entry.identity or "")
        problems.extend(self._schema_problems(doc))
        if forms is not None:
            problems.extend(self._unresolved(forms))
        return problems

    def _schema_problems(self, doc: dict) -> list[Problem]:
        schema = {k: v for k, v in self.schema.items() if k != "required"}
        validator = jsonschema.Draft202012Validator(schema, registry=self._registry)
        problems = []
        for error in sorted(validator.iter_errors(doc), key=lambda e: [str(p) for p in e.absolute_path]):
            path = list(error.absolute_path)
            section = path[0] if path and isinstance(path[0], str) else None
            key = self._key_at(section, path[1]) if section in self._author and len(path) > 1 else None
            where = "/".join(str(p) for p in path)
            problems.append(Problem(self.name, section, key, f"{where}: {error.message}"[:300]))
        return problems

    def _key_at(self, section: str, index: Any) -> str | None:
        info = self._sections[section]
        entries = self._author[section]
        if info.kind == MAP:
            return str(index)
        return entries[index].display if isinstance(index, int) and index < len(entries) else None

    def _unresolved(self, forms: FormSource) -> list[Problem]:
        problems: list[Problem] = []
        checked: dict[str, bool] = {}

        def check(text: str, section: str | None, key: str | None) -> None:
            if parse_form_key(text) is None:
                return
            if text not in checked:
                checked[text] = forms.resolves(text)
            # A warning: catalogs also name FO76 records FO4 has no type for (challenges, curves, snap nodes),
            # which never appear in the converted plugin.
            if not checked[text]:
                problems.append(Problem(self.name, section, key, f"FormKey {text} does not resolve", blocking=False))

        def walk(value: Any, section: str | None, key: str | None) -> None:
            if isinstance(value, str):
                check(value, section, key)
            elif isinstance(value, list):
                for item in value:
                    walk(item, section, key)
            elif isinstance(value, dict) and not _is_remove(value):
                for item in value.values():
                    walk(item, section, key)

        for name, entries in self._author.items():
            for entry in entries:
                if not _is_remove(entry.entry):
                    walk(entry.entry, name, entry.display)
                    if entry.map_key:
                        check(entry.map_key, name, entry.display)
        walk(self._author_settings, "settings", None)
        return problems

    def save(self) -> Path:
        problems = [p for p in self.validate(self._forms()) if p.blocking]
        if problems:
            raise CatalogSaveError(f"{self.name}: {len(problems)} problem(s), first: {problems[0].message}", problems)
        if self._guard is not None:
            self._guard(self.author_path)
        doc = self.author_document()
        self.author_path.parent.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory() as scratch:
            written = write_document(self.name, self.plugin, doc, scratch)
            staged = self.author_path.with_name(self.author_path.name + ".tmp")
            shutil.copyfile(written, staged)
        os.replace(staged, self.author_path)
        self._baseline = doc
        return self.author_path


def _find_catalog(directory: Path, feature: str, plugin: str) -> Path | None:
    wanted = directory / catalog_path(feature, plugin)
    if wanted.is_file():
        return wanted
    parent = wanted.parent
    if parent.is_dir():
        lowered = wanted.name.casefold()
        for candidate in parent.iterdir():
            if candidate.name.casefold() == lowered and candidate.is_file():
                return candidate
    return None


def _find_plugin(directories: Sequence[Path], name: str) -> Path | None:
    lowered = name.casefold()
    for directory in directories:
        if not directory.is_dir():
            continue
        direct = directory / name
        if direct.is_file():
            return direct
        for candidate in directory.iterdir():
            if candidate.name.casefold() == lowered and candidate.is_file():
                return candidate
    return None


class CatalogSession:
    forms: FormSource | None

    def __init__(self, plugin_path: Path, search_dirs: list[Path], forms: FormSource | None,
                 schemas: dict[str, dict]):
        self.plugin_path = Path(plugin_path)
        self.plugin = self.plugin_path.name
        self.forms = forms
        self._search_dirs = search_dirs
        self._schemas = schemas
        self._registry = _schema_registry(schemas)
        self._features: dict[str, FeatureDocument] = {}
        self._master_names = read_masters(self.plugin_path)
        self.masters: list[MasterInfo] = []
        self._resolve_masters()

    @classmethod
    def open(cls, plugin_path: Path, *, data_dir: Path | None = None, bacup_output_dir: Path | None = None,
             extra_master_dirs: Sequence[Path] = (), forms: FormSource | None = None,
             schemas: dict | None = None) -> "CatalogSession":
        plugin_path = Path(plugin_path)
        search = [plugin_path.parent, *(Path(p) for p in (data_dir, bacup_output_dir) if p),
                  *(Path(p) for p in extra_master_dirs)]
        return cls(plugin_path, search, forms, schemas if schemas is not None else load_schemas())

    def _resolve_masters(self) -> None:
        self.masters = [MasterInfo(name, _find_plugin(self._search_dirs, name)) for name in self._master_names]

    @property
    def missing_masters(self) -> list[str]:
        return [master.name for master in self.masters if master.path is None]

    def add_master_folder(self, path: Path) -> None:
        self._search_dirs.append(Path(path))
        self._resolve_masters()
        for name, document in self._features.items():
            layers, problems = self._read_layers(name, document.schema)
            document.set_layers(layers, problems)

    def features(self) -> list[str]:
        return sorted(self._schemas)

    def dirty_features(self) -> list[str]:
        return [name for name, document in self._features.items() if document.dirty]

    def feature(self, name: str) -> FeatureDocument:
        document = self._features.get(name)
        if document is not None:
            return document
        schema = self._schemas[name]
        layers, problems = self._read_layers(name, schema)
        author_path = self.plugin_path.parent / catalog_path(name, self.plugin)
        author_file = _find_catalog(self.plugin_path.parent, name, self.plugin)
        author_doc, author_problems = None, []
        if author_file is not None:
            author_path = author_file
            author_doc, error = self._read_json(name, author_file, schema, self.plugin, blocking=True)
            author_problems = [error] if error is not None else []
        document = FeatureDocument(
            name, schema, plugin=self.plugin, author_path=author_path, layers=layers, author_doc=author_doc,
            registry=self._registry, forms=lambda: self.forms, problems=problems, author_problems=author_problems, guard=self._guard_save)
        self._features[name] = document
        return document

    def _read_layers(self, feature: str, schema: dict) -> tuple[list[_Layer], list[Problem]]:
        layers: list[_Layer] = []
        problems: list[Problem] = []
        for master in self.masters:
            if master.path is None:
                continue
            file = _find_catalog(master.path.parent, feature, master.name)
            if file is None:
                continue
            doc, error = self._read_json(feature, file, schema, master.name, blocking=False)
            if error is not None:
                problems.append(error)
            if doc is not None:
                layers.append(_Layer(master.name, file, doc))
        return layers, problems

    @staticmethod
    def _read_json(feature: str, file: Path, schema: dict, plugin: str, *, blocking: bool):
        try:
            doc = json.loads(file.read_text(encoding="utf-8-sig"))
        except (OSError, ValueError) as exc:
            return None, Problem(feature, None, None, f"{plugin}: cannot read {file}: {exc}", blocking)
        if not isinstance(doc, dict):
            return None, Problem(feature, None, None, f"{plugin}: {file} is not a JSON object", blocking)
        version = doc.get("version")
        if version not in _known_versions(schema):
            return None, Problem(feature, None, None, f"{plugin}: unknown version {version!r}, layer skipped", blocking)
        return doc, None

    def _guard_save(self, target: Path) -> None:
        lowered = self.plugin.casefold()
        if any(master.name.casefold() == lowered for master in self.masters):
            raise CatalogSaveError(f"{self.plugin} is one of its own masters; refusing to write a master's catalog")
        resolved = target.resolve()
        for name in self._features:
            for master in self.masters:
                if master.path is None:
                    continue
                file = _find_catalog(master.path.parent, name, master.name)
                if file is not None and file.resolve() == resolved:
                    raise CatalogSaveError(f"{target} is the catalog of master {master.name}")
