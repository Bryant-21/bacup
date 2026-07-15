"""PapyrusWorkspace — Papyrus script editor workspace for the toolkit."""
from __future__ import annotations

import hashlib
import logging
import os
import tempfile
from pathlib import Path

from imgui_bundle import hello_imgui, imgui, icons_fontawesome_6 as fa

from ui.toolkit.app_paths import get_app_root
from creation_lib.ui.shell import BaseWorkspace, make_window
from creation_lib.ui.widgets.user_guide import UserGuide

_log = logging.getLogger("toolkit.papyrus")
_NS = "##papyrus"

_DEFAULT_SETTINGS = {
    "open_files": [],
    "active_file": None,
    "font_scale": 1.0,
    "extra_line_spacing": 0.0,
    "tree_state": {},
}


class PapyrusWorkspace(BaseWorkspace):
    """Workspace wrapper for the Papyrus script editor."""

    name = "Papyrus"
    icon = "PSC"
    id = "papyrus"

    def get_user_guide(self):
        from ui.papyrus.panels.help_panel import USER_GUIDE_MARKDOWN

        return UserGuide("Papyrus Editor User Guide", USER_GUIDE_MARKDOWN, "papyrus_user_guide")

    def __init__(self, toolkit_settings=None):
        super().__init__(toolkit_settings)

        self._lsp = None          # LspService — created in initialize()
        self._file_tree = None    # FileTreePanel
        self._editor_tabs = None  # EditorTabsPanel
        self._diagnostics = None  # DiagnosticsPanel
        self._help_panel = None

        self._pending_open_files: list[str] = []
        self._pending_active_file: str | None = None
        self._pending_font_scale: float = 1.0
        self._pending_extra_line_spacing: float = 0.0
        self._pending_tree_state: dict = {}
        self.mono_font = None  # set by ToolkitApp._post_init after fonts are loaded

    def get_dockable_windows(self) -> list[hello_imgui.DockableWindow]:
        return [
            make_window(f"Files{_NS}", "LeftDock"),
            make_window(f"Editor{_NS}", "MainDockSpace"),
            make_window(f"Diagnostics{_NS}", "BottomDock"),
            make_window(f"Help{_NS}", "RightDock", is_visible=False),
        ]

    def get_required_addons(self) -> dict:
        return {}

    def initialize(self) -> None:
        from ui.papyrus.papyrus_lsp_service import LspService
        from ui.papyrus.papyrus_editor import PapyrusEditorApp
        from ui.papyrus.panels.file_tree_panel import FileTreePanel
        from ui.papyrus.panels.editor_tabs_panel import EditorTabsPanel
        from ui.papyrus.panels.diagnostics_panel import DiagnosticsPanel
        from ui.papyrus.panels.help_panel import HelpPanel

        # Collect script source dirs for all configured games
        named_roots: list[tuple[str, str]] = []
        lsp_extra_dirs: list[str] = []
        if self._toolkit_settings is not None:
            from creation_lib.core.game_profiles import GAME_PROFILES
            for gid, profile in GAME_PROFILES.items():
                if not profile.papyrus_source_subpath:
                    continue
                scripts_dir = self._toolkit_settings.get_scripts_source_dir(gid)
                if scripts_dir and os.path.isdir(scripts_dir):
                    norm = os.path.normpath(scripts_dir)
                    if norm not in lsp_extra_dirs:
                        named_roots.append((f"{profile.display_name} Scripts", norm))
                        lsp_extra_dirs.append(norm)
                # Per-game user/base Papyrus source dirs from settings
                script_dirs = self._toolkit_settings.get_script_source_dirs(gid)
                for kind, label_suffix in (("user", "User Scripts"), ("base", "Base Scripts")):
                    val = script_dirs.get(kind, "")
                    if val and os.path.isdir(val):
                        norm = os.path.normpath(val)
                        if norm not in lsp_extra_dirs:
                            named_roots.append(
                                (f"{profile.display_name} {label_suffix}", norm)
                            )
                            lsp_extra_dirs.append(norm)
            # Legacy: also include any manually-configured script_source_paths
            for p in self._toolkit_settings.get_script_source_paths():
                norm = os.path.normpath(p)
                if norm not in lsp_extra_dirs:
                    named_roots.append((os.path.basename(norm), norm))
                    lsp_extra_dirs.append(norm)

        self._lsp = LspService(extra_source_dirs=lsp_extra_dirs)
        self._lsp.start()

        project_root = str(get_app_root())
        self._app = PapyrusEditorApp(lsp=self._lsp)

        self._file_tree = FileTreePanel(
            self._app,
            project_root=project_root,
            extra_roots=named_roots,
        )
        self._file_tree.window_name = f"Files{_NS}"
        if self._pending_tree_state:
            self._file_tree.set_tree_state(self._pending_tree_state)
        self._file_tree.start_watching()

        self._editor_tabs = EditorTabsPanel(self._app, mono_font=self.mono_font)
        self._editor_tabs.font_scale = self._pending_font_scale
        self._editor_tabs.extra_line_spacing = self._pending_extra_line_spacing
        self._editor_tabs.window_name = f"Editor{_NS}"

        self._diagnostics = DiagnosticsPanel(self._app)
        self._diagnostics.window_name = f"Diagnostics{_NS}"

        self._help_panel = HelpPanel()

        # Restore open files from settings
        for path in self._pending_open_files:
            if os.path.exists(path):
                self._app.open_file(path)
        if self._pending_active_file and self._pending_active_file in self._app.open_files:
            self._app.active_path = self._pending_active_file

        self._bind_panels({
            f"Files{_NS}": self._file_tree.draw,
            f"Editor{_NS}": self._editor_tabs.draw,
            f"Diagnostics{_NS}": self._diagnostics.draw,
            f"Help{_NS}": self._help_panel.draw,
        })
        self._initialized = True
        _log.info("PapyrusWorkspace initialized")

    def draw_menu(self) -> None:
        from imgui_bundle import imgui
        if imgui.begin_menu("File"):
            if imgui.menu_item("Open...", "Ctrl+O", False)[0]:
                self._open_file_dialog()
            if imgui.menu_item("Save", "Ctrl+S", False)[0]:
                if self._app and self._app.active_path:
                    self._app.save_file(self._app.active_path)
            imgui.end_menu()
        if self._view_helper:
            self._view_helper.draw([
                "Files##papyrus",
                "Editor##papyrus",
                "Diagnostics##papyrus",
                f"Help{_NS}",
            ])

    def has_toolbar(self) -> bool:
        return True

    def draw_toolbar(self, icon_font=None) -> None:
        from imgui_bundle import imgui, icons_fontawesome_6 as fa

        def _btn(icon):
            if icon_font:
                imgui.push_font(icon_font, icon_font.legacy_size)
            clicked = imgui.button(icon)
            if icon_font:
                imgui.pop_font()
            return clicked

        # New
        if _btn(fa.ICON_FA_FILE_CIRCLE_PLUS):
            self._new_file_dialog()
        imgui.set_item_tooltip("New Script (Ctrl+N)")

        imgui.same_line()

        # Open
        if _btn(fa.ICON_FA_FOLDER_OPEN):
            self._open_file_dialog()
        imgui.set_item_tooltip("Open Script")

        imgui.same_line()

        # Save — disabled when no file is active
        no_file = not self._app or not self._app.active_path
        if no_file:
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_FLOPPY_DISK):
            self._app.save_file(self._app.active_path)
        imgui.set_item_tooltip("Save Script (Ctrl+S)")
        if no_file:
            imgui.end_disabled()

        imgui.same_line()
        imgui.text("|")
        imgui.same_line()

        # Undo / Redo
        active_buf = self._app.open_files.get(self._app.active_path) if self._app and self._app.active_path else None
        editor = active_buf.editor if active_buf else None

        if not editor or not editor.can_undo():
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_ROTATE_LEFT):
            editor.undo()
        imgui.set_item_tooltip("Undo (Ctrl+Z)")
        if not editor or not editor.can_undo():
            imgui.end_disabled()

        imgui.same_line()

        if not editor or not editor.can_redo():
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_ROTATE_RIGHT):
            editor.redo()
        imgui.set_item_tooltip("Redo (Ctrl+Y)")
        if not editor or not editor.can_redo():
            imgui.end_disabled()

        imgui.same_line()
        imgui.text("|")
        imgui.same_line()

        # Find / Replace
        if not editor:
            imgui.begin_disabled()
        if _btn(fa.ICON_FA_MAGNIFYING_GLASS):
            editor.open_find()
        imgui.set_item_tooltip("Find  (Ctrl+F)")
        imgui.same_line()
        if _btn(fa.ICON_FA_RIGHT_LEFT):
            editor.open_find_replace()
        imgui.set_item_tooltip("Find & Replace  (Ctrl+H)")
        if not editor:
            imgui.end_disabled()

        imgui.same_line()
        imgui.text("|")
        imgui.same_line()

        # View settings — font scale & line spacing
        if _btn(fa.ICON_FA_TEXT_HEIGHT):
            imgui.open_popup("##papyrus_view")
        imgui.set_item_tooltip("View Settings")

        if imgui.begin_popup("##papyrus_view"):
            et = self._editor_tabs
            imgui.text("View Settings")
            imgui.separator()
            imgui.set_next_item_width(160)
            changed, val = imgui.slider_float("Font Scale", et.font_scale, 0.5, 2.0, "%.2f")
            if changed:
                et.font_scale = val
            imgui.set_next_item_width(160)
            changed, val = imgui.slider_float("Line Spacing", et.extra_line_spacing, 0.0, 16.0, "%.0f px")
            if changed:
                et.extra_line_spacing = val
            if imgui.button("Reset"):
                et.font_scale = 1.0
                et.extra_line_spacing = 0.0
            imgui.end_popup()

    def _toggle_help_panel(self):
        dp = hello_imgui.get_runner_params().docking_params
        for w in dp.dockable_windows:
            if w.label == f"Help{_NS}":
                w.is_visible = not w.is_visible
                break

    def toggle_user_guide(self) -> None:
        self._toggle_help_panel()

    def _open_file_dialog(self) -> None:
        """Open a native file dialog to pick a Papyrus source or compiled script."""
        try:
            from creation_lib.ui.widgets.pick_folder import pick_file
            path = pick_file(
                "Open Papyrus Script",
                [
                    ("Papyrus Scripts", "*.psc *.pex"),
                    ("Papyrus Source", "*.psc"),
                    ("Papyrus Compiled", "*.pex"),
                    ("All Files", "*.*"),
                ],
                default_path=str(get_app_root()),
            )
            if path:
                self.open_file(path)
        except Exception as e:
            _log.error("File dialog error: %s", e)

    def _new_file_dialog(self) -> None:
        """Open a native save-as dialog to create a new .psc file."""
        try:
            from creation_lib.ui.widgets.pick_folder import pick_save_file
            from pathlib import Path

            # Default to first mod's Scripts/Source/User/ if it exists
            initial_dir = str(get_app_root())
            mods_dir = Path(get_app_root()) / "mods"
            if mods_dir.is_dir():
                for mod_dir in sorted(mods_dir.iterdir()):
                    scripts_dir = mod_dir / "Scripts" / "Source" / "User"
                    if scripts_dir.is_dir():
                        initial_dir = str(scripts_dir)
                        break

            path = pick_save_file(
                "New Papyrus Script",
                [("Papyrus Script", "*.psc"), ("All Files", "*.*")],
                default_ext=".psc",
                default_path=initial_dir,
            )
            if not path:
                return
            path = path if path.endswith(".psc") else path + ".psc"
            script_name = Path(path).stem
            template = f"Scriptname {script_name}\n\n"
            Path(path).write_text(template, encoding="utf-8")
            self._app.open_file(path)
            self._app.active_path = path
        except Exception as e:
            _log.error("New file dialog error: %s", e)

    def draw(self) -> None:
        if not self.active or not self._initialized:
            return

        # Docked panels drawn by hello_imgui via DockableWindow gui_functions.

        # Handle keybindings
        from imgui_bundle import imgui
        io = imgui.get_io()
        if imgui.is_key_pressed(imgui.Key.f1) and not io.want_text_input:
            self._toggle_help_panel()
        if io.key_ctrl and imgui.is_key_pressed(imgui.Key.s):
            if self._app.active_path:
                self._app.save_file(self._app.active_path)
        if io.key_ctrl and imgui.is_key_pressed(imgui.Key.o):
            self._open_file_dialog()
        if io.key_ctrl and imgui.is_key_pressed(imgui.Key.n):
            self._new_file_dialog()

    def on_activate(self) -> None:
        from imgui_bundle import hello_imgui
        # Text editors need continuous redraws for cursor blink, so disable
        # idling entirely while this workspace is active.
        hello_imgui.get_runner_params().fps_idling.enable_idling = False
        self.active = True
        _log.info("PapyrusWorkspace activated")

    def on_deactivate(self) -> None:
        from imgui_bundle import hello_imgui
        hello_imgui.get_runner_params().fps_idling.enable_idling = True
        self.active = False
        _log.info("PapyrusWorkspace deactivated")

    def cleanup(self) -> None:
        if self._file_tree:
            self._file_tree.stop_watching()
        if self._app:
            self._app.cleanup()
        if self._lsp:
            self._lsp.stop()
        _log.info("PapyrusWorkspace cleaned up")

    def get_settings_defaults(self) -> dict:
        return dict(_DEFAULT_SETTINGS)

    def apply_settings(self, settings: dict) -> None:
        self._pending_open_files = settings.get("open_files", [])
        self._pending_active_file = settings.get("active_file")
        self._pending_tree_state = settings.get("tree_state", {})
        if self._editor_tabs:
            self._editor_tabs.font_scale = settings.get("font_scale", 1.0)
            self._editor_tabs.extra_line_spacing = settings.get("extra_line_spacing", 0.0)
        else:
            # Stash for when initialize() runs
            self._pending_font_scale = settings.get("font_scale", 1.0)
            self._pending_extra_line_spacing = settings.get("extra_line_spacing", 0.0)
        if self._file_tree:
            self._file_tree.set_tree_state(self._pending_tree_state)

    def collect_settings(self) -> dict:
        et = self._editor_tabs
        return {
            "open_files": list(self._app.open_files.keys()) if self._app else [],
            "active_file": self._app.active_path if self._app else None,
            "font_scale": et.font_scale if et else 1.0,
            "extra_line_spacing": et.extra_line_spacing if et else 0.0,
            "tree_state": self._file_tree.get_tree_state() if self._file_tree else {},
        }

    @staticmethod
    def _decompiled_pex_path(path: str) -> Path:
        """Return a stable temp .psc path for a decompiled .pex file."""
        src = Path(path)
        digest = hashlib.sha1(str(src.resolve(strict=False)).encode("utf-8")).hexdigest()[:10]
        temp_root = Path(tempfile.gettempdir()) / "modbox21" / "papyrus_decompiled"
        temp_root.mkdir(parents=True, exist_ok=True)
        return temp_root / f"{src.stem}.{digest}.decompiled.psc"

    def handle_file_drop(
        self,
        paths: list[str],
        *,
        x: float | None = None,
        y: float | None = None,
    ) -> bool:
        if self._app is None:
            return False
        for path in paths:
            ext = Path(path).suffix.lower()
            if ext in (".psc", ".pex"):
                self.open_file(path)
                return True
        return False

    def open_file(self, path: str) -> None:
        """Open a .psc directly or decompile a .pex into a read-only temp .psc."""
        normalized = os.path.normpath(path)
        ext = Path(normalized).suffix.lower()

        if ext == ".psc":
            self._app.open_file(normalized)
            self._app.active_path = normalized
            return

        if ext != ".pex":
            _log.warning("Unsupported Papyrus file type: %s", path)
            return

        from creation_lib.pex import decompile_pex

        decompiled_path = self._decompiled_pex_path(normalized)
        try:
            source = decompile_pex(Path(normalized))
            decompiled_path.write_text(source, encoding="utf-8")
            self._app.open_file(str(decompiled_path))
            self._app.active_path = str(decompiled_path)
            buf = self._app.open_files.get(str(decompiled_path))
            if buf is not None:
                # Decompiled output is a view of compiled bytecode, not an editable source file.
                buf.editor.set_read_only_enabled(True)
            _log.info("Decompiled PEX for viewing: %s -> %s", normalized, decompiled_path)
        except Exception as e:
            _log.error("Failed to decompile %s: %s", normalized, e)
