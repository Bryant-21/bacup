"""One-time local conversion from legacy toolkit_settings.json."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
import json
import shutil
from pathlib import Path

from .variants import FIRST_STANDALONE_VARIANT_IDS


@dataclass(frozen=True)
class MigrationResult:
    backup_path: Path
    shared_path: Path
    variant_paths: dict[str, Path]


_SHARED_KEYS = (
    "active_game",
    "setup_complete",
    "mod_prefix",
    "addon_node_index_start",
    "conversion_addon_node_index_start",
    "ai_engines",
    "tools",
    "paths",
    "theme",
    "indexes",
    "gitea",
)


def _write_json(path: Path, data: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2) + "\n", encoding="utf-8")


def _variant_payload(legacy: dict, variant_id: str, workspace_ids: tuple[str, ...] | None) -> dict:
    legacy_workspaces = legacy.get("workspaces", {}) or {}
    if workspace_ids is None:
        workspaces = dict(legacy_workspaces)
    else:
        workspaces = {
            workspace_id: legacy_workspaces[workspace_id]
            for workspace_id in workspace_ids
            if workspace_id in legacy_workspaces
        }

    active_workspace = legacy.get("active_workspace", "")
    if workspace_ids is not None and active_workspace not in workspace_ids:
        active_workspace = workspace_ids[0]
    if not active_workspace:
        active_workspace = "nif"

    return {
        "variant_id": variant_id,
        "active_workspace": active_workspace,
        "window_width": legacy.get("window_width", 1600),
        "window_height": legacy.get("window_height", 900),
        "workspaces": workspaces,
    }


def migrate_legacy_settings(
    legacy_path: Path | str,
    *,
    output_dir: Path | str | None = None,
    timestamp: str | None = None,
) -> MigrationResult:
    """Convert one legacy monolithic settings file into the new settings layout."""
    legacy_path = Path(legacy_path)
    if output_dir is None:
        output_dir = legacy_path.parent / "settings_data"
    output_dir = Path(output_dir)
    if timestamp is None:
        timestamp = datetime.now().strftime("%Y%m%d-%H%M%S")

    legacy = json.loads(legacy_path.read_text(encoding="utf-8"))

    backup_path = legacy_path.with_name(f"{legacy_path.stem}.{timestamp}{legacy_path.suffix}.bak")
    shutil.copy2(legacy_path, backup_path)

    shared = {key: legacy[key] for key in _SHARED_KEYS if key in legacy}
    shared_path = output_dir / "shared_settings.json"
    _write_json(shared_path, shared)

    variant_paths: dict[str, Path] = {}
    variants_dir = output_dir / "variants"
    full_path = variants_dir / "full.json"
    _write_json(full_path, _variant_payload(legacy, "full", None))
    variant_paths["full"] = full_path

    for variant_id in FIRST_STANDALONE_VARIANT_IDS:
        path = variants_dir / f"{variant_id}.json"
        _write_json(path, _variant_payload(legacy, variant_id, (variant_id,)))
        variant_paths[variant_id] = path

    return MigrationResult(
        backup_path=backup_path,
        shared_path=shared_path,
        variant_paths=variant_paths,
    )
