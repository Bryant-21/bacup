"""Record lookup behind the catalog editor's FormKey pickers and resolution checks."""

from __future__ import annotations

import struct
from collections.abc import Collection, Sequence
from pathlib import Path
from typing import Protocol, runtime_checkable

from creation_lib.catalog.plugin_catalog import form_key, parse_form_key
from creation_lib.esp.editor import EditorSession
from creation_lib.esp.native_runtime import (
    plugin_handle_get,
    plugin_handle_record_index_rows,
    plugin_handle_record_subrecords,
    plugin_handle_search_records,
)

# Bethesda's own masters, in load order. Loaded for picking even when the mod doesn't master them:
# a catalog may name any record the player's load order has.
OFFICIAL_MASTERS = ("Fallout4.esm", "DLCRobot.esm", "DLCworkshop01.esm", "DLCCoast.esm", "DLCworkshop02.esm",
                    "DLCworkshop03.esm", "DLCNukaWorld.esm", "DLCUltraHighResolution.esm")
PLAYER_REF = "Fallout4.esm:000014"  # built by the engine at startup; no plugin carries the record


@runtime_checkable
class FormSource(Protocol):
    def search(self, text: str, signatures: Sequence[str] | None = None, limit: int = 200) -> list[tuple[str, str]]:
        """(FormKey, "EditorID [SIG] Plugin:ID") for records whose EditorID or name contains `text`."""

    def describe(self, key: str) -> str | None: ...

    def resolves(self, key: str) -> bool: ...


