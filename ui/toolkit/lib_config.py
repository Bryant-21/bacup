"""ToolkitSettings to lib config adapters."""

from __future__ import annotations

from pathlib import Path

from app.env_config import build_data_config, build_project_config, build_resource_config
from ui.toolkit.settings import ToolkitSettings
from creation_lib.core.app_context import GameContext, LibConfig
from creation_lib.core.game_profiles import GAME_PROFILES, get_profile


def _path(value: str | None) -> Path | None:
    value = str(value or "").strip()
    return Path(value) if value else None


def build_game_context_from_settings(settings: ToolkitSettings, game: str) -> GameContext:
    profile = get_profile(game)
    paths = settings.get_game_paths(game)
    root_dir = _path(paths.get("root_dir"))
    extracted_dir = _path(paths.get("extracted_dir"))
    data_dir = root_dir / "Data" if root_dir is not None else None

    strings_dirs: list[Path] = []
    if data_dir is not None:
        strings_dirs.append(data_dir / "Strings")
    if extracted_dir is not None:
        strings_dirs.extend((extracted_dir / "Strings", extracted_dir / "Data" / "Strings"))

    script_source_dirs: list[Path] = []
    for key in ("scripts_user_dir", "scripts_base_dir"):
        value = _path(paths.get(key))
        if value is not None:
            script_source_dirs.append(value)
    derived = settings.get_scripts_source_dir(game)
    if derived:
        script_source_dirs.append(Path(derived))
    content_resources_dir = _path(paths.get("content_resources_zip"))
    if game == "starfield" and content_resources_dir is not None:
        script_source_dirs.append(content_resources_dir)

    return GameContext(
        game=game,
        root_dir=root_dir,
        data_dir=data_dir,
        extracted_dir=extracted_dir,
        strings_dir=strings_dirs[0] if strings_dirs else None,
        strings_dirs=tuple(strings_dirs),
        script_source_dirs=tuple(script_source_dirs),
        content_resources_dir=content_resources_dir,
        addon_index_start=int(getattr(settings, "addon_node_index_start", 20000)),
    )


def build_lib_config_from_settings(settings: ToolkitSettings) -> LibConfig:
    return LibConfig(
        resources=build_resource_config(),
        data=build_data_config(),
        project=build_project_config(),
        games={
            game_id: build_game_context_from_settings(settings, game_id)
            for game_id in GAME_PROFILES
        },
    )
