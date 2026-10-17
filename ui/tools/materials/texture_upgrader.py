"""Texture Upgrader — one mod's materials, textures and meshes to BGSM v22+, shipped the way the mod ships."""

from __future__ import annotations

from pathlib import Path

from imgui_bundle import imgui

from creation_lib.material_tools import texture_upgrader
from creation_lib.ui.widgets import pick_folder
from creation_lib.ui.widgets.forms import begin_form, draw_path_row, end_form
from ui.tools.base import BaseTool


def game_data_dir(toolkit_settings) -> str:
    """Fallout 4's Data folder from Settings → Paths, or "" when it is not set."""
    if toolkit_settings is None:
        return ""
    root = toolkit_settings.get_game_paths("fo4").get("root_dir") or ""
    return str(Path(root) / "Data") if root else ""


class TextureUpgraderTool(BaseTool):
    name = "Texture Upgrader"
    tool_id = "texture_upgrader"
    description = "Upgrade a mod's materials and textures to BGSM v22+ (loose stays loose, BA2s are repacked)"
    category = "Materials"

    def __init__(self):
        super().__init__()
        self._toolkit_settings = None
        self._mod_dir = ""
        self._data_dir = ""
        self._replace = False
        self._report: dict | None = None

    def get_default_settings(self) -> dict:
        return {"mod_dir": "", "replace": False}

    def apply_settings(self, settings: dict) -> None:
        self._mod_dir = settings.get("mod_dir", "")
        self._replace = bool(settings.get("replace", False))

    def collect_settings(self) -> dict:
        return {"mod_dir": self._mod_dir, "replace": self._replace}

    def draw_content(self) -> None:
        if not self._data_dir:
            self._data_dir = game_data_dir(self._toolkit_settings)
        imgui.text_wrapped("Select a mod folder (plugin with BA2s or loose files). Its materials, textures "
                           "and meshes are upgraded to BGSM v22+; loose files stay loose and BA2s are repacked.")
        if begin_form("##texture_upgrader"):
            self._mod_dir, clicked = draw_path_row("Mod folder", self._mod_dir)
            if clicked:
                path = pick_folder("Select the mod folder", self._mod_dir)
                if path:
                    self._mod_dir = path
            self._data_dir, clicked = draw_path_row("Fallout 4 Data", self._data_dir)
            if clicked:
                path = pick_folder("Select Fallout 4's Data folder", self._data_dir)
                if path:
                    self._data_dir = path
            end_form()
        _, self._replace = imgui.checkbox("Replace (update the mod's files and repack its BA2s)", self._replace)
        if not self._replace:
            imgui.text_disabled(f"Output: <mod>/{texture_upgrader.UPGRADED_DIR}")
        imgui.spacing()
        if not self._running and imgui.button("Upgrade", imgui.ImVec2(120, 0)):
            self._validate_and_start()
        self._draw_report()

    def _draw_report(self) -> None:
        report = self._report
        if not report:
            return
        summary = report["summary"]
        placed = report.get("placed") or {}
        imgui.spacing()
        imgui.text(f"{summary['upgrade']} upgraded, {summary['skipped']} skipped, "
                   f"{placed.get('loose', 0)} loose files, {len(placed.get('archives', []))} BA2s repacked")
        for name in placed.get("archives", []):
            imgui.bullet_text(name)
        errors = report["result"]["errors"]
        if errors and imgui.tree_node(f"Errors ({len(errors)})##errors"):
            for path, message in errors[:200]:
                imgui.text_wrapped(f"{path}: {message}")
            imgui.tree_pop()

    def _validate_and_start(self) -> None:
        if not self._mod_dir or not Path(self._mod_dir).is_dir():
            self._error_msg = "Select a mod folder."
            return
        if not self._data_dir or not Path(self._data_dir).is_dir():
            self._error_msg = "Set Fallout 4's Data folder (Settings → Paths, or pick it here)."
            return
        self._start_batch(self._run, self._mod_dir, self._data_dir, self._replace)

    def _run(self, mod_dir: str, data_dir: str, replace: bool) -> None:
        self._report = None

        def progress(done: int, total: int, message: str) -> bool:
            self._on_progress(done, total, message)
            return not self._cancel_requested

        report = texture_upgrader.upgrade_mod(mod_dir, data_dir, replace=replace, progress=progress)
        self._report = report
        if report["result"].get("cancelled"):
            self._result_msg = "Cancelled."
        else:
            self._result_msg = f"Upgraded {report['summary']['upgrade']} into {report['output_root']}."
