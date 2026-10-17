"""BGSM v22+ Upgrader — FO4 materials and texture sets to PBR (thin UI over material_upgrade_native)."""

from __future__ import annotations

import os

from imgui_bundle import imgui

from creation_lib.material_tools import upgrade_v22
from creation_lib.ui.widgets import pick_file, pick_folder
from creation_lib.ui.widgets.forms import begin_form, draw_combo_field, draw_path_row, end_form
from ui.tools.base import BaseTool

_METAL_LABELS = (
    "Off (all dielectric)",
    "Metal envmap materials",
    "Per-texel mask from _s (low precision)",
    "User masks (<diffuse stem>_metal.dds)",
)


def parse_worldspaces(text: str) -> list[str]:
    """Worldspace EditorIDs from a comma/space separated field; empty means every worldspace with land."""
    return [name for name in text.replace(",", " ").split() if name]


class MaterialV22UpgraderTool(BaseTool):
    name = "BGSM v22+ Upgrader"
    tool_id = "material_v22_upgrader"
    description = "Upgrade FO4 materials and texture sets to BGSM v22+ (PBR)"
    category = "Materials"

    def __init__(self):
        super().__init__()
        self._sources: list[str] = []
        self._output_dir = ""
        self._classes = {name: name in upgrade_v22.DEFAULT_CLASSES for name in upgrade_v22.CLASSES}
        self._metal_index = 0
        self._metal_mask_dir = ""
        self._smoothness_gain = 1.0
        self._use_knee = False
        self._knee = 0.5
        self._include_materials = True
        self._include_nifs = False
        self._include_terrain = False
        self._terrain_sidecars = True
        self._worldspaces = ""
        self._plugins: list[str] = []
        self._report: dict | None = None

    def get_default_settings(self) -> dict:
        return {"sources": [], "output_dir": ""}

    def apply_settings(self, settings: dict) -> None:
        self._sources = list(settings.get("sources", []))
        self._output_dir = settings.get("output_dir", "")

    def collect_settings(self) -> dict:
        return {"sources": list(self._sources), "output_dir": self._output_dir}

    def draw_content(self) -> None:
        self._draw_sources()
        imgui.separator()
        if begin_form("##v22_upgrader"):
            self._output_dir, clicked = draw_path_row("Output mod", self._output_dir)
            if clicked:
                path = pick_folder("Select output mod folder", self._output_dir)
                if path:
                    self._output_dir = path
            _, self._metal_index = draw_combo_field("Metal", list(_METAL_LABELS), self._metal_index)
            if upgrade_v22.METAL_MODES[self._metal_index] == "user-mask":
                self._metal_mask_dir, clicked = draw_path_row("Metal masks", self._metal_mask_dir)
                if clicked:
                    path = pick_folder("Select metal mask folder", self._metal_mask_dir)
                    if path:
                        self._metal_mask_dir = path
            end_form()

        imgui.text("Classes:")
        for index, name in enumerate(upgrade_v22.CLASSES):
            if index % 5:
                imgui.same_line()
            _, self._classes[name] = imgui.checkbox(f"{name.title()}##cls_{name}", self._classes[name])

        _, self._smoothness_gain = imgui.slider_float("Smoothness gain", self._smoothness_gain, 0.25, 2.0, "%.2f")
        _, self._use_knee = imgui.checkbox("Compress highlights above knee", self._use_knee)
        if self._use_knee:
            imgui.same_line()
            imgui.set_next_item_width(200)
            _, self._knee = imgui.slider_float("##knee", self._knee, 0.2, 0.9, "%.2f")

        _, self._include_materials = imgui.checkbox("Materials (.bgsm)", self._include_materials)
        imgui.same_line()
        _, self._include_nifs = imgui.checkbox("NIF texture sets", self._include_nifs)
        imgui.same_line()
        _, self._include_terrain = imgui.checkbox("Terrain layers (LTEX)", self._include_terrain)
        if self._include_terrain:
            _, self._terrain_sidecars = imgui.checkbox(
                "Write worldspace terrain files (terrain/fo4cs/<worldspace>.btd4)", self._terrain_sidecars)
            if self._terrain_sidecars:
                imgui.set_next_item_width(320)
                _, self._worldspaces = imgui.input_text("Worldspaces (EditorIDs, blank = all)", self._worldspaces)
            self._draw_plugins()

        imgui.spacing()
        imgui.separator()
        if not self._running:
            if imgui.button("Dry Run", imgui.ImVec2(120, 0)):
                self._validate_and_start(dry_run=True)
            imgui.same_line()
            if imgui.button("Upgrade", imgui.ImVec2(120, 0)):
                self._validate_and_start(dry_run=False)
        self._draw_report()

    def _draw_sources(self) -> None:
        imgui.text("Sources (Data folder, mod folders or .ba2; later ones override earlier ones):")
        remove = None
        for index, source in enumerate(self._sources):
            if imgui.small_button(f"x##src_{index}"):
                remove = index
            imgui.same_line()
            imgui.text(source)
        if remove is not None:
            self._sources.pop(remove)
        if imgui.button("Add Folder"):
            path = pick_folder("Select a Data or mod folder")
            if path:
                self._sources.append(path)
        imgui.same_line()
        if imgui.button("Add BA2"):
            path = pick_file("Select a BA2 archive", [("BA2 archives", "*.ba2")])
            if path:
                self._sources.append(path)

    def _draw_plugins(self) -> None:
        imgui.text_disabled("Plugins whose LTEX/TXST records name the terrain layers (load order):")
        remove = None
        for index, plugin in enumerate(self._plugins):
            if imgui.small_button(f"x##plg_{index}"):
                remove = index
            imgui.same_line()
            imgui.text(plugin)
        if remove is not None:
            self._plugins.pop(remove)
        if imgui.button("Add Plugin"):
            path = pick_file("Select a plugin", [("Plugins", "*.esm *.esp *.esl")])
            if path:
                self._plugins.append(path)

    def _draw_report(self) -> None:
        report = self._report
        if not report:
            return
        summary = report["summary"]
        imgui.spacing()
        imgui.text(f"{'Dry run' if report['dry_run'] else 'Upgrade'}: {summary['upgrade']} upgraded, "
                   f"{summary['skipped']} skipped, {summary['texture_outputs']} textures, "
                   f"{summary['warnings']} warnings")
        for title, key in (("By class", "by_class"), ("By kind", "by_kind"), ("Skip reasons", "skip_reasons")):
            if summary[key] and imgui.tree_node(f"{title}##{key}"):
                for name, count in summary[key].items():
                    imgui.text(f"{name}: {count}")
                imgui.tree_pop()
        result = report.get("result")
        if result and result["errors"] and imgui.tree_node(f"Errors ({len(result['errors'])})##errors"):
            for path, message in result["errors"][:200]:
                imgui.text_wrapped(f"{path}: {message}")
            imgui.tree_pop()

    def _options(self) -> upgrade_v22.UpgradeOptions:
        return upgrade_v22.UpgradeOptions(
            sources=list(self._sources),
            output=self._output_dir,
            classes=[name for name, on in self._classes.items() if on],
            metal_mode=upgrade_v22.METAL_MODES[self._metal_index],
            metal_mask_dir=self._metal_mask_dir or None,
            smoothness_gain=self._smoothness_gain,
            knee=self._knee if self._use_knee else None,
            include_materials=self._include_materials,
            include_nifs=self._include_nifs,
            include_terrain=self._include_terrain,
            terrain_sidecars=self._terrain_sidecars,
            terrain_worldspaces=parse_worldspaces(self._worldspaces),
            plugins=list(self._plugins),
            workers=max(1, min(8, (os.cpu_count() or 4) // 2)),
        )

    def _validate_and_start(self, dry_run: bool) -> None:
        if not self._sources:
            self._error_msg = "Add at least one source folder or BA2."
            return
        if not dry_run and not self._output_dir:
            self._error_msg = "Select an output mod folder."
            return
        self._start_batch(self._run, self._options(), dry_run)

    def _run(self, options: upgrade_v22.UpgradeOptions, dry_run: bool) -> None:
        self._report = None
        if dry_run:
            self._status_msg = "Planning..."
            report = upgrade_v22.plan(options)
        else:
            def progress(done: int, total: int, message: str) -> bool:
                self._on_progress(done, total, message)
                return not self._cancel_requested

            report = upgrade_v22.run(options, progress)
        self._report = report
        summary = report["summary"]
        if report.get("result", {}).get("cancelled"):
            self._result_msg = "Cancelled."
        elif dry_run:
            self._result_msg = f"Dry run: {summary['upgrade']} would be upgraded, {summary['skipped']} skipped."
        else:
            self._result_msg = (f"Upgraded {summary['upgrade']} into {options.output} "
                                f"(report: materialupgrade/upgrade_report.json).")
