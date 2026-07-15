"""Apply panel — batch run pose deltas onto every animation in the input folder."""

from __future__ import annotations

import logging
import threading
from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ..bone_editor_app import BoneEditorApp

_log = logging.getLogger("bone_editor.apply_panel")


class ApplyPanel:
    def __init__(self, app: "BoneEditorApp"):
        self.app = app
        self._dry_run = False
        self._running = False
        self._progress = 0.0
        self._current_file = ""
        self._results_text = ""
        self._failed: list[tuple[str, str]] = []
        self._show_failed = False
        self._thread: threading.Thread | None = None

    def draw(self) -> None:
        visible, _ = imgui.begin("Apply##bone_editor")
        if not visible:
            imgui.end()
            return

        sess = self.app.pose_session
        if sess is None:
            imgui.text_colored(imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                               "Load a skeleton first")
            imgui.end()
            return

        edited = sorted(sess.pose.edited_bones())
        imgui.text(f"Edited bones: {len(edited)}")
        if edited:
            imgui.text_disabled("  " + ", ".join(edited[:6])
                                + ("..." if len(edited) > 6 else ""))

        anim_folder = self.app.setup_panel.anim_folder if self.app.setup_panel else None
        out_folder = self.app.setup_panel.output_folder if self.app.setup_panel else None
        n_anims = self.app.setup_panel._anim_file_count if self.app.setup_panel else 0
        imgui.text(f"Animations: {n_anims}")

        imgui.spacing()
        _, self._dry_run = imgui.checkbox("Dry run", self._dry_run)

        can_apply = (
            not self._running and len(edited) > 0
            and anim_folder is not None and out_folder is not None
        )

        if not can_apply:
            imgui.begin_disabled()
        if imgui.button("Apply to All", imgui.ImVec2(-1, 0)):
            self._start_batch(anim_folder, out_folder)
        if not can_apply:
            imgui.end_disabled()

        if self._running:
            imgui.progress_bar(self._progress, imgui.ImVec2(-1, 0))
            if self._current_file:
                imgui.text(self._current_file)

        if self._results_text:
            imgui.spacing()
            imgui.text_wrapped(self._results_text)

        if self._failed:
            imgui.spacing()
            expanded = imgui.collapsing_header(
                f"Failed files ({len(self._failed)})##fail",
            )
            if isinstance(expanded, tuple):
                expanded = expanded[0]
            if expanded:
                for fname, msg in self._failed:
                    imgui.text_colored(imgui.ImVec4(1.0, 0.4, 0.4, 1.0), f"  {fname}")
                    imgui.indent(24)
                    imgui.text_wrapped(f"— {msg}")
                    imgui.unindent(24)

        imgui.end()

    def _start_batch(self, anim_folder, out_folder) -> None:
        from creation_lib.bone_edit.apply_pose import apply_pose_to_folder

        skel_path = (
            self.app.setup_panel._skeleton_path.strip()
            if self.app.setup_panel else ""
        )
        if not skel_path:
            self._results_text = "No skeleton path"
            return

        from pathlib import Path
        pose = self.app.pose_session.pose

        self._running = True
        self._progress = 0.0
        self._current_file = ""
        self._failed = []
        self._results_text = ""

        def _cb(cur, total, name):
            self._progress = cur / max(total, 1)
            self._current_file = name

        recursive = bool(self.app.setup_panel and self.app.setup_panel._recursive)

        def _worker():
            try:
                results = apply_pose_to_folder(
                    pose=pose,
                    skeleton_hkx_path=Path(skel_path),
                    animation_folder=anim_folder,
                    output_folder=out_folder,
                    dry_run=self._dry_run,
                    progress_callback=_cb,
                    recursive=recursive,
                )
                successes = sum(1 for r in results if r.success)
                failures = len(results) - successes
                self._failed = [(r.filename, r.message)
                                for r in results if not r.success]
                for fname, msg in self._failed:
                    _log.warning("apply failed: %s — %s", fname, msg)
                mode = "DRY RUN" if self._dry_run else "Done"
                self._results_text = (
                    f"{mode}: {successes}/{len(results)} files processed"
                    + (f" ({failures} failed)" if failures else "")
                )
            except Exception as e:
                _log.exception("Batch apply failed")
                self._results_text = f"Error: {e}"
            finally:
                self._running = False
                self._current_file = ""

        self._thread = threading.Thread(target=_worker, daemon=True)
        self._thread.start()
