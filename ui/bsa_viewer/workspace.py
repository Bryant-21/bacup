"""BSA Viewer workspace."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import hashlib
import logging
import queue
import shutil
import subprocess
import tempfile
import threading

import moderngl
import numpy as np
from imgui_bundle import hello_imgui, imgui

from creation_lib.ba2 import native_runtime
from creation_lib.ui.shell import BaseWorkspace, make_window
from ui.toolkit.app_paths import get_resource_dir
from creation_lib.ui.widgets import pick_folder
from creation_lib.ui.widgets.audio_player import InlineAudioPlayer as _InlineAudioPlayer
from ui.tools.imgui_helpers import pick_file

_log = logging.getLogger("ui.bsa_viewer")

_NS = "##bsa_viewer"
_AUDIO_EXTS = {".wav", ".xwm", ".fuz", ".ogg", ".mp3", ".flac"}
_TEXTURE_EXTS = {".dds"}


@dataclass(slots=True)
class ArchiveFileEntry:
    path: str
    folder: str
    name: str
    ext: str


@dataclass(slots=True)
class ArchiveDocument:
    path: str
    archive_name: str
    format: str
    version: int | None
    file_count: int
    backend: str
    files: list[ArchiveFileEntry]


def _build_file_entries(paths: list[str]) -> list[ArchiveFileEntry]:
    normalized_paths = sorted(raw.replace("\\", "/").lower() for raw in paths)
    entries: list[ArchiveFileEntry] = []
    for normalized in normalized_paths:
        parts = normalized.rsplit("/", 1)
        folder = parts[0] if len(parts) > 1 else ""
        name = parts[-1]
        ext = name.rsplit(".", 1)[-1] if "." in name else ""
        entries.append(ArchiveFileEntry(normalized, folder, name, ext))
    return entries


def _archive_from_payload(payload: dict) -> ArchiveDocument:
    files = payload.get("files", []) or []
    return ArchiveDocument(
        path=str(payload.get("path", "")),
        archive_name=str(payload.get("archive_name", Path(str(payload.get("path", ""))).name)),
        format=str(payload.get("format", "")),
        version=payload.get("version"),
        file_count=int(payload.get("file_count", len(files))),
        backend=str(payload.get("backend", "")),
        files=_build_file_entries(files),
    )


def _is_audio_member(path: str) -> bool:
    return Path(path).suffix.lower() in _AUDIO_EXTS


def _is_texture_member(path: str) -> bool:
    return Path(path).suffix.lower() in _TEXTURE_EXTS


def _archive_format_for_path(path: str) -> str:
    return "bsa" if Path(path).suffix.lower() == ".bsa" else "ba2"


def _safe_member_output_path(output_dir: str | Path, member_path: str) -> Path:
    root = Path(output_dir).expanduser().resolve(strict=False)
    normalized = member_path.replace("\\", "/").lstrip("/")
    parts = [part for part in normalized.split("/") if part and part != "."]
    if not parts or any(part == ".." or ":" in part for part in parts):
        raise RuntimeError(f"Unsafe archive member path: {member_path}")
    out_path = root.joinpath(*parts).resolve(strict=False)
    if out_path != root and root not in out_path.parents:
        raise RuntimeError(f"Unsafe archive member path: {member_path}")
    return out_path


class BSAViewerWorkspace(BaseWorkspace):
    name = "BSA Viewer"
    icon = "BSA"
    id = "bsa_viewer"
    user_guide_body = """
