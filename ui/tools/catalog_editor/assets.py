"""Asset lookup behind the catalog editor's path fields: loose files and BA2 contents, listed on a worker.

``AssetSource`` has no imgui; ``AssetBrowser`` is the modal built on it. Paths are relative to a Data
folder, written with backslashes (``Meshes\\Weapons\\X.nif``) and compared case-insensitively.
"""

from __future__ import annotations

import os
import threading
from collections.abc import Callable, Iterable, Sequence
from concurrent.futures import Future, ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path
from typing import Any

KIND_EXTENSIONS = {
    "nif": (".nif",), "texture": (".dds",), "material": (".bgsm", ".bgem"), "swf": (".swf",),
    "sound": (".wav", ".xwm", ".fuz"), "any": (),
}
KIND_ROOTS = {"nif": "Meshes", "texture": "Textures", "material": "Materials", "swf": "Interface", "sound": "Sound"}
SCAN_ROOTS = ("Meshes", "Textures", "Materials", "Interface", "Sound")
POPUP_ID = "Pick a file##catalog_editor_assets"
_LIST_LIMIT = 100_000


def normalize(path: str) -> str:
    return path.replace("/", "\\").strip("\\")


def to_stored_path(full: str, root: str, strip_root: bool) -> str:
    """The browser's Data-relative path as the schema stores it (without the root folder if ``strip_root``)."""
    full = normalize(full)
    prefix = f"{root}\\" if root else ""
    if strip_root and prefix and full.casefold().startswith(prefix.casefold()):
        return full[len(prefix):]
    return full


def to_full_path(stored: str, root: str, strip_root: bool) -> str:
    """Inverse of ``to_stored_path``: the Data-relative path a stored value names."""
    stored = normalize(stored)
    prefix = f"{root}\\" if root else ""
    if strip_root and prefix and stored and not stored.casefold().startswith(prefix.casefold()):
        return prefix + stored
    return stored


@dataclass(frozen=True)
class AssetFile:
    path: str
    source: str


def _default_lister(archive: str) -> list[str] | None:
    from creation_lib.ba2.native_runtime import list_archive

    return list_archive(archive)


