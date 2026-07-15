"""Image Utils tool — convert raster images to SVG or ICO."""

from __future__ import annotations

import io
import logging
import os
from enum import StrEnum
from pathlib import Path

from imgui_bundle import imgui

from ui.tools.base import BaseTool
from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import (
    begin_form,
    draw_combo_field,
    draw_path_row,
    end_form,
    pick_file,
    pick_save_file,
)

_log = logging.getLogger("tools.image_utils")

IMAGE_FILTERS = [("Images", "*.png *.jpg *.jpeg *.bmp *.tga *.webp"), ("All", "*.*")]
SUPPORTED_INPUT_EXTENSIONS = {".png", ".jpg", ".jpeg", ".bmp", ".tga", ".webp"}
ICON_SIZES = [16, 24, 32, 48, 64, 128, 256]


class ImageUtilsFormat(StrEnum):
    SVG = "svg"
    ICO = "ico"


def convert_image_to_ico(source: str | Path, output: str | Path, icon_size: int) -> Path:
    from PIL import Image

    source_path = Path(source)
    output_path = _with_suffix(Path(output), ".ico")
    output_path.parent.mkdir(parents=True, exist_ok=True)

    with Image.open(source_path) as image:
        icon = image.convert("RGBA").resize((icon_size, icon_size), Image.Resampling.LANCZOS)
        icon.save(output_path, format="ICO", sizes=[(icon_size, icon_size)])

    return output_path


def convert_image_to_svg(source: str | Path, output: str | Path) -> Path:
    import vtracer
    from PIL import Image

    source_path = Path(source)
    output_path = _with_suffix(Path(output), ".svg")
    output_path.parent.mkdir(parents=True, exist_ok=True)

    with Image.open(source_path) as image:
        normalized = image.convert("RGBA")
        png_bytes = io.BytesIO()
        normalized.save(png_bytes, format="PNG")

    svg = vtracer.convert_raw_image_to_svg(
        png_bytes.getvalue(),
        colormode="color",
        hierarchical="stacked",
        mode="spline",
        filter_speckle=4,
        color_precision=6,
        path_precision=8,
    )
    output_path.write_text(svg, encoding="utf-8")
    return output_path


def convert_images(
    input_path: str | Path,
    *,
    target_format: ImageUtilsFormat,
    output_dir: str | Path | None = None,
    output_file: str | Path | None = None,
    icon_size: int = 256,
    include_subdirs: bool = False,
) -> list[Path]:
    source = Path(input_path)
    if source.is_file():
        output = _resolve_single_output(source, target_format, output_dir, output_file)
        return [_convert_one(source, output, target_format, icon_size)]

    if not source.is_dir():
        raise ValueError(f"Input path does not exist: {source}")
    if output_file:
        raise ValueError("Output file can only be used with a single input image.")

    target_dir = Path(output_dir) if output_dir else source.with_name(f"{source.name}_converted")
    files = _collect_images(source, include_subdirs)
    written: list[Path] = []
    for image_path in files:
        rel_dir = image_path.parent.relative_to(source)
        output = target_dir / rel_dir / f"{image_path.stem}.{target_format.value}"
        written.append(_convert_one(image_path, output, target_format, icon_size))
    return written


def _convert_one(source: Path, output: Path, target_format: ImageUtilsFormat, icon_size: int) -> Path:
    if target_format == ImageUtilsFormat.ICO:
        return convert_image_to_ico(source, output, icon_size)
    if target_format == ImageUtilsFormat.SVG:
        return convert_image_to_svg(source, output)
    raise ValueError(f"Unsupported target format: {target_format}")


def _collect_images(source_dir: Path, include_subdirs: bool) -> list[Path]:
    pattern = "**/*" if include_subdirs else "*"
    return sorted(
        path
        for path in source_dir.glob(pattern)
        if path.is_file() and path.suffix.lower() in SUPPORTED_INPUT_EXTENSIONS
    )


def _resolve_single_output(
    source: Path,
    target_format: ImageUtilsFormat,
    output_dir: str | Path | None,
    output_file: str | Path | None,
) -> Path:
    if output_file:
        return _with_suffix(Path(output_file), f".{target_format.value}")
    target_dir = Path(output_dir) if output_dir else source.parent
    return target_dir / f"{source.stem}.{target_format.value}"


def _with_suffix(path: Path, suffix: str) -> Path:
    return path if path.suffix.lower() == suffix else path.with_suffix(suffix)


