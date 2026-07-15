"""Settings manager for shared and per-variant ModBox21 UI state."""

from __future__ import annotations

import copy
import json
import logging
from pathlib import Path

from .app_paths import get_shared_settings_path, get_variant_settings_path

_log = logging.getLogger("toolkit.settings")

_AVAILABLE_ENGINES = ["claude_code", "opencode"]

_LABEL_TO_ENGINE = {
    "Claude Code": "claude_code",
    "OpenCode": "opencode",
}

_SHARED_DEFAULTS = {
    "active_game": "fo4",
    "setup_complete": False,
    "mod_prefix": "",
    "addon_node_index_start": 21000,
    "conversion_addon_node_index_start": 31000,
    "theme": "falloutnv",
}

_VARIANT_DEFAULTS = {
    "active_workspace": "nif",
    "window_width": 1600,
    "window_height": 900,
    "workspaces": {},
}

_GITEA_DEFAULTS = {
    "url": "",
    "username": "",
    "orgs": {},
    "token": "",
}

_PATHS_TEMPLATE = {
    "fo4": {
        "root_dir": "",
        "extracted_dir": "",
        "additional_paths": [],
        "scripts_user_dir": "",
        "scripts_base_dir": "",
        "installs": [],
    },
    "fo76": {
        "root_dir": "",
        "extracted_dir": "",
        "additional_paths": [],
        "scripts_user_dir": "",
        "scripts_base_dir": "",
    },
    "skyrimse": {
        "root_dir": "",
        "extracted_dir": "",
        "additional_paths": [],
        "scripts_user_dir": "",
        "scripts_base_dir": "",
    },
    "starfield": {
        "root_dir": "",
        "extracted_dir": "",
        "additional_paths": [],
        "content_resources_zip": "",
        "scripts_user_dir": "",
        "scripts_base_dir": "",
    },
    "fo3": {
        "root_dir": "",
        "extracted_dir": "",
        "additional_paths": [],
        "scripts_user_dir": "",
        "scripts_base_dir": "",
    },
    "fnv": {
        "root_dir": "",
        "extracted_dir": "",
        "additional_paths": [],
        "scripts_user_dir": "",
        "scripts_base_dir": "",
    },
    "script_source_paths": [],
}

_GAME_KEYS = ("fo4", "fo76", "skyrimse", "starfield", "fo3", "fnv")
_LEGACY_KEY_MAP = {"fallout4": "fo4", "fallout76": "fo76"}

_AI_ENGINES_DEFAULTS = {
    "available": list(_AVAILABLE_ENGINES),
    "enabled": ["claude_code"],
    "default": "claude_code",
}

_TOOLS_DEFAULTS: dict = {}

_INDEX_DEFAULTS = {
    "fo4_data": True,
    "scripts": True,
    "wiki": True,
    "nifs": True,
    "behaviors": True,
    "swf": True,
    "voice_reference": True,
}