class AssetSource:
    """Files under search roots: ``<root>/<Meshes|Textures|...>`` loose, plus BA2s sitting in each root."""

    def __init__(self, roots: Iterable[str | Path] = (), list_archive: Callable[[str], list[str] | None] | None = None,
                 pool: ThreadPoolExecutor | None = None):
        self._lock = threading.Lock()
        self._list_archive = list_archive or _default_lister
        self._pool = pool or ThreadPoolExecutor(max_workers=2, thread_name_prefix="catalog_assets")
        self._loose: dict[tuple[str, str], list[str]] = {}
        self._archives: dict[tuple[str, float], list[str]] = {}
        self._futures: dict[Any, Future] = {}
        self._roots: list[Path] = []
        self.version = 0
        self.set_roots(roots)

    def set_roots(self, roots: Iterable[str | Path]) -> None:
        unique: dict[str, Path] = {}
        for root in roots:
            if root and Path(root).is_dir():
                unique.setdefault(str(Path(root).resolve()).casefold(), Path(root))
        with self._lock:
            self._roots = list(unique.values())
            self.version += 1

    @property
    def roots(self) -> list[Path]:
        return list(self._roots)

    def close(self) -> None:
        self._pool.shutdown(wait=False, cancel_futures=True)

    @staticmethod
    def label(root: Path) -> str:
        return root.name or str(root)

    def _subdirs(self, root: str) -> tuple[str, ...]:
        return (root,) if root else SCAN_ROOTS

    def _submit(self, key: Any, work: Callable[[], None]) -> None:
        if key not in self._futures:
            self._futures[key] = self._pool.submit(work)

    def start(self, root: str = "") -> None:
        """Begin listing what ``root`` (a Data subfolder, "" for all) needs; call again to pick up new roots."""
        for folder in self._roots:
            for sub in self._subdirs(root):
                key = ("loose", str(folder), sub.casefold())
                self._submit(key, lambda f=folder, s=sub: self._scan_loose(f, s))
            for archive in sorted(folder.glob("*.ba2")):
                try:
                    stamp = archive.stat().st_mtime
                except OSError:
                    continue
                key = ("ba2", str(archive), stamp)
                self._submit(key, lambda a=archive, t=stamp: self._scan_archive(a, t))

    def _scan_loose(self, folder: Path, sub: str) -> None:
        base = folder / sub
        found: list[str] = []
        if base.is_dir():
            for current, _, names in os.walk(base):
                relative = Path(current).relative_to(folder)
                found += [normalize(str(relative / name)) for name in names]
                if len(found) > _LIST_LIMIT * 5:
                    break
        with self._lock:
            self._loose[(str(folder), sub.casefold())] = found
            self.version += 1

    def _scan_archive(self, archive: Path, stamp: float) -> None:
        try:
            names = self._list_archive(str(archive)) or []
        except Exception:
            names = []
        with self._lock:
            self._archives[(str(archive), stamp)] = [normalize(name) for name in names]
            self.version += 1

    @property
    def loading(self) -> bool:
        return any(not future.done() for future in self._futures.values())

    def wait(self, timeout: float = 30.0) -> None:
        for future in list(self._futures.values()):
            future.result(timeout)

    def snapshot(self, kind: str = "any", root: str = "") -> list[AssetFile]:
        """Files listed so far matching ``kind`` under ``root``, loose before archived, unique by path."""
        extensions = KIND_EXTENSIONS.get(kind, ())
        prefix = f"{root}\\".casefold() if root else ""
        seen: set[str] = set()
        out: list[AssetFile] = []

        def take(paths: Sequence[str], source: str) -> None:
            for path in paths:
                folded = path.casefold()
                if prefix and not folded.startswith(prefix):
                    continue
                if extensions and not folded.endswith(extensions):
                    continue
                if folded not in seen:
                    seen.add(folded)
                    out.append(AssetFile(path, source))

        with self._lock:
            loose, archives = dict(self._loose), dict(self._archives)
        for folder in self._roots:
            for sub in self._subdirs(root):
                take(loose.get((str(folder), sub.casefold()), ()), self.label(folder))
        for (archive, _), names in archives.items():
            if any(Path(archive).parent == folder for folder in self._roots):
                take(names, Path(archive).name)
        out.sort(key=lambda item: item.path.casefold())
        return out

    def exists(self, stored: str, kind: str = "any", root: str = "", strip_root: bool = False) -> bool:
        wanted = to_full_path(stored, root, strip_root).casefold()
        return any(item.path.casefold() == wanted for item in self.snapshot(kind, root))


def folder_tree(paths: Iterable[str]) -> dict:
    """Nested ``{folder: {...}}`` of the directories holding ``paths``."""
    tree: dict = {}
    for path in paths:
        node = tree
        for part in path.split("\\")[:-1]:
            node = node.setdefault(part, {})
    return tree


def filter_files(files: Sequence[AssetFile], folder: str, needle: str) -> list[AssetFile]:
    prefix = f"{folder}\\".casefold() if folder else ""
    words = needle.casefold().split()
    return [f for f in files if f.path.casefold().startswith(prefix) and all(w in f.path.casefold() for w in words)]


