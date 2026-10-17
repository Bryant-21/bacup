"""RE Database workspace: search and browse the redb symbol database.

Browsing is read-only, built only on `tools.re.redb.query` through `RedbSession`; the database
directory is a workspace setting (default `<app root>/db/re`). The one write action is the Status
panel's "Game updated" flow (`GameUpdates`): manifest edit plus a background `ingest`.
"""
from __future__ import annotations

import logging
from pathlib import Path

import yaml
from imgui_bundle import imgui

from creation_lib.ui.shell import BaseWorkspace, make_window
from ui.toolkit.app_paths import get_app_root
from tools.re.redb.config import addrlib_bin
from tools.re.redb.game_update import Change
from ui.toolkit.workspaces.re_db_session import SEARCH_KINDS, RedbSession
from ui.toolkit.workspaces.re_db_update import GameUpdates

_NS = "##re_db"
_SEARCH, _FUNCTION, _XREFS, _STATUS = (f"{n}{_NS}" for n in ("Search", "Function", "Xrefs", "Status"))
_UPDATE_POPUP = f"Add new game version{_NS}"
_ERROR = imgui.ImVec4(1, .4, .4, 1)
_log = logging.getLogger(__name__)
_TABLE_FLAGS = (imgui.TableFlags_.borders_inner_v | imgui.TableFlags_.row_bg | imgui.TableFlags_.resizable
                | imgui.TableFlags_.scroll_y | imgui.TableFlags_.scroll_x)


def default_db_dir() -> str:
    return str(get_app_root() / "db" / "re")


def _hex(value) -> str:
    return f"{value:#x}" if isinstance(value, int) else ""


def _copy_button(label: str, text: str, uid: str) -> None:
    imgui.same_line()
    if imgui.small_button(f"{label}##copy_{uid}"):
        imgui.set_clipboard_text(text)