class PluginFormSource:
    """A plugin and its masters loaded through an EditorSession (masters load lazily)."""

    def __init__(self, session: EditorSession, path: str | Path, data_dir: str | Path | None = None):
        self.session = session
        self.plugin = session.load(path, game="fo4")
        self.name = self.plugin.plugin_name
        self.masters: list[str] = list(plugin_handle_get(self.plugin.handle, "masters") or [])
        self._resolved: dict[str, tuple[str, str] | None] = {}
        self._globals: dict[str, float | None] = {}
        if data_dir:
            for name in OFFICIAL_MASTERS:
                if (Path(data_dir) / name).is_file() and session.get_by_name(name) is None:
                    self.load_more(Path(data_dir) / name)

    @classmethod
    def open(cls, path: str | Path, *, data_dir: str | Path | None = None, master_dirs: Sequence[Path | str] = (),
             toolkit_settings=None) -> "PluginFormSource":
        search_paths = [str(p) for p in (*master_dirs, data_dir) if p]
        session = EditorSession(toolkit_settings=toolkit_settings, auto_scan_conflicts=False, lazy_masters=True,
                                master_search_paths=search_paths or None)
        return cls(session, path, data_dir)

    def load_more(self, path: str | Path) -> str:
        """Adds a plugin (and its masters) to the records the pickers search; returns its name."""
        loaded = self.session.load(path, game="fo4", as_master=True)
        self._resolved = {key: value for key, value in self._resolved.items() if value is not None}
        self._globals.clear()
        return loaded.plugin_name

    @property
    def loaded_names(self) -> list[str]:
        return [plugin.plugin_name for plugin in self.session.plugins]

    @property
    def missing_masters(self) -> list[str]:
        loaded = {name.casefold() for name in self.loaded_names}
        return [name for name in self.masters if name.casefold() not in loaded]

    def key(self, form_id: int, handle: int | None = None) -> str:
        """`Plugin:ObjectID` of a FormID as it is written in `handle`'s file (default: this plugin)."""
        handle = self.plugin.handle if handle is None else handle
        plugin = self.session.get_by_handle(handle)
        masters = self.masters if handle == self.plugin.handle else list(plugin_handle_get(handle, "masters") or [])
        index = (form_id >> 24) & 0xFF
        origin = masters[index] if index < len(masters) else plugin.plugin_name
        return form_key(origin, form_id & 0xFFFFFF)

    def resolve(self, key: str) -> tuple[str, str] | None:
        """(signature, EditorID) of the record `key` names, from the plugin that owns it, or None."""
        if key in self._resolved:
            return self._resolved[key]
        if key == PLAYER_REF:
            return ("ACHR", "PlayerRef")
        split = parse_form_key(key)
        plugin = self.session.get_by_name(split[0]) if split else None
        found = None
        if plugin is not None:
            # Index rows answer on lazily loaded masters too, where record summaries don't.
            rows = plugin_handle_record_index_rows(plugin.handle, form_keys=[form_key(plugin.plugin_name, split[1])])
            if rows:
                found = (rows[0][2], rows[0][1])
        self._resolved[key] = found
        return found

    def prefetch(self, keys) -> None:
        """Resolve many keys with one index query per plugin; a lazily loaded master answers each query slowly."""
        wanted: dict[int, tuple[object, dict[str, str]]] = {}
        for key in keys:
            if not isinstance(key, str) or key in self._resolved or key == PLAYER_REF:
                continue
            split = parse_form_key(key)
            plugin = self.session.get_by_name(split[0]) if split else None
            if plugin is None:
                self._resolved[key] = None
                continue
            wanted.setdefault(plugin.handle, (plugin, {}))[1][form_key(plugin.plugin_name, split[1])] = key
        for plugin, asked in wanted.values():
            rows = plugin_handle_record_index_rows(plugin.handle, form_keys=list(asked))
            found = {row[0].casefold(): (row[2], row[1]) for row in rows}
            for canonical, key in asked.items():
                self._resolved[key] = found.get(canonical.casefold())

    def resolves(self, key: str) -> bool:
        return self.resolve(key) is not None

    def describe(self, key: str) -> str | None:
        found = self.resolve(key)
        if found is None:
            return None
        return f"{found[1] or '<no EditorID>'} [{found[0]}] {key}"

    def global_value(self, key: str) -> float | None:
        """A GLOB's value as the last loaded plugin that carries the record sets it, or None."""
        if key in self._globals:
            return self._globals[key]
        split = parse_form_key(key)
        found = None
        for plugin in reversed(self.session.plugins) if split else ():
            masters = self.masters if plugin.handle == self.plugin.handle else \
                list(plugin_handle_get(plugin.handle, "masters") or [])
            folded = [name.casefold() for name in masters]
            if split[0].casefold() == plugin.plugin_name.casefold():
                index = len(masters)
            elif split[0].casefold() in folded:
                index = folded.index(split[0].casefold())
            else:
                continue
            subrecords = plugin_handle_record_subrecords(plugin.handle, (index << 24) | split[1])
            value = next((data for sig, data, _ in subrecords or () if sig == "FLTV" and len(data) >= 4), None)
            if value is not None:
                found = struct.unpack("<f", value[:4])[0]
                break
        self._globals[key] = found
        return found

    def search(self, text: str, signatures: Sequence[str] | None = None, limit: int = 200,
               plugins: Collection[str] | None = None) -> list[tuple[str, str]]:
        """`plugins` limits the search to those loaded plugin names (case-insensitive); None searches all."""
        pattern = f"*{text}*" if text else "*"
        wanted = None if plugins is None else {name.casefold() for name in plugins}
        out: dict[str, str] = {}
        for plugin in self.session.plugins:
            if wanted is not None and plugin.plugin_name.casefold() not in wanted:
                continue
            try:
                rows = plugin_handle_search_records(plugin.handle, pattern, signatures=list(signatures or ()) or None,
                                                    limit=limit)
            except Exception:
                continue
            for row in rows:
                key = self.key(row["form_id"], plugin.handle)
                out[key] = f"{row['editor_id'] or row['full_name'] or '<no EditorID>'} [{row['signature']}] {key}"
                if len(out) >= limit:
                    break
        return sorted(out.items(), key=lambda item: item[1].casefold())
