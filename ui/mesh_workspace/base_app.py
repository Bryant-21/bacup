"""MeshWorkspaceBase — abstract base for 3D mesh editing workspaces.

Provides: GL context, SceneRenderer, OrbitCamera, MeshPicker,
BrushCursor, and panel lifecycle. Subclasses (WeightPainterApp,
ClothMakerApp) add domain-specific data models and panels.
"""
from __future__ import annotations

import abc
import logging
from typing import TYPE_CHECKING

import moderngl
import numpy as np

from .brush import BrushState
from .camera import OrbitCamera, frame_on_bounds
from .undo import UndoStack

if TYPE_CHECKING:
    from creation_lib.renderer.scene_renderer import SceneRenderer

_log = logging.getLogger("mesh_workspace.base")


class MeshWorkspaceBase(abc.ABC):
    """Abstract base for mesh workspace applications."""

    def __init__(self, toolkit_settings=None):
        self.ctx: moderngl.Context | None = None
        self.renderer: "SceneRenderer | None" = None
        self.camera: OrbitCamera | None = None
        self.mesh_picker = None  # MeshPicker instance
        self.brush_cursor = None  # BrushCursor instance
        self.brush_state = BrushState()
        self.toolkit_settings = toolkit_settings
        self.status_text = ""
        self.active = True
        self._first_frame = True
        self._panels_initialized = False

        # Subclasses must set these for SceneRenderer compat
        self.render_mode_mgr = None
        self.registry = None

        from creation_lib.renderer.lighting import LightingSetup
        self.lighting = LightingSetup()

    def setup(self):
        """Called on first frame when GL context is available."""
        self.ctx = moderngl.get_context()

        from creation_lib.renderer.scene_renderer import SceneRenderer
        self.renderer = SceneRenderer(self.ctx)
        self.renderer.init_shaders()
        self.renderer.init_grid()
        self.renderer.grid_visible = False
        self._wire_renderer_hooks()

        from ui.mesh_workspace.brush_cursor import BrushCursor
        self.brush_cursor = BrushCursor(self.ctx)

        self.camera = OrbitCamera()

        from creation_lib.nif.rendering import MeshPicker
        self.mesh_picker = MeshPicker()

        _log.info("Mesh workspace GL context initialized: %s",
                  self.ctx.info["GL_RENDERER"])

    def _wire_renderer_hooks(self):
        """Wire optional renderer manager hooks. Override in subclasses."""
        pass

    @abc.abstractmethod
    def _init_panels(self):
        """Create panel instances. Called once."""
        ...

    @abc.abstractmethod
    def draw_workspace(self):
        """Draw the full workspace UI (panels + viewport). Called each frame."""
        ...

    def get_texture_dirs(self, game_id: str = None) -> list["Path"]:
        """Return texture search directories from toolkit settings.

        Builds a list of directories where game textures may reside:
        the extracted game data folder and the game root_dir/Data folder.
        Returns an empty list if settings are unavailable.

        Args:
            game_id: Game to look up paths for. Defaults to the toolkit's
                active game setting. Pass explicitly when the NIF's game
                differs from the active workspace game (avoids loading the
                wrong game's directory index on the main thread).
        """
        from pathlib import Path
        if not self.toolkit_settings:
            return []
        try:
            if game_id is None:
                game_id = self.toolkit_settings.get_active_game()
            paths = self.toolkit_settings.get_game_paths(game_id)
        except Exception:
            return []

        dirs: list[Path] = []
        extracted = paths.get("extracted_dir", "")
        if extracted:
            p = Path(extracted)
            if p.is_dir():
                dirs.append(p)
        root = paths.get("root_dir", "")
        if root:
            data_dir = Path(root) / "Data"
            if data_dir.is_dir():
                dirs.append(data_dir)
        # Additional paths (user-configured extra search dirs)
        for extra in paths.get("additional_paths", []):
            if extra:
                ep = Path(extra)
                if ep.is_dir():
                    dirs.append(ep)
        return dirs

    def frame_camera(self, vertices: np.ndarray) -> None:
        """Frame the camera on a bounding box of vertices."""
        if self.camera is not None:
            frame_on_bounds(self.camera, vertices)