class ToolkitSettings:
    """Load, merge, and persist shared plus per-variant UI settings."""

    def __init__(
        self,
        path: Path | str | None = None,
        editor_settings_path: Path | str | None = None,
        *,
        variant_id: str = "full",
        shared_path: Path | str | None = None,
        variant_path: Path | str | None = None,
    ):
        self.variant_id = variant_id
        if shared_path is None:
            shared_path = (
                Path(path).with_name("shared_settings.json")
                if path is not None
                else get_shared_settings_path()
            )
        if variant_path is None:
            variant_path = Path(path) if path is not None else get_variant_settings_path(variant_id)

        self._shared_path = Path(shared_path)
        self._variant_path = Path(variant_path)
        self._path = self._variant_path

        if editor_settings_path is None:
            editor_settings_path = (
                Path(__file__).parents[1] / "editor" / "editor_settings.json"
            )
        self._editor_settings_path = Path(editor_settings_path)

        self.active_workspace: str = _VARIANT_DEFAULTS["active_workspace"]
        self.active_game: str = _SHARED_DEFAULTS["active_game"]
        self.window_width: int = _VARIANT_DEFAULTS["window_width"]
        self.window_height: int = _VARIANT_DEFAULTS["window_height"]
        self.setup_complete: bool = _SHARED_DEFAULTS["setup_complete"]
        self.mod_prefix: str = _SHARED_DEFAULTS["mod_prefix"]
        self.addon_node_index_start: int = _SHARED_DEFAULTS["addon_node_index_start"]
        self.conversion_addon_node_index_start: int = _SHARED_DEFAULTS[
            "conversion_addon_node_index_start"
        ]
        self.ai_engines: dict = copy.deepcopy(_AI_ENGINES_DEFAULTS)
        self.tools: dict = copy.deepcopy(_TOOLS_DEFAULTS)
        self._paths: dict = copy.deepcopy(_PATHS_TEMPLATE)
        self._workspaces: dict[str, dict] = {}
        self._indexes: dict = copy.deepcopy(_INDEX_DEFAULTS)
        self._section_data: dict[str, dict] = {}
        self.theme: str = _SHARED_DEFAULTS["theme"]
        self.gitea: dict = copy.deepcopy(_GITEA_DEFAULTS)
        self._load()

    @property
    def shared_path(self) -> Path:
        return self._shared_path

    @property
    def variant_path(self) -> Path:
        return self._variant_path

    def _load(self) -> None:
        if self._shared_path.exists():
            self._load_shared_file(self._shared_path)
        elif self._variant_path.exists():
            self._load_shared_file(self._variant_path)
        else:
            self._migrate_paths()

        if self._variant_path.exists():
            self._load_variant_file(self._variant_path)

    def _load_shared_file(self, path: Path) -> None:
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except Exception as e:
            _log.warning("Failed to load shared settings from %s: %s", path, e)
            return

        self.active_game = data.get("active_game", self.active_game)
        self.setup_complete = data.get("setup_complete", self.setup_complete)
        self.mod_prefix = data.get("mod_prefix", self.mod_prefix)
        self.addon_node_index_start = data.get(
            "addon_node_index_start", self.addon_node_index_start
        )
        self.conversion_addon_node_index_start = data.get(
            "conversion_addon_node_index_start",
            self.conversion_addon_node_index_start,
        )
        self.theme = data.get("theme", self.theme)

        if "ai_engines" in data:
            stored = data["ai_engines"]
            self.ai_engines["enabled"] = stored.get("enabled", self.ai_engines["enabled"])
            self.ai_engines["default"] = stored.get("default", self.ai_engines["default"])
        else:
            label = data.get("ai_chat", {}).get("backend", "")
            if label in _LABEL_TO_ENGINE:
                self.ai_engines["default"] = _LABEL_TO_ENGINE[label]

        if "tools" in data:
            self.tools.update(data["tools"])

        self._load_paths(data.get("paths", {}))
        raw_indexes = data.get("indexes", {})
        self._indexes = {
            key: bool(raw_indexes.get(key, default))
            for key, default in _INDEX_DEFAULTS.items()
        }
        if "gitea" in data:
            self.gitea.update(data["gitea"])
        if "section_data" in data:
            self._section_data = {k: dict(v) for k, v in data["section_data"].items()}

    def _load_variant_file(self, path: Path) -> None:
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except Exception as e:
            _log.warning("Failed to load variant settings from %s: %s", path, e)
            return

        self.active_workspace = data.get("active_workspace", self.active_workspace)
        self.window_width = data.get("window_width", self.window_width)
        self.window_height = data.get("window_height", self.window_height)
        self._workspaces = data.get("workspaces", {})

    def _load_paths(self, raw_paths: dict) -> None:
        if not raw_paths:
            return
        raw_paths = copy.deepcopy(raw_paths)
        for old_key, new_key in _LEGACY_KEY_MAP.items():
            if old_key in raw_paths and new_key not in raw_paths:
                raw_paths[new_key] = raw_paths.pop(old_key)
            elif old_key in raw_paths:
                self._paths[new_key].update(raw_paths.pop(old_key))
        for game in _GAME_KEYS:
            if game in raw_paths:
                self._paths[game].update(raw_paths[game])
        if "script_source_paths" in raw_paths:
            self._paths["script_source_paths"] = list(raw_paths["script_source_paths"])

    def _migrate_paths(self) -> None:
        try:
            if self._editor_settings_path.exists():
                data = json.loads(
                    self._editor_settings_path.read_text(encoding="utf-8")
                )
                extra = data.get("extra_paths", [])
                if extra:
                    self._paths["fo4"]["additional_paths"] = list(extra)
        except Exception as e:
            _log.debug("Paths migration skipped: %s", e)

    @property
    def indexes(self) -> dict:
        return dict(self._indexes)

    @indexes.setter
    def indexes(self, value: dict) -> None:
        self._indexes = {
            key: bool(value.get(key, default))
            for key, default in _INDEX_DEFAULTS.items()
        }

    def save(self) -> None:
        shared = {
            "active_game": self.active_game,
            "setup_complete": self.setup_complete,
            "mod_prefix": self.mod_prefix,
            "addon_node_index_start": self.addon_node_index_start,
            "conversion_addon_node_index_start": self.conversion_addon_node_index_start,
            "ai_engines": {
                "available": list(_AVAILABLE_ENGINES),
                "enabled": self.ai_engines["enabled"],
                "default": self.ai_engines["default"],
            },
            "tools": self.tools,
            "paths": self._paths,
            "theme": self.theme,
            "indexes": self._indexes,
            "gitea": self.gitea,
            "section_data": self._section_data,
        }
        variant = {
            "variant_id": self.variant_id,
            "active_workspace": self.active_workspace,
            "window_width": self.window_width,
            "window_height": self.window_height,
            "workspaces": self._workspaces,
        }
        try:
            self._shared_path.parent.mkdir(parents=True, exist_ok=True)
            self._variant_path.parent.mkdir(parents=True, exist_ok=True)
            self._shared_path.write_text(
                json.dumps(shared, indent=2) + "\n", encoding="utf-8"
            )
            self._variant_path.write_text(
                json.dumps(variant, indent=2) + "\n", encoding="utf-8"
            )
        except Exception as e:
            _log.warning("Failed to save settings: %s", e)

    def get_game_paths(self, game_id: str) -> dict:
        return dict(self._paths.get(game_id, {}))

    def set_game_extracted_dir(self, game_id: str, path: str) -> None:
        if game_id not in self._paths:
            _log.warning("set_game_extracted_dir: unknown game_id %r", game_id)
            return
        self._paths[game_id]["extracted_dir"] = path
        self.save()

    def set_game_root_dir(self, game_id: str, path: str) -> None:
        if game_id not in self._paths:
            _log.warning("set_game_root_dir: unknown game_id %r", game_id)
            return
        self._paths[game_id]["root_dir"] = path
        self.save()

    def get_fo4_paths(self) -> dict:
        return self.get_game_paths("fo4")

    def get_fo76_paths(self) -> dict:
        return self.get_game_paths("fo76")

    def get_fo4_extra_installs(self) -> list[dict]:
        """Extra FO4 installs configured as deploy targets (excludes the primary)."""
        raw = self._paths.get("fo4", {}).get("installs", []) or []
        return [
            {"label": str(x.get("label", "")), "root_dir": str(x.get("root_dir", ""))}
            for x in raw
        ]

    def set_fo4_extra_installs(self, installs: list[dict]) -> None:
        """Persist the extra FO4 install list; drops empty roots, fills blank labels."""
        normalized: list[dict] = []
        for entry in installs or []:
            root = str(entry.get("root_dir", "")).strip()
            if not root:
                continue
            label = str(entry.get("label", "")).strip() or Path(root).name or "Fallout 4"
            normalized.append({"label": label, "root_dir": root})
        self._paths.setdefault("fo4", {})["installs"] = normalized
        self.save()

    def get_fo4_install_choices(self) -> list[dict]:
        """Deploy-target pick list: the primary install first, then extras."""
        primary_root = self._paths.get("fo4", {}).get("root_dir", "")
        choices = [
            {"label": "Primary install", "root_dir": primary_root, "primary": True}
        ]
        for extra in self.get_fo4_extra_installs():
            choices.append(
                {"label": extra["label"], "root_dir": extra["root_dir"], "primary": False}
            )
        return choices

    def get_active_game(self) -> str:
        return self.active_game

    def set_active_game(self, game_id: str) -> None:
        from creation_lib.core.game_profiles import GAME_PROFILES

        if game_id not in GAME_PROFILES:
            _log.warning("Unknown game_id %r; ignoring set_active_game", game_id)
            return
        self.active_game = game_id

    def get_script_source_paths(self) -> list[str]:
        return list(self._paths.get("script_source_paths", []))

    def get_tool_path(self, tool_id: str) -> str:
        return str(self.tools.get(tool_id, "") or "")

    def set_tool_path(self, tool_id: str, path: str) -> None:
        self.tools[tool_id] = str(path or "")

    def set_script_source_paths(self, paths: list[str]) -> None:
        self._paths["script_source_paths"] = list(paths)

    def get_script_source_dirs(self, game_id: str) -> dict[str, str]:
        game_paths = self.get_game_paths(game_id)
        return {
            "user": game_paths.get("scripts_user_dir", "") or "",
            "base": game_paths.get("scripts_base_dir", "") or "",
        }

    def get_scripts_source_dir(self, game_id: str) -> str | None:
        from creation_lib.core.game_profiles import GAME_PROFILES

        profile = GAME_PROFILES.get(game_id)
        if profile is None or profile.papyrus_compiler_dir is None:
            return None
        if not profile.papyrus_source_subpath:
            return None

        root_dir = self._paths.get(game_id, {}).get("root_dir", "")
        if not root_dir:
            return None

        return str(Path(root_dir) / profile.papyrus_source_subpath)

    def get_settings_section(self, section_id: str) -> dict:
        return dict(self._section_data.get(section_id, {}))

    def set_settings_section(self, section_id: str, value: dict) -> None:
        self._section_data[section_id] = dict(value)
        self.save()

    def get_workspace_settings(self, workspace_id: str) -> dict:
        return dict(self._workspaces.get(workspace_id, {}))

    def set_workspace_settings(self, workspace_id: str, settings: dict) -> None:
        existing = self._workspaces.get(workspace_id, {})
        existing.update(settings)
        self._workspaces[workspace_id] = existing

    def apply_defaults(self, workspace_id: str, defaults: dict) -> None:
        existing = self._workspaces.get(workspace_id, {})
        for key, val in defaults.items():
            if key not in existing:
                existing[key] = val
        self._workspaces[workspace_id] = existing
