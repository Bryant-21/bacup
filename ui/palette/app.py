"""PaletteApp — two-tab panel for FO4 remap + gradient texture generation."""
from __future__ import annotations

import logging
import os
from pathlib import Path
import colorsys

import moderngl
import numpy as np
from imgui_bundle import imgui

from creation_lib.ui.widgets import pick_folder
from ui.tools.imgui_helpers import draw_path_row, pick_file, begin_form, end_form

_log = logging.getLogger("palette.app")
_NS = "##palette_app"


class PaletteApp:
    """Core state and rendering for the Palette workspace."""

    def __init__(self):
        # Settings-backed state
        self._source_path = ""
        self._output_path = ""
        self._n_zones = 6
        self._gradient_width = 32
        self._active_tab = 0  # 0=Auto, 1=Manual, 2=Variants, 3=Adjust

        # Runtime state
        self._result = None       # PaletteResult | None
        self._running = False
        self._progress = 0.0
        self._status_msg = ""
        self._error_msg = ""

        # Debug + preview state
        self._debug_preview: bool = False
        self._banded: bool = False   # only used in debug full-gradient view
        self._paint_index: float = 1.0
        self._tex_applied = None    # moderngl.Texture | None
        self._applied_dirty: bool = False
        self._remap_small = None    # np.ndarray | None

        # Manual mode state
        self._manual_zones: list = []

        # Variant mode state
        self._variant_remap: np.ndarray | None = None
        self._variant_zones: list | None = None
        self._variants: list[dict] = []
        self._variant_gradient: np.ndarray | None = None
        self._remap_path: str = ""
        self._tex_var_gradient = None
        self._var_band_index: int = 0
        self._variant_output_path: str = ""
        self._tex_var_applied = None   # moderngl.Texture for variant applied preview

        # HSV adjustment mode state
        self._adjust_palette: np.ndarray | None = None
        self._adjust_palette_path: str = ""
        self._adjust_output_path: str = ""
        self._adjust_base_index: int = 0
        self._adjust_hue_shift: float = 0.0
        self._adjust_sat_scale: float = 1.0
        self._adjust_vibrance: float = 0.0
        self._adjust_val_scale: float = 1.0
        self._adjust_temperature: float = 0.0
        self._adjust_tint: float = 0.0
        self._adjust_brightness: float = 0.0
        self._adjust_contrast: float = 1.0
        self._adjust_luminosity: float = 0.0
        self._adjust_exposure: float = 0.0
        self._adjust_gamma: float = 1.0
        self._adjust_input_black: int = 0
        self._adjust_input_white: int = 255
        self._adjust_output_black: int = 0
        self._adjust_output_white: int = 255
        self._adjust_overlay_strength: float = 0.0
        self._adjust_overlay_color: list[float] = [255.0, 255.0, 255.0]
        self._adjust_overlay_mode: int = 0
        self._adjust_preview: np.ndarray | None = None
        self._adjust_dirty: bool = True
        self._tex_adjust_original = None
        self._tex_adjust_preview = None
        self._tex_adjust_applied = None

        # GL textures
        self._tex_source = None
        self._tex_remap = None
        self._tex_gradient = None
        self._tex_debug_gradient = None  # full 32-row gradient (debug only)
        self._tex_dirty = False

    # ------------------------------------------------------------------
    # Settings round-trip
    # ------------------------------------------------------------------

    def apply_settings(self, settings: dict) -> None:
        self._source_path = settings.get("last_source_path", "")
        self._output_path = settings.get("last_output_path", "")
        self._n_zones = settings.get("n_zones", 6)
        self._gradient_width = settings.get("gradient_width", 32)
        tab = settings.get("active_tab", "auto")
        self._active_tab = {"auto": 0, "manual": 1, "variants": 2, "adjust": 3}.get(tab, 0)
        self._debug_preview = settings.get("debug_preview", False)
        self._banded = settings.get("gradient_banded", False)
        self._paint_index = settings.get("paint_index", 1.0)
        self._remap_path = settings.get("variant_remap_path", "")
        self._variant_output_path = settings.get("variant_output_path", "")
        self._adjust_palette_path = settings.get("adjust_palette_path", "")
        self._adjust_output_path = settings.get("adjust_output_path", "")

    def collect_settings(self) -> dict:
        tab_names = {0: "auto", 1: "manual", 2: "variants", 3: "adjust"}
        return {
            "last_source_path": self._source_path,
            "last_output_path": self._output_path,
            "n_zones": self._n_zones,
            "gradient_width": self._gradient_width,
            "active_tab": tab_names.get(self._active_tab, "auto"),
            "debug_preview": self._debug_preview,
            "gradient_banded": self._banded,
            "paint_index": self._paint_index,
            "variant_remap_path": self._remap_path,
            "variant_output_path": self._variant_output_path,
            "adjust_palette_path": self._adjust_palette_path,
            "adjust_output_path": self._adjust_output_path,
        }

    # ------------------------------------------------------------------
    # Main draw
    # ------------------------------------------------------------------

    # Zone max lookup by gradient width
    _ZONE_MAX = {32: 14, 64: 24, 128: 32}

    def draw(self) -> None:
        # --- Top row: source picker ---
        clicked = False
        if begin_form("##pal_src_form"):
            _, clicked = draw_path_row("Source", self._source_path)
            end_form()
        if clicked:
            path = pick_file(
                "Select source texture",
                [("Images", "*.png *.jpg *.jpeg *.dds *.bmp *.tga"), ("All", "*.*")],
            )
            if path:
                self._source_path = path
                self._result = None
                self._tex_dirty = True

        # --- Width + Zones row ---
        imgui.text("Width")
        imgui.same_line()
        for w in (32, 64, 128):
            if imgui.radio_button(f"{w}##w{w}", self._gradient_width == w):
                self._gradient_width = w
                zone_max = self._ZONE_MAX[w]
                if self._n_zones > zone_max:
                    self._n_zones = zone_max
                self._result = None
                self._tex_dirty = True
            imgui.same_line()

        imgui.same_line()
        imgui.text("Zones")
        imgui.same_line()
        imgui.set_next_item_width(120)
        zone_max = self._ZONE_MAX.get(self._gradient_width, 12)
        _, self._n_zones = imgui.slider_int(f"##nzones{_NS}", self._n_zones, 2, zone_max)


        imgui.spacing()

        # --- Tab bar ---
        if imgui.begin_tab_bar(f"palette_tabs{_NS}"):
            if imgui.begin_tab_item("Auto")[0]:
                self._active_tab = 0
                self._draw_auto_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Manual")[0]:
                self._active_tab = 1
                self._draw_manual_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Variants")[0]:
                self._active_tab = 2
                self._draw_variants_tab()
                imgui.end_tab_item()
            if imgui.begin_tab_item("Adjust")[0]:
                self._active_tab = 3
                self._draw_adjust_tab()
                imgui.end_tab_item()
            imgui.end_tab_bar()

        # --- Always-visible save + preview (Auto / Manual result) ---
        imgui.spacing()
        imgui.separator()
        self._draw_save_row()
        imgui.separator()
        self._draw_preview()

    # ------------------------------------------------------------------
    # Auto tab
    # ------------------------------------------------------------------

    def _draw_auto_tab(self) -> None:
        # Generate button
        if not self._running:
            if imgui.button(f"Generate{_NS}", imgui.ImVec2(120, 0)):
                self._run_auto()
        else:
            imgui.progress_bar(self._progress, imgui.ImVec2(-1, 0), self._status_msg)

        if self._error_msg:
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1, 0.3, 0.3, 1))
            imgui.text_wrapped(self._error_msg)
            imgui.pop_style_color()

    # ------------------------------------------------------------------
    # Manual tab
    # ------------------------------------------------------------------

    def _draw_manual_tab(self) -> None:
        if not self._manual_zones and self._source_path and os.path.isfile(self._source_path):
            if imgui.button(f"Detect Zones{_NS}"):
                self._detect_manual_zones()

        if self._manual_zones:
            self._draw_zone_list()
            imgui.spacing()
            if imgui.button(f"Apply & Generate{_NS}", imgui.ImVec2(160, 0)):
                self._run_manual()

        if self._error_msg:
            imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1, 0.3, 0.3, 1))
            imgui.text_wrapped(self._error_msg)
            imgui.pop_style_color()

    def _draw_zone_list(self) -> None:
        imgui.text("Zones (drag to reorder):")
        to_remove = None
        to_swap = None
        for i, zone in enumerate(self._manual_zones):
            color = zone["avg_color"] / 255.0
            imgui.color_button(
                f"##swatch{i}{_NS}",
                imgui.ImVec4(float(color[0]), float(color[1]), float(color[2]), 1.0),
                imgui.ColorEditFlags_.no_tooltip,
                imgui.ImVec2(16, 16),
            )
            imgui.same_line()
            imgui.text(f"Zone {i}")
            imgui.same_line()
            if i > 0:
                if imgui.small_button(f"Up{_NS}{i}"):
                    to_swap = (i - 1, i)
            imgui.same_line()
            if i < len(self._manual_zones) - 1:
                if imgui.small_button(f"Down{_NS}{i}"):
                    to_swap = (i, i + 1)
            imgui.same_line()
            if imgui.small_button(f"X{_NS}{i}"):
                to_remove = i

        if to_swap:
            a, b = to_swap
            self._manual_zones[a], self._manual_zones[b] = self._manual_zones[b], self._manual_zones[a]
        if to_remove is not None:
            self._manual_zones.pop(to_remove)

    # ------------------------------------------------------------------
    # Variants tab
    # ------------------------------------------------------------------

    def _draw_variants_tab(self) -> None:
        # --- Remap source section ---
        imgui.text("Remap Source")
        has_result = self._result is not None and self._result.zones
        if not has_result:
            imgui.begin_disabled()
        if imgui.button(f"Use Current Result{_NS}var_use"):
            self._variant_remap = self._result.remap.copy()
            self._variant_zones = list(self._result.zones)
            self._remap_path = "(from current result)"
            self._variants.clear()
            self._variant_gradient = None
        if not has_result:
            imgui.end_disabled()

        imgui.same_line()
        if imgui.button(f"Load Remap PNG{_NS}var_load"):
            path = pick_file(
                "Select remap texture",
                [("PNG", "*.png"), ("All", "*.*")],
            )
            if path:
                self._load_remap_from_file(path)

        if self._variant_remap is not None:
            imgui.same_line()
            imgui.text_colored(
                imgui.ImVec4(0.5, 1.0, 0.5, 1.0),
                f"Loaded — {len(self._variant_zones or [])} zones",
            )
        else:
            imgui.text_disabled("No remap loaded. Generate a palette first, or load a remap PNG.")

        imgui.spacing()
        imgui.separator()

        # --- Variant list ---
        if self._variant_remap is not None:
            imgui.text("Variants")
            self._draw_variant_list()

            imgui.spacing()
            if imgui.button(f"Add Variant{_NS}var_add"):
                path = pick_file(
                    "Select recolored texture",
                    [("Images", "*.png *.jpg *.jpeg *.dds *.bmp *.tga"), ("All", "*.*")],
                )
                if path:
                    self._add_variant(path)

            max_variants = 128 // 4  # 32
            if len(self._variants) >= max_variants:
                imgui.same_line()
                imgui.text_colored(imgui.ImVec4(1, 0.6, 0.3, 1), f"Max {max_variants} variants")

            imgui.spacing()
            imgui.separator()

            # --- Build & Preview ---
            if len(self._variants) > 0:
                if imgui.button(f"Build Gradient{_NS}var_build", imgui.ImVec2(160, 0)):
                    self._build_variant_gradient()

                imgui.spacing()
                self._draw_variant_preview()

                imgui.separator()
                self._draw_variant_save_row()

    def _draw_variant_list(self) -> None:
        to_remove = None
        to_swap = None
        for i, var in enumerate(self._variants):
            imgui.push_id(f"var_{i}")
            # Color swatches (one per zone)
            colors = var.get("colors", [])
            for ci, c in enumerate(colors):
                rgb = np.clip(c, 0, 255) / 255.0
                imgui.color_button(
                    f"##vs{ci}",
                    imgui.ImVec4(float(rgb[0]), float(rgb[1]), float(rgb[2]), 1.0),
                    imgui.ColorEditFlags_.no_tooltip,
                    imgui.ImVec2(14, 14),
                )
                imgui.same_line()

            name = var.get("name", f"Variant {i}")
            imgui.text(name)
            imgui.same_line()

            if i > 0:
                if imgui.small_button("Up"):
                    to_swap = (i - 1, i)
                imgui.same_line()
            if i < len(self._variants) - 1:
                if imgui.small_button("Down"):
                    to_swap = (i, i + 1)
                imgui.same_line()
            if imgui.small_button("X"):
                to_remove = i

            imgui.pop_id()

        if to_swap:
            a, b = to_swap
            self._variants[a], self._variants[b] = self._variants[b], self._variants[a]
        if to_remove is not None:
            self._variants.pop(to_remove)

    def _load_remap_from_file(self, path: str) -> None:
        try:
            from PIL import Image
            from creation_lib.palette.remap import zones_from_remap
            img = Image.open(path).convert("L")
            remap = np.array(img, dtype=np.uint8)
            zones = zones_from_remap(remap, width=self._gradient_width)
            self._variant_remap = remap
            self._variant_zones = zones
            self._remap_path = path
            self._variants.clear()
            self._variant_gradient = None
        except Exception as exc:
            _log.exception("Failed to load remap")
            self._error_msg = str(exc)

    def _add_variant(self, path: str) -> None:
        if self._variant_remap is None or not self._variant_zones:
            return
        try:
            from PIL import Image
            from creation_lib.dds import load_image
            from creation_lib.palette.remap import sample_variant_colors
            img = load_image(path) or Image.open(path)
            colors = sample_variant_colors(img, self._variant_remap, self._variant_zones)
            name = Path(path).stem
            self._variants.append({"name": name, "path": path, "colors": colors})
        except Exception as exc:
            _log.exception("Failed to add variant")
            self._error_msg = str(exc)

    def _build_variant_gradient(self) -> None:
        if not self._variants or not self._variant_zones:
            return
        try:
            from creation_lib.palette.remap import build_variant_gradient
            variant_colors = [v["colors"] for v in self._variants]
            zone_columns = [z.gradient_column for z in self._variant_zones]
            self._variant_gradient = build_variant_gradient(
                variant_colors, zone_columns, band_height=4, width=self._gradient_width,
            )
            # Upload texture
            if self._tex_var_gradient is not None:
                try:
                    self._tex_var_gradient.release()
                except Exception:
                    pass
            from PIL import Image as PILImage
            grad_pil = PILImage.fromarray(self._variant_gradient, mode="RGBA")
            # Scale up for visibility: width→128, height proportional
            h, w = self._variant_gradient.shape[:2]
            scale = max(1, 128 // w)
            grad_big = grad_pil.resize((w * scale, h * scale), PILImage.NEAREST)
            self._tex_var_gradient = self._upload_texture(np.array(grad_big, dtype=np.uint8))
            self._compute_variant_applied()
        except Exception as exc:
            _log.exception("Failed to build variant gradient")
            self._error_msg = str(exc)

    def _compute_variant_applied(self) -> None:
        """Compute applied preview for the currently selected variant band."""
        if self._variant_gradient is None or self._variant_remap is None:
            return
        band_idx = self._var_band_index
        h_grad = self._variant_gradient.shape[0]
        n_bands = h_grad // 4
        if band_idx >= n_bands:
            return

        row = band_idx * 4  # pick first row of the band (all 4 are identical)
        max_col = self._gradient_width - 1
        col_map = np.round(
            self._variant_remap.astype(np.float32) / 255.0 * max_col
        ).astype(np.int32)
        col_map = np.clip(col_map, 0, max_col)

        applied_rgba = self._variant_gradient[row, col_map, :]

        if self._tex_var_applied is not None:
            try:
                self._tex_var_applied.release()
            except Exception:
                pass
        self._tex_var_applied = self._upload_texture(applied_rgba.astype(np.uint8))

    def _draw_variant_preview(self) -> None:
        if self._variant_gradient is None:
            imgui.text_disabled("Press Build Gradient to generate")
            return

        h, w = self._variant_gradient.shape[:2]
        n_bands = h // 4

        # Band selector
        imgui.text("Band")
        imgui.same_line()
        imgui.set_next_item_width(200)
        changed, self._var_band_index = imgui.slider_int(
            f"##var_band{_NS}", self._var_band_index, 0, max(0, n_bands - 1),
        )
        if self._var_band_index < len(self._variants):
            var = self._variants[self._var_band_index]
            imgui.same_line()
            imgui.text(var.get("name", ""))

        if changed:
            self._compute_variant_applied()

        # Side-by-side: variant gradient (left) + applied preview (right)
        preview_h = 128.0
        avail_w = imgui.get_content_region_avail().x
        half_w = max(64.0, (avail_w - 8) / 2.0)

        if self._tex_var_gradient is not None:
            imgui.image(
                imgui.ImTextureRef(self._tex_var_gradient.glo),
                imgui.ImVec2(half_w, preview_h),
            )
            imgui.same_line()

        if self._tex_var_applied is not None:
            imgui.image(
                imgui.ImTextureRef(self._tex_var_applied.glo),
                imgui.ImVec2(half_w, preview_h),
            )
        elif self._variant_remap is not None:
            imgui.text_disabled("Select a band to preview")

    def _draw_variant_save_row(self) -> None:
        clicked = False
        if begin_form("##var_output_form"):
            _, clicked = draw_path_row("Output", self._variant_output_path)
            end_form()
        if clicked:
            path = pick_folder("Select output folder")
            if path:
                self._variant_output_path = path

        if self._variant_gradient is None:
            imgui.begin_disabled()

        if imgui.button(f"Save Gradient{_NS}var_save"):
            self._save_variant_gradient()

        if self._variant_gradient is None:
            imgui.end_disabled()

    def _save_variant_gradient(self) -> None:
        if self._variant_gradient is None:
            return
        try:
            from PIL import Image
            # Derive name from remap path or source path
            if self._remap_path and self._remap_path != "(from current result)":
                src_name = Path(self._remap_path).stem
            elif self._source_path:
                src_name = Path(self._source_path).stem
            else:
                src_name = "texture"
            for suffix in ("_remap_d", "_remap", "_d", "_color", "_ColorGuide"):
                if src_name.endswith(suffix):
                    src_name = src_name[: -len(suffix)]
                    break
            out_dir = self._variant_output_path or self._output_path or (
                str(Path(self._remap_path).parent)
                if self._remap_path and self._remap_path != "(from current result)"
                else "."
            )
            os.makedirs(out_dir, exist_ok=True)
            p = os.path.join(out_dir, f"{src_name}_grad_d.png")
            Image.fromarray(self._variant_gradient, mode="RGBA").save(p)
            _log.info("Saved variant gradient: %s", p)
        except Exception as exc:
            _log.exception("variant save failed")
            self._error_msg = str(exc)

    # ------------------------------------------------------------------
    # HSV adjustment tab
    # ------------------------------------------------------------------

    def _draw_adjust_tab(self) -> None:
        imgui.text("Source")
        imgui.same_line()
        has_result = self._result is not None and self._result.gradient is not None
        if not has_result:
            imgui.begin_disabled()
        if imgui.button(f"Use Current Result{_NS}adj_use"):
            self._set_adjust_palette(self._result.gradient.copy(), "(from current result)")
        if not has_result:
            imgui.end_disabled()

        imgui.same_line()
        if imgui.button(f"Load Palette{_NS}adj_load"):
            path = pick_file(
                "Select palette texture",
                [("Images", "*.png *.dds *.bmp *.tga"), ("All", "*.*")],
            )
            if path:
                self._load_adjust_palette_from_file(path)

        if self._adjust_palette is None:
            imgui.text_disabled("No palette loaded.")
            if self._error_msg:
                imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1, 0.3, 0.3, 1))
                imgui.text_wrapped(self._error_msg)
                imgui.pop_style_color()
            return

        n_palettes = self._adjust_palette.shape[0] // 4
        imgui.same_line()
        imgui.text_colored(
            imgui.ImVec4(0.5, 1.0, 0.5, 1.0),
            f"Loaded - {n_palettes} palettes",
        )

        imgui.spacing()
        imgui.separator()

        if self._adjust_base_index >= n_palettes:
            self._adjust_base_index = max(0, n_palettes - 1)
            self._adjust_dirty = True

        if self._adjust_dirty:
            self._refresh_adjust_preview()

        avail = imgui.get_content_region_avail()
        left_w = min(460.0, max(340.0, avail.x * 0.42))
        if imgui.begin_child(
            f"##adjust_controls{_NS}",
            imgui.ImVec2(left_w, 0),
            child_flags=imgui.ChildFlags_.borders.value,
        ):
            self._adjust_dirty = (
                self._draw_adjust_controls(n_palettes) or self._adjust_dirty
            )

            imgui.spacing()
            if imgui.button(f"Add{_NS}adjust_add", imgui.ImVec2(100, 0)):
                self._append_adjust_palette()

            imgui.same_line()
            if imgui.button(f"Reset{_NS}adjust_reset", imgui.ImVec2(100, 0)):
                self._reset_adjust_controls()
                self._adjust_dirty = True

            imgui.separator()
            self._draw_adjust_save_row()

            if self._error_msg:
                imgui.push_style_color(imgui.Col_.text, imgui.ImVec4(1, 0.3, 0.3, 1))
                imgui.text_wrapped(self._error_msg)
                imgui.pop_style_color()
        imgui.end_child()

        imgui.same_line()
        if imgui.begin_child(
            f"##adjust_previews{_NS}",
            imgui.ImVec2(0, 0),
            child_flags=imgui.ChildFlags_.borders.value,
        ):
            if self._adjust_dirty:
                self._refresh_adjust_preview()
            self._draw_adjust_preview()
        imgui.end_child()

    def _draw_adjust_controls(self, n_palettes: int) -> bool:
        changed_any = False

        imgui.text("Palette")
        imgui.text("Base index")
        imgui.same_line()
        imgui.set_next_item_width(180)
        changed, self._adjust_base_index = imgui.slider_int(
            f"##adjust_base_index{_NS}", self._adjust_base_index, 0, max(0, n_palettes - 1),
        )
        changed_any = changed_any or changed

        imgui.separator()
        imgui.text("Color")
        imgui.text("Hue")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_hue_shift = imgui.slider_float(
            f"##adjust_hue{_NS}", self._adjust_hue_shift, -180.0, 180.0
        )
        changed_any = changed_any or changed

        imgui.text("Saturation")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_sat_scale = imgui.slider_float(
            f"##adjust_sat{_NS}", self._adjust_sat_scale, 0.0, 2.0
        )
        changed_any = changed_any or changed

        imgui.text("Vibrance")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_vibrance = imgui.slider_float(
            f"##adjust_vibrance{_NS}", self._adjust_vibrance, -1.0, 1.0
        )
        changed_any = changed_any or changed

        imgui.text("Value")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_val_scale = imgui.slider_float(
            f"##adjust_val{_NS}", self._adjust_val_scale, 0.0, 2.0
        )
        changed_any = changed_any or changed

        imgui.text("Temperature")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_temperature = imgui.slider_float(
            f"##adjust_temperature{_NS}", self._adjust_temperature, -100.0, 100.0
        )
        changed_any = changed_any or changed

        imgui.text("Tint")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_tint = imgui.slider_float(
            f"##adjust_tint{_NS}", self._adjust_tint, -100.0, 100.0
        )
        changed_any = changed_any or changed

        imgui.separator()
        imgui.text("Tone")
        imgui.text("Brightness")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_brightness = imgui.slider_float(
            f"##adjust_brightness{_NS}", self._adjust_brightness, -100.0, 100.0
        )
        changed_any = changed_any or changed

        imgui.text("Contrast")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_contrast = imgui.slider_float(
            f"##adjust_contrast{_NS}", self._adjust_contrast, 0.0, 2.0
        )
        changed_any = changed_any or changed

        imgui.text("Luminosity")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_luminosity = imgui.slider_float(
            f"##adjust_luminosity{_NS}", self._adjust_luminosity, -100.0, 100.0
        )
        changed_any = changed_any or changed

        imgui.text("Exposure")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_exposure = imgui.slider_float(
            f"##adjust_exposure{_NS}", self._adjust_exposure, -2.0, 2.0
        )
        changed_any = changed_any or changed

        imgui.text("Gamma")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_gamma = imgui.slider_float(
            f"##adjust_gamma{_NS}", self._adjust_gamma, 0.1, 3.0
        )
        changed_any = changed_any or changed

        imgui.separator()
        imgui.text("Levels")
        imgui.text("Input black")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_input_black = imgui.slider_int(
            f"##adjust_in_black{_NS}", self._adjust_input_black, 0, 254
        )
        changed_any = changed_any or changed

        imgui.text("Input white")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_input_white = imgui.slider_int(
            f"##adjust_in_white{_NS}", self._adjust_input_white, 1, 255
        )
        changed_any = changed_any or changed

        if self._adjust_input_black >= self._adjust_input_white:
            self._adjust_input_black = max(0, self._adjust_input_white - 1)
            changed_any = True

        imgui.text("Output black")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_output_black = imgui.slider_int(
            f"##adjust_out_black{_NS}", self._adjust_output_black, 0, 254
        )
        changed_any = changed_any or changed

        imgui.text("Output white")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_output_white = imgui.slider_int(
            f"##adjust_out_white{_NS}", self._adjust_output_white, 1, 255
        )
        changed_any = changed_any or changed

        if self._adjust_output_black >= self._adjust_output_white:
            self._adjust_output_black = max(0, self._adjust_output_white - 1)
            changed_any = True

        imgui.separator()
        imgui.text("Overlay")
        return self._draw_adjust_overlay_controls() or changed_any

    def _load_adjust_palette_from_file(self, path: str) -> None:
        try:
            from PIL import Image
            from creation_lib.dds import load_image
            img = load_image(path) or Image.open(path)
            if img.mode != "RGBA":
                img = img.convert("RGBA")
            palette = np.array(img, dtype=np.uint8)
            self._set_adjust_palette(palette, path)
        except Exception as exc:
            _log.exception("Failed to load palette")
            self._error_msg = str(exc)

    def _set_adjust_palette(self, palette: np.ndarray, path: str) -> None:
        if palette.ndim != 3 or palette.shape[2] != 4:
            self._error_msg = "Palette must be an RGBA image."
            return
        if palette.shape[0] < 4 or palette.shape[0] % 4 != 0:
            self._error_msg = "Palette height must be a multiple of 4 pixels."
            return
        self._adjust_palette = palette.copy()
        self._adjust_palette_path = path
        self._adjust_base_index = min(self._adjust_base_index, palette.shape[0] // 4 - 1)
        if palette.shape[1] in self._ZONE_MAX:
            self._gradient_width = palette.shape[1]
        self._adjust_dirty = True
        self._error_msg = ""

    def _refresh_adjust_preview(self) -> None:
        if self._adjust_palette is None:
            return
        start = self._adjust_base_index * 4
        row = self._adjust_palette[start:start + 4].copy()
        if self._tex_adjust_original is not None:
            try:
                self._tex_adjust_original.release()
            except Exception:
                pass
        self._tex_adjust_original = self._upload_scaled_texture(
            row, min_width=256, min_height=64
        )

        self._adjust_preview = self._adjust_palette_row_hsv(row)

        if self._tex_adjust_preview is not None:
            try:
                self._tex_adjust_preview.release()
            except Exception:
                pass
        self._tex_adjust_preview = self._upload_scaled_texture(
            self._adjust_preview, min_width=256, min_height=64
        )

        if self._tex_adjust_applied is not None:
            try:
                self._tex_adjust_applied.release()
            except Exception:
                pass
            self._tex_adjust_applied = None
        applied = self._compute_adjust_applied()
        if applied is not None:
            self._tex_adjust_applied = self._upload_texture(applied)
        self._adjust_dirty = False

    def _compute_adjust_applied(self) -> np.ndarray | None:
        if self._adjust_preview is None or self._result is None:
            return None
        try:
            from PIL import Image
            remap = self._result.remap
            remap_img = Image.fromarray(remap, mode="L")
            remap_img.thumbnail((320, 320), Image.NEAREST)
            remap_small = np.array(remap_img, dtype=np.uint8)
            max_col = self._adjust_preview.shape[1] - 1
            col_map = np.round(
                remap_small.astype(np.float32) / 255.0 * max_col
            ).astype(np.int32)
            col_map = np.clip(col_map, 0, max_col)
            return self._adjust_preview[0, col_map, :].astype(np.uint8)
        except Exception as exc:
            _log.warning("Adjust applied preview failed: %s", exc)
            return None

    def _draw_adjust_overlay_controls(self) -> bool:
        changed_any = False
        overlay_modes = ["Normal", "Color", "Overlay", "Multiply", "Screen", "Soft Light"]

        imgui.text("Overlay mode")
        imgui.same_line()
        imgui.set_next_item_width(160)
        changed, self._adjust_overlay_mode = imgui.combo(
            f"##adjust_overlay_mode{_NS}", self._adjust_overlay_mode, overlay_modes
        )
        changed_any = changed_any or changed

        imgui.text("Overlay")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, self._adjust_overlay_strength = imgui.slider_float(
            f"##adjust_overlay_strength{_NS}", self._adjust_overlay_strength, 0.0, 1.0
        )
        changed_any = changed_any or changed

        labels = ("Red", "Green", "Blue")
        for i, label in enumerate(labels):
            imgui.text(label)
            imgui.same_line()
            imgui.set_next_item_width(-1)
            changed, self._adjust_overlay_color[i] = imgui.slider_float(
                f"##adjust_overlay_{label.lower()}{_NS}",
                self._adjust_overlay_color[i],
                0.0,
                255.0,
            )
            changed_any = changed_any or changed

        rgb = np.clip(np.array(self._adjust_overlay_color, dtype=np.float32), 0, 255) / 255.0
        imgui.color_button(
            f"##adjust_overlay_swatch{_NS}",
            imgui.ImVec4(float(rgb[0]), float(rgb[1]), float(rgb[2]), 1.0),
            imgui.ColorEditFlags_.no_tooltip,
            imgui.ImVec2(28, 18),
        )
        return changed_any

    def _adjust_palette_row_hsv(self, rgba: np.ndarray) -> np.ndarray:
        adjusted = rgba.copy()
        rgb = adjusted[:, 1:, :3].astype(np.float32) / 255.0
        flat = rgb.reshape(-1, 3)
        hue_delta = self._adjust_hue_shift / 360.0
        for i, (r, g, b) in enumerate(flat):
            h, s, v = colorsys.rgb_to_hsv(float(r), float(g), float(b))
            h = (h + hue_delta) % 1.0
            s = max(0.0, min(1.0, s * self._adjust_sat_scale))
            if self._adjust_vibrance >= 0.0:
                s = s + (1.0 - s) * self._adjust_vibrance
            else:
                s = s * (1.0 + self._adjust_vibrance)
            v = max(0.0, min(1.0, v * self._adjust_val_scale))
            flat[i] = colorsys.hsv_to_rgb(h, s, v)
        rgb = flat.reshape(rgb.shape)
        rgb = self._apply_temperature_tint(rgb)
        rgb = self._apply_luminosity(rgb)
        rgb = np.clip(rgb * (2.0 ** self._adjust_exposure), 0.0, 1.0)
        rgb = np.clip(rgb + (self._adjust_brightness / 255.0), 0.0, 1.0)
        rgb = np.clip((rgb - 0.5) * self._adjust_contrast + 0.5, 0.0, 1.0)
        rgb = np.clip(rgb, 0.0, 1.0) ** (1.0 / max(0.1, self._adjust_gamma))
        rgb = self._apply_adjust_levels(rgb)
        rgb = self._apply_color_overlay(rgb)
        adjusted[:, 1:, :3] = np.clip(np.round(rgb * 255.0), 0, 255).astype(np.uint8)
        return adjusted

    def _apply_temperature_tint(self, rgb: np.ndarray) -> np.ndarray:
        if abs(self._adjust_temperature) < 0.001 and abs(self._adjust_tint) < 0.001:
            return rgb
        temp = self._adjust_temperature / 100.0 * 0.18
        tint = self._adjust_tint / 100.0 * 0.18
        shift = np.array([temp + tint, -tint, -temp + tint], dtype=np.float32)
        return np.clip(rgb + shift, 0.0, 1.0)

    def _apply_luminosity(self, rgb: np.ndarray) -> np.ndarray:
        if abs(self._adjust_luminosity) < 0.001:
            return rgb
        flat = rgb.reshape(-1, 3).copy()
        delta = self._adjust_luminosity / 100.0
        for i, (r, g, b) in enumerate(flat):
            h, lightness, s = colorsys.rgb_to_hls(float(r), float(g), float(b))
            lightness = max(0.0, min(1.0, lightness + delta))
            flat[i] = colorsys.hls_to_rgb(h, lightness, s)
        return flat.reshape(rgb.shape)

    def _apply_adjust_levels(self, rgb: np.ndarray) -> np.ndarray:
        in_black = self._adjust_input_black / 255.0
        in_white = self._adjust_input_white / 255.0
        out_black = self._adjust_output_black / 255.0
        out_white = self._adjust_output_white / 255.0
        rgb = (rgb - in_black) / max(0.001, in_white - in_black)
        rgb = np.clip(rgb, 0.0, 1.0)
        return out_black + rgb * (out_white - out_black)

    def _apply_color_overlay(self, rgb: np.ndarray) -> np.ndarray:
        strength = self._adjust_overlay_strength
        if strength <= 0.001:
            return rgb
        overlay = np.clip(np.array(self._adjust_overlay_color, dtype=np.float32), 0, 255) / 255.0
        mode = self._adjust_overlay_mode
        if mode == 1:
            blended = self._blend_color(rgb, overlay)
        elif mode == 2:
            blended = np.where(rgb < 0.5, 2.0 * rgb * overlay, 1.0 - 2.0 * (1.0 - rgb) * (1.0 - overlay))
        elif mode == 3:
            blended = rgb * overlay
        elif mode == 4:
            blended = 1.0 - (1.0 - rgb) * (1.0 - overlay)
        elif mode == 5:
            blended = np.where(
                overlay < 0.5,
                rgb - (1.0 - 2.0 * overlay) * rgb * (1.0 - rgb),
                rgb + (2.0 * overlay - 1.0) * (np.sqrt(np.clip(rgb, 0.0, 1.0)) - rgb),
            )
        else:
            blended = np.broadcast_to(overlay, rgb.shape)
        return np.clip(rgb * (1.0 - strength) + blended * strength, 0.0, 1.0)

    def _blend_color(self, rgb: np.ndarray, overlay: np.ndarray) -> np.ndarray:
        flat = rgb.reshape(-1, 3)
        out = np.empty_like(flat)
        overlay_h, overlay_l, overlay_s = colorsys.rgb_to_hls(
            float(overlay[0]), float(overlay[1]), float(overlay[2])
        )
        for i, (r, g, b) in enumerate(flat):
            _, base_l, _ = colorsys.rgb_to_hls(float(r), float(g), float(b))
            out[i] = colorsys.hls_to_rgb(overlay_h, base_l, overlay_s)
        return out.reshape(rgb.shape)

    def _reset_adjust_controls(self) -> None:
        self._adjust_hue_shift = 0.0
        self._adjust_sat_scale = 1.0
        self._adjust_vibrance = 0.0
        self._adjust_val_scale = 1.0
        self._adjust_temperature = 0.0
        self._adjust_tint = 0.0
        self._adjust_brightness = 0.0
        self._adjust_contrast = 1.0
        self._adjust_luminosity = 0.0
        self._adjust_exposure = 0.0
        self._adjust_gamma = 1.0
        self._adjust_input_black = 0
        self._adjust_input_white = 255
        self._adjust_output_black = 0
        self._adjust_output_white = 255
        self._adjust_overlay_strength = 0.0
        self._adjust_overlay_color = [255.0, 255.0, 255.0]
        self._adjust_overlay_mode = 0

    def _append_adjust_palette(self) -> None:
        if self._adjust_palette is None:
            return
        if self._adjust_dirty or self._adjust_preview is None:
            self._refresh_adjust_preview()
        if self._adjust_preview is None:
            return
        self._adjust_palette = np.vstack([self._adjust_palette, self._adjust_preview])
        self._adjust_base_index = self._adjust_palette.shape[0] // 4 - 1
        self._adjust_dirty = True

    def _draw_adjust_preview(self) -> None:
        avail_w = max(96.0, imgui.get_content_region_avail().x)
        band_h = 72.0

        self._draw_adjust_texture("Original", self._tex_adjust_original, avail_w, band_h)
        imgui.spacing()
        self._draw_adjust_texture("Adjusted", self._tex_adjust_preview, avail_w, band_h)
        imgui.spacing()
        if self._tex_adjust_applied is not None:
            self._draw_adjust_texture(
                "Applied Adjusted", self._tex_adjust_applied, avail_w, 260.0
            )
        else:
            imgui.text("Applied Adjusted")
            imgui.text_disabled("Generate a remap/source result first.")

    def _draw_adjust_texture(self, label: str, tex, width: float, height: float) -> None:
        imgui.text(label)
        if tex is not None:
            imgui.image(
                imgui.ImTextureRef(tex.glo),
                imgui.ImVec2(width, height),
            )
        else:
            imgui.dummy(imgui.ImVec2(width, height))

    def _draw_adjust_save_row(self) -> None:
        clicked = False
        if begin_form("##adjust_output_form"):
            _, clicked = draw_path_row("Output", self._adjust_output_path)
            end_form()
        if clicked:
            path = pick_folder("Select output folder")
            if path:
                self._adjust_output_path = path

        if self._adjust_palette is None:
            imgui.begin_disabled()

        if imgui.button(f"Save Palette{_NS}adjust_save"):
            self._save_adjust_palette()

        if self._adjust_palette is None:
            imgui.end_disabled()

    def _save_adjust_palette(self) -> None:
        if self._adjust_palette is None:
            return
        try:
            from PIL import Image
            if self._adjust_palette_path and self._adjust_palette_path != "(from current result)":
                src_name = Path(self._adjust_palette_path).stem
                default_dir = str(Path(self._adjust_palette_path).parent)
            elif self._source_path:
                src_name = Path(self._source_path).stem
                default_dir = str(Path(self._source_path).parent)
            else:
                src_name = "palette"
                default_dir = "."
            out_dir = self._adjust_output_path or self._output_path or default_dir
            os.makedirs(out_dir, exist_ok=True)
            p = os.path.join(out_dir, f"{src_name}_hsv.png")
            Image.fromarray(self._adjust_palette, mode="RGBA").save(p)
            _log.info("Saved adjusted palette: %s", p)
        except Exception as exc:
            _log.exception("adjust palette save failed")
            self._error_msg = str(exc)

    # ------------------------------------------------------------------
    # Preview
    # ------------------------------------------------------------------

    def _upload_texture(self, rgba: np.ndarray) -> "moderngl.Texture":
        """Upload a uint8 H*W*4 RGBA numpy array as a moderngl Texture.

        Store the returned Texture object (not .glo) -- call .release() when done.
        Pass .glo to imgui.ImTextureRef() at render time.
        """
        ctx = moderngl.get_context()
        h, w = rgba.shape[:2]
        tex = ctx.texture((w, h), 4, data=rgba.tobytes())
        tex.filter = (moderngl.LINEAR, moderngl.LINEAR)
        return tex

    def _upload_scaled_texture(
        self,
        rgba: np.ndarray,
        min_width: int = 128,
        min_height: int = 32,
    ) -> "moderngl.Texture":
        from PIL import Image as PILImage
        h, w = rgba.shape[:2]
        scale_x = max(1, min_width // max(1, w))
        scale_y = max(1, min_height // max(1, h))
        scale = max(scale_x, scale_y)
        img = PILImage.fromarray(rgba, mode="RGBA").resize(
            (w * scale, h * scale), PILImage.NEAREST
        )
        return self._upload_texture(np.array(img, dtype=np.uint8))

    def _refresh_textures(self) -> None:
        """Upload current result textures to GL."""
        if self._result is None:
            return
        from PIL import Image

        self._release_result_textures()

        # Remap texture (L -> RGBA for display)
        remap_rgba = np.stack(
            [self._result.remap] * 3 + [np.full_like(self._result.remap, 255)], axis=2
        )
        self._tex_remap = self._upload_texture(remap_rgba)

        # Gradient strip texture -- scale up for visibility
        from PIL import Image as PILImage
        grad_pil = PILImage.fromarray(self._result.gradient, mode="RGBA")
        gw, gh = grad_pil.size
        scale = max(1, 128 // gw)
        grad_big = grad_pil.resize((gw * scale, 4 * scale), PILImage.NEAREST)
        self._tex_gradient = self._upload_texture(np.array(grad_big, dtype=np.uint8))

        # Source texture + full gradient + applied preview
        if self._source_path and os.path.isfile(self._source_path):
            try:
                from creation_lib.dds import load_image
                src_img = load_image(self._source_path) or Image.open(self._source_path)
                if src_img.mode != "RGBA":
                    src_img = src_img.convert("RGBA")
                src_img.thumbnail((256, 256), Image.BILINEAR)
                self._tex_source = self._upload_texture(np.array(src_img, dtype=np.uint8))
                thumb_w, thumb_h = src_img.size
                remap_pil = Image.fromarray(self._result.remap, mode="L")
                self._remap_small = np.array(
                    remap_pil.resize((thumb_w, thumb_h), Image.NEAREST), dtype=np.uint8
                )
            except Exception:
                pass

        # Full 32-row gradient
        from creation_lib.palette.remap import build_gradient
        debug_grad = build_gradient(
            self._result.zones, width=self._gradient_width, banded=self._banded
        )
        debug_pil = PILImage.fromarray(debug_grad, mode="RGBA")
        dscale = max(1, 128 // self._gradient_width)
        debug_big = debug_pil.resize(
            (self._gradient_width * dscale, 32 * dscale), PILImage.NEAREST
        )
        self._tex_debug_gradient = self._upload_texture(
            np.array(debug_big, dtype=np.uint8)
        )
        self._applied_dirty = True

        self._tex_dirty = False

    def _compute_applied(self) -> None:
        """Compute applied preview using the full debug gradient."""
        if self._result is None or self._remap_small is None:
            return
        from creation_lib.palette.remap import build_gradient
        debug_grad = build_gradient(
            self._result.zones, width=self._gradient_width, banded=self._banded
        )
        max_col = self._gradient_width - 1
        row = round(self._paint_index * 31)
        col_map = np.round(
            self._remap_small.astype(np.float32) / 255.0 * max_col
        ).astype(np.int32)
        applied_rgba = debug_grad[row, col_map, :]
        if self._tex_applied is not None:
            try:
                self._tex_applied.release()
            except Exception:
                pass
        self._tex_applied = self._upload_texture(applied_rgba.astype(np.uint8))
        self._applied_dirty = False

    def _draw_preview(self) -> None:
        if self._result is None:
            imgui.text_disabled("No result yet — press Generate")
            return

        if self._tex_dirty:
            try:
                self._refresh_textures()
            except Exception as exc:
                _log.warning("Texture upload failed: %s", exc)

        avail_w = imgui.get_content_region_avail().x

        if self._applied_dirty:
            try:
                self._compute_applied()
            except Exception as exc:
                _log.warning("Applied compute failed: %s", exc)

        # Banded toggle
        changed, self._banded = imgui.checkbox(f"Banded{_NS}", self._banded)
        if changed:
            self._tex_dirty = True

        # 4 images in a row: remap (B&W), source, full gradient, applied
        item_spacing = imgui.get_style().item_spacing.x
        img_w = max(32.0, (avail_w - item_spacing * 3) / 4.0)
        img_h = img_w
        textures = (self._tex_remap, self._tex_source, self._tex_debug_gradient, self._tex_applied)
        for i, tex in enumerate(textures):
            if tex is not None:
                imgui.image(imgui.ImTextureRef(tex.glo), imgui.ImVec2(img_w, img_h))
            else:
                imgui.dummy(imgui.ImVec2(img_w, img_h))
            if i < 3:
                imgui.same_line()

        # Paint index slider
        imgui.text("Paint index")
        imgui.same_line()
        imgui.set_next_item_width(-1)
        changed, new_val = imgui.slider_float(
            f"##paint_index{_NS}", self._paint_index, 0.0, 1.0
        )
        if changed:
            self._paint_index = new_val
            self._applied_dirty = True

    # ------------------------------------------------------------------
    # Save row
    # ------------------------------------------------------------------

    def _draw_save_row(self) -> None:
        clicked = False
        if begin_form("##output_form"):
            _, clicked = draw_path_row("Output", self._output_path)
            end_form()
        if clicked:
            path = pick_folder("Select output folder")
            if path:
                self._output_path = path

        if self._result is None:
            imgui.begin_disabled()

        if imgui.button(f"Save Remap{_NS}"):
            self._save(remap=True, grad=False)
        imgui.same_line()
        if imgui.button(f"Save Grad{_NS}"):
            self._save(remap=False, grad=True)
        imgui.same_line()
        if imgui.button(f"Save Both{_NS}"):
            self._save(remap=True, grad=True)

        if self._result is None:
            imgui.end_disabled()

    # ------------------------------------------------------------------
    # Backend: run, detect, save
    # ------------------------------------------------------------------

    def _run_auto(self) -> None:
        if not self._source_path or not os.path.isfile(self._source_path):
            self._error_msg = "Select a valid source texture first."
            return
        self._error_msg = ""
        self._running = True
        self._progress = 0.0
        import threading
        threading.Thread(target=self._do_auto, daemon=True).start()

    def _do_auto(self) -> None:
        try:
            from PIL import Image
            from creation_lib.dds import load_image
            from creation_lib.palette.remap import auto_convert
            self._status_msg = "Loading..."
            self._progress = 0.1
            img = load_image(self._source_path) or Image.open(self._source_path)
            self._status_msg = "Clustering zones..."
            self._progress = 0.3
            result = auto_convert(img, n_zones=self._n_zones, width=self._gradient_width)
            self._result = result
            self._tex_dirty = True
            self._progress = 1.0
            self._status_msg = f"Done — {len(result.zones)} zones"
        except Exception as exc:
            _log.exception("auto_convert failed")
            self._error_msg = str(exc)
        finally:
            self._running = False

    def _detect_manual_zones(self) -> None:
        try:
            from PIL import Image
            from creation_lib.dds import load_image
            from creation_lib.palette.remap import auto_convert
            img = load_image(self._source_path) or Image.open(self._source_path)
            result = auto_convert(img, n_zones=self._n_zones, width=self._gradient_width)
            self._manual_zones = [
                {"avg_color": z.avg_color.copy(), "zone": z}
                for z in result.zones
            ]
        except Exception as exc:
            self._error_msg = str(exc)

    def _run_manual(self) -> None:
        if not self._manual_zones:
            return
        try:
            from PIL import Image
            from creation_lib.dds import load_image
            from creation_lib.palette.remap import (
                assign_columns, build_final_strip, build_remap_texture, PaletteResult,
            )
            import numpy as np
            img = load_image(self._source_path) or Image.open(self._source_path)
            if img.mode != "RGBA":
                img = img.convert("RGBA")
            rgba = np.array(img, dtype=np.uint8)
            h, w = rgba.shape[:2]

            sorted_colors = [z["avg_color"] for z in self._manual_zones]
            zones = assign_columns(sorted_colors, width=self._gradient_width)

            orig_zones = [z["zone"] for z in self._manual_zones]
            for new_z, orig_z in zip(zones, orig_zones):
                if orig_z.pixel_mask.shape == (h, w):
                    new_z.pixel_mask = orig_z.pixel_mask

            label_map = np.full((h, w), -1, dtype=np.int32)
            for z in zones:
                if z.pixel_mask.shape == (h, w):
                    label_map[z.pixel_mask] = z.index

            remap = build_remap_texture((h, w), label_map, zones)
            gradient = build_final_strip(zones, width=self._gradient_width)
            self._result = PaletteResult(remap=remap, gradient=gradient, zones=zones)
            self._tex_dirty = True
        except Exception as exc:
            _log.exception("manual generate failed")
            self._error_msg = str(exc)

    def _save(self, remap: bool, grad: bool) -> None:
        if self._result is None:
            return
        try:
            from PIL import Image
            src_name = Path(self._source_path).stem if self._source_path else "texture"
            # Strip common suffixes like _d, _color
            for suffix in ("_d", "_color", "_ColorGuide"):
                if src_name.endswith(suffix):
                    src_name = src_name[: -len(suffix)]
                    break
            out_dir = self._output_path or (
                str(Path(self._source_path).parent) if self._source_path else "."
            )
            os.makedirs(out_dir, exist_ok=True)
            if remap:
                p = os.path.join(out_dir, f"{src_name}_remap_d.png")
                Image.fromarray(self._result.remap, mode="L").save(p)
                _log.info("Saved remap: %s", p)
            if grad:
                p = os.path.join(out_dir, f"{src_name}_grad_d.png")
                Image.fromarray(self._result.gradient, mode="RGBA").save(p)
                _log.info("Saved gradient: %s", p)
        except Exception as exc:
            _log.exception("save failed")
            self._error_msg = str(exc)

    def cleanup(self) -> None:
        self._release_textures()

    def _release_result_textures(self) -> None:
        for attr in (
            "_tex_source",
            "_tex_remap",
            "_tex_gradient",
            "_tex_debug_gradient",
            "_tex_applied",
        ):
            tex = getattr(self, attr)
            if tex is not None:
                try:
                    tex.release()
                except Exception:
                    pass
            setattr(self, attr, None)
        self._remap_small = None
        self._tex_dirty = True

    def _release_textures(self) -> None:
        """Release moderngl Texture objects to free GPU memory."""
        self._release_result_textures()
        for attr in (
            "_tex_var_gradient",
            "_tex_var_applied",
            "_tex_adjust_original",
            "_tex_adjust_preview",
            "_tex_adjust_applied",
        ):
            tex = getattr(self, attr)
            if tex is not None:
                try:
                    tex.release()
                except Exception:
                    pass
            setattr(self, attr, None)