class AssetBrowser:
    """Modal asset picker (folder tree, filtered list, "type a path"); mirrors ``Picker``'s request/take."""

    def __init__(self, source: AssetSource):
        self.source = source
        self.landed: dict[str, str] = {}
        self._wants_open = False
        self._slot = ""
        self._config: dict = {}
        self._folder = ""
        self._needle = ""
        self._typed = ""
        self._files: list[AssetFile] = []
        self._tree: dict = {}
        self._shown: list[AssetFile] = []
        self._cache_key: tuple | None = None
        self._tree_key: tuple | None = None

    def request(self, slot: str, config: dict, current: str = "") -> None:
        self._slot, self._config = slot, dict(config or {})
        root = self._config.get("root", "")
        self._typed = to_full_path(current, root, bool(self._config.get("strip_root"))) if current else ""
        self._folder = root
        self._needle = ""
        self._cache_key = self._tree_key = None
        self._wants_open = True
        self.source.start(root)

    def take(self, slot: str) -> str | None:
        return self.landed.pop(slot, None)

    def _finish(self, path: str) -> None:
        from imgui_bundle import imgui

        self.landed[self._slot] = normalize(path)
        imgui.close_current_popup()

    def _refresh(self) -> None:
        kind, root = self._config.get("kind", "any"), self._config.get("root", "")
        tree_key = (self.source.version, kind, root)
        if tree_key != self._tree_key:
            self._files = self.source.snapshot(kind, root)
            self._tree = folder_tree(f.path for f in self._files)
            self._tree_key = tree_key
        key = (tree_key, self._folder, self._needle)
        if key != self._cache_key:
            self._shown = filter_files(self._files, self._folder, self._needle)
            self._cache_key = key

    def _draw_folder(self, name: str, children: dict, prefix: str) -> None:
        from imgui_bundle import imgui

        full = f"{prefix}\\{name}" if prefix else name
        flags = imgui.TreeNodeFlags_.open_on_arrow | imgui.TreeNodeFlags_.span_avail_width
        if not children:
            flags |= imgui.TreeNodeFlags_.leaf
        if self._folder.casefold() == full.casefold():
            flags |= imgui.TreeNodeFlags_.selected
        opened = imgui.tree_node_ex(f"{name}##{full}", flags)
        if imgui.is_item_clicked():
            self._folder = full
        if opened:
            for child in sorted(children, key=str.casefold):
                self._draw_folder(child, children[child], full)
            imgui.tree_pop()

    def draw(self) -> None:
        from imgui_bundle import imgui

        if self._wants_open:
            imgui.open_popup(POPUP_ID)
            self._wants_open = False
        viewport = imgui.get_main_viewport().size
        imgui.set_next_window_size(imgui.ImVec2(min(1000, viewport.x * 0.9), min(640, viewport.y * 0.85)),
                                   imgui.Cond_.appearing)
        opened, _ = imgui.begin_popup_modal(POPUP_ID, None)
        if not opened:
            return
        self._refresh()
        kind = self._config.get("kind", "any")
        extensions = ", ".join(KIND_EXTENSIONS.get(kind, ())) or "any file"
        imgui.text(f"Pick a file ({extensions})")
        if self.source.loading:
            imgui.same_line()
            imgui.text_disabled("still reading folders and archives...")
        if imgui.begin_child("##folders", imgui.ImVec2(260, -80), imgui.ChildFlags_.borders):
            if imgui.selectable("(all folders)", self._folder == self._config.get("root", ""))[0]:
                self._folder = self._config.get("root", "")
            for name in sorted(self._tree, key=str.casefold):
                self._draw_folder(name, self._tree[name], "")
        imgui.end_child()
        imgui.same_line()
        imgui.begin_group()
        imgui.set_next_item_width(-1)
        if imgui.is_window_appearing():
            imgui.set_keyboard_focus_here()
        _, self._needle = imgui.input_text_with_hint("##search", "Type part of a file name", self._needle)
        flags = imgui.TableFlags_.row_bg | imgui.TableFlags_.scroll_y | imgui.TableFlags_.resizable
        if imgui.begin_table("##files", 2, flags, imgui.ImVec2(0, -80)):
            imgui.table_setup_scroll_freeze(0, 1)
            imgui.table_setup_column("File", imgui.TableColumnFlags_.width_stretch, 4.0)
            imgui.table_setup_column("Source", imgui.TableColumnFlags_.width_stretch, 1.0)
            imgui.table_headers_row()
            clipper = imgui.ListClipper()
            clipper.begin(len(self._shown))
            while clipper.step():
                for index in range(clipper.display_start, clipper.display_end):
                    item = self._shown[index]
                    imgui.table_next_row()
                    imgui.table_next_column()
                    chosen = item.path.casefold() == self._typed.casefold()
                    clicked = imgui.selectable(f"{item.path}##{index}", chosen,
                                               imgui.SelectableFlags_.span_all_columns
                                               | imgui.SelectableFlags_.allow_double_click)[0]
                    if clicked:
                        self._typed = item.path
                        if imgui.is_mouse_double_clicked(0):
                            self._finish(item.path)
                    imgui.table_next_column()
                    imgui.text_disabled(item.source)
            imgui.end_table()
        imgui.end_group()
        imgui.set_next_item_width(500)
        _, self._typed = imgui.input_text_with_hint("##typed", "or type a path (relative to the Data folder)",
                                                    self._typed)
        imgui.same_line()
        if imgui.button("Use") and self._typed.strip():
            self._finish(self._typed.strip())
        imgui.same_line()
        if imgui.button("Clear"):
            self._finish("")
        imgui.same_line()
        if imgui.button("Cancel"):
            imgui.close_current_popup()
        imgui.end_popup()
