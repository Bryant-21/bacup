"""Mass BSA tool — convert loose MO2 mod assets to native archives."""

from __future__ import annotations

import logging
import shutil
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

from imgui_bundle import imgui, icons_fontawesome_6 as fa

from creation_lib.ba2 import native_runtime
from creation_lib.core.game_profiles import get_profile
from creation_lib.esp.editor.session import detect_game
from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, draw_combo_field, draw_path_row, end_form

_log = logging.getLogger("tools.mass_bsa")

PLUGIN_EXTENSIONS = {".esp", ".esm", ".esl"}
ARCHIVE_EXTENSIONS = {".ba2", ".bsa"}

ASSET_DIRS = {
    "animations",
    "grass",
    "interface",
    "lodsettings",
    "materials",
    "meshes",
    "music",
    "programs",
    "scripts",
    "seq",
    "shadersfx",
    "sound",
    "strings",
    "terrain",
    "textures",
    "video",
    "vis",
}

GAME_CHOICES = [
    ("Fallout 4", "fo4"),
    ("Fallout 76", "fo76"),
    ("Skyrim SE", "skyrimse"),
    ("Starfield", "starfield"),
    ("Fallout 3", "fo3"),
    ("Fallout: New Vegas", "fnv"),
    ("Oblivion", "oblivion"),
]


@dataclass
class MassBsaMod:
    path: Path
    name: str
    has_archive: bool
    asset_dirs: list[str]
    plugin_names: list[str]
    game: str
    selected: bool = False
    status: str = "Ready"
    archives_written: list[str] = field(default_factory=list)

    @property
    def primary_plugin_stem(self) -> str:
        if self.plugin_names:
            return Path(self.plugin_names[0]).stem
        return self.name


def _has_files(path: Path) -> bool:
    return path.is_dir() and any(item.is_file() for item in path.rglob("*"))


def _plugin_sort_key(path: Path) -> tuple[int, str]:
    priority = {".esm": 0, ".esp": 1, ".esl": 2}.get(path.suffix.lower(), 9)
    return priority, path.name.lower()


def _collect_plugins(mod_dir: Path) -> list[Path]:
    return sorted(
        [path for path in mod_dir.iterdir() if path.is_file() and path.suffix.lower() in PLUGIN_EXTENSIONS],
        key=_plugin_sort_key,
    )


def _detect_mod_game(mod_dir: Path, plugins: list[Path], default_game: str) -> str:
    for plugin in plugins:
        game = detect_game(plugin, fallback=default_game)
        if game:
            return game
    return default_game


def scan_mo2_mods(root: str | Path, default_game: str = "fo4") -> list[MassBsaMod]:
    root_path = Path(root)
    if not root_path.is_dir():
        return []

    mods: list[MassBsaMod] = []
    for child in sorted(root_path.iterdir(), key=lambda path: path.name.lower()):
        if not child.is_dir():
            continue
        archives = [path for path in child.iterdir() if path.is_file() and path.suffix.lower() in ARCHIVE_EXTENSIONS]
        asset_dirs = [
            item.name
            for item in sorted(child.iterdir(), key=lambda path: path.name.lower())
            if item.is_dir() and item.name.lower() in ASSET_DIRS and _has_files(item)
        ]
        plugins = _collect_plugins(child)
        mods.append(
            MassBsaMod(
                path=child,
                name=child.name,
                has_archive=bool(archives),
                asset_dirs=asset_dirs,
                plugin_names=[plugin.name for plugin in plugins],
                game=_detect_mod_game(child, plugins, default_game),
                selected=bool(asset_dirs) and not archives,
                status="Ready" if asset_dirs else "No loose asset folders",
            )
        )
    return mods


def _archive_type(game: str, *, textures: bool) -> str:
    game = game.lower()
    if game == "skyrimse":
        return "sse"
    if game == "skyrim":
        return "tes5"
    if game == "oblivion":
        return "tes4"
    if game == "starfield":
        return "starfielddds" if textures else "starfield"
    if textures and game in {"fo4", "fo76"}:
        return f"{game}dds"
    return game


def _archive_ext(game: str) -> str:
    return ".ba2" if get_profile(game).archive_format == "ba2" else ".bsa"


def _stage_dirs(source_mod: Path, stage_root: Path, dir_names: list[str]) -> bool:
    copied = False
    for dir_name in dir_names:
        src = source_mod / dir_name
        if not _has_files(src):
            continue
        shutil.copytree(src, stage_root / dir_name)
        copied = True
    return copied


