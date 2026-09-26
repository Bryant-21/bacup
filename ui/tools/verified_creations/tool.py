"""Verified Creations — writes Reddit/Discord/Wiki post drafts for newly published Creations."""

from __future__ import annotations

import calendar
import os
from datetime import date, datetime, timedelta, timezone
from pathlib import Path

from imgui_bundle import imgui

from creation_lib.ui.widgets import pick_folder
from creation_lib.ui.widgets.forms import (
    begin_form,
    draw_combo_field,
    draw_path_row,
    end_form,
    form_row_label,
)
from ui.toolkit.app_paths import get_exe_dir
from ui.tools.base import BaseTool

from .generator import TEMPLATES, generate_posts

_GAMES = ["FALLOUT4", "SKYRIM", "STARFIELD"]
_KINDS = list(TEMPLATES)
_WEEKDAYS = ["Su", "Mo", "Tu", "We", "Th", "Fr", "Sa"]
_CALENDAR = calendar.Calendar(firstweekday=calendar.SUNDAY)


class VerifiedCreationsTool(BaseTool):
    name = "Verified Creations"
    tool_id = "verified_creations"
    description = (
        "Checks the Bethesda Creations listing for verified, paid Creations published after the "
        "cutoff date and writes post drafts (Reddit and Wiki with images, Discord as a single "
        "markdown file) into a subfolder per template.\n\n"
        "The API works in UTC: the cutoff is midnight UTC, so pick a day earlier if your "
        "timezone would otherwise miss recent releases."
    )
    category = "Mod Tools"

    def __init__(self):
        super().__init__()
        self._cutoff = date.today()
        self._view_month = self._cutoff.replace(day=1)
        self._game_idx = 0
        self._kinds = {kind: True for kind in _KINDS}
        self._output_dir = ""

    def get_default_settings(self) -> dict:
        return {
            "cutoff_date": date.today().isoformat(),
            "game": _GAMES[0],
            "kinds": _KINDS,
            "output_dir": "",
        }

    def apply_settings(self, settings: dict) -> None:
        try:
            self._cutoff = min(date.fromisoformat(settings["cutoff_date"]), date.today())
            self._view_month = self._cutoff.replace(day=1)
        except (KeyError, TypeError, ValueError):
            pass
        if settings.get("game") in _GAMES:
            self._game_idx = _GAMES.index(settings["game"])
        if isinstance(settings.get("kinds"), list):
            self._kinds = {kind: kind in settings["kinds"] for kind in _KINDS}
        self._output_dir = settings.get("output_dir", self._output_dir)

    def collect_settings(self) -> dict:
        return {
            "cutoff_date": self._cutoff.isoformat(),
            "game": _GAMES[self._game_idx],
            "kinds": [kind for kind, on in self._kinds.items() if on],
            "output_dir": self._output_dir,
        }

    def draw_content(self) -> None:
        if begin_form("##verified_creations"):
            form_row_label("Cutoff date (UTC)")
            self._draw_date_picker()
            _, self._game_idx = draw_combo_field("Game", _GAMES, self._game_idx)

            form_row_label("Templates")
            for i, kind in enumerate(_KINDS):
                if i:
                    imgui.same_line()
                _, self._kinds[kind] = imgui.checkbox(kind.capitalize(), self._kinds[kind])

            _, clicked = draw_path_row("Output folder", str(self._resolved_output_dir()))
            if clicked:
                path = pick_folder("Select output folder")
                if path:
                    self._output_dir = path
            end_form()

        imgui.push_text_wrap_pos(0.0)
        imgui.text_disabled("Creations published after midnight UTC on the cutoff date are included.")
        imgui.pop_text_wrap_pos()
        imgui.spacing()
        imgui.separator()
        imgui.spacing()

        if not self._running and imgui.button("Generate", imgui.ImVec2(160, 0)):
            self._validate_and_run()

    def _draw_date_picker(self) -> None:
        if imgui.button(f"{self._cutoff.isoformat()}##cutoff"):
            self._view_month = self._cutoff.replace(day=1)
            imgui.open_popup("##cutoff_calendar")
        if not imgui.begin_popup("##cutoff_calendar"):
            return

        today = date.today()
        view = self._view_month
        if imgui.arrow_button("##prev_month", imgui.Dir.left):
            self._view_month = (view.replace(day=1) - timedelta(days=1)).replace(day=1)
        imgui.same_line()
        imgui.text(view.strftime("%B %Y"))
        imgui.same_line()
        imgui.begin_disabled(view.replace(day=28) + timedelta(days=4) > today)
        if imgui.arrow_button("##next_month", imgui.Dir.right):
            self._view_month = (view.replace(day=28) + timedelta(days=4)).replace(day=1)
        imgui.end_disabled()

        if imgui.begin_table("##cutoff_days", 7, imgui.TableFlags_.sizing_fixed_fit):
            day_width = imgui.calc_text_size("Mo ").x
            for name in _WEEKDAYS:
                imgui.table_setup_column(name, imgui.TableColumnFlags_.width_fixed, day_width)
            for name in _WEEKDAYS:
                imgui.table_next_column()
                imgui.text_disabled(name)
            for week in _CALENDAR.monthdatescalendar(view.year, view.month):
                for day in week:
                    imgui.table_next_column()
                    if day.month != view.month:
                        continue
                    imgui.begin_disabled(day > today)
                    if imgui.selectable(f"{day.day:>2}##{day}", day == self._cutoff)[0]:
                        self._cutoff = day
                    imgui.end_disabled()
            imgui.end_table()

        if imgui.button("Today##cutoff"):
            self._cutoff = today
            imgui.close_current_popup()
        imgui.end_popup()

    def _validate_and_run(self) -> None:
        cutoff = datetime.combine(self._cutoff, datetime.min.time(), timezone.utc)
        kinds = [kind for kind, on in self._kinds.items() if on]
        if not kinds:
            self._error_msg = "Select at least one template."
            return
        if self._output_dir and not os.path.isdir(self._output_dir):
            self._error_msg = f"Output folder not found: {self._output_dir}"
            return
        self._start_batch(self._run, cutoff, kinds, self._resolved_output_dir())

    def _resolved_output_dir(self) -> Path:
        return Path(self._output_dir) if self._output_dir else get_exe_dir() / "output" / "verified_creations"

    def _run(self, cutoff: datetime, kinds: list[str], output_dir: Path) -> None:
        product = _GAMES[self._game_idx]
        counts = []
        # Reddit runs before Wiki so the Wiki pass reuses its downloaded images.
        for step, kind in enumerate(sorted(kinds, key=_KINDS.index)):
            count = generate_posts(
                product,
                kind,
                cutoff,
                output_dir,
                cancelled=lambda: self._cancel_requested,
                on_page=lambda page, written, k=kind, s=step: self._on_progress(
                    s, len(kinds), f"{k}: page {page}, {written} posts"
                ),
            )
            counts.append(f"{kind}: {count}")
            self._on_progress(step + 1, len(kinds), f"{kind}: {count} posts")
            if self._cancel_requested:
                break
        self._result_msg = f"Posts written to {output_dir} ({', '.join(counts)})"
