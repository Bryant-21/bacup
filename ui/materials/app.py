"""MaterialEditorApp — core state and rendering for BGSM/BGEM editing.

Manages: open/save, flatten/unflatten dataclass <-> flat dict, undo/redo,
and draws the 3-tab layout (General always, Material for bgsm, Effect for bgem).
"""

from __future__ import annotations

import io
import json
import logging
from dataclasses import dataclass, fields as dc_fields
from pathlib import Path

from imgui_bundle import imgui

from creation_lib.material_tools.base import BaseHeader
from creation_lib.material_tools.bgsm_bin import BGSMData, BGSM_SIGNATURE, read_bgsm
from creation_lib.material_tools.bgem_bin import BGEMData, BGEM_SIGNATURE, read_bgem
from creation_lib.ui.widgets.user_guide import (
    UserGuide,
    draw_generic_user_guide_window,
    draw_help_menu,
    draw_toolbar_help_button,
)

from .field_registry import (
    GENERAL_FIELDS,
    MATERIAL_FIELDS,
    EFFECT_FIELDS,
    KIND_DEFAULTS,
    FIELD_BY_ATTR,
)
from .panels.toolbar import draw_menu as _draw_toolbar_menu, draw_version_row
from .panels.general_panel import draw_general_panel
from .panels.material_panel import draw_material_panel
from .panels.effect_panel import draw_effect_panel
from .preview import MaterialPreviewRenderer
from .user_guide import USER_GUIDE_MARKDOWN

_log = logging.getLogger("materials.app")

_MAX_UNDO = 100
_MAX_RECENT = 10


@dataclass
class UndoEntry:
    attr: str
    old_value: object
    new_value: object


# ---------------------------------------------------------------------------
# Default field values (built once from the field registry)
# ---------------------------------------------------------------------------
def _build_defaults() -> dict[str, object]:
    defaults: dict[str, object] = {}
    all_fields = GENERAL_FIELDS + MATERIAL_FIELDS + EFFECT_FIELDS
    for f in all_fields:
        defaults[f.attr] = KIND_DEFAULTS.get(f.kind)
    # Non-UI header fields needed for round-trip
    defaults["alpha_blend_mode1"] = 0
    defaults["alpha_blend_mode2"] = 0
    # Sensible overrides
    defaults["u_scale"] = 1.0
    defaults["v_scale"] = 1.0
    defaults["alpha"] = 1.0
    defaults["alpha_test_ref"] = 128
    defaults["zbuffer_write"] = True
    defaults["zbuffer_test"] = True
    defaults["SpecularMult"] = 1.0
    defaults["FresnelPower"] = 5.0
    defaults["ReceiveShadows"] = True
    defaults["CastShadows"] = True
    defaults["GrayscaleToPaletteScale"] = 1.0
    return defaults


_DEFAULTS = _build_defaults()


# ---------------------------------------------------------------------------
# Flatten / unflatten helpers
# ---------------------------------------------------------------------------
def _flatten(data: BGSMData | BGEMData) -> dict[str, object]:
    """Convert a loaded dataclass to a flat attr->value dict."""
    result = dict(_DEFAULTS)  # start with defaults for any None fields
    header = data.header
    for f in dc_fields(BaseHeader):
        if f.name == "signature":
            continue
        val = getattr(header, f.name)
        if val is not None:
            result[f.name] = val
    for f in dc_fields(type(data)):
        if f.name == "header":
            continue
        val = getattr(data, f.name)
        if val is not None:
            result[f.name] = val
    return result


def _unflatten_bgsm(fields_dict: dict, version: int) -> BGSMData:
    """Reconstruct BGSMData from the flat dict."""
    hdr_kw: dict = {"signature": BGSM_SIGNATURE, "version": version}
    for f in dc_fields(BaseHeader):
        if f.name in ("signature", "version"):
            continue
        hdr_kw[f.name] = fields_dict.get(f.name, _DEFAULTS.get(f.name))
    header = BaseHeader(**hdr_kw)

    body_kw: dict = {"header": header}
    for f in dc_fields(BGSMData):
        if f.name == "header":
            continue
        body_kw[f.name] = fields_dict.get(f.name, _DEFAULTS.get(f.name))
    return BGSMData(**body_kw)


