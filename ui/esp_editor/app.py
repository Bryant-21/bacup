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
import math
import re
import shutil
from concurrent.futures import Future, ThreadPoolExecutor
from dataclasses import dataclass, field
from datetime import datetime
from pathlib import Path
from typing import Any

from imgui_bundle import imgui

from creation_lib.esp.editor import (
    ConflictReport,
    ConflictScan,
    ConflictScanner,
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
)
from creation_lib.esp.record_types import record_type_display_label
from creation_lib.esp.schema import get_schema
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import pick_file, pick_save_file

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
        # Per-group async record fetches: (handle, signature) → Future.
        self._group_futures: dict[tuple[int, str], Future] = {}
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
        self._conflict_filter_text: str = ""
        self._conflict_filter_signature: str = ""
        self._conflict_only_mergeable: bool = False
        self._conflict_selected_fids: set[int] = set()
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
        self._renumber_base_text: str = "0x800"
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

        def _worker():
            plugin = self.session.load(path, game=game)
            warm = self._warmup_plugin(plugin)
            return [(plugin, warm)]

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

        def _worker():
            plugins = self.session.load_folder(folder)
            return [(p, self._warmup_plugin(p)) for p in plugins]

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
            record_count = sum(int(c) for _, c in groups)
            return {
                "groups": groups,
                "group_records": {},
                "record_count": record_count,
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
        self._start_auto_conflict_scan()
        for plugin in self.session.plugins:
            self._invalidate_plugin_tree(plugin.handle)

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
        reports = [
            self._conflict_scan.by_form_id[fid]
            for fid in self._conflict_selected_fids
            if fid in self._conflict_scan.by_form_id
        ]
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
        if not self.session.plugins:
            self._conflict_scan = None
            self._auto_conflict_rescan_pending = False
            return
        if self._auto_conflict_future is not None and not self._auto_conflict_future.done():
            self._auto_conflict_rescan_pending = True
            return
        self._auto_conflict_rescan_pending = False
        self._auto_conflict_future = self._executor.submit(self.session.run_conflict_scan)

    def run_conflict_scan(self) -> None:
        if self._busy:
            return
        if not self.session.plugins:
            return

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"Conflict scan failed: {result}")
                return
            self._conflict_scan = result
            _log.info("Conflict scan: %d conflict(s)", len(result))

        self._start_background(
            "Scanning conflicts...",
            lambda: self.session.run_conflict_scan(),
            _on_done,
        )

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
        imgui.open_popup("Save Plugins##esp_save_modal")

    def _draw_save_popup(self) -> None:
        """Draw the save-plugins modal. Must be called every frame."""
        imgui.set_next_window_size(imgui.ImVec2(420, 0))
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
                    f"{dirty_star}{entry.plugin_name}##save_cb_{entry.handle}",
                    entry.selected,
                )
            imgui.separator()
            _, self._save_backup = imgui.checkbox(
                "Create backup (.bak)##save_backup", self._save_backup
            )
            imgui.separator()
            if imgui.button("Save Selected", imgui.ImVec2(120, 0)):
                self._execute_save_selected()
                imgui.close_current_popup()
                self._save_popup_open = False
            imgui.same_line()
            if imgui.button("Cancel", imgui.ImVec2(80, 0)):
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
                if do_backup:
                    _make_backup(target)
                try:
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

            def _on_done(result, is_error: bool) -> None:
                if is_error:
                    self._show_error(f"Save failed: {result}")
                    return
                self._dirty_handles.discard(handle)
                self._dirty_records.pop(handle, None)
                _log.info("Saved %s -> %s", plugin_name, target)

            self._start_background(
                f"Saving {Path(target).name}...",
                lambda: plugin_handle_call(handle, "save", target),
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

        self._start_background(f"Exporting 0x{form_id:08X}...", _worker, _on_done)

    def _visible_conflict_reports(self) -> list[ConflictReport]:
        scan = self._conflict_scan
        active = self.session.active
        if scan is None or active is None:
            return []
        sig_filter = self._conflict_filter_signature.strip().upper()
        text_filter = self._conflict_filter_text.strip().lower()
        visible: list[ConflictReport] = []
        for signature in sorted(scan.by_signature.keys()):
            if sig_filter and signature.upper() != sig_filter:
                continue
            for fid in scan.by_signature[signature]:
                rpt = scan.by_form_id[fid]
                if not any(e.plugin_handle == active.handle for e in rpt.chain):
                    continue
                if self._conflict_only_mergeable and not rpt.mergeable:
                    continue
                if text_filter:
                    eid = (rpt.editor_id or "").lower()
                    if text_filter not in eid and text_filter not in f"{fid:08x}":
                        continue
                visible.append(rpt)
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

        self._start_background(f"Exporting validation report...", _worker, _on_done)

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

        self._start_background(f"Exporting conflict report...", _worker, _on_done)

    def close_active(self) -> None:
        active = self.session.active
        if active is None:
            return
        try:
            self.session.close(active.handle)
        except Exception as exc:
            self._show_error(str(exc))

    def _preload_schema(self, game: str | None) -> None:
        """Warm the schema cache for `game` on a worker thread so the first
        record selection doesn't block the UI while FO4's ~122k-line generated
        schema module imports."""
        if not game or game in self._schema_preload_started:
            return
        self._schema_preload_started.add(game)
        self._executor.submit(get_schema, game)

    # -- background plumbing ----------------------------------------------

    def _start_background(self, message: str, work, on_done) -> None:
        self._busy = True
        self._busy_message = message
        self._busy_done_cb = on_done
        self._busy_future = self._executor.submit(work)

    def poll(self) -> None:
        """Called every frame from the workspace. Finalizes background work."""
        # Drain group-record fetches.
        for key, fut in list(self._group_futures.items()):
            if fut.done():
                handle, sig = key
                try:
                    self._cached_group_records[key] = fut.result()
                except Exception:
                    _log.exception("Failed to fetch group %s for handle %s", sig, handle)
                    self._cached_group_records[key] = []
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
                self._conflict_scan = future.result()
                _log.info("Auto-scan: %d conflict(s)", len(self._conflict_scan))
            except Exception:
                _log.exception("Auto-scan failed")
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
        """Draw a foreground spinner+dim mask if a background task is running."""
        if not self._busy:
            return
        viewport = imgui.get_main_viewport()
        pos = viewport.work_pos
        size = viewport.work_size
        draw_list = imgui.get_foreground_draw_list()
        draw_list.add_rect_filled(
            pos,
            imgui.ImVec2(pos.x + size.x, pos.y + size.y),
            imgui.get_color_u32((0.0, 0.0, 0.0, 0.45)),
        )
        cx = pos.x + size.x * 0.5
        cy = pos.y + size.y * 0.5
        spin_radius = 22.0
        t = imgui.get_time()
        angle_start = math.fmod(t * 3.0, math.tau)
        arc_span = math.pi * 1.3
        draw_list.add_circle(
            imgui.ImVec2(cx, cy),
            spin_radius,
            imgui.get_color_u32((1.0, 1.0, 1.0, 0.2)),
            32,
            3.0,
        )
        draw_list.path_arc_to(
            imgui.ImVec2(cx, cy),
            spin_radius,
            angle_start,
            angle_start + arc_span,
            32,
        )
        draw_list.path_stroke(
            imgui.get_color_u32((1.0, 1.0, 1.0, 1.0)),
            False,
            3.0,
        )
        label = self._busy_message or "Working..."
        tw = imgui.calc_text_size(label).x
        draw_list.add_text(
            imgui.ImVec2(cx - tw * 0.5, cy + spin_radius + 12),
            imgui.get_color_u32((1.0, 1.0, 1.0, 1.0)),
            label,
        )

    # -- panels -----------------------------------------------------------

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
        # Pinned filter bar — stays put when the tree below scrolls.
        imgui.text("Filter")
        imgui.same_line()
        avail_x = imgui.get_content_region_avail().x
        sig_box_w = 70.0
        text_box_w = max(80.0, avail_x - sig_box_w - 70.0)
        imgui.set_next_item_width(text_box_w)
        changed_t, self._filter_text = imgui.input_text("##filter_text", self._filter_text)
        imgui.same_line()
        imgui.text("Sig")
        imgui.same_line()
        imgui.set_next_item_width(sig_box_w)
        changed_s, self._filter_signature = imgui.input_text(
            "##filter_sig", self._filter_signature
        )
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
            label = f"{plugin.plugin_name}"
            if plugin.handle == (self.session.active.handle if self.session.active else None):
                label += "  [active]"
            if plugin.handle == self.session._patch_handle:
                label += "  [patch]"
            if plugin.is_master:
                label += "  (master)"
            if plugin.handle in self._dirty_handles:
                label = "* " + label

            tree_open = imgui.tree_node_ex(
                f"{label}##plugin_{plugin.handle}",
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
            sig_filter = self._filter_signature.strip().upper()
            plugin = self.session.get_by_handle(handle)
            game = plugin.game if plugin else None
            for label, count in groups:
                if sig_filter and sig_filter != label.upper():
                    continue
                group_key = (handle, label)
                display = self._record_label(label, game)
                node_label = f"{display} ({count})##grp_{handle}_{label}"
                if imgui.tree_node(node_label):
                    records = self._cached_group_records.get(group_key)
                    if records is None:
                        if group_key not in self._group_futures:
                            self._group_futures[group_key] = self._executor.submit(
                                group_recs_fn or plugin_handle_group_record_summaries,
                                handle,
                                label,
                            )
                            self._cached_group_records[group_key] = None  # type: ignore[assignment]
                        imgui.text_disabled("Loading records...")
                    else:
                        for item in records:
                            self._draw_item(handle, item)
                    imgui.tree_pop()
        else:
            imgui.text_disabled("Record summary API unavailable.")

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
            if imgui.tree_node(f"{display} ({count})##grp_{handle}_{id(item)}"):
                for child in item.children:
                    self._draw_item(handle, child)
                imgui.tree_pop()
            return

        if not self._record_matches_filter(item):
            return
        self._draw_record_leaf(handle, item)

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
        marker = "▶ " if is_selected else ""
        label = f"{marker}{sig_label}  {editor_id}  [0x{item.form_id:08X}]"
        is_modified = item.form_id in self._dirty_records.get(handle, ())
        if is_modified:
            color = _MODIFIED_COLOR
        else:
            color = _ROLE_COLORS.get(self._record_role(handle, item.form_id))
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

        clicked, _ = imgui.selectable(f"{label}##rec_{handle}_{item.form_id}", is_selected)

        if pushed_header:
            imgui.pop_style_color(3)
        if pushed_text:
            imgui.pop_style_color()
        if clicked:
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
            imgui.text_disabled(f"{sig}  {edid}  [0x{item.form_id:08X}]")
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
            return fn(session, source_form_id, deep=deep, target_handle=target_handle)

        def _on_done(result, is_error: bool) -> None:
            if is_error:
                self._show_error(f"{op_name} failed: {result}")
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
            FLAG_LIGHT,
            FLAG_MASTER,
            FLAG_MEDIUM,
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
        self._change_fid_new_text = f"0x{source_form_id:08X}"
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
            imgui.text(f"Old FormID: 0x{source_fid:08X}")
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
        self._renumber_base_text = "0x800"
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

        self._start_background(f"Renumber FormIDs from 0x{base:X}...", _worker, _on_done)

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
        try:
            setter(handle, on)
        except Exception as exc:
            self._show_error(f"Toggle {label} failed: {exc}")
            return
        self._dirty_handles.add(handle)
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
        conflicts = self._plugin_conflict_count(plugin.handle)
        wins = self._plugin_winner_count(plugin.handle)
        lines = [
            f"{plugin.plugin_name}",
            f"Game:      {plugin.game}",
            f"Path:      {plugin.path}",
            f"Records:   {total_records}",
            f"Conflicts: {conflicts}",
            f"Winning:   {wins}",
        ]
        if plugin.is_master:
            lines.append("(master)")
        return "\n".join(lines)

    def _record_role(self, handle: int, form_id: int) -> str:
        """Classify a record as ``winner`` / ``loser`` / ``override`` / ``only``
        for tree coloring, using the cached conflict scan. Records not present
        in the scan are unique to a single plugin (``only`` — no color).

        ``OVERRIDE`` (bytes match across plugins) maps to ``override`` — no
        color. ``CONFLICT`` (bytes differ): the last chain entry in load order
        is the ``winner`` (green); earlier entries are ``loser`` (red).

        Lookup is keyed by ``(plugin_handle, raw_form_id)`` because the same
        logical record has a different raw form_id in each plugin (the high
        byte indexes that plugin's own masters list).
        """
        scan = self._conflict_scan
        if scan is None:
            return "only"
        report = scan.report_for(handle, form_id)
        if report is None or not report.chain:
            return "only"
        if report.status != ConflictStatus.CONFLICT:
            return "override"
        if report.chain[-1].plugin_handle == handle:
            return "winner"
        return "loser"

    def _plugin_conflict_count(self, handle: int) -> int:
        scan = self._conflict_scan
        if scan is None:
            return 0
        return sum(
            1
            for rpt in scan.by_form_id.values()
            if rpt.status == ConflictStatus.CONFLICT
            and any(e.plugin_handle == handle for e in rpt.chain)
        )

    def _plugin_winner_count(self, handle: int) -> int:
        scan = self._conflict_scan
        if scan is None:
            return 0
        return sum(
            1
            for rpt in scan.by_form_id.values()
            if rpt.status == ConflictStatus.CONFLICT
            and rpt.chain
            and rpt.chain[-1].plugin_handle == handle
        )

    def _record_matches_filter(self, record) -> bool:
        sig_filter = self._filter_signature.strip().upper()
        if sig_filter and record.signature.upper() != sig_filter:
            return False
        text_filter = self._filter_text.strip().lower()
        if not text_filter:
            return True
        editor_id = (record.editor_id or "").lower()
        if text_filter in editor_id:
            return True
        if text_filter in f"{record.form_id:08x}":
            return True
        return False

    def _select_record(self, handle: int, record) -> None:
        self.selection = _Selection(
            plugin_handle=handle,
            record=record,
            fields=None,
        )

    # -- record editor ----------------------------------------------------

    def draw_record_view(self) -> None:
        if self.selection.record is None:
            imgui.text_disabled("Select a record from the navigation tree.")
            return
        rec = self.selection.record
        plugin = self.session.get_by_handle(self.selection.plugin_handle)
        game = plugin.game if plugin else None
        sig_label = self._record_label(rec.signature, game)
        imgui.text(
            f"{sig_label}  ({rec.signature})  0x{rec.form_id:08X}  "
            f"{rec.editor_id or ''}"
        )
        imgui.separator()
        try:
            text = plugin_handle_call(
                self.selection.plugin_handle,
                "export_record_text",
                int(rec.form_id),
                "json",
            )
        except Exception as exc:
            imgui.text_disabled(f"Unable to export record text: {exc}")
            return
        imgui.text_wrapped(str(text))

    def _draw_field_widget(self, idx: int, fld: Field) -> None:
        widget_id = f"##fld_{idx}"
        # Render the value widget. For STRUCT/ARRAY fields the change-handler is
        # baked into the component widgets and the caller only needs to repack
        # the parent record after a sub-edit.
        on_change = lambda: self._commit_field(idx, fld)
        self._draw_value_widget(fld, widget_id, on_change)

    def _draw_value_widget(self, fld: Field, widget_id: str, on_change) -> None:
        """Render a single Field's value widget. ``on_change`` is invoked whenever
        the caller should re-pack the parent subrecord after an edit (used for
        nested struct/array components)."""
        if fld.kind == UiFieldKind.INT:
            value = int(fld.value or 0)
            imgui.set_next_item_width(-1)
            changed, new_val = imgui.input_int(widget_id, value)
            if changed:
                fld.value = int(new_val)
                on_change()
        elif fld.kind == UiFieldKind.FLOAT:
            value = float(fld.value or 0.0)
            imgui.set_next_item_width(-1)
            changed, new_val = imgui.input_float(widget_id, value)
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
            try:
                resolved = plugin_handle_call(
                    self.selection.plugin_handle, "resolve_string", value, ""
                )
            except Exception:
                resolved = ""
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
        imgui.text(f"0x{value:08X}")
        imgui.same_line()
        label = self._formid_label(value)
        if label:
            imgui.text_disabled(label)
        if fld.formlink_target:
            imgui.same_line()
            imgui.text_disabled(f"→ {fld.formlink_target}")
        imgui.same_line()
        imgui.set_next_item_width(120)
        changed, new_val = imgui.input_int(
            widget_id, value, 0, 0, imgui.InputTextFlags_.chars_hexadecimal
        )
        if changed:
            fld.value = int(new_val)
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
            self._show_error("Field editing is disabled until edits run through native record APIs.")
        imgui.same_line()
        imgui.text_disabled(f"({n_rows} {'entry' if n_rows == 1 else 'entries'})")

        if n_rows == 0:
            return
        if n_rows > 64:
            imgui.text_disabled(f"<array too long for inline view ({n_rows}) — hex>")
            if len(fld.raw) <= 64:
                imgui.same_line()
                imgui.text_disabled(fld.raw.hex())
            return

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
            for ri, row in enumerate(fld.rows):
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
                self._show_error("Field editing is disabled until edits run through native record APIs.")
            elif to_copy is not None:
                self._show_error("Field editing is disabled until edits run through native record APIs.")
            elif to_remove is not None:
                self._show_error("Field editing is disabled until edits run through native record APIs.")
        finally:
            imgui.end_table()

    def _make_array_row(self, fld: Field) -> list[Field]:
        return []

    def _commit_field(self, idx: int, fld: Field) -> None:
        self._show_error("Field editing is disabled until edits run through native record APIs.")

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
        self._show_error("Field editing is disabled until edits run through native record APIs.")

    def _invalidate_plugin_tree(self, handle: int) -> None:
        """Clear all tree caches for a plugin after an edit or reload."""
        self._cached_root.pop(handle, None)
        self._cached_groups.pop(handle, None)
        # Remove all group-record cache entries for this handle.
        for key in [k for k in self._cached_group_records if k[0] == handle]:
            self._cached_group_records.pop(key, None)
        # Cancel in-flight group futures for this handle.
        for key in [k for k in self._group_futures if k[0] == handle]:
            self._group_futures.pop(key, None)

    def undo(self) -> None:
        self._undo.clear()

    def redo(self) -> None:
        self._redo.clear()

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
        if imgui.button("Scan Conflicts"):
            self.run_conflict_scan()
        imgui.same_line()
        scan = self._conflict_scan
        if scan is None:
            imgui.text_disabled("(no scan run yet)")
            return
        active = self.session.active
        if active is None:
            imgui.text_disabled("(no active plugin — select one in the nav tree)")
            return
        active_handle = active.handle
        n_active = sum(
            1
            for rpt in scan.by_form_id.values()
            if any(e.plugin_handle == active_handle for e in rpt.chain)
        )
        imgui.text(f"{n_active} conflict(s) involving {active.plugin_name} (of {len(scan)} total)")

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
        imgui.set_next_item_width(160)
        _, self._conflict_filter_text = imgui.input_text(
            "Filter##conflict_text", self._conflict_filter_text
        )
        imgui.same_line()
        imgui.set_next_item_width(60)
        _, self._conflict_filter_signature = imgui.input_text(
            "Sig##conflict_sig", self._conflict_filter_signature
        )
        imgui.same_line()
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
            if imgui.collapsing_header(f"{display} ({len(reports)})##conf_grp_{signature}"):
                for rpt in reports:
                    self._draw_conflict_row(rpt)

    def _draw_conflict_row(self, report: ConflictReport) -> None:
        eid = report.editor_id or ""
        merge_tag = "[merge]" if report.mergeable else "[copy ]"
        label = (
            f"{merge_tag}  0x{report.form_id:08X}  {eid:24s}  "
            f"{len(report.chain)} plugin(s)"
        )
        is_selected = report.form_id in self._conflict_selected_fids
        clicked, new_selected = imgui.checkbox(
            f"##conf_sel_{report.form_id}", is_selected
        )
        if clicked:
            if new_selected:
                self._conflict_selected_fids.add(report.form_id)
            else:
                self._conflict_selected_fids.discard(report.form_id)
        imgui.same_line()
        color = self._conflict_row_color(report)
        if color is not None:
            imgui.push_style_color(imgui.Col_.text, color)
        if imgui.selectable(f"{label}##conf_row_{report.form_id}", False)[0]:
            located = self.session.resolve_form_id(report.form_id)
            if located is not None:
                self._select_record(located[0], located[1])
        if color is not None:
            imgui.pop_style_color()
        if imgui.is_item_hovered():
            chain_lines = "\n".join(
                f"  {e.load_order_index:>2}  {e.plugin_name}"
                for e in report.chain
            )
            imgui.set_tooltip(f"Override chain (winner last):\n{chain_lines}")

    def _conflict_row_color(
        self, report: ConflictReport
    ) -> tuple[float, float, float, float] | None:
        active = self.session.active
        if active is None:
            return None
        if not any(entry.plugin_handle == active.handle for entry in report.chain):
            return None
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
                elif imgui.selectable(f"0x{int(form_id):08X}##validation_{idx}", False)[0]:
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
        author = plugin_handle_get(active.handle, "header_author") or ""
        description = plugin_handle_get(active.handle, "header_description") or ""
        version = float(plugin_handle_get(active.handle, "header_version") or 1.0)
        header_flags = int(plugin_handle_get(active.handle, "header_flags") or 0)

        imgui.text(f"Plugin:  {active.plugin_name}")
        imgui.text(f"Path:    {active.path}")
        imgui.text(f"Game:    {active.game}")
        imgui.text(f"Version: {version}")
        imgui.separator()

        changed, new_author = imgui.input_text("Author", author)
        if changed:
            self._set_header(active.handle, "author", new_author)
        changed, new_desc = imgui.input_text_multiline(
            "Description", description, imgui.ImVec2(-1, 80)
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

        masters = list(plugin_handle_get(active.handle, "masters") or [])
        imgui.text(f"Masters ({len(masters)}):")
        for i, m in enumerate(masters):
            loaded = self.session.get_by_name(m) is not None
            color = (0.7, 0.9, 0.7, 1.0) if loaded else (0.95, 0.5, 0.5, 1.0)
            imgui.push_style_color(imgui.Col_.text, color)
            imgui.bullet_text(m)
            imgui.pop_style_color()
            imgui.same_line()
            if imgui.small_button(f"X##master_{i}"):
                new_masters = masters[:i] + masters[i + 1:]
                try:
                    plugin_handle_call(active.handle, "set_header_masters", new_masters)
                    self._dirty_handles.add(active.handle)
                except Exception:
                    _log.exception("Failed to remove master")

    def _draw_referenced_by_tab(self) -> None:
        if self.selection.record is None:
            imgui.text_disabled("Select a record to see references.")
            return
        rec = self.selection.record
        refs = self.session.referencing(rec.form_id)
        imgui.text(f"References to 0x{rec.form_id:08X}: {len(refs)}")
        imgui.separator()
        for handle, ref_fid in refs:
            label = self._referenced_by_label(handle, ref_fid)
            if imgui.selectable(f"{label}##ref_{handle}_{ref_fid}", False)[0]:
                record = self._record_for_handle_form_id(handle, ref_fid)
                if record is not None:
                    self._select_record(handle, record)
                else:
                    located = self.session.resolve_form_id(ref_fid)
                    if located is not None:
                        self._select_record(located[0], located[1])

    def _referenced_by_label(self, handle: int, form_id: int) -> str:
        plugin = self.session.get_by_handle(handle)
        name = plugin.plugin_name if plugin else "?"
        record = self._record_for_handle_form_id(handle, form_id)
        editor_id = getattr(record, "editor_id", None) or ""
        if editor_id:
            return f"{name}  0x{form_id:08X}  {editor_id}"
        return f"{name}  0x{form_id:08X}"

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
        try:
            plugin_handle_call(handle, f"set_header_{field}", value)
            self._dirty_handles.add(handle)
        except Exception:
            _log.exception("Failed to set header field %s", field)

    # -- helpers ----------------------------------------------------------

    def _formid_label(self, form_id: int) -> str:
        if form_id == 0:
            return "<null>"
        located = self.session.resolve_form_id(form_id)
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
        ts = datetime.now().strftime("%Y%m%d-%H%M%S")
        bak = src.with_suffix(src.suffix + f".bak-{ts}")
    try:
        shutil.copy2(src, bak)
        _log.info("Backup: %s -> %s", src, bak)
    except OSError:
        _log.exception("Backup failed for %s", src)


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
}

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
        (0x00000100, "Medium master", "Starfield medium master."),
        (0x00000200, "Small / light master", "Limited record-count master."),
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
