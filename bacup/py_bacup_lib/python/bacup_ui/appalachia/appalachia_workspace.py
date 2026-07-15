"""B.A.C.U.P. workspace for the supported Bethesda conversion projects."""
from __future__ import annotations

import logging
import subprocess
import sys

from imgui_bundle import hello_imgui, imgui

from creation_lib.ui.shell import BaseWorkspace, make_window
from bacup_ui.appalachia.window_title import appalachia_window_title

_log = logging.getLogger("toolkit.appalachia")
_NS = "##appalachia"
_CHANGELOG_POPUP = f"Changelog{_NS}"
_SETUP_CONFIRM_POPUP = f"Re-run Setup{_NS}"

_COL_OK = imgui.ImVec4(0.40, 0.85, 0.40, 1.0)
_COL_WARN = imgui.ImVec4(1.00, 0.85, 0.30, 1.0)
_COL_ACCENT = imgui.ImVec4(0.55, 0.78, 1.00, 1.0)

APP_NAME = "B.A.C.U.P."
APP_EXPANSION = "Bethesda Asset Converter Universal Platform"

_PROJECTS = (
    ("appalachia", "Tales From Appalachia", "fo76:fo4"),
    ("wasteland", "Legends of the Wasteland", "fnvfo3:fo4"),
    ("north", "Fables of the North", "skyrimse:fo4"),
)

# The FNV/FO3 and Skyrim workflows remain registered but are not ready for UI use.
_ENABLED_PROJECTS = (_PROJECTS[0],)


