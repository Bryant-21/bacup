"""Boundary helpers that convert app/.env settings into lib config objects."""

from __future__ import annotations

from collections.abc import Mapping
from pathlib import Path

from app.env_sync import ENV_KEY_MAP, parse_env_file
from app.paths import get_app_root, get_code_root, get_db_dir, get_logs_dir, get_resource_dir
from creation_lib.core.app_context import DataConfig, GameContext, LibConfig, ProjectConfig, ResourceConfig
from creation_lib.core.game_profiles import GAME_PROFILES, get_profile


def _path(value: str | None) -> Path | None:
    value = str(value or "").strip()
    return Path(value) if value else None


def _int(value: str | None, default: int) -> int:
    try:
        return int(str(value or "").strip())
    except ValueError:
        return default


def build_game_context_from_env(game: str, env: Mapping[str, str]) -> GameContext:
    profile = get_profile(game)
    key_map = ENV_KEY_MAP.get(game, {})
    root_dir = _path(env.get(key_map.get("root", f"{game.upper()}_DIR")))
    extracted_dir = _path(env.get(key_map.get("extracted", profile.env_var_name)))
    data_dir = root_dir / "Data" if root_dir is not None else None

    strings_dirs: list[Path] = []
    if data_dir is not None:
        strings_dirs.append(data_dir / "Strings")
    if extracted_dir is not None:
        strings_dirs.extend((extracted_dir / "Strings", extracted_dir / "Data" / "Strings"))

    script_source_dirs: list[Path] = []
    if data_dir is not None and profile.papyrus_source_subpath:
        script_source_dirs.append(root_dir / profile.papyrus_source_subpath)  # type: ignore[operator]
    content_resources_dir = _path(env.get("STARFIELD_CONTENT_RESOURCES_DIR"))
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
        fbx_sdk_dir=_path(env.get("FBXSDK_ROOT")),
        addon_index_start=_int(env.get("ADDON_NODE_INDEX_START"), 20000),
    )


def build_project_config(project_root: Path | None = None) -> ProjectConfig:
    root = project_root or get_app_root()
    return ProjectConfig(
        project_root=root,
        mods_dir=root / "mods",
        templates_dir=root / "templates",
        extracted_dir=root / "extracted",
        external_mods_dir=root / "external_mods",
        wiki_dir=root / "Wiki",
    )


def build_data_config(db_dir: Path | None = None) -> DataConfig:
    resolved_db = db_dir or get_db_dir()
    return DataConfig(
        db_dir=resolved_db,
        logs_dir=get_logs_dir(),
        cache_dir=resolved_db / "cache",
    )


def build_resource_config(code_root: Path | None = None) -> ResourceConfig:
    root = code_root or get_code_root()
    resource = get_resource_dir() if code_root is None else root / "resource"
    creation_resource = root / "py_creation_lib" / "python" / "creation_lib" / "resources"
    return ResourceConfig(
        nif_xml=root / "py_creation_lib/python/creation_lib" / "nif" / "nif_xml" / "nif.xml",
        grammar_lark=resource / "grammars" / "papyrus.lark",
        classxml_dir=creation_resource / "classxml",
        shader_dirs={
            "renderer": root / "py_creation_lib/python/creation_lib" / "renderer" / "shaders",
            "skinned": root / "py_creation_lib/python/creation_lib" / "nif" / "rendering" / "shaders",
            "simple": root / "py_creation_lib/python/creation_lib" / "renderer" / "shaders",
            "grid": root / "py_creation_lib/python/creation_lib" / "renderer" / "shaders",
            "shader_pipeline": root / "py_creation_lib/python/creation_lib" / "renderer" / "shaders",
        },
        hdri_dir=resource / "hdri",
        spellcheck_dict=root / "py_creation_lib/python/creation_lib" / "mod" / "spellcheck_dictionary.txt",
        havok_templates_dir=resource / "hkx_templates",
        novablast_bin=resource / "novablast.bin",
        convex_type_bin=resource / "convex_type.bin",
        conversion_yaml_dir=root / "py_creation_lib/python/creation_lib" / "conversion",
        semantic_overlay=root / "py_creation_lib/python/creation_lib" / "esp" / "schema" / "data",
        fbx_sdk_dir=resource / "fbx",
    )


def build_lib_config_from_env(
    env: Mapping[str, str] | None = None,
    *,
    project_root: Path | None = None,
    db_dir: Path | None = None,
) -> LibConfig:
    values = dict(env) if env is not None else parse_env_file()
    games = {
        game_id: build_game_context_from_env(game_id, values)
        for game_id in GAME_PROFILES
    }
    return LibConfig(
        resources=build_resource_config(),
        data=build_data_config(db_dir),
        project=build_project_config(project_root),
        games=games,
    )
