"""Material File Copier tool — bulk copy BGSM/BGEM with texture path remapping."""

from __future__ import annotations

import logging
import os

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import begin_form, end_form, draw_path_row, draw_text_field, draw_float_field

_log = logging.getLogger("tools.material_copier")


class MaterialCopierTool(BaseTool):
    name = "Material Copier"
    tool_id = "material_copier"
    description = "Bulk copy BGSM/BGEM files"
    category = "Materials"

    def __init__(self):
        super().__init__()
        self._input_dir = ""
        self._output_dir = ""
        self._folders_csv = ""
        self._excludes_csv = ""
        self._include_bgsm = True
        self._include_bgem = True
        self._clean_output = False
        self._grayscale_scale = 0.0

        # Texture field toggles
        self._tex_diffuse = True
        self._tex_normal = True
        self._tex_smoothspec = True
        self._tex_greyscale = True
        self._tex_envmap = False
        self._tex_glow = False
        self._tex_inner = False
        self._tex_wrinkles = False

    def draw_content(self) -> None:
        if begin_form("##bulk_copier"):
            _, clicked = draw_path_row("Source", self._input_dir)
            if clicked:
                path = pick_folder("Select source directory with BGSM/BGEM files")
                if path:
                    self._input_dir = path

            _, clicked = draw_path_row("Output", self._output_dir)
            if clicked:
                path = pick_folder("Select output root (optional)")
                if path:
                    self._output_dir = path

            _, self._folders_csv = draw_text_field("Folder Names (CSV)", self._folders_csv)
            _, self._excludes_csv = draw_text_field("Exclude (CSV)", self._excludes_csv)
            _, self._grayscale_scale = draw_float_field("Grayscale Scale", self._grayscale_scale, 0.1, 1.0, "%.2f")
            end_form()

        imgui.text_disabled("Leave empty to write next to input")
        imgui.text_disabled("e.g. bos, enclave, gold")
        imgui.separator()

        _, self._include_bgsm = imgui.checkbox("Include BGSM", self._include_bgsm)
        imgui.same_line()
        _, self._include_bgem = imgui.checkbox("Include BGEM", self._include_bgem)
        _, self._clean_output = imgui.checkbox("Clean target folders first", self._clean_output)

        imgui.spacing()
        imgui.text("Texture fields to remap:")
        _, self._tex_diffuse = imgui.checkbox("Diffuse", self._tex_diffuse)
        imgui.same_line()
        _, self._tex_normal = imgui.checkbox("Normal", self._tex_normal)
        imgui.same_line()
        _, self._tex_smoothspec = imgui.checkbox("SmoothSpec", self._tex_smoothspec)
        imgui.same_line()
        _, self._tex_greyscale = imgui.checkbox("Greyscale", self._tex_greyscale)

        _, self._tex_envmap = imgui.checkbox("Envmap", self._tex_envmap)
        imgui.same_line()
        _, self._tex_glow = imgui.checkbox("Glow", self._tex_glow)
        imgui.same_line()
        _, self._tex_inner = imgui.checkbox("InnerLayer", self._tex_inner)
        imgui.same_line()
        _, self._tex_wrinkles = imgui.checkbox("Wrinkles", self._tex_wrinkles)

        imgui.spacing()
        imgui.separator()

        if not self._running:
            if imgui.button("Run", imgui.ImVec2(120, 0)):
                self._validate_and_run()
        else:
            if imgui.button("Cancel", imgui.ImVec2(120, 0)):
                self._cancel_requested = True

    def _validate_and_run(self):
        if not self._input_dir or not os.path.isdir(self._input_dir):
            self._error_msg = "Please select a valid source directory."
            return
        if not self._folders_csv.strip():
            self._error_msg = "Please enter at least one folder name."
            return
        if not self._include_bgsm and not self._include_bgem:
            self._error_msg = "Please enable at least one of BGSM or BGEM."
            return
        self._start_batch(self._run_copy)

    def _get_selected_paths(self) -> set[str] | None:
        paths = set()
        if self._tex_diffuse:
            paths.add("DiffuseTexture")
        if self._tex_normal:
            paths.add("NormalTexture")
        if self._tex_smoothspec:
            paths.add("SmoothSpecTexture")
        if self._tex_greyscale:
            paths.add("GreyscaleTexture")
        if self._tex_envmap:
            paths.add("EnvmapTexture")
        if self._tex_glow:
            paths.add("GlowTexture")
        if self._tex_inner:
            paths.add("InnerLayerTexture")
        if self._tex_wrinkles:
            paths.add("WrinklesTexture")
        return paths if paths else None

    def _run_copy(self):
        # Import the existing material processing logic
        try:
            from ui.tools.materials._matfiles_logic import run as run_matfiles
        except ImportError:
            # Fallback: try the inline implementation
            run_matfiles = self._run_inline
            _log.info("Using inline material copy logic")

        folders = [f.strip() for f in self._folders_csv.split(",") if f.strip()]
        excludes = [p.strip() for p in self._excludes_csv.split(",") if p.strip()] if self._excludes_csv.strip() else []
        out_root = self._output_dir.strip() or None
        selected = self._get_selected_paths()
        gs_scale = self._grayscale_scale if self._grayscale_scale > 0 else None

        log_lines = []

        def logger_fn(msg):
            log_lines.append(msg)
            _log.debug(msg)

        try:
            run_matfiles(
                input_dir=self._input_dir,
                folders=folders,
                out_root=out_root,
                include_bgsm=self._include_bgsm,
                include_bgem=self._include_bgem,
                selected_paths=selected,
                exclude_patterns=excludes,
                grayscale_to_palette_scale=gs_scale,
                clean_output=self._clean_output,
                logger=logger_fn,
            )
            self._result_msg = f"Done. Folders: {', '.join(folders)}"
        except Exception as e:
            self._error_msg = f"Material copy failed: {e}"

    def _run_inline(self, **kwargs):
        """Inline fallback using the matfiles_copy module's run function."""
        import importlib
        for module_path in [
            "creation_lib.material_tools.mass_edit_materials",
        ]:
            try:
                mod = importlib.import_module(module_path)
                if hasattr(mod, "run"):
                    mod.run(**kwargs)
                    return
            except ImportError:
                continue
        raise ImportError("Could not find material processing module")

    def get_default_settings(self) -> dict:
        return {
            "include_bgsm": True,
            "include_bgem": True,
            "clean_output": False,
        }

    def apply_settings(self, settings: dict) -> None:
        self._include_bgsm = settings.get("include_bgsm", True)
        self._include_bgem = settings.get("include_bgem", True)
        self._clean_output = settings.get("clean_output", False)

    def collect_settings(self) -> dict:
        return {
            "include_bgsm": self._include_bgsm,
            "include_bgem": self._include_bgem,
            "clean_output": self._clean_output,
        }
