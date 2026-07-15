from __future__ import annotations

from dataclasses import dataclass, field


try:
    from creation_lib.world_renderer import CellBounds, RenderSettings
except ImportError:

    @dataclass
    class CellBounds:
        min_x: int = -1
        min_y: int = -1
        max_x: int = 1
        max_y: int = 1

    @dataclass
    class RenderSettings:
        include_terrain: bool = True
        include_statics: bool = True
        include_static_collections: bool = True
        include_markers: bool = False
        include_water: bool = True
        include_lights: bool = True
        include_foliage: bool = True
        include_disabled_refs: bool = False


@dataclass
class WorldViewerState:
    game: str = "fo4"
    plugin_paths: list[str] = field(default_factory=list)
    data_paths: list[str] = field(default_factory=list)
    archive_paths: list[str] = field(default_factory=list)
    worldspace: str = ""
    bounds: CellBounds = field(default_factory=CellBounds)
    settings: RenderSettings = field(default_factory=RenderSettings)
    selected_instance_id: int | None = None
    selected_report: object | None = None
    last_report: object | None = None
    stats_report: object | None = None
    visible_report: object | None = None
    worldspaces: list[str] = field(default_factory=list)
    warnings: list[dict] = field(default_factory=list)
    error_message: str = ""
    render_output_path: str = "world_view.png"
    render_width: int = 1280
    render_height: int = 720
