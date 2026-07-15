"""Setup panel — file pickers (skeleton, body, weapon, reference anim, anim folder, output)."""

from __future__ import annotations

import logging
from pathlib import Path
from typing import TYPE_CHECKING

from imgui_bundle import imgui

if TYPE_CHECKING:
    from ..bone_editor_app import BoneEditorApp

_log = logging.getLogger("bone_editor.setup")


def _open_file_dialog(title: str, filetypes=None) -> str | None:
    if filetypes is None:
        filetypes = [("All files", "*.*")]
    try:
        from creation_lib.ui.widgets.pick_folder import pick_file
        return pick_file(title, filetypes)
    except Exception:
        return None


def _open_dir_dialog(title: str) -> str | None:
    try:
        from creation_lib.ui.widgets.pick_folder import pick_folder
        return pick_folder(title)
    except Exception:
        return None


class SetupPanel:
    def __init__(self, app: "BoneEditorApp"):
        self.app = app

        self._skeleton_path = ""
        self._body_mesh_path = ""
        self._weapon_mesh_path = ""
        self._reference_anim_path = ""
        self._anim_folder_path = ""
        self._output_folder_path = ""

        self._skeleton_loaded = False
        self._mesh_loaded = False
        self._anim_loaded = False
        self._anim_file_count = 0
        self._recursive = False

        # Preset body picker — shared with weight_painter / aligner.
        # Button lives in the toolbar; popup is rendered here every frame.
        self._ref_body_panel = None
        self._show_preset_popup = False
        if app.toolkit_settings:
            from ui.shared.reference_body_panel import ReferenceBodyPanel
            self._ref_body_panel = ReferenceBodyPanel(
                toolkit_settings=app.toolkit_settings,
                on_load=self._on_preset_load,
                games=["fo4"],
            )

    @property
    def anim_folder(self) -> Path | None:
        p = self._anim_folder_path.strip()
        return Path(p) if p and Path(p).is_dir() else None

    @property
    def output_folder(self) -> Path | None:
        p = self._output_folder_path.strip()
        return Path(p) if p else None

    def draw(self) -> None:
        visible, _ = imgui.begin("Setup##bone_editor")
        if not visible:
            imgui.end()
            return

        # Skeleton
        imgui.text("Skeleton HKX")
        skel_changed, self._skeleton_path = imgui.input_text(
            "##skel", self._skeleton_path, 512)
        if skel_changed and Path(self._skeleton_path.strip()).is_file():
            self._load_skeleton()
        imgui.same_line()
        if imgui.button("Browse##skel"):
            p = _open_file_dialog("Select Skeleton HKX",
                                  [("HKX", "*.hkx"), ("All", "*.*")])
            if p:
                self._skeleton_path = p
                self._load_skeleton()
        if self._skeleton_loaded and self.app.skeleton is not None:
            imgui.text_colored(imgui.ImVec4(0.4, 0.9, 0.4, 1.0),
                               f"OK — {self.app.skeleton.bone_count} bones")

        imgui.spacing(); imgui.separator(); imgui.spacing()

        # Body mesh
        imgui.text("Body Mesh NIF")
        body_changed, self._body_mesh_path = imgui.input_text(
            "##body", self._body_mesh_path, 512)
        if body_changed and Path(self._body_mesh_path.strip()).is_file():
            self._load_mesh()
        imgui.same_line()
        if imgui.button("Browse##body"):
            p = _open_file_dialog("Select Body Mesh NIF",
                                  [("NIF", "*.nif"), ("All", "*.*")])
            if p:
                self._body_mesh_path = p
                self._load_mesh()
        if self._mesh_loaded:
            imgui.text_colored(imgui.ImVec4(0.4, 0.9, 0.4, 1.0), "OK")

        imgui.spacing()

        # Weapon mesh (optional)
        imgui.text("Weapon NIF (optional)")
        wpn_changed, self._weapon_mesh_path = imgui.input_text(
            "##wpn", self._weapon_mesh_path, 512)
        if wpn_changed and Path(self._weapon_mesh_path.strip()).is_file():
            self._load_weapon()
        imgui.same_line()
        if imgui.button("Browse##wpn"):
            p = _open_file_dialog("Select Weapon NIF",
                                  [("NIF", "*.nif"), ("All", "*.*")])
            if p:
                self._weapon_mesh_path = p
                self._load_weapon()

        imgui.spacing(); imgui.separator(); imgui.spacing()

        # Reference animation
        imgui.text("Reference Animation HKX (optional)")
        ref_changed, self._reference_anim_path = imgui.input_text(
            "##ref", self._reference_anim_path, 512)
        if ref_changed and Path(self._reference_anim_path.strip()).is_file():
            self._load_reference_anim()
        imgui.same_line()
        if imgui.button("Browse##ref"):
            p = _open_file_dialog("Select Reference HKX",
                                  [("HKX", "*.hkx"), ("All", "*.*")])
            if p:
                self._reference_anim_path = p
                self._load_reference_anim()
        if self._anim_loaded:
            imgui.text_colored(imgui.ImVec4(0.4, 0.9, 0.4, 1.0), "Pose loaded")

        imgui.spacing(); imgui.separator(); imgui.spacing()

        # Animation folder
        imgui.text("Animation Folder")
        changed, self._anim_folder_path = imgui.input_text("##af", self._anim_folder_path, 512)
        if changed:
            self._refresh_anim_count()
        imgui.same_line()
        if imgui.button("Browse##af"):
            p = _open_dir_dialog("Select Animation Folder")
            if p:
                self._anim_folder_path = p
                self._refresh_anim_count()
        rec_changed, self._recursive = imgui.checkbox(
            "Include subfolders", self._recursive,
        )
        if rec_changed:
            self._refresh_anim_count()
        if self._anim_file_count > 0:
            label = "files (recursive)" if self._recursive else "files"
            imgui.text(f"{self._anim_file_count} .hkx {label} found")

        imgui.spacing()

        # Output folder
        imgui.text("Output Folder")
        _, self._output_folder_path = imgui.input_text("##of", self._output_folder_path, 512)
        imgui.same_line()
        if imgui.button("Browse##of"):
            p = _open_dir_dialog("Select Output Folder")
            if p:
                self._output_folder_path = p

        self._draw_preset_popup()
        imgui.end()

    def _refresh_anim_count(self) -> None:
        folder = self._anim_folder_path.strip()
        if folder and Path(folder).is_dir():
            pattern = "**/*.hkx" if self._recursive else "*.hkx"
            self._anim_file_count = sum(1 for _ in Path(folder).glob(pattern))
        else:
            self._anim_file_count = 0

    def _load_skeleton(self) -> None:
        path = self._skeleton_path.strip()
        if not path or not Path(path).exists():
            return
        try:
            self.app.load_skeleton(path)
            self._skeleton_loaded = True
            self.app.status_text = (
                f"Skeleton loaded: {self.app.skeleton.bone_count} bones"
            )
        except Exception as e:
            _log.exception("Skeleton load failed")
            self.app.status_text = f"Error: {e}"

    def _load_mesh(self) -> None:
        path = self._body_mesh_path.strip()
        if not path or not Path(path).exists():
            return
        try:
            self.app.load_mesh(path)
            self._mesh_loaded = True
            self.app.status_text = "Body mesh loaded"
        except Exception as e:
            _log.exception("Body mesh load failed")
            self.app.status_text = f"Error: {e}"

    def _load_weapon(self) -> None:
        path = self._weapon_mesh_path.strip()
        if not path or not Path(path).exists():
            return
        try:
            self.app.load_weapon(path)
            self.app.status_text = "Weapon loaded"
        except Exception as e:
            _log.exception("Weapon load failed")
            self.app.status_text = f"Error: {e}"

    def _draw_preset_popup(self) -> None:
        if self._show_preset_popup:
            self._show_preset_popup = False
            imgui.open_popup("Preset Body##bone_editor_popup")
        if self._ref_body_panel is not None:
            if imgui.begin_popup("Preset Body##bone_editor_popup"):
                self._ref_body_panel.draw()
                imgui.end_popup()

    def _on_preset_load(self, skeleton_hkx: str, skeleton_nif: str | None,
                        body_nif_paths: list[str], game: str) -> None:
        try:
            self.app.load_composite_body(
                skeleton_hkx, skeleton_nif, body_nif_paths, game,
            )
            self._skeleton_path = skeleton_hkx
            self._skeleton_loaded = self.app.skeleton is not None
            self._mesh_loaded = bool(self.app.skinned_meshes)
        except Exception as e:
            _log.exception("Preset body load failed")
            self.app.status_text = f"Error: {e}"

    def _load_reference_anim(self) -> None:
        path = self._reference_anim_path.strip()
        if not path or not Path(path).exists():
            return
        try:
            self.app.load_reference_pose(path)
            self._anim_loaded = True
            self.app.status_text = f"Reference pose loaded from {Path(path).name}"
        except Exception as e:
            _log.exception("Reference pose load failed")
            self.app.status_text = f"Error: {e}"