class ReDbWorkspace(BaseWorkspace):
    name = "RE Database"
    icon = "RE"
    id = "re_db"
    user_guide_body = """
Search the reverse-engineering symbol database (built with `modkit re ingest`) by name, string,
finding or pseudocode text. Selecting a result opens the function: its names, the same function in
every binary version, Address Library ids per runtime, findings, pseudocode and callers/callees.
Click a caller or callee to follow it; Back/Forward walk the history. The database is opened
read-only; set its folder under Settings > RE Database.

After a Steam update, Status > Check for game updates lists every exe that changed on disk. Add new
version adds the new build to binaries.yaml under a key derived from its file version, retires the
old key (its data stays searchable) and starts `ingest` for the new key in the background; its
progress shows in the Status panel.
"""

    def __init__(self, toolkit_settings=None):
        super().__init__(toolkit_settings)
        self.session = RedbSession(default_db_dir())
        self._db_dir_edit = str(self.session.db_dir)
        self._overview_loaded = False
        self._query = ""
        self._kind_idx = 0
        self._game = ""
        self._binary = ""
        self._selected_row = -1
        self.updates = GameUpdates(self.session)
        self._update_change: Change | None = None
        self._update_heavy = False
        self._update_steps: list[str] = []
        self._update_error = ""
        self._open_update_popup = False

    def get_dockable_windows(self):
        return [
            make_window(_SEARCH, "LeftDock"),
            make_window(_FUNCTION, "MainDockSpace"),
            make_window(_XREFS, "RightDock"),
            make_window(_STATUS, "BottomDock"),
        ]

    def initialize(self) -> None:
        self._bind_panels({_SEARCH: self._draw_search, _FUNCTION: self._draw_function,
                           _XREFS: self._draw_xrefs, _STATUS: self._draw_status})
        self._initialized = True

    def on_activate(self) -> None:
        super().on_activate()
        if not self._overview_loaded:
            self._overview_loaded = True
            self.session.refresh_overview()

    def draw(self) -> None:
        if self.active and self._initialized:
            self.session.poll()
            self.updates.tick()
            # Here, not in the Status panel: a modal drawn from a docked tab vanishes when that tab is hidden.
            if self._open_update_popup:
                imgui.open_popup(_UPDATE_POPUP)
                self._open_update_popup = False
            self._draw_update_dialog()

    def draw_menu(self) -> None:
        if self._view_helper:
            self._view_helper.draw([_SEARCH, _FUNCTION, _XREFS, _STATUS])

    def cleanup(self) -> None:
        self.session.close()

    # -- settings ------------------------------------------------------------

    def get_settings_defaults(self) -> dict:
        return {"db_dir": default_db_dir()}

    def apply_settings(self, settings: dict) -> None:
        db_dir = str(settings.get("db_dir") or default_db_dir())
        self._db_dir_edit = db_dir
        if Path(db_dir) != self.session.db_dir:
            self.session.db_dir = Path(db_dir)
            if self._overview_loaded:
                self.session.set_db_dir(db_dir)

    def collect_settings(self) -> dict:
        return {"db_dir": str(self.session.db_dir)}

    def draw_settings(self) -> None:
        imgui.text_disabled("Database folder (holds re.sqlite; opened read-only)")
        imgui.set_next_item_width(-80)
        _, self._db_dir_edit = imgui.input_text(f"##db_dir{_NS}", self._db_dir_edit)
        imgui.same_line()
        if imgui.button(f"Apply{_NS}"):
            self.change_db_dir(self._db_dir_edit)
        if Path(self._db_dir_edit) != self.session.db_dir:
            imgui.text_disabled("Not applied yet.")

    def change_db_dir(self, db_dir: str) -> None:
        """Switch databases and store the folder now: closing Settings re-applies the stored value."""
        self._overview_loaded = True
        self.session.set_db_dir(db_dir)
        if self._toolkit_settings is not None:
            self._toolkit_settings.set_workspace_settings(self.id, {"db_dir": str(self.session.db_dir)})
            self._toolkit_settings.save()

    # -- search --------------------------------------------------------------

    def run_search(self) -> None:
        self._selected_row = -1
        self.session.search(self._query, SEARCH_KINDS[self._kind_idx], self._game, self._binary)

    def _combo(self, label: str, current: str, options: list[str], all_label: str) -> str:
        items = [all_label, *options]
        idx = items.index(current) if current in items else 0
        imgui.set_next_item_width(-1)
        _, idx = imgui.combo(label, idx, items)
        return "" if idx == 0 else items[idx]

    def _draw_search(self) -> None:
        if not imgui.begin(_SEARCH):
            imgui.end()
            return
        s = self.session
        imgui.set_next_item_width(-1)
        entered, self._query = imgui.input_text_with_hint(
            f"##query{_NS}", "Search names, strings, findings, pseudocode", self._query,
            imgui.InputTextFlags_.enter_returns_true)
        imgui.set_next_item_width(-1)
        _, self._kind_idx = imgui.combo(f"##kind{_NS}", self._kind_idx, [k.capitalize() for k in SEARCH_KINDS])
        self._game = self._combo(f"##game{_NS}", self._game, s.games(), "All games")
        self._binary = self._combo(f"##binary{_NS}", self._binary, s.binary_keys(self._game), "All binaries")
        if imgui.button(f"Search{_NS}", imgui.ImVec2(-1, 0)) or entered:
            self.run_search()
        if s.busy("search"):
            imgui.text_disabled("Searching...")
        elif s.search_error or s.binaries_error:
            imgui.text_colored(imgui.ImVec4(1, .4, .4, 1), s.search_error or s.binaries_error)
        else:
            imgui.text_disabled(f"{len(s.results)} results")
        if s.search_notice:
            imgui.text_colored(imgui.ImVec4(1, .8, .3, 1), s.search_notice)
        self._draw_results_table()
        imgui.end()

    def _draw_results_table(self) -> None:
        rows = self.session.results
        if not imgui.begin_table(f"results{_NS}", 5, _TABLE_FLAGS):
            return
        imgui.table_setup_scroll_freeze(0, 1)
        for col, sample in (("Name / text", "x" * 32), ("Binary", "fo4-game-1.11.240"), ("RVA", "0x0000000"),
                            ("Rank / source", "100 official_pdb"), ("Kind", "finding")):
            imgui.table_setup_column(col, imgui.TableColumnFlags_.width_fixed, imgui.calc_text_size(sample).x)
        imgui.table_headers_row()
        for i, row in enumerate(rows):
            label = (row.get("name") or row.get("text")) if row["kind"] == "decomp" else row.get("text")
            imgui.table_next_row()
            imgui.table_next_column()
            clicked, _ = imgui.selectable(f"{label or ''}##row{i}", self._selected_row == i,
                                          imgui.SelectableFlags_.span_all_columns)
            if clicked:
                self.select_result(i)
            imgui.table_next_column()
            imgui.text(row.get("binary") or "")
            imgui.table_next_column()
            imgui.text(_hex(row.get("rva", row.get("func_rva"))))
            imgui.table_next_column()
            imgui.text(f"{row['rank']} {row['source']}" if row.get("source") else "")
            imgui.table_next_column()
            imgui.text(row["kind"])
        imgui.end_table()

    def select_result(self, index: int) -> None:
        self._selected_row = index
        ref = RedbSession.result_ref(self.session.results[index])
        if ref:
            self.session.open(ref)

    # -- function ------------------------------------------------------------

    def _draw_function(self) -> None:
        if not imgui.begin(_FUNCTION):
            imgui.end()
            return
        s = self.session
        imgui.begin_disabled(not s.can_back())
        if imgui.button(f"< Back{_NS}"):
            s.back()
        imgui.end_disabled()
        imgui.same_line()
        imgui.begin_disabled(not s.can_forward())
        if imgui.button(f"Forward >{_NS}"):
            s.forward()
        imgui.end_disabled()
        view = s.view
        if view is not None:
            imgui.same_line()
            imgui.text_disabled(view.ref)
        imgui.separator()
        if view is None:
            imgui.text_disabled("Select a search result to open a function.")
        elif s.busy("function"):
            imgui.text_disabled("Loading...")
        elif view.error:
            imgui.text_colored(imgui.ImVec4(1, .4, .4, 1), view.error)
        elif imgui.begin_tab_bar(f"tabs{_NS}"):
            if imgui.begin_tab_item(f"Details{_NS}")[0]:
                self._draw_details(view.detail)
                imgui.end_tab_item()
            if imgui.begin_tab_item(f"Pseudocode{_NS}")[0]:
                imgui.input_text_multiline(f"##pseudo{_NS}", view.pseudocode, imgui.ImVec2(-1, -1),
                                           imgui.InputTextFlags_.read_only)
                imgui.end_tab_item()
            imgui.end_tab_bar()
        imgui.end()

    def _draw_details(self, d: dict) -> None:
        if not imgui.begin_child(f"details{_NS}"):
            imgui.end_child()
            return
        imgui.text(d["primary_name"] or "(unnamed)")
        _copy_button("Copy name", d["primary_name"] or "", "primary")
        imgui.text_disabled(f"rank {d['rank']}  class {d['class_id']}  resolved {d['resolved']}")
        if d.get("decorated"):
            imgui.text_disabled(d["decorated"])

        if imgui.collapsing_header(f"Versions ({len(d['members'])}){_NS}", imgui.TreeNodeFlags_.default_open):
            for i, m in enumerate(d["members"]):
                imgui.text(f"{m['binary']}  {_hex(m['rva'])}  size {m['size']}")
                _copy_button("Copy RVA", _hex(m["rva"]), f"m{i}")
                if m["addrlib_ids"]:
                    imgui.same_line()
                    imgui.text_disabled("AL " + ", ".join(map(str, m["addrlib_ids"])))
                imgui.indent()
                for j, n in enumerate(m["names"]):
                    imgui.text(f"{n['rank']:>3}  {n['source']:<13} {n['name']}")
                    _copy_button("Copy", n["name"], f"m{i}n{j}")
                for match in m.get("matches", []):
                    score = f" {match['score']:.2f}" if match["score"] is not None else ""
                    imgui.text_disabled(f"matched {match['tier']}{score} with {match['with']}")
                imgui.unindent()

        if imgui.collapsing_header(f"Address Library ({len(d['addrlib'])}){_NS}", imgui.TreeNodeFlags_.default_open):
            if not d["addrlib"]:
                imgui.text_disabled("No Address Library id.")
            for i, e in enumerate(d["addrlib"]):
                imgui.text(f"{e['space']} id {e['id']}{'  (paired)' if e['paired'] else ''}")
                _copy_button("Copy id", str(e["id"]), f"al{i}")
                imgui.indent()
                for runtime, rva in e["rvas"].items():
                    imgui.text(f"{runtime}  {_hex(rva)}")
                for name in e["names"]:
                    imgui.text_disabled(name)
                imgui.unindent()

        if imgui.collapsing_header(f"Findings ({len(d['findings'])}){_NS}", imgui.TreeNodeFlags_.default_open):
            if not d["findings"]:
                imgui.text_disabled("No findings.")
            for f in d["findings"]:
                imgui.text(f.get("title") or "")
                if f.get("body"):
                    imgui.text_wrapped(f["body"])
                if f.get("doc_path"):
                    imgui.text_disabled(f["doc_path"])

        if d["cross_game"] and imgui.collapsing_header(f"Cross-game ({len(d['cross_game'])}){_NS}"):
            for c in d["cross_game"]:
                imgui.text(f"{c['game']}  {c['primary_name'] or ''}  {c['method']} {c['score']}")

        if d["alternatives"] and imgui.collapsing_header(f"Other matches ({len(d['alternatives'])}){_NS}"):
            for i, alt in enumerate(d["alternatives"]):
                for ref in alt["functions"]:
                    if imgui.small_button(f"{alt['primary_name'] or ''} {ref}##alt{i}{ref}"):
                        self.session.open(ref)
        imgui.end_child()

    # -- xrefs / status ------------------------------------------------------

    def _draw_xrefs(self) -> None:
        if not imgui.begin(_XREFS):
            imgui.end()
            return
        view = self.session.view
        if view is None:
            imgui.text_disabled("No function open.")
        elif self.session.busy("function"):
            imgui.text_disabled("Loading...")
        elif not view.xrefs:
            imgui.text_disabled(view.error or "No xrefs.")
        for e, entry in enumerate(view.xrefs if view else []):
            for direction in ("callers", "callees"):
                rows = entry.get(direction, [])
                if not imgui.collapsing_header(f"{direction.capitalize()} ({len(rows)})##{e}{direction}{_NS}",
                                               imgui.TreeNodeFlags_.default_open):
                    continue
                for i, row in enumerate(rows):
                    target = RedbSession.xref_ref(entry["binary"], row, direction)
                    label = f"{_hex(row['rva'])}  {row['kind']:<5} {row.get('name') or '?'}##{e}{direction}{i}"
                    if target is None:
                        imgui.text_disabled(label.split("##")[0])
                    elif imgui.selectable(label, False)[0]:
                        self.session.open(target)
        imgui.end()

    def _draw_status(self) -> None:
        if not imgui.begin(_STATUS):
            imgui.end()
            return
        s = self.session
        if imgui.button(f"Refresh{_NS}"):
            s.refresh_overview()
        imgui.same_line()
        imgui.begin_disabled(self.updates.checking)
        if imgui.button(f"Check for game updates{_NS}"):
            self.updates.check()
        imgui.end_disabled()
        imgui.text_disabled(str(s.index_path))
        if s.status_error:
            imgui.text_colored(_ERROR, s.status_error)
        self._draw_update_check()
        self._draw_ingest_progress()
        rows = s.status_rows
        if rows:
            stages = [k for k in rows[0] if k != "key"]
            if imgui.begin_table(f"status{_NS}", 1 + len(stages), _TABLE_FLAGS):
                imgui.table_setup_scroll_freeze(1, 1)
                imgui.table_setup_column("binary")
                for stage in stages:
                    imgui.table_setup_column(stage)
                imgui.table_headers_row()
                for row in rows:
                    imgui.table_next_row()
                    imgui.table_next_column()
                    imgui.text(row["key"])
                    for stage in stages:
                        imgui.table_next_column()
                        state = row[stage]
                        if state == "done":
                            imgui.text(state)
                        else:
                            imgui.text_disabled(state)
                imgui.end_table()
        imgui.end()

    # -- game updated --------------------------------------------------------

    def _draw_update_check(self) -> None:
        u = self.updates
        if u.checking:
            imgui.text_disabled("Checking game exes...")
            return
        if u.check_error:
            imgui.text_colored(_ERROR, u.check_error)
            return
        r = u.result
        if r is None:
            return
        summary = f"Game updates: {len(r.changes)} changed, {len(r.unchanged)} unchanged"
        if r.unregistered:
            summary += f", {len(r.unregistered)} not ingested yet"
        imgui.push_style_color(imgui.Col_.text, imgui.get_style_color_vec4(imgui.Col_.text_disabled))
        imgui.text_wrapped(summary)
        imgui.pop_style_color()
        if r.missing:
            imgui.text_colored(_ERROR, "Exe missing: " + ", ".join(r.missing))
        flags = (imgui.TableFlags_.borders_inner_v | imgui.TableFlags_.row_bg | imgui.TableFlags_.resizable
                 | imgui.TableFlags_.scroll_x | imgui.TableFlags_.sizing_fixed_fit)
        # A bounded height: the Status matrix below takes the rest of the panel.
        height = imgui.get_frame_height_with_spacing() * (2 * len(r.changes) + 1.5)
        if not r.changes or not imgui.begin_table(f"updates{_NS}", 6, flags, imgui.ImVec2(0, height)):
            return
        for col in ("Old key", "Version", "Proposed key", "Address Library", "", "Exe"):
            imgui.table_setup_column(col)
        imgui.table_headers_row()
        for i, c in enumerate(r.changes):
            imgui.table_next_row()
            imgui.table_next_column()
            imgui.text(c.key)
            imgui.table_next_column()
            imgui.text(f"{c.stored_version} -> {c.new_version or '?'}")
            imgui.table_next_column()
            imgui.text(c.proposal["key"])
            if c.present:
                imgui.text_disabled("already in the manifest")
            imgui.table_next_column()
            if c.addrlib_runtime:
                imgui.text(f"yes {c.addrlib_runtime}")
            else:
                imgui.text_disabled(f"no ({c.expected_runtime})" if c.expected_runtime else "no")
            imgui.table_next_column()
            if imgui.small_button(f"Add new version##upd{i}"):
                self.start_update(c)
            imgui.table_next_column()
            imgui.text(str(c.path))
        imgui.end_table()

    def _draw_ingest_progress(self) -> None:
        u = self.updates
        keys = list(dict.fromkeys([*u.running_keys(), *u.runs]))
        for key in keys:
            p = u.progress(key)
            run = u.runs.get(key)
            if p.running:
                imgui.text(f"Ingest {key}: {p.stage or 'starting'} ({p.done}/{p.total} stages done)")
            else:
                code = run.returncode if run else None
                text = f"Ingest {key} finished" + (f" (exit {code})" if code is not None else "")
                if code:
                    imgui.text_colored(_ERROR, text + "; see the log below")
                else:
                    imgui.text(text)
            if p.log_tail:
                imgui.input_text_multiline(f"##tail{key}{_NS}", p.log_tail,
                                           imgui.ImVec2(-1, imgui.get_text_line_height() * 4),
                                           imgui.InputTextFlags_.read_only)

    def start_update(self, change: Change) -> None:
        self._update_change = change
        self._update_heavy = False
        self._update_steps, self._update_error = [], ""
        self._open_update_popup = True

    def confirm_update(self) -> None:
        try:
            self._update_steps = self.updates.add(self._update_change, self._update_heavy)
            self._update_error = ""
        except (ValueError, OSError, yaml.YAMLError) as exc:  # e.g. binaries.yaml read-only, locked or broken
            self._update_error = str(exc)
            return
        for step in self._update_steps:
            _log.info("RE Database: %s", step)

    def _draw_update_dialog(self) -> None:
        imgui.set_next_window_size(imgui.ImVec2(760, 0), imgui.Cond_.appearing)
        opened, _ = imgui.begin_popup_modal(_UPDATE_POPUP, None)
        if not opened:
            return
        c = self._update_change
        if c is None:
            imgui.close_current_popup()
            imgui.end_popup()
            return
        imgui.text(f"{c.key}: the exe changed on disk")
        imgui.text_disabled(f"version {c.stored_version} -> {c.new_version or 'unknown'}")
        imgui.text_wrapped(str(c.path))
        imgui.separator()
        if self._update_steps:
            for step in self._update_steps:
                imgui.bullet()
                imgui.text_wrapped(step)
            imgui.text_disabled("Progress shows in the Status panel; you can close this.")
            if imgui.button(f"Close{_NS}"):
                self._update_change = None
                imgui.close_current_popup()
            imgui.end_popup()
            return
        self._draw_proposal(c)
        _, self._update_heavy = imgui.checkbox(f"Also run IDA/Ghidra analysis (several hours){_NS}",
                                               self._update_heavy)
        if self._update_error:
            imgui.text_colored(_ERROR, self._update_error)
        if imgui.button(f"Add{_NS}"):
            self.confirm_update()
        imgui.same_line()
        if imgui.button(f"Cancel{_NS}"):
            self._update_change = None
            imgui.close_current_popup()
        imgui.end_popup()

    def _draw_proposal(self, c: Change) -> None:
        p = c.proposal
        imgui.text("New manifest entry")
        indent = imgui.get_cursor_pos_x() + imgui.calc_text_size("addrlib_runtime  ").x
        for label, value in (("key", p["key"]), ("game", p["game"]), ("product", p["product"]), ("version", p["version"]),
                             ("variant", p["variant"]), ("path", p["path"]), ("pdb", p.get("pdb") or "none"),
                             ("match_with", ", ".join(p["match_with"]) or "none"),
                             ("addrlib_runtime", p["addrlib_runtime"] or "empty")):
            imgui.text_disabled(label)
            imgui.same_line(indent)
            imgui.text_wrapped(str(value))
        if not p["addrlib_runtime"] and c.expected_runtime:
            imgui.text_disabled(f"addrlib_runtime: set when Address Library ships {addrlib_bin(c.expected_runtime).name}")
        if p["variant"] == "vr":
            imgui.text_disabled("addrlib_space: empty (no CommonLib names, no id matches); declare og or ng in "
                                "binaries.yaml once the CSV's id space is confirmed (redb.md, Fallout 4 VR)")
        if c.present:
            imgui.text_disabled("This build is already in binaries.yaml: Add only retires the old key and ingests.")
        elif c.suffixed:
            imgui.text_disabled(f"{c.derived_key} is already used by another exe, so the key gets a -r suffix.")
        if c.stale:
            imgui.text_disabled(f"Also retires {', '.join(c.stale)}: an older build's row for this exe that "
                                f"was never ingested.")
        imgui.separator()
        imgui.text_wrapped(f"Add also retires {c.key}: its manifest row is removed, its database rows stay "
                           f"searchable and matchable. Then `ingest {p['key']}` starts in the background.")