class AppalachiaWorkspace(BaseWorkspace):
    name = appalachia_window_title()
    icon = "BACUP"
    id = "appalachia"

    def __init__(self, toolkit_settings=None):
        super().__init__(toolkit_settings)
        self._regen_panel = None
        self._regen_panels = {}
        self._log_panel = None
        self._log_panels = {}
        self._runner = None
        self._runner_owner = None
        self._active_project_id = "appalachia"
        self._changelog_pending = False
        self._setup_confirm_pending = False

    def get_dockable_windows(self):
        return [make_window(f"{APP_NAME}{_NS}", "MainDockSpace")]

    def initialize(self) -> None:
        from bacup_ui.conversion.panels.conversion_log import ConversionLogPanel
        from bacup_ui.conversion.panels.regen_panel import RegenPanel

        for project_id, label, pair_id in _ENABLED_PROJECTS:
            log_panel = ConversionLogPanel(self)
            panel = RegenPanel(
                self,
                log_panel=log_panel,
                fixed_pair_id=pair_id,
                project_id=project_id,
                project_label=label,
            )
            self._log_panels[project_id] = log_panel
            self._regen_panels[project_id] = panel

        self._regen_panel = self._regen_panels["appalachia"]
        self._log_panel = self._log_panels["appalachia"]
        self._bind_panels({f"{APP_NAME}{_NS}": self._draw_projects})
        self._initialized = True
        _log.info("B.A.C.U.P. workspace initialized")

    def _draw_projects(self) -> None:
        if not imgui.begin(f"{APP_NAME}{_NS}"):
            imgui.end()
            return
        imgui.text(APP_NAME)
        imgui.text_disabled(APP_EXPANSION)
        imgui.separator()
        if imgui.begin_tab_bar(f"{_NS}_projects"):
            for project_id, label, _pair_id in _ENABLED_PROJECTS:
                opened = imgui.begin_tab_item(label)
                if isinstance(opened, tuple):
                    opened = opened[0]
                if opened:
                    self._active_project_id = project_id
                    self._regen_panels[project_id].draw_project()
                    imgui.end_tab_item()
            imgui.end_tab_bar()
        imgui.end()

    def start_conversion_runner(self, owner, runner) -> None:
        if self._runner is not None and not self._runner.done:
            raise RuntimeError("Another B.A.C.U.P. conversion is already running")
        self._runner = runner
        self._runner_owner = owner
        runner.start()

    def draw_menu(self) -> None:
        if imgui.begin_menu("Help"):
            if imgui.menu_item("Changelog...", "", False)[0]:
                self._changelog_pending = True
            imgui.end_menu()
        if imgui.begin_menu("Setup"):
            if imgui.menu_item("Re-run Setup / Re-extract Game Data...", "", False)[0]:
                self._setup_confirm_pending = True
            imgui.end_menu()

    def draw(self) -> None:
        if not self.active or not self._initialized:
            return
        if self._runner:
            events = self._runner.drain()
            owner = self._runner_owner or self._regen_panel
            owner_log = getattr(owner, "_log_panel", None)
            for event in events:
                owner.handle_event(event)
                if owner_log is not None:
                    owner_log.handle_event(event)
            if self._runner.done and not events:
                self._runner = None
                self._runner_owner = None
        self._draw_changelog_popup()
        self._draw_setup_confirm_popup()

    def _changelog_entries(self) -> list[tuple[str, bool, tuple[str, ...]]]:
        from bacup_lib.upgrade_manifest import (
            bundled_upgrade_manifest_path,
            load_upgrade_manifest,
        )

        try:
            manifest = load_upgrade_manifest(bundled_upgrade_manifest_path())
        except Exception:
            return []
        pair_id = self._active_pair_id()
        entries = []
        for version in reversed(manifest.versions):
            notes = version.notes_for_conversion(pair_id)
            if notes:
                entries.append((version.id, version.id == manifest.current, notes))
        return entries

    def _draw_changelog_popup(self) -> None:
        if self._changelog_pending:
            imgui.open_popup(_CHANGELOG_POPUP)
            self._changelog_pending = False
        imgui.set_next_window_size(imgui.ImVec2(480, 400), imgui.Cond_.appearing)
        opened, _ = imgui.begin_popup_modal(_CHANGELOG_POPUP)
        if opened:
            imgui.text(f"Changelog — {self._active_project_label()}")
            imgui.separator()
            entries = self._changelog_entries()
            imgui.begin_child(
                f"changelog_body{_NS}",
                imgui.ImVec2(0, -imgui.get_frame_height_with_spacing()),
            )
            if not entries:
                imgui.text_disabled("No changelog available.")
            else:
                for version_id, is_current, notes in entries:
                    imgui.text_colored(_COL_ACCENT, version_id)
                    if is_current:
                        imgui.same_line()
                        imgui.text_colored(_COL_OK, "(current)")
                    imgui.indent()
                    for note in notes:
                        imgui.bullet_text(note)
                    imgui.unindent()
                    imgui.dummy(imgui.ImVec2(0, 4))
            imgui.end_child()
            if imgui.button("Close"):
                imgui.close_current_popup()
            imgui.end_popup()

    def _active_project_label(self) -> str:
        return next(
            label for project_id, label, _pair_id in _PROJECTS
            if project_id == self._active_project_id
        )

    def _active_pair_id(self) -> str:
        return next(
            pair_id for project_id, _label, pair_id in _PROJECTS
            if project_id == self._active_project_id
        )

    def _draw_setup_confirm_popup(self) -> None:
        if self._setup_confirm_pending:
            imgui.open_popup(_SETUP_CONFIRM_POPUP)
            self._setup_confirm_pending = False
        opened, _ = imgui.begin_popup_modal(
            _SETUP_CONFIRM_POPUP,
            None,
            imgui.WindowFlags_.always_auto_resize,
        )
        if opened:
            imgui.text_colored(_COL_WARN, "Re-run project setup?")
            imgui.text_wrapped(
                f"This resets B.A.C.U.P.-owned extracted data for "
                f"{self._active_project_label()} and restarts its setup."
            )
            button_size = imgui.ImVec2(120, 0)
            if imgui.button("Continue", button_size):
                imgui.close_current_popup()
                self._rerun_setup()
            imgui.same_line()
            if imgui.button("Cancel", button_size):
                imgui.close_current_popup()
            imgui.end_popup()

    def _rerun_setup(self) -> None:
        try:
            from bacup_ui.setup import (
                clear_project_owned_extractions,
                request_project_setup,
            )
        except ImportError:
            clear_project_owned_extractions = None
            request_project_setup = None

        if clear_project_owned_extractions is not None:
            clear_project_owned_extractions(
                self._toolkit_settings,
                self._active_project_id,
            )
        panel = self._regen_panels.get(self._active_project_id, self._regen_panel)
        if panel is None:
            pair_id = next(
                pair_id for project_id, _label, pair_id in _PROJECTS
                if project_id == self._active_project_id
            )
            from bacup_lib.source_pairs import get_pair

            pair = get_pair(pair_id)
        else:
            pair = panel._pair()
        source_games = [pair.source_game]
        if pair.merge is not None and pair.merge.grafted_game != pair.source_game:
            source_games.append(pair.merge.grafted_game)
        for game_id in source_games:
            self._toolkit_settings.set_game_extracted_dir(game_id, "")
        if request_project_setup is not None:
            request_project_setup(
                self._toolkit_settings,
                self._active_project_id,
            )
        self._toolkit_settings.save()

        from app.paths import is_frozen

        if is_frozen():
            subprocess.Popen([sys.executable])
        else:
            subprocess.Popen([sys.executable] + sys.argv)
        hello_imgui.get_runner_params().app_shall_exit = True

    def cleanup(self) -> None:
        if self._runner and not self._runner.done:
            self._runner.cancel()
        for panel in self._regen_panels.values():
            panel.cleanup()
