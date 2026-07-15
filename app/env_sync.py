"""Shared .env import/export helpers for setup flows."""

from __future__ import annotations

from pathlib import Path

from app.paths import get_app_root
from creation_lib.core.game_profiles import GAME_PROFILES


ENV_KEY_MAP: dict[str, dict[str, str]] = {
    "fo4": {"root": "FO4_DIR", "extracted": "FO4_EXTRACTED_DIR"},
    "skyrimse": {"root": "SKYRIMSE_DIR", "extracted": "SKYRIMSE_EXTRACTED_DIR"},
    "starfield": {"root": "STARFIELD_DIR", "extracted": "STARFIELD_EXTRACTED_DIR"},
    "fo76": {"root": "FO76_DIR", "extracted": "FO76_EXTRACTED_DIR"},
    "fo3": {"root": "FO3_DIR", "extracted": "FO3_EXTRACTED_DIR"},
    "fnv": {"root": "FONV_DIR", "extracted": "FONV_EXTRACTED_DIR"},
}


def default_env_path() -> Path:
    return get_app_root() / ".env"


def parse_env_file(path: Path | str | None = None) -> dict[str, str]:
    env_path = Path(path) if path is not None else default_env_path()
    if not env_path.is_file():
        return {}

    result: dict[str, str] = {}
    for line in env_path.read_text(encoding="utf-8").splitlines():
        stripped = line.strip()
        if not stripped or stripped.startswith("#") or "=" not in stripped:
            continue
        key, value = stripped.split("=", 1)
        value = value.strip()
        if (value.startswith('"') and value.endswith('"')) or (
            value.startswith("'") and value.endswith("'")
        ):
            value = value[1:-1]
        result[key.strip()] = value
    return result


def update_env_file(updates: dict[str, str], path: Path | str | None = None) -> Path:
    env_path = Path(path) if path is not None else default_env_path()
    env_path.parent.mkdir(parents=True, exist_ok=True)

    lines = env_path.read_text(encoding="utf-8").splitlines() if env_path.exists() else []
    written: set[str] = set()
    output: list[str] = []

    for line in lines:
        stripped = line.strip()
        if stripped and not stripped.startswith("#") and "=" in stripped:
            key = stripped.split("=", 1)[0].strip()
            if key in updates:
                output.append(_format_env_line(key, updates[key]))
                written.add(key)
                continue
        output.append(line)

    for key, value in updates.items():
        if key not in written:
            output.append(_format_env_line(key, value))

    env_path.write_text("\n".join(output).rstrip() + "\n", encoding="utf-8")
    return env_path


def settings_to_env_updates(settings) -> dict[str, str]:
    updates: dict[str, str] = {}
    for game_id, field_map in ENV_KEY_MAP.items():
        paths = settings.get_game_paths(game_id)
        updates[field_map["root"]] = str(paths.get("root_dir", "") or "")
        updates[field_map["extracted"]] = str(paths.get("extracted_dir", "") or "")

    updates["DEFAULT_GAME"] = settings.get_active_game()
    updates["MOD_PREFIX"] = str(getattr(settings, "mod_prefix", "") or "")
    updates["ADDON_NODE_INDEX_START"] = str(
        getattr(settings, "addon_node_index_start", 21000)
    )
    updates["CONVERSION_ADDON_NODE_INDEX_START"] = str(
        getattr(settings, "conversion_addon_node_index_start", 31000)
    )

    gitea = getattr(settings, "gitea", {}) or {}
    updates["GITEA_URL"] = str(gitea.get("url", "") or "")
    updates["GITEA_USER"] = str(gitea.get("username", "") or "")
    updates["GITEA_TOKEN"] = str(gitea.get("token", "") or "")
    orgs = gitea.get("orgs", {}) or {}
    for game_id in GAME_PROFILES:
        updates[f"GITEA_ORG_{game_id.upper()}"] = str(orgs.get(game_id, "") or "")
    return updates


def export_settings_to_env(settings, path: Path | str | None = None) -> Path:
    return update_env_file(settings_to_env_updates(settings), path)


def _format_env_line(key: str, value: str) -> str:
    escaped = str(value).replace('"', '\\"')
    return f'{key}="{escaped}"'