def _unflatten_bgem(fields_dict: dict, version: int) -> BGEMData:
    """Reconstruct BGEMData from the flat dict."""
    hdr_kw: dict = {"signature": BGEM_SIGNATURE, "version": version}
    for f in dc_fields(BaseHeader):
        if f.name in ("signature", "version"):
            continue
        hdr_kw[f.name] = fields_dict.get(f.name, _DEFAULTS.get(f.name))
    header = BaseHeader(**hdr_kw)

    body_kw: dict = {"header": header}
    for f in dc_fields(BGEMData):
        if f.name == "header":
            continue
        body_kw[f.name] = fields_dict.get(f.name, _DEFAULTS.get(f.name))
    return BGEMData(**body_kw)


def _detect_file_type(path: str) -> str:
    """Read first 4 bytes to detect BGSM vs BGEM."""
    with open(path, "rb") as f:
        sig = int.from_bytes(f.read(4), "little")
    if sig == BGSM_SIGNATURE:
        return "bgsm"
    elif sig == BGEM_SIGNATURE:
        return "bgem"
    raise ValueError(f"Unknown material signature: 0x{sig:08X}")


# ---------------------------------------------------------------------------
# MaterialEditorApp
# ---------------------------------------------------------------------------
class MaterialEditorApp:
    def __init__(self, toolkit_settings=None):
        self.file_path: str | None = None
        self.file_type: str = "bgsm"
        self.version: int = 2
        self.fields_dict: dict[str, object] = dict(_DEFAULTS)
        self.dirty: bool = False
        self.undo_stack: list[UndoEntry] = []
        self.redo_stack: list[UndoEntry] = []
        self.recent_files: list[str] = []
        self._toolkit_settings = toolkit_settings
        self._pending_open: str | None = None
        self._preview = MaterialPreviewRenderer(toolkit_settings=toolkit_settings)
        self._show_user_guide = False

    def get_user_guide(self) -> UserGuide:
        return UserGuide(
            "Material Editor User Guide",
            USER_GUIDE_MARKDOWN,
            "material_editor_user_guide",
        )

    def toggle_user_guide(self) -> None:
        self._show_user_guide = not self._show_user_guide

    def draw_user_guide_window(self) -> None:
        self._show_user_guide = draw_generic_user_guide_window(
            self._show_user_guide,
            self.get_user_guide(),
        )

    def draw_toolbar(self, icon_font=None) -> None:
        draw_toolbar_help_button(self, icon_font)

    # ------------------------------------------------------------------
    # File operations
    # ------------------------------------------------------------------
    def new_material(self, file_type: str, version: int | None = None) -> None:
        if version is None:
            version = self.version
        self.file_path = None
        self.file_type = file_type
        self.version = version
        self.fields_dict = dict(_DEFAULTS)
        self.dirty = False
        self.undo_stack.clear()
        self.redo_stack.clear()
        _log.info("New %s material (v%d)", file_type.upper(), version)

    def open_file(self, path: str) -> None:
        try:
            ft = _detect_file_type(path)
            with open(path, "rb") as f:
                br = io.BufferedReader(f)
                if ft == "bgsm":
                    data = read_bgsm(br)
                else:
                    data = read_bgem(br)
            self.file_type = ft
            self.version = data.header.version
            self.fields_dict = _flatten(data)
            self.file_path = path
            self.dirty = False
            self.undo_stack.clear()
            self.redo_stack.clear()
            self._add_recent(path)
            _log.info("Opened %s (v%d): %s", ft.upper(), self.version, path)
        except Exception:
            _log.exception("Failed to open %s", path)

    def open_file_dialog(self) -> None:
        from ui.tools.imgui_helpers import pick_file

        path = pick_file(
            "Open Material",
            [
                ("Material Files", "*.bgsm *.bgem"),
                ("BGSM", "*.bgsm"),
                ("BGEM", "*.bgem"),
                ("All", "*.*"),
            ],
        )
        if path:
            self.open_file(path)

    def save_file(self, path: str | None = None) -> None:
        path = path or self.file_path
        if not path:
            self.save_file_as()
            return
        try:
            if self.file_type == "bgsm":
                data = _unflatten_bgsm(self.fields_dict, self.version)
            else:
                data = _unflatten_bgem(self.fields_dict, self.version)
            with open(path, "wb") as f:
                data.write(f)
            self.file_path = path
            self.dirty = False
            self._add_recent(path)
            _log.info("Saved %s: %s", self.file_type.upper(), path)
        except Exception:
            _log.exception("Failed to save %s", path)

    def save_file_as(self) -> None:
        from ui.tools.imgui_helpers import pick_save_file

        ext = f".{self.file_type}"
        ftype_label = "BGSM Files" if self.file_type == "bgsm" else "BGEM Files"
        path = pick_save_file(
            "Save Material As",
            [
                (ftype_label, f"*{ext}"),
                ("All", "*.*"),
            ],
            default_ext=ext,
        )
        if path:
            self.save_file(path)

    def import_json(self) -> None:
        from ui.tools.imgui_helpers import pick_file

        path = pick_file(
            "Import JSON Material",
            [
                ("JSON", "*.json"),
                ("All", "*.*"),
            ],
        )
        if not path:
            return
        try:
            with open(path, "r", encoding="utf-8") as f:
                obj = json.load(f)
            # Merge imported fields into current dict
            for key, val in obj.items():
                if key in self.fields_dict:
                    self.fields_dict[key] = val
            if "version" in obj:
                self.version = int(obj["version"])
            if "file_type" in obj:
                self.file_type = str(obj["file_type"])
            self.dirty = True
            _log.info("Imported JSON: %s", path)
        except Exception:
            _log.exception("Failed to import JSON %s", path)

    def export_json(self) -> None:
        from ui.tools.imgui_helpers import pick_save_file

        path = pick_save_file(
            "Export JSON Material",
            [
                ("JSON", "*.json"),
                ("All", "*.*"),
            ],
            default_ext=".json",
        )
        if not path:
            return
        try:
            obj = dict(self.fields_dict)
            obj["version"] = self.version
            obj["file_type"] = self.file_type
            # Convert tuples to lists for JSON
            for k, v in obj.items():
                if isinstance(v, tuple):
                    obj[k] = list(v)
            with open(path, "w", encoding="utf-8") as f:
                json.dump(obj, f, indent=2, ensure_ascii=False)
            _log.info("Exported JSON: %s", path)
        except Exception:
            _log.exception("Failed to export JSON %s", path)

    # ------------------------------------------------------------------
    # Undo / redo
    # ------------------------------------------------------------------
    def set_field(self, attr: str, value: object, track_undo: bool = True) -> None:
        old = self.fields_dict.get(attr)
        if old == value:
            return
        if track_undo:
            self.undo_stack.append(UndoEntry(attr, old, value))
            if len(self.undo_stack) > _MAX_UNDO:
                self.undo_stack.pop(0)
            self.redo_stack.clear()
        self.fields_dict[attr] = value
        self.dirty = True

    def undo(self) -> None:
        if not self.undo_stack:
            return
        entry = self.undo_stack.pop()
        self.redo_stack.append(entry)
        self.fields_dict[entry.attr] = entry.old_value
        self.dirty = True

    def redo(self) -> None:
        if not self.redo_stack:
            return
        entry = self.redo_stack.pop()
        self.undo_stack.append(entry)
        self.fields_dict[entry.attr] = entry.new_value
        self.dirty = True

    # ------------------------------------------------------------------
    # Recent files
    # ------------------------------------------------------------------
    def _add_recent(self, path: str) -> None:
        path = str(Path(path).resolve())
        if path in self.recent_files:
            self.recent_files.remove(path)
        self.recent_files.insert(0, path)
        if len(self.recent_files) > _MAX_RECENT:
            self.recent_files.pop()

    # ------------------------------------------------------------------
    # Drawing
    # ------------------------------------------------------------------
    def draw(self) -> None:
        """Draw the material editor content (version row + 3-tab layout)."""
        # Handle deferred open (e.g., from command-line arg)
        if self._pending_open:
            path = self._pending_open
            self._pending_open = None
            self.open_file(path)

        if not self.fields_dict:
            imgui.text_disabled("No material loaded. Use File > New or File > Open.")
            return

        draw_version_row(self)
        imgui.separator()

        avail = imgui.get_content_region_avail()
        if avail.x >= 900:
            preview_width = min(380.0, max(300.0, avail.x * 0.34))
            imgui.begin_child(
                "material_preview",
                imgui.ImVec2(preview_width, 0),
                imgui.ChildFlags_.borders,
                imgui.WindowFlags_.none,
            )
            self._draw_preview_panel()
            imgui.end_child()

            imgui.same_line()
            imgui.begin_child(
                "material_editor_tabs",
                imgui.ImVec2(0, 0),
                imgui.ChildFlags_.none,
                imgui.WindowFlags_.none,
            )
            self._draw_tabs()
            imgui.end_child()
            return

        preview_height = min(360.0, max(220.0, avail.y * 0.34))
        imgui.begin_child(
            "material_preview",
            imgui.ImVec2(0, preview_height),
            imgui.ChildFlags_.borders,
            imgui.WindowFlags_.none,
        )
        self._draw_preview_panel()
        imgui.end_child()
        imgui.spacing()

        imgui.begin_child(
            "material_editor_tabs",
            imgui.ImVec2(0, 0),
            imgui.ChildFlags_.none,
            imgui.WindowFlags_.none,
        )
        self._draw_tabs()
        imgui.end_child()

    def _draw_preview_panel(self) -> None:
        game_id = "fo4"
        if self._toolkit_settings is not None:
            try:
                game_id = self._toolkit_settings.get_active_game()
            except Exception:
                pass

        imgui.text(f"Live Preview ({game_id.upper()})")
        imgui.separator()

        avail = imgui.get_content_region_avail()
        image_size = int(max(96.0, min(avail.x, max(96.0, avail.y - 64.0))))
        tex_id = self._preview.render(
            image_size, image_size, self.file_type, self.version, self.fields_dict,
            file_path=self.file_path,
        )
        if tex_id:
            imgui.image(
                imgui.ImTextureRef(tex_id),
                imgui.ImVec2(float(image_size), float(image_size)),
                uv0=imgui.ImVec2(0, 1),
                uv1=imgui.ImVec2(1, 0),
            )
        else:
            imgui.text_disabled("Preview unavailable")

        for line in self._preview.status_lines:
            imgui.text_wrapped(line)

    def _draw_tabs(self) -> None:
        if imgui.begin_tab_bar("material_tabs"):
            if imgui.begin_tab_item("General")[0]:
                imgui.begin_child(
                    "general_scroll",
                    imgui.ImVec2(0, 0),
                    imgui.ChildFlags_.none,
                    imgui.WindowFlags_.none,
                )
                draw_general_panel(self)
                imgui.end_child()
                imgui.end_tab_item()

            if self.file_type == "bgsm":
                if imgui.begin_tab_item("Material")[0]:
                    imgui.begin_child(
                        "material_scroll",
                        imgui.ImVec2(0, 0),
                        imgui.ChildFlags_.none,
                        imgui.WindowFlags_.none,
                    )
                    draw_material_panel(self)
                    imgui.end_child()
                    imgui.end_tab_item()

            if self.file_type == "bgem":
                if imgui.begin_tab_item("Effect")[0]:
                    imgui.begin_child(
                        "effect_scroll",
                        imgui.ImVec2(0, 0),
                        imgui.ChildFlags_.none,
                        imgui.WindowFlags_.none,
                    )
                    draw_effect_panel(self)
                    imgui.end_child()
                    imgui.end_tab_item()

            imgui.end_tab_bar()

    def draw_menu(self) -> None:
        """Draw host-owned menu bar items."""
        _draw_toolbar_menu(self)

    def draw_standalone_menu(self) -> None:
        self.draw_menu()
        draw_help_menu(self)

    def process_shortcuts(self) -> None:
        """Handle keyboard shortcuts. Call each frame when workspace is active."""
        io = imgui.get_io()
        ctrl = io.key_ctrl

        if ctrl and imgui.is_key_pressed(imgui.Key.n):
            self.new_material(self.file_type, self.version)
        elif ctrl and imgui.is_key_pressed(imgui.Key.o):
            self.open_file_dialog()
        elif ctrl and imgui.is_key_pressed(imgui.Key.s):
            self.save_file()
        elif ctrl and imgui.is_key_pressed(imgui.Key.z):
            self.undo()
        elif ctrl and imgui.is_key_pressed(imgui.Key.y):
            self.redo()

    # ------------------------------------------------------------------
    # Settings persistence
    # ------------------------------------------------------------------
    def get_settings_defaults(self) -> dict:
        return {"recent_files": [], "default_game": "fo4"}

    def apply_settings(self, settings: dict) -> None:
        self.recent_files = settings.get("recent_files", [])

    def collect_settings(self) -> dict:
        return {"recent_files": self.recent_files[:_MAX_RECENT]}
