"""ESP Editor application — the imgui app behind EspEditorWorkspace.

Layout:
  Left   — navigation tree (plugins / groups / records) + filter bar
  Center — record editor (schema-driven field widgets)
  Right  — info tabs (Info / Conflicts / ReferencedBy)
"""

from __future__ import annotations

import csv
import json
import logging
import re
import shutil
from collections import OrderedDict
from copy import deepcopy
from concurrent.futures import Future, ThreadPoolExecutor
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path

from imgui_bundle import imgui
from creation_lib.ui.widgets.modern import loading_panel

from creation_lib.ui.widgets.modern import expandable_section

from creation_lib.esp.editor import (
    ConflictReport,
    ConflictScanner,
    ConflictScan,
    ConflictStatus,
    EditorSession,
    Field,
    Severity,
    UiFieldKind,
    ValidationReport,
    add_winners_to_patch,
    automerge_to_patch,
    copy_as_new,
    copy_as_override,
    create_patch_plugin,
    validate,
)
from creation_lib.esp.editor.session import detect_game
from creation_lib.esp.native_runtime import (
    plugin_handle_call,
    plugin_handle_get,
    plugin_handle_group_record_summaries,
    plugin_handle_record_summary,
    plugin_handle_search_records,
    plugin_handle_record_counts,
    plugin_handle_set_record_subrecords,
)
from creation_lib.esp.record_types import record_type_display_label
from creation_lib.esp.schema import get_schema
from creation_lib.ui.widgets import pick_folder
from creation_lib.ui.widgets.forms import pick_file, pick_save_file
from creation_lib.ui.widgets.modern import heading, semantic_color, prepare_dialog
from ui.esp_editor.record_view import (
    comparison_rows, edit_reason, encode_edit, load_comparison, winning_document,
)
from ui.esp_editor.navigation import (
    SEARCH_FIELDS, RECORD_SORTS, RecordIdentity, matches, sort_records, session_slots, record_identity,
)

_log = logging.getLogger("ui.esp_editor")

# Lazily resolved — may be absent from the native module.
_plugin_handle_group_signatures = None
_plugin_handle_group_record_summaries = None


def _get_group_api():
    """Return (group_signatures_fn, group_summaries_fn) or (None, None)."""
    global _plugin_handle_group_signatures, _plugin_handle_group_record_summaries
    if _plugin_handle_group_signatures is not None:
        return _plugin_handle_group_signatures, _plugin_handle_group_record_summaries
    try:
        from creation_lib.esp import native_runtime as _nr

        _plugin_handle_group_signatures = getattr(_nr, "plugin_handle_group_signatures", None)
        _plugin_handle_group_record_summaries = getattr(
            _nr,
            "plugin_handle_group_record_summaries",
            None,
        )
    except Exception:
        pass
    return _plugin_handle_group_signatures, _plugin_handle_group_record_summaries


@dataclass
class _Selection:
    plugin_handle: int | None = None
    record: object = None  # creation_lib.esp.model.Record
    fields: list[Field] | None = None
    document: object = None
    documents: list = field(default_factory=list)
    comparison: list = field(default_factory=list)
    differences: list = field(default_factory=list)
    error: str | None = None


@dataclass
class _SaveEntry:
    handle: int
    plugin_name: str
    path: str
    selected: bool = True


def _safe_export_stem(text: str, fallback: str) -> str:
    cleaned = re.sub(r"[^A-Za-z0-9._-]+", "_", text).strip("._-")
    return cleaned or fallback


