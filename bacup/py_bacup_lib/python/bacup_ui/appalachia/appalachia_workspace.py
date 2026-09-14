"""B.A.C.U.P. workspace for the supported Bethesda conversion projects."""
from __future__ import annotations

import logging
import subprocess
import sys

from imgui_bundle import hello_imgui, imgui

from creation_lib.ui.shell import BaseWorkspace, make_window
from bacup_ui.appalachia.window_title import appalachia_window_title
from bacup_ui.setup import PROJECT_PROFILES, get_active_project, set_active_project
from creation_lib.ui.widgets.modern import (
    InteractionState, heading, navigation_item, prepare_dialog, scaled, semantic_color, toggle,
)

_log = logging.getLogger("toolkit.appalachia")
_NS = "##appalachia"
_CHANGELOG_POPUP = f"Changelog{_NS}"
_SETUP_CONFIRM_POPUP = f"Re-run Setup{_NS}"

APP_NAME = "B.A.C.U.P."
APP_EXPANSION = "Bethesda Asset Converter Universal Platform"

_PROJECTS = tuple((p.id, p.title, p.conversion_id) for p in PROJECT_PROFILES.values())

_ENABLED_PROJECTS = _PROJECTS


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
        self._active_project_id = (
            get_active_project(toolkit_settings) if toolkit_settings is not None
            else "appalachia"
        )
        self._changelog_pending = False
        self._setup_confirm_pending = False
        self._navigation_state = InteractionState()
        self._music_controls_height = 0.0
        workspace_settings = (
            toolkit_settings.get_workspace_settings(self.id)
            if toolkit_settings is not None else {}
        )
        self.show_logs = bool(workspace_settings.get("show_logs", False))

    def get_dockable_windows(self):
        return [
            make_window(
                f"{APP_NAME}{_NS}",
                "MainDockSpace",
                is_visible=True,
                can_be_closed=False,
                remember_is_visible=False,
            )
        ]

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

        self._regen_panel = self._regen_panels[self._active_project_id]
        self._log_panel = self._log_panels[self._active_project_id]
        self._bind_panels({f"{APP_NAME}{_NS}": self._draw_projects})
        self._initialized = True
        _log.info("B.A.C.U.P. workspace initialized")

    def _draw_projects(self) -> None:
        if not imgui.begin(f"{APP_NAME}{_NS}"):
            imgui.end()
            return
        flags = imgui.TableFlags_.sizing_fixed_fit
        if imgui.begin_table(f"{_NS}_project_layout", 2, flags):
            imgui.table_setup_column("Projects", imgui.TableColumnFlags_.width_fixed, scaled(230))
            imgui.table_setup_column("Project", imgui.TableColumnFlags_.width_stretch)
            imgui.table_next_row()
            imgui.table_set_column_index(0)
            if imgui.begin_child(f"{_NS}_sidebar", imgui.ImVec2(0, 0), imgui.ChildFlags_.always_use_window_padding):
                self._draw_sidebar()
            imgui.end_child()
            imgui.table_set_column_index(1)
            imgui.push_style_color(imgui.Col_.child_bg, semantic_color("background"))
            visible = imgui.begin_child(
                f"{_NS}_project_content", imgui.ImVec2(0, 0),
                imgui.ChildFlags_.always_use_window_padding,
            )
            imgui.pop_style_color()
            if visible:
                imgui.push_id(self._active_project_id)
                self._regen_panel.draw_project()
                imgui.pop_id()
            imgui.end_child()
            imgui.end_table()
        imgui.end()

    def _select_project(self, project_id: str) -> None:
        if project_id not in self._regen_panels:
            raise KeyError(project_id)
        if self._active_project_id != project_id:
            self._active_project_id = project_id
            set_active_project(self._toolkit_settings, project_id)
        self._regen_panel = self._regen_panels[project_id]
        self._log_panel = self._log_panels[project_id]

    def _draw_sidebar(self) -> None:
        from imgui_bundle import icons_fontawesome_6 as fa

        icons = {"appalachia": fa.ICON_FA_TREE, "wasteland": fa.ICON_FA_SUN,
                 "north": fa.ICON_FA_MOUNTAIN, "stars": fa.ICON_FA_ROCKET}
        heading(APP_NAME)
        imgui.text_disabled("CONVERSION PROJECTS")
        imgui.spacing()
        footer_height = max(scaled(38), self._music_controls_height)
        if imgui.begin_child(f"{_NS}_project_list", imgui.ImVec2(
            0, -footer_height - imgui.get_style().item_spacing.y,
        )):
            self._draw_project_navigation(icons)
        imgui.end_child()

        imgui.begin_group()
        imgui.separator()
        imgui.push_id(self._active_project_id)
        self._regen_panel._draw_music_controls()
        imgui.pop_id()
        imgui.end_group()
        self._music_controls_height = imgui.get_item_rect_size().y

    def _draw_project_navigation(self, icons: dict[str, str]) -> None:
        from imgui_bundle import icons_fontawesome_6 as fa

        for project_id, label, _pair_id in _ENABLED_PROJECTS:
            panel = self._regen_panels[project_id]
            running = self._runner_owner is panel and self._runner is not None and not self._runner.done
            if navigation_item(project_id, label, selected=project_id == self._active_project_id,
                               icon=icons.get(project_id, fa.ICON_FA_FOLDER), running=running,
                               detail="Converting…" if running else "",
                               state=self._navigation_state):
                self._select_project(project_id)

        imgui.spacing()
        imgui.separator()
        changed, show_logs = toggle(f"Show logs{_NS}", self.show_logs)
        if changed:
            self.set_show_logs(show_logs)

    def set_show_logs(self, visible: bool) -> None:
        self.show_logs = visible
        if self._toolkit_settings is not None:
            self._toolkit_settings.set_workspace_settings(self.id, {"show_logs": visible})

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
        prepare_dialog(640, 520)
        opened, _ = imgui.begin_popup_modal(_CHANGELOG_POPUP)
        if opened:
            heading(f"Changelog — {self._active_project_label()}")
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
                    imgui.text_colored(semantic_color("accent"), version_id)
                    if is_current:
                        imgui.same_line()
                        imgui.text_colored(semantic_color("success"), "(current)")
                    imgui.indent()
                    for note in notes:
                        imgui.push_text_wrap_pos(0)
                        imgui.bullet_text(note)
                        imgui.pop_text_wrap_pos()
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
        prepare_dialog(550, 240)
        opened, _ = imgui.begin_popup_modal(
            _SETUP_CONFIRM_POPUP,
            None,
            imgui.WindowFlags_.none,
        )
        if opened:
            imgui.text_colored(semantic_color("warning"), "Re-run project setup?")
            imgui.text_wrapped(
                f"This resets B.A.C.U.P.-owned extracted data for "
                f"{self._active_project_label()} and restarts its setup."
            )
            button_size = imgui.ImVec2(scaled(120), 0)
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
