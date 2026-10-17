from __future__ import annotations

import queue
import threading
from pathlib import Path

from ui.previs.state import PrevisState


class PrevisApp:
    def __init__(self, toolkit_settings=None):
        self.state = PrevisState()
        self._toolkit_settings = toolkit_settings
        self._queue: queue.Queue[tuple] = queue.Queue()
        self._thread: threading.Thread | None = None
        self._spec = None

    def game_root(self) -> Path | None:
        if self._toolkit_settings is None:
            return None
        root = self._toolkit_settings.get_game_paths("fo4").get("root_dir")
        return Path(root) if root else None

    def default_library(self) -> str:
        from creation_lib.mod_manager.locations import default_library_path

        return str(default_library_path())

    def refresh_modbox_profiles(self) -> None:
        from creation_lib.previs import modbox_profiles

        state = self.state
        try:
            state.modbox_profiles = modbox_profiles(Path(state.modbox_library or self.default_library()))
        except Exception as exc:  # noqa: BLE001
            state.modbox_profiles = []
            state.error_message = f"could not read the ModBox21 library: {exc}"
            return
        ids = [pid for pid, _ in state.modbox_profiles]
        if state.modbox_profile_id not in ids:
            state.modbox_profile_id = ids[0] if ids else ""

    def apply_settings(self, settings: dict) -> None:
        state = self.state
        state.source_kind = str(settings.get("source_kind") or state.source_kind)
        state.mo2_profile_dir = str(settings.get("mo2_profile_dir") or "")
        state.modbox_library = str(settings.get("modbox_library") or "")
        state.modbox_profile_id = str(settings.get("modbox_profile_id") or "")
        state.plugin_path = str(settings.get("plugin_path") or "")
        state.against = [str(p) for p in settings.get("against") or []]
        state.include_base_conflicts = bool(settings.get("include_base_conflicts", False))
        state.restore_disabled = bool(settings.get("restore_disabled", False))
        state.unprecombined_enabled = bool(settings.get("unprecombined_enabled", False))
        state.unprecombined_threshold = int(settings.get("unprecombined_threshold", 5) or 5)
        state.output_dir = str(settings.get("output_dir") or "")
        state.patch_name = str(settings.get("patch_name") or "")
        state.scope = str(settings.get("scope") or "broken")
        state.into_plugin = bool(settings.get("into_plugin", False))
        state.plugin_type = "esm" if settings.get("plugin_type") == "esm" else "esp"
        state.pack = bool(settings.get("pack", True))
        state.small_model_cull = bool(settings.get("small_model_cull", False))

    def collect_settings(self) -> dict:
        state = self.state
        return {
            "source_kind": state.source_kind,
            "mo2_profile_dir": state.mo2_profile_dir,
            "modbox_library": state.modbox_library,
            "modbox_profile_id": state.modbox_profile_id,
            "plugin_path": state.plugin_path,
            "against": list(state.against),
            "include_base_conflicts": state.include_base_conflicts,
            "restore_disabled": state.restore_disabled,
            "unprecombined_enabled": state.unprecombined_enabled,
            "unprecombined_threshold": state.unprecombined_threshold,
            "output_dir": state.output_dir,
            "patch_name": state.patch_name,
            "scope": state.scope,
            "into_plugin": state.into_plugin,
            "plugin_type": state.plugin_type,
            "pack": state.pack,
            "small_model_cull": state.small_model_cull,
        }

    def _build_spec(self):
        from creation_lib import previs

        state = self.state
        if state.source_kind == "modbox":
            return previs.from_modbox_profile(Path(state.modbox_library or self.default_library()), state.modbox_profile_id or None)
        game_root = self.game_root()
        if state.source_kind == "mo2":
            if not state.mo2_profile_dir:
                raise ValueError("choose an MO2 profile folder")
            return previs.from_mo2_profile(Path(state.mo2_profile_dir), game_root)
        if not state.plugin_path:
            raise ValueError("choose a plugin")
        if game_root is None:
            raise ValueError("set the Fallout 4 folder in Settings > Paths")
        return previs.from_plugin(Path(state.plugin_path), game_root, against=[Path(p) for p in state.against_paths()])

    def start_check(self) -> None:
        state = self.state
        if state.running:
            return
        state.error_message = ""
        state.build_log = []
        state.build_result = None
        state.running = True
        state.progress_frac = 0.0
        state.progress_msg = "reading load order"
        options = {
            "ignore_base_only": not state.include_base_conflicts,
            "restore_disabled": state.restore_disabled,
            "unprecombined_threshold": state.unprecombined_threshold if state.unprecombined_enabled else None,
            "rebuild_plugin": Path(state.plugin_path).name if state.single_plugin() and state.scope == "plugin" else None,
            "rebuild_all": state.scope == "all",
        }
        q = self._queue

        def _work() -> None:
            from creation_lib.previs import check_load_order

            try:
                spec = self._build_spec()
                q.put(("spec", spec.label, [p.name for p in spec.plugins], spec))
                stages = {"loading plugins": 0, "indexing previsbine assets": 1, "checking cells": 2}

                def progress(stage, done, total):
                    fraction = (stages.get(stage, 2) + (done / total if total else 0.0)) / 3.0
                    q.put(("progress", stage, fraction))

                q.put(("done", check_load_order(spec, progress=progress, **options)))
            except Exception as exc:  # noqa: BLE001
                q.put(("error", exc))

        self._thread = threading.Thread(target=_work, daemon=True, name="previs-check")
        self._thread.start()

    def rebuild_counts(self) -> tuple[int, int]:
        """Cells whose precombines are rebuilt, and every CELL the patch build copies."""
        from creation_lib.previs import select_cells

        if not self.state.report:
            return 0, 0
        precombine, seed = select_cells(self.state.report)
        return len(precombine), len(seed)

    def start_build(self) -> None:
        state = self.state
        if state.running or not state.report or self._spec is None:
            return
        if not state.output_dir:
            state.error_message = "choose an output folder for the patch"
            return
        state.error_message = ""
        state.build_log = []
        state.build_result = None
        state.running = True
        state.progress_frac = 0.0
        state.progress_msg = "building patch"
        spec, report = self._spec, state.report
        output = Path(state.output_dir)
        into_plugin = Path(state.plugin_path) if state.stamps_plugin() else None
        pack = state.pack
        small_model_cull = state.small_model_cull
        against = state.against_paths()
        required_masters = [Path(p).name for p in against] + [Path(state.plugin_path).name] if against else []
        q = self._queue

        def _work() -> None:
            from creation_lib import previs

            try:
                def progress(stage, done, total):
                    q.put(("progress", stage, done / total if total else 0.0))

                result = previs.build_patch(
                    spec,
                    report,
                    output,
                    work_dir=output.with_name(output.name + "-work"),
                    patch_name=state.patch_name.strip() or state.default_patch_name(previs.DEFAULT_PATCH_NAME),
                    into_plugin=into_plugin,
                    pack=pack,
                    plugin_type=state.plugin_type,
                    required_masters=required_masters,
                    small_model_cull=small_model_cull,
                    progress=progress,
                    log=lambda message: q.put(("log", message)),
                )
                q.put(("built", {
                    "patch": str(result.patch) if result.patch else "",
                    "archive": str(result.archive) if result.archive else "",
                    "cells": (result.patch_report or {}).get("cells", []),
                    "unchanged": (result.patch_report or {}).get("unchanged", []),
                    "added": (result.patch_report or {}).get("added_cells", []),
                    "skipped": result.skipped,
                    "after": (result.recheck or {}).get("summary"),
                }))
            except Exception as exc:  # noqa: BLE001
                q.put(("error", exc))

        self._thread = threading.Thread(target=_work, daemon=True, name="previs-build")
        self._thread.start()

    def poll(self) -> None:
        state = self.state
        while True:
            try:
                item = self._queue.get_nowait()
            except queue.Empty:
                break
            kind = item[0]
            if kind == "spec":
                state.load_order_label, state.plugins = item[1], item[2]
                self._spec = item[3]
            elif kind == "log":
                state.build_log.append(item[1])
            elif kind == "built":
                state.build_result = item[1]
                state.progress_msg, state.progress_frac = "patch built", 1.0
                state.running = False
            elif kind == "progress":
                state.progress_msg, state.progress_frac = item[1], float(item[2])
            elif kind == "done":
                state.report = item[1]
                state.selected_cell = state.selected_cluster = ""
                state.progress_msg, state.progress_frac = "done", 1.0
                state.running = False
            elif kind == "error":
                state.error_message = str(item[1])
                state.progress_msg = ""
                state.running = False

    def selected_cell(self) -> dict | None:
        report = self.state.report
        if not report or not self.state.selected_cell:
            return None
        return next((c for c in report["cells"] if c["form_key"] == self.state.selected_cell), None)

    def cluster(self, form_key: str | None) -> dict | None:
        report = self.state.report
        if not report or not form_key:
            return None
        return next((c for c in report["clusters"] if c["form_key"] == form_key), None)

    def cleanup(self) -> None:
        if self._thread is not None and self._thread.is_alive():
            self._thread.join(timeout=2.0)