def convert_mod_to_archives(mod: MassBsaMod, *, compression_level: int = 9) -> list[Path]:
    """Pack one scanned mod and delete only the loose asset dirs that were packed."""
    profile = get_profile(mod.game)
    archive_ext = _archive_ext(mod.game)
    base_name = mod.primary_plugin_stem
    written: list[Path] = []

    texture_dirs = [name for name in mod.asset_dirs if name.lower() == "textures"]
    main_dirs = [name for name in mod.asset_dirs if name.lower() != "textures"]

    with tempfile.TemporaryDirectory(prefix="mass_bsa_") as tmp:
        tmp_root = Path(tmp)

        if main_dirs:
            main_stage = tmp_root / "main"
            main_stage.mkdir()
            if _stage_dirs(mod.path, main_stage, main_dirs):
                if profile.archive_format == "ba2":
                    out = mod.path / f"{base_name} - Main{archive_ext}"
                else:
                    out = mod.path / f"{base_name}{archive_ext}"
                native_runtime.pack_archive(
                    str(main_stage),
                    str(out),
                    _archive_type(mod.game, textures=False),
                    compress=True,
                    compression_level=compression_level,
                    share_data=False,
                )
                written.append(out)

        if texture_dirs:
            tex_stage = tmp_root / "textures"
            tex_stage.mkdir()
            if _stage_dirs(mod.path, tex_stage, texture_dirs):
                suffix = " - Textures" if profile.archive_format == "ba2" else " - Textures"
                out = mod.path / f"{base_name}{suffix}{archive_ext}"
                native_runtime.pack_archive(
                    str(tex_stage),
                    str(out),
                    _archive_type(mod.game, textures=True),
                    compress=True,
                    compression_level=compression_level,
                    share_data=False,
                )
                written.append(out)

    for dir_name in mod.asset_dirs:
        target = mod.path / dir_name
        if target.is_dir():
            shutil.rmtree(target)

    return written