class ImageUtilsTool(BaseTool):
    name = "Image Utils"
    tool_id = "image_utils"
    description = "Convert images to SVG or ICO"
    category = "Image"

    def __init__(self):
        super().__init__()
        self._input_path = ""
        self._output_dir = ""
        self._output_file = ""
        self._format_idx = 0
        self._icon_size_idx = ICON_SIZES.index(256)
        self._include_subdirs = False

    def draw_content(self) -> None:
        if begin_form("##image_utils"):
            _, clicked = draw_path_row("Input", self._input_path)
            if clicked:
                path = pick_file("Select image", IMAGE_FILTERS)
                if not path:
                    path = pick_folder("Select image folder")
                if path:
                    self._input_path = path

            _, clicked = draw_path_row("Output Folder", self._output_dir)
            if clicked:
                path = pick_folder("Select output folder")
                if path:
                    self._output_dir = path

            _, clicked = draw_path_row("Output File", self._output_file)
            if clicked:
                ext = f".{self._target_format().value}"
                filetypes = [("Selected Format", f"*{ext}"), ("All", "*.*")]
                path = pick_save_file("Select output file", filetypes, default_ext=ext)
                if path:
                    self._output_file = path

            _, self._format_idx = draw_combo_field("Format", ["SVG", "ICO"], self._format_idx)
            if self._target_format() == ImageUtilsFormat.ICO:
                _, self._icon_size_idx = draw_combo_field(
                    "Icon Size",
                    [str(size) for size in ICON_SIZES],
                    self._icon_size_idx,
                )
            end_form()

        _, self._include_subdirs = imgui.checkbox("Include subdirectories", self._include_subdirs)
        imgui.text_disabled("Output file is used only for single-image input.")
        if self._output_dir and imgui.small_button("Clear Output Folder"):
            self._output_dir = ""
        if self._output_file:
            if self._output_dir:
                imgui.same_line()
            if imgui.small_button("Clear Output File"):
                self._output_file = ""

        imgui.spacing()
        imgui.separator()

        if not self._running:
            if imgui.button("Convert", imgui.ImVec2(120, 0)):
                self._validate_and_run()
        else:
            if imgui.button("Cancel", imgui.ImVec2(120, 0)):
                self._cancel_requested = True

    def _validate_and_run(self) -> None:
        if not self._input_path:
            self._error_msg = "Please select an input image or folder."
            return
        if self._output_file and os.path.isdir(self._input_path):
            self._error_msg = "Output file can only be used with a single input image."
            return
        self._start_batch(self._run_convert)

    def _run_convert(self) -> None:
        source = Path(self._input_path)
        files = [source] if source.is_file() else _collect_images(source, self._include_subdirs)
        if not files:
            self._result_msg = "No supported image files found."
            return

        written: list[Path] = []
        total = len(files)
        for i, file_path in enumerate(files):
            if self._cancel_requested:
                break
            self._on_progress(i, total, f"Converting: {file_path.name}")
            if source.is_file():
                outputs = convert_images(
                    source,
                    target_format=self._target_format(),
                    output_dir=self._output_dir or None,
                    output_file=self._output_file or None,
                    icon_size=self._icon_size(),
                )
                written.extend(outputs)
                break

            rel_dir = file_path.parent.relative_to(source)
            target_dir = Path(self._output_dir) if self._output_dir else source.with_name(f"{source.name}_converted")
            output = target_dir / rel_dir / f"{file_path.stem}.{self._target_format().value}"
            written.append(_convert_one(file_path, output, self._target_format(), self._icon_size()))

        self._on_progress(total, total, "Done")
        self._result_msg = f"Done: {len(written)} converted"

    def _target_format(self) -> ImageUtilsFormat:
        return ImageUtilsFormat.SVG if self._format_idx == 0 else ImageUtilsFormat.ICO

    def _icon_size(self) -> int:
        return ICON_SIZES[self._icon_size_idx]

    def get_default_settings(self) -> dict:
        return {"format": "svg", "icon_size": 256, "include_subdirs": False}

    def apply_settings(self, settings: dict) -> None:
        fmt = settings.get("format", "svg")
        self._format_idx = 0 if fmt == "svg" else 1
        icon_size = settings.get("icon_size", 256)
        self._icon_size_idx = ICON_SIZES.index(icon_size) if icon_size in ICON_SIZES else ICON_SIZES.index(256)
        self._include_subdirs = bool(settings.get("include_subdirs", False))

    def collect_settings(self) -> dict:
        return {
            "format": self._target_format().value,
            "icon_size": self._icon_size(),
            "include_subdirs": self._include_subdirs,
        }