class EspEditorApp:
    def __init__(self, *, toolkit_settings=None):
        self.active = False
        self._toolkit_settings = toolkit_settings
        default_game = "fo4"
        if toolkit_settings is not None:
            try:
                default_game = toolkit_settings.get_active_game() or "fo4"
            except Exception:
                pass
        self.session = EditorSession(
            toolkit_settings=toolkit_settings,
            default_game=default_game,
            auto_scan_conflicts=False,
        )
        self.selection = _Selection()
        self._filter_text = ""
        self._filter_signature = ""
        self._search_field = "All indexed fields"
        self._record_sort = "FormID"
        self._sort_descending = False
        self._plugin_masters: dict[int, tuple[str, ...]] = {}
        self._plugin_flags: dict[int, int] = {}
        self._show_overview = False
        self._overview_query = ""
        self._overview_scope_active = False
        self._overview_status = "All overrides"
        self._overview_cache = None
        self._overview_future = None
        self._overview_error = None
        self._overview_totals = None
        self._group_errors: dict[tuple[int, str], str] = {}
        self._known_group_types: dict[tuple[int, str], set[str]] = {}
        self._filter_errors: dict = {}
        self._slots_cache = None
        self._analysis_future = None
        self._analysis_queue = []
        self._analysis_completed = 0
        self._analysis_total = 0
        self._analysis_stamp = None
        self._analysis_label = ""
        self._analysis_cancelled = False
        self._analyzed_types: set[str] = set()
        self._scan_complete = False
        self._undo: list[tuple[int, int, list]] = []  # (handle, form_id, prev_subrecords)
        self._redo: list[tuple[int, int, list]] = []
        self._dirty_handles: set[int] = set()
        # Per-plugin set of modified record FormIDs — colored blue in nav tree,
        # cleared on save.
        self._dirty_records: dict[int, set[int]] = {}
        # Cache: handle → list of top-level group (label, count) tuples.
        self._cached_groups: dict[int, list[tuple[str, int]]] = {}
        # Cache: (handle, signature) → list of records in that group.
        self._cached_group_records: dict[tuple[int, str], list] = {}
        # Fallback cache for old root-items path.
        self._cached_root: dict[int, list] = {}
        self._executor = ThreadPoolExecutor(max_workers=2, thread_name_prefix="esp_editor")
        # Track which game schemas we've kicked off a background preload for.
        self._schema_preload_started: set[str] = set()
        self._busy: bool = False
        self._busy_message: str = ""
        self._busy_future: Future | None = None
        self._busy_done_cb = None  # callable(result_or_exception, is_error)
        # Per-plugin async tree materialization (separate from the modal busy lane).
        self._tree_futures: dict[int, Future] = {}
        self._header_cache: dict[int, object] = {}
        self._header_futures: dict[int, Future] = {}
        # Per-group async record fetches: (handle, signature) → Future.
        self._group_futures: dict[tuple[int, str], Future] = {}
        self._filtered_records: dict[tuple[int, str], tuple[tuple, list]] = {}
        self._content_revision = 0
        self._selection_generation = 0
        self._selection_future: Future | None = None
        self._selection_job: tuple | None = None
        self._record_cache: OrderedDict = OrderedDict()
        self._detail_executor = ThreadPoolExecutor(max_workers=1, thread_name_prefix="esp_record")
        self._field_filter = ""
        self._differences_only = True
        self._raw_record_text: str | None = None
        self._reference_labels: dict[int, str] = {}
        self._references: list | None = None
        self._reference_future: Future | None = None
        self._reference_job: tuple | None = None
        self._array_pages: dict[str, int] = {}
        self._close_popup_handle: int | None = None
        self._close_popup_requested = False
        self._exit_popup_active = False
        self._auto_conflict_stamp: tuple | None = None
        self._copy_popup_requested = False
        self._visible_conflicts_cache: tuple | None = None
        self._scan_counts_cache: tuple | None = None
        # Per-plugin record totals (populated during the warmup load).
        self._plugin_record_counts: dict[int, int] = {}
        # Save popup state.
        self._save_popup_open: bool = False
        self._save_popup_entries: list[_SaveEntry] = []
        self._save_backup: bool = True
        # Conflict scan state.
        self._conflict_scan: ConflictScan | None = None
        self._auto_conflict_future: Future | None = None
        self._auto_conflict_rescan_pending: bool = False
        self._auto_conflict_deferred = False
        self._filter_futures: dict[tuple[int, str], tuple[tuple, Future]] = {}
        self._conflict_filter_text: str = ""
        self._conflict_filter_signature: str = ""
        self._conflict_only_mergeable: bool = False
        self._conflict_selected_fids: set[tuple[int, int]] = set()
        # Validation / visible message state.
        self._validation_report: ValidationReport | None = None
        self._validation_target_handle: int | None = None
        self._validation_summary: str = "(no check run yet)"
        self._latest_error: str | None = None
        self._messages: list[str] = []
        # Patch creation popup.
        self._patch_popup_open: bool = False
        self._patch_popup_name: str = "B21_Patch.esp"
        # Pending copy that's waiting on a "New plugin..." popup to resolve.
        # (source_handle, source_form_id, as_new, deep)
        self._pending_copy: tuple[int, int, bool, bool] | None = None
        # Add Masters modal state.
        self._add_masters_target: int | None = None
        self._add_masters_selected: set[str] = set()
        # Renumber FormIDs / Inject modal state.
        self._renumber_target: int | None = None
        self._renumber_base_text: str = "00000800"
        self._inject_source: int | None = None
        self._inject_target: int | None = None
        self._change_fid_source: tuple[int, int] | None = None
        self._change_fid_new_text: str = ""
        # Reachable / orphan state (populated by run_build_reachable).
        self._reachable_set: set[int] = set()
        self._orphan_form_ids: set[int] = set()

    # -- file operations --------------------------------------------------

    def open_plugin(self, path: str | None = None) -> None:
        if self._busy:
            return
        if path is None:
            path = pick_file(
                title="Open plugin",
                filetypes=[("Plugin", "*.esp;*.esm;*.esl"), ("All", "*.*")],
            )
        if not path:
            return

        # Detect the game on the main thread (cheap header read) so we can
        # kick the schema build off in parallel with the native plugin load.
        try:
            game = detect_game(
                path,
                toolkit_settings=self._toolkit_settings,
                fallback=self.session._default_game,
            )
        except Exception:
            game = None
        self._preload_schema(game)
        known_handles = {p.handle for p in self.session.plugins}

        def _worker():
            self.session.load(path, game=game)
            return [(p, self._warmup_plugin(p)) for p in self.session.plugins if p.handle not in known_handles]

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Failed to load: {result}")
                return
            for plugin, warm in result:
                self._absorb_warmup(plugin, warm)
            self._adopt_session_scan()
            self._start_auto_conflict_scan()
            _log.info("Loaded %s", path)

        self._start_background(
            f"Loading {Path(path).name}...",
            _worker,
            _on_done,
        )

    def open_folder(self, path: str | None = None) -> None:
        """Load every .esp/.esm/.esl in `path` (folder picker if not given)."""
        if self._busy:
            return
        if path is None:
            path = pick_folder(title="Open mod folder")
        if not path:
            return
        folder = Path(path)

        # Best-effort: kick off a schema preload for the toolkit's active
        # game so it overlaps with the native folder load.
        self._preload_schema(self.session._default_game)
        known_handles = {p.handle for p in self.session.plugins}

        def _worker():
            self.session.load_folder(folder)
            return [(p, self._warmup_plugin(p)) for p in self.session.plugins if p.handle not in known_handles]

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Failed to load folder: {result}")
                return
            for plugin, warm in result:
                self._absorb_warmup(plugin, warm)
            self._adopt_session_scan()
            self._start_auto_conflict_scan()
            _log.info("Loaded %d plugin(s) from %s", len(result), folder)

        self._start_background(
            f"Loading folder {folder.name}...",
            _worker,
            _on_done,
        )

    def _warmup_plugin(self, plugin) -> dict:
        """Worker-thread: fetch only group signatures (cheap) so the nav tree
        can render. Group records are fetched lazily by the UI on expand —
        see `_draw_plugin_children`. Schema is preloaded separately via
        `_preload_schema` so it overlaps with the native load."""
        # Idempotent — main-thread caller already kicks this off, but loading
        # masters can introduce additional games we haven't preloaded.
        self._preload_schema(plugin.game)

        group_sigs_fn, _group_summaries_fn = _get_group_api()
        if group_sigs_fn is not None:
            try:
                groups = list(group_sigs_fn(plugin.handle))
            except Exception:
                _log.exception("group_signatures failed for %s", plugin.plugin_name)
                groups = []
            # Group labels count top-level members, not nested CELL/WRLD
            # records. Fetch the real total on this worker, never per frame.
            record_count = int(plugin_handle_get(plugin.handle, "record_count") or 0)
            return {
                "groups": groups,
                "group_records": {},
                "record_count": record_count,
                "masters": tuple(plugin_handle_get(plugin.handle, "masters") or ()),
                "flags": int(plugin_handle_get(plugin.handle, "header_flags") or 0),
            }

        return {"groups": [], "group_records": {}, "record_count": 0}

    @staticmethod
    def _count_legacy_items(items) -> int:
        total = 0
        for item in items:
            if hasattr(item, "children"):
                total += EspEditorApp._count_legacy_items(item.children)
            else:
                total += 1
        return total

    def _absorb_warmup(self, plugin, warm: dict) -> None:
        """Main-thread: install pre-fetched data into UI caches."""
        handle = plugin.handle
        self._invalidate_plugin_tree(handle)
        if "legacy_items" in warm:
            self._cached_root[handle] = warm["legacy_items"]
        else:
            self._cached_groups[handle] = warm.get("groups", [])
            for label, recs in warm.get("group_records", {}).items():
                self._cached_group_records[(handle, label)] = recs
        self._plugin_record_counts[handle] = int(warm.get("record_count", 0))
        self._plugin_masters[handle] = warm.get("masters", ())
        self._plugin_flags[handle] = warm.get("flags", 0)

    def import_load_order(self, path: str | None = None) -> None:
        """Re-order already-loaded plugins by a textual loadorder.txt."""
        if self._busy:
            return
        if path is None:
            path = pick_file(
                title="Import load order",
                filetypes=[("Load order", "*.txt"), ("All", "*.*")],
            )
        if not path:
            return
        try:
            self.session.import_load_order(path)
        except Exception as exc:
            self._show_error(f"Import load order failed: {exc}")
            return
        for plugin in self.session.plugins:
            self._invalidate_plugin_tree(plugin.handle, reset_history=False)
        self._start_auto_conflict_scan()

    def open_new_patch_popup(self) -> None:
        """Request the modal that prompts for a patch plugin filename. The
        actual ``imgui.open_popup`` call is deferred to ``_draw_new_patch_popup``
        so it runs inside the same ID stack as ``begin_popup_modal``."""
        self._patch_popup_open = True

    def _draw_new_patch_popup(self) -> None:
        if self._patch_popup_open:
            imgui.open_popup("New Patch Plugin##esp_patch_modal")
            self._patch_popup_open = False
        imgui.set_next_window_size(imgui.ImVec2(380, 0))
        flags = imgui.WindowFlags_.always_auto_resize
        opened, _ = imgui.begin_popup_modal("New Patch Plugin##esp_patch_modal", None, flags)
        if not opened:
            return
        try:
            imgui.text("Patch plugin filename:")
            _, self._patch_popup_name = imgui.input_text(
                "##patch_name", self._patch_popup_name
            )
            imgui.separator()
            if imgui.button("Create", imgui.ImVec2(100, 0)):
                self._create_patch(self._patch_popup_name.strip())
                imgui.close_current_popup()
                self._patch_popup_open = False
            imgui.same_line()
            if imgui.button("Cancel", imgui.ImVec2(80, 0)):
                imgui.close_current_popup()
                self._patch_popup_open = False
        finally:
            imgui.end_popup()

    def _create_patch(self, name: str) -> None:
        if not name:
            return
        if not name.lower().endswith((".esp", ".esm", ".esl")):
            name = name + ".esp"
        try:
            handle = create_patch_plugin(self.session, name)
        except Exception as exc:
            self._show_error(f"Create patch failed: {exc}")
            return
        self._dirty_handles.add(handle)
        self._invalidate_plugin_tree(handle)
        _log.info("Created patch plugin %s (handle=%s)", name, handle)
        if self._pending_copy is not None:
            src_handle, src_fid, as_new, deep = self._pending_copy
            self._pending_copy = None
            self._do_copy(src_handle, src_fid, handle, as_new=as_new, deep=deep)

    def add_selected_to_patch(self, *, automerge: bool) -> None:
        """Copy or auto-merge the currently selected conflict reports into the
        session's patch plugin."""
        if self._busy:
            return
        if self._conflict_scan is None or not self._conflict_selected_fids:
            return
        patch_handle = self.session._patch_handle
        if patch_handle is None:
            self.open_new_patch_popup()
            return
        reports = [r for r in self._conflict_scan.reports
                   if (r.winner.plugin_handle, r.winner.form_id) in self._conflict_selected_fids]
        if not reports:
            return
        session = self.session

        def _worker():
            if automerge:
                merged = 0
                for rpt in reports:
                    if automerge_to_patch(session, patch_handle, rpt):
                        merged += 1
                return merged
            return add_winners_to_patch(session, patch_handle, reports)

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Patch update failed: {result}")
                return
            self._dirty_handles.add(patch_handle)
            self._invalidate_plugin_tree(patch_handle)
            for fid in {r.form_id for r in reports}:
                self.session.invalidate_form_id(fid)
            self._conflict_selected_fids.clear()
            verb = "Auto-merged" if automerge else "Copied"
            _log.info("%s %d record(s) into patch", verb, result)

        verb = "Auto-merging" if automerge else "Copying"
        self._start_background(
            f"{verb} {len(reports)} record(s)...",
            _worker,
            _on_done,
        )

    def _adopt_session_scan(self) -> None:
        """Pick up whatever scan the session ran during the last load op."""
        scan = getattr(self.session, "_last_conflict_scan", None)
        if scan is not None:
            self._conflict_scan = scan
            _log.info("Auto-scan: %d conflict(s)", len(scan))

    def _start_auto_conflict_scan(self) -> None:
        self._conflict_scan = None
        # Full scans take a process-wide native store lock. Profiling the FO4
        # base + DLC load order showed 1.38 s detail-read delays. Large sessions
        # keep immediate per-record override inspection and scan on request.
        self._auto_conflict_deferred = sum(self._plugin_record_counts.values()) > 100_000
        if self._auto_conflict_deferred:
            self._auto_conflict_rescan_pending = False
            return
        if not self.session.plugins:
            self._conflict_scan = None
            self._auto_conflict_rescan_pending = False
            return
        if self._auto_conflict_future is not None and not self._auto_conflict_future.done():
            self._auto_conflict_rescan_pending = True
            return
        self._auto_conflict_rescan_pending = False
        self._auto_conflict_stamp = self._view_stamp()
        self._auto_conflict_future = self._executor.submit(self.session.run_conflict_scan)

    def _view_stamp(self) -> tuple:
        return (self._content_revision, tuple(
            (p.handle, p.load_order_index, p.plugin_name) for p in self.session.plugins
        ))

    def run_conflict_scan(self) -> None:
        if self._busy:
            return
        if not self.session.plugins:
            return
        self._auto_conflict_deferred = False
        self.cancel_override_analysis()

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Conflict scan failed: {result}")
                return
            self._conflict_scan = result
            self._scan_complete = True
            _log.info("Conflict scan: %d conflict(s)", len(result))

        self._start_background(
            "Scanning conflicts...",
            lambda: self.session.run_conflict_scan(),
            _on_done,
        )

    def start_override_analysis(self) -> None:
        """Build richer cross-plugin info in cancellable record-type batches."""
        if self._analysis_future is not None or self._busy or not self.session.plugins:
            return
        self._analysis_stamp = self._view_stamp()
        self._analysis_completed = 0
        self._analysis_total = 0
        self._analysis_cancelled = False
        self._analysis_label = "Reading record types"
        self._analysis_queue = []
        self._auto_conflict_rescan_pending = False
        self._auto_conflict_stamp = ()
        if self._auto_conflict_future is not None:
            if self._auto_conflict_future.cancel():
                self._auto_conflict_future = None
        plugins = self.session.plugins
        def inventory():
            counts = {}
            for plugin in plugins:
                for sig, count in plugin_handle_record_counts(plugin.handle).items():
                    counts[sig] = counts.get(sig, 0) + int(count)
            return counts
        self._analysis_future = self._executor.submit(inventory)

    def cancel_override_analysis(self) -> None:
        if self._analysis_future is None and not self._analysis_queue:
            return
        self._analysis_cancelled = True
        self._analysis_queue = []
        if self._analysis_future is not None:
            self._analysis_future.cancel()
        self._analysis_label = "Cancelled; current read finishes safely" if self._analysis_future else "Cancelled"

    def _poll_analysis(self) -> None:
        future = self._analysis_future
        if future is None or not future.done():
            return
        self._analysis_future = None
        if self._analysis_cancelled or self._analysis_stamp != self._view_stamp():
            self._analysis_queue = []
            self._analysis_label = "Cancelled" if self._analysis_cancelled else "Records or load order changed; analyze again"
            return
        try:
            result = future.result()
        except Exception as exc:
            self._analysis_queue = []
            self._analysis_label = "Analysis failed"
            self._show_error(f"Override analysis failed: {exc}")
            return
        if isinstance(result, dict):
            # Eight signatures bound each pass's report conversion. Large
            # signatures still need the existing native store lock.
            signatures = sorted(result)
            self._analysis_queue = [signatures[i:i + 8] for i in range(0, len(signatures), 8)]
            self._analysis_total = len(self._analysis_queue)
            self._conflict_scan = ConflictScan()
            self._scan_complete = False
            self._analyzed_types.clear()
        else:
            reports = {**self._conflict_scan.by_record_key, **result.by_record_key}
            addresses = {**self._conflict_scan.by_handle_form_id, **result.by_handle_form_id}
            self._conflict_scan = ConflictScan(by_record_key=reports, by_handle_form_id=addresses)
            self._analysis_completed += 1
        if self._analysis_queue:
            batch = self._analysis_queue.pop(0)
            self._analyzed_types.update(batch)
            self._analysis_label = f"Reading {', '.join(batch)}"
            self._analysis_future = self._executor.submit(ConflictScanner().scan, self.session, signatures=batch)
        else:
            self._scan_complete = True
            self._analysis_label = "Analysis complete"

    def run_validation(self, handle: int | None = None) -> None:
        if self._busy:
            return
        target = self.session.get_by_handle(handle) if handle is not None else self.session.active
        if target is None:
            self._show_error("Check for errors failed: no active plugin")
            return
        target_handle = target.handle
        target_name = target.plugin_name

        def _worker():
            return validate(self.session, handle=target_handle)

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Check for errors failed: {result}")
                return
            self._validation_report = result
            self._validation_target_handle = target_handle
            issue_count = len(result)
            self._validation_summary = f"{issue_count} issue(s) in {target_name}"
            self._add_message(f"Check for errors: {self._validation_summary}")
            _log.info("Check for errors: %s", self._validation_summary)

        self._start_background(f"Checking {target_name} for errors...", _worker, _on_done)

    def open_save_popup(self) -> None:
        """Populate and open the multi-plugin save modal."""
        if self._busy:
            return
        if not self._dirty_handles:
            return
        self._save_popup_entries = []
        for plugin in self.session.plugins:
            if plugin.handle in self._dirty_handles:
                self._save_popup_entries.append(
                    _SaveEntry(
                        handle=plugin.handle,
                        plugin_name=plugin.plugin_name,
                        path=plugin.path,
                        selected=True,
                    )
                )
        if not self._save_popup_entries:
            return
        self._save_popup_open = True

    def _draw_save_popup(self) -> None:
        """Draw the save-plugins modal. Must be called every frame."""
        if self._save_popup_open:
            imgui.open_popup("Save Plugins##esp_save_modal")
            self._save_popup_open = False
        prepare_dialog(480, 300)
        imgui.set_next_window_size_constraints(imgui.ImVec2(480, 0), imgui.ImVec2(10000, 10000))
        flags = imgui.WindowFlags_.always_auto_resize
        modal_open, _ = imgui.begin_popup_modal("Save Plugins##esp_save_modal", None, flags)
        if not modal_open:
            return
        try:
            imgui.text("Select plugins to save:")
            imgui.separator()
            for entry in self._save_popup_entries:
                dirty_star = "* " if entry.handle in self._dirty_handles else ""
                _, entry.selected = imgui.checkbox(
                    f"##save_cb_{entry.handle}",
                    entry.selected,
                )
                imgui.same_line()
                imgui.text_wrapped(f"{dirty_star}{entry.plugin_name}")
            imgui.separator()
            _, self._save_backup = imgui.checkbox(
                "Create backup (.bak)##save_backup", self._save_backup
            )
            imgui.separator()
            if imgui.button("Save Selected"):
                self._execute_save_selected()
                imgui.close_current_popup()
                self._save_popup_open = False
            imgui.same_line()
            if imgui.button("Cancel"):
                imgui.close_current_popup()
                self._save_popup_open = False
        finally:
            imgui.end_popup()

    def _execute_save_selected(self) -> None:
        """Save all checked entries in a single background task."""
        to_save = [e for e in self._save_popup_entries if e.selected]
        if not to_save:
            return
        do_backup = self._save_backup
        # Snapshot the data we need; entries list is UI state.
        save_jobs = [(e.handle, e.plugin_name, e.path) for e in to_save]

        def _worker():
            results = []
            for handle, plugin_name, target in save_jobs:
                try:
                    if do_backup:
                        _make_backup(target)
                    plugin_handle_call(handle, "save", target)
                    results.append((handle, plugin_name, None))
                except Exception as exc:
                    results.append((handle, plugin_name, exc))
            return results

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Save failed: {result}")
                return
            for handle, plugin_name, exc in result:
                if exc is None:
                    self._dirty_handles.discard(handle)
                    self._dirty_records.pop(handle, None)
                    _log.info("Saved %s", plugin_name)
                else:
                    self._show_error(f"Save failed for {plugin_name}: {exc}")

        label = ", ".join(n for _, n, _ in save_jobs)
        self._start_background(f"Saving {label}...", _worker, _on_done)

    def save_active(self, *, save_as: bool = False) -> None:
        if self._busy:
            return
        active = self.session.active
        if active is None:
            return
        if save_as:
            target = pick_save_file(
                title="Save plugin as",
                filetypes=[("Plugin", "*.esp;*.esm;*.esl"), ("All", "*.*")],
                default_ext=".esp",
            )
            if not target:
                return
            handle = active.handle
            plugin_name = active.plugin_name
            target_name = Path(target).name
            existing = self.session.get_by_name(target_name)
            if existing is not None and existing.handle != handle:
                self._show_error(f"Save As target {target_name} is already loaded")
                return
            if target_name.casefold() != plugin_name.casefold():
                dependents = [p.plugin_name for p in self.session.plugins if plugin_name.casefold() in
                    [name.casefold() for name in (plugin_handle_get(p.handle, "masters") or [])]]
                if dependents:
                    self._show_error(f"Cannot rename {plugin_name} while required by {', '.join(dependents)}")
                    return

            def work():
                if self._save_backup:
                    _make_backup(target)
                return plugin_handle_call(handle, "save", target)

            def _on_done(result, is_error: bool) -> None:
                if is_error:
                    self._show_error(f"Save failed: {result}")
                    return
                self._dirty_handles.discard(handle)
                self._dirty_records.pop(handle, None)
                active.path = str(Path(target).resolve())
                active.plugin_name = target_name
                self.session._invalidate_cache()
                self._invalidate_plugin_tree(handle, reset_history=False)
                self.cancel_selection()
                self._request_selection()
                self._start_auto_conflict_scan()
                _log.info("Saved %s -> %s", plugin_name, target)

            self._start_background(
                f"Saving {Path(target).name}...",
                work,
                _on_done,
            )
        else:
            # Normal Ctrl+S / Save button → show popup.
            self.open_save_popup()

    def _export_plugin_text(self, plugin, format: str) -> None:
        if self._busy:
            return
        fmt = format.lower()
        plugin_name = getattr(plugin, "plugin_name", "Plugin")
        initialfile = plugin_name if plugin_name.lower().endswith(f".{fmt}") else f"{plugin_name}.{fmt}"
        target = pick_save_file(
            title=f"Export Plugin as {fmt.upper()}",
            filetypes=[(fmt.upper(), f"*.{fmt}"), ("All", "*.*")],
            default_ext=f".{fmt}",
            initialfile=initialfile,
        )
        if not target:
            return
        handle = int(getattr(plugin, "handle"))

        def _worker():
            text = plugin_handle_call(handle, "export_plugin_text", "lossless", fmt)
            Path(target).write_text(text, encoding="utf-8")
            return target

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Export failed: {result}")
                return
            _log.info("Exported %s to %s", plugin_name, result)

        self._start_background(f"Exporting {plugin_name}...", _worker, _on_done)

    def _export_record_text(self, source_handle: int, item, format: str) -> None:
        if self._busy:
            return
        fmt = format.lower()
        plugin = self.session.get_by_handle(source_handle)
        plugin_name = plugin.plugin_name if plugin else "Record"
        editor_id = getattr(item, "editor_id", None) or getattr(item, "signature", None) or "Record"
        initialfile = f"{_safe_export_stem(str(editor_id), 'Record')}.{fmt}"
        target = pick_save_file(
            title=f"Export Record as {fmt.upper()}",
            filetypes=[(fmt.upper(), f"*.{fmt}"), ("All", "*.*")],
            default_ext=f".{fmt}",
            initialfile=initialfile,
        )
        if not target:
            return
        form_id = int(getattr(item, "form_id"))

        def _worker():
            text = plugin_handle_call(source_handle, "export_record_text", form_id, fmt)
            Path(target).write_text(text, encoding="utf-8")
            return target

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Export failed: {result}")
                return
            _log.info("Exported record 0x%08X from %s to %s", form_id, plugin_name, result)

        self._start_background(f"Exporting {form_id:08X}...", _worker, _on_done)

    def _visible_conflict_reports(self) -> list[ConflictReport]:
        scan = self._conflict_scan
        active = self.session.active
        if scan is None or active is None:
            return []
        sig_filter = self._conflict_filter_signature.strip().upper()
        text_filter = self._conflict_filter_text.strip().lower()
        key = (id(scan), active.handle, sig_filter, text_filter, self._conflict_only_mergeable)
        if self._visible_conflicts_cache is not None and self._visible_conflicts_cache[0] == key:
            return self._visible_conflicts_cache[1]
        visible: list[ConflictReport] = []
        for rpt in sorted(scan.reports, key=lambda r: (r.signature, r.form_id)):
            if sig_filter and rpt.signature.upper() != sig_filter:
                continue
            if not any(e.plugin_handle == active.handle for e in rpt.chain):
                continue
            if self._conflict_only_mergeable and not rpt.mergeable:
                continue
            if text_filter:
                eid = (rpt.editor_id or "").lower()
                if text_filter not in eid and text_filter not in f"{rpt.form_id:08x}":
                    continue
            visible.append(rpt)
        self._visible_conflicts_cache = (key, visible)
        return visible

    def _export_validation_report(self, format: str) -> None:
        if self._busy:
            return
        report = self._validation_report
        if report is None:
            return
        fmt = format.lower()
        target = self.session.get_by_handle(self._validation_target_handle or -1)
        plugin_name = target.plugin_name if target else "validation"
        initialfile = f"{_safe_export_stem(plugin_name, 'validation')}.validation.{fmt}"
        path = pick_save_file(
            title=f"Export Validation Report as {fmt.upper()}",
            filetypes=[(fmt.upper(), f"*.{fmt}"), ("All", "*.*")],
            default_ext=f".{fmt}",
            initialfile=initialfile,
        )
        if not path:
            return
        issues = list(report)

        def _worker():
            if fmt == "csv":
                with open(path, "w", encoding="utf-8", newline="") as f:
                    writer = csv.DictWriter(
                        f,
                        fieldnames=[
                            "severity",
                            "category",
                            "plugin",
                            "form_id",
                            "message",
                        ],
                    )
                    writer.writeheader()
                    for issue in issues:
                        writer.writerow(
                            {
                                "severity": issue.severity.value,
                                "category": issue.category.value,
                                "plugin": issue.plugin_name,
                                "form_id": "" if issue.form_id is None else f"0x{int(issue.form_id):08X}",
                                "message": issue.message,
                            }
                        )
            else:
                payload = {
                    "kind": "validation_report",
                    "plugin": plugin_name,
                    "target_handle": self._validation_target_handle,
                    "rows": [
                        {
                            "severity": issue.severity.value,
                            "category": issue.category.value,
                            "plugin": issue.plugin_name,
                            "form_id": issue.form_id,
                            "message": issue.message,
                        }
                        for issue in issues
                    ],
                }
                Path(path).write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
            return path

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Export failed: {result}")
                return
            _log.info("Exported validation report to %s", result)

        self._start_background("Exporting validation report...", _worker, _on_done)

    def _export_conflict_report(self, format: str) -> None:
        if self._busy:
            return
        scan = self._conflict_scan
        if scan is None:
            return
        fmt = format.lower()
        active = self.session.active
        active_name = active.plugin_name if active else "conflicts"
        initialfile = f"{_safe_export_stem(active_name, 'conflicts')}.conflicts.{fmt}"
        path = pick_save_file(
            title=f"Export Conflict Report as {fmt.upper()}",
            filetypes=[(fmt.upper(), f"*.{fmt}"), ("All", "*.*")],
            default_ext=f".{fmt}",
            initialfile=initialfile,
        )
        if not path:
            return
        visible_reports = self._visible_conflict_reports()

        def _row_payload(report: ConflictReport) -> dict[str, object]:
            return {
                "form_id": f"0x{int(report.form_id):08X}",
                "signature": report.signature,
                "editor_id": report.editor_id,
                "status": report.status.value,
                "mergeable": report.mergeable,
                "chain_count": len(report.chain),
                "winner_plugin": report.winner.plugin_name,
                "winner_form_id": f"0x{int(report.winner.form_id):08X}",
                "chain": [
                    {
                        "plugin_handle": entry.plugin_handle,
                        "plugin_name": entry.plugin_name,
                        "load_order_index": entry.load_order_index,
                        "form_id": f"0x{int(entry.form_id):08X}",
                        "payload_hash": entry.payload_hash,
                    }
                    for entry in report.chain
                ],
            }

        def _worker():
            if fmt == "csv":
                with open(path, "w", encoding="utf-8", newline="") as f:
                    writer = csv.DictWriter(
                        f,
                        fieldnames=[
                            "form_id",
                            "signature",
                            "editor_id",
                            "status",
                            "mergeable",
                            "chain_count",
                            "winner_plugin",
                            "winner_form_id",
                        ],
                    )
                    writer.writeheader()
                    for report in visible_reports:
                        writer.writerow(
                            {
                                "form_id": f"0x{int(report.form_id):08X}",
                                "signature": report.signature,
                                "editor_id": report.editor_id or "",
                                "status": report.status.value,
                                "mergeable": str(report.mergeable).lower(),
                                "chain_count": len(report.chain),
                                "winner_plugin": report.winner.plugin_name,
                                "winner_form_id": f"0x{int(report.winner.form_id):08X}",
                            }
                        )
            else:
                payload = {
                    "kind": "conflict_report",
                    "active_plugin": active_name,
                    "filters": {
                        "text": self._conflict_filter_text,
                        "signature": self._conflict_filter_signature,
                        "mergeable_only": self._conflict_only_mergeable,
                    },
                    "rows": [_row_payload(report) for report in visible_reports],
                }
                Path(path).write_text(json.dumps(payload, indent=2, ensure_ascii=False), encoding="utf-8")
            return path

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Export failed: {result}")
                return
            _log.info("Exported conflict report to %s", result)

        self._start_background("Exporting conflict report...", _worker, _on_done)

    def close_active(self) -> None:
        if self._busy:
            return
        active = self.session.active
        if active is None:
            return
        if active.handle in self._dirty_handles:
            self._close_popup_handle = active.handle
            self._close_popup_requested = True
            return
        self._close_plugin(active.handle)

    def _close_plugin(self, handle: int) -> None:
        try:
            self.session.close(handle)
        except Exception as exc:
            self._show_error(str(exc))
            return
        self._invalidate_plugin_tree(handle)
        self._dirty_handles.discard(handle)
        self._dirty_records.pop(handle, None)
        self._plugin_record_counts.pop(handle, None)
        self._undo = [item for item in self._undo if item[0] != handle]
        self._redo = [item for item in self._redo if item[0] != handle]
        if self.selection.plugin_handle == handle:
            self.cancel_selection()
            self.selection = _Selection()
        self._start_auto_conflict_scan()

    def _draw_close_popup(self) -> None:
        if self._close_popup_requested:
            imgui.open_popup("Unsaved changes##esp_close")
            self._close_popup_requested = False
        prepare_dialog(560, 180)
        opened, _ = imgui.begin_popup_modal("Unsaved changes##esp_close", None,
                                           imgui.WindowFlags_.always_auto_resize)
        if not opened:
            return
        try:
            handle = self._close_popup_handle
            plugin = self.session.get_by_handle(handle)
            if plugin is None:
                imgui.close_current_popup()
                return
            imgui.text_wrapped(f"{plugin.plugin_name} has unsaved changes.")
            if imgui.button("Save and close"):
                path = plugin.path
                def work():
                    if self._save_backup:
                        _make_backup(path)
                    plugin_handle_call(handle, "save", path)
                def done(result, is_error):
                    if is_error:
                        self._show_error(f"Save failed: {result}")
                    else:
                        self._close_plugin(handle)
                self._start_background(f"Saving {plugin.plugin_name}...", work, done)
                imgui.close_current_popup()
            imgui.same_line()
            if imgui.button("Discard and close"):
                self._close_plugin(handle)
                imgui.close_current_popup()
            imgui.same_line()
            if imgui.button("Cancel"):
                self._close_popup_handle = None
                imgui.close_current_popup()
        finally:
            imgui.end_popup()

    def _preload_schema(self, game: str | None) -> None:
        """Warm the schema cache for `game` on a worker thread so the first
        record selection doesn't block the UI while FO4's ~122k-line generated
        schema module imports."""
        if not game or game in self._schema_preload_started:
            return
        self._schema_preload_started.add(game)
        self._executor.submit(get_schema, game)

    def exit_confirmation_state(self):
        """An approval covers these edits and pending native mutations only."""
        self.poll()
        return (self._view_stamp(), frozenset(self._dirty_handles),
                tuple((handle, tuple(sorted(ids))) for handle, ids in sorted(self._dirty_records.items())),
                self._busy, id(self._busy_future) if self._busy_future is not None else None)

    def reset_exit_confirmation(self):
        if self._exit_popup_active:
            opened, _ = imgui.begin_popup_modal("Unsaved plugins##esp_quit", None,
                                               imgui.WindowFlags_.always_auto_resize)
            if opened:
                imgui.close_current_popup()
                imgui.end_popup()
        self._exit_popup_active = False

    def confirm_exit(self) -> bool | None:
        """Protect every dirty plugin, including when this workspace is inactive."""
        self.poll()
        if not self._exit_popup_active:
            if not self._busy and not self._dirty_handles:
                return True
            imgui.open_popup("Unsaved plugins##esp_quit")
            self._exit_popup_active = True
        prepare_dialog(560, 220)
        opened, _ = imgui.begin_popup_modal("Unsaved plugins##esp_quit", None,
                                           imgui.WindowFlags_.always_auto_resize)
        if not opened:
            return None
        try:
            if not self._busy and not self._dirty_handles:
                imgui.close_current_popup()
                self._exit_popup_active = False
                return True
            imgui.text_wrapped("Save your changed plugins before quitting ModBox21?")
            dirty = [p for p in self.session.plugins if p.handle in self._dirty_handles]
            for plugin in dirty:
                imgui.text_wrapped(f"* {plugin.plugin_name}")
            if self._busy:
                imgui.text_wrapped(self._busy_message)
            elif self._latest_error:
                imgui.text_wrapped(self._latest_error)
            _, self._save_backup = imgui.checkbox("Create backup (.bak)##quit_backup", self._save_backup)
            imgui.separator()
            imgui.begin_disabled(self._busy)
            if imgui.button("Save all and quit") and not self._busy:
                self._save_popup_entries = [
                    _SaveEntry(p.handle, p.plugin_name, p.path) for p in dirty
                ]
                self._execute_save_selected()
            imgui.same_line()
            if imgui.button("Discard all and quit") and not self._busy:
                imgui.close_current_popup()
                self._exit_popup_active = False
                imgui.end_disabled()
                return True
            imgui.end_disabled()
            imgui.same_line()
            if imgui.button("Cancel"):
                imgui.close_current_popup()
                self._exit_popup_active = False
                return False
            return None
        finally:
            imgui.end_popup()

    # -- background plumbing ----------------------------------------------

    def _start_background(self, message: str, work, on_done) -> None:
        self._busy = True
        self._busy_message = message
        self._busy_done_cb = on_done
        self._busy_future = self._executor.submit(work)

    def poll(self) -> None:
        """Called every frame from the workspace. Finalizes background work."""
        self._poll_selection()
        self._poll_analysis()
        self._poll_overview()
        for key, (stamp, future) in list(self._filter_futures.items()):
            if future.done():
                self._filter_futures.pop(key, None)
                records = self._cached_group_records.get(key)
                current = self._filter_stamp(records)
                if stamp == current:
                    try:
                        self._filtered_records[key] = (stamp, future.result())
                    except Exception as exc:
                        self._filter_errors[key] = (stamp, str(exc))
                        self._show_error(f"Record filter failed: {exc}")
        for handle, future in list(self._header_futures.items()):
            if future.done():
                self._header_futures.pop(handle, None)
                if self.session.get_by_handle(handle) is not None:
                    try:
                        self._header_cache[handle] = future.result()
                        header = self._header_cache[handle]
                        self._plugin_masters[handle] = tuple(
                            str(item[0] if isinstance(item, (tuple, list)) else item)
                            for item in header.masters
                        )
                        self._plugin_flags[handle] = int(header.flags)
                    except Exception as exc:
                        self._show_error(f"Unable to load plugin information: {exc}")
        # Drain group-record fetches.
        for key, fut in list(self._group_futures.items()):
            if fut.done():
                handle, sig = key
                try:
                    fetched = fut.result()
                    if isinstance(fetched, tuple) and len(fetched) == 2 and isinstance(fetched[1], set):
                        self._cached_group_records[key], self._known_group_types[key] = fetched
                    else:
                        self._cached_group_records[key] = fetched
                except Exception as exc:
                    _log.exception("Failed to fetch group %s for handle %s", sig, handle)
                    self._cached_group_records[key] = []
                    self._group_errors[key] = str(exc)
                self._group_futures.pop(key, None)

        # Drain tree-skeleton futures (either group-signatures or legacy full-tree).
        group_sigs_fn, _ = _get_group_api()
        for handle, fut in list(self._tree_futures.items()):
            if fut.done():
                try:
                    result = fut.result()
                    if group_sigs_fn is not None:
                        # Lazy path: result is list[(label, count)].
                        self._cached_groups[handle] = list(result)
                    else:
                        # Legacy path: result is full item list.
                        self._cached_root[handle] = list(result)
                except Exception:
                    _log.exception("Failed tree fetch for handle %s", handle)
                    self._cached_groups[handle] = []
                    self._cached_root[handle] = []
                self._tree_futures.pop(handle, None)

        if self._auto_conflict_future is not None and self._auto_conflict_future.done():
            future = self._auto_conflict_future
            self._auto_conflict_future = None
            try:
                result = future.result()
                if not self._auto_conflict_rescan_pending and (
                    self._auto_conflict_stamp is None or self._auto_conflict_stamp == self._view_stamp()
                ):
                    self._conflict_scan = result
                    self._scan_complete = True
            except Exception as exc:
                self._show_error(f"Conflict scan failed: {exc}")
            if self._auto_conflict_rescan_pending:
                self._start_auto_conflict_scan()

        if not self._busy or self._busy_future is None:
            return
        if not self._busy_future.done():
            return
        future = self._busy_future
        cb = self._busy_done_cb
        self._busy_future = None
        self._busy_done_cb = None
        self._busy = False
        self._busy_message = ""
        try:
            result = future.result()
            if cb is not None:
                cb(result, False)
        except Exception as exc:
            _log.exception("Background task failed")
            if cb is not None:
                cb(exc, True)

    def draw_busy_overlay(self) -> None:
        if self._busy:
            viewport = imgui.get_main_viewport()
            loading_panel("Working", self._busy_message, None, bounds=(viewport.work_pos, viewport.work_size))

    # -- panels -----------------------------------------------------------

    @staticmethod
    def _choice(label, widget_id, options, selected):
        imgui.text(label)
        imgui.set_next_item_width(-1)
        if imgui.begin_combo(f"##{widget_id}", selected):
            try:
                for index, option in enumerate(options):
                    if imgui.selectable(f"{option}###{widget_id}_option_{index}", option == selected)[0]:
                        selected = option
            finally:
                imgui.end_combo()
        return selected

    def _filter_stamp(self, records):
        return (id(records), self._filter_text.strip().casefold(), self._filter_signature.strip().upper(),
                self._search_field, self._record_sort, self._sort_descending, self._content_revision)

    def _session_slots(self):
        stamp = tuple((p.handle, p.game, p.load_order_index, self._plugin_flags.get(p.handle))
                      for p in self.session.plugins)
        if self._slots_cache is None or self._slots_cache[0] != stamp:
            ready_games = {p.game for p in self.session.plugins if all(
                q.handle in self._plugin_flags for q in self.session.plugins if q.game == p.game)}
            self._slots_cache = (stamp, session_slots([p for p in self.session.plugins if p.game in ready_games], self._plugin_flags))
        return self._slots_cache[1]

    def _record_identity(self, handle, form_id):
        plugin = self.session.get_by_handle(handle)
        if plugin is None:
            return None
        masters = self._plugin_masters.get(handle)
        if masters is None and self.selection.document is not None and self.selection.document.handle == handle:
            masters = self.selection.document.masters
        if masters is None:
            return RecordIdentity(f"{form_id:08X}", None, form_id & 0xFFFFFF, None)
        return record_identity(plugin, form_id, masters, self.session.plugins, self._session_slots())

    def _draw_role_legend(self):
        for role, label in _ROLE_LABELS.items():
            imgui.text_colored(_ROLE_COLORS.get(role, _MODIFIED_COLOR), label)
        imgui.text_wrapped("A conflict means payload bytes differ. Inspect Overrides to compare values. Uncompared chains may include partial forms.")

    def _record_label(self, signature: str, game: str | None = None) -> str:
        """Schema display label for a record signature, e.g. 'Weapon' for 'WEAP'."""
        if not signature:
            return signature
        fallback = record_type_display_label(signature)
        # Try the active plugin's game first, then any loaded plugin.
        candidates: list[str] = []
        if game:
            candidates.append(game)
        active = self.session.active
        if active is not None and active.game not in candidates:
            candidates.append(active.game)
        for plugin in self.session.plugins:
            if plugin.game not in candidates:
                candidates.append(plugin.game)
        for g in candidates:
            label = record_type_display_label(signature, g)
            if label != fallback:
                return label
        return fallback

    def draw_nav_tree(self) -> None:
        self._search_field = self._choice("Search by", "search_field", SEARCH_FIELDS, self._search_field)
        imgui.set_next_item_width(-1)
        _, self._filter_text = imgui.input_text_with_hint("##filter_text", "Case-insensitive search", self._filter_text)
        imgui.set_item_tooltip("All indexed fields searches Editor ID, file FormID and record type. Name searches resolved FULL text on a worker. FormID accepts an optional 0x prefix.")
        types = sorted({sig for groups in self._cached_groups.values() if groups for sig, _ in groups}
                       | {sig for types in self._known_group_types.values() for sig in types})
        type_labels = {sig: f"{self._record_label(sig)} ({sig})" for sig in types}
        choices = ("All record types", *type_labels.values())
        current = type_labels.get(self._filter_signature, "All record types")
        selected = self._choice("Record type", "record_type", choices, current)
        self._filter_signature = next((sig for sig, label in type_labels.items() if label == selected), "")
        self._record_sort = self._choice("Sort records by", "record_sort", RECORD_SORTS, self._record_sort)
        _, self._sort_descending = imgui.checkbox("Descending##record_direction", self._sort_descending)
        imgui.set_item_tooltip("Display sorting only. Plugins remain in session load order; import Load Order to change their priority.")
        if imgui.button("Overrides overview##open_overview", imgui.ImVec2(-1, 0)):
            self._show_overview = True
        with expandable_section("Color legend###nav_legend") as expanded:
            if expanded:
                self._draw_role_legend()
        imgui.separator()

        plugins = self.session.plugins
        if not plugins:
            imgui.text_disabled("No plugins loaded.")
            self._draw_save_popup()
            self._draw_new_patch_popup()
            return

        # Scrollable tree region — keeps the filter bar fixed at top.
        imgui.begin_child("##esp_nav_scroll", imgui.ImVec2(0, 0))
        for plugin in plugins:
            slot = self._session_slots().get(plugin.handle)
            label = f"[{slot.label if slot else '?'}] {plugin.plugin_name}"
            if plugin.handle == (self.session.active.handle if self.session.active else None):
                label += "  [active]"
            if plugin.handle == self.session._patch_handle:
                label += "  [patch]"
            if plugin.is_master:
                label += "  (master)"
            if plugin.handle in self._dirty_handles:
                label = "* " + label

            tree_open = imgui.tree_node_ex(
                f"{label}###plugin_{plugin.handle}",
                imgui.TreeNodeFlags_.default_open if not plugin.is_master else 0,
            )
            if imgui.is_item_hovered():
                imgui.set_tooltip(self._plugin_tooltip(plugin))
            if imgui.is_item_clicked() and not imgui.is_item_toggled_open():
                self.session.set_active(plugin.handle)
            self._draw_plugin_context_menu(plugin)
            if tree_open:
                self._draw_plugin_children(plugin.handle)
                imgui.tree_pop()
        imgui.end_child()

        # Draw the modal popups (same ID stack scope as open_popup).
        self._draw_save_popup()
        self._draw_close_popup()
        self._draw_copy_popup()
        self._draw_new_patch_popup()
        self._draw_add_masters_popup()
        self._draw_renumber_popup()
        self._draw_inject_popup()
        self._draw_change_form_id_popup()

    def _draw_plugin_children(self, handle: int) -> None:
        group_sigs_fn, group_recs_fn = _get_group_api()

        if group_sigs_fn is not None and group_recs_fn is not None:
            # Lazy path: fetch group skeleton first, then records per group on expand.
            groups = self._cached_groups.get(handle)
            if groups is None:
                # First expand: kick off group-signature fetch (cheap).
                if handle not in self._tree_futures:
                    self._tree_futures[handle] = self._executor.submit(
                        group_sigs_fn, handle
                    )
                    # Store sentinel so we know we started.
                    self._cached_groups[handle] = None  # type: ignore[assignment]
                imgui.text_disabled("Loading groups...")
                return
            # groups is a list[(label, count)] — render group nodes.
            plugin = self.session.get_by_handle(handle)
            game = plugin.game if plugin else None
            for label, count in sorted(groups, key=lambda group: (self._record_label(group[0], game).casefold(), group[0])):
                group_key = (handle, label)
                display = self._record_label(label, game)
                records = self._cached_group_records.get(group_key)
                suffix = f" ({len(records)})" if records is not None else ""
                node_label = f"{display}{suffix}###grp_{handle}_{label}"
                if imgui.tree_node(node_label):
                    records = self._cached_group_records.get(group_key)
                    if records is None:
                        if group_key not in self._group_futures:
                            self._group_futures[group_key] = self._executor.submit(
                                self._fetch_group_records,
                                handle,
                                label,
                            )
                            self._cached_group_records[group_key] = None  # type: ignore[assignment]
                        imgui.text_disabled("Loading records...")
                    else:
                        if group_key in self._group_errors:
                            imgui.text_wrapped(f"Unable to load records: {self._group_errors[group_key]}")
                            if imgui.small_button(f"Retry records##{handle}_{label}"):
                                self._group_errors.pop(group_key, None)
                                self._cached_group_records.pop(group_key, None)
                        else:
                            self._draw_group_records(group_key, records)
                    imgui.tree_pop()
        else:
            imgui.text_disabled("Record summary API unavailable.")

    @staticmethod
    def _fetch_group_records(handle, label):
        _, getter = _get_group_api()
        records = (getter or plugin_handle_group_record_summaries)(handle, label)
        return records, {r.signature for r in records}

    def _draw_item(self, handle: int, item) -> None:
        if hasattr(item, "children"):
            label = item.label_text or "<group>"
            count = len(item.children)
            sig_filter = self._filter_signature.strip().upper()
            if sig_filter and sig_filter != label.upper():
                pass
            plugin = self.session.get_by_handle(handle)
            game = plugin.game if plugin else None
            display = self._record_label(label, game)
            if imgui.tree_node(f"{display} ({count})###grp_{handle}_{id(item)}"):
                for child in item.children:
                    self._draw_item(handle, child)
                imgui.tree_pop()
            return

        if not self._record_matches_filter(item):
            return
        self._draw_record_leaf(handle, item)

    def _filter_group_records(self, key: tuple[int, str], records: list) -> list:
        stamp = self._filter_stamp(records)
        cached = self._filtered_records.get(key)
        if cached is not None and cached[0] == stamp:
            return cached[1]
        error = self._filter_errors.get(key)
        if error and error[0] == stamp:
            return []
        pending = self._filter_futures.get(key)
        if pending is not None and pending[0] == stamp:
            return []
        text, signature, field, order, descending = stamp[1:6]
        plugin = self.session.get_by_handle(key[0])
        types = self._known_group_types.get(key)
        if types is None:
            types = {r.signature for r in records[:4096]}
        labels = {sig: self._record_label(sig, plugin.game if plugin else None) for sig in types}
        def work():
            if field == "Name" and text:
                rows = plugin_handle_search_records(key[0], text, mode="substring", match_full=True, read_full=True)
                found = {row["form_id"] for row in rows if text in (row.get("full_name") or "").casefold()}
                visible = [item for item in records if item.form_id in found and (not signature or item.signature == signature)]
            else:
                visible = [item for item in records if matches(item, text, signature, field, labels)] if text or signature else records
            return sort_records(visible, order, descending)
        if len(records) < 4096 and field != "Name":
            result = work()
            self._filtered_records[key] = (stamp, result)
            return result
        pending = self._filter_futures.get(key)
        if pending is None or pending[0] != stamp:
            if pending is not None:
                pending[1].cancel()
            self._filter_futures[key] = (stamp, self._executor.submit(work))
        return []

    def _draw_group_records(self, key: tuple[int, str], records: list) -> None:
        # Old nested-tree adapters are variable-height; native summaries are flat.
        if records and hasattr(records[0], "children"):
            for item in records:
                self._draw_item(key[0], item)
            return
        visible = self._filter_group_records(key, records)
        if key in self._filter_futures:
            imgui.text_disabled("Searching and sorting records...")
        elif key in self._filter_errors and self._filter_errors[key][0] == self._filter_stamp(records):
            imgui.text_wrapped(f"Search failed: {self._filter_errors[key][1]}")
            if imgui.small_button(f"Retry search##{key}"):
                self._filter_errors.pop(key, None)
        elif not visible:
            imgui.text_disabled("No matching records.")
        clipper = imgui.ListClipper()
        clipper.begin(len(visible))
        while clipper.step():
            for i in range(clipper.display_start, clipper.display_end):
                self._draw_record_leaf(key[0], visible[i])
        clipper.end()

    def _draw_record_leaf(self, handle: int, item) -> None:
        editor_id = item.editor_id or ""
        plugin = self.session.get_by_handle(handle)
        game = plugin.game if plugin else None
        sig_label = self._record_label(item.signature, game)
        is_selected = (
            self.selection.record is not None
            and self.selection.record.form_id == item.form_id
            and self.selection.plugin_handle == handle
        )
        role = self._record_role(handle, item.form_id)
        marker = f"[{_ROLE_TAGS[role]}] " if role in _ROLE_TAGS else ""
        label = f"{marker}{item.form_id:08X}  {editor_id or sig_label}"
        is_modified = item.form_id in self._dirty_records.get(handle, ())
        if is_modified:
            label = "[E] " + label
            color = _MODIFIED_COLOR
        else:
            color = _ROLE_COLORS.get(role)
        pushed_text = False
        if color is not None:
            imgui.push_style_color(imgui.Col_.text, color)
            pushed_text = True

        pushed_header = False
        if is_selected:
            imgui.push_style_color(imgui.Col_.header, _SELECTED_BG_COLOR)
            imgui.push_style_color(imgui.Col_.header_hovered, _SELECTED_BG_HOVER_COLOR)
            imgui.push_style_color(imgui.Col_.header_active, _SELECTED_BG_COLOR)
            pushed_header = True

        clicked, _ = imgui.selectable(f"{label}###rec_{handle}_{item.form_id}", is_selected)
        if imgui.is_item_hovered():
            identity = self._record_identity(handle, item.form_id)
            imgui.set_tooltip(f"Containing plugin: {plugin.plugin_name if plugin else ''}\n{sig_label} ({item.signature})\n{editor_id}\nFile FormID: {item.form_id:08X}\nOwner: {identity.owner if identity else 'Unknown'}\nSession FormID: {identity.session_id if identity else 'Unknown'}\n{_ROLE_LABELS.get(role, 'No known override')}")

        if pushed_header:
            imgui.pop_style_color(3)
        if pushed_text:
            imgui.pop_style_color()
        if clicked:
            self._show_overview = False
            self._select_record(handle, item)
        self._draw_record_context_menu(handle, item)

    def _draw_record_context_menu(self, source_handle: int, item) -> None:
        """xEdit-style right-click menu for a record in the nav tree.

        Lets the user copy the record into any other loaded plugin as either
        an override (preserves FormID; adds source plugin as master) or a
        new record (allocates a fresh FormID in the target's own slot).
        Each variant has a "deep" flavor that pulls in outbound references.
        """
        popup_id = f"##rec_ctx_{source_handle}_{item.form_id}"
        if not imgui.begin_popup_context_item(popup_id):
            return
        try:
            sig = item.signature
            edid = item.editor_id or ""
            imgui.text_disabled(f"{sig}  {edid}  [{item.form_id:08X}]")
            imgui.separator()
            if imgui.menu_item("Check for Errors", "", False, True)[0]:
                self.run_validation(handle=source_handle)
            if imgui.begin_menu("Export", True):
                try:
                    if imgui.menu_item("YAML", "", False, True)[0]:
                        self._export_record_text(source_handle, item, "yaml")
                    if imgui.menu_item("JSON", "", False, True)[0]:
                        self._export_record_text(source_handle, item, "json")
                finally:
                    imgui.end_menu()
            imgui.separator()

            self._draw_copy_submenu(
                "Copy as override into...",
                source_handle, item.form_id, as_new=False, deep=False,
            )
            self._draw_copy_submenu(
                "Deep copy as override into...",
                source_handle, item.form_id, as_new=False, deep=True,
            )
            imgui.separator()
            self._draw_copy_submenu(
                "Copy as new record into...",
                source_handle, item.form_id, as_new=True, deep=False,
            )
            self._draw_copy_submenu(
                "Deep copy as new record into...",
                source_handle, item.form_id, as_new=True, deep=True,
            )
            imgui.separator()
            if imgui.menu_item("Change FormID...", "", False, True)[0]:
                self._open_change_form_id_popup(source_handle, item.form_id)
        finally:
            imgui.end_popup()

    def _draw_copy_submenu(
        self,
        label: str,
        source_handle: int,
        source_form_id: int,
        *,
        as_new: bool,
        deep: bool,
    ) -> None:
        targets = self._copy_targets(source_handle, as_new=as_new)
        if not imgui.begin_menu(label, True):
            return
        try:
            if not targets:
                imgui.text_disabled("(no eligible plugins — load one or create a new patch)")
            for plugin in targets:
                tag = []
                if plugin.handle == (self.session.active.handle if self.session.active else None):
                    tag.append("active")
                if plugin.handle == self.session._patch_handle:
                    tag.append("patch")
                if plugin.is_master:
                    tag.append("master")
                suffix = f"  [{', '.join(tag)}]" if tag else ""
                if imgui.menu_item(
                    f"{plugin.plugin_name}{suffix}##copy_{plugin.handle}",
                    "", False, True,
                )[0]:
                    self._do_copy(
                        source_handle, source_form_id, plugin.handle,
                        as_new=as_new, deep=deep,
                    )
            imgui.separator()
            if imgui.menu_item("New plugin...", "", False, True)[0]:
                self._pending_copy = (source_handle, source_form_id, as_new, deep)
                self.open_new_patch_popup()
        finally:
            imgui.end_menu()

    def open_copy_popup(self) -> None:
        if not self._busy and self.selection.record is not None:
            self._copy_popup_requested = True

    def _draw_copy_popup(self) -> None:
        if self._copy_popup_requested:
            imgui.open_popup("Copy as override##esp_copy")
            self._copy_popup_requested = False
        prepare_dialog(500, 280)
        opened, _ = imgui.begin_popup_modal("Copy as override##esp_copy", None,
                                           imgui.WindowFlags_.always_auto_resize)
        if not opened:
            return
        try:
            sel = self.selection
            if sel.record is None:
                imgui.close_current_popup()
                return
            heading("Choose a destination plugin")
            imgui.text_wrapped("The source record stays unchanged. The destination receives an override.")
            for plugin in self._copy_targets(sel.plugin_handle, as_new=False):
                if imgui.button(f"{plugin.plugin_name}##copy_target_{plugin.handle}"):
                    self._do_copy(sel.plugin_handle, sel.record.form_id, plugin.handle, as_new=False, deep=False)
                    imgui.close_current_popup()
            if imgui.button("New patch plugin..."):
                self._pending_copy = (sel.plugin_handle, sel.record.form_id, False, False)
                imgui.close_current_popup()
                self.open_new_patch_popup()
            imgui.same_line()
            if imgui.button("Cancel"):
                imgui.close_current_popup()
        finally:
            imgui.end_popup()

    def _copy_targets(self, source_handle: int, *, as_new: bool):
        """Loaded plugins eligible to receive a copy from `source_handle`.

        For copy-as-override: target must load *after* the source (xEdit
        rule — a master can't load later than its dependent).
        For copy-as-new: any non-master loaded plugin works.
        """
        source_plugin = self.session.get_by_handle(source_handle)
        if source_plugin is None:
            return []
        out = []
        for plugin in self.session.plugins:
            if plugin.handle == source_handle:
                continue
            if plugin.game != source_plugin.game:
                continue
            if plugin.is_master:
                continue
            if not as_new and plugin.load_order_index <= source_plugin.load_order_index:
                continue
            out.append(plugin)
        return out

    def _do_copy(
        self,
        source_handle: int,
        source_form_id: int,
        target_handle: int,
        *,
        as_new: bool,
        deep: bool,
    ) -> None:
        if self._busy:
            return
        session = self.session
        op_name = "Copy as new record" if as_new else "Copy as override"
        if deep:
            op_name = "Deep " + op_name.lower()

        def _worker():
            fn = copy_as_new if as_new else copy_as_override
            return fn(session, source_form_id, deep=deep, target_handle=target_handle, source_handle=source_handle)

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"{op_name} failed: {result}")
                return
            if not result:
                self._show_error(f"{op_name}: no records copied. Check the destination's master order.")
                return
            self._dirty_handles.add(target_handle)
            self._invalidate_plugin_tree(target_handle)
            session.invalidate_form_id(source_form_id)
            for fid in result or []:
                session.invalidate_form_id(int(fid))
            target_plugin = session.get_by_handle(target_handle)
            target_name = target_plugin.plugin_name if target_plugin else "?"
            _log.info("%s: %d record(s) → %s", op_name, len(result or []), target_name)

        self._start_background(f"{op_name}...", _worker, _on_done)

    # -- plugin-level context menu (right-click on a plugin in the tree) --

    def _draw_plugin_context_menu(self, plugin) -> None:
        from creation_lib.esp.editor import (
            clean_masters,
            is_light,
            is_master,
            is_medium,
            remove_itm_records,
            set_light,
            set_master,
            set_medium,
            sort_masters,
            undelete_and_disable_refs,
        )

        popup_id = f"##plugin_ctx_{plugin.handle}"
        if not imgui.begin_popup_context_item(popup_id):
            return
        try:
            imgui.text_disabled(f"{plugin.plugin_name}")
            imgui.separator()
            if imgui.menu_item("Set Active", "", False, True)[0]:
                self.session.set_active(plugin.handle)
            if imgui.menu_item("Check for Errors", "", False, True)[0]:
                self.run_validation(handle=plugin.handle)
            if imgui.begin_menu("Export", True):
                try:
                    if imgui.menu_item("YAML", "", False, True)[0]:
                        self._export_plugin_text(plugin, "yaml")
                    if imgui.menu_item("JSON", "", False, True)[0]:
                        self._export_plugin_text(plugin, "json")
                finally:
                    imgui.end_menu()

            imgui.separator()
            if imgui.menu_item("Sort Masters", "", False, True)[0]:
                self._run_master_op(plugin.handle, "Sort Masters", sort_masters)
            if imgui.menu_item("Clean Masters (Remove unused)", "", False, True)[0]:
                self._run_master_op(plugin.handle, "Clean Masters", clean_masters)
            if imgui.menu_item("Add Masters...", "", False, True)[0]:
                self._open_add_masters_popup(plugin.handle)

            imgui.separator()
            if imgui.menu_item("Remove Identical to Master", "", False, True)[0]:
                self._run_cleanup_op(plugin.handle, "Remove ITM", remove_itm_records)
            if imgui.menu_item("Undelete and Disable References", "", False, True)[0]:
                self._run_cleanup_op(plugin.handle, "Undelete & Disable", undelete_and_disable_refs)

            imgui.separator()
            if imgui.menu_item("Compact FormIDs for ESL", "", False, True)[0]:
                self._run_compact_for_esl(plugin.handle)
            if imgui.menu_item("Renumber FormIDs from...", "", False, True)[0]:
                self._open_renumber_popup(plugin.handle)
            if imgui.menu_item("Inject into master...", "", False, True)[0]:
                self._open_inject_popup(plugin.handle)

            imgui.separator()
            light_on = is_light(plugin.handle)
            medium_on = is_medium(plugin.handle)
            master_on = is_master(plugin.handle)
            if imgui.menu_item("Light (ESL) Flag", "", light_on, True)[0]:
                self._toggle_flag(plugin.handle, set_light, not light_on, "Light")
            if plugin.game == "starfield":
                if imgui.menu_item("Medium Flag", "", medium_on, True)[0]:
                    self._toggle_flag(plugin.handle, set_medium, not medium_on, "Medium")
            if imgui.menu_item("Master (ESM) Flag", "", master_on, True)[0]:
                self._toggle_flag(plugin.handle, set_master, not master_on, "Master")
        finally:
            imgui.end_popup()

    def _run_master_op(self, handle: int, op_name: str, fn) -> None:
        if self._busy:
            return
        session = self.session

        def _worker():
            return fn(handle, session=session)

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"{op_name} failed: {result}")
                return
            self._dirty_handles.add(handle)
            self._invalidate_plugin_tree(handle)
            if isinstance(result, list) and result:
                _log.info("%s: dropped %d master(s): %s", op_name, len(result), ", ".join(result))
            else:
                _log.info("%s done", op_name)

        self._start_background(f"{op_name}...", _worker, _on_done)

    def _run_cleanup_op(self, handle: int, op_name: str, fn) -> None:
        """Cleanup ops (remove_itm_records, undelete_and_disable_refs) take
        `handles=[...]` rather than a single handle and return the list of
        FormIDs they touched."""
        if self._busy:
            return
        session = self.session

        def _worker():
            return fn(session, handles=[handle])

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"{op_name} failed: {result}")
                return
            if result:
                self._dirty_handles.add(handle)
                self._invalidate_plugin_tree(handle)
            _log.info("%s: touched %d record(s)", op_name, len(result or []))

        self._start_background(f"{op_name}...", _worker, _on_done)

    def _open_change_form_id_popup(self, source_handle: int, source_form_id: int) -> None:
        self._change_fid_source = (source_handle, source_form_id)
        self._change_fid_new_text = f"{source_form_id:08X}"
        imgui.open_popup("Change FormID##change_fid_modal")

    def _draw_change_form_id_popup(self) -> None:
        imgui.set_next_window_size(imgui.ImVec2(380, 0))
        flags = imgui.WindowFlags_.always_auto_resize
        modal_open, _ = imgui.begin_popup_modal("Change FormID##change_fid_modal", None, flags)
        if not modal_open:
            return
        try:
            src = getattr(self, "_change_fid_source", None)
            if src is None:
                imgui.text_disabled("No source record.")
                if imgui.button("Close", imgui.ImVec2(80, 0)):
                    imgui.close_current_popup()
                return
            _, source_fid = src
            imgui.text(f"Old file FormID: {source_fid:08X}")
            imgui.text("New FormID (hex):")
            _, self._change_fid_new_text = imgui.input_text(
                "##change_fid_new", self._change_fid_new_text,
            )
            imgui.separator()
            if imgui.button("Change", imgui.ImVec2(120, 0)):
                self._execute_change_form_id(source_fid, self._change_fid_new_text.strip())
                imgui.close_current_popup()
            imgui.same_line()
            if imgui.button("Cancel", imgui.ImVec2(80, 0)):
                imgui.close_current_popup()
        finally:
            imgui.end_popup()

    def _execute_change_form_id(self, old_fid: int, new_text: str) -> None:
        try:
            new_fid = int(new_text, 16) if new_text.lower().startswith("0x") else int(new_text)
        except ValueError:
            self._show_error(f"Invalid FormID: {new_text!r}")
            return
        from creation_lib.esp.editor import change_form_id
        session = self.session

        def _worker():
            return change_form_id(session, old_fid, new_fid)

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Change FormID failed: {result}")
                return
            located = session.resolve_form_id(new_fid)
            if located is not None:
                handle, _ = located
                self._dirty_handles.add(handle)
                self._invalidate_plugin_tree(handle)
            _log.info("Change FormID 0x%08X → 0x%08X: %d record(s)", old_fid, new_fid, result or 0)

        self._start_background("Change FormID...", _worker, _on_done)

    def _run_compact_for_esl(self, handle: int) -> None:
        if self._busy:
            return
        from creation_lib.esp.editor import compact_for_esl
        session = self.session

        def _worker():
            return compact_for_esl(session, handle)

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Compact FormIDs for ESL failed: {result}")
                return
            self._dirty_handles.add(handle)
            self._invalidate_plugin_tree(handle)
            _log.info("Compact FormIDs for ESL: rewrote %d record(s)", result or 0)

        self._start_background("Compact FormIDs for ESL...", _worker, _on_done)

    def _open_renumber_popup(self, handle: int) -> None:
        self._renumber_target = handle
        self._renumber_base_text = "00000800"
        imgui.open_popup("Renumber FormIDs from##renumber_modal")

    def _draw_renumber_popup(self) -> None:
        imgui.set_next_window_size(imgui.ImVec2(380, 0))
        flags = imgui.WindowFlags_.always_auto_resize
        modal_open, _ = imgui.begin_popup_modal("Renumber FormIDs from##renumber_modal", None, flags)
        if not modal_open:
            return
        try:
            handle = getattr(self, "_renumber_target", None)
            target = self.session.get_by_handle(handle) if handle else None
            if target is None:
                imgui.text_disabled("No target plugin.")
                if imgui.button("Close", imgui.ImVec2(80, 0)):
                    imgui.close_current_popup()
                return
            imgui.text(f"Renumber records of {target.plugin_name}")
            imgui.text("Base object_id (hex):")
            _, self._renumber_base_text = imgui.input_text(
                "##renumber_base", self._renumber_base_text,
            )
            imgui.separator()
            if imgui.button("Renumber", imgui.ImVec2(120, 0)):
                self._execute_renumber(target.handle, self._renumber_base_text.strip())
                imgui.close_current_popup()
            imgui.same_line()
            if imgui.button("Cancel", imgui.ImVec2(80, 0)):
                imgui.close_current_popup()
        finally:
            imgui.end_popup()

    def _execute_renumber(self, handle: int, base_text: str) -> None:
        try:
            base = int(base_text, 16) if base_text.lower().startswith("0x") else int(base_text)
        except ValueError:
            self._show_error(f"Invalid base object_id: {base_text!r}")
            return
        from creation_lib.esp.editor import renumber_form_ids_from
        session = self.session

        def _worker():
            return renumber_form_ids_from(session, handle, base)

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Renumber FormIDs failed: {result}")
                return
            self._dirty_handles.add(handle)
            self._invalidate_plugin_tree(handle)
            _log.info("Renumber FormIDs from 0x%X: rewrote %d record(s)", base, result or 0)

        self._start_background(f"Renumber FormIDs from {base:08X}...", _worker, _on_done)

    def _open_inject_popup(self, handle: int) -> None:
        self._inject_source = handle
        self._inject_target = None
        imgui.open_popup("Inject into master##inject_modal")

    def _draw_inject_popup(self) -> None:
        imgui.set_next_window_size(imgui.ImVec2(420, 0))
        flags = imgui.WindowFlags_.always_auto_resize
        modal_open, _ = imgui.begin_popup_modal("Inject into master##inject_modal", None, flags)
        if not modal_open:
            return
        try:
            source_handle = getattr(self, "_inject_source", None)
            source = self.session.get_by_handle(source_handle) if source_handle else None
            if source is None:
                imgui.text_disabled("No source plugin.")
                if imgui.button("Close", imgui.ImVec2(80, 0)):
                    imgui.close_current_popup()
                return
            imgui.text(f"Inject records from {source.plugin_name} into:")
            imgui.separator()
            for plugin in self.session.plugins:
                if plugin.handle == source.handle:
                    continue
                if plugin.load_order_index >= source.load_order_index:
                    continue  # target must load before source
                selected = self._inject_target == plugin.handle
                if imgui.radio_button(f"{plugin.plugin_name}##inj_{plugin.handle}", selected):
                    self._inject_target = plugin.handle
            imgui.separator()
            target_set = self._inject_target is not None
            if not target_set:
                imgui.begin_disabled()
            if imgui.button("Inject", imgui.ImVec2(120, 0)):
                self._execute_inject(source.handle, self._inject_target)
                imgui.close_current_popup()
            if not target_set:
                imgui.end_disabled()
            imgui.same_line()
            if imgui.button("Cancel", imgui.ImVec2(80, 0)):
                imgui.close_current_popup()
        finally:
            imgui.end_popup()

    def _execute_inject(self, source_handle: int, target_handle: int) -> None:
        from creation_lib.esp.editor import inject_into_master
        session = self.session

        def _worker():
            return inject_into_master(session, source_handle, target_handle)

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Inject into master failed: {result}")
                return
            self._dirty_handles.add(source_handle)
            self._dirty_handles.add(target_handle)
            self._invalidate_plugin_tree(source_handle)
            self._invalidate_plugin_tree(target_handle)
            _log.info("Inject into master: rewrote %d record(s)", result or 0)

        self._start_background("Inject into master...", _worker, _on_done)

    def run_apply_script(self) -> None:
        self._show_error("Apply Script is disabled until scripts run through native record APIs.")

    def run_build_reachable(self) -> None:
        """Compute the set of FormIDs reachable from engine entry points.
        Caches the orphan list so the UI can highlight unreachable records."""
        if self._busy:
            return
        if not self.session.plugins:
            return
        from creation_lib.esp.editor import build_reachable_set, find_orphan_records
        session = self.session
        active = self.session.active
        target_handle = active.handle if active else None

        def _worker():
            reachable = build_reachable_set(session)
            orphans = find_orphan_records(session, handle=target_handle) if target_handle else []
            return (reachable, orphans)

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Build Reachable Info failed: {result}")
                return
            reachable, orphans = result
            self._reachable_set = reachable
            self._orphan_form_ids = set(orphans)
            _log.info(
                "Build Reachable Info: %d reachable, %d orphan(s) in active plugin",
                len(reachable), len(orphans),
            )

        self._start_background("Building reachable info...", _worker, _on_done)

    def run_build_ref_info(self) -> None:
        """Warm the native back-reference index across every loaded plugin."""
        if self._busy:
            return
        if not self.session.plugins:
            return
        from creation_lib.esp.editor import build_reference_index
        session = self.session

        def _worker():
            return build_reference_index(session)

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Build Reference Info failed: {result}")
                return
            _log.info("Build Reference Info: %d record(s) indexed", result or 0)

        self._start_background("Building reference info...", _worker, _on_done)

    def _toggle_flag(self, handle: int, setter, on: bool, label: str) -> None:
        if self._busy:
            return
        try:
            setter(handle, on)
        except Exception as exc:
            self._show_error(f"Toggle {label} failed: {exc}")
            return
        self._dirty_handles.add(handle)
        self._invalidate_plugin_tree(handle)
        _log.info("%s flag set to %s", label, on)

    def _open_add_masters_popup(self, handle: int) -> None:
        self._add_masters_target = handle
        self._add_masters_selected = set()
        imgui.open_popup("Add Masters##add_masters_modal")

    def _draw_add_masters_popup(self) -> None:
        imgui.set_next_window_size(imgui.ImVec2(420, 0))
        flags = imgui.WindowFlags_.always_auto_resize
        modal_open, _ = imgui.begin_popup_modal("Add Masters##add_masters_modal", None, flags)
        if not modal_open:
            return
        try:
            target_handle = getattr(self, "_add_masters_target", None)
            target = self.session.get_by_handle(target_handle) if target_handle else None
            if target is None:
                imgui.text_disabled("No target plugin.")
                if imgui.button("Close", imgui.ImVec2(80, 0)):
                    imgui.close_current_popup()
                return
            imgui.text(f"Add masters to: {target.plugin_name}")
            imgui.separator()
            existing = {m.lower() for m in (plugin_handle_get(target.handle, "masters") or [])}
            target_idx = target.load_order_index
            any_eligible = False
            for plugin in self.session.plugins:
                if plugin.handle == target.handle:
                    continue
                if plugin.plugin_name.lower() in existing:
                    continue
                if plugin.load_order_index >= target_idx:
                    continue
                any_eligible = True
                checked = plugin.plugin_name in self._add_masters_selected
                _, new_checked = imgui.checkbox(
                    f"{plugin.plugin_name}##amaster_{plugin.handle}", checked,
                )
                if new_checked and plugin.plugin_name not in self._add_masters_selected:
                    self._add_masters_selected.add(plugin.plugin_name)
                elif not new_checked and plugin.plugin_name in self._add_masters_selected:
                    self._add_masters_selected.discard(plugin.plugin_name)
            if not any_eligible:
                imgui.text_disabled("(no eligible plugins — all loaded plugins are already masters or load after this one)")
            imgui.separator()
            if imgui.button("Add", imgui.ImVec2(100, 0)):
                self._execute_add_masters(target.handle, list(self._add_masters_selected))
                imgui.close_current_popup()
            imgui.same_line()
            if imgui.button("Cancel", imgui.ImVec2(80, 0)):
                imgui.close_current_popup()
        finally:
            imgui.end_popup()

    def _execute_add_masters(self, handle: int, names: list[str]) -> None:
        if not names:
            return
        from creation_lib.esp.editor import add_masters as _add_masters
        try:
            added = _add_masters(handle, names, session=self.session)
        except Exception as exc:
            self._show_error(f"Add Masters failed: {exc}")
            return
        if added:
            self._dirty_handles.add(handle)
            self._invalidate_plugin_tree(handle)
            _log.info("Add Masters: added %d (%s)", len(added), ", ".join(added))

    def _plugin_tooltip(self, plugin) -> str:
        total_records = self._plugin_record_counts.get(plugin.handle, 0)
        conflicts = self._plugin_conflict_count(plugin.handle) if self._conflict_scan is not None else "Not scanned"
        wins = self._plugin_winner_count(plugin.handle) if self._conflict_scan is not None else "Not scanned"
        if self._conflict_scan is not None and not self._scan_complete:
            conflicts, wins = f"{conflicts} (partial analysis)", f"{wins} (partial analysis)"
        lines = [
            f"{plugin.plugin_name}",
            f"Position: {plugin.load_order_index + 1} (one based)",
            f"Session slot: {self._session_slots().get(plugin.handle).label if plugin.handle in self._session_slots() else 'unresolved'} (zero based)",
            f"Game:      {plugin.game}",
            f"Path:      {plugin.path}",
            f"Records:   {total_records}",
            f"Conflicts: {conflicts}",
            f"Winning:   {wins}",
        ]
        if plugin.is_master:
            lines.append("(master)")
        lines.append("Slots follow this editor's order, not proof of the game's current order.")
        return "\n".join(lines)

    def _record_role(self, handle: int, form_id: int) -> str:
        """Use scoped comparisons for colors; mark inherited unscanned IDs.

        A partial form cannot replace a full winner. File IDs use each file's
        own master table, so the containing handle is part of every lookup.
        """
        scan = self._conflict_scan
        report = scan.report_for(handle, form_id) if scan is not None else None
        if report is None or not report.chain:
            masters = self._plugin_masters.get(handle, ())
            if ((form_id >> 24) & 255) < len(masters):
                return "uncompared"
            return "only"
        entry = next((e for e in report.chain if e.plugin_handle == handle and e.form_id == form_id), None)
        if entry is not None and entry.is_partial:
            return "partial"
        if report.status != ConflictStatus.CONFLICT:
            return "override"
        if report.winner.plugin_handle == handle:
            return "winner"
        return "loser"

    def _plugin_conflict_count(self, handle: int) -> int:
        scan = self._conflict_scan
        if scan is None:
            return 0
        return self._scan_counts()[0].get(handle, 0)

    def _plugin_winner_count(self, handle: int) -> int:
        scan = self._conflict_scan
        if scan is None:
            return 0
        return self._scan_counts()[1].get(handle, 0)

    def _scan_counts(self):
        scan = self._conflict_scan
        if self._scan_counts_cache is not None and self._scan_counts_cache[0] is scan:
            return self._scan_counts_cache[1]
        conflicts, winners, reports = {}, {}, {}
        for report in scan.reports if scan is not None else []:
            for entry in report.chain:
                reports[entry.plugin_handle] = reports.get(entry.plugin_handle, 0) + 1
                if report.status == ConflictStatus.CONFLICT:
                    conflicts[entry.plugin_handle] = conflicts.get(entry.plugin_handle, 0) + 1
            if report.status == ConflictStatus.CONFLICT and report.chain:
                winners[report.winner.plugin_handle] = winners.get(report.winner.plugin_handle, 0) + 1
        counts = conflicts, winners, reports
        self._scan_counts_cache = (scan, counts)
        return counts

    def _record_matches_filter(self, record) -> bool:
        sig_filter = self._filter_signature.strip().upper()
        if sig_filter and record.signature.upper() != sig_filter:
            return False
        return matches(record, self._filter_text.strip().casefold(), sig_filter, self._search_field,
                       {record.signature: self._record_label(record.signature)})

    def _select_record(self, handle: int, record) -> None:
        if (self.selection.plugin_handle == handle and self.selection.record is not None
                and self.selection.record.form_id == record.form_id
                and (self.selection.fields is not None or self._selection_future is not None)):
            return
        plugin = self.session.get_by_handle(handle)
        if plugin is None:
            return
        self.cancel_selection()
        self.session.set_active(handle)
        self.selection = _Selection(
            plugin_handle=handle,
            record=record,
            fields=None,
        )
        self._raw_record_text = None
        self._reference_labels.clear()
        self._request_selection()

    def cancel_selection(self) -> None:
        self._selection_generation += 1
        if self._selection_future is not None:
            self._selection_future.cancel()
        self._selection_future = None
        self._selection_job = None
        if self._reference_future is not None:
            self._reference_future.cancel()
        self._reference_future = None
        self._reference_job = None
        self._references = None
        self._array_pages.clear()
        if self.selection.record is not None and self.selection.fields is None:
            self.selection.error = "Loading cancelled. Select the record to try again."

    def _request_selection(self) -> None:
        sel = self.selection
        if sel.record is None:
            return
        plugin = self.session.get_by_handle(sel.plugin_handle)
        if plugin is None:
            return
        key = (plugin.handle, sel.record.form_id, self._view_stamp())
        cached = self._record_cache.get(key)
        if cached is not None:
            self._record_cache.move_to_end(key)
            self._install_selection(*cached)
            return
        self._selection_job = (self._selection_generation, key)
        plugins = deepcopy(self.session.plugins)
        def work():
            documents = load_comparison(plugins, deepcopy(plugin), sel.record)
            return documents, comparison_rows(documents)
        self._selection_future = self._detail_executor.submit(work)

    def _install_selection(self, documents, rows) -> None:
        sel = self.selection
        document = next(d for d in documents if d.handle == sel.plugin_handle)
        sel.document = document
        sel.documents = documents
        sel.comparison = rows
        sel.differences = [row for row in rows if len(set(row[2])) > 1]
        sel.fields = document.fields
        sel.error = None
        for doc in documents:
            self._plugin_masters[doc.handle] = doc.masters
            self._plugin_flags[doc.handle] = doc.plugin_flags

    def _poll_selection(self) -> None:
        if self._reference_future is not None and self._reference_future.done():
            future = self._reference_future
            self._reference_future = None
            if self._reference_job == (self._selection_generation, self._view_stamp()):
                try:
                    self._references = future.result()
                except Exception as exc:
                    self._references = []
                    self._show_error(f"Unable to load references: {exc}")
        future = self._selection_future
        if future is None or not future.done():
            return
        generation, key = self._selection_job
        self._selection_future = None
        self._selection_job = None
        if generation != self._selection_generation or key[2] != self._view_stamp():
            self._request_selection()
            return
        try:
            result = future.result()
            self._record_cache[key] = result
            while len(self._record_cache) > 24:
                self._record_cache.popitem(last=False)
            self._install_selection(*result)
        except Exception as exc:
            self.selection.error = str(exc)
            self._show_error(f"Unable to load record: {exc}")

    # -- record editor ----------------------------------------------------

    def _overview_stamp(self):
        active = self.session.active
        return (id(self._conflict_scan), active.handle if active else None, self._overview_scope_active,
                self._overview_status, self._overview_query.strip().casefold(), self._record_sort, self._sort_descending)

    def _poll_overview(self):
        pending = self._overview_future
        if pending is None or not pending[1].done():
            return
        self._overview_future = None
        stamp, future = pending
        if stamp != self._overview_stamp():
            return
        try:
            reports, totals = future.result()
            self._overview_cache = (stamp, reports)
            self._overview_totals = (self._conflict_scan, totals)
        except Exception as exc:
            self._overview_error = (stamp, str(exc))
            self._show_error(f"Override overview failed: {exc}")

    def _overview_reports(self):
        scan = self._conflict_scan
        stamp = self._overview_stamp()
        if self._overview_cache is not None and self._overview_cache[0] == stamp:
            return self._overview_cache[1]
        if self._overview_error is not None and self._overview_error[0] == stamp:
            return []
        pending = self._overview_future
        if pending is not None and pending[0] == stamp:
            return []
        if pending is not None:
            pending[1].cancel()
            self._overview_future = None
        totals = self._overview_totals[1] if self._overview_totals and self._overview_totals[0] is scan else None
        def work():
            return self._build_overview(scan, stamp, totals)
        if scan is not None and len(scan) >= 4096:
            self._overview_future = (stamp, self._executor.submit(work))
            return []
        reports, totals = work()
        self._overview_cache = (stamp, reports)
        self._overview_totals = (scan, totals)
        return reports

    @staticmethod
    def _build_overview(scan, stamp, totals):
        _, active_handle, scope_active, status, query, order, descending = stamp
        reports = []
        query = query.removeprefix("0x")
        all_reports = scan.reports if scan else []
        if totals is None:
            totals = (len(all_reports), sum(r.status == ConflictStatus.CONFLICT for r in all_reports))
        for report in all_reports:
            involved = active_handle is not None and any(entry.plugin_handle == active_handle for entry in report.chain)
            if scope_active and not involved:
                continue
            if status == "Conflicts" and report.status != ConflictStatus.CONFLICT:
                continue
            if status == "Identical overrides" and report.status != ConflictStatus.OVERRIDE:
                continue
            if status == "Active wins" and (not involved or report.winner.plugin_handle != active_handle):
                continue
            if status == "Active loses" and (not involved or report.winner.plugin_handle == active_handle):
                continue
            if status == "Partial forms" and not any(entry.is_partial for entry in report.chain):
                continue
            text = f"{report.editor_id or ''} {report.form_id:08X} {report.signature} " + " ".join(entry.plugin_name for entry in report.chain)
            if query and query not in text.casefold():
                continue
            reports.append(report)
        def key(report):
            stable = (report.signature, report.winner.form_id, report.master_entry.plugin_name.casefold(), report.winner.plugin_name.casefold())
            if order == "Editor ID":
                return ((report.editor_id or '').casefold(), *stable)
            if order == "Record type":
                return stable
            return (report.form_id & 0xFFFFFF, *stable)
        reports.sort(key=key, reverse=descending)
        return reports, totals

    def _draw_override_overview(self):
        heading("Overrides overview")
        if imgui.button("Back to selected record##overview_back"):
            self._show_overview = False
            return
        imgui.same_line()
        imgui.begin_disabled(self._busy or self._analysis_future is not None)
        if imgui.button("Analyze load order##overview_analyze"):
            self.start_override_analysis()
        imgui.end_disabled()
        if self._analysis_future is not None:
            imgui.same_line()
            if imgui.button("Cancel analysis##overview_cancel"):
                self.cancel_override_analysis()
            imgui.text_wrapped(f"{self._analysis_label} | {self._analysis_completed}/{self._analysis_total or '?'} batches")
            if self._analysis_total:
                imgui.progress_bar(self._analysis_completed / self._analysis_total, imgui.ImVec2(-1, 0))
        elif self._analysis_label:
            imgui.text_wrapped(self._analysis_label)
        scan = self._conflict_scan
        if scan is None:
            imgui.text_wrapped("Analyze the load order to see all override chains and conflict winners. Gold [O?] rows are known overrides whose payloads have not been compared.")
            return
        coverage = 'Complete' if self._scan_complete else 'Partial analysis'
        totals = self._overview_totals[1] if self._overview_totals and self._overview_totals[0] is scan else None
        if totals:
            count, conflicts = totals
            imgui.text_wrapped(f"{count} override chains | {conflicts} conflicts | {count - conflicts} identical | {coverage}")
        else:
            imgui.text_wrapped(f"{len(scan)} override chains | Counting conflicts... | {coverage}")
        _, self._overview_scope_active = imgui.checkbox("Active plugin only##overview_scope", self._overview_scope_active)
        status_options = ("All overrides", "Conflicts", "Identical overrides", "Active wins", "Active loses", "Partial forms")
        self._overview_status = self._choice("Show", "overview_status", status_options, self._overview_status)
        imgui.set_next_item_width(-1)
        _, self._overview_query = imgui.input_text_with_hint("##overview_query", "Search Editor ID, file FormID, type or plugin", self._overview_query)
        imgui.text_disabled(f"Display order: {self._record_sort}, {'descending' if self._sort_descending else 'ascending'} | plugin priority follows session load order")
        visible = self._overview_reports()
        if self._overview_future is not None:
            imgui.text_disabled("Filtering and sorting override chains...")
            return
        if self._overview_error is not None and self._overview_error[0] == self._overview_stamp():
            imgui.text_wrapped(f"Overview failed: {self._overview_error[1]}")
            if imgui.small_button("Retry overview"):
                self._overview_error = None
            return
        if not visible:
            imgui.text_disabled("No matching override chains.")
            return
        flags = imgui.TableFlags_.row_bg | imgui.TableFlags_.borders_inner_h | imgui.TableFlags_.scroll_y | imgui.TableFlags_.resizable
        if not imgui.begin_table("##overview_rows", 7, flags, imgui.ImVec2(-1, -1)):
            return
        try:
            for name in ("Status", "Type", "Editor ID", "Session FormID", "Winner", "Versions"):
                imgui.table_setup_column(name, imgui.TableColumnFlags_.width_stretch)
            action_width = imgui.calc_text_size("WinnerSource").x + 4 * imgui.get_style().frame_padding.x + imgui.get_style().item_spacing.x + 12
            imgui.table_setup_column("Inspect", imgui.TableColumnFlags_.width_fixed, action_width)
            imgui.table_setup_scroll_freeze(0, 1)
            imgui.table_headers_row()
            clipper = imgui.ListClipper()
            clipper.begin(len(visible))
            while clipper.step():
                for index in range(clipper.display_start, clipper.display_end):
                    report = visible[index]
                    winner = report.winner
                    key = f"{winner.plugin_handle}_{winner.form_id}"
                    identity = self._record_identity(winner.plugin_handle, winner.form_id)
                    imgui.table_next_row()
                    imgui.table_set_column_index(0)
                    imgui.text_colored(self._conflict_row_color(report) or _ROLE_COLORS['override'],
                                       "Conflict" if report.status == ConflictStatus.CONFLICT else "Identical")
                    imgui.set_item_tooltip("Native payload comparison. Open the record's Overrides tab to inspect values; partial forms do not replace a full winner.")
                    imgui.table_set_column_index(1)
                    imgui.text(report.signature)
                    imgui.table_set_column_index(2)
                    imgui.text(report.editor_id or "<no Editor ID>")
                    imgui.table_set_column_index(3)
                    imgui.text(identity.session_id or "unresolved")
                    imgui.set_item_tooltip(f"Winner file FormID: {identity.file_id}\nOriginal owner: {identity.owner}")
                    imgui.table_set_column_index(4)
                    imgui.text(winner.plugin_name)
                    imgui.table_set_column_index(5)
                    imgui.text(str(len(report.chain)))
                    imgui.set_item_tooltip("\n".join(f"{e.plugin_name} {'(partial)' if e.is_partial else '(winner)' if e is winner else ''}" for e in report.chain))
                    imgui.table_set_column_index(6)
                    if imgui.small_button(f"Winner###overview_winner_{key}"):
                        self._navigate_report(report, winner)
                    imgui.same_line()
                    if imgui.small_button(f"Source###overview_source_{key}"):
                        self._navigate_report(report, report.master_entry)
            clipper.end()
        finally:
            imgui.end_table()

    def _navigate_report(self, report, entry):
        record = self._record_for_handle_form_id(entry.plugin_handle, entry.form_id)
        if record is not None and record.form_id == entry.form_id:
            self._show_overview = False
            self._select_record(entry.plugin_handle, record)

    def draw_record_view(self) -> None:
        if self._show_overview:
            self._draw_override_overview()
            return
        if self.selection.record is None:
            imgui.text_disabled("Select a record from the navigation tree.")
            return
        rec = self.selection.record
        plugin = self.session.get_by_handle(self.selection.plugin_handle)
        game = plugin.game if plugin else None
        sig_label = self._record_label(rec.signature, game)
        heading(
            f"{sig_label}  ({rec.signature})  {rec.form_id:08X}  "
            f"{rec.editor_id or ''}"
        )
        if self.selection.fields is None:
            if self.selection.error:
                imgui.text_wrapped(self.selection.error)
                if imgui.button("Retry record"):
                    self._select_record(self.selection.plugin_handle, rec)
            else:
                imgui.text_disabled("Loading record and overrides...")
                if imgui.button("Cancel loading"):
                    self.cancel_selection()
            return
        doc = self.selection.document
        docs = self.selection.documents
        identity = self._record_identity(doc.handle, rec.form_id)
        imgui.text_wrapped(f"File FormID: {identity.file_id} | Session FormID: {identity.session_id or 'unresolved'}")
        imgui.text_wrapped(f"Containing plugin: {doc.plugin_name} | Original owner: {identity.owner or 'unresolved'}")
        imgui.set_item_tooltip("File FormID uses the containing plugin's master table. Session FormID maps the original owner to this editor's load order. Import the game's load order to match engine IDs.")
        winner = winning_document(docs)
        imgui.text_disabled(f"Identity: {doc.form_key(rec.form_id)}")
        imgui.text_disabled(f"Editing {doc.plugin_name}  |  Winner: {winner.plugin_name}  |  {len(docs)} version(s)")
        if doc.handle != winner.handle and imgui.small_button("Go to winner"):
            self._select_record(winner.handle, winner.summary)
            return
        if self._latest_error:
            imgui.text_wrapped(f"Last error: {self._latest_error}")
            if imgui.small_button("Dismiss error"):
                self._latest_error = None
        imgui.separator()
        if not imgui.begin_tab_bar("##record_views"):
            return
        try:
            if imgui.begin_tab_item("Fields")[0]:
                self._draw_fields()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Overrides")[0]:
                self._draw_override_comparison()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Source JSON")[0]:
                if self._raw_record_text is None and imgui.button("Load source JSON"):
                    def done(result, is_error):
                        if is_error:
                            self._show_error(f"Unable to export record text: {result}")
                        elif self.selection.document is doc:
                            self._raw_record_text = str(result)
                    self._start_background("Exporting selected record...", lambda: plugin_handle_call(
                        doc.handle, "export_record_text", rec.form_id, "json"), done)
                if self._raw_record_text is not None:
                    imgui.input_text_multiline("##record_source", self._raw_record_text,
                        imgui.ImVec2(-1, -1), imgui.InputTextFlags_.read_only)
                imgui.end_tab_item()
        finally:
            imgui.end_tab_bar()

    def _draw_fields(self) -> None:
        imgui.set_next_item_width(-1)
        _, self._field_filter = imgui.input_text_with_hint("##field_filter", "Find a field or signature", self._field_filter)
        query = self._field_filter.strip().casefold()
        imgui.begin_disabled(self._busy)
        try:
            flags = (imgui.TableFlags_.row_bg | imgui.TableFlags_.borders_inner_h |
                     imgui.TableFlags_.resizable | imgui.TableFlags_.sizing_stretch_prop)
            if not imgui.begin_table("##record_fields", 2, flags):
                return
            imgui.table_setup_column("Field", imgui.TableColumnFlags_.width_fixed, 210)
            imgui.table_setup_column("Value", imgui.TableColumnFlags_.width_stretch)
            imgui.table_headers_row()
            for idx, fld in enumerate(self.selection.fields or []):
                members = " ".join(fs.authoring_label or fs.name for fs in (fld.spec.fields or ())) if fld.spec else ""
                if query and query not in f"{fld.name} {fld.signature} {fld.notes} {members}".casefold():
                    continue
                reason = self.selection.document.edit_errors[idx]
                imgui.table_next_row()
                imgui.table_set_column_index(0)
                imgui.text_wrapped(fld.name)
                imgui.text_disabled(f"{fld.signature} | {fld.kind.value}")
                if fld.notes and imgui.is_item_hovered():
                    imgui.set_tooltip(fld.notes)
                imgui.table_set_column_index(1)
                if reason:
                    imgui.text_wrapped(reason)
                complex_field = fld.kind in (UiFieldKind.STRUCT, UiFieldKind.ARRAY, UiFieldKind.BYTES)
                if complex_field:
                    default = imgui.TreeNodeFlags_.default_open if fld.kind == UiFieldKind.STRUCT and len(fld.components) <= 8 else 0
                    with expandable_section(f"Inspect {fld.name}##field_{idx}", default,
                            description=f"{len(fld.raw)} bytes") as expanded:
                        if expanded:
                            imgui.begin_disabled(bool(reason))
                            self._draw_field_widget(idx, fld)
                            imgui.end_disabled()
                else:
                    imgui.begin_disabled(bool(reason))
                    self._draw_field_widget(idx, fld)
                    imgui.end_disabled()
            imgui.end_table()
        finally:
            imgui.end_disabled()

    def _draw_override_comparison(self) -> None:
        docs = self.selection.documents
        _, self._differences_only = imgui.checkbox("Differences only", self._differences_only)
        for i, doc in enumerate(docs):
            if i:
                imgui.same_line()
            winner = winning_document(docs)
            tag = "Winner" if doc is winner else "Partial" if doc.is_partial else "Source" if i == 0 else "Override"
            if imgui.small_button(f"{tag}: {doc.plugin_name}##version_{doc.handle}"):
                self._select_record(doc.handle, doc.summary)
                return
        rows = self.selection.differences if self._differences_only else self.selection.comparison
        imgui.text_disabled(f"{len(rows)} fields | columns follow load order; winner: {winner.plugin_name}")
        if not rows:
            imgui.text_disabled("These versions have the same displayed values.")
            return
        flags = (imgui.TableFlags_.row_bg | imgui.TableFlags_.borders_inner_v |
                 imgui.TableFlags_.scroll_x | imgui.TableFlags_.scroll_y | imgui.TableFlags_.resizable)
        if imgui.begin_table("##override_values", len(docs) + 1, flags, imgui.ImVec2(-1, -1)):
            try:
                imgui.table_setup_scroll_freeze(1, 1)
                imgui.table_setup_column("Field", imgui.TableColumnFlags_.width_fixed, 220)
                for doc in docs:
                    imgui.table_setup_column(doc.plugin_name, imgui.TableColumnFlags_.width_fixed, 260)
                imgui.table_headers_row()
                clipper = imgui.ListClipper()
                clipper.begin(len(rows))
                while clipper.step():
                    for i in range(clipper.display_start, clipper.display_end):
                        _, label, values = rows[i]
                        imgui.table_next_row()
                        imgui.table_set_column_index(0)
                        imgui.text(label)
                        for col, value in enumerate(values):
                            imgui.table_set_column_index(col + 1)
                            text = "<missing>" if value is None else str(value)
                            if docs[col] is winner:
                                imgui.push_style_color(imgui.Col_.text, semantic_color("success"))
                            imgui.text(text[:160].replace("\n", " "))
                            if docs[col] is winner:
                                imgui.pop_style_color()
                            if imgui.is_item_hovered():
                                imgui.set_tooltip(text[:4096])
                clipper.end()
            finally:
                imgui.end_table()

    def _draw_field_widget(self, idx: int, fld: Field) -> None:
        widget_id = f"##fld_{idx}"
        # Render the value widget. For STRUCT/ARRAY fields the change-handler is
        # baked into the component widgets and the caller only needs to repack
        # the parent record after a sub-edit.
        def on_change():
            self._commit_field(idx, fld)
        self._draw_value_widget(fld, widget_id, on_change)

    def _draw_value_widget(self, fld: Field, widget_id: str, on_change) -> None:
        """Render a single Field's value widget. ``on_change`` is invoked whenever
        the caller should re-pack the parent subrecord after an edit (used for
        nested struct/array components)."""
        if fld.kind == UiFieldKind.INT:
            value = int(fld.value or 0)
            imgui.set_next_item_width(-1)
            changed, text = imgui.input_text(widget_id, str(value), flags=imgui.InputTextFlags_.chars_decimal)
            if changed and text not in ("", "+", "-"):
                try:
                    fld.value = int(text)
                    on_change()
                except ValueError:
                    pass
        elif fld.kind == UiFieldKind.FLOAT:
            value = float(fld.value or 0.0)
            imgui.set_next_item_width(-1)
            changed, new_val = imgui.input_double(widget_id, value, format="%.9g")
            if changed:
                fld.value = float(new_val)
                on_change()
        elif fld.kind == UiFieldKind.STRING:
            value = str(fld.value or "")
            imgui.set_next_item_width(-1)
            changed, new_val = imgui.input_text(widget_id, value)
            if changed:
                fld.value = new_val
                on_change()
        elif fld.kind == UiFieldKind.FORMID:
            self._draw_formid_widget(fld, widget_id, on_change)
        elif fld.kind == UiFieldKind.LSTRING:
            value = int(fld.value or 0)
            resolved = self.selection.document.localized_values.get(value, "")
            imgui.text(f"#{value} = {resolved!r}")
        elif fld.kind == UiFieldKind.ENUM:
            self._draw_enum_widget(fld, widget_id, on_change)
        elif fld.kind == UiFieldKind.FLAGSET:
            self._draw_flagset_widget(fld, widget_id, on_change)
        elif fld.kind == UiFieldKind.EMPTY:
            imgui.text_disabled("<empty marker>")
        elif fld.kind == UiFieldKind.BYTES:
            self._draw_bytes_widget_inplace(fld, widget_id, on_change)
        elif fld.kind == UiFieldKind.STRUCT:
            self._draw_struct_widget_components(fld, widget_id, on_change)
        elif fld.kind == UiFieldKind.ARRAY:
            self._draw_array_widget_rows(fld, widget_id, on_change)
        else:
            imgui.text_disabled(f"<{fld.kind.value}, {len(fld.raw)} bytes>")
            if len(fld.raw) <= 64:
                imgui.same_line()
                imgui.text_disabled(fld.raw.hex())

    def _draw_formid_widget(self, fld: Field, widget_id: str, on_change) -> None:
        value = int(fld.value or 0)
        imgui.text(f"{value:08X}")
        imgui.same_line()
        label = self._formid_label(value)
        if label:
            imgui.text_disabled(label)
        if fld.formlink_target:
            imgui.same_line()
            imgui.text_disabled(f"→ {fld.formlink_target}")
        imgui.same_line()
        imgui.set_next_item_width(120)
        changed, text = imgui.input_text(
            widget_id, f"{value:08X}"
        )
        if changed and text:
            try:
                parsed = int(text, 16)
                if not 0 <= parsed <= 0xFFFFFFFF:
                    raise ValueError("FormID is outside the 32-bit range")
            except ValueError:
                self._show_error("Enter a hexadecimal file FormID, optionally prefixed with 0x")
                return
            fld.value = parsed
            on_change()

    def _draw_enum_widget(self, fld: Field, widget_id: str, on_change) -> None:
        options = fld.enum_options
        if not options and fld.enum_def is not None:
            options = list(fld.enum_def.labels) or list(fld.enum_def.values)
        current = int(fld.value or 0)
        labels = [f"{n} ({v})" for v, n in options]
        current_index = next(
            (i for i, (v, _) in enumerate(options) if v == current), -1
        )
        if current_index < 0:
            labels = [f"<unknown {current}>"] + labels
            display_index = 0
        else:
            display_index = current_index
        imgui.set_next_item_width(-1)
        changed, new_index = imgui.combo(widget_id, display_index, labels)
        if changed:
            offset = 0 if current_index >= 0 else -1
            actual = new_index + offset
            if 0 <= actual < len(options):
                fld.value = int(options[actual][0])
                on_change()

    def _draw_flagset_widget(self, fld: Field, widget_id: str, on_change) -> None:
        options = fld.flag_options
        if not options and fld.enum_def is not None:
            options = list(fld.enum_def.labels) or list(fld.enum_def.values)
        current = int(fld.value or 0)
        new_value = current
        for bit, name in options:
            set_now = bool(current & bit)
            changed, set_now = imgui.checkbox(f"{name}{widget_id}_{bit}", set_now)
            if changed:
                new_value = (new_value | bit) if set_now else (new_value & ~bit)
        if new_value != current:
            fld.value = int(new_value)
            on_change()

    def _draw_bytes_widget_inplace(self, fld: Field, widget_id: str, on_change) -> None:
        raw = fld.raw if fld.raw else (fld.value if isinstance(fld.value, (bytes, bytearray)) else b"")
        hex_str = bytes(raw).hex()
        imgui.set_next_item_width(-1)
        changed, new_hex = imgui.input_text(
            widget_id, hex_str, flags=imgui.InputTextFlags_.chars_hexadecimal
        )
        if changed:
            cleaned = new_hex.strip().replace(" ", "")
            if len(cleaned) == len(hex_str) and len(cleaned) % 2 == 0:
                try:
                    new_bytes = bytes.fromhex(cleaned)
                except ValueError:
                    return
                fld.value = new_bytes
                fld.raw = new_bytes
                on_change()

    def _draw_struct_widget_components(self, fld: Field, widget_id: str, on_change) -> None:
        """Render labelled struct members in a nested table."""
        if not fld.components:
            # Fallback to old generic struct widget when components weren't built.
            self._draw_struct_widget_legacy(fld, widget_id)
            return
        table_id = f"struct_tbl{widget_id}"
        if not imgui.begin_table(
            table_id, 2,
            imgui.TableFlags_.borders | imgui.TableFlags_.row_bg | imgui.TableFlags_.sizing_stretch_prop,
        ):
            return
        try:
            imgui.table_setup_column("Member", imgui.TableColumnFlags_.width_fixed, 160.0)
            imgui.table_setup_column("Value", imgui.TableColumnFlags_.width_stretch)
            for ci, comp in enumerate(fld.components):
                imgui.table_next_row()
                imgui.table_set_column_index(0)
                imgui.text(comp.name)
                if comp.notes:
                    imgui.same_line()
                    imgui.text_disabled("(?)")
                    if imgui.is_item_hovered():
                        imgui.set_tooltip(comp.notes)
                imgui.table_set_column_index(1)
                self._draw_value_widget(comp, f"{widget_id}_c{ci}", on_change)
        finally:
            imgui.end_table()

    def _draw_array_widget_rows(self, fld: Field, widget_id: str, on_change) -> None:
        """Render array elements as labelled rows with add/remove/copy controls."""
        if not fld.element_field_specs:
            self._draw_array_widget_legacy(fld, widget_id)
            return

        n_rows = len(fld.rows)

        # List header / actions row.
        if imgui.button(f"+ Add##{widget_id}_add"):
            if self._array_can_resize(fld):
                fld.rows.append(self._make_array_row(fld))
                on_change()
        imgui.same_line()
        imgui.text_disabled(f"({n_rows} {'entry' if n_rows == 1 else 'entries'})")

        if n_rows == 0:
            return

        page_size = 64
        page = min(self._array_pages.get(widget_id, 0), (n_rows - 1) // page_size)
        if n_rows > page_size:
            if imgui.small_button(f"Previous##{widget_id}_prev"):
                page = max(0, page - 1)
            imgui.same_line()
            if imgui.small_button(f"Next##{widget_id}_next"):
                page = min((n_rows - 1) // page_size, page + 1)
            imgui.same_line()
            imgui.text_disabled(f"Entries {page * page_size + 1}-{min(n_rows, (page + 1) * page_size)} of {n_rows}")
        self._array_pages[widget_id] = page
        col_specs = fld.element_field_specs
        n_cols = max(len(col_specs), len(fld.rows[0]) if fld.rows else 0)
        if n_cols == 0:
            return

        table_id = f"arr_tbl{widget_id}"
        flags = (
            imgui.TableFlags_.borders
            | imgui.TableFlags_.row_bg
            | imgui.TableFlags_.sizing_stretch_prop
        )
        # +1 index column, +1 actions column.
        if not imgui.begin_table(table_id, n_cols + 2, flags):
            return
        try:
            imgui.table_setup_column("#", imgui.TableColumnFlags_.width_fixed, 32.0)
            for ci in range(n_cols):
                if ci < len(col_specs):
                    label = col_specs[ci].authoring_label or col_specs[ci].name
                else:
                    label = f"col{ci}"
                imgui.table_setup_column(label, imgui.TableColumnFlags_.width_stretch)
            imgui.table_setup_column("", imgui.TableColumnFlags_.width_fixed, 160.0)
            imgui.table_headers_row()

            to_remove: int | None = None
            to_copy: int | None = None
            to_swap: tuple[int, int] | None = None
            for ri in range(page * page_size, min(n_rows, (page + 1) * page_size)):
                row = fld.rows[ri]
                imgui.table_next_row()
                imgui.table_set_column_index(0)
                imgui.text(f"{ri}")
                for ci, comp in enumerate(row):
                    imgui.table_set_column_index(1 + ci)
                    self._draw_value_widget(
                        comp, f"{widget_id}_r{ri}_c{ci}", on_change
                    )
                imgui.table_set_column_index(1 + n_cols)
                if imgui.small_button(f"^##{widget_id}_up_{ri}") and ri > 0:
                    to_swap = (ri, ri - 1)
                imgui.same_line()
                if imgui.small_button(f"v##{widget_id}_dn_{ri}") and ri < n_rows - 1:
                    to_swap = (ri, ri + 1)
                imgui.same_line()
                if imgui.small_button(f"copy##{widget_id}_cp_{ri}"):
                    to_copy = ri
                imgui.same_line()
                if imgui.small_button(f"x##{widget_id}_rm_{ri}"):
                    to_remove = ri

            if to_swap is not None:
                a, b = to_swap
                fld.rows[a], fld.rows[b] = fld.rows[b], fld.rows[a]
                on_change()
            elif to_copy is not None:
                if self._array_can_resize(fld):
                    fld.rows.insert(to_copy + 1, deepcopy(fld.rows[to_copy]))
                    on_change()
            elif to_remove is not None:
                if self._array_can_resize(fld):
                    del fld.rows[to_remove]
                    on_change()
        finally:
            imgui.end_table()

    def _make_array_row(self, fld: Field) -> list[Field]:
        from creation_lib.esp.editor.fields import make_array_row
        return make_array_row(fld, game=self.selection.document.game)

    def _array_can_resize(self, fld: Field) -> bool:
        spec = fld.spec
        if fld.signature == "KWDA" or (spec and spec.array and (
            spec.array.count_record_field or spec.array.count_field or spec.array.count_codec
        )):
            self._show_error("This array has a separate count field. Editing existing entries is supported; resizing requires a dedicated editor.")
            return False
        return True

    def _commit_field(self, idx: int, fld: Field) -> None:
        doc = self.selection.document
        if self._busy or doc is None or not (0 <= idx < len(doc.subrecords)):
            return
        if doc.edit_errors[idx]:
            self._restore_document_fields(doc)
            self._show_error(doc.edit_errors[idx])
            return
        previous = list(doc.subrecords)
        try:
            payload = encode_edit(fld)
            if payload == previous[idx][1]:
                return
            updated = list(previous)
            sig, _, semantic = updated[idx]
            updated[idx] = (sig, payload, semantic)
            if not plugin_handle_set_record_subrecords(doc.handle, doc.summary.form_id, updated):
                raise RuntimeError("Native record update was rejected")
        except Exception as exc:
            self._restore_document_fields(doc)
            self._show_error(f"Unable to edit {fld.name}: {exc}")
            return
        self._undo.append((doc.handle, doc.summary.form_id, previous))
        del self._undo[:-100]
        self._redo.clear()
        doc.subrecords = updated
        self._restore_document_fields(doc)
        self._after_record_edit(doc.handle, doc.summary.form_id)

    def _restore_document_fields(self, doc) -> None:
        from types import SimpleNamespace
        from creation_lib.esp.editor.fields import decode_record
        doc.fields = decode_record(SimpleNamespace(
            signature=doc.summary.signature,
            subrecords=[SimpleNamespace(signature=sig, data=data) for sig, data, _ in doc.subrecords],
        ), game=doc.game, is_localized=any(f.kind == UiFieldKind.LSTRING for f in doc.fields))
        doc.edit_errors = [edit_reason(fld, is_localized=doc.is_localized) for fld in doc.fields]
        if self.selection.document is doc:
            self.selection.fields = doc.fields

    def _after_record_edit(self, handle: int, form_id: int) -> None:
        self._dirty_handles.add(handle)
        self._dirty_records.setdefault(handle, set()).add(form_id)
        self._content_revision += 1
        self._record_cache.clear()
        self._raw_record_text = None
        self._reference_labels.clear()
        self._references = None
        self.session._invalidate_cache()
        try:
            summary = plugin_handle_record_summary(handle, form_id)
        except Exception as exc:
            summary = None
            self._show_error(f"Edit applied, but record label could not refresh: {exc}")
        for key, records in self._cached_group_records.items():
            if key[0] == handle and records is not None and summary is not None:
                for i, item in enumerate(records):
                    if getattr(item, "form_id", None) == form_id:
                        records[i] = summary
                self._filtered_records.pop(key, None)
        if self.selection.plugin_handle == handle and self.selection.record is not None:
            if self.selection.record.form_id == form_id:
                if summary is not None:
                    self.selection.record = summary
                    self.selection.document.summary = summary
        if self.selection.document is not None:
            self.selection.comparison = comparison_rows(self.selection.documents)
            self.selection.differences = [row for row in self.selection.comparison if len(set(row[2])) > 1]
        # Do not retain pre-edit conflict colors. Rescan when the user requests
        # it; launching a full native scan on every keystroke contends with edits.
        self._conflict_scan = None
        self._scan_complete = False
        self.cancel_override_analysis()

    def _draw_struct_widget_legacy(self, fld: Field, widget_id: str) -> None:
        """Generic numeric tuple editor (used when components aren't available)."""
        layout = fld.struct_layout
        if not layout:
            self._draw_bytes_widget_inplace(fld, widget_id, lambda: None)
            return
        import struct as _struct

        raw = fld.raw
        try:
            values = list(_struct.unpack_from(layout, raw))
        except _struct.error:
            imgui.text_disabled(f"<struct parse error, {len(raw)} bytes>")
            return
        fmt_chars = [c for c in layout if c in "iIhHbBqQfd"]
        changed_any = False
        new_values = list(values)
        for ci, (fmt_char, val) in enumerate(zip(fmt_chars, values)):
            comp_id = f"{widget_id}_c{ci}"
            imgui.text(f"[{ci}]")
            imgui.same_line()
            imgui.set_next_item_width(120)
            if fmt_char in "iIhHbBqQ":
                ch, nv = imgui.input_int(comp_id, int(val))
                if ch:
                    new_values[ci] = nv
                    changed_any = True
            elif fmt_char in "fd":
                ch, nv = imgui.input_float(comp_id, float(val))
                if ch:
                    new_values[ci] = nv
                    changed_any = True
            else:
                imgui.text_disabled(repr(val))
        if changed_any:
            try:
                # Mirror legacy behaviour: emit via the Field's value/encode path.
                fld.value = tuple(new_values)
                # Caller is expected to be the top-level subrecord, so commit.
                idx = next(
                    (i for i, f in enumerate(self.selection.fields or []) if f is fld),
                    -1,
                )
                if idx >= 0:
                    self._commit_field(idx, fld)
            except Exception:
                pass

    def _draw_array_widget_legacy(self, fld: Field, widget_id: str) -> None:
        """Hex / scalar fallback for arrays when no component metadata is available."""
        raw = fld.raw
        imgui.text_disabled(f"<array, {len(raw)} bytes>")
        if len(raw) <= 64:
            imgui.same_line()
            imgui.text_disabled(raw.hex())

    def _apply_field_edit(self, idx: int, fld: Field, new_value) -> None:
        fld.value = new_value
        self._commit_field(idx, fld)

    def _invalidate_plugin_tree(self, handle: int, *, reset_history: bool = True) -> None:
        """Clear all tree caches for a plugin after an edit or reload."""
        self._cached_root.pop(handle, None)
        self._cached_groups.pop(handle, None)
        self._header_cache.pop(handle, None)
        header_future = self._header_futures.pop(handle, None)
        if header_future is not None:
            header_future.cancel()
        self._content_revision += 1
        self._scan_complete = False
        self.cancel_override_analysis()
        self._plugin_masters.pop(handle, None)
        self._plugin_flags.pop(handle, None)
        if self.session.get_by_handle(handle) is not None:
            self._header_futures[handle] = self._executor.submit(plugin_handle_get, handle, "header")
        self._record_cache.clear()
        if reset_history:
            self._undo = [item for item in self._undo if item[0] != handle]
            self._redo = [item for item in self._redo if item[0] != handle]
        future = self._tree_futures.pop(handle, None)
        if future is not None:
            future.cancel()
        # Remove all group-record cache entries for this handle.
        for key in [k for k in self._cached_group_records if k[0] == handle]:
            self._cached_group_records.pop(key, None)
            self._filtered_records.pop(key, None)
            self._filter_errors.pop(key, None)
            self._group_errors.pop(key, None)
            self._known_group_types.pop(key, None)
        # Cancel in-flight group futures for this handle.
        for key in [k for k in self._group_futures if k[0] == handle]:
            self._group_futures.pop(key).cancel()
        for key in [k for k in self._filter_futures if k[0] == handle]:
            self._filter_futures.pop(key)[1].cancel()
        self._conflict_scan = None
        if self.selection.plugin_handle == handle:
            self.cancel_selection()
            self.selection.fields = None
            self.selection.document = None
            self.selection.error = None
            self._request_selection()

    def undo(self) -> None:
        self._restore_history(self._undo, self._redo)

    def redo(self) -> None:
        self._restore_history(self._redo, self._undo)

    def _restore_history(self, source: list, target: list) -> None:
        if self._busy or not source:
            return
        handle, form_id, payload = source[-1]
        try:
            current = plugin_handle_call(handle, "record_subrecords", form_id)
            if current is None or not plugin_handle_set_record_subrecords(handle, form_id, payload):
                raise RuntimeError("Record is no longer available")
        except Exception as exc:
            self._show_error(f"Unable to restore edit: {exc}")
            return
        source.pop()
        target.append((handle, form_id, current))
        doc = self.selection.document
        if doc is not None and (doc.handle, doc.summary.form_id) == (handle, form_id):
            doc.subrecords = payload
            self._restore_document_fields(doc)
        self._after_record_edit(handle, form_id)

    # -- info tabs --------------------------------------------------------

    def draw_info_tabs(self) -> None:
        if not imgui.begin_tab_bar("info_tabs"):
            return
        try:
            if imgui.begin_tab_item("Info")[0]:
                self._draw_info_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Conflicts")[0]:
                self._draw_conflicts_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Validation")[0]:
                self._draw_validation_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("ReferencedBy")[0]:
                self._draw_referenced_by_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Messages")[0]:
                self._draw_messages_tab()
                imgui.end_tab_item()
        finally:
            imgui.end_tab_bar()

    def _draw_conflicts_tab(self) -> None:
        if imgui.button("Open overrides overview##conflict_overview"):
            self._show_overview = True
        if imgui.button("Scan Conflicts"):
            self.run_conflict_scan()
        imgui.same_line()
        scan = self._conflict_scan
        if scan is None:
            imgui.text_disabled("(no scan run yet)")
            if self._auto_conflict_deferred:
                imgui.text_wrapped("For this large load order, run a full scan when needed. Select a record to inspect its overrides immediately.")
            return
        active = self.session.active
        if active is None:
            imgui.text_disabled("(no active plugin — select one in the nav tree)")
            return
        active_handle = active.handle
        n_active = self._scan_counts()[2].get(active_handle, 0)
        imgui.text(f"{n_active} conflict(s) involving {active.plugin_name} (of {len(scan)} total)")
        if not self._scan_complete:
            imgui.text_wrapped("Partial analysis: counts cover completed batches.")

        # Patch actions row.
        patch_handle = self.session._patch_handle
        if patch_handle is None:
            if imgui.button("New Patch Plugin..."):
                self.open_new_patch_popup()
            imgui.same_line()
            imgui.text_disabled("(no patch target)")
        else:
            patch_name = ""
            patch_plugin = self.session.get_by_handle(patch_handle)
            if patch_plugin is not None:
                patch_name = patch_plugin.plugin_name
            imgui.text_disabled(f"patch: {patch_name}")
            imgui.same_line()
            n_sel = len(self._conflict_selected_fids)
            disabled = n_sel == 0
            if disabled:
                imgui.begin_disabled()
            if imgui.button(f"Add Winner to Patch ({n_sel})"):
                self.add_selected_to_patch(automerge=False)
            imgui.same_line()
            if imgui.button(f"Auto-Merge Selected ({n_sel})"):
                self.add_selected_to_patch(automerge=True)
            if disabled:
                imgui.end_disabled()
        imgui.separator()

        # Filters.
        imgui.set_next_item_width(-1)
        _, self._conflict_filter_text = imgui.input_text_with_hint(
            "##conflict_text", "Editor ID or file FormID", self._conflict_filter_text
        )
        signatures = sorted({report.signature for report in scan.reports})
        labels = {sig: f"{self._record_label(sig)} ({sig})" for sig in signatures}
        selected = self._choice("Record type", "conflict_record_type", ("All record types", *labels.values()), labels.get(self._conflict_filter_signature, "All record types"))
        self._conflict_filter_signature = next((sig for sig, label in labels.items() if label == selected), "")
        _, self._conflict_only_mergeable = imgui.checkbox(
            "Mergeable only##conflict_merge", self._conflict_only_mergeable
        )
        imgui.separator()
        if imgui.button("Export CSV##conflicts_csv"):
            self._export_conflict_report("csv")
        imgui.same_line()
        if imgui.button("Export JSON##conflicts_json"):
            self._export_conflict_report("json")
        imgui.separator()

        visible_reports = self._visible_conflict_reports()
        grouped_reports: dict[str, list[ConflictReport]] = {}
        for rpt in visible_reports:
            grouped_reports.setdefault(rpt.signature, []).append(rpt)

        # Group by signature, sorted; render under collapsing headers.
        for signature in sorted(grouped_reports.keys()):
            reports = grouped_reports[signature]
            display = self._record_label(signature)
            with expandable_section(f"{display} ({len(reports)})###conf_grp_{signature}") as expanded:
                if expanded:
                    clipper = imgui.ListClipper()
                    clipper.begin(len(reports))
                    while clipper.step():
                        for i in range(clipper.display_start, clipper.display_end):
                            self._draw_conflict_row(reports[i])
                    clipper.end()

    def _draw_conflict_row(self, report: ConflictReport) -> None:
        eid = report.editor_id or ""
        merge_tag = "[merge]" if report.mergeable else "[copy ]"
        label = (
            f"{merge_tag}  {report.form_id:08X}  {eid:24s}  "
            f"{len(report.chain)} plugin(s)"
        )
        key = (report.winner.plugin_handle, report.winner.form_id)
        is_selected = key in self._conflict_selected_fids
        clicked, new_selected = imgui.checkbox(
            f"##conf_sel_{key}", is_selected
        )
        if clicked:
            if new_selected:
                self._conflict_selected_fids.add(key)
            else:
                self._conflict_selected_fids.discard(key)
        imgui.same_line()
        color = self._conflict_row_color(report)
        if color is not None:
            imgui.push_style_color(imgui.Col_.text, color)
        if imgui.selectable(f"{label}###conf_row_{key}", False)[0]:
            record = self._record_for_handle_form_id(*key)
            if record is not None:
                self._select_record(key[0], record)
        if color is not None:
            imgui.pop_style_color()
        if imgui.is_item_hovered():
            chain_lines = "\n".join(
                f"  {e.load_order_index:>2}  {e.plugin_name}"
                for e in report.chain
            )
            imgui.set_tooltip(f"Override chain in load order; full winner: {report.winner.plugin_name}\n{chain_lines}")

    def _conflict_row_color(
        self, report: ConflictReport
    ) -> tuple[float, float, float, float] | None:
        active = self.session.active
        if active is None:
            return None
        if not any(entry.plugin_handle == active.handle for entry in report.chain):
            return None
        if any(entry.plugin_handle == active.handle and entry.is_partial for entry in report.chain):
            return _ROLE_COLORS["partial"]
        if report.status == ConflictStatus.CONFLICT:
            if report.winner.plugin_handle == active.handle:
                return _ROLE_COLORS["winner"]
            return _ROLE_COLORS["loser"]
        if report.status == ConflictStatus.OVERRIDE:
            return _OVERRIDE_ROW_COLOR
        return None

    def _draw_validation_tab(self) -> None:
        active = self.session.active
        if active is None:
            imgui.text_disabled("No active plugin.")
            return
        if imgui.button("Check Active Plugin for Errors"):
            self.run_validation()
        imgui.same_line()
        imgui.text_disabled(self._validation_summary)
        report = self._validation_report
        if report is None:
            imgui.text_disabled("(no check run yet)")
            return
        target = self.session.get_by_handle(self._validation_target_handle or -1)
        if target is not None:
            imgui.text(f"Plugin: {target.plugin_name}")
        imgui.separator()
        if imgui.button("Export CSV##validation_csv"):
            self._export_validation_report("csv")
        imgui.same_line()
        if imgui.button("Export JSON##validation_json"):
            self._export_validation_report("json")
        imgui.separator()
        if len(report) == 0:
            imgui.text_disabled("(no issues found)")
            return
        flags = imgui.TableFlags_.borders | imgui.TableFlags_.row_bg | imgui.TableFlags_.sizing_stretch_prop
        if not imgui.begin_table("validation_issues", 5, flags):
            return
        try:
            imgui.table_setup_column("Severity", imgui.TableColumnFlags_.width_fixed, 80.0)
            imgui.table_setup_column("Category", imgui.TableColumnFlags_.width_fixed, 150.0)
            imgui.table_setup_column("Plugin", imgui.TableColumnFlags_.width_fixed, 140.0)
            imgui.table_setup_column("FormID", imgui.TableColumnFlags_.width_fixed, 90.0)
            imgui.table_setup_column("Message", imgui.TableColumnFlags_.width_stretch)
            imgui.table_headers_row()
            for idx, issue in enumerate(report):
                imgui.table_next_row()
                severity = issue.severity.value
                color = _VALIDATION_SEVERITY_COLORS.get(issue.severity)
                imgui.table_set_column_index(0)
                if color is not None:
                    imgui.push_style_color(imgui.Col_.text, color)
                imgui.text(severity)
                if color is not None:
                    imgui.pop_style_color()
                imgui.table_set_column_index(1)
                imgui.text(issue.category.value)
                imgui.table_set_column_index(2)
                imgui.text(issue.plugin_name)
                imgui.table_set_column_index(3)
                form_id = issue.form_id
                if form_id is None:
                    imgui.text_disabled("-")
                elif imgui.selectable(f"{int(form_id):08X}###validation_{idx}", False)[0]:
                    located = self.session.resolve_form_id(int(form_id))
                    if located is not None:
                        self._select_record(located[0], located[1])
                imgui.table_set_column_index(4)
                imgui.text_wrapped(issue.message)
        finally:
            imgui.end_table()

    def _draw_info_tab(self) -> None:
        active = self.session.active
        if active is None:
            imgui.text_disabled("No active plugin.")
            return
        header = self._header_cache.get(active.handle)
        if header is None:
            if active.handle not in self._header_futures:
                self._header_futures[active.handle] = self._detail_executor.submit(plugin_handle_get, active.handle, "header")
            imgui.text_disabled("Loading plugin information...")
            return
        author = header.author or ""
        description = header.description or ""
        version = float(header.version or 1.0)
        header_flags = int(header.flags or 0)

        imgui.text_wrapped(f"Plugin: {active.plugin_name}")
        imgui.text_wrapped(f"Path: {active.path}")
        imgui.text(f"Game:    {active.game}")
        imgui.text(f"Version: {version}")
        imgui.separator()

        imgui.text("Author")
        imgui.set_next_item_width(-1)
        changed, new_author = imgui.input_text("##plugin_author", author)
        if changed:
            self._set_header(active.handle, "author", new_author)
        imgui.text("Description")
        changed, new_desc = imgui.input_text_multiline(
            "##plugin_description", description, imgui.ImVec2(-1, 80)
        )
        if changed:
            self._set_header(active.handle, "description", new_desc)
        imgui.separator()

        imgui.text(f"Header Flags: 0x{header_flags:08X}")
        new_flags = header_flags
        flag_table = _HEADER_FLAG_TABLES.get(active.game, _HEADER_FLAG_TABLES["fo4"])
        for bit, label, tooltip in flag_table:
            set_now = bool(header_flags & bit)
            ch, set_now = imgui.checkbox(f"{label}##hdr_flag_{bit}", set_now)
            if tooltip and imgui.is_item_hovered():
                imgui.set_tooltip(tooltip)
            if ch:
                new_flags = (new_flags | bit) if set_now else (new_flags & ~bit)
        if new_flags != header_flags:
            self._set_header(active.handle, "flags", new_flags)
        imgui.separator()

        masters = list(header.masters or [])
        imgui.text(f"Masters ({len(masters)}):")
        for i, m in enumerate(masters):
            loaded = self.session.get_by_name(m) is not None
            color = (0.7, 0.9, 0.7, 1.0) if loaded else (0.95, 0.5, 0.5, 1.0)
            imgui.push_style_color(imgui.Col_.text, color)
            imgui.bullet_text(m)
            imgui.pop_style_color()
            imgui.same_line()
            if imgui.small_button(f"X##master_{i}"):
                try:
                    if self._busy:
                        continue
                    sizes = list(plugin_handle_get(active.handle, "master_sizes") or [])
                    sizes = (sizes + [0] * len(masters))[:len(masters)]
                    plugin_handle_call(active.handle, "set_masters", [
                        (name, size) for j, (name, size) in enumerate(zip(masters, sizes)) if j != i
                    ])
                    self._dirty_handles.add(active.handle)
                    self._invalidate_plugin_tree(active.handle)
                    self.cancel_selection()
                    self.selection = _Selection()
                    self._undo.clear()
                    self._redo.clear()
                    self._start_auto_conflict_scan()
                except Exception as exc:
                    self._show_error(f"Unable to remove master: {exc}")

    def _draw_referenced_by_tab(self) -> None:
        if self.selection.record is None:
            imgui.text_disabled("Select a record to see references.")
            return
        rec = self.selection.record
        if self._references is None:
            if self._reference_future is None:
                handle, form_id = self.selection.plugin_handle, rec.form_id
                self._reference_job = (self._selection_generation, self._view_stamp())
                def work():
                    return [(h, fid, self._referenced_by_label(h, fid)) for h, fid in
                            self.session.referencing(form_id, source_handle=handle)]
                self._reference_future = self._detail_executor.submit(work)
            imgui.text_disabled("Loading references...")
            return
        refs = self._references
        imgui.text(f"References to file FormID {rec.form_id:08X}: {len(refs)}")
        imgui.separator()
        clipper = imgui.ListClipper()
        clipper.begin(len(refs))
        while clipper.step():
            for i in range(clipper.display_start, clipper.display_end):
                handle, ref_fid, label = refs[i]
                if imgui.selectable(f"{label}##ref_{handle}_{ref_fid}", False)[0]:
                    record = self._record_for_handle_form_id(handle, ref_fid)
                    if record is not None:
                        self._select_record(handle, record)
        clipper.end()

    def _referenced_by_label(self, handle: int, form_id: int) -> str:
        plugin = self.session.get_by_handle(handle)
        name = plugin.plugin_name if plugin else "?"
        record = self._record_for_handle_form_id(handle, form_id)
        editor_id = getattr(record, "editor_id", None) or ""
        if editor_id:
            return f"{name}  {form_id:08X}  {editor_id}"
        return f"{name}  {form_id:08X}"

    @staticmethod
    def _record_for_handle_form_id(handle: int, form_id: int):
        try:
            return plugin_handle_record_summary(handle, int(form_id))
        except Exception:
            return None

    def _draw_messages_tab(self) -> None:
        if not self._messages:
            imgui.text_disabled("(no messages)")
            return
        if self._latest_error:
            imgui.text_colored((0.95, 0.45, 0.45, 1.0), f"Latest error: {self._latest_error}")
            imgui.separator()
        for idx, message in enumerate(self._messages[-200:]):
            imgui.text_wrapped(f"{idx + 1}. {message}")

    def _set_header(self, handle: int, field: str, value) -> None:
        if self._busy:
            return
        try:
            plugin_handle_call(handle, f"set_header_{field}", value)
            header = self._header_cache.get(handle)
            if header is not None:
                setattr(header, field, value)
            self._dirty_handles.add(handle)
            self._content_revision += 1
            self._record_cache.clear()
            if field == "flags":
                plugin = self.session.get_by_handle(handle)
                if plugin is not None:
                    plugin.is_master = bool(value & 1)
                self._invalidate_plugin_tree(handle)
        except Exception:
            _log.exception("Failed to set header field %s", field)

    # -- helpers ----------------------------------------------------------

    def _formid_label(self, form_id: int) -> str:
        if form_id == 0:
            return "<null>"
        doc = self.selection.document
        if doc is not None:
            return doc.form_key(form_id)
        located = self.session.resolve_form_id(form_id, source_handle=self.selection.plugin_handle)
        if located is None:
            return ""
        plugin = self.session.get_by_handle(located[0])
        record = located[1]
        editor_id = getattr(record, "editor_id", None) or ""
        if plugin is None:
            return editor_id
        return f"{plugin.plugin_name}:{editor_id}" if editor_id else plugin.plugin_name

    def _show_error(self, message: str) -> None:
        self._latest_error = message
        self._add_message(message)
        _log.error(message)

    def _add_message(self, message: str) -> None:
        self._messages.append(str(message))
        if len(self._messages) > 500:
            del self._messages[:-500]

    def cleanup(self) -> None:
        self.cancel_selection()
        # Drain native work before closing its handles; never close a handle
        # underneath a worker that is still reading or saving it.
        self._detail_executor.shutdown(wait=True, cancel_futures=True)
        self._executor.shutdown(wait=True, cancel_futures=True)
        try:
            self.session.close_all()
        except Exception:
            _log.exception("Cleanup failed")


# -- module-level helpers -------------------------------------------------

def _make_backup(target: str) -> None:
    """Copy target to <target>.bak; if .bak exists, use timestamped fallback."""
    src = Path(target)
    if not src.is_file():
        return
    bak = src.with_suffix(src.suffix + ".bak")
    if bak.exists():
        ts = datetime.now().strftime("%Y%m%d-%H%M%S-%f")
        bak = src.with_suffix(src.suffix + f".bak-{ts}")
    try:
        shutil.copy2(src, bak)
        _log.info("Backup: %s -> %s", src, bak)
    except OSError:
        _log.exception("Backup failed for %s", src)
        raise


_OVERRIDE_ROW_COLOR = (0.95, 0.85, 0.45, 1.0)

_VALIDATION_SEVERITY_COLORS = {
    Severity.ERROR: (1.0, 0.45, 0.45, 1.0),
    Severity.WARNING: (1.0, 0.80, 0.30, 1.0),
    Severity.INFO: (0.70, 0.80, 0.95, 1.0),
}

# Per-plugin conflict-role colors used by the navigation tree.
# "only" / "override" → no color (default text), "winner" → green, "loser" → red.
_ROLE_COLORS: dict[str, tuple[float, float, float, float]] = {
    "winner": (0.55, 0.95, 0.55, 1.0),
    "loser":  (1.0, 0.5, 0.5, 1.0),
    "override": (0.95, 0.85, 0.45, 1.0),
    "uncompared": (0.95, 0.85, 0.45, 1.0),
    "partial": (0.85, 0.65, 1.0, 1.0),
}
_ROLE_TAGS = {"winner": "W", "loser": "L", "override": "=", "uncompared": "O?", "partial": "P"}
_ROLE_LABELS = {"winner": "[W] Winning conflict", "loser": "[L] Overridden conflict",
                "override": "[=] Identical override", "uncompared": "[O?] Override, not compared",
                "partial": "[P] Partial form", "edited": "[E] Unsaved edit"}

# Records edited in this session (not yet saved) — overrides the role color.
_MODIFIED_COLOR = (0.45, 0.70, 1.0, 1.0)

# Background tint pushed onto the imgui Selectable when the row is the active
# editor selection — gives a visibly stronger highlight than the default
# Col_Header alpha.
_SELECTED_BG_COLOR = (0.20, 0.40, 0.65, 0.85)
_SELECTED_BG_HOVER_COLOR = (0.28, 0.50, 0.78, 0.95)

# Per-game TES4 header flag bit tables: (mask, label, tooltip).
_HEADER_FLAG_TABLES: dict[str, list[tuple[int, str, str]]] = {
    "fo4": [
        (0x00000001, "ESM (Master)", "Plugin loads as a master file."),
        (0x00000080, "Localized strings", "Strings are stored in external STRINGS files."),
        (0x00000200, "Light master (ESL)", "Limited to 0xFFF records; loaded into the FE slot."),
    ],
    "fo76": [
        (0x00000001, "ESM (Master)", "Plugin loads as a master file."),
        (0x00000080, "Localized strings", "Strings are stored in external STRINGS files."),
        (0x00000200, "Light master (ESL)", "Limited to 0xFFF records."),
    ],
    "starfield": [
        (0x00000001, "ESM (Master)", "Plugin loads as a master file."),
        (0x00000080, "Localized strings", "Strings are stored in external STRINGS files."),
        (0x00000100, "Small / light master", "Uses a 12-bit object ID in an FE slot."),
        (0x00000200, "Update master", "Starfield update master."),
        (0x00000400, "Medium master", "Uses a 16-bit object ID in an FD slot."),
    ],
    "skyrimse": [
        (0x00000001, "ESM (Master)", "Plugin loads as a master file."),
        (0x00000080, "Localized strings", "Strings are stored in external STRINGS files."),
        (0x00000200, "Light master (ESL)", "Limited to 0xFFF records."),
    ],
    "fo3": [
        (0x00000001, "ESM (Master)", "Plugin loads as a master file."),
    ],
    "fnv": [
        (0x00000001, "ESM (Master)", "Plugin loads as a master file."),
    ],
    "oblivion": [
        (0x00000001, "ESM (Master)", "Plugin loads as a master file."),
    ],
}