class MassBSATool(BaseTool):
    name = "Mass BSA"
    tool_id = "mass_bsa"
    description = "Pack loose MO2 mod asset folders into native archives"
    category = "Mod Tools"

    def __init__(self):
        super().__init__()
        self._mods_root = r"N:\ModOrganizer\Fallout 4 - Mod\mods"
        self._default_game_idx = 0
        self._mods: list[MassBsaMod] = []
        self._show_ready_only = True

    def draw_content(self) -> None:
        if self._running:
            imgui.begin_disabled()
        if begin_form("##mass_bsa"):
            _, clicked = draw_path_row("MO2 Mods", self._mods_root, btn_label=getattr(fa, "ICON_FA_FOLDER_OPEN", "Open"))
            if clicked:
                self.open_folder_dialog()
            _, self._default_game_idx = draw_combo_field(
                "Fallback Game",
                [label for label, _game in GAME_CHOICES],
                self._default_game_idx,
            )
            end_form()

        if imgui.button("Scan", imgui.ImVec2(120, 0)):
            self._start_scan()
        imgui.same_line()
        if imgui.button("Select All", imgui.ImVec2(120, 0)):
            for mod in self._mods:
                if not mod.has_archive and mod.asset_dirs:
                    mod.selected = True
        imgui.same_line()
        if imgui.button("Clear", imgui.ImVec2(90, 0)):
            for mod in self._mods:
                mod.selected = False
        imgui.same_line()
        _, self._show_ready_only = imgui.checkbox("Hide mods with archives", self._show_ready_only)

        total = len(self._mods)
        archived = sum(1 for mod in self._mods if mod.has_archive)
        ready = sum(1 for mod in self._mods if not mod.has_archive and mod.asset_dirs)
        selected = sum(1 for mod in self._mods if mod.selected and not mod.has_archive and mod.asset_dirs)
        imgui.text(f"{archived}/{total} mods have archive files. {ready} need archives. {selected} selected.")

        imgui.separator()
        imgui.begin_child("##mass_bsa_mods", imgui.ImVec2(0, max(180, imgui.get_content_region_avail().y - 52)), child_flags=imgui.ChildFlags_.borders)
        if imgui.begin_table("##mass_bsa_table", 5, imgui.TableFlags_.sizing_stretch_prop | imgui.TableFlags_.row_bg | imgui.TableFlags_.borders_inner_v):
            imgui.table_setup_column("", imgui.TableColumnFlags_.width_fixed, 28)
            imgui.table_setup_column("Mod")
            imgui.table_setup_column("Game", imgui.TableColumnFlags_.width_fixed, 80)
            imgui.table_setup_column("Plugin")
            imgui.table_setup_column("Status")
            imgui.table_headers_row()
            for idx, mod in enumerate(self._mods):
                if self._show_ready_only and mod.has_archive:
                    continue
                imgui.table_next_row()
                imgui.table_set_column_index(0)
                if mod.has_archive or not mod.asset_dirs:
                    imgui.begin_disabled()
                changed, mod.selected = imgui.checkbox(f"##sel_{idx}", mod.selected)
                if mod.has_archive or not mod.asset_dirs:
                    imgui.end_disabled()
                imgui.table_set_column_index(1)
                imgui.text(mod.name)
                imgui.table_set_column_index(2)
                imgui.text(mod.game)
                imgui.table_set_column_index(3)
                imgui.text(mod.plugin_names[0] if mod.plugin_names else "(none)")
                imgui.table_set_column_index(4)
                imgui.text(mod.status)
            imgui.end_table()
        imgui.end_child()

        if imgui.button("Convert Selected", imgui.ImVec2(160, 0)):
            selected_mods = [mod for mod in self._mods if mod.selected and not mod.has_archive and mod.asset_dirs]
            if not selected_mods:
                self._error_msg = "Select at least one mod with loose asset folders."
                if self._running:
                    imgui.end_disabled()
                return
            self._start_batch(self._convert_selected, selected_mods)

        if self._running:
            imgui.end_disabled()
            self._draw_loading_mask()

    def open_folder_dialog(self) -> None:
        if self._running:
            return
        path = pick_folder("Select Mod Organizer mods folder")
        if path:
            self._mods_root = path
            self._start_scan()

    def _start_scan(self) -> None:
        root = Path(self._mods_root)
        if not root.is_dir():
            self._error_msg = f"Folder does not exist: {root}"
            return
        self._start_batch(self._run_scan)

    def _run_scan(self) -> None:
        root = Path(self._mods_root)
        default_game = GAME_CHOICES[self._default_game_idx][1]
        self._on_progress(0, 1, f"Scanning {root.name}")
        self._mods = scan_mo2_mods(root, default_game=default_game)
        self._on_progress(1, 1, "Scan complete")
        self._result_msg = f"Scanned {len(self._mods)} mod folders."
        self._error_msg = ""

    def _convert_selected(self, selected_mods: list[MassBsaMod]) -> None:
        total = len(selected_mods)
        converted = 0
        failed = 0
        for index, mod in enumerate(selected_mods, start=1):
            if self._cancel_requested:
                break
            self._on_progress(index - 1, total, f"Packing {mod.name}")
            try:
                archives = convert_mod_to_archives(mod)
                mod.archives_written = [path.name for path in archives]
                mod.status = f"Packed {len(archives)} archive(s)"
                mod.has_archive = bool(archives)
                mod.selected = False
                converted += 1
            except Exception as exc:
                failed += 1
                mod.status = f"Failed: {exc}"
                _log.exception("Mass BSA failed for %s", mod.path)
        self._on_progress(total, total, "Done")
        self._result_msg = f"Converted {converted} mod(s). {failed} failed."

    def _draw_loading_mask(self) -> None:
        win_pos = imgui.get_window_pos()
        win_size = imgui.get_window_size()
        draw_list = imgui.get_foreground_draw_list()

        bg_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.0, 0.0, 0.0, 0.58))
        draw_list.add_rect_filled(
            imgui.ImVec2(win_pos.x, win_pos.y),
            imgui.ImVec2(win_pos.x + win_size.x, win_pos.y + win_size.y),
            bg_col,
        )

        spinner = ["|", "/", "-", "\\"][int(imgui.get_time() * 8) % 4]
        text = f"{spinner}  {self._status_msg or 'Working...'}"
        text_size = imgui.calc_text_size(text)
        pad = 14.0
        cx = win_pos.x + (win_size.x - text_size.x) * 0.5
        cy = win_pos.y + (win_size.y - text_size.y) * 0.5
        panel_min = imgui.ImVec2(cx - pad, cy - pad)
        panel_max = imgui.ImVec2(cx + text_size.x + pad, cy + text_size.y + pad)
        panel_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.14, 0.14, 0.16, 1.0))
        border_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.36, 0.36, 0.42, 1.0))
        text_col = imgui.color_convert_float4_to_u32(imgui.ImVec4(0.92, 0.92, 0.94, 1.0))
        draw_list.add_rect_filled(panel_min, panel_max, panel_col, 6.0)
        draw_list.add_rect(panel_min, panel_max, border_col, 6.0)
        draw_list.add_text(imgui.ImVec2(cx, cy), text_col, text)

    def get_default_settings(self) -> dict:
        return {
            "mods_root": self._mods_root,
            "default_game_idx": self._default_game_idx,
            "show_ready_only": self._show_ready_only,
        }

    def apply_settings(self, settings: dict) -> None:
        self._mods_root = settings.get("mods_root", self._mods_root)
        self._default_game_idx = max(0, min(int(settings.get("default_game_idx", 0)), len(GAME_CHOICES) - 1))
        self._show_ready_only = bool(settings.get("show_ready_only", True))

    def collect_settings(self) -> dict:
        return {
            "mods_root": self._mods_root,
            "default_game_idx": self._default_game_idx,
            "show_ready_only": self._show_ready_only,
        }