Open a BSA or BA2 archive, browse its files, and filter the contents list.
Preview audio or textures, then extract selected files or full archives as needed.
"""

    def __init__(self, toolkit_settings=None):
        super().__init__(toolkit_settings)
        self._archives: list[ArchiveDocument] = []
        self._selected_archive_idx: int = -1
        self._selected_file_path: str = ""
        self._search_text: str = ""
        self._filtered_files: list[ArchiveFileEntry] = []
        self._filter_dirty = True
        self._search_focus_requested = False
        self._current_match_index = -1
        self._status_msg = "Open a BSA or BA2 archive to browse its contents."
        self._result_msg = ""
        self._error_msg = ""
        self._busy = False
        self._job_label = ""
        self._jobs: queue.Queue[tuple[str, object]] = queue.Queue()
        self._last_extract_dir = ""
        self._pending_open_path = ""

        self._audio_player = _InlineAudioPlayer()
        self._audio_busy = False
        self._audio_job_label = ""
        self._audio_jobs: queue.Queue[tuple[str, object]] = queue.Queue()
        self._audio_result_msg = ""
        self._audio_error_msg = ""
        self._audio_loaded_key: tuple[str, str] | None = None
        self._audio_pending_key: tuple[str, str] | None = None
        self._audio_autoplay_key: tuple[str, str] | None = None
        self._audio_cache: dict[tuple[str, str], str] = {}
        self._audio_preview_root = Path(tempfile.mkdtemp(prefix="modbox_bsa_audio_"))

        self._texture_busy = False
        self._texture_job_label = ""
        self._texture_jobs: queue.Queue[tuple[str, object]] = queue.Queue()
        self._texture_result_msg = ""
        self._texture_error_msg = ""
        self._texture_loaded_key: tuple[str, str] | None = None
        self._texture_pending_key: tuple[str, str] | None = None
        self._texture_cache: dict[tuple[str, str], str] = {}
        self._texture_preview_root = Path(tempfile.mkdtemp(prefix="modbox_bsa_dds_"))
        self._texture_preview_tex = None
        self._texture_preview_size: tuple[int, int] = (0, 0)
        self._texture_preview_name = ""

    def get_dockable_windows(self) -> list[hello_imgui.DockableWindow]:
        return [
            make_window(f"Archives{_NS}", "LeftDock"),
            make_window(f"Files{_NS}", "MainDockSpace"),
            make_window(f"Preview{_NS}", "RightDock"),
        ]

    def initialize(self) -> None:
        self._bind_panels(
            {
                f"Archives{_NS}": self._draw_archives_panel,
                f"Files{_NS}": self._draw_files_panel,
                f"Preview{_NS}": self._draw_preview_panel,
            }
        )
        self._initialized = True

    def draw(self) -> None:
        if not self.active:
            return
        self._poll_jobs()
        self._poll_audio_jobs()
        self._poll_texture_jobs()
        self._handle_shortcuts()

    def draw_menu(self) -> None:
        if imgui.begin_menu("File"):
            if imgui.menu_item("Open Archive...", "Ctrl+O", False)[0]:
                self._open_archive_dialog()
            imgui.separator()
            can_extract_file = self._selected_archive is not None and bool(self._selected_file_path)
            can_extract_archive = self._selected_archive is not None
            if not can_extract_file:
                imgui.begin_disabled()
            if imgui.menu_item("Extract Selected File", "", False)[0]:
                self._extract_selected_file()
            if not can_extract_file:
                imgui.end_disabled()
            if not can_extract_archive:
                imgui.begin_disabled()
            if imgui.menu_item("Extract Archive...", "", False)[0]:
                self._extract_selected_archive()
            imgui.separator()
            if imgui.menu_item("Close Archive", "", False)[0]:
                self._close_selected_archive()
            if imgui.menu_item("Close All Archives", "", False)[0]:
                self._close_all_archives()
            if not can_extract_archive:
                imgui.end_disabled()
            imgui.end_menu()
        if imgui.begin_menu("Edit"):
            has_query = bool(self._search_text.strip())
            if imgui.menu_item("Find", "Ctrl+F", False)[0]:
                self._focus_search()
            if not has_query:
                imgui.begin_disabled()
            if imgui.menu_item("Find Next", "F3", False)[0]:
                self._advance_search(1)
            if imgui.menu_item("Find Previous", "Shift+F3", False)[0]:
                self._advance_search(-1)
            if imgui.menu_item("Clear Search", "Esc", False)[0]:
                self._clear_search()
            if not has_query:
                imgui.end_disabled()
            imgui.end_menu()

    @property
    def _selected_archive(self) -> ArchiveDocument | None:
        if 0 <= self._selected_archive_idx < len(self._archives):
            return self._archives[self._selected_archive_idx]
        return None

    def has_toolbar(self) -> bool:
        return True

    def draw_toolbar(self, icon_font=None) -> None:
        from imgui_bundle import icons_fontawesome_6 as fa

        def _btn(icon: str, tooltip: str, *, enabled: bool = True) -> bool:
            if not enabled:
                imgui.begin_disabled()
            if icon_font:
                imgui.push_font(icon_font, icon_font.legacy_size)
            clicked = imgui.button(icon)
            if icon_font:
                imgui.pop_font()
            if imgui.is_item_hovered():
                imgui.set_tooltip(tooltip)
            if not enabled:
                imgui.end_disabled()
            return clicked and enabled

        def _sep() -> None:
            imgui.text("|")
            imgui.same_line()

        has_archive = self._selected_archive is not None
        has_file = has_archive and bool(self._selected_file_path)
        has_audio = has_file and _is_audio_member(self._selected_file_path)
        current_key = self._selected_audio_key()
        selected_is_loaded = current_key is not None and current_key == self._audio_loaded_key
        audio_icon = (
            getattr(fa, "ICON_FA_PAUSE", "||")
            if selected_is_loaded and self._audio_player.is_playing
            else getattr(fa, "ICON_FA_PLAY", ">")
        )

        if _btn(getattr(fa, "ICON_FA_FOLDER_OPEN", "O"), "Open archive"):
            self._open_archive_dialog()
        imgui.same_line()
        _sep()
        if _btn(getattr(fa, "ICON_FA_MAGNIFYING_GLASS", "S"), "Find in archive (Ctrl+F)", enabled=has_archive):
            self._focus_search()
        imgui.same_line()
        if _btn(getattr(fa, "ICON_FA_ARROW_DOWN_A_Z", "N"), "Next match (F3)", enabled=bool(self._search_text.strip())):
            self._advance_search(1)
        imgui.same_line()
        _sep()
        if _btn(audio_icon, "Play or pause selected audio", enabled=has_audio):
            self._toggle_selected_audio()
        imgui.same_line()
        if _btn(getattr(fa, "ICON_FA_STOP", "[]"), "Stop audio preview", enabled=self._audio_player.has_audio):
            self._stop_audio()
        imgui.same_line()
        _sep()
        if _btn(getattr(fa, "ICON_FA_FILE_EXPORT", "E"), "Extract selected file", enabled=has_file):
            self._extract_selected_file()
        imgui.same_line()
        if _btn(getattr(fa, "ICON_FA_BOX_ARCHIVE", "A"), "Extract selected archive", enabled=has_archive):
            self._extract_selected_archive()
        imgui.same_line()
        _sep()
        if _btn(getattr(fa, "ICON_FA_XMARK", "X"), "Close selected archive", enabled=has_archive):
            self._close_selected_archive()
        imgui.same_line()
        if _btn(getattr(fa, "ICON_FA_BROOM", "C"), "Close all archives", enabled=bool(self._archives)):
            self._close_all_archives()
        imgui.same_line()
        _sep()
        archive = self._selected_archive
        if archive is None:
            imgui.text_disabled("No archive loaded")
        else:
            imgui.text(
                f"{archive.archive_name}  {archive.file_count:,} files  {archive.format} ({archive.backend})"
            )

    def open_file(self, path: str) -> None:
        normalized = str(Path(path).expanduser().resolve(strict=False))
        if self._initialized:
            self._pending_open_path = ""
            self._load_archive(normalized)
        else:
            self._pending_open_path = normalized

    def _apply_pending_open(self) -> None:
        if self._pending_open_path:
            path = self._pending_open_path
            self._pending_open_path = ""
            self._load_archive(path)

    def on_activate(self) -> None:
        super().on_activate()
        self._apply_pending_open()

    def cleanup(self) -> None:
        self._stop_audio()
        self._clear_texture_preview()
        shutil.rmtree(self._audio_preview_root, ignore_errors=True)
        shutil.rmtree(self._texture_preview_root, ignore_errors=True)

    def _draw_archives_panel(self) -> None:
        if not imgui.begin(f"Archives{_NS}"):
            imgui.end()
            return

        if imgui.button("Open Archive..."):
            self._open_archive_dialog()
        imgui.same_line()
        if self._busy:
            imgui.text_disabled(self._job_label or "Working...")

        imgui.separator()

        if not self._archives:
            imgui.text_wrapped("No archives open.")
            imgui.text_disabled("Use the toolbar or File > Open Archive...")
            imgui.end()
            return

        for idx, archive in enumerate(self._archives):
            selected = idx == self._selected_archive_idx
            label = f"{archive.archive_name}##archive_{idx}"
            flags = imgui.TreeNodeFlags_.open_on_arrow | imgui.TreeNodeFlags_.span_avail_width
            if selected:
                flags |= imgui.TreeNodeFlags_.selected
            opened = imgui.tree_node_ex(label, flags)
            if imgui.is_item_clicked():
                self._selected_archive_idx = idx
                self._set_selected_file("")
                self._mark_filter_dirty()
            if opened:
                imgui.text_disabled(archive.path)
                imgui.text(f"Files:   {archive.file_count:,}")
                imgui.text(f"Format:  {archive.format}")
                imgui.text(f"Backend: {archive.backend}")
                if archive.version is not None:
                    imgui.text(f"Version: {archive.version}")
                imgui.tree_pop()

        imgui.end()

    def _draw_files_panel(self) -> None:
        if not imgui.begin(f"Files{_NS}"):
            imgui.end()
            return

        archive = self._selected_archive
        if archive is None:
            imgui.text_wrapped("Select an archive from the left panel.")
            imgui.end()
            return

        imgui.text(archive.archive_name)
        imgui.same_line()
        imgui.text_disabled(archive.path)

        if self._search_focus_requested:
            imgui.set_keyboard_focus_here()
            self._search_focus_requested = False
        imgui.set_next_item_width(320)
        changed, new_filter = imgui.input_text_with_hint(
            f"##search{_NS}",
            "Search files (Ctrl+F)",
            self._search_text,
        )
        if changed:
            self._search_text = new_filter
            self._mark_filter_dirty()
        search_active = bool(self._search_text.strip())
        if imgui.is_item_active():
            if imgui.is_key_pressed(imgui.Key.enter) or imgui.is_key_pressed(imgui.Key.keypad_enter):
                self._advance_search(1)
            elif imgui.is_key_pressed(imgui.Key.escape):
                self._clear_search()

        imgui.same_line()
        if not search_active:
            imgui.begin_disabled()
        if imgui.button("<##search_prev"):
            self._advance_search(-1)
        if imgui.is_item_hovered():
            imgui.set_tooltip("Previous match (Shift+F3)")
        if not search_active:
            imgui.end_disabled()
        imgui.same_line()
        if not search_active:
            imgui.begin_disabled()
        if imgui.button(">##search_next"):
            self._advance_search(1)
        if imgui.is_item_hovered():
            imgui.set_tooltip("Next match (F3)")
        if not search_active:
            imgui.end_disabled()
        imgui.same_line()
        if search_active:
            match_text = (
                f"{self._current_match_index + 1}/{len(self._filtered_files)} matches"
                if self._filtered_files
                else "0 matches"
            )
        else:
            match_text = f"{archive.file_count:,} files"
        imgui.text_disabled(match_text)

        imgui.same_line()
        if search_active:
            if imgui.button(f"Clear##search_clear{_NS}"):
                self._clear_search()

        self._draw_audio_preview_section()

        imgui.separator()
        self._ensure_filtered_files()

        if imgui.begin_table(
            f"files_table{_NS}",
            3,
            imgui.TableFlags_.row_bg
            | imgui.TableFlags_.resizable
            | imgui.TableFlags_.scroll_y
            | imgui.TableFlags_.sortable,
        ):
            imgui.table_setup_column("Path", imgui.TableColumnFlags_.width_stretch, 0.70)
            imgui.table_setup_column("Folder", imgui.TableColumnFlags_.width_stretch, 0.22)
            imgui.table_setup_column("Type", imgui.TableColumnFlags_.width_fixed, 0.08)
            imgui.table_headers_row()

            clipper = imgui.ListClipper()
            clipper.begin(len(self._filtered_files))
            while clipper.step():
                for row_idx in range(clipper.display_start, clipper.display_end):
                    entry = self._filtered_files[row_idx]
                    imgui.table_next_row()
                    imgui.table_next_column()
                    selected = entry.path == self._selected_file_path
                    clicked = imgui.selectable(
                        entry.path,
                        selected,
                        imgui.SelectableFlags_.span_all_columns,
                    )[0]
                    if clicked:
                        self._set_selected_file(entry.path)
                        if imgui.is_mouse_double_clicked(0) and _is_audio_member(entry.path):
                            self._toggle_selected_audio()
                    imgui.table_next_column()
                    imgui.text_unformatted(entry.folder or ".")
                    imgui.table_next_column()
                    imgui.text_unformatted(entry.ext or "-")
            clipper.end()
            imgui.end_table()

        imgui.separator()
        if self._busy:
            imgui.text_disabled(self._job_label or "Working...")
        elif self._error_msg:
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.35, 0.35, 1.0))
            imgui.text_wrapped(self._error_msg)
            imgui.pop_style_color()
        elif self._result_msg:
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(0.45, 0.9, 0.45, 1.0))
            imgui.text_wrapped(self._result_msg)
            imgui.pop_style_color()
        else:
            imgui.text_disabled(self._status_msg)

        imgui.end()

    def _draw_preview_panel(self) -> None:
        if not imgui.begin(f"Preview{_NS}"):
            imgui.end()
            return

        archive = self._selected_archive
        member = self._selected_file_path
        imgui.text("Preview")
        imgui.separator()

        if archive is None or not member:
            imgui.text_disabled("Select a file to preview.")
            imgui.end()
            return

        imgui.text_unformatted(Path(member).name)
        imgui.text_disabled(member)
        imgui.separator()

        if not _is_texture_member(member):
            imgui.text_disabled("DDS preview is available when a .dds texture is selected.")
            imgui.end()
            return

        key = (archive.path, member)
        if self._texture_busy and self._texture_pending_key == key:
            imgui.text_disabled(self._texture_job_label or "Loading DDS preview...")
            imgui.end()
            return

        if self._texture_error_msg and self._texture_pending_key in (None, key):
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.35, 0.35, 1.0))
            imgui.text_wrapped(self._texture_error_msg)
            imgui.pop_style_color()
        elif self._texture_result_msg and self._texture_loaded_key == key:
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(0.45, 0.9, 0.45, 1.0))
            imgui.text_wrapped(self._texture_result_msg)
            imgui.pop_style_color()

        if self._texture_loaded_key != key or self._texture_preview_tex is None:
            imgui.text_disabled("Preview will load automatically for the selected DDS.")
            imgui.end()
            return

        width, height = self._texture_preview_size
        imgui.text_disabled(f"{width} x {height}")

        avail = imgui.get_content_region_avail()
        max_w = max(64.0, avail.x)
        max_h = max(64.0, avail.y - imgui.get_text_line_height_with_spacing() - 8.0)
        if width > 0 and height > 0:
            scale = min(max_w / width, max_h / height)
            scale = max(scale, min(1.0, max_w / width if width else 1.0))
            draw_w = max(1.0, width * scale)
            draw_h = max(1.0, height * scale)
            imgui.image(
                imgui.ImTextureRef(self._texture_preview_tex.glo),
                imgui.ImVec2(draw_w, draw_h),
            )
        else:
            imgui.text_disabled("Preview dimensions unavailable.")

        imgui.end()

    def _draw_audio_preview_section(self) -> None:
        archive = self._selected_archive
        member = self._selected_file_path
        imgui.separator()
        imgui.text_disabled("Audio Preview")

        if archive is None or not member:
            imgui.text_disabled("Select an audio file to preview.")
            return
        if not _is_audio_member(member):
            imgui.text_disabled("Selected file is not a previewable audio format.")
            return

        key = (archive.path, member)
        loaded_for_selected = self._audio_loaded_key == key
        play_label = "Pause Preview" if loaded_for_selected and self._audio_player.is_playing else "Play Preview"
        if imgui.button(f"{play_label}##audio_play_panel{_NS}"):
            self._toggle_selected_audio()
        imgui.same_line()
        if not self._audio_player.has_audio:
            imgui.begin_disabled()
        if imgui.button(f"Stop##audio_stop_panel{_NS}"):
            self._stop_audio()
        if not self._audio_player.has_audio:
            imgui.end_disabled()

        imgui.same_line()
        imgui.text_unformatted(Path(member).name)

        if self._audio_busy and self._audio_pending_key == key:
            imgui.text_disabled(self._audio_job_label or "Preparing preview...")
        elif self._audio_error_msg and self._audio_pending_key in (None, key):
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1.0, 0.35, 0.35, 1.0))
            imgui.text_wrapped(self._audio_error_msg)
            imgui.pop_style_color()
        elif self._audio_result_msg and loaded_for_selected:
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(0.45, 0.9, 0.45, 1.0))
            imgui.text_wrapped(self._audio_result_msg)
            imgui.pop_style_color()

        if loaded_for_selected:
            self._audio_player.draw(_NS)
        else:
            imgui.text_disabled("Preview will extract just this file into a temporary cache.")

    def _selected_texture_key(self) -> tuple[str, str] | None:
        archive = self._selected_archive
        member = self._selected_file_path
        if archive is None or not member or not _is_texture_member(member):
            return None
        return (archive.path, member)

    def _set_selected_file(self, path: str) -> None:
        self._selected_file_path = path
        self._sync_texture_preview_for_selection()

    def _sync_texture_preview_for_selection(self) -> None:
        key = self._selected_texture_key()
        if key is None:
            self._clear_texture_preview()
            return
        if self._texture_loaded_key == key and self._texture_preview_tex is not None:
            return
        cached = self._texture_cache.get(key, "")
        if cached and Path(cached).is_file():
            self._load_texture_preview(key, cached)
            return
        if self._texture_busy and self._texture_pending_key == key:
            return
        self._start_texture_job(
            key,
            f"Preparing DDS preview for {Path(key[1]).name}",
            lambda: ("texture_preview", key, self._prepare_texture_preview(*key)),
        )

    def _mark_filter_dirty(self) -> None:
        self._filter_dirty = True

    def _ensure_filtered_files(self) -> None:
        if not self._filter_dirty:
            return
        archive = self._selected_archive
        if archive is None:
            self._filtered_files = []
            self._filter_dirty = False
            self._current_match_index = -1
            return
        query = self._search_text.strip().lower()
        if not query:
            self._filtered_files = archive.files
        else:
            self._filtered_files = [entry for entry in archive.files if query in entry.path]
        self._sync_search_selection()
        self._filter_dirty = False

    def _sync_search_selection(self) -> None:
        if not self._filtered_files:
            self._set_selected_file("")
            self._current_match_index = -1
            return
        for idx, entry in enumerate(self._filtered_files):
            if entry.path == self._selected_file_path:
                self._current_match_index = idx
                return
        self._current_match_index = 0
        self._set_selected_file(self._filtered_files[0].path)

    def _advance_search(self, delta: int) -> None:
        self._ensure_filtered_files()
        if not self._filtered_files:
            return
        if self._current_match_index < 0:
            self._current_match_index = 0
        else:
            self._current_match_index = (self._current_match_index + delta) % len(self._filtered_files)
        self._set_selected_file(self._filtered_files[self._current_match_index].path)

    def _clear_search(self) -> None:
        if not self._search_text:
            return
        self._search_text = ""
        self._mark_filter_dirty()

    def _focus_search(self) -> None:
        self._search_focus_requested = True

    def _handle_shortcuts(self) -> None:
        io = imgui.get_io()
        if io.want_text_input:
            return
        if io.key_ctrl and imgui.is_key_pressed(imgui.Key.f):
            self._focus_search()
            return
        if imgui.is_key_pressed(imgui.Key.f3):
            self._advance_search(-1 if io.key_shift else 1)
            return
        if imgui.is_key_pressed(imgui.Key.escape):
            self._clear_search()

    def _open_archive_dialog(self) -> None:
        path = pick_file(
            "Open BSA/BA2 Archive",
            [("Bethesda Archives", "*.bsa *.ba2"), ("All files", "*.*")],
        )
        if path:
            self._load_archive(path)

    def _load_archive(self, path: str) -> None:
        normalized = str(Path(path).expanduser().resolve(strict=False))
        for idx, archive in enumerate(self._archives):
            if archive.path == normalized:
                self._selected_archive_idx = idx
                self._set_selected_file("")
                self._mark_filter_dirty()
                self._status_msg = f"{archive.archive_name} is already open."
                return

        self._start_job(
            f"Opening {Path(normalized).name}",
            lambda: ("open_archive", normalized, self._read_archive(normalized)),
        )

    def _extract_selected_file(self) -> None:
        archive = self._selected_archive
        member = self._selected_file_path
        if archive is None or not member:
            return
        target_dir = pick_folder("Extract selected file to")
        if not target_dir:
            return
        self._last_extract_dir = target_dir
        self._start_job(
            f"Extracting {Path(member).name}",
            lambda: (
                "extract_file",
                self._extract_member(archive.path, member, target_dir),
            ),
        )

    def _extract_selected_archive(self) -> None:
        archive = self._selected_archive
        if archive is None:
            return
        target_dir = pick_folder("Extract archive to")
        if not target_dir:
            return
        self._last_extract_dir = target_dir
        self._start_job(
            f"Extracting {archive.archive_name}",
            lambda: (
                "extract_archive",
                self._extract_archive(archive.path, target_dir),
            ),
        )

    def _selected_audio_key(self) -> tuple[str, str] | None:
        archive = self._selected_archive
        member = self._selected_file_path
        if archive is None or not member or not _is_audio_member(member):
            return None
        return (archive.path, member)

    def _toggle_selected_audio(self) -> None:
        key = self._selected_audio_key()
        if key is None:
            return
        if self._audio_loaded_key == key and self._audio_player.has_audio:
            if self._audio_player.is_playing:
                self._audio_player.pause()
            else:
                self._audio_player.play()
            self._audio_error_msg = ""
            return
        cached = self._audio_cache.get(key, "")
        if cached and Path(cached).is_file():
            self._load_cached_audio(key, cached, autoplay=True)
            return
        if self._audio_busy and self._audio_pending_key == key:
            return
        self._audio_autoplay_key = key
        self._start_audio_job(
            key,
            f"Preparing audio preview for {Path(key[1]).name}",
            lambda: ("audio_preview", key, self._prepare_audio_preview(*key)),
        )

    def _stop_audio(self) -> None:
        self._audio_player.stop()

    def _prepare_audio_preview(self, archive_path: str, member_path: str) -> dict:
        cache_id = hashlib.sha1(f"{archive_path}|{member_path}".encode("utf-8")).hexdigest()[:16]
        target_dir = self._audio_preview_root / cache_id
        target_dir.mkdir(parents=True, exist_ok=True)
        payload = self._extract_member(archive_path, member_path, target_dir)
        written = Path(str(payload.get("written", ""))).resolve(strict=False)
        if not written.is_file():
            raise RuntimeError(f"Archive extract did not produce a file for {member_path}")
        preview_path = self._prepare_preview_file(written, target_dir)
        return {
            "cache_id": cache_id,
            "written": str(written),
            "preview_path": str(preview_path),
            "member_name": Path(member_path).name,
        }

    def _prepare_preview_file(self, extracted_path: Path, target_dir: Path) -> Path:
        suffix = extracted_path.suffix.lower()
        if suffix == ".xwm":
            return self._convert_xwm_to_wav(extracted_path, target_dir)
        if suffix == ".fuz":
            return self._extract_fuz_to_wav(extracted_path, target_dir)
        return extracted_path

    def _convert_xwm_to_wav(self, xwm_path: Path, output_dir: Path) -> Path:
        from creation_lib.paths import get_resource_dir as get_creation_lib_resource_dir

        tool = get_creation_lib_resource_dir() / "xWMAEncode.exe"
        if not tool.is_file():
            raise FileNotFoundError(f"xWMAEncode.exe not found: {tool}")
        wav_path = output_dir / f"{xwm_path.stem}.wav"
        if wav_path.is_file():
            return wav_path
        result = subprocess.run(
            [str(tool), str(xwm_path), str(wav_path)],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.returncode != 0 or not wav_path.is_file():
            detail = result.stderr.strip() or "XWM decode failed"
            raise RuntimeError(detail)
        return wav_path

    def _extract_fuz_to_wav(self, fuz_path: Path, output_dir: Path) -> Path:
        fuz_decode = get_resource_dir() / "BmlFuzDecode.exe"
        if not fuz_decode.is_file():
            raise FileNotFoundError(f"BmlFuzDecode.exe not found: {fuz_decode}")
        wav_path = output_dir / f"{fuz_path.stem}.wav"
        if wav_path.is_file():
            return wav_path
        with tempfile.TemporaryDirectory(prefix="modbox_bsa_fuz_") as tmp:
            tmp_dir = Path(tmp)
            temp_fuz = tmp_dir / fuz_path.name
            shutil.copy2(fuz_path, temp_fuz)
            result = subprocess.run(
                [str(fuz_decode), str(temp_fuz)],
                capture_output=True,
                text=True,
                check=False,
            )
            if result.returncode != 0:
                detail = result.stderr.strip() or "FUZ decode failed"
                raise RuntimeError(detail)
            temp_xwm = tmp_dir / f"{fuz_path.stem}.xwm"
            if not temp_xwm.is_file():
                raise RuntimeError(f"FUZ decode did not produce XWM for {fuz_path.name}")
            return self._convert_xwm_to_wav(temp_xwm, output_dir)

    def _load_cached_audio(self, key: tuple[str, str], preview_path: str, *, autoplay: bool) -> None:
        try:
            self._audio_player.load_file(preview_path, Path(key[1]).name)
        except Exception as exc:
            self._audio_player.set_error(str(exc))
            self._audio_error_msg = str(exc)
            self._audio_result_msg = ""
            return
        self._audio_loaded_key = key
        self._audio_error_msg = ""
        self._audio_result_msg = f"Preview ready for {Path(key[1]).name}."
        if autoplay:
            self._audio_player.play(from_pos=0.0)

    def _prepare_texture_preview(self, archive_path: str, member_path: str) -> dict:
        cache_id = hashlib.sha1(f"dds|{archive_path}|{member_path}".encode("utf-8")).hexdigest()[:16]
        target_dir = self._texture_preview_root / cache_id
        target_dir.mkdir(parents=True, exist_ok=True)
        payload = self._extract_member(archive_path, member_path, target_dir)
        written = Path(str(payload.get("written", ""))).resolve(strict=False)
        if not written.is_file():
            raise RuntimeError(f"Archive extract did not produce a DDS file for {member_path}")
        return {
            "cache_id": cache_id,
            "written": str(written),
            "preview_path": str(written),
            "member_name": Path(member_path).name,
        }

    def _upload_texture_rgba(self, rgba: np.ndarray):
        ctx = moderngl.get_context()
        height, width = rgba.shape[:2]
        tex = ctx.texture((width, height), 4, data=rgba.tobytes())
        tex.filter = (moderngl.LINEAR, moderngl.LINEAR)
        return tex

    def _load_texture_preview(self, key: tuple[str, str], preview_path: str) -> None:
        try:
            from creation_lib.dds import load_image

            img = load_image(preview_path)
            if img.mode != "RGBA":
                img = img.convert("RGBA")
            rgba = np.array(img, dtype=np.uint8)
            self._clear_texture_preview(release_cache_state=False)
            self._texture_preview_tex = self._upload_texture_rgba(rgba)
            self._texture_preview_size = img.size
            self._texture_preview_name = Path(key[1]).name
            self._texture_loaded_key = key
            self._texture_error_msg = ""
            self._texture_result_msg = f"Preview ready for {Path(key[1]).name}."
        except Exception as exc:
            self._clear_texture_preview()
            self._texture_error_msg = str(exc)
            self._texture_result_msg = ""

    def _clear_texture_preview(self, *, release_cache_state: bool = True) -> None:
        tex = self._texture_preview_tex
        if tex is not None:
            try:
                tex.release()
            except Exception:
                pass
        self._texture_preview_tex = None
        self._texture_preview_size = (0, 0)
        self._texture_preview_name = ""
        if release_cache_state:
            self._texture_loaded_key = None

    def _drop_audio_for_archive(self, archive_path: str) -> None:
        stale_keys = [key for key in self._audio_cache if key[0] == archive_path]
        for key in stale_keys:
            self._audio_cache.pop(key, None)
        if self._audio_loaded_key and self._audio_loaded_key[0] == archive_path:
            self._audio_player.clear()
            self._audio_loaded_key = None
            self._audio_result_msg = ""
            self._audio_error_msg = ""
        if self._audio_pending_key and self._audio_pending_key[0] == archive_path:
            self._audio_pending_key = None
            self._audio_autoplay_key = None
            self._audio_busy = False
            self._audio_job_label = ""

    def _drop_texture_for_archive(self, archive_path: str) -> None:
        stale_keys = [key for key in self._texture_cache if key[0] == archive_path]
        for key in stale_keys:
            self._texture_cache.pop(key, None)
        if self._texture_loaded_key and self._texture_loaded_key[0] == archive_path:
            self._clear_texture_preview()
            self._texture_result_msg = ""
            self._texture_error_msg = ""
        if self._texture_pending_key and self._texture_pending_key[0] == archive_path:
            self._texture_pending_key = None
            self._texture_busy = False
            self._texture_job_label = ""

    def _close_selected_archive(self) -> None:
        if self._selected_archive is None:
            return
        removed = self._archives.pop(self._selected_archive_idx)
        self._drop_audio_for_archive(removed.path)
        self._drop_texture_for_archive(removed.path)
        if not self._archives:
            self._selected_archive_idx = -1
            self._set_selected_file("")
        else:
            self._selected_archive_idx = min(self._selected_archive_idx, len(self._archives) - 1)
            self._set_selected_file("")
        self._mark_filter_dirty()
        self._status_msg = f"Closed {removed.archive_name}."
        self._result_msg = ""
        self._error_msg = ""

    def _close_all_archives(self) -> None:
        count = len(self._archives)
        self._archives.clear()
        self._selected_archive_idx = -1
        self._set_selected_file("")
        self._search_text = ""
        self._filtered_files = []
        self._filter_dirty = False
        self._current_match_index = -1
        self._audio_cache.clear()
        self._audio_loaded_key = None
        self._audio_pending_key = None
        self._audio_autoplay_key = None
        self._audio_result_msg = ""
        self._audio_error_msg = ""
        self._audio_player.clear()
        self._texture_cache.clear()
        self._texture_loaded_key = None
        self._texture_pending_key = None
        self._texture_result_msg = ""
        self._texture_error_msg = ""
        self._clear_texture_preview()
        self._status_msg = f"Closed {count} archive(s)." if count else "No archives were open."
        self._result_msg = ""
        self._error_msg = ""

    def _start_job(self, label: str, fn) -> None:
        if self._busy:
            self._status_msg = "Archive viewer is already busy."
            return
        self._busy = True
        self._job_label = label
        self._error_msg = ""
        self._result_msg = ""

        def _runner():
            try:
                result = fn()
                self._jobs.put(("result", result))
            except Exception as exc:  # pragma: no cover - surfaced in UI
                self._jobs.put(("error", str(exc)))

        threading.Thread(target=_runner, daemon=True).start()

    def _start_audio_job(self, key: tuple[str, str], label: str, fn) -> None:
        self._audio_busy = True
        self._audio_pending_key = key
        self._audio_job_label = label
        self._audio_error_msg = ""
        self._audio_result_msg = ""

        def _runner():
            try:
                result = fn()
                self._audio_jobs.put(("result", result))
            except Exception as exc:  # pragma: no cover - surfaced in UI
                self._audio_jobs.put(("error", (key, str(exc))))

        threading.Thread(target=_runner, daemon=True).start()

    def _start_texture_job(self, key: tuple[str, str], label: str, fn) -> None:
        self._texture_busy = True
        self._texture_pending_key = key
        self._texture_job_label = label
        self._texture_error_msg = ""
        self._texture_result_msg = ""

        def _runner():
            try:
                result = fn()
                self._texture_jobs.put(("result", result))
            except Exception as exc:  # pragma: no cover - surfaced in UI
                self._texture_jobs.put(("error", (key, str(exc))))

        threading.Thread(target=_runner, daemon=True).start()

    def _poll_jobs(self) -> None:
        processed = False
        while True:
            try:
                kind, payload = self._jobs.get_nowait()
            except queue.Empty:
                break
            processed = True
            if kind == "error":
                self._error_msg = str(payload)
                self._status_msg = ""
            else:
                self._handle_job_result(payload)
        if processed:
            self._busy = False
            self._job_label = ""

    def _poll_audio_jobs(self) -> None:
        processed = False
        while True:
            try:
                kind, payload = self._audio_jobs.get_nowait()
            except queue.Empty:
                break
            processed = True
            if kind == "error":
                key, message = payload
                if self._audio_pending_key == key:
                    self._audio_error_msg = str(message)
                    self._audio_result_msg = ""
                    self._audio_pending_key = None
                    self._audio_autoplay_key = None
            else:
                self._handle_audio_job_result(payload)
        if processed:
            self._audio_busy = False
            self._audio_job_label = ""

    def _poll_texture_jobs(self) -> None:
        processed = False
        while True:
            try:
                kind, payload = self._texture_jobs.get_nowait()
            except queue.Empty:
                break
            processed = True
            if kind == "error":
                key, message = payload
                if self._texture_pending_key == key:
                    self._texture_error_msg = str(message)
                    self._texture_result_msg = ""
                    self._texture_pending_key = None
            else:
                self._handle_texture_job_result(payload)
        if processed:
            self._texture_busy = False
            self._texture_job_label = ""

    def _handle_job_result(self, payload) -> None:
        action = payload[0]
        if action == "open_archive":
            _action, _normalized, data = payload
            archive = _archive_from_payload(data)
            self._archives.append(archive)
            self._selected_archive_idx = len(self._archives) - 1
            self._set_selected_file("")
            self._mark_filter_dirty()
            self._status_msg = f"Opened {archive.archive_name}."
            self._result_msg = f"Loaded {archive.file_count:,} file(s) from {archive.archive_name}."
            return
        if action == "extract_file":
            _action, data = payload
            self._status_msg = f"Extracted {data.get('file', '')}"
            self._result_msg = f"Wrote {data.get('written', '')}"
            return
        if action == "extract_archive":
            _action, data = payload
            self._status_msg = f"Extracted {data.get('file_count', 0):,} file(s)."
            self._result_msg = f"Wrote archive contents to {data.get('output_dir', '')}"

    def _handle_audio_job_result(self, payload) -> None:
        action, key, data = payload
        if action != "audio_preview":
            return
        if not any(archive.path == key[0] for archive in self._archives):
            self._audio_pending_key = None
            self._audio_autoplay_key = None
            return
        preview_path = str(data.get("preview_path", ""))
        if not preview_path:
            self._audio_error_msg = "Audio preview did not produce a playable file."
            self._audio_pending_key = None
            self._audio_autoplay_key = None
            return
        self._audio_cache[key] = preview_path
        autoplay = self._audio_autoplay_key == key
        self._audio_pending_key = None
        self._audio_autoplay_key = None
        self._load_cached_audio(key, preview_path, autoplay=autoplay)

    def _handle_texture_job_result(self, payload) -> None:
        action, key, data = payload
        if action != "texture_preview":
            return
        if not any(archive.path == key[0] for archive in self._archives):
            self._texture_pending_key = None
            return
        preview_path = str(data.get("preview_path", ""))
        if not preview_path:
            self._texture_error_msg = "Texture preview did not produce a DDS file."
            self._texture_pending_key = None
            return
        self._texture_cache[key] = preview_path
        self._texture_pending_key = None
        if self._selected_texture_key() != key:
            return
        self._load_texture_preview(key, preview_path)

    def _read_archive(self, archive_path: str) -> dict:
        _log.info("Reading archive: %s", archive_path)
        files = list(native_runtime.list_archive(archive_path) or [])
        info = dict(native_runtime.archive_info(archive_path) or {})
        return {
            "path": archive_path,
            "archive_name": Path(archive_path).name,
            "format": str(info.get("format", _archive_format_for_path(archive_path))),
            "version": info.get("version"),
            "file_count": int(info.get("file_count", len(files)) or len(files)),
            "backend": "native",
            "files": files,
        }

    def _extract_member(self, archive_path: str, member_path: str, output_dir: str | Path) -> dict:
        _log.info("Extracting archive member: %s -> %s", member_path, output_dir)
        data = native_runtime.extract_one(archive_path, member_path)
        if data is None:
            raise RuntimeError(f"Archive member not found: {member_path}")
        written = _safe_member_output_path(output_dir, member_path)
        written.parent.mkdir(parents=True, exist_ok=True)
        written.write_bytes(bytes(data))
        return {
            "archive": archive_path,
            "file": member_path,
            "written": str(written),
        }

    def _extract_archive(self, archive_path: str, output_dir: str | Path) -> dict:
        _log.info("Extracting archive: %s -> %s", archive_path, output_dir)
        out_dir = Path(output_dir).expanduser().resolve(strict=False)
        out_dir.mkdir(parents=True, exist_ok=True)
        count = native_runtime.extract_archive(
            archive_path,
            str(out_dir),
            format=_archive_format_for_path(archive_path),
            workers=0,
        )
        return {
            "archive": archive_path,
            "output_dir": str(out_dir),
            "file_count": int(count or 0),
        }
