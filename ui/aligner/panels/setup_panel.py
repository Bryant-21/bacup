"""Setup panel — file pickers for weapon/scope NIFs, animation, and connect point selection."""
from __future__ import annotations

import logging
from pathlib import Path

from imgui_bundle import imgui

_log = logging.getLogger("aligner.setup")


def _open_file_dialog(title: str, filetypes=None) -> str | None:
    """Open a native file dialog."""
    if filetypes is None:
        filetypes = [("NIF files", "*.nif"), ("All files", "*.*")]
    try:
        from creation_lib.ui.widgets.pick_folder import pick_file
        return pick_file(title, filetypes)
    except Exception:
        _log.warning("File dialog not available — type path manually")
        return None


class SetupPanel:
    """File pickers and connect point dropdown."""

    def __init__(self, app):
        self._app = app
        self.window_name = "Setup##aligner"
        self._weapon_path = ""
        self._scope_path = ""
        self._anim_path = ""
        self._connect_points: list[str] = []
        self._selected_cp_idx = 0
        self._skeleton_loaded = False
        self._anim_loaded = False
        self._anim_loading = False
        self._anim_error = ""
        self._scope_loaded = False
        self._scope_nif_id: str | None = None  # track attached scope session

    def restore_paths(self, weapon_path: str, scope_path: str, anim_path: str):
        """Restore previously saved paths (called from workspace apply_settings)."""
        self._weapon_path = weapon_path
        self._scope_path = scope_path
        self._anim_path = anim_path

    def collect_paths(self) -> dict:
        """Collect current paths for persistence."""
        return {
            "weapon_path": self._weapon_path,
            "scope_path": self._scope_path,
            "anim_path": self._anim_path,
        }

    def draw(self):
        imgui.begin(self.window_name)

        # Skeleton status
        if self._skeleton_loaded:
            imgui.text_colored(imgui.ImVec4(0.4, 0.9, 0.4, 1.0), "Skeleton: loaded")
        else:
            imgui.text_colored(imgui.ImVec4(0.9, 0.4, 0.4, 1.0), "Skeleton: not loaded")
        imgui.separator()
        imgui.spacing()

        # -- Sighted Animation --
        imgui.text("Sighted Animation (HKX)")
        changed, self._anim_path = imgui.input_text(
            "##anim_path", self._anim_path, 512,
        )
        imgui.same_line()
        if imgui.button("Browse##anim"):
            self._browse_anim()
        imgui.same_line()
        if imgui.button("Load Anim"):
            self._load_anim()

        if self._anim_loaded:
            imgui.same_line()
            imgui.text_colored(imgui.ImVec4(0.4, 0.9, 0.4, 1.0), "OK")
        elif self._anim_error:
            imgui.text_colored(imgui.ImVec4(0.9, 0.4, 0.4, 1.0), self._anim_error)

        imgui.spacing()
        imgui.separator()
        imgui.spacing()

        # -- Weapon --
        imgui.text("Weapon NIF")
        changed, self._weapon_path = imgui.input_text(
            "##weapon_path", self._weapon_path, 512,
        )
        imgui.same_line()
        if imgui.button("Browse##weapon"):
            self._browse_weapon()
        imgui.same_line()
        if imgui.button("Load Weapon"):
            self._load_weapon()
        imgui.spacing()

        # -- Scope --
        imgui.text("Scope NIF")
        changed, self._scope_path = imgui.input_text(
            "##scope_path", self._scope_path, 512,
        )
        imgui.same_line()
        if imgui.button("Browse##scope"):
            self._browse_scope()
        imgui.spacing()

        # Connect point dropdown and attach/replace button
        if self._connect_points:
            imgui.text("Connect Point")
            changed, self._selected_cp_idx = imgui.combo(
                "##cp_select", self._selected_cp_idx, self._connect_points,
            )
            imgui.same_line()
            label = "Replace Scope" if self._scope_loaded else "Attach Scope"
            if imgui.button(label):
                self._attach_scope()
            if self._scope_loaded:
                imgui.same_line()
                imgui.text_colored(imgui.ImVec4(0.4, 0.9, 0.4, 1.0), "Attached")
        elif self._app.registry.sessions:
            imgui.text_colored(
                imgui.ImVec4(0.6, 0.6, 0.6, 1.0),
                "No connect points found on weapon",
            )

        imgui.end()

    def _browse_weapon(self):
        path = _open_file_dialog("Select Weapon NIF")
        if path:
            self._weapon_path = path

    def _browse_scope(self):
        path = _open_file_dialog("Select Scope NIF")
        if path:
            self._scope_path = path

    def _browse_anim(self):
        path = _open_file_dialog(
            "Select Sighted Animation HKX",
            filetypes=[("HKX files", "*.hkx"), ("All files", "*.*")],
        )
        if path:
            self._anim_path = path

    def _load_anim(self):
        path = self._anim_path.strip()
        if not path or not Path(path).exists():
            self._anim_error = "File not found"
            return
        self._anim_error = ""
        self._anim_loaded = False
        try:
            self._app.load_sighted_animation(path)
            self._anim_loaded = True
        except Exception as e:
            self._anim_error = str(e)[:60]
            _log.exception("Failed to load animation: %s", path)

    def _load_weapon(self):
        path = self._weapon_path.strip()
        if not path or not Path(path).exists():
            _log.warning("Invalid weapon path: %s", path)
            return
        self._app.load_weapon(path)
        self._connect_points = self._app.get_connect_point_names()
        self._selected_cp_idx = 0
        # Unload any previously attached scope (weapon was replaced)
        self._scope_loaded = False
        self._scope_nif_id = None

    def _detach_current_scope(self):
        """Detach the currently attached scope without resetting camera."""
        if not self._scope_nif_id:
            return
        try:
            session = self._app.registry.get_session(self._scope_nif_id)
            parent_session = self._app.registry.get_session("main")
            if session.attachment_node and session.attachment_node in parent_session.scene_root.children:
                parent_session.scene_root.children.remove(session.attachment_node)
            self._app.registry.remove_session(self._scope_nif_id)
            _log.info("Detached scope: %s", self._scope_nif_id)
        except KeyError:
            _log.debug("Scope session %s already gone", self._scope_nif_id)
        self._scope_nif_id = None
        self._scope_loaded = False

    def _attach_scope(self):
        path = self._scope_path.strip()
        if not path or not Path(path).exists():
            _log.warning("Invalid scope path: %s", path)
            return
        if not self._connect_points:
            return

        # Detach existing scope first (keeps camera position)
        self._detach_current_scope()

        cp_name = self._connect_points[self._selected_cp_idx]
        self._app.load_scope(path, cp_name)

        # Track the newly attached scope's nif_id
        # load_scope uses registry.next_child_id() which was called before adding
        # Find the most recently added non-main session
        for nif_id in reversed(list(self._app.registry.sessions.keys())):
            if nif_id != "main":
                self._scope_nif_id = nif_id
                break

        self._scope_loaded = True
